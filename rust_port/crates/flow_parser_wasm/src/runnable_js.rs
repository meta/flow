// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! Native Flow runtime emission shared with the Wasm entry point.

use dupe::Dupe;
use dupe::OptionDupedExt;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ParseOptions;
use flow_parser::ast;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::FileKeyInner;
use flow_parser::loc::Loc;
use flow_parser::loc::Position;
use flow_parser::offset_utils::OffsetTable;
use flow_parser_utils_output::indented_printer;
use flow_parser_utils_output::source_map::SourceMap;
use flow_parser_utils_output::source_map::SourceMapError;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

use crate::babel_adapter;
use crate::babel_adapter::BabelLoweringError;
use crate::babel_adapter::BabelLoweringOptions;
use crate::babel_adapter::EnumRuntime;
pub use crate::babel_adapter::ReactRuntimeTarget;
use crate::strip_flow;

/// The module system used to load the emitted code and enum runtime.
#[derive(Debug, Clone, Copy, Default)]
pub enum SourceType {
    #[default]
    CommonJs,
    Module,
}

/// Runtime lowering options. Flow runtime syntax and type erasure are enabled.
#[derive(Debug, Default)]
pub struct RunnableOptions {
    pub source_type: SourceType,
    /// A require path or import specifier; defaults to `flow-enums-runtime`.
    pub enum_runtime_module: Option<FlowSmolStr>,
    pub react_runtime_target: ReactRuntimeTarget,
    pub allow_return_outside_function: bool,
}

/// JavaScript and its source-map v3 document for the supplied source filename.
#[derive(Debug, Serialize)]
pub struct RunnableJs {
    pub code: String,
    pub map: SourceMap,
}

/// A source diagnostic uses one-based lines and zero-based UTF-16 columns.
#[derive(Debug, Error)]
pub enum RunnableError {
    #[error("{message}")]
    Syntax { message: String, loc: Position },
    #[error("Invalid emission options: {0}")]
    Options(#[from] serde_json::Error),
    #[error("Module parser rolled back without a diagnostic")]
    ParserRollback,
    #[error(transparent)]
    SourceMap(#[from] SourceMapError),
    #[error(transparent)]
    Printer(#[from] indented_printer::PrintError),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceMetadata {
    pub source_filename: String,
    pub enum_runtime_module: Option<FlowSmolStr>,
}

fn syntax_error(
    source: &str,
    location: &Loc,
    message: String,
) -> Result<RunnableError, SourceMapError> {
    let loc = OffsetTable::make(source)
        .convert_flow_position_to_js_position(location.start)
        .map_err(|error| SourceMapError::InvalidPosition(error.position))?;
    Ok(RunnableError::Syntax { message, loc })
}

/// Parse Flow source, lower its runtime constructs, and erase types.
pub fn flow_to_runnable_program(
    source: &str,
    source_filename: &str,
    options: &RunnableOptions,
) -> Result<ast::Program<Loc, Loc>, RunnableError> {
    let file = FileKey::new(FileKeyInner::SourceFile(source_filename.to_owned()));
    let parse_options = ParseOptions {
        components: true,
        enums: true,
        pattern_matching: true,
        records: true,
        esproposal_decorators: false,
        types: true,
        ambiguous_types: true,
        enable_types_in_comments: false,
        use_strict: matches!(options.source_type, SourceType::Module),
        module_ref_prefix: None,
        ambient: false,
        allow_return_outside_function: options.allow_return_outside_function,
    };
    let (program, errors) = match options.source_type {
        SourceType::CommonJs => flow_parser::parse_program_file::<()>(
            false,
            None,
            Some(parse_options),
            file.dupe(),
            Ok(source),
        ),
        SourceType::Module => {
            match flow_parser::parse_module_file(parse_options, file.dupe(), source) {
                Ok(program) => (program, Vec::new()),
                Err(errors) => {
                    let Some((location, error)) = errors.into_iter().next() else {
                        return Err(RunnableError::ParserRollback);
                    };
                    return Err(syntax_error(source, &location, error.to_string())?);
                }
            }
        }
    };
    if let Some((location, error)) = errors.into_iter().next() {
        return Err(syntax_error(source, &location, error.to_string())?);
    }
    let enum_module = options
        .enum_runtime_module
        .as_ref()
        .duped()
        .unwrap_or_else(|| "flow-enums-runtime".into());
    match babel_adapter::lower_program(
        source,
        &program,
        &BabelLoweringOptions {
            lower_enums: true,
            enum_runtime: match options.source_type {
                SourceType::CommonJs => EnumRuntime::Require(enum_module),
                SourceType::Module => EnumRuntime::Import(enum_module),
            },
            react_runtime_target: options.react_runtime_target,
        },
    ) {
        Ok(lowered) => Ok(strip_flow::lower_program(&lowered.program)),
        Err(BabelLoweringError::Syntax { message, loc }) => {
            Err(syntax_error(source, &loc, message)?)
        }
    }
}

/// Parse Flow source, lower its runtime constructs, erase types, and emit
/// indented JavaScript with a source map. Both outputs retain `source_filename`.
pub fn flow_to_runnable_js_with_source_map(
    source: &str,
    source_filename: &str,
    options: &RunnableOptions,
) -> Result<RunnableJs, RunnableError> {
    let program = flow_to_runnable_program(source, source_filename, options)?;
    let file = FileKey::new(FileKeyInner::SourceFile(source_filename.to_owned()));
    let (code, map) = indented_printer::print_with_source_map(&program, &file, source)?;
    Ok(RunnableJs {
        code: code.contents(),
        map,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn erases_types_and_emits_a_map_for_the_supplied_source() {
        let source = "type N = number;\nconst value: N = 1;\nthrow value;";
        let emitted =
            flow_to_runnable_js_with_source_map(source, "original.js", &RunnableOptions::default())
                .unwrap();
        assert_eq!(emitted.code, "const value = 1;\nthrow (value);\n");
        let map = serde_json::to_value(emitted.map).unwrap();
        assert_eq!(map["sources"], json!(["original.js"]));
        assert_eq!(map["sourcesContent"], json!([source]));
        assert_eq!(map["names"], json!(["value"]));
        assert_ne!(map["mappings"], "");
    }

    #[test]
    fn esm_enums_share_an_import_that_avoids_later_user_bindings() {
        let source = "enum E {A}\nenum F {B}\nconst $$gen$enum_runtime0 = 1;";
        let emitted = flow_to_runnable_js_with_source_map(
            source,
            "original.mjs",
            &RunnableOptions {
                source_type: SourceType::Module,
                ..RunnableOptions::default()
            },
        )
        .unwrap();
        assert!(
            emitted
                .code
                .starts_with("import $$gen$enum_runtime1 from \"flow-enums-runtime\";\n")
        );
        assert_eq!(emitted.code.matches("flow-enums-runtime").count(), 1);
        assert_eq!(emitted.code.matches(".Mirrored").count(), 2);
        assert!(emitted.code.contains("const $$gen$enum_runtime0 = 1;"));
        let no_enum = flow_to_runnable_js_with_source_map(
            "export const x: number = 1;",
            "plain.mjs",
            &RunnableOptions {
                source_type: SourceType::Module,
                ..RunnableOptions::default()
            },
        )
        .unwrap();
        assert!(!no_enum.code.contains("flow-enums-runtime"));
    }

    #[test]
    fn module_context_allows_await_and_retains_function_and_strict_rules() {
        let options = RunnableOptions {
            source_type: SourceType::Module,
            ..RunnableOptions::default()
        };
        let emitted = flow_to_runnable_js_with_source_map(
            "export const x: number = await Promise.resolve(42); for await (const n of []) {}",
            "await.mjs",
            &options,
        )
        .unwrap();
        assert!(emitted.code.contains("await ("));
        assert!(
            flow_parser::parse_module_file(
                ParseOptions::default(),
                FileKey::new(FileKeyInner::SourceFile("await.mjs".to_owned())),
                &emitted.code,
            )
            .is_ok()
        );
        assert!(
            flow_to_runnable_js_with_source_map("function f() { await x; }", "bad.mjs", &options)
                .is_err()
        );
        assert!(flow_to_runnable_js_with_source_map("with (x) {}", "bad.mjs", &options).is_err());
        assert!(
            flow_to_runnable_js_with_source_map("await x;", "bad.cjs", &RunnableOptions::default())
                .is_err()
        );
    }

    #[test]
    fn enum_runtime_specifiers_are_printed_and_mapped_with_the_program() {
        let module: FlowSmolStr = "file:///tmp/runtime\"with-quote.js".into();
        let emitted = flow_to_runnable_js_with_source_map(
            "enum E {A}",
            "enum.mjs",
            &RunnableOptions {
                source_type: SourceType::Module,
                enum_runtime_module: Some(module.dupe()),
                ..RunnableOptions::default()
            },
        )
        .unwrap();
        let (program, errors) = flow_parser::parse_program_file::<()>(
            false,
            None,
            None,
            FileKey::new(FileKeyInner::SourceFile("enum.mjs".to_owned())),
            Ok(&emitted.code),
        );
        assert!(errors.is_empty());
        let flow_parser::ast::statement::StatementInner::ImportDeclaration { inner, .. } =
            &*program.statements[0]
        else {
            panic!("expected runtime import");
        };
        assert_eq!(inner.source.1.value, module);
        let emitted = flow_to_runnable_js_with_source_map(
            "enum E {A}",
            "enum.cjs",
            &RunnableOptions {
                enum_runtime_module: Some(module),
                ..RunnableOptions::default()
            },
        )
        .unwrap();
        assert!(emitted.code.contains("(require)("));
        assert!(!emitted.code.contains("flow-enums-runtime"));
    }

    #[test]
    fn preserves_the_interpreter_and_unicode_source_contents() {
        let source = "#!/usr/bin/env node\r\nconst text: string = '😀';";
        let emitted =
            flow_to_runnable_js_with_source_map(source, "unicode.js", &RunnableOptions::default())
                .unwrap();
        assert!(emitted.code.starts_with("#!/usr/bin/env node\n"));
        assert_eq!(
            serde_json::to_value(emitted.map).unwrap()["sourcesContent"],
            json!([source])
        );
    }

    #[test]
    fn syntax_diagnostics_use_utf16_columns() {
        assert!(matches!(
            flow_to_runnable_js_with_source_map(
                "const text = '😀'; const x = ;",
                "bad.js",
                &RunnableOptions::default()
            ),
            Err(RunnableError::Syntax {
                loc: Position {
                    line: 1,
                    column: 29
                },
                ..
            })
        ));
    }

    #[test]
    fn return_outside_function_is_an_explicit_option() {
        assert!(matches!(
            flow_to_runnable_js_with_source_map(
                "return 42;",
                "script.js",
                &RunnableOptions::default()
            ),
            Err(RunnableError::Syntax { .. })
        ));
        let emitted = flow_to_runnable_js_with_source_map(
            "return 42;",
            "script.js",
            &RunnableOptions {
                allow_return_outside_function: true,
                ..RunnableOptions::default()
            },
        )
        .unwrap();
        assert_eq!(emitted.code, "return (42);\n");
    }
}
