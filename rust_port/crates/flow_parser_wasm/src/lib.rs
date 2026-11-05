/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! WASM entry point for the Flow Rust parser.
//!
//! Exports the same C ABI functions as Hermes (`hermesParse`, `hermesParseResult_*`)
//! so the existing `HermesParser.js` can load this WASM binary as a drop-in replacement.
//!
//! # Crate structure
//!
//! - [`babel_adapter`] — Babel-mode AST lowering and metadata collection.
//! - [`node_kinds`] — Schema-driven node definitions (`define_nodes!` macro).
//!   Single source of truth for node IDs, names, and property schemas.
//! - [`serializer`] — Binary encoder: Flow AST → `programBuffer` (Vec<u32>).
//! - [`position`] — Source position tracking (UTF-16 offsets for JS).
//! - `bin/codegen.rs` — Generates `FlowParserNodeDeserializers.js` from the schema.
//!   Run via `buck run fbcode//flow/rust_port/crates/flow_parser_wasm:codegen`.

pub mod babel_adapter;
pub mod node_kinds;
pub mod position;
pub mod runnable_js;
pub mod serializer;
pub mod strip_flow;

use std::ffi::CString;

use flow_parser::TokenSinkResult;
use flow_parser::ast;
use flow_parser::ast::statement::ExportKind;
use flow_parser::ast::statement::ImportKind;
use flow_parser::ast::statement::StatementInner;
use flow_parser::ast::statement::export_named_declaration::Specifier as ExportSpecifier;
use flow_parser::ast::statement::import_declaration::Specifier as ImportSpecifier;
use flow_parser::loc::Loc;
use position::PositionResult;
#[cfg(not(target_arch = "wasm32"))]
pub use runnable_js::flow_to_runnable_js_with_source_map;

/// Source type encoded by the JavaScript parser's integer ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum SourceType {
    Unspecified = 0,
    Script = 1,
    Module = 2,
}

impl TryFrom<i32> for SourceType {
    type Error = i32;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Unspecified),
            1 => Ok(Self::Script),
            2 => Ok(Self::Module),
            other => Err(other),
        }
    }
}

/// Opaque parse result stored in WASM memory.
pub struct ParseResult {
    error: Option<CString>,
    error_line: u32,
    error_column: u32,
    program_buffer: Vec<u32>,
    position_buffer: Vec<PositionResult>,
    /// Concatenated UTF-8/WTF-8 bytes for every string written by the
    /// serializer. The program buffer encodes string references as
    /// `(offset+1, len)` into this buffer, with 0 reserved as the null
    /// sentinel. The buffer must stay alive until the ParseResult is
    /// freed because the JS-side deserializer dereferences pointers
    /// computed from the base pointer + offset.
    string_buffer: Vec<u8>,
}

const BABEL_LOWERING_ERROR_PREFIX: &str = "$FlowBabelLoweringError$";

/// Saturate a possibly-negative line/column (`Loc.none` uses `-1`) to a
/// nonnegative `u32`. A naive `as u32` cast wraps `-1` to `4294967295`,
/// which is meaningless on the JS side (and worse, looks like a real
/// position). Clamp to 0 instead so callers see "line 0 / col 0" for
/// unknown locations.
fn nonneg_u32(v: i32) -> u32 {
    v.max(0) as u32
}

/// Parse source and return an opaque ParseResult pointer.
///
/// Arguments match the Hermes WASM API plus an extra `source_filename`
/// pair for plumbing the source path through to the parser. The path is
/// used for file-extension based parser modes (`.flow`, `.d.ts`,
/// `.d.cts`, `.d.mts`) — without it, ambient detection is impossible and
/// declarations like bodyless class methods inside `.d.ts` cannot be
/// classified correctly.
///
/// - source: pointer to UTF-8 source text in WASM memory (null-terminated)
/// - source_size: length including null terminator
/// - source_filename: pointer to UTF-8 filename in WASM memory (null-terminated, may be null)
/// - source_filename_size: length including null terminator (0 if filename is null)
/// - enable_components: enable component syntax
/// - enable_match: enable match expression syntax
/// - enable_decorators: enable Stage-1 decorator syntax (when 0, `@` is a parse error)
/// - tokens: if true, serialize token information
/// - allow_return_outside: when non-zero, suppress the `IllegalReturn`
///   diagnostic for top-level `return` statements. Mirrors hermes-parser
///   `allowReturnOutsideFunction`.
/// - enable_enums: enable Flow enum syntax (off by default per OCaml `default_parse_options`)
/// - enable_records: enable Flow record syntax (`#{ ... }`); off by default
/// - enable_types: enable Flow/TS type-grammar (on by default; fixtures opt out via `types: false`)
/// - enable_types_pragma_detection: when non-zero, scan the docblock for
///   `@flow` and override `enable_types` based on whether the pragma is
///   present (matches Hermes' C++ pragma scan in
///   `xplat/static_h/lib/Parser/FlowHelpers.cpp`). When zero, `enable_types`
///   is honored verbatim. The pragma scan walks leading whitespace, `//` and
///   `/* */` comments, and directive-prologue string literals up to the
///   first non-directive token; in each collected comment it searches for
///   `@flow` followed by a non-word boundary.
/// - enable_types_in_comments: ignored compatibility slot. This Hermes-compatible
///   entry point always treats Flow's legacy comment syntax (`/*: T */`,
///   `/*:: ... */`) as plain JavaScript comments.
/// - source_type: integer encoding of [`SourceType`].
///   Modules parse in strict mode with top-level await enabled.
#[unsafe(no_mangle)]
pub extern "C" fn hermesParse(
    source: *const u8,
    source_size: usize,
    source_filename: *const u8,
    source_filename_size: usize,
    enable_components: i32,
    enable_match: i32,
    enable_decorators: i32,
    tokens: i32,
    allow_return_outside: i32,
    enable_enums: i32,
    enable_records: i32,
    enable_types: i32,
    source_type: i32,
    enable_types_pragma_detection: i32,
    _enable_types_in_comments: i32,
    babel: i32,
    lower_enums: i32,
    custom_enum_runtime: i32,
    react_runtime_target: i32,
    throw_on_parse_errors: i32,
) -> *mut ParseResult {
    let source_type = match SourceType::try_from(source_type) {
        Ok(source_type) => source_type,
        Err(other) => panic!(
            "hermesParse: invalid source_type={other}; expected 0 (unspecified), \
             1 (script), or 2 (module)."
        ),
    };

    // SAFETY: The JS bridge in flow-parser/oxidized-src/FlowParser.js wraps
    // every call to `hermesParse` in a try/finally that allocates `source`
    // via `_malloc`, copies `source_size` bytes into it, calls this
    // function, and frees the allocation in `finally`. The pointer is
    // therefore valid for `source_size` bytes for the duration of this
    // call. `source_size` is the JS-side `Buffer.length + 1` (UTF-8 source
    // plus a null terminator), so the slice is in-bounds.
    let source_bytes = unsafe { std::slice::from_raw_parts(source, source_size) };
    // Source is null-terminated; exclude the null for parsing
    let source_len = if source_size > 0 { source_size - 1 } else { 0 };
    let source_str = match std::str::from_utf8(&source_bytes[..source_len]) {
        Ok(s) => s,
        Err(_) => {
            let result = Box::new(ParseResult {
                error: CString::new("Invalid UTF-8 in source").ok(),
                error_line: 1,
                error_column: 0,
                program_buffer: Vec::new(),
                position_buffer: Vec::new(),
                string_buffer: Vec::new(),
            });
            return Box::into_raw(result);
        }
    };

    // Decode the optional filename. Mirrors source decoding — null pointer or
    // zero-length means "no filename", which collapses to an empty `SourceFile`
    // (matching the original behavior). Invalid UTF-8 is treated the same way
    // rather than failing the whole parse, since the filename only affects
    // file-extension dispatch.
    let filename_str: String = if source_filename.is_null() || source_filename_size == 0 {
        String::new()
    } else {
        // SAFETY: When non-null, the JS bridge mallocs and copies
        // `filename` into WASM memory before this call (see
        // FlowParser.js:154-159) and frees it in the same `finally` block,
        // so the pointer is valid for `source_filename_size` bytes for the
        // duration of this call. Length includes the null terminator.
        let filename_bytes =
            unsafe { std::slice::from_raw_parts(source_filename, source_filename_size) };
        let len = if source_filename_size > 0 {
            source_filename_size - 1
        } else {
            0
        };
        match std::str::from_utf8(&filename_bytes[..len]) {
            Ok(s) => s.to_string(),
            Err(_) => String::new(),
        }
    };

    if babel == 2 {
        let emitted = serde_json::from_str::<runnable_js::SourceMetadata>(&filename_str)
            .map_err(runnable_js::RunnableError::Options)
            .and_then(|metadata| {
                runnable_js::flow_to_runnable_js_with_source_map(
                    source_str,
                    &metadata.source_filename,
                    &runnable_js::RunnableOptions {
                        enum_runtime_module: metadata.enum_runtime_module,
                        source_type: match source_type {
                            SourceType::Module => runnable_js::SourceType::Module,
                            SourceType::Unspecified | SourceType::Script => {
                                runnable_js::SourceType::CommonJs
                            }
                        },
                        react_runtime_target: if react_runtime_target == 19 {
                            runnable_js::ReactRuntimeTarget::React19
                        } else {
                            runnable_js::ReactRuntimeTarget::React18
                        },
                        allow_return_outside_function: allow_return_outside != 0,
                    },
                )
            });
        let mut result = Box::new(ParseResult {
            error: None,
            error_line: 0,
            error_column: 0,
            program_buffer: Vec::new(),
            position_buffer: Vec::new(),
            string_buffer: Vec::new(),
        });
        match emitted {
            Ok(emitted) => match serde_json::to_vec(&emitted) {
                Ok(bytes) => match u32::try_from(bytes.len()) {
                    Ok(length) => {
                        result.program_buffer.push(length);
                        result.string_buffer = bytes;
                    }
                    Err(error) => {
                        result.error = Some(
                            CString::new(error.to_string())
                                .expect("integer conversion error contains no NUL"),
                        );
                    }
                },
                Err(error) => {
                    result.error = Some(
                        CString::new(error.to_string().replace('\0', "\\0"))
                            .expect("NUL bytes were escaped"),
                    );
                }
            },
            Err(error) => {
                if let runnable_js::RunnableError::Syntax { loc, .. } = &error {
                    result.error_line = nonneg_u32(loc.line);
                    result.error_column = nonneg_u32(loc.column);
                }
                result.error = Some(
                    CString::new(error.to_string().replace('\0', "\\0"))
                        .expect("NUL bytes were escaped"),
                );
            }
        }
        return Box::into_raw(result);
    }

    // When pragma detection is requested, mirror upstream Hermes'
    // ParseFlowSetting::ALL vs ParseFlowSetting::UNAMBIGUOUS split:
    // no pragma still enables unambiguous Flow syntax such as `import type`,
    // while ambiguous call/new type-argument syntax stays disabled.
    let (resolved_enable_types, resolved_enable_ambiguous_types) =
        if enable_types_pragma_detection != 0 {
            (true, has_flow_pragma(source_str))
        } else {
            let enable_types = enable_types != 0;
            (enable_types, enable_types)
        };

    let parse_options = flow_parser::ParseOptions {
        components: enable_components != 0,
        enums: enable_enums != 0,
        pattern_matching: enable_match != 0,
        records: enable_records != 0,
        esproposal_decorators: enable_decorators != 0,
        types: resolved_enable_types,
        ambiguous_types: resolved_enable_ambiguous_types,
        enable_types_in_comments: false,
        use_strict: false,
        module_ref_prefix: None,
        ambient: false,
        allow_return_outside_function: allow_return_outside != 0,
    };

    let file_key = flow_parser::file_key::FileKey::new(
        flow_parser::file_key::FileKeyInner::SourceFile(filename_str),
    );

    // When the JS caller asks for `tokens: true`, install a token sink that
    // captures every lexed `TokenSinkResult` for the duration of the parse.
    // The buffer is owned here, mutated by the sink callback, then handed to
    // the serializer below. When tokens are not requested we pass `None` to
    // the parser to avoid the per-token callback overhead.
    let mut token_buffer: Vec<TokenSinkResult> = Vec::new();
    let mut sink = |token: TokenSinkResult| token_buffer.push(token);
    let token_sink = (tokens != 0).then_some(&mut sink as &mut dyn FnMut(TokenSinkResult));
    let (program, errors) = match source_type {
        SourceType::Module => flow_parser::parse_module_file_with_errors(
            token_sink,
            parse_options,
            file_key,
            source_str,
        ),
        SourceType::Unspecified | SourceType::Script => flow_parser::parse_program_file::<()>(
            false, // panic_if_failed
            token_sink,
            Some(parse_options),
            file_key,
            Ok(source_str),
        ),
    };

    let mut result = Box::new(ParseResult {
        error: None,
        error_line: 0,
        error_column: 0,
        program_buffer: Vec::new(),
        position_buffer: Vec::new(),
        string_buffer: Vec::new(),
    });

    let lowered_program = if babel != 0 {
        let enum_runtime = if custom_enum_runtime != 0 {
            babel_adapter::EnumRuntime::CustomPlaceholder
        } else {
            babel_adapter::EnumRuntime::Default
        };
        let react_runtime_target = if react_runtime_target == 19 {
            babel_adapter::ReactRuntimeTarget::React19
        } else {
            babel_adapter::ReactRuntimeTarget::React18
        };
        match babel_adapter::lower_program(
            source_str,
            &program,
            &babel_adapter::BabelLoweringOptions {
                lower_enums: lower_enums != 0,
                enum_runtime,
                react_runtime_target,
            },
        ) {
            Ok(program) => Some(program),
            Err(babel_adapter::BabelLoweringError::Syntax { message, loc }) => {
                result.error = Some(
                    CString::new(format!("{BABEL_LOWERING_ERROR_PREFIX}{message}"))
                        .expect("Babel lowering error messages cannot contain NUL bytes"),
                );
                result.error_line = nonneg_u32(loc.start.line);
                result.error_column = nonneg_u32(loc.start.column);
                None
            }
        }
    } else {
        None
    };

    // Always serialize the (possibly partial) program so JS-side consumers
    // see the full `errors` array; only surface the first error via the
    // single-error C ABI for callers that don't read the array.
    let lowering_failed = babel != 0 && lowered_program.is_none();
    if !lowering_failed && let Some((loc, err)) = errors.first() {
        result.error = CString::new(format!("{}", err)).ok();
        result.error_line = nonneg_u32(loc.start.line);
        result.error_column = nonneg_u32(loc.start.column);
    }
    if babel != 0 && throw_on_parse_errors == 0 && !lowering_failed {
        result.error = None;
        result.error_line = 0;
        result.error_column = 0;
    }
    let token_slice = if tokens != 0 {
        Some(token_buffer.as_slice())
    } else {
        None
    };
    let buffers = match &lowered_program {
        Some(lowered) => {
            serializer::Serializer::new_babel(source_str, &lowered.metadata, source_type)
                .serialize_program(&lowered.program, &errors, token_slice)
        }
        None => serializer::Serializer::new(source_str).serialize_program(
            &program,
            &errors,
            token_slice,
        ),
    };
    result.program_buffer = buffers.program_buffer;
    result.position_buffer = buffers.position_buffer;
    result.string_buffer = buffers.string_buffer;

    Box::into_raw(result)
}

#[unsafe(no_mangle)]
pub extern "C" fn hermesParseResult_free(result: *mut ParseResult) {
    if !result.is_null() {
        // SAFETY: Non-null path. The pointer was obtained from
        // `Box::into_raw` in `hermesParse` (the only producer of a
        // `*mut ParseResult` in this crate), so reconstituting the
        // `Box<ParseResult>` here is the matching `Box::from_raw` for
        // that allocation. Each ParseResult is freed exactly once: the
        // JS bridge calls `_hermesParseResult_free` from a `finally`
        // block and then drops its handle (FlowParser.js:215-217).
        unsafe {
            drop(Box::from_raw(result));
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn hermesParseResult_getError(result: *const ParseResult) -> *const u8 {
    if result.is_null() {
        return std::ptr::null();
    }
    // SAFETY: Non-null per the check above. `result` was produced by
    // `Box::into_raw(Box<ParseResult>)` in `hermesParse` and remains
    // valid until `hermesParseResult_free` runs; the JS bridge
    // (FlowParser.js:148-217) only calls getters between the producing
    // `flowParse` call and the matching `flowParseResult_free` in the
    // same `finally` block. The shared reference is dropped before the
    // function returns, so it cannot outlive the allocation.
    let result = unsafe { &*result };
    match &result.error {
        Some(err) => err.as_ptr() as *const u8,
        None => std::ptr::null(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn hermesParseResult_getErrorLine(result: *const ParseResult) -> u32 {
    if result.is_null() {
        return 0;
    }
    // SAFETY: Same lifetime contract as `hermesParseResult_getError`.
    unsafe { (*result).error_line }
}

#[unsafe(no_mangle)]
pub extern "C" fn hermesParseResult_getErrorColumn(result: *const ParseResult) -> u32 {
    if result.is_null() {
        return 0;
    }
    // SAFETY: Same lifetime contract as `hermesParseResult_getError`.
    unsafe { (*result).error_column }
}

#[unsafe(no_mangle)]
pub extern "C" fn hermesParseResult_getProgramBuffer(result: *const ParseResult) -> *const u32 {
    if result.is_null() {
        return std::ptr::null();
    }
    // SAFETY: Same lifetime contract as `hermesParseResult_getError`.
    let result = unsafe { &*result };
    // Empty buffer signals "the serializer never produced anything" (e.g.
    // invalid UTF-8); represent that as a null pointer to JS.
    if result.program_buffer.is_empty() {
        return std::ptr::null();
    }
    result.program_buffer.as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn hermesParseResult_getPositionBuffer(result: *const ParseResult) -> *const u32 {
    if result.is_null() {
        return std::ptr::null();
    }
    // SAFETY: Same lifetime contract as `hermesParseResult_getError`.
    let result = unsafe { &*result };
    if result.program_buffer.is_empty() {
        return std::ptr::null();
    }
    // PositionResult is #[repr(C)] with 5 x u32, so we can reinterpret the pointer
    result.position_buffer.as_ptr() as *const u32
}

#[unsafe(no_mangle)]
pub extern "C" fn hermesParseResult_getPositionBufferSize(result: *const ParseResult) -> usize {
    if result.is_null() {
        return 0;
    }
    // SAFETY: Same lifetime contract as `hermesParseResult_getError`.
    let result = unsafe { &*result };
    if result.program_buffer.is_empty() {
        return 0;
    }
    result.position_buffer.len()
}

/// Return a pointer to the contiguous UTF-8/WTF-8 byte buffer holding all
/// strings referenced by the program buffer. The program buffer encodes
/// each non-null string reference as `(offset+1, len)` into this buffer
/// (and `(0,)` for null strings). The JS deserializer reads
/// `actual_ptr = base + (encoded_offset - 1)` for non-null entries.
///
/// Returns null when the result is null or the string buffer is empty.
#[unsafe(no_mangle)]
pub extern "C" fn hermesParseResult_getStringBuffer(result: *const ParseResult) -> *const u8 {
    if result.is_null() {
        return std::ptr::null();
    }
    // SAFETY: Same lifetime contract as `hermesParseResult_getError`.
    let result = unsafe { &*result };
    if result.string_buffer.is_empty() {
        return std::ptr::null();
    }
    result.string_buffer.as_ptr()
}

/// Wasm entry point for runnable emission. Source buffers follow the same
/// contract as `hermesParse`; the result uses its existing ownership/getters.
/// Source metadata is UTF-8 JSON with `sourceFilename` and optional
/// `enumRuntimeModule`, in the existing filename buffer slot.
/// The program buffer contains the JSON byte length; the string buffer holds
/// the serialized `RunnableJs`. Source type is 1 for CommonJS or 2 for ESM.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn flow_to_runnable_js_with_source_map(
    source: *const u8,
    source_size: usize,
    source_metadata: *const u8,
    source_metadata_size: usize,
    source_type: i32,
    react_runtime_target: i32,
    allow_return_outside: i32,
) -> *mut ParseResult {
    hermesParse(
        source,
        source_size,
        source_metadata,
        source_metadata_size,
        1,
        1,
        0,
        0,
        allow_return_outside,
        1,
        1,
        1,
        source_type,
        0,
        0,
        2,
        1,
        0,
        react_runtime_target,
        1,
    )
}

/// Scan `code`'s docblock for an `@flow` pragma. Mirrors C++ hermes-parser
/// `parser::hasFlowPragma` + `parser::getCommentsInDocBlock` semantics
/// (`xplat/static_h/lib/Parser/FlowHelpers.cpp:17-82`): walk leading
/// whitespace, `//` and `/* ... */` comments, and directive-prologue string
/// literals (each optionally followed by `;`) up to the first non-directive
/// token, then in each collected comment search for `@flow` followed by a
/// non-word boundary (end of comment, or any character outside
/// `[A-Za-z0-9_]`).
fn has_flow_pragma(code: &str) -> bool {
    let bytes = code.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        let ch = bytes[i];
        // Whitespace
        if ch == b' ' || ch == b'\t' || ch == b'\n' || ch == b'\r' {
            i += 1;
            continue;
        }
        // Comments
        if ch == b'/' && i + 1 < len {
            let next = bytes[i + 1];
            if next == b'/' {
                i += 2;
                let start = i;
                while i < len {
                    let c = bytes[i];
                    if c == b'\n' || c == b'\r' {
                        break;
                    }
                    i += 1;
                }
                if comment_has_flow_pragma(bytes, start, i) {
                    return true;
                }
                continue;
            }
            if next == b'*' {
                i += 2;
                let start = i;
                while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                let end = i;
                if i + 1 < len {
                    i += 2; // skip the closing `*/`
                }
                if comment_has_flow_pragma(bytes, start, end) {
                    return true;
                }
                continue;
            }
        }
        // String-literal directive prologue
        if ch == b'"' || ch == b'\'' {
            let quote = ch;
            i += 1;
            while i < len {
                let c = bytes[i];
                if c == b'\\' && i + 1 < len {
                    // Skip the escape character; common case is `\"`/`\\`/`\n`.
                    i += 2;
                    continue;
                }
                if c == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        // Optional `;` between directives
        if ch == b';' {
            i += 1;
            continue;
        }
        // First non-directive token: stop. Anything past this point is
        // regular code, not docblock.
        break;
    }
    false
}

pub(crate) fn is_module(program: &ast::Program<Loc, Loc>) -> bool {
    program
        .statements
        .iter()
        .any(|statement| match &**statement {
            StatementInner::ImportDeclaration { inner, .. } => {
                matches!(inner.import_kind, ImportKind::ImportValue)
                    && (inner.default.is_some()
                        || match &inner.specifiers {
                            Some(ImportSpecifier::ImportNamedSpecifiers(specifiers)) => {
                                specifiers.is_empty()
                                    || specifiers.iter().any(|specifier| {
                                        specifier.kind.is_none_or(|kind| {
                                            matches!(kind, ImportKind::ImportValue)
                                        })
                                    })
                            }
                            _ => true,
                        })
            }
            StatementInner::ExportDefaultDeclaration { inner, .. } => {
                matches!(inner.export_kind(), ExportKind::ExportValue)
            }
            StatementInner::ExportNamedDeclaration { inner, .. } => {
                matches!(inner.export_kind, ExportKind::ExportValue)
                    && match &inner.specifiers {
                        Some(ExportSpecifier::ExportSpecifiers(specifiers)) => {
                            specifiers.is_empty()
                                || specifiers.iter().any(|specifier| {
                                    matches!(specifier.export_kind, ExportKind::ExportValue)
                                })
                        }
                        _ => true,
                    }
            }
            _ => false,
        })
}

fn comment_has_flow_pragma(bytes: &[u8], start: usize, end: usize) -> bool {
    let needle = b"@flow";
    let mut from = start;
    while from + needle.len() <= end {
        // Manual indexOf (no allocations).
        let mut found_at = None;
        let mut search = from;
        while search + needle.len() <= end {
            if &bytes[search..search + needle.len()] == needle {
                found_at = Some(search);
                break;
            }
            search += 1;
        }
        let idx = match found_at {
            Some(i) => i,
            None => return false,
        };
        let after = idx + needle.len();
        if after >= end {
            return true;
        }
        let c = bytes[after];
        let is_word = c.is_ascii_digit() || c.is_ascii_alphabetic() || c == b'_';
        if !is_word {
            return true;
        }
        from = idx + 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use crate::node_kinds::NodeKind;
    use crate::node_kinds::PropType;
    use crate::node_kinds::SCHEMA;
    use crate::serializer::Serializer;

    #[test]
    fn type_use_schemas_distinguish_estree_and_babel_fields() {
        for (estree, babel) in [
            (
                NodeKind::GenericTypeAnnotation,
                NodeKind::BabelGenericTypeAnnotation,
            ),
            (NodeKind::InterfaceExtends, NodeKind::BabelInterfaceExtends),
            (NodeKind::ClassImplements, NodeKind::BabelClassImplements),
        ] {
            let estree = &SCHEMA[estree as usize];
            let babel = &SCHEMA[babel as usize];
            assert_eq!(estree.estree_type, babel.estree_type);
            assert_eq!(
                estree.properties,
                &[("id", PropType::Node), ("typeArguments", PropType::Node)]
            );
            assert_eq!(
                babel.properties,
                &[("id", PropType::Node), ("typeParameters", PropType::Node)]
            );
        }
    }

    #[test]
    fn default_interface_exports_do_not_select_module_source_type() {
        // A type-only default export leaves module detection in script mode.
        let (program, errors) = flow_parser::parse_program_without_file(
            false,
            None,
            None,
            Ok("export default interface I {}"),
        );
        assert!(errors.is_empty(), "parse errors: {errors:?}"); // OK
        assert!(!super::is_module(&program)); // OK

        // Value exports still select modules alongside a default interface export.
        for source in [
            "export default interface I {} export const value = 1;",
            "export default interface I {} export default 1;",
        ] {
            let (program, errors) =
                flow_parser::parse_program_without_file(false, None, None, Ok(source));
            assert!(errors.is_empty(), "parse errors for {source}: {errors:?}"); // OK
            assert!(super::is_module(&program)); // OK
        }
    }

    #[test]
    fn default_exports_serialize_type_and_value_kinds() {
        // Only interface declarations are type-only default exports.
        for (source, expected) in [
            ("export default interface I {}", "type"),
            ("export default function F() {}", "value"),
            ("export default class C {}", "value"),
            ("export default I;", "value"),
            ("export default 1;", "value"),
            ("export default null;", "value"),
            ("export default true;", "value"),
            ("export default 'text';", "value"),
            ("export default 1n;", "value"),
            ("export default /x/;", "value"),
        ] {
            let buffers = parse_and_serialize_with_filename(source, "");
            assert_eq!(
                string_prop(&buffers, NodeKind::ExportDefaultDeclaration, "exportKind"),
                expected,
                "{source}",
            ); // OK
        }
    }

    #[test]
    fn module_detection_respects_specifier_kinds() {
        // Specifier-level type modifiers do not select module source type.
        for source in [
            "import {type T} from './types';",
            "import {typeof T} from './types';",
            "import {type T, typeof U} from './types';",
            "export {type T};",
            "export {type T as default};",
            "export {type T, type U} from './types';",
        ] {
            let (program, errors) =
                flow_parser::parse_program_without_file(false, None, None, Ok(source));
            assert!(errors.is_empty(), "parse errors for {source}: {errors:?}"); // OK
            assert!(!super::is_module(&program), "{source}"); // OK
        }

        // Runtime specifiers, side effects, and empty export markers select modules.
        for source in [
            "import './value';",
            "import {} from './value';",
            "import value, {type T} from './value';",
            "import * as value from './value';",
            "import {type T, value} from './value';",
            "import {type} from './value';",
            "export {};",
            "export {} from './value';",
            "export {type T, value};",
            "export {type};",
            "export * as value from './value';",
        ] {
            let (program, errors) =
                flow_parser::parse_program_without_file(false, None, None, Ok(source));
            assert!(errors.is_empty(), "parse errors for {source}: {errors:?}"); // OK
            assert!(super::is_module(&program), "{source}"); // OK
        }
    }

    /// Parse `source`, serialize it, and return the program_buffer.
    fn parse_and_serialize(source: &str) -> Vec<u32> {
        parse_and_serialize_with_filename(source, "").program_buffer
    }

    fn parse_and_serialize_with_filename(
        source: &str,
        filename: &str,
    ) -> crate::serializer::SerializerBuffers {
        let parse_options = flow_parser::ParseOptions {
            components: true,
            enums: true,
            pattern_matching: true,
            records: true,
            esproposal_decorators: true,
            types: true,
            ambiguous_types: true,
            enable_types_in_comments: false,
            use_strict: false,
            module_ref_prefix: None,
            ambient: false,
            allow_return_outside_function: false,
        };
        let file_key = flow_parser::file_key::FileKey::new(
            flow_parser::file_key::FileKeyInner::SourceFile(filename.to_string()),
        );
        let (program, errors) = flow_parser::parse_program_file::<()>(
            false,
            None,
            Some(parse_options),
            file_key,
            Ok(source),
        );
        assert!(
            errors.is_empty(),
            "parse errors for {:?}: {:?}",
            source,
            errors
        );
        let ser = Serializer::new(source);
        ser.serialize_program(&program, &[], None)
    }

    fn skip_string(buf: &[u32], idx: usize) -> usize {
        if buf[idx] == 0 { idx + 1 } else { idx + 2 }
    }

    fn skip_property(buf: &[u32], idx: usize, prop_type: PropType) -> usize {
        match prop_type {
            PropType::Node | PropType::OptionalNode => skip_node(buf, idx),
            PropType::NodeList | PropType::NonEmptyNodeList => {
                let len = buf[idx] as usize;
                (0..len).fold(idx + 1, |cursor, _| skip_node(buf, cursor))
            }
            PropType::String | PropType::MaybeString => skip_string(buf, idx),
            PropType::Boolean | PropType::TrueBoolean => idx + 1,
            PropType::Number => {
                let aligned = if idx.is_multiple_of(2) { idx } else { idx + 1 };
                aligned + 2
            }
            PropType::MaybeNode => {
                if buf[idx] == 0 {
                    idx + 1
                } else {
                    skip_node(buf, idx + 1)
                }
            }
            PropType::MaybeBoolean => {
                if buf[idx] == 0 {
                    idx + 1
                } else {
                    idx + 2
                }
            }
            PropType::CommentList => {
                let len = buf[idx] as usize;
                (0..len).fold(idx + 1, |cursor, _| {
                    let cursor = skip_string(buf, cursor);
                    let cursor = cursor + 2;
                    skip_string(buf, cursor)
                })
            }
        }
    }

    fn skip_node(buf: &[u32], idx: usize) -> usize {
        if buf[idx] == 0 {
            return idx + 1;
        }

        let kind_id = buf[idx] - 1;
        if kind_id == NodeKind::Literal as u32 {
            // Literals use a value discriminator rather than schema properties.
            let mut cursor = idx + 3;
            cursor = match buf[idx + 2] {
                0 => cursor,
                1 => skip_property(buf, cursor, PropType::Boolean),
                2 => skip_property(buf, cursor, PropType::Number),
                3 => skip_property(buf, cursor, PropType::String),
                kind => panic!("unknown literal value kind {kind}"),
            };
            // literalType, raw, bigint, regex pattern, and regex flags follow.
            for _ in 0..5 {
                cursor = skip_string(buf, cursor);
            }
            return cursor;
        }
        let def = SCHEMA
            .iter()
            .find(|def| def.kind_id == kind_id)
            .expect("serialized node kind should exist in schema");
        let mut cursor = idx + 2;
        for (_, prop_type) in def
            .extra_pre_props
            .iter()
            .chain(def.properties.iter())
            .chain(def.extra_post_props.iter())
        {
            cursor = skip_property(buf, cursor, *prop_type);
        }
        cursor
    }

    fn node_property_idx(buf: &[u32], kind: NodeKind, prop_name: &str) -> usize {
        let node_idx = buf
            .iter()
            .position(|v| *v == encoded_kind(kind))
            .expect("expected declaration node header in buffer");
        let def = SCHEMA
            .iter()
            .find(|def| def.kind_id == kind as u32)
            .expect("serialized node kind should exist in schema");
        let props = def
            .extra_pre_props
            .iter()
            .chain(def.properties.iter())
            .chain(def.extra_post_props.iter());
        let mut cursor = node_idx + 2;
        for (name, prop_type) in props {
            if *name == prop_name {
                return cursor;
            }
            cursor = skip_property(buf, cursor, *prop_type);
        }
        panic!("{kind:?}.{prop_name} should exist in schema");
    }

    fn bool_prop(buf: &[u32], kind: NodeKind, prop_name: &str) -> bool {
        buf[node_property_idx(buf, kind, prop_name)] == 1
    }

    fn string_prop<'a>(
        buffers: &'a crate::serializer::SerializerBuffers,
        kind: NodeKind,
        prop_name: &str,
    ) -> &'a str {
        optional_string_prop(buffers, kind, prop_name).expect("string property should be non-null")
    }

    fn optional_string_prop<'a>(
        buffers: &'a crate::serializer::SerializerBuffers,
        kind: NodeKind,
        prop_name: &str,
    ) -> Option<&'a str> {
        let idx = node_property_idx(&buffers.program_buffer, kind, prop_name);
        let offset = buffers.program_buffer[idx].checked_sub(1)? as usize;
        let len = buffers.program_buffer[idx + 1] as usize;
        Some(
            std::str::from_utf8(&buffers.string_buffer[offset..offset + len])
                .expect("serialized strings should be UTF-8"),
        )
    }

    /// Returns true if any u32 in the buffer equals (kind as u32 + 1),
    /// which is the encoding used by `write_node_header`.
    fn buffer_contains_kind(buf: &[u32], kind: NodeKind) -> bool {
        let needle = kind as u32 + 1;
        buf.contains(&needle)
    }

    fn encoded_kind(kind: NodeKind) -> u32 {
        kind as u32 + 1
    }

    #[test]
    fn variance_serializes_in_out_with_space() {
        let buffers = parse_and_serialize_with_filename("type T<in out A> = A;", "");
        assert_eq!(string_prop(&buffers, NodeKind::Variance, "kind"), "in out");
    }

    #[test]
    fn mapped_type_serializes_readonly_polarity_and_key_remapping() {
        for (source, expected_op) in [
            ("type T<O> = { readonly [K in keyof O]: O[K] };", None),
            ("type T<O> = { +readonly [K in keyof O]: O[K] };", Some("+")),
            ("type T<O> = { -readonly [K in keyof O]: O[K] };", Some("-")),
        ] {
            let buffers = parse_and_serialize_with_filename(source, "");
            assert_eq!(
                optional_string_prop(
                    &buffers,
                    NodeKind::ObjectTypeMappedTypeProperty,
                    "varianceOp",
                ),
                expected_op,
            );
        }

        let buffers = parse_and_serialize("type T<O> = { [K in keyof O as K]: O[K] };");
        let name_type_idx =
            node_property_idx(&buffers, NodeKind::ObjectTypeMappedTypeProperty, "nameType");
        assert_eq!(
            buffers[name_type_idx],
            encoded_kind(NodeKind::GenericTypeAnnotation),
            "ObjectTypeMappedTypeProperty.nameType should serialize the remapping type",
        );
    }

    #[test]
    fn class_nodes_serialize_abstract_modifier() {
        let declaration = parse_and_serialize("abstract class C {}");
        assert!(bool_prop(
            &declaration,
            NodeKind::ClassDeclaration,
            "abstract",
        ));

        let expression = parse_and_serialize("const C = abstract class {};");
        assert!(bool_prop(
            &expression,
            NodeKind::ClassExpression,
            "abstract",
        ));

        let ambient = parse_and_serialize("declare abstract class C {}");
        assert!(bool_prop(&ambient, NodeKind::DeclareClass, "abstract"));

        let concrete = parse_and_serialize("class C {}");
        assert!(!bool_prop(
            &concrete,
            NodeKind::ClassDeclaration,
            "abstract",
        ));
    }

    #[test]
    fn declare_methods_serialize_accessibility() {
        for filename in ["test.ts", "test.tsx", "test.mts", "test.cts", "test.d.ts"] {
            for accessibility in [Some("private"), Some("protected"), Some("public"), None] {
                let modifier = accessibility.unwrap_or("");
                for method in ["m(x: number): void;", "m?<T>(x: T): T;"] {
                    let source = format!("class C {{ {modifier} {method} }}");
                    let buffers = parse_and_serialize_with_filename(&source, filename);
                    assert_eq!(
                        optional_string_prop(
                            &buffers,
                            NodeKind::DeclareMethodDefinition,
                            "tsAccessibility",
                        ),
                        accessibility,
                        "{filename}: {source}",
                    );
                }
            }
        }
    }

    #[test]
    fn ambient_accessors_serialize_accessibility() {
        for accessibility in ["private", "protected", "public"] {
            for (method, kind) in [
                ("get value(): number;", "get"),
                ("set value(x: number);", "set"),
            ] {
                let source = format!("export class C {{ {accessibility} {method} }}");
                let buffers = parse_and_serialize_with_filename(&source, "test.d.ts");
                assert_eq!(
                    string_prop(
                        &buffers,
                        NodeKind::DeclareMethodDefinition,
                        "tsAccessibility"
                    ),
                    accessibility,
                );
                assert_eq!(
                    string_prop(&buffers, NodeKind::DeclareMethodDefinition, "kind"),
                    kind,
                );
            }
        }
    }

    #[test]
    fn declare_variable_serializes_all_declarators() {
        let buf = parse_and_serialize("declare const A: number, B: string = \"x\";");
        let declare_variable_idx = buf
            .iter()
            .position(|v| *v == encoded_kind(NodeKind::DeclareVariable))
            .expect("expected DeclareVariable node header in buffer");

        // DeclareVariable serializes as:
        // kind, loc, declarations_count, ...declarations, kind_string.
        assert_eq!(
            buf[declare_variable_idx + 2],
            2,
            "DeclareVariable.declarations should include every declarator"
        );
        assert_eq!(
            buf[declare_variable_idx + 3],
            encoded_kind(NodeKind::VariableDeclarator),
            "DeclareVariable.declarations should contain VariableDeclarator nodes"
        );
    }

    /// The buffer walkers above cannot step over a `Literal`, whose payload is
    /// `custom` in the schema, so a node containing one — every
    /// ImportDeclaration — is not addressable by property name. Search the
    /// string buffer instead: the phase is the only place these fixtures can
    /// put the word "defer" on the wire.
    fn string_buffer_contains(
        buffers: &crate::serializer::SerializerBuffers,
        needle: &str,
    ) -> bool {
        buffers
            .string_buffer
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
    }

    #[test]
    fn import_declaration_serializes_defer_phase() {
        let phased =
            parse_and_serialize_with_filename("import defer * as ns from \"m\";", "test.js");
        assert!(
            string_buffer_contains(&phased, "defer"),
            "`import defer * as ns from \"m\"` should serialize phase=\"defer\""
        );

        let unphased = parse_and_serialize_with_filename("import * as ns from \"m\";", "test.js");
        assert!(
            !string_buffer_contains(&unphased, "defer"),
            "an unmodified import should serialize a null phase"
        );
        assert_eq!(
            phased.program_buffer.len(),
            unphased.program_buffer.len() + 1,
            "the phase should cost one extra word over the null sentinel"
        );
    }

    #[test]
    fn import_expression_serializes_defer_phase() {
        let phased = parse_and_serialize_with_filename("import.defer(\"m\");", "test.js");
        assert!(
            string_buffer_contains(&phased, "defer"),
            "`import.defer(\"m\")` should serialize phase=\"defer\""
        );

        let unphased = parse_and_serialize_with_filename("import(\"m\");", "test.js");
        assert!(
            !string_buffer_contains(&unphased, "defer"),
            "a plain `import()` should serialize a null phase"
        );
    }

    #[test]
    fn declare_function_serializes_explicit_declare_marker() {
        let buf = parse_and_serialize("declare function foo(): void;");
        assert!(
            !bool_prop(&buf, NodeKind::DeclareFunction, "implicitDeclare"),
            "explicit `declare function` should serialize implicitDeclare=false"
        );
    }

    #[test]
    fn declare_function_serializes_ambient_declare_marker() {
        let buf = parse_and_serialize_with_filename("function foo(): void;", "test.js.flow")
            .program_buffer;
        assert!(
            bool_prop(&buf, NodeKind::DeclareFunction, "implicitDeclare"),
            "ambient `function` should serialize implicitDeclare=true"
        );
    }

    #[test]
    fn declare_hook_serializes_explicit_declare_marker() {
        let buf = parse_and_serialize("declare hook useFoo(): void;");
        assert!(
            !bool_prop(&buf, NodeKind::DeclareHook, "implicitDeclare"),
            "explicit `declare hook` should serialize implicitDeclare=false"
        );
    }

    #[test]
    fn declare_namespace_serializes_explicit_namespace_markers() {
        let buffers = parse_and_serialize_with_filename("declare namespace Foo {}", "test.js");
        let buf = &buffers.program_buffer;
        assert!(
            !bool_prop(buf, NodeKind::DeclareNamespace, "global"),
            "local `declare namespace` should serialize global=false"
        );
        assert!(
            !bool_prop(buf, NodeKind::DeclareNamespace, "implicitDeclare"),
            "explicit `declare namespace` should serialize implicitDeclare=false"
        );
        assert_eq!(
            string_prop(&buffers, NodeKind::DeclareNamespace, "keyword"),
            "namespace",
            "`declare namespace` should serialize keyword=\"namespace\""
        );
    }

    #[test]
    fn declare_namespace_serializes_ambient_module_markers() {
        let buffers = parse_and_serialize_with_filename("module Foo {}", "test.d.ts");
        let buf = &buffers.program_buffer;
        assert!(
            !bool_prop(buf, NodeKind::DeclareNamespace, "global"),
            "local ambient module namespace should serialize global=false"
        );
        assert!(
            bool_prop(buf, NodeKind::DeclareNamespace, "implicitDeclare"),
            "ambient `module` namespace should serialize implicitDeclare=true"
        );
        assert_eq!(
            string_prop(&buffers, NodeKind::DeclareNamespace, "keyword"),
            "module",
            "`module` namespace should serialize keyword=\"module\""
        );
    }

    #[test]
    fn declare_namespace_serializes_global_marker() {
        let buf = parse_and_serialize("declare global {}");
        assert!(
            bool_prop(&buf, NodeKind::DeclareNamespace, "global"),
            "`declare global` should serialize global=true"
        );
    }

    #[test]
    fn quoted_declare_nodes_serialize_explicit_declare_marker() {
        let cases = [
            ("declare class Foo {}", NodeKind::DeclareClass),
            ("declare component Foo();", NodeKind::DeclareComponent),
            ("declare const foo: number;", NodeKind::DeclareVariable),
            ("declare function foo(): void;", NodeKind::DeclareFunction),
            ("declare hook useFoo(): void;", NodeKind::DeclareHook),
            (
                "declare export const foo: number;",
                NodeKind::DeclareExportDeclaration,
            ),
            (
                "declare export * from \"foo\";",
                NodeKind::DeclareExportAllDeclaration,
            ),
            ("declare opaque type Foo;", NodeKind::DeclareOpaqueType),
            ("declare type Foo = string;", NodeKind::DeclareTypeAlias),
            ("declare enum Foo { A }", NodeKind::DeclareEnum),
            ("declare interface Foo {}", NodeKind::DeclareInterface),
        ];

        for (source, kind) in cases {
            let buf = parse_and_serialize(source);
            assert!(
                !bool_prop(&buf, kind, "implicitDeclare"),
                "{kind:?} from {source:?} should serialize implicitDeclare=false"
            );
        }
    }

    #[test]
    fn declare_export_class_serializes_declare_class_fields_in_schema_order() {
        let buf = parse_and_serialize("declare export class Foo {}");
        let declare_export_idx = buf
            .iter()
            .position(|v| *v == encoded_kind(NodeKind::DeclareExportDeclaration))
            .expect("expected DeclareExportDeclaration node header in buffer");
        let declare_class_idx = declare_export_idx + 3;
        assert_eq!(
            buf[declare_class_idx],
            encoded_kind(NodeKind::DeclareClass),
            "declare export class should serialize a nested DeclareClass declaration"
        );

        // DeclareClass serializes as:
        // kind, loc, id, typeParameters, extends, implements, mixins, body,
        // implicitDeclare.
        // The `Foo` Identifier payload is six u32 values:
        // kind, loc, string offset, string length, null typeAnnotation, optional=false.
        let extends_count_idx = declare_class_idx + 2 + 6 + 1;
        assert_eq!(
            buf[extends_count_idx], 0,
            "DeclareClass.extends should be the first field after id and typeParameters"
        );
        assert_eq!(
            buf[extends_count_idx + 1],
            0,
            "DeclareClass.implements should follow extends"
        );
        assert_eq!(
            buf[extends_count_idx + 2],
            0,
            "DeclareClass.mixins should follow implements"
        );
        assert_eq!(
            buf[extends_count_idx + 3],
            encoded_kind(NodeKind::ObjectTypeAnnotation),
            "DeclareClass.body should follow extends, implements, and mixins"
        );
    }

    #[test]
    fn hook_type_annotation_emitted_for_hook_function_type() {
        let buf = parse_and_serialize("type T = hook () => void;");
        assert!(
            buffer_contains_kind(&buf, NodeKind::HookTypeAnnotation),
            "expected HookTypeAnnotation node header in buffer"
        );
        assert!(
            !buffer_contains_kind(&buf, NodeKind::FunctionTypeAnnotation),
            "should not emit FunctionTypeAnnotation for `hook (...) => void`"
        );
    }

    #[test]
    fn object_type_private_field_emitted_for_private_field_in_declared_class() {
        // `ObjectTypePrivateField` is only legal inside a class-shaped object
        // type (parser threads `is_class: true`), e.g. inside `declare class`.
        // Source matches fixture `flow/ts_syntax/private_class_field.js`.
        let buf = parse_and_serialize("declare class Bar {\n  #private;\n}");
        assert!(
            buffer_contains_kind(&buf, NodeKind::ObjectTypePrivateField),
            "expected ObjectTypePrivateField node header in buffer"
        );
    }

    #[test]
    fn tuple_type_element_emitted_for_optional_unlabeled_tuple_element() {
        let buf = parse_and_serialize("type T = [number?];");
        assert!(
            buffer_contains_kind(&buf, NodeKind::TupleTypeElement),
            "expected TupleTypeElement node header in buffer for optional element"
        );
    }

    #[test]
    fn tuple_type_element_not_emitted_for_required_unlabeled_tuple_element() {
        let buf = parse_and_serialize("type T = [number];");
        assert!(
            !buffer_contains_kind(&buf, NodeKind::TupleTypeElement),
            "should not wrap a non-optional unlabeled element in TupleTypeElement"
        );
    }

    /// Exercise the FFI `hermesParse` entry point with the supplied
    /// `source_type` and return the resulting `ParseResult`. Test callers
    /// own the returned pointer and must call `hermesParseResult_free`.
    fn ffi_parse_with_source_type(
        source: &str,
        source_type: super::SourceType,
    ) -> *mut super::ParseResult {
        // The C ABI expects a null-terminated UTF-8 buffer; mirror the JS
        // bridge which appends `\0` and passes `Buffer.length + 1`.
        let mut bytes = source.as_bytes().to_vec();
        bytes.push(0);
        let size = bytes.len();
        let ptr = bytes.as_ptr();
        let result = super::hermesParse(
            ptr,
            size,
            std::ptr::null(),
            0,
            1, // enable_components
            1, // enable_match
            1, // enable_decorators
            0, // tokens
            0, // allow_return_outside
            1, // enable_enums
            1, // enable_records
            1, // enable_types
            source_type as i32,
            0,  // enable_types_pragma_detection
            0,  // enable_types_in_comments
            0,  // babel
            0,  // lower_enums
            0,  // custom_enum_runtime
            18, // react_runtime_target
            1,  // throw_on_parse_errors
        );
        // `bytes` must outlive the call — ensure rustc cannot drop it
        // before `hermesParse` returns by reading it after.
        drop(bytes);
        result
    }

    #[test]
    fn module_source_type_allows_top_level_await() {
        // Module source type enables top-level await through the integer ABI.
        let result =
            ffi_parse_with_source_type("export const x = await live();", super::SourceType::Module);
        assert!(!result.is_null(), "ParseResult pointer should be non-null"); // OK
        let buf_ptr = super::hermesParseResult_getProgramBuffer(result);
        assert!(
            !buf_ptr.is_null(),
            "program buffer should be non-null for valid module input"
        ); // OK
        let err_ptr = super::hermesParseResult_getError(result);
        assert!(
            err_ptr.is_null(),
            "valid module input should not produce a parse error"
        ); // OK
        super::hermesParseResult_free(result);
    }
}
