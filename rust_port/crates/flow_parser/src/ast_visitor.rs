/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;
use std::sync::Arc;

use dupe::Dupe;

use crate::ast;
use crate::ast::types::TypeInner;

#[derive(Debug, Clone, Copy)]
pub enum TypeParamsContext {
    Class,
    Function,
    DeclareFunction,
    DeclareClass,
    DeclareComponent,
    TypeAlias,
    Interface,
    OpaqueType,
    ComponentDeclaration,
    ComponentType,
    FunctionType,
    Infer,
    ObjectMappedType,
    Record,
}

/// A trait for visiting AST nodes in a Flow/JavaScript program.
///
/// # Usage
///
/// Implement this trait to traverse an AST. Each method has a default implementation
/// that recursively visits child nodes. Override methods to add custom behavior.
///
/// # Calling Base Implementation
///
/// When overriding a method, you can call the corresponding `*_default` free function
/// to invoke the base traversal behavior (similar to calling `super` in other languages):
///
/// ```ignore
/// use flow_parser::ast_visitor::{self, AstVisitor};
///
/// struct MyVisitor;
///
/// impl AstVisitor<Loc> for MyVisitor {
///     fn statement(&mut self, stmt: &'ast ast::statement::Statement<Loc, Loc>) {
///         // Custom logic before visiting children
///         println!("Visiting statement");
///
///         // Call base implementation to visit child nodes
///         ast_visitor::statement_default(self, stmt);
///
///         // Custom logic after visiting children
///     }
/// }
/// ```
///
/// # Conditional Visiting
///
/// You can conditionally skip the base implementation for certain cases:
///
/// ```ignore
/// fn import_declaration(
///     &mut self,
///     loc: &'ast Loc,
///     decl: &'ast ast::statement::ImportDeclaration<Loc, Loc>,
/// ) {
///     match decl.import_kind {
///         ast::statement::ImportKind::ImportType => {
///             // Skip type imports entirely
///         }
///         _ => {
///             // Visit value imports normally
///             ast_visitor::import_declaration_default(self, loc, decl);
///         }
///     }
/// }
/// ```
///
/// # Mutable Visitors
///
/// Most methods have `*_mut` variants for mutating AST nodes. Use these when you need
/// to transform the AST in place.
///
/// # Limitation: No Multi-Level Inheritance
///
/// This pattern only supports single-level inheritance. The `*_default` functions always
/// call the trait's default traversal logic—there is no way to inherit from an intermediate
/// visitor that overrides methods.
///
/// For example, if `VisitorB` overrides `expression()` with custom logic, and you want
/// `VisitorC` to inherit from `VisitorB`, there's no `expression_b()` function to call.
/// `VisitorC` must implement `AstVisitor` directly and manually replicate all of `VisitorB`'s
/// overrides, even for methods it doesn't intend to change.
///
/// **Workaround: Composition over Inheritance**
///
/// When you need shared visitor logic across multiple visitors, prefer composition:
///
/// ```ignore
/// // Extract shared logic into helper functions
/// fn handle_expression_common<Loc>(
///     visitor: &mut impl AstVisitor<Loc>,
///     expr: &'ast ast::expression::Expression<Loc, Loc>,
/// ) {
///     // Shared logic here
///     ast_visitor::expression_default(visitor, expr);
/// }
///
/// struct VisitorB;
/// impl AstVisitor<Loc> for VisitorB {
///     fn expression(&mut self, expr: &'ast ast::expression::Expression<Loc, Loc>) {
///         handle_expression_common(self, expr);
///     }
/// }
///
/// struct VisitorC;
/// impl AstVisitor<Loc> for VisitorC {
///     fn expression(&mut self, expr: &'ast ast::expression::Expression<Loc, Loc>) {
///         // Can reuse VisitorB's logic via the helper
///         handle_expression_common(self, expr);
///         // Plus additional VisitorC-specific logic
///     }
/// }
/// ```
///
/// This approach makes shared behavior explicit and avoids the complexity of simulating
/// multi-level inheritance.
pub trait AstVisitor<'ast, Loc: Dupe, Type: Dupe = Loc, C = &'ast Loc, E = !> {
    fn normalize_loc(loc: &'ast Loc) -> C;

    fn normalize_type(type_: &'ast Type) -> C;

    /// Maps a syntactic location carried by an optional-chain assertion.
    /// Overrides must also map enclosing expression locations consistently.
    fn map_loc_annot(&mut self, loc: &'ast Loc) -> Loc {
        loc.dupe()
    }

    fn program(&mut self, program: &'ast ast::Program<Loc, Type>) -> Result<(), E> {
        program_default(self, program)
    }

    fn map_program(&mut self, program: &'ast ast::Program<Loc, Loc>) -> ast::Program<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_program_default(self, program)
    }

    fn statement(&mut self, stmt: &'ast ast::statement::Statement<Loc, Type>) -> Result<(), E> {
        statement_default(self, stmt)
    }

    fn map_statement(
        &mut self,
        stmt: &'ast ast::statement::Statement<Loc, Loc>,
    ) -> ast::statement::Statement<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_statement_default(self, stmt)
    }

    fn comment(&mut self, comment: &'ast ast::Comment<Loc>) -> Result<(), E> {
        let _ = comment;
        Ok(())
    }

    fn map_comment(&mut self, comment: &'ast ast::Comment<Loc>) -> ast::Comment<Loc>
    where
        Loc: Dupe,
    {
        comment.dupe()
    }

    fn syntax_opt<Internal: Dupe>(
        &mut self,
        syntax_opt: Option<&'ast ast::Syntax<Loc, Internal>>,
    ) -> Result<(), E> {
        if let Some(syntax) = syntax_opt {
            self.syntax(syntax)?;
        }
        Ok(())
    }

    fn map_syntax_opt<Internal: Dupe>(
        &mut self,
        syntax_opt: Option<&'ast ast::Syntax<Loc, Internal>>,
    ) -> Option<ast::Syntax<Loc, Internal>>
    where
        Loc: Dupe,
    {
        syntax_opt.map(|syntax| self.map_syntax(syntax))
    }

    fn syntax<Internal: Dupe>(
        &mut self,
        syntax: &'ast ast::Syntax<Loc, Internal>,
    ) -> Result<(), E> {
        let ast::Syntax {
            leading,
            trailing,
            internal,
        } = syntax;
        for c in leading.iter() {
            self.comment(c)?;
        }
        for c in trailing.iter() {
            self.comment(c)?;
        }
        let _ = internal;
        Ok(())
    }

    fn map_syntax<Internal: Dupe>(
        &mut self,
        syntax: &'ast ast::Syntax<Loc, Internal>,
    ) -> ast::Syntax<Loc, Internal>
    where
        Loc: Dupe,
    {
        let ast::Syntax {
            leading,
            trailing,
            internal,
        } = syntax;
        let leading_ = Arc::from(
            leading
                .iter()
                .map(|c| self.map_comment(c))
                .collect::<Vec<_>>(),
        );
        let trailing_ = Arc::from(
            trailing
                .iter()
                .map(|c| self.map_comment(c))
                .collect::<Vec<_>>(),
        );
        ast::Syntax {
            leading: leading_,
            trailing: trailing_,
            internal: internal.dupe(),
        }
    }

    fn expression(&mut self, expr: &'ast ast::expression::Expression<Loc, Type>) -> Result<(), E> {
        expression_default(self, expr)
    }

    fn map_expression(
        &mut self,
        expr: &'ast ast::expression::Expression<Loc, Loc>,
    ) -> ast::expression::Expression<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_expression_default(self, expr)
    }

    fn array(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::Array<Loc, Type>,
    ) -> Result<(), E> {
        array_default(self, loc, expr)
    }

    fn map_array(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::Array<Loc, Loc>,
    ) -> ast::expression::Array<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_array_default(self, loc, expr)
    }

    fn array_element(
        &mut self,
        element: &'ast ast::expression::ArrayElement<Loc, Type>,
    ) -> Result<(), E> {
        array_element_default(self, element)
    }

    fn map_array_element(
        &mut self,
        element: &'ast ast::expression::ArrayElement<Loc, Loc>,
    ) -> ast::expression::ArrayElement<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_array_element_default(self, element)
    }

    fn arrow_function(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::function::Function<Loc, Type>,
    ) -> Result<(), E> {
        arrow_function_default(self, loc, expr)
    }

    fn map_arrow_function(
        &mut self,
        loc: &'ast Loc,
        func: &'ast ast::function::Function<Loc, Loc>,
    ) -> ast::function::Function<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_arrow_function_default(self, loc, func)
    }

    fn as_const_expression(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::AsConstExpression<Loc, Type>,
    ) -> Result<(), E> {
        as_const_expression_default(self, loc, expr)
    }

    fn map_as_const_expression(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::AsConstExpression<Loc, Loc>,
    ) -> ast::expression::AsConstExpression<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_as_const_expression_default(self, loc, expr)
    }

    fn as_expression(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::AsExpression<Loc, Type>,
    ) -> Result<(), E> {
        as_expression_default(self, loc, expr)
    }

    fn map_as_expression(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::AsExpression<Loc, Loc>,
    ) -> ast::expression::AsExpression<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_as_expression_default(self, loc, expr)
    }

    fn assignment(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::Assignment<Loc, Type>,
    ) -> Result<(), E> {
        assignment_default(self, loc, expr)
    }

    fn map_assignment(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::Assignment<Loc, Loc>,
    ) -> ast::expression::Assignment<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_assignment_default(self, loc, expr)
    }

    fn binary(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::Binary<Loc, Type>,
    ) -> Result<(), E> {
        binary_default(self, loc, expr)
    }

    fn map_binary(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::Binary<Loc, Loc>,
    ) -> ast::expression::Binary<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_binary_default(self, loc, expr)
    }

    fn block(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::Block<Loc, Type>,
    ) -> Result<(), E> {
        block_default(self, loc, stmt)
    }

    fn map_block(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::Block<Loc, Loc>,
    ) -> ast::statement::Block<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_block_default(self, loc, stmt)
    }

    fn break_(
        &mut self,
        loc: &'ast Loc,
        break_stmt: &'ast ast::statement::Break<Loc>,
    ) -> Result<(), E> {
        break_default(self, loc, break_stmt)
    }

    fn map_break_(
        &mut self,
        loc: &'ast Loc,
        break_stmt: &'ast ast::statement::Break<Loc>,
    ) -> ast::statement::Break<Loc>
    where
        Loc: Dupe,
    {
        map_break_default(self, loc, break_stmt)
    }

    fn call(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::Call<Loc, Type>,
    ) -> Result<(), E> {
        call_default(self, loc, expr)
    }

    fn map_call(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::Call<Loc, Loc>,
    ) -> ast::expression::Call<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_call_default(self, loc, expr)
    }

    fn arg_list(&mut self, arg_list: &'ast ast::expression::ArgList<Loc, Type>) -> Result<(), E> {
        arg_list_default(self, arg_list)
    }

    fn map_arg_list(
        &mut self,
        args: &'ast ast::expression::ArgList<Loc, Loc>,
    ) -> ast::expression::ArgList<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_arg_list_default(self, args)
    }

    fn optional_call(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::OptionalCall<Loc, Type>,
    ) -> Result<(), E> {
        optional_call_default(self, loc, expr)
    }

    fn map_optional_call(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::OptionalCall<Loc, Loc>,
    ) -> ast::expression::OptionalCall<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_optional_call_default(self, loc, expr)
    }

    fn call_type_args(
        &mut self,
        targs: &'ast ast::expression::CallTypeArgs<Loc, Type>,
    ) -> Result<(), E> {
        call_type_args_default(self, targs)
    }

    fn map_call_type_args(
        &mut self,
        targs: &'ast ast::expression::CallTypeArgs<Loc, Loc>,
    ) -> ast::expression::CallTypeArgs<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_call_type_args_default(self, targs)
    }

    fn call_type_arg(&mut self, t: &'ast ast::expression::CallTypeArg<Loc, Type>) -> Result<(), E> {
        call_type_arg_default(self, t)
    }

    fn map_call_type_arg(
        &mut self,
        t: &'ast ast::expression::CallTypeArg<Loc, Loc>,
    ) -> ast::expression::CallTypeArg<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_call_type_arg_default(self, t)
    }

    fn class_expression(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::class::Class<Loc, Type>,
    ) -> Result<(), E> {
        class_expression_default(self, loc, expr)
    }

    fn map_class_expression(
        &mut self,
        loc: &'ast Loc,
        cls: &'ast ast::class::Class<Loc, Loc>,
    ) -> ast::class::Class<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_expression_default(self, loc, cls)
    }

    fn catch_body(&mut self, body: &'ast (Loc, ast::statement::Block<Loc, Type>)) -> Result<(), E> {
        catch_body_default(self, body)
    }

    fn map_catch_body(
        &mut self,
        body: &'ast (Loc, ast::statement::Block<Loc, Loc>),
    ) -> (Loc, ast::statement::Block<Loc, Loc>)
    where
        Loc: Dupe,
    {
        map_catch_body_default(self, body)
    }

    fn catch_clause(
        &mut self,
        clause: &'ast ast::statement::try_::CatchClause<Loc, Type>,
    ) -> Result<(), E> {
        catch_clause_default(self, clause)
    }

    fn map_catch_clause(
        &mut self,
        catch: &'ast ast::statement::try_::CatchClause<Loc, Loc>,
    ) -> ast::statement::try_::CatchClause<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_catch_clause_default(self, catch)
    }

    fn class_declaration(
        &mut self,
        loc: &'ast Loc,
        cls: &'ast ast::class::Class<Loc, Type>,
    ) -> Result<(), E> {
        class_declaration_default(self, loc, cls)
    }

    fn map_class_declaration(
        &mut self,
        loc: &'ast Loc,
        cls: &'ast ast::class::Class<Loc, Loc>,
    ) -> ast::class::Class<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_declaration_default(self, loc, cls)
    }

    fn class_(&mut self, loc: C, cls: &'ast ast::class::Class<Loc, Type>) -> Result<(), E> {
        class_default(self, loc, cls)
    }

    fn map_class_(
        &mut self,
        loc: &'ast Loc,
        cls: &'ast ast::class::Class<Loc, Loc>,
    ) -> ast::class::Class<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_default(self, loc, cls)
    }

    fn class_extends(&mut self, extends: &'ast ast::class::Extends<Loc, Type>) -> Result<(), E> {
        class_extends_default(self, extends)
    }

    fn map_class_extends(
        &mut self,
        extends: &'ast ast::class::Extends<Loc, Loc>,
    ) -> ast::class::Extends<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_extends_default(self, extends)
    }

    fn class_identifier(&mut self, ident: &'ast ast::Identifier<Loc, Type>) -> Result<(), E> {
        class_identifier_default(self, ident)
    }

    fn map_class_identifier(
        &mut self,
        ident: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_identifier_default(self, ident)
    }

    fn class_body(&mut self, cls_body: &'ast ast::class::Body<Loc, Type>) -> Result<(), E> {
        class_body_default(self, cls_body)
    }

    fn map_class_body(
        &mut self,
        cls_body: &'ast ast::class::Body<Loc, Loc>,
    ) -> ast::class::Body<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_body_default(self, cls_body)
    }

    fn class_decorator(&mut self, dec: &'ast ast::class::Decorator<Loc, Type>) -> Result<(), E> {
        class_decorator_default(self, dec)
    }

    fn map_class_decorator(
        &mut self,
        dec: &'ast ast::class::Decorator<Loc, Loc>,
    ) -> ast::class::Decorator<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_decorator_default(self, dec)
    }

    fn class_element(&mut self, elem: &'ast ast::class::BodyElement<Loc, Type>) -> Result<(), E> {
        class_element_default(self, elem)
    }

    fn map_class_element(
        &mut self,
        elem: &'ast ast::class::BodyElement<Loc, Loc>,
    ) -> ast::class::BodyElement<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_element_default(self, elem)
    }

    fn class_method(&mut self, method: &'ast ast::class::Method<Loc, Type>) -> Result<(), E> {
        class_method_default(self, method)
    }

    fn map_class_method(
        &mut self,
        method: &'ast ast::class::Method<Loc, Loc>,
    ) -> ast::class::Method<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_method_default(self, method)
    }

    fn class_implements(
        &mut self,
        implements: &'ast ast::class::Implements<Loc, Type>,
    ) -> Result<(), E> {
        class_implements_default(self, implements)
    }

    fn map_class_implements(
        &mut self,
        implements: &'ast ast::class::Implements<Loc, Loc>,
    ) -> ast::class::Implements<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_implements_default(self, implements)
    }

    fn class_implements_interface(
        &mut self,
        interface: &'ast ast::class::implements::Interface<Loc, Type>,
    ) -> Result<(), E> {
        class_implements_interface_default(self, interface)
    }

    fn map_class_implements_interface(
        &mut self,
        interface: &'ast ast::class::implements::Interface<Loc, Loc>,
    ) -> ast::class::implements::Interface<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_implements_interface_default(self, interface)
    }

    fn class_property(&mut self, prop: &'ast ast::class::Property<Loc, Type>) -> Result<(), E> {
        class_property_default(self, prop)
    }

    fn map_class_property(
        &mut self,
        prop: &'ast ast::class::Property<Loc, Loc>,
    ) -> ast::class::Property<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_property_default(self, prop)
    }

    fn class_property_value(
        &mut self,
        value: &'ast ast::class::property::Value<Loc, Type>,
    ) -> Result<(), E> {
        class_property_value_default(self, value)
    }

    fn map_class_property_value(
        &mut self,
        value: &'ast ast::class::property::Value<Loc, Loc>,
    ) -> ast::class::property::Value<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_property_value_default(self, value)
    }

    fn class_private_field(
        &mut self,
        prop: &'ast ast::class::PrivateField<Loc, Type>,
    ) -> Result<(), E> {
        class_private_field_default(self, prop)
    }

    fn map_class_private_field(
        &mut self,
        prop: &'ast ast::class::PrivateField<Loc, Loc>,
    ) -> ast::class::PrivateField<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_private_field_default(self, prop)
    }

    fn class_static_block(
        &mut self,
        block: &'ast ast::class::StaticBlock<Loc, Type>,
    ) -> Result<(), E> {
        class_static_block_default(self, block)
    }

    fn map_class_static_block(
        &mut self,
        block: &'ast ast::class::StaticBlock<Loc, Loc>,
    ) -> ast::class::StaticBlock<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_static_block_default(self, block)
    }

    fn class_declare_method(
        &mut self,
        decl_meth: &'ast ast::class::DeclareMethod<Loc, Type>,
    ) -> Result<(), E> {
        class_declare_method_default(self, decl_meth)
    }

    fn map_class_declare_method(
        &mut self,
        decl_meth: &'ast ast::class::DeclareMethod<Loc, Loc>,
    ) -> ast::class::DeclareMethod<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_declare_method_default(self, decl_meth)
    }

    fn class_abstract_method(
        &mut self,
        abs_meth: &'ast ast::class::AbstractMethod<Loc, Type>,
    ) -> Result<(), E> {
        class_abstract_method_default(self, abs_meth)
    }

    fn map_class_abstract_method(
        &mut self,
        abs_meth: &'ast ast::class::AbstractMethod<Loc, Loc>,
    ) -> ast::class::AbstractMethod<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_abstract_method_default(self, abs_meth)
    }

    fn class_abstract_property(
        &mut self,
        abs_prop: &'ast ast::class::AbstractProperty<Loc, Type>,
    ) -> Result<(), E> {
        class_abstract_property_default(self, abs_prop)
    }

    fn map_class_abstract_property(
        &mut self,
        abs_prop: &'ast ast::class::AbstractProperty<Loc, Loc>,
    ) -> ast::class::AbstractProperty<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_class_abstract_property_default(self, abs_prop)
    }

    fn class_indexer(
        &mut self,
        indexer: &'ast ast::types::object::Indexer<Loc, Type>,
    ) -> Result<(), E> {
        self.object_indexer_property_type(indexer)
    }
    fn map_class_indexer(
        &mut self,
        indexer: &'ast ast::types::object::Indexer<Loc, Loc>,
    ) -> ast::types::object::Indexer<Loc, Loc>
    where
        Loc: Dupe,
    {
        self.map_object_indexer_property_type(indexer)
    }

    fn default_opt(
        &mut self,
        default: Option<&'ast ast::expression::Expression<Loc, Type>>,
    ) -> Result<(), E> {
        default_opt_default(self, default)
    }

    fn map_default_opt(
        &mut self,
        default: Option<&'ast ast::expression::Expression<Loc, Loc>>,
    ) -> Option<ast::expression::Expression<Loc, Loc>>
    where
        Loc: Dupe,
    {
        map_default_opt_default(self, default)
    }

    fn component_declaration(
        &mut self,
        loc: &'ast Loc,
        component: &'ast ast::statement::ComponentDeclaration<Loc, Type>,
    ) -> Result<(), E> {
        component_declaration_default(self, loc, component)
    }

    fn map_component_declaration(
        &mut self,
        loc: &'ast Loc,
        component: &'ast ast::statement::ComponentDeclaration<Loc, Loc>,
    ) -> ast::statement::ComponentDeclaration<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_component_declaration_default(self, loc, component)
    }

    fn component_identifier(&mut self, ident: &'ast ast::Identifier<Loc, Type>) -> Result<(), E> {
        component_identifier_default(self, ident)
    }

    fn map_component_identifier(
        &mut self,
        ident: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_component_identifier_default(self, ident)
    }

    fn component_params(
        &mut self,
        params: &'ast ast::statement::component_params::Params<Loc, Type>,
    ) -> Result<(), E> {
        component_params_default(self, params)
    }

    fn map_component_params(
        &mut self,
        params: &'ast ast::statement::component_params::Params<Loc, Loc>,
    ) -> ast::statement::component_params::Params<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_component_params_default(self, params)
    }

    fn component_param(
        &mut self,
        param: &'ast ast::statement::component_params::Param<Loc, Type>,
    ) -> Result<(), E> {
        component_param_default(self, param)
    }

    fn map_component_param(
        &mut self,
        param: &'ast ast::statement::component_params::Param<Loc, Loc>,
    ) -> ast::statement::component_params::Param<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_component_param_default(self, param)
    }

    fn component_param_name(
        &mut self,
        param_name: &'ast ast::statement::component_params::ParamName<Loc, Type>,
    ) -> Result<(), E> {
        component_param_name_default(self, param_name)
    }

    fn map_component_param_name(
        &mut self,
        param_name: &'ast ast::statement::component_params::ParamName<Loc, Loc>,
    ) -> ast::statement::component_params::ParamName<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_component_param_name_default(self, param_name)
    }

    fn component_param_pattern(
        &mut self,
        expr: &'ast ast::pattern::Pattern<Loc, Type>,
    ) -> Result<(), E> {
        component_param_pattern_default(self, expr)
    }

    fn map_component_param_pattern(
        &mut self,
        expr: &'ast ast::pattern::Pattern<Loc, Loc>,
    ) -> ast::pattern::Pattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_component_param_pattern_default(self, expr)
    }

    fn component_rest_param(
        &mut self,
        expr: &'ast ast::statement::component_params::RestParam<Loc, Type>,
    ) -> Result<(), E> {
        component_rest_param_default(self, expr)
    }

    fn map_component_rest_param(
        &mut self,
        rest: &'ast ast::statement::component_params::RestParam<Loc, Loc>,
    ) -> ast::statement::component_params::RestParam<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_component_rest_param_default(self, rest)
    }

    fn component_body(
        &mut self,
        body: &'ast (Loc, ast::statement::Block<Loc, Type>),
    ) -> Result<(), E> {
        component_body_default(self, body)
    }

    fn map_component_body(
        &mut self,
        body: &'ast (Loc, ast::statement::Block<Loc, Loc>),
    ) -> (Loc, ast::statement::Block<Loc, Loc>)
    where
        Loc: Dupe,
    {
        map_component_body_default(self, body)
    }

    fn conditional(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::Conditional<Loc, Type>,
    ) -> Result<(), E> {
        conditional_default(self, loc, expr)
    }

    fn map_conditional(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::Conditional<Loc, Loc>,
    ) -> ast::expression::Conditional<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_conditional_default(self, loc, expr)
    }

    fn continue_(
        &mut self,
        loc: &'ast Loc,
        cont: &'ast ast::statement::Continue<Loc>,
    ) -> Result<(), E> {
        continue_default(self, loc, cont)
    }

    fn map_continue_(
        &mut self,
        loc: &'ast Loc,
        cont: &'ast ast::statement::Continue<Loc>,
    ) -> ast::statement::Continue<Loc>
    where
        Loc: Dupe,
    {
        map_continue_default(self, loc, cont)
    }

    fn debugger(
        &mut self,
        loc: &'ast Loc,
        dbg: &'ast ast::statement::Debugger<Loc>,
    ) -> Result<(), E> {
        debugger_default(self, loc, dbg)
    }

    fn map_debugger(
        &mut self,
        loc: &'ast Loc,
        dbg: &'ast ast::statement::Debugger<Loc>,
    ) -> ast::statement::Debugger<Loc>
    where
        Loc: Dupe,
    {
        map_debugger_default(self, loc, dbg)
    }

    fn declare_class(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::DeclareClass<Loc, Type>,
    ) -> Result<(), E> {
        declare_class_default(self, loc, decl)
    }

    fn map_declare_class(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::DeclareClass<Loc, Loc>,
    ) -> ast::statement::DeclareClass<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_declare_class_default(self, loc, decl)
    }

    fn declare_component(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::DeclareComponent<Loc, Type>,
    ) -> Result<(), E> {
        declare_component_default(self, loc, decl)
    }

    fn map_declare_component(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::DeclareComponent<Loc, Loc>,
    ) -> ast::statement::DeclareComponent<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_declare_component_default(self, loc, decl)
    }

    fn component_type(
        &mut self,
        loc: &'ast Type,
        component: &'ast ast::types::Component<Loc, Type>,
    ) -> Result<(), E> {
        component_type_default(self, loc, component)
    }

    fn map_component_type(
        &mut self,
        loc: &'ast Loc,
        component: &'ast ast::types::Component<Loc, Loc>,
    ) -> ast::types::Component<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_component_type_default(self, loc, component)
    }

    fn component_type_params(
        &mut self,
        params: &'ast ast::types::component_params::Params<Loc, Type>,
    ) -> Result<(), E> {
        component_type_params_default(self, params)
    }

    fn map_component_type_params(
        &mut self,
        params: &'ast ast::types::component_params::Params<Loc, Loc>,
    ) -> ast::types::component_params::Params<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_component_type_params_default(self, params)
    }

    fn component_type_param(
        &mut self,
        param: &'ast ast::types::component_params::Param<Loc, Type>,
    ) -> Result<(), E> {
        component_type_param_default(self, param)
    }

    fn map_component_type_param(
        &mut self,
        param: &'ast ast::types::component_params::Param<Loc, Loc>,
    ) -> ast::types::component_params::Param<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_component_type_param_default(self, param)
    }

    fn component_type_rest_param(
        &mut self,
        expr: &'ast ast::types::component_params::RestParam<Loc, Type>,
    ) -> Result<(), E> {
        component_type_rest_param_default(self, expr)
    }

    fn map_component_type_rest_param(
        &mut self,
        expr: &'ast ast::types::component_params::RestParam<Loc, Loc>,
    ) -> ast::types::component_params::RestParam<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_component_type_rest_param_default(self, expr)
    }

    fn declare_enum(
        &mut self,
        loc: &'ast Loc,
        enum_: &'ast ast::statement::EnumDeclaration<Loc, Type>,
    ) -> Result<(), E> {
        declare_enum_default(self, loc, enum_)
    }

    fn map_declare_enum(
        &mut self,
        loc: &'ast Loc,
        enum_: &'ast ast::statement::EnumDeclaration<Loc, Loc>,
    ) -> ast::statement::EnumDeclaration<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_declare_enum_default(self, loc, enum_)
    }

    fn declare_export_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::DeclareExportDeclaration<Loc, Type>,
    ) -> Result<(), E> {
        declare_export_declaration_default(self, loc, decl)
    }

    fn map_declare_export_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::DeclareExportDeclaration<Loc, Loc>,
    ) -> ast::statement::DeclareExportDeclaration<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_declare_export_declaration_default(self, loc, decl)
    }

    fn declare_export_declaration_decl(
        &mut self,
        decl: &'ast ast::statement::declare_export_declaration::Declaration<Loc, Type>,
    ) -> Result<(), E> {
        declare_export_declaration_decl_default(self, decl)
    }

    fn map_declare_export_declaration_decl(
        &mut self,
        decl: &'ast ast::statement::declare_export_declaration::Declaration<Loc, Loc>,
    ) -> ast::statement::declare_export_declaration::Declaration<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_declare_export_declaration_decl_default(self, decl)
    }

    fn declare_function(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::DeclareFunction<Loc, Type>,
    ) -> Result<(), E> {
        declare_function_default(self, loc, decl)
    }

    fn map_declare_function(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::DeclareFunction<Loc, Loc>,
    ) -> ast::statement::DeclareFunction<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_declare_function_default(self, loc, decl)
    }

    fn declare_interface(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::Interface<Loc, Type>,
    ) -> Result<(), E> {
        declare_interface_default(self, loc, decl)
    }

    fn map_declare_interface(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::Interface<Loc, Loc>,
    ) -> ast::statement::Interface<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_declare_interface_default(self, loc, decl)
    }

    fn declare_module(
        &mut self,
        loc: &'ast Loc,
        m: &'ast ast::statement::DeclareModule<Loc, Type>,
    ) -> Result<(), E> {
        declare_module_default(self, loc, m)
    }

    fn map_declare_module(
        &mut self,
        loc: &'ast Loc,
        m: &'ast ast::statement::DeclareModule<Loc, Loc>,
    ) -> ast::statement::DeclareModule<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_declare_module_default(self, loc, m)
    }

    fn declare_module_exports(
        &mut self,
        loc: &'ast Loc,
        exports: &'ast ast::statement::DeclareModuleExports<Loc, Type>,
    ) -> Result<(), E> {
        declare_module_exports_default(self, loc, exports)
    }

    fn map_declare_module_exports(
        &mut self,
        loc: &'ast Loc,
        exports: &'ast ast::statement::DeclareModuleExports<Loc, Loc>,
    ) -> ast::statement::DeclareModuleExports<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_declare_module_exports_default(self, loc, exports)
    }

    fn declare_namespace(
        &mut self,
        loc: &'ast Loc,
        m: &'ast ast::statement::DeclareNamespace<Loc, Type>,
    ) -> Result<(), E> {
        declare_namespace_default(self, loc, m)
    }

    fn map_declare_namespace(
        &mut self,
        loc: &'ast Loc,
        ns: &'ast ast::statement::DeclareNamespace<Loc, Loc>,
    ) -> ast::statement::DeclareNamespace<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_declare_namespace_default(self, loc, ns)
    }

    fn declare_type_alias(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::TypeAlias<Loc, Type>,
    ) -> Result<(), E> {
        declare_type_alias_default(self, loc, decl)
    }

    fn map_declare_type_alias(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::TypeAlias<Loc, Loc>,
    ) -> ast::statement::TypeAlias<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_declare_type_alias_default(self, loc, decl)
    }

    fn declare_variable(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::DeclareVariable<Loc, Type>,
    ) -> Result<(), E> {
        declare_variable_default(self, loc, decl)
    }

    fn map_declare_variable(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::DeclareVariable<Loc, Loc>,
    ) -> ast::statement::DeclareVariable<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_declare_variable_default(self, loc, decl)
    }

    fn declare_variable_declarator(
        &mut self,
        kind: ast::VariableKind,
        decl: &'ast ast::statement::variable::Declarator<Loc, Type>,
    ) -> Result<(), E> {
        declare_variable_declarator_default(self, kind, decl)
    }

    fn map_declare_variable_declarator(
        &mut self,
        kind: ast::VariableKind,
        decl: &'ast ast::statement::variable::Declarator<Loc, Loc>,
    ) -> ast::statement::variable::Declarator<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_declare_variable_declarator_default(self, kind, decl)
    }

    fn do_while(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::DoWhile<Loc, Type>,
    ) -> Result<(), E> {
        do_while_default(self, loc, stmt)
    }

    fn map_do_while(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::DoWhile<Loc, Loc>,
    ) -> ast::statement::DoWhile<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_do_while_default(self, loc, stmt)
    }

    fn empty(&mut self, loc: &'ast Loc, empty: &'ast ast::statement::Empty<Loc>) -> Result<(), E> {
        empty_default(self, loc, empty)
    }

    fn map_empty(
        &mut self,
        loc: &'ast Loc,
        empty: &'ast ast::statement::Empty<Loc>,
    ) -> ast::statement::Empty<Loc>
    where
        Loc: Dupe,
    {
        map_empty_default(self, loc, empty)
    }

    fn enum_declaration(
        &mut self,
        loc: &'ast Loc,
        enum_: &'ast ast::statement::EnumDeclaration<Loc, Type>,
    ) -> Result<(), E> {
        enum_declaration_default(self, loc, enum_)
    }

    fn map_enum_declaration(
        &mut self,
        loc: &'ast Loc,
        enum_: &'ast ast::statement::EnumDeclaration<Loc, Loc>,
    ) -> ast::statement::EnumDeclaration<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_enum_declaration_default(self, loc, enum_)
    }

    fn enum_body(
        &mut self,
        body: &'ast ast::statement::enum_declaration::Body<Loc>,
    ) -> Result<(), E> {
        enum_body_default(self, body)
    }

    fn map_enum_body(
        &mut self,
        body: &'ast ast::statement::enum_declaration::Body<Loc>,
    ) -> ast::statement::enum_declaration::Body<Loc>
    where
        Loc: Dupe,
    {
        map_enum_body_default(self, body)
    }

    fn enum_member(
        &mut self,
        member: &'ast ast::statement::enum_declaration::Member<Loc>,
    ) -> Result<(), E> {
        enum_member_default(self, member)
    }

    fn map_enum_member(
        &mut self,
        member: &'ast ast::statement::enum_declaration::Member<Loc>,
    ) -> ast::statement::enum_declaration::Member<Loc>
    where
        Loc: Dupe,
    {
        map_enum_member_default(self, member)
    }

    fn enum_defaulted_member(
        &mut self,
        member: &'ast ast::statement::enum_declaration::DefaultedMember<Loc>,
    ) -> Result<(), E> {
        enum_defaulted_member_default(self, member)
    }

    fn map_enum_defaulted_member(
        &mut self,
        member: &'ast ast::statement::enum_declaration::DefaultedMember<Loc>,
    ) -> ast::statement::enum_declaration::DefaultedMember<Loc>
    where
        Loc: Dupe,
    {
        map_enum_defaulted_member_default(self, member)
    }

    fn enum_boolean_member(
        &mut self,
        member: &'ast ast::statement::enum_declaration::InitializedMember<
            ast::BooleanLiteral<Loc>,
            Loc,
        >,
    ) -> Result<(), E> {
        enum_boolean_member_default(self, member)
    }

    fn map_enum_boolean_member(
        &mut self,
        member: &'ast ast::statement::enum_declaration::InitializedMember<
            ast::BooleanLiteral<Loc>,
            Loc,
        >,
    ) -> ast::statement::enum_declaration::InitializedMember<ast::BooleanLiteral<Loc>, Loc>
    where
        Loc: Dupe,
    {
        map_enum_boolean_member_default(self, member)
    }

    fn enum_number_member(
        &mut self,
        member: &'ast ast::statement::enum_declaration::InitializedMember<
            ast::NumberLiteral<Loc>,
            Loc,
        >,
    ) -> Result<(), E> {
        enum_number_member_default(self, member)
    }

    fn map_enum_number_member(
        &mut self,
        member: &'ast ast::statement::enum_declaration::InitializedMember<
            ast::NumberLiteral<Loc>,
            Loc,
        >,
    ) -> ast::statement::enum_declaration::InitializedMember<ast::NumberLiteral<Loc>, Loc>
    where
        Loc: Dupe,
    {
        map_enum_number_member_default(self, member)
    }

    fn enum_string_member(
        &mut self,
        member: &'ast ast::statement::enum_declaration::InitializedMember<
            ast::StringLiteral<Loc>,
            Loc,
        >,
    ) -> Result<(), E> {
        enum_string_member_default(self, member)
    }

    fn map_enum_string_member(
        &mut self,
        member: &'ast ast::statement::enum_declaration::InitializedMember<
            ast::StringLiteral<Loc>,
            Loc,
        >,
    ) -> ast::statement::enum_declaration::InitializedMember<ast::StringLiteral<Loc>, Loc>
    where
        Loc: Dupe,
    {
        map_enum_string_member_default(self, member)
    }

    fn enum_bigint_member(
        &mut self,
        member: &'ast ast::statement::enum_declaration::InitializedMember<
            ast::BigIntLiteral<Loc>,
            Loc,
        >,
    ) -> Result<(), E> {
        enum_bigint_member_default(self, member)
    }

    fn map_enum_bigint_member(
        &mut self,
        member: &'ast ast::statement::enum_declaration::InitializedMember<
            ast::BigIntLiteral<Loc>,
            Loc,
        >,
    ) -> ast::statement::enum_declaration::InitializedMember<ast::BigIntLiteral<Loc>, Loc>
    where
        Loc: Dupe,
    {
        map_enum_bigint_member_default(self, member)
    }

    fn enum_member_name(
        &mut self,
        id: &'ast ast::statement::enum_declaration::MemberName<Loc>,
    ) -> Result<(), E> {
        enum_member_name_default(self, id)
    }

    fn map_enum_member_name(
        &mut self,
        id: &'ast ast::statement::enum_declaration::MemberName<Loc>,
    ) -> ast::statement::enum_declaration::MemberName<Loc>
    where
        Loc: Dupe,
    {
        map_enum_member_name_default(self, id)
    }

    fn enum_member_identifier(&mut self, id: &'ast ast::Identifier<Loc, Loc>) -> Result<(), E> {
        enum_member_identifier_default(self, id)
    }

    fn map_enum_member_identifier(
        &mut self,
        id: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_enum_member_identifier_default(self, id)
    }

    fn export_default_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::ExportDefaultDeclaration<Loc, Type>,
    ) -> Result<(), E> {
        export_default_declaration_default(self, loc, decl)
    }

    fn map_export_default_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::ExportDefaultDeclaration<Loc, Loc>,
    ) -> ast::statement::ExportDefaultDeclaration<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_export_default_declaration_default(self, loc, decl)
    }

    fn export_default_declaration_decl(
        &mut self,
        decl: &'ast ast::statement::export_default_declaration::Declaration<Loc, Type>,
    ) -> Result<(), E> {
        export_default_declaration_decl_default(self, decl)
    }

    fn map_export_default_declaration_decl(
        &mut self,
        decl: &'ast ast::statement::export_default_declaration::Declaration<Loc, Loc>,
    ) -> ast::statement::export_default_declaration::Declaration<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_export_default_declaration_decl_default(self, decl)
    }

    fn export_named_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::ExportNamedDeclaration<Loc, Type>,
    ) -> Result<(), E> {
        export_named_declaration_default(self, loc, decl)
    }

    fn map_export_named_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::ExportNamedDeclaration<Loc, Loc>,
    ) -> ast::statement::ExportNamedDeclaration<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_export_named_declaration_default(self, loc, decl)
    }

    fn export_assignment(
        &mut self,
        loc: &'ast Loc,
        assign: &'ast ast::statement::ExportAssignment<Loc, Type>,
    ) -> Result<(), E> {
        export_assignment_default(self, loc, assign)
    }

    fn map_export_assignment(
        &mut self,
        loc: &'ast Loc,
        assign: &'ast ast::statement::ExportAssignment<Loc, Loc>,
    ) -> ast::statement::ExportAssignment<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_export_assignment_default(self, loc, assign)
    }

    fn namespace_export_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::NamespaceExportDeclaration<Loc, Type>,
    ) -> Result<(), E> {
        namespace_export_declaration_default(self, loc, decl)
    }

    fn map_namespace_export_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::NamespaceExportDeclaration<Loc, Loc>,
    ) -> ast::statement::NamespaceExportDeclaration<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_namespace_export_declaration_default(self, loc, decl)
    }

    fn export_named_declaration_specifier(
        &mut self,
        spec: &'ast ast::statement::export_named_declaration::ExportSpecifier<Loc, Type>,
    ) -> Result<(), E> {
        export_named_declaration_specifier_default(self, spec)
    }

    fn map_export_named_declaration_specifier(
        &mut self,
        spec: &'ast ast::statement::export_named_declaration::ExportSpecifier<Loc, Loc>,
    ) -> ast::statement::export_named_declaration::ExportSpecifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_export_named_declaration_specifier_default(self, spec)
    }

    fn export_batch_specifier(
        &mut self,
        spec: &'ast ast::statement::export_named_declaration::ExportBatchSpecifier<Loc, Type>,
    ) -> Result<(), E> {
        export_batch_specifier_default(self, spec)
    }

    fn map_export_batch_specifier(
        &mut self,
        spec: &'ast ast::statement::export_named_declaration::ExportBatchSpecifier<Loc, Loc>,
    ) -> ast::statement::export_named_declaration::ExportBatchSpecifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_export_batch_specifier_default(self, spec)
    }

    fn export_named_specifier(
        &mut self,
        spec: &'ast ast::statement::export_named_declaration::Specifier<Loc, Type>,
    ) -> Result<(), E> {
        export_named_specifier_default(self, spec)
    }

    fn map_export_named_specifier(
        &mut self,
        spec: &'ast ast::statement::export_named_declaration::Specifier<Loc, Loc>,
    ) -> ast::statement::export_named_declaration::Specifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_export_named_specifier_default(self, spec)
    }

    fn export_source(
        &mut self,
        loc: &'ast Type,
        source: &'ast ast::StringLiteral<Loc>,
    ) -> Result<(), E> {
        export_source_default(self, loc, source)
    }

    fn map_export_source(
        &mut self,
        loc: &'ast Loc,
        source: &'ast ast::StringLiteral<Loc>,
    ) -> ast::StringLiteral<Loc>
    where
        Loc: Dupe,
    {
        map_export_source_default(self, loc, source)
    }

    fn expression_statement(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::Expression<Loc, Type>,
    ) -> Result<(), E> {
        expression_statement_default(self, loc, stmt)
    }

    fn map_expression_statement(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::Expression<Loc, Loc>,
    ) -> ast::statement::Expression<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_expression_statement_default(self, loc, stmt)
    }

    fn expression_or_spread(
        &mut self,
        expr_or_spread: &'ast ast::expression::ExpressionOrSpread<Loc, Type>,
    ) -> Result<(), E> {
        expression_or_spread_default(self, expr_or_spread)
    }

    fn map_expression_or_spread(
        &mut self,
        eos: &'ast ast::expression::ExpressionOrSpread<Loc, Loc>,
    ) -> ast::expression::ExpressionOrSpread<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_expression_or_spread_default(self, eos)
    }

    fn for_in_statement(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::ForIn<Loc, Type>,
    ) -> Result<(), E> {
        for_in_statement_default(self, loc, stmt)
    }

    fn map_for_in_statement(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::ForIn<Loc, Loc>,
    ) -> ast::statement::ForIn<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_for_in_statement_default(self, loc, stmt)
    }

    fn for_in_statement_lhs(
        &mut self,
        left: &'ast ast::statement::for_in::Left<Loc, Type>,
    ) -> Result<(), E> {
        for_in_statement_lhs_default(self, left)
    }

    fn map_for_in_statement_lhs(
        &mut self,
        lhs: &'ast ast::statement::for_in::Left<Loc, Loc>,
    ) -> ast::statement::for_in::Left<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_for_in_statement_lhs_default(self, lhs)
    }

    fn for_in_left_declaration(
        &mut self,
        left: &'ast (Loc, ast::statement::VariableDeclaration<Loc, Type>),
    ) -> Result<(), E> {
        for_in_left_declaration_default(self, left)
    }

    fn map_for_in_left_declaration(
        &mut self,
        left: &'ast (Loc, ast::statement::VariableDeclaration<Loc, Loc>),
    ) -> (Loc, ast::statement::VariableDeclaration<Loc, Loc>)
    where
        Loc: Dupe,
    {
        map_for_in_left_declaration_default(self, left)
    }

    fn for_of_statement(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::ForOf<Loc, Type>,
    ) -> Result<(), E> {
        for_of_statement_default(self, loc, stmt)
    }

    fn map_for_of_statement(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::ForOf<Loc, Loc>,
    ) -> ast::statement::ForOf<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_for_of_statement_default(self, loc, stmt)
    }

    fn for_of_statement_lhs(
        &mut self,
        left: &'ast ast::statement::for_of::Left<Loc, Type>,
    ) -> Result<(), E> {
        for_of_statement_lhs_default(self, left)
    }

    fn map_for_of_statement_lhs(
        &mut self,
        lhs: &'ast ast::statement::for_of::Left<Loc, Loc>,
    ) -> ast::statement::for_of::Left<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_for_of_statement_lhs_default(self, lhs)
    }

    fn for_of_left_declaration(
        &mut self,
        left: &'ast (Loc, ast::statement::VariableDeclaration<Loc, Type>),
    ) -> Result<(), E> {
        for_of_left_declaration_default(self, left)
    }

    fn map_for_of_left_declaration(
        &mut self,
        left: &'ast (Loc, ast::statement::VariableDeclaration<Loc, Loc>),
    ) -> (Loc, ast::statement::VariableDeclaration<Loc, Loc>)
    where
        Loc: Dupe,
    {
        map_for_of_left_declaration_default(self, left)
    }

    fn for_statement(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::For<Loc, Type>,
    ) -> Result<(), E> {
        for_statement_default(self, loc, stmt)
    }

    fn map_for_statement(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::For<Loc, Loc>,
    ) -> ast::statement::For<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_for_statement_default(self, loc, stmt)
    }

    fn for_statement_init(
        &mut self,
        init: &'ast ast::statement::for_::Init<Loc, Type>,
    ) -> Result<(), E> {
        for_statement_init_default(self, init)
    }

    fn map_for_statement_init(
        &mut self,
        init: &'ast ast::statement::for_::Init<Loc, Loc>,
    ) -> ast::statement::for_::Init<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_for_statement_init_default(self, init)
    }

    fn for_init_declaration(
        &mut self,
        init: &'ast (Loc, ast::statement::VariableDeclaration<Loc, Type>),
    ) -> Result<(), E> {
        for_init_declaration_default(self, init)
    }

    fn map_for_init_declaration(
        &mut self,
        init: &'ast (Loc, ast::statement::VariableDeclaration<Loc, Loc>),
    ) -> (Loc, ast::statement::VariableDeclaration<Loc, Loc>)
    where
        Loc: Dupe,
    {
        map_for_init_declaration_default(self, init)
    }

    fn function_param_type(
        &mut self,
        fpt: &'ast ast::types::function::Param<Loc, Type>,
    ) -> Result<(), E> {
        function_param_type_default(self, fpt)
    }

    fn map_function_param_type(
        &mut self,
        fpt: &'ast ast::types::function::Param<Loc, Loc>,
    ) -> ast::types::function::Param<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_function_param_type_default(self, fpt)
    }

    fn function_param_type_identifier(
        &mut self,
        id: &'ast ast::Identifier<Loc, Type>,
    ) -> Result<(), E> {
        self.identifier(id)
    }

    fn map_function_param_type_identifier(
        &mut self,
        id: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        self.map_identifier(id)
    }

    fn function_param_type_pattern(
        &mut self,
        patt: &'ast ast::pattern::Pattern<Loc, Type>,
    ) -> Result<(), E> {
        self.pattern(None, patt)
    }

    fn map_function_param_type_pattern(
        &mut self,
        patt: &'ast ast::pattern::Pattern<Loc, Loc>,
    ) -> ast::pattern::Pattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        self.map_pattern(None, patt)
    }

    fn function_rest_param_type(
        &mut self,
        frpt: &'ast ast::types::function::RestParam<Loc, Type>,
    ) -> Result<(), E> {
        function_rest_param_type_default(self, frpt)
    }

    fn map_function_rest_param_type(
        &mut self,
        frpt: &'ast ast::types::function::RestParam<Loc, Loc>,
    ) -> ast::types::function::RestParam<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_function_rest_param_type_default(self, frpt)
    }

    fn function_this_param_type(
        &mut self,
        this_param: &'ast ast::types::function::ThisParam<Loc, Type>,
    ) -> Result<(), E> {
        function_this_param_type_default(self, this_param)
    }

    fn map_function_this_param_type(
        &mut self,
        this_param: &'ast ast::types::function::ThisParam<Loc, Loc>,
    ) -> ast::types::function::ThisParam<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_function_this_param_type_default(self, this_param)
    }

    fn function_type_return_annotation(
        &mut self,
        return_: &'ast ast::types::function::ReturnAnnotation<Loc, Type>,
    ) -> Result<(), E> {
        function_type_return_annotation_default(self, return_)
    }

    fn map_function_type_return_annotation(
        &mut self,
        return_: &'ast ast::types::function::ReturnAnnotation<Loc, Loc>,
    ) -> ast::types::function::ReturnAnnotation<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_function_type_return_annotation_default(self, return_)
    }

    fn function_type(&mut self, ft: &'ast ast::types::Function<Loc, Type>) -> Result<(), E> {
        function_type_default(self, ft)
    }

    fn map_function_type(
        &mut self,
        loc: &'ast Loc,
        func: &'ast ast::types::Function<Loc, Loc>,
    ) -> ast::types::Function<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_function_type_default(self, loc, func)
    }

    fn label_identifier(&mut self, ident: &'ast ast::Identifier<Loc, Loc>) -> Result<(), E> {
        label_identifier_default(self, ident)
    }

    fn map_label_identifier(
        &mut self,
        ident: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_label_identifier_default(self, ident)
    }

    fn object_property_value_type(
        &mut self,
        opvt: &'ast ast::types::object::PropertyValue<Loc, Type>,
    ) -> Result<(), E> {
        object_property_value_type_default(self, opvt)
    }

    fn map_object_property_value_type(
        &mut self,
        opvt: &'ast ast::types::object::PropertyValue<Loc, Loc>,
    ) -> ast::types::object::PropertyValue<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_object_property_value_type_default(self, opvt)
    }

    fn object_type_property_getter(
        &mut self,
        loc: &'ast Loc,
        getter: &'ast ast::types::Function<Loc, Type>,
    ) -> Result<(), E> {
        object_type_property_getter_default(self, loc, getter)
    }

    fn map_object_type_property_getter(
        &mut self,
        loc: &'ast Loc,
        getter: &'ast ast::types::Function<Loc, Loc>,
    ) -> ast::types::Function<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_object_type_property_getter_default(self, loc, getter)
    }

    fn object_type_property_setter(
        &mut self,
        loc: &'ast Loc,
        setter: &'ast ast::types::Function<Loc, Type>,
    ) -> Result<(), E> {
        object_type_property_setter_default(self, loc, setter)
    }

    fn map_object_type_property_setter(
        &mut self,
        loc: &'ast Loc,
        setter: &'ast ast::types::Function<Loc, Loc>,
    ) -> ast::types::Function<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_object_type_property_setter_default(self, loc, setter)
    }

    fn object_property_type(
        &mut self,
        opt: &'ast ast::types::object::NormalProperty<Loc, Type>,
    ) -> Result<(), E> {
        object_property_type_default(self, opt)
    }

    fn map_object_property_type(
        &mut self,
        opt: &'ast ast::types::object::NormalProperty<Loc, Loc>,
    ) -> ast::types::object::NormalProperty<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_object_property_type_default(self, opt)
    }

    fn object_spread_property_type(
        &mut self,
        spread: &'ast ast::types::object::SpreadProperty<Loc, Type>,
    ) -> Result<(), E> {
        object_spread_property_type_default(self, spread)
    }

    fn map_object_spread_property_type(
        &mut self,
        spread: &'ast ast::types::object::SpreadProperty<Loc, Loc>,
    ) -> ast::types::object::SpreadProperty<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_object_spread_property_type_default(self, spread)
    }

    fn object_indexer_property_type(
        &mut self,
        indexer: &'ast ast::types::object::Indexer<Loc, Type>,
    ) -> Result<(), E> {
        object_indexer_property_type_default(self, indexer)
    }

    fn map_object_indexer_property_type(
        &mut self,
        indexer: &'ast ast::types::object::Indexer<Loc, Loc>,
    ) -> ast::types::object::Indexer<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_object_indexer_property_type_default(self, indexer)
    }

    fn object_internal_slot_property_type(
        &mut self,
        slot: &'ast ast::types::object::InternalSlot<Loc, Type>,
    ) -> Result<(), E> {
        object_internal_slot_property_type_default(self, slot)
    }

    fn map_object_internal_slot_property_type(
        &mut self,
        slot: &'ast ast::types::object::InternalSlot<Loc, Loc>,
    ) -> ast::types::object::InternalSlot<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_object_internal_slot_property_type_default(self, slot)
    }

    fn object_call_property_type(
        &mut self,
        call_prop: &'ast ast::types::object::CallProperty<Loc, Type>,
    ) -> Result<(), E> {
        object_call_property_type_default(self, call_prop)
    }

    fn map_object_call_property_type(
        &mut self,
        call_prop: &'ast ast::types::object::CallProperty<Loc, Loc>,
    ) -> ast::types::object::CallProperty<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_object_call_property_type_default(self, call_prop)
    }

    fn object_mapped_type_property(
        &mut self,
        mapped: &'ast ast::types::object::MappedType<Loc, Type>,
    ) -> Result<(), E> {
        object_mapped_type_property_default(self, mapped)
    }

    fn map_object_mapped_type_property(
        &mut self,
        mapped: &'ast ast::types::object::MappedType<Loc, Loc>,
    ) -> ast::types::object::MappedType<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_object_mapped_type_property_default(self, mapped)
    }

    fn object_private_field_type(
        &mut self,
        pf: &'ast ast::types::object::PrivateField<Loc>,
    ) -> Result<(), E>
    where
        Loc: Dupe,
    {
        object_private_field_type_default(self, pf)
    }

    fn map_object_private_field_type(
        &mut self,
        pf: &'ast ast::types::object::PrivateField<Loc>,
    ) -> ast::types::object::PrivateField<Loc>
    where
        Loc: Dupe,
    {
        map_object_private_field_type_default(self, pf)
    }

    fn object_type(&mut self, ot: &'ast ast::types::Object<Loc, Type>) -> Result<(), E> {
        object_type_default(self, ot)
    }

    fn map_object_type(
        &mut self,
        loc: &'ast Loc,
        object: &'ast ast::types::Object<Loc, Loc>,
    ) -> ast::types::Object<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_object_type_default(self, loc, object)
    }

    fn object_type_property(
        &mut self,
        p: &'ast ast::types::object::Property<Loc, Type>,
    ) -> Result<(), E> {
        object_type_property_default(self, p)
    }

    fn map_object_type_property(
        &mut self,
        prop: &'ast ast::types::object::Property<Loc, Loc>,
    ) -> ast::types::object::Property<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_object_type_property_default(self, prop)
    }

    fn interface_type(
        &mut self,
        loc: &'ast Type,
        i: &'ast ast::types::Interface<Loc, Type>,
    ) -> Result<(), E> {
        interface_type_default(self, loc, i)
    }

    fn map_interface_type(
        &mut self,
        loc: &'ast Loc,
        interface: &'ast ast::types::Interface<Loc, Loc>,
    ) -> ast::types::Interface<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_interface_type_default(self, loc, interface)
    }

    fn generic_identifier_type(
        &mut self,
        git: &'ast ast::types::generic::Identifier<Loc, Type>,
    ) -> Result<(), E> {
        generic_identifier_type_default(self, git)
    }

    fn map_generic_identifier_type(
        &mut self,
        git: &'ast ast::types::generic::Identifier<Loc, Loc>,
    ) -> ast::types::generic::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_generic_identifier_type_default(self, git)
    }

    fn generic_identifier_import_type(
        &mut self,
        import_type: &'ast ast::types::generic::ImportType<Loc, Type>,
    ) -> Result<(), E> {
        generic_identifier_import_type_default(self, import_type)
    }

    fn map_generic_identifier_import_type(
        &mut self,
        import_type: &'ast ast::types::generic::ImportType<Loc, Loc>,
    ) -> ast::types::generic::ImportType<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_generic_identifier_import_type_default(self, import_type)
    }

    fn generic_qualified_identifier_type(
        &mut self,
        qual: &'ast ast::types::generic::Qualified<Loc, Type>,
    ) -> Result<(), E> {
        generic_qualified_identifier_type_default(self, qual)
    }

    fn map_generic_qualified_identifier_type(
        &mut self,
        qual: &'ast ast::types::generic::Qualified<Loc, Loc>,
    ) -> ast::types::generic::Qualified<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_generic_qualified_identifier_type_default(self, qual)
    }

    fn member_type_identifier(&mut self, id: &'ast ast::Identifier<Loc, Type>) -> Result<(), E> {
        member_type_identifier_default(self, id)
    }

    fn map_member_type_identifier(
        &mut self,
        id: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_member_type_identifier_default(self, id)
    }

    fn variance(&mut self, variance: &'ast ast::Variance<Loc>) -> Result<(), E> {
        variance_default(self, variance)
    }

    fn map_variance(&mut self, variance: &'ast ast::Variance<Loc>) -> ast::Variance<Loc>
    where
        Loc: Dupe,
    {
        map_variance_default(self, variance)
    }

    fn variance_opt(&mut self, opt: Option<&'ast ast::Variance<Loc>>) -> Result<(), E> {
        variance_opt_default(self, opt)
    }

    fn map_variance_opt(
        &mut self,
        opt: Option<&'ast ast::Variance<Loc>>,
    ) -> Option<ast::Variance<Loc>>
    where
        Loc: Dupe,
    {
        map_variance_opt_default(self, opt)
    }

    fn tparam_const_modifier(
        &mut self,
        c: &'ast ast::types::type_param::ConstModifier<Loc>,
    ) -> Result<(), E> {
        tparam_const_modifier_default(self, c)
    }

    fn map_tparam_const_modifier(
        &mut self,
        c: &'ast ast::types::type_param::ConstModifier<Loc>,
    ) -> ast::types::type_param::ConstModifier<Loc>
    where
        Loc: Dupe,
    {
        map_tparam_const_modifier_default(self, c)
    }

    fn type_args(&mut self, targs: &'ast ast::types::TypeArgs<Loc, Type>) -> Result<(), E> {
        type_args_default(self, targs)
    }

    fn map_type_args(
        &mut self,
        targs: &'ast ast::types::TypeArgs<Loc, Loc>,
    ) -> ast::types::TypeArgs<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_type_args_default(self, targs)
    }

    fn type_params(
        &mut self,
        kind: &TypeParamsContext,
        tparams: &'ast ast::types::TypeParams<Loc, Type>,
    ) -> Result<(), E> {
        type_params_default(self, kind, tparams)
    }

    fn map_type_params(
        &mut self,
        kind: &TypeParamsContext,
        tparams: &'ast ast::types::TypeParams<Loc, Loc>,
    ) -> ast::types::TypeParams<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_type_params_default(self, kind, tparams)
    }

    fn type_param(
        &mut self,
        kind: &TypeParamsContext,
        tparam: &'ast ast::types::TypeParam<Loc, Type>,
    ) -> Result<(), E> {
        type_param_default(self, kind, tparam)
    }

    fn map_type_param(
        &mut self,
        kind: &TypeParamsContext,
        tparam: &'ast ast::types::TypeParam<Loc, Loc>,
    ) -> ast::types::TypeParam<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_type_param_default(self, kind, tparam)
    }

    fn generic_type(&mut self, gt: &'ast ast::types::Generic<Loc, Type>) -> Result<(), E> {
        generic_type_default(self, gt)
    }

    fn map_generic_type(
        &mut self,
        loc: &'ast Loc,
        generic: &'ast ast::types::Generic<Loc, Loc>,
    ) -> ast::types::Generic<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_generic_type_default(self, loc, generic)
    }

    fn indexed_access_type(
        &mut self,
        loc: &'ast Type,
        ia: &'ast ast::types::IndexedAccess<Loc, Type>,
    ) -> Result<(), E> {
        indexed_access_type_default(self, loc, ia)
    }

    fn map_indexed_access_type(
        &mut self,
        loc: &'ast Loc,
        indexed: &'ast ast::types::IndexedAccess<Loc, Loc>,
    ) -> ast::types::IndexedAccess<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_indexed_access_type_default(self, loc, indexed)
    }

    fn optional_indexed_access_type(
        &mut self,
        loc: &'ast Type,
        ia: &'ast ast::types::OptionalIndexedAccess<Loc, Type>,
    ) -> Result<(), E> {
        optional_indexed_access_type_default(self, loc, ia)
    }

    fn map_optional_indexed_access_type(
        &mut self,
        loc: &'ast Loc,
        indexed: &'ast ast::types::OptionalIndexedAccess<Loc, Loc>,
    ) -> ast::types::OptionalIndexedAccess<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_optional_indexed_access_type_default(self, loc, indexed)
    }

    fn string_literal(&mut self, lit: &'ast ast::StringLiteral<Loc>) -> Result<(), E> {
        string_literal_default(self, lit)
    }

    fn map_string_literal(&mut self, lit: &'ast ast::StringLiteral<Loc>) -> ast::StringLiteral<Loc>
    where
        Loc: Dupe,
    {
        map_string_literal_default(self, lit)
    }

    fn number_literal(&mut self, lit: &'ast ast::NumberLiteral<Loc>) -> Result<(), E> {
        number_literal_default(self, lit)
    }

    fn map_number_literal(&mut self, lit: &'ast ast::NumberLiteral<Loc>) -> ast::NumberLiteral<Loc>
    where
        Loc: Dupe,
    {
        map_number_literal_default(self, lit)
    }

    fn bigint_literal(&mut self, lit: &'ast ast::BigIntLiteral<Loc>) -> Result<(), E> {
        bigint_literal_default(self, lit)
    }

    fn map_bigint_literal(&mut self, lit: &'ast ast::BigIntLiteral<Loc>) -> ast::BigIntLiteral<Loc>
    where
        Loc: Dupe,
    {
        map_bigint_literal_default(self, lit)
    }

    fn boolean_literal(&mut self, lit: &'ast ast::BooleanLiteral<Loc>) -> Result<(), E> {
        boolean_literal_default(self, lit)
    }

    fn map_boolean_literal(
        &mut self,
        lit: &'ast ast::BooleanLiteral<Loc>,
    ) -> ast::BooleanLiteral<Loc>
    where
        Loc: Dupe,
    {
        map_boolean_literal_default(self, lit)
    }

    fn null_literal(&mut self, comments: Option<&'ast ast::Syntax<Loc, ()>>) -> Result<(), E> {
        null_literal_default(self, comments)
    }

    fn map_null_literal(
        &mut self,
        comments: Option<&'ast ast::Syntax<Loc, ()>>,
    ) -> Option<ast::Syntax<Loc, ()>>
    where
        Loc: Dupe,
    {
        map_null_literal_default(self, comments)
    }

    fn regexp_literal(&mut self, lit: &'ast ast::RegExpLiteral<Loc>) -> Result<(), E> {
        regexp_literal_default(self, lit)
    }

    fn map_regexp_literal(&mut self, lit: &'ast ast::RegExpLiteral<Loc>) -> ast::RegExpLiteral<Loc>
    where
        Loc: Dupe,
    {
        map_regexp_literal_default(self, lit)
    }

    fn module_ref_literal(&mut self, lit: &'ast ast::ModuleRefLiteral<Loc>) -> Result<(), E> {
        module_ref_literal_default(self, lit)
    }

    fn map_module_ref_literal(
        &mut self,
        lit: &'ast ast::ModuleRefLiteral<Loc>,
    ) -> ast::ModuleRefLiteral<Loc>
    where
        Loc: Dupe,
    {
        map_module_ref_literal_default(self, lit)
    }

    fn nullable_type(&mut self, nullable: &'ast ast::types::Nullable<Loc, Type>) -> Result<(), E> {
        nullable_type_default(self, nullable)
    }

    fn map_nullable_type(
        &mut self,
        loc: &'ast Loc,
        nullable: &'ast ast::types::Nullable<Loc, Loc>,
    ) -> ast::types::Nullable<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_nullable_type_default(self, loc, nullable)
    }

    fn conditional_type(
        &mut self,
        conditional: &'ast ast::types::Conditional<Loc, Type>,
    ) -> Result<(), E> {
        conditional_type_default(self, conditional)
    }

    fn map_conditional_type(
        &mut self,
        loc: &'ast Loc,
        conditional: &'ast ast::types::Conditional<Loc, Loc>,
    ) -> ast::types::Conditional<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_conditional_type_default(self, loc, conditional)
    }

    fn infer_type(&mut self, infer: &'ast ast::types::Infer<Loc, Type>) -> Result<(), E> {
        infer_type_default(self, infer)
    }

    fn map_infer_type(
        &mut self,
        loc: &'ast Loc,
        infer: &'ast ast::types::Infer<Loc, Loc>,
    ) -> ast::types::Infer<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_infer_type_default(self, loc, infer)
    }

    fn typeof_type(&mut self, typeof_: &'ast ast::types::Typeof<Loc, Type>) -> Result<(), E> {
        typeof_type_default(self, typeof_)
    }

    fn map_typeof_type(
        &mut self,
        loc: &'ast Loc,
        typeof_: &'ast ast::types::Typeof<Loc, Loc>,
    ) -> ast::types::Typeof<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_typeof_type_default(self, loc, typeof_)
    }

    fn typeof_expression(
        &mut self,
        git: &'ast ast::types::typeof_::Target<Loc, Type>,
    ) -> Result<(), E> {
        typeof_expression_default(self, git)
    }

    fn map_typeof_expression(
        &mut self,
        target: &'ast ast::types::typeof_::Target<Loc, Loc>,
    ) -> ast::types::typeof_::Target<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_typeof_expression_default(self, target)
    }

    fn typeof_identifier(&mut self, id: &'ast ast::Identifier<Loc, Type>) -> Result<(), E> {
        typeof_identifier_default(self, id)
    }

    fn map_typeof_identifier(
        &mut self,
        id: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_typeof_identifier_default(self, id)
    }

    fn typeof_member_identifier(&mut self, id: &'ast ast::Identifier<Loc, Type>) -> Result<(), E> {
        typeof_member_identifier_default(self, id)
    }

    fn map_typeof_member_identifier(
        &mut self,
        id: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_typeof_member_identifier_default(self, id)
    }

    fn typeof_qualified_identifier(
        &mut self,
        qual: &'ast ast::types::typeof_::Qualified<Loc, Type>,
    ) -> Result<(), E> {
        typeof_qualified_identifier_default(self, qual)
    }

    fn map_typeof_qualified_identifier(
        &mut self,
        qual: &'ast ast::types::typeof_::Qualified<Loc, Loc>,
    ) -> ast::types::typeof_::Qualified<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_typeof_qualified_identifier_default(self, qual)
    }

    fn keyof_type(&mut self, keyof: &'ast ast::types::Keyof<Loc, Type>) -> Result<(), E> {
        keyof_type_default(self, keyof)
    }

    fn map_keyof_type(
        &mut self,
        loc: &'ast Loc,
        keyof: &'ast ast::types::Keyof<Loc, Loc>,
    ) -> ast::types::Keyof<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_keyof_type_default(self, loc, keyof)
    }

    fn render_type(&mut self, renders: &'ast ast::types::Renders<Loc, Type>) -> Result<(), E> {
        render_type_default(self, renders)
    }

    fn map_render_type(
        &mut self,
        loc: &'ast Loc,
        renders: &'ast ast::types::Renders<Loc, Loc>,
    ) -> ast::types::Renders<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_render_type_default(self, loc, renders)
    }

    fn readonly_type(&mut self, readonly: &'ast ast::types::ReadOnly<Loc, Type>) -> Result<(), E> {
        readonly_type_default(self, readonly)
    }

    fn map_readonly_type(
        &mut self,
        loc: &'ast Loc,
        readonly: &'ast ast::types::ReadOnly<Loc, Loc>,
    ) -> ast::types::ReadOnly<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_readonly_type_default(self, loc, readonly)
    }

    fn tuple_type(&mut self, tuple: &'ast ast::types::Tuple<Loc, Type>) -> Result<(), E> {
        tuple_type_default(self, tuple)
    }

    fn map_tuple_type(
        &mut self,
        loc: &'ast Loc,
        tuple: &'ast ast::types::Tuple<Loc, Loc>,
    ) -> ast::types::Tuple<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_tuple_type_default(self, loc, tuple)
    }

    fn template_literal_type(
        &mut self,
        t: &'ast ast::types::TypeTemplateLiteral<Loc, Type>,
    ) -> Result<(), E> {
        template_literal_type_default(self, t)
    }

    fn map_template_literal_type(
        &mut self,
        _loc: &'ast Loc,
        t: &'ast ast::types::TypeTemplateLiteral<Loc, Loc>,
    ) -> ast::types::TypeTemplateLiteral<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_template_literal_type_default(self, _loc, t)
    }

    fn tuple_element(&mut self, el: &'ast ast::types::tuple::Element<Loc, Type>) -> Result<(), E> {
        tuple_element_default(self, el)
    }

    fn map_tuple_element(
        &mut self,
        el: &'ast ast::types::tuple::Element<Loc, Loc>,
    ) -> ast::types::tuple::Element<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_tuple_element_default(self, el)
    }

    fn tuple_labeled_element(
        &mut self,
        labeled: &'ast ast::types::tuple::LabeledElement<Loc, Type>,
    ) -> Result<(), E> {
        tuple_labeled_element_default(self, labeled)
    }

    fn map_tuple_labeled_element(
        &mut self,
        labeled: &'ast ast::types::tuple::LabeledElement<Loc, Loc>,
    ) -> ast::types::tuple::LabeledElement<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_tuple_labeled_element_default(self, labeled)
    }

    fn tuple_spread_element(
        &mut self,
        spread: &'ast ast::types::tuple::SpreadElement<Loc, Type>,
    ) -> Result<(), E> {
        tuple_spread_element_default(self, spread)
    }

    fn map_tuple_spread_element(
        &mut self,
        spread: &'ast ast::types::tuple::SpreadElement<Loc, Loc>,
    ) -> ast::types::tuple::SpreadElement<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_tuple_spread_element_default(self, spread)
    }

    fn array_type(&mut self, array: &'ast ast::types::Array<Loc, Type>) -> Result<(), E> {
        array_type_default(self, array)
    }

    fn map_array_type(
        &mut self,
        loc: &'ast Loc,
        array: &'ast ast::types::Array<Loc, Loc>,
    ) -> ast::types::Array<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_array_type_default(self, loc, array)
    }

    fn union_type(
        &mut self,
        loc: &'ast Type,
        t: &'ast ast::types::Union<Loc, Type>,
    ) -> Result<(), E> {
        union_type_default(self, loc, t)
    }

    fn map_union_type(
        &mut self,
        loc: &'ast Loc,
        t: &'ast ast::types::Union<Loc, Loc>,
    ) -> ast::types::Union<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_union_type_default(self, loc, t)
    }

    fn intersection_type(
        &mut self,
        loc: &'ast Type,
        t: &'ast ast::types::Intersection<Loc, Type>,
    ) -> Result<(), E> {
        intersection_type_default(self, loc, t)
    }

    fn map_intersection_type(
        &mut self,
        loc: &'ast Loc,
        t: &'ast ast::types::Intersection<Loc, Loc>,
    ) -> ast::types::Intersection<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_intersection_type_default(self, loc, t)
    }

    fn type_(&mut self, t: &'ast ast::types::Type<Loc, Type>) -> Result<(), E> {
        type_default(self, t)
    }

    fn map_type_(&mut self, t: &'ast ast::types::Type<Loc, Loc>) -> ast::types::Type<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_type_default(self, t)
    }

    fn type_annotation(&mut self, annot: &'ast ast::types::Annotation<Loc, Type>) -> Result<(), E> {
        type_annotation_default(self, annot)
    }

    fn map_type_annotation(
        &mut self,
        annot: &'ast ast::types::Annotation<Loc, Loc>,
    ) -> ast::types::Annotation<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_type_annotation_default(self, annot)
    }

    fn type_annotation_hint(
        &mut self,
        return_: &'ast ast::types::AnnotationOrHint<Loc, Type>,
    ) -> Result<(), E> {
        type_annotation_hint_default(self, return_)
    }

    fn map_type_annotation_hint(
        &mut self,
        hint: &'ast ast::types::AnnotationOrHint<Loc, Loc>,
    ) -> ast::types::AnnotationOrHint<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_type_annotation_hint_default(self, hint)
    }

    fn component_renders_annotation(
        &mut self,
        renders: &'ast ast::types::ComponentRendersAnnotation<Loc, Type>,
    ) -> Result<(), E> {
        component_renders_annotation_default(self, renders)
    }

    fn map_component_renders_annotation(
        &mut self,
        renders: &'ast ast::types::ComponentRendersAnnotation<Loc, Loc>,
    ) -> ast::types::ComponentRendersAnnotation<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_component_renders_annotation_default(self, renders)
    }

    fn function_declaration(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::function::Function<Loc, Type>,
    ) -> Result<(), E> {
        function_declaration_default(self, loc, stmt)
    }

    fn map_function_declaration(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::function::Function<Loc, Loc>,
    ) -> ast::function::Function<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_function_declaration_default(self, loc, stmt)
    }

    fn function_expression(
        &mut self,
        loc: &'ast Type,
        stmt: &'ast ast::function::Function<Loc, Type>,
    ) -> Result<(), E> {
        function_expression_default(self, loc, stmt)
    }

    fn map_function_expression(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::function::Function<Loc, Loc>,
    ) -> ast::function::Function<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_function_expression_default(self, loc, stmt)
    }

    fn function_expression_or_method(
        &mut self,
        loc: C,
        stmt: &'ast ast::function::Function<Loc, Type>,
    ) -> Result<(), E> {
        function_expression_or_method_default(self, loc, stmt)
    }

    fn map_function_expression_or_method(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::function::Function<Loc, Loc>,
    ) -> ast::function::Function<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_function_expression_or_method_default(self, loc, stmt)
    }

    fn function_(
        &mut self,
        loc: C,
        expr: &'ast ast::function::Function<Loc, Type>,
    ) -> Result<(), E> {
        function_default(self, loc, expr)
    }

    fn map_function_(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::function::Function<Loc, Loc>,
    ) -> ast::function::Function<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_function_default(self, loc, expr)
    }

    fn function_params(&mut self, params: &'ast ast::function::Params<Loc, Type>) -> Result<(), E> {
        function_params_default(self, params)
    }

    fn map_function_params(
        &mut self,
        params: &'ast ast::function::Params<Loc, Loc>,
    ) -> ast::function::Params<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_function_params_default(self, params)
    }

    fn function_this_param(
        &mut self,
        this_param: &'ast ast::function::ThisParam<Loc, Type>,
    ) -> Result<(), E> {
        function_this_param_default(self, this_param)
    }

    fn map_function_this_param(
        &mut self,
        this_param: &'ast ast::function::ThisParam<Loc, Loc>,
    ) -> ast::function::ThisParam<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_function_this_param_default(self, this_param)
    }

    fn function_param(&mut self, param: &'ast ast::function::Param<Loc, Type>) -> Result<(), E> {
        function_param_default(self, param)
    }

    fn map_function_param(
        &mut self,
        param: &'ast ast::function::Param<Loc, Loc>,
    ) -> ast::function::Param<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_function_param_default(self, param)
    }

    fn function_return_annotation(
        &mut self,
        return_: &'ast ast::function::ReturnAnnot<Loc, Type>,
    ) -> Result<(), E> {
        function_return_annotation_default(self, return_)
    }

    fn map_function_return_annotation(
        &mut self,
        return_: &'ast ast::function::ReturnAnnot<Loc, Loc>,
    ) -> ast::function::ReturnAnnot<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_function_return_annotation_default(self, return_)
    }

    fn function_body_any(&mut self, body: &'ast ast::function::Body<Loc, Type>) -> Result<(), E> {
        function_body_any_default(self, body)
    }

    fn map_function_body_any(
        &mut self,
        body: &'ast ast::function::Body<Loc, Loc>,
    ) -> ast::function::Body<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_function_body_any_default(self, body)
    }

    fn function_body(
        &mut self,
        body: &'ast (Loc, ast::statement::Block<Loc, Type>),
    ) -> Result<(), E> {
        function_body_default(self, body)
    }

    fn map_function_body(
        &mut self,
        body: &'ast (Loc, ast::statement::Block<Loc, Loc>),
    ) -> (Loc, ast::statement::Block<Loc, Loc>)
    where
        Loc: Dupe,
    {
        map_function_body_default(self, body)
    }

    fn body_expression(
        &mut self,
        expr: &'ast ast::expression::Expression<Loc, Type>,
    ) -> Result<(), E> {
        body_expression_default(self, expr)
    }

    fn map_body_expression(
        &mut self,
        expr: &'ast ast::expression::Expression<Loc, Loc>,
    ) -> ast::expression::Expression<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_body_expression_default(self, expr)
    }

    fn function_identifier(&mut self, ident: &'ast ast::Identifier<Loc, Type>) -> Result<(), E> {
        function_identifier_default(self, ident)
    }

    fn map_function_identifier(
        &mut self,
        ident: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_function_identifier_default(self, ident)
    }

    fn identifier(&mut self, id: &'ast ast::Identifier<Loc, Type>) -> Result<(), E> {
        identifier_default(self, id)
    }

    fn map_identifier(&mut self, id: &'ast ast::Identifier<Loc, Loc>) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_identifier_default(self, id)
    }

    fn untyped_identifier(&mut self, id: &'ast ast::Identifier<Loc, Loc>) -> Result<(), E> {
        untyped_identifier_default(self, id)
    }

    fn type_identifier(&mut self, id: &'ast ast::Identifier<Loc, Type>) -> Result<(), E> {
        type_identifier_default(self, id)
    }

    fn map_type_identifier(
        &mut self,
        id: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_type_identifier_default(self, id)
    }

    fn type_identifier_reference(&mut self, id: &'ast ast::Identifier<Loc, Type>) -> Result<(), E> {
        type_identifier_reference_default(self, id)
    }

    fn map_type_identifier_reference(
        &mut self,
        id: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_type_identifier_reference_default(self, id)
    }

    fn binding_type_identifier(&mut self, id: &'ast ast::Identifier<Loc, Type>) -> Result<(), E> {
        binding_type_identifier_default(self, id)
    }

    fn map_binding_type_identifier(
        &mut self,
        id: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_binding_type_identifier_default(self, id)
    }

    fn interface(
        &mut self,
        loc: &'ast Loc,
        interface: &'ast ast::statement::Interface<Loc, Type>,
    ) -> Result<(), E> {
        interface_default(self, loc, interface)
    }

    fn map_interface(
        &mut self,
        loc: &'ast Loc,
        interface: &'ast ast::statement::Interface<Loc, Loc>,
    ) -> ast::statement::Interface<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_interface_default(self, loc, interface)
    }

    fn interface_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::Interface<Loc, Type>,
    ) -> Result<(), E> {
        interface_declaration_default(self, loc, decl)
    }

    fn map_interface_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::Interface<Loc, Loc>,
    ) -> ast::statement::Interface<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_interface_declaration_default(self, loc, decl)
    }

    fn private_name(&mut self, id: &'ast ast::PrivateName<Loc>) -> Result<(), E> {
        private_name_default(self, id)
    }

    fn map_private_name(&mut self, id: &'ast ast::PrivateName<Loc>) -> ast::PrivateName<Loc>
    where
        Loc: Dupe,
    {
        map_private_name_default(self, id)
    }

    fn computed_key(&mut self, key: &'ast ast::ComputedKey<Loc, Type>) -> Result<(), E> {
        computed_key_default(self, key)
    }

    fn map_computed_key(
        &mut self,
        key: &'ast ast::ComputedKey<Loc, Loc>,
    ) -> ast::ComputedKey<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_computed_key_default(self, key)
    }

    fn import(&mut self, expr: &'ast ast::expression::Import<Loc, Type>) -> Result<(), E> {
        import_default(self, expr)
    }

    fn map_import(
        &mut self,
        expr: &'ast ast::expression::Import<Loc, Loc>,
    ) -> ast::expression::Import<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_import_default(self, expr)
    }

    fn if_consequent_statement(
        &mut self,
        has_else: bool,
        stmt: &'ast ast::statement::Statement<Loc, Type>,
    ) -> Result<(), E> {
        if_consequent_statement_default(self, has_else, stmt)
    }

    fn map_if_consequent_statement(
        &mut self,
        has_else: bool,
        stmt: &'ast ast::statement::Statement<Loc, Loc>,
    ) -> ast::statement::Statement<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_if_consequent_statement_default(self, has_else, stmt)
    }

    fn if_alternate_statement(
        &mut self,
        altern: &'ast ast::statement::if_::Alternate<Loc, Type>,
    ) -> Result<(), E> {
        if_alternate_statement_default(self, altern)
    }

    fn map_if_alternate_statement(
        &mut self,
        altern: &'ast ast::statement::if_::Alternate<Loc, Loc>,
    ) -> ast::statement::if_::Alternate<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_if_alternate_statement_default(self, altern)
    }

    fn if_statement(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::If<Loc, Type>,
    ) -> Result<(), E> {
        if_statement_default(self, loc, stmt)
    }

    fn map_if_statement(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::If<Loc, Loc>,
    ) -> ast::statement::If<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_if_statement_default(self, loc, stmt)
    }

    fn import_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::ImportDeclaration<Loc, Type>,
    ) -> Result<(), E> {
        import_declaration_default(self, loc, decl)
    }

    fn map_import_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::ImportDeclaration<Loc, Loc>,
    ) -> ast::statement::ImportDeclaration<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_import_declaration_default(self, loc, decl)
    }

    fn import_equals_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::ImportEqualsDeclaration<Loc, Type>,
    ) -> Result<(), E> {
        import_equals_declaration_default(self, loc, decl)
    }

    fn map_import_equals_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::ImportEqualsDeclaration<Loc, Loc>,
    ) -> ast::statement::ImportEqualsDeclaration<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_import_equals_declaration_default(self, loc, decl)
    }

    fn import_equals_module_reference(
        &mut self,
        module_ref: &'ast ast::statement::import_equals_declaration::ModuleReference<Loc, Type>,
    ) -> Result<(), E> {
        import_equals_module_reference_default(self, module_ref)
    }

    fn map_import_equals_module_reference(
        &mut self,
        module_ref: &'ast ast::statement::import_equals_declaration::ModuleReference<Loc, Loc>,
    ) -> ast::statement::import_equals_declaration::ModuleReference<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_import_equals_module_reference_default(self, module_ref)
    }

    fn import_source(
        &mut self,
        loc: &'ast Type,
        source: &'ast ast::StringLiteral<Loc>,
    ) -> Result<(), E> {
        import_source_default(self, loc, source)
    }

    fn map_import_source(
        &mut self,
        loc: &'ast Loc,
        source: &'ast ast::StringLiteral<Loc>,
    ) -> ast::StringLiteral<Loc>
    where
        Loc: Dupe,
    {
        map_import_source_default(self, loc, source)
    }

    fn import_attributes(
        &mut self,
        loc: &'ast Loc,
        attrs: &'ast [ast::statement::import_declaration::ImportAttribute<Loc, Type>],
    ) -> Result<(), E> {
        import_attributes_default(self, loc, attrs)
    }

    fn map_import_attributes(
        &mut self,
        loc: &'ast Loc,
        attrs: &'ast [ast::statement::import_declaration::ImportAttribute<Loc, Loc>],
    ) -> Vec<ast::statement::import_declaration::ImportAttribute<Loc, Loc>>
    where
        Loc: Dupe,
    {
        map_import_attributes_default(self, loc, attrs)
    }

    fn import_attribute(
        &mut self,
        attr: &'ast ast::statement::import_declaration::ImportAttribute<Loc, Type>,
    ) -> Result<(), E> {
        import_attribute_default(self, attr)
    }

    fn map_import_attribute(
        &mut self,
        attr: &'ast ast::statement::import_declaration::ImportAttribute<Loc, Loc>,
    ) -> ast::statement::import_declaration::ImportAttribute<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_import_attribute_default(self, attr)
    }

    fn import_attribute_key(
        &mut self,
        key: &'ast ast::statement::import_declaration::ImportAttributeKey<Loc, Type>,
    ) -> Result<(), E> {
        import_attribute_key_default(self, key)
    }

    fn map_import_attribute_key(
        &mut self,
        key: &'ast ast::statement::import_declaration::ImportAttributeKey<Loc, Loc>,
    ) -> ast::statement::import_declaration::ImportAttributeKey<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_import_attribute_key_default(self, key)
    }

    fn import_specifier(
        &mut self,
        import_kind: ast::statement::ImportKind,
        spec: &'ast ast::statement::import_declaration::Specifier<Loc, Type>,
    ) -> Result<(), E> {
        import_specifier_default(self, import_kind, spec)
    }

    fn map_import_specifier(
        &mut self,
        import_kind: ast::statement::ImportKind,
        spec: &'ast ast::statement::import_declaration::Specifier<Loc, Loc>,
    ) -> ast::statement::import_declaration::Specifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_import_specifier_default(self, import_kind, spec)
    }

    fn remote_identifier(&mut self, id: &'ast ast::Identifier<Loc, Type>) -> Result<(), E> {
        remote_identifier_default(self, id)
    }

    fn map_remote_identifier(
        &mut self,
        id: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_remote_identifier_default(self, id)
    }

    fn import_named_specifier(
        &mut self,
        import_kind: ast::statement::ImportKind,
        spec: &'ast ast::statement::import_declaration::NamedSpecifier<Loc, Type>,
    ) -> Result<(), E> {
        import_named_specifier_default(self, import_kind, spec)
    }

    fn map_import_named_specifier(
        &mut self,
        import_kind: ast::statement::ImportKind,
        spec: &'ast ast::statement::import_declaration::NamedSpecifier<Loc, Loc>,
    ) -> ast::statement::import_declaration::NamedSpecifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_import_named_specifier_default(self, import_kind, spec)
    }

    fn import_default_specifier(
        &mut self,
        import_kind: &'ast ast::statement::ImportKind,
        id: &'ast ast::Identifier<Loc, Type>,
    ) -> Result<(), E> {
        import_default_specifier_default(self, import_kind, id)
    }

    fn map_import_default_specifier(
        &mut self,
        import_kind: &'ast ast::statement::ImportKind,
        id: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_import_default_specifier_default(self, import_kind, id)
    }

    fn import_namespace_specifier(
        &mut self,
        import_kind: ast::statement::ImportKind,
        loc: &'ast Loc,
        id: &'ast ast::Identifier<Loc, Type>,
    ) -> Result<(), E> {
        import_namespace_specifier_default(self, import_kind, loc, id)
    }

    fn map_import_namespace_specifier(
        &mut self,
        import_kind: ast::statement::ImportKind,
        loc: &'ast Loc,
        id: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_import_namespace_specifier_default(self, import_kind, loc, id)
    }

    fn jsx_element(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::jsx::Element<Loc, Type>,
    ) -> Result<(), E> {
        jsx_element_default(self, loc, expr)
    }

    fn map_jsx_element(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::jsx::Element<Loc, Loc>,
    ) -> ast::jsx::Element<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_element_default(self, loc, expr)
    }

    fn jsx_fragment(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::jsx::Fragment<Loc, Type>,
    ) -> Result<(), E> {
        jsx_fragment_default(self, loc, expr)
    }

    fn map_jsx_fragment(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::jsx::Fragment<Loc, Loc>,
    ) -> ast::jsx::Fragment<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_fragment_default(self, loc, expr)
    }

    fn jsx_opening_element(&mut self, elem: &'ast ast::jsx::Opening<Loc, Type>) -> Result<(), E> {
        jsx_opening_element_default(self, elem)
    }

    fn map_jsx_opening_element(
        &mut self,
        elem: &'ast ast::jsx::Opening<Loc, Loc>,
    ) -> ast::jsx::Opening<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_opening_element_default(self, elem)
    }

    fn jsx_closing_element(&mut self, elem: &'ast ast::jsx::Closing<Loc, Type>) -> Result<(), E> {
        jsx_closing_element_default(self, elem)
    }

    fn map_jsx_closing_element(
        &mut self,
        elem: &'ast ast::jsx::Closing<Loc, Loc>,
    ) -> ast::jsx::Closing<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_closing_element_default(self, elem)
    }

    fn jsx_opening_attribute(
        &mut self,
        jsx_attr: &'ast ast::jsx::OpeningAttribute<Loc, Type>,
    ) -> Result<(), E> {
        jsx_opening_attribute_default(self, jsx_attr)
    }

    fn map_jsx_opening_attribute(
        &mut self,
        attr: &'ast ast::jsx::OpeningAttribute<Loc, Loc>,
    ) -> ast::jsx::OpeningAttribute<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_opening_attribute_default(self, attr)
    }

    fn jsx_spread_attribute(
        &mut self,
        attr: &'ast ast::jsx::SpreadAttribute<Loc, Type>,
    ) -> Result<(), E> {
        jsx_spread_attribute_default(self, attr)
    }

    fn map_jsx_spread_attribute(
        &mut self,
        attr: &'ast ast::jsx::SpreadAttribute<Loc, Loc>,
    ) -> ast::jsx::SpreadAttribute<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_spread_attribute_default(self, attr)
    }

    fn jsx_attribute(&mut self, attr: &'ast ast::jsx::Attribute<Loc, Type>) -> Result<(), E> {
        jsx_attribute_default(self, attr)
    }

    fn map_jsx_attribute(
        &mut self,
        attr: &'ast ast::jsx::Attribute<Loc, Loc>,
    ) -> ast::jsx::Attribute<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_attribute_default(self, attr)
    }

    fn jsx_attribute_name(
        &mut self,
        name: &'ast ast::jsx::attribute::Name<Loc, Type>,
    ) -> Result<(), E> {
        jsx_attribute_name_default(self, name)
    }

    fn map_jsx_attribute_name(
        &mut self,
        name: &'ast ast::jsx::attribute::Name<Loc, Loc>,
    ) -> ast::jsx::attribute::Name<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_attribute_name_default(self, name)
    }

    fn jsx_attribute_name_identifier(
        &mut self,
        ident: &'ast ast::jsx::Identifier<Loc, Type>,
    ) -> Result<(), E> {
        jsx_attribute_name_identifier_default(self, ident)
    }

    fn map_jsx_attribute_name_identifier(
        &mut self,
        ident: &'ast ast::jsx::Identifier<Loc, Loc>,
    ) -> ast::jsx::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_attribute_name_identifier_default(self, ident)
    }

    fn jsx_attribute_name_namespaced(
        &mut self,
        ns: &'ast ast::jsx::NamespacedName<Loc, Type>,
    ) -> Result<(), E> {
        jsx_attribute_name_namespaced_default(self, ns)
    }

    fn map_jsx_attribute_name_namespaced(
        &mut self,
        ns: &'ast ast::jsx::NamespacedName<Loc, Loc>,
    ) -> ast::jsx::NamespacedName<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_attribute_name_namespaced_default(self, ns)
    }

    fn jsx_attribute_value(
        &mut self,
        value: &'ast ast::jsx::attribute::Value<Loc, Type>,
    ) -> Result<(), E> {
        jsx_attribute_value_default(self, value)
    }

    fn map_jsx_attribute_value(
        &mut self,
        value: &'ast ast::jsx::attribute::Value<Loc, Loc>,
    ) -> ast::jsx::attribute::Value<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_attribute_value_default(self, value)
    }

    fn jsx_attribute_value_expression(
        &mut self,
        loc: &'ast Type,
        jsx_expr: &'ast ast::jsx::ExpressionContainer<Loc, Type>,
    ) -> Result<(), E> {
        jsx_attribute_value_expression_default(self, loc, jsx_expr)
    }

    fn map_jsx_attribute_value_expression(
        &mut self,
        loc: &'ast Loc,
        jsx_expr: &'ast ast::jsx::ExpressionContainer<Loc, Loc>,
    ) -> (Loc, ast::jsx::ExpressionContainer<Loc, Loc>)
    where
        Loc: Dupe,
    {
        map_jsx_attribute_value_expression_default(self, loc, jsx_expr)
    }

    fn jsx_attribute_value_literal(
        &mut self,
        loc: &'ast Type,
        lit: &'ast ast::StringLiteral<Loc>,
    ) -> Result<(), E> {
        jsx_attribute_value_literal_default(self, loc, lit)
    }

    fn map_jsx_attribute_value_literal(
        &mut self,
        loc: &'ast Loc,
        lit: &'ast ast::StringLiteral<Loc>,
    ) -> (Loc, ast::StringLiteral<Loc>)
    where
        Loc: Dupe,
    {
        map_jsx_attribute_value_literal_default(self, loc, lit)
    }

    fn jsx_child(&mut self, child: &'ast ast::jsx::Child<Loc, Type>) -> Result<(), E> {
        jsx_child_default(self, child)
    }

    fn map_jsx_child(&mut self, child: &'ast ast::jsx::Child<Loc, Loc>) -> ast::jsx::Child<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_child_default(self, child)
    }

    fn jsx_children(
        &mut self,
        loc: &'ast Loc,
        children: &'ast Vec<ast::jsx::Child<Loc, Type>>,
    ) -> Result<(), E> {
        jsx_children_default(self, loc, children)
    }

    fn map_jsx_children(
        &mut self,
        loc: &'ast Loc,
        children: &'ast [ast::jsx::Child<Loc, Loc>],
    ) -> Vec<ast::jsx::Child<Loc, Loc>>
    where
        Loc: Dupe,
    {
        map_jsx_children_default(self, loc, children)
    }

    fn jsx_expression(
        &mut self,
        jsx_expr: &'ast ast::jsx::ExpressionContainer<Loc, Type>,
    ) -> Result<(), E> {
        jsx_expression_default(self, jsx_expr)
    }

    fn map_jsx_expression(
        &mut self,
        expr: &'ast ast::jsx::ExpressionContainer<Loc, Loc>,
    ) -> ast::jsx::ExpressionContainer<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_expression_default(self, expr)
    }

    fn jsx_spread_child(&mut self, child: &'ast ast::jsx::SpreadChild<Loc, Type>) -> Result<(), E> {
        jsx_spread_child_default(self, child)
    }

    fn map_jsx_spread_child(
        &mut self,
        child: &'ast ast::jsx::SpreadChild<Loc, Loc>,
    ) -> ast::jsx::SpreadChild<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_spread_child_default(self, child)
    }

    fn jsx_element_name(&mut self, name: &'ast ast::jsx::Name<Loc, Type>) -> Result<(), E> {
        jsx_element_name_default(self, name)
    }

    fn map_jsx_element_name(
        &mut self,
        name: &'ast ast::jsx::Name<Loc, Loc>,
    ) -> ast::jsx::Name<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_element_name_default(self, name)
    }

    fn jsx_element_name_identifier(
        &mut self,
        ident: &'ast ast::jsx::Identifier<Loc, Type>,
    ) -> Result<(), E> {
        jsx_element_name_identifier_default(self, ident)
    }

    fn map_jsx_element_name_identifier(
        &mut self,
        ident: &'ast ast::jsx::Identifier<Loc, Loc>,
    ) -> ast::jsx::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_element_name_identifier_default(self, ident)
    }

    fn jsx_element_name_namespaced(
        &mut self,
        ns: &'ast ast::jsx::NamespacedName<Loc, Type>,
    ) -> Result<(), E> {
        jsx_element_name_namespaced_default(self, ns)
    }

    fn map_jsx_element_name_namespaced(
        &mut self,
        ns: &'ast ast::jsx::NamespacedName<Loc, Loc>,
    ) -> ast::jsx::NamespacedName<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_element_name_namespaced_default(self, ns)
    }

    fn jsx_element_name_member_expression(
        &mut self,
        member: &'ast ast::jsx::MemberExpression<Loc, Type>,
    ) -> Result<(), E> {
        jsx_element_name_member_expression_default(self, member)
    }

    fn map_jsx_element_name_member_expression(
        &mut self,
        member: &'ast ast::jsx::MemberExpression<Loc, Loc>,
    ) -> ast::jsx::MemberExpression<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_element_name_member_expression_default(self, member)
    }

    fn jsx_namespaced_name(
        &mut self,
        ns: &'ast ast::jsx::NamespacedName<Loc, Type>,
    ) -> Result<(), E> {
        jsx_namespaced_name_default(self, ns)
    }

    fn map_jsx_namespaced_name(
        &mut self,
        ns: &'ast ast::jsx::NamespacedName<Loc, Loc>,
    ) -> ast::jsx::NamespacedName<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_namespaced_name_default(self, ns)
    }

    fn jsx_member_expression(
        &mut self,
        member: &'ast ast::jsx::MemberExpression<Loc, Type>,
    ) -> Result<(), E> {
        jsx_member_expression_default(self, member)
    }

    fn map_jsx_member_expression(
        &mut self,
        member: &'ast ast::jsx::MemberExpression<Loc, Loc>,
    ) -> ast::jsx::MemberExpression<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_member_expression_default(self, member)
    }

    fn jsx_member_expression_object(
        &mut self,
        obj: &'ast ast::jsx::member_expression::Object<Loc, Type>,
    ) -> Result<(), E> {
        jsx_member_expression_object_default(self, obj)
    }

    fn map_jsx_member_expression_object(
        &mut self,
        obj: &'ast ast::jsx::member_expression::Object<Loc, Loc>,
    ) -> ast::jsx::member_expression::Object<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_member_expression_object_default(self, obj)
    }

    fn jsx_member_expression_identifier(
        &mut self,
        ident: &'ast ast::jsx::Identifier<Loc, Type>,
    ) -> Result<(), E> {
        jsx_member_expression_identifier_default(self, ident)
    }

    fn map_jsx_member_expression_identifier(
        &mut self,
        ident: &'ast ast::jsx::Identifier<Loc, Loc>,
    ) -> ast::jsx::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_member_expression_identifier_default(self, ident)
    }

    fn jsx_identifier(&mut self, ident: &'ast ast::jsx::Identifier<Loc, Type>) -> Result<(), E> {
        jsx_identifier_default(self, ident)
    }

    fn map_jsx_identifier(
        &mut self,
        ident: &'ast ast::jsx::Identifier<Loc, Loc>,
    ) -> ast::jsx::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_jsx_identifier_default(self, ident)
    }

    fn labeled_statement(
        &mut self,
        loc: &'ast Loc,
        label: &'ast ast::statement::Labeled<Loc, Type>,
    ) -> Result<(), E> {
        labeled_statement_default(self, loc, label)
    }

    fn map_labeled_statement(
        &mut self,
        loc: &'ast Loc,
        label: &'ast ast::statement::Labeled<Loc, Loc>,
    ) -> ast::statement::Labeled<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_labeled_statement_default(self, loc, label)
    }

    fn logical(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::Logical<Loc, Type>,
    ) -> Result<(), E> {
        logical_default(self, loc, expr)
    }

    fn map_logical(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::Logical<Loc, Loc>,
    ) -> ast::expression::Logical<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_logical_default(self, loc, expr)
    }

    fn match_<B>(
        &mut self,
        loc: C,
        m: &'ast ast::match_::Match<Loc, Type, B>,
        on_case_body: impl FnMut(&mut Self, &'ast B) -> Result<(), E>,
    ) -> Result<(), E> {
        match_default(self, loc, m, on_case_body)
    }

    fn map_match_<B, B2>(
        &mut self,
        loc: &'ast Loc,
        m: &'ast ast::match_::Match<Loc, Loc, B>,
        on_case_body: impl FnMut(&mut Self, &'ast B) -> B2,
    ) -> ast::match_::Match<Loc, Loc, B2>
    where
        Loc: Dupe,
    {
        map_match_default(self, loc, m, on_case_body)
    }

    fn match_case<B>(
        &mut self,
        case: &'ast ast::match_::Case<Loc, Type, B>,
        on_case_body: &mut impl FnMut(&mut Self, &'ast B) -> Result<(), E>,
    ) -> Result<(), E> {
        match_case_default(self, case, on_case_body)
    }

    fn map_match_case<B, B2>(
        &mut self,
        case: &'ast ast::match_::Case<Loc, Loc, B>,
        on_case_body: &mut impl FnMut(&mut Self, &'ast B) -> B2,
    ) -> ast::match_::Case<Loc, Loc, B2>
    where
        Loc: Dupe,
    {
        map_match_case_default(self, case, on_case_body)
    }

    fn match_expression(
        &mut self,
        loc: &'ast Type,
        m: &'ast ast::expression::MatchExpression<Loc, Type>,
    ) -> Result<(), E> {
        match_expression_default(self, loc, m)
    }

    fn map_match_expression(
        &mut self,
        loc: &'ast Loc,
        m: &'ast ast::expression::MatchExpression<Loc, Loc>,
    ) -> ast::expression::MatchExpression<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_expression_default(self, loc, m)
    }

    fn match_statement(
        &mut self,
        loc: &'ast Loc,
        m: &'ast ast::statement::MatchStatement<Loc, Type>,
    ) -> Result<(), E> {
        match_statement_default(self, loc, m)
    }

    fn map_match_statement(
        &mut self,
        loc: &'ast Loc,
        m: &'ast ast::statement::MatchStatement<Loc, Loc>,
    ) -> ast::statement::MatchStatement<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_statement_default(self, loc, m)
    }

    fn record_declaration(
        &mut self,
        loc: &'ast Loc,
        record: &'ast ast::statement::RecordDeclaration<Loc, Type>,
    ) -> Result<(), E> {
        record_declaration_default(self, loc, record)
    }

    fn map_record_declaration(
        &mut self,
        loc: &'ast Loc,
        record: &'ast ast::statement::RecordDeclaration<Loc, Loc>,
    ) -> ast::statement::RecordDeclaration<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_record_declaration_default(self, loc, record)
    }

    fn record_declaration_body(
        &mut self,
        body: &'ast ast::statement::record_declaration::Body<Loc, Type>,
    ) -> Result<(), E> {
        record_declaration_body_default(self, body)
    }

    fn map_record_declaration_body(
        &mut self,
        body: &'ast ast::statement::record_declaration::Body<Loc, Loc>,
    ) -> ast::statement::record_declaration::Body<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_record_declaration_body_default(self, body)
    }

    fn record_declaration_body_element(
        &mut self,
        element: &'ast ast::statement::record_declaration::BodyElement<Loc, Type>,
    ) -> Result<(), E> {
        record_declaration_body_element_default(self, element)
    }

    fn map_record_declaration_body_element(
        &mut self,
        element: &'ast ast::statement::record_declaration::BodyElement<Loc, Loc>,
    ) -> ast::statement::record_declaration::BodyElement<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_record_declaration_body_element_default(self, element)
    }

    fn record_declaration_property(
        &mut self,
        prop: &'ast ast::statement::record_declaration::Property<Loc, Type>,
    ) -> Result<(), E> {
        record_declaration_property_default(self, prop)
    }

    fn map_record_declaration_property(
        &mut self,
        prop: &'ast ast::statement::record_declaration::Property<Loc, Loc>,
    ) -> ast::statement::record_declaration::Property<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_record_declaration_property_default(self, prop)
    }

    fn record_declaration_static_property(
        &mut self,
        prop: &'ast ast::statement::record_declaration::StaticProperty<Loc, Type>,
    ) -> Result<(), E> {
        record_declaration_static_property_default(self, prop)
    }

    fn map_record_declaration_static_property(
        &mut self,
        prop: &'ast ast::statement::record_declaration::StaticProperty<Loc, Loc>,
    ) -> ast::statement::record_declaration::StaticProperty<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_record_declaration_static_property_default(self, prop)
    }

    fn match_pattern(
        &mut self,
        pattern: &'ast ast::match_pattern::MatchPattern<Loc, Type>,
    ) -> Result<(), E> {
        match_pattern_default(self, pattern)
    }

    fn map_match_pattern(
        &mut self,
        pattern: &'ast ast::match_pattern::MatchPattern<Loc, Loc>,
    ) -> ast::match_pattern::MatchPattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_pattern_default(self, pattern)
    }

    fn match_unary_pattern(
        &mut self,
        pattern: &'ast ast::match_pattern::UnaryPattern<Loc>,
    ) -> Result<(), E> {
        match_unary_pattern_default(self, pattern)
    }

    fn map_match_unary_pattern(
        &mut self,
        pattern: &'ast ast::match_pattern::UnaryPattern<Loc>,
    ) -> ast::match_pattern::UnaryPattern<Loc>
    where
        Loc: Dupe,
    {
        map_match_unary_pattern_default(self, pattern)
    }

    fn match_unary_pattern_argument(
        &mut self,
        loc: &'ast Loc,
        arg: &'ast ast::match_pattern::unary_pattern::Argument<Loc>,
    ) -> Result<(), E> {
        match_unary_pattern_argument_default(self, loc, arg)
    }

    fn map_match_unary_pattern_argument(
        &mut self,
        loc: &'ast Loc,
        arg: &'ast ast::match_pattern::unary_pattern::Argument<Loc>,
    ) -> ast::match_pattern::unary_pattern::Argument<Loc>
    where
        Loc: Dupe,
    {
        map_match_unary_pattern_argument_default(self, loc, arg)
    }

    fn match_member_pattern(
        &mut self,
        pattern: &'ast ast::match_pattern::MemberPattern<Loc, Type>,
    ) -> Result<(), E> {
        match_member_pattern_default(self, pattern)
    }

    fn map_match_member_pattern(
        &mut self,
        pattern: &'ast ast::match_pattern::MemberPattern<Loc, Loc>,
    ) -> ast::match_pattern::MemberPattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_member_pattern_default(self, pattern)
    }

    fn match_member_pattern_base(
        &mut self,
        base: &'ast ast::match_pattern::member_pattern::Base<Loc, Type>,
    ) -> Result<(), E> {
        match_member_pattern_base_default(self, base)
    }

    fn map_match_member_pattern_base(
        &mut self,
        base: &'ast ast::match_pattern::member_pattern::Base<Loc, Loc>,
    ) -> ast::match_pattern::member_pattern::Base<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_member_pattern_base_default(self, base)
    }

    fn match_member_pattern_property(
        &mut self,
        prop: &'ast ast::match_pattern::member_pattern::Property<Loc, Type>,
    ) -> Result<(), E> {
        match_member_pattern_property_default(self, prop)
    }

    fn map_match_member_pattern_property(
        &mut self,
        prop: &'ast ast::match_pattern::member_pattern::Property<Loc, Loc>,
    ) -> ast::match_pattern::member_pattern::Property<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_member_pattern_property_default(self, prop)
    }

    fn match_binding_pattern(
        &mut self,
        loc: &'ast Loc,
        binding: &'ast ast::match_pattern::BindingPattern<Loc, Type>,
    ) -> Result<(), E> {
        match_binding_pattern_default(self, loc, binding)
    }

    fn map_match_binding_pattern(
        &mut self,
        loc: &'ast Loc,
        binding: &'ast ast::match_pattern::BindingPattern<Loc, Loc>,
    ) -> ast::match_pattern::BindingPattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_binding_pattern_default(self, loc, binding)
    }

    fn match_object_pattern(
        &mut self,
        loc: &'ast Loc,
        pattern: &'ast ast::match_pattern::ObjectPattern<Loc, Type>,
    ) -> Result<(), E> {
        match_object_pattern_default(self, loc, pattern)
    }

    fn map_match_object_pattern(
        &mut self,
        loc: &'ast Loc,
        pattern: &'ast ast::match_pattern::ObjectPattern<Loc, Loc>,
    ) -> ast::match_pattern::ObjectPattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_object_pattern_default(self, loc, pattern)
    }

    fn match_object_pattern_property(
        &mut self,
        prop: &'ast ast::match_pattern::object_pattern::Property<Loc, Type>,
    ) -> Result<(), E> {
        match_object_pattern_property_default(self, prop)
    }

    fn map_match_object_pattern_property(
        &mut self,
        prop: &'ast ast::match_pattern::object_pattern::Property<Loc, Loc>,
    ) -> ast::match_pattern::object_pattern::Property<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_object_pattern_property_default(self, prop)
    }

    fn match_object_pattern_property_key(
        &mut self,
        key: &'ast ast::match_pattern::object_pattern::Key<Loc, Type>,
    ) -> Result<(), E> {
        match_object_pattern_property_key_default(self, key)
    }

    fn map_match_object_pattern_property_key(
        &mut self,
        key: &'ast ast::match_pattern::object_pattern::Key<Loc, Loc>,
    ) -> ast::match_pattern::object_pattern::Key<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_object_pattern_property_key_default(self, key)
    }

    fn match_array_pattern(
        &mut self,
        pattern: &'ast ast::match_pattern::ArrayPattern<Loc, Type>,
    ) -> Result<(), E> {
        match_array_pattern_default(self, pattern)
    }

    fn map_match_array_pattern(
        &mut self,
        pattern: &'ast ast::match_pattern::ArrayPattern<Loc, Loc>,
    ) -> ast::match_pattern::ArrayPattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_array_pattern_default(self, pattern)
    }

    fn match_pattern_array_element(
        &mut self,
        elem: &'ast ast::match_pattern::array_pattern::Element<Loc, Type>,
    ) -> Result<(), E> {
        match_pattern_array_element_default(self, elem)
    }

    fn map_match_pattern_array_element(
        &mut self,
        elem: &'ast ast::match_pattern::array_pattern::Element<Loc, Loc>,
    ) -> ast::match_pattern::array_pattern::Element<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_pattern_array_element_default(self, elem)
    }

    fn match_instance_pattern(
        &mut self,
        pattern: &'ast ast::match_pattern::InstancePattern<Loc, Type>,
    ) -> Result<(), E> {
        match_instance_pattern_default(self, pattern)
    }

    fn map_match_instance_pattern(
        &mut self,
        pattern: &'ast ast::match_pattern::InstancePattern<Loc, Loc>,
    ) -> ast::match_pattern::InstancePattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_instance_pattern_default(self, pattern)
    }

    fn match_instance_pattern_constructor(
        &mut self,
        constructor: &'ast ast::match_pattern::InstancePatternConstructor<Loc, Type>,
    ) -> Result<(), E> {
        match_instance_pattern_constructor_default(self, constructor)
    }

    fn map_match_instance_pattern_constructor(
        &mut self,
        constructor: &'ast ast::match_pattern::InstancePatternConstructor<Loc, Loc>,
    ) -> ast::match_pattern::InstancePatternConstructor<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_instance_pattern_constructor_default(self, constructor)
    }

    fn match_rest_pattern(
        &mut self,
        rest: &'ast ast::match_pattern::RestPattern<Loc, Type>,
    ) -> Result<(), E> {
        match_rest_pattern_default(self, rest)
    }

    fn map_match_rest_pattern(
        &mut self,
        rest: &'ast ast::match_pattern::RestPattern<Loc, Loc>,
    ) -> ast::match_pattern::RestPattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_rest_pattern_default(self, rest)
    }

    fn match_or_pattern(
        &mut self,
        pattern: &'ast ast::match_pattern::OrPattern<Loc, Type>,
    ) -> Result<(), E> {
        match_or_pattern_default(self, pattern)
    }

    fn map_match_or_pattern(
        &mut self,
        pattern: &'ast ast::match_pattern::OrPattern<Loc, Loc>,
    ) -> ast::match_pattern::OrPattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_or_pattern_default(self, pattern)
    }

    fn match_as_pattern(
        &mut self,
        pattern: &'ast ast::match_pattern::AsPattern<Loc, Type>,
    ) -> Result<(), E> {
        match_as_pattern_default(self, pattern)
    }

    fn map_match_as_pattern(
        &mut self,
        pattern: &'ast ast::match_pattern::AsPattern<Loc, Loc>,
    ) -> ast::match_pattern::AsPattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_as_pattern_default(self, pattern)
    }

    fn match_as_pattern_target(
        &mut self,
        target: &'ast ast::match_pattern::as_pattern::Target<Loc, Type>,
    ) -> Result<(), E> {
        match_as_pattern_target_default(self, target)
    }

    fn map_match_as_pattern_target(
        &mut self,
        target: &'ast ast::match_pattern::as_pattern::Target<Loc, Loc>,
    ) -> ast::match_pattern::as_pattern::Target<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_match_as_pattern_target_default(self, target)
    }

    fn match_wildcard_pattern(
        &mut self,
        wildcard: &'ast ast::match_pattern::WildcardPattern<Loc>,
    ) -> Result<(), E> {
        match_wildcard_pattern_default(self, wildcard)
    }

    fn map_match_wildcard_pattern(
        &mut self,
        wildcard: &'ast ast::match_pattern::WildcardPattern<Loc>,
    ) -> ast::match_pattern::WildcardPattern<Loc>
    where
        Loc: Dupe,
    {
        map_match_wildcard_pattern_default(self, wildcard)
    }

    fn member(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::Member<Loc, Type>,
    ) -> Result<(), E> {
        member_default(self, loc, expr)
    }

    fn map_member(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::Member<Loc, Loc>,
    ) -> ast::expression::Member<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_member_default(self, loc, expr)
    }

    fn optional_member(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::OptionalMember<Loc, Type>,
    ) -> Result<(), E> {
        optional_member_default(self, loc, expr)
    }

    fn map_optional_member(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::OptionalMember<Loc, Loc>,
    ) -> ast::expression::OptionalMember<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_optional_member_default(self, loc, expr)
    }

    fn record(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::Record<Loc, Type>,
    ) -> Result<(), E> {
        record_default(self, loc, expr)
    }

    fn map_record(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::Record<Loc, Loc>,
    ) -> ast::expression::Record<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_record_default(self, loc, expr)
    }

    fn member_property(
        &mut self,
        prop: &'ast ast::expression::member::Property<Loc, Type>,
    ) -> Result<(), E> {
        member_property_default(self, prop)
    }

    fn map_member_property(
        &mut self,
        prop: &'ast ast::expression::member::Property<Loc, Loc>,
    ) -> ast::expression::member::Property<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_member_property_default(self, prop)
    }

    fn member_property_identifier(
        &mut self,
        id: &'ast ast::Identifier<Loc, Type>,
    ) -> Result<(), E> {
        member_property_identifier_default(self, id)
    }

    fn map_member_property_identifier(
        &mut self,
        id: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_member_property_identifier_default(self, id)
    }

    fn member_private_name(&mut self, id: &'ast ast::PrivateName<Loc>) -> Result<(), E> {
        member_private_name_default(self, id)
    }

    fn map_member_private_name(&mut self, id: &'ast ast::PrivateName<Loc>) -> ast::PrivateName<Loc>
    where
        Loc: Dupe,
    {
        map_member_private_name_default(self, id)
    }

    fn member_property_expression(
        &mut self,
        expr: &'ast ast::expression::Expression<Loc, Type>,
    ) -> Result<(), E> {
        member_property_expression_default(self, expr)
    }

    fn map_member_property_expression(
        &mut self,
        expr: &'ast ast::expression::Expression<Loc, Loc>,
    ) -> ast::expression::Expression<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_member_property_expression_default(self, expr)
    }

    fn meta_property(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::MetaProperty<Loc>,
    ) -> Result<(), E> {
        meta_property_default(self, loc, expr)
    }

    fn map_meta_property(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::MetaProperty<Loc>,
    ) -> ast::expression::MetaProperty<Loc>
    where
        Loc: Dupe,
    {
        map_meta_property_default(self, loc, expr)
    }

    fn new(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::New<Loc, Type>,
    ) -> Result<(), E> {
        new_default(self, loc, expr)
    }

    fn map_new(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::New<Loc, Loc>,
    ) -> ast::expression::New<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_new_default(self, loc, expr)
    }

    fn object(&mut self, loc: C, expr: &'ast ast::expression::Object<Loc, Type>) -> Result<(), E> {
        object_default(self, loc, expr)
    }

    fn map_object(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::Object<Loc, Loc>,
    ) -> ast::expression::Object<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_object_default(self, loc, expr)
    }

    fn object_property(
        &mut self,
        prop: &'ast ast::expression::object::NormalProperty<Loc, Type>,
    ) -> Result<(), E> {
        object_property_default(self, prop)
    }

    fn map_object_property(
        &mut self,
        prop: &'ast ast::expression::object::Property<Loc, Loc>,
    ) -> ast::expression::object::Property<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_object_property_default(self, prop)
    }

    fn object_key(&mut self, key: &'ast ast::expression::object::Key<Loc, Type>) -> Result<(), E> {
        object_key_default(self, key)
    }

    fn map_object_key(
        &mut self,
        key: &'ast ast::expression::object::Key<Loc, Loc>,
    ) -> ast::expression::object::Key<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_object_key_default(self, key)
    }

    fn object_key_string_literal(
        &mut self,
        literal: &'ast ast::StringLiteral<Loc>,
    ) -> Result<(), E> {
        object_key_string_literal_default(self, literal)
    }

    fn map_object_key_string_literal(
        &mut self,
        literal: &'ast ast::StringLiteral<Loc>,
    ) -> ast::StringLiteral<Loc>
    where
        Loc: Dupe,
    {
        map_object_key_string_literal_default(self, literal)
    }

    fn object_key_number_literal(
        &mut self,
        literal: &'ast ast::NumberLiteral<Loc>,
    ) -> Result<(), E> {
        object_key_number_literal_default(self, literal)
    }

    fn map_object_key_number_literal(
        &mut self,
        literal: &'ast ast::NumberLiteral<Loc>,
    ) -> ast::NumberLiteral<Loc>
    where
        Loc: Dupe,
    {
        map_object_key_number_literal_default(self, literal)
    }

    fn object_key_bigint_literal(
        &mut self,
        literal: &'ast ast::BigIntLiteral<Loc>,
    ) -> Result<(), E> {
        object_key_bigint_literal_default(self, literal)
    }

    fn map_object_key_bigint_literal(
        &mut self,
        literal: &'ast ast::BigIntLiteral<Loc>,
    ) -> ast::BigIntLiteral<Loc>
    where
        Loc: Dupe,
    {
        map_object_key_bigint_literal_default(self, literal)
    }

    fn object_key_identifier(&mut self, id: &'ast ast::Identifier<Loc, Type>) -> Result<(), E> {
        object_key_identifier_default(self, id)
    }

    fn map_object_key_identifier(
        &mut self,
        id: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_object_key_identifier_default(self, id)
    }

    fn object_key_computed(&mut self, key: &'ast ast::ComputedKey<Loc, Type>) -> Result<(), E> {
        object_key_computed_default(self, key)
    }

    fn map_object_key_computed(
        &mut self,
        key: &'ast ast::ComputedKey<Loc, Loc>,
    ) -> ast::ComputedKey<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_object_key_computed_default(self, key)
    }

    fn opaque_type(
        &mut self,
        loc: &'ast Loc,
        otype: &'ast ast::statement::OpaqueType<Loc, Type>,
    ) -> Result<(), E> {
        opaque_type_default(self, loc, otype)
    }

    fn map_opaque_type(
        &mut self,
        loc: &'ast Loc,
        otype: &'ast ast::statement::OpaqueType<Loc, Loc>,
    ) -> ast::statement::OpaqueType<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_opaque_type_default(self, loc, otype)
    }

    fn function_param_pattern(
        &mut self,
        pattern: &'ast ast::pattern::Pattern<Loc, Type>,
    ) -> Result<(), E> {
        function_param_pattern_default(self, pattern)
    }

    fn map_function_param_pattern(
        &mut self,
        pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
    ) -> ast::pattern::Pattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_function_param_pattern_default(self, pattern)
    }

    fn variable_declarator_pattern(
        &mut self,
        kind: ast::VariableKind,
        pattern: &'ast ast::pattern::Pattern<Loc, Type>,
    ) -> Result<(), E> {
        variable_declarator_pattern_default(self, kind, pattern)
    }

    fn map_variable_declarator_pattern(
        &mut self,
        kind: ast::VariableKind,
        pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
    ) -> ast::pattern::Pattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_variable_declarator_pattern_default(self, kind, pattern)
    }

    fn catch_clause_pattern(
        &mut self,
        pattern: &'ast ast::pattern::Pattern<Loc, Type>,
    ) -> Result<(), E> {
        catch_clause_pattern_default(self, pattern)
    }

    fn map_catch_clause_pattern(
        &mut self,
        pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
    ) -> ast::pattern::Pattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_catch_clause_pattern_default(self, pattern)
    }

    fn for_in_assignment_pattern(
        &mut self,
        expr: &'ast ast::pattern::Pattern<Loc, Type>,
    ) -> Result<(), E> {
        for_in_assignment_pattern_default(self, expr)
    }

    fn map_for_in_assignment_pattern(
        &mut self,
        expr: &'ast ast::pattern::Pattern<Loc, Loc>,
    ) -> ast::pattern::Pattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_for_in_assignment_pattern_default(self, expr)
    }

    fn for_of_assignment_pattern(
        &mut self,
        expr: &'ast ast::pattern::Pattern<Loc, Type>,
    ) -> Result<(), E> {
        for_of_assignment_pattern_default(self, expr)
    }

    fn map_for_of_assignment_pattern(
        &mut self,
        expr: &'ast ast::pattern::Pattern<Loc, Loc>,
    ) -> ast::pattern::Pattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_for_of_assignment_pattern_default(self, expr)
    }

    fn binding_pattern(
        &mut self,
        kind: ast::VariableKind,
        pattern: &'ast ast::pattern::Pattern<Loc, Type>,
    ) -> Result<(), E> {
        binding_pattern_default(self, kind, pattern)
    }

    fn map_binding_pattern(
        &mut self,
        kind: ast::VariableKind,
        pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
    ) -> ast::pattern::Pattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_binding_pattern_default(self, kind, pattern)
    }

    fn assignment_pattern(
        &mut self,
        pattern: &'ast ast::pattern::Pattern<Loc, Type>,
    ) -> Result<(), E> {
        assignment_pattern_default(self, pattern)
    }

    fn map_assignment_pattern(
        &mut self,
        pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
    ) -> ast::pattern::Pattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_assignment_pattern_default(self, pattern)
    }

    /// NOTE: Patterns are highly overloaded. A pattern can be a binding pattern,
    /// which has a kind (Var/Let/Const, with Var being the default for all pre-ES5
    /// bindings), or an assignment pattern, which has no kind. Subterms that are
    /// patterns inherit the kind (or lack thereof).
    fn pattern(
        &mut self,
        kind: Option<ast::VariableKind>,
        pattern: &'ast ast::pattern::Pattern<Loc, Type>,
    ) -> Result<(), E> {
        pattern_default(self, kind, pattern)
    }

    fn map_pattern(
        &mut self,
        kind: Option<ast::VariableKind>,
        pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
    ) -> ast::pattern::Pattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_pattern_default(self, kind, pattern)
    }

    fn pattern_identifier(
        &mut self,
        kind: Option<ast::VariableKind>,
        ident: &'ast ast::Identifier<Loc, Type>,
    ) -> Result<(), E> {
        pattern_identifier_default(self, kind, ident)
    }

    fn map_pattern_identifier(
        &mut self,
        kind: Option<ast::VariableKind>,
        ident: &'ast ast::pattern::Identifier<Loc, Loc>,
    ) -> ast::pattern::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_pattern_identifier_default(self, kind, ident)
    }

    fn pattern_string_literal(
        &mut self,
        loc: &'ast Loc,
        lit: &'ast ast::StringLiteral<Loc>,
    ) -> Result<(), E> {
        pattern_string_literal_default(self, loc, lit)
    }

    fn map_pattern_string_literal(
        &mut self,
        loc: &'ast Loc,
        lit: &'ast ast::StringLiteral<Loc>,
    ) -> (Loc, ast::StringLiteral<Loc>)
    where
        Loc: Dupe,
    {
        map_pattern_string_literal_default(self, loc, lit)
    }

    fn pattern_number_literal(
        &mut self,
        loc: &'ast Loc,
        lit: &'ast ast::NumberLiteral<Loc>,
    ) -> Result<(), E> {
        pattern_number_literal_default(self, loc, lit)
    }

    fn map_pattern_number_literal(
        &mut self,
        loc: &'ast Loc,
        lit: &'ast ast::NumberLiteral<Loc>,
    ) -> (Loc, ast::NumberLiteral<Loc>)
    where
        Loc: Dupe,
    {
        map_pattern_number_literal_default(self, loc, lit)
    }

    fn pattern_bigint_literal(
        &mut self,
        loc: &'ast Loc,
        lit: &'ast ast::BigIntLiteral<Loc>,
    ) -> Result<(), E> {
        pattern_bigint_literal_default(self, loc, lit)
    }

    fn map_pattern_bigint_literal(
        &mut self,
        loc: &'ast Loc,
        lit: &'ast ast::BigIntLiteral<Loc>,
    ) -> (Loc, ast::BigIntLiteral<Loc>)
    where
        Loc: Dupe,
    {
        map_pattern_bigint_literal_default(self, loc, lit)
    }

    fn pattern_object_p(
        &mut self,
        kind: Option<ast::VariableKind>,
        loc: &'ast Type,
        prop: &'ast ast::pattern::object::Property<Loc, Type>,
    ) -> Result<(), E> {
        pattern_object_p_default(self, kind, loc, prop)
    }

    fn map_pattern_object_p(
        &mut self,
        kind: Option<ast::VariableKind>,
        loc: &'ast Loc,
        prop: &'ast ast::pattern::object::Property<Loc, Loc>,
    ) -> ast::pattern::object::Property<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_pattern_object_p_default(self, kind, loc, prop)
    }

    fn pattern_object_property(
        &mut self,
        kind: Option<ast::VariableKind>,
        prop: &'ast ast::pattern::object::NormalProperty<Loc, Type>,
    ) -> Result<(), E> {
        pattern_object_property_default(self, kind, prop)
    }

    fn map_pattern_object_property(
        &mut self,
        kind: Option<ast::VariableKind>,
        prop: &'ast ast::pattern::object::Property<Loc, Loc>,
    ) -> ast::pattern::object::Property<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_pattern_object_property_default(self, kind, prop)
    }

    fn pattern_object_property_key(
        &mut self,
        kind: Option<ast::VariableKind>,
        key: &'ast ast::pattern::object::Key<Loc, Type>,
    ) -> Result<(), E> {
        pattern_object_property_key_default(self, kind, key)
    }

    fn map_pattern_object_property_key(
        &mut self,
        kind: Option<ast::VariableKind>,
        key: &'ast ast::pattern::object::Key<Loc, Loc>,
    ) -> ast::pattern::object::Key<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_pattern_object_property_key_default(self, kind, key)
    }

    fn pattern_object_property_string_literal_key(
        &mut self,
        kind: Option<ast::VariableKind>,
        literal: &'ast (Loc, ast::StringLiteral<Loc>),
    ) -> Result<(), E> {
        pattern_object_property_string_literal_key_default(self, kind, literal)
    }

    fn map_pattern_object_property_string_literal_key(
        &mut self,
        kind: Option<ast::VariableKind>,
        literal: &'ast (Loc, ast::StringLiteral<Loc>),
    ) -> (Loc, ast::StringLiteral<Loc>)
    where
        Loc: Dupe,
    {
        map_pattern_object_property_string_literal_key_default(self, kind, literal)
    }

    fn pattern_object_property_number_literal_key(
        &mut self,
        kind: Option<ast::VariableKind>,
        literal: &'ast (Loc, ast::NumberLiteral<Loc>),
    ) -> Result<(), E> {
        pattern_object_property_number_literal_key_default(self, kind, literal)
    }

    fn map_pattern_object_property_number_literal_key(
        &mut self,
        kind: Option<ast::VariableKind>,
        literal: &'ast (Loc, ast::NumberLiteral<Loc>),
    ) -> (Loc, ast::NumberLiteral<Loc>)
    where
        Loc: Dupe,
    {
        map_pattern_object_property_number_literal_key_default(self, kind, literal)
    }

    fn pattern_object_property_bigint_literal_key(
        &mut self,
        kind: Option<ast::VariableKind>,
        literal: &'ast (Loc, ast::BigIntLiteral<Loc>),
    ) -> Result<(), E> {
        pattern_object_property_bigint_literal_key_default(self, kind, literal)
    }

    fn map_pattern_object_property_bigint_literal_key(
        &mut self,
        kind: Option<ast::VariableKind>,
        literal: &'ast (Loc, ast::BigIntLiteral<Loc>),
    ) -> (Loc, ast::BigIntLiteral<Loc>)
    where
        Loc: Dupe,
    {
        map_pattern_object_property_bigint_literal_key_default(self, kind, literal)
    }

    fn pattern_object_property_identifier_key(
        &mut self,
        kind: Option<ast::VariableKind>,
        id: &'ast ast::Identifier<Loc, Type>,
    ) -> Result<(), E> {
        pattern_object_property_identifier_key_default(self, kind, id)
    }

    fn map_pattern_object_property_identifier_key(
        &mut self,
        kind: Option<ast::VariableKind>,
        id: &'ast ast::Identifier<Loc, Loc>,
    ) -> ast::Identifier<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_pattern_object_property_identifier_key_default(self, kind, id)
    }

    fn pattern_object_property_computed_key(
        &mut self,
        kind: Option<ast::VariableKind>,
        key: &'ast ast::ComputedKey<Loc, Type>,
    ) -> Result<(), E> {
        pattern_object_property_computed_key_default(self, kind, key)
    }

    fn map_pattern_object_property_computed_key(
        &mut self,
        kind: Option<ast::VariableKind>,
        key: &'ast ast::ComputedKey<Loc, Loc>,
    ) -> ast::ComputedKey<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_pattern_object_property_computed_key_default(self, kind, key)
    }

    fn pattern_object_rest_property(
        &mut self,
        kind: Option<ast::VariableKind>,
        rest_elem: &'ast ast::pattern::RestElement<Loc, Type>,
    ) -> Result<(), E> {
        pattern_object_rest_property_default(self, kind, rest_elem)
    }

    fn map_pattern_object_rest_property(
        &mut self,
        kind: Option<ast::VariableKind>,
        rest_elem: &'ast ast::pattern::RestElement<Loc, Loc>,
    ) -> ast::pattern::RestElement<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_pattern_object_rest_property_default(self, kind, rest_elem)
    }

    fn pattern_object_property_pattern(
        &mut self,
        kind: Option<ast::VariableKind>,
        pattern: &'ast ast::pattern::Pattern<Loc, Type>,
    ) -> Result<(), E> {
        pattern_object_property_pattern_default(self, kind, pattern)
    }

    fn map_pattern_object_property_pattern(
        &mut self,
        kind: Option<ast::VariableKind>,
        pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
    ) -> ast::pattern::Pattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_pattern_object_property_pattern_default(self, kind, pattern)
    }

    fn pattern_object_rest_property_pattern(
        &mut self,
        kind: Option<ast::VariableKind>,
        pattern: &'ast ast::pattern::Pattern<Loc, Type>,
    ) -> Result<(), E> {
        pattern_object_rest_property_pattern_default(self, kind, pattern)
    }

    fn map_pattern_object_rest_property_pattern(
        &mut self,
        kind: Option<ast::VariableKind>,
        pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
    ) -> ast::pattern::Pattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_pattern_object_rest_property_pattern_default(self, kind, pattern)
    }

    fn pattern_array_e(
        &mut self,
        kind: Option<ast::VariableKind>,
        loc: &'ast Type,
        elem: &'ast ast::pattern::array::Element<Loc, Type>,
    ) -> Result<(), E> {
        pattern_array_e_default(self, kind, loc, elem)
    }

    fn map_pattern_array_e(
        &mut self,
        kind: Option<ast::VariableKind>,
        loc: &'ast Loc,
        elem: &'ast ast::pattern::array::Element<Loc, Loc>,
    ) -> ast::pattern::array::Element<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_pattern_array_e_default(self, kind, loc, elem)
    }

    fn pattern_array_element(
        &mut self,
        kind: Option<ast::VariableKind>,
        elem: &'ast ast::pattern::array::NormalElement<Loc, Type>,
    ) -> Result<(), E> {
        pattern_array_element_default(self, kind, elem)
    }

    fn map_pattern_array_element(
        &mut self,
        kind: Option<ast::VariableKind>,
        elem: &'ast ast::pattern::array::Element<Loc, Loc>,
    ) -> ast::pattern::array::Element<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_pattern_array_element_default(self, kind, elem)
    }

    fn pattern_array_element_pattern(
        &mut self,
        kind: Option<ast::VariableKind>,
        pattern: &'ast ast::pattern::Pattern<Loc, Type>,
    ) -> Result<(), E> {
        pattern_array_element_pattern_default(self, kind, pattern)
    }

    fn map_pattern_array_element_pattern(
        &mut self,
        kind: Option<ast::VariableKind>,
        pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
    ) -> ast::pattern::Pattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_pattern_array_element_pattern_default(self, kind, pattern)
    }

    fn pattern_array_rest_element(
        &mut self,
        kind: Option<ast::VariableKind>,
        rest_elem: &'ast ast::pattern::RestElement<Loc, Type>,
    ) -> Result<(), E> {
        pattern_array_rest_element_default(self, kind, rest_elem)
    }

    fn map_pattern_array_rest_element(
        &mut self,
        kind: Option<ast::VariableKind>,
        rest_elem: &'ast ast::pattern::RestElement<Loc, Loc>,
    ) -> ast::pattern::RestElement<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_pattern_array_rest_element_default(self, kind, rest_elem)
    }

    fn pattern_array_rest_element_pattern(
        &mut self,
        kind: Option<ast::VariableKind>,
        pattern: &'ast ast::pattern::Pattern<Loc, Type>,
    ) -> Result<(), E> {
        pattern_array_rest_element_pattern_default(self, kind, pattern)
    }

    fn map_pattern_array_rest_element_pattern(
        &mut self,
        kind: Option<ast::VariableKind>,
        pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
    ) -> ast::pattern::Pattern<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_pattern_array_rest_element_pattern_default(self, kind, pattern)
    }

    fn pattern_expression(
        &mut self,
        expr: &'ast ast::expression::Expression<Loc, Type>,
    ) -> Result<(), E> {
        pattern_expression_default(self, expr)
    }

    fn map_pattern_expression(
        &mut self,
        expr: &'ast ast::expression::Expression<Loc, Loc>,
    ) -> ast::expression::Expression<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_pattern_expression_default(self, expr)
    }

    fn predicate(&mut self, pred: &'ast ast::types::Predicate<Loc, Type>) -> Result<(), E> {
        predicate_default(self, pred)
    }

    fn map_predicate(
        &mut self,
        pred: &'ast ast::types::Predicate<Loc, Loc>,
    ) -> ast::types::Predicate<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_predicate_default(self, pred)
    }

    fn predicate_expression(
        &mut self,
        expr: &'ast ast::expression::Expression<Loc, Type>,
    ) -> Result<(), E> {
        predicate_expression_default(self, expr)
    }

    fn map_predicate_expression(
        &mut self,
        expr: &'ast ast::expression::Expression<Loc, Loc>,
    ) -> ast::expression::Expression<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_predicate_expression_default(self, expr)
    }

    fn type_guard_annotation(
        &mut self,
        annot: &'ast ast::types::TypeGuardAnnotation<Loc, Type>,
    ) -> Result<(), E> {
        type_guard_annotation_default(self, annot)
    }

    fn map_type_guard_annotation(
        &mut self,
        annot: &'ast ast::types::TypeGuardAnnotation<Loc, Loc>,
    ) -> ast::types::TypeGuardAnnotation<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_type_guard_annotation_default(self, annot)
    }

    fn type_guard(&mut self, guard: &'ast ast::types::TypeGuard<Loc, Type>) -> Result<(), E> {
        type_guard_default(self, guard)
    }

    fn map_type_guard(
        &mut self,
        guard: &'ast ast::types::TypeGuard<Loc, Loc>,
    ) -> ast::types::TypeGuard<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_type_guard_default(self, guard)
    }

    fn function_rest_param(
        &mut self,
        param: &'ast ast::function::RestParam<Loc, Type>,
    ) -> Result<(), E> {
        function_rest_param_default(self, param)
    }

    fn map_function_rest_param(
        &mut self,
        param: &'ast ast::function::RestParam<Loc, Loc>,
    ) -> ast::function::RestParam<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_function_rest_param_default(self, param)
    }

    fn return_(
        &mut self,
        loc: &'ast Loc,
        ret: &'ast ast::statement::Return<Loc, Type>,
    ) -> Result<(), E> {
        return_default(self, loc, ret)
    }

    fn map_return_(
        &mut self,
        loc: &'ast Loc,
        ret: &'ast ast::statement::Return<Loc, Loc>,
    ) -> ast::statement::Return<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_return_default(self, loc, ret)
    }

    fn sequence(
        &mut self,
        loc: &'ast Type,
        seq: &'ast ast::expression::Sequence<Loc, Type>,
    ) -> Result<(), E> {
        sequence_default(self, loc, seq)
    }

    fn map_sequence(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::Sequence<Loc, Loc>,
    ) -> ast::expression::Sequence<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_sequence_default(self, loc, expr)
    }

    fn toplevel_statement_list(
        &mut self,
        stmts: &'ast [ast::statement::Statement<Loc, Type>],
    ) -> Result<(), E> {
        toplevel_statement_list_default(self, stmts)
    }

    fn map_toplevel_statement_list(
        &mut self,
        stmts: &'ast Arc<[ast::statement::Statement<Loc, Loc>]>,
    ) -> Arc<[ast::statement::Statement<Loc, Loc>]>
    where
        Loc: Dupe,
    {
        map_toplevel_statement_list_default(self, stmts)
    }

    fn statement_list(
        &mut self,
        stmts: &'ast [ast::statement::Statement<Loc, Type>],
    ) -> Result<(), E> {
        statement_list_default(self, stmts)
    }

    fn map_statement_list(
        &mut self,
        stmts: &'ast Arc<[ast::statement::Statement<Loc, Loc>]>,
    ) -> Arc<[ast::statement::Statement<Loc, Loc>]>
    where
        Loc: Dupe,
    {
        map_statement_list_default(self, stmts)
    }

    fn spread_element(
        &mut self,
        spread: &'ast ast::expression::SpreadElement<Loc, Type>,
    ) -> Result<(), E> {
        spread_element_default(self, spread)
    }

    fn map_spread_element(
        &mut self,
        spread: &'ast ast::expression::SpreadElement<Loc, Loc>,
    ) -> ast::expression::SpreadElement<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_spread_element_default(self, spread)
    }

    fn spread_property(
        &mut self,
        spread: &'ast ast::expression::object::SpreadProperty<Loc, Type>,
    ) -> Result<(), E> {
        spread_property_default(self, spread)
    }

    fn map_spread_property(
        &mut self,
        spread: &'ast ast::expression::object::SpreadProperty<Loc, Loc>,
    ) -> ast::expression::object::SpreadProperty<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_spread_property_default(self, spread)
    }

    fn super_expression(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::Super<Loc>,
    ) -> Result<(), E> {
        super_expression_default(self, loc, expr)
    }

    fn map_super_expression(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::Super<Loc>,
    ) -> ast::expression::Super<Loc>
    where
        Loc: Dupe,
    {
        map_super_expression_default(self, loc, expr)
    }

    fn switch(
        &mut self,
        loc: &'ast Loc,
        switch: &'ast ast::statement::Switch<Loc, Type>,
    ) -> Result<(), E> {
        switch_default(self, loc, switch)
    }

    fn map_switch(
        &mut self,
        loc: &'ast Loc,
        switch: &'ast ast::statement::Switch<Loc, Loc>,
    ) -> ast::statement::Switch<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_switch_default(self, loc, switch)
    }

    fn switch_case(
        &mut self,
        case: &'ast ast::statement::switch::Case<Loc, Type>,
    ) -> Result<(), E> {
        switch_case_default(self, case)
    }

    fn map_switch_case(
        &mut self,
        case: &'ast ast::statement::switch::Case<Loc, Loc>,
    ) -> ast::statement::switch::Case<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_switch_case_default(self, case)
    }

    fn tagged_template(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::TaggedTemplate<Loc, Type>,
    ) -> Result<(), E> {
        tagged_template_default(self, loc, expr)
    }

    fn map_tagged_template(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::TaggedTemplate<Loc, Loc>,
    ) -> ast::expression::TaggedTemplate<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_tagged_template_default(self, loc, expr)
    }

    fn template_literal(
        &mut self,
        loc: C,
        lit: &'ast ast::expression::TemplateLiteral<Loc, Type>,
    ) -> Result<(), E> {
        template_literal_default(self, loc, lit)
    }

    fn map_template_literal(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::TemplateLiteral<Loc, Loc>,
    ) -> ast::expression::TemplateLiteral<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_template_literal_default(self, loc, expr)
    }

    fn template_literal_element(
        &mut self,
        elem: &'ast ast::expression::template_literal::Element<Loc>,
    ) -> Result<(), E> {
        template_literal_element_default(self, elem)
    }

    fn map_template_literal_element(
        &mut self,
        elem: &'ast ast::expression::template_literal::Element<Loc>,
    ) -> ast::expression::template_literal::Element<Loc>
    where
        Loc: Dupe,
    {
        map_template_literal_element_default(self, elem)
    }

    fn this_expression(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::This<Loc>,
    ) -> Result<(), E> {
        this_expression_default(self, loc, expr)
    }

    fn map_this_expression(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::This<Loc>,
    ) -> ast::expression::This<Loc>
    where
        Loc: Dupe,
    {
        map_this_expression_default(self, loc, expr)
    }

    fn throw(
        &mut self,
        loc: &'ast Loc,
        throw: &'ast ast::statement::Throw<Loc, Type>,
    ) -> Result<(), E> {
        throw_default(self, loc, throw)
    }

    fn map_throw(
        &mut self,
        loc: &'ast Loc,
        throw: &'ast ast::statement::Throw<Loc, Loc>,
    ) -> ast::statement::Throw<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_throw_default(self, loc, throw)
    }

    fn try_catch(
        &mut self,
        loc: &'ast Loc,
        try_stmt: &'ast ast::statement::Try<Loc, Type>,
    ) -> Result<(), E> {
        try_catch_default(self, loc, try_stmt)
    }

    fn map_try_catch(
        &mut self,
        loc: &'ast Loc,
        try_stmt: &'ast ast::statement::Try<Loc, Loc>,
    ) -> ast::statement::Try<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_try_catch_default(self, loc, try_stmt)
    }

    fn type_cast(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::TypeCast<Loc, Type>,
    ) -> Result<(), E> {
        type_cast_default(self, loc, expr)
    }

    fn map_type_cast(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::TypeCast<Loc, Loc>,
    ) -> ast::expression::TypeCast<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_type_cast_default(self, loc, expr)
    }

    fn ts_satisfies(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::TSSatisfies<Loc, Type>,
    ) -> Result<(), E> {
        ts_satisfies_default(self, loc, expr)
    }

    fn map_ts_satisfies(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::TSSatisfies<Loc, Loc>,
    ) -> ast::expression::TSSatisfies<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_ts_satisfies_default(self, loc, expr)
    }

    fn unary_expression(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::Unary<Loc, Type>,
    ) -> Result<(), E> {
        unary_expression_default(self, loc, expr)
    }

    fn map_unary_expression(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::Unary<Loc, Loc>,
    ) -> ast::expression::Unary<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_unary_expression_default(self, loc, expr)
    }

    fn update_expression(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::Update<Loc, Type>,
    ) -> Result<(), E> {
        update_expression_default(self, loc, expr)
    }

    fn map_update_expression(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::Update<Loc, Loc>,
    ) -> ast::expression::Update<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_update_expression_default(self, loc, expr)
    }

    fn variable_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::VariableDeclaration<Loc, Type>,
    ) -> Result<(), E> {
        variable_declaration_default(self, loc, decl)
    }

    fn map_variable_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::VariableDeclaration<Loc, Loc>,
    ) -> ast::statement::VariableDeclaration<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_variable_declaration_default(self, loc, decl)
    }

    fn variable_declarator(
        &mut self,
        kind: ast::VariableKind,
        declarator: &'ast ast::statement::variable::Declarator<Loc, Type>,
    ) -> Result<(), E> {
        variable_declarator_default(self, kind, declarator)
    }

    fn map_variable_declarator(
        &mut self,
        kind: ast::VariableKind,
        declarator: &'ast ast::statement::variable::Declarator<Loc, Loc>,
    ) -> ast::statement::variable::Declarator<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_variable_declarator_default(self, kind, declarator)
    }

    fn while_(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::While<Loc, Type>,
    ) -> Result<(), E> {
        while_default(self, loc, stmt)
    }

    fn map_while_(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::While<Loc, Loc>,
    ) -> ast::statement::While<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_while_default(self, loc, stmt)
    }

    fn with_(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::With<Loc, Type>,
    ) -> Result<(), E> {
        with_default(self, loc, stmt)
    }

    fn map_with_(
        &mut self,
        loc: &'ast Loc,
        stmt: &'ast ast::statement::With<Loc, Loc>,
    ) -> ast::statement::With<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_with_default(self, loc, stmt)
    }

    fn type_alias(
        &mut self,
        loc: &'ast Loc,
        alias: &'ast ast::statement::TypeAlias<Loc, Type>,
    ) -> Result<(), E> {
        type_alias_default(self, loc, alias)
    }

    fn map_type_alias(
        &mut self,
        loc: &'ast Loc,
        alias: &'ast ast::statement::TypeAlias<Loc, Loc>,
    ) -> ast::statement::TypeAlias<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_type_alias_default(self, loc, alias)
    }

    fn yield_(
        &mut self,
        loc: &'ast Type,
        expr: &'ast ast::expression::Yield<Loc, Type>,
    ) -> Result<(), E> {
        yield_default(self, loc, expr)
    }

    fn map_yield_(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::expression::Yield<Loc, Loc>,
    ) -> ast::expression::Yield<Loc, Loc>
    where
        Loc: Dupe,
    {
        map_yield_default(self, loc, expr)
    }
}

// ============================================================================
// Default implementations as free functions
// ============================================================================

pub fn program_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    program: &'ast ast::Program<Loc, Type>,
) -> Result<(), E> {
    let ast::Program {
        loc: _,
        statements,
        interpreter: _,
        comments,
        all_comments,
    } = program;
    visitor.toplevel_statement_list(statements)?;
    visitor.syntax_opt(comments.as_ref())?;
    for comment in all_comments.iter() {
        visitor.comment(comment)?;
    }
    Ok(())
}

pub fn map_program_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    program: &'ast ast::Program<Loc, Loc>,
) -> ast::Program<Loc, Loc> {
    let ast::Program {
        loc,
        statements,
        interpreter,
        comments,
        all_comments,
    } = program;
    let statements_ = visitor.map_toplevel_statement_list(statements);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    let all_comments_ = Arc::from(
        all_comments
            .iter()
            .map(|c| visitor.map_comment(c))
            .collect::<Vec<_>>(),
    );
    ast::Program {
        loc: loc.dupe(),
        statements: statements_,
        interpreter: interpreter.clone(),
        comments: comments_,
        all_comments: all_comments_,
    }
}

pub fn statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    stmt: &'ast ast::statement::Statement<Loc, Type>,
) -> Result<(), E> {
    match stmt.deref() {
        ast::statement::StatementInner::Block { loc, inner } => {
            visitor.block(loc, inner)?;
        }
        ast::statement::StatementInner::Break { loc, inner } => {
            visitor.break_(loc, inner)?;
        }
        ast::statement::StatementInner::ClassDeclaration { loc, inner } => {
            visitor.class_declaration(loc, inner)?;
        }
        ast::statement::StatementInner::ComponentDeclaration { loc, inner } => {
            visitor.component_declaration(loc, inner)?;
        }
        ast::statement::StatementInner::Continue { loc, inner } => {
            visitor.continue_(loc, inner)?;
        }
        ast::statement::StatementInner::Debugger { loc, inner } => {
            visitor.debugger(loc, inner)?;
        }
        ast::statement::StatementInner::DeclareClass { loc, inner } => {
            visitor.declare_class(loc, inner)?;
        }
        ast::statement::StatementInner::DeclareComponent { loc, inner } => {
            visitor.declare_component(loc, inner)?;
        }
        ast::statement::StatementInner::DeclareEnum { loc, inner } => {
            visitor.declare_enum(loc, inner)?;
        }
        ast::statement::StatementInner::DeclareExportDeclaration { loc, inner } => {
            visitor.declare_export_declaration(loc, inner)?;
        }
        ast::statement::StatementInner::DeclareFunction { loc, inner } => {
            visitor.declare_function(loc, inner)?;
        }
        ast::statement::StatementInner::DeclareInterface { loc, inner } => {
            visitor.declare_interface(loc, inner)?;
        }
        ast::statement::StatementInner::DeclareModule { loc, inner } => {
            visitor.declare_module(loc, inner)?;
        }
        ast::statement::StatementInner::DeclareModuleExports { loc, inner } => {
            visitor.declare_module_exports(loc, inner)?;
        }
        ast::statement::StatementInner::DeclareNamespace { loc, inner } => {
            visitor.declare_namespace(loc, inner)?;
        }
        ast::statement::StatementInner::DeclareTypeAlias { loc, inner } => {
            visitor.declare_type_alias(loc, inner)?;
        }
        ast::statement::StatementInner::DeclareOpaqueType { loc, inner } => {
            visitor.opaque_type(loc, inner)?;
        }
        ast::statement::StatementInner::DeclareVariable { loc, inner } => {
            visitor.declare_variable(loc, inner)?;
        }
        ast::statement::StatementInner::DoWhile { loc, inner } => {
            visitor.do_while(loc, inner)?;
        }
        ast::statement::StatementInner::Empty { loc, inner } => {
            visitor.empty(loc, inner)?;
        }
        ast::statement::StatementInner::EnumDeclaration { loc, inner } => {
            visitor.enum_declaration(loc, inner)?;
        }
        ast::statement::StatementInner::ExportDefaultDeclaration { loc, inner } => {
            visitor.export_default_declaration(loc, inner)?;
        }
        ast::statement::StatementInner::ExportNamedDeclaration { loc, inner } => {
            visitor.export_named_declaration(loc, inner)?;
        }
        ast::statement::StatementInner::ExportAssignment { loc, inner } => {
            visitor.export_assignment(loc, inner)?;
        }
        ast::statement::StatementInner::NamespaceExportDeclaration { loc, inner } => {
            visitor.namespace_export_declaration(loc, inner)?;
        }
        ast::statement::StatementInner::Expression { loc, inner } => {
            visitor.expression_statement(loc, inner)?;
        }
        ast::statement::StatementInner::For { loc, inner } => {
            visitor.for_statement(loc, inner)?;
        }
        ast::statement::StatementInner::ForIn { loc, inner } => {
            visitor.for_in_statement(loc, inner)?;
        }
        ast::statement::StatementInner::ForOf { loc, inner } => {
            visitor.for_of_statement(loc, inner)?;
        }
        ast::statement::StatementInner::FunctionDeclaration { loc, inner } => {
            visitor.function_declaration(loc, inner)?;
        }
        ast::statement::StatementInner::If { loc, inner } => {
            visitor.if_statement(loc, inner)?;
        }
        ast::statement::StatementInner::ImportDeclaration { loc, inner } => {
            visitor.import_declaration(loc, inner)?;
        }
        ast::statement::StatementInner::ImportEqualsDeclaration { loc, inner } => {
            visitor.import_equals_declaration(loc, inner)?;
        }
        ast::statement::StatementInner::InterfaceDeclaration { loc, inner } => {
            visitor.interface_declaration(loc, inner)?;
        }
        ast::statement::StatementInner::Labeled { loc, inner } => {
            visitor.labeled_statement(loc, inner)?;
        }
        ast::statement::StatementInner::Match { loc, inner } => {
            visitor.match_statement(loc, inner)?;
        }
        ast::statement::StatementInner::RecordDeclaration { loc, inner } => {
            visitor.record_declaration(loc, inner)?;
        }
        ast::statement::StatementInner::Return { loc, inner } => {
            visitor.return_(loc, inner)?;
        }
        ast::statement::StatementInner::Switch { loc, inner } => {
            visitor.switch(loc, inner)?;
        }
        ast::statement::StatementInner::Throw { loc, inner } => {
            visitor.throw(loc, inner)?;
        }
        ast::statement::StatementInner::Try { loc, inner } => {
            visitor.try_catch(loc, inner)?;
        }
        ast::statement::StatementInner::TypeAlias { loc, inner } => {
            visitor.type_alias(loc, inner)?;
        }
        ast::statement::StatementInner::OpaqueType { loc, inner } => {
            visitor.opaque_type(loc, inner)?;
        }
        ast::statement::StatementInner::VariableDeclaration { loc, inner } => {
            visitor.variable_declaration(loc, inner)?;
        }
        ast::statement::StatementInner::While { loc, inner } => {
            visitor.while_(loc, inner)?;
        }
        ast::statement::StatementInner::With { loc, inner } => {
            visitor.with_(loc, inner)?;
        }
    }
    Ok(())
}

pub fn map_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    stmt: &'ast ast::statement::Statement<Loc, Loc>,
) -> ast::statement::Statement<Loc, Loc> {
    ast::statement::Statement::new(match &**stmt {
        ast::statement::StatementInner::Block { loc, inner } => {
            ast::statement::StatementInner::Block {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_block(loc, inner)),
            }
        }
        ast::statement::StatementInner::Break { loc, inner } => {
            ast::statement::StatementInner::Break {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_break_(loc, inner)),
            }
        }
        ast::statement::StatementInner::ClassDeclaration { loc, inner } => {
            ast::statement::StatementInner::ClassDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_class_declaration(loc, inner)),
            }
        }
        ast::statement::StatementInner::ComponentDeclaration { loc, inner } => {
            ast::statement::StatementInner::ComponentDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_component_declaration(loc, inner)),
            }
        }
        ast::statement::StatementInner::Continue { loc, inner } => {
            ast::statement::StatementInner::Continue {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_continue_(loc, inner)),
            }
        }
        ast::statement::StatementInner::Debugger { loc, inner } => {
            ast::statement::StatementInner::Debugger {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_debugger(loc, inner)),
            }
        }
        ast::statement::StatementInner::DeclareClass { loc, inner } => {
            ast::statement::StatementInner::DeclareClass {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_declare_class(loc, inner)),
            }
        }
        ast::statement::StatementInner::DeclareComponent { loc, inner } => {
            ast::statement::StatementInner::DeclareComponent {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_declare_component(loc, inner)),
            }
        }
        ast::statement::StatementInner::DeclareEnum { loc, inner } => {
            ast::statement::StatementInner::DeclareEnum {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_declare_enum(loc, inner)),
            }
        }
        ast::statement::StatementInner::DeclareExportDeclaration { loc, inner } => {
            ast::statement::StatementInner::DeclareExportDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_declare_export_declaration(loc, inner)),
            }
        }
        ast::statement::StatementInner::DeclareFunction { loc, inner } => {
            ast::statement::StatementInner::DeclareFunction {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_declare_function(loc, inner)),
            }
        }
        ast::statement::StatementInner::DeclareInterface { loc, inner } => {
            ast::statement::StatementInner::DeclareInterface {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_declare_interface(loc, inner)),
            }
        }
        ast::statement::StatementInner::DeclareModule { loc, inner } => {
            ast::statement::StatementInner::DeclareModule {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_declare_module(loc, inner)),
            }
        }
        ast::statement::StatementInner::DeclareModuleExports { loc, inner } => {
            ast::statement::StatementInner::DeclareModuleExports {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_declare_module_exports(loc, inner)),
            }
        }
        ast::statement::StatementInner::DeclareNamespace { loc, inner } => {
            ast::statement::StatementInner::DeclareNamespace {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_declare_namespace(loc, inner)),
            }
        }
        ast::statement::StatementInner::DeclareTypeAlias { loc, inner } => {
            ast::statement::StatementInner::DeclareTypeAlias {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_declare_type_alias(loc, inner)),
            }
        }
        ast::statement::StatementInner::DeclareOpaqueType { loc, inner } => {
            ast::statement::StatementInner::DeclareOpaqueType {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_opaque_type(loc, inner)),
            }
        }
        ast::statement::StatementInner::DeclareVariable { loc, inner } => {
            ast::statement::StatementInner::DeclareVariable {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_declare_variable(loc, inner)),
            }
        }
        ast::statement::StatementInner::DoWhile { loc, inner } => {
            ast::statement::StatementInner::DoWhile {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_do_while(loc, inner)),
            }
        }
        ast::statement::StatementInner::Empty { loc, inner } => {
            ast::statement::StatementInner::Empty {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_empty(loc, inner)),
            }
        }
        ast::statement::StatementInner::EnumDeclaration { loc, inner } => {
            ast::statement::StatementInner::EnumDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_enum_declaration(loc, inner)),
            }
        }
        ast::statement::StatementInner::ExportDefaultDeclaration { loc, inner } => {
            ast::statement::StatementInner::ExportDefaultDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_export_default_declaration(loc, inner)),
            }
        }
        ast::statement::StatementInner::ExportNamedDeclaration { loc, inner } => {
            ast::statement::StatementInner::ExportNamedDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_export_named_declaration(loc, inner)),
            }
        }
        ast::statement::StatementInner::ExportAssignment { loc, inner } => {
            ast::statement::StatementInner::ExportAssignment {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_export_assignment(loc, inner)),
            }
        }
        ast::statement::StatementInner::NamespaceExportDeclaration { loc, inner } => {
            ast::statement::StatementInner::NamespaceExportDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_namespace_export_declaration(loc, inner)),
            }
        }
        ast::statement::StatementInner::Expression { loc, inner } => {
            ast::statement::StatementInner::Expression {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_expression_statement(loc, inner)),
            }
        }
        ast::statement::StatementInner::For { loc, inner } => ast::statement::StatementInner::For {
            loc: loc.dupe(),
            inner: Arc::new(visitor.map_for_statement(loc, inner)),
        },
        ast::statement::StatementInner::ForIn { loc, inner } => {
            ast::statement::StatementInner::ForIn {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_for_in_statement(loc, inner)),
            }
        }
        ast::statement::StatementInner::ForOf { loc, inner } => {
            ast::statement::StatementInner::ForOf {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_for_of_statement(loc, inner)),
            }
        }
        ast::statement::StatementInner::FunctionDeclaration { loc, inner } => {
            ast::statement::StatementInner::FunctionDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_function_declaration(loc, inner)),
            }
        }
        ast::statement::StatementInner::If { loc, inner } => ast::statement::StatementInner::If {
            loc: loc.dupe(),
            inner: Arc::new(visitor.map_if_statement(loc, inner)),
        },
        ast::statement::StatementInner::ImportDeclaration { loc, inner } => {
            ast::statement::StatementInner::ImportDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_import_declaration(loc, inner)),
            }
        }
        ast::statement::StatementInner::ImportEqualsDeclaration { loc, inner } => {
            ast::statement::StatementInner::ImportEqualsDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_import_equals_declaration(loc, inner)),
            }
        }
        ast::statement::StatementInner::InterfaceDeclaration { loc, inner } => {
            ast::statement::StatementInner::InterfaceDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_interface_declaration(loc, inner)),
            }
        }
        ast::statement::StatementInner::Labeled { loc, inner } => {
            ast::statement::StatementInner::Labeled {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_labeled_statement(loc, inner)),
            }
        }
        ast::statement::StatementInner::Match { loc, inner } => {
            ast::statement::StatementInner::Match {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_match_statement(loc, inner)),
            }
        }
        ast::statement::StatementInner::RecordDeclaration { loc, inner } => {
            ast::statement::StatementInner::RecordDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_record_declaration(loc, inner)),
            }
        }
        ast::statement::StatementInner::Return { loc, inner } => {
            ast::statement::StatementInner::Return {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_return_(loc, inner)),
            }
        }
        ast::statement::StatementInner::Switch { loc, inner } => {
            ast::statement::StatementInner::Switch {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_switch(loc, inner)),
            }
        }
        ast::statement::StatementInner::Throw { loc, inner } => {
            ast::statement::StatementInner::Throw {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_throw(loc, inner)),
            }
        }
        ast::statement::StatementInner::Try { loc, inner } => ast::statement::StatementInner::Try {
            loc: loc.dupe(),
            inner: Arc::new(visitor.map_try_catch(loc, inner)),
        },
        ast::statement::StatementInner::TypeAlias { loc, inner } => {
            ast::statement::StatementInner::TypeAlias {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_type_alias(loc, inner)),
            }
        }
        ast::statement::StatementInner::OpaqueType { loc, inner } => {
            ast::statement::StatementInner::OpaqueType {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_opaque_type(loc, inner)),
            }
        }
        ast::statement::StatementInner::VariableDeclaration { loc, inner } => {
            ast::statement::StatementInner::VariableDeclaration {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_variable_declaration(loc, inner)),
            }
        }
        ast::statement::StatementInner::While { loc, inner } => {
            ast::statement::StatementInner::While {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_while_(loc, inner)),
            }
        }
        ast::statement::StatementInner::With { loc, inner } => {
            ast::statement::StatementInner::With {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_with_(loc, inner)),
            }
        }
    })
}

pub fn expression_default<
    'ast,
    Loc: Dupe,
    Type: Dupe,
    C,
    E,
    V: AstVisitor<'ast, Loc, Type, C, E> + ?Sized,
>(
    visitor: &mut V,
    expr: &'ast ast::expression::Expression<Loc, Type>,
) -> Result<(), E> {
    match expr.deref() {
        ast::expression::ExpressionInner::Array { loc, inner } => {
            visitor.array(loc, inner)?;
        }
        ast::expression::ExpressionInner::ArrowFunction { loc, inner } => {
            visitor.arrow_function(loc, inner)?;
        }
        ast::expression::ExpressionInner::AsConstExpression { loc, inner } => {
            visitor.as_const_expression(loc, inner)?;
        }
        ast::expression::ExpressionInner::AsExpression { loc, inner } => {
            visitor.as_expression(loc, inner)?;
        }
        ast::expression::ExpressionInner::Assignment { loc, inner } => {
            visitor.assignment(loc, inner)?;
        }
        ast::expression::ExpressionInner::Binary { loc, inner } => {
            visitor.binary(loc, inner)?;
        }
        ast::expression::ExpressionInner::Call { loc, inner } => {
            visitor.call(loc, inner)?;
        }
        ast::expression::ExpressionInner::Class { loc, inner } => {
            visitor.class_expression(loc, inner)?;
        }
        ast::expression::ExpressionInner::Conditional { loc, inner } => {
            visitor.conditional(loc, inner)?;
        }
        ast::expression::ExpressionInner::Function { loc, inner } => {
            visitor.function_expression(loc, inner)?;
        }
        ast::expression::ExpressionInner::Identifier { loc: _, inner } => {
            visitor.identifier(inner)?;
        }
        ast::expression::ExpressionInner::Import { loc: _, inner } => {
            visitor.import(inner)?;
        }
        ast::expression::ExpressionInner::JSXElement { loc, inner } => {
            visitor.jsx_element(loc, inner)?;
        }
        ast::expression::ExpressionInner::JSXFragment { loc, inner } => {
            visitor.jsx_fragment(loc, inner)?;
        }
        ast::expression::ExpressionInner::StringLiteral { loc: _, inner } => {
            visitor.string_literal(inner)?;
        }
        ast::expression::ExpressionInner::BooleanLiteral { loc: _, inner } => {
            visitor.boolean_literal(inner)?;
        }
        ast::expression::ExpressionInner::NullLiteral { loc: _, inner } => {
            visitor.null_literal(inner.as_ref().as_ref())?;
        }
        ast::expression::ExpressionInner::NumberLiteral { loc: _, inner } => {
            visitor.number_literal(inner)?;
        }
        ast::expression::ExpressionInner::BigIntLiteral { loc: _, inner } => {
            visitor.bigint_literal(inner)?;
        }
        ast::expression::ExpressionInner::RegExpLiteral { loc: _, inner } => {
            visitor.regexp_literal(inner)?;
        }
        ast::expression::ExpressionInner::ModuleRefLiteral { loc: _, inner } => {
            visitor.module_ref_literal(inner)?;
        }
        ast::expression::ExpressionInner::Logical { loc, inner } => {
            visitor.logical(loc, inner)?;
        }
        ast::expression::ExpressionInner::Match { loc, inner } => {
            visitor.match_expression(loc, inner)?;
        }
        ast::expression::ExpressionInner::Member { loc, inner } => {
            visitor.member(loc, inner)?;
        }
        ast::expression::ExpressionInner::MetaProperty { loc, inner } => {
            visitor.meta_property(loc, inner)?;
        }
        ast::expression::ExpressionInner::New { loc, inner } => {
            visitor.new(loc, inner)?;
        }
        ast::expression::ExpressionInner::Object { loc, inner } => {
            visitor.object(V::normalize_type(loc), inner)?;
        }
        ast::expression::ExpressionInner::OptionalCall { loc, inner } => {
            visitor.optional_call(loc, inner)?;
        }
        ast::expression::ExpressionInner::OptionalMember { loc, inner } => {
            visitor.optional_member(loc, inner)?;
        }
        ast::expression::ExpressionInner::Record { loc, inner } => {
            visitor.record(loc, inner)?;
        }
        ast::expression::ExpressionInner::Sequence { loc, inner } => {
            visitor.sequence(loc, inner)?;
        }
        ast::expression::ExpressionInner::Super { loc, inner } => {
            visitor.super_expression(loc, inner)?;
        }
        ast::expression::ExpressionInner::TaggedTemplate { loc, inner } => {
            visitor.tagged_template(loc, inner)?;
        }
        ast::expression::ExpressionInner::TemplateLiteral { loc, inner } => {
            visitor.template_literal(V::normalize_type(loc), inner)?;
        }
        ast::expression::ExpressionInner::This { loc, inner } => {
            visitor.this_expression(loc, inner)?;
        }
        ast::expression::ExpressionInner::TypeCast { loc, inner } => {
            visitor.type_cast(loc, inner)?;
        }
        ast::expression::ExpressionInner::TSSatisfies { loc, inner } => {
            visitor.ts_satisfies(loc, inner)?;
        }
        ast::expression::ExpressionInner::Unary { loc, inner } => {
            visitor.unary_expression(loc, inner)?;
        }
        ast::expression::ExpressionInner::Update { loc, inner } => {
            visitor.update_expression(loc, inner)?;
        }
        ast::expression::ExpressionInner::Yield { loc, inner } => {
            visitor.yield_(loc, inner)?;
        }
    }
    Ok(())
}

pub fn map_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::expression::Expression<Loc, Loc>,
) -> ast::expression::Expression<Loc, Loc> {
    ast::expression::Expression::new(match expr.deref() {
        ast::expression::ExpressionInner::Array { loc, inner } => {
            ast::expression::ExpressionInner::Array {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_array(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::ArrowFunction { loc, inner } => {
            ast::expression::ExpressionInner::ArrowFunction {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_arrow_function(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::AsConstExpression { loc, inner } => {
            ast::expression::ExpressionInner::AsConstExpression {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_as_const_expression(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::AsExpression { loc, inner } => {
            ast::expression::ExpressionInner::AsExpression {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_as_expression(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::Assignment { loc, inner } => {
            ast::expression::ExpressionInner::Assignment {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_assignment(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::Binary { loc, inner } => {
            ast::expression::ExpressionInner::Binary {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_binary(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::Call { loc, inner } => {
            ast::expression::ExpressionInner::Call {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_call(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::Class { loc, inner } => {
            ast::expression::ExpressionInner::Class {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_class_expression(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::Conditional { loc, inner } => {
            ast::expression::ExpressionInner::Conditional {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_conditional(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::Function { loc, inner } => {
            ast::expression::ExpressionInner::Function {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_function_expression(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::Identifier { loc, inner } => {
            ast::expression::ExpressionInner::Identifier {
                loc: loc.dupe(),
                inner: visitor.map_identifier(inner),
            }
        }
        ast::expression::ExpressionInner::Import { loc, inner } => {
            ast::expression::ExpressionInner::Import {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_import(inner)),
            }
        }
        ast::expression::ExpressionInner::JSXElement { loc, inner } => {
            ast::expression::ExpressionInner::JSXElement {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_jsx_element(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::JSXFragment { loc, inner } => {
            ast::expression::ExpressionInner::JSXFragment {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_jsx_fragment(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::StringLiteral { loc, inner } => {
            ast::expression::ExpressionInner::StringLiteral {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_string_literal(inner)),
            }
        }
        ast::expression::ExpressionInner::BooleanLiteral { loc, inner } => {
            ast::expression::ExpressionInner::BooleanLiteral {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_boolean_literal(inner)),
            }
        }
        ast::expression::ExpressionInner::NullLiteral { loc, inner } => {
            ast::expression::ExpressionInner::NullLiteral {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_null_literal(inner.as_ref().as_ref())),
            }
        }
        ast::expression::ExpressionInner::NumberLiteral { loc, inner } => {
            ast::expression::ExpressionInner::NumberLiteral {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_number_literal(inner)),
            }
        }
        ast::expression::ExpressionInner::BigIntLiteral { loc, inner } => {
            ast::expression::ExpressionInner::BigIntLiteral {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_bigint_literal(inner)),
            }
        }
        ast::expression::ExpressionInner::RegExpLiteral { loc, inner } => {
            ast::expression::ExpressionInner::RegExpLiteral {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_regexp_literal(inner)),
            }
        }
        ast::expression::ExpressionInner::ModuleRefLiteral { loc, inner } => {
            ast::expression::ExpressionInner::ModuleRefLiteral {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_module_ref_literal(inner)),
            }
        }
        ast::expression::ExpressionInner::Logical { loc, inner } => {
            ast::expression::ExpressionInner::Logical {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_logical(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::Match { loc, inner } => {
            ast::expression::ExpressionInner::Match {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_match_expression(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::Member { loc, inner } => {
            ast::expression::ExpressionInner::Member {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_member(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::MetaProperty { loc, inner } => {
            ast::expression::ExpressionInner::MetaProperty {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_meta_property(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::New { loc, inner } => {
            ast::expression::ExpressionInner::New {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_new(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::Object { loc, inner } => {
            ast::expression::ExpressionInner::Object {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_object(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::OptionalCall { loc, inner } => {
            ast::expression::ExpressionInner::OptionalCall {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_optional_call(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::OptionalMember { loc, inner } => {
            ast::expression::ExpressionInner::OptionalMember {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_optional_member(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::Record { loc, inner } => {
            ast::expression::ExpressionInner::Record {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_record(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::Sequence { loc, inner } => {
            ast::expression::ExpressionInner::Sequence {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_sequence(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::Super { loc, inner } => {
            ast::expression::ExpressionInner::Super {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_super_expression(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::TaggedTemplate { loc, inner } => {
            ast::expression::ExpressionInner::TaggedTemplate {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_tagged_template(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::TemplateLiteral { loc, inner } => {
            ast::expression::ExpressionInner::TemplateLiteral {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_template_literal(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::This { loc, inner } => {
            ast::expression::ExpressionInner::This {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_this_expression(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::TypeCast { loc, inner } => {
            ast::expression::ExpressionInner::TypeCast {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_type_cast(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::TSSatisfies { loc, inner } => {
            ast::expression::ExpressionInner::TSSatisfies {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_ts_satisfies(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::Unary { loc, inner } => {
            ast::expression::ExpressionInner::Unary {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_unary_expression(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::Update { loc, inner } => {
            ast::expression::ExpressionInner::Update {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_update_expression(loc, inner)),
            }
        }
        ast::expression::ExpressionInner::Yield { loc, inner } => {
            ast::expression::ExpressionInner::Yield {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_yield_(loc, inner)),
            }
        }
    })
}

pub fn array_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    expr: &'ast ast::expression::Array<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::expression::Array {
        elements,
        trailing_comma: _,
        comments,
    } = expr;
    for element in elements.iter() {
        visitor.array_element(element)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_array_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::Array<Loc, Loc>,
) -> ast::expression::Array<Loc, Loc> {
    let ast::expression::Array {
        elements,
        trailing_comma,
        comments,
    } = expr;
    let elements_ = Arc::from(
        elements
            .iter()
            .map(|e| visitor.map_array_element(e))
            .collect::<Vec<_>>(),
    );
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::Array {
        elements: elements_,
        trailing_comma: *trailing_comma,
        comments: comments_,
    }
}

pub fn array_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    element: &'ast ast::expression::ArrayElement<Loc, Type>,
) -> Result<(), E> {
    match element {
        ast::expression::ArrayElement::Expression(expression) => {
            visitor.expression(expression)?;
        }
        ast::expression::ArrayElement::Spread(spread_element) => {
            visitor.spread_element(spread_element)?;
        }
        ast::expression::ArrayElement::Hole(_) => {}
    }
    Ok(())
}

pub fn map_array_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    element: &'ast ast::expression::ArrayElement<Loc, Loc>,
) -> ast::expression::ArrayElement<Loc, Loc> {
    match element {
        ast::expression::ArrayElement::Expression(expr) => {
            ast::expression::ArrayElement::Expression(visitor.map_expression(expr))
        }
        ast::expression::ArrayElement::Spread(spread) => {
            ast::expression::ArrayElement::Spread(visitor.map_spread_element(spread))
        }
        ast::expression::ArrayElement::Hole(loc) => ast::expression::ArrayElement::Hole(loc.dupe()),
    }
}

pub fn arrow_function_default<
    'ast,
    Loc: Dupe,
    Type: Dupe,
    C,
    E,
    V: AstVisitor<'ast, Loc, Type, C, E> + ?Sized,
>(
    visitor: &mut V,
    loc: &'ast Type,
    expr: &'ast ast::function::Function<Loc, Type>,
) -> Result<(), E> {
    visitor.function_(V::normalize_type(loc), expr)?;
    Ok(())
}

pub fn map_arrow_function_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    func: &'ast ast::function::Function<Loc, Loc>,
) -> ast::function::Function<Loc, Loc> {
    visitor.map_function_(loc, func)
}

pub fn as_const_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Type,
    expr: &'ast ast::expression::AsConstExpression<Loc, Type>,
) -> Result<(), E> {
    let ast::expression::AsConstExpression {
        expression,
        comments,
    } = expr;
    visitor.expression(expression)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_as_const_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::AsConstExpression<Loc, Loc>,
) -> ast::expression::AsConstExpression<Loc, Loc> {
    let ast::expression::AsConstExpression {
        expression,
        comments,
    } = expr;
    let expression_ = visitor.map_expression(expression);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::AsConstExpression {
        expression: expression_,
        comments: comments_,
    }
}

pub fn as_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Type,
    expr: &'ast ast::expression::AsExpression<Loc, Type>,
) -> Result<(), E> {
    let ast::expression::AsExpression {
        expression,
        annot,
        comments,
    } = expr;
    visitor.expression(expression)?;
    visitor.type_annotation(annot)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_as_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::AsExpression<Loc, Loc>,
) -> ast::expression::AsExpression<Loc, Loc> {
    let ast::expression::AsExpression {
        expression,
        annot,
        comments,
    } = expr;
    let expression_ = visitor.map_expression(expression);
    let annot_ = visitor.map_type_annotation(annot);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::AsExpression {
        expression: expression_,
        annot: annot_,
        comments: comments_,
    }
}

pub fn assignment_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Type,
    expr: &'ast ast::expression::Assignment<Loc, Type>,
) -> Result<(), E> {
    let ast::expression::Assignment {
        operator: _,
        left,
        right,
        comments,
    } = expr;
    visitor.assignment_pattern(left)?;
    visitor.expression(right)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_assignment_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::Assignment<Loc, Loc>,
) -> ast::expression::Assignment<Loc, Loc> {
    let ast::expression::Assignment {
        operator,
        left,
        right,
        comments,
    } = expr;
    let left_ = visitor.map_pattern(None, left);
    let right_ = visitor.map_expression(right);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::Assignment {
        operator: *operator,
        left: left_,
        right: right_,
        comments: comments_,
    }
}

pub fn binary_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Type,
    expr: &'ast ast::expression::Binary<Loc, Type>,
) -> Result<(), E> {
    let ast::expression::Binary {
        operator: _,
        left,
        right,
        comments,
    } = expr;
    visitor.expression(left)?;
    visitor.expression(right)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_binary_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::Binary<Loc, Loc>,
) -> ast::expression::Binary<Loc, Loc> {
    let ast::expression::Binary {
        operator,
        left,
        right,
        comments,
    } = expr;
    let left_ = visitor.map_expression(left);
    let right_ = visitor.map_expression(right);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::Binary {
        operator: *operator,
        left: left_,
        right: right_,
        comments: comments_,
    }
}

pub fn block_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    stmt: &'ast ast::statement::Block<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::Block { body, comments } = stmt;
    visitor.statement_list(body)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_block_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    stmt: &'ast ast::statement::Block<Loc, Loc>,
) -> ast::statement::Block<Loc, Loc> {
    let ast::statement::Block { body, comments } = stmt;
    let body_ = visitor.map_statement_list(body);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::Block {
        body: body_,
        comments: comments_,
    }
}

pub fn break_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    break_stmt: &'ast ast::statement::Break<Loc>,
) -> Result<(), E> {
    let ast::statement::Break { label, comments } = break_stmt;
    if let Some(label) = label {
        visitor.label_identifier(label)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_break_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    break_stmt: &'ast ast::statement::Break<Loc>,
) -> ast::statement::Break<Loc> {
    let ast::statement::Break { label, comments } = break_stmt;
    let label_ = label.as_ref().map(|l| visitor.map_identifier(l));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::Break {
        label: label_,
        comments: comments_,
    }
}

pub fn call_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Type,
    expr: &'ast ast::expression::Call<Loc, Type>,
) -> Result<(), E> {
    let ast::expression::Call {
        callee,
        targs,
        arguments,
        comments,
    } = expr;
    visitor.expression(callee)?;
    if let Some(targs) = targs {
        visitor.call_type_args(targs)?;
    }
    visitor.arg_list(arguments)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_call_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::Call<Loc, Loc>,
) -> ast::expression::Call<Loc, Loc> {
    let ast::expression::Call {
        callee,
        targs,
        arguments,
        comments,
    } = expr;
    let callee_ = visitor.map_expression(callee);
    let targs_ = targs.as_ref().map(|t| visitor.map_call_type_args(t));
    let arguments_ = visitor.map_arg_list(arguments);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::Call {
        callee: callee_,
        targs: targs_,
        arguments: arguments_,
        comments: comments_,
    }
}

pub fn arg_list_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    arg_list: &'ast ast::expression::ArgList<Loc, Type>,
) -> Result<(), E> {
    let ast::expression::ArgList {
        loc: _,
        arguments,
        comments,
    } = arg_list;
    for arg in arguments.iter() {
        visitor.expression_or_spread(arg)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_arg_list_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    args: &'ast ast::expression::ArgList<Loc, Loc>,
) -> ast::expression::ArgList<Loc, Loc> {
    let ast::expression::ArgList {
        loc,
        arguments,
        comments,
    } = args;
    let arguments_ = Arc::from(
        arguments
            .iter()
            .map(|a| visitor.map_expression_or_spread(a))
            .collect::<Vec<_>>(),
    );
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::ArgList {
        loc: loc.dupe(),
        arguments: arguments_,
        comments: comments_,
    }
}

pub fn optional_call_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    expr: &'ast ast::expression::OptionalCall<Loc, Type>,
) -> Result<(), E> {
    let ast::expression::OptionalCall { call, .. } = expr;
    visitor.call(loc, call)?;
    Ok(())
}

pub fn map_optional_call_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    expr: &'ast ast::expression::OptionalCall<Loc, Loc>,
) -> ast::expression::OptionalCall<Loc, Loc> {
    let ast::expression::OptionalCall {
        call,
        filtered_out,
        optional,
    } = expr;
    let call_ = visitor.map_call(loc, call);
    let optional_ = match optional {
        ast::expression::OptionalCallKind::Optional => ast::expression::OptionalCallKind::Optional,
        ast::expression::OptionalCallKind::NonOptional => {
            ast::expression::OptionalCallKind::NonOptional
        }
        ast::expression::OptionalCallKind::AssertNonnull { assertion_loc } => {
            ast::expression::OptionalCallKind::AssertNonnull {
                assertion_loc: visitor.map_loc_annot(assertion_loc),
            }
        }
    };
    ast::expression::OptionalCall {
        call: call_,
        filtered_out: filtered_out.dupe(),
        optional: optional_,
    }
}

pub fn call_type_args_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    targs: &'ast ast::expression::CallTypeArgs<Loc, Type>,
) -> Result<(), E> {
    let ast::expression::CallTypeArgs {
        loc: _,
        arguments,
        comments,
    } = targs;
    for arg in arguments.iter() {
        visitor.call_type_arg(arg)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_call_type_args_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    targs: &'ast ast::expression::CallTypeArgs<Loc, Loc>,
) -> ast::expression::CallTypeArgs<Loc, Loc> {
    let ast::expression::CallTypeArgs {
        loc,
        arguments,
        comments,
    } = targs;
    let arguments_: Arc<[ast::expression::CallTypeArg<Loc, Loc>]> = arguments
        .iter()
        .map(|arg| visitor.map_call_type_arg(arg))
        .collect();
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::CallTypeArgs {
        loc: loc.dupe(),
        arguments: arguments_,
        comments: comments_,
    }
}

pub fn call_type_arg_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    t: &'ast ast::expression::CallTypeArg<Loc, Type>,
) -> Result<(), E> {
    match t {
        ast::expression::CallTypeArg::Explicit(type_) => {
            visitor.type_(type_)?;
        }
        ast::expression::CallTypeArg::Implicit(ast::expression::CallTypeArgImplicit {
            loc: _,
            comments,
        }) => {
            visitor.syntax_opt(comments.as_ref())?;
        }
    }
    Ok(())
}

pub fn map_call_type_arg_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    t: &'ast ast::expression::CallTypeArg<Loc, Loc>,
) -> ast::expression::CallTypeArg<Loc, Loc> {
    match t {
        ast::expression::CallTypeArg::Explicit(type_) => {
            ast::expression::CallTypeArg::Explicit(visitor.map_type_(type_))
        }
        ast::expression::CallTypeArg::Implicit(ast::expression::CallTypeArgImplicit {
            loc,
            comments,
        }) => ast::expression::CallTypeArg::Implicit(ast::expression::CallTypeArgImplicit {
            loc: loc.dupe(),
            comments: visitor.map_syntax_opt(comments.as_ref()),
        }),
    }
}

pub fn catch_body_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    body: &'ast (Loc, ast::statement::Block<Loc, Type>),
) -> Result<(), E> {
    let (loc, block) = body;
    visitor.block(loc, block)?;
    Ok(())
}

pub fn map_catch_body_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    body: &'ast (Loc, ast::statement::Block<Loc, Loc>),
) -> (Loc, ast::statement::Block<Loc, Loc>) {
    let (loc, block) = body;
    (loc.dupe(), visitor.map_block(loc, block))
}

pub fn catch_clause_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    clause: &'ast ast::statement::try_::CatchClause<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::try_::CatchClause {
        loc: _,
        param,
        body,
        comments,
        ..
    } = clause;
    if let Some(param) = param {
        visitor.catch_clause_pattern(param)?;
    }
    visitor.catch_body(body)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_catch_clause_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    catch: &'ast ast::statement::try_::CatchClause<Loc, Loc>,
) -> ast::statement::try_::CatchClause<Loc, Loc> {
    let ast::statement::try_::CatchClause {
        loc,
        param,
        body,
        comments,
    } = catch;
    let param_ = param.as_ref().map(|p| visitor.map_pattern(None, p));
    let (body_loc, body_inner) = body;
    let body_ = (body_loc.dupe(), visitor.map_block(body_loc, body_inner));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::try_::CatchClause {
        loc: loc.dupe(),
        param: param_,
        body: body_,
        comments: comments_,
    }
}

pub fn class_declaration_default<
    'ast,
    Loc: Dupe,
    Type: Dupe,
    C,
    E,
    V: AstVisitor<'ast, Loc, Type, C, E> + ?Sized,
>(
    visitor: &mut V,
    loc: &'ast Loc,
    cls: &'ast ast::class::Class<Loc, Type>,
) -> Result<(), E> {
    visitor.class_(V::normalize_loc(loc), cls)?;
    Ok(())
}

pub fn map_class_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    cls: &'ast ast::class::Class<Loc, Loc>,
) -> ast::class::Class<Loc, Loc> {
    visitor.map_class_(loc, cls)
}

pub fn class_expression_default<
    'ast,
    Loc: Dupe,
    Type: Dupe,
    C,
    E,
    V: AstVisitor<'ast, Loc, Type, C, E> + ?Sized,
>(
    visitor: &mut V,
    loc: &'ast Type,
    cls: &'ast ast::class::Class<Loc, Type>,
) -> Result<(), E> {
    visitor.class_(V::normalize_type(loc), cls)?;
    Ok(())
}

pub fn map_class_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    cls: &'ast ast::class::Class<Loc, Loc>,
) -> ast::class::Class<Loc, Loc> {
    visitor.map_class_(loc, cls)
}

pub fn class_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: C,
    cls: &'ast ast::class::Class<Loc, Type>,
) -> Result<(), E> {
    let ast::class::Class {
        id,
        body,
        tparams,
        extends,
        implements,
        class_decorators,
        abstract_: _,
        comments,
    } = cls;
    if let Some(id) = id {
        visitor.class_identifier(id)?;
    }
    if let Some(tparams) = tparams {
        visitor.type_params(&TypeParamsContext::Class, tparams)?;
    }
    visitor.class_body(body)?;
    if let Some(extends) = extends {
        visitor.class_extends(extends)?;
    }
    if let Some(implements) = implements {
        visitor.class_implements(implements)?;
    }
    for decorator in class_decorators.iter() {
        visitor.class_decorator(decorator)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_class_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    cls: &'ast ast::class::Class<Loc, Loc>,
) -> ast::class::Class<Loc, Loc> {
    let ast::class::Class {
        id,
        body,
        tparams,
        extends,
        implements,
        class_decorators,
        abstract_,
        comments,
    } = cls;

    let id_ = id.as_ref().map(|id| visitor.map_class_identifier(id));
    let tparams_ = tparams
        .as_ref()
        .map(|tp| visitor.map_type_params(&TypeParamsContext::Class, tp));
    let extends_ = extends.as_ref().map(|ext| visitor.map_class_extends(ext));
    let implements_ = implements
        .as_ref()
        .map(|imp| visitor.map_class_implements(imp));
    let class_decorators_: Arc<[ast::class::Decorator<Loc, Loc>]> = class_decorators
        .iter()
        .map(|dec| visitor.map_class_decorator(dec))
        .collect();
    let body_ = visitor.map_class_body(body);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::class::Class {
        id: id_,
        body: body_,
        tparams: tparams_,
        extends: extends_,
        implements: implements_,
        class_decorators: class_decorators_,
        abstract_: *abstract_,
        comments: comments_,
    }
}

pub fn class_extends_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    extends: &'ast ast::class::Extends<Loc, Type>,
) -> Result<(), E> {
    let ast::class::Extends {
        loc: _,
        expr,
        targs,
        comments,
    } = extends;
    visitor.expression(expr)?;
    if let Some(targs) = targs {
        visitor.type_args(targs)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_class_extends_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    extends: &'ast ast::class::Extends<Loc, Loc>,
) -> ast::class::Extends<Loc, Loc> {
    let ast::class::Extends {
        loc,
        expr,
        targs,
        comments,
    } = extends;
    let expr_ = visitor.map_expression(expr);
    let targs_ = targs.as_ref().map(|t| visitor.map_type_args(t));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::class::Extends {
        loc: loc.dupe(),
        expr: expr_,
        targs: targs_,
        comments: comments_,
    }
}

pub fn class_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ident: &'ast ast::Identifier<Loc, Type>,
) -> Result<(), E> {
    visitor.pattern_identifier(Some(ast::VariableKind::Let), ident)?;
    Ok(())
}

pub fn map_class_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ident: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    visitor.map_identifier(ident)
}

pub fn class_body_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    cls_body: &'ast ast::class::Body<Loc, Type>,
) -> Result<(), E> {
    let ast::class::Body { body, comments, .. } = cls_body;
    for elem in body.iter() {
        visitor.class_element(elem)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_class_body_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    cls_body: &'ast ast::class::Body<Loc, Loc>,
) -> ast::class::Body<Loc, Loc> {
    let ast::class::Body {
        loc,
        body,
        comments,
    } = cls_body;

    let body_: Arc<[ast::class::BodyElement<Loc, Loc>]> = body
        .iter()
        .map(|elem| visitor.map_class_element(elem))
        .collect();
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::class::Body {
        loc: loc.dupe(),
        body: body_,
        comments: comments_,
    }
}

pub fn class_decorator_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    dec: &'ast ast::class::Decorator<Loc, Type>,
) -> Result<(), E> {
    let ast::class::Decorator {
        expression,
        comments,
        ..
    } = dec;
    visitor.expression(expression)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_class_decorator_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    dec: &'ast ast::class::Decorator<Loc, Loc>,
) -> ast::class::Decorator<Loc, Loc> {
    let ast::class::Decorator {
        loc,
        expression,
        comments,
    } = dec;

    let expression_ = visitor.map_expression(expression);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::class::Decorator {
        loc: loc.dupe(),
        expression: expression_,
        comments: comments_,
    }
}

pub fn class_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    elem: &'ast ast::class::BodyElement<Loc, Type>,
) -> Result<(), E> {
    match elem {
        ast::class::BodyElement::Method(method) => {
            visitor.class_method(method)?;
        }
        ast::class::BodyElement::Property(prop) => {
            visitor.class_property(prop)?;
        }
        ast::class::BodyElement::PrivateField(field) => {
            visitor.class_private_field(field)?;
        }
        ast::class::BodyElement::StaticBlock(block) => {
            visitor.class_static_block(block)?;
        }
        ast::class::BodyElement::DeclareMethod(decl_meth) => {
            visitor.class_declare_method(decl_meth)?;
        }
        ast::class::BodyElement::AbstractMethod(abs_meth) => {
            visitor.class_abstract_method(abs_meth)?;
        }
        ast::class::BodyElement::AbstractProperty(abs_prop) => {
            visitor.class_abstract_property(abs_prop)?;
        }
        ast::class::BodyElement::IndexSignature(indexer) => {
            visitor.object_indexer_property_type(indexer)?;
        }
    }
    Ok(())
}

pub fn map_class_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    elem: &'ast ast::class::BodyElement<Loc, Loc>,
) -> ast::class::BodyElement<Loc, Loc> {
    match elem {
        ast::class::BodyElement::Method(method) => {
            ast::class::BodyElement::Method(visitor.map_class_method(method))
        }
        ast::class::BodyElement::Property(prop) => {
            ast::class::BodyElement::Property(visitor.map_class_property(prop))
        }
        ast::class::BodyElement::PrivateField(prop) => {
            ast::class::BodyElement::PrivateField(visitor.map_class_private_field(prop))
        }
        ast::class::BodyElement::StaticBlock(block) => {
            ast::class::BodyElement::StaticBlock(visitor.map_class_static_block(block))
        }
        ast::class::BodyElement::DeclareMethod(decl_meth) => {
            ast::class::BodyElement::DeclareMethod(visitor.map_class_declare_method(decl_meth))
        }
        ast::class::BodyElement::AbstractMethod(abs_meth) => {
            ast::class::BodyElement::AbstractMethod(visitor.map_class_abstract_method(abs_meth))
        }
        ast::class::BodyElement::AbstractProperty(abs_prop) => {
            ast::class::BodyElement::AbstractProperty(visitor.map_class_abstract_property(abs_prop))
        }
        ast::class::BodyElement::IndexSignature(indexer) => {
            ast::class::BodyElement::IndexSignature(
                visitor.map_object_indexer_property_type(indexer),
            )
        }
    }
}

pub fn class_method_default<
    'ast,
    Loc: Dupe,
    Type: Dupe,
    C,
    E,
    V: AstVisitor<'ast, Loc, Type, C, E> + ?Sized,
>(
    visitor: &mut V,
    meth: &'ast ast::class::Method<Loc, Type>,
) -> Result<(), E> {
    let ast::class::Method {
        loc: _,
        kind: _,
        key,
        value,
        static_: _,
        override_: _,
        ts_accessibility: _,
        decorators,
        comments,
    } = meth;
    visitor.object_key(key)?;
    let (loc, func) = value;
    visitor.function_expression_or_method(V::normalize_loc(loc), func)?;
    for decorator in decorators.iter() {
        visitor.class_decorator(decorator)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_class_method_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    method: &'ast ast::class::Method<Loc, Loc>,
) -> ast::class::Method<Loc, Loc> {
    let ast::class::Method {
        loc,
        kind,
        key,
        value,
        static_,
        override_,
        ts_accessibility,
        decorators,
        comments,
    } = method;

    let key_ = visitor.map_object_key(key);
    let (value_loc, value_fn) = value;
    let value_fn_ = visitor.map_function_expression_or_method(value_loc, value_fn);
    let decorators_: Arc<[ast::class::Decorator<Loc, Loc>]> = decorators
        .iter()
        .map(|dec| visitor.map_class_decorator(dec))
        .collect();
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::class::Method {
        loc: loc.dupe(),
        kind: *kind,
        key: key_,
        value: (value_loc.dupe(), value_fn_),
        static_: *static_,
        override_: *override_,
        ts_accessibility: ts_accessibility.clone(),
        decorators: decorators_,
        comments: comments_,
    }
}

pub fn class_implements_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    implements: &'ast ast::class::Implements<Loc, Type>,
) -> Result<(), E> {
    let ast::class::Implements {
        loc: _,
        interfaces,
        comments,
    } = implements;
    for interface in interfaces.iter() {
        visitor.class_implements_interface(interface)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_class_implements_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    implements: &'ast ast::class::Implements<Loc, Loc>,
) -> ast::class::Implements<Loc, Loc> {
    let ast::class::Implements {
        loc,
        interfaces,
        comments,
    } = implements;

    let interfaces_: Arc<[ast::class::implements::Interface<Loc, Loc>]> = interfaces
        .iter()
        .map(|iface| visitor.map_class_implements_interface(iface))
        .collect();
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::class::Implements {
        loc: loc.dupe(),
        interfaces: interfaces_,
        comments: comments_,
    }
}

pub fn class_implements_interface_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    interface: &'ast ast::class::implements::Interface<Loc, Type>,
) -> Result<(), E> {
    let ast::class::implements::Interface { loc: _, id, targs } = interface;
    visitor.generic_identifier_type(id)?;
    if let Some(targs) = targs {
        visitor.type_args(targs)?;
    }
    Ok(())
}

pub fn map_class_implements_interface_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    interface: &'ast ast::class::implements::Interface<Loc, Loc>,
) -> ast::class::implements::Interface<Loc, Loc> {
    let ast::class::implements::Interface { loc, id, targs } = interface;

    let id_ = visitor.map_generic_identifier_type(id);
    let targs_ = targs.as_ref().map(|t| visitor.map_type_args(t));

    ast::class::implements::Interface {
        loc: loc.dupe(),
        id: id_,
        targs: targs_,
    }
}

pub fn class_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    prop: &'ast ast::class::Property<Loc, Type>,
) -> Result<(), E> {
    let ast::class::Property {
        loc: _,
        key,
        value,
        annot,
        static_: _,
        override_: _,
        optional: _,
        definite: _,
        variance,
        ts_accessibility: _,
        decorators,
        comments,
    } = prop;
    visitor.object_key(key)?;
    visitor.class_property_value(value)?;
    visitor.type_annotation_hint(annot)?;
    visitor.variance_opt(variance.as_ref())?;
    for decorator in decorators.iter() {
        visitor.class_decorator(decorator)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_class_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    prop: &'ast ast::class::Property<Loc, Loc>,
) -> ast::class::Property<Loc, Loc> {
    let ast::class::Property {
        loc,
        key,
        value,
        annot,
        static_,
        override_,
        optional,
        definite,
        variance,
        ts_accessibility,
        decorators,
        comments,
    } = prop;

    let key_ = visitor.map_object_key(key);
    let value_ = visitor.map_class_property_value(value);
    let annot_ = visitor.map_type_annotation_hint(annot);
    let decorators_: Arc<[ast::class::Decorator<Loc, Loc>]> = decorators
        .iter()
        .map(|dec| visitor.map_class_decorator(dec))
        .collect();
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::class::Property {
        loc: loc.dupe(),
        key: key_,
        value: value_,
        annot: annot_,
        static_: *static_,
        override_: *override_,
        optional: *optional,
        definite: *definite,
        variance: variance.clone(),
        ts_accessibility: ts_accessibility.clone(),
        decorators: decorators_,
        comments: comments_,
    }
}

pub fn class_property_value_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    value: &'ast ast::class::property::Value<Loc, Type>,
) -> Result<(), E> {
    match value {
        ast::class::property::Value::Declared | ast::class::property::Value::Uninitialized => {}
        ast::class::property::Value::Initialized(expr) => {
            visitor.expression(expr)?;
        }
    }
    Ok(())
}

pub fn map_class_property_value_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    value: &'ast ast::class::property::Value<Loc, Loc>,
) -> ast::class::property::Value<Loc, Loc> {
    match value {
        ast::class::property::Value::Declared => ast::class::property::Value::Declared,
        ast::class::property::Value::Uninitialized => ast::class::property::Value::Uninitialized,
        ast::class::property::Value::Initialized(expr) => {
            ast::class::property::Value::Initialized(visitor.map_expression(expr))
        }
    }
}

pub fn class_private_field_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    prop: &'ast ast::class::PrivateField<Loc, Type>,
) -> Result<(), E> {
    let ast::class::PrivateField {
        loc: _,
        key,
        value,
        annot,
        static_: _,
        override_: _,
        optional: _,
        definite: _,
        variance,
        ts_accessibility: _,
        decorators,
        comments,
    } = prop;
    visitor.private_name(key)?;
    visitor.class_property_value(value)?;
    visitor.type_annotation_hint(annot)?;
    visitor.variance_opt(variance.as_ref())?;
    for decorator in decorators.iter() {
        visitor.class_decorator(decorator)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_class_private_field_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    prop: &'ast ast::class::PrivateField<Loc, Loc>,
) -> ast::class::PrivateField<Loc, Loc> {
    let ast::class::PrivateField {
        loc,
        key,
        value,
        annot,
        static_,
        override_,
        optional,
        definite,
        variance,
        ts_accessibility,
        decorators,
        comments,
    } = prop;

    let key_ = visitor.map_private_name(key);
    let value_ = visitor.map_class_property_value(value);
    let annot_ = visitor.map_type_annotation_hint(annot);
    let decorators_: Arc<[ast::class::Decorator<Loc, Loc>]> = decorators
        .iter()
        .map(|dec| visitor.map_class_decorator(dec))
        .collect();
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::class::PrivateField {
        loc: loc.dupe(),
        key: key_,
        value: value_,
        annot: annot_,
        static_: *static_,
        override_: *override_,
        optional: *optional,
        definite: *definite,
        variance: variance.clone(),
        ts_accessibility: ts_accessibility.clone(),
        decorators: decorators_,
        comments: comments_,
    }
}

pub fn class_static_block_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    block: &'ast ast::class::StaticBlock<Loc, Type>,
) -> Result<(), E> {
    let ast::class::StaticBlock {
        loc: _,
        body,
        comments,
    } = block;
    visitor.statement_list(body)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_class_static_block_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    block: &'ast ast::class::StaticBlock<Loc, Loc>,
) -> ast::class::StaticBlock<Loc, Loc> {
    let ast::class::StaticBlock {
        loc,
        body,
        comments,
    } = block;

    let body_: Arc<[ast::statement::Statement<Loc, Loc>]> = body
        .iter()
        .map(|stmt| visitor.map_statement(stmt))
        .collect();
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::class::StaticBlock {
        loc: loc.dupe(),
        body: body_,
        comments: comments_,
    }
}

pub fn class_declare_method_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    decl_meth: &'ast ast::class::DeclareMethod<Loc, Type>,
) -> Result<(), E> {
    let ast::class::DeclareMethod {
        loc: _,
        kind: _,
        key,
        annot,
        static_: _,
        override_: _,
        optional: _,
        ts_accessibility: _,
        comments,
    } = decl_meth;
    visitor.object_key(key)?;
    visitor.type_annotation(annot)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_class_declare_method_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    decl_meth: &'ast ast::class::DeclareMethod<Loc, Loc>,
) -> ast::class::DeclareMethod<Loc, Loc> {
    let ast::class::DeclareMethod {
        loc,
        kind,
        key,
        annot,
        static_,
        override_,
        optional,
        ts_accessibility,
        comments,
    } = decl_meth;

    let key_ = visitor.map_object_key(key);
    let annot_ = visitor.map_type_annotation(annot);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::class::DeclareMethod {
        loc: loc.dupe(),
        kind: *kind,
        key: key_,
        annot: annot_,
        static_: *static_,
        override_: *override_,
        optional: *optional,
        ts_accessibility: ts_accessibility.clone(),
        comments: comments_,
    }
}

pub fn class_abstract_method_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    abs_meth: &'ast ast::class::AbstractMethod<Loc, Type>,
) -> Result<(), E> {
    let ast::class::AbstractMethod {
        loc: _,
        key,
        annot: (_annot_loc, func),
        override_: _,
        ts_accessibility: _,
        comments,
    } = abs_meth;
    visitor.object_key(key)?;
    visitor.function_type(func)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_class_abstract_method_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    abs_meth: &'ast ast::class::AbstractMethod<Loc, Loc>,
) -> ast::class::AbstractMethod<Loc, Loc> {
    let ast::class::AbstractMethod {
        loc,
        key,
        annot: (annot_loc, func),
        override_,
        ts_accessibility,
        comments,
    } = abs_meth;

    let key_ = visitor.map_object_key(key);
    let func_ = visitor.map_function_type(annot_loc, func);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::class::AbstractMethod {
        loc: loc.dupe(),
        key: key_,
        annot: (annot_loc.dupe(), func_),
        override_: *override_,
        ts_accessibility: ts_accessibility.clone(),
        comments: comments_,
    }
}

pub fn class_abstract_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    abs_prop: &'ast ast::class::AbstractProperty<Loc, Type>,
) -> Result<(), E> {
    let ast::class::AbstractProperty {
        loc: _,
        key,
        annot,
        override_: _,
        ts_accessibility: _,
        variance,
        comments,
    } = abs_prop;
    visitor.object_key(key)?;
    visitor.type_annotation_hint(annot)?;
    visitor.variance_opt(variance.as_ref())?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_class_abstract_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    abs_prop: &'ast ast::class::AbstractProperty<Loc, Loc>,
) -> ast::class::AbstractProperty<Loc, Loc> {
    let ast::class::AbstractProperty {
        loc,
        key,
        annot,
        override_,
        ts_accessibility,
        variance,
        comments,
    } = abs_prop;

    let key_ = visitor.map_object_key(key);
    let annot_ = visitor.map_type_annotation_hint(annot);
    let variance_ = visitor.map_variance_opt(variance.as_ref());
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::class::AbstractProperty {
        loc: loc.dupe(),
        key: key_,
        annot: annot_,
        override_: *override_,
        ts_accessibility: ts_accessibility.clone(),
        variance: variance_,
        comments: comments_,
    }
}

pub fn default_opt_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    default: Option<&'ast ast::expression::Expression<Loc, Type>>,
) -> Result<(), E> {
    if let Some(expr) = default {
        visitor.expression(expr)?;
    }
    Ok(())
}

pub fn map_default_opt_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    default: Option<&'ast ast::expression::Expression<Loc, Loc>>,
) -> Option<ast::expression::Expression<Loc, Loc>> {
    default.map(|expr| visitor.map_expression(expr))
}

pub fn component_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    component: &'ast ast::statement::ComponentDeclaration<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::ComponentDeclaration {
        id,
        tparams,
        params,
        body,
        renders,
        comments,
        sig_loc: _,
        async_: _,
    } = component;
    visitor.component_identifier(id)?;
    if let Some(tparams) = tparams {
        visitor.type_params(&TypeParamsContext::ComponentDeclaration, tparams)?;
    }
    visitor.component_params(params)?;
    if let Some(body) = body {
        visitor.component_body(body)?;
    }
    visitor.component_renders_annotation(renders)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_component_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    component: &'ast ast::statement::ComponentDeclaration<Loc, Loc>,
) -> ast::statement::ComponentDeclaration<Loc, Loc> {
    let ast::statement::ComponentDeclaration {
        id,
        tparams,
        params,
        renders,
        body,
        comments,
        sig_loc,
        async_,
    } = component;

    let id_ = visitor.map_component_identifier(id);
    let tparams_ = tparams
        .as_ref()
        .map(|tp| visitor.map_type_params(&TypeParamsContext::ComponentDeclaration, tp));
    let params_ = visitor.map_component_params(params);
    let renders_ = visitor.map_component_renders_annotation(renders);
    let body_ = body.as_ref().map(|b| visitor.map_component_body(b));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::statement::ComponentDeclaration {
        id: id_,
        tparams: tparams_,
        params: params_,
        renders: renders_,
        body: body_,
        comments: comments_,
        sig_loc: sig_loc.dupe(),
        async_: *async_,
    }
}

pub fn component_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ident: &'ast ast::Identifier<Loc, Type>,
) -> Result<(), E> {
    visitor.pattern_identifier(Some(ast::VariableKind::Var), ident)?;
    Ok(())
}

pub fn map_component_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ident: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    visitor.map_identifier(ident)
}

pub fn component_params_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    params: &'ast ast::statement::component_params::Params<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::component_params::Params {
        loc: _,
        params: params_list,
        rest,
        comments,
    } = params;
    for param in params_list.iter() {
        visitor.component_param(param)?;
    }
    if let Some(rest) = rest {
        visitor.component_rest_param(rest)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_component_params_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    params: &'ast ast::statement::component_params::Params<Loc, Loc>,
) -> ast::statement::component_params::Params<Loc, Loc> {
    let ast::statement::component_params::Params {
        loc,
        params: param_list,
        rest,
        comments,
    } = params;

    let params_: Arc<[ast::statement::component_params::Param<Loc, Loc>]> = param_list
        .iter()
        .map(|p| visitor.map_component_param(p))
        .collect();
    let rest_ = rest.as_ref().map(|r| visitor.map_component_rest_param(r));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::statement::component_params::Params {
        loc: loc.dupe(),
        params: params_,
        rest: rest_,
        comments: comments_,
    }
}

pub fn component_param_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    param: &'ast ast::statement::component_params::Param<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::component_params::Param {
        loc: _,
        name,
        local,
        default,
        shorthand: _,
    } = param;
    visitor.component_param_name(name)?;
    visitor.component_param_pattern(local)?;
    visitor.default_opt(default.as_ref())?;
    Ok(())
}

pub fn map_component_param_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    param: &'ast ast::statement::component_params::Param<Loc, Loc>,
) -> ast::statement::component_params::Param<Loc, Loc> {
    let ast::statement::component_params::Param {
        loc,
        name,
        local,
        default,
        shorthand,
    } = param;

    let name_ = visitor.map_component_param_name(name);
    let local_ = visitor.map_pattern(None, local);
    let default_ = default.as_ref().map(|d| visitor.map_expression(d));

    ast::statement::component_params::Param {
        loc: loc.dupe(),
        name: name_,
        local: local_,
        default: default_,
        shorthand: *shorthand,
    }
}

pub fn component_param_name_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    param_name: &'ast ast::statement::component_params::ParamName<Loc, Type>,
) -> Result<(), E> {
    match param_name {
        ast::statement::component_params::ParamName::Identifier(ident) => {
            visitor.identifier(ident)?;
        }
        ast::statement::component_params::ParamName::StringLiteral((_loc, lit)) => {
            visitor.string_literal(lit)?;
        }
    }
    Ok(())
}

pub fn map_component_param_name_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    param_name: &'ast ast::statement::component_params::ParamName<Loc, Loc>,
) -> ast::statement::component_params::ParamName<Loc, Loc> {
    match param_name {
        ast::statement::component_params::ParamName::Identifier(id) => {
            ast::statement::component_params::ParamName::Identifier(visitor.map_identifier(id))
        }
        ast::statement::component_params::ParamName::StringLiteral((loc, lit)) => {
            ast::statement::component_params::ParamName::StringLiteral((
                loc.dupe(),
                visitor.map_string_literal(lit),
            ))
        }
    }
}

pub fn component_param_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::pattern::Pattern<Loc, Type>,
) -> Result<(), E> {
    visitor.binding_pattern(ast::VariableKind::Let, expr)?;
    Ok(())
}

pub fn map_component_param_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::pattern::Pattern<Loc, Loc>,
) -> ast::pattern::Pattern<Loc, Loc> {
    visitor.map_pattern(Some(ast::VariableKind::Let), expr)
}

pub fn component_rest_param_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::statement::component_params::RestParam<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::component_params::RestParam {
        loc: _,
        argument,
        comments,
    } = expr;
    visitor.component_param_pattern(argument)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_component_rest_param_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    rest: &'ast ast::statement::component_params::RestParam<Loc, Loc>,
) -> ast::statement::component_params::RestParam<Loc, Loc> {
    let ast::statement::component_params::RestParam {
        loc,
        argument,
        comments,
    } = rest;

    let argument_ = visitor.map_pattern(None, argument);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::statement::component_params::RestParam {
        loc: loc.dupe(),
        argument: argument_,
        comments: comments_,
    }
}

pub fn component_body_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    body: &'ast (Loc, ast::statement::Block<Loc, Type>),
) -> Result<(), E> {
    let (loc, block) = body;
    visitor.block(loc, block)?;
    Ok(())
}

pub fn map_component_body_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    body: &'ast (Loc, ast::statement::Block<Loc, Loc>),
) -> (Loc, ast::statement::Block<Loc, Loc>) {
    let (loc, block) = body;
    let block_ = visitor.map_block(loc, block);
    (loc.dupe(), block_)
}

pub fn conditional_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Type,
    expr: &'ast ast::expression::Conditional<Loc, Type>,
) -> Result<(), E> {
    let ast::expression::Conditional {
        test,
        consequent,
        alternate,
        comments,
    } = expr;
    visitor.predicate_expression(test)?;
    visitor.expression(consequent)?;
    visitor.expression(alternate)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_conditional_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::Conditional<Loc, Loc>,
) -> ast::expression::Conditional<Loc, Loc> {
    let ast::expression::Conditional {
        test,
        consequent,
        alternate,
        comments,
    } = expr;
    let test_ = visitor.map_expression(test);
    let consequent_ = visitor.map_expression(consequent);
    let alternate_ = visitor.map_expression(alternate);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::Conditional {
        test: test_,
        consequent: consequent_,
        alternate: alternate_,
        comments: comments_,
    }
}

pub fn continue_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    cont: &'ast ast::statement::Continue<Loc>,
) -> Result<(), E> {
    let ast::statement::Continue { label, comments } = cont;
    if let Some(label) = label {
        visitor.label_identifier(label)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_continue_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    cont: &'ast ast::statement::Continue<Loc>,
) -> ast::statement::Continue<Loc> {
    let ast::statement::Continue { label, comments } = cont;
    let label_ = label.as_ref().map(|l| visitor.map_identifier(l));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::Continue {
        label: label_,
        comments: comments_,
    }
}

pub fn debugger_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    dbg: &'ast ast::statement::Debugger<Loc>,
) -> Result<(), E> {
    let ast::statement::Debugger { comments } = dbg;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_debugger_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    dbg: &'ast ast::statement::Debugger<Loc>,
) -> ast::statement::Debugger<Loc> {
    let ast::statement::Debugger { comments } = dbg;
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::Debugger {
        comments: comments_,
    }
}

pub fn declare_class_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::DeclareClass<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::DeclareClass {
        id,
        tparams,
        body,
        extends,
        mixins,
        implements,
        abstract_: _,
        comments,
    } = decl;
    visitor.class_identifier(id)?;
    if let Some(tparams) = tparams {
        visitor.type_params(&TypeParamsContext::DeclareClass, tparams)?;
    }
    let (_loc, obj_type) = body;
    visitor.object_type(obj_type)?;
    if let Some((_loc, ext)) = extends {
        visit_declare_class_extends(visitor, ext)?;
    }
    for (_loc, generic) in mixins.iter() {
        visitor.generic_type(generic)?;
    }
    if let Some(implements) = implements {
        visitor.class_implements(implements)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

fn visit_declare_class_extends<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ext: &'ast ast::statement::DeclareClassExtends<Loc, Type>,
) -> Result<(), E> {
    match ext {
        ast::statement::DeclareClassExtends::ExtendsIdent(generic) => {
            visitor.generic_type(generic)?;
        }
        ast::statement::DeclareClassExtends::ExtendsCall {
            callee: (_callee_loc, callee),
            arg,
        } => {
            visitor.generic_type(callee)?;
            visit_declare_class_extends(visitor, &arg.1)?;
        }
    }
    Ok(())
}

fn map_declare_class_extends<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ext_loc: &'ast Loc,
    ext: &'ast ast::statement::DeclareClassExtends<Loc, Loc>,
) -> ast::statement::DeclareClassExtends<Loc, Loc> {
    match ext {
        ast::statement::DeclareClassExtends::ExtendsIdent(generic) => {
            let generic_ = visitor.map_generic_type(ext_loc, generic);
            ast::statement::DeclareClassExtends::ExtendsIdent(generic_)
        }
        ast::statement::DeclareClassExtends::ExtendsCall {
            callee: (callee_loc, callee),
            arg,
        } => {
            let callee_ = visitor.map_generic_type(callee_loc, callee);
            let arg_ = map_declare_class_extends(visitor, &arg.0, &arg.1);
            ast::statement::DeclareClassExtends::ExtendsCall {
                callee: (callee_loc.dupe(), callee_),
                arg: Box::new((arg.0.dupe(), arg_)),
            }
        }
    }
}

pub fn map_declare_class_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::DeclareClass<Loc, Loc>,
) -> ast::statement::DeclareClass<Loc, Loc> {
    let ast::statement::DeclareClass {
        id,
        tparams,
        body,
        extends,
        mixins,
        implements,
        abstract_,
        comments,
    } = decl;
    let id_ = visitor.map_class_identifier(id);
    let tparams_ = tparams
        .as_ref()
        .map(|tp| visitor.map_type_params(&TypeParamsContext::DeclareClass, tp));
    let (body_loc, obj_type) = body;
    let obj_type_ = visitor.map_object_type(body_loc, obj_type);
    let body_ = (body_loc.dupe(), obj_type_);
    let extends_ = extends.as_ref().map(|(ext_loc, ext)| {
        let ext_ = map_declare_class_extends(visitor, ext_loc, ext);
        (ext_loc.dupe(), ext_)
    });
    let mixins_ = mixins
        .iter()
        .map(|(loc, generic)| {
            let generic_ = visitor.map_generic_type(loc, generic);
            (loc.dupe(), generic_)
        })
        .collect::<Vec<_>>()
        .into();
    let implements_ = implements
        .as_ref()
        .map(|impl_| visitor.map_class_implements(impl_));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::DeclareClass {
        id: id_,
        tparams: tparams_,
        body: body_,
        extends: extends_,
        mixins: mixins_,
        implements: implements_,
        abstract_: *abstract_,
        comments: comments_,
    }
}

pub fn declare_component_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    decl: &'ast ast::statement::DeclareComponent<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::statement::DeclareComponent {
        id,
        tparams,
        params,
        renders,
        comments,
    } = decl;
    visitor.component_identifier(id)?;
    if let Some(tparams) = tparams {
        visitor.type_params(&TypeParamsContext::DeclareComponent, tparams)?;
    }
    visitor.component_params(params)?;
    visitor.component_renders_annotation(renders)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_declare_component_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::DeclareComponent<Loc, Loc>,
) -> ast::statement::DeclareComponent<Loc, Loc> {
    let ast::statement::DeclareComponent {
        id,
        tparams,
        params,
        renders,
        comments,
    } = decl;
    let id_ = visitor.map_component_identifier(id);
    let tparams_ = tparams
        .as_ref()
        .map(|tp| visitor.map_type_params(&TypeParamsContext::DeclareComponent, tp));
    let params_ = visitor.map_component_params(params);
    let renders_ = visitor.map_component_renders_annotation(renders);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::DeclareComponent {
        id: id_,
        tparams: tparams_,
        params: params_,
        renders: renders_,
        comments: comments_,
    }
}

pub fn component_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Type,
    component: &'ast ast::types::Component<Loc, Type>,
) -> Result<(), E> {
    let ast::types::Component {
        tparams,
        params,
        renders,
        comments,
    } = component;
    if let Some(tparams) = tparams {
        visitor.type_params(&TypeParamsContext::ComponentType, tparams)?;
    }
    visitor.component_type_params(params)?;
    visitor.component_renders_annotation(renders)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_component_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    component: &'ast ast::types::Component<Loc, Loc>,
) -> ast::types::Component<Loc, Loc> {
    let ast::types::Component {
        tparams,
        params,
        renders,
        comments,
    } = component;
    let tparams_ = tparams
        .as_ref()
        .map(|tp| visitor.map_type_params(&TypeParamsContext::ComponentType, tp));
    let params_ = visitor.map_component_type_params(params);
    let renders_ = visitor.map_component_renders_annotation(renders);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::Component {
        tparams: tparams_,
        params: params_,
        renders: renders_,
        comments: comments_,
    }
}

pub fn component_type_params_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    params: &'ast ast::types::component_params::Params<Loc, Type>,
) -> Result<(), E> {
    let ast::types::component_params::Params {
        loc: _,
        params: params_list,
        rest,
        comments,
    } = params;
    for param in params_list.iter() {
        visitor.component_type_param(param)?;
    }
    if let Some(rest) = rest {
        visitor.component_type_rest_param(rest)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_component_type_params_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    params: &'ast ast::types::component_params::Params<Loc, Loc>,
) -> ast::types::component_params::Params<Loc, Loc> {
    let ast::types::component_params::Params {
        loc,
        params: params_list,
        rest,
        comments,
    } = params;
    let params_: Arc<[ast::types::component_params::Param<Loc, Loc>]> = params_list
        .iter()
        .map(|param| visitor.map_component_type_param(param))
        .collect();
    let rest_ = rest
        .as_ref()
        .map(|r| visitor.map_component_type_rest_param(r));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::component_params::Params {
        loc: loc.dupe(),
        params: params_,
        rest: rest_,
        comments: comments_,
    }
}

pub fn component_type_param_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    param: &'ast ast::types::component_params::Param<Loc, Type>,
) -> Result<(), E> {
    let ast::types::component_params::Param {
        loc: _,
        name,
        annot,
        optional: _,
    } = param;
    visitor.component_param_name(name)?;
    visitor.type_annotation(annot)?;
    Ok(())
}

pub fn map_component_type_param_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    param: &'ast ast::types::component_params::Param<Loc, Loc>,
) -> ast::types::component_params::Param<Loc, Loc> {
    let ast::types::component_params::Param {
        loc,
        name,
        annot,
        optional,
    } = param;
    let name_ = visitor.map_component_param_name(name);
    let annot_ = visitor.map_type_annotation(annot);
    ast::types::component_params::Param {
        loc: loc.dupe(),
        name: name_,
        annot: annot_,
        optional: *optional,
    }
}

pub fn component_type_rest_param_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::types::component_params::RestParam<Loc, Type>,
) -> Result<(), E> {
    let ast::types::component_params::RestParam {
        loc: _,
        argument,
        annot,
        optional: _,
        comments,
    } = expr;
    if let Some(argument) = argument {
        visitor.identifier(argument)?;
    }
    visitor.type_(annot)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_component_type_rest_param_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::types::component_params::RestParam<Loc, Loc>,
) -> ast::types::component_params::RestParam<Loc, Loc> {
    let ast::types::component_params::RestParam {
        loc,
        argument,
        annot,
        optional,
        comments,
    } = expr;
    let argument_ = argument.as_ref().map(|arg| visitor.map_identifier(arg));
    let annot_ = visitor.map_type_(annot);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::component_params::RestParam {
        loc: loc.dupe(),
        argument: argument_,
        annot: annot_,
        optional: *optional,
        comments: comments_,
    }
}

pub fn declare_enum_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    enum_: &'ast ast::statement::EnumDeclaration<Loc, Type>,
) -> Result<(), E> {
    visitor.enum_declaration(loc, enum_)?;
    Ok(())
}

pub fn map_declare_enum_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    enum_: &'ast ast::statement::EnumDeclaration<Loc, Loc>,
) -> ast::statement::EnumDeclaration<Loc, Loc> {
    visitor.map_enum_declaration(loc, enum_)
}

pub fn declare_export_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::DeclareExportDeclaration<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::DeclareExportDeclaration {
        default: _,
        source,
        specifiers,
        declaration,
        comments,
    } = decl;
    if let Some((loc, src)) = source {
        visitor.export_source(loc, src)?;
    }
    if let Some(specifiers) = specifiers {
        visitor.export_named_specifier(specifiers)?;
    }
    if let Some(declaration) = declaration {
        visitor.declare_export_declaration_decl(declaration)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_declare_export_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::DeclareExportDeclaration<Loc, Loc>,
) -> ast::statement::DeclareExportDeclaration<Loc, Loc> {
    let ast::statement::DeclareExportDeclaration {
        default,
        source,
        specifiers,
        declaration,
        comments,
    } = decl;
    let source_ = source.as_ref().map(|(loc, src)| {
        let src_ = visitor.map_export_source(loc, src);
        (loc.dupe(), src_)
    });
    let specifiers_ = specifiers
        .as_ref()
        .map(|spec| visitor.map_export_named_specifier(spec));
    let declaration_ = declaration
        .as_ref()
        .map(|d| visitor.map_declare_export_declaration_decl(d));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::DeclareExportDeclaration {
        default: default.clone(),
        source: source_,
        specifiers: specifiers_,
        declaration: declaration_,
        comments: comments_,
    }
}

pub fn declare_export_declaration_decl_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    decl: &'ast ast::statement::declare_export_declaration::Declaration<Loc, Type>,
) -> Result<(), E> {
    match decl {
        ast::statement::declare_export_declaration::Declaration::Variable { loc, declaration } => {
            visitor.declare_variable(loc, declaration)?;
        }
        ast::statement::declare_export_declaration::Declaration::Function { loc, declaration } => {
            visitor.declare_function(loc, declaration)?;
        }
        ast::statement::declare_export_declaration::Declaration::Class { loc, declaration } => {
            visitor.declare_class(loc, declaration)?;
        }
        ast::statement::declare_export_declaration::Declaration::Component { loc, declaration } => {
            visitor.declare_component(loc, declaration)?;
        }
        ast::statement::declare_export_declaration::Declaration::DefaultType { type_ } => {
            visitor.type_(type_)?;
        }
        ast::statement::declare_export_declaration::Declaration::NamedType { loc, declaration } => {
            visitor.type_alias(loc, declaration)?;
        }
        ast::statement::declare_export_declaration::Declaration::NamedOpaqueType {
            loc,
            declaration,
        } => {
            visitor.opaque_type(loc, declaration)?;
        }
        ast::statement::declare_export_declaration::Declaration::Interface { loc, declaration } => {
            visitor.interface(loc, declaration)?;
        }
        ast::statement::declare_export_declaration::Declaration::Enum { loc, declaration } => {
            visitor.enum_declaration(loc, declaration)?;
        }
        ast::statement::declare_export_declaration::Declaration::Namespace { loc, declaration } => {
            visitor.declare_namespace(loc, declaration)?;
        }
    }
    Ok(())
}

pub fn map_declare_export_declaration_decl_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    decl: &'ast ast::statement::declare_export_declaration::Declaration<Loc, Loc>,
) -> ast::statement::declare_export_declaration::Declaration<Loc, Loc> {
    match decl {
        ast::statement::declare_export_declaration::Declaration::Variable { loc, declaration } => {
            let declaration_ = visitor.map_declare_variable(loc, declaration);
            ast::statement::declare_export_declaration::Declaration::Variable {
                loc: loc.dupe(),
                declaration: Arc::new(declaration_),
            }
        }
        ast::statement::declare_export_declaration::Declaration::Function { loc, declaration } => {
            let declaration_ = visitor.map_declare_function(loc, declaration);
            ast::statement::declare_export_declaration::Declaration::Function {
                loc: loc.dupe(),
                declaration: Arc::new(declaration_),
            }
        }
        ast::statement::declare_export_declaration::Declaration::Class { loc, declaration } => {
            let declaration_ = visitor.map_declare_class(loc, declaration);
            ast::statement::declare_export_declaration::Declaration::Class {
                loc: loc.dupe(),
                declaration: Arc::new(declaration_),
            }
        }
        ast::statement::declare_export_declaration::Declaration::Component { loc, declaration } => {
            let declaration_ = visitor.map_declare_component(loc, declaration);
            ast::statement::declare_export_declaration::Declaration::Component {
                loc: loc.dupe(),
                declaration: Arc::new(declaration_),
            }
        }
        ast::statement::declare_export_declaration::Declaration::DefaultType { type_ } => {
            let type__ = visitor.map_type_(type_);
            ast::statement::declare_export_declaration::Declaration::DefaultType {
                type_: Arc::new(type__),
            }
        }
        ast::statement::declare_export_declaration::Declaration::NamedType { loc, declaration } => {
            let declaration_ = visitor.map_type_alias(loc, declaration);
            ast::statement::declare_export_declaration::Declaration::NamedType {
                loc: loc.dupe(),
                declaration: Arc::new(declaration_),
            }
        }
        ast::statement::declare_export_declaration::Declaration::NamedOpaqueType {
            loc,
            declaration,
        } => {
            let declaration_ = visitor.map_opaque_type(loc, declaration);
            ast::statement::declare_export_declaration::Declaration::NamedOpaqueType {
                loc: loc.dupe(),
                declaration: Arc::new(declaration_),
            }
        }
        ast::statement::declare_export_declaration::Declaration::Interface { loc, declaration } => {
            let declaration_ = visitor.map_interface_declaration(loc, declaration);
            ast::statement::declare_export_declaration::Declaration::Interface {
                loc: loc.dupe(),
                declaration: Arc::new(declaration_),
            }
        }
        ast::statement::declare_export_declaration::Declaration::Enum { loc, declaration } => {
            let declaration_ = visitor.map_enum_declaration(loc, declaration);
            ast::statement::declare_export_declaration::Declaration::Enum {
                loc: loc.dupe(),
                declaration: Arc::new(declaration_),
            }
        }
        ast::statement::declare_export_declaration::Declaration::Namespace { loc, declaration } => {
            let declaration_ = visitor.map_declare_namespace(loc, declaration);
            ast::statement::declare_export_declaration::Declaration::Namespace {
                loc: loc.dupe(),
                declaration: Box::new(declaration_),
            }
        }
    }
}

pub fn declare_function_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::DeclareFunction<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::DeclareFunction {
        id,
        annot,
        predicate,
        comments,
        implicit_declare: _,
    } = decl;
    if let Some(id) = id {
        visitor.function_identifier(id)?;
    }
    visitor.type_annotation(annot)?;
    if let Some(predicate) = predicate {
        visitor.predicate(predicate)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_declare_function_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::DeclareFunction<Loc, Loc>,
) -> ast::statement::DeclareFunction<Loc, Loc> {
    let ast::statement::DeclareFunction {
        id,
        annot,
        predicate,
        comments,
        implicit_declare,
    } = decl;
    let id_ = id.as_ref().map(|id| visitor.map_function_identifier(id));
    let annot_ = visitor.map_type_annotation(annot);
    let predicate_ = predicate.as_ref().map(|p| visitor.map_predicate(p));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::DeclareFunction {
        id: id_,
        annot: annot_,
        predicate: predicate_,
        comments: comments_,
        implicit_declare: *implicit_declare,
    }
}

pub fn declare_interface_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    decl: &'ast ast::statement::Interface<Loc, Type>,
) -> Result<(), E> {
    visitor.interface(loc, decl)?;
    Ok(())
}

pub fn map_declare_interface_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    decl: &'ast ast::statement::Interface<Loc, Loc>,
) -> ast::statement::Interface<Loc, Loc> {
    visitor.map_interface(loc, decl)
}

pub fn declare_module_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    m: &'ast ast::statement::DeclareModule<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::DeclareModule {
        id: _,
        body,
        comments,
    } = m;
    let (loc, block) = body;
    visitor.block(loc, block)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_declare_module_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    m: &'ast ast::statement::DeclareModule<Loc, Loc>,
) -> ast::statement::DeclareModule<Loc, Loc> {
    let ast::statement::DeclareModule { id, body, comments } = m;
    let id_ = match id {
        ast::statement::declare_module::Id::Identifier(ident) => {
            ast::statement::declare_module::Id::Identifier(visitor.map_identifier(ident))
        }
        ast::statement::declare_module::Id::Literal((loc, lit)) => {
            ast::statement::declare_module::Id::Literal((
                loc.dupe(),
                visitor.map_string_literal(lit),
            ))
        }
    };
    let (body_loc, block) = body;
    let block_ = visitor.map_block(body_loc, block);
    let body_ = (body_loc.dupe(), block_);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::DeclareModule {
        id: id_,
        body: body_,
        comments: comments_,
    }
}

pub fn declare_module_exports_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    exports: &'ast ast::statement::DeclareModuleExports<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::DeclareModuleExports { annot, comments } = exports;
    visitor.type_annotation(annot)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_declare_module_exports_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    exports: &'ast ast::statement::DeclareModuleExports<Loc, Loc>,
) -> ast::statement::DeclareModuleExports<Loc, Loc> {
    let ast::statement::DeclareModuleExports { annot, comments } = exports;
    let annot_ = visitor.map_type_annotation(annot);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::DeclareModuleExports {
        annot: annot_,
        comments: comments_,
    }
}

pub fn declare_namespace_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    m: &'ast ast::statement::DeclareNamespace<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::DeclareNamespace {
        id,
        body,
        implicit_declare: _,
        keyword: _,
        comments,
    } = m;
    match id {
        ast::statement::declare_namespace::Id::Global(g_id) => {
            visitor.untyped_identifier(g_id)?;
        }
        ast::statement::declare_namespace::Id::Local(p_id) => {
            visitor.pattern_identifier(Some(ast::VariableKind::Const), p_id)?;
        }
    }
    let (loc, block) = body;
    visitor.block(loc, block)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_declare_namespace_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    ns: &'ast ast::statement::DeclareNamespace<Loc, Loc>,
) -> ast::statement::DeclareNamespace<Loc, Loc> {
    let ast::statement::DeclareNamespace {
        id,
        body,
        implicit_declare,
        keyword,
        comments,
    } = ns;
    let id_ = match id {
        ast::statement::declare_namespace::Id::Global(g_id) => {
            let g_id_ = visitor.map_identifier(g_id);
            ast::statement::declare_namespace::Id::Global(g_id_)
        }
        ast::statement::declare_namespace::Id::Local(p_id) => {
            let p_id_ = visitor.map_identifier(p_id);
            ast::statement::declare_namespace::Id::Local(p_id_)
        }
    };
    let (body_loc, block) = body;
    let block_ = visitor.map_block(body_loc, block);
    let body_ = (body_loc.dupe(), block_);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::DeclareNamespace {
        id: id_,
        body: body_,
        implicit_declare: *implicit_declare,
        keyword: *keyword,
        comments: comments_,
    }
}

pub fn declare_type_alias_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    decl: &'ast ast::statement::TypeAlias<Loc, Type>,
) -> Result<(), E> {
    visitor.type_alias(loc, decl)?;
    Ok(())
}

pub fn map_declare_type_alias_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    decl: &'ast ast::statement::TypeAlias<Loc, Loc>,
) -> ast::statement::TypeAlias<Loc, Loc> {
    visitor.map_type_alias(loc, decl)
}

pub fn declare_variable_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::DeclareVariable<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::DeclareVariable {
        declarations,
        kind,
        comments,
    } = decl;
    for declarator in declarations.iter() {
        visitor.declare_variable_declarator(*kind, declarator)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_declare_variable_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::DeclareVariable<Loc, Loc>,
) -> ast::statement::DeclareVariable<Loc, Loc> {
    let ast::statement::DeclareVariable {
        declarations,
        kind,
        comments,
    } = decl;
    let declarations_ = Arc::from(
        declarations
            .iter()
            .map(|d| visitor.map_declare_variable_declarator(*kind, d))
            .collect::<Vec<_>>(),
    );
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::DeclareVariable {
        declarations: declarations_,
        kind: *kind,
        comments: comments_,
    }
}

pub fn declare_variable_declarator_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: ast::VariableKind,
    decl: &'ast ast::statement::variable::Declarator<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::variable::Declarator {
        loc: _,
        id,
        init,
        definite: _,
    } = decl;
    match id {
        ast::pattern::Pattern::Identifier { inner, .. } => {
            visitor.pattern_identifier(Some(kind), &inner.name)?;
            visitor.type_annotation_hint(&inner.annot)?;
        }
        _ => {
            visitor.pattern(None, id)?;
        }
    }
    if let Some(init_expr) = init {
        visitor.expression(init_expr)?;
    }
    Ok(())
}

pub fn map_declare_variable_declarator_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: ast::VariableKind,
    decl: &'ast ast::statement::variable::Declarator<Loc, Loc>,
) -> ast::statement::variable::Declarator<Loc, Loc> {
    let ast::statement::variable::Declarator {
        loc,
        id,
        init,
        definite,
    } = decl;
    let id_ = match id {
        ast::pattern::Pattern::Identifier { loc: ploc, inner } => {
            let ident_ = visitor.map_pattern_identifier(Some(kind), inner);
            ast::pattern::Pattern::Identifier {
                loc: ploc.dupe(),
                inner: Arc::new(ident_),
            }
        }
        _ => id.clone(),
    };
    let init_ = init.as_ref().map(|i| visitor.map_expression(i));
    ast::statement::variable::Declarator {
        loc: loc.dupe(),
        id: id_,
        init: init_,
        definite: *definite,
    }
}

pub fn do_while_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    stmt: &'ast ast::statement::DoWhile<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::DoWhile {
        body,
        test,
        comments,
    } = stmt;
    visitor.statement(body)?;
    visitor.predicate_expression(test)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_do_while_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    stmt: &'ast ast::statement::DoWhile<Loc, Loc>,
) -> ast::statement::DoWhile<Loc, Loc> {
    let ast::statement::DoWhile {
        body,
        test,
        comments,
    } = stmt;
    let body_ = visitor.map_statement(body);
    let test_ = visitor.map_expression(test);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::DoWhile {
        body: body_,
        test: test_,
        comments: comments_,
    }
}

pub fn empty_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    empty: &'ast ast::statement::Empty<Loc>,
) -> Result<(), E> {
    let ast::statement::Empty { comments } = empty;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_empty_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    empty: &'ast ast::statement::Empty<Loc>,
) -> ast::statement::Empty<Loc> {
    let ast::statement::Empty { comments } = empty;
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::Empty {
        comments: comments_,
    }
}

pub fn enum_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    enum_: &'ast ast::statement::EnumDeclaration<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::EnumDeclaration {
        id,
        body,
        const_: _,
        comments,
    } = enum_;
    visitor.pattern_identifier(Some(ast::VariableKind::Const), id)?;
    visitor.enum_body(body)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_enum_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    enum_: &'ast ast::statement::EnumDeclaration<Loc, Loc>,
) -> ast::statement::EnumDeclaration<Loc, Loc> {
    let ast::statement::EnumDeclaration {
        id,
        body,
        const_,
        comments,
    } = enum_;
    let id_ = visitor.map_identifier(id);
    let body_ = visitor.map_enum_body(body);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::EnumDeclaration {
        id: id_,
        body: body_,
        const_: *const_,
        comments: comments_,
    }
}

pub fn enum_body_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    body: &'ast ast::statement::enum_declaration::Body<Loc>,
) -> Result<(), E> {
    let ast::statement::enum_declaration::Body {
        loc: _,
        members,
        explicit_type: _,
        has_unknown_members: _,
        comments,
    } = body;
    for member in members.iter() {
        visitor.enum_member(member)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_enum_body_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    body: &'ast ast::statement::enum_declaration::Body<Loc>,
) -> ast::statement::enum_declaration::Body<Loc> {
    let ast::statement::enum_declaration::Body {
        loc,
        members,
        explicit_type,
        has_unknown_members,
        comments,
    } = body;
    let members_ = Arc::from(
        members
            .iter()
            .map(|m| visitor.map_enum_member(m))
            .collect::<Vec<_>>(),
    );
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::enum_declaration::Body {
        loc: loc.dupe(),
        members: members_,
        explicit_type: explicit_type.as_ref().map(|(l, t)| (l.dupe(), *t)),
        has_unknown_members: has_unknown_members.as_ref().map(|l| l.dupe()),
        comments: comments_,
    }
}

pub fn enum_member_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    member: &'ast ast::statement::enum_declaration::Member<Loc>,
) -> Result<(), E> {
    match member {
        ast::statement::enum_declaration::Member::BooleanMember(m) => {
            visitor.enum_boolean_member(m)?;
        }
        ast::statement::enum_declaration::Member::NumberMember(m) => {
            visitor.enum_number_member(m)?;
        }
        ast::statement::enum_declaration::Member::StringMember(m) => {
            visitor.enum_string_member(m)?;
        }
        ast::statement::enum_declaration::Member::BigIntMember(m) => {
            visitor.enum_bigint_member(m)?;
        }
        ast::statement::enum_declaration::Member::DefaultedMember(m) => {
            visitor.enum_defaulted_member(m)?;
        }
    }
    Ok(())
}

pub fn map_enum_member_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    member: &'ast ast::statement::enum_declaration::Member<Loc>,
) -> ast::statement::enum_declaration::Member<Loc> {
    match member {
        ast::statement::enum_declaration::Member::BooleanMember(m) => {
            ast::statement::enum_declaration::Member::BooleanMember(
                visitor.map_enum_boolean_member(m),
            )
        }
        ast::statement::enum_declaration::Member::NumberMember(m) => {
            ast::statement::enum_declaration::Member::NumberMember(
                visitor.map_enum_number_member(m),
            )
        }
        ast::statement::enum_declaration::Member::StringMember(m) => {
            ast::statement::enum_declaration::Member::StringMember(
                visitor.map_enum_string_member(m),
            )
        }
        ast::statement::enum_declaration::Member::BigIntMember(m) => {
            ast::statement::enum_declaration::Member::BigIntMember(
                visitor.map_enum_bigint_member(m),
            )
        }
        ast::statement::enum_declaration::Member::DefaultedMember(m) => {
            ast::statement::enum_declaration::Member::DefaultedMember(
                visitor.map_enum_defaulted_member(m),
            )
        }
    }
}

pub fn enum_defaulted_member_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    member: &'ast ast::statement::enum_declaration::DefaultedMember<Loc>,
) -> Result<(), E> {
    let ast::statement::enum_declaration::DefaultedMember { loc: _, id } = member;
    visitor.enum_member_name(id)?;
    Ok(())
}

pub fn map_enum_defaulted_member_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    member: &'ast ast::statement::enum_declaration::DefaultedMember<Loc>,
) -> ast::statement::enum_declaration::DefaultedMember<Loc> {
    let ast::statement::enum_declaration::DefaultedMember { loc, id } = member;
    let id_ = visitor.map_enum_member_name(id);
    ast::statement::enum_declaration::DefaultedMember {
        loc: loc.dupe(),
        id: id_,
    }
}

pub fn enum_boolean_member_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    member: &'ast ast::statement::enum_declaration::InitializedMember<
        ast::BooleanLiteral<Loc>,
        Loc,
    >,
) -> Result<(), E> {
    let ast::statement::enum_declaration::InitializedMember {
        loc: _,
        id,
        init: (_, init_val),
    } = member;
    visitor.enum_member_name(id)?;
    visitor.boolean_literal(init_val)?;
    Ok(())
}

pub fn map_enum_boolean_member_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    member: &'ast ast::statement::enum_declaration::InitializedMember<
        ast::BooleanLiteral<Loc>,
        Loc,
    >,
) -> ast::statement::enum_declaration::InitializedMember<ast::BooleanLiteral<Loc>, Loc> {
    let ast::statement::enum_declaration::InitializedMember { loc, id, init } = member;
    let id_ = visitor.map_enum_member_name(id);
    let (init_loc, init_lit) = init;
    let init_lit_ = visitor.map_boolean_literal(init_lit);
    ast::statement::enum_declaration::InitializedMember {
        loc: loc.dupe(),
        id: id_,
        init: (init_loc.dupe(), init_lit_),
    }
}

pub fn enum_number_member_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    member: &'ast ast::statement::enum_declaration::InitializedMember<ast::NumberLiteral<Loc>, Loc>,
) -> Result<(), E> {
    let ast::statement::enum_declaration::InitializedMember {
        loc: _,
        id,
        init: (_, init_val),
    } = member;
    visitor.enum_member_name(id)?;
    visitor.number_literal(init_val)?;
    Ok(())
}

pub fn map_enum_number_member_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    member: &'ast ast::statement::enum_declaration::InitializedMember<ast::NumberLiteral<Loc>, Loc>,
) -> ast::statement::enum_declaration::InitializedMember<ast::NumberLiteral<Loc>, Loc> {
    let ast::statement::enum_declaration::InitializedMember { loc, id, init } = member;
    let id_ = visitor.map_enum_member_name(id);
    let (init_loc, init_lit) = init;
    let init_lit_ = visitor.map_number_literal(init_lit);
    ast::statement::enum_declaration::InitializedMember {
        loc: loc.dupe(),
        id: id_,
        init: (init_loc.dupe(), init_lit_),
    }
}

pub fn enum_string_member_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    member: &'ast ast::statement::enum_declaration::InitializedMember<ast::StringLiteral<Loc>, Loc>,
) -> Result<(), E> {
    let ast::statement::enum_declaration::InitializedMember {
        loc: _,
        id,
        init: (_, init_val),
    } = member;
    visitor.enum_member_name(id)?;
    visitor.string_literal(init_val)?;
    Ok(())
}

pub fn map_enum_string_member_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    member: &'ast ast::statement::enum_declaration::InitializedMember<ast::StringLiteral<Loc>, Loc>,
) -> ast::statement::enum_declaration::InitializedMember<ast::StringLiteral<Loc>, Loc> {
    let ast::statement::enum_declaration::InitializedMember { loc, id, init } = member;
    let id_ = visitor.map_enum_member_name(id);
    let (init_loc, init_lit) = init;
    let init_lit_ = visitor.map_string_literal(init_lit);
    ast::statement::enum_declaration::InitializedMember {
        loc: loc.dupe(),
        id: id_,
        init: (init_loc.dupe(), init_lit_),
    }
}

pub fn enum_bigint_member_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    member: &'ast ast::statement::enum_declaration::InitializedMember<ast::BigIntLiteral<Loc>, Loc>,
) -> Result<(), E> {
    let ast::statement::enum_declaration::InitializedMember {
        loc: _,
        id,
        init: (_, init_val),
    } = member;
    visitor.enum_member_name(id)?;
    visitor.bigint_literal(init_val)?;
    Ok(())
}

pub fn map_enum_bigint_member_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    member: &'ast ast::statement::enum_declaration::InitializedMember<ast::BigIntLiteral<Loc>, Loc>,
) -> ast::statement::enum_declaration::InitializedMember<ast::BigIntLiteral<Loc>, Loc> {
    let ast::statement::enum_declaration::InitializedMember { loc, id, init } = member;
    let id_ = visitor.map_enum_member_name(id);
    let (init_loc, init_lit) = init;
    let init_lit_ = visitor.map_bigint_literal(init_lit);
    ast::statement::enum_declaration::InitializedMember {
        loc: loc.dupe(),
        id: id_,
        init: (init_loc.dupe(), init_lit_),
    }
}

pub fn enum_member_name_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::statement::enum_declaration::MemberName<Loc>,
) -> Result<(), E> {
    match id {
        ast::statement::enum_declaration::MemberName::Identifier(ident) => {
            visitor.enum_member_identifier(ident)?;
        }
        ast::statement::enum_declaration::MemberName::StringLiteral(_, lit) => {
            visitor.string_literal(lit)?;
        }
    }
    Ok(())
}

pub fn map_enum_member_name_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::statement::enum_declaration::MemberName<Loc>,
) -> ast::statement::enum_declaration::MemberName<Loc> {
    match id {
        ast::statement::enum_declaration::MemberName::Identifier(ident) => {
            ast::statement::enum_declaration::MemberName::Identifier(
                visitor.map_enum_member_identifier(ident),
            )
        }
        ast::statement::enum_declaration::MemberName::StringLiteral(loc, lit) => {
            ast::statement::enum_declaration::MemberName::StringLiteral(
                loc.dupe(),
                visitor.map_string_literal(lit),
            )
        }
    }
}

pub fn enum_member_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Loc>,
) -> Result<(), E> {
    visitor.untyped_identifier(id)?;
    Ok(())
}

pub fn map_enum_member_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    visitor.map_identifier(id)
}

pub fn export_default_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::ExportDefaultDeclaration<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::ExportDefaultDeclaration {
        default: _,
        declaration,
        comments,
    } = decl;
    visitor.export_default_declaration_decl(declaration)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_export_default_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::ExportDefaultDeclaration<Loc, Loc>,
) -> ast::statement::ExportDefaultDeclaration<Loc, Loc> {
    let ast::statement::ExportDefaultDeclaration {
        default,
        declaration,
        comments,
    } = decl;
    let declaration_ = visitor.map_export_default_declaration_decl(declaration);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::ExportDefaultDeclaration {
        default: default.clone(),
        declaration: declaration_,
        comments: comments_,
    }
}

pub fn export_default_declaration_decl_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    decl: &'ast ast::statement::export_default_declaration::Declaration<Loc, Type>,
) -> Result<(), E> {
    match decl {
        ast::statement::export_default_declaration::Declaration::Declaration(stmt) => {
            visitor.statement(stmt)?;
        }
        ast::statement::export_default_declaration::Declaration::Expression(expr) => {
            visitor.expression(expr)?;
        }
    }
    Ok(())
}

pub fn map_export_default_declaration_decl_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    decl: &'ast ast::statement::export_default_declaration::Declaration<Loc, Loc>,
) -> ast::statement::export_default_declaration::Declaration<Loc, Loc> {
    match decl {
        ast::statement::export_default_declaration::Declaration::Declaration(stmt) => {
            let stmt_ = visitor.map_statement(stmt);
            ast::statement::export_default_declaration::Declaration::Declaration(stmt_)
        }
        ast::statement::export_default_declaration::Declaration::Expression(expr) => {
            let expr_ = visitor.map_expression(expr);
            ast::statement::export_default_declaration::Declaration::Expression(expr_)
        }
    }
}

pub fn export_named_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::ExportNamedDeclaration<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::ExportNamedDeclaration {
        export_kind: _,
        source,
        specifiers,
        declaration,
        attributes,
        comments,
    } = decl;
    if let Some((loc, src)) = source {
        visitor.export_source(loc, src)?;
    }
    if let Some(specifiers) = specifiers {
        visitor.export_named_specifier(specifiers)?;
    }
    if let Some(declaration) = declaration {
        visitor.statement(declaration)?;
    }
    if let Some((loc, attributes)) = attributes {
        visitor.import_attributes(loc, attributes)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_export_named_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::ExportNamedDeclaration<Loc, Loc>,
) -> ast::statement::ExportNamedDeclaration<Loc, Loc> {
    let ast::statement::ExportNamedDeclaration {
        export_kind,
        source,
        specifiers,
        declaration,
        attributes,
        comments,
    } = decl;
    let source_ = source.as_ref().map(|(loc, src)| {
        let src_ = visitor.map_export_source(loc, src);
        (loc.dupe(), src_)
    });
    let specifiers_ = specifiers
        .as_ref()
        .map(|spec| visitor.map_export_named_specifier(spec));
    let declaration_ = declaration.as_ref().map(|d| visitor.map_statement(d));
    let attributes_ = attributes
        .as_ref()
        .map(|(loc, attrs)| (loc.dupe(), visitor.map_import_attributes(loc, attrs)));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::ExportNamedDeclaration {
        export_kind: *export_kind,
        source: source_,
        specifiers: specifiers_,
        declaration: declaration_,
        attributes: attributes_,
        comments: comments_,
    }
}

pub fn export_assignment_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    assign: &'ast ast::statement::ExportAssignment<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::ExportAssignment { rhs, comments } = assign;
    match rhs {
        ast::statement::ExportAssignmentRhs::Expression(expr) => {
            visitor.expression(expr)?;
        }
        ast::statement::ExportAssignmentRhs::DeclareFunction(loc, decl) => {
            visitor.declare_function(loc, decl)?;
        }
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_export_assignment_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    assign: &'ast ast::statement::ExportAssignment<Loc, Loc>,
) -> ast::statement::ExportAssignment<Loc, Loc> {
    let ast::statement::ExportAssignment { rhs, comments } = assign;
    let rhs_ = match rhs {
        ast::statement::ExportAssignmentRhs::Expression(expr) => {
            ast::statement::ExportAssignmentRhs::Expression(visitor.map_expression(expr))
        }
        ast::statement::ExportAssignmentRhs::DeclareFunction(loc, decl) => {
            ast::statement::ExportAssignmentRhs::DeclareFunction(
                loc.dupe(),
                visitor.map_declare_function(loc, decl),
            )
        }
    };
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::ExportAssignment {
        rhs: rhs_,
        comments: comments_,
    }
}

pub fn namespace_export_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::NamespaceExportDeclaration<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::NamespaceExportDeclaration { id, comments } = decl;
    visitor.identifier(id)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_namespace_export_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::NamespaceExportDeclaration<Loc, Loc>,
) -> ast::statement::NamespaceExportDeclaration<Loc, Loc> {
    let ast::statement::NamespaceExportDeclaration { id, comments } = decl;
    let id_ = visitor.map_identifier(id);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::NamespaceExportDeclaration {
        id: id_,
        comments: comments_,
    }
}

pub fn export_named_declaration_specifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    spec: &'ast ast::statement::export_named_declaration::ExportSpecifier<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::export_named_declaration::ExportSpecifier {
        loc: _,
        local,
        exported,
        export_kind: _,
        from_remote: _,
        imported_name_def_loc: _,
    } = spec;
    visitor.identifier(local)?;
    if let Some(exported) = exported {
        visitor.identifier(exported)?;
    }
    Ok(())
}

pub fn map_export_named_declaration_specifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    spec: &'ast ast::statement::export_named_declaration::ExportSpecifier<Loc, Loc>,
) -> ast::statement::export_named_declaration::ExportSpecifier<Loc, Loc> {
    let ast::statement::export_named_declaration::ExportSpecifier {
        loc,
        local,
        exported,
        export_kind,
        from_remote,
        imported_name_def_loc,
    } = spec;
    let local_ = visitor.map_identifier(local);
    let exported_ = exported.as_ref().map(|e| visitor.map_identifier(e));
    ast::statement::export_named_declaration::ExportSpecifier {
        loc: loc.dupe(),
        local: local_,
        exported: exported_,
        export_kind: *export_kind,
        from_remote: *from_remote,
        imported_name_def_loc: imported_name_def_loc.dupe(),
    }
}

pub fn export_batch_specifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    spec: &'ast ast::statement::export_named_declaration::ExportBatchSpecifier<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::export_named_declaration::ExportBatchSpecifier { loc: _, specifier } = spec;
    if let Some(id) = specifier {
        visitor.identifier(id)?;
    }
    Ok(())
}

pub fn map_export_batch_specifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    spec: &'ast ast::statement::export_named_declaration::ExportBatchSpecifier<Loc, Loc>,
) -> ast::statement::export_named_declaration::ExportBatchSpecifier<Loc, Loc> {
    let ast::statement::export_named_declaration::ExportBatchSpecifier { loc, specifier } = spec;
    let specifier_ = specifier.as_ref().map(|s| visitor.map_identifier(s));
    ast::statement::export_named_declaration::ExportBatchSpecifier {
        loc: loc.dupe(),
        specifier: specifier_,
    }
}

pub fn export_named_specifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    spec: &'ast ast::statement::export_named_declaration::Specifier<Loc, Type>,
) -> Result<(), E> {
    match spec {
        ast::statement::export_named_declaration::Specifier::ExportSpecifiers(spec_list) => {
            for spec in spec_list {
                visitor.export_named_declaration_specifier(spec)?;
            }
        }
        ast::statement::export_named_declaration::Specifier::ExportBatchSpecifier(batch) => {
            visitor.export_batch_specifier(batch)?;
        }
    }
    Ok(())
}

pub fn map_export_named_specifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    spec: &'ast ast::statement::export_named_declaration::Specifier<Loc, Loc>,
) -> ast::statement::export_named_declaration::Specifier<Loc, Loc> {
    match spec {
        ast::statement::export_named_declaration::Specifier::ExportSpecifiers(spec_list) => {
            let spec_list_ = spec_list
                .iter()
                .map(|s| visitor.map_export_named_declaration_specifier(s))
                .collect();
            ast::statement::export_named_declaration::Specifier::ExportSpecifiers(spec_list_)
        }
        ast::statement::export_named_declaration::Specifier::ExportBatchSpecifier(batch) => {
            let batch_ = visitor.map_export_batch_specifier(batch);
            ast::statement::export_named_declaration::Specifier::ExportBatchSpecifier(batch_)
        }
    }
}

pub fn export_source_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Type,
    source: &'ast ast::StringLiteral<Loc>,
) -> Result<(), E> {
    let ast::StringLiteral {
        value: _,
        raw: _,
        comments,
    } = source;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_export_source_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    source: &'ast ast::StringLiteral<Loc>,
) -> ast::StringLiteral<Loc> {
    let ast::StringLiteral {
        value,
        raw,
        comments,
    } = source;
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::StringLiteral {
        value: value.clone(),
        raw: raw.clone(),
        comments: comments_,
    }
}

pub fn expression_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    stmt: &'ast ast::statement::Expression<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::Expression {
        expression,
        directive: _,
        comments,
    } = stmt;
    visitor.expression(expression)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_expression_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    stmt: &'ast ast::statement::Expression<Loc, Loc>,
) -> ast::statement::Expression<Loc, Loc> {
    let ast::statement::Expression {
        expression,
        directive,
        comments,
    } = stmt;
    let expression_ = visitor.map_expression(expression);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::Expression {
        expression: expression_,
        directive: directive.clone(),
        comments: comments_,
    }
}

pub fn expression_or_spread_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr_or_spread: &'ast ast::expression::ExpressionOrSpread<Loc, Type>,
) -> Result<(), E> {
    match expr_or_spread {
        ast::expression::ExpressionOrSpread::Expression(expr) => {
            visitor.expression(expr)?;
        }
        ast::expression::ExpressionOrSpread::Spread(spread) => {
            visitor.spread_element(spread)?;
        }
    }
    Ok(())
}

pub fn map_expression_or_spread_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    eos: &'ast ast::expression::ExpressionOrSpread<Loc, Loc>,
) -> ast::expression::ExpressionOrSpread<Loc, Loc> {
    match eos {
        ast::expression::ExpressionOrSpread::Expression(expr) => {
            ast::expression::ExpressionOrSpread::Expression(visitor.map_expression(expr))
        }
        ast::expression::ExpressionOrSpread::Spread(spread) => {
            ast::expression::ExpressionOrSpread::Spread(visitor.map_spread_element(spread))
        }
    }
}

pub fn for_in_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    stmt: &'ast ast::statement::ForIn<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::ForIn {
        left,
        right,
        body,
        comments,
    } = stmt;
    visitor.for_in_statement_lhs(left)?;
    visitor.expression(right)?;
    visitor.statement(body)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_for_in_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    stmt: &'ast ast::statement::ForIn<Loc, Loc>,
) -> ast::statement::ForIn<Loc, Loc> {
    let ast::statement::ForIn {
        left,
        right,
        body,
        comments,
    } = stmt;
    let left_ = visitor.map_for_in_statement_lhs(left);
    let right_ = visitor.map_expression(right);
    let body_ = visitor.map_statement(body);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::ForIn {
        left: left_,
        right: right_,
        body: body_,
        comments: comments_,
    }
}

pub fn for_in_statement_lhs_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    left: &'ast ast::statement::for_in::Left<Loc, Type>,
) -> Result<(), E> {
    match left {
        ast::statement::for_in::Left::LeftDeclaration(decl) => {
            visitor.for_in_left_declaration(decl)?;
        }
        ast::statement::for_in::Left::LeftPattern(pattern) => {
            visitor.for_in_assignment_pattern(pattern)?;
        }
    }
    Ok(())
}

pub fn map_for_in_statement_lhs_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    lhs: &'ast ast::statement::for_in::Left<Loc, Loc>,
) -> ast::statement::for_in::Left<Loc, Loc> {
    match lhs {
        ast::statement::for_in::Left::LeftDeclaration((loc, decl)) => {
            let decl_ = visitor.map_variable_declaration(loc, decl);
            ast::statement::for_in::Left::LeftDeclaration((loc.dupe(), decl_))
        }
        ast::statement::for_in::Left::LeftPattern(pat) => {
            ast::statement::for_in::Left::LeftPattern(visitor.map_pattern(None, pat))
        }
    }
}

pub fn for_in_left_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    left: &'ast (Loc, ast::statement::VariableDeclaration<Loc, Type>),
) -> Result<(), E> {
    let (loc, decl) = left;
    visitor.variable_declaration(loc, decl)?;
    Ok(())
}

pub fn map_for_in_left_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    left: &'ast (Loc, ast::statement::VariableDeclaration<Loc, Loc>),
) -> (Loc, ast::statement::VariableDeclaration<Loc, Loc>) {
    let (loc, decl) = left;
    (loc.dupe(), visitor.map_variable_declaration(loc, decl))
}

pub fn for_of_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    stmt: &'ast ast::statement::ForOf<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::ForOf {
        left,
        right,
        body,
        await_: _,
        comments,
    } = stmt;
    visitor.for_of_statement_lhs(left)?;
    visitor.expression(right)?;
    visitor.statement(body)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_for_of_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    stmt: &'ast ast::statement::ForOf<Loc, Loc>,
) -> ast::statement::ForOf<Loc, Loc> {
    let ast::statement::ForOf {
        left,
        right,
        body,
        await_,
        comments,
    } = stmt;
    let left_ = visitor.map_for_of_statement_lhs(left);
    let right_ = visitor.map_expression(right);
    let body_ = visitor.map_statement(body);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::ForOf {
        left: left_,
        right: right_,
        body: body_,
        await_: *await_,
        comments: comments_,
    }
}

pub fn for_of_statement_lhs_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    left: &'ast ast::statement::for_of::Left<Loc, Type>,
) -> Result<(), E> {
    match left {
        ast::statement::for_of::Left::LeftDeclaration(decl) => {
            visitor.for_of_left_declaration(decl)?;
        }
        ast::statement::for_of::Left::LeftPattern(pattern) => {
            visitor.for_of_assignment_pattern(pattern)?;
        }
    }
    Ok(())
}

pub fn map_for_of_statement_lhs_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    lhs: &'ast ast::statement::for_of::Left<Loc, Loc>,
) -> ast::statement::for_of::Left<Loc, Loc> {
    match lhs {
        ast::statement::for_of::Left::LeftDeclaration((loc, decl)) => {
            let decl_ = visitor.map_variable_declaration(loc, decl);
            ast::statement::for_of::Left::LeftDeclaration((loc.dupe(), decl_))
        }
        ast::statement::for_of::Left::LeftPattern(pat) => {
            ast::statement::for_of::Left::LeftPattern(visitor.map_pattern(None, pat))
        }
    }
}

pub fn for_of_left_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    left: &'ast (Loc, ast::statement::VariableDeclaration<Loc, Type>),
) -> Result<(), E> {
    let (loc, decl) = left;
    visitor.variable_declaration(loc, decl)?;
    Ok(())
}

pub fn map_for_of_left_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    left: &'ast (Loc, ast::statement::VariableDeclaration<Loc, Loc>),
) -> (Loc, ast::statement::VariableDeclaration<Loc, Loc>) {
    let (loc, decl) = left;
    (loc.dupe(), visitor.map_variable_declaration(loc, decl))
}

pub fn for_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    stmt: &'ast ast::statement::For<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::For {
        init,
        test,
        update,
        body,
        comments,
    } = stmt;
    if let Some(init) = init {
        visitor.for_statement_init(init)?;
    }
    if let Some(test) = test {
        visitor.predicate_expression(test)?;
    }
    if let Some(update) = update {
        visitor.expression(update)?;
    }
    visitor.statement(body)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_for_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    stmt: &'ast ast::statement::For<Loc, Loc>,
) -> ast::statement::For<Loc, Loc> {
    let ast::statement::For {
        init,
        test,
        update,
        body,
        comments,
    } = stmt;
    let init_ = init.as_ref().map(|i| visitor.map_for_statement_init(i));
    let test_ = test.as_ref().map(|t| visitor.map_expression(t));
    let update_ = update.as_ref().map(|u| visitor.map_expression(u));
    let body_ = visitor.map_statement(body);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::For {
        init: init_,
        test: test_,
        update: update_,
        body: body_,
        comments: comments_,
    }
}

pub fn for_statement_init_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    init: &'ast ast::statement::for_::Init<Loc, Type>,
) -> Result<(), E> {
    match init {
        ast::statement::for_::Init::InitDeclaration(decl) => {
            visitor.for_init_declaration(decl)?;
        }
        ast::statement::for_::Init::InitExpression(expr) => {
            visitor.expression(expr)?;
        }
    }
    Ok(())
}

pub fn map_for_statement_init_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    init: &'ast ast::statement::for_::Init<Loc, Loc>,
) -> ast::statement::for_::Init<Loc, Loc> {
    match init {
        ast::statement::for_::Init::InitDeclaration((loc, decl)) => {
            let decl_ = visitor.map_variable_declaration(loc, decl);
            ast::statement::for_::Init::InitDeclaration((loc.dupe(), decl_))
        }
        ast::statement::for_::Init::InitExpression(expr) => {
            ast::statement::for_::Init::InitExpression(visitor.map_expression(expr))
        }
    }
}

pub fn for_init_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    init: &'ast (Loc, ast::statement::VariableDeclaration<Loc, Type>),
) -> Result<(), E> {
    let (loc, decl) = init;
    visitor.variable_declaration(loc, decl)?;
    Ok(())
}

pub fn map_for_init_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    init: &'ast (Loc, ast::statement::VariableDeclaration<Loc, Loc>),
) -> (Loc, ast::statement::VariableDeclaration<Loc, Loc>) {
    let (loc, decl) = init;
    (loc.dupe(), visitor.map_variable_declaration(loc, decl))
}

pub fn function_param_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    fpt: &'ast ast::types::function::Param<Loc, Type>,
) -> Result<(), E> {
    match &fpt.param {
        ast::types::function::ParamKind::Anonymous(annot) => {
            visitor.type_(annot)?;
        }
        ast::types::function::ParamKind::Labeled { name, annot, .. } => {
            visitor.function_param_type_identifier(name)?;
            visitor.type_(annot)?;
        }
        ast::types::function::ParamKind::Destructuring(pattern) => {
            visitor.function_param_type_pattern(pattern)?;
        }
    }
    Ok(())
}

pub fn map_function_param_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    fpt: &'ast ast::types::function::Param<Loc, Loc>,
) -> ast::types::function::Param<Loc, Loc> {
    let loc = &fpt.loc;
    let param = match &fpt.param {
        ast::types::function::ParamKind::Anonymous(annot) => {
            let annot_ = visitor.map_type_(annot);
            ast::types::function::ParamKind::Anonymous(annot_)
        }
        ast::types::function::ParamKind::Labeled {
            name,
            annot,
            optional,
        } => {
            let name_ = visitor.map_function_param_type_identifier(name);
            let annot_ = visitor.map_type_(annot);
            ast::types::function::ParamKind::Labeled {
                name: name_,
                annot: annot_,
                optional: *optional,
            }
        }
        ast::types::function::ParamKind::Destructuring(pattern) => {
            let pattern_ = visitor.map_function_param_type_pattern(pattern);
            ast::types::function::ParamKind::Destructuring(pattern_)
        }
    };
    ast::types::function::Param {
        loc: loc.dupe(),
        param,
    }
}

pub fn function_rest_param_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    frpt: &'ast ast::types::function::RestParam<Loc, Type>,
) -> Result<(), E> {
    let ast::types::function::RestParam {
        loc: _,
        argument,
        comments,
    } = frpt;
    visitor.function_param_type(argument)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_function_rest_param_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    frpt: &'ast ast::types::function::RestParam<Loc, Loc>,
) -> ast::types::function::RestParam<Loc, Loc> {
    let ast::types::function::RestParam {
        loc,
        argument,
        comments,
    } = frpt;
    let argument_ = visitor.map_function_param_type(argument);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::function::RestParam {
        loc: loc.dupe(),
        argument: argument_,
        comments: comments_,
    }
}

pub fn function_this_param_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    this_param: &'ast ast::types::function::ThisParam<Loc, Type>,
) -> Result<(), E> {
    let ast::types::function::ThisParam {
        loc: _,
        annot,
        comments,
    } = this_param;
    visitor.type_annotation(annot)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_function_this_param_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    this_param: &'ast ast::types::function::ThisParam<Loc, Loc>,
) -> ast::types::function::ThisParam<Loc, Loc> {
    let ast::types::function::ThisParam {
        loc,
        annot,
        comments,
    } = this_param;
    let annot_ = visitor.map_type_annotation(annot);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::function::ThisParam {
        loc: loc.dupe(),
        annot: annot_,
        comments: comments_,
    }
}

pub fn function_type_return_annotation_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    return_: &'ast ast::types::function::ReturnAnnotation<Loc, Type>,
) -> Result<(), E> {
    match return_ {
        ast::types::function::ReturnAnnotation::Missing(_) => {}
        ast::types::function::ReturnAnnotation::Available(annot) => {
            visitor.type_annotation(annot)?;
        }
        ast::types::function::ReturnAnnotation::TypeGuard(guard) => {
            visitor.type_guard(guard)?;
        }
    }
    Ok(())
}

pub fn map_function_type_return_annotation_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    return_: &'ast ast::types::function::ReturnAnnotation<Loc, Loc>,
) -> ast::types::function::ReturnAnnotation<Loc, Loc> {
    match return_ {
        ast::types::function::ReturnAnnotation::Missing(loc) => {
            ast::types::function::ReturnAnnotation::Missing(loc.dupe())
        }
        ast::types::function::ReturnAnnotation::Available(annot) => {
            ast::types::function::ReturnAnnotation::Available(visitor.map_type_annotation(annot))
        }
        ast::types::function::ReturnAnnotation::TypeGuard(guard) => {
            ast::types::function::ReturnAnnotation::TypeGuard(visitor.map_type_guard(guard))
        }
    }
}

pub fn function_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ft: &'ast ast::types::Function<Loc, Type>,
) -> Result<(), E> {
    let ast::types::Function {
        tparams,
        params,
        return_,
        comments,
        effect: _,
    } = ft;
    if let Some(tparams) = tparams {
        visitor.type_params(&TypeParamsContext::FunctionType, tparams)?;
    }
    let ast::types::function::Params {
        loc: _,
        this,
        params: params_list,
        rest,
        comments: params_comments,
    } = params;
    if let Some(this) = this {
        visitor.function_this_param_type(this)?;
    }
    for param in params_list.iter() {
        visitor.function_param_type(param)?;
    }
    if let Some(rest) = rest {
        visitor.function_rest_param_type(rest)?;
    }
    visitor.function_type_return_annotation(return_)?;
    visitor.syntax_opt(comments.as_ref())?;
    visitor.syntax_opt(params_comments.as_ref())?;
    Ok(())
}

pub fn map_function_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    func: &'ast ast::types::Function<Loc, Loc>,
) -> ast::types::Function<Loc, Loc> {
    let ast::types::Function {
        tparams,
        params,
        return_,
        comments,
        effect,
    } = func;
    let tparams_ = tparams
        .as_ref()
        .map(|tp| visitor.map_type_params(&TypeParamsContext::FunctionType, tp));
    let ast::types::function::Params {
        loc: params_loc,
        this,
        params: params_list,
        rest,
        comments: params_comments,
    } = params;
    let this_ = this
        .as_ref()
        .map(|t| visitor.map_function_this_param_type(t));
    let params_list_ = params_list
        .iter()
        .map(|p| visitor.map_function_param_type(p))
        .collect::<Vec<_>>()
        .into();
    let rest_ = rest
        .as_ref()
        .map(|r| visitor.map_function_rest_param_type(r));
    let params_comments_ = visitor.map_syntax_opt(params_comments.as_ref());
    let params_ = ast::types::function::Params {
        loc: params_loc.dupe(),
        this: this_,
        params: params_list_,
        rest: rest_,
        comments: params_comments_,
    };
    let return__ = visitor.map_function_type_return_annotation(return_);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::Function {
        tparams: tparams_,
        params: params_,
        return_: return__,
        comments: comments_,
        effect: *effect,
    }
}

pub fn label_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ident: &'ast ast::Identifier<Loc, Loc>,
) -> Result<(), E> {
    visitor.untyped_identifier(ident)?;
    Ok(())
}

pub fn map_label_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ident: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    visitor.map_identifier(ident)
}

pub fn object_property_value_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    opvt: &'ast ast::types::object::PropertyValue<Loc, Type>,
) -> Result<(), E> {
    match opvt {
        ast::types::object::PropertyValue::Init(Some(t)) => {
            visitor.type_(t)?;
        }
        ast::types::object::PropertyValue::Init(None) => {}
        ast::types::object::PropertyValue::Get(loc, getter) => {
            visitor.object_type_property_getter(loc, getter)?;
        }
        ast::types::object::PropertyValue::Set(loc, setter) => {
            visitor.object_type_property_setter(loc, setter)?;
        }
    }
    Ok(())
}

pub fn map_object_property_value_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    opvt: &'ast ast::types::object::PropertyValue<Loc, Loc>,
) -> ast::types::object::PropertyValue<Loc, Loc> {
    match opvt {
        ast::types::object::PropertyValue::Init(Some(t)) => {
            ast::types::object::PropertyValue::Init(Some(visitor.map_type_(t)))
        }
        ast::types::object::PropertyValue::Init(None) => {
            ast::types::object::PropertyValue::Init(None)
        }
        ast::types::object::PropertyValue::Get(loc, getter) => {
            ast::types::object::PropertyValue::Get(
                loc.dupe(),
                visitor.map_object_type_property_getter(loc, getter),
            )
        }
        ast::types::object::PropertyValue::Set(loc, setter) => {
            ast::types::object::PropertyValue::Set(
                loc.dupe(),
                visitor.map_object_type_property_setter(loc, setter),
            )
        }
    }
}

pub fn object_type_property_getter_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    getter: &'ast ast::types::Function<Loc, Type>,
) -> Result<(), E> {
    visitor.function_type(getter)?;
    Ok(())
}

pub fn map_object_type_property_getter_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    getter: &'ast ast::types::Function<Loc, Loc>,
) -> ast::types::Function<Loc, Loc> {
    visitor.map_function_type(loc, getter)
}

pub fn object_type_property_setter_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    setter: &'ast ast::types::Function<Loc, Type>,
) -> Result<(), E> {
    visitor.function_type(setter)?;
    Ok(())
}

pub fn map_object_type_property_setter_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    setter: &'ast ast::types::Function<Loc, Loc>,
) -> ast::types::Function<Loc, Loc> {
    visitor.map_function_type(loc, setter)
}

pub fn object_property_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    p: &'ast ast::types::object::NormalProperty<Loc, Type>,
) -> Result<(), E> {
    let ast::types::object::NormalProperty {
        loc: _,
        key,
        value,
        optional: _,
        static_: _,
        proto: _,
        method: _,
        abstract_: _,
        override_: _,
        variance,
        ts_accessibility: _,
        init,
        comments,
    } = p;
    visitor.object_key(key)?;
    visitor.object_property_value_type(value)?;
    if let Some(variance) = variance {
        visitor.variance(variance)?;
    }
    if let Some(init) = init {
        visitor.expression(init)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_object_property_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    p: &'ast ast::types::object::NormalProperty<Loc, Loc>,
) -> ast::types::object::NormalProperty<Loc, Loc> {
    let ast::types::object::NormalProperty {
        loc,
        key,
        value,
        optional,
        static_,
        proto,
        method,
        abstract_,
        override_,
        variance,
        ts_accessibility,
        init,
        comments,
    } = p;
    let key_ = visitor.map_object_key(key);
    let value_ = visitor.map_object_property_value_type(value);
    let variance_ = visitor.map_variance_opt(variance.as_ref());
    let init_ = init.as_ref().map(|i| visitor.map_expression(i));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::object::NormalProperty {
        loc: loc.dupe(),
        key: key_,
        value: value_,
        optional: *optional,
        static_: *static_,
        proto: *proto,
        method: *method,
        abstract_: *abstract_,
        override_: *override_,
        variance: variance_,
        ts_accessibility: ts_accessibility.clone(),
        init: init_,
        comments: comments_,
    }
}

pub fn object_spread_property_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    spread: &'ast ast::types::object::SpreadProperty<Loc, Type>,
) -> Result<(), E> {
    let ast::types::object::SpreadProperty {
        loc: _,
        argument,
        comments,
    } = spread;
    visitor.type_(argument)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_object_spread_property_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    spread: &'ast ast::types::object::SpreadProperty<Loc, Loc>,
) -> ast::types::object::SpreadProperty<Loc, Loc> {
    let ast::types::object::SpreadProperty {
        loc,
        argument,
        comments,
    } = spread;
    let argument_ = visitor.map_type_(argument);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::object::SpreadProperty {
        loc: loc.dupe(),
        argument: argument_,
        comments: comments_,
    }
}

pub fn object_indexer_property_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    indexer: &'ast ast::types::object::Indexer<Loc, Type>,
) -> Result<(), E> {
    let ast::types::object::Indexer {
        loc: _,
        id: _,
        key,
        value,
        static_: _,
        variance,
        optional: _,
        comments,
    } = indexer;
    visitor.type_(key)?;
    visitor.type_(value)?;
    if let Some(variance) = variance {
        visitor.variance(variance)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_object_indexer_property_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    indexer: &'ast ast::types::object::Indexer<Loc, Loc>,
) -> ast::types::object::Indexer<Loc, Loc> {
    let ast::types::object::Indexer {
        loc,
        id,
        key,
        value,
        static_,
        variance,
        optional,
        comments,
    } = indexer;
    let id_ = id.as_ref().map(|id| visitor.map_identifier(id));
    let key_ = visitor.map_type_(key);
    let value_ = visitor.map_type_(value);
    let variance_ = visitor.map_variance_opt(variance.as_ref());
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::object::Indexer {
        loc: loc.dupe(),
        id: id_,
        key: key_,
        value: value_,
        static_: *static_,
        variance: variance_,
        optional: *optional,
        comments: comments_,
    }
}

pub fn object_internal_slot_property_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    slot: &'ast ast::types::object::InternalSlot<Loc, Type>,
) -> Result<(), E> {
    let ast::types::object::InternalSlot {
        loc: _,
        id,
        value,
        optional: _,
        static_: _,
        method: _,
        comments,
    } = slot;
    visitor.untyped_identifier(id)?;
    visitor.type_(value)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_object_internal_slot_property_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    slot: &'ast ast::types::object::InternalSlot<Loc, Loc>,
) -> ast::types::object::InternalSlot<Loc, Loc> {
    let ast::types::object::InternalSlot {
        loc,
        id,
        value,
        optional,
        static_,
        method,
        comments,
    } = slot;
    let id_ = visitor.map_identifier(id);
    let value_ = visitor.map_type_(value);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::object::InternalSlot {
        loc: loc.dupe(),
        id: id_,
        value: value_,
        optional: *optional,
        static_: *static_,
        method: *method,
        comments: comments_,
    }
}

pub fn object_call_property_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    call: &'ast ast::types::object::CallProperty<Loc, Type>,
) -> Result<(), E> {
    let ast::types::object::CallProperty {
        loc: _,
        value,
        static_: _,
        comments,
    } = call;
    let (_loc, func) = value;
    visitor.function_type(func)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_object_call_property_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    call: &'ast ast::types::object::CallProperty<Loc, Loc>,
) -> ast::types::object::CallProperty<Loc, Loc> {
    let ast::types::object::CallProperty {
        loc,
        value,
        static_,
        comments,
    } = call;
    let (func_loc, func) = value;
    let func_ = visitor.map_function_type(func_loc, func);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::object::CallProperty {
        loc: loc.dupe(),
        value: (func_loc.dupe(), func_),
        static_: *static_,
        comments: comments_,
    }
}

pub fn object_mapped_type_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    mapped: &'ast ast::types::object::MappedType<Loc, Type>,
) -> Result<(), E> {
    let ast::types::object::MappedType {
        loc: _,
        key_tparam,
        prop_type,
        source_type,
        name_type,
        variance,
        variance_op: _,
        optional: _,
        comments,
    } = mapped;
    visitor.type_param(&TypeParamsContext::ObjectMappedType, key_tparam)?;
    visitor.type_(prop_type)?;
    visitor.type_(source_type)?;
    if let Some(name_type) = name_type {
        visitor.type_(name_type)?;
    }
    if let Some(variance) = variance {
        visitor.variance(variance)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_object_mapped_type_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    mapped: &'ast ast::types::object::MappedType<Loc, Loc>,
) -> ast::types::object::MappedType<Loc, Loc> {
    let ast::types::object::MappedType {
        loc,
        key_tparam,
        prop_type,
        source_type,
        name_type,
        variance,
        variance_op,
        optional,
        comments,
    } = mapped;
    let key_tparam_ = visitor.map_type_param(&TypeParamsContext::ObjectMappedType, key_tparam);
    let prop_type_ = visitor.map_type_(prop_type);
    let source_type_ = visitor.map_type_(source_type);
    let name_type_ = name_type.as_ref().map(|t| visitor.map_type_(t));
    let variance_ = visitor.map_variance_opt(variance.as_ref());
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::object::MappedType {
        loc: loc.dupe(),
        key_tparam: key_tparam_,
        prop_type: prop_type_,
        source_type: source_type_,
        name_type: name_type_,
        variance: variance_,
        variance_op: *variance_op,
        optional: optional.clone(),
        comments: comments_,
    }
}

pub fn object_private_field_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pf: &'ast ast::types::object::PrivateField<Loc>,
) -> Result<(), E> {
    let ast::types::object::PrivateField {
        loc: _,
        key,
        comments,
    } = pf;
    visitor.private_name(key)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_object_private_field_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pf: &'ast ast::types::object::PrivateField<Loc>,
) -> ast::types::object::PrivateField<Loc> {
    let ast::types::object::PrivateField { loc, key, comments } = pf;
    let key_ = visitor.map_private_name(key);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::object::PrivateField {
        loc: loc.dupe(),
        key: key_,
        comments: comments_,
    }
}

pub fn object_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    obj: &'ast ast::types::Object<Loc, Type>,
) -> Result<(), E> {
    let ast::types::Object {
        exact: _,
        inexact: _,
        properties,
        comments,
    } = obj;
    for property in properties.iter() {
        visitor.object_type_property(property)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_object_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    object: &'ast ast::types::Object<Loc, Loc>,
) -> ast::types::Object<Loc, Loc> {
    let ast::types::Object {
        exact,
        inexact,
        properties,
        comments,
    } = object;
    let properties_ = Arc::from(
        properties
            .iter()
            .map(|prop| visitor.map_object_type_property(prop))
            .collect::<Vec<_>>(),
    );
    let comments_ = comments.as_ref().map(|c| visitor.map_syntax(c));
    ast::types::Object {
        exact: *exact,
        inexact: *inexact,
        properties: properties_,
        comments: comments_,
    }
}

pub fn object_type_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    p: &'ast ast::types::object::Property<Loc, Type>,
) -> Result<(), E> {
    match p {
        ast::types::object::Property::NormalProperty(prop) => {
            visitor.object_property_type(prop)?;
        }
        ast::types::object::Property::SpreadProperty(spread) => {
            visitor.object_spread_property_type(spread)?;
        }
        ast::types::object::Property::Indexer(indexer) => {
            visitor.object_indexer_property_type(indexer)?;
        }
        ast::types::object::Property::InternalSlot(slot) => {
            visitor.object_internal_slot_property_type(slot)?;
        }
        ast::types::object::Property::CallProperty(call) => {
            visitor.object_call_property_type(call)?;
        }
        ast::types::object::Property::MappedType(mapped) => {
            visitor.object_mapped_type_property(mapped)?;
        }
        ast::types::object::Property::PrivateField(pf) => {
            visitor.object_private_field_type(pf)?;
        }
    }
    Ok(())
}

pub fn map_object_type_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    prop: &'ast ast::types::object::Property<Loc, Loc>,
) -> ast::types::object::Property<Loc, Loc> {
    match prop {
        ast::types::object::Property::NormalProperty(p) => {
            ast::types::object::Property::NormalProperty(visitor.map_object_property_type(p))
        }
        ast::types::object::Property::SpreadProperty(p) => {
            ast::types::object::Property::SpreadProperty(visitor.map_object_spread_property_type(p))
        }
        ast::types::object::Property::Indexer(p) => {
            ast::types::object::Property::Indexer(visitor.map_object_indexer_property_type(p))
        }
        ast::types::object::Property::InternalSlot(p) => {
            ast::types::object::Property::InternalSlot(
                visitor.map_object_internal_slot_property_type(p),
            )
        }
        ast::types::object::Property::CallProperty(p) => {
            ast::types::object::Property::CallProperty(visitor.map_object_call_property_type(p))
        }
        ast::types::object::Property::MappedType(p) => {
            ast::types::object::Property::MappedType(visitor.map_object_mapped_type_property(p))
        }
        ast::types::object::Property::PrivateField(p) => {
            ast::types::object::Property::PrivateField(visitor.map_object_private_field_type(p))
        }
    }
}

pub fn interface_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Type,
    i: &'ast ast::types::Interface<Loc, Type>,
) -> Result<(), E> {
    let ast::types::Interface {
        extends,
        body,
        comments,
    } = i;
    for (_loc, generic) in extends.iter() {
        visitor.generic_type(generic)?;
    }
    let (_loc, obj_type) = body;
    visitor.object_type(obj_type)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_interface_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    interface: &'ast ast::types::Interface<Loc, Loc>,
) -> ast::types::Interface<Loc, Loc> {
    let ast::types::Interface {
        body,
        extends,
        comments,
    } = interface;
    let (body_loc, body_obj) = body;
    let body_obj_ = visitor.map_object_type(body_loc, body_obj);
    let extends_ = Arc::from(
        extends
            .iter()
            .map(|(loc, generic)| {
                let generic_ = visitor.map_generic_type(loc, generic);
                (loc.dupe(), generic_)
            })
            .collect::<Vec<_>>(),
    );
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::Interface {
        body: (body_loc.dupe(), body_obj_),
        extends: extends_,
        comments: comments_,
    }
}

pub fn generic_identifier_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    git: &'ast ast::types::generic::Identifier<Loc, Type>,
) -> Result<(), E> {
    match git {
        ast::types::generic::Identifier::Unqualified(id) => {
            visitor.type_identifier_reference(id)?;
        }
        ast::types::generic::Identifier::Qualified(qual) => {
            visitor.generic_qualified_identifier_type(qual)?;
        }
        ast::types::generic::Identifier::ImportTypeAnnot(import_type) => {
            visitor.generic_identifier_import_type(import_type)?;
        }
    }
    Ok(())
}

pub fn map_generic_identifier_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    git: &'ast ast::types::generic::Identifier<Loc, Loc>,
) -> ast::types::generic::Identifier<Loc, Loc> {
    match git {
        ast::types::generic::Identifier::Unqualified(id) => {
            let id_ = visitor.map_type_identifier_reference(id);
            ast::types::generic::Identifier::Unqualified(id_)
        }
        ast::types::generic::Identifier::Qualified(qual) => {
            let qual_ = visitor.map_generic_qualified_identifier_type(qual);
            ast::types::generic::Identifier::Qualified(Arc::new(qual_))
        }
        ast::types::generic::Identifier::ImportTypeAnnot(import_type) => {
            let import_type_ = visitor.map_generic_identifier_import_type(import_type);
            ast::types::generic::Identifier::ImportTypeAnnot(Arc::new(import_type_))
        }
    }
}

pub fn generic_identifier_import_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    import_type: &'ast ast::types::generic::ImportType<Loc, Type>,
) -> Result<(), E> {
    visitor.string_literal(&import_type.source.1)?;
    visitor.syntax_opt(import_type.comments.as_ref())?;
    Ok(())
}

pub fn map_generic_identifier_import_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    import_type: &'ast ast::types::generic::ImportType<Loc, Loc>,
) -> ast::types::generic::ImportType<Loc, Loc> {
    let ast::types::generic::ImportType {
        loc,
        source,
        comments,
    } = import_type;
    let (source_loc, string_lit) = source;
    let string_lit_ = visitor.map_string_literal(string_lit);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::generic::ImportType {
        loc: loc.dupe(),
        source: (source_loc.dupe(), string_lit_),
        comments: comments_,
    }
}

pub fn generic_qualified_identifier_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    qual: &'ast ast::types::generic::Qualified<Loc, Type>,
) -> Result<(), E> {
    let ast::types::generic::Qualified {
        loc: _,
        qualification,
        id,
    } = qual;
    visitor.generic_identifier_type(qualification)?;
    visitor.member_type_identifier(id)?;
    Ok(())
}

pub fn map_generic_qualified_identifier_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    qual: &'ast ast::types::generic::Qualified<Loc, Loc>,
) -> ast::types::generic::Qualified<Loc, Loc> {
    let ast::types::generic::Qualified {
        loc,
        qualification,
        id,
    } = qual;
    let qualification_ = visitor.map_generic_identifier_type(qualification);
    let id_ = visitor.map_member_type_identifier(id);
    ast::types::generic::Qualified {
        loc: loc.dupe(),
        qualification: qualification_,
        id: id_,
    }
}

pub fn member_type_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Type>,
) -> Result<(), E> {
    visitor.identifier(id)?;
    Ok(())
}

pub fn map_member_type_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    visitor.map_identifier(id)
}

pub fn variance_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    variance: &'ast ast::Variance<Loc>,
) -> Result<(), E> {
    let ast::Variance {
        loc: _,
        kind: _,
        comments,
    } = variance;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_variance_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    variance: &'ast ast::Variance<Loc>,
) -> ast::Variance<Loc> {
    let ast::Variance {
        loc,
        kind,
        comments,
    } = variance;
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::Variance {
        loc: loc.dupe(),
        kind: kind.clone(),
        comments: comments_,
    }
}

pub fn variance_opt_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    opt: Option<&'ast ast::Variance<Loc>>,
) -> Result<(), E> {
    if let Some(variance) = opt {
        visitor.variance(variance)?;
    }
    Ok(())
}

pub fn map_variance_opt_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    opt: Option<&'ast ast::Variance<Loc>>,
) -> Option<ast::Variance<Loc>> {
    opt.map(|variance| visitor.map_variance(variance))
}

pub fn tparam_const_modifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    c: &'ast ast::types::type_param::ConstModifier<Loc>,
) -> Result<(), E> {
    let ast::types::type_param::ConstModifier { loc: _, comments } = c;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_tparam_const_modifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    c: &'ast ast::types::type_param::ConstModifier<Loc>,
) -> ast::types::type_param::ConstModifier<Loc> {
    let ast::types::type_param::ConstModifier { loc, comments } = c;
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::type_param::ConstModifier {
        loc: loc.dupe(),
        comments: comments_,
    }
}

pub fn type_args_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    targs: &'ast ast::types::TypeArgs<Loc, Type>,
) -> Result<(), E> {
    let ast::types::TypeArgs {
        loc: _,
        arguments,
        comments,
    } = targs;
    for arg in arguments.iter() {
        visitor.type_(arg)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_type_args_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    targs: &'ast ast::types::TypeArgs<Loc, Loc>,
) -> ast::types::TypeArgs<Loc, Loc> {
    let ast::types::TypeArgs {
        loc,
        arguments,
        comments,
    } = targs;
    let arguments_ = Arc::from(
        arguments
            .iter()
            .map(|arg| visitor.map_type_(arg))
            .collect::<Vec<_>>(),
    );
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::TypeArgs {
        loc: loc.dupe(),
        arguments: arguments_,
        comments: comments_,
    }
}

pub fn type_params_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: &TypeParamsContext,
    tparams: &'ast ast::types::TypeParams<Loc, Type>,
) -> Result<(), E> {
    let ast::types::TypeParams {
        loc: _,
        params,
        comments,
    } = tparams;
    for param in params.iter() {
        visitor.type_param(kind, param)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_type_params_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: &TypeParamsContext,
    tparams: &'ast ast::types::TypeParams<Loc, Loc>,
) -> ast::types::TypeParams<Loc, Loc> {
    let ast::types::TypeParams {
        loc,
        params,
        comments,
    } = tparams;
    let params_ = Arc::from(
        params
            .iter()
            .map(|p| visitor.map_type_param(kind, p))
            .collect::<Vec<_>>(),
    );
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::TypeParams {
        loc: loc.dupe(),
        params: params_,
        comments: comments_,
    }
}

pub fn type_param_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _kind: &TypeParamsContext,
    tparam: &'ast ast::types::TypeParam<Loc, Type>,
) -> Result<(), E> {
    let ast::types::TypeParam {
        loc: _,
        name,
        bound,
        bound_kind: _,
        variance,
        default,
        const_,
    } = tparam;
    visitor.type_annotation_hint(bound)?;
    visitor.variance_opt(variance.as_ref())?;
    if let Some(default_type) = default {
        visitor.type_(default_type)?;
    }
    if let Some(const_mod) = const_ {
        visitor.tparam_const_modifier(const_mod)?;
    }
    visitor.binding_type_identifier(name)?;
    Ok(())
}

pub fn map_type_param_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _kind: &TypeParamsContext,
    tparam: &'ast ast::types::TypeParam<Loc, Loc>,
) -> ast::types::TypeParam<Loc, Loc> {
    let ast::types::TypeParam {
        loc,
        name,
        bound,
        bound_kind,
        variance,
        default,
        const_,
    } = tparam;
    let name_ = visitor.map_identifier(name);
    let bound_ = visitor.map_type_annotation_hint(bound);
    let variance_ = visitor.map_variance_opt(variance.as_ref());
    let default_ = default.as_ref().map(|d| visitor.map_type_(d));
    let const__ = const_
        .as_ref()
        .map(|c| ast::types::type_param::ConstModifier {
            loc: c.loc.dupe(),
            comments: visitor.map_syntax_opt(c.comments.as_ref()),
        });
    ast::types::TypeParam {
        loc: loc.dupe(),
        name: name_,
        bound: bound_,
        bound_kind: bound_kind.clone(),
        variance: variance_,
        default: default_,
        const_: const__,
    }
}

pub fn generic_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    gt: &'ast ast::types::Generic<Loc, Type>,
) -> Result<(), E> {
    let ast::types::Generic {
        id,
        targs,
        comments,
    } = gt;
    visitor.generic_identifier_type(id)?;
    if let Some(targs_val) = targs {
        visitor.type_args(targs_val)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_generic_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    generic: &'ast ast::types::Generic<Loc, Loc>,
) -> ast::types::Generic<Loc, Loc> {
    let ast::types::Generic {
        id,
        targs,
        comments,
    } = generic;
    let id_ = visitor.map_generic_identifier_type(id);
    let targs_ = targs.as_ref().map(|t| visitor.map_type_args(t));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::Generic {
        id: id_,
        targs: targs_,
        comments: comments_,
    }
}

pub fn indexed_access_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Type,
    ia: &'ast ast::types::IndexedAccess<Loc, Type>,
) -> Result<(), E> {
    let ast::types::IndexedAccess {
        object,
        index,
        comments,
    } = ia;
    visitor.type_(object)?;
    visitor.type_(index)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_indexed_access_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    indexed: &'ast ast::types::IndexedAccess<Loc, Loc>,
) -> ast::types::IndexedAccess<Loc, Loc> {
    let ast::types::IndexedAccess {
        object,
        index,
        comments,
    } = indexed;
    let object_ = visitor.map_type_(object);
    let index_ = visitor.map_type_(index);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::IndexedAccess {
        object: object_,
        index: index_,
        comments: comments_,
    }
}

pub fn optional_indexed_access_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    ia: &'ast ast::types::OptionalIndexedAccess<Loc, Type>,
) -> Result<(), E> {
    let ast::types::OptionalIndexedAccess {
        indexed_access,
        optional: _,
    } = ia;
    visitor.indexed_access_type(loc, indexed_access)?;
    Ok(())
}

pub fn map_optional_indexed_access_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    indexed: &'ast ast::types::OptionalIndexedAccess<Loc, Loc>,
) -> ast::types::OptionalIndexedAccess<Loc, Loc> {
    let ast::types::OptionalIndexedAccess {
        indexed_access,
        optional,
    } = indexed;
    let indexed_access_ = visitor.map_indexed_access_type(loc, indexed_access);
    ast::types::OptionalIndexedAccess {
        indexed_access: indexed_access_,
        optional: *optional,
    }
}

pub fn string_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    lit: &'ast ast::StringLiteral<Loc>,
) -> Result<(), E> {
    let ast::StringLiteral {
        value: _,
        raw: _,
        comments,
    } = lit;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_string_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    lit: &'ast ast::StringLiteral<Loc>,
) -> ast::StringLiteral<Loc> {
    let ast::StringLiteral {
        value,
        raw,
        comments,
    } = lit;
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::StringLiteral {
        value: value.clone(),
        raw: raw.clone(),
        comments: comments_,
    }
}

pub fn number_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    lit: &'ast ast::NumberLiteral<Loc>,
) -> Result<(), E> {
    let ast::NumberLiteral {
        value: _,
        raw: _,
        comments,
    } = lit;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_number_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    lit: &'ast ast::NumberLiteral<Loc>,
) -> ast::NumberLiteral<Loc> {
    let ast::NumberLiteral {
        value,
        raw,
        comments,
    } = lit;
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::NumberLiteral {
        value: *value,
        raw: raw.clone(),
        comments: comments_,
    }
}

pub fn bigint_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    lit: &'ast ast::BigIntLiteral<Loc>,
) -> Result<(), E> {
    let ast::BigIntLiteral {
        value: _,
        raw: _,
        comments,
    } = lit;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_bigint_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    lit: &'ast ast::BigIntLiteral<Loc>,
) -> ast::BigIntLiteral<Loc> {
    let ast::BigIntLiteral {
        value,
        raw,
        comments,
    } = lit;
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::BigIntLiteral {
        value: *value,
        raw: raw.clone(),
        comments: comments_,
    }
}

pub fn boolean_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    lit: &'ast ast::BooleanLiteral<Loc>,
) -> Result<(), E> {
    let ast::BooleanLiteral { value: _, comments } = lit;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_boolean_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    lit: &'ast ast::BooleanLiteral<Loc>,
) -> ast::BooleanLiteral<Loc> {
    let ast::BooleanLiteral { value, comments } = lit;
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::BooleanLiteral {
        value: *value,
        comments: comments_,
    }
}

pub fn null_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    comments: Option<&'ast ast::Syntax<Loc, ()>>,
) -> Result<(), E> {
    visitor.syntax_opt(comments)?;
    Ok(())
}

pub fn map_null_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    comments: Option<&'ast ast::Syntax<Loc, ()>>,
) -> Option<ast::Syntax<Loc, ()>> {
    visitor.map_syntax_opt(comments)
}

pub fn regexp_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    lit: &'ast ast::RegExpLiteral<Loc>,
) -> Result<(), E> {
    let ast::RegExpLiteral {
        pattern: _,
        flags: _,
        raw: _,
        comments,
    } = lit;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_regexp_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    lit: &'ast ast::RegExpLiteral<Loc>,
) -> ast::RegExpLiteral<Loc> {
    let ast::RegExpLiteral {
        pattern,
        flags,
        raw,
        comments,
    } = lit;
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::RegExpLiteral {
        pattern: pattern.clone(),
        flags: flags.clone(),
        raw: raw.clone(),
        comments: comments_,
    }
}

pub fn module_ref_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    lit: &'ast ast::ModuleRefLiteral<Loc>,
) -> Result<(), E> {
    let ast::ModuleRefLiteral {
        value: _,
        require_loc: _,
        def_loc_opt: _,
        prefix_len: _,
        raw: _,
        comments,
    } = lit;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_module_ref_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    lit: &'ast ast::ModuleRefLiteral<Loc>,
) -> ast::ModuleRefLiteral<Loc> {
    let ast::ModuleRefLiteral {
        value,
        require_loc,
        def_loc_opt,
        prefix_len,
        raw,
        comments,
    } = lit;
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::ModuleRefLiteral {
        value: value.clone(),
        require_loc: require_loc.dupe(),
        def_loc_opt: def_loc_opt.clone(),
        prefix_len: *prefix_len,
        raw: raw.clone(),
        comments: comments_,
    }
}

pub fn nullable_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    nullable: &'ast ast::types::Nullable<Loc, Type>,
) -> Result<(), E> {
    let ast::types::Nullable { argument, comments } = nullable;
    visitor.type_(argument)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_nullable_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    nullable: &'ast ast::types::Nullable<Loc, Loc>,
) -> ast::types::Nullable<Loc, Loc> {
    let ast::types::Nullable { argument, comments } = nullable;
    let argument_ = visitor.map_type_(argument);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::Nullable {
        argument: argument_,
        comments: comments_,
    }
}

pub fn conditional_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    conditional: &'ast ast::types::Conditional<Loc, Type>,
) -> Result<(), E> {
    let ast::types::Conditional {
        check_type,
        extends_type,
        true_type,
        false_type,
        comments,
    } = conditional;
    visitor.type_(check_type)?;
    visitor.type_(extends_type)?;
    visitor.type_(true_type)?;
    visitor.type_(false_type)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_conditional_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    conditional: &'ast ast::types::Conditional<Loc, Loc>,
) -> ast::types::Conditional<Loc, Loc> {
    let ast::types::Conditional {
        check_type,
        extends_type,
        true_type,
        false_type,
        comments,
    } = conditional;
    let check_type_ = visitor.map_type_(check_type);
    let extends_type_ = visitor.map_type_(extends_type);
    let true_type_ = visitor.map_type_(true_type);
    let false_type_ = visitor.map_type_(false_type);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::Conditional {
        check_type: check_type_,
        extends_type: extends_type_,
        true_type: true_type_,
        false_type: false_type_,
        comments: comments_,
    }
}

pub fn infer_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    infer: &'ast ast::types::Infer<Loc, Type>,
) -> Result<(), E> {
    let ast::types::Infer { tparam, comments } = infer;
    visitor.type_param(&TypeParamsContext::Infer, tparam)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_infer_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    infer: &'ast ast::types::Infer<Loc, Loc>,
) -> ast::types::Infer<Loc, Loc> {
    let ast::types::Infer { tparam, comments } = infer;
    let tparam_ = visitor.map_type_param(&TypeParamsContext::Infer, tparam);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::Infer {
        tparam: tparam_,
        comments: comments_,
    }
}

pub fn typeof_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    typeof_: &'ast ast::types::Typeof<Loc, Type>,
) -> Result<(), E> {
    let ast::types::Typeof {
        argument,
        targs,
        comments,
    } = typeof_;
    visitor.typeof_expression(argument)?;
    if let Some(targs) = targs {
        visitor.type_args(targs)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_typeof_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    typeof_: &'ast ast::types::Typeof<Loc, Loc>,
) -> ast::types::Typeof<Loc, Loc> {
    let ast::types::Typeof {
        argument,
        targs,
        comments,
    } = typeof_;
    let argument_ = visitor.map_typeof_expression(argument);
    let targs_ = targs.as_ref().map(|t| visitor.map_type_args(t));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::Typeof {
        argument: argument_,
        targs: targs_,
        comments: comments_,
    }
}

pub fn typeof_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    git: &'ast ast::types::typeof_::Target<Loc, Type>,
) -> Result<(), E> {
    match git {
        ast::types::typeof_::Target::Unqualified(id) => {
            visitor.typeof_identifier(id)?;
        }
        ast::types::typeof_::Target::Qualified(qual) => {
            visitor.typeof_qualified_identifier(qual)?;
        }
        ast::types::typeof_::Target::Import(it) => {
            visitor.generic_identifier_import_type(it)?;
        }
    }
    Ok(())
}

pub fn map_typeof_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    target: &'ast ast::types::typeof_::Target<Loc, Loc>,
) -> ast::types::typeof_::Target<Loc, Loc> {
    match target {
        ast::types::typeof_::Target::Unqualified(id) => {
            let id_ = visitor.map_identifier(id);
            ast::types::typeof_::Target::Unqualified(id_)
        }
        ast::types::typeof_::Target::Qualified(qualified) => {
            let ast::types::typeof_::Qualified {
                loc,
                qualification,
                id,
            } = qualified.as_ref();
            let qualification_ = visitor.map_typeof_expression(qualification);
            let id_ = visitor.map_identifier(id);
            ast::types::typeof_::Target::Qualified(Arc::new(ast::types::typeof_::Qualified {
                loc: loc.dupe(),
                qualification: qualification_,
                id: id_,
            }))
        }
        ast::types::typeof_::Target::Import(it) => {
            let it_ = visitor.map_generic_identifier_import_type(it);
            ast::types::typeof_::Target::Import(Arc::new(it_))
        }
    }
}

pub fn typeof_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Type>,
) -> Result<(), E> {
    visitor.identifier(id)?;
    Ok(())
}

pub fn map_typeof_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    visitor.map_identifier(id)
}

pub fn typeof_member_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Type>,
) -> Result<(), E> {
    visitor.identifier(id)?;
    Ok(())
}

pub fn map_typeof_member_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    visitor.map_identifier(id)
}

pub fn typeof_qualified_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    qual: &'ast ast::types::typeof_::Qualified<Loc, Type>,
) -> Result<(), E> {
    let ast::types::typeof_::Qualified {
        loc: _,
        qualification,
        id,
    } = qual;
    visitor.typeof_expression(qualification)?;
    visitor.typeof_member_identifier(id)?;
    Ok(())
}

pub fn map_typeof_qualified_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    qual: &'ast ast::types::typeof_::Qualified<Loc, Loc>,
) -> ast::types::typeof_::Qualified<Loc, Loc> {
    let ast::types::typeof_::Qualified {
        loc,
        qualification,
        id,
    } = qual;
    let qualification_ = match qualification {
        ast::types::typeof_::Target::Unqualified(id) => {
            ast::types::typeof_::Target::Unqualified(visitor.map_identifier(id))
        }
        ast::types::typeof_::Target::Qualified(q) => ast::types::typeof_::Target::Qualified(
            Arc::new(visitor.map_typeof_qualified_identifier(q)),
        ),
        ast::types::typeof_::Target::Import(it) => ast::types::typeof_::Target::Import(Arc::new(
            visitor.map_generic_identifier_import_type(it),
        )),
    };
    let id_ = visitor.map_identifier(id);
    ast::types::typeof_::Qualified {
        loc: loc.dupe(),
        qualification: qualification_,
        id: id_,
    }
}

pub fn keyof_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    keyof: &'ast ast::types::Keyof<Loc, Type>,
) -> Result<(), E> {
    let ast::types::Keyof { argument, comments } = keyof;
    visitor.type_(argument)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_keyof_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    keyof: &'ast ast::types::Keyof<Loc, Loc>,
) -> ast::types::Keyof<Loc, Loc> {
    let ast::types::Keyof { argument, comments } = keyof;
    let argument_ = visitor.map_type_(argument);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::Keyof {
        argument: argument_,
        comments: comments_,
    }
}

pub fn render_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    renders: &'ast ast::types::Renders<Loc, Type>,
) -> Result<(), E> {
    let ast::types::Renders {
        operator_loc: _,
        argument,
        comments,
        variant: _,
    } = renders;
    visitor.type_(argument)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_render_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    renders: &'ast ast::types::Renders<Loc, Loc>,
) -> ast::types::Renders<Loc, Loc> {
    let ast::types::Renders {
        operator_loc,
        argument,
        comments,
        variant,
    } = renders;
    let argument_ = visitor.map_type_(argument);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::Renders {
        operator_loc: operator_loc.dupe(),
        argument: argument_,
        comments: comments_,
        variant: *variant,
    }
}

pub fn readonly_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    readonly: &'ast ast::types::ReadOnly<Loc, Type>,
) -> Result<(), E> {
    let ast::types::ReadOnly { argument, comments } = readonly;
    visitor.type_(argument)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_readonly_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    readonly: &'ast ast::types::ReadOnly<Loc, Loc>,
) -> ast::types::ReadOnly<Loc, Loc> {
    let ast::types::ReadOnly { argument, comments } = readonly;
    let argument_ = visitor.map_type_(argument);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::ReadOnly {
        argument: argument_,
        comments: comments_,
    }
}

pub fn tuple_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    tuple: &'ast ast::types::Tuple<Loc, Type>,
) -> Result<(), E> {
    let ast::types::Tuple {
        elements,
        inexact: _,
        comments,
    } = tuple;
    for element in elements.iter() {
        visitor.tuple_element(element)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_tuple_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    tuple: &'ast ast::types::Tuple<Loc, Loc>,
) -> ast::types::Tuple<Loc, Loc> {
    let ast::types::Tuple {
        elements,
        inexact,
        comments,
    } = tuple;
    let elements_ = Arc::from(
        elements
            .iter()
            .map(|el| visitor.map_tuple_element(el))
            .collect::<Vec<_>>(),
    );
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::Tuple {
        elements: elements_,
        inexact: *inexact,
        comments: comments_,
    }
}

pub fn template_literal_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    t: &'ast ast::types::TypeTemplateLiteral<Loc, Type>,
) -> Result<(), E> {
    let ast::types::TypeTemplateLiteral {
        quasis: _,
        types,
        comments,
    } = t;
    for ty in types.iter() {
        visitor.type_(ty)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_template_literal_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    t: &'ast ast::types::TypeTemplateLiteral<Loc, Loc>,
) -> ast::types::TypeTemplateLiteral<Loc, Loc> {
    let ast::types::TypeTemplateLiteral {
        quasis,
        types,
        comments,
    } = t;
    let types_ = Arc::from(
        types
            .iter()
            .map(|ty| visitor.map_type_(ty))
            .collect::<Vec<_>>(),
    );
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::TypeTemplateLiteral {
        quasis: quasis.dupe(),
        types: types_,
        comments: comments_,
    }
}

pub fn tuple_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    el: &'ast ast::types::tuple::Element<Loc, Type>,
) -> Result<(), E> {
    match el {
        ast::types::tuple::Element::UnlabeledElement {
            loc: _,
            annot,
            optional: _,
        } => {
            visitor.type_(annot)?;
        }
        ast::types::tuple::Element::LabeledElement { loc: _, element } => {
            visitor.tuple_labeled_element(element)?;
        }
        ast::types::tuple::Element::SpreadElement { loc: _, element } => {
            visitor.tuple_spread_element(element)?;
        }
    }
    Ok(())
}

pub fn map_tuple_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    el: &'ast ast::types::tuple::Element<Loc, Loc>,
) -> ast::types::tuple::Element<Loc, Loc> {
    match el {
        ast::types::tuple::Element::UnlabeledElement {
            loc,
            annot,
            optional,
        } => {
            let annot_ = visitor.map_type_(annot);
            ast::types::tuple::Element::UnlabeledElement {
                loc: loc.dupe(),
                annot: annot_,
                optional: *optional,
            }
        }
        ast::types::tuple::Element::LabeledElement { loc, element } => {
            let element_ = visitor.map_tuple_labeled_element(element);
            ast::types::tuple::Element::LabeledElement {
                loc: loc.dupe(),
                element: element_,
            }
        }
        ast::types::tuple::Element::SpreadElement { loc, element } => {
            let element_ = visitor.map_tuple_spread_element(element);
            ast::types::tuple::Element::SpreadElement {
                loc: loc.dupe(),
                element: element_,
            }
        }
    }
}

pub fn tuple_labeled_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    labeled: &'ast ast::types::tuple::LabeledElement<Loc, Type>,
) -> Result<(), E> {
    let ast::types::tuple::LabeledElement {
        annot,
        name: _,
        variance,
        optional: _,
    } = labeled;
    visitor.type_(annot)?;
    visitor.variance_opt(variance.as_ref())?;
    Ok(())
}

pub fn map_tuple_labeled_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    labeled: &'ast ast::types::tuple::LabeledElement<Loc, Loc>,
) -> ast::types::tuple::LabeledElement<Loc, Loc> {
    let ast::types::tuple::LabeledElement {
        name,
        annot,
        variance,
        optional,
    } = labeled;
    let annot_ = visitor.map_type_(annot);
    let variance_ = visitor.map_variance_opt(variance.as_ref());
    ast::types::tuple::LabeledElement {
        name: name.dupe(),
        annot: annot_,
        variance: variance_,
        optional: *optional,
    }
}

pub fn tuple_spread_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    spread: &'ast ast::types::tuple::SpreadElement<Loc, Type>,
) -> Result<(), E> {
    let ast::types::tuple::SpreadElement { annot, name: _ } = spread;
    visitor.type_(annot)?;
    Ok(())
}

pub fn map_tuple_spread_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    spread: &'ast ast::types::tuple::SpreadElement<Loc, Loc>,
) -> ast::types::tuple::SpreadElement<Loc, Loc> {
    let ast::types::tuple::SpreadElement { name, annot } = spread;
    let annot_ = visitor.map_type_(annot);
    ast::types::tuple::SpreadElement {
        name: name.dupe(),
        annot: annot_,
    }
}

pub fn array_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    array: &'ast ast::types::Array<Loc, Type>,
) -> Result<(), E> {
    let ast::types::Array { argument, comments } = array;
    visitor.type_(argument)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_array_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    array: &'ast ast::types::Array<Loc, Loc>,
) -> ast::types::Array<Loc, Loc> {
    let ast::types::Array { argument, comments } = array;
    let argument_ = visitor.map_type_(argument);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::Array {
        argument: argument_,
        comments: comments_,
    }
}

pub fn union_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    t: &'ast ast::types::Union<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::types::Union { types, comments } = t;
    let (t0, t1, ts) = types;
    visitor.type_(t0)?;
    visitor.type_(t1)?;
    for t in ts {
        visitor.type_(t)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_union_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    t: &'ast ast::types::Union<Loc, Loc>,
) -> ast::types::Union<Loc, Loc> {
    let ast::types::Union { types, comments } = t;
    let (t0, t1, ts) = types;
    let t0_ = visitor.map_type_(t0);
    let t1_ = visitor.map_type_(t1);
    let ts_ = ts.iter().map(|t| visitor.map_type_(t)).collect::<Vec<_>>();
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::Union {
        types: (t0_, t1_, ts_),
        comments: comments_,
    }
}

pub fn intersection_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    t: &'ast ast::types::Intersection<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::types::Intersection { types, comments } = t;
    let (t0, t1, ts) = types;
    visitor.type_(t0)?;
    visitor.type_(t1)?;
    for t in ts {
        visitor.type_(t)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_intersection_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    t: &'ast ast::types::Intersection<Loc, Loc>,
) -> ast::types::Intersection<Loc, Loc> {
    let ast::types::Intersection { types, comments } = t;
    let (t0, t1, ts) = types;
    let t0_ = visitor.map_type_(t0);
    let t1_ = visitor.map_type_(t1);
    let ts_ = ts.iter().map(|t| visitor.map_type_(t)).collect::<Vec<_>>();
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::Intersection {
        types: (t0_, t1_, ts_),
        comments: comments_,
    }
}

pub fn type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    t: &'ast ast::types::Type<Loc, Type>,
) -> Result<(), E> {
    match t.deref() {
        TypeInner::Any { loc: _, comments } => {
            visitor.syntax_opt(comments.as_ref())?;
        }
        TypeInner::Mixed { loc: _, comments } => {
            visitor.syntax_opt(comments.as_ref())?;
        }
        TypeInner::Empty { loc: _, comments } => {
            visitor.syntax_opt(comments.as_ref())?;
        }
        TypeInner::Void { loc: _, comments } => {
            visitor.syntax_opt(comments.as_ref())?;
        }
        TypeInner::Null { loc: _, comments } => {
            visitor.syntax_opt(comments.as_ref())?;
        }
        TypeInner::Symbol { loc: _, comments } => {
            visitor.syntax_opt(comments.as_ref())?;
        }
        TypeInner::Number { loc: _, comments } => {
            visitor.syntax_opt(comments.as_ref())?;
        }
        TypeInner::BigInt { loc: _, comments } => {
            visitor.syntax_opt(comments.as_ref())?;
        }
        TypeInner::String { loc: _, comments } => {
            visitor.syntax_opt(comments.as_ref())?;
        }
        TypeInner::Boolean {
            loc: _,
            raw: _,
            comments,
        } => {
            visitor.syntax_opt(comments.as_ref())?;
        }
        TypeInner::Exists { loc: _, comments } => {
            visitor.syntax_opt(comments.as_ref())?;
        }
        TypeInner::Unknown { loc: _, comments } => {
            visitor.syntax_opt(comments.as_ref())?;
        }
        TypeInner::Never { loc: _, comments } => {
            visitor.syntax_opt(comments.as_ref())?;
        }
        TypeInner::Undefined { loc: _, comments } => {
            visitor.syntax_opt(comments.as_ref())?;
        }
        TypeInner::UniqueSymbol {
            loc: _,
            symbol_loc: _,
            comments,
        } => {
            visitor.syntax_opt(comments.as_ref())?;
        }
        TypeInner::Nullable { loc: _, inner } => {
            visitor.nullable_type(inner)?;
        }
        TypeInner::Array { loc: _, inner } => {
            visitor.array_type(inner)?;
        }
        TypeInner::Conditional { loc: _, inner } => {
            visitor.conditional_type(inner)?;
        }
        TypeInner::Infer { loc: _, inner } => {
            visitor.infer_type(inner)?;
        }
        TypeInner::Typeof { loc: _, inner } => {
            visitor.typeof_type(inner)?;
        }
        TypeInner::Keyof { loc: _, inner } => {
            visitor.keyof_type(inner)?;
        }
        TypeInner::Renders { loc: _, inner } => {
            visitor.render_type(inner)?;
        }
        TypeInner::ReadOnly { loc: _, inner } => {
            visitor.readonly_type(inner)?;
        }
        TypeInner::Function { loc: _, inner } => {
            visitor.function_type(inner)?;
        }
        TypeInner::ConstructorType { loc: _, inner, .. } => {
            visitor.function_type(inner)?;
        }
        TypeInner::Component { loc, inner } => {
            visitor.component_type(loc, inner)?;
        }
        TypeInner::Object { loc: _, inner } => {
            visitor.object_type(inner)?;
        }
        TypeInner::Interface { loc, inner } => {
            visitor.interface_type(loc, inner)?;
        }
        TypeInner::Generic { loc: _, inner } => {
            visitor.generic_type(inner)?;
        }
        TypeInner::IndexedAccess { loc, inner } => {
            visitor.indexed_access_type(loc, inner)?;
        }
        TypeInner::OptionalIndexedAccess { loc, inner } => {
            visitor.optional_indexed_access_type(loc, inner)?;
        }
        TypeInner::StringLiteral { loc: _, literal } => {
            visitor.string_literal(literal)?;
        }
        TypeInner::NumberLiteral { loc: _, literal } => {
            visitor.number_literal(literal)?;
        }
        TypeInner::BigIntLiteral { loc: _, literal } => {
            visitor.bigint_literal(literal)?;
        }
        TypeInner::BooleanLiteral { loc: _, literal } => {
            visitor.boolean_literal(literal)?;
        }
        TypeInner::Union { loc, inner } => {
            visitor.union_type(loc, inner)?;
        }
        TypeInner::Intersection { loc, inner } => {
            visitor.intersection_type(loc, inner)?;
        }
        TypeInner::Tuple { loc: _, inner } => {
            visitor.tuple_type(inner)?;
        }
        TypeInner::TemplateLiteral { loc: _, inner } => {
            visitor.template_literal_type(inner)?;
        }
    }
    Ok(())
}

pub fn map_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    t: &'ast ast::types::Type<Loc, Loc>,
) -> ast::types::Type<Loc, Loc> {
    match t.deref() {
        TypeInner::Any { loc, comments } => {
            let comments_ = visitor.map_syntax_opt(comments.as_ref());
            ast::types::Type::new(TypeInner::Any {
                loc: loc.dupe(),
                comments: comments_,
            })
        }
        TypeInner::Mixed { loc, comments } => {
            let comments_ = visitor.map_syntax_opt(comments.as_ref());
            ast::types::Type::new(TypeInner::Mixed {
                loc: loc.dupe(),
                comments: comments_,
            })
        }
        TypeInner::Empty { loc, comments } => {
            let comments_ = visitor.map_syntax_opt(comments.as_ref());
            ast::types::Type::new(TypeInner::Empty {
                loc: loc.dupe(),
                comments: comments_,
            })
        }
        TypeInner::Void { loc, comments } => {
            let comments_ = visitor.map_syntax_opt(comments.as_ref());
            ast::types::Type::new(TypeInner::Void {
                loc: loc.dupe(),
                comments: comments_,
            })
        }
        TypeInner::Null { loc, comments } => {
            let comments_ = visitor.map_syntax_opt(comments.as_ref());
            ast::types::Type::new(TypeInner::Null {
                loc: loc.dupe(),
                comments: comments_,
            })
        }
        TypeInner::Symbol { loc, comments } => {
            let comments_ = visitor.map_syntax_opt(comments.as_ref());
            ast::types::Type::new(TypeInner::Symbol {
                loc: loc.dupe(),
                comments: comments_,
            })
        }
        TypeInner::Number { loc, comments } => {
            let comments_ = visitor.map_syntax_opt(comments.as_ref());
            ast::types::Type::new(TypeInner::Number {
                loc: loc.dupe(),
                comments: comments_,
            })
        }
        TypeInner::BigInt { loc, comments } => {
            let comments_ = visitor.map_syntax_opt(comments.as_ref());
            ast::types::Type::new(TypeInner::BigInt {
                loc: loc.dupe(),
                comments: comments_,
            })
        }
        TypeInner::String { loc, comments } => {
            let comments_ = visitor.map_syntax_opt(comments.as_ref());
            ast::types::Type::new(TypeInner::String {
                loc: loc.dupe(),
                comments: comments_,
            })
        }
        TypeInner::Boolean { loc, raw, comments } => {
            let comments_ = visitor.map_syntax_opt(comments.as_ref());
            ast::types::Type::new(TypeInner::Boolean {
                loc: loc.dupe(),
                raw: raw.clone(),
                comments: comments_,
            })
        }
        TypeInner::Exists { loc, comments } => {
            let comments_ = visitor.map_syntax_opt(comments.as_ref());
            ast::types::Type::new(TypeInner::Exists {
                loc: loc.dupe(),
                comments: comments_,
            })
        }
        TypeInner::Unknown { loc, comments } => {
            let comments_ = visitor.map_syntax_opt(comments.as_ref());
            ast::types::Type::new(TypeInner::Unknown {
                loc: loc.dupe(),
                comments: comments_,
            })
        }
        TypeInner::Never { loc, comments } => {
            let comments_ = visitor.map_syntax_opt(comments.as_ref());
            ast::types::Type::new(TypeInner::Never {
                loc: loc.dupe(),
                comments: comments_,
            })
        }
        TypeInner::Undefined { loc, comments } => {
            let comments_ = visitor.map_syntax_opt(comments.as_ref());
            ast::types::Type::new(TypeInner::Undefined {
                loc: loc.dupe(),
                comments: comments_,
            })
        }
        TypeInner::UniqueSymbol {
            loc,
            symbol_loc,
            comments,
        } => {
            let comments_ = visitor.map_syntax_opt(comments.as_ref());
            ast::types::Type::new(TypeInner::UniqueSymbol {
                loc: loc.dupe(),
                symbol_loc: symbol_loc.dupe(),
                comments: comments_,
            })
        }
        TypeInner::Nullable { loc, inner } => {
            let inner_ = visitor.map_nullable_type(loc, inner);
            ast::types::Type::new(TypeInner::Nullable {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::Array { loc, inner } => {
            let inner_ = visitor.map_array_type(loc, inner);
            ast::types::Type::new(TypeInner::Array {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::Conditional { loc, inner } => {
            let inner_ = visitor.map_conditional_type(loc, inner);
            ast::types::Type::new(TypeInner::Conditional {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::Infer { loc, inner } => {
            let inner_ = visitor.map_infer_type(loc, inner);
            ast::types::Type::new(TypeInner::Infer {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::Typeof { loc, inner } => {
            let inner_ = visitor.map_typeof_type(loc, inner);
            ast::types::Type::new(TypeInner::Typeof {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::Keyof { loc, inner } => {
            let inner_ = visitor.map_keyof_type(loc, inner);
            ast::types::Type::new(TypeInner::Keyof {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::Renders { loc, inner } => {
            let inner_ = visitor.map_render_type(loc, inner);
            ast::types::Type::new(TypeInner::Renders {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::ReadOnly { loc, inner } => {
            let inner_ = visitor.map_readonly_type(loc, inner);
            ast::types::Type::new(TypeInner::ReadOnly {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::Function { loc, inner } => {
            let inner_ = visitor.map_function_type(loc, inner);
            ast::types::Type::new(TypeInner::Function {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::ConstructorType {
            loc,
            abstract_,
            inner,
        } => {
            let inner_ = visitor.map_function_type(loc, inner);
            ast::types::Type::new(TypeInner::ConstructorType {
                loc: loc.dupe(),
                abstract_: *abstract_,
                inner: Arc::new(inner_),
            })
        }
        TypeInner::Component { loc, inner } => {
            let inner_ = visitor.map_component_type(loc, inner);
            ast::types::Type::new(TypeInner::Component {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::Object { loc, inner } => {
            let inner_ = visitor.map_object_type(loc, inner);
            ast::types::Type::new(TypeInner::Object {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::Interface { loc, inner } => {
            let inner_ = visitor.map_interface_type(loc, inner);
            ast::types::Type::new(TypeInner::Interface {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::Generic { loc, inner } => {
            let inner_ = visitor.map_generic_type(loc, inner);
            ast::types::Type::new(TypeInner::Generic {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::IndexedAccess { loc, inner } => {
            let inner_ = visitor.map_indexed_access_type(loc, inner);
            ast::types::Type::new(TypeInner::IndexedAccess {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::OptionalIndexedAccess { loc, inner } => {
            let inner_ = visitor.map_optional_indexed_access_type(loc, inner);
            ast::types::Type::new(TypeInner::OptionalIndexedAccess {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::StringLiteral { loc, literal } => {
            ast::types::Type::new(TypeInner::StringLiteral {
                loc: loc.dupe(),
                literal: visitor.map_string_literal(literal),
            })
        }
        TypeInner::NumberLiteral { loc, literal } => {
            ast::types::Type::new(TypeInner::NumberLiteral {
                loc: loc.dupe(),
                literal: visitor.map_number_literal(literal),
            })
        }
        TypeInner::BigIntLiteral { loc, literal } => {
            ast::types::Type::new(TypeInner::BigIntLiteral {
                loc: loc.dupe(),
                literal: visitor.map_bigint_literal(literal),
            })
        }
        TypeInner::BooleanLiteral { loc, literal } => {
            ast::types::Type::new(TypeInner::BooleanLiteral {
                loc: loc.dupe(),
                literal: visitor.map_boolean_literal(literal),
            })
        }
        TypeInner::Union { loc, inner } => {
            let inner_ = visitor.map_union_type(loc, inner);
            ast::types::Type::new(TypeInner::Union {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::Intersection { loc, inner } => {
            let inner_ = visitor.map_intersection_type(loc, inner);
            ast::types::Type::new(TypeInner::Intersection {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::Tuple { loc, inner } => {
            let inner_ = visitor.map_tuple_type(loc, inner);
            ast::types::Type::new(TypeInner::Tuple {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
        TypeInner::TemplateLiteral { loc, inner } => {
            let inner_ = visitor.map_template_literal_type(loc, inner);
            ast::types::Type::new(TypeInner::TemplateLiteral {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            })
        }
    }
}

pub fn type_annotation_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    annot: &'ast ast::types::Annotation<Loc, Type>,
) -> Result<(), E> {
    let ast::types::Annotation { loc: _, annotation } = annot;
    visitor.type_(annotation)?;
    Ok(())
}

pub fn map_type_annotation_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    annot: &'ast ast::types::Annotation<Loc, Loc>,
) -> ast::types::Annotation<Loc, Loc> {
    let ast::types::Annotation { loc, annotation } = annot;
    let annotation_ = visitor.map_type_(annotation);
    ast::types::Annotation {
        loc: loc.dupe(),
        annotation: annotation_,
    }
}

pub fn type_annotation_hint_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    return_: &'ast ast::types::AnnotationOrHint<Loc, Type>,
) -> Result<(), E> {
    match return_ {
        ast::types::AnnotationOrHint::Available(annot) => {
            visitor.type_annotation(annot)?;
        }
        ast::types::AnnotationOrHint::Missing(_loc) => {}
    }
    Ok(())
}

pub fn map_type_annotation_hint_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    hint: &'ast ast::types::AnnotationOrHint<Loc, Loc>,
) -> ast::types::AnnotationOrHint<Loc, Loc> {
    match hint {
        ast::types::AnnotationOrHint::Available(annot) => {
            let annot_ = visitor.map_type_annotation(annot);
            ast::types::AnnotationOrHint::Available(annot_)
        }
        ast::types::AnnotationOrHint::Missing(loc) => {
            ast::types::AnnotationOrHint::Missing(loc.dupe())
        }
    }
}

pub fn component_renders_annotation_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    renders: &'ast ast::types::ComponentRendersAnnotation<Loc, Type>,
) -> Result<(), E> {
    match renders {
        ast::types::ComponentRendersAnnotation::AvailableRenders(_, type_) => {
            visitor.render_type(type_)?;
        }
        ast::types::ComponentRendersAnnotation::MissingRenders(_) => {}
    }
    Ok(())
}

pub fn map_component_renders_annotation_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    renders: &'ast ast::types::ComponentRendersAnnotation<Loc, Loc>,
) -> ast::types::ComponentRendersAnnotation<Loc, Loc> {
    match renders {
        ast::types::ComponentRendersAnnotation::AvailableRenders(loc, type_) => {
            let type__ = visitor.map_render_type(loc, type_);
            ast::types::ComponentRendersAnnotation::AvailableRenders(loc.dupe(), type__)
        }
        ast::types::ComponentRendersAnnotation::MissingRenders(loc) => {
            ast::types::ComponentRendersAnnotation::MissingRenders(loc.dupe())
        }
    }
}

pub fn function_declaration_default<
    'ast,
    Loc: Dupe,
    Type: Dupe,
    C,
    E,
    V: AstVisitor<'ast, Loc, Type, C, E> + ?Sized,
>(
    visitor: &mut V,
    loc: &'ast Loc,
    stmt: &'ast ast::function::Function<Loc, Type>,
) -> Result<(), E> {
    visitor.function_(V::normalize_loc(loc), stmt)?;
    Ok(())
}

pub fn map_function_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    stmt: &'ast ast::function::Function<Loc, Loc>,
) -> ast::function::Function<Loc, Loc> {
    visitor.map_function_(loc, stmt)
}

pub fn function_expression_default<
    'ast,
    Loc: Dupe,
    Type: Dupe,
    C,
    E,
    V: AstVisitor<'ast, Loc, Type, C, E> + ?Sized,
>(
    visitor: &mut V,
    loc: &'ast Type,
    stmt: &'ast ast::function::Function<Loc, Type>,
) -> Result<(), E> {
    visitor.function_expression_or_method(V::normalize_type(loc), stmt)?;
    Ok(())
}

pub fn map_function_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    stmt: &'ast ast::function::Function<Loc, Loc>,
) -> ast::function::Function<Loc, Loc> {
    visitor.map_function_(loc, stmt)
}

pub fn function_expression_or_method_default<
    'ast,
    Loc: Dupe,
    Type: Dupe,
    C,
    E,
    V: AstVisitor<'ast, Loc, Type, C, E> + ?Sized,
>(
    visitor: &mut V,
    loc: C,
    stmt: &'ast ast::function::Function<Loc, Type>,
) -> Result<(), E> {
    visitor.function_(loc, stmt)?;
    Ok(())
}

pub fn map_function_expression_or_method_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    stmt: &'ast ast::function::Function<Loc, Loc>,
) -> ast::function::Function<Loc, Loc> {
    visitor.map_function_(loc, stmt)
}

pub fn function_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: C,
    expr: &'ast ast::function::Function<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::function::Function {
        id,
        params,
        body,
        async_: _,
        generator: _,
        effect_: _,
        predicate,
        return_,
        tparams,
        sig_loc: _,
        comments,
    } = expr;
    if let Some(id) = id {
        visitor.function_identifier(id)?;
    }
    if let Some(tparams) = tparams {
        visitor.type_params(&TypeParamsContext::Function, tparams)?;
    }
    visitor.function_params(params)?;
    visitor.function_return_annotation(return_)?;
    visitor.function_body_any(body)?;
    if let Some(predicate) = predicate {
        visitor.predicate(predicate)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_function_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::function::Function<Loc, Loc>,
) -> ast::function::Function<Loc, Loc> {
    let ast::function::Function {
        id,
        params,
        body,
        async_,
        generator,
        effect_,
        predicate,
        return_,
        tparams,
        comments,
        sig_loc,
    } = expr;

    let id_ = id.as_ref().map(|id| visitor.map_function_identifier(id));
    let tparams_ = tparams
        .as_ref()
        .map(|tp| visitor.map_type_params(&TypeParamsContext::Function, tp));
    let params_ = visitor.map_function_params(params);
    let return__ = visitor.map_function_return_annotation(return_);
    let predicate_ = predicate.as_ref().map(|p| visitor.map_predicate(p));
    let body_ = visitor.map_function_body_any(body);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::function::Function {
        id: id_,
        params: params_,
        body: body_,
        async_: *async_,
        generator: *generator,
        effect_: *effect_,
        predicate: predicate_,
        return_: return__,
        tparams: tparams_,
        comments: comments_,
        sig_loc: sig_loc.dupe(),
    }
}

pub fn function_params_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    params: &'ast ast::function::Params<Loc, Type>,
) -> Result<(), E> {
    let ast::function::Params {
        loc: _,
        params: params_list,
        rest,
        comments,
        this_,
    } = params;
    for param in params_list.iter() {
        visitor.function_param(param)?;
    }
    if let Some(rest) = rest {
        visitor.function_rest_param(rest)?;
    }
    if let Some(this_) = this_ {
        visitor.function_this_param(this_)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_function_params_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    params: &'ast ast::function::Params<Loc, Loc>,
) -> ast::function::Params<Loc, Loc> {
    let ast::function::Params {
        loc,
        this_,
        params: param_list,
        rest,
        comments,
    } = params;

    let this__ = this_.as_ref().map(|t| visitor.map_function_this_param(t));
    let params_: Arc<[ast::function::Param<Loc, Loc>]> = param_list
        .iter()
        .map(|p| visitor.map_function_param(p))
        .collect();
    let rest_ = rest.as_ref().map(|r| visitor.map_function_rest_param(r));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::function::Params {
        loc: loc.dupe(),
        this_: this__,
        params: params_,
        rest: rest_,
        comments: comments_,
    }
}

pub fn function_this_param_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    this_param: &'ast ast::function::ThisParam<Loc, Type>,
) -> Result<(), E> {
    let ast::function::ThisParam {
        loc: _,
        annot,
        comments,
    } = this_param;
    visitor.type_annotation(annot)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_function_this_param_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    this_param: &'ast ast::function::ThisParam<Loc, Loc>,
) -> ast::function::ThisParam<Loc, Loc> {
    let ast::function::ThisParam {
        loc,
        annot,
        comments,
    } = this_param;

    let annot_ = visitor.map_type_annotation(annot);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::function::ThisParam {
        loc: loc.dupe(),
        annot: annot_,
        comments: comments_,
    }
}

pub fn function_param_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    param: &'ast ast::function::Param<Loc, Type>,
) -> Result<(), E> {
    match param {
        ast::function::Param::RegularParam {
            loc: _,
            argument,
            default,
        } => {
            visitor.function_param_pattern(argument)?;
            visitor.default_opt(default.as_ref())?;
        }
        ast::function::Param::ParamProperty { loc: _, property } => {
            visitor.class_property(property)?;
        }
    }
    Ok(())
}

pub fn map_function_param_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    param: &'ast ast::function::Param<Loc, Loc>,
) -> ast::function::Param<Loc, Loc> {
    match param {
        ast::function::Param::RegularParam {
            loc,
            argument,
            default,
        } => {
            let argument_ = visitor.map_function_param_pattern(argument);
            let default_ = default.as_ref().map(|d| visitor.map_expression(d));
            ast::function::Param::RegularParam {
                loc: loc.dupe(),
                argument: argument_,
                default: default_,
            }
        }
        ast::function::Param::ParamProperty { loc, property } => {
            let property_ = visitor.map_class_property(property);
            ast::function::Param::ParamProperty {
                loc: loc.dupe(),
                property: property_,
            }
        }
    }
}

pub fn function_return_annotation_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    return_: &'ast ast::function::ReturnAnnot<Loc, Type>,
) -> Result<(), E> {
    match return_ {
        ast::function::ReturnAnnot::Missing(_) => {}
        ast::function::ReturnAnnot::Available(annot) => {
            visitor.type_annotation(annot)?;
        }
        ast::function::ReturnAnnot::TypeGuard(guard) => {
            visitor.type_guard_annotation(guard)?;
        }
    }
    Ok(())
}

pub fn map_function_return_annotation_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    return_: &'ast ast::function::ReturnAnnot<Loc, Loc>,
) -> ast::function::ReturnAnnot<Loc, Loc> {
    match return_ {
        ast::function::ReturnAnnot::Missing(loc) => ast::function::ReturnAnnot::Missing(loc.dupe()),
        ast::function::ReturnAnnot::Available(annot) => {
            ast::function::ReturnAnnot::Available(visitor.map_type_annotation(annot))
        }
        ast::function::ReturnAnnot::TypeGuard(guard) => {
            ast::function::ReturnAnnot::TypeGuard(visitor.map_type_guard_annotation(guard))
        }
    }
}

pub fn function_body_any_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    body: &'ast ast::function::Body<Loc, Type>,
) -> Result<(), E> {
    match body {
        ast::function::Body::BodyBlock(block) => {
            visitor.function_body(block)?;
        }
        ast::function::Body::BodyExpression(expr) => {
            visitor.body_expression(expr)?;
        }
    }
    Ok(())
}

pub fn map_function_body_any_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    body: &'ast ast::function::Body<Loc, Loc>,
) -> ast::function::Body<Loc, Loc> {
    match body {
        ast::function::Body::BodyBlock(block) => {
            ast::function::Body::BodyBlock(visitor.map_function_body(block))
        }
        ast::function::Body::BodyExpression(expr) => {
            ast::function::Body::BodyExpression(visitor.map_body_expression(expr))
        }
    }
}

pub fn function_body_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    body: &'ast (Loc, ast::statement::Block<Loc, Type>),
) -> Result<(), E> {
    let (loc, block) = body;
    visitor.block(loc, block)?;
    Ok(())
}

pub fn map_function_body_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    body: &'ast (Loc, ast::statement::Block<Loc, Loc>),
) -> (Loc, ast::statement::Block<Loc, Loc>) {
    let (loc, block) = body;
    let block_ = visitor.map_block(loc, block);
    (loc.dupe(), block_)
}

pub fn body_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::expression::Expression<Loc, Type>,
) -> Result<(), E> {
    visitor.expression(expr)?;
    Ok(())
}

pub fn map_body_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::expression::Expression<Loc, Loc>,
) -> ast::expression::Expression<Loc, Loc> {
    visitor.map_expression(expr)
}

pub fn function_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ident: &'ast ast::Identifier<Loc, Type>,
) -> Result<(), E> {
    visitor.pattern_identifier(Some(ast::VariableKind::Var), ident)?;
    Ok(())
}

pub fn map_function_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ident: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    visitor.map_identifier(ident)
}

pub fn identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Type>,
) -> Result<(), E> {
    let ast::IdentifierInner {
        loc: _,
        name: _,
        comments,
    } = id.deref();
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    let ast::IdentifierInner {
        loc,
        name,
        comments,
    } = id.deref();
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::Identifier::new(ast::IdentifierInner {
        loc: loc.dupe(),
        name: name.dupe(),
        comments: comments_,
    })
}

pub fn untyped_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Loc>,
) -> Result<(), E> {
    let ast::IdentifierInner {
        loc: _,
        name: _,
        comments,
    } = id.deref();
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn type_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Type>,
) -> Result<(), E> {
    visitor.identifier(id)?;
    Ok(())
}

pub fn map_type_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    visitor.map_identifier(id)
}

pub fn type_identifier_reference_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Type>,
) -> Result<(), E> {
    visitor.type_identifier(id)?;
    Ok(())
}

pub fn map_type_identifier_reference_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    visitor.map_type_identifier(id)
}

pub fn binding_type_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Type>,
) -> Result<(), E> {
    visitor.type_identifier(id)?;
    Ok(())
}

pub fn map_binding_type_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    visitor.map_type_identifier(id)
}

pub fn interface_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    interface: &'ast ast::statement::Interface<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::statement::Interface {
        id,
        tparams,
        extends,
        body,
        comments,
    } = interface;
    visitor.binding_type_identifier(id)?;
    if let Some(tparams) = tparams {
        visitor.type_params(&TypeParamsContext::Interface, tparams)?;
    }
    for (_loc, generic) in extends.iter() {
        visitor.generic_type(generic)?;
    }
    let (_loc, obj_type) = body;
    visitor.object_type(obj_type)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_interface_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    interface: &'ast ast::statement::Interface<Loc, Loc>,
) -> ast::statement::Interface<Loc, Loc> {
    let ast::statement::Interface {
        id,
        tparams,
        extends,
        body,
        comments,
    } = interface;
    let id_ = visitor.map_binding_type_identifier(id);
    let tparams_ = tparams
        .as_ref()
        .map(|tp| visitor.map_type_params(&TypeParamsContext::Interface, tp));
    let extends_: Arc<[(Loc, ast::types::Generic<Loc, Loc>)]> = extends
        .iter()
        .map(|(loc, generic)| (loc.dupe(), visitor.map_generic_type(loc, generic)))
        .collect();
    let (body_loc, obj_type) = body;
    let body_ = (body_loc.dupe(), visitor.map_object_type(body_loc, obj_type));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::Interface {
        id: id_,
        tparams: tparams_,
        extends: extends_,
        body: body_,
        comments: comments_,
    }
}

pub fn interface_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    decl: &'ast ast::statement::Interface<Loc, Type>,
) -> Result<(), E> {
    visitor.interface(loc, decl)?;
    Ok(())
}

pub fn map_interface_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    decl: &'ast ast::statement::Interface<Loc, Loc>,
) -> ast::statement::Interface<Loc, Loc> {
    visitor.map_interface(loc, decl)
}

pub fn private_name_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::PrivateName<Loc>,
) -> Result<(), E> {
    let ast::PrivateName {
        loc: _,
        name: _,
        comments,
    } = id;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_private_name_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::PrivateName<Loc>,
) -> ast::PrivateName<Loc> {
    let ast::PrivateName {
        loc,
        name,
        comments,
    } = id;
    ast::PrivateName {
        loc: loc.dupe(),
        name: name.dupe(),
        comments: visitor.map_syntax_opt(comments.as_ref()),
    }
}

pub fn computed_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    key: &'ast ast::ComputedKey<Loc, Type>,
) -> Result<(), E> {
    let ast::ComputedKey {
        loc: _,
        expression,
        comments,
    } = key;
    visitor.expression(expression)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_computed_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    key: &'ast ast::ComputedKey<Loc, Loc>,
) -> ast::ComputedKey<Loc, Loc> {
    let ast::ComputedKey {
        loc,
        expression,
        comments,
    } = key;
    let expression_ = visitor.map_expression(expression);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::ComputedKey {
        loc: loc.dupe(),
        expression: expression_,
        comments: comments_,
    }
}

pub fn import_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::expression::Import<Loc, Type>,
) -> Result<(), E> {
    let ast::expression::Import {
        argument,
        options,
        phase: _,
        comments,
    } = expr;
    visitor.expression(argument)?;
    if let Some(opts) = options {
        visitor.expression(opts)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_import_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::expression::Import<Loc, Loc>,
) -> ast::expression::Import<Loc, Loc> {
    let ast::expression::Import {
        argument,
        options,
        phase: _,
        comments,
    } = expr;
    let argument_ = visitor.map_expression(argument);
    let options_ = options.as_ref().map(|opts| visitor.map_expression(opts));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::Import {
        phase: expr.phase,
        argument: argument_,
        options: options_,
        comments: comments_,
    }
}

pub fn if_consequent_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    has_else: bool,
    stmt: &'ast ast::statement::Statement<Loc, Type>,
) -> Result<(), E> {
    let _ = has_else;
    visitor.statement(stmt)?;
    Ok(())
}

pub fn map_if_consequent_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    has_else: bool,
    stmt: &'ast ast::statement::Statement<Loc, Loc>,
) -> ast::statement::Statement<Loc, Loc> {
    let _ = has_else;
    visitor.map_statement(stmt)
}

pub fn if_alternate_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    altern: &'ast ast::statement::if_::Alternate<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::if_::Alternate {
        loc: _,
        body,
        comments,
    } = altern;
    visitor.statement(body)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_if_alternate_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    altern: &'ast ast::statement::if_::Alternate<Loc, Loc>,
) -> ast::statement::if_::Alternate<Loc, Loc> {
    let ast::statement::if_::Alternate {
        loc,
        body,
        comments,
    } = altern;
    let body_ = visitor.map_statement(body);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::if_::Alternate {
        loc: loc.dupe(),
        body: body_,
        comments: comments_,
    }
}

pub fn if_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    stmt: &'ast ast::statement::If<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::statement::If {
        test,
        consequent,
        alternate,
        comments,
    } = stmt;
    visitor.predicate_expression(test)?;
    let has_else = alternate.is_some();
    visitor.if_consequent_statement(has_else, consequent)?;
    if let Some(alternate) = alternate {
        visitor.if_alternate_statement(alternate)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_if_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    stmt: &'ast ast::statement::If<Loc, Loc>,
) -> ast::statement::If<Loc, Loc> {
    let ast::statement::If {
        test,
        consequent,
        alternate,
        comments,
    } = stmt;
    let test_ = visitor.map_expression(test);
    let consequent_ = visitor.map_statement(consequent);
    let alternate_ = alternate
        .as_ref()
        .map(|a| visitor.map_if_alternate_statement(a));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::If {
        test: test_,
        consequent: consequent_,
        alternate: alternate_,
        comments: comments_,
    }
}

pub fn import_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    decl: &'ast ast::statement::ImportDeclaration<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::statement::ImportDeclaration {
        import_kind,
        phase: _,
        source,
        specifiers,
        default,
        attributes,
        comments,
    } = decl;
    let (loc, src) = source;
    visitor.import_source(loc, src)?;
    if let Some(specifiers) = specifiers {
        visitor.import_specifier(*import_kind, specifiers)?;
    }
    if let Some(default) = default {
        visitor.import_default_specifier(import_kind, &default.identifier)?;
    }
    if let Some((loc, attrs)) = attributes {
        visitor.import_attributes(loc, attrs)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_import_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::ImportDeclaration<Loc, Loc>,
) -> ast::statement::ImportDeclaration<Loc, Loc> {
    let ast::statement::ImportDeclaration {
        import_kind,
        phase,
        source,
        default,
        specifiers,
        attributes,
        comments,
    } = decl;
    let (source_loc, source_lit) = source;
    let source_lit_ = visitor.map_import_source(source_loc, source_lit);
    let source_ = (source_loc.dupe(), source_lit_);
    let default_ = default.as_ref().map(|d| {
        let id_ = visitor.map_import_default_specifier(import_kind, &d.identifier);
        ast::statement::import_declaration::DefaultIdentifier {
            identifier: id_,
            remote_default_name_def_loc: d.remote_default_name_def_loc.dupe(),
        }
    });
    let specifiers_ = specifiers
        .as_ref()
        .map(|spec| visitor.map_import_specifier(*import_kind, spec));
    let attributes_ = attributes.as_ref().map(|(loc, attrs)| {
        let attrs_ = visitor.map_import_attributes(loc, attrs);
        (loc.dupe(), attrs_)
    });
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::ImportDeclaration {
        import_kind: *import_kind,
        phase: *phase,
        source: source_,
        default: default_,
        specifiers: specifiers_,
        attributes: attributes_,
        comments: comments_,
    }
}

pub fn import_equals_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::ImportEqualsDeclaration<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::ImportEqualsDeclaration {
        id,
        module_reference,
        import_kind,
        is_export: _,
        comments,
    } = decl;
    match import_kind {
        ast::statement::ImportKind::ImportType | ast::statement::ImportKind::ImportTypeof => {
            visitor.binding_type_identifier(id)?;
        }
        ast::statement::ImportKind::ImportValue => {
            visitor.pattern_identifier(Some(ast::VariableKind::Let), id)?;
        }
    }
    visitor.import_equals_module_reference(module_reference)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_import_equals_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::ImportEqualsDeclaration<Loc, Loc>,
) -> ast::statement::ImportEqualsDeclaration<Loc, Loc> {
    let ast::statement::ImportEqualsDeclaration {
        id,
        module_reference,
        import_kind,
        is_export,
        comments,
    } = decl;
    let id_ = match import_kind {
        ast::statement::ImportKind::ImportType | ast::statement::ImportKind::ImportTypeof => {
            visitor.map_binding_type_identifier(id)
        }
        ast::statement::ImportKind::ImportValue => visitor.map_identifier(id),
    };
    let module_reference_ = visitor.map_import_equals_module_reference(module_reference);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::ImportEqualsDeclaration {
        id: id_,
        module_reference: module_reference_,
        import_kind: *import_kind,
        is_export: *is_export,
        comments: comments_,
    }
}

pub fn import_equals_module_reference_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    module_ref: &'ast ast::statement::import_equals_declaration::ModuleReference<Loc, Type>,
) -> Result<(), E> {
    match module_ref {
        ast::statement::import_equals_declaration::ModuleReference::ExternalModuleReference(
            _annot,
            lit,
        ) => {
            visitor.string_literal(lit)?;
        }
        ast::statement::import_equals_declaration::ModuleReference::Identifier(ident) => {
            visitor.generic_identifier_type(ident)?;
        }
    }
    Ok(())
}

pub fn map_import_equals_module_reference_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    module_ref: &'ast ast::statement::import_equals_declaration::ModuleReference<Loc, Loc>,
) -> ast::statement::import_equals_declaration::ModuleReference<Loc, Loc> {
    match module_ref {
        ast::statement::import_equals_declaration::ModuleReference::ExternalModuleReference(
            annot,
            lit,
        ) => {
            let lit_ = visitor.map_string_literal(lit);
            ast::statement::import_equals_declaration::ModuleReference::ExternalModuleReference(
                annot.dupe(),
                lit_,
            )
        }
        ast::statement::import_equals_declaration::ModuleReference::Identifier(ident) => {
            let ident_ = visitor.map_generic_identifier_type(ident);
            ast::statement::import_equals_declaration::ModuleReference::Identifier(ident_)
        }
    }
}

pub fn import_source_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    source: &'ast ast::StringLiteral<Loc>,
) -> Result<(), E> {
    let _ = loc;
    let ast::StringLiteral {
        value: _,
        raw: _,
        comments,
    } = source;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_import_source_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    source: &'ast ast::StringLiteral<Loc>,
) -> ast::StringLiteral<Loc> {
    let ast::StringLiteral {
        value,
        raw,
        comments,
    } = source;
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::StringLiteral {
        value: value.clone(),
        raw: raw.clone(),
        comments: comments_,
    }
}

pub fn import_attributes_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    attrs: &'ast [ast::statement::import_declaration::ImportAttribute<Loc, Type>],
) -> Result<(), E> {
    for attr in attrs {
        visitor.import_attribute(attr)?;
    }
    Ok(())
}

pub fn map_import_attributes_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    attrs: &'ast [ast::statement::import_declaration::ImportAttribute<Loc, Loc>],
) -> Vec<ast::statement::import_declaration::ImportAttribute<Loc, Loc>> {
    attrs
        .iter()
        .map(|attr| visitor.map_import_attribute(attr))
        .collect()
}

pub fn import_attribute_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    attr: &'ast ast::statement::import_declaration::ImportAttribute<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::import_declaration::ImportAttribute { loc: _, key, value } = attr;
    visitor.import_attribute_key(key)?;
    let (_loc, lit) = value;
    visitor.string_literal(lit)?;
    Ok(())
}

pub fn map_import_attribute_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    attr: &'ast ast::statement::import_declaration::ImportAttribute<Loc, Loc>,
) -> ast::statement::import_declaration::ImportAttribute<Loc, Loc> {
    let ast::statement::import_declaration::ImportAttribute { loc, key, value } = attr;
    let key_ = visitor.map_import_attribute_key(key);
    let (value_loc, value_lit) = value;
    let value_ = (value_loc.dupe(), visitor.map_string_literal(value_lit));
    ast::statement::import_declaration::ImportAttribute {
        loc: loc.dupe(),
        key: key_,
        value: value_,
    }
}

pub fn import_attribute_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    key: &'ast ast::statement::import_declaration::ImportAttributeKey<Loc, Type>,
) -> Result<(), E> {
    use ast::statement::import_declaration::ImportAttributeKey;
    match key {
        ImportAttributeKey::Identifier(id) => visitor.identifier(id)?,
        ImportAttributeKey::StringLiteral(_loc, lit) => visitor.string_literal(lit)?,
    }
    Ok(())
}

pub fn map_import_attribute_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    key: &'ast ast::statement::import_declaration::ImportAttributeKey<Loc, Loc>,
) -> ast::statement::import_declaration::ImportAttributeKey<Loc, Loc> {
    use ast::statement::import_declaration::ImportAttributeKey;
    match key {
        ImportAttributeKey::Identifier(id) => {
            let id_ = visitor.map_identifier(id);
            ImportAttributeKey::Identifier(id_)
        }
        ImportAttributeKey::StringLiteral(loc, lit) => {
            ImportAttributeKey::StringLiteral(loc.dupe(), visitor.map_string_literal(lit))
        }
    }
}

pub fn import_specifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    import_kind: ast::statement::ImportKind,
    spec: &'ast ast::statement::import_declaration::Specifier<Loc, Type>,
) -> Result<(), E> {
    match spec {
        ast::statement::import_declaration::Specifier::ImportNamedSpecifiers(named_specifiers) => {
            for named_spec in named_specifiers.iter() {
                visitor.import_named_specifier(import_kind, named_spec)?;
            }
        }
        ast::statement::import_declaration::Specifier::ImportNamespaceSpecifier((loc, ident)) => {
            visitor.import_namespace_specifier(import_kind, loc, ident)?;
        }
    }
    Ok(())
}

pub fn map_import_specifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    import_kind: ast::statement::ImportKind,
    spec: &'ast ast::statement::import_declaration::Specifier<Loc, Loc>,
) -> ast::statement::import_declaration::Specifier<Loc, Loc> {
    match spec {
        ast::statement::import_declaration::Specifier::ImportNamedSpecifiers(named_specifiers) => {
            let named_specifiers_ = named_specifiers
                .iter()
                .map(|s| visitor.map_import_named_specifier(import_kind, s))
                .collect();
            ast::statement::import_declaration::Specifier::ImportNamedSpecifiers(named_specifiers_)
        }
        ast::statement::import_declaration::Specifier::ImportNamespaceSpecifier((loc, ident)) => {
            let ident_ = visitor.map_import_namespace_specifier(import_kind, loc, ident);
            ast::statement::import_declaration::Specifier::ImportNamespaceSpecifier((
                loc.dupe(),
                ident_,
            ))
        }
    }
}

pub fn remote_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Type>,
) -> Result<(), E> {
    visitor.identifier(id)?;
    Ok(())
}

pub fn map_remote_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    visitor.map_identifier(id)
}

pub fn import_named_specifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    import_kind: ast::statement::ImportKind,
    spec: &'ast ast::statement::import_declaration::NamedSpecifier<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::import_declaration::NamedSpecifier {
        kind,
        kind_loc: _,
        local,
        remote,
        remote_name_def_loc: _,
    } = spec;
    let (is_type_remote, is_type_local) = match (import_kind, kind) {
        (ast::statement::ImportKind::ImportType, _)
        | (_, Some(ast::statement::ImportKind::ImportType)) => (true, true),
        (ast::statement::ImportKind::ImportTypeof, _)
        | (_, Some(ast::statement::ImportKind::ImportTypeof)) => (false, true),
        _ => (false, false),
    };
    if local.is_none() {
        if is_type_remote {
            visitor.binding_type_identifier(remote)?;
        } else {
            visitor.pattern_identifier(Some(ast::VariableKind::Let), remote)?;
        }
    } else {
        visitor.remote_identifier(remote)?;
    }
    if let Some(ident) = local {
        if is_type_local {
            visitor.binding_type_identifier(ident)?;
        } else {
            visitor.pattern_identifier(Some(ast::VariableKind::Let), ident)?;
        }
    }
    Ok(())
}

pub fn map_import_named_specifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    import_kind: ast::statement::ImportKind,
    spec: &'ast ast::statement::import_declaration::NamedSpecifier<Loc, Loc>,
) -> ast::statement::import_declaration::NamedSpecifier<Loc, Loc> {
    let ast::statement::import_declaration::NamedSpecifier {
        kind,
        kind_loc,
        local,
        remote,
        remote_name_def_loc,
    } = spec;
    let (_is_type_remote, _is_type_local) = match (import_kind, kind) {
        (ast::statement::ImportKind::ImportType, _)
        | (_, Some(ast::statement::ImportKind::ImportType)) => (true, true),
        (ast::statement::ImportKind::ImportTypeof, _)
        | (_, Some(ast::statement::ImportKind::ImportTypeof)) => (false, true),
        _ => (false, false),
    };
    let remote_ = match local {
        None => visitor.map_identifier(remote),
        Some(_) => visitor.map_remote_identifier(remote),
    };
    let local_ = match local {
        None => None,
        Some(ident) => Some(visitor.map_identifier(ident)),
    };
    ast::statement::import_declaration::NamedSpecifier {
        kind: *kind,
        kind_loc: kind_loc.dupe(),
        local: local_,
        remote: remote_,
        remote_name_def_loc: remote_name_def_loc.dupe(),
    }
}

pub fn import_default_specifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    import_kind: &'ast ast::statement::ImportKind,
    id: &'ast ast::Identifier<Loc, Type>,
) -> Result<(), E> {
    match import_kind {
        ast::statement::ImportKind::ImportType | ast::statement::ImportKind::ImportTypeof => {
            visitor.binding_type_identifier(id)?;
        }
        _ => {
            visitor.pattern_identifier(Some(ast::VariableKind::Let), id)?;
        }
    }
    Ok(())
}

pub fn map_import_default_specifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _import_kind: &'ast ast::statement::ImportKind,
    id: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    visitor.map_identifier(id)
}

pub fn import_namespace_specifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    import_kind: ast::statement::ImportKind,
    loc: &'ast Loc,
    id: &'ast ast::Identifier<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    match import_kind {
        ast::statement::ImportKind::ImportType | ast::statement::ImportKind::ImportTypeof => {
            visitor.binding_type_identifier(id)?;
        }
        _ => {
            visitor.pattern_identifier(Some(ast::VariableKind::Let), id)?;
        }
    }
    Ok(())
}

pub fn map_import_namespace_specifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _import_kind: ast::statement::ImportKind,
    _loc: &'ast Loc,
    id: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    visitor.map_identifier(id)
}

pub fn jsx_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    expr: &'ast ast::jsx::Element<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::jsx::Element {
        opening_element,
        closing_element,
        children,
        comments,
    } = expr;
    visitor.jsx_opening_element(opening_element)?;
    if let Some(closing) = closing_element {
        visitor.jsx_closing_element(closing)?;
    }
    visitor.jsx_children(&children.0, &children.1)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_jsx_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::jsx::Element<Loc, Loc>,
) -> ast::jsx::Element<Loc, Loc> {
    let ast::jsx::Element {
        opening_element,
        closing_element,
        children,
        comments,
    } = expr;

    let opening_element_ = visitor.map_jsx_opening_element(opening_element);
    let closing_element_ = closing_element
        .as_ref()
        .map(|c| visitor.map_jsx_closing_element(c));
    let (children_loc, children_list) = children;
    let children_ = visitor.map_jsx_children(children_loc, children_list);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::jsx::Element {
        opening_element: opening_element_,
        closing_element: closing_element_,
        children: (children_loc.dupe(), children_),
        comments: comments_,
    }
}

pub fn jsx_fragment_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    expr: &'ast ast::jsx::Fragment<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::jsx::Fragment {
        frag_opening_element: _,
        frag_closing_element: _,
        frag_children,
        frag_comments,
    } = expr;
    visitor.jsx_children(&frag_children.0, &frag_children.1)?;
    visitor.syntax_opt(frag_comments.as_ref())?;
    Ok(())
}

pub fn map_jsx_fragment_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::jsx::Fragment<Loc, Loc>,
) -> ast::jsx::Fragment<Loc, Loc> {
    let ast::jsx::Fragment {
        frag_opening_element,
        frag_closing_element,
        frag_children,
        frag_comments,
    } = expr;

    let (children_loc, children_list) = frag_children;
    let children_ = visitor.map_jsx_children(children_loc, children_list);
    let comments_ = visitor.map_syntax_opt(frag_comments.as_ref());

    ast::jsx::Fragment {
        frag_opening_element: frag_opening_element.clone(),
        frag_closing_element: frag_closing_element.clone(),
        frag_children: (children_loc.dupe(), children_),
        frag_comments: comments_,
    }
}

pub fn jsx_opening_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    elem: &'ast ast::jsx::Opening<Loc, Type>,
) -> Result<(), E> {
    let ast::jsx::Opening {
        loc: _,
        name,
        targs,
        self_closing: _,
        attributes,
        props: _,
    } = elem;
    visitor.jsx_element_name(name)?;
    if let Some(targs) = targs {
        visitor.call_type_args(targs)?;
    }
    for attr in attributes.iter() {
        visitor.jsx_opening_attribute(attr)?;
    }
    Ok(())
}

pub fn map_jsx_opening_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    elem: &'ast ast::jsx::Opening<Loc, Loc>,
) -> ast::jsx::Opening<Loc, Loc> {
    let ast::jsx::Opening {
        loc,
        name,
        targs,
        self_closing,
        attributes,
        props,
    } = elem;

    let name_ = visitor.map_jsx_element_name(name);
    let targs_ = targs.as_ref().map(|t| visitor.map_call_type_args(t));
    let attributes_: Arc<[ast::jsx::OpeningAttribute<Loc, Loc>]> = attributes
        .iter()
        .map(|attr| visitor.map_jsx_opening_attribute(attr))
        .collect();

    ast::jsx::Opening {
        loc: loc.dupe(),
        name: name_,
        targs: targs_,
        self_closing: *self_closing,
        attributes: attributes_,
        props: props.dupe(),
    }
}

pub fn jsx_closing_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    elem: &'ast ast::jsx::Closing<Loc, Type>,
) -> Result<(), E> {
    let ast::jsx::Closing { loc: _, name } = elem;
    visitor.jsx_element_name(name)?;
    Ok(())
}

pub fn map_jsx_closing_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    elem: &'ast ast::jsx::Closing<Loc, Loc>,
) -> ast::jsx::Closing<Loc, Loc> {
    let ast::jsx::Closing { loc, name } = elem;

    let name_ = visitor.map_jsx_element_name(name);

    ast::jsx::Closing {
        loc: loc.dupe(),
        name: name_,
    }
}

pub fn jsx_opening_attribute_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    jsx_attr: &'ast ast::jsx::OpeningAttribute<Loc, Type>,
) -> Result<(), E> {
    match jsx_attr {
        ast::jsx::OpeningAttribute::Attribute(attr) => {
            visitor.jsx_attribute(attr)?;
        }
        ast::jsx::OpeningAttribute::SpreadAttribute(attr) => {
            visitor.jsx_spread_attribute(attr)?;
        }
    }
    Ok(())
}

pub fn map_jsx_opening_attribute_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    attr: &'ast ast::jsx::OpeningAttribute<Loc, Loc>,
) -> ast::jsx::OpeningAttribute<Loc, Loc> {
    match attr {
        ast::jsx::OpeningAttribute::Attribute(a) => {
            ast::jsx::OpeningAttribute::Attribute(visitor.map_jsx_attribute(a))
        }
        ast::jsx::OpeningAttribute::SpreadAttribute(s) => {
            ast::jsx::OpeningAttribute::SpreadAttribute(visitor.map_jsx_spread_attribute(s))
        }
    }
}

pub fn jsx_spread_attribute_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    attr: &'ast ast::jsx::SpreadAttribute<Loc, Type>,
) -> Result<(), E> {
    let ast::jsx::SpreadAttribute {
        loc: _,
        argument,
        comments,
    } = attr;
    visitor.expression(argument)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_jsx_spread_attribute_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    attr: &'ast ast::jsx::SpreadAttribute<Loc, Loc>,
) -> ast::jsx::SpreadAttribute<Loc, Loc> {
    let ast::jsx::SpreadAttribute {
        loc,
        argument,
        comments,
    } = attr;

    let argument_ = visitor.map_expression(argument);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::jsx::SpreadAttribute {
        loc: loc.dupe(),
        argument: argument_,
        comments: comments_,
    }
}

pub fn jsx_attribute_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    attr: &'ast ast::jsx::Attribute<Loc, Type>,
) -> Result<(), E> {
    let ast::jsx::Attribute {
        loc: _,
        name,
        value,
    } = attr;
    visitor.jsx_attribute_name(name)?;
    if let Some(value) = value {
        visitor.jsx_attribute_value(value)?;
    }
    Ok(())
}

pub fn map_jsx_attribute_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    attr: &'ast ast::jsx::Attribute<Loc, Loc>,
) -> ast::jsx::Attribute<Loc, Loc> {
    let ast::jsx::Attribute { loc, name, value } = attr;

    let name_ = visitor.map_jsx_attribute_name(name);
    let value_ = value.as_ref().map(|v| visitor.map_jsx_attribute_value(v));

    ast::jsx::Attribute {
        loc: loc.dupe(),
        name: name_,
        value: value_,
    }
}

pub fn jsx_attribute_name_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    name: &'ast ast::jsx::attribute::Name<Loc, Type>,
) -> Result<(), E> {
    match name {
        ast::jsx::attribute::Name::Identifier(ident) => {
            visitor.jsx_attribute_name_identifier(ident)?;
        }
        ast::jsx::attribute::Name::NamespacedName(ns) => {
            visitor.jsx_attribute_name_namespaced(ns)?;
        }
    }
    Ok(())
}

pub fn map_jsx_attribute_name_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    name: &'ast ast::jsx::attribute::Name<Loc, Loc>,
) -> ast::jsx::attribute::Name<Loc, Loc> {
    match name {
        ast::jsx::attribute::Name::Identifier(id) => {
            ast::jsx::attribute::Name::Identifier(visitor.map_jsx_identifier(id))
        }
        ast::jsx::attribute::Name::NamespacedName(ns) => {
            ast::jsx::attribute::Name::NamespacedName(visitor.map_jsx_namespaced_name(ns))
        }
    }
}

pub fn jsx_attribute_name_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ident: &'ast ast::jsx::Identifier<Loc, Type>,
) -> Result<(), E> {
    visitor.jsx_identifier(ident)?;
    Ok(())
}

pub fn map_jsx_attribute_name_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ident: &'ast ast::jsx::Identifier<Loc, Loc>,
) -> ast::jsx::Identifier<Loc, Loc> {
    visitor.map_jsx_identifier(ident)
}

pub fn jsx_attribute_name_namespaced_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ns: &'ast ast::jsx::NamespacedName<Loc, Type>,
) -> Result<(), E> {
    visitor.jsx_namespaced_name(ns)?;
    Ok(())
}

pub fn map_jsx_attribute_name_namespaced_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ns: &'ast ast::jsx::NamespacedName<Loc, Loc>,
) -> ast::jsx::NamespacedName<Loc, Loc> {
    visitor.map_jsx_namespaced_name(ns)
}

pub fn jsx_attribute_value_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    value: &'ast ast::jsx::attribute::Value<Loc, Type>,
) -> Result<(), E> {
    match value {
        ast::jsx::attribute::Value::StringLiteral((loc, lit)) => {
            visitor.jsx_attribute_value_literal(loc, lit)?;
        }
        ast::jsx::attribute::Value::ExpressionContainer((loc, expr)) => {
            visitor.jsx_attribute_value_expression(loc, expr)?;
        }
    }
    Ok(())
}

pub fn map_jsx_attribute_value_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    value: &'ast ast::jsx::attribute::Value<Loc, Loc>,
) -> ast::jsx::attribute::Value<Loc, Loc> {
    match value {
        ast::jsx::attribute::Value::StringLiteral((loc, lit)) => {
            ast::jsx::attribute::Value::StringLiteral((loc.dupe(), visitor.map_string_literal(lit)))
        }
        ast::jsx::attribute::Value::ExpressionContainer((loc, expr)) => {
            ast::jsx::attribute::Value::ExpressionContainer((
                loc.dupe(),
                visitor.map_jsx_expression(expr),
            ))
        }
    }
}

pub fn jsx_attribute_value_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    jsx_expr: &'ast ast::jsx::ExpressionContainer<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    visitor.jsx_expression(jsx_expr)?;
    Ok(())
}

pub fn map_jsx_attribute_value_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    jsx_expr: &'ast ast::jsx::ExpressionContainer<Loc, Loc>,
) -> (Loc, ast::jsx::ExpressionContainer<Loc, Loc>) {
    (loc.dupe(), visitor.map_jsx_expression(jsx_expr))
}

pub fn jsx_attribute_value_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Type,
    lit: &'ast ast::StringLiteral<Loc>,
) -> Result<(), E> {
    visitor.string_literal(lit)?;
    Ok(())
}

pub fn map_jsx_attribute_value_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    lit: &'ast ast::StringLiteral<Loc>,
) -> (Loc, ast::StringLiteral<Loc>) {
    (loc.dupe(), visitor.map_string_literal(lit))
}

pub fn jsx_child_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    child: &'ast ast::jsx::Child<Loc, Type>,
) -> Result<(), E> {
    match child {
        ast::jsx::Child::Element { loc, inner } => {
            visitor.jsx_element(loc, inner)?;
        }
        ast::jsx::Child::Fragment { loc, inner } => {
            visitor.jsx_fragment(loc, inner)?;
        }
        ast::jsx::Child::ExpressionContainer { loc: _, inner } => {
            visitor.jsx_expression(inner)?;
        }
        ast::jsx::Child::SpreadChild { loc: _, inner } => {
            visitor.jsx_spread_child(inner)?;
        }
        ast::jsx::Child::Text { .. } => {}
    }
    Ok(())
}

pub fn map_jsx_child_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    child: &'ast ast::jsx::Child<Loc, Loc>,
) -> ast::jsx::Child<Loc, Loc> {
    match child {
        ast::jsx::Child::Element { loc, inner } => ast::jsx::Child::Element {
            loc: loc.dupe(),
            inner: visitor.map_jsx_element(loc, inner),
        },
        ast::jsx::Child::Fragment { loc, inner } => ast::jsx::Child::Fragment {
            loc: loc.dupe(),
            inner: visitor.map_jsx_fragment(loc, inner),
        },
        ast::jsx::Child::ExpressionContainer { loc, inner } => {
            ast::jsx::Child::ExpressionContainer {
                loc: loc.dupe(),
                inner: visitor.map_jsx_expression(inner),
            }
        }
        ast::jsx::Child::SpreadChild { loc, inner } => ast::jsx::Child::SpreadChild {
            loc: loc.dupe(),
            inner: visitor.map_jsx_spread_child(inner),
        },
        ast::jsx::Child::Text { loc, inner } => ast::jsx::Child::Text {
            loc: loc.dupe(),
            inner: inner.clone(),
        },
    }
}

pub fn jsx_children_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    children: &'ast Vec<ast::jsx::Child<Loc, Type>>,
) -> Result<(), E> {
    let _ = loc;
    for child in children {
        visitor.jsx_child(child)?;
    }
    Ok(())
}

pub fn map_jsx_children_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    children: &'ast [ast::jsx::Child<Loc, Loc>],
) -> Vec<ast::jsx::Child<Loc, Loc>> {
    children.iter().map(|c| visitor.map_jsx_child(c)).collect()
}

pub fn jsx_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    jsx_expr: &'ast ast::jsx::ExpressionContainer<Loc, Type>,
) -> Result<(), E> {
    let ast::jsx::ExpressionContainer {
        expression,
        comments,
    } = jsx_expr;
    visitor.syntax_opt(comments.as_ref())?;
    match expression {
        ast::jsx::expression_container::Expression::Expression(expr) => {
            visitor.expression(expr)?;
        }
        ast::jsx::expression_container::Expression::EmptyExpression => {}
    }
    Ok(())
}

pub fn map_jsx_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::jsx::ExpressionContainer<Loc, Loc>,
) -> ast::jsx::ExpressionContainer<Loc, Loc> {
    let ast::jsx::ExpressionContainer {
        expression,
        comments,
    } = expr;

    let expression_ = match expression {
        ast::jsx::expression_container::Expression::Expression(e) => {
            ast::jsx::expression_container::Expression::Expression(visitor.map_expression(e))
        }
        ast::jsx::expression_container::Expression::EmptyExpression => {
            ast::jsx::expression_container::Expression::EmptyExpression
        }
    };
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::jsx::ExpressionContainer {
        expression: expression_,
        comments: comments_,
    }
}

pub fn jsx_spread_child_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    child: &'ast ast::jsx::SpreadChild<Loc, Type>,
) -> Result<(), E> {
    let ast::jsx::SpreadChild {
        expression,
        comments,
    } = child;
    visitor.expression(expression)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_jsx_spread_child_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    child: &'ast ast::jsx::SpreadChild<Loc, Loc>,
) -> ast::jsx::SpreadChild<Loc, Loc> {
    let ast::jsx::SpreadChild {
        expression,
        comments,
    } = child;

    let expression_ = visitor.map_expression(expression);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::jsx::SpreadChild {
        expression: expression_,
        comments: comments_,
    }
}

pub fn jsx_element_name_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    name: &'ast ast::jsx::Name<Loc, Type>,
) -> Result<(), E> {
    match name {
        ast::jsx::Name::Identifier(ident) => {
            visitor.jsx_element_name_identifier(ident)?;
        }
        ast::jsx::Name::NamespacedName(ns) => {
            visitor.jsx_element_name_namespaced(ns)?;
        }
        ast::jsx::Name::MemberExpression(member) => {
            visitor.jsx_element_name_member_expression(member)?;
        }
    }
    Ok(())
}

pub fn map_jsx_element_name_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    name: &'ast ast::jsx::Name<Loc, Loc>,
) -> ast::jsx::Name<Loc, Loc> {
    match name {
        ast::jsx::Name::Identifier(id) => {
            ast::jsx::Name::Identifier(visitor.map_jsx_element_name_identifier(id))
        }
        ast::jsx::Name::NamespacedName(ns) => {
            ast::jsx::Name::NamespacedName(visitor.map_jsx_element_name_namespaced(ns))
        }
        ast::jsx::Name::MemberExpression(member) => {
            ast::jsx::Name::MemberExpression(visitor.map_jsx_element_name_member_expression(member))
        }
    }
}

pub fn jsx_element_name_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ident: &'ast ast::jsx::Identifier<Loc, Type>,
) -> Result<(), E> {
    visitor.jsx_identifier(ident)?;
    Ok(())
}

pub fn map_jsx_element_name_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ident: &'ast ast::jsx::Identifier<Loc, Loc>,
) -> ast::jsx::Identifier<Loc, Loc> {
    visitor.map_jsx_identifier(ident)
}

pub fn jsx_element_name_namespaced_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ns: &'ast ast::jsx::NamespacedName<Loc, Type>,
) -> Result<(), E> {
    visitor.jsx_namespaced_name(ns)?;
    Ok(())
}

pub fn map_jsx_element_name_namespaced_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ns: &'ast ast::jsx::NamespacedName<Loc, Loc>,
) -> ast::jsx::NamespacedName<Loc, Loc> {
    visitor.map_jsx_namespaced_name(ns)
}

pub fn jsx_element_name_member_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    member: &'ast ast::jsx::MemberExpression<Loc, Type>,
) -> Result<(), E> {
    visitor.jsx_member_expression(member)?;
    Ok(())
}

pub fn map_jsx_element_name_member_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    member: &'ast ast::jsx::MemberExpression<Loc, Loc>,
) -> ast::jsx::MemberExpression<Loc, Loc> {
    visitor.map_jsx_member_expression(member)
}

pub fn jsx_namespaced_name_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ns: &'ast ast::jsx::NamespacedName<Loc, Type>,
) -> Result<(), E> {
    let ast::jsx::NamespacedName {
        loc: _,
        namespace,
        name,
    } = ns;
    visitor.jsx_identifier(namespace)?;
    visitor.jsx_identifier(name)?;
    Ok(())
}

pub fn map_jsx_namespaced_name_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ns: &'ast ast::jsx::NamespacedName<Loc, Loc>,
) -> ast::jsx::NamespacedName<Loc, Loc> {
    let ast::jsx::NamespacedName {
        loc,
        namespace,
        name,
    } = ns;

    let namespace_ = visitor.map_jsx_identifier(namespace);
    let name_ = visitor.map_jsx_identifier(name);

    ast::jsx::NamespacedName {
        loc: loc.dupe(),
        namespace: namespace_,
        name: name_,
    }
}

pub fn jsx_member_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    member: &'ast ast::jsx::MemberExpression<Loc, Type>,
) -> Result<(), E> {
    let ast::jsx::MemberExpression {
        loc: _,
        object,
        property,
    } = member;
    visitor.jsx_member_expression_object(object)?;
    visitor.jsx_identifier(property)?;
    Ok(())
}

pub fn map_jsx_member_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    member: &'ast ast::jsx::MemberExpression<Loc, Loc>,
) -> ast::jsx::MemberExpression<Loc, Loc> {
    let ast::jsx::MemberExpression {
        loc,
        object,
        property,
    } = member;

    let object_ = visitor.map_jsx_member_expression_object(object);
    let property_ = visitor.map_jsx_identifier(property);

    ast::jsx::MemberExpression {
        loc: loc.dupe(),
        object: object_,
        property: property_,
    }
}

pub fn jsx_member_expression_object_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    obj: &'ast ast::jsx::member_expression::Object<Loc, Type>,
) -> Result<(), E> {
    match obj {
        ast::jsx::member_expression::Object::Identifier(ident) => {
            visitor.jsx_member_expression_identifier(ident)?;
        }
        ast::jsx::member_expression::Object::MemberExpression(member) => {
            visitor.jsx_member_expression(member)?;
        }
    }
    Ok(())
}

pub fn map_jsx_member_expression_object_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    obj: &'ast ast::jsx::member_expression::Object<Loc, Loc>,
) -> ast::jsx::member_expression::Object<Loc, Loc> {
    match obj {
        ast::jsx::member_expression::Object::Identifier(id) => {
            ast::jsx::member_expression::Object::Identifier(visitor.map_jsx_identifier(id))
        }
        ast::jsx::member_expression::Object::MemberExpression(member) => {
            ast::jsx::member_expression::Object::MemberExpression(Arc::new(
                visitor.map_jsx_member_expression(member),
            ))
        }
    }
}

pub fn jsx_member_expression_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ident: &'ast ast::jsx::Identifier<Loc, Type>,
) -> Result<(), E> {
    visitor.jsx_element_name_identifier(ident)?;
    Ok(())
}

pub fn map_jsx_member_expression_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ident: &'ast ast::jsx::Identifier<Loc, Loc>,
) -> ast::jsx::Identifier<Loc, Loc> {
    visitor.map_jsx_element_name_identifier(ident)
}

pub fn jsx_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ident: &'ast ast::jsx::Identifier<Loc, Type>,
) -> Result<(), E> {
    let ast::jsx::Identifier {
        loc: _,
        name: _,
        comments,
    } = ident;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_jsx_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    ident: &'ast ast::jsx::Identifier<Loc, Loc>,
) -> ast::jsx::Identifier<Loc, Loc> {
    let ast::jsx::Identifier {
        loc,
        name,
        comments,
    } = ident;

    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::jsx::Identifier {
        loc: loc.dupe(),
        name: name.dupe(),
        comments: comments_,
    }
}

pub fn labeled_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    label: &'ast ast::statement::Labeled<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::statement::Labeled {
        label,
        body,
        comments,
    } = label;
    visitor.label_identifier(label)?;
    visitor.statement(body)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_labeled_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    label: &'ast ast::statement::Labeled<Loc, Loc>,
) -> ast::statement::Labeled<Loc, Loc> {
    let ast::statement::Labeled {
        label: lbl,
        body,
        comments,
    } = label;
    let lbl_ = visitor.map_identifier(lbl);
    let body_ = visitor.map_statement(body);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::Labeled {
        label: lbl_,
        body: body_,
        comments: comments_,
    }
}

pub fn logical_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    expr: &'ast ast::expression::Logical<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::expression::Logical {
        operator: _,
        left,
        right,
        comments,
    } = expr;
    visitor.expression(left)?;
    visitor.expression(right)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_logical_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::Logical<Loc, Loc>,
) -> ast::expression::Logical<Loc, Loc> {
    let ast::expression::Logical {
        operator,
        left,
        right,
        comments,
    } = expr;
    let left_ = visitor.map_expression(left);
    let right_ = visitor.map_expression(right);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::Logical {
        operator: *operator,
        left: left_,
        right: right_,
        comments: comments_,
    }
}

pub fn match_default<
    'ast,
    Loc: Dupe,
    Type: Dupe,
    C,
    E,
    Visitor: AstVisitor<'ast, Loc, Type, C, E> + ?Sized,
    B,
>(
    visitor: &mut Visitor,
    loc: C,
    m: &'ast ast::match_::Match<Loc, Type, B>,
    mut on_case_body: impl FnMut(&mut Visitor, &'ast B) -> Result<(), E>,
) -> Result<(), E> {
    let _ = loc;
    let ast::match_::Match {
        arg,
        cases,
        match_keyword_loc: _,
        comments,
    } = m;
    visitor.expression(arg)?;
    for case in cases.iter() {
        visitor.match_case(case, &mut on_case_body)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_match_default<
    'ast,
    Loc: Dupe,
    Type: Dupe,
    C,
    E,
    Visitor: AstVisitor<'ast, Loc, Type, C, E> + ?Sized,
    B,
    B2,
>(
    visitor: &mut Visitor,
    _loc: &'ast Loc,
    m: &'ast ast::match_::Match<Loc, Loc, B>,
    mut on_case_body: impl FnMut(&mut Visitor, &'ast B) -> B2,
) -> ast::match_::Match<Loc, Loc, B2> {
    let ast::match_::Match {
        arg,
        cases,
        match_keyword_loc,
        comments,
    } = m;
    let arg_ = visitor.map_expression(arg);
    let cases_: Arc<[ast::match_::Case<Loc, Loc, B2>]> = cases
        .iter()
        .map(|case| visitor.map_match_case(case, &mut on_case_body))
        .collect();
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::match_::Match {
        arg: arg_,
        cases: cases_,
        match_keyword_loc: match_keyword_loc.dupe(),
        comments: comments_,
    }
}

pub fn match_case_default<
    'ast,
    Loc: Dupe,
    Type: Dupe,
    C,
    E,
    Visitor: AstVisitor<'ast, Loc, Type, C, E> + ?Sized,
    B,
>(
    visitor: &mut Visitor,
    case: &'ast ast::match_::Case<Loc, Type, B>,
    on_case_body: &mut impl FnMut(&mut Visitor, &'ast B) -> Result<(), E>,
) -> Result<(), E> {
    let ast::match_::Case {
        loc: _,
        pattern,
        body,
        guard,
        comments,
        invalid_syntax: _,
        case_match_root_loc: _,
    } = case;
    visitor.match_pattern(pattern)?;
    if let Some(guard) = guard {
        visitor.expression(guard)?;
    }
    on_case_body(visitor, body)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_match_case_default<
    'ast,
    Loc: Dupe,
    Type: Dupe,
    C,
    E,
    Visitor: AstVisitor<'ast, Loc, Type, C, E> + ?Sized,
    B,
    B2,
>(
    visitor: &mut Visitor,
    case: &'ast ast::match_::Case<Loc, Loc, B>,
    on_case_body: &mut impl FnMut(&mut Visitor, &'ast B) -> B2,
) -> ast::match_::Case<Loc, Loc, B2> {
    let ast::match_::Case {
        loc,
        pattern,
        body,
        guard,
        comments,
        invalid_syntax,
        case_match_root_loc,
    } = case;
    let pattern_ = visitor.map_match_pattern(pattern);
    let body_ = on_case_body(visitor, body);
    let guard_ = guard.as_ref().map(|g| visitor.map_expression(g));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::match_::Case {
        loc: loc.dupe(),
        pattern: pattern_,
        body: body_,
        guard: guard_,
        comments: comments_,
        invalid_syntax: invalid_syntax.clone(),
        case_match_root_loc: case_match_root_loc.dupe(),
    }
}

pub fn match_expression_default<
    'ast,
    Loc: Dupe,
    Type: Dupe,
    C,
    E,
    V: AstVisitor<'ast, Loc, Type, C, E> + ?Sized,
>(
    visitor: &mut V,
    loc: &'ast Type,
    m: &'ast ast::expression::MatchExpression<Loc, Type>,
) -> Result<(), E> {
    visitor.match_(V::normalize_type(loc), m, |v, x| v.expression(x))?;
    Ok(())
}

pub fn map_match_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    m: &'ast ast::expression::MatchExpression<Loc, Loc>,
) -> ast::expression::MatchExpression<Loc, Loc> {
    visitor.map_match_(loc, m, |v, x| v.map_expression(x))
}

pub fn match_statement_default<
    'ast,
    Loc: Dupe,
    Type: Dupe,
    C,
    E,
    V: AstVisitor<'ast, Loc, Type, C, E> + ?Sized,
>(
    visitor: &mut V,
    loc: &'ast Loc,
    m: &'ast ast::statement::MatchStatement<Loc, Type>,
) -> Result<(), E> {
    visitor.match_(V::normalize_loc(loc), m, |v, x| v.statement(x))?;
    Ok(())
}

pub fn map_match_statement_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    m: &'ast ast::statement::MatchStatement<Loc, Loc>,
) -> ast::statement::MatchStatement<Loc, Loc> {
    visitor.map_match_(loc, m, |v, x| v.map_statement(x))
}

pub fn record_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    record: &'ast ast::statement::RecordDeclaration<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::statement::RecordDeclaration {
        id,
        tparams,
        implements,
        body,
        comments,
        invalid_syntax: _,
    } = record;
    visitor.pattern_identifier(Some(ast::VariableKind::Const), id)?;
    if let Some(tparams) = tparams {
        visitor.type_params(&TypeParamsContext::Record, tparams)?;
    }
    if let Some(implements) = implements {
        visitor.class_implements(implements)?;
    }
    visitor.record_declaration_body(body)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_record_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    record: &'ast ast::statement::RecordDeclaration<Loc, Loc>,
) -> ast::statement::RecordDeclaration<Loc, Loc> {
    let ast::statement::RecordDeclaration {
        id,
        tparams,
        implements,
        body,
        comments,
        invalid_syntax,
    } = record;
    let id_ = visitor.map_identifier(id);
    let tparams_ = tparams
        .as_ref()
        .map(|tp| visitor.map_type_params(&TypeParamsContext::Record, tp));
    let implements_ = implements
        .as_ref()
        .map(|imp| visitor.map_class_implements(imp));
    let body_ = visitor.map_record_declaration_body(body);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::RecordDeclaration {
        id: id_,
        tparams: tparams_,
        implements: implements_,
        body: body_,
        comments: comments_,
        invalid_syntax: invalid_syntax.clone(),
    }
}

pub fn record_declaration_body_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    body: &'ast ast::statement::record_declaration::Body<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::record_declaration::Body {
        loc: _,
        body,
        comments,
    } = body;
    for element in body.iter() {
        visitor.record_declaration_body_element(element)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_record_declaration_body_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    body: &'ast ast::statement::record_declaration::Body<Loc, Loc>,
) -> ast::statement::record_declaration::Body<Loc, Loc> {
    let ast::statement::record_declaration::Body {
        loc,
        body: body_elements,
        comments,
    } = body;
    let body_ = body_elements
        .iter()
        .map(|elem| visitor.map_record_declaration_body_element(elem))
        .collect();
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::record_declaration::Body {
        loc: loc.dupe(),
        body: body_,
        comments: comments_,
    }
}

pub fn record_declaration_body_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    element: &'ast ast::statement::record_declaration::BodyElement<Loc, Type>,
) -> Result<(), E> {
    match element {
        ast::statement::record_declaration::BodyElement::Method(meth) => {
            visitor.class_method(meth)?;
        }
        ast::statement::record_declaration::BodyElement::Property(prop) => {
            visitor.record_declaration_property(prop)?;
        }
        ast::statement::record_declaration::BodyElement::StaticProperty(prop) => {
            visitor.record_declaration_static_property(prop)?;
        }
    }
    Ok(())
}

pub fn map_record_declaration_body_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    element: &'ast ast::statement::record_declaration::BodyElement<Loc, Loc>,
) -> ast::statement::record_declaration::BodyElement<Loc, Loc> {
    match element {
        ast::statement::record_declaration::BodyElement::Method(meth) => {
            ast::statement::record_declaration::BodyElement::Method(visitor.map_class_method(meth))
        }
        ast::statement::record_declaration::BodyElement::Property(prop) => {
            ast::statement::record_declaration::BodyElement::Property(
                visitor.map_record_declaration_property(prop),
            )
        }
        ast::statement::record_declaration::BodyElement::StaticProperty(prop) => {
            ast::statement::record_declaration::BodyElement::StaticProperty(
                visitor.map_record_declaration_static_property(prop),
            )
        }
    }
}

pub fn record_declaration_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    prop: &'ast ast::statement::record_declaration::Property<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::record_declaration::Property {
        loc: _,
        key,
        annot,
        default_value,
        comments,
        invalid_syntax: _,
    } = prop;
    visitor.object_key(key)?;
    visitor.type_annotation(annot)?;
    if let Some(default_value) = default_value {
        visitor.expression(default_value)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_record_declaration_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    prop: &'ast ast::statement::record_declaration::Property<Loc, Loc>,
) -> ast::statement::record_declaration::Property<Loc, Loc> {
    let ast::statement::record_declaration::Property {
        loc,
        key,
        annot,
        default_value,
        comments,
        invalid_syntax,
    } = prop;
    let key_ = visitor.map_object_key(key);
    let annot_ = visitor.map_type_annotation(annot);
    let default_value_ = default_value.as_ref().map(|v| visitor.map_expression(v));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::record_declaration::Property {
        loc: loc.dupe(),
        key: key_,
        annot: annot_,
        default_value: default_value_,
        comments: comments_,
        invalid_syntax: invalid_syntax.clone(),
    }
}

pub fn record_declaration_static_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    prop: &'ast ast::statement::record_declaration::StaticProperty<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::record_declaration::StaticProperty {
        loc: _,
        key,
        annot,
        value,
        comments,
        invalid_syntax: _,
    } = prop;
    visitor.object_key(key)?;
    visitor.type_annotation(annot)?;
    visitor.expression(value)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_record_declaration_static_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    prop: &'ast ast::statement::record_declaration::StaticProperty<Loc, Loc>,
) -> ast::statement::record_declaration::StaticProperty<Loc, Loc> {
    let ast::statement::record_declaration::StaticProperty {
        loc,
        key,
        annot,
        value,
        comments,
        invalid_syntax,
    } = prop;
    let key_ = visitor.map_object_key(key);
    let annot_ = visitor.map_type_annotation(annot);
    let value_ = visitor.map_expression(value);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::record_declaration::StaticProperty {
        loc: loc.dupe(),
        key: key_,
        annot: annot_,
        value: value_,
        comments: comments_,
        invalid_syntax: invalid_syntax.clone(),
    }
}

pub fn match_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::match_pattern::MatchPattern<Loc, Type>,
) -> Result<(), E> {
    match pattern {
        ast::match_pattern::MatchPattern::WildcardPattern { loc: _, inner } => {
            visitor.match_wildcard_pattern(inner)?;
        }
        ast::match_pattern::MatchPattern::StringPattern { loc: _, inner } => {
            visitor.string_literal(inner)?;
        }
        ast::match_pattern::MatchPattern::BooleanPattern { loc: _, inner } => {
            visitor.boolean_literal(inner)?;
        }
        ast::match_pattern::MatchPattern::NullPattern { loc: _, inner } => {
            visitor.syntax_opt((**inner).as_ref())?;
        }
        ast::match_pattern::MatchPattern::NumberPattern { loc: _, inner } => {
            visitor.number_literal(inner)?;
        }
        ast::match_pattern::MatchPattern::BigIntPattern { loc: _, inner } => {
            visitor.bigint_literal(inner)?;
        }
        ast::match_pattern::MatchPattern::UnaryPattern { loc: _, inner } => {
            visitor.match_unary_pattern(inner)?;
        }
        ast::match_pattern::MatchPattern::IdentifierPattern { loc: _, inner } => {
            visitor.identifier(inner)?;
        }
        ast::match_pattern::MatchPattern::MemberPattern { loc: _, inner } => {
            visitor.match_member_pattern(inner)?;
        }
        ast::match_pattern::MatchPattern::BindingPattern { loc, inner } => {
            visitor.match_binding_pattern(loc, inner)?;
        }
        ast::match_pattern::MatchPattern::ObjectPattern { loc, inner } => {
            visitor.match_object_pattern(loc, inner)?;
        }
        ast::match_pattern::MatchPattern::ArrayPattern { loc: _, inner } => {
            visitor.match_array_pattern(inner)?;
        }
        ast::match_pattern::MatchPattern::InstancePattern { loc: _, inner } => {
            visitor.match_instance_pattern(inner)?;
        }
        ast::match_pattern::MatchPattern::OrPattern { loc: _, inner } => {
            visitor.match_or_pattern(inner)?;
        }
        ast::match_pattern::MatchPattern::AsPattern { loc: _, inner } => {
            visitor.match_as_pattern(inner)?;
        }
    }
    Ok(())
}

pub fn map_match_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::match_pattern::MatchPattern<Loc, Loc>,
) -> ast::match_pattern::MatchPattern<Loc, Loc> {
    match pattern {
        ast::match_pattern::MatchPattern::WildcardPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::WildcardPattern {
                loc: loc.dupe(),
                inner: Box::new(visitor.map_match_wildcard_pattern(inner)),
            }
        }
        ast::match_pattern::MatchPattern::StringPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::StringPattern {
                loc: loc.dupe(),
                inner: Box::new(visitor.map_string_literal(inner)),
            }
        }
        ast::match_pattern::MatchPattern::BooleanPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::BooleanPattern {
                loc: loc.dupe(),
                inner: Box::new(visitor.map_boolean_literal(inner)),
            }
        }
        ast::match_pattern::MatchPattern::NullPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::NullPattern {
                loc: loc.dupe(),
                inner: Box::new(visitor.map_syntax_opt((**inner).as_ref())),
            }
        }
        ast::match_pattern::MatchPattern::NumberPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::NumberPattern {
                loc: loc.dupe(),
                inner: Box::new(visitor.map_number_literal(inner)),
            }
        }
        ast::match_pattern::MatchPattern::BigIntPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::BigIntPattern {
                loc: loc.dupe(),
                inner: Box::new(visitor.map_bigint_literal(inner)),
            }
        }
        ast::match_pattern::MatchPattern::UnaryPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::UnaryPattern {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_match_unary_pattern(inner)),
            }
        }
        ast::match_pattern::MatchPattern::IdentifierPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::IdentifierPattern {
                loc: loc.dupe(),
                inner: Box::new(visitor.map_identifier(inner)),
            }
        }
        ast::match_pattern::MatchPattern::MemberPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::MemberPattern {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_match_member_pattern(inner)),
            }
        }
        ast::match_pattern::MatchPattern::BindingPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::BindingPattern {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_match_binding_pattern(loc, inner)),
            }
        }
        ast::match_pattern::MatchPattern::ObjectPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::ObjectPattern {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_match_object_pattern(loc, inner)),
            }
        }
        ast::match_pattern::MatchPattern::ArrayPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::ArrayPattern {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_match_array_pattern(inner)),
            }
        }
        ast::match_pattern::MatchPattern::InstancePattern { loc, inner } => {
            ast::match_pattern::MatchPattern::InstancePattern {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_match_instance_pattern(inner)),
            }
        }
        ast::match_pattern::MatchPattern::OrPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::OrPattern {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_match_or_pattern(inner)),
            }
        }
        ast::match_pattern::MatchPattern::AsPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::AsPattern {
                loc: loc.dupe(),
                inner: Arc::new(visitor.map_match_as_pattern(inner)),
            }
        }
    }
}

pub fn match_unary_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::match_pattern::UnaryPattern<Loc>,
) -> Result<(), E> {
    let ast::match_pattern::UnaryPattern {
        operator: _,
        argument,
        comments,
    } = pattern;
    let (arg_loc, arg) = argument;
    visitor.match_unary_pattern_argument(arg_loc, arg)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_match_unary_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::match_pattern::UnaryPattern<Loc>,
) -> ast::match_pattern::UnaryPattern<Loc> {
    let ast::match_pattern::UnaryPattern {
        operator,
        argument,
        comments,
    } = pattern;
    let (arg_loc, arg) = argument;
    let arg_ = visitor.map_match_unary_pattern_argument(arg_loc, arg);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::match_pattern::UnaryPattern {
        operator: operator.clone(),
        argument: (arg_loc.dupe(), arg_),
        comments: comments_,
    }
}

pub fn match_unary_pattern_argument_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    arg: &'ast ast::match_pattern::unary_pattern::Argument<Loc>,
) -> Result<(), E> {
    match arg {
        ast::match_pattern::unary_pattern::Argument::NumberLiteral(lit) => {
            visitor.number_literal(lit)?;
        }
        ast::match_pattern::unary_pattern::Argument::BigIntLiteral(lit) => {
            visitor.bigint_literal(lit)?;
        }
    }
    Ok(())
}

pub fn map_match_unary_pattern_argument_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    arg: &'ast ast::match_pattern::unary_pattern::Argument<Loc>,
) -> ast::match_pattern::unary_pattern::Argument<Loc> {
    match arg {
        ast::match_pattern::unary_pattern::Argument::NumberLiteral(lit) => {
            ast::match_pattern::unary_pattern::Argument::NumberLiteral(
                visitor.map_number_literal(lit),
            )
        }
        ast::match_pattern::unary_pattern::Argument::BigIntLiteral(lit) => {
            ast::match_pattern::unary_pattern::Argument::BigIntLiteral(
                visitor.map_bigint_literal(lit),
            )
        }
    }
}

pub fn match_member_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::match_pattern::MemberPattern<Loc, Type>,
) -> Result<(), E> {
    let ast::match_pattern::MemberPattern {
        loc: _,
        base,
        property,
        comments,
    } = pattern;
    visitor.match_member_pattern_base(base)?;
    visitor.match_member_pattern_property(property)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_match_member_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::match_pattern::MemberPattern<Loc, Loc>,
) -> ast::match_pattern::MemberPattern<Loc, Loc> {
    let ast::match_pattern::MemberPattern {
        loc,
        base,
        property,
        comments,
    } = pattern;
    let base_ = visitor.map_match_member_pattern_base(base);
    let property_ = visitor.map_match_member_pattern_property(property);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::match_pattern::MemberPattern {
        loc: loc.dupe(),
        base: base_,
        property: property_,
        comments: comments_,
    }
}

pub fn match_member_pattern_base_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    base: &'ast ast::match_pattern::member_pattern::Base<Loc, Type>,
) -> Result<(), E> {
    match base {
        ast::match_pattern::member_pattern::Base::BaseIdentifier(ident) => {
            visitor.identifier(ident)?;
        }
        ast::match_pattern::member_pattern::Base::BaseMember(member) => {
            visitor.match_member_pattern(member)?;
        }
    }
    Ok(())
}

pub fn map_match_member_pattern_base_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    base: &'ast ast::match_pattern::member_pattern::Base<Loc, Loc>,
) -> ast::match_pattern::member_pattern::Base<Loc, Loc> {
    match base {
        ast::match_pattern::member_pattern::Base::BaseIdentifier(ident) => {
            ast::match_pattern::member_pattern::Base::BaseIdentifier(visitor.map_identifier(ident))
        }
        ast::match_pattern::member_pattern::Base::BaseMember(member) => {
            ast::match_pattern::member_pattern::Base::BaseMember(Arc::new(
                visitor.map_match_member_pattern(member),
            ))
        }
    }
}

pub fn match_member_pattern_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    prop: &'ast ast::match_pattern::member_pattern::Property<Loc, Type>,
) -> Result<(), E> {
    match prop {
        ast::match_pattern::member_pattern::Property::PropertyString { loc: _, literal } => {
            visitor.string_literal(literal)?;
        }
        ast::match_pattern::member_pattern::Property::PropertyNumber { loc: _, literal } => {
            visitor.number_literal(literal)?;
        }
        ast::match_pattern::member_pattern::Property::PropertyBigInt { loc: _, literal } => {
            visitor.bigint_literal(literal)?;
        }
        ast::match_pattern::member_pattern::Property::PropertyIdentifier(ident) => {
            visitor.identifier(ident)?;
        }
    }
    Ok(())
}

pub fn map_match_member_pattern_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    prop: &'ast ast::match_pattern::member_pattern::Property<Loc, Loc>,
) -> ast::match_pattern::member_pattern::Property<Loc, Loc> {
    match prop {
        ast::match_pattern::member_pattern::Property::PropertyString { loc, literal } => {
            ast::match_pattern::member_pattern::Property::PropertyString {
                loc: loc.dupe(),
                literal: visitor.map_string_literal(literal),
            }
        }
        ast::match_pattern::member_pattern::Property::PropertyNumber { loc, literal } => {
            ast::match_pattern::member_pattern::Property::PropertyNumber {
                loc: loc.dupe(),
                literal: visitor.map_number_literal(literal),
            }
        }
        ast::match_pattern::member_pattern::Property::PropertyBigInt { loc, literal } => {
            ast::match_pattern::member_pattern::Property::PropertyBigInt {
                loc: loc.dupe(),
                literal: visitor.map_bigint_literal(literal),
            }
        }
        ast::match_pattern::member_pattern::Property::PropertyIdentifier(ident) => {
            ast::match_pattern::member_pattern::Property::PropertyIdentifier(
                visitor.map_identifier(ident),
            )
        }
    }
}

pub fn match_binding_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    binding: &'ast ast::match_pattern::BindingPattern<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::match_pattern::BindingPattern { id, kind, comments } = binding;
    visitor.pattern_identifier(Some(*kind), id)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_match_binding_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    binding: &'ast ast::match_pattern::BindingPattern<Loc, Loc>,
) -> ast::match_pattern::BindingPattern<Loc, Loc> {
    let ast::match_pattern::BindingPattern { id, kind, comments } = binding;
    let id_ = visitor.map_identifier(id);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::match_pattern::BindingPattern {
        id: id_,
        kind: *kind,
        comments: comments_,
    }
}

pub fn match_object_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    pattern: &'ast ast::match_pattern::ObjectPattern<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::match_pattern::ObjectPattern {
        properties,
        rest,
        comments,
    } = pattern;
    for prop in properties.iter() {
        visitor.match_object_pattern_property(prop)?;
    }
    if let Some(rest) = rest {
        visitor.match_rest_pattern(rest)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_match_object_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    pattern: &'ast ast::match_pattern::ObjectPattern<Loc, Loc>,
) -> ast::match_pattern::ObjectPattern<Loc, Loc> {
    let ast::match_pattern::ObjectPattern {
        properties,
        rest,
        comments,
    } = pattern;
    let properties_: Arc<[ast::match_pattern::object_pattern::Property<Loc, Loc>]> = properties
        .iter()
        .map(|prop| visitor.map_match_object_pattern_property(prop))
        .collect();
    let rest_ = rest.as_ref().map(|r| visitor.map_match_rest_pattern(r));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::match_pattern::ObjectPattern {
        properties: properties_,
        rest: rest_,
        comments: comments_,
    }
}

pub fn match_object_pattern_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    prop: &'ast ast::match_pattern::object_pattern::Property<Loc, Type>,
) -> Result<(), E> {
    match prop {
        ast::match_pattern::object_pattern::Property::Valid {
            loc: _,
            property:
                ast::match_pattern::object_pattern::PropertyStruct {
                    key,
                    pattern,
                    shorthand: _,
                    comments,
                },
        } => {
            visitor.match_object_pattern_property_key(key)?;
            visitor.match_pattern(pattern)?;
            visitor.syntax_opt(comments.as_ref())?;
        }
        ast::match_pattern::object_pattern::Property::InvalidShorthand { loc: _, identifier } => {
            visitor.untyped_identifier(identifier)?;
        }
    }
    Ok(())
}

pub fn map_match_object_pattern_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    prop: &'ast ast::match_pattern::object_pattern::Property<Loc, Loc>,
) -> ast::match_pattern::object_pattern::Property<Loc, Loc> {
    match prop {
        ast::match_pattern::object_pattern::Property::Valid {
            loc,
            property:
                ast::match_pattern::object_pattern::PropertyStruct {
                    key,
                    pattern,
                    shorthand,
                    comments,
                },
        } => {
            let key_ = visitor.map_match_object_pattern_property_key(key);
            let pattern_ = visitor.map_match_pattern(pattern);
            let comments_ = visitor.map_syntax_opt(comments.as_ref());
            ast::match_pattern::object_pattern::Property::Valid {
                loc: loc.dupe(),
                property: ast::match_pattern::object_pattern::PropertyStruct {
                    key: key_,
                    pattern: pattern_,
                    shorthand: *shorthand,
                    comments: comments_,
                },
            }
        }
        ast::match_pattern::object_pattern::Property::InvalidShorthand { loc, identifier } => {
            ast::match_pattern::object_pattern::Property::InvalidShorthand {
                loc: loc.dupe(),
                identifier: identifier.clone(),
            }
        }
    }
}

pub fn match_object_pattern_property_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    key: &'ast ast::match_pattern::object_pattern::Key<Loc, Type>,
) -> Result<(), E> {
    match key {
        ast::match_pattern::object_pattern::Key::StringLiteral((_loc, lit)) => {
            visitor.string_literal(lit)?;
        }
        ast::match_pattern::object_pattern::Key::NumberLiteral((_loc, lit)) => {
            visitor.number_literal(lit)?;
        }
        ast::match_pattern::object_pattern::Key::BigIntLiteral((_loc, lit)) => {
            visitor.bigint_literal(lit)?;
        }
        ast::match_pattern::object_pattern::Key::Identifier(ident) => {
            visitor.identifier(ident)?;
        }
    }
    Ok(())
}

pub fn map_match_object_pattern_property_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    key: &'ast ast::match_pattern::object_pattern::Key<Loc, Loc>,
) -> ast::match_pattern::object_pattern::Key<Loc, Loc> {
    match key {
        ast::match_pattern::object_pattern::Key::StringLiteral((loc, lit)) => {
            ast::match_pattern::object_pattern::Key::StringLiteral((
                loc.dupe(),
                visitor.map_string_literal(lit),
            ))
        }
        ast::match_pattern::object_pattern::Key::NumberLiteral((loc, lit)) => {
            ast::match_pattern::object_pattern::Key::NumberLiteral((
                loc.dupe(),
                visitor.map_number_literal(lit),
            ))
        }
        ast::match_pattern::object_pattern::Key::BigIntLiteral((loc, lit)) => {
            ast::match_pattern::object_pattern::Key::BigIntLiteral((
                loc.dupe(),
                visitor.map_bigint_literal(lit),
            ))
        }
        ast::match_pattern::object_pattern::Key::Identifier(ident) => {
            ast::match_pattern::object_pattern::Key::Identifier(visitor.map_identifier(ident))
        }
    }
}

pub fn match_array_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::match_pattern::ArrayPattern<Loc, Type>,
) -> Result<(), E> {
    let ast::match_pattern::ArrayPattern {
        elements,
        rest,
        comments,
    } = pattern;
    for elem in elements.iter() {
        visitor.match_pattern_array_element(elem)?;
    }
    if let Some(rest) = rest {
        visitor.match_rest_pattern(rest)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_match_array_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::match_pattern::ArrayPattern<Loc, Loc>,
) -> ast::match_pattern::ArrayPattern<Loc, Loc> {
    let ast::match_pattern::ArrayPattern {
        elements,
        rest,
        comments,
    } = pattern;
    let elements_: Arc<[ast::match_pattern::array_pattern::Element<Loc, Loc>]> = elements
        .iter()
        .map(|elem| visitor.map_match_pattern_array_element(elem))
        .collect();
    let rest_ = rest.as_ref().map(|r| visitor.map_match_rest_pattern(r));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::match_pattern::ArrayPattern {
        elements: elements_,
        rest: rest_,
        comments: comments_,
    }
}

pub fn match_pattern_array_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    elem: &'ast ast::match_pattern::array_pattern::Element<Loc, Type>,
) -> Result<(), E> {
    let ast::match_pattern::array_pattern::Element { pattern, index: _ } = elem;
    visitor.match_pattern(pattern)?;
    Ok(())
}

pub fn map_match_pattern_array_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    elem: &'ast ast::match_pattern::array_pattern::Element<Loc, Loc>,
) -> ast::match_pattern::array_pattern::Element<Loc, Loc> {
    let ast::match_pattern::array_pattern::Element { pattern, index } = elem;
    let pattern_ = visitor.map_match_pattern(pattern);
    ast::match_pattern::array_pattern::Element {
        pattern: pattern_,
        index: index.clone(),
    }
}

pub fn match_instance_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::match_pattern::InstancePattern<Loc, Type>,
) -> Result<(), E> {
    let ast::match_pattern::InstancePattern {
        constructor,
        properties,
        comments,
    } = pattern;
    visitor.match_instance_pattern_constructor(constructor)?;
    let (loc, obj_pattern) = properties;
    visitor.match_object_pattern(loc, obj_pattern)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_match_instance_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::match_pattern::InstancePattern<Loc, Loc>,
) -> ast::match_pattern::InstancePattern<Loc, Loc> {
    let ast::match_pattern::InstancePattern {
        constructor,
        properties,
        comments,
    } = pattern;
    let constructor_ = visitor.map_match_instance_pattern_constructor(constructor);
    let (loc, obj_pattern) = properties;
    let properties_ = (
        loc.dupe(),
        visitor.map_match_object_pattern(loc, obj_pattern),
    );
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::match_pattern::InstancePattern {
        constructor: constructor_,
        properties: properties_,
        comments: comments_,
    }
}

pub fn match_instance_pattern_constructor_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    constructor: &'ast ast::match_pattern::InstancePatternConstructor<Loc, Type>,
) -> Result<(), E> {
    match constructor {
        ast::match_pattern::InstancePatternConstructor::IdentifierConstructor(ident) => {
            visitor.identifier(ident)?;
        }
        ast::match_pattern::InstancePatternConstructor::MemberConstructor(member) => {
            visitor.match_member_pattern(member)?;
        }
    }
    Ok(())
}

pub fn map_match_instance_pattern_constructor_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    constructor: &'ast ast::match_pattern::InstancePatternConstructor<Loc, Loc>,
) -> ast::match_pattern::InstancePatternConstructor<Loc, Loc> {
    match constructor {
        ast::match_pattern::InstancePatternConstructor::IdentifierConstructor(ident) => {
            ast::match_pattern::InstancePatternConstructor::IdentifierConstructor(
                visitor.map_identifier(ident),
            )
        }
        ast::match_pattern::InstancePatternConstructor::MemberConstructor(member) => {
            ast::match_pattern::InstancePatternConstructor::MemberConstructor(
                visitor.map_match_member_pattern(member),
            )
        }
    }
}

pub fn match_rest_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    rest: &'ast ast::match_pattern::RestPattern<Loc, Type>,
) -> Result<(), E> {
    let ast::match_pattern::RestPattern {
        loc: _,
        argument,
        comments,
    } = rest;
    if let Some((loc, argument)) = argument {
        visitor.match_binding_pattern(loc, argument)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_match_rest_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    rest: &'ast ast::match_pattern::RestPattern<Loc, Loc>,
) -> ast::match_pattern::RestPattern<Loc, Loc> {
    let ast::match_pattern::RestPattern {
        loc,
        argument,
        comments,
    } = rest;
    let argument_ = argument.as_ref().map(|(arg_loc, arg)| {
        (
            arg_loc.dupe(),
            visitor.map_match_binding_pattern(arg_loc, arg),
        )
    });
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::match_pattern::RestPattern {
        loc: loc.dupe(),
        argument: argument_,
        comments: comments_,
    }
}

pub fn match_or_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::match_pattern::OrPattern<Loc, Type>,
) -> Result<(), E> {
    let ast::match_pattern::OrPattern { patterns, comments } = pattern;
    for p in patterns.iter() {
        visitor.match_pattern(p)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_match_or_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::match_pattern::OrPattern<Loc, Loc>,
) -> ast::match_pattern::OrPattern<Loc, Loc> {
    let ast::match_pattern::OrPattern { patterns, comments } = pattern;
    let patterns_ = patterns
        .iter()
        .map(|p| visitor.map_match_pattern(p))
        .collect();
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::match_pattern::OrPattern {
        patterns: patterns_,
        comments: comments_,
    }
}

pub fn match_as_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::match_pattern::AsPattern<Loc, Type>,
) -> Result<(), E> {
    let ast::match_pattern::AsPattern {
        pattern,
        target,
        comments,
    } = pattern;
    visitor.match_pattern(pattern)?;
    visitor.match_as_pattern_target(target)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_match_as_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::match_pattern::AsPattern<Loc, Loc>,
) -> ast::match_pattern::AsPattern<Loc, Loc> {
    let ast::match_pattern::AsPattern {
        pattern,
        target,
        comments,
    } = pattern;
    let pattern_ = visitor.map_match_pattern(pattern);
    let target_ = visitor.map_match_as_pattern_target(target);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::match_pattern::AsPattern {
        pattern: pattern_,
        target: target_,
        comments: comments_,
    }
}

pub fn match_as_pattern_target_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    target: &'ast ast::match_pattern::as_pattern::Target<Loc, Type>,
) -> Result<(), E> {
    match target {
        ast::match_pattern::as_pattern::Target::Binding { loc, pattern } => {
            visitor.match_binding_pattern(loc, pattern)?;
        }
        ast::match_pattern::as_pattern::Target::Identifier(ident) => {
            visitor.pattern_identifier(Some(ast::VariableKind::Const), ident)?;
        }
    }
    Ok(())
}

pub fn map_match_as_pattern_target_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    target: &'ast ast::match_pattern::as_pattern::Target<Loc, Loc>,
) -> ast::match_pattern::as_pattern::Target<Loc, Loc> {
    match target {
        ast::match_pattern::as_pattern::Target::Binding { loc, pattern } => {
            ast::match_pattern::as_pattern::Target::Binding {
                loc: loc.dupe(),
                pattern: visitor.map_match_binding_pattern(loc, pattern),
            }
        }
        ast::match_pattern::as_pattern::Target::Identifier(ident) => {
            ast::match_pattern::as_pattern::Target::Identifier(visitor.map_identifier(ident))
        }
    }
}

pub fn match_wildcard_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    wildcard: &'ast ast::match_pattern::WildcardPattern<Loc>,
) -> Result<(), E> {
    let ast::match_pattern::WildcardPattern {
        comments,
        invalid_syntax_default_keyword: _,
    } = wildcard;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_match_wildcard_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    wildcard: &'ast ast::match_pattern::WildcardPattern<Loc>,
) -> ast::match_pattern::WildcardPattern<Loc> {
    let ast::match_pattern::WildcardPattern {
        comments,
        invalid_syntax_default_keyword,
    } = wildcard;
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::match_pattern::WildcardPattern {
        comments: comments_,
        invalid_syntax_default_keyword: invalid_syntax_default_keyword.clone(),
    }
}

pub fn member_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    expr: &'ast ast::expression::Member<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::expression::Member {
        object,
        property,
        comments,
    } = expr;
    visitor.expression(object)?;
    visitor.member_property(property)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_member_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::Member<Loc, Loc>,
) -> ast::expression::Member<Loc, Loc> {
    let ast::expression::Member {
        object,
        property,
        comments,
    } = expr;
    let object_ = visitor.map_expression(object);
    let property_ = visitor.map_member_property(property);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::Member {
        object: object_,
        property: property_,
        comments: comments_,
    }
}

pub fn optional_member_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    expr: &'ast ast::expression::OptionalMember<Loc, Type>,
) -> Result<(), E> {
    let ast::expression::OptionalMember {
        member,
        optional: _,
        filtered_out: _,
    } = expr;
    visitor.member(loc, member)?;
    Ok(())
}

pub fn map_optional_member_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    expr: &'ast ast::expression::OptionalMember<Loc, Loc>,
) -> ast::expression::OptionalMember<Loc, Loc> {
    let ast::expression::OptionalMember {
        member,
        filtered_out,
        optional,
    } = expr;
    let member_ = visitor.map_member(loc, member);
    let optional_ = match optional {
        ast::expression::OptionalMemberKind::Optional => {
            ast::expression::OptionalMemberKind::Optional
        }
        ast::expression::OptionalMemberKind::NonOptional => {
            ast::expression::OptionalMemberKind::NonOptional
        }
        ast::expression::OptionalMemberKind::AssertNonnull { assertion_loc } => {
            ast::expression::OptionalMemberKind::AssertNonnull {
                assertion_loc: visitor.map_loc_annot(assertion_loc),
            }
        }
    };
    ast::expression::OptionalMember {
        member: member_,
        filtered_out: filtered_out.dupe(),
        optional: optional_,
    }
}

pub fn record_default<
    'ast,
    Loc: Dupe,
    Type: Dupe,
    C,
    E,
    V: AstVisitor<'ast, Loc, Type, C, E> + ?Sized,
>(
    visitor: &mut V,
    loc: &'ast Type,
    expr: &'ast ast::expression::Record<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::expression::Record {
        constructor,
        targs,
        properties,
        comments,
    } = expr;
    visitor.expression(constructor)?;
    if let Some(targs) = targs {
        visitor.call_type_args(targs)?;
    }
    let (props_loc, obj) = properties;
    visitor.object(V::normalize_loc(props_loc), obj)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_record_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::Record<Loc, Loc>,
) -> ast::expression::Record<Loc, Loc> {
    let ast::expression::Record {
        constructor,
        targs,
        properties,
        comments,
    } = expr;
    let constructor_ = visitor.map_expression(constructor);
    let targs_ = targs.as_ref().map(|t| visitor.map_call_type_args(t));
    let (props_loc, obj) = properties;
    let properties_ = (props_loc.dupe(), visitor.map_object(props_loc, obj));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::Record {
        constructor: constructor_,
        targs: targs_,
        properties: properties_,
        comments: comments_,
    }
}

pub fn member_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    prop: &'ast ast::expression::member::Property<Loc, Type>,
) -> Result<(), E> {
    match prop {
        ast::expression::member::Property::PropertyIdentifier(ident) => {
            visitor.member_property_identifier(ident)?;
        }
        ast::expression::member::Property::PropertyPrivateName(name) => {
            visitor.member_private_name(name)?;
        }
        ast::expression::member::Property::PropertyExpression(expr) => {
            visitor.member_property_expression(expr)?;
        }
    }
    Ok(())
}

pub fn map_member_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    prop: &'ast ast::expression::member::Property<Loc, Loc>,
) -> ast::expression::member::Property<Loc, Loc> {
    match prop {
        ast::expression::member::Property::PropertyIdentifier(ident) => {
            ast::expression::member::Property::PropertyIdentifier(
                visitor.map_member_property_identifier(ident),
            )
        }
        ast::expression::member::Property::PropertyPrivateName(name) => {
            ast::expression::member::Property::PropertyPrivateName(
                visitor.map_member_private_name(name),
            )
        }
        ast::expression::member::Property::PropertyExpression(expr) => {
            ast::expression::member::Property::PropertyExpression(
                visitor.map_member_property_expression(expr),
            )
        }
    }
}

pub fn member_property_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Type>,
) -> Result<(), E> {
    visitor.identifier(id)?;
    Ok(())
}

pub fn map_member_property_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    visitor.map_identifier(id)
}

pub fn member_private_name_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::PrivateName<Loc>,
) -> Result<(), E> {
    visitor.private_name(id)?;
    Ok(())
}

pub fn map_member_private_name_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::PrivateName<Loc>,
) -> ast::PrivateName<Loc> {
    visitor.map_private_name(id)
}

pub fn member_property_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::expression::Expression<Loc, Type>,
) -> Result<(), E> {
    visitor.expression(expr)?;
    Ok(())
}

pub fn map_member_property_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::expression::Expression<Loc, Loc>,
) -> ast::expression::Expression<Loc, Loc> {
    visitor.map_expression(expr)
}

pub fn meta_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    expr: &'ast ast::expression::MetaProperty<Loc>,
) -> Result<(), E> {
    let _ = loc;
    let ast::expression::MetaProperty {
        meta,
        property,
        comments,
    } = expr;
    visitor.untyped_identifier(meta)?;
    visitor.untyped_identifier(property)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_meta_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::MetaProperty<Loc>,
) -> ast::expression::MetaProperty<Loc> {
    let ast::expression::MetaProperty {
        meta,
        property,
        comments,
    } = expr;
    let meta_ = visitor.map_identifier(meta);
    let property_ = visitor.map_identifier(property);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::MetaProperty {
        meta: meta_,
        property: property_,
        comments: comments_,
    }
}

pub fn new_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    expr: &'ast ast::expression::New<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::expression::New {
        callee,
        targs,
        arguments,
        comments,
    } = expr;
    visitor.expression(callee)?;
    if let Some(targs) = targs {
        visitor.call_type_args(targs)?;
    }
    if let Some(arguments) = arguments {
        visitor.arg_list(arguments)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_new_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::New<Loc, Loc>,
) -> ast::expression::New<Loc, Loc> {
    let ast::expression::New {
        callee,
        targs,
        arguments,
        comments,
    } = expr;
    let callee_ = visitor.map_expression(callee);
    let targs_ = targs.as_ref().map(|t| visitor.map_call_type_args(t));
    let arguments_ = arguments.as_ref().map(|args| visitor.map_arg_list(args));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::New {
        callee: callee_,
        targs: targs_,
        arguments: arguments_,
        comments: comments_,
    }
}

pub fn object_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: C,
    expr: &'ast ast::expression::Object<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::expression::Object {
        properties,
        comments,
    } = expr;
    for prop in properties.iter() {
        match prop {
            ast::expression::object::Property::NormalProperty(p) => {
                visitor.object_property(p)?;
            }
            ast::expression::object::Property::SpreadProperty(s) => {
                visitor.spread_property(s)?;
            }
        }
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_object_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::Object<Loc, Loc>,
) -> ast::expression::Object<Loc, Loc> {
    let ast::expression::Object {
        properties,
        comments,
    } = expr;
    let properties_ = Arc::from(
        properties
            .iter()
            .map(|p| visitor.map_object_property(p))
            .collect::<Vec<_>>(),
    );
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::Object {
        properties: properties_,
        comments: comments_,
    }
}

pub fn object_property_default<
    'ast,
    Loc: Dupe,
    Type: Dupe,
    C,
    E,
    V: AstVisitor<'ast, Loc, Type, C, E> + ?Sized,
>(
    visitor: &mut V,
    prop: &'ast ast::expression::object::NormalProperty<Loc, Type>,
) -> Result<(), E> {
    match prop {
        ast::expression::object::NormalProperty::Init {
            loc: _,
            key,
            value,
            shorthand: _,
        } => {
            visitor.object_key(key)?;
            visitor.expression(value)?;
        }
        ast::expression::object::NormalProperty::Method { loc: _, key, value } => {
            visitor.object_key(key)?;
            let (loc, func) = value;
            visitor.function_expression_or_method(V::normalize_loc(loc), func)?;
        }
        ast::expression::object::NormalProperty::Get {
            loc: _,
            key,
            value,
            comments,
        } => {
            visitor.object_key(key)?;
            let (loc, func) = value;
            visitor.function_expression_or_method(V::normalize_loc(loc), func)?;
            visitor.syntax_opt(comments.as_ref())?;
        }
        ast::expression::object::NormalProperty::Set {
            loc: _,
            key,
            value,
            comments,
        } => {
            visitor.object_key(key)?;
            let (loc, func) = value;
            visitor.function_expression_or_method(V::normalize_loc(loc), func)?;
            visitor.syntax_opt(comments.as_ref())?;
        }
    }
    Ok(())
}

pub fn map_object_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    prop: &'ast ast::expression::object::Property<Loc, Loc>,
) -> ast::expression::object::Property<Loc, Loc> {
    match prop {
        ast::expression::object::Property::NormalProperty(np) => {
            let np_ = match np {
                ast::expression::object::NormalProperty::Init {
                    loc,
                    key,
                    value,
                    shorthand,
                } => {
                    let key_ = visitor.map_object_key(key);
                    let value_ = visitor.map_expression(value);
                    ast::expression::object::NormalProperty::Init {
                        loc: loc.dupe(),
                        key: key_,
                        value: value_,
                        shorthand: *shorthand,
                    }
                }
                ast::expression::object::NormalProperty::Method { loc, key, value } => {
                    let key_ = visitor.map_object_key(key);
                    let (value_loc, value_func) = value;
                    let value_func_ =
                        visitor.map_function_expression_or_method(value_loc, value_func);
                    ast::expression::object::NormalProperty::Method {
                        loc: loc.dupe(),
                        key: key_,
                        value: (value_loc.dupe(), value_func_),
                    }
                }
                ast::expression::object::NormalProperty::Get {
                    loc,
                    key,
                    value,
                    comments,
                } => {
                    let key_ = visitor.map_object_key(key);
                    let (value_loc, value_func) = value;
                    let value_func_ =
                        visitor.map_function_expression_or_method(value_loc, value_func);
                    let comments_ = visitor.map_syntax_opt(comments.as_ref());
                    ast::expression::object::NormalProperty::Get {
                        loc: loc.dupe(),
                        key: key_,
                        value: (value_loc.dupe(), value_func_),
                        comments: comments_,
                    }
                }
                ast::expression::object::NormalProperty::Set {
                    loc,
                    key,
                    value,
                    comments,
                } => {
                    let key_ = visitor.map_object_key(key);
                    let (value_loc, value_func) = value;
                    let value_func_ =
                        visitor.map_function_expression_or_method(value_loc, value_func);
                    let comments_ = visitor.map_syntax_opt(comments.as_ref());
                    ast::expression::object::NormalProperty::Set {
                        loc: loc.dupe(),
                        key: key_,
                        value: (value_loc.dupe(), value_func_),
                        comments: comments_,
                    }
                }
            };
            ast::expression::object::Property::NormalProperty(np_)
        }
        ast::expression::object::Property::SpreadProperty(sp) => {
            let ast::expression::object::SpreadProperty {
                loc,
                argument,
                comments,
            } = sp;
            let argument_ = visitor.map_expression(argument);
            let comments_ = visitor.map_syntax_opt(comments.as_ref());
            ast::expression::object::Property::SpreadProperty(
                ast::expression::object::SpreadProperty {
                    loc: loc.dupe(),
                    argument: argument_,
                    comments: comments_,
                },
            )
        }
    }
}

pub fn object_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    key: &'ast ast::expression::object::Key<Loc, Type>,
) -> Result<(), E> {
    match key {
        ast::expression::object::Key::StringLiteral((_, lit)) => {
            visitor.object_key_string_literal(lit)?;
        }
        ast::expression::object::Key::NumberLiteral((_, lit)) => {
            visitor.object_key_number_literal(lit)?;
        }
        ast::expression::object::Key::BigIntLiteral((_, lit)) => {
            visitor.object_key_bigint_literal(lit)?;
        }
        ast::expression::object::Key::Identifier(ident) => {
            visitor.object_key_identifier(ident)?;
        }
        ast::expression::object::Key::PrivateName(name) => {
            visitor.private_name(name)?;
        }
        ast::expression::object::Key::Computed(computed) => {
            visitor.object_key_computed(computed)?;
        }
    }
    Ok(())
}

pub fn map_object_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    key: &'ast ast::expression::object::Key<Loc, Loc>,
) -> ast::expression::object::Key<Loc, Loc> {
    match key {
        ast::expression::object::Key::StringLiteral((loc, lit)) => {
            ast::expression::object::Key::StringLiteral((
                loc.dupe(),
                visitor.map_string_literal(lit),
            ))
        }
        ast::expression::object::Key::NumberLiteral((loc, lit)) => {
            ast::expression::object::Key::NumberLiteral((
                loc.dupe(),
                visitor.map_number_literal(lit),
            ))
        }
        ast::expression::object::Key::BigIntLiteral((loc, lit)) => {
            ast::expression::object::Key::BigIntLiteral((
                loc.dupe(),
                visitor.map_bigint_literal(lit),
            ))
        }
        ast::expression::object::Key::Identifier(id) => {
            ast::expression::object::Key::Identifier(visitor.map_identifier(id))
        }
        ast::expression::object::Key::PrivateName(pn) => {
            ast::expression::object::Key::PrivateName(visitor.map_private_name(pn))
        }
        ast::expression::object::Key::Computed(ck) => {
            ast::expression::object::Key::Computed(visitor.map_computed_key(ck))
        }
    }
}

pub fn object_key_string_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    literal: &'ast ast::StringLiteral<Loc>,
) -> Result<(), E> {
    visitor.string_literal(literal)?;
    Ok(())
}

pub fn map_object_key_string_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    literal: &'ast ast::StringLiteral<Loc>,
) -> ast::StringLiteral<Loc> {
    visitor.map_string_literal(literal)
}

pub fn object_key_number_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    literal: &'ast ast::NumberLiteral<Loc>,
) -> Result<(), E> {
    visitor.number_literal(literal)?;
    Ok(())
}

pub fn map_object_key_number_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    literal: &'ast ast::NumberLiteral<Loc>,
) -> ast::NumberLiteral<Loc> {
    visitor.map_number_literal(literal)
}

pub fn object_key_bigint_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    literal: &'ast ast::BigIntLiteral<Loc>,
) -> Result<(), E> {
    visitor.bigint_literal(literal)?;
    Ok(())
}

pub fn map_object_key_bigint_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    literal: &'ast ast::BigIntLiteral<Loc>,
) -> ast::BigIntLiteral<Loc> {
    visitor.map_bigint_literal(literal)
}

pub fn object_key_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Type>,
) -> Result<(), E> {
    visitor.identifier(id)?;
    Ok(())
}

pub fn map_object_key_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    id: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    visitor.map_identifier(id)
}

pub fn object_key_computed_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    key: &'ast ast::ComputedKey<Loc, Type>,
) -> Result<(), E> {
    visitor.computed_key(key)?;
    Ok(())
}

pub fn map_object_key_computed_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    key: &'ast ast::ComputedKey<Loc, Loc>,
) -> ast::ComputedKey<Loc, Loc> {
    visitor.map_computed_key(key)
}

pub fn opaque_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    otype: &'ast ast::statement::OpaqueType<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::statement::OpaqueType {
        id,
        tparams,
        impl_type,
        lower_bound,
        upper_bound,
        legacy_upper_bound,
        comments,
    } = otype;
    visitor.binding_type_identifier(id)?;
    if let Some(tparams) = tparams {
        visitor.type_params(&TypeParamsContext::OpaqueType, tparams)?;
    }
    if let Some(impl_type) = impl_type {
        visitor.type_(impl_type)?;
    }
    if let Some(legacy_upper_bound) = legacy_upper_bound {
        visitor.type_(legacy_upper_bound)?;
    }
    if let Some(lower_bound) = lower_bound {
        visitor.type_(lower_bound)?;
    }
    if let Some(upper_bound) = upper_bound {
        visitor.type_(upper_bound)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_opaque_type_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    otype: &'ast ast::statement::OpaqueType<Loc, Loc>,
) -> ast::statement::OpaqueType<Loc, Loc> {
    let ast::statement::OpaqueType {
        id,
        tparams,
        impl_type,
        lower_bound,
        upper_bound,
        legacy_upper_bound,
        comments,
    } = otype;
    let id_ = visitor.map_binding_type_identifier(id);
    let tparams_ = tparams
        .as_ref()
        .map(|tp| visitor.map_type_params(&TypeParamsContext::OpaqueType, tp));
    let impl_type_ = impl_type.as_ref().map(|t| visitor.map_type_(t));
    let lower_bound_ = lower_bound.as_ref().map(|t| visitor.map_type_(t));
    let upper_bound_ = upper_bound.as_ref().map(|t| visitor.map_type_(t));
    let legacy_upper_bound_ = legacy_upper_bound.as_ref().map(|t| visitor.map_type_(t));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::OpaqueType {
        id: id_,
        tparams: tparams_,
        impl_type: impl_type_,
        lower_bound: lower_bound_,
        upper_bound: upper_bound_,
        legacy_upper_bound: legacy_upper_bound_,
        comments: comments_,
    }
}

pub fn function_param_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::pattern::Pattern<Loc, Type>,
) -> Result<(), E> {
    visitor.binding_pattern(ast::VariableKind::Let, pattern)?;
    Ok(())
}

pub fn map_function_param_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
) -> ast::pattern::Pattern<Loc, Loc> {
    visitor.map_binding_pattern(ast::VariableKind::Let, pattern)
}

pub fn variable_declarator_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: ast::VariableKind,
    pattern: &'ast ast::pattern::Pattern<Loc, Type>,
) -> Result<(), E> {
    visitor.binding_pattern(kind, pattern)?;
    Ok(())
}

pub fn map_variable_declarator_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: ast::VariableKind,
    pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
) -> ast::pattern::Pattern<Loc, Loc> {
    visitor.map_binding_pattern(kind, pattern)
}

pub fn catch_clause_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::pattern::Pattern<Loc, Type>,
) -> Result<(), E> {
    visitor.binding_pattern(ast::VariableKind::Let, pattern)?;
    Ok(())
}

pub fn map_catch_clause_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
) -> ast::pattern::Pattern<Loc, Loc> {
    visitor.map_binding_pattern(ast::VariableKind::Let, pattern)
}

pub fn for_in_assignment_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::pattern::Pattern<Loc, Type>,
) -> Result<(), E> {
    visitor.assignment_pattern(expr)?;
    Ok(())
}

pub fn map_for_in_assignment_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::pattern::Pattern<Loc, Loc>,
) -> ast::pattern::Pattern<Loc, Loc> {
    visitor.map_assignment_pattern(expr)
}

pub fn for_of_assignment_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::pattern::Pattern<Loc, Type>,
) -> Result<(), E> {
    visitor.assignment_pattern(expr)?;
    Ok(())
}

pub fn map_for_of_assignment_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::pattern::Pattern<Loc, Loc>,
) -> ast::pattern::Pattern<Loc, Loc> {
    visitor.map_assignment_pattern(expr)
}

pub fn binding_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: ast::VariableKind,
    pattern: &'ast ast::pattern::Pattern<Loc, Type>,
) -> Result<(), E> {
    visitor.pattern(Some(kind), pattern)?;
    Ok(())
}

pub fn map_binding_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: ast::VariableKind,
    pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
) -> ast::pattern::Pattern<Loc, Loc> {
    visitor.map_pattern(Some(kind), pattern)
}

pub fn assignment_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::pattern::Pattern<Loc, Type>,
) -> Result<(), E> {
    visitor.pattern(None, pattern)?;
    Ok(())
}

pub fn map_assignment_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
) -> ast::pattern::Pattern<Loc, Loc> {
    visitor.map_pattern(None, pattern)
}

pub fn pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    pattern: &'ast ast::pattern::Pattern<Loc, Type>,
) -> Result<(), E> {
    match pattern {
        ast::pattern::Pattern::Object { loc, inner } => {
            let ast::pattern::Object {
                properties,
                annot,
                optional: _,
                comments,
            } = inner.as_ref();
            for p in properties.iter() {
                visitor.pattern_object_p(kind, loc, p)?;
            }
            visitor.type_annotation_hint(annot)?;
            visitor.syntax_opt(comments.as_ref())?;
        }
        ast::pattern::Pattern::Array { loc, inner } => {
            let ast::pattern::Array {
                elements,
                annot,
                optional: _,
                comments,
            } = inner.as_ref();
            for e in elements.iter() {
                visitor.pattern_array_e(kind, loc, e)?;
            }
            visitor.type_annotation_hint(annot)?;
            visitor.syntax_opt(comments.as_ref())?;
        }
        ast::pattern::Pattern::Identifier { loc: _, inner } => {
            let ast::pattern::Identifier {
                name,
                annot,
                optional: _,
            } = inner.as_ref();
            visitor.pattern_identifier(kind, name)?;
            visitor.type_annotation_hint(annot)?;
        }
        ast::pattern::Pattern::Expression { loc: _, inner } => {
            visitor.pattern_expression(inner)?;
        }
    }
    Ok(())
}

pub fn map_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
) -> ast::pattern::Pattern<Loc, Loc> {
    match pattern {
        ast::pattern::Pattern::Object { loc, inner } => {
            let ast::pattern::Object {
                properties,
                annot,
                optional,
                comments,
            } = inner.as_ref();
            let properties_ = Arc::from(
                properties
                    .iter()
                    .map(|p| visitor.map_pattern_object_p(kind, loc, p))
                    .collect::<Vec<_>>(),
            );
            let annot_ = visitor.map_type_annotation_hint(annot);
            let comments_ = comments.as_ref().map(|c| visitor.map_syntax(c));
            ast::pattern::Pattern::Object {
                loc: loc.dupe(),
                inner: Arc::new(ast::pattern::Object {
                    properties: properties_,
                    annot: annot_,
                    optional: *optional,
                    comments: comments_,
                }),
            }
        }
        ast::pattern::Pattern::Array { loc, inner } => {
            let ast::pattern::Array {
                elements,
                annot,
                optional,
                comments,
            } = inner.as_ref();
            let elements_ = Arc::from(
                elements
                    .iter()
                    .map(|e| visitor.map_pattern_array_e(kind, loc, e))
                    .collect::<Vec<_>>(),
            );
            let annot_ = visitor.map_type_annotation_hint(annot);
            let comments_ = comments.as_ref().map(|c| visitor.map_syntax(c));
            ast::pattern::Pattern::Array {
                loc: loc.dupe(),
                inner: Arc::new(ast::pattern::Array {
                    elements: elements_,
                    annot: annot_,
                    optional: *optional,
                    comments: comments_,
                }),
            }
        }
        ast::pattern::Pattern::Identifier { loc, inner } => {
            let inner_ = visitor.map_pattern_identifier(kind, inner);
            ast::pattern::Pattern::Identifier {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            }
        }
        ast::pattern::Pattern::Expression { loc, inner } => {
            let inner_ = visitor.map_expression(inner);
            ast::pattern::Pattern::Expression {
                loc: loc.dupe(),
                inner: Arc::new(inner_),
            }
        }
    }
}

pub fn pattern_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    ident: &'ast ast::Identifier<Loc, Type>,
) -> Result<(), E> {
    let _ = kind;
    visitor.identifier(ident)?;
    Ok(())
}

pub fn map_pattern_identifier_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _kind: Option<ast::VariableKind>,
    ident: &'ast ast::pattern::Identifier<Loc, Loc>,
) -> ast::pattern::Identifier<Loc, Loc> {
    let ast::pattern::Identifier {
        name,
        annot,
        optional,
    } = ident;
    let name_ = visitor.map_identifier(name);
    let annot_ = visitor.map_type_annotation_hint(annot);
    ast::pattern::Identifier {
        name: name_,
        annot: annot_,
        optional: *optional,
    }
}

pub fn pattern_string_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    lit: &'ast ast::StringLiteral<Loc>,
) -> Result<(), E> {
    visitor.string_literal(lit)?;
    Ok(())
}

pub fn map_pattern_string_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    lit: &'ast ast::StringLiteral<Loc>,
) -> (Loc, ast::StringLiteral<Loc>) {
    (loc.dupe(), visitor.map_string_literal(lit))
}

pub fn pattern_number_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    lit: &'ast ast::NumberLiteral<Loc>,
) -> Result<(), E> {
    visitor.number_literal(lit)?;
    Ok(())
}

pub fn map_pattern_number_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    lit: &'ast ast::NumberLiteral<Loc>,
) -> (Loc, ast::NumberLiteral<Loc>) {
    (loc.dupe(), visitor.map_number_literal(lit))
}

pub fn pattern_bigint_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    lit: &'ast ast::BigIntLiteral<Loc>,
) -> Result<(), E> {
    visitor.bigint_literal(lit)?;
    Ok(())
}

pub fn map_pattern_bigint_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    lit: &'ast ast::BigIntLiteral<Loc>,
) -> (Loc, ast::BigIntLiteral<Loc>) {
    (loc.dupe(), visitor.map_bigint_literal(lit))
}

pub fn pattern_object_p_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    loc: &'ast Type,
    prop: &'ast ast::pattern::object::Property<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    match prop {
        ast::pattern::object::Property::NormalProperty(p) => {
            visitor.pattern_object_property(kind, p)?;
        }
        ast::pattern::object::Property::RestElement(r) => {
            visitor.pattern_object_rest_property(kind, r)?;
        }
    }
    Ok(())
}

pub fn map_pattern_object_p_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    _loc: &'ast Loc,
    prop: &'ast ast::pattern::object::Property<Loc, Loc>,
) -> ast::pattern::object::Property<Loc, Loc> {
    visitor.map_pattern_object_property(kind, prop)
}

pub fn pattern_object_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    prop: &'ast ast::pattern::object::NormalProperty<Loc, Type>,
) -> Result<(), E> {
    let ast::pattern::object::NormalProperty {
        loc: _,
        key,
        pattern,
        default,
        shorthand: _,
    } = prop;
    visitor.pattern_object_property_key(kind, key)?;
    visitor.pattern_object_property_pattern(kind, pattern)?;
    visitor.default_opt(default.as_ref())?;
    Ok(())
}

pub fn map_pattern_object_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    prop: &'ast ast::pattern::object::Property<Loc, Loc>,
) -> ast::pattern::object::Property<Loc, Loc> {
    match prop {
        ast::pattern::object::Property::NormalProperty(p) => {
            let ast::pattern::object::NormalProperty {
                loc,
                key,
                pattern,
                default,
                shorthand,
            } = p;
            let key_ = visitor.map_pattern_object_property_key(kind, key);
            let pattern_ = visitor.map_pattern_object_property_pattern(kind, pattern);
            let default_ = default.as_ref().map(|e| visitor.map_expression(e));
            ast::pattern::object::Property::NormalProperty(ast::pattern::object::NormalProperty {
                loc: loc.dupe(),
                key: key_,
                pattern: pattern_,
                default: default_,
                shorthand: *shorthand,
            })
        }
        ast::pattern::object::Property::RestElement(r) => {
            let r_ = visitor.map_pattern_object_rest_property(kind, r);
            ast::pattern::object::Property::RestElement(r_)
        }
    }
}

pub fn pattern_object_property_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    key: &'ast ast::pattern::object::Key<Loc, Type>,
) -> Result<(), E> {
    match key {
        ast::pattern::object::Key::StringLiteral(lit) => {
            visitor.pattern_object_property_string_literal_key(kind, lit)?;
        }
        ast::pattern::object::Key::NumberLiteral(lit) => {
            visitor.pattern_object_property_number_literal_key(kind, lit)?;
        }
        ast::pattern::object::Key::BigIntLiteral(lit) => {
            visitor.pattern_object_property_bigint_literal_key(kind, lit)?;
        }
        ast::pattern::object::Key::Identifier(ident) => {
            visitor.pattern_object_property_identifier_key(kind, ident)?;
        }
        ast::pattern::object::Key::Computed(computed) => {
            visitor.pattern_object_property_computed_key(kind, computed)?;
        }
    }
    Ok(())
}

pub fn map_pattern_object_property_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _kind: Option<ast::VariableKind>,
    key: &'ast ast::pattern::object::Key<Loc, Loc>,
) -> ast::pattern::object::Key<Loc, Loc> {
    match key {
        ast::pattern::object::Key::StringLiteral((loc, lit)) => {
            ast::pattern::object::Key::StringLiteral((loc.dupe(), visitor.map_string_literal(lit)))
        }
        ast::pattern::object::Key::NumberLiteral((loc, lit)) => {
            ast::pattern::object::Key::NumberLiteral((loc.dupe(), visitor.map_number_literal(lit)))
        }
        ast::pattern::object::Key::BigIntLiteral((loc, lit)) => {
            ast::pattern::object::Key::BigIntLiteral((loc.dupe(), visitor.map_bigint_literal(lit)))
        }
        ast::pattern::object::Key::Identifier(id) => {
            let id_ = visitor.map_identifier(id);
            ast::pattern::object::Key::Identifier(id_)
        }
        ast::pattern::object::Key::Computed(key) => {
            ast::pattern::object::Key::Computed(visitor.map_computed_key(key))
        }
    }
}

pub fn pattern_object_property_string_literal_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _kind: Option<ast::VariableKind>,
    literal: &'ast (Loc, ast::StringLiteral<Loc>),
) -> Result<(), E> {
    let (loc, lit) = literal;
    visitor.pattern_string_literal(loc, lit)?;
    Ok(())
}

pub fn map_pattern_object_property_string_literal_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _kind: Option<ast::VariableKind>,
    literal: &'ast (Loc, ast::StringLiteral<Loc>),
) -> (Loc, ast::StringLiteral<Loc>) {
    let (loc, lit) = literal;
    (loc.dupe(), visitor.map_string_literal(lit))
}

pub fn pattern_object_property_number_literal_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _kind: Option<ast::VariableKind>,
    literal: &'ast (Loc, ast::NumberLiteral<Loc>),
) -> Result<(), E> {
    let (loc, lit) = literal;
    visitor.pattern_number_literal(loc, lit)?;
    Ok(())
}

pub fn map_pattern_object_property_number_literal_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _kind: Option<ast::VariableKind>,
    literal: &'ast (Loc, ast::NumberLiteral<Loc>),
) -> (Loc, ast::NumberLiteral<Loc>) {
    let (loc, lit) = literal;
    (loc.dupe(), visitor.map_number_literal(lit))
}

pub fn pattern_object_property_bigint_literal_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _kind: Option<ast::VariableKind>,
    literal: &'ast (Loc, ast::BigIntLiteral<Loc>),
) -> Result<(), E> {
    let (loc, lit) = literal;
    visitor.pattern_bigint_literal(loc, lit)?;
    Ok(())
}

pub fn map_pattern_object_property_bigint_literal_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _kind: Option<ast::VariableKind>,
    literal: &'ast (Loc, ast::BigIntLiteral<Loc>),
) -> (Loc, ast::BigIntLiteral<Loc>) {
    let (loc, lit) = literal;
    (loc.dupe(), visitor.map_bigint_literal(lit))
}

pub fn pattern_object_property_identifier_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    id: &'ast ast::Identifier<Loc, Type>,
) -> Result<(), E> {
    visitor.pattern_identifier(kind, id)?;
    Ok(())
}

pub fn map_pattern_object_property_identifier_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _kind: Option<ast::VariableKind>,
    id: &'ast ast::Identifier<Loc, Loc>,
) -> ast::Identifier<Loc, Loc> {
    visitor.map_identifier(id)
}

pub fn pattern_object_property_computed_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _kind: Option<ast::VariableKind>,
    key: &'ast ast::ComputedKey<Loc, Type>,
) -> Result<(), E> {
    visitor.computed_key(key)?;
    Ok(())
}

pub fn map_pattern_object_property_computed_key_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _kind: Option<ast::VariableKind>,
    key: &'ast ast::ComputedKey<Loc, Loc>,
) -> ast::ComputedKey<Loc, Loc> {
    visitor.map_computed_key(key)
}

pub fn pattern_object_rest_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    rest_elem: &'ast ast::pattern::RestElement<Loc, Type>,
) -> Result<(), E> {
    let ast::pattern::RestElement {
        loc: _,
        argument,
        comments,
    } = rest_elem;
    visitor.pattern_object_rest_property_pattern(kind, argument)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_pattern_object_rest_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    rest_elem: &'ast ast::pattern::RestElement<Loc, Loc>,
) -> ast::pattern::RestElement<Loc, Loc> {
    let ast::pattern::RestElement {
        loc,
        argument,
        comments,
    } = rest_elem;
    let argument_ = visitor.map_pattern_object_rest_property_pattern(kind, argument);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::pattern::RestElement {
        loc: loc.dupe(),
        argument: argument_,
        comments: comments_,
    }
}

pub fn pattern_object_property_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    pattern: &'ast ast::pattern::Pattern<Loc, Type>,
) -> Result<(), E> {
    visitor.pattern(kind, pattern)?;
    Ok(())
}

pub fn map_pattern_object_property_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
) -> ast::pattern::Pattern<Loc, Loc> {
    visitor.map_pattern(kind, pattern)
}

pub fn pattern_object_rest_property_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    pattern: &'ast ast::pattern::Pattern<Loc, Type>,
) -> Result<(), E> {
    visitor.pattern(kind, pattern)?;
    Ok(())
}

pub fn map_pattern_object_rest_property_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
) -> ast::pattern::Pattern<Loc, Loc> {
    visitor.map_pattern(kind, pattern)
}

pub fn pattern_array_e_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    loc: &'ast Type,
    elem: &'ast ast::pattern::array::Element<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    match elem {
        ast::pattern::array::Element::Hole(_) => {}
        ast::pattern::array::Element::NormalElement(e) => {
            visitor.pattern_array_element(kind, e)?;
        }
        ast::pattern::array::Element::RestElement(r) => {
            visitor.pattern_array_rest_element(kind, r)?;
        }
    }
    Ok(())
}

pub fn map_pattern_array_e_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    _loc: &'ast Loc,
    elem: &'ast ast::pattern::array::Element<Loc, Loc>,
) -> ast::pattern::array::Element<Loc, Loc> {
    match elem {
        ast::pattern::array::Element::Hole(loc) => ast::pattern::array::Element::Hole(loc.dupe()),
        ast::pattern::array::Element::NormalElement(e) => {
            let ast::pattern::array::NormalElement {
                loc,
                argument,
                default,
            } = e;
            let argument_ = visitor.map_pattern_array_element_pattern(kind, argument);
            let default_ = visitor.map_default_opt(default.as_ref());
            ast::pattern::array::Element::NormalElement(ast::pattern::array::NormalElement {
                loc: loc.dupe(),
                argument: argument_,
                default: default_,
            })
        }
        ast::pattern::array::Element::RestElement(r) => {
            let r_ = visitor.map_pattern_array_rest_element(kind, r);
            ast::pattern::array::Element::RestElement(r_)
        }
    }
}

pub fn pattern_array_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    elem: &'ast ast::pattern::array::NormalElement<Loc, Type>,
) -> Result<(), E> {
    let ast::pattern::array::NormalElement {
        loc: _,
        argument,
        default,
    } = elem;
    visitor.pattern_array_element_pattern(kind, argument)?;
    visitor.default_opt(default.as_ref())?;
    Ok(())
}

pub fn map_pattern_array_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    elem: &'ast ast::pattern::array::Element<Loc, Loc>,
) -> ast::pattern::array::Element<Loc, Loc> {
    match elem {
        ast::pattern::array::Element::Hole(loc) => ast::pattern::array::Element::Hole(loc.dupe()),
        ast::pattern::array::Element::NormalElement(e) => {
            let ast::pattern::array::NormalElement {
                loc,
                argument,
                default,
            } = e;
            let argument_ = visitor.map_pattern_array_element_pattern(kind, argument);
            let default_ = default.as_ref().map(|d| visitor.map_expression(d));
            ast::pattern::array::Element::NormalElement(ast::pattern::array::NormalElement {
                loc: loc.dupe(),
                argument: argument_,
                default: default_,
            })
        }
        ast::pattern::array::Element::RestElement(r) => {
            let r_ = visitor.map_pattern_array_rest_element(kind, r);
            ast::pattern::array::Element::RestElement(r_)
        }
    }
}

pub fn pattern_array_element_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    pattern: &'ast ast::pattern::Pattern<Loc, Type>,
) -> Result<(), E> {
    visitor.pattern(kind, pattern)?;
    Ok(())
}

pub fn map_pattern_array_element_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
) -> ast::pattern::Pattern<Loc, Loc> {
    visitor.map_pattern(kind, pattern)
}

pub fn pattern_array_rest_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    rest_elem: &'ast ast::pattern::RestElement<Loc, Type>,
) -> Result<(), E> {
    let ast::pattern::RestElement {
        loc: _,
        argument,
        comments,
    } = rest_elem;
    visitor.pattern_array_rest_element_pattern(kind, argument)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_pattern_array_rest_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    rest_elem: &'ast ast::pattern::RestElement<Loc, Loc>,
) -> ast::pattern::RestElement<Loc, Loc> {
    let ast::pattern::RestElement {
        loc,
        argument,
        comments,
    } = rest_elem;
    let argument_ = visitor.map_pattern_array_rest_element_pattern(kind, argument);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::pattern::RestElement {
        loc: loc.dupe(),
        argument: argument_,
        comments: comments_,
    }
}

pub fn pattern_array_rest_element_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    pattern: &'ast ast::pattern::Pattern<Loc, Type>,
) -> Result<(), E> {
    visitor.pattern(kind, pattern)?;
    Ok(())
}

pub fn map_pattern_array_rest_element_pattern_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: Option<ast::VariableKind>,
    pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
) -> ast::pattern::Pattern<Loc, Loc> {
    visitor.map_pattern(kind, pattern)
}

pub fn pattern_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::expression::Expression<Loc, Type>,
) -> Result<(), E> {
    visitor.expression(expr)?;
    Ok(())
}

pub fn map_pattern_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::expression::Expression<Loc, Loc>,
) -> ast::expression::Expression<Loc, Loc> {
    visitor.map_expression(expr)
}

pub fn predicate_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pred: &'ast ast::types::Predicate<Loc, Type>,
) -> Result<(), E> {
    let ast::types::Predicate {
        loc: _,
        kind,
        comments,
    } = pred;
    match kind {
        ast::types::PredicateKind::Inferred => {}
        ast::types::PredicateKind::Declared(expr) => {
            visitor.expression(expr)?;
        }
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_predicate_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    pred: &'ast ast::types::Predicate<Loc, Loc>,
) -> ast::types::Predicate<Loc, Loc> {
    let ast::types::Predicate {
        loc,
        kind,
        comments,
    } = pred;
    let kind_ = match kind {
        ast::types::PredicateKind::Inferred => ast::types::PredicateKind::Inferred,
        ast::types::PredicateKind::Declared(expr) => {
            ast::types::PredicateKind::Declared(Arc::new(visitor.map_expression(expr)))
        }
    };
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::Predicate {
        loc: loc.dupe(),
        kind: kind_,
        comments: comments_,
    }
}

pub fn predicate_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::expression::Expression<Loc, Type>,
) -> Result<(), E> {
    visitor.expression(expr)?;
    Ok(())
}

pub fn map_predicate_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    expr: &'ast ast::expression::Expression<Loc, Loc>,
) -> ast::expression::Expression<Loc, Loc> {
    visitor.map_expression(expr)
}

pub fn type_guard_annotation_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    annot: &'ast ast::types::TypeGuardAnnotation<Loc, Type>,
) -> Result<(), E> {
    let ast::types::TypeGuardAnnotation { loc: _, guard } = annot;
    visitor.type_guard(guard)?;
    Ok(())
}

pub fn map_type_guard_annotation_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    annot: &'ast ast::types::TypeGuardAnnotation<Loc, Loc>,
) -> ast::types::TypeGuardAnnotation<Loc, Loc> {
    let ast::types::TypeGuardAnnotation { loc, guard } = annot;
    let guard_ = visitor.map_type_guard(guard);
    ast::types::TypeGuardAnnotation {
        loc: loc.dupe(),
        guard: guard_,
    }
}

pub fn type_guard_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    guard: &'ast ast::types::TypeGuard<Loc, Type>,
) -> Result<(), E> {
    let ast::types::TypeGuard {
        loc: _,
        kind: _,
        guard: (x, t),
        comments,
    } = guard;
    visitor.untyped_identifier(x)?;
    if let Some(t_type) = t {
        visitor.type_(t_type)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_type_guard_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    guard: &'ast ast::types::TypeGuard<Loc, Loc>,
) -> ast::types::TypeGuard<Loc, Loc> {
    let ast::types::TypeGuard {
        loc,
        kind,
        guard: (x, t),
        comments,
    } = guard;
    let x_ = visitor.map_identifier(x);
    let t_ = t.as_ref().map(|t_type| visitor.map_type_(t_type));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::types::TypeGuard {
        loc: loc.dupe(),
        kind: *kind,
        guard: (x_, t_),
        comments: comments_,
    }
}

pub fn function_rest_param_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    param: &'ast ast::function::RestParam<Loc, Type>,
) -> Result<(), E> {
    let ast::function::RestParam {
        loc: _,
        argument,
        comments,
    } = param;
    visitor.function_param_pattern(argument)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_function_rest_param_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    param: &'ast ast::function::RestParam<Loc, Loc>,
) -> ast::function::RestParam<Loc, Loc> {
    let ast::function::RestParam {
        loc,
        argument,
        comments,
    } = param;

    let argument_ = visitor.map_function_param_pattern(argument);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());

    ast::function::RestParam {
        loc: loc.dupe(),
        argument: argument_,
        comments: comments_,
    }
}

pub fn return_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    ret: &'ast ast::statement::Return<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::statement::Return {
        argument,
        comments,
        return_out: _,
    } = ret;
    if let Some(arg) = argument {
        visitor.expression(arg)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_return_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    ret: &'ast ast::statement::Return<Loc, Loc>,
) -> ast::statement::Return<Loc, Loc> {
    let ast::statement::Return {
        argument,
        comments,
        return_out,
    } = ret;
    let argument_ = argument.as_ref().map(|a| visitor.map_expression(a));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::Return {
        argument: argument_,
        comments: comments_,
        return_out: return_out.clone(),
    }
}

pub fn sequence_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    seq: &'ast ast::expression::Sequence<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::expression::Sequence {
        expressions,
        comments,
    } = seq;
    for expr in expressions.iter() {
        visitor.expression(expr)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_sequence_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::Sequence<Loc, Loc>,
) -> ast::expression::Sequence<Loc, Loc> {
    let ast::expression::Sequence {
        expressions,
        comments,
    } = expr;
    let expressions_ = Arc::from(
        expressions
            .iter()
            .map(|e| visitor.map_expression(e))
            .collect::<Vec<_>>(),
    );
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::Sequence {
        expressions: expressions_,
        comments: comments_,
    }
}

pub fn toplevel_statement_list_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    stmts: &'ast [ast::statement::Statement<Loc, Type>],
) -> Result<(), E> {
    visitor.statement_list(stmts)?;
    Ok(())
}

pub fn map_toplevel_statement_list_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    stmts: &'ast Arc<[ast::statement::Statement<Loc, Loc>]>,
) -> Arc<[ast::statement::Statement<Loc, Loc>]> {
    visitor.map_statement_list(stmts)
}

pub fn statement_list_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    stmts: &'ast [ast::statement::Statement<Loc, Type>],
) -> Result<(), E> {
    for stmt in stmts {
        visitor.statement(stmt)?;
    }
    Ok(())
}

pub fn map_statement_list_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    stmts: &'ast Arc<[ast::statement::Statement<Loc, Loc>]>,
) -> Arc<[ast::statement::Statement<Loc, Loc>]> {
    Arc::from(
        stmts
            .iter()
            .map(|stmt| visitor.map_statement(stmt))
            .collect::<Vec<_>>(),
    )
}

pub fn spread_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    spread: &'ast ast::expression::SpreadElement<Loc, Type>,
) -> Result<(), E> {
    let ast::expression::SpreadElement {
        loc: _,
        argument,
        comments,
    } = spread;
    visitor.expression(argument)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_spread_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    spread: &'ast ast::expression::SpreadElement<Loc, Loc>,
) -> ast::expression::SpreadElement<Loc, Loc> {
    let ast::expression::SpreadElement {
        loc,
        argument,
        comments,
    } = spread;
    let argument_ = visitor.map_expression(argument);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::SpreadElement {
        loc: loc.dupe(),
        argument: argument_,
        comments: comments_,
    }
}

pub fn spread_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    spread: &'ast ast::expression::object::SpreadProperty<Loc, Type>,
) -> Result<(), E> {
    let ast::expression::object::SpreadProperty {
        loc: _,
        argument,
        comments,
    } = spread;
    visitor.expression(argument)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_spread_property_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    spread: &'ast ast::expression::object::SpreadProperty<Loc, Loc>,
) -> ast::expression::object::SpreadProperty<Loc, Loc> {
    let ast::expression::object::SpreadProperty {
        loc,
        argument,
        comments,
    } = spread;
    let argument_ = visitor.map_expression(argument);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::object::SpreadProperty {
        loc: loc.dupe(),
        argument: argument_,
        comments: comments_,
    }
}

pub fn super_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    expr: &'ast ast::expression::Super<Loc>,
) -> Result<(), E> {
    let _ = loc;
    let ast::expression::Super { comments } = expr;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_super_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::Super<Loc>,
) -> ast::expression::Super<Loc> {
    let ast::expression::Super { comments } = expr;
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::Super {
        comments: comments_,
    }
}

pub fn switch_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    switch: &'ast ast::statement::Switch<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::statement::Switch {
        discriminant,
        cases,
        comments,
        exhaustive_out: _,
    } = switch;
    visitor.expression(discriminant)?;
    for case in cases.iter() {
        visitor.switch_case(case)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_switch_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    switch: &'ast ast::statement::Switch<Loc, Loc>,
) -> ast::statement::Switch<Loc, Loc> {
    let ast::statement::Switch {
        discriminant,
        cases,
        comments,
        exhaustive_out,
    } = switch;
    let discriminant_ = visitor.map_expression(discriminant);
    let cases_ = Arc::from(
        cases
            .iter()
            .map(|c| visitor.map_switch_case(c))
            .collect::<Vec<_>>(),
    );
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::Switch {
        discriminant: discriminant_,
        cases: cases_,
        comments: comments_,
        exhaustive_out: exhaustive_out.clone(),
    }
}

pub fn switch_case_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    case: &'ast ast::statement::switch::Case<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::switch::Case {
        loc: _,
        test,
        case_test_loc: _,
        consequent,
        comments,
    } = case;
    if let Some(test_expr) = test {
        visitor.expression(test_expr)?;
    }
    visitor.statement_list(consequent)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_switch_case_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    case: &'ast ast::statement::switch::Case<Loc, Loc>,
) -> ast::statement::switch::Case<Loc, Loc> {
    let ast::statement::switch::Case {
        loc,
        test,
        case_test_loc,
        consequent,
        comments,
    } = case;
    let test_ = test.as_ref().map(|t| visitor.map_expression(t));
    let consequent_ = Arc::from(
        consequent
            .iter()
            .map(|s| visitor.map_statement(s))
            .collect::<Vec<_>>(),
    );
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::switch::Case {
        loc: loc.dupe(),
        test: test_,
        case_test_loc: case_test_loc.dupe(),
        consequent: consequent_,
        comments: comments_,
    }
}

pub fn tagged_template_default<
    'ast,
    Loc: Dupe,
    Type: Dupe,
    C,
    E,
    V: AstVisitor<'ast, Loc, Type, C, E> + ?Sized,
>(
    visitor: &mut V,
    loc: &'ast Type,
    expr: &'ast ast::expression::TaggedTemplate<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::expression::TaggedTemplate {
        tag,
        targs,
        quasi,
        comments,
    } = expr;
    visitor.expression(tag)?;
    if let Some(targs) = targs {
        visitor.call_type_args(targs)?;
    }
    let (loc, lit) = quasi;
    visitor.template_literal(V::normalize_loc(loc), lit)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_tagged_template_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::TaggedTemplate<Loc, Loc>,
) -> ast::expression::TaggedTemplate<Loc, Loc> {
    let ast::expression::TaggedTemplate {
        tag,
        targs,
        quasi,
        comments,
    } = expr;
    let tag_ = visitor.map_expression(tag);
    let targs_ = targs.as_ref().map(|t| visitor.map_call_type_args(t));
    let (quasi_loc, quasi_lit) = quasi;
    let quasi_lit_ = visitor.map_template_literal(quasi_loc, quasi_lit);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::TaggedTemplate {
        tag: tag_,
        targs: targs_,
        quasi: (quasi_loc.dupe(), quasi_lit_),
        comments: comments_,
    }
}

pub fn template_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: C,
    lit: &'ast ast::expression::TemplateLiteral<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::expression::TemplateLiteral {
        quasis,
        expressions,
        comments,
    } = lit;
    for quasi in quasis.iter() {
        visitor.template_literal_element(quasi)?;
    }
    for expr in expressions.iter() {
        visitor.expression(expr)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_template_literal_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::TemplateLiteral<Loc, Loc>,
) -> ast::expression::TemplateLiteral<Loc, Loc> {
    let ast::expression::TemplateLiteral {
        quasis,
        expressions,
        comments,
    } = expr;
    let expressions_ = Arc::from(
        expressions
            .iter()
            .map(|e| visitor.map_expression(e))
            .collect::<Vec<_>>(),
    );
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::TemplateLiteral {
        quasis: quasis.clone(),
        expressions: expressions_,
        comments: comments_,
    }
}

pub fn template_literal_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    _visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    elem: &'ast ast::expression::template_literal::Element<Loc>,
) -> Result<(), E> {
    let _ = elem;
    Ok(())
}

pub fn map_template_literal_element_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    _visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    elem: &'ast ast::expression::template_literal::Element<Loc>,
) -> ast::expression::template_literal::Element<Loc> {
    elem.clone()
}

pub fn this_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    expr: &'ast ast::expression::This<Loc>,
) -> Result<(), E> {
    let _ = loc;
    let ast::expression::This { comments } = expr;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_this_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::This<Loc>,
) -> ast::expression::This<Loc> {
    let ast::expression::This { comments } = expr;
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::This {
        comments: comments_,
    }
}

pub fn throw_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    throw: &'ast ast::statement::Throw<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::statement::Throw { argument, comments } = throw;
    visitor.expression(argument)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_throw_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    throw: &'ast ast::statement::Throw<Loc, Loc>,
) -> ast::statement::Throw<Loc, Loc> {
    let ast::statement::Throw { argument, comments } = throw;
    let argument_ = visitor.map_expression(argument);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::Throw {
        argument: argument_,
        comments: comments_,
    }
}

pub fn try_catch_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    try_stmt: &'ast ast::statement::Try<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::statement::Try {
        block,
        handler,
        finalizer,
        comments,
    } = try_stmt;
    let (loc, block_stmt) = block;
    visitor.block(loc, block_stmt)?;
    if let Some(handler_clause) = handler {
        visitor.catch_clause(handler_clause)?;
    }
    if let Some((loc, finalizer_block)) = finalizer {
        visitor.block(loc, finalizer_block)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_try_catch_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    try_stmt: &'ast ast::statement::Try<Loc, Loc>,
) -> ast::statement::Try<Loc, Loc> {
    let ast::statement::Try {
        block,
        handler,
        finalizer,
        comments,
    } = try_stmt;
    let (block_loc, block_inner) = block;
    let block_ = (block_loc.dupe(), visitor.map_block(block_loc, block_inner));
    let handler_ = handler.as_ref().map(|h| visitor.map_catch_clause(h));
    let finalizer_ = finalizer
        .as_ref()
        .map(|(loc, fin)| (loc.dupe(), visitor.map_block(loc, fin)));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::Try {
        block: block_,
        handler: handler_,
        finalizer: finalizer_,
        comments: comments_,
    }
}

pub fn type_cast_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    expr: &'ast ast::expression::TypeCast<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::expression::TypeCast {
        expression,
        annot,
        comments,
    } = expr;
    visitor.expression(expression)?;
    visitor.type_annotation(annot)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_type_cast_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::TypeCast<Loc, Loc>,
) -> ast::expression::TypeCast<Loc, Loc> {
    let ast::expression::TypeCast {
        expression,
        annot,
        comments,
    } = expr;
    let expression_ = visitor.map_expression(expression);
    let annot_ = visitor.map_type_annotation(annot);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::TypeCast {
        expression: expression_,
        annot: annot_,
        comments: comments_,
    }
}

pub fn ts_satisfies_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    expr: &'ast ast::expression::TSSatisfies<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::expression::TSSatisfies {
        expression,
        annot,
        comments,
    } = expr;
    visitor.expression(expression)?;
    visitor.type_annotation(annot)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_ts_satisfies_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::TSSatisfies<Loc, Loc>,
) -> ast::expression::TSSatisfies<Loc, Loc> {
    let ast::expression::TSSatisfies {
        expression,
        annot,
        comments,
    } = expr;
    let expression_ = visitor.map_expression(expression);
    let annot_ = visitor.map_type_annotation(annot);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::TSSatisfies {
        expression: expression_,
        annot: annot_,
        comments: comments_,
    }
}

pub fn unary_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    expr: &'ast ast::expression::Unary<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::expression::Unary {
        argument,
        operator: _,
        comments,
    } = expr;
    visitor.expression(argument)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_unary_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::Unary<Loc, Loc>,
) -> ast::expression::Unary<Loc, Loc> {
    let ast::expression::Unary {
        operator,
        argument,
        comments,
    } = expr;
    let argument_ = visitor.map_expression(argument);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::Unary {
        operator: *operator,
        argument: argument_,
        comments: comments_,
    }
}

pub fn update_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    expr: &'ast ast::expression::Update<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::expression::Update {
        argument,
        operator: _,
        prefix: _,
        comments,
    } = expr;
    visitor.expression(argument)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_update_expression_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::Update<Loc, Loc>,
) -> ast::expression::Update<Loc, Loc> {
    let ast::expression::Update {
        operator,
        argument,
        prefix,
        comments,
    } = expr;
    let argument_ = visitor.map_expression(argument);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::Update {
        operator: operator.clone(),
        argument: argument_,
        prefix: *prefix,
        comments: comments_,
    }
}

pub fn variable_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    decl: &'ast ast::statement::VariableDeclaration<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::statement::VariableDeclaration {
        declarations,
        kind,
        comments,
    } = decl;
    for declarator in declarations.iter() {
        visitor.variable_declarator(*kind, declarator)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_variable_declaration_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    decl: &'ast ast::statement::VariableDeclaration<Loc, Loc>,
) -> ast::statement::VariableDeclaration<Loc, Loc> {
    let ast::statement::VariableDeclaration {
        declarations,
        kind,
        comments,
    } = decl;
    let declarations_ = Arc::from(
        declarations
            .iter()
            .map(|d| visitor.map_variable_declarator(*kind, d))
            .collect::<Vec<_>>(),
    );
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::VariableDeclaration {
        declarations: declarations_,
        kind: *kind,
        comments: comments_,
    }
}

pub fn variable_declarator_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: ast::VariableKind,
    declarator: &'ast ast::statement::variable::Declarator<Loc, Type>,
) -> Result<(), E> {
    let ast::statement::variable::Declarator {
        loc: _,
        id,
        init,
        definite: _,
    } = declarator;
    visitor.variable_declarator_pattern(kind, id)?;
    if let Some(init_expr) = init {
        visitor.expression(init_expr)?;
    }
    Ok(())
}

pub fn map_variable_declarator_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    kind: ast::VariableKind,
    declarator: &'ast ast::statement::variable::Declarator<Loc, Loc>,
) -> ast::statement::variable::Declarator<Loc, Loc> {
    let ast::statement::variable::Declarator {
        loc,
        id,
        init,
        definite,
    } = declarator;
    let id_ = visitor.map_variable_declarator_pattern(kind, id);
    let init_ = init.as_ref().map(|i| visitor.map_expression(i));
    ast::statement::variable::Declarator {
        loc: loc.dupe(),
        id: id_,
        init: init_,
        definite: *definite,
    }
}

pub fn while_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    stmt: &'ast ast::statement::While<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::statement::While {
        test,
        body,
        comments,
    } = stmt;
    visitor.predicate_expression(test)?;
    visitor.statement(body)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_while_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    stmt: &'ast ast::statement::While<Loc, Loc>,
) -> ast::statement::While<Loc, Loc> {
    let ast::statement::While {
        test,
        body,
        comments,
    } = stmt;
    let test_ = visitor.map_expression(test);
    let body_ = visitor.map_statement(body);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::While {
        test: test_,
        body: body_,
        comments: comments_,
    }
}

pub fn with_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    stmt: &'ast ast::statement::With<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::statement::With {
        object,
        body,
        comments,
    } = stmt;
    visitor.expression(object)?;
    visitor.statement(body)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_with_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    stmt: &'ast ast::statement::With<Loc, Loc>,
) -> ast::statement::With<Loc, Loc> {
    let ast::statement::With {
        object,
        body,
        comments,
    } = stmt;
    let object_ = visitor.map_expression(object);
    let body_ = visitor.map_statement(body);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::With {
        object: object_,
        body: body_,
        comments: comments_,
    }
}

pub fn type_alias_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Loc,
    alias: &'ast ast::statement::TypeAlias<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::statement::TypeAlias {
        id,
        tparams,
        right,
        comments,
    } = alias;
    visitor.binding_type_identifier(id)?;
    if let Some(tparams_val) = tparams {
        visitor.type_params(&TypeParamsContext::TypeAlias, tparams_val)?;
    }
    visitor.type_(right)?;
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_type_alias_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    alias: &'ast ast::statement::TypeAlias<Loc, Loc>,
) -> ast::statement::TypeAlias<Loc, Loc> {
    let ast::statement::TypeAlias {
        id,
        tparams,
        right,
        comments,
    } = alias;
    let id_ = visitor.map_identifier(id);
    let tparams_ = tparams
        .as_ref()
        .map(|tp| visitor.map_type_params(&TypeParamsContext::TypeAlias, tp));
    let right_ = visitor.map_type_(right);
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::statement::TypeAlias {
        id: id_,
        tparams: tparams_,
        right: right_,
        comments: comments_,
    }
}

pub fn yield_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    loc: &'ast Type,
    expr: &'ast ast::expression::Yield<Loc, Type>,
) -> Result<(), E> {
    let _ = loc;
    let ast::expression::Yield {
        argument,
        delegate: _,
        comments,
        result_out: _,
    } = expr;
    if let Some(arg) = argument {
        visitor.expression(arg)?;
    }
    visitor.syntax_opt(comments.as_ref())?;
    Ok(())
}

pub fn map_yield_default<'ast, Loc: Dupe, Type: Dupe, C, E>(
    visitor: &mut (impl AstVisitor<'ast, Loc, Type, C, E> + ?Sized),
    _loc: &'ast Loc,
    expr: &'ast ast::expression::Yield<Loc, Loc>,
) -> ast::expression::Yield<Loc, Loc> {
    let ast::expression::Yield {
        argument,
        comments,
        delegate,
        result_out,
    } = expr;
    let argument_ = argument.as_ref().map(|a| visitor.map_expression(a));
    let comments_ = visitor.map_syntax_opt(comments.as_ref());
    ast::expression::Yield {
        argument: argument_,
        comments: comments_,
        delegate: *delegate,
        result_out: result_out.clone(),
    }
}

/// For most of the visitors, we don't need statement_fork_point. Therefore, we can make it have
/// a more efficient implementation that just visit each statement one by one. For ones that do need
/// it, this visitor can help to achieve that. It comes with the perf overhead of creating a new
/// vector for each statements list.
pub trait MutableAstVisitorWithStatementForkPoint<'ast, Loc: 'ast + Dupe, Type: Dupe>:
    AstVisitor<'ast, Loc, Type>
{
    fn statement_list_mut(
        &mut self,
        statements: &'ast mut Vec<ast::statement::Statement<Loc, Loc>>,
    ) {
        let mut temp_statements = Vec::new();
        std::mem::swap(&mut temp_statements, statements);
        for stmt in temp_statements {
            statements.append(&mut self.statement_fork_point(stmt));
        }
    }

    fn statement_fork_point(
        &mut self,
        stmt: ast::statement::Statement<Loc, Loc>,
    ) -> Vec<ast::statement::Statement<Loc, Loc>> {
        vec![stmt]
    }
}
