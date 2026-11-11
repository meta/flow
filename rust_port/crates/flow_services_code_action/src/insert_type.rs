/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::files::FileOptions;
use flow_common_errors::error_utils::ConcreteLocPrintableErrorSet;
use flow_common_ty::ty::ALocTy;
use flow_common_ty::ty_serializer;
use flow_common_ty::ty_utils;
use flow_parser::ast;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc;
use flow_parser_utils::file_sig::FileSig;
use flow_parser_utils::flow_ast_differ;
use flow_parser_utils_output::js_layout_generator;
use flow_parser_utils_output::pretty_printer;
use flow_parser_utils_output::replacement_printer;
use flow_typing::query_types;
use flow_typing::typed_ast_finder;
use flow_typing_context::Context;
use flow_typing_flow_js::natural_inference;
use flow_typing_flow_js::natural_inference::SingletonAction;

use crate::insert_type_imports;
use crate::insert_type_utils;
use crate::insert_type_utils::error;

#[derive(Debug, Clone)]
pub enum Unexpected {
    UnknownTypeAtPoint(Loc),
    NoFileInLocation(Loc),
    FailedToSerialize { ty: ALocTy, error_message: String },
    FailedToNormalizeNoMatch,
}

#[derive(Debug, Clone)]
pub enum Expected {
    TypeAnnotationAtPoint {
        location: Loc,
        type_ast: ast::types::Type<Loc, Loc>,
    },
    InvalidAnnotationTarget(Loc),
    UnsupportedAnnotation {
        location: Loc,
        error_message: String,
    },
    FailedToValidateType {
        error: error::ValidationError,
        error_message: String,
    },
    FailedToTypeCheck(ConcreteLocPrintableErrorSet),
    FailedToNormalize(Loc, String),
    FailedToImport(error::ImportError),
}

#[derive(Debug, Clone)]
pub enum Errors {
    Unexpected(Unexpected),
    Expected(Expected),
}

pub type InsertTypeResult<T> = Result<T, Errors>;

fn expected(err: Expected) -> Errors {
    Errors::Expected(err)
}

fn unexpected(err: Unexpected) -> Errors {
    Errors::Unexpected(err)
}

pub fn simplify(ty: ALocTy) -> ALocTy {
    flow_common_ty::ty_utils::simplify_type(true, Some(true), ty)
}

pub fn serialize(
    cx: &mut Context<'_>,
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    get_ast_from_heap: &dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>,
    file_sig: &FileSig,
    typed_ast: &ast::Program<ALoc, (ALoc, flow_typing_type::type_::Type)>,
    loc: Loc,
    ty: ALocTy,
) -> ast::types::Type<Loc, Loc> {
    let empty_severities = flow_lint_settings::lint_settings::LintSettings::<
        flow_lint_settings::severity::Severity,
    >::empty_severities();
    let mut mapper = insert_type_utils::TypeNormalizationHardcodedFixesMapper::new(
        cx,
        loc_of_aloc,
        get_ast_from_heap,
        file_sig,
        typed_ast,
        &empty_severities,
        false,
        true,
        true,
        &|_, _| (),
    );
    let ty = mapper.on_t(&loc, ty);
    let ty = simplify(ty);
    let ty = ty_serializer::type_(&ty);
    insert_type_utils::patch_up_type_ast(&ty)
}

pub fn path_of_loc(error: Option<Result<String, String>>, loc: &Loc) -> Result<String, String> {
    match &loc.source {
        Some(src) => Ok(src.to_path_buf().to_string_lossy().to_string()),
        None => error.unwrap_or_else(|| Err("no path for location".to_string())),
    }
}

pub struct Mapper<F: FnMut(Loc) -> Result<(Loc, ast::types::Type<Loc, Loc>), Expected>> {
    strict: bool,
    synth_type: F,
    target: Loc,
    target_is_point: bool,
    error: Option<Errors>,
    changed: bool,
}

impl<F: FnMut(Loc) -> Result<(Loc, ast::types::Type<Loc, Loc>), Expected>> Mapper<F> {
    pub fn new(strict: bool, synth_type: F, target: Loc) -> Self {
        let target_is_point = insert_type_utils::is_point(&target);
        Mapper {
            strict,
            synth_type,
            target,
            target_is_point,
            error: None,
            changed: false,
        }
    }

    #[allow(dead_code)]
    fn target_contains(&self, loc: &Loc) -> bool {
        self.target.contains(loc)
    }

    fn target_contained_by(&self, loc: &Loc) -> bool {
        loc.contains(&self.target)
    }

    fn is_target(&self, loc: &Loc) -> bool {
        self.target == *loc
    }

    fn synth_type_annotation_hint(&mut self, loc: Loc) -> ast::types::AnnotationOrHint<Loc, Loc> {
        let loc_clone = loc.dupe();
        match (self.synth_type)(loc) {
            Ok((l, type_ast)) => ast::types::AnnotationOrHint::Available(ast::types::Annotation {
                loc: l,
                annotation: type_ast,
            }),
            Err(_) => ast::types::AnnotationOrHint::Missing(loc_clone),
        }
    }

    fn synth_return_type_annotation_hint(
        &mut self,
        loc: Loc,
    ) -> ast::function::ReturnAnnot<Loc, Loc> {
        let loc_clone = loc.dupe();
        match (self.synth_type)(loc) {
            Ok((l, type_ast)) => ast::function::ReturnAnnot::Available(ast::types::Annotation {
                loc: l,
                annotation: type_ast,
            }),
            Err(_) => ast::function::ReturnAnnot::Missing(loc_clone),
        }
    }

    fn update_type_annotation_hint(
        &mut self,
        type_loc: Option<Loc>,
        check_loc: bool,
        annot: ast::types::AnnotationOrHint<Loc, Loc>,
    ) -> Result<ast::types::AnnotationOrHint<Loc, Loc>, Errors> {
        match annot {
            ast::types::AnnotationOrHint::Missing(location)
                if !check_loc || self.target_contained_by(&location) =>
            {
                let type_loc = match type_loc {
                    Some(type_loc) => type_loc,
                    None => location,
                };
                self.changed = true;
                Ok(self.synth_type_annotation_hint(type_loc))
            }
            ast::types::AnnotationOrHint::Available(ast::types::Annotation {
                loc: location,
                annotation: type_ast,
            }) if !check_loc || self.target_contained_by(&location) => {
                Err(expected(Expected::TypeAnnotationAtPoint {
                    location,
                    type_ast,
                }))
            }
            _ => Ok(annot),
        }
    }

    #[allow(dead_code)]
    fn type_annotation_hint(
        &mut self,
        annot: ast::types::AnnotationOrHint<Loc, Loc>,
    ) -> Result<ast::types::AnnotationOrHint<Loc, Loc>, Errors> {
        self.update_type_annotation_hint(None, true, annot)
    }

    fn component_renders_annotation(
        &mut self,
        renders: ast::types::ComponentRendersAnnotation<Loc, Loc>,
    ) -> Result<ast::types::ComponentRendersAnnotation<Loc, Loc>, Errors> {
        match &renders {
            ast::types::ComponentRendersAnnotation::MissingRenders(loc)
                if self.target_contained_by(loc) =>
            {
                match (self.synth_type)(loc.dupe()) {
                    Ok((l, ref ty)) if let ast::types::TypeInner::Renders { inner, .. } = &**ty => {
                        self.changed = true;
                        Ok(ast::types::ComponentRendersAnnotation::AvailableRenders(
                            l,
                            inner.as_ref().clone(),
                        ))
                    }
                    Ok(_) | Err(_) => Ok(renders),
                }
            }
            _ => Ok(
                flow_parser::ast_visitor::map_component_renders_annotation_default(self, &renders),
            ),
        }
    }

    fn function_return_annotation(
        &mut self,
        return_annot: ast::function::ReturnAnnot<Loc, Loc>,
    ) -> Result<ast::function::ReturnAnnot<Loc, Loc>, Errors> {
        match return_annot {
            ast::function::ReturnAnnot::Missing(loc) if self.target_contained_by(&loc) => {
                self.changed = true;
                Ok(self.synth_return_type_annotation_hint(loc))
            }
            ast::function::ReturnAnnot::Available(ast::types::Annotation {
                loc: location,
                annotation: type_ast,
            }) if self.target_contained_by(&location) => {
                Err(expected(Expected::TypeAnnotationAtPoint {
                    location,
                    type_ast,
                }))
            }
            _ => Ok(
                flow_parser::ast_visitor::map_function_return_annotation_default(
                    self,
                    &return_annot,
                ),
            ),
        }
    }

    fn function_param_pattern(
        &mut self,
        node: ast::pattern::Pattern<Loc, Loc>,
    ) -> Result<ast::pattern::Pattern<Loc, Loc>, Errors> {
        match &node {
            ast::pattern::Pattern::Identifier { loc, inner }
                if self.is_target(loc)
                    || (self.target_is_point && self.target_contained_by(loc)) =>
            {
                if self.strict {
                    Err(expected(Expected::UnsupportedAnnotation {
                        location: loc.dupe(),
                        error_message: "Function parameter in strict mode.".to_string(),
                    }))
                } else {
                    let annot =
                        self.update_type_annotation_hint(None, false, inner.annot.clone())?;
                    Ok(ast::pattern::Pattern::Identifier {
                        loc: loc.dupe(),
                        inner: Arc::new(ast::pattern::Identifier {
                            annot,
                            ..(**inner).clone()
                        }),
                    })
                }
            }
            _ => Ok(flow_parser::ast_visitor::map_function_param_pattern_default(self, &node)),
        }
    }

    fn class_element(
        &mut self,
        elem: ast::class::BodyElement<Loc, Loc>,
    ) -> Result<ast::class::BodyElement<Loc, Loc>, Errors> {
        match &elem {
            ast::class::BodyElement::PrivateField(pf) if self.is_target(&pf.loc) => {
                Err(expected(Expected::UnsupportedAnnotation {
                    location: pf.loc.dupe(),
                    error_message: "Private field".to_string(),
                }))
            }
            ast::class::BodyElement::Property(prop) if self.is_target(&prop.loc) => {
                let prop = prop.clone();
                let annot = self.update_type_annotation_hint(None, false, prop.annot.clone())?;
                Ok(ast::class::BodyElement::Property(ast::class::Property {
                    annot,
                    ..prop
                }))
            }
            ast::class::BodyElement::Property(prop) => {
                let kloc = match &prop.key {
                    ast::expression::object::Key::StringLiteral((kloc, _)) => Some(kloc.dupe()),
                    ast::expression::object::Key::NumberLiteral((kloc, _)) => Some(kloc.dupe()),
                    ast::expression::object::Key::BigIntLiteral((kloc, _)) => Some(kloc.dupe()),
                    ast::expression::object::Key::Identifier(id) => Some(id.loc.dupe()),
                    ast::expression::object::Key::PrivateName(pn) => Some(pn.loc.dupe()),
                    ast::expression::object::Key::Computed(_) => None,
                };
                match kloc {
                    Some(kloc)
                        if self.is_target(&kloc)
                            || (self.target_is_point && self.target_contained_by(&kloc)) =>
                    {
                        if self.strict {
                            Err(expected(Expected::UnsupportedAnnotation {
                                location: kloc,
                                error_message: "property key in strict mode".to_string(),
                            }))
                        } else {
                            let prop = prop.clone();
                            let annot =
                                self.update_type_annotation_hint(None, false, prop.annot.clone())?;
                            Ok(ast::class::BodyElement::Property(ast::class::Property {
                                annot,
                                ..prop
                            }))
                        }
                    }
                    _ => Ok(flow_parser::ast_visitor::map_class_element_default(
                        self, &elem,
                    )),
                }
            }
            _ => Ok(flow_parser::ast_visitor::map_class_element_default(
                self, &elem,
            )),
        }
    }

    fn variable_declarator(
        &mut self,
        kind: ast::VariableKind,
        decl: ast::statement::variable::Declarator<Loc, Loc>,
    ) -> Result<ast::statement::variable::Declarator<Loc, Loc>, Errors> {
        match (&kind, &decl) {
            (
                ast::VariableKind::Const,
                ast::statement::variable::Declarator {
                    loc: _dloc,
                    id:
                        ast::pattern::Pattern::Identifier {
                            loc: iloc,
                            inner: id,
                        },
                    init: Some(init_expr),
                    definite: _,
                },
            ) if self.is_target(init_expr.loc()) => {
                let type_loc = init_expr.loc().dupe();
                let annot =
                    self.update_type_annotation_hint(Some(type_loc), false, id.annot.clone())?;
                Ok(ast::statement::variable::Declarator {
                    id: ast::pattern::Pattern::Identifier {
                        loc: iloc.dupe(),
                        inner: Arc::new(ast::pattern::Identifier {
                            annot,
                            ..(**id).clone()
                        }),
                    },
                    ..decl.clone()
                })
            }
            _ => Ok(flow_parser::ast_visitor::map_variable_declarator_default(
                self, kind, &decl,
            )),
        }
    }

    fn variable_declarator_pattern(
        &mut self,
        kind: ast::VariableKind,
        node: ast::pattern::Pattern<Loc, Loc>,
    ) -> Result<ast::pattern::Pattern<Loc, Loc>, Errors> {
        let loc = node.loc();
        if !self.target_contained_by(loc) {
            return Ok(node);
        }
        match (&node, &kind) {
            (ast::pattern::Pattern::Identifier { .. }, ast::VariableKind::Const) if self.strict => {
                Ok(
                    flow_parser::ast_visitor::map_variable_declarator_pattern_default(
                        self, kind, &node,
                    ),
                )
            }
            (
                ast::pattern::Pattern::Identifier { loc, inner: id },
                ast::VariableKind::Var | ast::VariableKind::Let | ast::VariableKind::Const,
            ) if self.target_is_point || self.is_target(loc) => {
                let type_loc = id.name.loc.dupe();
                let annot =
                    self.update_type_annotation_hint(Some(type_loc), false, id.annot.clone())?;
                Ok(ast::pattern::Pattern::Identifier {
                    loc: loc.dupe(),
                    inner: Arc::new(ast::pattern::Identifier {
                        annot,
                        ..(**id).clone()
                    }),
                })
            }
            _ => Ok(
                flow_parser::ast_visitor::map_variable_declarator_pattern_default(
                    self, kind, &node,
                ),
            ),
        }
    }

    #[allow(dead_code)]
    fn expression(
        &mut self,
        e: ast::expression::Expression<Loc, Loc>,
    ) -> Result<ast::expression::Expression<Loc, Loc>, Errors> {
        let l = e.loc().dupe();
        if self.target_contained_by(&l) {
            if self.is_target(&l) {
                let l2 = l.dupe();
                match (self.synth_type)(l) {
                    Err(_) => Ok(e),
                    Ok((annot_loc, annot_type)) => Ok(ast::expression::Expression::new(
                        ast::expression::ExpressionInner::AsExpression {
                            loc: l2,
                            inner: Arc::new(ast::expression::AsExpression {
                                expression: e,
                                annot: ast::types::Annotation {
                                    loc: annot_loc,
                                    annotation: annot_type,
                                },
                                comments: None,
                            }),
                        },
                    )),
                }
            } else {
                Ok(e)
            }
        } else {
            Ok(e)
        }
    }

    fn store_error(&mut self, err: Errors) {
        if self.error.is_none() {
            self.error = Some(err);
        }
    }

    pub fn program(
        &mut self,
        p: &ast::Program<Loc, Loc>,
    ) -> Result<ast::Program<Loc, Loc>, Errors> {
        use flow_parser::ast_visitor::AstVisitor;
        let p_prime = self.map_program(p);
        if let Some(err) = self.error.take() {
            return Err(err);
        }
        if !self.changed {
            return Err(expected(Expected::InvalidAnnotationTarget(
                self.target.dupe(),
            )));
        }
        Ok(p_prime)
    }
}

impl<'ast, F: FnMut(Loc) -> Result<(Loc, ast::types::Type<Loc, Loc>), Expected>>
    flow_parser::ast_visitor::AstVisitor<'ast, Loc> for Mapper<F>
{
    fn normalize_loc(loc: &Loc) -> &Loc {
        loc
    }

    fn normalize_type(type_: &Loc) -> &Loc {
        type_
    }

    fn map_program(&mut self, program: &'ast ast::Program<Loc, Loc>) -> ast::Program<Loc, Loc> {
        if self.target_contained_by(&program.loc)
            || program
                .all_comments
                .iter()
                .any(|c| self.target_contained_by(&c.loc))
        {
            flow_parser::ast_visitor::map_program_default(self, program)
        } else {
            program.clone()
        }
    }

    fn map_statement(
        &mut self,
        stmt: &'ast ast::statement::Statement<Loc, Loc>,
    ) -> ast::statement::Statement<Loc, Loc> {
        if self.target_contained_by(stmt.loc()) {
            flow_parser::ast_visitor::map_statement_default(self, stmt)
        } else {
            stmt.dupe()
        }
    }

    fn map_expression(
        &mut self,
        e: &'ast ast::expression::Expression<Loc, Loc>,
    ) -> ast::expression::Expression<Loc, Loc> {
        if self.error.is_some() {
            return e.dupe();
        }
        let l = e.loc().dupe();
        if self.target_contained_by(&l) {
            if self.is_target(&l) {
                let l2 = l.dupe();
                match (self.synth_type)(l) {
                    Err(_) => flow_parser::ast_visitor::map_expression_default(self, e),
                    Ok((annot_loc, annot_type)) => {
                        self.changed = true;
                        ast::expression::Expression::new(
                            ast::expression::ExpressionInner::AsExpression {
                                loc: l2,
                                inner: Arc::new(ast::expression::AsExpression {
                                    expression: e.dupe(),
                                    annot: ast::types::Annotation {
                                        loc: annot_loc,
                                        annotation: annot_type,
                                    },
                                    comments: None,
                                }),
                            },
                        )
                    }
                }
            } else {
                flow_parser::ast_visitor::map_expression_default(self, e)
            }
        } else {
            e.dupe()
        }
    }

    fn map_type_annotation_hint(
        &mut self,
        hint: &'ast ast::types::AnnotationOrHint<Loc, Loc>,
    ) -> ast::types::AnnotationOrHint<Loc, Loc> {
        if self.error.is_some() {
            return hint.clone();
        }
        match self.update_type_annotation_hint(None, true, hint.clone()) {
            Ok(result) => result,
            Err(err) => {
                self.store_error(err);
                hint.clone()
            }
        }
    }

    fn map_component_renders_annotation(
        &mut self,
        renders: &'ast ast::types::ComponentRendersAnnotation<Loc, Loc>,
    ) -> ast::types::ComponentRendersAnnotation<Loc, Loc> {
        if self.error.is_some() {
            return renders.clone();
        }
        match self.component_renders_annotation(renders.clone()) {
            Ok(result) => result,
            Err(err) => {
                self.store_error(err);
                renders.clone()
            }
        }
    }

    fn map_function_return_annotation(
        &mut self,
        return_: &'ast ast::function::ReturnAnnot<Loc, Loc>,
    ) -> ast::function::ReturnAnnot<Loc, Loc> {
        if self.error.is_some() {
            return return_.clone();
        }
        match self.function_return_annotation(return_.clone()) {
            Ok(result) => result,
            Err(err) => {
                self.store_error(err);
                return_.clone()
            }
        }
    }

    fn map_function_param_pattern(
        &mut self,
        pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
    ) -> ast::pattern::Pattern<Loc, Loc> {
        if self.error.is_some() {
            return pattern.clone();
        }
        match self.function_param_pattern(pattern.clone()) {
            Ok(result) => result,
            Err(err) => {
                self.store_error(err);
                pattern.clone()
            }
        }
    }

    fn map_class_element(
        &mut self,
        elem: &'ast ast::class::BodyElement<Loc, Loc>,
    ) -> ast::class::BodyElement<Loc, Loc> {
        if self.error.is_some() {
            return elem.clone();
        }
        match self.class_element(elem.clone()) {
            Ok(result) => result,
            Err(err) => {
                self.store_error(err);
                elem.clone()
            }
        }
    }

    fn map_variable_declarator(
        &mut self,
        kind: ast::VariableKind,
        declarator: &'ast ast::statement::variable::Declarator<Loc, Loc>,
    ) -> ast::statement::variable::Declarator<Loc, Loc> {
        if self.error.is_some() {
            return declarator.clone();
        }
        match self.variable_declarator(kind, declarator.clone()) {
            Ok(result) => result,
            Err(err) => {
                self.store_error(err);
                declarator.clone()
            }
        }
    }

    fn map_variable_declarator_pattern(
        &mut self,
        kind: ast::VariableKind,
        pattern: &'ast ast::pattern::Pattern<Loc, Loc>,
    ) -> ast::pattern::Pattern<Loc, Loc> {
        if self.error.is_some() {
            return pattern.clone();
        }
        match self.variable_declarator_pattern(kind, pattern.clone()) {
            Ok(result) => result,
            Err(err) => {
                self.store_error(err);
                pattern.clone()
            }
        }
    }
}

pub fn type_lookup_at_location(
    typed_ast: &ast::Program<ALoc, (ALoc, flow_typing_type::type_::Type)>,
    loc: Loc,
) -> InsertTypeResult<flow_typing_type::type_::Type> {
    match typed_ast_finder::find_exact_match_annotation(typed_ast, ALoc::of_loc(loc.dupe())) {
        Some(p) => Ok(p),
        None => Err(unexpected(Unexpected::UnknownTypeAtPoint(loc))),
    }
}

pub fn normalize<'a, 'cx>(
    cx: &'a mut Context<'cx>,
    file_sig: &Arc<FileSig>,
    typed_ast: &ast::Program<ALoc, (ALoc, flow_typing_type::type_::Type)>,
    omit_targ_defaults: bool,
    loc: Loc,
    t: &flow_typing_type::type_::Type,
) -> Result<flow_common_ty::ty::ALocElt, Errors> {
    match query_types::insert_type_normalize(
        cx,
        file_sig.dupe(),
        omit_targ_defaults,
        typed_ast,
        loc,
        t,
    ) {
        query_types::QueryResult::FailureNoMatch => {
            Err(unexpected(Unexpected::FailedToNormalizeNoMatch))
        }
        query_types::QueryResult::FailureUnparseable(loc, _, msg) => {
            Err(expected(Expected::FailedToNormalize(loc, msg)))
        }
        query_types::QueryResult::Success(_, ty) => Ok(ty),
    }
}

pub fn synth_type<'a, 'cx>(
    size_limit: Option<usize>,
    cx: &'a mut Context<'cx>,
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    get_ast_from_heap: &dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>,
    file_sig: &Arc<FileSig>,
    typed_ast: &ast::Program<ALoc, (ALoc, flow_typing_type::type_::Type)>,
    omit_targ_defaults: bool,
    remote_converter: &mut insert_type_imports::imports_helper::RemoteConverter<'_>,
    type_loc: Loc,
    t: flow_typing_type::type_::Type,
) -> Result<(Loc, ast::types::Type<Loc, Loc>), Expected> {
    let size_limit = size_limit.unwrap_or(30);
    let process = |ty: ALocTy| -> Result<ALocTy, Expected> {
        let (_, errors) =
            insert_type_utils::validator::validate_type(size_limit, loc_of_aloc, ty.dupe());
        match errors.first() {
            Some(error) => {
                // TODO surface all errors
                let error_message = error::serialize_validation_error(error);
                let err = Expected::FailedToValidateType {
                    error: error.clone(),
                    error_message,
                };
                Err(err)
            }
            None => Ok(ty),
        }
    };
    let t = natural_inference::convert_literal_type(cx, |_, _| SingletonAction::DoNotKeep, t);
    let elt = normalize(
        cx,
        file_sig,
        typed_ast,
        omit_targ_defaults,
        type_loc.dupe(),
        &t,
    )
    .map_err(|e| match e {
        Errors::Expected(e) => e,
        Errors::Unexpected(u) => {
            Expected::FailedToNormalize(type_loc.dupe(), format!("Unexpected: {:?}", u))
        }
    })?;
    match ty_utils::typify_elt(elt) {
        None => Err(Expected::FailedToNormalize(
            type_loc.dupe(),
            "Non-type".to_string(),
        )),
        Some(ty) => match process(ty.dupe()) {
            Err(err) => Err(err),
            Ok(ty) => {
                let import_fixed_ty = match remote_converter.type_(ty.dupe()) {
                    Ok(ty) => ty,
                    Err(e) => {
                        flow_hh_logger::error!("Insert type: {}", error::serialize(&e));
                        ty
                    }
                };
                let ast = serialize(
                    cx,
                    loc_of_aloc,
                    get_ast_from_heap,
                    file_sig,
                    typed_ast,
                    type_loc.dupe(),
                    import_fixed_ty,
                );
                Ok((type_loc, ast))
            }
        },
    }
}

fn type_to_string(t: &ast::types::Type<Loc, Loc>) -> String {
    let layout = js_layout_generator::type_(&js_layout_generator::default_opts(), t);
    let source = pretty_printer::print(true, &layout);
    source.contents()
}

fn unexpected_error_to_string(err: &Unexpected) -> String {
    match err {
        Unexpected::NoFileInLocation(_) => {
            "Target passed to insert-type without source in location".to_string()
        }
        Unexpected::UnknownTypeAtPoint(_) => {
            "Couldn't locate a type for this annotation".to_string()
        }
        Unexpected::FailedToSerialize { error_message, .. } => {
            format!("couldn't print type: {error_message}")
        }
        Unexpected::FailedToNormalizeNoMatch => {
            "couldn't print type: couldn't locate a type for this annotation".to_string()
        }
    }
}

fn expected_error_to_string(err: &Expected) -> String {
    match err {
        Expected::TypeAnnotationAtPoint { location, type_ast } => {
            format!(
                "Preexisiting type annotation at {}: {}",
                location.to_string_no_source(),
                type_to_string(type_ast),
            )
        }
        Expected::InvalidAnnotationTarget(location) => {
            format!(
                "Did not find an annotation at {}",
                location.to_string_no_source()
            )
        }
        Expected::UnsupportedAnnotation {
            location,
            error_message,
        } => {
            format!(
                "{error_message} found at {} is not currently supported",
                location.to_string_no_source()
            )
        }
        Expected::FailedToTypeCheck(_) => "Failed to typecheck file".to_string(),
        Expected::FailedToValidateType {
            error: error::ValidationError::TooBig { size_limit, size },
            ..
        } => {
            let size_str = match size {
                Some(size) => size.to_string(),
                None => format!(
                    ">{}",
                    insert_type_utils::validator::VALIDATE_TYPE_TOO_BIG_MAX
                ),
            };
            format!(
                "The type that would be generated (size: {size_str}) exceeds the size limit ({size_limit})"
            )
        }
        Expected::FailedToValidateType { error_message, .. } => {
            format!("Failed to validate type: {error_message}")
        }
        Expected::FailedToNormalize(_, msg) => format!("couldn't print type: {msg}"),
        Expected::FailedToImport(e) => {
            format!(
                "failed to import needed type: {}",
                error::serialize_import_error(e)
            )
        }
    }
}

pub fn error_to_string(errors: &Errors) -> String {
    match errors {
        Errors::Unexpected(err) => {
            format!(
                "flow autofix insert-type: {}",
                unexpected_error_to_string(err)
            )
        }
        Errors::Expected(err) => {
            format!(
                "flow autofix insert-type: {}",
                expected_error_to_string(err)
            )
        }
    }
}

pub fn add_statement_after_directive_and_type_imports(
    stmts: &[ast::statement::Statement<Loc, Loc>],
    new_imports: Vec<ast::statement::Statement<Loc, Loc>>,
) -> Vec<ast::statement::Statement<Loc, Loc>> {
    let flow_parser_utils::flow_ast_differ::PartitionResult {
        directives,
        imports,
        body,
    } = flow_ast_differ::partition_imports(stmts);
    let mut result = directives;
    result.extend(new_imports);
    result.extend(imports);
    result.extend(body);
    result
}

pub fn add_imports(
    remote_converter: &insert_type_imports::imports_helper::RemoteConverter<'_>,
    stmts: &[ast::statement::Statement<Loc, Loc>],
) -> Vec<ast::statement::Statement<Loc, Loc>> {
    let new_imports = remote_converter.to_import_stmts();
    add_statement_after_directive_and_type_imports(stmts, new_imports)
}

pub fn insert_type_custom_synth_type<'a>(
    file_options: Arc<FileOptions>,
    loc_of_aloc: &'a dyn Fn(&ALoc) -> Loc,
    get_haste_module_info: &'a dyn Fn(&FileKey) -> Option<flow_common_modulename::HasteModuleInfo>,
    get_type_sig: &'a dyn Fn(
        &FileKey,
    ) -> Option<
        flow_type_sig::packed_type_sig::Module<
            flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
        >,
    >,
    strict: bool,
    synth_type: &mut dyn FnMut(
        &mut insert_type_imports::imports_helper::RemoteConverter<'_>,
        Loc,
    ) -> Result<(Loc, ast::types::Type<Loc, Loc>), Expected>,
    remote_converter: Option<&mut insert_type_imports::imports_helper::RemoteConverter<'a>>,
    ast: &ast::Program<Loc, Loc>,
    target: Loc,
) -> InsertTypeResult<ast::Program<Loc, Loc>> {
    let file = match &target.source {
        Some(source) => source.dupe(),
        None => return Err(unexpected(Unexpected::NoFileInLocation(target))),
    };
    let mut owned_rc;
    let (rc, should_add_imports) = match remote_converter {
        Some(rc) => (rc, false),
        None => {
            owned_rc = insert_type_imports::imports_helper::RemoteConverter::new(
                Box::new(|aloc| loc_of_aloc(aloc)),
                file_options,
                Box::new(|fk| get_haste_module_info(fk)),
                Box::new(|fk| get_type_sig(fk)),
                0, // iteration
                file,
                std::collections::BTreeSet::new(), // reserved_names
            );
            (&mut owned_rc, true)
        }
    };
    let rc_cell = std::cell::RefCell::new(rc);
    let synth_type_bound = |loc: Loc| -> Result<(Loc, ast::types::Type<Loc, Loc>), Expected> {
        synth_type(&mut rc_cell.borrow_mut(), loc)
    };
    let mut mapper = Mapper::new(strict, synth_type_bound, target);
    let ast_prime = mapper.program(ast)?;
    let rc = rc_cell.into_inner();
    let statements = if should_add_imports {
        add_imports(rc, &ast_prime.statements)
    } else {
        ast_prime.statements.to_vec()
    };
    Ok(ast::Program {
        statements: statements.into(),
        ..ast_prime
    })
}

pub fn insert_type_<'a, 'b>(
    cx: &mut Context<'a>,
    loc_of_aloc: &'b dyn Fn(&ALoc) -> Loc,
    get_ast_from_heap: &'b dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>,
    get_haste_module_info: &'b dyn Fn(&FileKey) -> Option<flow_common_modulename::HasteModuleInfo>,
    get_type_sig: &'b dyn Fn(
        &FileKey,
    ) -> Option<
        flow_type_sig::packed_type_sig::Module<
            flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
        >,
    >,
    file_sig: &Arc<FileSig>,
    typed_ast: &ast::Program<ALoc, (ALoc, flow_typing_type::type_::Type)>,
    omit_targ_defaults: bool,
    strict: bool,
    remote_converter: Option<&mut insert_type_imports::imports_helper::RemoteConverter<'b>>,
    ast: &ast::Program<Loc, Loc>,
    target: Loc,
    loc_to_type: &dyn Fn(Loc) -> InsertTypeResult<flow_typing_type::type_::Type>,
) -> InsertTypeResult<ast::Program<Loc, Loc>> {
    let file_options = cx.file_options();
    let mut synth_type_fn = |rc: &mut insert_type_imports::imports_helper::RemoteConverter<'_>,
                             location: Loc|
     -> Result<(Loc, ast::types::Type<Loc, Loc>), Expected> {
        let t = loc_to_type(location.dupe()).map_err(|e| match e {
            Errors::Expected(e) => e,
            Errors::Unexpected(u) => Expected::FailedToNormalize(location.dupe(), format!("{u:?}")),
        })?;
        synth_type(
            None,
            cx,
            loc_of_aloc,
            get_ast_from_heap,
            file_sig,
            typed_ast,
            omit_targ_defaults,
            rc,
            location,
            t,
        )
    };
    insert_type_custom_synth_type(
        file_options,
        loc_of_aloc,
        get_haste_module_info,
        get_type_sig,
        strict,
        &mut synth_type_fn,
        remote_converter,
        ast,
        target,
    )
}

pub fn insert_type<'a, 'b>(
    cx: &mut Context<'a>,
    loc_of_aloc: &'b dyn Fn(&ALoc) -> Loc,
    get_ast_from_heap: &'b dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>,
    get_haste_module_info: &'b dyn Fn(&FileKey) -> Option<flow_common_modulename::HasteModuleInfo>,
    get_type_sig: &'b dyn Fn(
        &FileKey,
    ) -> Option<
        flow_type_sig::packed_type_sig::Module<
            flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
        >,
    >,
    file_sig: &Arc<FileSig>,
    typed_ast: &ast::Program<ALoc, (ALoc, flow_typing_type::type_::Type)>,
    omit_targ_defaults: bool,
    strict: bool,
    remote_converter: Option<&mut insert_type_imports::imports_helper::RemoteConverter<'b>>,
    ast: &ast::Program<Loc, Loc>,
    target: Loc,
) -> InsertTypeResult<ast::Program<Loc, Loc>> {
    let loc_to_type = |loc: Loc| -> InsertTypeResult<flow_typing_type::type_::Type> {
        type_lookup_at_location(typed_ast, loc)
    };
    insert_type_(
        cx,
        loc_of_aloc,
        get_ast_from_heap,
        get_haste_module_info,
        get_type_sig,
        file_sig,
        typed_ast,
        omit_targ_defaults,
        strict,
        remote_converter,
        ast,
        target,
        &loc_to_type,
    )
}

pub fn insert_type_t<'a, 'b>(
    cx: &mut Context<'a>,
    loc_of_aloc: &'b dyn Fn(&ALoc) -> Loc,
    get_ast_from_heap: &'b dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>,
    get_haste_module_info: &'b dyn Fn(&FileKey) -> Option<flow_common_modulename::HasteModuleInfo>,
    get_type_sig: &'b dyn Fn(
        &FileKey,
    ) -> Option<
        flow_type_sig::packed_type_sig::Module<
            flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
        >,
    >,
    file_sig: &Arc<FileSig>,
    typed_ast: &ast::Program<ALoc, (ALoc, flow_typing_type::type_::Type)>,
    omit_targ_defaults: bool,
    strict: bool,
    remote_converter: Option<&mut insert_type_imports::imports_helper::RemoteConverter<'b>>,
    ast: &ast::Program<Loc, Loc>,
    target: Loc,
    type_t: flow_typing_type::type_::Type,
) -> InsertTypeResult<ast::Program<Loc, Loc>> {
    let loc_to_type =
        |_loc: Loc| -> InsertTypeResult<flow_typing_type::type_::Type> { Ok(type_t.dupe()) };
    insert_type_(
        cx,
        loc_of_aloc,
        get_ast_from_heap,
        get_haste_module_info,
        get_type_sig,
        file_sig,
        typed_ast,
        omit_targ_defaults,
        strict,
        remote_converter,
        ast,
        target,
        &loc_to_type,
    )
}

pub fn insert_type_ty<'a>(
    cx: &mut Context,
    loc_of_aloc: &'a dyn Fn(&ALoc) -> Loc,
    get_ast_from_heap: &dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>,
    get_haste_module_info: &'a dyn Fn(&FileKey) -> Option<flow_common_modulename::HasteModuleInfo>,
    get_type_sig: &'a dyn Fn(
        &FileKey,
    ) -> Option<
        flow_type_sig::packed_type_sig::Module<
            flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
        >,
    >,
    file_sig: &FileSig,
    typed_ast: &ast::Program<ALoc, (ALoc, flow_typing_type::type_::Type)>,
    strict: bool,
    remote_converter: Option<&mut insert_type_imports::imports_helper::RemoteConverter<'a>>,
    ast: &ast::Program<Loc, Loc>,
    target: Loc,
    ty: ALocTy,
) -> InsertTypeResult<ast::Program<Loc, Loc>> {
    let file_options = cx.file_options();
    let mut synth_type_fn = |rc: &mut insert_type_imports::imports_helper::RemoteConverter<'_>,
                             loc: Loc|
     -> Result<(Loc, ast::types::Type<Loc, Loc>), Expected> {
        let import_fixed_ty = match rc.type_(ty.dupe()) {
            Ok(ty) => ty,
            Err(e) => {
                flow_hh_logger::error!("Insert type: {}", error::serialize(&e));
                ty.dupe()
            }
        };
        let ast = serialize(
            cx,
            loc_of_aloc,
            get_ast_from_heap,
            file_sig,
            typed_ast,
            loc.dupe(),
            import_fixed_ty,
        );
        Ok((loc, ast))
    };
    insert_type_custom_synth_type(
        file_options,
        loc_of_aloc,
        get_haste_module_info,
        get_type_sig,
        strict,
        &mut synth_type_fn,
        remote_converter,
        ast,
        target,
    )
}

pub fn mk_diff(
    ast: &ast::Program<Loc, Loc>,
    new_ast: &ast::Program<Loc, Loc>,
) -> Vec<flow_ast_differ::NodeChange> {
    flow_ast_differ::program(ast, new_ast)
}

pub fn mk_patch(
    opts: &js_layout_generator::Opts,
    ast: &ast::Program<Loc, Loc>,
    new_ast: &ast::Program<Loc, Loc>,
    file_content: &str,
) -> replacement_printer::Patch {
    replacement_printer::mk_patch_ast_differ(opts, &mk_diff(ast, new_ast), file_content)
}
