/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![feature(never_type)]

use std::cell::LazyCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering as AtomicOrdering;

use dupe::Dupe;
use dupe::OptionDupedExt;
use flow_aloc::ALoc;
use flow_aloc::ALocFuzzy;
use flow_aloc::ALocFuzzyMap;
use flow_aloc::ALocId;
use flow_aloc::ALocMap;
use flow_aloc::ALocSet;
use flow_aloc::LazyALocTable;
use flow_analysis::property_assignment::Errors as PropertyAssignmentErrors;
use flow_common::docblock::Docblock;
use flow_common::docblock::FlowMode;
use flow_common::docblock::JsxRuntimePragma;
use flow_common::enclosing_context::EnclosingContext;
use flow_common::files;
use flow_common::files::FileOptions;
use flow_common::flow_import_specifier::FlowImportSpecifier;
use flow_common::flow_import_specifier::Userland;
use flow_common::flow_projects::ProjectsOptions;
use flow_common::options::AssertOperator;
use flow_common::options::JsxMode;
use flow_common::options::Options;
use flow_common::options::ReactRule;
use flow_common::options::ReactRuntime;
use flow_common::platform_set::PlatformSet;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReason;
use flow_common::reason::string_of_aloc;
use flow_common::refinement_invalidation::RefinementInvalidation;
use flow_common::slow_to_check_logging::SlowToCheckLogging;
use flow_common::subst_name::SubstName;
use flow_common::type_strictness::TypeStrictnessKind;
use flow_common::verbose::Verbose;
use flow_data_structure_wrapper::int_map::IntHashMap;
use flow_data_structure_wrapper::int_map::IntHashSet;
use flow_data_structure_wrapper::multi_level_map::MultiLevelMap;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_data_structure_wrapper::vector::FlowVector;
use flow_env_builder::env_api::CacheableEnvError;
use flow_env_builder::name_def_types::ScopeKind;
use flow_lazy::ContextFamily;
use flow_lint_settings::lint_severity_cover::LintSeverityCover;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc;
use flow_type_sig::compact_table;
use flow_type_sig::compact_table::Table;
use flow_typing_builtins::Builtins;
use flow_typing_builtins::LazyModuleType;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_suppressions::ErrorSuppressions;
use flow_typing_errors::flow_error::ErrorSet;
use flow_typing_errors::flow_error::FlowError;
use flow_typing_errors::flow_error::error_of_msg;
use flow_typing_errors::intermediate_error_types::ExpressionReferenceData;
use flow_typing_exists_check::ExistsCheck;
use flow_typing_generics::GenericId;
use flow_typing_loc_env::loc_env::LocEnv;
use flow_typing_loc_env::node_cache::NodeCache;
use flow_typing_spread_cache::SpreadCache;
use flow_typing_type::type_;
use flow_typing_type::type_::ConstFoldMap;
use flow_typing_type::type_::DroType;
use flow_typing_type::type_::EvalIdCacheMap;
use flow_typing_type::type_::EvalReposCacheMap;
use flow_typing_type::type_::FixCacheMap;
use flow_typing_type::type_::IdCacheMap;
use flow_typing_type::type_::IndexerFallbackData;
use flow_typing_type::type_::MergedDeclarationConflict;
use flow_typing_type::type_::MergedDeclarationData;
use flow_typing_type::type_::ModuleType;
use flow_typing_type::type_::PropRef;
use flow_typing_type::type_::RootUseOp;
use flow_typing_type::type_::ThisSpecializeCacheMap;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeContext;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeTKind;
use flow_typing_type::type_::UnaryArithKind;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::aconstraint::AConstraint;
use flow_typing_type::type_::aconstraint::AnnotationInferenceOperation;
use flow_typing_type::type_::aconstraint::Op;
use flow_typing_type::type_::aconstraint::OpInner;
use flow_typing_type::type_::any_t;
use flow_typing_type::type_::arith_kind::ArithKind;
use flow_typing_type::type_::constraint::forcing_state::ForcingState;
use flow_typing_type::type_::object::ResolveTool;
use flow_typing_type::type_::object::Tool;
use flow_typing_type::type_::type_or_type_desc::ErrorTypeDesc;
use flow_typing_type::type_::type_or_type_desc::TypeOrTypeDescT;
use flow_utils_concurrency::check_budget::CheckBudget;
use flow_utils_concurrency::job_error::JobError;
use flow_utils_union_find::Node;
use flow_utils_union_find::Root;
use flow_utils_union_find::TvarNotFound;
use regex::Regex;
use vec1::Vec1;

#[derive(Debug, Clone, Dupe)]
pub struct PropsNotFound(pub type_::properties::Id);

#[derive(Debug, Clone, Copy, Dupe)]
pub struct CallNotFound(pub i32);

#[derive(Debug, Clone, Dupe)]
pub struct ExportsNotFound(pub type_::exports::Id);

#[derive(Debug, Clone, Dupe)]
pub struct RequireNotFound(pub FlowImportSpecifier);

#[derive(Debug, Clone, Dupe)]
pub struct ModuleNotFound(pub FlowSmolStr);

/// Fields that change per-file (via `docblock_overrides` or by setting `checked = false`).
/// Small struct, cheap to clone directly.
#[derive(Debug, Clone)]
pub struct OverridableMetadata {
    pub available_platforms: Option<PlatformSet>,
    pub checked: bool,
    pub has_explicit_supports_platform: bool,
    pub is_ts_global_augmentation: bool,
    pub jsx: JsxMode,
    pub munge_underscores: bool,
    pub react_runtime: ReactRuntime,
    pub strict: bool,
    pub strict_local: bool,
}

impl Default for OverridableMetadata {
    fn default() -> Self {
        Self {
            available_platforms: None,
            checked: true,
            has_explicit_supports_platform: false,
            is_ts_global_augmentation: false,
            jsx: JsxMode::JsxReact,
            munge_underscores: false,
            react_runtime: ReactRuntime::Classic,
            strict: false,
            strict_local: false,
        }
    }
}

/// Fields set once from `Options`, never modified per-file. Stored in `Rc<FrozenMetadata>` for
/// cheap cloning.
#[derive(Clone, Debug)]
pub struct FrozenMetadata {
    pub include_suppressions: bool,
    pub slow_to_check_logging: SlowToCheckLogging,
    pub verbose: Option<Arc<Verbose>>,
    pub assert_operator: AssertOperator,
    pub automatic_require_default: bool,
    pub babel_loose_array_spread: bool,
    pub ban_spread_key_props: bool,
    pub component_syntax: bool,
    pub async_component_syntax: bool,
    pub async_component_syntax_includes: Arc<[Regex]>,
    pub dev_only_refinement_info_as_errors: bool,
    pub enable_const_params: bool,
    pub enable_custom_error: bool,
    pub enable_enums: bool,
    pub enable_jest_integration: bool,
    pub enable_pattern_matching: bool,
    pub enable_records: bool,
    pub enable_relay_integration: bool,
    pub export_as_namespace_support: bool,
    pub facebook_fbs: Option<FlowSmolStr>,
    pub facebook_fbt: Option<FlowSmolStr>,
    pub facebook_module_interop: bool,
    pub file_options: Arc<FileOptions>,
    pub global_libdefs: Arc<BTreeSet<FileKey>>,
    pub haste_disallow_dynamic_import: bool,
    pub hook_compatibility: bool,
    pub hook_compatibility_excludes: Arc<[Regex]>,
    pub hook_compatibility_includes: Arc<[Regex]>,
    pub ignore_non_literal_requires: bool,
    pub records_includes: Arc<[Regex]>,
    pub max_workers: i32,
    pub missing_module_generators: Arc<[(Regex, String)]>,
    pub no_implicit_override: bool,
    pub no_unchecked_indexed_access: bool,
    pub normalize_intersections: bool,
    pub projects_options: Arc<ProjectsOptions>,
    pub react_custom_jsx_typing: bool,
    pub react_managed_attributes: bool,
    pub react_rules: Arc<[ReactRule]>,
    pub recursion_limit: i32,
    pub relay_integration_esmodules: bool,
    pub relay_integration_excludes: Arc<[Regex]>,
    pub relay_integration_module_prefix: Option<FlowSmolStr>,
    pub relay_integration_module_prefix_includes: Arc<[Regex]>,
    pub root: Arc<PathBuf>,
    pub strict_es6_import_export: bool,
    pub strict_property_initialization: bool,
    pub strip_root: bool,
    pub stylex_shorthand_prop: Option<String>,
    pub export_star_excludes_default: bool,
    pub ts_syntax: bool,
    pub ts_utility_syntax: bool,
    pub tslib_syntax: bool,
    pub typescript_library_definition_support: bool,
    pub type_expansion_recursion_limit: i32,
    pub use_unknown_in_catch_variables: bool,
}

impl Default for FrozenMetadata {
    fn default() -> Self {
        Self {
            include_suppressions: false,
            slow_to_check_logging: SlowToCheckLogging::default(),
            verbose: None,
            assert_operator: AssertOperator::Disabled,
            automatic_require_default: false,
            babel_loose_array_spread: false,
            ban_spread_key_props: false,
            component_syntax: true,
            async_component_syntax: false,
            async_component_syntax_includes: Arc::from([]),
            dev_only_refinement_info_as_errors: false,
            enable_const_params: false,
            enable_custom_error: false,
            enable_enums: true,
            enable_jest_integration: false,
            enable_pattern_matching: false,
            enable_records: false,
            enable_relay_integration: false,
            export_as_namespace_support: false,
            facebook_fbs: None,
            facebook_fbt: None,
            facebook_module_interop: false,
            file_options: Arc::new(FileOptions::default()),
            global_libdefs: Arc::new(BTreeSet::new()),
            haste_disallow_dynamic_import: false,
            hook_compatibility: false,
            hook_compatibility_excludes: Arc::from([]),
            hook_compatibility_includes: Arc::from([]),
            ignore_non_literal_requires: false,
            records_includes: Arc::from([]),
            max_workers: 0,
            missing_module_generators: Arc::from([]),
            no_implicit_override: false,
            no_unchecked_indexed_access: false,
            normalize_intersections: false,
            projects_options: Arc::new(ProjectsOptions::default()),
            react_custom_jsx_typing: false,
            react_managed_attributes: false,
            react_rules: Arc::from([]),
            recursion_limit: 10000,
            relay_integration_esmodules: false,
            relay_integration_excludes: Arc::from([]),
            relay_integration_module_prefix: None,
            relay_integration_module_prefix_includes: Arc::from([]),
            root: Arc::new(PathBuf::new()),
            strict_es6_import_export: false,
            strict_property_initialization: false,
            strip_root: true,
            stylex_shorthand_prop: None,
            export_star_excludes_default: false,
            ts_syntax: false,
            ts_utility_syntax: false,
            tslib_syntax: false,
            typescript_library_definition_support: false,
            type_expansion_recursion_limit: 3,
            use_unknown_in_catch_variables: false,
        }
    }
}

/// Metadata for a file. Split into frozen (shared via `Rc`) and overridable parts for cheap
/// cloning.
#[derive(Debug, Clone)]
pub struct Metadata {
    pub frozen: Rc<FrozenMetadata>,
    pub overridable: OverridableMetadata,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            frozen: Rc::new(FrozenMetadata::default()),
            overridable: OverridableMetadata::default(),
        }
    }
}

#[derive(Debug, Clone)]
enum TestPropHitOrMiss {
    Hit,
    Miss {
        name: Option<Name>,
        prop_reason_and_object: (Reason, Type),
        use_op: UseOp,
        suggestion: Option<FlowSmolStr>,
    },
}

#[derive(Debug, Clone)]
pub struct VoidableCheck {
    pub public_property_map: type_::properties::Id,
    pub private_property_map: type_::properties::Id,
    pub errors: PropertyAssignmentErrors<ALoc>,
}

#[derive(Debug, Clone, Dupe)]
pub enum SubstCacheErr {
    ETooFewTypeArgs(ALoc, i32),
    ETooManyTypeArgs(ALoc, i32),
}

#[derive(Debug, Clone)]
pub struct PossiblyRefinedWriteState {
    pub t: Type,
    pub errors: Vec<CacheableEnvError<ALoc>>,
    pub actually_refined_refining_locs: Option<FlowOrdSet<ALoc>>,
}

#[derive(Debug, Clone)]
pub struct BuiltinsGroup {
    pub builtin_locs: Arc<compact_table::Table<Loc>>,
    pub builtins: Arc<flow_type_sig::packed_type_sig::Builtins<Loc>>,
}

#[derive(Debug)]
pub enum MasterContext {
    EmptyMasterContext,
    NonEmptyMasterContext {
        builtin_leader_file_key: FileKey,
        all_unordered_libs: Arc<BTreeSet<FlowSmolStr>>,
        global_libdefs: Arc<BTreeSet<FileKey>>,
        builtins: BuiltinsGroup,
    },
}

impl MasterContext {
    pub fn global_libdefs(&self) -> Arc<BTreeSet<FileKey>> {
        match self {
            Self::EmptyMasterContext => Arc::new(BTreeSet::new()),
            Self::NonEmptyMasterContext { global_libdefs, .. } => global_libdefs.dupe(),
        }
    }

    pub fn is_global_libdef(&self, file: &FileKey) -> bool {
        file.is_lib_file()
            || match self {
                Self::EmptyMasterContext => false,
                Self::NonEmptyMasterContext { global_libdefs, .. } => global_libdefs.contains(file),
            }
    }
}

pub struct ComponentT {
    sig_cx: TypeContext<Cx>,
    // mapping from keyed alocs to concrete locations
    aloc_tables: HashMap<FileKey, LazyALocTable>,
    synthesis_produced_uncacheable_result: bool,
    errors: ErrorSet,
    error_suppressions: ErrorSuppressions,
    severity_cover: FlowOrdMap<FileKey, LintSeverityCover>,

    // map from exists proposition locations to the types of values running through them
    exists_checks: ALocMap<BTreeSet<Type>>,
    // map from exists proposition locations to the types of excuses for them
    // If a variable appears in something like `x || ''`, the existence check
    // is excused and not considered sketchy. (The program behaves identically to how it would
    // if the null check was made explicit (`x == null ? '' : x`), and this is a fairly
    // common pattern. Excusing it eliminates a lot of noise from the lint rule.)
    // The above example assumes that x is a string. If it were a different type
    // it wouldn't be excused.
    exists_excuses: ALocMap<ExistsCheck>,
    // For the definite instance property assignment analysis, we should only
    // emit errors for a given property if VoidT flows to the type of that
    // property. Ideally, we would create a VoidT ~> property type flow when we
    // perform the analysis. The problem is that doing that causes the type
    // inference behavior to depend on lint settings which can lead to some weird
    // behavior, such as extra errors even when the lint is off. The solution is
    // to collect all of potential errors that we would have created a flow for
    // in the context and deal with them post-merge. At this point, the tvars of
    // nearly all properties will have a concrete type that we can safely pattern
    // match on without affecting other constraints. For the unresolved tvars, we
    // conservatively emit errors.
    voidable_checks: Vec<VoidableCheck>,
    test_prop_hits_and_misses: IntHashMap<i32, TestPropHitOrMiss>,
    optional_chains_useful: ALocMap<(ExpressionReferenceData<ALoc>, bool)>,
    conditions: FlowVector<flow_parser::ast::expression::Expression<ALoc, (ALoc, Type)>>,
    strict_comparisons: FlowVector<(
        ALoc,
        (
            flow_parser::ast::expression::Expression<ALoc, (ALoc, Type)>,
            flow_parser::ast::expression::Expression<ALoc, (ALoc, Type)>,
        ),
    )>,
    maybe_unused_promises: FlowVector<(ALoc, Type, bool)>,

    inference_nodes: IntHashMap<i32, Node<type_::constraint::Constraints<Cx>>>,

    constraint_cache: type_::FlowSet<Cx>,
    subst_cache: type_::SubstCacheMap<(Vec<SubstCacheErr>, Type)>,
    eval_id_cache: EvalIdCacheMap,
    id_cache: IdCacheMap,
    eval_repos_cache: EvalReposCacheMap,
    fix_cache: FixCacheMap,
    this_specialize_cache: ThisSpecializeCacheMap,
    spread_cache: SpreadCache,
    const_fold_cache: ConstFoldMap,

    // Post-inference checks
    delayed_forcing_tvars: FlowOrdSet<i32>,
    post_component_tvar_forcing_states: FlowVector<ForcingState<Cx>>,
    active_tvar_forcing_states: Vec<ForcingState<Cx>>,
    /// Root ids of tvars whose every reachable tvar is resolved or already forced, so forcing
    /// them again is a no-op.
    fully_forced_tvars: IntHashSet<i32>,
    post_inference_polarity_checks: Vec<(
        BTreeMap<SubstName, type_::TypeParam>,
        flow_common::polarity::Polarity,
        Type,
    )>,
    post_inference_validation_flows: VecDeque<(Type, type_::UseT<Cx>)>,
    /// Arbitrary deferred validation thunks. Run after main inference (when
    /// Flow_js is fully available), so a thunk can concretize types and emit
    /// context-specific errors that don't fit a simple subtyping flow. The
    /// thunk is run with the [Context] being checked.
    post_inference_validation_callbacks: Vec<PostInferenceValidationCallback>,
    /// Supports interface declaration merging. When two or more
    /// `interface Foo { ... }` declarations share a name in one scope,
    /// TS-style merging folds them into a single Foo. A consequence is that
    /// any field name appearing in more than one declaration must have
    /// agreeing types across declarations — otherwise the merged interface
    /// would be contradictory. This table is what lets us check that
    /// agreement: for each merging interface (keyed by its name_loc), we
    /// remember the field types we saw in source. After inference is done,
    /// the post-inference pass walks these and unifies any field types that
    /// appear on both sides of a merge. Interfaces not involved in any merge
    /// are absent from this table — their loc never gets a slot, so
    /// recording is a no-op for them.
    merging_interface_field_types: HashMap<ALoc, HashMap<Name, Type>>,
    /// Declaration merging compares and copies member types across
    /// declaration boundaries. Type parameter names are local to each
    /// declaration, so an absorbed declaration's member types need a
    /// positional renaming into the canonical declaration's type parameter
    /// namespace before we replay those types during checking. This table
    /// stores each merging declaration's declared type parameters, paired with
    /// the `GenericT` created for that parameter, in source order.
    merging_interface_typeparams: HashMap<ALoc, Vec<(type_::TypeParam, Type)>>,
    /// Also supports interface declaration merging, but for a different step:
    /// actually combining the property maps. Each interface InstanceT stores
    /// its own/proto properties in two heap-allocated maps identified by
    /// `Properties::Id`. To fold N decls of `Foo` into one usable type, the
    /// maps of the later decls must be merged into the maps of the first.
    /// This table records the (own_props, proto_props) id pair of every
    /// interface decl in the file so the merger (env_resolution) can look up
    /// the right maps for each side of a merge. Populated for all interfaces,
    /// not only the merging ones, since at registration time we don't gate on
    /// participation.
    interface_prop_ids: BTreeMap<ALoc, (type_::properties::Id, type_::properties::Id)>,
    env_value_cache: IntHashMap<i32, PossiblyRefinedWriteState>,
    env_type_cache: IntHashMap<i32, PossiblyRefinedWriteState>,
    // map from annot tvar ids to nodes used during annotation processing
    annot_graph: IntHashMap<i32, AConstraint<Cx>>,
    // Shares one lazy indirection per `elab_open` demand so that repeated
    // demands tie the knot instead of expanding forever.
    elab_open_memo: MultiLevelMap<ElabOpenMemoKey, Type>,
    // Used to power an autofix that takes the lower bounds of types where we emit missing-local-annot
    // and turn them into annotations. This has to exist outside of the tvar graph because we will
    // eventually not use unresolved tvars to represent unannotated parameters. We use an ALocFuzzyMap
    // because we may compare keyed and concrete locations
    missing_local_annot_lower_bounds: ALocFuzzyMap<Vec<Type>>,
    // Used to power an autofix that takes the upper bounds of array & object literal declarations,
    // and turn them into annotations.
    array_or_object_literal_declaration_upper_bounds: IntHashMap<i32, (ALoc, Vec<Type>)>,
    array_or_object_literal_declaration_reposition_tracking: IntHashMap<i32, i32>,
    // Used to power a code action to automatically insert appropriate render type.
    // It is key-ed by the body_loc of components.
    inferred_component_return: ALocFuzzyMap<Vec1<Type>>,
    exhaustive_checks: ALocMap<(Vec<ALoc>, bool)>,
    // Signature help
    signature_help_callee: ALocMap<Type>,
    // Hover type
    ctor_callee: ALocMap<Type>,
    // Union optimization checks
    union_opt: ALocMap<Type>,
    // Natural inference (records checks on primitive literal types during
    // implicit instantiation)
    primitive_literal_checks: ALocSet,
    // `call`/`apply`/`bind` accesses that resolved on `Function.prototype`
    // rather than on a property of the receiver. Keyed by the location of the
    // property identifier.
    fun_proto_method_lookups: ALocSet,
    enclosing_context_for_call: ALocMap<EnclosingContext>,
}

#[derive(Debug, Clone, Dupe)]
pub enum TypingMode {
    CheckingMode,
    SynthesisMode { target_loc: Option<ALoc> },
    HintEvaluationMode,
}

impl TypingMode {
    fn show_frame(&self) -> String {
        match self {
            TypingMode::CheckingMode => "CheckingMode".to_string(),
            TypingMode::SynthesisMode {
                target_loc: Some(loc),
            } => format!("SynthesisMode (target={})", string_of_aloc(None, loc)),
            TypingMode::SynthesisMode { target_loc: None } => {
                "SynthesisMode (no target)".to_string()
            }
            TypingMode::HintEvaluationMode => "HintEvaluationMode".to_string(),
        }
    }

    pub fn show(frames: &[Self]) -> String {
        frames
            .iter()
            .map(Self::show_frame)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The exports of a typed module. The closure runs in the context the module belongs to.
pub type ModuleTypeThunk = Rc<dyn for<'a> Fn(&mut Context<'a>) -> Result<ModuleType, Type>>;

#[derive(Clone, Dupe)]
pub enum ResolvedRequire {
    TypedModule(ModuleTypeThunk),
    UncheckedModule(ALoc),
    MissingModule,
    /// The specifier resolved to a global libdef (a `[libs]` file), which is not
    /// a module. Carries the resolved libdef file so the import site can report a
    /// dedicated "cannot import a global libdef" error instead of the misleading
    /// cannot-resolve-module.
    GlobalLibdefModule(FileKey),
}

struct ContextInner {
    ccx: ComponentId,
    file: FileKey,
    all_unordered_libs: Arc<BTreeSet<FlowSmolStr>>,
    aloc_table: LazyALocTable,
    metadata: Metadata,
    budget: CheckBudget,
    resolve_require: ResolveRequire,
    builtins: Rc<flow_lazy::Lazy<Cx, Rc<Builtins<Cx>>>>,
    // The builtins read into this context so far.
    builtin_values: HashMap<FlowSmolStr, (ALoc, Type)>,
    builtin_types: HashMap<FlowSmolStr, (ALoc, Type)>,
    builtin_modules: HashMap<FlowSmolStr, (Reason, LazyModuleType<Cx>)>,
    builtin_umd_values: HashMap<FlowSmolStr, (ALoc, Type)>,
    hint_map_arglist_cache: HashMap<(ALoc, Option<ALoc>), Vec<(ALoc, type_::CallArg)>>,
    hint_map_jsx_cache: HashMap<(Reason, String, Vec<ALoc>, ALoc), LazyHintMapJsx>,
    hint_eval_cache: IntHashMap<i32, Option<Type>>,
    environment: LocEnv<Cx>,
    typing_mode: TypingMode,
    //  A subset of all transitive dependencies of the current file as determined by import/require.
    // This set will only be populated with type sig files that are actually forced.
    reachable_deps: BTreeSet<FileKey>,
    refined_locations: ALocMap<FlowOrdSet<ALoc>>,
    aggressively_invalidated_locations: ALocMap<RefinementInvalidation>,
    switch_to_match_eligible_locations: ALocSet,
    assertion_call_candidates: HashSet<ALoc>,
    node_cache: NodeCache<Cx>,
    ts_import_provenance: ALocMap<(FlowImportSpecifier, FlowSmolStr)>,
}

pub type LazyHintMapJsx = Rc<flow_lazy::Lazy<Cx, Result<Type, JobError>>>;

pub type ResolveRequire =
    Rc<dyn for<'a> Fn(&mut Context<'a>, &FlowImportSpecifier) -> ResolvedRequire>;

pub type MkBuiltins = Rc<dyn for<'a> Fn(&mut Context<'a>) -> Builtins<Cx>>;

pub type PostInferenceValidationCallback =
    Box<dyn for<'a> Fn(&mut Context<'a>) -> Result<(), JobError>>;

/// The family of [Context], for the generic type structures of the crates below this one.
#[derive(Debug, Clone, Copy, Dupe)]
pub enum Cx {}

impl ContextFamily for Cx {
    type Context<'cx> = Context<'cx>;
}

#[derive(Debug, Clone, Copy, Dupe, PartialEq, Eq, Hash)]
pub struct ArenaId(u64);

impl ArenaId {
    fn fresh() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        ArenaId(NEXT.fetch_add(1, AtomicOrdering::Relaxed))
    }
}

/// A context: the arena that owns it and its index there.
#[derive(Debug, Clone, Copy, Dupe, PartialEq, Eq, Hash)]
pub struct ContextId {
    arena: ArenaId,
    index: usize,
}

#[derive(Debug, Clone, Copy, Dupe, PartialEq, Eq, Hash)]
pub struct ComponentId {
    arena: ArenaId,
    index: usize,
}

#[derive(Debug, Clone, Copy, Dupe, PartialEq, Eq, Hash)]
pub struct SigFileId {
    arena: ArenaId,
    index: usize,
}

pub type LazyType = Rc<flow_lazy::Lazy<Cx, Type>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalDefBindingKind {
    DefBindingClassLikeMono,
    DefBindingClassLikePoly,
    DefBindingOther,
}

pub type LocalDefEntry = (ALoc, FlowSmolStr, LocalDefBindingKind, LazyType, LazyType);

pub type RemoteRefEntry = (ALoc, FlowSmolStr, Type, LazyType);

/// The tables of a packed type signature, each entry merged into types when it is first forced.
/// Entries find the file they belong to again through its [SigFileId].
pub struct SigFile {
    pub dependencies: Table<(Userland, Rc<flow_lazy::Lazy<Cx, ResolvedRequire>>)>,
    pub exports: ModuleTypeThunk,
    pub local_defs: Table<Rc<flow_lazy::Lazy<Cx, LocalDefEntry>>>,
    pub remote_refs: Table<Rc<flow_lazy::Lazy<Cx, RemoteRefEntry>>>,
    pub pattern_defs: Table<LazyType>,
    pub patterns: Table<LazyType>,
}

/// Owns contexts, the components they share, and the signature files they merge. Everything
/// that refers to one of these from elsewhere holds its id, which a [ContextStore] containing
/// this arena resolves.
pub struct ContextArena {
    id: ArenaId,
    contexts: Vec<ContextInner>,
    components: Vec<ComponentT>,
    sig_files: Vec<Rc<SigFile>>,
    /// The contexts made for dependency files, with the signature file each one merges.
    dependencies: HashMap<FileKey, (ContextId, SigFileId)>,
    /// The component shared by the dependency contexts of a cycle, by the cycle's leader.
    dependency_components: HashMap<FileKey, ComponentId>,
}

impl ContextArena {
    pub fn new() -> Self {
        ContextArena {
            id: ArenaId::fresh(),
            contexts: Vec::new(),
            components: Vec::new(),
            sig_files: Vec::new(),
            dependencies: HashMap::new(),
            dependency_components: HashMap::new(),
        }
    }

    pub fn id(&self) -> ArenaId {
        self.id
    }

    pub fn add_component(&mut self) -> ComponentId {
        self.components.push(make_ccx());
        ComponentId {
            arena: self.id,
            index: self.components.len() - 1,
        }
    }

    pub fn add_context(
        &mut self,
        ccx: ComponentId,
        metadata: Metadata,
        file: FileKey,
        all_unordered_libs: Arc<BTreeSet<FlowSmolStr>>,
        aloc_table: LazyALocTable,
        resolve_require: ResolveRequire,
        mk_builtins: MkBuiltins,
        budget: CheckBudget,
    ) -> ContextId {
        assert_eq!(
            ccx.arena, self.id,
            "a context shares a component of its own arena"
        );
        self.components
            .get_mut(ccx.index)
            .expect("a component id names a component of its arena")
            .aloc_tables
            .insert(file.dupe(), aloc_table.dupe());
        self.contexts.push(ContextInner {
            ccx,
            file,
            all_unordered_libs,
            aloc_table,
            metadata,
            budget,
            resolve_require,
            builtins: Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                Rc::new(mk_builtins(cx))
            })),
            builtin_values: HashMap::new(),
            builtin_types: HashMap::new(),
            builtin_modules: HashMap::new(),
            builtin_umd_values: HashMap::new(),
            hint_map_arglist_cache: HashMap::new(),
            hint_map_jsx_cache: HashMap::new(),
            hint_eval_cache: IntHashMap::default(),
            environment: LocEnv::empty(ScopeKind::Global),
            typing_mode: TypingMode::CheckingMode,
            reachable_deps: BTreeSet::new(),
            node_cache: NodeCache::mk_empty(),
            refined_locations: ALocMap::new(),
            aggressively_invalidated_locations: ALocMap::new(),
            switch_to_match_eligible_locations: ALocSet::new(),
            assertion_call_candidates: HashSet::new(),
            ts_import_provenance: ALocMap::new(),
        });
        ContextId {
            arena: self.id,
            index: self.contexts.len() - 1,
        }
    }

    /// Adds the signature file `mk_sig_file` builds. Its thunks may name the file by the id they
    /// are given.
    pub fn add_sig_file(&mut self, mk_sig_file: impl FnOnce(SigFileId) -> SigFile) -> SigFileId {
        let id = SigFileId {
            arena: self.id,
            index: self.sig_files.len(),
        };
        self.sig_files.push(Rc::new(mk_sig_file(id)));
        id
    }

    pub fn dependency(&self, file: &FileKey) -> Option<(ContextId, SigFileId)> {
        self.dependencies.get(file).copied()
    }

    pub fn add_dependency(&mut self, file: FileKey, dependency: (ContextId, SigFileId)) {
        self.dependencies.insert(file, dependency);
    }

    pub fn dependency_count(&self) -> usize {
        self.dependencies.len()
    }

    /// The component shared by the dependency contexts of the cycle `leader` leads.
    pub fn dependency_component(&mut self, leader: FileKey) -> ComponentId {
        if let Some(ccx) = self.dependency_components.get(&leader) {
            return *ccx;
        }
        let ccx = self.add_component();
        self.dependency_components.insert(leader, ccx);
        ccx
    }

    /// Drops everything this arena owns. Ids it handed out resolve to nothing afterwards.
    pub fn clear(&mut self) {
        *self = ContextArena::new();
    }
}

impl Default for ContextArena {
    fn default() -> Self {
        Self::new()
    }
}

/// The arenas a [Context] works on, borrowed exclusively for as long as it works on them.
pub struct ContextStore<'a> {
    arenas: Vec<&'a mut ContextArena>,
}

impl<'a> ContextStore<'a> {
    pub fn new(arenas: Vec<&'a mut ContextArena>) -> Self {
        ContextStore { arenas }
    }

    fn position(&self, id: ArenaId) -> Option<usize> {
        self.arenas.iter().position(|arena| arena.id == id)
    }

    pub fn arena(&self, id: ArenaId) -> Option<&ContextArena> {
        Some(&*self.arenas[self.position(id)?])
    }

    pub fn arena_mut(&mut self, id: ArenaId) -> Option<&mut ContextArena> {
        let position = self.position(id)?;
        Some(&mut *self.arenas[position])
    }

    pub fn has_context(&self, id: ContextId) -> bool {
        self.arena(id.arena)
            .is_some_and(|arena| id.index < arena.contexts.len())
    }

    pub fn sig_file(&self, id: SigFileId) -> Rc<SigFile> {
        self.arena(id.arena)
            .and_then(|arena| arena.sig_files.get(id.index))
            .unwrap_or_else(|| panic!("{id:?} is not a signature file of this store"))
            .dupe()
    }
}

/// The context a check is working on, with exclusive access to the arenas of the other contexts
/// it can reach. Code that needs another context switches to it with [Context::switch_to].
pub struct Context<'a> {
    store: ContextStore<'a>,
    id: ContextId,
    /// The position in `store` of the arena that owns the current context.
    position: usize,
    /// The context annotation inference reports errors on: the file being checked. Switching to
    /// a dependency keeps it, so errors raised while forcing a dependency's signature reach the
    /// file whose check forced it, however long the dependency context is cached.
    annot_error_dst: Option<ContextId>,
}

/// A [Context] switched to another context until this is dropped.
pub struct ContextSwitch<'s, 'a> {
    cx: &'s mut Context<'a>,
    previous: (ContextId, usize),
}

impl<'a> std::ops::Deref for ContextSwitch<'_, 'a> {
    type Target = Context<'a>;

    fn deref(&self) -> &Context<'a> {
        self.cx
    }
}

impl<'a> std::ops::DerefMut for ContextSwitch<'_, 'a> {
    fn deref_mut(&mut self) -> &mut Context<'a> {
        self.cx
    }
}

impl Drop for ContextSwitch<'_, '_> {
    fn drop(&mut self) {
        (self.cx.id, self.cx.position) = self.previous;
    }
}

pub fn mk_context_metadata(options: &Options, global_libdefs: Arc<BTreeSet<FileKey>>) -> Metadata {
    Metadata {
        overridable: OverridableMetadata {
            checked: options.all,
            jsx: JsxMode::JsxReact,
            munge_underscores: options.munge_underscores,
            strict: false,
            strict_local: false,
            available_platforms: None,
            has_explicit_supports_platform: false,
            is_ts_global_augmentation: false,
            react_runtime: options.react_runtime,
        },
        frozen: Rc::new(FrozenMetadata {
            include_suppressions: options.include_suppressions,
            verbose: options.verbose.dupe(),
            slow_to_check_logging: options.slow_to_check_logging,

            automatic_require_default: options.automatic_require_default,
            babel_loose_array_spread: options.babel_loose_array_spread,
            component_syntax: options.component_syntax,
            async_component_syntax: options.async_component_syntax,
            async_component_syntax_includes: options.async_component_syntax_includes.dupe(),
            hook_compatibility_excludes: options.hook_compatibility_excludes.dupe(),
            hook_compatibility_includes: options.hook_compatibility_includes.dupe(),
            hook_compatibility: options.hook_compatibility,
            records_includes: options
                .records_includes
                .iter()
                .filter_map(|s| Regex::new(s).ok())
                .collect::<Vec<_>>()
                .into(),
            dev_only_refinement_info_as_errors: options.dev_only_refinement_info_as_errors,
            enable_const_params: options.enable_const_params,
            enable_custom_error: options.enable_custom_error,
            enable_enums: options.enums,
            enable_jest_integration: options.enable_jest_integration,
            enable_pattern_matching: options.enable_pattern_matching,
            enable_records: options.enable_records,
            enable_relay_integration: options.enable_relay_integration,
            export_as_namespace_support: options.export_as_namespace_support,
            facebook_fbs: options.facebook_fbs.dupe(),
            facebook_fbt: options.facebook_fbt.dupe(),
            facebook_module_interop: options.facebook_module_interop,
            file_options: options.file_options.dupe(),
            global_libdefs,
            haste_disallow_dynamic_import: options.haste_disallow_dynamic_import,
            ignore_non_literal_requires: options.ignore_non_literal_requires,
            max_workers: options.max_workers,
            missing_module_generators: options.missing_module_generators.dupe(),
            no_implicit_override: options.no_implicit_override,
            no_unchecked_indexed_access: options.no_unchecked_indexed_access,
            normalize_intersections: options.normalize_intersections,
            projects_options: options.projects_options.dupe(),
            react_custom_jsx_typing: options.react_custom_jsx_typing,
            react_managed_attributes: options.react_managed_attributes,
            react_rules: options.react_rules.dupe(),
            recursion_limit: options.recursion_limit,
            relay_integration_esmodules: options.relay_integration_esmodules,
            relay_integration_excludes: options.relay_integration_excludes.dupe(),
            relay_integration_module_prefix: options.relay_integration_module_prefix.dupe(),
            relay_integration_module_prefix_includes: options
                .relay_integration_module_prefix_includes
                .dupe(),
            root: options.root.dupe(),
            strict_es6_import_export: options.strict_es6_import_export,
            strict_property_initialization: options.strict_property_initialization,
            strip_root: options.strip_root,
            stylex_shorthand_prop: options.stylex_shorthand_prop.clone(),
            export_star_excludes_default: options.export_star_excludes_default,
            ts_syntax: options.ts_syntax,
            ts_utility_syntax: options.ts_utility_syntax,
            tslib_syntax: options.tslib_syntax,
            typescript_library_definition_support: options.typescript_library_definition_support,
            assert_operator: options.assert_operator,
            type_expansion_recursion_limit: options.type_expansion_recursion_limit,
            use_unknown_in_catch_variables: options.use_unknown_in_catch_variables,
            ban_spread_key_props: options.ban_spread_key_props,
        }),
    }
}

pub fn docblock_overrides(
    docblock_info: &Docblock,
    file_key: &FileKey,
    mut metadata: Metadata,
) -> Metadata {
    let jsx = match docblock_info.jsx() {
        Some(jsx_pragma) => {
            let jsx_expr = flow_aloc::loc_to_aloc_expression_owned(jsx_pragma.expression.dupe());
            JsxMode::JsxPragma(jsx_pragma.raw.clone(), jsx_expr)
        }
        None => JsxMode::JsxReact,
    };
    metadata.overridable.jsx = jsx;

    match docblock_info.jsx_runtime() {
        Some(JsxRuntimePragma::Classic) => {
            metadata.overridable.react_runtime = ReactRuntime::Classic
        }
        Some(JsxRuntimePragma::Automatic) => {
            metadata.overridable.react_runtime = ReactRuntime::Automatic
        }
        None => {}
    }

    match docblock_info.flow() {
        None => {}
        Some(FlowMode::OptIn) => {
            metadata.overridable.checked = true;
        }
        Some(FlowMode::OptInStrict) => {
            metadata.overridable.checked = true;
            metadata.overridable.strict = true;
        }
        Some(FlowMode::OptInStrictLocal) => {
            metadata.overridable.checked = true;
            metadata.overridable.strict_local = true;
        }
        // --all (which sets metadata.checked = true) overrides @noflow, so there are
        // currently no scenarios where we'd change checked = true to false. in the
        // future, there may be a case where checked defaults to true (but is not
        // forced to be true ala --all), but for now we do *not* want to force
        // checked = false here.
        Some(FlowMode::OptOut) => {}
    }

    if docblock_info.prevent_munge() {
        metadata.overridable.munge_underscores = false;
    }

    let explicit_available_platforms = docblock_info.supports_platform();
    let available_platforms = flow_common::platform_set::available_platforms(
        &metadata.frozen.file_options,
        &metadata.frozen.projects_options,
        file_key.as_str(),
        explicit_available_platforms,
    );
    metadata.overridable.available_platforms = available_platforms;
    metadata.overridable.has_explicit_supports_platform = explicit_available_platforms.is_some();

    if flow_common::files::has_ts_ext(file_key)
        || flow_common::files::is_builtin_ts_module(
            &metadata.frozen.file_options,
            &file_key.to_absolute(),
        )
    {
        // .ts files are always checked, consistent with `types_checked` in `parsing_service_js`
        metadata.overridable.checked = true;
        let frozen = Rc::make_mut(&mut metadata.frozen);
        frozen.ts_syntax = true;
        frozen.tslib_syntax = true;
        frozen.ts_utility_syntax = true;
        // TS `enum`s are a distinct feature from Flow Enums and are always available
        // in .ts files, regardless of the `enums` option (which gates Flow Enums).
        frozen.enable_enums = true;
    }

    metadata
}

fn empty_sig_cx() -> TypeContext<Cx> {
    thread_local! {
        static CACHED_EVALUATED: type_::eval::Map<Type> = Default::default();
    }
    TypeContext {
        graph: flow_utils_union_find::Graph::with_capacity(16),
        property_maps: BTreeMap::new(),
        call_props: IntHashMap::with_capacity_and_hasher(128, Default::default()),
        export_maps: HashMap::with_capacity(4),
        evaluated: CACHED_EVALUATED.with(|c| c.clone()),
    }
}

fn make_ccx() -> ComponentT {
    thread_local! {
        static CACHED_ORD_SET: FlowOrdSet<i32> = FlowOrdSet::new();
        static CACHED_SEVERITY_COVER: FlowOrdMap<FileKey, LintSeverityCover> = FlowOrdMap::new();
    }
    ComponentT {
        sig_cx: empty_sig_cx(),
        aloc_tables: HashMap::new(),
        synthesis_produced_uncacheable_result: false,
        delayed_forcing_tvars: CACHED_ORD_SET.with(|c| c.clone()),
        post_component_tvar_forcing_states: FlowVector::new(),
        active_tvar_forcing_states: Vec::new(),
        fully_forced_tvars: IntHashSet::default(),
        post_inference_polarity_checks: Vec::new(),
        post_inference_validation_flows: VecDeque::new(),
        post_inference_validation_callbacks: Vec::new(),
        merging_interface_field_types: HashMap::new(),
        merging_interface_typeparams: HashMap::new(),
        interface_prop_ids: BTreeMap::new(),
        env_value_cache: IntHashMap::default(),
        env_type_cache: IntHashMap::default(),
        missing_local_annot_lower_bounds: BTreeMap::new(),
        array_or_object_literal_declaration_upper_bounds: IntHashMap::default(),
        array_or_object_literal_declaration_reposition_tracking: IntHashMap::default(),
        inferred_component_return: BTreeMap::new(),
        errors: ErrorSet::empty(),
        error_suppressions: ErrorSuppressions::empty(),
        severity_cover: CACHED_SEVERITY_COVER.with(|c| c.clone()),
        exists_checks: ALocMap::new(),
        exists_excuses: ALocMap::new(),
        voidable_checks: Vec::new(),
        test_prop_hits_and_misses: IntHashMap::default(),
        optional_chains_useful: ALocMap::new(),
        conditions: FlowVector::new(),
        strict_comparisons: FlowVector::new(),
        maybe_unused_promises: FlowVector::new(),
        inference_nodes: IntHashMap::default(),
        constraint_cache: type_::FlowSet::default(),
        subst_cache: Default::default(),
        eval_id_cache: Default::default(),
        id_cache: Default::default(),
        eval_repos_cache: Default::default(),
        fix_cache: Default::default(),
        this_specialize_cache: Default::default(),
        spread_cache: SpreadCache::new(),
        const_fold_cache: Default::default(),
        annot_graph: IntHashMap::default(),
        elab_open_memo: MultiLevelMap::new(),
        exhaustive_checks: ALocMap::new(),
        signature_help_callee: ALocMap::new(),
        ctor_callee: ALocMap::new(),
        union_opt: ALocMap::new(),
        primitive_literal_checks: ALocSet::new(),
        fun_proto_method_lookups: ALocSet::new(),
        enclosing_context_for_call: ALocMap::new(),
    }
}

impl<'a> Context<'a> {
    /// The context `id` of `store`.
    pub fn new(store: ContextStore<'a>, id: ContextId) -> Self {
        assert!(
            store.has_context(id),
            "{id:?} is not a context of this store"
        );
        let position = store
            .position(id.arena)
            .expect("the store has the arena of its context");
        Context {
            store,
            id,
            position,
            annot_error_dst: None,
        }
    }

    /// Gives the arenas back.
    pub fn into_store(self) -> ContextStore<'a> {
        self.store
    }

    pub fn id(&self) -> ContextId {
        self.id
    }

    pub fn store(&self) -> &ContextStore<'a> {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut ContextStore<'a> {
        &mut self.store
    }

    /// Whether `id` is a context this one can switch to. A context whose arena is gone is not.
    pub fn has_context(&self, id: ContextId) -> bool {
        self.store.has_context(id)
    }

    /// Works on the context `id` until the returned switch is dropped.
    pub fn switch_to(&mut self, id: ContextId) -> ContextSwitch<'_, 'a> {
        assert!(
            self.store.has_context(id),
            "{id:?} is not a context of this store"
        );
        let position = self
            .store
            .position(id.arena)
            .expect("the store has the arena of the context");
        let previous = (
            std::mem::replace(&mut self.id, id),
            std::mem::replace(&mut self.position, position),
        );
        ContextSwitch { cx: self, previous }
    }

    fn inner(&self) -> &ContextInner {
        &self.store.arenas[self.position].contexts[self.id.index]
    }

    fn inner_mut(&mut self) -> &mut ContextInner {
        &mut self.store.arenas[self.position].contexts[self.id.index]
    }

    fn ccx(&self) -> &ComponentT {
        let arena = &self.store.arenas[self.position];
        &arena.components[arena.contexts[self.id.index].ccx.index]
    }

    fn ccx_mut(&mut self) -> &mut ComponentT {
        let arena = &mut *self.store.arenas[self.position];
        let ccx = arena.contexts[self.id.index].ccx.index;
        &mut arena.components[ccx]
    }

    pub fn sig_file(&self, id: SigFileId) -> Rc<SigFile> {
        self.store.sig_file(id)
    }

    /// Adds a signature file to the arena that owns this context.
    pub fn add_sig_file(&mut self, mk_sig_file: impl FnOnce(SigFileId) -> SigFile) -> SigFileId {
        self.store.arenas[self.position].add_sig_file(mk_sig_file)
    }

    /// The context to report annotation inference errors on: the one set with
    /// [Context::set_annot_error_dst], or else the current context.
    pub fn annot_error_dst(&self) -> ContextId {
        self.annot_error_dst
            .filter(|id| self.store.has_context(*id))
            .unwrap_or(self.id)
    }

    /// Reports annotation inference errors raised through this context, in whatever context it
    /// switches to, on `dst_cx`.
    pub fn set_annot_error_dst(&mut self, dst_cx: ContextId) {
        self.annot_error_dst = Some(dst_cx);
    }

    /// Replaces how this context resolves its requires, for a context whose requires can only be
    /// resolved once it exists.
    pub fn set_resolve_require(&mut self, resolve_require: ResolveRequire) {
        self.inner_mut().resolve_require = resolve_require;
    }

    pub fn sig_cx(&self) -> &TypeContext<Cx> {
        &self.ccx().sig_cx
    }

    // modules

    pub fn in_declare_module(&self) -> bool {
        self.inner().environment.scope_kind == ScopeKind::DeclareModule
    }

    pub fn in_declare_namespace(&self) -> bool {
        self.inner().environment.scope_kind == ScopeKind::DeclareNamespace
    }

    // accessors

    pub fn metadata(&self) -> &Metadata {
        &self.inner().metadata
    }

    pub fn babel_loose_array_spread(&self) -> bool {
        self.inner().metadata.frozen.babel_loose_array_spread
    }

    pub fn file(&self) -> &FileKey {
        &self.inner().file
    }

    pub fn type_strictness_kind(&self) -> TypeStrictnessKind {
        TypeStrictnessKind::from_is_typescript(files::has_ts_ext(self.file()))
    }

    pub fn check_budget(&self) -> Result<(), JobError> {
        self.inner().budget.check()
    }

    /// Returns the per-file budget so dependency contexts (e.g., those built
    /// inside `dep_file`) can inherit the parent file's cancel + timeout
    /// budget rather than starting a fresh one.
    pub fn budget(&self) -> CheckBudget {
        self.inner().budget
    }

    pub fn is_global_libdef(&self, file: &FileKey) -> bool {
        files::is_lib_file(&self.inner().all_unordered_libs, file)
            || self.inner().metadata.frozen.global_libdefs.contains(file)
    }

    pub fn is_lib_reason(&self, reason: &Reason) -> bool {
        reason
            .loc()
            .source()
            .is_some_and(|file| self.is_global_libdef(file))
    }

    pub fn is_lib_reason_def(&self, reason: &Reason) -> bool {
        reason
            .def_loc()
            .source()
            .is_some_and(|file| self.is_global_libdef(file))
    }

    pub fn is_global_lib_context(&self) -> bool {
        self.is_global_libdef(&self.inner().file)
    }

    pub fn under_declaration_context(&self) -> bool {
        self.is_global_lib_context()
            || self.inner().file.check_suffix(".flow")
            || flow_parser::file_key::has_dts_ext(self.inner().file.as_str())
            || self.in_declare_module()
            || self.in_declare_namespace()
    }

    fn in_dirlist(&self, dirs: &[Regex]) -> bool {
        match dirs {
            [] => false,
            _ => {
                let filename = self.inner().file.to_absolute();
                let normalized_filename =
                    flow_common::sys_utils::normalize_filename_dir_sep(&filename);
                dirs.iter().any(|r| r.is_match(&normalized_filename))
            }
        }
    }

    pub fn builtins(&mut self) -> Rc<Builtins<Cx>> {
        let builtins = self.inner().builtins.dupe();
        builtins.get_forced(self).dupe()
    }

    pub fn builtin_value_opt(&mut self, name: &str) -> Option<(ALoc, Type)> {
        if let Some((l, t)) = self.inner().builtin_values.get(name) {
            return Some((l.dupe(), t.dupe()));
        }
        let (l, t) = self.builtins().read_value_opt(self, name)?;
        self.inner_mut()
            .builtin_values
            .insert(FlowSmolStr::from(name), (l.dupe(), t.dupe()));
        Some((l, t))
    }

    pub fn builtin_type_opt(&mut self, name: &str) -> Option<(ALoc, Type)> {
        if let Some((l, t)) = self.inner().builtin_types.get(name) {
            return Some((l.dupe(), t.dupe()));
        }
        match self.builtins().read_type_opt(self, name) {
            Some((l, t)) => {
                self.inner_mut()
                    .builtin_types
                    .insert(FlowSmolStr::from(name), (l.dupe(), t.dupe()));
                Some((l, t))
            }
            None => self
                .builtin_value_opt(name)
                .or_else(|| self.builtin_umd_value_opt(name)),
        }
    }

    /// The UMD global `name` declared with `export as namespace`. Like library signatures, which
    /// cannot see them, library definitions don't get UMD globals.
    pub fn builtin_umd_value_opt(&mut self, name: &str) -> Option<(ALoc, Type)> {
        if self.is_global_lib_context() {
            return None;
        }
        if let Some((l, t)) = self.inner().builtin_umd_values.get(name) {
            return Some((l.dupe(), t.dupe()));
        }
        let (l, t) = self.builtins().read_umd_value_opt(self, name)?;
        self.inner_mut()
            .builtin_umd_values
            .insert(FlowSmolStr::from(name), (l.dupe(), t.dupe()));
        Some((l, t))
    }

    pub fn builtin_module_opt(&mut self, name: &Userland) -> Option<(Reason, LazyModuleType<Cx>)> {
        if let Some((r, module)) = self.inner().builtin_modules.get(name.as_str()) {
            return Some((r.dupe(), module.dupe()));
        }
        let (r, module) = self.builtins().read_module_opt(self, name)?;
        let module: LazyModuleType<Cx> = Rc::new(flow_lazy::Lazy::new_forced(module));
        self.inner_mut()
            .builtin_modules
            .insert(FlowSmolStr::from(name.as_str()), (r.dupe(), module.dupe()));
        Some((r, module))
    }

    pub fn component_syntax(&self) -> bool {
        self.inner().metadata.frozen.component_syntax || self.is_global_lib_context()
    }

    pub fn async_component_syntax(&self) -> bool {
        self.in_dirlist(&self.inner().metadata.frozen.async_component_syntax_includes)
            || self.inner().metadata.frozen.async_component_syntax
    }

    pub fn hook_compatibility(&self) -> bool {
        self.in_dirlist(&self.inner().metadata.frozen.hook_compatibility_includes)
            || (self.inner().metadata.frozen.hook_compatibility
                && !self.in_dirlist(&self.inner().metadata.frozen.hook_compatibility_excludes))
    }

    pub fn disallow_dynamic_import(&self) -> bool {
        self.inner().metadata.frozen.haste_disallow_dynamic_import
            && flow_common::files::haste_name_opt(
                &self.inner().metadata.frozen.file_options,
                &self.inner().file,
            )
            .is_some()
    }

    pub fn react_rule_enabled(&self, rule: ReactRule) -> bool {
        self.inner().metadata.frozen.react_rules.contains(&rule)
    }

    pub fn dev_only_refinement_info_as_errors(&self) -> bool {
        self.inner()
            .metadata
            .frozen
            .dev_only_refinement_info_as_errors
    }

    pub fn enable_const_params(&self) -> bool {
        self.inner().metadata.frozen.enable_const_params
            || self.inner().metadata.overridable.strict
            || self.inner().metadata.overridable.strict_local
    }

    pub fn strict_property_initialization(&self) -> bool {
        self.inner().metadata.frozen.strict_property_initialization
    }

    pub fn enable_custom_error(&self) -> bool {
        self.inner().metadata.frozen.enable_custom_error
    }

    pub fn enable_enums(&self) -> bool {
        self.inner().metadata.frozen.enable_enums
    }

    pub fn enable_jest_integration(&self) -> bool {
        self.inner().metadata.frozen.enable_jest_integration
    }

    pub fn enable_pattern_matching(&self) -> bool {
        self.inner().metadata.frozen.enable_pattern_matching
    }

    pub fn enable_records(&self) -> bool {
        self.inner().metadata.frozen.enable_records
            && match &self.inner().metadata.frozen.records_includes[..] {
                [] => true,
                dirs => self.in_dirlist(dirs),
            }
    }

    pub fn enable_relay_integration(&self) -> bool {
        self.inner().metadata.frozen.enable_relay_integration
            && flow_common::relay_options::enabled_for_file(
                &self.inner().metadata.frozen.relay_integration_excludes,
                &self.inner().file,
            )
    }

    pub fn relay_integration_esmodules(&self) -> bool {
        self.inner().metadata.frozen.relay_integration_esmodules
    }

    pub fn relay_integration_module_prefix(&self) -> Option<String> {
        flow_common::relay_options::module_prefix_for_file(
            &self
                .inner()
                .metadata
                .frozen
                .relay_integration_module_prefix_includes,
            &self.inner().file,
            self.inner()
                .metadata
                .frozen
                .relay_integration_module_prefix
                .as_deref(),
        )
    }

    pub fn errors(&self) -> ErrorSet {
        self.ccx().errors.dupe()
    }

    pub fn error_suppressions(&self) -> &ErrorSuppressions {
        &self.ccx().error_suppressions
    }

    pub fn take_error_suppressions(&mut self) -> ErrorSuppressions {
        std::mem::take(&mut self.ccx_mut().error_suppressions)
    }

    pub fn evaluated(&self) -> type_::eval::Map<Type> {
        self.ccx().sig_cx.evaluated.dupe()
    }

    pub fn file_options(&self) -> Arc<FileOptions> {
        self.inner().metadata.frozen.file_options.dupe()
    }

    pub fn aloc_tables(&self) -> &HashMap<FileKey, LazyALocTable> {
        &self.ccx().aloc_tables
    }

    pub fn find_props(&self, id: type_::properties::Id) -> type_::properties::PropertiesMap {
        self.ccx()
            .sig_cx
            .property_maps
            .get(&id)
            .cloned()
            .ok_or(PropsNotFound(id))
            .unwrap()
    }

    pub fn find_props_opt(
        &self,
        id: type_::properties::Id,
    ) -> Option<type_::properties::PropertiesMap> {
        self.ccx().sig_cx.property_maps.get(&id).cloned()
    }

    pub fn find_call(&self, id: i32) -> Type {
        self.ccx()
            .sig_cx
            .call_props
            .get(&id)
            .cloned()
            .ok_or(CallNotFound(id))
            .unwrap()
    }

    pub fn find_exports(&self, id: type_::exports::Id) -> type_::exports::T {
        self.ccx()
            .sig_cx
            .export_maps
            .get(&id)
            .cloned()
            .ok_or(ExportsNotFound(id))
            .unwrap()
    }

    pub fn find_require(&mut self, mref: &FlowImportSpecifier) -> ResolvedRequire {
        let resolve_require = self.inner().resolve_require.dupe();
        resolve_require(self, mref)
    }

    pub fn find_tvar(
        &self,
        id: i32,
    ) -> flow_utils_union_find::Node<type_::constraint::Constraints<Cx>> {
        self.ccx()
            .sig_cx
            .graph
            .get(&id)
            .cloned()
            .ok_or(TvarNotFound(id))
            .unwrap()
    }

    pub fn graph(&self) -> &flow_utils_union_find::Graph<type_::constraint::Constraints<Cx>> {
        &self.ccx().sig_cx.graph
    }

    pub fn graph_mut(
        &mut self,
    ) -> &mut flow_utils_union_find::Graph<type_::constraint::Constraints<Cx>> {
        &mut self.ccx_mut().sig_cx.graph
    }

    pub fn is_checked(&self) -> bool {
        self.inner().metadata.overridable.checked
    }

    pub fn is_verbose(&self) -> bool {
        match &self.inner().metadata.frozen.verbose {
            None => false,
            Some(v) if v.focused_files.is_none() => {
                v.enabled_during_flowlib || !self.is_global_lib_context()
            }
            Some(v) => {
                let file = &self.inner().file;
                if self.is_global_lib_context() {
                    v.enabled_during_flowlib
                } else {
                    v.focused_files.as_ref().is_some_and(|files| {
                        let abs = file.to_absolute();
                        files.iter().any(|f| f == &abs)
                    })
                }
            }
        }
    }

    pub fn is_strict(&self) -> bool {
        self.in_declare_module() || self.inner().metadata.overridable.strict
    }

    pub fn is_strict_local(&self) -> bool {
        self.inner().metadata.overridable.strict_local
    }

    pub fn available_platforms(&self) -> Option<&PlatformSet> {
        self.inner()
            .metadata
            .overridable
            .available_platforms
            .as_ref()
    }

    pub fn has_explicit_supports_platform(&self) -> bool {
        self.inner()
            .metadata
            .overridable
            .has_explicit_supports_platform
    }

    pub fn include_suppressions(&self) -> bool {
        self.inner().metadata.frozen.include_suppressions
    }

    pub fn severity_cover(&self) -> &FlowOrdMap<FileKey, LintSeverityCover> {
        &self.ccx().severity_cover
    }

    pub fn has_property_map(&self, id: &type_::properties::Id) -> bool {
        self.ccx().sig_cx.property_maps.contains_key(id)
    }

    pub fn has_call_prop(&self, id: &i32) -> bool {
        self.ccx().sig_cx.call_props.contains_key(id)
    }

    pub fn has_export_map(&self, id: &type_::exports::Id) -> bool {
        self.ccx().sig_cx.export_maps.contains_key(id)
    }

    pub fn projects_options(&self) -> &ProjectsOptions {
        &self.inner().metadata.frozen.projects_options
    }

    pub fn react_custom_jsx_typing(&self) -> bool {
        self.inner().metadata.frozen.react_custom_jsx_typing
    }

    pub fn react_managed_attributes(&self) -> bool {
        self.inner().metadata.frozen.react_managed_attributes
    }

    pub fn react_runtime(&self) -> ReactRuntime {
        self.inner().metadata.overridable.react_runtime
    }

    pub fn recursion_limit(&self) -> i32 {
        self.inner().metadata.frozen.recursion_limit
    }

    pub fn root(&self) -> &PathBuf {
        &self.inner().metadata.frozen.root
    }

    pub fn facebook_fbs(&self) -> Option<&str> {
        self.inner().metadata.frozen.facebook_fbs.as_deref()
    }

    pub fn facebook_fbt(&self) -> Option<&str> {
        self.inner().metadata.frozen.facebook_fbt.as_deref()
    }

    pub fn facebook_module_interop(&self) -> bool {
        self.inner().metadata.frozen.facebook_module_interop
    }

    pub fn should_ignore_non_literal_requires(&self) -> bool {
        self.inner().metadata.frozen.ignore_non_literal_requires
    }

    pub fn should_munge_underscores(&self) -> bool {
        self.inner().metadata.overridable.munge_underscores
    }

    pub fn should_strip_root(&self) -> bool {
        self.inner().metadata.frozen.strip_root
    }

    pub fn stylex_shorthand_prop(&self) -> Option<&str> {
        self.inner()
            .metadata
            .frozen
            .stylex_shorthand_prop
            .as_deref()
    }

    pub fn export_as_namespace_support(&self) -> bool {
        self.inner().metadata.frozen.export_as_namespace_support
    }

    pub fn export_star_excludes_default(&self) -> bool {
        self.inner().metadata.frozen.export_star_excludes_default
    }

    pub fn normalize_intersections(&self) -> bool {
        self.inner().metadata.frozen.normalize_intersections
    }

    pub fn ts_syntax(&self) -> bool {
        self.inner().metadata.frozen.ts_syntax || self.is_global_lib_context()
    }

    pub fn ts_utility_syntax(&self) -> bool {
        self.inner().metadata.frozen.ts_utility_syntax
    }

    pub fn tslib_syntax(&self) -> bool {
        self.inner().metadata.frozen.tslib_syntax || self.is_global_lib_context()
    }

    pub fn typescript_library_definition_support(&self) -> bool {
        self.inner()
            .metadata
            .frozen
            .typescript_library_definition_support
    }

    /// Whether the current source module contributes a supported `declare global` block or a
    /// `.d.ts` `export as namespace` UMD global.
    pub fn is_ts_global_augmentation(&self) -> bool {
        self.inner().metadata.overridable.is_ts_global_augmentation
    }

    pub fn is_ts_file(&self) -> bool {
        files::has_ts_ext(self.file())
    }

    pub fn assert_operator_enabled(&self) -> bool {
        self.is_ts_file() || self.inner().metadata.frozen.assert_operator.usable()
    }

    pub fn assert_operator_specialized(&self) -> bool {
        !self.is_ts_file() && self.inner().metadata.frozen.assert_operator.specialized()
    }

    pub fn type_expansion_recursion_limit(&self) -> i32 {
        self.inner().metadata.frozen.type_expansion_recursion_limit
    }

    pub fn delayed_forcing_tvars(&self) -> &FlowOrdSet<i32> {
        &self.ccx().delayed_forcing_tvars
    }

    pub fn post_component_tvar_forcing_states(&mut self) -> FlowVector<ForcingState<Cx>> {
        std::mem::take(&mut self.ccx_mut().post_component_tvar_forcing_states)
    }

    pub fn force_post_component_tvars(&mut self) {
        for state in self.post_component_tvar_forcing_states().iter() {
            if self.is_active_tvar_forcing_state(state) {
                self.ccx_mut()
                    .post_component_tvar_forcing_states
                    .push_back(state.clone());
            } else {
                self.force_fully_resolved_tvar(state);
            }
        }
    }

    pub fn is_fully_forced_tvar(&self, root_id: i32) -> bool {
        self.ccx().fully_forced_tvars.contains(&root_id)
    }

    pub fn add_fully_forced_tvars(&mut self, root_ids: impl IntoIterator<Item = i32>) {
        self.ccx_mut().fully_forced_tvars.extend(root_ids);
    }

    pub fn is_active_tvar_forcing_state(&self, state: &ForcingState<Cx>) -> bool {
        self.ccx()
            .active_tvar_forcing_states
            .iter()
            .any(|active| active.ptr_eq(state))
    }

    pub fn post_inference_polarity_checks(
        &self,
    ) -> &Vec<(
        BTreeMap<SubstName, type_::TypeParam>,
        flow_common::polarity::Polarity,
        Type,
    )> {
        &self.ccx().post_inference_polarity_checks
    }

    /// The flows queued for the post-inference validation, which runs them once.
    pub fn take_post_inference_validation_flows(&mut self) -> VecDeque<(Type, type_::UseT<Cx>)> {
        std::mem::take(&mut self.ccx_mut().post_inference_validation_flows)
    }

    /// The callbacks queued for the post-inference validation, which runs them once.
    pub fn take_post_inference_validation_callbacks(
        &mut self,
    ) -> Vec<PostInferenceValidationCallback> {
        std::mem::take(&mut self.ccx_mut().post_inference_validation_callbacks)
    }

    /// Reserve a slot in `merging_interface_field_types` for every interface
    /// that participates in a merge, so later `record_interface_field` calls
    /// know who to record for in O(1). The set of participating interfaces is
    /// whatever env_builder reported in `interface_merge_conflicts`; we don't
    /// compute it ourselves.
    pub fn init_interface_merge_field_index(&mut self) {
        let interface_merge_conflicts =
            self.environment().var_info.interface_merge_conflicts.dupe();
        let declare_class_interface_merge_conflicts = self
            .environment()
            .var_info
            .declare_class_interface_merge_conflicts
            .dupe();
        let ccx = self.ccx_mut();
        ccx.merging_interface_typeparams.clear();
        let table = &mut ccx.merging_interface_field_types;
        table.clear();
        let mut install = |good_loc: &ALoc, bad_locs: &Vec<ALoc>| {
            table.entry(good_loc.dupe()).or_insert_with(HashMap::new);
            for bad_loc in bad_locs {
                table.entry(bad_loc.dupe()).or_insert_with(HashMap::new);
            }
        };
        for (good_loc, bad_locs) in interface_merge_conflicts.iter() {
            install(good_loc, bad_locs);
        }
        for (good_loc, bad_locs) in declare_class_interface_merge_conflicts.iter() {
            install(good_loc, bad_locs);
        }
    }

    pub fn record_interface_tparams(
        &mut self,
        id_loc: ALoc,
        tparams: &type_::TypeParams,
        tparams_map: &FlowOrdMap<SubstName, Type>,
    ) {
        if !self
            .ccx()
            .merging_interface_field_types
            .contains_key(&id_loc)
        {
            return;
        }

        let tparams = match tparams {
            None => Vec::new(),
            Some((_, tparams)) => tparams
                .iter()
                .filter_map(|tparam| {
                    tparams_map
                        .get(&tparam.name)
                        .map(|t| (tparam.dupe(), t.dupe()))
                })
                .collect(),
        };
        self.ccx_mut()
            .merging_interface_typeparams
            .insert(id_loc, tparams);
    }

    /// TypeScript declaration merging matches type parameters positionally,
    /// not by spelling. The source declaration may write
    /// `interface Foo<TValue>`, while the canonical declaration wrote
    /// `interface Foo<T>`. Member types copied or compared from the source
    /// declaration therefore need `TValue` substituted with the canonical
    /// `GenericT` for `T`. A length mismatch means another validation owns
    /// the arity error, so the checker leaves the member types unchanged.
    pub fn interface_tparam_subst_map(
        &self,
        source_loc: &ALoc,
        target_loc: &ALoc,
    ) -> FlowOrdMap<SubstName, Type> {
        let typeparams = &self.ccx().merging_interface_typeparams;
        let (Some(source_tparams), Some(target_tparams)) =
            (typeparams.get(source_loc), typeparams.get(target_loc))
        else {
            return FlowOrdMap::new();
        };
        if source_tparams.len() != target_tparams.len() {
            return FlowOrdMap::new();
        }
        source_tparams
            .iter()
            .zip(target_tparams)
            .map(|((source_tparam, _), (_, target_t))| (source_tparam.name.dupe(), target_t.dupe()))
            .collect()
    }

    /// Remember that interface `id_loc` declared a Field named `name` of type
    /// `t`. Only matters for interfaces that participate in declaration
    /// merging — for anyone else, the absence of a slot in the table makes
    /// this a no-op. If the same name shows up twice in one declaration (a
    /// syntax-level oddity that the rest of the diff treats as "first
    /// wins"), we keep the first type so the post-inference check sees the
    /// same type the merger sees.
    pub fn record_interface_field(&mut self, id_loc: ALoc, name: Name, t: Type) {
        if let Some(inner) = self
            .ccx_mut()
            .merging_interface_field_types
            .get_mut(&id_loc)
        {
            inner.entry(name).or_insert(t);
        }
    }

    /// The list of "these two types should be the same, and here's why"
    /// obligations that interface declaration merging produces. Post-inference
    /// substitutes the current declaration's type parameters into the earlier
    /// declaration's namespace and unifies shared fields and repeated type
    /// parameter defaults.
    pub fn interface_merge_unify_tasks(
        &self,
    ) -> Vec<(UseOp, FlowOrdMap<SubstName, Type>, Type, Type)> {
        let interface_conflicts = self.environment().var_info.interface_merge_conflicts.dupe();
        let dc_conflicts = self
            .environment()
            .var_info
            .declare_class_interface_merge_conflicts
            .dupe();
        let fields = &self.ccx().merging_interface_field_types;
        let typeparams = &self.ccx().merging_interface_typeparams;
        let declaration_type = |loc: &ALoc| {
            self.environment()
                .find_ordinary_write(loc.dupe())
                .map(|entry| TypeOrTypeDescT::Type(entry.t.dupe()))
                .unwrap_or(TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Unknown))
        };
        let walk =
            |conflicts: &FlowOrdMap<ALoc, Vec<ALoc>>,
             acc: &mut Vec<(UseOp, FlowOrdMap<SubstName, Type>, Type, Type)>| {
                for (good_loc, bad_locs) in conflicts.iter() {
                    let Some(good_fields) = fields.get(good_loc) else {
                        continue;
                    };
                    let first_decl = declaration_type(good_loc);
                    for bad_loc in bad_locs {
                        let Some(bad_fields) = fields.get(bad_loc) else {
                            continue;
                        };
                        let current_decl = VirtualReason::new(None, bad_loc.dupe());
                        let use_op = UseOp::Op(Arc::new(RootUseOp::MergedDeclaration(Box::new(
                            MergedDeclarationData {
                                conflict: MergedDeclarationConflict::Property,
                                first_decl_loc: good_loc.dupe(),
                                first_decl: first_decl.clone(),
                                current_decl,
                            },
                        ))));
                        let tparam_subst_map = self.interface_tparam_subst_map(bad_loc, good_loc);
                        for (name, bad_t) in bad_fields.iter() {
                            if let Some(good_t) = good_fields.get(name) {
                                acc.push((
                                    use_op.dupe(),
                                    tparam_subst_map.dupe(),
                                    bad_t.dupe(),
                                    good_t.dupe(),
                                ));
                            }
                        }
                    }

                    let declaration_locs: Vec<&ALoc> =
                        std::iter::once(good_loc).chain(bad_locs.iter()).collect();
                    let declaration_tparams: Option<Vec<_>> = declaration_locs
                        .iter()
                        .map(|loc| typeparams.get(*loc))
                        .collect();
                    let Some(declaration_tparams) = declaration_tparams else {
                        continue;
                    };
                    let Some(arity) = declaration_tparams.first().map(|tparams| tparams.len())
                    else {
                        continue;
                    };
                    if declaration_tparams
                        .iter()
                        .any(|tparams| tparams.len() != arity)
                    {
                        continue;
                    }

                    let mut first_defaults: Vec<Option<(ALoc, Type)>> =
                        std::iter::repeat_with(|| None).take(arity).collect();
                    for (decl_loc, tparams) in declaration_locs.iter().zip(declaration_tparams) {
                        for (index, (tparam, _)) in tparams.iter().enumerate() {
                            let Some(default) = tparam.default.as_ref() else {
                                continue;
                            };
                            match &first_defaults[index] {
                                None => {
                                    first_defaults[index] =
                                        Some(((*decl_loc).dupe(), default.dupe()));
                                }
                                Some((first_loc, first_default)) => {
                                    let use_op = UseOp::Op(Arc::new(RootUseOp::MergedDeclaration(
                                        Box::new(MergedDeclarationData {
                                            conflict: MergedDeclarationConflict::TypeParamDefault,
                                            first_decl_loc: first_loc.dupe(),
                                            first_decl: declaration_type(first_loc),
                                            current_decl: VirtualReason::new(
                                                None,
                                                (*decl_loc).dupe(),
                                            ),
                                        }),
                                    )));
                                    acc.push((
                                        use_op,
                                        self.interface_tparam_subst_map(decl_loc, first_loc),
                                        default.dupe(),
                                        first_default.dupe(),
                                    ));
                                }
                            }
                        }
                    }
                }
            };
        let mut acc = Vec::new();
        walk(&interface_conflicts, &mut acc);
        walk(&dc_conflicts, &mut acc);
        acc
    }

    pub fn interface_prop_ids(
        &self,
    ) -> &BTreeMap<ALoc, (type_::properties::Id, type_::properties::Id)> {
        &self.ccx().interface_prop_ids
    }

    /// Hand the merger a way to find this declaration's property maps later.
    /// The ids point at the heap-allocated maps that back its InstanceT; the
    /// merger takes those ids and copies properties between them when
    /// several decls of the same interface need to be folded together.
    pub fn add_interface_prop_ids(
        &mut self,
        loc: ALoc,
        own_props: type_::properties::Id,
        proto_props: type_::properties::Id,
    ) {
        self.ccx_mut()
            .interface_prop_ids
            .insert(loc, (own_props, proto_props));
    }

    pub fn env_cache_find_opt(
        &self,
        for_value: bool,
        id: i32,
    ) -> Option<PossiblyRefinedWriteState> {
        let cache = if for_value {
            &self.ccx().env_value_cache
        } else {
            &self.ccx().env_type_cache
        };
        cache.get(&id).cloned()
    }

    pub fn missing_local_annot_lower_bounds(&self) -> &ALocFuzzyMap<Vec<Type>> {
        &self.ccx().missing_local_annot_lower_bounds
    }

    pub fn array_or_object_literal_declaration_upper_bounds(&self) -> Vec<(ALoc, Vec<Type>)> {
        self.ccx()
            .array_or_object_literal_declaration_upper_bounds
            .values()
            .cloned()
            .collect()
    }

    pub fn inferred_component_return(&self) -> &ALocFuzzyMap<Vec1<Type>> {
        &self.ccx().inferred_component_return
    }

    pub fn use_unknown_in_catch_variables(&self) -> bool {
        self.inner().metadata.frozen.use_unknown_in_catch_variables
    }

    pub fn ban_spread_key_props(&self) -> bool {
        self.inner().metadata.frozen.ban_spread_key_props
    }

    pub fn verbose(&self) -> Option<&Verbose> {
        self.inner().metadata.frozen.verbose.as_deref()
    }

    pub fn slow_to_check_logging(&self) -> &SlowToCheckLogging {
        &self.inner().metadata.frozen.slow_to_check_logging
    }

    pub fn max_workers(&self) -> i32 {
        self.inner().metadata.frozen.max_workers
    }

    pub fn missing_module_generators(&self) -> &[(Regex, String)] {
        &self.inner().metadata.frozen.missing_module_generators
    }

    pub fn no_implicit_override(&self) -> bool {
        self.inner().metadata.frozen.no_implicit_override
    }

    pub fn no_unchecked_indexed_access(&self) -> bool {
        self.inner().metadata.frozen.no_unchecked_indexed_access
    }

    pub fn jsx(&self) -> &JsxMode {
        &self.inner().metadata.overridable.jsx
    }

    pub fn exists_checks(&self) -> &ALocMap<BTreeSet<Type>> {
        &self.ccx().exists_checks
    }

    pub fn exists_excuses(&self) -> &ALocMap<ExistsCheck> {
        &self.ccx().exists_excuses
    }

    pub fn exists_excuses_mut(&mut self) -> &mut ALocMap<ExistsCheck> {
        &mut self.ccx_mut().exists_excuses
    }

    pub fn voidable_checks(&self) -> &Vec<VoidableCheck> {
        &self.ccx().voidable_checks
    }

    pub fn reachable_deps(&self) -> &BTreeSet<FileKey> {
        &self.inner().reachable_deps
    }

    pub fn environment(&self) -> &LocEnv<Cx> {
        &self.inner().environment
    }

    pub fn environment_mut(&mut self) -> &mut LocEnv<Cx> {
        &mut self.inner_mut().environment
    }

    pub fn typing_mode(&self) -> &TypingMode {
        &self.inner().typing_mode
    }

    pub fn node_cache(&self) -> &NodeCache<Cx> {
        &self.inner().node_cache
    }

    pub fn node_cache_mut(&mut self) -> &mut NodeCache<Cx> {
        &mut self.inner_mut().node_cache
    }

    pub fn refined_locations(&self) -> &ALocMap<FlowOrdSet<ALoc>> {
        &self.inner().refined_locations
    }

    pub fn aggressively_invalidated_locations(&self) -> &ALocMap<RefinementInvalidation> {
        &self.inner().aggressively_invalidated_locations
    }

    pub fn switch_to_match_eligible_locations(&self) -> &ALocSet {
        &self.inner().switch_to_match_eligible_locations
    }

    pub fn add_ts_import_provenance(
        &mut self,
        def_loc: ALoc,
        source: FlowImportSpecifier,
        remote_name: FlowSmolStr,
    ) {
        self.inner_mut()
            .ts_import_provenance
            .insert(def_loc, (source, remote_name));
    }

    pub fn find_ts_import_provenance(
        &self,
        def_loc: &ALoc,
    ) -> Option<(FlowImportSpecifier, FlowSmolStr)> {
        self.inner()
            .ts_import_provenance
            .get(def_loc)
            .map(|(s, r)| (s.dupe(), r.dupe()))
    }

    pub fn hint_map_arglist_cache(
        &self,
    ) -> &HashMap<(ALoc, Option<ALoc>), Vec<(ALoc, type_::CallArg)>> {
        &self.inner().hint_map_arglist_cache
    }

    pub fn hint_map_jsx_cache(
        &self,
    ) -> &HashMap<(Reason, String, Vec<ALoc>, ALoc), LazyHintMapJsx> {
        &self.inner().hint_map_jsx_cache
    }

    pub fn hint_map_jsx_cache_mut(
        &mut self,
    ) -> &mut HashMap<(Reason, String, Vec<ALoc>, ALoc), LazyHintMapJsx> {
        &mut self.inner_mut().hint_map_jsx_cache
    }

    pub fn hint_map_arglist_cache_mut(
        &mut self,
    ) -> &mut HashMap<(ALoc, Option<ALoc>), Vec<(ALoc, type_::CallArg)>> {
        &mut self.inner_mut().hint_map_arglist_cache
    }

    pub fn hint_eval_cache_find_opt(&self, id: i32) -> Option<Option<Type>> {
        self.inner().hint_eval_cache.get(&id).duped()
    }

    pub fn automatic_require_default(&self) -> bool {
        self.inner().metadata.frozen.automatic_require_default
    }

    pub fn pid_prefix(&self) -> String {
        if self.max_workers() > 0 {
            format!("[{}] ", std::process::id())
        } else {
            String::new()
        }
    }

    // Mutators

    pub fn add_exhaustive_check(&mut self, loc: ALoc, x: (Vec<ALoc>, bool)) {
        self.ccx_mut().exhaustive_checks.insert(loc, x);
    }

    pub fn add_error(&mut self, error: FlowError<ALoc>) {
        self.ccx_mut().errors.add(error);
    }

    pub fn reset_errors(&mut self, errors: ErrorSet) {
        self.ccx_mut().errors = errors;
    }

    /// Run [f] with the current error set snapshotted, then restore it.
    /// Useful when a probe must call into machinery that can emit unrelated
    /// errors as a side effect (e.g. concretization). The snapshot is
    /// restored even if [f] returns an [Err].
    pub fn with_suppressed_errors<T, E>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        let saved = self.errors();
        let r = f(self);
        self.reset_errors(saved);
        r
    }

    pub fn add_error_suppressions(&mut self, suppressions: ErrorSuppressions) {
        self.ccx_mut().error_suppressions.union(suppressions);
    }

    pub fn add_severity_covers(&mut self, severity_covers: FlowOrdMap<FileKey, LintSeverityCover>) {
        self.ccx_mut().severity_cover.extend(severity_covers);
    }

    pub fn add_property_map(
        &mut self,
        id: type_::properties::Id,
        pmap: type_::properties::PropertiesMap,
    ) {
        self.ccx_mut().sig_cx.property_maps.insert(id, pmap);
    }

    pub fn add_call_prop(&mut self, id: i32, t: Type) {
        self.ccx_mut().sig_cx.call_props.insert(id, t);
    }

    pub fn add_export_map(&mut self, id: type_::exports::Id, tmap: type_::exports::T) {
        self.ccx_mut().sig_cx.export_maps.insert(id, tmap);
    }

    pub fn add_tvar(
        &mut self,
        id: i32,
        bounds: flow_utils_union_find::Node<type_::constraint::Constraints<Cx>>,
    ) {
        self.graph_mut().insert(id, bounds);
    }

    pub fn add_fresh_tvar(
        &mut self,
        id: i32,
        bounds: flow_utils_union_find::Node<type_::constraint::Constraints<Cx>>,
    ) {
        self.graph_mut().insert_fresh(id, bounds);
    }

    pub fn add_inference_node(&mut self, id: i32) {
        assert!(
            self.graph().get(&id).is_none(),
            "inference node id collides with a production node"
        );
        let previous = self.ccx_mut().inference_nodes.insert(
            id,
            Node::create_root(type_::constraint::Constraints::default()),
        );
        assert!(previous.is_none(), "inference node id registered twice");
    }

    pub fn is_inference_node(&self, id: i32) -> bool {
        self.ccx().inference_nodes.contains_key(&id)
    }

    pub fn find_inference_root(
        &mut self,
        id: i32,
    ) -> (i32, Root<type_::constraint::Constraints<Cx>>) {
        assert!(self.is_inference_node(id), "inference node not found");
        let mut current = id;
        let mut path = Vec::new();
        let mut seen = BTreeSet::new();
        let (root_id, root) = loop {
            assert!(seen.insert(current), "cyclic inference node aliases");
            let node = self.ccx().inference_nodes.get(&current).cloned();
            match node {
                Some(Node::Root(root)) => break (current, root),
                Some(Node::Goto { parent }) => {
                    path.push(current);
                    current = parent;
                }
                None => break self.find_root(current),
            }
        };
        let inference_nodes = &mut self.ccx_mut().inference_nodes;
        for id in path {
            inference_nodes.insert(id, Node::create_goto(root_id));
        }
        (root_id, root)
    }

    pub fn set_inference_root_constraints(
        &mut self,
        id: i32,
        constraints: type_::constraint::Constraints<Cx>,
    ) {
        let node = self
            .ccx_mut()
            .inference_nodes
            .get_mut(&id)
            .expect("inference root not found");
        let Node::Root(root) = node else {
            panic!("inference node is not a root");
        };
        root.constraints = constraints;
    }

    pub fn set_inference_root_rank(&mut self, id: i32, rank: i32) {
        let node = self
            .ccx_mut()
            .inference_nodes
            .get_mut(&id)
            .expect("inference root not found");
        let Node::Root(root) = node else {
            panic!("inference node is not a root");
        };
        root.rank = rank;
    }

    pub fn set_inference_goto(&mut self, id: i32, parent: i32) {
        let previous = self
            .ccx_mut()
            .inference_nodes
            .insert(id, Node::create_goto(parent));
        assert!(previous.is_some(), "inference node not found");
    }

    pub fn set_synthesis_produced_uncacheable_result(&mut self) {
        match self.inner().typing_mode {
            TypingMode::SynthesisMode { .. } => {
                self.ccx_mut().synthesis_produced_uncacheable_result = true;
            }
            _ => {}
        }
    }

    pub fn mk_placeholder(&mut self, reason: Reason) -> Type {
        self.set_synthesis_produced_uncacheable_result();
        any_t::placeholder(reason)
    }

    pub fn add_post_component_tvar_forcing_state(&mut self, id: i32, state: ForcingState<Cx>) {
        let ccx = self.ccx_mut();
        ccx.delayed_forcing_tvars.insert(id);
        ccx.post_component_tvar_forcing_states.push_front(state);
    }

    pub fn add_post_inference_polarity_check(
        &mut self,
        tparams: BTreeMap<SubstName, type_::TypeParam>,
        polarity: flow_common::polarity::Polarity,
        t: Type,
    ) {
        self.ccx_mut()
            .post_inference_polarity_checks
            .push((tparams, polarity, t));
    }

    pub fn add_post_inference_validation_flow(&mut self, t: Type, use_t: type_::UseT<Cx>) {
        self.ccx_mut()
            .post_inference_validation_flows
            .push_front((t, use_t));
    }

    pub fn add_post_inference_subtyping_check(&mut self, l: Type, use_op: UseOp, u: Type) {
        self.add_post_inference_validation_flow(
            l,
            type_::UseT::new(type_::UseTInner::UseT(use_op, u)),
        );
    }

    pub fn add_post_inference_validation_callback(&mut self, f: PostInferenceValidationCallback) {
        self.ccx_mut().post_inference_validation_callbacks.push(f);
    }

    pub fn add_env_cache_entry(&mut self, for_value: bool, id: i32, t: PossiblyRefinedWriteState) {
        if for_value {
            self.ccx_mut().env_value_cache.insert(id, t);
        } else {
            self.ccx_mut().env_type_cache.insert(id, t);
        }
    }

    pub fn add_voidable_check(&mut self, voidable_check: VoidableCheck) {
        self.ccx_mut().voidable_checks.push(voidable_check);
    }

    pub fn add_reachable_dep(&mut self, file_key: FileKey) {
        self.inner_mut().reachable_deps.insert(file_key);
    }

    pub fn add_refined_location(&mut self, read_loc: ALoc, refining_locs: FlowOrdSet<ALoc>) {
        self.inner_mut()
            .refined_locations
            .insert(read_loc, refining_locs);
    }

    pub fn add_aggressively_invalidated_location(
        &mut self,
        loc: ALoc,
        info: RefinementInvalidation,
    ) {
        self.inner_mut()
            .aggressively_invalidated_locations
            .insert(loc, info);
    }

    pub fn add_switch_to_match_eligible_location(&mut self, loc: ALoc) {
        self.inner_mut()
            .switch_to_match_eligible_locations
            .insert(loc);
    }

    /// Records a call whose finalized callee is a single assertion function.
    pub fn add_assertion_call_candidate(&mut self, callee_loc: ALoc) {
        self.inner_mut()
            .assertion_call_candidates
            .insert(callee_loc);
    }

    /// Whether call checking recorded an assertion function at this callee.
    pub fn is_assertion_call_candidate(&self, callee_loc: &ALoc) -> bool {
        self.inner().assertion_call_candidates.contains(callee_loc)
    }

    pub fn add_missing_local_annot_lower_bound(&mut self, loc: ALoc, t: Type) {
        self.ccx_mut()
            .missing_local_annot_lower_bounds
            .entry(ALocFuzzy(loc))
            .or_insert_with(Vec::new)
            .push(t);
    }

    pub fn add_array_or_object_literal_declaration_tracking(&mut self, tvar_id: i32, loc: ALoc) {
        self.ccx_mut()
            .array_or_object_literal_declaration_upper_bounds
            .insert(tvar_id, (loc, Vec::new()));
    }

    pub fn report_array_or_object_literal_declaration_reposition(
        &mut self,
        repositioned_tvar_id: i32,
        tvar_id: i32,
    ) {
        let ccx = self.ccx_mut();
        if ccx
            .array_or_object_literal_declaration_upper_bounds
            .contains_key(&tvar_id)
        {
            ccx.array_or_object_literal_declaration_reposition_tracking
                .insert(repositioned_tvar_id, tvar_id);
        }
    }

    pub fn add_array_or_object_literal_declaration_upper_bound(&mut self, tvar_id: i32, t: Type) {
        let ccx = self.ccx_mut();
        let tvar_id = ccx
            .array_or_object_literal_declaration_reposition_tracking
            .get(&tvar_id)
            .copied()
            .unwrap_or(tvar_id);
        if let Some((_, bounds)) = ccx
            .array_or_object_literal_declaration_upper_bounds
            .get_mut(&tvar_id)
        {
            bounds.push(t);
        }
    }

    pub fn add_inferred_component_return(&mut self, loc: ALoc, t: Type) {
        let aloc_fuzzy = ALocFuzzy(loc);
        let return_map = &mut self.ccx_mut().inferred_component_return;
        match return_map.get_mut(&aloc_fuzzy) {
            None => {
                return_map.insert(aloc_fuzzy, Vec1::new(t));
            }
            Some(bounds) => {
                bounds.push(t);
            }
        }
    }

    pub fn set_evaluated(&mut self, evaluated: type_::eval::Map<Type>) {
        self.ccx_mut().sig_cx.evaluated = evaluated;
    }

    pub fn set_exists_checks(&mut self, exists_checks: ALocMap<BTreeSet<Type>>) {
        self.ccx_mut().exists_checks = exists_checks;
    }

    pub fn set_signature_help_callee(&mut self, loc: ALoc, t: Type) {
        self.ccx_mut().signature_help_callee.insert(loc, t);
    }

    pub fn get_signature_help_callee(&self, loc: &ALoc) -> Option<Type> {
        self.ccx().signature_help_callee.get(loc).duped()
    }

    pub fn set_ctor_callee(&mut self, loc: ALoc, t: Type) {
        self.ccx_mut().ctor_callee.insert(loc, t);
    }

    pub fn get_ctor_callee(&self, loc: &ALoc) -> Option<Type> {
        self.ccx().ctor_callee.get(loc).duped()
    }

    pub fn record_primitive_literal_check(&mut self, loc: ALoc) {
        self.ccx_mut().primitive_literal_checks.insert(loc);
    }

    pub fn is_primitive_literal_checked(&self, loc: &ALoc) -> bool {
        self.ccx().primitive_literal_checks.contains(loc)
    }

    pub fn record_fun_proto_method_lookup(&mut self, loc: ALoc) {
        self.ccx_mut().fun_proto_method_lookups.insert(loc);
    }

    pub fn is_fun_proto_method_lookup(&self, loc: &ALoc) -> bool {
        self.ccx().fun_proto_method_lookups.contains(loc)
    }

    pub fn set_enclosing_context_for_call(&mut self, loc: ALoc, t: EnclosingContext) {
        self.ccx_mut().enclosing_context_for_call.insert(loc, t);
    }

    pub fn get_enclosing_context_for_call(&self, loc: &ALoc) -> Option<EnclosingContext> {
        self.ccx().enclosing_context_for_call.get(loc).cloned()
    }

    pub fn set_union_opt(&mut self, loc: ALoc, t: Type) {
        self.ccx_mut().union_opt.insert(loc, t);
    }

    pub fn iter_union_opt<F>(&mut self, f: F)
    where
        F: FnMut(&mut Self, &ALoc, &Type),
    {
        let mut f = f;
        for (loc, t) in self.ccx().union_opt.clone().iter() {
            f(self, loc, t);
        }
    }

    pub fn add_exists_check(&mut self, loc: ALoc, t: Type) {
        let checks = &mut self.ccx_mut().exists_checks;
        match checks.get_mut(&loc) {
            Some(tset) => {
                tset.insert(t);
            }
            None => {
                let mut tset = BTreeSet::default();
                tset.insert(t);
                checks.insert(loc, tset);
            }
        }
    }

    pub fn set_exists_excuses(&mut self, exists_excuses: ALocMap<ExistsCheck>) {
        self.ccx_mut().exists_excuses = exists_excuses;
    }
}

pub struct CacheSnapshot {
    constraint_cache_level_count: usize,
    // eval_id_cache, id_cache, eval_repos_cache, fix_cache, this_specialize_cache,
    // const_fold_cache use MultiLevelMap — their state is tracked internally
    // via push_level/pop_level, not stored here.
    snapshot_subst_cache: type_::SubstCacheMap<(Vec<SubstCacheErr>, Type)>,
    snapshot_spread_cache: SpreadCache,
    snapshot_evaluated: type_::eval::Map<Type>,
}

/// Memoizes `elab_open` demands so cyclic annotations terminate instead
/// of overflowing the stack. Each variant must key all data that can
/// change elaboration (`use_op`/`reason` only affect error text and are
/// excluded). Unkeyable or side-effecting ops skip via `None`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ElabOpenMemoTag {
    GetProp {
        prop_ref: PropRef,
    },
    Lookup {
        prop_ref: PropRef,
        type_: Type,
        indexer_fallback: Option<Box<IndexerFallbackData>>,
    },
    ThisSpecialize {
        this: Type,
    },
    // `AnnotSpecializeT.types` selects the specialization: same avar
    // specialized with different type args must not share an indirection.
    // `use_op`/`reason`/`reason2` are error text only.
    Specialize {
        types: Option<Rc<[Type]>>,
        operation: AnnotationInferenceOperation,
    },
    // Every op with elaboration-relevant data gets a precise variant:
    // sharing an indirection across different payloads produces wrong
    // types. `use_op`/`reason` are error text only and always excluded.
    ImportTypeof {
        name: FlowSmolStr,
    },
    AssertExportIsType {
        name: Name,
    },
    UseTType {
        kind: TypeTKind,
    },
    GetTypeFromNamespace {
        name: Name,
    },
    GetElem {
        key: Type,
    },
    Elem {
        from_annot: bool,
        source: Type,
    },
    ObjKit {
        resolve_tool: ResolveTool,
        tool: Tool,
    },
    Arith {
        flip: bool,
        rhs_t: Type,
        kind: ArithKind,
    },
    UnaryArith {
        kind: UnaryArithKind,
    },
    DeepReadOnly {
        loc: ALoc,
        dro_type: DroType,
    },
    ToString {
        orig_t: Option<Type>,
    },
    ObjRest {
        keys: Rc<[FlowSmolStr]>,
    },
    // Payload-free ops: the variant alone identifies the demand.
    ConcretizeForCJS,
    GetEnum,
    GetStatics,
    ObjTestProto,
    Mixin,
    Not,
    ObjKeyMirror,
    GetKeys,
    GetKeysDictKey,
    GetValues,
    FilterOptional,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ElabOpenMemoKey {
    pub tag: ElabOpenMemoTag,
    pub id: i32,
}

/// Key for `elab_open`'s memo: the annotation variable and operation identify
/// the semantic demand. `seen` is deliberately excluded: it is checked before
/// lookup, and memoized thunks start with a canonical set containing only their
/// own id. `use_op` and `reason` are error text only and are excluded. `None`
/// keeps old behavior for concretize ops: one applies an opaque closure
/// (unkeyable), while the others write to collectors (memoizing would skip
/// collection on re-demands). The match is exhaustive so new op kinds fail to
/// compile until classified.
pub fn elab_open_memo_key(op: &Op<Cx>, id: i32) -> Option<ElabOpenMemoKey> {
    let tag = match &**op {
        OpInner::AnnotGetPropT(data) => ElabOpenMemoTag::GetProp {
            prop_ref: data.prop_ref.clone(),
        },
        OpInner::AnnotLookupT(data) => ElabOpenMemoTag::Lookup {
            prop_ref: data.prop_ref.clone(),
            type_: data.type_.dupe(),
            indexer_fallback: data.indexer_fallback.clone(),
        },
        OpInner::AnnotThisSpecializeT { type_: this, .. } => {
            ElabOpenMemoTag::ThisSpecialize { this: this.dupe() }
        }
        OpInner::AnnotSpecializeT(data) => ElabOpenMemoTag::Specialize {
            types: data.types.clone(),
            operation: data.operation.dupe(),
        },
        OpInner::AnnotImportTypeofT { name, .. } => {
            ElabOpenMemoTag::ImportTypeof { name: name.clone() }
        }
        OpInner::AnnotAssertExportIsTypeT { name, .. } => {
            ElabOpenMemoTag::AssertExportIsType { name: name.clone() }
        }
        OpInner::AnnotUseTTypeT { kind, .. } => ElabOpenMemoTag::UseTType { kind: *kind },
        OpInner::AnnotGetTypeFromNamespaceT(data) => ElabOpenMemoTag::GetTypeFromNamespace {
            name: data.prop_ref.1.clone(),
        },
        OpInner::AnnotGetElemT { key, .. } => ElabOpenMemoTag::GetElem { key: key.dupe() },
        OpInner::AnnotElemT {
            from_annot, source, ..
        } => ElabOpenMemoTag::Elem {
            from_annot: *from_annot,
            source: source.dupe(),
        },
        OpInner::AnnotObjKitT(data) => ElabOpenMemoTag::ObjKit {
            resolve_tool: data.resolve_tool.clone(),
            tool: data.tool.clone(),
        },
        OpInner::AnnotArithT(data) => ElabOpenMemoTag::Arith {
            flip: data.flip,
            rhs_t: data.rhs_t.dupe(),
            kind: data.kind.clone(),
        },
        OpInner::AnnotUnaryArithT { kind, .. } => ElabOpenMemoTag::UnaryArith { kind: *kind },
        OpInner::AnnotDeepReadOnlyT(data) => ElabOpenMemoTag::DeepReadOnly {
            loc: data.loc.clone(),
            dro_type: data.dro_type.clone(),
        },
        OpInner::AnnotToStringT { orig_t, .. } => ElabOpenMemoTag::ToString {
            orig_t: orig_t.clone(),
        },
        OpInner::AnnotObjRestT { keys, .. } => ElabOpenMemoTag::ObjRest { keys: keys.clone() },
        // `ConcretizeForImportsExports` applies an opaque closure: unkeyable.
        // The other concretize ops write to collectors: memoizing would skip
        // collection on re-demands. All keep old behavior.
        OpInner::AnnotConcretizeForImportsExports(..)
        | OpInner::AnnotConcretizeForInspection { .. }
        | OpInner::AnnotConcretizeForClassExtends { .. } => return None,
        OpInner::AnnotConcretizeForCJSExtractNamedExportsAndTypeExports(..) => {
            ElabOpenMemoTag::ConcretizeForCJS
        }
        OpInner::AnnotGetEnumT(..) => ElabOpenMemoTag::GetEnum,
        OpInner::AnnotGetStaticsT(..) => ElabOpenMemoTag::GetStatics,
        OpInner::AnnotObjTestProtoT(..) => ElabOpenMemoTag::ObjTestProto,
        OpInner::AnnotMixinT(..) => ElabOpenMemoTag::Mixin,
        OpInner::AnnotNotT(..) => ElabOpenMemoTag::Not,
        OpInner::AnnotObjKeyMirror(..) => ElabOpenMemoTag::ObjKeyMirror,
        OpInner::AnnotGetKeysT(..) => ElabOpenMemoTag::GetKeys,
        OpInner::AnnotGetKeysDictKeyT(..) => ElabOpenMemoTag::GetKeysDictKey,
        OpInner::AnnotGetValuesT(..) => ElabOpenMemoTag::GetValues,
        OpInner::AnnotFilterOptionalT(..) => ElabOpenMemoTag::FilterOptional,
    };
    Some(ElabOpenMemoKey { tag, id })
}

impl<'a> Context<'a> {
    pub fn take_cache_snapshot(&mut self) -> CacheSnapshot {
        let ccx = self.ccx_mut();
        let snapshot_subst_cache = ccx.subst_cache.dupe();
        ccx.eval_id_cache.push_level();
        ccx.id_cache.push_level();
        ccx.eval_repos_cache.push_level();
        ccx.fix_cache.push_level();
        ccx.this_specialize_cache.push_level();
        ccx.const_fold_cache.push_level();
        ccx.elab_open_memo.push_level();
        let constraint_cache_level_count = ccx.constraint_cache.level_count();
        ccx.constraint_cache.push_level();
        CacheSnapshot {
            constraint_cache_level_count,
            snapshot_subst_cache,
            snapshot_spread_cache: ccx.spread_cache.dupe(),
            snapshot_evaluated: ccx.sig_cx.evaluated.dupe(),
        }
    }

    pub fn restore_cache_snapshot(&mut self, snapshot: CacheSnapshot) {
        let CacheSnapshot {
            constraint_cache_level_count,
            snapshot_subst_cache,
            snapshot_spread_cache,
            snapshot_evaluated,
        } = snapshot;
        let ccx = self.ccx_mut();
        ccx.constraint_cache
            .truncate_to(constraint_cache_level_count);
        ccx.subst_cache = snapshot_subst_cache;
        ccx.eval_id_cache.pop_level();
        ccx.id_cache.pop_level();
        ccx.eval_repos_cache.pop_level();
        ccx.fix_cache.pop_level();
        ccx.this_specialize_cache.pop_level();
        ccx.const_fold_cache.pop_level();
        ccx.elab_open_memo.pop_level();
        ccx.spread_cache = snapshot_spread_cache;
        self.set_evaluated(snapshot_evaluated);
    }

    pub fn run_and_rolled_back_cache<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        let cache_snapshot = self.take_cache_snapshot();
        let result = f(self);
        self.restore_cache_snapshot(cache_snapshot);
        result
    }

    pub fn run_in_synthesis_mode<F, R>(
        &mut self,
        reset_forcing_state: bool,
        target_loc: Option<ALoc>,
        f: F,
    ) -> (bool, R)
    where
        F: FnOnce(&mut Self) -> R,
    {
        let old_typing_mode = self.inner().typing_mode.dupe();
        let old_synthesis_produced_uncacheable_result =
            self.ccx().synthesis_produced_uncacheable_result;
        let old_delayed_forcing_tvars = self.ccx().delayed_forcing_tvars.dupe();
        let old_post_component_tvar_forcing_states =
            self.ccx().post_component_tvar_forcing_states.dupe();
        self.ccx_mut().synthesis_produced_uncacheable_result = false;
        self.inner_mut().typing_mode = TypingMode::SynthesisMode { target_loc };
        let cache_snapshot = self.take_cache_snapshot();

        let result = f(self);

        let synthesis_produced_uncacheable_result =
            self.ccx().synthesis_produced_uncacheable_result;

        self.restore_cache_snapshot(cache_snapshot);
        self.inner_mut().typing_mode = old_typing_mode;
        self.ccx_mut().synthesis_produced_uncacheable_result =
            old_synthesis_produced_uncacheable_result;
        if reset_forcing_state {
            let ccx = self.ccx_mut();
            ccx.delayed_forcing_tvars = old_delayed_forcing_tvars;
            ccx.post_component_tvar_forcing_states = old_post_component_tvar_forcing_states;
        }

        (synthesis_produced_uncacheable_result, result)
    }

    pub fn run_in_synthesis_mode_with_errors<F, R>(
        &mut self,
        target_loc: Option<ALoc>,
        f: F,
    ) -> (bool, (R, ErrorSet))
    where
        F: FnOnce(&mut Self) -> R,
    {
        // Since this mode resets errors, we make sure to reset_forcing_state so that
        // we don't raise errors in a post-inference pass.
        self.run_in_synthesis_mode(true, target_loc, |cx| {
            let original_errors = cx.ccx().errors.dupe();
            cx.reset_errors(ErrorSet::new());
            let t = f(cx);
            let new_errors = cx.ccx().errors.dupe();
            cx.reset_errors(original_errors);
            (t, new_errors)
        })
    }

    /// Run a signature tvar's thunk as a normal check. The `flow_js` half of
    /// this — dropping the enclosing speculation and type-application
    /// expansion — is `FlowJsEnv::signature_tvar_env`.
    pub fn run_in_signature_tvar_env<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        let saved_typing_mode = self.inner().typing_mode.dupe();
        self.inner_mut().typing_mode = TypingMode::CheckingMode;

        let result = f(self);

        self.inner_mut().typing_mode = saved_typing_mode;

        result
    }

    /// Hint evaluation is an independent unit of type evaluation, so it runs
    /// with the caches rolled back afterwards. Its `flow_js` half — dropping
    /// the enclosing speculation and expansion — is
    /// `FlowJsEnv::signature_tvar_env` applied at the call site.
    pub fn run_in_hint_eval_mode<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        let old_typing_mode = self.inner().typing_mode.dupe();
        self.inner_mut().typing_mode = TypingMode::HintEvaluationMode;
        let cache_snapshot = self.take_cache_snapshot();

        let result = f(self);

        self.restore_cache_snapshot(cache_snapshot);
        self.inner_mut().typing_mode = old_typing_mode;

        result
    }

    pub fn test_prop_hit(&mut self, id: i32) {
        self.ccx_mut()
            .test_prop_hits_and_misses
            .insert(id, TestPropHitOrMiss::Hit);
    }

    pub fn test_prop_miss(
        &mut self,
        id: i32,
        name: Option<Name>,
        prop_reason_and_object: (Reason, Type),
        use_op: UseOp,
        suggestion: Option<FlowSmolStr>,
    ) {
        if !self.ccx().test_prop_hits_and_misses.contains_key(&id) {
            self.ccx_mut().test_prop_hits_and_misses.insert(
                id,
                TestPropHitOrMiss::Miss {
                    name,
                    prop_reason_and_object,
                    use_op,
                    suggestion,
                },
            );
        }
    }

    pub fn test_prop_get_never_hit(
        &self,
    ) -> Vec<(Option<Name>, (Reason, Type), UseOp, Option<FlowSmolStr>)> {
        self.ccx()
            .test_prop_hits_and_misses
            .values()
            .filter_map(|hit_or_miss| match hit_or_miss {
                TestPropHitOrMiss::Hit => None,
                TestPropHitOrMiss::Miss {
                    name,
                    prop_reason_and_object,
                    use_op,
                    suggestion,
                } => Some((
                    name.dupe(),
                    prop_reason_and_object.dupe(),
                    use_op.dupe(),
                    suggestion.dupe(),
                )),
            })
            .collect()
    }

    pub fn add_condition(
        &mut self,
        e: flow_parser::ast::expression::Expression<ALoc, (ALoc, Type)>,
    ) {
        self.ccx_mut().conditions.push(e);
    }

    pub fn get_all_conditions(
        &self,
    ) -> FlowVector<flow_parser::ast::expression::Expression<ALoc, (ALoc, Type)>> {
        self.ccx().conditions.dupe()
    }

    pub fn add_strict_comparison(
        &mut self,
        comp: (
            ALoc,
            (
                flow_parser::ast::expression::Expression<ALoc, (ALoc, Type)>,
                flow_parser::ast::expression::Expression<ALoc, (ALoc, Type)>,
            ),
        ),
    ) {
        self.ccx_mut().strict_comparisons.push(comp);
    }

    pub fn get_all_strict_comparisons(
        &self,
    ) -> FlowVector<(
        ALoc,
        (
            flow_parser::ast::expression::Expression<ALoc, (ALoc, Type)>,
            flow_parser::ast::expression::Expression<ALoc, (ALoc, Type)>,
        ),
    )> {
        self.ccx().strict_comparisons.dupe()
    }

    pub fn mark_optional_chain(
        &mut self,
        loc: ALoc,
        lhs_expression: ExpressionReferenceData<ALoc>,
        useful: bool,
    ) {
        self.ccx_mut()
            .optional_chains_useful
            .entry(loc)
            .and_modify(|(_, u)| {
                *u = *u || useful;
            })
            .or_insert((lhs_expression, useful));
    }

    pub fn unnecessary_optional_chains(&self) -> Vec<(ALoc, ExpressionReferenceData<ALoc>)> {
        let mut result = Vec::new();
        for (loc, (lhs_expression, useful)) in self.ccx().optional_chains_useful.iter() {
            if !useful {
                result.push((loc.dupe(), lhs_expression.dupe()));
            }
        }
        result
    }

    pub fn mark_maybe_unused_promise(&mut self, loc: ALoc, t: Type, async_: bool) {
        self.ccx_mut().maybe_unused_promises.push((loc, t, async_));
    }

    pub fn maybe_unused_promises(&self) -> FlowVector<(ALoc, Type, bool)> {
        self.ccx().maybe_unused_promises.dupe()
    }

    pub fn add_hint_eval_cache_entry(&mut self, id: i32, result: Option<Type>) {
        self.inner_mut().hint_eval_cache.insert(id, result);
    }

    // utils

    pub fn iter_props<F>(&self, id: type_::properties::Id, f: F)
    where
        F: Fn(&flow_common::reason::Name, &type_::Property),
    {
        self.find_props(id).iter().for_each(|(k, v)| f(k, v))
    }

    pub fn try_iter_props<F, E>(&mut self, id: type_::properties::Id, mut f: F) -> Result<(), E>
    where
        F: FnMut(&mut Self, &flow_common::reason::Name, &type_::Property) -> Result<(), E>,
    {
        for (k, v) in self.find_props(id).iter() {
            f(self, k, v)?;
        }
        Ok(())
    }

    pub fn fold_props<F, A>(&self, id: type_::properties::Id, f: F, init: A) -> A
    where
        F: Fn(&flow_common::reason::Name, &type_::Property, A) -> A,
    {
        self.find_props(id)
            .iter()
            .fold(init, |acc, (k, v)| f(k, v, acc))
    }

    pub fn has_prop(&self, id: type_::properties::Id, x: &flow_common::reason::Name) -> bool {
        self.ccx()
            .sig_cx
            .property_maps
            .get(&id)
            .ok_or(PropsNotFound(id))
            .unwrap()
            .contains_key(x)
    }

    pub fn get_prop(
        &self,
        id: type_::properties::Id,
        x: &flow_common::reason::Name,
    ) -> Option<type_::Property> {
        self.ccx()
            .sig_cx
            .property_maps
            .get(&id)
            .ok_or(PropsNotFound(id))
            .unwrap()
            .get(x)
            .duped()
    }

    // constructors

    pub fn make_aloc_id(&self, aloc: &ALoc) -> ALocId {
        let table = self.inner().aloc_table.dupe();
        ALocId::of_aloc(aloc, move || LazyCell::force(&*table).dupe())
    }

    pub fn make_generic_id(&self, name: SubstName, loc: &ALoc) -> GenericId {
        GenericId::make_bound_id(self.make_aloc_id(loc), name)
    }

    pub fn generate_property_map(
        &mut self,
        pmap: type_::properties::PropertiesMap,
    ) -> type_::properties::Id {
        let id = type_::properties::Id::generate_id();
        self.add_property_map(id.dupe(), pmap);
        id
    }

    pub fn make_source_property_map(
        &mut self,
        pmap: type_::properties::PropertiesMap,
        type_sig: bool,
        aloc: &ALoc,
    ) -> type_::properties::Id {
        // To prevent cases where we might compare a concrete and an abstract
        // aloc (like in a cycle) we abstractify all incoming alocs before adding
        // them to the map. The only exception is for library files, which have only
        // concrete definitions and by definition cannot appear in cycles.
        let aloc_id = self.make_aloc_id(aloc);
        let id = type_::properties::Id::of_aloc_id(type_sig, aloc_id);
        self.add_property_map(id.dupe(), pmap);
        id
    }

    pub fn make_call_prop(&mut self, t: Type) -> i32 {
        let id = flow_common::reason::mk_id() as i32;
        self.add_call_prop(id, t);
        id
    }

    pub fn make_export_map(&mut self, tmap: type_::exports::T) -> type_::exports::Id {
        let id = type_::exports::Id::mk_id();
        self.add_export_map(id.dupe(), tmap);
        id
    }

    pub fn make_source_poly_id(&self, type_sig: bool, aloc: &ALoc) -> type_::poly::Id {
        let aloc_id = self.make_aloc_id(aloc);
        type_::poly::Id::of_aloc_id(type_sig, aloc_id)
    }

    pub fn find_graph(&mut self, id: i32) -> type_::constraint::Constraints<Cx> {
        self.graph_mut().find_graph(id).unwrap().clone()
    }

    pub fn find_constraints(&mut self, id: i32) -> (i32, type_::constraint::Constraints<Cx>) {
        if self.is_inference_node(id) {
            let (root_id, root) = self.find_inference_root(id);
            return (root_id, root.constraints);
        }

        let file = self.file().dupe();
        let (id, c) = self.graph_mut().find_constraints(id).unwrap_or_else(|e| {
            panic!("find_constraints: TvarNotFound({}) in file {:?}", e.0, file,)
        });
        (id, c.clone())
    }

    /// The root of `id` and its constraints, like [Context::find_constraints] but without the
    /// path compression that needs a mutable context.
    pub fn constraints_of(&self, id: i32) -> (i32, type_::constraint::Constraints<Cx>) {
        let mut current = id;
        loop {
            let node = match self.ccx().inference_nodes.get(&current) {
                Some(node) => node,
                None => self.graph().get(&current).unwrap_or_else(|| {
                    panic!(
                        "constraints_of: TvarNotFound({}) in file {:?}",
                        current,
                        self.file()
                    )
                }),
            };
            match node {
                Node::Root(root) => return (current, root.constraints.clone()),
                Node::Goto { parent } => current = *parent,
            }
        }
    }

    pub fn set_constraints(&mut self, id: i32, constraints: type_::constraint::Constraints<Cx>) {
        let (root_id, _) = self.find_constraints(id);
        if self.is_inference_node(root_id) {
            self.set_inference_root_constraints(root_id, constraints);
        } else {
            self.set_root_constraints(root_id, constraints);
        }
    }

    pub fn find_constraints_pair(
        &mut self,
        id1: i32,
        id2: i32,
    ) -> (
        (i32, type_::constraint::Constraints<Cx>),
        (i32, type_::constraint::Constraints<Cx>),
    ) {
        if self.is_inference_node(id1) || self.is_inference_node(id2) {
            return (self.find_constraints(id1), self.find_constraints(id2));
        }

        let file = self.file().dupe();
        let graph = self.graph_mut();
        let (root_id1, c1) = graph.find_constraints(id1).unwrap_or_else(|e| {
            panic!(
                "find_constraints_pair: TvarNotFound({}) in file {:?}",
                e.0, file,
            )
        });
        let c1 = c1.clone();
        let (root_id2, c2) = graph.find_constraints(id2).unwrap_or_else(|e| {
            panic!(
                "find_constraints_pair: TvarNotFound({}) in file {:?}",
                e.0, file,
            )
        });
        ((root_id1, c1), (root_id2, c2.clone()))
    }

    /// Mutate the constraints of a tvar's root in-place, avoiding the
    /// clone-modify-writeback overhead of find_constraints + set_root_constraints.
    /// The closure receives (root_id, &mut Constraints).
    pub fn modify_constraints<R>(
        &mut self,
        id: i32,
        f: impl FnOnce(i32, &mut type_::constraint::Constraints<Cx>) -> R,
    ) -> R {
        let file = self.file().dupe();
        let (root_id, c) = self.graph_mut().find_constraints(id).unwrap_or_else(|e| {
            panic!(
                "modify_constraints: TvarNotFound({}) in file {:?}",
                e.0, file,
            )
        });
        f(root_id, c)
    }

    /// Read-only inspection of constraints without cloning.
    /// The closure receives (root_id, &Constraints).
    pub fn inspect_constraints<R>(
        &mut self,
        id: i32,
        f: impl FnOnce(i32, &type_::constraint::Constraints<Cx>) -> R,
    ) -> R {
        let file = self.file().dupe();
        let (root_id, c) = self.graph_mut().find_constraints(id).unwrap_or_else(|e| {
            panic!(
                "inspect_constraints: TvarNotFound({}) in file {:?}",
                e.0, file,
            )
        });
        f(root_id, c)
    }

    pub fn find_root(
        &mut self,
        id: i32,
    ) -> (
        i32,
        flow_utils_union_find::Root<type_::constraint::Constraints<Cx>>,
    ) {
        let file = self.file().dupe();
        let (id, root) = self
            .graph_mut()
            .find_root(id)
            .unwrap_or_else(|e| panic!("find_root: TvarNotFound({}) in file {:?}", e.0, file,));
        (id, root.clone())
    }

    pub fn set_root_constraints(
        &mut self,
        id: i32,
        constraints: type_::constraint::Constraints<Cx>,
    ) {
        let (_root_id, root) = self.graph_mut().find_root(id).unwrap();
        root.constraints = constraints;
    }

    pub fn set_root_rank(&mut self, id: i32, rank: i32) {
        let (_root_id, root) = self.graph_mut().find_root(id).unwrap();
        root.rank = rank;
    }

    pub fn find_root_id(&mut self, id: i32) -> i32 {
        let file = self.file().dupe();
        self.graph_mut()
            .find_root_id(id)
            .unwrap_or_else(|e| panic!("find_root_id: TvarNotFound({}) in file {:?}", e.0, file,))
    }

    pub fn on_cyclic_tvar_error(&mut self, reason: Reason) -> Type {
        let msg = ErrorMessage::ETrivialRecursiveDefinition(reason.loc().dupe());
        let error = error_of_msg(self.file().dupe(), msg);
        if self.is_verbose() {
            eprintln!("\nCyclic type: {:?}", reason);
        }
        self.add_error(error);
        any_t::error(reason)
    }

    pub fn force_fully_resolved_tvar(&mut self, s: &ForcingState<Cx>) -> Type {
        let len = self.ccx().active_tvar_forcing_states.len();
        self.ccx_mut().active_tvar_forcing_states.push(s.clone());
        let t = s.force(self, |cx, reason| cx.on_cyclic_tvar_error(reason.dupe()));
        self.ccx_mut().active_tvar_forcing_states.truncate(len);
        t
    }

    pub fn find_resolved(&mut self, t_in: &Type) -> Option<Type> {
        fn find_resolved_inner<'a>(
            cx: &mut Context<'a>,
            seen: &mut BTreeSet<i32>,
            t_in: &Type,
        ) -> Option<Type> {
            if let Some(node) = flow_typing_type::type_util::constraint_node_id(t_in) {
                let id = node.id();
                if seen.contains(&id) {
                    Some(t_in.dupe())
                } else {
                    use flow_typing_type::type_::constraint::Constraints;
                    match cx.find_constraints(id).1 {
                        Constraints::Resolved(t) => {
                            seen.insert(id);
                            find_resolved_inner(cx, seen, &t)
                        }
                        Constraints::FullyResolved(s) => {
                            seen.insert(id);
                            let forced = cx.force_fully_resolved_tvar(&s);
                            find_resolved_inner(cx, seen, &forced)
                        }
                        Constraints::Unresolved(_) => None,
                    }
                }
            } else {
                match &**t_in {
                    TypeInner::AnnotT(_, t, _) => find_resolved_inner(cx, seen, t),
                    _ => Some(t_in.dupe()),
                }
            }
        }

        let mut seen = BTreeSet::new();
        find_resolved_inner(self, &mut seen, t_in)
    }

    pub fn constraint_cache(&self) -> &type_::FlowSet<Cx> {
        &self.ccx().constraint_cache
    }

    pub fn constraint_cache_mut(&mut self) -> &mut type_::FlowSet<Cx> {
        &mut self.ccx_mut().constraint_cache
    }

    pub fn subst_cache(&self) -> &type_::SubstCacheMap<(Vec<SubstCacheErr>, Type)> {
        &self.ccx().subst_cache
    }

    pub fn subst_cache_mut(&mut self) -> &mut type_::SubstCacheMap<(Vec<SubstCacheErr>, Type)> {
        &mut self.ccx_mut().subst_cache
    }

    pub fn eval_id_cache(&self) -> &EvalIdCacheMap {
        &self.ccx().eval_id_cache
    }

    pub fn eval_id_cache_mut(&mut self) -> &mut EvalIdCacheMap {
        &mut self.ccx_mut().eval_id_cache
    }

    pub fn id_cache(&self) -> &IdCacheMap {
        &self.ccx().id_cache
    }

    pub fn id_cache_mut(&mut self) -> &mut IdCacheMap {
        &mut self.ccx_mut().id_cache
    }

    pub fn eval_repos_cache(&self) -> &EvalReposCacheMap {
        &self.ccx().eval_repos_cache
    }

    pub fn eval_repos_cache_mut(&mut self) -> &mut EvalReposCacheMap {
        &mut self.ccx_mut().eval_repos_cache
    }

    pub fn fix_cache(&self) -> &FixCacheMap {
        &self.ccx().fix_cache
    }

    pub fn fix_cache_mut(&mut self) -> &mut FixCacheMap {
        &mut self.ccx_mut().fix_cache
    }

    pub fn this_specialize_cache(&self) -> &ThisSpecializeCacheMap {
        &self.ccx().this_specialize_cache
    }

    pub fn this_specialize_cache_mut(&mut self) -> &mut ThisSpecializeCacheMap {
        &mut self.ccx_mut().this_specialize_cache
    }

    pub fn spread_cache(&self) -> &SpreadCache {
        &self.ccx().spread_cache
    }

    pub fn spread_cache_mut(&mut self) -> &mut SpreadCache {
        &mut self.ccx_mut().spread_cache
    }

    pub fn const_fold_cache(&self) -> &ConstFoldMap {
        &self.ccx().const_fold_cache
    }

    pub fn const_fold_cache_mut(&mut self) -> &mut ConstFoldMap {
        &mut self.ccx_mut().const_fold_cache
    }

    pub fn exhaustive_check(&self, loc: &ALoc) -> Option<(Vec<ALoc>, bool)> {
        self.ccx().exhaustive_checks.get(loc).cloned()
    }

    pub fn add_avar(&mut self, id: i32, constraint_: AConstraint<Cx>) {
        self.ccx_mut().annot_graph.insert(id, constraint_);
    }

    pub fn find_avar(&self, id: i32) -> AConstraint<Cx> {
        self.ccx()
            .annot_graph
            .get(&id)
            .expect("find_avar: id not found in annot_graph")
            .dupe()
    }

    pub fn find_avar_opt(&self, id: i32) -> Option<AConstraint<Cx>> {
        self.ccx().annot_graph.get(&id).duped()
    }

    pub fn elab_open_memo_get(&self, key: &ElabOpenMemoKey) -> Option<Type> {
        self.ccx().elab_open_memo.get(key).duped()
    }

    pub fn elab_open_memo_insert(&mut self, key: ElabOpenMemoKey, t: Type) {
        self.ccx_mut().elab_open_memo.insert(key, t);
    }

    pub fn remove_avar(&mut self, id: i32) {
        self.ccx_mut().annot_graph.remove(&id);
    }

    pub fn iter_annot_dependent_set<F>(&mut self, mut f: F, set: &FlowOrdSet<i32>)
    where
        F: FnMut(&mut Self, i32, &type_::aconstraint::Op<Cx>),
    {
        for &i in set {
            let constraints = self.find_avar(i);
            let op = constraints.to_annot_op_exn();
            f(self, i, op);
        }
    }

    /// A specialized callee recorded by the checker, which never runs inside a
    /// speculative branch (see `FlowJsEnv`), so it starts with no speculation
    /// state of its own.
    pub fn new_specialized_callee(&self) -> type_::SpecializedCallee {
        type_::SpecializedCallee {
            init_speculation_state: None,
            finalized: std::rc::Rc::new(std::cell::RefCell::new(std::collections::VecDeque::new())),
            speculative_candidates: std::rc::Rc::new(std::cell::RefCell::new(
                std::collections::VecDeque::new(),
            )),
            sig_help: std::rc::Rc::new(std::cell::RefCell::new(std::collections::VecDeque::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use flow_aloc::ALocTable;
    use flow_common::reason::VirtualReasonDesc;
    use flow_common::reason::locationless_reason;
    use flow_typing_type::type_::constraint::Constraints;

    use super::*;

    fn add_context(arena: &mut ContextArena, path: &str) -> ContextId {
        let file = FileKey::source_file_of_absolute(path);
        let aloc_table: LazyALocTable = Rc::new(LazyCell::new(Box::new({
            let file = file.dupe();
            move || Rc::new(ALocTable::empty(file))
        })));
        let ccx = arena.add_component();
        arena.add_context(
            ccx,
            Metadata::default(),
            file,
            Arc::default(),
            aloc_table,
            Rc::new(|_: &mut Context<'_>, _: &FlowImportSpecifier| ResolvedRequire::MissingModule),
            Rc::new(|_: &mut Context<'_>| Builtins::empty()),
            CheckBudget::new(None),
        )
    }

    #[test]
    fn direct_recursive_forcing_still_reports_a_cycle() {
        let mut arena = ContextArena::new();
        let cx_id = add_context(&mut arena, "/forcing_test.js");
        let mut cx = Context::new(ContextStore::new(vec![&mut arena]), cx_id);
        let reason = locationless_reason(VirtualReasonDesc::RNumber);
        let id = flow_common::reason::mk_id() as i32;
        // The state forces itself through the graph, which owns it.
        let active = ForcingState::of_lazy_t(reason, move |cx: &mut Context<'_>| {
            let Constraints::FullyResolved(state) = cx.find_graph(id) else {
                panic!("the tvar was added as fully resolved");
            };
            cx.force_fully_resolved_tvar(&state)
        });
        cx.add_tvar(
            id,
            Node::create_root(Constraints::FullyResolved(active.clone())),
        );

        cx.force_fully_resolved_tvar(&active);
        assert!(active.already_forced_with_cyclic_error());
        assert_eq!(cx.errors().len(), 1);
    }

    #[test]
    fn a_context_switches_to_the_contexts_of_its_store() {
        let mut dependencies = ContextArena::new();
        let dependency = add_context(&mut dependencies, "/dependency.js");
        let mut checked = ContextArena::new();
        let checked_id = add_context(&mut checked, "/checked.js");
        {
            let mut cx = Context::new(
                ContextStore::new(vec![&mut checked, &mut dependencies]),
                checked_id,
            );
            assert_eq!(cx.switch_to(dependency).file().as_str(), "/dependency.js");
            assert_eq!(cx.id(), checked_id, "dropping the switch switches back");
        }
        let cx = Context::new(ContextStore::new(vec![&mut dependencies]), dependency);
        assert!(
            !cx.has_context(checked_id),
            "a store does not see the contexts of arenas it does not have"
        );
    }

    #[test]
    fn annot_error_destination_belongs_to_the_checking_context() {
        let mut dependencies = ContextArena::new();
        let dependency = add_context(&mut dependencies, "/dependency.js");
        let mut first = ContextArena::new();
        let first_id = add_context(&mut first, "/first.js");
        {
            let mut cx = Context::new(
                ContextStore::new(vec![&mut first, &mut dependencies]),
                first_id,
            );
            cx.set_annot_error_dst(first_id);
            assert_eq!(
                cx.switch_to(dependency).annot_error_dst(),
                first_id,
                "a dependency reports to the file whose check switched to it"
            );
        }
        let mut second = ContextArena::new();
        let second_id = add_context(&mut second, "/second.js");
        let mut cx = Context::new(
            ContextStore::new(vec![&mut second, &mut dependencies]),
            second_id,
        );
        assert_eq!(
            cx.switch_to(dependency).annot_error_dst(),
            dependency,
            "a cached dependency does not keep reporting to an earlier file"
        );
        cx.set_annot_error_dst(second_id);
        assert_eq!(cx.switch_to(dependency).annot_error_dst(), second_id);
    }

    #[test]
    fn clearing_an_arena_invalidates_its_ids() {
        let mut arena = ContextArena::new();
        let id = add_context(&mut arena, "/a.js");
        arena.clear();
        let store = ContextStore::new(vec![&mut arena]);
        assert!(!store.has_context(id));
    }
}
