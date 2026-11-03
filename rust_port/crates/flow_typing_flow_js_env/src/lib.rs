/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! The dynamic scope of a `flow_js` solve.
//!
//! Speculation, type-application expansion and implicit instantiation are
//! nested scopes of the constraint solver, not properties of the file being
//! checked, so they are carried in a [`FlowJsEnv`] threaded through `flow_js`
//! rather than in `Context`. The value is immutable: a nested scope derives a
//! new env, so a scope cannot outlive the call that created it and there is
//! nothing to restore on the way out.
//!
//! The scope stops at the edge of the solver. A checker entry point, a lazily
//! forced tvar, a hint evaluation and a module forcing each start their own
//! solve at [`FlowJsEnv::entry`], so code above `flow_js` sees the default env
//! and calls the plain `foo` rather than the `foo_with_env` it wraps. The one
//! way a live scope reaches that code is a closure the solver calls back
//! *inside* the scope — a speculation case and a `flow_js` helper-trait method
//! — which is why those take an env of their own.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::rc::Rc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::subst_name::SubstName;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_type::type_::ImplicitInstantiationTvarData;
use flow_typing_type::type_::SpecState;
use flow_typing_type::type_::SpeculationHintState;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::mixed_t;

#[derive(Debug, Clone)]
pub enum InformationForSynthesisLogging {
    CallInformationForSynthesisLogging {
        lhs_t: Type,
        call_callee_hint_ref: Rc<RefCell<SpeculationHintState>>,
    },
    NoInformationForSynthesisLogging,
}

// Next, a model for "cases." A case serves as the context for a speculative
// match. In other words, while we're trying to execute a flow in speculation
// mode, we use this data structure to record stuff.
//
// A case carries a (local) index that identifies which type we're currently
// considering among the members of a union or intersection type. This is used
// only for error reporting.
#[derive(Debug, Clone)]
pub struct Case {
    pub case_id: i32,
    pub errors: Rc<RefCell<Vec<ErrorMessage<ALoc>>>>,
    pub information_for_synthesis_logging: InformationForSynthesisLogging,
}

/// A branch is a wrapper around a case, that also carries the speculation id of
/// the spec currently being processed.
#[derive(Debug, Clone)]
pub struct Branch {
    pub speculation_id: i32,
    pub case: Case,
}

pub mod type_app_expansion {
    use super::*;

    // Array types function like type applications but are not implemented as such. Unless
    // we decide to unify their implementation with regular typeapps, they need special
    // handling here
    #[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord)]
    pub enum Root {
        Type(Type),
        Array(flow_common::reason::Reason),
        ROArray(flow_common::reason::Reason),
        Tuple(flow_common::reason::Reason, usize),
    }

    pub type RootSet = BTreeSet<Root>;

    #[derive(Debug, Clone, Copy, Dupe, PartialEq, Eq)]
    pub enum Bound {
        Lower,
        Upper,
        /// Specialized for a type destructor. A spread specializes its first operand this way
        /// but its later operands as lower bounds, within the continuation of the first one, so
        /// these expansions are counted apart from the lower bound ones.
        Destructed,
    }

    #[derive(Debug, Clone)]
    pub struct Entry(pub Type, pub Vec<RootSet>, pub Bound);
}

use type_app_expansion::Bound;
use type_app_expansion::Entry;
use type_app_expansion::RootSet;

/// One entered speculation branch, holding the branches it is nested in.
#[derive(Debug)]
struct SpeculationFrame {
    branch: Branch,
    enclosing: Option<Rc<SpeculationFrame>>,
}

/// One type application being expanded, holding the expansions it is nested in.
#[derive(Debug)]
struct TypeAppFrame {
    entry: Entry,
    enclosing: Option<Rc<TypeAppFrame>>,
}

/// One check of a generic value against a type (`PolyT ~> u`), holding the checks it is nested in.
#[derive(Debug)]
struct PolymorphicLowerFrame {
    tparams_loc: ALoc,
    enclosing: Option<Rc<PolymorphicLowerFrame>>,
}

/// How a solve instantiates a generic value that flows to a type mentioning the solve's tvars.
/// The examples use
///
///   declare function id<T>(x: T): T;
///   declare function apply<S>(fn: (S) => S, initial: S): S;
///   apply(id, 1);
#[derive(Debug)]
enum PolymorphicLowers {
    /// The first two runs of `solve_targs`: with placeholder type arguments. The flag records
    /// whether a run did so. In `apply(id, 1)`, the first run gives `id` placeholders. If the
    /// first run never does, it is the answer.
    Placeholders(bool),
    /// The final run of `solve_targs`: checked against the type it flows to, with what the hint
    /// solve pinned the solve's type parameters to, by name. In `apply(id, 1)`, that is
    /// `{S: number}`, and `id` is checked against `(S) => S` read as `(number) => number`.
    Hinted(Rc<BTreeMap<SubstName, Type>>),
    /// A check of a generic value against a type: not at all. The check is only there to infer
    /// the checked value's own type arguments, and the flow of the instantiated value after it
    /// checks the generic values it meets.
    Skipped,
}

/// One implicit-instantiation solve, holding the solves it is nested in.
#[derive(Debug)]
struct ImplicitInstantiationFrame {
    inference_tvars: RefCell<BTreeMap<i32, (Type, bool)>>,
    polymorphic_lowers: RefCell<PolymorphicLowers>,
    enclosing: Option<Rc<ImplicitInstantiationFrame>>,
}

/// The environment for `flow_js` related functions. Callers outside the
/// `flow_js` system use [`FlowJsEnv::entry`]; any non-default env out there
/// must be carefully audited (see the module docs for the two ways one can
/// legitimately get there).
///
/// Deriving an env is on the hot path of every entry into the solver, so the
/// scope stacks are shared with the env derived from rather than copied: only
/// entering a scope allocates, and only the single frame it pushes.
#[derive(Debug, Clone, Dupe, Default)]
pub struct FlowJsEnv {
    /// The innermost speculation branch being tried, or [`None`] when we are
    /// not speculating, in which case errors are reported instead of being
    /// deferred to a branch.
    speculation: Option<Rc<SpeculationFrame>>,
    /// The innermost type application being expanded, used to cut off expansion
    /// that would otherwise diverge.
    instantiation_stack: Option<Rc<TypeAppFrame>>,
    /// The innermost check of a generic value against a type, used to cut off checks that would
    /// otherwise recur through the generic value's own type.
    polymorphic_lower_stack: Option<Rc<PolymorphicLowerFrame>>,
    /// The innermost implicit-instantiation solve.
    implicit_instantiation: Option<Rc<ImplicitInstantiationFrame>>,
    /// Whether frozen tvars, placeholders included, are seen without their
    /// bounds, as TypeScript's restrictive instantiation sees type parameters
    /// when it decides that a conditional type definitely holds.
    restrictive_frozen_tvars: bool,
}

impl FlowJsEnv {
    /// The env at a checker entry point: not speculating, nothing expanding,
    /// not solving type arguments.
    pub fn entry() -> Self {
        Self::default()
    }

    // ---- speculation ----

    pub fn speculating(&self) -> bool {
        self.speculation.is_some()
    }

    /// The enclosing branches, innermost first.
    fn branches(&self) -> impl Iterator<Item = &Branch> + '_ {
        std::iter::successors(self.speculation.as_deref(), |frame| {
            frame.enclosing.as_deref()
        })
        .map(|frame| &frame.branch)
    }

    /// The innermost branch, identified by its speculation and case ids.
    pub fn speculation_id(&self) -> Option<SpecState> {
        self.speculation.as_ref().map(|frame| SpecState {
            speculation_id: frame.branch.speculation_id,
            case_id: frame.branch.case.case_id,
        })
    }

    /// The speculation ids of the enclosing branches, outermost first.
    pub fn speculation_id_path(&self) -> impl Iterator<Item = i32> + '_ {
        let mut path: Vec<i32> = self
            .branches()
            .map(|branch| branch.speculation_id)
            .collect();
        path.reverse();
        path.into_iter()
    }

    /// The (speculation id, case id) of every enclosing branch, innermost
    /// first. An edge recorded under any of these is already on the path to the
    /// current branch, so callers ask whether one is there rather than where.
    pub fn speculation_path(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.branches()
            .map(|branch| (branch.speculation_id, branch.case.case_id))
    }

    /// Record an error against the innermost branch, to be replayed if that
    /// branch turns out to be the one that wins. A no-op when not speculating.
    pub fn defer_error(&self, msg: ErrorMessage<ALoc>) {
        if let Some(frame) = &self.speculation {
            frame.branch.case.errors.borrow_mut().push(msg);
        }
    }

    /// How many errors the innermost branch has deferred.
    pub fn deferred_error_count(&self) -> usize {
        self.speculation
            .as_ref()
            .map_or(0, |frame| frame.branch.case.errors.borrow().len())
    }

    /// Drop the errors that the innermost branch deferred after the first `count`. To throw away
    /// the errors of a computation that gets redone, take `count` from `deferred_error_count`
    /// before it and call this after it.
    pub fn truncate_deferred_errors(&self, count: usize) {
        if let Some(frame) = &self.speculation {
            frame.branch.case.errors.borrow_mut().truncate(count);
        }
    }

    /// Enter `branch`. What the branch expands is its own business and is
    /// discarded along with the branch, so expansion starts over.
    pub fn with_branch(&self, branch: Branch) -> Self {
        Self {
            speculation: Some(Rc::new(SpeculationFrame {
                branch,
                enclosing: self.speculation.dupe(),
            })),
            instantiation_stack: self.instantiation_stack.dupe(),
            polymorphic_lower_stack: self.polymorphic_lower_stack.dupe(),
            implicit_instantiation: self.implicit_instantiation.dupe(),
            restrictive_frozen_tvars: self.restrictive_frozen_tvars,
        }
    }

    /// The same env, but not speculating. Use it for a computation whose errors are thrown away,
    /// like a hint. Inside a speculation branch, an error fails the branch or is deferred to it,
    /// so even a thrown-away error would affect the branch.
    pub fn without_speculation(&self) -> Self {
        Self {
            speculation: None,
            ..self.dupe()
        }
    }

    // ---- implicit instantiation ----

    pub fn in_implicit_instantiation(&self) -> bool {
        self.implicit_instantiation.is_some()
    }

    pub fn solving_implicit_instantiation(&self) -> Self {
        self.push_implicit_instantiation_frame(PolymorphicLowers::Placeholders(false))
    }

    /// Enter a solve that checks a generic value against a type, like `id` against
    /// `(string) => string`, to infer the generic value's own type arguments.
    pub fn checking_polymorphic_lower(&self) -> Self {
        self.push_implicit_instantiation_frame(PolymorphicLowers::Skipped)
    }

    fn push_implicit_instantiation_frame(&self, polymorphic_lowers: PolymorphicLowers) -> Self {
        Self {
            implicit_instantiation: Some(Rc::new(ImplicitInstantiationFrame {
                inference_tvars: RefCell::new(BTreeMap::new()),
                polymorphic_lowers: RefCell::new(polymorphic_lowers),
                enclosing: self.implicit_instantiation.dupe(),
            })),
            ..self.dupe()
        }
    }

    /// The innermost solve that has tvars of its own. A frame without any is an evaluation inside
    /// a solve, like a conditional type without `infer`, and it belongs to the solve around it.
    fn innermost_solve(&self) -> Option<&Rc<ImplicitInstantiationFrame>> {
        std::iter::successors(self.implicit_instantiation.as_ref(), |frame| {
            frame.enclosing.as_ref()
        })
        .find(|frame| !frame.inference_tvars.borrow().is_empty())
    }

    /// Whether the innermost solve checks a generic value against a type (see
    /// `checking_polymorphic_lower`).
    pub fn in_polymorphic_lower_check(&self) -> bool {
        self.innermost_solve().is_some_and(|frame| {
            matches!(
                *frame.polymorphic_lowers.borrow(),
                PolymorphicLowers::Skipped
            )
        })
    }

    pub fn with_restrictive_frozen_tvars(&self) -> Self {
        Self {
            restrictive_frozen_tvars: true,
            ..self.dupe()
        }
    }

    /// The same env, but seeing frozen tvars with their bounds and placeholders as `any`, as the
    /// check that a conditional type definitely fails does, even when that conditional type is
    /// evaluated inside a restrictive check.
    pub fn without_restrictive_frozen_tvars(&self) -> Self {
        Self {
            restrictive_frozen_tvars: false,
            ..self.dupe()
        }
    }

    pub fn restrictive_frozen_tvars(&self) -> bool {
        self.restrictive_frozen_tvars
    }

    /// What a use sees a frozen tvar as.
    pub fn frozen_tvar_bound(&self, tvar: &ImplicitInstantiationTvarData) -> Type {
        if self.restrictive_frozen_tvars {
            mixed_t::make(tvar.reason.dupe())
        } else {
            tvar.bound.dupe()
        }
    }

    pub fn add_implicit_instantiation_tvar(&self, id: i32, tvar: Type, has_default: bool) {
        if let Some(frame) = &self.implicit_instantiation {
            frame
                .inference_tvars
                .borrow_mut()
                .insert(id, (tvar, has_default));
        }
    }

    /// The tvars of the innermost solve, like `T` and `U` while solving a call of
    /// `applyTo<T, U>(f: (T) => U, x: T): U`.
    pub fn implicit_instantiation_tvars(&self) -> Vec<Type> {
        self.implicit_instantiation
            .as_ref()
            .map(|frame| {
                frame
                    .inference_tvars
                    .borrow()
                    .values()
                    .map(|(tvar, _)| tvar.dupe())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Set what the hint solve of the innermost solve pinned its type parameters to, for its final
    /// run.
    pub fn set_polymorphic_lower_hints(&self, hints: BTreeMap<SubstName, Type>) {
        if let Some(frame) = &self.implicit_instantiation {
            *frame.polymorphic_lowers.borrow_mut() = PolymorphicLowers::Hinted(Rc::new(hints));
        }
    }

    /// Whether a run of the innermost solve gave a generic value placeholder type arguments.
    pub fn gave_polymorphic_lower_placeholders(&self) -> bool {
        self.implicit_instantiation.as_ref().is_some_and(|frame| {
            matches!(
                *frame.polymorphic_lowers.borrow(),
                PolymorphicLowers::Placeholders(true)
            )
        })
    }

    /// Called when a solve meets a generic value flowing to a type. `mentioned` says which tvars
    /// the generic value or the type mention. Returns the env of the solve that decides how to
    /// instantiate the generic value, with that solve's hints to check it against, or `None` if
    /// the generic value gets placeholders instead.
    ///
    /// Only the innermost solve decides, and only if one of its own tvars is mentioned. With
    ///
    ///   declare function id<T>(x: T): T;
    ///   declare function apply<S>(fn: (S) => S, initial: S): S;
    ///   apply(id, 1);
    ///
    /// `id` flows to `(S) => S`, which mentions `apply`'s `S`, so the solve of `apply` decides how
    /// to instantiate `id`. If only tvars of outer solves are mentioned, this returns `None`: the
    /// innermost solve can't decide, and the outer solve meets the generic value again when it
    /// re-runs.
    ///
    /// The deciding solve has no hints in its first two runs. Then this returns `None` and records
    /// that the solve gave placeholders.
    pub fn polymorphic_lower_hints(
        &self,
        mut mentioned: impl FnMut(i32) -> bool,
    ) -> Option<(Self, Rc<BTreeMap<SubstName, Type>>)> {
        let frame = self.innermost_solve()?;
        if !frame
            .inference_tvars
            .borrow()
            .keys()
            .any(|id| mentioned(*id))
        {
            return None;
        }
        let mut polymorphic_lowers = frame.polymorphic_lowers.borrow_mut();
        match &*polymorphic_lowers {
            PolymorphicLowers::Hinted(hints) => Some((
                Self {
                    implicit_instantiation: Some(frame.dupe()),
                    ..self.dupe()
                },
                hints.dupe(),
            )),
            PolymorphicLowers::Placeholders(_) => {
                *polymorphic_lowers = PolymorphicLowers::Placeholders(true);
                None
            }
            PolymorphicLowers::Skipped => None,
        }
    }

    /// The tvar owned by a lower frame that satisfies `predicate`, if any.
    pub fn frozen_implicit_instantiation_tvar(
        &self,
        predicate: impl FnMut(i32) -> bool,
    ) -> Option<Type> {
        self.frozen_implicit_instantiation_tvar_with_default(predicate)
            .map(|(tvar, _)| tvar)
    }

    pub fn frozen_implicit_instantiation_tvar_with_default(
        &self,
        mut predicate: impl FnMut(i32) -> bool,
    ) -> Option<(Type, bool)> {
        std::iter::successors(
            self.implicit_instantiation
                .as_deref()
                .and_then(|frame| frame.enclosing.as_deref()),
            |frame| frame.enclosing.as_deref(),
        )
        .find_map(|frame| {
            frame
                .inference_tvars
                .borrow()
                .iter()
                .find_map(|(id, (tvar, has_default))| {
                    predicate(*id).then(|| (tvar.dupe(), *has_default))
                })
        })
    }

    /// The env of the enclosing solve that owns the tvar satisfying
    /// `predicate`, where that tvar is active.
    pub fn implicit_instantiation_owner(
        &self,
        mut predicate: impl FnMut(i32) -> bool,
    ) -> Option<Self> {
        let owner = std::iter::successors(
            self.implicit_instantiation
                .as_ref()
                .and_then(|frame| frame.enclosing.as_ref()),
            |frame| frame.enclosing.as_ref(),
        )
        .find(|frame| {
            frame
                .inference_tvars
                .borrow()
                .keys()
                .any(|id| predicate(*id))
        })?;
        Some(Self {
            implicit_instantiation: Some(owner.dupe()),
            ..self.dupe()
        })
    }

    // ---- type application expansion ----

    /// Detect whether expanding `c<ts>` would loop, given the roots `tss` of
    /// `ts`. Returns the env to expand under, or [`None`] if `limit`
    /// possibly-expanding occurrences of `c` are already on the stack.
    pub fn push_typeapp_unless_loop(
        &self,
        limit: i32,
        side: Bound,
        c: &Type,
        tss: Vec<RootSet>,
    ) -> Option<Self> {
        let expanding = std::iter::successors(self.instantiation_stack.as_deref(), |frame| {
            frame.enclosing.as_deref()
        });
        let mut count = 0;
        for frame in expanding {
            let Entry(prev_c, prev_tss, prev_side) = &frame.entry;
            if c == prev_c && possibly_expanding_targs(prev_tss, &tss) && side == *prev_side {
                count += 1;
                if count >= limit {
                    return None;
                }
            }
        }
        Some(Self {
            speculation: self.speculation.dupe(),
            instantiation_stack: Some(Rc::new(TypeAppFrame {
                entry: Entry(c.dupe(), tss, side),
                enclosing: self.instantiation_stack.dupe(),
            })),
            polymorphic_lower_stack: self.polymorphic_lower_stack.dupe(),
            implicit_instantiation: self.implicit_instantiation.dupe(),
            restrictive_frozen_tvars: self.restrictive_frozen_tvars,
        })
    }

    /// Detect whether checking the generic value declared at `tparams_loc` against a type would
    /// loop. Returns the env to check under, or [`None`] if `limit` checks of a generic value
    /// declared there are already on the stack. With
    ///
    ///   declare class Thenable<T> {
    ///     then<U>(f: (T) => U): Thenable<U>;
    ///   }
    ///
    /// checking a `Thenable<number>` against the `Promise` interface checks `then` against
    /// `Promise`'s `then`, whose return type is a `Thenable` checked against a `Promise` again.
    pub fn push_polymorphic_lower_unless_loop(
        &self,
        limit: i32,
        tparams_loc: &ALoc,
    ) -> Option<Self> {
        let checking = std::iter::successors(self.polymorphic_lower_stack.as_deref(), |frame| {
            frame.enclosing.as_deref()
        });
        let mut count = 0;
        for frame in checking {
            if frame.tparams_loc == *tparams_loc {
                count += 1;
                if count >= limit {
                    return None;
                }
            }
        }
        Some(Self {
            polymorphic_lower_stack: Some(Rc::new(PolymorphicLowerFrame {
                tparams_loc: tparams_loc.dupe(),
                enclosing: self.polymorphic_lower_stack.dupe(),
            })),
            ..self.dupe()
        })
    }

    /// Force a signature tvar's thunk: neither the enclosing speculation nor
    /// the enclosing expansion applies to it.
    pub fn signature_tvar_env(&self) -> Self {
        Self {
            speculation: None,
            instantiation_stack: None,
            polymorphic_lower_stack: None,
            implicit_instantiation: self.implicit_instantiation.dupe(),
            restrictive_frozen_tvars: false,
        }
    }
}

// Say that targs are possibly expanding when, given previous targs and
// current targs, each previously non-empty targ is contained in the
// corresponding current targ.
fn possibly_expanding_targs(prev_tss: &[RootSet], tss: &[RootSet]) -> bool {
    let mut seen_nonempty_prev_ts = false;
    let mut prev_iter = prev_tss.iter();
    let mut curr_iter = tss.iter();

    loop {
        match (prev_iter.next(), curr_iter.next()) {
            (Some(prev_ts), Some(ts)) => {
                // if prev_ts is not a subset of ts, we have found a counterexample
                // and we can bail out
                if !prev_ts.is_subset(ts) {
                    return false;
                }
                // otherwise, we recurse on the remaining targs, updating the bit
                seen_nonempty_prev_ts = seen_nonempty_prev_ts || !prev_ts.is_empty();
            }
            // we have found no counterexamples, so it comes down to whether we've
            // seen any non-empty prev_ts
            (None, None) => return seen_nonempty_prev_ts,
            // something's wrong around arities, but that's not our problem, so bail out
            _ => return false,
        }
    }
}
