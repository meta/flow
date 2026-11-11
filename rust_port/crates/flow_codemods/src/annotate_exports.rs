/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use dupe::Dupe;
use flow_parser::ast;
use flow_parser::ast::VariableKind;
use flow_parser::ast::class;
use flow_parser::ast::expression;
use flow_parser::ast::pattern;
use flow_parser::ast::statement;
use flow_parser::ast::types;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::loc::Loc;
use flow_parser::loc_sig::LocSig;

use crate::utils::codemod_annotator;
use crate::utils::codemod_context;

type TyResult = Result<
    flow_common_ty::ty::ALocTy,
    Vec<flow_services_code_action::insert_type_utils::error::Kind>,
>;

const WIDTH: usize = 45;

fn string_of_row(indent: usize, name: &str, i: usize) -> String {
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

pub mod signature_verification {
    use super::*;

    pub fn supported_error_kind<'a, 'cx>(
        cctx: &'a mut codemod_context::typed::TypedCodemodContext<'cx>,
        max_type_size: i32,
        mut acc: BTreeMap<Loc, TyResult>,
        loc: Loc,
    ) -> BTreeMap<Loc, TyResult> {
        let ty_result = codemod_annotator::get_validated_ty(cctx, max_type_size, loc.clone());
        acc.insert(loc, ty_result);
        acc
    }

    pub fn unsupported_error_kind(
        default_any: bool,
        mut acc: BTreeMap<Loc, TyResult>,
        loc: Loc,
    ) -> BTreeMap<Loc, TyResult> {
        if default_any {
            acc.insert(
                loc,
                Err(vec![
                    flow_services_code_action::insert_type_utils::error::Kind::UnsupportedErrorKind,
                ]),
            );
            acc
        } else {
            acc
        }
    }

    pub fn collect_annotations<'a, 'cx>(
        cctx: &'a mut codemod_context::typed::TypedCodemodContext<'cx>,
        default_any: bool,
        max_type_size: i32,
        ast: &ast::Program<Loc, Loc>,
    ) -> (usize, BTreeMap<Loc, TyResult>) {
        let options = &cctx.options;
        let docblock = &cctx.docblock;
        let file = &cctx.file;
        let prevent_munge = docblock.prevent_munge();
        let sig_opts = flow_type_sig::type_sig_options::TypeSigOptions::of_options(
            options,
            prevent_munge,
            vec![],
            file,
            cctx.cx.is_global_lib_context(),
        );
        let strict = docblock.is_strict();
        let platform_availability_set = flow_common::platform_set::available_platforms(
            &options.file_options,
            &options.projects_options,
            file.suffix(),
            docblock.supports_platform(),
        );
        let arenas = bumpalo::Bump::new();
        let (sig_errors, locs, _module) = flow_type_sig::type_sig_utils::parse_and_pack_module(
            &sig_opts,
            &arenas,
            strict,
            platform_availability_set,
            None,
            ast,
        );
        use flow_type_sig::compact_table::Index;
        use flow_type_sig::signature_error::SignatureError;
        use flow_type_sig::type_sig::Errno;
        sig_errors
            .into_iter()
            .fold((0, BTreeMap::new()), |(tot_errors, acc), err| match err {
                Errno::CheckError => (tot_errors, acc),
                Errno::BindingValidationError(_) => (tot_errors, acc),
                Errno::SigError(err) => {
                    let loc_index: Index<Loc> = match err.as_ref() {
                        SignatureError::ExpectedAnnotation(loc, _) => loc.clone(),
                        SignatureError::UnexpectedExpression(loc, _) => loc.clone(),
                        SignatureError::UnexpectedObjectKey(loc, _) => loc.clone(),
                        SignatureError::EmptyArray(loc) => loc.clone(),
                        SignatureError::EmptyObject(loc) => loc.clone(),
                        SignatureError::UnexpectedArraySpread(loc, _) => loc.clone(),
                        SignatureError::UnexpectedArrayHole(loc) => loc.clone(),
                    };
                    let loc = locs.get(loc_index).clone();
                    match *err {
                        SignatureError::ExpectedAnnotation(_, _)
                        | SignatureError::UnexpectedExpression(_, _)
                        | SignatureError::UnexpectedObjectKey(_, _)
                        | SignatureError::EmptyArray(_)
                        | SignatureError::EmptyObject(_)
                        | SignatureError::UnexpectedArraySpread(_, _) => (
                            tot_errors + 1,
                            supported_error_kind(cctx, max_type_size, acc, loc),
                        ),
                        SignatureError::UnexpectedArrayHole(_) => (
                            tot_errors + 1,
                            unsupported_error_kind(default_any, acc, loc),
                        ),
                    }
                }
            })
    }
}

pub mod signature_verification_error_stats {
    use super::*;

    #[derive(Debug, Clone, Default)]
    pub struct SignatureVerificationErrorStats {
        pub number_of_sig_ver_errors: usize,
        pub number_of_annotations_required: usize,
        pub number_of_annotations_skipped: usize,
    }

    impl SignatureVerificationErrorStats {
        pub fn empty() -> Self {
            Self {
                number_of_sig_ver_errors: 0,
                number_of_annotations_required: 0,
                number_of_annotations_skipped: 0,
            }
        }

        pub fn combine(c1: &Self, c2: &Self) -> Self {
            Self {
                number_of_sig_ver_errors: c1.number_of_sig_ver_errors + c2.number_of_sig_ver_errors,
                number_of_annotations_required: c1.number_of_annotations_required
                    + c2.number_of_annotations_required,
                number_of_annotations_skipped: c1.number_of_annotations_skipped
                    + c2.number_of_annotations_skipped,
            }
        }

        pub fn serialize(&self) -> Vec<String> {
            vec![
                format!("sig_ver_errors: {}", self.number_of_sig_ver_errors),
                format!(
                    "annotations_required: {}",
                    self.number_of_annotations_required
                ),
                format!(
                    "annotations_skipped: {}",
                    self.number_of_annotations_skipped
                ),
            ]
        }

        pub fn report(&self) -> Vec<String> {
            vec![
                string_of_row(
                    2,
                    "Number of sig. ver. errors",
                    self.number_of_sig_ver_errors,
                ),
                string_of_row(
                    2,
                    "Number of annotations required",
                    self.number_of_annotations_required,
                ),
                string_of_row(
                    2,
                    "Number of annotations skipped",
                    self.number_of_annotations_skipped,
                ),
            ]
        }
    }

    impl flow_services_code_action::insert_type_utils::BaseStats for SignatureVerificationErrorStats {
        fn empty() -> Self {
            Self::empty()
        }

        fn combine(a: &Self, b: &Self) -> Self {
            Self::combine(a, b)
        }

        fn serialize(&self) -> Vec<String> {
            self.serialize()
        }

        fn report(&self) -> Vec<String> {
            self.report()
        }
    }
}

use signature_verification_error_stats::SignatureVerificationErrorStats;

pub struct AnnotateExportsMapper<'a, 'cx> {
    max_type_size: i32,
    default_any: bool,
    sig_verification_loc_tys: BTreeMap<Loc, TyResult>,
    total_errors: usize,
    mapper: codemod_annotator::Mapper<'a, 'cx, SignatureVerificationErrorStats>,
}

impl<'a, 'cx> AnnotateExportsMapper<'a, 'cx> {
    pub fn new(
        max_type_size: i32,
        default_any: bool,
        cctx: &'a mut codemod_context::typed::TypedCodemodContext<'cx>,
    ) -> Self {
        let lint_severities = codemod_context::typed::lint_severities(cctx);
        Self {
            max_type_size,
            default_any,
            sig_verification_loc_tys: BTreeMap::new(),
            total_errors: 0,
            mapper: codemod_annotator::Mapper {
                added_annotations_locmap: BTreeMap::new(),
                wont_annotate_locs: BTreeSet::new(),
                codemod_error_locs: BTreeSet::new(),
                remote_converter: None,
                cctx,
                default_any,
                generalize_maybe: true,
                generalize_react_mixed_element: true,
                lint_severities,
                max_type_size: max_type_size as usize,
                merge_arrays: false,
                acc: flow_services_code_action::insert_type_utils::Acc::empty(),
                _phantom: std::marker::PhantomData,
            },
        }
    }

    #[allow(dead_code)]
    #[allow(dead_code)]
    fn annotate_class_prop(
        &mut self,
        loc: Loc,
        prop: &class::Property<Loc, Loc>,
        ty: &TyResult,
    ) -> Result<class::Property<Loc, Loc>, ()> {
        match ty {
            Ok(aloc_ty) => {
                let ty_or = codemod_annotator::TyOrTypeAst::Ty_(aloc_ty.dupe());
                let prop = prop.clone();
                self.mapper
                    .annotate_node(loc, ty_or, |type_ast| class::Property {
                        annot: types::AnnotationOrHint::Available(type_ast),
                        ..prop.clone()
                    })
                    .map_err(|_| ())
            }
            Err(_) => Err(()),
        }
    }

    fn add_annot_to_missing(
        &mut self,
        loc: Loc,
        ty: &TyResult,
        annot: types::AnnotationOrHint<Loc, Loc>,
    ) -> types::AnnotationOrHint<Loc, Loc> {
        match ty {
            Err(errs) => {
                for err in errs {
                    self.mapper.acc.error(&loc, err);
                }
                self.mapper.codemod_error_locs.insert(loc.clone());
                if self.default_any {
                    flow_services_code_action::insert_type_utils::Acc::<
                        SignatureVerificationErrorStats,
                    >::info(
                        &loc,
                        &flow_services_code_action::insert_type_utils::info::T::DefaultAny,
                    );
                    let explicit_any = flow_common_ty::ty::explicit_any();
                    let ty_or = codemod_annotator::TyOrTypeAst::Ty_(explicit_any);
                    match self.mapper.annotate_node(loc.clone(), ty_or, |a| {
                        types::AnnotationOrHint::Available(a)
                    }) {
                        Ok(y) => {
                            flow_services_code_action::insert_type_utils::Acc::<SignatureVerificationErrorStats>::debug(
                                &loc,
                                &flow_services_code_action::insert_type_utils::debug::T::AddAnnotation(
                                    flow_services_code_action::insert_type_utils::debug::NodeKind::Prop,
                                ),
                            );
                            y
                        }
                        Err(e) => {
                            self.mapper.acc.error(&loc, &e);
                            self.mapper.codemod_error_locs.insert(loc.clone());
                            let _desc =
                                flow_services_code_action::insert_type_utils::error::serialize(&e);
                            flow_services_code_action::insert_type_utils::Acc::<
                                SignatureVerificationErrorStats,
                            >::info(
                                &loc,
                                &flow_services_code_action::insert_type_utils::info::T::DefaultAny,
                            );
                            let flowfixme_ast =
                                codemod_context::typed::flowfixme_ast(&self.mapper.lint_severities);
                            types::AnnotationOrHint::Available(ast::types::Annotation {
                                loc: Loc::none(),
                                annotation: flowfixme_ast,
                            })
                        }
                    }
                } else {
                    annot
                }
            }
            Ok(aloc_ty) => {
                let ty_or = codemod_annotator::TyOrTypeAst::Ty_(aloc_ty.dupe());
                match self.mapper.annotate_node(loc.clone(), ty_or, |a| {
                    types::AnnotationOrHint::Available(a)
                }) {
                    Ok(y) => {
                        flow_services_code_action::insert_type_utils::Acc::<
                            SignatureVerificationErrorStats,
                        >::debug(
                            &loc,
                            &flow_services_code_action::insert_type_utils::debug::T::AddAnnotation(
                                flow_services_code_action::insert_type_utils::debug::NodeKind::Prop,
                            ),
                        );
                        y
                    }
                    Err(e) if self.default_any => {
                        self.mapper.acc.error(&loc, &e);
                        self.mapper.codemod_error_locs.insert(loc.clone());
                        let _desc =
                            flow_services_code_action::insert_type_utils::error::serialize(&e);
                        flow_services_code_action::insert_type_utils::Acc::<
                            SignatureVerificationErrorStats,
                        >::info(
                            &loc,
                            &flow_services_code_action::insert_type_utils::info::T::DefaultAny,
                        );
                        let flowfixme_ast =
                            codemod_context::typed::flowfixme_ast(&self.mapper.lint_severities);
                        types::AnnotationOrHint::Available(ast::types::Annotation {
                            loc: Loc::none(),
                            annotation: flowfixme_ast,
                        })
                    }
                    Err(_) => annot,
                }
            }
        }
    }

    pub fn post_run(&mut self) -> SignatureVerificationErrorStats {
        let lmap: BTreeMap<Loc, ()> = self
            .sig_verification_loc_tys
            .keys()
            .map(|k| (k.clone(), ()))
            .collect();
        self.mapper.add_unannotated_loc_warnings(&lmap);
        SignatureVerificationErrorStats {
            number_of_sig_ver_errors: self.total_errors,
            number_of_annotations_required: self.sig_verification_loc_tys.len(),
            number_of_annotations_skipped: self.mapper.wont_annotate_locs.len(),
        }
    }

    pub fn program(&mut self, prog: &ast::Program<Loc, Loc>) -> ast::Program<Loc, Loc> {
        let (total_errors_, sig_verification_loc_tys_) =
            signature_verification::collect_annotations(
                self.mapper.cctx,
                self.default_any,
                self.max_type_size,
                prog,
            );
        self.total_errors = total_errors_;
        self.sig_verification_loc_tys = sig_verification_loc_tys_;
        if self.sig_verification_loc_tys.is_empty() {
            prog.clone()
        } else {
            self.mapper.initialize_program_state(prog);
            let prog_ = ast_visitor::map_program_default(self, prog);
            let extra = self.post_run();
            self.mapper.finalize_program(prog, prog_, extra)
        }
    }

    pub fn acc(
        &mut self,
    ) -> flow_services_code_action::insert_type_utils::Acc<SignatureVerificationErrorStats> {
        self.mapper.acc.clone()
    }
}

impl<'ast, 'a, 'cx> AstVisitor<'ast, Loc> for AnnotateExportsMapper<'a, 'cx> {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn map_variable_declarator(
        &mut self,
        kind: ast::VariableKind,
        decl: &'ast statement::variable::Declarator<Loc, Loc>,
    ) -> statement::variable::Declarator<Loc, Loc> {
        match &decl.id {
            pattern::Pattern::Identifier { inner, .. }
                if matches!(&inner.annot, types::AnnotationOrHint::Missing(_))
                    && !inner.optional
                    && decl.init.as_ref().is_some_and(|init_expr| {
                        let eloc = init_expr.loc();
                        self.sig_verification_loc_tys.contains_key(eloc)
                    }) =>
            {
                let init_expr = decl.init.as_ref().unwrap();
                let eloc = init_expr.loc().clone();
                let ty = self.sig_verification_loc_tys.get(&eloc).cloned().unwrap();
                let annot_ = self.add_annot_to_missing(eloc, &ty, inner.annot.clone());
                if annot_ == inner.annot {
                    decl.clone()
                } else {
                    let id_loc = match &decl.id {
                        pattern::Pattern::Identifier { loc, .. } => loc.clone(),
                        _ => unreachable!(),
                    };
                    statement::variable::Declarator {
                        loc: decl.loc.clone(),
                        id: pattern::Pattern::Identifier {
                            loc: id_loc,
                            inner: Arc::new(pattern::Identifier {
                                name: inner.name.clone(),
                                annot: annot_,
                                optional: inner.optional,
                            }),
                        },
                        init: decl.init.clone(),
                        definite: decl.definite,
                    }
                }
            }
            _ => ast_visitor::map_variable_declarator_default(self, kind, decl),
        }
    }

    fn map_expression(
        &mut self,
        expr: &'ast expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        let _options = &self.mapper.cctx.options;
        let expr = ast_visitor::map_expression_default(self, expr);
        let loc = expr.loc().clone();
        match self.sig_verification_loc_tys.get(&loc) {
            Some(type_entry) => {
                let type_entry = type_entry.clone();
                match type_entry {
                    Err(errs) => {
                        for err in &errs {
                            self.mapper.acc.error(&loc, err);
                        }
                        self.mapper.codemod_error_locs.insert(loc.clone());
                        if self.default_any {
                            flow_services_code_action::insert_type_utils::Acc::<
                                SignatureVerificationErrorStats,
                            >::info(
                                &loc,
                                &flow_services_code_action::insert_type_utils::info::T::DefaultAny,
                            );
                            let explicit_any = flow_common_ty::ty::explicit_any();
                            let ty_or = codemod_annotator::TyOrTypeAst::Ty_(explicit_any);
                            match self.mapper.annotate_expr(loc.clone(), expr.clone(), ty_or) {
                                Ok(y) => {
                                    flow_services_code_action::insert_type_utils::Acc::<SignatureVerificationErrorStats>::debug(
                                        &loc,
                                        &flow_services_code_action::insert_type_utils::debug::T::AddAnnotation(
                                            flow_services_code_action::insert_type_utils::debug::NodeKind::Prop,
                                        ),
                                    );
                                    y
                                }
                                Err(e) => {
                                    self.mapper.acc.error(&loc, &e);
                                    self.mapper.codemod_error_locs.insert(loc.clone());
                                    let _desc = flow_services_code_action::insert_type_utils::error::serialize(&e);
                                    flow_services_code_action::insert_type_utils::Acc::<SignatureVerificationErrorStats>::info(
                                        &loc,
                                        &flow_services_code_action::insert_type_utils::info::T::DefaultAny,
                                    );
                                    let flowfixme_ast = codemod_context::typed::flowfixme_ast(
                                        &self.mapper.lint_severities,
                                    );
                                    let annot = ast::types::Annotation {
                                        loc: Loc::none(),
                                        annotation: flowfixme_ast,
                                    };
                                    expression::Expression::new(
                                        expression::ExpressionInner::AsExpression {
                                            loc: loc.clone(),
                                            inner: Arc::new(expression::AsExpression {
                                                expression: expr.clone(),
                                                annot,
                                                comments: None,
                                            }),
                                        },
                                    )
                                }
                            }
                        } else {
                            expr
                        }
                    }
                    Ok(aloc_ty) => {
                        let ty_or = codemod_annotator::TyOrTypeAst::Ty_(aloc_ty);
                        match self.mapper.annotate_expr(loc.clone(), expr.clone(), ty_or) {
                            Ok(y) => {
                                flow_services_code_action::insert_type_utils::Acc::<SignatureVerificationErrorStats>::debug(
                                    &loc,
                                    &flow_services_code_action::insert_type_utils::debug::T::AddAnnotation(
                                        flow_services_code_action::insert_type_utils::debug::NodeKind::Prop,
                                    ),
                                );
                                y
                            }
                            Err(e) if self.default_any => {
                                self.mapper.acc.error(&loc, &e);
                                self.mapper.codemod_error_locs.insert(loc.clone());
                                let _desc =
                                    flow_services_code_action::insert_type_utils::error::serialize(
                                        &e,
                                    );
                                flow_services_code_action::insert_type_utils::Acc::<SignatureVerificationErrorStats>::info(
                                    &loc,
                                    &flow_services_code_action::insert_type_utils::info::T::DefaultAny,
                                );
                                let flowfixme_ast = codemod_context::typed::flowfixme_ast(
                                    &self.mapper.lint_severities,
                                );
                                let annot = ast::types::Annotation {
                                    loc: Loc::none(),
                                    annotation: flowfixme_ast,
                                };
                                expression::Expression::new(
                                    expression::ExpressionInner::AsExpression {
                                        loc: loc.clone(),
                                        inner: Arc::new(expression::AsExpression {
                                            expression: expr.clone(),
                                            annot,
                                            comments: None,
                                        }),
                                    },
                                )
                            }
                            Err(_) => expr,
                        }
                    }
                }
            }
            None => expr,
        }
    }

    fn map_function_return_annotation(
        &mut self,
        return_: &'ast ast::function::ReturnAnnot<Loc, Loc>,
    ) -> ast::function::ReturnAnnot<Loc, Loc> {
        match return_ {
            ast::function::ReturnAnnot::Available(_) | ast::function::ReturnAnnot::TypeGuard(_) => {
                return_.clone()
            }
            ast::function::ReturnAnnot::Missing(loc) => {
                match self.sig_verification_loc_tys.get(loc) {
                    None => return_.clone(),
                    Some(ty) => {
                        let ty = ty.clone();
                        let loc = loc.clone();
                        match ty {
                            Err(errs) => {
                                for err in &errs {
                                    self.mapper.acc.error(&loc, err);
                                }
                                self.mapper.codemod_error_locs.insert(loc.clone());
                                if self.default_any {
                                    flow_services_code_action::insert_type_utils::Acc::<SignatureVerificationErrorStats>::info(
                                        &loc,
                                        &flow_services_code_action::insert_type_utils::info::T::DefaultAny,
                                    );
                                    let explicit_any = flow_common_ty::ty::explicit_any();
                                    let ty_or = codemod_annotator::TyOrTypeAst::Ty_(explicit_any);
                                    match self.mapper.annotate_node(loc.clone(), ty_or, |a| {
                                        ast::function::ReturnAnnot::Available(a)
                                    }) {
                                        Ok(y) => {
                                            flow_services_code_action::insert_type_utils::Acc::<SignatureVerificationErrorStats>::debug(
                                                &loc,
                                                &flow_services_code_action::insert_type_utils::debug::T::AddAnnotation(
                                                    flow_services_code_action::insert_type_utils::debug::NodeKind::Prop,
                                                ),
                                            );
                                            y
                                        }
                                        Err(e) => {
                                            self.mapper.acc.error(&loc, &e);
                                            self.mapper.codemod_error_locs.insert(loc.clone());
                                            let _desc = flow_services_code_action::insert_type_utils::error::serialize(&e);
                                            flow_services_code_action::insert_type_utils::Acc::<SignatureVerificationErrorStats>::info(
                                                &loc,
                                                &flow_services_code_action::insert_type_utils::info::T::DefaultAny,
                                            );
                                            let flowfixme_ast =
                                                codemod_context::typed::flowfixme_ast(
                                                    &self.mapper.lint_severities,
                                                );
                                            ast::function::ReturnAnnot::Available(
                                                ast::types::Annotation {
                                                    loc: Loc::none(),
                                                    annotation: flowfixme_ast,
                                                },
                                            )
                                        }
                                    }
                                } else {
                                    return_.clone()
                                }
                            }
                            Ok(aloc_ty) => {
                                let ty_or = codemod_annotator::TyOrTypeAst::Ty_(aloc_ty);
                                match self.mapper.annotate_node(loc.clone(), ty_or, |a| {
                                    ast::function::ReturnAnnot::Available(a)
                                }) {
                                    Ok(y) => {
                                        flow_services_code_action::insert_type_utils::Acc::<SignatureVerificationErrorStats>::debug(
                                            &loc,
                                            &flow_services_code_action::insert_type_utils::debug::T::AddAnnotation(
                                                flow_services_code_action::insert_type_utils::debug::NodeKind::Prop,
                                            ),
                                        );
                                        y
                                    }
                                    Err(e) if self.default_any => {
                                        self.mapper.acc.error(&loc, &e);
                                        self.mapper.codemod_error_locs.insert(loc.clone());
                                        let _desc = flow_services_code_action::insert_type_utils::error::serialize(&e);
                                        flow_services_code_action::insert_type_utils::Acc::<SignatureVerificationErrorStats>::info(
                                            &loc,
                                            &flow_services_code_action::insert_type_utils::info::T::DefaultAny,
                                        );
                                        let flowfixme_ast = codemod_context::typed::flowfixme_ast(
                                            &self.mapper.lint_severities,
                                        );
                                        ast::function::ReturnAnnot::Available(
                                            ast::types::Annotation {
                                                loc: Loc::none(),
                                                annotation: flowfixme_ast,
                                            },
                                        )
                                    }
                                    Err(_) => return_.clone(),
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn map_class_extends(
        &mut self,
        extends: &'ast class::Extends<Loc, Loc>,
    ) -> class::Extends<Loc, Loc> {
        match (&*extends.expr, &extends.targs) {
            (expression::ExpressionInner::Call { .. }, None) => {
                ast_visitor::map_class_extends_default(self, extends)
            }
            _ => {
                let loc = &extends.loc;
                if self.sig_verification_loc_tys.contains_key(loc) {
                    self.mapper.wont_annotate_locs.insert(loc.clone());
                }
                extends.clone()
            }
        }
    }

    fn map_function_param_pattern(
        &mut self,
        expr: &'ast pattern::Pattern<Loc, Loc>,
    ) -> pattern::Pattern<Loc, Loc> {
        match expr {
            pattern::Pattern::Identifier { loc, inner }
                if matches!(&inner.annot, types::AnnotationOrHint::Missing(_)) =>
            {
                if self.default_any {
                    match self.sig_verification_loc_tys.get(loc).cloned() {
                        None => expr.clone(),
                        Some(type_entry) => {
                            let annot_ = self.add_annot_to_missing(
                                loc.clone(),
                                &type_entry,
                                inner.annot.clone(),
                            );
                            pattern::Pattern::Identifier {
                                loc: loc.clone(),
                                inner: Arc::new(pattern::Identifier {
                                    name: inner.name.clone(),
                                    annot: annot_,
                                    optional: inner.optional,
                                }),
                            }
                        }
                    }
                } else {
                    if self.sig_verification_loc_tys.contains_key(loc) {
                        self.mapper.wont_annotate_locs.insert(loc.clone());
                    }
                    ast_visitor::map_function_param_pattern_default(self, expr)
                }
            }
            _ => ast_visitor::map_function_param_pattern_default(self, expr),
        }
    }

    fn map_class_element(
        &mut self,
        elem: &'ast class::BodyElement<Loc, Loc>,
    ) -> class::BodyElement<Loc, Loc> {
        let elem = ast_visitor::map_class_element_default(self, elem);
        match &elem {
            class::BodyElement::Property(prop) => {
                let loc = &prop.loc;
                match self.sig_verification_loc_tys.get(loc) {
                    None => elem,
                    Some(ty) => {
                        let ty = ty.clone();
                        let loc = loc.clone();
                        let prop = prop.clone();
                        let prop_ = match ty {
                            Err(errs) => {
                                for err in &errs {
                                    self.mapper.acc.error(&loc, err);
                                }
                                self.mapper.codemod_error_locs.insert(loc.clone());
                                if self.default_any {
                                    flow_services_code_action::insert_type_utils::Acc::<SignatureVerificationErrorStats>::info(
                                        &loc,
                                        &flow_services_code_action::insert_type_utils::info::T::DefaultAny,
                                    );
                                    let explicit_any = flow_common_ty::ty::explicit_any();
                                    let ty_or = codemod_annotator::TyOrTypeAst::Ty_(explicit_any);
                                    match self.mapper.annotate_node(
                                        loc.clone(),
                                        ty_or,
                                        |type_ast| class::Property {
                                            annot: types::AnnotationOrHint::Available(type_ast),
                                            ..prop.clone()
                                        },
                                    ) {
                                        Ok(y) => {
                                            flow_services_code_action::insert_type_utils::Acc::<SignatureVerificationErrorStats>::debug(
                                                &loc,
                                                &flow_services_code_action::insert_type_utils::debug::T::AddAnnotation(
                                                    flow_services_code_action::insert_type_utils::debug::NodeKind::Prop,
                                                ),
                                            );
                                            y
                                        }
                                        Err(e) => {
                                            self.mapper.acc.error(&loc, &e);
                                            self.mapper.codemod_error_locs.insert(loc.clone());
                                            let _desc = flow_services_code_action::insert_type_utils::error::serialize(&e);
                                            flow_services_code_action::insert_type_utils::Acc::<SignatureVerificationErrorStats>::info(
                                                &loc,
                                                &flow_services_code_action::insert_type_utils::info::T::DefaultAny,
                                            );
                                            let flowfixme_ast =
                                                codemod_context::typed::flowfixme_ast(
                                                    &self.mapper.lint_severities,
                                                );
                                            class::Property {
                                                annot: types::AnnotationOrHint::Available(
                                                    ast::types::Annotation {
                                                        loc: Loc::none(),
                                                        annotation: flowfixme_ast,
                                                    },
                                                ),
                                                ..prop.clone()
                                            }
                                        }
                                    }
                                } else {
                                    prop.clone()
                                }
                            }
                            Ok(aloc_ty) => {
                                let ty_or = codemod_annotator::TyOrTypeAst::Ty_(aloc_ty);
                                match self.mapper.annotate_node(loc.clone(), ty_or, |type_ast| {
                                    class::Property {
                                        annot: types::AnnotationOrHint::Available(type_ast),
                                        ..prop.clone()
                                    }
                                }) {
                                    Ok(y) => {
                                        flow_services_code_action::insert_type_utils::Acc::<SignatureVerificationErrorStats>::debug(
                                            &loc,
                                            &flow_services_code_action::insert_type_utils::debug::T::AddAnnotation(
                                                flow_services_code_action::insert_type_utils::debug::NodeKind::Prop,
                                            ),
                                        );
                                        y
                                    }
                                    Err(e) if self.default_any => {
                                        self.mapper.acc.error(&loc, &e);
                                        self.mapper.codemod_error_locs.insert(loc.clone());
                                        let _desc = flow_services_code_action::insert_type_utils::error::serialize(&e);
                                        flow_services_code_action::insert_type_utils::Acc::<SignatureVerificationErrorStats>::info(
                                            &loc,
                                            &flow_services_code_action::insert_type_utils::info::T::DefaultAny,
                                        );
                                        let flowfixme_ast = codemod_context::typed::flowfixme_ast(
                                            &self.mapper.lint_severities,
                                        );
                                        class::Property {
                                            annot: types::AnnotationOrHint::Available(
                                                ast::types::Annotation {
                                                    loc: Loc::none(),
                                                    annotation: flowfixme_ast,
                                                },
                                            ),
                                            ..prop.clone()
                                        }
                                    }
                                    Err(_) => prop.clone(),
                                }
                            }
                        };
                        if prop_ == prop {
                            elem
                        } else {
                            class::BodyElement::Property(prop_)
                        }
                    }
                }
            }
            class::BodyElement::PrivateField(_) => elem,
            class::BodyElement::Method(_) => elem,
            class::BodyElement::StaticBlock(_) => elem,
            class::BodyElement::DeclareMethod(_) => elem,
            class::BodyElement::AbstractMethod(_) => elem,
            class::BodyElement::AbstractProperty(_) => elem,
            class::BodyElement::IndexSignature(_) => elem,
        }
    }

    fn map_variable_declarator_pattern(
        &mut self,
        kind: VariableKind,
        expr: &'ast pattern::Pattern<Loc, Loc>,
    ) -> pattern::Pattern<Loc, Loc> {
        match (expr, &kind) {
            (pattern::Pattern::Expression { .. }, _)
            | (pattern::Pattern::Object { .. }, _)
            | (pattern::Pattern::Array { .. }, _) => {
                ast_visitor::map_variable_declarator_pattern_default(self, kind, expr)
            }
            (pattern::Pattern::Identifier { inner, .. }, _)
                if matches!(&inner.annot, types::AnnotationOrHint::Available(_)) =>
            {
                ast_visitor::map_variable_declarator_pattern_default(self, kind, expr)
            }
            (pattern::Pattern::Identifier { inner, .. }, VariableKind::Const)
                if matches!(&inner.annot, types::AnnotationOrHint::Missing(_)) =>
            {
                ast_visitor::map_variable_declarator_pattern_default(self, kind, expr)
            }
            (
                pattern::Pattern::Identifier { loc, inner },
                VariableKind::Var | VariableKind::Let,
            ) if matches!(&inner.annot, types::AnnotationOrHint::Missing(_)) => {
                let name_loc = &inner.name.loc;
                match self.sig_verification_loc_tys.get(name_loc).cloned() {
                    None => ast_visitor::map_variable_declarator_pattern_default(self, kind, expr),
                    Some(ty) => {
                        let mapped = self.map_pattern_identifier(Some(kind), inner);
                        let annot_ =
                            self.add_annot_to_missing(loc.clone(), &ty, inner.annot.clone());
                        pattern::Pattern::Identifier {
                            loc: loc.clone(),
                            inner: Arc::new(pattern::Identifier {
                                name: mapped.name,
                                annot: annot_,
                                optional: inner.optional,
                            }),
                        }
                    }
                }
            }
            _ => ast_visitor::map_variable_declarator_pattern_default(self, kind, expr),
        }
    }

    fn map_arrow_function(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::function::Function<Loc, Loc>,
    ) -> ast::function::Function<Loc, Loc> {
        let ploc = if expr.params.params.len() == 1
            && expr.params.rest.is_none()
            && expr.params.this_.is_none()
        {
            match &expr.params.params[0] {
                ast::function::Param::RegularParam {
                    argument: pattern::Pattern::Identifier { inner, .. },
                    ..
                } if matches!(&inner.annot, types::AnnotationOrHint::Missing(_)) => {
                    match &inner.annot {
                        types::AnnotationOrHint::Missing(ploc) => Some(ploc),
                        _ => None,
                    }
                }
                _ => None,
            }
        } else {
            None
        };
        let rloc = match &expr.return_ {
            ast::function::ReturnAnnot::Missing(rloc) => Some(rloc),
            _ => None,
        };
        match (ploc, rloc) {
            (Some(ploc), Some(rloc)) if ploc == rloc => {
                if self.sig_verification_loc_tys.contains_key(rloc) {
                    self.mapper.wont_annotate_locs.insert(rloc.clone());
                    self.mapper.acc.warn(
                        loc,
                        &flow_services_code_action::insert_type_utils::warning::Kind::SkippingArrowFunction,
                    );
                }
                expr.clone()
            }
            _ => self.map_function_(loc, expr),
        }
    }

    fn map_program(&mut self, prog: &'ast ast::Program<Loc, Loc>) -> ast::Program<Loc, Loc> {
        let (total_errors_, sig_verification_loc_tys_) =
            signature_verification::collect_annotations(
                self.mapper.cctx,
                self.default_any,
                self.max_type_size,
                prog,
            );
        self.total_errors = total_errors_;
        self.sig_verification_loc_tys = sig_verification_loc_tys_;
        if self.sig_verification_loc_tys.is_empty() {
            prog.clone()
        } else {
            ast_visitor::map_program_default(self, prog)
        }
    }
}
