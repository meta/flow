/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// @generated
//
// !!! GENERATED FILE !!!
//
// Any manual changes to this file will be overwritten.
// To regenerate:
//   buck run fbcode//flow/rust_port/crates/flow_parser_wasm:codegen -- --rust > \
//     fbcode/flow/rust_port/crates/flow_parser_wasm/src/serializer_dispatch.rs
//
// This file is included via `include!()` in serializer.rs at module level.
// It has access to all imports and types from serializer.rs.
impl<'a> Serializer<'a> {
    fn serialize_statement_dispatch(
        &mut self,
        stmt: &ast::statement::Statement<Loc, Loc>,
    ) {
        use ast::statement::StatementInner;
        match &**stmt {
            StatementInner::Empty { loc, .. } => {
                self.write_node_header(NodeKind::EmptyStatement, loc);
            }
            StatementInner::Expression { loc, inner } => {
                self.serialize_expression_statement(loc, inner);
            }
            StatementInner::Block { loc, inner } => {
                self.serialize_block_statement(loc, inner);
            }
            StatementInner::If { loc, inner } => {
                self.write_node_header(NodeKind::IfStatement, loc);
                self.serialize_expression(&inner.test);
                self.serialize_statement(&inner.consequent);
                match &inner.alternate {
                    Some(alt) => self.serialize_statement(&alt.body),
                    None => self.write_null_node(),
                };
            }
            StatementInner::Labeled { loc, inner } => {
                self.write_node_header(NodeKind::LabeledStatement, loc);
                self.serialize_identifier_node(&inner.label);
                self.serialize_statement(&inner.body);
            }
            StatementInner::Break { loc, inner } => {
                self.write_node_header(NodeKind::BreakStatement, loc);
                match &inner.label {
                    Some(id) => self.serialize_identifier_node(id),
                    None => self.write_null_node(),
                };
            }
            StatementInner::Continue { loc, inner } => {
                self.write_node_header(NodeKind::ContinueStatement, loc);
                match &inner.label {
                    Some(id) => self.serialize_identifier_node(id),
                    None => self.write_null_node(),
                };
            }
            StatementInner::With { loc, inner } => {
                self.write_node_header(NodeKind::WithStatement, loc);
                self.serialize_expression(&inner.object);
                self.serialize_statement(&inner.body);
            }
            StatementInner::Switch { loc, inner } => {
                self.write_node_header(NodeKind::SwitchStatement, loc);
                self.serialize_expression(&inner.discriminant);
                {
                    self.buf.push(inner.cases.len() as u32);
                    for case in inner.cases.iter() {
                        self.serialize_switch_case(case);
                    }
                };
            }
            StatementInner::Return { loc, inner } => {
                self.write_node_header(NodeKind::ReturnStatement, loc);
                match &inner.argument {
                    Some(expr) => self.serialize_expression(expr),
                    None => self.write_null_node(),
                };
            }
            StatementInner::Throw { loc, inner } => {
                self.write_node_header(NodeKind::ThrowStatement, loc);
                self.serialize_expression(&inner.argument);
            }
            StatementInner::Try { loc, inner } => {
                self.write_node_header(NodeKind::TryStatement, loc);
                self.serialize_block_statement(&inner.block.0, &inner.block.1);
                match &inner.handler {
                    Some(catch) => self.serialize_catch_clause(catch),
                    None => self.write_null_node(),
                };
                match &inner.finalizer {
                    Some((floc, fblock)) => self.serialize_block_statement(floc, fblock),
                    None => self.write_null_node(),
                };
            }
            StatementInner::While { loc, inner } => {
                self.write_node_header(NodeKind::WhileStatement, loc);
                self.serialize_statement(&inner.body);
                self.serialize_expression(&inner.test);
            }
            StatementInner::DoWhile { loc, inner } => {
                self.write_node_header(NodeKind::DoWhileStatement, loc);
                self.serialize_statement(&inner.body);
                self.serialize_expression(&inner.test);
            }
            StatementInner::For { loc, inner } => {
                self.write_node_header(NodeKind::ForStatement, loc);
                self.serialize_for_init(&inner.init);
                match &inner.test {
                    Some(expr) => self.serialize_expression(expr),
                    None => self.write_null_node(),
                };
                match &inner.update {
                    Some(expr) => self.serialize_expression(expr),
                    None => self.write_null_node(),
                };
                self.serialize_statement(&inner.body);
            }
            StatementInner::ForIn { loc, inner } => {
                self.write_node_header(NodeKind::ForInStatement, loc);
                self.serialize_for_in_left(&inner.left);
                self.serialize_expression(&inner.right);
                self.serialize_statement(&inner.body);
            }
            StatementInner::ForOf { loc, inner } => {
                self.write_node_header(NodeKind::ForOfStatement, loc);
                self.serialize_for_of_left(&inner.left);
                self.serialize_expression(&inner.right);
                self.serialize_statement(&inner.body);
                self.write_bool(inner.await_);
            }
            StatementInner::Debugger { loc, .. } => {
                self.write_node_header(NodeKind::DebuggerStatement, loc);
            }
            StatementInner::Match { loc, inner } => {
                self.write_node_header(NodeKind::MatchStatement, loc);
                self.serialize_expression(&inner.arg);
                {
                    self.buf.push(inner.cases.len() as u32);
                    for case in inner.cases.iter() {
                        self.serialize_match_statement_case(case);
                    }
                };
            }
            StatementInner::FunctionDeclaration { loc, inner } => {
                self.serialize_function_decl(loc, inner, NodeKind::FunctionDeclaration);
            }
            StatementInner::VariableDeclaration { loc, inner } => {
                self.serialize_variable_declaration(loc, inner);
            }
            StatementInner::ClassDeclaration { loc, inner } => {
                self.serialize_class(loc, inner, NodeKind::ClassDeclaration);
            }
            StatementInner::ComponentDeclaration { loc, inner } => {
                self.serialize_component_declaration(loc, inner);
            }
            StatementInner::EnumDeclaration { loc, inner } => {
                self.serialize_enum_declaration(loc, inner);
            }
            StatementInner::InterfaceDeclaration { loc, inner } => {
                self.serialize_interface_declaration(loc, inner);
            }
            StatementInner::TypeAlias { loc, inner } => {
                self.write_node_header(NodeKind::TypeAlias, loc);
                self.serialize_identifier_node(&inner.id);
                self.serialize_type_params_opt(&inner.tparams);
                self.serialize_type(&inner.right);
            }
            StatementInner::RecordDeclaration { loc, inner } => {
                self.serialize_record_declaration(loc, inner);
            }
            StatementInner::ImportDeclaration { loc, inner } => {
                self.serialize_import_declaration(loc, inner);
            }
            StatementInner::ImportEqualsDeclaration { loc, inner } => {
                self.serialize_import_equals_declaration(loc, inner);
            }
            StatementInner::ExportNamedDeclaration { loc, inner } => {
                self.serialize_export_named_declaration(loc, inner);
            }
            StatementInner::ExportDefaultDeclaration { loc, inner } => {
                self.serialize_export_default_declaration(loc, inner);
            }
            StatementInner::ExportAssignment { loc, inner } => {
                self.serialize_export_assignment(loc, inner);
            }
            StatementInner::DeclareVariable { loc, inner } => {
                self.serialize_declare_variable(loc, inner);
            }
            StatementInner::DeclareFunction { loc, inner } => {
                self.serialize_declare_function(loc, inner);
            }
            StatementInner::DeclareClass { loc, inner } => {
                self.serialize_declare_class(loc, inner);
            }
            StatementInner::DeclareComponent { loc, inner } => {
                self.serialize_declare_component(loc, inner);
            }
            StatementInner::DeclareModule { loc, inner } => {
                self.serialize_declare_module(loc, inner);
            }
            StatementInner::DeclareModuleExports { loc, inner } => {
                self.write_node_header(NodeKind::DeclareModuleExports, loc);
                self.serialize_type_annotation(&inner.annot);
            }
            StatementInner::DeclareExportDeclaration { loc, inner } => {
                self.serialize_declare_export_declaration(loc, inner);
            }
            StatementInner::DeclareNamespace { loc, inner } => {
                self.serialize_declare_namespace(loc, inner);
            }
            StatementInner::DeclareInterface { loc, inner } => {
                self.serialize_declare_interface(loc, inner);
            }
            StatementInner::DeclareTypeAlias { loc, inner } => {
                self.write_node_header(NodeKind::DeclareTypeAlias, loc);
                self.serialize_identifier_node(&inner.id);
                self.serialize_type_params_opt(&inner.tparams);
                self.serialize_type(&inner.right);
                self.write_bool(self.implicit_declare_from_loc(loc));
            }
            StatementInner::DeclareEnum { loc, inner } => {
                self.write_node_header(NodeKind::DeclareEnum, loc);
                self.serialize_identifier_node(&inner.id);
                self.serialize_enum_body(&inner.body);
                self.write_bool(self.implicit_declare_from_loc(loc));
                self.write_bool(inner.const_);
            }
            StatementInner::NamespaceExportDeclaration { loc, inner } => {
                self.write_node_header(NodeKind::NamespaceExportDeclaration, loc);
                self.serialize_identifier_node(&inner.id);
            }
            StatementInner::OpaqueType { loc, inner } => {
                self.serialize_opaque_type(loc, inner);
            }
            StatementInner::DeclareOpaqueType { loc, inner } => {
                self.serialize_declare_opaque_type(loc, inner);
            }
        }
    }
    fn serialize_expression_dispatch(
        &mut self,
        expr: &ast::expression::Expression<Loc, Loc>,
    ) {
        use ast::expression::ExpressionInner;
        match &**expr {
            ExpressionInner::This { loc, .. } => {
                self.write_node_header(NodeKind::ThisExpression, loc);
            }
            ExpressionInner::Super { loc, .. } => {
                self.write_node_header(NodeKind::Super, loc);
            }
            ExpressionInner::Array { loc, inner } => {
                self.serialize_array_expression(loc, inner);
            }
            ExpressionInner::Object { loc, inner } => {
                self.serialize_object_expression(loc, inner);
            }
            ExpressionInner::Function { loc, inner } => {
                self.serialize_function_expr(loc, inner, NodeKind::FunctionExpression);
            }
            ExpressionInner::ArrowFunction { loc, inner } => {
                self.serialize_function_expr(
                    loc,
                    inner,
                    NodeKind::ArrowFunctionExpression,
                );
            }
            ExpressionInner::Sequence { loc, inner } => {
                self.serialize_sequence_expression(loc, inner);
            }
            ExpressionInner::Unary { loc, inner } => {
                self.serialize_unary_dispatch(loc, inner);
            }
            ExpressionInner::Binary { loc, inner } => {
                self.write_node_header(NodeKind::BinaryExpression, loc);
                self.serialize_expression(&inner.left);
                self.serialize_expression(&inner.right);
                self.write_str(inner.operator.as_str());
            }
            ExpressionInner::Logical { loc, inner } => {
                self.write_node_header(NodeKind::LogicalExpression, loc);
                self.serialize_expression(&inner.left);
                self.serialize_expression(&inner.right);
                self.write_str(
                    match inner.operator {
                        ast::expression::LogicalOperator::Or => "||",
                        ast::expression::LogicalOperator::And => "&&",
                        ast::expression::LogicalOperator::NullishCoalesce => "??",
                    },
                );
            }
            ExpressionInner::Conditional { loc, inner } => {
                self.write_node_header(NodeKind::ConditionalExpression, loc);
                self.serialize_expression(&inner.test);
                self.serialize_expression(&inner.alternate);
                self.serialize_expression(&inner.consequent);
            }
            ExpressionInner::Update { loc, inner } => {
                self.write_node_header(NodeKind::UpdateExpression, loc);
                self.write_str(
                    match inner.operator {
                        ast::expression::UpdateOperator::Increment => "++",
                        ast::expression::UpdateOperator::Decrement => "--",
                    },
                );
                self.serialize_expression(&inner.argument);
                self.write_bool(inner.prefix);
            }
            ExpressionInner::Assignment { loc, inner } => {
                self.write_node_header(NodeKind::AssignmentExpression, loc);
                self.write_str(
                    match &inner.operator {
                        Some(op) => op.as_str(),
                        None => "=",
                    },
                );
                self.serialize_pattern(&inner.left);
                self.serialize_expression(&inner.right);
            }
            ExpressionInner::Member { loc, inner } => {
                self.serialize_member_expression(loc, inner);
            }
            ExpressionInner::OptionalMember { loc, inner } => {
                self.serialize_optional_member_as_member_expression(loc, inner);
            }
            ExpressionInner::Call { loc, inner } => {
                self.serialize_call_expression(loc, inner);
            }
            ExpressionInner::OptionalCall { loc, inner } => {
                self.serialize_optional_call_as_call_expression(loc, inner);
            }
            ExpressionInner::New { loc, inner } => {
                self.write_node_header(NodeKind::NewExpression, loc);
                self.serialize_expression(&inner.callee);
                self.serialize_call_type_args_opt(&inner.targs);
                match &inner.arguments {
                    Some(args) => self.serialize_arg_list(args),
                    None => self.buf.push(0),
                };
            }
            ExpressionInner::Yield { loc, inner } => {
                self.write_node_header(NodeKind::YieldExpression, loc);
                match &inner.argument {
                    Some(expr) => self.serialize_expression(expr),
                    None => self.write_null_node(),
                };
                self.write_bool(inner.delegate);
            }
            ExpressionInner::Import { loc, inner } => {
                self.serialize_import_expression(loc, inner);
            }
            ExpressionInner::MetaProperty { loc, inner } => {
                self.write_node_header(NodeKind::MetaProperty, loc);
                self.serialize_identifier_node(&inner.meta);
                self.serialize_identifier_node(&inner.property);
            }
            ExpressionInner::TaggedTemplate { loc, inner } => {
                self.write_node_header(NodeKind::TaggedTemplateExpression, loc);
                self.serialize_expression(&inner.tag);
                self.serialize_template_literal(&inner.quasi.0, &inner.quasi.1);
                self.serialize_tagged_template_type_arguments(&inner.targs);
            }
            ExpressionInner::TemplateLiteral { loc, inner } => {
                self.serialize_template_literal(loc, inner);
            }
            ExpressionInner::TypeCast { loc, inner } => {
                self.serialize_type_cast_expression(loc, inner);
            }
            ExpressionInner::AsExpression { loc, inner } => {
                self.serialize_as_expression(loc, inner);
            }
            ExpressionInner::TSSatisfies { loc, inner } => {
                self.write_node_header(NodeKind::SatisfiesExpression, loc);
                self.serialize_expression(&inner.expression);
                self.serialize_type(&inner.annot.annotation);
            }
            ExpressionInner::AsConstExpression { loc, inner } => {
                self.serialize_as_const_expression(loc, inner);
            }
            ExpressionInner::Match { loc, inner } => {
                self.serialize_match_expression(loc, inner);
            }
            ExpressionInner::Record { loc, inner } => {
                self.serialize_record_expression(loc, inner);
            }
            ExpressionInner::NullLiteral { loc, .. } => {
                self.write_null_literal(loc);
            }
            ExpressionInner::BooleanLiteral { loc, inner } => {
                self.write_boolean_literal(loc, inner.value);
            }
            ExpressionInner::NumberLiteral { loc, inner } => {
                self.write_number_literal(loc, inner.value, &inner.raw);
            }
            ExpressionInner::StringLiteral { loc, inner } => {
                self.write_string_literal(loc, &inner.value, &inner.raw);
            }
            ExpressionInner::BigIntLiteral { loc, inner } => {
                self.write_bigint_literal(loc, &inner.raw);
            }
            ExpressionInner::RegExpLiteral { loc, inner } => {
                self.write_regex_literal(loc, &inner.raw, &inner.pattern, &inner.flags);
            }
            ExpressionInner::ModuleRefLiteral { loc, inner } => {
                self.write_string_literal(loc, &inner.value, &inner.raw);
            }
            ExpressionInner::Identifier { inner, .. } => {
                self.serialize_identifier_node(inner);
            }
            ExpressionInner::Class { loc, inner } => {
                self.serialize_class(loc, inner, NodeKind::ClassExpression);
            }
            ExpressionInner::JSXElement { loc, inner } => {
                self.serialize_jsx_element(loc, inner);
            }
            ExpressionInner::JSXFragment { loc, inner } => {
                self.serialize_jsx_fragment(loc, inner);
            }
        }
    }
    fn serialize_type_dispatch(&mut self, ty: &ast::types::Type<Loc, Loc>) {
        use ast::types::TypeInner;
        match &**ty {
            TypeInner::Any { loc, .. } => {
                self.write_node_header(NodeKind::AnyTypeAnnotation, loc);
            }
            TypeInner::Mixed { loc, .. } => {
                self.write_node_header(NodeKind::MixedTypeAnnotation, loc);
            }
            TypeInner::Empty { loc, .. } => {
                self.write_node_header(NodeKind::EmptyTypeAnnotation, loc);
            }
            TypeInner::Void { loc, .. } => {
                self.write_node_header(NodeKind::VoidTypeAnnotation, loc);
            }
            TypeInner::Null { loc, .. } => {
                self.write_node_header(NodeKind::NullLiteralTypeAnnotation, loc);
            }
            TypeInner::Symbol { loc, .. } => {
                self.write_node_header(NodeKind::SymbolTypeAnnotation, loc);
            }
            TypeInner::Number { loc, .. } => {
                self.write_node_header(NodeKind::NumberTypeAnnotation, loc);
            }
            TypeInner::BigInt { loc, .. } => {
                self.write_node_header(NodeKind::BigIntTypeAnnotation, loc);
            }
            TypeInner::String { loc, .. } => {
                self.write_node_header(NodeKind::StringTypeAnnotation, loc);
            }
            TypeInner::Boolean { loc, .. } => {
                self.write_node_header(NodeKind::BooleanTypeAnnotation, loc);
            }
            TypeInner::Nullable { loc, inner } => {
                self.write_node_header(NodeKind::NullableTypeAnnotation, loc);
                self.serialize_type(&inner.argument);
            }
            TypeInner::Array { loc, inner } => {
                self.write_node_header(NodeKind::ArrayTypeAnnotation, loc);
                self.serialize_type(&inner.argument);
            }
            TypeInner::IndexedAccess { loc, inner } => {
                self.write_node_header(NodeKind::IndexedAccessType, loc);
                self.serialize_type(&inner.object);
                self.serialize_type(&inner.index);
            }
            TypeInner::OptionalIndexedAccess { loc, inner } => {
                self.write_node_header(NodeKind::OptionalIndexedAccessType, loc);
                self.serialize_type(&inner.indexed_access.object);
                self.serialize_type(&inner.indexed_access.index);
                self.write_bool(inner.optional);
            }
            TypeInner::Union { loc, inner } => {
                self.serialize_union_type(loc, inner);
            }
            TypeInner::Intersection { loc, inner } => {
                self.serialize_intersection_type(loc, inner);
            }
            TypeInner::Keyof { loc, inner } => {
                self.write_node_header(NodeKind::KeyofTypeAnnotation, loc);
                self.serialize_type(&inner.argument);
            }
            TypeInner::ReadOnly { loc, inner } => {
                self.write_node_header(NodeKind::TypeOperator, loc);
                self.write_str("readonly");
                self.serialize_type(&inner.argument);
            }
            TypeInner::UniqueSymbol { loc, symbol_loc, .. } => {
                self.write_node_header(NodeKind::TypeOperator, loc);
                self.write_str("unique");
                self.write_node_header(NodeKind::SymbolTypeAnnotation, symbol_loc);
            }
            TypeInner::Renders { loc, inner } => {
                self.serialize_renders_type_dispatch(loc, inner);
            }
            TypeInner::StringLiteral { loc, literal } => {
                self.write_node_header(NodeKind::StringLiteralTypeAnnotation, loc);
                self.write_string_literal_value(&literal.value, &literal.raw);
                self.write_str(&literal.raw);
            }
            TypeInner::NumberLiteral { loc, literal } => {
                self.write_node_header(NodeKind::NumberLiteralTypeAnnotation, loc);
                self.write_number(literal.value);
                self.write_str(&literal.raw);
            }
            TypeInner::BigIntLiteral { loc, literal } => {
                self.write_node_header(NodeKind::BigIntLiteralTypeAnnotation, loc);
                self.write_null_node();
                self.write_str(&literal.raw);
                self.write_str(&clean_bigint_raw(&literal.raw));
            }
            TypeInner::BooleanLiteral { loc, literal } => {
                self.write_node_header(NodeKind::BooleanLiteralTypeAnnotation, loc);
                self.write_bool(literal.value);
                self.write_str(if literal.value { "true" } else { "false" });
            }
            TypeInner::Exists { loc, .. } => {
                self.write_node_header(NodeKind::ExistsTypeAnnotation, loc);
            }
            TypeInner::Unknown { loc, .. } => {
                self.write_node_header(NodeKind::UnknownTypeAnnotation, loc);
            }
            TypeInner::Never { loc, .. } => {
                self.write_node_header(NodeKind::NeverTypeAnnotation, loc);
            }
            TypeInner::Undefined { loc, .. } => {
                self.write_node_header(NodeKind::UndefinedTypeAnnotation, loc);
            }
            TypeInner::Generic { loc, inner } => {
                self.serialize_generic_type(loc, inner);
            }
            TypeInner::Typeof { loc, inner } => {
                self.write_node_header(NodeKind::TypeofTypeAnnotation, loc);
                self.serialize_typeof_target(&inner.argument);
                self.serialize_type_args_opt(&inner.targs);
            }
            TypeInner::Tuple { loc, inner } => {
                self.serialize_tuple_type(loc, inner);
            }
            TypeInner::Object { loc, inner } => {
                self.serialize_type_object(loc, inner);
            }
            TypeInner::Conditional { loc, inner } => {
                self.write_node_header(NodeKind::ConditionalTypeAnnotation, loc);
                self.serialize_type(&inner.check_type);
                self.serialize_type(&inner.extends_type);
                self.serialize_type(&inner.true_type);
                self.serialize_type(&inner.false_type);
            }
            TypeInner::Infer { loc, inner } => {
                self.write_node_header(NodeKind::InferTypeAnnotation, loc);
                self.serialize_type_parameter(&inner.tparam);
            }
            TypeInner::Interface { loc, inner } => {
                self.serialize_interface_type(loc, inner);
            }
            TypeInner::Component { loc, inner } => {
                self.serialize_component_type(loc, inner);
            }
            TypeInner::Function { loc, inner } => {
                self.serialize_function_type(loc, inner);
            }
            TypeInner::TemplateLiteral { loc, inner } => {
                self.serialize_template_literal_type(loc, inner);
            }
            TypeInner::ConstructorType { loc, abstract_, inner } => {
                self.serialize_constructor_type(loc, *abstract_, inner);
            }
        }
    }
}
