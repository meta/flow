/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_parser::ast;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::loc::Loc;
use flow_typing_context::Context;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_statement::statement;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeParam;

pub fn mk_bound_t<'a>(cx: &mut Context<'a>, tparam: &TypeParam) -> Type {
    flow_js_utils::generic_of_tparam(cx, |_, x: &Type| x.dupe(), tparam)
}

#[derive(Clone)]
pub enum EnclosingNode<M: Dupe, T: Dupe> {
    EnclosingProgram(ast::Program<M, T>),
    EnclosingStatement(ast::statement::Statement<M, T>),
    EnclosingExpression(ast::expression::Expression<M, T>),
}

pub fn infer_node<'a>(
    cx: &mut Context<'a>,
    node: EnclosingNode<ALoc, ALoc>,
) -> Result<EnclosingNode<ALoc, (ALoc, Type)>, flow_typing_utils::abnormal::CheckExprError> {
    match node {
        EnclosingNode::EnclosingProgram(prog) => {
            let ast::Program {
                loc: prog_aloc,
                statements,
                interpreter,
                comments,
                all_comments,
            } = prog;
            let statements = statement::statement_list(cx, &statements)?;
            Ok(EnclosingNode::EnclosingProgram(ast::Program {
                loc: prog_aloc,
                statements: statements.into(),
                interpreter,
                comments,
                all_comments,
            }))
        }
        EnclosingNode::EnclosingStatement(stmt) => Ok(EnclosingNode::EnclosingStatement(
            statement::statement(cx, &stmt)?,
        )),
        EnclosingNode::EnclosingExpression(expr) => Ok(EnclosingNode::EnclosingExpression(
            statement::expression(None, None, None, cx, &expr)?,
        )),
    }
}

struct FindTypeAnnotVisitor {
    target_loc: ALoc,
}

impl flow_parser::polymorphic_ast_mapper::LocMapper<ALoc, (ALoc, Type), ALoc, (ALoc, Type), Type>
    for FindTypeAnnotVisitor
{
    fn on_loc_annot(&mut self, loc: &ALoc) -> Result<ALoc, Type> {
        Ok(loc.dupe())
    }

    fn on_type_annot(&mut self, annot: &(ALoc, Type)) -> Result<(ALoc, Type), Type> {
        let (loc, t) = annot;
        if *loc == self.target_loc {
            Err(t.dupe())
        } else {
            Ok(annot.dupe())
        }
    }
}

pub fn find_type_annot_in_node(
    loc: ALoc,
    node: &EnclosingNode<ALoc, (ALoc, Type)>,
) -> Option<Type> {
    let mut visitor = FindTypeAnnotVisitor { target_loc: loc };
    use flow_parser::polymorphic_ast_mapper;
    let result = match node {
        EnclosingNode::EnclosingProgram(prog) => {
            polymorphic_ast_mapper::program(&mut visitor, prog).map(|_| ())
        }
        EnclosingNode::EnclosingStatement(stmt) => {
            polymorphic_ast_mapper::statement(&mut visitor, stmt).map(|_| ())
        }
        EnclosingNode::EnclosingExpression(expr) => {
            polymorphic_ast_mapper::expression(&mut visitor, expr).map(|_| ())
        }
    };
    result.err()
}

/// Find exact location match
pub mod exact_match_query {
    use flow_parser::ast_visitor;
    use flow_parser::ast_visitor::AstVisitor;
    use flow_parser::polymorphic_ast_mapper;
    use flow_parser::polymorphic_ast_mapper::LocMapper;

    use super::*;

    enum FoundFilteredOut {
        Found(Type),
    }

    struct FilteredOutSearcher {
        target_loc: ALoc,
    }

    impl<'ast> AstVisitor<'ast, ALoc, (ALoc, Type), (), FoundFilteredOut> for FilteredOutSearcher {
        fn normalize_loc(_loc: &'ast ALoc) {}

        fn normalize_type(_type_: &'ast (ALoc, Type)) {}

        fn optional_call(
            &mut self,
            _loc: &'ast (ALoc, Type),
            expr: &'ast ast::expression::OptionalCall<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundFilteredOut> {
            let (filtered_out_loc, filtered_out_t) = &expr.filtered_out;
            if *filtered_out_loc == self.target_loc {
                return Err(FoundFilteredOut::Found(filtered_out_t.dupe()));
            }
            ast_visitor::optional_call_default(self, _loc, expr)
        }

        fn optional_member(
            &mut self,
            _loc: &'ast (ALoc, Type),
            expr: &'ast ast::expression::OptionalMember<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundFilteredOut> {
            let (filtered_out_loc, filtered_out_t) = &expr.filtered_out;
            if *filtered_out_loc == self.target_loc {
                return Err(FoundFilteredOut::Found(filtered_out_t.dupe()));
            }
            ast_visitor::optional_member_default(self, _loc, expr)
        }
    }

    struct ExactMatchSearcher {
        target_loc: ALoc,
    }

    impl LocMapper<ALoc, (ALoc, Type), ALoc, (ALoc, Type), Type> for ExactMatchSearcher {
        fn on_loc_annot(&mut self, loc: &ALoc) -> Result<ALoc, Type> {
            Ok(loc.dupe())
        }

        fn on_type_annot(&mut self, annot: &(ALoc, Type)) -> Result<(ALoc, Type), Type> {
            let (loc, t) = annot;
            if *loc == self.target_loc {
                Err(t.dupe())
            } else {
                Ok(annot.dupe())
            }
        }
    }

    pub fn find(typed_ast: &ast::Program<ALoc, (ALoc, Type)>, aloc: ALoc) -> Option<Type> {
        let mut filtered_out_searcher = FilteredOutSearcher {
            target_loc: aloc.dupe(),
        };
        if let Err(FoundFilteredOut::Found(t)) = filtered_out_searcher.program(typed_ast) {
            return Some(t);
        }

        let mut searcher = ExactMatchSearcher { target_loc: aloc };
        polymorphic_ast_mapper::program(&mut searcher, typed_ast).err()
    }
}

pub fn find_exact_match_annotation(
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    aloc: ALoc,
) -> Option<Type> {
    exact_match_query::find(typed_ast, aloc)
}

/// Find the inferred props of the JSX opening at an exact location.
pub fn find_jsx_props(typed_ast: &ast::Program<ALoc, (ALoc, Type)>, aloc: ALoc) -> Option<Type> {
    struct JsxPropsFinder {
        target_loc: ALoc,
    }

    impl<'ast> AstVisitor<'ast, ALoc, (ALoc, Type), (), Type> for JsxPropsFinder {
        fn normalize_loc(_loc: &'ast ALoc) {}

        fn normalize_type(_type_: &'ast (ALoc, Type)) {}

        fn jsx_opening_element(
            &mut self,
            opening: &'ast ast::jsx::Opening<ALoc, (ALoc, Type)>,
        ) -> Result<(), Type> {
            if opening.loc == self.target_loc {
                return Err(opening.props.1.dupe());
            }
            ast_visitor::jsx_opening_element_default(self, opening)
        }
    }

    JsxPropsFinder { target_loc: aloc }.program(typed_ast).err()
}

/// Find identifier under location
pub mod type_at_pos {
    use std::ops::Deref;
    use std::rc::Rc;

    use dupe::Dupe;
    use flow_aloc::ALoc;
    use flow_common::reason;
    use flow_common::reason::Name;
    use flow_common::reason::VirtualReasonDesc;
    use flow_common::subst_name::SubstName;
    use flow_common_ty::ty::Alias;
    use flow_common_ty::ty::AliasKind;
    use flow_common_ty::ty::Binder;
    use flow_common_ty::ty::BinderKind;
    use flow_common_ty::ty::TypeParameterContext;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_parser::ast;
    use flow_parser::ast::types::TypeInner;
    use flow_parser::ast_visitor;
    use flow_parser::ast_visitor::AstVisitor;
    use flow_parser::ast_visitor::TypeParamsContext;
    use flow_parser::loc::Loc;
    use flow_parser::loc::Position;
    use flow_typing_context::Context;
    use flow_typing_type::type_::AnySource;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::LazyHintT;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner as ConstraintTypeInner;
    use flow_typing_type::type_::TypeParam;
    use flow_typing_type::type_::TypeParamInner;
    use flow_typing_type::type_util;
    use flow_typing_utils::type_env;
    use flow_typing_utils::type_hint;
    use flow_typing_utils::typed_ast_utils;

    use super::mk_bound_t;

    /// How the declaration to frame a hover as is determined. A binding site is
    /// resolved by the finder, which is walking the declaration that introduced the
    /// name; a reference carries no such context, so the finder only records what
    /// kind of reference it is and leaves the lookup to the caller.
    pub enum Framing {
        /// The target is a binding's own name.
        Binder {
            binder: Binder,
            type_parameter_context: Option<Rc<TypeParameterContext<Type>>>,
        },
        /// The target is a bare identifier; resolve it to the binding it references.
        IdentifierRef,
        /// The target is the remote name in `import {a as b}`. Its definition is
        /// in another file, so the caller resolves the declaration kind there.
        RemoteIdentifierRef {
            def_loc: Option<ALoc>,
            name: FlowSmolStr,
        },
        /// The target is a type name. A type declaration prints a head of its own,
        /// so the binding is resolved only far enough to say whether it is imported.
        TypeIdentifierRef {
            type_parameter_context: Option<Rc<TypeParameterContext<Type>>>,
        },
        /// The target is the `b` of `export {a as b}`, which binds nothing of its
        /// own. Resolve `a` instead, and print its declaration under the name this
        /// module exports it as.
        RenamedExportRef { local: ALoc, name: FlowSmolStr },
        /// The target is the `p` of a `o.p` access; resolve `p` in the type of `o`.
        MemberRef {
            name: FlowSmolStr,
            object_type: Type,
        },
    }

    pub enum TypeAtPosResult {
        TypeResult {
            loc: Loc,
            is_type_identifier_reference: bool,
            type_: Type,
            framing: Option<Framing>,
            alias: Option<Alias>,
        },
        HardcodedModuleResult(Loc, FlowSmolStr),
        NoResult,
    }

    enum FoundResult {
        FoundType {
            loc: ALoc,
            is_type_identifier_reference: bool,
            type_: Type,
            framing: Option<Box<Framing>>,
            alias: Option<Alias>,
        },
        FoundHardcodedModule(ALoc, FlowSmolStr),
        Canceled(flow_utils_concurrency::job_error::JobError),
    }

    impl From<flow_utils_concurrency::job_error::JobError> for FoundResult {
        fn from(e: flow_utils_concurrency::job_error::JobError) -> Self {
            FoundResult::Canceled(e)
        }
    }

    // Kinds of nodes that "type-at-pos" is interested in:
    // - identifiers              (handled in t_identifier)
    // - type parameters          (handled in type_param_identifier)
    // - literal object keys      (handled in object_key)
    // - `this`, `super`          (handled in expression)
    // - private property names   (handled in expression)
    type TypeParameterDeclaration<'a> = &'a ast::Identifier<ALoc, (ALoc, Type)>;

    struct TypeAtPosSearcher<'a, 'cx> {
        cx: &'a mut Context<'cx>,
        target_loc: Loc,
        /// Type parameters in scope at the current visit position, each paired
        /// with the declaration that introduced it. Entries live only for the
        /// duration of their declaration's visit (see
        /// `in_type_parameter_scope`), so a lookup by definition location
        /// always finds the innermost owner and never a sibling that has
        /// already been exited.
        ///
        /// Anonymous declarations record nothing: there is no name to
        /// associate their parameters with. The synthetic `this` parameter
        /// of classes and records is likewise not recorded.
        enclosing_tparams: Vec<(TypeParam, TypeParameterDeclaration<'a>)>,
        /// Name of the class, declared class, record or interface whose own body
        /// is being visited, so that a member binder can qualify itself as `A.m`.
        enclosing_nominal: Option<FlowSmolStr>,
        /// Type of the object a destructuring pattern matches against, so that
        /// a property key frames like a member reference. Restored on exit,
        /// since patterns nest.
        enclosing_pattern_scrutinee: Option<Type>,
        /// Name and object type of the enum whose members are being visited,
        /// so that a member site frames like a member reference. Enum members
        /// carry no types of their own.
        enclosing_enum: Option<(FlowSmolStr, Type)>,
        /// Names bound by `infer` and in scope at the current position. The scope
        /// builder records these as ordinary type bindings, indistinguishable from
        /// a type alias, so the syntax that bound them is the only thing that says
        /// a reference to one is a reference to a type parameter.
        infer_tparams: Vec<FlowSmolStr>,
        /// The private members the enclosing class declares, with the `#`. A
        /// `this.#m` access carries no link back to the declaration that says
        /// whether `#m` is a field or a method, and private names are visible only
        /// inside the class body that declares them, so the body is scanned up
        /// front.
        enclosing_private_members: Vec<(FlowSmolStr, BinderKind)>,
    }

    impl<'a, 'cx> TypeAtPosSearcher<'a, 'cx> {
        fn covers_target(&self, loc: &ALoc) -> bool {
            reason::in_range(&self.target_loc, loc.to_loc_exn())
        }

        fn covers_target_loc(&self, loc: &Loc) -> bool {
            reason::in_range(&self.target_loc, loc)
        }

        /// A module specifier (`'./mod'` in an `import` or `export ... from`)
        /// names its module, like `declare module` does.
        fn find_module_specifier(
            &self,
            source: &((ALoc, Type), ast::StringLiteral<ALoc>),
        ) -> Result<(), FoundResult> {
            let ((loc, _), lit) = source;
            if self.covers_target(loc) {
                return Err(FoundResult::FoundHardcodedModule(
                    loc.dupe(),
                    lit.value.dupe(),
                ));
            }
            Ok(())
        }

        fn type_parameter_context(&self, t: &Type) -> Option<Rc<TypeParameterContext<Type>>> {
            let def_loc = type_util::def_loc_of_t(t);
            self.enclosing_tparams
                .iter()
                .rev()
                .find(|(tparam, _)| tparam.reason.loc() == def_loc)
                .map(|(_, declaration)| *declaration)
                .map(|id| {
                    let (_, declaration) = &id.loc;
                    Rc::new(TypeParameterContext {
                        name: id.name.dupe(),
                        declaration: declaration.dupe(),
                    })
                })
        }

        fn find_loc(
            &self,
            loc: &ALoc,
            t: &Type,
            is_type_identifier: bool,
            framing: Option<Framing>,
        ) -> Result<(), FoundResult> {
            Err(FoundResult::FoundType {
                loc: loc.dupe(),
                is_type_identifier_reference: is_type_identifier,
                type_: t.dupe(),
                framing: framing.map(Box::new),
                alias: None,
            })
        }

        /// The target is the name an `import` or `export` statement binds. The
        /// binding is still resolved, via `framing`, so exporting a local `const`
        /// keeps its `const x: T` head under the `(alias)` one.
        fn find_alias(
            &self,
            loc: &ALoc,
            t: &Type,
            kind: AliasKind,
            name: &FlowSmolStr,
            framing: Framing,
        ) -> Result<(), FoundResult> {
            Err(FoundResult::FoundType {
                loc: loc.dupe(),
                is_type_identifier_reference: false,
                type_: t.dupe(),
                framing: Some(Box::new(framing)),
                alias: Some(Alias {
                    kind,
                    name: name.dupe(),
                    import: None,
                }),
            })
        }

        /// Like `find_loc`, but records that the target is the declaration site of
        /// `binder` rather than a reference to it.
        fn find_binder(
            &self,
            loc: &ALoc,
            t: &Type,
            kind: BinderKind,
            name: &FlowSmolStr,
        ) -> Result<(), FoundResult> {
            let owner = match kind {
                BinderKind::Method
                | BinderKind::Getter
                | BinderKind::Setter
                | BinderKind::Property => self.enclosing_nominal.dupe(),
                _ => None,
            };
            self.find_binder_with_owner(loc, t, kind, name, owner)
        }

        /// Like `find_binder`, but the nominal owner is given explicitly.
        /// Use this when the binder's owner is not the lexically enclosing
        /// class/interface/record, e.g. a component's (anonymous) props
        /// object for a JSX attribute name that happens to sit inside a
        /// class body.
        fn find_binder_with_owner(
            &self,
            loc: &ALoc,
            t: &Type,
            kind: BinderKind,
            name: &FlowSmolStr,
            owner: Option<FlowSmolStr>,
        ) -> Result<(), FoundResult> {
            Err(FoundResult::FoundType {
                loc: loc.dupe(),
                is_type_identifier_reference: false,
                type_: t.dupe(),
                framing: Some(Box::new(Framing::Binder {
                    binder: Binder {
                        kind,
                        name: name.dupe(),
                        owner,
                        type_parameter_context: None,
                        // A declaration site prints the whole declared type, so an
                        // overload set is already visible there and needs no count.
                        overloads: 0,
                        targs: None,
                    },
                    type_parameter_context: (kind == BinderKind::TypeParameter)
                        .then(|| self.type_parameter_context(t))
                        .flatten(),
                })),
                alias: None,
            })
        }

        /// Records a declaration's type parameters while `f` runs, then drops
        /// them, so parameters of already-exited sibling declarations do not
        /// linger for the rest of the traversal.
        fn in_type_parameter_scope<R>(
            &mut self,
            id: Option<TypeParameterDeclaration<'a>>,
            tparams: Option<&'a ast::types::TypeParams<ALoc, (ALoc, Type)>>,
            f: impl FnOnce(&mut Self) -> R,
        ) -> R {
            let saved_len = self.enclosing_tparams.len();
            if let (Some(id), Some(tparams)) = (id, tparams) {
                self.enclosing_tparams.extend(
                    tparams
                        .params
                        .iter()
                        .map(|tparam| (Self::make_typeparam(tparam), id)),
                );
            }
            let res = f(self);
            self.enclosing_tparams.truncate(saved_len);
            res
        }

        /// A `this.#m` access. Private members are not properties of the receiver's
        /// type, so the usual member lookup cannot reach them; the enclosing class
        /// body is the only place that says whether `#m` is a field or a method.
        fn find_private_member_reference(
            &self,
            loc: &ALoc,
            t: &Type,
            pn: &ast::PrivateName<ALoc>,
        ) -> Result<(), FoundResult> {
            let name = Self::private_member_name(pn);
            let kind = self
                .enclosing_private_members
                .iter()
                .find(|(member, _)| *member == name)
                .map(|(_, kind)| *kind)
                .unwrap_or(BinderKind::Property);
            self.find_binder(loc, t, kind, &name)
        }

        /// The name and type of a property key, when the target is on the key
        /// itself. Literal keys are named by their source text
        /// (`(property) 'str-key': number`). Computed and private keys have no name
        /// to print.
        fn key_binder<'ast>(
            &self,
            key: &'ast ast::expression::object::Key<ALoc, (ALoc, Type)>,
        ) -> Option<(&'ast ALoc, &'ast Type, FlowSmolStr)> {
            use ast::expression::object::Key;
            let (loc, t, name) = match key {
                Key::Identifier(id) => {
                    let (loc, t) = &id.loc;
                    (loc, t, id.name.dupe())
                }
                Key::StringLiteral(((loc, t), lit)) => (loc, t, lit.raw.dupe()),
                Key::NumberLiteral(((loc, t), lit)) => (loc, t, lit.raw.dupe()),
                Key::BigIntLiteral(((loc, t), lit)) => (loc, t, lit.raw.dupe()),
                Key::PrivateName(_) | Key::Computed(_) => return None,
            };
            if self.covers_target(loc) {
                Some((loc, t, name))
            } else {
                None
            }
        }

        /// The binder for a `Pattern`, when it binds a single name. Destructuring
        /// patterns bind through nested `pattern_identifier`s that this does not
        /// reach; those reach `identifier` instead and are framed by resolving the
        /// name to its binding.
        fn simple_pattern_binder<'ast>(
            pattern: &'ast ast::pattern::Pattern<ALoc, (ALoc, Type)>,
        ) -> Option<&'ast ast::Identifier<ALoc, (ALoc, Type)>> {
            match pattern {
                ast::pattern::Pattern::Identifier { inner, .. } => Some(&inner.name),
                _ => None,
            }
        }

        fn binder_kind_of_variable_kind(kind: ast::VariableKind) -> BinderKind {
            match kind {
                ast::VariableKind::Var => BinderKind::Var,
                ast::VariableKind::Let => BinderKind::Let,
                ast::VariableKind::Const => BinderKind::Const,
            }
        }

        /// The name a declarator binds, shared by the `declare` form and the
        /// ordinary one.
        fn variable_binder(
            &self,
            kind: ast::VariableKind,
            declarator: &ast::statement::variable::Declarator<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            if let Some(id) = Self::simple_pattern_binder(&declarator.id)
                && let (loc, t) = &id.loc
                && self.covers_target(loc)
            {
                return self.find_binder(
                    loc,
                    t,
                    Self::binder_kind_of_variable_kind(kind),
                    &id.name,
                );
            }
            Ok(())
        }

        /// The name a function declaration binds. A function expression has no
        /// binding of its own, so its name is left to `identifier`.
        fn function_binder(
            &self,
            id: Option<&ast::Identifier<ALoc, (ALoc, Type)>>,
        ) -> Result<(), FoundResult> {
            let Some(id) = id else { return Ok(()) };
            let (loc, t) = &id.loc;
            if self.covers_target(loc) {
                return self.find_binder(loc, t, BinderKind::Function, &id.name);
            }
            Ok(())
        }

        /// Visits `f` with `enclosing_nominal` set, restoring it afterwards.
        fn in_nominal<R>(
            &mut self,
            name: Option<FlowSmolStr>,
            f: impl FnOnce(&mut Self) -> R,
        ) -> R {
            let outer = std::mem::replace(&mut self.enclosing_nominal, name);
            let res = f(self);
            self.enclosing_nominal = outer;
            res
        }

        /// Visits `f` with the private members declared by `body` in scope,
        /// restoring the outer scope afterwards. A private name is visible
        /// only inside the body that declares it, so a nested class shadows
        /// rather than extends the outer members.
        fn in_class<R>(
            &mut self,
            name: Option<FlowSmolStr>,
            body: &ast::class::Body<ALoc, (ALoc, Type)>,
            f: impl FnOnce(&mut Self) -> R,
        ) -> R {
            let outer_private = std::mem::replace(
                &mut self.enclosing_private_members,
                Self::private_members_of_class_body(body),
            );
            let res = self.in_nominal(name, f);
            self.enclosing_private_members = outer_private;
            res
        }

        /// The name hover reports a private member under. The parser drops the `#`
        /// from the name it records, but every rendering of the member keeps it.
        fn private_member_name(pn: &ast::PrivateName<ALoc>) -> FlowSmolStr {
            FlowSmolStr::new(format!("#{}", pn.name))
        }

        /// The private members declared directly in a class body, so that a
        /// `this.#m` access can say whether it is reaching a field or a method.
        fn private_members_of_class_body(
            body: &ast::class::Body<ALoc, (ALoc, Type)>,
        ) -> Vec<(FlowSmolStr, BinderKind)> {
            use ast::class::BodyElement;
            body.body
                .iter()
                .filter_map(|elem| match elem {
                    BodyElement::PrivateField(pf) => {
                        Some((Self::private_member_name(&pf.key), BinderKind::Property))
                    }
                    BodyElement::Method(method) => {
                        let ast::expression::object::Key::PrivateName(pn) = &method.key else {
                            return None;
                        };
                        Some((
                            Self::private_member_name(pn),
                            Self::binder_kind_of_method(method)?,
                        ))
                    }
                    _ => None,
                })
                .collect()
        }

        /// A constructor is framed as its class's own declaration, under the
        /// class's name rather than the `constructor` keyword.
        fn binder_kind_of_method(
            method: &ast::class::Method<ALoc, (ALoc, Type)>,
        ) -> Option<BinderKind> {
            use ast::class::MethodKind;
            match method.kind {
                MethodKind::Method => Some(BinderKind::Method),
                MethodKind::Get => Some(BinderKind::Getter),
                MethodKind::Set => Some(BinderKind::Setter),
                MethodKind::Constructor => Some(BinderKind::Constructor),
            }
        }

        fn make_typeparam(tparam: &ast::types::TypeParam<ALoc, (ALoc, Type)>) -> TypeParam {
            let (name_loc, _) = &tparam.name.loc;
            let name = &tparam.name.name;
            let reason =
                reason::mk_annot_reason(VirtualReasonDesc::RType(name.dupe()), name_loc.dupe());
            let bound = match &tparam.bound {
                ast::types::AnnotationOrHint::Missing((_, t)) => t.dupe(),
                ast::types::AnnotationOrHint::Available(annotation) => {
                    let (_, t) = annotation.annotation.loc();
                    t.dupe()
                }
            };
            let default = tparam.default.as_ref().map(|ty| {
                let (_, t) = ty.loc();
                t.dupe()
            });
            TypeParam::new(TypeParamInner {
                reason,
                name: SubstName::name(name.dupe()),
                bound,
                polarity: tparam
                    .variance
                    .as_ref()
                    .map(|variance| typed_ast_utils::polarity(Some(variance))),
                default,
                is_this: false,
                is_const: tparam.const_.is_some(),
            })
        }
    }

    /// Walks the members of a body that has a name to qualify them with — an
    /// interface's or a declared class's. Routing them through `object_type`
    /// instead would clear the qualifier, since a bare object type is anonymous.
    fn named_object_body<'ast>(
        searcher: &mut TypeAtPosSearcher<'ast, '_>,
        name: FlowSmolStr,
        obj_type: &'ast ast::types::Object<ALoc, (ALoc, Type)>,
    ) -> Result<(), FoundResult> {
        searcher.in_nominal(Some(name), |this| {
            for property in obj_type.properties.iter() {
                this.object_type_property(property)?;
            }
            this.syntax_opt(obj_type.comments.as_ref())
        })
    }

    /// `declare_class_default`'s walk, with the body's members qualified by the
    /// class name. Keep the visit order in step with the parser's.
    fn declare_class_members<'ast>(
        searcher: &mut TypeAtPosSearcher<'ast, '_>,
        decl: &'ast ast::statement::DeclareClass<ALoc, (ALoc, Type)>,
    ) -> Result<(), FoundResult> {
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
        searcher.class_identifier(id)?;
        if let Some(tparams) = tparams {
            searcher.type_params(&TypeParamsContext::DeclareClass, tparams)?;
        }
        let (_loc, obj_type) = body;
        named_object_body(searcher, id.name.dupe(), obj_type)?;
        if let Some((_loc, ext)) = extends {
            declare_class_extends(searcher, ext)?;
        }
        for (_loc, generic) in mixins.iter() {
            searcher.generic_type(generic)?;
        }
        if let Some(implements) = implements {
            searcher.class_implements(implements)?;
        }
        searcher.syntax_opt(comments.as_ref())?;
        Ok(())
    }

    /// The `extends` clause of a declared class, which nests through
    /// `extends mixin(Base)` calls. The parser's own walk of this is private.
    fn declare_class_extends<'ast>(
        searcher: &mut TypeAtPosSearcher<'ast, '_>,
        ext: &'ast ast::statement::DeclareClassExtends<ALoc, (ALoc, Type)>,
    ) -> Result<(), FoundResult> {
        match ext {
            ast::statement::DeclareClassExtends::ExtendsIdent(generic) => {
                searcher.generic_type(generic)?;
            }
            ast::statement::DeclareClassExtends::ExtendsCall {
                callee: (_callee_loc, callee),
                arg,
            } => {
                searcher.generic_type(callee)?;
                declare_class_extends(searcher, &arg.1)?;
            }
        }
        Ok(())
    }

    impl<'ast, 'cx> AstVisitor<'ast, ALoc, (ALoc, Type), &'ast ALoc, FoundResult>
        for TypeAtPosSearcher<'ast, 'cx>
    {
        fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
            loc
        }

        fn normalize_type(type_: &'ast (ALoc, Type)) -> &'ast ALoc {
            &type_.0
        }

        fn identifier(
            &mut self,
            id: &'ast ast::Identifier<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            let (loc, t) = &id.loc;
            if self.covers_target(loc) {
                self.find_loc(loc, t, false, Some(Framing::IdentifierRef))
            } else {
                ast_visitor::identifier_default(self, id)
            }
        }

        /// Intercepts `o.p` before the walk reaches `p`'s own identifier, which
        /// carries no link back to `o`. Optional chains route through here too:
        /// `optional_member_default` delegates to this hook.
        fn member(
            &mut self,
            loc: &'ast (ALoc, Type),
            expr: &'ast ast::expression::Member<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            use ast::expression::member::Property;
            if let Property::PropertyIdentifier(id) = &expr.property
                && let (id_loc, t) = &id.loc
                && self.covers_target(id_loc)
            {
                let (_, object_type) = expr.object.loc();
                return self.find_loc(
                    id_loc,
                    t,
                    false,
                    Some(Framing::MemberRef {
                        name: id.name.dupe(),
                        object_type: object_type.dupe(),
                    }),
                );
            }
            ast_visitor::member_default(self, loc, expr)
        }

        fn type_identifier_reference(
            &mut self,
            id: &'ast ast::Identifier<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            let (loc, t) = &id.loc;
            if self.covers_target(loc) {
                if self.infer_tparams.contains(&id.name) {
                    return self.find_binder(loc, t, BinderKind::TypeParameter, &id.name);
                }
                self.find_loc(
                    loc,
                    t,
                    true,
                    Some(Framing::TypeIdentifierRef {
                        type_parameter_context: self.type_parameter_context(t),
                    }),
                )
            } else {
                ast_visitor::identifier_default(self, id)
            }
        }

        fn jsx_identifier(
            &mut self,
            ident: &'ast ast::jsx::Identifier<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            let (loc, t) = &ident.loc;
            if self.covers_target(loc) {
                self.find_loc(loc, t, false, None)
            } else {
                ast_visitor::jsx_identifier_default(self, ident)
            }
        }

        /// `<Comp />`, `</Comp>`, and the receiver of `<NS.Sub />`, all of which
        /// name a value in scope. An intrinsic (`<div />`) is a name too, but not a
        /// bound one, so the binding lookup finds nothing and it prints as before.
        fn jsx_element_name_identifier(
            &mut self,
            ident: &'ast ast::jsx::Identifier<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            let (loc, t) = &ident.loc;
            if self.covers_target(loc) {
                self.find_loc(loc, t, false, Some(Framing::IdentifierRef))
            } else {
                ast_visitor::jsx_element_name_identifier_default(self, ident)
            }
        }

        /// The `Sub` of `<NS.Sub />`, which the walk would otherwise reach as a bare
        /// identifier with no link back to `NS`. A deeper receiver (`<A.B.C />`) is
        /// left alone: only an identifier object carries a type to resolve `C` in.
        fn jsx_member_expression(
            &mut self,
            member: &'ast ast::jsx::MemberExpression<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            let (loc, t) = &member.property.loc;
            if self.covers_target(loc)
                && let ast::jsx::member_expression::Object::Identifier(object) = &member.object
            {
                let (_, object_type) = &object.loc;
                return self.find_loc(
                    loc,
                    t,
                    false,
                    Some(Framing::MemberRef {
                        name: member.property.name.dupe(),
                        object_type: object_type.dupe(),
                    }),
                );
            }
            ast_visitor::jsx_member_expression_default(self, member)
        }

        fn type_param(
            &mut self,
            kind: &TypeParamsContext,
            tparam: &'ast ast::types::TypeParam<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            let (loc, _) = &tparam.name.loc;
            if self.covers_target(loc) {
                let tp = Self::make_typeparam(tparam);
                let t = mk_bound_t(self.cx, &tp);
                self.find_binder(loc, &t, BinderKind::TypeParameter, &tparam.name.name)
            } else {
                let res = ast_visitor::type_param_default(self, kind, tparam);
                if matches!(kind, TypeParamsContext::Infer) {
                    self.infer_tparams.push(tparam.name.name.dupe());
                }
                res
            }
        }

        /// An `infer` binding is written in the extends clause but is in scope only
        /// in the true branch, so the names it binds are dropped before the false
        /// branch is walked.
        fn conditional_type(
            &mut self,
            conditional: &'ast ast::types::Conditional<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            let ast::types::Conditional {
                check_type,
                extends_type,
                true_type,
                false_type,
                comments,
            } = conditional;
            let outer_infer_tparams = self.infer_tparams.clone();
            let res = (|this: &mut Self| {
                this.type_(check_type)?;
                this.type_(extends_type)?;
                this.type_(true_type)
            })(self);
            self.infer_tparams = outer_infer_tparams;
            res?;
            self.type_(false_type)?;
            self.syntax_opt(comments.as_ref())?;
            Ok(())
        }

        fn class_(
            &mut self,
            loc: &'ast ALoc,
            cls: &'ast ast::class::Class<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            let name = cls.id.as_ref().map(|id| id.name.dupe());
            self.in_type_parameter_scope(cls.id.as_ref(), cls.tparams.as_ref(), |searcher| {
                searcher.in_class(name, &cls.body, |this| {
                    ast_visitor::class_default(this, loc, cls)
                })
            })
        }

        /// Names the declared class for its own body only, the way `interface`
        /// does. Its body is an object type, so the walk is spelled out here
        /// rather than deferred to `declare_class_default`, which would route the
        /// members through `object_type` and lose the name.
        fn declare_class(
            &mut self,
            _loc: &'ast ALoc,
            decl: &'ast ast::statement::DeclareClass<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            self.in_type_parameter_scope(Some(&decl.id), decl.tparams.as_ref(), |searcher| {
                declare_class_members(searcher, decl)
            })
        }

        fn record_declaration(
            &mut self,
            loc: &'ast ALoc,
            record: &'ast ast::statement::RecordDeclaration<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            let name = Some(record.id.name.dupe());
            self.in_type_parameter_scope(Some(&record.id), record.tparams.as_ref(), |searcher| {
                searcher.in_nominal(name, |this| {
                    ast_visitor::record_declaration_default(this, loc, record)
                })
            })
        }

        fn object_key(
            &mut self,
            key: &'ast ast::expression::object::Key<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            use ast::expression::object::Key;
            match key {
                Key::StringLiteral(((loc, t), _))
                | Key::NumberLiteral(((loc, t), _))
                | Key::BigIntLiteral(((loc, t), _))
                    if self.covers_target(loc) =>
                {
                    self.find_loc(loc, t, false, None)
                }
                _ => ast_visitor::object_key_default(self, key),
            }
        }

        fn expression(
            &mut self,
            expr: &'ast ast::expression::Expression<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            use ast::expression::ExpressionInner;
            use ast::expression::member;
            match expr.deref() {
                ExpressionInner::This { loc: (loc, t), .. }
                | ExpressionInner::Super { loc: (loc, t), .. }
                    if self.covers_target(loc) =>
                {
                    self.find_loc(loc, t, false, None)
                }
                ExpressionInner::Member { loc: (_, t), inner }
                    if let member::Property::PropertyPrivateName(pn) = &inner.property
                        && self.covers_target(&pn.loc) =>
                {
                    self.find_private_member_reference(&pn.loc, t, pn)
                }
                ExpressionInner::OptionalMember { loc: (_, t), inner }
                    if let member::Property::PropertyPrivateName(pn) = &inner.member.property
                        && self.covers_target(&pn.loc) =>
                {
                    self.find_private_member_reference(&pn.loc, t, pn)
                }
                _ => ast_visitor::expression_default(self, expr),
            }
        }

        // `new C()` names the constructor declaration at both the keyword
        // and the callee: `constructor C(...): ...`
        fn new(
            &mut self,
            loc: &'ast (ALoc, Type),
            expr: &'ast ast::expression::New<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            let (expr_loc, _) = loc;
            let (callee_loc, _) = expr.callee.loc();
            let expr_start_loc = expr_loc.to_loc_exn().first_char();
            let callee_start_loc = callee_loc.to_loc_exn().char_before().char_before();
            let new_loc = Loc::between(&expr_start_loc, &callee_start_loc);
            // The name the callee gives the constructor: `C` itself in
            // `new C()`, the property in `new ns.C()`. A target anywhere
            // else in a member callee (e.g. on `ns`) falls through to the
            // default walk.
            let callee_ctor_name: Option<(&ALoc, &FlowSmolStr)> = match expr.callee.deref() {
                ast::expression::ExpressionInner::Identifier { inner: id, .. } => {
                    Some((callee_loc, &id.name))
                }
                ast::expression::ExpressionInner::Member { inner, .. } => {
                    use ast::expression::member::Property;
                    match &inner.property {
                        Property::PropertyIdentifier(id) => {
                            let (name_loc, _) = &id.loc;
                            Some((name_loc, &id.name))
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            let covers_ctor_name = callee_ctor_name
                .as_ref()
                .is_some_and(|(name_loc, _)| self.covers_target(name_loc));
            if self.covers_target_loc(&new_loc) || covers_ctor_name {
                match (callee_ctor_name, self.cx.get_ctor_callee(expr_loc)) {
                    (Some((name_loc, name)), Some(t)) => {
                        self.find_binder(name_loc, &t, BinderKind::Constructor, name)
                    }
                    (None, Some(t)) => self.find_loc(callee_loc, &t, false, None),
                    (_, None) => ast_visitor::new_default(self, loc, expr),
                }
            } else {
                ast_visitor::new_default(self, loc, expr)
            }
        }

        fn jsx_attribute_name_identifier(
            &mut self,
            ident: &'ast ast::jsx::Identifier<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            let (loc, value_t) = &ident.loc;
            if self.covers_target(loc) {
                let reason = reason::mk_reason(
                    VirtualReasonDesc::RProperty(Some(Name::new(ident.name.dupe()))),
                    loc.dupe(),
                );
                let LazyHintT(_, lazy_hint) = type_env::get_hint(self.cx, loc.dupe());
                let hint_result = lazy_hint(self.cx, false, None, reason)?;
                // Split with_hint to avoid double &mut self borrow in closures
                let hint_t = type_hint::with_hint(Some, || None, hint_result);
                match hint_t {
                    // Frame the settled hint as a member of the component's
                    // props. A hint variable that never settled (intrinsic
                    // attributes, whose props never constrain it) or an
                    // implicit `any` reports bare instead.
                    Some(t) => {
                        let resolved = self.cx.find_resolved(&t);
                        let settled = match &resolved {
                            Some(resolved) => !matches!(
                                resolved.deref(),
                                ConstraintTypeInner::OpenT(_)
                                    | ConstraintTypeInner::AnyT(
                                        _,
                                        AnySource::AnyError(_)
                                            | AnySource::Unsound(_)
                                            | AnySource::Untyped,
                                    )
                            ),
                            None => false,
                        };
                        if settled {
                            // The props object is structural, so the binder
                            // has no nominal owner, even when the element
                            // sits lexically inside a class body.
                            self.find_binder_with_owner(
                                loc,
                                resolved.as_ref().unwrap_or(&t),
                                BinderKind::Property,
                                &ident.name,
                                None,
                            )
                        } else {
                            self.find_loc(loc, &t, false, None)
                        }
                    }
                    // No hint: the prop is not in the component's declared
                    // props (an extra prop on an inexact props object). Frame
                    // the value type as a property of the props object being
                    // passed, rather than reporting it bare.
                    None => self.find_binder_with_owner(
                        loc,
                        value_t,
                        BinderKind::Property,
                        &ident.name,
                        None,
                    ),
                }
            } else {
                ast_visitor::jsx_attribute_name_identifier_default(self, ident)
            }
        }

        fn declare_module(
            &mut self,
            loc: &'ast ALoc,
            m: &'ast ast::statement::DeclareModule<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            match &m.id {
                ast::statement::declare_module::Id::Identifier(id) => {
                    let (id_loc, _) = &id.loc;
                    if self.covers_target(id_loc) {
                        return Err(FoundResult::FoundHardcodedModule(
                            id_loc.dupe(),
                            id.name.dupe(),
                        ));
                    }
                }
                ast::statement::declare_module::Id::Literal(_) => {}
            }
            ast_visitor::declare_module_default(self, loc, m)
        }

        fn import_declaration(
            &mut self,
            loc: &'ast ALoc,
            decl: &'ast ast::statement::ImportDeclaration<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            self.find_module_specifier(&decl.source)?;
            ast_visitor::import_declaration_default(self, loc, decl)
        }

        fn export_named_declaration(
            &mut self,
            loc: &'ast ALoc,
            decl: &'ast ast::statement::ExportNamedDeclaration<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            if let Some(source) = &decl.source {
                self.find_module_specifier(source)?;
            }
            ast_visitor::export_named_declaration_default(self, loc, decl)
        }

        fn declare_export_declaration(
            &mut self,
            loc: &'ast ALoc,
            decl: &'ast ast::statement::DeclareExportDeclaration<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            if let Some(source) = &decl.source {
                self.find_module_specifier(source)?;
            }
            ast_visitor::declare_export_declaration_default(self, loc, decl)
        }

        fn match_object_pattern_property(
            &mut self,
            prop: &'ast ast::match_pattern::object_pattern::Property<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            use ast::match_pattern::object_pattern::Property;
            match prop {
                Property::Valid { loc, property } if self.covers_target(loc) => {
                    // If shorthand, skip looking at the key which doesn't exist in the source.
                    if property.shorthand {
                        ast_visitor::match_pattern_default(self, &property.pattern)?;
                        Ok(())
                    } else {
                        ast_visitor::match_object_pattern_property_default(self, prop)
                    }
                }
                _ => ast_visitor::match_object_pattern_property_default(self, prop),
            }
        }

        /// The object a destructuring pattern matches against, so that a
        /// property key below can resolve itself in it. Restored on exit,
        /// since patterns nest.
        fn pattern_object_p(
            &mut self,
            kind: Option<ast::VariableKind>,
            loc: &'ast (ALoc, Type),
            prop: &'ast ast::pattern::object::Property<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            let (_, t) = loc;
            let outer = self.enclosing_pattern_scrutinee.replace(t.dupe());
            let res: Result<(), FoundResult> =
                ast_visitor::pattern_object_p_default(self, kind, loc, prop);
            self.enclosing_pattern_scrutinee = outer;
            res
        }

        /// `d02` in `const {d02: d03} = ...`: the key names a member of the
        /// scrutinee, so it frames like a member reference. A shorthand key
        /// is the binding itself rather than a member access, and is left
        /// alone.
        fn pattern_object_property(
            &mut self,
            kind: Option<ast::VariableKind>,
            prop: &'ast ast::pattern::object::NormalProperty<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            if !prop.shorthand
                && let ast::pattern::object::Key::Identifier(id) = &prop.key
                && let (id_loc, t) = &id.loc
                && self.covers_target(id_loc)
                && let Some(scrutinee) = self.enclosing_pattern_scrutinee.dupe()
            {
                return self.find_loc(
                    id_loc,
                    t,
                    false,
                    Some(Framing::MemberRef {
                        name: id.name.dupe(),
                        object_type: scrutinee,
                    }),
                );
            }
            ast_visitor::pattern_object_property_default(self, kind, prop)
        }

        /// `'p'` in `T08['p']`: the index names a member of the object type,
        /// so it frames like a member reference over the access's own result
        /// type. Only string literals name members; other indices keep the
        /// default walk. Optional access (`M?.['p']`) also reaches this hook
        /// via `optional_indexed_access_type_default`, but a maybe-typed
        /// object has no expandable member to frame, so that form keeps the
        /// default walk too.
        fn indexed_access_type(
            &mut self,
            loc: &'ast (ALoc, Type),
            ia: &'ast ast::types::IndexedAccess<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            let (_, result_t) = loc;
            if let ast::types::TypeInner::StringLiteral {
                loc: index_payload,
                literal,
            } = &*ia.index
                && let (index_aloc, _) = index_payload
                && self.covers_target(index_aloc)
                && let (_, object_t) = ia.object.loc()
                && {
                    let resolved = self
                        .cx
                        .find_resolved(object_t)
                        .unwrap_or_else(|| object_t.dupe());
                    !matches!(resolved.deref(), ConstraintTypeInner::MaybeT(..))
                }
            {
                return self.find_loc(
                    index_aloc,
                    result_t,
                    false,
                    Some(Framing::MemberRef {
                        name: literal.value.dupe(),
                        object_type: object_t.dupe(),
                    }),
                );
            }
            ast_visitor::indexed_access_type_default(self, loc, ia)
        }

        /// Records the enum being declared, so that a member site below can
        /// resolve itself in the enum's object type. Members carry no types
        /// of their own.
        fn enum_declaration(
            &mut self,
            loc: &'ast ALoc,
            enum_: &'ast ast::statement::EnumDeclaration<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            let (_, t) = &enum_.id.loc;
            let outer = self
                .enclosing_enum
                .replace((enum_.id.name.dupe(), t.dupe()));
            let res = ast_visitor::enum_declaration_default(self, loc, enum_);
            self.enclosing_enum = outer;
            res
        }

        /// `A` in `enum E01 { A, ... }`: the member names a field of its
        /// enum, so it frames like a member reference.
        fn enum_member_name(
            &mut self,
            id: &'ast ast::statement::enum_declaration::MemberName<ALoc>,
        ) -> Result<(), FoundResult> {
            use ast::statement::enum_declaration::MemberName;
            let (loc, name) = match id {
                MemberName::Identifier(id) => (&id.loc, id.name.dupe()),
                MemberName::StringLiteral(loc, lit) => (loc, lit.value.dupe()),
            };
            if self.covers_target(loc)
                && let Some((_, enum_t)) = self.enclosing_enum.as_ref()
            {
                // The member's type is the enum's value type: the object type
                // itself would normalize to the `enum E` declaration, whose
                // printing drops the framing.
                let member_t = match &**enum_t {
                    ConstraintTypeInner::DefT(_, def) => match def.deref() {
                        DefTInner::EnumObjectT { enum_value_t, .. } => enum_value_t.dupe(),
                        _ => enum_t.dupe(),
                    },
                    _ => enum_t.dupe(),
                };
                return self.find_loc(
                    loc,
                    &member_t,
                    false,
                    Some(Framing::MemberRef {
                        name,
                        object_type: enum_t.dupe(),
                    }),
                );
            }
            ast_visitor::enum_member_name_default(self, id)
        }

        // The hooks below fire when the target is a binding's own name, so that
        // hover can frame the type as a declaration. Each checks the bound
        // identifier directly and falls through to the default walk otherwise, so a
        // target anywhere else inside the declaration is unaffected.
        // TODO: also frame `import` specifiers as `(alias)` and enum members as
        // `const e: E.A`, both of which need more than the bound name and its type.

        fn variable_declarator(
            &mut self,
            kind: ast::VariableKind,
            declarator: &'ast ast::statement::variable::Declarator<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            self.variable_binder(kind, declarator)?;
            ast_visitor::variable_declarator_default(self, kind, declarator)
        }

        fn declare_variable_declarator(
            &mut self,
            kind: ast::VariableKind,
            declarator: &'ast ast::statement::variable::Declarator<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            self.variable_binder(kind, declarator)?;
            ast_visitor::declare_variable_declarator_default(self, kind, declarator)
        }

        fn function_(
            &mut self,
            loc: &'ast ALoc,
            func: &'ast ast::function::Function<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            self.in_type_parameter_scope(func.id.as_ref(), func.tparams.as_ref(), |searcher| {
                ast_visitor::function_default(searcher, loc, func)
            })
        }

        fn function_declaration(
            &mut self,
            loc: &'ast ALoc,
            func: &'ast ast::function::Function<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            self.function_binder(func.id.as_ref())?;
            ast_visitor::function_declaration_default(self, loc, func)
        }

        fn declare_function(
            &mut self,
            loc: &'ast ALoc,
            decl: &'ast ast::statement::DeclareFunction<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            self.function_binder(decl.id.as_ref())?;
            let tparams = match decl.annot.annotation.deref() {
                TypeInner::Function { inner, .. } | TypeInner::ConstructorType { inner, .. } => {
                    inner.tparams.as_ref()
                }
                // Any other annotation shape records nothing: the parameters
                // stay contextless rather than inheriting an unrelated
                // enclosing declaration.
                _ => None,
            };
            self.in_type_parameter_scope(decl.id.as_ref(), tparams, |searcher| {
                ast_visitor::declare_function_default(searcher, loc, decl)
            })
        }

        fn class_method(
            &mut self,
            method: &'ast ast::class::Method<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            if let Some(kind) = Self::binder_kind_of_method(method)
                && let ast::expression::object::Key::Identifier(id) = &method.key
                && let (loc, t) = &id.loc
                && self.covers_target(loc)
            {
                // The `constructor` keyword names no binding of its own, so a
                // constructor is reported under its class's name. An anonymous
                // class has no name to borrow, so its constructor falls through
                // unframed rather than rendering as `constructor constructor`.
                let name = if kind == BinderKind::Constructor {
                    let Some(name) = self.enclosing_nominal.as_ref() else {
                        return ast_visitor::class_method_default(self, method);
                    };
                    name
                } else {
                    &id.name
                };
                return self.find_binder(loc, t, kind, name);
            }
            // A private key carries no type of its own, so the method's
            // stands in for it.
            if let Some(kind) = Self::binder_kind_of_method(method)
                && let ast::expression::object::Key::PrivateName(pn) = &method.key
                && self.covers_target(&pn.loc)
            {
                let (_, t) = &method.loc;
                return self.find_binder(&pn.loc, t, kind, &Self::private_member_name(pn));
            }
            ast_visitor::class_method_default(self, method)
        }

        fn class_property(
            &mut self,
            prop: &'ast ast::class::Property<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            if let ast::expression::object::Key::Identifier(id) = &prop.key
                && let (loc, t) = &id.loc
                && self.covers_target(loc)
            {
                return self.find_binder(loc, t, BinderKind::Property, &id.name);
            }
            ast_visitor::class_property_default(self, prop)
        }

        /// `#p` is a `PrivateField` rather than a `Property`, and its key carries no
        /// type of its own, so the field's stands in for it.
        fn class_private_field(
            &mut self,
            prop: &'ast ast::class::PrivateField<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            if self.covers_target(&prop.key.loc) {
                let (_, t) = &prop.loc;
                return self.find_binder(
                    &prop.key.loc,
                    t,
                    BinderKind::Property,
                    &Self::private_member_name(&prop.key),
                );
            }
            ast_visitor::class_private_field_default(self, prop)
        }

        /// `import {a}` / `import {a as b}`. An unrenamed import is an alias at its
        /// only name. For a renamed import, the local name is the alias and the
        /// remote name is framed as the declaration it references.
        fn import_named_specifier(
            &mut self,
            import_kind: ast::statement::ImportKind,
            spec: &'ast ast::statement::import_declaration::NamedSpecifier<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            match &spec.local {
                None => {
                    let (loc, t) = &spec.remote.loc;
                    if self.covers_target(loc) {
                        return self.find_alias(
                            loc,
                            t,
                            AliasKind::Import,
                            &spec.remote.name,
                            Framing::IdentifierRef,
                        );
                    }
                }
                Some(local) => {
                    let (local_loc, local_t) = &local.loc;
                    if self.covers_target(local_loc) {
                        return self.find_alias(
                            local_loc,
                            local_t,
                            AliasKind::Import,
                            &local.name,
                            Framing::IdentifierRef,
                        );
                    }
                    let (remote_loc, remote_t) = &spec.remote.loc;
                    if self.covers_target(remote_loc) {
                        return self.find_loc(
                            remote_loc,
                            remote_t,
                            false,
                            Some(Framing::RemoteIdentifierRef {
                                def_loc: spec.remote_name_def_loc.dupe(),
                                name: spec.remote.name.dupe(),
                            }),
                        );
                    }
                }
            }
            ast_visitor::import_named_specifier_default(self, import_kind, spec)
        }

        /// `import a from '…'`.
        fn import_default_specifier(
            &mut self,
            import_kind: &'ast ast::statement::ImportKind,
            id: &'ast ast::Identifier<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            let (loc, t) = &id.loc;
            if self.covers_target(loc) {
                return self.find_alias(
                    loc,
                    t,
                    AliasKind::Import,
                    &id.name,
                    Framing::IdentifierRef,
                );
            }
            ast_visitor::import_default_specifier_default(self, import_kind, id)
        }

        /// `import * as ns from '…'`. The type is the module, so the head names
        /// the module and the alias line names the binding this file introduced.
        fn import_namespace_specifier(
            &mut self,
            import_kind: ast::statement::ImportKind,
            spec_loc: &'ast ALoc,
            id: &'ast ast::Identifier<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            let (loc, t) = &id.loc;
            if self.covers_target(loc) {
                return self.find_alias(
                    loc,
                    t,
                    AliasKind::Import,
                    &id.name,
                    Framing::IdentifierRef,
                );
            }
            ast_visitor::import_namespace_specifier_default(self, import_kind, spec_loc, id)
        }

        /// `export {a}` / `export {a as b}`. Both names are aliases: `a` for the
        /// local binding, `b` for the name other modules see. Only `a` is a use of
        /// anything, so `b` borrows its declaration under `b`'s own name.
        fn export_named_declaration_specifier(
            &mut self,
            spec: &'ast ast::statement::export_named_declaration::ExportSpecifier<
                ALoc,
                (ALoc, Type),
            >,
        ) -> Result<(), FoundResult> {
            let (local_loc, _) = &spec.local.loc;
            if self.covers_target(local_loc) {
                let (loc, t) = &spec.local.loc;
                return self.find_alias(
                    loc,
                    t,
                    AliasKind::Export,
                    &spec.local.name,
                    Framing::IdentifierRef,
                );
            }
            if let Some(exported) = &spec.exported {
                let (loc, t) = &exported.loc;
                if self.covers_target(loc) {
                    return self.find_alias(
                        loc,
                        t,
                        AliasKind::Export,
                        &exported.name,
                        Framing::RenamedExportRef {
                            local: local_loc.dupe(),
                            name: exported.name.dupe(),
                        },
                    );
                }
            }
            ast_visitor::export_named_declaration_specifier_default(self, spec)
        }

        /// An object literal is not the class it may be written inside, so its
        /// members carry no qualifier.
        fn object(
            &mut self,
            loc: &'ast ALoc,
            expr: &'ast ast::expression::Object<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            self.in_nominal(None, |this| ast_visitor::object_default(this, loc, expr))
        }

        /// A member of an object literal.
        fn object_property(
            &mut self,
            prop: &'ast ast::expression::object::NormalProperty<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            use ast::expression::object::NormalProperty;
            let (key, kind) = match prop {
                NormalProperty::Init { key, .. } => (key, BinderKind::Property),
                NormalProperty::Method { key, .. } => (key, BinderKind::Method),
                NormalProperty::Get { key, .. } => (key, BinderKind::Getter),
                NormalProperty::Set { key, .. } => (key, BinderKind::Setter),
            };
            if let Some((loc, t, name)) = self.key_binder(key) {
                return self.find_binder(loc, t, kind, &name);
            }
            ast_visitor::object_property_default(self, prop)
        }

        /// A member of an object type, an interface body or a declared class
        /// body.
        fn object_property_type(
            &mut self,
            prop: &'ast ast::types::object::NormalProperty<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            use ast::types::object::PropertyValue;
            let kind = match (&prop.value, prop.method) {
                (PropertyValue::Get(..), _) => BinderKind::Getter,
                (PropertyValue::Set(..), _) => BinderKind::Setter,
                (PropertyValue::Init(_), true) => BinderKind::Method,
                (PropertyValue::Init(_), false) => BinderKind::Property,
            };
            if let Some((loc, t, name)) = self.key_binder(&prop.key) {
                return self.find_binder(loc, t, kind, &name);
            }
            ast_visitor::object_property_type_default(self, prop)
        }

        /// An object type is anonymous, so its members are unqualified. The bodies
        /// of a named interface and of a declared class do not come through here —
        /// those walk their members directly, so that they keep the name.
        fn object_type(
            &mut self,
            ot: &'ast ast::types::Object<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            self.in_nominal(None, |this| ast_visitor::object_type_default(this, ot))
        }

        fn type_alias(
            &mut self,
            loc: &'ast ALoc,
            alias: &'ast ast::statement::TypeAlias<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            self.in_type_parameter_scope(Some(&alias.id), alias.tparams.as_ref(), |searcher| {
                ast_visitor::type_alias_default(searcher, loc, alias)
            })
        }

        fn opaque_type(
            &mut self,
            loc: &'ast ALoc,
            opaque: &'ast ast::statement::OpaqueType<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            self.in_type_parameter_scope(Some(&opaque.id), opaque.tparams.as_ref(), |searcher| {
                ast_visitor::opaque_type_default(searcher, loc, opaque)
            })
        }

        fn component_declaration(
            &mut self,
            loc: &'ast ALoc,
            component: &'ast ast::statement::ComponentDeclaration<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            self.in_type_parameter_scope(
                Some(&component.id),
                component.tparams.as_ref(),
                |searcher| ast_visitor::component_declaration_default(searcher, loc, component),
            )
        }

        fn declare_component(
            &mut self,
            loc: &'ast ALoc,
            component: &'ast ast::statement::DeclareComponent<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            self.in_type_parameter_scope(
                Some(&component.id),
                component.tparams.as_ref(),
                |searcher| ast_visitor::declare_component_default(searcher, loc, component),
            )
        }

        /// Names the interface for its own body only: the extends clause and type
        /// parameters are walked outside that scope.
        fn interface(
            &mut self,
            _loc: &'ast ALoc,
            iface: &'ast ast::statement::Interface<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            self.in_type_parameter_scope(Some(&iface.id), iface.tparams.as_ref(), |searcher| {
                let ast::statement::Interface {
                    id,
                    tparams,
                    extends,
                    body,
                    comments,
                } = iface;
                searcher.binding_type_identifier(id)?;
                if let Some(tparams) = tparams {
                    searcher.type_params(&TypeParamsContext::Interface, tparams)?;
                }
                for (_loc, generic) in extends.iter() {
                    searcher.generic_type(generic)?;
                }
                let (_loc, obj_type) = body;
                named_object_body(searcher, id.name.dupe(), obj_type)?;
                searcher.syntax_opt(comments.as_ref())?;
                Ok(())
            })
        }

        fn function_param(
            &mut self,
            param: &'ast ast::function::Param<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            if let ast::function::Param::RegularParam { argument, .. } = param
                && let Some(id) = Self::simple_pattern_binder(argument)
                && let (loc, t) = &id.loc
                && self.covers_target(loc)
            {
                return self.find_binder(loc, t, BinderKind::Parameter, &id.name);
            }
            ast_visitor::function_param_default(self, param)
        }

        /// A `this: T` parameter binds no identifier, but hovers as the
        /// parameter it is. The `this` keyword has no node of its own, so
        /// claim exactly its span and let the annotation keep its own hover.
        fn function_this_param(
            &mut self,
            this_param: &'ast ast::function::ThisParam<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            let ast::function::ThisParam { loc, annot, .. } = this_param;
            let param_loc = loc.to_loc_exn();
            let keyword = Loc {
                source: param_loc.source.dupe(),
                start: param_loc.start,
                end: Position {
                    line: param_loc.start.line,
                    column: param_loc.start.column + 4,
                },
            };
            if self.covers_target_loc(&keyword) {
                let (_, t) = annot.annotation.loc();
                return self.find_binder(loc, t, BinderKind::Parameter, &FlowSmolStr::new("this"));
            }
            ast_visitor::function_this_param_default(self, this_param)
        }

        /// `(a: number)` in an interface call signature: the label binds
        /// nothing, but hovers as the parameter it names.
        fn function_param_type(
            &mut self,
            fpt: &'ast ast::types::function::Param<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            if let ast::types::function::ParamKind::Labeled { name: id, .. } = &fpt.param
                && let (loc, t) = &id.loc
                && self.covers_target(loc)
            {
                return self.find_binder(loc, t, BinderKind::Parameter, &id.name);
            }
            ast_visitor::function_param_type_default(self, fpt)
        }

        /// `[k: string]` in an index signature: the key binds nothing, but
        /// hovers as the parameter it names. The default walk skips the id
        /// entirely, so without this hook the key is hover-silent. The id
        /// node itself carries the indexer's value type, so the hovered type
        /// comes from the key annotation instead.
        fn object_indexer_property_type(
            &mut self,
            indexer: &'ast ast::types::object::Indexer<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            if let Some(id) = &indexer.id
                && let (loc, _) = &id.loc
                && self.covers_target(loc)
            {
                let (_, key_t) = indexer.key.loc();
                return self.find_binder(loc, key_t, BinderKind::Parameter, &id.name);
            }
            ast_visitor::object_indexer_property_type_default(self, indexer)
        }

        fn call_type_arg(
            &mut self,
            t: &'ast ast::expression::CallTypeArg<ALoc, (ALoc, Type)>,
        ) -> Result<(), FoundResult> {
            use ast::expression::CallTypeArg;
            match t {
                CallTypeArg::Implicit(implicit) => {
                    let (loc, ty) = &implicit.loc;
                    if self.covers_target(loc) {
                        self.find_loc(loc, ty, false, None)
                    } else {
                        ast_visitor::call_type_arg_default(self, t)
                    }
                }
                _ => ast_visitor::call_type_arg_default(self, t),
            }
        }
    }

    pub fn find(
        cx: &mut Context<'_>,
        typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
        loc: Loc,
    ) -> Result<TypeAtPosResult, flow_utils_concurrency::job_error::JobError> {
        let mut searcher = TypeAtPosSearcher {
            cx,
            target_loc: loc,
            enclosing_tparams: Vec::new(),
            enclosing_nominal: None,
            enclosing_pattern_scrutinee: None,
            enclosing_enum: None,
            infer_tparams: Vec::new(),
            enclosing_private_members: Vec::new(),
        };
        match searcher.program(typed_ast) {
            Ok(()) => Ok(TypeAtPosResult::NoResult),
            Err(FoundResult::FoundType {
                loc,
                is_type_identifier_reference,
                type_,
                framing,
                alias,
            }) => Ok(TypeAtPosResult::TypeResult {
                loc: loc.to_loc_exn().dupe(),
                is_type_identifier_reference,
                type_,
                framing: framing.map(|framing| *framing),
                alias,
            }),
            Err(FoundResult::FoundHardcodedModule(loc, name)) => Ok(
                TypeAtPosResult::HardcodedModuleResult(loc.to_loc_exn().dupe(), name),
            ),
            Err(FoundResult::Canceled(e)) => Err(e),
        }
    }
}

pub fn find_type_at_pos_annotation(
    cx: &mut Context<'_>,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    loc: Loc,
) -> Result<type_at_pos::TypeAtPosResult, flow_utils_concurrency::job_error::JobError> {
    type_at_pos::find(cx, typed_ast, loc)
}
