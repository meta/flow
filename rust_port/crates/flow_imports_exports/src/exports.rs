/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::HashSet;

use dupe::Dupe;
use flow_common::flow_import_specifier::FlowImportSpecifier;
use flow_common::flow_import_specifier::Userland;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::js_id_unicode::string_is_valid_identifier_name;
use flow_type_sig::compact_table::Index;
use flow_type_sig::compact_table::Table;
use flow_type_sig::packed_type_sig::Builtins;
use flow_type_sig::packed_type_sig::Module;
use flow_type_sig::type_sig::AnnotObjAnnot;
use flow_type_sig::type_sig::AnnotTypeof;
use flow_type_sig::type_sig::Def;
use flow_type_sig::type_sig::ObjValueProp;
use flow_type_sig::type_sig::Op;
use flow_type_sig::type_sig::ValueDeclareModuleImplicitlyExportedObject;
use flow_type_sig::type_sig::ValueObjLit;
use flow_type_sig::type_sig_pack::CJSModuleInfo;
use flow_type_sig::type_sig_pack::ESModuleInfo;
use flow_type_sig::type_sig_pack::Export as TypeSigExport;
use flow_type_sig::type_sig_pack::ExportDefaultBindingData;
use flow_type_sig::type_sig_pack::ExportDefaultData;
use flow_type_sig::type_sig_pack::ModuleKind;
use flow_type_sig::type_sig_pack::Packed;
use flow_type_sig::type_sig_pack::PackedAnnot;
use flow_type_sig::type_sig_pack::PackedDef;
use flow_type_sig::type_sig_pack::PackedEval;
use flow_type_sig::type_sig_pack::PackedRef;
use flow_type_sig::type_sig_pack::PackedRefLocal;
use flow_type_sig::type_sig_pack::PackedTyRefApp;
use flow_type_sig::type_sig_pack::PackedValue;
use flow_type_sig::type_sig_pack::Pattern;
use flow_type_sig::type_sig_pack::TyRef;
use flow_type_sig::type_sig_pack::TyRefQualified;
use flow_type_sig::type_sig_pack::TypeExport;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Export {
    // e.g. `export default class Foo {}`
    DefaultType(Option<FlowSmolStr>),
    // e.g. `export default function() {}`
    Default(Option<FlowSmolStr>),
    // `export const foo: string = "foo"`
    Named(FlowSmolStr),
    // `export type T = string`
    NamedType(FlowSmolStr),
    // `declare module "foo" { ... exports ... }`
    Module(FlowImportSpecifier, Vec<Export>),
    ReExportModule(Userland),
    ReExportModuleTypes(Userland),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Exports(Vec<Export>);

impl Exports {
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    pub fn new(exports: Vec<Export>) -> Self {
        Self(exports)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Export> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

struct ExportSig<'a, Loc> {
    module_kind: Option<&'a ModuleKind<Index<Loc>>>,
    module_refs: &'a Table<Userland>,
    local_defs: &'a Table<PackedDef<Index<Loc>>>,
    pattern_defs: &'a Table<Packed<Index<Loc>>>,
    patterns: &'a Table<Pattern<Index<Loc>>>,
}

impl<'a, Loc> ExportSig<'a, Loc> {
    fn of_module(module: &'a Module<Loc>) -> Self {
        ExportSig {
            module_kind: Some(&module.module_kind),
            module_refs: &module.module_refs,
            local_defs: &module.local_defs,
            pattern_defs: &module.pattern_defs,
            patterns: &module.patterns,
        }
    }

    fn of_builtins(builtins: &'a Builtins<Loc>) -> Self {
        ExportSig {
            module_kind: None,
            module_refs: &builtins.module_refs,
            local_defs: &builtins.local_defs,
            pattern_defs: &builtins.pattern_defs,
            patterns: &builtins.patterns,
        }
    }
}

fn local_def_of_index<'a, Loc>(
    export_sig: &'a ExportSig<'a, Loc>,
    index: Index<PackedDef<Index<Loc>>>,
) -> &'a PackedDef<Index<Loc>> {
    export_sig.local_defs.get(index)
}

fn pattern_of_index<'a, Loc>(
    export_sig: &'a ExportSig<'a, Loc>,
    index: Index<Pattern<Index<Loc>>>,
) -> &'a Pattern<Index<Loc>> {
    export_sig.patterns.get(index)
}

fn pattern_def_of_index<'a, Loc>(
    export_sig: &'a ExportSig<'a, Loc>,
    index: Index<Packed<Index<Loc>>>,
) -> &'a Packed<Index<Loc>> {
    export_sig.pattern_defs.get(index)
}

fn module_ref_of_index<'a, Loc>(
    export_sig: &'a ExportSig<'a, Loc>,
    index: Index<FlowImportSpecifier>,
) -> &'a Userland {
    export_sig.module_refs.get(index)
}

mod eval {
    use super::*;

    #[derive(Debug, Clone)]
    pub enum Evaled<'a, Loc> {
        Annot(&'a PackedAnnot<Index<Loc>>, Option<&'a FlowSmolStr>),
        Value(&'a PackedValue<Index<Loc>>, Option<&'a FlowSmolStr>),
        ClassDecl(&'a FlowSmolStr),
        EnumDecl(&'a FlowSmolStr),
        ComponentDecl(&'a FlowSmolStr),
        Nothing,
    }

    impl<'a, Loc> Evaled<'a, Loc> {
        pub fn opt_name(&self) -> Option<&'a FlowSmolStr> {
            match self {
                Evaled::ClassDecl(name) | Evaled::EnumDecl(name) | Evaled::ComponentDecl(name) => {
                    Some(name)
                }
                Evaled::Annot(_, name_opt) | Evaled::Value(_, name_opt) => *name_opt,
                Evaled::Nothing => None,
            }
        }
    }

    fn seen_ref<Loc>(seen: &mut HashSet<usize>, r: &PackedRef<Index<Loc>>) -> bool {
        match r {
            PackedRef::LocalRef(deref!(PackedRefLocal { index, .. })) => {
                let index_val = index.as_usize();
                let dupe = seen.contains(&index_val);
                seen.insert(index_val);
                dupe
            }
            _ => false,
        }
    }

    fn field_of_obj_props<'a, Loc>(
        name: &str,
        props: &'a BTreeMap<FlowSmolStr, ObjValueProp<Index<Loc>, Packed<Index<Loc>>>>,
    ) -> Option<&'a Packed<Index<Loc>>> {
        // Looks up an object field by name. Returns None if the name doesn't exist or isn't a field.
        match props.get(name)? {
            ObjValueProp::ObjValueField(deref!((_, field, _))) => Some(field),
            // Accessors and methods don't have any sub-properties to contribute
            ObjValueProp::ObjValueAccess(_) | ObjValueProp::ObjValueMethod(_) => None,
        }
    }

    pub fn pattern<'a, Loc>(
        export_sig: &'a ExportSig<Loc>,
        seen: &mut HashSet<usize>,
        p: &Pattern<Index<Loc>>,
    ) -> Evaled<'a, Loc> {
        match p {
            Pattern::PDef(index) => {
                let def = super::pattern_def_of_index(export_sig, *index);
                packed(export_sig, seen, None, def)
            }
            Pattern::PropP { name, def, .. } => {
                let pat = super::pattern_of_index(export_sig, *def);
                let evaled = pattern(export_sig, seen, pat);
                get_field(export_sig, seen, name.as_str(), &evaled)
            }
            Pattern::ComputedP { .. }
            | Pattern::UnsupportedLiteralP(_)
            | Pattern::ObjRestP { .. }
            | Pattern::IndexP { .. }
            | Pattern::ArrRestP { .. } => {
                // TODO?
                Evaled::Nothing
            }
        }
    }

    pub fn tyref<'a, Loc>(
        export_sig: &'a ExportSig<Loc>,
        seen: &mut HashSet<usize>,
        name: Option<&'a FlowSmolStr>,
        r: &TyRef<Index<Loc>>,
    ) -> Evaled<'a, Loc> {
        match (r, name) {
            // This case is a heuristic to identify React.AbstractComponent exports, which can be used as types
            (
                TyRef::Qualified(deref!(TyRefQualified {
                    name: tyname,
                    qualification,
                    ..
                })),
                Some(n),
            ) if (tyname.as_str() == "AbstractComponent" || tyname.as_str() == "ComponentType")
                && matches!(
                    **qualification,
                    TyRef::Unqualified(PackedRef::RemoteRef(_) | PackedRef::BuiltinRef(_))
                ) =>
            {
                Evaled::ComponentDecl(n)
            }
            (TyRef::Qualified(deref!(TyRefQualified { qualification, .. })), _) => {
                match tyref(export_sig, seen, None, qualification) {
                    Evaled::Annot(_, _) => {
                        // TODO: get `_qual._name`
                        Evaled::Nothing
                    }
                    Evaled::Value(_, _) => {
                        // TODO: get `_qual._name`
                        Evaled::Nothing
                    }
                    Evaled::ClassDecl(n) => Evaled::ClassDecl(n),
                    Evaled::EnumDecl(n) => Evaled::EnumDecl(n),
                    Evaled::ComponentDecl(n) => Evaled::ComponentDecl(n),
                    Evaled::Nothing => Evaled::Nothing,
                }
            }
            (TyRef::Unqualified(r), _) => ref_(export_sig, seen, r),
        }
    }

    pub fn ref_<'a, Loc>(
        export_sig: &'a ExportSig<Loc>,
        seen: &mut HashSet<usize>,
        r: &PackedRef<Index<Loc>>,
    ) -> Evaled<'a, Loc> {
        let dupe = seen_ref(seen, r);
        if dupe {
            return Evaled::Nothing;
        }

        match r {
            PackedRef::LocalRef(deref!(PackedRefLocal { index, .. })) => {
                let d = super::local_def_of_index(export_sig, *index);
                def(export_sig, seen, d)
            }
            // TODO: remember these cross-module aliases. if the remote thing matches,
            // we can also suggest everything that aliases to it.
            PackedRef::RemoteRef(_) | PackedRef::BuiltinRef(_) => Evaled::Nothing,
        }
    }

    // [def type_sig d] steps through variable definitions to keep walking the
    // initializer. all other definitions (like classes or functions) do not contribute
    // any exported names, so we don't need to walk them.
    //
    // so, for [var x = y], returns [Some y]; for all other definitions, returns [None].
    pub fn def<'a, Loc>(
        export_sig: &'a ExportSig<Loc>,
        seen: &mut HashSet<usize>,
        d: &'a PackedDef<Index<Loc>>,
    ) -> Evaled<'a, Loc> {
        match d {
            Def::Variable(inner) => packed(export_sig, seen, Some(&inner.name), &inner.def),
            Def::Parameter(inner) => packed(export_sig, seen, Some(&inner.name), &inner.def),
            Def::TypeAlias(inner) => packed(export_sig, seen, Some(&inner.name), &inner.body),
            Def::ClassBinding(inner) => Evaled::ClassDecl(&inner.name),
            Def::DeclareClassBinding(inner) => Evaled::ClassDecl(&inner.name),
            Def::EnumBinding(inner) => Evaled::EnumDecl(&inner.name),
            Def::DisabledEnumBinding(inner) => Evaled::EnumDecl(&inner.name),
            Def::ComponentBinding(inner) => Evaled::ComponentDecl(&inner.name),
            Def::DisabledComponentBinding(inner) => Evaled::ComponentDecl(&inner.name),
            Def::RecordBinding(inner) => Evaled::ClassDecl(&inner.name),
            Def::DisabledRecordBinding(inner) => Evaled::ClassDecl(&inner.name),
            // None of these contain anything that can be imported separately. For example,
            // you can't `import {someMethod} ...` from an exported class.
            Def::Interface(_)
            | Def::FunBinding(_)
            | Def::DeclareFun(_)
            | Def::NamespaceBinding(_)
            | Def::OpaqueType(_) => Evaled::Nothing,
        }
    }

    pub fn packed<'a, Loc>(
        export_sig: &'a ExportSig<Loc>,
        seen: &mut HashSet<usize>,
        name: Option<&'a FlowSmolStr>,
        p: &'a Packed<Index<Loc>>,
    ) -> Evaled<'a, Loc> {
        match p {
            Packed::Value(deref!(x)) => Evaled::Value(x, name),
            Packed::Annot(deref!(a @ PackedAnnot::Typeof(deref!(AnnotTypeof { qname, .. }))))
                if qname.len() == 1 =>
            {
                let typeof_name = &qname[0];
                Evaled::Annot(a, Some(name.unwrap_or(typeof_name)))
            }
            Packed::Annot(deref!(PackedAnnot::ComponentAnnot(_))) if name.is_some() => {
                Evaled::ComponentDecl(name.unwrap())
            }
            Packed::Annot(deref!(x)) => Evaled::Annot(x, name),
            Packed::Ref(r) => ref_(export_sig, seen, r),
            Packed::TyRef(r) => tyref(export_sig, seen, name, r),
            Packed::TyRefApp(deref!(PackedTyRefApp {
                name: tyref_name, ..
            })) => tyref(export_sig, seen, name, tyref_name),
            Packed::Eval(deref!(PackedEval { packed: x, op, .. })) => eval(export_sig, seen, x, op),
            Packed::Pattern(index) => {
                let pat = super::pattern_of_index(export_sig, *index);
                pattern(export_sig, seen, pat)
            }
            Packed::Require(_) | Packed::ImportDynamic(_) => {
                // TODO: remember these cross-module aliases. if the remote thing matches,
                // we can also suggest everything that aliases to it.
                Evaled::Nothing
            }
            Packed::Err(_) => Evaled::Nothing,
            // TODO?
            Packed::ModuleRef(_) | Packed::AsyncVoidReturn(_) | Packed::ImportTypeAnnot(_) => {
                Evaled::Nothing
            }
        }
    }

    fn eval<'a, Loc>(
        export_sig: &'a ExportSig<Loc>,
        seen: &mut HashSet<usize>,
        x: &'a Packed<Index<Loc>>,
        op: &Op<Box<Packed<Index<Loc>>>>,
    ) -> Evaled<'a, Loc> {
        match op {
            Op::GetProp(name) => {
                let evaled = packed(export_sig, seen, None, x);
                get_field(export_sig, seen, name.as_str(), &evaled)
            }
            // TODO?
            _ => Evaled::Nothing,
        }
    }

    // [get_field type_sig name evaled] destructures an object pattern like
    // [let { name } = evaled], returning [Some evaled.name] if [evaled] is an
    // object AND it has a [name] field; [None] otherwise.
    fn get_field<'a, Loc>(
        export_sig: &'a ExportSig<Loc>,
        seen: &mut HashSet<usize>,
        name: &str,
        evaled: &Evaled<'a, Loc>,
    ) -> Evaled<'a, Loc> {
        match evaled {
            Evaled::Value(PackedValue::ObjLit(deref!(ValueObjLit { props, .. })), _)
            | Evaled::Value(
                PackedValue::DeclareModuleImplicitlyExportedObject(
                    deref!(ValueDeclareModuleImplicitlyExportedObject { props, .. }),
                ),
                _,
            ) => match field_of_obj_props(name, props) {
                Some(p) => packed(export_sig, seen, None, p),
                None => Evaled::Nothing,
            },
            // TODO
            Evaled::Value(PackedValue::ObjSpreadLit(_), _) => Evaled::Nothing,
            // TODO?
            Evaled::Annot(_, _) => Evaled::Nothing,
            Evaled::Value(
                PackedValue::ClassExpr(_)
                | PackedValue::FunExpr(_)
                | PackedValue::StringVal(_)
                | PackedValue::StringLit(..)
                | PackedValue::TemplateLiteral(..)
                | PackedValue::NumberVal(_)
                | PackedValue::NumberLit(..)
                | PackedValue::BooleanVal(_)
                | PackedValue::BooleanLit(..)
                | PackedValue::UniqueSymbol(_)
                | PackedValue::NullLit(..)
                | PackedValue::EmptyConstArrayLit(..)
                | PackedValue::ArrayLit(..)
                | PackedValue::BigIntVal(_)
                | PackedValue::BigIntLit(..)
                | PackedValue::AsConst(_),
                _,
            ) => Evaled::Nothing,
            Evaled::ClassDecl(_)
            | Evaled::EnumDecl(_)
            | Evaled::ComponentDecl(_)
            | Evaled::Nothing => Evaled::Nothing,
        }
    }
}

// [is_typeish def] determines if [def] is a class or enum, whose type is also exported because
// classes and enums are both values and types.
fn is_typeish<Loc>(evaled: &eval::Evaled<'_, Loc>) -> bool {
    matches!(
        evaled,
        eval::Evaled::ClassDecl(_) | eval::Evaled::EnumDecl(_) | eval::Evaled::ComponentDecl(_)
    )
}

// [add_named_type acc name def] adds [NamedType name] to [acc] if [def] is a
// class or enum, since its type is also exported because classes and enums are both
// values and types.
fn add_named_type<Loc>(acc: &mut Exports, name: &FlowSmolStr, evaled: &eval::Evaled<'_, Loc>) {
    if is_typeish(evaled) {
        acc.0.push(Export::NamedType(name.dupe()));
    }
}

// [add_default_type acc def] adds [DefaultType] to [acc] if [def] is a class or enum,
// since its type is also exported because classes and enums are both values and types.
fn add_default_type<Loc>(
    acc: &mut Exports,
    name_opt: Option<&FlowSmolStr>,
    evaled: &eval::Evaled<'_, Loc>,
) {
    if is_typeish(evaled) {
        acc.0.push(Export::DefaultType(name_opt.map(|n| n.dupe())));
    }
}

fn empty_seen() -> HashSet<usize> {
    HashSet::new()
}

mod esm {
    use super::*;

    fn fold_name<Loc>(
        export_sig: &ExportSig<Loc>,
        acc: &mut Exports,
        name: &FlowSmolStr,
        value: &TypeSigExport<Index<Loc>>,
    ) {
        match value {
            TypeSigExport::ExportRef(r) => {
                let mut seen = empty_seen();
                let evaled = eval::ref_(export_sig, &mut seen, r);
                add_named_type(acc, name, &evaled);
                acc.0.push(Export::Named(name.dupe()));
            }
            TypeSigExport::ExportBinding(index) => {
                let def = local_def_of_index(export_sig, *index);
                let mut seen = empty_seen();
                let evaled = eval::def(export_sig, &mut seen, def);
                add_named_type(acc, name, &evaled);
                acc.0.push(Export::Named(name.dupe()));
            }
            TypeSigExport::ExportDefault(deref!(ExportDefaultData { def, .. })) => {
                let mut seen = empty_seen();
                let evaled = eval::packed(export_sig, &mut seen, None, def);
                let opt_name = evaled.opt_name();
                add_default_type(acc, opt_name, &evaled);
                acc.0.push(Export::Default(opt_name.map(|n| n.dupe())));
            }
            TypeSigExport::ExportDefaultBinding(deref!(ExportDefaultBindingData { index, .. })) => {
                let def = local_def_of_index(export_sig, *index);
                let mut seen = empty_seen();
                let evaled = eval::def(export_sig, &mut seen, def);
                let opt_name = evaled.opt_name();
                add_default_type(acc, opt_name, &evaled);
                acc.0.push(Export::Default(opt_name.map(|n| n.dupe())));
            }
            TypeSigExport::ExportFrom { .. } => {
                // ExportFrom defines aliases. We decide to depend on autocomplete_ranked_by_usage to pick
                // the right one in most cases.
                // TODO: We cannot resolve to the underlying definition here from another module, so we don't
                // know whether the binding can also be used as a type.
                acc.0.push(Export::Named(name.dupe()));
            }
        }
    }

    fn fold_type<Loc>(acc: &mut Exports, name: &FlowSmolStr, value: &TypeExport<Index<Loc>>) {
        match value {
            TypeExport::ExportTypeRef { .. }
            | TypeExport::ExportTypeBinding { .. }
            | TypeExport::ExportTypeFrom { .. } => {
                acc.0.push(Export::NamedType(name.dupe()));
            }
        }
    }

    pub fn exports<Loc>(
        export_sig: &ExportSig<Loc>,
        type_exports: &[TypeExport<Index<Loc>>],
        exports: &[TypeSigExport<Index<Loc>>],
        info: &ESModuleInfo<Index<Loc>>,
    ) -> Exports {
        let ESModuleInfo {
            type_export_keys,
            export_keys,
            type_stars,
            stars,
            ts_pending_keys,
            ..
        } = info;

        let mut acc = Exports(Vec::new());

        for (_, module_index) in stars {
            let module_ref = module_ref_of_index(export_sig, *module_index);
            acc.0.push(Export::ReExportModule(module_ref.dupe()));
        }

        for (_, module_index) in type_stars {
            let module_ref = module_ref_of_index(export_sig, *module_index);
            acc.0.push(Export::ReExportModule(module_ref.dupe()));
        }

        for (key, value) in export_keys.iter().zip(exports.iter()) {
            fold_name(export_sig, &mut acc, key, value);
        }

        // ts_pending entries are .ts exports whose value-vs-type status can't be
        // determined at parse time (it's deferred to merge). Emit both Named and
        // NamedType so auto-import consumers can suggest the name regardless of
        // whether the resolved export turns out to be a value or a type.
        for name in ts_pending_keys {
            acc.0.push(Export::Named(name.dupe()));
            acc.0.push(Export::NamedType(name.dupe()));
        }

        for (key, value) in type_export_keys.iter().zip(type_exports.iter()) {
            fold_type(&mut acc, key, value);
        }

        acc
    }
}

mod cjs {
    use super::*;

    fn exports_of_value<Loc>(
        acc: &mut Exports,
        export_sig: &ExportSig<Loc>,
        value: &PackedValue<Index<Loc>>,
    ) {
        match value {
            PackedValue::ObjLit(deref!(ValueObjLit { props, .. }))
            | PackedValue::DeclareModuleImplicitlyExportedObject(
                deref!(ValueDeclareModuleImplicitlyExportedObject { props, .. }),
            ) => {
                // only property names that are valid identifier names can currently be
                // imported: `module.exports = { "Foo Bar": true }` cannot be imported
                // as `import { "Foo Bar" as Foo_bar } ...` yet. This will be allowed by
                // https://github.com/tc39/ecma262/pull/2154; until then, we only bother
                // indexing names that can actually be imported.
                for (name, obj_value) in props {
                    if string_is_valid_identifier_name(name.as_str()) {
                        match obj_value {
                            ObjValueProp::ObjValueField(deref!((
                                _,
                                Packed::Value(deref!(PackedValue::ClassExpr(_))),
                                _,
                            ))) => {
                                acc.0.push(Export::NamedType(name.dupe()));
                            }
                            ObjValueProp::ObjValueField(deref!((_, Packed::Ref(r), _))) => {
                                let mut seen = empty_seen();
                                let evaled = eval::ref_(export_sig, &mut seen, r);
                                add_named_type(acc, name, &evaled);
                            }
                            _ => {}
                        }
                        acc.0.push(Export::Named(name.dupe()));
                    }
                }
            }
            PackedValue::AsConst(deref!(v)) => exports_of_value(acc, export_sig, v),
            _ => {}
        }
    }

    fn exports_of_annot<Loc>(acc: &mut Exports, annot: &PackedAnnot<Index<Loc>>) {
        match annot {
            PackedAnnot::ObjAnnot(deref!(AnnotObjAnnot { props, .. })) => {
                for name in props.keys() {
                    acc.0.push(Export::Named(name.dupe()));
                }
            }
            _ => {}
        }
    }

    fn add_named_exports<Loc>(
        acc: &mut Exports,
        export_sig: &ExportSig<Loc>,
        packed: &Packed<Index<Loc>>,
    ) {
        let mut seen = empty_seen();
        let evaled = eval::packed(export_sig, &mut seen, None, packed);
        match evaled {
            eval::Evaled::Annot(annot, _) => exports_of_annot(acc, annot),
            eval::Evaled::Value(value, _) => exports_of_value(acc, export_sig, value),
            eval::Evaled::ClassDecl(_)
            | eval::Evaled::EnumDecl(_)
            | eval::Evaled::ComponentDecl(_)
            | eval::Evaled::Nothing => {}
        }
    }

    fn add_default_exports<Loc>(
        export_sig: &ExportSig<Loc>,
        acc: &mut Exports,
        module_exports: Option<&Packed<Index<Loc>>>,
    ) {
        match module_exports {
            Some(exports) => {
                acc.0.push(Export::Default(None));
                add_named_exports(acc, export_sig, exports);
            }
            None => {}
        }
    }

    fn fold_type<Loc>(acc: &mut Exports, name: &FlowSmolStr, value: &TypeExport<Index<Loc>>) {
        match value {
            TypeExport::ExportTypeRef { .. }
            | TypeExport::ExportTypeBinding { .. }
            | TypeExport::ExportTypeFrom { .. } => {
                acc.0.push(Export::NamedType(name.dupe()));
            }
        }
    }

    pub fn exports<Loc>(
        export_sig: &ExportSig<Loc>,
        type_exports: &[TypeExport<Index<Loc>>],
        exports: Option<&Packed<Index<Loc>>>,
        info: &CJSModuleInfo<Index<Loc>>,
    ) -> Exports {
        let CJSModuleInfo {
            type_export_keys,
            type_stars,
            ..
        } = info;

        let mut acc = Exports(Vec::new());

        for (_, module_index) in type_stars {
            let module_ref = module_ref_of_index(export_sig, *module_index);
            acc.0.push(Export::ReExportModule(module_ref.dupe()));
        }

        add_default_exports(export_sig, &mut acc, exports);

        for (key, value) in type_export_keys.iter().zip(type_exports.iter()) {
            fold_type(&mut acc, key, value);
        }

        acc
    }
}

fn of_sig<Loc>(export_sig: &ExportSig<Loc>) -> Exports {
    match &export_sig.module_kind {
        Some(ModuleKind::ESModule {
            type_exports,
            exports,
            ts_pending: _,
            info,
        }) => esm::exports(export_sig, type_exports, exports, info),
        Some(ModuleKind::CJSModule {
            type_exports,
            exports,
            info,
        }) => cjs::exports(export_sig, type_exports, exports.as_ref(), info),
        None => Exports(Vec::new()),
    }
}

fn add_global<Loc>(
    global_sig: &ExportSig<Loc>,
    acc: &mut Exports,
    name: &FlowSmolStr,
    index: Index<PackedDef<Index<Loc>>>,
) {
    fn add_named(acc: &mut Exports, name: &FlowSmolStr) {
        if name.contains('$') {
            return;
        }
        acc.0.push(Export::Named(name.dupe()));
    }

    let def = local_def_of_index(global_sig, index);
    match def {
        Def::Variable(_) | Def::Parameter(_) => {
            let mut seen = empty_seen();
            let evaled = eval::def(global_sig, &mut seen, def);
            add_named_type(acc, name, &evaled);
            add_named(acc, name);
        }
        Def::FunBinding(_)
        | Def::DeclareFun(_)
        | Def::ComponentBinding(_)
        | Def::DisabledComponentBinding(_)
        | Def::NamespaceBinding(_) => add_named(acc, name),
        Def::TypeAlias(_) | Def::Interface(_) | Def::OpaqueType(_) => {
            acc.0.push(Export::NamedType(name.dupe()));
        }
        Def::ClassBinding(_)
        | Def::DeclareClassBinding(_)
        | Def::RecordBinding(_)
        | Def::DisabledRecordBinding(_)
        | Def::EnumBinding(_)
        | Def::DisabledEnumBinding(_) => {
            acc.0.push(Export::NamedType(name.dupe()));
            add_named(acc, name);
        }
    }
}

pub fn of_module<Loc>(module: &Module<Loc>) -> Exports {
    let export_sig = ExportSig::of_module(module);
    of_sig(&export_sig)
}

pub fn of_builtins<Loc>(builtins: &Builtins<Loc>) -> Exports {
    let global_sig = ExportSig::of_builtins(builtins);

    let mut acc = Exports(Vec::new());

    for (name, index) in &builtins.global_values {
        add_global(&global_sig, &mut acc, name, *index);
    }

    for (name, index) in &builtins.global_types {
        add_global(&global_sig, &mut acc, name, *index);
    }

    for (name, module_def) in &builtins.global_modules {
        let export_sig = ExportSig {
            module_kind: Some(&module_def.module_kind),
            module_refs: &builtins.module_refs,
            local_defs: &builtins.local_defs,
            pattern_defs: &builtins.pattern_defs,
            patterns: &builtins.patterns,
        };
        let module_exports = of_sig(&export_sig);
        acc.0.push(Export::Module(
            FlowImportSpecifier::userland(name.dupe()),
            module_exports.0,
        ));
    }

    acc
}
