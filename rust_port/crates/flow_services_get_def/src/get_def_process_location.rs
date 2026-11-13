/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::ops::Deref;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::flow_import_specifier;
use flow_common::reason;
use flow_common::reason::Reason;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::expression;
use flow_parser::ast::statement;
use flow_parser::ast_utils;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::loc::Loc;
use flow_typing::typed_ast_finder;
use flow_typing::typed_ast_finder::EnclosingNode;
use flow_typing_context::Context;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_statement::statement as typing_statement;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_util;
use flow_typing_utils::typed_ast_utils::AvailableAst;

use crate::get_def_request::GetDefRequest;
use crate::get_def_request::MemberInfo;
use crate::get_def_types::Purpose;

#[derive(Debug, Clone, Copy)]
pub enum InternalError {
    EnclosingNodeError,
    OnDemandTastError,
    /// Worker received cancel signal during typed AST inference
    Canceled,
    /// Per-file `merge_timeout` exceeded during typed AST inference
    TimedOut,
    /// `$Flow$DebugThrow` was hit during typed AST inference
    DebugThrow,
}

/// This type is distinct from the one raised by the searcher because
/// it would never make sense for the searcher to raise LocNotFound
#[derive(Debug, Clone)]
pub enum ProcessLocationResult {
    OwnNamedDef(ALoc, FlowSmolStr),
    OwnUnnamedDef(ALoc),
    ModuleDef(ALoc),
    ModuleTypeDef(Type),
    Request(GetDefRequest<ALoc, (ALoc, Type)>),
    Empty(&'static str),
    LocNotFound,
    InternalError(InternalError),
}

fn def_loc_of_reason(r: &Reason) -> ALoc {
    match r.annot_loc() {
        Some(aloc) => aloc.dupe(),
        None => r.def_loc_opt().unwrap_or(&r.loc).dupe(),
    }
}

fn def_loc_of_t(t: &Type) -> ALoc {
    let r = type_util::reason_of_t(t);
    def_loc_of_reason(r)
}

/// here lies the difference between "Go to Definition" and "Go to Type Definition":
/// the former should stop on annot_loc (where the value was annotated), while the
/// latter should jump to the def_loc (where the type was defined).
///
/// for now, we only implement Go to Definition; if we want to do Go to Type
/// Definition, it would ignore the annot loc.
pub fn process_type_request<'cx>(cx: &mut Context<'cx>, t: &Type) -> Result<ALoc, String> {
    fn loop_fn<'cx>(
        cx: &mut Context<'cx>,
        mut seen: BTreeSet<i32>,
        t: &Type,
    ) -> Result<ALoc, String> {
        match &**t {
            TypeInner::OpenT(tvar) => {
                let root = cx.find_root_id(tvar.id() as i32);
                if seen.contains(&root) {
                    Ok(def_loc_of_t(t))
                } else {
                    let possible = flow_js_utils::possible_types_of_type(cx, t);
                    match possible.as_slice() {
                        [t_prime] => {
                            seen.insert(root);
                            loop_fn(cx, seen, t_prime)
                        }
                        [] => Err("No possible types".into()),
                        _ => Err("More than one possible type".into()),
                    }
                }
            }
            _ => Ok(def_loc_of_t(t)),
        }
    }
    loop_fn(cx, BTreeSet::new(), t)
}

/// Determines if the given expression is a [require()] call, or a member expression
/// containing one, like [require('foo').bar].
fn is_require<T: Dupe>(
    is_legit_require: &impl Fn(&T) -> bool,
    expr: &expression::Expression<ALoc, T>,
) -> bool {
    match expr.deref() {
        expression::ExpressionInner::Member { inner, .. } => {
            is_require(is_legit_require, &inner.object)
        }
        expression::ExpressionInner::Call { inner, .. } => {
            if let expression::ExpressionInner::Identifier {
                inner: callee_id, ..
            } = inner.callee.deref()
            {
                if callee_id.name.as_str() == "require" {
                    if let Some(expression::ExpressionOrSpread::Expression(source_expr)) =
                        inner.arguments.arguments.first()
                    {
                        let source_annot = source_expr.loc();
                        if is_legit_require(source_annot) {
                            return true;
                        }
                    }
                }
            }
            false
        }
        _ => false,
    }
}

struct RequireDeclaratorInfo<T> {
    toplevel_pattern_annot: T,
    require_t: T,
}

pub(crate) enum Found {
    // stops walking the tree
    Found,
    InternalError(InternalError),
}

pub(crate) trait SearcherCallback<T: Dupe> {
    fn loc_of_annot(&self, annot: &T) -> ALoc;

    fn annot_loc(annot: &T) -> &ALoc;

    fn type_from_enclosing_node(
        &mut self,
        annot: &T,
        enclosing_node: &EnclosingNode<ALoc, T>,
    ) -> Result<Type, Found>;

    fn get_module_def_loc(&mut self, annot: &T, module_name: &str) -> Result<ALoc, Found>;

    fn remote_name_def_loc_of_import_named_specifier(
        &mut self,
        decl: &statement::import_declaration::NamedSpecifier<ALoc, T>,
        enclosing_node: &EnclosingNode<ALoc, T>,
    ) -> Result<Option<ALoc>, Found>;

    fn imported_name_def_loc_of_export_named_declaration_specifier(
        &mut self,
        spec: &statement::export_named_declaration::ExportSpecifier<ALoc, T>,
        enclosing_node: &EnclosingNode<ALoc, T>,
    ) -> Result<Option<ALoc>, Found>;

    fn remote_default_name_def_loc_of_import_declaration(
        &mut self,
        loc: &ALoc,
        decl: &statement::ImportDeclaration<ALoc, T>,
    ) -> Result<Option<ALoc>, Found>;

    fn props_of_jsx_element(
        &mut self,
        loc: &T,
        expr: &ast::jsx::Element<ALoc, T>,
    ) -> Result<(ALoc, Type), Found>;
}

struct Searcher<'a, T: Dupe, C: SearcherCallback<T>> {
    callback: &'a mut C,
    is_local_use: &'a dyn Fn(&ALoc) -> bool,
    is_legit_require: &'a dyn Fn(&ALoc) -> bool,
    covers_target: &'a dyn Fn(&ALoc) -> bool,
    purpose: Purpose,
    require_declarator_info: Option<RequireDeclaratorInfo<T>>,
    in_graphql_tagged_literal: bool,
    available_private_names: FlowOrdMap<FlowSmolStr, ALoc>,
    found_loc: ProcessLocationResult,
    enclosing_node_stack: Vec<EnclosingNode<ALoc, T>>,
}

impl<'a, T: Dupe, C: SearcherCallback<T>> Searcher<'a, T, C> {
    fn new(
        callback: &'a mut C,
        is_local_use: &'a dyn Fn(&ALoc) -> bool,
        is_legit_require: &'a dyn Fn(&ALoc) -> bool,
        covers_target: &'a dyn Fn(&ALoc) -> bool,
        purpose: Purpose,
    ) -> Self {
        Searcher {
            callback,
            is_local_use,
            is_legit_require,
            covers_target,
            purpose,
            require_declarator_info: None,
            in_graphql_tagged_literal: false,
            available_private_names: FlowOrdMap::new(),
            found_loc: ProcessLocationResult::LocNotFound,
            enclosing_node_stack: Vec::new(),
        }
    }

    fn annot_covers_target(&self, annot: &T) -> bool {
        (self.covers_target)(&self.callback.loc_of_annot(annot))
    }

    fn is_legit_require_annot(&self, annot: &T) -> bool {
        (self.is_legit_require)(&self.callback.loc_of_annot(annot))
    }

    fn get_found_loc(&self) -> &ProcessLocationResult {
        &self.found_loc
    }

    fn own_named_def(&mut self, loc: ALoc, name: FlowSmolStr) -> Result<!, Found> {
        self.found_loc = ProcessLocationResult::OwnNamedDef(loc, name);
        Err(Found::Found)
    }

    fn own_unnamed_def(&mut self, loc: ALoc) -> Result<!, Found> {
        self.found_loc = ProcessLocationResult::OwnUnnamedDef(loc);
        Err(Found::Found)
    }

    fn module_def(&mut self, l: ALoc) -> Result<!, Found> {
        self.found_loc = ProcessLocationResult::ModuleDef(l);
        Err(Found::Found)
    }

    fn found_empty(&mut self, x: &'static str) -> Result<!, Found> {
        self.found_loc = ProcessLocationResult::Empty(x);
        Err(Found::Found)
    }

    fn request(&mut self, x: GetDefRequest<ALoc, (ALoc, Type)>) -> Result<!, Found> {
        self.found_loc = ProcessLocationResult::Request(x);
        Err(Found::Found)
    }

    fn type_from_enclosing_node(&mut self, annot: &T) -> Result<Type, Found> {
        let enclosing_node = self
            .enclosing_node_stack
            .last()
            .expect("enclosing_node_stack is empty");
        self.callback
            .type_from_enclosing_node(annot, enclosing_node)
    }

    fn module_def_for_entire_module_related_id(
        &mut self,
        module_def_f: impl FnOnce(&mut Self) -> Result<!, Found>,
        id: &ast::Identifier<ALoc, T>,
    ) -> Result<(), Found> {
        let name_annot = &id.loc;
        if self.annot_covers_target(name_annot) {
            match self.purpose {
                Purpose::GoToDefinition | Purpose::JSDoc => {
                    module_def_f(self)?;
                }
                Purpose::FindReferences => {
                    let loc = self.callback.loc_of_annot(name_annot);
                    self.own_named_def(loc, id.name.dupe())?;
                }
            }
        }
        Ok(())
    }
}

impl<'a, 'ast, T: Dupe + PartialEq + 'ast, C: SearcherCallback<T>>
    AstVisitor<'ast, ALoc, T, &'ast ALoc, Found> for Searcher<'a, T, C>
{
    fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
        loc
    }

    fn normalize_type(type_: &'ast T) -> &'ast ALoc {
        C::annot_loc(type_)
    }

    fn program(&mut self, prog: &'ast ast::Program<ALoc, T>) -> Result<(), Found> {
        self.enclosing_node_stack
            .push(EnclosingNode::EnclosingProgram(prog.clone()));
        let result = ast_visitor::program_default(self, prog);
        self.enclosing_node_stack.pop();
        result
    }

    fn statement(&mut self, stmt: &'ast statement::Statement<ALoc, T>) -> Result<(), Found> {
        self.enclosing_node_stack
            .push(EnclosingNode::EnclosingStatement(stmt.dupe()));
        let result = ast_visitor::statement_default(self, stmt);
        self.enclosing_node_stack.pop();
        result
    }

    fn variable_declarator(
        &mut self,
        kind: ast::VariableKind,
        declarator: &'ast statement::variable::Declarator<ALoc, T>,
    ) -> Result<(), Found> {
        let id_annot = declarator.id.loc();
        // If a variable declarator's initializer contains `require()`, then we want to jump
        // through it into the imported module. To do this, we set the `in_require_declarator`
        // flag, which we use when we visit the id, in lieu of parent pointers.
        let info = match &declarator.init {
            Some(init)
                if is_require(&|annot: &T| self.is_legit_require_annot(annot), init)
                    && self.annot_covers_target(id_annot) =>
            {
                Some(RequireDeclaratorInfo {
                    toplevel_pattern_annot: id_annot.dupe(),
                    require_t: init.loc().dupe(),
                })
            }
            _ => None,
        };
        let saved_require_declarator_info =
            std::mem::replace(&mut self.require_declarator_info, info);
        let result = ast_visitor::variable_declarator_default(self, kind, declarator);
        self.require_declarator_info = saved_require_declarator_info;
        result
    }

    fn import_source(
        &mut self,
        source_annot: &'ast T,
        source: &'ast ast::StringLiteral<ALoc>,
    ) -> Result<(), Found> {
        if self.annot_covers_target(source_annot) {
            let module_name = source.value.as_str();
            let loc = self
                .callback
                .get_module_def_loc(source_annot, module_name)?;
            self.module_def(loc)?;
        }
        ast_visitor::import_source_default(self, source_annot, source)
    }

    fn import_named_specifier(
        &mut self,
        _import_kind: ast::statement::ImportKind,
        decl: &'ast statement::import_declaration::NamedSpecifier<ALoc, T>,
    ) -> Result<(), Found> {
        let local = &decl.local;
        let remote_annot = &decl.remote.loc;
        let name = &decl.remote.name;

        if self.annot_covers_target(remote_annot) {
            let enclosing = self
                .enclosing_node_stack
                .last()
                .expect("enclosing_node_stack is empty");
            match self
                .callback
                .remote_name_def_loc_of_import_named_specifier(decl, enclosing)?
            {
                Some(l) => self.own_named_def(l, name.dupe())?,
                None => {
                    let loc = self.callback.loc_of_annot(remote_annot);
                    self.own_named_def(loc, "default".into())?;
                }
            }
        }
        if let Some(local_id) = local {
            let local_annot = &local_id.loc;
            if self.annot_covers_target(local_annot) {
                let enclosing = self
                    .enclosing_node_stack
                    .last()
                    .expect("enclosing_node_stack is empty");
                match self
                    .callback
                    .remote_name_def_loc_of_import_named_specifier(decl, enclosing)?
                {
                    Some(l) => self.own_named_def(l, name.dupe())?,
                    None => {
                        let loc = self.callback.loc_of_annot(local_annot);
                        self.own_named_def(loc, "default".into())?;
                    }
                }
            }
        }
        Ok(())
    }

    fn export_named_declaration(
        &mut self,
        loc: &'ast ALoc,
        decl: &'ast statement::ExportNamedDeclaration<ALoc, T>,
    ) -> Result<(), Found> {
        let source = &decl.source;
        let specifiers = &decl.specifiers;

        if let (
            Some((source_annot, source_lit)),
            Some(statement::export_named_declaration::Specifier::ExportBatchSpecifier(
                statement::export_named_declaration::ExportBatchSpecifier {
                    specifier: Some(id),
                    ..
                },
            )),
        ) = (source, specifiers)
        {
            let module_name = source_lit.value.as_str();
            let source_annot_clone = source_annot.dupe();
            self.module_def_for_entire_module_related_id(
                |this| match this
                    .callback
                    .get_module_def_loc(&source_annot_clone, module_name)
                {
                    Ok(loc) => this.module_def(loc),
                    Err(e) => Err(e),
                },
                id,
            )?;
        }
        ast_visitor::export_named_declaration_default(self, loc, decl)
    }

    fn export_named_declaration_specifier(
        &mut self,
        spec: &'ast statement::export_named_declaration::ExportSpecifier<ALoc, T>,
    ) -> Result<(), Found> {
        let local = &spec.local;
        let exported = &spec.exported;
        let from_remote = spec.from_remote;

        if let Some(exported_id) = exported {
            let annot = &exported_id.loc;
            if self.annot_covers_target(annot) {
                // Either `export {foo as bar}` or `export {foo as bar} from '...'`
                // In both case, get-def on bar should jump to itself
                let loc = self.callback.loc_of_annot(annot);
                self.own_named_def(loc, exported_id.name.dupe())?;
            }
        }

        let local_annot = &local.loc;
        let local_name = &local.name;
        if self.annot_covers_target(local_annot) {
            if from_remote {
                let enclosing = self
                    .enclosing_node_stack
                    .last()
                    .expect("enclosing_node_stack is empty");
                match self
                    .callback
                    .imported_name_def_loc_of_export_named_declaration_specifier(spec, enclosing)?
                {
                    // When we have imported_name_def_loc, we must be in the case of
                    // `export {foo [as bar]?} from '...'.
                    // In this case we should jump to the remote def loc stored in typed AST
                    Some(l) => self.own_named_def(l, local_name.dupe())?,
                    // When we don't have have imported_name_def_loc, then there is a type error.
                    // Similar to how we handle imported names with type error, we make it jump to itself.
                    None => {
                        let loc = self.callback.loc_of_annot(local_annot);
                        self.own_named_def(loc, local_name.dupe())?;
                    }
                }
            } else {
                // Given `export {foo}`, we should use the usual
                // use-def analysis result from scope builder.
                let loc = self.callback.loc_of_annot(local_annot);
                self.request(GetDefRequest::Identifier {
                    name: local_name.dupe(),
                    loc,
                })?;
            }
        }
        Ok(())
    }

    fn import_declaration(
        &mut self,
        loc: &'ast ALoc,
        decl: &'ast statement::ImportDeclaration<ALoc, T>,
    ) -> Result<(), Found> {
        let default = &decl.default;
        let specifiers = &decl.specifiers;
        let (source_annot, source_lit) = &decl.source;
        let module_name = source_lit.value.as_str();

        if let Some(default_id) = default {
            let annot = &default_id.identifier.loc;
            if self.annot_covers_target(annot) {
                match self
                    .callback
                    .remote_default_name_def_loc_of_import_declaration(loc, decl)?
                {
                    Some(l) => self.own_named_def(l, "default".into())?,
                    None => {
                        let loc = self.callback.loc_of_annot(annot);
                        self.own_named_def(loc, "default".into())?;
                    }
                }
            }
        }
        if let Some(specifiers_inner) = specifiers {
            match specifiers_inner {
                statement::import_declaration::Specifier::ImportNamedSpecifiers(_) => {}
                statement::import_declaration::Specifier::ImportNamespaceSpecifier((_, id)) => {
                    let source_annot_clone = source_annot.dupe();
                    self.module_def_for_entire_module_related_id(
                        |this| match this
                            .callback
                            .get_module_def_loc(&source_annot_clone, module_name)
                        {
                            Ok(loc) => this.module_def(loc),
                            Err(e) => Err(e),
                        },
                        id,
                    )?;
                }
            }
        }
        ast_visitor::import_declaration_default(self, loc, decl)
    }

    fn export_source(
        &mut self,
        source_annot: &'ast T,
        source: &'ast ast::StringLiteral<ALoc>,
    ) -> Result<(), Found> {
        if self.annot_covers_target(source_annot) {
            let module_name = source.value.as_str();
            let loc = self
                .callback
                .get_module_def_loc(source_annot, module_name)?;
            self.module_def(loc)?;
        }
        ast_visitor::export_source_default(self, source_annot, source)
    }

    fn member(
        &mut self,
        loc: &'ast T,
        expr: &'ast expression::Member<ALoc, T>,
    ) -> Result<(), Found> {
        let property = &expr.property;

        if let expression::member::Property::PropertyIdentifier(id) = property {
            let annot = &id.loc;
            if self.annot_covers_target(annot) {
                let obj_annot = expr.object.loc();
                let obj_type = self.type_from_enclosing_node(obj_annot)?;
                let obj_annot_typed = (self.callback.loc_of_annot(obj_annot), obj_type);
                let force_instance = ast_utils::is_super_member_access(expr);
                let result = GetDefRequest::Member(MemberInfo {
                    prop_name: id.name.dupe(),
                    object_type: obj_annot_typed,
                    force_instance,
                });
                self.request(result)?;
            }
        }
        ast_visitor::member_default(self, loc, expr)
    }

    fn match_member_pattern(
        &mut self,
        member_pattern: &'ast ast::match_pattern::MemberPattern<ALoc, T>,
    ) -> Result<(), Found> {
        let property = &member_pattern.property;

        if let ast::match_pattern::member_pattern::Property::PropertyIdentifier(id) = property {
            let annot = &id.loc;
            if self.annot_covers_target(annot) {
                let base_annot = match &member_pattern.base {
                    ast::match_pattern::member_pattern::Base::BaseIdentifier(id) => &id.loc,
                    ast::match_pattern::member_pattern::Base::BaseMember(member) => &member.loc,
                };
                let base_type = self.type_from_enclosing_node(base_annot)?;
                let base_annot_typed = (self.callback.loc_of_annot(base_annot), base_type);
                let result = GetDefRequest::Member(MemberInfo {
                    prop_name: id.name.dupe(),
                    object_type: base_annot_typed,
                    force_instance: false,
                });
                self.request(result)?;
            }
        }
        ast_visitor::match_member_pattern_default(self, member_pattern)
    }

    fn generic_type(&mut self, expr: &'ast ast::types::Generic<ALoc, T>) -> Result<(), Found> {
        let id = &expr.id;
        let targs = &expr.targs;

        if let ast::types::generic::Identifier::Unqualified(pick_id) = id {
            let pick_annot = &pick_id.loc;
            if pick_id.name.as_str() == "Pick" {
                if let Some(targs_inner) = targs {
                    if targs_inner.arguments.len() == 2
                        && !(self.is_local_use)(&self.callback.loc_of_annot(pick_annot))
                    {
                        let obj_annot = targs_inner.arguments[0].loc();
                        let keys = &targs_inner.arguments[1];

                        let do_request = |this: &mut Self,
                                          annot: &T,
                                          prop_name: FlowSmolStr|
                         -> Result<(), Found> {
                            if this.annot_covers_target(annot) {
                                let obj_type = this.type_from_enclosing_node(obj_annot)?;
                                let obj_annot_typed =
                                    (this.callback.loc_of_annot(obj_annot), obj_type);
                                let result = GetDefRequest::Member(MemberInfo {
                                    prop_name,
                                    object_type: obj_annot_typed,
                                    force_instance: false,
                                });
                                this.request(result)?;
                            }
                            Ok(())
                        };

                        match &**keys {
                            ast::types::TypeInner::StringLiteral { literal: sl, .. } => {
                                let annot = keys.loc();
                                do_request(self, annot, sl.value.dupe())?;
                            }
                            ast::types::TypeInner::Union { inner: union_, .. } => {
                                let (t1, t2, ts) = &union_.types;
                                let all_types: Vec<&ast::types::Type<ALoc, T>> =
                                    std::iter::once(t1)
                                        .chain(std::iter::once(t2))
                                        .chain(ts.iter())
                                        .collect();
                                for t in all_types {
                                    if let ast::types::TypeInner::StringLiteral {
                                        literal: sl,
                                        ..
                                    } = &**t
                                    {
                                        let annot = t.loc();
                                        do_request(self, annot, sl.value.dupe())?;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        ast_visitor::generic_type_default(self, expr)
    }

    fn indexed_access_type(
        &mut self,
        _loc: &'ast T,
        expr: &'ast ast::types::IndexedAccess<ALoc, T>,
    ) -> Result<(), Found> {
        let index = &expr.index;
        let object = &expr.object;

        if let ast::types::TypeInner::StringLiteral { literal: sl, .. } = &**index {
            let annot = index.loc();
            if self.annot_covers_target(annot) {
                let obj_annot = object.loc();
                let obj_type = self.type_from_enclosing_node(obj_annot)?;
                let obj_annot_typed = (self.callback.loc_of_annot(obj_annot), obj_type);
                let result = GetDefRequest::Member(MemberInfo {
                    prop_name: sl.value.dupe(),
                    object_type: obj_annot_typed,
                    force_instance: false,
                });
                self.request(result)?;
            }
        }
        ast_visitor::indexed_access_type_default(self, _loc, expr)
    }

    fn typeof_qualified_identifier(
        &mut self,
        qual: &'ast ast::types::typeof_::Qualified<ALoc, T>,
    ) -> Result<(), Found> {
        let id = &qual.id;
        if self.annot_covers_target(&id.loc) {
            let qualification_annot = qual.qualification.loc();
            let qualification_type = self.type_from_enclosing_node(qualification_annot)?;
            let result = GetDefRequest::Member(MemberInfo {
                prop_name: id.name.dupe(),
                object_type: (
                    self.callback.loc_of_annot(qualification_annot),
                    qualification_type,
                ),
                force_instance: false,
            });
            self.request(result)?;
        }
        ast_visitor::typeof_qualified_identifier_default(self, qual)
    }

    fn type_identifier(&mut self, id: &'ast ast::Identifier<ALoc, T>) -> Result<(), Found> {
        let loc = &id.loc;
        let name = &id.name;

        if self.annot_covers_target(loc) {
            let aloc = self.callback.loc_of_annot(loc);
            self.request(GetDefRequest::Identifier {
                name: name.dupe(),
                loc: aloc,
            })?;
        }
        ast_visitor::type_identifier_default(self, id)
    }

    fn identifier(&mut self, id: &'ast ast::Identifier<ALoc, T>) -> Result<(), Found> {
        let loc = &id.loc;
        let name = &id.name;

        if self.annot_covers_target(loc) {
            let aloc = self.callback.loc_of_annot(loc);
            self.request(GetDefRequest::Identifier {
                name: name.dupe(),
                loc: aloc,
            })?;
        }
        ast_visitor::identifier_default(self, id)
    }

    fn jsx_element(
        &mut self,
        expr_loc: &'ast T,
        expr: &'ast ast::jsx::Element<ALoc, T>,
    ) -> Result<(), Found> {
        let opening = &expr.opening_element;
        let attributes = &opening.attributes;

        for attr in attributes.iter() {
            if let ast::jsx::OpeningAttribute::Attribute(jsx_attr) = attr {
                if let ast::jsx::attribute::Name::Identifier(jsx_id) = &jsx_attr.name {
                    let annot = &jsx_id.loc;
                    if self.annot_covers_target(annot) {
                        let props = self.callback.props_of_jsx_element(expr_loc, expr)?;
                        self.request(GetDefRequest::Member(MemberInfo {
                            prop_name: jsx_id.name.dupe(),
                            object_type: props,
                            force_instance: false,
                        }))?;
                    }
                }
            }
        }
        ast_visitor::jsx_element_default(self, expr_loc, expr)
    }

    fn jsx_element_name_identifier(
        &mut self,
        id: &'ast ast::jsx::Identifier<ALoc, T>,
    ) -> Result<(), Found> {
        let annot = &id.loc;
        let name = &id.name;
        if self.annot_covers_target(annot) {
            let first_char = name.as_str().chars().next();
            if !first_char.is_some_and(|c| c.is_ascii_lowercase()) {
                let loc = self.callback.loc_of_annot(annot);
                self.request(GetDefRequest::Identifier {
                    name: name.dupe(),
                    loc,
                })?;
            } else {
                self.found_empty("jsx intrinsic element")?;
            }
        }
        ast_visitor::jsx_element_name_identifier_default(self, id)
    }

    fn jsx_element_name_namespaced(
        &mut self,
        ns: &'ast ast::jsx::NamespacedName<ALoc, T>,
    ) -> Result<(), Found> {
        let loc = &ns.loc;
        if (self.covers_target)(loc) {
            self.found_empty("jsx element (namespaced)")?;
        }
        ast_visitor::jsx_element_name_namespaced_default(self, ns)
    }

    fn jsx_member_expression_identifier(
        &mut self,
        id: &'ast ast::jsx::Identifier<ALoc, T>,
    ) -> Result<(), Found> {
        let annot = &id.loc;
        let name = &id.name;
        if self.annot_covers_target(annot) {
            let loc = self.callback.loc_of_annot(annot);
            self.request(GetDefRequest::Identifier {
                name: name.dupe(),
                loc,
            })?;
        }
        ast_visitor::jsx_member_expression_identifier_default(self, id)
    }

    fn jsx_member_expression(
        &mut self,
        expr: &'ast ast::jsx::MemberExpression<ALoc, T>,
    ) -> Result<(), Found> {
        let property_annot = &expr.property.loc;
        let prop_name = &expr.property.name;

        if self.annot_covers_target(property_annot) {
            let obj_annot = match &expr.object {
                ast::jsx::member_expression::Object::Identifier(id) => &id.loc,
                ast::jsx::member_expression::Object::MemberExpression(inner_me) => {
                    &inner_me.property.loc
                }
            };
            let obj_type = self.type_from_enclosing_node(obj_annot)?;
            let obj_annot_typed = (self.callback.loc_of_annot(obj_annot), obj_type);
            let result = GetDefRequest::Member(MemberInfo {
                prop_name: prop_name.dupe(),
                object_type: obj_annot_typed,
                force_instance: false,
            });
            self.request(result)?;
        }
        ast_visitor::jsx_member_expression_default(self, expr)
    }

    fn pattern(
        &mut self,
        kind: Option<ast::VariableKind>,
        pat: &'ast ast::pattern::Pattern<ALoc, T>,
    ) -> Result<(), Found> {
        let pat_annot = pat.loc();

        // In const {foo: bar} = require('some_module'); foo and bar should jump to prop def of foo,
        // while in other cases, bar should be its own definition.
        let is_id_pattern_of_obj_key_in_require_declarator =
            |pat: &ast::pattern::Pattern<ALoc, T>| -> bool {
                if let ast::pattern::Pattern::Identifier { .. } = pat {
                    self.require_declarator_info.is_some() && self.annot_covers_target(pat.loc())
                } else {
                    false
                }
            };

        match pat {
            ast::pattern::Pattern::Object { inner: obj, .. } => {
                for prop in obj.properties.iter() {
                    if let ast::pattern::object::Property::NormalProperty(prop_inner) = prop {
                        let key = &prop_inner.key;
                        let pattern = &prop_inner.pattern;
                        match key {
                            ast::pattern::object::Key::StringLiteral((loc, sl))
                                if (self.covers_target)(loc)
                                    || is_id_pattern_of_obj_key_in_require_declarator(pattern) =>
                            {
                                let pat_type = self.type_from_enclosing_node(pat_annot)?;
                                let pat_annot_typed =
                                    (self.callback.loc_of_annot(pat_annot), pat_type);
                                self.request(GetDefRequest::Member(MemberInfo {
                                    prop_name: sl.value.dupe(),
                                    object_type: pat_annot_typed,
                                    force_instance: false,
                                }))?;
                            }
                            ast::pattern::object::Key::Identifier(id)
                                if self.annot_covers_target(&id.loc)
                                    || is_id_pattern_of_obj_key_in_require_declarator(pattern) =>
                            {
                                let pat_type = self.type_from_enclosing_node(pat_annot)?;
                                let pat_annot_typed =
                                    (self.callback.loc_of_annot(pat_annot), pat_type);
                                self.request(GetDefRequest::Member(MemberInfo {
                                    prop_name: id.name.dupe(),
                                    object_type: pat_annot_typed,
                                    force_instance: false,
                                }))?;
                            }
                            _ => {}
                        }
                    }
                }
            }
            ast::pattern::Pattern::Identifier { inner: pat_id, .. } => {
                if let Some(info) = &self.require_declarator_info {
                    if info.toplevel_pattern_annot == *pat_annot {
                        let require_t = info.require_t.dupe();
                        self.module_def_for_entire_module_related_id(
                            |this| match this.type_from_enclosing_node(&require_t) {
                                Ok(t) => {
                                    this.found_loc = ProcessLocationResult::ModuleTypeDef(t);
                                    Err(Found::Found)
                                }
                                Err(e) => Err(e),
                            },
                            &pat_id.name,
                        )?;
                    }
                }
            }
            _ => {}
        }
        ast_visitor::pattern_default(self, kind, pat)
    }

    fn pattern_identifier(
        &mut self,
        kind: Option<ast::VariableKind>,
        id: &'ast ast::Identifier<ALoc, T>,
    ) -> Result<(), Found> {
        if kind.is_some() && self.annot_covers_target(&id.loc) {
            let loc = self.callback.loc_of_annot(&id.loc);
            self.own_named_def(loc, id.name.dupe())?;
        }
        ast_visitor::pattern_identifier_default(self, kind, id)
    }

    fn expression(&mut self, expr: &'ast expression::Expression<ALoc, T>) -> Result<(), Found> {
        self.enclosing_node_stack
            .push(EnclosingNode::EnclosingExpression(expr.dupe()));

        let result = self.expression_inner(expr);

        self.enclosing_node_stack.pop();
        result
    }

    fn type_(&mut self, type_: &'ast ast::types::Type<ALoc, T>) -> Result<(), Found> {
        let annot = type_.loc();
        let t = &**type_;
        if self.annot_covers_target(annot) {
            match t {
                ast::types::TypeInner::Any { .. }
                | ast::types::TypeInner::Mixed { .. }
                | ast::types::TypeInner::Empty { .. }
                | ast::types::TypeInner::Void { .. }
                | ast::types::TypeInner::Null { .. }
                | ast::types::TypeInner::Symbol { .. }
                | ast::types::TypeInner::Number { .. }
                | ast::types::TypeInner::NumberLiteral { .. }
                | ast::types::TypeInner::BigInt { .. }
                | ast::types::TypeInner::BigIntLiteral { .. }
                | ast::types::TypeInner::String { .. }
                | ast::types::TypeInner::StringLiteral { .. }
                | ast::types::TypeInner::Boolean { .. }
                | ast::types::TypeInner::BooleanLiteral { .. }
                | ast::types::TypeInner::Exists { .. }
                | ast::types::TypeInner::Unknown { .. }
                | ast::types::TypeInner::Never { .. }
                | ast::types::TypeInner::Undefined { .. }
                | ast::types::TypeInner::UniqueSymbol { .. } => {
                    self.found_empty("type literal")?;
                }
                ast::types::TypeInner::Nullable { .. }
                | ast::types::TypeInner::Array { .. }
                | ast::types::TypeInner::Conditional { .. }
                | ast::types::TypeInner::Infer { .. }
                | ast::types::TypeInner::Typeof { .. }
                | ast::types::TypeInner::Keyof { .. }
                | ast::types::TypeInner::ReadOnly { .. }
                | ast::types::TypeInner::Function { .. }
                | ast::types::TypeInner::Component { .. }
                | ast::types::TypeInner::Object { .. }
                | ast::types::TypeInner::Interface { .. }
                | ast::types::TypeInner::Generic { .. }
                | ast::types::TypeInner::IndexedAccess { .. }
                | ast::types::TypeInner::OptionalIndexedAccess { .. }
                | ast::types::TypeInner::Union { .. }
                | ast::types::TypeInner::Intersection { .. }
                | ast::types::TypeInner::Tuple { .. }
                | ast::types::TypeInner::Renders { .. }
                | ast::types::TypeInner::TemplateLiteral { .. }
                | ast::types::TypeInner::ConstructorType { .. } => {
                    return ast_visitor::type_default(self, type_);
                }
            }
        }
        // it is tempting to not recurse here, but comments are not included in
        // `annot`, so we have to dig into each child to visit their `comments`
        // fields.
        ast_visitor::type_default(self, type_)
    }

    fn binding_type_identifier(&mut self, id: &'ast ast::Identifier<ALoc, T>) -> Result<(), Found> {
        let annot = &id.loc;
        let loc = self.callback.loc_of_annot(annot);
        if (self.covers_target)(&loc) {
            self.own_named_def(loc, id.name.dupe())?;
        }
        Ok(())
    }

    fn module_ref_literal(&mut self, mref: &'ast ast::ModuleRefLiteral<ALoc>) -> Result<(), Found> {
        let require_loc = &mref.require_loc;
        let def_loc_opt = &mref.def_loc_opt;
        if (self.covers_target)(require_loc) {
            let loc = def_loc_opt.as_ref().unwrap_or(require_loc).dupe();
            self.own_unnamed_def(loc)?;
        }
        ast_visitor::module_ref_literal_default(self, mref)
    }

    fn enum_member_identifier(
        &mut self,
        id: &'ast ast::Identifier<ALoc, ALoc>,
    ) -> Result<(), Found> {
        let loc = &id.loc;
        if (self.covers_target)(loc) {
            self.own_named_def(loc.dupe(), id.name.dupe())?;
        }
        ast_visitor::enum_member_identifier_default(self, id)
    }

    // object keys would normally hit `t_identifier`; this circumvents that.
    fn object_key_identifier(&mut self, id: &'ast ast::Identifier<ALoc, T>) -> Result<(), Found> {
        let annot = &id.loc;
        if self.annot_covers_target(annot) {
            let loc = self.callback.loc_of_annot(annot);
            self.own_named_def(loc, id.name.dupe())?;
        }
        Ok(())
    }

    fn object_key(
        &mut self,
        key: &'ast ast::expression::object::Key<ALoc, T>,
    ) -> Result<(), Found> {
        match key {
            ast::expression::object::Key::StringLiteral((annot, _))
            | ast::expression::object::Key::NumberLiteral((annot, _))
            | ast::expression::object::Key::BigIntLiteral((annot, _))
                if self.annot_covers_target(annot) =>
            {
                self.found_empty("object key (literal)")?;
            }
            _ => {}
        }
        ast_visitor::object_key_default(self, key)
    }

    fn match_object_pattern_property_key(
        &mut self,
        key: &'ast ast::match_pattern::object_pattern::Key<ALoc, T>,
    ) -> Result<(), Found> {
        match key {
            ast::match_pattern::object_pattern::Key::StringLiteral((loc, _))
            | ast::match_pattern::object_pattern::Key::NumberLiteral((loc, _))
            | ast::match_pattern::object_pattern::Key::BigIntLiteral((loc, _)) => {
                if (self.covers_target)(loc) {
                    self.found_empty("match pattern object key (literal)")?;
                }
            }
            ast::match_pattern::object_pattern::Key::Identifier(id) => {
                let annot = &id.loc;
                if self.annot_covers_target(annot) {
                    let loc = self.callback.loc_of_annot(annot);
                    self.own_named_def(loc, id.name.dupe())?;
                }
            }
        }
        ast_visitor::match_object_pattern_property_key_default(self, key)
    }

    // for object properties using the shorthand {variableName} syntax,
    // process the value before the key so that the explicit-non-find in `object_key_identifier`
    // doesn't make us miss the variable
    fn object_property(
        &mut self,
        prop: &'ast expression::object::NormalProperty<ALoc, T>,
    ) -> Result<(), Found> {
        if let expression::object::NormalProperty::Init {
            shorthand: true,
            value,
            ..
        } = prop
        {
            self.expression(value)?;
        }
        ast_visitor::object_property_default(self, prop)
    }

    fn new(&mut self, _annot: &'ast T, expr: &'ast expression::New<ALoc, T>) -> Result<(), Found> {
        let callee = &expr.callee;
        if let expression::ExpressionInner::Identifier { loc: annot, .. } = callee.deref() {
            if self.annot_covers_target(annot) && self.purpose == Purpose::JSDoc {
                let t = self.type_from_enclosing_node(annot)?;
                let annot_typed = (self.callback.loc_of_annot(annot), t);
                self.request(GetDefRequest::Member(MemberInfo {
                    prop_name: "constructor".into(),
                    object_type: annot_typed,
                    // In `new Foo()`, the type of Foo is ThisClassT(InstanceT).
                    // We use force_instance to force the normalizer to inspect
                    // the InstanceT instead of the static properties of the class
                    force_instance: true,
                }))?;
            }
        }
        ast_visitor::new_default(self, _annot, expr)
    }

    fn comment(&mut self, c: &'ast ast::Comment<ALoc>) -> Result<(), Found> {
        let loc = &c.loc;
        if (self.covers_target)(loc) {
            self.found_empty("comment")?;
        }
        Ok(())
    }

    fn tagged_template(
        &mut self,
        _loc: &'ast T,
        expr: &'ast expression::TaggedTemplate<ALoc, T>,
    ) -> Result<(), Found> {
        let tag = &expr.tag;
        let quasi_loc = &expr.quasi.0;

        if let expression::ExpressionInner::Identifier { inner: tag_id, .. } = tag.deref() {
            if tag_id.name.as_str() == "graphql" && (self.covers_target)(quasi_loc) {
                let saved = self.in_graphql_tagged_literal;
                self.in_graphql_tagged_literal = true;
                let result = ast_visitor::tagged_template_default(self, _loc, expr);
                self.in_graphql_tagged_literal = saved;
                return result;
            }
        }
        ast_visitor::tagged_template_default(self, _loc, expr)
    }

    fn template_literal_element(
        &mut self,
        e: &'ast ast::expression::template_literal::Element<ALoc>,
    ) -> Result<(), Found> {
        let loc = &e.loc;
        if (self.covers_target)(loc) {
            let kind = if self.in_graphql_tagged_literal {
                "template(graphql)"
            } else {
                "template"
            };
            self.found_empty(kind)?;
        }
        Ok(())
    }

    fn jsx_attribute_value_literal(
        &mut self,
        annot: &'ast T,
        _lit: &'ast ast::StringLiteral<ALoc>,
    ) -> Result<(), Found> {
        if self.annot_covers_target(annot) {
            self.found_empty("jsx attribute literal")?;
        }
        Ok(())
    }

    fn jsx_attribute_name_namespaced(
        &mut self,
        name: &'ast ast::jsx::NamespacedName<ALoc, T>,
    ) -> Result<(), Found> {
        let loc = &name.loc;
        // TODO: this should be supported
        if (self.covers_target)(loc) {
            self.found_empty("jsx attribute (namespaced)")?;
        }
        ast_visitor::jsx_attribute_name_namespaced_default(self, name)
    }

    fn jsx_child(&mut self, child: &'ast ast::jsx::Child<ALoc, T>) -> Result<(), Found> {
        match child {
            ast::jsx::Child::Text { loc, .. } if self.annot_covers_target(loc) => {
                self.found_empty("jsx text")?;
            }
            _ => {}
        }
        ast_visitor::jsx_child_default(self, child)
    }

    fn class_body(&mut self, cls_body: &'ast ast::class::Body<ALoc, T>) -> Result<(), Found> {
        let body = &cls_body.body;

        let mut new_available_private_names = self.available_private_names.dupe();
        for member in body.iter() {
            match member {
                ast::class::BodyElement::Method(m) => {
                    if let expression::object::Key::PrivateName(pn) = &m.key {
                        new_available_private_names.insert(pn.name.dupe(), pn.loc.dupe());
                    }
                }
                ast::class::BodyElement::PrivateField(pf) => {
                    new_available_private_names.insert(pf.key.name.dupe(), pf.key.loc.dupe());
                }
                ast::class::BodyElement::Property(_) => {}
                ast::class::BodyElement::StaticBlock(_) => {}
                ast::class::BodyElement::DeclareMethod(_) => {}
                ast::class::BodyElement::AbstractMethod(_) => {}
                ast::class::BodyElement::AbstractProperty(_) => {}
                ast::class::BodyElement::IndexSignature(_) => {}
            }
        }
        let saved_available_private_names = std::mem::replace(
            &mut self.available_private_names,
            new_available_private_names,
        );
        let result = ast_visitor::class_body_default(self, cls_body);
        self.available_private_names = saved_available_private_names;
        result
    }

    fn private_name(&mut self, pn: &'ast ast::PrivateName<ALoc>) -> Result<(), Found> {
        let loc = &pn.loc;
        let name = &pn.name;
        if (self.covers_target)(loc) {
            match self.available_private_names.get(name) {
                None => self.found_empty("unbound private name")?,
                Some(l) => self.own_named_def(l.dupe(), name.dupe())?,
            }
        }
        Ok(())
    }

    // If shorthand syntax (e.g. `const foo`), don't map over the 'key' as its
    // location also covers the binding name.
    fn match_object_pattern_property(
        &mut self,
        prop: &'ast ast::match_pattern::object_pattern::Property<ALoc, T>,
    ) -> Result<(), Found> {
        match prop {
            ast::match_pattern::object_pattern::Property::Valid {
                loc: _,
                property:
                    ast::match_pattern::object_pattern::PropertyStruct {
                        key,
                        pattern,
                        shorthand,
                        comments,
                    },
            } => {
                if !shorthand {
                    self.match_object_pattern_property_key(key)?;
                }
                self.match_pattern(pattern)?;
                self.syntax_opt(comments.as_ref())?;
                Ok(())
            }
            ast::match_pattern::object_pattern::Property::InvalidShorthand { .. } => Ok(()),
        }
    }
}

impl<'a, 'ast, T: Dupe + PartialEq + 'ast, C: SearcherCallback<T>> Searcher<'a, T, C> {
    fn expression_inner(
        &mut self,
        expr: &'ast expression::Expression<ALoc, T>,
    ) -> Result<(), Found> {
        let annot = expr.loc();
        if self.annot_covers_target(annot) {
            match expr.deref() {
                expression::ExpressionInner::StringLiteral { .. } => {
                    self.found_empty("string")?;
                }
                expression::ExpressionInner::NumberLiteral { .. } => {
                    self.found_empty("number")?;
                }
                expression::ExpressionInner::BigIntLiteral { .. } => {
                    self.found_empty("bigint")?;
                }
                expression::ExpressionInner::BooleanLiteral { .. } => {
                    self.found_empty("boolean")?;
                }
                expression::ExpressionInner::NullLiteral { .. } => {
                    self.found_empty("null")?;
                }
                expression::ExpressionInner::RegExpLiteral { .. } => {
                    self.found_empty("regexp")?;
                }
                expression::ExpressionInner::Call { inner, .. } => {
                    if let expression::ExpressionInner::Identifier {
                        inner: callee_id, ..
                    } = inner.callee.deref()
                    {
                        if callee_id.name.as_str() == "require" {
                            if let [expression::ExpressionOrSpread::Expression(source_expr)] =
                                &*inner.arguments.arguments
                            {
                                let source_annot = source_expr.loc();
                                let is_string_or_template = matches!(
                                    source_expr.deref(),
                                    expression::ExpressionInner::StringLiteral { .. }
                                        | expression::ExpressionInner::TemplateLiteral { .. }
                                );
                                if is_string_or_template
                                    && self.is_legit_require_annot(source_annot)
                                {
                                    let t = self.type_from_enclosing_node(source_annot)?;
                                    self.found_loc = ProcessLocationResult::ModuleTypeDef(t);
                                    return Err(Found::Found);
                                }
                            }
                        }
                    }
                    return ast_visitor::expression_default(self, expr);
                }
                _ => {
                    return ast_visitor::expression_default(self, expr);
                }
            }
        }
        // it is tempting to not recurse here, but comments are not included in
        // `annot`, so we have to dig into each child to visit their `comments`
        // fields.
        ast_visitor::expression_default(self, expr)
    }
}

struct TypedAstSearcherCallback<'a, 'cx> {
    cx: &'a mut Context<'cx>,
}

impl<'a, 'cx> SearcherCallback<(ALoc, Type)> for TypedAstSearcherCallback<'a, 'cx> {
    fn loc_of_annot(&self, annot: &(ALoc, Type)) -> ALoc {
        annot.0.dupe()
    }

    fn annot_loc(annot: &(ALoc, Type)) -> &ALoc {
        &annot.0
    }

    fn remote_name_def_loc_of_import_named_specifier(
        &mut self,
        decl: &statement::import_declaration::NamedSpecifier<ALoc, (ALoc, Type)>,
        _enclosing_node: &EnclosingNode<ALoc, (ALoc, Type)>,
    ) -> Result<Option<ALoc>, Found> {
        Ok(decl.remote_name_def_loc.dupe())
    }

    fn remote_default_name_def_loc_of_import_declaration(
        &mut self,
        _loc: &ALoc,
        decl: &statement::ImportDeclaration<ALoc, (ALoc, Type)>,
    ) -> Result<Option<ALoc>, Found> {
        match &decl.default {
            None => Ok(None),
            Some(default_id) => Ok(default_id.remote_default_name_def_loc.dupe()),
        }
    }

    fn imported_name_def_loc_of_export_named_declaration_specifier(
        &mut self,
        spec: &statement::export_named_declaration::ExportSpecifier<ALoc, (ALoc, Type)>,
        _enclosing_node: &EnclosingNode<ALoc, (ALoc, Type)>,
    ) -> Result<Option<ALoc>, Found> {
        Ok(spec.imported_name_def_loc.dupe())
    }

    fn get_module_def_loc(
        &mut self,
        annot: &(ALoc, Type),
        module_name: &str,
    ) -> Result<ALoc, Found> {
        let (loc, _) = annot;
        let mref = flow_import_specifier::Userland::from_smol_str(FlowSmolStr::new(module_name));
        match flow_js_utils::import_export_utils::get_module_type_or_any(
            self.cx,
            false,
            None,
            loc.dupe(),
            mref,
        ) {
            Ok(Ok(m)) => Ok(def_loc_of_reason(&m.module_reason)),
            Ok(Err(t)) => Ok(def_loc_of_t(&t)),
            Err(_) => Err(Found::InternalError(InternalError::OnDemandTastError)),
        }
    }

    fn props_of_jsx_element(
        &mut self,
        _loc: &(ALoc, Type),
        expr: &ast::jsx::Element<ALoc, (ALoc, Type)>,
    ) -> Result<(ALoc, Type), Found> {
        Ok(expr.opening_element.props.dupe())
    }

    fn type_from_enclosing_node(
        &mut self,
        annot: &(ALoc, Type),
        _enclosing_node: &EnclosingNode<ALoc, (ALoc, Type)>,
    ) -> Result<Type, Found> {
        Ok(annot.1.dupe())
    }
}

fn find_remote_name_def_loc_in_node(
    loc: ALoc,
    node: &EnclosingNode<ALoc, (ALoc, Type)>,
) -> Option<Option<ALoc>> {
    enum FoundLoc {
        Found(Option<ALoc>),
    }

    struct ImportSearcher {
        target_loc: ALoc,
    }

    impl<'ast> AstVisitor<'ast, ALoc, (ALoc, Type), &'ast ALoc, FoundLoc> for ImportSearcher {
        fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
            loc
        }

        fn normalize_type(type_: &'ast (ALoc, Type)) -> &'ast ALoc {
            &type_.0
        }

        fn import_named_specifier(
            &mut self,
            _import_kind: ast::statement::ImportKind,
            spec: &'ast statement::import_declaration::NamedSpecifier<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundLoc> {
            let (loc_prime, _) = &spec.remote.loc;
            if *loc_prime == self.target_loc {
                return Err(FoundLoc::Found(spec.remote_name_def_loc.dupe()));
            }
            ast_visitor::import_named_specifier_default(self, _import_kind, spec)
        }
    }

    let mut searcher = ImportSearcher { target_loc: loc };
    let result = match node {
        EnclosingNode::EnclosingProgram(prog) => searcher.program(prog),
        EnclosingNode::EnclosingStatement(stmt) => searcher.statement(stmt),
        EnclosingNode::EnclosingExpression(expr) => searcher.expression(expr),
    };
    match result {
        Ok(()) => None,
        Err(FoundLoc::Found(t)) => Some(t),
    }
}

fn find_imported_name_def_loc_in_node(
    local_loc: ALoc,
    node: &EnclosingNode<ALoc, (ALoc, Type)>,
) -> Option<Option<ALoc>> {
    enum FoundLoc {
        Found(Option<ALoc>),
    }

    struct ExportSearcher {
        target_local_loc: ALoc,
    }

    impl<'ast> AstVisitor<'ast, ALoc, (ALoc, Type), &'ast ALoc, FoundLoc> for ExportSearcher {
        fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
            loc
        }

        fn normalize_type(type_: &'ast (ALoc, Type)) -> &'ast ALoc {
            &type_.0
        }

        fn export_named_declaration_specifier(
            &mut self,
            spec: &'ast statement::export_named_declaration::ExportSpecifier<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundLoc> {
            let (local_loc_prime, _) = &spec.local.loc;
            if self.target_local_loc == *local_loc_prime {
                return Err(FoundLoc::Found(spec.imported_name_def_loc.dupe()));
            }
            ast_visitor::export_named_declaration_specifier_default(self, spec)
        }
    }

    let mut searcher = ExportSearcher {
        target_local_loc: local_loc,
    };
    let result = match node {
        EnclosingNode::EnclosingProgram(prog) => searcher.program(prog),
        EnclosingNode::EnclosingStatement(stmt) => searcher.statement(stmt),
        EnclosingNode::EnclosingExpression(expr) => searcher.expression(expr),
    };
    match result {
        Ok(()) => None,
        Err(FoundLoc::Found(t)) => Some(t),
    }
}

struct OnDemandSearcherCallback<'a, 'cx> {
    cx: &'a mut Context<'cx>,
}

impl<'a, 'cx> SearcherCallback<ALoc> for OnDemandSearcherCallback<'a, 'cx> {
    fn loc_of_annot(&self, annot: &ALoc) -> ALoc {
        annot.dupe()
    }

    fn annot_loc(annot: &ALoc) -> &ALoc {
        annot
    }

    fn remote_name_def_loc_of_import_named_specifier(
        &mut self,
        decl: &statement::import_declaration::NamedSpecifier<ALoc, ALoc>,
        enclosing_node: &EnclosingNode<ALoc, ALoc>,
    ) -> Result<Option<ALoc>, Found> {
        let remote_loc = decl.remote.loc.dupe();
        let typed_node =
            typed_ast_finder::infer_node(self.cx, enclosing_node.clone()).map_err(|e| match e {
                flow_typing_utils::abnormal::CheckExprError::Canceled(_) => {
                    Found::InternalError(InternalError::Canceled)
                }
                flow_typing_utils::abnormal::CheckExprError::TimedOut(_) => {
                    Found::InternalError(InternalError::TimedOut)
                }
                flow_typing_utils::abnormal::CheckExprError::DebugThrow { .. } => {
                    Found::InternalError(InternalError::DebugThrow)
                }
                flow_typing_utils::abnormal::CheckExprError::Abnormal(_) => {
                    Found::InternalError(InternalError::OnDemandTastError)
                }
            })?;
        match find_remote_name_def_loc_in_node(remote_loc, &typed_node) {
            None => Err(Found::InternalError(InternalError::EnclosingNodeError)),
            Some(t) => Ok(t),
        }
    }

    fn remote_default_name_def_loc_of_import_declaration(
        &mut self,
        loc: &ALoc,
        decl: &statement::ImportDeclaration<ALoc, ALoc>,
    ) -> Result<Option<ALoc>, Found> {
        let stmt =
            ast::statement::Statement::new(ast::statement::StatementInner::ImportDeclaration {
                loc: loc.dupe(),
                inner: std::sync::Arc::new(decl.clone()),
            });
        let stmt_result = typing_statement::statement(self.cx, &stmt)
            .map_err(|_| Found::InternalError(InternalError::OnDemandTastError))?;
        // NOTE: remote_default_name_def_loc field gets updated during inference
        match &*stmt_result {
            ast::statement::StatementInner::ImportDeclaration {
                inner: import_decl, ..
            } => {
                if let Some(default_id) = &import_decl.default {
                    if let Some(l) = &default_id.remote_default_name_def_loc {
                        return Ok(Some(l.dupe()));
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn imported_name_def_loc_of_export_named_declaration_specifier(
        &mut self,
        spec: &statement::export_named_declaration::ExportSpecifier<ALoc, ALoc>,
        enclosing_node: &EnclosingNode<ALoc, ALoc>,
    ) -> Result<Option<ALoc>, Found> {
        let local_loc = spec.local.loc.dupe();
        let typed_node =
            typed_ast_finder::infer_node(self.cx, enclosing_node.clone()).map_err(|e| match e {
                flow_typing_utils::abnormal::CheckExprError::Canceled(_) => {
                    Found::InternalError(InternalError::Canceled)
                }
                flow_typing_utils::abnormal::CheckExprError::TimedOut(_) => {
                    Found::InternalError(InternalError::TimedOut)
                }
                flow_typing_utils::abnormal::CheckExprError::DebugThrow { .. } => {
                    Found::InternalError(InternalError::DebugThrow)
                }
                flow_typing_utils::abnormal::CheckExprError::Abnormal(_) => {
                    Found::InternalError(InternalError::OnDemandTastError)
                }
            })?;
        match find_imported_name_def_loc_in_node(local_loc, &typed_node) {
            None => Err(Found::InternalError(InternalError::EnclosingNodeError)),
            Some(t) => Ok(t),
        }
    }

    fn get_module_def_loc(&mut self, annot: &ALoc, module_name: &str) -> Result<ALoc, Found> {
        let mref = flow_import_specifier::Userland::from_smol_str(FlowSmolStr::new(module_name));
        match flow_js_utils::import_export_utils::get_module_type_or_any(
            self.cx,
            false,
            None,
            annot.dupe(),
            mref,
        ) {
            Ok(Ok(m)) => Ok(def_loc_of_reason(&m.module_reason)),
            Ok(Err(t)) => Ok(def_loc_of_t(&t)),
            Err(_) => Err(Found::InternalError(InternalError::OnDemandTastError)),
        }
    }

    fn props_of_jsx_element(
        &mut self,
        loc: &ALoc,
        expr: &ast::jsx::Element<ALoc, ALoc>,
    ) -> Result<(ALoc, Type), Found> {
        let jsx_expr = expression::Expression::new(expression::ExpressionInner::JSXElement {
            loc: loc.dupe(),
            inner: std::sync::Arc::new(expr.clone()),
        });
        let typed_expr = typing_statement::expression(None, None, None, self.cx, &jsx_expr)
            .map_err(|_| Found::InternalError(InternalError::OnDemandTastError))?;
        match typed_expr.deref() {
            expression::ExpressionInner::JSXElement { inner, .. } => {
                Ok(inner.opening_element.props.dupe())
            }
            _ => Err(Found::InternalError(InternalError::OnDemandTastError)),
        }
    }

    fn type_from_enclosing_node(
        &mut self,
        annot: &ALoc,
        enclosing_node: &EnclosingNode<ALoc, ALoc>,
    ) -> Result<Type, Found> {
        let typed_node =
            typed_ast_finder::infer_node(self.cx, enclosing_node.clone()).map_err(|e| match e {
                flow_typing_utils::abnormal::CheckExprError::Canceled(_) => {
                    Found::InternalError(InternalError::Canceled)
                }
                flow_typing_utils::abnormal::CheckExprError::TimedOut(_) => {
                    Found::InternalError(InternalError::TimedOut)
                }
                flow_typing_utils::abnormal::CheckExprError::DebugThrow { .. } => {
                    Found::InternalError(InternalError::DebugThrow)
                }
                flow_typing_utils::abnormal::CheckExprError::Abnormal(_) => {
                    Found::InternalError(InternalError::OnDemandTastError)
                }
            })?;
        match typed_ast_finder::find_type_annot_in_node(annot.dupe(), &typed_node) {
            None => Err(Found::InternalError(InternalError::EnclosingNodeError)),
            Some(t) => Ok(t),
        }
    }
}

fn search<T: Dupe + PartialEq, C: SearcherCallback<T>>(
    searcher: &mut Searcher<T, C>,
    ast: &ast::Program<ALoc, T>,
) -> ProcessLocationResult {
    match searcher.program(ast) {
        Err(Found::Found) => searcher.get_found_loc().clone(),
        Err(Found::InternalError(err)) => ProcessLocationResult::InternalError(err),
        Ok(()) => searcher.get_found_loc().clone(),
    }
}

pub fn process_location<'cx>(
    cx: &mut Context<'cx>,
    available_ast: &AvailableAst<'_>,
    is_local_use: &dyn Fn(&ALoc) -> bool,
    is_legit_require: &dyn Fn(&ALoc) -> bool,
    purpose: Purpose,
    loc: Loc,
) -> ProcessLocationResult {
    match available_ast {
        AvailableAst::TypedAst(typed_ast) => {
            let covers_target =
                |test_loc: &ALoc| -> bool { reason::in_range(&loc, test_loc.to_loc_exn()) };
            let mut callback = TypedAstSearcherCallback { cx };
            let mut searcher = Searcher::new(
                &mut callback,
                is_local_use,
                is_legit_require,
                &covers_target,
                purpose,
            );
            search(&mut searcher, typed_ast)
        }
        AvailableAst::ALocAst(aloc_ast) => {
            let covers_target =
                |test_loc: &ALoc| -> bool { reason::in_range(&loc, test_loc.to_loc_exn()) };
            let mut callback = OnDemandSearcherCallback { cx };
            let mut searcher = Searcher::new(
                &mut callback,
                is_local_use,
                is_legit_require,
                &covers_target,
                purpose,
            );
            search(&mut searcher, aloc_ast)
        }
    }
}
