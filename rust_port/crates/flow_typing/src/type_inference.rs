/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::sync::Arc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_common::files;
use flow_common::options::JsxMode;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_env_builder::env_api;
use flow_env_builder::env_api::AutocompleteHooks;
use flow_env_builder::name_def;
use flow_env_builder::name_def_types::ScopeKind;
use flow_env_builder_resolver::dependency_sigs;
use flow_env_builder_resolver::name_def_ordering;
use flow_env_builder_resolver::name_resolver;
use flow_lint_settings::lint_settings::LintParseError;
use flow_lint_settings::lint_settings::LintSettings;
use flow_lint_settings::lint_severity_cover::Builder;
use flow_lint_settings::lint_severity_cover::LintSeverityCover;
use flow_lint_settings::lints::LintKind;
use flow_lint_settings::severity::Severity;
use flow_parser::ast;
use flow_parser::ast::Comment;
use flow_parser::ast::CommentKind;
use flow_parser::ast::statement;
use flow_parser::ast_utils::pattern_has_type_annotation;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc;
use flow_parser::loc::Position;
use flow_parser::polymorphic_ast_mapper;
use flow_parser_utils::file_sig::FileSig;
use flow_type_sig::packed_type_sig::Module as PackedTypeSigModule;
use flow_type_sig::type_sig_options::TypeSigOptions;
use flow_typing_context::Context;
use flow_typing_context::Metadata;
use flow_typing_debug::verbose;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::InternalError;
use flow_typing_errors::error_suppressions::ErrorSuppressions;
use flow_typing_errors::intermediate_error;
use flow_typing_errors::intermediate_error_types::ContextDependentUnsupportedStatement;
use flow_typing_errors::intermediate_error_types::UnsupportedSyntax;
use flow_typing_errors::suppression_comments::ApplicableCodes;
use flow_typing_errors::suppression_comments::BadSuppressionKind;
use flow_typing_errors::suppression_comments::should_suppress;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_js::type_inference_hooks_js;
use flow_typing_loc_env::loc_env::LocEnv;
use flow_typing_statement::statement as statement_mod;
use flow_typing_type::type_::Type;
use flow_typing_utils::typed_ast_utils::ErrorMapper;

use crate::declared_types;

// Scan the list of comments to place suppressions on the appropriate locations.
// Because each comment can only contain a single code, in order to support
// suppressing multiple types of errors on one location we allow you to stack
// comments like so:
// //$FlowFixMe[x]
// //$FlowFixMe[y]
// some code causing errors x and y
//
// This logic produces a set of error codes associated with the location of the
// bottom suppression in the stack

fn scan_for_error_suppressions<'a>(
    is_ts_file: bool,
    mut acc: ErrorSuppressions,
    mut errs: Vec<ErrorMessage<ALoc>>,
    comments: impl Iterator<Item = &'a Comment<Loc>>,
) -> (ErrorSuppressions, Vec<ErrorMessage<ALoc>>) {
    // If multiple comments are stacked together, we join them into a codeset positioned on the
    // location of the last comment
    let mut supps: Vec<(Loc, ApplicableCodes)> = Vec::new();

    for comment in comments {
        match should_suppress(is_ts_file, &comment.text, &comment.loc) {
            Err(BadSuppressionKind::MalformedCode) => {
                errs.push(ErrorMessage::EMalformedCode(ALoc::of_loc(
                    comment.loc.dupe(),
                )));
            }
            Err(BadSuppressionKind::MissingCode) => {
                errs.push(ErrorMessage::ECodelessSuppression(ALoc::of_loc(
                    comment.loc.dupe(),
                )));
            }
            Ok(None) => {}
            Ok(Some(codes)) => {
                if let Some((prev_loc, prev_codes)) = supps.last_mut() {
                    if comment.loc.start.line == prev_loc.end.line + 1 {
                        let joined = prev_codes.clone().join(codes);
                        prev_loc.end = comment.loc.end;
                        *prev_codes = joined;
                        continue;
                    }
                }
                supps.push((comment.loc.dupe(), codes));
            }
        }
    }

    for (loc, codes) in supps {
        acc.add(loc, codes);
    }
    (acc, errs)
}

#[derive(Debug, Clone)]
struct Located<T> {
    value: T,
    loc: Loc,
}

#[derive(Debug, Clone, Copy)]
enum RangeKeyword {
    /// Comment lasting until negated
    Unending,
    /// covers current line
    Line,
    /// covers next line
    NextLine,
}

fn scan_for_lint_suppressions(
    in_libdef: bool,
    base_settings: &LintSettings<Severity>,
    mut acc: ErrorSuppressions,
    mut errs: Vec<ErrorMessage<ALoc>>,
    file_keys_with_comments: Vec<(FileKey, Vec<Comment<Loc>>)>,
) -> (
    FlowOrdMap<FileKey, LintSeverityCover>,
    ErrorSuppressions,
    Vec<ErrorMessage<ALoc>>,
) {
    // Get the position induced by reading the string str from the starting position pos
    // Get the position induced by reading [the substring of str from index
    // onwards] from the starting position pos
    fn update_pos(mut pos: Position, s: &str) -> Position {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\r' => {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                        i += 1;
                    }
                    pos.line += 1;
                    pos.column = 0;
                }
                b'\n' => {
                    pos.line += 1;
                    pos.column = 0;
                }
                _ => {
                    pos.column += 1;
                }
            }
            i += 1;
        }
        pos
    }

    // Given a string like `"flowlint-line foo:bar"`, returns `Some((Line, Some("foo:bar")))`
    fn parse_keyword(
        comment: &Located<String>,
    ) -> Option<(Located<RangeKeyword>, Option<Located<String>>)> {
        const IGNORE_CHARS: &str = " \t\n\r*";
        let keywords = [
            ("flowlint-line", RangeKeyword::Line),
            ("flowlint-next-line", RangeKeyword::NextLine),
            ("flowlint", RangeKeyword::Unending),
        ];

        // `prefix_length` returns the position of the first non-whitespace character in
        // [s] after [prefix]. If [s] does not start with [prefix], or [prefix] is not followed by
        // whitespace, returns [None].
        fn prefix_length(prefix: &str, s: &str, ignore_chars: &str) -> Option<usize> {
            let sl = prefix.len();
            if !s.starts_with(prefix) {
                None
            } else if s.len() == sl {
                Some(sl)
            } else {
                match s[sl..].find(|c: char| !ignore_chars.contains(c)) {
                    Some(0) => None,
                    Some(i) => Some(sl + i),
                    None => None,
                }
            }
        }

        fn try_keyword(
            comment: &Located<String>,
            keywords: &[(&str, RangeKeyword)],
            ignore_chars: &str,
        ) -> Option<(Located<RangeKeyword>, Option<Located<String>>)> {
            for (prefix, range) in keywords {
                let value = &comment.value;
                let value_len = value.len();

                match prefix_length(prefix, value, ignore_chars) {
                    Some(i) if i == value_len => {
                        return Some((
                            Located {
                                loc: comment.loc.dupe(),
                                value: *range,
                            },
                            None,
                        ));
                    }
                    Some(i) => {
                        let range_end = update_pos(comment.loc.start, prefix);
                        let args_start = update_pos(comment.loc.start, &value[..i]);
                        let range_located = Located {
                            value: *range,
                            loc: Loc {
                                source: comment.loc.source.dupe(),
                                start: comment.loc.start,
                                end: range_end,
                            },
                        };
                        let args = Located {
                            value: value[i..].to_string(),
                            loc: Loc {
                                source: comment.loc.source.dupe(),
                                start: args_start,
                                end: comment.loc.end,
                            },
                        };
                        return Some((range_located, Some(args)));
                    }
                    None => continue,
                }
            }
            None
        }

        try_keyword(comment, &keywords, IGNORE_CHARS)
    }

    // Trims whitespace and stars from the front and end of loc_str.
    fn trim_and_stars_locational(located: Located<String>) -> Located<String> {
        const IGNORE_CHARS: &str = " \t\n\r*";
        let value = &located.value;
        let start_offset = value.find(|c: char| !IGNORE_CHARS.contains(c));
        let end_offset = value.rfind(|c: char| !IGNORE_CHARS.contains(c));

        let start = match start_offset {
            Some(offset) => update_pos(located.loc.start, &value[..offset]),
            None => located.loc.start,
        };

        let value = match (start_offset, end_offset) {
            (Some(i), Some(j)) => {
                // j is byte offset where char STARTS - need to include full char
                // same below
                let char_at_j = value[j..].chars().next().unwrap();
                let end = j + char_at_j.len_utf8();
                value[i..end].to_string()
            }
            (Some(i), None) => value[i..].to_string(),
            (None, Some(j)) => {
                let char_at_j = value[j..].chars().next().unwrap();
                let end = j + char_at_j.len_utf8();
                value[..end].to_string()
            }
            (None, None) => value.to_string(),
        };

        let end = update_pos(start, &value);
        let loc = Loc {
            source: located.loc.source,
            start,
            end,
        };

        Located { value, loc }
    }

    fn split_delim_locational(delim: char, located: Located<String>) -> Vec<Located<String>> {
        let delim_str = delim.to_string();
        let source = located.loc.source.dupe();
        let parts: Vec<&str> = located.value.split(delim).collect();

        let mut result = Vec::new();
        let mut start = located.loc.start;

        for part in parts {
            let end = update_pos(start, part);
            result.push(Located {
                value: part.to_string(),
                loc: Loc {
                    source: source.dupe(),
                    start,
                    end,
                },
            });
            start = update_pos(end, &delim_str);
        }

        result
    }

    fn add_error((loc, kind): (Loc, LintParseError)) -> ErrorMessage<ALoc> {
        ErrorMessage::ELintSetting(Box::new((ALoc::of_loc(loc), kind)))
    }

    fn parse_kind(loc_str: &Located<String>) -> Result<Vec<LintKind>, (Loc, LintParseError)> {
        match LintKind::parse_from_str(&loc_str.value) {
            Some(kinds) => Ok(kinds),
            None => Err((loc_str.loc.dupe(), LintParseError::NonexistentRule)),
        }
    }

    fn parse_value(loc_value: &Located<String>) -> Result<Severity, (Loc, LintParseError)> {
        match Severity::severity_of_str(&loc_value.value) {
            Some(state) => Ok(state),
            None => Err((loc_value.loc.dupe(), LintParseError::InvalidSetting)),
        }
    }

    fn get_kind_setting(
        acc: &mut Vec<Vec<(LintKind, (Severity, Loc))>>,
        errs: &mut Vec<ErrorMessage<ALoc>>,
        arg: Located<String>,
    ) {
        let arg = trim_and_stars_locational(arg);
        let arg_loc = arg.loc.dupe();
        let mut parts = split_delim_locational(':', arg).into_iter();

        let (rule, setting) = match (parts.next(), parts.next(), parts.next()) {
            (Some(rule), Some(setting), None) => (rule, setting),
            _ => {
                errs.push(ErrorMessage::ELintSetting(Box::new((
                    ALoc::of_loc(arg_loc),
                    LintParseError::MalformedArgument,
                ))));
                return;
            }
        };

        let rule = trim_and_stars_locational(rule);
        let setting = trim_and_stars_locational(setting);

        match (parse_kind(&rule), parse_value(&setting)) {
            (Ok(kinds), Ok(severity)) => {
                let settings: Vec<(LintKind, (Severity, Loc))> = kinds
                    .into_iter()
                    .map(|kind| (kind, (severity, arg_loc.dupe())))
                    .collect();
                acc.push(settings);
            }
            (Err((loc, err)), Ok(_)) => {
                errs.push(ErrorMessage::ELintSetting(Box::new((
                    ALoc::of_loc(loc),
                    err,
                ))));
            }
            (Ok(_), Err((loc, err))) => {
                errs.push(ErrorMessage::ELintSetting(Box::new((
                    ALoc::of_loc(loc),
                    err,
                ))));
            }
            (Err((loc1, err1)), Err((loc2, err2))) => {
                errs.push(ErrorMessage::ELintSetting(Box::new((
                    ALoc::of_loc(loc1),
                    err1,
                ))));
                errs.push(ErrorMessage::ELintSetting(Box::new((
                    ALoc::of_loc(loc2),
                    err2,
                ))));
            }
        }
    }

    // parse arguments of the form lint1:setting1,lint2:setting2...
    fn get_settings_list(
        errs: &mut Vec<ErrorMessage<ALoc>>,
        args: Located<String>,
    ) -> Vec<Vec<(LintKind, (Severity, Loc))>> {
        let args_parts = split_delim_locational(',', args);
        let mut acc = Vec::new();
        for arg in args_parts {
            get_kind_setting(&mut acc, errs, arg);
        }
        acc
    }

    // Doesn't preserve offset, but is only used in locations where offset isn't used,
    // so that's fine.
    fn get_range(keyword: &Located<RangeKeyword>) -> Loc {
        match keyword.value {
            RangeKeyword::Unending => Loc {
                source: keyword.loc.source.dupe(),
                start: keyword.loc.start,
                end: Position {
                    line: i32::MAX / 2,
                    column: i32::MAX / 2,
                },
            },
            RangeKeyword::Line => Loc {
                source: keyword.loc.source.dupe(),
                start: Position {
                    line: keyword.loc.end.line,
                    column: 0,
                },
                end: Position {
                    line: keyword.loc.end.line + 1,
                    column: 0,
                },
            },
            RangeKeyword::NextLine => Loc {
                source: keyword.loc.source.dupe(),
                start: Position {
                    line: keyword.loc.end.line + 1,
                    column: 0,
                },
                end: Position {
                    line: keyword.loc.end.line + 2,
                    column: 0,
                },
            },
        }
    }

    fn convert_comment(comment: &Comment<Loc>) -> Located<String> {
        let loc = comment.loc.dupe();
        // Comment locs contain the comment characters themselves. (//, /*, and */)
        // Trim the locs to line up with the contents of the comment.
        match comment.kind {
            CommentKind::Block => Located {
                value: comment.text.to_string(),
                loc: Loc {
                    source: loc.source,
                    start: Position {
                        line: loc.start.line,
                        column: loc.start.column + 2,
                    },
                    end: Position {
                        line: loc.end.line,
                        column: loc.end.column - 2,
                    },
                },
            },
            CommentKind::Line => Located {
                value: comment.text.to_string(),
                loc: Loc {
                    source: loc.source,
                    start: Position {
                        line: loc.start.line,
                        column: loc.start.column + 2,
                    },
                    end: loc.end,
                },
            },
        }
    }

    fn process_comment(
        in_libdef: bool,
        acc: (
            Builder,
            LintSettings<Severity>,
            BTreeSet<Loc>,
            Vec<ErrorMessage<ALoc>>,
        ),
        comment: Comment<Loc>,
    ) -> (
        Builder,
        LintSettings<Severity>,
        BTreeSet<Loc>,
        Vec<ErrorMessage<ALoc>>,
    ) {
        let (mut severity_cover_builder, running_settings, suppression_locs, mut errs) = acc;
        let loc_comment = convert_comment(&comment);
        let loc_comment = trim_and_stars_locational(loc_comment);

        match parse_keyword(&loc_comment) {
            // Case where we're changing certain lint settings
            Some((keyword, Some(args))) => {
                let settings_list = get_settings_list(&mut errs, args);

                let mut error_encountered = false;

                let (new_builder, new_running_settings) = {
                    let covered_range = get_range(&keyword);
                    let new_running_settings = severity_cover_builder.update_settings_and_running(
                        in_libdef,
                        running_settings.clone(),
                        |(loc, err)| {
                            error_encountered = true;
                            errs.push(add_error((loc, err)));
                        },
                        &covered_range,
                        settings_list.clone(),
                    );
                    (severity_cover_builder, new_running_settings)
                };
                severity_cover_builder = new_builder;

                // Only report overwritten arguments if there are no no-op arguments,
                // to avoid error duplication
                if !error_encountered {
                    // Check for overwritten arguments
                    let used_locs = new_running_settings.fold(
                        |_kind, (_, loc), mut acc| {
                            if let Some(l) = loc {
                                acc.insert(l.dupe());
                            }
                            acc
                        },
                        BTreeSet::new(),
                    );

                    let arg_locs: Vec<Option<Loc>> = settings_list
                        .iter()
                        .map(|settings| settings.first().map(|(_, (_, loc))| loc.dupe()))
                        .collect();

                    for arg_loc_opt in arg_locs {
                        if let Some(arg_loc) = arg_loc_opt {
                            if !used_locs.contains(&arg_loc) {
                                error_encountered = true;
                                errs.push(add_error((
                                    arg_loc,
                                    LintParseError::OverwrittenArgument,
                                )));
                            }
                        }
                    }
                }

                // Only report unused suppressions if there are no redundant settings,
                // to avoid error duplication. (The suppression_locs are later used to detect
                // unused suppressions; by never storing their locations we are effectively
                // immediately using them.)
                let suppression_locs = if !error_encountered {
                    settings_list
                        .iter()
                        .fold(suppression_locs, |mut acc, settings| {
                            if let Some((_, (Severity::Off, loc))) = settings.first() {
                                acc.insert(loc.dupe());
                            }
                            acc
                        })
                } else {
                    suppression_locs
                };

                match keyword.value {
                    RangeKeyword::Line | RangeKeyword::NextLine => (
                        severity_cover_builder,
                        running_settings,
                        suppression_locs,
                        errs,
                    ),
                    RangeKeyword::Unending => (
                        severity_cover_builder,
                        new_running_settings,
                        suppression_locs,
                        errs,
                    ),
                }
            }
            Some((keyword, None)) => {
                // Case where we're wholly enabling/disabling linting
                // TODO (rballard): regional lint disabling
                errs.push(add_error((keyword.loc, LintParseError::NakedComment)));
                (
                    severity_cover_builder,
                    running_settings,
                    suppression_locs,
                    errs,
                )
            }
            None => (
                severity_cover_builder,
                running_settings,
                suppression_locs,
                errs,
            ),
        }
    }

    let mut severity_covers: FlowOrdMap<FileKey, LintSeverityCover> = FlowOrdMap::new();

    for (file_key, comments) in file_keys_with_comments {
        let severity_cover_builder = Builder::new(file_key.dupe(), base_settings.clone());
        let (severity_cover_builder, _, suppression_locs, new_errs) = comments.into_iter().fold(
            (
                severity_cover_builder,
                base_settings.clone(),
                BTreeSet::new(),
                errs,
            ),
            |acc, comment| process_comment(in_libdef, acc, comment),
        );
        errs = new_errs;

        let severity_cover = severity_cover_builder.bake();
        acc.add_lint_suppressions(suppression_locs);
        severity_covers.insert(file_key, severity_cover);
    }

    (severity_covers, acc, errs)
}

pub fn scan_for_suppressions(
    in_libdef: bool,
    lint_severities: &LintSettings<Severity>,
    file_keys_with_comments: Vec<(FileKey, &[Comment<Loc>])>,
) -> (
    FlowOrdMap<FileKey, LintSeverityCover>,
    ErrorSuppressions,
    Vec<ErrorMessage<ALoc>>,
) {
    let file_keys_with_comments: Vec<_> = file_keys_with_comments
        .into_iter()
        .map(|(file, comments)| {
            let mut comments = comments.to_vec();
            comments.sort_by(|c1, c2| c1.loc.cmp(&c2.loc));
            (file, comments)
        })
        .collect();

    let (acc, errs) = file_keys_with_comments.iter().fold(
        (ErrorSuppressions::empty(), Vec::new()),
        |(acc, errs), (file_key, comments)| {
            let is_ts_file = files::has_ts_ext(file_key);
            scan_for_error_suppressions(is_ts_file, acc, errs, comments.iter())
        },
    );
    scan_for_lint_suppressions(
        in_libdef,
        lint_severities,
        acc,
        errs,
        file_keys_with_comments,
    )
}

// Driver

struct DepSigsContext<'a, 'cx>(&'a mut Context<'cx>);

impl<'a, 'cx> dependency_sigs::Context for DepSigsContext<'a, 'cx> {
    fn enable_enums(&self) -> bool {
        self.0.enable_enums()
    }

    fn file(&self) -> FileKey {
        self.0.file().dupe()
    }

    fn jsx(&self) -> JsxMode {
        self.0.jsx().clone()
    }

    fn react_runtime(&self) -> flow_common::options::ReactRuntime {
        self.0.react_runtime()
    }

    fn enable_const_params(&self) -> bool {
        self.0.enable_const_params()
    }

    fn strict_property_initialization(&self) -> bool {
        self.0.strict_property_initialization()
    }

    fn stylex_shorthand_prop(&self) -> Option<&str> {
        self.0.stylex_shorthand_prop()
    }

    fn add_exhaustive_check(&mut self, loc: ALoc, cases: (Vec<ALoc>, bool)) {
        self.0.add_exhaustive_check(loc, cases);
    }

    fn exhaustive_check(&self, loc: &ALoc) -> Option<(Vec<ALoc>, bool)> {
        self.0.exhaustive_check(loc)
    }
}

struct FlowJsUtilsFlow<'a, 'cx>(std::marker::PhantomData<(&'a (), &'cx ())>);

impl<'a, 'cx: 'a> dependency_sigs::Flow for FlowJsUtilsFlow<'a, 'cx> {
    type Cx = DepSigsContext<'a, 'cx>;

    fn add_output(cx: &mut Self::Cx, error: ErrorMessage<ALoc>) {
        flow_js_utils::add_output_non_speculating(cx.0, error);
    }
}

enum DeclaredTypesMode<'a> {
    Skip,
    Compute {
        ast: &'a ast::Program<Loc, Loc>,
        type_sig_options: &'a TypeSigOptions,
        current_type_sig: Option<Arc<PackedTypeSigModule<Loc>>>,
        file_sig: &'a FileSig,
    },
}

/// `typed_aloc_ast` is the program whose typed AST is checked, as described at
/// `name_resolver::walk_with_prepass`.
fn initialize_env_with_mode<'cx>(
    cx: &mut Context<'cx>,
    exclude_syms: Option<BTreeSet<FlowSmolStr>>,
    aloc_ast: &ast::Program<ALoc, ALoc>,
    typed_aloc_ast: &ast::Program<ALoc, ALoc>,
    declared_types_mode: DeclaredTypesMode<'_>,
) -> Result<Option<declared_types::ResolvedAnalysis>, flow_utils_concurrency::job_error::JobError> {
    let lib = cx.is_global_lib_context();
    let toplevel_scope_kind = if lib {
        ScopeKind::Global
    } else {
        ScopeKind::Module
    };
    let exclude_syms = exclude_syms.unwrap_or_default();
    let result: Result<
        Result<Option<declared_types::ResolvedAnalysis>, env_api::EnvInvariant<ALoc>>,
        flow_utils_concurrency::job_error::JobError,
    > = (|| {
        let empty_file_sig = FileSig::empty();
        let file_sig = match &declared_types_mode {
            DeclaredTypesMode::Skip => &empty_file_sig,
            DeclaredTypesMode::Compute { file_sig, .. } => *file_sig,
        };
        let (program_loc, scopes, ssa_values, unbound_names, providers) =
            name_resolver::prepass(&DepSigsContext(cx), file_sig, aloc_ast);
        // The targeted callee analysis runs between the prepass and the
        // resolver walk; files without candidates return early from packing.
        let resolved_declared_types = match declared_types_mode {
            DeclaredTypesMode::Skip => None,
            DeclaredTypesMode::Compute {
                ast,
                type_sig_options,
                current_type_sig,
                file_sig,
            } => declared_types::pack(
                type_sig_options,
                cx,
                ast,
                aloc_ast,
                &scopes,
                &providers,
                current_type_sig,
                file_sig,
            )
            .map(|analysis| declared_types::resolve(cx, analysis)),
        };
        let assertion_calls = resolved_declared_types
            .as_ref()
            .map(|resolved| declared_types::assertion_calls(resolved).clone())
            .unwrap_or_default();
        let (_abrupt_completion, info) =
            name_resolver::walk_with_prepass::<DepSigsContext<'_, '_>, FlowJsUtilsFlow<'_, '_>>(
                &mut DepSigsContext(cx),
                lib,
                exclude_syms.into_iter().collect(),
                program_loc,
                scopes,
                ssa_values,
                unbound_names,
                providers,
                assertion_calls.clone(),
                file_sig,
                aloc_ast,
                typed_aloc_ast,
            );
        let info = info.to_env_info();
        let autocomplete_hooks = AutocompleteHooks {
            id_hook: Box::new(|name: &str, loc: &ALoc| {
                type_inference_hooks_js::dispatch_id_hook(name, loc.dupe())
            }),
            literal_hook: Box::new(|loc: &ALoc| {
                type_inference_hooks_js::dispatch_literal_hook(loc.dupe())
            }),
            obj_prop_decl_hook: Box::new(|name: &str, loc: &ALoc| {
                type_inference_hooks_js::dispatch_obj_prop_decl_hook(name, loc.dupe())
            }),
        };
        let react_jsx = matches!(cx.jsx(), JsxMode::JsxReact);
        let (name_def_graph, ast_hint_map) = match name_def::find_defs(
            &autocomplete_hooks,
            react_jsx,
            assertion_calls,
            &info,
            toplevel_scope_kind,
            aloc_ast,
        ) {
            Ok(v) => v,
            Err(invariant) => return Ok(Err(invariant)),
        };
        let mut hint_map = env_api::EnvMap::empty();
        for (loc, hints) in &ast_hint_map {
            hint_map.insert(
                env_api::EnvKey::ordinary(loc.dupe()),
                crate::env_resolution::lazily_resolve_hints(cx, loc.dupe(), hints),
            );
        }
        let pred_func_map = info
            .pred_func_map
            .iter()
            .map(|(loc, pfi)| {
                (
                    loc.dupe(),
                    crate::env_resolution::resolve_pred_func(cx, pfi),
                )
            })
            .collect();
        let info = std::rc::Rc::new(info);
        let env = LocEnv::with_info(
            ScopeKind::Global,
            ast_hint_map,
            hint_map,
            info.dupe(),
            pred_func_map,
            name_def_graph.dupe(),
        );
        *cx.environment_mut() = env;
        cx.init_interface_merge_field_index();
        let components = match name_def_ordering::build_ordering::<
            _,
            _,
            DepSigsContext<'_, '_>,
            FlowJsUtilsFlow<'_, '_>,
        >(
            &mut DepSigsContext(cx),
            &autocomplete_hooks,
            &info,
            &name_def_graph,
        ) {
            Ok(c) => c,
            Err(msg) => {
                flow_js_utils::add_output_non_speculating(cx, *msg);
                return Ok(Err(env_api::EnvInvariant::new(
                    None,
                    env_api::EnvInvariantFailure::NameDefGraphMismatch,
                )));
            }
        };
        for component in components.iter() {
            crate::cycles::handle_component(cx, &name_def_graph, component);
        }
        flow_typing_utils::type_env::init_env(cx, toplevel_scope_kind);
        let (scope_kind, class_stack) = {
            let env = cx.environment();
            (env.scope_kind, env.class_stack.dupe())
        };
        for component in components.iter() {
            crate::env_resolution::resolve_component(cx, &name_def_graph, component)?;
        }
        verbose::print_if_verbose_lazy(cx, None, None, None, || {
            vec!["Finished all components".to_string()]
        });
        {
            let env = cx.environment_mut();
            env.scope_kind = scope_kind;
            env.class_stack = class_stack;
        }
        Ok(Ok(resolved_declared_types))
    })();
    let result = result?;
    match result {
        Ok(resolved_declared_types) => Ok(resolved_declared_types),
        Err(env_api::EnvInvariant { loc, failure }) => {
            let loc = loc.unwrap_or_else(|| aloc_ast.loc.dupe());
            flow_js_utils::add_output_non_speculating(
                cx,
                ErrorMessage::EInternal(Box::new((loc, InternalError::EnvInvariant(failure)))),
            );
            Ok(None)
        }
    }
}

pub fn initialize_env<'cx>(
    cx: &mut Context<'cx>,
    type_sig_options: &TypeSigOptions,
    exclude_syms: Option<BTreeSet<FlowSmolStr>>,
    ast: &ast::Program<Loc, Loc>,
    aloc_ast: &ast::Program<ALoc, ALoc>,
    file_sig: &FileSig,
) -> Result<(), flow_utils_concurrency::job_error::JobError> {
    let _resolved_declared_types = initialize_env_with_mode(
        cx,
        exclude_syms,
        aloc_ast,
        aloc_ast,
        DeclaredTypesMode::Compute {
            ast,
            type_sig_options,
            current_type_sig: None,
            file_sig,
        },
    )?;
    Ok(())
}

/// Lint suppressions are handled iff lint_severities is Some.
/// `type_sig_options` must be the options used to parse `ast`.
/// `current_type_sig`'s location keys must resolve through `cx`'s ALoc table for
/// `filename`, i.e. both must come from the same parse as `ast`. Pass `None` when
/// no such signature exists; the required roots are then repacked from `ast`.
pub fn infer_ast<'a>(
    lint_severities: &LintSettings<Severity>,
    cx: &mut Context<'a>,
    filename: &FileKey,
    file_sig: Arc<FileSig>,
    metadata: &Metadata,
    loc_comments: &[Comment<Loc>],
    ast: &ast::Program<Loc, Loc>,
    aloc_ast: &ast::Program<ALoc, ALoc>,
    type_sig_options: &TypeSigOptions,
    current_type_sig: Option<Arc<PackedTypeSigModule<Loc>>>,
) -> Result<ast::Program<ALoc, (ALoc, Type)>, flow_utils_concurrency::job_error::JobError> {
    assert!(cx.is_checked());
    // Check if the file is in declarations mode and we're not in IDE mode.
    // IDE services should still get full type checking even for declaration files.
    if files::is_declaration(&cx.file_options(), &filename.to_absolute())
        && !type_inference_hooks_js::is_for_ide()
    {
        {
            let Ok(v) = polymorphic_ast_mapper::program(&mut ErrorMapper, aloc_ast);
            Ok(v)
        }
    } else {
        let ast::Program {
            loc: ref prog_aloc,
            ref statements,
            ref interpreter,
            ref comments,
            ref all_comments,
        } = *aloc_ast;
        let _resolved_declared_types = initialize_env_with_mode(
            cx,
            None,
            aloc_ast,
            aloc_ast,
            DeclaredTypesMode::Compute {
                ast,
                type_sig_options,
                current_type_sig,
                file_sig: &file_sig,
            },
        )?;
        let typed_statements = statement_mod::statement_list(cx, statements)?;
        let tast = ast::Program {
            loc: prog_aloc.dupe(),
            statements: typed_statements.into(),
            interpreter: interpreter.clone(),
            comments: comments.dupe(),
            all_comments: all_comments.dupe(),
        };
        crate::merge::post_merge_checks(cx, file_sig, aloc_ast, &tast, metadata)?;
        let (severity_cover, suppressions, suppression_errors) = scan_for_suppressions(
            false,
            lint_severities,
            vec![(filename.dupe(), loc_comments)],
        );
        cx.add_severity_covers(severity_cover);
        cx.add_error_suppressions(suppressions);
        for err in suppression_errors {
            flow_js_utils::add_output_non_speculating(cx, err);
        }
        cx.reset_errors(intermediate_error::post_process_errors(cx.errors()));
        Ok(tast)
    }
}

struct LibDefLocMapperAndValidator<'a, 'cx> {
    cx: &'a mut Context<'cx>,
}

fn stmt_validator(
    cx: &mut Context,
    in_toplevel_scope: bool,
    stmt: &statement::Statement<ALoc, ALoc>,
) -> bool {
    let loc = stmt.loc().dupe();
    let error = |kind: &str| -> ErrorMessage<ALoc> {
        ErrorMessage::EUnsupportedSyntax(Box::new((
            loc.dupe(),
            UnsupportedSyntax::ContextDependentUnsupportedStatement(
                ContextDependentUnsupportedStatement::UnsupportedStatementInLibdef(kind.into()),
            ),
        )))
    };
    let error_opt: Option<ErrorMessage<ALoc>> = match &**stmt {
        statement::StatementInner::DeclareClass { .. }
        | statement::StatementInner::DeclareComponent { .. }
        | statement::StatementInner::DeclareEnum { .. }
        | statement::StatementInner::DeclareExportDeclaration { .. }
        | statement::StatementInner::DeclareFunction { .. }
        | statement::StatementInner::DeclareInterface { .. }
        | statement::StatementInner::DeclareModule { .. }
        | statement::StatementInner::DeclareModuleExports { .. }
        | statement::StatementInner::ExportAssignment { .. }
        | statement::StatementInner::NamespaceExportDeclaration { .. }
        | statement::StatementInner::DeclareTypeAlias { .. }
        | statement::StatementInner::DeclareOpaqueType { .. }
        | statement::StatementInner::DeclareVariable { .. }
        | statement::StatementInner::Empty { .. }
        | statement::StatementInner::EnumDeclaration { .. }
        | statement::StatementInner::InterfaceDeclaration { .. }
        | statement::StatementInner::TypeAlias { .. }
        | statement::StatementInner::OpaqueType { .. } => None,
        // directives are not used and could be banned as well
        statement::StatementInner::Expression { inner, .. } if inner.directive.is_some() => None,
        statement::StatementInner::ExportNamedDeclaration { inner, .. }
            if inner.export_kind == statement::ExportKind::ExportType =>
        {
            None
        }
        statement::StatementInner::ExportNamedDeclaration { inner, .. }
            if inner.export_kind == statement::ExportKind::ExportValue =>
        {
            Some(error("value export"))
        }
        statement::StatementInner::ImportDeclaration { .. } => {
            if in_toplevel_scope {
                Some(ErrorMessage::EUnsupportedSyntax(Box::new((
                    loc.dupe(),
                    UnsupportedSyntax::ContextDependentUnsupportedStatement(
                        ContextDependentUnsupportedStatement::ToplevelLibraryImport,
                    ),
                ))))
            } else {
                None
            }
        }
        statement::StatementInner::ImportEqualsDeclaration { .. } => {
            if in_toplevel_scope {
                Some(ErrorMessage::EUnsupportedSyntax(Box::new((
                    loc.dupe(),
                    UnsupportedSyntax::ContextDependentUnsupportedStatement(
                        ContextDependentUnsupportedStatement::ToplevelLibraryImport,
                    ),
                ))))
            } else {
                None
            }
        }
        statement::StatementInner::DeclareNamespace { inner, .. } => match &inner.id {
            statement::declare_namespace::Id::Local(_) => None,
            statement::declare_namespace::Id::Global(_) => {
                if in_toplevel_scope {
                    Some(error("declare global"))
                } else {
                    None
                }
            }
        },
        statement::StatementInner::Block { .. } => Some(error("block")),
        statement::StatementInner::Break { .. } => Some(error("break")),
        statement::StatementInner::ClassDeclaration { .. } => Some(error("class declaration")),
        statement::StatementInner::ComponentDeclaration { inner, .. } => {
            if inner.body.is_none() {
                None
            } else {
                Some(error("component declaration"))
            }
        }
        statement::StatementInner::Continue { .. } => Some(error("continue")),
        statement::StatementInner::Debugger { .. } => Some(error("debugger")),
        statement::StatementInner::DoWhile { .. } => Some(error("do while")),
        statement::StatementInner::ExportDefaultDeclaration { .. } => Some(error("export default")),
        statement::StatementInner::Expression { .. } => Some(error("expression")),
        statement::StatementInner::For { .. } => Some(error("for")),
        statement::StatementInner::ForIn { .. } => Some(error("for in")),
        statement::StatementInner::ForOf { .. } => Some(error("for of")),
        statement::StatementInner::FunctionDeclaration { .. } => {
            Some(error("function declaration"))
        }
        statement::StatementInner::If { .. } => Some(error("if")),
        statement::StatementInner::Labeled { .. } => Some(error("labeled")),
        statement::StatementInner::Match { .. } => Some(error("match")),
        statement::StatementInner::RecordDeclaration { .. } => Some(error("record declaration")),
        statement::StatementInner::Return { .. } => Some(error("return")),
        statement::StatementInner::Switch { .. } => Some(error("switch")),
        statement::StatementInner::Throw { .. } => Some(error("throw")),
        statement::StatementInner::Try { .. } => Some(error("try")),
        statement::StatementInner::VariableDeclaration { inner, .. } => {
            // Allow variable declarations if all have type annotations and no initializers (ambient)
            let is_ambient_declarator =
                |decl: &statement::variable::Declarator<ALoc, ALoc>| -> bool {
                    decl.init.is_none() && pattern_has_type_annotation(&decl.id)
                };
            if inner.declarations.iter().all(is_ambient_declarator) {
                None
            } else {
                Some(error("variable declaration"))
            }
        }
        statement::StatementInner::While { .. } => Some(error("while")),
        statement::StatementInner::With { .. } => Some(error("with")),
        statement::StatementInner::ExportNamedDeclaration { .. } => None,
    };
    match error_opt {
        None => true,
        Some(err) => {
            cx.node_cache_mut().set_statement({
                let Ok(v) = polymorphic_ast_mapper::statement(&mut ErrorMapper, stmt);
                v
            });
            flow_js_utils::add_output_non_speculating(cx, err);
            false
        }
    }
}

impl<'ast> AstVisitor<'ast, ALoc, ALoc, &'ast ALoc, !> for LibDefLocMapperAndValidator<'_, '_> {
    fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
        loc
    }

    fn normalize_type(type_: &'ast ALoc) -> &'ast ALoc {
        type_
    }

    fn map_toplevel_statement_list(
        &mut self,
        stmts: &'ast Arc<[statement::Statement<ALoc, ALoc>]>,
    ) -> Arc<[statement::Statement<ALoc, ALoc>]> {
        let filtered: Arc<[statement::Statement<ALoc, ALoc>]> = Arc::from(
            stmts
                .iter()
                .filter(|stmt| stmt_validator(self.cx, true, stmt))
                .duped()
                .collect::<Vec<_>>(),
        );
        ast_visitor::map_statement_list_default(self, &filtered)
    }

    fn map_declare_module(
        &mut self,
        loc: &'ast ALoc,
        m: &'ast statement::DeclareModule<ALoc, ALoc>,
    ) -> statement::DeclareModule<ALoc, ALoc> {
        let statement::DeclareModule { id, body, comments } = m;
        let (body_loc, body_block) = body;
        let filtered_m = statement::DeclareModule {
            id: id.clone(),
            body: (
                body_loc.dupe(),
                statement::Block {
                    body: Arc::from(
                        body_block
                            .body
                            .iter()
                            .filter(|s| stmt_validator(self.cx, false, s))
                            .duped()
                            .collect::<Vec<_>>(),
                    ),
                    comments: body_block.comments.dupe(),
                },
            ),
            comments: comments.dupe(),
        };
        ast_visitor::map_declare_module_default(self, loc, &filtered_m)
    }
}

fn lib_def_loc_mapper_and_validator<'a>(
    cx: &mut Context<'a>,
    aloc_ast: &ast::Program<ALoc, ALoc>,
) -> ast::Program<ALoc, ALoc> {
    let mut visitor = LibDefLocMapperAndValidator { cx };
    visitor.map_program(aloc_ast)
}

// infer a parsed library file.
// processing is similar to an ordinary module, except that
// a) symbols from prior library loads are suppressed if found,
// b) bindings are added as properties to the builtin object
fn infer_lib_file<'a>(
    lint_severities: &LintSettings<Severity>,
    cx: &mut Context<'a>,
    file_key: &FileKey,
    file_sig: Arc<FileSig>,
    metadata: &Metadata,
    loc_comments: &[Comment<Loc>],
    aloc_ast: &ast::Program<ALoc, ALoc>,
) -> Result<ast::Program<ALoc, (ALoc, Type)>, flow_utils_concurrency::job_error::JobError> {
    let filtered_aloc_ast = lib_def_loc_mapper_and_validator(cx, aloc_ast);
    let ast::Program {
        loc: ref prog_aloc,
        ref statements,
        ref interpreter,
        ref comments,
        ref all_comments,
    } = *aloc_ast;
    let exclude_syms = cx.builtins().builtin_ordinary_name_set();
    let _resolved_declared_types = initialize_env_with_mode(
        cx,
        Some(exclude_syms),
        &filtered_aloc_ast,
        aloc_ast,
        DeclaredTypesMode::Skip,
    )?;
    let (severity_cover, suppressions, suppression_errors) =
        scan_for_suppressions(true, lint_severities, vec![(file_key.dupe(), loc_comments)]);
    let typed_statements = statement_mod::statement_list(cx, statements)?;
    let tast = ast::Program {
        loc: prog_aloc.dupe(),
        statements: typed_statements.into(),
        interpreter: interpreter.clone(),
        comments: comments.dupe(),
        all_comments: all_comments.dupe(),
    };
    crate::merge::post_merge_checks(cx, file_sig, aloc_ast, &tast, metadata)?;

    cx.add_severity_covers(severity_cover);
    cx.add_error_suppressions(suppressions);
    for err in suppression_errors {
        flow_js_utils::add_output_non_speculating(cx, err);
    }
    cx.reset_errors(intermediate_error::post_process_errors(cx.errors()));
    Ok(tast)
}

pub fn infer_file<'a>(
    lint_severities: &LintSettings<Severity>,
    cx: &mut Context<'a>,
    file_key: &FileKey,
    file_sig: Arc<FileSig>,
    metadata: &Metadata,
    all_comments: &[Comment<Loc>],
    ast: &ast::Program<Loc, Loc>,
    aloc_ast: &ast::Program<ALoc, ALoc>,
    type_sig_options: &TypeSigOptions,
    current_type_sig: Option<Arc<PackedTypeSigModule<Loc>>>,
) -> Result<ast::Program<ALoc, (ALoc, Type)>, flow_utils_concurrency::job_error::JobError> {
    if cx.is_global_libdef(file_key) {
        infer_lib_file(
            lint_severities,
            cx,
            file_key,
            file_sig.dupe(),
            metadata,
            all_comments,
            aloc_ast,
        )
    } else {
        infer_ast(
            lint_severities,
            cx,
            file_key,
            file_sig,
            metadata,
            all_comments,
            ast,
            aloc_ast,
            type_sig_options,
            current_type_sig,
        )
    }
}
