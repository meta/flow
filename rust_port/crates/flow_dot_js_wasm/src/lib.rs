/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![feature(never_type)]

use std::alloc::Layout;
use std::cell::LazyCell;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ffi::CString;
use std::os::raw::c_char;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Once;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocTable;
use flow_common::flow_import_specifier::FlowImportSpecifier;
use flow_common::options::AssertOperator;
use flow_common::options::Options;
use flow_common::options::ReactRule;
use flow_common::options::ReactRuntime;
use flow_common_errors::error_utils::ConcreteLocPrintableErrorSet;
use flow_common_errors::error_utils::json_output;
use flow_lint_settings::lint_settings::LintSettings;
use flow_lint_settings::lints::LintKind;
use flow_lint_settings::severity::Severity;
use flow_parser::LexMode;
use flow_parser::PERMISSIVE_PARSE_OPTIONS;
use flow_parser::TokenSinkResult;
use flow_parser::ast;
use flow_parser::comment_utils;
use flow_parser::estree_translator;
use flow_parser::file_key;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::FileKeyInner;
use flow_parser::loc::Loc;
use flow_parser::offset_utils::OffsetKind;
use flow_parser::offset_utils::OffsetTable;
use flow_parser::token::TokenKind;
use flow_parser_utils::file_sig::FileSig;
use flow_parser_utils::file_sig::FileSigOptions;
use flow_parsing::docblock_parser;
use flow_type_sig::type_sig_options::TypeSigOptions;
use flow_typing::merge;
use flow_typing::type_inference;
use flow_typing_context::Context;
use flow_typing_context::ContextArena;
use flow_typing_context::ContextStore;
use flow_typing_context::MasterContext;
use flow_typing_context::ModuleTypeThunk;
use flow_typing_context::ResolvedRequire;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_suppressions::ErrorSuppressions;
use flow_typing_errors::flow_error;
use flow_typing_errors::flow_error::ErrorSet;
use flow_typing_errors::intermediate_error;
use flow_typing_type::type_::Type;
use flow_typing_utils::annotation_inference;
use flow_utils_concurrency::check_budget::CheckBudget;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

struct State {
    files: BTreeMap<String, String>,
    master_cx: Arc<MasterContext>,
}

impl State {
    fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            master_cx: Arc::new(MasterContext::EmptyMasterContext),
        }
    }
}

struct ParsedFile {
    file_key: FileKey,
    docblock: Arc<flow_common::docblock::Docblock>,
    docblock_errors: Vec<docblock_parser::DocblockError>,
    ast: Arc<ast::Program<Loc, Loc>>,
    file_sig: Arc<FileSig>,
    parse_errors: Vec<(Loc, flow_parser::parse_error::ParseError)>,
}

struct PreparedFile<'cx> {
    parsed: ParsedFile,
    cx: Context<'cx>,
    metadata: flow_typing_context::Metadata,
    lint_severities: LintSettings<Severity>,
    type_sig_options: TypeSigOptions,
}

struct CheckedFile<'cx> {
    prepared: PreparedFile<'cx>,
    typed_ast: ast::Program<ALoc, (ALoc, Type)>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::new());
}

static INIT_ROOTS: Once = Once::new();

fn ensure_roots() {
    INIT_ROOTS.call_once(|| {
        file_key::set_project_root("");
        file_key::set_flowlib_root("");
    });
}

fn string_arg<'a>(params: &'a Value, key: &str) -> Result<&'a str, String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string parameter `{}`", key))
}

fn i32_arg(params: &Value, key: &str) -> Result<i32, String> {
    params
        .get(key)
        .and_then(Value::as_i64)
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| format!("missing i32 parameter `{}`", key))
}

fn bool_config(config: &Value, key: &str, default: bool) -> bool {
    config.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn config_options(config: Option<&Value>) -> Result<Options, String> {
    let empty = Value::Object(Default::default());
    let config = config.unwrap_or(&empty);

    let mut lint_severities = LintSettings::<Severity>::default_severities();
    for lint in [
        "deprecated-type",
        "sketchy-null",
        "sketchy-number",
        "unclear-type",
        "unnecessary-assertion",
        "unnecessary-optional-chain",
        "unsafe-getters-setters",
        "unused-promise",
    ] {
        if bool_config(config, lint, false) {
            if let Some(kinds) = LintKind::parse_from_str(lint) {
                for kind in kinds {
                    lint_severities.set_value(kind, (Severity::Err, None));
                }
            }
        }
    }

    let react_runtime = match config
        .get("react.runtime")
        .and_then(Value::as_str)
        .unwrap_or("automatic")
    {
        "classic" => ReactRuntime::Classic,
        "automatic" => ReactRuntime::Automatic,
        other => panic!("Unsupported config option react.runtime={}", other),
    };

    Ok(Options {
        all: true,
        assert_operator: if bool_config(config, "experimental.assert_operator", false) {
            AssertOperator::Enabled
        } else {
            AssertOperator::Disabled
        },
        babel_loose_array_spread: bool_config(config, "babel_loose_array_spread", false),
        component_syntax: true,
        enable_const_params: bool_config(config, "experimental.const_params", false),
        enable_pattern_matching: true,
        enable_records: true,
        enums: bool_config(config, "enums", true),
        hook_compatibility: true,
        lint_severities,
        max_header_tokens: 10,
        normalize_intersections: bool_config(config, "experimental.normalize_intersections", false),
        react_rules: Arc::from([
            ReactRule::ValidateRefAccessDuringRender,
            ReactRule::DeepReadOnlyProps,
            ReactRule::DeepReadOnlyHookReturns,
            ReactRule::RulesOfHooks,
        ]),
        react_runtime,
        recursion_limit: 10000,
        root: Arc::new(PathBuf::new()),
        strict_property_initialization: bool_config(
            config,
            "experimental.strict_property_initialization",
            false,
        ),
        strip_root: true,
        ts_syntax: bool_config(config, "experimental.ts_syntax", true),
        tslib_syntax: true,
        ts_utility_syntax: true,
        type_expansion_recursion_limit: 3,
        use_unknown_in_catch_variables: bool_config(
            config,
            "use_unknown_in_catch_variables",
            false,
        ),
        ..Default::default()
    })
}

fn builtin_sig_options() -> TypeSigOptions {
    TypeSigOptions {
        munge: false,
        facebook_key_mirror: false,
        facebook_fbt: None,
        enable_custom_error: false,
        enable_enums: true,
        enable_component_syntax: true,
        component_syntax_enabled_in_config: true,
        enable_ts_syntax: true,
        enable_ts_utility_syntax: true,
        export_as_namespace_support: false,
        hook_compatibility: true,
        enable_records: true,
        enable_relay_integration: false,
        relay_integration_module_prefix: None,
        for_builtins: true,
        locs_to_dirtify: Vec::new(),
        is_ts_file: false,
        is_dts_file: false,
        tslib_syntax: true,
        is_lib_file: true,
    }
}

fn file_sig_options(options: &Options) -> FileSigOptions {
    FileSigOptions {
        enable_enums: options.enums,
        enable_jest_integration: options.enable_jest_integration,
        enable_relay_integration: options.enable_relay_integration,
        explicit_available_platforms: None,
        file_options: options.file_options.dupe(),
        haste_module_ref_prefix: options.haste_module_ref_prefix.dupe(),
        project_options: options.projects_options.dupe(),
        relay_integration_module_prefix: options.relay_integration_module_prefix.dupe(),
        is_lib_file: false,
    }
}

fn parse_file(file_key: FileKey, content: &str, options: &Options) -> ParsedFile {
    let (docblock_errors, docblock) = docblock_parser::parse_docblock(
        options.max_header_tokens as usize,
        &options.file_options,
        &file_key,
        content,
    );
    let (ast, parse_errors) = flow_parser::parse_program_file::<()>(
        false,
        None,
        Some(PERMISSIVE_PARSE_OPTIONS),
        file_key.dupe(),
        Ok(content),
    );
    let file_sig = Arc::new(FileSig::from_program(
        &file_key,
        &ast,
        &file_sig_options(options),
    ));
    ParsedFile {
        file_key,
        docblock: Arc::new(docblock),
        docblock_errors,
        ast: Arc::new(ast),
        file_sig,
        parse_errors,
    }
}

fn parse_error_set(parsed: &ParsedFile) -> ErrorSet {
    let mut errors = ErrorSet::empty();
    for parse_error in parsed.parse_errors.iter().cloned() {
        let (loc, err) = parse_error;
        errors.add(flow_error::error_of_msg(
            parsed.file_key.dupe(),
            ErrorMessage::EParseError(Box::new((ALoc::of_loc(loc), err))),
        ));
    }
    errors
}

fn typed_builtin_module_opt(
    cx: &mut Context<'_>,
    builtin_module_name: &flow_common::flow_import_specifier::Userland,
) -> Option<ModuleTypeThunk> {
    cx.builtin_module_opt(builtin_module_name)
        .map(|(reason, lazy_module)| {
            let forcing_state =
                flow_typing_type::type_::constraint::forcing_state::ModuleTypeForcingState::of_lazy_module(
                    reason,
                    lazy_module,
                );
            annotation_inference::force_module_type_thunk(cx.id(), forcing_state)
        })
}

/// Makes the context of the file in `arena`.
fn prepare_file<'cx>(
    arena: &'cx mut ContextArena,
    filename: &str,
    content: &str,
    config: Option<&Value>,
    master_cx: Arc<MasterContext>,
) -> Result<PreparedFile<'cx>, String> {
    let options = config_options(config)?;
    let file_key = FileKey::new(FileKeyInner::SourceFile(filename.to_string()));
    let parsed = parse_file(file_key.dupe(), content, &options);
    let type_sig_options = TypeSigOptions::of_options(
        &options,
        parsed.docblock.prevent_munge(),
        Vec::new(),
        &file_key,
        false,
    );
    if !parsed.parse_errors.is_empty() {
        let ccx = arena.add_component();
        let cx = arena.add_context(
            ccx,
            flow_typing_context::mk_context_metadata(&options, Arc::default()),
            file_key.dupe(),
            Arc::default(),
            Rc::new(LazyCell::new(Box::new({
                let file_key = file_key.dupe();
                move || Rc::new(ALocTable::empty(file_key))
            })
                as Box<dyn FnOnce() -> Rc<ALocTable>>)),
            Rc::new(|_: &mut Context<'_>, _: &FlowImportSpecifier| ResolvedRequire::MissingModule),
            Rc::new(|_: &mut Context<'_>| flow_typing_builtins::Builtins::empty()),
            CheckBudget::new(None),
        );
        return Ok(PreparedFile {
            parsed,
            cx: Context::new(ContextStore::new(vec![arena]), cx),
            metadata: flow_typing_context::mk_context_metadata(&options, Arc::default()),
            lint_severities: LintSettings::<Severity>::empty_severities(),
            type_sig_options,
        });
    }

    let metadata = flow_typing_context::docblock_overrides(
        &parsed.docblock,
        &file_key,
        flow_typing_context::mk_context_metadata(&options, Arc::default()),
    );
    let lint_severities =
        merge::get_lint_severities(&metadata, &options.strict_mode, options.lint_severities);
    let aloc_table = Rc::new(LazyCell::new(Box::new({
        let file_key = file_key.dupe();
        move || Rc::new(ALocTable::empty(file_key))
    }) as Box<dyn FnOnce() -> Rc<ALocTable>>));
    let resolve_require =
        Rc::new(
            |cx: &mut Context<'_>, module_name: &FlowImportSpecifier| match module_name {
                FlowImportSpecifier::Userland(userland) => {
                    match typed_builtin_module_opt(cx, userland) {
                        Some(typed) => ResolvedRequire::TypedModule(typed),
                        None => ResolvedRequire::MissingModule,
                    }
                }
            },
        );
    let ccx = arena.add_component();
    let mk_builtins = merge::mk_builtins(&metadata, &master_cx, arena);
    let cx_id = arena.add_context(
        ccx,
        metadata.clone(),
        file_key,
        Arc::default(),
        aloc_table,
        resolve_require,
        mk_builtins,
        CheckBudget::new(None),
    );
    let mut cx = Context::new(ContextStore::new(vec![arena]), cx_id);
    cx.set_annot_error_dst(cx_id);
    Ok(PreparedFile {
        parsed,
        cx,
        metadata,
        lint_severities,
        type_sig_options,
    })
}

fn check_file<'cx>(
    arena: &'cx mut ContextArena,
    filename: &str,
    content: &str,
    config: Option<&Value>,
    master_cx: Arc<MasterContext>,
) -> Result<CheckedFile<'cx>, String> {
    let mut prepared = prepare_file(arena, filename, content, config, master_cx)?;
    if !prepared.parsed.parse_errors.is_empty() {
        return Err("parse error".to_string());
    }
    let ast::Program { all_comments, .. } = prepared.parsed.ast.as_ref();
    let aloc_ast = flow_aloc::loc_to_aloc_ast(prepared.parsed.ast.as_ref());
    let typed_ast = type_inference::infer_ast(
        &prepared.lint_severities,
        &mut prepared.cx,
        &prepared.parsed.file_key,
        prepared.parsed.file_sig.dupe(),
        &prepared.metadata,
        all_comments,
        prepared.parsed.ast.as_ref(),
        aloc_ast,
        &prepared.type_sig_options,
        None,
    )
    .map_err(|_| "type inference failed".to_string())?;
    Ok(CheckedFile {
        prepared,
        typed_ast,
    })
}

fn printable_errors(parsed: &ParsedFile, errors: ErrorSet) -> ConcreteLocPrintableErrorSet {
    let ast = parsed.ast.dupe();
    let file_key = parsed.file_key.dupe();
    intermediate_error::make_errors_printable(
        |aloc: &ALoc| aloc.to_loc_exn().dupe(),
        move |requested_file: &FileKey| {
            if requested_file == &file_key {
                Some(ast.dupe())
            } else {
                None
            }
        },
        Some(Path::new("")),
        errors,
        FileKey::is_lib_file,
    )
}

fn errors_json(
    parsed: &ParsedFile,
    content: &str,
    errors: ConcreteLocPrintableErrorSet,
    warnings: ConcreteLocPrintableErrorSet,
) -> Value {
    let stdin_file = Some((PathBuf::from(parsed.file_key.as_str()), content.to_string()));
    json_output::json_of_errors_with_context(
        Some(""),
        &stdin_file,
        &[],
        json_output::JsonVersion::JsonV1,
        OffsetKind::Utf8,
        &errors,
        &warnings,
    )
}

// Mirrors OCaml `src/flow_dot_js.ml` `check_content`: split lints by severity,
// apply suppressions, then preserve warnings in the JSON diagnostics array.
fn filtered_errors(
    cx: &mut Context<'_>,
    parsed: &ParsedFile,
    errors: ErrorSet,
) -> (ConcreteLocPrintableErrorSet, ConcreteLocPrintableErrorSet) {
    let mut suppressions = cx.take_error_suppressions();
    let severity_cover = cx.severity_cover();
    let include_suppressions = cx.include_suppressions();
    let aloc_tables = cx.aloc_tables();
    let (errors, warnings) =
        suppressions.filter_lints(errors, aloc_tables, include_suppressions, severity_cover);

    let loc_of_aloc = |aloc: &ALoc| aloc.to_loc_exn().dupe();
    let file_key = parsed.file_key.dupe();
    let ast = parsed.ast.dupe();
    let get_ast = move |requested_file: &FileKey| {
        if requested_file == &file_key {
            Some(ast.dupe())
        } else {
            None
        }
    };
    let root = Path::new("");
    let unsuppressable_error_codes = BTreeSet::new();

    let mut unused = ErrorSuppressions::empty();
    let (errors, _) = suppressions.filter_suppressed_errors(
        root,
        None,
        false,
        &unsuppressable_error_codes,
        loc_of_aloc,
        &get_ast,
        FileKey::is_lib_file,
        &errors,
        &mut unused,
    );

    let mut unused = ErrorSuppressions::empty();
    let (warnings, _) = suppressions.filter_suppressed_errors(
        root,
        None,
        false,
        &unsuppressable_error_codes,
        loc_of_aloc,
        &get_ast,
        FileKey::is_lib_file,
        &warnings,
        &mut unused,
    );

    (errors, warnings)
}

fn check_content(params: &Value) -> Result<Value, String> {
    let filename = string_arg(params, "filename")?;
    let content = string_arg(params, "content")?;
    let config = params.get("config");
    let master_cx = STATE.with(|state| state.borrow().master_cx.dupe());
    let mut arena = ContextArena::new();
    {
        let prepared = prepare_file(&mut arena, filename, content, config, master_cx)?;
        if !prepared.parsed.parse_errors.is_empty() || !prepared.parsed.docblock_errors.is_empty() {
            let errors = parse_error_set(&prepared.parsed);
            let errors = printable_errors(&prepared.parsed, errors);
            return Ok(errors_json(
                &prepared.parsed,
                content,
                errors,
                ConcreteLocPrintableErrorSet::empty(),
            ));
        }
    }
    let mut checked = check_file(
        &mut arena,
        filename,
        content,
        config,
        STATE.with(|state| state.borrow().master_cx.dupe()),
    )?;
    let errors = checked.prepared.cx.errors();
    let (errors, warnings) =
        filtered_errors(&mut checked.prepared.cx, &checked.prepared.parsed, errors);
    Ok(errors_json(
        &checked.prepared.parsed,
        content,
        errors,
        warnings,
    ))
}

fn check_registered(params: &Value) -> Result<Value, String> {
    let filename = string_arg(params, "filename")?;
    let config = params.get("config");
    let content = STATE.with(|state| state.borrow().files.get(filename).cloned());
    match content {
        Some(content) => check_content(&json!({
            "filename": filename,
            "content": content,
            "config": config.cloned().unwrap_or(Value::Null),
        })),
        None => Err(format!("File `{}` has not been registered", filename)),
    }
}

fn register_file(params: &Value) -> Result<Value, String> {
    let name = string_arg(params, "name")?;
    let content = string_arg(params, "content")?;
    STATE.with(|state| {
        state
            .borrow_mut()
            .files
            .insert(name.to_string(), content.to_string());
    });
    Ok(Value::Null)
}

fn init_builtins(params: &Value) -> Result<Value, String> {
    let filenames = params
        .get("filenames")
        .and_then(Value::as_array)
        .ok_or_else(|| "missing array parameter `filenames`".to_string())?;
    let options = config_options(None)?;
    let parsed_files = STATE.with(|state| {
        let state = state.borrow();
        filenames
            .iter()
            .rev()
            .map(|filename| {
                let filename = filename
                    .as_str()
                    .ok_or_else(|| "builtin filename must be a string".to_string())?;
                let content = state
                    .files
                    .get(filename)
                    .ok_or_else(|| format!("File `{}` has not been registered", filename))?;
                // Like the CLI, read the bundled TypeScript modules as source files that augment
                // the global scope.
                let is_ts_module = flow_common::files::has_builtin_ts_module_name(filename);
                let file_key = FileKey::new(if is_ts_module {
                    FileKeyInner::SourceFile(filename.to_string())
                } else {
                    FileKeyInner::LibFile(filename.to_string())
                });
                let strictness_kind =
                    flow_common::type_strictness::TypeStrictnessKind::from_is_typescript(
                        flow_common::files::has_ts_ext(&file_key),
                    );
                let parsed = parse_file(file_key, content, &options);
                if !parsed.parse_errors.is_empty() {
                    Err(format!("Failed to parse builtin `{}`", filename))
                } else {
                    Ok((is_ts_module, strictness_kind, parsed))
                }
            })
            .collect::<Result<Vec<_>, _>>()
    })?;
    let (ts_modules, libs): (Vec<_>, Vec<_>) = parsed_files
        .into_iter()
        .partition(|(is_ts_module, _, _)| *is_ts_module);
    let asts: Vec<_> = libs
        .into_iter()
        .map(|(_, strictness_kind, parsed)| (strictness_kind, parsed.ast))
        .collect();
    let global_augmentation_asts: Vec<_> = ts_modules
        .into_iter()
        .map(|(_, strictness_kind, parsed)| (strictness_kind, parsed.ast, parsed.file_sig))
        .collect();
    let (_errors, master_cx) = merge::merge_lib_files(
        &builtin_sig_options(),
        Arc::default(),
        &asts,
        &global_augmentation_asts,
    );
    STATE.with(|state| {
        state.borrow_mut().master_cx = Arc::new(master_cx);
    });
    Ok(Value::Null)
}

fn lex_mode_name(mode: LexMode) -> &'static str {
    match mode {
        LexMode::Normal => "normal",
        LexMode::Type => "type",
        LexMode::JsxTag => "jsxTag",
        LexMode::JsxChild => "jsxChild",
        LexMode::Regexp => "regexp",
    }
}

fn loc_json_estree(loc: &Loc) -> Value {
    json!({
        "start": {
            "line": loc.start.line,
            "column": loc.start.column,
        },
        "end": {
            "line": loc.end.line,
            "column": loc.end.column,
        },
    })
}

fn offset_of_position(content: &str, line: i32, column: i32) -> usize {
    let target_line = line.max(1) as usize;
    let target_col = column.max(0) as usize;
    let mut current_line = 1usize;
    let mut current_col = 0usize;
    for (offset, ch) in content.char_indices() {
        if current_line == target_line && current_col == target_col {
            return offset;
        }
        if ch == '\n' {
            current_line += 1;
            current_col = 0;
        } else {
            current_col += ch.len_utf8();
        }
    }
    content.len()
}

fn token_json(content: &str, token: &TokenSinkResult) -> Value {
    let loc = &token.token_loc;
    json!({
        "type": token.token_kind.as_str(),
        "context": lex_mode_name(token.token_context),
        "loc": loc_json_estree(loc),
        "range": [
            offset_of_position(content, loc.start.line, loc.start.column),
            offset_of_position(content, loc.end.line, loc.end.column),
        ],
        "value": token.token_kind.to_value(),
    })
}

fn parse(params: &Value) -> Result<Value, String> {
    let content = string_arg(params, "content")?;
    let source_filename = params
        .get("options")
        .and_then(|options| options.get("sourceFilename"))
        .and_then(Value::as_str);
    let options = params.get("options").unwrap_or(&Value::Null);
    let include_tokens = bool_config(options, "tokens", false);
    let mut tokens: Vec<TokenSinkResult> = Vec::new();
    let mut sink = |token: TokenSinkResult| {
        if !matches!(token.token_kind, TokenKind::TEof) {
            tokens.push(token);
        }
    };
    let (mut ast, parse_errors) = match source_filename {
        Some(filename) => flow_parser::parse_program_file::<()>(
            false,
            include_tokens.then_some(&mut sink),
            Some(PERMISSIVE_PARSE_OPTIONS),
            FileKey::new(FileKeyInner::SourceFile(filename.to_string())),
            Ok(content),
        ),
        None => flow_parser::parse_program_without_file(
            false,
            include_tokens.then_some(&mut sink),
            Some(PERMISSIVE_PARSE_OPTIONS),
            Ok(content),
        ),
    };
    comment_utils::strip_inlined_comments(&mut ast);

    let offset_table = OffsetTable::make(content);
    let mut result = estree_translator::program(
        &offset_table,
        &estree_translator::Config {
            include_locs: true,
            include_filename: true,
            offset_style: estree_translator::OffsetStyle::JsIndices,
        },
        &ast,
    );
    let Value::Object(result_object) = &mut result else {
        return Err("ESTree program serialization did not produce an object".to_string());
    };
    result_object.insert(
        "errors".to_string(),
        estree_translator::errors(
            &offset_table,
            true,
            estree_translator::OffsetStyle::JsIndices,
            &parse_errors,
        ),
    );
    if include_tokens {
        result_object.insert(
            "tokens".to_string(),
            Value::Array(tokens.iter().map(|t| token_json(content, t)).collect()),
        );
    }
    Ok(result)
}

fn loc_value(loc: &Loc) -> Value {
    use flow_common::reason;

    reason::json_of_loc(None, false, None, loc)
}

fn loc_as_range(loc: &Loc, end_inclusive: bool) -> Value {
    json!({
        "startLineNumber": loc.start.line,
        "startColumn": loc.start.column + 1,
        "endLineNumber": loc.end.line,
        "endColumn": loc.end.column + if end_inclusive { 1 } else { 0 },
    })
}

fn get_def(params: &Value) -> Result<Value, String> {
    use flow_services_get_def::get_def_js;
    use flow_services_get_def::get_def_types;

    let filename = string_arg(params, "filename")?;
    let content = string_arg(params, "content")?;
    let line = i32_arg(params, "line")?;
    let col = i32_arg(params, "col")?;
    let config = params.get("config");
    let mut arena = ContextArena::new();
    let mut checked = check_file(
        &mut arena,
        filename,
        content,
        config,
        STATE.with(|state| state.borrow().master_cx.dupe()),
    )?;
    let loc = Loc::cursor(Some(checked.prepared.parsed.file_key.dupe()), line, col);
    let result = get_def_js::get_def(
        &|aloc: &ALoc| aloc.to_loc_exn().dupe(),
        &mut checked.prepared.cx,
        &checked.prepared.parsed.file_sig,
        Some(content),
        checked.prepared.parsed.ast.as_ref(),
        flow_typing_utils::typed_ast_utils::AvailableAst::TypedAst(&checked.typed_ast),
        &get_def_types::Purpose::GoToDefinition,
        &loc,
    )
    .map_err(|_| "get-def failed".to_string())?;
    match result {
        get_def_js::GetDefResult::Def(locs, _) | get_def_js::GetDefResult::Partial(locs, _, _) => {
            Ok(Value::Array(locs.iter().map(loc_value).collect()))
        }
        get_def_js::GetDefResult::BadLoc(msg) | get_def_js::GetDefResult::DefError(msg) => Err(msg),
    }
}

fn completion_item_json(
    item: &flow_services_autocomplete::autocomplete_service_js::ac_completion::CompletionItem,
) -> Value {
    let mut props = serde_json::Map::new();
    props.insert("label".to_string(), Value::String(item.name.clone()));
    if let Some(kind) = item.kind {
        props.insert(
            "kind".to_string(),
            serde_json::to_value(kind).unwrap_or(Value::Null),
        );
    }
    if let Some(detail) = &item.itemDetail {
        props.insert("detail".to_string(), Value::String(detail.clone()));
    }
    if let flow_services_autocomplete::autocomplete_service_js::DocumentationAndTags::Value((
        Some(documentation),
        _,
    )) = &item.documentation_and_tags
    {
        props.insert(
            "documentation".to_string(),
            json!({ "value": documentation }),
        );
    }
    if let Some(edit) = &item.text_edit {
        props.insert(
            "insertText".to_string(),
            Value::String(edit.newText.clone()),
        );
        props.insert(
            "range".to_string(),
            json!({
                "insert": loc_as_range(&edit.insert, false),
                "replace": loc_as_range(&edit.replace, false),
            }),
        );
    }
    props.insert(
        "additionalTextEdits".to_string(),
        Value::Array(
            item.additional_text_edits
                .iter()
                .map(|(loc, text)| {
                    json!({
                        "text": text,
                        "range": loc_as_range(loc, false),
                    })
                })
                .collect(),
        ),
    );
    Value::Object(props)
}

fn autocomplete(params: &Value) -> Result<Value, String> {
    use flow_common::files::FileOptions;
    use flow_parser_utils_output::js_layout_generator;
    use flow_services_autocomplete::autocomplete_js;
    use flow_services_autocomplete::autocomplete_service_js;
    use flow_services_autocomplete::autocomplete_sigil;
    use flow_services_autocomplete::module_system_info::LspModuleSystemInfo;
    use flow_services_export::export_search_types;

    let filename = string_arg(params, "filename")?;
    let content = string_arg(params, "content")?;
    let line = i32_arg(params, "line")?;
    let col = i32_arg(params, "col")?;
    let config = params.get("config");
    let file_key = FileKey::new(FileKeyInner::SourceFile(filename.to_string()));
    let cursor_loc = Loc::cursor(Some(file_key.dupe()), line, col);
    let (content_with_sigil, _, canon_token) =
        autocomplete_sigil::add(Some(&file_key), content, line, col as usize);
    let canon_cursor = canon_token
        .as_ref()
        .map_or_else(|| cursor_loc.dupe(), |token| token.cursor.dupe());
    autocomplete_js::autocomplete_set_hooks(&canon_cursor);
    let result = (|| -> Result<Value, String> {
        let mut arena = ContextArena::new();
        let mut prepared = prepare_file(
            &mut arena,
            filename,
            &content_with_sigil,
            config,
            STATE.with(|state| state.borrow().master_cx.dupe()),
        )?;
        if !prepared.parsed.parse_errors.is_empty() {
            return Err("parse error".to_string());
        }
        let aloc_ast = flow_aloc::loc_to_aloc_ast(prepared.parsed.ast.as_ref());
        type_inference::initialize_env(
            &mut prepared.cx,
            &prepared.type_sig_options,
            None,
            prepared.parsed.ast.as_ref(),
            aloc_ast,
            &prepared.parsed.file_sig,
        )
        .map_err(|_| "autocomplete type inference failed".to_string())?;
        let module_system_info = LspModuleSystemInfo {
            file_options: Arc::<FileOptions>::default(),
            haste_module_system: false,
            get_haste_module_info: Arc::new(|_| None),
            get_package_info: Box::new(|_| None),
            is_package_file: Box::new(|_, _| false),
            node_resolver_root_relative_dirnames: Vec::new(),
            resolves_to_real_path: Box::new(|_, _| false),
        };
        let empty_exports_search_result = export_search_types::SearchResults {
            results: Vec::new(),
            is_incomplete: false,
        };
        let search_values = |_ac_options: &autocomplete_service_js::AcOptions, _name: &str| {
            empty_exports_search_result.clone()
        };
        let search_types = |_ac_options: &autocomplete_service_js::AcOptions, _name: &str| {
            empty_exports_search_result.clone()
        };
        let layout_options = js_layout_generator::default_opts();
        let mut artifacts = autocomplete_service_js::mk_typing_artifacts(
            &layout_options,
            Box::new(|aloc: &ALoc| aloc.to_loc_exn().dupe()),
            Box::new(|_| None),
            &module_system_info,
            &search_values,
            &search_types,
            &mut prepared.cx,
            prepared.parsed.file_sig.dupe(),
            prepared.parsed.ast.dupe(),
            canon_token.as_ref(),
        );
        let (_, _, _, result) = autocomplete_service_js::autocomplete_get_results(
            &mut artifacts,
            &autocomplete_service_js::AcOptions {
                imports: false,
                imports_min_characters: 0,
                imports_ranked_usage: true,
                imports_ranked_usage_boost_exact_match_min_length: 5,
                show_ranking_info: false,
            },
            None,
            &Loc::cursor(Some(file_key), line, col),
        )
        .map_err(|_| "autocomplete failed".to_string())?;
        match result {
            autocomplete_service_js::AutocompleteServiceResultGeneric::AcEmpty(_) => {
                Ok(json!({ "incomplete": false, "suggestions": [] }))
            }
            autocomplete_service_js::AutocompleteServiceResultGeneric::AcFatalError(msg) => {
                Err(msg)
            }
            autocomplete_service_js::AutocompleteServiceResultGeneric::AcResult(result) => {
                Ok(json!({
                    "incomplete": result.result.is_incomplete,
                    "suggestions": result.result.items.iter().map(completion_item_json).collect::<Vec<_>>(),
                }))
            }
        }
    })();
    autocomplete_js::autocomplete_unset_hooks();
    result
}

fn types_to_json(types: Vec<(Loc, String)>) -> Value {
    Value::Array(
        types
            .into_iter()
            .map(|(loc, type_)| {
                json!({
                    "type": type_,
                    "reasons": [],
                    "loc": loc_value(&loc),
                })
            })
            .collect(),
    )
}

fn dump_types(params: &Value, for_tool: bool) -> Result<Value, String> {
    use flow_services_type_info::type_info_service;
    use flow_typing_ty_normalizer::env::EvaluateTypeDestructorsMode;

    let filename = string_arg(params, "filename")?;
    let content = string_arg(params, "content")?;
    let config = params.get("config");
    let mut arena = ContextArena::new();
    let mut checked = check_file(
        &mut arena,
        filename,
        content,
        config,
        STATE.with(|state| state.borrow().master_cx.dupe()),
    )?;
    // No `--dedup` on the wasm surface, so there is never a `$defs` table to emit.
    let (types, _) = type_info_service::dump_types(
        EvaluateTypeDestructorsMode::EvaluateNone,
        for_tool.then_some(5),
        None,
        &mut checked.prepared.cx,
        checked.prepared.parsed.file_sig.dupe(),
        &checked.typed_ast,
    );
    Ok(types_to_json(types))
}

fn semantic_decorations(params: &Value) -> Result<Value, String> {
    let filename = string_arg(params, "filename")?;
    let content = string_arg(params, "content")?;
    let config = params.get("config");
    let mut arena = ContextArena::new();
    let checked = check_file(
        &mut arena,
        filename,
        content,
        config,
        STATE.with(|state| state.borrow().master_cx.dupe()),
    )?;
    let refined = checked.prepared.cx.refined_locations();
    let decorations = refined
        .iter()
        .map(|(aloc, _)| {
            let loc = aloc.to_loc_exn().dupe();
            json!({
                "kind": "refined-value",
                "range": loc_as_range(&loc, true),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({ "decorations": decorations }))
}

fn signature_to_json(
    signature: &flow_server_env::server_prot::response::FuncDetailsResult,
) -> Value {
    use flow_server_env::server_prot;

    match signature {
        server_prot::response::FuncDetailsResult::SigHelpFunc {
            func_documentation,
            param_tys,
            return_ty,
        } => {
            let parameters = param_tys
                .iter()
                .map(|param| {
                    let mut props = serde_json::Map::new();
                    props.insert(
                        "label".to_string(),
                        Value::String(format!("{}: {}", param.param_name, param.param_ty)),
                    );
                    if let Some(doc) = &param.param_documentation {
                        props.insert("documentation".to_string(), json!({ "value": doc }));
                    }
                    Value::Object(props)
                })
                .collect::<Vec<_>>();
            let label = format!(
                "({}): {}",
                param_tys
                    .iter()
                    .map(|param| format!("{}: {}", param.param_name, param.param_ty))
                    .collect::<Vec<_>>()
                    .join(", "),
                return_ty
            );
            let mut props = serde_json::Map::new();
            props.insert("label".to_string(), Value::String(label));
            props.insert("parameters".to_string(), Value::Array(parameters));
            if let Some(doc) = func_documentation {
                props.insert("documentation".to_string(), json!({ "value": doc }));
            }
            Value::Object(props)
        }
        server_prot::response::FuncDetailsResult::SigHelpJsxAttr {
            documentation,
            name,
            ty,
            optional,
        } => {
            let label = format!("{}{}: {}", name, if *optional { "?" } else { "" }, ty);
            let mut param = serde_json::Map::new();
            param.insert("label".to_string(), Value::String(label.clone()));
            if let Some(doc) = documentation {
                param.insert("documentation".to_string(), json!({ "value": doc }));
            }
            json!({
                "label": label,
                "parameters": [Value::Object(param)],
            })
        }
    }
}

fn signature_help(params: &Value) -> Result<Value, String> {
    use flow_services_type_info::signature_help;

    let filename = string_arg(params, "filename")?;
    let content = string_arg(params, "content")?;
    let line = i32_arg(params, "line")?;
    let col = i32_arg(params, "col")?;
    let config = params.get("config");
    let mut arena = ContextArena::new();
    let mut checked = check_file(
        &mut arena,
        filename,
        content,
        config,
        STATE.with(|state| state.borrow().master_cx.dupe()),
    )?;
    let cursor = Loc::cursor(Some(checked.prepared.parsed.file_key.dupe()), line, col);
    let details = signature_help::find_signatures(
        &|aloc: &ALoc| aloc.to_loc_exn().dupe(),
        &|_| None,
        &mut checked.prepared.cx,
        checked.prepared.parsed.file_sig.dupe(),
        checked.prepared.parsed.ast.as_ref(),
        &checked.typed_ast,
        cursor,
    )
    .map_err(|_| "signature help failed".to_string())?
    .map_err(|_| "Failed to normalize type".to_string())?;
    match details {
        None => Ok(Value::Null),
        Some((signatures, active_parameter)) => Ok(json!({
            "signatures": signatures.iter().map(signature_to_json).collect::<Vec<_>>(),
            "activeParameter": active_parameter,
            "activeSignature": 0,
        })),
    }
}

fn type_at_pos(params: &Value) -> Result<Value, String> {
    use flow_common_ty::ty_printer;
    use flow_services_type_info::type_info_service;

    let filename = string_arg(params, "filename")?;
    let content = string_arg(params, "content")?;
    let line = i32_arg(params, "line")?;
    let col = i32_arg(params, "col")?;
    let config = params.get("config");
    let mut arena = ContextArena::new();
    let mut checked = check_file(
        &mut arena,
        filename,
        content,
        config,
        STATE.with(|state| state.borrow().master_cx.dupe()),
    )?;
    let loc_of_aloc = |aloc: &ALoc| aloc.to_loc_exn().dupe();
    let ((_loc, tys, _refining, _invalidated), json_data) = type_info_service::type_at_pos(
        &mut checked.prepared.cx,
        checked.prepared.parsed.file_sig.dupe(),
        &checked.typed_ast,
        false,
        40,
        false,
        false,
        None,
        None,
        Some(&loc_of_aloc),
        checked.prepared.parsed.file_key.dupe(),
        line,
        col,
    )
    .map_err(|_| "type-at-pos failed".to_string())?;
    // Print like the LSP hover path (`infer_type_to_response`): frame the type
    // as the declaration it resolves to (`const x: number`, `function f(): T`).
    if let Some(result) = tys {
        let printer_opts = ty_printer::PrinterOptions {
            size: 100,
            ..Default::default()
        };
        let (type_str, _refs) = ty_printer::string_of_type_at_pos_result(
            ty_printer::TypeAtPosPrint {
                ty: &result.ty,
                refs: result.refs.as_ref(),
                binder: result.binder.as_ref(),
                alias: result.alias.as_ref(),
            },
            &loc_of_aloc,
            &printer_opts,
        );
        return Ok(json!([{ "type": "flow", "value": type_str }]));
    }
    let data = Value::Object(json_data.into_iter().collect());
    match data.get("type").and_then(Value::as_str) {
        Some(type_) => Ok(json!([{ "type": "flow", "value": type_ }])),
        None => Ok(Value::Null),
    }
}

#[derive(Deserialize)]
struct Request {
    method: String,
    #[serde(default)]
    params: Value,
}

fn handle_request(input: &str) -> Result<Value, String> {
    ensure_roots();
    let request: Request = serde_json::from_str(input).map_err(|err| err.to_string())?;
    match request.method.as_str() {
        "registerFile" => register_file(&request.params),
        "initBuiltins" => init_builtins(&request.params),
        "check" => check_registered(&request.params),
        "checkContent" => check_content(&request.params),
        "parse" => parse(&request.params),
        "autocomplete" => autocomplete(&request.params),
        "getDef" => get_def(&request.params),
        "dumpTypes" => dump_types(&request.params, false),
        "dumpTypesForTool" => dump_types(&request.params, true),
        "semanticDecorations" => semantic_decorations(&request.params),
        "signatureHelp" => signature_help(&request.params),
        "typeAtPos" => type_at_pos(&request.params),
        "flowVersion" => Ok(Value::String(
            flow_common::flow_version::version().to_string(),
        )),
        other => Err(format!("Unknown flow.js wasm method `{}`", other)),
    }
}

fn response_json(result: Result<Value, String>) -> String {
    let value = match result {
        Ok(value) => json!({ "ok": true, "value": value }),
        Err(error) => json!({ "ok": false, "error": error }),
    };
    serde_json::to_string(&value).unwrap_or_else(|err| {
        format!(
            "{{\"ok\":false,\"error\":\"failed to serialize response: {}\"}}",
            err
        )
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn flowDotJsAlloc(size: usize) -> *mut u8 {
    let Ok(layout) = Layout::array::<u8>(size) else {
        return std::ptr::null_mut();
    };
    // SAFETY: `layout` is a valid `u8` array layout. The returned pointer is
    // freed by `flowDotJsFree` with the same layout.
    unsafe { std::alloc::alloc(layout) }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowDotJsFree(ptr: *mut u8, size: usize) {
    if ptr.is_null() {
        return;
    }
    let Ok(layout) = Layout::array::<u8>(size) else {
        return;
    };
    // SAFETY: `ptr` must be a pointer returned by `flowDotJsAlloc(size)` and not
    // previously freed. The JS wrapper frees exactly that pointer and size.
    unsafe {
        std::alloc::dealloc(ptr, layout);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowDotJsStringFree(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: `ptr` must be a non-null pointer returned by `CString::into_raw`
    // in `flowDotJsCall`. Reconstructing the CString transfers ownership back
    // to Rust and drops it with the same allocator.
    unsafe {
        drop(CString::from_raw(ptr));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowDotJsCall(ptr: *const u8, len: usize) -> *mut c_char {
    let input = if ptr.is_null() {
        Err("null request pointer".to_string())
    } else {
        // SAFETY: The JS wrapper obtains `ptr` from `flowDotJsAlloc`, writes
        // exactly `len` initialized bytes into the allocation, and keeps it
        // alive until this function returns. This call only reads the slice.
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        std::str::from_utf8(bytes)
            .map_err(|err| err.to_string())
            .and_then(handle_request)
    };
    let response = response_json(input);
    CString::new(response).unwrap().into_raw()
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn main(op: usize, arg1: usize, arg2: usize) -> usize {
    match op {
        0 => flowDotJsAlloc(arg1) as usize,
        1 => {
            flowDotJsFree(arg1 as *mut u8, arg2);
            0
        }
        2 => {
            flowDotJsStringFree(arg1 as *mut c_char);
            0
        }
        3 => {
            let response = flowDotJsCall(arg1 as *const u8, arg2);
            flowDotJsFree(arg1 as *mut u8, arg2);
            response as usize
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_content_value(content: &str) -> Value {
        check_content_value_with_config(content, json!({}))
    }

    fn check_content_value_with_config(content: &str, config: Value) -> Value {
        ensure_roots();
        check_content(&json!({
            "filename": "test.js",
            "content": content,
            "config": config,
        }))
        .expect("checkContent should succeed")
    }

    fn diagnostics(value: &Value) -> &Vec<Value> {
        value
            .as_array()
            .expect("checkContent should return a diagnostics array")
    }

    fn level_is(diagnostic: &Value, level: &str) -> bool {
        diagnostic.get("level").and_then(Value::as_str) == Some(level)
    }

    #[test]
    fn check_content_filters_disabled_lints() {
        let value = check_content_value("type T = {foo: string};\n");

        assert!(
            diagnostics(&value).is_empty(),
            "disabled lint should not be reported: {value}"
        );
    }

    #[test]
    fn check_content_preserves_lint_warnings() {
        let value = check_content_value(
            "// flowlint ambiguous-object-type:warn\n type T = {foo: string};\n",
        );

        assert!(
            diagnostics(&value)
                .iter()
                .all(|diagnostic| !level_is(diagnostic, "error")),
            "warning lint should not be reported as an error: {value}"
        );
        assert!(
            diagnostics(&value)
                .iter()
                .any(|diagnostic| level_is(diagnostic, "warning")),
            "warning lint should be reported as a warning: {value}"
        );
        assert!(
            serde_json::to_string(diagnostics(&value))
                .expect("warnings should serialize")
                .contains("ambiguous-object-type"),
            "warning lint should be reported as a warning: {value}"
        );
    }

    fn type_at_pos_value(content: &str, line: i32, col: i32) -> Value {
        ensure_roots();
        type_at_pos(&json!({
            "filename": "test.js",
            "content": content,
            "line": line,
            "col": col,
            "config": {},
        }))
        .expect("typeAtPos should succeed")
    }

    fn hover_flow_value(value: &Value) -> &str {
        value
            .as_array()
            .expect("typeAtPos should return hover blocks")[0]
            .get("value")
            .and_then(Value::as_str)
            .expect("hover block should have a string value")
    }

    #[test]
    fn type_at_pos_frames_value_binding_as_declaration() {
        let value = type_at_pos_value("const foo: number = 42;\nfoo;\n", 2, 1);

        assert_eq!(hover_flow_value(&value), "const foo: number");
    }

    #[test]
    fn type_at_pos_frames_function_as_signature() {
        let value = type_at_pos_value("function fred(): number { return 1; }\nfred;\n", 2, 1);

        assert_eq!(hover_flow_value(&value), "function fred(): number");
    }

    #[test]
    fn check_content_supports_assertion_functions() {
        let value = check_content_value_with_config(
            "declare function assertString(value: unknown): asserts value is string;\n\
             declare const value: unknown;\n\
             assertString(value);\n\
             value as string;\n",
            json!({}),
        );

        assert!(
            diagnostics(&value).is_empty(),
            "assertion function should refine its argument: {value}"
        );
    }
}
