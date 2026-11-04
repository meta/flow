/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Checks that a change only touches types. Both sides of every changed JavaScript file are
//! compiled to runnable JavaScript, as Babel would, and the compiled programs are compared with
//! locations and comments stripped.

use std::collections::HashMap;
use std::io;
use std::num::NonZeroUsize;
use std::path::Path;
use std::path::PathBuf;

use flow_common_vcs::git;
use flow_common_vcs::hg;
use flow_common_vcs::vcs;
use flow_common_vcs::vcs::FileChange;
use flow_common_vcs::vcs::Vcs;
use flow_common_vcs::vcs_utils::ErrorStatus;
use flow_parser::ast;
use flow_parser::comment_utils;
use flow_parser::loc::Loc;
use flow_parser::polymorphic_ast_mapper;
use flow_parser::polymorphic_ast_mapper::LocMapper;
use flow_parser_utils_output::indented_printer;
use flow_parser_wasm::runnable_js;
use flow_parser_wasm::runnable_js::RunnableError;
use flow_parser_wasm::runnable_js::RunnableOptions;
use futures::StreamExt;
use similar::TextDiff;

const VCS_CONCURRENCY: usize = 16;

const CAT_CHUNK_SIZE: usize = 200;

const JS_EXTENSIONS: &[&str] = &["js", "jsx", "mjs", "cjs"];

/// The revision that the checked state is compared against.
pub enum Base {
    /// The parent of the checked state: the current commit for the working copy, or the
    /// checked revision's parent.
    Parent,
    Revision(String),
    /// The merge base of the checked state and the given revision.
    MergeBaseWith(String),
}

pub struct Args {
    /// A directory inside the repository.
    pub dir: PathBuf,
    pub base: Base,
    /// The revision to check. The working copy is checked when absent.
    pub rev: Option<String>,
    pub show_diff: bool,
}

enum Outcome {
    TypeOnly,
    RuntimeChanged { diff: Option<String> },
    Failed(String),
}

struct FileResult {
    path: String,
    outcome: Outcome,
}

struct Repo {
    vcs: Vcs,
    root: String,
}

fn vcs_error(error: ErrorStatus) -> io::Error {
    match error {
        ErrorStatus::NotInstalled { path } => {
            io::Error::other(format!("Version control binary not found in PATH {path:?}"))
        }
        ErrorStatus::Errored(message) => io::Error::other(message),
    }
}

impl Repo {
    fn find(dir: &Path) -> io::Result<Self> {
        let Some((vcs, root)) = vcs::find_root(None, dir) else {
            return Err(io::Error::other(format!(
                "{} is not inside a Mercurial or Git repository",
                dir.display()
            )));
        };
        let Some(root) = root.to_str() else {
            return Err(io::Error::other(format!(
                "Repository root {} is not valid UTF-8",
                root.display()
            )));
        };
        Ok(Self {
            vcs,
            root: root.to_owned(),
        })
    }

    fn current_commit(&self) -> &'static str {
        match self.vcs {
            Vcs::Hg => ".",
            Vcs::Git => "HEAD",
        }
    }

    async fn resolve_base(&self, base: &Base, rev: Option<&str>) -> io::Result<String> {
        let checked = rev.unwrap_or(self.current_commit());
        match (base, rev) {
            (Base::Parent, None) => Ok(checked.to_owned()),
            (Base::Parent, Some(rev)) => Ok(match self.vcs {
                Vcs::Hg => format!("p1({rev})"),
                Vcs::Git => format!("{rev}^"),
            }),
            (Base::Revision(base), _) => Ok(base.clone()),
            (Base::MergeBaseWith(other), _) => match self.vcs {
                Vcs::Hg => hg::merge_base(Some(&self.root), checked, other).await,
                Vcs::Git => git::merge_base(Some(&self.root), checked, other).await,
            }
            .map_err(vcs_error),
        }
    }

    async fn files_changed_between(
        &self,
        base: &str,
        rev: Option<&str>,
    ) -> io::Result<Vec<(FileChange, String)>> {
        match self.vcs {
            Vcs::Hg => hg::files_changed_between(Some(&self.root), base, rev).await,
            Vcs::Git => git::files_changed_between(Some(&self.root), base, rev).await,
        }
        .map_err(vcs_error)
    }

    async fn cat_files(&self, rev: &str, paths: &[&str]) -> io::Result<HashMap<String, String>> {
        futures::stream::iter(paths.chunks(CAT_CHUNK_SIZE))
            .map(|chunk| async move {
                match self.vcs {
                    Vcs::Hg => hg::cat_files(Some(&self.root), rev, chunk).await,
                    Vcs::Git => git::cat_files(Some(&self.root), rev, chunk).await,
                }
                .map_err(vcs_error)
            })
            .buffer_unordered(VCS_CONCURRENCY)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .try_fold(HashMap::new(), |mut files, chunk| {
                files.extend(chunk?);
                Ok(files)
            })
    }

    async fn read_working_copy(&self, paths: &[&str]) -> io::Result<HashMap<String, String>> {
        futures::stream::iter(paths)
            .map(|path| async move {
                let bytes = tokio::fs::read(Path::new(&self.root).join(path)).await?;
                Ok((
                    path.to_string(),
                    String::from_utf8_lossy(&bytes).into_owned(),
                ))
            })
            .buffer_unordered(VCS_CONCURRENCY)
            .collect::<Vec<io::Result<_>>>()
            .await
            .into_iter()
            .collect()
    }

    /// The contents of `changes` before and after, with absent files as empty sources.
    async fn contents(
        &self,
        base: &str,
        rev: Option<&str>,
        changes: Vec<(FileChange, String)>,
    ) -> io::Result<Vec<(String, String, String)>> {
        let paths_except = |excluded: FileChange| -> Vec<&str> {
            changes
                .iter()
                .filter(|(change, _)| *change != excluded)
                .map(|(_, path)| path.as_str())
                .collect()
        };
        let before_paths = paths_except(FileChange::Added);
        let after_paths = paths_except(FileChange::Removed);
        let (mut before, mut after) =
            futures::try_join!(self.cat_files(base, &before_paths), async {
                match rev {
                    Some(rev) => self.cat_files(rev, &after_paths).await,
                    None => self.read_working_copy(&after_paths).await,
                }
            })?;
        let take = |files: &mut HashMap<String, String>, path: &str| {
            files
                .remove(path)
                .ok_or_else(|| io::Error::other(format!("Missing contents of {path}")))
        };
        changes
            .into_iter()
            .map(|(change, path)| {
                let before = match change {
                    FileChange::Added => String::new(),
                    FileChange::Modified | FileChange::Removed => take(&mut before, &path)?,
                };
                let after = match change {
                    FileChange::Removed => String::new(),
                    FileChange::Added | FileChange::Modified => take(&mut after, &path)?,
                };
                Ok((path, before, after))
            })
            .collect()
    }
}

fn is_js(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| JS_EXTENSIONS.contains(&extension))
}

/// The compiled runtime program of `source`, without comments.
fn runtime_program(path: &str, source: &str) -> Result<ast::Program<Loc, Loc>, String> {
    let options = RunnableOptions {
        source_type: if path.ends_with(".mjs") {
            runnable_js::SourceType::Module
        } else {
            runnable_js::SourceType::CommonJs
        },
        ..RunnableOptions::default()
    };
    match runnable_js::flow_to_runnable_program(source, path, &options) {
        Ok(mut program) => {
            comment_utils::strip_all_comments(&mut program, false);
            Ok(program)
        }
        Err(RunnableError::Syntax { message, loc }) => {
            Err(format!("{}:{}: {}", loc.line, loc.column + 1, message))
        }
        Err(error) => Err(error.to_string()),
    }
}

struct LocStripper;

impl LocMapper<Loc, Loc, (), ()> for LocStripper {
    fn on_loc_annot(&mut self, _loc: &Loc) -> Result<(), !> {
        Ok(())
    }

    fn on_type_annot(&mut self, _annot: &Loc) -> Result<(), !> {
        Ok(())
    }
}

fn strip_locs(program: &ast::Program<Loc, Loc>) -> ast::Program<(), ()> {
    let Ok(program) = polymorphic_ast_mapper::program(&mut LocStripper, program);
    program
}

fn print(program: &ast::Program<Loc, Loc>) -> String {
    match indented_printer::print(program) {
        Ok(source) => source.contents(),
        Err(error) => format!("<failed to print: {error}>\n"),
    }
}

/// Compares the runtime programs of `before` and `after`. An absent file is an empty source.
fn compare(path: &str, before: &str, after: &str, show_diff: bool) -> Outcome {
    let before = match runtime_program(path, before) {
        Ok(program) => program,
        Err(message) => return Outcome::Failed(format!("before the change: {message}")),
    };
    let after = match runtime_program(path, after) {
        Ok(program) => program,
        Err(message) => return Outcome::Failed(format!("after the change: {message}")),
    };
    if strip_locs(&before) == strip_locs(&after) {
        return Outcome::TypeOnly;
    }
    let diff = show_diff.then(|| {
        TextDiff::from_lines(print(&before), print(&after))
            .unified_diff()
            .header(&format!("a/{path}"), &format!("b/{path}"))
            .to_string()
    });
    Outcome::RuntimeChanged { diff }
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// Prints the results and returns whether every checked file only changed types.
fn report(results: &[FileResult], skipped: usize) -> bool {
    if skipped > 0 {
        println!("Skipped {}.", plural(skipped, "non-JavaScript file"));
    }
    let changed: Vec<_> = results
        .iter()
        .filter_map(|result| match &result.outcome {
            Outcome::RuntimeChanged { diff } => Some((&result.path, diff)),
            Outcome::TypeOnly | Outcome::Failed(_) => None,
        })
        .collect();
    if !changed.is_empty() {
        println!("Runtime code changed in {}:", plural(changed.len(), "file"));
        changed.iter().for_each(|(path, diff)| match diff {
            Some(diff) => println!("  {path}\n{diff}"),
            None => println!("  {path}"),
        });
    }
    let failed: Vec<_> = results
        .iter()
        .filter_map(|result| match &result.outcome {
            Outcome::Failed(message) => Some((&result.path, message)),
            Outcome::TypeOnly | Outcome::RuntimeChanged { .. } => None,
        })
        .collect();
    if !failed.is_empty() {
        println!("Failed to compile {}:", plural(failed.len(), "file"));
        failed
            .iter()
            .for_each(|(path, message)| println!("  {path}: {message}"));
    }
    let type_only = changed.is_empty() && failed.is_empty();
    if results.is_empty() {
        println!("No JavaScript files changed.");
    } else if type_only {
        println!(
            "All changes to {} are type-only.",
            plural(results.len(), "JavaScript file")
        );
    }
    type_only
}

async fn check(args: Args) -> io::Result<bool> {
    let repo = Repo::find(&args.dir)?;
    let rev = args.rev.as_deref();
    let base = repo.resolve_base(&args.base, rev).await?;
    let (js, skipped): (Vec<_>, Vec<_>) = repo
        .files_changed_between(&base, rev)
        .await?
        .into_iter()
        .partition(|(_, path)| is_js(path));
    println!(
        "Comparing {} against {} ({}).",
        rev.unwrap_or("the working copy"),
        base,
        plural(js.len() + skipped.len(), "changed file"),
    );
    let show_diff = args.show_diff;
    let parallelism = std::thread::available_parallelism().map_or(1, NonZeroUsize::get);
    let mut results = futures::stream::iter(repo.contents(&base, rev, js).await?)
        .map(|(path, before, after)| {
            flow_tokio_runtime::spawn_blocking(move || {
                let outcome = compare(&path, &before, &after, show_diff);
                FileResult { path, outcome }
            })
        })
        .buffer_unordered(parallelism)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(io::Error::other)?;
    results.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(report(&results, skipped.len()))
}

/// Returns whether every changed JavaScript file only changed types.
pub fn run(args: Args) -> io::Result<bool> {
    flow_tokio_runtime::block_on(check(args))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_type_only(before: &str, after: &str) -> bool {
        match compare("test.js", before, after, false) {
            Outcome::TypeOnly => true,
            Outcome::RuntimeChanged { .. } => false,
            Outcome::Failed(message) => panic!("failed to compile: {message}"),
        }
    }

    #[test]
    fn ignores_types_comments_and_formatting() {
        assert!(is_type_only(
            "// @flow\nfunction f(x) { return x + 1; }\nconst y = f(1);",
            "/**\n * @flow\n */\ntype T = number;\nfunction f(x: T): T {\n  // add one\n  return x + 1;\n}\n\nconst y: T = f((1: any));",
        ));
        assert!(is_type_only(
            "import {a, type B} from 'm';\nexport const c = a;",
            "import {a, type B, type C} from 'm';\nexport type {C};\nexport const c = (a as B);",
        ));
        assert!(is_type_only(
            "class A { m() {} }",
            "class A<T> implements I { declare x: T; m(): void {} }",
        ));
    }

    #[test]
    fn a_file_with_only_types_has_no_runtime_code() {
        assert!(is_type_only("", "export type T = number;"));
        assert!(is_type_only("declare var x: number;", ""));
    }

    #[test]
    fn detects_runtime_changes() {
        assert!(
            !is_type_only("const x = f();", "const x = f() ?? 0;"),
            "an added fallback changes runtime code"
        );
        assert!(
            !is_type_only("const x = 'a';", "const x = 'b';"),
            "a changed literal changes runtime code"
        );
        assert!(
            !is_type_only(
                "import {a, B} from 'm';\nexport const c = a;",
                "import {a, type B} from 'm';\nexport const c = a;",
            ),
            "changing a value import to a type import removes a runtime binding"
        );
        assert!(
            !is_type_only("import {B} from 'm';", "import {type B} from 'm';"),
            "removing the last value import drops the module's side effects"
        );
        assert!(
            !is_type_only("", "export const x: number = 1;"),
            "an added file with runtime code is a runtime change"
        );
    }

    #[test]
    fn lowers_flow_runtime_syntax_before_comparing() {
        assert!(is_type_only(
            "component Foo(bar: string) { return bar; }",
            "component Foo(bar: ?string) renders* Bar { return bar; }",
        ));
        assert!(
            !is_type_only(
                "component Foo(bar: string) { return bar; }",
                "component Foo(bar: string = 'x') { return bar; }",
            ),
            "a default prop value is runtime code"
        );
        assert!(
            !is_type_only("enum E {A, B}", "enum E {A, C}"),
            "enum members are runtime values"
        );
    }

    #[test]
    fn reports_compile_errors() {
        assert!(matches!(
            compare("test.js", "const x = 1;", "const x = ;", false),
            Outcome::Failed(message) if message.starts_with("after the change: 1:")
        ));
    }

    #[test]
    fn shows_the_runtime_diff() {
        let Outcome::RuntimeChanged { diff: Some(diff) } = compare(
            "test.js",
            "const x: number = 1;",
            "const x: number = 2;",
            true,
        ) else {
            panic!("expected a runtime change with a diff");
        };
        assert!(diff.contains("-const x = 1;"), "{diff}");
        assert!(diff.contains("+const x = 2;"), "{diff}");
    }

    #[test]
    fn checks_javascript_extensions_only() {
        assert!(is_js("a/b.js"));
        assert!(is_js("a/b.mjs"));
        assert!(!is_js("a/b.js.flow"));
        assert!(!is_js("a/BUCK"));
    }
}
