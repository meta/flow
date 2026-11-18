/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Environment types for the type normalizer.
//!
//! Port of `ty_normalizer_env.ml`

use std::cell::OnceCell;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocFuzzyMap;
use flow_aloc::ALocId;
use flow_common::subst_name::SubstName;
use flow_common_ty::ty_symbol::ALocImportedIdent;
use flow_common_ty::ty_symbol::ALocSymbol;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_parser::ast;
use flow_parser_utils::file_sig::FileSig;
use flow_typing_context::Context;
use flow_typing_type::type_::Destructor;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::eval;
use flow_typing_type::type_::poly;
use flow_utils_concurrency::job_error::JobError;

#[derive(Clone, Dupe, Default)]
pub enum EvaluateTypeDestructorsMode {
    #[default]
    EvaluateNone,
    EvaluateAll,
    EvaluateSome,
    /// Decide destructor by destructor, so that one normalization pass yields the
    /// single form a hover should show, rather than normalizing twice and picking
    /// between the results. See `should_evaluate_for_hover`.
    EvaluateForHover,
    EvaluateCustom(Rc<dyn Fn(&Destructor) -> bool>),
}

#[derive(Clone, Dupe)]
pub struct Options {
    /// If this flag is set to `true` then the normalizer will attempt to reuse the
    /// cached results of evaluated type-destructors. If this is set to `false`, then
    /// instead it will try to use:
    ///  - a potentially attendant type-alias annotation, or
    ///  - reuse the utility type that corresponds to this the specific type-destructor.
    ///
    /// Choosing 'false' will typically result in smaller produced types, which makes
    /// it a more appropriate option for codemods.
    pub evaluate_type_destructors: EvaluateTypeDestructorsMode,
    /// Expand the signatures of built-in functions, such as:
    /// Function.prototype.apply: (thisArg: any, argArray?: any): any
    pub expand_internal_types: bool,
    /// If true, expand enum declarations to include their members.
    pub expand_enum_members: bool,
    /// Consider all kinds of Bot and Any the same when simplifying types.
    ///
    /// The normalized type Ty.Bot may correspond to either the `Empty` type,
    /// or not have any lower-bounds. These types are not easy to
    /// normalize, but may still encode some constraint. When using normalized types
    /// for codemods we might want to know if there might be some constraints that we
    /// missing in the normalized type.
    ///
    /// Any can be due to an annotation or implicitly arising from inference.
    pub merge_bot_and_any_kinds: bool,
    /// Omits type params if they match the defaults, e.g:
    ///
    /// Given `type Foo<A, B = Baz>`, `Foo<Bar, Baz>` is reduced to `Foo<Bar>`
    ///
    /// WARNING: May be slow due to the structural equality checks that this necessitates.
    pub omit_targ_defaults_option: bool,
    /// Run an optimization pass that removes duplicates from unions and intersections.
    ///
    /// WARNING May be slow for large types
    pub optimize_types: bool,
    /// Debug
    pub verbose_normalizer: bool,
    /// Maximum depth of recursion
    pub max_depth: Option<u32>,
    /// Maximum number of nodes visited while normalizing a type.
    pub max_type_size: Option<usize>,
    /// In typed AST for type references, we store the type as if it's read under the
    /// value namespace. In some places, we might want to record the fact that it's
    /// a type-namespace read.
    pub toplevel_is_type_identifier_reference: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            evaluate_type_destructors: EvaluateTypeDestructorsMode::EvaluateNone,
            expand_internal_types: false,
            expand_enum_members: false,
            merge_bot_and_any_kinds: true,
            omit_targ_defaults_option: false,
            optimize_types: true,
            verbose_normalizer: false,
            max_depth: Some(40),
            max_type_size: None,
            toplevel_is_type_identifier_reference: false,
        }
    }
}

impl Options {
    pub fn default_for_codemod() -> Self {
        Self {
            expand_internal_types: false,
            expand_enum_members: false,
            evaluate_type_destructors: EvaluateTypeDestructorsMode::EvaluateSome,
            optimize_types: false,
            omit_targ_defaults_option: true,
            merge_bot_and_any_kinds: false,
            verbose_normalizer: false,
            max_depth: None,
            max_type_size: None,
            toplevel_is_type_identifier_reference: false,
        }
    }
}

/// Global environment that does not change during normalization
#[derive(Clone)]
pub struct Genv<'a, 'cx> {
    /// Typed AST of the current file
    pub typed_ast_opt: Option<&'a ast::Program<ALoc, (ALoc, Type)>>,
    /// The file_sig of the current file
    pub file_sig: Arc<FileSig>,
    /// In determining whether a symbol is Local, Imported, Remote, etc, it is
    /// useful to keep a map of imported names and the corresponding
    /// location available. We can then make this decision by comparing the
    /// source file with the current context's file information.
    pub imported_names: ImportedNames<'a, 'cx>,
    /// Normalization parameters
    pub options: Options,
    /// When set, the normalizer collects the body Type.t for each type alias it encounters.
    /// This is keyed by the type alias name string. Used by type-of-name to expand refs.
    pub ref_type_bodies: Option<Rc<RefCell<BTreeMap<String, Type>>>>,
    /// The polymorphic type a call instantiates and the call's type arguments, when
    /// the type being normalized is that polymorphic type. A computed type in its
    /// signature is evaluated with the type arguments substituted, as it is in the
    /// instantiated signature.
    pub instantiation: Option<(poly::Id, FlowOrdMap<SubstName, Type>)>,
}

pub type ImportedNamesMap = ALocFuzzyMap<ALocImportedIdent>;

/// The imported names of the environment's context, computed with the context the first time
/// they are asked for, and shared by the copies of the environment.
#[derive(Clone, Dupe)]
pub struct ImportedNames<'a, 'cx> {
    names: Rc<OnceCell<Result<ImportedNamesMap, JobError>>>,
    compute: Rc<dyn Fn(&mut Context<'cx>) -> Result<ImportedNamesMap, JobError> + 'a>,
}

impl<'a, 'cx> ImportedNames<'a, 'cx> {
    pub fn new(
        compute: impl Fn(&mut Context<'cx>) -> Result<ImportedNamesMap, JobError> + 'a,
    ) -> Self {
        ImportedNames {
            names: Rc::new(OnceCell::new()),
            compute: Rc::new(compute),
        }
    }

    pub fn get(&self, cx: &mut Context<'cx>) -> Result<&ImportedNamesMap, JobError> {
        self.names
            .get_or_init(|| (self.compute)(cx))
            .as_ref()
            .map_err(|e| e.dupe())
    }
}

pub type SymbolSet = BTreeSet<ALocSymbol>;

pub struct Env<'a, 'cx> {
    /// The context the normalized types belong to.
    pub cx: &'a mut Context<'cx>,
    /// Does not change. Set once in the beginning.
    pub genv: Genv<'a, 'cx>,
    pub infer_tparam_ids: Rc<Vec<ALocId>>,
    /// For debugging purposes mostly
    pub depth: u32,
    pub keep_only_namespace_name: bool,
    /// Detect recursive types
    pub seen_tvar_ids: FlowOrdSet<i32>,
    pub seen_eval_ids: FlowOrdSet<eval::Id>,
    pub omit_targ_defaults: bool,
}

impl<'a, 'cx> Env<'a, 'cx> {
    pub fn init(cx: &'a mut Context<'cx>, genv: Genv<'a, 'cx>) -> Self {
        let omit_targ_defaults = genv.options.omit_targ_defaults_option;
        Env {
            cx,
            genv,
            depth: 0,
            infer_tparam_ids: Rc::new(Vec::new()),
            keep_only_namespace_name: false,
            seen_tvar_ids: Self::empty_tvar_ids(),
            seen_eval_ids: Self::empty_eval_ids(),
            omit_targ_defaults,
        }
    }

    pub fn descend(&mut self) {
        self.depth += 1;
    }

    pub fn empty_tvar_ids() -> FlowOrdSet<i32> {
        thread_local! {
            static CACHED: FlowOrdSet<i32> = FlowOrdSet::new();
        }
        CACHED.with(|c| c.clone())
    }

    pub fn empty_eval_ids() -> FlowOrdSet<eval::Id> {
        thread_local! {
            static CACHED: FlowOrdSet<eval::Id> = FlowOrdSet::new();
        }
        CACHED.with(|c| c.clone())
    }

    pub fn imported_names(&mut self) -> Result<&ImportedNamesMap, JobError> {
        self.genv.imported_names.get(self.cx)
    }

    pub fn expand_internal_types(&self) -> bool {
        self.genv.options.expand_internal_types
    }

    pub fn expand_enum_members(&self) -> bool {
        self.genv.options.expand_enum_members
    }

    pub fn evaluate_type_destructors(&self) -> &EvaluateTypeDestructorsMode {
        &self.genv.options.evaluate_type_destructors
    }

    pub fn omit_targ_defaults(&self) -> bool {
        self.omit_targ_defaults
    }

    pub fn optimize_types(&self) -> bool {
        self.genv.options.optimize_types
    }

    pub fn max_depth(&self) -> Option<u32> {
        self.genv.options.max_depth
    }

    pub fn max_type_size(&self) -> Option<usize> {
        self.genv.options.max_type_size
    }

    pub fn merge_bot_and_any_kinds(&self) -> bool {
        self.genv.options.merge_bot_and_any_kinds
    }

    pub fn verbose(&self) -> bool {
        self.genv.options.verbose_normalizer
    }

    pub fn toplevel_is_type_identifier_reference(&self) -> bool {
        self.genv.options.toplevel_is_type_identifier_reference
    }
}
