/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Binary serializer for the Flow AST → Hermes wire format.
//!
//! Converts Flow parser AST into the binary protocol expected by
//! `FlowParserDeserializer.js`, producing two buffers:
//! - `program_buffer: Vec<u32>` — nodes and their properties
//! - `position_buffer: Vec<PositionResult>` — source positions in UTF-16

use std::borrow::Cow;

use flow_parser::TokenSinkResult;
use flow_parser::ast;
use flow_parser::ast::CommentKind;
use flow_parser::estree_translator::is_optional_chain;
use flow_parser::loc::Loc;
use flow_parser::loc::Position;
use flow_parser::parse_error::ParseError;
use flow_parser::source_location::SourceLocationTable;
use flow_parser::token::TokenKind;

use crate::SourceType;
use crate::babel_adapter::BabelMetadata;
use crate::babel_adapter::COMPONENT_FUNCTION_FLAG;
use crate::babel_adapter::COMPONENT_PATTERN_OPTIONAL_FLAG;
use crate::babel_adapter::COMPONENT_REST_TYPE_ANNOTATION_FLAG;
use crate::babel_adapter::HOOK_FUNCTION_FLAG;
use crate::node_kinds::NodeKind;
use crate::position::PositionInfo;
use crate::position::PositionResult;
use crate::position::compute_positions;

// Generated dispatch methods (serialize_statement_dispatch,
// serialize_expression_dispatch, serialize_type_dispatch) in a
// separate impl block.
include!("serializer_dispatch.rs");

const LOCATION_IDENTIFIER_NAME: u32 = 1 << 0;

/// The serialization output buffers.
pub struct SerializerBuffers {
    pub program_buffer: Vec<u32>,
    pub position_buffer: Vec<PositionResult>,
    /// Concatenated UTF-8/WTF-8 bytes for every string written via
    /// `write_str`.
    /// The program buffer encodes string references as `(offset+1, len)`,
    /// reserving 0 as the null sentinel for `write_str_opt(None)`.
    pub string_buffer: Vec<u8>,
}

/// Serializes a Flow AST into the Hermes-compatible binary format.
pub struct Serializer<'a> {
    source: &'a str,
    source_locations: SourceLocationTable<'a>,
    babel: bool,
    babel_source_type: SourceType,
    babel_metadata: Option<&'a BabelMetadata>,
    buf: Vec<u32>,
    positions: Vec<PositionInfo>,
    string_buffer: Vec<u8>,
    next_loc_id: u32,
}

impl<'a> Serializer<'a> {
    pub fn new(source: &'a str) -> Self {
        Serializer {
            source,
            source_locations: SourceLocationTable::new(source),
            babel: false,
            babel_source_type: SourceType::Unspecified,
            babel_metadata: None,
            buf: Vec::with_capacity(4096),
            positions: Vec::with_capacity(1024),
            string_buffer: Vec::with_capacity(4096),
            next_loc_id: 0,
        }
    }

    pub fn new_babel(
        source: &'a str,
        babel_metadata: &'a BabelMetadata,
        source_type: SourceType,
    ) -> Self {
        Serializer {
            babel: true,
            babel_source_type: source_type,
            babel_metadata: Some(babel_metadata),
            ..Self::new(source)
        }
    }

    fn is_babel(&self) -> bool {
        self.babel
    }

    fn identifier_kind(&self) -> NodeKind {
        if self.is_babel() {
            NodeKind::BabelIdentifier
        } else {
            NodeKind::Identifier
        }
    }

    fn babel_flags(&self, loc: &Loc) -> u32 {
        self.babel_metadata
            .map_or(0, |metadata| metadata.flags_for_loc(loc))
    }

    // ---------------------------------------------------------------
    // Core encoding methods
    // ---------------------------------------------------------------

    fn write_bool(&mut self, val: bool) {
        self.buf.push(if val { 1 } else { 0 });
    }

    fn write_number(&mut self, val: f64) {
        // Align to 8-byte boundary (2 u32 slots)
        if !self.buf.len().is_multiple_of(2) {
            self.buf.push(0);
        }
        let bits = val.to_bits();
        self.buf.push(bits as u32);
        self.buf.push((bits >> 32) as u32);
    }

    fn write_str_bytes(&mut self, bytes: &[u8]) {
        // Append the bytes to a single shared string buffer and emit a
        // (offset+1, len) reference. The +1 reserves 0 as the null
        // sentinel used by `write_str_opt(None)`. The JS deserializer
        // subtracts 1 before reading from the string buffer.
        let offset = self.string_buffer.len() as u32;
        self.string_buffer.extend_from_slice(bytes);
        self.buf.push(offset + 1);
        self.buf.push(bytes.len() as u32);
    }

    fn write_str(&mut self, s: &str) {
        self.write_str_bytes(s.as_bytes());
    }

    fn write_str_opt(&mut self, s: Option<&str>) {
        match s {
            Some(s) => self.write_str(s),
            None => self.buf.push(0), // null string: only 0, no size word
        }
    }

    fn write_string_literal_value(&mut self, value: &str, raw: &str) {
        let bytes = string_literal_value_bytes(value, raw);
        self.write_str_bytes(bytes.as_ref());
    }

    fn write_string_literal_preserving_value(&mut self, loc: &Loc, value: &str, raw: &str) {
        if self.is_babel() {
            self.write_node_header(NodeKind::BabelStringLiteral, loc);
            self.write_str(value);
            self.write_str(raw);
            return;
        }
        self.write_node_header(NodeKind::Literal, loc);
        self.buf.push(3);
        self.write_str(value);
        self.write_str("string");
        self.write_str(raw);
        self.write_str_opt(None);
        self.write_str_opt(None);
        self.write_str_opt(None);
    }

    fn write_template_cooked_value(&mut self, raw: &str, cooked: &str) {
        let bytes = template_cooked_value_bytes(raw, cooked);
        self.write_str_bytes(bytes.as_ref());
    }

    fn add_loc(&mut self, loc: &Loc) -> u32 {
        let loc_id = self.next_loc_id;
        self.next_loc_id += 1;
        self.positions.push(PositionInfo::start(
            loc_id,
            loc.start.line,
            loc.start.column,
        ));
        self.positions
            .push(PositionInfo::end(loc_id, loc.end.line, loc.end.column));
        loc_id
    }

    fn write_loc_header(&mut self, loc: &Loc, identifier_name: Option<&str>) {
        let loc_id = self.add_loc(loc);
        self.buf.push(loc_id);
        if !self.is_babel() {
            return;
        }
        let mut flags = 0;
        if identifier_name.is_some() {
            flags |= LOCATION_IDENTIFIER_NAME;
        }
        self.buf.push(flags);
        if let Some(identifier_name) = identifier_name {
            self.write_str(identifier_name);
        }
    }

    fn write_node_header(&mut self, kind: NodeKind, loc: &Loc) {
        self.buf.push(kind as u32 + 1); // +1 because 0 = null node
        self.write_loc_header(loc, None);
    }

    fn write_identifier_node_header(&mut self, loc: &Loc, name: &str) {
        self.buf.push(self.identifier_kind() as u32 + 1);
        self.write_loc_header(loc, self.is_babel().then_some(name));
    }

    fn loc_starts_with_keyword(&self, loc: &Loc, keyword: &str) -> bool {
        self.source_locations.loc_starts_with_keyword(loc, keyword)
    }

    fn implicit_declare_from_loc(&self, loc: &Loc) -> bool {
        !self.loc_starts_with_keyword(loc, "declare")
    }

    fn write_null_node(&mut self) {
        self.buf.push(0);
    }

    // ---------------------------------------------------------------
    // Literal encoding helpers
    // ---------------------------------------------------------------

    /// Encode a Literal node with string value.
    /// Wire format: header, valueKind=3, value(str), literalType="string",
    ///              raw(str), bigint(null), regex(null,null)
    fn write_string_literal(&mut self, loc: &Loc, value: &str, raw: &str) {
        if self.is_babel() {
            self.write_node_header(NodeKind::BabelStringLiteral, loc);
            self.write_string_literal_value(value, raw);
            self.write_str(raw);
            return;
        }
        self.write_node_header(NodeKind::Literal, loc);
        self.buf.push(3);
        self.write_string_literal_value(value, raw);
        self.write_str("string");
        self.write_str(raw);
        self.write_str_opt(None);
        self.write_str_opt(None);
        self.write_str_opt(None);
    }

    /// Encode a Literal node with number value.
    /// Wire format: header, valueKind=2, value(f64), literalType="numeric",
    ///              raw(str), bigint(null), regex(null,null)
    fn write_number_literal(&mut self, loc: &Loc, value: f64, raw: &str) {
        if self.is_babel() {
            self.write_node_header(NodeKind::BabelNumericLiteral, loc);
            self.write_number(value);
            self.write_str(raw);
            return;
        }
        self.write_node_header(NodeKind::Literal, loc);
        self.buf.push(2);
        self.write_number(value);
        self.write_str("numeric");
        self.write_str(raw);
        self.write_str_opt(None);
        self.write_str_opt(None);
        self.write_str_opt(None);
    }

    /// Encode a Literal node with bigint value.
    /// Wire format: header, valueKind=0, literalType="bigint", raw(str),
    ///              bigint(str), regex(null,null)
    ///
    /// Per the ESTree spec, the `bigint` property is the decimal-digit string
    /// of the BigInt value, with no numeric separators (`_`) and no `n` suffix.
    fn write_bigint_literal(&mut self, loc: &Loc, raw: &str) {
        if self.is_babel() {
            self.write_node_header(NodeKind::BabelBigIntLiteral, loc);
            self.write_str(&clean_bigint_raw(raw));
            self.write_str(raw);
            return;
        }
        self.write_node_header(NodeKind::Literal, loc);
        self.buf.push(0);
        self.write_str("bigint");
        self.write_str(raw);
        self.write_str(&clean_bigint_raw(raw));
        self.write_str_opt(None);
        self.write_str_opt(None);
    }

    /// Encode a Literal node with null value.
    /// Wire format: header, valueKind=0, literalType="null", raw="null",
    ///              bigint(null), regex(null,null)
    fn write_null_literal(&mut self, loc: &Loc) {
        if self.is_babel() {
            self.write_node_header(NodeKind::BabelNullLiteral, loc);
            return;
        }
        self.write_node_header(NodeKind::Literal, loc);
        self.buf.push(0);
        self.write_str("null");
        self.write_str("null");
        self.write_str_opt(None);
        self.write_str_opt(None);
        self.write_str_opt(None);
    }

    /// Encode a Literal node with boolean value.
    /// Wire format: header, valueKind=1, value(bool), literalType="boolean",
    ///              raw(str), bigint(null), regex(null,null)
    fn write_boolean_literal(&mut self, loc: &Loc, value: bool) {
        if self.is_babel() {
            self.write_node_header(NodeKind::BabelBooleanLiteral, loc);
            self.write_bool(value);
            return;
        }
        self.write_node_header(NodeKind::Literal, loc);
        self.buf.push(1);
        self.write_bool(value);
        self.write_str("boolean");
        self.write_str(if value { "true" } else { "false" });
        self.write_str_opt(None);
        self.write_str_opt(None);
        self.write_str_opt(None);
    }

    /// Encode a Literal node with regex value.
    /// Wire format: header, valueKind=0, literalType="regexp", raw(str),
    ///              bigint(null), pattern(str), flags(str)
    fn write_regex_literal(&mut self, loc: &Loc, raw: &str, pattern: &str, flags: &str) {
        if self.is_babel() {
            self.write_node_header(NodeKind::BabelRegExpLiteral, loc);
            self.write_str(raw);
            self.write_str(pattern);
            self.write_str(flags);
            return;
        }
        self.write_node_header(NodeKind::Literal, loc);
        self.buf.push(0);
        self.write_str("regexp");
        self.write_str(raw);
        self.write_str_opt(None);
        self.write_str(pattern);
        self.write_str(flags);
    }

    // ---------------------------------------------------------------
    // Entry point
    // ---------------------------------------------------------------

    pub fn serialize_program(
        mut self,
        program: &ast::Program<Loc, Loc>,
        errors: &[(Loc, ParseError)],
        tokens: Option<&[TokenSinkResult]>,
    ) -> SerializerBuffers {
        if self.is_babel() {
            self.write_node_header(NodeKind::BabelFile, &program.loc);
            self.write_node_header(NodeKind::BabelProgram, &program.loc);
            self.serialize_program_body(program);
            self.serialize_directives(program);
            self.write_str(if self.babel_source_type(program) {
                "module"
            } else {
                "script"
            });
            self.serialize_interpreter(program);
            self.serialize_comments(program);
        } else {
            let loc_id = self.add_loc(&program.loc);
            self.buf.push(loc_id);
            self.serialize_program_body(program);
            self.serialize_comments(program);
            self.serialize_interpreter(program);
            match tokens {
                Some(tokens) => self.serialize_tokens(tokens),
                None => self.buf.push(0),
            }
            self.serialize_errors(errors);
        }

        let position_buffer = compute_positions(self.source.as_bytes(), &mut self.positions);

        SerializerBuffers {
            program_buffer: self.buf,
            position_buffer,
            string_buffer: self.string_buffer,
        }
    }

    fn serialize_program_body(&mut self, program: &ast::Program<Loc, Loc>) {
        let directive_count = if self.is_babel() {
            program
                .statements
                .iter()
                .take_while(|statement| self.directive_parts(statement).is_some())
                .count()
        } else {
            0
        };
        self.buf
            .push((program.statements.len() - directive_count) as u32);
        for stmt in program.statements.iter().skip(directive_count) {
            self.serialize_statement(stmt);
        }
    }

    fn serialize_directives(&mut self, program: &ast::Program<Loc, Loc>) {
        let directive_count = program
            .statements
            .iter()
            .take_while(|statement| self.directive_parts(statement).is_some())
            .count();
        self.buf.push(directive_count as u32);
        for statement in program.statements.iter().take(directive_count) {
            self.serialize_directive(statement);
        }
    }

    fn serialize_comments(&mut self, program: &ast::Program<Loc, Loc>) {
        self.buf.push(program.all_comments.len() as u32);
        for comment in program.all_comments.iter() {
            if self.is_babel() {
                self.write_str(match comment.kind {
                    CommentKind::Block => "CommentBlock",
                    CommentKind::Line => "CommentLine",
                });
                self.write_loc_header(&comment.loc, None);
            } else {
                self.buf.push(match comment.kind {
                    CommentKind::Block => 0,
                    CommentKind::Line => 1,
                });
                let loc_id = self.add_loc(&comment.loc);
                self.buf.push(loc_id);
            }
            self.write_str(&comment.text);
        }
    }

    fn serialize_interpreter(&mut self, program: &ast::Program<Loc, Loc>) {
        match &program.interpreter {
            Some((interp_loc, value)) => {
                // 213: InterpreterDirective — value(String)
                self.write_node_header(NodeKind::InterpreterDirective, interp_loc);
                self.write_str(value);
            }
            None => self.write_null_node(),
        }
    }

    fn serialize_tokens(&mut self, tokens: &[TokenSinkResult]) {
        self.buf.push(tokens.len() as u32);
        for token in tokens {
            self.buf
                .push(token_kind_to_estree_type_index(&token.token_kind));
            let loc_id = self.add_loc(&token.token_loc);
            self.buf.push(loc_id);
            let value = token.token_kind.to_value();
            self.write_str(&value);
        }
    }

    fn serialize_errors(&mut self, errors: &[(Loc, ParseError)]) {
        self.buf.push(errors.len() as u32);
        for (loc, err) in errors.iter() {
            let loc_id = self.add_loc(loc);
            self.buf.push(loc_id);
            let msg = format!("{}", err);
            self.write_str(&msg);
        }
    }

    fn directive_parts<'b>(
        &self,
        statement: &'b ast::statement::Statement<Loc, Loc>,
    ) -> Option<(&'b Loc, &'b str, &'b ast::expression::Expression<Loc, Loc>)> {
        let ast::statement::StatementInner::Expression { loc, inner } = &**statement else {
            return None;
        };
        Some((loc, inner.directive.as_deref()?, &inner.expression))
    }

    fn serialize_directive(&mut self, statement: &ast::statement::Statement<Loc, Loc>) {
        let Some((loc, value, expression)) = self.directive_parts(statement) else {
            return;
        };
        let (value_loc, raw) = match &**expression {
            ast::expression::ExpressionInner::StringLiteral { loc, inner } => {
                (loc, inner.raw.as_str())
            }
            _ => (loc, ""),
        };
        self.write_node_header(NodeKind::BabelDirective, loc);
        self.write_node_header(NodeKind::BabelDirectiveLiteral, value_loc);
        self.write_str(value);
        self.write_str(value);
        self.write_str(raw);
    }

    fn babel_source_type(&self, program: &ast::Program<Loc, Loc>) -> bool {
        match self.babel_source_type {
            SourceType::Script => false,
            SourceType::Module => true,
            SourceType::Unspecified => crate::is_module(program),
        }
    }

    // ---------------------------------------------------------------
    // Dispatch methods — thin wrappers around generated dispatch
    // ---------------------------------------------------------------

    fn serialize_statement(&mut self, stmt: &ast::statement::Statement<Loc, Loc>) {
        self.serialize_statement_dispatch(stmt);
    }

    // ---------------------------------------------------------------
    // Statement helpers
    // ---------------------------------------------------------------

    fn serialize_identifier_node(&mut self, id: &ast::Identifier<Loc, Loc>) {
        if self.is_babel() && id.name.starts_with("\0flow_enum_runtime_") {
            self.write_node_header(NodeKind::BabelEnumRuntime, &id.loc);
            return;
        }
        // 76: Identifier — name(String), typeAnnotation(Node null), optional(Bool false)
        self.write_identifier_node_header(&id.loc, &id.name);
        self.write_str(&id.name);
        self.write_null_node(); // typeAnnotation
        self.write_bool(false); // optional
    }

    fn serialize_identifier_node_with_loc(&mut self, id: &ast::Identifier<Loc, Loc>, loc: &Loc) {
        self.write_identifier_node_header(loc, &id.name);
        self.write_str(&id.name);
        self.write_null_node();
        self.write_bool(false);
    }

    fn serialize_block_statement(&mut self, loc: &Loc, block: &ast::statement::Block<Loc, Loc>) {
        let directive_count = if self.is_babel() {
            block
                .body
                .iter()
                .take_while(|statement| self.directive_parts(statement).is_some())
                .count()
        } else {
            0
        };
        self.write_node_header(
            if self.is_babel() {
                NodeKind::BabelBlockStatement
            } else {
                NodeKind::BlockStatement
            },
            loc,
        );
        self.buf.push((block.body.len() - directive_count) as u32);
        for stmt in block.body.iter().skip(directive_count) {
            self.serialize_statement(stmt);
        }
        if self.is_babel() {
            self.buf.push(directive_count as u32);
            for statement in block.body.iter().take(directive_count) {
                self.serialize_directive(statement);
            }
        }
    }

    fn serialize_expression_statement(
        &mut self,
        loc: &Loc,
        statement: &ast::statement::Expression<Loc, Loc>,
    ) {
        self.write_node_header(
            if self.is_babel() {
                NodeKind::BabelExpressionStatement
            } else {
                NodeKind::ExpressionStatement
            },
            loc,
        );
        self.serialize_expression(&statement.expression);
        if !self.is_babel() {
            self.write_str_opt(statement.directive.as_deref());
        }
    }

    fn serialize_switch_case(&mut self, case: &ast::statement::switch::Case<Loc, Loc>) {
        // 95: SwitchCase — test(Node nullable), consequent(NodeList)
        self.write_node_header(NodeKind::SwitchCase, &case.loc);
        match &case.test {
            Some(expr) => self.serialize_expression(expr),
            None => self.write_null_node(),
        }
        self.buf.push(case.consequent.len() as u32);
        for stmt in case.consequent.iter() {
            self.serialize_statement(stmt);
        }
    }

    fn serialize_catch_clause(&mut self, catch: &ast::statement::try_::CatchClause<Loc, Loc>) {
        // 96: CatchClause — param(Node nullable), body(Node)
        self.write_node_header(NodeKind::CatchClause, &catch.loc);
        match &catch.param {
            Some(pat) => self.serialize_pattern(pat),
            None => self.write_null_node(),
        }
        self.serialize_block_statement(&catch.body.0, &catch.body.1);
    }

    fn serialize_variable_declaration(
        &mut self,
        loc: &Loc,
        decl: &ast::statement::VariableDeclaration<Loc, Loc>,
    ) {
        // 20: VariableDeclaration — kind(String), declarations(NodeList)
        self.write_node_header(NodeKind::VariableDeclaration, loc);
        self.write_str(decl.kind.as_str());
        self.buf.push(decl.declarations.len() as u32);
        for declarator in decl.declarations.iter() {
            self.serialize_variable_declarator(declarator);
        }
    }

    fn serialize_variable_declarator(
        &mut self,
        decl: &ast::statement::variable::Declarator<Loc, Loc>,
    ) {
        // 21: VariableDeclarator — init(Node nullable), id(Node)
        self.write_node_header(NodeKind::VariableDeclarator, &decl.loc);
        match &decl.init {
            Some(expr) => self.serialize_expression(expr),
            None => self.write_null_node(),
        }
        self.serialize_pattern(&decl.id);
    }

    fn serialize_variable_declarator_list(
        &mut self,
        declarations: &[ast::statement::variable::Declarator<Loc, Loc>],
    ) {
        self.buf.push(declarations.len() as u32);
        for declarator in declarations.iter() {
            self.serialize_variable_declarator(declarator);
        }
    }

    fn serialize_for_init(&mut self, init: &Option<ast::statement::for_::Init<Loc, Loc>>) {
        match init {
            Some(ast::statement::for_::Init::InitDeclaration((loc, decl))) => {
                self.serialize_variable_declaration(loc, decl);
            }
            Some(ast::statement::for_::Init::InitExpression(expr)) => {
                self.serialize_expression(expr);
            }
            None => self.write_null_node(),
        }
    }

    fn serialize_for_in_left(&mut self, left: &ast::statement::for_in::Left<Loc, Loc>) {
        match left {
            ast::statement::for_in::Left::LeftDeclaration((loc, decl)) => {
                self.serialize_variable_declaration(loc, decl);
            }
            ast::statement::for_in::Left::LeftPattern(pat) => {
                self.serialize_pattern(pat);
            }
        }
    }

    fn serialize_for_of_left(&mut self, left: &ast::statement::for_of::Left<Loc, Loc>) {
        match left {
            ast::statement::for_of::Left::LeftDeclaration((loc, decl)) => {
                self.serialize_variable_declaration(loc, decl);
            }
            ast::statement::for_of::Left::LeftPattern(pat) => {
                self.serialize_pattern(pat);
            }
        }
    }

    fn serialize_expression(&mut self, expr: &ast::expression::Expression<Loc, Loc>) {
        // Optional-chain rewrite: mirror upstream Hermes' post-rewrite ESTree
        // shape. OptionalMember/OptionalCall serialize as Member/Call with an
        // `optional` flag and a ChainExpression wrapper at the chain root. Only
        // the object/callee child of another optional chain is unwrapped; other
        // sub-expressions such as call arguments, computed keys, array elements,
        // and logical operands keep their own ChainExpression wrappers.
        use ast::expression::ExpressionInner;
        if !self.is_babel() && is_optional_chain(expr) {
            self.write_node_header(NodeKind::ChainExpression, expr.loc());
            self.serialize_optional_chain_child(expr, expr.loc());
            return;
        }
        match &**expr {
            ExpressionInner::OptionalMember { loc, inner }
                if !matches!(
                    inner.optional,
                    ast::expression::OptionalMemberKind::AssertNonnull { .. }
                ) =>
            {
                self.serialize_babel_optional_member(loc, inner);
            }
            ExpressionInner::OptionalCall { loc, inner }
                if !matches!(
                    inner.optional,
                    ast::expression::OptionalCallKind::AssertNonnull { .. }
                ) =>
            {
                self.serialize_babel_optional_call(loc, inner);
            }
            ExpressionInner::OptionalMember { loc, inner }
                if let ast::expression::OptionalMemberKind::AssertNonnull { assertion_loc } =
                    &inner.optional =>
            {
                self.write_node_header(
                    if self.is_babel() {
                        NodeKind::BabelMemberExpression
                    } else {
                        NodeKind::MemberExpression
                    },
                    loc,
                );
                self.write_node_header(NodeKind::NonNullExpression, assertion_loc);
                self.serialize_expression(&inner.member.object);
                self.serialize_member_property(&inner.member.property);
                self.write_bool(matches!(
                    &inner.member.property,
                    ast::expression::member::Property::PropertyExpression(_)
                ));
                if !self.is_babel() {
                    self.write_bool(false);
                }
            }
            ExpressionInner::OptionalCall { loc, inner }
                if let ast::expression::OptionalCallKind::AssertNonnull { assertion_loc } =
                    &inner.optional =>
            {
                self.write_node_header(
                    if self.is_babel() {
                        NodeKind::BabelCallExpression
                    } else {
                        NodeKind::CallExpression
                    },
                    loc,
                );
                self.write_node_header(NodeKind::NonNullExpression, assertion_loc);
                self.serialize_expression(&inner.call.callee);
                self.serialize_call_type_args_opt(&inner.call.targs);
                self.serialize_arg_list(&inner.call.arguments);
                self.write_bool(false);
            }
            _ => self.serialize_expression_dispatch(expr),
        }
    }

    fn serialize_optional_chain_child(
        &mut self,
        expr: &ast::expression::Expression<Loc, Loc>,
        parent_loc: &Loc,
    ) {
        use ast::expression::ExpressionInner;
        if expr.loc().start != parent_loc.start {
            self.serialize_expression(expr);
            return;
        }
        match &**expr {
            ExpressionInner::OptionalMember { loc, inner } => {
                self.serialize_optional_member_as_member_expression(loc, inner);
            }
            ExpressionInner::OptionalCall { loc, inner } => {
                self.serialize_optional_call_as_call_expression(loc, inner);
            }
            ExpressionInner::Unary { loc, inner }
                if inner.operator == ast::expression::UnaryOperator::Nonnull
                    && is_optional_chain(expr) =>
            {
                self.write_node_header(NodeKind::NonNullExpression, loc);
                self.serialize_optional_chain_child(&inner.argument, loc);
            }
            _ => self.serialize_expression(expr),
        }
    }

    /// Serialize an `OptionalMember` AST node as a wire-format
    /// `MemberExpression` with the `optional` flag set from the
    /// `OptionalMemberKind`. Used inside an optional chain (after the chain
    /// root has emitted its `ChainExpression` wrapper).
    fn serialize_optional_member_as_member_expression(
        &mut self,
        loc: &Loc,
        inner: &ast::expression::OptionalMember<Loc, Loc>,
    ) {
        // 55: MemberExpression — object(Node) property(Node) computed(Bool) optional(Bool)
        self.write_node_header(NodeKind::MemberExpression, loc);
        if let ast::expression::OptionalMemberKind::AssertNonnull { assertion_loc } =
            &inner.optional
        {
            self.write_node_header(NodeKind::NonNullExpression, assertion_loc);
            self.serialize_optional_chain_child(&inner.member.object, loc);
        } else {
            self.serialize_optional_chain_child(&inner.member.object, loc);
        }
        self.serialize_member_property(&inner.member.property);
        self.write_bool(matches!(
            &inner.member.property,
            ast::expression::member::Property::PropertyExpression(_)
        ));
        self.write_bool(matches!(
            inner.optional,
            ast::expression::OptionalMemberKind::Optional
        ));
    }

    fn serialize_babel_optional_member(
        &mut self,
        loc: &Loc,
        inner: &ast::expression::OptionalMember<Loc, Loc>,
    ) {
        self.write_node_header(NodeKind::OptionalMemberExpression, loc);
        self.serialize_expression(&inner.member.object);
        self.serialize_member_property(&inner.member.property);
        self.write_bool(matches!(
            &inner.member.property,
            ast::expression::member::Property::PropertyExpression(_)
        ));
        self.write_bool(matches!(
            inner.optional,
            ast::expression::OptionalMemberKind::Optional
        ));
    }

    fn serialize_babel_optional_call(
        &mut self,
        loc: &Loc,
        inner: &ast::expression::OptionalCall<Loc, Loc>,
    ) {
        self.write_node_header(NodeKind::BabelOptionalCallExpression, loc);
        self.serialize_expression(&inner.call.callee);
        self.serialize_call_type_args_opt(&inner.call.targs);
        self.serialize_arg_list(&inner.call.arguments);
        self.write_bool(matches!(
            inner.optional,
            ast::expression::OptionalCallKind::Optional
        ));
    }

    /// Serialize an `OptionalCall` AST node as a wire-format `CallExpression`
    /// with the `optional` flag set from the `OptionalCallKind`. Used inside
    /// an optional chain (after the chain root has emitted its
    /// `ChainExpression` wrapper).
    fn serialize_optional_call_as_call_expression(
        &mut self,
        loc: &Loc,
        inner: &ast::expression::OptionalCall<Loc, Loc>,
    ) {
        // 57: CallExpression — callee(Node) typeArguments(Node) arguments(NodeList) optional(Bool)
        self.write_node_header(
            if self.is_babel() {
                NodeKind::BabelCallExpression
            } else {
                NodeKind::CallExpression
            },
            loc,
        );
        if let ast::expression::OptionalCallKind::AssertNonnull { assertion_loc } = &inner.optional
        {
            self.write_node_header(NodeKind::NonNullExpression, assertion_loc);
            self.serialize_optional_chain_child(&inner.call.callee, loc);
        } else {
            self.serialize_optional_chain_child(&inner.call.callee, loc);
        }
        self.serialize_call_type_args_opt(&inner.call.targs);
        self.serialize_arg_list(&inner.call.arguments);
        self.write_bool(matches!(
            inner.optional,
            ast::expression::OptionalCallKind::Optional
        ));
    }

    /// Serialize a plain `Member` AST node as a wire-format
    /// `MemberExpression` with `optional: false`.
    fn serialize_member_expression(
        &mut self,
        loc: &Loc,
        inner: &ast::expression::Member<Loc, Loc>,
    ) {
        // 55: MemberExpression — object(Node) property(Node) computed(Bool) optional(Bool)
        self.write_node_header(
            if self.is_babel() {
                NodeKind::BabelMemberExpression
            } else {
                NodeKind::MemberExpression
            },
            loc,
        );
        self.serialize_expression(&inner.object);
        self.serialize_member_property(&inner.property);
        self.write_bool(matches!(
            &inner.property,
            ast::expression::member::Property::PropertyExpression(_)
        ));
        if !self.is_babel() {
            self.write_bool(false);
        }
    }

    /// Serialize a plain `Call` AST node as a wire-format `CallExpression`
    /// with `optional: false`.
    fn serialize_call_expression(&mut self, loc: &Loc, inner: &ast::expression::Call<Loc, Loc>) {
        // 57: CallExpression — callee(Node) typeArguments(Node) arguments(NodeList) optional(Bool)
        self.write_node_header(
            if self.is_babel() {
                NodeKind::BabelCallExpression
            } else {
                NodeKind::CallExpression
            },
            loc,
        );
        self.serialize_expression(&inner.callee);
        self.serialize_call_type_args_opt(&inner.targs);
        self.serialize_arg_list(&inner.arguments);
        self.write_bool(false);
    }

    fn serialize_type_cast_expression(
        &mut self,
        loc: &Loc,
        inner: &ast::expression::TypeCast<Loc, Loc>,
    ) {
        let babel_loc = if self.is_babel() {
            Loc {
                source: loc.source.clone(),
                start: Position {
                    line: loc.start.line,
                    column: loc.start.column + 1,
                },
                end: Position {
                    line: loc.end.line,
                    column: loc.end.column - 1,
                },
            }
        } else {
            loc.clone()
        };
        self.write_node_header(NodeKind::TypeCastExpression, &babel_loc);
        self.serialize_expression(&inner.expression);
        self.serialize_type_annotation(&inner.annot);
    }

    fn serialize_as_expression(
        &mut self,
        loc: &Loc,
        inner: &ast::expression::AsExpression<Loc, Loc>,
    ) {
        self.write_node_header(
            if self.is_babel() {
                NodeKind::TypeCastExpression
            } else {
                NodeKind::AsExpression
            },
            loc,
        );
        self.serialize_expression(&inner.expression);
        if self.is_babel() {
            self.serialize_type_annotation(&inner.annot);
        } else {
            self.serialize_type(&inner.annot.annotation);
        }
    }

    fn serialize_as_const_expression(
        &mut self,
        loc: &Loc,
        inner: &ast::expression::AsConstExpression<Loc, Loc>,
    ) {
        if self.is_babel() {
            self.serialize_expression(&inner.expression);
        } else {
            self.write_node_header(NodeKind::AsConstExpression, loc);
            self.serialize_expression(&inner.expression);
        }
    }

    fn serialize_import_expression(
        &mut self,
        loc: &Loc,
        inner: &ast::expression::Import<Loc, Loc>,
    ) {
        // A phase-modified call has no `Import` callee to hang the phase off,
        // so Babel represents it as an ImportExpression too — only a plain
        // `import(...)` lowers to `CallExpression(Import, ...)`.
        if self.is_babel() && inner.phase.is_none() {
            self.write_node_header(NodeKind::BabelCallExpression, loc);
            let import_loc = Loc {
                source: loc.source.clone(),
                start: loc.start,
                end: Position {
                    line: loc.start.line,
                    column: loc.start.column + 6,
                },
            };
            self.write_node_header(NodeKind::BabelImport, &import_loc);
            self.write_null_node();
            self.buf.push(if inner.options.is_some() { 2 } else { 1 });
            self.serialize_expression(&inner.argument);
            if let Some(options) = &inner.options {
                self.serialize_expression(options);
            }
            self.write_bool(false);
        } else {
            self.write_node_header(NodeKind::ImportExpression, loc);
            self.serialize_expression(&inner.argument);
            match &inner.options {
                Some(options) => self.serialize_expression(options),
                None => self.write_null_node(),
            }
            self.write_str_opt(inner.phase.map(|phase| phase.as_str()));
        }
    }

    // ---------------------------------------------------------------
    // Type serialization
    // ---------------------------------------------------------------

    fn write_type_use_node_header(&mut self, kind: NodeKind, loc: &Loc) {
        let kind = if self.is_babel() {
            match kind {
                NodeKind::GenericTypeAnnotation => NodeKind::BabelGenericTypeAnnotation,
                NodeKind::InterfaceExtends => NodeKind::BabelInterfaceExtends,
                NodeKind::ClassImplements => NodeKind::BabelClassImplements,
                _ => kind,
            }
        } else {
            kind
        };
        self.write_node_header(kind, loc);
    }

    fn serialize_generic_type(&mut self, loc: &Loc, generic: &ast::types::Generic<Loc, Loc>) {
        self.write_type_use_node_header(NodeKind::GenericTypeAnnotation, loc);
        self.serialize_generic_type_identifier(&generic.id);
        self.serialize_type_args_opt(&generic.targs);
    }

    fn serialize_type(&mut self, ty: &ast::types::Type<Loc, Loc>) {
        // Mirror upstream Hermes' mapGenericTypeAnnotation: collapse the
        // no-targs `this` identifier case to a ThisTypeAnnotation leaf node.
        // The parser produces a `Type::Generic` with an unqualified `this` id
        // and no targs for both `type T = this` and `(this) => void` /
        // `m(): this`.
        use ast::types::TypeInner;
        use ast::types::generic;
        if self.is_babel() {
            match &**ty {
                TypeInner::StringLiteral { loc, literal } => {
                    self.write_node_header(NodeKind::BabelStringLiteralTypeAnnotation, loc);
                    self.write_string_literal_value(&literal.value, &literal.raw);
                    self.write_str(&literal.raw);
                    return;
                }
                TypeInner::NumberLiteral { loc, literal } => {
                    self.write_node_header(NodeKind::BabelNumberLiteralTypeAnnotation, loc);
                    self.write_number(literal.value);
                    self.write_str(&literal.raw);
                    return;
                }
                TypeInner::BigIntLiteral { loc, literal } => {
                    self.write_node_header(NodeKind::BabelBigIntLiteralTypeAnnotation, loc);
                    self.write_str(&clean_bigint_raw(&literal.raw));
                    self.write_str(&literal.raw);
                    return;
                }
                TypeInner::BooleanLiteral { loc, literal } => {
                    self.write_node_header(NodeKind::BabelBooleanLiteralTypeAnnotation, loc);
                    self.write_bool(literal.value);
                    return;
                }
                TypeInner::Typeof { loc, inner } => {
                    self.serialize_babel_typeof(loc, inner);
                    return;
                }
                TypeInner::Unknown { loc, .. } => {
                    self.serialize_babel_keyword_generic(loc, "unknown");
                    return;
                }
                TypeInner::Never { loc, .. } => {
                    self.serialize_babel_keyword_generic(loc, "never");
                    return;
                }
                TypeInner::Undefined { loc, .. } => {
                    self.serialize_babel_keyword_generic(loc, "undefined");
                    return;
                }
                _ => {}
            }
        }
        if let TypeInner::Generic { loc, inner } = &**ty
            && inner.targs.is_none()
            && let generic::Identifier::Unqualified(id) = &inner.id
            && id.name == "this"
        {
            self.write_node_header(NodeKind::ThisTypeAnnotation, loc);
            return;
        }
        self.serialize_type_dispatch(ty);
    }

    fn serialize_babel_keyword_generic(&mut self, loc: &Loc, name: &str) {
        self.write_type_use_node_header(NodeKind::GenericTypeAnnotation, loc);
        self.write_identifier_node_header(loc, name);
        self.write_str(name);
        self.write_null_node();
        self.write_bool(false);
        self.write_null_node();
    }

    fn serialize_type_annotation(&mut self, annot: &ast::types::Annotation<Loc, Loc>) {
        // 111: TypeAnnotation — typeAnnotation(Node)
        self.write_node_header(NodeKind::TypeAnnotation, &annot.loc);
        self.serialize_type(&annot.annotation);
    }

    fn serialize_annotation_or_hint(&mut self, annot: &ast::types::AnnotationOrHint<Loc, Loc>) {
        match annot {
            ast::types::AnnotationOrHint::Available(annot) => {
                self.serialize_type_annotation(annot);
            }
            ast::types::AnnotationOrHint::Missing(_) => {
                self.write_null_node();
            }
        }
    }

    fn serialize_babel_pattern_optional(&mut self, loc: &Loc, optional: bool) {
        let present = optional || self.babel_flags(loc) & COMPONENT_PATTERN_OPTIONAL_FLAG != 0;
        self.write_bool(present);
        if present {
            self.write_bool(optional);
        }
    }

    fn serialize_tagged_template_type_arguments(
        &mut self,
        targs: &Option<ast::expression::CallTypeArgs<Loc, Loc>>,
    ) {
        self.write_bool(!self.is_babel());
        if !self.is_babel() {
            self.serialize_call_type_args_opt(targs);
        }
    }

    fn serialize_type_params_opt(&mut self, tparams: &Option<ast::types::TypeParams<Loc, Loc>>) {
        match tparams {
            Some(tp) => {
                // 160: TypeParameterDeclaration — params(NodeList)
                self.write_node_header(NodeKind::TypeParameterDeclaration, &tp.loc);
                self.buf.push(tp.params.len() as u32);
                for param in tp.params.iter() {
                    self.serialize_type_parameter(param);
                }
            }
            None => self.write_null_node(),
        }
    }

    fn serialize_type_parameter(&mut self, tp: &ast::types::TypeParam<Loc, Loc>) {
        // 161: TypeParameter — name const bound variance default usesExtendsBound
        self.write_node_header(NodeKind::TypeParameter, &tp.loc);
        self.write_str(&tp.name.name);
        self.write_bool(tp.const_.is_some());
        match &tp.bound {
            ast::types::AnnotationOrHint::Available(annot) => {
                if self.is_babel() {
                    self.serialize_type_annotation(annot);
                } else {
                    self.serialize_type(&annot.annotation);
                }
            }
            ast::types::AnnotationOrHint::Missing(_) => self.write_null_node(),
        }
        match &tp.variance {
            Some(v) => self.serialize_variance(v),
            None => self.write_null_node(),
        }
        match &tp.default {
            Some(ty) => self.serialize_type(ty),
            None => self.write_null_node(),
        }
        self.write_bool(matches!(
            tp.bound_kind,
            ast::types::type_param::BoundKind::Extends
        ));
    }

    fn serialize_type_args_opt(&mut self, targs: &Option<ast::types::TypeArgs<Loc, Loc>>) {
        match targs {
            Some(ta) => {
                // 162: TypeParameterInstantiation — params(NodeList)
                self.write_node_header(NodeKind::TypeParameterInstantiation, &ta.loc);
                self.buf.push(ta.arguments.len() as u32);
                for arg in ta.arguments.iter() {
                    self.serialize_type(arg);
                }
            }
            None => self.write_null_node(),
        }
    }

    fn serialize_variance(&mut self, v: &ast::Variance<Loc>) {
        // 112: Variance — kind(String)
        self.write_node_header(NodeKind::Variance, &v.loc);
        self.write_str(match v.kind {
            ast::VarianceKind::Plus => "plus",
            ast::VarianceKind::Minus => "minus",
            ast::VarianceKind::Readonly => "readonly",
            ast::VarianceKind::Writeonly => "writeonly",
            ast::VarianceKind::In => "in",
            ast::VarianceKind::Out => "out",
            ast::VarianceKind::InOut => "in out",
        });
    }

    fn serialize_predicate(&mut self, pred: &ast::types::Predicate<Loc, Loc>) {
        match &pred.kind {
            ast::types::PredicateKind::Declared(expr) => {
                // 165: DeclaredPredicate — value(Node)
                self.write_node_header(NodeKind::DeclaredPredicate, &pred.loc);
                self.serialize_expression(expr);
            }
            ast::types::PredicateKind::Inferred => {
                // 166: InferredPredicate
                self.write_node_header(NodeKind::InferredPredicate, &pred.loc);
            }
        }
    }

    // ---------------------------------------------------------------
    // Pattern serialization
    // ---------------------------------------------------------------

    fn serialize_pattern(&mut self, pat: &ast::pattern::Pattern<Loc, Loc>) {
        match pat {
            ast::pattern::Pattern::Object { loc, inner } => {
                // 78: ObjectPattern — properties(NodeList) typeAnnotation(Node) optional(TrueBoolean)
                self.write_node_header(
                    if self.is_babel() {
                        NodeKind::BabelObjectPattern
                    } else {
                        NodeKind::ObjectPattern
                    },
                    loc,
                );
                self.buf.push(inner.properties.len() as u32);
                for prop in inner.properties.iter() {
                    self.serialize_pattern_object_property(prop);
                }
                self.serialize_annotation_or_hint(&inner.annot);
                if self.is_babel() {
                    self.serialize_babel_pattern_optional(loc, inner.optional);
                } else {
                    self.write_bool(inner.optional);
                }
            }
            ast::pattern::Pattern::Array { loc, inner } => {
                // 79: ArrayPattern — elements(NodeList) typeAnnotation(Node) optional(TrueBoolean)
                self.write_node_header(
                    if self.is_babel() {
                        NodeKind::BabelArrayPattern
                    } else {
                        NodeKind::ArrayPattern
                    },
                    loc,
                );
                self.buf.push(inner.elements.len() as u32);
                for elem in inner.elements.iter() {
                    self.serialize_pattern_array_element(elem);
                }
                self.serialize_annotation_or_hint(&inner.annot);
                if self.is_babel() {
                    self.serialize_babel_pattern_optional(loc, inner.optional);
                } else {
                    self.write_bool(inner.optional);
                }
            }
            ast::pattern::Pattern::Identifier { loc, inner } => {
                // 76: Identifier — name(String) typeAnnotation(Node) optional(Bool)
                // Use the outer pattern loc (covers name + typeAnnotation),
                // not the inner name loc.
                self.write_identifier_node_header(loc, &inner.name.name);
                self.write_str(&inner.name.name);
                self.serialize_annotation_or_hint(&inner.annot);
                self.write_bool(inner.optional);
            }
            ast::pattern::Pattern::Expression { inner, .. } => {
                self.serialize_expression(inner);
            }
        }
    }

    fn serialize_pattern_object_property(
        &mut self,
        prop: &ast::pattern::object::Property<Loc, Loc>,
    ) {
        match prop {
            ast::pattern::object::Property::NormalProperty(np) => {
                // 82: Property — key value kind method shorthand computed
                self.write_node_header(
                    if self.is_babel() {
                        NodeKind::BabelObjectProperty
                    } else {
                        NodeKind::Property
                    },
                    &np.loc,
                );
                if self.is_babel() {
                    self.write_bool(is_pattern_key_computed(&np.key));
                }
                self.serialize_pattern_object_key(&np.key);
                match &np.default {
                    Some(def) => {
                        // 81: AssignmentPattern — left(Node) right(Node)
                        // The AssignmentPattern spans from the inner pattern's
                        // start to the default expression's end, not the outer
                        // property loc.
                        let ap_loc = Loc::between(np.pattern.loc(), def.loc());
                        self.write_node_header(NodeKind::AssignmentPattern, &ap_loc);
                        self.serialize_pattern(&np.pattern);
                        self.serialize_expression(def);
                    }
                    None => {
                        self.serialize_pattern(&np.pattern);
                    }
                }
                if !self.is_babel() {
                    self.write_str("init");
                }
                self.write_bool(false); // method
                self.write_bool(np.shorthand);
                if !self.is_babel() {
                    self.write_bool(is_pattern_key_computed(&np.key));
                }
            }
            ast::pattern::object::Property::RestElement(rest) => {
                self.serialize_rest_element(&rest.loc, &rest.argument);
            }
        }
    }

    fn serialize_pattern_object_key(&mut self, key: &ast::pattern::object::Key<Loc, Loc>) {
        match key {
            ast::pattern::object::Key::StringLiteral((loc, lit)) => {
                self.write_string_literal(loc, &lit.value, &lit.raw);
            }
            ast::pattern::object::Key::NumberLiteral((loc, lit)) => {
                self.write_number_literal(loc, lit.value, &lit.raw);
            }
            ast::pattern::object::Key::BigIntLiteral((loc, lit)) => {
                self.write_bigint_literal(loc, &lit.raw);
            }
            ast::pattern::object::Key::Identifier(id) => {
                self.serialize_identifier_node(id);
            }
            ast::pattern::object::Key::Computed(ck) => {
                self.serialize_expression(&ck.expression);
            }
        }
    }

    fn serialize_pattern_array_element(&mut self, elem: &ast::pattern::array::Element<Loc, Loc>) {
        match elem {
            ast::pattern::array::Element::NormalElement(ne) => match &ne.default {
                Some(def) => {
                    // 81: AssignmentPattern — left(Node) right(Node)
                    self.write_node_header(NodeKind::AssignmentPattern, &ne.loc);
                    self.serialize_pattern(&ne.argument);
                    self.serialize_expression(def);
                }
                None => {
                    self.serialize_pattern(&ne.argument);
                }
            },
            ast::pattern::array::Element::RestElement(rest) => {
                self.serialize_rest_element(&rest.loc, &rest.argument);
            }
            ast::pattern::array::Element::Hole(_) => {
                self.write_null_node();
            }
        }
    }

    fn serialize_rest_element(&mut self, loc: &Loc, argument: &ast::pattern::Pattern<Loc, Loc>) {
        if !self.is_babel() {
            self.write_node_header(NodeKind::RestElement, loc);
            self.serialize_pattern(argument);
            return;
        }
        self.write_node_header(NodeKind::BabelRestElement, loc);
        match argument {
            ast::pattern::Pattern::Identifier { loc, inner } => {
                let argument_loc =
                    if matches!(inner.annot, ast::types::AnnotationOrHint::Available(_)) {
                        Loc {
                            source: loc.source.clone(),
                            start: loc.start,
                            end: Position {
                                line: loc.start.line,
                                column: loc.start.column + inner.name.name.len() as i32,
                            },
                        }
                    } else {
                        loc.clone()
                    };
                self.write_identifier_node_header(&argument_loc, &inner.name.name);
                self.write_str(&inner.name.name);
                self.write_null_node();
                self.write_bool(inner.optional);
            }
            ast::pattern::Pattern::Object { loc, inner } => {
                self.write_node_header(NodeKind::BabelObjectPattern, loc);
                self.buf.push(inner.properties.len() as u32);
                for property in inner.properties.iter() {
                    self.serialize_pattern_object_property(property);
                }
                self.write_null_node();
                self.serialize_babel_pattern_optional(loc, inner.optional);
            }
            ast::pattern::Pattern::Array { loc, inner } => {
                self.write_node_header(NodeKind::BabelArrayPattern, loc);
                self.buf.push(inner.elements.len() as u32);
                for element in inner.elements.iter() {
                    self.serialize_pattern_array_element(element);
                }
                self.write_null_node();
                self.serialize_babel_pattern_optional(loc, inner.optional);
            }
            ast::pattern::Pattern::Expression { inner, .. } => {
                self.serialize_expression(inner);
            }
        }
        let annotation = match argument {
            ast::pattern::Pattern::Identifier { inner, .. } => Some(&inner.annot),
            ast::pattern::Pattern::Object { inner, .. } => Some(&inner.annot),
            ast::pattern::Pattern::Array { inner, .. } => Some(&inner.annot),
            ast::pattern::Pattern::Expression { .. } => None,
        };
        let has_annotation = annotation.is_some_and(|annotation| {
            matches!(annotation, ast::types::AnnotationOrHint::Available(_))
        });
        let present =
            has_annotation || self.babel_flags(loc) & COMPONENT_REST_TYPE_ANNOTATION_FLAG != 0;
        self.write_bool(present);
        if present {
            match annotation {
                Some(annotation) => self.serialize_annotation_or_hint(annotation),
                None => self.write_null_node(),
            }
        }
    }

    // ---------------------------------------------------------------
    // Match pattern serialization
    // ---------------------------------------------------------------

    fn serialize_match_pattern(&mut self, pat: &ast::match_pattern::MatchPattern<Loc, Loc>) {
        use ast::match_pattern::MatchPattern;
        match pat {
            MatchPattern::WildcardPattern { loc, .. } => {
                // 198: MatchWildcardPattern
                self.write_node_header(NodeKind::MatchWildcardPattern, loc);
            }
            MatchPattern::NumberPattern { loc, inner } => {
                // 199: MatchLiteralPattern — literal(Node)
                self.write_node_header(NodeKind::MatchLiteralPattern, loc);
                self.write_number_literal(loc, inner.value, &inner.raw);
            }
            MatchPattern::BigIntPattern { loc, inner } => {
                // 199: MatchLiteralPattern — literal(Node)
                self.write_node_header(NodeKind::MatchLiteralPattern, loc);
                self.write_bigint_literal(loc, &inner.raw);
            }
            MatchPattern::StringPattern { loc, inner } => {
                // 199: MatchLiteralPattern — literal(Node)
                self.write_node_header(NodeKind::MatchLiteralPattern, loc);
                self.write_string_literal(loc, &inner.value, &inner.raw);
            }
            MatchPattern::BooleanPattern { loc, inner } => {
                // 199: MatchLiteralPattern — literal(Node)
                self.write_node_header(NodeKind::MatchLiteralPattern, loc);
                self.write_boolean_literal(loc, inner.value);
            }
            MatchPattern::NullPattern { loc, .. } => {
                // 199: MatchLiteralPattern — literal(Node)
                self.write_node_header(NodeKind::MatchLiteralPattern, loc);
                self.write_null_literal(loc);
            }
            MatchPattern::UnaryPattern { loc, inner } => {
                // 200: MatchUnaryPattern — operator(String) argument(Node)
                self.write_node_header(NodeKind::MatchUnaryPattern, loc);
                let op_str = match inner.operator {
                    ast::match_pattern::unary_pattern::Operator::Plus => "+",
                    ast::match_pattern::unary_pattern::Operator::Minus => "-",
                };
                self.write_str(op_str);
                // argument is a number or bigint literal
                let (arg_loc, arg) = &inner.argument;
                match arg {
                    ast::match_pattern::unary_pattern::Argument::NumberLiteral(num) => {
                        self.write_number_literal(arg_loc, num.value, &num.raw);
                    }
                    ast::match_pattern::unary_pattern::Argument::BigIntLiteral(big) => {
                        self.write_bigint_literal(arg_loc, &big.raw);
                    }
                }
            }
            MatchPattern::ObjectPattern { loc, inner } => {
                // 201: MatchObjectPattern — properties(NodeList) rest(Node)
                self.write_node_header(NodeKind::MatchObjectPattern, loc);
                self.buf.push(inner.properties.len() as u32);
                for prop in inner.properties.iter() {
                    match prop {
                        ast::match_pattern::object_pattern::Property::Valid { loc, property } => {
                            // 210: MatchObjectPatternProperty — key pattern shorthand
                            self.write_node_header(NodeKind::MatchObjectPatternProperty, loc);
                            let shorthand_key_loc = match (&property.key, &property.pattern) {
                                (
                                    ast::match_pattern::object_pattern::Key::Identifier(_),
                                    MatchPattern::BindingPattern { loc, .. },
                                ) if property.shorthand => {
                                    Some(self.source_locations.next_token_loc_after(loc))
                                }
                                _ => None,
                            };
                            self.serialize_match_object_key(
                                &property.key,
                                shorthand_key_loc.as_ref(),
                            );
                            self.serialize_match_pattern(&property.pattern);
                            self.write_bool(property.shorthand);
                        }
                        ast::match_pattern::object_pattern::Property::InvalidShorthand {
                            loc,
                            identifier,
                        } => {
                            // Emit (key=identifier, pattern=MatchIdentifierPattern,
                            // shorthand=true) so the deserializer can reconstruct
                            // the invalid shorthand for diagnostic purposes.
                            self.write_node_header(NodeKind::MatchObjectPatternProperty, loc);
                            self.serialize_identifier_node(identifier);
                            // 206: MatchIdentifierPattern — id(Node)
                            self.write_node_header(
                                NodeKind::MatchIdentifierPattern,
                                &identifier.loc,
                            );
                            self.serialize_identifier_node(identifier);
                            self.write_bool(true);
                        }
                    }
                }
                match &inner.rest {
                    Some(rest) => {
                        // 211: MatchRestPattern — argument(Node)
                        self.write_node_header(NodeKind::MatchRestPattern, &rest.loc);
                        match &rest.argument {
                            Some((bind_loc, bind)) => {
                                // 208: MatchBindingPattern — id kind
                                self.write_node_header(NodeKind::MatchBindingPattern, bind_loc);
                                let id_loc = self.source_locations.next_token_loc_after(bind_loc);
                                self.serialize_identifier_node_with_loc(&bind.id, &id_loc);
                                self.write_str(bind.kind.as_str());
                            }
                            None => self.write_null_node(),
                        }
                    }
                    None => self.write_null_node(),
                }
            }
            MatchPattern::ArrayPattern { loc, inner } => {
                // 209: MatchArrayPattern — elements(NodeList) rest(Node)
                self.write_node_header(NodeKind::MatchArrayPattern, loc);
                self.buf.push(inner.elements.len() as u32);
                for elem in inner.elements.iter() {
                    self.serialize_match_pattern(&elem.pattern);
                }
                match &inner.rest {
                    Some(rest) => {
                        self.write_node_header(NodeKind::MatchRestPattern, &rest.loc);
                        match &rest.argument {
                            Some((bind_loc, bind)) => {
                                self.write_node_header(NodeKind::MatchBindingPattern, bind_loc);
                                let id_loc = self.source_locations.next_token_loc_after(bind_loc);
                                self.serialize_identifier_node_with_loc(&bind.id, &id_loc);
                                self.write_str(bind.kind.as_str());
                            }
                            None => self.write_null_node(),
                        }
                    }
                    None => self.write_null_node(),
                }
            }
            MatchPattern::InstancePattern { loc, inner } => {
                // 202: MatchInstancePattern — targetConstructor(Node) properties(Node)
                self.write_node_header(NodeKind::MatchInstancePattern, loc);
                match &inner.constructor {
                    ast::match_pattern::InstancePatternConstructor::IdentifierConstructor(id) => {
                        // 206: MatchIdentifierPattern — id(Node)
                        self.write_node_header(NodeKind::MatchIdentifierPattern, &id.loc);
                        self.serialize_identifier_node(id);
                    }
                    ast::match_pattern::InstancePatternConstructor::MemberConstructor(mp) => {
                        self.serialize_match_member_pattern(mp);
                    }
                }
                // 203: MatchInstanceObjectPattern — properties(NodeList) rest(Node)
                let (obj_loc, obj) = &inner.properties;
                self.write_node_header(NodeKind::MatchInstanceObjectPattern, obj_loc);
                self.buf.push(obj.properties.len() as u32);
                for prop in obj.properties.iter() {
                    match prop {
                        ast::match_pattern::object_pattern::Property::Valid { loc, property } => {
                            self.write_node_header(NodeKind::MatchObjectPatternProperty, loc);
                            let shorthand_key_loc = match (&property.key, &property.pattern) {
                                (
                                    ast::match_pattern::object_pattern::Key::Identifier(_),
                                    MatchPattern::BindingPattern { loc, .. },
                                ) if property.shorthand => {
                                    Some(self.source_locations.next_token_loc_after(loc))
                                }
                                _ => None,
                            };
                            self.serialize_match_object_key(
                                &property.key,
                                shorthand_key_loc.as_ref(),
                            );
                            self.serialize_match_pattern(&property.pattern);
                            self.write_bool(property.shorthand);
                        }
                        ast::match_pattern::object_pattern::Property::InvalidShorthand {
                            loc,
                            identifier,
                        } => {
                            // Same shape as the ObjectPattern InvalidShorthand
                            // arm above.
                            self.write_node_header(NodeKind::MatchObjectPatternProperty, loc);
                            self.serialize_identifier_node(identifier);
                            // 206: MatchIdentifierPattern — id(Node)
                            self.write_node_header(
                                NodeKind::MatchIdentifierPattern,
                                &identifier.loc,
                            );
                            self.serialize_identifier_node(identifier);
                            self.write_bool(true);
                        }
                    }
                }
                match &obj.rest {
                    Some(rest) => {
                        self.write_node_header(NodeKind::MatchRestPattern, &rest.loc);
                        match &rest.argument {
                            Some((bind_loc, bind)) => {
                                self.write_node_header(NodeKind::MatchBindingPattern, bind_loc);
                                let id_loc = self.source_locations.next_token_loc_after(bind_loc);
                                self.serialize_identifier_node_with_loc(&bind.id, &id_loc);
                                self.write_str(bind.kind.as_str());
                            }
                            None => self.write_null_node(),
                        }
                    }
                    None => self.write_null_node(),
                }
            }
            MatchPattern::OrPattern { loc, inner } => {
                // 204: MatchOrPattern — patterns(NodeList)
                self.write_node_header(NodeKind::MatchOrPattern, loc);
                self.buf.push(inner.patterns.len() as u32);
                for p in inner.patterns.iter() {
                    self.serialize_match_pattern(p);
                }
            }
            MatchPattern::AsPattern { loc, inner } => {
                // 205: MatchAsPattern — pattern(Node) target(Node)
                self.write_node_header(NodeKind::MatchAsPattern, loc);
                self.serialize_match_pattern(&inner.pattern);
                match &inner.target {
                    ast::match_pattern::as_pattern::Target::Identifier(id) => {
                        let id_loc = self.source_locations.next_token_loc_after(&id.loc);
                        self.serialize_identifier_node_with_loc(id, &id_loc);
                    }
                    ast::match_pattern::as_pattern::Target::Binding { loc, pattern } => {
                        // 208: MatchBindingPattern — id kind
                        self.write_node_header(NodeKind::MatchBindingPattern, loc);
                        let id_loc = self.source_locations.next_token_loc_after(loc);
                        self.serialize_identifier_node_with_loc(&pattern.id, &id_loc);
                        self.write_str(pattern.kind.as_str());
                    }
                }
            }
            MatchPattern::IdentifierPattern { loc, inner } => {
                // 206: MatchIdentifierPattern — id(Node)
                self.write_node_header(NodeKind::MatchIdentifierPattern, loc);
                self.serialize_identifier_node(inner);
            }
            MatchPattern::MemberPattern { loc: _, inner } => {
                self.serialize_match_member_pattern(inner);
            }
            MatchPattern::BindingPattern { loc, inner } => {
                // 208: MatchBindingPattern — id(Node) kind(String)
                self.write_node_header(NodeKind::MatchBindingPattern, loc);
                let id_loc = self.source_locations.next_token_loc_after(loc);
                self.serialize_identifier_node_with_loc(&inner.id, &id_loc);
                self.write_str(inner.kind.as_str());
            }
        }
    }

    fn serialize_match_member_pattern(&mut self, mp: &ast::match_pattern::MemberPattern<Loc, Loc>) {
        // 207: MatchMemberPattern — base(Node) property(Node)
        self.write_node_header(NodeKind::MatchMemberPattern, &mp.loc);
        match &mp.base {
            ast::match_pattern::member_pattern::Base::BaseIdentifier(id) => {
                // 206: MatchIdentifierPattern — id(Node)
                self.write_node_header(NodeKind::MatchIdentifierPattern, &id.loc);
                self.serialize_identifier_node(id);
            }
            ast::match_pattern::member_pattern::Base::BaseMember(nested) => {
                self.serialize_match_member_pattern(nested);
            }
        }
        match &mp.property {
            ast::match_pattern::member_pattern::Property::PropertyIdentifier(id) => {
                self.serialize_identifier_node(id);
            }
            ast::match_pattern::member_pattern::Property::PropertyString { loc, literal } => {
                self.write_string_literal(loc, &literal.value, &literal.raw);
            }
            ast::match_pattern::member_pattern::Property::PropertyNumber { loc, literal } => {
                self.write_number_literal(loc, literal.value, &literal.raw);
            }
            ast::match_pattern::member_pattern::Property::PropertyBigInt { loc, literal } => {
                self.write_bigint_literal(loc, &literal.raw);
            }
        }
    }

    fn serialize_match_object_key(
        &mut self,
        key: &ast::match_pattern::object_pattern::Key<Loc, Loc>,
        identifier_loc: Option<&Loc>,
    ) {
        match key {
            ast::match_pattern::object_pattern::Key::Identifier(id) => match identifier_loc {
                Some(loc) => self.serialize_identifier_node_with_loc(id, loc),
                None => self.serialize_identifier_node(id),
            },
            ast::match_pattern::object_pattern::Key::StringLiteral((loc, lit)) => {
                self.write_string_literal(loc, &lit.value, &lit.raw);
            }
            ast::match_pattern::object_pattern::Key::NumberLiteral((loc, lit)) => {
                self.write_number_literal(loc, lit.value, &lit.raw);
            }
            ast::match_pattern::object_pattern::Key::BigIntLiteral((loc, lit)) => {
                self.write_bigint_literal(loc, &lit.raw);
            }
        }
    }

    fn serialize_match_expression_case(
        &mut self,
        case: &ast::match_::Case<Loc, Loc, ast::expression::Expression<Loc, Loc>>,
    ) {
        // 196: MatchExpressionCase — pattern(Node) body(Node) guard(Node)
        self.write_node_header(NodeKind::MatchExpressionCase, &case.loc);
        self.serialize_match_pattern(&case.pattern);
        self.serialize_expression(&case.body);
        match &case.guard {
            Some(expr) => self.serialize_expression(expr),
            None => self.write_null_node(),
        }
    }

    fn serialize_match_statement_case(
        &mut self,
        case: &ast::match_::Case<Loc, Loc, ast::statement::Statement<Loc, Loc>>,
    ) {
        // 197: MatchStatementCase — pattern(Node) body(Node) guard(Node)
        self.write_node_header(NodeKind::MatchStatementCase, &case.loc);
        self.serialize_match_pattern(&case.pattern);
        self.serialize_statement(&case.body);
        match &case.guard {
            Some(expr) => self.serialize_expression(expr),
            None => self.write_null_node(),
        }
    }

    // ---------------------------------------------------------------
    // Expression helpers
    // ---------------------------------------------------------------

    fn serialize_array_element(&mut self, elem: &ast::expression::ArrayElement<Loc, Loc>) {
        match elem {
            ast::expression::ArrayElement::Expression(expr) => {
                self.serialize_expression(expr);
            }
            ast::expression::ArrayElement::Spread(spread) => {
                self.serialize_spread_element(spread);
            }
            ast::expression::ArrayElement::Hole(_) => {
                self.write_null_node();
            }
        }
    }

    fn serialize_spread_element(&mut self, spread: &ast::expression::SpreadElement<Loc, Loc>) {
        // 83: SpreadElement — argument(Node)
        self.write_node_header(NodeKind::SpreadElement, &spread.loc);
        self.serialize_expression(&spread.argument);
    }

    fn serialize_object_property(&mut self, prop: &ast::expression::object::Property<Loc, Loc>) {
        match prop {
            ast::expression::object::Property::NormalProperty(normal) => {
                if self.is_babel() {
                    match normal {
                        ast::expression::object::NormalProperty::Init {
                            key,
                            value,
                            shorthand,
                            ..
                        } => {
                            self.write_node_header(NodeKind::BabelObjectProperty, normal.loc());
                            self.write_bool(is_key_computed(key));
                            self.serialize_object_key(key);
                            self.serialize_expression(value);
                            self.write_bool(false);
                            self.write_bool(*shorthand);
                        }
                        ast::expression::object::NormalProperty::Method { key, value, .. } => {
                            self.serialize_babel_object_method(
                                normal.loc(),
                                "method",
                                true,
                                key,
                                &value.1,
                            );
                        }
                        ast::expression::object::NormalProperty::Get { key, value, .. } => {
                            self.serialize_babel_object_method(
                                normal.loc(),
                                "get",
                                false,
                                key,
                                &value.1,
                            );
                        }
                        ast::expression::object::NormalProperty::Set { key, value, .. } => {
                            self.serialize_babel_object_method(
                                normal.loc(),
                                "set",
                                false,
                                key,
                                &value.1,
                            );
                        }
                    }
                    return;
                }
                // 82: Property — key value kind method shorthand computed
                self.write_node_header(NodeKind::Property, normal.loc());
                match normal {
                    ast::expression::object::NormalProperty::Init {
                        key,
                        value,
                        shorthand,
                        ..
                    } => {
                        self.serialize_object_key(key);
                        self.serialize_expression(value);
                        self.write_str("init");
                        self.write_bool(false);
                        self.write_bool(*shorthand);
                        self.write_bool(is_key_computed(key));
                    }
                    ast::expression::object::NormalProperty::Method { key, value, .. } => {
                        self.serialize_object_key(key);
                        self.serialize_function_expr(
                            &value.0,
                            &value.1,
                            NodeKind::FunctionExpression,
                        );
                        self.write_str("init");
                        self.write_bool(true);
                        self.write_bool(false);
                        self.write_bool(is_key_computed(key));
                    }
                    ast::expression::object::NormalProperty::Get { key, value, .. } => {
                        self.serialize_object_key(key);
                        self.serialize_function_expr(
                            &value.0,
                            &value.1,
                            NodeKind::FunctionExpression,
                        );
                        self.write_str("get");
                        self.write_bool(false);
                        self.write_bool(false);
                        self.write_bool(is_key_computed(key));
                    }
                    ast::expression::object::NormalProperty::Set { key, value, .. } => {
                        self.serialize_object_key(key);
                        self.serialize_function_expr(
                            &value.0,
                            &value.1,
                            NodeKind::FunctionExpression,
                        );
                        self.write_str("set");
                        self.write_bool(false);
                        self.write_bool(false);
                        self.write_bool(is_key_computed(key));
                    }
                }
            }
            ast::expression::object::Property::SpreadProperty(spread) => {
                // Serialize as SpreadElement (83)
                self.write_node_header(NodeKind::SpreadElement, &spread.loc);
                self.serialize_expression(&spread.argument);
            }
        }
    }

    fn serialize_babel_object_method(
        &mut self,
        loc: &Loc,
        kind: &str,
        method: bool,
        key: &ast::expression::object::Key<Loc, Loc>,
        function: &ast::function::Function<Loc, Loc>,
    ) {
        self.write_node_header(NodeKind::BabelObjectMethod, loc);
        self.write_str(kind);
        self.write_bool(method);
        self.write_bool(is_key_computed(key));
        self.serialize_object_key(key);
        self.write_null_node();
        self.serialize_function_params(&function.params);
        self.serialize_function_body(&function.body);
        self.write_bool(function.async_);
        self.write_bool(function.generator);
        self.serialize_return_type(&function.return_);
        self.serialize_type_params_opt(&function.tparams);
        let has_variance = kind != "method";
        self.write_bool(has_variance);
        if has_variance {
            self.write_null_node();
        }
    }

    fn serialize_object_key(&mut self, key: &ast::expression::object::Key<Loc, Loc>) {
        match key {
            ast::expression::object::Key::StringLiteral((loc, lit)) => {
                self.write_string_literal(loc, &lit.value, &lit.raw);
            }
            ast::expression::object::Key::NumberLiteral((loc, lit)) => {
                self.write_number_literal(loc, lit.value, &lit.raw);
            }
            ast::expression::object::Key::BigIntLiteral((loc, lit)) => {
                self.write_bigint_literal(loc, &lit.raw);
            }
            ast::expression::object::Key::Identifier(id) => {
                self.serialize_identifier_node(id);
            }
            ast::expression::object::Key::PrivateName(pn) => {
                self.serialize_private_name(pn);
            }
            ast::expression::object::Key::Computed(ck) => {
                self.serialize_expression(&ck.expression);
            }
        }
    }

    fn serialize_private_name(&mut self, pn: &ast::PrivateName<Loc>) {
        if self.is_babel() {
            self.write_node_header(NodeKind::BabelPrivateName, &pn.loc);
            let id_loc = Loc {
                source: pn.loc.source.clone(),
                start: Position {
                    line: pn.loc.start.line,
                    column: pn.loc.start.column + 1,
                },
                end: pn.loc.end,
            };
            self.write_identifier_node_header(&id_loc, &pn.name);
            self.write_str(&pn.name);
            self.write_null_node();
            self.write_bool(false);
            return;
        }
        // 77: PrivateIdentifier — name(String) typeAnnotation(Node) optional(Bool)
        self.write_node_header(NodeKind::PrivateIdentifier, &pn.loc);
        self.write_str(&pn.name);
        self.write_null_node(); // typeAnnotation
        self.write_bool(false); // optional
    }

    fn serialize_member_property(&mut self, prop: &ast::expression::member::Property<Loc, Loc>) {
        match prop {
            ast::expression::member::Property::PropertyIdentifier(id) => {
                self.serialize_identifier_node(id);
            }
            ast::expression::member::Property::PropertyPrivateName(pn) => {
                self.serialize_private_name(pn);
            }
            ast::expression::member::Property::PropertyExpression(expr) => {
                self.serialize_expression(expr);
            }
        }
    }

    fn serialize_call_type_args_opt(
        &mut self,
        targs: &Option<ast::expression::CallTypeArgs<Loc, Loc>>,
    ) {
        match targs {
            Some(targs) => {
                // 162: TypeParameterInstantiation — params(NodeList)
                self.write_node_header(NodeKind::TypeParameterInstantiation, &targs.loc);
                self.buf.push(targs.arguments.len() as u32);
                for arg in targs.arguments.iter() {
                    match arg {
                        ast::expression::CallTypeArg::Explicit(ty) => {
                            self.serialize_type(ty);
                        }
                        ast::expression::CallTypeArg::Implicit(imp) => {
                            // Synthesize a GenericTypeAnnotation with
                            // id = Identifier "_" for implicit type args (`_`).
                            // 139: GenericTypeAnnotation — id(Node) typeArguments(Node)
                            self.write_type_use_node_header(
                                NodeKind::GenericTypeAnnotation,
                                &imp.loc,
                            );
                            // 76: Identifier — name typeAnnotation optional
                            self.write_identifier_node_header(&imp.loc, "_");
                            self.write_str("_");
                            self.write_null_node();
                            self.write_bool(false);
                            self.write_null_node();
                        }
                    }
                }
            }
            None => self.write_null_node(),
        }
    }

    fn serialize_arg_list(&mut self, args: &ast::expression::ArgList<Loc, Loc>) {
        self.buf.push(args.arguments.len() as u32);
        for arg in args.arguments.iter() {
            match arg {
                ast::expression::ExpressionOrSpread::Expression(expr) => {
                    self.serialize_expression(expr);
                }
                ast::expression::ExpressionOrSpread::Spread(spread) => {
                    self.serialize_spread_element(spread);
                }
            }
        }
    }

    fn serialize_template_literal(
        &mut self,
        loc: &Loc,
        tl: &ast::expression::TemplateLiteral<Loc, Loc>,
    ) {
        // 65: TemplateLiteral — quasis(NodeList) expressions(NodeList)
        self.write_node_header(NodeKind::TemplateLiteral, loc);
        self.buf.push(tl.quasis.len() as u32);
        for quasi in tl.quasis.iter() {
            // 66: TemplateElement — tail(Bool) cooked(String) raw(String)
            let quasi_loc = self.template_element_loc(&quasi.loc, quasi.tail);
            self.write_node_header(NodeKind::TemplateElement, &quasi_loc);
            self.write_bool(quasi.tail);
            self.write_template_cooked_value(&quasi.value.raw, &quasi.value.cooked);
            self.write_str(&quasi.value.raw);
        }
        self.buf.push(tl.expressions.len() as u32);
        for expr in tl.expressions.iter() {
            self.serialize_expression(expr);
        }
    }

    fn template_element_loc(&self, loc: &Loc, tail: bool) -> Loc {
        if !self.is_babel() {
            return loc.clone();
        }
        Loc {
            source: loc.source.clone(),
            start: Position {
                line: loc.start.line,
                column: loc.start.column + 1,
            },
            end: Position {
                line: loc.end.line,
                column: loc.end.column - if tail { 1 } else { 2 },
            },
        }
    }

    // ---------------------------------------------------------------
    // Function helpers
    // ---------------------------------------------------------------

    fn serialize_function_expr(
        &mut self,
        loc: &Loc,
        func: &ast::function::Function<Loc, Loc>,
        kind: NodeKind,
    ) {
        let wire_kind = if self.is_babel() {
            if kind == NodeKind::ArrowFunctionExpression {
                NodeKind::BabelArrowFunctionExpression
            } else {
                NodeKind::BabelFunctionExpression
            }
        } else {
            kind
        };
        self.write_node_header(wire_kind, loc);

        if kind == NodeKind::ArrowFunctionExpression {
            // ArrowFunctionExpression: params body typeParameters returnType generator async id predicate expression
            self.serialize_function_params(&func.params);
            self.serialize_function_body(&func.body);
            self.serialize_type_params_opt(&func.tparams);
            self.serialize_return_type(&func.return_);
            if !self.is_babel() {
                self.write_bool(false);
            }
            self.write_bool(func.async_);
            match &func.id {
                Some(id) => self.serialize_identifier_node(id),
                None => self.write_null_node(),
            }
        } else {
            // FunctionExpression: id params body typeParameters returnType generator async predicate expression
            match &func.id {
                Some(id) => self.serialize_identifier_node(id),
                None => self.write_null_node(),
            }
            self.serialize_function_params(&func.params);
            self.serialize_function_body(&func.body);
            self.serialize_type_params_opt(&func.tparams);
            self.serialize_return_type(&func.return_);
            self.write_bool(func.generator);
            self.write_bool(func.async_);
        }

        match &func.predicate {
            Some(pred) => self.serialize_predicate(pred),
            None => self.write_null_node(),
        }
        if !self.is_babel() {
            self.write_bool(matches!(&func.body, ast::function::Body::BodyExpression(_)));
        }
    }

    fn serialize_function_decl(
        &mut self,
        loc: &Loc,
        func: &ast::function::Function<Loc, Loc>,
        kind: NodeKind,
    ) {
        // Hook functions emit a different node kind (HookDeclaration) with a
        // narrower property set: no generator/predicate/expression.
        if !self.is_babel() && func.effect_ == ast::function::Effect::Hook {
            // 24: HookDeclaration — id params body returnType typeParameters async
            self.write_node_header(NodeKind::HookDeclaration, loc);
            match &func.id {
                Some(id) => self.serialize_identifier_node(id),
                None => self.write_null_node(),
            }
            self.serialize_function_params(&func.params);
            self.serialize_function_body(&func.body);
            self.serialize_return_type(&func.return_);
            self.serialize_type_params_opt(&func.tparams);
            self.write_bool(func.async_);
            return;
        }
        // 19: FunctionDeclaration — id params body typeParameters returnType generator async predicate expression
        self.write_node_header(
            if self.is_babel() {
                NodeKind::BabelFunctionDeclaration
            } else {
                kind
            },
            loc,
        );
        match &func.id {
            Some(id) => self.serialize_identifier_node(id),
            None => self.write_null_node(),
        }
        self.serialize_function_params(&func.params);
        self.serialize_function_body(&func.body);
        self.serialize_type_params_opt(&func.tparams);
        self.serialize_return_type(&func.return_);
        self.write_bool(func.generator);
        self.write_bool(func.async_);
        match &func.predicate {
            Some(pred) => self.serialize_predicate(pred),
            None => self.write_null_node(),
        }
        if self.is_babel() {
            let flags = self.babel_flags(loc);
            self.write_bool(flags & COMPONENT_FUNCTION_FLAG != 0);
            self.write_bool(flags & HOOK_FUNCTION_FLAG != 0);
        } else {
            self.write_bool(matches!(&func.body, ast::function::Body::BodyExpression(_)));
        }
    }

    fn serialize_function_params(&mut self, params: &ast::function::Params<Loc, Loc>) {
        let this_count = if params.this_.is_some() { 1 } else { 0 };
        let rest_count = if params.rest.is_some() { 1 } else { 0 };
        let count = this_count + params.params.len() + rest_count;
        self.buf.push(count as u32);
        // Prepend `this` as Identifier { name: "this", typeAnnotation }
        // ahead of the regular params.
        if let Some(this) = &params.this_ {
            // 76: Identifier — name(String), typeAnnotation(Node), optional(Bool)
            self.write_identifier_node_header(&this.loc, "this");
            self.write_str("this");
            self.serialize_type_annotation(&this.annot);
            self.write_bool(false);
        }
        for param in params.params.iter() {
            self.serialize_function_param(param);
        }
        if let Some(rest) = &params.rest {
            self.serialize_rest_element(&rest.loc, &rest.argument);
        }
    }

    fn serialize_function_param(&mut self, param: &ast::function::Param<Loc, Loc>) {
        match param {
            ast::function::Param::RegularParam {
                loc,
                argument,
                default,
            } => match default {
                Some(def) => {
                    // 81: AssignmentPattern — left(Node) right(Node)
                    self.write_node_header(NodeKind::AssignmentPattern, loc);
                    self.serialize_pattern(argument);
                    self.serialize_expression(def);
                }
                None => {
                    self.serialize_pattern(argument);
                }
            },
            ast::function::Param::ParamProperty { loc, property } => {
                // 91: ParameterProperty — key value typeAnnotation computed static variance
                //     tsAccessibility declare optional decorators
                self.write_node_header(NodeKind::ParameterProperty, loc);
                self.serialize_object_key(&property.key);
                match &property.value {
                    ast::class::property::Value::Initialized(expr) => {
                        self.serialize_expression(expr);
                    }
                    _ => self.write_null_node(),
                }
                self.serialize_annotation_or_hint(&property.annot);
                self.write_bool(is_key_computed(&property.key));
                self.write_bool(property.static_);
                match &property.variance {
                    Some(v) => self.serialize_variance(v),
                    None => self.write_null_node(),
                }
                self.write_str_opt(
                    property
                        .ts_accessibility
                        .as_ref()
                        .map(|a| ts_accessibility_str(a.kind)),
                );
                self.write_bool(matches!(
                    property.value,
                    ast::class::property::Value::Declared
                ));
                self.write_bool(property.optional);
                self.buf.push(property.decorators.len() as u32);
                for dec in property.decorators.iter() {
                    self.write_node_header(NodeKind::Decorator, &dec.loc);
                    self.serialize_expression(&dec.expression);
                }
            }
        }
    }

    fn serialize_function_body(&mut self, body: &ast::function::Body<Loc, Loc>) {
        match body {
            ast::function::Body::BodyBlock((loc, block)) => {
                self.serialize_block_statement(loc, block);
            }
            ast::function::Body::BodyExpression(expr) => {
                self.serialize_expression(expr);
            }
        }
    }

    fn serialize_return_type(&mut self, ret: &ast::function::ReturnAnnot<Loc, Loc>) {
        match ret {
            ast::function::ReturnAnnot::Missing(_) => self.write_null_node(),
            ast::function::ReturnAnnot::Available(annot) => {
                self.serialize_type_annotation(annot);
            }
            ast::function::ReturnAnnot::TypeGuard(tga) => {
                self.serialize_type_guard_annotation(tga);
            }
        }
    }

    fn serialize_type_guard_annotation(&mut self, tga: &ast::types::TypeGuardAnnotation<Loc, Loc>) {
        // 111: TypeAnnotation — typeAnnotation(Node)
        self.write_node_header(NodeKind::TypeAnnotation, &tga.loc);
        self.serialize_type_guard(&tga.guard);
    }

    // ---------------------------------------------------------------
    // Class helpers
    // ---------------------------------------------------------------

    fn serialize_class(&mut self, loc: &Loc, class: &ast::class::Class<Loc, Loc>, kind: NodeKind) {
        // 22/84: ClassDeclaration/ClassExpression
        // id typeParameters superClass implements body superTypeArguments decorators abstract
        self.write_node_header(
            if self.is_babel() {
                if kind == NodeKind::ClassDeclaration {
                    NodeKind::BabelClassDeclaration
                } else {
                    NodeKind::BabelClassExpression
                }
            } else {
                kind
            },
            loc,
        );
        match &class.id {
            Some(id) => self.serialize_identifier_node(id),
            None => self.write_null_node(),
        }
        self.serialize_type_params_opt(&class.tparams);
        match &class.extends {
            Some(ext) => self.serialize_expression(&ext.expr),
            None => self.write_null_node(),
        }
        match &class.implements {
            Some(impls) => {
                self.buf.push(impls.interfaces.len() as u32);
                for iface in impls.interfaces.iter() {
                    // 86: ClassImplements — id(Node) typeArguments(Node)
                    self.write_type_use_node_header(NodeKind::ClassImplements, &iface.loc);
                    self.serialize_generic_type_identifier(&iface.id);
                    self.serialize_type_args_opt(&iface.targs);
                }
            }
            None => self.buf.push(0),
        }
        self.serialize_class_body(&class.body);
        match &class.extends {
            Some(ext) => self.serialize_type_args_opt(&ext.targs),
            None => self.write_null_node(),
        }
        self.buf.push(class.class_decorators.len() as u32);
        for dec in class.class_decorators.iter() {
            // 90: Decorator — expression(Node)
            self.write_node_header(NodeKind::Decorator, &dec.loc);
            self.serialize_expression(&dec.expression);
        }
        if !self.is_babel() {
            self.write_bool(class.abstract_);
        }
    }

    fn serialize_class_body(&mut self, body: &ast::class::Body<Loc, Loc>) {
        // 85: ClassBody — body(NodeList)
        self.write_node_header(NodeKind::ClassBody, &body.loc);
        self.buf.push(body.body.len() as u32);
        for elem in body.body.iter() {
            self.serialize_class_body_element(elem);
        }
    }

    fn serialize_class_body_element(&mut self, elem: &ast::class::BodyElement<Loc, Loc>) {
        match elem {
            ast::class::BodyElement::Method(m) => {
                if self.is_babel() {
                    let private = matches!(m.key, ast::expression::object::Key::PrivateName(_));
                    self.write_node_header(
                        if private {
                            NodeKind::BabelClassPrivateMethod
                        } else {
                            NodeKind::BabelClassMethod
                        },
                        &m.loc,
                    );
                    self.write_str(method_kind_str(m.kind));
                    if !private {
                        self.write_bool(is_key_computed(&m.key));
                    }
                    self.write_bool(m.static_);
                    self.serialize_object_key(&m.key);
                    self.write_null_node();
                    let (_, function) = &m.value;
                    self.serialize_function_params(&function.params);
                    self.serialize_function_body(&function.body);
                    self.write_bool(function.async_);
                    self.write_bool(function.generator);
                    self.serialize_return_type(&function.return_);
                    self.serialize_type_params_opt(&function.tparams);
                    self.write_null_node();
                    return;
                }
                // 87: MethodDefinition — key value kind static computed decorators
                //     override tsAccessibility
                self.write_node_header(NodeKind::MethodDefinition, &m.loc);
                self.serialize_object_key(&m.key);
                // value is a FunctionExpression node
                let (fn_loc, func) = &m.value;
                self.serialize_function_expr(fn_loc, func, NodeKind::FunctionExpression);
                self.write_str(method_kind_str(m.kind));
                self.write_bool(m.static_);
                self.write_bool(is_key_computed(&m.key));
                self.buf.push(m.decorators.len() as u32);
                for dec in m.decorators.iter() {
                    self.write_node_header(NodeKind::Decorator, &dec.loc);
                    self.serialize_expression(&dec.expression);
                }
                self.write_bool(m.override_);
                self.write_str_opt(
                    m.ts_accessibility
                        .as_ref()
                        .map(|a| ts_accessibility_str(a.kind)),
                );
            }
            ast::class::BodyElement::Property(p) => {
                if self.is_babel() {
                    self.write_node_header(NodeKind::BabelClassProperty, &p.loc);
                    self.serialize_object_key(&p.key);
                    match &p.value {
                        ast::class::property::Value::Initialized(expr) => {
                            self.serialize_expression(expr);
                        }
                        _ => self.write_null_node(),
                    }
                    self.serialize_annotation_or_hint(&p.annot);
                    self.write_bool(p.static_);
                    match &p.variance {
                        Some(variance) => self.serialize_variance(variance),
                        None => self.write_null_node(),
                    }
                    self.write_bool(matches!(p.value, ast::class::property::Value::Declared));
                    self.write_bool(p.optional);
                    self.write_bool(is_key_computed(&p.key));
                    return;
                }
                // 88: PropertyDefinition — key value typeAnnotation computed static variance
                //     tsAccessibility declare optional override decorators
                self.write_node_header(NodeKind::PropertyDefinition, &p.loc);
                self.serialize_object_key(&p.key);
                match &p.value {
                    ast::class::property::Value::Initialized(expr) => {
                        self.serialize_expression(expr);
                    }
                    _ => self.write_null_node(),
                }
                self.serialize_annotation_or_hint(&p.annot);
                self.write_bool(is_key_computed(&p.key));
                self.write_bool(p.static_);
                match &p.variance {
                    Some(v) => self.serialize_variance(v),
                    None => self.write_null_node(),
                }
                self.write_str_opt(
                    p.ts_accessibility
                        .as_ref()
                        .map(|a| ts_accessibility_str(a.kind)),
                );
                self.write_bool(matches!(p.value, ast::class::property::Value::Declared));
                self.write_bool(p.optional);
                self.write_bool(p.override_);
                self.buf.push(p.decorators.len() as u32);
                for dec in p.decorators.iter() {
                    self.write_node_header(NodeKind::Decorator, &dec.loc);
                    self.serialize_expression(&dec.expression);
                }
            }
            ast::class::BodyElement::PrivateField(pf) => {
                // 88: PropertyDefinition — key value typeAnnotation computed static variance
                //     tsAccessibility declare optional override decorators
                let mut private_field_loc = pf.loc.clone();
                private_field_loc.start = pf.key.loc.start;
                if self.is_babel() {
                    self.write_node_header(NodeKind::BabelClassPrivateProperty, &private_field_loc);
                    self.serialize_private_name(&pf.key);
                    match &pf.value {
                        ast::class::property::Value::Initialized(expr) => {
                            self.serialize_expression(expr);
                        }
                        _ => self.write_null_node(),
                    }
                    self.serialize_annotation_or_hint(&pf.annot);
                    self.write_bool(pf.static_);
                    match &pf.variance {
                        Some(variance) => self.serialize_variance(variance),
                        None => self.write_null_node(),
                    }
                    return;
                }
                self.write_node_header(NodeKind::PropertyDefinition, &private_field_loc);
                // key: PrivateIdentifier
                self.serialize_private_name(&pf.key);
                match &pf.value {
                    ast::class::property::Value::Initialized(expr) => {
                        self.serialize_expression(expr);
                    }
                    _ => self.write_null_node(),
                }
                self.serialize_annotation_or_hint(&pf.annot);
                self.write_bool(false); // computed: always false for private
                self.write_bool(pf.static_);
                match &pf.variance {
                    Some(v) => self.serialize_variance(v),
                    None => self.write_null_node(),
                }
                self.write_str_opt(
                    pf.ts_accessibility
                        .as_ref()
                        .map(|a| ts_accessibility_str(a.kind)),
                );
                self.write_bool(matches!(pf.value, ast::class::property::Value::Declared));
                self.write_bool(pf.optional);
                self.write_bool(pf.override_);
                self.buf.push(pf.decorators.len() as u32);
                for dec in pf.decorators.iter() {
                    self.write_node_header(NodeKind::Decorator, &dec.loc);
                    self.serialize_expression(&dec.expression);
                }
            }
            ast::class::BodyElement::StaticBlock(sb) => {
                // 89: StaticBlock — body(NodeList)
                self.write_node_header(NodeKind::StaticBlock, &sb.loc);
                self.buf.push(sb.body.len() as u32);
                for stmt in sb.body.iter() {
                    self.serialize_statement(stmt);
                }
            }
            ast::class::BodyElement::DeclareMethod(dm) => {
                // 92: DeclareMethodDefinition — key value static optional computed kind override tsAccessibility
                self.write_node_header(NodeKind::DeclareMethodDefinition, &dm.loc);
                self.serialize_object_key(&dm.key);
                // value: TypeAnnotation node wrapping the type
                self.serialize_type_annotation(&dm.annot);
                self.write_bool(dm.static_);
                self.write_bool(dm.optional);
                self.write_bool(is_key_computed(&dm.key));
                let kind_str = match dm.kind {
                    ast::class::MethodKind::Get => Some("get"),
                    ast::class::MethodKind::Set => Some("set"),
                    ast::class::MethodKind::Method | ast::class::MethodKind::Constructor => None,
                };
                self.write_str_opt(kind_str);
                self.write_bool(dm.override_);
                self.write_str_opt(
                    dm.ts_accessibility
                        .as_ref()
                        .map(|a| ts_accessibility_str(a.kind)),
                );
            }
            ast::class::BodyElement::AbstractMethod(am) => {
                // 93: AbstractMethodDefinition — key value computed override tsAccessibility
                self.write_node_header(NodeKind::AbstractMethodDefinition, &am.loc);
                self.serialize_object_key(&am.key);
                // value: function type annotation
                let (fn_loc, func) = &am.annot;
                self.serialize_function_type(fn_loc, func);
                self.write_bool(is_key_computed(&am.key));
                self.write_bool(am.override_);
                self.write_str_opt(
                    am.ts_accessibility
                        .as_ref()
                        .map(|a| ts_accessibility_str(a.kind)),
                );
            }
            ast::class::BodyElement::AbstractProperty(ap) => {
                self.write_node_header(NodeKind::AbstractPropertyDefinition, &ap.loc);
                self.serialize_object_key(&ap.key);
                self.write_null_node();
                self.serialize_annotation_or_hint(&ap.annot);
                self.write_bool(is_key_computed(&ap.key));
                match &ap.variance {
                    Some(v) => self.serialize_variance(v),
                    None => self.write_null_node(),
                }
                self.write_bool(ap.override_);
                self.write_str_opt(
                    ap.ts_accessibility
                        .as_ref()
                        .map(|a| ts_accessibility_str(a.kind)),
                );
            }
            ast::class::BodyElement::IndexSignature(indexer) => {
                // 150: ObjectTypeIndexer — id key value static variance optional
                self.write_node_header(NodeKind::ObjectTypeIndexer, &indexer.loc);
                match &indexer.id {
                    Some(id) => self.serialize_identifier_node(id),
                    None => self.write_null_node(),
                }
                self.serialize_type(&indexer.key);
                self.serialize_type(&indexer.value);
                self.write_bool(indexer.static_);
                match &indexer.variance {
                    Some(v) => self.serialize_variance(v),
                    None => self.write_null_node(),
                }
                self.write_bool(indexer.optional);
            }
        }
    }

    // ---------------------------------------------------------------
    // JSX helpers
    // ---------------------------------------------------------------

    fn serialize_jsx_element(&mut self, loc: &Loc, elem: &ast::jsx::Element<Loc, Loc>) {
        // 167: JSXElement — openingElement children closingElement
        self.write_node_header(NodeKind::JSXElement, loc);
        self.serialize_jsx_opening_element(&elem.opening_element);
        self.buf.push(elem.children.1.len() as u32);
        for child in elem.children.1.iter() {
            self.serialize_jsx_child(child);
        }
        match &elem.closing_element {
            Some(closing) => {
                // 171: JSXClosingElement — name(Node)
                self.write_node_header(NodeKind::JSXClosingElement, &closing.loc);
                self.serialize_jsx_name(&closing.name);
            }
            None => self.write_null_node(),
        }
    }

    fn serialize_jsx_fragment(&mut self, loc: &Loc, frag: &ast::jsx::Fragment<Loc, Loc>) {
        // 168: JSXFragment — openingFragment children closingFragment
        self.write_node_header(NodeKind::JSXFragment, loc);
        self.write_node_header(NodeKind::JSXOpeningFragment, &frag.frag_opening_element);
        self.buf.push(frag.frag_children.1.len() as u32);
        for child in frag.frag_children.1.iter() {
            self.serialize_jsx_child(child);
        }
        self.write_node_header(NodeKind::JSXClosingFragment, &frag.frag_closing_element);
    }

    fn serialize_jsx_opening_element(&mut self, opening: &ast::jsx::Opening<Loc, Loc>) {
        // 169: JSXOpeningElement — name attributes selfClosing typeArguments
        self.write_node_header(
            if self.is_babel() {
                NodeKind::BabelJSXOpeningElement
            } else {
                NodeKind::JSXOpeningElement
            },
            &opening.loc,
        );
        self.serialize_jsx_name(&opening.name);
        self.buf.push(opening.attributes.len() as u32);
        for attr in opening.attributes.iter() {
            match attr {
                ast::jsx::OpeningAttribute::Attribute(a) => {
                    // 173: JSXAttribute — name(Node) value(Node)
                    self.write_node_header(NodeKind::JSXAttribute, &a.loc);
                    self.serialize_jsx_attribute_name(&a.name);
                    match &a.value {
                        Some(v) => self.serialize_jsx_attribute_value(v),
                        None => self.write_null_node(),
                    }
                }
                ast::jsx::OpeningAttribute::SpreadAttribute(sa) => {
                    // 174: JSXSpreadAttribute — argument(Node)
                    self.write_node_header(NodeKind::JSXSpreadAttribute, &sa.loc);
                    self.serialize_expression(&sa.argument);
                }
            }
        }
        self.write_bool(opening.self_closing);
        if !self.is_babel() {
            self.serialize_call_type_args_opt(&opening.targs);
        }
    }

    fn serialize_jsx_name(&mut self, name: &ast::jsx::Name<Loc, Loc>) {
        match name {
            ast::jsx::Name::Identifier(id) => {
                // 181: JSXIdentifier — name(String)
                self.write_node_header(NodeKind::JSXIdentifier, &id.loc);
                self.write_str(&id.name);
            }
            ast::jsx::Name::NamespacedName(ns) => {
                // 180: JSXNamespacedName — namespace(Node) name(Node)
                self.write_node_header(NodeKind::JSXNamespacedName, &ns.loc);
                self.write_node_header(NodeKind::JSXIdentifier, &ns.namespace.loc);
                self.write_str(&ns.namespace.name);
                self.write_node_header(NodeKind::JSXIdentifier, &ns.name.loc);
                self.write_str(&ns.name.name);
            }
            ast::jsx::Name::MemberExpression(me) => {
                self.serialize_jsx_member_expression(me);
            }
        }
    }

    fn serialize_jsx_member_expression(&mut self, me: &ast::jsx::MemberExpression<Loc, Loc>) {
        // 179: JSXMemberExpression — object(Node) property(Node)
        self.write_node_header(NodeKind::JSXMemberExpression, &me.loc);
        match &me.object {
            ast::jsx::member_expression::Object::Identifier(id) => {
                self.write_node_header(NodeKind::JSXIdentifier, &id.loc);
                self.write_str(&id.name);
            }
            ast::jsx::member_expression::Object::MemberExpression(inner) => {
                self.serialize_jsx_member_expression(inner);
            }
        }
        self.write_node_header(NodeKind::JSXIdentifier, &me.property.loc);
        self.write_str(&me.property.name);
    }

    fn serialize_jsx_attribute_name(&mut self, name: &ast::jsx::attribute::Name<Loc, Loc>) {
        match name {
            ast::jsx::attribute::Name::Identifier(id) => {
                self.write_node_header(NodeKind::JSXIdentifier, &id.loc);
                self.write_str(&id.name);
            }
            ast::jsx::attribute::Name::NamespacedName(ns) => {
                self.write_node_header(NodeKind::JSXNamespacedName, &ns.loc);
                self.write_node_header(NodeKind::JSXIdentifier, &ns.namespace.loc);
                self.write_str(&ns.namespace.name);
                self.write_node_header(NodeKind::JSXIdentifier, &ns.name.loc);
                self.write_str(&ns.name.name);
            }
        }
    }

    fn serialize_jsx_attribute_value(&mut self, val: &ast::jsx::attribute::Value<Loc, Loc>) {
        match val {
            ast::jsx::attribute::Value::StringLiteral((loc, lit)) => {
                self.write_string_literal_preserving_value(loc, &lit.value, &lit.raw);
            }
            ast::jsx::attribute::Value::ExpressionContainer((loc, container)) => {
                // 176: JSXExpressionContainer — expression(Node)
                self.write_node_header(NodeKind::JSXExpressionContainer, loc);
                match &container.expression {
                    ast::jsx::expression_container::Expression::Expression(expr) => {
                        self.serialize_expression(expr);
                    }
                    ast::jsx::expression_container::Expression::EmptyExpression => {
                        // The inner JSXEmptyExpression spans the gap between
                        // `{` and `}` exclusively rather than reusing the outer
                        // container loc.
                        let mut empty_loc = loc.clone();
                        empty_loc.start.column += 1;
                        empty_loc.end.column -= 1;
                        self.write_node_header(NodeKind::JSXEmptyExpression, &empty_loc);
                    }
                }
            }
        }
    }

    fn serialize_jsx_child(&mut self, child: &ast::jsx::Child<Loc, Loc>) {
        match child {
            ast::jsx::Child::Element { loc, inner } => {
                self.serialize_jsx_element(loc, inner);
            }
            ast::jsx::Child::Fragment { loc, inner } => {
                self.serialize_jsx_fragment(loc, inner);
            }
            ast::jsx::Child::ExpressionContainer { loc, inner } => {
                self.write_node_header(NodeKind::JSXExpressionContainer, loc);
                match &inner.expression {
                    ast::jsx::expression_container::Expression::Expression(expr) => {
                        self.serialize_expression(expr);
                    }
                    ast::jsx::expression_container::Expression::EmptyExpression => {
                        // The inner JSXEmptyExpression spans the gap between
                        // `{` and `}` exclusively rather than reusing the outer
                        // container loc.
                        let mut empty_loc = loc.clone();
                        empty_loc.start.column += 1;
                        empty_loc.end.column -= 1;
                        self.write_node_header(NodeKind::JSXEmptyExpression, &empty_loc);
                    }
                }
            }
            ast::jsx::Child::SpreadChild { loc, inner } => {
                // 177: JSXSpreadChild — expression(Node)
                self.write_node_header(NodeKind::JSXSpreadChild, loc);
                self.serialize_expression(&inner.expression);
            }
            ast::jsx::Child::Text { loc, inner } => {
                // 178: JSXText — value(String) raw(String)
                self.write_node_header(
                    if self.is_babel() {
                        NodeKind::BabelJSXText
                    } else {
                        NodeKind::JSXText
                    },
                    loc,
                );
                self.write_str(&inner.value);
                self.write_str(&inner.raw);
            }
        }
    }

    // ---------------------------------------------------------------
    // Import / Export helpers
    // ---------------------------------------------------------------

    fn serialize_import_declaration(
        &mut self,
        loc: &Loc,
        decl: &ast::statement::ImportDeclaration<Loc, Loc>,
    ) {
        // 29: ImportDeclaration — specifiers(NodeList) source(Node) importKind(String) attributes(NodeList) phase(String)
        self.write_node_header(
            if self.is_babel() {
                NodeKind::BabelImportDeclaration
            } else {
                NodeKind::ImportDeclaration
            },
            loc,
        );
        // Build specifiers list: default (if any) + specifiers (named or namespace)
        let default_count = if decl.default.is_some() { 1 } else { 0 };
        let spec_count = match &decl.specifiers {
            Some(ast::statement::import_declaration::Specifier::ImportNamedSpecifiers(specs)) => {
                specs.len()
            }
            Some(ast::statement::import_declaration::Specifier::ImportNamespaceSpecifier(_)) => 1,
            None => 0,
        };
        self.buf.push((default_count + spec_count) as u32);
        if let Some(def) = &decl.default {
            // 30: ImportDefaultSpecifier — local(Node)
            self.write_node_header(NodeKind::ImportDefaultSpecifier, &def.identifier.loc);
            self.serialize_identifier_node(&def.identifier);
        }
        match &decl.specifiers {
            Some(ast::statement::import_declaration::Specifier::ImportNamedSpecifiers(specs)) => {
                for spec in specs.iter() {
                    // 32: ImportSpecifier — imported(Node) local(Node) importKind(String)
                    // The span starts at the per-specifier `type`/`typeof`
                    // keyword loc when present, otherwise at the remote id;
                    // and ends at the local id when an alias is present,
                    // otherwise at the remote id.
                    let start_loc = match &spec.kind_loc {
                        Some(kl) => kl,
                        None => &spec.remote.loc,
                    };
                    let span_loc = match &spec.local {
                        Some(local) => Loc::between(start_loc, &local.loc),
                        None => Loc::between(start_loc, &spec.remote.loc),
                    };
                    self.write_node_header(NodeKind::ImportSpecifier, &span_loc);
                    self.serialize_identifier_node(&spec.remote);
                    match &spec.local {
                        Some(local) => self.serialize_identifier_node(local),
                        None => self.serialize_identifier_node(&spec.remote),
                    }
                    // ImportValue and absent kind both encode as the null
                    // string sentinel.
                    let kind_str = match spec.kind {
                        Some(ast::statement::ImportKind::ImportType) => Some("type"),
                        Some(ast::statement::ImportKind::ImportTypeof) => Some("typeof"),
                        Some(ast::statement::ImportKind::ImportValue) | None => None,
                    };
                    self.write_str_opt(kind_str);
                }
            }
            Some(ast::statement::import_declaration::Specifier::ImportNamespaceSpecifier((
                ns_loc,
                ns_id,
            ))) => {
                // 31: ImportNamespaceSpecifier — local(Node)
                self.write_node_header(NodeKind::ImportNamespaceSpecifier, ns_loc);
                self.serialize_identifier_node(ns_id);
            }
            None => {}
        }
        // source
        self.write_string_literal(&decl.source.0, &decl.source.1.value, &decl.source.1.raw);
        // importKind
        self.write_str(import_kind_str(decl.import_kind));
        // attributes
        self.serialize_import_attributes(decl.attributes.as_ref());
        // phase — the null string sentinel for an unmodified import.
        self.write_str_opt(decl.phase.map(|phase| phase.as_str()));
    }

    fn serialize_import_attributes(
        &mut self,
        attributes: Option<&(
            Loc,
            Vec<ast::statement::import_declaration::ImportAttribute<Loc, Loc>>,
        )>,
    ) {
        match attributes {
            Some((_attrs_loc, attrs)) => {
                self.buf.push(attrs.len() as u32);
                for attr in attrs.iter() {
                    self.serialize_import_attribute(attr);
                }
            }
            None => self.buf.push(0),
        }
    }

    fn serialize_import_attribute(
        &mut self,
        attr: &ast::statement::import_declaration::ImportAttribute<Loc, Loc>,
    ) {
        // 33: ImportAttribute — key(Node) value(Node)
        self.write_node_header(NodeKind::ImportAttribute, &attr.loc);
        match &attr.key {
            ast::statement::import_declaration::ImportAttributeKey::Identifier(id) => {
                self.serialize_identifier_node(id);
            }
            ast::statement::import_declaration::ImportAttributeKey::StringLiteral(loc, lit) => {
                self.write_string_literal(loc, &lit.value, &lit.raw);
            }
        }
        // value (string literal)
        self.write_string_literal(&attr.value.0, &attr.value.1.value, &attr.value.1.raw);
    }

    fn serialize_import_equals_declaration(
        &mut self,
        loc: &Loc,
        decl: &ast::statement::ImportEqualsDeclaration<Loc, Loc>,
    ) {
        // 34: ImportEqualsDeclaration — id moduleReference importKind isExport
        self.write_node_header(NodeKind::ImportEqualsDeclaration, loc);
        self.serialize_identifier_node(&decl.id);
        match &decl.module_reference {
            ast::statement::import_equals_declaration::ModuleReference::ExternalModuleReference(
                ref_loc,
                lit,
            ) => {
                // 35: ExternalModuleReference — expression(Node)
                self.write_node_header(NodeKind::ExternalModuleReference, ref_loc);
                self.write_string_literal(ref_loc, &lit.value, &lit.raw);
            }
            ast::statement::import_equals_declaration::ModuleReference::Identifier(id) => {
                self.serialize_generic_type_identifier(id);
            }
        }
        self.write_str(import_kind_str(decl.import_kind));
        self.write_bool(decl.is_export);
    }

    fn serialize_export_named_declaration(
        &mut self,
        loc: &Loc,
        decl: &ast::statement::ExportNamedDeclaration<Loc, Loc>,
    ) {
        // When the only specifier is `ExportBatchSpecifier` (`export * from`,
        // `export * as foo from`, `export type * from`), emit an
        // `ExportAllDeclaration` node with (source, exported, exportKind);
        // otherwise fall through to the standard ExportNamedDeclaration shape.
        match &decl.specifiers {
            Some(ast::statement::export_named_declaration::Specifier::ExportBatchSpecifier(
                batch,
            )) => {
                if self.is_babel()
                    && let Some(exported) = &batch.specifier
                {
                    self.write_node_header(NodeKind::BabelExportNamedDeclaration, loc);
                    self.write_null_node();
                    self.buf.push(1);
                    let specifier_loc = Loc {
                        source: loc.source.clone(),
                        start: Position {
                            line: loc.start.line,
                            column: loc.start.column + 7,
                        },
                        end: exported.loc.end,
                    };
                    self.write_node_header(NodeKind::ExportNamespaceSpecifier, &specifier_loc);
                    self.serialize_identifier_node(exported);
                    match &decl.source {
                        Some((source_loc, source)) => {
                            self.write_string_literal(source_loc, &source.value, &source.raw);
                        }
                        None => self.write_null_node(),
                    }
                    self.write_str(export_kind_str(decl.export_kind));
                    self.serialize_import_attributes(decl.attributes.as_ref());
                    return;
                }
                // 38: ExportAllDeclaration — source(Node) exported(Node) exportKind(String)
                self.write_node_header(
                    if self.is_babel() {
                        NodeKind::BabelExportAllDeclaration
                    } else {
                        NodeKind::ExportAllDeclaration
                    },
                    loc,
                );
                match &decl.source {
                    Some((src_loc, src_lit)) => {
                        self.write_string_literal(src_loc, &src_lit.value, &src_lit.raw);
                    }
                    None => self.write_null_node(),
                }
                if !self.is_babel() {
                    match &batch.specifier {
                        Some(id) => self.serialize_identifier_node(id),
                        None => self.write_null_node(),
                    }
                }
                self.write_str(export_kind_str(decl.export_kind));
                self.serialize_import_attributes(decl.attributes.as_ref());
            }
            specifiers => {
                // 36: ExportNamedDeclaration — declaration specifiers source exportKind
                self.write_node_header(
                    if self.is_babel() {
                        NodeKind::BabelExportNamedDeclaration
                    } else {
                        NodeKind::ExportNamedDeclaration
                    },
                    loc,
                );
                match &decl.declaration {
                    Some(stmt) => self.serialize_statement(stmt),
                    None => self.write_null_node(),
                }
                match specifiers {
                    Some(
                        ast::statement::export_named_declaration::Specifier::ExportSpecifiers(
                            specs,
                        ),
                    ) => {
                        self.buf.push(specs.len() as u32);
                        for spec in specs.iter() {
                            // 39: ExportSpecifier — exported(Node) local(Node) exportKind(String)
                            self.write_node_header(NodeKind::ExportSpecifier, &spec.loc);
                            match &spec.exported {
                                Some(exported) => self.serialize_identifier_node(exported),
                                None => self.serialize_identifier_node(&spec.local),
                            }
                            self.serialize_identifier_node(&spec.local);
                            self.write_str(export_kind_str(spec.export_kind));
                        }
                    }
                    None => self.buf.push(0),
                    Some(
                        ast::statement::export_named_declaration::Specifier::ExportBatchSpecifier(
                            _,
                        ),
                    ) => {
                        // Handled by the outer match arm above.
                    }
                }
                match &decl.source {
                    Some((src_loc, src_lit)) => {
                        self.write_string_literal(src_loc, &src_lit.value, &src_lit.raw);
                    }
                    None => self.write_null_node(),
                }
                self.write_str(export_kind_str(decl.export_kind));
                self.serialize_import_attributes(decl.attributes.as_ref());
            }
        }
    }

    fn serialize_export_default_declaration(
        &mut self,
        loc: &Loc,
        decl: &ast::statement::ExportDefaultDeclaration<Loc, Loc>,
    ) {
        // 37: ExportDefaultDeclaration — declaration(Node) exportKind(MaybeString)
        self.write_node_header(NodeKind::ExportDefaultDeclaration, loc);
        match &decl.declaration {
            ast::statement::export_default_declaration::Declaration::Declaration(stmt) => {
                self.serialize_statement(stmt);
            }
            ast::statement::export_default_declaration::Declaration::Expression(expr) => {
                self.serialize_expression(expr);
            }
        }
        self.write_str_opt((!self.is_babel()).then_some(export_kind_str(decl.export_kind())));
    }

    fn serialize_export_assignment(
        &mut self,
        loc: &Loc,
        decl: &ast::statement::ExportAssignment<Loc, Loc>,
    ) {
        // 41: ExportAssignment — expression(Node)
        self.write_node_header(NodeKind::ExportAssignment, loc);
        match &decl.rhs {
            ast::statement::ExportAssignmentRhs::Expression(expr) => {
                self.serialize_expression(expr);
            }
            ast::statement::ExportAssignmentRhs::DeclareFunction(fn_loc, df) => {
                self.serialize_declare_function(fn_loc, df);
            }
        }
    }

    // ---------------------------------------------------------------
    // Misc helpers
    // ---------------------------------------------------------------

    fn serialize_typed_identifier(
        &mut self,
        id: &ast::Identifier<Loc, Loc>,
        annot: &ast::types::Annotation<Loc, Loc>,
    ) {
        // 76: Identifier — name(String) typeAnnotation(Node) optional(Bool)
        // The Identifier loc spans the name through its type annotation.
        let id_loc = Loc::between(&id.loc, &annot.loc);
        self.write_identifier_node_header(&id_loc, &id.name);
        self.write_str(&id.name);
        self.serialize_type_annotation(annot);
        self.write_bool(false);
    }

    fn serialize_typed_identifier_opt(
        &mut self,
        id: &Option<ast::Identifier<Loc, Loc>>,
        annot: &ast::types::Annotation<Loc, Loc>,
    ) {
        match id {
            Some(id) => self.serialize_typed_identifier(id, annot),
            None => self.write_null_node(),
        }
    }

    fn serialize_interface_extends(&mut self, loc: &Loc, generic: &ast::types::Generic<Loc, Loc>) {
        // 158: InterfaceExtends — id(Node) typeArguments(Node)
        self.write_type_use_node_header(NodeKind::InterfaceExtends, loc);
        self.serialize_generic_type_identifier(&generic.id);
        match &generic.targs {
            Some(targs) => {
                self.write_node_header(NodeKind::TypeParameterInstantiation, &targs.loc);
                self.buf.push(targs.arguments.len() as u32);
                for arg in targs.arguments.iter() {
                    self.serialize_type(arg);
                }
            }
            None => self.write_null_node(),
        }
    }

    fn serialize_declare_class_extends(
        &mut self,
        loc: &Loc,
        ext: &ast::statement::DeclareClassExtends<Loc, Loc>,
    ) {
        match ext {
            ast::statement::DeclareClassExtends::ExtendsIdent(generic) => {
                self.serialize_interface_extends(loc, generic);
            }
            ast::statement::DeclareClassExtends::ExtendsCall { callee, arg } => {
                // DeclareClassExtendsCall — callee(Node) argument(Node)
                self.write_node_header(NodeKind::DeclareClassExtendsCall, loc);
                self.serialize_generic_type(&callee.0, &callee.1);
                self.serialize_declare_class_extends(&arg.0, &arg.1);
            }
        }
    }

    fn serialize_generic_type_identifier(
        &mut self,
        id: &ast::types::generic::Identifier<Loc, Loc>,
    ) {
        match id {
            ast::types::generic::Identifier::Unqualified(ident) => {
                self.serialize_identifier_node(ident);
            }
            ast::types::generic::Identifier::Qualified(q) => {
                // 140: QualifiedTypeIdentifier — qualification(Node) id(Node)
                self.write_node_header(NodeKind::QualifiedTypeIdentifier, &q.loc);
                self.serialize_generic_type_identifier(&q.qualification);
                self.serialize_identifier_node(&q.id);
            }
            ast::types::generic::Identifier::ImportTypeAnnot(imp) => {
                // 143: ImportType — source(Node)
                self.write_node_header(NodeKind::ImportType, &imp.loc);
                // source is a string literal
                let (lit_loc, lit) = &imp.source;
                self.write_string_literal(lit_loc, &lit.value, &lit.raw);
            }
        }
    }

    fn serialize_type_object(&mut self, loc: &Loc, obj: &ast::types::Object<Loc, Loc>) {
        // 147: ObjectTypeAnnotation — properties indexers callProperties internalSlots inexact exact
        self.write_node_header(NodeKind::ObjectTypeAnnotation, loc);
        // Properties are categorized into 4 separate NodeList sections
        // Serialize properties NodeList (NormalProperty + SpreadProperty + MappedType + PrivateField)
        let prop_count = obj
            .properties
            .iter()
            .filter(|p| {
                matches!(
                    p,
                    ast::types::object::Property::NormalProperty(_)
                        | ast::types::object::Property::SpreadProperty(_)
                        | ast::types::object::Property::MappedType(_)
                        | ast::types::object::Property::PrivateField(_)
                )
            })
            .count();
        self.buf.push(prop_count as u32);
        for prop in obj.properties.iter() {
            match prop {
                ast::types::object::Property::NormalProperty(p) => {
                    self.serialize_object_type_property(p);
                }
                ast::types::object::Property::SpreadProperty(sp) => {
                    // 149: ObjectTypeSpreadProperty — argument(Node)
                    self.write_node_header(NodeKind::ObjectTypeSpreadProperty, &sp.loc);
                    self.serialize_type(&sp.argument);
                }
                ast::types::object::Property::MappedType(mt) => {
                    // 152: ObjectTypeMappedTypeProperty — keyTparam propType sourceType nameType
                    // variance varianceOp optional
                    self.write_node_header(NodeKind::ObjectTypeMappedTypeProperty, &mt.loc);
                    self.serialize_type_parameter(&mt.key_tparam);
                    self.serialize_type(&mt.prop_type);
                    self.serialize_type(&mt.source_type);
                    match &mt.name_type {
                        Some(name_type) => self.serialize_type(name_type),
                        None => self.write_null_node(),
                    }
                    match &mt.variance {
                        Some(v) => self.serialize_variance(v),
                        None => self.write_null_node(),
                    }
                    self.write_str_opt(match mt.variance_op {
                        Some(ast::types::object::MappedTypeVarianceOp::Add) => Some("+"),
                        Some(ast::types::object::MappedTypeVarianceOp::Remove) => Some("-"),
                        None => None,
                    });
                    // optional: matches OCaml string()/null (no "none")
                    let optional_str = match mt.optional {
                        ast::types::object::MappedTypeOptionalFlag::PlusOptional => {
                            Some("PlusOptional")
                        }
                        ast::types::object::MappedTypeOptionalFlag::MinusOptional => {
                            Some("MinusOptional")
                        }
                        ast::types::object::MappedTypeOptionalFlag::Optional => Some("Optional"),
                        ast::types::object::MappedTypeOptionalFlag::NoOptionalFlag => None,
                    };
                    self.write_str_opt(optional_str);
                }
                ast::types::object::Property::PrivateField(pf) => {
                    // 226: ObjectTypePrivateField — key(Node)
                    self.write_node_header(NodeKind::ObjectTypePrivateField, &pf.loc);
                    self.serialize_private_name(&pf.key);
                }
                _ => {}
            }
        }
        // Serialize indexers NodeList
        let idx_count = obj
            .properties
            .iter()
            .filter(|p| matches!(p, ast::types::object::Property::Indexer(_)))
            .count();
        self.buf.push(idx_count as u32);
        for prop in obj.properties.iter() {
            if let ast::types::object::Property::Indexer(idx) = prop {
                // 150: ObjectTypeIndexer — id key value static variance
                self.write_node_header(NodeKind::ObjectTypeIndexer, &idx.loc);
                match &idx.id {
                    Some(id) => self.serialize_identifier_node(id),
                    None => self.write_null_node(),
                }
                self.serialize_type(&idx.key);
                self.serialize_type(&idx.value);
                self.write_bool(idx.static_);
                match &idx.variance {
                    Some(v) => self.serialize_variance(v),
                    None => self.write_null_node(),
                }
                self.write_bool(idx.optional);
            }
        }
        // Serialize callProperties NodeList
        let call_count = obj
            .properties
            .iter()
            .filter(|p| matches!(p, ast::types::object::Property::CallProperty(_)))
            .count();
        self.buf.push(call_count as u32);
        for prop in obj.properties.iter() {
            if let ast::types::object::Property::CallProperty(cp) = prop {
                // 151: ObjectTypeCallProperty — value(Node) static(Bool)
                self.write_node_header(NodeKind::ObjectTypeCallProperty, &cp.loc);
                self.serialize_function_type(&cp.value.0, &cp.value.1);
                self.write_bool(cp.static_);
            }
        }
        // Serialize internalSlots NodeList
        let slot_count = obj
            .properties
            .iter()
            .filter(|p| matches!(p, ast::types::object::Property::InternalSlot(_)))
            .count();
        self.buf.push(slot_count as u32);
        for prop in obj.properties.iter() {
            if let ast::types::object::Property::InternalSlot(slot) = prop {
                // 153: ObjectTypeInternalSlot — id optional static method value
                self.write_node_header(NodeKind::ObjectTypeInternalSlot, &slot.loc);
                self.serialize_identifier_node(&slot.id);
                self.write_bool(slot.optional);
                self.write_bool(slot.static_);
                self.write_bool(slot.method);
                self.serialize_type(&slot.value);
            }
        }
        self.write_bool(obj.inexact);
        self.write_bool(obj.exact);
    }

    fn serialize_object_type_property(&mut self, p: &ast::types::object::NormalProperty<Loc, Loc>) {
        // 148: ObjectTypeProperty — key value method optional static proto variance kind
        // computed abstract override tsAccessibility init
        self.write_node_header(NodeKind::ObjectTypeProperty, &p.loc);
        self.serialize_object_key(&p.key);
        // value: depends on PropertyValue variant
        let kind = match &p.value {
            ast::types::object::PropertyValue::Init(ty) => {
                match ty {
                    Some(ty) => self.serialize_type(ty),
                    None => self.write_null_node(),
                }
                "init"
            }
            ast::types::object::PropertyValue::Get(fn_loc, func) => {
                self.serialize_function_type(fn_loc, func);
                "get"
            }
            ast::types::object::PropertyValue::Set(fn_loc, func) => {
                self.serialize_function_type(fn_loc, func);
                "set"
            }
        };
        self.write_bool(p.method);
        self.write_bool(p.optional);
        self.write_bool(p.static_);
        self.write_bool(p.proto);
        match &p.variance {
            Some(v) => self.serialize_variance(v),
            None => self.write_null_node(),
        }
        self.write_str(kind);
        self.write_bool(is_key_computed(&p.key));
        self.write_bool(p.abstract_);
        self.write_bool(p.override_);
        self.write_str_opt(p.ts_accessibility.as_ref().map(|accessibility| {
            use ast::class::ts_accessibility::Kind;

            match accessibility.kind {
                Kind::Private => "private",
                Kind::Protected => "protected",
                Kind::Public => "public",
            }
        }));
        match &p.init {
            Some(init) => self.serialize_expression(init),
            None => self.write_null_node(),
        }
    }

    fn serialize_constructor_type(
        &mut self,
        loc: &Loc,
        abstract_: bool,
        func: &ast::types::Function<Loc, Loc>,
    ) {
        // 225: ConstructorTypeAnnotation — abstract params returnType rest typeParameters
        self.write_node_header(NodeKind::ConstructorTypeAnnotation, loc);
        self.write_bool(abstract_);
        // params NodeList
        self.buf.push(func.params.params.len() as u32);
        for param in func.params.params.iter() {
            self.serialize_function_type_param(param);
        }
        // returnType
        match &func.return_ {
            ast::types::function::ReturnAnnotation::Available(annot) => {
                self.serialize_type(&annot.annotation);
            }
            ast::types::function::ReturnAnnotation::TypeGuard(tg) => {
                self.serialize_type_guard(tg);
            }
            ast::types::function::ReturnAnnotation::Missing(_) => {
                self.write_null_node();
            }
        }
        // rest: discard the outer loc (which includes `...`) and emit the
        // inner argument via `serialize_function_type_param`, which unwraps
        // Destructuring directly into a pattern node.
        match &func.params.rest {
            Some(rest) => {
                self.serialize_function_type_param(&rest.argument);
            }
            None => self.write_null_node(),
        }
        // typeParameters
        self.serialize_type_params_opt(&func.tparams);
    }

    fn serialize_function_type_param(&mut self, param: &ast::types::function::Param<Loc, Loc>) {
        // Anonymous and Labeled emit a FunctionTypeParam wrapper; Destructuring
        // emits the pattern node directly with no wrapper.
        match &param.param {
            ast::types::function::ParamKind::Destructuring(pat) => {
                self.serialize_pattern(pat);
            }
            _ => {
                // 154: FunctionTypeParam — name typeAnnotation optional
                self.write_node_header(NodeKind::FunctionTypeParam, &param.loc);
                self.serialize_function_type_param_kind(&param.param);
            }
        }
    }

    fn serialize_function_type_param_kind(
        &mut self,
        kind: &ast::types::function::ParamKind<Loc, Loc>,
    ) {
        match kind {
            ast::types::function::ParamKind::Anonymous(ty) => {
                self.write_null_node();
                self.serialize_type(ty);
                self.write_bool(false);
            }
            ast::types::function::ParamKind::Labeled {
                name,
                annot,
                optional,
            } => {
                self.serialize_identifier_node(name);
                self.serialize_type(annot);
                self.write_bool(*optional);
            }
            ast::types::function::ParamKind::Destructuring(_) => {
                // Destructuring is handled by callers that emit the pattern
                // node in place of the FunctionTypeParam wrapper. This branch
                // is unreachable in correct callers — we keep it as a safe
                // no-op rather than panicking so the schema layout never
                // shifts unexpectedly.
                self.write_null_node();
                self.write_null_node();
                self.write_bool(false);
            }
        }
    }

    fn serialize_function_type(&mut self, loc: &Loc, func: &ast::types::Function<Loc, Loc>) {
        // FunctionTypeAnnotation — params this returnType rest typeParameters
        // HookTypeAnnotation     — params returnType rest typeParameters    (no `this` slot)
        let is_hook = func.effect == ast::function::Effect::Hook;
        let kind = if is_hook {
            NodeKind::HookTypeAnnotation
        } else {
            NodeKind::FunctionTypeAnnotation
        };
        let hook_loc;
        let loc = if is_hook {
            hook_loc = Loc {
                start: func.params.loc.start,
                ..loc.clone()
            };
            &hook_loc
        } else {
            loc
        };
        self.write_node_header(kind, loc);
        // params NodeList
        self.buf.push(func.params.params.len() as u32);
        for param in func.params.params.iter() {
            self.serialize_function_type_param(param);
        }
        // this — omitted for HookTypeAnnotation
        if !is_hook {
            match &func.params.this {
                Some(this) => {
                    // FunctionTypeParam node: name(null) typeAnnotation optional(false)
                    self.write_node_header(NodeKind::FunctionTypeParam, &this.loc);
                    self.write_null_node();
                    self.serialize_type(&this.annot.annotation);
                    self.write_bool(false);
                }
                None => self.write_null_node(),
            }
        }
        // returnType
        match &func.return_ {
            ast::types::function::ReturnAnnotation::Available(annot) => {
                self.serialize_type(&annot.annotation);
            }
            ast::types::function::ReturnAnnotation::TypeGuard(tg) => {
                self.serialize_type_guard(tg);
            }
            ast::types::function::ReturnAnnotation::Missing(_) => {
                self.write_null_node();
            }
        }
        // rest
        match &func.params.rest {
            Some(rest) => {
                // Discard the outer loc (which includes `...`) and emit the
                // inner argument; `serialize_function_type_param` unwraps
                // Destructuring into a pattern node directly.
                self.serialize_function_type_param(&rest.argument);
            }
            None => self.write_null_node(),
        }
        // typeParameters
        self.serialize_type_params_opt(&func.tparams);
    }

    fn serialize_component_type(&mut self, loc: &Loc, comp: &ast::types::Component<Loc, Loc>) {
        // 163: ComponentTypeAnnotation — params rest rendersType typeParameters
        self.write_node_header(NodeKind::ComponentTypeAnnotation, loc);
        // params NodeList
        self.buf.push(comp.params.params.len() as u32);
        for param in comp.params.params.iter() {
            // 164: ComponentTypeParameter — name typeAnnotation optional
            // Pass the inner Type to `serialize_type`, not the Annotation
            // wrapper.
            self.write_node_header(NodeKind::ComponentTypeParameter, &param.loc);
            self.serialize_component_param_name(&param.name);
            self.serialize_type(&param.annot.annotation);
            self.write_bool(param.optional);
        }
        // rest
        match &comp.params.rest {
            Some(rest) => {
                // ComponentTypeParameter for rest
                self.write_node_header(NodeKind::ComponentTypeParameter, &rest.loc);
                match &rest.argument {
                    Some(id) => self.serialize_identifier_node(id),
                    None => self.write_null_node(),
                }
                self.serialize_type(&rest.annot);
                self.write_bool(rest.optional);
            }
            None => self.write_null_node(),
        }
        // rendersType
        self.serialize_renders_annotation(&comp.renders);
        // typeParameters
        self.serialize_type_params_opt(&comp.tparams);
    }

    fn serialize_tuple_type(&mut self, loc: &Loc, tuple: &ast::types::Tuple<Loc, Loc>) {
        // 144: TupleTypeAnnotation — elementTypes(NodeList) inexact(Bool)
        self.write_node_header(
            if self.is_babel() {
                NodeKind::BabelTupleTypeAnnotation
            } else {
                NodeKind::TupleTypeAnnotation
            },
            loc,
        );
        self.buf.push(tuple.elements.len() as u32);
        for elem in tuple.elements.iter() {
            match elem {
                ast::types::tuple::Element::UnlabeledElement {
                    loc,
                    annot,
                    optional,
                } => {
                    if *optional {
                        // 227: TupleTypeElement — elementType(Node) optional(Bool)
                        self.write_node_header(NodeKind::TupleTypeElement, loc);
                        self.serialize_type(annot);
                        self.write_bool(true);
                    } else {
                        // Non-optional unlabeled elements are serialized directly as type nodes
                        self.serialize_type(annot);
                    }
                }
                ast::types::tuple::Element::LabeledElement { loc, element } => {
                    // 145: TupleTypeLabeledElement — label elementType variance optional
                    self.write_node_header(NodeKind::TupleTypeLabeledElement, loc);
                    self.serialize_identifier_node(&element.name);
                    self.serialize_type(&element.annot);
                    match &element.variance {
                        Some(v) => self.serialize_variance(v),
                        None => self.write_null_node(),
                    }
                    self.write_bool(element.optional);
                }
                ast::types::tuple::Element::SpreadElement { loc, element } => {
                    // 146: TupleTypeSpreadElement — label typeAnnotation
                    self.write_node_header(NodeKind::TupleTypeSpreadElement, loc);
                    match &element.name {
                        Some(id) => self.serialize_identifier_node(id),
                        None => self.write_null_node(),
                    }
                    self.serialize_type(&element.annot);
                }
            }
        }
        if !self.is_babel() {
            self.write_bool(tuple.inexact);
        }
    }

    fn serialize_typeof_target(&mut self, target: &ast::types::typeof_::Target<Loc, Loc>) {
        match target {
            ast::types::typeof_::Target::Unqualified(ident) => {
                self.serialize_identifier_node(ident);
            }
            ast::types::typeof_::Target::Qualified(q) => {
                // 141: QualifiedTypeofIdentifier — qualification(Node) id(Node)
                self.write_node_header(NodeKind::QualifiedTypeofIdentifier, &q.loc);
                self.serialize_typeof_target(&q.qualification);
                self.serialize_identifier_node(&q.id);
            }
            ast::types::typeof_::Target::Import(imp) => {
                // 143: ImportType — source(Node)
                self.write_node_header(NodeKind::ImportType, &imp.loc);
                self.write_string_literal(&imp.source.0, &imp.source.1.value, &imp.source.1.raw);
            }
        }
    }

    fn serialize_babel_typeof(&mut self, loc: &Loc, typeof_: &ast::types::Typeof<Loc, Loc>) {
        self.write_node_header(NodeKind::BabelTypeofTypeAnnotation, loc);
        self.write_type_use_node_header(NodeKind::GenericTypeAnnotation, typeof_.argument.loc());
        self.serialize_babel_typeof_identifier(&typeof_.argument);
        self.write_null_node();
    }

    fn serialize_babel_typeof_identifier(
        &mut self,
        target: &ast::types::typeof_::Target<Loc, Loc>,
    ) {
        match target {
            ast::types::typeof_::Target::Unqualified(identifier) => {
                self.serialize_identifier_node(identifier);
            }
            ast::types::typeof_::Target::Qualified(qualified) => {
                self.write_node_header(NodeKind::QualifiedTypeIdentifier, &qualified.loc);
                self.serialize_babel_typeof_identifier(&qualified.qualification);
                self.serialize_identifier_node(&qualified.id);
            }
            ast::types::typeof_::Target::Import(import) => {
                self.write_node_header(NodeKind::ImportType, &import.loc);
                self.write_string_literal(
                    &import.source.0,
                    &import.source.1.value,
                    &import.source.1.raw,
                );
            }
        }
    }

    fn serialize_type_guard(&mut self, guard: &ast::types::TypeGuard<Loc, Loc>) {
        // 155: TypePredicate — parameterName typeAnnotation kind
        self.write_node_header(NodeKind::TypePredicate, &guard.loc);
        self.serialize_identifier_node(&guard.guard.0);
        match &guard.guard.1 {
            Some(ty) => self.serialize_type(ty),
            None => self.write_null_node(),
        }
        let kind_str = match guard.kind {
            ast::types::TypeGuardKind::Default => None,
            ast::types::TypeGuardKind::Implies => Some("implies"),
            ast::types::TypeGuardKind::Asserts => Some("asserts"),
        };
        self.write_str_opt(kind_str);
    }

    fn serialize_renders_annotation(
        &mut self,
        renders: &ast::types::ComponentRendersAnnotation<Loc, Loc>,
    ) {
        // Every variant emits ESTree TypeOperator with
        // operator: "renders" | "renders?" | "renders*" and
        // typeAnnotation: <argument>, spanning the outer renders span (from
        // the `renders` keyword through the operand). The argument's own loc
        // is not used.
        match renders {
            ast::types::ComponentRendersAnnotation::AvailableRenders(loc, r) => {
                let operator = match r.variant {
                    ast::types::RendersVariant::Normal => "renders",
                    ast::types::RendersVariant::Maybe => "renders?",
                    ast::types::RendersVariant::Star => "renders*",
                };
                self.write_node_header(NodeKind::TypeOperator, loc);
                self.write_str(operator);
                self.serialize_type(&r.argument);
            }
            ast::types::ComponentRendersAnnotation::MissingRenders(_) => {
                self.write_null_node();
            }
        }
    }

    fn serialize_component_param_name(
        &mut self,
        name: &ast::statement::component_params::ParamName<Loc, Loc>,
    ) {
        match name {
            ast::statement::component_params::ParamName::Identifier(id) => {
                self.serialize_identifier_node(id);
            }
            ast::statement::component_params::ParamName::StringLiteral((lit_loc, lit)) => {
                self.write_string_literal(lit_loc, &lit.value, &lit.raw);
            }
        }
    }

    fn serialize_component_declaration_param(
        &mut self,
        param: &ast::statement::component_params::Param<Loc, Loc>,
    ) {
        // 97: ComponentParameter — name local shorthand
        self.write_node_header(NodeKind::ComponentParameter, &param.loc);
        self.serialize_component_param_name(&param.name);
        // local: pattern, potentially wrapped with AssignmentPattern for default
        match &param.default {
            Some(def) => {
                let ap_loc = Loc::between(param.local.loc(), def.loc());
                self.write_node_header(NodeKind::AssignmentPattern, &ap_loc);
                self.serialize_pattern(&param.local);
                self.serialize_expression(def);
            }
            None => {
                self.serialize_pattern(&param.local);
            }
        }
        self.write_bool(param.shorthand);
    }

    fn serialize_enum_declaration(
        &mut self,
        loc: &Loc,
        decl: &ast::statement::EnumDeclaration<Loc, Loc>,
    ) {
        // 25: EnumDeclaration — id(Node) body(Node)
        self.write_node_header(NodeKind::EnumDeclaration, loc);
        self.serialize_identifier_node(&decl.id);
        self.serialize_enum_body(&decl.body);
        self.write_bool(decl.const_);
    }

    fn serialize_enum_body(&mut self, body: &ast::statement::enum_declaration::Body<Loc>) {
        use ast::statement::enum_declaration::Member;
        let explicit_type = body.explicit_type.as_ref().map(|(_, t)| t.as_str());
        let body_kind = if self.is_babel() {
            match explicit_type {
                Some("boolean") => NodeKind::BabelEnumBooleanBody,
                Some("number") => NodeKind::BabelEnumNumberBody,
                Some("bigint") => NodeKind::BabelEnumBigIntBody,
                Some("symbol") => NodeKind::BabelEnumSymbolBody,
                Some("string") | None => match body.members.first() {
                    Some(Member::BooleanMember(_)) => NodeKind::BabelEnumBooleanBody,
                    Some(Member::NumberMember(_)) => NodeKind::BabelEnumNumberBody,
                    Some(Member::BigIntMember(_)) => NodeKind::BabelEnumBigIntBody,
                    Some(Member::StringMember(_)) | Some(Member::DefaultedMember(_)) | None => {
                        NodeKind::BabelEnumStringBody
                    }
                },
                Some(_) => NodeKind::BabelEnumStringBody,
            }
        } else {
            NodeKind::ESTreeEnumBody
        };
        self.write_node_header(body_kind, &body.loc);
        self.buf.push(body.members.len() as u32);
        for member in body.members.iter() {
            match member {
                Member::BooleanMember(m) => {
                    // EnumBooleanMember — id init
                    self.write_node_header(NodeKind::EnumBooleanMember, &m.loc);
                    self.serialize_enum_member_name(&m.id);
                    let (init_loc, init_lit) = &m.init;
                    self.write_boolean_literal(init_loc, init_lit.value);
                }
                Member::NumberMember(m) => {
                    // EnumNumberMember — id init
                    self.write_node_header(NodeKind::EnumNumberMember, &m.loc);
                    self.serialize_enum_member_name(&m.id);
                    let (init_loc, init_lit) = &m.init;
                    self.write_number_literal(init_loc, init_lit.value, &init_lit.raw);
                }
                Member::StringMember(m) => {
                    // EnumStringMember — id init
                    self.write_node_header(NodeKind::EnumStringMember, &m.loc);
                    self.serialize_enum_member_name(&m.id);
                    let (init_loc, init_lit) = &m.init;
                    self.write_string_literal(init_loc, &init_lit.value, &init_lit.raw);
                }
                Member::BigIntMember(m) => {
                    // EnumBigIntMember — id init
                    self.write_node_header(NodeKind::EnumBigIntMember, &m.loc);
                    self.serialize_enum_member_name(&m.id);
                    let (init_loc, init_lit) = &m.init;
                    self.write_bigint_literal(init_loc, &init_lit.raw);
                }
                Member::DefaultedMember(m) => {
                    // EnumDefaultedMember — id
                    self.write_node_header(NodeKind::EnumDefaultedMember, &m.loc);
                    self.serialize_enum_member_name(&m.id);
                }
            }
        }
        if !self.is_babel() {
            self.write_str_opt(explicit_type);
        } else if body_kind != NodeKind::BabelEnumSymbolBody {
            self.write_bool(explicit_type.is_some());
        }
        self.write_bool(body.has_unknown_members.is_some());
    }

    fn serialize_enum_member_name(
        &mut self,
        name: &ast::statement::enum_declaration::MemberName<Loc>,
    ) {
        use ast::statement::enum_declaration::MemberName;
        match name {
            MemberName::Identifier(ident) => self.serialize_identifier_node(ident),
            MemberName::StringLiteral(loc, sl) => {
                self.write_string_literal(loc, &sl.value, &sl.raw);
            }
        }
    }

    fn serialize_record_declaration(
        &mut self,
        loc: &Loc,
        decl: &ast::statement::RecordDeclaration<Loc, Loc>,
    ) {
        // 28: RecordDeclaration — id typeParameters implements body
        self.write_node_header(NodeKind::RecordDeclaration, loc);
        self.serialize_identifier_node(&decl.id);
        self.serialize_type_params_opt(&decl.tparams);
        // implements NodeList
        match &decl.implements {
            Some(impls) => {
                self.buf.push(impls.interfaces.len() as u32);
                for iface in impls.interfaces.iter() {
                    // 195: RecordDeclarationImplements — id typeArguments
                    self.write_node_header(NodeKind::RecordDeclarationImplements, &iface.loc);
                    self.serialize_generic_type_identifier(&iface.id);
                    self.serialize_type_args_opt(&iface.targs);
                }
            }
            None => self.buf.push(0),
        }
        // 192: RecordDeclarationBody — elements(NodeList)
        self.write_node_header(NodeKind::RecordDeclarationBody, &decl.body.loc);
        self.buf.push(decl.body.body.len() as u32);
        for elem in decl.body.body.iter() {
            match elem {
                ast::statement::record_declaration::BodyElement::Property(p) => {
                    // 193: RecordDeclarationProperty — key typeAnnotation defaultValue
                    self.write_node_header(NodeKind::RecordDeclarationProperty, &p.loc);
                    self.serialize_object_key(&p.key);
                    self.serialize_type_annotation(&p.annot);
                    match &p.default_value {
                        Some(expr) => self.serialize_expression(expr),
                        None => self.write_null_node(),
                    }
                }
                ast::statement::record_declaration::BodyElement::StaticProperty(sp) => {
                    // 194: RecordDeclarationStaticProperty — key typeAnnotation value
                    self.write_node_header(NodeKind::RecordDeclarationStaticProperty, &sp.loc);
                    self.serialize_object_key(&sp.key);
                    self.serialize_type_annotation(&sp.annot);
                    self.serialize_expression(&sp.value);
                }
                ast::statement::record_declaration::BodyElement::Method(m) => {
                    // Method within record body — serialize as MethodDefinition
                    self.write_node_header(NodeKind::MethodDefinition, &m.loc);
                    self.serialize_object_key(&m.key);
                    let (fn_loc, func) = &m.value;
                    self.serialize_function_expr(fn_loc, func, NodeKind::FunctionExpression);
                    self.write_str(method_kind_str(m.kind));
                    self.write_bool(m.static_);
                    self.write_bool(is_key_computed(&m.key));
                    self.buf.push(m.decorators.len() as u32);
                    for dec in m.decorators.iter() {
                        self.write_node_header(NodeKind::Decorator, &dec.loc);
                        self.serialize_expression(&dec.expression);
                    }
                    self.write_bool(m.override_);
                    self.write_str_opt(
                        m.ts_accessibility
                            .as_ref()
                            .map(|a| ts_accessibility_str(a.kind)),
                    );
                }
            }
        }
    }

    fn serialize_declare_export_declaration(
        &mut self,
        loc: &Loc,
        decl: &ast::statement::DeclareExportDeclaration<Loc, Loc>,
    ) {
        // Check if this is an export-all declaration (has no declaration, specifiers
        // are ExportBatchSpecifier with no specifier name)
        let is_export_all = decl.declaration.is_none()
            && decl.specifiers.as_ref().is_some_and(|s| {
                matches!(
                    s,
                    ast::statement::export_named_declaration::Specifier::ExportBatchSpecifier(
                        batch
                    ) if batch.specifier.is_none()
                )
            })
            && decl.source.is_some();

        if is_export_all {
            // 106: DeclareExportAllDeclaration — source(Node) implicitDeclare(TrueBoolean)
            self.write_node_header(NodeKind::DeclareExportAllDeclaration, loc);
            if let Some((src_loc, src_lit)) = &decl.source {
                self.write_string_literal(src_loc, &src_lit.value, &src_lit.raw);
            }
            self.write_bool(self.implicit_declare_from_loc(loc));
            return;
        }

        // 105: DeclareExportDeclaration — default declaration specifiers source
        // implicitDeclare(TrueBoolean)
        self.write_node_header(NodeKind::DeclareExportDeclaration, loc);
        self.write_bool(decl.default.is_some());
        // declaration
        match &decl.declaration {
            Some(d) => {
                use ast::statement::declare_export_declaration::Declaration;
                match d {
                    Declaration::Variable { loc, declaration } => {
                        self.serialize_declare_variable(loc, declaration);
                    }
                    Declaration::Function { loc, declaration } => {
                        // Defer entirely to serialize_declare_function, which
                        // emits the upstream DeclareFunction/DeclareHook shape.
                        self.serialize_declare_function(loc, declaration);
                    }
                    Declaration::Class { loc, declaration } => {
                        self.serialize_declare_class(loc, declaration);
                    }
                    Declaration::Component { loc, declaration } => {
                        self.serialize_declare_component(loc, declaration);
                    }
                    Declaration::DefaultType { type_ } => {
                        self.serialize_type(type_);
                    }
                    Declaration::NamedType { loc, declaration } => {
                        self.write_node_header(NodeKind::TypeAlias, loc);
                        self.serialize_identifier_node(&declaration.id);
                        self.serialize_type_params_opt(&declaration.tparams);
                        self.serialize_type(&declaration.right);
                    }
                    Declaration::NamedOpaqueType { loc, declaration } => {
                        // Emit DeclareOpaqueType with the full payload, not a
                        // TypeAlias stub.
                        self.serialize_declare_opaque_type(loc, declaration);
                    }
                    Declaration::Interface { loc, declaration } => {
                        self.write_node_header(NodeKind::InterfaceDeclaration, loc);
                        self.serialize_identifier_node(&declaration.id);
                        self.serialize_type_params_opt(&declaration.tparams);
                        self.buf.push(declaration.extends.len() as u32);
                        for (ext_loc, ext) in declaration.extends.iter() {
                            self.serialize_interface_extends(ext_loc, ext);
                        }
                        self.serialize_type_object(&declaration.body.0, &declaration.body.1);
                    }
                    Declaration::Enum { loc, declaration } => {
                        // Emit DeclareEnum, not the value-statement
                        // EnumDeclaration.
                        self.write_node_header(NodeKind::DeclareEnum, loc);
                        self.serialize_identifier_node(&declaration.id);
                        self.serialize_enum_body(&declaration.body);
                        self.write_bool(self.implicit_declare_from_loc(loc));
                        self.write_bool(declaration.const_);
                    }
                    Declaration::Namespace { loc, declaration } => {
                        self.serialize_declare_namespace(loc, declaration);
                    }
                }
            }
            None => self.write_null_node(),
        }
        // specifiers NodeList
        match &decl.specifiers {
            Some(ast::statement::export_named_declaration::Specifier::ExportSpecifiers(specs)) => {
                self.buf.push(specs.len() as u32);
                for spec in specs.iter() {
                    // 39: ExportSpecifier — exported local exportKind
                    self.write_node_header(NodeKind::ExportSpecifier, &spec.loc);
                    match &spec.exported {
                        Some(exported) => self.serialize_identifier_node(exported),
                        None => self.serialize_identifier_node(&spec.local),
                    }
                    self.serialize_identifier_node(&spec.local);
                    self.write_str(export_kind_str(spec.export_kind));
                }
            }
            Some(ast::statement::export_named_declaration::Specifier::ExportBatchSpecifier(
                batch,
            )) => {
                if let Some(spec) = &batch.specifier {
                    // 40: ExportNamespaceSpecifier — exported
                    self.buf.push(1);
                    self.write_node_header(NodeKind::ExportNamespaceSpecifier, &batch.loc);
                    self.serialize_identifier_node(spec);
                } else {
                    self.buf.push(0);
                }
            }
            None => self.buf.push(0),
        }
        // source
        match &decl.source {
            Some((src_loc, src_lit)) => {
                self.write_string_literal(src_loc, &src_lit.value, &src_lit.raw);
            }
            None => self.write_null_node(),
        }
        self.write_bool(self.implicit_declare_from_loc(loc));
    }

    // ---------------------------------------------------------------
    // Delegate methods for generated dispatch
    // ---------------------------------------------------------------

    fn serialize_component_declaration(
        &mut self,
        loc: &Loc,
        inner: &ast::statement::ComponentDeclaration<Loc, Loc>,
    ) {
        // Dispatch on body presence:
        //   None    -> ESTree "DeclareComponent"
        //   Some(_) -> ESTree "ComponentDeclaration"
        // NodeKind::DeclareComponentAmbient (228) is used for body=None so the
        // JS deserializer emits type "DeclareComponent".
        // The explicit `Statement::DeclareComponent` form has a different
        // SHORT shape and goes through `serialize_declare_component` below
        // (NodeKind::DeclareComponent, 101).
        let (kind, implicit_declare) = match &inner.body {
            None => (NodeKind::DeclareComponentAmbient, true),
            Some(_) => (NodeKind::ComponentDeclaration, false),
        };
        self.write_node_header(kind, loc);
        match &inner.body {
            Some((body_loc, body)) => {
                self.write_node_header(NodeKind::BlockStatement, body_loc);
                self.buf.push(body.body.len() as u32);
                for stmt in body.body.iter() {
                    self.serialize_statement(stmt);
                }
            }
            None => self.write_null_node(),
        }
        self.serialize_identifier_node(&inner.id);
        self.write_bool(implicit_declare);
        // params NodeList. Both body=Some (ComponentDeclaration) and body=None
        // (DeclareComponentAmbient) use value-position ComponentParameter(97)
        // with rest appended as RestElement(80), matching OCaml parser output.
        if implicit_declare {
            let rest_count = if inner.params.rest.is_some() { 1 } else { 0 };
            self.buf
                .push((inner.params.params.len() + rest_count) as u32);
            for param in inner.params.params.iter() {
                self.serialize_component_declaration_param(param);
            }
            if let Some(rest) = &inner.params.rest {
                self.serialize_rest_element(&rest.loc, &rest.argument);
            }
            // rest: always null (rest is in params list above)
            self.write_null_node();
        } else {
            let rest_count = if inner.params.rest.is_some() { 1 } else { 0 };
            self.buf
                .push((inner.params.params.len() + rest_count) as u32);
            for param in inner.params.params.iter() {
                self.serialize_component_declaration_param(param);
            }
            if let Some(rest) = &inner.params.rest {
                self.serialize_rest_element(&rest.loc, &rest.argument);
            }
        }
        // rendersType
        self.serialize_renders_annotation(&inner.renders);
        // typeParameters
        self.serialize_type_params_opt(&inner.tparams);
        // async
        self.write_bool(inner.async_);
    }

    fn serialize_interface_declaration(
        &mut self,
        loc: &Loc,
        inner: &ast::statement::Interface<Loc, Loc>,
    ) {
        // 26: InterfaceDeclaration — id typeParameters extends body
        self.write_node_header(NodeKind::InterfaceDeclaration, loc);
        self.serialize_identifier_node(&inner.id);
        self.serialize_type_params_opt(&inner.tparams);
        self.buf.push(inner.extends.len() as u32);
        for (ext_loc, ext) in inner.extends.iter() {
            self.serialize_interface_extends(ext_loc, ext);
        }
        self.serialize_type_object(&inner.body.0, &inner.body.1);
    }

    fn serialize_opaque_type(&mut self, loc: &Loc, inner: &ast::statement::OpaqueType<Loc, Loc>) {
        // OpaqueType — id typeParameters impltype lowerBound upperBound supertype
        self.write_node_header(
            if self.is_babel() {
                NodeKind::BabelOpaqueType
            } else {
                NodeKind::OpaqueType
            },
            loc,
        );
        self.serialize_opaque_type_payload(inner);
    }

    fn serialize_opaque_type_payload(&mut self, inner: &ast::statement::OpaqueType<Loc, Loc>) {
        self.serialize_identifier_node(&inner.id);
        self.serialize_type_params_opt(&inner.tparams);
        match &inner.impl_type {
            Some(ty) => self.serialize_type(ty),
            None => self.write_null_node(),
        }
        if !self.is_babel() {
            match &inner.lower_bound {
                Some(ty) => self.serialize_type(ty),
                None => self.write_null_node(),
            }
            match &inner.upper_bound {
                Some(ty) => self.serialize_type(ty),
                None => self.write_null_node(),
            }
        }
        match &inner.legacy_upper_bound {
            Some(ty) => self.serialize_type(ty),
            None => self.write_null_node(),
        }
    }

    fn serialize_declare_variable(
        &mut self,
        loc: &Loc,
        inner: &ast::statement::DeclareVariable<Loc, Loc>,
    ) {
        if self.is_babel() {
            self.write_node_header(NodeKind::BabelDeclareVariable, loc);
            match inner.declarations.first() {
                Some(declaration) => self.serialize_pattern(&declaration.id),
                None => self.write_null_node(),
            }
            return;
        }
        // 98: DeclareVariable — declarations(NodeList) kind(String) implicitDeclare(TrueBoolean)
        self.write_node_header(NodeKind::DeclareVariable, loc);
        self.serialize_variable_declarator_list(&inner.declarations);
        self.write_str(inner.kind.as_str());
        self.write_bool(self.implicit_declare_from_loc(loc));
    }

    fn serialize_declare_function(
        &mut self,
        loc: &Loc,
        inner: &ast::statement::DeclareFunction<Loc, Loc>,
    ) {
        // Discriminate on the annotation's effect: a hook annotation produces
        // DeclareHook; any other annotation produces DeclareFunction.
        use std::ops::Deref;
        let is_hook = matches!(
            inner.annot.annotation.deref(),
            ast::types::TypeInner::Function { inner: f, .. }
                if f.effect == ast::function::Effect::Hook
        );
        if is_hook {
            // 102: DeclareHook — id(Node) implicitDeclare(TrueBoolean)
            self.write_node_header(NodeKind::DeclareHook, loc);
            self.serialize_typed_identifier_opt(&inner.id, &inner.annot);
            self.write_bool(inner.implicit_declare);
        } else {
            // 99: DeclareFunction — id(Node) typeAnnotation(Node)
            // implicitDeclare(TrueBoolean) predicate(Node)
            self.write_node_header(NodeKind::DeclareFunction, loc);
            self.serialize_typed_identifier_opt(&inner.id, &inner.annot);
            if inner.id.is_none() {
                self.serialize_type_annotation(&inner.annot);
            } else {
                self.write_null_node();
            }
            self.write_bool(inner.implicit_declare);
            match &inner.predicate {
                Some(pred) => self.serialize_predicate(pred),
                None => self.write_null_node(),
            }
        }
    }

    fn serialize_declare_class(
        &mut self,
        loc: &Loc,
        inner: &ast::statement::DeclareClass<Loc, Loc>,
    ) {
        // 100: DeclareClass — id typeParameters extends implements mixins body
        // implicitDeclare(TrueBoolean) abstract(TrueBoolean)
        self.write_node_header(NodeKind::DeclareClass, loc);
        self.serialize_identifier_node(&inner.id);
        self.serialize_type_params_opt(&inner.tparams);
        // extends NodeList
        match &inner.extends {
            Some((ext_loc, ext)) => {
                self.buf.push(1);
                self.serialize_declare_class_extends(ext_loc, ext);
            }
            None => self.buf.push(0),
        }
        // implements NodeList
        match &inner.implements {
            Some(impls) => {
                self.buf.push(impls.interfaces.len() as u32);
                for iface in impls.interfaces.iter() {
                    // 86: ClassImplements — id typeArguments
                    self.write_type_use_node_header(NodeKind::ClassImplements, &iface.loc);
                    self.serialize_generic_type_identifier(&iface.id);
                    self.serialize_type_args_opt(&iface.targs);
                }
            }
            None => self.buf.push(0),
        }
        // mixins NodeList
        self.buf.push(inner.mixins.len() as u32);
        for (mixin_loc, mixin) in inner.mixins.iter() {
            self.serialize_interface_extends(mixin_loc, mixin);
        }
        self.serialize_type_object(&inner.body.0, &inner.body.1);
        self.write_bool(self.implicit_declare_from_loc(loc));
        self.write_bool(inner.abstract_);
    }

    fn serialize_declare_component(
        &mut self,
        loc: &Loc,
        inner: &ast::statement::DeclareComponent<Loc, Loc>,
    ) {
        // 101: DeclareComponent — id params rest rendersType typeParameters
        // implicitDeclare(TrueBoolean).
        // The OCaml parser emits value-position ComponentParameter for declare
        // component params (same shape as ComponentDeclaration), with rest
        // appended as RestElement in the params list.
        self.write_node_header(NodeKind::DeclareComponent, loc);
        self.serialize_identifier_node(&inner.id);
        let rest_count = if inner.params.rest.is_some() { 1 } else { 0 };
        self.buf
            .push((inner.params.params.len() + rest_count) as u32);
        for param in inner.params.params.iter() {
            self.serialize_component_declaration_param(param);
        }
        if let Some(rest) = &inner.params.rest {
            self.serialize_rest_element(&rest.loc, &rest.argument);
        }
        // rest: always null (rest is in params list above)
        self.write_null_node();
        // rendersType
        self.serialize_renders_annotation(&inner.renders);
        // typeParameters
        self.serialize_type_params_opt(&inner.tparams);
        self.write_bool(self.implicit_declare_from_loc(loc));
    }

    fn serialize_declare_module(
        &mut self,
        loc: &Loc,
        inner: &ast::statement::DeclareModule<Loc, Loc>,
    ) {
        // 103: DeclareModule — id(Node) body(Node)
        self.write_node_header(NodeKind::DeclareModule, loc);
        match &inner.id {
            ast::statement::declare_module::Id::Identifier(id) => {
                self.serialize_identifier_node(id);
            }
            ast::statement::declare_module::Id::Literal((lit_loc, lit)) => {
                self.write_string_literal(lit_loc, &lit.value, &lit.raw);
            }
        }
        self.serialize_block_statement(&inner.body.0, &inner.body.1);
    }

    fn serialize_declare_namespace(
        &mut self,
        loc: &Loc,
        inner: &ast::statement::DeclareNamespace<Loc, Loc>,
    ) {
        // 107: DeclareNamespace — global(Boolean) id(Node) body(Node)
        // implicitDeclare(TrueBoolean) keyword(String)
        self.write_node_header(NodeKind::DeclareNamespace, loc);
        self.write_bool(matches!(
            &inner.id,
            ast::statement::declare_namespace::Id::Global(_)
        ));
        match &inner.id {
            ast::statement::declare_namespace::Id::Global(id) => {
                self.serialize_identifier_node(id);
            }
            ast::statement::declare_namespace::Id::Local(id) => {
                self.serialize_identifier_node(id);
            }
        }
        self.serialize_block_statement(&inner.body.0, &inner.body.1);
        self.write_bool(inner.implicit_declare);
        self.write_str(match inner.keyword {
            ast::statement::declare_namespace::Keyword::Namespace => "namespace",
            ast::statement::declare_namespace::Keyword::Module => "module",
        });
    }

    fn serialize_declare_interface(
        &mut self,
        loc: &Loc,
        inner: &ast::statement::Interface<Loc, Loc>,
    ) {
        // 108: DeclareInterface — id typeParameters body extends implicitDeclare(TrueBoolean)
        self.write_node_header(NodeKind::DeclareInterface, loc);
        self.serialize_identifier_node(&inner.id);
        self.serialize_type_params_opt(&inner.tparams);
        self.serialize_type_object(&inner.body.0, &inner.body.1);
        self.buf.push(inner.extends.len() as u32);
        for (ext_loc, ext) in inner.extends.iter() {
            self.serialize_interface_extends(ext_loc, ext);
        }
        self.write_bool(self.implicit_declare_from_loc(loc));
    }

    fn serialize_declare_opaque_type(
        &mut self,
        loc: &Loc,
        inner: &ast::statement::OpaqueType<Loc, Loc>,
    ) {
        // DeclareOpaqueType — id typeParameters impltype lowerBound upperBound
        // supertype implicitDeclare(TrueBoolean)
        self.write_node_header(
            if self.is_babel() {
                NodeKind::BabelDeclareOpaqueType
            } else {
                NodeKind::DeclareOpaqueType
            },
            loc,
        );
        self.serialize_opaque_type_payload(inner);
        self.write_bool(self.implicit_declare_from_loc(loc));
    }

    fn serialize_array_expression(&mut self, loc: &Loc, inner: &ast::expression::Array<Loc, Loc>) {
        // 44: ArrayExpression — elements(NodeList) trailingComma(Bool)
        self.write_node_header(
            if self.is_babel() {
                NodeKind::BabelArrayExpression
            } else {
                NodeKind::ArrayExpression
            },
            loc,
        );
        self.buf.push(inner.elements.len() as u32);
        for elem in inner.elements.iter() {
            self.serialize_array_element(elem);
        }
        if !self.is_babel() {
            self.write_bool(inner.trailing_comma);
        }
    }

    fn serialize_object_expression(
        &mut self,
        loc: &Loc,
        inner: &ast::expression::Object<Loc, Loc>,
    ) {
        // 45: ObjectExpression — properties(NodeList)
        self.write_node_header(NodeKind::ObjectExpression, loc);
        self.buf.push(inner.properties.len() as u32);
        for prop in inner.properties.iter() {
            self.serialize_object_property(prop);
        }
    }

    fn serialize_sequence_expression(
        &mut self,
        loc: &Loc,
        inner: &ast::expression::Sequence<Loc, Loc>,
    ) {
        // 48: SequenceExpression — expressions(NodeList)
        self.write_node_header(NodeKind::SequenceExpression, loc);
        self.buf.push(inner.expressions.len() as u32);
        for expr in inner.expressions.iter() {
            self.serialize_expression(expr);
        }
    }

    fn serialize_unary_dispatch(&mut self, loc: &Loc, inner: &ast::expression::Unary<Loc, Loc>) {
        match inner.operator {
            ast::expression::UnaryOperator::Await => {
                // 61: AwaitExpression — argument(Node)
                self.write_node_header(NodeKind::AwaitExpression, loc);
                self.serialize_expression(&inner.argument);
            }
            ast::expression::UnaryOperator::Nonnull => {
                // 71: NonNullExpression — expression(Node)
                self.write_node_header(NodeKind::NonNullExpression, loc);
                self.serialize_expression(&inner.argument);
            }
            _ => {
                // 49: UnaryExpression — operator(String) argument(Node) prefix(Bool)
                self.write_node_header(NodeKind::UnaryExpression, loc);
                self.write_str(unary_operator_str(inner.operator));
                self.serialize_expression(&inner.argument);
                self.write_bool(true);
            }
        }
    }

    fn serialize_match_expression(
        &mut self,
        loc: &Loc,
        inner: &ast::match_::Match<Loc, Loc, ast::expression::Expression<Loc, Loc>>,
    ) {
        // 72: MatchExpression — argument(Node) cases(NodeList)
        self.write_node_header(NodeKind::MatchExpression, loc);
        self.serialize_expression(&inner.arg);
        self.buf.push(inner.cases.len() as u32);
        for case in inner.cases.iter() {
            self.serialize_match_expression_case(case);
        }
    }

    fn serialize_record_expression(
        &mut self,
        loc: &Loc,
        inner: &ast::expression::Record<Loc, Loc>,
    ) {
        // 73: RecordExpression — recordConstructor typeArguments properties
        self.write_node_header(NodeKind::RecordExpression, loc);
        self.serialize_expression(&inner.constructor);
        self.serialize_call_type_args_opt(&inner.targs);
        // 74: RecordExpressionProperties — properties(NodeList)
        self.write_node_header(NodeKind::RecordExpressionProperties, &inner.properties.0);
        self.buf.push(inner.properties.1.properties.len() as u32);
        for prop in inner.properties.1.properties.iter() {
            self.serialize_object_property(prop);
        }
    }

    fn serialize_union_type(&mut self, loc: &Loc, inner: &ast::types::Union<Loc, Loc>) {
        // 127: UnionTypeAnnotation — types(NodeList)
        self.write_node_header(NodeKind::UnionTypeAnnotation, loc);
        let count = 2 + inner.types.2.len();
        self.buf.push(count as u32);
        self.serialize_type(&inner.types.0);
        self.serialize_type(&inner.types.1);
        for ty in inner.types.2.iter() {
            self.serialize_type(ty);
        }
    }

    fn serialize_intersection_type(
        &mut self,
        loc: &Loc,
        inner: &ast::types::Intersection<Loc, Loc>,
    ) {
        // 128: IntersectionTypeAnnotation — types(NodeList)
        self.write_node_header(NodeKind::IntersectionTypeAnnotation, loc);
        let count = 2 + inner.types.2.len();
        self.buf.push(count as u32);
        self.serialize_type(&inner.types.0);
        self.serialize_type(&inner.types.1);
        for ty in inner.types.2.iter() {
            self.serialize_type(ty);
        }
    }

    fn serialize_interface_type(&mut self, loc: &Loc, inner: &ast::types::Interface<Loc, Loc>) {
        // 159: InterfaceTypeAnnotation — extends(NodeList) body(Node)
        self.write_node_header(NodeKind::InterfaceTypeAnnotation, loc);
        self.buf.push(inner.extends.len() as u32);
        for (ext_loc, ext) in inner.extends.iter() {
            self.serialize_interface_extends(ext_loc, ext);
        }
        self.serialize_type_object(&inner.body.0, &inner.body.1);
    }

    fn serialize_renders_type_dispatch(
        &mut self,
        loc: &Loc,
        inner: &ast::types::Renders<Loc, Loc>,
    ) {
        // Every variant emits TypeOperator with
        // operator: "renders" | "renders?" | "renders*" and
        // typeAnnotation: <argument>; same as `serialize_renders_annotation`.
        let operator = match inner.variant {
            ast::types::RendersVariant::Normal => "renders",
            ast::types::RendersVariant::Maybe => "renders?",
            ast::types::RendersVariant::Star => "renders*",
        };
        self.write_node_header(NodeKind::TypeOperator, loc);
        self.write_str(operator);
        self.serialize_type(&inner.argument);
    }

    fn serialize_template_literal_type(
        &mut self,
        loc: &Loc,
        inner: &ast::types::TypeTemplateLiteral<Loc, Loc>,
    ) {
        // 218: TemplateLiteralTypeAnnotation — quasis(NodeList) types(NodeList)
        self.write_node_header(NodeKind::TemplateLiteralTypeAnnotation, loc);
        self.buf.push(inner.quasis.len() as u32);
        for quasi in inner.quasis.iter() {
            // Reuse TemplateElement node kind (66)
            let quasi_loc = self.template_element_loc(&quasi.loc, quasi.tail);
            self.write_node_header(NodeKind::TemplateElement, &quasi_loc);
            self.write_bool(quasi.tail);
            self.write_template_cooked_value(&quasi.value.raw, &quasi.value.cooked);
            self.write_str(&quasi.value.raw);
        }
        self.buf.push(inner.types.len() as u32);
        for ty in inner.types.iter() {
            self.serialize_type(ty);
        }
    }
}

fn string_literal_value_bytes<'a>(value: &'a str, raw: &str) -> Cow<'a, [u8]> {
    if raw.as_bytes().contains(&b'\\')
        && let Some(bytes) = decode_quoted_string_raw_bytes(raw)
    {
        return Cow::Owned(bytes);
    }
    Cow::Borrowed(value.as_bytes())
}

fn template_cooked_value_bytes<'a>(raw: &str, cooked: &'a str) -> Cow<'a, [u8]> {
    if raw.as_bytes().contains(&b'\\')
        && let Some(bytes) = decode_escaped_raw_bytes(raw)
    {
        return Cow::Owned(bytes);
    }
    match cooked_template_value(raw, cooked) {
        Cow::Borrowed(cooked) => Cow::Borrowed(cooked.as_bytes()),
        Cow::Owned(cooked) => Cow::Owned(cooked.into_bytes()),
    }
}

fn decode_quoted_string_raw_bytes(raw: &str) -> Option<Vec<u8>> {
    let bytes = raw.as_bytes();
    let quote = *bytes.first()?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    if bytes.len() < 2 || bytes[bytes.len() - 1] != quote {
        return None;
    }
    decode_escaped_raw_bytes(&raw[1..raw.len() - 1])
}

fn decode_escaped_raw_bytes(raw: &str) -> Option<Vec<u8>> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            let ch = raw[i..].chars().next()?;
            push_wtf8_codepoint(&mut out, ch as u32)?;
            i += ch.len_utf8();
            continue;
        }

        i += 1;
        if i >= bytes.len() {
            return None;
        }

        match bytes[i] {
            b'u' => {
                i += 1;
                if i < bytes.len() && bytes[i] == b'{' {
                    i += 1;
                    let mut value = 0u32;
                    let mut digits = 0;
                    while i < bytes.len() && bytes[i] != b'}' {
                        value = value.checked_mul(16)?.checked_add(hex_value(bytes[i])?)?;
                        digits += 1;
                        i += 1;
                    }
                    if digits == 0 || i >= bytes.len() || bytes[i] != b'}' {
                        return None;
                    }
                    i += 1;
                    push_wtf8_codepoint(&mut out, value)?;
                } else {
                    let value = parse_hex_u16(bytes, i)?;
                    i += 4;
                    push_wtf8_codepoint(&mut out, value as u32)?;
                }
            }
            b'x' => {
                i += 1;
                let value = parse_hex_u8(bytes, i)?;
                i += 2;
                push_wtf8_codepoint(&mut out, value as u32)?;
            }
            b'b' => {
                out.push(0x08);
                i += 1;
            }
            b'f' => {
                out.push(0x0c);
                i += 1;
            }
            b'n' => {
                out.push(b'\n');
                i += 1;
            }
            b'r' => {
                out.push(b'\r');
                i += 1;
            }
            b't' => {
                out.push(b'\t');
                i += 1;
            }
            b'v' => {
                out.push(0x0b);
                i += 1;
            }
            b'0'..=b'7' => {
                let (value, len) = decode_octal_escape(bytes, i)?;
                push_wtf8_codepoint(&mut out, value)?;
                i += len;
            }
            b'\r' => {
                i += 1;
                if i < bytes.len() && bytes[i] == b'\n' {
                    i += 1;
                }
            }
            b'\n' => {
                i += 1;
            }
            _ => {
                let ch = raw[i..].chars().next()?;
                push_wtf8_codepoint(&mut out, ch as u32)?;
                i += ch.len_utf8();
            }
        }
    }
    Some(out)
}

fn decode_octal_escape(bytes: &[u8], start: usize) -> Option<(u32, usize)> {
    let first = *bytes.get(start)?;
    if !is_octal_digit(first) {
        return None;
    }
    if first <= b'3'
        && let (Some(second), Some(third)) = (bytes.get(start + 1), bytes.get(start + 2))
        && is_octal_digit(*second)
        && is_octal_digit(*third)
    {
        return Some((octal_value(&bytes[start..start + 3])?, 3));
    }
    if let Some(second) = bytes.get(start + 1)
        && is_octal_digit(*second)
    {
        return Some((octal_value(&bytes[start..start + 2])?, 2));
    }
    Some(((first - b'0') as u32, 1))
}

fn is_octal_digit(b: u8) -> bool {
    (b'0'..=b'7').contains(&b)
}

fn octal_value(bytes: &[u8]) -> Option<u32> {
    let mut value = 0u32;
    for b in bytes {
        if !is_octal_digit(*b) {
            return None;
        }
        value = value.checked_mul(8)?.checked_add((*b - b'0') as u32)?;
    }
    Some(value)
}

fn push_wtf8_codepoint(out: &mut Vec<u8>, value: u32) -> Option<()> {
    match value {
        0x0000..=0x007f => {
            out.push(value as u8);
            Some(())
        }
        0x0080..=0x07ff => {
            out.push((0xc0 | (value >> 6)) as u8);
            out.push((0x80 | (value & 0x3f)) as u8);
            Some(())
        }
        0x0800..=0xffff => {
            out.push((0xe0 | (value >> 12)) as u8);
            out.push((0x80 | ((value >> 6) & 0x3f)) as u8);
            out.push((0x80 | (value & 0x3f)) as u8);
            Some(())
        }
        0x10000..=0x10ffff => {
            out.push((0xf0 | (value >> 18)) as u8);
            out.push((0x80 | ((value >> 12) & 0x3f)) as u8);
            out.push((0x80 | ((value >> 6) & 0x3f)) as u8);
            out.push((0x80 | (value & 0x3f)) as u8);
            Some(())
        }
        _ => None,
    }
}

fn cooked_template_value<'a>(raw: &str, cooked: &'a str) -> Cow<'a, str> {
    if !cooked.contains('\u{FFFD}') || !raw.contains("\\u") {
        return Cow::Borrowed(cooked);
    }
    match decode_template_raw(raw) {
        Some(decoded) => Cow::Owned(decoded),
        None => Cow::Borrowed(cooked),
    }
}

fn decode_template_raw(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            let ch = raw[i..].chars().next()?;
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }

        i += 1;
        if i >= bytes.len() {
            return None;
        }

        match bytes[i] {
            b'u' => {
                i += 1;
                if i < bytes.len() && bytes[i] == b'{' {
                    i += 1;
                    let mut value = 0u32;
                    let mut digits = 0;
                    while i < bytes.len() && bytes[i] != b'}' {
                        value = value.checked_mul(16)?.checked_add(hex_value(bytes[i])?)?;
                        digits += 1;
                        i += 1;
                    }
                    if digits == 0 || i >= bytes.len() || bytes[i] != b'}' {
                        return None;
                    }
                    i += 1;
                    out.push(char::from_u32(value)?);
                } else {
                    let high = parse_hex_u16(bytes, i)?;
                    i += 4;
                    if (0xD800..=0xDBFF).contains(&high) {
                        if i + 6 > bytes.len() || bytes[i] != b'\\' || bytes[i + 1] != b'u' {
                            return None;
                        }
                        let low = parse_hex_u16(bytes, i + 2)?;
                        if !(0xDC00..=0xDFFF).contains(&low) {
                            return None;
                        }
                        i += 6;
                        let codepoint =
                            0x10000 + (((high as u32 - 0xD800) << 10) | (low as u32 - 0xDC00));
                        out.push(char::from_u32(codepoint)?);
                    } else if (0xDC00..=0xDFFF).contains(&high) {
                        return None;
                    } else {
                        out.push(char::from_u32(high as u32)?);
                    }
                }
            }
            b'x' => {
                i += 1;
                let value = parse_hex_u8(bytes, i)?;
                i += 2;
                out.push(value as char);
            }
            b'b' => {
                out.push('\u{0008}');
                i += 1;
            }
            b'f' => {
                out.push('\u{000C}');
                i += 1;
            }
            b'n' => {
                out.push('\n');
                i += 1;
            }
            b'r' => {
                out.push('\r');
                i += 1;
            }
            b't' => {
                out.push('\t');
                i += 1;
            }
            b'v' => {
                out.push('\u{000B}');
                i += 1;
            }
            b'0' => {
                out.push('\0');
                i += 1;
            }
            b'\r' => {
                i += 1;
                if i < bytes.len() && bytes[i] == b'\n' {
                    i += 1;
                }
            }
            b'\n' => {
                i += 1;
            }
            _ => {
                let ch = raw[i..].chars().next()?;
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    Some(out)
}

fn parse_hex_u16(bytes: &[u8], start: usize) -> Option<u16> {
    if start + 4 > bytes.len() {
        return None;
    }
    let mut value = 0u16;
    for b in &bytes[start..start + 4] {
        value = value.checked_mul(16)?.checked_add(hex_value(*b)? as u16)?;
    }
    Some(value)
}

fn parse_hex_u8(bytes: &[u8], start: usize) -> Option<u8> {
    if start + 2 > bytes.len() {
        return None;
    }
    let high = hex_value(bytes[start])?;
    let low = hex_value(bytes[start + 1])?;
    Some((high * 16 + low) as u8)
}

fn hex_value(b: u8) -> Option<u32> {
    match b {
        b'0'..=b'9' => Some((b - b'0') as u32),
        b'a'..=b'f' => Some((b - b'a' + 10) as u32),
        b'A'..=b'F' => Some((b - b'A' + 10) as u32),
        _ => None,
    }
}

fn unary_operator_str(op: ast::expression::UnaryOperator) -> &'static str {
    match op {
        ast::expression::UnaryOperator::Minus => "-",
        ast::expression::UnaryOperator::Plus => "+",
        ast::expression::UnaryOperator::Not => "!",
        ast::expression::UnaryOperator::BitNot => "~",
        ast::expression::UnaryOperator::Typeof => "typeof",
        ast::expression::UnaryOperator::Void => "void",
        ast::expression::UnaryOperator::Delete => "delete",
        _ => unreachable!(),
    }
}

fn import_kind_str(kind: ast::statement::ImportKind) -> &'static str {
    match kind {
        ast::statement::ImportKind::ImportType => "type",
        ast::statement::ImportKind::ImportTypeof => "typeof",
        ast::statement::ImportKind::ImportValue => "value",
    }
}

fn export_kind_str(kind: ast::statement::ExportKind) -> &'static str {
    match kind {
        ast::statement::ExportKind::ExportType => "type",
        ast::statement::ExportKind::ExportValue => "value",
    }
}

fn method_kind_str(kind: ast::class::MethodKind) -> &'static str {
    match kind {
        ast::class::MethodKind::Constructor => "constructor",
        ast::class::MethodKind::Method => "method",
        ast::class::MethodKind::Get => "get",
        ast::class::MethodKind::Set => "set",
    }
}

fn ts_accessibility_str(kind: ast::class::ts_accessibility::Kind) -> &'static str {
    match kind {
        ast::class::ts_accessibility::Kind::Public => "public",
        ast::class::ts_accessibility::Kind::Protected => "protected",
        ast::class::ts_accessibility::Kind::Private => "private",
    }
}

fn is_key_computed(key: &ast::expression::object::Key<Loc, Loc>) -> bool {
    matches!(key, ast::expression::object::Key::Computed(_))
}

fn is_pattern_key_computed(key: &ast::pattern::object::Key<Loc, Loc>) -> bool {
    matches!(key, ast::pattern::object::Key::Computed(_))
}

/// Map a `TokenKind` to the ESTree token-type index expected by
/// `FlowParserDeserializer.tokenTypes` (FlowParserDeserializer.js:28-40):
///
///   0 Boolean | 1 Identifier | 2 Keyword | 3 Null | 4 Numeric | 5 BigInt
///   6 Punctuator | 7 String | 8 RegularExpression | 9 Template | 10 JSXText
///
/// This is the standard ESLint token taxonomy. Mirrors the Hermes serializer's
/// per-token classification (which the Rust port has no token translator for —
/// upstream Hermes computes the same buckets in C++ at lex time). Type-grammar
/// keywords (`any`, `mixed`, `keyof`, …) and the JSX placeholder tokens fall
/// under `Identifier` because that's how ESLint-compatible consumers see them.
fn token_kind_to_estree_type_index(t: &TokenKind) -> u32 {
    match t {
        // Literal tokens
        TokenKind::TTrue | TokenKind::TFalse => 0, // Boolean
        TokenKind::TNull => 3,                     // Null
        TokenKind::TNumber { .. } | TokenKind::TNumberSingletonType { .. } => 4, // Numeric
        TokenKind::TBigint { .. } | TokenKind::TBigintSingletonType { .. } => 5, // BigInt
        TokenKind::TString(..) => 7,               // String
        TokenKind::TRegexp(..) => 8,               // RegularExpression
        TokenKind::TTemplatePart(..) => 9,         // Template
        TokenKind::TJsxChildText(..) | TokenKind::TJsxQuoteText(..) => 10, // JSXText

        // Identifiers (incl. JSX identifiers)
        TokenKind::TIdentifier { .. } | TokenKind::TJsxIdentifier { .. } => 1,

        // Keywords (ECMAScript reserved + Flow contextual keywords that the
        // lexer emits as their own kinds).
        TokenKind::TFunction
        | TokenKind::TIf
        | TokenKind::TIn
        | TokenKind::TInstanceof
        | TokenKind::TReturn
        | TokenKind::TSwitch
        | TokenKind::TMatch
        | TokenKind::TRecord
        | TokenKind::TThis
        | TokenKind::TThrow
        | TokenKind::TTry
        | TokenKind::TVar
        | TokenKind::TWhile
        | TokenKind::TWith
        | TokenKind::TConst
        | TokenKind::TLet
        | TokenKind::TBreak
        | TokenKind::TCase
        | TokenKind::TCatch
        | TokenKind::TContinue
        | TokenKind::TDefault
        | TokenKind::TDo
        | TokenKind::TFinally
        | TokenKind::TFor
        | TokenKind::TClass
        | TokenKind::TExtends
        | TokenKind::TStatic
        | TokenKind::TElse
        | TokenKind::TNew
        | TokenKind::TDelete
        | TokenKind::TTypeof
        | TokenKind::TVoid
        | TokenKind::TEnum
        | TokenKind::TExport
        | TokenKind::TImport
        | TokenKind::TSuper
        | TokenKind::TImplements
        | TokenKind::TInterface
        | TokenKind::TPackage
        | TokenKind::TPrivate
        | TokenKind::TProtected
        | TokenKind::TPublic
        | TokenKind::TYield
        | TokenKind::TDebugger
        | TokenKind::TDeclare
        | TokenKind::TType
        | TokenKind::TOpaque
        | TokenKind::TOf
        | TokenKind::TAsync
        | TokenKind::TAwait
        | TokenKind::TChecks => 2, // Keyword

        // Type primitives surface as identifiers to ESLint consumers (no
        // dedicated token-type bucket exists for them).
        TokenKind::TAnyType
        | TokenKind::TMixedType
        | TokenKind::TEmptyType
        | TokenKind::TBooleanType(_)
        | TokenKind::TNumberType
        | TokenKind::TBigintType
        | TokenKind::TStringType
        | TokenKind::TVoidType
        | TokenKind::TSymbolType
        | TokenKind::TUnknownType
        | TokenKind::TNeverType
        | TokenKind::TUndefinedType
        | TokenKind::TKeyof
        | TokenKind::TReadonly
        | TokenKind::TWriteonly
        | TokenKind::TInfer
        | TokenKind::TIs
        | TokenKind::TAsserts
        | TokenKind::TImplies
        | TokenKind::TRendersQuestion
        | TokenKind::TRendersStar => 1, // Identifier

        // Everything else lexes as punctuation. Includes T_ERROR / T_EOF /
        // T_INTERPRETER which never reach userland token streams in well-formed
        // input but need a fallback to satisfy the exhaustive match.
        TokenKind::TLcurly
        | TokenKind::TRcurly
        | TokenKind::TLcurlybar
        | TokenKind::TRcurlybar
        | TokenKind::TLparen
        | TokenKind::TRparen
        | TokenKind::TLbracket
        | TokenKind::TRbracket
        | TokenKind::TSemicolon
        | TokenKind::TComma
        | TokenKind::TPeriod
        | TokenKind::TArrow
        | TokenKind::TEllipsis
        | TokenKind::TAt
        | TokenKind::TPound
        | TokenKind::TRshift3Assign
        | TokenKind::TRshiftAssign
        | TokenKind::TLshiftAssign
        | TokenKind::TBitXorAssign
        | TokenKind::TBitOrAssign
        | TokenKind::TBitAndAssign
        | TokenKind::TModAssign
        | TokenKind::TDivAssign
        | TokenKind::TMultAssign
        | TokenKind::TExpAssign
        | TokenKind::TMinusAssign
        | TokenKind::TPlusAssign
        | TokenKind::TNullishAssign
        | TokenKind::TAndAssign
        | TokenKind::TOrAssign
        | TokenKind::TAssign
        | TokenKind::TPlingPeriod
        | TokenKind::TPlingPling
        | TokenKind::TPling
        | TokenKind::TColon
        | TokenKind::TOr
        | TokenKind::TAnd
        | TokenKind::TBitOr
        | TokenKind::TBitXor
        | TokenKind::TBitAnd
        | TokenKind::TEqual
        | TokenKind::TNotEqual
        | TokenKind::TStrictEqual
        | TokenKind::TStrictNotEqual
        | TokenKind::TLessThanEqual
        | TokenKind::TGreaterThanEqual
        | TokenKind::TLessThan
        | TokenKind::TGreaterThan
        | TokenKind::TLshift
        | TokenKind::TRshift
        | TokenKind::TRshift3
        | TokenKind::TPlus
        | TokenKind::TMinus
        | TokenKind::TDiv
        | TokenKind::TMult
        | TokenKind::TExp
        | TokenKind::TMod
        | TokenKind::TNot
        | TokenKind::TBitNot
        | TokenKind::TIncr
        | TokenKind::TDecr
        | TokenKind::TInterpreter(..)
        | TokenKind::TError(_)
        | TokenKind::TEof => 6, // Punctuator
    }
}

/// Convert a cleaned (no `_` separators, no `n` suffix) bigint literal source
/// such as `"0xfff123"`, `"0b101011101"`, `"0o16432"`, or `"1000"` into its
/// decimal-digit string. Returns `None` if the input is not a valid integer
/// literal so callers can fall back to the raw source.
///
/// Strip the `n` suffix and any `_` separators from a bigint literal's `raw`,
/// then convert the cleaned literal to its decimal-string form. Falls back
/// to the cleaned source string if it isn't a valid integer literal we can
/// parse. Mirrors the `bigint` field that upstream Hermes emits on both
/// `Literal` and `BigIntLiteralTypeAnnotation`.
fn clean_bigint_raw(raw: &str) -> String {
    let cleaned = raw.trim_end_matches('n').replace('_', "");
    parse_bigint_value(&cleaned).unwrap_or(cleaned)
}

/// Uses arbitrary-precision long multiplication so values larger than
/// `i128::MAX` (e.g. 256-bit hex literals) round-trip correctly to decimal.
fn parse_bigint_value(s: &str) -> Option<String> {
    let (sign, rest) = if let Some(stripped) = s.strip_prefix('-') {
        ("-", stripped)
    } else {
        ("", s)
    };
    let (radix, digits) =
        if let Some(hex) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
            (16u32, hex)
        } else if let Some(bin) = rest.strip_prefix("0b").or_else(|| rest.strip_prefix("0B")) {
            (2u32, bin)
        } else if let Some(oct) = rest.strip_prefix("0o").or_else(|| rest.strip_prefix("0O")) {
            (8u32, oct)
        } else {
            (10u32, rest)
        };
    if digits.is_empty() {
        return None;
    }
    // `decimal` holds little-endian decimal digits (index 0 = ones place).
    let mut decimal: Vec<u8> = vec![0];
    for ch in digits.chars() {
        let d = ch.to_digit(radix)?;
        // decimal *= radix
        let mut carry: u32 = 0;
        for slot in decimal.iter_mut() {
            let v = (*slot as u32) * radix + carry;
            *slot = (v % 10) as u8;
            carry = v / 10;
        }
        while carry > 0 {
            decimal.push((carry % 10) as u8);
            carry /= 10;
        }
        // decimal += d
        let mut carry: u32 = d;
        for slot in decimal.iter_mut() {
            let v = (*slot as u32) + carry;
            *slot = (v % 10) as u8;
            carry = v / 10;
            if carry == 0 {
                break;
            }
        }
        while carry > 0 {
            decimal.push((carry % 10) as u8);
            carry /= 10;
        }
    }
    // Strip trailing zero "limbs" but keep at least one digit.
    // `pop_if` reads the last element and pops only when the predicate
    // holds, avoiding a separate `.last().unwrap()`.
    while decimal.len() > 1 && decimal.pop_if(|d| *d == 0).is_some() {}
    let mut out = String::with_capacity(sign.len() + decimal.len());
    out.push_str(sign);
    for &d in decimal.iter().rev() {
        out.push((b'0' + d) as char);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: parse source and serialize, returning the program buffer.
    fn parse_and_serialize(source: &str) -> SerializerBuffers {
        let file_key = flow_parser::file_key::FileKey::new(
            flow_parser::file_key::FileKeyInner::SourceFile(String::new()),
        );
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
        let (program, errors) = flow_parser::parse_program_file::<()>(
            false,
            None,
            Some(parse_options),
            file_key,
            Ok(source),
        );
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);
        let ser = Serializer::new(source);
        ser.serialize_program(&program, &[], None)
    }

    #[test]
    fn test_empty_program() {
        let buffers = parse_and_serialize("");
        // Wire format: locId, body count, comments count, tokens count, errors count.
        assert!(buffers.program_buffer.len() >= 5);
        assert_eq!(buffers.program_buffer[1], 0);
    }

    #[test]
    fn test_simple_variable_declaration() {
        let buffers = parse_and_serialize("const x = 42;");
        assert!(buffers.program_buffer.len() > 5);
        assert_eq!(buffers.program_buffer[1], 1);
        // First node after body count is a real (kind + 1) tag, not the null marker.
        assert_ne!(buffers.program_buffer[2], 0);
    }

    #[test]
    fn test_function_declaration() {
        let buffers =
            parse_and_serialize("function foo(x: number): string { return x.toString(); }");
        assert!(buffers.program_buffer.len() > 10);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_type_annotation() {
        let buffers = parse_and_serialize("type Foo = { bar: number, baz: string };");
        assert!(buffers.program_buffer.len() > 5);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_multiple_statements() {
        let source = "const a = 1;\nconst b = 2;\nconst c = 3;";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 3);
    }

    #[test]
    fn test_class_with_methods() {
        let source = r#"class Foo {
            bar(): void {}
            baz: number;
        }"#;
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
        assert!(buffers.program_buffer.len() > 10);
    }

    #[test]
    fn test_comments_serialized() {
        let source = "// line comment\n/* block comment */\nconst x = 1;";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
        assert!(!buffers.position_buffer.is_empty());
    }

    #[test]
    fn test_position_buffer_populated() {
        let buffers = parse_and_serialize("const x = 42;");
        assert!(!buffers.position_buffer.is_empty());
        let first = &buffers.position_buffer[0];
        assert_eq!(first.line, 1);
    }

    #[test]
    fn test_jsx_element() {
        let source = "const x = <div className=\"foo\">hello</div>;";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
        assert!(buffers.program_buffer.len() > 20);
    }

    #[test]
    fn test_import_export() {
        let source = "import {foo} from 'bar';\nexport default 42;";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 2);
    }

    #[test]
    fn test_enum_declaration() {
        let source = "enum Status { Active = 'active', Inactive = 'inactive' }";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
        assert!(buffers.program_buffer.len() > 10);
    }

    #[test]
    fn test_arrow_function() {
        let source = "const f = (x: number): number => x + 1;";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_generic_types() {
        let source = "type Wrapper<T> = { value: T };";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_union_intersection_types() {
        let source = "type U = string | number | boolean;\ntype I = A & B & C;";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 2);
    }

    #[test]
    fn test_interface() {
        let source = "interface Foo extends Bar { baz(): void; qux: string }";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_try_catch() {
        let source = "try { foo(); } catch (e) { bar(); } finally { baz(); }";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_for_of_loop() {
        let source = "for (const x of [1, 2, 3]) { console.log(x); }";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_template_literal() {
        let source = "const s = `hello ${name} world`;";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_template_literal_surrogate_pair_cooked_value() {
        let cooked = cooked_template_value("\\uD83C\\uDFDE ", "\u{FFFD}\u{FFFD} ");
        assert_eq!(cooked.as_ref(), "\u{1F3DE} ");
    }

    #[test]
    fn string_literal_lone_surrogate_value_uses_wtf8() {
        assert_eq!(
            decode_quoted_string_raw_bytes(r#""\uD83E""#).as_deref(),
            Some([0xed, 0xa0, 0xbe].as_slice())
        );
        assert_eq!(
            decode_quoted_string_raw_bytes(r#""\uDD21""#).as_deref(),
            Some([0xed, 0xb4, 0xa1].as_slice())
        );
    }

    #[test]
    fn string_literal_escaped_backslash_does_not_decode_surrogate_escape() {
        assert_eq!(
            decode_quoted_string_raw_bytes(r#""\\uD83E""#).as_deref(),
            Some(br"\uD83E".as_slice())
        );
    }

    #[test]
    fn template_literal_lone_surrogate_value_uses_wtf8() {
        let bytes = template_cooked_value_bytes("\\uD83E", "\\u0D83\\uD83E");
        assert_eq!(bytes.as_ref(), [0xed, 0xa0, 0xbe].as_slice());
    }

    #[test]
    fn test_destructuring() {
        let source = "const { a, b: c, ...rest } = obj;";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_spread_and_rest() {
        let source = "function f(...args: Array<number>): number { return Math.max(...args); }";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_optional_chaining() {
        let source = "const x = a?.b?.c;";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_optional_chain_argument_starts_independent_chain() {
        let source = "foo?.(bar?.baz);";
        let buffers = parse_and_serialize(source);
        let chain_tag = NodeKind::ChainExpression as u32 + 1;
        assert_eq!(
            buffers
                .program_buffer
                .iter()
                .filter(|&&tag| tag == chain_tag)
                .count(),
            2
        );
    }

    #[test]
    fn test_optional_chain_computed_property_starts_independent_chain() {
        let source = "foo?.[bar?.baz];";
        let buffers = parse_and_serialize(source);
        let chain_tag = NodeKind::ChainExpression as u32 + 1;
        assert_eq!(
            buffers
                .program_buffer
                .iter()
                .filter(|&&tag| tag == chain_tag)
                .count(),
            2
        );
    }

    #[test]
    fn test_optional_chain_array_element_starts_independent_chain() {
        let source = "[foo?.bar]?.map(Boolean);";
        let buffers = parse_and_serialize(source);
        let chain_tag = NodeKind::ChainExpression as u32 + 1;
        assert_eq!(
            buffers
                .program_buffer
                .iter()
                .filter(|&&tag| tag == chain_tag)
                .count(),
            2
        );
    }

    #[test]
    fn test_optional_chain_logical_operand_starts_independent_chain() {
        let source = "(foo?.bar ?? [])?.map(Boolean);";
        let buffers = parse_and_serialize(source);
        let chain_tag = NodeKind::ChainExpression as u32 + 1;
        assert_eq!(
            buffers
                .program_buffer
                .iter()
                .filter(|&&tag| tag == chain_tag)
                .count(),
            2
        );
    }

    #[test]
    fn test_declare_class() {
        let source = "declare class Foo { bar(): void; baz: number; }";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_typeof_type() {
        let source = "type T = typeof someVar;";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_tuple_type() {
        let source = "type T = [number, string, boolean];";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_conditional_expression() {
        let source = "const x = a ? b : c;";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_async_await() {
        let source = "async function f() { const x = await fetch('url'); }";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_switch_statement() {
        let source = r#"switch (x) { case 1: break; case 2: break; default: throw new Error(); }"#;
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_tagged_template() {
        let source = "const x = tag`hello ${name}`;";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_declare_export() {
        let source = "declare export function foo(): void;";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_opaque_type() {
        let source = "opaque type ID: string = string;";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_nullable_type() {
        let source = "type T = ?string;";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_indexer_type() {
        let source = "type Dict = { [key: string]: number };";
        let buffers = parse_and_serialize(source);
        assert_eq!(buffers.program_buffer[1], 1);
    }

    #[test]
    fn test_complex_flow_program() {
        // A larger test that exercises many serialization paths at once
        let source = r#"
// @flow
import type {Node} from 'ast';
import {parse} from 'parser';

type Options = {
    tokens?: boolean,
    sourceType?: 'module' | 'script',
};

interface Visitor {
    enter(node: Node): void;
    exit(node: Node): void;
}

export function transform(
    source: string,
    options?: Options,
): Node {
    const ast = parse(source, options ?? {});
    const result: Array<Node> = [];
    for (const node of ast.body) {
        if (node.type === 'FunctionDeclaration') {
            result.push(node);
        }
    }
    return ast;
}

export default class Transformer {
    +options: Options;

    constructor(options: Options) {
        this.options = options;
    }

    run(source: string): Node {
        return transform(source, this.options);
    }
}
"#;
        let buffers = parse_and_serialize(source);
        // Should parse without panicking and produce non-trivial output
        assert!(buffers.program_buffer.len() > 50);
        assert!(buffers.position_buffer.len() > 10);
    }

    #[test]
    fn parse_bigint_value_small_decimal() {
        assert_eq!(parse_bigint_value("0").as_deref(), Some("0"));
        assert_eq!(parse_bigint_value("1").as_deref(), Some("1"));
        assert_eq!(parse_bigint_value("1000").as_deref(), Some("1000"));
        assert_eq!(parse_bigint_value("-42").as_deref(), Some("-42"));
    }

    #[test]
    fn parse_bigint_value_small_radix() {
        assert_eq!(parse_bigint_value("0xff").as_deref(), Some("255"));
        assert_eq!(parse_bigint_value("0XFF").as_deref(), Some("255"));
        assert_eq!(parse_bigint_value("0b1010").as_deref(), Some("10"));
        assert_eq!(parse_bigint_value("0B1010").as_deref(), Some("10"));
        assert_eq!(parse_bigint_value("0o17").as_deref(), Some("15"));
        assert_eq!(parse_bigint_value("0O17").as_deref(), Some("15"));
    }

    #[test]
    fn parse_bigint_value_invalid() {
        assert_eq!(parse_bigint_value(""), None);
        assert_eq!(parse_bigint_value("0x"), None);
        assert_eq!(parse_bigint_value("0xZ"), None);
        assert_eq!(parse_bigint_value("0b2"), None);
        assert_eq!(parse_bigint_value("0o9"), None);
    }

    #[test]
    fn parse_bigint_value_i128_max_plus_one_decimal() {
        // i128::MAX = 170141183460469231731687303715884105727
        // i128::MAX + 1 = 170141183460469231731687303715884105728
        let v = parse_bigint_value("170141183460469231731687303715884105728");
        assert_eq!(
            v.as_deref(),
            Some("170141183460469231731687303715884105728")
        );
    }

    #[test]
    fn parse_bigint_value_i128_max_plus_one_hex() {
        // 0x80000000000000000000000000000000 = 2^127 = i128::MAX + 1
        let v = parse_bigint_value("0x80000000000000000000000000000000");
        assert_eq!(
            v.as_deref(),
            Some("170141183460469231731687303715884105728")
        );
    }

    #[test]
    fn parse_bigint_value_256_bit_hex() {
        // 2^256 - 1 in hex (sixty-four `f`s).
        let raw = "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
        let expected =
            "115792089237316195423570985008687907853269984665640564039457584007913129639935";
        assert_eq!(parse_bigint_value(raw).as_deref(), Some(expected));
    }

    #[test]
    fn parse_bigint_value_256_bit_binary() {
        // 2^255 — a 1 followed by 255 zeros in binary.
        let mut raw = String::from("0b1");
        for _ in 0..255 {
            raw.push('0');
        }
        let expected =
            "57896044618658097711785492504343953926634992332820282019728792003956564819968";
        assert_eq!(parse_bigint_value(&raw).as_deref(), Some(expected));
    }

    #[test]
    fn parse_bigint_value_negative_large() {
        let v = parse_bigint_value("-0x80000000000000000000000000000000");
        assert_eq!(
            v.as_deref(),
            Some("-170141183460469231731687303715884105728")
        );
    }

    #[test]
    fn schema_is_complete_and_contiguous() {
        use crate::node_kinds::NodeKind;
        use crate::node_kinds::SCHEMA;
        // Verify SCHEMA has COUNT entries and IDs are 0..COUNT-1
        assert_eq!(
            SCHEMA.len(),
            NodeKind::COUNT,
            "SCHEMA length ({}) does not match NodeKind::COUNT ({})",
            SCHEMA.len(),
            NodeKind::COUNT,
        );
        for (i, def) in SCHEMA.iter().enumerate() {
            assert_eq!(
                def.kind_id as usize, i,
                "SCHEMA[{}] has kind_id {} (expected {}), name = {}",
                i, def.kind_id, i, def.name,
            );
        }
    }
}
