/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocTable;
use flow_aloc::LazyALocTable;
use flow_common::options::Options;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_lint_settings::lint_settings::LintSettings;
use flow_lint_settings::severity::Severity;
use flow_parser::ast;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::FileKeyInner;
use flow_parser::loc::LOC_NONE;
use flow_parser::loc::Loc;
use flow_parser::loc_sig::LocSig;
use flow_parser::polymorphic_ast_mapper;
use flow_parser::polymorphic_ast_mapper::LocMapper;
use flow_parser_utils::file_sig::FileSig;
use flow_parser_utils_output::js_layout_generator;
use flow_parser_utils_output::pretty_printer;
use flow_type_sig::type_sig_options::TypeSigOptions;
use flow_typing_builtins::builtins::Builtins;
use flow_typing_context::Context;
use flow_typing_context::ContextArena;
use flow_typing_context::ContextStore;
use flow_typing_context::Cx;
use flow_typing_context::FrozenMetadata;
use flow_typing_context::Metadata;
use flow_typing_context::MkBuiltins;
use flow_typing_context::OverridableMetadata;
use flow_typing_context::ResolveRequire;
use flow_typing_context::ResolvedRequire;
use flow_typing_type::type_::AnyErrorKind;
use flow_typing_type::type_::AnySource;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;

use crate::type_inference;

// pretty much copied from Flow_dot_js
fn metadata() -> Metadata {
    let frozen = FrozenMetadata {
        hook_compatibility: true,
        ts_syntax: true,
        ts_utility_syntax: true,
        tslib_syntax: true,
        ..Default::default()
    };
    Metadata {
        frozen: Rc::new(frozen),
        overridable: OverridableMetadata::default(),
    }
}

// somewhat copied from Flow_dot_js
fn parse_content(file: FileKey, content: &str) -> ast::Program<Loc, Loc> {
    let parse_options = Some(flow_parser::PERMISSIVE_PARSE_OPTIONS);
    let (ast, _parse_errors) =
        flow_parser::parse_program_file::<()>(false, None, parse_options, file, Ok(content));
    ast
}

fn check_before_and_after_stmts(relative_path: &str, file_name: &str) {
    let content = String::from_utf8_lossy(&std::fs::read(file_name).unwrap()).into_owned();
    let file_key = FileKey::new(FileKeyInner::SourceFile(file_name.to_string()));
    let ast = parse_content(file_key.dupe(), &content);
    // Loading the entire libdefs here would be overkill, but the typed_ast tests do use Object
    // in a few tests. In order to avoid EBuiltinLookupFailed errors with an empty source location,
    // we manually add "Object" -> Any into the builtins map. We use the UnresolvedName any type
    // to avoid any "Any value used as type" errors that may otherwise appear
    let mk_builtins: MkBuiltins = Rc::new(|_cx: &mut Context<'_>| {
        let reason = flow_common::reason::mk_reason(None, ALoc::none());
        let t = Type::new(TypeInner::AnyT(
            reason.dupe(),
            AnySource::AnyError(Some(AnyErrorKind::UnresolvedName)),
        ));
        let mut values: FlowOrdMap<FlowSmolStr, flow_typing_builtins::LazyVal<Cx>> =
            FlowOrdMap::new();
        values.insert(
            FlowSmolStr::from("Object"),
            Rc::new(flow_lazy::Lazy::new(move |_cx: &mut Context<'_>| {
                (ALoc::none(), t.dupe())
            })),
        );
        Builtins::of_name_map(values, FlowOrdMap::new(), FlowOrdMap::new())
    });
    let aloc_table: LazyALocTable = Rc::new(std::cell::LazyCell::new(Box::new({
        let fk = file_key.dupe();
        move || Rc::new(ALocTable::empty(fk))
    })
        as Box<dyn FnOnce() -> Rc<ALocTable>>));
    let resolve_require: ResolveRequire = Rc::new(
        |_: &mut Context<'_>, _: &flow_common::flow_import_specifier::FlowImportSpecifier| {
            ResolvedRequire::MissingModule
        },
    );
    let md = metadata();
    let mut arena = ContextArena::new();
    let ccx = arena.add_component();
    let cx_id = arena.add_context(
        ccx,
        md.clone(),
        file_key.dupe(),
        Arc::default(),
        aloc_table,
        resolve_require,
        mk_builtins,
        flow_utils_concurrency::check_budget::CheckBudget::new(None),
    );
    let mut cx = Context::new(ContextStore::new(vec![&mut arena]), cx_id);
    let aloc_ast = flow_aloc::loc_to_aloc_ast(&ast);
    let type_sig_options = TypeSigOptions::of_options(
        &Options {
            hook_compatibility: true,
            ts_syntax: true,
            ts_utility_syntax: true,
            tslib_syntax: true,
            ..Default::default()
        },
        false,
        Vec::new(),
        &file_key,
        false,
    );
    let typed_ast = type_inference::infer_ast(
        &LintSettings::<Severity>::empty_severities(),
        &mut cx,
        &file_key,
        Arc::new(FileSig::empty()),
        &md,
        &[],
        &ast,
        aloc_ast,
        &type_sig_options,
        None,
    )
    .expect("infer_ast should not be canceled in test");
    let aloc_stmts = flow_aloc::loc_to_aloc_statement_list(&ast.statements);
    check_structural_equality(relative_path, file_name, aloc_stmts, &typed_ast.statements);
}

struct LocNoneMapper;

impl<M: Dupe, T: Dupe> LocMapper<M, T, Loc, Loc> for LocNoneMapper {
    fn on_loc_annot(&mut self, _x: &M) -> Result<Loc, !> {
        Ok(LOC_NONE)
    }

    fn on_type_annot(&mut self, _x: &T) -> Result<Loc, !> {
        Ok(LOC_NONE)
    }
}

struct ALocMapper;

impl LocMapper<ALoc, (ALoc, Type), ALoc, ALoc> for ALocMapper {
    fn on_loc_annot(&mut self, x: &ALoc) -> Result<ALoc, !> {
        Ok(x.dupe())
    }

    fn on_type_annot(&mut self, annot: &(ALoc, Type)) -> Result<ALoc, !> {
        Ok(annot.0.dupe())
    }
}

fn diff_dir() -> String {
    let extension = format!(
        "typed_ast_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    );
    let temp_dir = flow_server_files::server_files_js::default_temp_dir();
    temp_dir.join(extension).to_string_lossy().into_owned()
}

fn system_diff(
    f: &dyn Fn(&[ast::statement::Statement<ALoc, ALoc>]) -> String,
    prefix: &str,
    stmts1: &[ast::statement::Statement<ALoc, ALoc>],
    stmts2: &[ast::statement::Statement<ALoc, ALoc>],
) -> String {
    let dir = diff_dir();
    let dump_stmts = |filename: &str, stmts: &[ast::statement::Statement<ALoc, ALoc>]| -> String {
        let stmts_str = f(stmts);
        let stmts_file = format!("{}/{}", dir, filename);
        let mut oc = std::fs::File::create(&stmts_file).unwrap();
        oc.write_all(stmts_str.as_bytes()).unwrap();
        stmts_file
    };
    let result = (|| -> Result<String, String> {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let stmts1_file = dump_stmts(&format!("{}_A.js", prefix), stmts1);
        let stmts2_file = dump_stmts(&format!("{}_B.js", prefix), stmts2);
        let out_file = format!("{}/{}_diff.txt", dir, prefix);
        let cmd = format!("diff -U7 {} {} > {}", stmts1_file, stmts2_file, out_file);
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .status()
            .map_err(|e| e.to_string())?;
        match status.code() {
            Some(0) | Some(1) => {
                let s = std::fs::read_to_string(&out_file).map_err(|e| e.to_string())?;
                eprintln!("READ: {}", s);
                Ok(s)
            }
            Some(code) => {
                eprintln!("diff read error code {}", code);
                Err("diff wasn't able to run for some reason".to_string())
            }
            None => Err("diff process terminated by signal".to_string()),
        }
    })();
    // Cleanup is best-effort; ignore errors (e.g. dir already removed)
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {}
        Err(_) => {}
    }
    match result {
        Ok(diff) => diff,
        Err(msg) => panic!("{}", msg),
    }
}

fn pp_diff(
    stmts1: &[ast::statement::Statement<ALoc, ALoc>],
    stmts2: &[ast::statement::Statement<ALoc, ALoc>],
) -> String {
    let string_of_ast = |stmts: &[ast::statement::Statement<ALoc, ALoc>]| -> String {
        stmts
            .iter()
            .map(|s| format!("{:?}", s))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let string_of_src = |stmts: &[ast::statement::Statement<ALoc, ALoc>]| -> String {
        let Ok(mapped_stmts): Result<Vec<_>, !> = stmts
            .iter()
            .map(|s| polymorphic_ast_mapper::statement(&mut LocNoneMapper, s))
            .collect();
        let prog = ast::Program {
            loc: LOC_NONE,
            statements: mapped_stmts.into(),
            interpreter: None,
            comments: None,
            all_comments: Arc::from([]),
        };
        let opts = js_layout_generator::default_opts();
        let layout = js_layout_generator::program(&opts, false, None, &prog);
        let source = pretty_printer::print(false, &layout);
        source.contents()
    };
    let ast_diff = system_diff(&string_of_ast, "ast", stmts1, stmts2);
    let src_diff = system_diff(&string_of_src, "src", stmts1, stmts2);
    format!(
        "\nAST tree diff:\n{}\n\nSource diff:\n{}",
        ast_diff, src_diff
    )
}

fn check_structural_equality(
    relative_path: &str,
    file_name: &str,
    stmts1: &[ast::statement::Statement<ALoc, ALoc>],
    stmts2: &[ast::statement::Statement<ALoc, (ALoc, Type)>],
) {
    let Ok(stmts2_mapped) =
        polymorphic_ast_mapper::toplevel_statement_list(&mut ALocMapper, stmts2);
    let path = std::fs::canonicalize(file_name)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| relative_path.to_string());
    let msg = format!(
        "{}:\n\
         The structure of the produced Typed AST differs from that of the parsed AST.\n\n\
         To fix this do one of the following:\n \
         * restore the produced Typed AST, or\n \
         * include \"{}\" in the blocklist section\n   \
         in src/typing/__tests__/typed_ast_test.ml and file a task with the\n   \
         'flow-typed-ast' tag.\n",
        path, relative_path
    );
    if stmts1 != stmts2_mapped {
        let diff = pp_diff(stmts1, &stmts2_mapped);
        panic!("{}\n{}", msg, diff);
    }
}

fn test_case(relative_path: &str, file_name: &str) {
    check_before_and_after_stmts(relative_path, file_name);
}

// This list includes files for which the produced Typed AST differs in structure
// from the parsed AST, and files that only check within their own `recursion_limit`.
fn blocklist() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "invariant_reachability/index.js",
        "return/implicit_void.js",
        "sealed_tvars/abnormal.js",
        "abnormal/return-throw.js",
        "destructuring_recursion_limit/test.js",
    ])
}

fn tests(root: &str) -> Vec<(String, String, String)> {
    let filter: Box<dyn Fn(&str) -> bool> = Box::new(|path: &str| path.ends_with(".js"));
    let mut next_files = flow_utils_find::make_next_files(filter, vec![], root.to_string());
    let files = flow_common::files::get_all(&mut next_files);
    let bl = blocklist();
    let mut result = Vec::new();
    for file in &files {
        let relative_path = flow_common::files::relative_path(Path::new(root), file);
        if bl.contains(relative_path.as_str()) {
            continue;
        }
        let test_name = relative_path.replace('/', "_");
        let test_name = test_name
            .rsplit_once('.')
            .map(|(base, _)| base.to_string())
            .unwrap_or(test_name);
        let full_path = format!("{}/{}", root, relative_path);
        result.push((test_name, relative_path, full_path));
    }
    result
}

#[test]
fn typed_ast_tests() {
    let tests_dir = match std::env::var("TYPED_AST_TESTS_DIR") {
        Ok(dir) => dir,
        Err(_) => {
            let candidates = ["flow/tests", "tests", "../tests", "fbcode/flow/tests"];
            match candidates.iter().find(|p| std::path::Path::new(p).is_dir()) {
                Some(p) => p.to_string(),
                None => {
                    if let Some(cargo_manifest_dir) = option_env!("CARGO_MANIFEST_DIR") {
                        let cargo_candidate =
                            std::path::Path::new(cargo_manifest_dir).join("../../../tests");
                        if cargo_candidate.is_dir() {
                            cargo_candidate.to_string_lossy().into_owned()
                        } else {
                            panic!(
                                "typed_ast_tests: tests directory not found. \
                                 Set TYPED_AST_TESTS_DIR to the absolute path of flow/tests/. \
                                 Tried: {:?} and {}",
                                candidates,
                                cargo_candidate.display(),
                            );
                        }
                    } else {
                        panic!(
                            "typed_ast_tests: tests directory not found. \
                             Set TYPED_AST_TESTS_DIR to the absolute path of flow/tests/. \
                             Tried: {:?}; CARGO_MANIFEST_DIR is unavailable",
                            candidates,
                        );
                    }
                }
            }
        }
    };
    let root = std::fs::canonicalize(&tests_dir)
        .unwrap_or_else(|_| panic!("failed to resolve tests directory '{}'", tests_dir))
        .to_string_lossy()
        .into_owned();
    let handle = std::thread::Builder::new()
        .stack_size(1024 * 1024 * 1024)
        .spawn(move || {
            let test_cases = tests(&root);
            assert!(!test_cases.is_empty(), "No test files found in {}", root);
            let mut failures = Vec::new();
            for (test_name, relative_path, full_path) in &test_cases {
                let result = std::panic::catch_unwind(|| {
                    test_case(relative_path, full_path);
                });
                if let Err(e) = result {
                    let msg = if let Some(s) = e.downcast_ref::<String>() {
                        s.clone()
                    } else if let Some(s) = e.downcast_ref::<&str>() {
                        s.to_string()
                    } else {
                        "unknown error".to_string()
                    };
                    failures.push((test_name.clone(), msg));
                }
            }
            if !failures.is_empty() {
                let mut report = format!("{} typed AST test(s) failed:\n", failures.len());
                for (name, msg) in &failures {
                    report.push_str(&format!("\n--- {} ---\n{}\n", name, msg));
                }
                panic!("{}", report);
            }
        })
        .expect("failed to spawn test thread");
    handle.join().unwrap();
}
