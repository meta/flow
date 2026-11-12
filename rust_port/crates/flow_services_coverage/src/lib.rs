/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![feature(box_patterns)]
#![feature(never_type)]

use std::collections::BTreeMap;
use std::ops::Deref;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_parser::ast;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::loc::Loc;
use flow_typing_context::Context;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::ImplicitInstantiationTvarData;
use flow_typing_type::type_::ImplicitInstantiationTvarKind;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::ThisTypeAppTData;
use flow_typing_type::type_::Type as FlowType;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::constraint::Constraints;
use flow_typing_type::type_::eval;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpMode {
    OpAnd,
    OpOr,
}

pub fn unit_of_op(op: OpMode) -> bool {
    match op {
        OpMode::OpAnd => true, // mixed
        OpMode::OpOr => false, // empty
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize
)]
pub enum Kind {
    Checked,
    Any,
    Empty,
}

impl Kind {
    pub fn to_str(self) -> &'static str {
        match self {
            Kind::Checked => "Checked",
            Kind::Any => "Any",
            Kind::Empty => "Empty",
        }
    }

    pub fn m_and(a: Kind, b: Kind) -> Kind {
        match (a, b) {
            (Kind::Any, _) => Kind::Any,
            (_, Kind::Any) => Kind::Any,
            (Kind::Empty, _) => Kind::Empty,
            (_, Kind::Empty) => Kind::Empty,
            (Kind::Checked, Kind::Checked) => Kind::Checked,
        }
    }

    pub fn m_or(a: Kind, b: Kind) -> Kind {
        match (a, b) {
            (Kind::Any, _) => Kind::Any,
            (_, Kind::Any) => Kind::Any,
            (Kind::Empty, m2) => m2,
            (m1, Kind::Empty) => m1,
            (Kind::Checked, Kind::Checked) => Kind::Checked,
        }
    }

    pub fn merge(op: OpMode, a: Kind, b: Kind) -> Kind {
        match op {
            OpMode::OpAnd => Kind::m_and(a, b),
            OpMode::OpOr => Kind::m_or(a, b),
        }
    }

    pub fn to_bool(self) -> bool {
        match self {
            Kind::Any | Kind::Empty => false,
            Kind::Checked => true,
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize
)]
pub struct FileCoverage {
    pub checked: i32,
    pub uncovered: i32,
    pub empty: i32,
}

pub fn initial_coverage() -> FileCoverage {
    FileCoverage {
        checked: 0,
        uncovered: 0,
        empty: 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvarStatus {
    Started,
    Done(Kind),
}

pub struct CoverageVisitor {
    // Type variables may appear in a cycle in the dependency graph, which requires
    // us to track the ones we've visited to avoid infinite recursion. There are three
    // stages of tvar resolution w.r.t coverage:
    //
    // - The tvar has not been seen before (there is no entry in tvar_cache). In this
    //   case we descend into the lower bounds of the tvar, marking its binding as
    //   Started in the tvar_cache.
    //
    // - The tvar has been seen and has been resolved (status is `Done`). In this case
    //   we reuse the cached result.
    //
    // - The tvar is in the process of resolution (status is `Started`).
    //   These are types of the form:
    //
    //   ```js
    //     type X = X | number
    //              ^
    //   ```
    //   we consider the recursive occurrence as uncovered (Any). This case should
    //   be rare and it's arguable if we should be allowing it in the first place,
    //   so we assign the value that corresponds to the fewest guarantees.
    tvar_cache: BTreeMap<u32, TvarStatus>,
}

impl CoverageVisitor {
    pub fn new() -> Self {
        CoverageVisitor {
            tvar_cache: BTreeMap::new(),
        }
    }

    fn tvar<'a>(&mut self, cx: &mut Context<'a>, id: u32) -> Kind {
        let (root_id, constraints) = cx.find_constraints(id as i32);
        let root_id = root_id as u32;
        if id != root_id {
            self.tvar(cx, root_id)
        } else {
            match self.tvar_cache.get(&root_id) {
                Some(TvarStatus::Started) => Kind::Any,
                Some(&TvarStatus::Done(cov)) => cov,
                None => {
                    self.tvar_cache.insert(root_id, TvarStatus::Started);
                    let cov = match &constraints {
                        Constraints::Resolved(t) => self.type_(cx, t),
                        Constraints::FullyResolved(s) => {
                            let t = cx.force_fully_resolved_tvar(s);
                            self.type_(cx, &t)
                        }
                        Constraints::Unresolved(bounds) => {
                            let keys: Vec<FlowType> =
                                bounds.borrow().lower.keys().map(|k| k.dupe()).collect();
                            self.types_list(cx, OpMode::OpOr, &keys)
                        }
                    };
                    self.tvar_cache.insert(root_id, TvarStatus::Done(cov));
                    cov
                }
            }
        }
    }

    pub fn type_<'a>(&mut self, cx: &mut Context<'a>, t: &FlowType) -> Kind {
        match t.deref() {
            TypeInner::OpenT(tvar) => self.tvar(cx, tvar.id()),
            TypeInner::ImplicitInstantiationTvar(
                data @ box ImplicitInstantiationTvarData {
                    kind: ImplicitInstantiationTvarKind::Inferred,
                    ..
                },
            ) => self.tvar(cx, data.id as u32),
            TypeInner::EvalT { type_, id, .. } => self.eval_t(cx, type_, id),
            // Non-concrete (fallthrough) constructors
            TypeInner::AnnotT(_, t, _)
            | TypeInner::TypeAppT(box TypeAppTData { type_: t, .. })
            | TypeInner::GenericT(box GenericTData { bound: t, .. })
            | TypeInner::ThisTypeAppT(box ThisTypeAppTData { this_t: t, .. }) => self.type_(cx, t),
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::PolyT(box PolyTData { t_out: t, .. }) | DefTInner::TypeT(_, t) => {
                    self.type_(cx, t)
                }
                // Concrete covered
                DefTInner::ArrT(_)
                | DefTInner::BigIntGeneralT(_)
                | DefTInner::BoolGeneralT
                | DefTInner::ClassT(_)
                | DefTInner::FunT(..)
                | DefTInner::InstanceT(_)
                | DefTInner::MixedT(_)
                | DefTInner::NumGeneralT(_)
                | DefTInner::NullT
                | DefTInner::SymbolT
                | DefTInner::UniqueSymbolT(_)
                | DefTInner::ObjT(_)
                | DefTInner::ReactAbstractComponentT(_)
                | DefTInner::RendersT(_)
                | DefTInner::NumericStrKeyT(_)
                | DefTInner::SingletonNumT { .. }
                | DefTInner::SingletonStrT { .. }
                | DefTInner::SingletonBigIntT { .. }
                | DefTInner::SingletonBoolT { .. }
                | DefTInner::StrGeneralT(_)
                | DefTInner::VoidT
                | DefTInner::TsVoidT
                | DefTInner::EnumObjectT { .. }
                | DefTInner::EnumValueT(_) => Kind::Checked,
                // Concrete uncovered
                // TODO: Rethink coverage for these types
                DefTInner::EmptyT => Kind::Empty,
            },
            TypeInner::UnionT(_, rep) => {
                let mut iter = rep.members_iter();
                let t0 = iter.next().unwrap();
                let rest: Vec<FlowType> = iter.map(|t| t.dupe()).collect();
                self.types_nel(cx, OpMode::OpOr, t0, &rest)
            }
            TypeInner::IntersectionT(_, rep) => {
                let mut iter = rep.members_iter();
                let t0 = iter.next().unwrap();
                let rest: Vec<FlowType> = iter.map(|t| t.dupe()).collect();
                self.types_nel(cx, OpMode::OpAnd, t0, &rest)
            }
            // Concrete covered constructors
            TypeInner::FunProtoT(_)
            | TypeInner::ThisInstanceT(..)
            | TypeInner::KeysT(..)
            | TypeInner::TemplateLiteralT { .. }
            | TypeInner::StringMappingT { .. }
            | TypeInner::MaybeT(..)
            | TypeInner::NamespaceT(_)
            | TypeInner::NullProtoT(_)
            | TypeInner::NominalT { .. }
            | TypeInner::ObjProtoT(_)
            | TypeInner::OptionalT { .. } => Kind::Checked,
            // Concrete uncovered constructors
            TypeInner::AnyT(..)
            | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                kind: ImplicitInstantiationTvarKind::Placeholder,
                ..
            }) => Kind::Any,
        }
    }

    fn eval_t<'a>(&mut self, cx: &mut Context<'a>, t: &FlowType, id: &eval::Id) -> Kind {
        let evaluated = cx.evaluated();
        let t = match evaluated.get(id) {
            Some(cached) => cached,
            None => t,
        };
        self.type_(cx, t)
    }

    fn types_<'a>(&mut self, cx: &mut Context<'a>, op: OpMode, acc: Kind, ts: &[FlowType]) -> Kind {
        match ts {
            [] => acc,
            [t, rest @ ..] => {
                let cov = self.type_(cx, t);
                let merged_kind = Kind::merge(op, cov, acc);
                match merged_kind {
                    // Cannot recover from Any, so exit early
                    Kind::Any => Kind::Any,
                    Kind::Checked | Kind::Empty => self.types_(cx, op, merged_kind, rest),
                }
            }
        }
    }

    fn types_list<'a>(&mut self, cx: &mut Context<'a>, op: OpMode, ts: &[FlowType]) -> Kind {
        match ts {
            [] => Kind::Empty,
            [t, rest @ ..] => self.types_nel(cx, op, t, rest),
        }
    }

    fn types_nel<'a>(
        &mut self,
        cx: &mut Context<'a>,
        op: OpMode,
        t: &FlowType,
        ts: &[FlowType],
    ) -> Kind {
        let init_kind = self.type_(cx, t);
        match init_kind {
            Kind::Any => Kind::Any,
            Kind::Checked | Kind::Empty => self.types_(cx, op, init_kind, ts),
        }
    }
}

struct CoverageFolder<F, A> {
    f: F,
    acc: A,
}

impl<'ast, F, A> AstVisitor<'ast, ALoc, (ALoc, FlowType), &'ast ALoc> for CoverageFolder<F, A>
where
    F: FnMut(&ALoc, &FlowType, &mut A),
{
    fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
        loc
    }

    fn normalize_type(type_: &'ast (ALoc, FlowType)) -> &'ast ALoc {
        &type_.0
    }

    fn expression(
        &mut self,
        expr: &'ast ast::expression::Expression<ALoc, (ALoc, FlowType)>,
    ) -> Result<(), !> {
        let (ref loc, ref t) = *expr.loc();
        (self.f)(loc, t, &mut self.acc);
        ast_visitor::expression_default(self, expr)
    }

    fn object_property(
        &mut self,
        prop: &'ast ast::expression::object::NormalProperty<ALoc, (ALoc, FlowType)>,
    ) -> Result<(), !> {
        ast_visitor::object_property_default(self, prop)?;
        match prop {
            ast::expression::object::NormalProperty::Method { loc, key, .. } => {
                let t = match key {
                    ast::expression::object::Key::Identifier(id) => Some(&id.loc.1),
                    ast::expression::object::Key::StringLiteral((annot, _)) => Some(&annot.1),
                    ast::expression::object::Key::NumberLiteral((annot, _)) => Some(&annot.1),
                    ast::expression::object::Key::BigIntLiteral((annot, _)) => Some(&annot.1),
                    _ => None,
                };
                if let Some(t) = t {
                    (self.f)(loc, t, &mut self.acc);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn statement(
        &mut self,
        stmt: &'ast ast::statement::Statement<ALoc, (ALoc, FlowType)>,
    ) -> Result<(), !> {
        ast_visitor::statement_default(self, stmt)?;
        match &**stmt {
            ast::statement::StatementInner::ClassDeclaration { loc, inner } => {
                if let Some(id) = &inner.id {
                    (self.f)(loc, &id.loc.1, &mut self.acc);
                }
            }
            ast::statement::StatementInner::DeclareClass { loc, inner } => {
                (self.f)(loc, &inner.id.loc.1, &mut self.acc);
            }
            ast::statement::StatementInner::DeclareExportDeclaration { inner, .. } => {
                if let Some(ref decl) = inner.declaration {
                    match decl {
                        ast::statement::declare_export_declaration::Declaration::NamedOpaqueType {
                            loc,
                            declaration,
                        } => {
                            (self.f)(loc, &declaration.id.loc.1, &mut self.acc);
                        }
                        ast::statement::declare_export_declaration::Declaration::Class {
                            loc,
                            declaration,
                        } => {
                            (self.f)(loc, &declaration.id.loc.1, &mut self.acc);
                        }
                        _ => {}
                    }
                }
            }
            ast::statement::StatementInner::DeclareInterface { loc, inner } => {
                (self.f)(loc, &inner.id.loc.1, &mut self.acc);
            }
            ast::statement::StatementInner::DeclareModule { loc, inner } => match &inner.id {
                ast::statement::declare_module::Id::Identifier(id) => {
                    (self.f)(loc, &id.loc.1, &mut self.acc);
                }
                ast::statement::declare_module::Id::Literal((annot, _)) => {
                    (self.f)(loc, &annot.1, &mut self.acc);
                }
            },
            ast::statement::StatementInner::DeclareTypeAlias { loc, inner } => {
                (self.f)(loc, &inner.id.loc.1, &mut self.acc);
            }
            ast::statement::StatementInner::DeclareOpaqueType { loc, inner } => {
                (self.f)(loc, &inner.id.loc.1, &mut self.acc);
            }
            ast::statement::StatementInner::InterfaceDeclaration { loc, inner } => {
                (self.f)(loc, &inner.id.loc.1, &mut self.acc);
            }
            ast::statement::StatementInner::OpaqueType { loc, inner } => {
                (self.f)(loc, &inner.id.loc.1, &mut self.acc);
            }
            ast::statement::StatementInner::TypeAlias { loc, inner } => {
                (self.f)(loc, &inner.id.loc.1, &mut self.acc);
            }
            _ => {}
        }
        Ok(())
    }

    fn class_identifier(
        &mut self,
        _ident: &'ast ast::Identifier<ALoc, (ALoc, FlowType)>,
    ) -> Result<(), !> {
        Ok(())
    }

    // skip this
    fn jsx_element_name(
        &mut self,
        name: &'ast ast::jsx::Name<ALoc, (ALoc, FlowType)>,
    ) -> Result<(), !> {
        ast_visitor::jsx_element_name_default(self, name)?;
        match name {
            ast::jsx::Name::MemberExpression(member) => {
                (self.f)(&member.loc, &member.property.loc.1, &mut self.acc);
            }
            ast::jsx::Name::Identifier(_) | ast::jsx::Name::NamespacedName(_) => {}
        }
        Ok(())
    }

    fn jsx_member_expression_object(
        &mut self,
        object: &'ast ast::jsx::member_expression::Object<ALoc, (ALoc, FlowType)>,
    ) -> Result<(), !> {
        match object {
            ast::jsx::member_expression::Object::Identifier(ident) => {
                (self.f)(&ident.loc.0, &ident.loc.1, &mut self.acc);
                Ok(())
            }
            ast::jsx::member_expression::Object::MemberExpression(_) => {
                ast_visitor::jsx_member_expression_object_default(self, object)
            }
        }
    }

    fn import_named_specifier(
        &mut self,
        _import_kind: ast::statement::ImportKind,
        spec: &'ast ast::statement::import_declaration::NamedSpecifier<ALoc, (ALoc, FlowType)>,
    ) -> Result<(), !> {
        let ast::statement::import_declaration::NamedSpecifier {
            kind: _,
            kind_loc: _,
            local,
            remote,
            remote_name_def_loc: _,
        } = spec;
        if let Some(ident) = local {
            self.pattern_identifier(None, ident)?;
        }
        self.pattern_identifier(None, remote)?;
        Ok(())
    }

    fn pattern_identifier(
        &mut self,
        kind: Option<ast::VariableKind>,
        ident: &'ast ast::Identifier<ALoc, (ALoc, FlowType)>,
    ) -> Result<(), !> {
        (self.f)(&ident.loc.0, &ident.loc.1, &mut self.acc);
        ast_visitor::pattern_identifier_default(self, kind, ident)
    }
}

pub fn coverage_fold_tast<F, A>(f: F, init: A, tast: &ast::Program<ALoc, (ALoc, FlowType)>) -> A
where
    F: FnMut(&ALoc, &FlowType, &mut A),
{
    let mut folder = CoverageFolder { f, acc: init };
    let Ok(()) = folder.program(tast);
    folder.acc
}

pub fn covered_types<'a>(
    should_check: bool,
    cx: &mut Context<'a>,
    tast: &ast::Program<ALoc, (ALoc, FlowType)>,
) -> Vec<(Loc, Kind)> {
    let mut visitor = CoverageVisitor::new();
    let mut compute_cov = |t: &FlowType| -> Kind {
        if should_check {
            visitor.type_(cx, t)
        } else {
            Kind::Empty
        }
    };
    let step = |loc: &ALoc, t: &FlowType, acc: &mut Vec<(Loc, Kind)>| {
        acc.push((loc.to_loc_exn().dupe(), compute_cov(t)));
    };
    let mut result = coverage_fold_tast(step, Vec::new(), tast);
    result.sort_by(|(a, _), (b, _)| a.cmp(b));
    result
}

pub fn file_coverage<'a>(
    cx: &mut Context<'a>,
    tast: &ast::Program<ALoc, (ALoc, FlowType)>,
) -> FileCoverage {
    let mut visitor = CoverageVisitor::new();
    let step = |_loc: &ALoc, t: &FlowType, acc: &mut FileCoverage| match visitor.type_(cx, t) {
        Kind::Any => acc.uncovered += 1,
        Kind::Checked => acc.checked += 1,
        Kind::Empty => acc.empty += 1,
    };
    coverage_fold_tast(step, initial_coverage(), tast)
}
