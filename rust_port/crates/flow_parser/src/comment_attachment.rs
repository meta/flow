/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::sync::Arc;

use dupe::Dupe;

use crate::ast;
use crate::ast_utils;
use crate::ast_visitor::AstVisitor;
use crate::ast_visitor::TypeParamsContext;
use crate::loc::LOC_NONE;
use crate::loc::Loc;
use crate::loc::Position;
use crate::parser_env::ParserEnv;
use crate::parser_env::eat;
use crate::parser_env::peek;

// Mapper that removes all trailing comments that appear after a given position in an AST node
pub(super) struct TrailingCommentsRemover {
    after_pos: Position,
}

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, !> for TrailingCommentsRemover {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn map_syntax<Internal: Dupe>(
        &mut self,
        syntax: &'ast ast::Syntax<Loc, Internal>,
    ) -> ast::Syntax<Loc, Internal>
    where
        Loc: Dupe,
    {
        let filtered: Vec<_> = syntax
            .trailing
            .iter()
            .filter(|c| c.loc.start < self.after_pos)
            .cloned()
            .collect();
        ast::Syntax {
            leading: syntax.leading.dupe(),
            trailing: Arc::from(filtered),
            internal: syntax.internal.clone(),
        }
    }

    fn map_array(
        &mut self,
        _loc: &'ast Loc,
        expr: &'ast ast::expression::Array<Loc, Loc>,
    ) -> ast::expression::Array<Loc, Loc> {
        ast::expression::Array {
            elements: expr.elements.dupe(),
            trailing_comma: expr.trailing_comma,
            comments: self.map_syntax_opt(expr.comments.as_ref()),
        }
    }

    fn map_array_type(
        &mut self,
        _loc: &'ast Loc,
        t: &'ast ast::types::Array<Loc, Loc>,
    ) -> ast::types::Array<Loc, Loc> {
        ast::types::Array {
            argument: t.argument.clone(),
            comments: self.map_syntax_opt(t.comments.as_ref()),
        }
    }

    fn map_assignment(
        &mut self,
        _loc: &'ast Loc,
        expr: &'ast ast::expression::Assignment<Loc, Loc>,
    ) -> ast::expression::Assignment<Loc, Loc> {
        let right = self.map_expression(&expr.right);
        ast::expression::Assignment {
            operator: expr.operator,
            left: expr.left.clone(),
            right,
            comments: self.map_syntax_opt(expr.comments.as_ref()),
        }
    }

    fn map_binary(
        &mut self,
        _loc: &'ast Loc,
        expr: &'ast ast::expression::Binary<Loc, Loc>,
    ) -> ast::expression::Binary<Loc, Loc> {
        let right = self.map_expression(&expr.right);
        ast::expression::Binary {
            operator: expr.operator,
            left: expr.left.clone(),
            right,
            comments: self.map_syntax_opt(expr.comments.as_ref()),
        }
    }

    fn map_block(
        &mut self,
        _loc: &'ast Loc,
        stmt: &'ast ast::statement::Block<Loc, Loc>,
    ) -> ast::statement::Block<Loc, Loc> {
        ast::statement::Block {
            body: stmt.body.dupe(),
            comments: self.map_syntax_opt(stmt.comments.as_ref()),
        }
    }

    fn map_call(
        &mut self,
        _loc: &'ast Loc,
        expr: &'ast ast::expression::Call<Loc, Loc>,
    ) -> ast::expression::Call<Loc, Loc> {
        let arguments = self.map_arg_list(&expr.arguments);
        ast::expression::Call {
            callee: expr.callee.clone(),
            targs: expr.targs.clone(),
            arguments,
            comments: self.map_syntax_opt(expr.comments.as_ref()),
        }
    }

    fn map_arg_list(
        &mut self,
        arg_list: &'ast ast::expression::ArgList<Loc, Loc>,
    ) -> ast::expression::ArgList<Loc, Loc> {
        ast::expression::ArgList {
            loc: arg_list.loc.dupe(),
            arguments: arg_list.arguments.dupe(),
            comments: self.map_syntax_opt(arg_list.comments.as_ref()),
        }
    }

    fn map_call_type_args(
        &mut self,
        targs: &'ast ast::expression::CallTypeArgs<Loc, Loc>,
    ) -> ast::expression::CallTypeArgs<Loc, Loc> {
        ast::expression::CallTypeArgs {
            loc: targs.loc.dupe(),
            arguments: targs.arguments.dupe(),
            comments: self.map_syntax_opt(targs.comments.as_ref()),
        }
    }

    fn map_class_(
        &mut self,
        _loc: &'ast Loc,
        cls: &'ast ast::class::Class<Loc, Loc>,
    ) -> ast::class::Class<Loc, Loc> {
        let body = self.map_class_body(&cls.body);
        ast::class::Class {
            id: cls.id.dupe(),
            body,
            tparams: cls.tparams.clone(),
            extends: cls.extends.clone(),
            implements: cls.implements.clone(),
            class_decorators: cls.class_decorators.dupe(),
            abstract_: cls.abstract_,
            comments: self.map_syntax_opt(cls.comments.as_ref()),
        }
    }

    fn map_class_body(
        &mut self,
        body: &'ast ast::class::Body<Loc, Loc>,
    ) -> ast::class::Body<Loc, Loc> {
        ast::class::Body {
            loc: body.loc.dupe(),
            body: body.body.dupe(),
            comments: self.map_syntax_opt(body.comments.as_ref()),
        }
    }

    fn map_class_extends(
        &mut self,
        extends: &'ast ast::class::Extends<Loc, Loc>,
    ) -> ast::class::Extends<Loc, Loc> {
        if extends.targs.is_none() {
            ast::class::Extends {
                loc: extends.loc.dupe(),
                expr: self.map_expression(&extends.expr),
                targs: None,
                comments: extends.comments.dupe(),
            }
        } else {
            ast::class::Extends {
                loc: extends.loc.dupe(),
                expr: extends.expr.dupe(),
                targs: extends.targs.as_ref().map(|t| self.map_type_args(t)),
                comments: extends.comments.dupe(),
            }
        }
    }

    fn map_class_implements(
        &mut self,
        implements: &'ast ast::class::Implements<Loc, Loc>,
    ) -> ast::class::Implements<Loc, Loc> {
        let interfaces = if let Some(last) = implements.interfaces.last() {
            let mut new_interfaces: Vec<_> = implements.interfaces.iter().cloned().collect();
            let new_last = self.map_class_implements_interface(last);
            if let Some(last_ref) = new_interfaces.last_mut() {
                *last_ref = new_last;
            }
            Arc::from(new_interfaces)
        } else {
            implements.interfaces.dupe()
        };
        ast::class::Implements {
            loc: implements.loc.dupe(),
            interfaces,
            comments: implements.comments.dupe(),
        }
    }

    fn map_class_implements_interface(
        &mut self,
        interface: &'ast ast::class::implements::Interface<Loc, Loc>,
    ) -> ast::class::implements::Interface<Loc, Loc> {
        if interface.targs.is_some() {
            ast::class::implements::Interface {
                loc: interface.loc.dupe(),
                id: interface.id.clone(),
                targs: interface.targs.as_ref().map(|t| self.map_type_args(t)),
            }
        } else {
            ast::class::implements::Interface {
                loc: interface.loc.dupe(),
                id: self.map_generic_identifier_type(&interface.id),
                targs: None,
            }
        }
    }

    fn map_component_declaration(
        &mut self,
        _loc: &'ast Loc,
        component: &'ast ast::statement::ComponentDeclaration<Loc, Loc>,
    ) -> ast::statement::ComponentDeclaration<Loc, Loc> {
        let body = component
            .body
            .as_ref()
            .map(|(loc, block)| (loc.dupe(), self.map_block(loc, block)));
        ast::statement::ComponentDeclaration {
            id: component.id.dupe(),
            tparams: component.tparams.clone(),
            params: component.params.clone(),
            renders: component.renders.clone(),
            body,
            comments: self.map_syntax_opt(component.comments.as_ref()),
            sig_loc: component.sig_loc.dupe(),
            async_: component.async_,
        }
    }

    fn map_component_params(
        &mut self,
        params: &'ast ast::statement::component_params::Params<Loc, Loc>,
    ) -> ast::statement::component_params::Params<Loc, Loc> {
        ast::statement::component_params::Params {
            loc: params.loc.dupe(),
            params: params.params.dupe(),
            rest: params.rest.clone(),
            comments: self.map_syntax_opt(params.comments.as_ref()),
        }
    }

    fn map_computed_key(
        &mut self,
        key: &'ast ast::ComputedKey<Loc, Loc>,
    ) -> ast::ComputedKey<Loc, Loc> {
        ast::ComputedKey {
            loc: key.loc.dupe(),
            expression: key.expression.clone(),
            comments: self.map_syntax_opt(key.comments.as_ref()),
        }
    }

    fn map_conditional(
        &mut self,
        _loc: &'ast Loc,
        expr: &'ast ast::expression::Conditional<Loc, Loc>,
    ) -> ast::expression::Conditional<Loc, Loc> {
        let alternate = self.map_expression(&expr.alternate);
        ast::expression::Conditional {
            test: expr.test.clone(),
            consequent: expr.consequent.clone(),
            alternate,
            comments: self.map_syntax_opt(expr.comments.as_ref()),
        }
    }

    fn map_function_(
        &mut self,
        _loc: &'ast Loc,
        func: &'ast ast::function::Function<Loc, Loc>,
    ) -> ast::function::Function<Loc, Loc> {
        let body = self.map_function_body_any(&func.body);
        ast::function::Function {
            id: func.id.dupe(),
            params: func.params.clone(),
            body,
            async_: func.async_,
            generator: func.generator,
            predicate: func.predicate.clone(),
            return_: func.return_.clone(),
            tparams: func.tparams.clone(),
            sig_loc: func.sig_loc.dupe(),
            effect_: func.effect_,
            comments: self.map_syntax_opt(func.comments.as_ref()),
        }
    }

    fn map_function_params(
        &mut self,
        params: &'ast ast::function::Params<Loc, Loc>,
    ) -> ast::function::Params<Loc, Loc> {
        ast::function::Params {
            loc: params.loc.dupe(),
            this_: params.this_.clone(),
            params: params.params.dupe(),
            rest: params.rest.clone(),
            comments: self.map_syntax_opt(params.comments.as_ref()),
        }
    }

    fn map_function_type(
        &mut self,
        _loc: &'ast Loc,
        func: &'ast ast::types::Function<Loc, Loc>,
    ) -> ast::types::Function<Loc, Loc> {
        let return_ = self.map_function_type_return_annotation(&func.return_);
        ast::types::Function {
            tparams: func.tparams.clone(),
            params: func.params.clone(),
            return_,
            effect: func.effect,
            comments: self.map_syntax_opt(func.comments.as_ref()),
        }
    }

    fn map_generic_identifier_type(
        &mut self,
        git: &'ast ast::types::generic::Identifier<Loc, Loc>,
    ) -> ast::types::generic::Identifier<Loc, Loc> {
        match git {
            ast::types::generic::Identifier::Unqualified(identifier) => {
                ast::types::generic::Identifier::Unqualified(self.map_identifier(identifier))
            }
            ast::types::generic::Identifier::Qualified(qualified) => {
                let mut new_qualified = (**qualified).clone();
                new_qualified.id = self.map_identifier(&qualified.id);
                ast::types::generic::Identifier::Qualified(Arc::new(new_qualified))
            }
            ast::types::generic::Identifier::ImportTypeAnnot(import_type) => {
                ast::types::generic::Identifier::ImportTypeAnnot(import_type.dupe())
            }
        }
    }

    fn map_import(
        &mut self,
        expr: &'ast ast::expression::Import<Loc, Loc>,
    ) -> ast::expression::Import<Loc, Loc> {
        ast::expression::Import {
            phase: expr.phase,
            argument: expr.argument.clone(),
            options: expr.options.clone(),
            comments: self.map_syntax_opt(expr.comments.as_ref()),
        }
    }

    fn map_interface_type(
        &mut self,
        _loc: &'ast Loc,
        t: &'ast ast::types::Interface<Loc, Loc>,
    ) -> ast::types::Interface<Loc, Loc> {
        let body = (t.body.0.dupe(), self.map_object_type(&t.body.0, &t.body.1));
        ast::types::Interface {
            extends: t.extends.dupe(),
            body,
            comments: self.map_syntax_opt(t.comments.as_ref()),
        }
    }

    fn map_intersection_type(
        &mut self,
        _loc: &'ast Loc,
        t: &'ast ast::types::Intersection<Loc, Loc>,
    ) -> ast::types::Intersection<Loc, Loc> {
        let (t0, t1, ts) = &t.types;
        let (t1_, ts_) = if ts.is_empty() {
            (self.map_type_(t1), Vec::new())
        } else {
            let mut new_ts: Vec<_> = ts.to_vec();
            if let Some(last) = new_ts.last_mut() {
                *last = self.map_type_(last);
            }
            (t1.clone(), new_ts)
        };
        ast::types::Intersection {
            types: (t0.clone(), t1_, ts_),
            comments: self.map_syntax_opt(t.comments.as_ref()),
        }
    }

    fn map_jsx_element(
        &mut self,
        _loc: &'ast Loc,
        elem: &'ast ast::jsx::Element<Loc, Loc>,
    ) -> ast::jsx::Element<Loc, Loc> {
        ast::jsx::Element {
            opening_element: elem.opening_element.clone(),
            closing_element: elem.closing_element.clone(),
            children: elem.children.clone(),
            comments: self.map_syntax_opt(elem.comments.as_ref()),
        }
    }

    fn map_jsx_fragment(
        &mut self,
        _loc: &'ast Loc,
        frag: &'ast ast::jsx::Fragment<Loc, Loc>,
    ) -> ast::jsx::Fragment<Loc, Loc> {
        ast::jsx::Fragment {
            frag_opening_element: frag.frag_opening_element.dupe(),
            frag_closing_element: frag.frag_closing_element.dupe(),
            frag_children: frag.frag_children.clone(),
            frag_comments: self.map_syntax_opt(frag.frag_comments.as_ref()),
        }
    }

    fn map_logical(
        &mut self,
        _loc: &'ast Loc,
        expr: &'ast ast::expression::Logical<Loc, Loc>,
    ) -> ast::expression::Logical<Loc, Loc> {
        let right = self.map_expression(&expr.right);
        ast::expression::Logical {
            operator: expr.operator,
            left: expr.left.clone(),
            right,
            comments: self.map_syntax_opt(expr.comments.as_ref()),
        }
    }

    fn map_new(
        &mut self,
        _loc: &'ast Loc,
        expr: &'ast ast::expression::New<Loc, Loc>,
    ) -> ast::expression::New<Loc, Loc> {
        match (&expr.targs, &expr.arguments) {
            // new Callee<T>()
            (_, Some(_)) => {
                let arguments = expr.arguments.as_ref().map(|a| self.map_arg_list(a));
                ast::expression::New {
                    callee: expr.callee.clone(),
                    targs: expr.targs.clone(),
                    arguments,
                    comments: self.map_syntax_opt(expr.comments.as_ref()),
                }
            }
            // new Callee<T>
            (Some(_), None) => {
                let targs = expr.targs.as_ref().map(|t| self.map_call_type_args(t));
                ast::expression::New {
                    callee: expr.callee.clone(),
                    targs,
                    arguments: None,
                    comments: self.map_syntax_opt(expr.comments.as_ref()),
                }
            }
            // new Callee
            (None, None) => {
                let callee = self.map_expression(&expr.callee);
                ast::expression::New {
                    callee,
                    targs: None,
                    arguments: None,
                    comments: self.map_syntax_opt(expr.comments.as_ref()),
                }
            }
        }
    }

    fn map_member(
        &mut self,
        _loc: &'ast Loc,
        expr: &'ast ast::expression::Member<Loc, Loc>,
    ) -> ast::expression::Member<Loc, Loc> {
        let property = self.map_member_property(&expr.property);
        ast::expression::Member {
            object: expr.object.clone(),
            property,
            comments: self.map_syntax_opt(expr.comments.as_ref()),
        }
    }

    fn map_object(
        &mut self,
        _loc: &'ast Loc,
        expr: &'ast ast::expression::Object<Loc, Loc>,
    ) -> ast::expression::Object<Loc, Loc> {
        ast::expression::Object {
            properties: expr.properties.dupe(),
            comments: self.map_syntax_opt(expr.comments.as_ref()),
        }
    }

    fn map_object_type(
        &mut self,
        _loc: &'ast Loc,
        obj: &'ast ast::types::Object<Loc, Loc>,
    ) -> ast::types::Object<Loc, Loc> {
        ast::types::Object {
            exact: obj.exact,
            inexact: obj.inexact,
            properties: obj.properties.dupe(),
            comments: self.map_syntax_opt(obj.comments.as_ref()),
        }
    }

    fn map_predicate(
        &mut self,
        pred: &'ast ast::types::Predicate<Loc, Loc>,
    ) -> ast::types::Predicate<Loc, Loc> {
        ast::types::Predicate {
            loc: pred.loc.dupe(),
            kind: pred.kind.clone(),
            comments: self.map_syntax_opt(pred.comments.as_ref()),
        }
    }

    fn map_sequence(
        &mut self,
        _loc: &'ast Loc,
        expr: &'ast ast::expression::Sequence<Loc, Loc>,
    ) -> ast::expression::Sequence<Loc, Loc> {
        let expressions = if let Some(last) = expr.expressions.last() {
            let mut new_exprs: Vec<_> = expr.expressions.iter().cloned().collect();
            if let Some(last_ref) = new_exprs.last_mut() {
                *last_ref = self.map_expression(last);
            }
            Arc::from(new_exprs)
        } else {
            expr.expressions.dupe()
        };
        ast::expression::Sequence {
            expressions,
            comments: self.map_syntax_opt(expr.comments.as_ref()),
        }
    }

    fn map_template_literal(
        &mut self,
        _loc: &'ast Loc,
        expr: &'ast ast::expression::TemplateLiteral<Loc, Loc>,
    ) -> ast::expression::TemplateLiteral<Loc, Loc> {
        ast::expression::TemplateLiteral {
            quasis: expr.quasis.dupe(),
            expressions: expr.expressions.dupe(),
            comments: self.map_syntax_opt(expr.comments.as_ref()),
        }
    }

    fn map_tuple_type(
        &mut self,
        _loc: &'ast Loc,
        t: &'ast ast::types::Tuple<Loc, Loc>,
    ) -> ast::types::Tuple<Loc, Loc> {
        ast::types::Tuple {
            elements: t.elements.dupe(),
            inexact: t.inexact,
            comments: self.map_syntax_opt(t.comments.as_ref()),
        }
    }

    fn map_type_cast(
        &mut self,
        _loc: &'ast Loc,
        expr: &'ast ast::expression::TypeCast<Loc, Loc>,
    ) -> ast::expression::TypeCast<Loc, Loc> {
        ast::expression::TypeCast {
            expression: expr.expression.clone(),
            annot: expr.annot.clone(),
            comments: self.map_syntax_opt(expr.comments.as_ref()),
        }
    }

    fn map_type_params(
        &mut self,
        _kind: &TypeParamsContext,
        tparams: &'ast ast::types::TypeParams<Loc, Loc>,
    ) -> ast::types::TypeParams<Loc, Loc> {
        ast::types::TypeParams {
            loc: tparams.loc.dupe(),
            params: tparams.params.dupe(),
            comments: self.map_syntax_opt(tparams.comments.as_ref()),
        }
    }

    fn map_union_type(
        &mut self,
        _loc: &'ast Loc,
        t: &'ast ast::types::Union<Loc, Loc>,
    ) -> ast::types::Union<Loc, Loc> {
        let (t0, t1, ts) = &t.types;
        let (t1_, ts_) = if ts.is_empty() {
            (self.map_type_(t1), Vec::new())
        } else {
            let mut new_ts: Vec<_> = ts.to_vec();
            if let Some(last) = new_ts.last_mut() {
                *last = self.map_type_(last);
            }
            (t1.clone(), new_ts)
        };
        ast::types::Union {
            types: (t0.clone(), t1_, ts_),
            comments: self.map_syntax_opt(t.comments.as_ref()),
        }
    }

    fn map_variable_declarator(
        &mut self,
        _kind: ast::VariableKind,
        decl: &'ast ast::statement::variable::Declarator<Loc, Loc>,
    ) -> ast::statement::variable::Declarator<Loc, Loc> {
        match &decl.init {
            None => ast::statement::variable::Declarator {
                loc: decl.loc.dupe(),
                id: self.map_pattern(None, &decl.id),
                init: None,
                definite: decl.definite,
            },
            Some(init) => ast::statement::variable::Declarator {
                loc: decl.loc.dupe(),
                id: decl.id.clone(),
                init: Some(self.map_expression(init)),
                definite: decl.definite,
            },
        }
    }
}

pub(super) struct TrailingAndRemoverResult {
    pub(super) trailing: Vec<ast::Comment<Loc>>,
    pub(super) remover: Option<TrailingCommentsRemover>,
}

/// Returns a remover function which removes comments beginning after the previous token.
/// No trailing comments are returned, since all comments since the last loc should be removed.
fn trailing_and_remover_after_last_loc(env: &mut ParserEnv) -> TrailingAndRemoverResult {
    if let Some(last_loc) = env.last_loc() {
        let end = last_loc.end;
        if peek::has_eaten_comments(env) {
            env.consume_comments_until(end);
            return TrailingAndRemoverResult {
                trailing: Vec::new(),
                remover: Some(TrailingCommentsRemover { after_pos: end }),
            };
        }
    }
    TrailingAndRemoverResult {
        trailing: Vec::new(),
        remover: None,
    }
}

/// Consumes and returns comments on the same line as the previous token. Also returns a remover
/// function which can be used to remove comments beginning after the previous token's line.
pub(super) fn trailing_and_remover_after_last_line(
    env: &mut ParserEnv,
) -> TrailingAndRemoverResult {
    if let Some(last_loc) = env.last_loc() {
        let end = last_loc.end;
        if peek::has_eaten_comments(env) {
            env.consume_comments_until(end);
            let trailing = eat::comments_until_next_line(env);
            let next_line_start = Position {
                line: end.line + 1,
                column: 0,
            };
            TrailingAndRemoverResult {
                trailing,
                remover: Some(TrailingCommentsRemover {
                    after_pos: next_line_start,
                }),
            }
        } else {
            let trailing = eat::comments_until_next_line(env);
            TrailingAndRemoverResult {
                trailing,
                remover: None,
            }
        }
    } else {
        TrailingAndRemoverResult {
            trailing: Vec::new(),
            remover: None,
        }
    }
}

pub(super) fn trailing_and_remover(env: &mut ParserEnv) -> TrailingAndRemoverResult {
    if peek::is_line_terminator(env) {
        trailing_and_remover_after_last_line(env)
    } else {
        trailing_and_remover_after_last_loc(env)
    }
}

pub(super) fn id_remove_trailing(env: &mut ParserEnv, node: &mut ast::Identifier<Loc, Loc>) {
    if let Some(mut remover) = trailing_and_remover(env).remover {
        *node = remover.map_identifier(node);
    }
}

pub(super) fn expression_remove_trailing(
    env: &mut ParserEnv,
    node: &mut ast::expression::Expression<Loc, Loc>,
) {
    if let Some(mut remover) = trailing_and_remover(env).remover {
        *node = remover.map_expression(node);
    }
}

pub(super) fn block_remove_trailing(
    env: &mut ParserEnv,
    node: &mut ast::statement::Block<Loc, Loc>,
) {
    if let Some(mut remover) = trailing_and_remover(env).remover {
        *node = remover.map_block(&LOC_NONE, node);
    }
}

pub(super) fn type_params_remove_trailing(
    env: &mut ParserEnv,
    tparams: Option<&mut ast::types::TypeParams<Loc, Loc>>,
) {
    if let Some(mut remover) = trailing_and_remover(env).remover {
        if let Some(tparams) = tparams {
            *tparams = remover.map_type_params(&TypeParamsContext::Class, tparams);
        }
    }
}

pub(super) fn type_remove_trailing(env: &mut ParserEnv, node: &mut ast::types::Type<Loc, Loc>) {
    if let Some(mut remover) = trailing_and_remover(env).remover {
        *node = remover.map_type_(node);
    }
}

pub(super) fn component_renders_annotation_remove_trailing(
    env: &mut ParserEnv,
    node: &mut ast::types::ComponentRendersAnnotation<Loc, Loc>,
) {
    if let Some(mut remover) = trailing_and_remover(env).remover {
        *node = remover.map_component_renders_annotation(node);
    }
}

pub(super) fn return_annotation_remove_trailing(
    env: &mut ParserEnv,
    node: &mut ast::function::ReturnAnnot<Loc, Loc>,
) {
    if let Some(mut remover) = trailing_and_remover(env).remover {
        *node = remover.map_function_return_annotation(node);
    }
}

pub(super) fn function_params_remove_trailing(
    env: &mut ParserEnv,
    node: &mut ast::function::Params<Loc, Loc>,
) {
    if let Some(mut remover) = trailing_and_remover(env).remover {
        *node = remover.map_function_params(node);
    }
}

pub(super) fn component_params_remove_trailing(
    env: &mut ParserEnv,
    node: &mut ast::statement::component_params::Params<Loc, Loc>,
) {
    if let Some(mut remover) = trailing_and_remover(env).remover {
        *node = remover.map_component_params(node);
    }
}

pub(super) fn component_type_params_remove_trailing(
    env: &mut ParserEnv,
    node: &mut ast::types::component_params::Params<Loc, Loc>,
) {
    if let Some(mut remover) = trailing_and_remover(env).remover {
        *node = remover.map_component_type_params(node);
    }
}

pub(super) fn predicate_remove_trailing(
    env: &mut ParserEnv,
    node: Option<&mut ast::types::Predicate<Loc, Loc>>,
) {
    if let Some(mut remover) = trailing_and_remover(env).remover {
        if let Some(node) = node {
            *node = remover.map_predicate(node);
        }
    }
}

pub(super) fn object_key_remove_trailing(
    env: &mut ParserEnv,
    node: &mut ast::expression::object::Key<Loc, Loc>,
) {
    if let Some(mut remover) = trailing_and_remover(env).remover {
        *node = remover.map_object_key(node);
    }
}

#[allow(dead_code)]
pub(super) fn generic_type_remove_trailing(
    env: &mut ParserEnv,
    node: &mut ast::types::Generic<Loc, Loc>,
) {
    if let Some(mut remover) = trailing_and_remover(env).remover {
        *node = remover.map_generic_type(&LOC_NONE, node);
    }
}

pub(super) fn generic_type_list_remove_trailing(
    env: &mut ParserEnv,
    nodes: &mut [(Loc, ast::types::Generic<Loc, Loc>)],
) {
    if let Some(mut remover) = trailing_and_remover(env).remover {
        if let Some((loc, node)) = nodes.last_mut() {
            *node = remover.map_generic_type(loc, node);
        }
    }
}

pub(super) fn class_implements_remove_trailing(
    env: &mut ParserEnv,
    node: &mut ast::class::Implements<Loc, Loc>,
) {
    if let Some(mut remover) = trailing_and_remover(env).remover {
        *node = remover.map_class_implements(node);
    }
}

pub(super) fn string_literal_remove_trailing(
    env: &mut ParserEnv,
    node: &mut ast::StringLiteral<Loc>,
) {
    if let Some(mut remover) = trailing_and_remover(env).remover {
        *node = remover.map_string_literal(node);
    }
}

pub fn statement_add_comments(
    stmt: ast::statement::Statement<Loc, Loc>,
    comments: Option<ast::Syntax<Loc, ()>>,
) -> ast::statement::Statement<Loc, Loc> {
    fn merge_comments(
        inner: Option<ast::Syntax<Loc, ()>>,
        outer: Option<ast::Syntax<Loc, ()>>,
    ) -> Option<ast::Syntax<Loc, ()>> {
        ast_utils::merge_comments(inner, outer)
    }

    fn merge_comments_with_internal(
        inner: Option<ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>,
        outer: Option<ast::Syntax<Loc, ()>>,
    ) -> Option<ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>> {
        ast_utils::merge_comments_with_internal(inner, outer)
    }
    use ast::statement::Statement;
    use ast::statement::StatementInner;

    let new_inner = match &*stmt {
        StatementInner::Block { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments_with_internal(content.comments, comments);
            StatementInner::Block {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::Break { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::Break {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::ClassDeclaration { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::ClassDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::ComponentDeclaration { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::ComponentDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::Continue { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::Continue {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::Debugger { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::Debugger {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::DeclareClass { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::DeclareClass {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::DeclareComponent { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::DeclareComponent {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::DeclareEnum { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::DeclareEnum {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::DeclareExportDeclaration { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::DeclareExportDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::DeclareFunction { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::DeclareFunction {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::DeclareInterface { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::DeclareInterface {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::DeclareModule { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::DeclareModule {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::DeclareModuleExports { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::DeclareModuleExports {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::DeclareNamespace { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::DeclareNamespace {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::DeclareTypeAlias { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::DeclareTypeAlias {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::DeclareOpaqueType { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::DeclareOpaqueType {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::DeclareVariable { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::DeclareVariable {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::DoWhile { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::DoWhile {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::Empty { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::Empty {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::EnumDeclaration { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::EnumDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::ExportDefaultDeclaration { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::ExportDefaultDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::ExportNamedDeclaration { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::ExportNamedDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::ExportAssignment { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::ExportAssignment {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::NamespaceExportDeclaration { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::NamespaceExportDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::Expression { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::Expression {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::For { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::For {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::ForIn { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::ForIn {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::ForOf { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::ForOf {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::FunctionDeclaration { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::FunctionDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::If { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::If {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::ImportDeclaration { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::ImportDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::ImportEqualsDeclaration { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::ImportEqualsDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::InterfaceDeclaration { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::InterfaceDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::Labeled { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::Labeled {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::Match { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::Match {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::RecordDeclaration { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::RecordDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::Return { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::Return {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::Switch { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::Switch {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::Throw { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::Throw {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::Try { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::Try {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::TypeAlias { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::TypeAlias {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::OpaqueType { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::OpaqueType {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::VariableDeclaration { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::VariableDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::While { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::While {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
        StatementInner::With { loc, inner } => {
            let mut content = (**inner).clone();
            content.comments = merge_comments(content.comments, comments);
            StatementInner::With {
                loc: loc.dupe(),
                inner: Arc::new(content),
            }
        }
    };
    Statement(Arc::new(new_inner))
}

// ============================================================================
// Comment bounds collector - uses AstVisitor
// ============================================================================

pub struct CommentsBoundsCollector<'a> {
    loc: &'a Loc,
    first_leading: Option<(Loc, bool)>,
    last_trailing: Option<(Loc, ast::CommentKind)>,
}

impl<'a> CommentsBoundsCollector<'a> {
    pub fn new(loc: &'a Loc) -> Self {
        Self {
            loc,
            first_leading: None,
            last_trailing: None,
        }
    }

    fn visit_leading_comment(&mut self, comment: &ast::Comment<Loc>) {
        if let Some((first_leading_loc, _)) = &self.first_leading {
            if comment.loc.start < first_leading_loc.start {
                self.first_leading = Some((comment.loc.dupe(), comment.on_newline));
            }
        } else if comment.loc.start < self.loc.start {
            self.first_leading = Some((comment.loc.dupe(), comment.on_newline));
        }
    }

    fn visit_trailing_comment(&mut self, comment: &ast::Comment<Loc>) {
        if let Some((last_trailing_loc, _)) = &self.last_trailing {
            if last_trailing_loc.start < comment.loc.start {
                self.last_trailing = Some((comment.loc.dupe(), comment.kind));
            }
        } else if comment.loc.start >= self.loc.end {
            self.last_trailing = Some((comment.loc.dupe(), comment.kind));
        }
    }

    fn collect_comments<I: Dupe>(&mut self, syntax: &ast::Syntax<Loc, I>) -> Result<(), !> {
        let ast::Syntax {
            leading,
            trailing,
            internal: _,
        } = syntax;
        for c in leading.iter() {
            self.visit_leading_comment(c);
        }
        for c in trailing.iter() {
            self.visit_trailing_comment(c);
        }
        Ok(())
    }

    fn collect_comments_opt<I: Dupe>(
        &mut self,
        syntax_opt: Option<&ast::Syntax<Loc, I>>,
    ) -> Result<(), !> {
        if let Some(syntax) = syntax_opt {
            self.collect_comments(syntax)
        } else {
            Ok(())
        }
    }

    pub fn comment_bounds(self) -> CommentsBounds {
        let Self {
            loc: _,
            first_leading,
            last_trailing,
        } = self;
        CommentsBounds {
            first_leading,
            last_trailing,
        }
    }
}

impl<'a, 'ast> AstVisitor<'ast, Loc> for CommentsBoundsCollector<'a> {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn syntax<Internal: Dupe>(
        &mut self,
        syntax: &'ast ast::Syntax<Loc, Internal>,
    ) -> Result<(), !> {
        self.collect_comments(syntax)
    }

    fn block(
        &mut self,
        _loc: &'ast Loc,
        stmt: &'ast ast::statement::Block<Loc, Loc>,
    ) -> Result<(), !> {
        self.collect_comments_opt(stmt.comments.as_ref())
    }
}

pub struct CommentsBounds {
    pub first_leading: Option<(Loc, bool)>,
    pub last_trailing: Option<(Loc, ast::CommentKind)>,
}

// Given an AST node and a function to collect all its comments, return the first leading
// and last trailing comment on the node.
pub fn comment_bounds<'a, F>(loc: &'a Loc, f: F) -> CommentsBounds
where
    F: FnOnce(&mut CommentsBoundsCollector<'a>) -> Result<(), !>,
{
    let mut collector = CommentsBoundsCollector::new(loc);
    let Ok(()) = f(&mut collector);
    collector.comment_bounds()
}

pub fn expand_loc_with_comment_bounds(
    loc: &Loc,
    CommentsBounds {
        first_leading,
        last_trailing,
    }: &CommentsBounds,
) -> Loc {
    let start = match first_leading {
        Some((first_leading_loc, _)) => first_leading_loc,
        None => loc,
    };
    let end = match last_trailing {
        Some((last_trailing_loc, _)) => last_trailing_loc,
        None => loc,
    };
    Loc::between(start, end)
}

/// Remove the trailing comment bound if it is a line comment
pub fn comment_bounds_without_trailing_line_comment(mut bounds: CommentsBounds) -> CommentsBounds {
    if matches!(&bounds.last_trailing, Some((_, ast::CommentKind::Line))) {
        bounds.last_trailing = None;
        bounds
    } else {
        bounds
    }
}

/// Return the first leading and last trailing comment of a statement
pub fn statement_comment_bounds(node: &ast::statement::Statement<Loc, Loc>) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(node.loc());
    let Ok(()) = collector.statement(node);
    collector.comment_bounds()
}

pub fn expression_comment_bounds(node: &ast::expression::Expression<Loc, Loc>) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(node.loc());
    let Ok(()) = collector.expression(node);
    collector.comment_bounds()
}

pub fn type_comment_bounds(node: &ast::types::Type<Loc, Loc>) -> CommentsBounds {
    let mut collector: CommentsBoundsCollector<'_> = CommentsBoundsCollector::new(node.loc());
    let Ok(()) = collector.type_(node);
    collector.comment_bounds()
}

pub fn block_comment_bounds(
    (loc, node): &(Loc, ast::statement::Block<Loc, Loc>),
) -> CommentsBounds {
    let mut collector: CommentsBoundsCollector<'_> = CommentsBoundsCollector::new(loc);
    let Ok(()) = collector.block(loc, node);
    collector.comment_bounds()
}

pub fn object_property_comment_bounds(
    property: &ast::expression::object::Property<Loc, Loc>,
) -> CommentsBounds {
    let bounds = match property {
        ast::expression::object::Property::NormalProperty(p) => {
            let mut collector = CommentsBoundsCollector::new(p.loc());
            let Ok(()) = collector.object_property(p);
            collector.comment_bounds()
        }
        ast::expression::object::Property::SpreadProperty(p) => {
            let mut collector = CommentsBoundsCollector::new(&p.loc);
            let Ok(()) = collector.spread_property(p);
            collector.comment_bounds()
        }
    };
    comment_bounds_without_trailing_line_comment(bounds)
}

// Port of object_type_property_comment_bounds from comment_attachment.ml (lines 712-741)
pub fn object_type_property_comment_bounds(
    property: &ast::types::object::Property<Loc, Loc>,
) -> CommentsBounds {
    let collector = match property {
        ast::types::object::Property::NormalProperty(p) => {
            let mut collector = CommentsBoundsCollector::new(&p.loc);
            let Ok(()) = collector.object_property_type(p);
            collector
        }
        ast::types::object::Property::SpreadProperty(p) => {
            let mut collector = CommentsBoundsCollector::new(&p.loc);
            let Ok(()) = collector.object_spread_property_type(p);
            collector
        }
        ast::types::object::Property::Indexer(p) => {
            let mut collector = CommentsBoundsCollector::new(&p.loc);
            let Ok(()) = collector.object_indexer_property_type(p);
            collector
        }
        ast::types::object::Property::InternalSlot(p) => {
            let mut collector = CommentsBoundsCollector::new(&p.loc);
            let Ok(()) = collector.object_internal_slot_property_type(p);
            collector
        }
        ast::types::object::Property::CallProperty(p) => {
            let mut collector = CommentsBoundsCollector::new(&p.loc);
            let Ok(()) = collector.object_call_property_type(p);
            collector
        }
        ast::types::object::Property::MappedType(p) => {
            let mut collector = CommentsBoundsCollector::new(&p.loc);
            let Ok(()) = collector.object_mapped_type_property(p);
            collector
        }
        ast::types::object::Property::PrivateField(p) => {
            let mut collector = CommentsBoundsCollector::new(&p.loc);
            let Ok(()) = collector.object_private_field_type(p);
            collector
        }
    };
    comment_bounds_without_trailing_line_comment(collector.comment_bounds())
}

pub fn object_pattern_property_comment_bounds(
    loc: &Loc,
    property: &ast::pattern::object::Property<Loc, Loc>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(loc);
    let Ok(()) = collector.pattern_object_p(None, loc, property);
    comment_bounds_without_trailing_line_comment(collector.comment_bounds())
}

pub fn match_expression_case_comment_bounds(
    case: &ast::match_::Case<Loc, Loc, ast::expression::Expression<Loc, Loc>>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(&case.loc);
    let Ok(()) = collector.match_case(
        case,
        &mut |v: &mut CommentsBoundsCollector, body: &ast::expression::Expression<Loc, Loc>| {
            v.expression(body)
        },
    );
    collector.comment_bounds()
}

pub fn match_statement_case_comment_bounds(
    case: &ast::match_::Case<Loc, Loc, ast::statement::Statement<Loc, Loc>>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(&case.loc);
    let Ok(()) = collector.match_case(
        case,
        &mut |v: &mut CommentsBoundsCollector, body: &ast::statement::Statement<Loc, Loc>| {
            v.statement(body)
        },
    );
    collector.comment_bounds()
}

pub fn switch_case_comment_bounds(case: &ast::statement::switch::Case<Loc, Loc>) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(&case.loc);
    let Ok(()) = collector.switch_case(case);
    collector.comment_bounds()
}

pub fn function_param_comment_bounds(param: &ast::function::Param<Loc, Loc>) -> CommentsBounds {
    let loc = match param {
        ast::function::Param::RegularParam { loc, .. } => loc,
        ast::function::Param::ParamProperty { loc, .. } => loc,
    };
    let mut collector = CommentsBoundsCollector::new(loc);
    let Ok(()) = collector.function_param(param);
    comment_bounds_without_trailing_line_comment(collector.comment_bounds())
}

pub fn function_rest_param_comment_bounds(
    param: &ast::function::RestParam<Loc, Loc>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(&param.loc);
    let Ok(()) = collector.function_rest_param(param);
    comment_bounds_without_trailing_line_comment(collector.comment_bounds())
}

pub fn function_this_param_comment_bounds(
    param: &ast::function::ThisParam<Loc, Loc>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(&param.loc);
    let Ok(()) = collector.function_this_param(param);
    comment_bounds_without_trailing_line_comment(collector.comment_bounds())
}

pub fn function_type_param_comment_bounds(
    param: &ast::types::function::Param<Loc, Loc>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(&param.loc);
    let Ok(()) = collector.function_param_type(param);
    comment_bounds_without_trailing_line_comment(collector.comment_bounds())
}

pub fn function_type_rest_param_comment_bounds(
    param: &ast::types::function::RestParam<Loc, Loc>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(&param.loc);
    let Ok(()) = collector.function_rest_param_type(param);
    comment_bounds_without_trailing_line_comment(collector.comment_bounds())
}

pub fn function_type_this_param_comment_bounds(
    param: &ast::types::function::ThisParam<Loc, Loc>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(&param.loc);
    let Ok(()) = collector.function_this_param_type(param);
    comment_bounds_without_trailing_line_comment(collector.comment_bounds())
}

pub fn component_param_comment_bounds(
    param: &ast::statement::component_params::Param<Loc, Loc>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(&param.loc);
    let Ok(()) = collector.component_param(param);
    comment_bounds_without_trailing_line_comment(collector.comment_bounds())
}

pub fn component_rest_param_comment_bounds(
    param: &ast::statement::component_params::RestParam<Loc, Loc>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(&param.loc);
    let Ok(()) = collector.component_rest_param(param);
    comment_bounds_without_trailing_line_comment(collector.comment_bounds())
}

pub fn component_type_param_comment_bounds(
    param: &ast::types::component_params::Param<Loc, Loc>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(&param.loc);
    let Ok(()) = collector.component_type_param(param);
    comment_bounds_without_trailing_line_comment(collector.comment_bounds())
}

pub fn component_type_rest_param_comment_bounds(
    param: &ast::types::component_params::RestParam<Loc, Loc>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(&param.loc);
    let Ok(()) = collector.component_type_rest_param(param);
    comment_bounds_without_trailing_line_comment(collector.comment_bounds())
}

pub fn array_element_comment_bounds(
    loc: &Loc,
    element: &ast::expression::ArrayElement<Loc, Loc>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(loc);
    let Ok(()) = collector.array_element(element);
    comment_bounds_without_trailing_line_comment(collector.comment_bounds())
}

pub fn array_pattern_element_comment_bounds(
    loc: &Loc,
    element: &ast::pattern::array::Element<Loc, Loc>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(loc);
    let Ok(()) = collector.pattern_array_e(None, loc, element);
    comment_bounds_without_trailing_line_comment(collector.comment_bounds())
}

pub fn expression_or_spread_comment_bounds(
    loc: &Loc,
    expr_or_spread: &ast::expression::ExpressionOrSpread<Loc, Loc>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(loc);
    let Ok(()) = collector.expression_or_spread(expr_or_spread);
    comment_bounds_without_trailing_line_comment(collector.comment_bounds())
}

pub fn call_type_arg_comment_bounds(
    loc: &Loc,
    arg: &ast::expression::CallTypeArg<Loc, Loc>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(loc);
    let Ok(()) = collector.call_type_arg(arg);
    comment_bounds_without_trailing_line_comment(collector.comment_bounds())
}

pub fn type_param_comment_bounds(
    context: TypeParamsContext,
    param: &ast::types::TypeParam<Loc, Loc>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(&param.loc);
    let Ok(()) = collector.type_param(&context, param);
    comment_bounds_without_trailing_line_comment(collector.comment_bounds())
}

pub fn function_body_comment_bounds(body: &ast::function::Body<Loc, Loc>) -> CommentsBounds {
    let loc = match body {
        ast::function::Body::BodyBlock((loc, _)) => loc,
        ast::function::Body::BodyExpression(expr) => expr.loc(),
    };
    let mut collector = CommentsBoundsCollector::new(loc);
    let Ok(()) = collector.function_body_any(body);
    collector.comment_bounds()
}

pub fn if_alternate_statement_comment_bounds(
    loc: &Loc,
    alternate: &ast::statement::if_::Alternate<Loc, Loc>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(loc);
    let Ok(()) = collector.if_alternate_statement(alternate);
    collector.comment_bounds()
}

pub fn member_property_comment_bounds(
    loc: &Loc,
    property: &ast::expression::member::Property<Loc, Loc>,
) -> CommentsBounds {
    let mut collector = CommentsBoundsCollector::new(loc);
    let Ok(()) = collector.member_property(property);
    collector.comment_bounds()
}
