/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::path::PathBuf;

use flow_common::refinement_invalidation;
use flow_common::verbose::Verbose;
use flow_common_errors::error_utils::ConcreteLocPrintableErrorSet;
use flow_common_errors::error_utils::PrintableError;
use flow_parser_utils_output::replacement_printer::Patch;

pub mod infer_type_options {
    use std::path::PathBuf;

    use flow_common::verbose::Verbose;
    use flow_server_utils::file_input::FileInput;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub struct T {
        pub input: FileInput,
        pub line: i32,
        pub r#char: i32,
        pub verbose: Option<Verbose>,
        pub omit_targ_defaults: bool,
        pub wait_for_recheck: Option<bool>,
        pub verbose_normalizer: bool,
        pub max_depth: i32,
        pub json: bool,
        pub strip_root: Option<PathBuf>,
        pub expanded: bool,
        pub debug_print_internal_repr: bool,
        pub no_typed_ast_for_imports: bool,
    }
}

pub mod inlay_hint_options {
    use flow_common::verbose::Verbose;
    use flow_server_utils::file_input::FileInput;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub struct T {
        pub input: FileInput,
        pub verbose: Option<Verbose>,
        pub omit_targ_defaults: bool,
        pub wait_for_recheck: Option<bool>,
        pub verbose_normalizer: bool,
        pub max_depth: i32,
        pub no_typed_ast_for_imports: bool,
    }
}

pub mod inspect_types_options {
    use flow_server_utils::file_input::FileInput;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub struct T {
        pub input: FileInput,
        pub wait_for_recheck: Option<bool>,
    }
}

pub mod type_of_name_options {
    use std::path::PathBuf;

    use flow_common::verbose::Verbose;
    use flow_server_utils::file_input::FileInput;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub struct T {
        pub input: FileInput,
        pub names: Vec<String>,
        pub verbose: Option<Verbose>,
        pub wait_for_recheck: Option<bool>,
        pub expand_component_props: bool,
        pub exact_match_only: bool,
        pub strip_root: Option<PathBuf>,
    }
}

pub mod code_action {
    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub enum T {
        Quickfix { include_best_effort_fix: bool },
        SourceAddMissingImports,
        SuggestImports,
    }

    impl T {
        pub fn to_string(&self) -> &'static str {
            match self {
                T::Quickfix {
                    include_best_effort_fix: false,
                } => "quickfix.safe",
                T::Quickfix {
                    include_best_effort_fix: true,
                } => "quickfix.include_best_effort_fix",
                T::SourceAddMissingImports => "source.addMissingImports",
                T::SuggestImports => "suggestImports",
            }
        }
    }
}

pub mod request {
    use flow_parser::loc::Loc;
    use flow_server_utils::file_input::FileInput;

    use crate::flow_clock::Since;
    use crate::server_prot::PathBuf;
    use crate::server_prot::code_action;
    use crate::server_prot::infer_type_options;
    use crate::server_prot::inlay_hint_options;
    use crate::server_prot::inspect_types_options;
    use crate::server_prot::type_of_name_options;

    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum QueryField {
        #[serde(rename = "name")]
        Name,
        #[serde(rename = "exists")]
        Exists,
        #[serde(rename = "new")]
        New,
        #[serde(rename = "flow.content_hash")]
        ContentHash,
    }

    impl QueryField {
        pub fn as_key(self) -> &'static str {
            match self {
                QueryField::Name => "name",
                QueryField::Exists => "exists",
                QueryField::New => "new",
                QueryField::ContentHash => "flow.content_hash",
            }
        }
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Query {
        pub fields: Vec<QueryField>,
        /// A clock from a prior response.
        #[serde(default)]
        pub since: Option<Since>,
        /// Return no files when the response is a fresh instance.
        #[serde(default)]
        pub empty_on_fresh_instance: bool,
    }

    #[allow(non_camel_case_types)]
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub enum Command {
        APPLY_CODE_ACTION {
            input: FileInput,
            action: code_action::T,
            wait_for_recheck: Option<bool>,
        },
        AUTOCOMPLETE {
            input: FileInput,
            cursor: (i32, i32),
            trigger_character: Option<String>,
            wait_for_recheck: Option<bool>,
            imports: bool,
            imports_ranked_usage: bool,
            show_ranking_info: bool,
        },
        AUTOFIX_EXPORTS {
            input: FileInput,
            verbose: Option<crate::server_prot::Verbose>,
            wait_for_recheck: Option<bool>,
        },
        AUTOFIX_MISSING_LOCAL_ANNOT {
            input: FileInput,
            verbose: Option<crate::server_prot::Verbose>,
            wait_for_recheck: Option<bool>,
        },
        CHECK_FILE {
            input: FileInput,
            verbose: Option<crate::server_prot::Verbose>,
            force: bool,
            include_warnings: bool,
            wait_for_recheck: Option<bool>,
        },
        COVERAGE {
            input: FileInput,
            force: bool,
            wait_for_recheck: Option<bool>,
        },
        BATCH_COVERAGE {
            batch: Vec<String>,
            wait_for_recheck: Option<bool>,
        },
        CYCLE {
            filename: String,
            types_only: bool,
        },
        QUERY {
            query: Query,
        },
        DUMP_TYPES {
            input: FileInput,
            evaluate_type_destructors: bool,
            for_tool: Option<i32>,
            /// When set, `for_tool` output replaces repeated subtrees larger
            /// than this many bytes with `{"$ref": id}` and returns a shared
            /// `$defs` table alongside the per-location types.
            dedup_threshold: Option<usize>,
            wait_for_recheck: Option<bool>,
        },
        FIND_MODULE {
            moduleref: String,
            filename: String,
            wait_for_recheck: Option<bool>,
        },
        FORCE_RECHECK {
            files: Vec<String>,
            focus: bool,
            missed_changes: bool,
            changed_mergebase: bool,
        },
        GET_DEF {
            input: FileInput,
            line: i32,
            r#char: i32,
            wait_for_recheck: Option<bool>,
        },
        GRAPH_DEP_GRAPH {
            root: String,
            strip_root: bool,
            outfile: String,
            types_only: bool,
        },
        INFER_TYPE(infer_type_options::T),
        INLAY_HINT(inlay_hint_options::T),
        INSPECT_TYPES(inspect_types_options::T),
        TYPE_OF_NAME(type_of_name_options::T),
        INSERT_TYPE {
            input: FileInput,
            target: Loc,
            verbose: Option<crate::server_prot::Verbose>,
            location_is_strict: bool,
            wait_for_recheck: Option<bool>,
            omit_targ_defaults: bool,
        },
        RAGE {
            files: Vec<String>,
        },
        SAVE_STATE {
            out: SaveStateOut,
        },
        STATUS {
            include_warnings: bool,
        },
        // Appended last so the preceding shared variants' bincode indices stay stable.
        #[cfg(fbcode_build)]
        FOX(flow_facebook_fox_protocol::FoxCommand),
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub enum SaveStateOut {
        File(PathBuf),
        Scm,
    }

    pub fn to_string(command: &Command) -> String {
        match command {
            Command::APPLY_CODE_ACTION {
                input,
                action,
                wait_for_recheck: _,
            } => format!(
                "apply-code-action source.addMissingImports {} {}",
                input.filename_of_file_input(),
                action.to_string(),
            ),
            Command::AUTOCOMPLETE {
                input,
                cursor: _,
                wait_for_recheck: _,
                trigger_character: _,
                imports: _,
                imports_ranked_usage: _,
                show_ranking_info: _,
            } => format!("autocomplete {}", input.filename_of_file_input()),
            Command::AUTOFIX_EXPORTS { input, .. } => {
                format!("autofix exports {}", input.filename_of_file_input())
            }
            Command::AUTOFIX_MISSING_LOCAL_ANNOT { input, .. } => {
                format!(
                    "autofix missing-local-annot {}",
                    input.filename_of_file_input()
                )
            }
            Command::CHECK_FILE {
                input,
                verbose: _,
                force: _,
                include_warnings: _,
                wait_for_recheck: _,
            } => format!("check {}", input.filename_of_file_input()),
            Command::BATCH_COVERAGE {
                batch: _,
                wait_for_recheck: _,
            } => "batch-coverage".to_string(),
            Command::COVERAGE {
                input,
                force: _,
                wait_for_recheck: _,
            } => format!("coverage {}", input.filename_of_file_input()),
            Command::CYCLE {
                filename,
                types_only,
            } => format!("cycle (types_only: {}) {}", types_only, filename),
            Command::QUERY { query: _ } => "query".to_string(),
            Command::GRAPH_DEP_GRAPH { .. } => "dep-graph".to_string(),
            Command::DUMP_TYPES {
                input,
                evaluate_type_destructors: _,
                wait_for_recheck: _,
                for_tool: _,
                dedup_threshold: _,
            } => format!("dump-types {}", input.filename_of_file_input()),
            Command::FIND_MODULE {
                moduleref,
                filename,
                wait_for_recheck: _,
            } => format!("find-module {} {}", moduleref, filename),
            Command::FORCE_RECHECK {
                files,
                focus,
                missed_changes,
                changed_mergebase,
            } => {
                let mut parts = Vec::new();
                if *focus {
                    parts.push(format!("focus = {}", focus));
                }
                if *missed_changes {
                    parts.push(format!("missed_changes = {}", missed_changes));
                }
                if *changed_mergebase {
                    parts.push(format!("changed_mergebase = {}", changed_mergebase));
                }
                format!("force-recheck {} ({})", files.join(" "), parts.join("; "))
            }
            Command::GET_DEF {
                input,
                line,
                r#char,
                wait_for_recheck: _,
            } => format!(
                "get-def {}:{}:{}",
                input.filename_of_file_input(),
                line,
                r#char
            ),
            Command::INFER_TYPE(infer_type_options::T {
                input,
                line,
                r#char,
                ..
            }) => format!(
                "type-at-pos {}:{}:{}",
                input.filename_of_file_input(),
                line,
                r#char
            ),
            Command::INLAY_HINT(inlay_hint_options::T { input, .. }) => {
                format!("inlay-hint {}", input.filename_of_file_input())
            }
            Command::INSPECT_TYPES(inspect_types_options::T { input, .. }) => {
                format!("inspect-types {}", input.filename_of_file_input())
            }
            Command::TYPE_OF_NAME(type_of_name_options::T { input, names, .. }) => format!(
                "type-of-name {} {}",
                input.filename_of_file_input(),
                names.join(" "),
            ),
            Command::INSERT_TYPE { input, target, .. } => format!(
                "autofix insert-type {}:{}:{}-{}:{}",
                input.filename_of_file_input(),
                target.start.line,
                target.start.column,
                target.end.line,
                target.end.column,
            ),
            Command::RAGE { files } => format!("rage {}", files.join(" ")),
            Command::STATUS {
                include_warnings: _,
            } => "status".to_string(),
            Command::SAVE_STATE { out } => {
                let out = match out {
                    SaveStateOut::Scm => "--scm".to_string(),
                    SaveStateOut::File(file) => file.display().to_string(),
                };
                format!("save-state {}", out)
            }
            #[cfg(fbcode_build)]
            Command::FOX(_) => "fox".to_string(),
        }
    }
}

pub mod response {
    use std::collections::BTreeSet;

    use flow_parser::file_key::FileKey;
    use flow_parser::loc::Loc;
    use flow_services_coverage::FileCoverage;
    use flow_services_coverage::Kind as CoverageKind;
    pub use lsp_types::CodeActionOrCommand;

    use crate::server_prot::ConcreteLocPrintableErrorSet;
    use crate::server_prot::Patch;
    use crate::server_prot::PrintableError;

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub struct LazyStats {
        pub lazy_mode: bool,
        pub checked_files: i32,
        pub total_files: i32,
    }

    // Details about functions to be added in json output
    #[derive(serde::Serialize, serde::Deserialize)]
    pub struct FuncParamResult {
        pub param_documentation: Option<String>,
        pub param_name: String,
        pub param_ty: String,
    }

    #[derive(serde::Serialize, serde::Deserialize)]
    pub enum FuncDetailsResult {
        SigHelpFunc {
            func_documentation: Option<String>,
            param_tys: Vec<FuncParamResult>,
            return_ty: String,
        },
        SigHelpJsxAttr {
            documentation: Option<String>,
            name: String,
            ty: String,
            optional: bool,
        },
    }

    pub type TextEdit = (Loc, String);

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    #[allow(non_snake_case)]
    pub struct InsertReplaceEdit {
        pub newText: String,
        pub insert: Loc,
        pub replace: Loc,
    }

    pub mod completion {
        use lsp_types::CompletionItemKind;
        use lsp_types::CompletionItemTag;
        use lsp_types::InsertTextFormat;

        use crate::server_prot::response::InsertReplaceEdit;
        use crate::server_prot::response::TextEdit;

        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[allow(non_snake_case)]
        pub struct CompletionItem {
            pub kind: Option<CompletionItemKind>,
            pub name: String,
            pub labelDetail: Option<String>,
            pub description: Option<String>,
            pub itemDetail: Option<String>,
            pub text_edit: Option<InsertReplaceEdit>,
            pub additional_text_edits: Vec<TextEdit>,
            pub sort_text: Option<String>,
            pub preselect: bool,
            pub documentation: Option<String>,
            pub tags: Option<Vec<CompletionItemTag>>,
            pub log_info: String,
            pub insert_text_format: InsertTextFormat,
        }

        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[allow(non_snake_case)]
        pub struct T {
            pub items: Vec<CompletionItem>,
            pub is_incomplete: bool,
        }
    }

    // Which "type" of autocomplete this was. e.g. identifier vs type vs member.
    // See AutocompleteService_js.string_of_autocomplete_type
    pub type AcType = String;

    pub type AutocompleteResponse = Result<(completion::T, AcType), String>;

    pub type ApplyCodeActionResponse = Result<Patch, String>;

    pub type AutofixExportsResponse = Result<(Patch, Vec<String>), String>;

    pub type AutofixMissingLocalAnnotResponse = Result<Patch, String>;

    pub type CoverageResponse = Result<Vec<(Loc, CoverageKind)>, String>;

    pub type BatchCoverageResponse = Result<Vec<(FileKey, FileCoverage)>, String>;

    /// The per-location types and, for a `--dedup` dump, the serialized `$defs` object they
    /// reference. The table lives inside the `Ok` so a failed dump cannot carry one.
    pub type DumpTypesResponse = Result<(Vec<(Loc, String)>, Option<String>), String>;

    pub type GetDefResponse = Result<Vec<Loc>, String>;

    pub mod infer_type_of_name {
        use flow_parser::loc::Loc;
        use flow_services_export_index::export_index;

        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct PropDoc {
            pub prop_name: String,
            pub description: String,
        }

        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct T {
            pub loc: Loc,
            pub actual_name: String,
            pub type_: String,
            pub refs: Option<Vec<(String, Loc, Option<String>)>>,
            pub documentation: Option<String>,
            pub prop_docs: Option<Vec<PropDoc>>,
            pub source: export_index::Source,
        }
    }

    pub type InferTypeOfNameResponse = Result<infer_type_of_name::T, String>;

    pub mod infer_type {
        use flow_parser::loc::Loc;

        use crate::server_prot::refinement_invalidation;

        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct FriendlyResponse {
            pub type_str: String,
            /// The type arguments of a callee's instantiation, a line each.
            pub instantiation: Option<String>,
            pub refs: Option<Vec<(String, Loc)>>,
        }

        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub enum Payload {
            Friendly(Option<FriendlyResponse>),
            // Carries the JSON payload pre-serialized as a string. `serde_json::Value` cannot
            // round-trip through bincode because its `Deserialize` impl uses `deserialize_any`,
            // which bincode 1.x does not support. The string is parsed on the client side.
            Json(String),
        }

        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct T {
            pub loc: Loc,
            pub tys: Payload,
            pub refining_locs: Vec<Loc>,
            pub refinement_invalidated: Vec<(Loc, refinement_invalidation::Reason)>,
            pub documentation: Option<String>,
        }
    }

    pub type InferTypeResponse = Result<infer_type::T, String>;

    pub mod inlay_hint {
        use flow_parser::loc::Loc;

        use crate::server_prot::refinement_invalidation;
        use crate::server_prot::response::infer_type;

        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct Item {
            pub cursor_loc: Loc,
            pub type_loc: Loc,
            pub tys: Option<infer_type::FriendlyResponse>,
            pub refining_locs: Vec<Loc>,
            pub refinement_invalidated: Vec<(Loc, refinement_invalidation::Reason)>,
            pub documentation: Option<String>,
        }

        pub type Response = Result<Vec<Item>, String>;
    }

    pub mod inspect_types {
        use flow_parser::loc::Loc;

        use crate::server_prot::refinement_invalidation;

        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct Item {
            pub cursor_loc: Loc,
            pub type_loc: Loc,
            pub type_str: Option<String>,
            pub refining_locs: Vec<Loc>,
            pub refinement_invalidated: Vec<(Loc, refinement_invalidation::Reason)>,
        }

        pub type Response = Result<Vec<Item>, String>;
    }

    pub type InsertTypeResponse = Result<Patch, String>;

    pub type RageResponse = Vec<(String, String)>;

    pub type SuggestImportsResponse = Result<String, String>;

    pub type GraphResponse = Result<GraphResponseSubgraph, String>;

    pub type GraphResponseSubgraph = Vec<(String, Vec<String>)>;

    #[allow(non_camel_case_types)]
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub enum StatusResponse {
        ERRORS {
            errors: ConcreteLocPrintableErrorSet,
            warnings: ConcreteLocPrintableErrorSet,
            suppressed_errors: Vec<(PrintableError<Loc>, BTreeSet<Loc>)>,
        },
        NO_ERRORS,
        NOT_COVERED,
    }

    pub type CheckFileResponse = StatusResponse;

    pub type FindModuleResponse = (Option<FileKey>, Vec<String>);

    #[allow(non_camel_case_types)]
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub enum Response {
        APPLY_CODE_ACTION(ApplyCodeActionResponse),
        AUTOCOMPLETE(AutocompleteResponse),
        AUTOFIX_EXPORTS(AutofixExportsResponse),
        AUTOFIX_MISSING_LOCAL_ANNOT(AutofixMissingLocalAnnotResponse),
        CHECK_FILE(CheckFileResponse),
        COVERAGE(CoverageResponse),
        BATCH_COVERAGE(BatchCoverageResponse),
        CYCLE(GraphResponse),
        // Ok = pre-serialized JSON mirror (clock, is_fresh_instance, files[]).
        QUERY(Result<String, String>),
        GRAPH_DEP_GRAPH(Result<(), String>),
        DUMP_TYPES(DumpTypesResponse),
        FIND_MODULE(FindModuleResponse),
        FORCE_RECHECK,
        GET_DEF(GetDefResponse),
        INFER_TYPE(InferTypeResponse),
        INLAY_HINT(inlay_hint::Response),
        INSPECT_TYPES(inspect_types::Response),
        TYPE_OF_NAME(Vec<InferTypeOfNameResponse>),
        INSERT_TYPE(InsertTypeResponse),
        RAGE(RageResponse),
        STATUS {
            status_response: StatusResponse,
            lazy_stats: LazyStats,
            /// Rechecks since the current type errors first appeared, if there are any.
            unresolved_error_rounds: Option<u32>,
        },
        SAVE_STATE(Result<String, String>),
        SUGGEST_IMPORTS(SuggestImportsResponse),
        // Appended last so the preceding shared variants' bincode indices stay stable.
        #[cfg(fbcode_build)]
        FOX(flow_facebook_fox_protocol::FoxResponse),
    }

    pub fn to_string(response: &Response) -> &'static str {
        match response {
            Response::APPLY_CODE_ACTION(_) => "apply-code-action response",
            Response::AUTOCOMPLETE(_) => "autocomplete response",
            Response::AUTOFIX_EXPORTS(_) => "autofix exports response",
            Response::AUTOFIX_MISSING_LOCAL_ANNOT(_) => "autofix missing-local-annot response",
            Response::CHECK_FILE(_) => "check_file response",
            Response::COVERAGE(_) => "coverage response",
            Response::BATCH_COVERAGE(_) => "batch-coverage response",
            Response::CYCLE(_) => "cycle response",
            Response::QUERY(_) => "query response",
            Response::GRAPH_DEP_GRAPH(_) => "dep-graph response",
            Response::DUMP_TYPES(_) => "dump_types response",
            Response::FIND_MODULE(_) => "find_module response",
            Response::FORCE_RECHECK => "force_recheck response",
            Response::GET_DEF(_) => "get_def response",
            Response::INFER_TYPE(_) => "infer_type response",
            Response::INLAY_HINT(_) => "inlay_hint response",
            Response::INSPECT_TYPES(_) => "inspect_types response",
            Response::TYPE_OF_NAME(_) => "type_of_name response",
            Response::INSERT_TYPE(_) => "insert_type response",
            Response::RAGE(_) => "rage response",
            Response::STATUS { .. } => "status response",
            Response::SAVE_STATE(_) => "save_state response",
            Response::SUGGEST_IMPORTS(_) => "suggest imports response",
            #[cfg(fbcode_build)]
            Response::FOX(_) => "fox response",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::request::Query;
    use super::request::QueryField;

    #[test]
    fn query_decodes_supported_fields() {
        let query: Query =
            serde_json::from_str(r#"{"fields": ["name", "exists", "new", "flow.content_hash"]}"#)
                .expect("decodes");
        assert_eq!(
            query.fields,
            vec![
                QueryField::Name,
                QueryField::Exists,
                QueryField::New,
                QueryField::ContentHash
            ]
        );
        assert!(query.since.is_none());
    }

    #[test]
    fn query_rejects_unknown_field() {
        assert!(serde_json::from_str::<Query>(r#"{"fields": ["flow.bogus"]}"#).is_err());
    }

    #[test]
    fn query_rejects_overlapping_or_unknown_keys() {
        assert!(serde_json::from_str::<Query>(r#"{"fields": [], "root": "/x"}"#).is_err());
    }

    #[test]
    fn query_accepts_since() {
        use crate::flow_clock::Since;
        let token = format!("flow:1:{:032x}:0", 0xabcu128);
        let query: Query =
            serde_json::from_str(&format!(r#"{{"fields": ["name"], "since": "{token}"}}"#))
                .expect("decodes");
        assert!(matches!(query.since, Some(Since::Clock(_))));
    }

    #[test]
    fn query_defaults_and_decodes_empty_on_fresh_instance() {
        let default: Query = serde_json::from_str(r#"{"fields": ["name"]}"#).expect("decodes");
        assert!(
            !default.empty_on_fresh_instance,
            "empty_on_fresh_instance must default to false when omitted"
        );
        let set: Query =
            serde_json::from_str(r#"{"fields": ["name"], "empty_on_fresh_instance": true}"#)
                .expect("decodes");
        assert!(
            set.empty_on_fresh_instance,
            "empty_on_fresh_instance must decode to true when set"
        );
    }
}
