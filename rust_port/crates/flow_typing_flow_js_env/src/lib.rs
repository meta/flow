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
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_type::type_::BoundedId;
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
    // handling here. Object, intersection and template literal types are counted by where
    // they were written, since a type argument like `{v: T}`, `T & string` or `` `a${T}` ``
    // can grow without a nested type application.
    #[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord)]
    pub enum Root {
        Type(Type),
        Array(flow_common::reason::Reason),
        ROArray(flow_common::reason::Reason),
        Tuple(flow_common::reason::Reason, usize),
        Written(flow_common::reason::Reason),
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

/// One entry of the expansion stack, holding the entries below it.
#[derive(Debug)]
struct TypeAppFrame {
    entry: StackEntry,
    enclosing: Option<Rc<TypeAppFrame>>,
}

#[derive(Debug)]
enum StackEntry {
    TypeApp(Entry),
    /// Where the union or object spread with this id started resolving its parts.
    Parts(i32),
    /// Where the bound of a type with this identity, whose type arguments have these roots,
    /// started flowing in its place.
    Bound(BoundedId, Vec<RootSet>),
}

/// One check of a generic value against a type (`PolyT ~> u`), holding the checks it is nested in.
#[derive(Debug)]
struct PolymorphicLowerFrame {
    tparams_loc: ALoc,
    enclosing: Option<Rc<PolymorphicLowerFrame>>,
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
    /// Whether an implicit-instantiation solve is ongoing.
    implicit_instantiation: bool,
    /// Whether placeholders are seen without their bounds, as TypeScript's
    /// restrictive instantiation sees type parameters when it decides that a
    /// conditional type definitely holds.
    restrictive_frozen_tvars: bool,
    /// The bounds of the `infer` types of the conditional type being checked, by the id of the
    /// tvar that stands for each, as template literal types consult them (see
    /// `template_literal_type::flow_matched_placeholder`).
    infer_type_bounds: Option<Rc<BTreeMap<u32, Type>>>,
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
            implicit_instantiation: self.implicit_instantiation,
            restrictive_frozen_tvars: self.restrictive_frozen_tvars,
            infer_type_bounds: self.infer_type_bounds.dupe(),
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
        self.implicit_instantiation
    }

    pub fn solving_implicit_instantiation(&self) -> Self {
        Self {
            implicit_instantiation: true,
            ..self.dupe()
        }
    }

    pub fn with_restrictive_frozen_tvars(&self) -> Self {
        Self {
            restrictive_frozen_tvars: true,
            ..self.dupe()
        }
    }

    /// The same env, but seeing placeholders as `any`, as the check that a conditional type
    /// definitely fails does, even when that conditional type is evaluated inside a restrictive
    /// check.
    pub fn without_restrictive_frozen_tvars(&self) -> Self {
        Self {
            restrictive_frozen_tvars: false,
            ..self.dupe()
        }
    }

    pub fn restrictive_frozen_tvars(&self) -> bool {
        self.restrictive_frozen_tvars
    }

    pub fn with_infer_type_bounds(&self, bounds: BTreeMap<u32, Type>) -> Self {
        Self {
            infer_type_bounds: Some(Rc::new(bounds)),
            ..self.dupe()
        }
    }

    /// The bound of the `infer` type that the tvar `id` stands for, if it is one of the conditional
    /// type being checked.
    pub fn infer_type_bound(&self, id: u32) -> Option<&Type> {
        self.infer_type_bounds.as_ref()?.get(&id)
    }

    /// What a use sees a frozen tvar as.
    pub fn frozen_tvar_bound(&self, tvar: &ImplicitInstantiationTvarData) -> Type {
        if self.restrictive_frozen_tvars {
            mixed_t::make(tvar.reason.dupe())
        } else {
            tvar.bound.dupe()
        }
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
            if let StackEntry::TypeApp(Entry(prev_c, prev_tss, prev_side)) = &frame.entry
                && c == prev_c
                && possibly_expanding_targs(prev_tss, &tss)
                && side == *prev_side
            {
                count += 1;
                if count >= limit {
                    return None;
                }
            }
        }
        Some(Self {
            speculation: self.speculation.dupe(),
            instantiation_stack: Some(Rc::new(TypeAppFrame {
                entry: StackEntry::TypeApp(Entry(c.dupe(), tss, side)),
                enclosing: self.instantiation_stack.dupe(),
            })),
            polymorphic_lower_stack: self.polymorphic_lower_stack.dupe(),
            implicit_instantiation: self.implicit_instantiation,
            restrictive_frozen_tvars: self.restrictive_frozen_tvars,
            infer_type_bounds: self.infer_type_bounds.dupe(),
        })
    }

    pub fn push_parts_marker(&self, id: i32) -> Self {
        Self {
            instantiation_stack: Some(Rc::new(TypeAppFrame {
                entry: StackEntry::Parts(id),
                enclosing: self.instantiation_stack.dupe(),
            })),
            ..self.dupe()
        }
    }

    /// Pops the type applications expanded since `push_parts_marker(id)`.
    pub fn pop_to_parts_marker(&self, id: i32) -> Self {
        let mut stack = &self.instantiation_stack;
        while let Some(frame) = stack
            && !matches!(frame.entry, StackEntry::Parts(marker) if marker == id)
        {
            stack = &frame.enclosing;
        }
        Self {
            instantiation_stack: stack.dupe(),
            ..self.dupe()
        }
    }

    /// Detect whether flowing the bound of the type with identity `bounded_id` in its place would
    /// loop, given the roots `tss` of its type arguments: an opaque type can be a member of its
    /// own upper bound (`D<X>: D<X> | null`). An occurrence counts, like a type application, when
    /// its type arguments are the same or possibly expanding. Returns the env to flow the bound
    /// under, or [`None`] if `limit` such occurrences are already flowing their bounds.
    pub fn push_bound_unless_loop(
        &self,
        limit: i32,
        bounded_id: &BoundedId,
        tss: Vec<RootSet>,
    ) -> Option<Self> {
        let expanding = std::iter::successors(self.instantiation_stack.as_deref(), |frame| {
            frame.enclosing.as_deref()
        });
        let mut count = 0;
        for frame in expanding {
            if let StackEntry::Bound(prev, prev_tss) = &frame.entry
                && prev.same_identity(bounded_id)
                && (prev.same_type_args(bounded_id) || possibly_expanding_targs(prev_tss, &tss))
            {
                count += 1;
                if count >= limit {
                    return None;
                }
            }
        }
        Some(Self {
            instantiation_stack: Some(Rc::new(TypeAppFrame {
                entry: StackEntry::Bound(bounded_id.clone(), tss),
                enclosing: self.instantiation_stack.dupe(),
            })),
            ..self.dupe()
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
            implicit_instantiation: self.implicit_instantiation,
            restrictive_frozen_tvars: false,
            infer_type_bounds: None,
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
