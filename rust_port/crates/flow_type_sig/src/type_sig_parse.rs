/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// The parsing phase performs a single pass over the AST, building a parsed
// signature. The parsed signature includes unresolved references which will be
// resolved in a separate pass.
//
// When we see a definition in this pass, we make a note of the name, but do not
// eagerly visit it's body. Instead, we visit definition bodies lazily during
// the marking pass, which walks the graph of references starting at exports.
//
// This lazy walk is an optimization, to avoid walking parts of the AST which
// are not reachable from the exports of a module.

use std::cell::Cell;
use std::cell::OnceCell;
use std::cell::RefCell;
use std::cell::UnsafeCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ops::Deref;
use std::rc::Rc;

use dupe::Dupe;
use flow_common::alpha_rename;
use flow_common::flow_import_specifier::Userland;
use flow_common::platform_set::PlatformSet;
use flow_common::polarity::Polarity;
use flow_common::type_strictness::TypeStrictnessKind;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::IdentifierInner;
use flow_parser::ast::expression::ExpressionInner;
use flow_parser::ast::statement::StatementInner;
use flow_parser::ast::types::TypeInner;
use flow_parser::ast_utils;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::jsdoc;
use flow_parser::loc::Loc;
use flow_parser_utils::enum_validate;
use flow_parser_utils::graphql;
use flow_parser_utils::object_type_key;
use flow_parser_utils::object_type_key::ObjectTypeKeyForm;
use flow_parser_utils::record_utils;
use flow_parser_utils::signature_utils;
use flow_parser_utils::symbol_call;
use flow_parser_utils::type_param_analysis::analyze_type_params;
use flow_parser_utils::type_param_analysis::type_param_order;
use vec1::Vec1;

use crate::compact_table::Builder;
use crate::compact_table::InternedBuilder;
use crate::compact_table::Node;
use crate::expected_annotation_sort::ExpectedAnnotationSort;
use crate::signature_error;
use crate::type_sig::*;
use crate::type_sig_options::TypeSigOptions;

mod tparam_stack {
    use std::collections::HashMap;

    use dupe::Dupe;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use vec1::Vec1;

    struct ReferenceContext {
        frame_index: usize,
        allow_forward: bool,
    }

    pub(super) struct TParamStack {
        // The Boolean records whether a parameter is available to defaults.
        // Bounds may still reference an uninitialized sibling when the active
        // reference context allows forward references.
        frames: Vec1<HashMap<FlowSmolStr, bool>>,
        reference_contexts: Vec<ReferenceContext>,
    }

    impl TParamStack {
        pub(super) fn new() -> Self {
            TParamStack {
                frames: Vec1::new(HashMap::new()),
                reference_contexts: Vec::new(),
            }
        }

        pub(super) fn push_new_frame(&mut self) {
            self.frames.push(HashMap::new());
        }

        pub(super) fn pop_frame(&mut self) {
            self.frames.pop().unwrap();
        }

        pub(super) fn insert(&mut self, name: FlowSmolStr) {
            self.frames.last_mut().insert(name, true);
        }

        pub(super) fn insert_uninitialized(&mut self, name: FlowSmolStr) {
            self.frames.last_mut().insert(name, false);
        }

        pub(super) fn initialize(&mut self, name: &FlowSmolStr) {
            *self
                .frames
                .last_mut()
                .get_mut(name)
                .expect("type parameter should belong to the current frame") = true;
        }

        pub(super) fn contains(&self, name: &FlowSmolStr) -> bool {
            self.frames
                .iter()
                .rev()
                .any(|frame| frame.contains_key(name))
        }

        pub(super) fn begin_references(&mut self, allow_forward: bool) {
            self.reference_contexts.push(ReferenceContext {
                frame_index: self.frames.len() - 1,
                allow_forward,
            });
        }

        pub(super) fn finish_references(&mut self) {
            self.reference_contexts
                .pop()
                .expect("a type parameter reference context should be active");
        }

        pub(super) fn reference(&mut self, name: &FlowSmolStr) -> Option<bool> {
            for (frame_index, frame) in self.frames.iter().enumerate().rev() {
                if let Some(initialized) = frame.get(name) {
                    if let Some(context) = self
                        .reference_contexts
                        .iter()
                        .rev()
                        .find(|context| context.frame_index == frame_index)
                    {
                        return Some(context.allow_forward || *initialized);
                    }
                    return Some(*initialized);
                }
            }
            None
        }

        /// Temporarily remove [name] from all frames; returns the frames it was
        /// present in so the caller can restore it after descending into a
        /// nested scope. Mirrors OCaml's [SSet.remove], which produces
        /// a new immutable set scoped to the recursive call only.
        pub(super) fn remove_all(&mut self, name: &FlowSmolStr) -> Vec<(usize, bool)> {
            let mut removed = Vec::new();
            for (i, frame) in self.frames.iter_mut().enumerate() {
                if let Some(entry) = frame.remove(name) {
                    removed.push((i, entry));
                }
            }
            removed
        }

        /// Restore [name] in the frames previously recorded by [remove_all].
        pub(super) fn restore_all(&mut self, name: &FlowSmolStr, entries: Vec<(usize, bool)>) {
            for (i, entry) in entries {
                self.frames[i].insert(name.dupe(), entry);
            }
        }
    }
}

// This type encodes the fixed point of parsed signatures. Of particular note
// here are the Ref and Err constructors.
//
// The Ref constructor encodes some name lookup which will be resolved in the
// mark phase. The parsing phase builds up a scope as it descends into the AST,
// which can be used to resolve the names to a definition once the program has
// been fully parsed.
//
// The Err constructor encodes some failure in the parsing phase. These failures
// are detected while visiting the AST, and thus are encoded directly in the
// signature. The compaction step is responsible for collating these errors.
#[derive(Clone)]
pub(super) enum ValRefLookup {
    ValueRefLookup,
    NamespaceValueRefLookup(Cell<Polarity>),
    TypeRefLookup,
    TypeofRefLookup,
}

#[derive(Clone)]
pub(super) enum Parsed<'arena, 'ast> {
    Annot(Box<ParsedAnnot<'arena, 'ast>>),
    Value(Box<ParsedValue<'arena, 'ast>>),
    TyRef(TyName<'arena, 'ast>),
    TyRefApp {
        loc: LocNode<'arena>,
        name: TyName<'arena, 'ast>,
        targs: Vec<Parsed<'arena, 'ast>>,
    },
    AsyncVoidReturn(LocNode<'arena>),
    ValRef {
        lookup: ValRefLookup,
        ref_: Ref<'arena, 'ast>,
    },
    Err(LocNode<'arena>, Errno<LocNode<'arena>>),
    BuiltinTyRef {
        ref_loc: LocNode<'arena>,
        name: FlowSmolStr,
    },
    Pattern(PatternNode<'arena, 'ast>),
    Eval(
        LocNode<'arena>,
        Box<Parsed<'arena, 'ast>>,
        Box<Op<Parsed<'arena, 'ast>>>,
    ),
    Require {
        loc: LocNode<'arena>,
        mref: ModuleRefNode<'arena>,
    },
    ImportDynamic {
        loc: LocNode<'arena>,
        mref: ModuleRefNode<'arena>,
    },
    ModuleRef {
        loc: LocNode<'arena>,
        mref: ModuleRefNode<'arena>,
    },
    ImportTypeAnnot {
        loc: LocNode<'arena>,
        mref: ModuleRefNode<'arena>,
    },
}

#[derive(Clone)]
pub(super) enum TyName<'arena, 'ast> {
    Unqualified(Box<Ref<'arena, 'ast>>),
    Qualified {
        loc: LocNode<'arena>,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        qualification: Box<TyName<'arena, 'ast>>,
    },
}

pub(super) type ParsedValue<'arena, 'ast> = Value<LocNode<'arena>, Parsed<'arena, 'ast>>;

pub(super) type ParsedAnnot<'arena, 'ast> = Annot<LocNode<'arena>, Parsed<'arena, 'ast>>;

pub(super) type ParsedDef<'arena, 'ast> = Def<LocNode<'arena>, Parsed<'arena, 'ast>>;

#[derive(Clone)]
pub(super) enum Export<'arena, 'ast> {
    ExportRef(Ref<'arena, 'ast>),
    ExportBinding(LocalDefNode<'arena, 'ast>),
    ExportDefault {
        default_loc: LocNode<'arena>,
        def: Parsed<'arena, 'ast>,
    },
    ExportDefaultBinding {
        default_loc: LocNode<'arena>,
        #[expect(dead_code)]
        name: FlowSmolStr,
        binding: LocalDefNode<'arena, 'ast>,
    },
    ExportFrom(RemoteRefNode<'arena>),
}

#[derive(Clone)]
pub(super) enum ExportType<'arena, 'ast> {
    ExportTypeRef(Ref<'arena, 'ast>),
    ExportTypeBinding(LocalDefNode<'arena, 'ast>),
    ExportTypeFrom(RemoteRefNode<'arena>),
}

#[derive(Clone)]
pub(super) enum TsPendingExport<'arena, 'ast> {
    TsExportRef {
        export_loc: LocNode<'arena>,
        ref_: Ref<'arena, 'ast>,
        import_provenance: Option<(ModuleRefNode<'arena>, FlowSmolStr)>,
    },
    TsExportFrom {
        export_loc: LocNode<'arena>,
        mref: ModuleRefNode<'arena>,
        remote_name: FlowSmolStr,
    },
}

#[derive(Clone)]
pub(super) enum ModuleKind<'arena, 'ast> {
    UnknownModule,
    CJSModule(Parsed<'arena, 'ast>),
    CJSModuleProps {
        props: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
        strictness_kind: TypeStrictnessKind,
    },
    CJSDeclareModule {
        props: BTreeMap<FlowSmolStr, LocalDefNode<'arena, 'ast>>,
        strictness_kind: TypeStrictnessKind,
    },
    ESModule {
        names: BTreeMap<FlowSmolStr, Export<'arena, 'ast>>,
        stars: Vec<(LocNode<'arena>, ModuleRefNode<'arena>)>,
    },
}

#[derive(Clone)]
pub(super) struct Ref<'arena, 'ast> {
    pub(super) ref_loc: LocNode<'arena>,
    pub(super) name: FlowSmolStr,
    pub(super) scope: ScopeId,
    pub(super) resolved: OnceCell<Option<BindingNode<'arena, 'ast>>>,
}

#[derive(Clone)]
pub(super) struct Exports<'arena, 'ast> {
    pub(super) kind: ModuleKind<'arena, 'ast>,
    pub(super) types: BTreeMap<FlowSmolStr, ExportType<'arena, 'ast>>,
    pub(super) type_stars: Vec<(LocNode<'arena>, ModuleRefNode<'arena>)>,
    pub(super) ts_pending: BTreeMap<FlowSmolStr, TsPendingExport<'arena, 'ast>>,
    pub(super) strict: bool,
    pub(super) platform_availability_set: Option<PlatformSet>,
}

// When resolving names in the next phase, it will become possible to visit a
// given binding multiple times, or even circularly. The node indirection here,
// provided by the Compact_table module, deals with these cycles automatically.
//
// Bodies of definitions are stored as lazy thunks, to avoid parsing things
// which are not reachable from exports.
pub(super) enum LocalBinding<'arena, 'ast> {
    TypeBinding {
        id_loc: LocNode<'arena>,
        def: Lazy<'arena, 'ast, ParsedDef<'arena, 'ast>>,
    },
    VarBinding {
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Lazy<'arena, 'ast, Parsed<'arena, 'ast>>,
    },
    LetConstBinding {
        is_const: bool,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Lazy<'arena, 'ast, Parsed<'arena, 'ast>>,
    },
    ParamBinding {
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Lazy<'arena, 'ast, Parsed<'arena, 'ast>>,
        tparams: TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
        strictness_kind: TypeStrictnessKind,
    },
    ConstRefBinding {
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        ref_: Ref<'arena, 'ast>,
    },
    ConstFunBinding {
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        loc: LocNode<'arena>,
        async_: bool,
        generator: bool,
        def: Lazy<'arena, 'ast, FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        statics: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
    },
    ClassBinding {
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Lazy<'arena, 'ast, ClassSig<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        namespace_values: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
        namespace_types: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
    },
    DeclareClassBinding {
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Lazy<'arena, 'ast, DeclareClassSig<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        namespace_values: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
        namespace_types: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
    },
    RecordBinding {
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Option<Lazy<'arena, 'ast, ClassSig<LocNode<'arena>, Parsed<'arena, 'ast>>>>,
        defaulted_props: std::collections::BTreeSet<FlowSmolStr>,
    },
    FunBinding {
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        async_: bool,
        generator: bool,
        fn_loc: LocNode<'arena>,
        def: Lazy<'arena, 'ast, FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        statics: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
        namespace_types: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
    },
    DeclareFunBinding {
        name: FlowSmolStr,
        defs: Vec<(
            LocNode<'arena>,
            LocNode<'arena>,
            Lazy<'arena, 'ast, FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        )>,
        statics: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
        namespace_types: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
    },
    ComponentBinding {
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        fn_loc: LocNode<'arena>,
        def: Option<Lazy<'arena, 'ast, ComponentSig<LocNode<'arena>, Parsed<'arena, 'ast>>>>,
    },
    EnumBinding {
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Option<
            Lazy<
                'arena,
                'ast,
                (
                    Option<EnumRep>,
                    BTreeMap<FlowSmolStr, LocNode<'arena>>,
                    Option<BTreeMap<FlowSmolStr, TsEnumMemberValue>>,
                    bool,
                ),
            >,
        >,
    },
    NamespaceBinding {
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        values: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
        types: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
    },
}

pub(super) struct Lazy<'arena, 'ast, T>(UnsafeCell<LazyInner<'arena, 'ast, T>>);

impl<'arena, 'ast, T> Lazy<'arena, 'ast, T> {
    fn new(
        v: Box<
            dyn FnOnce(
                    &TypeSigOptions,
                    &mut scope::Scopes<'arena, 'ast>,
                    &mut Tables<'arena, 'ast>,
                ) -> T
                + 'ast,
        >,
    ) -> Self {
        Self(UnsafeCell::new(LazyInner::Lazy(v)))
    }

    pub(super) fn get_forced(
        &self,
        opts: &TypeSigOptions,
        scopes: &mut scope::Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
    ) -> &T {
        // SAFETY: there are no other references that point to the *contents* of this `UnsafeCell`.
        let mut_self = unsafe { &mut *self.0.get() };
        if matches!(mut_self, LazyInner::Lazy(_)) {
            // Suppose there are other concurrent borrows of the same lazy data type, they cannot be
            // in this case, because we already turned the inner data into either Forcing or Forced,
            // so they will all be under different state, accessing different contents.
            let inner = std::mem::replace(mut_self, LazyInner::Forcing);
            let LazyInner::Lazy(f) = inner else {
                unreachable!()
            };
            let v = f(opts, scopes, tbls);
            *mut_self = LazyInner::Forced(v);
        }
        match mut_self {
            LazyInner::Lazy(_) => unreachable!("Lazy after being forced"),
            LazyInner::Forcing => unreachable!("Forcing after being forced"),
            LazyInner::Forced(v) => v,
        }
    }

    pub(super) fn as_already_forced(&self) -> &T {
        match unsafe { &*self.0.get() } {
            LazyInner::Lazy(_) => unreachable!("Lazy after being forced"),
            LazyInner::Forcing => unreachable!("Forcing after being forced"),
            LazyInner::Forced(v) => v,
        }
    }
}

enum LazyInner<'arena, 'ast, T> {
    Lazy(
        Box<
            dyn FnOnce(
                    &TypeSigOptions,
                    &mut scope::Scopes<'arena, 'ast>,
                    &mut Tables<'arena, 'ast>,
                ) -> T
                + 'ast,
        >,
    ),
    Forcing,
    Forced(T),
}

pub(super) enum RemoteBinding<'arena> {
    ImportBinding {
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        mref: ModuleRefNode<'arena>,
        remote: FlowSmolStr,
    },
    ImportTypeBinding {
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        mref: ModuleRefNode<'arena>,
        remote: FlowSmolStr,
    },
    ImportTypeofBinding {
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        mref: ModuleRefNode<'arena>,
        remote: FlowSmolStr,
    },
    ImportNsBinding {
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        mref: ModuleRefNode<'arena>,
    },
    ImportTypeofNsBinding {
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        mref: ModuleRefNode<'arena>,
    },
    ImportTypeNsBinding {
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        mref: ModuleRefNode<'arena>,
    },
}

pub(super) enum Pattern<'arena, 'ast> {
    PDef(Lazy<'arena, 'ast, PatternDefNode<'arena, 'ast>>),
    PropP {
        def: PatternNode<'arena, 'ast>,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
    },
    ComputedP {
        def: PatternNode<'arena, 'ast>,
        elem: PatternDefNode<'arena, 'ast>,
    },
    UnsupportedLiteralP(LocNode<'arena>),
    ObjRestP {
        def: PatternNode<'arena, 'ast>,
        loc: LocNode<'arena>,
        xs: Vec<FlowSmolStr>,
    },
    IndexP {
        def: PatternNode<'arena, 'ast>,
        loc: LocNode<'arena>,
        i: usize,
    },
    ArrRestP {
        def: PatternNode<'arena, 'ast>,
        loc: LocNode<'arena>,
        i: usize,
    },
}

#[derive(Clone, Dupe)]
pub(super) enum BindingNode<'arena, 'ast> {
    LocalBinding(LocalDefNode<'arena, 'ast>),
    RemoteBinding(RemoteRefNode<'arena>),
}

#[derive(Clone, Dupe)]
pub(super) struct LocNode<'arena>(pub(super) Node<'arena, Loc>);

#[derive(Clone, Dupe)]
pub(super) struct LocalDefNode<'arena, 'ast>(pub(super) Node<'arena, LocalBinding<'arena, 'ast>>);

#[derive(Clone, Dupe)]
pub(super) struct ModuleRefNode<'arena>(pub(super) Node<'arena, Userland>);

#[derive(Clone, Dupe)]
pub(super) struct RemoteRefNode<'arena>(pub(super) Node<'arena, RemoteBinding<'arena>>);

#[derive(Clone, Dupe)]
pub(super) struct PatternDefNode<'arena, 'ast>(pub(super) Node<'arena, Parsed<'arena, 'ast>>);

#[derive(Clone, Dupe)]
pub(super) struct PatternNode<'arena, 'ast>(pub(super) Node<'arena, Pattern<'arena, 'ast>>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FrozenKind {
    NotFrozen,
    FrozenProp,
    FrozenDirect,
}

fn binding_locs<'arena>(binding: &BindingNode<'arena, '_>) -> Vec<LocNode<'arena>> {
    match binding {
        BindingNode::LocalBinding(node) => {
            let binding = node.0.data();
            match binding.deref() {
                LocalBinding::VarBinding { id_loc, .. }
                | LocalBinding::LetConstBinding { id_loc, .. }
                | LocalBinding::ConstRefBinding { id_loc, .. }
                | LocalBinding::ParamBinding { id_loc, .. }
                | LocalBinding::ConstFunBinding { id_loc, .. }
                | LocalBinding::ClassBinding { id_loc, .. }
                | LocalBinding::DeclareClassBinding { id_loc, .. }
                | LocalBinding::RecordBinding { id_loc, .. }
                | LocalBinding::FunBinding { id_loc, .. }
                | LocalBinding::ComponentBinding { id_loc, .. }
                | LocalBinding::EnumBinding { id_loc, .. }
                | LocalBinding::NamespaceBinding { id_loc, .. }
                | LocalBinding::TypeBinding { id_loc, .. } => vec![id_loc.dupe()],
                LocalBinding::DeclareFunBinding { defs, .. } => {
                    defs.iter().map(|(id_loc, _, _)| id_loc.dupe()).collect()
                }
            }
        }
        BindingNode::RemoteBinding(node) => {
            let binding = node.0.data();
            match binding.deref() {
                RemoteBinding::ImportBinding { id_loc, .. }
                | RemoteBinding::ImportTypeBinding { id_loc, .. }
                | RemoteBinding::ImportTypeofBinding { id_loc, .. }
                | RemoteBinding::ImportNsBinding { id_loc, .. }
                | RemoteBinding::ImportTypeofNsBinding { id_loc, .. }
                | RemoteBinding::ImportTypeNsBinding { id_loc, .. } => vec![id_loc.dupe()],
            }
        }
    }
}

fn loc_of_binding<'arena>(binding: &BindingNode<'arena, '_>) -> LocNode<'arena> {
    binding_locs(binding)
        .into_iter()
        .next()
        .expect("bindings should have at least one location")
}

pub(super) struct Tables<'arena, 'ast> {
    pub(super) locs: Builder<'arena, Loc>,
    pub(super) local_defs: Builder<'arena, LocalBinding<'arena, 'ast>>,
    pub(super) module_refs: InternedBuilder<'arena, Userland>,
    pub(super) remote_refs: Builder<'arena, RemoteBinding<'arena>>,
    pub(super) pattern_defs: Builder<'arena, Parsed<'arena, 'ast>>,
    pub(super) patterns: Builder<'arena, Pattern<'arena, 'ast>>,
    pub(super) additional_errors: Vec<signature_error::BindingValidation<LocNode<'arena>>>,
    pub(super) strictness_kind: TypeStrictnessKind,
}

impl<'arena, 'ast> Tables<'arena, 'ast> {
    pub(super) fn new(arena: &'arena bumpalo::Bump, strictness_kind: TypeStrictnessKind) -> Self {
        Tables {
            locs: Builder::new(arena),
            local_defs: Builder::new(arena),
            module_refs: InternedBuilder::new(arena),
            remote_refs: Builder::new(arena),
            pattern_defs: Builder::new(arena),
            patterns: Builder::new(arena),
            additional_errors: Vec::new(),
            strictness_kind,
        }
    }

    pub(super) fn set_strictness_kind(&mut self, strictness_kind: TypeStrictnessKind) {
        self.strictness_kind = strictness_kind;
    }

    // Builtin files share these tables, and a definition may be forced after a later file is parsed.
    fn lazy<T: 'ast>(
        &self,
        value: Box<
            dyn FnOnce(
                    &TypeSigOptions,
                    &mut scope::Scopes<'arena, 'ast>,
                    &mut Tables<'arena, 'ast>,
                ) -> T
                + 'ast,
        >,
    ) -> Lazy<'arena, 'ast, T>
    where
        'arena: 'ast,
    {
        let strictness_kind = self.strictness_kind;
        Lazy::new(Box::new(move |opts, scopes, tbls| {
            let previous = tbls.strictness_kind;
            tbls.strictness_kind = strictness_kind;
            let value = value(opts, scopes, tbls);
            tbls.strictness_kind = previous;
            value
        }))
    }

    pub(super) fn push_loc(&mut self, loc: Loc) -> LocNode<'arena> {
        LocNode(self.locs.push(loc))
    }

    fn push_local_def(&mut self, def: LocalBinding<'arena, 'ast>) -> LocalDefNode<'arena, 'ast> {
        LocalDefNode(self.local_defs.push(def))
    }

    fn push_module_ref(&mut self, mref: Userland) -> ModuleRefNode<'arena> {
        ModuleRefNode(self.module_refs.push(mref))
    }

    fn push_remote_ref(&mut self, rref: RemoteBinding<'arena>) -> RemoteRefNode<'arena> {
        RemoteRefNode(self.remote_refs.push(rref))
    }

    fn push_pattern_def(&mut self, def: Parsed<'arena, 'ast>) -> PatternDefNode<'arena, 'ast> {
        PatternDefNode(self.pattern_defs.push(def))
    }

    fn push_pattern(&mut self, pattern: Pattern<'arena, 'ast>) -> PatternNode<'arena, 'ast> {
        PatternNode(self.patterns.push(pattern))
    }

    fn splice<R>(&mut self, id_loc: LocNode<'arena>, f: impl FnOnce(&mut Self) -> R) -> R {
        let mut spliced = self.locs.new_with_shared_arena();
        std::mem::swap(&mut spliced, &mut self.locs);
        let x = f(self);
        std::mem::swap(&mut spliced, &mut self.locs);
        self.locs.merge_spliced(id_loc.0, spliced);
        x
    }
}

fn polarity<Loc: Dupe>(variance: Option<(Loc, ast::Variance<Loc>)>) -> Polarity {
    match variance {
        None => Polarity::Neutral,
        Some((
            _,
            ast::Variance {
                kind: ast::VarianceKind::InOut,
                ..
            },
        )) => Polarity::Neutral,
        Some((
            _,
            ast::Variance {
                kind: ast::VarianceKind::Readonly,
                ..
            },
        ))
        | Some((
            _,
            ast::Variance {
                kind: ast::VarianceKind::Out,
                ..
            },
        )) => Polarity::Positive,
        Some((
            _,
            ast::Variance {
                kind: ast::VarianceKind::In,
                ..
            },
        ))
        | Some((
            _,
            ast::Variance {
                kind: ast::VarianceKind::Writeonly,
                ..
            },
        )) => Polarity::Negative,
        Some((
            _,
            ast::Variance {
                kind: ast::VarianceKind::Plus,
                ..
            },
        )) => Polarity::Positive,
        Some((
            _,
            ast::Variance {
                kind: ast::VarianceKind::Minus,
                ..
            },
        )) => Polarity::Negative,
    }
}

fn id_name<M: Dupe, T: Dupe>(id: &ast::Identifier<M, T>) -> &FlowSmolStr {
    &id.name
}

fn val_ref<'arena, 'ast>(
    type_only: bool,
    scope: ScopeId,
    ref_loc: LocNode<'arena>,
    name: FlowSmolStr,
) -> Parsed<'arena, 'ast> {
    let lookup = if type_only {
        ValRefLookup::TypeRefLookup
    } else {
        ValRefLookup::ValueRefLookup
    };
    Parsed::ValRef {
        lookup,
        ref_: Ref {
            ref_loc,
            name,
            scope,
            resolved: OnceCell::new(),
        },
    }
}

fn namespace_value_ref<'arena, 'ast>(
    scope: ScopeId,
    ref_loc: LocNode<'arena>,
    name: FlowSmolStr,
) -> Parsed<'arena, 'ast> {
    Parsed::ValRef {
        lookup: ValRefLookup::NamespaceValueRefLookup(Cell::new(Polarity::Neutral)),
        ref_: Ref {
            ref_loc,
            name,
            scope,
            resolved: OnceCell::new(),
        },
    }
}

fn typeof_ref<'arena, 'ast>(
    scope: ScopeId,
    ref_loc: LocNode<'arena>,
    name: FlowSmolStr,
) -> Parsed<'arena, 'ast> {
    Parsed::ValRef {
        lookup: ValRefLookup::TypeofRefLookup,
        ref_: Ref {
            ref_loc,
            name,
            scope,
            resolved: OnceCell::new(),
        },
    }
}

fn merge_accessors<Loc, T>(a: Accessor<Loc, T>, b: Accessor<Loc, T>) -> Accessor<Loc, T> {
    match (a, b) {
        (Accessor::Get(box (get_loc, get_t)), Accessor::Set(box (set_loc, set_t)))
        | (Accessor::Set(box (set_loc, set_t)), Accessor::Get(box (get_loc, get_t)))
        | (Accessor::GetSet(box (get_loc, get_t, _, _)), Accessor::Set(box (set_loc, set_t)))
        | (Accessor::GetSet(box (_, _, set_loc, set_t)), Accessor::Get(box (get_loc, get_t))) => {
            Accessor::GetSet(Box::new((get_loc, get_t, set_loc, set_t)))
        }
        (_, x) => x,
    }
}

fn extract_string_literal<M: Dupe, T: Dupe>(
    expr: &ast::expression::Expression<M, T>,
) -> Option<&FlowSmolStr> {
    match expr.deref() {
        ExpressionInner::StringLiteral { inner, .. } => Some(&inner.value),
        ExpressionInner::TemplateLiteral { inner, .. } if inner.expressions.is_empty() => {
            if let [element] = &inner.quasis[..] {
                Some(&element.value.cooked)
            } else {
                None
            }
        }
        _ => None,
    }
}

impl<'arena, 'ast> Exports<'arena, 'ast> {
    pub(crate) fn create(
        strict: bool,
        platform_availability_set: Option<PlatformSet>,
    ) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            kind: ModuleKind::UnknownModule,
            types: BTreeMap::new(),
            type_stars: Vec::new(),
            ts_pending: BTreeMap::new(),
            strict,
            platform_availability_set,
        }))
    }

    fn add(&mut self, name: FlowSmolStr, t: Export<'arena, 'ast>) {
        match &mut self.kind {
            ModuleKind::ESModule { names, .. } => {
                names.insert(name, t);
            }
            ModuleKind::UnknownModule => {
                let mut names = BTreeMap::new();
                names.insert(name, t);
                self.kind = ModuleKind::ESModule {
                    names,
                    stars: Vec::new(),
                };
            }
            ModuleKind::CJSModule(_)
            | ModuleKind::CJSModuleProps { .. }
            | ModuleKind::CJSDeclareModule { .. } => {
                // indeterminate
            }
        }
    }

    fn add_star(&mut self, loc: LocNode<'arena>, mref: ModuleRefNode<'arena>) {
        match &mut self.kind {
            ModuleKind::ESModule { names: _, stars } => {
                stars.push((loc, mref));
            }
            ModuleKind::UnknownModule => {
                self.kind = ModuleKind::ESModule {
                    names: BTreeMap::new(),
                    stars: vec![(loc, mref)],
                };
            }
            ModuleKind::CJSModule(_)
            | ModuleKind::CJSModuleProps { .. }
            | ModuleKind::CJSDeclareModule { .. } => {
                // indeterminate
            }
        }
    }

    fn cjs_clobber(&mut self, t: Parsed<'arena, 'ast>) {
        match &self.kind {
            ModuleKind::UnknownModule
            | ModuleKind::CJSModule(_)
            | ModuleKind::CJSModuleProps { .. }
            | ModuleKind::CJSDeclareModule { .. } => {
                self.kind = ModuleKind::CJSModule(t);
            }
            ModuleKind::ESModule { .. } => {
                // indeterminate
            }
        }
    }

    fn cjs_declare_module_set_prop(
        &mut self,
        name: FlowSmolStr,
        prop: LocalDefNode<'arena, 'ast>,
        strictness_kind: TypeStrictnessKind,
    ) {
        match &mut self.kind {
            ModuleKind::UnknownModule => {
                let mut props = BTreeMap::new();
                props.insert(name, prop);
                self.kind = ModuleKind::CJSDeclareModule {
                    props,
                    strictness_kind,
                };
            }
            ModuleKind::CJSDeclareModule { props, .. } => {
                props.insert(name, prop);
            }
            ModuleKind::CJSModuleProps { .. }
            | ModuleKind::CJSModule(_)
            | ModuleKind::ESModule { .. } => {
                // indeterminate
            }
        }
    }

    fn add_type(&mut self, name: FlowSmolStr, t: ExportType<'arena, 'ast>) {
        self.types.insert(name, t);
    }

    fn add_type_star(&mut self, loc: LocNode<'arena>, mref: ModuleRefNode<'arena>) {
        self.type_stars.push((loc, mref));
    }

    fn add_ts_pending(&mut self, name: FlowSmolStr, t: TsPendingExport<'arena, 'ast>) {
        self.ts_pending.insert(name, t);
        match &self.kind {
            ModuleKind::ESModule { .. } => {}
            ModuleKind::UnknownModule => {
                self.kind = ModuleKind::ESModule {
                    names: BTreeMap::new(),
                    stars: Vec::new(),
                };
            }
            ModuleKind::CJSModule(_)
            | ModuleKind::CJSModuleProps { .. }
            | ModuleKind::CJSDeclareModule { .. } => {
                // indeterminate
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScopeId(usize);

pub(crate) mod scope {
    use std::ops::DerefMut;

    use super::*;
    use crate::signature_error::BindingValidation;

    // The Global scope constructor is used only when parsing a library definition.
    // For implementation files, the top-level scope will be a Module.
    pub(crate) enum Scope<'arena, 'ast> {
        Global {
            values: BTreeMap<FlowSmolStr, BindingNode<'arena, 'ast>>,
            types: BTreeMap<FlowSmolStr, BindingNode<'arena, 'ast>>,
            modules: BTreeMap<FlowSmolStr, (LocNode<'arena>, Rc<RefCell<Exports<'arena, 'ast>>>)>,
            umd_modules:
                BTreeMap<FlowSmolStr, (LocNode<'arena>, Rc<RefCell<Exports<'arena, 'ast>>>)>,
        },
        DeclareGlobal {
            parent: ScopeId,
            global_target: ScopeId,
        },
        DeclareModule {
            values: BTreeMap<FlowSmolStr, BindingNode<'arena, 'ast>>,
            types: BTreeMap<FlowSmolStr, BindingNode<'arena, 'ast>>,
            parent: ScopeId,
            exports: Rc<RefCell<Exports<'arena, 'ast>>>,
        },
        DeclareNamespace {
            name: FlowSmolStr,
            values: BTreeMap<FlowSmolStr, BindingNode<'arena, 'ast>>,
            types: BTreeMap<FlowSmolStr, BindingNode<'arena, 'ast>>,
            exported_values: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
            exported_types: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
            parent: ScopeId,
        },
        Module {
            parent: Option<ScopeId>,
            values: BTreeMap<FlowSmolStr, BindingNode<'arena, 'ast>>,
            types: BTreeMap<FlowSmolStr, BindingNode<'arena, 'ast>>,
            exports: Rc<RefCell<Exports<'arena, 'ast>>>,
        },
        Lexical {
            values: BTreeMap<FlowSmolStr, BindingNode<'arena, 'ast>>,
            types: BTreeMap<FlowSmolStr, BindingNode<'arena, 'ast>>,
            parent: ScopeId,
        },
        ConditionalTypeExtends(ConditionalTypeExtends<'arena, 'ast>),
    }

    pub(crate) struct ConditionalTypeExtends<'arena, 'ast> {
        pub(super) infer_type_names: BTreeMap<FlowSmolStr, Loc>,
        pub(super) infer_tparams: Vec<TParam<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        pub(super) parent: ScopeId,
    }

    pub(crate) struct Scopes<'arena, 'ast> {
        scopes: Vec<Scope<'arena, 'ast>>,
        use_defs: Option<BTreeMap<Loc, Vec<Loc>>>,
    }

    impl<'arena, 'ast> Scopes<'arena, 'ast> {
        pub(crate) fn new() -> Self {
            Scopes {
                scopes: Vec::new(),
                use_defs: None,
            }
        }

        pub(crate) fn set_use_defs(&mut self, use_defs: BTreeMap<Loc, Vec<Loc>>) {
            self.use_defs = Some(use_defs);
        }

        pub(crate) fn lookup_ref(
            &mut self,
            tbls: &mut Tables<'arena, 'ast>,
            ref_: &Ref<'arena, 'ast>,
            type_only: bool,
        ) -> Option<BindingNode<'arena, 'ast>> {
            let native = if type_only {
                lookup_type(self, ref_.scope, &ref_.name)
            } else {
                lookup_value(self, ref_.scope, &ref_.name)
            }
            .map(|(binding, _)| binding);
            let Some(def_locs) = self
                .use_defs
                .as_ref()
                .and_then(|use_defs| use_defs.get(&ref_.ref_loc.0.data()))
            else {
                return native;
            };
            if let Some(binding) = &native
                && super::binding_locs(binding)
                    .iter()
                    .any(|loc| def_locs.contains(&loc.0.data()))
            {
                return native;
            }
            // The source declaration has no binding here (e.g. it is local to a
            // function body), but it must still shadow outer names.
            let def_loc = def_locs.first()?.dupe();
            let id_loc = tbls.push_loc(def_loc.dupe());
            let def = Lazy::new(Box::new(move |_, _, tbls| {
                Parsed::Annot(Box::new(Annot::Mixed(Box::new(tbls.push_loc(def_loc)))))
            }));
            Some(BindingNode::LocalBinding(tbls.push_local_def(
                LocalBinding::ParamBinding {
                    id_loc,
                    name: ref_.name.dupe(),
                    def,
                    tparams: TParams::Mono,
                    strictness_kind: tbls.strictness_kind,
                },
            )))
        }

        pub(super) fn push(&mut self, scope: Scope<'arena, 'ast>) -> ScopeId {
            let id = ScopeId(self.scopes.len());
            self.scopes.push(scope);
            id
        }

        pub(super) fn get(&self, id: ScopeId) -> &Scope<'arena, 'ast> {
            &self.scopes[id.0]
        }

        pub(crate) fn get_mut(&mut self, id: ScopeId) -> &mut Scope<'arena, 'ast> {
            &mut self.scopes[id.0]
        }

        pub(crate) fn value_bindings_at_locs(
            &self,
            target_locs: &BTreeSet<Loc>,
        ) -> BTreeMap<Loc, BindingNode<'arena, 'ast>> {
            let mut result = BTreeMap::new();
            for scope in &self.scopes {
                let values = match scope {
                    Scope::Global { values, .. }
                    | Scope::DeclareModule { values, .. }
                    | Scope::DeclareNamespace { values, .. }
                    | Scope::Module { values, .. }
                    // Lexical bindings (parameters, function-local
                    // definitions) resolve like module bindings. Consumers
                    // decide which roots their analysis can address: names
                    // that escape module scope are filtered downstream.
                    | Scope::Lexical { values, .. } => values,
                    Scope::DeclareGlobal { .. } | Scope::ConditionalTypeExtends(_) => continue,
                };
                for binding in values.values() {
                    for loc in super::binding_locs(binding) {
                        let loc = loc.0.data();
                        if target_locs.contains(&loc) {
                            result.insert(loc.dupe(), binding.dupe());
                        }
                    }
                }
            }
            result
        }
    }

    pub(crate) fn create_global<'arena, 'ast>(scopes: &mut Scopes<'arena, 'ast>) -> ScopeId {
        scopes.push(Scope::Global {
            values: BTreeMap::new(),
            types: BTreeMap::new(),
            modules: BTreeMap::new(),
            umd_modules: BTreeMap::new(),
        })
    }

    pub(crate) fn create_module<'arena, 'ast>(
        scopes: &mut Scopes<'arena, 'ast>,
        strict: bool,
        platform_availability_set: Option<PlatformSet>,
    ) -> ScopeId {
        scopes.push(Scope::Module {
            parent: None,
            values: BTreeMap::new(),
            types: BTreeMap::new(),
            exports: Exports::create(strict, platform_availability_set),
        })
    }

    pub(crate) fn create_augmentation_module<'arena, 'ast>(
        scopes: &mut Scopes<'arena, 'ast>,
        global_target: ScopeId,
    ) -> ScopeId {
        scopes.push(Scope::Module {
            parent: Some(global_target),
            values: BTreeMap::new(),
            types: BTreeMap::new(),
            exports: Exports::create(true, None),
        })
    }

    pub(crate) fn push_declare_global<'arena, 'ast>(
        scopes: &mut Scopes<'arena, 'ast>,
        parent: ScopeId,
        global_target: ScopeId,
    ) -> ScopeId {
        scopes.push(Scope::DeclareGlobal {
            parent,
            global_target,
        })
    }

    fn binding_scope(scopes: &Scopes, id: ScopeId) -> ScopeId {
        match scopes.get(id) {
            Scope::DeclareGlobal { global_target, .. } => *global_target,
            _ => id,
        }
    }

    pub(super) fn push_lex(scopes: &mut Scopes, parent: ScopeId) -> ScopeId {
        scopes.push(Scope::Lexical {
            parent,
            values: BTreeMap::new(),
            types: BTreeMap::new(),
        })
    }

    pub(super) fn push_declare_namespace(
        scopes: &mut Scopes,
        parent: ScopeId,
        name: FlowSmolStr,
    ) -> ScopeId {
        scopes.push(Scope::DeclareNamespace {
            name,
            parent,
            values: BTreeMap::new(),
            types: BTreeMap::new(),
            exported_values: BTreeMap::new(),
            exported_types: BTreeMap::new(),
        })
    }

    pub(super) fn push_declare_module<'arena, 'ast>(
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        parent: ScopeId,
        loc: LocNode<'arena>,
        name: FlowSmolStr,
    ) -> ScopeId {
        let exports = Exports::create(true, None);
        match scopes.get_mut(parent) {
            Scope::Global { modules, .. } => {
                if let Some((override_binding_loc, _)) = modules.get(&name) {
                    tbls.additional_errors.push(
                        signature_error::BindingValidation::ModuleOverride {
                            name: name.clone(),
                            override_binding_loc: override_binding_loc.dupe(),
                            existing_binding_loc: loc,
                        },
                    );
                } else {
                    modules.insert(name.clone(), (loc, exports.dupe()));
                }
            }
            _ => {}
        }
        scopes.push(Scope::DeclareModule {
            values: BTreeMap::new(),
            types: BTreeMap::new(),
            exports,
            parent,
        })
    }

    pub(crate) fn bind_umd_global<'arena, 'ast>(
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        scope: ScopeId,
        id: &ast::Identifier<Loc, Loc>,
    ) {
        let Scope::Module {
            parent: Some(global),
            exports,
            ..
        } = scopes.get(scope)
        else {
            return;
        };
        let (global, exports) = (*global, exports.dupe());
        let Scope::Global { umd_modules, .. } = scopes.get_mut(global) else {
            return;
        };
        // Like TypeScript, the first UMD declaration of a name wins.
        umd_modules
            .entry(id.name.dupe())
            .or_insert_with(|| (tbls.push_loc(id.loc.dupe()), exports));
    }

    fn modify_exports<'arena, 'ast>(
        scopes: &mut Scopes<'arena, 'ast>,
        id: ScopeId,
        f: impl FnOnce(&mut Exports<'arena, 'ast>),
    ) {
        match scopes.get_mut(id) {
            Scope::Module { exports, .. } | Scope::DeclareModule { exports, .. } => {
                f(&mut exports.borrow_mut())
            }
            Scope::DeclareNamespace { .. }
            | Scope::DeclareGlobal { .. }
            | Scope::Global { .. }
            | Scope::Lexical { .. }
            | Scope::ConditionalTypeExtends(_) => {}
        }
    }

    pub(crate) fn builtins_exn<'arena, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        scope: ScopeId,
    ) -> (
        BTreeMap<FlowSmolStr, BindingNode<'arena, 'ast>>,
        BTreeMap<FlowSmolStr, BindingNode<'arena, 'ast>>,
        BTreeMap<FlowSmolStr, (LocNode<'arena>, Rc<RefCell<Exports<'arena, 'ast>>>)>,
        BTreeMap<FlowSmolStr, (LocNode<'arena>, Rc<RefCell<Exports<'arena, 'ast>>>)>,
    ) {
        match scopes.get(scope) {
            Scope::Global {
                values,
                types,
                modules,
                umd_modules,
            } => {
                // Like TypeScript, UMD globals never shadow other globals.
                let umd_modules = umd_modules
                    .iter()
                    .filter(|(name, _)| !values.contains_key(*name) && !types.contains_key(*name))
                    .map(|(name, module)| (name.dupe(), module.clone()))
                    .collect();
                (values.clone(), types.clone(), modules.clone(), umd_modules)
            }
            _ => panic!("Expected Global scope"),
        }
    }

    pub(crate) fn exports_exn<'arena, 'ast>(
        scopes: &mut Scopes<'arena, 'ast>,
        scope: ScopeId,
    ) -> Rc<RefCell<Exports<'arena, 'ast>>> {
        match scopes.get_mut(scope) {
            Scope::Module { exports, .. } => exports.dupe(),
            _ => panic!("Expected Module scope"),
        }
    }

    fn block_scoped(binding: &LocalBinding) -> bool {
        match binding {
            LocalBinding::TypeBinding { .. } | LocalBinding::VarBinding { .. } => false,
            LocalBinding::LetConstBinding { .. }
            | LocalBinding::ConstFunBinding { .. }
            | LocalBinding::ConstRefBinding { .. }
            | LocalBinding::ParamBinding { .. }
            | LocalBinding::FunBinding { .. }
            | LocalBinding::DeclareFunBinding { .. }
            | LocalBinding::ComponentBinding { .. }
            | LocalBinding::ClassBinding { .. }
            | LocalBinding::DeclareClassBinding { .. }
            | LocalBinding::RecordBinding { .. }
            | LocalBinding::EnumBinding { .. }
            | LocalBinding::NamespaceBinding { .. } => true,
        }
    }

    pub(super) fn value_binding<'arena, 'ast>(
        kind: ast::VariableKind,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Lazy<'arena, 'ast, Parsed<'arena, 'ast>>,
    ) -> LocalBinding<'arena, 'ast> {
        match kind {
            ast::VariableKind::Var => LocalBinding::VarBinding { id_loc, name, def },
            ast::VariableKind::Let | ast::VariableKind::Const => LocalBinding::LetConstBinding {
                is_const: kind == ast::VariableKind::Const,
                id_loc,
                name,
                def,
            },
        }
    }

    pub(super) fn import_binding<'arena>(
        kind: ast::statement::ImportKind,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        mref: ModuleRefNode<'arena>,
        remote: FlowSmolStr,
    ) -> RemoteBinding<'arena> {
        match kind {
            ast::statement::ImportKind::ImportValue => RemoteBinding::ImportBinding {
                id_loc,
                name,
                mref,
                remote,
            },
            ast::statement::ImportKind::ImportType => RemoteBinding::ImportTypeBinding {
                id_loc,
                name,
                mref,
                remote,
            },
            ast::statement::ImportKind::ImportTypeof => RemoteBinding::ImportTypeofBinding {
                id_loc,
                name,
                mref,
                remote,
            },
        }
    }

    pub(super) fn import_ns_binding<'arena>(
        kind: ast::statement::ImportKind,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        mref: ModuleRefNode<'arena>,
    ) -> RemoteBinding<'arena> {
        match kind {
            ast::statement::ImportKind::ImportValue => {
                RemoteBinding::ImportNsBinding { id_loc, name, mref }
            }
            ast::statement::ImportKind::ImportTypeof => {
                RemoteBinding::ImportTypeofNsBinding { id_loc, name, mref }
            }
            ast::statement::ImportKind::ImportType => {
                RemoteBinding::ImportTypeNsBinding { id_loc, name, mref }
            }
        }
    }

    // Cross-namespace name conflict detection. With the TypeScript-style
    // value/type namespace split, `bind` only consults its own namespace map.
    // For most kinds (e.g. `const A = 1; interface A {}`) cross-namespace
    // coexistence is intentional. But a `class`/`declare class`/`enum` plus a
    // `type` alias of the same name is not legitimate coexistence — both
    // occupy the type namespace via lookup, and TypeScript itself rejects this
    // pair. The only carve-outs are class/declare-class/namespace + interface
    // (declaration merging support, deferred).
    #[derive(Clone, Copy)]
    pub(super) enum IncomingBindingKind {
        Type,
        Interface,
        Class,
        DeclareClass,
        Enum,
    }

    fn is_cross_namespace_conflict<'arena, 'ast>(
        opts: &TypeSigOptions,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        existing: &LocalBinding<'arena, 'ast>,
        incoming: IncomingBindingKind,
    ) -> bool {
        match (incoming, existing) {
            // Type-side incoming, value-side existing.
            (
                IncomingBindingKind::Type,
                LocalBinding::ClassBinding { .. }
                | LocalBinding::DeclareClassBinding { .. }
                | LocalBinding::EnumBinding { .. }
                | LocalBinding::NamespaceBinding { .. },
            ) => true,
            (IncomingBindingKind::Interface, LocalBinding::EnumBinding { .. }) => true,
            // Value-side incoming, type-side existing. Force the existing def
            // lazy to discriminate Interface (allowed for decl-merging) from
            // TypeAlias/OpaqueType (rejected). Mirrors the OCaml
            // `Lazy.force` in is_cross_namespace_conflict.
            (
                IncomingBindingKind::Class | IncomingBindingKind::DeclareClass,
                LocalBinding::TypeBinding { def, .. },
            ) => {
                let parsed_def = def.get_forced(opts, scopes, tbls);
                !matches!(parsed_def, Def::Interface(_))
            }
            (IncomingBindingKind::Enum, LocalBinding::TypeBinding { .. }) => true,
            _ => false,
        }
    }

    pub(super) fn report_cross_namespace_conflict<'arena, 'ast>(
        opts: &TypeSigOptions,
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        name: &FlowSmolStr,
        id_loc: LocNode<'arena>,
        type_only: bool,
        incoming: IncomingBindingKind,
    ) {
        let existing = match scopes.get(binding_scope(scopes, id)) {
            Scope::Global { values, types, .. } => {
                let map = if type_only { values } else { types };
                map.get(name).map(|binding| binding.dupe())
            }
            _ => None,
        };
        let Some(BindingNode::LocalBinding(existing)) = existing else {
            return;
        };
        let conflict = {
            let data = existing.0.data();
            is_cross_namespace_conflict(opts, scopes, tbls, &data, incoming)
        };
        if conflict {
            let override_binding_loc = loc_of_binding(&BindingNode::LocalBinding(existing.dupe()));
            tbls.additional_errors
                .push(signature_error::BindingValidation::NameOverride {
                    name: name.clone(),
                    override_binding_loc,
                    existing_binding_loc: id_loc,
                });
        }
    }

    pub(super) fn bind<'arena, 'ast, F>(
        type_only: bool,
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        name: &FlowSmolStr,
        id_loc: LocNode<'arena>,
        f: F,
    ) where
        F: FnOnce(
            &mut Scopes<'arena, 'ast>,
            &mut Tables<'arena, 'ast>,
            Option<BindingNode<'arena, 'ast>>,
        ) -> (BindingNode<'arena, 'ast>, bool),
    {
        fn check_name_override<'arena, 'ast>(
            in_global_scope: bool,
            legal: bool,
            name: &FlowSmolStr,
            id_loc: LocNode<'arena>,
            tbls: &mut Tables<'arena, 'ast>,
            existing_binding: &BindingNode<'arena, 'ast>,
        ) {
            // For now, we allow $JSXIntrinsics, since we intentionally kept only a minimal version in
            // the builtins, and let flow-typed supply a full one. Libraries may also widen the
            // user-visible string attributes of the React DOM intrinsics by redeclaring
            // ReactDOM$UserVisibleString (see lib/react.js).
            if in_global_scope
                && name != "$JSXIntrinsics"
                && name != "ReactDOM$UserVisibleString"
                && !legal
            {
                let override_binding_loc = loc_of_binding(existing_binding);
                tbls.additional_errors
                    .push(signature_error::BindingValidation::NameOverride {
                        name: name.clone(),
                        override_binding_loc,
                        existing_binding_loc: id_loc,
                    });
            }
        }

        enum CheckBindResult<'arena, 'ast> {
            SameKindExisting {
                existing: BindingNode<'arena, 'ast>,
                in_global_scope: bool,
            },
            Missing,
            Skip,
        }

        // TypeScript-style two-namespace model: only check the matching map
        // for an existing binding. Cross-namespace coexistence (e.g.
        // `const A = 1; interface A {}`) is permitted by `bind` itself; the
        // class/enum-vs-type-alias rejection is handled by callers via
        // `report_cross_namespace_conflict` invoked before `bind` runs.
        fn scope_check_bind<'arena, 'ast>(
            scopes: &Scopes<'arena, 'ast>,
            id: ScopeId,
            name: &FlowSmolStr,
            type_only: bool,
        ) -> CheckBindResult<'arena, 'ast> {
            match scopes.get(id) {
                Scope::Global { values, types, .. }
                | Scope::DeclareModule { values, types, .. }
                | Scope::DeclareNamespace { values, types, .. }
                | Scope::Module { values, types, .. }
                | Scope::Lexical { values, types, .. } => {
                    let map = if type_only { types } else { values };
                    if let Some(existing) = map.get(name) {
                        CheckBindResult::SameKindExisting {
                            existing: existing.dupe(),
                            in_global_scope: matches!(
                                scopes.get(id),
                                Scope::Global { .. } | Scope::DeclareGlobal { .. }
                            ),
                        }
                    } else {
                        CheckBindResult::Missing
                    }
                }
                Scope::DeclareGlobal { .. } | Scope::ConditionalTypeExtends(_) => {
                    CheckBindResult::Skip
                }
            }
        }

        let id = binding_scope(scopes, id);
        let new_binding = match scope_check_bind(scopes, id, name, type_only) {
            CheckBindResult::SameKindExisting {
                existing,
                in_global_scope,
            } => {
                let (new_binding, legal) = f(scopes, tbls, Some(existing.dupe()));
                check_name_override(in_global_scope, legal, name, id_loc, tbls, &existing);
                Some(new_binding)
            }
            CheckBindResult::Missing => {
                let (new_binding, _) = f(scopes, tbls, None);
                Some(new_binding)
            }
            CheckBindResult::Skip => None,
        };
        let Some(new_binding) = new_binding else {
            return;
        };
        match scopes.get_mut(id) {
            Scope::Global { values, types, .. }
            | Scope::DeclareModule { values, types, .. }
            | Scope::DeclareNamespace { values, types, .. }
            | Scope::Module { values, types, .. }
            | Scope::Lexical { values, types, .. } => {
                if type_only {
                    types.insert(name.clone(), new_binding);
                } else {
                    values.insert(name.clone(), new_binding);
                }
            }
            Scope::DeclareGlobal { .. } | Scope::ConditionalTypeExtends(_) => {}
        }
    }

    pub(crate) fn lookup_value<'a, 'arena, 'ast>(
        scopes: &'a Scopes<'arena, 'ast>,
        mut id: ScopeId,
        name: &FlowSmolStr,
    ) -> Option<(BindingNode<'arena, 'ast>, ScopeId)> {
        loop {
            match scopes.get(id) {
                Scope::Global { values, .. } => {
                    return values.get(name).map(|binding| (binding.dupe(), id));
                }
                Scope::Module { parent, values, .. } => match values.get(name) {
                    Some(binding) => return Some((binding.dupe(), id)),
                    None => id = (*parent)?,
                },
                Scope::ConditionalTypeExtends(ConditionalTypeExtends { parent, .. }) => {
                    id = *parent;
                }
                Scope::DeclareModule { parent, values, .. }
                | Scope::Lexical { parent, values, .. } => match values.get(name) {
                    Some(binding) => return Some((binding.dupe(), id)),
                    None => {
                        id = *parent;
                    }
                },
                Scope::DeclareGlobal {
                    parent,
                    global_target,
                } => {
                    if let Some(binding) = lookup_value(scopes, *global_target, name) {
                        return Some(binding);
                    }
                    id = *parent;
                }
                Scope::DeclareNamespace { parent, values, .. } => {
                    match lookup_merged_namespace_member_value(scopes, id, name)
                        .or_else(|| values.get(name).map(|binding| binding.dupe()))
                    {
                        Some(binding) => return Some((binding.dupe(), id)),
                        None => {
                            id = *parent;
                        }
                    }
                }
            }
        }
    }

    fn lookup_value_in_scope<'arena, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        id: ScopeId,
        name: &FlowSmolStr,
    ) -> Option<BindingNode<'arena, 'ast>> {
        match scopes.get(id) {
            Scope::Global { values, .. }
            | Scope::DeclareModule { values, .. }
            | Scope::DeclareNamespace { values, .. }
            | Scope::Module { values, .. }
            | Scope::Lexical { values, .. } => values.get(name).map(|binding| binding.dupe()),
            Scope::DeclareGlobal { global_target, .. } => {
                lookup_value_in_scope(scopes, *global_target, name)
            }
            Scope::ConditionalTypeExtends(_) => None,
        }
    }

    fn lookup_scope<'arena, 'ast>(
        name: &FlowSmolStr,
        values: &BTreeMap<FlowSmolStr, BindingNode<'arena, 'ast>>,
        types: &BTreeMap<FlowSmolStr, BindingNode<'arena, 'ast>>,
    ) -> Option<BindingNode<'arena, 'ast>> {
        types
            .get(name)
            .or_else(|| values.get(name))
            .map(|b| b.dupe())
    }

    fn namespace_member_type<'arena, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        binding: BindingNode<'arena, 'ast>,
        name: &FlowSmolStr,
    ) -> Option<BindingNode<'arena, 'ast>> {
        let BindingNode::LocalBinding(node) = binding else {
            return None;
        };
        match node.0.data().deref() {
            LocalBinding::NamespaceBinding { values, types, .. } => types
                .get(name)
                .and_then(|entry| local_binding_node_of_namespace_entry(scopes, true, name, entry))
                .or_else(|| {
                    values.get(name).and_then(|entry| {
                        local_value_binding_node_of_namespace_entry(scopes, name, entry)
                    })
                })
                .map(BindingNode::LocalBinding),
            _ => None,
        }
    }

    fn namespace_member_value<'arena, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        binding: BindingNode<'arena, 'ast>,
        name: &FlowSmolStr,
    ) -> Option<BindingNode<'arena, 'ast>> {
        let BindingNode::LocalBinding(node) = binding else {
            return None;
        };
        match node.0.data().deref() {
            LocalBinding::NamespaceBinding { values, .. } => values
                .get(name)
                .and_then(|entry| local_value_binding_node_of_namespace_entry(scopes, name, entry))
                .map(BindingNode::LocalBinding),
            _ => None,
        }
    }

    fn lookup_type_in_scope<'arena, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        id: ScopeId,
        name: &FlowSmolStr,
    ) -> Option<BindingNode<'arena, 'ast>> {
        match scopes.get(id) {
            Scope::Global { values, types, .. }
            | Scope::DeclareModule { values, types, .. }
            | Scope::DeclareNamespace { values, types, .. }
            | Scope::Module { values, types, .. }
            | Scope::Lexical { values, types, .. } => lookup_scope(name, values, types),
            Scope::DeclareGlobal { global_target, .. } => {
                lookup_type_in_scope(scopes, *global_target, name)
            }
            Scope::ConditionalTypeExtends(_) => None,
        }
    }

    fn namespace_path_and_host(
        scopes: &Scopes,
        mut id: ScopeId,
    ) -> Option<(ScopeId, Vec<FlowSmolStr>)> {
        let mut path = Vec::new();
        loop {
            match scopes.get(id) {
                Scope::DeclareNamespace { name, parent, .. } => {
                    path.push(name.dupe());
                    id = *parent;
                }
                Scope::ConditionalTypeExtends(ConditionalTypeExtends { parent, .. })
                | Scope::Lexical { parent, .. } => {
                    id = *parent;
                }
                Scope::Global { .. }
                | Scope::DeclareGlobal { .. }
                | Scope::DeclareModule { .. }
                | Scope::Module { .. } => {
                    path.reverse();
                    return if path.is_empty() {
                        None
                    } else {
                        Some((id, path))
                    };
                }
            }
        }
    }

    fn lookup_merged_namespace_member_type<'arena, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        id: ScopeId,
        name: &FlowSmolStr,
    ) -> Option<BindingNode<'arena, 'ast>> {
        let (host, path) = namespace_path_and_host(scopes, id)?;
        let mut path = path.iter();
        let first = path.next()?;
        let mut binding = lookup_type_in_scope(scopes, host, first)?;
        for namespace_name in path {
            binding = namespace_member_type(scopes, binding, namespace_name)?;
        }
        namespace_member_type(scopes, binding, name)
    }

    fn lookup_merged_namespace_member_value<'arena, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        id: ScopeId,
        name: &FlowSmolStr,
    ) -> Option<BindingNode<'arena, 'ast>> {
        let (host, path) = namespace_path_and_host(scopes, id)?;
        let mut path = path.iter();
        let first = path.next()?;
        let mut binding = lookup_value_in_scope(scopes, host, first)?;
        for namespace_name in path {
            binding = namespace_member_value(scopes, binding, namespace_name)?;
        }
        namespace_member_value(scopes, binding, name)
    }

    pub(crate) fn lookup_type<'a, 'arena, 'ast>(
        scopes: &'a Scopes<'arena, 'ast>,
        mut id: ScopeId,
        name: &FlowSmolStr,
    ) -> Option<(BindingNode<'arena, 'ast>, ScopeId)> {
        loop {
            match scopes.get(id) {
                Scope::Global { values, types, .. } => {
                    return lookup_scope(name, values, types).map(|binding| (binding, id));
                }
                Scope::ConditionalTypeExtends(ConditionalTypeExtends { parent, .. }) => {
                    id = *parent;
                }
                Scope::DeclareNamespace {
                    parent,
                    values,
                    types,
                    ..
                } => match lookup_merged_namespace_member_type(scopes, id, name)
                    .or_else(|| lookup_scope(name, values, types))
                {
                    Some(binding) => return Some((binding, id)),
                    None => {
                        id = *parent;
                    }
                },
                Scope::Lexical {
                    parent,
                    values,
                    types,
                } => match lookup_scope(name, values, types) {
                    Some(binding) => return Some((binding, id)),
                    None => {
                        id = *parent;
                    }
                },
                Scope::Module {
                    parent,
                    values,
                    types,
                    ..
                } => match lookup_scope(name, values, types) {
                    Some(binding) => return Some((binding, id)),
                    None => id = (*parent)?,
                },
                Scope::DeclareModule {
                    parent,
                    values,
                    types,
                    ..
                } => match lookup_scope(name, values, types) {
                    Some(binding) => return Some((binding, id)),
                    None => {
                        id = *parent;
                    }
                },
                Scope::DeclareGlobal {
                    parent,
                    global_target,
                } => {
                    if let Some(binding) = lookup_type(scopes, *global_target, name) {
                        return Some(binding);
                    }
                    id = *parent;
                }
            }
        }
    }

    /// Whether a name written in type position can head a computed key, that
    /// is, whether it binds a value that is not a namespace. A class and an
    /// enum bind a value, so they head one, even though they name a type as
    /// well. A namespace does not, because `A.B` may be reaching a type
    /// through it.
    ///
    /// This pipeline has its own bindings, so it answers from those; the
    /// checking pipeline asks `flow_analysis::bindings::Kind::namespaces` of
    /// the corresponding source binding kind. The two must agree, or an
    /// imported type would be read differently from the local one.
    pub(crate) fn name_heads_computed_key<'arena, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        id: ScopeId,
        name: &FlowSmolStr,
    ) -> bool {
        match lookup_type(scopes, id, name) {
            None => false,
            Some((binding, _)) => match binding {
                BindingNode::LocalBinding(node) => match &*node.0.data() {
                    LocalBinding::VarBinding { .. }
                    | LocalBinding::LetConstBinding { .. }
                    | LocalBinding::ParamBinding { .. }
                    | LocalBinding::ConstRefBinding { .. }
                    | LocalBinding::ConstFunBinding { .. }
                    | LocalBinding::FunBinding { .. }
                    | LocalBinding::DeclareFunBinding { .. }
                    | LocalBinding::ComponentBinding { .. }
                    | LocalBinding::ClassBinding { .. }
                    | LocalBinding::DeclareClassBinding { .. }
                    | LocalBinding::RecordBinding { .. }
                    | LocalBinding::EnumBinding { .. } => true,
                    LocalBinding::TypeBinding { .. } | LocalBinding::NamespaceBinding { .. } => {
                        false
                    }
                },
                BindingNode::RemoteBinding(node) => match &*node.0.data() {
                    RemoteBinding::ImportBinding { .. } => true,
                    RemoteBinding::ImportNsBinding { .. }
                    | RemoteBinding::ImportTypeBinding { .. }
                    | RemoteBinding::ImportTypeofBinding { .. }
                    | RemoteBinding::ImportTypeofNsBinding { .. }
                    | RemoteBinding::ImportTypeNsBinding { .. } => false,
                },
            },
        }
    }

    fn lookup_local_type<'arena, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        id: ScopeId,
        name: &FlowSmolStr,
    ) -> Option<BindingNode<'arena, 'ast>> {
        match scopes.get(binding_scope(scopes, id)) {
            Scope::Global { types, .. }
            | Scope::DeclareModule { types, .. }
            | Scope::DeclareNamespace { types, .. }
            | Scope::Module { types, .. }
            | Scope::Lexical { types, .. } => types.get(name).map(|binding| binding.dupe()),
            Scope::DeclareGlobal { .. } | Scope::ConditionalTypeExtends(_) => None,
        }
    }

    fn lookup_local_value<'arena, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        id: ScopeId,
        name: &FlowSmolStr,
    ) -> Option<BindingNode<'arena, 'ast>> {
        match scopes.get(binding_scope(scopes, id)) {
            Scope::Global { values, .. }
            | Scope::DeclareModule { values, .. }
            | Scope::DeclareNamespace { values, .. }
            | Scope::Module { values, .. }
            | Scope::Lexical { values, .. } => values.get(name).map(|binding| binding.dupe()),
            Scope::DeclareGlobal { .. } | Scope::ConditionalTypeExtends(_) => None,
        }
    }

    fn find_host(scopes: &Scopes, mut id: ScopeId, b: &LocalBinding) -> ScopeId {
        loop {
            match scopes.get(id) {
                Scope::Global { .. }
                | Scope::DeclareGlobal { .. }
                | Scope::DeclareModule { .. }
                | Scope::DeclareNamespace { .. }
                | Scope::Module { .. } => return id,
                Scope::ConditionalTypeExtends(ConditionalTypeExtends { parent, .. }) => {
                    id = *parent;
                }
                Scope::Lexical { parent, .. } => {
                    if block_scoped(b) {
                        return id;
                    } else {
                        id = *parent;
                    }
                }
            }
        }
    }

    pub(super) fn scope_of_infer_name<'a, 'arena, 'ast>(
        scopes: &'a Scopes<'arena, 'ast>,
        mut id: ScopeId,
        name: &FlowSmolStr,
        loc: &Loc,
    ) -> Option<&'a ConditionalTypeExtends<'arena, 'ast>> {
        loop {
            match scopes.get(id) {
                Scope::Global { .. }
                | Scope::DeclareGlobal { .. }
                | Scope::DeclareModule { .. }
                | Scope::DeclareNamespace { .. }
                | Scope::Module { .. } => return None,
                Scope::Lexical { parent, .. } => id = *parent,
                Scope::ConditionalTypeExtends(cond_scope) => {
                    return match cond_scope.infer_type_names.get(name) {
                        Some(l) if l == loc => Some(cond_scope),
                        _ => None,
                    };
                }
            }
        }
    }

    fn push_value_def_with_namespace<'arena, 'ast>(
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        id: ScopeId,
        mut def: LocalBinding<'arena, 'ast>,
    ) -> LocalDefNode<'arena, 'ast> {
        let (name, namespace_types) = match &mut def {
            LocalBinding::ClassBinding {
                name,
                namespace_types,
                ..
            }
            | LocalBinding::DeclareClassBinding {
                name,
                namespace_types,
                ..
            }
            | LocalBinding::FunBinding {
                name,
                namespace_types,
                ..
            }
            | LocalBinding::DeclareFunBinding {
                name,
                namespace_types,
                ..
            } => (name, namespace_types),
            _ => return tbls.push_local_def(def),
        };
        if let Some(BindingNode::LocalBinding(node)) = lookup_type_in_scope(scopes, id, name) {
            let types = match node.0.data_mut().deref_mut() {
                LocalBinding::NamespaceBinding { values, types, .. } if values.is_empty() => {
                    Some(std::mem::take(types))
                }
                _ => None,
            };
            if let Some(types) = types {
                namespace_types.extend(types);
                *node.0.data_mut() = def;
                return node;
            }
        }
        tbls.push_local_def(def)
    }

    fn bind_local<'arena, 'ast>(
        type_only: bool,
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        name: FlowSmolStr,
        id_loc: LocNode<'arena>,
        def: LocalBinding<'arena, 'ast>,
        k: impl Fn(&mut Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
    ) {
        let host = find_host(scopes, id, &def);
        bind(
            type_only,
            host,
            scopes,
            tbls,
            &name,
            id_loc,
            |scopes, tbls, existing_binding| match existing_binding {
                Some(b) => (b, false),
                None => {
                    let node = push_value_def_with_namespace(scopes, tbls, id, def);
                    k(scopes, &name, node.dupe());
                    (BindingNode::LocalBinding(node), true)
                }
            },
        );
    }

    fn bind_remote<'arena, 'ast>(
        type_only: bool,
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        name: FlowSmolStr,
        id_loc: LocNode<'arena>,
        ref_: RemoteBinding<'arena>,
    ) {
        bind(
            type_only,
            id,
            scopes,
            tbls,
            &name,
            id_loc,
            |_scopes, tbls, existing_binding| match existing_binding {
                Some(b) => (b, false),
                None => {
                    let node = tbls.push_remote_ref(ref_);
                    (BindingNode::RemoteBinding(node), true)
                }
            },
        );
    }

    pub(super) fn bind_type<'arena, 'ast>(
        opts: &TypeSigOptions,
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Lazy<'arena, 'ast, ParsedDef<'arena, 'ast>>,
        k: impl Fn(&mut Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
    ) {
        report_cross_namespace_conflict(
            opts,
            id,
            scopes,
            tbls,
            &name,
            id_loc.dupe(),
            true,
            IncomingBindingKind::Type,
        );
        bind_local(
            true,
            id,
            scopes,
            tbls,
            name.clone(),
            id_loc.dupe(),
            LocalBinding::TypeBinding { id_loc, def },
            k,
        );
    }

    mod type_param_renaming {
        use std::collections::BTreeSet;

        use super::*;

        type TParamRenameMap = BTreeMap<FlowSmolStr, FlowSmolStr>;

        fn tparams_names<'arena, 'ast>(
            tparams: &TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> Vec<FlowSmolStr> {
            match tparams {
                TParams::Mono => Vec::new(),
                TParams::Poly(poly) => poly.tparams.iter().map(|tp| tp.name.dupe()).collect(),
            }
        }

        fn tparams_name_locs<'arena, 'ast>(
            tparams: &TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> Vec<(LocNode<'arena>, FlowSmolStr)> {
            match tparams {
                TParams::Mono => Vec::new(),
                TParams::Poly(poly) => poly
                    .tparams
                    .iter()
                    .map(|tp| (tp.name_loc.dupe(), tp.name.dupe()))
                    .collect(),
            }
        }

        // TypeScript declaration merging treats type parameters as positional
        // slots, while the parsed type-sig representation refers to them by name.
        // If `declare class Iterator<Yield, Return, Next>` absorbs
        // `interface Iterator<T, TReturn, TNext>`, the interface members must be
        // alpha-renamed into the class's names before we concatenate signatures.
        // Otherwise the merged signature contains free references like `TReturn`
        // under a declaration that only binds `Return`.
        fn tparam_rename_map<'arena, 'ast>(
            from_tparams: &TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
            to_tparams: &TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> TParamRenameMap {
            tparams_names(from_tparams)
                .into_iter()
                .zip(tparams_names(to_tparams))
                .collect()
        }

        fn remove_tparams_from_rename_map<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            tparams: &TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> TParamRenameMap {
            let mut rename_map = rename_map.clone();
            for name in tparams_names(tparams) {
                rename_map.remove(&name);
            }
            rename_map
        }

        fn rename_tparam_name(rename_map: &TParamRenameMap, name: &FlowSmolStr) -> FlowSmolStr {
            rename_map.get(name).map_or_else(|| name.dupe(), Dupe::dupe)
        }

        fn rename_map_targets(rename_map: &TParamRenameMap) -> BTreeSet<FlowSmolStr> {
            rename_map.values().map(Dupe::dupe).collect()
        }

        fn rename_map_names(rename_map: &TParamRenameMap) -> BTreeSet<FlowSmolStr> {
            rename_map
                .iter()
                .flat_map(|(from_name, to_name)| [from_name.dupe(), to_name.dupe()])
                .collect()
        }

        fn capture_avoidance_map<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            tparams: &TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> (TParamRenameMap, TParamRenameMap) {
            let body_rename_map = remove_tparams_from_rename_map(rename_map, tparams);
            let capture_names = rename_map_targets(&body_rename_map);
            let mut used_names = rename_map_names(rename_map);
            used_names.extend(tparams_names(tparams));

            let (alpha_map, body_rename_map, _) = tparams_names(tparams).into_iter().fold(
                (TParamRenameMap::new(), body_rename_map, used_names),
                |(mut alpha_map, body_rename_map, mut used_names), name| {
                    let original_name = name.dupe();
                    let (renamed_name, body_rename_map) = alpha_rename::avoid_capture(
                        name,
                        body_rename_map,
                        |name| capture_names.contains(name),
                        || used_names.clone(),
                        alpha_rename::string,
                        |mut map, name| {
                            map.remove(name);
                            map
                        },
                        |mut map, name, new_name| {
                            map.insert(name, new_name);
                            map
                        },
                    );
                    if renamed_name != original_name {
                        alpha_map.insert(original_name, renamed_name.dupe());
                    }
                    used_names.insert(renamed_name);
                    (alpha_map, body_rename_map, used_names)
                },
            );
            (alpha_map, body_rename_map)
        }

        fn loc_node_eq(left: &LocNode<'_>, right: &LocNode<'_>) -> bool {
            left.0.data().deref() == right.0.data().deref()
        }

        // Rename only references to the outer declaration's type parameters. Nested
        // binders (function/component/class type params, conditional `infer`, and
        // mapped-type keys) shadow same-named outer params. When an outer type
        // parameter is renamed to a name used by a nested binder, alpha-rename the
        // nested binder first so the renamed outer reference stays free.
        fn rename_tparams_in_parsed<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            parsed: &Parsed<'arena, 'ast>,
        ) -> Parsed<'arena, 'ast> {
            match parsed {
                Parsed::Annot(annot) => {
                    Parsed::Annot(Box::new(rename_tparams_in_annot(rename_map, annot)))
                }
                Parsed::Value(value) => {
                    Parsed::Value(Box::new(rename_tparams_in_value(rename_map, value)))
                }
                Parsed::TyRefApp { loc, name, targs } => Parsed::TyRefApp {
                    loc: loc.dupe(),
                    name: name.clone(),
                    targs: targs
                        .iter()
                        .map(|t| rename_tparams_in_parsed(rename_map, t))
                        .collect(),
                },
                Parsed::Eval(loc, t, op) => Parsed::Eval(
                    loc.dupe(),
                    Box::new(rename_tparams_in_parsed(rename_map, t)),
                    Box::new(op.map(&mut (), |_, t| rename_tparams_in_parsed(rename_map, t))),
                ),
                Parsed::TyRef(_)
                | Parsed::AsyncVoidReturn(_)
                | Parsed::ValRef { .. }
                | Parsed::Err(_, _)
                | Parsed::BuiltinTyRef { .. }
                | Parsed::Pattern(_)
                | Parsed::Require { .. }
                | Parsed::ImportDynamic { .. }
                | Parsed::ModuleRef { .. }
                | Parsed::ImportTypeAnnot { .. } => parsed.clone(),
            }
        }

        fn override_bound_names_by_loc<'arena, 'ast>(
            loc_names: &[(LocNode<'arena>, FlowSmolStr)],
            parsed: &Parsed<'arena, 'ast>,
        ) -> Parsed<'arena, 'ast> {
            match parsed {
                Parsed::Annot(box Annot::Bound(inner)) => {
                    let name = loc_names
                        .iter()
                        .find_map(|(loc, name)| {
                            if loc_node_eq(loc, &inner.ref_loc) {
                                Some(name.dupe())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| inner.name.dupe());
                    Parsed::Annot(Box::new(Annot::Bound(Box::new(AnnotBound {
                        ref_loc: inner.ref_loc.dupe(),
                        name,
                    }))))
                }
                Parsed::Annot(annot) => Parsed::Annot(Box::new(annot.map(
                    &mut (),
                    |_, loc: &LocNode<'arena>| loc.dupe(),
                    |_, t| override_bound_names_by_loc(loc_names, t),
                ))),
                Parsed::Value(value) => Parsed::Value(Box::new(value.map(
                    &mut (),
                    &|_, loc: &LocNode<'arena>| loc.dupe(),
                    &|_, t| override_bound_names_by_loc(loc_names, t),
                ))),
                Parsed::TyRefApp { loc, name, targs } => Parsed::TyRefApp {
                    loc: loc.dupe(),
                    name: name.clone(),
                    targs: targs
                        .iter()
                        .map(|t| override_bound_names_by_loc(loc_names, t))
                        .collect(),
                },
                Parsed::Eval(loc, t, op) => Parsed::Eval(
                    loc.dupe(),
                    Box::new(override_bound_names_by_loc(loc_names, t)),
                    Box::new(op.map(&mut (), |_, t| override_bound_names_by_loc(loc_names, t))),
                ),
                Parsed::TyRef(_)
                | Parsed::AsyncVoidReturn(_)
                | Parsed::ValRef { .. }
                | Parsed::Err(_, _)
                | Parsed::BuiltinTyRef { .. }
                | Parsed::Pattern(_)
                | Parsed::Require { .. }
                | Parsed::ImportDynamic { .. }
                | Parsed::ModuleRef { .. }
                | Parsed::ImportTypeAnnot { .. } => parsed.clone(),
            }
        }

        fn rename_tparams_in_value<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            value: &ParsedValue<'arena, 'ast>,
        ) -> ParsedValue<'arena, 'ast> {
            match value {
                Value::ClassExpr(box (loc, def)) => Value::ClassExpr(Box::new((
                    loc.dupe(),
                    rename_tparams_in_class_sig(rename_map, def),
                ))),
                Value::FunExpr(inner) => Value::FunExpr(Box::new(ValueFunExpr {
                    loc: inner.loc.dupe(),
                    async_: inner.async_,
                    generator: inner.generator,
                    def: rename_tparams_in_fun_sig(rename_map, &inner.def),
                    statics: inner
                        .statics
                        .iter()
                        .map(|(name, (loc, t))| {
                            (
                                name.dupe(),
                                (loc.dupe(), rename_tparams_in_parsed(rename_map, t)),
                            )
                        })
                        .collect(),
                })),
                Value::DeclareModuleImplicitlyExportedObject(inner) => {
                    Value::DeclareModuleImplicitlyExportedObject(Box::new(
                        ValueDeclareModuleImplicitlyExportedObject {
                            loc: inner.loc.dupe(),
                            module_name: inner.module_name.clone(),
                            props: inner
                                .props
                                .iter()
                                .map(|(name, prop)| {
                                    (
                                        name.dupe(),
                                        rename_tparams_in_obj_value_prop(rename_map, prop),
                                    )
                                })
                                .collect(),
                            strictness_kind: inner.strictness_kind,
                        },
                    ))
                }
                Value::ObjLit(inner) => Value::ObjLit(Box::new(ValueObjLit {
                    loc: inner.loc.dupe(),
                    frozen: inner.frozen,
                    proto: inner
                        .proto
                        .as_ref()
                        .map(|(loc, t)| (loc.dupe(), rename_tparams_in_parsed(rename_map, t))),
                    props: inner
                        .props
                        .iter()
                        .map(|(name, prop)| {
                            (
                                name.dupe(),
                                rename_tparams_in_obj_value_prop(rename_map, prop),
                            )
                        })
                        .collect(),
                    strictness_kind: inner.strictness_kind,
                })),
                Value::ObjSpreadLit(inner) => Value::ObjSpreadLit(Box::new(ValueObjSpreadLit {
                    loc: inner.loc.dupe(),
                    frozen: inner.frozen,
                    proto: inner
                        .proto
                        .as_ref()
                        .map(|(loc, t)| (loc.dupe(), rename_tparams_in_parsed(rename_map, t))),
                    elems: inner.elems.mapped_ref(|elem| {
                        rename_tparams_in_obj_value_spread_elem(rename_map, elem)
                    }),
                    strictness_kind: inner.strictness_kind,
                })),
                Value::ArrayLit(inner) => Value::ArrayLit(Box::new(ValueArrayLit {
                    loc: inner.loc.dupe(),
                    elem_t: rename_tparams_in_parsed(rename_map, &inner.elem_t),
                    elements: inner
                        .elements
                        .iter()
                        .map(|t| rename_tparams_in_parsed(rename_map, t))
                        .collect(),
                    strictness_kind: inner.strictness_kind,
                })),
                Value::TemplateLiteral(inner) => {
                    Value::TemplateLiteral(Box::new(ValueTemplateLiteral {
                        loc: inner.loc.dupe(),
                        quasis: inner.quasis.clone(),
                        types: inner
                            .types
                            .iter()
                            .map(|t| rename_tparams_in_parsed(rename_map, t))
                            .collect(),
                    }))
                }
                Value::AsConst(value) => {
                    Value::AsConst(Box::new(rename_tparams_in_value(rename_map, value)))
                }
                Value::StringVal(_)
                | Value::StringLit(_)
                | Value::NumberVal(_)
                | Value::NumberLit(_)
                | Value::BigIntVal(_)
                | Value::BigIntLit(_)
                | Value::BooleanVal(_)
                | Value::BooleanLit(_)
                | Value::UniqueSymbol(_)
                | Value::NullLit(_)
                | Value::EmptyConstArrayLit(_) => value.clone(),
            }
        }

        fn rename_tparams_in_annot<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            annot: &ParsedAnnot<'arena, 'ast>,
        ) -> ParsedAnnot<'arena, 'ast> {
            match annot {
                Annot::Bound(inner) => Annot::Bound(Box::new(AnnotBound {
                    ref_loc: inner.ref_loc.dupe(),
                    name: rename_tparam_name(rename_map, &inner.name),
                })),
                Annot::FunAnnot(box (loc, def)) => Annot::FunAnnot(Box::new((
                    loc.dupe(),
                    rename_tparams_in_fun_sig(rename_map, def),
                ))),
                Annot::MethodAnnot(box (loc, is_static, def)) => Annot::MethodAnnot(Box::new((
                    loc.dupe(),
                    *is_static,
                    rename_tparams_in_fun_sig(rename_map, def),
                ))),
                Annot::ComponentAnnot(box (loc, def)) => Annot::ComponentAnnot(Box::new((
                    loc.dupe(),
                    rename_tparams_in_component_sig(rename_map, def),
                ))),
                Annot::Conditional(inner) => {
                    let (infer_tparams, infer_body_rename_map) =
                        rename_tparams_for_nested_scope(rename_map, &inner.infer_tparams);
                    let extends_type = override_bound_names_by_loc(
                        &tparams_name_locs(&infer_tparams),
                        &rename_tparams_in_parsed(rename_map, &inner.extends_type),
                    );
                    Annot::Conditional(Box::new(AnnotConditional {
                        loc: inner.loc.dupe(),
                        distributive_tparam: inner
                            .distributive_tparam
                            .as_ref()
                            .map(|tp| rename_distributive_tparam(rename_map, tp)),
                        infer_tparams,
                        check_type: rename_tparams_in_parsed(rename_map, &inner.check_type),
                        extends_type,
                        true_type: rename_tparams_in_parsed(
                            &infer_body_rename_map,
                            &inner.true_type,
                        ),
                        false_type: rename_tparams_in_parsed(rename_map, &inner.false_type),
                    }))
                }
                Annot::MappedTypeAnnot(inner) => {
                    let (key_tparam, property_type_rename_map) =
                        rename_tparam_for_nested_scope(rename_map, &inner.loc, &inner.key_tparam);
                    Annot::MappedTypeAnnot(Box::new(AnnotMappedTypeAnnot {
                        loc: inner.loc.dupe(),
                        source_type: rename_tparams_in_parsed(rename_map, &inner.source_type),
                        property_type: rename_tparams_in_parsed(
                            &property_type_rename_map,
                            &inner.property_type,
                        ),
                        name_type: inner
                            .name_type
                            .as_ref()
                            .map(|nt| rename_tparams_in_parsed(&property_type_rename_map, nt)),
                        key_tparam,
                        variance: inner.variance,
                        variance_op: inner.variance_op,
                        optional: inner.optional,
                        inline_keyof: inner.inline_keyof,
                        strictness_kind: inner.strictness_kind,
                    }))
                }
                Annot::ObjAnnot(inner) => Annot::ObjAnnot(Box::new(AnnotObjAnnot {
                    loc: inner.loc.dupe(),
                    obj_kind: rename_tparams_in_obj_kind(rename_map, &inner.obj_kind),
                    props: inner
                        .props
                        .iter()
                        .map(|(name, prop)| {
                            (
                                name.dupe(),
                                rename_tparams_in_obj_annot_prop(rename_map, prop),
                            )
                        })
                        .collect(),
                    computed_props: inner
                        .computed_props
                        .iter()
                        .map(|(key, prop)| {
                            (
                                rename_tparams_in_parsed(rename_map, key),
                                rename_tparams_in_obj_annot_prop(rename_map, prop),
                            )
                        })
                        .collect(),
                    proto: rename_tparams_in_obj_annot_proto(rename_map, &inner.proto),
                    strictness_kind: inner.strictness_kind,
                })),
                Annot::ObjSpreadAnnot(inner) => {
                    Annot::ObjSpreadAnnot(Box::new(AnnotObjSpreadAnnot {
                        loc: inner.loc.dupe(),
                        exact: inner.exact,
                        elems: inner.elems.mapped_ref(|elem| {
                            rename_tparams_in_obj_spread_annot_elem(rename_map, elem)
                        }),
                        strictness_kind: inner.strictness_kind,
                    }))
                }
                Annot::InlineInterface(box (loc, def)) => Annot::InlineInterface(Box::new((
                    loc.dupe(),
                    rename_tparams_in_interface_sig(rename_map, def),
                ))),
                _ => annot.map(
                    &mut (),
                    |_, loc| loc.dupe(),
                    |_, t| rename_tparams_in_parsed(rename_map, t),
                ),
            }
        }

        fn rename_tparams_in_tparam<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            alpha_map: &TParamRenameMap,
            tparam: &TParam<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> TParam<LocNode<'arena>, Parsed<'arena, 'ast>> {
            TParam {
                name_loc: tparam.name_loc.dupe(),
                name: rename_tparam_name(alpha_map, &tparam.name),
                polarity: tparam.polarity,
                bound: tparam
                    .bound
                    .as_ref()
                    .map(|t| rename_tparams_in_parsed(rename_map, t)),
                default: tparam
                    .default
                    .as_ref()
                    .map(|t| rename_tparams_in_parsed(rename_map, t)),
                is_const: tparam.is_const,
            }
        }

        fn rename_distributive_tparam<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            tparam: &TParam<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> TParam<LocNode<'arena>, Parsed<'arena, 'ast>> {
            TParam {
                name_loc: tparam.name_loc.dupe(),
                name: rename_tparam_name(rename_map, &tparam.name),
                polarity: tparam.polarity,
                bound: tparam
                    .bound
                    .as_ref()
                    .map(|t| rename_tparams_in_parsed(rename_map, t)),
                default: tparam
                    .default
                    .as_ref()
                    .map(|t| rename_tparams_in_parsed(rename_map, t)),
                is_const: tparam.is_const,
            }
        }

        fn rename_tparams_in_tparams_with_alpha<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            alpha_map: &TParamRenameMap,
            tparams: &TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> TParams<LocNode<'arena>, Parsed<'arena, 'ast>> {
            match tparams {
                TParams::Mono => TParams::Mono,
                TParams::Poly(poly) => {
                    let mut scoped_rename_map = rename_map.clone();
                    let tparams = poly
                        .tparams
                        .iter()
                        .map(|tparam| {
                            let renamed =
                                rename_tparams_in_tparam(&scoped_rename_map, alpha_map, tparam);
                            scoped_rename_map.remove(&tparam.name);
                            if let Some(fresh_name) = alpha_map.get(&tparam.name) {
                                scoped_rename_map.insert(tparam.name.dupe(), fresh_name.dupe());
                            }
                            renamed
                        })
                        .collect();
                    let tparams = Vec1::try_from_vec(tparams)
                        .expect("poly type parameters should be non-empty");
                    TParams::Poly(Box::new(PolyTParams {
                        loc: poly.loc.dupe(),
                        tparams,
                        bound_order: poly.bound_order.clone(),
                    }))
                }
            }
        }

        fn rename_tparams_for_nested_scope<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            tparams: &TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> (
            TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
            TParamRenameMap,
        ) {
            let (alpha_map, body_rename_map) = capture_avoidance_map(rename_map, tparams);
            (
                rename_tparams_in_tparams_with_alpha(rename_map, &alpha_map, tparams),
                body_rename_map,
            )
        }

        fn rename_tparam_for_nested_scope<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            loc: &LocNode<'arena>,
            tparam: &TParam<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> (
            TParam<LocNode<'arena>, Parsed<'arena, 'ast>>,
            TParamRenameMap,
        ) {
            let tparams = TParams::Poly(Box::new(PolyTParams {
                loc: loc.dupe(),
                tparams: Vec1::new((*tparam).clone()),
                bound_order: vec![0],
            }));
            let (tparams, body_rename_map) = rename_tparams_for_nested_scope(rename_map, &tparams);
            let TParams::Poly(poly) = tparams else {
                return ((*tparam).clone(), body_rename_map);
            };
            let tparam = poly
                .tparams
                .into_iter()
                .next()
                .expect("poly type parameters should be non-empty");
            (tparam, body_rename_map)
        }

        fn rename_tparams_in_fun_sig<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            sig: &FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> FunSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
            let (tparams, body_rename_map) =
                rename_tparams_for_nested_scope(rename_map, &sig.tparams);
            FunSig {
                tparams,
                params: sig
                    .params
                    .iter()
                    .map(|param| FunParam {
                        name: param.name.dupe(),
                        t: rename_tparams_in_parsed(&body_rename_map, &param.t),
                    })
                    .collect(),
                rest_param: sig.rest_param.as_ref().map(|param| FunRestParam {
                    name: param.name.dupe(),
                    loc: param.loc.dupe(),
                    t: rename_tparams_in_parsed(&body_rename_map, &param.t),
                }),
                this_param: sig
                    .this_param
                    .as_ref()
                    .map(|t| rename_tparams_in_parsed(&body_rename_map, t)),
                return_: rename_tparams_in_parsed(&body_rename_map, &sig.return_),
                type_guard: sig.type_guard.as_ref().map(|type_guard| TypeGuard {
                    loc: type_guard.loc.dupe(),
                    param_name: (
                        type_guard.param_name.0.dupe(),
                        type_guard.param_name.1.dupe(),
                    ),
                    type_guard: type_guard
                        .type_guard
                        .as_ref()
                        .map(|t| rename_tparams_in_parsed(&body_rename_map, t)),
                    kind: type_guard.kind,
                }),
                effect_: sig.effect_.clone(),
                strictness_kind: sig.strictness_kind,
            }
        }

        fn rename_tparams_in_component_sig<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            sig: &ComponentSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> ComponentSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
            let (tparams, body_rename_map) =
                rename_tparams_for_nested_scope(rename_map, &sig.tparams);
            ComponentSig {
                params_loc: sig.params_loc.dupe(),
                tparams,
                params: sig
                    .params
                    .iter()
                    .map(|param| ComponentParam {
                        name: param.name.dupe(),
                        name_loc: param.name_loc.dupe(),
                        t: rename_tparams_in_parsed(&body_rename_map, &param.t),
                    })
                    .collect(),
                rest_param: sig.rest_param.as_ref().map(|param| ComponentRestParam {
                    t: rename_tparams_in_parsed(&body_rename_map, &param.t),
                }),
                renders: rename_tparams_in_parsed(&body_rename_map, &sig.renders),
                strictness_kind: sig.strictness_kind,
            }
        }

        fn rename_tparams_in_accessor<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            accessor: &Accessor<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> Accessor<LocNode<'arena>, Parsed<'arena, 'ast>> {
            match accessor {
                Accessor::Get(box (loc, t)) => Accessor::Get(Box::new((
                    loc.dupe(),
                    rename_tparams_in_parsed(rename_map, t),
                ))),
                Accessor::Set(box (loc, t)) => Accessor::Set(Box::new((
                    loc.dupe(),
                    rename_tparams_in_parsed(rename_map, t),
                ))),
                Accessor::GetSet(box (get_loc, get_t, set_loc, set_t)) => {
                    Accessor::GetSet(Box::new((
                        get_loc.dupe(),
                        rename_tparams_in_parsed(rename_map, get_t),
                        set_loc.dupe(),
                        rename_tparams_in_parsed(rename_map, set_t),
                    )))
                }
            }
        }

        fn rename_tparams_in_interface_prop<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            prop: &InterfaceProp<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> InterfaceProp<LocNode<'arena>, Parsed<'arena, 'ast>> {
            match prop {
                InterfaceProp::InterfaceField(box (id_loc, t, polarity)) => {
                    InterfaceProp::InterfaceField(Box::new((
                        id_loc.dupe(),
                        rename_tparams_in_parsed(rename_map, t),
                        *polarity,
                    )))
                }
                InterfaceProp::InterfaceAccess(accessor) => InterfaceProp::InterfaceAccess(
                    Box::new(rename_tparams_in_accessor(rename_map, accessor)),
                ),
                InterfaceProp::InterfaceMethod(methods) => InterfaceProp::InterfaceMethod(
                    Box::new(methods.mapped_ref(|(id_loc, fn_loc, def)| {
                        (
                            id_loc.dupe(),
                            fn_loc.dupe(),
                            rename_tparams_in_fun_sig(rename_map, def),
                        )
                    })),
                ),
            }
        }

        fn rename_tparams_in_obj_annot_dict<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            dict: &ObjAnnotDict<Parsed<'arena, 'ast>>,
        ) -> ObjAnnotDict<Parsed<'arena, 'ast>> {
            ObjAnnotDict {
                name: dict.name.dupe(),
                polarity: dict.polarity,
                key: rename_tparams_in_parsed(rename_map, &dict.key),
                value: rename_tparams_in_parsed(rename_map, &dict.value),
            }
        }

        fn rename_tparams_in_obj_kind<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            obj_kind: &ObjKind<Parsed<'arena, 'ast>>,
        ) -> ObjKind<Parsed<'arena, 'ast>> {
            match obj_kind {
                ObjKind::ExactObj => ObjKind::ExactObj,
                ObjKind::InexactObj => ObjKind::InexactObj,
                ObjKind::IndexedObj(dict) => ObjKind::IndexedObj(Box::new(
                    rename_tparams_in_obj_annot_dict(rename_map, dict),
                )),
            }
        }

        fn rename_tparams_in_obj_annot_prop<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            prop: &ObjAnnotProp<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> ObjAnnotProp<LocNode<'arena>, Parsed<'arena, 'ast>> {
            match prop {
                ObjAnnotProp::ObjAnnotField(box (loc, t, polarity)) => {
                    ObjAnnotProp::ObjAnnotField(Box::new((
                        loc.dupe(),
                        rename_tparams_in_parsed(rename_map, t),
                        *polarity,
                    )))
                }
                ObjAnnotProp::ObjAnnotAccess(accessor) => ObjAnnotProp::ObjAnnotAccess(Box::new(
                    rename_tparams_in_accessor(rename_map, accessor),
                )),
                ObjAnnotProp::ObjAnnotMethod(inner) => {
                    ObjAnnotProp::ObjAnnotMethod(Box::new(ObjAnnotMethodData {
                        id_loc: inner.id_loc.dupe(),
                        fn_loc: inner.fn_loc.dupe(),
                        def: rename_tparams_in_fun_sig(rename_map, &inner.def),
                    }))
                }
            }
        }

        fn rename_tparams_in_obj_annot_proto<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            proto: &ObjAnnotProto<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> ObjAnnotProto<LocNode<'arena>, Parsed<'arena, 'ast>> {
            match proto {
                ObjAnnotProto::ObjAnnotImplicitProto => ObjAnnotProto::ObjAnnotImplicitProto,
                ObjAnnotProto::ObjAnnotExplicitProto(box (loc, t)) => {
                    ObjAnnotProto::ObjAnnotExplicitProto(Box::new((
                        loc.dupe(),
                        rename_tparams_in_parsed(rename_map, t),
                    )))
                }
                ObjAnnotProto::ObjAnnotCallable(ts) => ObjAnnotProto::ObjAnnotCallable(Box::new(
                    ts.mapped_ref(|t| rename_tparams_in_parsed(rename_map, t)),
                )),
            }
        }

        fn rename_tparams_in_obj_spread_annot_elem<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            elem: &ObjSpreadAnnotElem<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> ObjSpreadAnnotElem<LocNode<'arena>, Parsed<'arena, 'ast>> {
            match elem {
                ObjSpreadAnnotElem::ObjSpreadAnnotElem(t) => {
                    ObjSpreadAnnotElem::ObjSpreadAnnotElem(Box::new(rename_tparams_in_parsed(
                        rename_map, t,
                    )))
                }
                ObjSpreadAnnotElem::ObjSpreadAnnotSlice(inner) => {
                    ObjSpreadAnnotElem::ObjSpreadAnnotSlice(Box::new(ObjSpreadAnnotSliceData {
                        dict: inner
                            .dict
                            .as_ref()
                            .map(|dict| rename_tparams_in_obj_annot_dict(rename_map, dict)),
                        props: inner
                            .props
                            .iter()
                            .map(|(name, prop)| {
                                (
                                    name.dupe(),
                                    rename_tparams_in_obj_annot_prop(rename_map, prop),
                                )
                            })
                            .collect(),
                        computed_props: inner
                            .computed_props
                            .iter()
                            .map(|(key, prop)| {
                                (
                                    rename_tparams_in_parsed(rename_map, key),
                                    rename_tparams_in_obj_annot_prop(rename_map, prop),
                                )
                            })
                            .collect(),
                    }))
                }
            }
        }

        fn rename_tparams_in_obj_value_prop<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            prop: &ObjValueProp<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> ObjValueProp<LocNode<'arena>, Parsed<'arena, 'ast>> {
            match prop {
                ObjValueProp::ObjValueField(box (loc, t, polarity)) => {
                    ObjValueProp::ObjValueField(Box::new((
                        loc.dupe(),
                        rename_tparams_in_parsed(rename_map, t),
                        *polarity,
                    )))
                }
                ObjValueProp::ObjValueAccess(accessor) => ObjValueProp::ObjValueAccess(Box::new(
                    rename_tparams_in_accessor(rename_map, accessor),
                )),
                ObjValueProp::ObjValueMethod(ms) => {
                    ObjValueProp::ObjValueMethod(Box::new(ms.mapped_ref(
                        |ObjValueMethodData {
                             id_loc,
                             fn_loc,
                             async_,
                             generator,
                             uses_this,
                             def,
                         }| ObjValueMethodData {
                            id_loc: id_loc.dupe(),
                            fn_loc: fn_loc.dupe(),
                            async_: *async_,
                            generator: *generator,
                            uses_this: *uses_this,
                            def: rename_tparams_in_fun_sig(rename_map, def),
                        },
                    )))
                }
            }
        }

        fn rename_tparams_in_obj_value_spread_elem<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            elem: &ObjValueSpreadElem<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> ObjValueSpreadElem<LocNode<'arena>, Parsed<'arena, 'ast>> {
            match elem {
                ObjValueSpreadElem::ObjValueSpreadElem(t) => {
                    ObjValueSpreadElem::ObjValueSpreadElem(Box::new(rename_tparams_in_parsed(
                        rename_map, t,
                    )))
                }
                ObjValueSpreadElem::ObjValueSpreadSlice(props) => {
                    ObjValueSpreadElem::ObjValueSpreadSlice(Box::new(
                        props
                            .iter()
                            .map(|(name, prop)| {
                                (
                                    name.dupe(),
                                    rename_tparams_in_obj_value_prop(rename_map, prop),
                                )
                            })
                            .collect(),
                    ))
                }
            }
        }

        fn rename_tparams_in_class_extends<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            extends: &ClassExtends<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> ClassExtends<LocNode<'arena>, Parsed<'arena, 'ast>> {
            match extends {
                ClassExtends::ClassExplicitExtends(box (loc, t)) => {
                    ClassExtends::ClassExplicitExtends(Box::new((
                        loc.dupe(),
                        rename_tparams_in_parsed(rename_map, t),
                    )))
                }
                ClassExtends::ClassExplicitExtendsApp(box (loc, t, targs)) => {
                    ClassExtends::ClassExplicitExtendsApp(Box::new((
                        loc.dupe(),
                        rename_tparams_in_parsed(rename_map, t),
                        targs
                            .iter()
                            .map(|t| rename_tparams_in_parsed(rename_map, t))
                            .collect(),
                    )))
                }
                ClassExtends::ClassImplicitExtends => ClassExtends::ClassImplicitExtends,
                ClassExtends::ObjectPrototypeExtendsNull => {
                    ClassExtends::ObjectPrototypeExtendsNull
                }
            }
        }

        fn rename_tparams_in_class_sig<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            sig: &ClassSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> ClassSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
            let (tparams, body_rename_map) =
                rename_tparams_for_nested_scope(rename_map, &sig.tparams);
            ClassSig {
                tparams,
                extends: rename_tparams_in_class_extends(&body_rename_map, &sig.extends),
                implements: sig
                    .implements
                    .iter()
                    .map(|t| rename_tparams_in_parsed(&body_rename_map, t))
                    .collect(),
                static_props: sig
                    .static_props
                    .iter()
                    .map(|(name, prop)| {
                        (
                            name.dupe(),
                            rename_tparams_in_obj_value_prop(&body_rename_map, prop),
                        )
                    })
                    .collect(),
                proto_props: sig
                    .proto_props
                    .iter()
                    .map(|(name, prop)| {
                        (
                            name.dupe(),
                            rename_tparams_in_obj_value_prop(&body_rename_map, prop),
                        )
                    })
                    .collect(),
                own_props: sig
                    .own_props
                    .iter()
                    .map(|(name, prop)| {
                        (
                            name.dupe(),
                            rename_tparams_in_obj_value_prop(&body_rename_map, prop),
                        )
                    })
                    .collect(),
                computed_own_props: sig
                    .computed_own_props
                    .iter()
                    .map(|(key, prop)| {
                        (
                            rename_tparams_in_parsed(&body_rename_map, key),
                            rename_tparams_in_obj_value_prop(&body_rename_map, prop),
                        )
                    })
                    .collect(),
                computed_proto_props: sig
                    .computed_proto_props
                    .iter()
                    .map(|(key, prop)| {
                        (
                            rename_tparams_in_parsed(&body_rename_map, key),
                            rename_tparams_in_obj_value_prop(&body_rename_map, prop),
                        )
                    })
                    .collect(),
                computed_static_props: sig
                    .computed_static_props
                    .iter()
                    .map(|(key, prop)| {
                        (
                            rename_tparams_in_parsed(&body_rename_map, key),
                            rename_tparams_in_obj_value_prop(&body_rename_map, prop),
                        )
                    })
                    .collect(),
                dict: sig
                    .dict
                    .as_ref()
                    .map(|dict| rename_tparams_in_obj_annot_dict(&body_rename_map, dict)),
                abstract_: sig.abstract_,
                abstract_props: sig.abstract_props.clone(),
                strictness_kind: sig.strictness_kind,
            }
        }

        fn rename_tparams_in_interface_sig<'arena, 'ast>(
            rename_map: &TParamRenameMap,
            sig: &InterfaceSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> InterfaceSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
            InterfaceSig {
                extends: sig
                    .extends
                    .iter()
                    .map(|t| rename_tparams_in_parsed(rename_map, t))
                    .collect(),
                props: sig
                    .props
                    .iter()
                    .map(|(name, prop)| {
                        (
                            name.dupe(),
                            rename_tparams_in_interface_prop(rename_map, prop),
                        )
                    })
                    .collect(),
                computed_props: sig
                    .computed_props
                    .iter()
                    .map(|(key, prop)| {
                        (
                            rename_tparams_in_parsed(rename_map, key),
                            rename_tparams_in_interface_prop(rename_map, prop),
                        )
                    })
                    .collect(),
                calls: sig
                    .calls
                    .iter()
                    .map(|t| rename_tparams_in_parsed(rename_map, t))
                    .collect(),
                constructs: sig
                    .constructs
                    .iter()
                    .map(|t| rename_tparams_in_parsed(rename_map, t))
                    .collect(),
                dict: sig
                    .dict
                    .as_ref()
                    .map(|dict| rename_tparams_in_obj_annot_dict(rename_map, dict)),
                abstract_: sig.abstract_,
                strictness_kind: sig.strictness_kind,
            }
        }

        pub(super) fn merge_tparams<'arena, 'ast>(
            existing_tparams: &mut TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
            current_tparams: &TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> bool {
            match (existing_tparams, current_tparams) {
                (TParams::Mono, TParams::Mono) => true,
                (TParams::Poly(existing), TParams::Poly(current))
                    if existing.tparams.len() == current.tparams.len() =>
                {
                    if existing
                        .tparams
                        .iter()
                        .zip(&current.tparams)
                        .any(|(existing, current)| {
                            existing.polarity.is_some()
                                && current.polarity.is_some()
                                && existing.polarity != current.polarity
                        })
                    {
                        return false;
                    }
                    let rename_map = current
                        .tparams
                        .iter()
                        .zip(existing.tparams.iter())
                        .map(|(current, existing)| (current.name.dupe(), existing.name.dupe()))
                        .collect();
                    for (existing_tparam, current_tparam) in
                        existing.tparams.iter_mut().zip(&current.tparams)
                    {
                        if existing_tparam.polarity.is_none() {
                            existing_tparam.polarity = current_tparam.polarity;
                        }
                        if existing_tparam.default.is_none() {
                            existing_tparam.default = current_tparam
                                .default
                                .as_ref()
                                .map(|default| rename_tparams_in_parsed(&rename_map, default));
                        }
                    }
                    true
                }
                _ => false,
            }
        }

        pub(super) fn rename_interface_sig<'arena, 'ast>(
            from_tparams: &TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
            to_tparams: &TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
            sig: &InterfaceSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) -> InterfaceSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
            let rename_map = tparam_rename_map(from_tparams, to_tparams);
            rename_tparams_in_interface_sig(&rename_map, sig)
        }
    }

    fn merge_interface_props_combine<'arena, 'ast>(
        _prop_name: &FlowSmolStr,
        existing_prop: &InterfaceProp<LocNode<'arena>, Parsed<'arena, 'ast>>,
        new_prop: &InterfaceProp<LocNode<'arena>, Parsed<'arena, 'ast>>,
    ) -> Option<InterfaceProp<LocNode<'arena>, Parsed<'arena, 'ast>>> {
        match (existing_prop, new_prop) {
            (
                InterfaceProp::InterfaceMethod(old_sigs),
                InterfaceProp::InterfaceMethod(new_sigs),
            ) => {
                let mut merged: Vec<_> = new_sigs.iter().cloned().collect();
                merged.extend(old_sigs.iter().cloned());
                let merged = Vec1::try_from_vec(merged).unwrap();
                Some(InterfaceProp::InterfaceMethod(Box::new(merged)))
            }
            _ => Some(existing_prop.clone()),
        }
    }

    // Fold an [interface_sig] into an existing [declare_class_sig], producing a
    // new [declare_class_sig] whose own/proto props absorb the interface's props
    // and whose implements list gains the interface's extends. The class wins on
    // overlapping fields; overlapping methods build an overload chain. Type
    // mismatches between merged-in fields and class fields are caught later by
    // post-inference unification (see Context.interface_merge_unify_tasks).
    fn merge_interface_into_declare_class_sig<'arena, 'ast>(
        existing_id_loc: LocNode<'arena>,
        current_id_loc: LocNode<'arena>,
        tbls: &mut Tables<'arena, 'ast>,
        dc_sig: &DeclareClassSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
        iface_tparams: &TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
        iface_sig: &InterfaceSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
    ) -> DeclareClassSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
        let iface_sig =
            type_param_renaming::rename_interface_sig(iface_tparams, &dc_sig.tparams, iface_sig);
        let mut proto_props = dc_sig.proto_props.clone();
        for (prop_name, new_prop) in &iface_sig.props {
            use std::collections::btree_map::Entry;
            match proto_props.entry(prop_name.clone()) {
                Entry::Occupied(mut e) => {
                    if let Some(combined) =
                        merge_interface_props_combine(prop_name, e.get(), new_prop)
                    {
                        *e.get_mut() = combined;
                    }
                }
                Entry::Vacant(e) => {
                    e.insert(new_prop.clone());
                }
            }
        }
        let mut computed_proto_props = dc_sig.computed_proto_props.clone();
        computed_proto_props.extend(iface_sig.computed_props.iter().cloned());
        let mut calls = dc_sig.calls.clone();
        calls.extend(iface_sig.calls.iter().cloned());
        let mut constructs = dc_sig.constructs.clone();
        constructs.extend(iface_sig.constructs.iter().cloned());
        let mut implements = dc_sig.implements.clone();
        implements.extend(iface_sig.extends.iter().cloned());
        let dict = match (&dc_sig.dict, &iface_sig.dict) {
            (Some(existing_dict), Some(_)) => {
                tbls.additional_errors.push(
                    signature_error::BindingValidation::InterfaceMergePropertyConflict {
                        name: FlowSmolStr::new_inline("[indexer]"),
                        current_binding_loc: current_id_loc,
                        existing_binding_loc: existing_id_loc,
                    },
                );
                Some(existing_dict.clone())
            }
            (Some(_), None) => dc_sig.dict.clone(),
            (None, _) => iface_sig.dict.clone(),
        };
        DeclareClassSig {
            tparams: dc_sig.tparams.clone(),
            extends: dc_sig.extends.clone(),
            mixins: dc_sig.mixins.clone(),
            implements,
            static_props: dc_sig.static_props.clone(),
            own_props: dc_sig.own_props.clone(),
            proto_props,
            computed_own_props: dc_sig.computed_own_props.clone(),
            computed_proto_props,
            computed_static_props: dc_sig.computed_static_props.clone(),
            static_calls: dc_sig.static_calls.clone(),
            calls,
            constructs,
            dict,
            static_dict: dc_sig.static_dict.clone(),
            abstract_: dc_sig.abstract_,
            abstract_props: dc_sig.abstract_props.clone(),
            strictness_kind: dc_sig.strictness_kind.join(iface_sig.strictness_kind),
        }
    }

    fn merge_interface_sigs<'arena, 'ast>(
        existing_id_loc: LocNode<'arena>,
        current_id_loc: LocNode<'arena>,
        tbls: &mut Tables<'arena, 'ast>,
        old_tparams: &TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
        new_tparams: &TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
        old_sig: &InterfaceSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
        new_sig: &InterfaceSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
    ) -> InterfaceSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
        let new_sig = type_param_renaming::rename_interface_sig(new_tparams, old_tparams, new_sig);
        // props: union, first wins; methods merge overloads, fields conflict
        let mut props = old_sig.props.clone();
        for (prop_name, new_prop) in &new_sig.props {
            use std::collections::btree_map::Entry;
            match props.entry(prop_name.clone()) {
                Entry::Occupied(mut e) => {
                    if let (
                        InterfaceProp::InterfaceMethod(old_sigs),
                        InterfaceProp::InterfaceMethod(new_sigs),
                    ) = (e.get(), new_prop)
                    {
                        // merge overloads
                        let mut merged: Vec<_> = new_sigs.iter().cloned().collect();
                        merged.extend(old_sigs.iter().cloned());
                        let merged = Vec1::try_from_vec(merged).unwrap();
                        *e.get_mut() = InterfaceProp::InterfaceMethod(Box::new(merged));
                    } else {
                        // First prop wins. Type conflict checking is deferred to
                        // post-inference via Flow.unify.
                    }
                }
                Entry::Vacant(e) => {
                    e.insert(new_prop.clone());
                }
            }
        }

        // extends: concatenate
        let mut extends = old_sig.extends.clone();
        extends.extend(new_sig.extends.iter().cloned());

        // computed_props: concatenate
        let mut computed_props = old_sig.computed_props.clone();
        computed_props.extend(new_sig.computed_props.iter().cloned());

        // calls: concatenate
        let mut calls = old_sig.calls.clone();
        calls.extend(new_sig.calls.iter().cloned());

        let mut constructs = old_sig.constructs.clone();
        constructs.extend(new_sig.constructs.iter().cloned());

        // dict: first wins if both exist, error
        let dict = match (&old_sig.dict, &new_sig.dict) {
            (Some(_), Some(_)) => {
                tbls.additional_errors.push(
                    signature_error::BindingValidation::InterfaceMergePropertyConflict {
                        name: FlowSmolStr::new_inline("[indexer]"),
                        current_binding_loc: current_id_loc,
                        existing_binding_loc: existing_id_loc,
                    },
                );
                old_sig.dict.clone()
            }
            (Some(_), None) => old_sig.dict.clone(),
            (None, _) => new_sig.dict.clone(),
        };

        // Merge target abstractness: declarations + interfaces only set
        // [abstract=false]; the only [abstract=true] producer (inline
        // interface from [abstract new () => T]) is never merged. OR for
        // defensive correctness anyway.
        let abstract_ = old_sig.abstract_ || new_sig.abstract_;

        InterfaceSig {
            extends,
            props,
            computed_props,
            calls,
            constructs,
            dict,
            abstract_,
            strictness_kind: old_sig.strictness_kind.join(new_sig.strictness_kind),
        }
    }

    pub(super) fn bind_interface<'arena: 'ast, 'ast>(
        opts: &TypeSigOptions,
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        new_def: Lazy<'arena, 'ast, ParsedDef<'arena, 'ast>>,
        k: impl Fn(&mut Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
    ) {
        report_cross_namespace_conflict(
            opts,
            id,
            scopes,
            tbls,
            &name,
            id_loc.dupe(),
            true,
            IncomingBindingKind::Interface,
        );
        // Declaration merging: if a `declare class` of the same name already exists
        // in the value namespace, fold the interface's members into it. The class
        // remains the canonical entity and is reachable from both value and type
        // positions (type lookups fall back to values when the type namespace has
        // no entry).
        let merged_class_node: Option<LocalDefNode<'arena, 'ast>> =
            match lookup_local_value(scopes, id, &name) {
                Some(BindingNode::LocalBinding(class_node)) => {
                    let is_dc = matches!(
                        &*class_node.0.data(),
                        LocalBinding::DeclareClassBinding { .. }
                    );
                    if is_dc { Some(class_node) } else { None }
                }
                _ => None,
            };
        if let Some(class_node) = merged_class_node {
            let (dc_id_loc, dc_name, dc_namespace_values, dc_namespace_types) = {
                let data = class_node.0.data();
                match &*data {
                    LocalBinding::DeclareClassBinding {
                        id_loc,
                        name,
                        namespace_values,
                        namespace_types,
                        ..
                    } => (
                        id_loc.dupe(),
                        name.dupe(),
                        namespace_values.clone(),
                        namespace_types.clone(),
                    ),
                    _ => unreachable!(),
                }
            };
            let dc_def = {
                let mut data = class_node.0.data_mut();
                let old = std::mem::replace(
                    &mut *data,
                    LocalBinding::DeclareClassBinding {
                        id_loc: dc_id_loc.dupe(),
                        name: dc_name.dupe(),
                        def: tbls.lazy(Box::new(|_, _, _| {
                            unreachable!("placeholder during interface merge")
                        })),
                        namespace_values: dc_namespace_values.clone(),
                        namespace_types: dc_namespace_types.clone(),
                    },
                );
                match old {
                    LocalBinding::DeclareClassBinding { def, .. } => def,
                    _ => unreachable!(),
                }
            };
            let merge_dc_id_loc = dc_id_loc.dupe();
            let merge_name = name.dupe();
            let merged_def = tbls.lazy(Box::new(
                move |opts, scopes, tbls: &mut Tables<'arena, 'ast>| {
                    let mut forced_dc = dc_def.get_forced(opts, scopes, tbls).clone();
                    match new_def.get_forced(opts, scopes, tbls) {
                        Def::Interface(iface) => {
                            if type_param_renaming::merge_tparams(
                                &mut forced_dc.tparams,
                                &iface.tparams,
                            ) {
                                let iface_id_loc_local = iface.id_loc.dupe();
                                let iface_def_local = iface.def.clone();
                                merge_interface_into_declare_class_sig(
                                    merge_dc_id_loc.dupe(),
                                    iface_id_loc_local,
                                    tbls,
                                    &forced_dc,
                                    &iface.tparams,
                                    &iface_def_local,
                                )
                            } else {
                                tbls.additional_errors.push(
                                    signature_error::BindingValidation::InterfaceMergeTparamMismatch {
                                        name: merge_name,
                                        current_binding_loc: iface.id_loc.dupe(),
                                        existing_binding_loc: merge_dc_id_loc,
                                    },
                                );
                                forced_dc
                            }
                        }
                        _ => forced_dc,
                    }
                },
            ));
            {
                let mut data = class_node.0.data_mut();
                *data = LocalBinding::DeclareClassBinding {
                    id_loc: dc_id_loc,
                    name: dc_name,
                    def: merged_def,
                    namespace_values: dc_namespace_values,
                    namespace_types: dc_namespace_types,
                };
            }
            return;
        }
        bind(
            true,
            id,
            scopes,
            tbls,
            &name,
            id_loc.dupe(),
            |scopes, tbls, binding_opt| match binding_opt {
                Some(BindingNode::LocalBinding(existing_node)) => {
                    // Check if existing is a TypeBinding; if so, create merged lazy
                    let existing_is_type_binding =
                        matches!(&*existing_node.0.data(), LocalBinding::TypeBinding { .. });
                    if existing_is_type_binding {
                        let existing_id_loc = {
                            let data = existing_node.0.data();
                            match &*data {
                                LocalBinding::TypeBinding { id_loc, .. } => id_loc.dupe(),
                                _ => unreachable!(),
                            }
                        };
                        // Take old def out, replace with merged lazy
                        let old_def = {
                            let mut data = existing_node.0.data_mut();
                            let old = std::mem::replace(
                                &mut *data,
                                LocalBinding::TypeBinding {
                                    id_loc: existing_id_loc.dupe(),
                                    def: tbls.lazy(Box::new(|_, _, _| unreachable!("placeholder"))),
                                },
                            );
                            match old {
                                LocalBinding::TypeBinding { def, .. } => def,
                                _ => unreachable!(),
                            }
                        };

                        let merge_existing_id_loc = existing_id_loc.dupe();
                        let merge_id_loc = id_loc.dupe();
                        let merge_name = name.dupe();
                        let merged_def = tbls.lazy(Box::new(
                            move |opts, scopes, tbls: &mut Tables<'arena, 'ast>| {
                                let old_val = old_def.get_forced(opts, scopes, tbls);
                                let new_val = new_def.get_forced(opts, scopes, tbls);
                                match (old_val, new_val) {
                                    (Def::Interface(old_iface), Def::Interface(new_iface)) => {
                                        let mut merged_tparams = old_iface.tparams.clone();
                                        if type_param_renaming::merge_tparams(
                                            &mut merged_tparams,
                                            &new_iface.tparams,
                                        ) {
                                            let merged_sig = merge_interface_sigs(
                                                old_iface.id_loc.dupe(),
                                                new_iface.id_loc.dupe(),
                                                tbls,
                                                &old_iface.tparams,
                                                &new_iface.tparams,
                                                &old_iface.def,
                                                &new_iface.def,
                                            );
                                            Def::Interface(Box::new(DefInterface {
                                                id_loc: old_iface.id_loc.dupe(),
                                                name: old_iface.name.dupe(),
                                                tparams: merged_tparams,
                                                def: merged_sig,
                                            }))
                                        } else {
                                            tbls.additional_errors.push(
                                                signature_error::BindingValidation::InterfaceMergeTparamMismatch {
                                                    name: merge_name,
                                                    current_binding_loc: new_iface.id_loc.dupe(),
                                                    existing_binding_loc: old_iface.id_loc.dupe(),
                                                },
                                            );
                                            old_val.clone()
                                        }
                                    }
                                    _ => {
                                        // Existing binding is not an interface — emit NameOverride
                                        tbls.additional_errors.push(
                                            signature_error::BindingValidation::NameOverride {
                                                name: merge_name,
                                                override_binding_loc: merge_existing_id_loc,
                                                existing_binding_loc: merge_id_loc,
                                            },
                                        );
                                        old_val.clone()
                                    }
                                }
                            },
                        ));

                        {
                            let mut data = existing_node.0.data_mut();
                            *data = LocalBinding::TypeBinding {
                                id_loc: existing_id_loc,
                                def: merged_def,
                            };
                        }
                        (BindingNode::LocalBinding(existing_node), true)
                    } else {
                        (BindingNode::LocalBinding(existing_node), false)
                    }
                }
                None => {
                    let node = tbls.push_local_def(LocalBinding::TypeBinding {
                        id_loc,
                        def: new_def,
                    });
                    k(scopes, &name, node.dupe());
                    (BindingNode::LocalBinding(node), true)
                }
                Some(b) => (b, false),
            },
        );
    }

    pub(super) fn bind_class<'arena, 'ast>(
        opts: &TypeSigOptions,
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Lazy<'arena, 'ast, ClassSig<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        k: impl Fn(&mut Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
    ) {
        report_cross_namespace_conflict(
            opts,
            id,
            scopes,
            tbls,
            &name,
            id_loc.dupe(),
            false,
            IncomingBindingKind::Class,
        );
        bind_local(
            false,
            id,
            scopes,
            tbls,
            name.clone(),
            id_loc.dupe(),
            LocalBinding::ClassBinding {
                id_loc,
                name: name.clone(),
                def,
                namespace_values: BTreeMap::new(),
                namespace_types: BTreeMap::new(),
            },
            k,
        );
    }

    pub(super) fn bind_declare_class<'arena: 'ast, 'ast>(
        opts: &TypeSigOptions,
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Lazy<'arena, 'ast, DeclareClassSig<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        k: impl Fn(&mut Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
    ) {
        report_cross_namespace_conflict(
            opts,
            id,
            scopes,
            tbls,
            &name,
            id_loc.dupe(),
            false,
            IncomingBindingKind::DeclareClass,
        );
        // Declaration merging: if a same-name `interface` was declared first, fold
        // its members into this declare class. We mutate the existing type-side
        // node to BECOME a DeclareClassBinding (so type lookups find the merged
        // class), and then re-use the same node when binding in values.
        let prior_interface_node: Option<(
            LocalDefNode<'arena, 'ast>,
            LocNode<'arena>,
            TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
            InterfaceSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
        )> = {
            let candidate_type_node: Option<LocalDefNode<'arena, 'ast>> =
                match lookup_local_type(scopes, id, &name) {
                    Some(BindingNode::LocalBinding(node)) => {
                        let is_tb = matches!(&*node.0.data(), LocalBinding::TypeBinding { .. });
                        if is_tb { Some(node) } else { None }
                    }
                    _ => None,
                };
            if let Some(type_node) = candidate_type_node {
                let tb_id_loc = {
                    let data = type_node.0.data();
                    match &*data {
                        LocalBinding::TypeBinding { id_loc, .. } => id_loc.dupe(),
                        _ => unreachable!(),
                    }
                };
                let lazy_taken = {
                    let mut data = type_node.0.data_mut();
                    let old = std::mem::replace(
                        &mut *data,
                        LocalBinding::TypeBinding {
                            id_loc: tb_id_loc.dupe(),
                            def: tbls.lazy(Box::new(|_, _, _| {
                                unreachable!("placeholder during declare-class peek")
                            })),
                        },
                    );
                    match old {
                        LocalBinding::TypeBinding { def, .. } => def,
                        _ => unreachable!(),
                    }
                };
                let captured = match lazy_taken.get_forced(opts, scopes, tbls) {
                    Def::Interface(iface) => Some((
                        iface.id_loc.dupe(),
                        iface.tparams.clone(),
                        iface.def.clone(),
                    )),
                    _ => None,
                };
                {
                    let mut data = type_node.0.data_mut();
                    *data = LocalBinding::TypeBinding {
                        id_loc: tb_id_loc,
                        def: lazy_taken,
                    };
                }
                captured.map(|(iface_id_loc, iface_tp, iface_sig)| {
                    (type_node, iface_id_loc, iface_tp, iface_sig)
                })
            } else {
                None
            }
        };
        let bind_name = name.clone();
        bind(
            false,
            id,
            scopes,
            tbls,
            &bind_name,
            id_loc.dupe(),
            move |scopes, tbls, binding_opt| match binding_opt {
                None => {
                    let merged_def = match prior_interface_node.as_ref() {
                        None => def,
                        Some((_type_node, iface_id_loc, iface_tp, iface_sig)) => {
                            let iface_id_loc = iface_id_loc.dupe();
                            let iface_tp = iface_tp.clone();
                            let iface_sig = iface_sig.clone();
                            let merge_id_loc = id_loc.dupe();
                            let merge_name = name.dupe();
                            tbls.lazy(Box::new(
                                move |opts, scopes, tbls: &mut Tables<'arena, 'ast>| {
                                    let mut forced_dc = def.get_forced(opts, scopes, tbls).clone();
                                    if type_param_renaming::merge_tparams(
                                        &mut forced_dc.tparams,
                                        &iface_tp,
                                    ) {
                                        merge_interface_into_declare_class_sig(
                                            merge_id_loc.dupe(),
                                            iface_id_loc.dupe(),
                                            tbls,
                                            &forced_dc,
                                            &iface_tp,
                                            &iface_sig,
                                        )
                                    } else {
                                        tbls.additional_errors.push(
                                            signature_error::BindingValidation::InterfaceMergeTparamMismatch {
                                                name: merge_name,
                                                current_binding_loc: iface_id_loc,
                                                existing_binding_loc: merge_id_loc,
                                            },
                                        );
                                        forced_dc
                                    }
                                },
                            ))
                        }
                    };
                    let dc_local = LocalBinding::DeclareClassBinding {
                        id_loc,
                        name: name.clone(),
                        def: merged_def,
                        namespace_values: BTreeMap::new(),
                        namespace_types: BTreeMap::new(),
                    };
                    let node = match prior_interface_node {
                        Some((type_node, _, _, _)) => {
                            // Re-use the existing type-side node so that both type- and
                            // value-side lookups resolve to the same merged class.
                            {
                                let mut data = type_node.0.data_mut();
                                *data = dc_local;
                            }
                            type_node
                        }
                        None => push_value_def_with_namespace(scopes, tbls, id, dc_local),
                    };
                    k(scopes, &name, node.dupe());
                    (BindingNode::LocalBinding(node), true)
                }
                Some(b @ BindingNode::RemoteBinding(_)) => (b, false),
                Some(BindingNode::LocalBinding(node)) => (BindingNode::LocalBinding(node), false),
            },
        );
    }

    pub(super) fn bind_record<'arena, 'ast>(
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Option<Lazy<'arena, 'ast, ClassSig<LocNode<'arena>, Parsed<'arena, 'ast>>>>,
        defaulted_props: std::collections::BTreeSet<FlowSmolStr>,
        k: impl Fn(&mut Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
    ) {
        bind_local(
            false,
            id,
            scopes,
            tbls,
            name.clone(),
            id_loc.dupe(),
            LocalBinding::RecordBinding {
                id_loc,
                name: name.clone(),
                def,
                defaulted_props,
            },
            k,
        );
    }

    pub(super) fn bind_enum<'arena, 'ast>(
        opts: &TypeSigOptions,
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Option<
            Lazy<
                'arena,
                'ast,
                (
                    Option<EnumRep>,
                    BTreeMap<FlowSmolStr, LocNode<'arena>>,
                    Option<BTreeMap<FlowSmolStr, TsEnumMemberValue>>,
                    bool,
                ),
            >,
        >,
        k: impl Fn(&mut Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
    ) {
        report_cross_namespace_conflict(
            opts,
            id,
            scopes,
            tbls,
            &name,
            id_loc.dupe(),
            false,
            IncomingBindingKind::Enum,
        );
        bind_local(
            false,
            id,
            scopes,
            tbls,
            name.clone(),
            id_loc.dupe(),
            LocalBinding::EnumBinding {
                id_loc,
                name: name.clone(),
                def,
            },
            k,
        );
    }

    // Function declarations preceded by declared functions are taken to have the
    // type of the declared functions. This is a weird special case aimed to
    // support overloaded signatures.
    pub(super) fn bind_function<'arena, 'ast>(
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        id_loc: LocNode<'arena>,
        fn_loc: LocNode<'arena>,
        name: FlowSmolStr,
        async_: bool,
        generator: bool,
        _effect: ast::function::Effect,
        def: Lazy<'arena, 'ast, FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        k: &dyn Fn(&mut Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
    ) {
        bind(
            false,
            id,
            scopes,
            tbls,
            &name,
            id_loc.dupe(),
            |scopes, tbls, binding_opt| match binding_opt {
                None => {
                    let statics = BTreeMap::new();
                    let namespace_types = BTreeMap::new();
                    let def = LocalBinding::FunBinding {
                        id_loc,
                        name: name.clone(),
                        async_,
                        generator,
                        fn_loc,
                        def,
                        statics,
                        namespace_types,
                    };
                    let node = push_value_def_with_namespace(scopes, tbls, id, def);
                    k(scopes, &name, node.dupe());
                    (BindingNode::LocalBinding(node), true)
                }
                Some(b @ BindingNode::RemoteBinding(_)) => (b, false),
                Some(BindingNode::LocalBinding(node)) => {
                    {
                        let def = node.0.data();
                        if let LocalBinding::DeclareFunBinding { .. } = def.deref() {
                            k(scopes, &name, node.dupe());
                        }
                    }
                    (BindingNode::LocalBinding(node), false)
                }
            },
        );
    }

    // Multiple declared functions with the same name in the same scope define an
    // overloaded function. Note that declared functions are block scoped, so we
    // don't need to walk the scope chain since the scope argument is certainly
    // the host scope.
    pub(super) fn bind_declare_function<'arena, 'ast>(
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        id_loc: LocNode<'arena>,
        fn_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Lazy<'arena, 'ast, FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        k: impl Fn(&mut Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
    ) {
        bind(
            false,
            id,
            scopes,
            tbls,
            &name,
            id_loc.dupe(),
            |scopes, tbls, binding_opt| match binding_opt {
                None => {
                    let defs = vec![(id_loc, fn_loc, def)];
                    let statics = BTreeMap::new();
                    let namespace_types = BTreeMap::new();
                    let def = LocalBinding::DeclareFunBinding {
                        name: name.clone(),
                        defs,
                        statics,
                        namespace_types,
                    };
                    let node = push_value_def_with_namespace(scopes, tbls, id, def);
                    k(scopes, &name, node.dupe());
                    (BindingNode::LocalBinding(node), true)
                }
                Some(b @ BindingNode::RemoteBinding(_)) => (b, false),
                Some(BindingNode::LocalBinding(node)) => {
                    let mut legal = false;
                    if matches!(
                        node.0.data().deref(),
                        LocalBinding::DeclareFunBinding { .. }
                    ) {
                        k(scopes, &name, node.dupe());
                        legal = true;
                    }
                    {
                        let mut def_val = node.0.data_mut();
                        if let LocalBinding::DeclareFunBinding { defs, .. } = def_val.deref_mut() {
                            defs.push((id_loc, fn_loc, def));
                        }
                    }
                    (BindingNode::LocalBinding(node), legal)
                }
            },
        );
    }

    pub(super) fn bind_component<'arena, 'ast>(
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        id_loc: LocNode<'arena>,
        fn_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Option<Lazy<'arena, 'ast, ComponentSig<LocNode<'arena>, Parsed<'arena, 'ast>>>>,
        k: impl Fn(&mut Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
    ) {
        bind_local(
            false,
            id,
            scopes,
            tbls,
            name.clone(),
            id_loc.dupe(),
            LocalBinding::ComponentBinding {
                id_loc,
                fn_loc,
                name: name.clone(),
                def,
            },
            k,
        );
    }

    pub(super) fn bind_var<'arena, 'ast>(
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        kind: ast::VariableKind,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Lazy<'arena, 'ast, Parsed<'arena, 'ast>>,
        k: impl Fn(&mut Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
    ) {
        bind_local(
            false,
            id,
            scopes,
            tbls,
            name.clone(),
            id_loc.dupe(),
            value_binding(kind, id_loc, name.clone(), def),
            k,
        );
    }

    pub(super) fn bind_param<'arena, 'ast>(
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Lazy<'arena, 'ast, Parsed<'arena, 'ast>>,
        tparams: TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
        k: impl Fn(&mut Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
    ) {
        bind_local(
            false,
            id,
            scopes,
            tbls,
            name.clone(),
            id_loc.dupe(),
            LocalBinding::ParamBinding {
                id_loc,
                name: name.clone(),
                def,
                tparams,
                strictness_kind: tbls.strictness_kind,
            },
            k,
        );
    }

    pub(super) fn bind_const<'arena, 'ast>(
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        def: Lazy<'arena, 'ast, Parsed<'arena, 'ast>>,
        k: impl Fn(&mut Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
    ) {
        bind_local(
            false,
            id,
            scopes,
            tbls,
            name.clone(),
            id_loc.dupe(),
            LocalBinding::LetConstBinding {
                is_const: true,
                id_loc,
                name: name.clone(),
                def,
            },
            k,
        );
    }

    pub(super) fn bind_const_ref<'arena, 'ast>(
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        ref_loc: LocNode<'arena>,
        ref_name: FlowSmolStr,
        ref_scope: ScopeId,
        k: impl Fn(&mut Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
    ) {
        let ref_ = Ref {
            ref_loc,
            name: ref_name,
            scope: ref_scope,
            resolved: OnceCell::new(),
        };
        bind_local(
            false,
            id,
            scopes,
            tbls,
            name.clone(),
            id_loc.dupe(),
            LocalBinding::ConstRefBinding {
                id_loc,
                name: name.clone(),
                ref_,
            },
            k,
        );
    }

    pub(super) fn bind_const_fun<'arena, 'ast>(
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        loc: LocNode<'arena>,
        async_: bool,
        generator: bool,
        def: Lazy<'arena, 'ast, FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        k: impl Fn(&mut Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
    ) {
        let statics = BTreeMap::new();
        bind_local(
            false,
            id,
            scopes,
            tbls,
            name.clone(),
            id_loc.dupe(),
            LocalBinding::ConstFunBinding {
                id_loc,
                name: name.clone(),
                loc,
                async_,
                generator,
                def,
                statics,
            },
            k,
        );
    }

    pub(super) fn bind_import<'arena, 'ast>(
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        kind: ast::statement::ImportKind,
        id_loc: LocNode<'arena>,
        local: FlowSmolStr,
        remote: FlowSmolStr,
        mref: ModuleRefNode<'arena>,
    ) {
        // Keep plain imports in the value namespace. Type lookups already fall
        // back to value bindings, and synthesizing a type-side JSImport makes
        // JS full-checks spuriously more precise.
        let type_only = match kind {
            ast::statement::ImportKind::ImportValue => false,
            ast::statement::ImportKind::ImportType | ast::statement::ImportKind::ImportTypeof => {
                true
            }
        };
        bind_remote(
            type_only,
            id,
            scopes,
            tbls,
            local.clone(),
            id_loc.dupe(),
            import_binding(kind, id_loc, local, mref, remote),
        );
    }

    pub(super) fn bind_import_ns<'arena, 'ast>(
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        kind: ast::statement::ImportKind,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        mref: ModuleRefNode<'arena>,
    ) {
        // Namespace imports follow the same rule as named/default imports
        // above.
        let type_only = match kind {
            ast::statement::ImportKind::ImportValue => false,
            ast::statement::ImportKind::ImportType | ast::statement::ImportKind::ImportTypeof => {
                true
            }
        };
        bind_remote(
            type_only,
            id,
            scopes,
            tbls,
            name.clone(),
            id_loc.dupe(),
            import_ns_binding(kind, id_loc, name, mref),
        );
    }

    fn find_binding_node_to_assign<'arena, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        id: ScopeId,
        ref_name: &FlowSmolStr,
    ) -> Option<LocalDefNode<'arena, 'ast>> {
        fn get_local_binding_node<'arena, 'ast>(
            scopes: &Scopes<'arena, 'ast>,
            id: ScopeId,
            ref_name: &FlowSmolStr,
        ) -> Option<LocalDefNode<'arena, 'ast>> {
            match lookup_value(scopes, id, ref_name) {
                None => None,
                Some((BindingNode::RemoteBinding(_), _)) => None,
                Some((BindingNode::LocalBinding(node), found_id)) => {
                    if id == found_id {
                        Some(node)
                    } else {
                        None
                    }
                }
            }
        }

        fn find_node_to_modify<'arena, 'ast>(
            scopes: &Scopes<'arena, 'ast>,
            mut node: LocalDefNode<'arena, 'ast>,
        ) -> Option<LocalDefNode<'arena, 'ast>> {
            loop {
                let binding = node.0.data();
                match binding.deref() {
                    LocalBinding::TypeBinding { .. }
                    | LocalBinding::VarBinding { .. }
                    | LocalBinding::LetConstBinding { .. }
                    | LocalBinding::ParamBinding { .. }
                    | LocalBinding::ClassBinding { .. }
                    | LocalBinding::DeclareClassBinding { .. }
                    | LocalBinding::RecordBinding { .. }
                    | LocalBinding::ComponentBinding { .. }
                    | LocalBinding::EnumBinding { .. }
                    | LocalBinding::NamespaceBinding { .. } => return None,
                    LocalBinding::FunBinding { .. }
                    | LocalBinding::ConstFunBinding { .. }
                    | LocalBinding::DeclareFunBinding { .. } => {
                        return Some(node.dupe());
                    }
                    LocalBinding::ConstRefBinding {
                        id_loc: _,
                        name: _,
                        ref_:
                            Ref {
                                ref_loc: _,
                                name: ref_name,
                                scope,
                                resolved: _,
                            },
                    } => {
                        let new_node = get_local_binding_node(scopes, *scope, ref_name)?;
                        drop(binding);
                        node = new_node;
                    }
                }
            }
        }

        let node = get_local_binding_node(scopes, id, ref_name)?;
        find_node_to_modify(scopes, node)
    }

    pub(super) fn assign_binding<'arena, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        id: ScopeId,
        prop_name: FlowSmolStr,
        prop: (LocNode<'arena>, Parsed<'arena, 'ast>),
        ref_name: &FlowSmolStr,
    ) {
        if let Some(node) = find_binding_node_to_assign(scopes, id, ref_name) {
            let mut def = node.0.data_mut();
            match def.deref_mut() {
                LocalBinding::FunBinding { statics, .. }
                | LocalBinding::ConstFunBinding { statics, .. }
                | LocalBinding::DeclareFunBinding { statics, .. } => {
                    statics.insert(prop_name, prop);
                }
                _ => {}
            }
        }
    }

    pub(super) fn export_ref<'arena, 'ast>(
        opts: &TypeSigOptions,
        scopes: &mut Scopes<'arena, 'ast>,
        id: ScopeId,
        tbls: &mut Tables<'arena, 'ast>,
        kind: ast::statement::ExportKind,
        local: &ast::Identifier<Loc, Loc>,
        exported: Option<&ast::Identifier<Loc, Loc>>,
    ) {
        if let Scope::DeclareNamespace {
            exported_values,
            exported_types,
            ..
        } = scopes.get_mut(id)
        {
            let local_loc = tbls.push_loc(local.loc.dupe());
            let exported = exported.unwrap_or(local);
            let loc = tbls.push_loc(exported.loc.dupe());
            let type_only = kind == ast::statement::ExportKind::ExportType;
            let member = (
                loc.dupe(),
                if type_only {
                    val_ref(true, id, local_loc, local.name.dupe())
                } else {
                    namespace_value_ref(id, local_loc, local.name.dupe())
                },
            );
            let exports = if type_only {
                exported_types
            } else {
                exported_values
            };
            exports.insert(exported.name.dupe(), member);
            return;
        }
        let (ref_loc, name, exported_name) = match exported {
            None => (
                local.loc.dupe(),
                id_name(local).clone(),
                id_name(local).clone(),
            ),
            Some(exported_id) => (
                exported_id.loc.dupe(),
                id_name(local).clone(),
                id_name(exported_id).clone(),
            ),
        };
        let ref_loc = tbls.push_loc(ref_loc);
        let ref_ = Ref {
            ref_loc: ref_loc.dupe(),
            name: name.dupe(),
            scope: id,
            resolved: OnceCell::new(),
        };
        match kind {
            ast::statement::ExportKind::ExportType => {
                modify_exports(scopes, id, |exports| {
                    exports.add_type(exported_name, ExportType::ExportTypeRef(ref_))
                });
            }
            ast::statement::ExportKind::ExportValue if opts.is_ts_file => {
                // In .ts files, check if this is a local type binding
                match lookup_value(scopes, id, &name) {
                    None => {
                        // Not in value scope; check type scope
                        match lookup_type(scopes, id, &name) {
                            Some(_) => {
                                // Local type binding: emit as type export immediately
                                modify_exports(scopes, id, |exports| {
                                    exports.add_type(exported_name, ExportType::ExportTypeRef(ref_))
                                });
                            }
                            None => {
                                // Not found anywhere; keep as value export
                                modify_exports(scopes, id, |exports| {
                                    exports.add(exported_name, Export::ExportRef(ref_))
                                });
                            }
                        }
                    }
                    Some((BindingNode::RemoteBinding(node), _)) => {
                        let data = node.0.data();
                        match &*data {
                            RemoteBinding::ImportBinding { mref, remote, .. } => {
                                let mref = mref.dupe();
                                let remote = remote.dupe();
                                drop(data);
                                // Imported value binding in .ts: defer classification to merge/check
                                modify_exports(scopes, id, |exports| {
                                    exports.add_ts_pending(
                                        exported_name,
                                        TsPendingExport::TsExportRef {
                                            export_loc: ref_loc,
                                            ref_,
                                            import_provenance: Some((mref, remote)),
                                        },
                                    )
                                });
                            }
                            _ => {
                                drop(data);
                                // Other remote bindings: keep as value export
                                modify_exports(scopes, id, |exports| {
                                    exports.add(exported_name, Export::ExportRef(ref_))
                                });
                            }
                        }
                    }
                    Some((BindingNode::LocalBinding(_), _)) => {
                        // Local value binding: keep as value export
                        modify_exports(scopes, id, |exports| {
                            exports.add(exported_name, Export::ExportRef(ref_))
                        });
                    }
                }
            }
            ast::statement::ExportKind::ExportValue => {
                modify_exports(scopes, id, |exports| {
                    exports.add(exported_name, Export::ExportRef(ref_))
                });
            }
        }
    }

    pub(super) fn export_binding<'arena, 'ast>(
        scopes: &mut Scopes<'arena, 'ast>,
        id: ScopeId,
        kind: ast::statement::ExportKind,
        name: FlowSmolStr,
        binding: LocalDefNode<'arena, 'ast>,
    ) {
        match kind {
            ast::statement::ExportKind::ExportType => {
                modify_exports(scopes, id, |exports| {
                    exports.add_type(name, ExportType::ExportTypeBinding(binding))
                });
            }
            ast::statement::ExportKind::ExportValue => {
                modify_exports(scopes, id, |exports| {
                    exports.add(name, Export::ExportBinding(binding))
                });
            }
        }
    }

    pub(super) fn export_default<'arena, 'ast>(
        scopes: &mut Scopes<'arena, 'ast>,
        id: ScopeId,
        default_loc: LocNode<'arena>,
        def: Parsed<'arena, 'ast>,
    ) {
        modify_exports(scopes, id, |exports| {
            exports.add(
                FlowSmolStr::new_inline("default"),
                Export::ExportDefault { default_loc, def },
            )
        });
    }

    pub(super) fn export_default_binding<'arena, 'ast>(
        scopes: &mut Scopes<'arena, 'ast>,
        id: ScopeId,
        default_loc: &LocNode<'arena>,
        name: FlowSmolStr,
        binding: LocalDefNode<'arena, 'ast>,
    ) {
        modify_exports(scopes, id, |exports| {
            exports.add(
                FlowSmolStr::new_inline("default"),
                Export::ExportDefaultBinding {
                    default_loc: default_loc.dupe(),
                    name,
                    binding,
                },
            )
        });
    }

    pub(super) fn export_from<'arena, 'ast>(
        opts: &TypeSigOptions,
        scopes: &mut Scopes<'arena, 'ast>,
        id: ScopeId,
        tbls: &mut Tables<'arena, 'ast>,
        kind: ast::statement::ExportKind,
        mref: Userland,
        local: &ast::Identifier<Loc, Loc>,
        exported: Option<&ast::Identifier<Loc, Loc>>,
    ) {
        let mref = tbls.push_module_ref(mref);
        let (id_loc, exported_name, remote) = match exported {
            None => (
                local.loc.dupe(),
                id_name(local).clone(),
                id_name(local).clone(),
            ),
            Some(exported_id) => (
                exported_id.loc.dupe(),
                id_name(exported_id).clone(),
                id_name(local).clone(),
            ),
        };
        let id_loc = tbls.push_loc(id_loc);
        match kind {
            ast::statement::ExportKind::ExportType => {
                let node = tbls.push_remote_ref(RemoteBinding::ImportTypeBinding {
                    id_loc,
                    name: exported_name.clone(),
                    mref,
                    remote,
                });
                modify_exports(scopes, id, |exports| {
                    exports.add_type(exported_name, ExportType::ExportTypeFrom(node))
                })
            }
            ast::statement::ExportKind::ExportValue if opts.is_ts_file => {
                // In .ts files, we can't classify at parse time whether the remote
                // export is type-only or value. Emit as TsExportFrom so merge/check
                // can classify at resolution time.
                modify_exports(scopes, id, |exports| {
                    exports.add_ts_pending(
                        exported_name,
                        TsPendingExport::TsExportFrom {
                            export_loc: id_loc,
                            mref,
                            remote_name: remote,
                        },
                    )
                })
            }
            ast::statement::ExportKind::ExportValue => {
                let node = tbls.push_remote_ref(RemoteBinding::ImportBinding {
                    id_loc,
                    name: exported_name.clone(),
                    mref,
                    remote,
                });
                modify_exports(scopes, id, |exports| {
                    exports.add(exported_name, Export::ExportFrom(node))
                })
            }
        }
    }

    pub(super) fn export_ns<'arena, 'ast>(
        scopes: &mut Scopes<'arena, 'ast>,
        id: ScopeId,
        tbls: &mut Tables<'arena, 'ast>,
        kind: ast::statement::ExportKind,
        mref: Userland,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
    ) {
        match kind {
            ast::statement::ExportKind::ExportType => {
                panic!("unexpected export type * as");
            }
            ast::statement::ExportKind::ExportValue => {
                let mref = tbls.push_module_ref(mref);
                let node = tbls.push_remote_ref(RemoteBinding::ImportNsBinding {
                    id_loc,
                    name: name.clone(),
                    mref,
                });
                modify_exports(scopes, id, |exports| {
                    exports.add(name, Export::ExportFrom(node))
                });
            }
        }
    }

    pub(super) fn export_star<'arena, 'ast>(
        scopes: &mut Scopes<'arena, 'ast>,
        id: ScopeId,
        tbls: &mut Tables<'arena, 'ast>,
        kind: ast::statement::ExportKind,
        loc: LocNode<'arena>,
        mref: Userland,
    ) {
        let mref = tbls.push_module_ref(mref);
        match kind {
            ast::statement::ExportKind::ExportType => {
                modify_exports(scopes, id, |exports| exports.add_type_star(loc, mref))
            }
            ast::statement::ExportKind::ExportValue => {
                modify_exports(scopes, id, |exports| exports.add_star(loc, mref))
            }
        }
    }

    pub(super) fn cjs_clobber<'arena, 'ast>(
        scopes: &mut Scopes<'arena, 'ast>,
        id: ScopeId,
        t: Parsed<'arena, 'ast>,
    ) {
        modify_exports(scopes, id, |exports| exports.cjs_clobber(t));
    }

    pub(super) fn cjs_set_prop<'arena, 'ast>(
        scopes: &mut Scopes<'arena, 'ast>,
        id: ScopeId,
        name: FlowSmolStr,
        prop: (LocNode<'arena>, Parsed<'arena, 'ast>),
        strictness_kind: TypeStrictnessKind,
    ) {
        match scopes.get(id) {
            Scope::Module { exports, .. } | Scope::DeclareModule { exports, .. } => {
                let mut exports = exports.borrow_mut();
                if let ModuleKind::CJSModule(parsed) = &exports.kind {
                    {
                        match parsed {
                            Parsed::Value(_) => {
                                if let ModuleKind::CJSModule(Parsed::Value(
                                    box ParsedValue::FunExpr(inner),
                                )) = &mut exports.kind
                                {
                                    inner.statics.insert(name, prop);
                                }
                            }
                            Parsed::ValRef {
                                lookup:
                                    ValRefLookup::ValueRefLookup
                                    | ValRefLookup::NamespaceValueRefLookup(_),
                                ref_:
                                    Ref {
                                        ref_loc: _,
                                        name: ref_name,
                                        scope: ref_scope,
                                        resolved: _,
                                    },
                            } => {
                                assign_binding(scopes, *ref_scope, name, prop, ref_name);
                            }
                            _ => {}
                        }
                    }
                    return;
                }
            }
            Scope::DeclareNamespace { .. }
            | Scope::DeclareGlobal { .. }
            | Scope::Global { .. }
            | Scope::Lexical { .. }
            | Scope::ConditionalTypeExtends(_) => {
                return;
            }
        };
        modify_exports(scopes, id, |exports| {
            match &mut exports.kind {
                ModuleKind::UnknownModule | ModuleKind::CJSDeclareModule { .. } => {
                    let mut props = BTreeMap::new();
                    props.insert(name, prop);
                    exports.kind = ModuleKind::CJSModuleProps {
                        props,
                        strictness_kind,
                    };
                }
                ModuleKind::CJSModuleProps { props, .. } => {
                    props.insert(name, prop);
                }
                ModuleKind::CJSModule(_) => {
                    // Already handled above
                }
                ModuleKind::ESModule { .. } => {
                    // indeterminate
                }
            }
        });
    }

    pub(super) fn finalize_declare_module_exports_exn<'arena, 'ast>(
        scopes: &mut Scopes<'arena, 'ast>,
        scope: ScopeId,
        strictness_kind: TypeStrictnessKind,
    ) {
        enum Action<'arena, 'ast> {
            CommonJSModule {
                export_types: Vec<(FlowSmolStr, ExportType<'arena, 'ast>)>,
            },
            UnknownModule {
                export_values: Vec<(FlowSmolStr, LocalDefNode<'arena, 'ast>)>,
                export_types: Vec<(FlowSmolStr, ExportType<'arena, 'ast>)>,
            },
        }

        let action = match scopes.get(scope) {
            Scope::DeclareModule {
                values,
                types,
                exports,
                ..
            } => {
                match &exports.borrow().kind {
                    ModuleKind::ESModule { .. } => {
                        // has explicit exports so do nothing here
                        return;
                    }
                    ModuleKind::CJSModule(_) | ModuleKind::CJSModuleProps { .. } => {
                        // Always auto export all local types for CJS declare modules
                        let mut export_types = Vec::new();
                        for (name, binding) in types {
                            match binding {
                                BindingNode::LocalBinding(node) => {
                                    let binding = node.0.data();
                                    match binding.deref() {
                                        LocalBinding::TypeBinding { .. } => {
                                            export_types.push((
                                                name.clone(),
                                                ExportType::ExportTypeBinding(node.dupe()),
                                            ));
                                        }
                                        _ => {}
                                    }
                                }
                                BindingNode::RemoteBinding(_) => {}
                            }
                        }
                        Action::CommonJSModule { export_types }
                    }
                    ModuleKind::UnknownModule => {
                        let mut export_values = Vec::new();
                        let mut export_types = Vec::new();
                        for (name, binding) in values {
                            match binding {
                                BindingNode::LocalBinding(node) => {
                                    let binding = node.0.data();
                                    match binding.deref() {
                                        LocalBinding::VarBinding { .. }
                                        | LocalBinding::LetConstBinding { .. }
                                        | LocalBinding::ParamBinding { .. }
                                        | LocalBinding::ConstRefBinding { .. }
                                        | LocalBinding::ConstFunBinding { .. }
                                        | LocalBinding::ClassBinding { .. }
                                        | LocalBinding::DeclareClassBinding { .. }
                                        | LocalBinding::RecordBinding { .. }
                                        | LocalBinding::FunBinding { .. }
                                        | LocalBinding::DeclareFunBinding { .. }
                                        | LocalBinding::ComponentBinding { .. }
                                        | LocalBinding::EnumBinding { .. }
                                        | LocalBinding::NamespaceBinding { .. } => {
                                            export_values.push((name.clone(), node.dupe()));
                                        }
                                        LocalBinding::TypeBinding { .. } => {}
                                    }
                                }
                                BindingNode::RemoteBinding(_) => {}
                            }
                        }
                        for (name, binding) in types {
                            match binding {
                                BindingNode::LocalBinding(node) => {
                                    let binding = node.0.data();
                                    match binding.deref() {
                                        LocalBinding::VarBinding { .. }
                                        | LocalBinding::LetConstBinding { .. }
                                        | LocalBinding::ParamBinding { .. }
                                        | LocalBinding::ConstRefBinding { .. }
                                        | LocalBinding::ConstFunBinding { .. }
                                        | LocalBinding::ClassBinding { .. }
                                        | LocalBinding::DeclareClassBinding { .. }
                                        | LocalBinding::RecordBinding { .. }
                                        | LocalBinding::FunBinding { .. }
                                        | LocalBinding::DeclareFunBinding { .. }
                                        | LocalBinding::ComponentBinding { .. }
                                        | LocalBinding::EnumBinding { .. }
                                        | LocalBinding::NamespaceBinding { .. } => {}
                                        LocalBinding::TypeBinding { .. } => {
                                            export_types.push((
                                                name.clone(),
                                                ExportType::ExportTypeBinding(node.dupe()),
                                            ));
                                        }
                                    }
                                }
                                BindingNode::RemoteBinding(_) => {}
                            }
                        }
                        Action::UnknownModule {
                            export_values,
                            export_types,
                        }
                    }
                    ModuleKind::CJSDeclareModule { .. } => panic!(
                        "only call finalize_declare_module_exports_exn once per DeclareModule"
                    ),
                }
            }
            _ => panic!("expected DeclareModule to still be the scope"),
        };

        match action {
            Action::CommonJSModule { export_types } => {
                modify_exports(scopes, scope, |exports| {
                    for (name, t) in export_types {
                        exports.add_type(name, t);
                    }
                });
            }
            Action::UnknownModule {
                export_values,
                export_types,
            } => {
                modify_exports(scopes, scope, |exports| {
                    for (name, node) in export_values {
                        exports.cjs_declare_module_set_prop(name, node, strictness_kind);
                    }
                    for (name, t) in export_types {
                        exports.add_type(name, t);
                    }
                });
            }
        }
    }

    fn namespace_binding_of_values_and_types<'arena, 'ast>(
        scope: ScopeId,
        values: &BTreeMap<FlowSmolStr, BindingNode<'arena, 'ast>>,
        types: &BTreeMap<FlowSmolStr, BindingNode<'arena, 'ast>>,
    ) -> (
        BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
        BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
    ) {
        let new_values = values
            .iter()
            .filter_map(|(name, binding)| match binding {
                BindingNode::LocalBinding(node) => {
                    let binding = node.0.data();
                    match binding.deref() {
                        LocalBinding::VarBinding { id_loc, .. }
                        | LocalBinding::LetConstBinding { id_loc, .. }
                        | LocalBinding::ParamBinding { id_loc, .. }
                        | LocalBinding::ConstRefBinding { id_loc, .. }
                        | LocalBinding::ConstFunBinding { id_loc, .. }
                        | LocalBinding::ClassBinding { id_loc, .. }
                        | LocalBinding::DeclareClassBinding { id_loc, .. }
                        | LocalBinding::RecordBinding { id_loc, .. }
                        | LocalBinding::FunBinding { id_loc, .. }
                        | LocalBinding::ComponentBinding { id_loc, .. }
                        | LocalBinding::EnumBinding { id_loc, .. }
                        | LocalBinding::NamespaceBinding { id_loc, .. } => Some((
                            name.clone(),
                            (
                                id_loc.dupe(),
                                namespace_value_ref(scope, id_loc.dupe(), name.clone()),
                            ),
                        )),
                        LocalBinding::DeclareFunBinding { defs, .. } => {
                            let (id_loc, _, _) = defs.first().unwrap();
                            Some((
                                name.clone(),
                                (
                                    id_loc.dupe(),
                                    namespace_value_ref(scope, id_loc.dupe(), name.clone()),
                                ),
                            ))
                        }
                        LocalBinding::TypeBinding { .. } => None,
                    }
                }
                BindingNode::RemoteBinding(_) => None,
            })
            .collect();

        let new_types = types
            .iter()
            .filter_map(|(name, binding)| match binding {
                BindingNode::LocalBinding(node) => {
                    let binding = node.0.data();
                    match binding.deref() {
                        LocalBinding::TypeBinding { id_loc, .. }
                        | LocalBinding::NamespaceBinding { id_loc, .. } => Some((
                            name.clone(),
                            (
                                id_loc.dupe(),
                                val_ref(true, scope, id_loc.dupe(), name.clone()),
                            ),
                        )),
                        LocalBinding::VarBinding { .. }
                        | LocalBinding::LetConstBinding { .. }
                        | LocalBinding::ParamBinding { .. }
                        | LocalBinding::ConstRefBinding { .. }
                        | LocalBinding::ConstFunBinding { .. }
                        | LocalBinding::ClassBinding { .. }
                        | LocalBinding::DeclareClassBinding { .. }
                        | LocalBinding::RecordBinding { .. }
                        | LocalBinding::FunBinding { .. }
                        | LocalBinding::ComponentBinding { .. }
                        | LocalBinding::EnumBinding { .. }
                        | LocalBinding::DeclareFunBinding { .. } => None,
                    }
                }
                BindingNode::RemoteBinding(_) => None,
            })
            .collect();

        (new_values, new_types)
    }

    fn local_binding_node_of_namespace_entry<'arena, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        type_only: bool,
        name: &FlowSmolStr,
        entry: &(LocNode<'arena>, Parsed<'arena, 'ast>),
    ) -> Option<LocalDefNode<'arena, 'ast>> {
        match &entry.1 {
            Parsed::ValRef { lookup, ref_ }
                if &ref_.name == name
                    && matches!(
                        (type_only, lookup),
                        (true, ValRefLookup::TypeRefLookup)
                            | (
                                false,
                                ValRefLookup::ValueRefLookup
                                    | ValRefLookup::NamespaceValueRefLookup(_)
                                    | ValRefLookup::TypeofRefLookup,
                            )
                    ) =>
            {
                let binding = if type_only {
                    lookup_local_type(scopes, ref_.scope, name)
                } else {
                    lookup_local_value(scopes, ref_.scope, name)
                };
                match binding {
                    Some(BindingNode::LocalBinding(node)) => Some(node),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn local_value_binding_node_of_namespace_entry<'arena, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        name: &FlowSmolStr,
        entry: &(LocNode<'arena>, Parsed<'arena, 'ast>),
    ) -> Option<LocalDefNode<'arena, 'ast>> {
        local_binding_node_of_namespace_entry(scopes, false, name, entry)
    }

    fn merge_interface_type_bindings<'arena: 'ast, 'ast>(
        name: &FlowSmolStr,
        existing_node: &LocalDefNode<'arena, 'ast>,
        new_node: &LocalDefNode<'arena, 'ast>,
        existing_entry: &(LocNode<'arena>, Parsed<'arena, 'ast>),
        entry: &(LocNode<'arena>, Parsed<'arena, 'ast>),
    ) -> bool {
        let existing_id_loc = {
            let data = existing_node.0.data();
            match &*data {
                LocalBinding::TypeBinding { id_loc, .. } => id_loc.dupe(),
                _ => return false,
            }
        };
        let new_id_loc = {
            let data = new_node.0.data();
            match &*data {
                LocalBinding::TypeBinding { id_loc, .. } => id_loc.dupe(),
                _ => return false,
            }
        };

        let old_def = {
            let mut data = existing_node.0.data_mut();
            let old = std::mem::replace(
                &mut *data,
                LocalBinding::TypeBinding {
                    id_loc: existing_id_loc.dupe(),
                    def: Lazy::new(Box::new(|_, _, _| {
                        unreachable!("placeholder during namespace interface merge")
                    })),
                },
            );
            match old {
                LocalBinding::TypeBinding { def, .. } => def,
                _ => unreachable!(),
            }
        };
        let new_def = {
            let mut data = new_node.0.data_mut();
            let old = std::mem::replace(
                &mut *data,
                LocalBinding::TypeBinding {
                    id_loc: new_id_loc.dupe(),
                    def: Lazy::new(Box::new(|_, _, _| {
                        unreachable!("placeholder during namespace interface merge")
                    })),
                },
            );
            match old {
                LocalBinding::TypeBinding { def, .. } => def,
                _ => unreachable!(),
            }
        };
        let new_def = std::rc::Rc::new(new_def);
        {
            let mut data = new_node.0.data_mut();
            let new_def = std::rc::Rc::clone(&new_def);
            *data = LocalBinding::TypeBinding {
                id_loc: new_id_loc,
                def: Lazy::new(Box::new(move |opts, scopes, tbls| {
                    new_def.get_forced(opts, scopes, tbls).clone()
                })),
            };
        }

        let merge_name = name.dupe();
        let invalid_binding_loc = entry.0.dupe();
        let existing_binding_loc = existing_entry.0.dupe();
        let merged_def = Lazy::new(Box::new(
            move |opts, scopes, tbls: &mut Tables<'arena, 'ast>| {
                let old_val = old_def.get_forced(opts, scopes, tbls);
                let new_val = new_def.get_forced(opts, scopes, tbls);
                match (old_val, new_val) {
                    (Def::Interface(old_iface), Def::Interface(new_iface)) => {
                        let mut merged_tparams = old_iface.tparams.clone();
                        if type_param_renaming::merge_tparams(
                            &mut merged_tparams,
                            &new_iface.tparams,
                        ) {
                            let merged_sig = merge_interface_sigs(
                                old_iface.id_loc.dupe(),
                                new_iface.id_loc.dupe(),
                                tbls,
                                &old_iface.tparams,
                                &new_iface.tparams,
                                &old_iface.def,
                                &new_iface.def,
                            );
                            Def::Interface(Box::new(DefInterface {
                                id_loc: old_iface.id_loc.dupe(),
                                name: old_iface.name.dupe(),
                                tparams: merged_tparams,
                                def: merged_sig,
                            }))
                        } else {
                            tbls.additional_errors.push(
                                signature_error::BindingValidation::InterfaceMergeTparamMismatch {
                                    name: merge_name.dupe(),
                                    current_binding_loc: new_iface.id_loc.dupe(),
                                    existing_binding_loc: old_iface.id_loc.dupe(),
                                },
                            );
                            old_val.clone()
                        }
                    }
                    _ => {
                        tbls.additional_errors.push(
                            signature_error::BindingValidation::NamespacedNameAlreadyBound {
                                name: merge_name,
                                invalid_binding_loc,
                                existing_binding_loc,
                            },
                        );
                        old_val.clone()
                    }
                }
            },
        ));

        {
            let mut data = existing_node.0.data_mut();
            *data = LocalBinding::TypeBinding {
                id_loc: existing_id_loc,
                def: merged_def,
            };
        }
        true
    }

    fn merge_interface_into_declare_class_binding<'arena: 'ast, 'ast>(
        name: &FlowSmolStr,
        class_node: &LocalDefNode<'arena, 'ast>,
        iface_node: &LocalDefNode<'arena, 'ast>,
        existing_entry: &(LocNode<'arena>, Parsed<'arena, 'ast>),
        entry: &(LocNode<'arena>, Parsed<'arena, 'ast>),
    ) -> bool {
        let (dc_id_loc, dc_name, dc_namespace_values, dc_namespace_types) = {
            let data = class_node.0.data();
            match &*data {
                LocalBinding::DeclareClassBinding {
                    id_loc,
                    name,
                    namespace_values,
                    namespace_types,
                    ..
                } => (
                    id_loc.dupe(),
                    name.dupe(),
                    namespace_values.clone(),
                    namespace_types.clone(),
                ),
                _ => return false,
            }
        };
        let iface_id_loc = {
            let data = iface_node.0.data();
            match &*data {
                LocalBinding::TypeBinding { id_loc, .. } => id_loc.dupe(),
                _ => return false,
            }
        };

        let dc_def = {
            let mut data = class_node.0.data_mut();
            let old = std::mem::replace(
                &mut *data,
                LocalBinding::DeclareClassBinding {
                    id_loc: dc_id_loc.dupe(),
                    name: dc_name.dupe(),
                    def: Lazy::new(Box::new(|_, _, _| {
                        unreachable!("placeholder during namespace declare-class merge")
                    })),
                    namespace_values: dc_namespace_values.clone(),
                    namespace_types: dc_namespace_types.clone(),
                },
            );
            match old {
                LocalBinding::DeclareClassBinding { def, .. } => def,
                _ => unreachable!(),
            }
        };
        let iface_def = {
            let mut data = iface_node.0.data_mut();
            let old = std::mem::replace(
                &mut *data,
                LocalBinding::TypeBinding {
                    id_loc: iface_id_loc.dupe(),
                    def: Lazy::new(Box::new(|_, _, _| {
                        unreachable!("placeholder during namespace interface merge")
                    })),
                },
            );
            match old {
                LocalBinding::TypeBinding { def, .. } => def,
                _ => unreachable!(),
            }
        };
        let iface_def = std::rc::Rc::new(iface_def);
        {
            let mut data = iface_node.0.data_mut();
            let iface_def = std::rc::Rc::clone(&iface_def);
            *data = LocalBinding::TypeBinding {
                id_loc: iface_id_loc,
                def: Lazy::new(Box::new(move |opts, scopes, tbls| {
                    iface_def.get_forced(opts, scopes, tbls).clone()
                })),
            };
        }

        let merge_dc_id_loc = dc_id_loc.dupe();
        let merge_name = name.dupe();
        let invalid_binding_loc = entry.0.dupe();
        let existing_binding_loc = existing_entry.0.dupe();
        let merged_def = Lazy::new(Box::new(
            move |opts, scopes, tbls: &mut Tables<'arena, 'ast>| {
                let mut forced_dc = dc_def.get_forced(opts, scopes, tbls).clone();
                match iface_def.get_forced(opts, scopes, tbls) {
                    Def::Interface(iface) => {
                        if type_param_renaming::merge_tparams(
                            &mut forced_dc.tparams,
                            &iface.tparams,
                        ) {
                            let iface_id_loc_local = iface.id_loc.dupe();
                            let iface_def_local = iface.def.clone();
                            merge_interface_into_declare_class_sig(
                                merge_dc_id_loc.dupe(),
                                iface_id_loc_local,
                                tbls,
                                &forced_dc,
                                &iface.tparams,
                                &iface_def_local,
                            )
                        } else {
                            tbls.additional_errors.push(
                                signature_error::BindingValidation::InterfaceMergeTparamMismatch {
                                    name: merge_name,
                                    current_binding_loc: iface.id_loc.dupe(),
                                    existing_binding_loc: merge_dc_id_loc,
                                },
                            );
                            forced_dc
                        }
                    }
                    _ => {
                        tbls.additional_errors.push(
                            signature_error::BindingValidation::NamespacedNameAlreadyBound {
                                name: merge_name,
                                invalid_binding_loc,
                                existing_binding_loc,
                            },
                        );
                        forced_dc
                    }
                }
            },
        ));

        {
            let mut data = class_node.0.data_mut();
            *data = LocalBinding::DeclareClassBinding {
                id_loc: dc_id_loc,
                name: dc_name,
                def: merged_def,
                namespace_values: dc_namespace_values,
                namespace_types: dc_namespace_types,
            };
        }
        true
    }

    fn clone_fun_sig_lazy<'arena: 'ast, 'ast>(
        def: std::rc::Rc<Lazy<'arena, 'ast, FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>>>,
    ) -> Lazy<'arena, 'ast, FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>> {
        Lazy::new(Box::new(move |opts, scopes, tbls| {
            def.get_forced(opts, scopes, tbls).clone()
        }))
    }

    fn restore_declare_fun_defs<'arena: 'ast, 'ast>(
        defs: Vec<(
            LocNode<'arena>,
            LocNode<'arena>,
            Lazy<'arena, 'ast, FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        )>,
    ) -> (
        Vec<(
            LocNode<'arena>,
            LocNode<'arena>,
            Lazy<'arena, 'ast, FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        )>,
        Vec<(
            LocNode<'arena>,
            LocNode<'arena>,
            Lazy<'arena, 'ast, FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        )>,
    ) {
        let mut defs_for_existing = Vec::new();
        let mut defs_for_restore = Vec::new();
        for (id_loc, fn_loc, def) in defs {
            let def = std::rc::Rc::new(def);
            defs_for_existing.push((
                id_loc.dupe(),
                fn_loc.dupe(),
                clone_fun_sig_lazy(std::rc::Rc::clone(&def)),
            ));
            defs_for_restore.push((id_loc, fn_loc, clone_fun_sig_lazy(def)));
        }
        (defs_for_existing, defs_for_restore)
    }

    fn merge_namespace_value_value_entry<'arena: 'ast, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        name: &FlowSmolStr,
        existing_entry: &(LocNode<'arena>, Parsed<'arena, 'ast>),
        entry: &(LocNode<'arena>, Parsed<'arena, 'ast>),
    ) -> bool {
        let existing_node =
            local_value_binding_node_of_namespace_entry(scopes, name, existing_entry);
        let new_node = local_value_binding_node_of_namespace_entry(scopes, name, entry);
        let (Some(existing_node), Some(new_node)) = (existing_node, new_node) else {
            return false;
        };

        let new_namespace = {
            let data = new_node.0.data();
            match &*data {
                LocalBinding::NamespaceBinding { values, types, .. } => {
                    Some((values.clone(), types.clone()))
                }
                _ => None,
            }
        };
        if let Some((values, types)) = new_namespace {
            return merge_namespace_into_local_binding_node(
                scopes,
                tbls,
                &existing_node,
                values,
                types,
            );
        }

        let new_declare_fun = {
            let mut data = new_node.0.data_mut();
            match &mut *data {
                LocalBinding::DeclareFunBinding {
                    name,
                    defs,
                    statics,
                    namespace_types,
                } => {
                    let name = name.dupe();
                    let defs = std::mem::take(defs);
                    let statics = statics.clone();
                    let namespace_types = namespace_types.clone();
                    let (defs_for_existing, defs_for_restore) = restore_declare_fun_defs(defs);
                    *data = LocalBinding::DeclareFunBinding {
                        name: name.dupe(),
                        defs: defs_for_restore,
                        statics: statics.clone(),
                        namespace_types: namespace_types.clone(),
                    };
                    Some((defs_for_existing, statics, namespace_types))
                }
                _ => None,
            }
        };
        let Some((new_defs, new_statics, new_namespace_types)) = new_declare_fun else {
            return false;
        };
        let mut data = existing_node.0.data_mut();
        match &mut *data {
            LocalBinding::DeclareFunBinding {
                defs,
                statics,
                namespace_types,
                ..
            } => {
                merge_namespace_entries(
                    scopes,
                    tbls,
                    statics,
                    namespace_types,
                    new_statics,
                    new_namespace_types,
                );
                defs.extend(new_defs);
                true
            }
            _ => false,
        }
    }

    fn merge_namespace_type_type_entry<'arena: 'ast, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        name: &FlowSmolStr,
        existing_entry: &(LocNode<'arena>, Parsed<'arena, 'ast>),
        entry: &(LocNode<'arena>, Parsed<'arena, 'ast>),
    ) -> bool {
        let existing_node =
            local_binding_node_of_namespace_entry(scopes, true, name, existing_entry);
        let new_node = local_binding_node_of_namespace_entry(scopes, true, name, entry);
        let (Some(existing_node), Some(new_node)) = (existing_node, new_node) else {
            return false;
        };
        let new_namespace = {
            let data = new_node.0.data();
            match &*data {
                LocalBinding::NamespaceBinding { values, types, .. } => {
                    Some((values.clone(), types.clone()))
                }
                _ => None,
            }
        };
        if let Some((values, types)) = new_namespace {
            return merge_namespace_into_local_binding_node(
                scopes,
                tbls,
                &existing_node,
                values,
                types,
            );
        }
        merge_interface_type_bindings(name, &existing_node, &new_node, existing_entry, entry)
    }

    fn merge_namespace_value_type_entry<'arena: 'ast, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        name: &FlowSmolStr,
        existing_entry: &(LocNode<'arena>, Parsed<'arena, 'ast>),
        entry: &(LocNode<'arena>, Parsed<'arena, 'ast>),
    ) -> Option<(LocNode<'arena>, Parsed<'arena, 'ast>)> {
        let existing_node =
            local_binding_node_of_namespace_entry(scopes, true, name, existing_entry);
        let new_node = local_value_binding_node_of_namespace_entry(scopes, name, entry);
        match (existing_node, new_node) {
            (Some(existing_node), Some(new_node)) => {
                let namespace_merge = {
                    let existing_data = existing_node.0.data();
                    let data = new_node.0.data();
                    match (&*existing_data, &*data) {
                        (
                            LocalBinding::NamespaceBinding {
                                values: existing_values,
                                types: existing_types,
                                ..
                            },
                            LocalBinding::NamespaceBinding {
                                id_loc,
                                name,
                                values,
                                types,
                            },
                        ) => Some((
                            existing_values.clone(),
                            existing_types.clone(),
                            id_loc.dupe(),
                            name.dupe(),
                            values.clone(),
                            types.clone(),
                        )),
                        _ => None,
                    }
                };
                if let Some((
                    mut merged_values,
                    mut merged_types,
                    id_loc,
                    namespace_name,
                    values,
                    types,
                )) = namespace_merge
                {
                    merge_namespace_entries(
                        scopes,
                        tbls,
                        &mut merged_values,
                        &mut merged_types,
                        values,
                        types,
                    );
                    let mut data = new_node.0.data_mut();
                    *data = LocalBinding::NamespaceBinding {
                        id_loc,
                        name: namespace_name,
                        values: merged_values,
                        types: merged_types,
                    };
                    Some(entry.clone())
                } else if merge_interface_into_declare_class_binding(
                    name,
                    &new_node,
                    &existing_node,
                    existing_entry,
                    entry,
                ) {
                    Some(entry.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn merge_namespace_type_value_entry<'arena: 'ast, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        name: &FlowSmolStr,
        existing_entry: &(LocNode<'arena>, Parsed<'arena, 'ast>),
        entry: &(LocNode<'arena>, Parsed<'arena, 'ast>),
    ) -> bool {
        let existing_node =
            local_value_binding_node_of_namespace_entry(scopes, name, existing_entry);
        let new_node = local_binding_node_of_namespace_entry(scopes, true, name, entry);
        match (existing_node, new_node) {
            (Some(existing_node), Some(new_node)) => {
                let new_namespace = {
                    let data = new_node.0.data();
                    match &*data {
                        LocalBinding::NamespaceBinding { values, types, .. } => {
                            Some((values.clone(), types.clone()))
                        }
                        _ => None,
                    }
                };
                if let Some((values, types)) = new_namespace {
                    merge_namespace_into_local_binding_node(
                        scopes,
                        tbls,
                        &existing_node,
                        values,
                        types,
                    )
                } else {
                    merge_interface_into_declare_class_binding(
                        name,
                        &existing_node,
                        &new_node,
                        existing_entry,
                        entry,
                    )
                }
            }
            _ => false,
        }
    }

    fn merge_namespace_entries<'arena: 'ast, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        existing_values: &mut BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
        existing_types: &mut BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
        ns_values: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
        ns_types: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
    ) {
        for (n, entry) in ns_values {
            if let Some(existing_entry) = existing_values.get(&n).cloned() {
                let existing_binding_loc = existing_entry.0.dupe();
                if !merge_namespace_value_value_entry(scopes, tbls, &n, &existing_entry, &entry) {
                    tbls.additional_errors
                        .push(BindingValidation::NamespacedNameAlreadyBound {
                            name: n,
                            invalid_binding_loc: entry.0,
                            existing_binding_loc,
                        });
                }
            } else if let Some(existing_entry) = existing_types.get(&n).cloned() {
                if let Some(merged_entry) =
                    merge_namespace_value_type_entry(scopes, tbls, &n, &existing_entry, &entry)
                {
                    existing_types.remove(&n);
                    existing_values.insert(n, merged_entry);
                } else {
                    existing_values.insert(n, entry);
                }
            } else {
                existing_values.insert(n, entry);
            }
        }

        for (n, entry) in ns_types {
            if let Some(existing_entry) = existing_types.get(&n).cloned() {
                let existing_binding_loc = existing_entry.0.dupe();
                if !merge_namespace_type_type_entry(scopes, tbls, &n, &existing_entry, &entry) {
                    tbls.additional_errors
                        .push(BindingValidation::NamespacedNameAlreadyBound {
                            name: n,
                            invalid_binding_loc: entry.0,
                            existing_binding_loc,
                        });
                }
            } else if let Some(existing_entry) = existing_values.get(&n).cloned() {
                if !merge_namespace_type_value_entry(scopes, tbls, &n, &existing_entry, &entry) {
                    existing_types.insert(n, entry);
                }
            } else {
                existing_types.insert(n, entry);
            }
        }
    }

    fn merge_namespace_into_local_binding_node<'arena: 'ast, 'ast>(
        scopes: &Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        node: &LocalDefNode<'arena, 'ast>,
        ns_values: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
        ns_types: BTreeMap<FlowSmolStr, (LocNode<'arena>, Parsed<'arena, 'ast>)>,
    ) -> bool {
        let mut binding = node.0.data_mut();
        match binding.deref_mut() {
            LocalBinding::NamespaceBinding {
                values: existing_values,
                types: existing_types,
                ..
            }
            | LocalBinding::FunBinding {
                statics: existing_values,
                namespace_types: existing_types,
                ..
            }
            | LocalBinding::DeclareFunBinding {
                statics: existing_values,
                namespace_types: existing_types,
                ..
            }
            | LocalBinding::ClassBinding {
                namespace_values: existing_values,
                namespace_types: existing_types,
                ..
            }
            | LocalBinding::DeclareClassBinding {
                namespace_values: existing_values,
                namespace_types: existing_types,
                ..
            } => {
                merge_namespace_entries(
                    scopes,
                    tbls,
                    existing_values,
                    existing_types,
                    ns_values,
                    ns_types,
                );
                true
            }
            _ => false,
        }
    }

    pub(super) fn resolve_declare_namespace_exports(
        opts: &TypeSigOptions,
        scopes: &mut Scopes<'_, '_>,
        id: ScopeId,
    ) {
        if !opts.is_ts_file {
            return;
        }
        let exports = match scopes.get_mut(id) {
            Scope::DeclareNamespace {
                exported_values, ..
            } => std::mem::take(exported_values),
            _ => return,
        };
        for (name, (loc, mut value)) in exports {
            let type_only = if let Parsed::ValRef { lookup, ref_ } = &mut value {
                if lookup_value(scopes, id, &ref_.name).is_none()
                    && lookup_type(scopes, id, &ref_.name).is_some()
                {
                    *lookup = ValRefLookup::TypeRefLookup;
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if let Scope::DeclareNamespace {
                exported_values,
                exported_types,
                ..
            } = scopes.get_mut(id)
            {
                let members = if type_only {
                    exported_types
                } else {
                    exported_values
                };
                members.insert(name, (loc, value));
            }
        }
    }

    pub(super) fn finalize_declare_namespace_exn<'arena: 'ast, 'ast>(
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        is_type_only: bool,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
    ) {
        let (ns_values, ns_types, parent) = match scopes.get_mut(id) {
            Scope::DeclareNamespace {
                values,
                types,
                exported_values,
                exported_types,
                parent,
                ..
            } => {
                let (mut ns_values, mut ns_types) =
                    namespace_binding_of_values_and_types(id, values, types);
                ns_values.extend(exported_values.clone());
                ns_types.extend(exported_types.clone());
                (ns_values, ns_types, *parent)
            }
            _ => panic!("The scope must be DeclareNamespace"),
        };
        let parent = binding_scope(scopes, parent);
        let (values, types) = match scopes.get_mut(parent) {
            Scope::Global { values, types, .. }
            | Scope::DeclareModule { values, types, .. }
            | Scope::DeclareNamespace { values, types, .. }
            | Scope::Module { values, types, .. }
            | Scope::Lexical { values, types, .. } => (values, types),
            Scope::DeclareGlobal { .. } | Scope::ConditionalTypeExtends(_) => {
                panic!("The host scope cannot be ConditionalTypeExtends")
            }
        };

        match (
            is_type_only,
            values.get(&name).map(|n| n.dupe()),
            types.get(&name).map(|n| n.dupe()),
        ) {
            (
                _,
                Some(BindingNode::LocalBinding(value_node)),
                Some(BindingNode::LocalBinding(type_node)),
            ) => {
                if value_node.0.ptr_eq(&type_node.0) {
                    merge_namespace_into_local_binding_node(
                        scopes,
                        tbls,
                        &value_node,
                        ns_values,
                        ns_types,
                    );
                } else {
                    merge_namespace_into_local_binding_node(
                        scopes,
                        tbls,
                        &value_node,
                        ns_values.clone(),
                        ns_types.clone(),
                    );
                    merge_namespace_into_local_binding_node(
                        scopes, tbls, &type_node, ns_values, ns_types,
                    );
                }
            }
            (_, Some(_), Some(_)) => {}
            (_, None, None) => {
                let def = LocalBinding::NamespaceBinding {
                    id_loc,
                    name: name.clone(),
                    values: ns_values.clone(),
                    types: ns_types.clone(),
                };
                let node = tbls.push_local_def(def);
                if is_type_only {
                    types.insert(name.clone(), BindingNode::LocalBinding(node));
                } else {
                    values.insert(name.clone(), BindingNode::LocalBinding(node));
                }
            }
            (false, Some(BindingNode::LocalBinding(node)), None)
            | (true, None, Some(BindingNode::LocalBinding(node))) => {
                if !merge_namespace_into_local_binding_node(
                    scopes,
                    tbls,
                    &node,
                    ns_values.clone(),
                    ns_types.clone(),
                ) {
                    // Has an existing non-namespace binding, do nothing.
                }
            }
            (true, Some(BindingNode::LocalBinding(node)), None) => {
                if !merge_namespace_into_local_binding_node(
                    scopes,
                    tbls,
                    &node,
                    ns_values.clone(),
                    ns_types.clone(),
                ) {
                    // Has an existing non-namespace binding, do nothing.
                }
            }
            (false, Some(BindingNode::RemoteBinding(_)), None)
            | (true, None, Some(BindingNode::RemoteBinding(_)))
            | (true, Some(BindingNode::RemoteBinding(_)), None) => {}
            (false, None, Some(BindingNode::LocalBinding(node)))
                if matches!(node.0.data().deref(), LocalBinding::NamespaceBinding { .. }) =>
            {
                types.remove(&name);
                values.insert(name, BindingNode::LocalBinding(node.dupe()));
                merge_namespace_into_local_binding_node(scopes, tbls, &node, ns_values, ns_types);
            }
            (false, None, Some(_)) => {}
        }
    }

    pub(super) fn finalize_declare_namespace_exported_exn<'arena: 'ast, 'ast>(
        id: ScopeId,
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        is_type_only: bool,
        id_loc: LocNode<'arena>,
        name: FlowSmolStr,
        k: impl Fn(&mut Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
    ) {
        match scopes.get_mut(id) {
            Scope::DeclareNamespace {
                values,
                types,
                exported_values,
                exported_types,
                parent,
                ..
            } => {
                let (mut ns_values, mut ns_types) =
                    namespace_binding_of_values_and_types(id, values, types);
                ns_values.extend(exported_values.clone());
                ns_types.extend(exported_types.clone());
                let parent = *parent;
                let parent = binding_scope(scopes, parent);
                let (parent_values, parent_types) = match scopes.get_mut(parent) {
                    Scope::Global { values, types, .. }
                    | Scope::DeclareModule { values, types, .. }
                    | Scope::DeclareNamespace { values, types, .. }
                    | Scope::Module { values, types, .. }
                    | Scope::Lexical { values, types, .. } => (values, types),
                    Scope::DeclareGlobal { .. } | Scope::ConditionalTypeExtends(_) => {
                        panic!("The host scope cannot be ConditionalTypeExtends")
                    }
                };
                let merged_existing = match (
                    is_type_only,
                    parent_values.get(&name).map(|n| n.dupe()),
                    parent_types.get(&name).map(|n| n.dupe()),
                ) {
                    (
                        _,
                        Some(BindingNode::LocalBinding(value_node)),
                        Some(BindingNode::LocalBinding(type_node)),
                    ) => {
                        let merged = if value_node.0.ptr_eq(&type_node.0) {
                            merge_namespace_into_local_binding_node(
                                scopes,
                                tbls,
                                &value_node,
                                ns_values.clone(),
                                ns_types.clone(),
                            )
                        } else {
                            let merged_value = merge_namespace_into_local_binding_node(
                                scopes,
                                tbls,
                                &value_node,
                                ns_values.clone(),
                                ns_types.clone(),
                            );
                            let merged_type = merge_namespace_into_local_binding_node(
                                scopes,
                                tbls,
                                &type_node,
                                ns_values.clone(),
                                ns_types.clone(),
                            );
                            merged_value || merged_type
                        };
                        if merged {
                            k(scopes, &name, value_node);
                        }
                        merged
                    }
                    (_, Some(_), Some(_)) => false,
                    (false, Some(BindingNode::LocalBinding(node)), None)
                    | (true, Some(BindingNode::LocalBinding(node)), None)
                    | (true, None, Some(BindingNode::LocalBinding(node)))
                        if merge_namespace_into_local_binding_node(
                            scopes,
                            tbls,
                            &node,
                            ns_values.clone(),
                            ns_types.clone(),
                        ) =>
                    {
                        k(scopes, &name, node);
                        true
                    }
                    _ => false,
                };
                if !merged_existing {
                    bind_local(
                        is_type_only,
                        parent,
                        scopes,
                        tbls,
                        name.clone(),
                        id_loc.dupe(),
                        LocalBinding::NamespaceBinding {
                            id_loc,
                            name,
                            values: ns_values,
                            types: ns_types,
                        },
                        k,
                    );
                }
            }
            Scope::Global { .. } => {
                // Global namespaces cannot be exported
            }
            _ => panic!("The scope must be DeclareNamespace"),
        }
    }

    pub(crate) fn bind_global_this<'arena, 'ast>(
        scopes: &mut Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        id: ScopeId,
        global_this_loc: Loc,
    ) {
        let Scope::Global { values, types, .. } = scopes.get_mut(id) else {
            panic!("finalize_globalThis must be called after parsing all lib files on global scope")
        };
        let name = FlowSmolStr::new_inline("globalThis");
        let (mut ns_values, ns_types) = namespace_binding_of_values_and_types(id, values, types);
        let loc = tbls.push_loc(global_this_loc);
        // Patch in globalThis so that we can do globalThis.globalThis
        ns_values.insert(
            name.clone(),
            (loc.dupe(), val_ref(false, id, loc.dupe(), name.clone())),
        );
        bind_local(
            false,
            id,
            scopes,
            tbls,
            name.clone(),
            loc.dupe(),
            LocalBinding::NamespaceBinding {
                id_loc: loc,
                name,
                values: ns_values,
                types: ns_types,
            },
            |_, _, _| {},
        );
    }
}

mod obj_annot_acc {
    use std::collections::BTreeMap;

    use flow_common::polarity::Polarity;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use vec1::Vec1;

    use super::*;

    pub(super) struct ObjAnnotAcc<'arena, 'ast> {
        pub(super) dict: Option<ObjAnnotDict<Parsed<'arena, 'ast>>>,
        pub(super) props:
            BTreeMap<FlowSmolStr, ObjAnnotProp<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        pub(super) computed_props: Vec<(
            Parsed<'arena, 'ast>,
            ObjAnnotProp<LocNode<'arena>, Parsed<'arena, 'ast>>,
        )>,
        pub(super) tail: Vec<ObjSpreadAnnotElem<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        pub(super) proto: Option<(LocNode<'arena>, Parsed<'arena, 'ast>)>,
        pub(super) calls: Vec<Parsed<'arena, 'ast>>,
    }

    impl<'arena, 'ast> ObjAnnotAcc<'arena, 'ast> {
        pub(super) fn empty() -> Self {
            ObjAnnotAcc {
                dict: None,
                props: BTreeMap::new(),
                computed_props: Vec::new(),
                tail: Vec::new(),
                proto: None,
                calls: Vec::new(),
            }
        }

        fn empty_slice() -> ObjSpreadAnnotElem<LocNode<'arena>, Parsed<'arena, 'ast>> {
            ObjSpreadAnnotElem::ObjSpreadAnnotSlice(Box::new(ObjSpreadAnnotSliceData {
                dict: None,
                props: BTreeMap::new(),
                computed_props: Vec::new(),
            }))
        }

        fn head_slice(
            &mut self,
        ) -> Option<ObjSpreadAnnotElem<LocNode<'arena>, Parsed<'arena, 'ast>>> {
            if self.dict.is_none() && self.props.is_empty() && self.computed_props.is_empty() {
                None
            } else {
                Some(ObjSpreadAnnotElem::ObjSpreadAnnotSlice(Box::new(
                    ObjSpreadAnnotSliceData {
                        dict: self.dict.take(),
                        props: std::mem::take(&mut self.props),
                        computed_props: std::mem::take(&mut self.computed_props),
                    },
                )))
            }
        }

        pub(super) fn add_field(
            &mut self,
            name: FlowSmolStr,
            id_loc: LocNode<'arena>,
            polarity: Polarity,
            t: Parsed<'arena, 'ast>,
        ) {
            self.props.insert(
                name,
                ObjAnnotProp::ObjAnnotField(Box::new((id_loc, t, polarity))),
            );
        }

        pub(super) fn add_method(
            &mut self,
            name: FlowSmolStr,
            id_loc: LocNode<'arena>,
            fn_loc: LocNode<'arena>,
            def: FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) {
            self.props.insert(
                name,
                ObjAnnotProp::ObjAnnotMethod(Box::new(ObjAnnotMethodData {
                    id_loc,
                    fn_loc,
                    def,
                })),
            );
        }

        pub(super) fn add_accessor(
            &mut self,
            name: FlowSmolStr,
            x: Accessor<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) {
            if let Some(ObjAnnotProp::ObjAnnotAccess(x_prime)) = self.props.remove(&name) {
                self.props.insert(
                    name,
                    ObjAnnotProp::ObjAnnotAccess(Box::new(super::merge_accessors(*x_prime, x))),
                );
                return;
            }
            self.props
                .insert(name, ObjAnnotProp::ObjAnnotAccess(Box::new(x)));
        }

        pub(super) fn add_computed_field(
            &mut self,
            key: Parsed<'arena, 'ast>,
            id_loc: LocNode<'arena>,
            polarity: Polarity,
            t: Parsed<'arena, 'ast>,
        ) {
            self.computed_props.push((
                key,
                ObjAnnotProp::ObjAnnotField(Box::new((id_loc, t, polarity))),
            ));
        }

        pub(super) fn add_computed_method(
            &mut self,
            key: Parsed<'arena, 'ast>,
            id_loc: LocNode<'arena>,
            fn_loc: LocNode<'arena>,
            def: FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) {
            self.computed_props.push((
                key,
                ObjAnnotProp::ObjAnnotMethod(Box::new(ObjAnnotMethodData {
                    id_loc,
                    fn_loc,
                    def,
                })),
            ));
        }

        pub(super) fn add_dict(&mut self, d: ObjAnnotDict<Parsed<'arena, 'ast>>) {
            if self.dict.is_none() {
                self.dict = Some(d);
            }
            // invalid: multiple indexers, just ignore
        }

        pub(super) fn add_call(&mut self, t: Parsed<'arena, 'ast>) {
            if self.proto.is_some() {
                // invalid: call after proto, just ignore
                return;
            }
            self.calls.push(t);
        }

        pub(super) fn add_proto(&mut self, x: (LocNode<'arena>, Parsed<'arena, 'ast>)) {
            if self.proto.is_some() || !self.calls.is_empty() {
                // invalid: multiple proto or proto after call, just ignore
                return;
            }
            self.proto = Some(x);
        }

        pub(super) fn add_spread(&mut self, t: Parsed<'arena, 'ast>) {
            if let Some(slice) = self.head_slice() {
                self.tail.push(slice);
            }
            self.tail
                .push(ObjSpreadAnnotElem::ObjSpreadAnnotElem(Box::new(t)));
            self.dict = None;
            self.props = BTreeMap::new();
            self.computed_props = Vec::new();
        }

        pub(super) fn object_type(
            mut self,
            loc: LocNode<'arena>,
            exact: bool,
            strictness_kind: TypeStrictnessKind,
        ) -> Parsed<'arena, 'ast> {
            let (last, tail) = if let Some(slice) = self.head_slice() {
                (slice, self.tail)
            } else if self.tail.is_empty() {
                (Self::empty_slice(), Vec::new())
            } else {
                let last = self.tail.pop().unwrap();
                (last, self.tail)
            };
            if tail.is_empty()
                && let ObjSpreadAnnotElem::ObjSpreadAnnotSlice(box ObjSpreadAnnotSliceData {
                    dict,
                    props,
                    computed_props,
                }) = last
            {
                let proto = if let Ok(calls) = Vec1::try_from_vec(self.calls) {
                    ObjAnnotProto::ObjAnnotCallable(Box::new(calls))
                } else if let Some((loc, t)) = self.proto {
                    ObjAnnotProto::ObjAnnotExplicitProto(Box::new((loc, t)))
                } else {
                    ObjAnnotProto::ObjAnnotImplicitProto
                };
                let obj_kind = match dict {
                    Some(d) => ObjKind::IndexedObj(Box::new(d)),
                    None => {
                        if exact {
                            ObjKind::ExactObj
                        } else {
                            ObjKind::InexactObj
                        }
                    }
                };
                let mut computed_props = computed_props;
                computed_props.reverse();
                return Parsed::Annot(Box::new(ParsedAnnot::ObjAnnot(Box::new(AnnotObjAnnot {
                    loc,
                    obj_kind,
                    props,
                    computed_props,
                    proto,
                    strictness_kind,
                }))));
            }
            let elems = Vec1::from_vec_push(tail, last);
            Parsed::Annot(Box::new(ParsedAnnot::ObjSpreadAnnot(Box::new(
                AnnotObjSpreadAnnot {
                    loc,
                    exact,
                    elems,
                    strictness_kind,
                },
            ))))
        }
    }
}

mod class_acc {
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;

    use flow_common::polarity::Polarity;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use vec1::Vec1;

    use super::*;

    pub(super) struct ClassAcc<'arena, 'ast> {
        pub(super) static_:
            BTreeMap<FlowSmolStr, ObjValueProp<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        pub(super) proto:
            BTreeMap<FlowSmolStr, ObjValueProp<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        pub(super) own: BTreeMap<FlowSmolStr, ObjValueProp<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        pub(super) computed_own: Vec<(
            Parsed<'arena, 'ast>,
            ObjValueProp<LocNode<'arena>, Parsed<'arena, 'ast>>,
        )>,
        pub(super) computed_proto: Vec<(
            Parsed<'arena, 'ast>,
            ObjValueProp<LocNode<'arena>, Parsed<'arena, 'ast>>,
        )>,
        pub(super) computed_static: Vec<(
            Parsed<'arena, 'ast>,
            ObjValueProp<LocNode<'arena>, Parsed<'arena, 'ast>>,
        )>,
        pub(super) dict: Option<ObjAnnotDict<Parsed<'arena, 'ast>>>,
        pub(super) abstract_props: BTreeSet<FlowSmolStr>,
    }

    impl<'arena, 'ast> ClassAcc<'arena, 'ast> {
        pub(super) fn empty() -> Self {
            ClassAcc {
                static_: BTreeMap::new(),
                proto: BTreeMap::new(),
                own: BTreeMap::new(),
                computed_own: Vec::new(),
                computed_proto: Vec::new(),
                computed_static: Vec::new(),
                dict: None,
                abstract_props: BTreeSet::new(),
            }
        }

        pub(super) fn add_abstract_name(&mut self, name: FlowSmolStr) {
            self.abstract_props.insert(name);
        }

        pub(super) fn add_field(
            &mut self,
            static_: bool,
            name: FlowSmolStr,
            id_loc: LocNode<'arena>,
            polarity: Polarity,
            t: Parsed<'arena, 'ast>,
        ) {
            let prop = ObjValueProp::ObjValueField(Box::new((id_loc, t, polarity)));
            if static_ {
                self.static_.insert(name, prop);
            } else {
                self.own.insert(name, prop);
            }
        }

        // Accumulate overloaded methods (e.g. bodyless overloads in a `.d.ts` class
        // body) into a Nel. Runtime classes can't actually have multiple bodies for
        // the same name, but bodyless overloads under tslib_syntax can.
        pub(super) fn add_method(
            &mut self,
            static_: bool,
            name: FlowSmolStr,
            id_loc: LocNode<'arena>,
            fn_loc: LocNode<'arena>,
            async_: bool,
            generator: bool,
            uses_this: bool,
            def: FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) {
            let m = ObjValueMethodData {
                id_loc,
                fn_loc,
                async_,
                generator,
                uses_this,
                def,
            };
            let map = if static_ {
                &mut self.static_
            } else {
                &mut self.proto
            };
            match map.entry(name) {
                std::collections::btree_map::Entry::Occupied(mut e) => {
                    if let ObjValueProp::ObjValueMethod(ms) = e.get_mut() {
                        ms.push(m);
                    } else {
                        e.insert(ObjValueProp::ObjValueMethod(Box::new(Vec1::new(m))));
                    }
                }
                std::collections::btree_map::Entry::Vacant(e) => {
                    e.insert(ObjValueProp::ObjValueMethod(Box::new(Vec1::new(m))));
                }
            }
        }

        pub(super) fn add_accessor(
            &mut self,
            static_: bool,
            name: FlowSmolStr,
            x: Accessor<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) {
            fn update<'arena, 'ast>(
                props: &mut BTreeMap<
                    FlowSmolStr,
                    ObjValueProp<LocNode<'arena>, Parsed<'arena, 'ast>>,
                >,
                name: FlowSmolStr,
                x: Accessor<LocNode<'arena>, Parsed<'arena, 'ast>>,
            ) {
                if let Some(ObjValueProp::ObjValueAccess(x_prime)) = props.remove(&name) {
                    props.insert(
                        name,
                        ObjValueProp::ObjValueAccess(Box::new(super::merge_accessors(*x_prime, x))),
                    );
                    return;
                }
                props.insert(name, ObjValueProp::ObjValueAccess(Box::new(x)));
            }

            if static_ {
                update(&mut self.static_, name, x)
            } else {
                update(&mut self.proto, name, x);
            }
        }

        pub(super) fn add_indexer(
            &mut self,
            static_: bool,
            dict: ObjAnnotDict<Parsed<'arena, 'ast>>,
        ) {
            if static_ {
                // static indexers not yet supported
                return;
            }
            if self.dict.is_none() {
                self.dict = Some(dict);
            }
        }

        pub(super) fn add_computed_field(
            &mut self,
            static_: bool,
            key: Parsed<'arena, 'ast>,
            id_loc: LocNode<'arena>,
            polarity: Polarity,
            t: Parsed<'arena, 'ast>,
        ) {
            let prop = ObjValueProp::ObjValueField(Box::new((id_loc, t, polarity)));
            if static_ {
                self.computed_static.push((key, prop));
            } else {
                self.computed_own.push((key, prop));
            }
        }

        pub(super) fn add_computed_method(
            &mut self,
            static_: bool,
            key: Parsed<'arena, 'ast>,
            id_loc: LocNode<'arena>,
            fn_loc: LocNode<'arena>,
            async_: bool,
            generator: bool,
            uses_this: bool,
            def: FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) {
            let prop = ObjValueProp::ObjValueMethod(Box::new(Vec1::new(ObjValueMethodData {
                id_loc,
                fn_loc,
                async_,
                generator,
                uses_this,
                def,
            })));
            if static_ {
                self.computed_static.push((key, prop));
            } else {
                self.computed_proto.push((key, prop));
            }
        }

        pub(super) fn class_def(
            self,
            abstract_: bool,
            tparams: TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
            extends: ClassExtends<LocNode<'arena>, Parsed<'arena, 'ast>>,
            implements: Vec<Parsed<'arena, 'ast>>,
            strictness_kind: TypeStrictnessKind,
        ) -> ClassSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
            ClassSig {
                tparams,
                extends,
                implements,
                static_props: self.static_,
                proto_props: self.proto,
                own_props: self.own,
                computed_own_props: self.computed_own,
                computed_proto_props: self.computed_proto,
                computed_static_props: self.computed_static,
                dict: self.dict,
                abstract_,
                abstract_props: self.abstract_props,
                strictness_kind,
            }
        }
    }
}

mod declare_class_acc {
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;

    use flow_common::polarity::Polarity;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use vec1::Vec1;

    use super::*;

    pub(super) struct DeclareClassAcc<'arena, 'ast> {
        pub(super) static_:
            BTreeMap<FlowSmolStr, InterfaceProp<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        pub(super) proto:
            BTreeMap<FlowSmolStr, InterfaceProp<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        pub(super) own: BTreeMap<FlowSmolStr, InterfaceProp<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        pub(super) computed_own: Vec<(
            Parsed<'arena, 'ast>,
            InterfaceProp<LocNode<'arena>, Parsed<'arena, 'ast>>,
        )>,
        pub(super) computed_proto: Vec<(
            Parsed<'arena, 'ast>,
            InterfaceProp<LocNode<'arena>, Parsed<'arena, 'ast>>,
        )>,
        pub(super) computed_static: Vec<(
            Parsed<'arena, 'ast>,
            InterfaceProp<LocNode<'arena>, Parsed<'arena, 'ast>>,
        )>,
        pub(super) scalls: Vec<Parsed<'arena, 'ast>>,
        pub(super) calls: Vec<Parsed<'arena, 'ast>>,
        pub(super) constructs: Vec<Parsed<'arena, 'ast>>,
        pub(super) dict: Option<ObjAnnotDict<Parsed<'arena, 'ast>>>,
        pub(super) static_dict: Option<ObjAnnotDict<Parsed<'arena, 'ast>>>,
        pub(super) abstract_props: BTreeSet<FlowSmolStr>,
    }

    impl<'arena, 'ast> DeclareClassAcc<'arena, 'ast> {
        pub(super) fn empty() -> Self {
            DeclareClassAcc {
                static_: BTreeMap::new(),
                proto: BTreeMap::new(),
                own: BTreeMap::new(),
                computed_own: Vec::new(),
                computed_proto: Vec::new(),
                computed_static: Vec::new(),
                scalls: Vec::new(),
                calls: Vec::new(),
                constructs: Vec::new(),
                dict: None,
                static_dict: None,
                abstract_props: BTreeSet::new(),
            }
        }

        pub(super) fn add_abstract_name(&mut self, name: FlowSmolStr) {
            self.abstract_props.insert(name);
        }

        pub(super) fn add_field(
            &mut self,
            static_: bool,
            name: FlowSmolStr,
            id_loc: LocNode<'arena>,
            polarity: Polarity,
            t: Parsed<'arena, 'ast>,
        ) {
            let prop = InterfaceProp::InterfaceField(Box::new((Some(id_loc), t, polarity)));
            if static_ {
                self.static_.insert(name, prop);
            } else {
                self.own.insert(name, prop);
            }
        }

        pub(super) fn add_indexer(
            &mut self,
            static_: bool,
            dict: ObjAnnotDict<Parsed<'arena, 'ast>>,
        ) {
            if static_ {
                if self.static_dict.is_none() {
                    self.static_dict = Some(dict);
                }
                // invalid: multiple indexers, just ignore
            } else if self.dict.is_none() {
                self.dict = Some(dict);
            } // invalid: multiple indexers, just ignore
        }

        pub(super) fn add_proto_field(
            &mut self,
            name: FlowSmolStr,
            id_loc: LocNode<'arena>,
            polarity: Polarity,
            t: Parsed<'arena, 'ast>,
        ) {
            self.proto.insert(
                name,
                InterfaceProp::InterfaceField(Box::new((Some(id_loc), t, polarity))),
            );
        }

        pub(super) fn append_method(
            &mut self,
            static_: bool,
            name: FlowSmolStr,
            id_loc: LocNode<'arena>,
            fn_loc: LocNode<'arena>,
            def: FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) {
            let m = (id_loc, fn_loc, def);
            let map = if static_ {
                &mut self.static_
            } else {
                &mut self.proto
            };

            match map.entry(name) {
                std::collections::btree_map::Entry::Occupied(mut e) => {
                    if let InterfaceProp::InterfaceMethod(ms) = e.get_mut() {
                        ms.push(m);
                    }
                }
                std::collections::btree_map::Entry::Vacant(e) => {
                    e.insert(InterfaceProp::InterfaceMethod(Box::new(Vec1::new(m))));
                }
            }
        }

        pub(super) fn add_accessor(
            &mut self,
            static_: bool,
            name: FlowSmolStr,
            x: Accessor<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) {
            fn update<'arena, 'ast>(
                props: &mut BTreeMap<
                    FlowSmolStr,
                    InterfaceProp<LocNode<'arena>, Parsed<'arena, 'ast>>,
                >,
                name: FlowSmolStr,
                x: Accessor<LocNode<'arena>, Parsed<'arena, 'ast>>,
            ) {
                if let Some(InterfaceProp::InterfaceAccess(x_prime)) = props.remove(&name) {
                    props.insert(
                        name,
                        InterfaceProp::InterfaceAccess(Box::new(super::merge_accessors(
                            *x_prime, x,
                        ))),
                    );
                    return;
                }
                props.insert(name, InterfaceProp::InterfaceAccess(Box::new(x)));
            }

            if static_ {
                update(&mut self.static_, name, x)
            } else {
                update(&mut self.proto, name, x);
            }
        }

        pub(super) fn add_computed_field(
            &mut self,
            static_: bool,
            proto: bool,
            key: Parsed<'arena, 'ast>,
            id_loc: LocNode<'arena>,
            polarity: Polarity,
            t: Parsed<'arena, 'ast>,
        ) {
            let prop = InterfaceProp::InterfaceField(Box::new((Some(id_loc), t, polarity)));
            if proto {
                self.computed_proto.push((key, prop));
            } else if static_ {
                self.computed_static.push((key, prop));
            } else {
                self.computed_own.push((key, prop));
            }
        }

        pub(super) fn append_computed_method(
            &mut self,
            static_: bool,
            key: Parsed<'arena, 'ast>,
            id_loc: LocNode<'arena>,
            fn_loc: LocNode<'arena>,
            def: FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) {
            let prop = InterfaceProp::InterfaceMethod(Box::new(Vec1::new((id_loc, fn_loc, def))));
            if static_ {
                self.computed_static.push((key, prop));
            } else {
                self.computed_proto.push((key, prop));
            }
        }

        pub(super) fn append_call(&mut self, static_: bool, t: Parsed<'arena, 'ast>) {
            if static_ {
                self.scalls.push(t);
            } else {
                self.calls.push(t);
            }
        }

        pub(super) fn declare_class_def(
            self,
            abstract_: bool,
            tparams: TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
            extends: ClassExtends<LocNode<'arena>, Parsed<'arena, 'ast>>,
            mixins: Vec<ClassMixins<LocNode<'arena>, Parsed<'arena, 'ast>>>,
            implements: Vec<Parsed<'arena, 'ast>>,
            strictness_kind: TypeStrictnessKind,
        ) -> DeclareClassSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
            let mut computed_own = self.computed_own;
            computed_own.reverse();
            let mut computed_proto = self.computed_proto;
            computed_proto.reverse();
            let mut computed_static = self.computed_static;
            computed_static.reverse();
            DeclareClassSig {
                tparams,
                extends,
                mixins,
                implements,
                static_props: self.static_,
                own_props: self.own,
                proto_props: self.proto,
                computed_own_props: computed_own,
                computed_proto_props: computed_proto,
                computed_static_props: computed_static,
                static_calls: self.scalls,
                calls: self.calls,
                constructs: self.constructs,
                dict: self.dict,
                static_dict: self.static_dict,
                abstract_,
                abstract_props: self.abstract_props,
                strictness_kind,
            }
        }
    }
}

mod interface_acc {
    use std::collections::BTreeMap;

    use flow_common::polarity::Polarity;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use vec1::Vec1;

    use super::*;

    pub(super) struct InterfaceAcc<'arena, 'ast> {
        pub(super) props:
            BTreeMap<FlowSmolStr, InterfaceProp<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        pub(super) computed_props: Vec<(
            Parsed<'arena, 'ast>,
            InterfaceProp<LocNode<'arena>, Parsed<'arena, 'ast>>,
        )>,
        pub(super) calls: Vec<Parsed<'arena, 'ast>>,
        pub(super) constructs: Vec<Parsed<'arena, 'ast>>,
        pub(super) dict: Option<ObjAnnotDict<Parsed<'arena, 'ast>>>,
    }

    impl<'arena, 'ast> InterfaceAcc<'arena, 'ast> {
        pub(super) fn empty() -> Self {
            InterfaceAcc {
                props: BTreeMap::new(),
                computed_props: Vec::new(),
                calls: Vec::new(),
                constructs: Vec::new(),
                dict: None,
            }
        }

        pub(super) fn add_field(
            &mut self,
            name: FlowSmolStr,
            id_loc: LocNode<'arena>,
            polarity: Polarity,
            t: Parsed<'arena, 'ast>,
        ) {
            self.props.insert(
                name,
                InterfaceProp::InterfaceField(Box::new((Some(id_loc), t, polarity))),
            );
        }

        pub(super) fn add_indexer(&mut self, dict: ObjAnnotDict<Parsed<'arena, 'ast>>) {
            if self.dict.is_none() {
                self.dict = Some(dict);
            }
            // invalid: multiple indexers, just ignore
        }

        pub(super) fn append_method(
            &mut self,
            name: FlowSmolStr,
            id_loc: LocNode<'arena>,
            fn_loc: LocNode<'arena>,
            def: FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) {
            let m = (id_loc, fn_loc, def);
            match self.props.entry(name) {
                std::collections::btree_map::Entry::Occupied(mut e) => {
                    if let InterfaceProp::InterfaceMethod(ms) = e.get_mut() {
                        ms.push(m);
                    }
                }
                std::collections::btree_map::Entry::Vacant(e) => {
                    e.insert(InterfaceProp::InterfaceMethod(Box::new(Vec1::new(m))));
                }
            }
        }

        pub(super) fn add_accessor(
            &mut self,
            name: FlowSmolStr,
            x: Accessor<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) {
            if let Some(InterfaceProp::InterfaceAccess(x_prime)) = self.props.remove(&name) {
                self.props.insert(
                    name,
                    InterfaceProp::InterfaceAccess(Box::new(super::merge_accessors(*x_prime, x))),
                );
                return;
            }
            self.props
                .insert(name, InterfaceProp::InterfaceAccess(Box::new(x)));
        }

        pub(super) fn add_computed_field(
            &mut self,
            key: Parsed<'arena, 'ast>,
            id_loc: LocNode<'arena>,
            polarity: Polarity,
            t: Parsed<'arena, 'ast>,
        ) {
            self.computed_props.push((
                key,
                InterfaceProp::InterfaceField(Box::new((Some(id_loc), t, polarity))),
            ));
        }

        pub(super) fn append_computed_method(
            &mut self,
            key: Parsed<'arena, 'ast>,
            id_loc: LocNode<'arena>,
            fn_loc: LocNode<'arena>,
            def: FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) {
            self.computed_props.push((
                key,
                InterfaceProp::InterfaceMethod(Box::new(Vec1::new((id_loc, fn_loc, def)))),
            ));
        }

        pub(super) fn append_call(&mut self, t: Parsed<'arena, 'ast>) {
            self.calls.push(t);
        }

        pub(super) fn append_construct(&mut self, t: Parsed<'arena, 'ast>) {
            self.constructs.push(t);
        }

        pub(super) fn interface_def(
            self,
            abstract_: bool,
            extends: Vec<Parsed<'arena, 'ast>>,
            strictness_kind: TypeStrictnessKind,
        ) -> InterfaceSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
            let mut computed_props = self.computed_props;
            computed_props.reverse();
            InterfaceSig {
                extends,
                props: self.props,
                computed_props,
                calls: self.calls,
                constructs: self.constructs,
                dict: self.dict,
                abstract_,
                strictness_kind,
            }
        }
    }
}

mod object_literal_acc {
    use std::collections::BTreeMap;

    use flow_common::polarity::Polarity;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use vec1::Vec1;

    use super::*;

    pub(super) struct ObjectLiteralAcc<'arena, 'ast> {
        pub(super) proto: Option<(LocNode<'arena>, Parsed<'arena, 'ast>)>,
        pub(super) props:
            BTreeMap<FlowSmolStr, ObjValueProp<LocNode<'arena>, Parsed<'arena, 'ast>>>,
        pub(super) tail: Vec<ObjValueSpreadElem<LocNode<'arena>, Parsed<'arena, 'ast>>>,
    }

    impl<'arena, 'ast> ObjectLiteralAcc<'arena, 'ast> {
        pub(super) fn empty() -> Self {
            ObjectLiteralAcc {
                proto: None,
                props: BTreeMap::new(),
                tail: Vec::new(),
            }
        }

        fn head_slice(
            &mut self,
        ) -> Option<ObjValueSpreadElem<LocNode<'arena>, Parsed<'arena, 'ast>>> {
            if self.props.is_empty() {
                None
            } else {
                Some(ObjValueSpreadElem::ObjValueSpreadSlice(Box::new(
                    std::mem::take(&mut self.props),
                )))
            }
        }

        pub(super) fn add_proto(&mut self, x: (LocNode<'arena>, Parsed<'arena, 'ast>)) {
            self.proto = Some(x);
        }

        pub(super) fn add_field(
            &mut self,
            name: FlowSmolStr,
            id_loc: LocNode<'arena>,
            t: Parsed<'arena, 'ast>,
            polarity: Polarity,
        ) {
            self.props.insert(
                name,
                ObjValueProp::ObjValueField(Box::new((id_loc, t, polarity))),
            );
        }

        pub(super) fn add_method(
            &mut self,
            name: FlowSmolStr,
            id_loc: LocNode<'arena>,
            fn_loc: LocNode<'arena>,
            async_: bool,
            generator: bool,
            def: FunSig<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) {
            let m = ObjValueMethodData {
                id_loc,
                fn_loc,
                async_,
                generator,
                uses_this: false,
                def,
            };
            self.props
                .insert(name, ObjValueProp::ObjValueMethod(Box::new(Vec1::new(m))));
        }

        pub(super) fn add_accessor(
            &mut self,
            name: FlowSmolStr,
            x: Accessor<LocNode<'arena>, Parsed<'arena, 'ast>>,
        ) {
            if let Some(ObjValueProp::ObjValueAccess(x_prime)) = self.props.remove(&name) {
                self.props.insert(
                    name,
                    ObjValueProp::ObjValueAccess(Box::new(super::merge_accessors(*x_prime, x))),
                );
                return;
            }
            self.props
                .insert(name, ObjValueProp::ObjValueAccess(Box::new(x)));
        }

        pub(super) fn add_spread(&mut self, t: Parsed<'arena, 'ast>) {
            if let Some(slice) = self.head_slice() {
                self.tail.push(slice);
            }
            self.props = BTreeMap::new();
            self.tail
                .push(ObjValueSpreadElem::ObjValueSpreadElem(Box::new(t)));
        }

        pub(super) fn object_lit(
            mut self,
            loc: LocNode<'arena>,
            frozen: bool,
            strictness_kind: TypeStrictnessKind,
        ) -> Parsed<'arena, 'ast> {
            let mut elems = match self.head_slice() {
                None => self.tail,
                Some(slice) => {
                    let mut tail = self.tail;
                    tail.push(slice);
                    tail
                }
            };
            if elems.is_empty() {
                return Parsed::Value(Box::new(ParsedValue::ObjLit(Box::new(ValueObjLit {
                    loc,
                    frozen,
                    proto: self.proto,
                    props: BTreeMap::new(),
                    strictness_kind,
                }))));
            }
            if elems.len() == 1
                && matches!(&elems[0], ObjValueSpreadElem::ObjValueSpreadSlice(box _))
                && let Some(ObjValueSpreadElem::ObjValueSpreadSlice(box props)) = elems.pop()
            {
                return Parsed::Value(Box::new(ParsedValue::ObjLit(Box::new(ValueObjLit {
                    loc,
                    frozen,
                    proto: self.proto,
                    props,
                    strictness_kind,
                }))));
            }

            Parsed::Value(Box::new(ParsedValue::ObjSpreadLit(Box::new(
                ValueObjSpreadLit {
                    loc,
                    frozen,
                    proto: self.proto,
                    elems: Vec1::try_from_vec(elems).unwrap(),
                    strictness_kind,
                },
            ))))
        }
    }
}

fn expression_for_computed_key<'arena, 'ast>(
    scope: ScopeId,
    tbls: &mut Tables<'arena, 'ast>,
    expr: &ast::expression::Expression<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    let loc = tbls.push_loc(expr.loc().dupe());
    match expr.deref() {
        ExpressionInner::Identifier { inner, .. } => {
            let id_loc = tbls.push_loc(inner.loc.dupe());
            val_ref(false, scope, id_loc, inner.name.dupe())
        }
        ExpressionInner::StringLiteral { inner, .. } => {
            string_literal(FrozenKind::NotFrozen, loc, &inner.value)
        }
        ExpressionInner::NumberLiteral { inner, .. } => Parsed::Value(Box::new(
            ParsedValue::NumberLit(Box::new((loc, inner.value, inner.raw.dupe()))),
        )),
        ExpressionInner::BigIntLiteral { inner, .. } => Parsed::Value(Box::new(
            ParsedValue::BigIntLit(Box::new((loc, inner.value, inner.raw.dupe()))),
        )),
        ExpressionInner::Member { inner, .. } => {
            let base = expression_for_computed_key(scope, tbls, &inner.object);
            match &inner.property {
                ast::expression::member::Property::PropertyIdentifier(id) => Parsed::Eval(
                    loc,
                    Box::new(base),
                    Box::new(Op::GetProp(Box::new(id_name(id).clone()))),
                ),
                _ => Parsed::Err(
                    loc.dupe(),
                    Errno::SigError(Box::new(
                        signature_error::SignatureError::UnexpectedObjectKey(loc.dupe(), loc),
                    )),
                ),
            }
        }
        _ => Parsed::Err(
            loc.dupe(),
            Errno::SigError(Box::new(
                signature_error::SignatureError::UnexpectedObjectKey(loc.dupe(), loc),
            )),
        ),
    }
}

/// Build the key of an object-type computed key written as a value name,
/// `{[x]: V}` or `{[x.y]: V}`. The name is read as the value's type, the way
/// `typeof x` is, which is also how the value-level `{[x]: v}` reads it.
fn value_object_type_key<'arena, 'ast>(
    scope: ScopeId,
    tbls: &mut Tables<'arena, 'ast>,
    id: &ast::types::generic::Identifier<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    match id {
        ast::types::generic::Identifier::Unqualified(head) => {
            let ref_loc = tbls.push_loc(head.loc.dupe());
            val_ref(false, scope, ref_loc, head.name.dupe())
        }
        ast::types::generic::Identifier::Qualified(qualified) => {
            let loc = tbls.push_loc(qualified.loc.dupe());
            let base = value_object_type_key(scope, tbls, &qualified.qualification);
            Parsed::Eval(
                loc,
                Box::new(base),
                Box::new(Op::GetProp(Box::new(qualified.id.name.dupe()))),
            )
        }
        // `object_type_key_form` reports the `Name` form only for a plain or a
        // qualified name, and `import('m').X` is neither.
        ast::types::generic::Identifier::ImportTypeAnnot(import_type) => {
            let loc = tbls.push_loc(import_type.loc.dupe());
            Parsed::Err(loc, Errno::CheckError)
        }
    }
}

enum ResolvedPropKey<'ast> {
    Named(FlowSmolStr, Loc),
    ComputedRef {
        expr_loc: Loc,
        ref_name: FlowSmolStr,
    },
    ComputedExpr {
        expr: &'ast ast::expression::Expression<Loc, Loc>,
    },
    ComputedError {
        loc: Loc,
    },
}

fn resolve_prop_key<'ast>(
    key: &'ast ast::expression::object::Key<Loc, Loc>,
) -> ResolvedPropKey<'ast> {
    use ast::expression::object::Key;
    match key {
        Key::Identifier(id) => ResolvedPropKey::Named(id.name.dupe(), id.loc.dupe()),
        Key::StringLiteral((id_loc, ast::StringLiteral { value: name, .. })) => {
            ResolvedPropKey::Named(name.dupe(), id_loc.dupe())
        }
        Key::NumberLiteral((id_loc, ast::NumberLiteral { value, .. }))
            if flow_common::js_number::is_float_safe_integer(*value) =>
        {
            ResolvedPropKey::Named(
                FlowSmolStr::new(flow_common::js_number::ecma_string_of_float(*value)),
                id_loc.dupe(),
            )
        }
        Key::BigIntLiteral((id_loc, lit)) => ResolvedPropKey::Named(
            FlowSmolStr::new(ast_utils::string_of_bigint(lit)),
            id_loc.dupe(),
        ),
        Key::Computed(ck) => {
            if let Some(name) = ast_utils::well_known_symbol_name(&ck.expression) {
                return ResolvedPropKey::Named(FlowSmolStr::from(name), ck.expression.loc().dupe());
            }
            match ck.expression.deref() {
                ExpressionInner::StringLiteral { loc, inner, .. } => {
                    ResolvedPropKey::Named(inner.value.dupe(), loc.dupe())
                }
                ExpressionInner::NumberLiteral { loc, inner, .. }
                    if flow_common::js_number::is_float_safe_integer(inner.value) =>
                {
                    ResolvedPropKey::Named(
                        FlowSmolStr::new(flow_common::js_number::ecma_string_of_float(inner.value)),
                        loc.dupe(),
                    )
                }
                ExpressionInner::BigIntLiteral { loc, inner, .. } => ResolvedPropKey::Named(
                    FlowSmolStr::new(ast_utils::string_of_bigint(inner)),
                    loc.dupe(),
                ),
                // A negative numeric key is a unary minus over a positive
                // literal, both from the expression parser and from the
                // type-to-expression converter for a `.js` computed method key.
                // The within-file checker names it by evaluating the key, so it
                // has to be named here too or the property goes missing across a
                // module boundary.
                ExpressionInner::Unary { loc, inner, .. }
                    if inner.operator == ast::expression::UnaryOperator::Minus =>
                {
                    match inner.argument.deref() {
                        ExpressionInner::NumberLiteral { inner: num, .. }
                            if flow_common::js_number::is_float_safe_integer(-num.value) =>
                        {
                            ResolvedPropKey::Named(
                                FlowSmolStr::new(flow_common::js_number::ecma_string_of_float(
                                    -num.value,
                                )),
                                loc.dupe(),
                            )
                        }
                        _ => ResolvedPropKey::ComputedError { loc: ck.loc.dupe() },
                    }
                }
                ExpressionInner::Identifier { inner, .. } => ResolvedPropKey::ComputedRef {
                    expr_loc: inner.loc.dupe(),
                    ref_name: inner.name.dupe(),
                },
                ExpressionInner::Member { .. } => ResolvedPropKey::ComputedExpr {
                    expr: &ck.expression,
                },
                _ => ResolvedPropKey::ComputedError { loc: ck.loc.dupe() },
            }
        }
        Key::NumberLiteral((loc, _)) | Key::PrivateName(ast::PrivateName { loc, .. }) => {
            ResolvedPropKey::ComputedError { loc: loc.dupe() }
        }
    }
}

// Parse the value type of a class field, given an already-pushed `id_loc_node`.
// Mirrors the identifier-field logic in `class_def` but uses `id_loc_node` for
// the missing-annotation error so callers can push the key loc first (keeping
// packed locs in ascending order for computed keys).
fn class_field_value_type<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    t: &'ast ast::types::AnnotationOrHint<Loc, Loc>,
    value: &'ast ast::class::property::Value<Loc, Loc>,
    id_loc_node: LocNode<'arena>,
    name_for_err: &FlowSmolStr,
) -> Parsed<'arena, 'ast> {
    match t {
        ast::types::AnnotationOrHint::Available(annot_t) => {
            annot(opts, scope, scopes, tbls, xs, &annot_t.annotation)
        }
        ast::types::AnnotationOrHint::Missing(_) => match value {
            ast::class::property::Value::Initialized(e) => {
                expression(opts, scope, scopes, tbls, FrozenKind::NotFrozen, e)
            }
            ast::class::property::Value::Declared | ast::class::property::Value::Uninitialized => {
                Parsed::Err(
                    id_loc_node.dupe(),
                    Errno::SigError(Box::new(
                        signature_error::SignatureError::ExpectedAnnotation(
                            id_loc_node,
                            ExpectedAnnotationSort::Property {
                                name: name_for_err.clone(),
                            },
                        ),
                    )),
                )
            }
        },
    }
}

fn annot<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    t: &ast::types::Type<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    let (_, annot) = annot_with_loc(opts, scope, scopes, tbls, xs, t);
    annot
}

fn annot_with_loc<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    t: &ast::types::Type<Loc, Loc>,
) -> (LocNode<'arena>, Parsed<'arena, 'ast>) {
    let raw_loc = t.loc();
    let loc = tbls.push_loc(raw_loc.clone());
    let loc_return = loc.dupe();
    let annot = match t.deref() {
        TypeInner::Any { .. } => Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc)))),
        TypeInner::Mixed { .. } => Parsed::Annot(Box::new(ParsedAnnot::Mixed(Box::new(loc)))),
        TypeInner::Empty { .. } => Parsed::Annot(Box::new(ParsedAnnot::Empty(Box::new(loc)))),
        TypeInner::Void { .. } => Parsed::Annot(Box::new(ParsedAnnot::Void(Box::new(loc)))),
        TypeInner::Null { .. } => Parsed::Annot(Box::new(ParsedAnnot::Null(Box::new(loc)))),
        TypeInner::Symbol { .. } => Parsed::Annot(Box::new(ParsedAnnot::Symbol(Box::new(loc)))),
        TypeInner::Number { .. } => Parsed::Annot(Box::new(ParsedAnnot::Number(Box::new(loc)))),
        TypeInner::BigInt { .. } => Parsed::Annot(Box::new(ParsedAnnot::BigInt(Box::new(loc)))),
        TypeInner::String { .. } => Parsed::Annot(Box::new(ParsedAnnot::String(Box::new(loc)))),
        TypeInner::Boolean { .. } => Parsed::Annot(Box::new(ParsedAnnot::Boolean(Box::new(loc)))),
        TypeInner::Unknown { .. } => Parsed::Annot(Box::new(ParsedAnnot::Mixed(Box::new(loc)))),
        TypeInner::Never { .. } => Parsed::Annot(Box::new(ParsedAnnot::Empty(Box::new(loc)))),
        TypeInner::Undefined { .. } => Parsed::Annot(Box::new(ParsedAnnot::Void(Box::new(loc)))),
        TypeInner::UniqueSymbol { .. } => {
            Parsed::Annot(Box::new(ParsedAnnot::UniqueSymbol(Box::new(loc))))
        }
        TypeInner::StringLiteral { literal, .. } => Parsed::Annot(Box::new(
            ParsedAnnot::SingletonString(Box::new((loc, literal.value.dupe()))),
        )),
        TypeInner::NumberLiteral { literal, .. } => Parsed::Annot(Box::new(
            ParsedAnnot::SingletonNumber(Box::new((loc, literal.value, literal.raw.dupe()))),
        )),
        TypeInner::BigIntLiteral { literal, .. } => Parsed::Annot(Box::new(
            ParsedAnnot::SingletonBigInt(Box::new((loc, literal.value, literal.raw.dupe()))),
        )),
        TypeInner::BooleanLiteral { literal, .. } => Parsed::Annot(Box::new(
            ParsedAnnot::SingletonBoolean(Box::new((loc, literal.value))),
        )),
        TypeInner::Nullable { inner, .. } => {
            let arg = annot(opts, scope, scopes, tbls, xs, &inner.argument);
            Parsed::Annot(Box::new(ParsedAnnot::Maybe(Box::new((loc, arg)))))
        }
        TypeInner::Array { inner, .. } => {
            let arg = annot(opts, scope, scopes, tbls, xs, &inner.argument);
            Parsed::Annot(Box::new(ParsedAnnot::Array(Box::new((loc, arg)))))
        }
        TypeInner::Conditional { inner, .. } => {
            conditional_type(opts, scope, scopes, tbls, xs, loc, inner.as_ref())
        }
        TypeInner::Infer { inner, .. } => {
            infer_type(opts, scope, scopes, tbls, xs, loc, inner.as_ref())
        }
        TypeInner::Function { inner, .. } => {
            let def = function_type(false, opts, scope, scopes, tbls, xs, inner.as_ref());
            Parsed::Annot(Box::new(ParsedAnnot::FunAnnot(Box::new((loc, def)))))
        }
        TypeInner::Component { inner, .. } => {
            let def = component_type(
                opts,
                scope,
                scopes,
                tbls,
                xs,
                &inner.tparams,
                &inner.params,
                &inner.renders,
            );
            Parsed::Annot(Box::new(ParsedAnnot::ComponentAnnot(Box::new((loc, def)))))
        }
        TypeInner::Object { inner, .. } => {
            // Strip [this] when descending into a nested object type. The
            // enclosing interface's [this] is NOT in scope inside nested object
            // types — matches both TypeScript (TS2526) and Flow's existing
            // behavior in classes. Without this strip, [this] propagates from
            // the enclosing [interface_def]'s [xs_body] all the way down through
            // arbitrary annotation nesting.
            let this_name = FlowSmolStr::new("this");
            let removed = xs.remove_all(&this_name);
            let result = object_type(opts, scope, scopes, tbls, xs, loc, inner.as_ref());
            xs.restore_all(&this_name, removed);
            result
        }
        TypeInner::Interface { inner, .. } => {
            // Same reasoning as the [T.Object] case: an inline interface annotation
            // introduces its own [this]-binding scope (controlled by its own
            // [bind_this_in_body] flag, which defaults to false here), so the
            // enclosing scope's [this] should NOT leak in.
            let this_name = FlowSmolStr::new("this");
            let removed = xs.remove_all(&this_name);
            let def = interface_def(
                false,
                opts,
                scope,
                scopes,
                tbls,
                xs,
                &inner.extends,
                &inner.body.1.properties,
            );
            xs.restore_all(&this_name, removed);
            Parsed::Annot(Box::new(ParsedAnnot::InlineInterface(Box::new((loc, def)))))
        }
        TypeInner::Generic { inner, .. } => {
            maybe_special_generic(opts, scope, scopes, tbls, xs, loc, inner.as_ref())
        }
        TypeInner::IndexedAccess { inner, .. } => {
            let obj = annot(opts, scope, scopes, tbls, xs, &inner.object);
            match &*inner.index {
                TypeInner::StringLiteral { literal: lit, .. } => Parsed::Annot(Box::new(
                    ParsedAnnot::PropertyType(Box::new(AnnotPropertyType {
                        loc,
                        obj,
                        prop: lit.value.dupe(),
                    })),
                )),
                _ => {
                    let elem = annot(opts, scope, scopes, tbls, xs, &inner.index);
                    Parsed::Annot(Box::new(ParsedAnnot::ElementType(Box::new(
                        AnnotElementType { loc, obj, elem },
                    ))))
                }
            }
        }
        TypeInner::OptionalIndexedAccess { inner, .. } => {
            let (_, result) =
                optional_indexed_access(opts, scope, scopes, tbls, xs, loc, inner.as_ref());
            result
        }
        TypeInner::Tuple { inner, .. } => {
            let mut elems = Vec::new();
            for el in inner.elements.iter() {
                elems.push(tuple_element(opts, scope, scopes, tbls, xs, el));
            }
            Parsed::Annot(Box::new(ParsedAnnot::Tuple(Box::new(AnnotTuple {
                loc,
                elems,
                inexact: inner.inexact,
                strictness_kind: tbls.strictness_kind,
            }))))
        }
        TypeInner::Union { inner, .. } => {
            let t0 = annot(opts, scope, scopes, tbls, xs, &inner.types.0);
            let t1 = annot(opts, scope, scopes, tbls, xs, &inner.types.1);
            let ts: Vec<_> = inner
                .types
                .2
                .iter()
                .map(|t| annot(opts, scope, scopes, tbls, xs, t))
                .collect();
            Parsed::Annot(Box::new(ParsedAnnot::Union(Box::new(AnnotUnion {
                loc,
                t0,
                t1,
                ts,
            }))))
        }
        TypeInner::Intersection { inner, .. } => {
            let t0 = annot(opts, scope, scopes, tbls, xs, &inner.types.0);
            let t1 = annot(opts, scope, scopes, tbls, xs, &inner.types.1);
            let ts: Vec<_> = inner
                .types
                .2
                .iter()
                .map(|t| annot(opts, scope, scopes, tbls, xs, t))
                .collect();
            Parsed::Annot(Box::new(ParsedAnnot::Intersection(Box::new(
                AnnotIntersection { loc, t0, t1, ts },
            ))))
        }
        TypeInner::Typeof { inner, .. } => typeof_(
            opts,
            scope,
            scopes,
            tbls,
            xs,
            loc,
            &inner.argument,
            &inner.targs,
        ),
        TypeInner::Renders { inner, .. } => {
            let arg = annot(opts, scope, scopes, tbls, xs, &inner.argument);
            Parsed::Annot(Box::new(ParsedAnnot::Renders(Box::new(AnnotRenders {
                loc,
                arg,
                variant: inner.variant,
            }))))
        }
        TypeInner::Keyof { inner, .. } => {
            let t = annot(opts, scope, scopes, tbls, xs, &inner.argument);
            Parsed::Annot(Box::new(ParsedAnnot::Keys(Box::new((loc, t)))))
        }
        TypeInner::ReadOnly { inner, .. } => match &*inner.argument {
            TypeInner::Tuple { .. } => {
                let t = annot(opts, scope, scopes, tbls, xs, &inner.argument);
                Parsed::Annot(Box::new(ParsedAnnot::ReadOnly(Box::new((loc, t)))))
            }
            TypeInner::Array { inner: arr, .. } => {
                let t = annot(opts, scope, scopes, tbls, xs, &arr.argument);
                Parsed::Annot(Box::new(ParsedAnnot::ReadOnlyArray(Box::new((loc, t)))))
            }
            _ => Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc)))),
        },
        TypeInner::TemplateLiteral { inner, .. } if opts.tslib_syntax => {
            let quasis = inner.quasis.iter().map(|q| q.value.cooked.dupe()).collect();
            let types = inner
                .types
                .iter()
                .map(|t| annot(opts, scope, scopes, tbls, xs, t))
                .collect();
            Parsed::Annot(Box::new(ParsedAnnot::TemplateLiteral(Box::new(
                AnnotTemplateLiteral { loc, quasis, types },
            ))))
        }
        TypeInner::TemplateLiteral { inner, .. } => {
            for t in inner.types.iter() {
                annot(opts, scope, scopes, tbls, xs, t);
            }
            Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc))))
        }
        TypeInner::ConstructorType {
            inner: func,
            abstract_,
            ..
        } if opts.tslib_syntax => {
            use interface_acc::InterfaceAcc;
            let fsig = function_type(false, opts, scope, scopes, tbls, xs, func);
            let mut acc = InterfaceAcc::empty();
            acc.append_construct(Parsed::Annot(Box::new(ParsedAnnot::FunAnnot(Box::new((
                loc.dupe(),
                fsig,
            ))))));
            let def = acc.interface_def(*abstract_, vec![], tbls.strictness_kind);
            Parsed::Annot(Box::new(ParsedAnnot::InlineInterface(Box::new((loc, def)))))
        }
        TypeInner::ConstructorType { inner: func, .. } => {
            // tslib_syntax off: degrade to Any, matching the TemplateLiteral
            // pattern above. type_annotation surfaces the user-facing error.
            function_type(false, opts, scope, scopes, tbls, xs, func);
            Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc))))
        }
        TypeInner::Exists { .. } => Parsed::Annot(Box::new(ParsedAnnot::Exists(Box::new(loc)))),
    };
    (loc_return, annot)
}

fn typeof_<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    typeof_loc: LocNode<'arena>,
    expr: &ast::types::typeof_::Target<Loc, Loc>,
    targs: &Option<ast::types::TypeArgs<Loc, Loc>>,
) -> Parsed<'arena, 'ast> {
    fn finish<'arena, 'ast>(
        opts: &TypeSigOptions,
        scope: ScopeId,
        scopes: &mut scope::Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        xs: &mut tparam_stack::TParamStack,
        typeof_loc: LocNode<'arena>,
        mut t: Parsed<'arena, 'ast>,
        mut qname: Vec<FlowSmolStr>,
        targs: &Option<ast::types::TypeArgs<Loc, Loc>>,
        chain: Vec<(Loc, FlowSmolStr)>,
    ) -> Parsed<'arena, 'ast> {
        for (id_loc, x) in chain.into_iter().rev() {
            let id_loc = tbls.push_loc(id_loc);
            t = Parsed::Eval(
                id_loc,
                Box::new(t),
                Box::new(Op::GetProp(Box::new(x.clone()))),
            );
            qname.push(x);
        }
        let targs = targs.as_ref().map(|targs| {
            targs
                .arguments
                .iter()
                .map(|t| annot(opts, scope, scopes, tbls, xs, t))
                .collect()
        });
        Parsed::Annot(Box::new(ParsedAnnot::Typeof(Box::new(AnnotTypeof {
            loc: typeof_loc,
            qname,
            t,
            targs,
        }))))
    }

    let mut target = expr;
    let mut chain = Vec::new();
    loop {
        match target {
            ast::types::typeof_::Target::Qualified(qualified) => {
                let id_loc = qualified.id.loc.dupe();
                let name = qualified.id.name.dupe();
                chain.push((id_loc, name));
                target = &qualified.qualification;
            }
            ast::types::typeof_::Target::Unqualified(id) => {
                let id_loc = tbls.push_loc(id.loc.dupe());
                let name = id.name.dupe();
                let t = typeof_ref(scope, id_loc, name.clone());
                return finish(
                    opts,
                    scope,
                    scopes,
                    tbls,
                    xs,
                    typeof_loc,
                    t,
                    vec![name],
                    targs,
                    chain,
                );
            }
            ast::types::typeof_::Target::Import(it) => {
                let import_loc = tbls.push_loc(it.loc.dupe());
                let value = &it.source.1.value;
                let mref = Userland::from_smol_str(value.dupe());
                let mref = tbls.push_module_ref(mref);
                let t = Parsed::ImportTypeAnnot {
                    loc: import_loc,
                    mref,
                };
                return finish(
                    opts,
                    scope,
                    scopes,
                    tbls,
                    xs,
                    typeof_loc,
                    t,
                    vec![value.dupe()],
                    targs,
                    chain,
                );
            }
        }
    }
}

fn tuple_element<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    el: &ast::types::tuple::Element<Loc, Loc>,
) -> TupleElement<LocNode<'arena>, Parsed<'arena, 'ast>> {
    let loc = tbls.push_loc(el.loc().clone());
    match el {
        ast::types::tuple::Element::UnlabeledElement {
            annot: type_,
            optional,
            ..
        } => {
            let t = annot(opts, scope, scopes, tbls, xs, type_);
            let t = if *optional {
                Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))))
            } else {
                t
            };
            TupleElement::TupleElement(Box::new(TupleElementData {
                loc,
                name: None,
                t,
                polarity: Polarity::Neutral,
                optional: *optional,
            }))
        }
        ast::types::tuple::Element::LabeledElement { element, .. } => {
            let t = annot(opts, scope, scopes, tbls, xs, &element.annot);
            let t = if element.optional {
                Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))))
            } else {
                t
            };
            let name = Some(id_name(&element.name).clone());
            TupleElement::TupleElement(Box::new(TupleElementData {
                loc,
                name,
                t,
                polarity: polarity(element.variance.as_ref().map(|v| (v.loc.dupe(), v.clone()))),
                optional: element.optional,
            }))
        }
        ast::types::tuple::Element::SpreadElement { element, .. } => {
            let t = annot(opts, scope, scopes, tbls, xs, &element.annot);
            let name = element.name.as_ref().map(|n| id_name(n).clone());
            TupleElement::TupleSpread(Box::new(TupleSpreadData { loc, name, t }))
        }
    }
}

fn type_guard_opt<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    guard: &ast::types::TypeGuard<Loc, Loc>,
) -> Option<(
    LocNode<'arena>,
    (LocNode<'arena>, FlowSmolStr),
    Option<Parsed<'arena, 'ast>>,
    ast::types::TypeGuardKind,
)> {
    let (x, t_opt) = &guard.guard;
    if t_opt.is_none() && guard.kind != ast::types::TypeGuardKind::Asserts {
        return None;
    }
    let gloc = tbls.push_loc(guard.loc.clone());
    let loc = tbls.push_loc(x.loc.dupe());
    let name = x.name.dupe();
    let t = t_opt
        .as_ref()
        .map(|t| annot(opts, scope, scopes, tbls, xs, t));
    Some((gloc, (loc, name), t, guard.kind))
}

fn type_guard_return_annot<'arena, 'ast>(
    kind: ast::types::TypeGuardKind,
    loc: LocNode<'arena>,
) -> Parsed<'arena, 'ast> {
    let annot = match kind {
        ast::types::TypeGuardKind::Asserts => ParsedAnnot::Void(Box::new(loc)),
        _ => ParsedAnnot::Boolean(Box::new(loc)),
    };
    Parsed::Annot(Box::new(annot))
}

fn return_annot<'arena, 'ast>(
    is_method: bool,
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    r: &ast::types::function::ReturnAnnotation<Loc, Loc>,
) -> (
    Parsed<'arena, 'ast>,
    Option<TypeGuard<LocNode<'arena>, Parsed<'arena, 'ast>>>,
) {
    match r {
        ast::types::function::ReturnAnnotation::Available(a) => {
            let t = annot(opts, scope, scopes, tbls, xs, &a.annotation);
            (t, None)
        }
        ast::types::function::ReturnAnnotation::TypeGuard(tg) => {
            let loc = tbls.push_loc(tg.loc.dupe());
            let guard = type_guard_opt(opts, scope, scopes, tbls, xs, tg).map(
                |(guard_loc, param_name, type_guard, kind)| TypeGuard {
                    loc: guard_loc,
                    param_name,
                    type_guard,
                    kind,
                },
            );
            (type_guard_return_annot(tg.kind, loc), guard)
        }
        ast::types::function::ReturnAnnotation::Missing(loc) => {
            let loc = tbls.push_loc(loc.dupe());
            // Methods in `.ts` / `.d.ts` files default to implicit `any` to match
            // TypeScript's ambient-declaration semantics; everywhere else (and all
            // non-method callers) preserve the legacy Void default. The typing layer
            // in `type_annotation.rs` applies the same gate so the cross-module
            // signature stays consistent with in-file inference.
            if is_method && opts.is_ts_file {
                (
                    Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc)))),
                    None,
                )
            } else {
                (
                    Parsed::Annot(Box::new(ParsedAnnot::Void(Box::new(loc)))),
                    None,
                )
            }
        }
    }
}

fn convert_effect<Loc>(
    opts: &TypeSigOptions,
    effect_: &ast::function::Effect,
    fun_loc_opt: Option<Loc>,
    name_opt: Option<&FlowSmolStr>,
) -> ReactEffect<Loc> {
    use ast::function::Effect;
    match (effect_, fun_loc_opt) {
        (Effect::Hook, Some(loc)) if opts.component_syntax_enabled_in_config => {
            ReactEffect::HookDecl(Box::new(loc))
        }
        (Effect::Hook, None) if opts.component_syntax_enabled_in_config => ReactEffect::HookAnnot,
        (Effect::Hook, _) => ReactEffect::ArbitraryEffect,
        (Effect::Arbitrary, _)
            if opts.hook_compatibility
                && matches!(name_opt, Some(n) if ast_utils::hook_name(n)) =>
        {
            ReactEffect::HookAnnot
        }
        (Effect::Arbitrary, _) => ReactEffect::ArbitraryEffect,
    }
}

fn function_type<'arena, 'ast>(
    is_method: bool,
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    f: &ast::types::Function<Loc, Loc>,
) -> FunSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
    xs.push_new_frame();
    let tparams = tparams(opts, scope, scopes, tbls, xs, f.tparams.as_ref());
    let this_param = function_type_this_param(opts, scope, scopes, tbls, xs, &f.params.this);
    let params = function_type_params(opts, scope, scopes, tbls, xs, &f.params.params);
    let rest_param = function_type_rest_param(opts, scope, scopes, tbls, xs, &f.params.rest);
    let (return_, type_guard) = return_annot(is_method, opts, scope, scopes, tbls, xs, &f.return_);
    let effect_ = convert_effect(opts, &f.effect, None, None);
    xs.pop_frame();

    FunSig {
        tparams,
        params,
        rest_param,
        this_param,
        return_,
        type_guard,
        effect_,
        strictness_kind: tbls.strictness_kind,
    }
}

fn function_component_type_param<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    t: &ast::types::Type<Loc, Loc>,
    optional: bool,
) -> Parsed<'arena, 'ast> {
    let t = annot(opts, scope, scopes, tbls, xs, t);
    if optional {
        Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))))
    } else {
        t
    }
}

fn function_type_params<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    params: &[ast::types::function::Param<Loc, Loc>],
) -> Vec<FunParam<Parsed<'arena, 'ast>>> {
    params
        .iter()
        .map(|p| {
            let (id, annot_t, optional) =
                flow_parser::ast_utils::function_type_param_parts(&p.param);
            let name = id.map(|id| id_name(id).clone());
            let t = function_component_type_param(opts, scope, scopes, tbls, xs, annot_t, optional);
            FunParam { name, t }
        })
        .collect()
}

fn function_type_rest_param<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    rest: &Option<ast::types::function::RestParam<Loc, Loc>>,
) -> Option<FunRestParam<LocNode<'arena>, Parsed<'arena, 'ast>>> {
    rest.as_ref().map(|rp| {
        let loc = tbls.push_loc(rp.loc.dupe());
        let (id, annot_t, _optional) =
            flow_parser::ast_utils::function_type_param_parts(&rp.argument.param);
        let name = id.map(|id| id_name(id).clone());
        let t = annot(opts, scope, scopes, tbls, xs, annot_t);
        FunRestParam { name, loc, t }
    })
}

fn function_type_this_param<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    this: &Option<ast::types::function::ThisParam<Loc, Loc>>,
) -> Option<Parsed<'arena, 'ast>> {
    this.as_ref()
        .map(|tp| annot(opts, scope, scopes, tbls, xs, &tp.annot.annotation))
}

fn component_type<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    tparams: &Option<ast::types::TypeParams<Loc, Loc>>,
    params: &ast::types::component_params::Params<Loc, Loc>,
    renders: &ast::types::ComponentRendersAnnotation<Loc, Loc>,
) -> ComponentSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
    xs.push_new_frame();
    let tparams = self::tparams(opts, scope, scopes, tbls, xs, tparams.as_ref());
    let params_loc = tbls.push_loc(params.loc.dupe());
    let mut parsed_params = Vec::new();
    for ast::types::component_params::Param {
        loc: _,
        name,
        annot,
        optional,
    } in params.params.iter()
    {
        let (name, name_loc) = match name {
            ast::statement::component_params::ParamName::Identifier(id) => {
                let IdentifierInner { loc, name, .. } = id.deref();
                (name, loc)
            }
            ast::statement::component_params::ParamName::StringLiteral((
                loc,
                ast::StringLiteral { value: name, .. },
            )) => (name, loc),
        };
        let name_loc = tbls.push_loc(name_loc.dupe());
        let t = function_component_type_param(
            opts,
            scope,
            scopes,
            tbls,
            xs,
            &annot.annotation,
            *optional,
        );
        parsed_params.push(ComponentParam {
            name: name.clone(),
            name_loc,
            t,
        });
    }
    let rest_param = params.rest.as_ref().map(|rest| {
        let t = annot(opts, scope, scopes, tbls, xs, &rest.annot);
        ComponentRestParam { t }
    });

    let renders = match renders {
        ast::types::ComponentRendersAnnotation::AvailableRenders(loc, r) => {
            let loc = tbls.push_loc(loc.dupe());
            let arg = annot(opts, scope, scopes, tbls, xs, &r.argument);
            Parsed::Annot(Box::new(ParsedAnnot::Renders(Box::new(AnnotRenders {
                loc,
                arg,
                variant: r.variant,
            }))))
        }
        ast::types::ComponentRendersAnnotation::MissingRenders(loc) => {
            let loc = tbls.push_loc(loc.dupe());
            Parsed::Annot(Box::new(ParsedAnnot::ComponentMissingRenders(Box::new(
                loc,
            ))))
        }
    };
    xs.pop_frame();

    ComponentSig {
        params_loc,
        tparams,
        params: parsed_params,
        rest_param,
        renders,
        strictness_kind: tbls.strictness_kind,
    }
}

fn getter_type<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    id_loc: LocNode<'arena>,
    f: &ast::types::Function<Loc, Loc>,
) -> Accessor<LocNode<'arena>, Parsed<'arena, 'ast>> {
    Accessor::Get(Box::new((
        id_loc,
        return_annot(false, opts, scope, scopes, tbls, xs, &f.return_).0,
    )))
}

fn setter_type<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    id_loc: LocNode<'arena>,
    f: &ast::types::Function<Loc, Loc>,
) -> Accessor<LocNode<'arena>, Parsed<'arena, 'ast>> {
    if f.params.params.len() == 1
        && let Some(p) = f.params.params.first()
    {
        let (_, annot_t, optional) = flow_parser::ast_utils::function_type_param_parts(&p.param);
        let t = annot(opts, scope, scopes, tbls, xs, annot_t);
        let t = if optional {
            Parsed::Annot(Box::new(Annot::Optional(Box::new(t))))
        } else {
            t
        };
        Accessor::Set(Box::new((id_loc, t)))
    } else {
        panic!("unexpected setter")
    }
}

fn indexer<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    dict: &ast::types::object::Indexer<Loc, Loc>,
) -> ObjAnnotDict<Parsed<'arena, 'ast>> {
    let name = dict.id.as_ref().map(|id| id_name(id).clone());
    let key = annot(opts, scope, scopes, tbls, xs, &dict.key);
    let value = indexer_value(opts, scope, scopes, tbls, xs, dict);
    let variance = dict.variance.as_ref().map(|v| (v.loc.dupe(), v.clone()));
    ObjAnnotDict {
        name,
        polarity: polarity(variance),
        key,
        value,
    }
}

/// The value type of a bracketed object-type member, whichever way its key is
/// read.
fn indexer_value<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    dict: &ast::types::object::Indexer<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    let value = annot(opts, scope, scopes, tbls, xs, &dict.value);
    if dict.optional {
        Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(value))))
    } else {
        value
    }
}

/// The type of a class field: `T | void` for an optional field (`x?: T`), which holds
/// `undefined` until it is assigned.
fn optional_class_field<'arena, 'ast>(
    optional: bool,
    t: Parsed<'arena, 'ast>,
) -> Parsed<'arena, 'ast> {
    if optional {
        Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))))
    } else {
        t
    }
}

fn optional_method_as_field<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    is_static: bool,
    id_loc: &Loc,
    fn_loc: &Loc,
    func: &ast::types::Function<Loc, Loc>,
) -> (LocNode<'arena>, Parsed<'arena, 'ast>) {
    // Optional method: treat as optional field with function type.
    // We must push fn_loc before id_loc because they share the same
    // start position and fn_loc has a larger end, so fn_loc must come
    // first in Packed_locs.compare_locs order. We then call
    // function_type (not annot) to avoid pushing all inner locs
    // before id_loc.
    let fn_loc = tbls.push_loc(fn_loc.dupe());
    let id_loc = tbls.push_loc(id_loc.dupe());
    let def = function_type(true, opts, scope, scopes, tbls, xs, func);
    let t = Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(Parsed::Annot(
        Box::new(ParsedAnnot::MethodAnnot(Box::new((fn_loc, is_static, def)))),
    )))));
    (id_loc, t)
}

fn object_type<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    loc: LocNode<'arena>,
    o: &ast::types::Object<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    fn add_method<'arena, 'ast>(
        opts: &TypeSigOptions,
        scope: ScopeId,
        scopes: &mut scope::Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        xs: &mut tparam_stack::TParamStack,
        acc: &mut obj_annot_acc::ObjAnnotAcc<'arena, 'ast>,
        id_loc: &Loc,
        name: &FlowSmolStr,
        t: &ast::types::Type<Loc, Loc>,
    ) {
        if let TypeInner::Function { loc, inner, .. } = t.deref() {
            let fn_loc = tbls.push_loc(loc.dupe());
            let id_loc = tbls.push_loc(id_loc.dupe());
            let def = function_type(true, opts, scope, scopes, tbls, xs, inner.as_ref());
            acc.add_method(name.clone(), id_loc, fn_loc, def);
        } else {
            unreachable!("Unexpected method")
        }
    }

    fn object_prop<'arena, 'ast>(
        opts: &TypeSigOptions,
        scope: ScopeId,
        scopes: &mut scope::Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        xs: &mut tparam_stack::TParamStack,
        acc: &mut obj_annot_acc::ObjAnnotAcc<'arena, 'ast>,
        p: &ast::types::object::NormalProperty<Loc, Loc>,
    ) {
        use ast::expression::object::Key;
        use ast::types::object as O;

        fn obj_add_computed_prop<'arena, 'ast>(
            opts: &TypeSigOptions,
            scope: ScopeId,
            scopes: &mut scope::Scopes<'arena, 'ast>,
            tbls: &mut Tables<'arena, 'ast>,
            xs: &mut tparam_stack::TParamStack,
            acc: &mut obj_annot_acc::ObjAnnotAcc<'arena, 'ast>,
            p: &ast::types::object::NormalProperty<Loc, Loc>,
            t: &ast::types::Type<Loc, Loc>,
            build_key: impl FnOnce(
                &mut Tables<'arena, 'ast>,
                &mut scope::Scopes<'arena, 'ast>,
            ) -> (Parsed<'arena, 'ast>, LocNode<'arena>),
        ) {
            if p.method {
                match t.deref() {
                    TypeInner::Function {
                        loc: fn_loc, inner, ..
                    } => {
                        let fn_loc = tbls.push_loc(fn_loc.dupe());
                        let (key_ref, ref_loc) = build_key(tbls, scopes);
                        let def =
                            function_type(true, opts, scope, scopes, tbls, xs, inner.as_ref());
                        if p.optional {
                            let t = Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(
                                Parsed::Annot(Box::new(ParsedAnnot::MethodAnnot(Box::new((
                                    fn_loc, p.static_, def,
                                ))))),
                            ))));
                            let polarity_val =
                                polarity(p.variance.as_ref().map(|v| (v.loc.dupe(), v.clone())));
                            acc.add_computed_field(key_ref, ref_loc, polarity_val, t);
                        } else {
                            acc.add_computed_method(key_ref, ref_loc, fn_loc, def);
                        }
                    }
                    _ => unreachable!("unexpected method type"),
                }
            } else {
                let (key_ref, ref_loc) = build_key(tbls, scopes);
                let mut t = annot(opts, scope, scopes, tbls, xs, t);
                if p.optional {
                    t = Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))));
                }
                let polarity_val = polarity(p.variance.as_ref().map(|v| (v.loc.dupe(), v.clone())));
                acc.add_computed_field(key_ref, ref_loc, polarity_val, t);
            }
        }

        match &p.value {
            O::PropertyValue::Init(Some(t)) => {
                match resolve_prop_key(&p.key) {
                    ResolvedPropKey::Named(name, id_loc) => {
                        if p.method {
                            if p.optional {
                                if let TypeInner::Function {
                                    loc: fn_loc, inner, ..
                                } = t.deref()
                                {
                                    let (id_loc, t) = optional_method_as_field(
                                        opts,
                                        scope,
                                        scopes,
                                        tbls,
                                        xs,
                                        p.static_,
                                        &id_loc,
                                        fn_loc,
                                        inner.as_ref(),
                                    );
                                    let polarity_val = polarity(
                                        p.variance.as_ref().map(|v| (v.loc.dupe(), v.clone())),
                                    );
                                    acc.add_field(name, id_loc, polarity_val, t);
                                } else {
                                    unreachable!("unexpected optional method type")
                                }
                            } else {
                                add_method(opts, scope, scopes, tbls, xs, acc, &id_loc, &name, t);
                            }
                        } else {
                            let id_loc = tbls.push_loc(id_loc);
                            let value_loc = tbls.push_loc(t.loc().dupe());
                            let mut t = annot(opts, scope, scopes, tbls, xs, t);

                            if name.as_str() == "__proto__" && !p.optional && p.variance.is_none() {
                                acc.add_proto((value_loc, t));
                            } else {
                                if p.optional {
                                    t = Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))));
                                }
                                let polarity_val = polarity(
                                    p.variance.as_ref().map(|v| (v.loc.dupe(), v.clone())),
                                );
                                acc.add_field(name, id_loc, polarity_val, t);
                            }
                        }
                    }
                    ResolvedPropKey::ComputedRef { expr_loc, ref_name } => {
                        obj_add_computed_prop(
                            opts,
                            scope,
                            scopes,
                            tbls,
                            xs,
                            acc,
                            p,
                            t,
                            |tbls, _scopes| {
                                // Thunk defers loc pushes so callers can push fn_loc first
                                // for methods, satisfying Packed_locs ascending-order
                                // requirement.
                                let ref_loc = tbls.push_loc(expr_loc);
                                (val_ref(false, scope, ref_loc.dupe(), ref_name), ref_loc)
                            },
                        );
                    }
                    ResolvedPropKey::ComputedExpr { expr } => {
                        obj_add_computed_prop(
                            opts,
                            scope,
                            scopes,
                            tbls,
                            xs,
                            acc,
                            p,
                            t,
                            |tbls, _scopes| {
                                let ref_loc = tbls.push_loc(expr.loc().dupe());
                                let key_ref = expression_for_computed_key(scope, tbls, expr);
                                (key_ref, ref_loc)
                            },
                        );
                    }
                    ResolvedPropKey::ComputedError { loc } => {
                        obj_add_computed_prop(
                            opts,
                            scope,
                            scopes,
                            tbls,
                            xs,
                            acc,
                            p,
                            t,
                            |tbls, _scopes| {
                                let loc = tbls.push_loc(loc);
                                let key_ref = Parsed::Err(
                                    loc.dupe(),
                                    Errno::SigError(Box::new(
                                        signature_error::SignatureError::UnexpectedObjectKey(
                                            loc.dupe(),
                                            loc.dupe(),
                                        ),
                                    )),
                                );
                                (key_ref, loc)
                            },
                        );
                    }
                }
            }
            O::PropertyValue::Init(None) => {}
            O::PropertyValue::Get(_, fn_expr) => {
                if let Key::Identifier(id) = &p.key {
                    let id_loc = tbls.push_loc(id.loc.dupe());
                    let name = id.name.dupe();
                    let getter = getter_type(opts, scope, scopes, tbls, xs, id_loc, fn_expr);
                    acc.add_accessor(name, getter);
                }
                // unsupported getter syntax
            }
            O::PropertyValue::Set(_, fn_expr) => {
                if let Key::Identifier(id) = &p.key {
                    let id_loc = tbls.push_loc(id.loc.dupe());
                    let name = id.name.dupe();
                    let setter = setter_type(opts, scope, scopes, tbls, xs, id_loc, fn_expr);
                    acc.add_accessor(name, setter);
                }
                // unsupported setter syntax
            }
        }
    }

    fn object_mapped_type<'arena, 'ast>(
        opts: &TypeSigOptions,
        scope: ScopeId,
        scopes: &mut scope::Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        xs: &mut tparam_stack::TParamStack,
        p: &ast::types::object::MappedType<Loc, Loc>,
    ) -> Parsed<'arena, 'ast> {
        let ast::types::object::MappedType {
            loc,
            key_tparam,
            source_type,
            prop_type,
            variance,
            variance_op,
            optional: _,
            name_type,
            comments: _,
        } = p;
        let loc = tbls.push_loc(loc.dupe());

        if name_type.is_some() && !opts.tslib_syntax {
            // The `as` key-remapping clause is gated on `experimental.tslib_syntax`.
            // type_annotation errors and falls back to Any in this case; mirror that here so
            // a gated mapped type defined in an imported file does not silently produce a
            // different result than one defined locally.
            return Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc))));
        }

        // The source type does not have the key_tparam in scope, but we need to parse locs in syntax
        // order or we will violate type sig invariants.
        let key_loc = tbls.push_loc(p.key_tparam.name.loc.dupe());
        let key_name = key_tparam.name.name.dupe();

        let (source_type, inline_keyof) = match source_type.deref() {
            TypeInner::Keyof { inner, .. } => {
                (annot(opts, scope, scopes, tbls, xs, &inner.argument), true)
            }
            _ => (annot(opts, scope, scopes, tbls, xs, source_type), false),
        };

        let key_tparam = TParam {
            name_loc: key_loc,
            name: key_name.clone(),
            polarity: None,
            bound: None,
            default: None,
            is_const: false,
        };

        xs.push_new_frame();
        xs.insert(key_name);
        // `as Name` appears before `: Prop` in source, so parse name_type first to keep loc parsing
        // in syntax order (the type-sig invariant).
        let name_type = name_type
            .as_ref()
            .map(|nt| annot(opts, scope, scopes, tbls, xs, nt));
        let property_type = annot(opts, scope, scopes, tbls, xs, prop_type);
        xs.pop_frame();

        Parsed::Annot(Box::new(ParsedAnnot::MappedTypeAnnot(Box::new(
            AnnotMappedTypeAnnot {
                loc,
                source_type,
                property_type,
                name_type,
                key_tparam,
                variance: polarity(variance.as_ref().map(|v| (v.loc.dupe(), v.clone()))),
                variance_op: *variance_op,
                optional: p.optional.clone(),
                inline_keyof,
                strictness_kind: tbls.strictness_kind,
            },
        ))))
    }

    use ast::types::object as O;
    use obj_annot_acc::ObjAnnotAcc;

    // Mapped types do not allow extra properties yet. We syntactically match
    // on objects with only a mapped type property and make other objects
    // including a mapped type `any`.
    if let [O::Property::MappedType(p)] = &o.properties[..] {
        return object_mapped_type(opts, scope, scopes, tbls, xs, p);
    }

    // An `ObjAnnot` has a call slot but no construct slot, so an object type
    // carrying `new(): T` is lowered to an inline interface instead — the same
    // desugar [ConstructorType] gets in [annot]. A spread has no interface
    // equivalent ([interface_props] drops it), so those keep the object lowering
    // and the `new` member stays an ordinary method. Mirrors the routing in
    // [type_annotation.rs] [convert]'s `Object` case, including its gate: TS and
    // library definitions spell a construct signature this way, whereas Flow
    // source has always read `{new(): T}` as a method named `new`. Consumers in
    // Flow files read the resulting annotation, so they are unaffected by the
    // gate.
    let has_construct_sig = (tbls.strictness_kind.is_typescript_loose() || opts.for_builtins)
        && o.properties.iter().any(|p| match p {
            O::Property::NormalProperty(p) => is_construct_signature(p),
            _ => false,
        });
    let has_spread = o
        .properties
        .iter()
        .any(|p| matches!(p, O::Property::SpreadProperty(_)));
    if has_construct_sig && !has_spread {
        let mut acc = interface_acc::InterfaceAcc::empty();
        interface_props(opts, scope, scopes, tbls, xs, &o.properties, &mut acc);
        let def = acc.interface_def(false, vec![], tbls.strictness_kind);
        return Parsed::Annot(Box::new(ParsedAnnot::InlineInterface(Box::new((loc, def)))));
    }

    let exact = o.exact || !o.inexact;
    let mut acc = ObjAnnotAcc::empty();

    for objprop in o.properties.iter() {
        match objprop {
            O::Property::NormalProperty(p) => {
                object_prop(opts, scope, scopes, tbls, xs, &mut acc, p);
            }
            O::Property::SpreadProperty(p) => {
                let t = annot(opts, scope, scopes, tbls, xs, &p.argument);
                acc.add_spread(t);
            }
            O::Property::Indexer(p) => {
                // `{[K]: V}` is ambiguous between a computed key and an index
                // signature, and the way the key is written decides (see
                // `object_type_key`). The labeled form `[label: K]` is an index
                // signature outright.
                let key_form = if p.id.is_some() {
                    ObjectTypeKeyForm::IndexSignature
                } else {
                    object_type_key::object_type_key_form(&p.key)
                };
                let polarity_val = polarity(p.variance.as_ref().map(|v| (v.loc.dupe(), v.clone())));
                match key_form {
                    ObjectTypeKeyForm::Literal(name) => {
                        let key_loc = tbls.push_loc(p.key.loc().dupe());
                        let value = indexer_value(opts, scope, scopes, tbls, xs, p);
                        acc.add_field(name, key_loc, polarity_val, value);
                    }
                    ObjectTypeKeyForm::Name { generic, head, .. }
                        if !xs.contains(&head.name)
                            && scope::name_heads_computed_key(scopes, scope, &head.name) =>
                    {
                        let key_loc = tbls.push_loc(p.key.loc().dupe());
                        let key = value_object_type_key(scope, tbls, &generic.id);
                        let value = indexer_value(opts, scope, scopes, tbls, xs, p);
                        acc.add_computed_field(key, key_loc, polarity_val, value);
                    }
                    ObjectTypeKeyForm::Name { .. } | ObjectTypeKeyForm::IndexSignature => {
                        let dict = indexer(opts, scope, scopes, tbls, xs, p);
                        acc.add_dict(dict);
                    }
                }
            }
            O::Property::CallProperty(p) => {
                let fn_loc = tbls.push_loc(p.value.0.dupe());
                let def = function_type(false, opts, scope, scopes, tbls, xs, &p.value.1);
                let t = Parsed::Annot(Box::new(ParsedAnnot::FunAnnot(Box::new((fn_loc, def)))));
                acc.add_call(t);
            }
            O::Property::InternalSlot(p) => {
                let name = id_name(&p.id);
                if name.as_str() == "call" {
                    let t = annot(opts, scope, scopes, tbls, xs, &p.value);
                    let t = if p.optional {
                        Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))))
                    } else {
                        t
                    };
                    acc.add_call(t);
                }
                // unsupported slot name
            }
            O::Property::MappedType(_) => {
                break;
            }
            O::Property::PrivateField(_) => {}
        }
    }

    acc.object_type(loc, exact, tbls.strictness_kind)
}

/// A construct signature uses the keyword `new` as a syntactic marker — it's not
/// a property whose name happens to be "new". A quoted `"new"()` or an optional
/// `new?()` stays an ordinary property. Statics are excluded because construct
/// sigs live on the instance side only.
///
/// Mirror predicate in [type_annotation.rs] [is_construct_signature].
fn is_construct_signature(p: &ast::types::object::NormalProperty<Loc, Loc>) -> bool {
    p.method
        && !p.static_
        && !p.optional
        && matches!(
            &p.key,
            ast::expression::object::Key::Identifier(id) if id.name == "new"
        )
}

fn interface_def<'arena, 'ast>(
    bind_this_in_body: bool,
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    extends: &[(Loc, ast::types::Generic<Loc, Loc>)],
    properties: &[ast::types::object::Property<Loc, Loc>],
) -> InterfaceSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
    use interface_acc::InterfaceAcc;

    let extends = extends
        .iter()
        .map(|(loc, g)| {
            let loc = tbls.push_loc(loc.dupe());
            generic(opts, scope, scopes, tbls, xs, loc, g)
        })
        .collect();

    let mut acc = InterfaceAcc::empty();
    let this_name = FlowSmolStr::new("this");
    let added_this = bind_this_in_body && {
        xs.insert(this_name.dupe());
        true
    };
    interface_props(opts, scope, scopes, tbls, xs, properties, &mut acc);
    if added_this {
        let _removed = xs.remove_all(&this_name);
    }
    acc.interface_def(false, extends, tbls.strictness_kind)
}

fn interface_props<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    properties: &[ast::types::object::Property<Loc, Loc>],
    acc: &mut interface_acc::InterfaceAcc<'arena, 'ast>,
) {
    fn interface_prop<'arena, 'ast>(
        opts: &TypeSigOptions,
        scope: ScopeId,
        scopes: &mut scope::Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        xs: &mut tparam_stack::TParamStack,
        acc: &mut interface_acc::InterfaceAcc<'arena, 'ast>,
        p: &ast::types::object::NormalProperty<Loc, Loc>,
    ) {
        use ast::types::object as O;

        fn add_named_prop<'arena, 'ast>(
            opts: &TypeSigOptions,
            scope: ScopeId,
            scopes: &mut scope::Scopes<'arena, 'ast>,
            tbls: &mut Tables<'arena, 'ast>,
            xs: &mut tparam_stack::TParamStack,
            acc: &mut interface_acc::InterfaceAcc<'arena, 'ast>,
            p: &ast::types::object::NormalProperty<Loc, Loc>,
            name: FlowSmolStr,
            id_loc: Loc,
        ) {
            match (p.method, &p.value) {
                (true, O::PropertyValue::Init(Some(ty))) => {
                    if let TypeInner::Function {
                        loc: fn_loc, inner, ..
                    } = ty.deref()
                    {
                        if p.optional {
                            let (id_loc, t) = optional_method_as_field(
                                opts,
                                scope,
                                scopes,
                                tbls,
                                xs,
                                p.static_,
                                &id_loc,
                                fn_loc,
                                inner.as_ref(),
                            );
                            let polarity_val =
                                polarity(p.variance.as_ref().map(|v| (v.loc.dupe(), v.clone())));
                            acc.add_field(name, id_loc, polarity_val, t);
                        } else {
                            let fn_loc = tbls.push_loc(p.loc.dupe());
                            let id_loc = tbls.push_loc(id_loc);
                            if is_construct_signature(p) {
                                let def = function_type(
                                    false,
                                    opts,
                                    scope,
                                    scopes,
                                    tbls,
                                    xs,
                                    inner.as_ref(),
                                );
                                acc.append_construct(Parsed::Annot(Box::new(
                                    ParsedAnnot::FunAnnot(Box::new((fn_loc, def))),
                                )));
                            } else {
                                let def = function_type(
                                    true,
                                    opts,
                                    scope,
                                    scopes,
                                    tbls,
                                    xs,
                                    inner.as_ref(),
                                );
                                acc.append_method(name, id_loc, fn_loc, def);
                            }
                        }
                    }
                    // unexpected non-function method otherwise
                }
                (false, O::PropertyValue::Init(Some(t))) => {
                    let id_loc = tbls.push_loc(id_loc);
                    let mut t = annot(opts, scope, scopes, tbls, xs, t);
                    if p.optional {
                        t = Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))));
                    }
                    let polarity_val =
                        polarity(p.variance.as_ref().map(|v| (v.loc.dupe(), v.clone())));
                    acc.add_field(name, id_loc, polarity_val, t);
                }
                (_, O::PropertyValue::Init(None)) => {}
                (_, O::PropertyValue::Get(_, fn_expr)) => {
                    let id_loc = tbls.push_loc(id_loc);
                    let getter = getter_type(opts, scope, scopes, tbls, xs, id_loc, fn_expr);
                    acc.add_accessor(name, getter);
                }
                (_, O::PropertyValue::Set(_, fn_expr)) => {
                    let id_loc = tbls.push_loc(id_loc);
                    let setter = setter_type(opts, scope, scopes, tbls, xs, id_loc, fn_expr);
                    acc.add_accessor(name, setter);
                }
            }
        }

        fn add_computed_prop<'arena, 'ast>(
            opts: &TypeSigOptions,
            scope: ScopeId,
            scopes: &mut scope::Scopes<'arena, 'ast>,
            tbls: &mut Tables<'arena, 'ast>,
            xs: &mut tparam_stack::TParamStack,
            acc: &mut interface_acc::InterfaceAcc<'arena, 'ast>,
            p: &ast::types::object::NormalProperty<Loc, Loc>,
            build_key: impl FnOnce(
                &mut Tables<'arena, 'ast>,
                &mut scope::Scopes<'arena, 'ast>,
            ) -> (Parsed<'arena, 'ast>, LocNode<'arena>),
        ) {
            match (p.method, &p.value) {
                (true, O::PropertyValue::Init(Some(ty))) => {
                    if let TypeInner::Function {
                        loc: fn_loc, inner, ..
                    } = ty.deref()
                    {
                        let fn_loc = tbls.push_loc(fn_loc.dupe());
                        let (key_ref, ref_loc) = build_key(tbls, scopes);
                        let def =
                            function_type(true, opts, scope, scopes, tbls, xs, inner.as_ref());
                        if p.optional {
                            let t = Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(
                                Parsed::Annot(Box::new(ParsedAnnot::MethodAnnot(Box::new((
                                    fn_loc, p.static_, def,
                                ))))),
                            ))));
                            let polarity_val =
                                polarity(p.variance.as_ref().map(|v| (v.loc.dupe(), v.clone())));
                            acc.add_computed_field(key_ref, ref_loc, polarity_val, t);
                        } else {
                            acc.append_computed_method(key_ref, ref_loc, fn_loc, def);
                        }
                    }
                }
                (true, _) => {}
                (false, O::PropertyValue::Init(Some(t))) => {
                    let (key_ref, ref_loc) = build_key(tbls, scopes);
                    let mut t = annot(opts, scope, scopes, tbls, xs, t);
                    if p.optional {
                        t = Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))));
                    }
                    let polarity_val =
                        polarity(p.variance.as_ref().map(|v| (v.loc.dupe(), v.clone())));
                    acc.add_computed_field(key_ref, ref_loc, polarity_val, t);
                }
                _ => {}
            }
        }

        match resolve_prop_key(&p.key) {
            ResolvedPropKey::Named(name, id_loc) => {
                add_named_prop(opts, scope, scopes, tbls, xs, acc, p, name, id_loc);
            }
            ResolvedPropKey::ComputedRef { expr_loc, ref_name } => {
                add_computed_prop(opts, scope, scopes, tbls, xs, acc, p, |tbls, _scopes| {
                    let ref_loc = tbls.push_loc(expr_loc);
                    (val_ref(false, scope, ref_loc.dupe(), ref_name), ref_loc)
                });
            }
            ResolvedPropKey::ComputedExpr { expr } => {
                add_computed_prop(opts, scope, scopes, tbls, xs, acc, p, |tbls, _scopes| {
                    let ref_loc = tbls.push_loc(expr.loc().dupe());
                    let t = expression_for_computed_key(scope, tbls, expr);
                    (t, ref_loc)
                });
            }
            ResolvedPropKey::ComputedError { loc } => {
                add_computed_prop(opts, scope, scopes, tbls, xs, acc, p, |tbls, _scopes| {
                    let loc = tbls.push_loc(loc);
                    (
                        Parsed::Err(
                            loc.dupe(),
                            Errno::SigError(Box::new(
                                signature_error::SignatureError::UnexpectedObjectKey(
                                    loc.dupe(),
                                    loc.dupe(),
                                ),
                            )),
                        ),
                        loc,
                    )
                });
            }
        }
    }

    use ast::types::object as O;

    for objprop in properties {
        match objprop {
            O::Property::NormalProperty(p) => {
                interface_prop(opts, scope, scopes, tbls, xs, acc, p);
            }
            O::Property::Indexer(p) => {
                // `[K]: V` is ambiguous between a computed key and an index
                // signature, and the way the key is written decides (see
                // `object_type_key`), just as it does in an object type body.
                let key_form = if p.id.is_some() {
                    ObjectTypeKeyForm::IndexSignature
                } else {
                    object_type_key::object_type_key_form(&p.key)
                };
                let polarity_val = polarity(p.variance.as_ref().map(|v| (v.loc.dupe(), v.clone())));
                match key_form {
                    ObjectTypeKeyForm::Literal(name) => {
                        let key_loc = tbls.push_loc(p.key.loc().dupe());
                        let value = indexer_value(opts, scope, scopes, tbls, xs, p);
                        acc.add_field(name, key_loc, polarity_val, value);
                    }
                    ObjectTypeKeyForm::Name { generic, head, .. }
                        if !xs.contains(&head.name)
                            && scope::name_heads_computed_key(scopes, scope, &head.name) =>
                    {
                        let key_loc = tbls.push_loc(p.key.loc().dupe());
                        let key = value_object_type_key(scope, tbls, &generic.id);
                        let value = indexer_value(opts, scope, scopes, tbls, xs, p);
                        acc.add_computed_field(key, key_loc, polarity_val, value);
                    }
                    ObjectTypeKeyForm::Name { .. } | ObjectTypeKeyForm::IndexSignature => {
                        let i = indexer(opts, scope, scopes, tbls, xs, p);
                        acc.add_indexer(i);
                    }
                }
            }
            O::Property::CallProperty(p) => {
                let fn_loc = tbls.push_loc(p.value.0.dupe());
                let def = function_type(false, opts, scope, scopes, tbls, xs, &p.value.1);
                let t = Parsed::Annot(Box::new(ParsedAnnot::FunAnnot(Box::new((fn_loc, def)))));
                acc.append_call(t);
            }
            O::Property::InternalSlot(p) => {
                let name = id_name(&p.id);
                if name.as_str() == "call" {
                    let t = annot(opts, scope, scopes, tbls, xs, &p.value);
                    let t = if p.optional {
                        Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))))
                    } else {
                        t
                    };
                    acc.append_call(t);
                }
                // unsupported slot name
            }
            O::Property::SpreadProperty(_)
            | O::Property::MappedType(_)
            | O::Property::PrivateField(_) => {
                // no spread, mapped types, or private fields in interface
            }
        }
    }
}

fn literal_expr_to_annot<'arena, 'ast>(
    loc: LocNode<'arena>,
    expr: &ast::expression::Expression<Loc, Loc>,
) -> Option<Parsed<'arena, 'ast>> {
    use ast::expression::ExpressionInner;
    match expr.deref() {
        ExpressionInner::StringLiteral { inner, .. } => Some(Parsed::Annot(Box::new(
            ParsedAnnot::SingletonString(Box::new((loc, inner.value.dupe()))),
        ))),
        ExpressionInner::NumberLiteral { inner, .. } => Some(Parsed::Annot(Box::new(
            ParsedAnnot::SingletonNumber(Box::new((loc, inner.value, inner.raw.dupe()))),
        ))),
        ExpressionInner::BigIntLiteral { inner, .. } => Some(Parsed::Annot(Box::new(
            ParsedAnnot::SingletonBigInt(Box::new((loc, inner.value, inner.raw.dupe()))),
        ))),
        ExpressionInner::BooleanLiteral { inner, .. } => Some(Parsed::Annot(Box::new(
            ParsedAnnot::SingletonBoolean(Box::new((loc, inner.value))),
        ))),
        ExpressionInner::Unary { inner, .. }
            if inner.operator == ast::expression::UnaryOperator::Minus =>
        {
            match inner.argument.deref() {
                ExpressionInner::NumberLiteral { inner: num, .. } => {
                    Some(Parsed::Annot(Box::new(ParsedAnnot::SingletonNumber(
                        Box::new((loc, -num.value, FlowSmolStr::new(format!("-{}", num.raw)))),
                    ))))
                }
                ExpressionInner::BigIntLiteral {
                    inner: bigint_inner,
                    ..
                } => {
                    let negated = bigint_inner.value.map(|i| -i);
                    Some(Parsed::Annot(Box::new(ParsedAnnot::SingletonBigInt(
                        Box::new((
                            loc,
                            negated,
                            FlowSmolStr::new(format!("-{}", bigint_inner.raw)),
                        )),
                    ))))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn declare_class_props<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    class_tparam_names: &[FlowSmolStr],
    properties: &[ast::types::object::Property<Loc, Loc>],
    acc: &mut declare_class_acc::DeclareClassAcc<'arena, 'ast>,
) {
    fn declare_class_prop<'arena, 'ast>(
        opts: &TypeSigOptions,
        scope: ScopeId,
        scopes: &mut scope::Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        xs: &mut tparam_stack::TParamStack,
        acc: &mut declare_class_acc::DeclareClassAcc<'arena, 'ast>,
        p: &ast::types::object::NormalProperty<Loc, Loc>,
    ) {
        use ast::types::object as O;

        // Static abstract is parsed but not enforced — TS-style static abstract
        // requires checking [typeof Subclass] at constructor sites and we don't
        // wire that up. Filter [static] OUT of the abstract obligation set so
        // a `declare abstract class { static abstract foo }` does not falsely
        // require subclasses to provide an INSTANCE foo.
        let is_abstract = p.abstract_ && opts.tslib_syntax && !p.static_;
        // Skip private properties entirely
        if matches!(
            &p.ts_accessibility,
            Some(ast::class::ts_accessibility::TSAccessibility {
                kind: ast::class::ts_accessibility::Kind::Private,
                ..
            })
        ) {
            return;
        }

        fn mark_abstract<'arena, 'ast>(
            is_abstract: bool,
            name: &FlowSmolStr,
            acc: &mut declare_class_acc::DeclareClassAcc<'arena, 'ast>,
        ) {
            if is_abstract {
                acc.add_abstract_name(name.dupe());
            }
        }

        fn add_named_prop<'arena, 'ast>(
            opts: &TypeSigOptions,
            scope: ScopeId,
            scopes: &mut scope::Scopes<'arena, 'ast>,
            tbls: &mut Tables<'arena, 'ast>,
            xs: &mut tparam_stack::TParamStack,
            acc: &mut declare_class_acc::DeclareClassAcc<'arena, 'ast>,
            p: &ast::types::object::NormalProperty<Loc, Loc>,
            is_abstract: bool,
            name: FlowSmolStr,
            id_loc: Loc,
        ) {
            match (p.method, &p.value) {
                (true, O::PropertyValue::Init(Some(ty))) => {
                    if let TypeInner::Function {
                        loc: fn_loc, inner, ..
                    } = ty.deref()
                    {
                        if p.optional {
                            let (id_loc, t) = optional_method_as_field(
                                opts,
                                scope,
                                scopes,
                                tbls,
                                xs,
                                p.static_,
                                &id_loc,
                                fn_loc,
                                inner.as_ref(),
                            );
                            let polarity_val =
                                polarity(p.variance.as_ref().map(|v| (v.loc.dupe(), v.clone())));
                            if p.proto {
                                acc.add_proto_field(name.dupe(), id_loc, polarity_val, t);
                            } else {
                                acc.add_field(p.static_, name.dupe(), id_loc, polarity_val, t);
                            }
                            mark_abstract(is_abstract, &name, acc);
                        } else {
                            let fn_loc = tbls.push_loc(fn_loc.dupe());
                            let id_loc = tbls.push_loc(id_loc);
                            let def =
                                function_type(true, opts, scope, scopes, tbls, xs, inner.as_ref());
                            acc.append_method(p.static_, name.dupe(), id_loc, fn_loc, def);
                            mark_abstract(is_abstract, &name, acc);
                        }
                    }
                    // unexpected non-function method otherwise
                }
                (false, O::PropertyValue::Init(Some(t))) => {
                    let id_loc = tbls.push_loc(id_loc);
                    let mut t = annot(opts, scope, scopes, tbls, xs, t);
                    if p.optional {
                        t = Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))));
                    }
                    let polarity_val =
                        polarity(p.variance.as_ref().map(|v| (v.loc.dupe(), v.clone())));
                    if p.proto {
                        acc.add_proto_field(name.dupe(), id_loc, polarity_val, t);
                    } else {
                        acc.add_field(p.static_, name.dupe(), id_loc, polarity_val, t);
                    }
                    mark_abstract(is_abstract, &name, acc);
                }
                (_, O::PropertyValue::Init(None)) => {
                    if let Some(ref init_expr) = p.init {
                        let id_loc = tbls.push_loc(id_loc);
                        if let Some(mut t) = literal_expr_to_annot(id_loc.dupe(), init_expr) {
                            if p.optional {
                                t = Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))));
                            }
                            let polarity_val =
                                polarity(p.variance.as_ref().map(|v| (v.loc.dupe(), v.clone())));
                            if p.proto {
                                acc.add_proto_field(name.dupe(), id_loc, polarity_val, t);
                            } else {
                                acc.add_field(p.static_, name.dupe(), id_loc, polarity_val, t);
                            }
                            mark_abstract(is_abstract, &name, acc);
                        }
                    }
                }
                (_, O::PropertyValue::Get(_, fn_expr)) => {
                    let id_loc = tbls.push_loc(id_loc);
                    let getter = getter_type(opts, scope, scopes, tbls, xs, id_loc, fn_expr);
                    acc.add_accessor(p.static_, name.dupe(), getter);
                    mark_abstract(is_abstract, &name, acc);
                }
                (_, O::PropertyValue::Set(_, fn_expr)) => {
                    let id_loc = tbls.push_loc(id_loc);
                    let setter = setter_type(opts, scope, scopes, tbls, xs, id_loc, fn_expr);
                    acc.add_accessor(p.static_, name.dupe(), setter);
                    mark_abstract(is_abstract, &name, acc);
                }
            }
        }

        fn add_computed_prop<'arena, 'ast>(
            opts: &TypeSigOptions,
            scope: ScopeId,
            scopes: &mut scope::Scopes<'arena, 'ast>,
            tbls: &mut Tables<'arena, 'ast>,
            xs: &mut tparam_stack::TParamStack,
            acc: &mut declare_class_acc::DeclareClassAcc<'arena, 'ast>,
            p: &ast::types::object::NormalProperty<Loc, Loc>,
            build_key: impl FnOnce(
                &mut Tables<'arena, 'ast>,
                &mut scope::Scopes<'arena, 'ast>,
            ) -> (Parsed<'arena, 'ast>, LocNode<'arena>),
        ) {
            match (p.method, &p.value) {
                (true, O::PropertyValue::Init(Some(ty))) => {
                    if let TypeInner::Function {
                        loc: fn_loc, inner, ..
                    } = ty.deref()
                    {
                        let fn_loc = tbls.push_loc(fn_loc.dupe());
                        let (key_ref, ref_loc) = build_key(tbls, scopes);
                        let def =
                            function_type(true, opts, scope, scopes, tbls, xs, inner.as_ref());
                        if p.optional {
                            let t = Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(
                                Parsed::Annot(Box::new(ParsedAnnot::MethodAnnot(Box::new((
                                    fn_loc, p.static_, def,
                                ))))),
                            ))));
                            let polarity_val =
                                polarity(p.variance.as_ref().map(|v| (v.loc.dupe(), v.clone())));
                            acc.add_computed_field(
                                p.static_,
                                p.proto,
                                key_ref,
                                ref_loc,
                                polarity_val,
                                t,
                            );
                        } else {
                            acc.append_computed_method(p.static_, key_ref, ref_loc, fn_loc, def);
                        }
                    }
                }
                (true, _) => {}
                (false, O::PropertyValue::Init(Some(t))) => {
                    let (key_ref, ref_loc) = build_key(tbls, scopes);
                    let mut t = annot(opts, scope, scopes, tbls, xs, t);
                    if p.optional {
                        t = Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))));
                    }
                    let polarity_val =
                        polarity(p.variance.as_ref().map(|v| (v.loc.dupe(), v.clone())));
                    acc.add_computed_field(p.static_, p.proto, key_ref, ref_loc, polarity_val, t);
                }
                (false, O::PropertyValue::Init(None)) => {
                    if let Some(ref init_expr) = p.init {
                        let (key_ref, ref_loc) = build_key(tbls, scopes);
                        if let Some(mut t) = literal_expr_to_annot(ref_loc.dupe(), init_expr) {
                            if p.optional {
                                t = Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))));
                            }
                            let polarity_val =
                                polarity(p.variance.as_ref().map(|v| (v.loc.dupe(), v.clone())));
                            acc.add_computed_field(
                                p.static_,
                                p.proto,
                                key_ref,
                                ref_loc,
                                polarity_val,
                                t,
                            );
                        }
                    }
                }
                _ => {}
            }
        }

        match resolve_prop_key(&p.key) {
            ResolvedPropKey::Named(name, id_loc) => {
                add_named_prop(
                    opts,
                    scope,
                    scopes,
                    tbls,
                    xs,
                    acc,
                    p,
                    is_abstract,
                    name,
                    id_loc,
                );
            }
            ResolvedPropKey::ComputedRef { expr_loc, ref_name } => {
                add_computed_prop(opts, scope, scopes, tbls, xs, acc, p, |tbls, _scopes| {
                    let ref_loc = tbls.push_loc(expr_loc);
                    (val_ref(false, scope, ref_loc.dupe(), ref_name), ref_loc)
                });
            }
            ResolvedPropKey::ComputedExpr { expr } => {
                add_computed_prop(opts, scope, scopes, tbls, xs, acc, p, |tbls, _scopes| {
                    let ref_loc = tbls.push_loc(expr.loc().dupe());
                    let t = expression_for_computed_key(scope, tbls, expr);
                    (t, ref_loc)
                });
            }
            ResolvedPropKey::ComputedError { loc } => {
                add_computed_prop(opts, scope, scopes, tbls, xs, acc, p, |tbls, _scopes| {
                    let loc = tbls.push_loc(loc);
                    (
                        Parsed::Err(
                            loc.dupe(),
                            Errno::SigError(Box::new(
                                signature_error::SignatureError::UnexpectedObjectKey(
                                    loc.dupe(),
                                    loc.dupe(),
                                ),
                            )),
                        ),
                        loc,
                    )
                });
            }
        }
    }

    use ast::types::object as O;

    for objprop in properties {
        // Static members cannot reference the type parameters of the class.
        let static_ = match objprop {
            O::Property::NormalProperty(p) => p.static_,
            O::Property::Indexer(p) => p.static_,
            O::Property::CallProperty(p) => p.static_,
            O::Property::InternalSlot(p) => p.static_,
            O::Property::SpreadProperty(_)
            | O::Property::MappedType(_)
            | O::Property::PrivateField(_) => false,
        };
        for name in class_tparam_names {
            if static_ {
                xs.insert_uninitialized(name.dupe());
            } else {
                xs.initialize(name);
            }
        }
        match objprop {
            O::Property::NormalProperty(p) => {
                declare_class_prop(opts, scope, scopes, tbls, xs, acc, p);
            }
            O::Property::Indexer(p) => {
                // `[K]: V` is ambiguous between a computed key and an index
                // signature, and the way the key is written decides (see
                // `object_type_key`), just as it does in an object type body.
                let key_form = if p.id.is_some() {
                    ObjectTypeKeyForm::IndexSignature
                } else {
                    object_type_key::object_type_key_form(&p.key)
                };
                let polarity_val = polarity(p.variance.as_ref().map(|v| (v.loc.dupe(), v.clone())));
                match key_form {
                    ObjectTypeKeyForm::Literal(name) => {
                        let key_loc = tbls.push_loc(p.key.loc().dupe());
                        let value = indexer_value(opts, scope, scopes, tbls, xs, p);
                        acc.add_field(p.static_, name, key_loc, polarity_val, value);
                    }
                    ObjectTypeKeyForm::Name { generic, head, .. }
                        if !xs.contains(&head.name)
                            && scope::name_heads_computed_key(scopes, scope, &head.name) =>
                    {
                        let key_loc = tbls.push_loc(p.key.loc().dupe());
                        let key = value_object_type_key(scope, tbls, &generic.id);
                        let value = indexer_value(opts, scope, scopes, tbls, xs, p);
                        acc.add_computed_field(p.static_, false, key, key_loc, polarity_val, value);
                    }
                    ObjectTypeKeyForm::Name { .. } | ObjectTypeKeyForm::IndexSignature => {
                        let i = indexer(opts, scope, scopes, tbls, xs, p);
                        acc.add_indexer(p.static_, i);
                    }
                }
            }
            O::Property::CallProperty(p) => {
                let fn_loc = tbls.push_loc(p.value.0.dupe());
                let def = function_type(false, opts, scope, scopes, tbls, xs, &p.value.1);
                let t = Parsed::Annot(Box::new(ParsedAnnot::FunAnnot(Box::new((fn_loc, def)))));
                acc.append_call(p.static_, t);
            }
            O::Property::InternalSlot(p) => {
                let name = id_name(&p.id);
                if name.as_str() == "call" {
                    let t = annot(opts, scope, scopes, tbls, xs, &p.value);
                    let t = if p.optional {
                        Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))))
                    } else {
                        t
                    };
                    acc.append_call(p.static_, t);
                }
                // unsupported slot name
            }
            O::Property::SpreadProperty(_)
            | O::Property::MappedType(_)
            | O::Property::PrivateField(_) => {
                // no spread, mapped types, or private fields in interface / declare class
            }
        }
    }
}

fn nominal_type<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    loc: LocNode<'arena>,
    name: TyName<'arena, 'ast>,
    targs: &Option<ast::types::TypeArgs<Loc, Loc>>,
) -> Parsed<'arena, 'ast> {
    match targs {
        None => Parsed::TyRef(name),
        Some(type_args) => {
            let targs = type_args
                .arguments
                .iter()
                .map(|t| annot(opts, scope, scopes, tbls, xs, t))
                .collect();
            Parsed::TyRefApp { loc, name, targs }
        }
    }
}

fn get_import_type_base<'a>(
    id: &'a ast::types::generic::Identifier<Loc, Loc>,
) -> Option<(
    &'a Loc,
    &'a ast::types::generic::ImportType<Loc, Loc>,
    Vec<(Loc, FlowSmolStr)>,
)> {
    fn loop_<'a>(
        mut chain: Vec<(Loc, FlowSmolStr)>,
        id: &'a ast::types::generic::Identifier<Loc, Loc>,
    ) -> Option<(
        &'a Loc,
        &'a ast::types::generic::ImportType<Loc, Loc>,
        Vec<(Loc, FlowSmolStr)>,
    )> {
        match id {
            ast::types::generic::Identifier::ImportTypeAnnot(import_type) => {
                chain.reverse();
                Some((&import_type.loc, import_type.as_ref(), chain))
            }
            ast::types::generic::Identifier::Qualified(q) => {
                let id_loc = q.id.loc.dupe();
                let name = q.id.name.dupe();
                chain.push((id_loc, name));
                loop_(chain, &q.qualification)
            }
            ast::types::generic::Identifier::Unqualified(_) => None,
        }
    }
    loop_(Vec::new(), id)
}

// Handles import("module") type expressions by building ImportTypeAnnot with Eval chain
fn handle_import_type<'arena, 'ast>(
    tbls: &mut Tables<'arena, 'ast>,
    import_loc: &Loc,
    import_type: &ast::types::generic::ImportType<Loc, Loc>,
    chain: Vec<(Loc, FlowSmolStr)>,
) -> Parsed<'arena, 'ast> {
    let import_loc = tbls.push_loc(import_loc.dupe());
    let value = &import_type.source.1.value;
    let mref = Userland::from_smol_str(value.dupe());
    let mref = tbls.push_module_ref(mref);
    let base = Parsed::ImportTypeAnnot {
        loc: import_loc,
        mref,
    };
    chain.into_iter().fold(base, |t, (id_loc, name)| {
        let id_loc = tbls.push_loc(id_loc);
        Parsed::Eval(id_loc, Box::new(t), Box::new(Op::GetProp(Box::new(name))))
    })
}

fn generic_id<'arena, 'ast>(
    scope: ScopeId,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &tparam_stack::TParamStack,
    chain: Vec<(LocNode<'arena>, Loc, FlowSmolStr)>,
    id: &ast::types::generic::Identifier<Loc, Loc>,
) -> Result<TyName<'arena, 'ast>, LocNode<'arena>> {
    fn finish<'arena, 'ast>(
        tbls: &mut Tables<'arena, 'ast>,
        tyname: TyName<'arena, 'ast>,
        chain: Vec<(LocNode<'arena>, Loc, FlowSmolStr)>,
    ) -> TyName<'arena, 'ast> {
        let mut result = tyname;
        for (loc, id_loc, name) in chain.into_iter().rev() {
            let id_loc = tbls.push_loc(id_loc.dupe());
            result = TyName::Qualified {
                loc,
                id_loc,
                name,
                qualification: Box::new(result),
            };
        }
        result
    }

    match id {
        ast::types::generic::Identifier::Qualified(inner) => {
            let id_loc = inner.id.loc.dupe();
            let name = inner.id.name.dupe();
            let qloc = tbls.push_loc(inner.loc.dupe());
            let mut new_chain = chain;
            new_chain.push((qloc, id_loc, name));
            generic_id(scope, tbls, xs, new_chain, &inner.qualification)
        }
        ast::types::generic::Identifier::Unqualified(id) => {
            let ref_loc = tbls.push_loc(id.loc.dupe());
            let name = id.name.dupe();
            // Type params in scope should be handled before generic_id
            if xs.contains(&name) {
                Err(ref_loc)
            } else {
                let tyname = TyName::Unqualified(Box::new(Ref {
                    ref_loc,
                    name,
                    scope,
                    resolved: OnceCell::new(),
                }));
                Ok(finish(tbls, tyname, chain))
            }
        }
        ast::types::generic::Identifier::ImportTypeAnnot(import_type) => {
            let loc = tbls.push_loc(import_type.loc.dupe());
            Err(loc)
        }
    }
}

fn generic<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    loc: LocNode<'arena>,
    g: &ast::types::Generic<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    // First check if the generic identifier has an ImportType base
    match get_import_type_base(&g.id) {
        Some((import_loc, import_type, chain)) => {
            let t = handle_import_type(tbls, import_loc, import_type, chain);
            match &g.targs {
                None => t,
                Some(type_args) => {
                    let targs_loc = tbls.push_loc(type_args.loc.dupe());
                    Parsed::Err(targs_loc, Errno::CheckError)
                }
            }
        }
        None => match generic_id(scope, tbls, xs, Vec::new(), &g.id) {
            Ok(tyname) => nominal_type(opts, scope, scopes, tbls, xs, loc, tyname, &g.targs),
            Err(loc) => Parsed::Err(loc, Errno::CheckError),
        },
    }
}

fn maybe_special_generic<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    loc: LocNode<'arena>,
    g: &ast::types::Generic<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    // First check if the generic identifier has an ImportType base
    match get_import_type_base(&g.id) {
        Some((import_loc, import_type, chain)) => {
            let t = handle_import_type(tbls, import_loc, import_type, chain);
            match &g.targs {
                None => t,
                Some(type_args) => {
                    let targs_loc = tbls.push_loc(type_args.loc.dupe());
                    Parsed::Err(targs_loc, Errno::CheckError)
                }
            }
        }
        None => match &g.id {
            ast::types::generic::Identifier::Qualified(inner) => {
                let id_loc = inner.id.loc.dupe();
                let name = inner.id.name.dupe();
                let qloc = tbls.push_loc(inner.loc.dupe());
                let chain = vec![(qloc, id_loc, name)];
                match generic_id(scope, tbls, xs, chain, &inner.qualification) {
                    Ok(tyname) => {
                        nominal_type(opts, scope, scopes, tbls, xs, loc, tyname, &g.targs)
                    }
                    Err(loc) => Parsed::Err(loc, Errno::CheckError),
                }
            }
            ast::types::generic::Identifier::Unqualified(id) => {
                let ref_loc = tbls.push_loc(id.loc.dupe());
                let name = &id.name;
                maybe_special_unqualified_generic(
                    opts, scope, scopes, tbls, xs, loc, &g.targs, ref_loc, name,
                )
            }
            ast::types::generic::Identifier::ImportTypeAnnot(_) => {
                // This case is handled above by get_import_type_base, should not reach here
                unreachable!("ImportTypeAnnot should have been handled by get_import_type_base")
            }
        },
    }
}

fn maybe_special_unqualified_generic<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    loc: LocNode<'arena>,
    targs: &Option<ast::types::TypeArgs<Loc, Loc>>,
    ref_loc: LocNode<'arena>,
    name: &FlowSmolStr,
) -> Parsed<'arena, 'ast> {
    if let Some(initialized) = xs.reference(name) {
        return if initialized {
            Parsed::Annot(Box::new(ParsedAnnot::Bound(Box::new(AnnotBound {
                ref_loc,
                name: name.clone(),
            }))))
        } else {
            Parsed::Err(ref_loc, Errno::CheckError)
        };
    }

    match name.as_str() {
        _ if !opts.for_builtins && scope::lookup_type(scopes, scope, name).is_some() => {
            let name = TyName::Unqualified(Box::new(Ref {
                ref_loc,
                name: name.clone(),
                scope,
                resolved: OnceCell::new(),
            }));
            nominal_type(opts, scope, scopes, tbls, xs, loc, name, targs)
        }
        // Lowercase `object` is the builtin that rejects primitives.
        "object" if opts.is_ts_file || opts.for_builtins => match targs {
            None => Parsed::Annot(Box::new(ParsedAnnot::ObjectBuiltin(Box::new((
                loc,
                tbls.strictness_kind,
            ))))),
            Some(_) => Parsed::Err(loc, Errno::CheckError),
        },
        // `ThisType<T>` is a marker that Flow reduces to a
        // structurally-empty interface; the type argument is irrelevant (see
        // type_annotation.rs). We parse the argument so any types it references
        // still resolve, then discard it. This mirrors the checking path so
        // cross-file consumers of a `.d.ts` see `M & ThisType<T>` as `M` rather
        // than degrading to `any`.
        "ThisType" if opts.is_ts_file || opts.for_builtins => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                let def = interface_acc::InterfaceAcc::empty().interface_def(
                    false,
                    Vec::new(),
                    tbls.strictness_kind,
                );
                Parsed::Annot(Box::new(ParsedAnnot::InlineInterface(Box::new((loc, def)))))
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "Array" => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                Parsed::Annot(Box::new(ParsedAnnot::Array(Box::new((loc, t)))))
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "Class" => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                Parsed::Annot(Box::new(ParsedAnnot::ClassT(Box::new((loc, t)))))
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "function" => match targs {
            None => Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc)))),
            Some(_) => Parsed::Err(loc, Errno::CheckError),
        },
        "NoInfer" => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                Parsed::Annot(Box::new(ParsedAnnot::NoInfer(Box::new(t))))
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "ExactObjectType" => match targs {
            None => Parsed::Annot(Box::new(ParsedAnnot::ExactObjectType(Box::new(loc)))),
            Some(_) => Parsed::Err(loc, Errno::CheckError),
        },
        "$ReadOnlyArray" => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                Parsed::Annot(Box::new(ParsedAnnot::ReadOnlyArray(Box::new((loc, t)))))
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "$EnumValue" => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                Parsed::Annot(Box::new(ParsedAnnot::EnumValue(Box::new((loc, t)))))
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "$Enum" => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                Parsed::Annot(Box::new(ParsedAnnot::Enum(Box::new((loc, t)))))
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "$NonMaybeType" => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                Parsed::Annot(Box::new(ParsedAnnot::NonMaybeType(Box::new((loc, t)))))
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "$Shape" => Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc)))),
        "$Omit" => match targs {
            Some(type_args) if type_args.arguments.len() == 2 => {
                let t1 = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                let t2 = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[1]);
                Parsed::Annot(Box::new(ParsedAnnot::Omit(Box::new((loc, t1, t2)))))
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "$ReadOnly" => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                Parsed::Annot(Box::new(ParsedAnnot::ReadOnly(Box::new((loc, t)))))
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "$Partial" => Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc)))),
        "Partial" => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                Parsed::Annot(Box::new(ParsedAnnot::Partial(Box::new((loc, t)))))
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "Required" => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                Parsed::Annot(Box::new(ParsedAnnot::Required(Box::new((loc, t)))))
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "$Keys" => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                Parsed::Annot(Box::new(ParsedAnnot::Keys(Box::new((loc, t)))))
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "$Values" => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                Parsed::Annot(Box::new(ParsedAnnot::Values(Box::new((loc, t)))))
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "Values" => {
            if opts.enable_ts_syntax || opts.enable_ts_utility_syntax {
                match targs {
                    Some(type_args) if type_args.arguments.len() == 1 => {
                        let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                        Parsed::Annot(Box::new(ParsedAnnot::Values(Box::new((loc, t)))))
                    }
                    _ => Parsed::Err(loc, Errno::CheckError),
                }
            } else {
                Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc))))
            }
        }
        "$Exact" => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                Parsed::Annot(Box::new(ParsedAnnot::Exact(Box::new((loc, t)))))
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "$Exports" => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                match type_args.arguments[0].deref() {
                    TypeInner::StringLiteral { literal, .. } => {
                        Parsed::Annot(Box::new(ParsedAnnot::ExportsT(Box::new((
                            loc,
                            Userland::from_smol_str(literal.value.dupe()),
                        )))))
                    }
                    _ => {
                        let arg_loc = tbls.push_loc(type_args.arguments[0].loc().clone());
                        Parsed::Err(arg_loc, Errno::CheckError)
                    }
                }
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "$KeyMirror" => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                let obj = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                Parsed::Annot(Box::new(ParsedAnnot::ObjKeyMirror(Box::new(
                    AnnotObjKeyMirror { loc, obj },
                ))))
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        kind_name @ ("Uppercase" | "Lowercase" | "Capitalize" | "Uncapitalize") => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                match kind_name {
                    "Uppercase" => {
                        Parsed::Annot(Box::new(ParsedAnnot::Uppercase(Box::new((loc, t)))))
                    }
                    "Lowercase" => {
                        Parsed::Annot(Box::new(ParsedAnnot::Lowercase(Box::new((loc, t)))))
                    }
                    "Capitalize" => {
                        Parsed::Annot(Box::new(ParsedAnnot::Capitalize(Box::new((loc, t)))))
                    }
                    "Uncapitalize" => {
                        Parsed::Annot(Box::new(ParsedAnnot::Uncapitalize(Box::new((loc, t)))))
                    }
                    _ => unreachable!(),
                }
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "$Flow$EnforceOptimized" => match targs {
            Some(type_args) if type_args.arguments.len() == 1 => {
                annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0])
            }
            _ => Parsed::Err(loc, Errno::CheckError),
        },
        "Readonly" => {
            if opts.enable_ts_syntax || opts.enable_ts_utility_syntax {
                match targs {
                    Some(type_args) if type_args.arguments.len() == 1 => {
                        let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                        Parsed::Annot(Box::new(ParsedAnnot::ReadOnly(Box::new((loc, t)))))
                    }
                    _ => Parsed::Err(loc, Errno::CheckError),
                }
            } else {
                Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc))))
            }
        }
        "ReadonlyArray" => {
            if opts.enable_ts_syntax || opts.enable_ts_utility_syntax {
                match targs {
                    Some(type_args) if type_args.arguments.len() == 1 => {
                        let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                        Parsed::Annot(Box::new(ParsedAnnot::ReadOnlyArray(Box::new((loc, t)))))
                    }
                    _ => Parsed::Err(loc, Errno::CheckError),
                }
            } else {
                Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc))))
            }
        }
        "NonNullable" => {
            if opts.enable_ts_syntax || opts.enable_ts_utility_syntax {
                match targs {
                    Some(type_args) if type_args.arguments.len() == 1 => {
                        let t = annot(opts, scope, scopes, tbls, xs, &type_args.arguments[0]);
                        Parsed::Annot(Box::new(ParsedAnnot::NonMaybeType(Box::new((loc, t)))))
                    }
                    _ => Parsed::Err(loc, Errno::CheckError),
                }
            } else {
                Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc))))
            }
        }
        "ReadonlyMap" if !(opts.enable_ts_syntax || opts.enable_ts_utility_syntax) => {
            Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc))))
        }
        "ReadonlySet" if !(opts.enable_ts_syntax || opts.enable_ts_utility_syntax) => {
            Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc))))
        }
        _ => {
            let name = TyName::Unqualified(Box::new(Ref {
                ref_loc,
                name: name.clone(),
                scope,
                resolved: OnceCell::new(),
            }));
            nominal_type(opts, scope, scopes, tbls, xs, loc, name, targs)
        }
    }
}

fn tparam<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    param: &ast::types::TypeParam<Loc, Loc>,
    include_default: bool,
) -> TParam<LocNode<'arena>, Parsed<'arena, 'ast>> {
    fn bound<'arena, 'ast>(
        opts: &TypeSigOptions,
        scope: ScopeId,
        scopes: &mut scope::Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        xs: &mut tparam_stack::TParamStack,
        b: &ast::types::AnnotationOrHint<Loc, Loc>,
    ) -> Option<Parsed<'arena, 'ast>> {
        match b {
            ast::types::AnnotationOrHint::Available(annotation) => {
                Some(annot(opts, scope, scopes, tbls, xs, &annotation.annotation))
            }
            ast::types::AnnotationOrHint::Missing(_) => None,
        }
    }

    fn default<'arena, 'ast>(
        opts: &TypeSigOptions,
        scope: ScopeId,
        scopes: &mut scope::Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        xs: &mut tparam_stack::TParamStack,
        d: &Option<ast::types::Type<Loc, Loc>>,
    ) -> Option<Parsed<'arena, 'ast>> {
        d.as_ref().map(|t| annot(opts, scope, scopes, tbls, xs, t))
    }

    let name_loc = tbls.push_loc(param.name.loc.dupe());
    let name = param.name.name.dupe();
    let bound_val = bound(opts, scope, scopes, tbls, xs, &param.bound);
    let default_val = include_default
        .then(|| default(opts, scope, scopes, tbls, xs, &param.default))
        .flatten();
    let is_const = param.const_.is_some();
    TParam {
        name_loc,
        name,
        polarity: param
            .variance
            .as_ref()
            .map(|v| polarity(Some((v.loc.dupe(), v.clone())))),
        bound: bound_val,
        default: default_val,
        is_const,
    }
}

fn tparams<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    tparams: Option<&ast::types::TypeParams<Loc, Loc>>,
) -> TParams<LocNode<'arena>, Parsed<'arena, 'ast>> {
    match tparams {
        None => TParams::Mono,
        Some(tparams) => {
            let analysis = analyze_type_params(tparams);
            let cyclic: BTreeSet<usize> = analysis
                .components()
                .into_iter()
                .filter(|component| component.cyclic)
                .flat_map(|component| component.members)
                .collect();
            let order = type_param_order(tparams, &cyclic);
            let tparams_loc = tbls.push_loc(tparams.loc.dupe());
            for tp in tparams.params.iter() {
                xs.insert_uninitialized(tp.name.name.dupe());
            }
            let mut acc = Vec::with_capacity(tparams.params.len());
            for (index, tp) in tparams.params.iter().enumerate() {
                xs.begin_references(true);
                let mut tp_val = tparam(opts, scope, scopes, tbls, xs, tp, false);
                xs.finish_references();
                if cyclic.contains(&index) {
                    tp_val.bound = Some(Parsed::Err(tp_val.name_loc.dupe(), Errno::CheckError));
                }
                xs.begin_references(false);
                tp_val.default = tp
                    .default
                    .as_ref()
                    .map(|default| annot(opts, scope, scopes, tbls, xs, default));
                xs.finish_references();
                xs.initialize(&tp.name.name);
                acc.push(tp_val);
            }
            if let Ok(tps) = Vec1::try_from_vec(acc) {
                TParams::Poly(Box::new(PolyTParams {
                    loc: tparams_loc,
                    tparams: tps,
                    bound_order: order,
                }))
            } else {
                TParams::Mono
            }
        }
    }
}

fn conditional_type<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    loc: LocNode<'arena>,
    t: &ast::types::Conditional<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    let check_type = annot(opts, scope, scopes, tbls, xs, &t.check_type);
    let distributive_tparam = if let Parsed::Annot(annot) = &check_type
        && let ParsedAnnot::Bound(inner) = annot.as_ref()
    {
        let AnnotBound { ref_loc, name } = inner.as_ref();
        // If check type is a bound type, then this is a distributive conditional type.
        Some(TParam {
            name_loc: ref_loc.dupe(),
            name: name.clone(),
            polarity: None,
            bound: Some(check_type.clone()),
            default: None,
            is_const: false,
        })
    } else {
        None
    };
    let hoisted_infer_types = flow_analysis::infer_type_hoister::hoist_infer_types(&t.extends_type);
    let infer_type_names: BTreeMap<FlowSmolStr, Loc> = hoisted_infer_types
        .iter()
        .map(|(_loc, infer)| (infer.tparam.name.name.dupe(), infer.tparam.loc.dupe()))
        .collect();
    let extends_scope = scopes.push(scope::Scope::ConditionalTypeExtends(
        scope::ConditionalTypeExtends {
            infer_type_names,
            infer_tparams: Vec::new(),
            parent: scope,
        },
    ));
    let (extends_type_loc, extends_type) =
        annot_with_loc(opts, extends_scope, scopes, tbls, xs, &t.extends_type);
    // Ensure all hoisted infer type parameters get registered, even if they were
    // inside constructs that short-circuited to Any without visiting their children
    // (e.g. $Shape, $Partial, unsupported TS utilities, TemplateLiteral, etc.).
    for (_, infer) in &hoisted_infer_types {
        let name = &infer.tparam.name.name;
        let already_registered = match scopes.get(extends_scope) {
            scope::Scope::ConditionalTypeExtends(cond_scope) => {
                cond_scope.infer_tparams.iter().any(|tp| &tp.name == name)
            }
            _ => unreachable!(),
        };
        if !already_registered {
            let tp = tparam(opts, scope, scopes, tbls, xs, &infer.tparam, true);
            if let scope::Scope::ConditionalTypeExtends(cond_scope) = scopes.get_mut(extends_scope)
            {
                cond_scope.infer_tparams.push(tp);
            }
        }
    }
    let infer_tparams = match scopes.get(extends_scope) {
        scope::Scope::ConditionalTypeExtends(cond_scope) => {
            if let Ok(tparams) = Vec1::try_from_vec(cond_scope.infer_tparams.to_vec()) {
                let order = (0..tparams.len()).collect();
                TParams::Poly(Box::new(PolyTParams {
                    loc: extends_type_loc,
                    tparams,
                    bound_order: order,
                }))
            } else {
                TParams::Mono
            }
        }
        _ => unreachable!(),
    };
    xs.push_new_frame();
    for (_, infer) in &hoisted_infer_types {
        xs.insert(infer.tparam.name.name.dupe());
    }
    let true_type = annot(opts, scope, scopes, tbls, xs, &t.true_type);
    xs.pop_frame();
    let false_type = annot(opts, scope, scopes, tbls, xs, &t.false_type);
    Parsed::Annot(Box::new(ParsedAnnot::Conditional(Box::new(
        AnnotConditional {
            loc,
            distributive_tparam,
            infer_tparams,
            check_type,
            extends_type,
            true_type,
            false_type,
        },
    ))))
}

fn infer_type<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    loc: LocNode<'arena>,
    t: &ast::types::Infer<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    let tp_loc = &t.tparam.loc;
    let name = &t.tparam.name.name;
    match scope::scope_of_infer_name(scopes, scope, name, tp_loc) {
        None => Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc)))),
        Some(infer_scope) => {
            let name_loc = if let Some(name_loc) = infer_scope
                .infer_tparams
                .iter()
                .find(|tp| &tp.name == name)
                .map(|tp| tp.name_loc.dupe())
            {
                name_loc
            } else {
                let tp = tparam(opts, scope, scopes, tbls, xs, &t.tparam, true);
                let name_loc = tp.name_loc.dupe();
                if let scope::Scope::ConditionalTypeExtends(cond_scope) = scopes.get_mut(scope) {
                    cond_scope.infer_tparams.push(tp);
                }
                name_loc
            };
            Parsed::Annot(Box::new(ParsedAnnot::Bound(Box::new(AnnotBound {
                ref_loc: name_loc,
                name: name.clone(),
            }))))
        }
    }
}

fn optional_indexed_access<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    loc: LocNode<'arena>,
    oia: &ast::types::OptionalIndexedAccess<Loc, Loc>,
) -> (Parsed<'arena, 'ast>, Parsed<'arena, 'ast>) {
    let indexed_access = &oia.indexed_access;
    let optional = oia.optional;
    let (obj_loc, obj) = match indexed_access.object.deref() {
        TypeInner::OptionalIndexedAccess { loc, inner } => {
            let loc = tbls.push_loc(loc.dupe());
            let (non_maybe_obj, _) =
                optional_indexed_access(opts, scope, scopes, tbls, xs, loc.dupe(), inner.as_ref());
            (loc, non_maybe_obj)
        }
        _ => annot_with_loc(opts, scope, scopes, tbls, xs, &indexed_access.object),
    };
    let index = annot(opts, scope, scopes, tbls, xs, &indexed_access.index);
    let non_maybe_result = if optional {
        Parsed::Annot(Box::new(ParsedAnnot::OptionalIndexedAccessNonMaybeType(
            Box::new(AnnotOptionalIndexedAccessNonMaybeType {
                loc: loc.dupe(),
                obj,
                index,
            }),
        )))
    } else {
        Parsed::Annot(Box::new(ParsedAnnot::ElementType(Box::new(
            AnnotElementType {
                loc: loc.dupe(),
                obj,
                elem: index,
            },
        ))))
    };
    let result = Parsed::Annot(Box::new(ParsedAnnot::OptionalIndexedAccessResultType(
        Box::new(AnnotOptionalIndexedAccessResultType {
            loc,
            non_maybe_result: non_maybe_result.clone(),
            void_loc: obj_loc,
        }),
    )));
    (non_maybe_result, result)
}

fn annot_or_hint<'arena, 'ast>(
    sort: ExpectedAnnotationSort,
    err_loc: Option<&LocNode<'arena>>,
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    hint: &ast::types::AnnotationOrHint<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    match hint {
        ast::types::AnnotationOrHint::Available(annotation) => {
            annot(opts, scope, scopes, tbls, xs, &annotation.annotation)
        }
        ast::types::AnnotationOrHint::Missing(loc) => {
            let err_loc = if let Some(l) = err_loc {
                l.dupe()
            } else {
                tbls.push_loc(loc.dupe())
            };
            Parsed::Err(
                err_loc.dupe(),
                Errno::SigError(Box::new(
                    signature_error::SignatureError::ExpectedAnnotation(err_loc, sort),
                )),
            )
        }
    }
}

fn function_return_annot<'arena, 'ast>(
    sort: ExpectedAnnotationSort,
    err_loc: Option<LocNode<'arena>>,
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    r: &ast::function::ReturnAnnot<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    match r {
        ast::function::ReturnAnnot::Available(annotation) => {
            annot(opts, scope, scopes, tbls, xs, &annotation.annotation)
        }
        ast::function::ReturnAnnot::TypeGuard(type_guard) => {
            // This is only used for getters, where type guards are not allowed
            let loc = tbls.push_loc(type_guard.loc.dupe());
            Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc))))
        }
        ast::function::ReturnAnnot::Missing(loc) => {
            let err_loc = if let Some(l) = err_loc {
                l
            } else {
                tbls.push_loc(loc.dupe())
            };
            Parsed::Err(
                err_loc.dupe(),
                Errno::SigError(Box::new(
                    signature_error::SignatureError::ExpectedAnnotation(err_loc, sort),
                )),
            )
        }
    }
}

fn class_implements<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    implements: &Option<ast::class::Implements<Loc, Loc>>,
) -> Vec<Parsed<'arena, 'ast>> {
    let Some(implements) = implements else {
        return Vec::new();
    };

    let mut acc = Vec::new();
    for interface in implements.interfaces.iter() {
        let loc = tbls.push_loc(interface.loc.dupe());
        match generic_id(scope, tbls, xs, vec![], &interface.id) {
            Ok(name) => {
                let t = nominal_type(opts, scope, scopes, tbls, xs, loc, name, &interface.targs);
                acc.push(t);
            }
            // type param as implements target — skip
            Err(_loc) => {}
        }
    }
    acc
}

fn getter_def<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    id_loc: LocNode<'arena>,
    f: &ast::function::Function<Loc, Loc>,
) -> Accessor<LocNode<'arena>, Parsed<'arena, 'ast>> {
    let t = function_return_annot(
        ExpectedAnnotationSort::FunctionReturn,
        None,
        opts,
        scope,
        scopes,
        tbls,
        xs,
        &f.return_,
    );
    Accessor::Get(Box::new((id_loc, t)))
}

fn setter_def<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    id_loc: LocNode<'arena>,
    f: &ast::function::Function<Loc, Loc>,
) -> Accessor<LocNode<'arena>, Parsed<'arena, 'ast>> {
    use ast::pattern::Pattern;
    let params = &f.params.params;
    match &**params {
        [param] => match param {
            ast::function::Param::RegularParam {
                loc,
                argument,
                default: _,
            } => match argument {
                Pattern::Identifier {
                    inner: ident_pattern,
                    ..
                } => {
                    let param_loc = tbls.push_loc(loc.clone());
                    let t = annot_or_hint(
                        ExpectedAnnotationSort::Identifier,
                        Some(&param_loc),
                        opts,
                        scope,
                        scopes,
                        tbls,
                        xs,
                        &ident_pattern.annot,
                    );
                    Accessor::Set(Box::new((id_loc, t)))
                }
                _ => panic!("unexpected setter"),
            },
            ast::function::Param::ParamProperty { .. } => panic!("unexpected setter"),
        },
        _ => panic!("unexpected setter"),
    }
}

fn module_ref_literal<'arena, 'ast>(
    tbls: &mut Tables<'arena, 'ast>,
    loc: LocNode<'arena>,
    mref: &ast::ModuleRefLiteral<Loc>,
) -> Parsed<'arena, 'ast> {
    let remaining = &mref.value[mref.prefix_len..];
    let spec = Userland::from_smol_str(FlowSmolStr::new(remaining));
    let mref_node = tbls.push_module_ref(spec);
    Parsed::ModuleRef {
        loc,
        mref: mref_node,
    }
}

fn string_literal<'arena, 'ast>(
    frozen: FrozenKind,
    loc: LocNode<'arena>,
    s: &FlowSmolStr,
) -> Parsed<'arena, 'ast> {
    if frozen == FrozenKind::FrozenProp {
        Parsed::Annot(Box::new(ParsedAnnot::SingletonString(Box::new((
            loc,
            s.clone(),
        )))))
    } else {
        Parsed::Value(Box::new(ParsedValue::StringLit(Box::new((loc, s.clone())))))
    }
}

fn template_literal<'arena, 'ast>(
    loc: LocNode<'arena>,
    quasis: &[ast::expression::template_literal::Element<Loc>],
) -> Parsed<'arena, 'ast> {
    match quasis {
        [element] => string_literal(FrozenKind::NotFrozen, loc, &element.value.cooked),
        _ => Parsed::Value(Box::new(ParsedValue::StringVal(Box::new(loc)))),
    }
}

fn template_literal_expression<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    frozen: FrozenKind,
    loc: LocNode<'arena>,
    quasi: &'ast ast::expression::TemplateLiteral<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    match quasi.quasis.as_ref() {
        [element] => string_literal(frozen, loc, &element.value.cooked),
        _ => {
            let quasis = quasi
                .quasis
                .iter()
                .map(|element| element.value.cooked.dupe())
                .collect();
            let types = quasi
                .expressions
                .iter()
                .map(|expr| expression(opts, scope, scopes, tbls, FrozenKind::NotFrozen, expr))
                .collect();
            Parsed::Value(Box::new(ParsedValue::TemplateLiteral(Box::new(
                ValueTemplateLiteral { loc, quasis, types },
            ))))
        }
    }
}

fn graphql_literal<'arena, 'ast>(
    opts: &TypeSigOptions,
    tbls: &mut Tables<'arena, 'ast>,
    loc: LocNode<'arena>,
    quasi: &ast::expression::TemplateLiteral<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    let module_prefix = opts.relay_integration_module_prefix.as_deref();
    match graphql::extract_module_name(quasi, module_prefix) {
        Ok(module_name) => {
            let mref = tbls.push_module_ref(Userland::from_smol_str(FlowSmolStr::new(module_name)));
            Parsed::Require { loc, mref }
        }
        Err(_) => Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc)))),
    }
}

fn key_mirror<'arena, 'ast>(
    tbls: &mut Tables<'arena, 'ast>,
    loc: LocNode<'arena>,
    properties: &[ast::expression::object::Property<Loc, Loc>],
) -> Parsed<'arena, 'ast> {
    use ast::expression::object::Key;
    use ast::expression::object::NormalProperty;
    use ast::expression::object::Property;
    use object_literal_acc::ObjectLiteralAcc;

    let mut acc = ObjectLiteralAcc::empty();

    for p in properties {
        match p {
            Property::NormalProperty(NormalProperty::Init { key, .. }) => {
                let (id_loc, name) = match key {
                    Key::Identifier(id) => {
                        let IdentifierInner {
                            loc: id_loc, name, ..
                        } = &**id;
                        (id_loc, name)
                    }
                    Key::StringLiteral((id_loc, ast::StringLiteral { value: name, .. })) => {
                        (id_loc, name)
                    }
                    Key::NumberLiteral((key_loc, _)) | Key::BigIntLiteral((key_loc, _)) => {
                        let prop_loc = tbls.push_loc(key_loc.dupe());
                        return Parsed::Err(
                            loc.dupe(),
                            Errno::SigError(Box::new(
                                signature_error::SignatureError::UnexpectedObjectKey(loc, prop_loc),
                            )),
                        );
                    }
                    Key::Computed(computed) => {
                        let prop_loc = tbls.push_loc(computed.loc.dupe());
                        return Parsed::Err(
                            loc.dupe(),
                            Errno::SigError(Box::new(
                                signature_error::SignatureError::UnexpectedObjectKey(loc, prop_loc),
                            )),
                        );
                    }
                    Key::PrivateName(pn) => {
                        let prop_loc = tbls.push_loc(pn.loc.dupe());
                        return Parsed::Err(
                            loc.dupe(),
                            Errno::SigError(Box::new(
                                signature_error::SignatureError::UnexpectedObjectKey(loc, prop_loc),
                            )),
                        );
                    }
                };
                let id_loc = tbls.push_loc(id_loc.dupe());
                if name.as_str() != "__proto__" {
                    let t = Parsed::Annot(Box::new(ParsedAnnot::SingletonString(Box::new((
                        id_loc.dupe(),
                        name.clone(),
                    )))));
                    acc.add_field(name.clone(), id_loc, t, Polarity::Neutral);
                }
            }
            Property::NormalProperty(normal_prop) => {
                let prop_loc = tbls.push_loc(normal_prop.loc().dupe());
                return Parsed::Err(
                    loc.dupe(),
                    Errno::SigError(Box::new(
                        signature_error::SignatureError::UnexpectedObjectKey(loc, prop_loc),
                    )),
                );
            }
            Property::SpreadProperty(spread_prop) => {
                let prop_loc = tbls.push_loc(spread_prop.loc.dupe());
                return Parsed::Err(
                    loc.dupe(),
                    Errno::SigError(Box::new(
                        signature_error::SignatureError::UnexpectedObjectKey(loc, prop_loc),
                    )),
                );
            }
        }
    }

    acc.object_lit(loc, false, tbls.strictness_kind)
}

fn jsx_element<'arena, 'ast>(
    opts: &TypeSigOptions,
    tbls: &mut Tables<'arena, 'ast>,
    loc: LocNode<'arena>,
    elem: &'ast ast::jsx::Element<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    let opening_element = &elem.opening_element;
    match (&opening_element.name, &opts.facebook_fbt) {
        (ast::jsx::Name::Identifier(id), Some(custom_jsx_type)) if id.name.as_str() == "fbt" => {
            let ref_loc = tbls.push_loc(id.loc.dupe());
            Parsed::BuiltinTyRef {
                ref_loc,
                name: custom_jsx_type.dupe(),
            }
        }
        _ => Parsed::Err(
            loc.dupe(),
            Errno::SigError(Box::new(
                signature_error::SignatureError::UnexpectedExpression(
                    loc,
                    ast_utils::expression_sort::ExpressionSort::JSXElement,
                ),
            )),
        ),
    }
}

fn binary<'arena, 'ast>(
    loc: LocNode<'arena>,
    lhs_t: Parsed<'arena, 'ast>,
    rhs_t: Parsed<'arena, 'ast>,
    op: &ast::expression::BinaryOperator,
) -> Parsed<'arena, 'ast> {
    use ast::expression::BinaryOperator;
    match op {
        BinaryOperator::Equal
        | BinaryOperator::NotEqual
        | BinaryOperator::StrictEqual
        | BinaryOperator::StrictNotEqual
        | BinaryOperator::LessThan
        | BinaryOperator::LessThanEqual
        | BinaryOperator::GreaterThan
        | BinaryOperator::GreaterThanEqual
        | BinaryOperator::In
        | BinaryOperator::Instanceof => {
            Parsed::Value(Box::new(ParsedValue::BooleanVal(Box::new(loc))))
        }
        BinaryOperator::LShift
        | BinaryOperator::RShift
        | BinaryOperator::RShift3
        | BinaryOperator::Minus
        | BinaryOperator::Mult
        | BinaryOperator::Exp
        | BinaryOperator::Div
        | BinaryOperator::Mod
        | BinaryOperator::BitOr
        | BinaryOperator::Xor
        | BinaryOperator::BitAnd
        | BinaryOperator::Plus => Parsed::Eval(
            loc,
            Box::new(lhs_t),
            Box::new(Op::Arith(Box::new((*op, rhs_t)))),
        ),
    }
}

fn expression<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    frozen: FrozenKind,
    expr: &'ast ast::expression::Expression<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    let loc = tbls.push_loc(expr.loc().dupe());
    use ast::expression::ExpressionInner as E;

    match expr.deref() {
        E::StringLiteral { inner, .. } => string_literal(frozen, loc, &inner.value),
        E::NumberLiteral { inner, .. } => {
            if frozen == FrozenKind::FrozenProp {
                Parsed::Annot(Box::new(ParsedAnnot::SingletonNumber(Box::new((
                    loc,
                    inner.value,
                    inner.raw.dupe(),
                )))))
            } else {
                Parsed::Value(Box::new(ParsedValue::NumberLit(Box::new((
                    loc,
                    inner.value,
                    inner.raw.dupe(),
                )))))
            }
        }
        E::BigIntLiteral { inner, .. } => {
            if frozen == FrozenKind::FrozenProp {
                Parsed::Annot(Box::new(ParsedAnnot::SingletonBigInt(Box::new((
                    loc,
                    inner.value,
                    inner.raw.dupe(),
                )))))
            } else {
                Parsed::Value(Box::new(ParsedValue::BigIntLit(Box::new((
                    loc,
                    inner.value,
                    inner.raw.dupe(),
                )))))
            }
        }
        E::BooleanLiteral { inner, .. } => {
            if frozen == FrozenKind::FrozenProp {
                Parsed::Annot(Box::new(ParsedAnnot::SingletonBoolean(Box::new((
                    loc,
                    inner.value,
                )))))
            } else {
                Parsed::Value(Box::new(ParsedValue::BooleanLit(Box::new((
                    loc,
                    inner.value,
                )))))
            }
        }
        E::NullLiteral { .. } => Parsed::Value(Box::new(ParsedValue::NullLit(Box::new(loc)))),
        E::RegExpLiteral { .. } => Parsed::Err(
            loc.dupe(),
            Errno::SigError(Box::new(
                signature_error::SignatureError::UnexpectedExpression(
                    loc,
                    ast_utils::expression_sort::ExpressionSort::Literal,
                ),
            )),
        ),
        E::ModuleRefLiteral { inner, .. } => module_ref_literal(tbls, loc, inner.as_ref()),
        E::TaggedTemplate { inner, .. } if opts.enable_relay_integration => match &*inner.tag {
            E::Identifier { inner: id, .. } if id.name.as_str() == "graphql" => {
                graphql_literal(opts, tbls, loc, &inner.quasi.1)
            }
            _ => template_literal(loc, &inner.quasi.1.quasis),
        },
        E::TemplateLiteral { inner, .. } => {
            template_literal_expression(opts, scope, scopes, tbls, frozen, loc, inner)
        }
        E::Identifier { inner, .. } => {
            let id_loc = tbls.push_loc(inner.loc.dupe());
            val_ref(false, scope, id_loc, inner.name.dupe())
        }
        E::Member { inner, .. } => member(
            opts,
            scope,
            scopes,
            tbls,
            &inner.object,
            loc,
            &inner.property,
        ),
        E::Class { inner, .. } => match &inner.id {
            Some(id) => {
                let id_loc = tbls.push_loc(id.loc.dupe());
                let splice_loc = id_loc.dupe();
                let name = id.name.dupe();
                let child_scope = scope::push_lex(scopes, scope);
                let def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
                    tbls.splice(splice_loc, |tbls| {
                        class_def(opts, child_scope, scopes, tbls, inner.as_ref())
                    })
                }));
                scope::bind_class(
                    opts,
                    child_scope,
                    scopes,
                    tbls,
                    id_loc.dupe(),
                    name.clone(),
                    def,
                    |_, _, _| {},
                );
                val_ref(false, child_scope, id_loc, name)
            }
            None => {
                let def = class_def(opts, scope, scopes, tbls, inner.as_ref());
                Parsed::Value(Box::new(ParsedValue::ClassExpr(Box::new((loc, def)))))
            }
        },
        E::Function { inner, .. } => {
            let ast::function::Function {
                id,
                async_,
                generator,
                sig_loc,
                ..
            } = inner.as_ref();
            let sig_loc_node = tbls.push_loc(sig_loc.dupe());
            match id {
                Some(id) => {
                    let id_loc = tbls.push_loc(id.loc.dupe());
                    let splice_loc = id_loc.dupe();
                    let name = id.name.dupe();
                    let child_scope = scope::push_lex(scopes, scope);
                    let def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
                        tbls.splice(splice_loc, |tbls| {
                            function_def(
                                opts,
                                child_scope,
                                scopes,
                                tbls,
                                &mut tparam_stack::TParamStack::new(),
                                loc,
                                inner.as_ref(),
                            )
                        })
                    }));
                    scope::bind_function(
                        child_scope,
                        scopes,
                        tbls,
                        id_loc.dupe(),
                        sig_loc_node,
                        name.clone(),
                        *async_,
                        *generator,
                        ast::function::Effect::Arbitrary,
                        def,
                        &|_, _, _| {},
                    );
                    val_ref(false, child_scope, id_loc, name)
                }
                None => {
                    let mut xs = tparam_stack::TParamStack::new();
                    let def = function_def(opts, scope, scopes, tbls, &mut xs, loc, inner.as_ref());
                    let statics = BTreeMap::new();
                    Parsed::Value(Box::new(ParsedValue::FunExpr(Box::new(ValueFunExpr {
                        loc: sig_loc_node,
                        async_: *async_,
                        generator: *generator,
                        def,
                        statics,
                    }))))
                }
            }
        }
        E::ArrowFunction { inner, .. } => {
            let ast::function::Function {
                async_, generator, ..
            } = inner.as_ref();
            let mut xs = tparam_stack::TParamStack::new();
            let def = function_def(
                opts,
                scope,
                scopes,
                tbls,
                &mut xs,
                loc.dupe(),
                inner.as_ref(),
            );
            let statics = BTreeMap::new();
            Parsed::Value(Box::new(ParsedValue::FunExpr(Box::new(ValueFunExpr {
                loc,
                async_: *async_,
                generator: *generator,
                def,
                statics,
            }))))
        }
        E::TypeCast { inner, .. } => {
            let mut xs = tparam_stack::TParamStack::new();
            annot(opts, scope, scopes, tbls, &mut xs, &inner.annot.annotation)
        }
        E::AsExpression { inner, .. } => {
            let mut xs = tparam_stack::TParamStack::new();
            annot(opts, scope, scopes, tbls, &mut xs, &inner.annot.annotation)
        }
        E::AsConstExpression { inner, .. } => {
            // Empty array literals are an error in general. Special-case `[] as const`.
            if let E::Array {
                inner: array_inner, ..
            } = inner.expression.deref()
            {
                if array_inner.elements.is_empty() {
                    return Parsed::Value(Box::new(ParsedValue::EmptyConstArrayLit(Box::new(
                        ValueEmptyConstArrayLit {
                            loc,
                            strictness_kind: tbls.strictness_kind,
                        },
                    ))));
                }
            }

            match expression(opts, scope, scopes, tbls, frozen, &inner.expression) {
                Parsed::Value(v) => Parsed::Value(Box::new(ParsedValue::AsConst(v))),
                Parsed::Eval(
                    loc,
                    box Parsed::Value(v),
                    box Op::Unary(ast::expression::UnaryOperator::Minus),
                ) => Parsed::Eval(
                    loc,
                    Box::new(Parsed::Value(Box::new(ParsedValue::AsConst(v)))),
                    Box::new(Op::Unary(ast::expression::UnaryOperator::Minus)),
                ),
                e => e,
            }
        }
        E::TSSatisfies { inner, .. } => {
            expression(opts, scope, scopes, tbls, frozen, &inner.expression)
        }
        E::Object { inner, .. } => {
            let frozen_flag = frozen == FrozenKind::FrozenDirect;
            object_literal(opts, scope, scopes, tbls, frozen_flag, loc, inner)
        }
        E::Array { inner, .. } => array_literal(opts, scope, scopes, tbls, loc, inner),
        E::Unary { inner, .. } => match inner.operator {
            ast::expression::UnaryOperator::Nonnull | ast::expression::UnaryOperator::Await => {
                let e = signature_error::SignatureError::UnexpectedExpression(
                    loc.dupe(),
                    ast_utils::expression_sort::ExpressionSort::Unary,
                );
                Parsed::Err(loc, Errno::SigError(Box::new(e)))
            }
            _ => {
                let t = expression(opts, scope, scopes, tbls, frozen, &inner.argument);
                Parsed::Eval(loc, Box::new(t), Box::new(Op::Unary(inner.operator)))
            }
        },
        E::Binary { inner, .. } => {
            let lhs_t = expression(
                opts,
                scope,
                scopes,
                tbls,
                FrozenKind::NotFrozen,
                &inner.left,
            );
            let rhs_t = expression(
                opts,
                scope,
                scopes,
                tbls,
                FrozenKind::NotFrozen,
                &inner.right,
            );
            binary(loc, lhs_t, rhs_t, &inner.operator)
        }
        E::Update { inner, .. } => {
            let t = expression(opts, scope, scopes, tbls, frozen, &inner.argument);
            Parsed::Eval(loc, Box::new(t), Box::new(Op::Update))
        }
        E::Sequence { inner, .. } => {
            let expr = inner.expressions.last().expect("unexpected empty sequence");
            expression(opts, scope, scopes, tbls, FrozenKind::NotFrozen, expr)
        }
        E::Assignment { inner, .. } => {
            match &inner.operator {
                None => {
                    // This is sketchy: the RHS may have side effects that are not tracked!
                    expression(opts, scope, scopes, tbls, frozen, &inner.right)
                }
                Some(_) => Parsed::Err(
                    loc.dupe(),
                    Errno::SigError(Box::new(
                        signature_error::SignatureError::UnexpectedExpression(
                            loc,
                            ast_utils::expression_sort::ExpressionSort::Assignment,
                        ),
                    )),
                ),
            }
        }
        E::Call { inner, .. } => {
            use ast::expression::ExpressionOrSpread;
            use ast::expression::member;

            // Special case 1: require("module")
            if let E::Identifier {
                inner: id_inner, ..
            } = &*inner.callee
            {
                if id_inner.name.as_str() == "require" {
                    // TODO: We should only special-case this logic if "require" is not aliased
                    // in the current scope. i.e., it should resolves to a builtin. The current
                    // signature builder does not do this, so we should probably fix that first.
                    let mref = match (&inner.targs, &inner.arguments.arguments[..]) {
                        (None, [ExpressionOrSpread::Expression(e)]) => extract_string_literal(e),
                        _ => None,
                    };

                    return match mref {
                        Some(mref) => {
                            let mref = Userland::from_smol_str(mref.clone());
                            let mref = tbls.push_module_ref(mref);
                            Parsed::Require { loc, mref }
                        }
                        None => {
                            // error cases: explicit targs / non-literal require
                            Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc))))
                        }
                    };
                }
            }

            // Special case 2: Object.freeze(object_literal)
            if let E::Member {
                inner: member_inner,
                ..
            } = inner.callee.deref()
            {
                if let (
                    E::Identifier {
                        inner: obj_id_inner,
                        ..
                    },
                    member::Property::PropertyIdentifier(freeze_id),
                ) = (member_inner.object.deref(), &member_inner.property)
                {
                    if obj_id_inner.name.as_str() == "Object"
                        && freeze_id.name.as_str() == "freeze"
                        && inner.targs.is_none()
                    {
                        // TODO: Similar to the "require" case above, we should only special-case
                        // this call "Object" is not in scope. Again, we should fix the existing
                        // signature builder first.
                        match &inner.arguments.arguments[..] {
                            [ExpressionOrSpread::Expression(obj_expr)] => {
                                // Case 2a: Object.freeze({...})
                                if let E::Object {
                                    inner: obj_inner, ..
                                } = obj_expr.deref()
                                {
                                    let obj_loc = tbls.push_loc(obj_expr.loc().dupe());
                                    return object_literal(
                                        opts,
                                        scope,
                                        scopes,
                                        tbls,
                                        true,
                                        obj_loc,
                                        obj_inner.as_ref(),
                                    );
                                }

                                // Case 2b: Object.freeze({...} as const)
                                // Treat `Object.freeze({[props]} as const)` as `{[props]} as const` since the
                                // latter provides stronger guarantees that subsume those of Object.freeze().
                                if let E::AsConstExpression {
                                    inner: as_const_inner,
                                    ..
                                } = obj_expr.deref()
                                {
                                    if matches!(as_const_inner.expression.deref(), E::Object { .. })
                                    {
                                        return expression(
                                            opts, scope, scopes, tbls, frozen, obj_expr,
                                        );
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            // Special case 3: keyMirror(object_literal)
            if let E::Identifier {
                inner: id_inner, ..
            } = &*inner.callee
            {
                if id_inner.name.as_str() == "keyMirror"
                    && opts.facebook_key_mirror
                    && inner.targs.is_none()
                {
                    // TODO: In general, "keyMirror" could resolve to anything. It might not be
                    // possible to ensure that it resolves to the expected function. If not,
                    // we should document this limitation along with the flag to enable this
                    // behavior.
                    if let [ExpressionOrSpread::Expression(obj_expr)] =
                        &inner.arguments.arguments[..]
                    {
                        if let E::Object {
                            inner: obj_inner, ..
                        } = &**obj_expr
                        {
                            let obj_loc = tbls.push_loc(obj_expr.loc().dupe());
                            return key_mirror(tbls, obj_loc, &obj_inner.properties);
                        }
                    }
                }
            }

            // Default case: unexpected call expression
            Parsed::Err(
                loc.dupe(),
                Errno::SigError(Box::new(
                    signature_error::SignatureError::UnexpectedExpression(
                        loc,
                        ast_utils::expression_sort::ExpressionSort::Call,
                    ),
                )),
            )
        }
        E::JSXElement { inner, .. } => jsx_element(opts, tbls, loc, inner.as_ref()),
        E::Import { inner, .. } => match extract_string_literal(&inner.argument) {
            None => Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(loc)))),
            Some(mref) => {
                let mref = Userland::from_smol_str(mref.clone());
                let mref = tbls.push_module_ref(mref);
                Parsed::ImportDynamic { loc, mref }
            }
        },
        E::Conditional { .. } => Parsed::Err(
            loc.dupe(),
            Errno::SigError(Box::new(
                signature_error::SignatureError::UnexpectedExpression(
                    loc,
                    ast_utils::expression_sort::ExpressionSort::Conditional,
                ),
            )),
        ),
        E::JSXFragment { .. } => Parsed::Err(
            loc.dupe(),
            Errno::SigError(Box::new(
                signature_error::SignatureError::UnexpectedExpression(
                    loc,
                    ast_utils::expression_sort::ExpressionSort::JSXFragment,
                ),
            )),
        ),
        E::Logical { .. } => Parsed::Err(
            loc.dupe(),
            Errno::SigError(Box::new(
                signature_error::SignatureError::UnexpectedExpression(
                    loc,
                    ast_utils::expression_sort::ExpressionSort::Logical,
                ),
            )),
        ),
        E::Match { .. } => Parsed::Err(
            loc.dupe(),
            Errno::SigError(Box::new(
                signature_error::SignatureError::UnexpectedExpression(
                    loc,
                    ast_utils::expression_sort::ExpressionSort::Match,
                ),
            )),
        ),
        E::MetaProperty { .. } => Parsed::Err(
            loc.dupe(),
            Errno::SigError(Box::new(
                signature_error::SignatureError::UnexpectedExpression(
                    loc,
                    ast_utils::expression_sort::ExpressionSort::MetaProperty,
                ),
            )),
        ),
        E::New { .. } => Parsed::Err(
            loc.dupe(),
            Errno::SigError(Box::new(
                signature_error::SignatureError::UnexpectedExpression(
                    loc,
                    ast_utils::expression_sort::ExpressionSort::New,
                ),
            )),
        ),
        E::OptionalCall { .. } => Parsed::Err(
            loc.dupe(),
            Errno::SigError(Box::new(
                signature_error::SignatureError::UnexpectedExpression(
                    loc,
                    ast_utils::expression_sort::ExpressionSort::OptionalCall,
                ),
            )),
        ),
        E::OptionalMember { .. } => Parsed::Err(
            loc.dupe(),
            Errno::SigError(Box::new(
                signature_error::SignatureError::UnexpectedExpression(
                    loc,
                    ast_utils::expression_sort::ExpressionSort::OptionalMember,
                ),
            )),
        ),
        E::Super { .. } => Parsed::Err(
            loc.dupe(),
            Errno::SigError(Box::new(
                signature_error::SignatureError::UnexpectedExpression(
                    loc,
                    ast_utils::expression_sort::ExpressionSort::Super,
                ),
            )),
        ),
        E::TaggedTemplate { .. } => Parsed::Err(
            loc.dupe(),
            Errno::SigError(Box::new(
                signature_error::SignatureError::UnexpectedExpression(
                    loc,
                    ast_utils::expression_sort::ExpressionSort::TaggedTemplate,
                ),
            )),
        ),
        E::This { .. } => Parsed::Err(
            loc.dupe(),
            Errno::SigError(Box::new(
                signature_error::SignatureError::UnexpectedExpression(
                    loc,
                    ast_utils::expression_sort::ExpressionSort::This,
                ),
            )),
        ),
        E::Yield { .. } => Parsed::Err(
            loc.dupe(),
            Errno::SigError(Box::new(
                signature_error::SignatureError::UnexpectedExpression(
                    loc,
                    ast_utils::expression_sort::ExpressionSort::Yield,
                ),
            )),
        ),
        E::Record { .. } => Parsed::Err(
            loc.dupe(),
            Errno::SigError(Box::new(
                signature_error::SignatureError::UnexpectedExpression(
                    loc,
                    ast_utils::expression_sort::ExpressionSort::Record,
                ),
            )),
        ),
    }
}

fn pattern<'arena: 'ast, 'ast, F>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    f: &mut F,
    def: PatternNode<'arena, 'ast>,
    patt: &'ast ast::pattern::Pattern<Loc, Loc>,
) where
    F: FnMut(
        &mut Tables<'arena, 'ast>,
        &mut scope::Scopes<'arena, 'ast>,
        LocNode<'arena>,
        FlowSmolStr,
        PatternNode<'arena, 'ast>,
    ),
{
    use ast::pattern::Pattern;
    match patt {
        Pattern::Identifier { inner: id, .. } => {
            let id_loc = tbls.push_loc(id.name.loc.dupe());
            let name = id.name.name.dupe();
            f(tbls, scopes, id_loc, name, def);
        }
        Pattern::Object { inner, .. } => {
            object_pattern(opts, scope, scopes, tbls, f, &def, &inner.properties);
        }
        Pattern::Array { inner, .. } => {
            array_pattern(opts, scope, scopes, tbls, f, &def, &inner.elements);
        }
        Pattern::Expression { .. } => {
            panic!("unexpected expression pattern");
        }
    }
}

fn object_pattern<'arena: 'ast, 'ast, F>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    f: &mut F,
    def: &PatternNode<'arena, 'ast>,
    props: &'ast [ast::pattern::object::Property<Loc, Loc>],
) where
    F: FnMut(
        &mut Tables<'arena, 'ast>,
        &mut scope::Scopes<'arena, 'ast>,
        LocNode<'arena>,
        FlowSmolStr,
        PatternNode<'arena, 'ast>,
    ),
{
    fn prop<'arena: 'ast, 'ast, F>(
        opts: &TypeSigOptions,
        scope: ScopeId,
        scopes: &mut scope::Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        f: &mut F,
        def: &PatternNode<'arena, 'ast>,
        xs: &mut Vec<FlowSmolStr>,
        property: &'ast ast::pattern::object::Property<Loc, Loc>,
    ) where
        F: FnMut(
            &mut Tables<'arena, 'ast>,
            &mut scope::Scopes<'arena, 'ast>,
            LocNode<'arena>,
            FlowSmolStr,
            PatternNode<'arena, 'ast>,
        ),
    {
        use ast::pattern::object as O;
        match property {
            O::Property::NormalProperty(prop) => {
                let def = match &prop.key {
                    O::Key::Identifier(id) => {
                        let IdentifierInner {
                            loc: id_loc, name, ..
                        } = &**id;
                        let id_loc = tbls.push_loc(id_loc.dupe());
                        let def = tbls.push_pattern(Pattern::PropP {
                            id_loc,
                            name: name.clone(),
                            def: def.dupe(),
                        });
                        xs.push(name.clone());
                        def
                    }
                    O::Key::StringLiteral((id_loc, ast::StringLiteral { value: name, .. })) => {
                        let id_loc = tbls.push_loc(id_loc.dupe());
                        let def = tbls.push_pattern(Pattern::PropP {
                            id_loc,
                            name: name.clone(),
                            def: def.dupe(),
                        });
                        xs.push(name.clone());
                        def
                    }
                    O::Key::Computed(computed) => {
                        let t = expression(
                            opts,
                            scope,
                            scopes,
                            tbls,
                            FrozenKind::NotFrozen,
                            &computed.expression,
                        );
                        let elem = tbls.push_pattern_def(t);
                        tbls.push_pattern(Pattern::ComputedP {
                            elem,
                            def: def.dupe(),
                        })
                    }
                    O::Key::NumberLiteral((loc, _)) | O::Key::BigIntLiteral((loc, _)) => {
                        let loc = tbls.push_loc(loc.dupe());
                        tbls.push_pattern(Pattern::UnsupportedLiteralP(loc))
                    }
                };
                pattern(opts, scope, scopes, tbls, f, def, &prop.pattern);
            }
            O::Property::RestElement(rest) => {
                let loc = tbls.push_loc(rest.loc.dupe());
                let def = tbls.push_pattern(Pattern::ObjRestP {
                    loc,
                    xs: std::mem::take(xs),
                    def: def.dupe(),
                });
                pattern(opts, scope, scopes, tbls, f, def, &rest.argument);
            }
        }
    }

    let mut xs = Vec::new();
    for p in props {
        prop(opts, scope, scopes, tbls, f, def, &mut xs, p);
    }
}

fn array_pattern<'arena: 'ast, 'ast, F>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    f: &mut F,
    def: &PatternNode<'arena, 'ast>,
    elements: &'ast [ast::pattern::array::Element<Loc, Loc>],
) where
    F: FnMut(
        &mut Tables<'arena, 'ast>,
        &mut scope::Scopes<'arena, 'ast>,
        LocNode<'arena>,
        FlowSmolStr,
        PatternNode<'arena, 'ast>,
    ),
{
    use ast::pattern::array as A;

    for (i, element) in elements.iter().enumerate() {
        match element {
            A::Element::Hole(_) => {}
            A::Element::NormalElement(el) => {
                let loc = tbls.push_loc(el.loc.dupe());
                let def = tbls.push_pattern(Pattern::IndexP {
                    loc,
                    i,
                    def: def.dupe(),
                });
                pattern(opts, scope, scopes, tbls, f, def, &el.argument);
            }
            A::Element::RestElement(rest) => {
                let loc = tbls.push_loc(rest.loc.dupe());
                let def = tbls.push_pattern(Pattern::ArrRestP {
                    loc,
                    i,
                    def: def.dupe(),
                });
                pattern(opts, scope, scopes, tbls, f, def, &rest.argument);
            }
        }
    }
}

fn member<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    obj: &'ast ast::expression::Expression<Loc, Loc>,
    toplevel_loc: LocNode<'arena>,
    prop: &'ast ast::expression::member::Property<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    fn prop_op<'arena: 'ast, 'ast>(
        opts: &TypeSigOptions,
        scope: ScopeId,
        scopes: &mut scope::Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        property: &'ast ast::expression::member::Property<Loc, Loc>,
    ) -> Op<Parsed<'arena, 'ast>> {
        match property {
            ast::expression::member::Property::PropertyIdentifier(id) => {
                Op::GetProp(Box::new(id_name(id).clone()))
            }
            ast::expression::member::Property::PropertyExpression(expr) => {
                let t = expression(opts, scope, scopes, tbls, FrozenKind::NotFrozen, expr);
                Op::GetElem(Box::new(t))
            }
            ast::expression::member::Property::PropertyPrivateName(_) => {
                panic!("unexpected private name outside class")
            }
        }
    }

    fn finish<'arena: 'ast, 'ast>(
        opts: &TypeSigOptions,
        scope: ScopeId,
        scopes: &mut scope::Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        mut t: Parsed<'arena, 'ast>,
        chain: Vec<(
            LocNode<'arena>,
            &'ast ast::expression::member::Property<Loc, Loc>,
        )>,
    ) -> Parsed<'arena, 'ast> {
        for (loc, property) in chain.into_iter().rev() {
            let op = prop_op(opts, scope, scopes, tbls, property);
            t = Parsed::Eval(loc, Box::new(t), Box::new(op));
        }
        t
    }

    let mut chain = vec![(toplevel_loc.dupe(), prop)];
    let mut expr = obj;
    loop {
        match expr.deref() {
            ExpressionInner::Identifier { inner, .. } => {
                let id_loc = tbls.push_loc(inner.loc.dupe());
                let name = inner.name.dupe();
                let t = val_ref(false, scope, id_loc, name);
                return finish(opts, scope, scopes, tbls, t, chain);
            }
            ExpressionInner::Member { loc, inner, .. } => {
                let loc = tbls.push_loc(loc.dupe());
                chain.push((loc, &inner.property));
                expr = &inner.object;
            }
            _ => {
                return Parsed::Err(
                    toplevel_loc.dupe(),
                    Errno::SigError(Box::new(
                        signature_error::SignatureError::UnexpectedExpression(
                            toplevel_loc,
                            ast_utils::expression_sort::ExpressionSort::Member,
                        ),
                    )),
                );
            }
        }
    }
}

fn param<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    tparams: &TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
    loc: &Loc,
    patt: &'ast ast::pattern::Pattern<Loc, Loc>,
    bind_names: bool,
    default: &Option<ast::expression::Expression<Loc, Loc>>,
) -> (Option<FlowSmolStr>, ScopeId, Parsed<'arena, 'ast>) {
    match patt {
        ast::pattern::Pattern::Identifier {
            inner: ident_pattern,
            ..
        } => {
            let id = &ident_pattern.name;
            let id_loc = tbls.push_loc(id.loc.dupe());
            let name = id.name.dupe();
            let t = annot_or_hint(
                ExpectedAnnotationSort::Identifier,
                Some(&id_loc),
                opts,
                scope,
                scopes,
                tbls,
                xs,
                &ident_pattern.annot,
            );
            let name_t = if ident_pattern.optional && default.is_none() {
                Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t.clone()))))
            } else {
                t.clone()
            };
            let t = if ident_pattern.optional || default.is_some() {
                Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))))
            } else {
                t
            };
            let scope = if bind_names {
                let scope = scope::push_lex(scopes, scope);
                scope::bind_param(
                    scope,
                    scopes,
                    tbls,
                    id_loc,
                    name.clone(),
                    tbls.lazy(Box::new(move |_, _, _| name_t)),
                    tparams.clone(),
                    |_, _, _| {},
                );
                scope
            } else {
                scope
            };
            (Some(name), scope, t)
        }
        ast::pattern::Pattern::Object { loc: _, inner } => {
            let annot = &inner.annot;
            let optional = inner.optional;
            let sort = ExpectedAnnotationSort::ObjectPattern;
            let loc_node = tbls.push_loc(loc.clone());

            let (scope, t) = if bind_names {
                let outer_scope = scope;
                let scope = scope::push_lex(scopes, scope);
                let shared_t: Rc<RefCell<Option<Parsed<'arena, 'ast>>>> =
                    Rc::new(RefCell::new(None));
                let pattern_def_node = {
                    let shared_t = shared_t.dupe();
                    tbls.lazy(Box::new(move |_, _, tbls: &mut Tables<'arena, 'ast>| {
                        let borrow = shared_t.borrow();
                        let t = borrow
                            .as_ref()
                            .expect("annot must be resolved before PDef is forced");
                        tbls.push_pattern_def(t.clone())
                    }))
                };
                let def = tbls.push_pattern(Pattern::PDef(pattern_def_node));
                pattern(
                    opts,
                    scope,
                    scopes,
                    tbls,
                    &mut |tbls, scopes, id_loc, name, p| {
                        let def = tbls.lazy(Box::new(move |_, _, _| Parsed::Pattern(p)));
                        scope::bind_var(
                            scope,
                            scopes,
                            tbls,
                            ast::VariableKind::Let,
                            id_loc,
                            name,
                            def,
                            |_, _, _| {},
                        );
                    },
                    def,
                    patt,
                );
                let t = annot_or_hint(
                    sort,
                    Some(&loc_node),
                    opts,
                    outer_scope,
                    scopes,
                    tbls,
                    xs,
                    annot,
                );
                *shared_t.borrow_mut() = Some(t.clone());
                (scope, t)
            } else {
                let t = annot_or_hint(sort, Some(&loc_node), opts, scope, scopes, tbls, xs, annot);
                (scope, t)
            };

            let t = if optional || default.is_some() {
                Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))))
            } else {
                t
            };
            (None, scope, t)
        }
        ast::pattern::Pattern::Array { loc: _, inner } => {
            let annot = &inner.annot;
            let optional = inner.optional;
            let sort = ExpectedAnnotationSort::ArrayPattern;
            let loc_node = tbls.push_loc(loc.clone());

            let (scope, t) = if bind_names {
                let outer_scope = scope;
                let scope = scope::push_lex(scopes, scope);
                let shared_t: Rc<RefCell<Option<Parsed<'arena, 'ast>>>> =
                    Rc::new(RefCell::new(None));
                let pattern_def_node = {
                    let shared_t = shared_t.clone();
                    tbls.lazy(Box::new(move |_, _, tbls: &mut Tables<'arena, 'ast>| {
                        let borrow = shared_t.borrow();
                        let t = borrow
                            .as_ref()
                            .expect("annot must be resolved before PDef is forced");
                        tbls.push_pattern_def(t.clone())
                    }))
                };
                let def = tbls.push_pattern(Pattern::PDef(pattern_def_node));
                pattern(
                    opts,
                    scope,
                    scopes,
                    tbls,
                    &mut |tbls, scopes, id_loc, name, p| {
                        let def = tbls.lazy(Box::new(move |_, _, _| Parsed::Pattern(p)));
                        scope::bind_var(
                            scope,
                            scopes,
                            tbls,
                            ast::VariableKind::Let,
                            id_loc,
                            name,
                            def,
                            |_, _, _| {},
                        );
                    },
                    def,
                    patt,
                );
                let t = annot_or_hint(
                    sort,
                    Some(&loc_node),
                    opts,
                    outer_scope,
                    scopes,
                    tbls,
                    xs,
                    annot,
                );
                *shared_t.borrow_mut() = Some(t.clone());
                (scope, t)
            } else {
                let t = annot_or_hint(sort, Some(&loc_node), opts, scope, scopes, tbls, xs, annot);
                (scope, t)
            };

            let t = if optional || default.is_some() {
                Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(t))))
            } else {
                t
            };
            (None, scope, t)
        }
        ast::pattern::Pattern::Expression { .. } => {
            panic!("unexpected expression pattern");
        }
    }
}

fn rest_param<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    tparams: &TParams<LocNode<'arena>, Parsed<'arena, 'ast>>,
    param_loc: &Loc,
    bind_names: bool,
    p: &ast::pattern::Pattern<Loc, Loc>,
) -> (
    Option<FunRestParam<LocNode<'arena>, Parsed<'arena, 'ast>>>,
    ScopeId,
) {
    use ast::pattern::Pattern;
    match p {
        Pattern::Identifier {
            inner: ident_pattern,
            ..
        } => {
            let loc = tbls.push_loc(param_loc.clone());
            let id = &ident_pattern.name;
            let name = Some(id.name.dupe());
            let id_loc = tbls.push_loc(id.loc.dupe());
            let t = annot_or_hint(
                ExpectedAnnotationSort::Identifier,
                Some(&id_loc),
                opts,
                scope,
                scopes,
                tbls,
                xs,
                &ident_pattern.annot,
            );
            let scope = if bind_names {
                let scope = scope::push_lex(scopes, scope);
                scope::bind_param(
                    scope,
                    scopes,
                    tbls,
                    loc.dupe(),
                    id.name.dupe(),
                    {
                        let t = t.clone();
                        tbls.lazy(Box::new(move |_, _, _| t))
                    },
                    tparams.clone(),
                    |_, _, _| {},
                );
                scope
            } else {
                scope
            };
            (Some(FunRestParam { name, loc, t }), scope)
        }
        Pattern::Object { inner, .. } => {
            let loc = tbls.push_loc(param_loc.clone());
            let t = annot_or_hint(
                ExpectedAnnotationSort::ObjectPattern,
                Some(&loc),
                opts,
                scope,
                scopes,
                tbls,
                xs,
                &inner.annot,
            );
            (Some(FunRestParam { name: None, loc, t }), scope)
        }
        Pattern::Array { inner, .. } => {
            let loc = tbls.push_loc(param_loc.clone());
            let t = annot_or_hint(
                ExpectedAnnotationSort::ArrayPattern,
                Some(&loc),
                opts,
                scope,
                scopes,
                tbls,
                xs,
                &inner.annot,
            );
            (Some(FunRestParam { name: None, loc, t }), scope)
        }
        Pattern::Expression { .. } => (None, scope),
    }
}

fn function_def_helper<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    constructor: bool,
    fn_loc: LocNode<'arena>,
    f: &'ast ast::function::Function<Loc, Loc>,
) -> FunSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
    let ast::function::Function {
        id,
        tparams: tps,
        params:
            ast::function::Params {
                params: ps,
                rest: rp,
                this_,
                ..
            },
        body,
        return_: r,
        predicate: _,
        async_,
        generator,
        effect_,
        ..
    } = f;
    let tparams = tparams(opts, scope, scopes, tbls, xs, tps.as_ref());
    let this_param = this_.as_ref().map(
        |ast::function::ThisParam {
             annot: this_annot, ..
         }| annot(opts, scope, scopes, tbls, xs, &this_annot.annotation),
    );
    let mut params_vec = Vec::new();
    let mut param_scope = scope;
    for fn_param in ps.iter() {
        let (loc, patt, default) = match fn_param {
            ast::function::Param::RegularParam {
                loc,
                argument,
                default,
            } => (loc, argument, default),
            ast::function::Param::ParamProperty { .. } => {
                // Parameter properties not supported, skip
                continue;
            }
        };
        let (name, new_scope, t) = param(
            opts,
            param_scope,
            scopes,
            tbls,
            xs,
            &tparams,
            loc,
            patt,
            true,
            default,
        );
        params_vec.push(FunParam { name, t });
        param_scope = new_scope;
    }
    let (rest_param_opt, rest_scope) = match rp {
        Some(ast::function::RestParam {
            loc: param_loc,
            argument: p,
            ..
        }) => {
            let (rp, scope) = rest_param(
                opts,
                param_scope,
                scopes,
                tbls,
                xs,
                &tparams,
                param_loc,
                true,
                p,
            );
            (rp, scope)
        }
        None => (None, param_scope),
    };
    let (return_, type_guard_opt) = if constructor {
        let loc_node = match r {
            ast::function::ReturnAnnot::Missing(l) => tbls.push_loc(l.dupe()),
            ast::function::ReturnAnnot::Available(a) => tbls.push_loc(a.loc.dupe()),
            ast::function::ReturnAnnot::TypeGuard(tg) => tbls.push_loc(tg.loc.dupe()),
        };
        (
            Parsed::Annot(Box::new(ParsedAnnot::Void(Box::new(loc_node)))),
            None,
        )
    } else {
        match r {
            ast::function::ReturnAnnot::Available(a) => (
                annot(opts, rest_scope, scopes, tbls, xs, &a.annotation),
                None,
            ),
            ast::function::ReturnAnnot::Missing(loc) => {
                let loc_node = tbls.push_loc(loc.dupe());
                if *generator || !signature_utils::procedure_decider::is_procedure(body) {
                    let err = Parsed::Err(
                        loc_node.dupe(),
                        Errno::SigError(Box::new(
                            signature_error::SignatureError::ExpectedAnnotation(
                                loc_node,
                                ExpectedAnnotationSort::FunctionReturn,
                            ),
                        )),
                    );
                    (err, None)
                } else if *async_ {
                    (Parsed::AsyncVoidReturn(loc_node), None)
                } else {
                    (
                        Parsed::Annot(Box::new(ParsedAnnot::Void(Box::new(loc_node)))),
                        None,
                    )
                }
            }
            ast::function::ReturnAnnot::TypeGuard(guard_annot) => {
                let loc_node = tbls.push_loc(guard_annot.loc.dupe());
                let guard_opt =
                    type_guard_opt(opts, rest_scope, scopes, tbls, xs, &guard_annot.guard);
                (
                    type_guard_return_annot(guard_annot.guard.kind, loc_node),
                    guard_opt,
                )
            }
        }
    };
    let type_guard = type_guard_opt.map(|(loc, param_name, type_guard, kind)| TypeGuard {
        loc,
        param_name,
        type_guard,
        kind,
    });
    let effect_ = convert_effect(opts, effect_, Some(fn_loc), id.as_ref().map(|id| &id.name));
    FunSig {
        tparams,
        params: params_vec,
        rest_param: rest_param_opt,
        this_param,
        return_,
        type_guard,
        effect_,
        strictness_kind: tbls.strictness_kind,
    }
}

fn function_def<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    fn_loc: LocNode<'arena>,
    f: &'ast ast::function::Function<Loc, Loc>,
) -> FunSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
    function_def_helper(opts, scope, scopes, tbls, xs, false, fn_loc, f)
}

fn constructor_def<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    xs: &mut tparam_stack::TParamStack,
    fn_loc: LocNode<'arena>,
    f: &'ast ast::function::Function<Loc, Loc>,
) -> FunSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
    function_def_helper(opts, scope, scopes, tbls, xs, true, fn_loc, f)
}

fn component_sig_helper<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    tps: &Option<ast::types::TypeParams<Loc, Loc>>,
    params: &'ast ast::statement::component_params::Params<Loc, Loc>,
    r: &ast::types::ComponentRendersAnnotation<Loc, Loc>,
) -> ComponentSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
    let ast::statement::component_params::Params {
        loc,
        params: ps,
        rest: rp,
        ..
    } = params;

    let mut xs = tparam_stack::TParamStack::new();
    let tparams = tparams(opts, scope, scopes, tbls, &mut xs, tps.as_ref());
    let params_loc = tbls.push_loc(loc.clone());
    let mut params_vec = Vec::new();
    for ast::statement::component_params::Param {
        loc: param_loc,
        name,
        local: patt,
        default,
        ..
    } in ps.iter()
    {
        let (param_name, name_loc) = match name {
            ast::statement::component_params::ParamName::Identifier(id) => {
                (id.name.dupe(), id.loc.dupe())
            }
            ast::statement::component_params::ParamName::StringLiteral((loc, lit)) => {
                (lit.value.dupe(), loc.dupe())
            }
        };
        let name_loc = tbls.push_loc(name_loc);
        let (_, _, t) = param(
            opts, scope, scopes, tbls, &mut xs, &tparams, param_loc, patt, false, default,
        );

        params_vec.push(ComponentParam {
            name: param_name,
            name_loc,
            t,
        });
    }
    let rest_param_opt = match rp {
        Some(ast::statement::component_params::RestParam {
            loc: param_loc,
            argument: patt,
            ..
        }) => {
            let (rp_opt, _scope) = rest_param(
                opts, scope, scopes, tbls, &mut xs, &tparams, param_loc, false, // bind_names
                patt,
            );
            rp_opt.map(|rp| ComponentRestParam { t: rp.t })
        }
        None => None,
    };
    let renders = match r {
        ast::types::ComponentRendersAnnotation::AvailableRenders(loc, rt) => {
            let loc_node = tbls.push_loc(loc.clone());
            let arg = annot(opts, scope, scopes, tbls, &mut xs, &rt.argument);
            Parsed::Annot(Box::new(ParsedAnnot::Renders(Box::new(AnnotRenders {
                loc: loc_node,
                arg,
                variant: rt.variant,
            }))))
        }
        ast::types::ComponentRendersAnnotation::MissingRenders(loc) => {
            let loc_node = tbls.push_loc(loc.clone());
            Parsed::Annot(Box::new(ParsedAnnot::ComponentMissingRenders(Box::new(
                loc_node,
            ))))
        }
    };
    ComponentSig {
        params_loc,
        tparams,
        params: params_vec,
        rest_param: rest_param_opt,
        renders,
        strictness_kind: tbls.strictness_kind,
    }
}

fn component_def<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    f: &'ast ast::statement::ComponentDeclaration<Loc, Loc>,
) -> ComponentSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
    let ast::statement::ComponentDeclaration {
        tparams: tps,
        params,
        renders: r,
        ..
    } = f;

    component_sig_helper(opts, scope, scopes, tbls, tps, params, r)
}

fn declare_component_def<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    f: &'ast ast::statement::DeclareComponent<Loc, Loc>,
) -> ComponentSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
    let ast::statement::DeclareComponent {
        id: _,
        tparams: tps,
        params,
        renders: r,
        ..
    } = f;

    component_sig_helper(opts, scope, scopes, tbls, tps, params, r)
}

fn class_def<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    c: &'ast ast::class::Class<Loc, Loc>,
) -> ClassSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
    use ast::class;
    use ast::expression::object::Key as ObjKey;
    use class_acc::ClassAcc;

    let is_ts_private =
        |ts_accessibility: &Option<ast::class::ts_accessibility::TSAccessibility<Loc>>| {
            matches!(
                ts_accessibility,
                Some(ast::class::ts_accessibility::TSAccessibility {
                    kind: ast::class::ts_accessibility::Kind::Private,
                    ..
                })
            )
        };

    let ast::class::Class {
        body,
        tparams: tps,
        extends,
        implements,
        abstract_,
        ..
    } = c;

    let abstract_ = *abstract_ && opts.tslib_syntax;
    let mut xs = tparam_stack::TParamStack::new();
    let tparams = tparams(opts, scope, scopes, tbls, &mut xs, tps.as_ref());
    xs.insert(FlowSmolStr::new_inline("this"));
    let extends = match extends {
        None => ClassExtends::ClassImplicitExtends,
        Some(extends_info) => {
            let loc_node = tbls.push_loc(extends_info.loc.dupe());
            let t = expression(
                opts,
                scope,
                scopes,
                tbls,
                FrozenKind::NotFrozen,
                &extends_info.expr,
            );
            match &extends_info.targs {
                None => ClassExtends::ClassExplicitExtends(Box::new((loc_node, t))),
                Some(type_args) => {
                    let targs = type_args
                        .arguments
                        .iter()
                        .map(|arg| annot(opts, scope, scopes, tbls, &mut xs, arg))
                        .collect();
                    ClassExtends::ClassExplicitExtendsApp(Box::new((loc_node, t, targs)))
                }
            }
        }
    };
    let implements = class_implements(opts, scope, scopes, tbls, &mut xs, implements);
    let class_tparam_names: Vec<FlowSmolStr> = tps
        .iter()
        .flat_map(|tps| tps.params.iter())
        .map(|tp| tp.name.name.dupe())
        .collect();
    let mut acc = ClassAcc::empty();
    for element in body.body.iter() {
        // Static members cannot reference the type parameters of the class.
        let static_ = match element {
            class::BodyElement::Method(method) => method.static_,
            class::BodyElement::Property(prop) => prop.static_,
            class::BodyElement::PrivateField(field) => field.static_,
            class::BodyElement::DeclareMethod(method) => method.static_,
            class::BodyElement::StaticBlock(_) => true,
            class::BodyElement::AbstractMethod(_) | class::BodyElement::AbstractProperty(_) => {
                false
            }
            class::BodyElement::IndexSignature(indexer) => indexer.static_,
        };
        for name in &class_tparam_names {
            if static_ {
                xs.insert_uninitialized(name.dupe());
            } else {
                xs.initialize(name);
            }
        }
        match element {
            class::BodyElement::Method(class::Method {
                key,
                value: (_val_loc, fn_val),
                kind,
                static_: is_static,
                ts_accessibility,
                loc: fn_loc,
                ..
            }) => {
                if let ObjKey::Identifier(id) = key {
                    if opts.munge
                        && flow_parser_utils::signature_utils::is_munged_property_string(&id.name)
                    {
                        continue;
                    } else if is_ts_private(ts_accessibility)
                        && *kind != class::MethodKind::Constructor
                    {
                        // TS-private non-constructor members have no public interface.
                        // Private constructors are kept so the merge layer doesn't
                        // synthesize a default zero-arg constructor (which would make
                        // `class C { private constructor(x: number) }` spuriously
                        // constructible via `new C()`).
                        continue;
                    }
                    let name = id.name.dupe();
                    let fn_loc_node = tbls.push_loc(fn_loc.dupe());
                    let id_loc = tbls.push_loc(id.loc.dupe());
                    let async_ = fn_val.async_;
                    let generator = fn_val.generator;
                    let uses_this = *is_static
                        && *kind == class::MethodKind::Method
                        && flow_parser_utils::this_finder::function_uses_this(fn_val);
                    match kind {
                        class::MethodKind::Method => {
                            let def = function_def(
                                opts,
                                scope,
                                scopes,
                                tbls,
                                &mut xs,
                                fn_loc_node.dupe(),
                                fn_val,
                            );
                            acc.add_method(
                                *is_static,
                                name,
                                id_loc,
                                fn_loc_node,
                                async_,
                                generator,
                                uses_this,
                                def,
                            );
                        }
                        class::MethodKind::Constructor => {
                            let def = constructor_def(
                                opts,
                                scope,
                                scopes,
                                tbls,
                                &mut xs,
                                fn_loc_node.dupe(),
                                fn_val,
                            );
                            acc.add_method(
                                *is_static,
                                name,
                                id_loc,
                                fn_loc_node,
                                async_,
                                generator,
                                uses_this,
                                def,
                            );
                        }
                        class::MethodKind::Get => {
                            let getter =
                                getter_def(opts, scope, scopes, tbls, &mut xs, id_loc, fn_val);
                            acc.add_accessor(*is_static, name, getter);
                        }
                        class::MethodKind::Set => {
                            let setter =
                                setter_def(opts, scope, scopes, tbls, &mut xs, id_loc, fn_val);
                            acc.add_accessor(*is_static, name, setter);
                        }
                    }
                } else if let ObjKey::Computed(ck) = key {
                    // Only a plain method whose computed key resolves to a literal
                    // string or number (directly or through a value reference) is
                    // recorded. Computed getters/setters and non-literal keys are
                    // reported by the typing pipeline, so they are dropped here.
                    if *kind == class::MethodKind::Method && !is_ts_private(ts_accessibility) {
                        let async_ = fn_val.async_;
                        let generator = fn_val.generator;
                        let uses_this = *is_static
                            && flow_parser_utils::this_finder::function_uses_this(fn_val);
                        match ck.expression.deref() {
                            // Computed keys are never munged: the name-munging
                            // transform only rewrites non-computed identifier keys, so
                            // a computed underscore-prefixed key stays public at runtime.
                            ExpressionInner::StringLiteral { loc, inner, .. } => {
                                let fn_loc_node = tbls.push_loc(fn_loc.dupe());
                                let id_loc = tbls.push_loc(loc.dupe());
                                let def = function_def(
                                    opts,
                                    scope,
                                    scopes,
                                    tbls,
                                    &mut xs,
                                    fn_loc_node.dupe(),
                                    fn_val,
                                );
                                acc.add_method(
                                    *is_static,
                                    inner.value.dupe(),
                                    id_loc,
                                    fn_loc_node,
                                    async_,
                                    generator,
                                    uses_this,
                                    def,
                                );
                            }
                            ExpressionInner::NumberLiteral { loc, inner, .. }
                                if flow_common::js_number::is_float_safe_integer(inner.value) =>
                            {
                                let name = FlowSmolStr::new(
                                    flow_common::js_number::ecma_string_of_float(inner.value),
                                );
                                let fn_loc_node = tbls.push_loc(fn_loc.dupe());
                                let id_loc = tbls.push_loc(loc.dupe());
                                let def = function_def(
                                    opts,
                                    scope,
                                    scopes,
                                    tbls,
                                    &mut xs,
                                    fn_loc_node.dupe(),
                                    fn_val,
                                );
                                acc.add_method(
                                    *is_static,
                                    name,
                                    id_loc,
                                    fn_loc_node,
                                    async_,
                                    generator,
                                    uses_this,
                                    def,
                                );
                            }
                            ExpressionInner::Identifier { inner, .. } => {
                                let fn_loc_node = tbls.push_loc(fn_loc.dupe());
                                let ref_loc = tbls.push_loc(inner.loc.dupe());
                                let key_ref =
                                    val_ref(false, scope, ref_loc.dupe(), inner.name.dupe());
                                let def = function_def(
                                    opts,
                                    scope,
                                    scopes,
                                    tbls,
                                    &mut xs,
                                    fn_loc_node.dupe(),
                                    fn_val,
                                );
                                acc.add_computed_method(
                                    *is_static,
                                    key_ref,
                                    ref_loc,
                                    fn_loc_node,
                                    async_,
                                    generator,
                                    uses_this,
                                    def,
                                );
                            }
                            // Any other key (`[o.p]`, `[(m as 'go')]`, ...) is
                            // recorded as an expression resolved during merge.
                            // `expression_for_computed_key` yields an error key for
                            // anything it cannot represent, so an exported class fails
                            // signature verification rather than silently dropping a
                            // method the checker accepted.
                            _ => {
                                let fn_loc_node = tbls.push_loc(fn_loc.dupe());
                                let ref_loc = tbls.push_loc(ck.expression.loc().dupe());
                                let key_ref =
                                    expression_for_computed_key(scope, tbls, &ck.expression);
                                let def = function_def(
                                    opts,
                                    scope,
                                    scopes,
                                    tbls,
                                    &mut xs,
                                    fn_loc_node.dupe(),
                                    fn_val,
                                );
                                acc.add_computed_method(
                                    *is_static,
                                    key_ref,
                                    ref_loc,
                                    fn_loc_node,
                                    async_,
                                    generator,
                                    uses_this,
                                    def,
                                );
                            }
                        }
                    }
                }
                // Skip literal, bigint, and private-name keys.
            }
            class::BodyElement::Property(class::Property {
                key,
                annot: t,
                value,
                static_: is_static,
                variance,
                optional,
                ts_accessibility,
                loc: prop_loc,
                ..
            }) => {
                if is_ts_private(ts_accessibility) {
                    // TS-private members have no public interface.
                    continue;
                }
                let polarity_val = polarity(variance.as_ref().map(|v| (v.loc.dupe(), v.clone())));
                match key {
                    ObjKey::Identifier(id) => {
                        if opts.munge
                            && flow_parser_utils::signature_utils::is_munged_property_string(
                                &id.name,
                            )
                        {
                            continue;
                        }

                        let name = id.name.dupe();
                        let (id_loc_node, parsed_t) = match t {
                            ast::types::AnnotationOrHint::Available(annot_t) => {
                                let id_loc_node = tbls.push_loc(id.loc.dupe());
                                let parsed =
                                    annot(opts, scope, scopes, tbls, &mut xs, &annot_t.annotation);
                                (id_loc_node, parsed)
                            }
                            ast::types::AnnotationOrHint::Missing(_) => {
                                let prop_loc_node = tbls.push_loc(prop_loc.dupe());
                                let id_loc_node = tbls.push_loc(id.loc.dupe());
                                let res = match value {
                                    class::property::Value::Initialized(e) => expression(
                                        opts,
                                        scope,
                                        scopes,
                                        tbls,
                                        FrozenKind::NotFrozen,
                                        e,
                                    ),
                                    class::property::Value::Declared
                                    | class::property::Value::Uninitialized => Parsed::Err(
                                        prop_loc_node.dupe(),
                                        Errno::SigError(Box::new(
                                            signature_error::SignatureError::ExpectedAnnotation(
                                                prop_loc_node,
                                                ExpectedAnnotationSort::Property {
                                                    name: name.clone(),
                                                },
                                            ),
                                        )),
                                    ),
                                };
                                (id_loc_node, res)
                            }
                        };
                        acc.add_field(
                            *is_static,
                            name,
                            id_loc_node,
                            polarity_val,
                            optional_class_field(*optional, parsed_t),
                        );
                    }
                    // Match the typing pipeline (statement.rs): a computed key is a
                    // field only when it resolves to a single literal string or
                    // safe-integer number, either directly or through a value
                    // reference. bigint keys, symbol keys, and everything else are
                    // unsupported and reported at the declaration site, so they are
                    // dropped here rather than emitting a duplicate signature error.
                    ObjKey::Computed(ck) => match ck.expression.deref() {
                        // Computed keys are never munged: the name-munging transform
                        // only rewrites non-computed identifier keys, so a computed
                        // underscore-prefixed key stays public at runtime.
                        ExpressionInner::StringLiteral { loc, inner, .. } => {
                            let name = inner.value.dupe();
                            let id_loc_node = tbls.push_loc(loc.dupe());
                            let parsed_t = class_field_value_type(
                                opts,
                                scope,
                                scopes,
                                tbls,
                                &mut xs,
                                t,
                                value,
                                id_loc_node.dupe(),
                                &name,
                            );
                            acc.add_field(
                                *is_static,
                                name,
                                id_loc_node,
                                polarity_val,
                                optional_class_field(*optional, parsed_t),
                            );
                        }
                        ExpressionInner::NumberLiteral { loc, inner, .. }
                            if flow_common::js_number::is_float_safe_integer(inner.value) =>
                        {
                            let name = FlowSmolStr::new(
                                flow_common::js_number::ecma_string_of_float(inner.value),
                            );
                            let id_loc_node = tbls.push_loc(loc.dupe());
                            let parsed_t = class_field_value_type(
                                opts,
                                scope,
                                scopes,
                                tbls,
                                &mut xs,
                                t,
                                value,
                                id_loc_node.dupe(),
                                &name,
                            );
                            acc.add_field(
                                *is_static,
                                name,
                                id_loc_node,
                                polarity_val,
                                optional_class_field(*optional, parsed_t),
                            );
                        }
                        // `[s]` where `s` is a value reference: record the key ref
                        // and resolve it to a name during signature merge.
                        ExpressionInner::Identifier { inner, .. } => {
                            let ref_loc = tbls.push_loc(inner.loc.dupe());
                            let key_ref = val_ref(false, scope, ref_loc.dupe(), inner.name.dupe());
                            let parsed_t = class_field_value_type(
                                opts,
                                scope,
                                scopes,
                                tbls,
                                &mut xs,
                                t,
                                value,
                                ref_loc.dupe(),
                                &inner.name,
                            );
                            acc.add_computed_field(
                                *is_static,
                                key_ref,
                                ref_loc,
                                polarity_val,
                                optional_class_field(*optional, parsed_t),
                            );
                        }
                        // Any other key (`[o.p]`, `[(s as 'foo')]`, ...) is recorded
                        // as an expression resolved during merge. `expression_for_computed_key`
                        // yields a resolvable reference for member expressions and literals,
                        // and an error key for anything it cannot represent. The error key
                        // makes an exported class fail signature verification rather than
                        // silently dropping a member the checker accepted. bigint and
                        // non-safe-integer number literals resolve to a non-name at merge and
                        // are dropped, matching the unsupported-syntax error the typing
                        // pipeline already reports for them.
                        _ => {
                            let ref_loc = tbls.push_loc(ck.expression.loc().dupe());
                            let key_ref = expression_for_computed_key(scope, tbls, &ck.expression);
                            let parsed_t = class_field_value_type(
                                opts,
                                scope,
                                scopes,
                                tbls,
                                &mut xs,
                                t,
                                value,
                                ref_loc.dupe(),
                                &FlowSmolStr::new_inline("[computed]"),
                            );
                            acc.add_computed_field(
                                *is_static,
                                key_ref,
                                ref_loc,
                                polarity_val,
                                optional_class_field(*optional, parsed_t),
                            );
                        }
                    },
                    // Skip string/number/bigint literal and private-name keys:
                    // these are unsupported class members flagged by the typing
                    // pipeline.
                    _ => {}
                }
            }
            class::BodyElement::PrivateField(_) => {
                // Private fields are unreachable from exports
            }
            class::BodyElement::StaticBlock(_) => {
                // Static blocks are unreachable from exports
            }
            class::BodyElement::DeclareMethod(class::DeclareMethod {
                kind,
                key: ObjKey::Identifier(id),
                annot:
                    ast::types::Annotation {
                        loc: annot_loc,
                        annotation,
                    },
                static_: is_static,
                optional,
                ..
            }) => {
                if let TypeInner::Function { inner: f, .. } = annotation.deref() {
                    if opts.munge
                        && flow_parser_utils::signature_utils::is_munged_property_string(&id.name)
                    {
                        continue;
                    }
                    let name = id.name.dupe();
                    if *optional {
                        let id_loc_node = tbls.push_loc(id.loc.dupe());
                        let fn_loc_node = tbls.push_loc(annot_loc.dupe());
                        let def =
                            function_type(true, opts, scope, scopes, tbls, &mut xs, f.as_ref());
                        let t = Parsed::Annot(Box::new(ParsedAnnot::Optional(Box::new(
                            Parsed::Annot(Box::new(ParsedAnnot::MethodAnnot(Box::new((
                                fn_loc_node,
                                *is_static,
                                def,
                            ))))),
                        ))));
                        let polarity = Polarity::Neutral;
                        acc.add_field(*is_static, name, id_loc_node, polarity, t);
                    } else {
                        match kind {
                            class::MethodKind::Get => {
                                let id_loc_node = tbls.push_loc(id.loc.dupe());
                                let getter = getter_type(
                                    opts,
                                    scope,
                                    scopes,
                                    tbls,
                                    &mut xs,
                                    id_loc_node,
                                    f.as_ref(),
                                );
                                acc.add_accessor(*is_static, name, getter);
                            }
                            class::MethodKind::Set => {
                                let id_loc_node = tbls.push_loc(id.loc.dupe());
                                let setter = setter_type(
                                    opts,
                                    scope,
                                    scopes,
                                    tbls,
                                    &mut xs,
                                    id_loc_node,
                                    f.as_ref(),
                                );
                                acc.add_accessor(*is_static, name, setter);
                            }
                            class::MethodKind::Method | class::MethodKind::Constructor => {
                                let id_loc_node = tbls.push_loc(id.loc.dupe());
                                let fn_loc_node = tbls.push_loc(annot_loc.dupe());
                                let def = function_type(
                                    true,
                                    opts,
                                    scope,
                                    scopes,
                                    tbls,
                                    &mut xs,
                                    f.as_ref(),
                                );
                                acc.add_method(
                                    *is_static,
                                    name,
                                    id_loc_node,
                                    fn_loc_node,
                                    false,
                                    false,
                                    false,
                                    def,
                                );
                            }
                        }
                    }
                }
            }
            class::BodyElement::DeclareMethod(_) => {
                // unsupported DeclareMethod key types
            }
            class::BodyElement::AbstractMethod(class::AbstractMethod {
                key: ObjKey::Identifier(id),
                annot: (fn_loc, f),
                ts_accessibility,
                ..
            }) if opts.tslib_syntax => {
                if (opts.munge
                    && flow_parser_utils::signature_utils::is_munged_property_string(&id.name))
                    || is_ts_private(ts_accessibility)
                {
                    continue;
                }
                let name = id.name.dupe();
                let id_loc = tbls.push_loc(id.loc.dupe());
                let fn_loc = tbls.push_loc(fn_loc.dupe());
                let def = function_type(true, opts, scope, scopes, tbls, &mut xs, f);
                acc.add_method(false, name.dupe(), id_loc, fn_loc, false, false, false, def);
                acc.add_abstract_name(name);
            }
            class::BodyElement::AbstractMethod(_) => {
                // Reached when [tslib_syntax] is off OR when the key is
                // non-Identifier (computed / string-literal / number-literal).
                // Match the precedent for regular Method/Property with computed
                // keys above (silent drop): the type-sig pipeline doesn't carry
                // a slot for non-Identifier abstract members, so cross-module
                // consumers won't see the obligation. The typing pipeline
                // ([statement.rs]) still flags abstract-modifier issues at the
                // declaration site within the file.
            }
            class::BodyElement::AbstractProperty(class::AbstractProperty {
                key: ObjKey::Identifier(id),
                annot: t,
                ts_accessibility,
                variance,
                ..
            }) if opts.tslib_syntax => {
                if (opts.munge
                    && flow_parser_utils::signature_utils::is_munged_property_string(&id.name))
                    || is_ts_private(ts_accessibility)
                {
                    continue;
                }
                let name = id.name.dupe();
                let id_loc = tbls.push_loc(id.loc.dupe());
                let t = match t {
                    ast::types::AnnotationOrHint::Available(annot_t) => {
                        annot(opts, scope, scopes, tbls, &mut xs, &annot_t.annotation)
                    }
                    ast::types::AnnotationOrHint::Missing(_) => {
                        // Require an annotation on abstract properties (matches
                        // statement.rs's mk_field, which emits EMissingLocalAnnotation
                        // on the same shape). Silent fallback to [any] would let
                        // cross-module consumers see [any] for a member the local
                        // pipeline correctly rejects.
                        Parsed::Err(
                            id_loc.dupe(),
                            Errno::SigError(Box::new(
                                signature_error::SignatureError::ExpectedAnnotation(
                                    id_loc.dupe(),
                                    ExpectedAnnotationSort::Property { name: name.clone() },
                                ),
                            )),
                        )
                    }
                };
                let polarity_val = polarity(variance.as_ref().map(|v| (v.loc.dupe(), v.clone())));
                acc.add_field(false, name.dupe(), id_loc, polarity_val, t);
                acc.add_abstract_name(name);
            }
            class::BodyElement::AbstractProperty(_) => {
                // See AbstractMethod above — silent drop matches the precedent
                // for regular Property with computed keys.
            }
            class::BodyElement::IndexSignature(p) => {
                let static_ = p.static_;
                let i = indexer(opts, scope, scopes, tbls, &mut xs, p);
                acc.add_indexer(static_, i);
            }
        }
    }

    acc.class_def(
        abstract_,
        tparams,
        extends,
        implements,
        tbls.strictness_kind,
    )
}

fn object_literal<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    frozen: bool,
    loc: LocNode<'arena>,
    obj: &'ast ast::expression::Object<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    use ast::expression::object::Key;
    use ast::expression::object::NormalProperty;
    use ast::expression::object::Property;
    use object_literal_acc::ObjectLiteralAcc;

    fn prop<'arena: 'ast, 'ast>(
        frozen: bool,
        opts: &TypeSigOptions,
        scope: ScopeId,
        scopes: &mut scope::Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        acc: &mut ObjectLiteralAcc<'arena, 'ast>,
        prop_loc: &Loc,
        p: &'ast NormalProperty<Loc, Loc>,
    ) {
        let (frozen_inner, polarity) = if frozen {
            (FrozenKind::FrozenProp, Polarity::Positive)
        } else {
            (FrozenKind::NotFrozen, Polarity::Neutral)
        };
        match p {
            NormalProperty::Init { key, value, .. } => {
                let (id_loc, name) = match key {
                    Key::Identifier(id) => {
                        let IdentifierInner {
                            loc: id_loc, name, ..
                        } = &**id;
                        (id_loc.dupe(), name.clone())
                    }
                    Key::StringLiteral((id_loc, ast::StringLiteral { value: name, .. })) => {
                        (id_loc.dupe(), name.clone())
                    }
                    Key::NumberLiteral((
                        id_loc,
                        ast::NumberLiteral {
                            value: num_value, ..
                        },
                    )) if flow_common::js_number::is_float_safe_integer(*num_value) => {
                        let name = FlowSmolStr::new(flow_common::js_number::ecma_string_of_float(
                            *num_value,
                        ));
                        (id_loc.dupe(), name)
                    }
                    Key::NumberLiteral(_) | Key::BigIntLiteral(_) => {
                        // unsupported literal key: BigIntLiteral or non-safe-integer NumberLiteral
                        return;
                    }
                    Key::Computed(_) => {
                        panic!("unexpected computed field key")
                    }
                    Key::PrivateName(_) => {
                        panic!("unexpected private field in object literal")
                    }
                };
                let id_loc = tbls.push_loc(id_loc);
                let value_loc = tbls.push_loc(value.loc().dupe());
                let t = expression(opts, scope, scopes, tbls, frozen_inner, value);
                if name.as_str() == "__proto__" {
                    acc.add_proto((value_loc, t));
                } else {
                    acc.add_field(name, id_loc, t, polarity);
                }
            }
            NormalProperty::Method {
                loc: _,
                key,
                value: (_, fn_expr),
            } => {
                let (id_loc, name) = match key {
                    Key::Identifier(id) => {
                        let IdentifierInner {
                            loc: id_loc, name, ..
                        } = &**id;
                        (id_loc.dupe(), name.clone())
                    }
                    Key::StringLiteral((id_loc, ast::StringLiteral { value: name, .. })) => {
                        (id_loc.dupe(), name.clone())
                    }
                    Key::NumberLiteral((
                        id_loc,
                        ast::NumberLiteral {
                            value: num_value, ..
                        },
                    )) if flow_common::js_number::is_float_safe_integer(*num_value) => {
                        let name = FlowSmolStr::new(flow_common::js_number::ecma_string_of_float(
                            *num_value,
                        ));
                        (id_loc.dupe(), name)
                    }
                    Key::NumberLiteral(_) | Key::BigIntLiteral(_) => {
                        // unsupported literal key: BigIntLiteral or non-safe-integer NumberLiteral
                        return;
                    }
                    Key::Computed(_) => {
                        panic!("unexpected computed field key")
                    }
                    Key::PrivateName(_) => {
                        panic!("unexpected private field in object literal")
                    }
                };
                let fn_loc = tbls.push_loc(prop_loc.dupe());
                let id_loc = tbls.push_loc(id_loc);
                let async_ = fn_expr.async_;
                let generator = fn_expr.generator;
                let def = function_def(
                    opts,
                    scope,
                    scopes,
                    tbls,
                    &mut tparam_stack::TParamStack::new(),
                    fn_loc.dupe(),
                    fn_expr,
                );
                acc.add_method(name, id_loc, fn_loc, async_, generator, def);
            }
            NormalProperty::Get {
                key,
                value: (_, fn_expr),
                ..
            } => {
                let (id_loc, name) = match key {
                    Key::Identifier(id) => {
                        let IdentifierInner {
                            loc: id_loc, name, ..
                        } = id.deref();
                        (id_loc.dupe(), name.clone())
                    }
                    Key::StringLiteral((id_loc, ast::StringLiteral { value: name, .. })) => {
                        (id_loc.dupe(), name.clone())
                    }
                    Key::NumberLiteral((
                        id_loc,
                        ast::NumberLiteral {
                            value: num_value, ..
                        },
                    )) if flow_common::js_number::is_float_safe_integer(*num_value) => {
                        let name = FlowSmolStr::new(flow_common::js_number::ecma_string_of_float(
                            *num_value,
                        ));
                        (id_loc.dupe(), name)
                    }
                    Key::NumberLiteral(_) | Key::BigIntLiteral(_) | Key::Computed(_) => {
                        // unsupported key
                        return;
                    }
                    Key::PrivateName(_) => {
                        panic!("unexpected private field in object literal")
                    }
                };
                let id_loc = tbls.push_loc(id_loc);
                let getter = getter_def(
                    opts,
                    scope,
                    scopes,
                    tbls,
                    &mut tparam_stack::TParamStack::new(),
                    id_loc,
                    fn_expr,
                );
                acc.add_accessor(name, getter);
            }
            NormalProperty::Set {
                key,
                value: (_, fn_expr),
                ..
            } => {
                let (id_loc, name) = match key {
                    Key::Identifier(id) => {
                        let IdentifierInner {
                            loc: id_loc, name, ..
                        } = &**id;
                        (id_loc.dupe(), name.clone())
                    }
                    Key::StringLiteral((id_loc, ast::StringLiteral { value: name, .. })) => {
                        (id_loc.dupe(), name.clone())
                    }
                    Key::NumberLiteral((
                        id_loc,
                        ast::NumberLiteral {
                            value: num_value, ..
                        },
                    )) if flow_common::js_number::is_float_safe_integer(*num_value) => {
                        let name = FlowSmolStr::new(flow_common::js_number::ecma_string_of_float(
                            *num_value,
                        ));
                        (id_loc.dupe(), name)
                    }
                    Key::NumberLiteral(_) | Key::BigIntLiteral(_) | Key::Computed(_) => {
                        // unsupported key
                        return;
                    }
                    Key::PrivateName(_) => {
                        panic!("unexpected private field in object literal")
                    }
                };
                let id_loc = tbls.push_loc(id_loc);
                let setter = setter_def(
                    opts,
                    scope,
                    scopes,
                    tbls,
                    &mut tparam_stack::TParamStack::new(),
                    id_loc,
                    fn_expr,
                );
                acc.add_accessor(name, setter);
            }
        }
    }

    fn spread<'arena: 'ast, 'ast>(
        opts: &TypeSigOptions,
        scope: ScopeId,
        scopes: &mut scope::Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        frozen: bool,
        acc: &mut ObjectLiteralAcc<'arena, 'ast>,
        p: &'ast ast::expression::object::SpreadProperty<Loc, Loc>,
    ) {
        let frozen_inner = if frozen {
            FrozenKind::FrozenDirect
        } else {
            FrozenKind::NotFrozen
        };
        let t = expression(opts, scope, scopes, tbls, frozen_inner, &p.argument);
        acc.add_spread(t);
    }

    let mut acc = ObjectLiteralAcc::empty();
    for p in obj.properties.iter() {
        match p {
            Property::SpreadProperty(spread_prop) => {
                spread(opts, scope, scopes, tbls, frozen, &mut acc, spread_prop);
            }
            Property::NormalProperty(
                NormalProperty::Init {
                    loc: prop_loc,
                    key: Key::Computed(_),
                    ..
                }
                | NormalProperty::Method {
                    loc: prop_loc,
                    key: Key::Computed(_),
                    ..
                },
            ) => {
                let prop_loc = tbls.push_loc(prop_loc.dupe());
                return Parsed::Err(
                    loc.dupe(),
                    Errno::SigError(Box::new(
                        signature_error::SignatureError::UnexpectedObjectKey(loc, prop_loc),
                    )),
                );
            }
            Property::NormalProperty(normal_prop) => {
                let prop_loc = normal_prop.loc();
                prop(
                    frozen,
                    opts,
                    scope,
                    scopes,
                    tbls,
                    &mut acc,
                    prop_loc,
                    normal_prop,
                );
            }
        }
    }
    acc.object_lit(loc, frozen, tbls.strictness_kind)
}

fn array_literal<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    loc: LocNode<'arena>,
    arr: &'ast ast::expression::Array<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    let mut acc = Vec::new();

    for elem in arr.elements.iter() {
        match elem {
            ast::expression::ArrayElement::Hole(_) => {
                return Parsed::Err(
                    loc.dupe(),
                    Errno::SigError(Box::new(
                        signature_error::SignatureError::UnexpectedArrayHole(loc),
                    )),
                );
            }
            ast::expression::ArrayElement::Expression(expr) => {
                let t = expression(opts, scope, scopes, tbls, FrozenKind::NotFrozen, expr);
                acc.push(t);
            }
            ast::expression::ArrayElement::Spread(spread) => {
                let spread_loc = tbls.push_loc(spread.loc.dupe());
                return Parsed::Err(
                    loc.dupe(),
                    Errno::SigError(Box::new(
                        signature_error::SignatureError::UnexpectedArraySpread(loc, spread_loc),
                    )),
                );
            }
        }
    }

    if acc.is_empty() {
        Parsed::Err(
            loc.dupe(),
            Errno::SigError(Box::new(signature_error::SignatureError::EmptyArray(loc))),
        )
    } else {
        let first = acc.remove(0);
        Parsed::Value(Box::new(ParsedValue::ArrayLit(Box::new(ValueArrayLit {
            loc,
            elem_t: first,
            elements: acc,
            strictness_kind: tbls.strictness_kind,
        }))))
    }
}

#[derive(Clone, Copy)]
enum GenericIdRootLookup {
    Value,
    Type,
}

fn member_expr_of_generic_id<'arena, 'ast>(
    scope: ScopeId,
    tbls: &mut Tables<'arena, 'ast>,
    lookup: GenericIdRootLookup,
    id: &ast::types::generic::Identifier<Loc, Loc>,
) -> Parsed<'arena, 'ast> {
    let mut chain = Vec::new();
    let mut id = id;
    loop {
        match id {
            ast::types::generic::Identifier::Qualified(inner) => {
                let loc = tbls.push_loc(inner.loc.dupe());
                let name = id_name(&inner.id).clone();
                chain.push((loc, name));
                id = &inner.qualification;
            }
            ast::types::generic::Identifier::Unqualified(id) => {
                let ref_loc = tbls.push_loc(id.loc.dupe());
                let name = id.name.dupe();
                let type_only = matches!(lookup, GenericIdRootLookup::Type);
                let mut t = val_ref(type_only, scope, ref_loc, name);
                for (loc, name) in chain.into_iter().rev() {
                    t = Parsed::Eval(
                        loc.dupe(),
                        Box::new(t),
                        Box::new(Op::GetProp(Box::new(name.clone()))),
                    );
                }
                return t;
            }
            ast::types::generic::Identifier::ImportTypeAnnot(import_type) => {
                let import_loc = tbls.push_loc(import_type.loc.dupe());
                let value = &import_type.source.1.value;
                let mref = Userland::from_smol_str(value.dupe());
                let mref = tbls.push_module_ref(mref);
                let base = Parsed::ImportTypeAnnot {
                    loc: import_loc,
                    mref,
                };
                let t = chain.into_iter().rev().fold(base, |t, (loc, name)| {
                    Parsed::Eval(
                        loc.dupe(),
                        Box::new(t),
                        Box::new(Op::GetProp(Box::new(name))),
                    )
                });
                return t;
            }
        }
    }
}

fn declare_class_def<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    c: &ast::statement::DeclareClass<Loc, Loc>,
) -> DeclareClassSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
    use declare_class_acc::DeclareClassAcc;
    let ast::statement::DeclareClass {
        id,
        tparams: tps,
        body: (_body_loc, body),
        extends,
        mixins,
        implements,
        abstract_,
        ..
    } = c;
    let abstract_ = *abstract_ && opts.tslib_syntax;
    let mut xs = tparam_stack::TParamStack::new();
    let tparams = tparams(opts, scope, scopes, tbls, &mut xs, tps.as_ref());
    xs.insert(FlowSmolStr::new_inline("this"));

    let extends = match extends {
        None => {
            if opts.is_lib_file && id.name.as_str() == "Object" {
                ClassExtends::ObjectPrototypeExtendsNull
            } else {
                ClassExtends::ClassImplicitExtends
            }
        }
        Some((loc, ast::statement::DeclareClassExtends::ExtendsIdent(generic))) => {
            let loc = tbls.push_loc(loc.dupe());
            let t = member_expr_of_generic_id(scope, tbls, GenericIdRootLookup::Value, &generic.id);
            match &generic.targs {
                None => ClassExtends::ClassExplicitExtends(Box::new((loc, t))),
                Some(type_args) => {
                    let targs = type_args
                        .arguments
                        .iter()
                        .map(|arg| annot(opts, scope, scopes, tbls, &mut xs, arg))
                        .collect();
                    ClassExtends::ClassExplicitExtendsApp(Box::new((loc, t, targs)))
                }
            }
        }
        Some((_, ast::statement::DeclareClassExtends::ExtendsCall { .. })) => {
            ClassExtends::ClassImplicitExtends
        }
    };

    let mut parsed_mixins = Vec::new();
    for (loc, mixin) in mixins.iter() {
        let loc = tbls.push_loc(loc.dupe());
        let t = member_expr_of_generic_id(scope, tbls, GenericIdRootLookup::Type, &mixin.id);
        let mixin_val = match &mixin.targs {
            None => ClassMixins::ClassMixin(Box::new((loc, t))),
            Some(type_args) => {
                let targs = type_args
                    .arguments
                    .iter()
                    .map(|arg| annot(opts, scope, scopes, tbls, &mut xs, arg))
                    .collect();
                ClassMixins::ClassMixinApp(Box::new((loc, t, targs)))
            }
        };
        parsed_mixins.push(mixin_val);
    }
    let parsed_implements = class_implements(opts, scope, scopes, tbls, &mut xs, implements);
    let class_tparam_names: Vec<FlowSmolStr> = tps
        .iter()
        .flat_map(|tps| tps.params.iter())
        .map(|tp| tp.name.name.dupe())
        .collect();

    let mut acc = DeclareClassAcc::empty();
    declare_class_props(
        opts,
        scope,
        scopes,
        tbls,
        &mut xs,
        &class_tparam_names,
        &body.properties,
        &mut acc,
    );
    acc.declare_class_def(
        abstract_,
        tparams,
        extends,
        parsed_mixins,
        parsed_implements,
        tbls.strictness_kind,
    )
}

fn type_alias_decl<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    export_comments: Option<&ast::Syntax<Loc, ()>>,
    decl: &'ast ast::statement::TypeAlias<Loc, Loc>,
    k: &dyn Fn(&mut scope::Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
) {
    let ast::statement::TypeAlias {
        id,
        tparams: tps,
        right: t,
        comments,
    } = decl;
    let name = &id.name;

    let custom_error_loc_opt: Option<LocNode<'arena>> = if opts.enable_custom_error {
        let extract_loc = |tbls: &mut Tables<'arena, 'ast>,
                           comments: Option<&ast::Syntax<Loc, ()>>|
         -> Option<LocNode<'arena>> {
            match jsdoc::of_comments(comments) {
                Some((custom_error_loc, ref jsdoc_val))
                    if jsdoc_val
                        .unrecognized_tags()
                        .0
                        .iter()
                        .any(|(tag_name, _)| tag_name == "flowCustomError") =>
                {
                    Some(tbls.push_loc(custom_error_loc))
                }
                _ => None,
            }
        };
        match extract_loc(tbls, comments.as_ref()) {
            Some(l) => Some(l),
            None => extract_loc(tbls, export_comments),
        }
    } else {
        None
    };

    let id_loc_node = tbls.push_loc(id.loc.dupe());
    let id_loc_node_for_def = id_loc_node.dupe();
    let tps_clone = tps.clone();
    let t_clone = t.clone();
    let name_string = name.to_string();

    let def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
        tbls.splice(id_loc_node_for_def.dupe(), |tbls| {
            let mut xs = tparam_stack::TParamStack::new();
            let tparams = tparams(opts, scope, scopes, tbls, &mut xs, tps_clone.as_ref());
            let body = annot(opts, scope, scopes, tbls, &mut xs, &t_clone);
            Def::TypeAlias(Box::new(DefTypeAlias {
                id_loc: id_loc_node_for_def,
                custom_error_loc_opt,
                name: name_string.into(),
                tparams,
                body,
                strictness_kind: tbls.strictness_kind,
            }))
        })
    }));

    scope::bind_type(opts, scope, scopes, tbls, id_loc_node, name.clone(), def, k);
}

fn opaque_type_decl<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    decl: &'ast ast::statement::OpaqueType<Loc, Loc>,
    k: &dyn Fn(&mut scope::Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
) {
    let ast::statement::OpaqueType {
        id,
        tparams: tps,
        lower_bound,
        upper_bound,
        legacy_upper_bound,
        impl_type,
        comments: _,
    } = decl;
    let name = &id.name;

    let id_loc_node = tbls.push_loc(id.loc.dupe());
    let id_loc_node_for_def = id_loc_node.dupe();
    let name_string = name.clone();

    let def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
        tbls.splice(id_loc_node_for_def.dupe(), |tbls| {
            let mut xs = tparam_stack::TParamStack::new();
            let tparams = tparams(opts, scope, scopes, tbls, &mut xs, tps.as_ref());
            let lower_bound = lower_bound
                .as_ref()
                .map(|t| annot(opts, scope, scopes, tbls, &mut xs, t));
            let upper_bound_final = upper_bound.as_ref().or(legacy_upper_bound.as_ref());
            let upper_bound =
                upper_bound_final.map(|t| annot(opts, scope, scopes, tbls, &mut xs, t));
            let body = impl_type
                .as_ref()
                .map(|t| annot(opts, scope, scopes, tbls, &mut xs, t));
            Def::OpaqueType(Box::new(DefOpaqueType {
                id_loc: id_loc_node_for_def,
                name: name_string,
                tparams,
                lower_bound,
                upper_bound,
                body,
                strictness_kind: tbls.strictness_kind,
            }))
        })
    }));

    scope::bind_type(opts, scope, scopes, tbls, id_loc_node, name.clone(), def, k);
}

fn const_var_init_decl<'arena: 'ast, 'ast>(
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    id_loc: LocNode<'arena>,
    name: FlowSmolStr,
    expr: &'ast ast::expression::Expression<Loc, Loc>,
    k: &dyn Fn(&mut scope::Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
) {
    use ast::expression::ExpressionInner as E;

    match expr.deref() {
        // const x = id
        E::Identifier {
            loc: ref_loc,
            inner: id,
        } => {
            let ref_name = &id.name;
            let ref_loc_node = tbls.push_loc(ref_loc.dupe());
            scope::bind_const_ref(
                scope,
                scopes,
                tbls,
                id_loc,
                name,
                ref_loc_node,
                ref_name.clone(),
                scope,
                k,
            );
        }
        // const x = function
        E::Function { inner: f, .. } => {
            let sig_loc = &f.sig_loc;
            let async_ = f.async_;
            let generator = f.generator;
            let sig_loc_node = tbls.push_loc(sig_loc.dupe());

            match &f.id {
                Some(fn_id) => {
                    let fn_name = &fn_id.name;
                    let fn_id_loc = &fn_id.loc;
                    let fn_id_loc_node = tbls.push_loc(fn_id_loc.dupe());
                    let splice_loc = fn_id_loc_node.dupe();
                    let fn_scope = scope::push_lex(scopes, scope);

                    let def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
                        tbls.splice(splice_loc.dupe(), |tbls| {
                            let mut xs = tparam_stack::TParamStack::new();
                            function_def(opts, fn_scope, scopes, tbls, &mut xs, splice_loc, f)
                        })
                    }));

                    scope::bind_function(
                        fn_scope,
                        scopes,
                        tbls,
                        fn_id_loc_node.dupe(),
                        sig_loc_node,
                        fn_name.clone(),
                        async_,
                        generator,
                        ast::function::Effect::Arbitrary,
                        def,
                        &|_, _, _| {},
                    );

                    scope::bind_const_ref(
                        scope,
                        scopes,
                        tbls,
                        id_loc,
                        name,
                        fn_id_loc_node,
                        fn_name.clone(),
                        fn_scope,
                        k,
                    );
                }
                None => {
                    let splice_loc = sig_loc_node.dupe();
                    let def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
                        tbls.splice(splice_loc.dupe(), |tbls| {
                            let mut xs = tparam_stack::TParamStack::new();
                            function_def(opts, scope, scopes, tbls, &mut xs, splice_loc, f)
                        })
                    }));

                    scope::bind_const_fun(
                        scope,
                        scopes,
                        tbls,
                        id_loc,
                        name,
                        sig_loc_node,
                        async_,
                        generator,
                        def,
                        k,
                    );
                }
            }
        }
        // const x = arrow function
        E::ArrowFunction { loc, inner: f } => {
            let async_ = f.async_;
            let generator = f.generator;
            let loc_node = tbls.push_loc(loc.dupe());
            let splice_loc = loc_node.dupe();

            let def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
                tbls.splice(splice_loc.dupe(), |tbls| {
                    let mut xs = tparam_stack::TParamStack::new();
                    function_def(opts, scope, scopes, tbls, &mut xs, splice_loc, f)
                })
            }));

            scope::bind_const_fun(
                scope, scopes, tbls, id_loc, name, loc_node, async_, generator, def, k,
            );
        }
        // const x = a, b
        E::Sequence { inner: s, .. } => {
            let expr = s.expressions.last().expect("unexpected empty sequence");
            const_var_init_decl(scope, scopes, tbls, id_loc, name.clone(), expr, &k);
        }
        // const x = ... fallback
        _ => {
            let splice_loc = id_loc.dupe();
            let def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
                tbls.splice(splice_loc, |tbls| {
                    expression(opts, scope, scopes, tbls, FrozenKind::NotFrozen, expr)
                })
            }));

            scope::bind_const(scope, scopes, tbls, id_loc, name, def, k);
        }
    }
}

fn variable_decl<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    kind: ast::VariableKind,
    decl: &'ast ast::statement::variable::Declarator<Loc, Loc>,
    k: &dyn Fn(&mut scope::Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
) {
    use ast::pattern::Pattern as P;
    let init = &decl.init;
    match &decl.id {
        P::Identifier { inner: ident, .. } => {
            let name_ident = &ident.name;
            let id_loc = &name_ident.loc;
            let name = &name_ident.name;
            let annot = &ident.annot;

            let id_loc_node = tbls.push_loc(id_loc.dupe());

            match (kind, annot, init) {
                // A symbol constructor call is a fresh value. Keep it distinct
                // from an explicit `unique symbol` annotation so natural
                // inference can widen later uses of the binding.
                (
                    ast::VariableKind::Const,
                    ast::types::AnnotationOrHint::Missing(_),
                    Some(expr),
                ) if symbol_call::symbol_constructor_call(expr).is_some() => {
                    let splice_loc = id_loc_node.dupe();
                    let def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
                        tbls.splice(splice_loc, |tbls| {
                            // Only the global constructor mints. A binding of
                            // that name in the file shadows it, leaving an
                            // ordinary call, which a signature has no way to
                            // describe. Which of the two it is cannot be
                            // settled until every binding of the file is in
                            // scope, so the call's location is recorded here
                            // rather than on the way past it.
                            if scope::lookup_value(
                                scopes,
                                scope,
                                &FlowSmolStr::new_inline("Symbol"),
                            )
                            .is_some()
                            {
                                return expression(
                                    opts,
                                    scope,
                                    scopes,
                                    tbls,
                                    FrozenKind::NotFrozen,
                                    expr,
                                );
                            }
                            let call_loc = tbls.push_loc(expr.loc().dupe());
                            Parsed::Value(Box::new(ParsedValue::UniqueSymbol(Box::new(call_loc))))
                        })
                    }));

                    scope::bind_const(scope, scopes, tbls, id_loc_node, name.clone(), def, k);
                }
                // const x = ... special cases
                (
                    ast::VariableKind::Const,
                    ast::types::AnnotationOrHint::Missing(_),
                    Some(expr),
                ) => {
                    const_var_init_decl(scope, scopes, tbls, id_loc_node, name.clone(), expr, k);
                }
                _ => {
                    let id_loc_node_for_def = id_loc_node.dupe();
                    let def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
                        tbls.splice(id_loc_node_for_def.dupe(), |tbls| {
                            annot_or_hint(
                                ExpectedAnnotationSort::VariableDefinition { name: name.clone() },
                                Some(&id_loc_node_for_def),
                                opts,
                                scope,
                                scopes,
                                tbls,
                                &mut tparam_stack::TParamStack::new(),
                                annot,
                            )
                        })
                    }));

                    scope::bind_var(scope, scopes, tbls, kind, id_loc_node, name.clone(), def, k);
                }
            }
        }
        P::Object { inner, .. } => {
            let annot = &inner.annot;
            let splice_loc_cell: std::rc::Rc<std::cell::OnceCell<LocNode<'arena>>> =
                std::rc::Rc::new(std::cell::OnceCell::new());
            let splice_loc_cell_dupe = splice_loc_cell.dupe();
            let pattern_def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
                let splice_loc = splice_loc_cell_dupe
                    .get()
                    .expect("splice_loc should be set")
                    .dupe();
                let def = tbls.splice(splice_loc, |tbls| match (kind, annot, init) {
                    (
                        ast::VariableKind::Const,
                        ast::types::AnnotationOrHint::Missing(_),
                        Some(expr),
                    ) => expression(opts, scope, scopes, tbls, FrozenKind::NotFrozen, expr),
                    // Note: OCaml uses ArrayPattern for BOTH Object and Array patterns here
                    _ => annot_or_hint(
                        ExpectedAnnotationSort::ArrayPattern,
                        None,
                        opts,
                        scope,
                        scopes,
                        tbls,
                        &mut tparam_stack::TParamStack::new(),
                        annot,
                    ),
                });
                tbls.push_pattern_def(def)
            }));
            let def = tbls.push_pattern(Pattern::PDef(pattern_def));
            pattern(
                opts,
                scope,
                scopes,
                tbls,
                &mut |tbls, scopes, id_loc, name, p| {
                    let def = tbls.lazy(Box::new(move |_, _, _| Parsed::Pattern(p)));
                    scope::bind_var(scope, scopes, tbls, kind, id_loc, name, def, k);
                },
                def,
                &decl.id,
            );
            splice_loc_cell
                .set(LocNode(tbls.locs.tail_exn()))
                .ok()
                .unwrap();
        }
        P::Array { inner, .. } => {
            let annot = &inner.annot;
            let splice_loc_cell: std::rc::Rc<std::cell::OnceCell<LocNode<'arena>>> =
                std::rc::Rc::new(std::cell::OnceCell::new());
            let splice_loc_cell_dupe = splice_loc_cell.dupe();
            let pattern_def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
                let splice_loc = splice_loc_cell_dupe
                    .get()
                    .expect("splice_loc should be set")
                    .dupe();
                let def = tbls.splice(splice_loc, |tbls| match (kind, annot, init) {
                    (
                        ast::VariableKind::Const,
                        ast::types::AnnotationOrHint::Missing(_),
                        Some(expr),
                    ) => expression(opts, scope, scopes, tbls, FrozenKind::NotFrozen, expr),
                    _ => annot_or_hint(
                        ExpectedAnnotationSort::ArrayPattern,
                        None,
                        opts,
                        scope,
                        scopes,
                        tbls,
                        &mut tparam_stack::TParamStack::new(),
                        annot,
                    ),
                });
                tbls.push_pattern_def(def)
            }));
            let def = tbls.push_pattern(Pattern::PDef(pattern_def));
            pattern(
                opts,
                scope,
                scopes,
                tbls,
                &mut |tbls, scopes, id_loc, name, p| {
                    let def = tbls.lazy(Box::new(move |_, _, _| Parsed::Pattern(p)));
                    scope::bind_var(scope, scopes, tbls, kind, id_loc, name, def, k);
                },
                def,
                &decl.id,
            );
            splice_loc_cell
                .set(LocNode(tbls.locs.tail_exn()))
                .ok()
                .unwrap();
        }
        P::Expression { .. } => panic!("unexpected expression pattern"),
    }
}

fn variable_decls<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    decl: &'ast ast::statement::VariableDeclaration<Loc, Loc>,
    k: &dyn Fn(&mut scope::Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
) {
    for declarator in decl.declarations.iter() {
        variable_decl(opts, scope, scopes, tbls, decl.kind, declarator, &k);
    }
}

fn record_def<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    decl: &'ast ast::statement::RecordDeclaration<Loc, Loc>,
) -> ClassSig<LocNode<'arena>, Parsed<'arena, 'ast>> {
    use ast::statement::record_declaration;
    use class_acc::ClassAcc;

    let ast::statement::RecordDeclaration {
        id: _,
        body,
        tparams: tps,
        implements,
        ..
    } = decl;

    let mut xs = tparam_stack::TParamStack::new();
    let tparams = tparams(opts, scope, scopes, tbls, &mut xs, tps.as_ref());
    xs.insert(FlowSmolStr::new_inline("this"));
    let extends = ClassExtends::ClassImplicitExtends;
    let implements = class_implements(opts, scope, scopes, tbls, &mut xs, implements);
    let mut acc = ClassAcc::empty();
    // Process record properties
    for element in body.body.iter() {
        match element {
            record_declaration::BodyElement::Property(record_declaration::Property {
                key,
                annot: annot_field,
                default_value: _,
                comments: _,
                invalid_syntax: _,
                loc: _,
            }) => {
                let (key_loc, name) = match record_utils::loc_and_string_of_property_key(key) {
                    Some(v) => v,
                    None => continue, // Skip computed/private keys (shouldn't happen)
                };
                let id_loc = tbls.push_loc(key_loc);
                let parsed_t = annot(opts, scope, scopes, tbls, &mut xs, &annot_field.annotation);
                // Records have covariant fields
                acc.add_field(false, name, id_loc, Polarity::Positive, parsed_t);
            }
            record_declaration::BodyElement::Method(method) => {
                if let ast::class::MethodKind::Method = method.kind {
                    let key = &method.key;
                    let name = match key {
                        ast::expression::object::Key::Identifier(id) => id.name.dupe(),
                        _ => continue,
                    };
                    let fn_val = &method.value.1;
                    let async_ = fn_val.async_;
                    let generator = fn_val.generator;
                    let static_ = method.static_;
                    let fn_loc_node = tbls.push_loc(method.loc.dupe());
                    let id_loc = match key {
                        ast::expression::object::Key::Identifier(id) => {
                            tbls.push_loc(id.loc.dupe())
                        }
                        _ => continue,
                    };
                    let def = function_def(
                        opts,
                        scope,
                        scopes,
                        tbls,
                        &mut xs,
                        fn_loc_node.dupe(),
                        fn_val,
                    );
                    acc.add_method(
                        static_,
                        name,
                        id_loc,
                        fn_loc_node,
                        async_,
                        generator,
                        static_ && flow_parser_utils::this_finder::function_uses_this(fn_val),
                        def,
                    );
                }
            }
            record_declaration::BodyElement::StaticProperty(
                record_declaration::StaticProperty {
                    key,
                    annot: annot_field,
                    value: _,
                    comments: _,
                    invalid_syntax: _,
                    loc: _,
                },
            ) => {
                let (key_loc, name) = match record_utils::loc_and_string_of_property_key(key) {
                    Some(v) => v,
                    None => continue, // Skip computed/private keys (shouldn't happen)
                };
                let id_loc = tbls.push_loc(key_loc);
                let parsed_t = annot(opts, scope, scopes, tbls, &mut xs, &annot_field.annotation);
                acc.add_field(true, name, id_loc, Polarity::Positive, parsed_t);
            }
        }
    }

    acc.class_def(false, tparams, extends, implements, tbls.strictness_kind)
}

fn record_decl<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    decl: &'ast ast::statement::RecordDeclaration<Loc, Loc>,
    k: &dyn Fn(&mut scope::Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
) {
    let ast::statement::RecordDeclaration { id, .. } = decl;
    let name = id.name.dupe();
    let id_loc_node = tbls.push_loc(id.loc.dupe());

    let defaulted_props = record_utils::defaulted_props_of_record(decl);

    let def = if opts.enable_records {
        let id_loc_node_for_def = id_loc_node.dupe();
        Some(tbls.lazy(Box::new(move |opts, scopes, tbls| {
            tbls.splice(id_loc_node_for_def.dupe(), |tbls| {
                record_def(opts, scope, scopes, tbls, decl)
            })
        })))
    } else {
        None
    };

    scope::bind_record(
        scope,
        scopes,
        tbls,
        id_loc_node,
        name,
        def,
        defaulted_props,
        k,
    );
}

fn class_decl<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    decl: &'ast ast::class::Class<Loc, Loc>,
    k: &dyn Fn(&mut scope::Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
) {
    let id = decl.id.as_ref().expect("class declaration must have id");
    let name = id.name.dupe();
    let id_loc_node = tbls.push_loc(id.loc.dupe());
    let id_loc_node_for_def = id_loc_node.dupe();
    let def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
        tbls.splice(id_loc_node_for_def.dupe(), |tbls| {
            class_def(opts, scope, scopes, tbls, decl)
        })
    }));
    scope::bind_class(opts, scope, scopes, tbls, id_loc_node, name, def, k);
}

fn function_decl<'arena: 'ast, 'ast>(
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    decl: &'ast ast::function::Function<Loc, Loc>,
    k: &dyn Fn(&mut scope::Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
) {
    let ast::function::Function {
        id,
        async_,
        generator,
        effect_,
        sig_loc,
        ..
    } = decl;
    let id_ident = id.as_ref().expect("function declaration must have id");
    let name = id_ident.name.dupe();
    let sig_loc_node = tbls.push_loc(sig_loc.dupe());
    let id_loc_node = tbls.push_loc(id_ident.loc.dupe());
    let id_loc_node_for_def = id_loc_node.dupe();

    let def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
        let mut xs = tparam_stack::TParamStack::new();
        tbls.splice(id_loc_node_for_def.dupe(), |tbls| {
            function_def(
                opts,
                scope,
                scopes,
                tbls,
                &mut xs,
                id_loc_node_for_def,
                decl,
            )
        })
    }));

    scope::bind_function(
        scope,
        scopes,
        tbls,
        id_loc_node,
        sig_loc_node,
        name,
        *async_,
        *generator,
        *effect_,
        def,
        k,
    );
}

fn component_decl<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    decl: &'ast ast::statement::ComponentDeclaration<Loc, Loc>,
    k: &dyn Fn(&mut scope::Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
) {
    let ast::statement::ComponentDeclaration { id, sig_loc, .. } = decl;
    let name = &id.name;
    let sig_loc_node = tbls.push_loc(sig_loc.dupe());
    let id_loc_node = tbls.push_loc(id.loc.dupe());
    let id_loc_node_for_def = id_loc_node.dupe();
    let enable_component_syntax = opts.enable_component_syntax;

    let def = if enable_component_syntax {
        Some(tbls.lazy(Box::new(move |opts, scopes, tbls| {
            tbls.splice(id_loc_node_for_def.dupe(), |tbls| {
                component_def(opts, scope, scopes, tbls, decl)
            })
        })))
    } else {
        None
    };

    scope::bind_component(
        scope,
        scopes,
        tbls,
        id_loc_node,
        sig_loc_node,
        name.clone(),
        def,
        k,
    );
}

fn declare_component_decl<'arena: 'ast, 'ast>(
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    c_loc: &Loc,
    decl: &'ast ast::statement::DeclareComponent<Loc, Loc>,
    k: &dyn Fn(&mut scope::Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
) {
    let name = &decl.id.name;
    let sig_loc_node = tbls.push_loc(c_loc.clone());
    let id_loc_node = tbls.push_loc(decl.id.loc.dupe());
    let id_loc_node_for_def = id_loc_node.dupe();

    let def = Some(tbls.lazy(Box::new(move |opts, scopes, tbls| {
        tbls.splice(id_loc_node_for_def.dupe(), |tbls| {
            declare_component_def(opts, scope, scopes, tbls, decl)
        })
    })));

    scope::bind_component(
        scope,
        scopes,
        tbls,
        id_loc_node,
        sig_loc_node,
        name.clone(),
        def,
        k,
    );
}

fn declare_variable_decl<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    decl: &'ast ast::statement::DeclareVariable<Loc, Loc>,
    k: &dyn Fn(&mut scope::Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
) {
    let ast::statement::DeclareVariable {
        declarations,
        kind,
        comments: _,
    } = decl;
    let kind_val = *kind;

    for declarator in declarations.iter() {
        let ast::statement::variable::Declarator { id: _, init, .. } = declarator;
        if let ast::pattern::Pattern::Identifier { inner: pat_id, .. } = &declarator.id {
            let id_loc = &pat_id.name.loc;
            let name = &pat_id.name.name;
            let id_annot = &pat_id.annot;
            let id_loc_node = tbls.push_loc(id_loc.dupe());

            let mut xs = tparam_stack::TParamStack::new();
            match id_annot {
                ast::types::AnnotationOrHint::Available(type_annot) => {
                    let t = annot(opts, scope, scopes, tbls, &mut xs, &type_annot.annotation);
                    let def = tbls.lazy(Box::new(move |_, _, _| t));
                    scope::bind_var(
                        scope,
                        scopes,
                        tbls,
                        kind_val,
                        id_loc_node.dupe(),
                        name.dupe(),
                        def,
                        k,
                    );
                }
                ast::types::AnnotationOrHint::Missing(_) => {
                    match init
                        .as_ref()
                        .and_then(|e| literal_expr_to_annot(id_loc_node.dupe(), e))
                    {
                        Some(t) => {
                            let id_loc_for_bind = id_loc_node.dupe();
                            let def = tbls.lazy(Box::new(move |_, _, _| t));
                            scope::bind_var(
                                scope,
                                scopes,
                                tbls,
                                kind_val,
                                id_loc_for_bind,
                                name.dupe(),
                                def,
                                k,
                            );
                        }
                        None => {
                            let id_loc_node_for_bind = id_loc_node.dupe();
                            let def = tbls.lazy(Box::new(move |_, _, _| {
                                Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(id_loc_node))))
                            }));
                            scope::bind_var(
                                scope,
                                scopes,
                                tbls,
                                kind_val,
                                id_loc_node_for_bind,
                                name.dupe(),
                                def,
                                k,
                            );
                        }
                    }
                }
            }
        }
    }
}

fn declare_function_decl<'arena: 'ast, 'ast>(
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    decl: &'ast ast::statement::DeclareFunction<Loc, Loc>,
    k: &dyn Fn(&mut scope::Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
) {
    let ast::statement::DeclareFunction {
        id,
        annot: fn_annot,
        predicate: _,
        comments: _,
        implicit_declare: _,
    } = decl;
    match id {
        Some(id) => {
            let name = &id.name;
            let id_loc_node = tbls.push_loc(id.loc.dupe());
            let fn_loc_node = tbls.push_loc(fn_annot.loc.dupe());
            let fn_loc_node_for_def = fn_loc_node.dupe();
            let t = &fn_annot.annotation;

            let def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
                tbls.splice(fn_loc_node_for_def, |tbls| {
                    use ast::types as T;
                    match t.deref() {
                        T::TypeInner::Function {
                            loc: _,
                            inner: fn_type,
                        } => {
                            let T::Function {
                                tparams: tps,
                                params: params_inner,
                                return_: r,
                                effect,
                                comments: _,
                            } = fn_type.as_ref();
                            let mut xs = tparam_stack::TParamStack::new();
                            let tparams = tparams(opts, scope, scopes, tbls, &mut xs, tps.as_ref());
                            let this_param = function_type_this_param(
                                opts,
                                scope,
                                scopes,
                                tbls,
                                &mut xs,
                                &params_inner.this,
                            );
                            let params = function_type_params(
                                opts,
                                scope,
                                scopes,
                                tbls,
                                &mut xs,
                                &params_inner.params,
                            );
                            let rest_param = function_type_rest_param(
                                opts,
                                scope,
                                scopes,
                                tbls,
                                &mut xs,
                                &params_inner.rest,
                            );
                            let (return_, type_guard) =
                                return_annot(false, opts, scope, scopes, tbls, &mut xs, r);
                            let effect_val = convert_effect(opts, effect, None, Some(name));
                            FunSig {
                                tparams,
                                params,
                                rest_param,
                                this_param,
                                return_,
                                type_guard,
                                effect_: effect_val,
                                strictness_kind: tbls.strictness_kind,
                            }
                        }
                        _ => panic!("unexpected declare function annot"),
                    }
                })
            }));

            scope::bind_declare_function(
                scope,
                scopes,
                tbls,
                id_loc_node,
                fn_loc_node,
                name.clone(),
                def,
                k,
            );
        }
        None => {}
    }
}

fn declare_class_decl<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    decl: &'ast ast::statement::DeclareClass<Loc, Loc>,
    k: &dyn Fn(&mut scope::Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
) {
    let name = &decl.id.name;
    let id_loc_node = tbls.push_loc(decl.id.loc.dupe());
    let id_loc_node_for_def = id_loc_node.dupe();
    let decl_clone = decl.clone();

    let def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
        tbls.splice(id_loc_node_for_def.dupe(), |tbls| {
            declare_class_def(opts, scope, scopes, tbls, &decl_clone)
        })
    }));

    scope::bind_declare_class(opts, scope, scopes, tbls, id_loc_node, name.clone(), def, k);
}

fn namespace_decl<'arena: 'ast, 'ast, F>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    is_type_only: bool,
    decl: &'ast ast::statement::DeclareNamespace<Loc, Loc>,
    visit_statement: F,
) where
    F: Fn(
        &TypeSigOptions,
        ScopeId,
        &mut scope::Scopes<'arena, 'ast>,
        &mut Tables<'arena, 'ast>,
        &'ast ast::statement::Statement<Loc, Loc>,
    ),
{
    match &decl.id {
        ast::statement::declare_namespace::Id::Global(_) => {}
        ast::statement::declare_namespace::Id::Local(id) => {
            let id_loc = tbls.push_loc(id.loc.dupe());
            let name = id.name.dupe();
            let scope = scope::push_declare_namespace(scopes, scope, name.dupe());
            for s in decl.body.1.body.iter().filter(|stmt| {
                ast_utils::acceptable_statement_in_declaration_context(
                    ast_utils::DeclarationContext::DeclareNamespace,
                    stmt,
                )
                .is_ok()
            }) {
                visit_statement(opts, scope, scopes, tbls, s);
            }
            scope::resolve_declare_namespace_exports(opts, scopes, scope);
            scope::finalize_declare_namespace_exn(scope, scopes, tbls, is_type_only, id_loc, name);
        }
    }
}

pub(crate) fn declare_global<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    augmentation_scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    decl: &'ast ast::statement::DeclareNamespace<Loc, Loc>,
) {
    for stmt in decl.body.1.body.iter() {
        if ast_utils::acceptable_statement_in_declaration_context(
            ast_utils::DeclarationContext::DeclareGlobal,
            stmt,
        )
        .is_ok()
        {
            statement(opts, augmentation_scope, scopes, tbls, stmt);
        }
    }
}

fn import_decl<'arena, 'ast>(
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    decl: &'ast ast::statement::ImportDeclaration<Loc, Loc>,
) {
    let ast::statement::ImportDeclaration {
        source,
        default,
        specifiers,
        import_kind: kind,
        // Deferred evaluation does not change the shape of the namespace, so
        // a deferred import is signed exactly like the eager one.
        phase: _,
        attributes: _,
        comments: _,
    } = decl;
    let mref = tbls.push_module_ref(Userland::from_smol_str(source.1.value.dupe()));
    if let Some(default) = default {
        let id_loc = tbls.push_loc(default.identifier.loc.dupe());
        let local = default.identifier.name.dupe();
        scope::bind_import(
            scope,
            scopes,
            tbls,
            *kind,
            id_loc,
            local,
            FlowSmolStr::new_inline("default"),
            mref.dupe(),
        );
    }
    match specifiers {
        None => {}
        Some(ast::statement::import_declaration::Specifier::ImportNamespaceSpecifier((_, id))) => {
            let id_loc = tbls.push_loc(id.loc.dupe());
            let name = id.name.dupe();
            scope::bind_import_ns(scope, scopes, tbls, *kind, id_loc, name, mref);
        }
        Some(ast::statement::import_declaration::Specifier::ImportNamedSpecifiers(specifiers)) => {
            for specifier in specifiers {
                let kind = specifier.kind.unwrap_or(*kind);
                let (local_id_loc, local) = match &specifier.local {
                    Some(local) => (local.loc.dupe(), local.name.dupe()),
                    None => (specifier.remote.loc.dupe(), specifier.remote.name.dupe()),
                };
                let remote = specifier.remote.name.dupe();
                let local_id_loc = tbls.push_loc(local_id_loc);
                scope::bind_import(
                    scope,
                    scopes,
                    tbls,
                    kind,
                    local_id_loc,
                    local,
                    remote,
                    mref.dupe(),
                );
            }
        }
    }
}

fn interface_decl<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    decl: &'ast ast::statement::Interface<Loc, Loc>,
    k: &dyn Fn(&mut scope::Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
) {
    let name = &decl.id.name;
    let tps = &decl.tparams;
    let properties = &decl.body.1.properties;
    let extends = &decl.extends;

    let id_loc_node = tbls.push_loc(decl.id.loc.dupe());
    let id_loc_node_for_def = id_loc_node.dupe();

    let def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
        tbls.splice(id_loc_node_for_def.dupe(), |tbls| {
            let mut xs = tparam_stack::TParamStack::new();
            let tparams = tparams(opts, scope, scopes, tbls, &mut xs, tps.as_ref());
            let def = interface_def(
                true, opts, scope, scopes, tbls, &mut xs, extends, properties,
            );
            Def::Interface(Box::new(DefInterface {
                id_loc: id_loc_node_for_def,
                name: name.clone(),
                tparams,
                def,
            }))
        })
    }));

    scope::bind_interface(opts, scope, scopes, tbls, id_loc_node, name.clone(), def, k);
}

fn enum_decl<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    decl: &'ast ast::statement::EnumDeclaration<Loc, Loc>,
    declared: bool,
    k: &dyn Fn(&mut scope::Scopes<'arena, 'ast>, &FlowSmolStr, LocalDefNode<'arena, 'ast>),
) {
    fn enum_rep_to_type_sig_rep(rep: &enum_validate::EnumRep) -> EnumRep {
        match rep {
            enum_validate::EnumRep::BoolRep(lit) => EnumRep::BoolRep(*lit),
            enum_validate::EnumRep::NumberRep { truthy } => EnumRep::NumberRep { truthy: *truthy },
            enum_validate::EnumRep::StringRep { truthy } => EnumRep::StringRep { truthy: *truthy },
            enum_validate::EnumRep::SymbolRep => EnumRep::SymbolRep,
            enum_validate::EnumRep::BigIntRep { truthy } => EnumRep::BigIntRep { truthy: *truthy },
        }
    }

    fn enum_def<'arena, 'ast>(
        is_ts_file: bool,
        ambient: bool,
        tbls: &mut Tables<'arena, 'ast>,
        body_loc: &Loc,
        body: &ast::statement::enum_declaration::Body<Loc>,
    ) -> (
        Option<EnumRep>,
        BTreeMap<FlowSmolStr, LocNode<'arena>>,
        Option<BTreeMap<FlowSmolStr, TsEnumMemberValue>>,
        bool,
    ) {
        let result = enum_validate::classify_enum_body(body, body_loc);
        let members = result
            .members
            .iter()
            .fold(BTreeMap::new(), |mut acc, (name, loc)| {
                let loc = tbls.push_loc(loc.clone());
                acc.insert(FlowSmolStr::new(name), loc);
                acc
            });
        // The parsed signature retains each member's literal value (auto-numbered
        // per TS, see ts_enum_sig.rs's ts_enum_member_values) so cross-module consumers
        // can model the enum as an object of literal members + a union type. The
        // locs are unused here; the merge step recovers them from [members].
        // The signature ignores per-member errors; they are reported by the
        // local-checking path (ts_enum.rs's mk_ts_enum_namespace). An errored
        // member is still recorded, degraded to a computed value.
        let member_values = if is_ts_file {
            Some(
                crate::ts_enum_sig::ts_enum_member_values(ambient, &body.members)
                    .into_iter()
                    .fold(
                        BTreeMap::new(),
                        |mut acc, (name, _member_loc, _init_loc, value, _error)| {
                            acc.insert(name, value);
                            acc
                        },
                    ),
            )
        } else {
            None
        };
        let has_unknown_members = result.has_unknown_members.is_some();
        match &result.rep {
            Some(rep) => (
                Some(enum_rep_to_type_sig_rep(rep)),
                members,
                member_values,
                has_unknown_members,
            ),
            None => (None, members, member_values, has_unknown_members),
        }
    }

    let name = decl.id.name.dupe();
    let body = &decl.body;
    let body_loc = body.loc.dupe();
    let id_loc_node = tbls.push_loc(decl.id.loc.dupe());
    let splice_loc = id_loc_node.dupe();
    // A `declare enum` is ambient regardless of file extension, like any enum in a
    // .d.ts file.
    let ambient = opts.is_dts_file || declared;
    let is_ts_file = opts.is_ts_file;
    let def = if opts.enable_enums {
        Some(tbls.lazy(Box::new(move |_, _, tbls| {
            tbls.splice(splice_loc, |tbls| {
                enum_def(is_ts_file, ambient, tbls, &body_loc, body)
            })
        })))
    } else {
        None
    };
    scope::bind_enum(opts, scope, scopes, tbls, id_loc_node, name.clone(), def, k);
}

fn export_named_decl<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    export_comments: Option<&ast::Syntax<Loc, ()>>,
    kind: ast::statement::ExportKind,
    stmt: &'ast ast::statement::Statement<Loc, Loc>,
) {
    let export_binding_fn = |scopes: &mut scope::Scopes<'arena, 'ast>,
                             name: &FlowSmolStr,
                             binding: LocalDefNode<'arena, 'ast>| {
        scope::export_binding(scopes, scope, kind, name.clone(), binding);
    };
    match stmt.deref() {
        StatementInner::FunctionDeclaration { inner, .. } => {
            function_decl(scope, scopes, tbls, inner, &export_binding_fn)
        }
        StatementInner::ClassDeclaration { inner, .. } => {
            class_decl(opts, scope, scopes, tbls, inner, &export_binding_fn)
        }
        StatementInner::RecordDeclaration { inner, .. } => {
            record_decl(opts, scope, scopes, tbls, inner, &export_binding_fn)
        }
        StatementInner::TypeAlias { inner, .. } => type_alias_decl(
            opts,
            scope,
            scopes,
            tbls,
            export_comments,
            inner,
            &export_binding_fn,
        ),
        StatementInner::OpaqueType { inner, .. } => {
            opaque_type_decl(opts, scope, scopes, tbls, inner, &export_binding_fn)
        }
        StatementInner::InterfaceDeclaration { inner, .. } => {
            interface_decl(opts, scope, scopes, tbls, inner, &export_binding_fn)
        }
        StatementInner::VariableDeclaration { inner, .. } => {
            variable_decls(opts, scope, scopes, tbls, inner, &export_binding_fn)
        }
        StatementInner::EnumDeclaration { inner, .. } => {
            enum_decl(opts, scope, scopes, tbls, inner, false, &export_binding_fn)
        }
        StatementInner::ComponentDeclaration { inner, .. } => {
            component_decl(opts, scope, scopes, tbls, inner, &export_binding_fn)
        }
        _ => panic!("unexpected export declaration"),
    }
}

fn declare_export_decl<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    export_comments: Option<&ast::Syntax<Loc, ()>>,
    default: Option<LocNode<'arena>>,
    decl: &'ast ast::statement::declare_export_declaration::Declaration<Loc, Loc>,
) {
    use ast::statement::ExportKind;
    use ast::statement::declare_export_declaration::Declaration as D;

    let export_maybe_default_binding =
        |scopes: &mut scope::Scopes<'arena, 'ast>,
         name: &FlowSmolStr,
         binding: LocalDefNode<'arena, 'ast>| {
            match &default {
                None => scope::export_binding(
                    scopes,
                    scope,
                    ExportKind::ExportValue,
                    name.clone(),
                    binding,
                ),
                Some(default_loc) => {
                    scope::export_default_binding(scopes, scope, default_loc, name.clone(), binding)
                }
            }
        };

    match decl {
        D::Variable {
            loc: _,
            declaration: v,
        } => {
            declare_variable_decl(opts, scope, scopes, tbls, v, &|scopes, name, binding| {
                scope::export_binding(
                    scopes,
                    scope,
                    ExportKind::ExportValue,
                    name.clone(),
                    binding,
                );
            });
        }
        D::Function {
            loc: _,
            declaration: f,
        } => match &f.id {
            Some(_) => {
                declare_function_decl(scope, scopes, tbls, f, &export_maybe_default_binding);
            }
            None => {
                let default_loc = default.as_ref().unwrap().dupe();
                let mut xs = tparam_stack::TParamStack::new();
                let def = annot(opts, scope, scopes, tbls, &mut xs, &f.annot.annotation);
                scope::export_default(scopes, scope, default_loc, def);
            }
        },
        D::Class {
            loc: _,
            declaration: c,
        } => {
            declare_class_decl(opts, scope, scopes, tbls, c, &export_maybe_default_binding);
        }
        D::Component {
            loc,
            declaration: c,
        } => {
            declare_component_decl(scope, scopes, tbls, loc, c, &export_maybe_default_binding);
        }
        D::Enum {
            loc: _,
            declaration: enum_,
        } => {
            // `declare export enum` is ambient regardless of file extension.
            enum_decl(
                opts,
                scope,
                scopes,
                tbls,
                enum_,
                true,
                &|scopes, name, binding| {
                    scope::export_binding(
                        scopes,
                        scope,
                        ExportKind::ExportValue,
                        name.clone(),
                        binding,
                    );
                },
            );
        }
        D::DefaultType { type_: t } => {
            let default_loc = default.expect("default type must have default location");
            let mut xs = tparam_stack::TParamStack::new();
            let def = annot(opts, scope, scopes, tbls, &mut xs, t);
            scope::export_default(scopes, scope, default_loc, def);
        }
        D::NamedType {
            loc: _,
            declaration: t,
        } => {
            type_alias_decl(
                opts,
                scope,
                scopes,
                tbls,
                export_comments,
                t,
                &|scopes, name, binding| {
                    scope::export_binding(
                        scopes,
                        scope,
                        ExportKind::ExportType,
                        name.clone(),
                        binding,
                    );
                },
            );
        }
        D::NamedOpaqueType {
            loc: _,
            declaration: t,
        } => {
            opaque_type_decl(opts, scope, scopes, tbls, t, &|scopes, name, binding| {
                scope::export_binding(scopes, scope, ExportKind::ExportType, name.clone(), binding);
            });
        }
        D::Interface {
            loc: _,
            declaration: i,
        } => {
            interface_decl(opts, scope, scopes, tbls, i, &|scopes, name, binding| {
                scope::export_binding(scopes, scope, ExportKind::ExportType, name.clone(), binding);
            });
        }
        D::Namespace {
            loc: _,
            declaration: ns,
        } => {
            // Handle exported namespace - process the namespace body and export the binding
            match &ns.id {
                ast::statement::declare_namespace::Id::Global(_) => {}
                ast::statement::declare_namespace::Id::Local(id) => {
                    let id_loc = tbls.push_loc(id.loc.dupe());
                    let name = id.name.dupe();
                    let is_type_only = ns
                        .body
                        .1
                        .body
                        .iter()
                        .all(ast_utils::is_type_only_declaration_statement);
                    let stmts = ns.body.1.body.iter().filter(|stmt| {
                        ast_utils::acceptable_statement_in_declaration_context(
                            ast_utils::DeclarationContext::DeclareNamespace,
                            stmt,
                        )
                        .is_ok()
                    });
                    let ns_scope = scope::push_declare_namespace(scopes, scope, name.dupe());
                    for s in stmts {
                        statement(opts, ns_scope, scopes, tbls, s);
                    }
                    scope::resolve_declare_namespace_exports(opts, scopes, ns_scope);
                    scope::finalize_declare_namespace_exported_exn(
                        ns_scope,
                        scopes,
                        tbls,
                        is_type_only,
                        id_loc,
                        name,
                        |scopes, name, binding| {
                            scope::export_binding(
                                scopes,
                                scope,
                                ExportKind::ExportValue,
                                name.clone(),
                                binding,
                            );
                        },
                    );
                }
            }
        }
    }
}

fn export_default_decl<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    decl: &'ast ast::statement::ExportDefaultDeclaration<Loc, Loc>,
) {
    use ast::statement::StatementInner as S;
    use ast::statement::export_default_declaration::Declaration as D;

    let ast::statement::ExportDefaultDeclaration {
        default: default_loc,
        declaration,
        comments: _,
    } = decl;
    let default_loc = tbls.push_loc(default_loc.dupe());

    match declaration {
        D::Declaration(stmt) => match stmt.deref() {
            S::InterfaceDeclaration { inner: decl, .. } => {
                interface_decl(opts, scope, scopes, tbls, decl, &|_, _, _| {});
            }
            S::ClassDeclaration { loc, inner: decl } => {
                let loc_node = tbls.push_loc(loc.dupe());
                match &decl.id {
                    Some(_) => {
                        class_decl(opts, scope, scopes, tbls, decl, &|scopes, name, binding| {
                            scope::export_default_binding(
                                scopes,
                                scope,
                                &default_loc,
                                name.clone(),
                                binding,
                            );
                        });
                    }
                    None => {
                        let def = class_def(opts, scope, scopes, tbls, decl);
                        let def = Parsed::Value(Box::new(ParsedValue::ClassExpr(Box::new((
                            loc_node, def,
                        )))));
                        scope::export_default(scopes, scope, default_loc, def);
                    }
                }
            }
            S::RecordDeclaration {
                loc: _,
                inner: decl,
            } => {
                record_decl(opts, scope, scopes, tbls, decl, &|scopes, name, binding| {
                    scope::export_default_binding(
                        scopes,
                        scope,
                        &default_loc,
                        name.clone(),
                        binding,
                    );
                });
            }
            S::FunctionDeclaration {
                loc,
                inner: fun_decl,
            } => {
                let loc_node = tbls.push_loc(loc.dupe());
                match &fun_decl.id {
                    Some(_) => {
                        function_decl(scope, scopes, tbls, fun_decl, &|scopes, name, binding| {
                            scope::export_default_binding(
                                scopes,
                                scope,
                                &default_loc,
                                name.clone(),
                                binding,
                            );
                        });
                    }
                    None => {
                        let mut xs = tparam_stack::TParamStack::new();
                        let def = function_def(
                            opts,
                            scope,
                            scopes,
                            tbls,
                            &mut xs,
                            loc_node.dupe(),
                            fun_decl,
                        );
                        let statics = BTreeMap::new();
                        let def =
                            Parsed::Value(Box::new(ParsedValue::FunExpr(Box::new(ValueFunExpr {
                                loc: loc_node,
                                async_: fun_decl.async_,
                                generator: fun_decl.generator,
                                def,
                                statics,
                            }))));
                        scope::export_default(scopes, scope, default_loc, def);
                    }
                }
            }
            S::EnumDeclaration {
                loc: _,
                inner: decl,
            } => {
                enum_decl(
                    opts,
                    scope,
                    scopes,
                    tbls,
                    decl,
                    false,
                    &|scopes, name, binding| {
                        scope::export_default_binding(
                            scopes,
                            scope,
                            &default_loc,
                            name.clone(),
                            binding,
                        );
                    },
                );
            }
            S::ComponentDeclaration {
                loc: _,
                inner: decl,
            } => {
                component_decl(opts, scope, scopes, tbls, decl, &|scopes, name, binding| {
                    scope::export_default_binding(
                        scopes,
                        scope,
                        &default_loc,
                        name.clone(),
                        binding,
                    );
                });
            }
            _ => {
                panic!("unexpected default export declaration");
            }
        },
        D::Expression(expr) => {
            let def = expression(opts, scope, scopes, tbls, FrozenKind::NotFrozen, expr);
            scope::export_default(scopes, scope, default_loc, def);
        }
    }
}

fn export_specifiers<'arena, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    kind: ast::statement::ExportKind,
    source: &Option<(Loc, ast::StringLiteral<Loc>)>,
    specifiers: &'ast ast::statement::export_named_declaration::Specifier<Loc, Loc>,
) {
    use ast::statement::export_named_declaration::Specifier;

    let source_opt = source.as_ref().map(|(_, s)| s.value.dupe());

    match specifiers {
        Specifier::ExportBatchSpecifier(
            ast::statement::export_named_declaration::ExportBatchSpecifier {
                loc: _,
                specifier: Some(id),
            },
        ) => {
            let mref = source_opt.expect("export batch specifier must have source");
            let mref = Userland::from_smol_str(mref);
            let id_loc = tbls.push_loc(id.loc.dupe());
            let name = id.name.dupe();
            scope::export_ns(scopes, scope, tbls, kind, mref, id_loc, name);
        }
        Specifier::ExportBatchSpecifier(
            ast::statement::export_named_declaration::ExportBatchSpecifier {
                loc,
                specifier: None,
            },
        ) => {
            let loc_node = tbls.push_loc(loc.clone());
            let mref = source_opt.expect("export batch specifier must have source");
            let mref = Userland::from_smol_str(mref);
            scope::export_star(scopes, scope, tbls, kind, loc_node, mref);
        }
        Specifier::ExportSpecifiers(specifiers_list) => {
            for spec in specifiers_list {
                let specifier_kind =
                    flow_parser::ast_utils::effective_export_kind(kind, spec.export_kind);
                match &source_opt {
                    None => {
                        scope::export_ref(
                            opts,
                            scopes,
                            scope,
                            tbls,
                            specifier_kind,
                            &spec.local,
                            spec.exported.as_ref(),
                        );
                    }
                    Some(mref) => {
                        let mref = Userland::from_smol_str(mref.clone());
                        scope::export_from(
                            opts,
                            scopes,
                            scope,
                            tbls,
                            specifier_kind,
                            mref,
                            &spec.local,
                            spec.exported.as_ref(),
                        );
                    }
                }
            }
        }
    }
}

fn assignment<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    a: &'ast ast::expression::Assignment<Loc, Loc>,
) {
    use ast::expression::ExpressionInner as E;
    use ast::pattern::Pattern as P;

    // Helper to check if this is a module.exports = ... assignment
    fn is_module_exports_assign<'a>(
        left: &'a ast::pattern::Pattern<Loc, Loc>,
        scopes: &scope::Scopes<'_, '_>,
        scope: ScopeId,
    ) -> Option<()> {
        if let P::Expression { inner, .. } = left {
            if let E::Member { inner: member, .. } = inner.deref().deref() {
                if let E::Identifier {
                    inner: object_id, ..
                } = member.object.deref()
                {
                    if let ast::expression::member::Property::PropertyIdentifier(prop_id) =
                        &member.property
                    {
                        if object_id.name.as_str() == "module"
                            && prop_id.name.as_str() == "exports"
                            && scope::lookup_value(scopes, scope, &object_id.name).is_none()
                        {
                            return Some(());
                        }
                    }
                }
            }
        }
        None
    }

    // Helper to check if this is an exports.foo = ... assignment
    fn is_exports_prop_assign<'a>(
        left: &'a ast::pattern::Pattern<Loc, Loc>,
        scopes: &scope::Scopes<'_, '_>,
        scope: ScopeId,
    ) -> Option<&'a ast::Identifier<Loc, Loc>> {
        if let P::Expression { inner, .. } = left {
            if let E::Member { inner: member, .. } = inner.deref().deref() {
                if let E::Identifier {
                    inner: object_id, ..
                } = member.object.deref()
                {
                    if let ast::expression::member::Property::PropertyIdentifier(prop_id) =
                        &member.property
                    {
                        if object_id.name.as_str() == "exports"
                            && scope::lookup_value(scopes, scope, &object_id.name).is_none()
                        {
                            return Some(prop_id);
                        }
                    }
                }
            }
        }
        None
    }

    // Helper to check if this is a module.exports.foo = ... assignment
    fn is_module_exports_prop_assign<'a>(
        left: &'a ast::pattern::Pattern<Loc, Loc>,
        scopes: &scope::Scopes<'_, '_>,
        scope: ScopeId,
    ) -> Option<&'a ast::Identifier<Loc, Loc>> {
        if let P::Expression { inner, .. } = left {
            if let E::Member { inner: member, .. } = inner.deref().deref() {
                // Check if object is module.exports
                if let E::Member {
                    inner: inner_member,
                    ..
                } = member.object.deref()
                {
                    if let E::Identifier {
                        inner: module_id, ..
                    } = inner_member.object.deref()
                    {
                        if let ast::expression::member::Property::PropertyIdentifier(exports_id) =
                            &inner_member.property
                        {
                            if let ast::expression::member::Property::PropertyIdentifier(prop_id) =
                                &member.property
                            {
                                if module_id.name.as_str() == "module"
                                    && exports_id.name.as_str() == "exports"
                                    && scope::lookup_value(scopes, scope, &module_id.name).is_none()
                                {
                                    return Some(prop_id);
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    match a.operator {
        None => {
            // module.exports = ...
            if is_module_exports_assign(&a.left, scopes, scope).is_some() {
                let t = expression(opts, scope, scopes, tbls, FrozenKind::NotFrozen, &a.right);
                scope::cjs_clobber(scopes, scope, t);
                return;
            }

            // exports.foo = ...
            if let Some(prop_id) = is_exports_prop_assign(&a.left, scopes, scope) {
                let id_loc = tbls.push_loc(prop_id.loc.dupe());
                let t = expression(opts, scope, scopes, tbls, FrozenKind::NotFrozen, &a.right);
                let name = prop_id.name.dupe();
                scope::cjs_set_prop(
                    scopes,
                    scope,
                    name.clone(),
                    (id_loc, t),
                    tbls.strictness_kind,
                );
                return;
            }

            // module.exports.foo = ...
            if let Some(prop_id) = is_module_exports_prop_assign(&a.left, scopes, scope) {
                let id_loc = tbls.push_loc(prop_id.loc.dupe());
                let t = expression(opts, scope, scopes, tbls, FrozenKind::NotFrozen, &a.right);
                let name = prop_id.name.dupe();
                scope::cjs_set_prop(
                    scopes,
                    scope,
                    name.clone(),
                    (id_loc, t),
                    tbls.strictness_kind,
                );
                return;
            }
        }
        Some(_) => {}
    }

    // Check for expression pattern that is a member expression assignment
    if let P::Expression { inner, .. } = &a.left {
        if let E::Member { inner: member, .. } = inner.deref().deref() {
            if let E::Identifier {
                inner: object_id, ..
            } = member.object.deref()
            {
                if let ast::expression::member::Property::PropertyIdentifier(_prop_id) =
                    &member.property
                {
                    // Only process if it's a known global object (this, exports, module)
                    let obj_name = object_id.name.as_str();
                    if obj_name == "this"
                        || obj_name == "exports"
                        || scope::lookup_value(scopes, scope, &object_id.name).is_none()
                    {
                        // Fall through to default handling
                    }
                }
            }
        }
    }

    // id.foo = ... case - handle member assignment on identifier
    if a.operator.is_none() {
        if let P::Expression { inner, .. } = &a.left {
            if let E::Member { inner: member, .. } = inner.deref().deref() {
                if let E::Identifier { inner: id, .. } = member.object.deref() {
                    if let ast::expression::member::Property::PropertyIdentifier(prop_id) =
                        &member.property
                    {
                        let id_loc = tbls.push_loc(prop_id.loc.dupe());
                        let t =
                            expression(opts, scope, scopes, tbls, FrozenKind::NotFrozen, &a.right);
                        scope::assign_binding(
                            scopes,
                            scope,
                            prop_id.name.dupe(),
                            (id_loc, t),
                            &id.name,
                        );
                    }
                }
            }
        }
    }

    // TODO: Assignment expressions can alter the actual type of something in
    // a way that is currently invisible to the signature extractor.
}

pub(super) fn statement<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    stmt: &'ast ast::statement::Statement<Loc, Loc>,
) {
    use ast::statement::StatementInner as S;
    fn block<'arena: 'ast, 'ast>(
        opts: &TypeSigOptions,
        scope: ScopeId,
        scopes: &mut scope::Scopes<'arena, 'ast>,
        tbls: &mut Tables<'arena, 'ast>,
        block: &'ast ast::statement::Block<Loc, Loc>,
    ) {
        let ast::statement::Block { body, .. } = block;
        let scope = scope::push_lex(scopes, scope);
        for stmt in body.iter() {
            statement(opts, scope, scopes, tbls, stmt);
        }
    }

    match stmt.deref() {
        S::TypeAlias {
            loc: _,
            inner: decl,
        } => {
            type_alias_decl(opts, scope, scopes, tbls, None, decl, &|_, _, _| {});
        }
        S::DeclareTypeAlias {
            loc: _,
            inner: decl,
        } => {
            type_alias_decl(opts, scope, scopes, tbls, None, decl, &|_, _, _| {});
        }
        S::OpaqueType {
            loc: _,
            inner: decl,
        } => {
            opaque_type_decl(opts, scope, scopes, tbls, decl, &|_, _, _| {});
        }
        S::DeclareOpaqueType {
            loc: _,
            inner: decl,
        } => {
            opaque_type_decl(opts, scope, scopes, tbls, decl, &|_, _, _| {});
        }
        S::ClassDeclaration {
            loc: _,
            inner: decl,
        } => {
            class_decl(opts, scope, scopes, tbls, decl, &|_, _, _| {});
        }
        S::RecordDeclaration {
            loc: _,
            inner: decl,
        } => {
            record_decl(opts, scope, scopes, tbls, decl, &|_, _, _| {});
        }
        S::DeclareClass {
            loc: _,
            inner: decl,
        } => {
            declare_class_decl(opts, scope, scopes, tbls, decl, &|_, _, _| {});
        }
        S::DeclareComponent { loc, inner: decl } => {
            declare_component_decl(scope, scopes, tbls, loc, decl, &|_, _, _| {});
        }
        S::InterfaceDeclaration {
            loc: _,
            inner: decl,
        } => {
            interface_decl(opts, scope, scopes, tbls, decl, &|_, _, _| {});
        }
        S::DeclareInterface {
            loc: _,
            inner: decl,
        } => {
            interface_decl(opts, scope, scopes, tbls, decl, &|_, _, _| {});
        }
        S::FunctionDeclaration {
            loc: _,
            inner: decl,
        } => {
            function_decl(scope, scopes, tbls, decl, &|_, _, _| {});
        }
        S::ComponentDeclaration {
            loc: _,
            inner: decl,
        } => {
            component_decl(opts, scope, scopes, tbls, decl, &|_, _, _| {});
        }
        S::DeclareFunction {
            loc: _,
            inner: decl,
        } => {
            declare_function_decl(scope, scopes, tbls, decl, &|_, _, _| {});
        }
        S::ImportDeclaration {
            loc: _,
            inner: decl,
        } => {
            match scopes.get(scope) {
                scope::Scope::Global { .. } => {
                    // Illegal in libdefs; errors from type_inference.rs
                    // (EUnsupportedSyntax ToplevelLibraryImport).
                }
                _ => {
                    import_decl(scope, scopes, tbls, decl);
                }
            }
        }
        S::ExportNamedDeclaration { loc: _, inner } => {
            let ast::statement::ExportNamedDeclaration {
                export_kind,
                source,
                specifiers,
                declaration,
                attributes: _,
                comments,
            } = inner.as_ref();
            if let Some(stmt) = declaration {
                export_named_decl(
                    opts,
                    scope,
                    scopes,
                    tbls,
                    comments.as_ref(),
                    *export_kind,
                    stmt,
                );
            }
            if let Some(specifiers) = specifiers {
                export_specifiers(opts, scope, scopes, tbls, *export_kind, source, specifiers);
            }
        }
        // walk lex scopes to collect hoisted names in scope
        S::Block { loc: _, inner } => {
            block(opts, scope, scopes, tbls, inner.as_ref());
        }
        S::DoWhile { loc: _, inner } => {
            let ast::statement::DoWhile { body, .. } = inner.as_ref();
            statement(opts, scope, scopes, tbls, body);
        }
        S::For { loc: _, inner } => {
            let ast::statement::For { init, body, .. } = inner.as_ref();
            let scope = match init {
                None => scope,
                Some(ast::statement::for_::Init::InitExpression(_)) => scope,
                Some(ast::statement::for_::Init::InitDeclaration((_, decl))) => {
                    let scope = scope::push_lex(scopes, scope);
                    variable_decls(opts, scope, scopes, tbls, decl, &|_, _, _| {});
                    scope
                }
            };
            statement(opts, scope, scopes, tbls, body);
        }
        S::ForIn { loc: _, inner } => {
            let ast::statement::ForIn { left, body, .. } = inner.as_ref();
            let scope = match left {
                ast::statement::for_in::Left::LeftPattern(_) => scope,
                ast::statement::for_in::Left::LeftDeclaration((_, decl)) => {
                    let scope = scope::push_lex(scopes, scope);
                    variable_decls(opts, scope, scopes, tbls, decl, &|_, _, _| {});
                    scope
                }
            };
            statement(opts, scope, scopes, tbls, body);
        }
        S::ForOf { loc: _, inner } => {
            let ast::statement::ForOf { left, body, .. } = inner.as_ref();
            let scope = match left {
                ast::statement::for_of::Left::LeftPattern(_) => scope,
                ast::statement::for_of::Left::LeftDeclaration((_, decl)) => {
                    let scope = scope::push_lex(scopes, scope);
                    variable_decls(opts, scope, scopes, tbls, decl, &|_, _, _| {});
                    scope
                }
            };
            statement(opts, scope, scopes, tbls, body);
        }
        S::If { loc: _, inner } => {
            let ast::statement::If {
                consequent,
                alternate,
                ..
            } = inner.as_ref();
            statement(opts, scope, scopes, tbls, consequent);
            if let Some(ast::statement::if_::Alternate { body, .. }) = alternate {
                statement(opts, scope, scopes, tbls, body);
            }
        }
        S::Match { loc: _, inner } => {
            let ast::statement::MatchStatement { cases, .. } = inner.as_ref();
            for case_ in cases.iter() {
                statement(opts, scope, scopes, tbls, &case_.body);
            }
        }
        S::Switch { loc: _, inner } => {
            let ast::statement::Switch { cases, .. } = inner.as_ref();
            let scope = scope::push_lex(scopes, scope);
            for case_ in cases.iter() {
                for stmt in case_.consequent.iter() {
                    statement(opts, scope, scopes, tbls, stmt);
                }
            }
        }
        S::Try { loc: _, inner } => {
            block(opts, scope, scopes, tbls, &inner.block.1);
            if let Some(h) = &inner.handler {
                block(opts, scope, scopes, tbls, &h.body.1);
            }
            if let Some(finalizer) = &inner.finalizer {
                block(opts, scope, scopes, tbls, &finalizer.1);
            }
        }
        S::While { loc: _, inner } => {
            let ast::statement::While { body, .. } = inner.as_ref();
            statement(opts, scope, scopes, tbls, body);
        }
        S::Labeled { loc: _, inner } => {
            let ast::statement::Labeled { body, .. } = inner.as_ref();
            statement(opts, scope, scopes, tbls, body);
        }
        S::VariableDeclaration {
            loc: _,
            inner: decl,
        } => {
            variable_decls(opts, scope, scopes, tbls, decl, &|_, _, _| {});
        }
        S::DeclareVariable {
            loc: _,
            inner: decl,
        } => {
            declare_variable_decl(opts, scope, scopes, tbls, decl, &|_, _, _| {});
        }
        S::DeclareExportDeclaration { loc: _, inner } => {
            let ast::statement::DeclareExportDeclaration {
                default,
                declaration,
                specifiers,
                source,
                comments,
            } = inner.as_ref();
            let default = default.as_ref().map(|loc| tbls.push_loc(loc.clone()));
            if let Some(declaration) = declaration {
                declare_export_decl(
                    opts,
                    scope,
                    scopes,
                    tbls,
                    comments.as_ref(),
                    default,
                    declaration,
                );
            }
            if let Some(specifiers) = specifiers {
                export_specifiers(
                    opts,
                    scope,
                    scopes,
                    tbls,
                    ast::statement::ExportKind::ExportValue,
                    source,
                    specifiers,
                );
            }
        }
        S::ExportDefaultDeclaration {
            loc: _,
            inner: decl,
        } => {
            export_default_decl(opts, scope, scopes, tbls, decl);
        }
        S::Expression { loc: _, inner } => {
            let ast::statement::Expression { expression, .. } = inner.as_ref();
            if let ExpressionInner::Assignment { loc: _, inner: a } = expression.deref() {
                assignment(opts, scope, scopes, tbls, a);
            }
        }
        S::DeclareModuleExports { loc: _, inner } => {
            let ast::statement::DeclareModuleExports {
                annot: annot_field, ..
            } = inner.as_ref();
            let mut xs = tparam_stack::TParamStack::new();
            let t = annot(opts, scope, scopes, tbls, &mut xs, &annot_field.annotation);
            scope::cjs_clobber(scopes, scope, t);
        }
        S::ExportAssignment { loc: _, inner } => {
            let ast::statement::ExportAssignment { rhs, comments: _ } = inner.as_ref();
            match rhs {
                ast::statement::ExportAssignmentRhs::Expression(e) => {
                    let t = expression(opts, scope, scopes, tbls, FrozenKind::NotFrozen, e);
                    scope::cjs_clobber(scopes, scope, t);
                }
                ast::statement::ExportAssignmentRhs::DeclareFunction(
                    _,
                    ast::statement::DeclareFunction {
                        annot: annot_field, ..
                    },
                ) => {
                    let mut xs = tparam_stack::TParamStack::new();
                    let t = annot(opts, scope, scopes, tbls, &mut xs, &annot_field.annotation);
                    scope::cjs_clobber(scopes, scope, t);
                }
            }
        }
        S::NamespaceExportDeclaration { .. } => {}
        S::ImportEqualsDeclaration { loc: _, inner } => {
            let ast::statement::ImportEqualsDeclaration {
                id,
                module_reference,
                import_kind,
                is_export,
                comments: _,
            } = inner.as_ref();
            match scopes.get(scope) {
                scope::Scope::Global { .. } => {
                    // Illegal in libdefs; errors from type_inference.rs
                    // (EUnsupportedSyntax ToplevelLibraryImport). Mirrors the
                    // ImportDeclaration guard above.
                }
                _ => {
                    let id_loc = tbls.push_loc(id.loc.dupe());
                    let local = id.name.dupe();
                    match module_reference {
                        ast::statement::import_equals_declaration::ModuleReference::ExternalModuleReference(
                            _,
                            lit,
                        ) => {
                            let mref = Userland::from_smol_str(lit.value.dupe());
                            let mref = tbls.push_module_ref(mref);
                            scope::bind_import(
                                scope,
                                scopes,
                                tbls,
                                *import_kind,
                                id_loc,
                                local,
                                FlowSmolStr::new_inline("default"),
                                mref,
                            );
                            if *is_export {
                                let export_kind = match import_kind {
                                    ast::statement::ImportKind::ImportType => {
                                        ast::statement::ExportKind::ExportType
                                    }
                                    _ => ast::statement::ExportKind::ExportValue,
                                };
                                scope::export_ref(opts, scopes, scope, tbls, export_kind, id, None);
                            }
                        }
                        ast::statement::import_equals_declaration::ModuleReference::Identifier(_) => {
                            // import Foo = A.B.C: qualified name form is not supported;
                            // bind as any so downstream code doesn't break.
                            let id_loc_for_any = id_loc.dupe();
                            let def = tbls.lazy(Box::new(move |_, _, _| {
                                Parsed::Annot(Box::new(ParsedAnnot::Any(Box::new(id_loc_for_any))))
                            }));
                            scope::bind_var(
                                scope,
                                scopes,
                                tbls,
                                ast::VariableKind::Const,
                                id_loc,
                                local,
                                def,
                                |_, _, _| {},
                            );
                            if *is_export {
                                let export_kind = match import_kind {
                                    ast::statement::ImportKind::ImportType => {
                                        ast::statement::ExportKind::ExportType
                                    }
                                    _ => ast::statement::ExportKind::ExportValue,
                                };
                                scope::export_ref(opts, scopes, scope, tbls, export_kind, id, None);
                            }
                        }
                    }
                }
            }
        }
        S::DeclareModule { loc: _, inner } => {
            let ast::statement::DeclareModule { id, body, .. } = inner.as_ref();
            use ast::statement::declare_module::Id;
            let (loc, name) = match id {
                Id::Identifier(id_inner) => {
                    (tbls.push_loc(id_inner.loc.dupe()), id_inner.name.dupe())
                }
                Id::Literal((loc, lit)) => (tbls.push_loc(loc.dupe()), lit.value.dupe()),
            };
            let scope = scope::push_declare_module(scopes, tbls, scope, loc, name);
            let (_, block) = body;
            let ast::statement::Block { body: stmts, .. } = block;
            for stmt in stmts.iter() {
                match ast_utils::acceptable_statement_in_declaration_context(
                    ast_utils::DeclarationContext::DeclareModule,
                    stmt,
                ) {
                    Ok(_) => {
                        statement(opts, scope, scopes, tbls, stmt);
                    }
                    Err(_) => {}
                }
            }
            scope::finalize_declare_module_exports_exn(scopes, scope, tbls.strictness_kind);
        }
        S::DeclareNamespace {
            loc: _,
            inner: decl,
        } => {
            let is_type_only = ast_utils::is_type_only_declaration_statement(stmt);
            namespace_decl(opts, scope, scopes, tbls, is_type_only, decl, statement);
        }
        // `declare enum` is ambient regardless of file extension.
        S::DeclareEnum {
            loc: _,
            inner: decl,
        } => {
            enum_decl(opts, scope, scopes, tbls, decl, true, &|_, _, _| {});
        }
        S::EnumDeclaration {
            loc: _,
            inner: decl,
        } => {
            enum_decl(opts, scope, scopes, tbls, decl, false, &|_, _, _| {});
        }
        // unsupported
        S::With { .. } => {}
        // statements that won't introduce a top-level type or name in module scope
        S::Break { .. }
        | S::Continue { .. }
        | S::Debugger { .. }
        | S::Empty { .. }
        | S::Return { .. }
        | S::Throw { .. } => {}
    }
}

// ---------------------------------------------------------------------------
// Lexical annotation roots for targeted signatures
//
// Function bodies are irrelevant to signatures, so bindings inside them (and
// parameters of functions whose signatures were never forced) never enter
// scopes. A targeted root naming such a binding therefore resolves to
// nothing. When the binding carries its own annotation, bind it here from
// the annotation alone, so scope+provider-targeted analyses can address
// parameters and function-local definitions. Function-local declarations
// (`declare function`, `declare const`, `function`) and classes outside
// module scope (class expressions, local classes) bind through the same
// handlers module-level ones use.
struct LexicalAnnotRoot<'ast> {
    loc: Loc,
    name: FlowSmolStr,
    annotation: &'ast ast::types::Annotation<Loc, Loc>,
    kind: LexicalAnnotKind,
}

enum LexicalAnnotKind {
    Param,
    Var(ast::VariableKind),
}

struct LexicalDeclRoot<'ast> {
    loc: Loc,
    name: FlowSmolStr,
    decl: LexicalDecl<'ast>,
}

enum LexicalDecl<'ast> {
    // All same-named overloads in the statement list, so they merge into one
    // binding as they do at module level.
    DeclareFunctions(Vec<&'ast ast::statement::DeclareFunction<Loc, Loc>>),
    DeclareVariable(&'ast ast::statement::DeclareVariable<Loc, Loc>),
    Function(&'ast ast::function::Function<Loc, Loc>),
    Class(&'ast ast::class::Class<Loc, Loc>),
}

struct LexicalAnnotCollector<'a, 'ast> {
    targets: &'a BTreeSet<Loc>,
    roots: Vec<LexicalAnnotRoot<'ast>>,
    decls: Vec<LexicalDeclRoot<'ast>>,
}

impl<'a, 'ast> LexicalAnnotCollector<'a, 'ast> {
    fn record_identifier(
        &mut self,
        kind: LexicalAnnotKind,
        id: &'ast ast::pattern::Identifier<Loc, Loc>,
    ) {
        let ast::types::AnnotationOrHint::Available(annotation) = &id.annot else {
            return;
        };
        let loc = id.name.loc.dupe();
        if !self.targets.contains(&loc) {
            return;
        }
        self.roots.push(LexicalAnnotRoot {
            loc,
            name: id.name.name.dupe(),
            annotation,
            kind,
        });
    }

    fn record_pattern(
        &mut self,
        kind: LexicalAnnotKind,
        pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
    ) {
        if let ast::pattern::Pattern::Identifier { inner, .. } = pattern {
            self.record_identifier(kind, inner);
        }
    }

    fn record_declarations(&mut self, stmts: &'ast [ast::statement::Statement<Loc, Loc>]) {
        let declare_functions_named = |name: &FlowSmolStr| {
            stmts
                .iter()
                .filter_map(|stmt| match stmt.deref() {
                    StatementInner::DeclareFunction { inner, .. }
                        if inner.id.as_ref().is_some_and(|id| id.name == *name) =>
                    {
                        Some(inner.as_ref())
                    }
                    _ => None,
                })
                .collect()
        };
        for stmt in stmts {
            match stmt.deref() {
                StatementInner::DeclareFunction { inner, .. }
                    if let Some(id) = inner.id.as_ref()
                        && self.targets.contains(&id.loc) =>
                {
                    self.decls.push(LexicalDeclRoot {
                        loc: id.loc.dupe(),
                        name: id.name.dupe(),
                        decl: LexicalDecl::DeclareFunctions(declare_functions_named(&id.name)),
                    });
                }
                StatementInner::DeclareVariable { inner, .. } => {
                    for declarator in inner.declarations.iter() {
                        if let ast::pattern::Pattern::Identifier { inner: id, .. } = &declarator.id
                            && self.targets.contains(&id.name.loc)
                        {
                            self.decls.push(LexicalDeclRoot {
                                loc: id.name.loc.dupe(),
                                name: id.name.name.dupe(),
                                decl: LexicalDecl::DeclareVariable(inner),
                            });
                        }
                    }
                }
                StatementInner::FunctionDeclaration { inner, .. }
                    if let Some(id) = inner.id.as_ref()
                        && self.targets.contains(&id.loc) =>
                {
                    self.decls.push(LexicalDeclRoot {
                        loc: id.loc.dupe(),
                        name: id.name.dupe(),
                        decl: LexicalDecl::Function(inner),
                    });
                }
                _ => {}
            }
        }
    }
}

impl<'a, 'ast> AstVisitor<'ast, Loc> for LexicalAnnotCollector<'a, 'ast> {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn class_(
        &mut self,
        loc: &'ast Loc,
        class: &'ast ast::class::Class<Loc, Loc>,
    ) -> Result<(), !> {
        let (binding_loc, name) = class.id.as_ref().map_or_else(
            || (loc.dupe(), FlowSmolStr::new_inline("<anonymous class>")),
            |id| (id.loc.dupe(), id.name.dupe()),
        );
        if self.targets.contains(&binding_loc) {
            self.decls.push(LexicalDeclRoot {
                loc: binding_loc,
                name,
                decl: LexicalDecl::Class(class),
            });
        }
        ast_visitor::class_default(self, loc, class)
    }

    fn function_this_param(
        &mut self,
        param: &'ast ast::function::ThisParam<Loc, Loc>,
    ) -> Result<(), !> {
        if self.targets.contains(&param.loc) {
            self.roots.push(LexicalAnnotRoot {
                loc: param.loc.dupe(),
                name: FlowSmolStr::new_inline("this"),
                annotation: &param.annot,
                kind: LexicalAnnotKind::Param,
            });
        }
        ast_visitor::function_this_param_default(self, param)
    }

    fn function_param(&mut self, param: &'ast ast::function::Param<Loc, Loc>) -> Result<(), !> {
        if let ast::function::Param::RegularParam { argument, .. } = param {
            self.record_pattern(LexicalAnnotKind::Param, argument);
        }
        ast_visitor::function_param_default(self, param)
    }

    fn variable_declaration(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::VariableDeclaration<Loc, Loc>,
    ) -> Result<(), !> {
        for declarator in decl.declarations.iter() {
            if let ast::pattern::Pattern::Identifier { inner, .. } = &declarator.id {
                self.record_identifier(LexicalAnnotKind::Var(decl.kind), inner);
            }
        }
        ast_visitor::variable_declaration_default(self, loc, decl)
    }

    fn catch_clause(
        &mut self,
        clause: &'ast ast::statement::try_::CatchClause<Loc, Loc>,
    ) -> Result<(), !> {
        if let Some(param) = clause.param.as_ref() {
            self.record_pattern(LexicalAnnotKind::Var(ast::VariableKind::Var), param);
        }
        ast_visitor::catch_clause_default(self, clause)
    }

    fn statement_list(
        &mut self,
        stmts: &'ast [ast::statement::Statement<Loc, Loc>],
    ) -> Result<(), !> {
        self.record_declarations(stmts);
        ast_visitor::statement_list_default(self, stmts)
    }
}

pub(crate) fn bind_lexical_annotation_roots<'arena: 'ast, 'ast>(
    opts: &TypeSigOptions,
    module_scope: ScopeId,
    scopes: &mut scope::Scopes<'arena, 'ast>,
    tbls: &mut Tables<'arena, 'ast>,
    program: &'ast ast::Program<Loc, Loc>,
    missing: &BTreeSet<Loc>,
) -> BTreeMap<Loc, BindingNode<'arena, 'ast>> {
    let mut collector = LexicalAnnotCollector {
        targets: missing,
        roots: vec![],
        decls: vec![],
    };
    let Ok(()) = collector.program(program);
    let mut bound = BTreeMap::new();
    for root in collector.roots {
        let LexicalAnnotRoot {
            loc,
            name,
            annotation,
            kind,
        } = root;
        let id_loc_node = tbls.push_loc(loc.dupe());
        let id_loc_node_for_def = id_loc_node.dupe();
        let def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
            tbls.splice(id_loc_node_for_def.dupe(), |tbls| {
                annot(
                    opts,
                    module_scope,
                    scopes,
                    tbls,
                    &mut tparam_stack::TParamStack::new(),
                    &annotation.annotation,
                )
            })
        }));
        // A fresh scope per root: a lexical name may shadow, or be shadowed
        // by, an unrelated same-named binding elsewhere in the file.
        let lex = scope::push_lex(scopes, module_scope);
        let slot: Rc<RefCell<Option<LocalDefNode>>> = Rc::new(RefCell::new(None));
        match kind {
            LexicalAnnotKind::Param => {
                let slot = Rc::clone(&slot);
                scope::bind_param(
                    lex,
                    scopes,
                    tbls,
                    id_loc_node,
                    name.dupe(),
                    def,
                    TParams::Mono,
                    move |_, _, node| {
                        *slot.borrow_mut() = Some(node.dupe());
                    },
                );
            }
            LexicalAnnotKind::Var(kind) => {
                let slot = Rc::clone(&slot);
                scope::bind_var(
                    lex,
                    scopes,
                    tbls,
                    kind,
                    id_loc_node,
                    name.dupe(),
                    def,
                    move |_, _, node| {
                        *slot.borrow_mut() = Some(node.dupe());
                    },
                );
            }
        }
        if let Some(node) = slot.borrow().as_ref() {
            bound.insert(loc, BindingNode::LocalBinding(node.dupe()));
        }
    }
    for LexicalDeclRoot { loc, name, decl } in collector.decls {
        let lex = scope::push_lex(scopes, module_scope);
        let slot: RefCell<Option<LocalDefNode>> = RefCell::new(None);
        let record = |_: &mut scope::Scopes<'arena, 'ast>,
                      bound_name: &FlowSmolStr,
                      node: LocalDefNode<'arena, 'ast>| {
            if *bound_name == name {
                *slot.borrow_mut() = Some(node);
            }
        };
        match decl {
            LexicalDecl::DeclareFunctions(decls) => {
                for decl in decls {
                    declare_function_decl(lex, scopes, tbls, decl, &record);
                }
            }
            LexicalDecl::DeclareVariable(decl) => {
                declare_variable_decl(opts, lex, scopes, tbls, decl, &record);
            }
            LexicalDecl::Function(decl) => {
                function_decl(lex, scopes, tbls, decl, &record);
            }
            LexicalDecl::Class(decl) if decl.id.is_some() => {
                class_decl(opts, lex, scopes, tbls, decl, &record);
            }
            LexicalDecl::Class(decl) => {
                let id_loc_node = tbls.push_loc(loc.dupe());
                let id_loc_node_for_def = id_loc_node.dupe();
                let def = tbls.lazy(Box::new(move |opts, scopes, tbls| {
                    tbls.splice(id_loc_node_for_def.dupe(), |tbls| {
                        class_def(opts, lex, scopes, tbls, decl)
                    })
                }));
                scope::bind_class(
                    opts,
                    lex,
                    scopes,
                    tbls,
                    id_loc_node,
                    name.dupe(),
                    def,
                    record,
                );
            }
        }
        if let Some(node) = slot.into_inner() {
            bound.insert(loc, BindingNode::LocalBinding(node));
        }
    }
    bound
}
