/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::io::Read;
use std::io::Write;

use flow_common::refinement_invalidation;
use flow_common::verbose::Verbose;
use flow_common_errors::error_utils::cli_output;
use flow_common_errors::error_utils::json_output::JsonVersion;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc as FlowLoc;
use flow_parser::loc::Position as FlowPosition;
use flow_parser::offset_utils::OffsetKind;
use flow_parser_utils_output::replacement_printer::Patch;
use flow_services_coverage::FileCoverage as ServerFileCoverage;
use flow_services_coverage::Kind as ServerCoverageKind;
use flow_services_export::export_index;
use lsp_types::CompletionItemKind;
use lsp_types::CompletionItemTag;
use lsp_types::InsertTextFormat;
use serde::Deserialize;
use serde::Serialize;

use crate::lsp_prot;
use crate::server_prot;
use crate::server_prot::response::LazyStats;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SaveStateOut {
    File(String),
    Scm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerRequest {
    CliEphemeralCommand {
        command: CliCommand,
    },
    PersistentConnect {
        client_id: lsp_prot::ClientId,
        lsp_initialize_params: lsp_types::InitializeParams,
    },
    PersistentRequest {
        client_id: lsp_prot::ClientId,
        request: lsp_prot::RequestWithMetadata,
    },
    PersistentPoll {
        client_id: lsp_prot::ClientId,
    },
    PersistentDisconnect {
        client_id: lsp_prot::ClientId,
    },
    CheckContents {
        input: FileInput,
        verbose: Option<VerboseWire>,
        force: bool,
        error_flags: ErrorFlagsWire,
        wait_for_recheck: Option<bool>,
        strip_root: bool,
        json: bool,
        pretty: bool,
        json_version: Option<JsonVersion>,
        offset_kind: OffsetKind,
    },
    Status {
        error_flags: ErrorFlagsWire,
        from: Option<String>,
        strip_root: bool,
        json: bool,
        pretty: bool,
        json_version: Option<JsonVersion>,
        offset_kind: OffsetKind,
    },
    ForceRecheck {
        files: Vec<String>,
        focus: bool,
        missed_changes: bool,
        changed_mergebase: bool,
    },
    SaveState {
        out: SaveStateOut,
        from: Option<String>,
    },
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerResponse {
    CliEphemeralResponse {
        response: CliResponse,
    },
    PersistentConnected,
    PersistentBusy {
        server_status: crate::server_status::Status,
        watcher_status: crate::file_watcher_status::Status,
    },
    PersistentAck,
    ShutdownAck,
    PersistentPoll {
        message: Option<lsp_prot::MessageFromServer>,
    },
    CheckContents {
        has_errors: bool,
        warning_count: usize,
        error_output: String,
        not_covered: bool,
    },
    Status {
        has_errors: bool,
        error_count: usize,
        warning_count: usize,
        error_output: String,
        lazy_stats: LazyStats,
    },
    ForceRecheck,
    SaveState {
        result: Result<String, String>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileInput {
    FileName(String),
    FileContent(Option<String>, String),
}

impl From<flow_server_utils::file_input::FileInput> for FileInput {
    fn from(input: flow_server_utils::file_input::FileInput) -> Self {
        match input {
            flow_server_utils::file_input::FileInput::FileName(path) => Self::FileName(path),
            flow_server_utils::file_input::FileInput::FileContent(path, content) => {
                Self::FileContent(path, content.to_string())
            }
        }
    }
}

impl FileInput {
    pub fn into_server_file_input(self) -> flow_server_utils::file_input::FileInput {
        match self {
            FileInput::FileName(path) => flow_server_utils::file_input::FileInput::FileName(path),
            FileInput::FileContent(path, content) => {
                flow_server_utils::file_input::FileInput::FileContent(path, content.into())
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerboseWire {
    pub indent: u32,
    pub depth: u32,
    pub enabled_during_flowlib: bool,
    pub focused_files: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RenderingModeWire {
    CliColorAlways,
    CliColorNever,
    CliColorAuto,
    IdeDetailedError,
}

impl From<cli_output::RenderingMode> for RenderingModeWire {
    fn from(mode: cli_output::RenderingMode) -> Self {
        match mode {
            cli_output::RenderingMode::CliColorAlways => Self::CliColorAlways,
            cli_output::RenderingMode::CliColorNever => Self::CliColorNever,
            cli_output::RenderingMode::CliColorAuto => Self::CliColorAuto,
            cli_output::RenderingMode::IdeDetailedError => Self::IdeDetailedError,
        }
    }
}

impl From<RenderingModeWire> for cli_output::RenderingMode {
    fn from(mode: RenderingModeWire) -> Self {
        match mode {
            RenderingModeWire::CliColorAlways => Self::CliColorAlways,
            RenderingModeWire::CliColorNever => Self::CliColorNever,
            RenderingModeWire::CliColorAuto => Self::CliColorAuto,
            RenderingModeWire::IdeDetailedError => Self::IdeDetailedError,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorFlagsWire {
    pub rendering_mode: RenderingModeWire,
    pub include_warnings: bool,
    pub max_warnings: Option<i32>,
    pub one_line: bool,
    pub list_files: bool,
    pub show_all_errors: bool,
    pub show_all_branches: bool,
    pub unicode: bool,
    pub message_width: i32,
}

impl From<cli_output::ErrorFlags> for ErrorFlagsWire {
    fn from(flags: cli_output::ErrorFlags) -> Self {
        Self {
            rendering_mode: flags.rendering_mode.into(),
            include_warnings: flags.include_warnings,
            max_warnings: flags.max_warnings,
            one_line: flags.one_line,
            list_files: flags.list_files,
            show_all_errors: flags.show_all_errors,
            show_all_branches: flags.show_all_branches,
            unicode: flags.unicode,
            message_width: flags.message_width,
        }
    }
}

impl From<ErrorFlagsWire> for cli_output::ErrorFlags {
    fn from(flags: ErrorFlagsWire) -> Self {
        Self {
            rendering_mode: flags.rendering_mode.into(),
            include_warnings: flags.include_warnings,
            max_warnings: flags.max_warnings,
            one_line: flags.one_line,
            list_files: flags.list_files,
            show_all_errors: flags.show_all_errors,
            show_all_branches: flags.show_all_branches,
            unicode: flags.unicode,
            message_width: flags.message_width,
        }
    }
}

impl From<Verbose> for VerboseWire {
    fn from(verbose: Verbose) -> Self {
        Self {
            indent: verbose.indent,
            depth: verbose.depth,
            enabled_during_flowlib: verbose.enabled_during_flowlib,
            focused_files: verbose.focused_files,
        }
    }
}

impl From<VerboseWire> for Verbose {
    fn from(verbose: VerboseWire) -> Self {
        Self {
            indent: verbose.indent,
            depth: verbose.depth,
            enabled_during_flowlib: verbose.enabled_during_flowlib,
            focused_files: verbose.focused_files,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodeAction {
    Quickfix { include_best_effort_fix: bool },
    SourceAddMissingImports,
    SuggestImports,
}

impl From<server_prot::code_action::T> for CodeAction {
    fn from(action: server_prot::code_action::T) -> Self {
        match action {
            server_prot::code_action::T::Quickfix {
                include_best_effort_fix,
            } => Self::Quickfix {
                include_best_effort_fix,
            },
            server_prot::code_action::T::SourceAddMissingImports => Self::SourceAddMissingImports,
            server_prot::code_action::T::SuggestImports => Self::SuggestImports,
        }
    }
}

impl From<CodeAction> for server_prot::code_action::T {
    fn from(action: CodeAction) -> Self {
        match action {
            CodeAction::Quickfix {
                include_best_effort_fix,
            } => Self::Quickfix {
                include_best_effort_fix,
            },
            CodeAction::SourceAddMissingImports => Self::SourceAddMissingImports,
            CodeAction::SuggestImports => Self::SuggestImports,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferTypeOptions {
    pub input: FileInput,
    pub line: i32,
    pub r#char: i32,
    pub verbose: Option<VerboseWire>,
    pub omit_targ_defaults: bool,
    pub wait_for_recheck: Option<bool>,
    pub verbose_normalizer: bool,
    pub max_depth: i32,
    pub json: bool,
    pub strip_root: Option<std::path::PathBuf>,
    pub expanded: bool,
    pub debug_print_internal_repr: bool,
    pub no_typed_ast_for_imports: bool,
}

impl From<server_prot::infer_type_options::T> for InferTypeOptions {
    fn from(options: server_prot::infer_type_options::T) -> Self {
        Self {
            input: options.input.into(),
            line: options.line,
            r#char: options.r#char,
            verbose: options.verbose.map(VerboseWire::from),
            omit_targ_defaults: options.omit_targ_defaults,
            wait_for_recheck: options.wait_for_recheck,
            verbose_normalizer: options.verbose_normalizer,
            max_depth: options.max_depth,
            json: options.json,
            strip_root: options.strip_root,
            expanded: options.expanded,
            debug_print_internal_repr: options.debug_print_internal_repr,
            no_typed_ast_for_imports: options.no_typed_ast_for_imports,
        }
    }
}

impl InferTypeOptions {
    fn into_server_options(self) -> server_prot::infer_type_options::T {
        server_prot::infer_type_options::T {
            input: self.input.into_server_file_input(),
            line: self.line,
            r#char: self.r#char,
            verbose: self.verbose.map(Verbose::from),
            omit_targ_defaults: self.omit_targ_defaults,
            wait_for_recheck: self.wait_for_recheck,
            verbose_normalizer: self.verbose_normalizer,
            max_depth: self.max_depth,
            json: self.json,
            strip_root: self.strip_root,
            expanded: self.expanded,
            debug_print_internal_repr: self.debug_print_internal_repr,
            no_typed_ast_for_imports: self.no_typed_ast_for_imports,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlayHintOptions {
    pub input: FileInput,
    pub verbose: Option<VerboseWire>,
    pub omit_targ_defaults: bool,
    pub wait_for_recheck: Option<bool>,
    pub verbose_normalizer: bool,
    pub max_depth: i32,
    pub no_typed_ast_for_imports: bool,
}

impl From<server_prot::inlay_hint_options::T> for InlayHintOptions {
    fn from(options: server_prot::inlay_hint_options::T) -> Self {
        Self {
            input: options.input.into(),
            verbose: options.verbose.map(VerboseWire::from),
            omit_targ_defaults: options.omit_targ_defaults,
            wait_for_recheck: options.wait_for_recheck,
            verbose_normalizer: options.verbose_normalizer,
            max_depth: options.max_depth,
            no_typed_ast_for_imports: options.no_typed_ast_for_imports,
        }
    }
}

impl InlayHintOptions {
    fn into_server_options(self) -> server_prot::inlay_hint_options::T {
        server_prot::inlay_hint_options::T {
            input: self.input.into_server_file_input(),
            verbose: self.verbose.map(Verbose::from),
            omit_targ_defaults: self.omit_targ_defaults,
            wait_for_recheck: self.wait_for_recheck,
            verbose_normalizer: self.verbose_normalizer,
            max_depth: self.max_depth,
            no_typed_ast_for_imports: self.no_typed_ast_for_imports,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeOfNameOptions {
    pub input: FileInput,
    pub names: Vec<String>,
    pub verbose: Option<VerboseWire>,
    pub wait_for_recheck: Option<bool>,
    pub expand_component_props: bool,
    pub exact_match_only: bool,
    pub strip_root: Option<std::path::PathBuf>,
}

impl From<server_prot::type_of_name_options::T> for TypeOfNameOptions {
    fn from(options: server_prot::type_of_name_options::T) -> Self {
        Self {
            input: options.input.into(),
            names: options.names,
            verbose: options.verbose.map(VerboseWire::from),
            wait_for_recheck: options.wait_for_recheck,
            expand_component_props: options.expand_component_props,
            exact_match_only: options.exact_match_only,
            strip_root: options.strip_root,
        }
    }
}

impl TypeOfNameOptions {
    fn into_server_options(self) -> server_prot::type_of_name_options::T {
        server_prot::type_of_name_options::T {
            input: self.input.into_server_file_input(),
            names: self.names,
            verbose: self.verbose.map(Verbose::from),
            wait_for_recheck: self.wait_for_recheck,
            expand_component_props: self.expand_component_props,
            exact_match_only: self.exact_match_only,
            strip_root: self.strip_root,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum CliCommand {
    APPLY_CODE_ACTION {
        input: FileInput,
        action: CodeAction,
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
        verbose: Option<VerboseWire>,
        wait_for_recheck: Option<bool>,
    },
    AUTOFIX_MISSING_LOCAL_ANNOT {
        input: FileInput,
        verbose: Option<VerboseWire>,
        wait_for_recheck: Option<bool>,
    },
    CHECK_FILE {
        input: FileInput,
        verbose: Option<VerboseWire>,
        force: bool,
        include_warnings: bool,
        wait_for_recheck: Option<bool>,
    },
    BATCH_COVERAGE {
        batch: Vec<String>,
        wait_for_recheck: Option<bool>,
    },
    COVERAGE {
        input: FileInput,
        force: bool,
        wait_for_recheck: Option<bool>,
    },
    CYCLE {
        filename: String,
        types_only: bool,
    },
    QUERY {
        query: server_prot::request::Query,
    },
    DUMP_TYPES {
        input: FileInput,
        evaluate_type_destructors: bool,
        for_tool: Option<i32>,
        dedup_threshold: Option<usize>,
        wait_for_recheck: Option<bool>,
    },
    FIND_MODULE {
        moduleref: String,
        filename: String,
        wait_for_recheck: Option<bool>,
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
    INFER_TYPE(InferTypeOptions),
    INLAY_HINT(InlayHintOptions),
    INSPECT_TYPES {
        input: FileInput,
        wait_for_recheck: Option<bool>,
    },
    TYPE_OF_NAME(TypeOfNameOptions),
    INSERT_TYPE {
        input: FileInput,
        target: Loc,
        verbose: Option<VerboseWire>,
        location_is_strict: bool,
        wait_for_recheck: Option<bool>,
        omit_targ_defaults: bool,
    },
    STATUS {
        include_warnings: bool,
    },
}

impl From<server_prot::request::Command> for CliCommand {
    fn from(command: server_prot::request::Command) -> Self {
        match command {
            server_prot::request::Command::APPLY_CODE_ACTION {
                input,
                action,
                wait_for_recheck,
            } => Self::APPLY_CODE_ACTION {
                input: input.into(),
                action: action.into(),
                wait_for_recheck,
            },
            server_prot::request::Command::AUTOCOMPLETE {
                input,
                cursor,
                trigger_character,
                wait_for_recheck,
                imports,
                imports_ranked_usage,
                show_ranking_info,
            } => Self::AUTOCOMPLETE {
                input: input.into(),
                cursor,
                trigger_character,
                wait_for_recheck,
                imports,
                imports_ranked_usage,
                show_ranking_info,
            },
            server_prot::request::Command::AUTOFIX_EXPORTS {
                input,
                verbose,
                wait_for_recheck,
            } => Self::AUTOFIX_EXPORTS {
                input: input.into(),
                verbose: verbose.map(VerboseWire::from),
                wait_for_recheck,
            },
            server_prot::request::Command::AUTOFIX_MISSING_LOCAL_ANNOT {
                input,
                verbose,
                wait_for_recheck,
            } => Self::AUTOFIX_MISSING_LOCAL_ANNOT {
                input: input.into(),
                verbose: verbose.map(VerboseWire::from),
                wait_for_recheck,
            },
            server_prot::request::Command::CHECK_FILE {
                input,
                verbose,
                force,
                include_warnings,
                wait_for_recheck,
            } => Self::CHECK_FILE {
                input: input.into(),
                verbose: verbose.map(VerboseWire::from),
                force,
                include_warnings,
                wait_for_recheck,
            },
            server_prot::request::Command::BATCH_COVERAGE {
                batch,
                wait_for_recheck,
            } => Self::BATCH_COVERAGE {
                batch,
                wait_for_recheck,
            },
            server_prot::request::Command::COVERAGE {
                input,
                force,
                wait_for_recheck,
            } => Self::COVERAGE {
                input: input.into(),
                force,
                wait_for_recheck,
            },
            server_prot::request::Command::CYCLE {
                filename,
                types_only,
            } => Self::CYCLE {
                filename,
                types_only,
            },
            server_prot::request::Command::QUERY { query } => Self::QUERY { query },
            server_prot::request::Command::DUMP_TYPES {
                input,
                evaluate_type_destructors,
                for_tool,
                dedup_threshold,
                wait_for_recheck,
            } => Self::DUMP_TYPES {
                input: input.into(),
                evaluate_type_destructors,
                for_tool,
                dedup_threshold,
                wait_for_recheck,
            },
            server_prot::request::Command::FIND_MODULE {
                moduleref,
                filename,
                wait_for_recheck,
            } => Self::FIND_MODULE {
                moduleref,
                filename,
                wait_for_recheck,
            },
            server_prot::request::Command::GET_DEF {
                input,
                line,
                r#char,
                wait_for_recheck,
            } => Self::GET_DEF {
                input: input.into(),
                line,
                r#char,
                wait_for_recheck,
            },
            server_prot::request::Command::GRAPH_DEP_GRAPH {
                root,
                strip_root,
                outfile,
                types_only,
            } => Self::GRAPH_DEP_GRAPH {
                root,
                strip_root,
                outfile,
                types_only,
            },
            server_prot::request::Command::INFER_TYPE(input) => Self::INFER_TYPE(input.into()),
            server_prot::request::Command::INLAY_HINT(input) => Self::INLAY_HINT(input.into()),
            server_prot::request::Command::INSPECT_TYPES(
                server_prot::inspect_types_options::T {
                    input,
                    wait_for_recheck,
                },
            ) => Self::INSPECT_TYPES {
                input: input.into(),
                wait_for_recheck,
            },
            server_prot::request::Command::TYPE_OF_NAME(input) => Self::TYPE_OF_NAME(input.into()),
            server_prot::request::Command::INSERT_TYPE {
                input,
                target,
                verbose,
                location_is_strict,
                wait_for_recheck,
                omit_targ_defaults,
            } => Self::INSERT_TYPE {
                input: input.into(),
                target: target.into(),
                verbose: verbose.map(VerboseWire::from),
                location_is_strict,
                wait_for_recheck,
                omit_targ_defaults,
            },
            server_prot::request::Command::STATUS { include_warnings } => {
                Self::STATUS { include_warnings }
            }
            command => panic!(
                "Unsupported CLI socket command: {}",
                server_prot::request::to_string(&command)
            ),
        }
    }
}

impl CliCommand {
    pub fn into_server_command(self) -> server_prot::request::Command {
        match self {
            CliCommand::APPLY_CODE_ACTION {
                input,
                action,
                wait_for_recheck,
            } => server_prot::request::Command::APPLY_CODE_ACTION {
                input: input.into_server_file_input(),
                action: action.into(),
                wait_for_recheck,
            },
            CliCommand::AUTOCOMPLETE {
                input,
                cursor,
                trigger_character,
                wait_for_recheck,
                imports,
                imports_ranked_usage,
                show_ranking_info,
            } => server_prot::request::Command::AUTOCOMPLETE {
                input: input.into_server_file_input(),
                cursor,
                trigger_character,
                wait_for_recheck,
                imports,
                imports_ranked_usage,
                show_ranking_info,
            },
            CliCommand::AUTOFIX_EXPORTS {
                input,
                verbose,
                wait_for_recheck,
            } => server_prot::request::Command::AUTOFIX_EXPORTS {
                input: input.into_server_file_input(),
                verbose: verbose.map(Verbose::from),
                wait_for_recheck,
            },
            CliCommand::AUTOFIX_MISSING_LOCAL_ANNOT {
                input,
                verbose,
                wait_for_recheck,
            } => server_prot::request::Command::AUTOFIX_MISSING_LOCAL_ANNOT {
                input: input.into_server_file_input(),
                verbose: verbose.map(Verbose::from),
                wait_for_recheck,
            },
            CliCommand::CHECK_FILE {
                input,
                verbose,
                force,
                include_warnings,
                wait_for_recheck,
            } => server_prot::request::Command::CHECK_FILE {
                input: input.into_server_file_input(),
                verbose: verbose.map(Verbose::from),
                force,
                include_warnings,
                wait_for_recheck,
            },
            CliCommand::BATCH_COVERAGE {
                batch,
                wait_for_recheck,
            } => server_prot::request::Command::BATCH_COVERAGE {
                batch,
                wait_for_recheck,
            },
            CliCommand::COVERAGE {
                input,
                force,
                wait_for_recheck,
            } => server_prot::request::Command::COVERAGE {
                input: input.into_server_file_input(),
                force,
                wait_for_recheck,
            },
            CliCommand::CYCLE {
                filename,
                types_only,
            } => server_prot::request::Command::CYCLE {
                filename,
                types_only,
            },
            CliCommand::QUERY { query } => server_prot::request::Command::QUERY { query },
            CliCommand::DUMP_TYPES {
                input,
                evaluate_type_destructors,
                for_tool,
                dedup_threshold,
                wait_for_recheck,
            } => server_prot::request::Command::DUMP_TYPES {
                input: input.into_server_file_input(),
                evaluate_type_destructors,
                for_tool,
                dedup_threshold,
                wait_for_recheck,
            },
            CliCommand::FIND_MODULE {
                moduleref,
                filename,
                wait_for_recheck,
            } => server_prot::request::Command::FIND_MODULE {
                moduleref,
                filename,
                wait_for_recheck,
            },
            CliCommand::GET_DEF {
                input,
                line,
                r#char,
                wait_for_recheck,
            } => server_prot::request::Command::GET_DEF {
                input: input.into_server_file_input(),
                line,
                r#char,
                wait_for_recheck,
            },
            CliCommand::GRAPH_DEP_GRAPH {
                root,
                strip_root,
                outfile,
                types_only,
            } => server_prot::request::Command::GRAPH_DEP_GRAPH {
                root,
                strip_root,
                outfile,
                types_only,
            },
            CliCommand::INFER_TYPE(input) => {
                server_prot::request::Command::INFER_TYPE(input.into_server_options())
            }
            CliCommand::INLAY_HINT(input) => {
                server_prot::request::Command::INLAY_HINT(input.into_server_options())
            }
            CliCommand::INSPECT_TYPES {
                input,
                wait_for_recheck,
            } => server_prot::request::Command::INSPECT_TYPES(
                server_prot::inspect_types_options::T {
                    input: input.into_server_file_input(),
                    wait_for_recheck,
                },
            ),
            CliCommand::TYPE_OF_NAME(input) => {
                server_prot::request::Command::TYPE_OF_NAME(input.into_server_options())
            }
            CliCommand::INSERT_TYPE {
                input,
                target,
                verbose,
                location_is_strict,
                wait_for_recheck,
                omit_targ_defaults,
            } => server_prot::request::Command::INSERT_TYPE {
                input: input.into_server_file_input(),
                target: target.into_flow_loc(),
                verbose: verbose.map(Verbose::from),
                location_is_strict,
                wait_for_recheck,
                omit_targ_defaults,
            },
            CliCommand::STATUS { include_warnings } => {
                server_prot::request::Command::STATUS { include_warnings }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: i32,
    pub column: i32,
}

impl From<FlowPosition> for Position {
    fn from(position: FlowPosition) -> Self {
        Self {
            line: position.line,
            column: position.column,
        }
    }
}

impl From<&FlowPosition> for Position {
    fn from(position: &FlowPosition) -> Self {
        Self::from(*position)
    }
}

impl From<Position> for FlowPosition {
    fn from(position: Position) -> Self {
        Self {
            line: position.line,
            column: position.column,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Loc {
    pub source: Option<String>,
    pub start: Position,
    pub end: Position,
}

impl From<FlowLoc> for Loc {
    fn from(loc: FlowLoc) -> Self {
        Self {
            source: loc.source.map(|source| source.to_absolute()),
            start: loc.start.into(),
            end: loc.end.into(),
        }
    }
}

impl From<&FlowLoc> for Loc {
    fn from(loc: &FlowLoc) -> Self {
        Self::from(loc.clone())
    }
}

impl Loc {
    pub fn into_flow_loc(self) -> FlowLoc {
        FlowLoc {
            source: self
                .source
                .map(|source| FileKey::source_file_of_absolute(&source)),
            start: self.start.into(),
            end: self.end.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CoverageKind {
    Checked,
    Any,
    Empty,
}

impl From<ServerCoverageKind> for CoverageKind {
    fn from(kind: ServerCoverageKind) -> Self {
        match kind {
            ServerCoverageKind::Checked => Self::Checked,
            ServerCoverageKind::Any => Self::Any,
            ServerCoverageKind::Empty => Self::Empty,
        }
    }
}

impl CoverageKind {
    fn into_server_kind(self) -> ServerCoverageKind {
        match self {
            CoverageKind::Checked => ServerCoverageKind::Checked,
            CoverageKind::Any => ServerCoverageKind::Any,
            CoverageKind::Empty => ServerCoverageKind::Empty,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FileCoverage {
    pub checked: i32,
    pub uncovered: i32,
    pub empty: i32,
}

impl From<ServerFileCoverage> for FileCoverage {
    fn from(coverage: ServerFileCoverage) -> Self {
        Self {
            checked: coverage.checked,
            uncovered: coverage.uncovered,
            empty: coverage.empty,
        }
    }
}

impl FileCoverage {
    fn into_server_coverage(self) -> ServerFileCoverage {
        ServerFileCoverage {
            checked: self.checked,
            uncovered: self.uncovered,
            empty: self.empty,
        }
    }
}

pub type TextEdit = (Loc, String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct InsertReplaceEdit {
    pub newText: String,
    pub insert: Loc,
    pub replace: Loc,
}

impl From<server_prot::response::InsertReplaceEdit> for InsertReplaceEdit {
    fn from(edit: server_prot::response::InsertReplaceEdit) -> Self {
        Self {
            newText: edit.newText,
            insert: edit.insert.into(),
            replace: edit.replace.into(),
        }
    }
}

impl InsertReplaceEdit {
    fn into_server_edit(self) -> server_prot::response::InsertReplaceEdit {
        server_prot::response::InsertReplaceEdit {
            newText: self.newText,
            insert: self.insert.into_flow_loc(),
            replace: self.replace.into_flow_loc(),
        }
    }
}

pub mod completion {
    use super::InsertReplaceEdit;
    use super::TextEdit;
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

    impl From<server_prot::response::completion::CompletionItem> for CompletionItem {
        fn from(item: server_prot::response::completion::CompletionItem) -> Self {
            Self {
                kind: item.kind,
                name: item.name,
                labelDetail: item.labelDetail,
                description: item.description,
                itemDetail: item.itemDetail,
                text_edit: item.text_edit.map(InsertReplaceEdit::from),
                additional_text_edits: item
                    .additional_text_edits
                    .into_iter()
                    .map(|(loc, text)| (loc.into(), text))
                    .collect(),
                sort_text: item.sort_text,
                preselect: item.preselect,
                documentation: item.documentation,
                tags: item.tags,
                log_info: item.log_info,
                insert_text_format: item.insert_text_format,
            }
        }
    }

    impl CompletionItem {
        pub fn into_server_item(self) -> server_prot::response::completion::CompletionItem {
            server_prot::response::completion::CompletionItem {
                kind: self.kind,
                name: self.name,
                labelDetail: self.labelDetail,
                description: self.description,
                itemDetail: self.itemDetail,
                text_edit: self.text_edit.map(InsertReplaceEdit::into_server_edit),
                additional_text_edits: self
                    .additional_text_edits
                    .into_iter()
                    .map(|(loc, text)| (loc.into_flow_loc(), text))
                    .collect(),
                sort_text: self.sort_text,
                preselect: self.preselect,
                documentation: self.documentation,
                tags: self.tags,
                log_info: self.log_info,
                insert_text_format: self.insert_text_format,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[allow(non_snake_case)]
    pub struct T {
        pub items: Vec<CompletionItem>,
        pub is_incomplete: bool,
    }

    impl From<server_prot::response::completion::T> for T {
        fn from(completion: server_prot::response::completion::T) -> Self {
            Self {
                items: completion
                    .items
                    .into_iter()
                    .map(CompletionItem::from)
                    .collect(),
                is_incomplete: completion.is_incomplete,
            }
        }
    }

    impl T {
        pub fn into_server_completion(self) -> server_prot::response::completion::T {
            server_prot::response::completion::T {
                items: self
                    .items
                    .into_iter()
                    .map(CompletionItem::into_server_item)
                    .collect(),
                is_incomplete: self.is_incomplete,
            }
        }
    }
}

pub mod infer_type {
    use super::Loc;
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FriendlyResponse {
        pub type_str: String,
        pub instantiation: Option<String>,
        pub refs: Option<Vec<(String, Loc)>>,
    }

    impl From<server_prot::response::infer_type::FriendlyResponse> for FriendlyResponse {
        fn from(response: server_prot::response::infer_type::FriendlyResponse) -> Self {
            Self {
                type_str: response.type_str,
                instantiation: response.instantiation,
                refs: response.refs.map(|refs| {
                    refs.into_iter()
                        .map(|(name, loc)| (name, loc.into()))
                        .collect()
                }),
            }
        }
    }

    impl FriendlyResponse {
        pub fn into_server_response(self) -> server_prot::response::infer_type::FriendlyResponse {
            server_prot::response::infer_type::FriendlyResponse {
                type_str: self.type_str,
                instantiation: self.instantiation,
                refs: self.refs.map(|refs| {
                    refs.into_iter()
                        .map(|(name, loc)| (name, loc.into_flow_loc()))
                        .collect()
                }),
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum Payload {
        Friendly(Option<FriendlyResponse>),
        // Carries the JSON payload pre-serialized as a string; see
        // `server_prot::response::infer_type::Payload`.
        Json(String),
    }

    impl From<server_prot::response::infer_type::Payload> for Payload {
        fn from(payload: server_prot::response::infer_type::Payload) -> Self {
            match payload {
                server_prot::response::infer_type::Payload::Friendly(response) => {
                    Self::Friendly(response.map(FriendlyResponse::from))
                }
                server_prot::response::infer_type::Payload::Json(json) => Self::Json(json),
            }
        }
    }

    impl Payload {
        fn into_server_payload(self) -> server_prot::response::infer_type::Payload {
            match self {
                Payload::Friendly(response) => {
                    server_prot::response::infer_type::Payload::Friendly(
                        response.map(FriendlyResponse::into_server_response),
                    )
                }
                Payload::Json(json) => server_prot::response::infer_type::Payload::Json(json),
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct T {
        pub loc: Loc,
        pub tys: Payload,
        pub refining_locs: Vec<Loc>,
        pub refinement_invalidated: Vec<(Loc, refinement_invalidation::Reason)>,
        pub documentation: Option<String>,
    }

    impl From<server_prot::response::infer_type::T> for T {
        fn from(response: server_prot::response::infer_type::T) -> Self {
            Self {
                loc: response.loc.into(),
                tys: response.tys.into(),
                refining_locs: response.refining_locs.into_iter().map(Loc::from).collect(),
                refinement_invalidated: response
                    .refinement_invalidated
                    .into_iter()
                    .map(|(loc, reason)| (loc.into(), reason))
                    .collect(),
                documentation: response.documentation,
            }
        }
    }

    impl T {
        pub fn into_server_response(self) -> server_prot::response::infer_type::T {
            server_prot::response::infer_type::T {
                loc: self.loc.into_flow_loc(),
                tys: self.tys.into_server_payload(),
                refining_locs: self
                    .refining_locs
                    .into_iter()
                    .map(Loc::into_flow_loc)
                    .collect(),
                refinement_invalidated: self
                    .refinement_invalidated
                    .into_iter()
                    .map(|(loc, reason)| (loc.into_flow_loc(), reason))
                    .collect(),
                documentation: self.documentation,
            }
        }
    }
}

pub mod infer_type_of_name {
    use super::Loc;
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PropDoc {
        pub prop_name: String,
        pub description: String,
    }

    impl From<server_prot::response::infer_type_of_name::PropDoc> for PropDoc {
        fn from(prop_doc: server_prot::response::infer_type_of_name::PropDoc) -> Self {
            Self {
                prop_name: prop_doc.prop_name,
                description: prop_doc.description,
            }
        }
    }

    impl From<PropDoc> for server_prot::response::infer_type_of_name::PropDoc {
        fn from(prop_doc: PropDoc) -> Self {
            Self {
                prop_name: prop_doc.prop_name,
                description: prop_doc.description,
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum Source {
        Global,
        Builtin(String),
        FileKey(String),
    }

    impl From<export_index::Source> for Source {
        fn from(source: export_index::Source) -> Self {
            match source {
                export_index::Source::Global => Self::Global,
                export_index::Source::Builtin(userland) => {
                    Self::Builtin(userland.display().to_string())
                }
                export_index::Source::FileKey(file_key) => Self::FileKey(file_key.to_absolute()),
            }
        }
    }

    impl Source {
        fn into_server_source(self) -> export_index::Source {
            match self {
                Source::Global => export_index::Source::Global,
                Source::Builtin(userland) => export_index::Source::Builtin(
                    flow_common::flow_import_specifier::Userland::from_smol_str(userland.into()),
                ),
                Source::FileKey(path) => {
                    export_index::Source::FileKey(FileKey::source_file_of_absolute(&path))
                }
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct T {
        pub loc: Loc,
        pub actual_name: String,
        pub type_: String,
        pub refs: Option<Vec<(String, Loc, Option<String>)>>,
        pub documentation: Option<String>,
        pub prop_docs: Option<Vec<PropDoc>>,
        pub source: Source,
    }

    impl From<server_prot::response::infer_type_of_name::T> for T {
        fn from(response: server_prot::response::infer_type_of_name::T) -> Self {
            Self {
                loc: response.loc.into(),
                actual_name: response.actual_name,
                type_: response.type_,
                refs: response.refs.map(|refs| {
                    refs.into_iter()
                        .map(|(name, loc, detail)| (name, loc.into(), detail))
                        .collect()
                }),
                documentation: response.documentation,
                prop_docs: response
                    .prop_docs
                    .map(|prop_docs| prop_docs.into_iter().map(PropDoc::from).collect()),
                source: response.source.into(),
            }
        }
    }

    impl T {
        pub fn into_server_response(self) -> server_prot::response::infer_type_of_name::T {
            server_prot::response::infer_type_of_name::T {
                loc: self.loc.into_flow_loc(),
                actual_name: self.actual_name,
                type_: self.type_,
                refs: self.refs.map(|refs| {
                    refs.into_iter()
                        .map(|(name, loc, detail)| (name, loc.into_flow_loc(), detail))
                        .collect()
                }),
                documentation: self.documentation,
                prop_docs: self
                    .prop_docs
                    .map(|prop_docs| prop_docs.into_iter().map(Into::into).collect()),
                source: self.source.into_server_source(),
            }
        }
    }
}

pub mod inlay_hint {
    use super::Loc;
    use super::infer_type;
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Item {
        pub cursor_loc: Loc,
        pub type_loc: Loc,
        pub tys: Option<infer_type::FriendlyResponse>,
        pub refining_locs: Vec<Loc>,
        pub refinement_invalidated: Vec<(Loc, refinement_invalidation::Reason)>,
        pub documentation: Option<String>,
    }

    impl From<server_prot::response::inlay_hint::Item> for Item {
        fn from(item: server_prot::response::inlay_hint::Item) -> Self {
            Self {
                cursor_loc: item.cursor_loc.into(),
                type_loc: item.type_loc.into(),
                tys: item.tys.map(infer_type::FriendlyResponse::from),
                refining_locs: item.refining_locs.into_iter().map(Loc::from).collect(),
                refinement_invalidated: item
                    .refinement_invalidated
                    .into_iter()
                    .map(|(loc, reason)| (loc.into(), reason))
                    .collect(),
                documentation: item.documentation,
            }
        }
    }

    impl Item {
        pub fn into_server_item(self) -> server_prot::response::inlay_hint::Item {
            server_prot::response::inlay_hint::Item {
                cursor_loc: self.cursor_loc.into_flow_loc(),
                type_loc: self.type_loc.into_flow_loc(),
                tys: self
                    .tys
                    .map(infer_type::FriendlyResponse::into_server_response),
                refining_locs: self
                    .refining_locs
                    .into_iter()
                    .map(Loc::into_flow_loc)
                    .collect(),
                refinement_invalidated: self
                    .refinement_invalidated
                    .into_iter()
                    .map(|(loc, reason)| (loc.into_flow_loc(), reason))
                    .collect(),
                documentation: self.documentation,
            }
        }
    }

    pub type Response = Result<Vec<Item>, String>;
}

pub mod inspect_types {
    use super::Loc;
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Item {
        pub cursor_loc: Loc,
        pub type_loc: Loc,
        pub type_str: Option<String>,
        pub refining_locs: Vec<Loc>,
        pub refinement_invalidated: Vec<(Loc, refinement_invalidation::Reason)>,
    }

    impl From<server_prot::response::inspect_types::Item> for Item {
        fn from(item: server_prot::response::inspect_types::Item) -> Self {
            Self {
                cursor_loc: item.cursor_loc.into(),
                type_loc: item.type_loc.into(),
                type_str: item.type_str,
                refining_locs: item.refining_locs.into_iter().map(Loc::from).collect(),
                refinement_invalidated: item
                    .refinement_invalidated
                    .into_iter()
                    .map(|(loc, reason)| (loc.into(), reason))
                    .collect(),
            }
        }
    }

    impl Item {
        pub fn into_server_item(self) -> server_prot::response::inspect_types::Item {
            server_prot::response::inspect_types::Item {
                cursor_loc: self.cursor_loc.into_flow_loc(),
                type_loc: self.type_loc.into_flow_loc(),
                type_str: self.type_str,
                refining_locs: self
                    .refining_locs
                    .into_iter()
                    .map(Loc::into_flow_loc)
                    .collect(),
                refinement_invalidated: self
                    .refinement_invalidated
                    .into_iter()
                    .map(|(loc, reason)| (loc.into_flow_loc(), reason))
                    .collect(),
            }
        }
    }

    pub type Response = Result<Vec<Item>, String>;
}

pub type AutocompleteResponse = Result<(completion::T, server_prot::response::AcType), String>;
pub type ApplyCodeActionResponse = Result<Patch, String>;
pub type AutofixExportsResponse = Result<(Patch, Vec<String>), String>;
pub type AutofixMissingLocalAnnotResponse = Result<Patch, String>;
pub type BatchCoverageResponse = Result<Vec<(String, FileCoverage)>, String>;
pub type CoverageResponse = Result<Vec<(Loc, CoverageKind)>, String>;
/// Mirrors `server_prot`'s: the `$defs` table lives inside the `Ok` so a failed dump cannot
/// carry one.
pub type DumpTypesResponse = Result<(Vec<(Loc, String)>, Option<String>), String>;
pub type CycleResponse = Result<Vec<(String, Vec<String>)>, String>;
pub type FindModuleResponse = (Option<String>, Vec<String>);
pub type GetDefResponse = Result<Vec<Loc>, String>;
pub type GraphDepGraphResponse = Result<(), String>;
pub type InferTypeResponse = Result<infer_type::T, String>;
pub type InferTypeOfNameResponse = Result<infer_type_of_name::T, String>;
pub type InsertTypeResponse = Result<Patch, String>;
pub type CheckFileResponse = server_prot::response::CheckFileResponse;
pub type SuggestImportsResponse = Result<String, String>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum CliResponse {
    APPLY_CODE_ACTION(ApplyCodeActionResponse),
    AUTOCOMPLETE(AutocompleteResponse),
    AUTOFIX_EXPORTS(AutofixExportsResponse),
    AUTOFIX_MISSING_LOCAL_ANNOT(AutofixMissingLocalAnnotResponse),
    CHECK_FILE(CheckFileResponse),
    BATCH_COVERAGE(BatchCoverageResponse),
    COVERAGE(CoverageResponse),
    CYCLE(CycleResponse),
    QUERY(Result<String, String>),
    DUMP_TYPES(DumpTypesResponse),
    FIND_MODULE(FindModuleResponse),
    GET_DEF(GetDefResponse),
    GRAPH_DEP_GRAPH(GraphDepGraphResponse),
    INFER_TYPE(InferTypeResponse),
    INLAY_HINT(inlay_hint::Response),
    INSPECT_TYPES(inspect_types::Response),
    TYPE_OF_NAME(Vec<InferTypeOfNameResponse>),
    INSERT_TYPE(InsertTypeResponse),
    SUGGEST_IMPORTS(SuggestImportsResponse),
    STATUS {
        status_response: server_prot::response::StatusResponse,
        lazy_stats: LazyStats,
        unresolved_error_rounds: Option<u32>,
    },
}

impl CliResponse {
    pub fn try_from_server_response(
        response: server_prot::response::Response,
    ) -> Result<Self, String> {
        match response {
            server_prot::response::Response::APPLY_CODE_ACTION(response) => {
                Ok(Self::APPLY_CODE_ACTION(response))
            }
            server_prot::response::Response::AUTOCOMPLETE(response) => Ok(Self::AUTOCOMPLETE(
                response.map(|(completion, ac_type)| (completion.into(), ac_type)),
            )),
            server_prot::response::Response::AUTOFIX_EXPORTS(response) => {
                Ok(Self::AUTOFIX_EXPORTS(response))
            }
            server_prot::response::Response::AUTOFIX_MISSING_LOCAL_ANNOT(response) => {
                Ok(Self::AUTOFIX_MISSING_LOCAL_ANNOT(response))
            }
            server_prot::response::Response::CHECK_FILE(response) => Ok(Self::CHECK_FILE(response)),
            server_prot::response::Response::BATCH_COVERAGE(response) => {
                Ok(Self::BATCH_COVERAGE(response.map(|items| {
                    items
                        .into_iter()
                        .map(|(file_key, coverage)| (file_key.to_absolute(), coverage.into()))
                        .collect()
                })))
            }
            server_prot::response::Response::COVERAGE(response) => {
                Ok(Self::COVERAGE(response.map(|items| {
                    items
                        .into_iter()
                        .map(|(loc, kind)| (Loc::from(loc), kind.into()))
                        .collect()
                })))
            }
            server_prot::response::Response::CYCLE(response) => Ok(Self::CYCLE(response)),
            server_prot::response::Response::QUERY(response) => Ok(Self::QUERY(response)),
            server_prot::response::Response::DUMP_TYPES(response) => {
                Ok(Self::DUMP_TYPES(response.map(|(types, defs)| {
                    (
                        types
                            .into_iter()
                            .map(|(loc, ty)| (Loc::from(loc), ty))
                            .collect(),
                        defs,
                    )
                })))
            }
            server_prot::response::Response::FIND_MODULE((file_key, failed_candidates)) => {
                Ok(Self::FIND_MODULE((
                    file_key.map(|file_key| file_key.to_absolute()),
                    failed_candidates,
                )))
            }
            server_prot::response::Response::GET_DEF(response) => Ok(Self::GET_DEF(
                response.map(|locs| locs.into_iter().map(Loc::from).collect()),
            )),
            server_prot::response::Response::GRAPH_DEP_GRAPH(response) => {
                Ok(Self::GRAPH_DEP_GRAPH(response))
            }
            server_prot::response::Response::INFER_TYPE(response) => {
                Ok(Self::INFER_TYPE(response.map(infer_type::T::from)))
            }
            server_prot::response::Response::INLAY_HINT(response) => {
                Ok(Self::INLAY_HINT(response.map(|items| {
                    items.into_iter().map(inlay_hint::Item::from).collect()
                })))
            }
            server_prot::response::Response::INSPECT_TYPES(response) => {
                Ok(Self::INSPECT_TYPES(response.map(|items| {
                    items.into_iter().map(inspect_types::Item::from).collect()
                })))
            }
            server_prot::response::Response::TYPE_OF_NAME(response) => Ok(Self::TYPE_OF_NAME(
                response
                    .into_iter()
                    .map(|response| response.map(infer_type_of_name::T::from))
                    .collect(),
            )),
            server_prot::response::Response::INSERT_TYPE(response) => {
                Ok(Self::INSERT_TYPE(response))
            }
            server_prot::response::Response::SUGGEST_IMPORTS(response) => {
                Ok(Self::SUGGEST_IMPORTS(response))
            }
            server_prot::response::Response::STATUS {
                status_response,
                lazy_stats,
                unresolved_error_rounds,
            } => Ok(Self::STATUS {
                status_response,
                lazy_stats,
                unresolved_error_rounds,
            }),
            response => Err(format!(
                "Unsupported CLI socket response: {}",
                server_prot::response::to_string(&response)
            )),
        }
    }

    pub fn into_server_response(self) -> server_prot::response::Response {
        match self {
            CliResponse::APPLY_CODE_ACTION(response) => {
                server_prot::response::Response::APPLY_CODE_ACTION(response)
            }
            CliResponse::AUTOCOMPLETE(response) => server_prot::response::Response::AUTOCOMPLETE(
                response
                    .map(|(completion, ac_type)| (completion.into_server_completion(), ac_type)),
            ),
            CliResponse::AUTOFIX_EXPORTS(response) => {
                server_prot::response::Response::AUTOFIX_EXPORTS(response)
            }
            CliResponse::AUTOFIX_MISSING_LOCAL_ANNOT(response) => {
                server_prot::response::Response::AUTOFIX_MISSING_LOCAL_ANNOT(response)
            }
            CliResponse::CHECK_FILE(response) => {
                server_prot::response::Response::CHECK_FILE(response)
            }
            CliResponse::BATCH_COVERAGE(response) => {
                server_prot::response::Response::BATCH_COVERAGE(response.map(|items| {
                    items
                        .into_iter()
                        .map(|(filename, coverage)| {
                            (
                                FileKey::source_file_of_absolute(&filename),
                                coverage.into_server_coverage(),
                            )
                        })
                        .collect()
                }))
            }
            CliResponse::COVERAGE(response) => {
                server_prot::response::Response::COVERAGE(response.map(|items| {
                    items
                        .into_iter()
                        .map(|(loc, kind)| (loc.into_flow_loc(), kind.into_server_kind()))
                        .collect()
                }))
            }
            CliResponse::CYCLE(response) => server_prot::response::Response::CYCLE(response),
            CliResponse::QUERY(response) => server_prot::response::Response::QUERY(response),
            CliResponse::DUMP_TYPES(response) => {
                server_prot::response::Response::DUMP_TYPES(response.map(|(types, defs)| {
                    (
                        types
                            .into_iter()
                            .map(|(loc, ty)| (loc.into_flow_loc(), ty))
                            .collect(),
                        defs,
                    )
                }))
            }
            CliResponse::FIND_MODULE((file_key, failed_candidates)) => {
                server_prot::response::Response::FIND_MODULE((
                    file_key.map(|filename| FileKey::source_file_of_absolute(&filename)),
                    failed_candidates,
                ))
            }
            CliResponse::GET_DEF(response) => server_prot::response::Response::GET_DEF(
                response.map(|locs| locs.into_iter().map(Loc::into_flow_loc).collect()),
            ),
            CliResponse::GRAPH_DEP_GRAPH(response) => {
                server_prot::response::Response::GRAPH_DEP_GRAPH(response)
            }
            CliResponse::INFER_TYPE(response) => server_prot::response::Response::INFER_TYPE(
                response.map(infer_type::T::into_server_response),
            ),
            CliResponse::INLAY_HINT(response) => {
                server_prot::response::Response::INLAY_HINT(response.map(|items| {
                    items
                        .into_iter()
                        .map(inlay_hint::Item::into_server_item)
                        .collect()
                }))
            }
            CliResponse::INSPECT_TYPES(response) => {
                server_prot::response::Response::INSPECT_TYPES(response.map(|items| {
                    items
                        .into_iter()
                        .map(inspect_types::Item::into_server_item)
                        .collect()
                }))
            }
            CliResponse::TYPE_OF_NAME(response) => server_prot::response::Response::TYPE_OF_NAME(
                response
                    .into_iter()
                    .map(|response| response.map(infer_type_of_name::T::into_server_response))
                    .collect(),
            ),
            CliResponse::INSERT_TYPE(response) => {
                server_prot::response::Response::INSERT_TYPE(response)
            }
            CliResponse::SUGGEST_IMPORTS(response) => {
                server_prot::response::Response::SUGGEST_IMPORTS(response)
            }
            CliResponse::STATUS {
                status_response,
                lazy_stats,
                unresolved_error_rounds,
            } => server_prot::response::Response::STATUS {
                status_response,
                lazy_stats,
                unresolved_error_rounds,
            },
        }
    }
}

pub fn send_message<W: Write, T: Serialize>(writer: &mut W, msg: &T) -> std::io::Result<()> {
    flow_parser::loc::with_full_source_serde(|| {
        let json = serde_json::to_vec(msg)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let len = json.len() as u32;
        writer.write_all(&len.to_be_bytes())?;
        writer.write_all(&json)?;
        writer.flush()
    })
}

pub fn receive_message<R: Read, T: for<'de> Deserialize<'de>>(
    reader: &mut R,
) -> std::io::Result<T> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    flow_parser::loc::with_full_source_serde(|| {
        serde_json::from_slice(&buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::receive_message;

    #[test]
    fn receive_message_accepts_small_frame() {
        let payload = serde_json::json!({ "ok": true });
        let json = serde_json::to_vec(&payload).unwrap();
        let mut framed = (json.len() as u32).to_be_bytes().to_vec();
        framed.extend_from_slice(&json);

        let value: serde_json::Value = receive_message(&mut Cursor::new(framed)).unwrap();
        assert_eq!(value, payload);
    }
}
