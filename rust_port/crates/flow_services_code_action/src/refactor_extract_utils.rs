/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_analysis::scope_api;
use flow_analysis::scope_api::ScopeInfo;
use flow_analysis::ssa_api;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::statement;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc;
use flow_parser_utils::file_sig::FileSig;
use flow_typing_context::Context;

use crate::autofix_imports;
use crate::insert_type;

pub mod information_collectors {
    use flow_parser::ast_visitor;

    use super::*;

    #[derive(Debug, Clone)]
    pub struct T {
        pub has_unwrapped_control_flow: bool,
        pub async_function: bool,
        pub has_this_super: bool,
    }

    struct ExtractedInformationCollector {
        has_this_super: bool,
        is_async: bool,
        has_unwrapped_control_flow: bool,
        inside_loop: bool,
        inside_switch: bool,
    }

    impl ExtractedInformationCollector {
        fn new() -> Self {
            Self {
                has_this_super: false,
                is_async: false,
                has_unwrapped_control_flow: false,
                inside_loop: false,
                inside_switch: false,
            }
        }
    }

    impl<'ast> AstVisitor<'ast, Loc> for ExtractedInformationCollector {
        fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
            loc
        }

        fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
            type_
        }

        fn class_(
            &mut self,
            _loc: &'ast Loc,
            _cls: &'ast ast::class::Class<Loc, Loc>,
        ) -> Result<(), !> {
            Ok(())
        }

        fn function_declaration(
            &mut self,
            loc: &'ast Loc,
            f: &'ast ast::function::Function<Loc, Loc>,
        ) -> Result<(), !> {
            if f.async_ {
                Ok(())
            } else {
                ast_visitor::function_declaration_default(self, loc, f)
            }
        }

        fn function_expression(
            &mut self,
            loc: &'ast Loc,
            f: &'ast ast::function::Function<Loc, Loc>,
        ) -> Result<(), !> {
            if f.async_ {
                Ok(())
            } else {
                ast_visitor::function_expression_default(self, loc, f)
            }
        }

        fn function_expression_or_method(
            &mut self,
            loc: &'ast Loc,
            f: &'ast ast::function::Function<Loc, Loc>,
        ) -> Result<(), !> {
            if f.async_ {
                Ok(())
            } else {
                ast_visitor::function_expression_or_method_default(self, loc, f)
            }
        }

        fn arrow_function(
            &mut self,
            loc: &'ast Loc,
            f: &'ast ast::function::Function<Loc, Loc>,
        ) -> Result<(), !> {
            if f.async_ {
                Ok(())
            } else {
                ast_visitor::arrow_function_default(self, loc, f)
            }
        }

        fn for_of_statement(
            &mut self,
            loc: &'ast Loc,
            stmt: &'ast ast::statement::ForOf<Loc, Loc>,
        ) -> Result<(), !> {
            if stmt.await_ {
                self.is_async = true;
            }
            let old_inside_loop = self.inside_loop;
            self.inside_loop = true;
            let result = ast_visitor::for_of_statement_default(self, loc, stmt);
            self.inside_loop = old_inside_loop;
            result
        }

        fn for_in_statement(
            &mut self,
            loc: &'ast Loc,
            stmt: &'ast ast::statement::ForIn<Loc, Loc>,
        ) -> Result<(), !> {
            let old_inside_loop = self.inside_loop;
            self.inside_loop = true;
            let result = ast_visitor::for_in_statement_default(self, loc, stmt);
            self.inside_loop = old_inside_loop;
            result
        }

        fn do_while(
            &mut self,
            loc: &'ast Loc,
            stmt: &'ast ast::statement::DoWhile<Loc, Loc>,
        ) -> Result<(), !> {
            let old_inside_loop = self.inside_loop;
            self.inside_loop = true;
            let result = ast_visitor::do_while_default(self, loc, stmt);
            self.inside_loop = old_inside_loop;
            result
        }

        fn while_(
            &mut self,
            loc: &'ast Loc,
            stmt: &'ast ast::statement::While<Loc, Loc>,
        ) -> Result<(), !> {
            let old_inside_loop = self.inside_loop;
            self.inside_loop = true;
            let result = ast_visitor::while_default(self, loc, stmt);
            self.inside_loop = old_inside_loop;
            result
        }

        fn switch(
            &mut self,
            loc: &'ast Loc,
            stmt: &'ast ast::statement::Switch<Loc, Loc>,
        ) -> Result<(), !> {
            let old_inside_switch = self.inside_switch;
            self.inside_switch = true;
            let result = ast_visitor::switch_default(self, loc, stmt);
            self.inside_switch = old_inside_switch;
            result
        }

        fn return_(
            &mut self,
            loc: &'ast Loc,
            ret: &'ast ast::statement::Return<Loc, Loc>,
        ) -> Result<(), !> {
            self.has_unwrapped_control_flow = true;
            ast_visitor::return_default(self, loc, ret)
        }

        fn yield_(
            &mut self,
            loc: &'ast Loc,
            expr: &'ast ast::expression::Yield<Loc, Loc>,
        ) -> Result<(), !> {
            self.has_unwrapped_control_flow = true;
            ast_visitor::yield_default(self, loc, expr)
        }

        fn break_(
            &mut self,
            loc: &'ast Loc,
            break_stmt: &'ast ast::statement::Break<Loc>,
        ) -> Result<(), !> {
            if (!self.inside_loop && !self.inside_switch) || break_stmt.label.is_some() {
                self.has_unwrapped_control_flow = true;
            }
            ast_visitor::break_default(self, loc, break_stmt)
        }

        fn continue_(
            &mut self,
            loc: &'ast Loc,
            cont: &'ast ast::statement::Continue<Loc>,
        ) -> Result<(), !> {
            if !self.inside_loop || cont.label.is_some() {
                self.has_unwrapped_control_flow = true;
            }
            ast_visitor::continue_default(self, loc, cont)
        }

        fn labeled_statement(
            &mut self,
            loc: &'ast Loc,
            label: &'ast ast::statement::Labeled<Loc, Loc>,
        ) -> Result<(), !> {
            self.has_unwrapped_control_flow = true;
            ast_visitor::labeled_statement_default(self, loc, label)
        }

        fn this_expression(
            &mut self,
            loc: &'ast Loc,
            expr: &'ast ast::expression::This<Loc>,
        ) -> Result<(), !> {
            self.has_this_super = true;
            ast_visitor::this_expression_default(self, loc, expr)
        }

        fn super_expression(
            &mut self,
            loc: &'ast Loc,
            expr: &'ast ast::expression::Super<Loc>,
        ) -> Result<(), !> {
            self.has_this_super = true;
            ast_visitor::super_expression_default(self, loc, expr)
        }

        fn unary_expression(
            &mut self,
            loc: &'ast Loc,
            expr: &'ast ast::expression::Unary<Loc, Loc>,
        ) -> Result<(), !> {
            if expr.operator == ast::expression::UnaryOperator::Await {
                self.is_async = true;
            }
            ast_visitor::unary_expression_default(self, loc, expr)
        }
    }

    pub fn collect_statements_information(
        extracted_statements: &[statement::Statement<Loc, Loc>],
    ) -> T {
        let mut collector = ExtractedInformationCollector::new();
        for stmt in extracted_statements {
            let Ok(()) = collector.statement(stmt);
        }
        T {
            has_unwrapped_control_flow: collector.has_unwrapped_control_flow,
            async_function: collector.is_async,
            has_this_super: collector.has_this_super,
        }
    }

    pub fn collect_expression_information(
        extracted_expression: &ast::expression::Expression<Loc, Loc>,
    ) -> T {
        let mut collector = ExtractedInformationCollector::new();
        let Ok(()) = collector.expression(extracted_expression);
        T {
            has_unwrapped_control_flow: collector.has_unwrapped_control_flow,
            async_function: collector.is_async,
            has_this_super: collector.has_this_super,
        }
    }
}

pub mod refactor_program_mappers {
    use std::sync::Arc;

    use flow_parser::ast_visitor;

    use super::*;

    fn replace_statements_with_visitor<'ast, V: AstVisitor<'ast, Loc>>(
        visitor: &mut V,
        extracted_statements_loc: &Loc,
        function_call_statements: &[statement::Statement<Loc, Loc>],
        stmts: &'ast Arc<[statement::Statement<Loc, Loc>]>,
    ) -> Arc<[statement::Statement<Loc, Loc>]> {
        let mut result = Vec::new();
        for stmt in stmts.iter() {
            let statement_loc =
                flow_parser_utils::flow_ast_differ::expand_statement_comment_bounds(stmt);
            if Loc::contains(extracted_statements_loc, &statement_loc) {
                if Loc::start_loc(extracted_statements_loc) == Loc::start_loc(&statement_loc) {
                    result.extend(function_call_statements.iter().cloned());
                }
            } else {
                result.push(visitor.map_statement(stmt));
            }
        }
        Arc::from(result)
    }

    struct ExtractStatementsToFunctionMapper<'a> {
        target_body_loc: &'a Loc,
        extracted_statements_loc: &'a Loc,
        function_call_statements: &'a [statement::Statement<Loc, Loc>],
        function_declaration_statement: &'a statement::Statement<Loc, Loc>,
    }

    impl<'ast, 'a> AstVisitor<'ast, Loc> for ExtractStatementsToFunctionMapper<'a> {
        fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
            loc
        }
        fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
            type_
        }

        fn map_program(&mut self, program: &'ast ast::Program<Loc, Loc>) -> ast::Program<Loc, Loc> {
            if program.loc == *self.target_body_loc {
                let extracted_loc = self.extracted_statements_loc.dupe();
                let call_stmts: Vec<_> = self.function_call_statements.to_vec();
                let mut new_stmts = replace_statements_with_visitor(
                    self,
                    &extracted_loc,
                    &call_stmts,
                    &program.statements,
                )
                .to_vec();
                new_stmts.push(self.function_declaration_statement.dupe());
                ast::Program {
                    loc: program.loc.dupe(),
                    statements: Arc::from(new_stmts),
                    interpreter: program.interpreter.clone(),
                    comments: program.comments.clone(),
                    all_comments: program.all_comments.clone(),
                }
            } else {
                ast_visitor::map_program_default(self, program)
            }
        }
        fn map_function_body(
            &mut self,
            body: &'ast (Loc, ast::statement::Block<Loc, Loc>),
        ) -> (Loc, ast::statement::Block<Loc, Loc>) {
            let (body_loc, block) = body;
            if body_loc == self.target_body_loc {
                let extracted_loc = self.extracted_statements_loc.dupe();
                let call_stmts: Vec<_> = self.function_call_statements.to_vec();
                let mut new_stmts =
                    replace_statements_with_visitor(self, &extracted_loc, &call_stmts, &block.body)
                        .to_vec();
                new_stmts.push(self.function_declaration_statement.dupe());
                (
                    body_loc.dupe(),
                    ast::statement::Block {
                        body: Arc::from(new_stmts),
                        comments: block.comments.clone(),
                    },
                )
            } else {
                ast_visitor::map_function_body_default(self, body)
            }
        }

        fn map_statement_list(
            &mut self,
            stmts: &'ast Arc<[statement::Statement<Loc, Loc>]>,
        ) -> Arc<[statement::Statement<Loc, Loc>]> {
            let extracted_loc = self.extracted_statements_loc.dupe();
            let call_stmts: Vec<_> = self.function_call_statements.to_vec();
            replace_statements_with_visitor(self, &extracted_loc, &call_stmts, stmts)
        }

        fn map_toplevel_statement_list(
            &mut self,
            stmts: &'ast Arc<[statement::Statement<Loc, Loc>]>,
        ) -> Arc<[statement::Statement<Loc, Loc>]> {
            self.map_statement_list(stmts)
        }
    }

    pub fn extract_statements_to_function(
        target_body_loc: &Loc,
        extracted_statements_loc: &Loc,
        function_call_statements: &[statement::Statement<Loc, Loc>],
        function_declaration_statement: &statement::Statement<Loc, Loc>,
        ast: &ast::Program<Loc, Loc>,
    ) -> ast::Program<Loc, Loc> {
        let mut mapper = ExtractStatementsToFunctionMapper {
            target_body_loc,
            extracted_statements_loc,
            function_call_statements,
            function_declaration_statement,
        };
        mapper.map_program(ast)
    }

    struct ExtractStatementsToMethodMapper<'a> {
        target_body_loc: &'a Loc,
        extracted_statements_loc: &'a Loc,
        function_call_statements: &'a [statement::Statement<Loc, Loc>],
        method_declaration: &'a ast::class::BodyElement<Loc, Loc>,
    }

    impl<'ast, 'a> AstVisitor<'ast, Loc> for ExtractStatementsToMethodMapper<'a> {
        fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
            loc
        }
        fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
            type_
        }

        fn map_class_body(
            &mut self,
            cls_body: &'ast ast::class::Body<Loc, Loc>,
        ) -> ast::class::Body<Loc, Loc> {
            if cls_body.loc == *self.target_body_loc {
                let mut new_body: Vec<_> = cls_body
                    .body
                    .iter()
                    .map(|elem| self.map_class_element(elem))
                    .collect();
                new_body.push(self.method_declaration.clone());
                ast::class::Body {
                    loc: cls_body.loc.dupe(),
                    body: Arc::from(new_body),
                    comments: cls_body.comments.clone(),
                }
            } else {
                ast_visitor::map_class_body_default(self, cls_body)
            }
        }

        fn map_statement_list(
            &mut self,
            stmts: &'ast Arc<[statement::Statement<Loc, Loc>]>,
        ) -> Arc<[statement::Statement<Loc, Loc>]> {
            let extracted_loc = self.extracted_statements_loc.dupe();
            let call_stmts: Vec<_> = self.function_call_statements.to_vec();
            replace_statements_with_visitor(self, &extracted_loc, &call_stmts, stmts)
        }

        fn map_toplevel_statement_list(
            &mut self,
            stmts: &'ast Arc<[statement::Statement<Loc, Loc>]>,
        ) -> Arc<[statement::Statement<Loc, Loc>]> {
            self.map_statement_list(stmts)
        }
    }

    pub fn extract_statements_to_method(
        target_body_loc: &Loc,
        extracted_statements_loc: &Loc,
        function_call_statements: &[statement::Statement<Loc, Loc>],
        method_declaration: &ast::class::BodyElement<Loc, Loc>,
        ast: &ast::Program<Loc, Loc>,
    ) -> ast::Program<Loc, Loc> {
        let mut mapper = ExtractStatementsToMethodMapper {
            target_body_loc,
            extracted_statements_loc,
            function_call_statements,
            method_declaration,
        };
        mapper.map_program(ast)
    }

    fn replace_expression_jsx_child(
        expression_loc: &Loc,
        expression_replacement: &ast::expression::Expression<Loc, Loc>,
        child: &ast::jsx::Child<Loc, Loc>,
    ) -> ast::jsx::Child<Loc, Loc> {
        let loc = child.loc();
        if loc == expression_loc {
            match child {
                ast::jsx::Child::Element { .. } | ast::jsx::Child::Fragment { .. } => {
                    match &**expression_replacement {
                        ast::expression::ExpressionInner::JSXElement { loc, inner } => {
                            ast::jsx::Child::Element {
                                loc: loc.dupe(),
                                inner: (**inner).clone(),
                            }
                        }
                        ast::expression::ExpressionInner::JSXFragment { loc, inner } => {
                            ast::jsx::Child::Fragment {
                                loc: loc.dupe(),
                                inner: (**inner).clone(),
                            }
                        }
                        _ => ast::jsx::Child::ExpressionContainer {
                            loc: loc.dupe(),
                            inner: ast::jsx::ExpressionContainer {
                                expression: ast::jsx::expression_container::Expression::Expression(
                                    expression_replacement.dupe(),
                                ),
                                comments: None,
                            },
                        },
                    }
                }
                _ => child.clone(),
            }
        } else {
            child.clone()
        }
    }

    struct ExtractExpressionToReactComponentMapper<'a> {
        expression_loc: &'a Loc,
        expression_replacement: &'a ast::expression::Expression<Loc, Loc>,
        component_declaration_statement: &'a statement::Statement<Loc, Loc>,
    }

    impl<'ast, 'a> AstVisitor<'ast, Loc> for ExtractExpressionToReactComponentMapper<'a> {
        fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
            loc
        }
        fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
            type_
        }

        fn map_expression(
            &mut self,
            expr: &'ast ast::expression::Expression<Loc, Loc>,
        ) -> ast::expression::Expression<Loc, Loc> {
            let expr_loc = expr.loc();
            if expr_loc == self.expression_loc {
                self.expression_replacement.dupe()
            } else {
                ast_visitor::map_expression_default(self, expr)
            }
        }

        fn map_jsx_child(
            &mut self,
            child: &'ast ast::jsx::Child<Loc, Loc>,
        ) -> ast::jsx::Child<Loc, Loc> {
            replace_expression_jsx_child(self.expression_loc, self.expression_replacement, child)
        }

        fn map_program(&mut self, program: &'ast ast::Program<Loc, Loc>) -> ast::Program<Loc, Loc> {
            let mut new_stmts: Vec<_> = self
                .map_toplevel_statement_list(&program.statements)
                .to_vec();
            new_stmts.push(self.component_declaration_statement.dupe());
            ast::Program {
                loc: program.loc.dupe(),
                statements: Arc::from(new_stmts),
                interpreter: program.interpreter.clone(),
                comments: program.comments.clone(),
                all_comments: program.all_comments.clone(),
            }
        }
    }

    pub fn extract_expression_to_react_component(
        expression_loc: &Loc,
        expression_replacement: &ast::expression::Expression<Loc, Loc>,
        component_declaration_statement: &statement::Statement<Loc, Loc>,
        ast: &ast::Program<Loc, Loc>,
    ) -> ast::Program<Loc, Loc> {
        let mut mapper = ExtractExpressionToReactComponentMapper {
            expression_loc,
            expression_replacement,
            component_declaration_statement,
        };
        mapper.map_program(ast)
    }

    struct ExtractExpressionToConstantMapper<'a> {
        statement_loc: &'a Loc,
        expression_loc: &'a Loc,
        expression_replacement: &'a ast::expression::Expression<Loc, Loc>,
        constant_definition: &'a statement::Statement<Loc, Loc>,
    }

    impl<'ast, 'a> AstVisitor<'ast, Loc> for ExtractExpressionToConstantMapper<'a> {
        fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
            loc
        }
        fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
            type_
        }

        fn map_expression(
            &mut self,
            expr: &'ast ast::expression::Expression<Loc, Loc>,
        ) -> ast::expression::Expression<Loc, Loc> {
            let expr_loc = expr.loc();
            if expr_loc == self.expression_loc {
                self.expression_replacement.dupe()
            } else {
                ast_visitor::map_expression_default(self, expr)
            }
        }

        fn map_jsx_child(
            &mut self,
            child: &'ast ast::jsx::Child<Loc, Loc>,
        ) -> ast::jsx::Child<Loc, Loc> {
            replace_expression_jsx_child(self.expression_loc, self.expression_replacement, child)
        }

        fn map_statement_list(
            &mut self,
            stmts: &'ast Arc<[statement::Statement<Loc, Loc>]>,
        ) -> Arc<[statement::Statement<Loc, Loc>]> {
            let mut result = Vec::new();
            for stmt in stmts.iter() {
                let stmt_loc = stmt.loc();
                if stmt_loc == self.statement_loc {
                    result.push(self.constant_definition.dupe());
                    result.push(self.map_statement(stmt));
                } else {
                    result.push(self.map_statement(stmt));
                }
            }
            Arc::from(result)
        }

        fn map_toplevel_statement_list(
            &mut self,
            stmts: &'ast Arc<[statement::Statement<Loc, Loc>]>,
        ) -> Arc<[statement::Statement<Loc, Loc>]> {
            self.map_statement_list(stmts)
        }
    }

    pub fn extract_expression_to_constant(
        statement_loc: &Loc,
        expression_loc: &Loc,
        expression_replacement: &ast::expression::Expression<Loc, Loc>,
        constant_definition: &statement::Statement<Loc, Loc>,
        ast: &ast::Program<Loc, Loc>,
    ) -> ast::Program<Loc, Loc> {
        let mut mapper = ExtractExpressionToConstantMapper {
            statement_loc,
            expression_loc,
            expression_replacement,
            constant_definition,
        };
        mapper.map_program(ast)
    }

    struct ExtractExpressionToClassFieldMapper<'a> {
        class_body_loc: &'a Loc,
        expression_loc: &'a Loc,
        expression_replacement: &'a ast::expression::Expression<Loc, Loc>,
        field_definition: &'a ast::class::BodyElement<Loc, Loc>,
    }

    impl<'ast, 'a> AstVisitor<'ast, Loc> for ExtractExpressionToClassFieldMapper<'a> {
        fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
            loc
        }
        fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
            type_
        }

        fn map_expression(
            &mut self,
            expr: &'ast ast::expression::Expression<Loc, Loc>,
        ) -> ast::expression::Expression<Loc, Loc> {
            let expr_loc = expr.loc();
            if expr_loc == self.expression_loc {
                self.expression_replacement.dupe()
            } else {
                ast_visitor::map_expression_default(self, expr)
            }
        }

        fn map_jsx_child(
            &mut self,
            child: &'ast ast::jsx::Child<Loc, Loc>,
        ) -> ast::jsx::Child<Loc, Loc> {
            replace_expression_jsx_child(self.expression_loc, self.expression_replacement, child)
        }

        fn map_class_body(
            &mut self,
            cls_body: &'ast ast::class::Body<Loc, Loc>,
        ) -> ast::class::Body<Loc, Loc> {
            if cls_body.loc == *self.class_body_loc {
                let mut new_body = vec![self.field_definition.clone()];
                new_body.extend(
                    cls_body
                        .body
                        .iter()
                        .map(|elem| self.map_class_element(elem)),
                );
                ast::class::Body {
                    loc: cls_body.loc.dupe(),
                    body: Arc::from(new_body),
                    comments: cls_body.comments.clone(),
                }
            } else {
                ast_visitor::map_class_body_default(self, cls_body)
            }
        }
    }

    pub fn extract_expression_to_class_field(
        class_body_loc: &Loc,
        expression_loc: &Loc,
        expression_replacement: &ast::expression::Expression<Loc, Loc>,
        field_definition: &ast::class::BodyElement<Loc, Loc>,
        ast: &ast::Program<Loc, Loc>,
    ) -> ast::Program<Loc, Loc> {
        let mut mapper = ExtractExpressionToClassFieldMapper {
            class_body_loc,
            expression_loc,
            expression_replacement,
            field_definition,
        };
        mapper.map_program(ast)
    }

    struct ExtractTypeToTypeAliasMapper<'a> {
        statement_loc: &'a Loc,
        type_loc: &'a Loc,
        type_replacement: &'a ast::types::Type<Loc, Loc>,
        type_alias: &'a statement::Statement<Loc, Loc>,
    }

    impl<'ast, 'a> AstVisitor<'ast, Loc> for ExtractTypeToTypeAliasMapper<'a> {
        fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
            loc
        }
        fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
            type_
        }

        fn map_type_(&mut self, t: &'ast ast::types::Type<Loc, Loc>) -> ast::types::Type<Loc, Loc> {
            let loc = t.loc();
            if loc == self.type_loc {
                self.type_replacement.dupe()
            } else {
                ast_visitor::map_type_default(self, t)
            }
        }

        fn map_statement_list(
            &mut self,
            stmts: &'ast Arc<[statement::Statement<Loc, Loc>]>,
        ) -> Arc<[statement::Statement<Loc, Loc>]> {
            let mut result = Vec::new();
            for stmt in stmts.iter() {
                let stmt_loc = stmt.loc();
                if stmt_loc == self.statement_loc {
                    result.push(self.type_alias.dupe());
                    result.push(self.map_statement(stmt));
                } else {
                    result.push(self.map_statement(stmt));
                }
            }
            Arc::from(result)
        }

        fn map_toplevel_statement_list(
            &mut self,
            stmts: &'ast Arc<[statement::Statement<Loc, Loc>]>,
        ) -> Arc<[statement::Statement<Loc, Loc>]> {
            self.map_statement_list(stmts)
        }
    }

    pub fn extract_type_to_type_alias(
        statement_loc: &Loc,
        type_loc: &Loc,
        type_replacement: &ast::types::Type<Loc, Loc>,
        type_alias: &statement::Statement<Loc, Loc>,
        ast: &ast::Program<Loc, Loc>,
    ) -> ast::Program<Loc, Loc> {
        let mut mapper = ExtractTypeToTypeAliasMapper {
            statement_loc,
            type_loc,
            type_replacement,
            type_alias,
        };
        mapper.map_program(ast)
    }
}

pub mod variable_analysis {
    use super::*;

    struct IdentifierCollector {
        acc: BTreeSet<FlowSmolStr>,
    }

    impl IdentifierCollector {
        fn new() -> Self {
            Self {
                acc: BTreeSet::new(),
            }
        }
    }

    impl<'ast> AstVisitor<'ast, Loc> for IdentifierCollector {
        fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
            loc
        }

        fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
            type_
        }

        fn identifier(&mut self, id: &'ast ast::Identifier<Loc, Loc>) -> Result<(), !> {
            self.acc.insert(id.name.dupe());
            Ok(())
        }
    }

    pub fn collect_used_names(ast: &ast::Program<Loc, Loc>) -> BTreeSet<FlowSmolStr> {
        let mut collector = IdentifierCollector::new();
        let Ok(()) = collector.program(ast);
        collector.acc
    }

    #[derive(Debug, Clone)]
    pub struct RelevantDefs {
        pub defs_with_scopes_of_local_uses: Vec<(scope_api::Def<Loc>, scope_api::Scope<Loc>)>,
        pub vars_with_shadowed_local_reassignments: Vec<(FlowSmolStr, Loc)>,
    }

    pub fn collect_relevant_defs_with_scope(
        scope_info: &ScopeInfo<Loc>,
        ssa_values: &ssa_api::Values<Loc>,
        extracted_loc: &Loc,
    ) -> RelevantDefs {
        let mut used_defs: BTreeSet<scope_api::Def<Loc>> = BTreeSet::new();
        let mut shadowed_local_reassignments: BTreeMap<FlowSmolStr, Loc> = BTreeMap::new();

        for scope in scope_info.scopes.values() {
            for (use_loc, def) in &scope.locals {
                if Loc::contains(extracted_loc, use_loc) {
                    used_defs.insert(def.clone());
                } else {
                    let def_loc = def.locs.first();
                    if !Loc::contains(extracted_loc, def_loc) {
                        let has_local_reassignment = match ssa_values.get(use_loc) {
                            None => false,
                            Some(writes) => writes.iter().any(|w| match w {
                                ssa_api::WriteLoc::Uninitialized => false,
                                ssa_api::WriteLoc::Write(reason) => {
                                    let write_loc = reason.loc();
                                    Loc::contains(extracted_loc, write_loc)
                                }
                            }),
                        };
                        if has_local_reassignment {
                            shadowed_local_reassignments
                                .insert(def.actual_name.dupe(), def_loc.dupe());
                        }
                    }
                }
            }
        }

        let mut defs_with_scopes: BTreeMap<
            scope_api::Def<Loc>,
            (scope_api::Def<Loc>, scope_api::Scope<Loc>),
        > = BTreeMap::new();
        for scope in scope_info.scopes.values() {
            for def in scope.defs.values() {
                if used_defs.contains(def) {
                    defs_with_scopes.insert(def.clone(), (def.clone(), scope.clone()));
                }
            }
        }
        let defs_with_scopes_of_local_uses: Vec<_> = defs_with_scopes.into_values().collect();

        let vars_with_shadowed_local_reassignments: Vec<_> =
            shadowed_local_reassignments.into_iter().collect();

        RelevantDefs {
            defs_with_scopes_of_local_uses,
            vars_with_shadowed_local_reassignments,
        }
    }

    pub fn undefined_variables_after_extraction(
        scope_info: &ScopeInfo<Loc>,
        defs_with_scopes_of_local_uses: &[(scope_api::Def<Loc>, scope_api::Scope<Loc>)],
        new_function_target_scope_loc: Option<&Loc>,
        extracted_loc: &Loc,
    ) -> Vec<(FlowSmolStr, Loc)> {
        let new_function_target_scopes = match new_function_target_scope_loc {
            Some(scope_loc) => scope_info.scope_of_loc(scope_loc),
            None => scope_info
                .scopes
                .keys()
                .next()
                .copied()
                .into_iter()
                .collect(),
        };

        let to_undefined_variable =
            |(def, def_scope): &(scope_api::Def<Loc>, scope_api::Scope<Loc>)| {
                let def_loc = def.locs.first();
                let actual_name = &def.actual_name;
                if Loc::contains(extracted_loc, def_loc) {
                    None
                } else {
                    let all_within = new_function_target_scopes
                        .iter()
                        .all(|function_scope| scope_info.scope_within(*function_scope, def_scope));
                    if all_within {
                        Some((actual_name.dupe(), def_loc.dupe()))
                    } else {
                        None
                    }
                }
            };

        defs_with_scopes_of_local_uses
            .iter()
            .filter_map(to_undefined_variable)
            .collect()
    }

    #[derive(Debug, Clone)]
    pub struct EscapingDefinitions {
        pub escaping_variables: Vec<(FlowSmolStr, Loc)>,
        pub has_external_writes: bool,
    }

    pub fn collect_escaping_local_defs(
        scope_info: &ScopeInfo<Loc>,
        ssa_values: &ssa_api::Values<Loc>,
        extracted_statements_loc: &Loc,
    ) -> EscapingDefinitions {
        let mut escaping_vars: BTreeMap<FlowSmolStr, Loc> = BTreeMap::new();
        let mut has_external_writes = false;

        for scope in scope_info.scopes.values() {
            for (use_loc, def) in &scope.locals {
                let def_loc = def.locs.first();
                if Loc::contains(extracted_statements_loc, def_loc)
                    && !Loc::contains(extracted_statements_loc, use_loc)
                {
                    escaping_vars.insert(def.actual_name.dupe(), def_loc.dupe());
                    has_external_writes = has_external_writes
                        || match ssa_values.get(use_loc) {
                            None => true,

                            Some(write_locs) => write_locs.iter().any(|w| match w {
                                ssa_api::WriteLoc::Uninitialized => false,
                                ssa_api::WriteLoc::Write(reason) => {
                                    let write_loc = reason.loc();
                                    !Loc::contains(extracted_statements_loc, write_loc)
                                }
                            }),
                        };
                }
            }
        }

        EscapingDefinitions {
            escaping_variables: escaping_vars.into_iter().collect(),
            has_external_writes,
        }
    }
}

pub mod type_synthesizer {
    use super::*;

    struct GenericNameCollector;

    impl
        flow_typing_visitors::type_visitor::TypeVisitor<
            BTreeSet<flow_common::subst_name::SubstName>,
        > for GenericNameCollector
    {
        fn type_<'cx>(
            &mut self,
            cx: &mut flow_typing_context::Context<'cx>,
            pole: flow_common::polarity::Polarity,
            acc: BTreeSet<flow_common::subst_name::SubstName>,
            t: &flow_typing_type::type_::Type,
        ) -> BTreeSet<flow_common::subst_name::SubstName> {
            use flow_typing_type::type_::TypeInner;
            match &**t {
                TypeInner::GenericT(box flow_typing_type::type_::GenericTData { id, .. }) => id
                    .fold_ids(
                        |_aloc_id, name, mut acc| {
                            acc.insert(name.dupe());
                            acc
                        },
                        acc,
                    ),
                _ => flow_typing_visitors::type_visitor::type_default(self, cx, pole, acc, t),
            }
        }
    }

    fn keep_used_tparam_rev(
        cx: &mut flow_typing_context::Context,
        tparams_rev: &[flow_typing_type::type_::TypeParam],
        type_: &flow_typing_type::type_::Type,
    ) -> Vec<flow_typing_type::type_::TypeParam> {
        use flow_common::polarity::Polarity;
        use flow_typing_visitors::type_visitor::TypeVisitor;

        let mut collector = GenericNameCollector;
        let mut collect_used_generic_names =
            |t: &flow_typing_type::type_::Type,
             acc: BTreeSet<flow_common::subst_name::SubstName>|
             -> BTreeSet<flow_common::subst_name::SubstName> {
                collector.type_(cx, Polarity::Neutral, acc, t)
            };
        let mut appeared_generic_names = collect_used_generic_names(type_, BTreeSet::new());
        let mut tparams = Vec::new();
        for tparam in tparams_rev {
            if appeared_generic_names.contains(&tparam.name) {
                appeared_generic_names =
                    collect_used_generic_names(&tparam.bound, appeared_generic_names);
                if let Some(default) = &tparam.default {
                    appeared_generic_names =
                        collect_used_generic_names(default, appeared_generic_names);
                }
                tparams.push(tparam.dupe());
            }
        }
        tparams
    }

    #[derive(Clone)]
    pub struct SynthesizerContext<'a> {
        pub file: FileKey,
        pub file_sig: std::sync::Arc<FileSig>,
        pub typed_ast: ast::Program<ALoc, (ALoc, flow_typing_type::type_::Type)>,
        pub loc_of_aloc: &'a dyn Fn(&ALoc) -> Loc,
        pub get_ast_from_heap: &'a dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>,
        pub get_haste_module_info:
            &'a dyn Fn(&FileKey) -> Option<flow_common_modulename::HasteModuleInfo>,
        pub get_type_sig: &'a dyn Fn(
            &FileKey,
        ) -> Option<
            flow_type_sig::packed_type_sig::Module<
                flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
            >,
        >,
        pub type_at_loc_map: BTreeMap<
            Loc,
            (
                Vec<flow_typing_type::type_::TypeParam>,
                flow_typing_type::type_::Type,
            ),
        >,
    }

    struct TypeCollector<'a> {
        loc_of_aloc: &'a dyn Fn(&ALoc) -> Loc,
        locs: &'a BTreeSet<Loc>,
        bound_tparams: Vec<flow_typing_type::type_::TypeParam>,
        acc: BTreeMap<
            Loc,
            (
                Vec<flow_typing_type::type_::TypeParam>,
                flow_typing_type::type_::Type,
            ),
        >,
    }

    impl<'a> TypeCollector<'a> {
        fn new(loc_of_aloc: &'a dyn Fn(&ALoc) -> Loc, locs: &'a BTreeSet<Loc>) -> Self {
            Self {
                loc_of_aloc,
                locs,
                bound_tparams: Vec::new(),
                acc: BTreeMap::new(),
            }
        }

        fn collect_type_at_loc(&mut self, loc: Loc, t: flow_typing_type::type_::Type) {
            self.acc.insert(loc, (self.bound_tparams.clone(), t));
        }
    }

    impl<'ast, 'a> AstVisitor<'ast, ALoc, (ALoc, flow_typing_type::type_::Type), &'ast ALoc, !>
        for TypeCollector<'a>
    {
        fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
            loc
        }

        fn normalize_type(type_: &'ast (ALoc, flow_typing_type::type_::Type)) -> &'ast ALoc {
            &type_.0
        }

        fn identifier(
            &mut self,
            id: &'ast ast::Identifier<ALoc, (ALoc, flow_typing_type::type_::Type)>,
        ) -> Result<(), !> {
            let (aloc, t) = &id.loc;
            let loc = (self.loc_of_aloc)(aloc);
            if self.locs.contains(&loc) {
                self.collect_type_at_loc(loc, t.dupe());
            }
            Ok(())
        }

        fn expression(
            &mut self,
            expr: &'ast ast::expression::Expression<ALoc, (ALoc, flow_typing_type::type_::Type)>,
        ) -> Result<(), !> {
            let (aloc, type_) = expr.loc();
            let loc = (self.loc_of_aloc)(aloc);
            if self.locs.contains(&loc) {
                self.collect_type_at_loc(loc, type_.dupe());
            }
            flow_parser::ast_visitor::expression_default(self, expr)
        }

        fn type_(
            &mut self,
            t: &'ast ast::types::Type<ALoc, (ALoc, flow_typing_type::type_::Type)>,
        ) -> Result<(), !> {
            let (aloc, type_) = t.loc();
            let loc = (self.loc_of_aloc)(aloc);
            if self.locs.contains(&loc) {
                self.collect_type_at_loc(loc, type_.dupe());
            }
            flow_parser::ast_visitor::type_default(self, t)
        }

        fn type_param(
            &mut self,
            kind: &flow_parser::ast_visitor::TypeParamsContext,
            tparam: &'ast ast::types::TypeParam<ALoc, (ALoc, flow_typing_type::type_::Type)>,
        ) -> Result<(), !> {
            let res = flow_parser::ast_visitor::type_param_default(self, kind, tparam);
            let tp =
                crate::ast_extraction_utils::insertion_point_collectors::make_typeparam(tparam);
            self.bound_tparams.insert(0, tp);
            res
        }

        fn function_(
            &mut self,
            loc: &'ast ALoc,
            func: &'ast ast::function::Function<ALoc, (ALoc, flow_typing_type::type_::Type)>,
        ) -> Result<(), !> {
            let originally_bound_tparams = self.bound_tparams.clone();
            let res = flow_parser::ast_visitor::function_default(self, loc, func);
            self.bound_tparams = originally_bound_tparams;
            res
        }

        fn class_(
            &mut self,
            loc: &'ast ALoc,
            cls: &'ast ast::class::Class<ALoc, (ALoc, flow_typing_type::type_::Type)>,
        ) -> Result<(), !> {
            let this_tparam =
                crate::ast_extraction_utils::insertion_point_collectors::make_class_this(cls);
            let originally_bound_tparams = self.bound_tparams.clone();
            self.bound_tparams.insert(0, this_tparam);
            let res = flow_parser::ast_visitor::class_default(self, loc, cls);
            self.bound_tparams = originally_bound_tparams;
            res
        }
    }

    pub fn create_synthesizer_context<'a>(
        file: FileKey,
        file_sig: std::sync::Arc<FileSig>,
        typed_ast: ast::Program<ALoc, (ALoc, flow_typing_type::type_::Type)>,
        loc_of_aloc: &'a dyn Fn(&ALoc) -> Loc,
        get_ast_from_heap: &'a dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>,
        get_haste_module_info: &'a dyn Fn(
            &FileKey,
        )
            -> Option<flow_common_modulename::HasteModuleInfo>,
        get_type_sig: &'a dyn Fn(
            &FileKey,
        ) -> Option<
            flow_type_sig::packed_type_sig::Module<
                flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
            >,
        >,
        locs: &BTreeSet<Loc>,
    ) -> SynthesizerContext<'a> {
        let mut collector = TypeCollector::new(loc_of_aloc, locs);
        let Ok(()) = collector.program(&typed_ast);
        let type_at_loc_map = collector.acc;
        SynthesizerContext {
            file,
            file_sig,
            typed_ast,
            loc_of_aloc,
            get_ast_from_heap,
            get_haste_module_info,
            get_type_sig,
            type_at_loc_map,
        }
    }

    pub struct TypeSynthesizerWithImportAdder<'a> {
        pub type_param_synthesizer: Box<
            dyn Fn(
                    &mut Context<'_>,
                    &flow_typing_type::type_::TypeParam,
                )
                    -> Result<ast::types::TypeParam<Loc, Loc>, insert_type::Expected>
                + 'a,
        >,
        pub type_synthesizer: Box<
            dyn Fn(
                    &mut Context<'_>,
                    &Loc,
                ) -> Result<
                    Option<(
                        Vec<flow_typing_type::type_::TypeParam>,
                        ast::types::Type<Loc, Loc>,
                    )>,
                    insert_type::Expected,
                > + 'a,
        >,
        pub added_imports: Box<dyn Fn() -> Vec<(String, autofix_imports::Bindings)> + 'a>,
    }

    pub fn create_type_synthesizer_with_import_adder<'a>(
        cx: &Context<'_>,
        synthesizer_context: SynthesizerContext<'a>,
    ) -> TypeSynthesizerWithImportAdder<'a> {
        use std::cell::RefCell;
        use std::rc::Rc;

        use crate::insert_type_imports;

        let SynthesizerContext {
            file,
            file_sig,
            typed_ast,
            loc_of_aloc,
            get_ast_from_heap,
            get_haste_module_info,
            get_type_sig,
            type_at_loc_map,
        } = synthesizer_context;

        let file_options = cx.file_options();
        let remote_converter = Rc::new(RefCell::new(
            insert_type_imports::imports_helper::RemoteConverter::new(
                Box::new(|aloc| loc_of_aloc(aloc)),
                file_options,
                Box::new(|fk| get_haste_module_info(fk)),
                Box::new(|fk| get_type_sig(fk)),
                0,
                file,
                BTreeSet::new(),
            ),
        ));

        let file_sig_clone = file_sig.dupe();
        let remote_converter_clone = remote_converter.dupe();
        let synth_type =
            move |cx: &mut Context<'_>,
                  loc: Loc,
                  t: flow_typing_type::type_::Type|
                  -> Result<(Loc, ast::types::Type<Loc, Loc>), insert_type::Expected> {
                let mut rc = remote_converter_clone.borrow_mut();
                insert_type::synth_type(
                    None,
                    cx,
                    loc_of_aloc,
                    get_ast_from_heap,
                    &file_sig_clone,
                    &typed_ast,
                    false,
                    &mut rc,
                    loc,
                    t,
                )
            };

        let synth_type = Rc::new(synth_type);

        let synth_type_for_tparam = synth_type.dupe();
        let type_param_synthesizer: Box<
            dyn Fn(
                &mut Context<'_>,
                &flow_typing_type::type_::TypeParam,
            ) -> Result<ast::types::TypeParam<Loc, Loc>, insert_type::Expected>,
        > = Box::new(
            move |cx, tparam| -> Result<ast::types::TypeParam<Loc, Loc>, insert_type::Expected> {
                use flow_common::polarity::Polarity;
                use flow_parser_utils::ast_builder;

                let name = &tparam.name;
                let name_str = name.string_of_subst_name();

                let bound = {
                    let (_, synth_bound) =
                        synth_type_for_tparam(cx, Loc::default(), tparam.bound.dupe())?;
                    match &*synth_bound {
                        ast::types::TypeInner::Mixed { .. }
                        | ast::types::TypeInner::Unknown { .. } => None,
                        _ => Some(ast::types::AnnotationOrHint::Available(
                            ast::types::Annotation {
                                loc: synth_bound.loc().dupe(),
                                annotation: synth_bound,
                            },
                        )),
                    }
                };

                let variance = match tparam.polarity {
                    None | Some(Polarity::Neutral) => None,
                    Some(Polarity::Positive) => Some(ast::Variance {
                        loc: Loc::default(),
                        kind: ast::VarianceKind::Out,
                        comments: None,
                    }),
                    Some(Polarity::Negative) => Some(ast::Variance {
                        loc: Loc::default(),
                        kind: ast::VarianceKind::In,
                        comments: None,
                    }),
                };

                let default = match &tparam.default {
                    None => None,
                    Some(default_t) => {
                        let (_, ast) = synth_type_for_tparam(cx, Loc::default(), default_t.dupe())?;
                        Some(ast)
                    }
                };

                Ok(ast_builder::types::type_param(
                    None,
                    bound,
                    None,
                    variance,
                    default,
                    None,
                    name_str.as_str(),
                ))
            },
        );

        let synth_type_for_type = synth_type.dupe();
        let type_synthesizer: Box<
            dyn Fn(
                &mut Context<'_>,
                &Loc,
            ) -> Result<
                Option<(
                    Vec<flow_typing_type::type_::TypeParam>,
                    ast::types::Type<Loc, Loc>,
                )>,
                insert_type::Expected,
            >,
        > = Box::new(
            move |cx,
                  loc|
                  -> Result<
                Option<(
                    Vec<flow_typing_type::type_::TypeParam>,
                    ast::types::Type<Loc, Loc>,
                )>,
                insert_type::Expected,
            > {
                match type_at_loc_map.get(loc) {
                    None => Ok(None),
                    Some((tparams_rev, type_)) => {
                        let (_, ast_type) = synth_type_for_type(cx, loc.dupe(), type_.dupe())?;
                        let kept = keep_used_tparam_rev(cx, tparams_rev, type_);
                        Ok(Some((kept, ast_type)))
                    }
                }
            },
        );

        let remote_converter_for_imports = remote_converter.dupe();
        TypeSynthesizerWithImportAdder {
            type_param_synthesizer,
            type_synthesizer,
            added_imports: Box::new(move || {
                remote_converter_for_imports.borrow().to_import_bindings()
            }),
        }
    }
}
