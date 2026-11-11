/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Implicit instantiation by a walk (`experimental.implicit_instantiation_walk`).
//!
//! Instead of running the operation's flows against fresh tvars and pinning what reaches them,
//! the solve walks the argument types against the parameter types, and the return type against
//! the return hint, in the shape of subtyping. Each type parameter is replaced by a marker, a
//! `GenericT` with a fresh name, and the walk records the types each marker must be a supertype
//! of (lower bounds) or a subtype of (upper bounds). The walk adds no flows and reports no errors.
//! A placeholder gives no lower or upper bound. Pinning then reads the recorded bounds as before,
//! and the operation is checked with the solution by the usual flows, which report the errors.
//!
//! A generic value is instantiated where the walk meets it, against the type it flows to with
//! the solution so far in place of the markers. Generic values that are arguments are visited after
//! the other arguments and the return hint.
//!
//! A type destructor whose parts don't mention a marker is evaluated as usual. One that does is
//! walked through: utilities that keep the shape of their argument (`$ReadOnly`, `Partial`, ...)
//! as their argument, and a conditional type into both of its branches.
//!
//! A destructor whose result the flow-based solve's pinning reverses (a spread, a rest, the config
//! of a component) gives the marker under it the reversal of the other side, computed directly. A
//! pair that the walk has no rule for, such as a destructor stuck on a marker (`T[K]` with `T` a
//! marker), says nothing about the markers: the walk makes no type variables for it. With the flag
//! on, the walk solves every implicit instantiation: there is no fallback to the flow-based solve.
//! Only whether the operation fails anyway, which keeps a type parameter without evidence from
//! being underconstrained, is found by flowing its arguments (`check_instantiation`).

use std::sync::Arc;

use flow_aloc::ALoc;
use flow_aloc::ALocId;
use flow_common::reason::Name;
use flow_common::reason::VirtualReasonDesc;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_typing_errors::intermediate_error_types::ExpectedModulePurpose;
use flow_typing_type::type_::CanonicalRendersForm;
use flow_typing_type::type_::DestructorSpreadTupleTypeData;
use flow_typing_type::type_::DictType;
use flow_typing_type::type_::EnumInfoInner;
use flow_typing_type::type_::ImplicitInstantiationTvarData;
use flow_typing_type::type_::ImplicitInstantiationTvarKind;
use flow_typing_type::type_::InstanceKind;
use flow_typing_type::type_::InstanceT;
use flow_typing_type::type_::MixedFlavor;
use flow_typing_type::type_::NominalType;
use flow_typing_type::type_::NominalTypeInner;
use flow_typing_type::type_::ObjKind;
use flow_typing_type::type_::ObjType;
use flow_typing_type::type_::OptionalIndexedAccessIndex;
use flow_typing_type::type_::PropRef;
use flow_typing_type::type_::PropertyType;
use flow_typing_type::type_::RendersVariant;
use flow_typing_type::type_::ResolvedArgData;
use flow_typing_type::type_::ResolvedParam;
use flow_typing_type::type_::ResolvedSpreadArgData;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::TupleElement;
use flow_typing_type::type_::TupleViewKind;
use flow_typing_type::type_::TypeStrictnessKind;
use flow_typing_type::type_::TypeTKind;
use flow_typing_type::type_::UnionEnum;
use flow_typing_type::type_::UnresolvedArgData;
use flow_typing_type::type_::UnresolvedParam;
use flow_typing_type::type_::UnsoundnessKind;
use flow_typing_type::type_::inter_rep;
use flow_typing_type::type_::mixed_t;
use flow_typing_type::type_::null;
use flow_typing_type::type_::object::ObjectToolReactConfigData;
use flow_typing_type::type_::placeholder_t;
use flow_typing_type::type_::poly;
use flow_typing_type::type_::property;

use super::*;

const MAX_DEPTH: u32 = 32;

/// How many times the walk may be inside the same pair of classes or aliases: recursive library
/// types (an `Array` method returning an `Array`) would otherwise unfold without end.
const MAX_NESTING: usize = 1;

/// How deep the nesting of type arguments that the nesting limit tells apart goes.
const MAX_TARG_DEPTH: usize = 8;

/// The most pairs a solve walks.
const MAX_STEPS: usize = 100_000;

/// The most members a union with nested unions is flattened to.
const MAX_FLAT_MEMBERS: usize = 128;

/// How much a bound says about its marker. The solution so far, which a generic value or a
/// conditional type met during the walk is instantiated with, reads only the bounds of the highest
/// priority a marker has, but `Soft` ones. Pinning reads all of them, and merges the upper bounds
/// in this order, each replacing the one taken so far if it is a subtype of it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Priority {
    /// An upper bound from the bound of a type parameter pinned before: the bound check of that
    /// type parameter fails unless the solution is below it.
    Bound,
    /// What the arguments and the hint give the marker.
    Direct,
    /// A reversal met while walking the solution of a type parameter pinned before against its
    /// bound (what flows to `Partial<T>`). As a reversal, it says only which properties the marker
    /// may have, so it ranks after direct evidence, but it comes from a solution, so before the
    /// reversals met in the arguments and the hint, which have the solution so far.
    BoundReversal,
    /// The reversal of a destructor of the marker (a spread, a rest, the config of a component),
    /// or what flows to `Partial` or `Required`. The flow-based solve reads it after the others.
    Reversal,
    /// The hint through `NonNullable`, and an invariant position of an instance of a generic
    /// value whose type parameters had no evidence. The solution so far does not read them.
    Soft,
}

/// The bounds recorded for one marker.
#[derive(Default, Clone)]
struct Bounds {
    lower: Vec<(Type, Priority)>,
    upper: Vec<(Type, Priority)>,
    /// Types met in invariant positions. The flow-based solve unifies there, so the first one is
    /// the solution.
    exact: Vec<Type>,
}

fn types(bounds: &[(Type, Priority)]) -> impl Iterator<Item = &Type> {
    bounds.iter().map(|(t, _)| t)
}

/// Adds `t` to `bounds` with `priority`, or raises the priority of `t` if it is there.
fn add_bound(bounds: &mut Vec<(Type, Priority)>, t: &Type, priority: Priority) {
    match bounds.iter_mut().find(|(b, _)| b.ptr_eq(t)) {
        Some((_, p)) => *p = (*p).min(priority),
        None => bounds.push((t.dupe(), priority)),
    }
}

/// The bounds of the highest priority among `bounds`, but `Soft`.
fn best(bounds: &[(Type, Priority)]) -> Vec<Type> {
    let top = (bounds.iter())
        .map(|(_, p)| *p)
        .filter(|p| *p != Priority::Soft)
        .min();
    (bounds.iter())
        .filter(|(_, p)| Some(*p) == top)
        .map(|(t, _)| t.dupe())
        .collect()
}

/// The parts of a function type that a call walks its arguments against.
struct Signature<'a> {
    params: &'a [FunParam],
    rest_param: Option<&'a FunRestParam>,
    /// `None` for a constructor, whose receiver is the new instance.
    this_param: Option<&'a Type>,
    return_t: &'a Type,
}

fn log(cx: &Context<'_>, line: impl FnOnce() -> String) {
    flow_typing_debug::verbose::print_if_verbose_lazy(cx, None, None, None, || vec![line()]);
}

/// Runs flows whose errors are not the walk's to report: they belong to the check of the
/// operation, if it makes them. `None` if they fail under speculation.
fn sandbox<'cx, T>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    preserve: usize,
    f: impl FnOnce(&mut Context<'cx>) -> Result<T, FlowJsException>,
) -> Result<Option<T>, FlowJsException> {
    let errors = cx.errors();
    let result = run_keeping_clean_flows(cx, env, preserve, f);
    cx.reset_errors(errors);
    instantiation_solver::sandboxed(result)
}

/// Runs flows that the walk sandboxes. As in the flow-based solve, the caches the solve fills are
/// shared by its flows and restored after it, but for the constraint cache entries of flows without
/// errors, which are kept, at the level `preserve` of the solve: a later flow of the same types is
/// known to succeed. The entries of a flow with errors go at once, so that a later flow of the
/// same types fails too. The errors the flows defer to an enclosing speculation branch are
/// dropped, as the caller drops their others.
fn run_keeping_clean_flows<'cx, T>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    preserve: usize,
    f: impl FnOnce(&mut Context<'cx>) -> Result<T, FlowJsException>,
) -> Result<T, FlowJsException> {
    let errors = cx.errors();
    let deferred = env.deferred_error_count();
    let level = cx.constraint_cache().level_count();
    cx.constraint_cache_mut().push_level();
    let result = f(cx);
    if result.is_ok() && errors.ptr_eq(&cx.errors()) && env.deferred_error_count() == deferred {
        cx.constraint_cache_mut()
            .move_top_entries_to_level(preserve);
    }
    cx.constraint_cache_mut().truncate_to(level);
    env.truncate_deferred_errors(deferred);
    result
}

/// The result of an operation that `f` flows into the tvar it is given: the tvar, resolved to it
/// (see `resolve_result`). `None` if the operation gives nothing.
fn flowed<'cx>(
    cx: &mut Context<'cx>,
    reason: &Reason,
    f: impl FnOnce(&mut Context<'cx>, &Type) -> Result<(), FlowJsException>,
) -> Result<Option<Type>, FlowJsException> {
    let tvar = flow_typing_tvar::mk_where(cx, reason.dupe(), f)?;
    Ok(resolve_result(cx, &tvar).then_some(tvar))
}

/// Resolves `tvar`, which an operation flowed its result into, to that result, as
/// `mk_type_destructor` resolves an evaluation, so that the walk leaves no unresolved tvar
/// behind. A result that mentions unresolved tvars is left as it is: those can still give it more,
/// and are not the walk's to resolve. Says whether there is a result.
fn resolve_result<'cx>(cx: &mut Context<'cx>, tvar: &Type) -> bool {
    let Some(node) = type_util::constraint_node_id(tvar) else {
        return true;
    };
    let lowers = flow_js_utils::possible_types(cx, node.id());
    if (lowers.iter()).any(|t| flow_js_utils::tvar_visitors::has_unresolved_tvars(cx, t)) {
        return true;
    }
    tvar_resolver::resolve(cx, tvar_resolver::default_no_lowers, true, tvar);
    !lowers.is_empty()
}

/// `reverse_obj_spread` of `l`, with the result of each step resolved.
fn reverse_spread<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    todo_rev: &flow_data_structure_wrapper::list::FlowOcamlList<object::spread::Operand>,
    acc: &flow_data_structure_wrapper::list::FlowOcamlList<object::spread::AccElement>,
    l: &Type,
    literal: ObjLiteral,
) -> Result<Option<Type>, FlowJsException> {
    let mut t = l.dupe();
    for rest in spread_reversal_rests(cx, reason, todo_rev, acc, literal) {
        match flowed(cx, reason, |cx, tout| {
            flow_spread_reversal_rest(cx, env, reason, &t, rest, tout)
        })? {
            Some(next) => t = next,
            None => return Ok(None),
        }
    }
    Ok(Some(t))
}

/// `reverse_obj_kit_rest` of `l`, resolved.
fn reverse_rest<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    t_rest: &Type,
    l: &Type,
) -> Result<Option<Type>, FlowJsException> {
    flowed(cx, reason, |cx, tout| {
        flow_rest_reversal(cx, env, reason, t_rest, l, false, tout)
    })
}

/// `reverse_component_check_config` of `l`, resolved.
fn reverse_config<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    pmap: &properties::PropertiesMap,
    l: &Type,
) -> Result<Option<Type>, FlowJsException> {
    if pmap.is_empty() {
        return Ok(Some(l.dupe()));
    }
    let rest = component_config_rest(cx, reason, pmap, false);
    flowed(cx, reason, |cx, tout| {
        flow_spread_reversal_rest(cx, env, reason, l, rest, tout)
    })
}

fn is_mixed_or_any(t: &Type) -> bool {
    match t.deref() {
        TypeInner::AnyT(..) => true,
        TypeInner::DefT(_, d) => matches!(d.deref(), DefTInner::MixedT(_)),
        _ => false,
    }
}

/// Whether the destructor `d` gives its operand back, up to read-only-ness and optionality.
fn passes_through(d: &Destructor) -> bool {
    match d {
        Destructor::ReadOnlyType
        | Destructor::PartialType
        | Destructor::RequiredType
        | Destructor::ExactType
        | Destructor::ReactDRO(_) => true,
        Destructor::ReactCheckComponentConfig { props } => props.is_empty(),
        _ => false,
    }
}

/// Creates a marker for `tparam`: a `GenericT` with a fresh name, a renaming that no type
/// parameter in the source can have.
/// A marker is rigid and has no bound: a conditional type over it is decided only when the branch
/// holds whatever the type argument.
fn mk_marker<'cx>(cx: &mut Context<'cx>, tparam: &TypeParam) -> (SubstName, Type) {
    let name = SubstName::id(mk_id() as i32, tparam.name.string_of_subst_name().dupe());
    let id = cx.make_generic_id(name.dupe(), tparam.reason.loc());
    let t = Type::new(TypeInner::GenericT(Box::new(GenericTData {
        reason: tparam.reason.dupe(),
        name: name.dupe(),
        bound: mixed_t::make(tparam.reason.dupe()),
        no_infer: false,
        id,
    })));
    (name, t)
}

fn subst<'cx>(cx: &mut Context<'cx>, map: &FlowOrdMap<SubstName, Type>, t: &Type) -> Type {
    type_subst::subst(
        cx,
        None,
        true,
        false,
        type_subst::Purpose::Normal,
        map,
        t.dupe(),
    )
}

/// The instance of a class whose `this` is `this`, as a subclass inherits it.
fn this_specialize<'cx>(cx: &mut Context<'cx>, d: &ThisInstanceTData, this: &Type) -> Type {
    let mut map = FlowOrdMap::new();
    map.insert(d.subst_name.dupe(), this.dupe());
    let instance = type_subst::subst_instance_type(
        cx,
        None,
        true,
        false,
        type_subst::Purpose::Normal,
        &map,
        &d.instance,
    );
    Type::new(TypeInner::DefT(
        d.reason.dupe(),
        DefT::new(DefTInner::InstanceT(Rc::new(instance))),
    ))
}

/// Substitutes the markers into a signature. `NoInfer<T>` becomes a placeholder, so it gives no
/// evidence.
fn subst_markers<'cx>(cx: &mut Context<'cx>, map: &FlowOrdMap<SubstName, Type>, t: &Type) -> Type {
    subst_markers_with_use_op(cx, None, map, t)
}

/// `subst_markers` that also gives the type applications and evaluations of `t` the use of the
/// operation, as instantiating the signature for the flow-based solve does.
fn subst_markers_with_use_op<'cx>(
    cx: &mut Context<'cx>,
    use_op: Option<UseOp>,
    map: &FlowOrdMap<SubstName, Type>,
    t: &Type,
) -> Type {
    type_subst::subst(
        cx,
        use_op,
        true,
        true,
        type_subst::Purpose::Normal,
        map,
        t.dupe(),
    )
}

/// Whether `t` is `void`, or `null` too for a maybe type.
fn is_nullish(t: &Type, maybe: bool) -> bool {
    matches!(t.deref(), TypeInner::DefT(_, d)
        if matches!(d.deref(), DefTInner::VoidT) || (maybe && matches!(d.deref(), DefTInner::NullT)))
}

/// Whether some members of a union are absorbed by a maybe (or optional) type, when subtyping
/// knows it from the optimized representation of the union (`quick_mem_enum`); otherwise it
/// compares the members one by one.
fn nullish_members<'cx>(
    cx: &mut Context<'cx>,
    reason: &Reason,
    rep: &union_rep::UnionRep,
    maybe: bool,
) -> Option<bool> {
    use union_rep::QuickMemResult;
    let mut mem = |t: Type| {
        union_rep::quick_mem_enum(
            cx,
            |_, t1, t2| type_util::quick_subtype(None::<&mut fn(&Type)>, t1, t2),
            &t,
            rep,
        )
    };
    let void = mem(void::why(reason.dupe()));
    let null = if maybe {
        mem(null::make(reason.dupe()))
    } else {
        QuickMemResult::No
    };
    match (void, null) {
        (QuickMemResult::No, QuickMemResult::No) => Some(false),
        (QuickMemResult::Yes | QuickMemResult::No, QuickMemResult::Yes | QuickMemResult::No) => {
            Some(true)
        }
        _ => None,
    }
}

/// Whether `t` is a literal of a value, which `update_lit_type_from_annot` keeps precise.
fn is_value_literal(t: &Type) -> bool {
    match t.deref() {
        TypeInner::DefT(_, d) => matches!(
            d.deref(),
            DefTInner::SingletonStrT {
                from_annot: false,
                ..
            } | DefTInner::SingletonNumT {
                from_annot: false,
                ..
            } | DefTInner::SingletonBoolT {
                from_annot: false,
                ..
            } | DefTInner::SingletonBigIntT {
                from_annot: false,
                ..
            }
        ),
        TypeInner::TemplateLiteralT {
            from_annot: false, ..
        } => true,
        _ => false,
    }
}

fn is_placeholder(t: &Type) -> bool {
    matches!(
        t.deref(),
        TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
            kind: ImplicitInstantiationTvarKind::Placeholder,
            ..
        })
    )
}

/// Whether `l` and `u` are the same opaque type, which subtyping compares by its type arguments.
fn same_nominal(l: &Type, u: &Type) -> bool {
    matches!(
        (l.deref(), u.deref()),
        (TypeInner::NominalT { nominal_type: a, .. }, TypeInner::NominalT { nominal_type: b, .. })
            if a.nominal_id == b.nominal_id
    )
}

/// The underlying type of an opaque type `t` in the file that defines it, where subtyping sees it.
fn local_underlying(cx: &Context<'_>, t: &Type) -> Option<Type> {
    match t.deref() {
        TypeInner::NominalT { nominal_type, .. }
            if let nominal::Id::UserDefinedOpaqueTypeId(
                box nominal::UserDefinedOpaqueTypeIdData(id, _),
            ) = &nominal_type.nominal_id
                && id.0.source() == Some(cx.file())
                && let nominal::UnderlyingT::OpaqueWithLocal { t } = &nominal_type.underlying_t =>
        {
            Some(t.dupe())
        }
        _ => None,
    }
}

/// A property as subtyping compares it with an indexer: an optional field with its inner type.
fn field_for_indexer(p: &Property) -> PropertyType {
    match p.deref() {
        PropertyInner::Field(f) if let TypeInner::OptionalT { type_, .. } = f.type_.deref() => {
            PropertyType::OrdinaryField {
                type_: type_.dupe(),
                polarity: f.polarity,
            }
        }
        _ => property::property_type(p),
    }
}

/// The variance subtyping compares a type argument at: an unannotated type parameter is invariant,
/// but covariant in TypeScript (`type_app_variance_check`).
fn unannotated_variance(polarity: Option<Polarity>, strictness: TypeStrictnessKind) -> Polarity {
    match polarity {
        Some(p) => p,
        None if strictness.is_typescript_loose() => Polarity::Positive,
        None => Polarity::Neutral,
    }
}

/// The coarse shape of a type, used to pick the members of a union that an argument can match.
#[derive(PartialEq, Eq)]
enum Head {
    Num,
    Str,
    Bool,
    BigInt,
    Null,
    Void,
    Symbol,
    Fun,
    Obj,
    Arr,
    Inst(ALocId),
    Class,
    Other,
}

impl Head {
    /// Whether a type of this head may be a subtype of one of head `other`: a primitive only of
    /// the same primitive, a structure only of a structure.
    fn may_match(&self, other: &Head) -> bool {
        let structure = |h: &Head| {
            matches!(
                h,
                Head::Fun | Head::Obj | Head::Arr | Head::Inst(_) | Head::Class
            )
        };
        match (self, other) {
            (Head::Other, _) | (_, Head::Other) => true,
            (a, b) if structure(a) => structure(b),
            (a, b) => a == b,
        }
    }
}

/// A pair that the walk compares structurally, for the nesting limit.
#[derive(Clone, PartialEq)]
enum Nesting {
    /// A pair of classes, `None` for an object.
    Classes(Option<ALocId>, Option<ALocId>),
    /// A pair of type aliases, compared by their expansions.
    Aliases(poly::Id, poly::Id),
}

struct Walker<'e> {
    env: &'e FlowJsEnv,
    /// The level of the constraint cache that keeps the entries of the solve's flows without
    /// errors.
    preserve: usize,
    /// The location of the call being solved, which decides how a solution is generalized.
    call_loc: ALoc,
    vars: BTreeMap<SubstName, Bounds>,
    depth: u32,
    seen: BTreeMap<(usize, usize), (Type, Type)>,
    /// The changes to `seen` in order, so that a snapshot is restored by undoing those made since.
    seen_log: Vec<SeenChange>,
    mentions_cache: BTreeMap<usize, (Type, bool)>,
    /// The pairs walked so far, including those of walks that were undone.
    steps: usize,
    /// Whether an invariant position records a lower and an upper bound rather than an exact one.
    soft_exact: bool,
    /// The expansions of the type applications met so far, so that an application met again is
    /// expanded to the same type, which the walk has seen.
    expand_cache: std::cell::RefCell<BTreeMap<usize, (Type, Type)>>,
    /// The type applications expanded above, by the address of their expansion.
    expanded_from: std::cell::RefCell<BTreeMap<usize, Type>>,
    /// Pairs whose evidence depends on the solution of other markers (a conditional type whose
    /// check type mentions an unsolved marker), walked again after the other arguments.
    deferred: Vec<(Type, Type)>,
    second_pass: bool,
    /// Spreads of a marker against an object whose other operands have markers inside, walked
    /// again at the end of the walk: if the marker has evidence by then, the spread is evaluated
    /// with it and walked against the object.
    pending_spreads: Vec<(Type, Type)>,
    spreads_pass: bool,
    /// The pairs that the walk is comparing structurally, with how deeply their type arguments
    /// nest on each side.
    nesting: Vec<(Nesting, usize, usize)>,
    /// The defaults of the markers' type parameters, for fixing a marker without evidence.
    defaults: BTreeMap<SubstName, Type>,
    /// Set while the return type is walked against the hint.
    in_hint: bool,
    /// Set while the solution of a type parameter is walked against its bound.
    in_bound: bool,
    /// The type parameters the markers stand for.
    tparams: BTreeMap<SubstName, TypeParam>,
    /// The pairs `l <: [...prefix, ...T]` met, for the literals they need precise.
    spread_tuple_pairs: Vec<(Type, Type)>,
}

/// The arguments of a call from its first spread of unknown length on.
struct Tail {
    /// The types of the elements, each of which may be at any position.
    elems: Vec<Type>,
    /// The spread array, when it is all there is.
    array: Option<Type>,
}

/// Whether a type is written with a nullish member, as subtyping tells for the supertype of an
/// opaque type against a maybe or optional type.
fn has_syntactic_nullish_member(t: &Type) -> bool {
    match t.deref() {
        TypeInner::MaybeT(..) | TypeInner::OptionalT { .. } => true,
        TypeInner::UnionT(_, rep) => rep.members_iter().any(has_syntactic_nullish_member),
        TypeInner::DefT(_, d) => matches!(d.deref(), DefTInner::NullT | DefTInner::VoidT),
        _ => false,
    }
}

/// The type of the arguments a rest parameter receives when a spread of unknown length is among
/// them: an array of their elements.
fn mk_array(reason: &Reason, ts: Vec<Type>) -> Type {
    let elem_t = type_util::union_of_ts(reason.dupe(), ts, None);
    Type::new(TypeInner::DefT(
        reason.dupe(),
        DefT::new(DefTInner::ArrT(Rc::new(ArrType::ArrayAT(Box::new(
            ArrayATData {
                react_dro: None,
                elem_t,
                tuple_view: None,
                strictness_kind: TypeStrictnessKind::Flow,
                literal: ArrLiteral::Literal,
                fresh: true,
            },
        ))))),
    ))
}

/// The type of the arguments a rest parameter receives: an array with a tuple view, as the call
/// gives it.
fn mk_tuple(reason: &Reason, ts: Vec<Type>, literal: bool) -> Type {
    let n = ts.len() as i32;
    let elements: Vec<TupleElement> = ts
        .iter()
        .map(|t| {
            type_util::mk_tuple_element(
                type_util::reason_of_t(t).dupe(),
                t.dupe(),
                None,
                false,
                Polarity::Neutral,
            )
        })
        .collect();
    let elem_t = type_util::union_of_ts(reason.dupe(), ts, None);
    Type::new(TypeInner::DefT(
        reason.dupe(),
        DefT::new(DefTInner::ArrT(Rc::new(ArrType::ArrayAT(Box::new(
            ArrayATData {
                react_dro: None,
                elem_t,
                tuple_view: Some(TupleView {
                    kind: TupleViewKind::RestArguments,
                    elements: elements.into(),
                    arity: (n, n),
                    inexact: false,
                    rest: None,
                }),
                strictness_kind: TypeStrictnessKind::Flow,
                literal: ArrLiteral::of_literal(literal),
                fresh: true,
            },
        ))))),
    ))
}

/// A change to `seen`, which `restore` undoes.
enum SeenChange {
    Added((usize, usize)),
    Removed((usize, usize), (Type, Type)),
}

/// The state of a walk, to try a member of a union and undo it.
struct Snapshot {
    vars: BTreeMap<SubstName, Bounds>,
    seen_len: usize,
    deferred: Vec<(Type, Type)>,
    pending_spreads: Vec<(Type, Type)>,
    spread_tuple_pairs: Vec<(Type, Type)>,
}

/// Finds the markers in a type. An instance has them only through its type arguments, so its
/// members (often a large library class) are not visited.
struct MarkerVisitor<'a> {
    names: &'a BTreeMap<SubstName, Bounds>,
    /// For `mentions`, which stops at the first marker: whether each type visited mentions one.
    cache: Option<&'a mut BTreeMap<usize, (Type, bool)>>,
    found: BTreeSet<SubstName>,
    mentions: bool,
}

impl TypeVisitor<()> for MarkerVisitor<'_> {
    fn type_<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, _acc: (), t: &Type) {
        if let Some(cache) = &self.cache {
            if self.mentions {
                return;
            }
            if let Some((_, found)) = cache.get(&addr(t)) {
                self.mentions = *found;
                return;
            }
        }
        match t.deref() {
            TypeInner::GenericT(box GenericTData { name, .. }) if self.names.contains_key(name) => {
                self.found.insert(name.dupe());
                self.mentions = true;
            }
            TypeInner::DefT(_, d)
                if let DefTInner::InstanceT(i) = d.deref()
                    && i.inst.class_name.is_some()
                    && !matches!(
                        i.inst.inst_kind,
                        InstanceKind::InterfaceKind { inline: true, .. }
                    ) =>
            {
                for (_, _, t, _) in i.inst.type_args.iter() {
                    self.type_(cx, pole, (), t);
                }
            }
            TypeInner::ThisInstanceT(d) => {
                for (_, _, t, _) in d.instance.inst.type_args.iter() {
                    self.type_(cx, pole, (), t);
                }
            }
            _ => type_visitor::type_default(self, cx, pole, (), t),
        }
        // Unless a marker was found, the whole of `t` was visited.
        if let Some(cache) = &mut self.cache {
            cache.insert(addr(t), (t.dupe(), self.mentions));
        }
    }
}

/// Why `method_type_of` finds no method.
enum NoMethod {
    NotAnObject,
    Static,
    OfFunction,
    OfGenericValue,
    NotReadable,
}

impl NoMethod {
    fn why(&self) -> &'static str {
        match self {
            NoMethod::NotAnObject => "receiver is not an object",
            NoMethod::Static => "static method",
            NoMethod::OfFunction => "method of a function",
            NoMethod::OfGenericValue => "method of a generic value",
            NoMethod::NotReadable => "method is not readable",
        }
    }
}

/// What the reversal of a destructor bounds.
enum ReversalTarget {
    Marker(SubstName),
    /// An operand of a spread that is a spread with markers inside.
    Spread(Type),
}

/// The address of a type, to key caches on: types are compared by identity, and the cache keeps
/// each type it keys on alive.
fn addr(t: &Type) -> usize {
    std::ptr::from_ref::<TypeInner>(t) as usize
}

impl<'e> Walker<'e> {
    fn new(
        env: &'e FlowJsEnv,
        preserve: usize,
        call_loc: ALoc,
        names: impl IntoIterator<Item = SubstName>,
    ) -> Self {
        Walker {
            env,
            preserve,
            call_loc,
            vars: names.into_iter().map(|n| (n, Bounds::default())).collect(),
            depth: 0,
            seen: BTreeMap::new(),
            seen_log: Vec::new(),
            mentions_cache: BTreeMap::new(),
            steps: 0,
            soft_exact: false,
            expand_cache: std::cell::RefCell::new(BTreeMap::new()),
            expanded_from: std::cell::RefCell::new(BTreeMap::new()),
            deferred: Vec::new(),
            second_pass: false,
            pending_spreads: Vec::new(),
            spreads_pass: false,
            nesting: Vec::new(),
            defaults: BTreeMap::new(),
            tparams: BTreeMap::new(),
            spread_tuple_pairs: Vec::new(),
            in_hint: false,
            in_bound: false,
        }
    }

    /// A pair the walk can't read evidence from but that mentions markers: the solve keeps the
    /// evidence it has, and the walk goes on.
    fn give_up<'cx>(
        &self,
        cx: &mut Context<'cx>,
        what: &str,
        l: &Type,
        u: &Type,
    ) -> Result<(), FlowJsException> {
        log(cx, || {
            format!(
                "Implicit instantiation walk gives up ({}): {} <: {}",
                what,
                flow_typing_debug::dump_t(Some(1), cx, l),
                flow_typing_debug::dump_t(Some(1), cx, u)
            )
        });
        Ok(())
    }

    /// The deferred pairs, at the end of a walk, and then the pending spreads.
    fn walk_deferred<'cx>(&mut self, cx: &mut Context<'cx>) -> Result<(), FlowJsException> {
        self.walk_deferred_pairs(cx)?;
        let spreads = std::mem::take(&mut self.pending_spreads);
        self.spreads_pass = true;
        let result = self.walk_again(cx, spreads);
        self.spreads_pass = false;
        result
    }

    /// The deferred pairs, also before the generic values among the props of an object.
    fn walk_deferred_pairs<'cx>(&mut self, cx: &mut Context<'cx>) -> Result<(), FlowJsException> {
        self.second_pass = true;
        let deferred = std::mem::take(&mut self.deferred);
        self.walk_again(cx, deferred)
    }

    fn walk_again<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pairs: Vec<(Type, Type)>,
    ) -> Result<(), FlowJsException> {
        for (l, u) in pairs {
            let key = (addr(&l), addr(&u));
            if let Some(pair) = self.seen.remove(&key) {
                self.seen_log.push(SeenChange::Removed(key, pair));
            }
            self.walk(cx, &l, &u)?;
        }
        Ok(())
    }

    fn marker_of(&self, t: &Type) -> Option<SubstName> {
        match t.deref() {
            TypeInner::GenericT(box GenericTData { name, .. }) if self.vars.contains_key(name) => {
                Some(name.dupe())
            }
            _ => None,
        }
    }

    /// The markers that occur in `t`.
    fn markers_in<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> BTreeSet<SubstName> {
        let mut visitor = MarkerVisitor {
            names: &self.vars,
            cache: None,
            found: BTreeSet::new(),
            mentions: false,
        };
        visitor.type_(cx, Polarity::Positive, (), t);
        visitor.found
    }

    /// Whether `t` mentions a marker. Each type the check visits is remembered, so that the walk,
    /// which asks again at every level, visits each type once.
    fn mentions<'cx>(&mut self, cx: &mut Context<'cx>, t: &Type) -> bool {
        if let Some((_, found)) = self.mentions_cache.get(&addr(t)) {
            return *found;
        }
        let mut visitor = MarkerVisitor {
            names: &self.vars,
            cache: Some(&mut self.mentions_cache),
            found: BTreeSet::new(),
            mentions: false,
        };
        visitor.type_(cx, Polarity::Positive, (), t);
        visitor.mentions
    }

    /// A bound as subtyping would see it: a `this`-abstracted instance is fixed first.
    fn normalize<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> Type {
        match t.deref() {
            TypeInner::ThisInstanceT(d) => flow_js_utils::fix_this_instance(
                cx,
                d.reason.dupe(),
                d.reason.dupe(),
                &d.instance,
                d.is_this,
                d.subst_name.dupe(),
            ),
            _ => t.dupe(),
        }
    }

    fn add_lower<'cx>(&mut self, cx: &mut Context<'cx>, name: &SubstName, t: &Type) {
        self.add_lower_at(cx, name, t, Priority::Direct);
    }

    fn add_lower_at<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        name: &SubstName,
        t: &Type,
        priority: Priority,
    ) {
        if flow_js_utils::tvar_visitors::has_placeholders(cx, t) {
            // As in the flow-based solve, the members of a union are bounds one by one, so the
            // members that are placeholders drop out.
            if let TypeInner::UnionT(r, rep) = self.resolve(cx, t).deref() {
                let members: Vec<Type> = rep
                    .members_iter()
                    .filter(|m| !flow_js_utils::tvar_visitors::has_placeholders(cx, m))
                    .duped()
                    .collect();
                if !members.is_empty() && members.len() < rep.members_iter().count() {
                    let t = type_util::union_of_ts(r.dupe(), members, None);
                    self.add_lower_at(cx, name, &t, priority);
                }
            }
            return;
        }
        // An expansion that reaches a marker is recorded as the type application it expanded, which
        // later checks compare by its type arguments rather than by expanding it again.
        let mut t = t.dupe();
        while let Some(app) = self.expanded_from.borrow().get(&addr(&t)).map(Dupe::dupe) {
            t = app;
        }
        let t = &self.normalize(cx, &t);
        log(cx, || {
            format!(
                "Implicit instantiation walk: {} :> {}",
                name.string_of_subst_name(),
                flow_typing_debug::dump_t(Some(2), cx, t)
            )
        });
        let bounds = self.vars.get_mut(name).expect("marker should have bounds");
        add_bound(&mut bounds.lower, t, priority);
    }

    fn add_exact<'cx>(&mut self, cx: &mut Context<'cx>, name: &SubstName, t: &Type) {
        if flow_js_utils::tvar_visitors::has_placeholders(cx, t) {
            return;
        }
        if self.soft_exact {
            self.add_lower_at(cx, name, t, Priority::Soft);
            self.add_upper_at(cx, name, t, Priority::Soft);
            return;
        }
        log(cx, || {
            format!(
                "Implicit instantiation walk: {} == {}",
                name.string_of_subst_name(),
                flow_typing_debug::dump_t(Some(2), cx, t)
            )
        });
        let bounds = self.vars.get_mut(name).expect("marker should have bounds");
        if !bounds.exact.iter().any(|b| b.ptr_eq(t)) {
            bounds.exact.push(t.dupe());
        }
    }

    /// An invariant position: unify a bare marker with a type that mentions no marker, as the
    /// flow-based solve does, and otherwise walk both ways.
    fn walk_invariant<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        l: &Type,
        u: &Type,
    ) -> Result<(), FlowJsException> {
        if let Some(name) = self.marker_of(u)
            && !self.mentions(cx, l)
            && !is_placeholder(l)
        {
            self.add_exact(cx, &name, l);
            return Ok(());
        }
        if let Some(name) = self.marker_of(l)
            && !self.mentions(cx, u)
            && !is_placeholder(u)
        {
            self.add_exact(cx, &name, u);
            return Ok(());
        }
        // As unification unifies two function types with as many parameters receiver by receiver
        // and parameter by parameter, so that a marker met there is exact.
        let (lr, ur) = (self.resolve(cx, l), self.resolve(cx, u));
        if let (TypeInner::DefT(_, ld), TypeInner::DefT(_, ud)) = (lr.deref(), ur.deref())
            && let (DefTInner::FunT(_, lf), DefTInner::FunT(_, uf)) = (ld.deref(), ud.deref())
            && lf.type_guard.is_none()
            && uf.type_guard.is_none()
            && lf.params.len() == uf.params.len()
        {
            let lthis = type_util::this_for_unification(lf);
            let uthis = type_util::this_for_unification(uf);
            self.walk_invariant(cx, &lthis, &uthis)?;
            for (FunParam(_, lt), FunParam(_, ut)) in lf.params.iter().zip(uf.params.iter()) {
                self.walk_invariant(cx, lt, ut)?;
            }
            return self.walk_invariant(cx, &lf.return_t, &uf.return_t);
        }
        self.walk(cx, l, u)?;
        self.walk(cx, u, l)
    }

    fn add_upper<'cx>(&mut self, cx: &mut Context<'cx>, name: &SubstName, t: &Type) {
        self.add_upper_at(cx, name, t, Priority::Direct);
    }

    fn add_upper_at<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        name: &SubstName,
        t: &Type,
        priority: Priority,
    ) {
        if flow_js_utils::tvar_visitors::has_placeholders(cx, t) {
            return;
        }
        log(cx, || {
            format!(
                "Implicit instantiation walk: {} <: {}",
                name.string_of_subst_name(),
                flow_typing_debug::dump_t(Some(2), cx, t)
            )
        });
        let priority = match priority {
            Priority::Direct if self.in_bound => Priority::Bound,
            Priority::Reversal if self.in_bound => Priority::BoundReversal,
            priority => priority,
        };
        let bounds = self.vars.get_mut(name).expect("marker should have bounds");
        add_bound(&mut bounds.upper, t, priority);
    }

    /// Follows tvars and annotations to the type they stand for. An unresolved tvar stands for the
    /// union of what has flowed into it so far.
    fn resolve<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> Type {
        let mut t = t.dupe();
        for _ in 0..16 {
            let next = match t.deref() {
                TypeInner::AnnotT(_, inner, _) => inner.dupe(),
                TypeInner::OpenT(tvar) => {
                    let id = tvar.id() as i32;
                    match cx.find_constraints(id).1 {
                        constraint::Constraints::Resolved(r) => r,
                        constraint::Constraints::FullyResolved(s) => {
                            cx.force_fully_resolved_tvar(&s)
                        }
                        constraint::Constraints::Unresolved(_) => {
                            let ts = flow_js_utils::possible_types(cx, id);
                            if ts.is_empty() {
                                return t;
                            }
                            type_util::union_of_ts(type_util::reason_of_t(&t).dupe(), ts, None)
                        }
                    }
                }
                _ => return t,
            };
            t = next;
        }
        t
    }

    /// Walks a position of the given variance. An invariant one is unified, as subtyping does.
    fn walk_by_polarity<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        l: &Type,
        u: &Type,
        polarity: Polarity,
    ) -> Result<(), FlowJsException> {
        match polarity {
            Polarity::Positive => self.walk(cx, l, u),
            Polarity::Negative => self.walk(cx, u, l),
            Polarity::Neutral => self.walk_invariant(cx, l, u),
        }
    }

    /// Records what `l <: u` says about the markers.
    fn walk<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        l: &Type,
        u: &Type,
    ) -> Result<(), FlowJsException> {
        if self.depth > MAX_DEPTH {
            return self.give_up(cx, "depth limit", l, u);
        }
        if let Some(name) = self.marker_of(u) {
            if self.marker_of(l).is_none() {
                self.add_lower(cx, &name, l);
            }
            return Ok(());
        }
        if let Some(name) = self.marker_of(l) {
            self.add_upper(cx, &name, u);
            return Ok(());
        }
        // `empty` is a subtype of anything, which subtyping finds at once (`empty_success`) for
        // anything but a type variable: it gives evidence only to a bare marker.
        if matches!(l.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::EmptyT)) {
            return Ok(());
        }
        if is_placeholder(l) || is_placeholder(u) {
            return Ok(());
        }
        if !self.mentions(cx, u) && !self.mentions(cx, l) {
            return Ok(());
        }
        let key = (addr(l), addr(u));
        if self.seen.contains_key(&key) {
            return Ok(());
        }
        self.seen.insert(key, (l.dupe(), u.dupe()));
        self.seen_log.push(SeenChange::Added(key));
        // A walk that keeps finding new pairs, as speculation over recursive generic interfaces
        // can, stops with the evidence it has, as the flow-based solve stops at its own limits.
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return self.give_up(cx, "step limit", l, u);
        }
        self.depth += 1;
        let result = self.walk_structural(cx, l, u);
        self.depth -= 1;
        result
    }

    fn walk_structural<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        l: &Type,
        u: &Type,
    ) -> Result<(), FlowJsException> {
        let l = self.resolve(cx, l);
        let u = self.resolve(cx, u);
        log(cx, || {
            format!(
                "Implicit instantiation walk: {}{} ~> {}",
                "  ".repeat(self.depth as usize),
                flow_typing_debug::dump_t(Some(2), cx, &l),
                flow_typing_debug::dump_t(Some(2), cx, &u),
            )
        });
        if self.marker_of(&u).is_some() || self.marker_of(&l).is_some() {
            return self.walk(cx, &l, &u);
        }
        // A type variable that nothing has flowed into yet, such as the result of a hint being
        // computed, only records what flows into it, which says nothing about the markers.
        if matches!(u.deref(), TypeInner::OpenT(_)) {
            return Ok(());
        }
        match (l.deref(), u.deref()) {
            // A module namespace is the object of its values.
            (TypeInner::NamespaceT(ns), _) => {
                let values = ns.values_type.dupe();
                self.walk(cx, &values, &u)
            }
            // An element of an `any` component renders `any`, which flows on as `any`.
            (TypeInner::DefT(_, d), _)
                if let DefTInner::RendersT(form) = d.deref()
                    && let CanonicalRendersForm::StructuralRenders {
                        renders_variant: RendersVariant::RendersNormal,
                        renders_structural_type: any,
                    } = form.deref()
                    && matches!(any.deref(), TypeInner::AnyT(..)) =>
            {
                let any = any.dupe();
                self.propagate_any(cx, &any, &u, true);
                Ok(())
            }
            // A primitive renders itself: against `renders T`, it goes to `T`.
            (TypeInner::DefT(_, ld), TypeInner::DefT(_, ud))
                if is_atomic(ld)
                    && let DefTInner::RendersT(form) = ud.deref()
                    && let CanonicalRendersForm::StructuralRenders {
                        renders_variant: RendersVariant::RendersNormal,
                        renders_structural_type: t,
                    } = form.deref()
                    && self.marker_of(t).is_some() =>
            {
                let t = t.dupe();
                self.walk(cx, &l, &t)
            }
            // A rigid type parameter is matched by itself among the members of a union, before its
            // bound is compared.
            (TypeInner::GenericT(box GenericTData { id, .. }), TypeInner::UnionT(_, rep))
                if rep.members_iter().any(|m| {
                    matches!(
                        self.resolve(cx, m).deref(),
                        TypeInner::GenericT(box GenericTData { id: other, .. }) if other.equal(id)
                    )
                }) =>
            {
                Ok(())
            }
            // `any` reaches the markers inside a destructor, as it flows into its operands.
            (TypeInner::AnyT(..), TypeInner::EvalT { .. }) => {
                self.propagate_any(cx, &l, &u, true);
                Ok(())
            }
            (TypeInner::EvalT { .. }, TypeInner::AnyT(..)) => {
                self.propagate_any(cx, &u, &l, false);
                Ok(())
            }
            // A destructor that its operand passes through (`Readonly<T>`, the config of a component
            // with only a rest parameter), with markers, against another destructor: its operand
            // meets the other destructor.
            (TypeInner::EvalT { defer_use_t, .. }, TypeInner::EvalT { .. })
                if passes_through(&defer_use_t.2) && self.mentions(cx, &l) =>
            {
                self.walk_from_eval(cx, &l, &u)
            }
            (_, TypeInner::EvalT { .. }) => self.walk_to_eval(cx, &l, &u),
            (TypeInner::EvalT { .. }, _) => self.walk_from_eval(cx, &l, &u),
            // As in subtyping, an opaque type stands for its underlying type in the file that
            // defines it, unless the other side is the same opaque type.
            (TypeInner::NominalT { .. }, _)
                if !same_nominal(&l, &u)
                    && let Some(t) = local_underlying(cx, &l) =>
            {
                self.walk(cx, &t, &u)
            }
            (_, TypeInner::NominalT { .. })
                if !same_nominal(&l, &u)
                    && let Some(t) = local_underlying(cx, &u) =>
            {
                self.walk(cx, &l, &t)
            }
            // The opaque type of an alias with a custom error is transparent, as in subtyping.
            (
                TypeInner::NominalT {
                    nominal_type: a, ..
                },
                _,
            ) if let nominal::UnderlyingT::CustomError(box nominal::CustomErrorData {
                t, ..
            }) = &a.underlying_t =>
            {
                let t = t.dupe();
                self.walk(cx, &t, &u)
            }
            (
                _,
                TypeInner::NominalT {
                    nominal_type: b, ..
                },
            ) if let nominal::UnderlyingT::CustomError(box nominal::CustomErrorData {
                t, ..
            }) = &b.underlying_t =>
            {
                let t = t.dupe();
                self.walk(cx, &l, &t)
            }
            // As in subtyping, an application above a union is expanded before the union is split,
            // so that its members meet the expanded type, not the application itself.
            (TypeInner::UnionT(..), TypeInner::TypeAppT(_) | TypeInner::ThisTypeAppT(_))
                if let Some(expanded) = self.expand_typeapp(cx, &u) =>
            {
                self.walk(cx, &l, &expanded)
            }
            // As subtyping compares a union with a maybe or optional type as a subset: a union of
            // types that are not null or void meets the inner type whole, and one with null or void
            // members meets it without them. The union is then one bound of a marker there, as in
            // the flow-based solve, rather than one bound per member.
            (
                TypeInner::UnionT(reason, rep),
                TypeInner::MaybeT(_, inner) | TypeInner::OptionalT { type_: inner, .. },
            ) if let Some(nullish) = nullish_members(
                cx,
                type_util::reason_of_t(&u),
                rep,
                matches!(u.deref(), TypeInner::MaybeT(..)),
            ) =>
            {
                let inner = inner.dupe();
                if nullish {
                    let maybe = matches!(u.deref(), TypeInner::MaybeT(..));
                    let filtered = flow_js_utils::remove_predicate_from_union(
                        reason.dupe(),
                        cx,
                        |t| is_nullish(t, maybe),
                        rep,
                    );
                    self.walk(cx, &filtered, &inner)
                } else {
                    self.walk(cx, &l, &inner)
                }
            }
            (TypeInner::UnionT(_, rep), _) => {
                let members: Vec<Type> = rep.members_iter().duped().collect();
                for m in members {
                    self.walk(cx, &m, &u)?;
                }
                Ok(())
            }
            // Null and void are absorbed; the rest is compared with the whole upper bound, so that it
            // can be evaluated or split first.
            (TypeInner::MaybeT(_, li), TypeInner::MaybeT(..)) => self.walk(cx, li, &u),
            (TypeInner::OptionalT { type_: li, .. }, TypeInner::OptionalT { .. }) => {
                self.walk(cx, li, &u)
            }
            // `any` fits each member of a union: see `propagate_any`.
            (TypeInner::AnyT(..), TypeInner::UnionT(..)) => {
                self.propagate_any(cx, &l, &u, true);
                Ok(())
            }
            // A union with `unknown` among its members: the speculative match of the flow-based
            // solve takes that member, which needs nothing of the markers.
            (_, TypeInner::UnionT(_, rep))
                if rep.members_iter().any(|m| {
                    matches!(
                        self.resolve(cx, m).deref(),
                        TypeInner::DefT(_, d)
                            if matches!(d.deref(), DefTInner::MixedT(MixedFlavor::MixedEverything))
                    )
                }) =>
            {
                Ok(())
            }
            // An application meets the applications of the same alias or class among the members
            // of a union, which it closely matches, and not the others: the first that it fits
            // with the markers open and whose evidence agrees with the evidence so far, and none if
            // no member does, as in the speculative match of the flow-based solve.
            // The flow-based solve expands an alias first, so its body meets the members.
            (TypeInner::TypeAppT(a), TypeInner::UnionT(_, rep))
                if !self.is_alias(cx, &a.type_)
                    && let Some(members) = self.same_typeapp_members(cx, &a.type_, rep) =>
            {
                // A member before them that `l` fits with the markers open is matched first, as
                // in the speculative match of the flow-based solve.
                for m in rep.members_iter() {
                    if members.iter().any(|s| s.ptr_eq(m)) {
                        break;
                    }
                    if self.fits_open(cx, &l, m)? {
                        return self.walk_to_union(cx, &l, rep.members_iter().duped().collect());
                    }
                }
                for m in &members {
                    if !self.fits_open(cx, &l, m)? {
                        continue;
                    }
                    let snapshot = self.snapshot();
                    self.walk(cx, &l, m)?;
                    if self.agrees_with(cx, &snapshot.vars)? {
                        return Ok(());
                    }
                    self.restore(snapshot);
                }
                Ok(())
            }
            // Nor does an application of a class meet an optional or maybe application of the same
            // class through its expansion: `null` and `void` play no part. (The flow-based solve
            // expands an alias first.)
            (
                TypeInner::TypeAppT(a),
                TypeInner::MaybeT(_, ui) | TypeInner::OptionalT { type_: ui, .. },
            ) if !self.is_alias(cx, &a.type_)
                && matches!(
                    self.resolve(cx, ui).deref(),
                    TypeInner::TypeAppT(b) if self.same_typeapp(cx, &a.type_, &b.type_).is_some()
                ) =>
            {
                let ui = self.resolve(cx, ui);
                self.walk(cx, &l, &ui)
            }
            // An alias or class below a union is expanded first, so that its members or its
            // shape pick the members of the union it matches. So is an alias below a maybe or
            // optional type, which absorbs the `null` and `void` it stands for.
            (TypeInner::TypeAppT(_) | TypeInner::ThisTypeAppT(_), TypeInner::UnionT(..))
                if let Some(expanded) = self.expand_typeapp(cx, &l) =>
            {
                self.walk(cx, &expanded, &u)
            }
            (
                TypeInner::TypeAppT(_) | TypeInner::ThisTypeAppT(_),
                TypeInner::MaybeT(..) | TypeInner::OptionalT { .. },
            ) if let Some(expanded) = self.expand_typeapp(cx, &l)
                && !matches!(expanded.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::InstanceT(_))) =>
            {
                self.walk(cx, &expanded, &u)
            }
            // As subtyping: an opaque type with a supertype without a nullish member flows to a
            // maybe or optional type through its inner type, and one with a union supertype to a
            // union as the union of the opaque type over each member of its supertype.
            (
                TypeInner::NominalT {
                    nominal_type: a, ..
                },
                TypeInner::MaybeT(_, inner) | TypeInner::OptionalT { type_: inner, .. },
            ) if let Some(sup) = &a.upper_t
                && !has_syntactic_nullish_member(sup) =>
            {
                let inner = inner.dupe();
                self.walk(cx, &l, &inner)
            }
            (
                TypeInner::NominalT {
                    reason,
                    nominal_type: a,
                },
                TypeInner::UnionT(..),
            ) if let Some(sup) = &a.upper_t
                && let members = self.flatten_aliases(cx, vec![sup.dupe()])
                && members.len() > 1 =>
            {
                for m in members {
                    let opaque = Type::new(TypeInner::NominalT {
                        reason: reason.dupe(),
                        nominal_type: Rc::new(NominalType::new(NominalTypeInner {
                            nominal_id: a.nominal_id.clone(),
                            underlying_t: a.underlying_t.clone(),
                            lower_t: a.lower_t.clone(),
                            upper_t: Some(m),
                            nominal_type_args: a.nominal_type_args.dupe(),
                        })),
                    });
                    self.walk(cx, &opaque, &u)?;
                }
                Ok(())
            }
            // An opaque type with a supertype flows to a union-like type through it.
            (
                TypeInner::NominalT {
                    nominal_type: a, ..
                },
                TypeInner::MaybeT(..) | TypeInner::OptionalT { .. } | TypeInner::UnionT(..),
            ) if let Some(sup) = &a.upper_t
                && matches!(
                    self.resolve(cx, sup).deref(),
                    TypeInner::MaybeT(..) | TypeInner::OptionalT { .. } | TypeInner::UnionT(..)
                ) =>
            {
                let sup = sup.dupe();
                self.walk(cx, &sup, &u)
            }
            (_, TypeInner::MaybeT(r, ui)) => match l.deref() {
                TypeInner::OptionalT { type_: li, .. } => self.walk(cx, li, &u),
                // A tvar without lower bounds yet: what it will hold, minus null and void.
                TypeInner::OpenT(_) => {
                    let non_maybe = flow_cache::eval::id(
                        cx,
                        l.dupe(),
                        TypeDestructorT::new(TypeDestructorTInner(
                            unknown_use(),
                            r.dupe(),
                            Rc::new(Destructor::NonMaybeType),
                        )),
                    );
                    let ui = ui.dupe();
                    self.walk(cx, &non_maybe, &ui)
                }
                // Null and void are absorbed, and the inner type sees `empty`, as in subtyping.
                _ if matches!(self.head(cx, &l), Head::Null | Head::Void) => {
                    let empty = empty_t::why(type_util::reason_of_t(&l).dupe());
                    let ui = ui.dupe();
                    self.walk(cx, &empty, &ui)
                }
                _ => {
                    let ui = ui.dupe();
                    self.walk(cx, &l, &ui)
                }
            },
            (_, TypeInner::OptionalT { type_: ui, .. }) => match l.deref() {
                TypeInner::MaybeT(r, li) => {
                    self.walk(cx, &null::make(r.dupe()), ui)?;
                    self.walk(cx, li, &u)
                }
                _ if matches!(self.head(cx, &l), Head::Void) => {
                    let empty = empty_t::why(type_util::reason_of_t(&l).dupe());
                    let ui = ui.dupe();
                    self.walk(cx, &empty, &ui)
                }
                _ => {
                    let ui = ui.dupe();
                    self.walk(cx, &l, &ui)
                }
            },
            // A maybe or optional lower bound is the union of its parts.
            (TypeInner::MaybeT(r, li), _) => {
                self.walk(cx, &null::make(r.dupe()), &u)?;
                self.walk(cx, &void::why(r.dupe()), &u)?;
                self.walk(cx, li, &u)
            }
            (
                TypeInner::OptionalT {
                    reason, type_: li, ..
                },
                _,
            ) => {
                self.walk(cx, &void::why(reason.dupe()), &u)?;
                self.walk(cx, li, &u)
            }
            (_, TypeInner::UnionT(_, rep)) => {
                let members: Vec<Type> = rep.members_iter().duped().collect();
                self.walk_to_union(cx, &l, members)
            }
            // Overloads against overloads: each overload of the upper bound against the one of the
            // lower bound as far from the last, or the first.
            (TypeInner::IntersectionT(_, lrep), TypeInner::IntersectionT(_, urep))
                if self.all_funs(cx, lrep) && self.all_funs(cx, urep) =>
            {
                let lmembers: Vec<Type> = lrep.members_iter().duped().collect();
                let umembers: Vec<Type> = urep.members_iter().duped().collect();
                let (ln, un) = (lmembers.len(), umembers.len());
                for (i, m) in umembers.iter().enumerate() {
                    let l = &lmembers[(ln + i).saturating_sub(un)];
                    self.walk(cx, l, m)?;
                }
                Ok(())
            }
            // An intersection of several bare markers says nothing about each.
            (_, TypeInner::IntersectionT(_, rep)) => {
                let members: Vec<Type> = rep.members_iter().duped().collect();
                let bare = members
                    .iter()
                    .filter(|m| self.marker_of(m).is_some())
                    .count();
                for m in members {
                    if bare > 1 && self.marker_of(&m).is_some() {
                        continue;
                    }
                    self.walk(cx, &l, &m)?;
                }
                Ok(())
            }
            // An application below an intersection is expanded first, so that an object type meets
            // the intersection as a whole.
            (TypeInner::IntersectionT(..), TypeInner::TypeAppT(_) | TypeInner::ThisTypeAppT(_))
                if let Some(e) = self.expand_typeapp(cx, &u) =>
            {
                self.walk(cx, &l, &e)
            }
            // An intersection of objects meets an object type through the first member that has
            // its required properties, fits it with the markers open, and whose evidence agrees,
            // as in speculation; otherwise
            // property by property, each from the first member that has it, as a property lookup
            // on an intersection does.
            (TypeInner::IntersectionT(_, rep), TypeInner::DefT(_, d))
                if let DefTInner::ObjT(uo) = d.deref()
                    && let Some(lp) = self.intersection_props(cx, rep) =>
            {
                let up = cx.find_props(uo.props_tmap.dupe());
                let required: Vec<Name> = up
                    .iter()
                    .filter(|(_, p)| {
                        property::read_t(p)
                            .is_some_and(|t| !matches!(t.deref(), TypeInner::OptionalT { .. }))
                    })
                    .map(|(name, _)| name.dupe())
                    .collect();
                let members: Vec<Type> = rep.members_iter().duped().collect();
                for m in &members {
                    let Some((mp, _)) = self.object_shape(cx, m) else {
                        continue;
                    };
                    if !required.iter().all(|name| mp.get(name).is_some())
                        || !self.fits_open(cx, m, &u)?
                    {
                        continue;
                    }
                    let snapshot = self.snapshot();
                    self.walk(cx, m, &u)?;
                    if self.agrees_with(cx, &snapshot.vars)? {
                        return Ok(());
                    }
                    self.restore(snapshot);
                }
                let ud = obj_type::get_dict_opt(&uo.flags.obj_kind).cloned();
                self.walk_props(cx, &lp, None, &up, ud.as_ref(), false, false)
            }
            // Any other intersection meets the other type through its first member that fits it
            // with the markers open and whose evidence agrees, as in speculation. A bare marker
            // among the members (here, in a contravariant position, as a parameter type) is a
            // candidate only if it is the only one.
            (TypeInner::IntersectionT(_, rep), _) => {
                let members: Vec<Type> = rep.members_iter().duped().collect();
                let bare = (members.iter())
                    .filter(|m| self.marker_through_identity(cx, m).is_some())
                    .count();
                for m in members {
                    if bare > 1 && self.marker_through_identity(cx, &m).is_some() {
                        continue;
                    }
                    if !self.fits_open(cx, &m, &u)? {
                        continue;
                    }
                    let snapshot = self.snapshot();
                    self.walk(cx, &m, &u)?;
                    if self.agrees_with(cx, &snapshot.vars)? {
                        return Ok(());
                    }
                    self.restore(snapshot);
                }
                Ok(())
            }
            (TypeInner::AnyT(..), _) => {
                self.propagate_any(cx, &l, &u, true);
                Ok(())
            }
            (_, TypeInner::AnyT(..)) => {
                self.propagate_any(cx, &u, &l, false);
                Ok(())
            }
            // As in subtyping, an application above a rigid type parameter is expanded before the
            // parameter is replaced by its bound, so that the parameter can match a marker in the
            // expansion. A bound that applies the same alias or class is compared argument by
            // argument instead.
            (TypeInner::GenericT(box GenericTData { bound, .. }), TypeInner::TypeAppT(b))
                if !self.bound_applies(cx, bound, &b.type_, 0)
                    && let Some(e) = self.expand_typeapp(cx, &u) =>
            {
                self.walk(cx, &l, &e)
            }
            (TypeInner::GenericT(box GenericTData { bound, .. }), _) => self.walk(cx, bound, &u),
            (TypeInner::TypeAppT(a), TypeInner::TypeAppT(b)) => {
                // Applications of the same alias or class are compared argument by argument
                // when they write as many arguments, as subtyping does (`ConcretizeTypeAppsT`);
                // otherwise by their expansions, where a default stands in for an argument left
                // out.
                if a.targs.len() == b.targs.len()
                    && let Some(polarities) = self.same_typeapp(cx, &a.type_, &b.type_)
                {
                    let ltargs = self.full_targs(cx, &a.type_, &a.targs);
                    let utargs = self.full_targs(cx, &b.type_, &b.targs);
                    let pairs: Vec<(Type, Type, Polarity)> = ltargs
                        .iter()
                        .zip(utargs.iter())
                        .zip(polarities)
                        .map(|((l, u), p)| (l.dupe(), u.dupe(), p))
                        .collect();
                    for (l, u, p) in pairs {
                        self.walk_by_polarity(cx, &l, &u, p)?;
                    }
                    Ok(())
                } else {
                    // An alias whose body applies the alias of the other side, such as
                    // `type MySpec<A> = Spec<?A, Props<A>>` against `Spec<R, P>`, stands for that
                    // application, which meets the other argument by argument. Their expansions
                    // come to the same, but the walk can't always take them apart: a spread of a
                    // union makes a union.
                    if let Some(e) = self.expand_typeapp(cx, &u)
                        && let e = self.resolve(cx, &e)
                        && matches!(e.deref(), TypeInner::TypeAppT(eb) if self.same_typeapp(cx, &a.type_, &eb.type_).is_some())
                    {
                        return self.walk(cx, &l, &e);
                    }
                    if let Some(e) = self.expand_typeapp(cx, &l)
                        && let e = self.resolve(cx, &e)
                        && matches!(e.deref(), TypeInner::TypeAppT(ea) if self.same_typeapp(cx, &ea.type_, &b.type_).is_some())
                    {
                        return self.walk(cx, &e, &u);
                    }
                    // Applications of aliases that are compared by their expansions are nested
                    // like classes: a recursive alias would otherwise unfold without end.
                    let la = self.resolve(cx, &a.type_);
                    let ua = self.resolve(cx, &b.type_);
                    match (la.deref(), ua.deref()) {
                        (TypeInner::DefT(_, ld), TypeInner::DefT(_, ud))
                            if let (DefTInner::PolyT(lp), DefTInner::PolyT(up)) =
                                (ld.deref(), ud.deref()) =>
                        {
                            let key = Nesting::Aliases(lp.id.dupe(), up.id.dupe());
                            let depths = (self.depth_of(cx, &l, 0), self.depth_of(cx, &u, 0));
                            self.nested(cx, key, depths, move |this, cx| {
                                this.walk_expansions(cx, &l, &u)
                            })
                        }
                        _ => self.walk_expansions(cx, &l, &u),
                    }
                }
            }
            (TypeInner::TypeAppT(_) | TypeInner::ThisTypeAppT(_), _) => {
                match self.expand_typeapp(cx, &l) {
                    Some(l) => self.walk(cx, &l, &u),
                    None => self.give_up(cx, "type application", &l, &u),
                }
            }
            (_, TypeInner::TypeAppT(_) | TypeInner::ThisTypeAppT(_)) => {
                match self.expand_typeapp(cx, &u) {
                    Some(u) => self.walk(cx, &l, &u),
                    None => self.give_up(cx, "type application", &l, &u),
                }
            }
            (TypeInner::ThisInstanceT(d), _) => {
                let l = flow_js_utils::fix_this_instance(
                    cx,
                    d.reason.dupe(),
                    d.reason.dupe(),
                    &d.instance,
                    d.is_this,
                    d.subst_name.dupe(),
                );
                self.walk(cx, &l, &u)
            }
            (_, TypeInner::ThisInstanceT(d)) => {
                let u = Type::new(TypeInner::DefT(
                    d.reason.dupe(),
                    DefT::new(DefTInner::InstanceT(Rc::new(d.instance.dupe()))),
                ));
                self.walk(cx, &l, &u)
            }
            (
                TypeInner::NominalT {
                    nominal_type: a, ..
                },
                TypeInner::NominalT {
                    nominal_type: b, ..
                },
            ) if a.nominal_id == b.nominal_id => {
                let pairs: Vec<(Type, Type, Polarity)> = a
                    .nominal_type_args
                    .iter()
                    .zip(b.nominal_type_args.iter())
                    .map(|((_, _, l, _), (_, _, u, p))| {
                        (l.dupe(), u.dupe(), p.unwrap_or(Polarity::Neutral))
                    })
                    .collect();
                for (l, u, p) in pairs {
                    self.walk_by_polarity(cx, &l, &u, p)?;
                }
                Ok(())
            }
            // An opaque type, such as a React element, against a renders type, and back.
            (TypeInner::NominalT { .. }, TypeInner::DefT(_, ud))
                if matches!(ud.deref(), DefTInner::RendersT(_)) =>
            {
                self.walk_renders(cx, &l, &u)
            }
            (TypeInner::DefT(_, ld), TypeInner::NominalT { .. })
                if matches!(ld.deref(), DefTInner::RendersT(_)) =>
            {
                self.walk_renders(cx, &l, &u)
            }
            (
                TypeInner::NominalT {
                    nominal_type: a, ..
                },
                _,
            ) => match &a.upper_t {
                Some(sup) => {
                    let sup = sup.dupe();
                    self.walk(cx, &sup, &u)
                }
                None => Ok(()),
            },
            // Elsewhere, a type flows to an opaque type through its lower bound.
            (
                _,
                TypeInner::NominalT {
                    nominal_type: b, ..
                },
            ) => match &b.lower_t {
                Some(lower) => {
                    let lower = lower.dupe();
                    self.walk(cx, &l, &lower)
                }
                None => Ok(()),
            },
            (TypeInner::DefT(_, ld), TypeInner::DefT(_, ud)) => self.walk_def(cx, &l, ld, &u, ud),
            // A string literal against a template literal type: the parts of the string that the
            // placeholders match go to them.
            (TypeInner::DefT(lr, ld), TypeInner::TemplateLiteralT { quasis, types, .. })
                if let DefTInner::SingletonStrT { value, .. } = ld.deref() =>
            {
                let (quasis, types) =
                    crate::template_literal_type::fold_concrete_placeholders(quasis, types);
                let crate::template_literal_type::MatchResult::Match(pairs) =
                    crate::template_literal_type::match_string_against_template(
                        value.as_str(),
                        &quasis,
                        &types,
                    )
                else {
                    return Ok(());
                };
                for (part, t) in pairs {
                    if !self.mentions(cx, &t) {
                        continue;
                    }
                    let part: FlowSmolStr = part.as_str().into();
                    let r = lr
                        .dupe()
                        .replace_desc(VirtualReasonDesc::RStringLit(part.dupe()));
                    let part_t = Type::new(TypeInner::DefT(
                        r,
                        DefT::new(DefTInner::SingletonStrT {
                            from_annot: true,
                            value: part,
                        }),
                    ));
                    self.walk(cx, &part_t, &t)?;
                }
                Ok(())
            }
            (TypeInner::TemplateLiteralT { .. }, _)
            | (_, TypeInner::TemplateLiteralT { .. })
            | (_, TypeInner::KeysT(..))
            | (TypeInner::KeysT(..), _) => self.give_up(cx, "unhandled pair", &l, &u),
            (TypeInner::DefT(_, ld), _) => match ld.deref() {
                DefTInner::PolyT(p) => self.walk_generic_value(cx, p, &u),
                _ => self.give_up(cx, "unhandled pair", &l, &u),
            },
            _ => self.give_up(cx, "unhandled pair", &l, &u),
        }
    }

    fn walk_def<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        l: &Type,
        ld: &DefT,
        u: &Type,
        ud: &DefT,
    ) -> Result<(), FlowJsException> {
        match (ld.deref(), ud.deref()) {
            // A generic upper bound is compared through its body, its type parameters rigid.
            (_, DefTInner::PolyT(up)) => {
                let body = up.t_out.dupe();
                self.walk(cx, l, &body)
            }
            (DefTInner::PolyT(p), _) => self.walk_generic_value(cx, p, u),
            (DefTInner::FunT(_, lf), DefTInner::FunT(_, uf)) => self.walk_fun(cx, lf, uf),
            (DefTInner::ObjT(lo), DefTInner::ObjT(uo)) => {
                self.walk_obj(cx, type_util::reason_of_t(l), lo, uo)
            }
            (DefTInner::FunT(..), DefTInner::ObjT(uo)) => match uo.call_t {
                Some(id) => {
                    let call = cx.find_call(id);
                    self.walk(cx, l, &call)
                }
                None => Ok(()),
            },
            // A function against an instance with a call signature, such as a callable interface:
            // the call signature, as in subtyping.
            (DefTInner::FunT(..), DefTInner::InstanceT(ui)) => {
                match (self.ancestors(cx, ui).into_iter()).find_map(|i| i.inst.inst_call_t) {
                    Some(id) => {
                        let call = cx.find_call(id);
                        self.walk(cx, l, &call)
                    }
                    None => Ok(()),
                }
            }
            (DefTInner::ObjT(lo), DefTInner::FunT(..)) => match lo.call_t {
                Some(id) => {
                    let call = cx.find_call(id);
                    self.walk(cx, &call, u)
                }
                None => Ok(()),
            },
            (DefTInner::InstanceT(li), DefTInner::InstanceT(ui)) => self.walk_inst(cx, li, u, ui),
            (DefTInner::InstanceT(li), DefTInner::ObjT(uo)) => {
                let key = Nesting::Classes(Some(li.inst.class_id.dupe()), None);
                let depths = (self.inst_depth(cx, li, 0), 0);
                let (li, uo) = (li.dupe(), uo.dupe());
                self.nested(cx, key, depths, move |this, cx| {
                    let lp = this.inst_props(cx, &li);
                    let ld = this.inst_dict(cx, &li);
                    let up = cx.find_props(uo.props_tmap.dupe());
                    let ud = obj_type::get_dict_opt(&uo.flags.obj_kind).cloned();
                    this.walk_props(cx, &lp, ld.as_ref(), &up, ud.as_ref(), false, false)
                })
            }
            // As subtyping compares an object with an interface as with the object type of the
            // interface, the call property meets the interface's call signature too.
            (DefTInner::ObjT(lo), DefTInner::InstanceT(ui))
                if matches!(ui.inst.inst_kind, InstanceKind::InterfaceKind { .. }) =>
            {
                let key = Nesting::Classes(None, Some(ui.inst.class_id.dupe()));
                let depths = (0, self.inst_depth(cx, ui, 0));
                let (lo, ui) = (lo.dupe(), ui.dupe());
                self.nested(cx, key, depths, move |this, cx| {
                    let lp = cx.find_props(lo.props_tmap.dupe());
                    let ld = obj_type::get_dict_opt(&lo.flags.obj_kind).cloned();
                    let up = this.inst_props(cx, &ui);
                    let ud = this.inst_dict(cx, &ui);
                    this.walk_props(
                        cx,
                        &lp,
                        ld.as_ref(),
                        &up,
                        ud.as_ref(),
                        true,
                        lo.literal.is_literal(),
                    )?;
                    let ucall = this
                        .ancestors(cx, &ui)
                        .into_iter()
                        .find_map(|i| i.inst.inst_call_t);
                    if let (Some(lc), Some(uc)) = (lo.call_t, ucall) {
                        let lc = cx.find_call(lc);
                        let uc = cx.find_call(uc);
                        this.walk(cx, &lc, &uc)?;
                    }
                    Ok(())
                })
            }
            (DefTInner::ArrT(la), DefTInner::ArrT(ua)) => self.walk_arr(cx, la, ua),
            // An array flows to an instance through its builtin class, as in subtyping.
            (DefTInner::ArrT(la), DefTInner::InstanceT(_)) => {
                let instance = self.array_instance(cx, l, la);
                self.walk(cx, &instance, u)
            }
            // An instance flows to an array type if its class extends the builtin one, and never
            // to a tuple type, as in subtyping.
            (DefTInner::InstanceT(_), DefTInner::ArrT(ua)) => match ua.deref() {
                ArrType::TupleAT(_) => Ok(()),
                _ => {
                    let instance = self.array_instance(cx, u, ua);
                    self.walk(cx, l, &instance)
                }
            },
            (
                DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. },
                DefTInner::InstanceT(ui),
            ) if matches!(ui.inst.inst_kind, InstanceKind::InterfaceKind { .. }) => {
                let reason = type_util::reason_of_t(l).dupe();
                let string =
                    FlowJs::get_builtin_type_with_env(cx, self.env, None, &reason, None, "String")?;
                self.walk(cx, &string, u)
            }
            // A class flows to a construct signature: its instance is what the signature returns.
            (DefTInner::ClassT(inst), DefTInner::InstanceT(ui))
                if let Some(ctor) = ui.inst.inst_construct_t =>
            {
                let ctor = cx.find_call(ctor);
                let ctor = self.resolve(cx, &ctor);
                let inst = inst.dupe();
                match ctor.deref() {
                    TypeInner::DefT(_, d) => match d.deref() {
                        DefTInner::FunT(_, uf) => {
                            let ret = uf.return_t.dupe();
                            self.walk(cx, &inst, &ret)
                        }
                        _ => self.give_up(cx, "construct signature", l, u),
                    },
                    _ => self.give_up(cx, "construct signature", l, u),
                }
            }
            // A value with a construct signature (`declare var HTMLElement: {new(): HTMLElement}`)
            // against `Class<T>`: what the signature constructs, as in subtyping.
            (DefTInner::InstanceT(li), DefTInner::ClassT(b))
                if let Some(ctor) =
                    (self.ancestors(cx, li).into_iter()).find_map(|i| i.inst.inst_construct_t) =>
            {
                let b = b.dupe();
                let ctor = cx.find_call(ctor);
                let ctor = self.resolve(cx, &ctor);
                match ctor.deref() {
                    TypeInner::DefT(_, d) if let DefTInner::FunT(_, lf) = d.deref() => {
                        let ret = lf.return_t.dupe();
                        self.walk(cx, &ret, &b)
                    }
                    _ => Ok(()),
                }
            }
            (DefTInner::ClassT(a), DefTInner::ClassT(b)) => {
                let (a, b) = (a.dupe(), b.dupe());
                self.walk(cx, &a, &b)
            }
            (DefTInner::TypeT(_, a), DefTInner::TypeT(_, b)) => {
                let (a, b) = (a.dupe(), b.dupe());
                self.walk(cx, &a, &b)
            }
            (DefTInner::ReactAbstractComponentT(lc), DefTInner::ReactAbstractComponentT(uc)) => {
                let (lconfig, uconfig) = (lc.config.dupe(), uc.config.dupe());
                let urenders = uc.renders.dupe();
                // A component of component syntax against a structural one renders itself.
                let lrenders = match (&lc.component_kind, &uc.component_kind) {
                    (
                        ComponentKind::Nominal(renders_id, renders_name, _),
                        ComponentKind::Structural,
                    ) => {
                        let reason = type_util::reason_of_t(l).dupe().replace_desc(
                            VirtualReasonDesc::RReactElement {
                                name_opt: Some(Name::new(renders_name.dupe())),
                                from_component_syntax: true,
                            },
                        );
                        Type::new(TypeInner::DefT(
                            reason,
                            DefT::new(DefTInner::RendersT(Rc::new(
                                CanonicalRendersForm::NominalRenders {
                                    renders_id: renders_id.dupe(),
                                    renders_name: renders_name.dupe(),
                                    renders_super: lc.renders.dupe(),
                                },
                            ))),
                        ))
                    }
                    _ => lc.renders.dupe(),
                };
                self.walk(cx, &uconfig, &lconfig)?;
                self.walk(cx, &lrenders, &urenders)
            }
            // An array flows to an object type through its builtin class.
            (DefTInner::ArrT(la), DefTInner::ObjT(_)) => {
                let instance = self.array_instance(cx, l, la);
                self.walk(cx, &instance, u)
            }
            (DefTInner::RendersT(_), _) | (_, DefTInner::RendersT(_)) => {
                self.walk_renders(cx, l, u)
            }
            // Enums, as in subtyping: an enum object's values against an abstract enum's, and an
            // enum value's representation type against an abstract one's.
            (
                DefTInner::EnumObjectT {
                    enum_value_t: lv, ..
                },
                DefTInner::EnumObjectT {
                    enum_value_t: uv, ..
                },
            ) => {
                let (lv, uv) = (lv.dupe(), uv.dupe());
                self.walk(cx, &lv, &uv)
            }
            (DefTInner::EnumValueT(li), DefTInner::EnumValueT(ui))
                if let EnumInfoInner::AbstractEnum {
                    representation_t: ur,
                } = ui.deref().deref() =>
            {
                let lr = match li.deref().deref() {
                    EnumInfoInner::ConcreteEnum(c) => c.representation_t.dupe(),
                    EnumInfoInner::AbstractEnum { representation_t } => representation_t.dupe(),
                };
                let ur = ur.dupe();
                self.walk(cx, &lr, &ur)
            }
            // A function, or an object with a call property, as a component: the config goes to
            // its props, and what it returns to what the component renders, as subtyping checks
            // them.
            (DefTInner::FunT(_, lf), DefTInner::ReactAbstractComponentT(uc))
                if matches!(uc.component_kind, ComponentKind::Structural) =>
            {
                let (lf, uc) = (lf.dupe(), uc.dupe());
                self.walk_function_component(cx, &lf, uc)
            }
            (DefTInner::ObjT(lo), DefTInner::ReactAbstractComponentT(uc))
                if matches!(uc.component_kind, ComponentKind::Structural)
                    && let Some(id) = lo.call_t =>
            {
                let call = cx.find_call(id);
                let call = self.resolve(cx, &call);
                let uc = uc.dupe();
                match call.deref() {
                    TypeInner::DefT(_, d) if let DefTInner::FunT(_, lf) = d.deref() => {
                        let lf = lf.dupe();
                        self.walk_function_component(cx, &lf, uc)
                    }
                    _ => Ok(()),
                }
            }
            // A class as a component: the config goes to the config of the class (its props, with
            // the default props optional), and what `render` returns to what the component
            // renders.
            (DefTInner::ClassT(instance), DefTInner::ReactAbstractComponentT(uc))
                if matches!(uc.component_kind, ComponentKind::Structural) =>
            {
                let (instance, uc) = (instance.dupe(), uc.dupe());
                let reason = type_util::reason_of_t(l).dupe();
                let d = TypeDestructorT::new(TypeDestructorTInner(
                    unknown_use(),
                    reason,
                    Rc::new(Destructor::ReactElementConfigType),
                ));
                let instance = self.normalize(cx, &instance);
                // `Class<C<T>>`: the instance is an application of the class.
                let instance = match instance.deref() {
                    TypeInner::TypeAppT(_) => {
                        self.expand_typeapp(cx, &instance).unwrap_or(instance)
                    }
                    _ => instance,
                };
                // The config of a class with markers is `React.ReactManagedAttributes` of a spread
                // of its props, which the walk can't take apart.
                let config = if self.mentions(cx, l) {
                    None
                } else {
                    self.evaluate(cx, l, &d)?
                };
                match config {
                    Some(config) => self.walk(cx, &uc.config, &config)?,
                    // With markers in the class, its config is stuck on them: the props it
                    // declares, through the `React.Component` it extends.
                    None => {
                        // The props stand for the config, which the flow-based solve checks only
                        // once they are known, through `React.ReactManagedAttributes`: their
                        // evidence counts where it agrees with the evidence so far, as where that
                        // check succeeds.
                        if let Some(props) = self.class_component_props(cx, &instance)? {
                            let config = self.evaluated(cx, &uc.config)?;
                            let snapshot = self.snapshot();
                            self.walk(cx, &config, &props)?;
                            if !self.agrees_with(cx, &snapshot.vars)? {
                                self.restore(snapshot);
                            }
                        }
                    }
                }
                if let Ok(Some(render)) = self.method_type_of(cx, &instance, "render") {
                    let render = self.resolve(cx, &render);
                    if let TypeInner::DefT(_, d) = render.deref()
                        && let DefTInner::FunT(_, rf) = d.deref()
                    {
                        let ret = rf.return_t.dupe();
                        self.walk(cx, &ret, &uc.renders)?;
                    }
                }
                Ok(())
            }
            // A callable instance, or a class with a static call signature, as a function: its
            // call signature, as in subtyping.
            (DefTInner::InstanceT(li), DefTInner::FunT(..)) => {
                let call = (self.ancestors(cx, li).into_iter()).find_map(|i| i.inst.inst_call_t);
                match call {
                    Some(id) => {
                        let call = cx.find_call(id);
                        self.walk(cx, &call, u)
                    }
                    None => Ok(()),
                }
            }
            (DefTInner::ClassT(instance), DefTInner::FunT(..) | DefTInner::InstanceT(_)) => {
                let instance = self.normalize(cx, instance);
                let TypeInner::DefT(_, d) = instance.deref() else {
                    return Ok(());
                };
                let DefTInner::InstanceT(i) = d.deref() else {
                    return Ok(());
                };
                let statics = self.resolve(cx, &i.static_);
                let TypeInner::DefT(_, sd) = statics.deref() else {
                    return Ok(());
                };
                match (sd.deref(), ud.deref()) {
                    // The statics against an interface, which subtyping compares structurally.
                    (DefTInner::ObjT(_), DefTInner::InstanceT(ui))
                        if matches!(ui.inst.inst_kind, InstanceKind::InterfaceKind { .. }) =>
                    {
                        self.walk(cx, &statics, u)
                    }
                    (DefTInner::ObjT(so), DefTInner::FunT(..)) if let Some(id) = so.call_t => {
                        let call = cx.find_call(id);
                        self.walk(cx, &call, u)
                    }
                    _ => Ok(()),
                }
            }
            // A pair that fails regardless of the markers (a primitive against a structure, or the
            // reverse) says nothing about them; the check of the operation reports it.
            (l_def, u_def) if is_atomic(l_def) || is_atomic(u_def) => Ok(()),
            // Anything else, such as a component against anything but a component: no evidence.
            _ => self.give_up(cx, "unhandled def pair", l, u),
        }
    }

    /// Walks the arguments of a call (or of `new`, or of a method call) against the signature it
    /// calls, and its result against the return hint, in this order: the receiver and the ordinary
    /// arguments, then the return hint, then the conditional types that waited for those, then the
    /// generic values. `Err` names what the walk does not handle.
    fn walk_call<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        reason_op: &Reason,
        this_t: &Type,
        args: &[CallArg],
        signature: &Signature<'_>,
        return_hint: &Option<(Type, flow_common::hint::HintKind)>,
    ) -> Result<(), FlowJsException> {
        let params = signature.params;
        let Some((args, tail)) = self.flatten_args(cx, args)? else {
            // A spread of something else: the arguments before it, and the rest gets no evidence.
            let before: Vec<CallArg> = args
                .iter()
                .take_while(|a| matches!(&***a, CallArgInner::Arg(_)))
                .cloned()
                .collect();
            let n = before.len();
            let params: Vec<FunParam> = signature.params.iter().take(n).cloned().collect();
            let truncated = Signature {
                params: &params,
                rest_param: None,
                this_param: signature.this_param,
                return_t: signature.return_t,
            };
            return self.walk_call(cx, reason_op, this_t, &before, &truncated, return_hint);
        };
        let mut pairs = Vec::new();
        let mut rest_args = Vec::new();
        for (index, t) in args.iter().enumerate() {
            match params.get(index) {
                Some(FunParam(_, param)) => pairs.push((t.dupe(), param.dupe())),
                None => rest_args.push(t.dupe()),
            }
        }
        if let Some(tail) = &tail {
            // The elements after a spread of unknown length may be passed to any of the remaining
            // parameters.
            for FunParam(_, param) in params.iter().skip(args.len()) {
                for elem in &tail.elems {
                    pairs.push((elem.dupe(), param.dupe()));
                }
            }
        }
        if let Some(FunRestParam(_, _, rest)) = signature.rest_param {
            if self.rest_is_array(cx, rest) {
                let n = rest_args.len();
                for (i, arg) in rest_args.into_iter().enumerate() {
                    if let Some(elem) = self.rest_elem(cx, rest, i) {
                        pairs.push((arg, elem));
                    }
                }
                if let Some(tail) = &tail
                    && let Some(rest_elem) = self.rest_elem(cx, rest, n)
                {
                    for elem in &tail.elems {
                        pairs.push((elem.dupe(), rest_elem.dupe()));
                    }
                }
            } else {
                // A rest parameter whose type is inferred receives the tuple of the arguments, the
                // spread array when nothing else gets its elements, or an array of all the
                // elements.
                match &tail {
                    None => pairs.push((mk_tuple(reason_op, rest_args, true), rest.dupe())),
                    Some(Tail {
                        array: Some(arr), ..
                    }) if rest_args.is_empty() && args.len() == params.len() => {
                        pairs.push((arr.dupe(), rest.dupe()))
                    }
                    Some(Tail { elems, .. }) => {
                        let ts = rest_args.into_iter().chain(elems.iter().duped()).collect();
                        pairs.push((mk_array(reason_op, ts), rest.dupe()))
                    }
                }
            }
        }
        // An omitted argument passes `void`, as the call does.
        if tail.is_none() {
            for FunParam(_, param) in params.iter().skip(args.len()) {
                pairs.push((void::why(reason_op.dupe()), param.dupe()));
            }
        }
        if let Some(this_param) = signature.this_param {
            pairs.insert(0, (this_t.dupe(), this_param.dupe()));
        }

        let (generic_values, others): (Vec<_>, Vec<_>) = pairs.into_iter().partition(|(arg, _)| {
            let arg = self.resolve(cx, arg);
            matches!(arg.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::PolyT(_)))
        });
        for (arg, param) in &others {
            self.walk(cx, arg, param)?;
        }
        if let Some((hint, kind)) = return_hint {
            // The flow-based solve drops a best-effort hint whose flow fails: keep its evidence
            // only if it agrees with the evidence of the arguments.
            let snapshot =
                (*kind == flow_common::hint::HintKind::BestEffortHint).then(|| self.snapshot());
            self.in_hint = true;
            self.walk(cx, signature.return_t, hint)?;
            self.in_hint = false;
            if let Some(snapshot) = snapshot
                && !self.agrees_with(cx, &snapshot.vars)?
            {
                self.restore(snapshot);
            }
        }
        self.walk_deferred(cx)?;
        for (arg, param) in &generic_values {
            self.walk(cx, arg, param)?;
        }
        Ok(())
    }

    /// The arguments of a call, with spreads of tuples flattened, and from the first spread of
    /// unknown length on, the types of the elements that may be at any position. `None` if the
    /// elements of a spread are not known.
    fn flatten_args<'cx>(
        &self,
        cx: &mut Context<'cx>,
        args: &[CallArg],
    ) -> Result<Option<(Vec<Type>, Option<Tail>)>, FlowJsException> {
        let mut out = Vec::new();
        let mut tail: Option<Tail> = None;
        for arg in args.iter() {
            let ts = match arg.deref() {
                CallArgInner::Arg(t) => vec![t.dupe()],
                CallArgInner::SpreadArg(t) => {
                    let t = self.resolve(cx, t);
                    if let Some(elements) = tuple_elements(&t) {
                        elements
                    } else {
                        let (elem, array) = match t.deref() {
                            TypeInner::DefT(_, d) if let DefTInner::ArrT(a) = d.deref() => {
                                match a.deref() {
                                    ArrType::ArrayAT(a) => (Some(a.elem_t.dupe()), Some(t.dupe())),
                                    ArrType::ROArrayAT(a) => {
                                        (Some(a.elem_t.dupe()), Some(t.dupe()))
                                    }
                                    ArrType::TupleAT(_) => (None, None),
                                }
                            }
                            // A spread of a type parameter, such as a generic rest parameter, is
                            // also that type parameter as the array.
                            TypeInner::GenericT(_) => (self.iterable_elem(cx, &t)?, Some(t.dupe())),
                            _ => (self.iterable_elem(cx, &t)?, None),
                        };
                        let Some(elem) = elem else {
                            return Ok(None);
                        };
                        let tail = tail.get_or_insert_with(|| Tail {
                            elems: Vec::new(),
                            array,
                        });
                        if !tail.elems.is_empty() {
                            tail.array = None;
                        }
                        tail.elems.push(elem);
                        continue;
                    }
                }
            };
            match &mut tail {
                None => out.extend(ts),
                Some(tail) => {
                    tail.array = None;
                    tail.elems.extend(ts);
                }
            }
        }
        Ok(Some((out, tail)))
    }

    /// The type of the elements of an iterable, as a spread of it reads them: the solution of `E`
    /// in `t <: Iterable<E>`, walked with a marker for `E`.
    fn iterable_elem<'cx>(
        &self,
        cx: &mut Context<'cx>,
        t: &Type,
    ) -> Result<Option<Type>, FlowJsException> {
        let reason = type_util::reason_of_t(t).dupe();
        let name = SubstName::id(mk_id() as i32, FlowSmolStr::new("E"));
        let id = cx.make_generic_id(name.dupe(), reason.loc());
        let elem = Type::new(TypeInner::GenericT(Box::new(GenericTData {
            reason: reason.dupe(),
            name: name.dupe(),
            bound: mixed_t::make(reason.dupe()),
            no_infer: false,
            id,
        })));
        let any = || unsoundness::why(UnsoundnessKind::ResolveSpread, reason.dupe());
        let iterable = FlowJs::get_builtin_typeapp_with_env(
            cx,
            self.env,
            &reason.dupe().replace_desc_new(None),
            None,
            "Iterable",
            vec![elem, any(), any()],
        );
        let mut walker = Walker::new(self.env, self.preserve, self.call_loc.dupe(), [name.dupe()]);
        walker.walk(cx, t, &iterable)?;
        let bounds = &walker.vars[&name];
        let elem = match bounds.exact.first() {
            Some(t) => Some(t.dupe()),
            None => {
                type_util::union_of_ts_opt(reason, types(&bounds.lower).duped().collect(), None)
            }
        };
        Ok(elem.filter(|t| !flow_js_utils::tvar_visitors::has_placeholders(cx, t)))
    }

    /// The type read from the property `name` of `receiver`, an instance or an object.
    fn prop_of<'cx>(&self, cx: &mut Context<'cx>, receiver: &Type, name: &str) -> Option<Type> {
        let name = Name::new(FlowSmolStr::new(name));
        let receiver = self.resolve(cx, receiver);
        let TypeInner::DefT(_, d) = receiver.deref() else {
            return None;
        };
        let prop = match d.deref() {
            DefTInner::InstanceT(i) => self.inst_prop(cx, i, &name)?,
            DefTInner::ObjT(o) => cx
                .find_props(o.props_tmap.dupe())
                .get(&name)
                .map(|p| p.dupe())?,
            _ => return None,
        };
        property::read_t(&prop)
    }

    /// The type of the property that the indexed access `d` of the object type `object` reads, as
    /// declared. Where it is a destructor with markers inside, such as `Readonly<{...T}>` in
    /// `{a: Readonly<{...T}>}['a']`, the walk takes it apart, but evaluating the access does not
    /// give it: it evaluates the destructor too, which makes a type from the markers.
    fn declared_property<'cx>(
        &self,
        cx: &mut Context<'cx>,
        object: &Type,
        d: &Destructor,
    ) -> Option<Type> {
        let Destructor::PropertyType { name } = d else {
            return None;
        };
        self.prop_of(cx, object, name.as_str_opt()?)
    }

    /// The props of a class component, as the flow-based solve reads them when it checks the class
    /// against `React.Component<Props, Instance>`: the type argument of the `React.Component` it
    /// extends, rather than a `props` field it may redeclare.
    fn class_component_props<'cx>(
        &self,
        cx: &mut Context<'cx>,
        instance: &Type,
    ) -> Result<Option<Type>, FlowJsException> {
        let reason = type_util::reason_of_t(instance).dupe();
        let placeholder = placeholder_t::why(reason.dupe(), mixed_t::why(reason.dupe()));
        let component = sandbox(cx, self.env, self.preserve, |cx| {
            FlowJs::get_builtin_react_typeapp_with_env(
                cx,
                self.env,
                &reason,
                None,
                ExpectedModulePurpose::ReactModuleForReactClassComponent,
                vec![placeholder.dupe(), placeholder],
            )
        })?;
        let component = component.and_then(|c| self.expand_typeapp(cx, &c));
        let resolved = self.resolve(cx, instance);
        if let Some(component) = component
            && let TypeInner::DefT(_, cd) = component.deref()
            && let DefTInner::InstanceT(ci) = cd.deref()
            && let TypeInner::DefT(_, d) = resolved.deref()
            && let DefTInner::InstanceT(i) = d.deref()
            && let Some(ancestor) =
                (self.ancestors(cx, i).into_iter()).find(|a| a.inst.class_id == ci.inst.class_id)
            && let Some((_, _, props, _)) = ancestor.inst.type_args.first()
        {
            return Ok(Some(props.dupe()));
        }
        Ok(self.prop_of(cx, instance, "props"))
    }

    /// The config of a class component is its props with `ref?: React.RefSetter<I>` for its
    /// instance `I`, so the `ref` of an element meets the instance.
    fn walk_jsx_ref<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        jsx_props: &Type,
        instance: &Type,
    ) -> Result<(), FlowJsException> {
        let Some(ref_t) = self.prop_of(cx, jsx_props, "ref") else {
            return Ok(());
        };
        let reason = type_util::reason_of_t(instance).dupe();
        let ref_setter = sandbox(cx, self.env, self.preserve, |cx| {
            FlowJs::get_builtin_react_typeapp_with_env(
                cx,
                self.env,
                &reason,
                None,
                ExpectedModulePurpose::ReactModuleForReactRefSetterType,
                vec![instance.dupe()],
            )
        })?;
        match ref_setter {
            Some(ref_setter) => self.walk(cx, &ref_t, &ref_setter),
            None => Ok(()),
        }
    }

    /// The props that the config check of a class component makes of the props of a JSX element:
    /// the element's props, with the component's default props for those it leaves out. `None`
    /// if the component has no default props.
    fn props_with_defaults<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        component: &Type,
        jsx_props: &Type,
    ) -> Result<Option<Type>, FlowJsException> {
        let Ok(Some(defaults)) = self.method_type_of(cx, component, "defaultProps") else {
            return Ok(None);
        };
        if self.mentions(cx, jsx_props) || self.mentions(cx, &defaults) {
            return Ok(None);
        }
        let env = self.env;
        let reason = type_util::reason_of_t(jsx_props)
            .dupe()
            .replace_desc_new(None);
        let props = sandbox(cx, env, self.preserve, |cx| {
            flowed(cx, &reason, |cx, tout| {
                let tool = object::Tool::ReactConfig(Box::new(ObjectToolReactConfigData {
                    state: object::react_config::State::Config {
                        component_default_props: Some(defaults.dupe()),
                    },
                    ref_manipulation: object::react_config::RefManipulation::KeepRef,
                    literal: true,
                }));
                FlowJs::flow_with_env(
                    cx,
                    env,
                    jsx_props,
                    &UseT::new(UseTInner::ObjKitT(
                        unknown_use(),
                        reason.dupe(),
                        Box::new(object::ResolveTool::Resolve(object::Resolve::Next)),
                        Box::new(tool),
                        tout.dupe(),
                    )),
                )
            })
        })?
        .flatten();
        Ok(props.map(|props| self.resolve(cx, &props)))
    }

    /// The type of the method `name` of `receiver`, resolved.
    fn method_type_of<'cx>(
        &self,
        cx: &mut Context<'cx>,
        receiver: &Type,
        name: &str,
    ) -> Result<Option<Type>, NoMethod> {
        let name = Name::new(FlowSmolStr::new(name));
        let receiver = self.resolve(cx, receiver);
        let TypeInner::DefT(_, d) = receiver.deref() else {
            return Err(NoMethod::NotAnObject);
        };
        let prop = match d.deref() {
            DefTInner::InstanceT(i) => self.inst_prop(cx, i, &name),
            DefTInner::ObjT(o) => cx
                .find_props(o.props_tmap.dupe())
                .get(&name)
                .map(|p| p.dupe()),
            // A static method of a generic class: the statics of its instance, its own or
            // inherited.
            DefTInner::ClassT(inner) => {
                let instance = self.normalize(cx, inner);
                let TypeInner::DefT(_, d) = instance.deref() else {
                    return Err(NoMethod::Static);
                };
                let DefTInner::InstanceT(i) = d.deref() else {
                    return Err(NoMethod::Static);
                };
                self.ancestors(cx, i).iter().find_map(|ancestor| {
                    match self.resolve(cx, &ancestor.static_).deref() {
                        TypeInner::DefT(_, d) if let DefTInner::ObjT(o) = d.deref() => cx
                            .find_props(o.props_tmap.dupe())
                            .get(&name)
                            .map(|p| p.dupe()),
                        _ => None,
                    }
                })
            }
            DefTInner::FunT(..) => return Err(NoMethod::OfFunction),
            DefTInner::PolyT(_) => return Err(NoMethod::OfGenericValue),
            _ => return Err(NoMethod::NotAnObject),
        };
        let Some(prop) = prop else {
            return Ok(None);
        };
        let Some(t) = property::read_t(&prop) else {
            return Err(NoMethod::NotReadable);
        };
        Ok(Some(self.resolve(cx, &t)))
    }

    /// The signature that a call with `args` takes from `t`, a function or an overloaded one,
    /// generic ones with fresh markers: of overloads, the first whose arity admits the arguments
    /// and whose parameters the arguments fit with the markers open, as in the speculative match
    /// of the flow-based solve. `None` if there is none.
    fn signature_for_call<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        t: &Type,
        args: &[CallArg],
    ) -> Result<Option<Rc<FunType>>, FlowJsException> {
        let t = self.instantiate_generic(cx, t);
        match t.deref() {
            TypeInner::DefT(_, d) => match d.deref() {
                DefTInner::FunT(_, f) => Ok(Some(f.dupe())),
                _ => Ok(None),
            },
            TypeInner::IntersectionT(_, rep) => {
                let n = args.len();
                let spread = args
                    .iter()
                    .any(|a| matches!(a.deref(), CallArgInner::SpreadArg(_)));
                let admits = |f: &FunType| {
                    let required = f
                        .params
                        .iter()
                        .filter(|FunParam(_, t)| !matches!(t.deref(), TypeInner::OptionalT { .. }))
                        .count();
                    spread || (required <= n && (n <= f.params.len() || f.rest_param.is_some()))
                };
                for member in rep.members_iter() {
                    let member = self.resolve(cx, member);
                    let member = self.instantiate_generic(cx, &member);
                    if let TypeInner::DefT(_, d) = member.deref()
                        && let DefTInner::FunT(_, f) = d.deref()
                        && admits(f)
                        && self.args_fit(cx, f, args)?
                    {
                        return Ok(Some(f.dupe()));
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Whether the arguments before any spread fit the parameters of `f` they are passed to, with
    /// the markers open.
    fn args_fit<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        f: &FunType,
        args: &[CallArg],
    ) -> Result<bool, FlowJsException> {
        for (i, arg) in args.iter().enumerate() {
            let CallArgInner::Arg(arg) = arg.deref() else {
                break;
            };
            let param = match f.params.get(i) {
                Some(FunParam(_, param)) => Some(param.dupe()),
                None => (f.rest_param.as_ref()).and_then(|FunRestParam(_, _, rest)| {
                    self.rest_elem(cx, rest, i - f.params.len())
                }),
            };
            if let Some(param) = param
                && !self.fits_open(cx, arg, &param)?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The body of `t` with fresh markers for its type parameters, if it is generic: they get
    /// bounds like the others, which are not part of the solution.
    fn instantiate_generic<'cx>(&mut self, cx: &mut Context<'cx>, t: &Type) -> Type {
        let TypeInner::DefT(_, d) = t.deref() else {
            return t.dupe();
        };
        let DefTInner::PolyT(p) = d.deref() else {
            return t.dupe();
        };
        let mut map = FlowOrdMap::new();
        for tparam in p.tparams.iter() {
            let (name, marker) = mk_marker(cx, tparam);
            self.vars.insert(name.dupe(), Bounds::default());
            self.tparams.insert(name, tparam.dupe());
            map.insert(tparam.name.dupe(), marker);
        }
        let body = subst_markers(cx, &map, &p.t_out);
        self.resolve(cx, &body)
    }

    /// The props of a JSX element against the props its component takes. The config of a
    /// component declared with component syntax keeps its declared props in the destructor and
    /// applies it to its rest props: the element's props go to the declared props of the same
    /// name, and the others to the rest props, as the flow-based solve's reversal computes them.
    fn walk_jsx_props<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        jsx_props: &Type,
        config: &Type,
    ) -> Result<(), FlowJsException> {
        let jsx_props = &self.without_key(cx, jsx_props);
        let config = self.resolve(cx, config);
        let TypeInner::EvalT {
            type_: rest,
            defer_use_t,
            ..
        } = config.deref()
        else {
            return self.walk(cx, jsx_props, &config);
        };
        let TypeDestructorTInner(_, reason, d) = defer_use_t.deref();
        let Destructor::ReactCheckComponentConfig { props } = d.deref() else {
            return self.walk(cx, jsx_props, &config);
        };
        let resolved = self.resolve(cx, jsx_props);
        let TypeInner::DefT(_, ld) = resolved.deref() else {
            return self.walk(cx, jsx_props, &config);
        };
        let lo = match ld.deref() {
            DefTInner::ObjT(lo) => lo,
            // An element without attributes gives its component no props.
            DefTInner::NullT | DefTInner::VoidT => return Ok(()),
            _ => return self.walk(cx, jsx_props, &config),
        };
        if self.mentions(cx, jsx_props) {
            return self.walk(cx, jsx_props, &config);
        }
        let lp = cx.find_props(lo.props_tmap.dupe());
        let dict = obj_type::get_dict_opt(&lo.flags.obj_kind).cloned();
        // As with arguments, generic values go after the other props.
        let (generic_values, others): (BTreeMap<_, _>, BTreeMap<_, _>) =
            lp.iter().map(|(n, p)| (n.dupe(), p.dupe())).partition(|(_, p)| {
                property::read_t(p).is_some_and(|t| {
                    let t = self.resolve(cx, &t);
                    matches!(t.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::PolyT(_)))
                })
            });
        let others = properties::PropertiesMap::from_btree_map(others);
        let generic_values = properties::PropertiesMap::from_btree_map(generic_values);
        self.walk_props(
            cx,
            &others,
            dict.as_ref(),
            props,
            None,
            true,
            lo.literal.is_literal(),
        )?;
        self.walk_deferred_pairs(cx)?;
        self.walk_props(
            cx,
            &generic_values,
            None,
            props,
            None,
            true,
            lo.literal.is_literal(),
        )?;
        let env = self.env;
        match sandbox(cx, env, self.preserve, |cx| {
            reverse_config(cx, env, reason, props, jsx_props)
        })?
        .flatten()
        {
            Some(rest_props) => self.walk(cx, &rest_props, rest),
            None => Ok(()),
        }
    }

    /// The props of a JSX element without `key`, which the config check removes.
    fn without_key<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> Type {
        let resolved = self.resolve(cx, t);
        let TypeInner::DefT(reason, d) = resolved.deref() else {
            return t.dupe();
        };
        let DefTInner::ObjT(o) = d.deref() else {
            return t.dupe();
        };
        let key = Name::new("key");
        let props = cx.find_props(o.props_tmap.dupe());
        if props.get(&key).is_none() {
            return t.dupe();
        }
        let kept: BTreeMap<Name, Property> = props
            .iter()
            .filter(|(name, _)| **name != key)
            .map(|(name, p)| (name.dupe(), p.dupe()))
            .collect();
        obj_type::mk_with_proto(
            cx,
            reason.dupe(),
            o.flags.obj_kind.clone(),
            Some(o.reachable_targs.dupe()),
            None,
            Some(properties::PropertiesMap::from_btree_map(kept)),
            None,
            o.proto_t.dupe(),
            o.literal,
        )
    }

    /// A spread whose operands are not markers.
    fn is_spread_of_types(&self, t: &Type) -> bool {
        let TypeInner::EvalT {
            type_, defer_use_t, ..
        } = t.deref()
        else {
            return false;
        };
        let Destructor::SpreadType(box DestructorSpreadTypeData(_, todo_rev, _)) =
            defer_use_t.2.deref()
        else {
            return false;
        };
        self.marker_of(type_).is_none()
            && todo_rev.iter().all(|operand| match operand {
                object::spread::Operand::Type(t) => self.marker_of(t).is_none(),
                object::spread::Operand::Slice(_) => true,
            })
    }

    /// Whether `t` is an object, instance or array type, whose parts are known.
    fn is_structure<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> bool {
        let t = self.resolve(cx, t);
        matches!(t.deref(), TypeInner::DefT(_, d) if matches!(
            d.deref(),
            DefTInner::ObjT(_) | DefTInner::InstanceT(_) | DefTInner::ArrT(_)
        ))
    }

    /// Whether `t` is an object, instance or array type, or a union of them.
    fn is_structure_or_union_of<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> bool {
        match self.resolve(cx, t).deref() {
            TypeInner::UnionT(_, rep) => rep.members_iter().all(|m| self.is_structure(cx, m)),
            _ => self.is_structure(cx, t),
        }
    }

    /// The body of the application `t` of an alias of an object type.
    fn object_alias_body<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> Option<Type> {
        let resolved = self.resolve(cx, t);
        if !matches!(resolved.deref(), TypeInner::TypeAppT(a) if self.is_alias(cx, &a.type_)) {
            return None;
        }
        let body = self.expand_typeapp(cx, &resolved)?;
        let body = self.resolve(cx, &body);
        matches!(body.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::ObjT(_)))
            .then_some(body)
    }

    /// The body of the application `t` of an alias of a union.
    fn union_alias_body<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> Option<Type> {
        let resolved = self.resolve(cx, t);
        if !matches!(resolved.deref(), TypeInner::TypeAppT(a) if self.is_alias(cx, &a.type_)) {
            return None;
        }
        let body = self.expand_typeapp(cx, &resolved)?;
        let body = self.resolve(cx, &body);
        matches!(body.deref(), TypeInner::UnionT(..)).then_some(body)
    }

    /// `t`, or the one type it concretizes to, if it is an object, instance or array type: an
    /// application of an alias of one, or a destructor that makes one, such as a spread of types or
    /// a read-only object, as the flow-based solve evaluates it before it indexes it.
    fn structure_of<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        t: &Type,
    ) -> Result<Option<Type>, FlowJsException> {
        if self.is_structure(cx, t) {
            return Ok(Some(t.dupe()));
        }
        Ok(match self.concretize(cx, t)?.as_slice() {
            [only] if self.is_structure(cx, only) => Some(only.dupe()),
            _ => None,
        })
    }

    /// The concrete types `t` stands for, as the flow-based solve concretizes it for inspection,
    /// without errors or flows that outlive it. A destructor that would act on a marker stays.
    fn concretize<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        let markers = Arc::new(self.vars.keys().duped().collect::<BTreeSet<_>>());
        let reason = type_util::reason_of_t(t).dupe();
        let errors = cx.errors();
        let result = run_keeping_clean_flows(cx, self.env, self.preserve, |cx| {
            FlowJs::possible_concrete_types_for_walk_with_env(cx, self.env, &reason, t, markers)
        });
        cx.reset_errors(errors);
        Ok(instantiation_solver::sandboxed(result)?.unwrap_or_default())
    }

    /// Whether `t` is an object or an instance with a call signature, or a class with a static one,
    /// which the walk takes to a function type through that signature.
    fn has_call_signature<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> bool {
        let t = self.resolve(cx, t);
        let TypeInner::DefT(_, d) = t.deref() else {
            return false;
        };
        match d.deref() {
            DefTInner::ObjT(o) => o.call_t.is_some(),
            DefTInner::InstanceT(i) => {
                (self.ancestors(cx, i).iter()).any(|a| a.inst.inst_call_t.is_some())
            }
            DefTInner::ClassT(instance) => {
                let instance = self.normalize(cx, instance);
                match instance.deref() {
                    TypeInner::DefT(_, d) if let DefTInner::InstanceT(i) = d.deref() => {
                        matches!(self.resolve(cx, &i.static_).deref(), TypeInner::DefT(_, sd)
                            if matches!(sd.deref(), DefTInner::ObjT(so) if so.call_t.is_some()))
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// The builtin class an array type stands for in structural and nominal checks.
    fn array_instance<'cx>(&self, cx: &mut Context<'cx>, t: &Type, a: &ArrType) -> Type {
        let (name, elem) = match a {
            ArrType::ArrayAT(a) => ("Array", a.elem_t.dupe()),
            ArrType::TupleAT(a) => ("ReadonlyArray", a.elem_t.dupe()),
            ArrType::ROArrayAT(a) => ("ReadonlyArray", a.elem_t.dupe()),
        };
        let reason = type_util::reason_of_t(t).dupe();
        FlowJs::get_builtin_typeapp_with_env(cx, self.env, &reason, None, name, vec![elem])
    }

    fn walk_fun<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        lf: &FunType,
        uf: &FunType,
    ) -> Result<(), FlowJsException> {
        let (ln, un) = (lf.params.len(), uf.params.len());
        let lrest = lf.rest_param.as_ref().map(|FunRestParam(_, _, t)| t.dupe());
        let urest = uf.rest_param.as_ref().map(|FunRestParam(_, _, t)| t.dupe());
        // `u`'s parameters that only `l`'s rest parameter receives, when it is not an array.
        let mut into_lrest = Vec::new();
        for (i, FunParam(_, ut)) in uf.params.iter().enumerate() {
            if let Some(FunParam(_, lt)) = lf.params.get(i) {
                self.walk(cx, ut, lt)?;
            } else if let Some(lrest) = &lrest {
                match self.rest_elem(cx, lrest, i - ln) {
                    Some(elem) => self.walk(cx, ut, &elem)?,
                    None => into_lrest.push(ut.dupe()),
                }
            }
        }
        // `l`'s parameters that `u`'s rest parameter feeds.
        if let Some(urest) = &urest {
            let finite = self.rest_is_array(cx, urest);
            for (i, FunParam(_, lt)) in lf.params.iter().enumerate().skip(un) {
                match self.rest_elem(cx, urest, i - un) {
                    Some(elem) => self.walk(cx, &elem, lt)?,
                    // Past the end of a tuple, the caller passes nothing.
                    None if finite => {
                        let reason = type_util::reason_of_t(urest).dupe();
                        self.walk(cx, &void::why(reason), lt)?;
                    }
                    // When it is not an array, the reversal below covers them.
                    None => {}
                }
            }
        }
        let reason = type_util::reason_of_t(&lf.return_t).dupe();
        // A caller of `u` passes nothing for `l`'s extra parameters.
        if urest.is_none() {
            for FunParam(_, lt) in lf.params.iter().skip(un) {
                self.walk(cx, &void::why(reason.dupe()), lt)?;
            }
        }
        match (&urest, &lrest) {
            // `l`'s rest parameter receives `u`'s parameters past `l`'s and `u`'s rest parameter.
            (Some(urest), Some(lrest)) if un > ln && !self.rest_is_array(cx, urest) => {
                let tuple = reverse_resolve_spread_multiflow_subtype_full_no_resolution(
                    cx,
                    &reason,
                    uf.params.get(ln..).unwrap_or(&[]),
                    &uf.rest_param,
                )?;
                self.walk(cx, &tuple, lrest)?;
            }
            // `u`'s rest parameter feeds `l`'s remaining parameters: it is a subtype of their tuple,
            // as the flow-based solve's reversal builds it.
            (Some(urest), _) if !self.rest_is_array(cx, urest) => {
                let tuple = reverse_resolve_spread_multiflow_subtype_full_no_resolution(
                    cx,
                    &reason,
                    lf.params.get(un..).unwrap_or(&[]),
                    &lf.rest_param,
                )?;
                self.walk(cx, urest, &tuple)?;
            }
            // `u`'s remaining parameters are the arguments `l`'s rest parameter receives.
            (_, Some(lrest)) if !self.rest_is_array(cx, lrest) && uf.rest_param.is_none() => {
                let reason = type_util::reason_of_t(lrest).dupe();
                let args = std::mem::take(&mut into_lrest);
                self.walk(cx, &mk_tuple(&reason, args, false), lrest)?;
            }
            // When `l`'s parameters also took elements of an array `u`'s rest parameter spreads,
            // `l`'s rest parameter receives a new array of the remaining elements.
            (Some(urest), Some(lrest))
                if !self.rest_is_array(cx, lrest)
                    && ln > un
                    && let Some(elem) = self.array_elem(cx, urest) =>
            {
                let reason = type_util::reason_of_t(urest).dupe();
                self.walk(cx, &mk_array(&reason, vec![elem]), lrest)?;
            }
            // `u`'s rest parameter is a tuple: `l`'s rest parameter receives `u`'s parameters past
            // `l`'s and the elements of that tuple, as the call builds them (`multiflow_args`): an
            // array with a tuple view, which fits an `Array` as well as a tuple.
            (Some(urest), Some(lrest))
                if !self.rest_is_array(cx, lrest)
                    && let Some(elems) = tuple_elements(&self.resolve(cx, urest)) =>
            {
                let reason = type_util::reason_of_t(lrest).dupe();
                let params = uf.params.get(ln..).unwrap_or(&[]);
                // `l`'s parameters past `u`'s took the first elements.
                let args = (params.iter().map(|FunParam(_, t)| t.dupe()))
                    .chain(elems.into_iter().skip(ln.saturating_sub(un)))
                    .collect();
                self.walk(cx, &mk_tuple(&reason, args, false), lrest)?;
            }
            (_, Some(lrest)) if !self.rest_is_array(cx, lrest) => {
                let tuple = reverse_resolve_spread_multiflow_subtype_full_no_resolution(
                    cx,
                    &reason,
                    uf.params.get(ln..).unwrap_or(&[]),
                    &uf.rest_param,
                )?;
                self.walk(cx, &tuple, lrest)?;
            }
            (Some(urest), Some(lrest)) => self.walk(cx, urest, lrest)?,
            _ => {}
        }
        // As in subtyping, the types of two type guards are unified unless the upper one is
        // one-sided.
        if let (Some(lg), Some(ug)) = (&lf.type_guard, &uf.type_guard)
            && let (Some(lt), Some(ut)) = (&lg.type_guard, &ug.type_guard)
        {
            if ug.one_sided() {
                self.walk(cx, lt, ut)?;
            } else {
                self.walk_invariant(cx, lt, ut)?;
            }
        }
        // As in subtyping, receivers are compared unless both are methods.
        if !(lf.this_t.1.is_method() && uf.this_t.1.is_method()) {
            self.walk(cx, &uf.this_t.0, &lf.this_t.0)?;
        }
        self.walk(cx, &lf.return_t, &uf.return_t)
    }

    fn walk_prop<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        lp: &Property,
        up: &Property,
        lower_is_literal: bool,
    ) -> Result<(), FlowJsException> {
        let (lp, up) = (property::property_type(lp), property::property_type(up));
        self.walk_property_type(cx, &lp, &up, lower_is_literal)
    }

    /// Two properties, as subtyping compares them (`rec_flow_p`).
    fn walk_property_type<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        lp: &PropertyType,
        up: &PropertyType,
        lower_is_literal: bool,
    ) -> Result<(), FlowJsException> {
        // Read-write fields are unified, as subtyping does.
        if !lower_is_literal
            && let (
                PropertyType::OrdinaryField {
                    type_: lt,
                    polarity: Polarity::Neutral,
                },
                PropertyType::OrdinaryField {
                    type_: ut,
                    polarity: Polarity::Neutral,
                },
            ) = (lp, up)
        {
            return self.walk_invariant(cx, lt, ut);
        }
        if let (Some(lr), Some(ur)) = (
            property::read_t_of_property_type(lp),
            property::read_t_of_property_type(up),
        ) {
            self.walk(cx, &lr, &ur)?;
        }
        // The properties of an object literal are checked covariantly.
        if !lower_is_literal
            && let (Some(lw), Some(uw)) = (
                property::write_t_of_property_type(lp, None),
                property::write_t_of_property_type(up, None),
            )
        {
            self.walk(cx, &uw, &lw)?;
        }
        Ok(())
    }

    /// A field of type `lt` with polarity `lpole` against one of type `ut` with polarity `upole`:
    /// read-write fields are unified, and the properties of an object literal are covariant.
    fn walk_field<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        lt: &Type,
        lpole: Polarity,
        ut: &Type,
        upole: Polarity,
        lower_is_literal: bool,
    ) -> Result<(), FlowJsException> {
        if lower_is_literal {
            return self.walk(cx, lt, ut);
        }
        match (lpole, upole) {
            (Polarity::Neutral, Polarity::Neutral) => self.walk_invariant(cx, lt, ut),
            (_, Polarity::Positive) => self.walk(cx, lt, ut),
            (_, Polarity::Negative) => self.walk(cx, ut, lt),
            (_, Polarity::Neutral) => {
                self.walk(cx, lt, ut)?;
                self.walk(cx, ut, lt)
            }
        }
    }

    fn walk_props<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        lp: &properties::PropertiesMap,
        ld: Option<&DictType>,
        up: &properties::PropertiesMap,
        ud: Option<&DictType>,
        lower_is_object: bool,
        lower_is_literal: bool,
    ) -> Result<(), FlowJsException> {
        // As with arguments, generic values go after the other props, which they are
        // instantiated against the evidence of.
        let (generic_values, others): (Vec<_>, Vec<_>) = up.iter().partition(|(name, _)| {
            lp.get(name)
                .and_then(property::read_t)
                .is_some_and(|t| {
                    let t = self.resolve(cx, &t);
                    matches!(t.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::PolyT(_)))
                })
        });
        for (name, uprop) in others.into_iter().chain(generic_values) {
            match lp.get(name) {
                Some(lprop) => self.walk_prop(cx, lprop, uprop, lower_is_literal)?,
                // The indexer of `l` against the property, as two fields, as in subtyping.
                None => {
                    if let Some(ld) = ld {
                        let lfield = PropertyType::OrdinaryField {
                            type_: ld.value.dupe(),
                            polarity: ld.dict_polarity,
                        };
                        let ufield = field_for_indexer(uprop);
                        self.walk_property_type(cx, &lfield, &ufield, lower_is_literal)?;
                    }
                }
            }
        }
        if let Some(ud) = ud {
            for (name, lprop) in lp.iter().filter(|_| lower_is_object) {
                if !up.contains_key(name) {
                    // The property against the indexer of `u`, as two fields, as in subtyping.
                    let ufield = PropertyType::OrdinaryField {
                        type_: ud.value.dupe(),
                        polarity: ud.dict_polarity,
                    };
                    let lfield = field_for_indexer(lprop);
                    self.walk_property_type(cx, &lfield, &ufield, lower_is_literal)?;
                    let reason = type_util::reason_of_t(&ud.key).dupe();
                    let key_t =
                        flow_js_utils::type_of_key_name_with_env(self.env, name.dupe(), &reason);
                    self.walk(cx, &key_t, &ud.key)?;
                }
            }
            // The key and the value of indexers are compared as fields, as subtyping does.
            if let Some(ld) = ld {
                let (lpole, upole) = (ld.dict_polarity, ud.dict_polarity);
                self.walk_field(cx, &ld.key, lpole, &ud.key, upole, lower_is_literal)?;
                self.walk_field(cx, &ld.value, lpole, &ud.value, upole, lower_is_literal)?;
            }
        }
        Ok(())
    }

    fn walk_obj<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        lreason: &Reason,
        lo: &ObjType,
        uo: &ObjType,
    ) -> Result<(), FlowJsException> {
        let lp = cx.find_props(lo.props_tmap.dupe());
        let up = cx.find_props(uo.props_tmap.dupe());
        let ld = obj_type::get_dict_opt(&lo.flags.obj_kind).cloned();
        let ud = obj_type::get_dict_opt(&uo.flags.obj_kind).cloned();
        self.walk_props(
            cx,
            &lp,
            ld.as_ref(),
            &up,
            ud.as_ref(),
            true,
            lo.literal.is_literal(),
        )?;
        // As subtyping flows `mixed`, the unknown properties of an inexact object, to the indexer of
        // the other.
        if matches!(lo.flags.obj_kind, ObjKind::Inexact)
            && let Some(ud) = &ud
        {
            let r = flow_common::reason::mk_reason(
                lreason
                    .desc(true)
                    .cloned()
                    .map(|desc| VirtualReasonDesc::RUnknownUnspecifiedProperty(Arc::new(desc))),
                lreason.loc().dupe(),
            );
            let mixed = Type::new(TypeInner::DefT(
                r,
                DefT::new(DefTInner::MixedT(MixedFlavor::MixedEverything)),
            ));
            self.walk(cx, &mixed, &ud.value)?;
        }
        // The keys of an object without an indexer go to the indexer of the other, as their union:
        // `empty` when it has none besides the other's props.
        if ld.is_none()
            && let Some(ud) = &ud
            && lp.iter().all(|(name, _)| up.contains_key(name))
        {
            let keys = type_util::union_of_ts(lreason.dupe(), Vec::new(), None);
            self.walk(cx, &keys, &ud.key)?;
        }
        if let (Some(lc), Some(uc)) = (lo.call_t, uo.call_t) {
            let lc = cx.find_call(lc);
            let uc = cx.find_call(uc);
            self.walk(cx, &lc, &uc)?;
        }
        Ok(())
    }

    /// The instances an instance inherits from, itself first: its `super_` chain and, for an
    /// interface, every interface it extends (an intersection when there are several, as with merged
    /// declarations).
    fn ancestors<'cx>(&self, cx: &mut Context<'cx>, inst: &InstanceT) -> Vec<InstanceT> {
        let mut result: Vec<InstanceT> = Vec::new();
        let mut work = VecDeque::from([inst.dupe()]);
        // TODO: remove the cap on the number of ancestors.
        while let Some(i) = work.pop_front()
            && result.len() < 32
        {
            if result.iter().any(|r| r.ptr_eq(&i)) {
                continue;
            }
            // The interfaces a class implements (or an interface extends) are nominal ancestors,
            // after its superclass, which the class's members are looked up in first.
            let mut supers = VecDeque::from([i.super_.dupe()]);
            supers.extend(i.implements.iter().duped());
            result.push(i);
            while let Some(sup) = supers.pop_front() {
                let sup = self.resolve(cx, &sup);
                match sup.deref() {
                    TypeInner::IntersectionT(_, rep) => {
                        let members: Vec<Type> = rep.members_iter().duped().collect();
                        for m in members.into_iter().rev() {
                            supers.push_front(m);
                        }
                    }
                    TypeInner::ThisInstanceT(d) => work.push_back(d.instance.dupe()),
                    TypeInner::DefT(_, d) => {
                        if let DefTInner::InstanceT(s) = d.deref() {
                            work.push_back((**s).dupe());
                        }
                    }
                    TypeInner::TypeAppT(_) | TypeInner::ThisTypeAppT(_) => {
                        if let Some(t) = self.expand_typeapp(cx, &sup) {
                            supers.push_front(t);
                        } else if let Some(t) = self.constructed(cx, &sup) {
                            supers.push_front(t);
                        }
                    }
                    _ => {}
                }
            }
        }
        result
    }

    /// What a class extends when its superclass is a value with a construct signature (`class B
    /// extends Promise<T>`, with `declare var Promise: PromiseConstructor`), as subtyping gets it:
    /// what the first construct signature that takes the type arguments returns.
    fn constructed<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> Option<Type> {
        let (type_, targs) = match t.deref() {
            TypeInner::TypeAppT(a) => (a.type_.dupe(), a.targs.dupe()),
            TypeInner::ThisTypeAppT(a) => (
                a.type_.dupe(),
                a.targs.clone().unwrap_or_else(|| Rc::from([])),
            ),
            _ => return None,
        };
        let c = self.resolve(cx, &type_);
        let TypeInner::DefT(_, d) = c.deref() else {
            return None;
        };
        let DefTInner::InstanceT(i) = d.deref() else {
            return None;
        };
        let ctor = cx.find_call(i.inst.inst_construct_t?);
        let ctor = self.resolve(cx, &ctor);
        let signatures: Vec<Type> = match ctor.deref() {
            TypeInner::IntersectionT(_, rep) => rep.members_iter().duped().collect(),
            _ => vec![ctor],
        };
        signatures.iter().find_map(|signature| {
            let signature = self.resolve(cx, signature);
            let TypeInner::DefT(_, d) = signature.deref() else {
                return None;
            };
            match d.deref() {
                DefTInner::FunT(_, f) if targs.is_empty() => Some(f.return_t.dupe()),
                DefTInner::PolyT(p)
                    if targs.len() <= p.tparams.len()
                        && p.tparams
                            .iter()
                            .skip(targs.len())
                            .all(|tp| tp.default.is_some()) =>
                {
                    let mut map = FlowOrdMap::new();
                    for (i, tparam) in p.tparams.iter().enumerate() {
                        let targ = match (targs.get(i), &tparam.default) {
                            (Some(t), _) => t.dupe(),
                            (None, Some(default)) => subst(cx, &map, default),
                            (None, None) => return None,
                        };
                        map.insert(tparam.name.dupe(), targ);
                    }
                    let f = subst(cx, &map, &p.t_out);
                    match self.resolve(cx, &f).deref() {
                        TypeInner::DefT(_, d) if let DefTInner::FunT(_, f) = d.deref() => {
                            Some(f.return_t.dupe())
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        })
    }

    /// The `any` that an instance extends, itself or through an ancestor, if it does.
    fn any_super<'cx>(&self, cx: &mut Context<'cx>, inst: &InstanceT) -> Option<Type> {
        self.ancestors(cx, inst).iter().find_map(|i| {
            let sup = self.resolve(cx, &i.super_);
            let sup = match sup.deref() {
                TypeInner::TypeAppT(a) => self.resolve(cx, &a.type_),
                TypeInner::ThisTypeAppT(a) => self.resolve(cx, &a.type_),
                _ => sup,
            };
            matches!(sup.deref(), TypeInner::AnyT(..)).then_some(sup)
        })
    }

    /// The fields and methods of an instance, including inherited ones.
    fn inst_props<'cx>(
        &self,
        cx: &mut Context<'cx>,
        inst: &InstanceT,
    ) -> properties::PropertiesMap {
        let mut map = BTreeMap::new();
        for i in self.ancestors(cx, inst) {
            for id in [i.inst.own_props.dupe(), i.inst.proto_props.dupe()] {
                for (name, prop) in cx.find_props(id).iter() {
                    map.entry(name.dupe()).or_insert_with(|| prop.dupe());
                }
            }
        }
        properties::PropertiesMap::from_btree_map(map)
    }

    /// The field or method `name` of an instance, its own or an inherited one.
    fn inst_prop<'cx>(
        &self,
        cx: &mut Context<'cx>,
        inst: &InstanceT,
        name: &Name,
    ) -> Option<Property> {
        self.ancestors(cx, inst).iter().find_map(|i| {
            [i.inst.own_props.dupe(), i.inst.proto_props.dupe()]
                .into_iter()
                .find_map(|id| cx.find_props(id).get(name).map(|p| p.dupe()))
        })
    }

    /// The indexer of an instance, its own or an inherited one.
    fn inst_dict<'cx>(&self, cx: &mut Context<'cx>, inst: &InstanceT) -> Option<DictType> {
        self.ancestors(cx, inst)
            .into_iter()
            .find_map(|i| i.inst.inst_dict.clone())
    }

    /// Two type applications, by their expansions.
    fn walk_expansions<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        l: &Type,
        u: &Type,
    ) -> Result<(), FlowJsException> {
        match (self.expand_typeapp(cx, l), self.expand_typeapp(cx, u)) {
            (Some(l), Some(u)) => self.walk(cx, &l, &u),
            (Some(l), None) => self.walk(cx, &l, u),
            (None, Some(u)) => self.walk(cx, l, &u),
            (None, None) => self.give_up(cx, "type application", l, u),
        }
    }

    /// Runs `f` inside the structural comparison of `l` and `u`, unless the walk is already
    /// nested too deeply in the same pair on both sides. As in TypeScript's `isDeeplyNestedType`,
    /// a side counts an enclosing comparison of the same pair only if its type arguments nest no
    /// deeper there: a type argument of the enclosing type (`Iterable<T>` in `Iterable<Iterable<T>>`)
    /// is not a recursive expansion of it.
    fn nested<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        key: Nesting,
        (ldepth, udepth): (usize, usize),
        f: impl FnOnce(&mut Self, &mut Context<'cx>) -> Result<(), FlowJsException>,
    ) -> Result<(), FlowJsException> {
        let enclosing = self.nesting.iter().filter(|(k, _, _)| *k == key);
        let lower = enclosing.clone().filter(|(_, l, _)| ldepth >= *l).count();
        let upper = enclosing.filter(|(_, _, u)| udepth >= *u).count();
        if lower >= MAX_NESTING && upper >= MAX_NESTING {
            return Ok(());
        }
        self.nesting.push((key, ldepth, udepth));
        let result = f(self, cx);
        self.nesting.pop();
        result
    }

    /// How deeply the type arguments of `t` nest, up to a small limit.
    fn depth_of<'cx>(&self, cx: &mut Context<'cx>, t: &Type, level: usize) -> usize {
        if level > MAX_TARG_DEPTH {
            return level;
        }
        let t = self.resolve(cx, t);
        let targs: Vec<Type> = match t.deref() {
            TypeInner::TypeAppT(a) => a.targs.iter().duped().collect(),
            TypeInner::ThisTypeAppT(a) => a.targs.iter().flat_map(|ts| ts.iter().duped()).collect(),
            TypeInner::MaybeT(_, t) => return self.depth_of(cx, t, level),
            TypeInner::OptionalT { type_, .. } => return self.depth_of(cx, type_, level),
            TypeInner::DefT(_, d) => match d.deref() {
                DefTInner::InstanceT(i) => return self.inst_depth(cx, i, level),
                DefTInner::ArrT(a) => match a.deref() {
                    ArrType::ArrayAT(a) => vec![a.elem_t.dupe()],
                    ArrType::TupleAT(a) => vec![a.elem_t.dupe()],
                    ArrType::ROArrayAT(a) => vec![a.elem_t.dupe()],
                },
                _ => return level,
            },
            _ => return level,
        };
        targs
            .iter()
            .map(|t| self.depth_of(cx, t, level + 1))
            .max()
            .unwrap_or(level)
    }

    fn inst_depth<'cx>(&self, cx: &mut Context<'cx>, i: &InstanceT, level: usize) -> usize {
        i.inst
            .type_args
            .iter()
            .map(|(_, _, t, _)| self.depth_of(cx, t, level + 1))
            .max()
            .unwrap_or(level)
    }

    fn walk_inst<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        li: &InstanceT,
        u: &Type,
        ui: &InstanceT,
    ) -> Result<(), FlowJsException> {
        let key = Nesting::Classes(Some(li.inst.class_id.dupe()), Some(ui.inst.class_id.dupe()));
        let depths = (self.inst_depth(cx, li, 0), self.inst_depth(cx, ui, 0));
        let (li, u, ui) = (li.dupe(), u.dupe(), ui.dupe());
        self.nested(cx, key, depths, move |this, cx| {
            this.walk_inst_(cx, &li, &u, &ui)
        })
    }

    fn walk_inst_<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        li: &InstanceT,
        u: &Type,
        ui: &InstanceT,
    ) -> Result<(), FlowJsException> {
        let ancestor = self
            .ancestors(cx, li)
            .into_iter()
            .find(|a| a.inst.class_id == ui.inst.class_id);
        if let Some(a) = ancestor {
            let loose = a.inst.strictness_kind.join(ui.inst.strictness_kind);
            let pairs: Vec<(Type, Type, Polarity)> = a
                .inst
                .type_args
                .iter()
                .zip(ui.inst.type_args.iter())
                .map(|((_, _, l, _), (_, _, u, p))| {
                    (l.dupe(), u.dupe(), unannotated_variance(*p, loose))
                })
                .collect();
            for (l, u, p) in pairs {
                self.walk_by_polarity(cx, &l, &u, p)?;
            }
            return Ok(());
        }
        // A class that extends `any` meets the other instance through that `any`, as in subtyping.
        if let Some(any) = self.any_super(cx, li) {
            return self.walk(cx, &any, u);
        }
        if matches!(ui.inst.inst_kind, InstanceKind::InterfaceKind { .. }) {
            // The call and construct signatures, as in structural subtyping.
            let mut signatures = false;
            let slots: [fn(&InstanceT) -> Option<i32>; 2] =
                [|i| i.inst.inst_call_t, |i| i.inst.inst_construct_t];
            for slot in slots {
                let Some(usig) = self.ancestors(cx, ui).iter().find_map(slot) else {
                    continue;
                };
                signatures = true;
                if let Some(lsig) = self.ancestors(cx, li).iter().find_map(slot) {
                    let (lsig, usig) = (cx.find_call(lsig), cx.find_call(usig));
                    self.walk(cx, &lsig, &usig)?;
                }
            }
            let lp = self.inst_props(cx, li);
            let up = self.inst_props(cx, ui);
            let ld = self.inst_dict(cx, li);
            let ud = self.inst_dict(cx, ui);
            if up.is_empty() && ud.is_none() {
                if signatures {
                    return Ok(());
                }
                let l = Type::new(TypeInner::DefT(
                    type_util::reason_of_t(u).dupe(),
                    DefT::new(DefTInner::InstanceT(Rc::new(li.dupe()))),
                ));
                return self.give_up(cx, "interface without members", &l, u);
            }
            return self.walk_props(cx, &lp, ld.as_ref(), &up, ud.as_ref(), false, false);
        }
        Ok(())
    }

    fn walk_arr<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        la: &ArrType,
        ua: &ArrType,
    ) -> Result<(), FlowJsException> {
        match (la, ua) {
            // As in subtyping, the elements of a literal are compared covariantly, and read-write
            // elements of other tuples are unified.
            (ArrType::TupleAT(lt), ArrType::TupleAT(ut)) => {
                let pairs: Vec<(Type, Type, Polarity)> = lt
                    .elements
                    .iter()
                    .zip(ut.elements.iter())
                    .map(|(l, u)| {
                        let p = if lt.literal.is_literal() {
                            Polarity::Positive
                        } else {
                            u.polarity
                        };
                        (l.t.dupe(), u.t.dupe(), p)
                    })
                    .collect();
                for (l, u, p) in pairs {
                    self.walk_by_polarity(cx, &l, &u, p)?;
                }
                Ok(())
            }
            (_, ArrType::ROArrayAT(u)) => {
                let l = arr_elem(la);
                self.walk(cx, &l, &u.elem_t)
            }
            // A fresh array literal is checked covariantly.
            (ArrType::ArrayAT(l), ArrType::ArrayAT(u)) if l.fresh => {
                self.walk(cx, &l.elem_t, &u.elem_t)
            }
            (ArrType::ArrayAT(l), ArrType::ArrayAT(u)) => {
                self.walk_invariant(cx, &l.elem_t, &u.elem_t)
            }
            (ArrType::TupleAT(l), ArrType::ArrayAT(u)) => {
                self.walk(cx, &l.elem_t, &u.elem_t)?;
                self.walk(cx, &u.elem_t, &l.elem_t)
            }
            (ArrType::ArrayAT(l), ArrType::TupleAT(u)) => match &l.tuple_view {
                Some(view) => {
                    let literal = l.literal.is_literal();
                    let pairs: Vec<(Type, Type, Polarity)> = view
                        .elements
                        .iter()
                        .zip(u.elements.iter())
                        .map(|(l, u)| {
                            let p = if literal {
                                Polarity::Positive
                            } else {
                                u.polarity
                            };
                            (l.t.dupe(), u.t.dupe(), p)
                        })
                        .collect();
                    for (l, u, p) in pairs {
                        self.walk_by_polarity(cx, &l, &u, p)?;
                    }
                    Ok(())
                }
                None => Ok(()),
            },
            _ => Ok(()),
        }
    }

    /// Whether a rest parameter's type is an array or tuple, so that `rest_elem` answers for it.
    fn rest_is_array<'cx>(&self, cx: &mut Context<'cx>, rest: &Type) -> bool {
        let rest = self.resolve(cx, rest);
        matches!(rest.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::ArrT(_)))
    }

    /// The element type of an array of unknown length.
    fn array_elem<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> Option<Type> {
        let t = self.resolve(cx, t);
        let TypeInner::DefT(_, d) = t.deref() else {
            return None;
        };
        let DefTInner::ArrT(a) = d.deref() else {
            return None;
        };
        match a.deref() {
            ArrType::ArrayAT(a) if a.tuple_view.is_none() => Some(a.elem_t.dupe()),
            ArrType::ROArrayAT(a) => Some(a.elem_t.dupe()),
            _ => None,
        }
    }

    /// The type of the `index`th argument that a rest parameter of type `rest` receives.
    fn rest_elem<'cx>(&self, cx: &mut Context<'cx>, rest: &Type, index: usize) -> Option<Type> {
        let rest = self.resolve(cx, rest);
        match rest.deref() {
            TypeInner::DefT(_, d) => match d.deref() {
                DefTInner::ArrT(a) => match a.deref() {
                    ArrType::ArrayAT(a) => match &a.tuple_view {
                        Some(view) => view.elements.get(index).map(|e| e.t.dupe()),
                        None => Some(a.elem_t.dupe()),
                    },
                    ArrType::ROArrayAT(a) => Some(a.elem_t.dupe()),
                    ArrType::TupleAT(t) => t.elements.get(index).map(|e| e.t.dupe()),
                },
                _ => None,
            },
            _ => None,
        }
    }

    /// `l` against a union: the members of the same shape as `l`, and only when there are none, the
    /// member that is a bare marker, if there is exactly one.
    fn walk_to_union<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        l: &Type,
        members: Vec<Type>,
    ) -> Result<(), FlowJsException> {
        if members.iter().any(|m| m.ptr_eq(l)) {
            return Ok(());
        }
        let members = self.flatten_aliases(cx, members);
        let n_members = members.len();
        let (naked, others): (Vec<Type>, Vec<Type>) = members
            .into_iter()
            .partition(|m| self.marker_of(m).is_some());
        let lhead = self.head(cx, l);
        // An instance also has the shape of its nominal ancestors.
        let ancestors: Vec<ALocId> = match self.instance_of(cx, l) {
            Some(i) if matches!(lhead, Head::Inst(_)) => (self.ancestors(cx, &i).iter())
                .map(|a| a.inst.class_id.dupe())
                .collect(),
            _ => Vec::new(),
        };
        // A callable object, instance or class also matches a function type, through its call
        // signature, as in subtyping.
        let callable = matches!(lhead, Head::Obj | Head::Inst(_) | Head::Class)
            && self.has_call_signature(cx, l);
        let mut matching = Vec::new();
        for m in &others {
            let m = self.resolve(cx, m);
            let mhead = self.head(cx, &m);
            // An array type stands for its builtin class against an instance, as subtyping
            // relates an instance to the array type the member concretizes to: an `Array` to an
            // array, a `ReadonlyArray` to a read-only array, and nothing to a tuple.
            let array_class = match (&lhead, &mhead) {
                (Head::Inst(_), Head::Arr) => match self.structure_of(cx, &m)?.as_deref() {
                    Some(TypeInner::DefT(_, d)) if let DefTInner::ArrT(a) = d.deref() => {
                        match a.deref() {
                            ArrType::ArrayAT(_) => Some("Array"),
                            ArrType::ROArrayAT(_) => Some("ReadonlyArray"),
                            ArrType::TupleAT(_) => None,
                        }
                    }
                    _ => None,
                },
                _ => None,
            };
            if lhead != Head::Other
                && (mhead == lhead
                    || matches!(&mhead, Head::Inst(c) if ancestors.contains(c))
                    || array_class.is_some_and(|name| {
                        (ancestors.iter())
                            .any(|c| flow_js_utils::is_builtin_class_id(name, c.dupe(), cx))
                    })
                    || (callable && mhead == Head::Fun))
            {
                matching.push(m);
            }
        }
        // Of objects, drop the members whose literal property, a discriminant, differs from that of
        // `l`.
        if matching.len() > 1 && lhead == Head::Obj {
            let narrowed = self.discriminated(cx, l, &matching);
            if !narrowed.is_empty() {
                matching = narrowed;
            }
        }
        // With a bare marker among the members, a member that `l` does not fit even with the markers
        // open fails in the speculative match of the flow-based solve, which goes on to the marker.
        if matching.len() == 1 && !naked.is_empty() && !self.fits_open(cx, l, &matching[0])? {
            matching.clear();
        }
        // A member of a union that the flow-based solve would try alone: its evidence is kept
        // unless it contradicts itself, as when the flows of that member fail.
        if matching.len() == 1 && n_members > 1 {
            let snapshot = self.snapshot();
            self.walk(cx, l, &matching[0])?;
            if !self.self_consistent(cx, &snapshot.vars)? {
                self.restore(snapshot);
            }
            return Ok(());
        }
        // Like the speculative match of the flow-based solve, the first of several members that
        // has the properties of `l` and whose evidence agrees with the evidence so far. A member
        // without markers matches when `l` is a subtype of it, and is skipped otherwise. If no
        // member matches, the speculative match goes on to the bare marker, if there is one.
        if matching.len() > 1 {
            for m in &matching {
                if !self.has_props_of(cx, l, m) || !self.fits_open(cx, l, m)? {
                    continue;
                }
                if !self.mentions(cx, m) && !self.mentions(cx, l) {
                    if self.within_members(cx, l, std::slice::from_ref(m))? {
                        return Ok(());
                    }
                    continue;
                }
                let snapshot = self.snapshot();
                self.walk(cx, l, m)?;
                if self.agrees_with(cx, &snapshot.vars)? {
                    return Ok(());
                }
                self.restore(snapshot);
            }
            if naked.is_empty() {
                return Ok(());
            }
            matching.clear();
        }
        for m in &matching {
            self.walk(cx, l, m)?;
        }
        if matching.is_empty() {
            match naked.as_slice() {
                // Nothing is inferred to the bare marker from a type that is already one of the other
                // members.
                [_] if self.within_members(cx, l, &others)? => {}
                // A primitive or a literal gives no evidence through an object or interface type.
                [only]
                    if matches!(
                        self.resolve(cx, l).deref(),
                        TypeInner::DefT(_, d) if is_atomic(d)
                    ) =>
                {
                    self.walk(cx, l, only)?;
                }
                // A type that fits another member, an object or interface type, is not inferred
                // to the bare marker. A member that `l` does not fit even with the markers open
                // gives none, as its speculative match in the flow-based solve fails.
                [only] => {
                    let mut fits = false;
                    for m in &others {
                        if self.mentions(cx, m)
                            && matches!(self.head(cx, m), Head::Inst(_) | Head::Obj)
                            && self.fits_open(cx, l, m)?
                        {
                            fits = true;
                            self.walk(cx, l, m)?;
                        }
                    }
                    if !fits {
                        self.walk(cx, l, only)?;
                    }
                }
                // Of several members that may match, as the speculative match of the flow-based
                // solve, the first that gives evidence that agrees with the evidence so far, and
                // none if no member does.
                [] => {
                    let l_mentions = self.mentions(cx, l);
                    let candidates: Vec<Type> = others
                        .iter()
                        .filter(|m| {
                            (l_mentions || self.mentions(cx, m))
                                && lhead.may_match(&self.head(cx, m))
                        })
                        .duped()
                        .collect();
                    if candidates.len() < 2 {
                        // As the speculative match of the flow-based solve, the one member that
                        // may match gives no evidence if `l`, with the solution so far in place of
                        // the markers, does not fit it.
                        let snapshot = self.snapshot();
                        for m in &others {
                            self.walk(cx, l, m)?;
                        }
                        if let [only] = candidates.as_slice()
                            && !self.fits_solution_so_far(cx, l, only)?
                        {
                            self.restore(snapshot);
                        }
                        return Ok(());
                    }
                    for m in &candidates {
                        let snapshot = self.snapshot();
                        let before = self.evidence_count();
                        self.walk(cx, l, m)?;
                        if self.evidence_count() > before && self.agrees_with(cx, &snapshot.vars)? {
                            return Ok(());
                        }
                        self.restore(snapshot);
                    }
                }
                // Of several bare markers, the first, which the speculative match of the
                // flow-based solve tries first, unless `l` is already one of the other members.
                [first, ..] => {
                    if !self.within_members(cx, l, &others)? {
                        self.walk(cx, l, first)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// The members of a union, with a member that stands for a union, such as an alias of one or
    /// an application of a generic alias of one, replaced by its members.
    fn flatten_aliases<'cx>(&self, cx: &mut Context<'cx>, members: Vec<Type>) -> Vec<Type> {
        let mut flat = Vec::with_capacity(members.len());
        let mut work: Vec<Type> = members.into_iter().rev().collect();
        let mut budget = 64;
        while let Some(m) = work.pop() {
            let resolved = self.resolve(cx, &m);
            let union = match resolved.deref() {
                // A large union, such as one of many literals, is kept whole: matching its
                // members one by one costs more than it tells.
                TypeInner::UnionT(_, rep)
                    if flat.len() + work.len() + rep.members_iter().count() <= MAX_FLAT_MEMBERS =>
                {
                    Some(resolved.dupe())
                }
                TypeInner::TypeAppT(a) if self.is_alias(cx, &a.type_) => self
                    .expand_typeapp(cx, &resolved)
                    .map(|expanded| self.resolve(cx, &expanded))
                    .filter(|expanded| matches!(expanded.deref(), TypeInner::UnionT(..))),
                _ => None,
            };
            if budget > 0
                && let Some(union) = union
                && let TypeInner::UnionT(_, rep) = union.deref()
            {
                budget -= 1;
                let inner: Vec<Type> = rep.members_iter().duped().collect();
                work.extend(inner.into_iter().rev());
                continue;
            }
            flat.push(m);
        }
        flat
    }

    /// Whether `l` is a subtype of the union of `members` that mention no marker.
    fn within_members<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        l: &Type,
        members: &[Type],
    ) -> Result<bool, FlowJsException> {
        if self.mentions(cx, l) {
            return Ok(false);
        }
        let mut closed = Vec::new();
        for m in members {
            if !self.mentions(cx, m) {
                closed.push(m.dupe());
            }
        }
        if closed.is_empty() {
            return Ok(false);
        }
        let env = self.env;
        let u = type_util::union_of_ts(type_util::reason_of_t(l).dupe(), closed, None);
        let ok = sandbox(cx, env, self.preserve, |cx| {
            FlowJs::speculative_subtyping_succeeds_with_flow_errors(cx, env, l, &u)
        })?;
        Ok(ok == Some(true))
    }

    /// Whether the object `l` has the properties that the object type `u` requires, and, if `u` is
    /// exact, no others.
    fn has_props_of<'cx>(&self, cx: &mut Context<'cx>, l: &Type, u: &Type) -> bool {
        let (Some((lp, _)), Some((up, exact))) =
            (self.object_shape(cx, l), self.object_shape(cx, u))
        else {
            return true;
        };
        let required = up.iter().all(|(name, p)| {
            lp.get(name).is_some()
                || property::read_t(p)
                    .is_none_or(|t| matches!(t.deref(), TypeInner::OptionalT { .. }))
        });
        required && (!exact || lp.iter().all(|(name, _)| up.get(name).is_some()))
    }

    /// Whether `l` is a subtype of `u` with the markers in either taking any type, as the
    /// speculative match of the flow-based solve would find it with its type variables.
    fn fits_open<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        l: &Type,
        u: &Type,
    ) -> Result<bool, FlowJsException> {
        let mut names = self.markers_in(cx, l);
        names.extend(self.markers_in(cx, u));
        let reason = type_util::reason_of_t(u).dupe();
        let map: FlowOrdMap<SubstName, Type> = names
            .into_iter()
            .map(|name| {
                (
                    name,
                    placeholder_t::why(reason.dupe(), mixed_t::why(reason.dupe())),
                )
            })
            .collect();
        let (l, u) = (subst(cx, &map, l), subst(cx, &map, u));
        let env = self.env;
        // The types here are made for this question alone: the cache entries of its flows would
        // never be read again.
        let errors = cx.errors();
        let deferred = env.deferred_error_count();
        let level = cx.constraint_cache().level_count();
        cx.constraint_cache_mut().push_level();
        let ok = FlowJs::speculative_subtyping_succeeds_with_flow_errors(cx, env, &l, &u);
        cx.constraint_cache_mut().truncate_to(level);
        cx.reset_errors(errors);
        env.truncate_deferred_errors(deferred);
        Ok(instantiation_solver::sandboxed(ok)? == Some(true))
    }

    /// Whether `l` is a subtype of `u` with the solution so far in place of the markers in either,
    /// as the speculative match of the flow-based solve would find it with its type variables.
    fn fits_solution_so_far<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        l: &Type,
        u: &Type,
    ) -> Result<bool, FlowJsException> {
        let mut names = self.markers_in(cx, l);
        names.extend(self.markers_in(cx, u));
        let reason = type_util::reason_of_t(u).dupe();
        let map: FlowOrdMap<SubstName, Type> = names
            .into_iter()
            .map(|name| {
                let t = self.current_solution(cx, &name, &reason);
                (name, t)
            })
            .collect();
        let (l, u) = (subst(cx, &map, l), subst(cx, &map, u));
        let env = self.env;
        let ok = sandbox(cx, env, self.preserve, |cx| {
            FlowJs::speculative_subtyping_succeeds_with_flow_errors(cx, env, &l, &u)
        })?;
        Ok(ok == Some(true))
    }

    /// Whether every member of an intersection is a function: a list of overloads.
    fn all_funs<'cx>(&self, cx: &mut Context<'cx>, rep: &inter_rep::InterRep) -> bool {
        rep.members_iter()
            .all(|m| matches!(self.head(cx, m), Head::Fun))
    }

    fn evidence_count(&self) -> usize {
        self.vars
            .values()
            .map(|b| b.lower.len() + b.upper.len() + b.exact.len())
            .sum()
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            vars: self.vars.clone(),
            seen_len: self.seen_log.len(),
            deferred: self.deferred.clone(),
            pending_spreads: self.pending_spreads.clone(),
            spread_tuple_pairs: self.spread_tuple_pairs.clone(),
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.vars = snapshot.vars;
        while self.seen_log.len() > snapshot.seen_len {
            match self.seen_log.pop() {
                Some(SeenChange::Added(key)) => {
                    self.seen.remove(&key);
                }
                Some(SeenChange::Removed(key, pair)) => {
                    self.seen.insert(key, pair);
                }
                None => break,
            }
        }
        self.deferred = snapshot.deferred;
        self.pending_spreads = snapshot.pending_spreads;
        self.spread_tuple_pairs = snapshot.spread_tuple_pairs;
    }

    /// Whether the evidence added since `before` agrees with the evidence there and with itself, as
    /// flows into a type variable meet its bounds: each lower bound of a marker, or its first exact
    /// one, is a subtype of each exact or upper bound the marker got since, and each lower or exact
    /// bound it got since is a subtype of its earlier upper bounds and first exact one.
    fn agrees_with<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        before: &BTreeMap<SubstName, Bounds>,
    ) -> Result<bool, FlowJsException> {
        let env = self.env;
        let pairs: Vec<(Type, Type)> = self
            .vars
            .iter()
            .filter_map(|(name, now)| Some((before.get(name)?, now)))
            .flat_map(|(old, now)| {
                let lowers: Vec<Type> =
                    types(&old.lower).chain(old.exact.first()).duped().collect();
                let uppers: Vec<Type> = now.exact[old.exact.len()..]
                    .iter()
                    .chain(types(&now.upper[old.upper.len()..]))
                    .duped()
                    .collect();
                let new_lowers: Vec<Type> = types(&now.lower[old.lower.len()..])
                    .chain(now.exact[old.exact.len()..].iter())
                    .duped()
                    .collect();
                let old_uppers: Vec<Type> =
                    types(&old.upper).chain(old.exact.first()).duped().collect();
                let forward: Vec<(Type, Type)> = lowers
                    .iter()
                    .chain(new_lowers.iter())
                    .flat_map(|l| uppers.iter().map(move |u| (l.dupe(), u.dupe())))
                    .filter(|(l, u)| !l.ptr_eq(u))
                    .collect();
                let backward: Vec<(Type, Type)> = new_lowers
                    .iter()
                    .flat_map(|l| old_uppers.iter().map(move |u| (l.dupe(), u.dupe())))
                    .collect();
                forward.into_iter().chain(backward)
            })
            .collect();
        for (l, u) in pairs {
            if self.mentions(cx, &l) || self.mentions(cx, &u) {
                continue;
            }
            let ok = sandbox(cx, env, self.preserve, |cx| {
                FlowJs::speculative_subtyping_succeeds_with_flow_errors(cx, env, &l, &u)
            })?;
            if ok != Some(true) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Whether the evidence added since `before` agrees with itself: each new lower or exact
    /// bound of a marker is a subtype of each of its new upper or exact bounds.
    fn self_consistent<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        before: &BTreeMap<SubstName, Bounds>,
    ) -> Result<bool, FlowJsException> {
        let env = self.env;
        let pairs: Vec<(Type, Type)> = self
            .vars
            .iter()
            .filter_map(|(name, now)| Some((before.get(name)?, now)))
            .flat_map(|(old, now)| {
                let new_exact = &now.exact[old.exact.len()..];
                let lowers: Vec<Type> = types(&now.lower[old.lower.len()..])
                    .chain(new_exact)
                    .duped()
                    .collect();
                let uppers: Vec<Type> = new_exact
                    .iter()
                    .chain(types(&now.upper[old.upper.len()..]))
                    .duped()
                    .collect();
                lowers
                    .iter()
                    .flat_map(|l| uppers.iter().map(move |u| (l.dupe(), u.dupe())))
                    .filter(|(l, u)| !l.ptr_eq(u))
                    .collect::<Vec<_>>()
            })
            .collect();
        for (l, u) in pairs {
            if self.mentions(cx, &l) || self.mentions(cx, &u) {
                continue;
            }
            let ok = sandbox(cx, env, self.preserve, |cx| {
                FlowJs::speculative_subtyping_succeeds_with_flow_errors(cx, env, &l, &u)
            })?;
            if ok != Some(true) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn head<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> Head {
        self.head_at(cx, t, 0)
    }

    /// The instance that `t` is, or that an application in `t` stands for.
    fn instance_of<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> Option<InstanceT> {
        let t = self.resolve(cx, t);
        let t = match t.deref() {
            TypeInner::TypeAppT(_) | TypeInner::ThisTypeAppT(_) => self.expand_typeapp(cx, &t)?,
            _ => t,
        };
        match self.resolve(cx, &t).deref() {
            TypeInner::DefT(_, d) if let DefTInner::InstanceT(i) = d.deref() => Some((**i).dupe()),
            TypeInner::ThisInstanceT(d) => Some(d.instance.dupe()),
            _ => None,
        }
    }

    /// The head of `t`, through at most `MAX_DEPTH` aliases, as `structure_of` follows them.
    fn head_at<'cx>(&self, cx: &mut Context<'cx>, t: &Type, depth: u32) -> Head {
        if depth > MAX_DEPTH {
            return Head::Other;
        }
        let t = self.resolve(cx, t);
        match t.deref() {
            TypeInner::DefT(_, d) => match d.deref() {
                DefTInner::NumGeneralT(_)
                | DefTInner::SingletonNumT { .. }
                | DefTInner::NumericStrKeyT(_) => Head::Num,
                DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. } => Head::Str,
                DefTInner::BoolGeneralT | DefTInner::SingletonBoolT { .. } => Head::Bool,
                DefTInner::BigIntGeneralT(_) | DefTInner::SingletonBigIntT { .. } => Head::BigInt,
                DefTInner::NullT => Head::Null,
                DefTInner::VoidT => Head::Void,
                DefTInner::SymbolT | DefTInner::UniqueSymbolT(_) => Head::Symbol,
                DefTInner::FunT(..) | DefTInner::PolyT(_) => Head::Fun,
                DefTInner::ObjT(_) => Head::Obj,
                DefTInner::ArrT(_) => Head::Arr,
                DefTInner::InstanceT(i) => Head::Inst(i.inst.class_id.dupe()),
                DefTInner::ClassT(_) => Head::Class,
                _ => Head::Other,
            },
            TypeInner::TypeAppT(_) | TypeInner::ThisTypeAppT(_) => {
                match self.expand_typeapp(cx, &t) {
                    Some(e) => self.head_at(cx, &e, depth + 1),
                    None => Head::Other,
                }
            }
            TypeInner::ThisInstanceT(d) => Head::Inst(d.instance.inst.class_id.dupe()),
            TypeInner::EvalT {
                type_, defer_use_t, ..
            } => match defer_use_t.2.deref() {
                Destructor::ReadOnlyType => self.head_at(cx, type_, depth + 1),
                Destructor::SpreadType(..) => Head::Obj,
                _ => Head::Other,
            },
            _ => Head::Other,
        }
    }

    /// The type arguments of an application of `c`, with the defaults of the omitted ones.
    fn full_targs<'cx>(&self, cx: &mut Context<'cx>, c: &Type, targs: &[Type]) -> Vec<Type> {
        let c = self.resolve(cx, c);
        let TypeInner::DefT(_, d) = c.deref() else {
            return targs.to_vec();
        };
        let DefTInner::PolyT(p) = d.deref() else {
            return targs.to_vec();
        };
        let mut map = FlowOrdMap::new();
        let mut result = Vec::with_capacity(p.tparams.len());
        for (i, tparam) in p.tparams.iter().enumerate() {
            let targ = match targs.get(i) {
                Some(t) => t.dupe(),
                None => match &tparam.default {
                    Some(default) => subst(cx, &map, default),
                    None => break,
                },
            };
            map.insert(tparam.name.dupe(), targ.dupe());
            result.push(targ);
        }
        result
    }

    /// Whether a bound applies `c`, through other type parameters and aliases of an application.
    fn bound_applies<'cx>(
        &self,
        cx: &mut Context<'cx>,
        bound: &Type,
        c: &Type,
        depth: u32,
    ) -> bool {
        if depth > MAX_DEPTH {
            return false;
        }
        let bound = self.resolve(cx, bound);
        match bound.deref() {
            TypeInner::GenericT(box GenericTData { bound, .. }) => {
                self.bound_applies(cx, bound, c, depth + 1)
            }
            TypeInner::TypeAppT(a) if self.same_typeapp(cx, &a.type_, c).is_some() => true,
            TypeInner::TypeAppT(_) => self
                .expand_typeapp(cx, &bound)
                .is_some_and(|t| self.bound_applies(cx, &t, c, depth + 1)),
            _ => false,
        }
    }

    /// The polarities of the type parameters if `a` and `b` are the same polymorphic type.
    fn same_typeapp<'cx>(
        &self,
        cx: &mut Context<'cx>,
        a: &Type,
        b: &Type,
    ) -> Option<Vec<Polarity>> {
        let a = self.resolve(cx, a);
        let b = self.resolve(cx, b);
        match (a.deref(), b.deref()) {
            (TypeInner::DefT(_, da), TypeInner::DefT(_, db)) => match (da.deref(), db.deref()) {
                // As in subtyping, an alias of a utility type is compared by its expansion, and a
                // type parameter without a variance is invariant, but covariant in TypeScript.
                (DefTInner::PolyT(pa), DefTInner::PolyT(pb))
                    if pa.id == pb.id && !flow_js_utils::wraps_utility_type(cx, &pa.t_out) =>
                {
                    let loose = pa.strictness_kind.join(pb.strictness_kind);
                    Some(
                        pa.tparams
                            .iter()
                            .map(|tp| unannotated_variance(tp.polarity, loose))
                            .collect(),
                    )
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// The properties of an intersection of objects and instances, each from the first member that
    /// has it; `None` if a member is something else.
    fn intersection_props<'cx>(
        &self,
        cx: &mut Context<'cx>,
        rep: &inter_rep::InterRep,
    ) -> Option<properties::PropertiesMap> {
        let mut map = BTreeMap::new();
        let mut work: Vec<Type> = rep.members_iter().duped().collect();
        work.reverse();
        let mut steps = 0;
        while let Some(t) = work.pop() {
            steps += 1;
            if steps > 64 {
                return None;
            }
            let t = self.resolve(cx, &t);
            let props = match t.deref() {
                TypeInner::IntersectionT(_, r) => {
                    let mut members: Vec<Type> = r.members_iter().duped().collect();
                    members.reverse();
                    work.extend(members);
                    continue;
                }
                TypeInner::TypeAppT(_) => {
                    work.push(self.expand_typeapp(cx, &t)?);
                    continue;
                }
                TypeInner::DefT(_, d) => match d.deref() {
                    DefTInner::ObjT(o) => cx.find_props(o.props_tmap.dupe()),
                    DefTInner::InstanceT(i) => self.inst_props(cx, i),
                    _ => return None,
                },
                _ => return None,
            };
            for (name, prop) in props.iter() {
                map.entry(name.dupe()).or_insert_with(|| prop.dupe());
            }
        }
        Some(properties::PropertiesMap::from_btree_map(map))
    }

    /// The properties of an object type and whether it is exact, through aliases and `Readonly`.
    fn object_shape<'cx>(
        &self,
        cx: &mut Context<'cx>,
        t: &Type,
    ) -> Option<(properties::PropertiesMap, bool)> {
        self.object_shape_at(cx, t, 0)
    }

    fn object_shape_at<'cx>(
        &self,
        cx: &mut Context<'cx>,
        t: &Type,
        depth: u32,
    ) -> Option<(properties::PropertiesMap, bool)> {
        if depth > MAX_DEPTH {
            return None;
        }
        let t = self.resolve(cx, t);
        match t.deref() {
            TypeInner::TypeAppT(_) => {
                let e = self.expand_typeapp(cx, &t)?;
                self.object_shape_at(cx, &e, depth + 1)
            }
            TypeInner::EvalT {
                type_, defer_use_t, ..
            } if matches!(defer_use_t.2.deref(), Destructor::ReadOnlyType) => {
                self.object_shape_at(cx, type_, depth + 1)
            }
            TypeInner::DefT(_, d) if let DefTInner::ObjT(o) = d.deref() => Some((
                cx.find_props(o.props_tmap.dupe()),
                matches!(o.flags.obj_kind, ObjKind::Exact),
            )),
            _ => None,
        }
    }

    /// The literal type of a readable property, if it has one.
    fn literal_prop<'cx>(&self, cx: &mut Context<'cx>, prop: &Property) -> Option<UnionEnum> {
        let t = property::read_t(prop)?;
        union_rep::canon(&self.resolve(cx, &t))
    }

    /// The members whose literal properties agree with those of `l`, and that have every
    /// property of `l` if they are exact.
    fn discriminated<'cx>(&self, cx: &mut Context<'cx>, l: &Type, members: &[Type]) -> Vec<Type> {
        let Some((lp, _)) = self.object_shape(cx, l) else {
            return members.to_vec();
        };
        let literals: Vec<(Name, UnionEnum)> = lp
            .iter()
            .filter_map(|(name, prop)| Some((name.dupe(), self.literal_prop(cx, prop)?)))
            .collect();
        members
            .iter()
            .filter(|m| {
                let Some((mp, exact)) = self.object_shape(cx, m) else {
                    return true;
                };
                (!exact || lp.iter().all(|(name, _)| mp.get(name).is_some()))
                    && literals.iter().all(|(name, lit)| match mp.get(name) {
                        Some(prop) => self.literal_prop(cx, prop).is_none_or(|ml| &ml == lit),
                        None => true,
                    })
            })
            .duped()
            .collect()
    }

    /// Whether `c` is a polymorphic type alias.
    fn is_alias<'cx>(&self, cx: &mut Context<'cx>, c: &Type) -> bool {
        let c = self.resolve(cx, c);
        let TypeInner::DefT(_, d) = c.deref() else {
            return false;
        };
        let DefTInner::PolyT(p) = d.deref() else {
            return false;
        };
        matches!(
            p.t_out.deref(),
            TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::TypeT(TypeTKind::TypeAliasKind, _))
        )
    }

    /// The members of a union that apply the alias or class `c`, if any.
    fn same_typeapp_members<'cx>(
        &self,
        cx: &mut Context<'cx>,
        c: &Type,
        rep: &union_rep::UnionRep,
    ) -> Option<Vec<Type>> {
        let members: Vec<Type> = rep
            .members_iter()
            .filter(|m| {
                matches!(
                    self.resolve(cx, m).deref(),
                    TypeInner::TypeAppT(b) if self.same_typeapp(cx, c, &b.type_).is_some()
                )
            })
            .duped()
            .collect();
        if members.is_empty() {
            None
        } else {
            Some(members)
        }
    }

    /// Walks the config of a component with only a rest parameter (`rest`) against `other` as if
    /// it were `rest`, if that gives every marker of `rest` evidence; otherwise undoes the walk.
    fn walk_config_as_rest<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        a: &Type,
        b: &Type,
        rest_is_upper: bool,
    ) -> Result<bool, FlowJsException> {
        let rest = if rest_is_upper { b } else { a };
        let markers = self.markers_in(cx, rest);
        let snapshot = self.snapshot();
        self.walk(cx, a, b)?;
        let all = markers.iter().all(|m| {
            let bounds = &self.vars[m];
            !(bounds.lower.is_empty() && bounds.upper.is_empty() && bounds.exact.is_empty())
        });
        if !all {
            self.restore(snapshot);
        }
        Ok(all)
    }

    /// An application of an alias of an object type, or of a read-only one.
    fn is_object_alias<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> bool {
        if !matches!(t.deref(), TypeInner::TypeAppT(_)) {
            return false;
        }
        let Some(expanded) = self.expand_typeapp(cx, t) else {
            return false;
        };
        match self.resolve(cx, &expanded).deref() {
            TypeInner::DefT(_, d) => matches!(d.deref(), DefTInner::ObjT(_)),
            TypeInner::EvalT { defer_use_t, .. } => {
                matches!(defer_use_t.2.deref(), Destructor::ReadOnlyType)
            }
            _ => false,
        }
    }

    /// The instance or alias body that a type application stands for, by substitution only.
    fn expand_typeapp<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> Option<Type> {
        if let Some((_, expanded)) = self.expand_cache.borrow().get(&addr(t)) {
            return Some(expanded.dupe());
        }
        let expanded = self.expand_typeapp_uncached(cx, t)?;
        self.expand_cache
            .borrow_mut()
            .insert(addr(t), (t.dupe(), expanded.dupe()));
        self.expanded_from
            .borrow_mut()
            .insert(addr(&expanded), t.dupe());
        Some(expanded)
    }

    fn expand_typeapp_uncached<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> Option<Type> {
        let (type_, targs, from_value, this_t, use_op) = match t.deref() {
            TypeInner::TypeAppT(a) => (
                a.type_.dupe(),
                Some(a.targs.dupe()),
                a.from_value,
                None,
                Some(a.use_op.dupe()),
            ),
            TypeInner::ThisTypeAppT(a) => (
                a.type_.dupe(),
                a.targs.clone(),
                false,
                Some(a.this_t.dupe()),
                None,
            ),
            _ => return None,
        };
        let c = self.resolve(cx, &type_);
        // A class imported as a type.
        let c = match c.deref() {
            TypeInner::DefT(_, d) if let DefTInner::TypeT(_, inner) = d.deref() => {
                self.resolve(cx, inner)
            }
            _ => c,
        };
        let TypeInner::DefT(_, d) = c.deref() else {
            return None;
        };
        let p = match d.deref() {
            DefTInner::PolyT(p) => p,
            // A class without type parameters, as the `extends` of an interface.
            DefTInner::ClassT(inner) => {
                return match inner.deref() {
                    TypeInner::ThisInstanceT(d) if let Some(this) = &this_t => {
                        Some(this_specialize(cx, d, this))
                    }
                    TypeInner::ThisInstanceT(_) => Some(self.normalize(cx, inner)),
                    TypeInner::DefT(_, d2) if matches!(d2.deref(), DefTInner::InstanceT(_)) => {
                        Some(inner.dupe())
                    }
                    _ => None,
                };
            }
            _ => return None,
        };
        let targs = targs.unwrap_or_else(|| Rc::from([]));
        let mut map = FlowOrdMap::new();
        for (i, tparam) in p.tparams.iter().enumerate() {
            let targ = match (targs.get(i), &tparam.default) {
                (Some(t), _) => t.dupe(),
                // A default may mention the type parameters before it.
                (None, Some(default)) => subst(cx, &map, default),
                // Too few type arguments, an error of the annotation.
                (None, None) => any_t::error(type_util::reason_of_t(t).dupe()),
            };
            map.insert(tparam.name.dupe(), targ);
        }
        // `NoInfer<T>` in the body gives no evidence, as in a signature. The expansion takes the
        // use of the application, as in subtyping.
        let body = subst_markers_with_use_op(cx, use_op, &map, &p.t_out);
        let body = self.resolve(cx, &body);
        match body.deref() {
            TypeInner::DefT(_, d) => match d.deref() {
                DefTInner::ClassT(inner) => match inner.deref() {
                    TypeInner::ThisInstanceT(d) if let Some(this) = &this_t => {
                        Some(this_specialize(cx, d, this))
                    }
                    TypeInner::ThisInstanceT(_) => Some(self.normalize(cx, inner)),
                    TypeInner::DefT(_, d2) if matches!(d2.deref(), DefTInner::InstanceT(_)) => {
                        Some(inner.dupe())
                    }
                    // The class of an imported class: an application of the class.
                    TypeInner::TypeAppT(_) => self.expand_typeapp(cx, inner),
                    _ => None,
                },
                DefTInner::TypeT(_, body) => Some(body.dupe()),
                // `typeof f<T>`: the instantiated value.
                _ if from_value => Some(body.dupe()),
                _ => None,
            },
            // The instantiated value, such as the type of a property of a mapped type for a key.
            _ if from_value => Some(body.dupe()),
            _ => None,
        }
    }

    /// A destructor without markers, evaluated in a sandbox: its errors belong to the check of
    /// the operation, if it evaluates it. `None` if it fails under speculation.
    fn eval_sandboxed<'cx>(
        &self,
        cx: &mut Context<'cx>,
        use_op: &UseOp,
        reason: &Reason,
        type_: &Type,
        d: &Destructor,
        id: &eval::Id,
    ) -> Result<Option<Type>, FlowJsException> {
        let errors = cx.errors();
        let result = run_keeping_clean_flows(cx, self.env, self.preserve, |cx| {
            FlowJs::mk_type_destructor_with_env(
                cx,
                self.env,
                DepthTrace::dummy_trace(),
                use_op.dupe(),
                reason,
                type_,
                d,
                id.dupe(),
            )
        });
        cx.reset_errors(errors);
        instantiation_solver::sandboxed(result)
    }

    fn walk_to_eval<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        l: &Type,
        u: &Type,
    ) -> Result<(), FlowJsException> {
        let TypeInner::EvalT {
            type_,
            defer_use_t,
            id,
        } = u.deref()
        else {
            return Ok(());
        };
        let TypeDestructorTInner(use_op, reason, d) = defer_use_t.deref();
        if !self.mentions(cx, u) {
            let Some(result) = self.eval_sandboxed(cx, use_op, reason, type_, d, id)? else {
                return Ok(());
            };
            return self.walk(cx, l, &result);
        }
        // The hint says nothing through a `NonNullable` of a marker, as in the flow-based solve.
        if self.in_hint && matches!(d.deref(), Destructor::NonMaybeType) {
            return Ok(());
        }
        match d.deref() {
            // The read-only object, whose fields are compared covariantly.
            Destructor::ReadOnlyType
                if matches!(
                    self.resolve(cx, type_).deref(),
                    TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::ObjT(_))
                ) && let Some(result) =
                    self.eval_sandboxed(cx, use_op, reason, type_, d, id)? =>
            {
                let result = self.resolve(cx, &result);
                self.walk(cx, l, &result)
            }
            // `Readonly<A<T>>` of an alias of an object type: the read-only object of its body.
            Destructor::ReadOnlyType
                if let Some(body) = self.object_alias_body(cx, type_)
                    && let Some(result) = self.evaluate(cx, &body, defer_use_t)? =>
            {
                let result = self.resolve(cx, &result);
                self.walk(cx, l, &result)
            }
            // `Readonly<A<T>>` of an alias of a union: the union of the read-only members, so that
            // their fields are compared covariantly.
            Destructor::ReadOnlyType
                if let Some(body) = self.union_alias_body(cx, type_)
                    && let Some(result) = self.evaluate(cx, &body, defer_use_t)?
                    && self.is_structure_or_union_of(cx, &result) =>
            {
                self.walk(cx, l, &result)
            }
            // `Readonly<{...A, ...B}>` where no operand is a marker: made in full, so that the
            // fields of the spread are compared covariantly. (A spread of a union makes a union.)
            Destructor::ReadOnlyType
                if !self.mentions(cx, l)
                    && self.is_spread_of_types(type_)
                    && let Some(result) = self.evaluate(cx, type_, defer_use_t)?
                    && self.is_structure_or_union_of(cx, &result) =>
            {
                self.walk(cx, l, &result)
            }
            // What flows to `Partial<T>` says only which properties `T` may have: an upper bound
            // of `T`, as the flow-based solve reads it, of lower priority than direct evidence.
            Destructor::PartialType | Destructor::RequiredType
                if let Some(name) = self.marker_of(&self.resolve(cx, type_)) =>
            {
                self.add_upper_at(cx, &name, l, Priority::Reversal);
                Ok(())
            }
            Destructor::ReadOnlyType
            | Destructor::PartialType
            | Destructor::RequiredType
            | Destructor::ExactType
            | Destructor::NonMaybeType
            | Destructor::ReactDRO(_) => self.walk(cx, l, type_),
            // The config of a component of component syntax with only a rest parameter of an
            // alias type is that type.
            Destructor::ReactCheckComponentConfig { props }
                if props.is_empty()
                    && self.is_object_alias(cx, type_)
                    && self.walk_config_as_rest(cx, l, type_, true)? =>
            {
                Ok(())
            }
            Destructor::ConditionalType(c) => match self.eval_conditional(cx, u, true)? {
                Some(branch) => self.walk(cx, l, &branch),
                None if !self.second_pass => {
                    self.deferred.push((l.dupe(), u.dupe()));
                    Ok(())
                }
                None => {
                    let (t, f) = branches(cx, type_, c);
                    self.walk(cx, l, &t)?;
                    self.walk(cx, l, &f)
                }
            },
            // An indexed access of an object type with markers inside is indexed first:
            // `{a: T}['a']` is `T`. So is one of an alias of an object type, as the flow-based
            // solve evaluates it. (`T[K]` itself, like a mapped type, still falls back.)
            Destructor::PropertyType { .. }
            | Destructor::ElementType { .. }
            | Destructor::OptionalIndexedAccessNonMaybeType { .. }
            | Destructor::OptionalIndexedAccessResultType { .. }
                if let Some(object) = self.structure_of(cx, type_)? =>
            {
                match self.evaluate(cx, &object, defer_use_t)? {
                    Some(result) => self.walk(cx, l, &result),
                    None => match self.declared_property(cx, &object, d) {
                        Some(t) => self.walk(cx, l, &t),
                        None => self.give_up(cx, "destructor", l, u),
                    },
                }
            }
            // `l <: [...prefix, ...T]`: the elements of `l` go to the prefix, and `T` gets the rest,
            // as the flow-based solve's reversal (`ArrRestT`) computes it.
            Destructor::SpreadTupleType(st) if let Some(items) = spread_tuple_items(st, type_) => {
                self.walk_spread_tuple(cx, items, l, u, false)
            }
            // `l <: Rest<T, R>`: `T` gets `l` with the props of `R` it lacks, as the flow-based
            // solve's reversal computes it, as an upper bound of lower priority than direct
            // evidence (a return hint).
            Destructor::RestType(_, t_rest)
                if let Some(name) = self.marker_of(type_)
                    && !self.mentions(cx, l) =>
            {
                if self.rest_waits(cx, d) {
                    self.deferred.push((l.dupe(), u.dupe()));
                    return Ok(());
                }
                // The props to remove may depend on other markers (`Omit<T, keyof U>`): those
                // with evidence take their solution so far, as in the flow-based solve.
                let t_rest = &self.with_evidence(cx, t_rest, reason);
                let env = self.env;
                let Some(reversed) = sandbox(cx, env, self.preserve, |cx| {
                    reverse_rest(cx, env, reason, t_rest, l)
                })?
                .flatten() else {
                    return self.give_up(cx, "destructor", l, u);
                };
                let reversed = self.resolve(cx, &reversed);
                self.add_upper_at(cx, &name, &reversed, Priority::Reversal);
                Ok(())
            }
            // `l <: {...T, ...rest}`: what `l` has besides the props of the other operands, as
            // the flow-based solve's reversal computes it, bounds `T` from above, as the
            // flow-based solve reads it when it merges the upper bounds of `T`, with the priority of
            // a reversal.
            Destructor::SpreadType(box DestructorSpreadTypeData(target, ..))
                if !self.mentions(cx, l)
                    && let Some((bounded, d)) = self.reversal_target(cx, type_, d)
                    && let Destructor::SpreadType(box DestructorSpreadTypeData(
                        _,
                        todo_rev,
                        head_slice,
                    )) = &d =>
            {
                let acc = match head_slice {
                    Some(slice) => flow_data_structure_wrapper::list::FlowOcamlList::unit(
                        object::spread::AccElement::InlineSlice(slice.clone()),
                    ),
                    None => flow_data_structure_wrapper::list::FlowOcamlList::new(),
                };
                let env = self.env;
                let Some(reversed) = sandbox(cx, env, self.preserve, |cx| {
                    reverse_spread(cx, env, reason, todo_rev, &acc, l, target.literal())
                })?
                .flatten() else {
                    return self.give_up(cx, "destructor", l, u);
                };
                let reversed = self.resolve(cx, &reversed);
                self.add_reversal(cx, &bounded, &reversed)
            }
            // `l <: {...A, ...B}` where no operand is a marker: `l` is walked against the spread
            // made in full, with the markers inside its props, as the flow-based solve flows `l`
            // into the evaluated spread. (A spread of a union makes a union, which the flow-based
            // solve matches by speculation.)
            Destructor::SpreadType(box DestructorSpreadTypeData(_, todo_rev, _))
                if self.marker_of(type_).is_none()
                    && todo_rev.iter().all(|operand| match operand {
                        object::spread::Operand::Type(t) => self.marker_of(t).is_none(),
                        object::spread::Operand::Slice(_) => true,
                    })
                    && !self.mentions(cx, l)
                    && let Some(result) = self.evaluate(cx, type_, defer_use_t)?
                    && self.is_structure_or_union_of(cx, &result) =>
            {
                self.walk(cx, l, &result)
            }
            // `l <: config(T)`, the config of a component of component syntax: `T` gets the
            // flow-based solve's reversal of `l` as an upper bound, with the priority of a reversal.
            Destructor::ReactCheckComponentConfig { .. } | Destructor::EnumType
                if let Some(name) = self.marker_through_identity(cx, type_)
                    && !self.mentions(cx, l) =>
            {
                let l = self.evaluated(cx, l)?;
                match self.reverse_destructor(cx, reason, d, &l)? {
                    Some(reversed) => self.add_upper_at(cx, &name, &reversed, Priority::Reversal),
                    None => self.give_up(cx, "destructor", &l, u)?,
                }
                Ok(())
            }
            // Operands with markers inside: the destructor evaluated with the markers rigid, if
            // it makes a structure. Otherwise, no evidence.
            Destructor::SpreadType(_)
            | Destructor::RestType(..)
            | Destructor::ReactCheckComponentConfig { .. }
                if !self.mentions(cx, l) =>
            {
                if self.spread_waits(cx, type_, d) {
                    self.deferred.push((l.dupe(), u.dupe()));
                    return Ok(());
                }
                if let Destructor::ReactCheckComponentConfig { props } = d.deref()
                    && props.is_empty()
                {
                    return self.walk(cx, l, type_);
                }
                match self.evaluate(cx, type_, defer_use_t)? {
                    Some(result) if self.is_structure(cx, &result) => self.walk(cx, l, &result),
                    _ => self.give_up(cx, "destructor", l, u),
                }
            }
            Destructor::ReactCheckComponentConfig { props } if props.is_empty() => {
                self.walk(cx, l, type_)
            }
            // Other destructors of markers, such as `T[K]` or a mapped type: the destructor
            // evaluated with the markers rigid, if that decides it (`ComponentProps<X<T>>['a']`).
            // Otherwise it is stuck on a marker, and says nothing about it.
            _ => match self.evaluate(cx, type_, defer_use_t)? {
                Some(result) => self.walk(cx, l, &result),
                None => self.give_up(cx, "destructor", l, u),
            },
        }
    }

    /// Renders types, as subtyping relates them (`renders_kit`). `renders A` sends `A` to the
    /// other render type; the render type of a component or an intrinsic, or `React.Node`, goes to
    /// `T` in `renders T` (a component's, to its super render type if it does not fit `T`); an array
    /// sends its elements to `renders* T`, and other types go to `T` (an element, if it fits `T`).
    fn walk_renders<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        l: &Type,
        u: &Type,
    ) -> Result<(), FlowJsException> {
        fn form(t: &Type) -> Option<Rc<CanonicalRendersForm>> {
            match t.deref() {
                TypeInner::DefT(_, d) => match d.deref() {
                    DefTInner::RendersT(f) => Some(f.dupe()),
                    _ => None,
                },
                _ => None,
            }
        }
        match (form(l), form(u)) {
            (Some(lf), Some(uf)) => match (lf.deref(), uf.deref()) {
                (
                    CanonicalRendersForm::StructuralRenders {
                        renders_variant: RendersVariant::RendersNormal,
                        renders_structural_type: a,
                    },
                    _,
                )
                | (
                    CanonicalRendersForm::StructuralRenders {
                        renders_structural_type: a,
                        ..
                    },
                    CanonicalRendersForm::StructuralRenders { .. },
                ) => self.walk(cx, a, u),
                (
                    CanonicalRendersForm::NominalRenders { renders_super, .. },
                    CanonicalRendersForm::StructuralRenders {
                        renders_structural_type: t,
                        ..
                    },
                ) => {
                    if self.marker_of(t).is_some() || self.fits_open(cx, l, t)? {
                        self.walk(cx, l, t)
                    } else {
                        self.walk(cx, renders_super, u)
                    }
                }
                (
                    CanonicalRendersForm::IntrinsicRenders(_)
                    | CanonicalRendersForm::DefaultRenders,
                    CanonicalRendersForm::StructuralRenders {
                        renders_structural_type: t,
                        ..
                    },
                ) => self.walk(cx, l, t),
                _ => Ok(()),
            },
            (None, Some(uf)) => match uf.deref() {
                CanonicalRendersForm::StructuralRenders {
                    renders_variant,
                    renders_structural_type: t,
                } => {
                    if matches!(renders_variant, RendersVariant::RendersStar)
                        && let Some(elem) = self.elem_of_array(cx, l)
                    {
                        return self.walk(cx, &elem, u);
                    }
                    if matches!(l.deref(), TypeInner::NominalT { .. })
                        && self.marker_of(t).is_none()
                        && !self.fits_open(cx, l, t)?
                    {
                        return Ok(());
                    }
                    self.walk(cx, l, t)
                }
                CanonicalRendersForm::DefaultRenders => match self.elem_of_array(cx, l) {
                    Some(elem) => self.walk(cx, &elem, u),
                    None => Ok(()),
                },
                _ => Ok(()),
            },
            // Out of the renders world, as subtyping leaves it: what `renders A` renders, `A`; the
            // render type of a component or an intrinsic is a `React.MixedElement`.
            (Some(lf), None) => match lf.deref() {
                CanonicalRendersForm::StructuralRenders {
                    renders_structural_type: a,
                    ..
                } => self.walk(cx, a, u),
                CanonicalRendersForm::NominalRenders { .. }
                | CanonicalRendersForm::IntrinsicRenders(_) => {
                    let reason = type_util::reason_of_t(l).dupe();
                    let mixed_element = FlowJs::get_builtin_react_type_with_env(
                        cx,
                        self.env,
                        None,
                        &reason,
                        None,
                        ExpectedModulePurpose::ReactModuleForReactMixedElementType,
                    )?;
                    self.walk(cx, &mixed_element, u)
                }
                CanonicalRendersForm::DefaultRenders => Ok(()),
            },
            (None, None) => Ok(()),
        }
    }

    /// The element type of an array or tuple type.
    fn elem_of_array<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> Option<Type> {
        let t = self.resolve(cx, t);
        match t.deref() {
            TypeInner::DefT(_, d) => match d.deref() {
                DefTInner::ArrT(a) => match a.deref() {
                    ArrType::ArrayAT(box ArrayATData { elem_t, .. })
                    | ArrType::TupleAT(box TupleATData { elem_t, .. })
                    | ArrType::ROArrayAT(box ROArrayATData { elem_t, .. }) => Some(elem_t.dupe()),
                },
                _ => None,
            },
            _ => None,
        }
    }

    /// A function as a component: the config goes to its props, its first parameter, and what it
    /// returns to what the component renders.
    fn walk_function_component<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        lf: &FunType,
        uc: &ReactAbstractComponentTData,
    ) -> Result<(), FlowJsException> {
        // Without parameters, the props are an exact empty object.
        let props = match lf.params.first() {
            Some(FunParam(_, props)) => props.dupe(),
            None => {
                let reason = type_util::reason_of_t(&lf.return_t).dupe();
                obj_type::mk(ObjKind::Exact, cx, reason, ObjLiteral::NonLiteral)
            }
        };
        self.walk(cx, &uc.config, &props)?;
        self.walk(cx, &lf.return_t, &uc.renders)
    }

    /// A tuple with spreads (`items`, the side `lower` of the pair `l <: u`) against an array type
    /// on the other side: each element against the element at its position, and each spread
    /// against the elements it covers. A spread before the end covers as many elements as its
    /// tuple has, or the tuple its marker has as a solution so far; without one, the pair is
    /// walked again after the other arguments.
    fn walk_spread_tuple<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        items: Vec<TupleItem>,
        l: &Type,
        u: &Type,
        lower: bool,
    ) -> Result<(), FlowJsException> {
        let other = self.resolve(cx, if lower { u } else { l });
        if !lower {
            self.spread_tuple_pairs.push((l.dupe(), u.dupe()));
        }
        let mut offset = 0;
        let n = items.len();
        for (k, item) in items.into_iter().enumerate() {
            let (t, part) = match item {
                TupleItem::Elem(t) => {
                    let part = arr_elem_at(&other, offset);
                    offset += 1;
                    (t, part)
                }
                TupleItem::Spread(t) if k + 1 == n => {
                    let part = arr_slice(&other, offset, None);
                    (t, part)
                }
                TupleItem::Spread(t) => match self.known_length(cx, &t) {
                    Some(len) => {
                        let part = arr_slice(&other, offset, Some(len));
                        offset += len;
                        (t, part)
                    }
                    None if !self.second_pass => {
                        self.deferred.push((l.dupe(), u.dupe()));
                        return Ok(());
                    }
                    None => {
                        return self.give_up(cx, "spread of unknown length", l, u);
                    }
                },
            };
            let Some(part) = part else {
                return self.give_up(cx, "destructor", l, u);
            };
            if lower {
                self.walk(cx, &t, &part)?;
            } else {
                self.walk(cx, &part, &t)?;
            }
        }
        Ok(())
    }

    /// The number of elements of `t`, a tuple or a marker whose solution so far is one.
    fn known_length<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> Option<usize> {
        let t = match self.marker_of(t) {
            Some(name) => {
                let bounds = &self.vars[&name];
                match (bounds.exact.as_slice(), bounds.lower.as_slice()) {
                    ([t], _) | ([], [(t, _)]) => t.dupe(),
                    _ => return None,
                }
            }
            None => t.dupe(),
        };
        let t = self.resolve(cx, &t);
        tuple_elements(&t).map(|elements| elements.len())
    }

    /// The marker `t` is, possibly under destructors that the flow-based solve's pinning reads
    /// through (`$Exact<T>`, `Readonly<T>`, `Partial<T>`, `Required<T>`).
    fn marker_through_identity<'cx>(&self, cx: &mut Context<'cx>, t: &Type) -> Option<SubstName> {
        let t = self.resolve(cx, t);
        if let Some(name) = self.marker_of(&t) {
            return Some(name);
        }
        match t.deref() {
            TypeInner::EvalT {
                type_, defer_use_t, ..
            } if matches!(
                defer_use_t.2.deref(),
                Destructor::ExactType
                    | Destructor::ReadOnlyType
                    | Destructor::PartialType
                    | Destructor::RequiredType
                    | Destructor::ReactDRO(_)
            ) =>
            {
                self.marker_through_identity(cx, type_)
            }
            _ => None,
        }
    }

    /// The marker whose bound the flow-based solve's pinning reverses from the destructor `d` of
    /// `type_`, and the destructor to reverse. For a spread, the marker may be any operand when the
    /// others have no markers: the reversal takes the props of all the others away, whatever their
    /// order. Without a marker among them, the target may be an operand that is a spread with
    /// markers inside, which the reversal bounds in turn.
    fn reversal_target<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        type_: &Type,
        d: &Destructor,
    ) -> Option<(ReversalTarget, Destructor)> {
        let Destructor::SpreadType(box DestructorSpreadTypeData(target, todo_rev, head_slice)) = d
        else {
            return self
                .marker_through_identity(cx, type_)
                .map(|name| (ReversalTarget::Marker(name), d.clone()));
        };
        // The other operands may have markers inside, but not be one, unless it has evidence: then
        // it stands for its solution so far. The reversal takes their props away by name.
        let reason = type_util::reason_of_t(type_).dupe();
        let mut found = None;
        let mut nested = Vec::new();
        let mut others = Vec::new();
        let operands = std::iter::once(object::spread::Operand::Type(type_.dupe()))
            .chain(todo_rev.iter().cloned())
            .collect::<Vec<_>>();
        for operand in operands {
            match &operand {
                object::spread::Operand::Type(t) => match self.marker_through_identity(cx, t) {
                    Some(name) if !self.has_evidence(&name) => {
                        if found.is_some() {
                            return None;
                        }
                        found = Some(name);
                    }
                    Some(_) => {
                        let t = self.with_evidence(cx, t, &reason);
                        others.push(object::spread::Operand::Type(t));
                    }
                    None if self.is_spread_with_markers(cx, t) => nested.push(operand.clone()),
                    None => others.push(operand.clone()),
                },
                object::spread::Operand::Slice(_) => others.push(operand.clone()),
            }
        }
        let name = match (found, nested.as_slice()) {
            (Some(name), _) => {
                others.extend(nested);
                ReversalTarget::Marker(name)
            }
            (None, [object::spread::Operand::Type(t)]) => ReversalTarget::Spread(t.dupe()),
            (None, _) => return None,
        };
        let mut list = flow_data_structure_wrapper::list::FlowOcamlList::new();
        for operand in others.into_iter().rev() {
            list.push_front(operand);
        }
        Some((
            name,
            Destructor::SpreadType(Box::new(DestructorSpreadTypeData(
                target.clone(),
                list,
                head_slice.clone(),
            ))),
        ))
    }

    /// The reversal of a destructor bounds its target: a marker, with the priority of a reversal,
    /// or a spread with markers inside, which is walked against it.
    fn add_reversal<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        target: &ReversalTarget,
        reversed: &Type,
    ) -> Result<(), FlowJsException> {
        match target {
            ReversalTarget::Marker(name) => {
                self.add_upper_at(cx, name, reversed, Priority::Reversal);
                Ok(())
            }
            ReversalTarget::Spread(t) => self.walk(cx, t, reversed),
        }
    }

    /// Whether `t` is a spread with markers inside.
    fn is_spread_with_markers<'cx>(&mut self, cx: &mut Context<'cx>, t: &Type) -> bool {
        let t = self.resolve(cx, t);
        matches!(t.deref(), TypeInner::EvalT { defer_use_t, .. }
            if matches!(defer_use_t.2.deref(), Destructor::SpreadType(_)))
            && self.mentions(cx, &t)
    }

    fn has_evidence(&self, name: &SubstName) -> bool {
        let bounds = &self.vars[name];
        !(bounds.lower.is_empty() && bounds.exact.is_empty())
    }

    /// Whether a spread has several operands that are markers without evidence yet: its reversal
    /// waits for the other arguments, which may give all but one of them a solution.
    fn spread_waits<'cx>(&self, cx: &mut Context<'cx>, type_: &Type, d: &Destructor) -> bool {
        let Destructor::SpreadType(box DestructorSpreadTypeData(_, todo_rev, _)) = d else {
            return false;
        };
        let operands: Vec<Type> = std::iter::once(type_.dupe())
            .chain(todo_rev.iter().filter_map(|o| match o {
                object::spread::Operand::Type(t) => Some(t.dupe()),
                object::spread::Operand::Slice(_) => None,
            }))
            .collect();
        !self.second_pass
            && operands
                .iter()
                .filter(|t| {
                    self.marker_through_identity(cx, t)
                        .is_some_and(|name| !self.has_evidence(&name))
                })
                .count()
                > 1
    }

    /// Whether a rest removes props that depend on a marker without evidence yet
    /// (`Omit<T, keyof U>`): its reversal waits for the other arguments, which may give that marker
    /// evidence, as the flow-based solve reverses it once the tvar of `U` has its lower bounds.
    fn rest_waits<'cx>(&self, cx: &mut Context<'cx>, d: &Destructor) -> bool {
        let Destructor::RestType(_, t_rest) = d else {
            return false;
        };
        !self.second_pass
            && (self.markers_in(cx, t_rest).iter()).any(|name| !self.has_evidence(name))
    }

    /// `t` evaluated if it is a destructor without markers.
    fn evaluated<'cx>(&mut self, cx: &mut Context<'cx>, t: &Type) -> Result<Type, FlowJsException> {
        let t = self.resolve(cx, t);
        if let TypeInner::EvalT {
            type_,
            defer_use_t,
            id,
        } = t.deref()
            && !self.mentions(cx, &t)
        {
            let TypeDestructorTInner(use_op, reason, d) = defer_use_t.deref();
            if let Some(result) = self.eval_sandboxed(cx, use_op, reason, type_, d, id)? {
                return Ok(self.resolve(cx, &result));
            }
        }
        Ok(t)
    }

    /// The flow-based solve's reversal of the destructor `d` of a marker: the upper bound that
    /// its pinning reads for the marker from `t`, a bound of the destructor's result.
    fn reverse_destructor<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        reason: &Reason,
        d: &Destructor,
        t: &Type,
    ) -> Result<Option<Type>, FlowJsException> {
        let env = self.env;
        let reversed = match d {
            Destructor::RestType(_, t_rest) => {
                let t_rest = self.with_evidence(cx, t_rest, reason);
                sandbox(cx, env, self.preserve, |cx| {
                    reverse_rest(cx, env, reason, &t_rest, t)
                })?
                .flatten()
            }
            Destructor::ReactCheckComponentConfig { props } => {
                sandbox(cx, env, self.preserve, |cx| {
                    reverse_config(cx, env, reason, props, t)
                })?
                .flatten()
            }
            Destructor::SpreadType(box DestructorSpreadTypeData(_, todo_rev, head_slice)) => {
                let acc = match head_slice {
                    Some(slice) => flow_data_structure_wrapper::list::FlowOcamlList::unit(
                        object::spread::AccElement::InlineSlice(slice.clone()),
                    ),
                    None => flow_data_structure_wrapper::list::FlowOcamlList::new(),
                };
                sandbox(cx, env, self.preserve, |cx| {
                    reverse_spread(cx, env, reason, todo_rev, &acc, t, ObjLiteral::NonLiteral)
                })?
                .flatten()
            }
            // `$Enum<T>`: the values of the enum.
            Destructor::EnumType => match self.resolve(cx, t).deref() {
                TypeInner::DefT(_, d) => match d.deref() {
                    DefTInner::EnumObjectT { enum_value_t, .. } => Some(enum_value_t.dupe()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        };
        Ok(reversed.map(|r| self.resolve(cx, &r)))
    }

    fn walk_from_eval<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        l: &Type,
        u: &Type,
    ) -> Result<(), FlowJsException> {
        let TypeInner::EvalT {
            type_,
            defer_use_t,
            id,
        } = l.deref()
        else {
            return Ok(());
        };
        let TypeDestructorTInner(use_op, reason, d) = defer_use_t.deref();
        if !self.mentions(cx, l) {
            let Some(result) = self.eval_sandboxed(cx, use_op, reason, type_, d, id)? else {
                return Ok(());
            };
            return self.walk(cx, &result, u);
        }
        // `NonNullable<T>` against the hint `u`: the flow-based solve's pinning reads `?u` as an upper
        // bound of `T`. It is soft, so that the arguments decide without it. A hint of `mixed` or
        // `any` says nothing (it stands in for evidence the walk may not see, such as a nested
        // generic call's result).
        if self.in_hint
            && matches!(d.deref(), Destructor::NonMaybeType)
            && let Some(name) = self.marker_of(type_)
        {
            let says_nothing = match self.resolve(cx, u).deref() {
                TypeInner::AnyT(..) => true,
                TypeInner::DefT(_, d) => matches!(d.deref(), DefTInner::MixedT(_)),
                _ => false,
            };
            if !says_nothing {
                let maybe = Type::new(TypeInner::MaybeT(reason.dupe(), u.dupe()));
                self.add_upper_at(cx, &name, &maybe, Priority::Soft);
            }
            return Ok(());
        }
        match d.deref() {
            Destructor::ReadOnlyType
            | Destructor::PartialType
            | Destructor::RequiredType
            | Destructor::ExactType
            | Destructor::NonMaybeType
            | Destructor::ReactDRO(_) => self.walk(cx, type_, u),
            // A conditional type below (as in a return type checked against its hint) gives
            // evidence only through the branch it takes for every solution. Its branches, or the
            // branch for the solution so far, would give the hint the weight of an argument.
            Destructor::ConditionalType(_) => match self.eval_conditional(cx, l, false)? {
                Some(branch) => self.walk(cx, &branch, u),
                None if !self.second_pass => {
                    self.deferred.push((l.dupe(), u.dupe()));
                    Ok(())
                }
                None => Ok(()),
            },
            // The config of a component of component syntax with only a rest parameter of an
            // alias type is that type. (The config of other props is read-only: for those, the
            // flow-based solve's reversal.)
            Destructor::ReactCheckComponentConfig { props }
                if props.is_empty()
                    && self.is_object_alias(cx, type_)
                    && self.walk_config_as_rest(cx, type_, u, false)? =>
            {
                Ok(())
            }
            // `{...T, ...rest} <: u`, `Rest<T, R> <: u`, and the config of a component of
            // component syntax: the flow-based solve's reversals.
            Destructor::SpreadType(_)
            | Destructor::RestType(..)
            | Destructor::ReactCheckComponentConfig { .. }
            | Destructor::EnumType
                if !self.mentions(cx, u) =>
            {
                if self.rest_waits(cx, d) {
                    self.deferred.push((l.dupe(), u.dupe()));
                    return Ok(());
                }
                if let Some((target, d)) = self.reversal_target(cx, type_, d) {
                    // A pending spread whose marker still has no evidence at the end gives nothing
                    // more.
                    if self.spreads_pass {
                        return Ok(());
                    }
                    let u = self.evaluated(cx, u)?;
                    let Some(reversed) = self.reverse_destructor(cx, reason, &d, &u)? else {
                        return self.give_up(cx, "destructor", l, &u);
                    };
                    self.add_reversal(cx, &target, &reversed)?;
                    // Once the marker has a lower bound from the other arguments, the spread is
                    // also walked against `u`, so that the props of the other operands, such as
                    // `onClose: C` in `{...T, ...{onClose: C}}`, meet those of `u`. That waits for
                    // the end of the walk.
                    if matches!(target, ReversalTarget::Marker(_))
                        && let Destructor::SpreadType(box DestructorSpreadTypeData(_, others, _)) =
                            &d
                        && others.iter().any(|operand| match operand {
                            object::spread::Operand::Type(t) => self.mentions(cx, t),
                            object::spread::Operand::Slice(_) => false,
                        })
                    {
                        self.pending_spreads.push((l.dupe(), u.dupe()));
                    }
                    return Ok(());
                }
                if self.spread_waits(cx, type_, d) {
                    self.deferred.push((l.dupe(), u.dupe()));
                    return Ok(());
                }
                // The config of a component with only a rest parameter is that parameter's type.
                if let Destructor::ReactCheckComponentConfig { props } = d.deref()
                    && props.is_empty()
                {
                    return self.walk(cx, type_, u);
                }
                // The config of a component whose rest parameter has markers inside: the rest
                // parameter against `u` without the declared props, the flow-based solve's
                // reversal. (The declared props go on with the config below.)
                if matches!(d.deref(), Destructor::ReactCheckComponentConfig { .. })
                    && self.mentions(cx, type_)
                {
                    let u = self.evaluated(cx, u)?;
                    if let Some(rest) = self.reverse_destructor(cx, reason, d, &u)? {
                        self.walk(cx, type_, &rest)?;
                    }
                }
                // A pending spread whose marker has evidence at the end: the spread with the
                // solutions so far, against `u`.
                if self.spreads_pass && matches!(d.deref(), Destructor::SpreadType(_)) {
                    let spread = self.with_evidence(cx, l, reason);
                    if let TypeInner::EvalT {
                        type_, defer_use_t, ..
                    } = spread.deref()
                        && let Some(result) = self.evaluate(cx, type_, defer_use_t)?
                        && self.is_structure(cx, &result)
                    {
                        return self.walk(cx, &result, u);
                    }
                }
                match self.evaluate(cx, type_, defer_use_t)? {
                    Some(result) if self.is_structure(cx, &result) => self.walk(cx, &result, u),
                    _ => self.give_up(cx, "destructor", l, u),
                }
            }
            // `[...prefix, ...T] <: u`: the prefix goes to the elements of `u`, and `T` to the rest.
            Destructor::SpreadTupleType(st) if let Some(items) = spread_tuple_items(st, type_) => {
                self.walk_spread_tuple(cx, items, l, u, true)
            }
            // An indexed access of an object type with markers inside, such as an element of an
            // alias of an array type, is indexed first: `Array<T>[number]` is `T`. So are the values
            // of one: `Values<{a: T}>` is `T`.
            Destructor::PropertyType { .. }
            | Destructor::ElementType { .. }
            | Destructor::OptionalIndexedAccessNonMaybeType { .. }
            | Destructor::OptionalIndexedAccessResultType { .. }
            | Destructor::ValuesType
                if let Some(object) = self.structure_of(cx, type_)?
                    && let Some(result) = match self.evaluate(cx, &object, defer_use_t)? {
                        Some(result) => Some(result),
                        None => self.declared_property(cx, &object, d),
                    } =>
            {
                self.walk(cx, &result, u)
            }
            // Against a type without markers, only a reversal could say something about the
            // markers in an indexed access or a `Values`. The flow-based solve has none.
            Destructor::PropertyType { .. }
            | Destructor::ElementType { .. }
            | Destructor::OptionalIndexedAccessNonMaybeType { .. }
            | Destructor::OptionalIndexedAccessResultType { .. }
            | Destructor::ValuesType
                if !self.mentions(cx, u) =>
            {
                Ok(())
            }
            // The config of a component with only a rest parameter is that parameter's type, also
            // against a type with markers (the config of another component).
            Destructor::ReactCheckComponentConfig { props } if props.is_empty() => {
                self.walk(cx, type_, u)
            }
            // Other destructors of markers, such as a mapped type: the destructor evaluated with
            // the markers rigid, if that decides it. Otherwise it is stuck on a marker, and says
            // nothing about it.
            _ => match self.evaluate(cx, type_, defer_use_t)? {
                Some(result) => self.walk(cx, &result, u),
                None => self.give_up(cx, "destructor", l, u),
            },
        }
    }

    /// The branch a conditional type takes, if it is known. First with the markers rigid, as for
    /// a type parameter in a signature: a branch taken for every solution. Then with the solution
    /// so far (or, on the second pass, the default) in place of the markers of the check type.
    /// `None` if neither decides, or a marker of the check type has no solution yet.
    fn eval_conditional<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        t: &Type,
        with_solution: bool,
    ) -> Result<Option<Type>, FlowJsException> {
        let TypeInner::EvalT {
            type_: check,
            defer_use_t,
            ..
        } = t.deref()
        else {
            return Ok(None);
        };
        // A bare marker is rigid with the bound `mixed`, so the branch is known for every solution
        // only if the extends type is `mixed` or `any`.
        let rigid_undecided = self.marker_of(&self.resolve(cx, check)).is_some()
            && matches!(defer_use_t.2.deref(), Destructor::ConditionalType(c)
                if !matches!(self.resolve(cx, &c.extends_t).deref(),
                    TypeInner::AnyT(..) | TypeInner::DefT(..)
                        if is_mixed_or_any(&self.resolve(cx, &c.extends_t))));
        if !rigid_undecided && let Some(branch) = self.evaluate(cx, check, defer_use_t)? {
            return Ok(Some(branch));
        }
        if let Destructor::ConditionalType(c) = defer_use_t.2.deref()
            && let Some(branch) = self.infer_conditional(cx, check, c)?
        {
            return Ok(Some(branch));
        }
        if !with_solution {
            return Ok(None);
        }
        let names: Vec<SubstName> = type_subst::free_var_finder(cx, None, check)
            .iter()
            .filter(|n| self.vars.contains_key(*n))
            .duped()
            .collect();
        if names.is_empty() {
            return Ok(None);
        }
        let reason = type_util::reason_of_t(check).dupe();
        let mut fixed = FlowOrdMap::new();
        for name in names {
            // A default stands in for a marker without evidence only on the second pass, once the
            // later arguments have given theirs.
            let solution = match self.solution_so_far(cx, &name, &reason) {
                Some(solution) => solution,
                None if !self.second_pass => return Ok(None),
                None => self.current_solution(cx, &name, &reason),
            };
            if is_placeholder(&solution) {
                return Ok(None);
            }
            fixed.insert(name, solution);
        }
        let check = subst(cx, &fixed, check);
        self.evaluate(cx, &check, defer_use_t)
    }

    /// The `infer` types of a conditional type whose check type has markers inside, as
    /// `ComponentProps<Dialog<T>>`: the check type walked against the extends
    /// type, with fresh markers for the `infer` types, gives each of them a type (its exact bound,
    /// else the union of its lower bounds, else its first upper bound). If each has one, and the
    /// check type fits the extends type with the markers taking any type, the result is the true
    /// branch with those types. What the walk records about the other markers on the way is
    /// dropped.
    fn infer_conditional<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        check: &Type,
        c: &DestructorConditionalTypeData,
    ) -> Result<Option<Type>, FlowJsException> {
        if c.infer_tparams.is_empty()
            || self.marker_through_identity(cx, check).is_some()
            || !self.mentions(cx, check)
        {
            return Ok(None);
        }
        let snapshot = self.snapshot();
        let mut map = FlowOrdMap::new();
        let mut names = Vec::new();
        for tparam in c.infer_tparams.iter() {
            let (name, marker) = mk_marker(cx, tparam);
            self.vars.insert(name.dupe(), Bounds::default());
            map.insert(tparam.name.dupe(), marker);
            names.push((tparam.name.dupe(), name));
        }
        let extends = subst(cx, &map, &c.extends_t);
        let walked = self.walk(cx, check, &extends);
        let reason = type_util::reason_of_t(check).dupe();
        let solutions: Option<FlowOrdMap<SubstName, Type>> = names
            .iter()
            .map(|(tparam, name)| {
                let bounds = &self.vars[name];
                let t = (bounds.exact.first().map(Dupe::dupe))
                    .or_else(|| {
                        (!bounds.lower.is_empty()).then(|| {
                            let lower = types(&bounds.lower).duped().collect();
                            type_util::union_of_ts(reason.dupe(), lower, None)
                        })
                    })
                    .or_else(|| types(&bounds.upper).next().map(Dupe::dupe));
                t.map(|t| (tparam.dupe(), t))
            })
            .collect();
        let fits = match (&walked, &solutions) {
            (Ok(()), Some(_)) => self.fits_open(cx, check, &extends),
            _ => Ok(false),
        };
        self.restore(snapshot);
        walked?;
        let (Some(solutions), true) = (solutions, fits?) else {
            return Ok(None);
        };
        let (t, _) = branches(cx, check, c);
        Ok(Some(subst(cx, &solutions, &t)))
    }

    /// Evaluates the destructor `d` on `check`, without errors or flows that outlive it. `None`
    /// if the result is not known: a tvar or a destructor, a placeholder, or a conditional type
    /// kept abstract.
    fn evaluate<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        check: &Type,
        d: &TypeDestructorT,
    ) -> Result<Option<Type>, FlowJsException> {
        let evalt = flow_cache::eval::id(cx, check.dupe(), d.dupe());
        let TypeInner::EvalT {
            type_,
            defer_use_t,
            id,
        } = evalt.deref()
        else {
            return Ok(None);
        };
        let TypeDestructorTInner(use_op, reason, d) = defer_use_t.deref();
        let errors = cx.errors();
        let result = run_keeping_clean_flows(cx, self.env, self.preserve, |cx| {
            FlowJs::mk_type_destructor_with_env(
                cx,
                self.env,
                DepthTrace::dummy_trace(),
                use_op.dupe(),
                reason,
                type_,
                d,
                id.dupe(),
            )
        });
        cx.reset_errors(errors);
        let result = match instantiation_solver::sandboxed(result)? {
            Some(result) => self.resolve(cx, &result),
            None => return Ok(None),
        };
        let stuck = match result.deref() {
            TypeInner::OpenT(_) | TypeInner::EvalT { .. } => true,
            TypeInner::NominalT { nominal_type, .. } => {
                matches!(nominal_type.nominal_id, nominal::Id::StuckEval(_))
            }
            // A generic standing for a type built from markers, like the config of
            // `component(...P)`, which the walk can't take apart.
            TypeInner::GenericT(box GenericTData { name, .. }) => match name.deref() {
                flow_common::subst_name::SubstNameInner::Synthetic { ts, .. } => {
                    ts.iter().any(|t| self.vars.contains_key(t))
                }
                _ => false,
            },
            _ => is_placeholder(&result),
        };
        Ok((!stuck).then_some(result))
    }

    /// `any` flowing to (or from) a type that mentions markers gives each of them `any` as a bound
    /// on the side its position says. As in the flow-based solve, `any` flowing into a union goes
    /// to its first member only, which the speculative match takes, as `any` fits it; and it goes
    /// through the body of an alias, but not again into an application of the same alias to the
    /// same type arguments in it, where that flow meets itself and stops.
    fn propagate_any<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        any: &Type,
        other: &Type,
        any_is_lower: bool,
    ) {
        struct Collector<'a, 'e> {
            walker: &'a mut Walker<'e>,
            any_is_lower: bool,
            /// The aliases whose bodies are being visited, and how many more bodies may be.
            expanding: Vec<usize>,
            budget: usize,
            found: BTreeSet<(SubstName, Polarity)>,
        }
        impl TypeVisitor<()> for Collector<'_, '_> {
            fn type_<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, _acc: (), t: &Type) {
                // A part without markers has none to give `any` to.
                if !self.walker.mentions(cx, t) {
                    return;
                }
                match t.deref() {
                    TypeInner::GenericT(box GenericTData { name, .. })
                        if self.walker.vars.contains_key(name) =>
                    {
                        self.found.insert((name.dupe(), pole));
                    }
                    TypeInner::UnionT(_, rep) => {
                        // The polarity at which `any` flows into the union, and the one at which
                        // a member flows to `any`.
                        let (into, out) = if self.any_is_lower {
                            (pole, Polarity::Negative)
                        } else {
                            (Polarity::inv(pole), Polarity::Positive)
                        };
                        for (i, m) in rep.members_iter().enumerate() {
                            match (into, i) {
                                (Polarity::Negative, _) | (_, 0) => self.type_(cx, pole, (), m),
                                (Polarity::Neutral, _) => self.type_(cx, out, (), m),
                                (Polarity::Positive, _) => {}
                            }
                        }
                    }
                    TypeInner::TypeAppT(a) if self.walker.is_alias(cx, &a.type_) => {
                        // A recursive alias is visited once on each path, and only so many alias
                        // bodies are in all, as their applications can grow without end.
                        let key = addr(&self.walker.resolve(cx, &a.type_));
                        if self.expanding.contains(&key) || self.budget == 0 {
                            return;
                        }
                        match self.walker.expand_typeapp(cx, t) {
                            Some(body) => {
                                self.budget -= 1;
                                self.expanding.push(key);
                                self.type_(cx, pole, (), &body);
                                self.expanding.pop();
                            }
                            None => type_visitor::type_default(self, cx, pole, (), t),
                        }
                    }
                    // The type arguments of a class or interface, at the variance of its type
                    // parameters, as subtyping relates `any` to them.
                    TypeInner::TypeAppT(a)
                        if let Some(polarities) =
                            self.walker.same_typeapp(cx, &a.type_, &a.type_) =>
                    {
                        for (targ, p) in a.targs.iter().zip(polarities) {
                            self.type_(cx, Polarity::mult(pole, p), (), targ);
                        }
                    }
                    _ => type_visitor::type_default(self, cx, pole, (), t),
                }
            }
        }
        let mut collector = Collector {
            walker: self,
            any_is_lower,
            expanding: Vec::new(),
            budget: 64,
            found: BTreeSet::new(),
        };
        collector.type_(cx, Polarity::Positive, (), other);
        let found = collector.found;
        for (name, pole) in found {
            let pole = if any_is_lower {
                pole
            } else {
                Polarity::inv(pole)
            };
            match pole {
                Polarity::Positive => self.add_lower(cx, &name, any),
                Polarity::Negative => self.add_upper(cx, &name, any),
                Polarity::Neutral => {
                    self.add_lower(cx, &name, any);
                    self.add_upper(cx, &name, any);
                }
            }
        }
    }

    /// `t` with the markers that have evidence replaced by their solution so far.
    fn with_evidence<'cx>(&mut self, cx: &mut Context<'cx>, t: &Type, reason: &Reason) -> Type {
        let map: FlowOrdMap<SubstName, Type> = self
            .markers_in(cx, t)
            .into_iter()
            .filter(|name| {
                let bounds = &self.vars[name];
                !(bounds.lower.is_empty() && bounds.exact.is_empty())
            })
            .map(|name| {
                let solution = self.current_solution(cx, &name, reason);
                (name, solution)
            })
            .collect();
        if map.is_empty() {
            return t.dupe();
        }
        subst(cx, &map, t)
    }

    /// What a marker stands for so far, for instantiating a generic value against it: the union
    /// of its lower bounds, otherwise its first upper bound (of the highest priority, but `Soft`),
    /// otherwise its default or a placeholder.
    fn current_solution<'cx>(
        &self,
        cx: &mut Context<'cx>,
        name: &SubstName,
        reason: &Reason,
    ) -> Type {
        match self.solution_so_far(cx, name, reason) {
            Some(t) => t,
            None => match self.defaults.get(name) {
                Some(default) => default.dupe(),
                None => placeholder_t::why(reason.dupe(), mixed_t::why(reason.dupe())),
            },
        }
    }

    /// What the evidence of a marker says it stands for so far, if anything.
    fn solution_so_far<'cx>(
        &self,
        cx: &mut Context<'cx>,
        name: &SubstName,
        reason: &Reason,
    ) -> Option<Type> {
        let bounds = &self.vars[name];
        let lower = best(&bounds.lower);
        let upper = best(&bounds.upper);
        if let Some(t) = bounds.exact.first() {
            Some(t.dupe())
        } else if !lower.is_empty() {
            let t = type_util::union_of_ts(reason.dupe(), lower, None);
            Some(generalize_singletons(cx, &self.call_loc, false, t))
        } else {
            upper.into_iter().next()
        }
    }

    /// A generic value `l` flowing to `u`: instantiate it against `u` with the solution so far in
    /// place of the markers, then walk its instance against `u`.
    fn walk_generic_value<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        p: &PolyTData,
        u: &Type,
    ) -> Result<(), FlowJsException> {
        let reason = type_util::reason_of_t(u).dupe();
        let names: Vec<SubstName> = self.vars.keys().duped().collect();
        let mut fixed = FlowOrdMap::new();
        for name in names {
            let t = self.current_solution(cx, &name, &reason);
            fixed.insert(name, t);
        }
        let u_fixed = subst(cx, &fixed, u);

        let mut markers = FlowOrdMap::new();
        let mut inner_names = Vec::new();
        for tparam in p.tparams.iter() {
            let (name, marker) = mk_marker(cx, tparam);
            markers.insert(tparam.name.dupe(), marker);
            inner_names.push(name);
        }
        let body = subst_markers(cx, &markers, &p.t_out);
        let mut inner = Walker::new(
            self.env,
            self.preserve,
            self.call_loc.dupe(),
            inner_names.iter().duped(),
        );
        // The inner walk is part of this one: it shares its depth and nesting limits.
        inner.depth = self.depth;
        inner.nesting = self.nesting.clone();
        inner.steps = self.steps;
        inner.walk(cx, &body, &u_fixed)?;
        inner.walk_deferred(cx)?;
        self.steps = inner.steps;

        let mut theta = FlowOrdMap::new();
        let mut fallback = false;
        for (tparam, name) in p.tparams.iter().zip(inner_names.iter()) {
            let bounds = &inner.vars[name];
            let t = if let Some(t) = bounds.exact.first() {
                t.dupe()
            } else if !bounds.lower.is_empty() {
                let lower = types(&bounds.lower).duped().collect();
                let t = type_util::union_of_ts(tparam.reason.dupe(), lower, None);
                generalize_singletons(cx, &self.call_loc, false, t)
            } else if let Some(u) = types(&bounds.upper).next() {
                u.dupe()
            } else {
                fallback = true;
                // A default or bound may mention the type parameters before it.
                subst(cx, &theta, tparam.default.as_ref().unwrap_or(&tparam.bound))
            };
            theta.insert(tparam.name.dupe(), t);
        }
        let instance = subst(cx, &theta, &p.t_out);
        // A type parameter without evidence stands for the fresh type variable the flow-based
        // solve instantiates it with, which the markers are unified with, so it decides none.
        let soft = self.soft_exact;
        self.soft_exact = soft || fallback;
        let result = self.walk(cx, &instance, u);
        self.soft_exact = soft;
        result
    }
}

/// The branches of a conditional type over `check`. In a distributive one, the branches name
/// each member of the check type by the type parameter it distributes over; for inference, that
/// is the check type itself.
fn branches<'cx>(
    cx: &mut Context<'cx>,
    check: &Type,
    c: &DestructorConditionalTypeData,
) -> (Type, Type) {
    match &c.distributive_tparam_name {
        Some(name) => {
            let map: FlowOrdMap<SubstName, Type> =
                [(name.dupe(), check.dupe())].into_iter().collect();
            (subst(cx, &map, &c.true_t), subst(cx, &map, &c.false_t))
        }
        None => (c.true_t.dupe(), c.false_t.dupe()),
    }
}

/// The markers that `t` uses as the key of an indexer or as the index of an indexed access,
/// including through the type arguments of an alias that uses its parameter so.
fn key_markers<'cx>(cx: &mut Context<'cx>, t: &Type, out: &mut BTreeSet<SubstName>) {
    key_markers_seen(cx, t, out, &mut BTreeSet::new());
}

fn key_markers_seen<'cx>(
    cx: &mut Context<'cx>,
    t: &Type,
    out: &mut BTreeSet<SubstName>,
    seen: &mut BTreeSet<poly::Id>,
) {
    struct Visitor<'o> {
        out: &'o mut BTreeSet<SubstName>,
        seen: &'o mut BTreeSet<poly::Id>,
    }
    fn marker(t: &Type) -> Option<SubstName> {
        match t.deref() {
            TypeInner::GenericT(box GenericTData { name, .. }) => Some(name.dupe()),
            _ => None,
        }
    }
    impl TypeVisitor<()> for Visitor<'_> {
        fn type_<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, _acc: (), t: &Type) {
            match t.deref() {
                // The type parameters of a generic value hide those of the same name outside it.
                TypeInner::DefT(_, d) if let DefTInner::PolyT(p) = d.deref() => {
                    let mut inner = BTreeSet::new();
                    Visitor {
                        out: &mut inner,
                        seen: self.seen,
                    }
                    .type_(cx, pole, (), &p.t_out);
                    for tparam in p.tparams.iter() {
                        inner.remove(&tparam.name);
                    }
                    self.out.extend(inner);
                    return;
                }
                TypeInner::DefT(_, d) if let DefTInner::ObjT(o) = d.deref() => {
                    if let Some(dict) = obj_type::get_dict_opt(&o.flags.obj_kind)
                        && let Some(name) = marker(&dict.key)
                    {
                        self.out.insert(name);
                    }
                }
                TypeInner::EvalT { defer_use_t, .. } => match defer_use_t.2.deref() {
                    Destructor::ElementType { index_type } => self.out.extend(marker(index_type)),
                    Destructor::OptionalIndexedAccessNonMaybeType {
                        index: OptionalIndexedAccessIndex::OptionalIndexedAccessTypeIndex(index),
                    } => self.out.extend(marker(index)),
                    _ => {}
                },
                TypeInner::TypeAppT(a) => {
                    let mut resolved = a.type_.dupe();
                    for _ in 0..16 {
                        let next = match resolved.deref() {
                            TypeInner::AnnotT(_, inner, _) => inner.dupe(),
                            TypeInner::OpenT(tvar) => {
                                match cx.find_constraints(tvar.id() as i32).1 {
                                    constraint::Constraints::Resolved(t) => t,
                                    constraint::Constraints::FullyResolved(s) => {
                                        cx.force_fully_resolved_tvar(&s)
                                    }
                                    constraint::Constraints::Unresolved(_) => break,
                                }
                            }
                            _ => break,
                        };
                        resolved = next;
                    }
                    if let TypeInner::DefT(_, d) = resolved.deref()
                        && let DefTInner::PolyT(p) = d.deref()
                        && self.seen.insert(p.id.dupe())
                    {
                        let mut inner = BTreeSet::new();
                        key_markers_seen(cx, &p.t_out, &mut inner, self.seen);
                        for (tparam, targ) in p.tparams.iter().zip(a.targs.iter()) {
                            if inner.contains(&tparam.name) {
                                self.out.extend(marker(targ));
                            }
                        }
                    }
                }
                _ => {}
            }
            type_visitor::type_default(self, cx, pole, (), t)
        }
    }
    Visitor { out, seen }.type_(cx, Polarity::Positive, (), t);
}

/// The element types of a tuple, or of an array literal with a tuple view.
fn tuple_elements(t: &Type) -> Option<Vec<Type>> {
    let TypeInner::DefT(_, d) = t.deref() else {
        return None;
    };
    let DefTInner::ArrT(a) = d.deref() else {
        return None;
    };
    match a.deref() {
        ArrType::TupleAT(t) => Some(t.elements.iter().map(|e| e.t.dupe()).collect()),
        ArrType::ArrayAT(a) => a
            .tuple_view
            .as_ref()
            .map(|view| view.elements.iter().map(|e| e.t.dupe()).collect()),
        ArrType::ROArrayAT(_) => None,
    }
}

/// An item of a tuple type with spreads.
enum TupleItem {
    Elem(Type),
    Spread(Type),
}

/// The items of the tuple that a spread tuple destructor makes of `spread`, in order, if the
/// tuple is exact.
fn spread_tuple_items(d: &DestructorSpreadTupleTypeData, spread: &Type) -> Option<Vec<TupleItem>> {
    if d.inexact {
        return None;
    }
    let mut items = d
        .resolved
        .iter()
        .map(|p| match p {
            ResolvedParam::ResolvedArg(box ResolvedArgData(elem, _)) => {
                Some(TupleItem::Elem(elem.t.dupe()))
            }
            ResolvedParam::ResolvedSpreadArg(box ResolvedSpreadArgData(r, arr, _)) => {
                Some(TupleItem::Spread(Type::new(TypeInner::DefT(
                    r.dupe(),
                    DefT::new(DefTInner::ArrT(Rc::new(arr.clone()))),
                ))))
            }
            ResolvedParam::ResolvedAnySpreadArg(..) => None,
        })
        .collect::<Option<Vec<_>>>()?;
    items.reverse();
    items.push(TupleItem::Spread(spread.dupe()));
    items.extend(d.unresolved.iter().map(|p| match p {
        UnresolvedParam::UnresolvedArg(box UnresolvedArgData(elem, _)) => {
            TupleItem::Elem(elem.t.dupe())
        }
        UnresolvedParam::UnresolvedSpreadArg(t) => TupleItem::Spread(t.dupe()),
    }));
    Some(items)
}

/// The type of the `i`th element of an array type: an element of a tuple, or the element type.
fn arr_elem_at(t: &Type, i: usize) -> Option<Type> {
    if let Some(elements) = tuple_elements(t) {
        return elements.get(i).map(|t| t.dupe());
    }
    let TypeInner::DefT(_, d) = t.deref() else {
        return None;
    };
    let DefTInner::ArrT(a) = d.deref() else {
        return None;
    };
    match a.deref() {
        ArrType::ArrayAT(a) => Some(a.elem_t.dupe()),
        ArrType::ROArrayAT(a) => Some(a.elem_t.dupe()),
        ArrType::TupleAT(_) => None,
    }
}

/// The elements of an array type from `n` on (`len` of them, if given): a part of a tuple, or the
/// array itself. Not a literal, as `ArrRestT` makes it, so pinning keeps its literal types.
fn arr_slice(t: &Type, n: usize, len: Option<usize>) -> Option<Type> {
    let TypeInner::DefT(r, d) = t.deref() else {
        return None;
    };
    let DefTInner::ArrT(a) = d.deref() else {
        return None;
    };
    let sliced = match a.deref() {
        ArrType::TupleAT(tuple) => {
            let elements: Vec<TupleElement> = tuple
                .elements
                .iter()
                .skip(n)
                .take(len.unwrap_or(usize::MAX))
                .cloned()
                .collect();
            let rest = if len.is_some() {
                None
            } else {
                tuple.rest.clone()
            };
            let required = elements.iter().filter(|e| !e.optional).count() as i32;
            let total = elements.len() as i32;
            let ts = elements
                .iter()
                .chain(rest.iter())
                .map(|e| e.t.dupe())
                .collect();
            let elem_t = type_util::union_of_ts(r.dupe(), ts, None);
            match rest {
                // A tuple with only a rest element is an array, as `ArrRestT` makes it.
                Some(_) if elements.is_empty() => ArrType::ArrayAT(Box::new(ArrayATData {
                    elem_t,
                    tuple_view: None,
                    react_dro: tuple.react_dro.clone(),
                    strictness_kind: tuple.strictness_kind,
                    literal: tuple.literal,
                    fresh: false,
                })),
                _ => ArrType::TupleAT(Box::new(TupleATData {
                    elem_t,
                    elements: elements.into(),
                    arity: (required, total),
                    rest,
                    ..(**tuple).clone()
                })),
            }
        }
        ArrType::ArrayAT(arr) => match &arr.tuple_view {
            Some(view) => {
                let ts = view
                    .elements
                    .iter()
                    .skip(n)
                    .take(len.unwrap_or(usize::MAX))
                    .map(|e| e.t.dupe())
                    .collect();
                return Some(mk_tuple(r, ts, false));
            }
            None => a.deref().clone(),
        },
        ArrType::ROArrayAT(_) => a.deref().clone(),
    };
    Some(Type::new(TypeInner::DefT(
        r.dupe(),
        DefT::new(DefTInner::ArrT(Rc::new(
            sliced.with_literal(ArrLiteral::NonLiteral, false),
        ))),
    )))
}

/// Whether `t` contains a literal type that comes from a value rather than an annotation.
fn has_value_literal<'cx>(cx: &mut Context<'cx>, t: &Type) -> bool {
    find_value_literal(cx, t, false)
}

/// Whether `t` contains a literal type or a template literal type that comes from a value: a
/// literal that implicit instantiation may need to keep precise.
fn has_inferred_literal<'cx>(cx: &mut Context<'cx>, t: &Type) -> bool {
    find_value_literal(cx, t, true)
}

/// The search for a literal of a value skips what only annotations define: the body of a type
/// alias or class, and the parts of a type destructor other than its argument. It also skips
/// function types, whose literals a bound does not need precise. The answer for a resolved type
/// variable, and for a type whose visit met no unresolved one, does not change: it is kept for the
/// check of the file, whose solves meet the same types again and again.
fn find_value_literal<'cx>(cx: &mut Context<'cx>, t: &Type, templates: bool) -> bool {
    struct Visitor {
        found: bool,
        templates: bool,
        tvars: BTreeSet<u32>,
        seen: BTreeSet<usize>,
        /// Counts the unresolved type variables met so far. A type variable or type met again in
        /// this visit counts too: its answer was not kept, so it depends on an unresolved type
        /// variable or on a type variable still being visited. The answer for a type whose visit
        /// adds to the count is not kept.
        incomplete: usize,
    }
    impl TypeVisitor<()> for Visitor {
        // A resolved type variable, as a solution of an earlier pass in a hint.
        fn tvar<'cx>(
            &mut self,
            cx: &mut Context<'cx>,
            pole: Polarity,
            _acc: (),
            _r: &Reason,
            id: u32,
        ) {
            let key = (id, self.templates);
            if let Some(found) = cx.literal_cache_mut().tvars.get(&key).copied() {
                self.found |= found;
                return;
            }
            if !self.tvars.insert(id) {
                self.incomplete += 1;
                return;
            }
            let t = match cx.find_constraints(id as i32).1 {
                constraint::Constraints::Resolved(t) => t,
                constraint::Constraints::FullyResolved(s) => cx.force_fully_resolved_tvar(&s),
                constraint::Constraints::Unresolved(_) => {
                    self.incomplete += 1;
                    return;
                }
            };
            let before = self.incomplete;
            self.type_(cx, pole, (), &t);
            if self.found || self.incomplete == before {
                cx.literal_cache_mut().tvars.insert(key, self.found);
            }
        }

        fn type_<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, _acc: (), t: &Type) {
            if self.found {
                return;
            }
            let key = (addr(t), self.templates);
            if let Some((_, found)) = cx.literal_cache_mut().types.get(&key) {
                self.found |= *found;
                return;
            }
            if !self.seen.insert(addr(t)) {
                self.incomplete += 1;
                return;
            }
            let before = self.incomplete;
            self.visit(cx, pole, t);
            if self.found || self.incomplete == before {
                cx.literal_cache_mut()
                    .types
                    .insert(key, (t.dupe(), self.found));
            }
        }
    }
    impl Visitor {
        fn visit<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, t: &Type) {
            match t.deref() {
                TypeInner::DefT(_, d)
                    if matches!(
                        d.deref(),
                        DefTInner::SingletonStrT {
                            from_annot: false,
                            ..
                        } | DefTInner::SingletonNumT {
                            from_annot: false,
                            ..
                        } | DefTInner::SingletonBoolT {
                            from_annot: false,
                            ..
                        } | DefTInner::SingletonBigIntT {
                            from_annot: false,
                            ..
                        }
                    ) =>
                {
                    self.found = true;
                }
                TypeInner::TemplateLiteralT {
                    from_annot: false, ..
                } if self.templates => {
                    self.found = true;
                }
                // The members of a class are annotated, and they can reach much of the program: of
                // an instance, only its type arguments.
                TypeInner::DefT(_, d) if let DefTInner::InstanceT(i) = d.deref() => {
                    for (_, _, targ, _) in i.inst.type_args.iter() {
                        self.type_(cx, pole, (), targ);
                    }
                }
                TypeInner::DefT(_, d)
                    if matches!(
                        d.deref(),
                        DefTInner::ClassT(_)
                            | DefTInner::PolyT(_)
                            | DefTInner::TypeT(..)
                            | DefTInner::FunT(..)
                    ) => {}
                TypeInner::TypeAppT(a) => {
                    for targ in a.targs.iter() {
                        self.type_(cx, pole, (), targ);
                    }
                }
                TypeInner::EvalT { type_, .. } => self.type_(cx, pole, (), type_),
                _ => type_visitor::type_default(self, cx, pole, (), t),
            }
        }
    }
    let mut visitor = Visitor {
        found: false,
        templates,
        tvars: BTreeSet::new(),
        seen: BTreeSet::new(),
        incomplete: 0,
    };
    visitor.type_(cx, Polarity::Positive, (), t);
    visitor.found
}

/// Whether `t` contains a type destructor.
fn contains_eval<'cx>(cx: &mut Context<'cx>, t: &Type) -> bool {
    struct Visitor(bool);
    impl TypeVisitor<()> for Visitor {
        fn type_<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, _acc: (), t: &Type) {
            if self.0 {
                return;
            }
            match t.deref() {
                TypeInner::EvalT { .. } => self.0 = true,
                _ => type_visitor::type_default(self, cx, pole, (), t),
            }
        }
    }
    let mut visitor = Visitor(false);
    visitor.type_(cx, Polarity::Positive, (), t);
    visitor.0
}

/// A def type without types inside: a primitive, a literal, `empty` or `mixed`.
fn is_atomic(d: &DefTInner) -> bool {
    matches!(
        d,
        DefTInner::NumGeneralT(_)
            | DefTInner::StrGeneralT(_)
            | DefTInner::BoolGeneralT
            | DefTInner::BigIntGeneralT(_)
            | DefTInner::EmptyT
            | DefTInner::MixedT(_)
            | DefTInner::NullT
            | DefTInner::VoidT
            | DefTInner::SymbolT
            | DefTInner::UniqueSymbolT(_)
            | DefTInner::SingletonStrT { .. }
            | DefTInner::NumericStrKeyT(_)
            | DefTInner::SingletonNumT { .. }
            | DefTInner::SingletonBoolT { .. }
            | DefTInner::SingletonBigIntT { .. }
            | DefTInner::EnumValueT(_)
    )
}

fn arr_elem(a: &ArrType) -> Type {
    match a {
        ArrType::ArrayAT(a) => a.elem_t.dupe(),
        ArrType::TupleAT(a) => a.elem_t.dupe(),
        ArrType::ROArrayAT(a) => a.elem_t.dupe(),
    }
}

type Solution = (BTreeMap<SubstName, GeneralizedTarg>, Vec<(Type, SubstName)>);

/// Solves `check` by walking. As the flow-based solve, it restores the caches after, but for
/// the constraint cache entries of flows without errors.
pub(super) fn solve_targs<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: &UseOp,
    allow_underconstrained: bool,
    has_syntactic_hint: bool,
    return_hint: &Option<(Type, flow_common::hint::HintKind)>,
    check: &ImplicitInstantiationCheck,
) -> Result<Solution, FlowJsException> {
    // The level sits below the ones the snapshot rolls back.
    let preserve_level = cx.constraint_cache().level_count();
    cx.constraint_cache_mut().push_level();
    cx.run_and_rolled_back_cache(|cx| {
        solve_targs_in_snapshot(
            cx,
            env,
            preserve_level,
            use_op,
            allow_underconstrained,
            has_syntactic_hint,
            return_hint,
            check,
        )
    })
}

fn solve_targs_in_snapshot<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    preserve: usize,
    use_op: &UseOp,
    allow_underconstrained: bool,
    has_syntactic_hint: bool,
    return_hint: &Option<(Type, flow_common::hint::HintKind)>,
    check: &ImplicitInstantiationCheck,
) -> Result<Solution, FlowJsException> {
    let (_, ref reason_op, ref op) = check.operation;
    let (ref tparams_loc, ref tparams, ref poly_body) = check.poly_t;
    let reason_tapp = type_util::reason_of_t(&check.lhs).dupe();

    let explicit = match op {
        Operation::Call(call) => call.call_targs.as_deref(),
        Operation::Constructor(targs, _) | Operation::ReactJSX { targs, .. } => targs.as_deref(),
        Operation::SubtypeLowerPoly(_) | Operation::Method { .. } => None,
    };
    if let Some(targs) = explicit {
        force_type_arguments(cx, targs);
    }
    // The type arguments of a method call are the method's, forced as in the flow-based solve.
    if let Operation::Method { call, .. } = op
        && let Some(targs) = call.meth_targs.as_deref()
    {
        force_type_arguments(cx, targs);
    }
    // An operation that implicit instantiation ignores has nothing to infer, as in the flow-based
    // solve.
    let Some(mut marked) = marks_of_check(cx, check) else {
        return Ok((BTreeMap::new(), Vec::new()));
    };
    // In the flow-based solve, too many explicit type arguments are an error of instantiating the
    // operation: under speculation it rejects the candidate (an overload), and otherwise it is
    // dropped with the other errors of the solve, but the operation has failed.
    let too_many_targs = explicit.is_some_and(|targs| targs.len() > tparams.len());
    if too_many_targs {
        let errors = cx.errors();
        flow_js_utils::add_output_with_env(
            cx,
            env,
            ErrorMessage::ETooManyTypeArgs(Box::new(ETooManyTypeArgsData {
                reason_tapp: reason_tapp.to_error_reference(),
                arity_loc: tparams_loc.dupe(),
                maximum_arity: tparams.len() as i32,
            })),
        )?;
        cx.reset_errors(errors);
    }

    let mut subst_map = FlowOrdMap::new();
    let mut markers: Vec<Option<SubstName>> = Vec::with_capacity(tparams.len());
    // As in the flow-based solve, `_` or a missing explicit type argument is inferred, and extra
    // ones are ignored.
    for (i, tparam) in tparams.iter().enumerate() {
        match explicit.and_then(|targs| targs.get(i)) {
            Some(Targ::ExplicitArg(t)) => {
                subst_map.insert(tparam.name.dupe(), t.dupe());
                markers.push(None);
            }
            _ => {
                let (name, marker) = mk_marker(cx, tparam);
                subst_map.insert(tparam.name.dupe(), marker);
                markers.push(Some(name));
            }
        }
    }
    let body = subst_markers_with_use_op(cx, Some(use_op.dupe()), &subst_map, poly_body);
    let mut walker = Walker::new(
        env,
        preserve,
        reason_op.loc().dupe(),
        markers.iter().flatten().duped(),
    );
    for (tparam, marker) in tparams.iter().zip(markers.iter()) {
        let Some(name) = marker else {
            continue;
        };
        walker.tparams.insert(name.dupe(), tparam.dupe());
        if let Some(default) = &tparam.default {
            let default = subst(cx, &subst_map, default);
            walker.defaults.insert(name.dupe(), default);
        }
    }
    let mut return_t = None;

    macro_rules! give_up {
        ($label:lifetime, $reason:expr) => {{
            let reason: &str = &$reason;
            log(cx, || {
                format!("Implicit instantiation walk gives up: {}", reason)
            });
            break $label;
        }};
    }
    'op: {
        match op {
            Operation::Call(call) => {
                let body = walker.resolve(cx, &body);
                let funtype = match body.deref() {
                    TypeInner::DefT(_, d) => match d.deref() {
                        DefTInner::FunT(_, f) => f.dupe(),
                        _ => give_up!('op, "callee is not a function"),
                    },
                    _ => give_up!('op, "callee is not a function"),
                };
                let signature = Signature {
                    params: &funtype.params,
                    rest_param: funtype.rest_param.as_ref(),
                    this_param: Some(&funtype.this_t.0),
                    return_t: &funtype.return_t,
                };
                walker.walk_call(
                    cx,
                    reason_op,
                    &call.call_this_t,
                    &call.call_args_tlist,
                    &signature,
                    return_hint,
                )?;
                return_t = Some(funtype.return_t.dupe());
            }
            // `new C(args)`: the arguments against the constructor the class has or inherits, and its
            // instance against the hint.
            Operation::Constructor(_, args) => {
                let body = walker.resolve(cx, &body);
                let instance = match body.deref() {
                    TypeInner::DefT(_, d) => match d.deref() {
                        DefTInner::ClassT(inner) => walker.normalize(cx, inner),
                        _ => give_up!('op, "constructor of a non-class"),
                    },
                    _ => give_up!('op, "constructor of a non-class"),
                };
                let ctor = match walker.method_type_of(cx, &instance, "constructor") {
                    Ok(None) => {
                        // Without a constructor of its own or inherited, a class that extends
                        // `any` gets its constructor from `any`. The flow-based solve calls that
                        // `any` with the new instance as the receiver, which flows the instance to
                        // `any`.
                        if let TypeInner::DefT(_, d) = walker.resolve(cx, &instance).deref()
                            && let DefTInner::InstanceT(i) = d.deref()
                            && let Some(any) = walker.any_super(cx, i)
                        {
                            walker.walk(cx, &instance, &any)?;
                        }
                        None
                    }
                    Ok(Some(ctor)) => match walker.signature_for_call(cx, &ctor, args)? {
                        Some(f) => Some(f),
                        None => give_up!('op, "constructor is not a function"),
                    },
                    Err(no) => give_up!('op, no.why()),
                };
                let no_params: Vec<FunParam> = Vec::new();
                let signature = match &ctor {
                    Some(f) => Signature {
                        params: &f.params,
                        rest_param: f.rest_param.as_ref(),
                        this_param: None,
                        return_t: &instance,
                    },
                    None => Signature {
                        params: &no_params,
                        rest_param: None,
                        this_param: None,
                        return_t: &instance,
                    },
                };
                walker.walk_call(cx, reason_op, &instance, args, &signature, return_hint)?;
                return_t = Some(instance.dupe());
            }
            // `o.m(args)` on a generic receiver: the arguments against the method's parameters.
            Operation::Method { propref, call, .. } => {
                let receiver = walker.resolve(cx, &body);
                let name: Option<FlowSmolStr> = match propref {
                    PropRef::Named { name, .. } => name.as_str_opt().map(FlowSmolStr::new),
                    _ => None,
                };
                let Some(name) = name else {
                    give_up!('op, "computed method");
                };
                // `f.call(this, ...args)` and `f.apply(this, args)` on a generic function call it.
                if let TypeInner::DefT(_, d) = receiver.deref()
                    && let DefTInner::FunT(_, f) = d.deref()
                {
                    let f = f.dupe();
                    let args = &call.meth_args_tlist;
                    let this_arg = match args.first().map(|a| a.deref()) {
                        Some(CallArgInner::Arg(t)) => t.dupe(),
                        Some(CallArgInner::SpreadArg(_)) => {
                            give_up!('op, "spread argument");
                        }
                        None => void::why(reason_op.dupe()),
                    };
                    let signature = Signature {
                        params: &f.params,
                        rest_param: f.rest_param.as_ref(),
                        this_param: Some(&f.this_t.0),
                        return_t: &f.return_t,
                    };
                    let walked = match name.as_str() {
                        "call" => {
                            walker.walk_call(
                                cx,
                                reason_op,
                                &this_arg,
                                args.get(1..).unwrap_or(&[]),
                                &signature,
                                return_hint,
                            )?;
                            Ok(())
                        }
                        "apply" => match args.get(1).map(|a| a.deref()) {
                            Some(CallArgInner::SpreadArg(_)) => Err("spread argument"),
                            arr => {
                                walker.walk(cx, &this_arg, &f.this_t.0)?;
                                if let Some(CallArgInner::Arg(arr)) = arr {
                                    let params =
                                    reverse_resolve_spread_multiflow_subtype_full_no_resolution(
                                        cx,
                                        reason_op,
                                        &f.params,
                                        &f.rest_param,
                                    )?;
                                    walker.walk(cx, arr, &params)?;
                                }
                                if let Some((hint, _)) = return_hint {
                                    walker.in_hint = true;
                                    walker.walk(cx, &f.return_t, hint)?;
                                    walker.in_hint = false;
                                }
                                walker.walk_deferred(cx)?;
                                Ok(())
                            }
                        },
                        _ => Err("method of a function"),
                    };
                    if let Err(reason) = walked {
                        give_up!('op, reason);
                    }
                    return_t = Some(f.return_t.dupe());
                } else {
                    let method = match walker.method_type_of(cx, &receiver, &name) {
                        Ok(Some(method)) => method,
                        Ok(None) => give_up!('op, "method not found"),
                        Err(no) => give_up!('op, no.why()),
                    };
                    match walker.signature_for_call(cx, &method, &call.meth_args_tlist)? {
                        Some(method) => {
                            let signature = Signature {
                                params: &method.params,
                                rest_param: method.rest_param.as_ref(),
                                this_param: Some(&method.this_t.0),
                                return_t: &method.return_t,
                            };
                            walker.walk_call(
                                cx,
                                reason_op,
                                &receiver,
                                &call.meth_args_tlist,
                                &signature,
                                return_hint,
                            )?;
                        }
                        // An overloaded method, or anything else, that says nothing about the type
                        // parameters.
                        _ if !walker.mentions(cx, &method) => {}
                        _ => give_up!('op, "method is not a function"),
                    }
                }
            }
            Operation::SubtypeLowerPoly(u) => {
                walker.walk(cx, &body, u)?;
                walker.walk_deferred(cx)?;
            }
            // `<C {...props} />`: the props against the props the component takes.
            Operation::ReactJSX { jsx_props, .. } => {
                let component = walker.resolve(cx, &body);
                let mut jsx_props = jsx_props.dupe();
                let props = match component.deref() {
                    TypeInner::DefT(_, d) => match d.deref() {
                        DefTInner::FunT(_, f) => f.params.first().map(|FunParam(_, t)| t.dupe()),
                        DefTInner::ReactAbstractComponentT(c) => Some(c.config.dupe()),
                        DefTInner::ClassT(inner) => {
                            let instance = walker.normalize(cx, inner);
                            match walker.class_component_props(cx, &instance)? {
                                Some(props) => {
                                    walker.walk_jsx_ref(cx, &jsx_props, &instance)?;
                                    // Props that are a type parameter get the props of the
                                    // element with the default props, as the config check
                                    // flows them there.
                                    if walker.marker_of(&walker.resolve(cx, &props)).is_some()
                                        && let Some(with_defaults) = walker
                                            .props_with_defaults(cx, &component, &jsx_props)?
                                    {
                                        jsx_props = with_defaults;
                                    }
                                    Some(props)
                                }
                                None => {
                                    give_up!('op, "class component without props");
                                }
                            }
                        }
                        _ => give_up!('op, "jsx component"),
                    },
                    _ => give_up!('op, "jsx component"),
                };
                if let Some(props) = props {
                    walker.walk_jsx_props(cx, &jsx_props, &props)?;
                    walker.walk_deferred(cx)?;
                }
            }
        }
    }

    // The flow-based solve flows each lower bound of a type parameter to its upper bounds. That
    // records the literals an annotation needs precise (`'hi'` against `'hi'`), which pinning
    // then keeps instead of generalizing. Do the same for bounds without markers, dropping the
    // errors. An upper bound with a literal of a value (an invariant position met by a literal)
    // asks for no precision.
    // Only a lower bound with a literal of a value has a literal to keep precise, and only an
    // upper bound without one asks for it. Each bound is checked once, not once per pair, and the
    // lower bounds, which can be large, only when an upper bound asks.
    // The flow-based solve also flows each type parameter to its bound, so the bound is one of
    // those upper bounds, with the solution so far for the type parameters it mentions, which that
    // solve pins first, its literals still precise.
    let open_map: FlowOrdMap<SubstName, Type> = (tparams.iter().zip(markers.iter()))
        .map(|(tparam, marker)| {
            let t = match marker {
                Some(name) => {
                    let bounds = &walker.vars[name];
                    let lower = best(&bounds.lower);
                    match bounds.exact.first() {
                        Some(t) => t.dupe(),
                        None if !lower.is_empty() => {
                            type_util::union_of_ts(tparam.reason.dupe(), lower, None)
                        }
                        None => walker.current_solution(cx, name, &tparam.reason),
                    }
                }
                None => subst_map[&tparam.name].dupe(),
            };
            (tparam.name.dupe(), t)
        })
        .collect();
    let mut pairs: Vec<(Type, Type)> = Vec::new();
    for (tparam, marker) in tparams.iter().zip(markers.iter()) {
        let Some(name) = marker else { continue };
        let bounds = walker.vars[name].clone();
        let lower: Vec<Type> = types(&bounds.lower).duped().collect();
        let upper: Vec<Type> = types(&bounds.upper).duped().collect();
        let as_upper = |walker: &mut Walker<'_>, cx: &mut Context<'cx>, ts: &[Type]| {
            ts.iter()
                .map(|t| !walker.mentions(cx, t) && !has_value_literal(cx, t))
                .collect::<Vec<bool>>()
        };
        let as_lower = |walker: &mut Walker<'_>, cx: &mut Context<'cx>, ts: &[Type]| {
            ts.iter()
                .map(|t| !walker.mentions(cx, t) && has_inferred_literal(cx, t))
                .collect::<Vec<bool>>()
        };
        if !lower.is_empty() {
            let upper_ok = as_upper(&mut walker, cx, &upper);
            if upper_ok.iter().any(|ok| *ok) {
                let lower_ok = as_lower(&mut walker, cx, &lower);
                for (l, _) in lower.iter().zip(&lower_ok).filter(|(_, ok)| **ok) {
                    for (u, _) in upper.iter().zip(&upper_ok).filter(|(_, ok)| **ok) {
                        pairs.push((l.dupe(), u.dupe()));
                    }
                }
            }
        }
        // The tvar flows to the bound with its first exact bound, or its lower bounds. A bound is
        // an annotation: a literal of a value in it was not met in an invariant position, but the
        // solutions so far of the type parameters it mentions can have literals that the flow
        // needs precise.
        if !is_mixed_or_any(&walker.resolve(cx, &tparam.bound)) {
            let bound = subst(cx, &open_map, &tparam.bound);
            if !walker.mentions(cx, &bound) {
                let from = match bounds.exact.first() {
                    Some(t) => vec![t.dupe()],
                    None => lower.clone(),
                };
                let from_ok = as_lower(&mut walker, cx, &from);
                let bound_ok = has_inferred_literal(cx, &bound);
                for (l, _) in from
                    .iter()
                    .zip(&from_ok)
                    .filter(|(l, ok)| **ok || (bound_ok && !walker.mentions(cx, l)))
                {
                    pairs.push((l.dupe(), bound.dupe()));
                }
            }
        }
        // Types met in invariant positions are unified with each other.
        if bounds.exact.len() > 1 {
            let exact_upper_ok = as_upper(&mut walker, cx, &bounds.exact);
            let exact_lower_ok = as_lower(&mut walker, cx, &bounds.exact);
            for (i, a) in bounds.exact.iter().enumerate() {
                for (j, b) in bounds.exact.iter().enumerate() {
                    if i != j && exact_lower_ok[i] && exact_upper_ok[j] {
                        pairs.push((a.dupe(), b.dupe()));
                    }
                }
            }
        }
    }
    // A literal that a marker carries into a computed key or an index is needed precise too,
    // as when the flow-based solve flows it there.
    let mut literal_lowers: BTreeMap<SubstName, Vec<Type>> = BTreeMap::new();
    for (name, bounds) in walker.vars.iter() {
        for l in types(&bounds.lower) {
            let l = walker.resolve(cx, l);
            if is_value_literal(&l) {
                literal_lowers.entry(name.dupe()).or_default().push(l);
            }
        }
    }
    let mut keys = BTreeSet::new();
    if !literal_lowers.is_empty() {
        key_markers(cx, &body, &mut keys);
        for tparam in tparams.iter() {
            let bound = subst(cx, &subst_map, &tparam.bound);
            key_markers(cx, &bound, &mut keys);
        }
    }
    for name in keys.iter() {
        for l in literal_lowers.get(name).into_iter().flatten() {
            flow_js_utils::update_lit_type_from_annot(cx, env, l);
        }
    }
    for (l, u) in pairs {
        // Arguments against the parameters they are spread into are checked one by one, so
        // that a wrong number of them still checks the others. A type that stands for several
        // concrete types, such as a union or a maybe type, meets the bound one concrete type at a
        // time, as they reach a type variable in the flow-based solve: subtyping takes a fast path
        // for a union of literals against another one, which records none of them as needed
        // precise. (A union is a subtype of a type when each of its members is.)
        let pairs = match (
            tuple_elements(&walker.resolve(cx, &l)),
            tuple_elements(&walker.resolve(cx, &u)),
        ) {
            (Some(ls), Some(us)) => ls.into_iter().zip(us).collect(),
            _ => match walker.concretize(cx, &l)? {
                members if members.len() > 1 => {
                    members.into_iter().map(|m| (m, u.dupe())).collect()
                }
                _ => vec![(l, u)],
            },
        };
        for (l, u) in pairs {
            let errors = cx.errors();
            let flowed = run_keeping_clean_flows(cx, env, preserve, |cx| {
                FlowJs::flow_t_with_env(cx, env, &l, &u)
            });
            cx.reset_errors(errors);
            instantiation_solver::sandboxed(flowed)?;
        }
    }

    // In the flow-based solve, a tuple type with a spread of a type variable is resolved once the
    // variable gets the elements: the tuple then meets the elements it was given, which records
    // the literals they need precise (`[1, 2, 3]` against `[1, ...T]` keeps `T = [2, 3]`). Do the
    // same with the solution before generalization.
    let inferred: FlowOrdMap<SubstName, Type> = markers
        .iter()
        .flatten()
        .filter_map(|name| {
            let bounds = &walker.vars[name];
            let t = match bounds.exact.first() {
                Some(t) => t.dupe(),
                None if !bounds.lower.is_empty() => {
                    let lower = types(&bounds.lower).duped().collect();
                    type_util::union_of_ts(reason_op.dupe(), lower, None)
                }
                None => return None,
            };
            Some((name.dupe(), t))
        })
        .collect();
    for (arg, param) in std::mem::take(&mut walker.spread_tuple_pairs) {
        if !has_inferred_literal(cx, &arg) {
            continue;
        }
        let param = subst(cx, &inferred, &param);
        if walker.mentions(cx, &param) {
            continue;
        }
        let errors = cx.errors();
        let flowed = run_keeping_clean_flows(cx, env, preserve, |cx| {
            FlowJs::flow_t_with_env(cx, env, &arg, &param)
        });
        cx.reset_errors(errors);
        instantiation_solver::sandboxed(flowed)?;
    }

    if allow_underconstrained {
        marked = Marked::new();
    }
    let pinning = Pinning {
        use_op,
        check,
        markers: &markers,
        subst_map: &subst_map,
        marked: &marked,
        has_syntactic_hint,
    };
    let init_errors = cx.errors();
    let before_pinning = walker.snapshot();
    let mut result = pin_keeping_errors(cx, env, &mut walker, &pinning, too_many_targs);
    // A type parameter without evidence is not underconstrained when the operation fails anyway
    // with the solution: pin again with the error fallback.
    if !too_many_targs && let Ok(((solution, _), true)) = &result {
        let errors = cx.errors();
        let solution: Vec<Type> = (tparams.iter())
            .map(|tparam| solution[&tparam.name].generalized.dupe())
            .collect();
        let failed = run_keeping_clean_flows(cx, env, preserve, |cx| {
            let (_, tout) = check_instantiation(cx, env, check, Some(&solution))?;
            if let Some(tout) = tout {
                resolve_result(cx, &tout);
            }
            Ok(!errors.ptr_eq(&cx.errors()))
        });
        cx.reset_errors(errors);
        if instantiation_solver::sandboxed(failed)?.unwrap_or(true) {
            cx.reset_errors(init_errors.dupe());
            walker.restore(before_pinning);
            result = pin_keeping_errors(cx, env, &mut walker, &pinning, true);
        }
    }
    let (result, _) = result?;
    // A `_` among the explicit type arguments gets the inferred type, as in the flow-based solve.
    for (tparam, targ) in tparams.iter().zip(explicit.into_iter().flatten()) {
        if let Targ::ImplicitArg(tvar) = targ
            && let Some(solved) = result.0.get(&tparam.name)
        {
            FlowJs::flow_with_env(
                cx,
                env,
                &solved.inferred,
                &UseT::new(UseTInner::UseT(
                    use_op.dupe(),
                    Type::new(TypeInner::OpenT(tvar.dupe())),
                )),
            )?;
        }
    }
    for (name, targ) in result.0.iter() {
        log(cx, || {
            format!(
                "Implicit instantiation walk solved {} = {}",
                name.string_of_subst_name(),
                flow_typing_debug::dump_t(Some(2), cx, &targ.generalized),
            )
        });
    }

    // The flow-based solve flows the result into an expected-type hint, so that a conflict fails an
    // enclosing speculation (an overload). Outside speculation, the flow unifies the type variables
    // of the hint with the solution in invariant positions; its other errors are dropped, so a hint
    // without unresolved type variables, or a result with a destructor, needs no flow there.
    if let (Some((hint, flow_common::hint::HintKind::ExpectedTypeHint)), Some(return_t)) =
        (return_hint, &return_t)
        && (env.speculating() || flow_js_utils::tvar_visitors::has_unresolved_tvars(cx, hint))
    {
        // A conditional type that the type parameters leave undecided is stuck in the flow-based
        // solve, whose result still has them as type variables, so it flows nothing there.
        let undecided_conditional = matches!(
            walker.resolve(cx, return_t).deref(),
            TypeInner::EvalT { defer_use_t, .. }
                if matches!(defer_use_t.2.deref(), Destructor::ConditionalType(_))
        ) && walker.eval_conditional(cx, return_t, false)?.is_none();
        let instantiated =
            if (!env.speculating() && contains_eval(cx, return_t)) || undecided_conditional {
                None
            } else {
                let solution: FlowOrdMap<SubstName, Type> = markers
                    .iter()
                    .zip(tparams.iter())
                    .filter_map(|(marker, tparam)| {
                        let name = marker.as_ref()?;
                        let targ = result.0.get(&tparam.name)?;
                        Some((name.dupe(), targ.generalized.dupe()))
                    })
                    .collect();
                Some(subst(cx, &solution, return_t))
            };
        if let Some(instantiated) = instantiated {
            let errors = cx.errors();
            let flowed = run_keeping_clean_flows(cx, env, preserve, |cx| {
                FlowJs::flow_t_with_env(cx, env, &instantiated, hint)
            });
            cx.reset_errors(errors);
            flowed?;
        }
    }
    Ok(result)
}

/// What pinning needs of a solve.
struct Pinning<'a> {
    use_op: &'a UseOp,
    check: &'a ImplicitInstantiationCheck,
    /// The marker of each type parameter, `None` for an explicit type argument.
    markers: &'a [Option<SubstName>],
    /// The type parameters to their markers or explicit type arguments.
    subst_map: &'a FlowOrdMap<SubstName, Type>,
    marked: &'a Marked<SubstName>,
    has_syntactic_hint: bool,
}

/// The bounds a solution comes from.
enum Source {
    /// The first exact bound, or the lower bounds.
    Lower,
    Upper,
}

/// Pins the type parameters, without the caches and errors of pinning other than the
/// underconstrained ones, and says whether there are any of those.
fn pin_keeping_errors<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    walker: &mut Walker<'_>,
    pinning: &Pinning<'_>,
    has_new_errors: bool,
) -> Result<(Solution, bool), FlowJsException> {
    let init_errors = cx.errors();
    cx.run_and_rolled_back_cache(|cx| {
        cx.reset_errors(Default::default());
        let result = pin(cx, env, walker, pinning, has_new_errors);
        let pinning_errors = cx.errors().filter(|error| {
            matches!(
                error.msg,
                ErrorMessage::EImplicitInstantiationUnderconstrainedError(..)
                    | ErrorMessage::EInternal(..)
            )
        });
        let underconstrained = !pinning_errors.is_empty();
        cx.reset_errors(init_errors.union(&pinning_errors));
        result.map(|result| (result, underconstrained))
    })
}

/// Pins each type parameter to its bounds, in the order of `type_param_pin_order`, as `pin_types`
/// pins the tvars of the flow-based solve: its first exact bound; otherwise the union of its lower
/// bounds or, for a type parameter that the result mentions only negatively, the most specific of
/// its upper bounds, each with the other as the fallback, and of either the bounds of the highest
/// priority. A type parameter with neither takes `error_fallback`, its default or its bound, or is
/// underconstrained, as there. The evidence of a pinned type parameter is then walked against its
/// bound, as the flow-based solve flows each tvar to its bound: that gives the type parameters in
/// the bound, which are pinned after it, their evidence from it. A solution that mentions the
/// marker of a type parameter pinned after it takes that one's solution.
fn pin<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    walker: &mut Walker<'_>,
    pinning: &Pinning<'_>,
    has_new_errors: bool,
) -> Result<Solution, FlowJsException> {
    let (_, ref reason_op, ref op) = pinning.check.operation;
    let tparams = &pinning.check.poly_t.1;
    let reason_tapp = type_util::reason_of_t(&pinning.check.lhs).dupe();
    let mut solutions: Vec<Option<Type>> = vec![None; tparams.len()];
    let mut solved = FlowOrdMap::new();
    for index in type_param_pin_order(cx, tparams) {
        let tparam = &tparams[index];
        let bound = subst(cx, pinning.subst_map, &tparam.bound);
        let (t, source) = match &pinning.markers[index] {
            None => {
                let t = (pinning.subst_map.get(&tparam.name))
                    .expect("explicit type argument")
                    .dupe();
                log_pinned(cx, tparam, &t);
                (t, Some(Source::Lower))
            }
            Some(name) => {
                let reason = instantiation_utils::implicit_type_argument::mk_targ_reason(
                    tparam,
                    reason_op,
                    &reason_tapp,
                );
                let polarity = pinning.marked.get(&tparam.name);
                let bounds = &walker.vars[name];
                let (t, source) =
                    match pin_bounds(cx, env, bounds, tparam, polarity, &reason, &solved)? {
                        Some((t, source)) => (t, Some(source)),
                        // The operation failed: a type parameter without evidence is not
                        // underconstrained.
                        None if polarity.is_some() && has_new_errors => {
                            let t = match *cx.typing_mode() {
                                TypingMode::CheckingMode => any_t::error(reason),
                                TypingMode::SynthesisMode { .. }
                                | TypingMode::HintEvaluationMode => {
                                    placeholder_t::why(reason.dupe(), mixed_t::why(reason))
                                }
                            };
                            log_pinned(cx, tparam, &t);
                            (t, None)
                        }
                        None => {
                            let t = UnmarkedFallback::Bound.without_bounds(
                                cx,
                                env,
                                pinning.use_op,
                                tparam,
                                polarity.is_some(),
                                &reason,
                                reason_op,
                            )?;
                            (t, None)
                        }
                    };
                solved.insert(name.dupe(), t.dupe());
                (t, source)
            }
        };
        // As the flow-based solve flows the lower bounds of a pinned tvar to its bound.
        let walk_to_bound = walker.mentions(cx, &bound) && matches!(source, Some(Source::Lower));
        if walk_to_bound {
            walker.in_bound = true;
            let walked = walker.walk(cx, &t, &bound);
            walker.in_bound = false;
            walked?;
        }
        solutions[index] = Some(t);
    }

    // A solution mentions the marker of a type parameter pinned after it only through evidence
    // that mentions it. In a cycle of those, the marker is `unknown`.
    let mut solutions: Vec<Type> = solutions
        .into_iter()
        .map(|t| t.expect("each type parameter is pinned"))
        .collect();
    for _ in 0..=tparams.len() {
        let pending: Vec<usize> = (0..solutions.len())
            .filter(|i| walker.mentions(cx, &solutions[*i]))
            .collect();
        if pending.is_empty() {
            break;
        }
        let map: FlowOrdMap<SubstName, Type> = (pinning.markers.iter())
            .zip(solutions.iter())
            .filter_map(|(m, t)| Some((m.as_ref()?.dupe(), t.dupe())))
            .collect();
        for i in pending {
            solutions[i] = subst(cx, &map, &solutions[i]);
        }
    }
    let unknown: FlowOrdMap<SubstName, Type> = (pinning.markers.iter())
        .zip(tparams.iter())
        .filter_map(|(m, tparam)| Some((m.as_ref()?.dupe(), mixed_t::why(tparam.reason.dupe()))))
        .collect();

    // The flow-based solve flows each tvar to its bound once the tvars that the bound mentions are
    // pinned. That records the literals the bound needs precise, also those it needs through a
    // type parameter pinned after this one, which the solution so far that the solve checks bounds
    // with before pinning does not know. Do the same with the solutions, dropping the errors. As
    // there, a bound with a literal of a value asks for no precision.
    let solved: FlowOrdMap<SubstName, Type> = (pinning.markers.iter())
        .zip(solutions.iter())
        .filter_map(|(m, t)| Some((m.as_ref()?.dupe(), t.dupe())))
        .collect();
    for (tparam, t) in tparams.iter().zip(solutions.iter()) {
        let bound = subst(cx, pinning.subst_map, &tparam.bound);
        if !walker.mentions(cx, &bound) || walker.mentions(cx, t) || !has_inferred_literal(cx, t) {
            continue;
        }
        let bound = subst(cx, &solved, &bound);
        if walker.mentions(cx, &bound) || has_value_literal(cx, &bound) {
            continue;
        }
        let errors = cx.errors();
        let flowed = run_keeping_clean_flows(cx, env, walker.preserve, |cx| {
            FlowJs::flow_t_with_env(cx, env, t, &bound)
        });
        cx.reset_errors(errors);
        instantiation_solver::sandboxed(flowed)?;
    }

    let call_loc = reason_op.loc().dupe();
    let mut result_map = BTreeMap::new();
    let mut result_list = Vec::with_capacity(tparams.len());
    for (tparam, inferred) in tparams.iter().zip(solutions) {
        let inferred = if walker.mentions(cx, &inferred) {
            subst(cx, &unknown, &inferred)
        } else {
            inferred
        };
        let generalized = match op {
            Operation::Call(_) | Operation::Constructor(_, _) | Operation::Method { .. }
                if tparam.is_const =>
            {
                // Adjust 'const' type parameters. Keeping container reasons for
                // compatibility with existing code.
                natural_inference::convert_literal_type_to_const(
                    call_loc.dupe(),
                    cx,
                    inferred.dupe(),
                )
            }
            _ if matches!(tparam.bound.deref(), TypeInner::DefT(_, d)
                if matches!(d.deref(), DefTInner::NumGeneralT(_))) =>
            {
                inferred.dupe()
            }
            // Prevent leaking of precise singleton types
            _ => generalize_singletons(cx, &call_loc, pinning.has_syntactic_hint, inferred.dupe()),
        };
        result_list.push((generalized.dupe(), tparam.name.dupe()));
        result_map.insert(
            tparam.name.dupe(),
            GeneralizedTarg {
                tparam: tparam.dupe(),
                inferred,
                generalized,
            },
        );
    }
    Ok((result_map, result_list))
}

/// The solution of `tparam` from its `bounds`, as `pin_type` pins a tvar, with the markers of the
/// type parameters already pinned replaced by their solutions. `None` without usable bounds.
fn pin_bounds<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    bounds: &Bounds,
    tparam: &TypeParam,
    polarity: Option<Polarity>,
    reason: &Reason,
    solved: &FlowOrdMap<SubstName, Type>,
) -> Result<Option<(Type, Source)>, FlowJsException> {
    let with_solved = |cx: &mut Context<'cx>, t: &Type| {
        let t = if solved.is_empty() {
            t.dupe()
        } else {
            subst(cx, solved, t)
        };
        (!flow_js_utils::tvar_visitors::has_placeholders(cx, &t)).then_some(t)
    };
    if let Some(t) = bounds.exact.first()
        && let Some(t) = with_solved(cx, t)
    {
        log_pinned(cx, tparam, &t);
        return Ok(Some((t, Source::Lower)));
    }
    // Each type once, in the order the walk met them, the upper bounds also in the order of their
    // priorities: the flow-based solve merges the uses through which its reversals read their bounds
    // after the others.
    let mut seen = BTreeSet::new();
    let lowers: Vec<Type> = types(&bounds.lower)
        .filter_map(|t| with_solved(cx, t))
        .filter(|t| seen.insert(t.dupe()))
        .collect();
    let mut by_priority: Vec<&(Type, Priority)> = bounds.upper.iter().collect();
    by_priority.sort_by_key(|(_, p)| *p);
    let mut seen = BTreeSet::new();
    let uppers: Vec<Type> = (by_priority.into_iter())
        .filter_map(|(t, _)| with_solved(cx, t))
        .filter(|t| seen.insert(t.dupe()))
        .collect();
    let filter_empty = tparam.default.is_some();
    let merge_lowers = |cx: &mut Context<'cx>| {
        if lowers.is_empty() {
            return None;
        }
        let mut collected = Vec::new();
        collect_pinning_lowers(
            cx,
            env,
            &mut BTreeSet::new(),
            &mut collected,
            lowers.iter().duped().collect(),
        );
        // Lower bounds that are tvars holding the same type are one bound.
        let mut seen = BTreeSet::new();
        let filtered: Vec<Type> = union_flatten_list(collected)
            .into_iter()
            .filter(|t| seen.insert(t.dupe()))
            .filter(|t| {
                !(filter_empty
                    && matches!(t.deref(), TypeInner::DefT(_, d)
                            if matches!(d.deref(), DefTInner::EmptyT)))
            })
            .filter(|t| !flow_js_utils::tvar_visitors::has_placeholders(cx, t))
            .collect();
        type_util::union_of_ts_opt(
            reason.dupe(),
            filtered,
            Some(union_rep::UnionKind::ImplicitInstantiationKind),
        )
    };
    // The upper bounds folded to a common subtype: each replaces the one taken so far if it is a
    // subtype of it. An upper bound of `any` bounds nothing: it replaces none, so that the order of
    // the bounds does not decide the solution.
    let merge_uppers = |cx: &mut Context<'cx>| -> Result<Option<Type>, FlowJsException> {
        let mut acc: Option<Type> = None;
        for t in uppers.iter() {
            let t = match type_util::constraint_node_id(t) {
                Some(_) => match merge_upper_bounds(cx, env, &mut BTreeSet::new(), t)? {
                    UseTResult::UpperT(t)
                        if !flow_js_utils::tvar_visitors::has_placeholders(cx, &t) =>
                    {
                        t
                    }
                    _ => continue,
                },
                None => t.dupe(),
            };
            acc = Some(match acc {
                Some(prev) if matches!(t.deref(), TypeInner::AnyT(..)) => prev,
                Some(prev)
                    if !(concrete_type_eq::eq_with_env(cx, env, &prev, &t)
                        || type_util::quick_subtype(None::<&mut fn(&Type)>, &t, &prev)
                        || speculative_subtyping_succeeds(
                            cx,
                            env,
                            DepthTrace::dummy_trace(),
                            unknown_use(),
                            &t,
                            &prev,
                        )?) =>
                {
                    prev
                }
                _ => t,
            });
        }
        Ok(acc)
    };
    let merged = match polarity {
        Some(Polarity::Negative) => match merge_uppers(cx)? {
            Some(t) => Some((t, Source::Upper)),
            None => merge_lowers(cx).map(|t| (t, Source::Lower)),
        },
        None | Some(Polarity::Positive | Polarity::Neutral) => match merge_lowers(cx) {
            Some(t) => Some((t, Source::Lower)),
            None => merge_uppers(cx)?.map(|t| (t, Source::Upper)),
        },
    };
    if let Some((t, _)) = &merged {
        log_pinned(cx, tparam, t);
    }
    Ok(merged)
}
