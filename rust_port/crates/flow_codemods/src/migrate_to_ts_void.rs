/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Prepares code for `experimental.ts_void`, under which `void` is TypeScript's `void`: a type
//! that `undefined` flows into, but that flows into nothing but `void`, `any` and `unknown`.
//!
//! The codemod checks the code under the flag and keeps every `void` the flag accepts. It changes
//! to `undefined` only the `void` annotations whose values the flag rejects, and annotates
//! `: undefined` the functions that return no value whose inferred `void` is rejected. A function
//! passed as a function type whose return type rejects its returned `void` keeps the `void`: the
//! return type, which admits `undefined`, is changed to admit `void`, as TypeScript code types
//! callbacks whose result is optional. Likewise, a type parameter keeps its `void` default when its
//! bound rejects it: the bound is changed to admit `void`. Changing a `void` to `undefined` can make
//! the values flowing into it rejected in turn, so the codemod checks again until nothing is
//! rejected.
//!
//! As in TypeScript, the flag also requires a `return` statement in a function whose return type
//! doesn't include `void` and isn't `any` or `undefined`: such a function returns `undefined`
//! explicitly at its end, which it returned implicitly.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ops::Deref;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common_ty::ty::Elt;
use flow_common_ty::ty::Ty;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc;
use flow_parser::loc_sig::LocSig;
use flow_parser_utils::ast_builder;
use flow_typing::typed_ast_finder;
use flow_typing_errors::error_message::EConstantConditionData;
use flow_typing_errors::error_message::EExpectedBigIntLitData;
use flow_typing_errors::error_message::EExpectedBooleanLitData;
use flow_typing_errors::error_message::EExpectedNumberLitData;
use flow_typing_errors::error_message::EExpectedStringLitData;
use flow_typing_errors::error_message::EIncompatibleSpeculationData;
use flow_typing_errors::error_message::EIncompatibleTypeData;
use flow_typing_errors::error_message::EIncompatibleTypesWithUseOpData;
use flow_typing_errors::error_message::EObjectComputedPropertyAccessData;
use flow_typing_errors::error_message::EPropNotFoundInLookupData;
use flow_typing_errors::error_message::EUnionSpeculationFailedData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::TypeOrTypeDesc;
use flow_typing_errors::flow_error::ErrorSet;
use flow_typing_errors::flow_error::FlowError;
use flow_typing_errors::intermediate_error_types::ConstantConditionKind;
use flow_typing_errors::intermediate_error_types::ConstantConditionWarning;
use flow_typing_flow_js::flow_js::FlowJs;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::FunImplicitReturnData;
use flow_typing_type::type_::FunReturnData;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::VirtualFrameUseOp;
use flow_typing_type::type_::VirtualRootUseOp;
use flow_typing_type::type_::VirtualUseOp;
use flow_typing_type::type_::root_of_use_op;
use flow_typing_type::type_::type_or_type_desc::ErrorTypeDesc;
use flow_typing_type::type_util;

use crate::utils::codemod_context::typed::TypedCodemodContext;

/// The `void`s to change, by file: the location of a `void` annotation, or of the missing return
/// annotation of a function that returns no value. Also the bodies of the functions to return
/// `undefined` at their end, and the type parameters whose bound is to admit their `void` default.
#[derive(Debug, Clone, Default)]
pub struct Targets(pub BTreeMap<FileKey, BTreeSet<Loc>>);

impl Targets {
    pub fn combine(mut self, other: Self) -> Self {
        for (file, locs) in other.0 {
            self.0.entry(file).or_default().extend(locs);
        }
        self
    }

    pub fn len(&self) -> usize {
        self.0.values().map(BTreeSet::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn insert(&mut self, loc: Loc) {
        if let Some(file) = loc.source.dupe() {
            self.0.entry(file).or_default().insert(loc);
        }
    }
}

/// The return types of function types that reject the `void` returned by a function passed as
/// one, by file, with the `void`s each rejects. Such a return type admits `undefined`, as the
/// code checks without the flag: making it admit `void` keeps every `void` it rejects.
#[derive(Debug, Clone, Default)]
pub struct Returns(pub BTreeMap<FileKey, BTreeMap<Loc, Targets>>);

impl Returns {
    pub fn combine(mut self, other: Self) -> Self {
        for (file, returns) in other.0 {
            let entry = self.0.entry(file).or_default();
            for (loc, voids) in returns {
                let voids = entry.remove(&loc).unwrap_or_default().combine(voids);
                entry.insert(loc, voids);
            }
        }
        self
    }
}

/// How many `void` annotations there are, and how many of them are a function's return type.
#[derive(Debug, Clone, Copy, Default)]
pub struct VoidCounts {
    pub annotations: usize,
    pub returns: usize,
}

impl VoidCounts {
    pub fn combine(a: Self, b: Self) -> Self {
        Self {
            annotations: a.annotations + b.annotations,
            returns: a.returns + b.returns,
        }
    }
}

/// What checking a file finds: the `void`s to change, the return types to make admit `void`, and
/// the `void` annotations of the file.
#[derive(Debug, Clone, Default)]
pub struct CheckResult {
    pub targets: Targets,
    pub returns: Returns,
    pub counts: VoidCounts,
}

impl CheckResult {
    pub fn combine(a: Self, b: Self) -> Self {
        Self {
            targets: a.targets.combine(b.targets),
            returns: a.returns.combine(b.returns),
            counts: VoidCounts::combine(a.counts, b.counts),
        }
    }
}

/// The `void` annotations of a file, and those that are a function's return type.
#[derive(Default)]
struct VoidAnnotations {
    count: usize,
    returns: BTreeSet<Loc>,
}

impl VoidAnnotations {
    fn of_program(program: &ast::Program<Loc, Loc>) -> Self {
        let mut annotations = Self::default();
        let Ok(()) = annotations.program(program);
        annotations
    }

    fn insert_if_return(&mut self, annotation: &ast::types::Annotation<Loc, Loc>) {
        if let ast::types::TypeInner::Void { loc, .. } = annotation.annotation.deref() {
            self.returns.insert(loc.dupe());
        }
    }
}

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, !> for VoidAnnotations {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn type_(&mut self, t: &'ast ast::types::Type<Loc, Loc>) -> Result<(), !> {
        if matches!(t.deref(), ast::types::TypeInner::Void { .. }) {
            self.count += 1;
        }
        ast_visitor::type_default(self, t)
    }

    fn function_return_annotation(
        &mut self,
        return_: &'ast ast::function::ReturnAnnot<Loc, Loc>,
    ) -> Result<(), !> {
        if let ast::function::ReturnAnnot::Available(annotation) = return_ {
            self.insert_if_return(annotation);
        }
        ast_visitor::function_return_annotation_default(self, return_)
    }

    fn function_type_return_annotation(
        &mut self,
        return_: &'ast ast::types::function::ReturnAnnotation<Loc, Loc>,
    ) -> Result<(), !> {
        if let ast::types::function::ReturnAnnotation::Available(annotation) = return_ {
            self.insert_if_return(annotation);
        }
        ast_visitor::function_type_return_annotation_default(self, return_)
    }
}

pub fn void_counts(program: &ast::Program<Loc, Loc>) -> VoidCounts {
    let annotations = VoidAnnotations::of_program(program);
    VoidCounts {
        annotations: annotations.count,
        returns: annotations.returns.len(),
    }
}

/// How many `void` annotations and `void` return types there are, before the codemod and after.
pub fn kept_report(counts: VoidCounts, changed: &stats::Stats) -> String {
    let after = |before: usize, delta: i64| before as i64 + delta;
    format!(
        "`void` annotations: {} before, {} after. `void` return types: {} before, {} after",
        counts.annotations,
        after(counts.annotations, changed.annotations_delta),
        counts.returns,
        after(counts.returns, changed.returns_delta),
    )
}

/// Whether an error describes a type as TypeScript's `void`. Errors describe their types by the
/// normalized type once checking is done.
fn is_ts_void(desc: &TypeOrTypeDesc<ALoc>) -> bool {
    match desc {
        TypeOrTypeDesc::Type(t) => {
            matches!(t.deref(), TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::TsVoidT))
        }
        TypeOrTypeDesc::TypeDesc(ErrorTypeDesc::Normalized(Elt::Type(ty))) => {
            matches!(ty.deref(), Ty::TsVoid)
        }
        TypeOrTypeDesc::TypeDesc(_) => false,
    }
}

/// A `void` an error rejects.
pub enum Rejected {
    /// A `void` flowing into another type, read as an object, or tested for truthiness, by its
    /// annotation or by the function whose inferred return it is.
    Void(ALoc),
    /// A function's returned `void` rejected by the return type of the function type the function
    /// is passed as.
    Return { void: ALoc, return_type: ALoc },
    /// A `void`, written at `void`, flowing from `lower` into the type at `upper`. A type
    /// parameter's default flows into its bound.
    Flow {
        void: ALoc,
        lower: ALoc,
        upper: ALoc,
    },
    /// The return annotation of a function without a `return` statement, whose return type
    /// rejects the `void` it returns implicitly.
    MissingReturn(ALoc),
}

/// The return type of the function type that a function's return is checked against, if the
/// error is about that return itself.
fn checked_return_type(use_op: &VirtualUseOp<ALoc>) -> Option<ALoc> {
    match use_op {
        VirtualUseOp::Frame(frame, _) => match frame.deref() {
            VirtualFrameUseOp::FunReturn(deref!(FunReturnData { upper, .. })) => {
                Some(upper.def_loc().dupe())
            }
            _ => None,
        },
        VirtualUseOp::Op(_) => None,
    }
}

/// The `void`s an error rejects.
pub fn rejected_ts_voids(msg: &ErrorMessage<ALoc>, acc: &mut Vec<Rejected>) {
    match msg {
        ErrorMessage::EIncompatibleType(deref!(EIncompatibleTypeData {
            lower_desc,
            lower_def_loc,
            ..
        })) => {
            if is_ts_void(lower_desc) {
                acc.push(Rejected::Void(lower_def_loc.dupe()));
            }
        }
        ErrorMessage::EIncompatibleTypesWithUseOp(deref!(EIncompatibleTypesWithUseOpData {
            use_op,
            lower_loc,
            lower_desc,
            lower_def_loc,
            upper_def_loc,
            branches,
            ..
        })) => {
            if is_ts_void(lower_desc) {
                acc.push(match (checked_return_type(use_op), root_of_use_op(use_op)) {
                    (Some(return_type), _) => Rejected::Return {
                        void: lower_def_loc.dupe(),
                        return_type,
                    },
                    (
                        None,
                        VirtualRootUseOp::FunImplicitReturn(deref!(FunImplicitReturnData {
                            upper, ..
                        })),
                    ) => Rejected::MissingReturn(upper.loc().dupe()),
                    (None, _) => Rejected::Flow {
                        void: lower_def_loc.dupe(),
                        lower: lower_loc.dupe(),
                        upper: upper_def_loc.dupe(),
                    },
                });
            }
            for branch in branches {
                rejected_ts_voids(branch, acc);
            }
        }
        ErrorMessage::EExpectedStringLit(deref!(EExpectedStringLitData { lower: reference, .. }))
        | ErrorMessage::EExpectedNumberLit(deref!(EExpectedNumberLitData {
            lower: reference, ..
        }))
        | ErrorMessage::EExpectedBooleanLit(deref!(EExpectedBooleanLitData {
            lower: reference, ..
        }))
        | ErrorMessage::EExpectedBigIntLit(deref!(EExpectedBigIntLitData {
            lower: reference, ..
        }))
        | ErrorMessage::EPropNotFoundInLookup(deref!(EPropNotFoundInLookupData {
            object: reference,
            ..
        }))
        | ErrorMessage::EObjectComputedPropertyAccess(deref!(EObjectComputedPropertyAccessData {
            object: reference,
            ..
        })) => {
            if is_ts_void(&reference.type_desc) {
                acc.push(Rejected::Void(reference.reference_loc.dupe()));
            }
        }
        ErrorMessage::EIncompatibleSpeculation(deref!(EIncompatibleSpeculationData {
            branches,
            ..
        })) => {
            for branch in branches {
                rejected_ts_voids(branch, acc);
            }
        }
        // The branches of a union are checked within the union's use: a returned `void` they all
        // reject is rejected by the return type, which is the union.
        ErrorMessage::EUnionSpeculationFailed(deref!(EUnionSpeculationFailedData {
            use_op,
            branches,
            ..
        })) => {
            let mut branch_acc = vec![];
            for branch in branches {
                rejected_ts_voids(branch, &mut branch_acc);
            }
            let return_type = checked_return_type(use_op);
            acc.extend(branch_acc.into_iter().map(|rejected| match (rejected, &return_type) {
                (Rejected::Void(void) | Rejected::Flow { void, .. }, Some(return_type)) => {
                    Rejected::Return {
                        void,
                        return_type: return_type.dupe(),
                    }
                }
                (rejected, _) => rejected,
            }));
        }
        ErrorMessage::EConstantCondition(deref!(EConstantConditionData {
            constant_condition_kind: ConstantConditionKind::TsVoid,
            warning:
                ConstantConditionWarning::Likely {
                    suggested_loc: Some(loc),
                },
            ..
        })) => acc.push(Rejected::Void(loc.dupe())),
        _ => {}
    }
}

/// The `void`s that the errors of a checked file reject, grouped by the file they come from, and
/// the return types rejecting returned `void`s. Suppressed errors are left alone.
pub fn targets_of_file(cctx: &mut TypedCodemodContext<'_>) -> (Targets, Returns) {
    let rejected: Vec<Rejected> = {
        let cx = &cctx.cx;
        let reader = &cctx.reader;
        let unsuppressable_error_codes: BTreeSet<FlowSmolStr> = cctx
            .options
            .unsuppressable_error_codes
            .iter()
            .cloned()
            .collect();
        let loc_of_aloc = |aloc: &ALoc| reader.loc_of_aloc(aloc);
        let get_ast = |file: &FileKey| reader.get_ast(file);
        let is_suppressed = |error: &FlowError<ALoc>| {
            let mut unused = cctx.error_suppressions.clone();
            let (errors, _) = cctx.error_suppressions.filter_suppressed_errors(
                &cctx.options.root,
                None,
                false,
                &unsuppressable_error_codes,
                loc_of_aloc,
                get_ast,
                |file| cx.is_global_libdef(file),
                &ErrorSet::singleton(error.clone()),
                &mut unused,
            );
            errors.is_empty()
        };
        cx.errors().fold(vec![], |mut rejected, error| {
            let mut error_rejected = vec![];
            rejected_ts_voids(error.msg_of_error(), &mut error_rejected);
            if !error_rejected.is_empty() && !is_suppressed(&error) {
                rejected.extend(error_rejected);
            }
            rejected
        })
    };
    let mut targets = Targets::default();
    let mut returns = Returns::default();
    for rejected in rejected {
        match rejected {
            Rejected::Flow { lower, upper, .. }
                if let Some(type_param) = rejecting_bound(cctx, &lower, &upper) =>
            {
                targets.insert(type_param);
            }
            Rejected::Void(void) | Rejected::Flow { void, .. } => {
                let loc = cctx.reader.loc_of_aloc(&void);
                for loc in written_ts_voids(cctx, loc) {
                    targets.insert(loc);
                }
            }
            Rejected::Return { void, return_type } => {
                let return_type = cctx.reader.loc_of_aloc(&return_type);
                let Some(file) = return_type.source.dupe() else {
                    continue;
                };
                let loc = cctx.reader.loc_of_aloc(&void);
                let voids = returns
                    .0
                    .entry(file)
                    .or_default()
                    .entry(return_type)
                    .or_default();
                for loc in written_ts_voids(cctx, loc) {
                    voids.insert(loc);
                }
            }
            Rejected::MissingReturn(annotation) => {
                let annotation = cctx.reader.loc_of_aloc(&annotation);
                let body = annotation
                    .source
                    .as_ref()
                    .and_then(|file| cctx.reader.get_ast(file))
                    .and_then(|program| BodyFinder::find(&program, &annotation));
                targets.insert(body.unwrap_or(annotation));
            }
        }
    }
    (targets, returns)
}

/// Finds the block body of the function whose return annotation is at `annotation`.
struct BodyFinder<'a> {
    annotation: &'a Loc,
    found: Option<Loc>,
}

impl BodyFinder<'_> {
    fn find(program: &ast::Program<Loc, Loc>, annotation: &Loc) -> Option<Loc> {
        let mut finder = BodyFinder {
            annotation,
            found: None,
        };
        let Ok(()) = finder.program(program);
        finder.found
    }
}

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, !> for BodyFinder<'_> {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn function_(
        &mut self,
        loc: &'ast Loc,
        function: &'ast ast::function::Function<Loc, Loc>,
    ) -> Result<(), !> {
        if let ast::function::ReturnAnnot::Available(annotation) = &function.return_
            && annotation.annotation.loc() == self.annotation
            && let ast::function::Body::BodyBlock((body_loc, _)) = &function.body
        {
            self.found = Some(body_loc.dupe());
        }
        ast_visitor::function_default(self, loc, function)
    }
}

/// The type parameter whose bound rejects its default, flowing from `lower` into the type at
/// `upper`, if the bound can be made to admit `void`.
fn rejecting_bound(cctx: &TypedCodemodContext<'_>, lower: &ALoc, upper: &ALoc) -> Option<Loc> {
    let lower = cctx.reader.loc_of_aloc(lower);
    let upper = cctx.reader.loc_of_aloc(upper);
    let program = cctx.reader.get_ast(lower.source.as_ref()?)?;
    BoundFinder::find(&program, &lower, &upper)
}

/// Whether the type at `loc` is `t`, or one of the types `t` is a union of or a nullable type of.
/// Making a bound admit `void` admits these types' `void`.
fn is_member(t: &ast::types::Type<Loc, Loc>, loc: &Loc) -> bool {
    t.loc() == loc
        || match t.deref() {
            ast::types::TypeInner::Nullable { inner, .. } => is_member(&inner.argument, loc),
            ast::types::TypeInner::Union { inner, .. } => {
                let (t0, t1, ts) = &inner.types;
                [t0, t1].into_iter().chain(ts).any(|t| is_member(t, loc))
            }
            _ => false,
        }
}

/// Finds the type parameter whose default has the type at `lower` as a member, and whose bound,
/// which can be made to admit `void`, has the type at `upper` as a member.
struct BoundFinder<'a> {
    lower: &'a Loc,
    upper: &'a Loc,
    found: Option<Loc>,
}

impl BoundFinder<'_> {
    fn find(program: &ast::Program<Loc, Loc>, lower: &Loc, upper: &Loc) -> Option<Loc> {
        let mut finder = BoundFinder {
            lower,
            upper,
            found: None,
        };
        let Ok(()) = finder.program(program);
        finder.found
    }
}

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, !> for BoundFinder<'_> {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn type_param(
        &mut self,
        kind: &ast_visitor::TypeParamsContext,
        tparam: &'ast ast::types::TypeParam<Loc, Loc>,
    ) -> Result<(), !> {
        if let Some(default) = &tparam.default
            && is_member(default, self.lower)
            && let ast::types::AnnotationOrHint::Available(bound) = &tparam.bound
            && is_member(&bound.annotation, self.upper)
            && admitting_void(&bound.annotation).is_some()
        {
            self.found = Some(tparam.loc.dupe());
        }
        ast_visitor::type_param_default(self, kind, tparam)
    }
}

/// Where the `void`s an error refers to at `loc` are written. An error refers to a `void` where
/// its type was last placed: where it is written, or at a type alias referring to it, or at a
/// call returning it. In the checked file, the type found there still knows where its `void`s
/// are written.
fn written_ts_voids(cctx: &mut TypedCodemodContext<'_>, loc: Loc) -> Vec<Loc> {
    if loc.source.as_ref() != Some(&cctx.file) {
        return vec![loc];
    }
    let Some(t) = typed_ast_finder::find_exact_match_annotation(
        &cctx.typed_ast,
        ALoc::of_loc(loc.dupe()),
    ) else {
        return vec![loc];
    };
    let reason = type_util::reason_of_t(&t).dupe();
    let Ok(ts) = FlowJs::possible_concrete_types_for_inspection(&mut cctx.cx, &reason, &t) else {
        return vec![loc];
    };
    let written: Vec<Loc> = ts
        .iter()
        .filter_map(|t| match t.deref() {
            TypeInner::DefT(reason, def_t) if matches!(def_t.deref(), DefTInner::TsVoidT) => {
                Some(cctx.reader.loc_of_aloc(reason.def_loc()))
            }
            _ => None,
        })
        .collect();
    if written.is_empty() { vec![loc] } else { written }
}

pub mod stats {
    use flow_parser::loc::Loc;

    #[derive(Debug, Clone, Default)]
    pub struct Stats {
        /// `void` annotations changed to `undefined`.
        pub changed_annotations: usize,
        /// `: undefined` return annotations added to functions that return no value.
        pub added_return_annotations: usize,
        /// Function types' return types changed to admit the returned `void`s they rejected.
        pub widened_return_types: usize,
        /// Type parameters' bounds changed to admit the `void` default they rejected.
        pub widened_bounds: usize,
        /// Return types that could not be changed to admit `void`: the `void`s they reject are
        /// changed instead.
        pub kept_return_types: Vec<Loc>,
        /// `return undefined;` added to functions without a `return` statement.
        pub added_returns: usize,
        /// `unknown` return types of functions without a `return` statement changed to `void`.
        pub void_return_types: usize,
        /// Rejected `void`s returned by function types, kept: changed to `undefined`, they would
        /// reject every function they type that returns no value.
        pub kept_function_type_returns: usize,
        /// Rejected `void`s in generated files and TypeScript declaration files.
        pub unchangeable: usize,
        /// Rejected `void`s that are neither a `void` annotation nor a function's return.
        pub unmatched: Vec<Loc>,
        /// How many more `void` annotations the changed files have.
        pub annotations_delta: i64,
        /// How many more `void` return types the changed files have.
        pub returns_delta: i64,
    }

    impl Stats {
        pub fn combine(mut a: Self, b: Self) -> Self {
            a.changed_annotations += b.changed_annotations;
            a.added_return_annotations += b.added_return_annotations;
            a.widened_return_types += b.widened_return_types;
            a.widened_bounds += b.widened_bounds;
            a.kept_return_types.extend(b.kept_return_types);
            a.added_returns += b.added_returns;
            a.void_return_types += b.void_return_types;
            a.kept_function_type_returns += b.kept_function_type_returns;
            a.unchangeable += b.unchangeable;
            a.unmatched.extend(b.unmatched);
            a.annotations_delta += b.annotations_delta;
            a.returns_delta += b.returns_delta;
            a
        }

        pub fn report(&self) -> String {
            let mut lines = vec![
                format!(
                    "Changed `void` to `undefined`: {}",
                    self.changed_annotations
                ),
                format!(
                    "Added `: undefined` return annotations: {}",
                    self.added_return_annotations
                ),
                format!(
                    "Changed function types' return types to admit `void`: {}",
                    self.widened_return_types
                ),
                format!(
                    "Changed type parameters' bounds to admit their `void` default: {}",
                    self.widened_bounds
                ),
                format!(
                    "Return types that cannot admit `void`, whose `void`s are changed next: {}",
                    self.kept_return_types.len()
                ),
                format!(
                    "Added `return undefined;` to functions without a `return` statement: {}",
                    self.added_returns
                ),
                format!(
                    "Changed `unknown` return types of functions without a `return` statement to `void`: {}",
                    self.void_return_types
                ),
                format!(
                    "Kept in the return types of function types: {}",
                    self.kept_function_type_returns
                ),
                format!(
                    "Left in generated files and TypeScript declaration files: {}",
                    self.unchangeable
                ),
                format!(
                    "Left because they are neither an annotation nor a return: {}",
                    self.unmatched.len()
                ),
            ];
            lines.extend(
                self.unmatched
                    .iter()
                    .take(20)
                    .map(|loc| format!("  {}", flow_common::reason::string_of_loc(None, loc))),
            );
            lines.join("\n")
        }
    }
}

/// Changes the `void`s at the target locations of one file to `undefined`, and makes the functions
/// whose body is there return `undefined` at their end, and the bounds of the type parameters
/// there admit `void`. Makes the return types at the given locations admit `void`.
pub struct Mapper {
    targets: BTreeSet<Loc>,
    return_types: BTreeSet<Loc>,
    type_params: BTreeSet<FlowSmolStr>,
    declared_signatures: BTreeSet<Loc>,
    function_types: Vec<bool>,
    matched: BTreeSet<Loc>,
    pub stats: stats::Stats,
}

impl Mapper {
    pub fn new(targets: BTreeSet<Loc>, return_types: BTreeSet<Loc>) -> Self {
        Self {
            targets,
            return_types,
            type_params: BTreeSet::new(),
            declared_signatures: BTreeSet::new(),
            function_types: vec![],
            matched: BTreeSet::new(),
            stats: stats::Stats::default(),
        }
    }

    pub fn program(&mut self, program: &ast::Program<Loc, Loc>) -> ast::Program<Loc, Loc> {
        self.type_params = TypeParamNames::of_program(program);
        let program_ = self.map_program(program);
        let before = void_counts(program);
        let after = void_counts(&program_);
        self.stats.annotations_delta = after.annotations as i64 - before.annotations as i64;
        self.stats.returns_delta = after.returns as i64 - before.returns as i64;
        self.stats.unmatched = self.targets.difference(&self.matched).cloned().collect();
        self.stats.kept_return_types = self
            .return_types
            .difference(&self.matched)
            .cloned()
            .collect();
        program_
    }
}

fn undefined_type() -> ast::types::Type<Loc, Loc> {
    ast::types::Type::new(ast::types::TypeInner::Undefined {
        loc: Loc::none(),
        comments: None,
    })
}

/// The names of the type parameters a file declares.
#[derive(Default)]
struct TypeParamNames(BTreeSet<FlowSmolStr>);

impl TypeParamNames {
    fn of_program(program: &ast::Program<Loc, Loc>) -> BTreeSet<FlowSmolStr> {
        let mut names = Self::default();
        let Ok(()) = names.program(program);
        names.0
    }
}

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, !> for TypeParamNames {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn type_param(
        &mut self,
        kind: &ast_visitor::TypeParamsContext,
        tparam: &'ast ast::types::TypeParam<Loc, Loc>,
    ) -> Result<(), !> {
        self.0.insert(tparam.name.name.dupe());
        ast_visitor::type_param_default(self, kind, tparam)
    }
}

/// Whether a type refers to one of the given type parameters.
struct MentionsTypeParam<'a> {
    names: &'a BTreeSet<FlowSmolStr>,
    found: bool,
}

impl MentionsTypeParam<'_> {
    fn check(names: &BTreeSet<FlowSmolStr>, t: &ast::types::Type<Loc, Loc>) -> bool {
        let mut visitor = MentionsTypeParam {
            names,
            found: false,
        };
        let Ok(()) = visitor.type_(t);
        visitor.found
    }
}

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, !> for MentionsTypeParam<'_> {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn generic_type(&mut self, gt: &'ast ast::types::Generic<Loc, Loc>) -> Result<(), !> {
        if let ast::types::generic::Identifier::Unqualified(id) = &gt.id
            && self.names.contains(&id.name)
        {
            self.found = true;
        }
        ast_visitor::generic_type_default(self, gt)
    }
}

/// The `void` a return type is, or that it is a `Promise` of.
fn returned_void(t: &ast::types::Type<Loc, Loc>) -> Option<&Loc> {
    match t.deref() {
        ast::types::TypeInner::Void { loc, .. } => Some(loc),
        ast::types::TypeInner::Generic { inner, .. }
            if let ast::types::generic::Identifier::Unqualified(id) = &inner.id
                && id.name == "Promise"
                && let Some(targs) = &inner.targs
                && let [targ] = &*targs.arguments
                && let ast::types::TypeInner::Void { loc, .. } = targ.deref() =>
        {
            Some(loc)
        }
        _ => None,
    }
}

fn void_type() -> ast::types::Type<Loc, Loc> {
    ast::types::Type::new(ast::types::TypeInner::Void {
        loc: Loc::none(),
        comments: None,
    })
}

/// The `void` return type of a function without a `return` statement annotated as returning
/// `unknown`, or a `Promise` of it: as in TypeScript, `void` types a function that returns no value.
fn void_of_unknown_return(
    t: &ast::types::Type<Loc, Loc>,
    async_: bool,
) -> Option<ast::types::Type<Loc, Loc>> {
    let void_of_unknown = |t: &ast::types::Type<Loc, Loc>| match t.deref() {
        ast::types::TypeInner::Unknown { loc, comments }
        | ast::types::TypeInner::Mixed { loc, comments } => {
            Some(ast::types::Type::new(ast::types::TypeInner::Void {
                loc: loc.dupe(),
                comments: comments.clone(),
            }))
        }
        _ => None,
    };
    if !async_ {
        return void_of_unknown(t);
    }
    match t.deref() {
        ast::types::TypeInner::Generic { inner, .. }
            if let ast::types::generic::Identifier::Unqualified(id) = &inner.id
                && id.name == "Promise"
                && let Some(targs) = &inner.targs
                && let [targ] = &*targs.arguments =>
        {
            let void = void_of_unknown(targ)?;
            Some(ast_builder::types::unqualified_generic(
                None,
                None,
                Some(ast_builder::types::type_args(None, None, vec![void])),
                "Promise",
            ))
        }
        _ => None,
    }
}

/// A return type that admits `undefined`, changed to admit `void` as well: a union's `undefined`
/// becomes `void`, and a nullable type is joined with `void`.
fn admitting_void(t: &ast::types::Type<Loc, Loc>) -> Option<ast::types::Type<Loc, Loc>> {
    match t.deref() {
        ast::types::TypeInner::Nullable { .. } => {
            Some(ast::types::Type::new(ast::types::TypeInner::Union {
                loc: Loc::none(),
                inner: Arc::new(ast::types::Union {
                    types: (t.dupe(), void_type(), vec![]),
                    comments: None,
                }),
            }))
        }
        ast::types::TypeInner::Union { loc, inner } => {
            let void_of_undefined = |t: &ast::types::Type<Loc, Loc>| match t.deref() {
                ast::types::TypeInner::Undefined { loc, comments } => {
                    ast::types::Type::new(ast::types::TypeInner::Void {
                        loc: loc.dupe(),
                        comments: comments.clone(),
                    })
                }
                _ => t.dupe(),
            };
            let (t0, t1, ts) = &inner.types;
            let has_undefined = [t0, t1].into_iter().chain(ts).any(|t| {
                matches!(t.deref(), ast::types::TypeInner::Undefined { .. })
            });
            has_undefined.then(|| {
                ast::types::Type::new(ast::types::TypeInner::Union {
                    loc: loc.dupe(),
                    inner: Arc::new(ast::types::Union {
                        types: (
                            void_of_undefined(t0),
                            void_of_undefined(t1),
                            ts.iter().map(void_of_undefined).collect(),
                        ),
                        comments: inner.comments.clone(),
                    }),
                })
            })
        }
        _ => None,
    }
}

impl<'ast> AstVisitor<'ast, Loc> for Mapper {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn map_type_(&mut self, t: &'ast ast::types::Type<Loc, Loc>) -> ast::types::Type<Loc, Loc> {
        // A return type that refers to a type parameter is inferred from the functions passed as
        // it: admitting `void` there changes what the type parameter is inferred as.
        if self.return_types.contains(t.loc())
            && !MentionsTypeParam::check(&self.type_params, t)
            && let Some(t_) = admitting_void(t)
        {
            self.matched.insert(t.loc().dupe());
            self.stats.widened_return_types += 1;
            return t_;
        }
        match t.deref() {
            ast::types::TypeInner::Void { loc, comments } if self.targets.contains(loc) => {
                self.matched.insert(loc.dupe());
                self.stats.changed_annotations += 1;
                ast::types::Type::new(ast::types::TypeInner::Undefined {
                    loc: loc.dupe(),
                    comments: comments.clone(),
                })
            }
            // An error places a `void` written in a union, like a return type, at the union.
            ast::types::TypeInner::Union { loc, inner } => {
                let (t0, t1, ts) = &inner.types;
                if self.targets.contains(loc) {
                    self.matched.insert(loc.dupe());
                    let mut members: Vec<&ast::types::Type<Loc, Loc>> = vec![t0, t1];
                    members.extend(ts);
                    while let Some(member) = members.pop() {
                        match member.deref() {
                            ast::types::TypeInner::Void { loc, .. } => {
                                self.targets.insert(loc.dupe());
                            }
                            ast::types::TypeInner::Union { inner, .. } => {
                                let (t0, t1, ts) = &inner.types;
                                members.extend([t0, t1]);
                                members.extend(ts);
                            }
                            _ => {}
                        }
                    }
                }
                // A rejected `void` beside a type admitting `undefined` is dropped: changed to
                // `undefined`, it would add nothing.
                let members: Vec<&ast::types::Type<Loc, Loc>> =
                    [t0, t1].into_iter().chain(ts).collect();
                let admits_undefined = members.iter().any(|t| {
                    matches!(
                        &***t,
                        ast::types::TypeInner::Nullable { .. }
                            | ast::types::TypeInner::Undefined { .. }
                    )
                });
                let is_rejected_void = |t: &ast::types::Type<Loc, Loc>| {
                    matches!(t.deref(), ast::types::TypeInner::Void { loc, .. } if self.targets.contains(loc))
                };
                if !admits_undefined || !members.iter().any(|t| is_rejected_void(t)) {
                    return ast_visitor::map_type_default(self, t);
                }
                let (dropped, kept): (Vec<_>, Vec<_>) =
                    members.into_iter().partition(|t| is_rejected_void(t));
                for t in dropped {
                    self.matched.insert(t.loc().dupe());
                    self.stats.changed_annotations += 1;
                }
                let mut kept: Vec<ast::types::Type<Loc, Loc>> =
                    kept.into_iter().map(|t| self.map_type_(t)).collect();
                if kept.len() == 1 {
                    return kept.remove(0);
                }
                let rest = kept.split_off(2);
                let t1 = kept.remove(1);
                let t0 = kept.remove(0);
                ast::types::Type::new(ast::types::TypeInner::Union {
                    loc: loc.dupe(),
                    inner: Arc::new(ast::types::Union {
                        types: (t0, t1, rest),
                        comments: inner.comments.clone(),
                    }),
                })
            }
            _ => ast_visitor::map_type_default(self, t),
        }
    }

    // A type parameter's bound that rejects its `void` default admits `void`, as in TypeScript:
    // `T extends ?{...} | void = void`. A default whose `void`s are changed needs no such bound.
    fn map_type_param(
        &mut self,
        kind: &ast_visitor::TypeParamsContext,
        tparam: &'ast ast::types::TypeParam<Loc, Loc>,
    ) -> ast::types::TypeParam<Loc, Loc> {
        let tparam_ = ast_visitor::map_type_param_default(self, kind, tparam);
        if !self.targets.contains(&tparam.loc) {
            return tparam_;
        }
        if tparam_.default != tparam.default {
            self.matched.insert(tparam.loc.dupe());
            return tparam_;
        }
        if let ast::types::AnnotationOrHint::Available(bound) = &tparam_.bound
            && let Some(annotation) = admitting_void(&bound.annotation)
        {
            self.matched.insert(tparam.loc.dupe());
            self.stats.widened_bounds += 1;
            return ast::types::TypeParam {
                bound: ast::types::AnnotationOrHint::Available(ast::types::Annotation {
                    loc: bound.loc.dupe(),
                    annotation,
                }),
                ..tparam_
            };
        }
        tparam_
    }

    // A `void` a function type returns stays `void`: as in TypeScript, the functions it types
    // return `void`, and as `undefined` it would reject every one of them that returns no value.
    // A declared function's signature types no other function.
    fn map_declare_function(
        &mut self,
        loc: &'ast Loc,
        decl: &'ast ast::statement::DeclareFunction<Loc, Loc>,
    ) -> ast::statement::DeclareFunction<Loc, Loc> {
        if let ast::types::TypeInner::Function { loc, .. } = decl.annot.annotation.deref() {
            self.declared_signatures.insert(loc.dupe());
        }
        ast_visitor::map_declare_function_default(self, loc, decl)
    }

    fn map_function_type(
        &mut self,
        loc: &'ast Loc,
        func: &'ast ast::types::Function<Loc, Loc>,
    ) -> ast::types::Function<Loc, Loc> {
        self.function_types
            .push(self.declared_signatures.contains(loc));
        let func_ = ast_visitor::map_function_type_default(self, loc, func);
        self.function_types.pop();
        func_
    }

    fn map_function_type_return_annotation(
        &mut self,
        return_: &'ast ast::types::function::ReturnAnnotation<Loc, Loc>,
    ) -> ast::types::function::ReturnAnnotation<Loc, Loc> {
        if !self.function_types.last().copied().unwrap_or(false)
            && let ast::types::function::ReturnAnnotation::Available(annot) = return_
            && let Some(void) = returned_void(&annot.annotation)
            && self.targets.contains(void)
        {
            self.matched.insert(void.dupe());
            self.stats.kept_function_type_returns += 1;
            return return_.clone();
        }
        ast_visitor::map_function_type_return_annotation_default(self, return_)
    }

    fn map_function_(
        &mut self,
        loc: &'ast Loc,
        function: &'ast ast::function::Function<Loc, Loc>,
    ) -> ast::function::Function<Loc, Loc> {
        let mut function_ = ast_visitor::map_function_default(self, loc, function);
        // A function's inferred return is located as the checker locates it: at the expression
        // body, or just before the block body. The signature of an exported function locates it
        // where the return annotation is missing.
        let inferred_return_loc = match &function.body {
            ast::function::Body::BodyExpression(expr) => expr.loc().dupe(),
            ast::function::Body::BodyBlock((body_loc, _)) => body_loc.char_before(),
        };
        if let ast::function::ReturnAnnot::Missing(missing_loc) = &function.return_
            && (self.targets.contains(&inferred_return_loc) || self.targets.contains(missing_loc))
            && !function.generator
        {
            self.matched.insert(inferred_return_loc);
            self.matched.insert(missing_loc.dupe());
            let return_t = if function.async_ {
                ast_builder::types::unqualified_generic(
                    None,
                    None,
                    Some(ast_builder::types::type_args(
                        None,
                        None,
                        vec![undefined_type()],
                    )),
                    "Promise",
                )
            } else {
                undefined_type()
            };
            self.stats.added_return_annotations += 1;
            function_.return_ =
                ast::function::ReturnAnnot::Available(ast_builder::types::annotation(return_t));
        }
        // A function without a `return` statement returns explicitly the `undefined` it returned
        // implicitly at its end, unless it is annotated as returning `unknown`: that annotation
        // becomes `void`.
        if let ast::function::Body::BodyBlock((body_loc, block)) = &function_.body
            && self.targets.contains(body_loc)
        {
            self.matched.insert(body_loc.dupe());
            if let ast::function::ReturnAnnot::Available(annot) = &function_.return_
                && let Some(void) = void_of_unknown_return(&annot.annotation, function.async_)
            {
                self.stats.void_return_types += 1;
                function_.return_ = ast::function::ReturnAnnot::Available(ast::types::Annotation {
                    loc: annot.loc.dupe(),
                    annotation: void,
                });
                return function_;
            }
            self.stats.added_returns += 1;
            let return_undefined = ast_builder::statements::return_(
                None,
                None,
                Some(ast_builder::expressions::identifier(None, None, "undefined")),
            );
            let body = block.body.iter().cloned().chain([return_undefined]).collect();
            function_.body = ast::function::Body::BodyBlock((
                body_loc.dupe(),
                ast::statement::Block {
                    body,
                    comments: block.comments.clone(),
                },
            ));
        }
        function_
    }
}

/// Whether a file's targets can be changed: files generated by a tool are changed through the
/// tool, and TypeScript declaration files already mean TypeScript's `void`.
pub fn is_changeable(file: &FileKey, program: &ast::Program<Loc, Loc>) -> bool {
    let header_end = program.statements.first().map(|s| s.loc().start);
    !file.as_str().ends_with(".d.ts")
        && !program
            .all_comments
            .iter()
            .take_while(|comment| header_end.is_none_or(|end| comment.loc.start < end))
            .any(|comment| comment.text.contains("@generated"))
}
