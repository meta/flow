/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// The mark phase visits a signature starting from exports, such that the only
// visited data are also reachable from exports.
//
// During this walk, it resolves references from the parsing phase, which are
// left unresolved as (name, scope) pairs until this point.
//
// The "mark" name is borrowed from garbage collection vocabulary. Like a tracing
// collector, the marking process is used to detect what values are reachable so
// the remainder can be discarded.

use flow_common::polarity::Polarity;
use flow_parser::loc::Loc;

use crate::signature_error::BindingValidation;
use crate::type_sig;
use crate::type_sig_options::TypeSigOptions;
use crate::type_sig_parse as parse;
use crate::type_sig_parse::LocNode;
use crate::type_sig_parse::ModuleRefNode;

pub(super) struct Marker<'a> {
    locs_to_dirtify: &'a [Loc],
    should_be_dirty: bool,
}

impl<'a> Marker<'a> {
    pub(super) fn new(locs_to_dirtify: &'a [Loc]) -> Self {
        Self {
            locs_to_dirtify,
            should_be_dirty: false,
        }
    }

    fn mark_local_binding<'arena, 'ast>(
        &mut self,
        opts: &TypeSigOptions,
        scopes: &mut parse::scope::Scopes<'arena, 'ast>,
        tbls: &mut parse::Tables<'arena, 'ast>,
        node: &parse::LocalDefNode<'arena, 'ast>,
    ) {
        node.0.mark(|binding| {
            self.mark_and_report_should_be_dirtified(|marker| {
                mark_local_binding(opts, scopes, tbls, marker, binding)
            })
        });
    }

    fn mark_remote_ref<'arena>(&mut self, node: &parse::RemoteRefNode<'arena>) {
        node.0.mark(|binding| {
            self.mark_skip_visit_loc(|marker| mark_remote_binding(marker, binding));
            false
        });
    }

    fn mark_pattern_def<'arena, 'ast>(
        &mut self,
        opts: &TypeSigOptions,
        scopes: &mut parse::scope::Scopes<'arena, 'ast>,
        tbls: &mut parse::Tables<'arena, 'ast>,
        node: &parse::PatternDefNode<'arena, 'ast>,
    ) {
        node.0.mark(|parsed| {
            self.mark_and_report_should_be_dirtified(|marker| {
                mark_parsed(opts, scopes, tbls, marker, parsed)
            })
        });
    }

    fn mark_pattern<'arena, 'ast>(
        &mut self,
        opts: &TypeSigOptions,
        scopes: &mut parse::scope::Scopes<'arena, 'ast>,
        tbls: &mut parse::Tables<'arena, 'ast>,
        node: &parse::PatternNode<'arena, 'ast>,
    ) {
        node.0.mark(|pattern| {
            self.mark_and_report_should_be_dirtified(|marker| {
                mark_pattern(opts, scopes, tbls, marker, pattern)
            })
        });
    }

    fn mark_and_report_should_be_dirtified(&mut self, f: impl FnOnce(&mut Self)) -> bool {
        let saved_should_be_dirty = self.should_be_dirty;
        self.should_be_dirty = false;
        f(self);
        let d = self.should_be_dirty;
        self.should_be_dirty = saved_should_be_dirty;
        d
    }

    fn mark_skip_visit_loc(&mut self, f: impl FnOnce(&mut Self)) {
        let saved_should_be_dirty = self.should_be_dirty;
        self.should_be_dirty = true;
        f(self);
        self.should_be_dirty = saved_should_be_dirty;
    }

    fn visit_loc(&mut self, loc: &Loc) {
        if self.should_be_dirty {
            return;
        }
        for l in self.locs_to_dirtify {
            if loc.contains(l) {
                self.should_be_dirty = true;
                return;
            }
        }
    }
}

fn mark_loc<'arena>(marker: &mut Marker<'_>, loc: &parse::LocNode<'arena>) {
    loc.0.mark(|loc| {
        marker.visit_loc(loc);
        false
    });
}

fn mark_mref<'arena>(mref: &parse::ModuleRefNode<'arena>) {
    mref.0.mark(|_| false);
}

fn mark_parsed<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    parsed: &parse::Parsed<'arena, 'ast>,
) {
    match parsed {
        parse::Parsed::Annot(t) => mark_annot(opts, scopes, tbls, marker, t),
        parse::Parsed::Value(def) => mark_value(opts, scopes, tbls, marker, def),
        parse::Parsed::TyRef(name) => mark_tyname(opts, scopes, tbls, marker, name),
        parse::Parsed::TyRefApp { loc, name, targs } => {
            mark_loc(marker, loc);
            mark_tyname(opts, scopes, tbls, marker, name);
            for t in targs {
                mark_parsed(opts, scopes, tbls, marker, t);
            }
        }
        parse::Parsed::AsyncVoidReturn(loc) => mark_loc(marker, loc),
        parse::Parsed::BuiltinTyRef { ref_loc, name: _ } => mark_loc(marker, ref_loc),
        parse::Parsed::Err(loc, errno) => {
            errno.iter(|loc| mark_loc(marker, loc));
            mark_loc(marker, loc);
        }
        parse::Parsed::ValRef {
            lookup: parse::ValRefLookup::ValueRefLookup,
            ref_,
        } => resolve_value_ref(opts, scopes, tbls, marker, ref_),
        parse::Parsed::ValRef {
            lookup: parse::ValRefLookup::NamespaceValueRefLookup(polarity),
            ref_,
        } => {
            resolve_value_ref(opts, scopes, tbls, marker, ref_);
            if let Some(Some(parse::BindingNode::LocalBinding(node))) = ref_.resolved.get()
                && matches!(
                    &*node.0.data(),
                    parse::LocalBinding::LetConstBinding { is_const: true, .. }
                        | parse::LocalBinding::ConstRefBinding { .. }
                        | parse::LocalBinding::ConstFunBinding { .. }
                )
            {
                polarity.set(Polarity::Positive);
            }
        }
        parse::Parsed::ValRef {
            lookup: parse::ValRefLookup::TypeRefLookup,
            ref_,
        } => resolve_type_ref(opts, scopes, tbls, marker, ref_),
        parse::Parsed::ValRef {
            lookup: parse::ValRefLookup::TypeofRefLookup,
            ref_,
        } => resolve_typeof_ref(opts, scopes, tbls, marker, ref_),
        parse::Parsed::Pattern(p) => {
            marker.mark_pattern(opts, scopes, tbls, p);
        }
        parse::Parsed::Eval(loc, t, op, _) => {
            mark_loc(marker, loc);
            mark_parsed(opts, scopes, tbls, marker, t);
            mark_op(opts, scopes, tbls, marker, op);
        }
        parse::Parsed::Require { loc, mref, .. }
        | parse::Parsed::ImportDynamic { loc, mref, .. }
        | parse::Parsed::ModuleRef { loc, mref, .. }
        | parse::Parsed::ImportTypeAnnot { loc, mref, .. } => {
            mark_loc(marker, loc);
            mark_mref(mref);
        }
    }
}

fn mark_tyname<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    tyname: &parse::TyName<'arena, 'ast>,
) {
    match tyname {
        parse::TyName::Unqualified(ref_) => resolve_type_ref(opts, scopes, tbls, marker, ref_),
        parse::TyName::Qualified {
            loc,
            id_loc,
            name: _,
            qualification,
            strictness_kind: _,
        } => {
            mark_loc(marker, loc);
            mark_loc(marker, id_loc);
            mark_tyname(opts, scopes, tbls, marker, qualification);
        }
    }
}

pub(crate) fn mark_binding<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    binding: &parse::BindingNode<'arena, 'ast>,
) {
    match binding {
        parse::BindingNode::LocalBinding(node) => {
            marker.mark_local_binding(opts, scopes, tbls, node);
        }
        parse::BindingNode::RemoteBinding(node) => {
            marker.mark_remote_ref(node);
        }
    }
}

fn mark_local_binding<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    binding: &parse::LocalBinding<'arena, 'ast>,
) {
    match binding {
        parse::LocalBinding::TypeBinding { id_loc: _, def } => {
            let def = def.get_forced(opts, scopes, tbls);
            mark_def(opts, scopes, tbls, marker, def);
        }
        parse::LocalBinding::VarBinding {
            id_loc,
            name: _,
            def,
        }
        | parse::LocalBinding::LetConstBinding {
            is_const: _,
            id_loc,
            name: _,
            def,
        } => {
            mark_loc(marker, id_loc);
            let parsed = def.get_forced(opts, scopes, tbls);
            mark_parsed(opts, scopes, tbls, marker, parsed);
        }
        parse::LocalBinding::ParamBinding {
            id_loc,
            name: _,
            def,
            tparams,
            strictness_kind: _,
        } => {
            mark_loc(marker, id_loc);
            let parsed = def.get_forced(opts, scopes, tbls);
            mark_parsed(opts, scopes, tbls, marker, parsed);
            // Pack consumes the tparams, so they must be marked: a lone param
            // root (targeted packing) otherwise panics in `index_exn`.
            let mut cx = (tbls, marker, scopes);
            tparams.iter(
                &mut cx,
                &|(_tbls, marker, _scopes), loc| mark_loc(marker, loc),
                &|(tbls, marker, scopes), t| mark_parsed(opts, scopes, tbls, marker, t),
            );
        }
        parse::LocalBinding::ConstRefBinding {
            id_loc,
            name: _,
            ref_,
        } => {
            mark_loc(marker, id_loc);
            resolve_value_ref(opts, scopes, tbls, marker, ref_);
        }
        parse::LocalBinding::ConstFunBinding {
            id_loc,
            name: _,
            loc,
            async_: _,
            generator: _,
            def,
            statics,
        } => {
            mark_loc(marker, id_loc);
            mark_loc(marker, loc);
            let parsed = def.get_forced(opts, scopes, tbls);
            mark_fun(opts, scopes, tbls, marker, parsed);
            for (id_loc, def) in statics.values() {
                mark_loc(marker, id_loc);
                mark_parsed(opts, scopes, tbls, marker, def);
            }
        }
        parse::LocalBinding::FunBinding {
            id_loc,
            name: _,
            async_: _,
            generator: _,
            fn_loc,
            def,
            statics,
            namespace_types,
        } => {
            mark_loc(marker, id_loc);
            mark_loc(marker, fn_loc);
            let parsed = def.get_forced(opts, scopes, tbls);
            mark_fun(opts, scopes, tbls, marker, parsed);
            for (id_loc, def) in statics.values() {
                mark_loc(marker, id_loc);
                mark_parsed(opts, scopes, tbls, marker, def);
            }
            for (id_loc, def) in namespace_types.values() {
                mark_loc(marker, id_loc);
                mark_parsed(opts, scopes, tbls, marker, def);
            }
        }
        parse::LocalBinding::ComponentBinding {
            id_loc,
            name: _,
            fn_loc,
            def,
        } => {
            mark_loc(marker, id_loc);
            if let Some(def) = def {
                mark_loc(marker, fn_loc);
                let def = def.get_forced(opts, scopes, tbls);
                mark_component(opts, scopes, tbls, marker, def);
            }
        }
        parse::LocalBinding::ClassBinding {
            id_loc,
            name: _,
            def,
            namespace_values,
            namespace_types,
        } => {
            mark_loc(marker, id_loc);
            let def = def.get_forced(opts, scopes, tbls);
            mark_class(opts, scopes, tbls, marker, def);
            for (id_loc, def) in namespace_types.values().chain(namespace_values.values()) {
                mark_loc(marker, id_loc);
                mark_parsed(opts, scopes, tbls, marker, def);
            }
        }
        parse::LocalBinding::DeclareClassBinding {
            id_loc,
            name: _,
            def,
            namespace_values,
            namespace_types,
        } => {
            mark_loc(marker, id_loc);
            let def = def.get_forced(opts, scopes, tbls);
            mark_declare_class(opts, scopes, tbls, marker, def);
            for (id_loc, def) in namespace_types.values().chain(namespace_values.values()) {
                mark_loc(marker, id_loc);
                mark_parsed(opts, scopes, tbls, marker, def);
            }
        }
        parse::LocalBinding::RecordBinding {
            id_loc,
            name: _,
            def,
            defaulted_props: _,
        } => {
            mark_loc(marker, id_loc);
            if let Some(def) = def {
                let def = def.get_forced(opts, scopes, tbls);
                mark_class(opts, scopes, tbls, marker, def);
            }
        }
        parse::LocalBinding::DeclareFunBinding {
            name: _,
            defs,
            statics,
            namespace_types,
        } => {
            for (id_loc, fn_loc, def) in defs {
                mark_loc(marker, id_loc);
                mark_loc(marker, fn_loc);
                let def = def.get_forced(opts, scopes, tbls);
                mark_fun(opts, scopes, tbls, marker, def);
            }
            for (id_loc, def) in statics.values() {
                mark_loc(marker, id_loc);
                mark_parsed(opts, scopes, tbls, marker, def);
            }
            for (id_loc, def) in namespace_types.values() {
                mark_loc(marker, id_loc);
                mark_parsed(opts, scopes, tbls, marker, def);
            }
        }
        parse::LocalBinding::EnumBinding {
            id_loc,
            name: _,
            def,
            strictness_kind: _,
        } => {
            mark_loc(marker, id_loc);
            if let Some((_rep, members, _member_values, _has_unknown_members)) =
                def.as_ref().map(|d| d.get_forced(opts, scopes, tbls))
            {
                for member_loc in members.values() {
                    mark_loc(marker, member_loc);
                }
            }
        }
        parse::LocalBinding::NamespaceBinding {
            id_loc,
            name: _,
            values,
            types,
            strictness_kind: _,
        } => {
            mark_loc(marker, id_loc);
            for (loc, parsed) in values.values() {
                mark_loc(marker, loc);
                mark_parsed(opts, scopes, tbls, marker, parsed);
            }
            for (loc, parsed) in types.values() {
                mark_loc(marker, loc);
                mark_parsed(opts, scopes, tbls, marker, parsed);
            }
        }
    }
}

fn mark_remote_binding<'arena>(marker: &mut Marker<'_>, binding: &parse::RemoteBinding<'arena>) {
    match binding {
        parse::RemoteBinding::ImportBinding {
            id_loc,
            name: _,
            mref,
            remote: _,
            strictness_kind: _,
        }
        | parse::RemoteBinding::ImportTypeBinding {
            id_loc,
            name: _,
            mref,
            remote: _,
            strictness_kind: _,
        }
        | parse::RemoteBinding::ImportTypeofBinding {
            id_loc,
            name: _,
            mref,
            remote: _,
            strictness_kind: _,
        }
        | parse::RemoteBinding::ImportNsBinding {
            id_loc,
            name: _,
            mref,
            strictness_kind: _,
        }
        | parse::RemoteBinding::ImportTypeofNsBinding {
            id_loc,
            name: _,
            mref,
            strictness_kind: _,
        }
        | parse::RemoteBinding::ImportTypeNsBinding {
            id_loc,
            name: _,
            mref,
            strictness_kind: _,
        } => {
            marker.mark_skip_visit_loc(|marker| mark_loc(marker, id_loc));
            mark_mref(mref);
        }
    }
}

fn mark_pattern<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    pattern: &parse::Pattern<'arena, 'ast>,
) {
    match pattern {
        parse::Pattern::PDef(def) => {
            let def = def.get_forced(opts, scopes, tbls);
            marker.mark_pattern_def(opts, scopes, tbls, def);
        }
        parse::Pattern::PropP {
            def,
            id_loc: loc,
            name: _,
            strictness_kind: _,
        }
        | parse::Pattern::ObjRestP {
            def,
            loc,
            xs: _,
            strictness_kind: _,
        }
        | parse::Pattern::IndexP {
            def,
            loc,
            i: _,
            strictness_kind: _,
        }
        | parse::Pattern::ArrRestP { def, loc, i: _ } => {
            mark_loc(marker, loc);
            marker.mark_pattern(opts, scopes, tbls, def);
        }
        parse::Pattern::ComputedP {
            def,
            elem,
            strictness_kind: _,
        } => {
            marker.mark_pattern(opts, scopes, tbls, def);
            marker.mark_pattern_def(opts, scopes, tbls, elem);
        }
        parse::Pattern::UnsupportedLiteralP(loc) => mark_loc(marker, loc),
    }
}

fn mark_value<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    def: &type_sig::Value<LocNode<'arena>, parse::Parsed<'arena, 'ast>>,
) {
    let mut cx = (tbls, marker, scopes);
    def.iter(
        &mut cx,
        &|(_tbls, marker, _scopes), loc| mark_loc(marker, loc),
        &|(tbls, marker, scopes), t| mark_parsed(opts, scopes, tbls, marker, t),
    );
}

fn mark_def<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    def: &type_sig::Def<LocNode<'arena>, parse::Parsed<'arena, 'ast>>,
) {
    let mut cx = (tbls, marker, scopes);
    def.iter(
        &mut cx,
        &|(_tbls, marker, _scopes), loc| mark_loc(marker, loc),
        &|(tbls, marker, scopes), t| mark_parsed(opts, scopes, tbls, marker, t),
    );
}

fn mark_annot<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    t: &type_sig::Annot<LocNode<'arena>, parse::Parsed<'arena, 'ast>>,
) {
    let mut cx = (tbls, marker, scopes);
    t.iter(
        &mut cx,
        &|(_tbls, marker, _scopes), loc| mark_loc(marker, loc),
        &|(tbls, marker, scopes), t| mark_parsed(opts, scopes, tbls, marker, t),
    );
}

fn mark_fun<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    def: &type_sig::FunSig<LocNode<'arena>, parse::Parsed<'arena, 'ast>>,
) {
    let mut cx = (tbls, marker, scopes);
    def.iter(
        &mut cx,
        &|(_tbls, marker, _scopes), loc| mark_loc(marker, loc),
        &|(tbls, marker, scopes), t| mark_parsed(opts, scopes, tbls, marker, t),
    );
}

fn mark_component<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    def: &type_sig::ComponentSig<LocNode<'arena>, parse::Parsed<'arena, 'ast>>,
) {
    let mut cx = (tbls, marker, scopes);
    def.iter(
        &mut cx,
        &|(_tbls, marker, _scopes), loc| mark_loc(marker, loc),
        &|(tbls, marker, scopes), t| mark_parsed(opts, scopes, tbls, marker, t),
    );
}

fn mark_class<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    def: &type_sig::ClassSig<LocNode<'arena>, parse::Parsed<'arena, 'ast>>,
) {
    let mut cx = (tbls, marker, scopes);
    def.iter(
        &mut cx,
        &|(_tbls, marker, _scopes), loc| mark_loc(marker, loc),
        &|(tbls, marker, scopes), t| mark_parsed(opts, scopes, tbls, marker, t),
    );
}

fn mark_declare_class<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    def: &type_sig::DeclareClassSig<LocNode<'arena>, parse::Parsed<'arena, 'ast>>,
) {
    let mut cx = (tbls, marker, scopes);
    def.iter(
        &mut cx,
        &|(_tbls, marker, _scopes), loc| mark_loc(marker, loc),
        &|(tbls, marker, scopes), t| mark_parsed(opts, scopes, tbls, marker, t),
    );
}

fn mark_op<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    op: &type_sig::Op<parse::Parsed<'arena, 'ast>>,
) {
    op.iter(|t| mark_parsed(opts, scopes, tbls, marker, t));
}

fn resolve_value_ref<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    ref_: &parse::Ref<'arena, 'ast>,
) {
    mark_loc(marker, &ref_.ref_loc);
    match scopes.lookup_ref(tbls, ref_, false) {
        Some(binding) => {
            mark_binding(opts, scopes, tbls, marker, &binding);
            let _ = ref_.resolved.set(Some(binding));
        }
        None => {
            let _ = ref_.resolved.set(None);
        }
    }
}

fn resolve_type_ref<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    ref_: &parse::Ref<'arena, 'ast>,
) {
    mark_loc(marker, &ref_.ref_loc);
    match scopes.lookup_ref(tbls, ref_, true) {
        Some(binding) => {
            mark_binding(opts, scopes, tbls, marker, &binding);
            let _ = ref_.resolved.set(Some(binding));
            // Keep alive a co-existing value-side binding for the same name (e.g. an
            // `import { X }` next to an `interface X`). The check phase eagerly
            // resolves every parsed module ref regardless of marking, so dropping the
            // import here would leave the dep file out of the sig dep graph and the
            // merge set, causing workers to fail with Parsing_heaps.Leader_not_found
            // when checking reaches the unmerged dep.
            match parse::scope::lookup_value(scopes, ref_.scope, &ref_.name) {
                Some((value_binding, _)) => {
                    mark_binding(opts, scopes, tbls, marker, &value_binding);
                }
                None => {}
            }
        }
        None => {
            let _ = ref_.resolved.set(None);
        }
    }
}

fn binding_allowed_in_typeof<'arena, 'ast>(binding: &parse::BindingNode<'arena, 'ast>) -> bool {
    match binding {
        parse::BindingNode::LocalBinding(node) => {
            matches!(
                &*node.0.data(),
                parse::LocalBinding::NamespaceBinding { .. }
            )
        }
        parse::BindingNode::RemoteBinding(node) => matches!(
            &*node.0.data(),
            parse::RemoteBinding::ImportTypeNsBinding { .. }
                | parse::RemoteBinding::ImportTypeofNsBinding { .. }
        ),
    }
}

fn resolve_typeof_ref<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    ref_: &parse::Ref<'arena, 'ast>,
) {
    mark_loc(marker, &ref_.ref_loc);
    match scopes.lookup_ref(tbls, ref_, false) {
        Some(binding) => {
            mark_binding(opts, scopes, tbls, marker, &binding);
            let _ = ref_.resolved.set(Some(binding));
        }
        None => match scopes.lookup_ref(tbls, ref_, true) {
            Some(binding) if binding_allowed_in_typeof(&binding) => {
                mark_binding(opts, scopes, tbls, marker, &binding);
                let _ = ref_.resolved.set(Some(binding));
            }
            _ => {
                let _ = ref_.resolved.set(None);
            }
        },
    }
}

fn mark_export<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    export: &parse::Export<'arena, 'ast>,
) {
    match export {
        parse::Export::ExportRef(ref_) => resolve_value_ref(opts, scopes, tbls, marker, ref_),
        parse::Export::ExportBinding(binding) => {
            marker.mark_local_binding(opts, scopes, tbls, binding);
        }
        parse::Export::ExportDefaultBinding {
            default_loc,
            name: _,
            binding,
        } => {
            mark_loc(marker, default_loc);
            marker.mark_local_binding(opts, scopes, tbls, binding);
        }
        parse::Export::ExportDefault { default_loc, def } => {
            mark_loc(marker, default_loc);
            mark_parsed(opts, scopes, tbls, marker, def);
        }
        parse::Export::ExportFrom(ref_) => {
            marker.mark_remote_ref(ref_);
        }
    }
}

fn mark_export_type<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    export: &parse::ExportType<'arena, 'ast>,
) {
    match export {
        parse::ExportType::ExportTypeRef(ref_) => {
            resolve_type_ref(opts, scopes, tbls, marker, ref_)
        }
        parse::ExportType::ExportTypeBinding(binding) => {
            marker.mark_local_binding(opts, scopes, tbls, binding);
        }
        parse::ExportType::ExportTypeFrom(ref_) => {
            marker.mark_remote_ref(ref_);
        }
    }
}

fn mark_ts_pending_export<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    export: &parse::TsPendingExport<'arena, 'ast>,
) {
    match export {
        parse::TsPendingExport::TsExportRef {
            export_loc,
            ref_,
            import_provenance,
        } => {
            mark_loc(marker, export_loc);
            resolve_value_ref(opts, scopes, tbls, marker, ref_);
            if let Some((mref, _)) = import_provenance {
                mark_mref(mref);
            }
        }
        parse::TsPendingExport::TsExportFrom {
            export_loc,
            mref,
            remote_name: _,
        } => {
            mark_loc(marker, export_loc);
            mark_mref(mref);
        }
    }
}

fn mark_star<'arena>(
    marker: &mut Marker<'_>,
    (loc, mref): &(LocNode<'arena>, ModuleRefNode<'arena>),
) {
    mark_loc(marker, loc);
    mark_mref(mref);
}

pub(super) fn mark_exports<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    marker: &mut Marker<'_>,
    file_loc: &LocNode<'arena>,
    exports: &parse::Exports<'arena, 'ast>,
) {
    marker.mark_skip_visit_loc(|marker| {
        for t in exports.types.values() {
            mark_export_type(opts, scopes, tbls, marker, t);
        }
        for star in &exports.type_stars {
            mark_star(marker, star);
        }
        for t in exports.ts_pending.values() {
            mark_ts_pending_export(opts, scopes, tbls, marker, t);
        }
        match &exports.kind {
            parse::ModuleKind::UnknownModule => {}
            parse::ModuleKind::CJSModule(t) => mark_parsed(opts, scopes, tbls, marker, t),
            parse::ModuleKind::CJSModuleProps { props, .. } => {
                mark_loc(marker, file_loc);
                for (loc, t) in props.values() {
                    mark_loc(marker, loc);
                    mark_parsed(opts, scopes, tbls, marker, t);
                }
            }
            parse::ModuleKind::CJSDeclareModule { props, .. } => {
                mark_loc(marker, file_loc);
                for b in props.values() {
                    marker.mark_local_binding(opts, scopes, tbls, b);
                }
            }
            parse::ModuleKind::ESModule { names, stars } => {
                for t in names.values() {
                    mark_export(opts, scopes, tbls, marker, t);
                }
                for star in stars {
                    mark_star(marker, star);
                }
            }
        }
    });
}

pub(super) fn mark_errors<'arena>(
    marker: &mut Marker<'_>,
    errors: &[BindingValidation<LocNode<'arena>>],
) {
    marker.mark_skip_visit_loc(|marker| {
        for error in errors {
            error.iter(|loc| mark_loc(marker, loc));
        }
    });
}

pub(super) fn mark_builtin_module<'arena, 'ast>(
    opts: &TypeSigOptions,
    scopes: &mut parse::scope::Scopes<'arena, 'ast>,
    tbls: &mut parse::Tables<'arena, 'ast>,
    loc: &LocNode<'arena>,
    exports: &parse::Exports<'arena, 'ast>,
) {
    let mut marker = Marker::new(&[]);
    marker.mark_skip_visit_loc(|marker| mark_loc(marker, loc));
    mark_exports(opts, scopes, tbls, &mut marker, loc, exports);
}
