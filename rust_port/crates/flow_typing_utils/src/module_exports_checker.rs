/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_analysis::scope_builder;
use flow_parser::ast;
use flow_parser::ast::expression::ExpressionInner;
use flow_parser::ast::statement::ExportKind;
use flow_parser::ast::statement::StatementInner;
use flow_parser::ast_utils;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser_utils::file_sig::FileSig;
use flow_typing_errors::error_message::ErrorMessage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModuleKind {
    Unknown,
    CommonJS,
    ES,
}

type State = (ModuleKind, Vec<ErrorMessage<ALoc>>);

fn add_es_export(
    loc: ALoc,
    module_kind: ModuleKind,
    errs: &mut Vec<ErrorMessage<ALoc>>,
) -> ModuleKind {
    match module_kind {
        ModuleKind::CommonJS => {
            errs.push(ErrorMessage::EIndeterminateModuleType(loc));
            module_kind
        }
        ModuleKind::Unknown | ModuleKind::ES => ModuleKind::ES,
    }
}

fn set_cjs_exports(
    mod_exp_loc: ALoc,
    module_kind: ModuleKind,
    errs: &mut Vec<ErrorMessage<ALoc>>,
) -> ModuleKind {
    match module_kind {
        ModuleKind::Unknown | ModuleKind::CommonJS => ModuleKind::CommonJS,
        ModuleKind::ES => {
            let err = ErrorMessage::EIndeterminateModuleType(mod_exp_loc);
            errs.push(err);
            module_kind
        }
    }
}

struct ExportsErrorChecker<'a> {
    is_local_use: &'a dyn Fn(&ALoc) -> bool,
    state: State,
}

impl<'a> ExportsErrorChecker<'a> {
    fn new(is_local_use: &'a dyn Fn(&ALoc) -> bool) -> Self {
        Self {
            is_local_use,
            state: (ModuleKind::Unknown, Vec::new()),
        }
    }

    fn add_exports(&mut self, loc: &ALoc, kind: &ExportKind) {
        match kind {
            ExportKind::ExportValue => {
                let (module_kind, errs) = &mut self.state;
                *module_kind = add_es_export(loc.dupe(), *module_kind, errs);
            }
            ExportKind::ExportType => {}
        }
    }

    fn set_cjs_exports_method(&mut self, mod_exp_loc: &ALoc) {
        let (module_kind, errs) = &mut self.state;
        *module_kind = set_cjs_exports(mod_exp_loc.dupe(), *module_kind, errs);
    }

    fn add_cjs_export(&mut self, mod_exp_loc: &ALoc) {
        let (module_kind, errs) = &mut self.state;
        *module_kind = set_cjs_exports(mod_exp_loc.dupe(), *module_kind, errs);
    }

    fn add_error(&mut self, err: ErrorMessage<ALoc>) {
        self.state.1.push(err);
    }

    fn handle_assignment(
        &mut self,
        is_toplevel: bool,
        _loc: &ALoc,
        expr: &ast::expression::Assignment<ALoc, ALoc>,
    ) {
        let ast::expression::Assignment {
            operator,
            left,
            right,
            comments: _,
        } = expr;
        let (left, _) = ast_utils::unwrap_nonnull_lhs(left);
        // Handle exports
        match (operator, &*left) {
            // module.exports = ...
            (
                None,
                ast::pattern::Pattern::Expression {
                    loc: mod_exp_loc,
                    inner,
                },
            ) if let ExpressionInner::Member { inner: member, .. } = inner.deref().deref()
                && let ExpressionInner::Identifier {
                    loc: module_loc,
                    inner: module_ident,
                    ..
                } = member.object.deref()
                && module_ident.name == "module"
                && let ast::expression::member::Property::PropertyIdentifier(exports_ident) =
                    &member.property
                && exports_ident.name == "exports"
                && !(self.is_local_use)(module_loc) =>
            {
                if !(self.is_local_use)(module_loc) {
                    self.set_cjs_exports_method(mod_exp_loc);
                }
                let Ok(()) = self.expression(right);
                if !is_toplevel {
                    self.add_error(ErrorMessage::EBadExportPosition(mod_exp_loc.dupe()));
                }
            }
            // `exports.foo = ...`
            // `module.exports.foo = ...`
            (None, ast::pattern::Pattern::Expression { inner, .. })
                if let ExpressionInner::Member { inner: member, .. } = inner.deref().deref()
                    && let ast::expression::member::Property::PropertyIdentifier(_) =
                        &member.property
                    && let Some((mod_exp_loc, module_loc)) = (match member.object.deref() {
                        ExpressionInner::Identifier {
                            loc, inner: ident, ..
                        } if ident.name == "exports" => Some((loc, loc)),
                        ExpressionInner::Member {
                            loc: mexp_loc,
                            inner: inner_member,
                            ..
                        } if let ExpressionInner::Identifier {
                            loc: mod_loc,
                            inner: ident,
                            ..
                        } = inner_member.object.deref()
                            && ident.name == "module"
                            && let ast::expression::member::Property::PropertyIdentifier(prop) =
                                &inner_member.property
                            && prop.name == "exports" =>
                        {
                            Some((mexp_loc, mod_loc))
                        }
                        _ => None,
                    })
                    && !(self.is_local_use)(module_loc) =>
            {
                self.add_cjs_export(mod_exp_loc);
                let Ok(()) = self.expression(right);
                if !is_toplevel {
                    self.add_error(ErrorMessage::EBadExportPosition(mod_exp_loc.dupe()));
                }
            }
            // `module = ...`
            (None, ast::pattern::Pattern::Identifier { inner, .. })
                if (inner.name.name == "exports" || inner.name.name == "module")
                    && !(self.is_local_use)(&inner.name.loc) =>
            {
                let id = inner.name.name.dupe();
                let loc = inner.name.loc.dupe();
                let Ok(()) = self.expression(right);
                self.add_error(ErrorMessage::EBadExportContext(Box::new((id, loc))));
            }
            _ => {
                let Ok(()) = ast_visitor::assignment_default(self, _loc, expr);
            }
        }
    }

    fn check_declare_module_or_declare_namespace_body(
        &mut self,
        body: &[ast::statement::Statement<ALoc, ALoc>],
    ) {
        let mut module_kind = ModuleKind::Unknown;
        for stmt in body {
            let loc = match &**stmt {
                StatementInner::DeclareModuleExports { loc, .. } => loc.dupe(),
                StatementInner::DeclareExportDeclaration { loc, .. } => loc.dupe(),
                _ => continue,
            };
            match (&module_kind, &**stmt) {
                // The first time we see either a `declare export` or a
                // `declare module.exports`, we lock in the kind of the module.
                //
                // `declare export type` and `declare export interface` are the two
                // exceptions to this rule because they are valid in both CommonJS
                // and ES modules (and thus do not indicate an intent for either).
                (ModuleKind::Unknown, StatementInner::DeclareModuleExports { .. }) => {
                    module_kind = ModuleKind::CommonJS;
                }
                (ModuleKind::Unknown, StatementInner::DeclareExportDeclaration { inner, .. }) => {
                    match &inner.declaration {
                        Some(
                            ast::statement::declare_export_declaration::Declaration::NamedType {
                                ..
                            },
                        )
                        | Some(
                            ast::statement::declare_export_declaration::Declaration::Interface {
                                ..
                            },
                        ) => {}
                        _ => {
                            module_kind = ModuleKind::ES;
                        }
                    }
                }
                // We allow more than one `declare module.exports` statement
                (ModuleKind::CommonJS, StatementInner::DeclareModuleExports { .. }) => {}
                // It's never ok to mix and match `declare export` and
                // `declare module.exports` in the same module because it leaves the
                // kind of the module (CommonJS vs ES) ambiguous.
                //
                // The 1 exception to this rule is that `export type/interface` are
                // both ok in CommonJS modules.
                (ModuleKind::ES, StatementInner::DeclareModuleExports { .. }) => {
                    self.add_error(ErrorMessage::EIndeterminateModuleType(loc));
                }
                (ModuleKind::CommonJS, StatementInner::DeclareExportDeclaration { inner, .. }) => {
                    match &inner.declaration {
                        Some(
                            ast::statement::declare_export_declaration::Declaration::NamedType {
                                ..
                            },
                        )
                        | Some(
                            ast::statement::declare_export_declaration::Declaration::Interface {
                                ..
                            },
                        ) => {}
                        _ => {
                            self.add_error(ErrorMessage::EIndeterminateModuleType(loc));
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

impl<'ast, 'a> AstVisitor<'ast, ALoc> for ExportsErrorChecker<'a> {
    fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
        loc
    }

    fn normalize_type(type_: &'ast ALoc) -> &'ast ALoc {
        type_
    }

    fn expression(&mut self, expr: &'ast ast::expression::Expression<ALoc, ALoc>) -> Result<(), !> {
        match &**expr {
            ExpressionInner::Identifier {
                loc, inner: ident, ..
            } if (ident.name == "module" || ident.name == "exports")
                && !(self.is_local_use)(&ident.loc) =>
            {
                self.add_error(ErrorMessage::EBadExportContext(Box::new((
                    ident.name.dupe(),
                    ident.loc.dupe(),
                ))));
            }
            _ => {}
        }
        ast_visitor::expression_default(self, expr)
    }

    fn binary(
        &mut self,
        loc: &'ast ALoc,
        expr: &'ast ast::expression::Binary<ALoc, ALoc>,
    ) -> Result<(), !> {
        let is_module_or_exports = |subexpr: &ast::expression::Expression<ALoc, ALoc>| -> bool {
            matches!(
                subexpr.deref(),
                ExpressionInner::Identifier { inner: ident, .. }
                if ident.name == "module" || ident.name == "exports"
            )
        };
        let is_legal_operator = |op: &ast::expression::BinaryOperator| -> bool {
            matches!(
                op,
                ast::expression::BinaryOperator::StrictEqual
                    | ast::expression::BinaryOperator::StrictNotEqual
            )
        };
        let ast::expression::Binary {
            operator,
            left,
            right,
            comments: _,
        } = expr;
        // Allow e.g. `require.main === module` by avoiding the recursive calls (where the errors
        // are generated) if the AST matches specific patterns.
        if is_legal_operator(operator) {
            if !is_module_or_exports(left) {
                self.expression(left)?;
            }
            if !is_module_or_exports(right) {
                self.expression(right)?;
            }
            Ok(())
        } else {
            ast_visitor::binary_default(self, loc, expr)
        }
    }

    fn member(
        &mut self,
        loc: &'ast ALoc,
        expr: &'ast ast::expression::Member<ALoc, ALoc>,
    ) -> Result<(), !> {
        let ast::expression::Member {
            object,
            property,
            comments: _,
        } = expr;
        // Strip the loc to simplify the patterns
        let object_inner = &**object;

        // This gets called when patterns like `module.id` appear on the LHS of an
        // assignment, in addition to when they appear in ordinary expression
        // locations. Therefore we have to prevent anything that would be dangerous
        // if it appeared on the LHS side of an assignment. Ordinary export
        // statements are handled by handle_assignment, which stops recursion so we
        // don't arrive here in those cases.
        match (object_inner, property) {
            // Allow `module.anythingButExports`
            (
                ExpressionInner::Identifier { inner: ident, .. },
                ast::expression::member::Property::PropertyIdentifier(prop_ident),
            ) if ident.name == "module" && prop_ident.name != "exports" => {}
            // Allow `module.exports.whatever` -- this is safe because handle_assignment has already
            // looked for assignments to it before recursing down here.
            (
                ExpressionInner::Member {
                    inner: inner_member,
                    ..
                },
                ast::expression::member::Property::PropertyIdentifier(_),
            ) if matches!(
                &*inner_member.object,
                ExpressionInner::Identifier { inner: ident, .. } if ident.name == "module"
            ) && matches!(
                &inner_member.property,
                ast::expression::member::Property::PropertyIdentifier(prop) if prop.name == "exports"
            ) =>
            {
                // In these cases we don't know much about the property so we should recurse
                self.member_property(property)?;
            }
            // Allow `exports.whatever`, for the same reason as above
            (
                ExpressionInner::Identifier { inner: ident, .. },
                ast::expression::member::Property::PropertyIdentifier(_),
            ) if ident.name == "exports" => {
                // In these cases we don't know much about the property so we should recurse
                self.member_property(property)?;
            }
            _ => {
                ast_visitor::member_default(self, loc, expr)?;
            }
        }
        Ok(())
    }

    fn export_default_declaration(
        &mut self,
        loc: &'ast ALoc,
        decl: &'ast ast::statement::ExportDefaultDeclaration<ALoc, ALoc>,
    ) -> Result<(), !> {
        let ast::statement::ExportDefaultDeclaration {
            default: _,
            declaration: _,
            comments: _,
        } = decl;
        self.add_exports(loc, &ExportKind::ExportValue);
        ast_visitor::export_default_declaration_default(self, loc, decl)
    }

    fn export_named_declaration(
        &mut self,
        loc: &'ast ALoc,
        decl: &'ast ast::statement::ExportNamedDeclaration<ALoc, ALoc>,
    ) -> Result<(), !> {
        let ast::statement::ExportNamedDeclaration {
            export_kind,
            source: _,
            specifiers,
            declaration,
            attributes: _,
            comments: _,
        } = decl;
        if declaration.is_some() {
            self.add_exports(loc, export_kind);
        }
        match specifiers {
            None => {}
            Some(ast::statement::export_named_declaration::Specifier::ExportSpecifiers(specs)) => {
                if flow_parser::ast_utils::export_specifiers_has_value_export(*export_kind, specs) {
                    self.add_exports(loc, &ast::statement::ExportKind::ExportValue);
                } else {
                    self.add_exports(loc, &ast::statement::ExportKind::ExportType);
                }
            }
            Some(ast::statement::export_named_declaration::Specifier::ExportBatchSpecifier(_)) => {
                self.add_exports(loc, export_kind);
            }
        }
        ast_visitor::export_named_declaration_default(self, loc, decl)
    }

    fn declare_module_exports(
        &mut self,
        loc: &'ast ALoc,
        exports: &'ast ast::statement::DeclareModuleExports<ALoc, ALoc>,
    ) -> Result<(), !> {
        self.set_cjs_exports_method(loc);
        ast_visitor::declare_module_exports_default(self, loc, exports)
    }

    fn declare_export_declaration(
        &mut self,
        loc: &'ast ALoc,
        decl: &'ast ast::statement::DeclareExportDeclaration<ALoc, ALoc>,
    ) -> Result<(), !> {
        let ast::statement::DeclareExportDeclaration {
            default: _,
            source: _,
            specifiers,
            declaration,
            comments: _,
        } = decl;
        if let Some(declaration) = declaration {
            let export_kind = match declaration {
                ast::statement::declare_export_declaration::Declaration::Variable { .. }
                | ast::statement::declare_export_declaration::Declaration::Function { .. }
                | ast::statement::declare_export_declaration::Declaration::Class { .. }
                | ast::statement::declare_export_declaration::Declaration::Component { .. }
                | ast::statement::declare_export_declaration::Declaration::DefaultType { .. }
                | ast::statement::declare_export_declaration::Declaration::Enum { .. }
                | ast::statement::declare_export_declaration::Declaration::Namespace { .. } => {
                    ExportKind::ExportValue
                }
                ast::statement::declare_export_declaration::Declaration::NamedType { .. }
                | ast::statement::declare_export_declaration::Declaration::NamedOpaqueType {
                    ..
                }
                | ast::statement::declare_export_declaration::Declaration::Interface { .. } => {
                    ExportKind::ExportType
                }
            };
            self.add_exports(loc, &export_kind);
        }
        if specifiers.is_some() {
            self.add_exports(loc, &ExportKind::ExportValue);
        }
        ast_visitor::declare_export_declaration_default(self, loc, decl)
    }

    fn assignment(
        &mut self,
        loc: &'ast ALoc,
        expr: &'ast ast::expression::Assignment<ALoc, ALoc>,
    ) -> Result<(), !> {
        self.handle_assignment(false, loc, expr);
        Ok(())
    }

    // skip declare module for other visits designed for toplevel exports only
    fn declare_module(
        &mut self,
        _loc: &'ast ALoc,
        m: &'ast ast::statement::DeclareModule<ALoc, ALoc>,
    ) -> Result<(), !> {
        let (_, block) = &m.body;
        self.check_declare_module_or_declare_namespace_body(&block.body);
        Ok(())
    }

    // skip declare namespace for other visits designed for toplevel exports only
    fn declare_namespace(
        &mut self,
        _loc: &'ast ALoc,
        ns: &'ast ast::statement::DeclareNamespace<ALoc, ALoc>,
    ) -> Result<(), !> {
        let (_, block) = &ns.body;
        self.check_declare_module_or_declare_namespace_body(&block.body);
        Ok(())
    }

    fn toplevel_statement_list(
        &mut self,
        stmts: &'ast [ast::statement::Statement<ALoc, ALoc>],
    ) -> Result<(), !> {
        for stmt in stmts {
            match &**stmt {
                StatementInner::Expression { inner, .. } => match &*inner.expression {
                    ExpressionInner::Assignment { loc, inner: assg } => {
                        self.handle_assignment(true, loc, assg);
                    }
                    _ => {
                        self.expression(&inner.expression)?;
                    }
                },
                _ => {
                    self.statement(stmt)?;
                }
            }
        }
        Ok(())
    }
}

// Now that we've determined the kind of the export, we can filter out
// BadExportContext from ES modules. These errors are only
// relevant for CommonJS modules, since their goal is to capture aliasing of
// `module` and `module.exports`. For ES modules these uses should be allowed,
// so we discard these errors to allow more flexibility. Note that this pass only
// accummulates BadExportContext errors.
fn filter_irrelevant_errors(module_kind: ModuleKind, errors: &mut Vec<ErrorMessage<ALoc>>) {
    match module_kind {
        ModuleKind::Unknown | ModuleKind::CommonJS => {}
        ModuleKind::ES => {
            errors.retain(|err| {
                !matches!(
                    err,
                    ErrorMessage::EBadExportPosition(_) | ErrorMessage::EBadExportContext(deref!((_, _)))
                )
            });
        }
    }
}

pub fn check_program(
    file_sig: &FileSig,
    ast: &ast::Program<ALoc, ALoc>,
) -> Vec<ErrorMessage<ALoc>> {
    let scope_info = scope_builder::program(true, true, file_sig.requires().is_empty(), ast);
    let is_local_use = |loc: &ALoc| scope_info.is_local_use(loc);
    let mut walk = ExportsErrorChecker::new(&is_local_use);
    let Ok(()) = walk.program(ast);
    let (module_kind, mut errors) = walk.state;
    filter_irrelevant_errors(module_kind, &mut errors);
    errors
}
