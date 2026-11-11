/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::HashSet;
use std::collections::VecDeque;
use std::sync::Arc;

use dupe::Dupe;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;

use crate::ast::*;
use crate::file_key::FileKey;
use crate::file_key::has_dts_ext;
use crate::file_key::has_ts_ext;
use crate::flow_lexer::LexResult;
use crate::loc::LOC_NONE;
use crate::loc::Loc;
use crate::loc::Position;
use crate::parse_error::ParseError;
use crate::parser_env::try_parse::Rollback;
use crate::token::TokenKind;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LexMode {
    Normal,
    Type,
    JsxTag,
    JsxChild,
    Regexp,
}

pub(crate) mod wrapped_lex_env {
    use dupe::Dupe;

    use crate::file_key::FileKey;
    use crate::lex_env;
    use crate::lex_env::LexEnvState;
    use crate::logos_tokens::MainToken;

    #[derive(Clone, Copy)]
    pub(crate) struct WrappedLexCursor {
        span_start: usize,
        span_end: usize,
        lex_env_state: LexEnvState,
    }

    impl WrappedLexCursor {
        pub(super) fn span_end(&self) -> usize {
            self.span_end
        }
    }

    pub(crate) struct WrappedLexEnv<'a> {
        source: &'a str,
        lex_env: lex_env::LexEnv,
        lexer: logos::Lexer<'a, MainToken>,
    }

    impl<'a> WrappedLexEnv<'a> {
        pub(crate) fn new(
            source: Option<FileKey>,
            content: &'a str,
            enable_types_in_comments: bool,
        ) -> Self {
            Self {
                source: content,
                lex_env: lex_env::LexEnv::new(source, enable_types_in_comments),
                lexer: logos::Lexer::new(content),
            }
        }

        pub(crate) fn source(&self) -> Option<FileKey> {
            self.lex_env.source()
        }

        pub(crate) fn source_text(&self) -> &'a str {
            self.source
        }

        pub(crate) fn cursor(&self) -> WrappedLexCursor {
            let span = self.lexer.span();
            WrappedLexCursor {
                span_start: span.start,
                span_end: span.end,
                lex_env_state: self.lex_env.state(),
            }
        }

        pub(crate) fn seek(&mut self, cursor: WrappedLexCursor) {
            let mut lexer: logos::Lexer<'a, MainToken> = logos::Lexer::new(self.source);
            lexer.bump(cursor.span_end);
            self.lexer = lexer;
            self.lex_env.restore_state(cursor.lex_env_state);
        }

        pub(crate) fn lex(&mut self, lex_mode: super::LexMode) -> crate::flow_lexer::LexResult {
            match lex_mode {
                super::LexMode::Normal => {
                    crate::flow_lexer::token(&mut self.lexer, &mut self.lex_env)
                }
                super::LexMode::Type => {
                    crate::flow_lexer::type_token(&mut self.lexer, &mut self.lex_env)
                }
                super::LexMode::JsxTag => {
                    crate::flow_lexer::jsx_tag(&mut self.lexer, &mut self.lex_env)
                }
                super::LexMode::JsxChild => {
                    crate::flow_lexer::jsx_child(&mut self.lexer, &mut self.lex_env)
                }
                super::LexMode::Regexp => {
                    crate::flow_lexer::regexp(&mut self.lexer, &mut self.lex_env)
                }
            }
        }

        pub(crate) fn lex_template_tail_start(
            &mut self,
            cursor: WrappedLexCursor,
        ) -> crate::flow_lexer::LexResult {
            let mut lexer: logos::Lexer<'a, MainToken> = logos::Lexer::new(self.source);
            lexer.bump(cursor.span_start);
            self.lex_env.restore_state(cursor.lex_env_state);
            let prev_last_loc = self.lex_env.last_loc.dupe();
            let lex_result = crate::flow_lexer::template_tail_start(&mut lexer, &mut self.lex_env);
            self.lexer = lexer;
            self.lex_env.last_loc = prev_last_loc;
            lex_result
        }
    }
}

/// READ THIS BEFORE YOU MODIFY:
///
/// This cache intentionally stores only the current parser token. Code that
/// needs to inspect the following token must do an uncached lexer scan from the
/// current-token cursor and restore immediately.
pub(crate) mod lookahead {
    use super::*;

    struct CachedLexResult {
        cursor: wrapped_lex_env::WrappedLexCursor,
        result: LexResult,
    }

    pub(crate) struct Lookahead<'a> {
        results_0: Option<CachedLexResult>,
        lex_mode: LexMode,
        lex_env: wrapped_lex_env::WrappedLexEnv<'a>,
        committed_cursor: wrapped_lex_env::WrappedLexCursor,
    }

    impl<'a> Lookahead<'a> {
        pub(super) fn new(lex_mode: LexMode, lex_env: wrapped_lex_env::WrappedLexEnv<'a>) -> Self {
            let committed_cursor = lex_env.cursor();
            Self {
                results_0: None,
                lex_mode,
                lex_env,
                committed_cursor,
            }
        }

        fn lex(&mut self) {
            let lex_result = self.lex_env.lex(self.lex_mode);
            let cursor = self.lex_env.cursor();
            self.results_0 = Some(CachedLexResult {
                cursor,
                result: lex_result,
            });
        }

        pub(crate) fn peek_0(&mut self) -> &LexResult {
            if self.results_0.is_none() {
                self.lex();
            }
            &self.results_0.as_ref().unwrap().result
        }

        pub(super) fn cursor_0(&mut self) -> wrapped_lex_env::WrappedLexCursor {
            if self.results_0.is_none() {
                self.lex();
            }
            self.results_0.as_ref().unwrap().cursor
        }

        pub(super) fn committed_cursor(&self) -> wrapped_lex_env::WrappedLexCursor {
            self.committed_cursor
        }

        pub(super) fn source(&self) -> Option<FileKey> {
            self.lex_env.source()
        }

        pub(super) fn source_text(&self) -> &str {
            self.lex_env.source_text()
        }

        pub(super) fn current_end_offset(&mut self) -> usize {
            self.cursor_0().span_end()
        }

        pub(super) fn with_lookahead_1<T>(&mut self, f: impl FnOnce(&LexResult) -> T) -> T {
            if self.results_0.is_none() {
                self.lex();
            }
            let cursor_0 = self.results_0.as_ref().unwrap().cursor;
            let lex_result = self.lex_env.lex(self.lex_mode);
            let result = f(&lex_result);
            self.lex_env.seek(cursor_0);
            result
        }

        pub(super) fn reset_to(
            &mut self,
            lex_mode: LexMode,
            cursor: wrapped_lex_env::WrappedLexCursor,
        ) {
            self.results_0 = None;
            self.lex_mode = lex_mode;
            self.committed_cursor = cursor;
            self.lex_env.seek(cursor);
        }

        pub(super) fn reset(&mut self, lex_mode: LexMode) {
            self.reset_to(lex_mode, self.committed_cursor);
        }

        pub(crate) fn take_0(&mut self) -> LexResult {
            self.peek_0();
            let result = self.results_0.take().unwrap();
            self.committed_cursor = result.cursor;
            result.result
        }

        pub(super) fn rescan_template_part_from(
            &mut self,
            prev_cursor: wrapped_lex_env::WrappedLexCursor,
        ) {
            let existing_comments = self
                .results_0
                .as_ref()
                .map(|cached| cached.result.comments.clone())
                .unwrap_or_default();
            self.results_0 = None;
            let mut lex_result = self.lex_env.lex_template_tail_start(prev_cursor);
            lex_result.comments = existing_comments;
            let cursor = self.lex_env.cursor();
            self.results_0 = Some(CachedLexResult {
                cursor,
                result: lex_result,
            });
        }
    }
}

#[derive(Debug, Clone)]
pub struct TokenSinkResult {
    pub token_loc: Loc,
    pub token_kind: TokenKind,
    pub token_context: LexMode,
}

#[derive(Debug)]
pub struct ParenthesizedExpressionLocation {
    pub loc: Loc,
    pub expression_loc: Loc,
}

#[derive(Debug, Clone)]
pub struct ParseOptions {
    /// enable parsing of Flow component syntax
    pub components: bool,
    /// enable parsing of Flow enums
    pub enums: bool,
    pub pattern_matching: bool,
    pub records: bool,
    /// enable parsing of decorators
    pub esproposal_decorators: bool, /* enable parsing of decorators */
    /// enable parsing of Flow types
    pub types: bool,
    /// enable parsing of Flow syntax that is ambiguous with valid JavaScript
    pub ambiguous_types: bool,
    /// enable parsing Flow comment syntax like `/*: T */` and `/*:: T */`
    pub enable_types_in_comments: bool,
    /// treat the file as strict, without needing a "use strict" directive
    pub use_strict: bool,
    pub module_ref_prefix: Option<FlowSmolStr>,
    /// treat the file as ambient context (e.g. .js.flow or .d.ts)
    pub ambient: bool,
    /// suppress the [IllegalReturn] diagnostic for top-level [return]
    /// statements; matches hermes-parser [allowReturnOutsideFunction]
    pub allow_return_outside_function: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        ParseOptions {
            components: false,
            enums: false,
            pattern_matching: false,
            records: false,
            esproposal_decorators: false,
            types: true,
            ambiguous_types: true,
            enable_types_in_comments: true,
            use_strict: false,
            module_ref_prefix: None,
            ambient: false,
            allow_return_outside_function: false,
        }
    }
}

pub const PERMISSIVE_PARSE_OPTIONS: ParseOptions = ParseOptions {
    components: true,
    enums: true,
    pattern_matching: true,
    records: true,
    esproposal_decorators: true,
    types: true,
    ambiguous_types: true,
    enable_types_in_comments: true,
    use_strict: false,
    module_ref_prefix: None,
    ambient: false,
    allow_return_outside_function: false,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AllowedSuper {
    NoSuper,
    SuperProp,
    SuperPropOrCall,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ParserEnvFlags {
    in_strict_mode: bool,
    in_export: bool,
    in_export_default: bool,
    in_loop: bool,
    in_switch: bool,
    in_formal_parameters: bool,
    in_function: bool,
    in_match_expression: bool,
    in_match_statement: bool,
    no_in: bool,
    no_call: bool,
    no_let: bool,
    no_anon_function_type: bool,
    no_conditional_type: bool,
    no_new: bool,
    no_record: bool,
    allow_yield: bool,
    allow_await: bool,
    allow_directive: bool,
    has_simple_parameters: bool,
    allow_super: AllowedSuper,
    in_ambient_context: bool,
}

pub struct ParserEnv<'a> {
    errors: Vec<(Loc, ParseError)>,
    comments: Vec<Comment<Loc>>,
    parenthesized_expression_locations: Option<Vec<ParenthesizedExpressionLocation>>,
    labels: HashSet<FlowSmolStr>,
    last_lex_result: Option<(Loc, TokenKind)>,
    flags: ParserEnvFlags,
    error_callback: Option<fn(ParseError) -> Result<(), Rollback>>,
    lex_mode_stack: Vec<LexMode>,
    /// This needs to be reset whenever we change lexing context.
    lookahead: lookahead::Lookahead<'a>,
    token_sink: Option<TokenSink<'a>>,
    parse_options: ParseOptions,
    /* It is a syntax error to reference private fields not in scope. In order to enforce this,
     * we keep track of the privates we've seen declared and used. */
    privates: Vec<(HashSet<String>, Vec<(String, Loc)>)>,
    /* The position up to which comments have been consumed, exclusive. */
    consumed_comments_pos: Position,
}

enum TokenSink<'a> {
    Direct(&'a mut dyn FnMut(TokenSinkResult)),
    Buffered {
        next: Box<TokenSink<'a>>,
        buffer: VecDeque<TokenSinkResult>,
    },
}

fn send_token_to_sink<'a>(token_sink: TokenSink<'a>, token_data: TokenSinkResult) -> TokenSink<'a> {
    match token_sink {
        TokenSink::Direct(token_sink) => {
            token_sink(token_data);
            TokenSink::Direct(token_sink)
        }
        TokenSink::Buffered { next, mut buffer } => {
            buffer.push_back(token_data);
            TokenSink::Buffered { next, buffer }
        }
    }
}

/* constructor */
pub fn init_env<'a, E>(
    token_sink: Option<&'a mut dyn FnMut(TokenSinkResult)>,
    parse_options: Option<ParseOptions>,
    source: Option<FileKey>,
    content: Result<&'a str, E>,
) -> ParserEnv<'a> {
    let mut errors = Vec::new();
    let content = match content {
        Ok(s) => s,
        Err(_) => {
            errors.push((
                Loc {
                    source: source.dupe(),
                    ..LOC_NONE
                },
                ParseError::MalformedUnicode,
            ));
            ""
        }
    };
    let mut parse_options = parse_options.unwrap_or(PERMISSIVE_PARSE_OPTIONS);
    if source
        .as_ref()
        .is_some_and(|file| has_ts_ext(file.as_str()))
    {
        parse_options.types = true;
        parse_options.ambiguous_types = true;
    }
    let enable_types_in_comments = parse_options.types && parse_options.enable_types_in_comments;
    let lex_env = wrapped_lex_env::WrappedLexEnv::new(source, content, enable_types_in_comments);
    let lookahead = lookahead::Lookahead::new(LexMode::Normal, lex_env);

    ParserEnv {
        errors,
        comments: Vec::new(),
        parenthesized_expression_locations: None,
        labels: HashSet::new(),
        last_lex_result: None,
        flags: ParserEnvFlags {
            has_simple_parameters: true,
            in_strict_mode: parse_options.use_strict,
            in_export: false,
            in_export_default: false,
            in_loop: false,
            in_switch: false,
            in_formal_parameters: false,
            in_function: false,
            in_match_expression: false,
            in_match_statement: false,
            no_in: false,
            no_call: false,
            no_let: false,
            no_anon_function_type: false,
            no_conditional_type: false,
            no_new: false,
            no_record: false,
            allow_yield: false,
            allow_await: false,
            allow_directive: false,
            allow_super: AllowedSuper::NoSuper,
            in_ambient_context: false,
        },
        error_callback: None,
        lex_mode_stack: vec![LexMode::Normal],
        lookahead,
        token_sink: token_sink.map(TokenSink::Direct),
        parse_options,
        privates: Vec::new(),
        consumed_comments_pos: Position { line: 0, column: 0 },
    }
}

impl<'a> ParserEnv<'a> {
    pub fn enable_parenthesized_expression_locations(&mut self) {
        self.parenthesized_expression_locations = Some(Vec::new());
    }

    pub fn take_parenthesized_expression_locations(
        &mut self,
    ) -> Vec<ParenthesizedExpressionLocation> {
        self.parenthesized_expression_locations
            .take()
            .unwrap_or_default()
    }

    pub(crate) fn record_parenthesized_expression(&mut self, loc: Loc, expression_loc: Loc) {
        if let Some(locations) = &mut self.parenthesized_expression_locations {
            locations.push(ParenthesizedExpressionLocation {
                loc,
                expression_loc,
            });
        }
    }

    // getters

    pub(crate) fn in_strict_mode(&self) -> bool {
        self.flags.in_strict_mode
    }

    pub(crate) fn lex_mode(&self) -> LexMode {
        self.lex_mode_stack.last().copied().unwrap()
    }

    pub(crate) fn in_export(&self) -> bool {
        self.flags.in_export
    }

    pub(crate) fn in_export_default(&self) -> bool {
        self.flags.in_export_default
    }

    pub(crate) fn take_comments(&mut self) -> Vec<Comment<Loc>> {
        std::mem::take(&mut self.comments)
    }

    pub(crate) fn in_labels(&self, label: &str) -> bool {
        self.labels.contains(label)
    }

    pub(crate) fn in_loop(&self) -> bool {
        self.flags.in_loop
    }

    pub(crate) fn in_switch(&self) -> bool {
        self.flags.in_switch
    }

    pub(crate) fn in_formal_parameters(&self) -> bool {
        self.flags.in_formal_parameters
    }

    pub(crate) fn in_function(&self) -> bool {
        self.flags.in_function
    }

    pub(crate) fn in_match_expression(&self) -> bool {
        self.flags.in_match_expression
    }

    pub(crate) fn in_match_statement(&self) -> bool {
        self.flags.in_match_statement
    }

    pub(crate) fn allow_yield(&self) -> bool {
        self.flags.allow_yield
    }

    pub(crate) fn allow_await(&self) -> bool {
        self.flags.allow_await
    }

    pub(crate) fn allow_directive(&self) -> bool {
        self.flags.allow_directive
    }

    pub(crate) fn allow_super(&self) -> AllowedSuper {
        self.flags.allow_super
    }

    pub(crate) fn in_ambient_context(&self) -> bool {
        self.flags.in_ambient_context
    }

    pub(crate) fn has_simple_parameters(&self) -> bool {
        self.flags.has_simple_parameters
    }

    pub(crate) fn no_in(&self) -> bool {
        self.flags.no_in
    }

    pub(crate) fn no_call(&self) -> bool {
        self.flags.no_call
    }

    pub(crate) fn no_let(&self) -> bool {
        self.flags.no_let
    }

    pub(crate) fn no_anon_function_type(&self) -> bool {
        self.flags.no_anon_function_type
    }

    pub(crate) fn no_conditional_type(&self) -> bool {
        self.flags.no_conditional_type
    }

    pub(crate) fn no_new(&self) -> bool {
        self.flags.no_new
    }

    pub(crate) fn no_record(&self) -> bool {
        self.flags.no_record
    }

    pub(crate) fn errors(self) -> Vec<(Loc, ParseError)> {
        self.errors
    }

    pub(crate) fn parse_options(&self) -> &ParseOptions {
        &self.parse_options
    }

    pub(crate) fn should_parse_types(&self) -> bool {
        self.parse_options.types
    }

    pub(crate) fn should_parse_ambiguous_types(&self) -> bool {
        self.parse_options.types && self.parse_options.ambiguous_types
    }

    pub fn is_d_ts(&self) -> bool {
        self.source()
            .is_some_and(|file_key| has_dts_ext(file_key.as_str()))
    }

    pub(crate) fn is_ts(&self) -> bool {
        self.source()
            .is_some_and(|file_key| has_ts_ext(file_key.as_str()))
    }

    pub(crate) fn source(&self) -> Option<FileKey> {
        self.lookahead.source()
    }

    // mutators

    pub(crate) fn error_at(&mut self, loc: Loc, error: ParseError) -> Result<(), Rollback> {
        if let Some(callback) = &self.error_callback {
            callback(error.clone())?;
        }
        self.errors.push((loc, error));
        Ok(())
    }

    /* Since private fields out of scope are a parse error, we keep track of the declared and used
     * private fields.
     *
     * Whenever we enter a class, we push new empty lists of declared and used privates.
     * When we encounter a new declared private, we add it to the top of the declared_privates list
     * via add_declared_private. We do the same with used_privates via add_used_private.
     *
     * When we exit a class, we look for all the unbound private variables. Since class fields
     * are hoisted to the scope of the class, we may need to look further before we conclude that
     * a field is out of scope. To do that, we add all of the unbound private fields to the
     * next used_private list. Once we run out of declared private lists, any leftover used_privates
     * are unbound private variables. */
    pub(crate) fn enter_class(&mut self) {
        self.privates.push((HashSet::new(), Vec::new()));
    }

    pub(crate) fn exit_class(&mut self) -> Result<(), Rollback> {
        fn get_unbound_privates(
            declared_privates: HashSet<String>,
            used_privates: Vec<(String, Loc)>,
        ) -> Vec<(String, Loc)> {
            used_privates
                .into_iter()
                .filter(|(name, _)| !declared_privates.contains(name))
                .collect()
        }

        match self.privates.len() {
            1 => {
                let (declared_privates, used_privates) = self.privates.pop().unwrap();
                let unbound_privates = get_unbound_privates(declared_privates, used_privates);
                for (name, loc) in unbound_privates {
                    self.error_at(loc, ParseError::UnboundPrivate(name))?;
                }
            }
            n if n >= 2 => {
                let (loc_declared_privates, loc_used_privates) = self.privates.pop().unwrap();
                let unbound_privates =
                    get_unbound_privates(loc_declared_privates, loc_used_privates);
                let (decl_head, mut used_head) = self.privates.pop().unwrap();
                used_head.extend(unbound_privates);
                self.privates.push((decl_head, used_head));
            }
            _ => panic!("Internal Error: `exit_class` called before a matching `enter_class`"),
        }
        Ok(())
    }

    pub fn add_declared_private(&mut self, name: String) {
        let (declared, _) = self
            .privates
            .last_mut()
            .expect("Internal Error: Tried to add_declared_private with outside of class scope.");
        declared.insert(name);
    }

    pub(crate) fn add_used_private(&mut self, name: String, loc: Loc) -> Result<(), Rollback> {
        match self.privates.last_mut() {
            Some((_, used)) => used.push((name, loc)),
            None => self.error_at(loc, ParseError::PrivateNotInClass)?,
        }
        Ok(())
    }

    pub(crate) fn consume_comments_until(&mut self, pos: Position) {
        self.consumed_comments_pos = pos;
    }

    // lookaheads

    pub(crate) fn lookahead_0(&mut self) -> &LexResult {
        self.lookahead.peek_0()
    }

    // functional operations
    pub(crate) fn with_strict<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        in_strict_mode: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.in_strict_mode = in_strict_mode;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_in_formal_parameters<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        in_formal_parameters: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.in_formal_parameters = in_formal_parameters;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_in_match_expression<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        in_match_expression: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.in_match_expression = in_match_expression;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_in_match_statement<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        in_match_statement: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.in_match_statement = in_match_statement;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_allow_yield<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        allow_yield: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.allow_yield = allow_yield;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_allow_await<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        allow_await: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.allow_await = allow_await;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_allow_directive<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        allow_directive: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.allow_directive = allow_directive;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_allow_super<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        allow_super: AllowedSuper,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.allow_super = allow_super;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub fn with_ambient_context<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        in_ambient_context: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.in_ambient_context = in_ambient_context;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_no_let<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        no_let: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.no_let = no_let;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_in_loop<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        in_loop: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.in_loop = in_loop;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_no_in<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        no_in: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.no_in = no_in;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_no_anon_function_type<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        no_anon_function_type: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.no_anon_function_type = no_anon_function_type;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_no_conditional_type<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        no_conditional_type: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.no_conditional_type = no_conditional_type;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_no_new<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        no_new: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.no_new = no_new;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_no_record<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        no_record: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.no_record = no_record;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_in_switch<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        in_switch: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.in_switch = in_switch;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_in_export<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        in_export: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.in_export = in_export;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_in_export_default<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        in_export_default: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.in_export_default = in_export_default;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_no_call<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        no_call: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.no_call = no_call;
        let result = f(self);
        self.flags = saved_flags;
        result
    }

    pub(crate) fn with_error_callback<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        error_callback: fn(ParseError) -> Result<(), Rollback>,
        f: F,
    ) -> T {
        let saved_error_callback = self.error_callback;
        self.error_callback = Some(error_callback);
        let result = f(self);
        self.error_callback = saved_error_callback;
        result
    }

    pub(crate) fn without_error_callback(&mut self) {
        self.error_callback = None
    }

    /// other helpful functions
    pub(crate) fn add_errors(&mut self, errors: Vec<(Loc, ParseError)>) -> Result<(), Rollback> {
        for (loc, error) in errors {
            self.error_at(loc, error)?;
        }
        Ok(())
    }

    pub(crate) fn last_loc(&self) -> Option<&Loc> {
        self.last_lex_result.as_ref().map(|(loc, _)| loc)
    }

    pub(crate) fn last_token(&self) -> Option<&TokenKind> {
        self.last_lex_result
            .as_ref()
            .map(|(_, token_kind)| token_kind)
    }

    pub(crate) fn with_added_label<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        label: FlowSmolStr,
        f: F,
    ) -> T {
        self.labels.insert(label.clone());
        let result = f(self);
        self.labels.remove(&label);
        result
    }

    pub(crate) fn enter_function<T, F: FnOnce(&mut ParserEnv<'a>) -> T>(
        &mut self,
        async_fn: bool,
        generator: bool,
        simple_params: bool,
        f: F,
    ) -> T {
        let saved_flags = self.flags;
        self.flags.in_formal_parameters = false;
        self.flags.has_simple_parameters = simple_params;
        self.flags.in_function = true;
        self.flags.in_loop = false;
        self.flags.in_switch = false;
        self.flags.in_match_expression = false;
        self.flags.in_match_statement = false;
        self.flags.in_export = false;
        self.flags.in_export_default = false;
        let saved_labels = std::mem::take(&mut self.labels);
        self.flags.allow_await = async_fn;
        self.flags.allow_yield = generator;
        let result = f(self);
        self.flags = saved_flags;
        self.labels = saved_labels;
        result
    }
}

/// IdentifierNames that can't be used as Identifiers in strict mode.
///
/// https://tc39.es/ecma262/#sec-strict-mode-of-ecmascript
pub(crate) fn is_strict_reserved(s: &str) -> bool {
    match s {
        "implements" | "interface" | "let" | "package" | "private" | "protected" | "public"
        | "static" | "yield" => true,
        _ => false,
    }
}

/// Tokens which, if parsed as an identifier, are reserved words in strict mode.
pub(crate) fn token_is_strict_reserved(token: &TokenKind) -> bool {
    match token {
        TokenKind::TIdentifier { value, .. } => is_strict_reserved(value),
        TokenKind::TInterface
        | TokenKind::TImplements
        | TokenKind::TLet
        | TokenKind::TPackage
        | TokenKind::TPrivate
        | TokenKind::TProtected
        | TokenKind::TPublic
        | TokenKind::TStatic
        | TokenKind::TYield => true,
        _ => false,
    }
}

// #sec-strict-mode-of-ecmascript
pub(crate) fn is_restricted(s: &str) -> bool {
    match s {
        "eval" | "arguments" => true,
        _ => false,
    }
}

/// Words that are sometimes reserved, and sometimes allowed as identifiers
/// (namely "await" and "yield")
///
/// https://tc39.es/ecma262/#sec-keywords-and-reserved-words
pub(crate) fn is_contextually_reserved(str_val: &str) -> bool {
    match str_val {
        "await" | "yield" => true,
        _ => false,
    }
}

/// Words that are always reserved (mostly keywords)
///
/// https://tc39.es/ecma262/#sec-keywords-and-reserved-words
pub(crate) fn is_reserved(str_val: &str) -> bool {
    match str_val {
        "break" | "case" | "catch" | "class" | "const" | "continue" | "debugger" | "default"
        | "delete" | "do" | "else" | "enum" | "export" | "extends" | "false" | "finally"
        | "for" | "function" | "if" | "import" | "in" | "instanceof" | "new" | "null"
        | "return" | "super" | "switch" | "this" | "throw" | "true" | "try" | "typeof" | "var"
        | "void" | "while" | "with" => true,
        _ => false,
    }
}

pub(crate) fn token_is_reserved(token: &TokenKind) -> bool {
    match token {
        TokenKind::TIdentifier { raw, .. } if is_reserved(raw) => true,
        TokenKind::TBreak
        | TokenKind::TCase
        | TokenKind::TCatch
        | TokenKind::TClass
        | TokenKind::TConst
        | TokenKind::TContinue
        | TokenKind::TDebugger
        | TokenKind::TDefault
        | TokenKind::TDelete
        | TokenKind::TDo
        | TokenKind::TElse
        | TokenKind::TEnum
        | TokenKind::TExport
        | TokenKind::TExtends
        | TokenKind::TFalse
        | TokenKind::TFinally
        | TokenKind::TFor
        | TokenKind::TFunction
        | TokenKind::TIf
        | TokenKind::TImport
        | TokenKind::TIn
        | TokenKind::TInstanceof
        | TokenKind::TNew
        | TokenKind::TNull
        | TokenKind::TReturn
        | TokenKind::TSuper
        | TokenKind::TSwitch
        | TokenKind::TThis
        | TokenKind::TThrow
        | TokenKind::TTrue
        | TokenKind::TTry
        | TokenKind::TTypeof
        | TokenKind::TVar
        | TokenKind::TVoid
        | TokenKind::TWhile
        | TokenKind::TWith => true,
        _ => false,
    }
}

pub(crate) fn is_reserved_type(str_val: &str) -> bool {
    match str_val {
        "any" | "bigint" | "bool" | "boolean" | "const" | "empty" | "extends" | "false"
        | "function" | "interface" | "keyof" | "mixed" | "never" | "null" | "number"
        | "readonly" | "static" | "string" | "symbol" | "true" | "typeof" | "undefined"
        | "unknown" | "void" => true,
        _ => false,
    }
}

pub(crate) fn token_is_reserved_type(t: &TokenKind) -> bool {
    match t {
        TokenKind::TIdentifier { raw, .. } if is_reserved_type(raw) => true,
        TokenKind::TAnyType
        | TokenKind::TBigintType
        | TokenKind::TBooleanType(_)
        | TokenKind::TConst
        | TokenKind::TEmptyType
        | TokenKind::TExtends
        | TokenKind::TFalse
        | TokenKind::TFunction
        | TokenKind::TInterface
        | TokenKind::TKeyof
        | TokenKind::TMixedType
        | TokenKind::TNeverType
        | TokenKind::TNull
        | TokenKind::TNumberType
        | TokenKind::TReadonly
        | TokenKind::TWriteonly
        | TokenKind::TStatic
        | TokenKind::TStringType
        | TokenKind::TSymbolType
        | TokenKind::TTrue
        | TokenKind::TTypeof
        | TokenKind::TUndefinedType
        | TokenKind::TUnknownType
        | TokenKind::TVoidType => true,
        _ => false,
    }
}

fn token_is_type_identifier_under_lex_mode(lex_mode: LexMode, t: &TokenKind) -> bool {
    match lex_mode {
        LexMode::Type => match t {
            TokenKind::TIdentifier { .. } => true,
            _ => false,
        },
        LexMode::Normal => {
            /* Sometimes we peek at type identifiers while in normal lex mode. For
            example, when deciding whether a `type` token is an identifier or the
            start of a type declaration, based on whether the following token
            `is_type_identifier`. */
            match t {
                TokenKind::TIdentifier { raw, .. } if is_reserved_type(raw) => false,
                /* reserved type identifiers, but these don't appear in NORMAL mode */
                TokenKind::TAnyType | TokenKind::TMixedType | TokenKind::TEmptyType | TokenKind::TNumberType
                | TokenKind::TBigintType | TokenKind::TStringType | TokenKind::TVoidType | TokenKind::TSymbolType
                | TokenKind::TUnknownType | TokenKind::TNeverType | TokenKind::TUndefinedType
                | TokenKind::TBooleanType(_) | TokenKind::TNumberSingletonType { .. } | TokenKind::TBigintSingletonType { .. }
                /* identifier-ish */
                | TokenKind::TAsync | TokenKind::TAwait | TokenKind::TBreak | TokenKind::TCase | TokenKind::TCatch
                | TokenKind::TClass | TokenKind::TConst | TokenKind::TContinue | TokenKind::TDebugger | TokenKind::TDeclare
                | TokenKind::TDefault | TokenKind::TDelete | TokenKind::TDo | TokenKind::TElse | TokenKind::TEnum
                | TokenKind::TExport | TokenKind::TExtends | TokenKind::TFalse | TokenKind::TFinally | TokenKind::TFor
                | TokenKind::TIdentifier { .. } | TokenKind::TIf | TokenKind::TImplements | TokenKind::TImport | TokenKind::TIn
                | TokenKind::TInstanceof | TokenKind::TInterface | TokenKind::TLet | TokenKind::TMatch | TokenKind::TRecord | TokenKind::TNew
                | TokenKind::TNull | TokenKind::TOf | TokenKind::TOpaque | TokenKind::TPackage | TokenKind::TPrivate
                | TokenKind::TProtected | TokenKind::TPublic | TokenKind::TReturn | TokenKind::TSuper | TokenKind::TSwitch
                | TokenKind::TThis | TokenKind::TThrow | TokenKind::TTrue | TokenKind::TTry | TokenKind::TType | TokenKind::TVar
                | TokenKind::TWhile | TokenKind::TWith | TokenKind::TYield => true,
                /* identifier-ish, but not valid types */
                TokenKind::TStatic | TokenKind::TTypeof | TokenKind::TFunction | TokenKind::TKeyof | TokenKind::TReadonly
                | TokenKind::TWriteonly | TokenKind::TInfer | TokenKind::TIs | TokenKind::TAsserts | TokenKind::TImplies | TokenKind::TVoid
                | TokenKind::TRendersQuestion | TokenKind::TRendersStar => false,
                /* syntax */
                TokenKind::TLcurly | TokenKind::TRcurly | TokenKind::TLcurlybar | TokenKind::TRcurlybar | TokenKind::TLparen
                | TokenKind::TRparen | TokenKind::TLbracket | TokenKind::TRbracket | TokenKind::TSemicolon | TokenKind::TComma
                | TokenKind::TPeriod | TokenKind::TArrow | TokenKind::TEllipsis | TokenKind::TAt | TokenKind::TPound
                | TokenKind::TChecks | TokenKind::TRshift3Assign | TokenKind::TRshiftAssign | TokenKind::TLshiftAssign
                | TokenKind::TBitXorAssign | TokenKind::TBitOrAssign | TokenKind::TBitAndAssign | TokenKind::TModAssign
                | TokenKind::TDivAssign | TokenKind::TMultAssign | TokenKind::TExpAssign | TokenKind::TMinusAssign
                | TokenKind::TPlusAssign | TokenKind::TNullishAssign | TokenKind::TAndAssign | TokenKind::TOrAssign
                | TokenKind::TAssign | TokenKind::TPlingPeriod | TokenKind::TPlingPling | TokenKind::TPling | TokenKind::TColon
                | TokenKind::TOr | TokenKind::TAnd | TokenKind::TBitOr | TokenKind::TBitXor | TokenKind::TBitAnd | TokenKind::TEqual
                | TokenKind::TNotEqual | TokenKind::TStrictEqual | TokenKind::TStrictNotEqual | TokenKind::TLessThanEqual
                | TokenKind::TGreaterThanEqual | TokenKind::TLessThan | TokenKind::TGreaterThan | TokenKind::TLshift
                | TokenKind::TRshift | TokenKind::TRshift3 | TokenKind::TPlus | TokenKind::TMinus | TokenKind::TDiv
                | TokenKind::TMult | TokenKind::TExp | TokenKind::TMod | TokenKind::TNot | TokenKind::TBitNot | TokenKind::TIncr
                | TokenKind::TDecr | TokenKind::TInterpreter(_, _) | TokenKind::TEof => false,
                /* literals */
                TokenKind::TNumber { .. } | TokenKind::TBigint { .. } | TokenKind::TString(_, _, _, _) | TokenKind::TTemplatePart(_)
                | TokenKind::TRegexp(_, _, _) => false,
                /* misc that shouldn't appear in NORMAL mode */
                TokenKind::TJsxIdentifier { .. } | TokenKind::TJsxChildText(_, _, _) | TokenKind::TJsxQuoteText(_, _, _)
                | TokenKind::TError(_) => false,
            }
        }
        LexMode::JsxTag | LexMode::JsxChild | LexMode::Regexp => false,
    }
}

pub(crate) fn token_is_type_identifier(env: &ParserEnv, t: &TokenKind) -> bool {
    token_is_type_identifier_under_lex_mode(env.lex_mode(), t)
}

pub(crate) fn token_is_variance(token: &TokenKind) -> bool {
    match token {
        TokenKind::TPlus | TokenKind::TMinus => true,
        _ => false,
    }
}

/// Answer questions about what comes next
pub(crate) mod peek {
    use super::*;
    use crate::parser_common::identifier_name_for_token;
    use crate::token::TokenKind;

    pub(crate) fn token<'a>(env: &'a mut ParserEnv) -> &'a TokenKind {
        &env.lookahead_0().token_kind
    }

    pub(crate) fn loc<'a>(env: &'a mut ParserEnv) -> &'a Loc {
        &env.lookahead_0().loc
    }

    /// loc_skip_lookahead is used to give a loc hint to optional tokens such as type annotations
    pub(crate) fn loc_skip_lookahead(env: &ParserEnv) -> Loc {
        let loc = env
            .last_loc()
            .expect("Peeking current location when not available");
        Loc {
            start: loc.end,
            ..loc.dupe()
        }
    }

    pub(crate) fn errors(env: &mut ParserEnv) -> Vec<(Loc, ParseError)> {
        let errors = env.lookahead_0().errors.as_errors();
        if errors.is_empty() {
            Vec::new()
        } else {
            errors.to_vec()
        }
    }

    pub(crate) fn comments(env: &mut ParserEnv) -> Vec<Comment<Loc>> {
        let consumed_comments_pos = env.consumed_comments_pos;
        let comments = &env.lookahead_0().comments;
        if comments.is_empty() {
            Vec::new()
        } else if comments[0].loc.start >= consumed_comments_pos {
            comments.to_vec()
        } else {
            match comments
                .iter()
                .position(|comment| comment.loc.start >= consumed_comments_pos)
            {
                Some(index) => comments[index..].to_vec(),
                None => Vec::new(),
            }
        }
    }

    pub(crate) fn has_eaten_comments(env: &mut ParserEnv) -> bool {
        let consumed_comments_pos = env.consumed_comments_pos;
        let comments = &env.lookahead_0().comments;
        comments
            .iter()
            .any(|comment| comment.loc.start < consumed_comments_pos)
    }

    pub(crate) fn lex_cursor(env: &mut ParserEnv) -> wrapped_lex_env::WrappedLexCursor {
        env.lookahead.cursor_0()
    }

    /// True if there is a line terminator before the current token.
    pub(crate) fn is_line_terminator(env: &mut ParserEnv) -> bool {
        match env.last_loc() {
            None => false,
            Some(loc_prev) => {
                // Only newlines between tokens count, not newlines within the previous
                // token (e.g. a multiline template or a continued string). A postfix `!`
                // on that token's ending line must not be separated by ASI.
                let loc_prev_end_line = loc_prev.end.line;
                loc(env).start.line > loc_prev_end_line
            }
        }
    }

    pub(crate) fn is_implicit_semicolon(env: &mut ParserEnv) -> bool {
        match token(env) {
            TokenKind::TEof | TokenKind::TRcurly => true,
            TokenKind::TSemicolon => false,
            _ => is_line_terminator(env),
        }
    }

    fn with_lexer_lookahead1<T>(env: &mut ParserEnv, f: impl FnOnce(&LexResult) -> T) -> T {
        env.lookahead.with_lookahead_1(f)
    }

    fn lexer_lookahead1_token_kind(env: &mut ParserEnv) -> TokenKind {
        with_lexer_lookahead1(env, |next| next.token_kind.clone())
    }

    fn unicode_line_terminator_len(bytes: &[u8], offset: usize) -> Option<usize> {
        if bytes.get(offset) == Some(&0xe2)
            && bytes.get(offset + 1) == Some(&0x80)
            && matches!(bytes.get(offset + 2), Some(0xa8 | 0xa9))
        {
            Some(3)
        } else {
            None
        }
    }

    fn non_line_whitespace_len(bytes: &[u8], offset: usize) -> Option<usize> {
        match bytes.get(offset) {
            Some(0xc2) if bytes.get(offset + 1) == Some(&0xa0) => Some(2),
            Some(0xe1)
                if bytes.get(offset + 1) == Some(&0x9a) && bytes.get(offset + 2) == Some(&0x80) =>
            {
                Some(3)
            }
            Some(0xe2)
                if bytes.get(offset + 1) == Some(&0x80)
                    && matches!(bytes.get(offset + 2), Some(0x80..=0x8a | 0xaf)) =>
            {
                Some(3)
            }
            Some(0xe2)
                if bytes.get(offset + 1) == Some(&0x81) && bytes.get(offset + 2) == Some(&0x9f) =>
            {
                Some(3)
            }
            Some(0xe3)
                if bytes.get(offset + 1) == Some(&0x80) && bytes.get(offset + 2) == Some(&0x80) =>
            {
                Some(3)
            }
            Some(0xef)
                if bytes.get(offset + 1) == Some(&0xbb) && bytes.get(offset + 2) == Some(&0xbf) =>
            {
                Some(3)
            }
            _ => None,
        }
    }

    fn flow_comment_marker_after_whitespace(source: &str, mut offset: usize) -> bool {
        while offset < source.len() {
            let ch = source[offset..].chars().next().unwrap();
            if !ch.is_whitespace() {
                break;
            }
            offset += ch.len_utf8();
        }
        let rest = &source[offset..];
        rest.starts_with("::") || rest.starts_with(':') || rest.starts_with("flow-include")
    }

    fn offset_after_current_trivia(env: &mut ParserEnv) -> Result<(Option<usize>, bool), ()> {
        let mut offset = env.lookahead.current_end_offset();
        let source = env.lookahead.source_text();
        let bytes = source.as_bytes();
        let mut has_line_terminator = false;

        while offset < source.len() {
            match bytes[offset] {
                b' ' | b'\t' | 0x0B | 0x0C => {
                    offset += 1;
                    while offset < source.len()
                        && matches!(bytes[offset], b' ' | b'\t' | 0x0B | 0x0C)
                    {
                        offset += 1;
                    }
                }
                b'\r' => {
                    offset += if bytes.get(offset + 1) == Some(&b'\n') {
                        2
                    } else {
                        1
                    };
                    has_line_terminator = true;
                }
                b'\n' => {
                    offset += 1;
                    has_line_terminator = true;
                }
                b'/' if bytes.get(offset + 1) == Some(&b'/') => {
                    offset += 2;
                    while offset < source.len() {
                        if matches!(bytes[offset], b'\r' | b'\n')
                            || unicode_line_terminator_len(bytes, offset).is_some()
                        {
                            break;
                        }
                        offset += if bytes[offset].is_ascii() {
                            1
                        } else {
                            source[offset..].chars().next().unwrap().len_utf8()
                        };
                    }
                }
                b'/' if bytes.get(offset + 1) == Some(&b'*') => {
                    if flow_comment_marker_after_whitespace(source, offset + 2) {
                        return Err(());
                    }

                    offset += 2;
                    loop {
                        if offset >= source.len() {
                            return Err(());
                        }
                        match bytes[offset] {
                            b'*' if bytes.get(offset + 1) == Some(&b'/') => {
                                offset += 2;
                                break;
                            }
                            b'*' if bytes.get(offset + 1) == Some(&b'-')
                                && bytes.get(offset + 2) == Some(&b'/') =>
                            {
                                return Err(());
                            }
                            b'\r' => {
                                offset += if bytes.get(offset + 1) == Some(&b'\n') {
                                    2
                                } else {
                                    1
                                };
                                has_line_terminator = true;
                            }
                            b'\n' => {
                                offset += 1;
                                has_line_terminator = true;
                            }
                            _ => {
                                if let Some(len) = unicode_line_terminator_len(bytes, offset) {
                                    offset += len;
                                    has_line_terminator = true;
                                } else {
                                    offset += if bytes[offset].is_ascii() {
                                        1
                                    } else {
                                        source[offset..].chars().next().unwrap().len_utf8()
                                    };
                                }
                            }
                        }
                    }
                }
                _ => {
                    if let Some(len) = unicode_line_terminator_len(bytes, offset) {
                        offset += len;
                        has_line_terminator = true;
                    } else if let Some(len) = non_line_whitespace_len(bytes, offset) {
                        offset += len;
                    } else {
                        return Ok((Some(offset), has_line_terminator));
                    }
                }
            }
        }

        Ok((None, has_line_terminator))
    }

    fn ascii_identifier_continue(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || matches!(byte, b'$' | b'_')
    }

    fn ascii_identifier_start(byte: u8) -> bool {
        byte.is_ascii_alphabetic() || matches!(byte, b'$' | b'_')
    }

    fn rest_starts_less_than_token(lex_mode: LexMode, rest: &str) -> bool {
        if !rest.starts_with('<') {
            false
        } else if lex_mode == LexMode::Type {
            true
        } else {
            !rest.starts_with("<=") && !rest.starts_with("<<")
        }
    }

    fn rest_starts_period_token(rest: &str) -> bool {
        rest.starts_with('.') && !rest.starts_with("...")
    }

    fn rest_starts_assign_token(rest: &str) -> bool {
        rest.starts_with('=') && !rest.starts_with("=>") && !rest.starts_with("==")
    }

    fn rest_starts_not_token(rest: &str) -> bool {
        rest.starts_with('!') && !rest.starts_with("!=")
    }

    fn rest_starts_pling_token(rest: &str) -> Result<bool, ()> {
        if !rest.starts_with('?') {
            Ok(false)
        } else if rest.starts_with("?.") {
            Err(())
        } else {
            Ok(true)
        }
    }

    fn rest_starts_in_token(rest: &str) -> Result<bool, ()> {
        if !rest.starts_with('i') {
            return Ok(false);
        }
        if !rest.starts_with("in") {
            return Ok(false);
        }
        match rest.as_bytes().get(2) {
            None => Ok(true),
            Some(b'\\') => Err(()),
            Some(byte) if byte.is_ascii() => Ok(!ascii_identifier_continue(*byte)),
            Some(_) => Err(()),
        }
    }

    fn rest_starts_word_token(source: &str, offset: usize, word: &str) -> Result<bool, ()> {
        let bytes = source.as_bytes();
        match bytes.get(offset) {
            Some(b'\\') => return Err(()),
            Some(byte) if !byte.is_ascii() => return Err(()),
            _ => {}
        }
        if !bytes[offset..].starts_with(word.as_bytes()) {
            return Ok(false);
        }
        match bytes.get(offset + word.len()) {
            None => Ok(true),
            Some(b'\\') => Err(()),
            Some(byte) if byte.is_ascii() => Ok(!ascii_identifier_continue(*byte)),
            Some(_) => Err(()),
        }
    }

    fn current_token_is_followed_by_word(
        env: &mut ParserEnv,
        word: &str,
        f: impl FnOnce(&TokenKind) -> bool,
    ) -> bool {
        match offset_after_current_trivia(env) {
            Ok((Some(offset), _)) => {
                match rest_starts_word_token(env.lookahead.source_text(), offset, word) {
                    Ok(result) => result,
                    Err(()) => lexer_lookahead1_matches(env, f),
                }
            }
            Ok((None, _)) => false,
            Err(()) => lexer_lookahead1_matches(env, f),
        }
    }

    fn current_token_is_followed_by_word_on_current_end_line(
        env: &mut ParserEnv,
        word: &str,
        f: impl FnOnce(&TokenKind) -> bool,
    ) -> bool {
        match offset_after_current_trivia(env) {
            Ok((Some(offset), false)) => {
                match rest_starts_word_token(env.lookahead.source_text(), offset, word) {
                    Ok(result) => result,
                    Err(()) => lexer_lookahead1_matches_on_current_end_line(env, f),
                }
            }
            Ok((None, false)) => lexer_lookahead1_matches_on_current_end_line(env, f),
            Ok(_) => false,
            Err(()) => lexer_lookahead1_matches_on_current_end_line(env, f),
        }
    }

    fn current_token_is_followed_by_char(
        env: &mut ParserEnv,
        ch: char,
        f: impl FnOnce(&TokenKind) -> bool,
    ) -> bool {
        match offset_after_current_trivia(env) {
            Ok((Some(offset), _)) => env.lookahead.source_text()[offset..].starts_with(ch),
            Ok((None, _)) => false,
            Err(()) => lexer_lookahead1_matches(env, f),
        }
    }

    fn current_token_is_followed_by_less_than(env: &mut ParserEnv) -> bool {
        match offset_after_current_trivia(env) {
            Ok((Some(offset), _)) => {
                let rest = &env.lookahead.source_text()[offset..];
                rest_starts_less_than_token(env.lex_mode(), rest)
            }
            Ok((None, _)) => false,
            Err(()) => lexer_lookahead1_matches(env, |token| token == &TokenKind::TLessThan),
        }
    }

    fn current_token_is_followed_by_arrow(env: &mut ParserEnv) -> bool {
        match offset_after_current_trivia(env) {
            Ok((Some(offset), _)) => env.lookahead.source_text()[offset..].starts_with("=>"),
            Ok((None, _)) => false,
            Err(()) => lexer_lookahead1_matches(env, |token| token == &TokenKind::TArrow),
        }
    }

    fn current_token_is_followed_by_char_on_current_end_line(
        env: &mut ParserEnv,
        ch: char,
        f: impl FnOnce(&TokenKind) -> bool,
    ) -> bool {
        match offset_after_current_trivia(env) {
            Ok((Some(offset), false)) => env.lookahead.source_text()[offset..].starts_with(ch),
            Ok(_) => false,
            Err(()) => lexer_lookahead1_matches_on_current_end_line(env, f),
        }
    }

    fn lexer_lookahead1_matches(env: &mut ParserEnv, f: impl FnOnce(&TokenKind) -> bool) -> bool {
        with_lexer_lookahead1(env, |next| f(&next.token_kind))
    }

    fn lexer_lookahead1_matches_on_current_end_line(
        env: &mut ParserEnv,
        f: impl FnOnce(&TokenKind) -> bool,
    ) -> bool {
        let current_end_line = loc(env).end.line;
        with_lexer_lookahead1(env, |next| {
            next.loc.start.line == current_end_line && f(&next.token_kind)
        })
    }

    fn lexer_lookahead1_token_kind_on_current_start_line(env: &mut ParserEnv) -> Option<TokenKind> {
        let current_start_line = loc(env).start.line;
        with_lexer_lookahead1(env, |next| {
            if next.loc.start.line == current_start_line {
                Some(next.token_kind.clone())
            } else {
                None
            }
        })
    }

    fn lexer_lookahead1_token_kind_on_current_end_line(env: &mut ParserEnv) -> Option<TokenKind> {
        let current_end_line = loc(env).end.line;
        with_lexer_lookahead1(env, |next| {
            if next.loc.start.line == current_end_line {
                Some(next.token_kind.clone())
            } else {
                None
            }
        })
    }

    pub(crate) fn loc_after_current_if_lbracket(env: &mut ParserEnv) -> Option<Loc> {
        match offset_after_current_trivia(env) {
            Ok((Some(offset), _)) if !env.lookahead.source_text()[offset..].starts_with('[') => {
                return None;
            }
            Ok((None, _)) => return None,
            _ => {}
        }
        with_lexer_lookahead1(env, |next| {
            if next.token_kind == TokenKind::TLbracket {
                Some(next.loc.dupe())
            } else {
                None
            }
        })
    }

    pub(crate) fn token_after_current_is_lcurly(env: &mut ParserEnv) -> bool {
        lexer_lookahead1_matches(env, |token| token == &TokenKind::TLcurly)
    }

    pub(crate) fn let_token_is_followed_by_in(env: &mut ParserEnv) -> bool {
        match offset_after_current_trivia(env) {
            Ok((Some(offset), _)) => {
                match rest_starts_in_token(&env.lookahead.source_text()[offset..]) {
                    Ok(result) => result,
                    Err(()) => lexer_lookahead1_matches(env, |token| token == &TokenKind::TIn),
                }
            }
            Ok((None, _)) => false,
            Err(()) => lexer_lookahead1_matches(env, |token| token == &TokenKind::TIn),
        }
    }

    pub(crate) fn opaque_token_is_followed_by_type(env: &mut ParserEnv) -> bool {
        current_token_is_followed_by_word(env, "type", |token| token == &TokenKind::TType)
    }

    pub(crate) fn declare_token_is_followed_by_export(env: &mut ParserEnv) -> bool {
        current_token_is_followed_by_word(env, "export", |token| token == &TokenKind::TExport)
    }

    pub(crate) fn declaration_after_declare(env: &mut ParserEnv) -> TokenKind {
        lexer_lookahead1_token_kind(env)
    }

    pub(crate) fn token_after_current_is_identifier_token(env: &mut ParserEnv) -> bool {
        lexer_lookahead1_matches(env, |token| matches!(token, TokenKind::TIdentifier { .. }))
    }

    pub(crate) fn token_after_current_is_string(env: &mut ParserEnv) -> bool {
        lexer_lookahead1_matches(env, |token| matches!(token, TokenKind::TString(..)))
    }

    pub(crate) fn token_after_current_is_identifier_other_than_from(env: &mut ParserEnv) -> bool {
        lexer_lookahead1_matches(
            env,
            |token| matches!(token, TokenKind::TIdentifier { raw, .. } if raw != "from"),
        )
    }

    pub(crate) fn token_after_current_is_lparen(env: &mut ParserEnv) -> bool {
        lexer_lookahead1_matches(env, |token| token == &TokenKind::TLparen)
    }

    pub(crate) fn identifier_token_is_followed_by_namespace(env: &mut ParserEnv) -> bool {
        current_token_is_followed_by_word(
            env,
            "namespace",
            |token| matches!(token, TokenKind::TIdentifier { raw, .. } if raw == "namespace"),
        )
    }

    pub(crate) fn identifier_token_is_followed_by_symbol_type(env: &mut ParserEnv) -> bool {
        current_token_is_followed_by_word(env, "symbol", |token| token == &TokenKind::TSymbolType)
    }

    pub(crate) fn token_after_current_is_new(env: &mut ParserEnv) -> bool {
        current_token_is_followed_by_word(env, "new", |token| token == &TokenKind::TNew)
    }

    pub(crate) fn token_after_current_is_static(env: &mut ParserEnv) -> bool {
        current_token_is_followed_by_word(env, "static", |token| token == &TokenKind::TStatic)
    }

    pub(crate) fn token_after_current_starts_type_call(env: &mut ParserEnv) -> bool {
        match offset_after_current_trivia(env) {
            Ok((Some(offset), _)) => {
                let rest = &env.lookahead.source_text()[offset..];
                rest.starts_with('(') || rest_starts_less_than_token(env.lex_mode(), rest)
            }
            Ok((None, _)) => false,
            Err(()) => lexer_lookahead1_matches(env, |token| {
                matches!(token, TokenKind::TLessThan | TokenKind::TLparen)
            }),
        }
    }

    pub(crate) fn token_after_current_is_pling_or_colon(env: &mut ParserEnv) -> bool {
        match offset_after_current_trivia(env) {
            Ok((Some(offset), _)) => {
                let bytes = env.lookahead.source_text().as_bytes();
                match bytes.get(offset) {
                    Some(b':') => true,
                    Some(b'?') if bytes.get(offset + 1) != Some(&b'.') => true,
                    Some(b'?') => {
                        lexer_lookahead1_matches(env, |token| token == &TokenKind::TPling)
                    }
                    _ => false,
                }
            }
            Ok((None, _)) => false,
            Err(()) => lexer_lookahead1_matches(env, |token| {
                matches!(token, TokenKind::TPling | TokenKind::TColon)
            }),
        }
    }

    pub(crate) fn token_after_current_is_colon(env: &mut ParserEnv) -> bool {
        match offset_after_current_trivia(env) {
            Ok((Some(offset), _)) => {
                env.lookahead.source_text().as_bytes().get(offset) == Some(&b':')
            }
            Ok((None, _)) => false,
            Err(()) => lexer_lookahead1_matches(env, |token| token == &TokenKind::TColon),
        }
    }

    pub(crate) fn type_guard_second_token_kind(env: &mut ParserEnv) -> TokenKind {
        lexer_lookahead1_token_kind(env)
    }

    pub(crate) fn computed_type_property_continuation(env: &mut ParserEnv) -> TokenKind {
        lexer_lookahead1_token_kind(env)
    }

    fn comma_trailing_comments_after_current_if_line_terminator_slow(
        env: &mut ParserEnv,
    ) -> Option<Vec<Comment<Loc>>> {
        let current_start_line = loc(env).start.line;
        let consumed_comments_pos = env.consumed_comments_pos;
        with_lexer_lookahead1(env, |next| {
            if next.loc.start.line == current_start_line {
                None
            } else if next.comments.is_empty() {
                Some(Vec::new())
            } else if next.comments[0].loc.start >= consumed_comments_pos {
                Some(next.comments.to_vec())
            } else {
                match next
                    .comments
                    .iter()
                    .position(|comment| comment.loc.start >= consumed_comments_pos)
                {
                    Some(index) => Some(next.comments[index..].to_vec()),
                    None => Some(Vec::new()),
                }
            }
        })
    }

    pub(crate) fn comma_trailing_comments_after_current_if_line_terminator(
        env: &mut ParserEnv,
    ) -> Option<Vec<Comment<Loc>>> {
        let current_loc = loc(env).dupe();
        let consumed_comments_pos = env.consumed_comments_pos;
        let mut offset = env.lookahead.current_end_offset();
        let source = env.lookahead.source_text();
        let mut line = current_loc.end.line;
        let mut column = current_loc.end.column;
        let mut has_line_terminator = false;
        let mut comments = Vec::new();

        while offset < source.len() {
            let rest = &source[offset..];
            if matches!(rest.as_bytes()[0], b' ' | b'\t' | 0x0B | 0x0C) {
                offset += 1;
                column += 1;
            } else if rest.starts_with("\r\n") {
                offset += 2;
                line += 1;
                column = 0;
                has_line_terminator = true;
            } else if rest.starts_with('\n') || rest.starts_with('\r') {
                offset += 1;
                line += 1;
                column = 0;
                has_line_terminator = true;
            } else if rest.starts_with('\u{2028}') || rest.starts_with('\u{2029}') {
                let ch = rest.chars().next().unwrap();
                offset += ch.len_utf8();
                line += 1;
                column = 0;
                has_line_terminator = true;
            } else if rest.starts_with("//") {
                let start = Position { line, column };
                offset += 2;
                column += 2;
                let text_start = offset;
                while offset < source.len() {
                    let rest = &source[offset..];
                    if rest.starts_with("\r\n")
                        || rest.starts_with('\n')
                        || rest.starts_with('\r')
                        || rest.starts_with('\u{2028}')
                        || rest.starts_with('\u{2029}')
                    {
                        break;
                    }
                    let ch = rest.chars().next().unwrap();
                    offset += ch.len_utf8();
                    column += ch.len_utf8() as i32;
                }
                let end = Position { line, column };
                let loc = Loc {
                    source: current_loc.source.dupe(),
                    start,
                    end,
                };
                if loc.start >= consumed_comments_pos {
                    comments.push(Comment {
                        loc,
                        kind: CommentKind::Line,
                        text: Arc::from(&source[text_start..offset]),
                        on_newline: current_loc.end.line < start.line,
                    });
                }
            } else if let Some(after_open) = rest.strip_prefix("/*") {
                let after_whitespace = after_open.trim_start_matches(char::is_whitespace);
                if after_whitespace.starts_with("::")
                    || after_whitespace.starts_with(':')
                    || after_whitespace.starts_with("flow-include")
                {
                    return comma_trailing_comments_after_current_if_line_terminator_slow(env);
                }

                let start = Position { line, column };
                offset += 2;
                column += 2;
                let text_start = offset;
                loop {
                    if offset >= source.len() {
                        return comma_trailing_comments_after_current_if_line_terminator_slow(env);
                    }
                    let rest = &source[offset..];
                    if rest.starts_with("*/") {
                        let text_end = offset;
                        offset += 2;
                        column += 2;
                        let end = Position { line, column };
                        let loc = Loc {
                            source: current_loc.source.dupe(),
                            start,
                            end,
                        };
                        if loc.start >= consumed_comments_pos {
                            comments.push(Comment {
                                loc,
                                kind: CommentKind::Block,
                                text: Arc::from(&source[text_start..text_end]),
                                on_newline: current_loc.end.line < start.line,
                            });
                        }
                        break;
                    } else if rest.starts_with("*-/") {
                        return comma_trailing_comments_after_current_if_line_terminator_slow(env);
                    } else if rest.starts_with("\r\n") {
                        offset += 2;
                        line += 1;
                        column = 0;
                        has_line_terminator = true;
                    } else if rest.starts_with('\n') || rest.starts_with('\r') {
                        offset += 1;
                        line += 1;
                        column = 0;
                        has_line_terminator = true;
                    } else if rest.starts_with('\u{2028}') || rest.starts_with('\u{2029}') {
                        let ch = rest.chars().next().unwrap();
                        offset += ch.len_utf8();
                        line += 1;
                        column = 0;
                        has_line_terminator = true;
                    } else {
                        let ch = rest.chars().next().unwrap();
                        offset += ch.len_utf8();
                        column += ch.len_utf8() as i32;
                    }
                }
            } else if let Some(
                ch @ ('\u{00a0}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200a}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'),
            ) = rest.chars().next()
            {
                offset += ch.len_utf8();
                column += ch.len_utf8() as i32;
            } else {
                return if has_line_terminator {
                    Some(comments)
                } else {
                    None
                };
            }
        }

        Some(comments)
    }

    pub(crate) fn has_line_terminator_after_current(env: &mut ParserEnv) -> bool {
        match offset_after_current_trivia(env) {
            Ok((_, has_line_terminator)) => has_line_terminator,
            Err(()) => lexer_lookahead1_token_kind_on_current_end_line(env).is_none(),
        }
    }

    pub(crate) fn current_token_is_followed_by_implicit_semicolon(env: &mut ParserEnv) -> bool {
        match lexer_lookahead1_token_kind_on_current_start_line(env) {
            None => true,
            Some(next) => match next {
                TokenKind::TEof | TokenKind::TRcurly => true,
                TokenKind::TSemicolon => false,
                _ => false,
            },
        }
    }

    pub(crate) fn token_kind_is_identifier(t: &TokenKind) -> bool {
        match t {
            t if token_is_strict_reserved(t) => true,
            TokenKind::TType
            | TokenKind::TOpaque
            | TokenKind::TOf
            | TokenKind::TDeclare
            | TokenKind::TAsync
            | TokenKind::TAwait
            | TokenKind::TEnum
            | TokenKind::TMatch
            | TokenKind::TRecord
            | TokenKind::TPound
            | TokenKind::TIdentifier { .. }
            | TokenKind::TReadonly
            | TokenKind::TWriteonly
            | TokenKind::TInfer => true,
            _ => false,
        }
    }

    pub(crate) fn is_identifier(env: &mut ParserEnv) -> bool {
        token_kind_is_identifier(token(env))
    }

    pub(crate) fn is_type_identifier(env: &mut ParserEnv) -> bool {
        let lev_mode = env.lex_mode();
        token_is_type_identifier_under_lex_mode(lev_mode, token(env))
    }

    pub(crate) fn is_identifier_name(env: &mut ParserEnv) -> bool {
        is_identifier(env) || is_type_identifier(env)
    }

    fn lookahead1_kind_is_identifier_name(token_kind: &TokenKind) -> bool {
        identifier_name_for_token(token_kind).is_some()
    }

    fn lookahead1_kind_is_object_key(token_kind: &TokenKind, is_class: bool) -> bool {
        match token_kind {
            TokenKind::TString { .. }
            | TokenKind::TNumber { .. }
            | TokenKind::TBigint { .. }
            | TokenKind::TLbracket => true,
            TokenKind::TPound if is_class => true,
            t => lookahead1_kind_is_identifier_name(t),
        }
    }

    pub(crate) fn current_token_is_followed_by_identifier(env: &mut ParserEnv) -> bool {
        lexer_lookahead1_matches(env, token_kind_is_identifier)
    }

    pub(crate) fn current_token_is_followed_by_type_identifier(env: &mut ParserEnv) -> bool {
        let lex_mode = env.lex_mode();
        lexer_lookahead1_matches(env, |token| {
            token_is_type_identifier_under_lex_mode(lex_mode, token)
        })
    }

    pub(crate) fn current_token_is_followed_by_identifier_name(env: &mut ParserEnv) -> bool {
        lexer_lookahead1_matches(env, lookahead1_kind_is_identifier_name)
    }

    #[expect(dead_code)]
    pub(crate) fn is_object_key(env: &mut ParserEnv, is_class: bool) -> bool {
        match token(env) {
            TokenKind::TString { .. }
            | TokenKind::TNumber { .. }
            | TokenKind::TBigint { .. }
            | TokenKind::TLbracket => true,
            TokenKind::TPound if is_class => true,
            t if token_is_reserved(t) => true,
            _ => is_identifier_name(env),
        }
    }

    pub(crate) fn current_token_is_followed_by_object_key(
        env: &mut ParserEnv,
        is_class: bool,
    ) -> bool {
        lexer_lookahead1_matches(env, |token| lookahead1_kind_is_object_key(token, is_class))
    }

    pub(crate) fn class_member_continuation_allows_modifier(env: &mut ParserEnv) -> bool {
        lexer_lookahead1_matches(env, |token| {
            token == &TokenKind::TMult || lookahead1_kind_is_object_key(token, true)
        })
    }

    pub(crate) fn async_token_continues_on_same_line(env: &mut ParserEnv) -> bool {
        token(env) == &TokenKind::TAsync
            && match offset_after_current_trivia(env) {
                Ok((Some(_), false)) => true,
                Ok((None, false)) => lexer_lookahead1_matches_on_current_end_line(env, |_| true),
                Ok(_) => false,
                Err(()) => lexer_lookahead1_matches_on_current_end_line(env, |_| true),
            }
    }

    pub(crate) fn async_token_can_start_arrow_function(env: &mut ParserEnv) -> bool {
        token(env) == &TokenKind::TAsync
            && match offset_after_current_trivia(env) {
                Ok((Some(offset), false)) => {
                    let rest = &env.lookahead.source_text()[offset..];
                    match rest.as_bytes().first() {
                        Some(b'=') => rest.starts_with("=>"),
                        Some(b'(') => true,
                        Some(b'<') => rest_starts_less_than_token(env.lex_mode(), rest),
                        Some(byte) if ascii_identifier_start(*byte) => true,
                        Some(b'\\') | Some(0x80..=0xff) => {
                            lexer_lookahead1_matches_on_current_end_line(env, |token| {
                                token_kind_is_identifier(token)
                                    || matches!(
                                        token,
                                        TokenKind::TLparen
                                            | TokenKind::TLessThan
                                            | TokenKind::TArrow
                                    )
                            })
                        }
                        _ => false,
                    }
                }
                Ok((None, false)) => false,
                Ok(_) => false,
                Err(()) => lexer_lookahead1_matches_on_current_end_line(env, |token| {
                    token_kind_is_identifier(token)
                        || matches!(
                            token,
                            TokenKind::TLparen | TokenKind::TLessThan | TokenKind::TArrow
                        )
                }),
            }
    }

    pub(crate) fn async_token_is_followed_by_function(env: &mut ParserEnv) -> bool {
        current_token_is_followed_by_word_on_current_end_line(env, "function", |token| {
            token == &TokenKind::TFunction
        })
    }

    pub(crate) fn async_token_is_followed_by_component(env: &mut ParserEnv) -> bool {
        current_token_is_followed_by_word_on_current_end_line(
            env,
            "component",
            |token| matches!(token, TokenKind::TIdentifier { raw, .. } if raw == "component"),
        )
    }

    pub(crate) fn async_token_is_followed_by_hook(env: &mut ParserEnv) -> bool {
        current_token_is_followed_by_word_on_current_end_line(
            env,
            "hook",
            |token| matches!(token, TokenKind::TIdentifier { raw, .. } if raw == "hook"),
        )
    }

    pub(crate) fn hook_token_is_followed_by_identifier(env: &mut ParserEnv) -> bool {
        lexer_lookahead1_matches_on_current_end_line(env, token_kind_is_identifier)
    }

    pub(crate) fn record_token_is_followed_by_identifier(env: &mut ParserEnv) -> bool {
        lexer_lookahead1_matches_on_current_end_line(env, token_kind_is_identifier)
    }

    pub(crate) fn abstract_token_is_followed_by_class(env: &mut ParserEnv) -> bool {
        (env.should_parse_types() || !has_line_terminator_after_current(env))
            && current_token_is_followed_by_word(env, "class", |token| token == &TokenKind::TClass)
    }

    pub(crate) fn match_token_is_followed_by_lparen_on_same_line(env: &mut ParserEnv) -> bool {
        current_token_is_followed_by_char_on_current_end_line(env, '(', |token| {
            token == &TokenKind::TLparen
        })
    }

    pub(crate) fn current_token_is_followed_by_lbracket(env: &mut ParserEnv) -> bool {
        current_token_is_followed_by_char(env, '[', |token| token == &TokenKind::TLbracket)
    }

    pub(crate) fn current_token_is_followed_by_lparen(env: &mut ParserEnv) -> bool {
        current_token_is_followed_by_char(env, '(', |token| token == &TokenKind::TLparen)
    }

    pub(crate) fn current_token_is_followed_by_tless_than(env: &mut ParserEnv) -> bool {
        current_token_is_followed_by_less_than(env)
    }

    pub(crate) fn current_token_is_followed_by_enum(env: &mut ParserEnv) -> bool {
        current_token_is_followed_by_word(env, "enum", |token| token == &TokenKind::TEnum)
    }

    pub(crate) fn current_token_is_followed_by_import_expression_continuation(
        env: &mut ParserEnv,
    ) -> bool {
        match offset_after_current_trivia(env) {
            Ok((Some(offset), _)) => {
                let rest = &env.lookahead.source_text()[offset..];
                rest.starts_with('(') || rest_starts_period_token(rest)
            }
            Ok((None, _)) => false,
            Err(()) => lexer_lookahead1_matches(env, |token| {
                matches!(token, TokenKind::TLparen | TokenKind::TPeriod)
            }),
        }
    }

    pub(crate) fn jsx_type_args_can_follow_current_less_than(env: &mut ParserEnv) -> bool {
        lexer_lookahead1_matches(env, |token| token != &TokenKind::TDiv)
    }

    pub(crate) fn unary_minus_is_followed_by_number(env: &mut ParserEnv) -> Option<TokenKind> {
        match lexer_lookahead1_token_kind(env) {
            t @ TokenKind::TNumber { .. } => Some(t),
            _ => None,
        }
    }

    pub(crate) fn optional_chain_bang_continuation(env: &mut ParserEnv) -> TokenKind {
        lexer_lookahead1_token_kind(env)
    }

    pub(crate) fn assert_operator_bang_continuation(env: &mut ParserEnv) -> TokenKind {
        lexer_lookahead1_token_kind(env)
    }

    pub(crate) fn array_rest_continuation_is_rbracket(env: &mut ParserEnv) -> bool {
        lexer_lookahead1_matches(env, |token| token == &TokenKind::TRbracket)
    }

    pub(crate) fn async_arrow_parameter_continuation_is_arrow(env: &mut ParserEnv) -> bool {
        current_token_is_followed_by_arrow(env)
    }

    pub(crate) fn class_optional_method_continuation_is_call_or_type_args(
        env: &mut ParserEnv,
    ) -> bool {
        match offset_after_current_trivia(env) {
            Ok((Some(offset), _)) => {
                let rest = &env.lookahead.source_text()[offset..];
                rest.starts_with('(') || rest_starts_less_than_token(env.lex_mode(), rest)
            }
            Ok((None, _)) => false,
            Err(()) => lexer_lookahead1_matches(env, |token| {
                matches!(token, TokenKind::TLparen | TokenKind::TLessThan)
            }),
        }
    }

    pub(crate) fn token_after_current_implies_identifier(env: &mut ParserEnv) -> bool {
        match offset_after_current_trivia(env) {
            Ok((Some(offset), _)) => {
                let rest = &env.lookahead.source_text()[offset..];
                if rest_starts_less_than_token(env.lex_mode(), rest)
                    || rest.starts_with(':')
                    || rest_starts_assign_token(rest)
                    || rest_starts_not_token(rest)
                    || rest.starts_with(';')
                    || rest.starts_with('(')
                    || rest.starts_with('}')
                {
                    true
                } else {
                    match rest_starts_pling_token(rest) {
                        Ok(result) => result,
                        Err(()) => lexer_lookahead1_matches(env, |token| {
                            matches!(
                                token,
                                TokenKind::TLessThan
                                    | TokenKind::TColon
                                    | TokenKind::TAssign
                                    | TokenKind::TNot
                                    | TokenKind::TSemicolon
                                    | TokenKind::TLparen
                                    | TokenKind::TPling
                                    | TokenKind::TRcurly
                            )
                        }),
                    }
                }
            }
            Ok((None, _)) => false,
            Err(()) => lexer_lookahead1_matches(env, |token| {
                matches!(
                    token,
                    TokenKind::TLessThan
                        | TokenKind::TColon
                        | TokenKind::TAssign
                        | TokenKind::TNot
                        | TokenKind::TSemicolon
                        | TokenKind::TLparen
                        | TokenKind::TPling
                        | TokenKind::TRcurly
                )
            }),
        }
    }

    pub(crate) fn class_static_continuation_allows_modifier(env: &mut ParserEnv) -> bool {
        match offset_after_current_trivia(env) {
            Ok((Some(offset), _)) => {
                let rest = &env.lookahead.source_text()[offset..];
                if rest_starts_assign_token(rest)
                    || rest.starts_with(':')
                    || rest_starts_less_than_token(env.lex_mode(), rest)
                    || rest_starts_not_token(rest)
                    || rest.starts_with('(')
                    || rest.starts_with('}')
                    || rest.starts_with(';')
                {
                    false
                } else {
                    match rest_starts_pling_token(rest) {
                        Ok(result) => !result,
                        Err(()) => lexer_lookahead1_matches(env, |token| {
                            !matches!(
                                token,
                                TokenKind::TAssign
                                    | TokenKind::TColon
                                    | TokenKind::TEof
                                    | TokenKind::TLessThan
                                    | TokenKind::TLparen
                                    | TokenKind::TNot
                                    | TokenKind::TPling
                                    | TokenKind::TRcurly
                                    | TokenKind::TSemicolon
                            )
                        }),
                    }
                }
            }
            Ok((None, _)) => false,
            Err(()) => lexer_lookahead1_matches(env, |token| {
                !matches!(
                    token,
                    TokenKind::TAssign
                        | TokenKind::TColon
                        | TokenKind::TEof
                        | TokenKind::TLessThan
                        | TokenKind::TLparen
                        | TokenKind::TNot
                        | TokenKind::TPling
                        | TokenKind::TRcurly
                        | TokenKind::TSemicolon
                )
            }),
        }
    }

    pub(crate) fn ambient_computed_property_is_indexer(env: &mut ParserEnv) -> bool {
        lexer_lookahead1_matches(env, |token| token == &TokenKind::TColon)
    }

    pub(crate) fn record_modifier_allows_modifier(env: &mut ParserEnv) -> bool {
        lexer_lookahead1_matches(env, |token| {
            !matches!(
                token,
                TokenKind::TColon
                    | TokenKind::TLessThan
                    | TokenKind::TLparen
                    | TokenKind::TEof
                    | TokenKind::TRcurly
            )
        })
    }

    pub(crate) fn is_function(env: &mut ParserEnv) -> bool {
        token(env) == &TokenKind::TFunction
            || (token(env) == &TokenKind::TAsync && async_token_is_followed_by_function(env))
    }

    pub(crate) fn is_hook(env: &mut ParserEnv) -> bool {
        match token(env) {
            TokenKind::TIdentifier { raw, .. } if raw == "hook" => {
                env.parse_options().components && hook_token_is_followed_by_identifier(env)
            }
            TokenKind::TAsync => {
                env.parse_options().components && async_token_is_followed_by_hook(env)
            }
            _ => false,
        }
    }

    pub(crate) fn is_class(env: &mut ParserEnv) -> bool {
        match token(env) {
            TokenKind::TClass | TokenKind::TAt => true,
            TokenKind::TIdentifier { raw, .. } if raw == "abstract" => {
                abstract_token_is_followed_by_class(env)
            }
            _ => false,
        }
    }

    pub(crate) fn is_component(env: &mut ParserEnv) -> bool {
        if !env.parse_options().components {
            return false;
        }
        match token(env) {
            TokenKind::TIdentifier { raw, .. } if raw == "component" => {
                current_token_is_followed_by_identifier(env)
            }
            TokenKind::TAsync => async_token_is_followed_by_component(env),
            _ => false,
        }
    }

    pub(crate) fn is_record(env: &mut ParserEnv) -> bool {
        env.parse_options().records && token(env) == &TokenKind::TRecord && {
            record_token_is_followed_by_identifier(env)
        }
    }

    pub(crate) fn is_renders_ident(env: &mut ParserEnv) -> bool {
        match token(env) {
            TokenKind::TIdentifier { raw, .. } if raw == "renders" => true,
            _ => false,
        }
    }
}

/******************************************************************************/
/* Errors                                                                     */
/******************************************************************************/

impl<'a> ParserEnv<'a> {
    /// Complains about an error at the location of the lookahead  
    pub(crate) fn error(&mut self, e: ParseError) -> Result<(), Rollback> {
        let loc = peek::loc(self).dupe();
        self.error_at(loc, e)
    }

    fn get_unexpected_error(expected: Option<String>, token: &TokenKind) -> ParseError {
        let unexpected = TokenKind::explanation_of_token(token);
        match expected {
            Some(expected_msg) => ParseError::UnexpectedWithExpected(unexpected, expected_msg),
            None => ParseError::Unexpected(unexpected),
        }
    }

    pub(crate) fn error_unexpected(&mut self, expected: Option<String>) -> Result<(), Rollback> {
        /* So normally we consume the lookahead lex result when Eat.token calls
         * Parser_env.advance, which will add any lexing errors to our list of errors.
         * However, raising an unexpected error for a lookahead is kind of like
         * consuming that token, so we should process any lexing errors before
         * complaining about the unexpected token */
        for (loc, e) in peek::errors(self) {
            self.error_at(loc, e)?;
        }
        let error = Self::get_unexpected_error(expected, peek::token(self));
        self.error(error)?;
        Ok(())
    }

    pub(crate) fn error_on_decorators(
        &mut self,
        decorators: &[class::Decorator<Loc, Loc>],
    ) -> Result<(), Rollback> {
        for d in decorators {
            self.error_at(d.loc.dupe(), ParseError::UnsupportedDecorator)?;
        }
        Ok(())
    }

    pub(crate) fn error_nameless_declaration(&mut self, kind: &str) -> Result<(), Rollback> {
        let expected = if self.in_export() {
            format!(
                "an identifier. When exporting a {} as a named export, you must specify a {} name. Did you mean `export default {} ...`?",
                kind, kind, kind
            )
        } else {
            "an identifier".to_string()
        };
        self.error_unexpected(Some(expected))?;
        Ok(())
    }

    pub(crate) fn strict_error(&mut self, e: ParseError) -> Result<(), Rollback> {
        if self.in_strict_mode() {
            self.error(e)?;
        }
        Ok(())
    }

    pub(crate) fn strict_error_at(
        &mut self,
        (loc, error): (Loc, ParseError),
    ) -> Result<(), Rollback> {
        if self.in_strict_mode() {
            self.error_at(loc, error)?;
        }
        Ok(())
    }

    pub(crate) fn function_as_statement_error_at(&mut self, loc: Loc) -> Result<(), Rollback> {
        self.error_at(
            loc,
            ParseError::FunctionAsStatement {
                in_strict_mode: self.in_strict_mode(),
            },
        )?;
        Ok(())
    }
}

/// Consume zero or more tokens
pub mod eat {
    use super::*;

    /// Consume a single token
    pub(crate) fn token(env: &mut ParserEnv) -> Result<(), Rollback> {
        /* If there's a token_sink, emit the lexed token before moving forward.
         * Skip T_EOF — upstream Hermes' token stream does not include a
         * sentinel end-of-input token, so neither should ours. The serializer
         * maps T_EOF to Punctuator with empty value
         * (`token_kind_to_estree_type_index` in serializer.rs), and the adapter
         * used to pop it in JS; emit-time skip is the cleaner fix. */
        let token_sink = env.token_sink.take();
        match token_sink {
            None => {}
            Some(token_sink) if matches!(peek::token(env), TokenKind::TEof) => {
                env.token_sink = Some(token_sink);
            }
            Some(token_sink) => {
                let token_loc = peek::loc(env).dupe();
                let token = peek::token(env).clone();
                let token_loc = match &token {
                    TokenKind::TInterpreter(loc, _) => loc.dupe(),
                    _ => token_loc,
                };
                let token_sink = send_token_to_sink(
                    token_sink,
                    TokenSinkResult {
                        token_loc,
                        token_kind: token,
                        /*
                         * The lex mode is useful because it gives context to some
                         * context-sensitive tokens.
                         *
                         * Some examples of such tokens include:
                         *
                         * `=>` - Part of an arrow function? or part of a type annotation?
                         * `<`  - A less-than? Or an opening to a JSX element?
                         * ...etc...
                         */
                        token_context: env.lex_mode(),
                    },
                );
                env.token_sink = Some(token_sink);
            }
        }

        if !env.lookahead_0().errors.is_empty() {
            for (loc, e) in peek::errors(env) {
                env.error_at(loc, e)?;
            }
        }

        let result = env.lookahead.take_0();
        if !result.comments.is_empty() {
            env.comments.extend(result.comments);
        }
        env.last_lex_result = Some((result.loc, result.token_kind));
        Ok(())
    }

    /* `maybe(env, t)` eats the next token and returns `true` if it is `t`, else returns `false` */
    pub(crate) fn maybe(env: &mut ParserEnv, t: TokenKind) -> Result<bool, Rollback> {
        let is_t = peek::token(env) == &t;
        if is_t {
            token(env)?;
        }
        Ok(is_t)
    }

    pub(crate) fn push_lex_mode(env: &mut ParserEnv, mode: LexMode) {
        env.lex_mode_stack.push(mode);
        let new_lex_mode = env.lex_mode();
        env.lookahead.reset(new_lex_mode);
    }

    pub(crate) fn pop_lex_mode(env: &mut ParserEnv) {
        env.lex_mode_stack
            .pop()
            .expect("Popping lex mode from empty stack");
        let new_lex_mode = env.lex_mode();
        env.lookahead.reset(new_lex_mode);
    }

    pub(crate) fn double_pop_lex_mode(env: &mut ParserEnv) {
        env.lex_mode_stack
            .pop()
            .expect("Popping lex mode from empty stack");
        env.lex_mode_stack
            .pop()
            .expect("Popping lex mode from empty stack");
        let new_lex_mode = env.lex_mode();
        env.lookahead.reset(new_lex_mode);
    }

    pub(crate) fn rescan_as_template_from<'a>(
        env: &mut ParserEnv<'a>,
        prev_cursor: wrapped_lex_env::WrappedLexCursor,
    ) {
        env.lookahead.rescan_template_part_from(prev_cursor);
    }

    pub(crate) fn trailing_comments(env: &mut ParserEnv) -> Vec<Comment<Loc>> {
        let (loc_end, is_comma) = {
            let lookahead = env.lookahead_0();
            (
                lookahead.loc.end,
                matches!(lookahead.token_kind, TokenKind::TComma),
            )
        };
        if is_comma
            && let Some(trailing_after_comma) =
                peek::comma_trailing_comments_after_current_if_line_terminator(env)
        {
            let trailing_before_comma = peek::comments(env);
            let trailing_after_comma: Vec<Comment<Loc>> = trailing_after_comma
                .into_iter()
                .filter(|comment| comment.loc.start.line <= loc_end.line)
                .collect();
            let mut trailing = trailing_before_comma;
            trailing.extend(trailing_after_comma);
            env.consume_comments_until(Position {
                line: loc_end.line + 1,
                column: 0,
            });
            trailing
        } else {
            let trailing = peek::comments(env);
            env.consume_comments_until(loc_end);
            trailing
        }
    }

    pub(crate) fn comments_until_next_line(env: &mut ParserEnv) -> Vec<Comment<Loc>> {
        if let Some(last_loc) = env.last_loc() {
            let last_loc_end = last_loc.end;
            let comments: Vec<Comment<Loc>> = peek::comments(env)
                .into_iter()
                .filter(|comment| comment.loc.start.line <= last_loc_end.line)
                .collect();
            env.consume_comments_until(Position {
                line: last_loc_end.line + 1,
                column: 0,
            });
            comments
        } else {
            Vec::new()
        }
    }

    pub(crate) fn program_comments(env: &mut ParserEnv) -> Vec<Comment<Loc>> {
        let comments = peek::comments(env);

        for (i, comment) in comments.iter().enumerate().rev() {
            if comment.text.contains("@flow") {
                env.consumed_comments_pos = comment.loc.end;
                return comments[..=i].to_vec();
            }
        }

        /* If there is no @flow directive, consider the first block comment a program comment if
        it starts with '/' '*' '*' */
        match comments.first() {
            Some(comment)
                if matches!(comment.kind, CommentKind::Block) && comment.text.starts_with('*') =>
            {
                env.consumed_comments_pos = comment.loc.end;
                vec![comment.clone()]
            }
            _ => Vec::new(),
        }
    }
}

pub mod expect {
    use super::*;

    pub(crate) fn get_error(env: &mut ParserEnv, t: &TokenKind) -> (Loc, ParseError) {
        let expected = TokenKind::explanation_of_token_with_article(t);
        (
            peek::loc(env).clone(),
            ParserEnv::get_unexpected_error(Some(expected), peek::token(env)),
        )
    }

    pub(crate) fn error(env: &mut ParserEnv, t: &TokenKind) -> Result<(), Rollback> {
        let expected = TokenKind::explanation_of_token_with_article(t);
        env.error_unexpected(Some(expected))?;
        Ok(())
    }

    pub(crate) fn token(env: &mut ParserEnv, t: TokenKind) -> Result<(), Rollback> {
        if peek::token(env) != &t {
            error(env, &t)?;
        }
        eat::token(env)?;
        Ok(())
    }

    /// `token_maybe(env, t)` eats a token if it is `t`, and errors without consuming if
    /// not. Returns whether it consumed a token, like `eat::maybe`.
    pub(crate) fn token_maybe(env: &mut ParserEnv, t: TokenKind) -> Result<bool, Rollback> {
        let ate = eat::maybe(env, t.clone())?;
        if !ate {
            error(env, &t)?;
        }
        Ok(ate)
    }

    /// `token_opt(env, t)` eats a token if it is `t`, and errors without consuming if not.
    /// This differs from `token`, which always consumes. Only use `token_opt` when it's ok for
    /// the parser to not advance, like if you are guaranteed that something else has eaten a
    /// token.
    pub(crate) fn token_opt(env: &mut ParserEnv, t: TokenKind) -> Result<(), Rollback> {
        token_maybe(env, t)?;
        Ok(())
    }

    pub(crate) fn identifier(env: &mut ParserEnv, name: &str) -> Result<(), Rollback> {
        let t = peek::token(env);
        match t {
            TokenKind::TIdentifier { raw, .. } if raw == name => {}
            _ => {
                let expected = format!("the identifier `{}`", name);
                ParserEnv::error_unexpected(env, Some(expected))?;
            }
        }
        eat::token(env)?;
        Ok(())
    }
}

/// This module allows you to try parsing and rollback if you need. This is not
/// cheap and its usage is strongly discouraged
pub mod try_parse {
    use super::*;

    #[derive(Debug)]
    pub enum ParseResult<T> {
        ParsedSuccessfully(T),
        FailedToParse,
    }

    pub struct Rollback;

    struct SavedState {
        saved_errors_len: usize,
        saved_comments_len: usize,
        saved_parenthesized_expression_locations_len: usize,
        saved_last_lex_result: Option<(Loc, TokenKind)>,
        saved_lex_mode_stack: Vec<LexMode>,
        saved_lex_cursor: wrapped_lex_env::WrappedLexCursor,
        saved_consumed_comments_pos: Position,
        token_buffer: bool,
    }

    fn save_state<'a>(env: &mut ParserEnv<'a>) -> SavedState {
        let token_buffer = match env.token_sink.take() {
            None => false,
            Some(orig_token_sink) => {
                env.token_sink = Some(TokenSink::Buffered {
                    next: Box::new(orig_token_sink),
                    buffer: VecDeque::new(),
                });
                true
            }
        };

        SavedState {
            saved_errors_len: env.errors.len(),
            saved_comments_len: env.comments.len(),
            saved_parenthesized_expression_locations_len: env
                .parenthesized_expression_locations
                .as_ref()
                .map_or(0, Vec::len),
            saved_last_lex_result: env.last_lex_result.clone(),
            saved_lex_mode_stack: env.lex_mode_stack.clone(),
            saved_lex_cursor: env.lookahead.committed_cursor(),
            saved_consumed_comments_pos: env.consumed_comments_pos,
            token_buffer,
        }
    }

    fn reset_token_sink<'a>(env: &mut ParserEnv<'a>, token_buffer_info: bool, flush: bool) {
        if token_buffer_info {
            let token_sink = env.token_sink.take();
            match token_sink {
                Some(TokenSink::Buffered { next, buffer }) => {
                    let mut next = *next;
                    if flush {
                        for token_data in buffer {
                            next = send_token_to_sink(next, token_data);
                        }
                    }
                    env.token_sink = Some(next);
                }
                None | Some(TokenSink::Direct(_)) => unreachable!(),
            }
        }
    }

    fn rollback_state<'a, T>(env: &mut ParserEnv<'a>, saved_state: SavedState) -> ParseResult<T> {
        reset_token_sink(env, saved_state.token_buffer, false);
        env.errors.truncate(saved_state.saved_errors_len);
        env.comments.truncate(saved_state.saved_comments_len);
        if let Some(locations) = &mut env.parenthesized_expression_locations {
            locations.truncate(saved_state.saved_parenthesized_expression_locations_len);
        }
        env.last_lex_result = saved_state.saved_last_lex_result;
        env.lex_mode_stack = saved_state.saved_lex_mode_stack;
        env.consumed_comments_pos = saved_state.saved_consumed_comments_pos;
        let new_lex_mode = env.lex_mode();
        env.lookahead
            .reset_to(new_lex_mode, saved_state.saved_lex_cursor);

        ParseResult::FailedToParse
    }

    fn success<'a, T>(
        env: &mut ParserEnv<'a>,
        saved_state: SavedState,
        result: T,
    ) -> ParseResult<T> {
        reset_token_sink(env, saved_state.token_buffer, true);
        ParseResult::ParsedSuccessfully(result)
    }

    pub(crate) fn to_parse<'a, T, F>(env: &mut ParserEnv<'a>, parse: F) -> ParseResult<T>
    where
        F: FnOnce(&mut ParserEnv<'a>) -> Result<T, Rollback>,
    {
        let saved_state = save_state(env);
        match parse(env) {
            Ok(result) => success(env, saved_state, result),
            Err(_) => rollback_state(env, saved_state),
        }
    }

    pub(crate) fn or_else<'a, T, F>(env: &mut ParserEnv<'a>, parse: F, fallback: T) -> T
    where
        F: FnOnce(&mut ParserEnv<'a>) -> Result<T, Rollback>,
    {
        match to_parse(env, parse) {
            ParseResult::ParsedSuccessfully(result) => result,
            ParseResult::FailedToParse => fallback,
        }
    }
}
