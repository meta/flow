/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! The type normalizer converts inferred types (of type `Type.t`) under a context
//! cx to the simplified form of type `Ty.t`. It is called by various modules,
//! e.g. type-at-pos, coverage, dump-types, and so is parameterized by a
//! configuration struct, instantiated by the client.
//!
//! The type normalizer should only be used on types arising from "fully merged"
//! contexts -- that is, contexts which have all dependencies copied in and
//! constraints evaluated.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocFuzzy;
use flow_aloc::ALocId;
use flow_common::polarity::Polarity;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc as ReasonDesc;
use flow_common::subst_name::SubstName;
use flow_common_ty::ty;
use flow_common_ty::ty::ALocDecl;
use flow_common_ty::ty::ALocElt;
use flow_common_ty::ty::ALocTy;
use flow_common_ty::ty::BotKind;
use flow_common_ty::ty::UpperBoundKind;
use flow_common_ty::ty_symbol::ALocSymbol;
use flow_common_ty::ty_symbol::ImportMode;
use flow_common_ty::ty_symbol::Provenance;
use flow_common_ty::ty_symbol::Symbol;
use flow_common_ty::ty_symbol::builtin_symbol;
use flow_common_ty::ty_utils::simplify_elt;
use flow_common_ty::ty_utils::size_of_type;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::file_key::FileKeyInner;
use flow_parser_utils::file_sig::FileSig;
use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_flow_common::type_subst;
use flow_typing_type::type_::AnySource;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::Destructor;
use flow_typing_type::type_::DestructorConditionalTypeData;
use flow_typing_type::type_::DestructorMappedTypeData;
use flow_typing_type::type_::DestructorSpreadTupleTypeData;
use flow_typing_type::type_::DestructorSpreadTypeData;
use flow_typing_type::type_::EnumInfoInner;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::ModuleType;
use flow_typing_type::type_::NominalType;
use flow_typing_type::type_::OptionalIndexedAccessIndex;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::PolyTKind;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::ReactAbstractComponentTData;
use flow_typing_type::type_::ResolvedArgData;
use flow_typing_type::type_::ResolvedSpreadArgData;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::ThisTypeAppTData;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeMap;
use flow_typing_type::type_::UnresolvedArgData;
use flow_typing_type::type_::eval as type_eval;
use flow_typing_type::type_::nominal;
use flow_typing_type::type_util;
use flow_utils_concurrency::job_error::JobError;

use crate::env::Env;
use crate::env::EvaluateTypeDestructorsMode;
use crate::env::Genv;
use crate::env::ImportedNames;
use crate::env::ImportedNamesMap;
use crate::env::Options;

// =============================================================================
// Error Reporting
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    BadMethodType,
    BadCallProp,
    BadClassT,
    BadMappedType,
    BadThisClassT,
    BadPoly,
    BadTypeAlias,
    BadTypeApp,
    BadInlineInterfaceExtends,
    BadInstanceT,
    BadEvalT,
    BadUse,
    ShadowTypeParam,
    SyntheticBoundT,
    UnexpectedTypeCtor(&'static str),
    UnsupportedTypeCtor,
    UnsupportedUseCtor,
    RecursionLimit,
    TypeSizeLimit,
    /// Worker cancellation surfaced from a non-speculating flow_js call inside the
    /// normalizer continuation. The continuation type A is generic, so we encode
    /// the cancel as an Error variant that callers must propagate.
    WorkerCanceled,
    /// Per-file budget exceeded.
    TimedOut,
    /// `$Flow$DebugThrow` was hit inside a normalizer continuation; the cascade
    /// surfaces it the same way as cancel/timeout so the per-file boundary in
    /// `merge_service::mk_check` can map it to `InternalError::DebugThrow`.
    DebugThrow,
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorKind::BadMethodType => write!(f, "Bad method type"),
            ErrorKind::BadCallProp => write!(f, "Bad call property"),
            ErrorKind::BadClassT => write!(f, "Bad class"),
            ErrorKind::BadMappedType => write!(f, "Bad mapped type"),
            ErrorKind::BadThisClassT => write!(f, "Bad this class"),
            ErrorKind::BadPoly => write!(f, "Bad polymorphic type"),
            ErrorKind::BadTypeAlias => write!(f, "Bad type alias"),
            ErrorKind::BadTypeApp => write!(f, "Bad type application"),
            ErrorKind::BadInlineInterfaceExtends => write!(f, "Bad inline interface extends"),
            ErrorKind::BadInstanceT => write!(f, "Bad instance type"),
            ErrorKind::BadEvalT => write!(f, "Bad eval"),
            ErrorKind::BadUse => write!(f, "Bad use"),
            ErrorKind::ShadowTypeParam => write!(f, "Shadowed type parameters"),
            ErrorKind::SyntheticBoundT => write!(f, "Synthetic type parameter"),
            ErrorKind::UnexpectedTypeCtor(c) => write!(f, "Unexpected type constructor ({})", c),
            ErrorKind::UnsupportedTypeCtor => write!(f, "Unsupported type constructor"),
            ErrorKind::UnsupportedUseCtor => write!(f, "Unsupported use constructor"),
            ErrorKind::RecursionLimit => write!(f, "recursion limit"),
            ErrorKind::TypeSizeLimit => write!(f, "type size limit"),
            ErrorKind::WorkerCanceled => write!(f, "Worker canceled"),
            ErrorKind::TimedOut => write!(f, "Per-file budget exceeded"),
            ErrorKind::DebugThrow => write!(f, "$Flow$DebugThrow"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub msg: String,
}

impl Error {
    pub fn new(kind: ErrorKind, msg: String) -> Self {
        Self { kind, msg }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.kind, self.msg)
    }
}

impl From<JobError> for Error {
    fn from(e: JobError) -> Self {
        match e {
            JobError::Canceled(_) => {
                Error::new(ErrorKind::WorkerCanceled, String::from("Worker canceled"))
            }
            JobError::TimedOut(_) => Error::new(
                ErrorKind::TimedOut,
                String::from("Per-file budget exceeded"),
            ),
            JobError::DebugThrow { .. } => {
                Error::new(ErrorKind::DebugThrow, String::from("$Flow$DebugThrow"))
            }
        }
    }
}

// Utility that determines the next immediate concrete constructor, ie. reads
// through OpenTs and AnnotTs. This is useful in determining, for example, the
// toplevel cosntructor and adjusting the logic accordingly.
pub mod lookahead {
    use std::collections::HashSet;
    use std::ops::Deref;

    use dupe::Dupe;
    use flow_typing_context::Context;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::constraint::Constraints;
    use flow_typing_type::type_util;

    #[derive(Debug, Clone)]
    pub enum Lookahead {
        Recursive,
        LowerBounds(Vec<Type>),
    }

    #[derive(Debug, Clone, Copy)]
    struct RecursiveError;

    pub fn peek<'cx>(cx: &mut Context<'cx>, t: &Type) -> Lookahead {
        peek_types(cx, std::slice::from_ref(t))
    }

    pub fn peek_types<'cx>(cx: &mut Context<'cx>, ts: &[Type]) -> Lookahead {
        fn loop_<'cx>(
            cx: &mut Context<'cx>,
            acc: &mut Vec<Type>,
            seen: &mut HashSet<i32>,
            t: &Type,
        ) -> Result<(), RecursiveError> {
            match t.deref() {
                _ if let Some(node) = type_util::constraint_node_id(t) => {
                    let (root_id, constraints) = cx.find_constraints(node.id());
                    if seen.contains(&root_id) {
                        return Err(RecursiveError);
                    }
                    let inserted = seen.insert(root_id);
                    match constraints {
                        Constraints::Resolved(t) => loop_(cx, acc, seen, &t)?,
                        Constraints::FullyResolved(s) => {
                            let t = cx.force_fully_resolved_tvar(&s);
                            loop_(cx, acc, seen, &t)?;
                        }
                        Constraints::Unresolved(bounds) => {
                            let ts: Vec<_> = bounds.borrow().lower.keys().cloned().collect();
                            for t in ts {
                                loop_(cx, acc, seen, &t)?;
                            }
                        }
                    }
                    if inserted {
                        seen.remove(&root_id);
                    }
                    Ok(())
                }
                TypeInner::AnnotT(_, inner_t, _) => loop_(cx, acc, seen, inner_t),
                _ => {
                    acc.push(t.dupe());
                    Ok(())
                }
            }
        }

        let mut seen = HashSet::new();
        let mut acc = Vec::new();
        for t in ts {
            if loop_(cx, &mut acc, &mut seen, t).is_err() {
                return Lookahead::Recursive;
            }
        }
        Lookahead::LowerBounds(acc)
    }
}

pub trait NormalizerInput {
    fn eval<'cx>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        should_eval: bool,
        cont: impl FnOnce(&mut Env<'_, 'cx>, &mut State, &Type) -> Result<ALocTy, Error>,
        default: impl FnOnce(&mut Env<'_, 'cx>, &mut State, &Type) -> Result<ALocTy, Error>,
        non_eval: impl FnOnce(
            &mut Env<'_, 'cx>,
            &mut State,
            &Type,
            &Destructor,
        ) -> Result<ALocTy, Error>,
        x: (
            &Type,
            &flow_typing_type::type_::TypeDestructorT,
            &type_eval::Id,
        ),
    ) -> Result<ALocTy, Error>;

    fn keys<'cx, A>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        should_evaluate: bool,
        cont: impl FnOnce(&mut Env<'_, 'cx>, &mut State, Type) -> Result<A, Error>,
        default: impl FnOnce(&mut Env<'_, 'cx>, &mut State) -> Result<A, Error>,
        reason: Reason,
        t: Type,
    ) -> Result<A, Error>;

    /// Reduce mapped type property and key expressions before normalizing their bodies.
    fn typeapp<'cx, A>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        cont: &mut dyn FnMut(&mut Env<'_, 'cx>, &mut State, &Type) -> Result<A, Error>,
        type_: &mut dyn FnMut(&mut Env<'_, 'cx>, &mut State, &Type) -> Result<A, Error>,
        app: impl FnOnce(Result<A, Error>, Vec<Result<A, Error>>) -> Result<A, Error>,
        from_value: bool,
        reason: Reason,
        t: Type,
        targs: &[Type],
    ) -> Result<A, Error> {
        match env.cx.find_resolved(&t).as_deref() {
            Some(TypeInner::DefT(_, def_t))
                if let DefTInner::PolyT(box PolyTData {
                    kind: PolyTKind::MappedType,
                    tparams,
                    t_out,
                    ..
                }) = def_t.deref() =>
            {
                if tparams.len() != targs.len() {
                    return Err(terr(
                        ErrorKind::BadTypeApp,
                        Some("unsaturated type function"),
                        Some(&t),
                    ));
                }
                let bindings: FlowOrdMap<SubstName, Type> = tparams
                    .iter()
                    .zip(targs)
                    .map(|(parameter, argument)| (parameter.name.dupe(), argument.dupe()))
                    .collect();
                let body = type_subst::subst(
                    env.cx,
                    None,
                    true,
                    false,
                    type_subst::Purpose::Normal,
                    &bindings,
                    t_out.dupe(),
                );
                cont(env, state, &body)
            }
            _ => Self::typeapp_regular(env, state, cont, type_, app, from_value, reason, t, targs),
        }
    }

    /// Normalize regular applications through the backend's instantiation policy.
    fn typeapp_regular<'cx, A>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        cont: &mut dyn FnMut(&mut Env<'_, 'cx>, &mut State, &Type) -> Result<A, Error>,
        type_: &mut dyn FnMut(&mut Env<'_, 'cx>, &mut State, &Type) -> Result<A, Error>,
        app: impl FnOnce(Result<A, Error>, Vec<Result<A, Error>>) -> Result<A, Error>,
        from_value: bool,
        reason: Reason,
        t: Type,
        targs: &[Type],
    ) -> Result<A, Error>;

    fn builtin_type<'cx, A>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        cont: &mut dyn FnMut(&mut Env<'_, 'cx>, &mut State, Type) -> Result<A, Error>,
        reason: Reason,
        name: &str,
    ) -> Result<A, Error>;

    fn builtin_typeapp<'cx, A>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        cont: &mut dyn FnMut(&mut Env<'_, 'cx>, &mut State, Type) -> Result<A, Error>,
        type_: &mut dyn FnMut(&mut Env<'_, 'cx>, &mut State, Type) -> Result<A, Error>,
        app: impl FnOnce(Result<A, Error>, Vec<Result<A, Error>>) -> Result<A, Error>,
        reason: Reason,
        name: &str,
        targs: &[Type],
    ) -> Result<A, Error>;
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq, Hash)]
pub enum IdKey {
    TVarKey(i32),
    EvalKey(type_eval::Id),
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub rec_tvar_ids: BTreeSet<i32>,
    pub rec_eval_ids: BTreeSet<type_eval::Id>,
    pub found_computed_type: bool,
    remaining_type_size: Option<usize>,
}

impl State {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn found_computed_type(&self) -> bool {
        self.found_computed_type
    }

    fn reset_type_size_budget(&mut self, max_type_size: Option<usize>) {
        self.remaining_type_size = max_type_size;
    }

    fn consume_type_node(&mut self) -> Result<(), Error> {
        match self.remaining_type_size {
            None => Ok(()),
            Some(0) => Err(Error::new(
                ErrorKind::TypeSizeLimit,
                String::from("Type size limit exceeded"),
            )),
            Some(remaining) => {
                self.remaining_type_size = Some(remaining - 1);
                Ok(())
            }
        }
    }
}

// Each run of the monad gets assigned its own id.
thread_local! {
    static RUN_ID: Cell<i32> = const { Cell::new(0) };
}

// Wrapper around 'run' that assigns a distinct id to every run of the monad.
fn run<T, F>(state: &mut State, f: F) -> Result<T, Error>
where
    F: FnOnce(&mut State) -> Result<T, Error>,
{
    let result = f(state);
    RUN_ID.with(|id| id.set(id.get() + 1));
    result
}

fn get_run_id() -> i32 {
    RUN_ID.with(|id| id.get())
}

/// The prototype properties to show for an inline interface: its methods and
/// accessors. An interface the object kit builds by slicing another type (`Omit`,
/// `Pick`, mapped types) has no class id and has already folded every member it
/// keeps into its own properties; its prototype is the source's, so showing it
/// would bring back members the slice dropped.
fn inline_interface_proto_props(
    inst: &flow_typing_type::type_::InstType,
) -> Option<flow_typing_type::type_::properties::Id> {
    (inst.class_id != ALocId::none()).then(|| inst.proto_props.dupe())
}

fn terr(kind: ErrorKind, msg: Option<&str>, t: Option<&Type>) -> Error {
    let t_str = t.map(|t| {
        format!(
            "Raised on type: {}",
            flow_typing_type::type_::string_of_ctor(t)
        )
    });
    let parts: Vec<String> = [msg.map(|s| s.to_string()), t_str]
        .into_iter()
        .flatten()
        .collect();
    let msg = parts.join(", ");
    Error::new(kind, msg)
}

fn descend<'cx>(env: &mut Env<'_, 'cx>, t: &Type) -> Result<(), Error> {
    let depth = env.depth;
    env.descend();
    match env.max_depth() {
        Some(max_depth) if depth > max_depth => Err(terr(ErrorKind::RecursionLimit, None, Some(t))),
        _ => Ok(()),
    }
}

// Update state
fn add_rec_id(state: &mut State, id: IdKey) {
    match id {
        IdKey::TVarKey(tvar_id) => {
            state.rec_tvar_ids.insert(tvar_id);
        }
        IdKey::EvalKey(eval_id) => {
            state.rec_eval_ids.insert(eval_id);
        }
    }
}

fn is_rec_id(state: &State, id: IdKey) -> bool {
    match id {
        IdKey::TVarKey(tvar_id) => state.rec_tvar_ids.contains(&tvar_id),
        IdKey::EvalKey(eval_id) => state.rec_eval_ids.contains(&eval_id),
    }
}

// =============================================================================
// Type Constructors
// =============================================================================

fn generic_talias(name: ALocSymbol, targs: Option<Vec<ALocTy>>) -> ALocTy {
    Arc::new(ty::mk_generic_talias(name, targs.map(Into::into)))
}

fn empty_type() -> ALocTy {
    Arc::new(ty::Ty::Bot(BotKind::EmptyType))
}

fn mk_empty(bot_kind: BotKind<ALoc>) -> ALocTy {
    match bot_kind {
        BotKind::EmptyType => empty_type(),
        BotKind::NoLowerWithUpper(_) => Arc::new(ty::Ty::Bot(bot_kind)),
    }
}

// This is intended to only be used by ty_normaizer_debug
fn app_on_generic(
    c: Result<ALocTy, Error>,
    ts: Vec<Result<ALocTy, Error>>,
) -> Result<ALocTy, Error> {
    let c = c?;
    let ts: Vec<ALocTy> = ts.into_iter().collect::<Result<_, Error>>()?;
    Ok(match c.as_ref() {
        ty::Ty::Generic(box (s, k, _)) => Arc::new(ty::Ty::Generic(Box::new((
            s.clone(),
            k.clone(),
            Some(ts.into()),
        )))),
        _ => c,
    })
}

fn symbol_from_loc<'cx>(
    env: &mut Env<'_, 'cx>,
    sym_def_loc: ALoc,
    sym_name: FlowSmolStr,
) -> Result<ALocSymbol, Error> {
    let symbol_source = sym_def_loc.source();
    let current_source = env.cx.file();
    let sym_provenance = match symbol_source {
        Some(def_file) if current_source == def_file => Provenance::Local,
        Some(def_file) if env.cx.is_global_libdef(def_file) => {
            Provenance::Library(flow_common_ty::ty_symbol::RemoteInfo {
                imported_as: env
                    .imported_names()?
                    .get(&ALocFuzzy(sym_def_loc.dupe()))
                    .cloned(),
            })
        }
        Some(def_file) if matches!(def_file.inner(), FileKeyInner::SourceFile(_)) => {
            Provenance::Remote(flow_common_ty::ty_symbol::RemoteInfo {
                imported_as: env
                    .imported_names()?
                    .get(&ALocFuzzy(sym_def_loc.dupe()))
                    .cloned(),
            })
        }
        Some(_) => Provenance::Local,
        None => Provenance::Local,
    };
    let sym_anonymous = sym_name.as_str() == "<<anonymous class>>";
    Ok(Symbol {
        sym_provenance,
        sym_def_loc,
        sym_name,
        sym_anonymous,
    })
}

fn ty_symbol_from_symbol<'cx>(
    env: &mut Env<'_, 'cx>,
    symbol: &flow_common::flow_symbol::Symbol,
) -> Result<ALocSymbol, Error> {
    symbol_from_loc(
        env,
        symbol.def_loc_of_symbol().dupe(),
        FlowSmolStr::new(symbol.name()),
    )
}

// NOTE Due to repositioning, `reason_loc` may not point to the actual location
// where `name` was defined.
fn symbol_from_reason<'cx>(
    env: &mut Env<'_, 'cx>,
    reason: &Reason,
    name: FlowSmolStr,
) -> Result<ALocSymbol, Error> {
    let def_loc = reason.def_loc().dupe();
    symbol_from_loc(env, def_loc, name)
}

fn remove_targs_matching_defaults(
    targs: Option<Vec<ALocTy>>,
    tparams: Option<&Vec<ty::TypeParam<ALoc>>>,
) -> Option<Vec<ALocTy>> {
    fn matches_default(targ: &ALocTy, tparam: &ty::TypeParam<ALoc>) -> bool {
        match &tparam.tp_default {
            Some(default) => default.as_ref() == targ.as_ref(),
            None => false,
        }
    }

    fn remove_if_able(
        mut targ_lst: Vec<ALocTy>,
        tparam_lst: &[ty::TypeParam<ALoc>],
    ) -> Vec<ALocTy> {
        let paired_len = targ_lst.len().min(tparam_lst.len());
        let mut truncate_to = targ_lst.len();
        for i in (0..paired_len).rev() {
            if truncate_to == i + 1 && matches_default(&targ_lst[i], &tparam_lst[i]) {
                truncate_to = i;
            } else {
                break;
            }
        }
        targ_lst.truncate(truncate_to);
        targ_lst
    }

    match (targs, tparams) {
        (Some(targ_lst), Some(tparam_lst)) => Some(remove_if_able(targ_lst, tparam_lst)),
        (targs, _) => targs,
    }
}

fn normalize_type_members<F>(
    mut f: F,
    types: Vec<Type>,
    state: &mut State,
    max_type_size: Option<usize>,
) -> Result<Vec<ALocTy>, Error>
where
    F: FnMut(&Type, &mut State) -> Result<ALocTy, Error>,
{
    let mut results = Vec::new();
    let mut size = 1usize;
    for (index, t) in types.iter().enumerate() {
        let result = match f(t, state) {
            Ok(result) => result,
            Err(error) if error.kind == ErrorKind::TypeSizeLimit => {
                results.push(Arc::new(ty::Ty::Truncated));
                break;
            }
            Err(error) => return Err(error),
        };
        if let Some(max_type_size) = max_type_size {
            let has_more = index + 1 < types.len();
            let reserved_size = size.saturating_add(usize::from(has_more));
            let remaining = max_type_size.saturating_sub(reserved_size);
            let Some(member_size) = size_of_type(Some(remaining), &result) else {
                results.push(Arc::new(ty::Ty::Truncated));
                break;
            };
            size += member_size;
        }
        results.push(result);
    }
    Ok(results)
}

fn app_intersection<F>(
    f: F,
    types: Vec<Type>,
    state: &mut State,
    max_type_size: Option<usize>,
) -> Result<ALocTy, Error>
where
    F: FnMut(&Type, &mut State) -> Result<ALocTy, Error>,
{
    if types.is_empty() {
        return Ok(Arc::new(ty::Ty::Top));
    }
    let results = normalize_type_members(f, types, state, max_type_size)?;
    match ty::mk_inter(results) {
        Some(arc_ty) => Ok(arc_ty),
        None => Ok(Arc::new(ty::Ty::Top)),
    }
}

fn app_union<F>(
    from_bounds: bool,
    f: F,
    types: Vec<Type>,
    state: &mut State,
    max_type_size: Option<usize>,
) -> Result<ALocTy, Error>
where
    F: FnMut(&Type, &mut State) -> Result<ALocTy, Error>,
{
    if types.is_empty() {
        return Ok(Arc::new(ty::Ty::Bot(BotKind::EmptyType)));
    }
    let results = normalize_type_members(f, types, state, max_type_size)?;
    match ty::mk_union(from_bounds, results) {
        Some(arc_ty) => Ok(arc_ty),
        None => Ok(Arc::new(ty::Ty::Bot(BotKind::EmptyType))),
    }
}

fn should_eval_skip_aliases<'cx>(env: &mut Env<'_, 'cx>) -> bool {
    match env.evaluate_type_destructors() {
        EvaluateTypeDestructorsMode::EvaluateNone => false,
        EvaluateTypeDestructorsMode::EvaluateSome => false,
        EvaluateTypeDestructorsMode::EvaluateCustom(_) => false,
        EvaluateTypeDestructorsMode::EvaluateForHover => false,
        EvaluateTypeDestructorsMode::EvaluateAll => true,
    }
}

// Sometimes, we need to inspect Type.t so that we can avoid doing the work of
// printing giant types repeatedly. See should_force_eval_to_avoid_giant_types below.
fn unwrap_unless_aliased<'cx>(env: &mut Env<'_, 'cx>, t: &Type) -> Option<Type> {
    match t.deref() {
        TypeInner::OpenT(tvar) => flow_typing_flow_common::flow_js_utils::merge_tvar_opt(
            env.cx,
            false,
            flow_typing_type::type_::union_rep::UnionKind::ResolvedKind,
            tvar.reason(),
            tvar.id() as i32,
        )
        .and_then(|t| unwrap_unless_aliased(env, &t)),
        TypeInner::EvalT { id, .. } if should_eval_skip_aliases(env) => env
            .cx
            .evaluated()
            .get(id)
            .and_then(|t| unwrap_unless_aliased(env, t)),
        _ => {
            // Type aliases won't be expanded, so we don't waste time printing giant types.
            // As a result, we don't have to unwrap them
            let reason = flow_typing_type::type_util::reason_of_t(t);
            match reason.desc(false) {
                Some(flow_common::reason::VirtualReasonDesc::RTypeAlias(box (_, Some(_), _))) => {
                    None
                }
                _ => Some(t.dupe()),
            }
        }
    }
}

// Without the heuristic, given
//
// ```
// declare opaque type Id<T>;
// type Obj = $ReadOnly<{ ...giant object... }>
// type IDMap<+Obj: {+[string]: mixed}> = {[Key in keyof Obj]: Id<Obj[Key]>}
// declare const props: { obj: $ReadOnly<IDMap<Obj>> };
// props.obj;
// ```
//
// we will print something like
//
// ```
// {
//   field1: Id<{...full giant type...}['field1'],
//   field2: Id<{...full giant type...}['field2']>,
//  ...
// }
//  ```
//
// This heuristic prevents that by detecting we have an indexed access EvalT on objects,
// and then we force the evaluation so we will not hit the potentially expensive unevaluated case.
fn should_force_eval_to_avoid_giant_types<'cx>(
    env: &mut Env<'_, 'cx>,
    t: &Type,
    d: &Destructor,
) -> bool {
    use flow_typing_type::type_::Destructor as D;
    match d {
        D::PropertyType { .. }
        | D::OptionalIndexedAccessNonMaybeType { .. }
        | D::OptionalIndexedAccessResultType { .. } => match unwrap_unless_aliased(env, t) {
            Some(t) => {
                matches!(t.deref(), TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::ObjT(_)))
            }
            None => false,
        },
        D::ElementType { index_type }
            if matches!(
                index_type.deref(),
                TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::SingletonStrT { .. } | DefTInner::StrGeneralT(_))
            ) =>
        {
            match unwrap_unless_aliased(env, t) {
                Some(t) => {
                    matches!(t.deref(), TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::ObjT(_)))
                }
                None => false,
            }
        }
        D::NormalizeIntersection { .. }
        | D::NonMaybeType
        | D::ElementType { .. }
        | D::ExactType
        | D::ReadOnlyType
        | D::PartialType
        | D::RequiredType
        | D::SpreadType(..)
        | D::SpreadTupleType(..)
        | D::RestType(..)
        | D::ValuesType
        | D::ConditionalType(..)
        | D::TypeMap(..)
        | D::ReactElementConfigType
        | D::ReactCheckComponentConfig { .. }
        | D::ReactDRO(..)
        | D::MappedType(box DestructorMappedTypeData { .. })
        | D::EnumType => false,
    }
}

/// Whether the operand still mentions type parameters, in the same sense that
/// [`type_subst`] means it: a destructor over one of these cannot reduce to
/// anything the reader has not already been shown, so evaluating it is pure cost.
///
/// [`type_subst`]: flow_typing_flow_common::type_subst
fn operand_is_generic<'cx>(env: &mut Env<'_, 'cx>, t: &Type) -> bool {
    !flow_typing_flow_common::type_subst::free_var_finder(env.cx, None, t).is_empty()
}

/// The policy behind [`EvaluateTypeDestructorsMode::EvaluateForHover`].
///
/// A hover shows one form, and this picks it while normalizing rather than by
/// comparing two finished normalizations. Reducing earns its place only when it
/// turns a question into an answer -- `AdAccount['account_id']` into
/// `AdAccountID`. For the operators that restate a property set it does not:
/// `Readonly<Props>` says more than the properties it expands to, and shorter.
fn should_evaluate_for_hover<'cx>(env: &mut Env<'_, 'cx>, t: &Type, d: &Destructor) -> bool {
    use flow_typing_type::type_::Destructor as D;

    match d {
        // An indexed access reduces only if the key is known too: `M[K]` for a
        // type parameter `K` has no answer beyond the union of every value type,
        // which is longer than the question and says less.
        D::ElementType { index_type } => {
            !operand_is_generic(env, t) && !operand_is_generic(env, index_type)
        }
        D::OptionalIndexedAccessNonMaybeType { index } => {
            !operand_is_generic(env, t)
                && match index {
                    OptionalIndexedAccessIndex::OptionalIndexedAccessStrLitIndex(_) => true,
                    OptionalIndexedAccessIndex::OptionalIndexedAccessTypeIndex(index_type) => {
                        !operand_is_generic(env, index_type)
                    }
                }
        }
        // The answer is one property or one branch, so its size is unrelated to
        // the operand's.
        D::PropertyType { .. }
        | D::OptionalIndexedAccessResultType { .. }
        | D::NonMaybeType
        | D::ConditionalType(..) => !operand_is_generic(env, t),
        D::NormalizeIntersection { .. }
        | D::ValuesType
        | D::MappedType(box DestructorMappedTypeData { .. }) => !operand_is_generic(env, t),
        // The written spelling is the one a reader wants, whatever the operand
        // looks like.
        D::ReadOnlyType
        | D::PartialType
        | D::RequiredType
        | D::RestType(..)
        | D::ExactType
        | D::SpreadType(..)
        | D::SpreadTupleType(..)
        | D::TypeMap(..)
        | D::EnumType
        | D::ReactCheckComponentConfig { .. }
        | D::ReactDRO(..)
        | D::ReactElementConfigType => false,
    }
}

fn should_evaluate_destructor<'cx>(
    env: &mut Env<'_, 'cx>,
    force_eval: bool,
    t: &Type,
    d: &Destructor,
) -> bool {
    use flow_typing_type::type_::Destructor as D;

    force_eval
        || should_force_eval_to_avoid_giant_types(env, t, d)
        || matches!(
            d,
            // If we print out $Omit<Foo, 'bar'> unevaluated, it will end up with something like
            // Omit<Foo, {[K in 'bar']: mixed}>, which is our implementation detail and makes no sense
            // to users. Therefore, let's always evaluate.
            D::RestType(flow_typing_type::type_::object::rest::MergeMode::Omit, _)
        )
        || match env.evaluate_type_destructors() {
            EvaluateTypeDestructorsMode::EvaluateNone => false,
            EvaluateTypeDestructorsMode::EvaluateSome => match d {
                D::NormalizeIntersection { .. }
                | D::MappedType(box DestructorMappedTypeData { .. })
                | D::NonMaybeType
                | D::PropertyType { .. }
                | D::ElementType { .. }
                | D::OptionalIndexedAccessNonMaybeType { .. }
                | D::OptionalIndexedAccessResultType { .. }
                | D::ConditionalType(..) => true,
                D::ExactType
                | D::ReadOnlyType
                | D::ReactDRO(..)
                | D::PartialType
                | D::RequiredType
                | D::SpreadType(..)
                | D::SpreadTupleType(..)
                | D::RestType(..)
                | D::ReactCheckComponentConfig { .. }
                | D::ValuesType
                | D::TypeMap(..)
                | D::EnumType
                | D::ReactElementConfigType => false,
            },
            EvaluateTypeDestructorsMode::EvaluateForHover => should_evaluate_for_hover(env, t, d),
            EvaluateTypeDestructorsMode::EvaluateCustom(f) => f(d),
            EvaluateTypeDestructorsMode::EvaluateAll => match d {
                D::ReactDRO(..) => false,
                _ => true,
            },
        }
}

fn eval_t<'cx, I: NormalizerInput>(
    env: &mut Env<'_, 'cx>,
    state: &mut State,
    mut cont: impl FnMut(&mut Env<'_, 'cx>, &mut State, Option<IdKey>, &Type) -> Result<ALocTy, Error>,
    mut default: impl FnMut(
        &mut Env<'_, 'cx>,
        &mut State,
        Option<IdKey>,
        &Type,
    ) -> Result<ALocTy, Error>,
    non_eval: fn(&mut Env<'_, 'cx>, &mut State, &Type, &Destructor) -> Result<ALocTy, Error>,
    force_eval: bool,
    x: (
        &Type,
        &flow_typing_type::type_::TypeDestructorT,
        &type_eval::Id,
    ),
) -> Result<ALocTy, Error> {
    let (t, defer_use_t, id) = x;
    let flow_typing_type::type_::TypeDestructorTInner(_, _, d) = defer_use_t.deref();
    state.found_computed_type = true;
    let should_eval = should_evaluate_destructor(env, force_eval, t, d);
    let eval_id = id.dupe();
    I::eval(
        env,
        state,
        should_eval,
        |env, state, t| cont(env, state, Some(IdKey::EvalKey(eval_id)), t),
        |env, state, t| default(env, state, None, t),
        non_eval,
        x,
    )
}

fn type_variable<'cx>(
    env: &mut Env<'_, 'cx>,
    state: &mut State,
    cont: &mut dyn FnMut(
        &mut Env<'_, 'cx>,
        &mut State,
        Option<IdKey>,
        &Type,
    ) -> Result<ALocTy, Error>,
    id: i32,
) -> Result<ALocTy, Error> {
    use flow_typing_type::type_::UseTInner;
    use flow_typing_type::type_::constraint::Bounds;
    use flow_typing_type::type_::constraint::Constraints;
    use flow_typing_type::type_::string_of_use_ctor;

    fn uses_t<'cx>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        cont: &mut dyn FnMut(
            &mut Env<'_, 'cx>,
            &mut State,
            Option<IdKey>,
            &Type,
        ) -> Result<ALocTy, Error>,
        id: i32,
        uses: &[flow_typing_type::type_::UseT<Cx>],
    ) -> Result<UpperBoundKind<ALoc>, Error> {
        let mut acc = Vec::new();
        let mut uses = VecDeque::from(uses.to_vec());
        loop {
            if uses.is_empty() {
                acc.retain(|t: &ALocTy| {
                    !matches!(
                        t.as_ref(),
                        ty::Ty::Bot(BotKind::NoLowerWithUpper(UpperBoundKind::NoUpper))
                    )
                });
                break if acc.is_empty() {
                    Ok(UpperBoundKind::NoUpper)
                } else {
                    match ty::mk_inter(acc) {
                        Some(inter) => Ok(UpperBoundKind::SomeKnownUpper(inter)),
                        None => Ok(UpperBoundKind::NoUpper),
                    }
                };
            }
            match uses[0].deref() {
                UseTInner::UseT(_, t) => {
                    let ty = cont(env, state, Some(IdKey::TVarKey(id)), t)?;
                    acc.push(ty);
                    uses.pop_front();
                }
                UseTInner::ReposLowerT { use_t, .. } => {
                    let u = use_t.deref().dupe();
                    uses[0] = u;
                }
                // skip these
                _ => {
                    break Ok(UpperBoundKind::SomeUnknownUpper(string_of_use_ctor(
                        &uses[0],
                    )));
                }
            }
        }
    }

    fn empty_with_upper_bounds<'cx>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        cont: &mut dyn FnMut(
            &mut Env<'_, 'cx>,
            &mut State,
            Option<IdKey>,
            &Type,
        ) -> Result<ALocTy, Error>,
        id: i32,
        bounds: &Bounds<Cx>,
    ) -> Result<ALocTy, Error> {
        let uses: Vec<_> = bounds.upper.keys().map(|k| k.use_t.dupe()).collect();
        let use_kind = uses_t(env, state, cont, id, &uses)?;
        Ok(Arc::new(ty::Ty::Bot(BotKind::NoLowerWithUpper(use_kind))))
    }

    fn resolve_from_lower_bounds<'cx>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        cont: &mut dyn FnMut(
            &mut Env<'_, 'cx>,
            &mut State,
            Option<IdKey>,
            &Type,
        ) -> Result<ALocTy, Error>,
        id: i32,
        bounds: &Bounds<Cx>,
    ) -> Result<Vec<ALocTy>, Error> {
        let mut result = Vec::new();
        for t in bounds.lower.keys() {
            let ty_result = cont(env, state, Some(IdKey::TVarKey(id)), t)?;
            result.extend(ty::bk_union(&ty_result));
        }
        result.sort();
        result.dedup();
        Ok(result)
    }

    let (_, constraints) = env.cx.find_constraints(id);
    match constraints {
        Constraints::Resolved(t) => cont(env, state, Some(IdKey::TVarKey(id)), &t),
        Constraints::FullyResolved(s) => {
            let t = env.cx.force_fully_resolved_tvar(&s);
            cont(env, state, Some(IdKey::TVarKey(id)), &t)
        }
        Constraints::Unresolved(bounds) => {
            let bounds = bounds.borrow();
            let lower_bounds = resolve_from_lower_bounds(env, state, cont, id, &bounds)?;
            if lower_bounds.is_empty() {
                empty_with_upper_bounds(env, state, cont, id, &bounds)
            } else {
                match ty::mk_union_with_flattened(true, true, lower_bounds) {
                    Some(union) => Ok(union),
                    None => empty_with_upper_bounds(env, state, cont, id, &bounds),
                }
            }
        }
    }
}

fn maybe_t<'cx>(
    cont: impl FnOnce(&mut Env<'_, 'cx>, &mut State, Option<IdKey>, &Type) -> Result<ALocTy, Error>,
    env: &mut Env<'_, 'cx>,
    state: &mut State,
    id: Option<IdKey>,
    t: &Type,
) -> Result<ALocTy, Error> {
    let t = cont(env, state, id, t)?;
    Ok(ty::mk_union(
        false,
        vec![Arc::new(ty::Ty::Void), Arc::new(ty::Ty::Null), t],
    )
    .unwrap())
}

fn optional_t<'cx>(
    cont: impl FnOnce(&mut Env<'_, 'cx>, &mut State, Option<IdKey>, &Type) -> Result<ALocTy, Error>,
    env: &mut Env<'_, 'cx>,
    state: &mut State,
    id: Option<IdKey>,
    t: &Type,
) -> Result<ALocTy, Error> {
    let t = cont(env, state, id, t)?;
    Ok(ty::mk_union(false, vec![Arc::new(ty::Ty::Void), t]).unwrap())
}

fn keys_t<'cx, I: NormalizerInput>(
    cont: fn(&mut Env<'_, 'cx>, &mut State, Option<IdKey>, &Type) -> Result<ALocTy, Error>,
    env: &mut Env<'_, 'cx>,
    state: &mut State,
    force_eval: bool,
    reason: &Reason,
    t: &Type,
) -> Result<ALocTy, Error> {
    state.found_computed_type = true;
    let should_evaluate = force_eval
        || match env.evaluate_type_destructors() {
            EvaluateTypeDestructorsMode::EvaluateNone => false,
            EvaluateTypeDestructorsMode::EvaluateForHover => !operand_is_generic(env, t),
            EvaluateTypeDestructorsMode::EvaluateAll
            | EvaluateTypeDestructorsMode::EvaluateSome
            | EvaluateTypeDestructorsMode::EvaluateCustom(_) => true,
        };
    I::keys(
        env,
        state,
        should_evaluate,
        |env, state, resolved_t| cont(env, state, None, &resolved_t),
        |env, state| {
            let ty = cont(env, state, None, t)?;
            Ok(Arc::new(ty::Ty::Utility(ty::Utility::Keys(ty))))
        },
        reason.dupe(),
        t.dupe(),
    )
}

mod reason_utils {
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;

    use super::*;

    // The local type alias name extractable from [reason], or [None] when the
    // reason is not alias-shaped (e.g. a synthetic [TypeT] wrapping a TS enum
    // member literal, whose reason is the literal's own reason).
    pub(super) fn local_type_alias_symbol_opt<'cx>(
        env: &mut Env<'_, 'cx>,
        reason: &Reason,
    ) -> Result<Option<ALocSymbol>, Error> {
        match reason.desc(false) {
            Some(ReasonDesc::REnum { name: Some(name) }) => {
                Ok(Some(symbol_from_reason(env, reason, name.dupe())?))
            }
            Some(ReasonDesc::RTypeAlias(box (name, Some(loc), _))) => {
                Ok(Some(symbol_from_loc(env, loc.dupe(), name.dupe())?))
            }
            Some(ReasonDesc::RType(name)) => {
                Ok(Some(symbol_from_reason(env, reason, name.dupe())?))
            }
            _ => Ok(None),
        }
    }

    pub(super) fn local_type_alias_symbol<'cx>(
        env: &mut Env<'_, 'cx>,
        reason: &Reason,
    ) -> Result<ALocSymbol, Error> {
        match local_type_alias_symbol_opt(env, reason)? {
            Some(symbol) => Ok(symbol),
            None => Err(terr(
                ErrorKind::BadTypeAlias,
                Some("could not extract local type alias name from reason"),
                None,
            )),
        }
    }

    pub fn imported_type_alias_symbol<'cx>(
        env: &mut Env<'_, 'cx>,
        reason: &Reason,
    ) -> Result<ALocSymbol, Error> {
        match reason.desc(false) {
            Some(
                ReasonDesc::RNamedImportedType(_, name)
                | ReasonDesc::RDefaultImportedType(name, _)
                | ReasonDesc::RImportStarType(name)
                | ReasonDesc::RImportStarTypeOf(name)
                | ReasonDesc::RImportStar(name),
            ) => symbol_from_reason(env, reason, name.dupe()),
            Some(ReasonDesc::RType(name)) => symbol_from_reason(env, reason, name.dupe()),
            _ => Err(terr(
                ErrorKind::BadTypeAlias,
                Some("could not extract imported type alias name from reason"),
                None,
            )),
        }
    }

    pub fn opaque_type_alias_symbol<'cx>(
        env: &mut Env<'_, 'cx>,
        reason: &Reason,
    ) -> Result<ALocSymbol, Error> {
        match reason.desc(false) {
            Some(ReasonDesc::ROpaqueType(name)) => symbol_from_reason(env, reason, name.dupe()),
            Some(ReasonDesc::RType(name)) => symbol_from_reason(env, reason, name.dupe()),
            _ => Err(terr(
                ErrorKind::BadTypeAlias,
                Some("could not extract opaque name from reason"),
                None,
            )),
        }
    }

    pub fn instance_symbol<'cx>(
        env: &mut Env<'_, 'cx>,
        reason: &Reason,
        inst: &flow_typing_type::type_::InstType,
    ) -> Result<ALocSymbol, Error> {
        if matches!(reason.desc(true), Some(ReasonDesc::RThisType)) {
            return symbol_from_reason(env, reason, FlowSmolStr::new_inline("this"));
        }
        if let Some(name) = &inst.class_name {
            return symbol_from_loc(env, inst.class_id.0.dupe(), name.dupe());
        }
        match reason.desc(true) {
            Some(ReasonDesc::RType(name) | ReasonDesc::RIdentifier(name)) => {
                symbol_from_reason(env, reason, name.dupe())
            }
            _ => Err(terr(
                ErrorKind::BadInstanceT,
                Some("could not extract instance name from reason"),
                None,
            )),
        }
    }

    pub fn component_symbol<'cx>(
        env: &mut Env<'_, 'cx>,
        name: &FlowSmolStr,
        reason: &Reason,
    ) -> Result<ALocSymbol, Error> {
        symbol_from_reason(env, reason, name.dupe())
    }

    pub fn module_symbol_opt<'cx>(
        env: &mut Env<'_, 'cx>,
        reason: &Reason,
    ) -> Result<Option<ALocSymbol>, Error> {
        match reason.desc(true) {
            Some(ReasonDesc::RModule(name)) => Ok(Some(symbol_from_reason(
                env,
                reason,
                FlowSmolStr::new(name.display()),
            )?)),
            Some(ReasonDesc::RExports) => Ok(None),
            _ => Err(terr(
                ErrorKind::UnsupportedTypeCtor,
                Some("could not extract module name from reason"),
                None,
            )),
        }
    }
}

mod type_converter {
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;

    use super::*;

    fn type_debug<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        id: Option<IdKey>,
        depth: u32,
        t: &Type,
    ) -> Result<ALocTy, Error> {
        let prefix = format!(
            "{:width$}[Norm|run_id:{}|depth:{}]",
            "",
            get_run_id(),
            depth,
            width = (2 * depth) as usize
        );
        eprintln!(
            "{} Input: {}\n",
            prefix,
            flow_typing_debug::dump_t(None, env.cx, t)
        );
        let result = type_with_alias_reason::<I>(env, state, id, t);
        let result_str = match &result {
            Ok(ty) => format!(
                "[Ok] {}",
                flow_common_ty::ty_debug::dump_t_EXPOSES_ABSTRACT_LOCS(ty.as_ref())
            ),
            Err(e) => format!("[Error] {}", e),
        };
        eprintln!("{} Output: {}\n", prefix, result_str);
        result
    }

    pub(super) fn type__<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        id: Option<IdKey>,
        t: &Type,
    ) -> Result<ALocTy, Error> {
        state.consume_type_node()?;
        descend(env, t)?;
        let depth = env.depth - 1;
        let result = if env.verbose() {
            type_debug::<I>(env, state, id, depth, t)
        } else {
            type_with_alias_reason::<I>(env, state, id, t)
        };
        env.depth -= 1;
        result
    }

    fn type_with_alias_reason<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        id: Option<IdKey>,
        t: &Type,
    ) -> Result<ALocTy, Error> {
        match t.deref() {
            TypeInner::OpenT(_) | TypeInner::ImplicitInstantiationTvar(_) => {
                type_ctor::<I>(env, state, id, type_with_alias_reason::<I>, t)
            }
            TypeInner::EvalT { .. } if should_eval_skip_aliases(env) => {
                type_ctor::<I>(env, state, id, type_with_alias_reason::<I>, t)
            }
            TypeInner::NamespaceT(ns) if env.keep_only_namespace_name => {
                let sym_def_loc = ns.namespace_symbol.def_loc_of_symbol().clone();
                let sym_name = FlowSmolStr::new(ns.namespace_symbol.name());
                let symbol = symbol_from_loc(env, sym_def_loc, sym_name)?;
                Ok(Arc::new(ty::Ty::TypeOf(Box::new((
                    ty::BuiltinOrSymbol::TSymbol(symbol),
                    None,
                )))))
            }
            _ => {
                let reason = flow_typing_type::type_util::reason_of_t(t);
                match reason.desc(false) {
                    Some(flow_common::reason::VirtualReasonDesc::RTypeAlias(box (
                        name,
                        Some(loc),
                        _,
                    ))) => {
                        // The default action is to avoid expansion by using the type alias name,
                        // when this can be trusted. The one case where we want to skip this process
                        // is when recovering the body of a type alias A. In that case the environment
                        // field under_type_alias will be `Some(A)`. If the type alias name in the reason
                        // is also A, then we are still at the top-level of the type-alias, so we
                        // proceed by expanding one level preserving the same environment.
                        let symbol = symbol_from_loc(env, loc.clone(), name.dupe())?;
                        // Optionally collect the body type for ref expansion
                        if let Some(tbl) = &env.genv.ref_type_bodies {
                            let mut tbl = tbl.borrow_mut();
                            let key = name.to_string();
                            tbl.entry(key).or_insert_with(|| t.dupe());
                        }
                        Ok(generic_talias(symbol, None))
                    }
                    // We are now beyond the point of the one-off expansion. Reset the environment
                    // assigning None to under_type_alias, so that aliases are used in subsequent
                    // invocations.
                    _ => type_ctor::<I>(env, state, id, type_with_alias_reason::<I>, t),
                }
            }
        }
    }

    fn constraint_node<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        id: Option<IdKey>,
        node_id: i32,
    ) -> Result<ALocTy, Error> {
        let (root_id, _) = env.cx.find_constraints(node_id);
        if id == Some(IdKey::TVarKey(root_id)) {
            return Ok(Arc::new(ty::Ty::Bot(BotKind::NoLowerWithUpper(
                UpperBoundKind::NoUpper,
            ))));
        }
        if is_rec_id(state, IdKey::TVarKey(root_id)) {
            return Ok(Arc::new(ty::Ty::Any(ty::AnyKind::Recursive)));
        }
        if env.seen_tvar_ids.contains(&root_id) {
            add_rec_id(state, IdKey::TVarKey(root_id));
            return Ok(Arc::new(ty::Ty::Any(ty::AnyKind::Recursive)));
        }
        let old_seen_tvar_ids = env.seen_tvar_ids.dupe();
        env.seen_tvar_ids.insert(root_id);
        let result = type_variable(env, state, &mut type_converter::type__::<I>, root_id);
        env.seen_tvar_ids = old_seen_tvar_ids;
        result
    }

    fn type_ctor<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        id: Option<IdKey>,
        cont: fn(&mut Env<'_, 'cx>, &mut State, Option<IdKey>, &Type) -> Result<ALocTy, Error>,
        t: &Type,
    ) -> Result<ALocTy, Error> {
        match t.deref() {
            TypeInner::OpenT(tvar) => constraint_node::<I>(env, state, id, tvar.id() as i32),
            TypeInner::ImplicitInstantiationTvar(data) => {
                constraint_node::<I>(env, state, id, data.id)
            }
            TypeInner::GenericT(generic) => generic_t::<I>(env, state, generic, t),
            TypeInner::AnnotT(_, inner_t, _) => type__::<I>(env, state, id, inner_t),
            TypeInner::AnyT(reason, kind) => Ok(Arc::new(ty::Ty::Any(any_t(reason, kind)))),
            TypeInner::EvalT {
                type_,
                defer_use_t,
                id: eval_id,
            } => {
                if id == Some(IdKey::EvalKey(eval_id.dupe())) {
                    return Ok(Arc::new(ty::Ty::Bot(BotKind::NoLowerWithUpper(
                        UpperBoundKind::NoUpper,
                    ))));
                }
                if is_rec_id(state, IdKey::EvalKey(eval_id.dupe())) {
                    return Ok(Arc::new(ty::Ty::Any(ty::AnyKind::Recursive)));
                }
                if env.seen_eval_ids.contains(eval_id) {
                    add_rec_id(state, IdKey::EvalKey(eval_id.dupe()));
                    return Ok(Arc::new(ty::Ty::Any(ty::AnyKind::Recursive)));
                }
                let old_seen_eval_ids = env.seen_eval_ids.dupe();
                env.seen_eval_ids.insert(eval_id.dupe());
                let result = eval_t::<I>(
                    env,
                    state,
                    cont,
                    type_converter::type__::<I>,
                    type_converter::type_destructor_unevaluated::<I>,
                    false,
                    (type_, defer_use_t, eval_id),
                );
                env.seen_eval_ids = old_seen_eval_ids;
                result
            }
            TypeInner::NamespaceT(ns) => {
                let old = env.keep_only_namespace_name;
                env.keep_only_namespace_name = true;
                let result = cont(env, state, id, &ns.values_type);
                env.keep_only_namespace_name = old;
                result
            }
            TypeInner::DefT(reason, def_t) => match def_t.deref() {
                DefTInner::MixedT(_) => Ok(Arc::new(ty::Ty::Top)),
                DefTInner::VoidT => Ok(Arc::new(ty::Ty::Void)),
                DefTInner::NullT => Ok(Arc::new(ty::Ty::Null)),
                DefTInner::NumGeneralT(_) => Ok(Arc::new(ty::Ty::Num)),
                DefTInner::StrGeneralT(_) => Ok(Arc::new(ty::Ty::Str)),
                DefTInner::BoolGeneralT => Ok(Arc::new(ty::Ty::Bool)),
                DefTInner::BigIntGeneralT(_) => Ok(Arc::new(ty::Ty::BigInt)),
                DefTInner::SymbolT => Ok(Arc::new(ty::Ty::Symbol)),
                DefTInner::UniqueSymbolT(_) => Ok(Arc::new(ty::Ty::UniqueSymbol)),
                DefTInner::EmptyT => Ok(mk_empty(BotKind::EmptyType)),
                DefTInner::NumericStrKeyT(num_lit) => {
                    Ok(Arc::new(ty::Ty::StrLit(num_lit.1.dupe())))
                }
                DefTInner::SingletonNumT { value, .. } => {
                    Ok(Arc::new(ty::Ty::NumLit(value.1.to_string())))
                }
                DefTInner::SingletonStrT { value, .. } => {
                    Ok(Arc::new(ty::Ty::StrLit(value.dupe())))
                }
                DefTInner::SingletonBoolT { value, .. } => Ok(Arc::new(ty::Ty::BoolLit(*value))),
                DefTInner::SingletonBigIntT { value, .. } => {
                    Ok(Arc::new(ty::Ty::BigIntLit(value.1.to_string())))
                }
                DefTInner::FunT(static_, f) => {
                    let fun_t = fun_ty::<I>(env, state, static_, f, None)?;
                    Ok(Arc::new(ty::Ty::Fun(Box::new(fun_t))))
                }
                DefTInner::ObjT(o) => {
                    let obj =
                        obj_ty::<I>(env, state, reason, o, false, ty::PropSource::Other, None)?;
                    Ok(Arc::new(ty::Ty::Obj(Box::new(obj))))
                }
                DefTInner::ArrT(a) => arr_ty::<I>(env, state, a),
                DefTInner::PolyT(box PolyTData { tparams, t_out, .. }) => {
                    poly_ty::<I>(env, state, t_out, tparams)
                }
                DefTInner::InstanceT(instance_t_val) => instance_t::<I>(
                    env,
                    state,
                    reason,
                    &instance_t_val.super_,
                    &instance_t_val.inst,
                ),
                DefTInner::ClassT(class_t) => match class_t.deref() {
                    TypeInner::ThisInstanceT(box ThisInstanceTData {
                        reason: r,
                        instance: inst_t,
                        ..
                    }) => this_class_t(env, r, inst_t),
                    _ => {
                        let ty = type__::<I>(env, state, id, class_t)?;
                        Ok(Arc::new(ty::Ty::Utility(ty::Utility::Class(ty))))
                    }
                },
                DefTInner::ReactAbstractComponentT(box ReactAbstractComponentTData {
                    component_kind:
                        flow_typing_type::type_::ComponentKind::Nominal(_, name, inferred_targs),
                    ..
                }) => {
                    let targs = match inferred_targs {
                        None => None,
                        Some(targs) => {
                            let mut result = Vec::new();
                            for t in targs.iter() {
                                result.push(type__::<I>(env, state, None, t)?);
                            }
                            Some(result)
                        }
                    };
                    let symbol = reason_utils::component_symbol(env, name, reason)?;
                    Ok(Arc::new(ty::Ty::TypeOf(Box::new((
                        ty::BuiltinOrSymbol::TSymbol(symbol),
                        targs.map(Into::into),
                    )))))
                }
                DefTInner::ReactAbstractComponentT(box ReactAbstractComponentTData {
                    config,
                    renders,
                    ..
                }) => {
                    let (regular_props, renders_ty) =
                        convert_component::<I>(env, state, config, renders, false)?;
                    Ok(Arc::new(ty::Ty::Component {
                        regular_props,
                        renders: renders_ty,
                    }))
                }
                DefTInner::RendersT(renders) => {
                    use flow_typing_type::type_::CanonicalRendersForm;
                    use flow_typing_type::type_::RendersVariant as TRendersVariant;
                    match renders.deref() {
                        CanonicalRendersForm::IntrinsicRenders(n) => {
                            Ok(Arc::new(ty::Ty::StrLit(n.dupe())))
                        }
                        CanonicalRendersForm::NominalRenders {
                            renders_id,
                            renders_name,
                            ..
                        } => {
                            let renders_reason = flow_common::reason::mk_reason(
                                ReasonDesc::RComponent(renders_name.dupe()),
                                renders_id.0.dupe(),
                            );
                            let symbol =
                                reason_utils::component_symbol(env, renders_name, &renders_reason)?;
                            Ok(Arc::new(ty::Ty::Generic(Box::new((
                                symbol,
                                ty::GenKind::ComponentKind,
                                None,
                            )))))
                        }
                        CanonicalRendersForm::StructuralRenders {
                            renders_variant,
                            renders_structural_type,
                        } => {
                            let ty =
                                type_ctor::<I>(env, state, None, cont, renders_structural_type)?;
                            let variant = match renders_variant {
                                TRendersVariant::RendersNormal => ty::RendersKind::RendersNormal,
                                TRendersVariant::RendersMaybe => ty::RendersKind::RendersMaybe,
                                TRendersVariant::RendersStar => ty::RendersKind::RendersStar,
                            };
                            Ok(Arc::new(ty::Ty::Renders(ty, variant)))
                        }
                        CanonicalRendersForm::DefaultRenders => {
                            let symbol = reason_utils::component_symbol(
                                env,
                                &FlowSmolStr::new("React.Node"),
                                reason,
                            )?;
                            let ty = ty::Ty::Generic(Box::new((
                                symbol,
                                ty::GenKind::ComponentKind,
                                None,
                            )));
                            Ok(Arc::new(ty::Ty::Renders(
                                Arc::new(ty),
                                ty::RendersKind::RendersNormal,
                            )))
                        }
                    }
                }
                DefTInner::EnumObjectT {
                    enum_value_t,
                    enum_info,
                } if matches!(
                    enum_info.deref().deref(),
                    EnumInfoInner::AbstractEnum { .. }
                ) =>
                {
                    builtin_application::<I>(
                        env,
                        state,
                        reason,
                        "Enum",
                        std::slice::from_ref(enum_value_t),
                    )
                }
                DefTInner::EnumObjectT { .. } => {
                    let symbol = reason_utils::local_type_alias_symbol(env, reason)?;
                    Ok(Arc::new(ty::Ty::TypeOf(Box::new((
                        ty::BuiltinOrSymbol::TSymbol(symbol),
                        None,
                    )))))
                }
                DefTInner::EnumValueT(enum_info)
                    if let EnumInfoInner::AbstractEnum { representation_t } =
                        enum_info.deref().deref() =>
                {
                    builtin_application::<I>(
                        env,
                        state,
                        reason,
                        "EnumValue",
                        std::slice::from_ref(representation_t),
                    )
                }
                DefTInner::EnumValueT(_) => {
                    let symbol = reason_utils::local_type_alias_symbol(env, reason)?;
                    Ok(Arc::new(ty::Ty::Generic(Box::new((
                        symbol,
                        ty::GenKind::EnumKind,
                        None,
                    )))))
                }
                DefTInner::TypeT(..) => Err(terr(
                    ErrorKind::UnexpectedTypeCtor(flow_typing_type::type_::string_of_ctor(t)),
                    None,
                    Some(t),
                )),
            },
            TypeInner::TemplateLiteralT {
                reason: _,
                from_annot: _,
                quasis,
                types,
            } => {
                let norm_types: Result<Vec<_>, _> = types
                    .iter()
                    .map(|t| type__::<I>(env, state, None, t))
                    .collect();
                Ok(Arc::new(ty::Ty::TemplateLiteral {
                    quasis: quasis.clone().into(),
                    types: norm_types?.into(),
                }))
            }
            TypeInner::StringMappingT {
                reason: _,
                kind,
                arg,
            } => {
                let arg = type__::<I>(env, state, None, arg)?;
                let utility = match kind {
                    flow_typing_type::type_::StringMappingKind::StringMappingUppercase => {
                        ty::Utility::Uppercase(arg)
                    }
                    flow_typing_type::type_::StringMappingKind::StringMappingLowercase => {
                        ty::Utility::Lowercase(arg)
                    }
                    flow_typing_type::type_::StringMappingKind::StringMappingCapitalize => {
                        ty::Utility::Capitalize(arg)
                    }
                    flow_typing_type::type_::StringMappingKind::StringMappingUncapitalize => {
                        ty::Utility::Uncapitalize(arg)
                    }
                };
                Ok(Arc::new(ty::Ty::Utility(utility)))
            }
            TypeInner::MaybeT(_, inner_t) => maybe_t(type__::<I>, env, state, id, inner_t),
            TypeInner::OptionalT { type_, .. } => optional_t(type__::<I>, env, state, id, type_),
            TypeInner::UnionT(_, rep) => {
                let types: Vec<Type> = rep.members_iter().map(|t| t.dupe()).collect();
                let max_type_size = env.max_type_size();
                app_union(
                    false,
                    |t, s| type__::<I>(env, s, id.clone(), t),
                    types,
                    state,
                    max_type_size,
                )
            }
            TypeInner::IntersectionT(_, rep) => {
                let types: Vec<Type> = rep.members_iter().map(|t| t.dupe()).collect();
                let max_type_size = env.max_type_size();
                app_intersection(
                    |t, s| type__::<I>(env, s, id.clone(), t),
                    types,
                    state,
                    max_type_size,
                )
            }
            TypeInner::TypeAppT(box TypeAppTData {
                type_,
                targs,
                from_value,
                ..
            }) => type_app::<I>(env, state, *from_value, type_, Some(&**targs)),
            TypeInner::ThisInstanceT(box ThisInstanceTData {
                reason,
                instance: inst_t,
                ..
            }) => instance_t::<I>(env, state, reason, &inst_t.super_, &inst_t.inst),
            TypeInner::ThisTypeAppT(box ThisTypeAppTData {
                type_: c,
                targs: ts,
                ..
            }) => type_app::<I>(env, state, false, c, ts.as_deref()),
            TypeInner::KeysT(reason, inner_t) => {
                keys_t::<I>(type__::<I>, env, state, false, reason, inner_t)
            }
            TypeInner::NominalT {
                reason,
                nominal_type,
            } => nominal_t::<I>(env, state, reason, nominal_type),
            TypeInner::ObjProtoT(_) => Ok(Arc::new(ty::Ty::TypeOf(Box::new((
                ty::BuiltinOrSymbol::ObjProto,
                None,
            ))))),
            TypeInner::FunProtoT(_) => Ok(Arc::new(ty::Ty::TypeOf(Box::new((
                ty::BuiltinOrSymbol::FunProto,
                None,
            ))))),
            TypeInner::NullProtoT(_) => Ok(Arc::new(ty::Ty::Null)),
        }
    }

    fn any_t(reason: &Reason, kind: &AnySource) -> ty::AnyKind<ALoc> {
        match kind {
            AnySource::AnnotatedAny | AnySource::CatchAny => {
                let aloc = reason.loc().clone();
                ty::AnyKind::Annotated(aloc)
            }
            AnySource::AnyError(k) => ty::AnyKind::AnyError(any_error_kind(k.as_ref())),
            AnySource::Unsound(k) => ty::AnyKind::Unsound(unsoundness_any_t(k)),
            AnySource::Untyped => ty::AnyKind::Untyped,
            AnySource::Placeholder => ty::AnyKind::Placeholder,
        }
    }

    fn any_error_kind(
        kind: Option<&flow_typing_type::type_::AnyErrorKind>,
    ) -> Option<ty::AnyErrorKind> {
        use flow_typing_type::type_::AnyErrorKind as TAnyErrorKind;
        match kind {
            Some(TAnyErrorKind::UnresolvedName) => Some(ty::AnyErrorKind::UnresolvedName),
            Some(TAnyErrorKind::MissingAnnotation) => Some(ty::AnyErrorKind::MissingAnnotation),
            None => None,
        }
    }

    fn unsoundness_any_t(k: &flow_typing_type::type_::UnsoundnessKind) -> ty::UnsoundnessKind {
        use flow_typing_type::type_::UnsoundnessKind as TUnsoundnessKind;
        match k {
            TUnsoundnessKind::BoundFunctionThis => ty::UnsoundnessKind::BoundFunctionThis,
            TUnsoundnessKind::Constructor => ty::UnsoundnessKind::Constructor,
            TUnsoundnessKind::DummyStatic => ty::UnsoundnessKind::DummyStatic,
            TUnsoundnessKind::Exports => ty::UnsoundnessKind::Exports,
            TUnsoundnessKind::InferenceHooks => ty::UnsoundnessKind::InferenceHooks,
            TUnsoundnessKind::InstanceOfRefinement => ty::UnsoundnessKind::InstanceOfRefinement,
            TUnsoundnessKind::Merged => ty::UnsoundnessKind::Merged,
            TUnsoundnessKind::ResolveSpread => ty::UnsoundnessKind::ResolveSpread,
            TUnsoundnessKind::Unchecked => ty::UnsoundnessKind::Unchecked,
            TUnsoundnessKind::Unimplemented => ty::UnsoundnessKind::Unimplemented,
            TUnsoundnessKind::UnresolvedType => ty::UnsoundnessKind::UnresolvedType,
            TUnsoundnessKind::NonBindingPattern => ty::UnsoundnessKind::NonBindingPattern,
        }
    }

    fn fun_ty<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        static_: &Type,
        f: &flow_typing_type::type_::FunType,
        fun_type_params: Option<Vec<ty::TypeParam<ALoc>>>,
    ) -> Result<ty::FunT<ALoc>, Error> {
        use flow_typing_type::type_::ReactEffectType;
        let fun_static = type__::<I>(env, state, None, static_)?;
        let fun_this_param = match type__::<I>(env, state, None, &f.this_t.0)? {
            t if matches!(t.as_ref(), ty::Ty::Top | ty::Ty::Any(_)) => None,
            t => Some(t),
        };
        let fun_effect = match &f.effect_ {
            ReactEffectType::HookAnnot | ReactEffectType::HookDecl(_) => ty::FunEffect::Hook,
            ReactEffectType::ArbitraryEffect | ReactEffectType::AnyEffect => {
                ty::FunEffect::Arbitrary
            }
        };
        let mut fun_params = Vec::new();
        for param in f.params.iter() {
            fun_params.push(fun_param::<I>(env, state, param)?);
        }
        let fun_rest_param = fun_rest_param_t::<I>(env, state, &f.rest_param)?;
        let fun_return = match &f.type_guard {
            Some(tg) => {
                let t = match &tg.type_guard {
                    Some(type_guard) => Some(type__::<I>(env, state, None, type_guard)?),
                    None => None,
                };
                ty::ReturnT::TypeGuard(tg.kind, tg.param_name.1.dupe(), t)
            }
            None => {
                let t = type__::<I>(env, state, None, &f.return_t)?;
                ty::ReturnT::ReturnType(t)
            }
        };
        Ok(ty::FunT {
            fun_this_param,
            fun_params: fun_params.into(),
            fun_rest_param,
            fun_return,
            fun_type_params: fun_type_params.map(Into::into),
            fun_static,
            fun_effect,
        })
    }

    fn method_ty<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        t: &Type,
    ) -> Result<Vec<ty::FunT<ALoc>>, Error> {
        fn go(ty: &ALocTy, t: &Type) -> Result<Vec<ty::FunT<ALoc>>, Error> {
            match ty.as_ref() {
                ty::Ty::Fun(ft) => Ok(vec![(**ft).clone()]),
                ty::Ty::Inter(t1, t2, ts) => {
                    let mut result = Vec::new();
                    result.extend(go(t1, t)?);
                    result.extend(go(t2, t)?);
                    for ti in ts.iter() {
                        result.extend(go(ti, t)?);
                    }
                    Ok(result)
                }
                _ => Err(terr(ErrorKind::BadMethodType, None, Some(t))),
            }
        }
        let ty = type__::<I>(env, state, None, t)?;
        go(&ty, t)
    }

    fn fun_param<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        param: &flow_typing_type::type_::FunParam,
    ) -> Result<(Option<FlowSmolStr>, ALocTy, ty::FunParam), Error> {
        let (t, prm_optional) = opt_t::<I>(env, state, &param.1)?;
        Ok((param.0.dupe(), t, ty::FunParam { prm_optional }))
    }

    fn fun_rest_param_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        rest_param: &Option<flow_typing_type::type_::FunRestParam>,
    ) -> Result<Option<(Option<FlowSmolStr>, ALocTy)>, Error> {
        match rest_param {
            Some(flow_typing_type::type_::FunRestParam(x, _, t)) => {
                let ty = type__::<I>(env, state, None, t)?;
                Ok(Some((x.dupe(), ty)))
            }
            None => Ok(None),
        }
    }

    fn obj_ty<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        reason: &Reason,
        o: &flow_typing_type::type_::ObjType,
        inherited: bool,
        source: ty::PropSource,
        allowed_prop_names: Option<&[Name]>,
    ) -> Result<ty::ObjT<ALoc>, Error> {
        let obj_def_loc = Some(reason.def_loc().dupe());
        let obj_props = obj_props_t::<I>(
            env,
            state,
            o.props_tmap.dupe(),
            o.call_t,
            inherited,
            source,
            allowed_prop_names,
        )?;
        let obj_kind = match &o.flags.obj_kind {
            flow_typing_type::type_::ObjKind::Exact => ty::ObjKind::ExactObj,
            flow_typing_type::type_::ObjKind::Inexact => ty::ObjKind::InexactObj,
            flow_typing_type::type_::ObjKind::Indexed(d) => {
                let dict_polarity = type_polarity(d.dict_polarity);
                let dict_key = type__::<I>(env, state, None, &d.key)?;
                let dict_value = type__::<I>(env, state, None, &d.value)?;
                ty::ObjKind::IndexedObj(ty::Dict {
                    dict_polarity,
                    dict_name: d.dict_name.clone(),
                    dict_key,
                    dict_value,
                })
            }
        };
        Ok(ty::ObjT {
            obj_def_loc,
            obj_kind,
            obj_props: obj_props.into(),
        })
    }

    fn obj_prop_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        name: &Name,
        prop: &flow_typing_type::type_::Property,
        inherited: bool,
        source: ty::PropSource,
    ) -> Result<Vec<ty::Prop<ALoc>>, Error> {
        use flow_typing_type::type_::PropertyInner;

        let def_locs = |fallback_t: &Type, p: &flow_typing_type::type_::Property| -> Vec<ALoc> {
            match flow_typing_type::type_::property::def_locs(p) {
                None => vec![flow_typing_type::type_util::loc_of_t(fallback_t).dupe()],
                Some(nel) => nel.into_vec(),
            }
        };

        match prop.deref() {
            PropertyInner::Field(fd) => {
                let polarity = type_polarity(fd.polarity);
                let (t, optional) = opt_t::<I>(env, state, &fd.type_)?;
                let dl = def_locs(&fd.type_, prop);
                let named_prop = ty::NamedProp::Field {
                    t,
                    polarity,
                    optional,
                };
                Ok(vec![ty::Prop::NamedProp {
                    name: name.clone(),
                    prop: named_prop,
                    inherited,
                    source,
                    def_locs: dl.into(),
                }])
            }
            PropertyInner::Method { type_, .. } => {
                let (method_t, optional) = match type_.deref() {
                    TypeInner::OptionalT { type_, .. } => (type_, true),
                    _ => (type_, false),
                };
                let tys = method_ty::<I>(env, state, method_t)?;
                let dl = def_locs(type_, prop);
                Ok(tys
                    .into_iter()
                    .map(|ft| ty::Prop::NamedProp {
                        name: name.clone(),
                        prop: ty::NamedProp::Method { fun: ft, optional },
                        inherited,
                        source: source.clone(),
                        def_locs: Arc::from(dl.clone()),
                    })
                    .collect())
            }
            PropertyInner::Get { type_, .. } => {
                let t = type__::<I>(env, state, None, type_)?;
                Ok(vec![ty::Prop::NamedProp {
                    name: name.clone(),
                    prop: ty::NamedProp::Get(t),
                    inherited,
                    source,
                    def_locs: def_locs(type_, prop).into(),
                }])
            }
            PropertyInner::Set { type_, .. } => {
                let t = type__::<I>(env, state, None, type_)?;
                Ok(vec![ty::Prop::NamedProp {
                    name: name.clone(),
                    prop: ty::NamedProp::Set(t),
                    inherited,
                    source,
                    def_locs: def_locs(type_, prop).into(),
                }])
            }
            PropertyInner::GetSet(gs) => {
                let get_prop = flow_typing_type::type_::Property::new(PropertyInner::Get {
                    key_loc: gs.get_key_loc.clone(),
                    type_: gs.get_type.clone(),
                });
                let mut props =
                    obj_prop_t::<I>(env, state, name, &get_prop, inherited, source.clone())?;
                let set_prop = flow_typing_type::type_::Property::new(PropertyInner::Set {
                    key_loc: gs.set_key_loc.clone(),
                    type_: gs.set_type.clone(),
                });
                props.extend(obj_prop_t::<I>(
                    env, state, name, &set_prop, inherited, source,
                )?);
                Ok(props)
            }
        }
    }

    fn call_prop_from_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        t: &Type,
    ) -> Result<Vec<ty::Prop<ALoc>>, Error> {
        let ts: Vec<Type> = match t.deref() {
            TypeInner::IntersectionT(_, rep) => rep.members_iter().map(|t| t.dupe()).collect(),
            _ => vec![t.clone()],
        };
        let mut result = Vec::new();
        for inner_t in &ts {
            let fun_ts = method_ty::<I>(env, state, inner_t)?;
            for ft in fun_ts {
                result.push(ty::Prop::CallProp(ft));
            }
        }
        Ok(result)
    }

    fn obj_props_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        props_id: flow_typing_type::type_::properties::Id,
        call_id_opt: Option<i32>,
        inherited: bool,
        source: ty::PropSource,
        allowed_prop_names: Option<&[Name]>,
    ) -> Result<Vec<ty::Prop<ALoc>>, Error> {
        let prop_map = env.cx.find_props(props_id);
        let props: Vec<_> = match allowed_prop_names {
            Some(names) => names
                .iter()
                .filter_map(|name| prop_map.get(name).map(|prop| (name, prop)))
                .collect(),
            None => prop_map.iter().collect(),
        };
        let call_props = match call_id_opt {
            Some(call_id) => {
                let call_t = env.cx.find_call(call_id);
                call_prop_from_t::<I>(env, state, &call_t)?
            }
            None => Vec::new(),
        };
        let mut converted_props = Vec::new();
        for (name, prop) in props {
            let props = obj_prop_t::<I>(env, state, name, prop, inherited, source.clone())?;
            converted_props.extend(props);
        }
        let mut obj_props = call_props;
        obj_props.extend(converted_props);
        Ok(obj_props)
    }

    fn arr_ty<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        arr: &flow_typing_type::type_::ArrType,
    ) -> Result<ALocTy, Error> {
        use flow_typing_type::type_::ArrType;
        use flow_typing_type::type_::ArrayATData;
        use flow_typing_type::type_::TupleATData;
        use flow_typing_type::type_::TupleView;
        use flow_typing_type::type_::TupleViewKind;
        match arr {
            ArrType::ArrayAT(box ArrayATData {
                tuple_view:
                    Some(TupleView {
                        kind: TupleViewKind::RestArguments,
                        elements,
                        inexact,
                        rest,
                        ..
                    }),
                ..
            })
            | ArrType::TupleAT(box TupleATData {
                elements,
                inexact,
                rest,
                ..
            }) => {
                let rest = rest.as_ref();
                // Heuristic to use $ReadOnly<> instead of repeating the polarity symbol
                // and a made up label in each element.
                let readonly = elements
                    .iter()
                    .chain(rest)
                    .all(|e| e.polarity == Polarity::Positive);
                let mut ty_elements = Vec::new();
                for elem in elements.iter() {
                    let inner_t = match elem.t.deref() {
                        TypeInner::OptionalT { type_, .. } if elem.optional => type_,
                        _ => &elem.t,
                    };
                    let t = type__::<I>(env, state, None, inner_t)?;
                    let polarity = if readonly {
                        ty::Polarity::Neutral
                    } else {
                        type_polarity(elem.polarity)
                    };
                    ty_elements.push(ty::TupleElement::TupleElement {
                        name: elem.name.clone(),
                        t,
                        polarity,
                        optional: elem.optional,
                    });
                }
                if let Some(rest) = rest {
                    let arr_elt_t = type__::<I>(env, state, None, &rest.t)?;
                    ty_elements.push(ty::TupleElement::TupleSpread {
                        name: rest.name.clone(),
                        t: Arc::new(ty::Ty::Arr(ty::ArrT {
                            arr_readonly: !readonly && rest.polarity == Polarity::Positive,
                            arr_elt_t,
                        })),
                    });
                }
                let tup = ty::Ty::Tup {
                    elements: ty_elements.into(),
                    inexact: *inexact,
                };
                if readonly {
                    Ok(Arc::new(ty::Ty::Utility(ty::Utility::ReadOnly(Arc::new(
                        tup,
                    )))))
                } else {
                    Ok(Arc::new(tup))
                }
            }
            ArrType::ArrayAT(box ArrayATData { elem_t, .. }) => {
                let arr_elt_t = type__::<I>(env, state, None, elem_t)?;
                Ok(Arc::new(ty::Ty::Arr(ty::ArrT {
                    arr_readonly: false,
                    arr_elt_t,
                })))
            }
            ArrType::ROArrayAT(box ROArrayATData { elem_t, .. }) => {
                let arr_elt_t = type__::<I>(env, state, None, elem_t)?;
                Ok(Arc::new(ty::Ty::Arr(ty::ArrT {
                    arr_readonly: true,
                    arr_elt_t,
                })))
            }
        }
    }

    fn to_generic<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        kind: ty::GenKind,
        reason: &Reason,
        inst: &flow_typing_type::type_::InstType,
    ) -> Result<ALocTy, Error> {
        let symbol = reason_utils::instance_symbol(env, reason, inst)?;
        let mut tys = Vec::new();
        for (_, _, t, _) in inst.type_args.iter() {
            tys.push(type__::<I>(env, state, None, t)?);
        }
        let targs = if tys.is_empty() { None } else { Some(tys) };
        Ok(Arc::new(ty::Ty::Generic(Box::new((
            symbol,
            kind,
            targs.map(Into::into),
        )))))
    }

    fn instance_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        reason: &Reason,
        super_: &Type,
        inst: &flow_typing_type::type_::InstType,
    ) -> Result<ALocTy, Error> {
        use flow_typing_type::type_::InstanceKind;
        match &inst.inst_kind {
            InstanceKind::InterfaceKind { inline: true, .. } => inline_interface::<I>(
                env,
                state,
                super_,
                inst.own_props.dupe(),
                inline_interface_proto_props(inst),
                inst.inst_call_t,
                inst.inst_construct_t,
                &inst.inst_dict,
            ),
            InstanceKind::InterfaceKind { inline: false, .. } => {
                to_generic::<I>(env, state, ty::GenKind::InterfaceKind, reason, inst)
            }
            InstanceKind::ClassKind => {
                to_generic::<I>(env, state, ty::GenKind::ClassKind, reason, inst)
            }
            InstanceKind::RecordKind { .. } => {
                to_generic::<I>(env, state, ty::GenKind::RecordKind, reason, inst)
            }
        }
    }

    fn inline_interface<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        super_: &Type,
        own_props: flow_typing_type::type_::properties::Id,
        proto_props: Option<flow_typing_type::type_::properties::Id>,
        inst_call_t: Option<i32>,
        inst_construct_t: Option<i32>,
        inst_dict: &flow_typing_type::type_::object::Dict,
    ) -> Result<ALocTy, Error> {
        use flow_typing_type::type_::PropertyInner;

        /// Whether a method type takes the receiver it is called on rather than
        /// an explicitly annotated `this`.
        fn has_implicit_receiver(cx: &mut Context<'_>, t: &Type) -> bool {
            let t = cx.find_resolved(t).unwrap_or_else(|| t.dupe());
            match t.deref() {
                TypeInner::OptionalT { type_, .. } => has_implicit_receiver(cx, type_),
                TypeInner::DefT(_, def_t) => match def_t.deref() {
                    DefTInner::FunT(_, ft) => {
                        matches!(ft.this_t.1, flow_typing_type::type_::ThisStatus::ThisMethod)
                    }
                    _ => false,
                },
                TypeInner::IntersectionT(_, rep) => rep
                    .members_iter()
                    .map(|t| t.dupe())
                    .collect::<Vec<_>>()
                    .iter()
                    .all(|t| has_implicit_receiver(cx, t)),
                _ => false,
            }
        }

        fn extends(ty: &ALocTy) -> Result<Vec<ty::GenericT<ALoc>>, Error> {
            match ty.as_ref() {
                ty::Ty::Generic(g) => Ok(vec![*g.clone()]),
                ty::Ty::Inter(t1, t2, ts) => {
                    let mut result = Vec::new();
                    result.extend(extends(t1)?);
                    result.extend(extends(t2)?);
                    for t in ts.iter() {
                        result.extend(extends(t)?);
                    }
                    Ok(result)
                }
                // Do not contribute to the extends clause
                // interface {} && interface { (): void }
                ty::Ty::TypeOf(box (ty::BuiltinOrSymbol::ObjProto, _))
                | ty::Ty::TypeOf(box (ty::BuiltinOrSymbol::FunProto, _)) => Ok(Vec::new()),
                // Top-level syntax only allows generics in extends
                _ => Err(terr(ErrorKind::BadInlineInterfaceExtends, None, None)),
            }
        }
        fn construct_props<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            inst_construct_t: Option<i32>,
        ) -> Result<Vec<ty::Prop<ALoc>>, Error> {
            match inst_construct_t {
                None => Ok(Vec::new()),
                Some(construct_id) => {
                    let ts: Vec<Type> = {
                        let ft = flow_typing_flow_common::flow_js_utils::read_construct_t(
                            env.cx,
                            construct_id,
                        );
                        match ft.deref() {
                            TypeInner::IntersectionT(_, rep) => {
                                rep.members_iter().map(|t| t.dupe()).collect()
                            }
                            _ => vec![ft.dupe()],
                        }
                    };
                    let mut fts: Vec<ty::FunT<ALoc>> = Vec::new();
                    for t in &ts {
                        fts.extend(method_ty::<I>(env, state, t)?);
                    }
                    Ok(fts
                        .into_iter()
                        .map(|ft| ty::Prop::NamedProp {
                            name: Name::new(FlowSmolStr::new_inline("new")),
                            prop: ty::NamedProp::Method {
                                fun: ft,
                                optional: false,
                            },
                            inherited: false,
                            source: ty::PropSource::Interface,
                            def_locs: Vec::new().into(),
                        })
                        .collect())
                }
            }
        }

        let super_ty = type__::<I>(env, state, None, super_)?;
        let if_extends = extends(&super_ty)?;
        let call_and_named = obj_props_t::<I>(
            env,
            state,
            own_props.dupe(),
            inst_call_t,
            false,
            ty::PropSource::Other,
            None,
        )?;
        // Methods and accessors of an interface body live on the prototype. A
        // name already among the own properties shadows its prototype entry.
        let proto = match proto_props {
            None => Vec::new(),
            Some(proto_props) => {
                let own_map = env.cx.find_props(own_props.dupe());
                let proto_map = env.cx.find_props(proto_props.dupe());
                let proto_names: Vec<Name> = proto_map
                    .keys()
                    .filter(|name| !own_map.contains_key(name))
                    .map(|name| name.dupe())
                    .collect();
                let implicit_receiver: Vec<Name> = proto_map
                    .iter()
                    .filter(|(_, prop)| match &***prop {
                        PropertyInner::Method { type_, .. } => has_implicit_receiver(env.cx, type_),
                        _ => false,
                    })
                    .map(|(name, _)| name.dupe())
                    .collect();
                let mut proto = obj_props_t::<I>(
                    env,
                    state,
                    proto_props,
                    None,
                    false,
                    ty::PropSource::Other,
                    Some(&proto_names),
                )?;
                // The implicit receiver of an interface method is the interface
                // itself, which would print the whole interface again.
                for prop in &mut proto {
                    if let ty::Prop::NamedProp {
                        name,
                        prop: ty::NamedProp::Method { fun, .. },
                        ..
                    } = prop
                        && implicit_receiver.contains(name)
                    {
                        fun.fun_this_param = None;
                    }
                }
                proto
            }
        };
        let construct = construct_props::<I>(env, state, inst_construct_t)?;
        // Print order: construct signatures first, then call signature +
        // regular properties. Multiple overloads at the [inst_construct_t]
        // slot expand into multiple [Ty.NamedProp] entries (see
        // [construct_props]), all of which print before the rest.
        let mut if_props = construct;
        if_props.extend(call_and_named);
        if_props.extend(proto);
        let if_dict = match inst_dict {
            Some(dict_type) => {
                let dict_polarity = type_polarity(dict_type.dict_polarity);
                let dict_key = type__::<I>(env, state, None, &dict_type.key)?;
                let dict_value = type__::<I>(env, state, None, &dict_type.value)?;
                Some(ty::Dict {
                    dict_polarity,
                    dict_name: dict_type.dict_name.dupe(),
                    dict_key,
                    dict_value,
                })
            }
            None => None,
        };
        Ok(Arc::new(ty::Ty::InlineInterface(Box::new(
            ty::InterfaceT {
                if_extends: if_extends.into(),
                if_props: if_props.into(),
                if_dict,
            },
        ))))
    }

    pub(super) fn convert_component<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        config: &Type,
        renders: &Type,
        expand_config_alias: bool,
    ) -> Result<(ty::ComponentProps<ALoc>, Option<ALocTy>), Error> {
        use flow_typing_type::type_::CanonicalRendersForm;

        fn expanded_config<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            config: &Type,
        ) -> Result<Option<ALocTy>, Error> {
            let TypeInner::EvalT {
                type_: operand,
                defer_use_t,
                id,
            } = config.deref()
            else {
                return Ok(None);
            };
            let flow_typing_type::type_::TypeDestructorTInner(_, _, destructor) =
                defer_use_t.deref();
            // The attempt runs for any non-generic operand, including
            // utilities (`Pick`, `Omit`, `Partial`, `ReadOnly`) and inline
            // objects. Only a result that is an object of plain field
            // props is used; anything else keeps its written form via the
            // check below and the fallback above.
            if !matches!(&**destructor, Destructor::ReactCheckComponentConfig { .. })
                || operand_is_generic(env, operand)
                || env.seen_eval_ids.contains(id)
            {
                return Ok(None);
            }
            descend(env, config)?;
            let old_seen_eval_ids = env.seen_eval_ids.dupe();
            env.seen_eval_ids.insert(id.dupe());
            let expanded = eval_t::<I>(
                env,
                state,
                // Dispatch has already produced the config object. Bypass its
                // outer alias while preserving aliases in its property types.
                |env, state, id, t| type_ctor::<I>(env, state, id, type_with_alias_reason::<I>, t),
                |env, state, _id, t| type__::<I>(env, state, None, t),
                type_destructor_unevaluated::<I>,
                true,
                (operand, defer_use_t, id),
            );
            env.depth -= 1;
            env.seen_eval_ids = old_seen_eval_ids;
            let expanded = expanded?;
            Ok(matches!(
                expanded.as_ref(),
                ty::Ty::Obj(obj)
                    if obj.obj_props.iter().all(|prop| matches!(
                        prop,
                        ty::Prop::NamedProp {
                            prop: ty::NamedProp::Field { .. },
                            ..
                        }
                    ))
            )
            .then_some(expanded))
        }

        let config_ty = if expand_config_alias {
            match expanded_config::<I>(env, state, config)? {
                Some(expanded) => expanded,
                None => type__::<I>(env, state, None, config)?,
            }
        } else {
            type__::<I>(env, state, None, config)?
        };
        let renders_ty = match renders.deref() {
            TypeInner::DefT(_, def_t)
                if matches!(
                    def_t.deref(),
                    DefTInner::RendersT(r) if matches!(r.deref(), CanonicalRendersForm::DefaultRenders)
                ) =>
            {
                None
            }
            _ => Some(type__::<I>(env, state, None, renders)?),
        };
        let regular_props = match config_ty.as_ref() {
            ty::Ty::Obj(obj)
                if matches!(obj.obj_kind, ty::ObjKind::ExactObj)
                    && matches!(&obj.obj_props[..], [ty::Prop::SpreadProp(_)]) =>
            {
                match &obj.obj_props[0] {
                    ty::Prop::SpreadProp(config) => {
                        ty::ComponentProps::UnflattenedComponentProps(config.dupe())
                    }
                    _ => unreachable!(),
                }
            }
            ty::Ty::Obj(obj) => {
                // An indexer tail is carried alongside the flat params so it
                // prints as `...{[string]: T}` instead of being silently
                // dropped. Anything that is not a plain field prop still
                // keeps the whole config written via the fallback below.
                let dict = match &obj.obj_kind {
                    ty::ObjKind::IndexedObj(dict) => Some(dict.clone()),
                    ty::ObjKind::ExactObj
                    | ty::ObjKind::InexactObj
                    | ty::ObjKind::MappedTypeObj => None,
                };
                let props_result: Result<Vec<_>, _> = obj
                    .obj_props
                    .iter()
                    .map(|p| match p {
                        ty::Prop::NamedProp {
                            name,
                            prop: ty::NamedProp::Field { t, optional, .. },
                            def_locs,
                            ..
                        } => Ok(ty::FlattenedComponentProp::FlattenedComponentProp {
                            name: name.clone(),
                            optional: *optional,
                            def_locs: def_locs.clone(),
                            t: t.clone(),
                        }),
                        _ => Err(()),
                    })
                    .collect();
                match props_result {
                    Ok(props) => ty::ComponentProps::FlattenedComponentProps {
                        props: props.into(),
                        inexact: matches!(obj.obj_kind, ty::ObjKind::InexactObj),
                        dict,
                    },
                    Err(_) => ty::ComponentProps::UnflattenedComponentProps(config_ty.dupe()),
                }
            }
            _ => ty::ComponentProps::UnflattenedComponentProps(config_ty.dupe()),
        };
        Ok((regular_props, renders_ty))
    }

    fn this_class_t<'cx>(
        env: &mut Env<'_, 'cx>,
        reason: &Reason,
        inst_t: &flow_typing_type::type_::InstanceT,
    ) -> Result<ALocTy, Error> {
        use flow_typing_type::type_::InstanceKind;
        match &inst_t.inst.inst_kind {
            InstanceKind::ClassKind | InstanceKind::RecordKind { .. } => {
                let symbol = reason_utils::instance_symbol(env, reason, &inst_t.inst)?;
                Ok(Arc::new(ty::Ty::TypeOf(Box::new((
                    ty::BuiltinOrSymbol::TSymbol(symbol),
                    None,
                )))))
            }
            InstanceKind::InterfaceKind { .. } => {
                Err(terr(ErrorKind::BadThisClassT, Some("InterfaceKind"), None))
            }
        }
    }

    pub fn type_params_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        tparams: &[flow_typing_type::type_::TypeParam],
    ) -> Result<Option<Vec<ty::TypeParam<ALoc>>>, Error> {
        let mut result = Vec::new();
        for tp in tparams.iter() {
            result.push(type_param::<I>(env, state, tp)?);
        }
        let ps = if result.is_empty() {
            None
        } else {
            Some(result)
        };
        Ok(ps)
    }

    fn poly_ty<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        t_out: &Type,
        tparams: &[flow_typing_type::type_::TypeParam],
    ) -> Result<ALocTy, Error> {
        match t_out.deref() {
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::ClassT(class_t) => match class_t.deref() {
                    TypeInner::ThisInstanceT(box ThisInstanceTData {
                        reason: r,
                        instance: inst_t,
                        ..
                    }) => this_class_t(env, r, inst_t),
                    _ => Err(terr(ErrorKind::BadPoly, None, Some(t_out))),
                },
                DefTInner::FunT(static_, f) => {
                    let ps = type_params_t::<I>(env, state, tparams)?;
                    let fun_t = fun_ty::<I>(env, state, static_, f, ps)?;
                    Ok(Arc::new(ty::Ty::Fun(Box::new(fun_t))))
                }
                DefTInner::ReactAbstractComponentT(_) => type__::<I>(env, state, None, t_out),
                _ => Err(terr(ErrorKind::BadPoly, None, Some(t_out))),
            },
            _ => Err(terr(ErrorKind::BadPoly, None, Some(t_out))),
        }
    }

    fn type_app<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        from_value: bool,
        t: &Type,
        targs: Option<&[Type]>,
    ) -> Result<ALocTy, Error> {
        fn mk_generic<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            symbol: ALocSymbol,
            kind: ty::GenKind,
            tparams: &[flow_typing_type::type_::TypeParam],
            targs: Option<&[Type]>,
        ) -> Result<ALocTy, Error> {
            let converted_targs = match targs {
                None => None,
                Some(ts) => {
                    let mut result = Vec::new();
                    for t in ts {
                        result.push(type__::<I>(env, state, None, t)?);
                    }
                    Some(result)
                }
            };
            let converted_targs = if env.omit_targ_defaults() {
                // Disable the option for recursive calls to type_params_t to avoid
                // infinite recursion in cases like `class C<T: C<any>> {}`
                let old = env.omit_targ_defaults;
                env.omit_targ_defaults = false;
                let tparams_converted = type_params_t::<I>(env, state, tparams)?;
                env.omit_targ_defaults = old;
                remove_targs_matching_defaults(converted_targs, tparams_converted.as_ref())
            } else {
                converted_targs
            };
            Ok(Arc::new(ty::Ty::Generic(Box::new((
                symbol,
                kind,
                converted_targs.map(Into::into),
            )))))
        }

        fn instance_app<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            r: &Reason,
            inst: &flow_typing_type::type_::InstType,
            tparams: &[flow_typing_type::type_::TypeParam],
            targs: Option<&[Type]>,
        ) -> Result<ALocTy, Error> {
            let symbol = reason_utils::instance_symbol(env, r, inst)?;
            let kind = match &inst.inst_kind {
                flow_typing_type::type_::InstanceKind::InterfaceKind { .. } => {
                    ty::GenKind::InterfaceKind
                }
                flow_typing_type::type_::InstanceKind::ClassKind => ty::GenKind::ClassKind,
                flow_typing_type::type_::InstanceKind::RecordKind { .. } => ty::GenKind::RecordKind,
            };
            mk_generic::<I>(env, state, symbol, kind, tparams, targs)
        }

        fn type_t_app<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            r: &Reason,
            kind: &flow_typing_type::type_::TypeTKind,
            tparams: &[flow_typing_type::type_::TypeParam],
            targs: Option<&[Type]>,
        ) -> Result<ALocTy, Error> {
            use flow_typing_type::type_::TypeTKind;
            let symbol = match kind {
                TypeTKind::TypeAliasKind | TypeTKind::InstanceKind | TypeTKind::RenderTypeKind => {
                    reason_utils::local_type_alias_symbol(env, r)?
                }
                TypeTKind::ImportTypeofKind
                | TypeTKind::ImportClassKind
                | TypeTKind::ImportEnumKind => reason_utils::imported_type_alias_symbol(env, r)?,
                TypeTKind::OpaqueKind => reason_utils::opaque_type_alias_symbol(env, r)?,
                TypeTKind::TypeParamKind => {
                    return Err(terr(ErrorKind::BadTypeAlias, Some("TypeParamKind"), None));
                }
            };
            mk_generic::<I>(
                env,
                state,
                symbol,
                ty::GenKind::TypeAliasKind,
                tparams,
                targs,
            )
        }

        fn singleton_poly<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            targs: Option<&[Type]>,
            tparams: &[flow_typing_type::type_::TypeParam],
            t: &Type,
        ) -> Result<ALocTy, Error> {
            use flow_typing_type::type_::InstanceKind;
            use flow_typing_type::type_::TypeTKind;
            match t.deref() {
                TypeInner::DefT(r, def_t) => match def_t.deref() {
                    DefTInner::TypeT(kind, inner) => match inner.deref() {
                        TypeInner::ThisInstanceT(box ThisInstanceTData {
                            reason,
                            instance,
                            ..
                        }) if matches!(kind, TypeTKind::ImportClassKind) => {
                            instance_app::<I>(env, state, reason, &instance.inst, tparams, targs)
                        }
                        TypeInner::DefT(inner_r, inner_def) => match inner_def.deref() {
                            DefTInner::InstanceT(instance_t) => {
                                let r = if matches!(
                                    instance_t.inst.inst_kind,
                                    InstanceKind::InterfaceKind { inline: true, .. }
                                ) {
                                    r
                                } else {
                                    inner_r
                                };
                                instance_app::<I>(env, state, r, &instance_t.inst, tparams, targs)
                            }
                            _ => type_t_app::<I>(env, state, r, kind, tparams, targs),
                        },
                        TypeInner::TypeAppT(box TypeAppTData {
                            type_, from_value, ..
                        }) => {
                            let is_interface = if matches!(kind, TypeTKind::ImportClassKind) {
                                match type_.deref() {
                                    TypeInner::DefT(_, def_t) => match def_t.deref() {
                                        DefTInner::PolyT(box PolyTData { t_out, .. }) => {
                                            match t_out.deref() {
                                                TypeInner::DefT(_, inner_def_t) => {
                                                    match inner_def_t.deref() {
                                                        DefTInner::ClassT(inner) => {
                                                            match inner.deref() {
                                                                TypeInner::ThisInstanceT(box ThisInstanceTData {
                                                                    instance,
                                                                    ..
                                                                }) => matches!(
                                                                    instance.inst.inst_kind,
                                                                    InstanceKind::InterfaceKind { .. }
                                                                ),
                                                                TypeInner::DefT(_, inst_def_t) => {
                                                                    match inst_def_t.deref() {
                                                                        DefTInner::InstanceT(instance_t) => matches!(
                                                                            instance_t.inst.inst_kind,
                                                                            InstanceKind::InterfaceKind { .. }
                                                                        ),
                                                                        _ => false,
                                                                    }
                                                                }
                                                                _ => false,
                                                            }
                                                        }
                                                        _ => false,
                                                    }
                                                }
                                                _ => false,
                                            }
                                        }
                                        _ => false,
                                    },
                                    _ => false,
                                }
                            } else {
                                false
                            };
                            if is_interface {
                                type_app::<I>(env, state, *from_value, type_, targs)
                            } else {
                                type_t_app::<I>(env, state, r, kind, tparams, targs)
                            }
                        }
                        _ => type_t_app::<I>(env, state, r, kind, tparams, targs),
                    },
                    DefTInner::ClassT(inner) => match inner.deref() {
                        TypeInner::ThisInstanceT(box ThisInstanceTData {
                            reason,
                            instance,
                            ..
                        }) => instance_app::<I>(env, state, reason, &instance.inst, tparams, targs),
                        TypeInner::DefT(inner_r, inner_def) => match inner_def.deref() {
                            DefTInner::InstanceT(instance_t) => instance_app::<I>(
                                env,
                                state,
                                inner_r,
                                &instance_t.inst,
                                tparams,
                                targs,
                            ),
                            _ => {
                                let msg =
                                    format!("PolyT:{}", flow_typing_type::type_::string_of_ctor(t));
                                Err(terr(ErrorKind::BadTypeApp, Some(&msg), None))
                            }
                        },
                        TypeInner::TypeAppT(box TypeAppTData {
                            type_, from_value, ..
                        }) => type_app::<I>(env, state, *from_value, type_, targs),
                        _ => {
                            let msg =
                                format!("PolyT:{}", flow_typing_type::type_::string_of_ctor(t));
                            Err(terr(ErrorKind::BadTypeApp, Some(&msg), None))
                        }
                    },
                    DefTInner::ReactAbstractComponentT(box ReactAbstractComponentTData {
                        component_kind: flow_typing_type::type_::ComponentKind::Nominal(_, name, _),
                        ..
                    }) => {
                        let symbol = reason_utils::component_symbol(env, name, r)?;
                        mk_generic::<I>(
                            env,
                            state,
                            symbol,
                            ty::GenKind::ComponentKind,
                            tparams,
                            targs,
                        )
                    }
                    _ => {
                        let msg = format!("PolyT:{}", flow_typing_type::type_::string_of_ctor(t));
                        Err(terr(ErrorKind::BadTypeApp, Some(&msg), None))
                    }
                },
                _ => {
                    let msg = format!("PolyT:{}", flow_typing_type::type_::string_of_ctor(t));
                    Err(terr(ErrorKind::BadTypeApp, Some(&msg), None))
                }
            }
        }

        fn singleton<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            from_value: bool,
            targs: Option<&[Type]>,
            t: &Type,
        ) -> Result<ALocTy, Error> {
            if from_value && let Some(targs) = targs {
                return I::typeapp(
                    env,
                    state,
                    &mut |env, state, t| type__::<I>(env, state, None, t),
                    &mut |env, state, t| type__::<I>(env, state, None, t),
                    app_on_generic,
                    from_value,
                    type_util::reason_of_t(t).dupe(),
                    t.dupe(),
                    targs,
                );
            }
            match t.deref() {
                TypeInner::AnyT(_, _) => type__::<I>(env, state, None, t),
                TypeInner::DefT(_, def_t) => match def_t.deref() {
                    DefTInner::PolyT(box PolyTData { tparams, t_out, .. }) => {
                        singleton_poly::<I>(env, state, targs, tparams, t_out)
                    }
                    DefTInner::ClassT(inner) => match inner.deref() {
                        TypeInner::ThisInstanceT(box ThisInstanceTData {
                            reason,
                            instance,
                            ..
                        }) => {
                            // This is likely an error - cannot apply on non-polymorphic type.
                            // E.g type Foo = any; var x: Foo<number>
                            let synth = Type::new(TypeInner::DefT(
                                reason.dupe(),
                                flow_typing_type::type_::DefT::new(DefTInner::InstanceT(
                                    std::rc::Rc::new(instance.clone()),
                                )),
                            ));
                            type__::<I>(env, state, None, &synth)
                        }
                        _ if targs.is_none() => {
                            // For example see tests/type-at-pos_class/FluxStore.js
                            let converted = type__::<I>(env, state, None, inner)?;
                            Ok(Arc::new(ty::Ty::Utility(ty::Utility::Class(converted))))
                        }
                        _ => {
                            let msg = flow_typing_type::type_::string_of_ctor(t);
                            Err(terr(ErrorKind::BadTypeApp, Some(msg), None))
                        }
                    },
                    // This is likely an error - cannot apply on non-polymorphic type.
                    // E.g type Foo = any; var x: Foo<number>
                    DefTInner::TypeT(_, inner) => type__::<I>(env, state, None, inner),
                    _ => {
                        let msg = flow_typing_type::type_::string_of_ctor(t);
                        Err(terr(ErrorKind::BadTypeApp, Some(msg), None))
                    }
                },
                TypeInner::UnionT(_, rep) => {
                    // This case targeting UnionTs created during tvar_resolution.
                    let members: Vec<Type> = rep.members_iter().map(|t| t.dupe()).collect();
                    let mut tys = Vec::new();
                    for inner_t in &members {
                        tys.push(singleton::<I>(env, state, from_value, targs, inner_t)?);
                    }
                    match ty::mk_union(true, tys) {
                        Some(arc_ty) => Ok(arc_ty),
                        None => Ok(Arc::new(ty::Ty::Bot(BotKind::NoLowerWithUpper(
                            UpperBoundKind::NoUpper,
                        )))),
                    }
                }
                // This is most likely already a Flow error: E.g.
                //
                // function f<A>(): void { }
                // type Foo = f<number>;
                //
                // gives "Cannot use function as a type."
                _ => {
                    let msg = flow_typing_type::type_::string_of_ctor(t);
                    Err(terr(ErrorKind::BadTypeApp, Some(msg), None))
                }
            }
        }

        match lookahead::peek(env.cx, t) {
            lookahead::Lookahead::Recursive => {
                Err(terr(ErrorKind::BadTypeApp, Some("recursive"), Some(t)))
            }
            // It's unlikely that an upper bound would be useful here
            lookahead::Lookahead::LowerBounds(ts) if ts.is_empty() => Ok(Arc::new(ty::Ty::Bot(
                BotKind::NoLowerWithUpper(UpperBoundKind::NoUpper),
            ))),
            lookahead::Lookahead::LowerBounds(ts) => {
                // TypeAppT distributes over multiple lower bounds
                let mut tys = Vec::new();
                for inner_t in &ts {
                    tys.push(singleton::<I>(env, state, from_value, targs, inner_t)?);
                }
                match ty::mk_union(true, tys) {
                    Some(arc_ty) => Ok(arc_ty),
                    None => Ok(Arc::new(ty::Ty::Bot(BotKind::NoLowerWithUpper(
                        UpperBoundKind::NoUpper,
                    )))),
                }
            }
        }
    }

    fn builtin_application<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        reason: &Reason,
        name: &str,
        args: &[Type],
    ) -> Result<ALocTy, Error> {
        let builtin = flow_typing_flow_common::flow_js_utils::lookup_builtin_type(
            env.cx,
            name,
            reason.dupe(),
        );
        type_app::<I>(env, state, false, &builtin, Some(args))
    }

    fn nominal_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        reason: &Reason,
        nominal: &NominalType,
    ) -> Result<ALocTy, Error> {
        use flow_typing_type::type_::nominal::Id as NominalId;
        match &nominal.nominal_id {
            NominalId::UserDefinedOpaqueTypeId(box nominal::UserDefinedOpaqueTypeIdData(
                _,
                name,
            )) => {
                let opaque_symbol = symbol_from_reason(env, reason, name.dupe())?;
                let targs = if nominal.nominal_type_args.is_empty() {
                    None
                } else {
                    let mut result = Vec::new();
                    for (_, _, t, _) in nominal.nominal_type_args.iter() {
                        result.push(type__::<I>(env, state, None, t)?);
                    }
                    Some(result)
                };
                Ok(generic_talias(opaque_symbol, targs))
            }
            // InternalEnforceUnionOptimized is a special opaque type that only appears in upper
            // bound position to test optimization. It should never be visible to users.
            NominalId::InternalEnforceUnionOptimized => Err(terr(
                ErrorKind::UnsupportedTypeCtor,
                Some("Special InternalEnforceUnionOptimized upper bound"),
                None,
            )),
            NominalId::ExactObjectType => Ok(generic_talias(
                builtin_symbol(FlowSmolStr::new_inline("ExactObjectType")),
                None,
            )),
            NominalId::StuckEval(kind) => stuck_eval::<I>(env, state, reason, kind, nominal),
        }
    }

    fn stuck_eval<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        reason: &Reason,
        kind: &nominal::StuckEvalKind,
        nominal: &NominalType,
    ) -> Result<ALocTy, Error> {
        use flow_typing_type::type_::Destructor as D;
        use nominal::StuckEvalKind as K;

        let args: Vec<&Type> = nominal
            .nominal_type_args
            .iter()
            .map(|(_, _, t, _)| t)
            .collect();
        if let (K::StuckEvalForGenericallyMappedObject(name), [bound]) = (kind, args.as_slice()) {
            use flow_common::subst_name::OpKind;
            use flow_common::subst_name::SubstNameInner;
            return match name.deref() {
                SubstNameInner::Synthetic {
                    op_kind: Some(OpKind::MappedObject | OpKind::MappedArray),
                    ..
                } => type__::<I>(env, state, None, bound),
                _ => subst_name::<I>(env, state, reason.def_loc().dupe(), bound, bound, name),
            };
        }
        if let (
            K::StuckEvalForConditionalType { infer_tparam_ids },
            [check_t, extends_t, true_t, false_t],
        ) = (kind, args.as_slice())
        {
            let check_type = type__::<I>(env, state, None, check_t)?;
            return conditional_type::<I>(
                env,
                state,
                ConditionalTypeInput {
                    check_type,
                    infer_tparam_ids: infer_tparam_ids.dupe(),
                    extends_t,
                    true_t,
                    false_t,
                },
            );
        }
        let (input, destructor) = match (kind, args.as_slice()) {
            (K::StuckEvalForNonMaybeType, [t]) => (*t, D::NonMaybeType),
            (K::StuckEvalForExactType, [t]) => (*t, D::ExactType),
            (K::StuckEvalForReadOnlyType, [t]) => (*t, D::ReadOnlyType),
            (K::StuckEvalForPartialType, [t]) => (*t, D::PartialType),
            (K::StuckEvalForRequiredType, [t]) => (*t, D::RequiredType),
            (K::StuckEvalForValuesType, [t]) => (*t, D::ValuesType),
            (K::StuckEvalForKeyMirrorType, [t]) => (*t, D::TypeMap(TypeMap::ObjectKeyMirror)),
            (K::StuckEvalForEnumType, [t]) => (*t, D::EnumType),
            (K::StuckEvalForPropertyType { name }, [t]) => (
                *t,
                D::PropertyType {
                    name: name.deref().dupe(),
                },
            ),
            (K::StuckEvalForElementType, [t, index]) => (
                *t,
                D::ElementType {
                    index_type: (*index).dupe(),
                },
            ),
            (K::StuckEvalForOptionalIndexedAccessWithStrLitIndexNonMaybeType { name }, [t]) => (
                *t,
                D::OptionalIndexedAccessNonMaybeType {
                    index: OptionalIndexedAccessIndex::OptionalIndexedAccessStrLitIndex(
                        name.deref().dupe(),
                    ),
                },
            ),
            (K::StuckEvalForOptionalIndexedAccessWithTypeIndexNonMaybeType, [t, index]) => (
                *t,
                D::OptionalIndexedAccessNonMaybeType {
                    index: OptionalIndexedAccessIndex::OptionalIndexedAccessTypeIndex(
                        (*index).dupe(),
                    ),
                },
            ),
            (K::StuckEvalForOptionalIndexedAccessResultType, [t])
            | (K::StuckEvalForReactDRO(_), [t]) => return type__::<I>(env, state, None, t),
            _ => {
                return Err(terr(
                    ErrorKind::BadEvalT,
                    Some("invalid stuck evaluation arguments"),
                    None,
                ));
            }
        };
        type_destructor_unevaluated::<I>(env, state, input, &destructor)
    }

    fn subst_name<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        loc: ALoc,
        t: &Type,
        bound: &Type,
        name: &flow_common::subst_name::SubstName,
    ) -> Result<ALocTy, Error> {
        use flow_common::subst_name::OpKind;
        use flow_common::subst_name::SubstNameInner;
        match name.deref() {
            // When `op_kind` is None we can rely on `name`
            SubstNameInner::Name(n)
            | SubstNameInner::Id(_, n)
            | SubstNameInner::Synthetic {
                op_kind: None,
                name: n,
                ..
            } => Ok(Arc::new(ty::Ty::Bound(Box::new((loc, n.to_string()))))),
            SubstNameInner::Synthetic {
                op_kind: Some(op),
                ts,
                ..
            } => match op {
                OpKind::ReadOnly if ts.len() == 1 => {
                    let inner = subst_name::<I>(env, state, loc, t, bound, &ts[0])?;
                    Ok(Arc::new(ty::Ty::Utility(ty::Utility::ReadOnly(inner))))
                }
                OpKind::Partial if ts.len() == 1 => {
                    let inner = subst_name::<I>(env, state, loc, t, bound, &ts[0])?;
                    Ok(Arc::new(ty::Ty::Utility(ty::Utility::Partial(inner))))
                }
                OpKind::Required if ts.len() == 1 => {
                    let inner = subst_name::<I>(env, state, loc, t, bound, &ts[0])?;
                    Ok(Arc::new(ty::Ty::Utility(ty::Utility::Required(inner))))
                }
                OpKind::Spread => {
                    let mut obj_props: Vec<ty::Prop<ALoc>> = ts
                        .iter()
                        .map(|b| {
                            let t = subst_name::<I>(env, state, loc.clone(), t, bound, b)?;
                            Ok(ty::Prop::SpreadProp(t))
                        })
                        .collect::<Result<Vec<_>, Error>>()?;
                    let ty_bound = type__::<I>(env, state, None, bound)?;
                    let obj_kind = match ty_bound.as_ref() {
                        ty::Ty::Obj(obj_t) => {
                            obj_props.extend(obj_t.obj_props.iter().cloned());
                            obj_t.obj_kind.clone()
                        }
                        _ => ty::ObjKind::ExactObj,
                    };
                    Ok(Arc::new(ty::Ty::Obj(Box::new(ty::ObjT {
                        obj_def_loc: None,
                        obj_props: obj_props.into(),
                        obj_kind,
                    }))))
                }
                _ => Err(terr(ErrorKind::SyntheticBoundT, None, Some(t))),
            },
        }
    }

    fn generic_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        generic: &GenericTData,
        t: &Type,
    ) -> Result<ALocTy, Error> {
        let GenericTData {
            reason,
            name,
            bound,
            id,
            ..
        } = generic;
        // GenericT normalizes to Ty.Bound, except for conditional "infer" types.
        if id
            .definition_id()
            .is_some_and(|id| env.infer_tparam_ids.contains(id))
        {
            let symbol = symbol_from_reason(env, reason, name.string_of_subst_name().dupe())?;
            let bound = param_bound::<I>(env, state, bound)?;
            Ok(Arc::new(ty::Ty::Infer(Box::new((symbol, bound)))))
        } else {
            let loc = id
                .definition_id()
                .map_or_else(|| reason.def_loc().dupe(), |id| id.0.dupe());
            subst_name::<I>(env, state, loc, t, bound, name)
        }
    }

    struct ConditionalTypeInput<'a> {
        check_type: ALocTy,
        infer_tparam_ids: Rc<Vec<ALocId>>,
        extends_t: &'a Type,
        true_t: &'a Type,
        false_t: &'a Type,
    }

    fn conditional_type<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        input: ConditionalTypeInput<'_>,
    ) -> Result<ALocTy, Error> {
        let ConditionalTypeInput {
            check_type,
            infer_tparam_ids,
            extends_t,
            true_t,
            false_t,
        } = input;
        let old_infer = std::mem::replace(&mut env.infer_tparam_ids, infer_tparam_ids);
        let extends_type = type__::<I>(env, state, None, extends_t);
        env.infer_tparam_ids = old_infer;
        let extends_type = extends_type?;
        let true_type = type__::<I>(env, state, None, true_t)?;
        let false_type = type__::<I>(env, state, None, false_t)?;
        Ok(Arc::new(ty::Ty::Conditional {
            check_type,
            extends_type,
            true_type,
            false_type,
        }))
    }

    fn param_bound<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        bound: &Type,
    ) -> Result<Option<ALocTy>, Error> {
        match bound.deref() {
            TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::MixedT(_)) => Ok(None),
            _ => {
                // A bound is a constraint the author wrote about a parameter that
                // is generic by construction, so reducing it answers a question
                // nobody asked -- `keyof EventMap` becomes every key. TypeScript
                // prints a bound from its declared type node for the same reason.
                let restore = matches!(
                    env.evaluate_type_destructors(),
                    EvaluateTypeDestructorsMode::EvaluateForHover
                )
                .then(|| {
                    std::mem::replace(
                        &mut env.genv.options.evaluate_type_destructors,
                        EvaluateTypeDestructorsMode::EvaluateNone,
                    )
                });
                let t = type__::<I>(env, state, None, bound);
                if let Some(mode) = restore {
                    env.genv.options.evaluate_type_destructors = mode;
                }
                Ok(Some(t?))
            }
        }
    }

    fn type_param<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        tp: &flow_typing_type::type_::TypeParam,
    ) -> Result<ty::TypeParam<ALoc>, Error> {
        let tp_polarity = type_polarity(tp.polarity);
        let tp_bound = param_bound::<I>(env, state, &tp.bound)?;
        let tp_default = match &tp.default {
            Some(d) => Some(type__::<I>(env, state, None, d)?),
            None => None,
        };
        let tp_name = tp.name.string_of_subst_name().dupe();
        Ok(ty::TypeParam {
            tp_name,
            tp_bound,
            tp_polarity,
            tp_default,
            tp_const: tp.is_const,
        })
    }

    fn opt_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        t: &Type,
    ) -> Result<(ALocTy, bool), Error> {
        match t.deref() {
            TypeInner::OptionalT { type_, .. } => {
                let ty = type__::<I>(env, state, None, type_)?;
                Ok((ty, true))
            }
            _ => {
                let ty = type__::<I>(env, state, None, t)?;
                Ok((ty, false))
            }
        }
    }

    fn type_polarity(p: Polarity) -> ty::Polarity {
        match p {
            Polarity::Positive => ty::Polarity::Positive,
            Polarity::Negative => ty::Polarity::Negative,
            Polarity::Neutral => ty::Polarity::Neutral,
        }
    }

    // EvalT

    fn spread_of_ty(ty: &ALocTy) -> Vec<ty::Prop<ALoc>> {
        match ty.as_ref() {
            ty::Ty::Obj(obj) => obj.obj_props.to_vec(),
            _ => vec![ty::Prop::SpreadProp(ty.dupe())],
        }
    }

    fn spread<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        ty: ALocTy,
        target: &flow_typing_type::type_::object::spread::Target,
        ts: &flow_data_structure_wrapper::list::FlowOcamlList<
            flow_typing_type::type_::object::spread::Operand,
        >,
        head_slice: Option<&flow_typing_type::type_::object::spread::OperandSlice>,
    ) -> Result<ALocTy, Error> {
        use flow_typing_type::type_::object::spread;

        fn obj_exact(target: &spread::Target) -> Result<bool, Error> {
            match target {
                spread::Target::Annot { make_exact, .. } => Ok(*make_exact),
                spread::Target::Value { .. } => {
                    Err(terr(ErrorKind::BadEvalT, Some("spread-target-value"), None))
                }
            }
        }

        fn mk_spread(
            ty: &ALocTy,
            target: &spread::Target,
            prefix_tys: Vec<ty::Prop<ALoc>>,
            head_slice: &Option<ALocTy>,
        ) -> Result<ALocTy, Error> {
            let mut obj_props = prefix_tys;
            obj_props.extend(spread_of_ty(ty));
            if let Some(obj) = head_slice {
                obj_props.extend(spread_of_ty(obj));
            }
            let exact = obj_exact(target)?;
            let obj_kind = if exact {
                ty::ObjKind::ExactObj
            } else {
                ty::ObjKind::InexactObj
            };
            Ok(Arc::new(ty::Ty::Obj(Box::new(ty::ObjT {
                obj_def_loc: None,
                obj_props: obj_props.into(),
                obj_kind,
            }))))
        }

        fn spread_operand_slice<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            slice: &spread::OperandSlice,
        ) -> Result<ALocTy, Error> {
            let mut obj_props = Vec::new();
            for (name, prop) in slice.prop_map.iter().rev() {
                let props = obj_prop_t::<I>(env, state, name, prop, false, ty::PropSource::Other)?;
                obj_props.extend(props);
            }
            let obj_kind = match &slice.dict {
                Some(dict) => {
                    let dict_key = type__::<I>(env, state, None, &dict.key)?;
                    let dict_value = type__::<I>(env, state, None, &dict.value)?;
                    ty::ObjKind::IndexedObj(ty::Dict {
                        dict_polarity: type_polarity(dict.dict_polarity),
                        dict_name: dict.dict_name.clone(),
                        dict_key,
                        dict_value,
                    })
                }
                None => ty::ObjKind::ExactObj,
            };
            Ok(Arc::new(ty::Ty::Obj(Box::new(ty::ObjT {
                obj_def_loc: None,
                obj_props: obj_props.into(),
                obj_kind,
            }))))
        }

        fn spread_operand<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            operand: &spread::Operand,
        ) -> Result<ALocTy, Error> {
            match operand {
                spread::Operand::Type(t) => type__::<I>(env, state, None, t),
                spread::Operand::Slice(slice) => spread_operand_slice::<I>(env, state, slice),
            }
        }

        let head_slice_ty = match head_slice {
            None => None,
            Some(s) => {
                let s = spread_operand_slice::<I>(env, state, s)?;
                Some(s)
            }
        };
        let tys: Vec<ALocTy> = ts
            .iter()
            .map(|op| spread_operand::<I>(env, state, op))
            .collect::<Result<_, _>>()?;
        let prefix_tys = tys.into_iter().fold(Vec::new(), |acc, t| {
            let mut spread = spread_of_ty(&t);
            spread.reverse();
            spread.extend(acc);
            spread
        });
        mk_spread(&ty, target, prefix_tys, &head_slice_ty)
    }

    fn tuple_spread<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        ty: ALocTy,
        inexact: bool,
        resolved: &flow_data_structure_wrapper::list::FlowOcamlList<
            flow_typing_type::type_::ResolvedParam,
        >,
        unresolved: &flow_data_structure_wrapper::list::FlowOcamlList<
            flow_typing_type::type_::UnresolvedParam,
        >,
    ) -> Result<ALocTy, Error> {
        use flow_typing_type::type_::ArrType;
        use flow_typing_type::type_::ResolvedParam;
        use flow_typing_type::type_::UnresolvedParam;

        let resolved_rev_vec: Vec<&ResolvedParam> = resolved.iter().collect();
        let mut head: Vec<ty::TupleElement<ALoc>> = Vec::new();
        for resolved in resolved_rev_vec.iter().rev() {
            match resolved {
                ResolvedParam::ResolvedArg(box ResolvedArgData(elem, _)) => {
                    let t = type__::<I>(env, state, None, &elem.t)?;
                    head.push(ty::TupleElement::TupleElement {
                        name: elem.name.clone(),
                        t,
                        polarity: type_polarity(elem.polarity),
                        optional: elem.optional,
                    });
                }
                ResolvedParam::ResolvedSpreadArg(box ResolvedSpreadArgData(_, arr, _)) => match arr
                {
                    ArrType::TupleAT(box TupleATData { elements, rest, .. }) => {
                        for elem in elements.iter() {
                            let t = type__::<I>(env, state, None, &elem.t)?;
                            head.push(ty::TupleElement::TupleElement {
                                name: elem.name.clone(),
                                t,
                                polarity: type_polarity(elem.polarity),
                                optional: elem.optional,
                            });
                        }
                        if let Some(rest) = rest {
                            let arr_elt_t = type__::<I>(env, state, None, &rest.t)?;
                            head.push(ty::TupleElement::TupleSpread {
                                name: rest.name.clone(),
                                t: Arc::new(ty::Ty::Arr(ty::ArrT {
                                    arr_readonly: false,
                                    arr_elt_t,
                                })),
                            });
                        }
                    }
                    _ => {
                        let t = arr_ty::<I>(env, state, arr)?;
                        head.push(ty::TupleElement::TupleSpread { name: None, t });
                    }
                },
                ResolvedParam::ResolvedAnySpreadArg(reason, src) => {
                    let t = Arc::new(ty::Ty::Any(any_t(reason, src)));
                    head.push(ty::TupleElement::TupleSpread { name: None, t });
                }
            }
        }

        let spread_ty = ty::TupleElement::TupleSpread { name: None, t: ty };

        let mut tail: Vec<ty::TupleElement<ALoc>> = Vec::new();
        for u in unresolved {
            match u {
                UnresolvedParam::UnresolvedArg(box UnresolvedArgData(elem, _)) => {
                    let t = type__::<I>(env, state, None, &elem.t)?;
                    tail.push(ty::TupleElement::TupleElement {
                        name: elem.name.dupe(),
                        t,
                        polarity: type_polarity(elem.polarity),
                        optional: elem.optional,
                    });
                }
                UnresolvedParam::UnresolvedSpreadArg(t) => {
                    let t = type__::<I>(env, state, None, t)?;
                    tail.push(ty::TupleElement::TupleSpread { name: None, t });
                }
            }
        }

        let mut elements = head;
        elements.push(spread_ty);
        elements.extend(tail);
        Ok(Arc::new(ty::Ty::Tup {
            elements: elements.into(),
            inexact,
        }))
    }

    fn check_component<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        ty: ALocTy,
        pmap: &flow_typing_type::type_::properties::PropertiesMap,
    ) -> Result<ALocTy, Error> {
        let mut map_props = Vec::new();
        for (name, prop) in pmap.iter() {
            let props = obj_prop_t::<I>(env, state, name, prop, false, ty::PropSource::Other)?;
            map_props.extend(props);
        }
        let mut obj_props = spread_of_ty(&ty);
        obj_props.extend(map_props);
        let obj_kind = ty::ObjKind::ExactObj;
        Ok(Arc::new(ty::Ty::Obj(Box::new(ty::ObjT {
            obj_def_loc: None,
            obj_props: obj_props.into(),
            obj_kind,
        }))))
    }

    fn mapped_type<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        source: ALocTy,
        property_type: &Type,
        name_type: Option<&Type>,
        mapped_type_flags: &flow_typing_type::type_::MappedTypeFlags,
        homomorphic: &flow_typing_type::type_::MappedTypeHomomorphicFlag,
    ) -> Result<ALocTy, Error> {
        use flow_typing_type::type_::MappedTypeHomomorphicFlag;
        use flow_typing_type::type_::MappedTypeOptionality;

        let (key_tparam, prop) = match property_type.deref() {
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::PolyT(box PolyTData { tparams, t_out, .. }) if tparams.len() == 1 => {
                    let key_tparam_ty = type_param::<I>(env, state, &tparams[0])?;
                    let property_ty = type__::<I>(env, state, None, t_out)?;
                    (key_tparam_ty, property_ty)
                }
                _ => return Err(terr(ErrorKind::BadMappedType, None, Some(property_type))),
            },
            _ => return Err(terr(ErrorKind::BadMappedType, None, Some(property_type))),
        };

        let name_ty = match name_type {
            None => None,
            Some(t) => match t.deref() {
                TypeInner::DefT(_, def_t)
                    if let DefTInner::PolyT(box PolyTData {
                        tparams,
                        t_out: name_t_out,
                        ..
                    }) = def_t.deref()
                        && tparams.len() == 1 =>
                {
                    let nty = type__::<I>(env, state, None, name_t_out)?;
                    Some(nty)
                }
                _ => {
                    let nty = type__::<I>(env, state, None, t)?;
                    Some(nty)
                }
            },
        };

        let optional = match mapped_type_flags.optional {
            MappedTypeOptionality::MakeOptional => ty::MappedTypeOptionalFlag::MakeOptional,
            MappedTypeOptionality::RemoveOptional => ty::MappedTypeOptionalFlag::RemoveOptional,
            MappedTypeOptionality::KeepOptionality => ty::MappedTypeOptionalFlag::KeepOptionality,
        };

        let variance = match mapped_type_flags.variance {
            flow_typing_type::type_::MappedTypeVariance::OverrideVariance(pol) => {
                ty::MappedTypeVariance::OverrideVariance(type_polarity(pol))
            }
            flow_typing_type::type_::MappedTypeVariance::RemoveVariance(pol) => {
                ty::MappedTypeVariance::RemoveVariance(type_polarity(pol))
            }
            flow_typing_type::type_::MappedTypeVariance::KeepVariance => {
                ty::MappedTypeVariance::KeepVariance
            }
        };
        let flags = ty::MappedTypeFlags { optional, variance };

        let homomorphic_ty = match homomorphic {
            MappedTypeHomomorphicFlag::Homomorphic => ty::MappedTypeHomomorphicFlag::Homomorphic,
            MappedTypeHomomorphicFlag::SemiHomomorphic(t) => {
                let t_ty = type__::<I>(env, state, None, t)?;
                ty::MappedTypeHomomorphicFlag::SemiHomomorphic(t_ty)
            }
            MappedTypeHomomorphicFlag::Unspecialized => {
                ty::MappedTypeHomomorphicFlag::Unspecialized
            }
        };

        let mapped_prop = ty::Prop::MappedTypeProp {
            key_tparam,
            source,
            prop,
            name_type: name_ty,
            flags,
            homomorphic: homomorphic_ty,
        };

        Ok(Arc::new(ty::Ty::Obj(Box::new(ty::ObjT {
            obj_def_loc: None,
            obj_props: Arc::from(vec![mapped_prop]),
            obj_kind: ty::ObjKind::MappedTypeObj,
        }))))
    }

    pub(super) fn type_destructor_unevaluated<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        t: &Type,
        d: &Destructor,
    ) -> Result<ALocTy, Error> {
        use flow_typing_type::type_::Destructor as D;

        let ty = type__::<I>(env, state, None, t)?;

        match d {
            D::NormalizeIntersection { .. } => Ok(ty),
            D::ReactDRO(_) => Ok(ty),
            D::NonMaybeType => Ok(Arc::new(ty::Ty::Utility(ty::Utility::NonMaybeType(ty)))),
            D::ExactType => Ok(Arc::new(ty::Ty::Utility(ty::Utility::Exact(ty)))),
            D::ReadOnlyType => Ok(Arc::new(ty::Ty::Utility(ty::Utility::ReadOnly(ty)))),
            D::EnumType => Ok(Arc::new(ty::Ty::Utility(ty::Utility::Enum(ty)))),
            D::PartialType => Ok(Arc::new(ty::Ty::Utility(ty::Utility::Partial(ty)))),
            D::RequiredType => Ok(Arc::new(ty::Ty::Utility(ty::Utility::Required(ty)))),
            D::ValuesType => Ok(Arc::new(ty::Ty::Utility(ty::Utility::Values(ty)))),
            D::ElementType { index_type } => {
                let index = type__::<I>(env, state, None, index_type)?;
                Ok(Arc::new(ty::Ty::IndexedAccess {
                    _object: ty,
                    index,
                    optional: false,
                }))
            }
            D::OptionalIndexedAccessNonMaybeType { index } => {
                use flow_typing_type::type_::OptionalIndexedAccessIndex;
                let index_ty = match index {
                    OptionalIndexedAccessIndex::OptionalIndexedAccessTypeIndex(t) => {
                        type__::<I>(env, state, None, t)?
                    }
                    OptionalIndexedAccessIndex::OptionalIndexedAccessStrLitIndex(name) => {
                        match name {
                            Name::Symbol(_) => Arc::new(ty::Ty::UniqueSymbol),
                            Name::Str(s) => Arc::new(ty::Ty::StrLit(s.dupe())),
                        }
                    }
                };
                Ok(Arc::new(ty::Ty::IndexedAccess {
                    _object: ty,
                    index: index_ty,
                    optional: true,
                }))
            }
            D::OptionalIndexedAccessResultType { .. } => Ok(ty),
            D::ConditionalType(box DestructorConditionalTypeData {
                infer_tparams,
                extends_t,
                true_t,
                false_t,
                ..
            }) => {
                let infer_tparam_ids = Rc::new(
                    infer_tparams
                        .iter()
                        .map(|tp| env.cx.make_aloc_id(tp.reason.loc()))
                        .collect(),
                );
                conditional_type::<I>(
                    env,
                    state,
                    ConditionalTypeInput {
                        check_type: ty,
                        infer_tparam_ids,
                        extends_t,
                        true_t,
                        false_t,
                    },
                )
            }
            D::TypeMap(TypeMap::ObjectKeyMirror) => {
                Ok(Arc::new(ty::Ty::Utility(ty::Utility::ObjKeyMirror(ty))))
            }
            D::PropertyType { name } => {
                let index = match name {
                    Name::Symbol(_) => Arc::new(ty::Ty::UniqueSymbol),
                    Name::Str(s) => Arc::new(ty::Ty::StrLit(s.dupe())),
                };
                Ok(Arc::new(ty::Ty::IndexedAccess {
                    _object: ty,
                    index,
                    optional: false,
                }))
            }
            D::RestType(flow_typing_type::type_::object::rest::MergeMode::Omit, t_prime) => {
                let ty_prime = type__::<I>(env, state, None, t_prime)?;
                Ok(Arc::new(ty::Ty::Utility(ty::Utility::Omit(ty, ty_prime))))
            }
            D::SpreadType(box DestructorSpreadTypeData(target, operands, head_slice)) => {
                spread::<I>(env, state, ty, target, operands, head_slice.as_ref())
            }
            D::SpreadTupleType(box DestructorSpreadTupleTypeData {
                inexact,
                resolved,
                unresolved,
                ..
            }) => tuple_spread::<I>(env, state, ty, *inexact, resolved, unresolved),
            D::ReactCheckComponentConfig { props, .. } => {
                check_component::<I>(env, state, ty, props)
            }
            D::ReactElementConfigType => Ok(Arc::new(ty::Ty::Utility(
                ty::Utility::ReactElementConfigType(ty),
            ))),
            D::RestType(_, _) => {
                let msg = flow_typing_debug::string_of_destructor(d);
                Err(terr(ErrorKind::BadEvalT, Some(&msg), None))
            }
            D::MappedType(box DestructorMappedTypeData {
                property_type,
                name_type,
                mapped_type_flags,
                homomorphic,
                distributive_tparam_name,
            }) => {
                let (property_type, name_type, homomorphic) =
                    flow_typing_flow_common::flow_js_utils::substitute_mapped_type_distributive_tparams(
                        env.cx,
                        Some(flow_typing_type::type_::unknown_use()),
                        distributive_tparam_name.clone(),
                        property_type.dupe(),
                        name_type.clone(),
                        homomorphic.clone(),
                        t.dupe(),
                    );
                mapped_type::<I>(
                    env,
                    state,
                    ty,
                    &property_type,
                    name_type.as_ref(),
                    mapped_type_flags,
                    &homomorphic,
                )
            }
        }
    }

    fn type_ctor_<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        id: Option<IdKey>,
        t: &Type,
    ) -> Result<ALocTy, Error> {
        type_ctor::<I>(env, state, id, type_ctor_::<I>, t)
    }

    pub(super) fn convert_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        skip_reason: bool,
        t: &Type,
    ) -> Result<ALocTy, Error> {
        if skip_reason {
            let old_tvar_ids = std::mem::replace(&mut env.seen_tvar_ids, Env::empty_tvar_ids());
            let old_eval_ids = std::mem::replace(&mut env.seen_eval_ids, Env::empty_eval_ids());
            let result = type_ctor_::<I>(env, state, None, t);
            env.seen_tvar_ids = old_tvar_ids;
            env.seen_eval_ids = old_eval_ids;
            result
        } else {
            type__::<I>(env, state, None, t)
        }
    }

    pub(super) fn convert_type_params_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        tparams: &[flow_typing_type::type_::TypeParam],
    ) -> Result<Option<Vec<ty::TypeParam<ALoc>>>, Error> {
        let old_tvar_ids = std::mem::replace(&mut env.seen_tvar_ids, Env::empty_tvar_ids());
        let old_eval_ids = std::mem::replace(&mut env.seen_eval_ids, Env::empty_eval_ids());
        let result = type_params_t::<I>(env, state, tparams);
        env.seen_tvar_ids = old_tvar_ids;
        env.seen_eval_ids = old_eval_ids;
        result
    }

    pub(super) fn convert_inline_interface<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        super_: &Type,
        own_props: flow_typing_type::type_::properties::Id,
        proto_props: Option<flow_typing_type::type_::properties::Id>,
        inst_call_t: Option<i32>,
        inst_construct_t: Option<i32>,
        inst_dict: &flow_typing_type::type_::object::Dict,
    ) -> Result<ALocTy, Error> {
        let old_tvar_ids = std::mem::replace(&mut env.seen_tvar_ids, Env::empty_tvar_ids());
        let old_eval_ids = std::mem::replace(&mut env.seen_eval_ids, Env::empty_eval_ids());
        let result = inline_interface::<I>(
            env,
            state,
            super_,
            own_props,
            proto_props,
            inst_call_t,
            inst_construct_t,
            inst_dict,
        );
        env.seen_tvar_ids = old_tvar_ids;
        env.seen_eval_ids = old_eval_ids;
        result
    }

    pub(super) fn convert_obj_props_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        props_id: flow_typing_type::type_::properties::Id,
        call_id_opt: Option<i32>,
        inherited: bool,
        source: ty::PropSource,
        allowed_prop_names: Option<&[Name]>,
    ) -> Result<Vec<ty::Prop<ALoc>>, Error> {
        let old_tvar_ids = std::mem::replace(&mut env.seen_tvar_ids, Env::empty_tvar_ids());
        let old_eval_ids = std::mem::replace(&mut env.seen_eval_ids, Env::empty_eval_ids());
        let result = obj_props_t::<I>(
            env,
            state,
            props_id,
            call_id_opt,
            inherited,
            source,
            allowed_prop_names,
        );
        env.seen_tvar_ids = old_tvar_ids;
        env.seen_eval_ids = old_eval_ids;
        result
    }

    pub(super) fn convert_obj_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        reason: &Reason,
        o: &flow_typing_type::type_::ObjType,
        inherited: bool,
        source: ty::PropSource,
        allowed_prop_names: Option<&[Name]>,
    ) -> Result<ty::ObjT<ALoc>, Error> {
        let old_tvar_ids = std::mem::replace(&mut env.seen_tvar_ids, Env::empty_tvar_ids());
        let old_eval_ids = std::mem::replace(&mut env.seen_eval_ids, Env::empty_eval_ids());
        let result = obj_ty::<I>(env, state, reason, o, inherited, source, allowed_prop_names);
        env.seen_tvar_ids = old_tvar_ids;
        env.seen_eval_ids = old_eval_ids;
        result
    }

    pub(super) fn convert_type_destructor_unevaluated<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        t: &Type,
        d: &Destructor,
    ) -> Result<ALocTy, Error> {
        let old_tvar_ids = std::mem::replace(&mut env.seen_tvar_ids, Env::empty_tvar_ids());
        let old_eval_ids = std::mem::replace(&mut env.seen_eval_ids, Env::empty_eval_ids());
        let result = type_destructor_unevaluated::<I>(env, state, t, d);
        env.seen_tvar_ids = old_tvar_ids;
        env.seen_eval_ids = old_eval_ids;
        result
    }
}

pub mod element_converter {

    use super::*;

    // An opaque type is a TypeAlias marked opaque. Its underlying type, when
    // available, is the alias body; its bounds are kept apart from the body.
    fn nominal_type_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        reason: &Reason,
        nominal_type: &NominalType,
        tparams: Option<Vec<ty::TypeParam<ALoc>>>,
    ) -> Result<ty::Decl<ALoc>, Error> {
        let name_str = match &nominal_type.nominal_id {
            nominal::Id::UserDefinedOpaqueTypeId(box nominal::UserDefinedOpaqueTypeIdData(
                _,
                name,
            )) => name.dupe(),
            // The following cases should error, for the same reason as in nominal_t
            nominal::Id::InternalEnforceUnionOptimized => {
                return Err(terr(
                    ErrorKind::UnsupportedTypeCtor,
                    Some("Special InternalEnforceUnionOptimized upper bound"),
                    None,
                ));
            }
            // The intrinsic has no declaration.
            nominal::Id::ExactObjectType => {
                return Err(terr(
                    ErrorKind::UnsupportedTypeCtor,
                    Some("Intrinsic ExactObjectType"),
                    None,
                ));
            }
            nominal::Id::StuckEval(_) => {
                // If we are under the mode when we never evaluate EvalT, then we will never get here.
                // When we do evaluate EvalT, there is no good way to show that the EvalT is stuck,
                // so it's better to error, so that only the unevaluated form is shown.
                return Err(terr(
                    ErrorKind::UnsupportedTypeCtor,
                    Some("Stuck EvalT"),
                    None,
                ));
            }
        };

        let current_source = env.cx.file().dupe();
        let opaque_source = reason.def_loc().source();
        let name = symbol_from_reason(env, reason, name_str.dupe())?;
        // Compare the current file (of the query) and the file that the opaque
        // type is defined. If they differ, then hide the underlying type and the
        // bounds.
        let same_file = Some(&current_source) == opaque_source;
        let impl_t: Option<&Type> = match &nominal_type.underlying_t {
            // opaque type A = number;
            nominal::UnderlyingT::OpaqueWithLocal { t } if same_file => Some(t),
            nominal::UnderlyingT::CustomError(box nominal::CustomErrorData { t, .. }) => Some(t),
            _ => None,
        };
        // declare opaque type B extends number;
        // TODO: This will potentially report a remote name.
        // The same fix for T25963804 should be applied here as well.
        let (lower_t, upper_t) = if same_file {
            (nominal_type.lower_t.as_ref(), nominal_type.upper_t.as_ref())
        } else {
            (None, None)
        };

        let mut convert = |t: Option<&Type>| match t {
            Some(t) => type_converter::convert_t::<I>(env, state, false, t).map(Some),
            None => Ok(None),
        };
        let type_ = convert(impl_t)?;
        let lower = convert(lower_t)?;
        let upper = convert(upper_t)?;
        Ok(ty::Decl::TypeAliasDecl(Box::new(
            ty::DeclTypeAliasDeclData {
                import: false,
                name,
                tparams: tparams.map(Into::into),
                type_,
                opaque: Some(ty::OpaqueBounds { lower, upper }),
            },
        )))
    }

    fn type_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        reason: &Reason,
        kind: &flow_typing_type::type_::TypeTKind,
        t: &Type,
        tparams: Option<Vec<ty::TypeParam<ALoc>>>,
    ) -> Result<ALocElt, Error> {
        use flow_typing_type::type_::TypeTKind;

        fn local<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            reason: &Reason,
            t: &Type,
            tparams: Option<Vec<ty::TypeParam<ALoc>>>,
        ) -> Result<ALocElt, Error> {
            match reason_utils::local_type_alias_symbol_opt(env, reason)? {
                Some(name) => {
                    let ty = type_converter::convert_t::<I>(env, state, true, t)?;
                    Ok(ty::Elt::Decl(ty::Decl::TypeAliasDecl(Box::new(
                        ty::DeclTypeAliasDeclData {
                            import: false,
                            name,
                            tparams: tparams.map(Into::into),
                            type_: Some(ty),
                            opaque: None,
                        },
                    ))))
                }
                None => {
                    // The reason carries no alias name — e.g. a TS enum member used as a
                    // type (`Color.Red`), a [TypeT] wrapping the member's literal. Render
                    // the wrapped type directly (e.g. `1`) instead of failing.
                    let ty = type_converter::convert_t::<I>(env, state, false, t)?;
                    Ok(ty::Elt::Type(ty))
                }
            }
        }

        fn import<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            reason: &Reason,
            t: &Type,
            tparams: Option<Vec<ty::TypeParam<ALoc>>>,
        ) -> Result<ALocElt, Error> {
            let name = reason_utils::imported_type_alias_symbol(env, reason)?;
            let ty = type_converter::convert_t::<I>(env, state, false, t)?;
            Ok(ty::Elt::Decl(ty::Decl::TypeAliasDecl(Box::new(
                ty::DeclTypeAliasDeclData {
                    import: true,
                    name,
                    tparams: tparams.map(Into::into),
                    type_: Some(ty),
                    opaque: None,
                },
            ))))
        }

        fn class_<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            t: &Type,
        ) -> Result<ALocElt, Error> {
            use flow_typing_type::type_::DefT as FlowDefT;
            use flow_typing_type::type_::DefTInner as FlowDefTInner;
            use flow_typing_type::type_::TypeInner as FlowTypeInner;
            let ct = Type::new(FlowTypeInner::DefT(
                type_util::reason_of_t(t).clone(),
                FlowDefT::new(FlowDefTInner::ClassT(t.dupe())),
            ));
            let c = type_converter::convert_t::<I>(env, state, false, &ct)?;
            Ok(ty::Elt::Type(c))
        }

        fn opaque<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            t: &Type,
            tparams: Option<Vec<ty::TypeParam<ALoc>>>,
        ) -> Result<ALocElt, Error> {
            use std::ops::Deref;

            use flow_typing_type::type_::TypeInner;
            match t.deref() {
                TypeInner::NominalT {
                    reason,
                    nominal_type,
                } => {
                    let decl = nominal_type_t::<I>(env, state, reason, nominal_type, tparams)?;
                    Ok(ty::Elt::Decl(decl))
                }
                _ => Err(terr(ErrorKind::BadTypeAlias, Some("opaque"), Some(t))),
            }
        }

        match kind {
            TypeTKind::TypeAliasKind => local::<I>(env, state, reason, t, tparams),
            TypeTKind::ImportClassKind => class_::<I>(env, state, t),
            TypeTKind::ImportEnumKind => Err(terr(
                ErrorKind::UnexpectedTypeCtor("EnumObjectT"),
                None,
                None,
            )),
            TypeTKind::ImportTypeofKind => import::<I>(env, state, reason, t, tparams),
            TypeTKind::OpaqueKind => opaque::<I>(env, state, t, tparams),
            TypeTKind::TypeParamKind => {
                let desc = reason.desc(true);
                match desc {
                    Some(flow_common::reason::VirtualReasonDesc::RType(name)) => {
                        let loc = reason.def_loc().clone();
                        Ok(ty::Elt::Type(Arc::new(ty::Ty::Bound(Box::new((
                            loc,
                            name.to_string(),
                        ))))))
                    }
                    _ => {
                        let msg = format!(
                            "type param: {}",
                            desc.map(flow_common::reason::string_of_desc)
                                .unwrap_or_default()
                        );
                        Err(terr(ErrorKind::BadTypeAlias, Some(&msg), Some(t)))
                    }
                }
            }
            TypeTKind::InstanceKind => {
                Err(terr(ErrorKind::BadTypeAlias, Some("instance"), Some(t)))
            }
            TypeTKind::RenderTypeKind => {
                Err(terr(ErrorKind::BadTypeAlias, Some("render type"), Some(t)))
            }
        }
    }

    /// The normalizer input, Type.t, is a rather flat structure. It encompasses types
    /// that expressions might have (e.g. number, string, object), but also types that
    /// represent declarations (e.g. class and type alias declarations). This representation
    /// makes it harder to enforce invariants that intuitively should exist. E.g.
    ///
    /// - Type alias, class declaration types, etc. do not nest.
    ///
    /// - Type aliases, class declarations and modules are toplevel or parts of modules.
    ///
    /// To restore these, we trap Type.t constructors that should only appear at the
    /// toplevel, like modules, type aliases, etc.
    pub fn toplevel<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        t: &Type,
    ) -> Result<ALocElt, Error> {
        fn class_or_interface_decl<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            r: &Reason,
            tparams: Option<&[flow_typing_type::type_::TypeParam]>,
            super_: &Type,
            inst: &flow_typing_type::type_::InstType,
        ) -> Result<ALocElt, Error> {
            use flow_typing_type::type_::InstanceKind;

            let ps = match tparams {
                Some(tparams) => type_converter::convert_type_params_t::<I>(env, state, tparams)?,
                None => None,
            };
            let inst_kind = &inst.inst_kind;
            let own_props = inst.own_props.dupe();
            let inst_call_t = inst.inst_call_t;
            let inst_construct_t = inst.inst_construct_t;
            let inst_dict = &inst.inst_dict;

            match inst_kind {
                InstanceKind::InterfaceKind { inline: false, .. } => {
                    let symbol = reason_utils::instance_symbol(env, r, inst)?;
                    Ok(ty::Elt::Decl(ty::Decl::InterfaceDecl(Box::new((
                        symbol,
                        ps.map(Into::into),
                    )))))
                }
                InstanceKind::InterfaceKind { inline: true, .. } => {
                    let ty = type_converter::convert_inline_interface::<I>(
                        env,
                        state,
                        super_,
                        own_props,
                        inline_interface_proto_props(inst),
                        inst_call_t,
                        inst_construct_t,
                        inst_dict,
                    )?;
                    Ok(ty::Elt::Type(ty))
                }
                InstanceKind::ClassKind => {
                    let symbol = reason_utils::instance_symbol(env, r, inst)?;
                    Ok(ty::Elt::Decl(ty::Decl::ClassDecl(Box::new((
                        symbol,
                        ps.map(Into::into),
                    )))))
                }
                InstanceKind::RecordKind { .. } => {
                    let symbol = reason_utils::instance_symbol(env, r, inst)?;
                    Ok(ty::Elt::Decl(ty::Decl::RecordDecl(Box::new((
                        symbol,
                        ps.map(Into::into),
                    )))))
                }
            }
        }

        fn component_decl<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            targs: Option<&[Type]>,
            tparams: Option<&[flow_typing_type::type_::TypeParam]>,
            config: &Type,
            renders: &Type,
            name: &FlowSmolStr,
            reason: &Reason,
        ) -> Result<ALocElt, Error> {
            let tparams = match tparams {
                Some(tparams) => type_converter::convert_type_params_t::<I>(env, state, tparams)?,
                None => None,
            };
            let targs: Option<Vec<ALocTy>> = match targs {
                Some(targs) => {
                    let mut result = Vec::new();
                    for targ in targs {
                        result.push(type_converter::convert_t::<I>(env, state, false, targ)?);
                    }
                    Some(result)
                }
                None => None,
            };
            let is_type = env.toplevel_is_type_identifier_reference();
            let (props, renders) =
                type_converter::convert_component::<I>(env, state, config, renders, !is_type)?;
            Ok(ty::Elt::Decl(ty::Decl::NominalComponentDecl(Box::new(
                ty::DeclNominalComponentDeclData {
                    name: reason_utils::component_symbol(env, name, reason)?,
                    tparams: tparams.map(Into::into),
                    targs: targs.map(Into::into),
                    props,
                    renders: renders.map(|t| t.as_ref().clone()),
                    is_type,
                },
            ))))
        }

        fn enum_decl<'cx>(
            env: &mut Env<'_, 'cx>,
            reason: &Reason,
            enum_info_opt: Option<&std::rc::Rc<flow_typing_type::type_::EnumInfo>>,
        ) -> Result<ALocElt, Error> {
            let symbol = reason_utils::local_type_alias_symbol(env, reason)?;
            let (members, has_unknown_members, truncated_members_count) =
                if env.expand_enum_members() {
                    let max_enum_members = 50;
                    match enum_info_opt {
                        Some(enum_info) => match &***enum_info {
                            EnumInfoInner::ConcreteEnum(concrete) => {
                                let mut all_members: Vec<_> =
                                    concrete.members.keys().cloned().collect();
                                all_members.sort();
                                let num_members = all_members.len();
                                if num_members > max_enum_members {
                                    let truncated_members_count = num_members - max_enum_members;
                                    all_members.truncate(max_enum_members);
                                    (
                                        Some(all_members.into()),
                                        concrete.has_unknown_members,
                                        truncated_members_count as i64,
                                    )
                                } else {
                                    (Some(all_members.into()), concrete.has_unknown_members, 0)
                                }
                            }
                            _ => (None, false, 0),
                        },
                        None => (None, false, 0),
                    }
                } else {
                    (None, false, 0)
                };
            Ok(ty::Elt::Decl(ty::Decl::EnumDecl(Box::new(
                ty::DeclEnumDeclData {
                    name: symbol,
                    members,
                    has_unknown_members,
                    truncated_members_count,
                },
            ))))
        }

        fn singleton_poly<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            orig_t: &Type,
            tparams: &[flow_typing_type::type_::TypeParam],
            t_out: &Type,
        ) -> Result<ALocElt, Error> {
            use std::ops::Deref;

            use flow_typing_type::type_::DefTInner;
            use flow_typing_type::type_::TypeInner;
            use flow_typing_type::type_::TypeTKind;

            match t_out.deref() {
                TypeInner::DefT(_, def_t) => match &**def_t {
                    // Imported interfaces
                    DefTInner::TypeT(TypeTKind::ImportClassKind, inner_t) => {
                        match inner_t.deref() {
                            TypeInner::ThisInstanceT(box ThisInstanceTData {
                                reason: r,
                                instance: inst_t,
                                ..
                            }) => {
                                return class_or_interface_decl::<I>(
                                    env,
                                    state,
                                    r,
                                    Some(tparams),
                                    &inst_t.super_,
                                    &inst_t.inst,
                                );
                            }
                            TypeInner::DefT(r, inner_def_t) => match &**inner_def_t {
                                DefTInner::InstanceT(inst_t) => {
                                    return class_or_interface_decl::<I>(
                                        env,
                                        state,
                                        r,
                                        Some(tparams),
                                        &inst_t.super_,
                                        &inst_t.inst,
                                    );
                                }
                                _ => {}
                            },
                            // Imported polymorphic interfaces.
                            // canonicalize_imported_type wraps a polymorphic interface as
                            // [TypeT(ImportClassKind, TypeAppT(..))] to mirror the
                            // [ClassT(TypeAppT(..))] wrapping used for classes, so [this]
                            // survives the import. Unwrap to the original PolyT so the
                            // inner instance can be rendered as an interface declaration
                            // rather than [Class<...>].
                            TypeInner::TypeAppT(box TypeAppTData { type_, .. }) => {
                                return toplevel::<I>(env, state, type_);
                            }
                            _ => {}
                        }
                    }
                    // Classes, Interfaces
                    DefTInner::ClassT(inner_t) => {
                        match inner_t.deref() {
                            TypeInner::ThisInstanceT(box ThisInstanceTData {
                                reason: r,
                                instance: inst_t,
                                ..
                            }) => {
                                return class_or_interface_decl::<I>(
                                    env,
                                    state,
                                    r,
                                    Some(tparams),
                                    &inst_t.super_,
                                    &inst_t.inst,
                                );
                            }
                            TypeInner::DefT(r, inner_def_t) => match &**inner_def_t {
                                DefTInner::InstanceT(inst_t) => {
                                    return class_or_interface_decl::<I>(
                                        env,
                                        state,
                                        r,
                                        Some(tparams),
                                        &inst_t.super_,
                                        &inst_t.inst,
                                    );
                                }
                                _ => {}
                            },
                            // See flow_js.rs canonicalize_imported_type, case of PolyT (ThisClassT):
                            // The initial abstraction is wrapper within an abstraction and a type application.
                            // The current case unwraps the abstraction and application to reveal the
                            // initial imported type.
                            TypeInner::TypeAppT(box TypeAppTData { type_, .. }) => {
                                return toplevel::<I>(env, state, type_);
                            }
                            _ => {}
                        }
                    }
                    DefTInner::ReactAbstractComponentT(box ReactAbstractComponentTData {
                        component_kind:
                            flow_typing_type::type_::ComponentKind::Nominal(_, name, targs),
                        config,
                        renders,
                    }) => {
                        let reason = type_util::reason_of_t(t_out);
                        return component_decl::<I>(
                            env,
                            state,
                            targs.as_deref(),
                            Some(tparams),
                            config,
                            renders,
                            name,
                            reason,
                        );
                    }
                    // Type Aliases
                    DefTInner::TypeT(kind, inner_t) => {
                        let reason = type_util::reason_of_t(t_out);
                        let ps = type_converter::convert_type_params_t::<I>(env, state, tparams)?;
                        return type_t::<I>(env, state, reason, kind, inner_t, ps);
                    }
                    _ => {}
                },
                _ => {}
            }
            let ty = type_converter::convert_t::<I>(env, state, false, orig_t)?;
            Ok(ty::Elt::Type(ty))
        }

        fn singleton<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            orig_t: &Type,
            t: &Type,
        ) -> Result<ALocElt, Error> {
            use std::ops::Deref;

            use flow_typing_type::type_::DefTInner;
            use flow_typing_type::type_::TypeInner;
            use flow_typing_type::type_::TypeTKind;

            match t.deref() {
                // Namespaces
                TypeInner::NamespaceT(ns) => {
                    use flow_common::flow_symbol::SymbolKind;
                    match ns.values_type.deref() {
                        TypeInner::DefT(_, values_def_t) => match &**values_def_t {
                            DefTInner::ObjT(obj) => match ns.namespace_symbol.kind() {
                                SymbolKind::SymbolModule if !env.keep_only_namespace_name => {
                                    let (exports, default) =
                                        namespace_t::<I>(env, state, obj, ns.types_tmap.dupe())?;
                                    let name = ty_symbol_from_symbol(env, &ns.namespace_symbol)?;
                                    return Ok(ty::Elt::Decl(ty::Decl::ModuleDecl(Box::new(
                                        ty::DeclModuleDeclData {
                                            name: Some(name),
                                            exports: exports.into(),
                                            default: default.map(|t| t.dupe()),
                                        },
                                    ))));
                                }
                                SymbolKind::SymbolNamespace if !env.keep_only_namespace_name => {
                                    let (exports, _) =
                                        namespace_t::<I>(env, state, obj, ns.types_tmap.dupe())?;
                                    let name = ty_symbol_from_symbol(env, &ns.namespace_symbol)?;
                                    return Ok(ty::Elt::Decl(ty::Decl::NamespaceDecl(Box::new(
                                        ty::DeclNamespaceDeclData {
                                            name: Some(name),
                                            exports: exports.into(),
                                        },
                                    ))));
                                }
                                // A TS enum is a NamespaceT tagged with SymbolEnum. Its meaning as a type
                                // is the union of its member literals, so render it that way (e.g.
                                // `1 | 2`). The member literals are read directly from [types_tmap]
                                // rather than via [namespace_t]: those entries are TypeT-wrapped
                                // singletons carrying the members' own (non-alias) reasons, which the
                                // namespace path cannot turn into type-alias names.
                                SymbolKind::SymbolEnum if !env.keep_only_namespace_name => {
                                    let member_ts =
                                        flow_typing_flow_common::flow_js_utils::ts_enum_member_types(
                                            env.cx,
                                            ns.types_tmap.dupe(),
                                        );
                                    match member_ts.split_first() {
                                        None => {
                                            return Ok(ty::Elt::Type(mk_empty(BotKind::EmptyType)));
                                        }
                                        Some((t0, ts)) => {
                                            let ty0 = type_converter::convert_t::<I>(
                                                env, state, false, t0,
                                            )?;
                                            let mut tys = vec![ty0];
                                            for t in ts {
                                                tys.push(type_converter::convert_t::<I>(
                                                    env, state, false, t,
                                                )?);
                                            }
                                            return Ok(ty::Elt::Type(
                                                ty::mk_union(false, tys)
                                                    .expect("non-empty members yield Some union"),
                                            ));
                                        }
                                    }
                                }
                                _ => {}
                            },
                            _ => {}
                        },
                        _ => {}
                    }

                    let ty = type_converter::convert_t::<I>(env, state, false, orig_t)?;
                    Ok(ty::Elt::Type(ty))
                }
                TypeInner::DefT(r, def_t) => {
                    match &**def_t {
                        // Polymorphic variants - see singleton_poly
                        DefTInner::PolyT(box PolyTData { tparams, t_out, .. }) => {
                            return singleton_poly::<I>(env, state, orig_t, tparams, t_out);
                        }
                        // Monomorphic Classes/Interfaces
                        DefTInner::ClassT(inner_t) => match inner_t.deref() {
                            TypeInner::ThisInstanceT(box ThisInstanceTData {
                                reason: r,
                                instance: inst_t,
                                ..
                            }) => {
                                return class_or_interface_decl::<I>(
                                    env,
                                    state,
                                    r,
                                    None,
                                    &inst_t.super_,
                                    &inst_t.inst,
                                );
                            }
                            TypeInner::DefT(r, inner_def_t) => match &**inner_def_t {
                                DefTInner::InstanceT(inst_t) => {
                                    return class_or_interface_decl::<I>(
                                        env,
                                        state,
                                        r,
                                        None,
                                        &inst_t.super_,
                                        &inst_t.inst,
                                    );
                                }
                                _ => {}
                            },
                            _ => {}
                        },
                        DefTInner::TypeT(
                            kind @ (TypeTKind::InstanceKind | TypeTKind::ImportClassKind),
                            inner_t,
                        ) => match inner_t.deref() {
                            TypeInner::ThisInstanceT(box ThisInstanceTData {
                                reason: inner_r,
                                instance: inst_t,
                                ..
                            }) => {
                                return class_or_interface_decl::<I>(
                                    env,
                                    state,
                                    inner_r,
                                    None,
                                    &inst_t.super_,
                                    &inst_t.inst,
                                );
                            }
                            TypeInner::DefT(inner_r, inner_def_t) => match &**inner_def_t {
                                DefTInner::InstanceT(inst_t) => {
                                    return class_or_interface_decl::<I>(
                                        env,
                                        state,
                                        inner_r,
                                        None,
                                        &inst_t.super_,
                                        &inst_t.inst,
                                    );
                                }
                                _ => {
                                    // Monomorphic Type Aliases
                                    let reason = match kind {
                                        TypeTKind::ImportClassKind => r,
                                        _ => type_util::reason_of_t(inner_t),
                                    };
                                    return type_t::<I>(env, state, reason, kind, inner_t, None);
                                }
                            },
                            _ => {
                                // Monomorphic Type Aliases
                                let reason = match kind {
                                    TypeTKind::ImportClassKind => r,
                                    _ => type_util::reason_of_t(inner_t),
                                };
                                return type_t::<I>(env, state, reason, kind, inner_t, None);
                            }
                        },
                        // Enums
                        DefTInner::EnumObjectT { enum_info, .. }
                            if matches!(
                                enum_info.deref().deref(),
                                EnumInfoInner::ConcreteEnum(_)
                            ) =>
                        {
                            return enum_decl(env, r, Some(enum_info));
                        }
                        DefTInner::ReactAbstractComponentT(box ReactAbstractComponentTData {
                            component_kind:
                                flow_typing_type::type_::ComponentKind::Nominal(_, name, targs),
                            config,
                            renders,
                        }) => {
                            return component_decl::<I>(
                                env,
                                state,
                                targs.as_deref(),
                                None,
                                config,
                                renders,
                                name,
                                r,
                            );
                        }
                        DefTInner::ReactAbstractComponentT(box ReactAbstractComponentTData {
                            component_kind: flow_typing_type::type_::ComponentKind::Structural,
                            config,
                            renders,
                        }) if env.toplevel_is_type_identifier_reference() => {
                            let orig_reason = type_util::reason_of_t(orig_t);
                            match orig_reason.desc(true) {
                                Some(ReasonDesc::RIdentifier(name)) => {
                                    return component_decl::<I>(
                                        env,
                                        state,
                                        None,
                                        None,
                                        config,
                                        renders,
                                        name,
                                        orig_reason,
                                    );
                                }
                                _ => {}
                            }
                        }
                        // Monomorphic Type Aliases
                        DefTInner::TypeT(kind, inner_t) => {
                            // Check for ImportEnumKind + EnumValueT pattern first
                            if matches!(kind, TypeTKind::ImportEnumKind) {
                                if let TypeInner::DefT(enum_reason, inner_def_t) = inner_t.deref() {
                                    if matches!(&**inner_def_t, DefTInner::EnumValueT(_)) {
                                        return enum_decl(env, enum_reason, None);
                                    }
                                }
                            }
                            let reason = match kind {
                                TypeTKind::ImportClassKind => r,
                                _ => type_util::reason_of_t(inner_t),
                            };
                            return type_t::<I>(env, state, reason, kind, inner_t, None);
                        }
                        _ => {}
                    }

                    let ty = type_converter::convert_t::<I>(env, state, false, orig_t)?;
                    Ok(ty::Elt::Type(ty))
                }
                _ => {
                    let ty = type_converter::convert_t::<I>(env, state, false, orig_t)?;
                    Ok(ty::Elt::Type(ty))
                }
            }
        }

        match lookahead::peek(env.cx, t) {
            lookahead::Lookahead::LowerBounds(ref bounds) if bounds.len() == 1 => {
                singleton::<I>(env, state, t, &bounds[0])
            }
            lookahead::Lookahead::Recursive | lookahead::Lookahead::LowerBounds(_) => {
                let ty = type_converter::convert_t::<I>(env, state, false, t)?;
                Ok(ty::Elt::Type(ty))
            }
        }
    }

    pub fn module_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        reason: &Reason,
        export_types: &flow_typing_type::type_::ExportTypes,
    ) -> Result<ty::Decl<ALoc>, Error> {
        use std::ops::Deref;

        use flow_typing_type::type_::DefTInner;
        use flow_typing_type::type_::TypeInner;

        fn from_cjs_export<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            cjs_export: &Option<(Option<ALoc>, Type)>,
        ) -> Result<Option<ALocTy>, Error> {
            match cjs_export {
                None => Ok(None),
                Some((_def_loc, exports)) => match lookahead::peek(env.cx, exports) {
                    lookahead::Lookahead::LowerBounds(ref bounds) if bounds.len() == 1 => {
                        match bounds[0].deref() {
                            TypeInner::DefT(_, def_t) => match &**def_t {
                                DefTInner::ObjT(obj) => {
                                    let (_, default) = module_of_object::<I>(env, state, obj)?;
                                    Ok(default)
                                }
                                _ => {
                                    let t =
                                        type_converter::convert_t::<I>(env, state, false, exports)?;
                                    Ok(Some(t))
                                }
                            },
                            _ => {
                                let t = type_converter::convert_t::<I>(env, state, false, exports)?;
                                Ok(Some(t))
                            }
                        }
                    }
                    lookahead::Lookahead::Recursive | lookahead::Lookahead::LowerBounds(_) => {
                        let t = type_converter::convert_t::<I>(env, state, false, exports)?;
                        Ok(Some(t))
                    }
                },
            }
        }

        fn from_exports_tmap<'cx, I: NormalizerInput>(
            env: &mut Env<'_, 'cx>,
            state: &mut State,
            exports_tmap: &flow_typing_type::type_::exports::T,
        ) -> Result<Vec<ty::Decl<ALoc>>, Error> {
            let mut result = Vec::new();
            for (x, named_symbol) in exports_tmap.iter() {
                let t = &named_symbol.type_;
                match toplevel::<I>(env, state, t)? {
                    ty::Elt::Decl(d) => result.push(d),
                    ty::Elt::Type(t) => {
                        result.push(ty::Decl::VariableDecl(Box::new((x.dupe(), t))))
                    }
                }
            }
            Ok(result)
        }

        let name = reason_utils::module_symbol_opt(env, reason)?;
        let value_exports = env.cx.find_exports(export_types.value_exports_tmap);
        let type_exports = env.cx.find_exports(export_types.type_exports_tmap);
        let exports_tmap = value_exports.union(type_exports);
        let exports = from_exports_tmap::<I>(env, state, &exports_tmap)?;
        let default = from_cjs_export::<I>(env, state, &export_types.cjs_export)?;
        Ok(ty::Decl::ModuleDecl(Box::new(ty::DeclModuleDeclData {
            name,
            exports: exports.into(),
            default: default.map(|t| t.dupe()),
        })))
    }

    fn module_of_object<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        o: &flow_typing_type::type_::ObjType,
    ) -> Result<(Vec<ty::Decl<ALoc>>, Option<ALocTy>), Error> {
        use flow_typing_type::type_::PropertyInner;

        let default_name = Name::new("default");
        let props = env.cx.find_props(o.props_tmap.dupe());
        let mut decls: Vec<ty::Decl<ALoc>> = Vec::new();
        let mut default: Option<ALocTy> = None;

        for (x, prop) in props.iter() {
            match prop.deref() {
                PropertyInner::Field(fd) => match toplevel::<I>(env, state, &fd.type_)? {
                    ty::Elt::Type(ref ty) if *x == default_name => match ty.as_ref() {
                        ty::Ty::Obj(_) => {}
                        _ => {
                            default = Some(ty.dupe());
                        }
                    },
                    ty::Elt::Type(t) => {
                        decls.push(ty::Decl::VariableDecl(Box::new((x.dupe(), t))));
                    }
                    ty::Elt::Decl(d) => {
                        decls.push(d);
                    }
                },
                _ => {
                    return Err(terr(
                        ErrorKind::UnsupportedTypeCtor,
                        Some("module-prop"),
                        None,
                    ));
                }
            }
        }
        Ok((decls, default))
    }

    fn namespace_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        values_type: &flow_typing_type::type_::ObjType,
        types_tmap: flow_typing_type::type_::properties::Id,
    ) -> Result<(Vec<ty::Decl<ALoc>>, Option<ALocTy>), Error> {
        use flow_typing_type::type_::PropertyInner;

        let old_keep_only_namespace_name = env.keep_only_namespace_name;
        env.keep_only_namespace_name = true;

        let (mut exports, default) = module_of_object::<I>(env, state, values_type)?;
        let type_props = env.cx.find_props(types_tmap);
        for (x, prop) in type_props.iter() {
            match prop.deref() {
                PropertyInner::Field(fd) => match toplevel::<I>(env, state, &fd.type_)? {
                    ty::Elt::Type(t) => {
                        exports.push(ty::Decl::VariableDecl(Box::new((x.dupe(), t))));
                    }
                    ty::Elt::Decl(d) => {
                        exports.push(d);
                    }
                },
                _ => {
                    env.keep_only_namespace_name = old_keep_only_namespace_name;
                    return Err(terr(
                        ErrorKind::UnsupportedTypeCtor,
                        Some("namespace-prop"),
                        None,
                    ));
                }
            }
        }

        env.keep_only_namespace_name = old_keep_only_namespace_name;
        Ok((exports, default))
    }

    pub fn convert_toplevel<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        t: &Type,
    ) -> Result<ALocElt, Error> {
        toplevel::<I>(env, state, t)
    }
}

mod expand_members {
    use std::rc::Rc;

    use flow_data_structure_wrapper::ord_map::FlowOrdMap;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_typing_type::type_::EnumInfo;
    use flow_typing_type::type_::EnumInfoInner;
    use flow_typing_type::type_::InstanceKind;

    use super::*;

    // Sets how to expand members upon encountering an InstanceT:
    // - if set to IMStatic then expand the static members
    // - if set to IMUnset or IMInstance then expand the instance members.
    //
    // We distinguish between this being not yet set (IMUnset) and this being explicitly
    // set to instance (IMInstance) for the sake of determining how to update this flag
    // upon a ThisClassT:
    // - if the flag was IMUnset, then we know we want to proceed by setting it to IMStatic
    //   so that the InstanceT within is looked at as a static class.
    // - if the flag was IMInstance then we could be looking at the superclass of another
    //   InstanceT, in which case we want to look at the superclass as an instance.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum InstanceMode {
        IMUnset,
        IMStatic,
        IMInstance,
    }

    fn no_members() -> ALocTy {
        Arc::new(ty::Ty::Obj(Box::new(ty::ObjT {
            obj_def_loc: None,
            obj_kind: ty::ObjKind::ExactObj,
            obj_props: Arc::from([] as [ty::Prop<ALoc>; 0]),
        })))
    }

    fn arr_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        inherited: bool,
        force_instance: bool,
        allowed_prop_names: Option<&[Name]>,
        r: &Reason,
        a: &flow_typing_type::type_::ArrType,
    ) -> Result<ALocTy, Error> {
        let builtin = match a {
            flow_typing_type::type_::ArrType::ArrayAT(box ArrayATData { .. }) => "Array",
            flow_typing_type::type_::ArrType::ROArrayAT(box ROArrayATData { .. })
            | flow_typing_type::type_::ArrType::TupleAT(box TupleATData { .. }) => "ReadonlyArray",
        };
        let t = match a {
            flow_typing_type::type_::ArrType::ArrayAT(box ArrayATData { .. }) => {
                flow_typing_flow_common::flow_js_utils::lookup_builtin_value(
                    env.cx,
                    "Array",
                    r.dupe(),
                )
            }
            flow_typing_type::type_::ArrType::ROArrayAT(box ROArrayATData { .. })
            | flow_typing_type::type_::ArrType::TupleAT(box TupleATData { .. }) => {
                flow_typing_flow_common::flow_js_utils::lookup_builtin_type(
                    env.cx,
                    "ReadonlyArray",
                    r.dupe(),
                )
            }
        };
        type__::<I>(
            env,
            state,
            None,
            inherited,
            ty::PropSource::PrimitiveProto(FlowSmolStr::from(builtin)),
            InstanceMode::IMInstance,
            force_instance,
            allowed_prop_names,
            &t,
        )
    }

    fn member_expand_object<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        inherited: bool,
        source: ty::PropSource,
        force_instance: bool,
        allowed_prop_names: Option<&[Name]>,
        super_: &Type,
        implements: &[Type],
        inst: &flow_typing_type::type_::InstType,
    ) -> Result<ALocTy, Error> {
        let own_props = inst.own_props.dupe();
        let proto_props = inst.proto_props.dupe();
        let own_ty_props = type_converter::convert_obj_props_t::<I>(
            env,
            state,
            own_props,
            None,
            inherited,
            source.clone(),
            allowed_prop_names,
        )?;
        let proto_ty_props = type_converter::convert_obj_props_t::<I>(
            env,
            state,
            proto_props,
            None,
            true,
            source.clone(),
            allowed_prop_names,
        )?;
        let super_ty = type__::<I>(
            env,
            state,
            None,
            true,
            source.clone(),
            InstanceMode::IMInstance,
            force_instance,
            allowed_prop_names,
            super_,
        )?;
        let super_props = vec![ty::Prop::SpreadProp(super_ty)];
        let mut interface_props = Vec::new();
        for t in implements {
            let ty = type__::<I>(
                env,
                state,
                None,
                true,
                ty::PropSource::Interface,
                InstanceMode::IMInstance,
                force_instance,
                allowed_prop_names,
                t,
            )?;
            interface_props.push(ty::Prop::SpreadProp(ty));
        }
        // The order of these props is significant to ty_members which will take the
        // last one in case of name conflicts. They are ordered here by distance in
        // the prototype chain (and interface members last), so, for example,
        // overriding methods will have priority.
        let mut obj_props = interface_props;
        obj_props.extend(super_props);
        obj_props.extend(proto_ty_props);
        obj_props.extend(own_ty_props);
        Ok(Arc::new(ty::Ty::Obj(Box::new(ty::ObjT {
            obj_def_loc: None,
            obj_kind: ty::ObjKind::InexactObj,
            obj_props: obj_props.into(),
        }))))
    }

    fn enum_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        inherited: bool,
        force_instance: bool,
        allowed_prop_names: Option<&[Name]>,
        reason: &Reason,
        enum_info: &Rc<EnumInfo>,
    ) -> Result<ALocTy, Error> {
        let (members, representation_t) = match &***enum_info {
            EnumInfoInner::ConcreteEnum(concrete) => {
                (concrete.members.dupe(), concrete.representation_t.dupe())
            }
            EnumInfoInner::AbstractEnum { representation_t } => {
                (FlowOrdMap::new(), representation_t.dupe())
            }
        };
        let enum_value_t = flow_typing_type::type_::mk_enum_type(reason.dupe(), enum_info.dupe());
        let enum_object_t = Type::new(TypeInner::DefT(
            reason.dupe(),
            flow_typing_type::type_::DefT::new(DefTInner::EnumObjectT {
                enum_value_t: enum_value_t.dupe(),
                enum_info: enum_info.dupe(),
            }),
        ));
        let proto_ty = I::builtin_typeapp(
            env,
            state,
            &mut |env, state, t| {
                type__::<I>(
                    env,
                    state,
                    None,
                    true,
                    ty::PropSource::PrimitiveProto(FlowSmolStr::from("$EnumProto")),
                    InstanceMode::IMUnset,
                    force_instance,
                    allowed_prop_names,
                    &t,
                )
            },
            &mut |env, state, t| {
                type__::<I>(
                    env,
                    state,
                    None,
                    false,
                    ty::PropSource::Other,
                    InstanceMode::IMUnset,
                    force_instance,
                    allowed_prop_names,
                    &t,
                )
            },
            app_on_generic,
            reason.dupe(),
            "$EnumProto",
            &[enum_object_t, enum_value_t.dupe(), representation_t],
        )?;
        let enum_value_ty = type_converter::convert_t::<I>(env, state, false, &enum_value_t)?;
        let members_ty: Vec<ty::Prop<ALoc>> = members
            .iter()
            .map(|(name, loc)| {
                let prop = ty::NamedProp::Field {
                    t: enum_value_ty.clone(),
                    polarity: ty::Polarity::Positive,
                    optional: false,
                };
                ty::Prop::NamedProp {
                    name: Name::new(name.dupe()),
                    prop,
                    inherited,
                    source: ty::PropSource::Other,
                    def_locs: Arc::from(vec![loc.clone()]),
                }
            })
            .collect();
        let mut obj_props = vec![ty::Prop::SpreadProp(proto_ty)];
        obj_props.extend(members_ty);
        Ok(Arc::new(ty::Ty::Obj(Box::new(ty::ObjT {
            obj_def_loc: Some(reason.def_loc().clone()),
            obj_kind: ty::ObjKind::InexactObj,
            obj_props: obj_props.into(),
        }))))
    }

    fn obj_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        inherited: bool,
        source: ty::PropSource,
        imode: InstanceMode,
        force_instance: bool,
        allowed_prop_names: Option<&[Name]>,
        reason: &Reason,
        o: &flow_typing_type::type_::ObjType,
    ) -> Result<ty::ObjT<ALoc>, Error> {
        let obj = type_converter::convert_obj_t::<I>(
            env,
            state,
            reason,
            o,
            inherited,
            source.clone(),
            allowed_prop_names,
        )?;
        let proto = type__::<I>(
            env,
            state,
            None,
            true,
            source,
            imode,
            force_instance,
            allowed_prop_names,
            &o.proto_t,
        )?;
        let extra_props = vec![ty::Prop::SpreadProp(proto)];
        let mut obj_props_vec = obj.obj_props.to_vec();
        obj_props_vec.extend(extra_props);
        Ok(ty::ObjT {
            obj_def_loc: obj.obj_def_loc,
            obj_kind: obj.obj_kind,
            obj_props: obj_props_vec.into(),
        })
    }

    fn primitive<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        force_instance: bool,
        allowed_prop_names: Option<&[Name]>,
        reason: &Reason,
        name: &str,
    ) -> Result<ALocTy, Error> {
        I::builtin_type(
            env,
            state,
            &mut |env, state, t| {
                type__::<I>(
                    env,
                    state,
                    None,
                    true,
                    ty::PropSource::PrimitiveProto(FlowSmolStr::from(name)),
                    InstanceMode::IMUnset,
                    force_instance,
                    allowed_prop_names,
                    &t,
                )
            },
            reason.dupe(),
            name,
        )
    }

    fn instance_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        inherited: bool,
        source: ty::PropSource,
        imode: InstanceMode,
        force_instance: bool,
        allowed_prop_names: Option<&[Name]>,
        _r: &Reason,
        static_: &Type,
        super_: &Type,
        implements: &[Type],
        inst: &flow_typing_type::type_::InstType,
    ) -> Result<ALocTy, Error> {
        let inst_kind = &inst.inst_kind;
        match (inst_kind, imode) {
            (InstanceKind::ClassKind | InstanceKind::RecordKind { .. }, InstanceMode::IMStatic) => {
                type__::<I>(
                    env,
                    state,
                    None,
                    inherited,
                    source,
                    imode,
                    force_instance,
                    allowed_prop_names,
                    static_,
                )
            }
            (
                InstanceKind::ClassKind | InstanceKind::RecordKind { .. },
                InstanceMode::IMUnset | InstanceMode::IMInstance,
            )
            | (InstanceKind::InterfaceKind { .. }, _) => member_expand_object::<I>(
                env,
                state,
                inherited,
                source,
                force_instance,
                allowed_prop_names,
                super_,
                implements,
                inst,
            ),
        }
    }

    fn nominal_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        inherited: bool,
        source: ty::PropSource,
        imode: InstanceMode,
        force_instance: bool,
        allowed_prop_names: Option<&[Name]>,
        r: &Reason,
        nominal_type: &NominalType,
    ) -> Result<ALocTy, Error> {
        let current_source = env.cx.file();
        let opaque_source = r.def_loc().source();
        // Compare the current file (of the query) and the file that the opaque
        //    type is defined. If they differ, then hide the underlying type.
        let same_file = opaque_source == Some(current_source);
        match (&nominal_type.underlying_t, &nominal_type.upper_t) {
            (flow_typing_type::type_::nominal::UnderlyingT::OpaqueWithLocal { t }, _)
                if same_file =>
            {
                type__::<I>(
                    env,
                    state,
                    None,
                    inherited,
                    source,
                    imode,
                    force_instance,
                    allowed_prop_names,
                    t,
                )
            }
            (
                flow_typing_type::type_::nominal::UnderlyingT::CustomError(
                    box flow_typing_type::type_::nominal::CustomErrorData { t, .. },
                ),
                _,
            ) => type__::<I>(
                env,
                state,
                None,
                inherited,
                source,
                imode,
                force_instance,
                allowed_prop_names,
                t,
            ),
            (_, Some(t)) => type__::<I>(
                env,
                state,
                None,
                inherited,
                source,
                imode,
                force_instance,
                allowed_prop_names,
                t,
            ),
            _ => Ok(no_members()),
        }
    }

    fn this_class_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        inherited: bool,
        source: ty::PropSource,
        imode: InstanceMode,
        force_instance: bool,
        allowed_prop_names: Option<&[Name]>,
        t: &Type,
    ) -> Result<ALocTy, Error> {
        match imode {
            InstanceMode::IMUnset if !force_instance => type__::<I>(
                env,
                state,
                None,
                inherited,
                source,
                InstanceMode::IMStatic,
                force_instance,
                allowed_prop_names,
                t,
            ),
            _ => type__::<I>(
                env,
                state,
                None,
                inherited,
                source,
                imode,
                force_instance,
                allowed_prop_names,
                t,
            ),
        }
    }

    // A class extending a value with a construct signature (`extends Map<K, V>`)
    // inherits what the signature returns, the way [ThisSpecializeT] works it
    // out, rather than the members of the value itself.
    fn constructed_instance<'cx>(
        cx: &mut Context<'cx>,
        c: &Type,
        targs: Option<&[Type]>,
    ) -> Option<Type> {
        let c = cx.find_resolved(c)?;
        let TypeInner::DefT(_, def_t) = c.deref() else {
            return None;
        };
        let DefTInner::InstanceT(instance) = def_t.deref() else {
            return None;
        };
        let construct_t = cx.find_call(instance.inst.inst_construct_t?);
        let construct_t = cx.find_resolved(&construct_t)?;
        let construct_t = match construct_t.deref() {
            TypeInner::IntersectionT(_, rep) => {
                let construct_ts = rep
                    .members_iter()
                    .map(|t| cx.find_resolved(t))
                    .collect::<Option<Vec<_>>>()?;
                flow_typing_flow_common::flow_js_utils::construct_ts_for_targs(
                    &construct_ts,
                    targs.map_or(0, |targs| targs.len()),
                )
                .into_iter()
                .chain(construct_ts.first().map(|t| t.dupe()))
                .next()?
            }
            _ => construct_t,
        };
        let fun_t = match construct_t.deref() {
            TypeInner::DefT(_, def_t)
                if let DefTInner::PolyT(box PolyTData { tparams, t_out, .. }) = def_t.deref() =>
            {
                let map = tparams
                    .iter()
                    .zip(targs.unwrap_or_default())
                    .map(|(tparam, targ)| (tparam.name.dupe(), targ.dupe()))
                    .collect();
                let t_out = type_subst::subst(
                    cx,
                    None,
                    true,
                    false,
                    type_subst::Purpose::Normal,
                    &map,
                    t_out.dupe(),
                );
                cx.find_resolved(&t_out)?
            }
            _ => construct_t,
        };
        match fun_t.deref() {
            TypeInner::DefT(_, def_t) if let DefTInner::FunT(_, fun_type) = def_t.deref() => {
                Some(fun_type.return_t.dupe())
            }
            _ => None,
        }
    }

    fn type__<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        id: Option<IdKey>,
        inherited: bool,
        source: ty::PropSource,
        imode: InstanceMode,
        force_instance: bool,
        allowed_prop_names: Option<&[Name]>,
        t: &Type,
    ) -> Result<ALocTy, Error> {
        match t.deref() {
            _ if let Some(node) = type_util::constraint_node_id(t) => {
                let id_ = node.id();
                let (root_id, _) = env.cx.find_constraints(id_);
                if id == Some(IdKey::TVarKey(root_id)) {
                    return Ok(Arc::new(ty::Ty::Bot(BotKind::NoLowerWithUpper(
                        UpperBoundKind::NoUpper,
                    ))));
                }
                if is_rec_id(state, IdKey::TVarKey(root_id)) {
                    return Ok(Arc::new(ty::Ty::Any(ty::AnyKind::Recursive)));
                }
                if env.seen_tvar_ids.contains(&root_id) {
                    add_rec_id(state, IdKey::TVarKey(root_id));
                    return Ok(Arc::new(ty::Ty::Any(ty::AnyKind::Recursive)));
                }
                let old_seen_tvar_ids = env.seen_tvar_ids.dupe();
                env.seen_tvar_ids.insert(root_id);
                let result = type_variable(
                    env,
                    state,
                    &mut |env, state, id, t| {
                        type__::<I>(
                            env,
                            state,
                            id,
                            inherited,
                            source.clone(),
                            imode,
                            force_instance,
                            allowed_prop_names,
                            t,
                        )
                    },
                    id_,
                );
                env.seen_tvar_ids = old_seen_tvar_ids;
                result
            }
            TypeInner::AnnotT(_, inner_t, _) => type__::<I>(
                env,
                state,
                None,
                inherited,
                source,
                imode,
                force_instance,
                allowed_prop_names,
                inner_t,
            ),
            TypeInner::ThisTypeAppT(box ThisTypeAppTData {
                type_: c, targs, ..
            }) => {
                let inherited_t = constructed_instance(env.cx, c, targs.as_deref());
                type__::<I>(
                    env,
                    state,
                    None,
                    inherited,
                    source,
                    imode,
                    force_instance,
                    allowed_prop_names,
                    inherited_t.as_ref().unwrap_or(c),
                )
            }
            TypeInner::DefT(r, def_t) => match def_t.deref() {
                DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. } => {
                    primitive::<I>(env, state, force_instance, allowed_prop_names, r, "Number")
                }
                DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. } => {
                    primitive::<I>(env, state, force_instance, allowed_prop_names, r, "String")
                }
                DefTInner::BoolGeneralT | DefTInner::SingletonBoolT { .. } => {
                    primitive::<I>(env, state, force_instance, allowed_prop_names, r, "Boolean")
                }
                DefTInner::SymbolT | DefTInner::UniqueSymbolT(_) => {
                    primitive::<I>(env, state, force_instance, allowed_prop_names, r, "Symbol")
                }
                DefTInner::EnumValueT(_) => Ok(no_members()),
                DefTInner::ObjT(o) => {
                    let obj = obj_t::<I>(
                        env,
                        state,
                        inherited,
                        source,
                        imode,
                        force_instance,
                        allowed_prop_names,
                        r,
                        o,
                    )?;
                    Ok(Arc::new(ty::Ty::Obj(Box::new(obj))))
                }
                DefTInner::ClassT(class_t) => match class_t.deref() {
                    TypeInner::ThisInstanceT(box ThisInstanceTData {
                        reason: r2,
                        instance: inst_t,
                        ..
                    }) => {
                        let def_t = Type::new(TypeInner::DefT(
                            r2.dupe(),
                            flow_typing_type::type_::DefT::new(DefTInner::InstanceT(Rc::new(
                                inst_t.clone(),
                            ))),
                        ));
                        this_class_t::<I>(
                            env,
                            state,
                            inherited,
                            source,
                            imode,
                            force_instance,
                            allowed_prop_names,
                            &def_t,
                        )
                    }
                    _ => type__::<I>(
                        env,
                        state,
                        None,
                        inherited,
                        source,
                        imode,
                        force_instance,
                        allowed_prop_names,
                        class_t,
                    ),
                },
                DefTInner::ArrT(a) => arr_t::<I>(
                    env,
                    state,
                    inherited,
                    force_instance,
                    allowed_prop_names,
                    r,
                    a,
                ),
                DefTInner::EnumObjectT { enum_info, .. } => enum_t::<I>(
                    env,
                    state,
                    inherited,
                    force_instance,
                    allowed_prop_names,
                    r,
                    enum_info,
                ),
                DefTInner::InstanceT(instance_t_val) => instance_t::<I>(
                    env,
                    state,
                    inherited,
                    source,
                    imode,
                    force_instance,
                    allowed_prop_names,
                    r,
                    &instance_t_val.static_,
                    &instance_t_val.super_,
                    &instance_t_val.implements,
                    &instance_t_val.inst,
                ),
                DefTInner::PolyT(box PolyTData { t_out, .. }) => type__::<I>(
                    env,
                    state,
                    None,
                    inherited,
                    source,
                    imode,
                    force_instance,
                    allowed_prop_names,
                    t_out,
                ),
                DefTInner::FunT(static_, _) => type__::<I>(
                    env,
                    state,
                    None,
                    inherited,
                    source,
                    imode,
                    force_instance,
                    allowed_prop_names,
                    static_,
                ),
                DefTInner::TypeT(_, inner_t) => type__::<I>(
                    env,
                    state,
                    None,
                    inherited,
                    source,
                    imode,
                    force_instance,
                    allowed_prop_names,
                    inner_t,
                ),
                DefTInner::ReactAbstractComponentT(_) => I::builtin_type(
                    env,
                    state,
                    &mut |env, state, t| {
                        type__::<I>(
                            env,
                            state,
                            None,
                            inherited,
                            source.clone(),
                            imode,
                            force_instance,
                            allowed_prop_names,
                            &t,
                        )
                    },
                    r.dupe(),
                    "React$AbstractComponentStatics",
                ),
                _ => type_converter::convert_t::<I>(env, state, false, t),
            },
            TypeInner::ThisInstanceT(box ThisInstanceTData {
                reason: r,
                instance: inst_t,
                ..
            }) => instance_t::<I>(
                env,
                state,
                inherited,
                source,
                imode,
                force_instance,
                allowed_prop_names,
                r,
                &inst_t.static_,
                &inst_t.super_,
                &inst_t.implements,
                &inst_t.inst,
            ),
            TypeInner::ObjProtoT(r) => {
                primitive::<I>(env, state, force_instance, allowed_prop_names, r, "Object")
            }
            TypeInner::FunProtoT(r) => primitive::<I>(
                env,
                state,
                force_instance,
                allowed_prop_names,
                r,
                "Function",
            ),
            TypeInner::MaybeT(_, inner_t) => maybe_t(
                |env, state, id, t| {
                    type__::<I>(
                        env,
                        state,
                        id,
                        inherited,
                        source.clone(),
                        imode,
                        force_instance,
                        allowed_prop_names,
                        t,
                    )
                },
                env,
                state,
                id,
                inner_t,
            ),
            TypeInner::IntersectionT(_, rep) => {
                let types: Vec<Type> = rep.members_iter().map(|t| t.dupe()).collect();
                let max_type_size = env.max_type_size();
                app_intersection(
                    |t, s| {
                        type__::<I>(
                            env,
                            s,
                            id.clone(),
                            inherited,
                            source.clone(),
                            imode,
                            force_instance,
                            allowed_prop_names,
                            t,
                        )
                    },
                    types,
                    state,
                    max_type_size,
                )
            }
            TypeInner::UnionT(_, rep) => {
                let types: Vec<Type> = rep.members_iter().map(|t| t.dupe()).collect();
                let max_type_size = env.max_type_size();
                app_union(
                    false,
                    |t, s| {
                        type__::<I>(
                            env,
                            s,
                            id.clone(),
                            inherited,
                            source.clone(),
                            imode,
                            force_instance,
                            allowed_prop_names,
                            t,
                        )
                    },
                    types,
                    state,
                    max_type_size,
                )
            }
            TypeInner::TypeAppT(box TypeAppTData {
                reason,
                type_: c,
                targs,
                from_value,
                ..
            }) => I::typeapp(
                env,
                state,
                &mut |env, state, t| {
                    type__::<I>(
                        env,
                        state,
                        None,
                        inherited,
                        source.clone(),
                        imode,
                        force_instance,
                        allowed_prop_names,
                        t,
                    )
                },
                &mut |env, state, t| {
                    type__::<I>(
                        env,
                        state,
                        None,
                        false,
                        ty::PropSource::Other,
                        InstanceMode::IMUnset,
                        force_instance,
                        allowed_prop_names,
                        t,
                    )
                },
                app_on_generic,
                *from_value,
                reason.dupe(),
                c.dupe(),
                targs,
            ),
            TypeInner::OptionalT { type_: inner_t, .. } => optional_t(
                |env, state, id, t| {
                    type__::<I>(
                        env,
                        state,
                        id,
                        inherited,
                        source.clone(),
                        imode,
                        force_instance,
                        allowed_prop_names,
                        t,
                    )
                },
                env,
                state,
                id,
                inner_t,
            ),
            TypeInner::EvalT {
                type_: eval_type,
                defer_use_t,
                id: eval_id,
            } => {
                if id == Some(IdKey::EvalKey(eval_id.dupe())) {
                    return Ok(Arc::new(ty::Ty::Bot(BotKind::NoLowerWithUpper(
                        UpperBoundKind::NoUpper,
                    ))));
                }
                if is_rec_id(state, IdKey::EvalKey(eval_id.dupe())) {
                    return Ok(Arc::new(ty::Ty::Any(ty::AnyKind::Recursive)));
                }
                if env.seen_eval_ids.contains(eval_id) {
                    add_rec_id(state, IdKey::EvalKey(eval_id.dupe()));
                    return Ok(Arc::new(ty::Ty::Any(ty::AnyKind::Recursive)));
                }
                let old_seen_eval_ids = env.seen_eval_ids.dupe();
                env.seen_eval_ids.insert(eval_id.dupe());
                let result = eval_t::<I>(
                    env,
                    state,
                    |env, state, id, t| {
                        type__::<I>(
                            env,
                            state,
                            id,
                            inherited,
                            source.clone(),
                            imode,
                            force_instance,
                            allowed_prop_names,
                            t,
                        )
                    },
                    |env, state, _id, t| type_converter::convert_t::<I>(env, state, false, t),
                    type_converter::convert_type_destructor_unevaluated::<I>,
                    true,
                    (eval_type, defer_use_t, eval_id),
                );
                env.seen_eval_ids = old_seen_eval_ids;
                result
            }
            TypeInner::GenericT(box GenericTData { bound, .. }) => type__::<I>(
                env,
                state,
                None,
                inherited,
                source,
                imode,
                force_instance,
                allowed_prop_names,
                bound,
            ),
            TypeInner::NominalT {
                reason: r,
                nominal_type,
            } => nominal_t::<I>(
                env,
                state,
                inherited,
                source,
                imode,
                force_instance,
                allowed_prop_names,
                r,
                nominal_type,
            ),
            _ => type_converter::convert_t::<I>(env, state, false, t),
        }
    }

    pub fn convert_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        t: &Type,
        force_instance: bool,
        allowed_prop_names: Option<&[Name]>,
    ) -> Result<ALocTy, Error> {
        type__::<I>(
            env,
            state,
            None,
            false,
            ty::PropSource::Other,
            InstanceMode::IMUnset,
            force_instance,
            allowed_prop_names,
            t,
        )
    }
}

// A kind of shallow type normalizer that is only concerned with expanding types
// which could contribute literals to a union. All other types immediately yield
// empty. This strong base case allows expansion in cases that might present
// performance issues (e.g., expanding through type aliases) in the standard
// TypeConverter type normalizer.
//
// This is useful for autocomplete based on a type's upper bound.
mod expand_literal_union {
    use super::*;

    pub fn convert_t<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        t: &Type,
    ) -> Result<ALocTy, Error> {
        type__::<I>(env, state, None, t)
    }

    fn type__<'cx, I: NormalizerInput>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        id: Option<IdKey>,
        t: &Type,
    ) -> Result<ALocTy, Error> {
        descend(env, t)?;
        match t.deref() {
            _ if let Some(node) = type_util::constraint_node_id(t) => {
                let (root_id, _) = env.cx.find_constraints(node.id());
                if id == Some(IdKey::TVarKey(root_id)) {
                    return Ok(Arc::new(ty::Ty::Bot(BotKind::NoLowerWithUpper(
                        UpperBoundKind::NoUpper,
                    ))));
                }
                if is_rec_id(state, IdKey::TVarKey(root_id)) {
                    return Ok(Arc::new(ty::Ty::Any(ty::AnyKind::Recursive)));
                }
                if env.seen_tvar_ids.contains(&root_id) {
                    add_rec_id(state, IdKey::TVarKey(root_id));
                    return Ok(Arc::new(ty::Ty::Any(ty::AnyKind::Recursive)));
                }
                let old_seen_tvar_ids = env.seen_tvar_ids.dupe();
                env.seen_tvar_ids.insert(root_id);
                let result = type_variable(
                    env,
                    state,
                    &mut |env, state, id, t| type__::<I>(env, state, id, t),
                    root_id,
                );
                env.seen_tvar_ids = old_seen_tvar_ids;
                result
            }
            TypeInner::AnnotT(_, inner_t, _) => type__::<I>(env, state, id, inner_t),
            TypeInner::UnionT(_, rep) => {
                let types: Vec<Type> = rep.members_iter().map(|t| t.dupe()).collect();
                let max_type_size = env.max_type_size();
                app_union(
                    false,
                    |t, s| type__::<I>(env, s, id.clone(), t),
                    types,
                    state,
                    max_type_size,
                )
            }
            TypeInner::IntersectionT(_, rep) => {
                let types: Vec<Type> = rep.members_iter().map(|t| t.dupe()).collect();
                let max_type_size = env.max_type_size();
                app_intersection(
                    |t, s| type__::<I>(env, s, id.clone(), t),
                    types,
                    state,
                    max_type_size,
                )
            }
            TypeInner::TypeAppT(box TypeAppTData {
                reason,
                type_: c,
                targs,
                from_value,
                ..
            }) => I::typeapp(
                env,
                state,
                &mut |env, state, t| type__::<I>(env, state, None, t),
                &mut |env, state, t| type__::<I>(env, state, None, t),
                app_on_generic,
                *from_value,
                reason.dupe(),
                c.dupe(),
                targs,
            ),
            TypeInner::EvalT {
                type_: eval_type,
                defer_use_t,
                id: eval_id,
            } => {
                if id == Some(IdKey::EvalKey(eval_id.dupe())) {
                    return Ok(Arc::new(ty::Ty::Bot(BotKind::NoLowerWithUpper(
                        UpperBoundKind::NoUpper,
                    ))));
                }
                if is_rec_id(state, IdKey::EvalKey(eval_id.dupe())) {
                    return Ok(Arc::new(ty::Ty::Any(ty::AnyKind::Recursive)));
                }
                if env.seen_eval_ids.contains(eval_id) {
                    add_rec_id(state, IdKey::EvalKey(eval_id.dupe()));
                    return Ok(Arc::new(ty::Ty::Any(ty::AnyKind::Recursive)));
                }
                let old_seen_eval_ids = env.seen_eval_ids.dupe();
                env.seen_eval_ids.insert(eval_id.dupe());
                let result = eval_t::<I>(
                    env,
                    state,
                    |env, state, id, t| type__::<I>(env, state, id, t),
                    |env, state, _id, t| type_converter::convert_t::<I>(env, state, false, t),
                    type_converter::convert_type_destructor_unevaluated::<I>,
                    true,
                    (eval_type, defer_use_t, eval_id),
                );
                env.seen_eval_ids = old_seen_eval_ids;
                result
            }
            TypeInner::MaybeT(_, inner_t) => maybe_t(
                |env, state, id, t| type__::<I>(env, state, id, t),
                env,
                state,
                id,
                inner_t,
            ),
            TypeInner::OptionalT { type_: inner_t, .. } => optional_t(
                |env, state, id, t| type__::<I>(env, state, id, t),
                env,
                state,
                id,
                inner_t,
            ),
            TypeInner::KeysT(reason, inner_t) => {
                keys_t::<I>(type__::<I>, env, state, true, reason, inner_t)
            }
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::SingletonNumT { value, .. } => {
                    Ok(Arc::new(ty::Ty::NumLit(value.1.to_string())))
                }
                DefTInner::SingletonStrT { value, .. } => {
                    Ok(Arc::new(ty::Ty::StrLit(value.dupe())))
                }
                DefTInner::SingletonBoolT { value, .. } => Ok(Arc::new(ty::Ty::BoolLit(*value))),
                DefTInner::BoolGeneralT => Ok(Arc::new(ty::Ty::Bool)),
                DefTInner::NullT => Ok(Arc::new(ty::Ty::Null)),
                _ => Ok(empty_type()),
            },
            _ => Ok(empty_type()),
        }
    }
}

pub struct Normalizer<I: NormalizerInput> {
    _phantom: PhantomData<I>,
}

impl<I: NormalizerInput> Normalizer<I> {
    fn run_type_aux<'cx, T>(
        cx: &mut Context<'cx>,
        genv: &Genv<'_, 'cx>,
        state: &mut State,
        t: &Type,
        f: impl FnOnce(&mut Env<'_, 'cx>, &mut State, &Type) -> Result<T, Error>,
        simpl: impl FnOnce(bool, Option<bool>, T) -> T,
    ) -> Result<T, Error> {
        let mut env = Env::init(cx, genv.clone());
        let result = run(state, |state| f(&mut env, state, t));
        match result {
            Ok(r) if env.optimize_types() => {
                Ok(simpl(env.merge_bot_and_any_kinds(), Some(false), r))
            }
            r => r,
        }
    }

    pub fn run_type<'cx>(
        cx: &mut Context<'cx>,
        genv: &Genv<'_, 'cx>,
        state: &mut State,
        t: &Type,
    ) -> Result<ALocElt, Error> {
        state.reset_type_size_budget(genv.options.max_type_size);
        match Self::run_type_aux(
            cx,
            genv,
            state,
            t,
            element_converter::convert_toplevel::<I>,
            simplify_elt,
        ) {
            Err(error) if error.kind == ErrorKind::TypeSizeLimit => {
                Ok(ALocElt::Type(Arc::new(ty::Ty::Truncated)))
            }
            result => result,
        }
    }

    pub fn run_module_type<'cx>(
        cx: &mut Context<'cx>,
        genv: &Genv<'_, 'cx>,
        state: &mut State,
        module_type: &ModuleType,
    ) -> Result<ALocDecl, Error> {
        let mut env = Env::init(cx, genv.clone());
        let result = run(state, |state| {
            element_converter::module_t::<I>(
                &mut env,
                state,
                &module_type.module_reason,
                &module_type.module_export_types,
            )
        });
        match result {
            Ok(decl) if env.optimize_types() => Ok(flow_common_ty::ty_utils::simplify_decl(
                env.merge_bot_and_any_kinds(),
                Some(false),
                decl,
            )),
            r => r,
        }
    }

    // Before we start normalizing the input type we populate our environment with
    // aliases that are in scope due to typed imports. These appear inside
    // File_sig.module_sig.requires. This step includes the normalization
    // of all imported types and the creation of a map to hold bindings of imported
    // names to location of definition. This map will be used later to determine
    // whether a located name (symbol) appearing is part of the file's imports or a
    // remote (hidden or non-imported) name.

    pub fn normalize_imports<'cx>(
        cx: &mut Context<'cx>,
        file_sig: std::sync::Arc<FileSig>,
        typed_ast_opt: Option<&flow_parser::ast::Program<ALoc, (ALoc, Type)>>,
        options: &Options,
        imports: Vec<(String, ALoc, ImportMode, Type)>,
    ) -> ImportedNamesMap {
        fn def_loc_of_ty(t: &ty::Ty<ALoc>) -> Option<ALoc> {
            match t {
                ty::Ty::Utility(ty::Utility::Class(inner)) => match inner.as_ref() {
                    ty::Ty::Generic(box (sym, _, None)) => Some(sym.sym_def_loc.clone()),
                    _ => None,
                },
                // This is an acceptable proxy only if the class is not polymorphic
                ty::Ty::TypeOf(box (ty::BuiltinOrSymbol::TSymbol(sym), None)) => {
                    Some(sym.sym_def_loc.clone())
                }
                _ => None,
            }
        }

        fn def_loc_of_decl(d: &ty::Decl<ALoc>) -> Option<ALoc> {
            match d {
                ty::Decl::TypeAliasDecl(box ty::DeclTypeAliasDeclData {
                    import: false,
                    name,
                    ..
                }) => Some(name.sym_def_loc.dupe()),
                ty::Decl::ClassDecl(box (sym, _)) => Some(sym.sym_def_loc.dupe()),
                ty::Decl::InterfaceDecl(box (sym, _)) => Some(sym.sym_def_loc.dupe()),
                ty::Decl::RecordDecl(box (sym, _)) => Some(sym.sym_def_loc.dupe()),
                ty::Decl::EnumDecl(box ty::DeclEnumDeclData { name, .. }) => {
                    Some(name.sym_def_loc.dupe())
                }
                ty::Decl::NominalComponentDecl(box ty::DeclNominalComponentDeclData {
                    name,
                    ..
                }) => Some(name.sym_def_loc.dupe()),
                ty::Decl::TypeAliasDecl(box ty::DeclTypeAliasDeclData {
                    import: true,
                    type_: Some(t),
                    ..
                }) => def_loc_of_ty(t),
                ty::Decl::TypeAliasDecl(..)
                | ty::Decl::VariableDecl(..)
                | ty::Decl::ModuleDecl(..)
                | ty::Decl::NamespaceDecl(..) => None,
            }
        }

        fn def_loc_of_elt(elt: &ty::Elt<ALoc>) -> Option<ALoc> {
            match elt {
                ty::Elt::Type(t) => def_loc_of_ty(t),
                ty::Elt::Decl(d) => def_loc_of_decl(d),
            }
        }

        let mut options = options.clone();
        // We shouldn't need to evaluate any destructors for imports.
        options.evaluate_type_destructors = EvaluateTypeDestructorsMode::EvaluateNone;

        // Note: Genv holds references, so we construct it directly
        let genv = Genv {
            typed_ast_opt,
            file_sig,
            imported_names: ImportedNames::new(|_| Ok(ImportedNamesMap::new())),
            options,
            ref_type_bodies: None,
        };

        let mut state = State::empty();
        let mut result = ImportedNamesMap::new();

        fn convert<'cx, I: NormalizerInput>(
            cx: &mut Context<'cx>,
            genv: &Genv<'_, 'cx>,
            state: &mut State,
            t: &Type,
        ) -> Result<Option<ALoc>, Error> {
            let mut env = Env::init(cx, genv.clone());
            let ty = element_converter::convert_toplevel::<I>(&mut env, state, t)?;
            Ok(def_loc_of_elt(&ty))
        }

        for (name, loc, import_mode, t) in imports {
            match convert::<I>(cx, &genv, &mut state, &t) {
                Ok(Some(def_loc)) => {
                    let imported_ident = flow_common_ty::ty_symbol::ImportedIdent(
                        loc,
                        name.to_string(),
                        import_mode,
                    );
                    result.insert(ALocFuzzy(def_loc), imported_ident);
                }
                // unrecognizable remote type - skip
                Ok(None) => {}
                // normalization error - skip
                Err(_) => {}
            }
        }

        result
    }

    pub fn run_expand_members<'cx>(
        force_instance: bool,
        allowed_prop_names: Option<Vec<Name>>,
        cx: &mut Context<'cx>,
        genv: &Genv<'_, 'cx>,
        state: &mut State,
        t: &Type,
    ) -> Result<ALocTy, Error> {
        Self::run_type_aux(
            cx,
            genv,
            state,
            t,
            |env, state, t| {
                expand_members::convert_t::<I>(
                    env,
                    state,
                    t,
                    force_instance,
                    allowed_prop_names.as_deref(),
                )
            },
            flow_common_ty::ty_utils::simplify_type,
        )
    }

    pub fn run_expand_literal_union<'cx>(
        cx: &mut Context<'cx>,
        genv: &Genv<'_, 'cx>,
        state: &mut State,
        t: &Type,
    ) -> Result<ALocTy, Error> {
        Self::run_type_aux(
            cx,
            genv,
            state,
            t,
            expand_literal_union::convert_t::<I>,
            flow_common_ty::ty_utils::simplify_type,
        )
    }
}

pub fn make_normalizer<I: NormalizerInput>() -> Normalizer<I> {
    Normalizer {
        _phantom: PhantomData,
    }
}
