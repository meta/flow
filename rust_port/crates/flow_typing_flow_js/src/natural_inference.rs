/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::ops::Deref;
use std::rc::Rc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::enclosing_context::EnclosingContext;
use flow_common::hint::Hint;
use flow_common::hint::HintKind;
use flow_common::polarity::Polarity;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast::VariableKind;
use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_context::TypingMode;
use flow_typing_flow_common::type_subst;
use flow_typing_tvar;
use flow_typing_type::type_::ArrLiteral;
use flow_typing_type::type_::ArrType;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::BigIntLiteral;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::FieldData;
use flow_typing_type::type_::FrozenKind;
use flow_typing_type::type_::FunType;
use flow_typing_type::type_::Literal;
use flow_typing_type::type_::NumberLiteral;
use flow_typing_type::type_::ObjLiteral;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::Property;
use flow_typing_type::type_::PropertyInner;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::TupleElement;
use flow_typing_type::type_::Tvar;
use flow_typing_type::type_::TvarSeenSet;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeDestructorT;
use flow_typing_type::type_::TypeDestructorTInner;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::UniqueSymbolTData;
use flow_typing_type::type_::constraint;
use flow_typing_type::type_::elemt_of_arrtype;
use flow_typing_type::type_::eval;
use flow_typing_type::type_::exports;
use flow_typing_type::type_::properties;
use flow_typing_type::type_::property;
use flow_typing_type::type_::union_rep;
use flow_typing_type::type_::union_rep::UnionKind;
use flow_typing_type::type_util::mod_reason_of_t;
use flow_typing_type::type_util::non_literal_of_t;
use flow_typing_type::type_util::non_object_literal_of_t;
use flow_typing_type::type_util::reason_of_t;
use flow_typing_visitors::type_mapper;
use flow_typing_visitors::type_mapper::TypeMapper;
pub type LazyBool = Rc<flow_lazy::Lazy<Cx, bool>>;

pub struct SyntacticFlags {
    pub encl_ctx: EnclosingContext,
    pub decl: Option<VariableKind>,
    pub as_const: bool,
    pub frozen: FrozenKind,
    pub has_hint: LazyBool,
}

pub fn empty_syntactic_flags() -> SyntacticFlags {
    SyntacticFlags {
        encl_ctx: EnclosingContext::NoContext,
        decl: None,
        as_const: false,
        frozen: FrozenKind::NotFrozen,
        has_hint: Rc::new(flow_lazy::Lazy::new_forced(false)),
    }
}

pub fn mk_syntactic_flags(
    encl_ctx: Option<EnclosingContext>,
    decl: Option<VariableKind>,
    as_const: Option<bool>,
    frozen: Option<FrozenKind>,
    has_hint: Option<LazyBool>,
) -> SyntacticFlags {
    SyntacticFlags {
        encl_ctx: encl_ctx.unwrap_or(EnclosingContext::NoContext),
        decl,
        as_const: as_const.unwrap_or(false),
        frozen: frozen.unwrap_or(FrozenKind::NotFrozen),
        has_hint: has_hint.unwrap_or_else(|| Rc::new(flow_lazy::Lazy::new_forced(false))),
    }
}

fn is_builtin_promise<'cx>(cx: &mut Context<'cx>, t: &Type) -> bool {
    fn loop_<'cx>(cx: &mut Context<'cx>, seen: &mut BTreeSet<i32>, t: &Type) -> bool {
        match t.deref() {
            TypeInner::OpenT(tvar) => {
                let (root_id, constraints) = cx.find_constraints(tvar.id() as i32);
                if seen.contains(&root_id) {
                    false
                } else {
                    match constraints {
                        constraint::Constraints::FullyResolved(s) => {
                            let t = cx.force_fully_resolved_tvar(&s);
                            seen.insert(root_id);
                            loop_(cx, seen, &t)
                        }
                        _ => false,
                    }
                }
            }
            TypeInner::DefT(_, def_t)
                if let DefTInner::PolyT(box PolyTData { t_out, .. }) = def_t.deref() =>
            {
                match t_out.deref() {
                    TypeInner::DefT(_, inner_def_t)
                        if let DefTInner::ClassT(class_t) = inner_def_t.deref()
                            && let TypeInner::ThisInstanceT(data) = class_t.deref() =>
                    {
                        let inst = &data.instance.inst;
                        inst.class_name
                            .as_ref()
                            .is_some_and(|name| name.as_str() == "Promise")
                            && inst
                                .class_id
                                .0
                                .source()
                                .is_some_and(|file| cx.is_global_libdef(file))
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }
    let mut seen = BTreeSet::new();
    loop_(cx, &mut seen, t)
}

fn is_literal_union(rep: &union_rep::UnionRep) -> bool {
    match rep.union_kind() {
        UnionKind::ConditionalKind
        | UnionKind::ImplicitInstantiationKind
        | UnionKind::LogicalKind => true,
        UnionKind::ProvidersKind | UnionKind::ResolvedKind | UnionKind::UnknownKind => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SingletonAction {
    /// Generalize singleton type
    DoNotKeep,
    /// Keep singleton type and `from_annot` value to `false`. This is used to
    /// avoid generalizing singleton types that are checked against annotations
    /// where the added precision is useful.
    KeepAsIs,
    /// Keep singleton type, but change `from_annot` to `true`.
    /// Used for 'const' type parameter conversion.
    KeepAsConst,
}

struct LiteralMapCx(RefCell<BTreeSet<i32>>);

impl LiteralMapCx {
    fn new() -> Self {
        Self(RefCell::new(BTreeSet::new()))
    }

    fn contains(&self, root_id: &i32) -> bool {
        self.0.borrow().contains(root_id)
    }

    fn with_added<R>(&self, root_id: i32, f: impl FnOnce() -> R) -> R {
        let inserted = self.0.borrow_mut().insert(root_id);
        let result = f();
        if inserted {
            self.0.borrow_mut().remove(&root_id);
        }
        result
    }
}

// Free-function versions of singleton handlers, parameterized by SingletonAction.
fn singleton_str_action(action: SingletonAction, t: Type, r: &Reason, value: &FlowSmolStr) -> Type {
    match action {
        SingletonAction::KeepAsIs => t,
        SingletonAction::KeepAsConst => Type::new(TypeInner::DefT(
            r.dupe(),
            DefT::new(DefTInner::SingletonStrT {
                from_annot: true,
                value: value.dupe(),
            }),
        )),
        SingletonAction::DoNotKeep => Type::new(TypeInner::DefT(
            r.dupe().replace_desc(VirtualReasonDesc::RString),
            DefT::new(DefTInner::StrGeneralT(Literal::AnyLiteral)),
        )),
    }
}

fn singleton_num_action(
    action: SingletonAction,
    t: Type,
    r: &Reason,
    value: &NumberLiteral,
) -> Type {
    match action {
        SingletonAction::KeepAsIs => t,
        SingletonAction::KeepAsConst => Type::new(TypeInner::DefT(
            r.dupe(),
            DefT::new(DefTInner::SingletonNumT {
                from_annot: true,
                value: value.dupe(),
            }),
        )),
        SingletonAction::DoNotKeep => Type::new(TypeInner::DefT(
            r.dupe().replace_desc(VirtualReasonDesc::RNumber),
            DefT::new(DefTInner::NumGeneralT(Literal::AnyLiteral)),
        )),
    }
}

fn singleton_bool_action(action: SingletonAction, t: Type, r: &Reason, value: bool) -> Type {
    match action {
        SingletonAction::KeepAsIs => t,
        SingletonAction::KeepAsConst => Type::new(TypeInner::DefT(
            r.dupe(),
            DefT::new(DefTInner::SingletonBoolT {
                from_annot: true,
                value,
            }),
        )),
        SingletonAction::DoNotKeep => Type::new(TypeInner::DefT(
            r.dupe().replace_desc(VirtualReasonDesc::RBoolean),
            DefT::new(DefTInner::BoolGeneralT),
        )),
    }
}

fn singleton_bigint_action(
    action: SingletonAction,
    t: Type,
    r: &Reason,
    value: &BigIntLiteral,
) -> Type {
    match action {
        SingletonAction::KeepAsIs => t,
        SingletonAction::KeepAsConst => Type::new(TypeInner::DefT(
            r.dupe(),
            DefT::new(DefTInner::SingletonBigIntT {
                from_annot: true,
                value: value.dupe(),
            }),
        )),
        SingletonAction::DoNotKeep => Type::new(TypeInner::DefT(
            r.dupe().replace_desc(VirtualReasonDesc::RBigInt),
            DefT::new(DefTInner::BigIntGeneralT(Literal::AnyLiteral)),
        )),
    }
}

fn unique_symbol_action(
    action: SingletonAction,
    t: Type,
    r: &Reason,
    data: &UniqueSymbolTData,
) -> Type {
    match action {
        SingletonAction::KeepAsIs => t,
        SingletonAction::KeepAsConst => Type::new(TypeInner::DefT(
            r.dupe(),
            DefT::new(DefTInner::UniqueSymbolT(UniqueSymbolTData {
                symbol: data.symbol.dupe(),
                from_annot: true,
            })),
        )),
        SingletonAction::DoNotKeep => Type::new(TypeInner::DefT(
            r.dupe().replace_desc(VirtualReasonDesc::RSymbol),
            DefT::new(DefTInner::SymbolT),
        )),
    }
}

// Template literals from values generalize to `string`, exactly like string
// literals. There is no separate const form: keeping the template as-is is
// already the precise form that `const` type parameters want.
fn template_literal_action(action: SingletonAction, t: Type, r: &Reason) -> Type {
    match action {
        SingletonAction::KeepAsIs | SingletonAction::KeepAsConst => t,
        SingletonAction::DoNotKeep => Type::new(TypeInner::DefT(
            r.dupe().replace_desc(VirtualReasonDesc::RString),
            DefT::new(DefTInner::StrGeneralT(Literal::AnyLiteral)),
        )),
    }
}

fn literal_type_mapper_tvar<'cx>(
    type_fn: &mut dyn FnMut(&mut Context<'cx>, &LiteralMapCx, Type) -> Type,
    cx: &mut Context<'cx>,
    map_cx: &LiteralMapCx,
    id: u32,
) -> u32 {
    match cx.find_constraints(id as i32) {
        (root_id, constraint::Constraints::FullyResolved(s)) => {
            if map_cx.contains(&root_id) {
                id
            } else {
                map_cx.with_added(root_id, || {
                    let t = cx.force_fully_resolved_tvar(&s);
                    let t_prime = type_fn(cx, map_cx, t.dupe());
                    if t.ptr_eq(&t_prime) {
                        id
                    } else {
                        flow_typing_tvar::mk_fully_resolved_no_wrap(cx, t_prime) as u32
                    }
                })
            }
        }
        _ => id,
    }
}

// Result of literal_type_mapper_type_dispatch telling the caller what to do.
enum LiteralMapAction {
    // Return this type as-is
    Done(Type),
    // Call type_default on this type
    TypeDefault(Type),
    // Call map_literal_array on this type
    LiteralArray(Type),
}

// The element type of a literal array with several element types resolves to
// their union, so it is generalized along with them.
fn literal_array_elem_t<'cx, M: TypeMapper<'cx, LiteralMapCx>>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &LiteralMapCx,
    elem_t: Type,
) -> Type {
    if let TypeInner::OpenT(tvar) = elem_t.deref()
        && let (root_id, constraint::Constraints::FullyResolved(s)) =
            cx.find_constraints(tvar.id() as i32)
        && !map_cx.contains(&root_id)
    {
        let t = cx.force_fully_resolved_tvar(&s);
        if let TypeInner::UnionT(_, rep) = t.deref()
            && let UnionKind::ResolvedKind = rep.union_kind()
        {
            return map_cx.with_added(root_id, || {
                let t_prime = type_mapper::type_default(mapper, cx, map_cx, t.dupe());
                if t.ptr_eq(&t_prime) {
                    elem_t.dupe()
                } else {
                    let id = flow_typing_tvar::mk_fully_resolved_no_wrap(cx, t_prime) as u32;
                    Type::new(TypeInner::OpenT(Tvar::new(tvar.reason().dupe(), id)))
                }
            });
        }
    }
    mapper.type_(cx, map_cx, elem_t)
}

fn map_literal_array<'cx, M: TypeMapper<'cx, LiteralMapCx>>(
    mapper: &mut M,
    cx: &mut Context<'cx>,
    map_cx: &LiteralMapCx,
    t: Type,
) -> Type {
    let t = match t.deref() {
        TypeInner::DefT(r, def_t) if let DefTInner::ArrT(arr) = def_t.deref() => {
            let elem_t = elemt_of_arrtype(arr);
            let elem_t_prime = literal_array_elem_t(mapper, cx, map_cx, elem_t.dupe());
            if elem_t.ptr_eq(&elem_t_prime) {
                t
            } else {
                let arr_prime = match arr.deref() {
                    ArrType::ArrayAT(box data) => ArrType::ArrayAT(Box::new(ArrayATData {
                        elem_t: elem_t_prime,
                        ..data.clone()
                    })),
                    ArrType::TupleAT(box data) => ArrType::TupleAT(Box::new(TupleATData {
                        elem_t: elem_t_prime,
                        ..data.clone()
                    })),
                    ArrType::ROArrayAT(box data) => ArrType::ROArrayAT(Box::new(ROArrayATData {
                        elem_t: elem_t_prime,
                        ..data.clone()
                    })),
                };
                Type::new(TypeInner::DefT(
                    r.dupe(),
                    DefT::new(DefTInner::ArrT(Rc::new(arr_prime))),
                ))
            }
        }
        _ => t,
    };
    type_mapper::type_default(mapper, cx, map_cx, t)
}

// Both LiteralTypeMapper and ConvertLiteralTypeToConstMapper delegate to this.
fn literal_type_mapper_type_dispatch<'cx>(
    singleton_action: &dyn Fn(&Context<'_>, &ALoc) -> SingletonAction,
    cx: &mut Context<'cx>,
    t: &Type,
) -> LiteralMapAction {
    match t.deref() {
        TypeInner::OpenT(_) => LiteralMapAction::TypeDefault(t.dupe()),
        TypeInner::DefT(_, def_t)
            if let DefTInner::ArrT(arr) = def_t.deref()
                && arr.literal().is_literal() =>
        {
            let t = match arr.literal() {
                ArrLiteral::LiteralExpression => non_literal_of_t(&mod_reason_of_t(
                    &|r| r.replace_desc_new(VirtualReasonDesc::RArrayLit),
                    t,
                )),
                ArrLiteral::NonLiteral | ArrLiteral::Literal | ArrLiteral::Expression => t.dupe(),
            };
            LiteralMapAction::LiteralArray(t)
        }
        TypeInner::DefT(_, def_t)
            if let DefTInner::ObjT(o) = def_t.deref()
                && o.literal.is_literal() =>
        {
            let t = match o.literal {
                ObjLiteral::LiteralExpression => non_literal_of_t(&mod_reason_of_t(
                    &|r| r.replace_desc_new(VirtualReasonDesc::RObjectLit),
                    t,
                )),
                ObjLiteral::NonLiteral | ObjLiteral::Literal | ObjLiteral::Expression => t.dupe(),
            };
            LiteralMapAction::TypeDefault(t)
        }
        TypeInner::DefT(r, def_t)
            if let DefTInner::FunT(statics, ft) = def_t.deref()
                && ft.literal =>
        {
            LiteralMapAction::TypeDefault(Type::new(TypeInner::DefT(
                r.dupe(),
                DefT::new(DefTInner::FunT(
                    statics.dupe(),
                    Rc::new(FunType {
                        literal: false,
                        ..ft.as_ref().clone()
                    }),
                )),
            )))
        }
        TypeInner::TypeAppT(box TypeAppTData { type_, .. }) if is_builtin_promise(cx, type_) => {
            // async expressions will wrap result in Promise<>, so we need to descend here
            LiteralMapAction::TypeDefault(t.dupe())
        }
        TypeInner::UnionT(_, rep) if is_literal_union(rep) => {
            LiteralMapAction::TypeDefault(t.dupe())
        }
        TypeInner::DefT(r, def_t) => match def_t.deref() {
            DefTInner::SingletonStrT {
                from_annot: false,
                value,
            } => {
                let action = singleton_action(cx, r.loc());
                LiteralMapAction::Done(singleton_str_action(action, t.dupe(), r, value))
            }
            DefTInner::SingletonNumT {
                from_annot: false,
                value,
            } => {
                let action = singleton_action(cx, r.loc());
                LiteralMapAction::Done(singleton_num_action(action, t.dupe(), r, value))
            }
            DefTInner::SingletonBoolT {
                from_annot: false,
                value,
            } => {
                let action = singleton_action(cx, r.loc());
                LiteralMapAction::Done(singleton_bool_action(action, t.dupe(), r, *value))
            }
            DefTInner::SingletonBigIntT {
                from_annot: false,
                value,
            } => {
                let action = singleton_action(cx, r.loc());
                LiteralMapAction::Done(singleton_bigint_action(action, t.dupe(), r, value))
            }
            DefTInner::UniqueSymbolT(data) if !data.from_annot => {
                let action = singleton_action(cx, r.loc());
                LiteralMapAction::Done(unique_symbol_action(action, t.dupe(), r, data))
            }
            _ => LiteralMapAction::Done(t.dupe()),
        },
        TypeInner::TemplateLiteralT {
            reason,
            from_annot: false,
            ..
        } => {
            let action = singleton_action(cx, reason.loc());
            LiteralMapAction::Done(template_literal_action(action, t.dupe(), reason))
        }
        _ => LiteralMapAction::Done(t.dupe()),
    }
}

// `literal_type_mapper` walks a literal type and replaces singleton types that
// originate from literals according to `singleton_action`.
//
// A key assumption is that `t` is the type inferred for a literal expression
// (object, array, primitive literal) or the result of a conditional or logical
// expression. This allows us to keep the cost of this visit low, by only
// descending into types that are considered literals (for now we use reasons to
// determine this.)
struct LiteralTypeMapper<F: Fn(&Context<'_>, &ALoc) -> SingletonAction> {
    singleton_action: F,
}

impl<'cx, F: Fn(&Context<'_>, &ALoc) -> SingletonAction> TypeMapper<'cx, LiteralMapCx>
    for LiteralTypeMapper<F>
{
    fn exports(
        &mut self,
        _cx: &mut Context<'cx>,
        _map_cx: &LiteralMapCx,
        id: exports::Id,
    ) -> exports::Id {
        id
    }

    fn props(
        &mut self,
        cx: &mut Context<'cx>,
        map_cx: &LiteralMapCx,
        id: properties::Id,
    ) -> properties::Id {
        let props_map = cx.find_props(id.dupe());
        let props_map_prime = props_map.ident_map(|prop| {
            property::ident_map_t(|t| self.type_(cx, map_cx, t.dupe()), prop).into_owned()
        });
        if props_map.ptr_eq(&props_map_prime) {
            id
        } else {
            cx.generate_property_map(props_map_prime)
        }
    }

    fn eval_id(
        &mut self,
        _cx: &mut Context<'cx>,
        _map_cx: &LiteralMapCx,
        id: eval::Id,
    ) -> eval::Id {
        id
    }

    fn call_prop(&mut self, _cx: &mut Context<'cx>, _map_cx: &LiteralMapCx, id: i32) -> i32 {
        id
    }

    fn tvar(&mut self, cx: &mut Context<'cx>, map_cx: &LiteralMapCx, _r: &Reason, id: u32) -> u32 {
        literal_type_mapper_tvar(
            &mut |cx, map_cx, t| self.type_(cx, map_cx, t),
            cx,
            map_cx,
            id,
        )
    }

    fn type_(&mut self, cx: &mut Context<'cx>, map_cx: &LiteralMapCx, t: Type) -> Type {
        match literal_type_mapper_type_dispatch(&self.singleton_action, cx, &t) {
            LiteralMapAction::Done(t) => t,
            LiteralMapAction::TypeDefault(t) => type_mapper::type_default(self, cx, map_cx, t),
            LiteralMapAction::LiteralArray(t) => map_literal_array(self, cx, map_cx, t),
        }
    }
}

/// Walk a literal type and replaces singleton types that originate from literals
/// according to `singleton_action`
pub fn convert_literal_type<'cx>(
    cx: &mut Context<'cx>,
    singleton_action: impl Fn(&Context<'_>, &ALoc) -> SingletonAction,
    t: Type,
) -> Type {
    let mut mapper = LiteralTypeMapper { singleton_action };
    mapper.type_(cx, &LiteralMapCx::new(), t)
}

fn is_literal_type<'cx>(cx: &mut Context<'cx>, seen: &mut BTreeSet<i32>, t: &Type) -> bool {
    match t.deref() {
        TypeInner::OpenT(tvar) => match cx.find_constraints(tvar.id() as i32) {
            (root_id, constraint::Constraints::FullyResolved(s)) => {
                if seen.contains(&root_id) {
                    false
                } else {
                    let t = cx.force_fully_resolved_tvar(&s);
                    seen.insert(root_id);
                    is_literal_type(cx, seen, &t)
                }
            }
            _ => false,
        },
        TypeInner::DefT(_, def_t) => match def_t.deref() {
            DefTInner::ArrT(arr) => arr.literal().is_literal(),
            DefTInner::ObjT(o) => o.literal.is_literal(),
            DefTInner::FunT(_, ft) => ft.literal,
            DefTInner::SingletonStrT {
                from_annot: false, ..
            }
            | DefTInner::SingletonNumT {
                from_annot: false, ..
            }
            | DefTInner::SingletonBoolT {
                from_annot: false, ..
            }
            | DefTInner::SingletonBigIntT {
                from_annot: false, ..
            }
            | DefTInner::UniqueSymbolT(UniqueSymbolTData {
                from_annot: false, ..
            }) => true,
            _ => false,
        },
        TypeInner::TemplateLiteralT {
            from_annot: false, ..
        } => true,
        _ => false,
    }
}

struct ImplicitInstantiationLiteralMapper<F: Fn(&Context<'_>, &ALoc) -> SingletonAction> {
    singleton_action: F,
}

impl<'cx, F: Fn(&Context<'_>, &ALoc) -> SingletonAction> TypeMapper<'cx, ()>
    for ImplicitInstantiationLiteralMapper<F>
{
    fn tvar(&mut self, _cx: &mut Context<'cx>, _map_cx: &(), _r: &Reason, id: u32) -> u32 {
        id
    }

    fn call_prop(&mut self, cx: &mut Context<'cx>, map_cx: &(), id: i32) -> i32 {
        type_subst::call_prop(self, cx, map_cx, id)
    }

    fn props(&mut self, cx: &mut Context<'cx>, map_cx: &(), id: properties::Id) -> properties::Id {
        type_subst::props(self, cx, map_cx, id)
    }

    fn exports(&mut self, cx: &mut Context<'cx>, map_cx: &(), id: exports::Id) -> exports::Id {
        type_subst::exports(self, cx, map_cx, id)
    }

    fn type_(&mut self, cx: &mut Context<'cx>, map_cx: &(), t: Type) -> Type {
        if is_literal_type(cx, &mut BTreeSet::new(), &t) {
            return convert_literal_type(cx, &self.singleton_action, t);
        }
        match t.deref() {
            TypeInner::EvalT {
                type_: x,
                defer_use_t,
                ..
            } => {
                let op = &defer_use_t.0;
                let r = &defer_use_t.1;
                let d = &defer_use_t.2;
                let x_prime = self.type_(cx, map_cx, x.dupe());
                let d_prime = self.destructor(cx, map_cx, d.dupe());
                if x.ptr_eq(&x_prime) && Rc::ptr_eq(d, &d_prime) {
                    t
                } else {
                    let new_defer_use =
                        TypeDestructorT::new(TypeDestructorTInner(op.clone(), r.dupe(), d_prime));
                    flow_typing_flow_common::flow_cache::eval::id(cx, x_prime, new_defer_use)
                }
            }
            TypeInner::OpenT(_)
            | TypeInner::ImplicitInstantiationTvar(_)
            | TypeInner::DefT(..)
            | TypeInner::ThisInstanceT(..)
            | TypeInner::ThisTypeAppT(..)
            | TypeInner::TypeAppT(..)
            | TypeInner::GenericT(..)
            | TypeInner::NominalT { .. }
            | TypeInner::OptionalT { .. }
            | TypeInner::MaybeT(..)
            | TypeInner::IntersectionT(..)
            | TypeInner::UnionT(..) => type_mapper::type_default(self, cx, map_cx, t),
            // Stop here cause we need the precision
            TypeInner::AnnotT(..) | TypeInner::KeysT(..) => t,
            // Stop cause there's nothing interesting
            TypeInner::FunProtoT(_)
            | TypeInner::ObjProtoT(_)
            | TypeInner::NullProtoT(_)
            | TypeInner::NamespaceT(_)
            | TypeInner::TemplateLiteralT { .. }
            | TypeInner::StringMappingT { .. }
            | TypeInner::AnyT(..) => t,
        }
    }

    // The EvalT case is the only case that calls this function. We've explicitly
    // overrided it in all cases, so this should never be called
    fn eval_id(&mut self, _cx: &mut Context<'cx>, _map_cx: &(), _id: eval::Id) -> eval::Id {
        unreachable!("eval_id should never be called on implicit_instantiation_literal_mapper")
    }
}

pub fn convert_implicit_instantiation_literal_type<'cx>(
    cx: &mut Context<'cx>,
    singleton_action: impl Fn(&Context<'_>, &ALoc) -> SingletonAction,
    t: Type,
) -> Type {
    let mut mapper = ImplicitInstantiationLiteralMapper { singleton_action };
    mapper.type_(cx, &(), t)
}

fn aloc_contains(outer: &ALoc, inner: &ALoc) -> bool {
    use flow_aloc::aloc_representation_do_not_use::is_keyed;
    if is_keyed(outer) || is_keyed(inner) {
        return false;
    }
    let outer = outer.to_loc_exn();
    let inner = inner.to_loc_exn();
    outer.contains(inner)
}

struct ConvertLiteralTypeToConstMapper {
    loc_range: ALoc,
}

impl ConvertLiteralTypeToConstMapper {
    fn reason_def_loc_within_call(&self, reason: &Reason) -> bool {
        aloc_contains(&self.loc_range, reason.def_loc())
    }
}

impl<'cx> TypeMapper<'cx, LiteralMapCx> for ConvertLiteralTypeToConstMapper {
    fn exports(
        &mut self,
        _cx: &mut Context<'cx>,
        _map_cx: &LiteralMapCx,
        id: exports::Id,
    ) -> exports::Id {
        id
    }

    fn eval_id(
        &mut self,
        _cx: &mut Context<'cx>,
        _map_cx: &LiteralMapCx,
        id: eval::Id,
    ) -> eval::Id {
        id
    }

    fn call_prop(&mut self, _cx: &mut Context<'cx>, _map_cx: &LiteralMapCx, id: i32) -> i32 {
        id
    }

    fn tvar(&mut self, cx: &mut Context<'cx>, map_cx: &LiteralMapCx, _r: &Reason, id: u32) -> u32 {
        literal_type_mapper_tvar(
            &mut |cx, map_cx, t| self.type_(cx, map_cx, t),
            cx,
            map_cx,
            id,
        )
    }

    fn type_(&mut self, cx: &mut Context<'cx>, map_cx: &LiteralMapCx, t: Type) -> Type {
        match t.deref() {
            TypeInner::DefT(r, def_t) if let DefTInner::ArrT(arr) = def_t.deref() => {
                match arr.deref() {
                    ArrType::ArrayAT(box ArrayATData {
                        elem_t,
                        tuple_view: Some(tv),
                        ..
                    }) => {
                        if arr.literal().is_literal() && self.reason_def_loc_within_call(r) {
                            let elem_t_prime =
                                literal_array_elem_t(self, cx, map_cx, elem_t.dupe());
                            let elements: Vec<TupleElement> = tv
                                .elements
                                .iter()
                                .map(|el| {
                                    let t_prime = self.type_(cx, map_cx, el.t.dupe());
                                    TupleElement {
                                        reason: el.reason.dupe(),
                                        name: el.name.dupe(),
                                        t: t_prime,
                                        polarity: Polarity::Positive,
                                        optional: el.optional,
                                    }
                                })
                                .collect();
                            let rest = tv.rest.as_ref().map(|el| TupleElement {
                                t: self.type_(cx, map_cx, el.t.dupe()),
                                polarity: Polarity::Positive,
                                ..el.clone()
                            });
                            let r = r.dupe().replace_desc(None);
                            Type::new(TypeInner::DefT(
                                r,
                                DefT::new(DefTInner::ArrT(Rc::new(ArrType::TupleAT(Box::new(
                                    TupleATData {
                                        elem_t: elem_t_prime,
                                        elements: elements.into(),
                                        react_dro: None,
                                        arity: tv.arity,
                                        inexact: false,
                                        rest,
                                        strictness_kind: cx.type_strictness_kind(),
                                        literal: ArrLiteral::NonLiteral,
                                        fresh: false,
                                    },
                                ))))),
                            ))
                        } else {
                            t
                        }
                    }
                    _ => self.type_as_literal_type_mapper(cx, map_cx, t),
                }
            }
            TypeInner::DefT(r, def_t) if let DefTInner::ObjT(o) = def_t.deref() => {
                if o.literal.is_literal() && self.reason_def_loc_within_call(r) {
                    let t = self.type_as_literal_type_mapper(cx, map_cx, t);
                    non_object_literal_of_t(&mod_reason_of_t(&|r| r.replace_desc(None), &t))
                } else {
                    t
                }
            }
            _ => self.type_as_literal_type_mapper(cx, map_cx, t),
        }
    }

    // This method is only reachable through a literal object argument to this call.
    fn props(
        &mut self,
        cx: &mut Context<'cx>,
        map_cx: &LiteralMapCx,
        id: properties::Id,
    ) -> properties::Id {
        let props_map = cx.find_props(id.dupe());
        let props_map_prime = props_map.ident_map(|p| match p.deref() {
            PropertyInner::Field(fd) => {
                let type_prime = self.type_(cx, map_cx, fd.type_.dupe());
                if fd.type_.ptr_eq(&type_prime) && fd.polarity == Polarity::Positive {
                    p.dupe()
                } else {
                    Property::new(PropertyInner::Field(Box::new(FieldData {
                        preferred_def_locs: fd.preferred_def_locs.clone(),
                        key_loc: fd.key_loc.dupe(),
                        type_: type_prime,
                        polarity: Polarity::Positive,
                    })))
                }
            }
            _ => p.dupe(),
        });
        if props_map.ptr_eq(&props_map_prime) {
            id
        } else {
            cx.generate_property_map(props_map_prime)
        }
    }
}

impl ConvertLiteralTypeToConstMapper {
    fn type_as_literal_type_mapper<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        map_cx: &LiteralMapCx,
        t: Type,
    ) -> Type {
        match literal_type_mapper_type_dispatch(&|_, _| SingletonAction::KeepAsConst, cx, &t) {
            LiteralMapAction::Done(t) => t,
            LiteralMapAction::TypeDefault(t) => type_mapper::type_default(self, cx, map_cx, t),
            LiteralMapAction::LiteralArray(t) => map_literal_array(self, cx, map_cx, t),
        }
    }
}

pub fn convert_literal_type_to_const<'cx>(loc_range: ALoc, cx: &mut Context<'cx>, t: Type) -> Type {
    let mut mapper = ConvertLiteralTypeToConstMapper { loc_range };
    mapper.type_(cx, &LiteralMapCx::new(), t)
}

fn is_generalization_candidate_inner<'cx>(
    cx: &mut Context<'cx>,
    seen: &mut TvarSeenSet<i32>,
    t: &Type,
) -> bool {
    match t.deref() {
        TypeInner::DefT(_, def_t)
            if let DefTInner::SingletonStrT {
                from_annot: false, ..
            }
            | DefTInner::SingletonBoolT {
                from_annot: false, ..
            }
            | DefTInner::SingletonNumT {
                from_annot: false, ..
            }
            | DefTInner::SingletonBigIntT {
                from_annot: false, ..
            }
            | DefTInner::UniqueSymbolT(UniqueSymbolTData {
                from_annot: false, ..
            }) = def_t.deref() =>
        {
            true
        }
        TypeInner::OpenT(tvar) => match cx.find_constraints(tvar.id() as i32) {
            (root_id, constraint::Constraints::FullyResolved(s)) => {
                if seen.contains(&root_id) {
                    false
                } else {
                    seen.with_added(root_id, |seen| {
                        let t = cx.force_fully_resolved_tvar(&s);
                        is_generalization_candidate_inner(cx, seen, &t)
                    })
                }
            }
            _ => false,
        },
        TypeInner::UnionT(_, rep) => rep
            .members_iter()
            .any(|member| is_generalization_candidate_inner(cx, seen, member)),
        TypeInner::TemplateLiteralT {
            from_annot: false, ..
        } => true,
        _ => false,
    }
}

pub fn is_generalization_candidate<'cx>(cx: &mut Context<'cx>, t: &Type) -> bool {
    is_generalization_candidate_inner(cx, &mut TvarSeenSet::new(), t)
}

/// Whether the given location has any non-best-effort hint, based only on the
/// hint's kind. Unlike `has_ast_syntactic_hint` below, checker-synthesized
/// hints count.
pub fn loc_has_hint<'cx>(cx: &mut Context<'cx>, loc: &ALoc) -> bool {
    let env = cx.environment();
    match env.ast_hint_map.get(loc) {
        None => false,
        Some(hints) if hints.is_empty() => false,
        Some(hints) => hints.iter().any(|hint| match hint {
            Hint::HintPlaceholder => true,
            Hint::HintT(_, HintKind::ExpectedTypeHint) => true,
            Hint::HintDecomp(_, _, HintKind::ExpectedTypeHint) => true,
            Hint::HintT(_, HintKind::BestEffortHint) => false,
            Hint::HintDecomp(_, _, HintKind::BestEffortHint) => false,
        }),
    }
}

/// Whether any AST hint at the given location originates from a user-written
/// type annotation. Unlike `loc_has_hint` above, which is kind-based,
/// checker-synthesized hints do not count.
pub fn has_ast_syntactic_hint<'cx>(cx: &mut Context<'cx>, loc: &ALoc) -> bool {
    cx.environment().ast_hint_map.get(loc).is_some_and(|hints| {
        hints.iter().any(|hint| match hint {
            Hint::HintT(node, _) | Hint::HintDecomp(_, node, _) => node.is_syntactic(),
            Hint::HintPlaceholder => false,
        })
    })
}

pub fn enclosing_context_needs_precise(encl_ctx: &EnclosingContext) -> bool {
    match encl_ctx {
        EnclosingContext::NoContext => false,
        EnclosingContext::SwitchTestContext { .. }
        | EnclosingContext::OtherTestContext
        | EnclosingContext::IndexContext
        | EnclosingContext::JsxTitleNameContext
        | EnclosingContext::JsxAttrOrChildrenContext
        | EnclosingContext::LiteralTestContext
        | EnclosingContext::MatchPattern
        | EnclosingContext::StrictComparison => true,
    }
}

/// When do we need to preserve a precise primitive literal type?
///
/// 1. The enclosing context is such that requires increased precision:
///    a. conditional contexts, since this means that some refinement
///    will be performed, and so precise types can give us more accurate
///    results, and
///    b. index contexts, since this allows for more precise operations over
///    the object or array that is being accessed.
///
/// 2. We are initializing a const-variable.
///
/// 3. We are in an `as const` context.
///
/// 4. We are computing a property of a frozen object.
///
/// 5. The value is about to be compared against an annotation. We use the presence
///    of a hint as a proxy to determine this. If the primitive literal is part of
///    another container literal expression, we might not have a hint recorded for
///    it. For this reason, we first check if the enclosing expression has a hint.
///    This is done by forcing the value of `has_hint` that has been accumulated
///    until this point. For example, in `{f:['a']}`, if the outer object has a
///    hint then `'a'` is also considered to have a hint. Finally, if none of the
///    above hold, we check for a hint for the target location.
///
/// Template literals with substitutions share this predicate (see the
/// `TemplateLiteral` branch in `flow_typing_statement`), so they stay
/// precise under exactly the same conditions as primitive literals.
pub fn needs_precise_type<'cx>(
    cx: &mut Context<'cx>,
    encl_ctx: &EnclosingContext,
    decl: &Option<VariableKind>,
    as_const: bool,
    frozen: &FrozenKind,
    has_hint: &LazyBool,
    loc: &ALoc,
) -> bool {
    enclosing_context_needs_precise(encl_ctx)
        || *decl == Some(VariableKind::Const)
        || as_const
        || *frozen == FrozenKind::FrozenProp
        || *has_hint.get_forced(cx)
        || loc_has_hint(cx, loc)
}

fn choose_precision<'cx>(
    cx: &mut Context<'cx>,
    precise: impl FnOnce(&mut Context<'cx>) -> Type,
    general: impl FnOnce(&mut Context<'cx>) -> Type,
    loc: &ALoc,
    needs_precise: impl FnOnce(&mut Context<'cx>) -> bool,
) -> Type {
    let typing_mode = cx.typing_mode().dupe();
    if matches!(
        &typing_mode,
        TypingMode::SynthesisMode {
            target_loc: Some(target_loc),
        } if aloc_contains(target_loc, loc)
    ) {
        let general_t = general(cx);
        return cx.mk_placeholder(reason_of_t(&general_t).dupe());
    }
    if matches!(&typing_mode, TypingMode::SynthesisMode { .. }) {
        cx.set_synthesis_produced_uncacheable_result();
    }
    if needs_precise(cx) {
        precise(cx)
    } else {
        general(cx)
    }
}

pub fn adjust_precision<'cx>(
    cx: &mut Context<'cx>,
    syntactic_flags: &SyntacticFlags,
    precise: impl FnOnce(&mut Context<'cx>) -> Type,
    general: impl FnOnce(&mut Context<'cx>) -> Type,
    loc: &ALoc,
) -> Type {
    let SyntacticFlags {
        encl_ctx,
        decl,
        as_const,
        frozen,
        has_hint,
    } = syntactic_flags;
    choose_precision(cx, precise, general, loc, |cx| {
        needs_precise_type(cx, encl_ctx, decl, *as_const, frozen, has_hint, loc)
    })
}

/// Unlike primitive literals, a call to `Symbol()` only has a stable identity
/// when it initializes a `const`. Once that inferred identity is stored in a
/// binding, ordinary natural inference decides whether a later use should keep
/// it or widen it to `symbol`.
pub fn adjust_unique_symbol_precision<'cx>(
    cx: &mut Context<'cx>,
    syntactic_flags: &SyntacticFlags,
    precise: impl FnOnce(&mut Context<'cx>) -> Type,
    general: impl FnOnce(&mut Context<'cx>) -> Type,
    loc: &ALoc,
) -> Type {
    choose_precision(cx, precise, general, loc, |_| {
        syntactic_flags.decl == Some(VariableKind::Const)
    })
}

pub fn try_generalize<'cx>(
    cx: &mut Context<'cx>,
    syntactic_flags: &SyntacticFlags,
    loc: &ALoc,
    t: Type,
) -> Type {
    if is_generalization_candidate(cx, &t) {
        let t_clone = t.dupe();
        let general = |cx: &mut Context<'cx>| {
            convert_literal_type(cx, |_, _| SingletonAction::DoNotKeep, t_clone)
        };
        let t_precise = t;
        let precise = |_: &mut Context<'cx>| t_precise;
        adjust_precision(cx, syntactic_flags, precise, general, loc)
    } else {
        t
    }
}
