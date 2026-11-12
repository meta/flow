/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Port of `insert_type_utils.rs`

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_common_ty::ty;
use flow_common_ty::ty::ALocTy;
use flow_common_ty::ty_symbol::Symbol;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::loc::Loc;
use flow_parser::loc_sig::LocSig;

pub type SymbolMap<V> = BTreeMap<Symbol<ALoc>, V>;

pub type SymbolSet = BTreeSet<Symbol<ALoc>>;

pub fn is_react_file_key(file_key: &flow_parser::file_key::FileKey, is_lib_file: bool) -> bool {
    is_lib_file
        && std::path::Path::new(file_key.as_str())
            .file_name()
            .is_some_and(|f| f == "react.js")
}

pub fn is_react_redux_file_key(
    file_key: &flow_parser::file_key::FileKey,
    is_lib_file: bool,
) -> bool {
    let basename = std::path::Path::new(file_key.as_str())
        .file_name()
        .map_or("", |f| f.to_str().unwrap_or(""));
    is_lib_file && basename.contains("react-redux")
}

pub fn is_react_loc(loc: &ALoc, is_lib_file: bool) -> bool {
    match loc.source() {
        Some(f) => is_react_file_key(f, is_lib_file),
        _ => false,
    }
}

pub fn is_react_redux_loc(loc: &ALoc, is_lib_file: bool) -> bool {
    match loc.source() {
        Some(f) => is_react_redux_file_key(f, is_lib_file),
        _ => false,
    }
}

const WIDTH: usize = 45;

pub fn string_of_row(indent: usize, name: &str, i: i64) -> String {
    let len = name.len();
    let padding = WIDTH.saturating_sub(len + indent + 7);
    format!(
        "{:indent$}{name}:{:padding$}{i:6}",
        "",
        "",
        indent = indent,
        name = name,
        padding = padding,
        i = i,
    )
}

pub fn print_section(name: &str) -> String {
    format!("{name}:")
}

pub mod debug {
    #[derive(Debug, Clone, Copy)]
    pub enum NodeKind {
        Expr,
        Prop,
    }

    #[derive(Debug, Clone, Copy)]
    pub enum T {
        AddAnnotation(NodeKind),
    }

    pub fn serialize_node_kind(kind: NodeKind) -> &'static str {
        match kind {
            NodeKind::Expr => "Expr",
            NodeKind::Prop => "Prop",
        }
    }

    pub fn serialize(t: &T) -> String {
        match t {
            T::AddAnnotation(k) => format!("Add_annotation {}", serialize_node_kind(*k)),
        }
    }
}

pub mod info {
    #[derive(Debug, Clone, Copy)]
    pub enum T {
        DefaultAny,
    }

    pub fn serialize(t: &T) -> &'static str {
        match t {
            T::DefaultAny => "Default_any",
        }
    }
}

pub mod warning {
    use super::*;

    #[derive(Debug, Clone)]
    pub enum Kind {
        SkippingArrowFunction,
        LargeTypeAdded(i64),
        LocationUnhandled,
        EmptyNoUpper,
        EmptySomeKnownUpper,
    }

    #[derive(Debug, Clone, Default)]
    pub struct Counts {
        pub skipping_arrow_function: i64,
        pub large_type_added: i64,
        pub location_unhandled: i64,
        pub empty_no_upper: i64,
        pub empty_some_known_upper: i64,
    }

    pub fn empty() -> Counts {
        Counts::default()
    }

    pub fn combine(c1: &Counts, c2: &Counts) -> Counts {
        Counts {
            skipping_arrow_function: c1.skipping_arrow_function + c2.skipping_arrow_function,
            large_type_added: c1.large_type_added + c2.large_type_added,
            location_unhandled: c1.location_unhandled + c2.location_unhandled,
            empty_no_upper: c1.empty_no_upper + c2.empty_no_upper,
            empty_some_known_upper: c1.empty_some_known_upper + c2.empty_some_known_upper,
        }
    }

    pub fn serialize(kind: &Kind) -> String {
        match kind {
            Kind::SkippingArrowFunction => "Skipping_arrow_function".to_string(),
            Kind::LargeTypeAdded(n) => format!("Large_type_added {n}"),
            Kind::LocationUnhandled => "Location_unhandled".to_string(),
            Kind::EmptyNoUpper => "Empty_NoUpper".to_string(),
            Kind::EmptySomeKnownUpper => "Empty_SomeKnownUpper".to_string(),
        }
    }

    pub fn report(c: &Counts) -> String {
        let rows = [
            string_of_row(2, "Skipping arrow function", c.skipping_arrow_function),
            string_of_row(2, "Large type added", c.large_type_added),
            string_of_row(2, "Location unhandled", c.location_unhandled),
            string_of_row(2, "Empty NoUpper", c.empty_no_upper),
            string_of_row(2, "Empty SomeKnownUpper", c.empty_some_known_upper),
        ];
        rows.join("\n")
    }

    pub fn add(c: &mut Counts, kind: &Kind) {
        match kind {
            Kind::SkippingArrowFunction => c.skipping_arrow_function += 1,
            Kind::LargeTypeAdded(_) => c.large_type_added += 1,
            Kind::LocationUnhandled => c.location_unhandled += 1,
            Kind::EmptyNoUpper => c.empty_no_upper += 1,
            Kind::EmptySomeKnownUpper => c.empty_some_known_upper += 1,
        }
    }
}

pub mod error {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ValidationError {
        TooBig {
            size_limit: usize,
            size: Option<usize>,
        },
        Anonymous(Loc),
        AnyUnsound(ty::UnsoundnessKind),
        Recursive,
        ReactElementConfigFunArg,
        EmptyTypeDestructorTriggerT(Loc),
        EmptySomeUnknownUpper(String),
        SymbolKey,
    }

    pub fn serialize_validation_error(error: &ValidationError) -> String {
        match error {
            ValidationError::TooBig { .. } => "TooBig".to_string(),
            ValidationError::Anonymous(loc) => {
                format!("Anonymous (def: {})", loc.to_string_no_source())
            }
            ValidationError::AnyUnsound(kind) => format!("Any_Unsound {kind:?}"),
            ValidationError::Recursive => "Recursive".to_string(),
            ValidationError::ReactElementConfigFunArg => "ReactElementConfigFunArg".to_string(),
            ValidationError::EmptyTypeDestructorTriggerT(loc) => {
                format!(
                    "Empty_TypeDestructorTriggerT (def: {})",
                    loc.to_string_no_source()
                )
            }
            ValidationError::EmptySomeUnknownUpper(u) => {
                format!("Empty_SomeUnknownUpper (use: {u})")
            }
            ValidationError::SymbolKey => "Symbol_key".to_string(),
        }
    }

    #[derive(Debug, Clone)]
    pub enum ImportError {
        LocSourceNone,
        IndeterminateModuleType,
        NoMatchingExport(String, Loc),
    }

    #[derive(Debug, Clone, Default)]
    pub struct ImportErrorCounts {
        pub loc_source_none: i64,
        pub indeterminate_module_type: i64,
        pub no_matching_export: i64,
    }

    #[derive(Debug, Clone)]
    pub enum Kind {
        MissingAnnotationOrNormalizerError,
        ValidationError(ValidationError),
        ImportError(ImportError),
        UnsupportedErrorKind,
    }

    #[derive(Debug, Clone, Default)]
    pub struct Counts {
        pub missing_annotation_or_normalizer_error: i64,
        pub validation_error: i64,
        pub import_error: ImportErrorCounts,
        pub unsupported_error_kind: i64,
    }

    pub fn empty() -> Counts {
        Counts::default()
    }

    pub fn combine_import_errors(
        c1: &ImportErrorCounts,
        c2: &ImportErrorCounts,
    ) -> ImportErrorCounts {
        ImportErrorCounts {
            loc_source_none: c1.loc_source_none + c2.loc_source_none,
            indeterminate_module_type: c1.indeterminate_module_type + c2.indeterminate_module_type,
            no_matching_export: c1.no_matching_export + c2.no_matching_export,
        }
    }

    pub fn combine(c1: &Counts, c2: &Counts) -> Counts {
        Counts {
            missing_annotation_or_normalizer_error: c1.missing_annotation_or_normalizer_error
                + c2.missing_annotation_or_normalizer_error,
            validation_error: c1.validation_error + c2.validation_error,
            import_error: combine_import_errors(&c1.import_error, &c2.import_error),
            unsupported_error_kind: c1.unsupported_error_kind + c2.unsupported_error_kind,
        }
    }

    pub fn serialize_import_error(error: &ImportError) -> String {
        match error {
            ImportError::LocSourceNone => "Loc_source_none".to_string(),
            ImportError::IndeterminateModuleType => "Indeterminate_module_type".to_string(),
            ImportError::NoMatchingExport(x, loc) => {
                format!("No_matching_export {x} {}", loc.debug_to_string(true))
            }
        }
    }

    pub fn serialize(kind: &Kind) -> String {
        match kind {
            Kind::MissingAnnotationOrNormalizerError => {
                "Missing_annotation_or_normalizer_error".to_string()
            }
            Kind::ValidationError(e) => {
                format!("Validation_error {}", serialize_validation_error(e))
            }
            Kind::ImportError(e) => format!("Import_error {}", serialize_import_error(e)),
            Kind::UnsupportedErrorKind => "Unsupported_error_kind".to_string(),
        }
    }

    pub fn report(c: &Counts) -> String {
        let rows = [
            string_of_row(
                2,
                "Missing annot./normalizer error",
                c.missing_annotation_or_normalizer_error,
            ),
            string_of_row(2, "Validation Error", c.validation_error),
            "  Import Error:".to_string(),
            string_of_row(4, "Loc source none", c.import_error.loc_source_none),
            string_of_row(
                4,
                "Indeterminate module type",
                c.import_error.indeterminate_module_type,
            ),
            string_of_row(4, "No matching export", c.import_error.no_matching_export),
            string_of_row(2, "Unsupported error kind", c.unsupported_error_kind),
        ];
        rows.join("\n")
    }

    pub fn add_import_error(c: &mut ImportErrorCounts, error: &ImportError) {
        match error {
            ImportError::LocSourceNone => c.loc_source_none += 1,
            ImportError::IndeterminateModuleType => c.indeterminate_module_type += 1,
            ImportError::NoMatchingExport(_, _) => c.no_matching_export += 1,
        }
    }

    pub fn add(c: &mut Counts, kind: &Kind) {
        match kind {
            Kind::MissingAnnotationOrNormalizerError => {
                c.missing_annotation_or_normalizer_error += 1;
            }
            Kind::ValidationError(_) => c.validation_error += 1,
            Kind::ImportError(e) => add_import_error(&mut c.import_error, e),
            Kind::UnsupportedErrorKind => c.unsupported_error_kind += 1,
        }
    }
}

pub trait BaseStats: Clone + Default {
    fn empty() -> Self;
    fn combine(a: &Self, b: &Self) -> Self;
    fn serialize(&self) -> Vec<String>;
    fn report(&self) -> Vec<String>;
}

#[derive(Debug, Clone, Default)]
pub struct UnitStats;

impl BaseStats for UnitStats {
    fn empty() -> Self {
        UnitStats
    }

    fn combine(_a: &Self, _b: &Self) -> Self {
        UnitStats
    }

    fn serialize(&self) -> Vec<String> {
        vec![]
    }

    fn report(&self) -> Vec<String> {
        vec![]
    }
}

#[derive(Debug, Clone)]
pub struct Stats<Extra: BaseStats> {
    pub number_of_annotations_added: i64,
    pub total_size_of_annotations: i64,
    pub extra: Extra,
}

impl<Extra: BaseStats> Default for Stats<Extra> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<Extra: BaseStats> Stats<Extra> {
    pub fn empty() -> Self {
        Stats {
            number_of_annotations_added: 0,
            total_size_of_annotations: 0,
            extra: Extra::empty(),
        }
    }

    pub fn combine(c1: &Self, c2: &Self) -> Self {
        Stats {
            number_of_annotations_added: c1.number_of_annotations_added
                + c2.number_of_annotations_added,
            total_size_of_annotations: c1.total_size_of_annotations + c2.total_size_of_annotations,
            extra: Extra::combine(&c1.extra, &c2.extra),
        }
    }

    pub fn serialize(&self) -> String {
        let mut stats = vec![
            format!("annotations_added: {}", self.number_of_annotations_added),
            format!(
                "total_size_of_annotations: {}",
                self.total_size_of_annotations
            ),
        ];
        stats.extend(self.extra.serialize());
        format!("({})", stats.join(", "))
    }

    pub fn report(&self) -> String {
        let mut rows = vec![
            string_of_row(
                2,
                "Number of annotations added",
                self.number_of_annotations_added,
            ),
            string_of_row(
                2,
                "Total size of annotations",
                self.total_size_of_annotations,
            ),
        ];
        rows.extend(self.extra.report());
        rows.join("\n")
    }
}

#[derive(Debug, Clone)]
pub struct UntypedAcc<Extra: BaseStats> {
    pub changed_set: BTreeSet<flow_parser::file_key::FileKey>,
    pub stats: Extra,
}

impl<Extra: BaseStats> UntypedAcc<Extra> {
    pub fn empty() -> Self {
        UntypedAcc {
            changed_set: BTreeSet::new(),
            stats: Extra::empty(),
        }
    }

    pub fn update_stats(&mut self, stats: Extra) {
        self.stats = stats;
    }

    pub fn combine(c1: &Self, c2: &Self) -> Self {
        let mut changed_set = c1.changed_set.clone();
        changed_set.extend(c2.changed_set.iter().duped());
        UntypedAcc {
            changed_set,
            stats: Extra::combine(&c1.stats, &c2.stats),
        }
    }

    pub fn debug(loc: &Loc, x: &debug::T) {
        flow_hh_logger::debug!(
            "{} {}",
            debug::serialize(x),
            flow_common::reason::string_of_loc(None, loc)
        );
    }

    pub fn info(loc: &Loc, x: &info::T) {
        flow_hh_logger::info!(
            "{} {}",
            info::serialize(x),
            flow_common::reason::string_of_loc(None, loc)
        );
    }

    pub fn report(&self) -> String {
        let mut rows = vec![
            print_section("Stats"),
            string_of_row(2, "Files changed", self.changed_set.len() as i64),
        ];
        rows.extend(self.stats.report());
        rows.join("\n")
    }
}

#[derive(Debug, Clone)]
pub struct Acc<Extra: BaseStats> {
    pub changed_set: BTreeSet<flow_parser::file_key::FileKey>,
    pub stats: Stats<Extra>,
    pub errors: error::Counts,
    pub warnings: warning::Counts,
}

impl<Extra: BaseStats> Acc<Extra> {
    pub fn empty() -> Self {
        Acc {
            changed_set: BTreeSet::new(),
            stats: Stats::empty(),
            errors: error::empty(),
            warnings: warning::empty(),
        }
    }

    pub fn combine(c1: &Self, c2: &Self) -> Self {
        let mut changed_set = c1.changed_set.clone();
        changed_set.extend(c2.changed_set.iter().duped());
        Acc {
            changed_set,
            stats: Stats::combine(&c1.stats, &c2.stats),
            errors: error::combine(&c1.errors, &c2.errors),
            warnings: warning::combine(&c1.warnings, &c2.warnings),
        }
    }

    pub fn debug(loc: &Loc, x: &debug::T) {
        flow_hh_logger::debug!(
            "{} {}",
            debug::serialize(x),
            flow_common::reason::string_of_loc(None, loc)
        );
    }

    pub fn info(loc: &Loc, x: &info::T) {
        flow_hh_logger::info!(
            "{} {}",
            info::serialize(x),
            flow_common::reason::string_of_loc(None, loc)
        );
    }

    pub fn warn(&mut self, loc: &Loc, x: &warning::Kind) {
        flow_hh_logger::warn!(
            "{} {}",
            warning::serialize(x),
            flow_common::reason::string_of_loc(None, loc)
        );
        warning::add(&mut self.warnings, x);
    }

    pub fn error(&mut self, loc: &Loc, x: &error::Kind) {
        flow_hh_logger::error!(
            "{} {}",
            error::serialize(x),
            flow_common::reason::string_of_loc(None, loc)
        );
        error::add(&mut self.errors, x);
    }

    pub fn report(&self) -> String {
        let sep = "";
        let rows = vec![
            print_section("Stats"),
            string_of_row(2, "Files changed", self.changed_set.len() as i64),
            self.stats.report(),
            sep.to_string(),
            print_section("Errors"),
            error::report(&self.errors),
            sep.to_string(),
            print_section("Warnings"),
            warning::report(&self.warnings),
        ];
        rows.join("\n")
    }
}

pub mod builtins {
    use super::*;

    pub fn flowfixme_ty(
        lint_severities: &flow_lint_settings::lint_settings::LintSettings<
            flow_lint_settings::severity::Severity,
        >,
    ) -> ALocTy {
        if *lint_severities.get_value(flow_lint_settings::lints::LintKind::UnclearType)
            == flow_lint_settings::severity::Severity::Err
        {
            Arc::new(ty::Ty::Generic(Box::new((
                flow_common_ty::ty_symbol::builtin_symbol(FlowSmolStr::new_inline("$FlowFixMe")),
                ty::GenKind::TypeAliasKind,
                None,
            ))))
        } else {
            Arc::new(ty::Ty::Any(ty::AnyKind::Annotated(ALoc::none())))
        }
    }

    pub fn empty() -> ALocTy {
        Arc::new(ty::Ty::Bot(ty::BotKind::EmptyType))
    }

    pub fn flowfixme_ty_default() -> ALocTy {
        Arc::new(ty::Ty::Any(ty::AnyKind::Annotated(ALoc::none())))
    }

    pub fn flowfixme_ast(
        lint_severities: &flow_lint_settings::lint_settings::LintSettings<
            flow_lint_settings::severity::Severity,
        >,
    ) -> flow_parser::ast::types::Type<Loc, Loc> {
        let ty = flowfixme_ty(lint_severities);
        flow_common_ty::ty_serializer::type_(&ty)
    }
}

pub mod validator {
    pub use super::error::ValidationError;
    use super::*;

    pub struct TypeValidatorVisitor<'a> {
        pub loc_of_aloc: &'a dyn Fn(&ALoc) -> Loc,
        pub env: &'a RefCell<Vec<ValidationError>>,
    }

    impl flow_common_ty::ty_ancestors::TyEndoBase<(), ALoc> for TypeValidatorVisitor<'_> {
        fn on_name(
            &mut self,
            _env: &(),
            name: flow_common::reason::Name,
        ) -> flow_common::reason::Name {
            // A `unique symbol` key has no annotation spelling. The serializer
            // drops such a property, which would describe a *different* type
            // (and, against an exact object, one the annotated value no longer
            // satisfies), so reject the type rather than insert a wrong one.
            if name.is_symbol() {
                self.env.borrow_mut().push(ValidationError::SymbolKey);
            }
            name
        }
    }

    impl flow_common_ty::ty::TyEndoTy<ALoc, ()> for TypeValidatorVisitor<'_> {
        fn on_t(&mut self, env: &(), t: ALocTy) -> ALocTy {
            match t.as_ref() {
                ty::Ty::Any(ty::AnyKind::Recursive) => {
                    self.env.borrow_mut().push(ValidationError::Recursive);
                    ty::explicit_any()
                }
                ty::Ty::Bot(ty::BotKind::NoLowerWithUpper(
                    ty::UpperBoundKind::SomeUnknownUpper(u),
                )) => {
                    self.env
                        .borrow_mut()
                        .push(ValidationError::EmptySomeUnknownUpper(u.clone()));
                    ty::explicit_any()
                }
                ty::Ty::Any(ty::AnyKind::Unsound(kind))
                    if matches!(
                        kind,
                        ty::UnsoundnessKind::Constructor
                            | ty::UnsoundnessKind::DummyStatic
                            | ty::UnsoundnessKind::Exports
                            | ty::UnsoundnessKind::InferenceHooks
                            | ty::UnsoundnessKind::InstanceOfRefinement
                            | ty::UnsoundnessKind::Merged
                            | ty::UnsoundnessKind::ResolveSpread
                            | ty::UnsoundnessKind::Unchecked
                            | ty::UnsoundnessKind::Unimplemented
                            | ty::UnsoundnessKind::UnresolvedType
                    ) =>
                {
                    self.env
                        .borrow_mut()
                        .push(ValidationError::AnyUnsound(*kind));
                    ty::explicit_any()
                }
                ty::Ty::Utility(ty::Utility::ReactElementConfigType(inner))
                    if matches!(inner.as_ref(), ty::Ty::Fun(_)) =>
                {
                    self.env
                        .borrow_mut()
                        .push(ValidationError::ReactElementConfigFunArg);
                    ty::explicit_any()
                }
                ty::Ty::TypeOf(box (ty::BuiltinOrSymbol::TSymbol(symbol), _)) => {
                    if symbol.sym_anonymous {
                        self.env
                            .borrow_mut()
                            .push(ValidationError::Anonymous((self.loc_of_aloc)(
                                &symbol.sym_def_loc,
                            )));
                        ty::explicit_any()
                    } else {
                        self.default_on_t(env, t)
                    }
                }
                ty::Ty::Generic(box (symbol, _, _)) => {
                    if symbol.sym_anonymous {
                        self.env
                            .borrow_mut()
                            .push(ValidationError::Anonymous((self.loc_of_aloc)(
                                &symbol.sym_def_loc,
                            )));
                        ty::explicit_any()
                    } else {
                        self.default_on_t(env, t)
                    }
                }
                ty::Ty::Fun(f) => {
                    let modified = Arc::new(ty::Ty::Fun(Box::new(ty::FunT {
                        fun_static: Arc::new(ty::Ty::Top),
                        ..(**f).clone()
                    })));
                    self.default_on_t(env, modified)
                }
                _ => self.default_on_t(env, t),
            }
        }
    }

    pub const VALIDATE_TYPE_TOO_BIG_MAX: usize = 1000;

    pub fn validate_type(
        size_limit: usize,
        loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
        t: ALocTy,
    ) -> (ALocTy, Vec<ValidationError>) {
        match flow_common_ty::ty_utils::size_of_type(Some(size_limit), &t) {
            None => {
                let max = VALIDATE_TYPE_TOO_BIG_MAX;
                let error = ValidationError::TooBig {
                    size_limit,
                    size: flow_common_ty::ty_utils::size_of_type(Some(max), &t),
                };
                (t, vec![error])
            }
            Some(_) => {
                let env = RefCell::new(Vec::new());
                let mut visitor = TypeValidatorVisitor {
                    loc_of_aloc,
                    env: &env,
                };
                let t = <TypeValidatorVisitor as flow_common_ty::ty::TyEndoTy<ALoc, ()>>::on_t(
                    &mut visitor,
                    &(),
                    t,
                );
                (t, env.into_inner())
            }
        }
    }
}

pub struct MapperTypePrintingHardcodedFixes;

impl MapperTypePrintingHardcodedFixes {
    fn normalize_function(
        &self,
        ff: &flow_parser::ast::types::Function<Loc, Loc>,
    ) -> flow_parser::ast::types::Function<Loc, Loc> {
        use flow_parser::ast::types::function;
        use flow_parser::ast_utils;
        let params_inner = &ff.params;
        let mut normalized_params_rev = Vec::new();
        let mut c: usize = 0;
        for param in params_inner.params.iter() {
            match &param.param {
                function::ParamKind::Anonymous(annot) => {
                    let name = ast_utils::ident_of_source(
                        None,
                        param.loc.dupe(),
                        flow_data_structure_wrapper::smol_str::FlowSmolStr::new(format!("_{c}")),
                    );
                    let normalized_param = function::Param {
                        loc: param.loc.dupe(),
                        param: function::ParamKind::Labeled {
                            name,
                            annot: annot.clone(),
                            optional: false,
                        },
                    };
                    normalized_params_rev.push(normalized_param);
                    c += 1;
                }
                function::ParamKind::Destructuring(pattern) => {
                    let optional = ast_utils::pattern_optional(pattern);
                    let (_, annot, _) = ast_utils::function_type_param_parts(&param.param);
                    let name = ast_utils::ident_of_source(
                        None,
                        param.loc.dupe(),
                        flow_data_structure_wrapper::smol_str::FlowSmolStr::new(format!("_{c}")),
                    );
                    let normalized_param = function::Param {
                        loc: param.loc.dupe(),
                        param: function::ParamKind::Labeled {
                            name,
                            annot: annot.clone(),
                            optional,
                        },
                    };
                    normalized_params_rev.push(normalized_param);
                    c += 1;
                }
                _ => {
                    normalized_params_rev.push(param.clone());
                    c += 1;
                }
            }
        }
        let normalized_params = normalized_params_rev;
        flow_parser::ast::types::Function {
            params: function::Params {
                params: normalized_params.into(),
                ..params_inner.clone()
            },
            ..ff.clone()
        }
    }

    fn type_generic_normalize(
        &mut self,
        t: &flow_parser::ast::types::Type<Loc, Loc>,
    ) -> flow_parser::ast::types::Type<Loc, Loc> {
        let normalized = match &**t {
            flow_parser::ast::types::TypeInner::Function { loc, inner } => {
                let nf = self.normalize_function(inner);
                flow_parser::ast::types::Type::new(flow_parser::ast::types::TypeInner::Function {
                    loc: loc.dupe(),
                    inner: Arc::new(nf),
                })
            }
            _ => t.dupe(),
        };
        flow_parser::ast_visitor::map_type_default(self, &normalized)
    }

    // In Rust, this delegates to map_type_default via AstVisitor.
    pub fn type_(
        &mut self,
        t: &flow_parser::ast::types::Type<Loc, Loc>,
    ) -> flow_parser::ast::types::Type<Loc, Loc> {
        flow_parser::ast_visitor::map_type_default(self, t)
    }
}

// AstVisitor implementation for MapperTypePrintingHardcodedFixes.
// In OCaml this class inherits Flow_ast_mapper.mapper and overrides type_args.
impl flow_parser::ast_visitor::AstVisitor<'_, Loc> for MapperTypePrintingHardcodedFixes {
    fn normalize_loc(loc: &Loc) -> &Loc {
        loc
    }

    fn normalize_type(type_: &Loc) -> &Loc {
        type_
    }

    fn map_type_args(
        &mut self,
        targs: &flow_parser::ast::types::TypeArgs<Loc, Loc>,
    ) -> flow_parser::ast::types::TypeArgs<Loc, Loc> {
        let ts_prime: Arc<[_]> = targs
            .arguments
            .iter()
            .map(|t| self.type_generic_normalize(t))
            .collect();
        flow_parser::ast::types::TypeArgs {
            arguments: ts_prime,
            ..targs.clone()
        }
    }
}

pub fn patch_up_type_ast(
    type_: &flow_parser::ast::types::Type<Loc, Loc>,
) -> flow_parser::ast::types::Type<Loc, Loc> {
    MapperTypePrintingHardcodedFixes.type_(type_)
}

// If a React library symbol was imported by the user, it is imported with the same mechanism we
// import other Remote symbols. Otherwise, we refer to these names as 'React.NAME'.
pub struct PatchUpReactMapper;

impl flow_common_ty::ty_ancestors::TyEndoBase<Loc, ALoc> for PatchUpReactMapper {}

impl flow_common_ty::ty::TyEndoTy<ALoc, Loc> for PatchUpReactMapper {
    fn on_t(&mut self, loc: &Loc, t: ALocTy) -> ALocTy {
        match t.as_ref() {
            ty::Ty::Generic(box (symbol, kind, args_opt)) => {
                if let Some(new_symbol) = flow_common_ty::ty_utils::patch_up_react_symbol(symbol) {
                    let args_opt = args_opt.as_ref().map(|args| {
                        let mapped: Arc<[ALocTy]> =
                            args.iter().map(|arg| self.on_t(loc, arg.dupe())).collect();
                        mapped
                    });
                    Arc::new(ty::Ty::Generic(Box::new((new_symbol, *kind, args_opt))))
                } else {
                    self.default_on_t(loc, t)
                }
            }
            _ => self.default_on_t(loc, t),
        }
    }
}

impl PatchUpReactMapper {
    // This method explicitly delegates to super, which is the default endo_ty traversal.
    pub fn on_prop(&mut self, loc: &Loc, prop: ty::Prop<ALoc>) -> ty::Prop<ALoc> {
        // on_prop is the default trait method in TyEndoTy (not overridden), so calling it
        // directly performs the recursive traversal.
        <Self as flow_common_ty::ty::TyEndoTy<ALoc, Loc>>::on_prop(self, loc, prop)
    }
}

pub fn reverse_append_all<T>(lists: Vec<Vec<T>>) -> Vec<T> {
    // Builds rev(last) ++ ... ++ rev(first), i.e. the reverse of all lists concatenated.
    let mut result = Vec::new();
    for mut list in lists {
        list.reverse();
        // Prepend the reversed list to the result.
        list.append(&mut result);
        result = list;
    }
    result
}

pub struct PartitionAcc {
    pub bools: Vec<ALocTy>,
    pub nums: Vec<ALocTy>,
    pub strings: Vec<ALocTy>,
    pub others: Vec<ALocTy>,
}

pub struct StylizeTyMapper;

impl flow_common_ty::ty_ancestors::TyEndoBase<Loc, ALoc> for StylizeTyMapper {}

impl flow_common_ty::ty::TyEndoTy<ALoc, Loc> for StylizeTyMapper {
    fn on_t(&mut self, loc: &Loc, t: ALocTy) -> ALocTy {
        match t.as_ref() {
            // Union case: dispatch to on_union (stylize_ty_mapper's override)
            ty::Ty::Union(from_bounds, _ty1, _ty2, _tys) => {
                self.on_union(loc, t.dupe(), *from_bounds)
            }
            // React name fixup (inherited from patch_up_react_mapper)
            ty::Ty::Generic(box (symbol, kind, args_opt)) => {
                if let Some(new_symbol) = flow_common_ty::ty_utils::patch_up_react_symbol(symbol) {
                    let args_opt = args_opt.as_ref().map(|args| {
                        let mapped: Arc<[ALocTy]> =
                            args.iter().map(|arg| self.on_t(loc, arg.dupe())).collect();
                        mapped
                    });
                    Arc::new(ty::Ty::Generic(Box::new((new_symbol, *kind, args_opt))))
                } else {
                    self.default_on_t(loc, t)
                }
            }
            _ => self.default_on_t(loc, t),
        }
    }
}

impl StylizeTyMapper {
    pub fn new() -> Self {
        StylizeTyMapper
    }

    fn on_union(&mut self, loc: &Loc, t: ALocTy, from_bounds: bool) -> ALocTy {
        let filter_union = |a: PartitionAcc, t: &ALocTy| -> PartitionAcc {
            match (t.as_ref(), &a) {
                // The general type is already in the partition: ignore the element.
                (ty::Ty::Bool | ty::Ty::BoolLit(_), PartitionAcc { bools, .. })
                    if bools.len() == 1 && matches!(bools[0].as_ref(), ty::Ty::Bool) =>
                {
                    a
                }
                (ty::Ty::Num | ty::Ty::NumLit(_), PartitionAcc { nums, .. })
                    if nums.len() == 1 && matches!(nums[0].as_ref(), ty::Ty::Num) =>
                {
                    a
                }
                (ty::Ty::Str | ty::Ty::StrLit(_), PartitionAcc { strings, .. })
                    if strings.len() == 1 && matches!(strings[0].as_ref(), ty::Ty::Str) =>
                {
                    a
                }
                (ty::Ty::Bool, _) => PartitionAcc {
                    bools: vec![t.dupe()],
                    ..a
                },
                (ty::Ty::Num, _) => PartitionAcc {
                    nums: vec![t.dupe()],
                    ..a
                },
                (ty::Ty::Str, _) => PartitionAcc {
                    strings: vec![t.dupe()],
                    ..a
                },
                (ty::Ty::BoolLit(true), PartitionAcc { bools, .. })
                    if bools.len() == 1 && matches!(bools[0].as_ref(), ty::Ty::BoolLit(false)) =>
                {
                    PartitionAcc {
                        bools: vec![Arc::new(ty::Ty::Bool)],
                        ..a
                    }
                }
                (ty::Ty::BoolLit(false), PartitionAcc { bools, .. })
                    if bools.len() == 1 && matches!(bools[0].as_ref(), ty::Ty::BoolLit(true)) =>
                {
                    PartitionAcc {
                        bools: vec![Arc::new(ty::Ty::Bool)],
                        ..a
                    }
                }
                (ty::Ty::BoolLit(_), _) => {
                    let mut bools = a.bools;
                    bools.push(t.dupe());
                    PartitionAcc { bools, ..a }
                }
                (ty::Ty::NumLit(_), _) => {
                    let mut nums = a.nums;
                    nums.push(t.dupe());
                    PartitionAcc { nums, ..a }
                }
                (ty::Ty::StrLit(_), _) => {
                    let mut strings = a.strings;
                    strings.push(t.dupe());
                    PartitionAcc { strings, ..a }
                }
                _ => {
                    let mut others = a.others;
                    others.push(t.dupe());
                    PartitionAcc { others, ..a }
                }
            }
        };
        let empty = PartitionAcc {
            bools: vec![],
            nums: vec![],
            strings: vec![],
            others: vec![],
        };
        let members = ty::bk_union(&t);
        let PartitionAcc {
            bools,
            nums,
            strings,
            others,
        } = members.iter().fold(empty, filter_union);
        let all: Vec<_> = bools
            .into_iter()
            .chain(nums)
            .chain(strings)
            .chain(others)
            .collect();
        match all.as_slice() {
            [] => panic!("Impossible! this only removes elements when others are added/present"),
            // Since StylizeTyMapper's on_t already includes that logic, and t is not a Union here,
            // calling self.on_t will go through the React fixup path and then default_on_t.
            [t] => <Self as flow_common_ty::ty::TyEndoTy<ALoc, Loc>>::on_t(self, loc, t.dupe()),
            // self.default_on_t dispatches Union to on_t for each element, achieving the same effect.
            [t1, t2, ts @ ..] => {
                let union = Arc::new(ty::Ty::Union(
                    from_bounds,
                    t1.dupe(),
                    t2.dupe(),
                    ts.iter().duped().collect(),
                ));
                <Self as flow_common_ty::ty::TyEndoTy<ALoc, Loc>>::default_on_t(self, loc, union)
            }
        }
    }
}

// Returns true if the location given a zero width location.
pub fn is_point(loc: &Loc) -> bool {
    loc.start == loc.end
}

pub mod graphql {
    use flow_parser::ast;
    use flow_parser::ast::expression;
    use flow_parser::ast::statement;
    use flow_parser::ast::types;

    use super::*;

    fn visit_object_property_type(
        defs: &BTreeMap<Loc, (ALoc, String, flow_common_ty::ty_symbol::ImportMode)>,
        tgt: &Loc,
        opt_chain: bool,
        ty_: ALocTy,
        p: &types::object::NormalProperty<Loc, Loc>,
    ) -> Option<(ALocTy, bool)> {
        let key = &p.key;
        let value = &p.value;
        let optional = p.optional;
        let name = match key {
            expression::object::Key::Identifier(ident) => Some(ident.name.dupe()),
            expression::object::Key::StringLiteral((_, sl)) => Some(sl.value.dupe()),
            _ => None,
        };
        match name {
            Some(name) => match value {
                types::object::PropertyValue::Init(Some(t)) => {
                    let optional = opt_chain || optional;
                    let ty_prime = Arc::new(ty::Ty::IndexedAccess {
                        _object: ty_,
                        index: Arc::new(ty::Ty::StrLit(name)),
                        optional,
                    });
                    visit_type(defs, tgt, optional, ty_prime, t)
                }
                types::object::PropertyValue::Init(None)
                | types::object::PropertyValue::Get(_, _)
                | types::object::PropertyValue::Set(_, _) => None,
            },
            None => None,
        }
    }

    fn visit_object_type(
        defs: &BTreeMap<Loc, (ALoc, String, flow_common_ty::ty_symbol::ImportMode)>,
        tgt: &Loc,
        opt_chain: bool,
        ty_: ALocTy,
        ot: &types::Object<Loc, Loc>,
    ) -> Option<(ALocTy, bool)> {
        ot.properties.iter().find_map(|p| match p {
            types::object::Property::NormalProperty(np) => {
                visit_object_property_type(defs, tgt, opt_chain, ty_.dupe(), np)
            }
            _ => None,
        })
    }

    fn visit_readonlyarray(
        defs: &BTreeMap<Loc, (ALoc, String, flow_common_ty::ty_symbol::ImportMode)>,
        tgt: &Loc,
        opt_chain: bool,
        ty_: ALocTy,
        t_prime: &types::Type<Loc, Loc>,
    ) -> Option<(ALocTy, bool)> {
        let ty_prime = Arc::new(ty::Ty::IndexedAccess {
            _object: ty_,
            index: Arc::new(ty::Ty::NumLit("0".to_string())),
            optional: opt_chain,
        });
        visit_type(defs, tgt, opt_chain, ty_prime, t_prime)
    }

    fn visit_type(
        defs: &BTreeMap<Loc, (ALoc, String, flow_common_ty::ty_symbol::ImportMode)>,
        tgt: &Loc,
        opt_chain: bool,
        ty_: ALocTy,
        t: &types::Type<Loc, Loc>,
    ) -> Option<(ALocTy, bool)> {
        match &**t {
            types::TypeInner::Object { loc: oloc, inner } => {
                if tgt == oloc {
                    Some((ty_, opt_chain))
                } else {
                    visit_object_type(defs, tgt, opt_chain, ty_, inner)
                }
            }
            types::TypeInner::Nullable {
                inner: nullable, ..
            } => match &*nullable.argument {
                types::TypeInner::Object { loc: oloc, inner } => {
                    if tgt == oloc {
                        Some((ty_, true))
                    } else {
                        visit_object_type(defs, tgt, true, ty_, inner)
                    }
                }
                types::TypeInner::Generic { inner: generic, .. }
                    if matches!(
                        &generic.id,
                        types::generic::Identifier::Unqualified(ident)
                        if matches!(ident.name.as_str(), "$ReadOnlyArray" | "ReadonlyArray")
                    ) && matches!(
                        &generic.targs,
                        Some(targs) if targs.arguments.len() == 1
                    ) =>
                {
                    let targs = generic.targs.as_ref().unwrap();
                    visit_readonlyarray(defs, tgt, true, ty_, &targs.arguments[0])
                }
                _ => None,
            },
            types::TypeInner::Generic { inner: generic, .. }
                if matches!(
                    &generic.id,
                    types::generic::Identifier::Unqualified(ident)
                    if matches!(ident.name.as_str(), "$ReadOnlyArray" | "ReadonlyArray")
                ) && matches!(
                    &generic.targs,
                    Some(targs) if targs.arguments.len() == 1
                ) =>
            {
                let targs = generic.targs.as_ref().unwrap();
                visit_readonlyarray(defs, tgt, opt_chain, ty_, &targs.arguments[0])
            }
            _ => None,
        }
    }

    fn visit_type_alias(
        defs: &BTreeMap<Loc, (ALoc, String, flow_common_ty::ty_symbol::ImportMode)>,
        tgt: &Loc,
        type_alias: &statement::TypeAlias<Loc, Loc>,
    ) -> Option<ALocTy> {
        if type_alias.tparams.is_some() {
            return None;
        }
        let id_loc = type_alias.id.loc.dupe();
        let name = type_alias.id.name.dupe();
        let (name, sym_provenance) = match defs.get(&id_loc) {
            Some((_, local_name, _)) => (
                FlowSmolStr::new(local_name.as_str()),
                flow_common_ty::ty_symbol::Provenance::Local,
            ),
            None => (
                name.dupe(),
                flow_common_ty::ty_symbol::Provenance::Remote(
                    flow_common_ty::ty_symbol::RemoteInfo { imported_as: None },
                ),
            ),
        };
        let symbol = Symbol {
            sym_provenance,
            sym_def_loc: ALoc::of_loc(id_loc),
            sym_name: name,
            sym_anonymous: false,
        };
        let ty_ = Arc::new(ty::Ty::Generic(Box::new((
            symbol,
            ty::GenKind::TypeAliasKind,
            None,
        ))));
        let (ty_result, opt_chain) = visit_type(defs, tgt, false, ty_, &type_alias.right)?;
        if opt_chain {
            Some(Arc::new(ty::Ty::Utility(ty::Utility::NonMaybeType(
                ty_result,
            ))))
        } else {
            Some(ty_result)
        }
    }

    fn visit_declaration(
        defs: &BTreeMap<Loc, (ALoc, String, flow_common_ty::ty_symbol::ImportMode)>,
        tgt: &Loc,
        decl: &statement::Statement<Loc, Loc>,
    ) -> Option<ALocTy> {
        match &**decl {
            statement::StatementInner::TypeAlias { inner, .. } => {
                visit_type_alias(defs, tgt, inner)
            }
            _ => None,
        }
    }

    fn visit_statement(
        defs: &BTreeMap<Loc, (ALoc, String, flow_common_ty::ty_symbol::ImportMode)>,
        tgt: &Loc,
        stmt: &statement::Statement<Loc, Loc>,
    ) -> Option<ALocTy> {
        match &**stmt {
            statement::StatementInner::ExportNamedDeclaration { inner, .. } => {
                if inner.export_kind == statement::ExportKind::ExportType {
                    if let Some(ref decl) = inner.declaration {
                        return visit_declaration(defs, tgt, decl);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn visit_program(
        defs: &BTreeMap<Loc, (ALoc, String, flow_common_ty::ty_symbol::ImportMode)>,
        tgt: &Loc,
        prog: &ast::Program<Loc, Loc>,
    ) -> Option<ALocTy> {
        prog.statements
            .iter()
            .find_map(|stmt| visit_statement(defs, tgt, stmt))
    }

    fn get_imported_ident(
        cx: &mut flow_typing_context::Context,
        loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
        item: (
            flow_data_structure_wrapper::smol_str::FlowSmolStr,
            ALoc,
            flow_common_ty::ty_symbol::ImportMode,
            flow_typing_type::type_::Type,
        ),
    ) -> (Loc, (ALoc, String, flow_common_ty::ty_symbol::ImportMode)) {
        let (local_name, loc, import_mode, t) = item;
        let t = match flow_typing_ty_normalizer::normalizer::lookahead::peek(cx, &t) {
            flow_typing_ty_normalizer::normalizer::lookahead::Lookahead::LowerBounds(
                ref bounds,
            ) if bounds.len() == 1 => bounds[0].clone(),
            _ => t,
        };
        let def_loc = flow_typing_type::type_util::def_loc_of_t(&t);
        (
            loc_of_aloc(def_loc),
            (loc, local_name.to_string(), import_mode),
        )
    }

    pub fn extract_graphql_fragment(
        cx: &mut flow_typing_context::Context,
        loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
        get_ast_from_heap: &dyn Fn(
            &flow_parser::file_key::FileKey,
        ) -> Option<flow_parser::ast::Program<Loc, Loc>>,
        file_sig: &flow_parser_utils::file_sig::FileSig,
        typed_ast: &flow_parser::ast::Program<ALoc, (ALoc, flow_typing_type::type_::Type)>,
        tgt_aloc: ALoc,
    ) -> Option<ALocTy> {
        let graphql_file = tgt_aloc.source()?;
        let graphql_ast = get_ast_from_heap(graphql_file)?;
        // The typed_ast branch (Some) only invokes add_bind_ident_from_typed_ast which
        // does not call import_named_specifier_type — so cancel cannot surface here.
        // Convert to None on the (unreachable) cancel branch; this matches the OCaml
        // behavior where this caller never sees Worker_should_cancel.
        let import_types = match flow_typing::ty_normalizer_imports::extract_types(
            cx,
            file_sig,
            Some(typed_ast),
        ) {
            Ok(t) => t,
            Err(_canceled) => return None,
        };
        let defs: BTreeMap<Loc, (ALoc, String, flow_common_ty::ty_symbol::ImportMode)> =
            import_types
                .into_iter()
                .map(|item| get_imported_ident(cx, loc_of_aloc, item))
                .collect();
        let tgt_loc = loc_of_aloc(&tgt_aloc);
        let r = visit_program(&defs, &tgt_loc, &graphql_ast);
        if r.is_none() {
            flow_hh_logger::info!(
                "Failed to extract GraphQL type fragment {}",
                flow_common::reason::string_of_loc(None, &tgt_loc)
            );
        }
        r
    }
}

pub struct TypeNormalizationHardcodedFixesMapper<'a, 'cx> {
    pub cx: &'a mut flow_typing_context::Context<'cx>,
    pub loc_of_aloc: &'a dyn Fn(&ALoc) -> Loc,
    pub get_ast_from_heap:
        &'a dyn Fn(&flow_parser::file_key::FileKey) -> Option<flow_parser::ast::Program<Loc, Loc>>,
    pub file_sig: &'a flow_parser_utils::file_sig::FileSig,
    pub typed_ast: &'a flow_parser::ast::Program<ALoc, (ALoc, flow_typing_type::type_::Type)>,
    pub lint_severities:
        &'a flow_lint_settings::lint_settings::LintSettings<flow_lint_settings::severity::Severity>,
    pub allow_dollar_flowfixme: bool,
    pub generalize_maybe: bool,
    pub generalize_react_mixed_element: bool,
    pub add_warning: &'a dyn Fn(Loc, warning::Kind),
    sanitized_any: ALocTy,
}

impl<'a, 'cx> TypeNormalizationHardcodedFixesMapper<'a, 'cx> {
    pub fn new(
        cx: &'a mut flow_typing_context::Context<'cx>,
        loc_of_aloc: &'a dyn Fn(&ALoc) -> Loc,
        get_ast_from_heap: &'a dyn Fn(
            &flow_parser::file_key::FileKey,
        ) -> Option<flow_parser::ast::Program<Loc, Loc>>,
        file_sig: &'a flow_parser_utils::file_sig::FileSig,
        typed_ast: &'a flow_parser::ast::Program<ALoc, (ALoc, flow_typing_type::type_::Type)>,
        lint_severities: &'a flow_lint_settings::lint_settings::LintSettings<
            flow_lint_settings::severity::Severity,
        >,
        allow_dollar_flowfixme: bool,
        generalize_maybe: bool,
        generalize_react_mixed_element: bool,
        add_warning: &'a dyn Fn(Loc, warning::Kind),
    ) -> Self {
        let sanitized_any = if allow_dollar_flowfixme {
            builtins::flowfixme_ty(lint_severities)
        } else {
            builtins::flowfixme_ty_default()
        };
        TypeNormalizationHardcodedFixesMapper {
            cx,
            loc_of_aloc,
            get_ast_from_heap,
            file_sig,
            typed_ast,
            lint_severities,
            allow_dollar_flowfixme,
            generalize_maybe,
            generalize_react_mixed_element,
            add_warning,
            sanitized_any,
        }
    }

    fn super_on_t(&mut self, env: &Loc, t: ALocTy) -> ALocTy {
        match t.as_ref() {
            ty::Ty::Generic(box (symbol, kind, args_opt)) => {
                if let Some(new_symbol) = flow_common_ty::ty_utils::patch_up_react_symbol(symbol) {
                    let args_opt = args_opt.as_ref().map(|args| {
                        let mapped: Arc<[ALocTy]> =
                            args.iter().map(|arg| self.on_t(env, arg.dupe())).collect();
                        mapped
                    });
                    Arc::new(ty::Ty::Generic(Box::new((new_symbol, *kind, args_opt))))
                } else {
                    <Self as flow_common_ty::ty::TyEndoTy<ALoc, Loc>>::default_on_t(self, env, t)
                }
            }
            _ => <Self as flow_common_ty::ty::TyEndoTy<ALoc, Loc>>::default_on_t(self, env, t),
        }
    }

    pub fn on_union(
        &mut self,
        env: &Loc,
        from_bounds: bool,
        ty1: ALocTy,
        ty2: ALocTy,
        tys: &[ALocTy],
    ) -> ALocTy {
        let frozen_metadata = self.cx.metadata().frozen.dupe();
        // Move `any`/dynamic members to the front of the union as a single sanitized `any`.
        // This is a heuristic that helps union resolution later on.
        let (ty1, ty2, tys) = match (ty1.as_ref(), ty2.as_ref()) {
            (ty::Ty::Any(_), _) => (self.sanitized_any.dupe(), ty2.dupe(), tys.to_vec()),
            (_, ty::Ty::Any(_)) => (self.sanitized_any.dupe(), ty1.dupe(), tys.to_vec()),
            _ => {
                if tys.iter().any(|t| ty::is_dynamic(t)) {
                    let tys_prime: Vec<_> =
                        tys.iter().filter(|t| !ty::is_dynamic(t)).duped().collect();
                    if tys_prime.len() == tys.len() {
                        (ty1.dupe(), ty2.dupe(), tys.to_vec())
                    } else {
                        let mut new_tys = vec![ty2.dupe()];
                        new_tys.extend(tys_prime);
                        (self.sanitized_any.dupe(), ty1.dupe(), new_tys)
                    }
                } else {
                    (ty1.dupe(), ty2.dupe(), tys.to_vec())
                }
            }
        };

        let mut ts: Vec<ALocTy> = vec![ty1.dupe(), ty2.dupe()];
        ts.extend(tys.iter().duped());

        match ts.len() {
            0 => {
                let mut all = vec![ty1, ty2];
                all.extend(tys.iter().duped());
                let u = ty::mk_union(from_bounds, all).unwrap_or_else(ty::explicit_any);
                self.super_on_t(env, u)
            }
            1 => self.on_t(env, ts.into_iter().next().unwrap()),
            _ => {
                let ts: Vec<ALocTy> = ts.into_iter().map(|t| self.on_t(env, t)).collect();

                let has_fbt = ts.iter().any(|t| {
                    matches!(t.as_ref(),
                        ty::Ty::Generic(box (symbol, _, None))
                        if symbol.sym_name == "Fbt"
                           && matches!(&symbol.sym_provenance,
                                       flow_common_ty::ty_symbol::Provenance::Library(_))
                    )
                });
                let ts = if has_fbt && from_bounds {
                    ts.into_iter()
                        .filter(|t| !matches!(t.as_ref(), ty::Ty::Str))
                        .collect()
                } else {
                    ts
                };

                let ts = if self.generalize_maybe && from_bounds && ts.len() > 1 {
                    let has_null = ts.iter().any(|t| matches!(t.as_ref(), ty::Ty::Null));
                    let has_void = ts
                        .iter()
                        .any(|t| matches!(t.as_ref(), ty::Ty::Void | ty::Ty::Undefined));
                    if has_null && !has_void {
                        let mut new_ts = vec![Arc::new(ty::Ty::Void)];
                        new_ts.extend(ts);
                        new_ts
                    } else if has_void && !has_null {
                        let mut new_ts = vec![Arc::new(ty::Ty::Null)];
                        new_ts.extend(ts);
                        new_ts
                    } else {
                        ts
                    }
                } else {
                    ts
                };

                //    We should remove it from the union.
                let ts = {
                    let ts_without_array_empty: Vec<_> = ts
                        .iter()
                        .filter(|t| {
                            !matches!(
                                t.as_ref(),
                                ty::Ty::Arr(arr)
                                if matches!(arr.arr_elt_t.as_ref(), ty::Ty::Bot(_) | ty::Ty::Any(_))
                            )
                        })
                        .duped()
                        .collect();
                    if ts_without_array_empty.is_empty() {
                        ts
                    } else {
                        ts_without_array_empty
                    }
                };

                let ts = {
                    let react_element_def_loc = ts.iter().find_map(|t| match t.as_ref() {
                        ty::Ty::Generic(box (symbol, _, _))
                            if matches!(
                                Some(symbol.sym_name.as_str()),
                                Some(
                                    "ExactReactElement_DEPRECATED"
                                        | "MixedElement"
                                        | "React.MixedElement"
                                )
                            ) && matches!(
                                &symbol.sym_provenance,
                                flow_common_ty::ty_symbol::Provenance::Library(_)
                            ) && is_react_loc(&symbol.sym_def_loc, true) =>
                        {
                            Some(symbol.sym_def_loc.dupe())
                        }
                        _ => None,
                    });
                    match react_element_def_loc {
                        Some(react_element_def_loc)
                            if from_bounds && self.generalize_react_mixed_element =>
                        {
                            let ts: Vec<_> = ts
                                .into_iter()
                                .filter(|t| match t.as_ref() {
                                    ty::Ty::Generic(box (symbol, _, Some(_)))
                                        if matches!(
                                            Some(symbol.sym_name.as_str()),
                                            Some(
                                                "ExactReactElement_DEPRECATED"
                                                    | "MixedElement"
                                                    | "React.MixedElement"
                                            )
                                        ) && matches!(
                                            &symbol.sym_provenance,
                                            flow_common_ty::ty_symbol::Provenance::Library(_)
                                        ) =>
                                    {
                                        !is_react_loc(&symbol.sym_def_loc, true)
                                    }
                                    ty::Ty::Generic(box (symbol, _, None))
                                        if symbol.sym_name == "Fbt"
                                            && matches!(
                                                &symbol.sym_provenance,
                                                flow_common_ty::ty_symbol::Provenance::Library(_)
                                            )
                                            && frozen_metadata
                                                .facebook_fbt
                                                .as_deref()
                                                .is_some() =>
                                    {
                                        false
                                    }
                                    _ => true,
                                })
                                .collect();
                            let react_mixed_element = Arc::new(ty::Ty::Generic(Box::new((
                                Symbol {
                                    sym_name: FlowSmolStr::new_inline("React.MixedElement"),
                                    sym_provenance: flow_common_ty::ty_symbol::Provenance::Library(
                                        flow_common_ty::ty_symbol::RemoteInfo { imported_as: None },
                                    ),
                                    sym_def_loc: react_element_def_loc,
                                    sym_anonymous: false,
                                },
                                ty::GenKind::TypeAliasKind,
                                None,
                            ))));
                            let mut result = vec![react_mixed_element];
                            result.extend(ts);
                            result
                        }
                        _ => ts,
                    }
                };

                let has_react_node = ts.iter().any(|t| {
                    matches!(t.as_ref(),
                        ty::Ty::Generic(box (symbol, _, _))
                        if matches!(symbol.sym_name.as_str(), "Node" | "React.Node")
                           && matches!(&symbol.sym_provenance,
                                       flow_common_ty::ty_symbol::Provenance::Library(_))
                           && is_react_loc(&symbol.sym_def_loc, true)
                    )
                });
                let ts = if has_react_node && from_bounds {
                    ts.into_iter()
                        .filter(|t| match t.as_ref() {
                            ty::Ty::Generic(box (symbol, _, Some(_)))
                                if matches!(
                                    Some(symbol.sym_name.as_str()),
                                    Some(
                                        "ExactReactElement_DEPRECATED"
                                            | "MixedElement"
                                            | "React.MixedElement"
                                    )
                                ) && matches!(
                                    &symbol.sym_provenance,
                                    flow_common_ty::ty_symbol::Provenance::Library(_)
                                ) =>
                            {
                                !is_react_loc(&symbol.sym_def_loc, true)
                            }
                            ty::Ty::Generic(box (symbol, _, None))
                                if symbol.sym_name == "Fbt"
                                    && matches!(
                                        &symbol.sym_provenance,
                                        flow_common_ty::ty_symbol::Provenance::Library(_)
                                    )
                                    && frozen_metadata.facebook_fbt.as_deref().is_some() =>
                            {
                                false
                            }
                            _ => true,
                        })
                        .collect()
                } else {
                    ts
                };

                match ts.as_slice() {
                    [] => {
                        let mut all = vec![ty1, ty2];
                        all.extend(tys.iter().duped());
                        ty::mk_union(from_bounds, all).unwrap_or_else(ty::explicit_any)
                    }
                    [t] => t.dupe(),
                    _ => ty::mk_union(from_bounds, ts).unwrap_or_else(ty::explicit_any),
                }
            }
        }
    }

    pub fn on_t(&mut self, env: &Loc, t: ALocTy) -> ALocTy {
        let frozen_metadata = self.cx.metadata().frozen.dupe();
        let loc = env.dupe();
        match t.as_ref() {
            ty::Ty::Bot(ty::BotKind::NoLowerWithUpper(ty::UpperBoundKind::NoUpper)) => {
                (self.add_warning)(loc, warning::Kind::EmptyNoUpper);
                self.sanitized_any.dupe()
            }
            ty::Ty::Bot(ty::BotKind::EmptyType) => t,
            ty::Ty::Bot(ty::BotKind::NoLowerWithUpper(ty::UpperBoundKind::SomeKnownUpper(ub))) => {
                (self.add_warning)(loc, warning::Kind::EmptySomeKnownUpper);
                self.on_t(env, ub.dupe())
            }
            ty::Ty::Generic(box (symbol, kind, Some(args)))
                if symbol.sym_name == "ExactReactElement_DEPRECATED"
                    && matches!(
                        &symbol.sym_provenance,
                        flow_common_ty::ty_symbol::Provenance::Library(_)
                    )
                    && is_react_loc(&symbol.sym_def_loc, true)
                    && args.len() == 1
                    && matches!(args[0].as_ref(), ty::Ty::Str | ty::Ty::StrLit(_)) =>
            {
                let new_symbol = Symbol {
                    sym_name: FlowSmolStr::new_inline("React.MixedElement"),
                    ..symbol.clone()
                };
                self.on_t(
                    env,
                    Arc::new(ty::Ty::Generic(Box::new((new_symbol, *kind, None)))),
                )
            }
            ty::Ty::Generic(box (symbol, kind, Some(_)))
                if symbol.sym_name == "ExactReactElement_DEPRECATED"
                    && matches!(
                        &symbol.sym_provenance,
                        flow_common_ty::ty_symbol::Provenance::Library(_)
                    )
                    && is_react_loc(&symbol.sym_def_loc, true) =>
            {
                let name = if self.generalize_react_mixed_element {
                    "React.MixedElement"
                } else {
                    "React.Node"
                };
                let new_symbol = Symbol {
                    sym_name: FlowSmolStr::new(name),
                    ..symbol.clone()
                };
                self.on_t(
                    env,
                    Arc::new(ty::Ty::Generic(Box::new((new_symbol, *kind, None)))),
                )
            }
            ty::Ty::Generic(box (symbol, _, None))
                if matches!(
                    symbol.sym_name.as_str(),
                    "FbtElement" | "FbtResultBase" | "$FbtResultBase" | "FbtString"
                ) && matches!(
                    &symbol.sym_provenance,
                    flow_common_ty::ty_symbol::Provenance::Library(_)
                ) && frozen_metadata.facebook_fbt.as_deref() == Some("FbtElement") =>
            {
                let new_symbol = Symbol {
                    sym_name: FlowSmolStr::new_inline("Fbt"),
                    ..symbol.clone()
                };
                Arc::new(ty::Ty::Generic(Box::new((
                    new_symbol,
                    ty::GenKind::TypeAliasKind,
                    None,
                ))))
            }
            ty::Ty::Obj(obj) if obj.obj_def_loc.is_some() => {
                let aloc = obj.obj_def_loc.as_ref().unwrap();
                let remote_file = aloc.source().unwrap();
                if remote_file.as_str().ends_with("graphql.js") {
                    let result = graphql::extract_graphql_fragment(
                        self.cx,
                        self.loc_of_aloc,
                        self.get_ast_from_heap,
                        self.file_sig,
                        self.typed_ast,
                        aloc.dupe(),
                    );
                    match result {
                        Some(t) => t,
                        None => self.super_on_t(env, t),
                    }
                } else {
                    self.super_on_t(env, t)
                }
            }
            ty::Ty::Any(_) => self.sanitized_any.dupe(),
            _ => {
                // Check if this is a Union and dispatch to on_union
                match t.as_ref() {
                    ty::Ty::Union(from_bounds, ty1, ty2, tys) => {
                        self.on_union(env, *from_bounds, ty1.dupe(), ty2.dupe(), tys)
                    }
                    _ => self.super_on_t(env, t),
                }
            }
        }
    }
}

impl<'a, 'cx> flow_common_ty::ty_ancestors::TyEndoBase<Loc, ALoc>
    for TypeNormalizationHardcodedFixesMapper<'a, 'cx>
{
}

impl<'a, 'cx> flow_common_ty::ty::TyEndoTy<ALoc, Loc>
    for TypeNormalizationHardcodedFixesMapper<'a, 'cx>
{
    fn on_t(&mut self, env: &Loc, t: ALocTy) -> ALocTy {
        TypeNormalizationHardcodedFixesMapper::on_t(self, env, t)
    }
}

pub mod make_hardcoded_fixes {
    use super::*;

    // Merge the array members of a union into a single read-only array of the union of their
    // element types. In certain kinds of codemods this has shown to cause fewer
    // [ambiguous-speculation] errors.
    pub fn array_simplification(t: ALocTy) -> ALocTy {
        let members = ty::bk_union(&t);
        let mut arr_members: Vec<ALocTy> = Vec::new();
        let mut other_members: Vec<ALocTy> = Vec::new();
        for m in members.iter() {
            match m.as_ref() {
                ty::Ty::Arr(arr) => {
                    arr_members.push(arr.arr_elt_t.dupe());
                }
                _ => {
                    other_members.push(m.dupe());
                }
            }
        }
        match arr_members.len() {
            0 | 1 => t,
            _ => {
                let arr_union = ty::mk_union(true, arr_members).unwrap_or_else(ty::explicit_any);
                let arr = Arc::new(ty::Ty::Arr(ty::ArrT {
                    arr_elt_t: arr_union,
                    arr_readonly: true,
                }));
                let mut all = vec![arr];
                all.extend(other_members);
                ty::mk_union(true, all).unwrap_or_else(ty::explicit_any)
            }
        }
    }

    pub fn run<Extra: BaseStats>(
        cx: &mut flow_typing_context::Context,
        loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
        get_ast_from_heap: &dyn Fn(
            &flow_parser::file_key::FileKey,
        ) -> Option<flow_parser::ast::Program<Loc, Loc>>,
        file_sig: &flow_parser_utils::file_sig::FileSig,
        typed_ast: &flow_parser::ast::Program<ALoc, (ALoc, flow_typing_type::type_::Type)>,
        generalize_maybe: bool,
        generalize_react_mixed_element: bool,
        merge_arrays: bool,
        lint_severities: &flow_lint_settings::lint_settings::LintSettings<
            flow_lint_settings::severity::Severity,
        >,
        allow_dollar_flowfixme: bool,
        acc: Acc<Extra>,
        loc: Loc,
        t: ALocTy,
    ) -> (Acc<Extra>, ALocTy) {
        let acc_ref = std::cell::RefCell::new(acc);
        let add_warning = |loc: Loc, warning: warning::Kind| {
            acc_ref.borrow_mut().warn(&loc, &warning);
        };
        let mut mapper = TypeNormalizationHardcodedFixesMapper::new(
            cx,
            loc_of_aloc,
            get_ast_from_heap,
            file_sig,
            typed_ast,
            lint_severities,
            allow_dollar_flowfixme,
            generalize_maybe,
            generalize_react_mixed_element,
            &add_warning,
        );
        let t_prime = mapper.on_t(&loc, t.dupe());
        let t_prime = if merge_arrays {
            array_simplification(t_prime)
        } else {
            t_prime
        };
        let t_double_prime = if std::sync::Arc::ptr_eq(&t, &t_prime) {
            t
        } else {
            flow_common_ty::ty_utils::simplify_type(false, None, t_prime)
        };
        (acc_ref.into_inner(), t_double_prime)
    }
}
