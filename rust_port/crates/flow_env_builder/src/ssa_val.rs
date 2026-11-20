/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cmp;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use dupe::Dupe;
use flow_analysis::bindings::Kind as BindingsKind;
use flow_common::reason::VirtualReason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::reason::mk_reason;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::loc_sig::LocSig;

use crate::env_api::Read;
use crate::env_api::ValKind;
use crate::env_api::WriteLoc;

static CURR_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum WriteStateInner<L: LocSig> {
    Uninitialized(L),
    Undeclared(FlowSmolStr, L),
    DeclaredButSkipped(FlowSmolStr, L),
    Projection(L),
    FunctionThis(VirtualReason<L>),
    GlobalThis(VirtualReason<L>),
    IllegalThis(VirtualReason<L>),
    ClassInstanceThis(VirtualReason<L>),
    ClassStaticThis(VirtualReason<L>),
    ClassInstanceSuper(VirtualReason<L>),
    ClassStaticSuper(VirtualReason<L>),
    ModuleScoped(FlowSmolStr),
    Global(FlowSmolStr),
    Loc(VirtualReason<L>),
    EmptyArray {
        reason: VirtualReason<L>,
        arr_providers: FlowOrdSet<L>,
    },
    IllegalWrite(VirtualReason<L>),
    PHI(Vec<WriteStateInner<L>>),
    Refinement {
        refinement_id: usize,
        val_t: Box<Val<L>>,
    },
    Undefined(VirtualReason<L>),
    Number(VirtualReason<L>),
    DeclaredFunction(L),
}

// Mirrors OCaml's `Hashtbl.hash`: bounded depth/breadth so hashing huge PHI
// trees built up by env_builder (e.g. switches with hundreds of cases, or
// large generated code stays O(1) instead of blowing up to O(tree_size).
// Equality is still structural, so cache correctness is preserved.
const HASH_BOUND: usize = 8;

impl<L: LocSig + std::hash::Hash> std::hash::Hash for WriteStateInner<L> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            WriteStateInner::Uninitialized(l)
            | WriteStateInner::Projection(l)
            | WriteStateInner::DeclaredFunction(l) => l.hash(state),
            WriteStateInner::Undeclared(s, l) | WriteStateInner::DeclaredButSkipped(s, l) => {
                s.hash(state);
                l.hash(state);
            }
            WriteStateInner::FunctionThis(r)
            | WriteStateInner::GlobalThis(r)
            | WriteStateInner::IllegalThis(r)
            | WriteStateInner::ClassInstanceThis(r)
            | WriteStateInner::ClassStaticThis(r)
            | WriteStateInner::ClassInstanceSuper(r)
            | WriteStateInner::ClassStaticSuper(r)
            | WriteStateInner::Loc(r)
            | WriteStateInner::IllegalWrite(r)
            | WriteStateInner::Undefined(r)
            | WriteStateInner::Number(r) => r.hash(state),
            WriteStateInner::ModuleScoped(s) | WriteStateInner::Global(s) => s.hash(state),
            WriteStateInner::EmptyArray {
                reason,
                arr_providers,
            } => {
                reason.hash(state);
                arr_providers.len().hash(state);
                for p in arr_providers.iter().take(HASH_BOUND) {
                    p.hash(state);
                }
            }
            WriteStateInner::PHI(ts) => {
                ts.len().hash(state);
                for t in ts.iter().take(HASH_BOUND) {
                    t.hash(state);
                }
            }
            WriteStateInner::Refinement {
                refinement_id,
                val_t,
            } => {
                refinement_id.hash(state);
                // Each Val has a unique id assigned by ValCache, so the id alone
                // distinguishes Vals; skipping val_t.write_state avoids a deep
                // recursive walk through nested Refinement/PHI chains.
                val_t.id.hash(state);
            }
        }
    }
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WriteState<L: LocSig>(pub Rc<WriteStateInner<L>>);

impl<L: LocSig> WriteState<L> {
    pub fn new(inner: WriteStateInner<L>) -> Self {
        WriteState(Rc::new(inner))
    }

    pub fn inner(&self) -> &WriteStateInner<L> {
        &self.0
    }
}

/// For every read of a variable x, we are interested in tracking writes to x
/// that can reach that read. Ultimately the writes are going to be represented
/// as a list of locations, where each location corresponds to a "single static
/// assignment" of the variable in the code. But for the purposes of analysis, it
/// is useful to represent these writes with a data type that contains either a
/// single write, or a "join" of writes (in compiler terminology, a PHI node), or
/// a reference to something that is unknown at a particular point in the AST
/// during traversal, but will be known by the time traversal is complete.
#[derive(Debug, Clone, Dupe)]
pub struct Val<L: LocSig> {
    pub id: usize,
    pub write_state: WriteState<L>,
}

// Each Val gets a fresh id, so two Vals with the same id have the same write state. Comparing ids
// alone therefore gives the same answers as comparing ids and then write states, without walking
// the write states: they share parts (a refinement points at the value it refines), so a walk can
// visit the same part exponentially many times.
impl<L: LocSig> PartialEq for Val<L> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<L: LocSig> Eq for Val<L> {}

impl<L: LocSig> PartialOrd for Val<L> {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: LocSig> Ord for Val<L> {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

impl<L: LocSig> std::hash::Hash for Val<L> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // ValCache enforces id <-> write_state bijection, so id alone is enough
        // for hashing. Avoids walking `write_state`'s recursive structure.
        self.id.hash(state);
    }
}

// Ensure we only produce one unique val for the same Loc
#[derive(Debug, Clone, Default)]
pub struct ValCache<L: LocSig>(HashMap<WriteState<L>, Val<L>>);

impl<L: LocSig> ValCache<L> {
    pub fn new() -> Self {
        ValCache(HashMap::new())
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

fn debug_write_state<L: LocSig, F>(get_refi: &F, state: &WriteStateInner<L>) -> String
where
    F: Fn(usize) -> String,
{
    match state {
        WriteStateInner::Uninitialized(_) => "(uninitialized)".to_string(),
        WriteStateInner::Undeclared(_, _) => "(undeclared)".to_string(),
        WriteStateInner::DeclaredButSkipped(_, _) => "(declared but skipped)".to_string(),
        WriteStateInner::Projection(_) => "projection".to_string(),
        WriteStateInner::Loc(_) => "write".to_string(),
        WriteStateInner::EmptyArray { .. } => "(empty array)".to_string(),
        WriteStateInner::IllegalWrite(_) => "illegal write".to_string(),
        WriteStateInner::Refinement {
            refinement_id,
            val_t,
        } => {
            let refinement_kind = get_refi(*refinement_id);
            let write_str = debug_to_string(get_refi, val_t);
            format!(
                "{{refinement = {}; write = {}}}",
                refinement_kind, write_str
            )
        }
        WriteStateInner::FunctionThis(_) => "This(function)".to_string(),
        WriteStateInner::GlobalThis(_) => "This(global)".to_string(),
        WriteStateInner::IllegalThis(_) => "This(illegal)".to_string(),
        WriteStateInner::ClassInstanceThis(_) => "This(instance)".to_string(),
        WriteStateInner::ClassStaticThis(_) => "This(static)".to_string(),
        WriteStateInner::ClassInstanceSuper(_) => "Super(instance)".to_string(),
        WriteStateInner::ClassStaticSuper(_) => "Super(static)".to_string(),
        WriteStateInner::ModuleScoped(name) => format!("ModuleScoped {}", name),
        WriteStateInner::Global(name) => format!("Global {}", name),
        WriteStateInner::Undefined(_) => "undefined".to_string(),
        WriteStateInner::Number(_) => "number".to_string(),
        WriteStateInner::DeclaredFunction(_) => "declared function".to_string(),
        WriteStateInner::PHI(ts) => {
            let strs: Vec<_> = ts.iter().map(|t| debug_write_state(get_refi, t)).collect();
            format!("[{}]", strs.join(","))
        }
    }
}

pub fn debug_to_string<L: LocSig, F>(get_refi: &F, t: &Val<L>) -> String
where
    F: Fn(usize) -> String,
{
    format!(
        "{} {}",
        t.id,
        debug_write_state(get_refi, t.write_state.inner())
    )
}

pub fn is_global_undefined<L: LocSig>(t: &Val<L>) -> bool {
    fn loop_<L: LocSig>(state: &WriteStateInner<L>) -> bool {
        match state {
            WriteStateInner::Global(name) => name.as_str() == "undefined",
            WriteStateInner::Refinement { val_t, .. } => loop_(val_t.write_state.inner()),
            WriteStateInner::PHI(ts) => !ts.is_empty() && ts.iter().all(loop_),
            _ => false,
        }
    }
    loop_(t.write_state.inner())
}

pub fn is_global<L: LocSig>(t: &Val<L>) -> bool {
    matches!(t.write_state.inner(), WriteStateInner::Global(_))
}

pub fn is_undeclared<L: LocSig>(t: &Val<L>) -> bool {
    matches!(t.write_state.inner(), WriteStateInner::Undeclared(_, _))
}

pub fn is_undeclared_or_skipped<L: LocSig>(t: &Val<L>) -> bool {
    matches!(
        t.write_state.inner(),
        WriteStateInner::Undeclared(_, _) | WriteStateInner::DeclaredButSkipped(_, _)
    )
}

pub fn is_declared_function<L: LocSig>(t: &Val<L>) -> bool {
    matches!(t.write_state.inner(), WriteStateInner::DeclaredFunction(_))
}

pub fn contains_bare_projection<L: LocSig>(t: &Val<L>) -> bool {
    fn loop_<L: LocSig>(state: &WriteStateInner<L>) -> bool {
        match state {
            WriteStateInner::Projection(_) => true,
            WriteStateInner::PHI(ts) => ts.iter().any(loop_),
            _ => false,
        }
    }
    loop_(t.write_state.inner())
}

fn new_id() -> usize {
    CURR_ID.fetch_add(1, Ordering::SeqCst)
}

fn mk_with_write_state<L: LocSig>(
    cache: &mut ValCache<L>,
    write_state_inner: WriteStateInner<L>,
) -> Val<L> {
    let write_state = WriteState::new(write_state_inner);
    if let Some(v) = cache.0.get(&write_state) {
        return v.dupe();
    }
    let id = new_id();
    let v = Val {
        id,
        write_state: write_state.dupe(),
    };
    cache.0.insert(write_state, v.dupe());
    v
}

pub fn of_write<L: LocSig>(cache: &mut ValCache<L>, write_state: WriteStateInner<L>) -> Val<L> {
    mk_with_write_state(cache, write_state)
}

pub fn empty<L: LocSig>(cache: &mut ValCache<L>) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::PHI(Vec::new()))
}

pub fn uninitialized<L: LocSig>(cache: &mut ValCache<L>, r: L) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::Uninitialized(r))
}

pub fn undefined<L: LocSig>(cache: &mut ValCache<L>, r: VirtualReason<L>) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::Undefined(r))
}

pub fn empty_array<L: LocSig>(
    cache: &mut ValCache<L>,
    reason: VirtualReason<L>,
    arr_providers: FlowOrdSet<L>,
) -> Val<L> {
    mk_with_write_state(
        cache,
        WriteStateInner::EmptyArray {
            reason,
            arr_providers,
        },
    )
}

pub fn number<L: LocSig>(cache: &mut ValCache<L>, r: VirtualReason<L>) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::Number(r))
}

pub fn projection<L: LocSig>(cache: &mut ValCache<L>, loc: L) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::Projection(loc))
}

pub fn declared_function<L: LocSig>(cache: &mut ValCache<L>, loc: L) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::DeclaredFunction(loc))
}

pub fn refinement<L: LocSig>(
    cache: &mut ValCache<L>,
    refinement_id: usize,
    val_t: Val<L>,
) -> Val<L> {
    mk_with_write_state(
        cache,
        WriteStateInner::Refinement {
            refinement_id,
            val_t: Box::new(val_t),
        },
    )
}

pub fn undeclared<L: LocSig>(cache: &mut ValCache<L>, name: FlowSmolStr, def_loc: L) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::Undeclared(name, def_loc))
}

pub fn declared_but_skipped<L: LocSig>(
    cache: &mut ValCache<L>,
    name: FlowSmolStr,
    def_loc: L,
) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::DeclaredButSkipped(name, def_loc))
}

fn unrefine_deeply_write_state_inner<L: LocSig>(
    cache: &mut ValCache<L>,
    id: usize,
    write_state: &WriteStateInner<L>,
) -> WriteStateInner<L> {
    match write_state {
        WriteStateInner::Refinement {
            refinement_id,
            val_t,
        } if *refinement_id == id => {
            unrefine_deeply_write_state_inner(cache, id, val_t.write_state.inner())
        }
        WriteStateInner::Refinement {
            refinement_id,
            val_t,
        } => {
            let val_t_new = unrefine_deeply(cache, id, (*val_t).dupe());
            if val_t_new.id == val_t.id {
                WriteStateInner::Refinement {
                    refinement_id: *refinement_id,
                    val_t: val_t.clone(),
                }
            } else {
                WriteStateInner::Refinement {
                    refinement_id: *refinement_id,
                    val_t: Box::new(val_t_new),
                }
            }
        }
        WriteStateInner::PHI(ts) => {
            let ts_new: Vec<_> = ts
                .iter()
                .map(|t| unrefine_deeply_write_state_inner(cache, id, t))
                .collect();
            if ts_new == *ts {
                write_state.clone()
            } else {
                WriteStateInner::PHI(ts_new)
            }
        }
        _ => write_state.clone(),
    }
}

pub fn unrefine_deeply<L: LocSig>(cache: &mut ValCache<L>, id: usize, t: Val<L>) -> Val<L> {
    let state_new = unrefine_deeply_write_state_inner(cache, id, t.write_state.inner());
    if state_new == *t.write_state.inner() {
        t
    } else {
        mk_with_write_state(cache, state_new)
    }
}

pub fn base_id_of_val<L: LocSig>(t: &Val<L>) -> usize {
    match t.write_state.inner() {
        WriteStateInner::Refinement { val_t, .. } => base_id_of_val(val_t),
        _ => t.id,
    }
}

pub fn unrefine<L: LocSig>(id: usize, t: Val<L>) -> Val<L> {
    match t.write_state.inner() {
        WriteStateInner::Refinement {
            refinement_id,
            val_t,
        } if *refinement_id == id => (**val_t).dupe(),
        _ => t,
    }
}

pub fn replace_refinement_base_write<L: LocSig>(
    cache: &mut ValCache<L>,
    base: Val<L>,
    t: Val<L>,
) -> Val<L> {
    match t.write_state.inner() {
        WriteStateInner::Refinement { refinement_id, .. } => {
            refinement(cache, *refinement_id, base)
        }
        _ => base,
    }
}

fn join_write_states<L: LocSig>(states: Vec<WriteStateInner<L>>) -> WriteStateInner<L> {
    match states.len() {
        0 => WriteStateInner::PHI(Vec::new()),
        1 => states.into_iter().next().unwrap(),
        _ => WriteStateInner::PHI(states),
    }
}

pub fn one<L: LocSig>(cache: &mut ValCache<L>, reason: VirtualReason<L>) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::Loc(reason))
}

pub fn illegal_write<L: LocSig>(cache: &mut ValCache<L>, reason: VirtualReason<L>) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::IllegalWrite(reason))
}

fn join<L: LocSig>(cache: &mut ValCache<L>, write_states: Vec<WriteStateInner<L>>) -> Val<L> {
    match join_write_states(write_states) {
        WriteStateInner::Loc(reason) => one(cache, reason),
        write_state => mk_with_write_state(cache, write_state),
    }
}

fn normalize<L: LocSig>(t: &WriteStateInner<L>) -> BTreeSet<WriteStateInner<L>> {
    match t {
        WriteStateInner::Uninitialized(_)
        | WriteStateInner::Undefined(_)
        | WriteStateInner::Number(_)
        | WriteStateInner::DeclaredFunction(_)
        | WriteStateInner::Undeclared(_, _)
        | WriteStateInner::DeclaredButSkipped(_, _)
        | WriteStateInner::Projection(_)
        | WriteStateInner::FunctionThis(_)
        | WriteStateInner::GlobalThis(_)
        | WriteStateInner::IllegalThis(_)
        | WriteStateInner::ClassInstanceThis(_)
        | WriteStateInner::ClassStaticThis(_)
        | WriteStateInner::ClassInstanceSuper(_)
        | WriteStateInner::ClassStaticSuper(_)
        | WriteStateInner::ModuleScoped(_)
        | WriteStateInner::Global(_)
        | WriteStateInner::Loc(_)
        | WriteStateInner::EmptyArray { .. }
        | WriteStateInner::IllegalWrite(_)
        | WriteStateInner::Refinement { .. } => {
            let mut set = BTreeSet::new();
            set.insert(t.clone());
            set
        }
        WriteStateInner::PHI(ts) => ts.iter().flat_map(normalize).collect(),
    }
}

pub fn merge<L: LocSig>(cache: &mut ValCache<L>, t1: Val<L>, t2: Val<L>) -> Val<L> {
    // Merging can easily lead to exponential blowup in size of terms if we're not careful. We
    // amortize costs by computing normal forms as sets of "atomic" terms, so that merging would
    // correspond to set union. (Atomic terms include `Uninitialized`, `Loc`, and unresolved refs.)
    // Note that normal forms might change over time, as unresolved refs become
    // resolved; thus, we do not shortcut normalization of previously normalized terms. Still, we
    // expect (and have experimentally validated that) the cost of computing normal forms becomes
    // smaller over time as terms remain close to their final normal forms.
    if t1.id == t2.id {
        t1
    } else {
        let vals = normalize(t1.write_state.inner())
            .union(&normalize(t2.write_state.inner()))
            .cloned()
            .collect::<Vec<_>>();
        join(cache, vals)
    }
}

pub fn function_this<L: LocSig>(cache: &mut ValCache<L>, reason: VirtualReason<L>) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::FunctionThis(reason))
}

pub fn global_this<L: LocSig>(cache: &mut ValCache<L>, reason: VirtualReason<L>) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::GlobalThis(reason))
}

pub fn illegal_this<L: LocSig>(cache: &mut ValCache<L>, reason: VirtualReason<L>) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::IllegalThis(reason))
}

pub fn class_instance_this<L: LocSig>(cache: &mut ValCache<L>, reason: VirtualReason<L>) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::ClassInstanceThis(reason))
}

pub fn class_static_this<L: LocSig>(cache: &mut ValCache<L>, reason: VirtualReason<L>) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::ClassStaticThis(reason))
}

pub fn class_instance_super<L: LocSig>(
    cache: &mut ValCache<L>,
    reason: VirtualReason<L>,
) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::ClassInstanceSuper(reason))
}

pub fn class_static_super<L: LocSig>(cache: &mut ValCache<L>, reason: VirtualReason<L>) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::ClassStaticSuper(reason))
}

pub fn module_scoped<L: LocSig>(cache: &mut ValCache<L>, name: FlowSmolStr) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::ModuleScoped(name))
}

pub fn global<L: LocSig>(cache: &mut ValCache<L>, name: FlowSmolStr) -> Val<L> {
    mk_with_write_state(cache, WriteStateInner::Global(name))
}

pub fn providers<L: LocSig>(
    cache: &mut ValCache<L>,
    locs: Vec<crate::provider_api::Provider<L>>,
) -> Val<L> {
    let write_states: Vec<WriteStateInner<L>> = locs
        .into_iter()
        .map(|provider| match provider.empty_array_writes {
            Some(arr_providers) => WriteStateInner::EmptyArray {
                reason: provider.reason,
                arr_providers,
            },
            None => WriteStateInner::Loc(provider.reason),
        })
        .collect();
    join(cache, write_states)
}

fn simplify_val<L: LocSig>(
    cache: &mut HashMap<usize, Rc<Vec<WriteLoc<L>>>>,
    t: &Val<L>,
) -> Rc<Vec<WriteLoc<L>>> {
    // Mirrors the OCaml cache lookup: cache hits
    // share the underlying list by reference (Rc::dupe is O(1)) instead of
    // deep-cloning a `Vec<WriteLoc<L>>`, which was a major hot spot for files
    // building up large refinement chains.
    if let Some(v) = cache.get(&t.id) {
        return v.dupe();
    }

    let vals = normalize(t.write_state.inner());

    let all_uninitialized_or_illegal = vals.iter().all(|v| {
        matches!(
            v,
            WriteStateInner::IllegalWrite(_) | WriteStateInner::Uninitialized(_)
        )
    });

    let result: Rc<Vec<WriteLoc<L>>> = Rc::new(
        vals.iter()
            .map(|write_state| match write_state {
                WriteStateInner::Uninitialized(l) if all_uninitialized_or_illegal => {
                    WriteLoc::Uninitialized(mk_reason(VirtualReasonDesc::RUninitialized, l.dupe()))
                }
                WriteStateInner::Undefined(r) => WriteLoc::Undefined(r.dupe()),
                WriteStateInner::Number(r) => WriteLoc::Number(r.dupe()),
                WriteStateInner::DeclaredFunction(l) => WriteLoc::DeclaredFunction(l.dupe()),
                WriteStateInner::Undeclared(name, loc)
                | WriteStateInner::DeclaredButSkipped(name, loc) => {
                    WriteLoc::Undeclared(name.dupe(), loc.dupe())
                }
                WriteStateInner::Uninitialized(l) => WriteLoc::Uninitialized(mk_reason(
                    VirtualReasonDesc::RPossiblyUninitialized,
                    l.dupe(),
                )),
                WriteStateInner::Projection(loc) => WriteLoc::Projection(loc.dupe()),
                WriteStateInner::FunctionThis(r) => WriteLoc::FunctionThis(r.dupe()),
                WriteStateInner::GlobalThis(r) => WriteLoc::GlobalThis(r.dupe()),
                WriteStateInner::IllegalThis(r) => WriteLoc::IllegalThis(r.dupe()),
                WriteStateInner::ClassInstanceThis(r) => WriteLoc::ClassInstanceThis(r.dupe()),
                WriteStateInner::ClassStaticThis(r) => WriteLoc::ClassStaticThis(r.dupe()),
                WriteStateInner::ClassInstanceSuper(r) => WriteLoc::ClassInstanceSuper(r.dupe()),
                WriteStateInner::ClassStaticSuper(r) => WriteLoc::ClassStaticSuper(r.dupe()),
                WriteStateInner::Loc(r) => WriteLoc::Write(r.dupe()),
                WriteStateInner::EmptyArray {
                    reason,
                    arr_providers,
                } => WriteLoc::EmptyArray {
                    reason: reason.dupe(),
                    arr_providers: arr_providers.dupe(),
                },
                WriteStateInner::IllegalWrite(r) => WriteLoc::IllegalWrite(r.dupe()),
                WriteStateInner::Refinement {
                    refinement_id,
                    val_t,
                } => WriteLoc::Refinement {
                    writes: simplify_val(cache, val_t),
                    refinement_id: *refinement_id as i32,
                    write_id: Some(val_t.id as i32),
                },
                WriteStateInner::ModuleScoped(name) => WriteLoc::ModuleScoped(name.dupe()),
                WriteStateInner::Global(name) => WriteLoc::Global(name.dupe()),
                WriteStateInner::PHI(_) => {
                    panic!("A normalized value cannot be a PHI")
                }
            })
            .collect(),
    );

    cache.insert(t.id, result.dupe());
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValBindingKind {
    // A source level binding is something we can point to a user and say that the definition
    // come from there. For refined reads of these bindings, they can be highlighted in IDEs.
    SourceLevelBinding(BindingsKind),
    // Internal bindings are completely internal to Flow. We abuse the same read-write analysis
    // to do our own analysis, and a user would never consider these to be actual reads.
    InternalBinding,
}

// Simplification converts a Val.t to a list of locations.
pub fn simplify<L: LocSig>(
    cache: &mut HashMap<usize, Rc<Vec<WriteLoc<L>>>>,
    def_loc: Option<L>,
    val_binding_kind: ValBindingKind,
    name: Option<FlowSmolStr>,
    value: &Val<L>,
) -> Read<L> {
    let write_locs = simplify_val(cache, value);
    let val_kind = match val_binding_kind {
        ValBindingKind::SourceLevelBinding(BindingsKind::Type {
            imported,
            type_only_namespace,
        })
        | ValBindingKind::SourceLevelBinding(BindingsKind::Interface {
            imported,
            type_only_namespace,
        }) => ValKind::Type {
            imported,
            type_only_namespace,
        },
        ValBindingKind::SourceLevelBinding(BindingsKind::TypeParam) => ValKind::TypeParam,
        ValBindingKind::SourceLevelBinding(BindingsKind::TsImport) => ValKind::TsImport,
        ValBindingKind::SourceLevelBinding(_) => ValKind::Value,
        ValBindingKind::InternalBinding => ValKind::Internal,
    };
    Read {
        def_loc,
        write_locs,
        val_kind,
        name,
        id: Some(value.id as i32),
    }
}

pub fn id_of_val<L: LocSig>(t: &Val<L>) -> usize {
    t.id
}

pub fn writes_of_uninitialized<L: LocSig + Ord, F>(
    refine_to_undefined: F,
    t: &Val<L>,
) -> Vec<WriteStateInner<L>>
where
    F: Fn(usize) -> bool,
{
    // Refinements share the values they refine, so the result for each refined value is computed
    // once and reused.
    fn state_is_uninitialized<L: LocSig + Ord, F>(
        v: &WriteStateInner<L>,
        refine_to_undefined: &F,
        refined_vals: &mut HashMap<usize, BTreeSet<WriteStateInner<L>>>,
    ) -> BTreeSet<WriteStateInner<L>>
    where
        F: Fn(usize) -> bool,
    {
        match v {
            WriteStateInner::Undeclared(_, _) => BTreeSet::from([v.clone()]),
            WriteStateInner::DeclaredButSkipped(_, _) => BTreeSet::new(),
            WriteStateInner::Undefined(_) => BTreeSet::new(),
            WriteStateInner::Number(_) => BTreeSet::new(),
            WriteStateInner::DeclaredFunction(_) => BTreeSet::new(),
            WriteStateInner::Uninitialized(_) => BTreeSet::from([v.clone()]),
            WriteStateInner::PHI(states) => states
                .iter()
                .flat_map(|s| state_is_uninitialized(s, refine_to_undefined, refined_vals))
                .collect(),
            WriteStateInner::Refinement {
                refinement_id,
                val_t,
            } => {
                if !refine_to_undefined(*refinement_id) {
                    return BTreeSet::new();
                }
                if let Some(states) = refined_vals.get(&val_t.id) {
                    return states.clone();
                }
                let states = state_is_uninitialized(
                    val_t.write_state.inner(),
                    refine_to_undefined,
                    refined_vals,
                );
                refined_vals.insert(val_t.id, states.clone());
                states
            }
            WriteStateInner::Loc(_) => BTreeSet::new(),
            WriteStateInner::EmptyArray { .. } => BTreeSet::new(),
            WriteStateInner::IllegalWrite(_) => BTreeSet::new(),
            WriteStateInner::FunctionThis(_) => BTreeSet::new(),
            WriteStateInner::GlobalThis(_) => BTreeSet::new(),
            WriteStateInner::IllegalThis(_) => BTreeSet::new(),
            WriteStateInner::ClassInstanceThis(_) => BTreeSet::new(),
            WriteStateInner::ClassStaticThis(_) => BTreeSet::new(),
            WriteStateInner::ClassInstanceSuper(_) => BTreeSet::new(),
            WriteStateInner::ClassStaticSuper(_) => BTreeSet::new(),
            WriteStateInner::ModuleScoped(_) => BTreeSet::new(),
            WriteStateInner::Global(_) => BTreeSet::new(),
            WriteStateInner::Projection(_) => BTreeSet::new(),
        }
    }

    state_is_uninitialized(
        t.write_state.inner(),
        &refine_to_undefined,
        &mut HashMap::new(),
    )
    .into_iter()
    .collect()
}
