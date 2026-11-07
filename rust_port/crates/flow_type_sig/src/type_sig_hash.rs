/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::LazyCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use dupe::Dupe;
use flow_common_cycle_hash as cycle_hash;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;

use crate::compact_table::Index;
use crate::compact_table::Table;
use crate::type_sig;
use crate::type_sig_pack as P;

pub type Node = cycle_hash::Node;

pub type ReadHash = cycle_hash::ReadHash;

pub enum CheckedDep<A> {
    CJS {
        filename: ReadHash,
        ns: A,
        type_exports: BTreeMap<FlowSmolStr, A>,
        exports: Option<A>,
    },
    ES {
        filename: ReadHash,
        ns: A,
        type_exports: BTreeMap<FlowSmolStr, A>,
        exports: BTreeMap<FlowSmolStr, A>,
    },
}

pub enum Dependency {
    Cyclic(LazyCell<Rc<CheckedDep<Rc<Node>>>, Box<dyn FnOnce() -> Rc<CheckedDep<Rc<Node>>>>>),
    Acyclic(LazyCell<CheckedDep<ReadHash>, Box<dyn FnOnce() -> CheckedDep<ReadHash>>>),
    Resource(ReadHash),
    Unchecked,
}

pub struct File {
    pub dependencies: Table<Dependency>,
    pub local_defs: Table<Rc<Node>>,
    pub remote_refs: Table<Rc<Node>>,
    pub pattern_defs: Table<Rc<Node>>,
    pub patterns: Table<Rc<Node>>,
}

fn require<A>(edge: &dyn Fn(&A), dep_edge: &dyn Fn(&ReadHash), dep: &CheckedDep<A>) {
    match dep {
        CheckedDep::CJS {
            filename, exports, ..
        } => {
            dep_edge(filename);
            if let Some(exports) = exports {
                edge(exports);
            }
        }
        CheckedDep::ES { filename, ns, .. } => {
            dep_edge(filename);
            edge(ns);
        }
    }
}

fn import<A>(
    name: &FlowSmolStr,
    edge: &dyn Fn(&A),
    dep_edge: &dyn Fn(&ReadHash),
    dep: &CheckedDep<A>,
) {
    match dep {
        CheckedDep::CJS {
            filename,
            ns,
            type_exports,
            exports,
        } => {
            dep_edge(filename);
            // If we `import type` from a CJS module, the module's exports object might
            // override any named type export. For example:
            //   > export type T = ...
            //   > module.exports = { T: class {} }
            // For hashing, we conservatively edge to both the exports object as well as
            // any reachable named exports, including those reachable via star exports \
            if let Some(exports) = exports {
                edge(exports);
            }
            match type_exports.get(name) {
                Some(node) => edge(node),
                None => {
                    // The ns object changes if any star exports change.
                    edge(ns)
                }
            }
        }
        CheckedDep::ES {
            filename,
            ns,
            type_exports,
            exports,
        } => {
            dep_edge(filename);
            // Note: type_exports and exports are disjoint.
            match type_exports.get(name) {
                Some(node) => edge(node),
                None => {
                    match exports.get(name) {
                        Some(node) => edge(node),
                        None => {
                            // If this module does not export the name directly, it might still
                            // export indirectly. The ns object changes if any star exports change.
                            edge(ns)
                        }
                    }
                }
            }
        }
    }
}

fn import_ns<A>(edge: &dyn Fn(&A), dep_edge: &dyn Fn(&ReadHash), dep: &CheckedDep<A>) {
    match dep {
        CheckedDep::CJS { filename, ns, .. } | CheckedDep::ES { filename, ns, .. } => {
            dep_edge(filename);
            edge(ns);
        }
    }
}

fn edge_local_def<Loc>(edge: &dyn Fn(Rc<Node>), file: &File, index: Index<Loc>) {
    edge(file.local_defs.get(index).dupe());
}

fn edge_remote_ref<Loc>(edge: &dyn Fn(Rc<Node>), file: &File, index: Index<Loc>) {
    edge(file.remote_refs.get(index).dupe());
}

fn edge_pattern<Loc>(edge: &dyn Fn(Rc<Node>), file: &File, index: Index<Loc>) {
    edge(file.patterns.get(index).dupe());
}

fn edge_pattern_def<Loc>(edge: &dyn Fn(Rc<Node>), file: &File, index: Index<Loc>) {
    edge(file.pattern_defs.get(index).dupe());
}

fn edge_require<Loc>(
    edge: &dyn Fn(Rc<Node>),
    dep_edge: &dyn Fn(&ReadHash),
    file: &File,
    index: Index<Loc>,
) {
    match file.dependencies.get(index) {
        Dependency::Cyclic(lazy_dep) => {
            let dep = LazyCell::force(lazy_dep);
            let edge = &|n: &Rc<Node>| edge(n.dupe());
            require(edge, dep_edge, dep);
        }
        Dependency::Acyclic(lazy_dep) => {
            let dep = LazyCell::force(lazy_dep);
            require(dep_edge, dep_edge, dep);
        }
        Dependency::Resource(dep) => dep_edge(dep),
        Dependency::Unchecked => {
            // All unchecked dependencies are equivalent. If a dependency changes from
            // unchecked to checked, then the hash will change anyway.
        }
    }
}

fn edge_import<Loc>(
    name: &FlowSmolStr,
    edge: &dyn Fn(Rc<Node>),
    dep_edge: &dyn Fn(&ReadHash),
    file: &File,
    index: Index<Loc>,
) {
    match file.dependencies.get(index) {
        Dependency::Cyclic(lazy_dep) => {
            let dep = LazyCell::force(lazy_dep);
            let edge = &|n: &Rc<Node>| edge(n.dupe());
            import(name, edge, dep_edge, dep);
        }
        Dependency::Acyclic(lazy_dep) => {
            let dep = LazyCell::force(lazy_dep);
            import(name, dep_edge, dep_edge, dep);
        }
        Dependency::Resource(dep) => dep_edge(dep),
        Dependency::Unchecked => {
            // All unchecked dependencies are equivalent. If a dependency changes from
            // unchecked to checked, then the hash will change anyway.
        }
    }
}

pub fn edge_import_ns<Loc>(
    edge: &dyn Fn(Rc<Node>),
    dep_edge: &dyn Fn(&ReadHash),
    file: &File,
    index: Index<Loc>,
) {
    match file.dependencies.get(index) {
        Dependency::Cyclic(lazy_dep) => {
            let dep = LazyCell::force(lazy_dep);
            let edge = &|n: &Rc<Node>| edge(n.dupe());
            import_ns(edge, dep_edge, dep);
        }
        Dependency::Acyclic(lazy_dep) => {
            let dep = LazyCell::force(lazy_dep);
            import_ns(dep_edge, dep_edge, dep);
        }
        Dependency::Resource(dep) => dep_edge(dep),
        Dependency::Unchecked => {
            // All unchecked dependencies are equivalent. If a dependency changes from
            // unchecked to checked, then the hash will change anyway.
        }
    }
}

fn visit_ref<Loc>(edge: &dyn Fn(Rc<Node>), file: &File, ref_: &P::PackedRef<Loc>) {
    match ref_ {
        P::PackedRef::LocalRef(box P::PackedRefLocal { ref_loc: _, index }) => {
            edge_local_def(edge, file, *index)
        }
        P::PackedRef::RemoteRef(box P::PackedRefRemote { ref_loc: _, index }) => {
            edge_remote_ref(edge, file, *index)
        }
        P::PackedRef::BuiltinRef(box P::PackedRefBuiltin { .. }) => {
            // If the builtins change, we will restart anyway, so there is no need to do
            // any hashing here.
        }
    }
}

fn visit_tyref<Loc>(edge: &dyn Fn(Rc<Node>), file: &File, tyref: &P::TyRef<Loc>) {
    match tyref {
        P::TyRef::Unqualified(ref_) => visit_ref(edge, file, ref_),
        P::TyRef::Qualified(box P::TyRefQualified { qualification, .. }) => {
            visit_tyref(edge, file, qualification);
        }
    }
}

pub fn visit_packed<Loc>(
    edge: &dyn Fn(Rc<Node>),
    dep_edge: &dyn Fn(&ReadHash),
    file: &File,
    packed: &P::Packed<Loc>,
) {
    match packed {
        P::Packed::Annot(t) => visit_annot(edge, dep_edge, file, t),
        P::Packed::Value(t) => visit_value(edge, dep_edge, file, t),
        P::Packed::Ref(ref_) => visit_ref(edge, file, ref_),
        P::Packed::TyRef(ref_) => visit_tyref(edge, file, ref_),
        P::Packed::TyRefApp(box P::PackedTyRefApp {
            loc: _,
            name,
            targs,
        }) => {
            visit_tyref(edge, file, name);
            for targ in targs {
                visit_packed(edge, dep_edge, file, targ);
            }
        }
        P::Packed::AsyncVoidReturn(_loc) => {}
        P::Packed::Pattern(index) => edge_pattern(edge, file, *index),
        P::Packed::Err(_loc) => {}
        P::Packed::Eval(box P::PackedEval {
            loc: _,
            packed: t,
            op,
            strictness_kind: _,
        }) => {
            visit_eval(edge, dep_edge, file, t, op);
        }
        P::Packed::Require(box P::PackedLocIndex { loc: _, index, .. }) => {
            edge_require(edge, dep_edge, file, *index)
        }
        P::Packed::ImportDynamic(box P::PackedLocIndex { loc: _, index, .. }) => {
            edge_import_ns(edge, dep_edge, file, *index)
        }
        P::Packed::ModuleRef(box P::PackedLocIndex { loc: _, index, .. }) => {
            edge_require(edge, dep_edge, file, *index)
        }
        P::Packed::ImportTypeAnnot(box P::PackedLocIndex { loc: _, index, .. }) => {
            edge_import_ns(edge, dep_edge, file, *index)
        }
    }
}

fn visit_eval<Loc>(
    edge: &dyn Fn(Rc<Node>),
    dep_edge: &dyn Fn(&ReadHash),
    file: &File,
    t: &P::Packed<Loc>,
    op: &type_sig::Op<Box<P::Packed<Loc>>>,
) {
    visit_packed(edge, dep_edge, file, t);
    visit_op(edge, dep_edge, file, op);
}

fn visit_annot<Loc>(
    edge: &dyn Fn(Rc<Node>),
    dep_edge: &dyn Fn(&ReadHash),
    file: &File,
    t: &type_sig::Annot<Loc, P::Packed<Loc>>,
) {
    t.iter(&mut (), &|_, _loc| { /* ignore */ }, &|_, packed| {
        visit_packed(edge, dep_edge, file, packed)
    });
}

fn visit_value<Loc>(
    edge: &dyn Fn(Rc<Node>),
    dep_edge: &dyn Fn(&ReadHash),
    file: &File,
    t: &type_sig::Value<Loc, P::Packed<Loc>>,
) {
    t.iter(&mut (), &|_, _loc| { /* ignore */ }, &|_, packed| {
        visit_packed(edge, dep_edge, file, packed)
    });
}

fn visit_op<Loc>(
    edge: &dyn Fn(Rc<Node>),
    dep_edge: &dyn Fn(&ReadHash),
    file: &File,
    op: &type_sig::Op<Box<P::Packed<Loc>>>,
) {
    op.iter(|packed| visit_packed(edge, dep_edge, file, packed));
}

pub fn visit_def<Loc: Clone>(
    edge: &dyn Fn(Rc<Node>),
    dep_edge: &dyn Fn(&ReadHash),
    file: &File,
    def: &type_sig::Def<Loc, P::Packed<Loc>>,
) {
    def.iter(&mut (), &|_, _loc| { /* ignore */ }, &|_, packed| {
        visit_packed(edge, dep_edge, file, packed)
    });
}

pub fn visit_remote_ref<Loc>(
    edge: &dyn Fn(Rc<Node>),
    dep_edge: &dyn Fn(&ReadHash),
    file: &File,
    rref: &P::RemoteRef<Loc>,
) {
    match rref {
        P::RemoteRef::Import { index, remote, .. }
        | P::RemoteRef::ImportType { index, remote, .. }
        | P::RemoteRef::ImportTypeof { index, remote, .. } => {
            edge_import(remote, edge, dep_edge, file, *index);
        }
        P::RemoteRef::ImportTypeofNs { index, .. }
        | P::RemoteRef::ImportNs { index, .. }
        | P::RemoteRef::ImportTypeNs { index, .. } => {
            edge_import_ns(edge, dep_edge, file, *index);
        }
    }
}

pub fn visit_pattern<Loc>(edge: &dyn Fn(Rc<Node>), file: &File, pattern: &P::Pattern<Loc>) {
    match pattern {
        P::Pattern::PDef(index) => edge_pattern_def(edge, file, *index),
        P::Pattern::ComputedP { elem, def, .. } => {
            edge_pattern_def(edge, file, *elem);
            edge_pattern(edge, file, *def);
        }
        P::Pattern::PropP { def, .. }
        | P::Pattern::ObjRestP { def, .. }
        | P::Pattern::IndexP { def, .. }
        | P::Pattern::ArrRestP { def, .. } => {
            edge_pattern(edge, file, *def);
        }
        P::Pattern::UnsupportedLiteralP(_loc) => {}
    }
}

pub fn visit_export<Loc>(
    edge: &dyn Fn(Rc<Node>),
    dep_edge: &dyn Fn(&ReadHash),
    file: &File,
    export: &P::Export<Loc>,
) {
    match export {
        P::Export::ExportRef(ref_) => visit_ref(edge, file, ref_),
        P::Export::ExportDefault(box P::ExportDefaultData {
            default_loc: _,
            def,
        }) => visit_packed(edge, dep_edge, file, def),
        P::Export::ExportBinding(index) => {
            edge_local_def(edge, file, *index);
        }
        P::Export::ExportDefaultBinding(box P::ExportDefaultBindingData {
            default_loc: _,
            index,
        }) => {
            edge_local_def(edge, file, *index);
        }
        P::Export::ExportFrom(index) => edge_remote_ref(edge, file, *index),
    }
}

pub fn visit_type_export<Loc>(edge: &dyn Fn(Rc<Node>), file: &File, texport: &P::TypeExport<Loc>) {
    match texport {
        P::TypeExport::ExportTypeRef(ref_) => visit_ref(edge, file, ref_),
        P::TypeExport::ExportTypeBinding(index) => edge_local_def(edge, file, *index),
        P::TypeExport::ExportTypeFrom(index) => edge_remote_ref(edge, file, *index),
    }
}

pub fn visit_ts_pending_export<Loc>(
    edge: &dyn Fn(Rc<Node>),
    dep_edge: &dyn Fn(&ReadHash),
    file: &File,
    pending: &P::TsPendingExport<Loc>,
) {
    match pending {
        P::TsPendingExport::TsExportRef {
            export_loc: _,
            ref_,
            import_provenance,
        } => {
            visit_ref(edge, file, ref_);
            if let Some((index, remote)) = import_provenance {
                edge_import(remote, edge, dep_edge, file, *index);
            }
        }
        P::TsPendingExport::TsExportFrom {
            export_loc: _,
            mref,
            remote_name,
        } => {
            edge_import(remote_name, edge, dep_edge, file, *mref);
        }
    }
}
