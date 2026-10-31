/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::HashMap;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocMap;
use flow_common::alpha_rename;
use flow_common::flow_import_specifier::Userland;
use flow_common::flow_symbol::Symbol;
use flow_common::polarity::Polarity;
use flow_common::reason;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::subst_name::SubstName;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_env_builder::name_def_types::Import as NameDefImport;
use flow_parser::ast;
use flow_parser::polymorphic_ast_mapper;
use flow_parser::polymorphic_ast_mapper::LocMapper;
use flow_parser_utils::object_type_key;
use flow_parser_utils::object_type_key::ObjectTypeKeyForm;
use flow_typing_context::Context;
use flow_typing_errors::error_message::EIncorrectTypeWithReplacementData;
use flow_typing_errors::error_message::ETSSyntaxData;
use flow_typing_errors::error_message::ETypeGuardIncompatibleWithFunctionKindData;
use flow_typing_errors::error_message::ETypeGuardInvalidParameterData;
use flow_typing_errors::error_message::EVarianceKeywordData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::InternalError;
use flow_typing_errors::error_message::TypeGuardBindingKind;
use flow_typing_errors::error_message::TypeGuardParameterData;
use flow_typing_errors::intermediate_error_types;
use flow_typing_errors::intermediate_error_types::InternalType;
use flow_typing_errors::intermediate_error_types::InvalidObjKey;
use flow_typing_errors::intermediate_error_types::TsLibSyntaxKind;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_common::obj_type;
use flow_typing_flow_common::string_case_transform;
use flow_typing_flow_common::type_subst;
use flow_typing_flow_js::flow_js;
use flow_typing_flow_js::flow_js::FlowJs;
use flow_typing_flow_js::intersection_kit;
use flow_typing_flow_js::template_literal_type;
use flow_typing_loc_env::func_class_sig_types;
use flow_typing_loc_env::func_class_sig_types::class::ClassLikeBindingKind;
use flow_typing_type::type_;
use flow_typing_type::type_::EvalMappedTypeData;
use flow_typing_type::type_::RootUseOp;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeDestructorT;
use flow_typing_type::type_::TypeDestructorTInner;
use flow_typing_type::type_::TypeInner as FlowTypeInner;
use flow_typing_type::type_::TypeParam;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::eval::Id as EvalId;
use flow_typing_type::type_::type_or_type_desc::TypeOrTypeDescT;
use flow_typing_type::type_::unknown_use;
use flow_typing_type::type_util;
use flow_typing_utils::type_env;
use flow_typing_utils::typed_ast_utils;
use flow_utils_concurrency::job_error::JobError;

use crate::class_sig;

type FuncTypeParam = (Type, ast::types::function::Param<ALoc, (ALoc, Type)>);
type FuncTypeRest = (Type, ast::types::function::RestParam<ALoc, (ALoc, Type)>);
type FuncTypeThisParam = (Type, ast::types::function::ThisParam<ALoc, (ALoc, Type)>);

fn mapped_type_use_op(
    reason: &Reason,
    source: &Type,
    destructor: &type_::Destructor,
    eval_id: EvalId,
) -> UseOp {
    let mapped_type = Type::new(FlowTypeInner::EvalT {
        type_: source.dupe(),
        defer_use_t: TypeDestructorT::new(TypeDestructorTInner(
            unknown_use(),
            reason.dupe(),
            Rc::new(destructor.clone()),
        )),
        id: eval_id,
    });
    UseOp::Op(Arc::new(RootUseOp::EvalMappedType(Box::new(
        EvalMappedTypeData {
            mapped_type_loc: reason.loc.dupe(),
            mapped_type: TypeOrTypeDescT::Type(mapped_type),
        },
    ))))
}

fn try_map_vec<T, U, E>(items: &[T], mut f: impl FnMut(&T) -> Result<U, E>) -> Result<Vec<U>, E> {
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        result.push(f(item)?);
    }
    Ok(result)
}

fn try_map_pair_vecs<T, U, V, E>(
    items: &[T],
    mut f: impl FnMut(&T) -> Result<(U, V), E>,
) -> Result<(Vec<U>, Vec<V>), E> {
    let mut left = Vec::with_capacity(items.len());
    let mut right = Vec::with_capacity(items.len());
    for item in items {
        let (l, r) = f(item)?;
        left.push(l);
        right.push(r);
    }
    Ok((left, right))
}

fn func_type_id_name(id: &ast::Identifier<ALoc, (ALoc, Type)>) -> FlowSmolStr {
    id.name.dupe()
}

pub struct FuncTypeParamsConfig;

impl flow_typing_loc_env::func_class_sig_types::ConfigTypes for FuncTypeParamsConfig {
    type Param = FuncTypeParam;
    type Rest = FuncTypeRest;
    type ThisParam = FuncTypeThisParam;
    type ParamAst = ast::types::function::Param<ALoc, (ALoc, Type)>;
    type RestAst = ast::types::function::RestParam<ALoc, (ALoc, Type)>;
    type ThisAst = ast::types::function::ThisParam<ALoc, (ALoc, Type)>;
    type Ast = ast::types::function::Params<ALoc, (ALoc, Type)>;
}

impl crate::func_params_intf::Config for FuncTypeParamsConfig {
    fn param_type(param: &Self::Param) -> type_::FunParam {
        let (t, param_ast) = param;
        let (name, optional) = match &param_ast.param {
            ast::types::function::ParamKind::Anonymous(_) => (None, false),
            ast::types::function::ParamKind::Labeled { name, optional, .. } => {
                (Some(func_type_id_name(name)), *optional)
            }
            ast::types::function::ParamKind::Destructuring(pattern) => {
                (None, flow_parser::ast_utils::pattern_optional(pattern))
            }
        };
        let t = if optional {
            type_util::optional(t.dupe(), None, false)
        } else {
            t.dupe()
        };
        type_::FunParam(name, t)
    }

    fn rest_type(rest: &Self::Rest) -> type_::FunRestParam {
        let (t, rest_ast) = rest;
        let name = match &rest_ast.argument.param {
            ast::types::function::ParamKind::Labeled { name, .. } => Some(func_type_id_name(name)),
            _ => None,
        };
        type_::FunRestParam(name, rest_ast.loc.dupe(), t.dupe())
    }

    fn this_type(this: &Self::ThisParam) -> Type {
        this.0.dupe()
    }

    fn is_param_type_annotated(_param: &Self::Param) -> bool {
        true
    }

    fn is_rest_type_annotated(_rest: &Self::Rest) -> bool {
        true
    }

    fn subst_param<'a>(
        cx: &mut Context<'a>,
        map: &FlowOrdMap<SubstName, Type>,
        param: &Self::Param,
    ) -> Self::Param {
        let t = flow_js::subst(cx, None, None, None, map, param.0.dupe());
        (t, param.1.clone())
    }

    fn subst_rest<'a>(
        cx: &mut Context<'a>,
        map: &FlowOrdMap<SubstName, Type>,
        rest: &Self::Rest,
    ) -> Self::Rest {
        let t = flow_js::subst(cx, None, None, None, map, rest.0.dupe());
        (t, rest.1.clone())
    }

    fn subst_this<'a>(
        cx: &mut Context<'a>,
        map: &FlowOrdMap<SubstName, Type>,
        this: &Self::ThisParam,
    ) -> Self::ThisParam {
        let t = flow_js::subst(cx, None, None, None, map, this.0.dupe());
        (t, this.1.clone())
    }

    fn eval_param<'a>(
        _cx: &mut Context<'a>,
        param: &Self::Param,
    ) -> Result<Self::ParamAst, flow_typing_utils::abnormal::CheckExprError> {
        Ok(param.1.clone())
    }

    fn eval_rest<'a>(_cx: &mut Context<'a>, rest: &Self::Rest) -> Self::RestAst {
        rest.1.clone()
    }

    fn eval_this<'a>(_cx: &mut Context<'a>, this: &Self::ThisParam) -> Self::ThisAst {
        this.1.clone()
    }
}

type ComponentTypeParam = (
    Type,
    ast::types::component_params::Param<ALoc, (ALoc, Type)>,
);

type ComponentTypeRest = (
    Type,
    ast::types::component_params::RestParam<ALoc, (ALoc, Type)>,
);

struct TypeAnnotationConfig;

impl crate::component_params_intf::Config for TypeAnnotationConfig {
    type Param = ComponentTypeParam;
    type Rest = ComponentTypeRest;
    type ParamAst = ast::types::component_params::Param<ALoc, (ALoc, Type)>;
    type RestAst = ast::types::component_params::RestParam<ALoc, (ALoc, Type)>;

    fn param_type_with_name(param: &Self::Param) -> (ALoc, FlowSmolStr, Type) {
        let (t, param_ast) = param;
        let param_t = if param_ast.optional {
            type_util::optional(t.dupe(), None, false)
        } else {
            t.dupe()
        };
        use flow_parser::ast::statement::component_params::ParamName;
        match &param_ast.name {
            ParamName::Identifier(id) => {
                let (ref loc, _) = id.loc;
                (loc.dupe(), id.name.dupe(), param_t)
            }
            ParamName::StringLiteral((loc, lit)) => {
                (loc.dupe(), FlowSmolStr::new(&lit.value), param_t)
            }
        }
    }

    fn rest_type(rest: &Self::Rest) -> Type {
        rest.0.dupe()
    }

    fn eval_param<'a>(
        _cx: &mut Context<'a>,
        param: &Self::Param,
    ) -> Result<Self::ParamAst, flow_typing_utils::abnormal::CheckExprError> {
        Ok(param.1.clone())
    }

    fn eval_rest<'a>(
        _cx: &mut Context<'a>,
        rest: &Self::Rest,
    ) -> Result<Self::RestAst, flow_typing_utils::abnormal::CheckExprError> {
        Ok(rest.1.clone())
    }
}

// =========================================================================
// AST helpers
// =========================================================================

fn qualified_name(mut id: &ast::types::generic::Identifier<ALoc, ALoc>) -> String {
    let mut acc = Vec::new();
    loop {
        use flow_parser::ast::types::generic::Identifier;
        match id {
            Identifier::Unqualified(ident) => {
                acc.push(ident.name.to_string());
                acc.reverse();
                return acc.join(".");
            }
            Identifier::Qualified(q) => {
                acc.push(q.id.name.to_string());
                id = &q.qualification;
            }
            Identifier::ImportTypeAnnot(import) => {
                let value = &import.source.1.value;
                acc.push(format!("import(\"{}\")", value));
                acc.reverse();
                return acc.join(".");
            }
        }
    }
}

fn typeof_name(mut target: &ast::types::typeof_::Target<ALoc, ALoc>) -> String {
    use flow_parser::ast::types::typeof_::Target;

    let mut acc = Vec::new();
    loop {
        match target {
            Target::Unqualified(ident) => {
                acc.push(ident.name.as_str());
                acc.reverse();
                return acc.join(".");
            }
            Target::Qualified(q) => {
                acc.push(q.id.name.as_str());
                target = &q.qualification;
            }
            Target::Import(it) => {
                let value = &it.source.1.value;
                let import_str = format!("import(\"{}\")", value);
                acc.reverse();
                if acc.is_empty() {
                    return import_str;
                } else {
                    return format!("{}.{}", import_str, acc.join("."));
                }
            }
        }
    }
}

fn ident_name(id: &ast::Identifier<ALoc, ALoc>) -> FlowSmolStr {
    id.name.dupe()
}

pub fn error_type<'a>(
    cx: &mut Context<'a>,
    loc: ALoc,
    msg: ErrorMessage<ALoc>,
    t_in: &ast::types::Type<ALoc, ALoc>,
) -> ast::types::Type<ALoc, (ALoc, Type)> {
    flow_js_utils::add_output_non_speculating(cx, msg);
    let Ok(mapped) = polymorphic_ast_mapper::type_(&mut typed_ast_utils::ErrorMapper, t_in);
    let any_t = type_::any_t::at(type_::AnySource::AnyError(None), loc.dupe());
    let mut inner = (*mapped).clone();
    *inner.loc_mut() = (loc, any_t);
    ast::types::Type::new(inner)
}

fn check_type_arg_arity<'a>(
    cx: &mut Context<'a>,
    loc: ALoc,
    t_ast: &ast::types::Type<ALoc, ALoc>,
    params: Option<&ast::types::TypeArgs<ALoc, ALoc>>,
    n: i32,
    f: impl FnOnce(
        &mut Context<'a>,
    ) -> Result<
        ast::types::Type<ALoc, (ALoc, Type)>,
        flow_utils_concurrency::job_error::JobError,
    >,
) -> Result<ast::types::Type<ALoc, (ALoc, Type)>, flow_utils_concurrency::job_error::JobError> {
    match params {
        None => {
            if n == 0 {
                f(cx)
            } else {
                Ok(error_type(
                    cx,
                    loc.dupe(),
                    ErrorMessage::ETypeParamArity(loc, n),
                    t_ast,
                ))
            }
        }
        Some(targs) => {
            let l_len = targs.arguments.len() as i32;
            if n == l_len && n != 0 {
                f(cx)
            } else {
                Ok(error_type(
                    cx,
                    loc.dupe(),
                    ErrorMessage::ETypeParamArity(loc, n),
                    t_ast,
                ))
            }
        }
    }
}

fn mk_eval_id<'a>(cx: &Context<'a>, loc: ALoc) -> type_::eval::Id {
    if type_env::in_toplevel_scope(cx) {
        type_::eval::Id::of_aloc_id(false, cx.make_aloc_id(&loc))
    } else {
        type_::eval::Id::generate_id()
    }
}

fn add_unclear_type_error_if_not_lib_file<'a>(cx: &mut Context<'a>, loc: ALoc) {
    if !cx.is_global_lib_context() {
        flow_js_utils::add_output_non_speculating(cx, ErrorMessage::EUnclearType(loc));
    }
}

pub fn polarity<'a>(
    cx: &mut Context<'a>,
    on: flow_typing_errors::intermediate_error_types::VarianceSigilParent,
    variance: Option<&ast::Variance<ALoc>>,
) -> Polarity {
    if !cx.ts_syntax() {
        match variance {
            Some(ast::Variance {
                loc,
                kind: ast::VarianceKind::InOut,
                ..
            }) => {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::ETSSyntax(Box::new(ETSSyntaxData {
                        kind: flow_typing_errors::error_message::TSSyntaxKind::TSInOutVariance,
                        loc: loc.dupe(),
                    })),
                );
            }
            _ => {}
        }
    }
    match variance {
        Some(ast::Variance {
            loc,
            kind: ast::VarianceKind::Plus,
            ..
        }) => {
            flow_js_utils::add_output_non_speculating(
                cx,
                ErrorMessage::EVarianceKeyword(Box::new(EVarianceKeywordData {
                    kind: flow_typing_errors::error_message::VarianceKeywordKind::Plus(on),
                    loc: loc.dupe(),
                })),
            );
        }
        Some(ast::Variance {
            loc,
            kind: ast::VarianceKind::Minus,
            ..
        }) => {
            flow_js_utils::add_output_non_speculating(
                cx,
                ErrorMessage::EVarianceKeyword(Box::new(EVarianceKeywordData {
                    kind: flow_typing_errors::error_message::VarianceKeywordKind::Minus(on),
                    loc: loc.dupe(),
                })),
            );
        }
        _ => {}
    }
    typed_ast_utils::polarity(variance)
}

// =========================================================================
// Distributive tparam name helpers
// =========================================================================

fn use_distributive_tparam_name<'a>(
    cx: &mut Context<'a>,
    name: &FlowSmolStr,
    name_loc: ALoc,
    tparams_map: &mut FlowOrdMap<SubstName, Type>,
) -> Option<SubstName> {
    let subst_name = SubstName::name(name.dupe());
    match tparams_map.get(&subst_name) {
        Some(bound) => {
            let bound = bound.dupe();
            let distributive_tparam = TypeParam::new(type_::TypeParamInner {
                reason: reason::mk_annot_reason(
                    reason::VirtualReasonDesc::RType(name.dupe()),
                    name_loc,
                ),
                name: subst_name.dupe(),
                bound,
                polarity: Polarity::Neutral,
                default: None,
                is_this: false,
                is_const: false,
            });
            let generic =
                flow_js_utils::generic_of_tparam(cx, |_, x: &Type| x.dupe(), &distributive_tparam);
            tparams_map.insert(subst_name.dupe(), generic);
            Some(subst_name)
        }
        None => None,
    }
}

fn use_distributive_tparam_name_from_ast<'a>(
    cx: &mut Context<'a>,
    ast_type: &ast::types::Type<ALoc, ALoc>,
    tparams_map: &mut FlowOrdMap<SubstName, Type>,
) -> Option<SubstName> {
    use flow_parser::ast::types::TypeInner;
    use flow_parser::ast::types::generic::Identifier as GenericId;
    match ast_type.deref() {
        TypeInner::Generic { inner, .. }
            if let GenericId::Unqualified(ref ident) = inner.id
                && inner.targs.is_none() =>
        {
            use_distributive_tparam_name(cx, &ident.name, ident.loc.dupe(), tparams_map)
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    FunctionKind,
    MethodKind { is_static: bool },
    ConstructorKind,
    GetterKind,
    SetterKind,
}

fn allows_type_guards(kind: MethodKind) -> bool {
    match kind {
        MethodKind::FunctionKind | MethodKind::MethodKind { .. } => true,
        MethodKind::ConstructorKind | MethodKind::GetterKind | MethodKind::SetterKind => false,
    }
}

// Only a non-static method binds `this` to a class instance, which is what makes a
// `this` type guard refinable and therefore checkable.
pub(crate) fn allows_this_type_guards(kind: MethodKind) -> bool {
    match kind {
        MethodKind::MethodKind { is_static } => !is_static,
        MethodKind::FunctionKind
        | MethodKind::ConstructorKind
        | MethodKind::GetterKind
        | MethodKind::SetterKind => false,
    }
}

pub(crate) fn method_kind_of_class_method(
    kind: ast::class::MethodKind,
    is_static: bool,
) -> MethodKind {
    match kind {
        ast::class::MethodKind::Constructor => MethodKind::ConstructorKind,
        ast::class::MethodKind::Get => MethodKind::GetterKind,
        ast::class::MethodKind::Set => MethodKind::SetterKind,
        ast::class::MethodKind::Method => MethodKind::MethodKind { is_static },
    }
}

pub fn method_kind_to_string(kind: MethodKind) -> &'static str {
    match kind {
        MethodKind::FunctionKind => "function",
        MethodKind::MethodKind { .. } => "method",
        MethodKind::ConstructorKind => "constructor",
        MethodKind::GetterKind => "getter",
        MethodKind::SetterKind => "setter",
    }
}

// =========================================================================
// Transform annotations to types
// =========================================================================

pub fn error_on_unsupported_variance_annotation<'a>(
    cx: &mut Context<'a>,
    kind: &str,
    tparams: Option<&ast::types::TypeParams<ALoc, ALoc>>,
) {
    if let Some(tparams) = tparams {
        for tparam in tparams.params.iter() {
            if let Some(ref variance) = tparam.variance {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EUnsupportedVarianceAnnotation(Box::new((
                        variance.loc.dupe(),
                        FlowSmolStr::new(kind),
                    ))),
                );
            }
        }
    }
}

/// Whether an annotation is the type of a position that binds exactly one
/// value. That is the only kind of position where a bare `unique symbol` may be
/// written: the annotation *is* the identity of one symbol, so anywhere one
/// annotation can be shared by several values it would make distinct runtime
/// symbols compare equal.
#[derive(Debug, Clone, Dupe, PartialEq, Eq)]
pub enum BindsSingleValue {
    /// Carries the name the value is bound to, when the position has one, so a
    /// `unique symbol` renders as `[name]` rather than a generic `[symbol]`.
    /// Only pass a name where this is the sole conversion of the annotation,
    /// since two conversions that disagree would render the one symbol two ways.
    Yes(Option<FlowSmolStr>),
    No,
}

#[derive(Clone)]
struct ConvertEnv {
    tparams_map: FlowOrdMap<SubstName, Type>,
    infer_tparams_map: Rc<ALocMap<(ast::types::TypeParam<ALoc, (ALoc, Type)>, Type)>>,
    in_no_infer: bool,
    in_renders_arg: bool,
    /// Whether the annotation this env was built to convert binds exactly one
    /// value. Taken by `mk_type_available_annotation_inner`, so it describes
    /// that one annotation and never the types nested inside it.
    binds_single_value: BindsSingleValue,
}

impl ConvertEnv {
    fn new(
        infer_tparams_map: Option<ALocMap<(ast::types::TypeParam<ALoc, (ALoc, Type)>, Type)>>,
        in_no_infer: Option<bool>,
        in_renders_arg: Option<bool>,
        tparams_map: FlowOrdMap<SubstName, Type>,
        binds_single_value: BindsSingleValue,
    ) -> Self {
        Self {
            tparams_map,
            infer_tparams_map: Rc::new(infer_tparams_map.unwrap_or_default()),
            in_no_infer: in_no_infer.unwrap_or(false),
            in_renders_arg: in_renders_arg.unwrap_or(false),
            binds_single_value,
        }
    }

    /// Read [`ConvertEnv::binds_single_value`], leaving [`BindsSingleValue::No`]
    /// behind. A whole annotation may stand for one symbol, but the types
    /// nested inside it may not, and they are converted with the same env.
    fn take_binds_single_value(&mut self) -> BindsSingleValue {
        std::mem::replace(&mut self.binds_single_value, BindsSingleValue::No)
    }
}

fn resolve_computed_key_name(
    cx: &mut Context,
    expr: &ast::expression::Expression<ALoc, ALoc>,
) -> Result<
    (
        ast::expression::Expression<ALoc, (ALoc, Type)>,
        Option<Name>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    if let Some(name) = flow_parser::ast_utils::well_known_symbol_name(expr) {
        let typed_expr = match crate::statement::expression(None, None, None, cx, expr) {
            Ok(v) => v,
            Err(flow_typing_utils::abnormal::CheckExprError::Canceled(c)) => {
                return Err(flow_utils_concurrency::job_error::JobError::Canceled(c));
            }
            Err(flow_typing_utils::abnormal::CheckExprError::TimedOut(t)) => {
                return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
            }
            Err(flow_typing_utils::abnormal::CheckExprError::DebugThrow { loc }) => {
                return Err(flow_utils_concurrency::job_error::JobError::DebugThrow { loc });
            }
            Err(_) => panic!("Should not be under speculation"),
        };
        Ok((typed_expr, Some(Name::new(name))))
    } else {
        let typed_expr = match crate::statement::expression(None, None, Some(true), cx, expr) {
            Ok(v) => v,
            Err(flow_typing_utils::abnormal::CheckExprError::Canceled(c)) => {
                return Err(flow_utils_concurrency::job_error::JobError::Canceled(c));
            }
            Err(flow_typing_utils::abnormal::CheckExprError::TimedOut(t)) => {
                return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
            }
            Err(flow_typing_utils::abnormal::CheckExprError::DebugThrow { loc }) => {
                return Err(flow_utils_concurrency::job_error::JobError::DebugThrow { loc });
            }
            Err(_) => panic!("Should not be under speculation"),
        };
        let key_t = typed_expr.loc().1.dupe();
        let reason = type_util::reason_of_t(&key_t);
        let concrete_keys =
            match FlowJs::possible_concrete_types_for_computed_object_keys(cx, reason, &key_t) {
                Ok(v) => v,
                Err(FlowJsException::WorkerCanceled(c)) => {
                    return Err(JobError::Canceled(c));
                }
                Err(FlowJsException::TimedOut(t)) => {
                    return Err(JobError::TimedOut(t));
                }
                Err(err) => panic!("Should not be under speculation: {:?}", err),
            };
        let resolved_name = match concrete_keys.as_slice() {
            [key] => match flow_js_utils::propref_for_elem_t(cx, key) {
                type_::PropRef::Named { name, .. } => Some(name),
                type_::PropRef::Computed(_) => None,
            },
            _ => None,
        };
        Ok((typed_expr, resolved_name))
    }
}

/// The property a computed key names, or `None` when the key type is not a
/// single literal and so names no one property.
fn resolve_key_type_name(
    cx: &mut Context,
    key_t: &Type,
) -> Result<Option<Name>, flow_utils_concurrency::job_error::JobError> {
    let reason = type_util::reason_of_t(key_t);
    let concrete_keys = flow_js_utils::flow_js_result_to_job_error(
        FlowJs::possible_concrete_types_for_computed_object_keys(cx, reason, key_t),
    )?;
    Ok(match concrete_keys.as_slice() {
        [key] => match flow_js_utils::propref_for_elem_t(cx, key) {
            type_::PropRef::Named { name, .. } => Some(name),
            type_::PropRef::Computed(_) => None,
        },
        _ => None,
    })
}

/// The last key location of each getter and setter pair in an object type
/// body. The signature pipeline stores such a pair as one member and orders it
/// by that location, so a computed key has to lose to a pair that ends after
/// it. Folding members in source order instead would read
/// `{get a(): T, [k]: U, set a(x: T): void}` as a lone setter, and the two
/// pipelines would disagree on it.
fn accessor_pair_last_locs(
    properties: &[ast::types::object::Property<ALoc, ALoc>],
) -> HashMap<Name, ALoc> {
    use ast::types::object::Property;
    use ast::types::object::PropertyValue;
    use flow_parser::ast::expression::object::Key;

    let mut halves: HashMap<Name, (Option<ALoc>, Option<ALoc>)> = HashMap::new();
    for prop in properties {
        let Property::NormalProperty(p) = prop else {
            continue;
        };
        // Only an identifier key names an accessor here. Every other spelling
        // is rejected as unsupported syntax before it can pair up.
        let Key::Identifier(id) = &p.key else {
            continue;
        };
        let entry = halves.entry(Name::new(id.name.dupe())).or_default();
        match &p.value {
            PropertyValue::Get(..) => entry.0 = Some(id.loc.dupe()),
            PropertyValue::Set(..) => entry.1 = Some(id.loc.dupe()),
            PropertyValue::Init(_) => {}
        }
    }
    halves
        .into_iter()
        .filter_map(|(name, (get_loc, set_loc))| {
            let (get_loc, set_loc) = (get_loc?, set_loc?);
            let last = if get_loc.quick_compare(&set_loc).is_gt() {
                get_loc
            } else {
                set_loc
            };
            Some((name, last))
        })
        .collect()
}

/// How a bracketed object-type key is read.
enum KeyRead {
    /// A computed key naming this property.
    Named(Name),
    /// An index signature.
    IndexSignature,
    /// A computed key whose value names no property, so the member is dropped.
    /// Carries why, so the error can say what to write instead.
    IllegalName(InvalidObjKey),
}

/// Whether a rejected key names a type, so it is an index signature written
/// without a label rather than a computed key. Either the key was read as the
/// value side of a name that also names a type, as in `{[C]: V}`, or it names a
/// type the namespace half of a merge exposes, as in `{[C.K]: V}`.
fn key_is_type_name(cx: &mut Context, key_ast: &ast::types::Type<ALoc, (ALoc, Type)>) -> bool {
    is_type_name_value(cx, &key_ast.loc().1) || names_namespace_type(cx, key_ast)
}

/// Whether a type is the value side of something that also names a type: a
/// class, an enum, or a namespace merged into one of them, which is the only
/// way a namespace object reaches here at all. Answered from the type rather
/// than the binding, so an imported class reads like a local one.
fn is_type_name_value(cx: &mut Context, t: &Type) -> bool {
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::PolyTData;
    use flow_typing_type::type_::TypeInner;

    let Some(resolved) = cx.find_resolved(t) else {
        return false;
    };
    match resolved.deref() {
        TypeInner::NamespaceT(_) => true,
        TypeInner::DefT(_, def_t) => match def_t.deref() {
            DefTInner::ClassT(_) | DefTInner::EnumObjectT { .. } => true,
            DefTInner::PolyT(box PolyTData { t_out, .. }) => {
                matches!(t_out.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::ClassT(_)))
            }
            _ => false,
        },
        _ => false,
    }
}

/// Whether a qualified key reads a name that a namespace exposes as a type, as
/// `{[C.K]: V}` does for a class merged with a namespace declaring `K`. Only
/// the namespace half of a merge can hold such a name, so the value read the
/// key went through was always going to land on nothing. Asked of the namespace
/// the read walked to, so an ordinary missing property, as a misspelled static,
/// is not mistaken for one.
fn names_namespace_type(cx: &mut Context, key_ast: &ast::types::Type<ALoc, (ALoc, Type)>) -> bool {
    use ast::types::TypeInner as AstTypeInner;
    use ast::types::generic::Identifier;
    use flow_typing_type::type_::TypeInner;

    let AstTypeInner::Generic { inner, .. } = key_ast.deref() else {
        return false;
    };
    let Identifier::Qualified(qualified) = &inner.id else {
        return false;
    };
    let Some(namespace_t) = cx.find_resolved(type_of_type_name(&qualified.qualification)) else {
        return false;
    };
    let TypeInner::NamespaceT(ns) = namespace_t.deref() else {
        return false;
    };
    cx.find_props_opt(ns.types_tmap.dupe())
        .is_some_and(|types| types.get(&Name::new(qualified.id.name.dupe())).is_some())
}

/// The type a converted type name was read as.
fn type_of_type_name(id: &ast::types::generic::Identifier<ALoc, (ALoc, Type)>) -> &Type {
    use ast::types::generic::Identifier;

    match id {
        Identifier::Unqualified(ident) => &ident.loc.1,
        Identifier::Qualified(qualified) => &qualified.id.loc.1,
        Identifier::ImportTypeAnnot(import) => &import.loc.1,
    }
}

/// Convert an object-type key written as a value name, `{[x]: V}` or
/// `{[x.y]: V}`. The name is read as the value's type, the way `typeof x` is,
/// and names a property when that type is a single literal, matching the
/// value-level computed key `{[x]: v}`. Returns the typed key and that name.
fn convert_value_object_type_key<'a>(
    cx: &mut Context<'a>,
    loc: &ALoc,
    generic: &ast::types::Generic<ALoc, ALoc>,
) -> Result<
    (ast::types::Type<ALoc, (ALoc, Type)>, Option<Name>),
    flow_utils_concurrency::job_error::JobError,
> {
    let (value_t, id_ast) = convert_qualification_with_lookup_mode(
        cx,
        Some(type_env::LookupMode::ForTypeof),
        &generic.id,
    )?;
    let name = resolve_key_type_name(cx, &value_t)?;
    let key_ast = ast::types::Type::new(ast::types::TypeInner::Generic {
        loc: (loc.dupe(), value_t),
        inner: ast::types::Generic {
            id: id_ast,
            targs: None,
            comments: generic.comments.dupe(),
        }
        .into(),
    });
    Ok((key_ast, name))
}

// =========================================================================
// converter
// =========================================================================

fn empty_interface_annot(loc: ALoc) -> ast::types::Type<ALoc, ALoc> {
    ast::types::Type::new(ast::types::TypeInner::Interface {
        loc: loc.dupe(),
        inner: std::sync::Arc::new(ast::types::Interface {
            body: (
                loc,
                ast::types::Object {
                    exact: false,
                    inexact: true,
                    properties: Vec::new().into(),
                    comments: None,
                },
            ),
            extends: Vec::new().into(),
            comments: None,
        }),
    })
}

/// Convert a `unique symbol` type annotation. `name_hint` is the binding or
/// property name whose type is this symbol, when known, so it renders as
/// `[name]` rather than a generic `[symbol]`.
fn convert_unique_symbol<'a>(
    cx: &mut Context<'a>,
    t: &ast::types::Type<ALoc, ALoc>,
    loc: &ALoc,
    symbol_loc: &ALoc,
    comments: &Option<ast::Syntax<ALoc, ()>>,
    name_hint: Option<FlowSmolStr>,
) -> ast::types::Type<ALoc, (ALoc, Type)> {
    use flow_parser::ast::types::TypeInner;
    use flow_typing_type::type_::unique_symbol_t;

    if !cx.tslib_syntax() {
        error_type(
            cx,
            loc.dupe(),
            ErrorMessage::EUnsupportedSyntax(Box::new((
                loc.dupe(),
                intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                    TsLibSyntaxKind::UniqueSymbolType,
                ),
            ))),
            t,
        )
    } else {
        let rt = unique_symbol_t::at(cx.make_aloc_id(loc), loc.dupe(), name_hint);
        ast::types::Type::new(TypeInner::UniqueSymbol {
            loc: (loc.dupe(), rt),
            symbol_loc: symbol_loc.dupe(),
            comments: comments.clone(),
        })
    }
}

/// Convert a type in a position that binds exactly one value, where a bare
/// `unique symbol` is therefore allowed. `convert_inner` rejects it everywhere
/// else, so every legal position routes through here. `name_hint` is the
/// binding or property the symbol is declared on, when there is one, so it
/// renders as `[name]` rather than a generic `[symbol]`. Any other type is
/// converted normally, which also means a *nested* `unique symbol` (say
/// `Array<unique symbol>`) is still rejected: only the whole annotation
/// standing for one symbol is meaningful.
fn convert_single_binding_value<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    value: &ast::types::Type<ALoc, ALoc>,
    name_hint: Option<&FlowSmolStr>,
) -> Result<ast::types::Type<ALoc, (ALoc, Type)>, flow_utils_concurrency::job_error::JobError> {
    use std::ops::Deref;

    if let ast::types::TypeInner::UniqueSymbol {
        loc,
        symbol_loc,
        comments,
    } = value.deref()
    {
        Ok(convert_unique_symbol(
            cx,
            value,
            loc,
            symbol_loc,
            comments,
            name_hint.map(Dupe::dupe),
        ))
    } else {
        convert_inner(cx, env, value)
    }
}

/// The location of a bare `unique symbol` annotation, the spelling that names
/// one specific symbol. A nested occurrence is rejected outright by
/// `convert_inner`, so only a whole annotation is reported on here.
pub(crate) fn bare_unique_symbol_loc(t: &ast::types::Type<ALoc, ALoc>) -> Option<&ALoc> {
    use std::ops::Deref;

    match t.deref() {
        ast::types::TypeInner::UniqueSymbol { loc, .. } => Some(loc),
        _ => None,
    }
}

/// Report a bare `unique symbol` written where the annotation cannot stand for
/// one symbol. Reports nothing when `unique symbol` is not enabled at all, since
/// the annotation is then rejected outright by [`convert_unique_symbol`] and
/// saying where it may appear would only point at another error.
fn report_unique_symbol<'a>(
    cx: &mut Context<'a>,
    value: &ast::types::Type<ALoc, ALoc>,
    kind: intermediate_error_types::UnsupportedSyntax,
) {
    if !cx.tslib_syntax() {
        return;
    }
    if let Some(loc) = bare_unique_symbol_loc(value) {
        flow_js_utils::add_output_non_speculating(
            cx,
            ErrorMessage::EUnsupportedSyntax(Box::new((loc.dupe(), kind))),
        );
    }
}

/// Report a bare `unique symbol` on a property that can be written. The
/// annotation names one symbol, so a writable slot would let a different one
/// take its place. TypeScript requires the same (`TS1330`). The test is on
/// polarity rather than the `readonly` keyword, so a covariant property counts
/// however it is spelled.
fn check_unique_symbol_read_only<'a>(
    cx: &mut Context<'a>,
    value: &ast::types::Type<ALoc, ALoc>,
    polarity: Polarity,
) {
    if polarity == Polarity::Positive {
        return;
    }
    report_unique_symbol(
        cx,
        value,
        intermediate_error_types::UnsupportedSyntax::UniqueSymbolNotReadOnly,
    );
}

/// Report a bare `unique symbol` on a class field that is not both `static` and
/// read-only. A static field is one value on the one class object, but an
/// instance field is one per instance, so a single annotation there would be
/// shared by every instance. TypeScript requires the same (`TS1331`).
pub(crate) fn check_unique_symbol_class_field<'a>(
    cx: &mut Context<'a>,
    value: &ast::types::Type<ALoc, ALoc>,
    static_: bool,
    polarity: Polarity,
) {
    if static_ && polarity == Polarity::Positive {
        return;
    }
    report_unique_symbol(
        cx,
        value,
        intermediate_error_types::UnsupportedSyntax::UniqueSymbolNotStaticReadOnly,
    );
}

/// Report a bare `unique symbol` on a member of an interface, object type, or
/// `declare class` body, under whichever rule that body imposes.
fn check_unique_symbol_member<'a>(
    cx: &mut Context<'a>,
    value: &ast::types::Type<ALoc, ALoc>,
    obj_kind: &intermediate_error_types::ObjKind,
    static_: bool,
    polarity: Polarity,
) {
    if *obj_kind == intermediate_error_types::ObjKind::DeclareClass {
        check_unique_symbol_class_field(cx, value, static_, polarity);
    } else {
        check_unique_symbol_read_only(cx, value, polarity);
    }
}

/// Report a bare `unique symbol` on a binding that can be reassigned. See
/// [`check_unique_symbol_read_only`]. TypeScript requires the same (`TS1332`).
pub(crate) fn check_unique_symbol_const<'a>(
    cx: &mut Context<'a>,
    value: &ast::types::Type<ALoc, ALoc>,
) {
    report_unique_symbol(
        cx,
        value,
        intermediate_error_types::UnsupportedSyntax::UniqueSymbolNotConst,
    );
}

/// Report a bare `unique symbol` on a `for`...`in`/`for`...`of` binding, which
/// takes a different value on each iteration however it is declared. TypeScript
/// requires the same (`TS1334`).
pub(crate) fn check_unique_symbol_loop_binding<'a>(
    cx: &mut Context<'a>,
    id: &ast::pattern::Pattern<ALoc, ALoc>,
) {
    if let ast::pattern::Pattern::Identifier { inner, .. } = id
        && let ast::types::AnnotationOrHint::Available(annot) = &inner.annot
    {
        report_unique_symbol(
            cx,
            &annot.annotation,
            intermediate_error_types::UnsupportedSyntax::UniqueSymbolLoopBinding,
        );
    }
}

fn convert_inner<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    t: &ast::types::Type<ALoc, ALoc>,
) -> Result<ast::types::Type<ALoc, (ALoc, Type)>, flow_utils_concurrency::job_error::JobError> {
    use std::ops::Deref;
    use std::sync::Arc;

    use flow_parser::ast::types::TypeInner;
    use flow_typing_type::type_::*;

    Ok(match t.deref() {
        TypeInner::Any { loc, comments } => {
            add_unclear_type_error_if_not_lib_file(cx, loc.dupe());
            let rt = any_t::at(AnySource::AnnotatedAny, loc.dupe());
            ast::types::Type::new(TypeInner::Any {
                loc: (loc.dupe(), rt),
                comments: comments.clone(),
            })
        }
        TypeInner::Mixed { loc, comments } => {
            flow_js_utils::add_output_non_speculating(
                cx,
                ErrorMessage::EIncorrectTypeWithReplacement(Box::new(
                    EIncorrectTypeWithReplacementData {
                        loc: loc.dupe(),
                        kind: intermediate_error_types::IncorrectType::Mixed,
                    },
                )),
            );
            let rt = mixed_t::at(loc.dupe());
            ast::types::Type::new(TypeInner::Mixed {
                loc: (loc.dupe(), rt),
                comments: comments.clone(),
            })
        }
        TypeInner::Empty { loc, comments } => {
            let rt = empty_t::at(loc.dupe());
            ast::types::Type::new(TypeInner::Empty {
                loc: (loc.dupe(), rt),
                comments: comments.clone(),
            })
        }
        TypeInner::Void { loc, comments } => {
            let rt = void::at(loc.dupe());
            ast::types::Type::new(TypeInner::Void {
                loc: (loc.dupe(), rt),
                comments: comments.clone(),
            })
        }
        TypeInner::Null { loc, comments } => {
            let rt = null::at(loc.dupe());
            ast::types::Type::new(TypeInner::Null {
                loc: (loc.dupe(), rt),
                comments: comments.clone(),
            })
        }
        TypeInner::Symbol { loc, comments } => {
            let rt = symbol_t::at(loc.dupe());
            ast::types::Type::new(TypeInner::Symbol {
                loc: (loc.dupe(), rt),
                comments: comments.clone(),
            })
        }
        TypeInner::Number { loc, comments } => {
            let rt = num_module_t::at(loc.dupe());
            ast::types::Type::new(TypeInner::Number {
                loc: (loc.dupe(), rt),
                comments: comments.clone(),
            })
        }
        TypeInner::BigInt { loc, comments } => {
            let rt = bigint_module_t::at(loc.dupe());
            ast::types::Type::new(TypeInner::BigInt {
                loc: (loc.dupe(), rt),
                comments: comments.clone(),
            })
        }
        TypeInner::String { loc, comments } => {
            let rt = str_module_t::at(loc.dupe());
            ast::types::Type::new(TypeInner::String {
                loc: (loc.dupe(), rt),
                comments: comments.clone(),
            })
        }
        TypeInner::Boolean { loc, raw, comments } => {
            if *raw == ast::types::BooleanRaw::Bool {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EDeprecatedBool(loc.dupe()),
                );
            }
            let rt = bool_module_t::at(loc.dupe());
            ast::types::Type::new(TypeInner::Boolean {
                loc: (loc.dupe(), rt),
                raw: raw.clone(),
                comments: comments.clone(),
            })
        }
        TypeInner::Unknown { loc, comments } => {
            let rt = mixed_t::at(loc.dupe());
            ast::types::Type::new(TypeInner::Unknown {
                loc: (loc.dupe(), rt),
                comments: comments.clone(),
            })
        }
        TypeInner::Never { loc, comments } => {
            if !cx.ts_syntax() && !cx.is_global_libdef(cx.file()) {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::ETSSyntax(Box::new(ETSSyntaxData {
                        kind: flow_typing_errors::error_message::TSSyntaxKind::TSNever,
                        loc: loc.dupe(),
                    })),
                );
            }
            let rt = empty_t::at(loc.dupe());
            ast::types::Type::new(TypeInner::Never {
                loc: (loc.dupe(), rt),
                comments: comments.clone(),
            })
        }
        TypeInner::Undefined { loc, comments } => {
            if !cx.ts_syntax() {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::ETSSyntax(Box::new(ETSSyntaxData {
                        kind: flow_typing_errors::error_message::TSSyntaxKind::TSUndefined,
                        loc: loc.dupe(),
                    })),
                );
            }
            let rt = void::at(loc.dupe());
            ast::types::Type::new(TypeInner::Undefined {
                loc: (loc.dupe(), rt),
                comments: comments.clone(),
            })
        }
        // A `unique symbol` annotation is the identity of one symbol, so it is
        // only meaningful where it stands for exactly one value. In any other
        // position a single annotation would be shared by every value written
        // against it, which would make distinct runtime symbols compare equal.
        // The positions that do bind one value call `convert_unique_symbol`
        // directly. Where the syntax is not enabled at all, that call is still
        // the one to make: it reports the feature rather than the position.
        TypeInner::UniqueSymbol { loc, .. } if cx.tslib_syntax() => error_type(
            cx,
            loc.dupe(),
            ErrorMessage::EUnsupportedSyntax(Box::new((
                loc.dupe(),
                intermediate_error_types::UnsupportedSyntax::UniqueSymbolPosition,
            ))),
            t,
        ),
        TypeInner::UniqueSymbol {
            loc,
            symbol_loc,
            comments,
        } => convert_unique_symbol(cx, t, loc, symbol_loc, comments, None),
        TypeInner::Nullable { loc, inner } => {
            let t_ast = convert_inner(cx, env, &inner.argument)?;
            let inner_t = t_ast.loc().1.dupe();
            let reason =
                reason::mk_annot_reason(type_util::desc_of_t(&inner_t).cloned(), loc.dupe());
            let rt = Type::new(type_::TypeInner::MaybeT(reason, inner_t));
            ast::types::Type::new(TypeInner::Nullable {
                loc: (loc.dupe(), rt),
                inner: (ast::types::Nullable {
                    argument: t_ast,
                    comments: inner.comments.clone(),
                })
                .into(),
            })
        }
        TypeInner::Union { loc, inner } => {
            let t0_ast = convert_inner(cx, env, &inner.types.0)?;
            let t0 = t0_ast.loc().1.dupe();
            let t1_ast = convert_inner(cx, env, &inner.types.1)?;
            let t1 = t1_ast.loc().1.dupe();
            let (ts, ts_ast) = convert_list_inner(cx, env, &inner.types.2)?;
            let rep = type_::union_rep::make(
                Some(cx.make_aloc_id(loc)),
                type_::union_rep::UnionKind::UnknownKind,
                t0,
                t1,
                ts.into(),
            );
            let reason = reason::mk_annot_reason(None, loc.dupe());
            let rt = Type::new(type_::TypeInner::UnionT(reason, rep));
            ast::types::Type::new(TypeInner::Union {
                loc: (loc.dupe(), rt),
                inner: (ast::types::Union {
                    types: (t0_ast, t1_ast, ts_ast),
                    comments: inner.comments.clone(),
                })
                .into(),
            })
        }
        TypeInner::Intersection { loc, inner } => {
            let t0_ast = convert_inner(cx, env, &inner.types.0)?;
            let t0 = t0_ast.loc().1.dupe();
            let t1_ast = convert_inner(cx, env, &inner.types.1)?;
            let t1 = t1_ast.loc().1.dupe();
            let (ts, ts_ast) = convert_list_inner(cx, env, &inner.types.2)?;
            let rep = type_::inter_rep::make(t0, t1, ts.into());
            let reason = reason::mk_annot_reason(None, loc.dupe());
            let intersection = Type::new(type_::TypeInner::IntersectionT(reason.dupe(), rep));
            let rt = match intersection_kit::evaluate_annotation(cx, &reason, intersection, |cx| {
                mk_eval_id(cx, loc.dupe())
            }) {
                Ok(v) => v,
                Err(flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c)) => {
                    return Err(flow_utils_concurrency::job_error::JobError::Canceled(c));
                }
                Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => {
                    return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
                }
                Err(err) => panic!("Should not be under speculation: {:?}", err),
            };
            ast::types::Type::new(TypeInner::Intersection {
                loc: (loc.dupe(), rt),
                inner: (ast::types::Intersection {
                    types: (t0_ast, t1_ast, ts_ast),
                    comments: inner.comments.dupe(),
                })
                .into(),
            })
        }
        TypeInner::Typeof { loc, inner } => {
            let (valtype, qualification_ast) = convert_typeof(cx, &inner.argument)?;
            let desc =
                reason::VirtualReasonDesc::RTypeof(FlowSmolStr::new(typeof_name(&inner.argument)));
            let reason = reason::mk_reason(desc, loc.dupe());
            let (targs, targs_ast) = match &inner.targs {
                None => (None, None),
                Some(type_args) => {
                    let (targs_vec, targs_ast_vec) =
                        convert_list_inner(cx, env, &type_args.arguments)?;
                    (
                        Some(targs_vec),
                        Some(ast::types::TypeArgs::<ALoc, (ALoc, Type)> {
                            loc: type_args.loc.dupe(),
                            arguments: Arc::from(targs_ast_vec),
                            comments: type_args.comments.dupe(),
                        }),
                    )
                }
            };
            let rt = type_util::typeof_annotation(reason, valtype, targs);
            ast::types::Type::new(TypeInner::Typeof {
                loc: (loc.dupe(), rt),
                inner: (ast::types::Typeof {
                    argument: qualification_ast,
                    targs: targs_ast,
                    comments: inner.comments.dupe(),
                })
                .into(),
            })
        }
        TypeInner::Keyof { loc, inner } => {
            let argument_ast = convert_inner(cx, env, &inner.argument)?;
            let arg_t = argument_ast.loc().1.dupe();
            let t = Type::new(type_::TypeInner::KeysT(
                reason::mk_reason(None, loc.dupe()),
                arg_t,
            ));
            ast::types::Type::new(TypeInner::Keyof {
                loc: (loc.dupe(), t),
                inner: ast::types::Keyof {
                    argument: argument_ast,
                    comments: inner.comments.dupe(),
                }
                .into(),
            })
        }
        TypeInner::Renders { loc, inner } => {
            let (t, renders_ast) = convert_render_type_inner(cx, env, loc.dupe(), inner)?;
            ast::types::Type::new(TypeInner::Renders {
                loc: (loc.dupe(), t),
                inner: renders_ast.into(),
            })
        }
        TypeInner::ReadOnly { loc, inner } => match inner.argument.deref() {
            TypeInner::Tuple { .. } => {
                if !cx.ts_syntax() {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::ETSSyntax(Box::new(ETSSyntaxData {
                            kind: flow_typing_errors::error_message::TSSyntaxKind::TSReadonlyType(
                                Some(flow_typing_errors::error_message::ReadonlyTypeKind::Tuple),
                            ),
                            loc: loc.dupe(),
                        })),
                    );
                }
                let argument_ast = convert_inner(cx, env, &inner.argument)?;
                let arg_t = argument_ast.loc().1.dupe();
                let reason = reason::mk_reason(None, loc.dupe());
                let use_op = type_::UseOp::Op(Arc::new(type_::RootUseOp::MakeReadOnly {
                    loc: Box::new(loc.dupe()),
                }));
                let t = match FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                    cx,
                    use_op,
                    &reason,
                    &arg_t,
                    &type_::Destructor::ReadOnlyType,
                    mk_eval_id(cx, loc.dupe()),
                ) {
                    Ok(v) => v,
                    Err(
                        flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c),
                    ) => return Err(flow_utils_concurrency::job_error::JobError::Canceled(c)),
                    Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => {
                        return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
                    }
                    Err(err) => panic!("Should not be under speculation: {:?}", err),
                };
                ast::types::Type::new(TypeInner::ReadOnly {
                    loc: (loc.dupe(), t),
                    inner: ast::types::ReadOnly {
                        argument: argument_ast,
                        comments: inner.comments.dupe(),
                    }
                    .into(),
                })
            }
            TypeInner::Array {
                loc: arr_loc,
                inner: arr_inner,
            } => {
                if !cx.ts_syntax() {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::ETSSyntax(Box::new(ETSSyntaxData {
                            kind: flow_typing_errors::error_message::TSSyntaxKind::TSReadonlyType(
                                Some(flow_typing_errors::error_message::ReadonlyTypeKind::Array),
                            ),
                            loc: loc.dupe(),
                        })),
                    );
                }
                let argument_ast = convert_inner(cx, env, &arr_inner.argument)?;
                let elem_t = argument_ast.loc().1.dupe();
                let arr_t = Type::new(type_::TypeInner::DefT(
                    reason::mk_annot_reason(reason::VirtualReasonDesc::RROArrayType, loc.dupe()),
                    type_::DefT::new(type_::DefTInner::ArrT(Rc::new(type_::ArrType::ArrayAT(
                        Box::new(ArrayATData {
                            react_dro: None,
                            elem_t: elem_t.dupe(),
                            tuple_view: None,
                            strictness_kind: cx.type_strictness_kind(),
                            literal: false,
                            fresh: false,
                        }),
                    )))),
                ));
                let ro_t = Type::new(type_::TypeInner::DefT(
                    reason::mk_annot_reason(reason::VirtualReasonDesc::RROArrayType, loc.dupe()),
                    type_::DefT::new(type_::DefTInner::ArrT(Rc::new(type_::ArrType::ROArrayAT(
                        Box::new(ROArrayATData {
                            elem_t: elem_t.dupe(),
                            react_dro: None,
                            literal: false,
                        }),
                    )))),
                ));
                ast::types::Type::new(TypeInner::ReadOnly {
                    loc: (loc.dupe(), ro_t),
                    inner: (ast::types::ReadOnly {
                        argument: ast::types::Type::new(TypeInner::Array {
                            loc: (arr_loc.dupe(), arr_t),
                            inner: (ast::types::Array {
                                argument: argument_ast,
                                comments: arr_inner.comments.dupe(),
                            })
                            .into(),
                        }),
                        comments: inner.comments.dupe(),
                    })
                    .into(),
                })
            }
            _ => {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::ETSSyntax(Box::new(ETSSyntaxData {
                        kind: flow_typing_errors::error_message::TSSyntaxKind::TSReadonlyType(None),
                        loc: loc.dupe(),
                    })),
                );
                let t = type_::any_t::at(type_::AnySource::AnyError(None), loc.dupe());
                let mapped_ro = {
                    let Ok(v) = polymorphic_ast_mapper::readonly_type(
                        &mut typed_ast_utils::ErrorMapper,
                        inner,
                    );
                    v
                };
                ast::types::Type::new(TypeInner::ReadOnly {
                    loc: (loc.dupe(), t),
                    inner: mapped_ro.into(),
                })
            }
        },
        TypeInner::Tuple { loc, inner } => {
            let reason = reason::mk_annot_reason(reason::VirtualReasonDesc::RTupleType, loc.dupe());
            let mut unresolved = Vec::new();
            let mut els_asts = Vec::new();
            for element in inner.elements.iter() {
                let (el, el_ast) = convert_tuple_element(cx, env, element)?;
                unresolved.push(el);
                els_asts.push(el_ast);
            }
            let id = mk_eval_id(cx, loc.dupe());
            let t = match flow_js_utils::mk_tuple_type(
                cx,
                cx.type_strictness_kind(),
                id,
                |cx, use_op, reason, t, destructor, eval_id| {
                    FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                        cx,
                        use_op,
                        &reason,
                        &t,
                        &destructor,
                        eval_id,
                    )
                },
                inner.inexact,
                reason,
                unresolved,
            ) {
                Ok(v) => v,
                Err(flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c)) => {
                    return Err(flow_utils_concurrency::job_error::JobError::Canceled(c));
                }
                Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => {
                    return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
                }
                Err(err) => panic!("Should not be under speculation: {:?}", err),
            };
            ast::types::Type::new(TypeInner::Tuple {
                loc: (loc.dupe(), t),
                inner: (ast::types::Tuple {
                    elements: els_asts.into(),
                    inexact: inner.inexact,
                    comments: inner.comments.dupe(),
                })
                .into(),
            })
        }
        TypeInner::Array { loc, inner } => {
            let r = reason::mk_annot_reason(reason::VirtualReasonDesc::RArrayType, loc.dupe());
            let t_ast = convert_inner(cx, env, &inner.argument)?;
            let elem_t = t_ast.loc().1.dupe();
            let rt = Type::new(type_::TypeInner::DefT(
                r,
                type_::DefT::new(type_::DefTInner::ArrT(Rc::new(type_::ArrType::ArrayAT(
                    Box::new(ArrayATData {
                        elem_t,
                        tuple_view: None,
                        react_dro: None,
                        strictness_kind: cx.type_strictness_kind(),
                        literal: false,
                        fresh: false,
                    }),
                )))),
            ));
            ast::types::Type::new(TypeInner::Array {
                loc: (loc.dupe(), rt),
                inner: (ast::types::Array {
                    argument: t_ast,
                    comments: inner.comments.dupe(),
                })
                .into(),
            })
        }
        TypeInner::Conditional { loc, inner } => {
            let distributive_tparam_restore = {
                use flow_parser::ast::types::TypeInner;
                use flow_parser::ast::types::generic::Identifier as GenericId;
                match inner.check_type.deref() {
                    TypeInner::Generic { inner, .. }
                        if let GenericId::Unqualified(ref ident) = inner.id
                            && inner.targs.is_none() =>
                    {
                        let subst_name = SubstName::name(ident.name.dupe());
                        env.tparams_map
                            .get(&subst_name)
                            .map(|t| (subst_name, t.dupe()))
                    }
                    _ => None,
                }
            };
            let distributive_tparam_name =
                use_distributive_tparam_name_from_ast(cx, &inner.check_type, &mut env.tparams_map);
            let check_type_ast = convert_inner(cx, &mut *env, &inner.check_type)?;
            let check_t = check_type_ast.loc().1.dupe();
            let hoisted_infer_types =
                flow_analysis::infer_type_hoister::hoist_infer_types(&inner.extends_type);
            let mut tparams: Vec<type_::TypeParam> = Vec::new();
            let mut additional_true_type_tparams_map: FlowOrdMap<SubstName, Type> =
                FlowOrdMap::default();
            let mut extends_infer_tparams_map = ALocMap::new();
            let mut infer_bounds_map: FlowOrdMap<SubstName, Type> = FlowOrdMap::default();
            for (_infer_loc, infer) in hoisted_infer_types.iter() {
                let subst_name = SubstName::name(infer.tparam.name.name.dupe());
                let old_infer_tparams_map =
                    std::mem::replace(&mut env.infer_tparams_map, Rc::new(ALocMap::new()));
                let (tparam_tast, tparam, t) = mk_type_param_inner(
                    cx,
                    &mut *env,
                    flow_parser::ast_visitor::TypeParamsContext::Infer,
                    &infer.tparam,
                )?;
                env.infer_tparams_map = old_infer_tparams_map;
                match infer_bounds_map.get(&subst_name) {
                    Some(existing_bound) => {
                        let use_op = type_::UseOp::Op(Arc::new(
                            type_::RootUseOp::InferBoundCompatibilityCheck {
                                bound: type_util::reason_of_t(&tparam.bound).dupe(),
                                infer: type_util::reason_of_t(&t).dupe(),
                            },
                        ));
                        match flow_js::unify(cx, Some(use_op), &tparam.bound, existing_bound) {
                            Ok(()) => (),
                            Err(flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c)) => return Err(flow_utils_concurrency::job_error::JobError::Canceled(c)),
                            Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t)),
                            Err(err) => panic!("Should not be under speculation: {:?}", err),
                        }
                        let t = additional_true_type_tparams_map
                            .get(&subst_name)
                            .expect("subst_name should exist in additional_true_type_tparams_map")
                            .dupe();
                        extends_infer_tparams_map.insert(tparam_tast.loc.dupe(), (tparam_tast, t));
                    }
                    None => {
                        tparams.push(tparam.dupe());
                        additional_true_type_tparams_map.insert(subst_name.dupe(), t.dupe());
                        extends_infer_tparams_map.insert(tparam_tast.loc.dupe(), (tparam_tast, t));
                        infer_bounds_map.insert(subst_name, tparam.bound.dupe());
                    }
                }
            }
            let infer_tparams = tparams;
            let old_infer_tparams_map = std::mem::replace(
                &mut env.infer_tparams_map,
                Rc::new(extends_infer_tparams_map),
            );
            let extends_type_ast = convert_inner(cx, &mut *env, &inner.extends_type)?;
            let extends_t = extends_type_ast.loc().1.dupe();
            env.infer_tparams_map = old_infer_tparams_map;
            let mut old_true_tparams = Vec::new();
            for (k, v) in additional_true_type_tparams_map.iter() {
                let old = env.tparams_map.get(k).map(|v| v.dupe());
                env.tparams_map.insert(k.dupe(), v.dupe());
                old_true_tparams.push((k.dupe(), old));
            }
            let true_type_ast = convert_inner(cx, &mut *env, &inner.true_type)?;
            let true_t = true_type_ast.loc().1.dupe();
            for (k, v) in old_true_tparams.into_iter().rev() {
                match v {
                    Some(v) => {
                        env.tparams_map.insert(k, v);
                    }
                    None => {
                        env.tparams_map.remove(&k);
                    }
                }
            }
            let false_type_ast = convert_inner(cx, &mut *env, &inner.false_type)?;
            let false_t = false_type_ast.loc().1.dupe();
            if let Some((subst_name, t)) = distributive_tparam_restore {
                env.tparams_map.insert(subst_name, t);
            }
            let reason = reason::mk_reason(None, loc.dupe());
            let destructor =
                type_::Destructor::ConditionalType(Box::new(DestructorConditionalTypeData {
                    distributive_tparam_name,
                    infer_tparams: infer_tparams.into(),
                    extends_t,
                    true_t,
                    false_t,
                }));
            let t = match FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                cx,
                type_::unknown_use(),
                &reason,
                &check_t,
                &destructor,
                mk_eval_id(cx, loc.dupe()),
            ) {
                Ok(v) => v,
                Err(flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c)) => {
                    return Err(flow_utils_concurrency::job_error::JobError::Canceled(c));
                }
                Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => {
                    return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
                }
                Err(err) => panic!("Should not be under speculation: {:?}", err),
            };
            ast::types::Type::new(TypeInner::Conditional {
                loc: (loc.dupe(), t),
                inner: (ast::types::Conditional {
                    check_type: check_type_ast,
                    extends_type: extends_type_ast,
                    true_type: true_type_ast,
                    false_type: false_type_ast,
                    comments: inner.comments.dupe(),
                })
                .into(),
            })
        }
        TypeInner::Infer { loc, inner } => {
            let tparam_loc = &inner.tparam.loc;
            let name_loc = &inner.tparam.name.loc;
            let environment = cx.environment();
            let var_info = &environment.var_info;
            match var_info.env_entries.get_ordinary(name_loc) {
                Some(flow_env_builder::env_api::EnvEntry::NonAssigningWrite) => {
                    let Ok(tparam_tast) = polymorphic_ast_mapper::type_param(
                        &mut typed_ast_utils::ErrorMapper,
                        &inner.tparam,
                    );
                    let t = type_::any_t::error(reason::mk_reason(None, loc.dupe()));
                    ast::types::Type::new(TypeInner::Infer {
                        loc: (loc.dupe(), t),
                        inner: (ast::types::Infer {
                            tparam: tparam_tast,
                            comments: inner.comments.dupe(),
                        })
                        .into(),
                    })
                }
                _ => {
                    let (tparam_tast, t) = env
                        .infer_tparams_map
                        .get(tparam_loc)
                        .expect("tparam_loc should exist in infer_tparams_map")
                        .clone();
                    ast::types::Type::new(TypeInner::Infer {
                        loc: (loc.dupe(), t),
                        inner: (ast::types::Infer {
                            tparam: tparam_tast,
                            comments: inner.comments.dupe(),
                        })
                        .into(),
                    })
                }
            }
        }
        TypeInner::StringLiteral { loc, literal } => {
            let rt = mk_singleton_string(loc.dupe(), &literal.value);
            ast::types::Type::new(TypeInner::StringLiteral {
                loc: (loc.dupe(), rt),
                literal: literal.clone(),
            })
        }
        TypeInner::NumberLiteral { loc, literal } => {
            let rt = mk_singleton_number(loc.dupe(), literal.value, &literal.raw);
            ast::types::Type::new(TypeInner::NumberLiteral {
                loc: (loc.dupe(), rt),
                literal: literal.clone(),
            })
        }
        TypeInner::BigIntLiteral { loc, literal } => {
            let rt = mk_singleton_bigint(loc.dupe(), literal.value, &literal.raw);
            ast::types::Type::new(TypeInner::BigIntLiteral {
                loc: (loc.dupe(), rt),
                literal: literal.clone(),
            })
        }
        TypeInner::BooleanLiteral { loc, literal } => {
            let rt = mk_singleton_boolean(loc.dupe(), literal.value);
            ast::types::Type::new(TypeInner::BooleanLiteral {
                loc: (loc.dupe(), rt),
                literal: literal.clone(),
            })
        }
        TypeInner::IndexedAccess { loc, inner } => {
            let reason = reason::mk_reason(None, loc.dupe());
            let object_ast = convert_inner(cx, env, &inner.object)?;
            let object_type = object_ast.loc().1.dupe();
            let index_ast = convert_inner(cx, env, &inner.index)?;
            let index_type = index_ast.loc().1.dupe();
            let use_op = type_::UseOp::Op(Arc::new(type_::RootUseOp::IndexedTypeAccess(Box::new(
                type_::IndexedTypeAccessData {
                    index_loc: type_util::loc_of_t(&index_type).dupe(),
                    object: flow_js_utils::type_or_type_desc_for_error(&object_type),
                    index: flow_js_utils::type_or_type_desc_for_error(&index_type),
                },
            ))));
            let destructor = match index_ast.deref() {
                TypeInner::StringLiteral { literal, .. } => type_::Destructor::PropertyType {
                    name: Name::new(literal.value.dupe()),
                },
                _ => type_::Destructor::ElementType {
                    index_type: index_type.dupe(),
                },
            };
            let t = match FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                cx,
                use_op,
                &reason,
                &object_type,
                &destructor,
                mk_eval_id(cx, loc.dupe()),
            ) {
                Ok(v) => v,
                Err(flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c)) => {
                    return Err(flow_utils_concurrency::job_error::JobError::Canceled(c));
                }
                Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => {
                    return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
                }
                Err(err) => panic!("Should not be under speculation: {:?}", err),
            };
            ast::types::Type::new(TypeInner::IndexedAccess {
                loc: (loc.dupe(), t),
                inner: ast::types::IndexedAccess {
                    object: object_ast,
                    index: index_ast,
                    comments: inner.comments.dupe(),
                }
                .into(),
            })
        }
        TypeInner::OptionalIndexedAccess { loc: _, inner } => {
            let loc = t.loc().dupe();
            let (_, ast) = optional_indexed_access(cx, env, loc, inner)?;
            ast
        }
        TypeInner::Generic { loc, inner } => match &inner.id {
            ast::types::generic::Identifier::Qualified(q) => {
                let qid = &inner.id;
                let qid_loc = &q.loc;
                let targs = &inner.targs;
                let comments = &inner.comments;

                let (m, qualification_ast) = convert_qualification(cx, &q.qualification)?;
                let id_loc = q.id.loc.dupe();
                let id_name = &q.id;
                let name = &id_name.name;
                let reason =
                    reason::mk_reason(reason::VirtualReasonDesc::RType(name.dupe()), loc.dupe());
                let id_reason =
                    reason::mk_reason(reason::VirtualReasonDesc::RType(name.dupe()), id_loc.dupe());
                let qid_name = qualified_name(qid);
                let qid_reason = reason::mk_reason(
                    reason::VirtualReasonDesc::RType(FlowSmolStr::new(&qid_name)),
                    qid_loc.dupe(),
                );
                let use_op =
                    type_::UseOp::Op(Arc::new(type_::RootUseOp::GetProperty(qid_reason.dupe())));
                let t_unapplied = crate::type_annotation_cons_gen::qualify_type(
                    cx,
                    use_op,
                    id_reason,
                    qid_reason,
                    Name::new(name.dupe()),
                    m,
                );
                let (t, targs_ast) =
                    mk_nominal_type_inner(cx, env, reason, t_unapplied.dupe(), targs.as_ref())?;
                ast::types::Type::new(TypeInner::Generic {
                    loc: (loc.dupe(), t),
                    inner: (ast::types::Generic {
                        id: ast::types::generic::Identifier::Qualified(
                            (ast::types::generic::Qualified {
                                loc: qid_loc.dupe(),
                                qualification: qualification_ast,
                                id: ast::Identifier::new(ast::IdentifierInner {
                                    loc: (id_loc, t_unapplied),
                                    name: name.dupe(),
                                    comments: id_name.comments.dupe(),
                                }),
                            })
                            .into(),
                        ),
                        targs: targs_ast,
                        comments: comments.clone(),
                    })
                    .into(),
                })
            }
            // import("module") type syntax
            ast::types::generic::Identifier::ImportTypeAnnot(import) => {
                let import_id = &inner.id;
                let import_loc = &import.loc;
                let source = &import.source;
                let import_comments = &import.comments;
                let targs = &inner.targs;
                let comments = &inner.comments;

                let (m, import_ast) = convert_qualification(cx, import_id)?;
                let (t, targs_ast) = {
                    let import_id_name = qualified_name(import_id);
                    let reason = reason::mk_reason(
                        reason::VirtualReasonDesc::RType(FlowSmolStr::new(&import_id_name)),
                        loc.dupe(),
                    );
                    mk_nominal_type_inner(cx, env, reason, m.dupe(), targs.as_ref())?
                };
                let id_ast = match import_ast {
                    ast::types::generic::Identifier::ImportTypeAnnot(import_type_ast) => {
                        ast::types::generic::Identifier::ImportTypeAnnot(import_type_ast)
                    }
                    _ => ast::types::generic::Identifier::ImportTypeAnnot(
                        (ast::types::generic::ImportType {
                            loc: (import_loc.dupe(), m),
                            source: source.clone(),
                            comments: import_comments.clone(),
                        })
                        .into(),
                    ),
                };
                ast::types::Type::new(TypeInner::Generic {
                    loc: (loc.dupe(), t),
                    inner: (ast::types::Generic {
                        id: id_ast,
                        targs: targs_ast,
                        comments: comments.clone(),
                    })
                    .into(),
                })
            }
            // type applications: name < params >
            ast::types::generic::Identifier::Unqualified(ident) => {
                let name_loc = ident.loc.dupe();
                let id_name = ident;
                let name = &ident.name;
                // Comments are innecessary, so they can be stripped to meet the generic requirements
                let convert_type_params = |cx: &mut Context<'a>,
                                           env: &mut ConvertEnv,
                                           targs: Option<&ast::types::TypeArgs<ALoc, ALoc>>|
                 -> Result<
                    (Vec<Type>, Option<ast::types::TypeArgs<ALoc, (ALoc, Type)>>),
                    flow_utils_concurrency::job_error::JobError,
                > {
                    Ok(match targs {
                        None => (vec![], None),
                        Some(targs_node) => {
                            let (elemts, targs_ast) =
                                convert_list_inner(cx, env, &targs_node.arguments)?;
                            (
                                elemts,
                                Some(ast::types::TypeArgs::<ALoc, (ALoc, Type)> {
                                    loc: targs_node.loc.dupe(),
                                    arguments: Arc::from(targs_ast),
                                    comments: targs_node.comments.dupe(),
                                }),
                            )
                        }
                    })
                };
                let reconstruct_ast = |t: Type,
                                       id_t: Option<Type>,
                                       targs: Option<ast::types::TypeArgs<ALoc, (ALoc, Type)>>|
                 -> ast::types::Type<ALoc, (ALoc, Type)> {
                    let id_t_val = id_t.unwrap_or_else(|| t.dupe());
                    ast::types::Type::new(TypeInner::Generic {
                        loc: (loc.dupe(), t),
                        inner: (ast::types::Generic {
                            id: ast::types::generic::Identifier::Unqualified(ast::Identifier::new(
                                ast::IdentifierInner {
                                    loc: (name_loc.dupe(), id_t_val),
                                    name: id_name.name.dupe(),
                                    comments: id_name.comments.dupe(),
                                },
                            )),
                            targs,
                            comments: inner.comments.dupe(),
                        })
                        .into(),
                    })
                };

                let use_op = |reason: &Reason| -> type_::UseOp {
                    type_::UseOp::Op(Arc::new(type_::RootUseOp::TypeApplication {
                        type_: reason.dupe(),
                    }))
                };
                let local_generic_type =
                    |cx: &mut Context<'a>,
                     env: &mut ConvertEnv,
                     name: &FlowSmolStr,
                     name_loc: ALoc,
                     loc: ALoc,
                     targs: Option<&ast::types::TypeArgs<ALoc, ALoc>>,
                     reconstruct_ast: &dyn Fn(
                        Type,
                        Option<Type>,
                        Option<ast::types::TypeArgs<ALoc, (ALoc, Type)>>,
                    )
                        -> ast::types::Type<ALoc, (ALoc, Type)>|
                     -> Result<
                        ast::types::Type<ALoc, (ALoc, Type)>,
                        flow_utils_concurrency::job_error::JobError,
                    > {
                        let reason = reason::mk_reason(
                            reason::VirtualReasonDesc::RType(name.dupe()),
                            loc.dupe(),
                        );
                        let c = type_identifier(cx, name, name_loc)?;
                        let (t, targs_ast) =
                            mk_nominal_type_inner(cx, env, reason, c.dupe(), targs)?;
                        Ok(reconstruct_ast(t, Some(c), targs_ast))
                    };
                let mod_tparam_t_annot_loc = |annot_loc: ALoc, tp: &Type| -> Type {
                    type_util::mod_reason_of_t(
                        &|r: Reason| {
                            r.reposition(loc.dupe())
                                .opt_annotate(Some(annot_loc.dupe()))
                        },
                        tp,
                    )
                };
                match name.as_str() {
                    "this" => {
                        if env
                            .tparams_map
                            .contains_key(&SubstName::name(FlowSmolStr::new_inline("this")))
                        {
                            // We model a this type like a type parameter. The bound on a this
                            // type reflects the interface of `this` exposed in the current
                            // environment. Currently, we only support this types in a class
                            // environment: a this type in class C is bounded by C.
                            check_type_arg_arity(
                                cx,
                                loc.dupe(),
                                t,
                                inner.targs.as_ref(),
                                0,
                                |_| {
                                    let tparam_t = env
                                        .tparams_map
                                        .get(&SubstName::name(FlowSmolStr::new_inline("this")))
                                        .unwrap()
                                        .dupe();
                                    let tp = mod_tparam_t_annot_loc(loc.dupe(), &tparam_t);
                                    Ok(reconstruct_ast(tp, None, None))
                                },
                            )?
                        } else {
                            flow_js_utils::add_output_non_speculating(
                                cx,
                                ErrorMessage::EUnexpectedThisType(loc.dupe()),
                            );
                            {
                                let Ok(v) = polymorphic_ast_mapper::type_(
                                    &mut typed_ast_utils::ErrorMapper,
                                    t,
                                );
                                v
                            }
                        }
                    }
                    // A class type parameter in a static member of the class, which the name
                    // resolver reports.
                    _ if cx
                        .environment()
                        .var_info
                        .invalid_static_type_param_locs
                        .contains(&name_loc) =>
                    {
                        let Ok(v) =
                            polymorphic_ast_mapper::type_(&mut typed_ast_utils::ErrorMapper, t);
                        v
                    }
                    // in-scope type vars
                    _ if env.tparams_map.contains_key(&SubstName::name(name.dupe()))
                        && !cx
                            .environment()
                            .var_info
                            .invalid_type_param_default_locs
                            .contains(&name_loc) =>
                    {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 0, |_| {
                            let mut tp = env
                                .tparams_map
                                .get(&SubstName::name(name.dupe()))
                                .unwrap()
                                .dupe();
                            if env.in_no_infer {
                                if let type_::TypeInner::GenericT(box GenericTData {
                                    reason: gr,
                                    name: gn,
                                    bound: gb,
                                    no_infer: _,
                                    id: gid,
                                }) = tp.deref()
                                {
                                    tp = Type::new(type_::TypeInner::GenericT(Box::new(
                                        GenericTData {
                                            reason: gr.dupe(),
                                            name: gn.dupe(),
                                            bound: gb.dupe(),
                                            no_infer: true,
                                            id: gid.clone(),
                                        },
                                    )));
                                }
                            }
                            let tp = mod_tparam_t_annot_loc(loc.dupe(), &tp);
                            Ok(reconstruct_ast(tp, None, None))
                        })?
                    }
                    _ if type_env::local_scope_entry_exists(cx, name_loc.dupe()) => {
                        local_generic_type(
                            cx,
                            env,
                            name,
                            name_loc.dupe(),
                            loc.dupe(),
                            inner.targs.as_ref(),
                            &reconstruct_ast,
                        )?
                    }
                    // TS-only: `object` is a TypeScript primitive denoting any
                    // non-primitive value. Flow has no exact equivalent, but
                    // `.d.ts` library definitions reference it pervasively. We
                    // accept the name in TS files and global libdefs and model it as a
                    // structurally-empty interface. Defining it here (rather than
                    // in core.js) keeps it available when consuming TS lib
                    // defs that do not load core.js.
                    "object"
                        if flow_common::files::has_ts_ext(cx.file())
                            || cx.is_global_libdef(cx.file()) =>
                    {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 0, |cx| {
                            let interface_ast = empty_interface_annot(loc.dupe());
                            let result = convert_inner(cx, env, &interface_ast)?;
                            let (_, result_t) = result.loc();
                            // Tag the empty interface as the `object` builtin so
                            // it rejects primitives (a normal empty interface
                            // promotes them to wrappers in .ts files; see
                            // subtyping_kit.rs).
                            let result_t = match result_t.deref() {
                                type_::TypeInner::DefT(r, def) => match def.deref() {
                                    type_::DefTInner::InstanceT(instance_t) => {
                                        match &instance_t.inst.inst_kind {
                                            type_::InstanceKind::InterfaceKind {
                                                inline, ..
                                            } => {
                                                let inline = *inline;
                                                let inst =
                                                    type_::InstType::new(type_::InstTypeInner {
                                                        inst_kind:
                                                            type_::InstanceKind::InterfaceKind {
                                                                inline,
                                                                reject_primitives: true,
                                                            },
                                                        ..(*instance_t.inst).clone()
                                                    });
                                                let instance_t =
                                                    type_::InstanceT::new(type_::InstanceTInner {
                                                        inst,
                                                        ..(***instance_t).clone()
                                                    });
                                                Type::new(type_::TypeInner::DefT(
                                                    r.dupe(),
                                                    type_::DefT::new(type_::DefTInner::InstanceT(
                                                        Rc::new(instance_t),
                                                    )),
                                                ))
                                            }
                                            _ => result_t.dupe(),
                                        }
                                    }
                                    _ => result_t.dupe(),
                                },
                                _ => result_t.dupe(),
                            };
                            Ok(reconstruct_ast(result_t, None, None))
                        })?
                    }
                    // `intrinsic` marks a type alias whose value is supplied by the
                    // type-checker rather than written in source (e.g.
                    // `type Uppercase<S> = intrinsic`). Flow intercepts such aliases at
                    // their use sites, so this body is never consulted; we accept the name
                    // in library definitions and model it as `empty`.
                    "intrinsic" if cx.is_global_lib_context() => {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 0, |_| {
                            Ok(reconstruct_ast(empty_t::at(loc.dupe()), None, None))
                        })?
                    }
                    // NoInfer intrinsic that makes every GenericT inside it no_infer
                    "NoInfer" => {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let mut env_no_infer = env.clone();
                            env_no_infer.in_no_infer = true;
                            let (elemts, targs_ast) =
                                convert_type_params(cx, &mut env_no_infer, inner.targs.as_ref())?;
                            let elem_t = elemts.into_iter().next().unwrap();
                            Ok(reconstruct_ast(elem_t, None, targs_ast))
                        })?
                    }
                    "ExactObjectType" => {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 0, |cx| {
                            let reason = reason::mk_annot_reason(
                                reason::VirtualReasonDesc::RType(name.dupe()),
                                loc.dupe(),
                            );
                            let t = obj_type::mk_exact_object_type(cx, reason);
                            Ok(reconstruct_ast(t, None, None))
                        })?
                    }
                    kind_name @ ("Uppercase" | "Lowercase" | "Capitalize" | "Uncapitalize") => {
                        let kind = string_case_transform::kind_of_name(kind_name).unwrap();
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let arg = ts.into_iter().next().unwrap();
                            let result_reason = reason::mk_reason(
                                reason::VirtualReasonDesc::RType(FlowSmolStr::new(kind_name)),
                                loc.dupe(),
                            );
                            let uo = use_op(&result_reason);
                            cx.add_post_inference_subtyping_check(
                                arg.dupe(),
                                uo,
                                type_::str_module_t::at(loc.dupe()),
                            );
                            let t = string_case_transform::resolve(
                                Some(&|cx: &mut Context<'_>, r: &Reason, t: &Type| {
                                    FlowJs::possible_concrete_types_for_inspection(cx, r, t)
                                }),
                                cx,
                                kind,
                                loc.dupe(),
                                arg,
                            );
                            Ok(reconstruct_ast(t, None, targs_ast))
                        })?
                    }
                    // Array<T>
                    "Array" => {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (elemts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let elem_t = elemts.into_iter().next().unwrap();
                            // The annotation desugars to a builtin array type,
                            // but the head still names the `Array` class, so
                            // record that on the identifier for hover.
                            let id_t = unapplied_type_identifier_opt(cx, name, name_loc.dupe())?;
                            Ok(reconstruct_ast(
                                Type::new(type_::TypeInner::DefT(
                                    reason::mk_annot_reason(
                                        reason::VirtualReasonDesc::RArrayType,
                                        loc.dupe(),
                                    ),
                                    type_::DefT::new(type_::DefTInner::ArrT(Rc::new(
                                        type_::ArrType::ArrayAT(Box::new(ArrayATData {
                                            react_dro: None,
                                            elem_t,
                                            tuple_view: None,
                                            strictness_kind: cx.type_strictness_kind(),
                                            literal: false,
                                            fresh: false,
                                        })),
                                    ))),
                                )),
                                id_t,
                                targs_ast,
                            ))
                        })?
                    }
                    // $ReadOnlyArray<T> is the supertype of all tuples and all arrays
                    "$ReadOnlyArray" => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EIncorrectTypeWithReplacement(Box::new(
                                EIncorrectTypeWithReplacementData {
                                    loc: loc.dupe(),
                                    kind:
                                        intermediate_error_types::IncorrectType::DollarReadOnlyArray,
                                },
                            )),
                        );
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (elemts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let elemt = elemts.into_iter().next().unwrap();
                            Ok(reconstruct_ast(
                                Type::new(type_::TypeInner::DefT(
                                    reason::mk_annot_reason(
                                        reason::VirtualReasonDesc::RROArrayType,
                                        loc.dupe(),
                                    ),
                                    type_::DefT::new(type_::DefTInner::ArrT(Rc::new(
                                        type_::ArrType::ROArrayAT(Box::new(ROArrayATData {
                                            elem_t: elemt,
                                            react_dro: None,
                                            literal: false,
                                        })),
                                    ))),
                                )),
                                None,
                                targs_ast,
                            ))
                        })?
                    }
                    // $NonMaybeType<T> acts as the type T without null and void
                    "$NonMaybeType" => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EIncorrectTypeWithReplacement(Box::new(
                                EIncorrectTypeWithReplacementData {
                                    loc: loc.dupe(),
                                    kind:
                                        intermediate_error_types::IncorrectType::DollarNonMaybeType,
                                },
                            )),
                        );
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let tp = ts.into_iter().next().unwrap();
                            let reason = reason::mk_reason(
                                reason::VirtualReasonDesc::RType(FlowSmolStr::new_inline(
                                    "$NonMaybeType",
                                )),
                                loc.dupe(),
                            );
                            let result_t =
                                FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                                    cx,
                                    use_op(&reason),
                                    &reason,
                                    &tp,
                                    &type_::Destructor::NonMaybeType,
                                    mk_eval_id(cx, loc.dupe()),
                                )
                                .expect("mk_type_destructor should not fail");
                            Ok(reconstruct_ast(result_t, None, targs_ast))
                        })?
                    }
                    // Deprecated former alias of `Partial`
                    "$Partial" => error_type(
                        cx,
                        loc.dupe(),
                        ErrorMessage::EIncorrectTypeWithReplacement(Box::new(
                            EIncorrectTypeWithReplacementData {
                                loc: loc.dupe(),
                                kind: intermediate_error_types::IncorrectType::Partial,
                            },
                        )),
                        t,
                    ),
                    // Partial<T> makes all of `T`'s properties optional
                    "Partial" => {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let tp = ts.into_iter().next().unwrap();
                            let reason = reason::mk_reason(
                                type_util::desc_of_t(&tp).cloned().map(|desc| {
                                    reason::VirtualReasonDesc::RPartialOf(Arc::new(desc))
                                }),
                                type_util::loc_of_t(&tp).dupe(),
                            );
                            let result_t =
                                FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                                    cx,
                                    use_op(&reason),
                                    &reason,
                                    &tp,
                                    &type_::Destructor::PartialType,
                                    mk_eval_id(cx, loc.dupe()),
                                )
                                .expect("mk_type_destructor should not fail");
                            Ok(reconstruct_ast(result_t, None, targs_ast))
                        })?
                    }
                    // Required<T> makes all of `T`'s optional properties required.
                    "Required" => {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let tp = ts.into_iter().next().unwrap();
                            let reason = reason::mk_reason(
                                type_util::desc_of_t(&tp).cloned().map(|desc| {
                                    reason::VirtualReasonDesc::RRequiredOf(Arc::new(desc))
                                }),
                                type_util::loc_of_t(&tp).dupe(),
                            );
                            let result_t =
                                FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                                    cx,
                                    use_op(&reason),
                                    &reason,
                                    &tp,
                                    &type_::Destructor::RequiredType,
                                    mk_eval_id(cx, loc.dupe()),
                                )
                                .expect("mk_type_destructor should not fail");
                            Ok(reconstruct_ast(result_t, None, targs_ast))
                        })?
                    }
                    // TS-only: `ThisType<T>` is a marker used by TypeScript to rewire `this`
                    // inside object literal methods. That rewiring is moot in Flow -- Flow
                    // bans `this` references in object literals outright, and methods on
                    // classes/interfaces cannot be unbound from their declaring type -- so
                    // there is no value to importing the semantics. We accept the name
                    // in TS files and global libdefs, treating
                    // it as a structurally-empty interface (intersections like
                    // `M & ThisType<T>` reduce to `M`).
                    "ThisType"
                        if flow_common::files::has_ts_ext(cx.file())
                            || cx.is_global_libdef(cx.file()) =>
                    {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (_ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let interface_ast = empty_interface_annot(loc.dupe());
                            let result = convert_inner(cx, env, &interface_ast)?;
                            let (_, result_t) = result.loc();
                            Ok(reconstruct_ast(result_t.dupe(), None, targs_ast))
                        })?
                    }
                    // `$Shape` is deprecated in favor of `Partial`
                    "$Shape" => error_type(
                        cx,
                        loc.dupe(),
                        ErrorMessage::EIncorrectTypeWithReplacement(Box::new(
                            EIncorrectTypeWithReplacementData {
                                loc: loc.dupe(),
                                kind: intermediate_error_types::IncorrectType::Shape,
                            },
                        )),
                        t,
                    ),
                    // Omit<T, K> removes keys K from T.
                    "$Omit" => {
                        if !cx.is_global_lib_context() {
                            flow_js_utils::add_output_non_speculating(
                                cx,
                                ErrorMessage::EInternalType(
                                    loc.dupe(),
                                    InternalType::DollarUtilityTypeWithNonDollarAliases(
                                        FlowSmolStr::new_inline("Omit"),
                                    ),
                                ),
                            );
                        }
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 2, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let mut ts_iter = ts.into_iter();
                            let t1 = ts_iter.next().unwrap();
                            let t2 = ts_iter.next().unwrap();
                            let reason = reason::mk_reason(
                                reason::VirtualReasonDesc::RType(FlowSmolStr::new_inline("Omit")),
                                loc.dupe(),
                            );
                            let destructor =
                                type_::Destructor::MappedType(Box::new(DestructorMappedTypeData {
                                    homomorphic: type_::MappedTypeHomomorphicFlag::Unspecialized,
                                    property_type: type_::mixed_t::make(reason.dupe()),
                                    name_type: None,
                                    mapped_type_flags: type_::MappedTypeFlags {
                                        optional: type_::MappedTypeOptionality::KeepOptionality,
                                        variance: type_::MappedTypeVariance::KeepVariance,
                                    },
                                    distributive_tparam_name: None,
                                }));
                            let eval_id = EvalId::generate_id();
                            let mapped_use_op =
                                mapped_type_use_op(&reason, &t2, &destructor, eval_id.dupe());
                            let t2_mapped =
                                FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                                    cx,
                                    mapped_use_op,
                                    &reason,
                                    &t2,
                                    &destructor,
                                    eval_id,
                                )
                                .expect("mk_type_destructor should not fail");
                            let result_t =
                                FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                                    cx,
                                    use_op(&reason),
                                    &reason,
                                    &t1,
                                    &type_::Destructor::RestType(
                                        type_::object::rest::MergeMode::Omit,
                                        t2_mapped,
                                    ),
                                    mk_eval_id(cx, loc.dupe()),
                                )
                                .expect("mk_type_destructor should not fail");
                            Ok(reconstruct_ast(result_t, None, targs_ast))
                        })?
                    }
                    // $ReadOnly<T>
                    "$ReadOnly" => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EIncorrectTypeWithReplacement(Box::new(
                                EIncorrectTypeWithReplacementData {
                                    loc: loc.dupe(),
                                    kind: intermediate_error_types::IncorrectType::DollarReadOnly,
                                },
                            )),
                        );
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let tp = ts.into_iter().next().unwrap();
                            let reason = reason::mk_reason(None, loc.dupe());
                            let result_t =
                                FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                                    cx,
                                    type_::UseOp::Op(Arc::new(type_::RootUseOp::MakeReadOnly {
                                        loc: Box::new(loc.dupe()),
                                    })),
                                    &reason,
                                    &tp,
                                    &type_::Destructor::ReadOnlyType,
                                    mk_eval_id(cx, loc.dupe()),
                                )
                                .expect("mk_type_destructor should not fail");
                            Ok(reconstruct_ast(result_t, None, targs_ast))
                        })?
                    }
                    // $ReactDeepReadOnly<T>
                    "$ReactDeepReadOnly" => {
                        if !cx.is_global_lib_context() {
                            flow_js_utils::add_output_non_speculating(
                                cx,
                                ErrorMessage::EInternalType(
                                    loc.dupe(),
                                    InternalType::DollarReactDeepReadOnly,
                                ),
                            );
                        }
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let tp = ts.into_iter().next().unwrap();
                            let reason = reason::mk_reason(None, loc.dupe());
                            let result_t =
                                FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                                    cx,
                                    type_::UseOp::Op(Arc::new(type_::RootUseOp::MakeReadOnly {
                                        loc: Box::new(loc.dupe()),
                                    })),
                                    &reason,
                                    &tp,
                                    &type_::Destructor::ReactDRO(Box::new(type_::ReactDro(
                                        loc.dupe(),
                                        type_::DroType::DebugAnnot,
                                    ))),
                                    mk_eval_id(cx, loc.dupe()),
                                )
                                .expect("mk_type_destructor should not fail");
                            Ok(reconstruct_ast(result_t, None, targs_ast))
                        })?
                    }
                    // $Keys<T> is the set of keys of T
                    "$Keys" => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EIncorrectTypeWithReplacement(Box::new(
                                EIncorrectTypeWithReplacementData {
                                    loc: loc.dupe(),
                                    kind: intermediate_error_types::IncorrectType::DollarKeys,
                                },
                            )),
                        );
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let tp = ts.into_iter().next().unwrap();
                            Ok(reconstruct_ast(
                                Type::new(type_::TypeInner::KeysT(
                                    reason::mk_reason(None, loc.dupe()),
                                    tp,
                                )),
                                None,
                                targs_ast,
                            ))
                        })?
                    }
                    // $Values<T> is a union of all the own enumerable value types of T
                    "$Values" => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EIncorrectTypeWithReplacement(Box::new(
                                EIncorrectTypeWithReplacementData {
                                    loc: loc.dupe(),
                                    kind: intermediate_error_types::IncorrectType::DollarValues,
                                },
                            )),
                        );
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let tp = ts.into_iter().next().unwrap();
                            let reason = reason::mk_reason(
                                reason::VirtualReasonDesc::RType(FlowSmolStr::new_inline(
                                    "$Values",
                                )),
                                loc.dupe(),
                            );
                            let result_t =
                                FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                                    cx,
                                    use_op(&reason),
                                    &reason,
                                    &tp,
                                    &type_::Destructor::ValuesType,
                                    mk_eval_id(cx, loc.dupe()),
                                )
                                .expect("mk_type_destructor should not fail");
                            Ok(reconstruct_ast(result_t, None, targs_ast))
                        })?
                    }
                    // Values<T> replaces $Values<T>
                    "Values" => {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let tp = ts.into_iter().next().unwrap();
                            let reason = reason::mk_reason(
                                reason::VirtualReasonDesc::RType(FlowSmolStr::new_inline("Values")),
                                loc.dupe(),
                            );
                            let result_t =
                                FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                                    cx,
                                    use_op(&reason),
                                    &reason,
                                    &tp,
                                    &type_::Destructor::ValuesType,
                                    mk_eval_id(cx, loc.dupe()),
                                )
                                .expect("mk_type_destructor should not fail");
                            Ok(reconstruct_ast(result_t, None, targs_ast))
                        })?
                    }
                    "$Exact" => {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let tp = ts.into_iter().next().unwrap();
                            let desc = type_util::desc_of_t(&tp).cloned();
                            let reason = reason::mk_annot_reason(desc, loc.dupe());
                            let tp = type_util::push_type_alias_reason(&reason, tp);
                            let result_t =
                                FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                                    cx,
                                    use_op(&reason),
                                    &reason,
                                    &tp,
                                    &type_::Destructor::ExactType,
                                    mk_eval_id(cx, loc.dupe()),
                                )
                                .expect("mk_type_destructor should not fail");
                            Ok(reconstruct_ast(result_t, None, targs_ast))
                        })?
                    }
                    // Exports<'M'> is the type of the exports of module 'M'
                    "$Exports" => {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            Ok(match &inner.targs {
                                Some(targs_node)
                                    if !targs_node.arguments.is_empty()
                                        && let TypeInner::StringLiteral {
                                            loc: str_loc,
                                            literal: str_lit,
                                        } = targs_node.arguments[0].deref() =>
                                {
                                    let value = &str_lit.value;
                                    let import_specifier =
                                        Userland::from_smol_str(FlowSmolStr::new(value.as_str()));
                                    let remote_module: Result<type_::ModuleType, Type> = match cx
                                        .builtin_module_opt(&import_specifier)
                                    {
                                        Some((_, m)) => Ok(m.get_forced(cx).dupe()),
                                        None => Err(match flow_js_utils::lookup_builtin_module_error(
                                            cx,
                                            value,
                                            loc.dupe(),
                                        ) {
                                            Ok(v) => v,
                                            Err(flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c)) => return Err(flow_utils_concurrency::job_error::JobError::Canceled(c)),
                                            Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t)),
                                            Err(err) => panic!("Should not be under speculation: {:?}", err),
                                        }),
                                    };
                                    let str_t = mk_singleton_string(str_loc.dupe(), value.as_str());
                                    let module_reason = reason::mk_annot_reason(
                                        reason::VirtualReasonDesc::RModule(import_specifier.dupe()),
                                        loc.dupe(),
                                    );
                                    let namespace_symbol =
                                        Symbol::mk_module_symbol(value.dupe(), loc.dupe());
                                    let (_def_loc_opt, require_t) =
                                        match flow_js_utils::import_export_utils::cjs_require_type(
                                            cx,
                                            module_reason,
                                            |cx, loc, t| {
                                                flow_js::reposition_non_speculating(cx, loc, t)
                                            },
                                            namespace_symbol,
                                            false,
                                            &remote_module,
                                        ) {
                                            Ok(v) => v,
                                            Err(
                                                flow_js_utils::FlowJsException::WorkerCanceled(c),
                                            ) => {
                                                return Err(flow_utils_concurrency::job_error::JobError::Canceled(c));
                                            }
                                            Err(flow_js_utils::FlowJsException::TimedOut(t)) => {
                                                return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
                                            }
                                            Err(_) => panic!("Should not be under speculation"),
                                        };
                                    let str_ast = ast::types::Type::new(TypeInner::StringLiteral {
                                        loc: (str_loc.dupe(), str_t),
                                        literal: str_lit.clone(),
                                    });
                                    reconstruct_ast(
                                        require_t.dupe(),
                                        None,
                                        Some(ast::types::TypeArgs::<ALoc, (ALoc, Type)> {
                                            loc: targs_node.loc.dupe(),
                                            arguments: Arc::from(vec![str_ast]),
                                            comments: targs_node.comments.dupe(),
                                        }),
                                    )
                                }
                                _ => error_type(
                                    cx,
                                    loc.dupe(),
                                    ErrorMessage::EExportsAnnot(loc.dupe()),
                                    t,
                                ),
                            })
                        })?
                    }
                    "$KeyMirror" => {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let t1 = ts.into_iter().next().unwrap();
                            let reason = reason::mk_reason(None, loc.dupe());
                            let type_reason = reason.dupe().replace_desc(VirtualReasonDesc::RType(
                                FlowSmolStr::new_inline("$KeyMirror"),
                            ));
                            let result_t =
                                FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                                    cx,
                                    use_op(&type_reason),
                                    &reason,
                                    &t1,
                                    &type_::Destructor::TypeMap(type_::TypeMap::ObjectKeyMirror),
                                    mk_eval_id(cx, loc.dupe()),
                                )
                                .expect("mk_type_destructor should not fail");
                            Ok(reconstruct_ast(result_t, None, targs_ast))
                        })?
                    }
                    // Class<T> is the type of the class whose instances are of type T
                    "Class" => {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let tp = ts.into_iter().next().unwrap();
                            let reason = reason::mk_reason(None, loc.dupe());
                            Ok(reconstruct_ast(
                                Type::new(type_::TypeInner::DefT(
                                    reason,
                                    type_::DefT::new(type_::DefTInner::ClassT(tp)),
                                )),
                                None,
                                targs_ast,
                            ))
                        })?
                    }
                    "$EnumValue" => {
                        if !cx.is_global_lib_context() {
                            flow_js_utils::add_output_non_speculating(
                                cx,
                                ErrorMessage::EInternalType(
                                    loc.dupe(),
                                    InternalType::DollarUtilityTypeWithNonDollarAliases(
                                        FlowSmolStr::new_inline("EnumValue"),
                                    ),
                                ),
                            );
                        }
                        let reason = reason::mk_annot_reason(
                            reason::VirtualReasonDesc::REnum { name: None },
                            loc.dupe(),
                        );
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let representation_t = ts.into_iter().next().unwrap();
                            Ok(reconstruct_ast(
                                Type::new(type_::TypeInner::DefT(
                                    reason,
                                    type_::DefT::new(type_::DefTInner::EnumValueT(Rc::new(
                                        type_::EnumInfo::new(type_::EnumInfoInner::AbstractEnum {
                                            representation_t,
                                        }),
                                    ))),
                                )),
                                None,
                                targs_ast,
                            ))
                        })?
                    }
                    "$Enum" => {
                        if !cx.is_global_lib_context() {
                            flow_js_utils::add_output_non_speculating(
                                cx,
                                ErrorMessage::EInternalType(
                                    loc.dupe(),
                                    InternalType::DollarUtilityTypeWithNonDollarAliases(
                                        FlowSmolStr::new_inline("Enum"),
                                    ),
                                ),
                            );
                        }
                        let reason = reason::mk_annot_reason(
                            reason::VirtualReasonDesc::REnum { name: None },
                            loc.dupe(),
                        );
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let tp = ts.into_iter().next().unwrap();
                            let result_t =
                                FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                                    cx,
                                    use_op(&reason),
                                    &reason,
                                    &tp,
                                    &type_::Destructor::EnumType,
                                    mk_eval_id(cx, loc.dupe()),
                                )
                                .expect("mk_type_destructor should not fail");
                            Ok(reconstruct_ast(result_t, None, targs_ast))
                        })?
                    }
                    "$Flow$EnforceOptimized" => {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let tp = ts.into_iter().next().unwrap();
                            cx.set_union_opt(loc.dupe(), tp.dupe());
                            Ok(reconstruct_ast(tp, None, targs_ast))
                        })?
                    }
                    "Readonly" => {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let tp = ts.into_iter().next().unwrap();
                            let reason = reason::mk_reason(None, loc.dupe());
                            let result_t =
                                FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                                    cx,
                                    type_::UseOp::Op(Arc::new(type_::RootUseOp::MakeReadOnly {
                                        loc: Box::new(loc.dupe()),
                                    })),
                                    &reason,
                                    &tp,
                                    &type_::Destructor::ReadOnlyType,
                                    mk_eval_id(cx, loc.dupe()),
                                )
                                .expect("mk_type_destructor should not fail");
                            Ok(reconstruct_ast(result_t, None, targs_ast))
                        })?
                    }
                    "ReadonlyArray" => {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (elemts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let elemt = elemts.into_iter().next().unwrap();
                            // As with `Array` above, the head still names the
                            // `ReadonlyArray` class, so record that for hover.
                            let id_t = unapplied_type_identifier_opt(cx, name, name_loc.dupe())?;
                            Ok(reconstruct_ast(
                                Type::new(type_::TypeInner::DefT(
                                    reason::mk_annot_reason(
                                        reason::VirtualReasonDesc::RROArrayType,
                                        loc.dupe(),
                                    ),
                                    type_::DefT::new(type_::DefTInner::ArrT(Rc::new(
                                        type_::ArrType::ROArrayAT(Box::new(ROArrayATData {
                                            elem_t: elemt,
                                            react_dro: None,
                                            literal: false,
                                        })),
                                    ))),
                                )),
                                id_t,
                                targs_ast,
                            ))
                        })?
                    }
                    "NonNullable" => {
                        check_type_arg_arity(cx, loc.dupe(), t, inner.targs.as_ref(), 1, |cx| {
                            let (ts, targs_ast) =
                                convert_type_params(cx, env, inner.targs.as_ref())?;
                            let tp = ts.into_iter().next().unwrap();
                            let reason = reason::mk_reason(
                                reason::VirtualReasonDesc::RType(FlowSmolStr::new_inline(
                                    "NonNullable",
                                )),
                                loc.dupe(),
                            );
                            let result_t =
                                FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                                    cx,
                                    use_op(&reason),
                                    &reason,
                                    &tp,
                                    &type_::Destructor::NonMaybeType,
                                    mk_eval_id(cx, loc.dupe()),
                                )
                                .expect("mk_type_destructor should not fail");
                            Ok(reconstruct_ast(result_t, None, targs_ast))
                        })?
                    }
                    "React$Node" => {
                        if !cx.is_global_lib_context() {
                            flow_js_utils::add_output_non_speculating(
                                cx,
                                ErrorMessage::EInternalType(
                                    loc.dupe(),
                                    InternalType::ReactDollarUtilityTypesWithNonDollarAliases(
                                        FlowSmolStr::new_inline("Node"),
                                    ),
                                ),
                            );
                        }
                        local_generic_type(
                            cx,
                            env,
                            name,
                            name_loc.dupe(),
                            loc.dupe(),
                            inner.targs.as_ref(),
                            &reconstruct_ast,
                        )?
                    }
                    "$ReadOnlyMap" => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EIncorrectTypeWithReplacement(Box::new(
                                EIncorrectTypeWithReplacementData {
                                    loc: loc.dupe(),
                                    kind:
                                        intermediate_error_types::IncorrectType::DollarReadOnlyMap,
                                },
                            )),
                        );
                        local_generic_type(
                            cx,
                            env,
                            name,
                            name_loc.dupe(),
                            loc.dupe(),
                            inner.targs.as_ref(),
                            &reconstruct_ast,
                        )?
                    }
                    "$ReadOnlySet" => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EIncorrectTypeWithReplacement(Box::new(
                                EIncorrectTypeWithReplacementData {
                                    loc: loc.dupe(),
                                    kind:
                                        intermediate_error_types::IncorrectType::DollarReadOnlySet,
                                },
                            )),
                        );
                        local_generic_type(
                            cx,
                            env,
                            name,
                            name_loc.dupe(),
                            loc.dupe(),
                            inner.targs.as_ref(),
                            &reconstruct_ast,
                        )?
                    }
                    "$ReadOnlyWeakMap" => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EIncorrectTypeWithReplacement(Box::new(EIncorrectTypeWithReplacementData {
                                loc: loc.dupe(),
                                kind:
                                    intermediate_error_types::IncorrectType::DollarReadOnlyWeakMap,
                            })),
                        );
                        local_generic_type(
                            cx,
                            env,
                            name,
                            name_loc.dupe(),
                            loc.dupe(),
                            inner.targs.as_ref(),
                            &reconstruct_ast,
                        )?
                    }
                    "$ReadOnlyWeakSet" => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EIncorrectTypeWithReplacement(Box::new(EIncorrectTypeWithReplacementData {
                                loc: loc.dupe(),
                                kind:
                                    intermediate_error_types::IncorrectType::DollarReadOnlyWeakSet,
                            })),
                        );
                        local_generic_type(
                            cx,
                            env,
                            name,
                            name_loc.dupe(),
                            loc.dupe(),
                            inner.targs.as_ref(),
                            &reconstruct_ast,
                        )?
                    }
                    // other applications with id as head expr
                    _ => local_generic_type(
                        cx,
                        env,
                        name,
                        name_loc.dupe(),
                        loc.dupe(),
                        inner.targs.as_ref(),
                        &reconstruct_ast,
                    )?,
                }
            }
        },
        TypeInner::Function { loc, inner } => {
            let params_node = &inner.params;
            let params_loc = &params_node.loc;
            let ps = &params_node.params;
            let rest = &params_node.rest;
            let this_ = &params_node.this;
            let params_comments = &params_node.comments;
            let func_comments = &inner.comments;
            let effect_ = &inner.effect;
            error_on_unsupported_variance_annotation(cx, "function type", inner.tparams.as_ref());
            let saved_tparams_map = env.tparams_map.dupe();
            let (tparams, tparams_ast) = mk_type_param_declarations_inner(
                cx,
                env,
                flow_parser::ast_visitor::TypeParamsContext::FunctionType,
                inner.tparams.as_ref(),
            )?;
            let mut params = Vec::new();
            let mut param_asts = Vec::new();
            for param in ps.iter() {
                let param_loc = &param.loc;
                let (name, annot, optional) =
                    flow_parser::ast_utils::function_type_param_parts(&param.param);
                let annot_ast = convert_inner(cx, env, annot)?;
                let (_, t) = annot_ast.loc();
                let t = t.dupe();
                let t = if optional {
                    type_util::optional(t, None, false)
                } else {
                    t
                };
                let fun_param_name = name.map(ident_name);
                params.push(type_::FunParam(fun_param_name, t.dupe()));
                let param_ast = typed_function_param_ast(&param.param, t, annot_ast);
                param_asts.push(ast::types::function::Param {
                    loc: param_loc.dupe(),
                    param: param_ast,
                });
            }
            let (this_t, this_param_ast) = match this_ {
                // A function type with no `this` param can be called with any
                // receiver, so its implicit `this` is `mixed` and the
                // contravariant check in `funt_to_funt_check_this_contravariant`
                // rejects methods that need a real one.
                None => (type_::dummy_this(params_loc.dupe()), None),
                Some(tp) => {
                    let this_loc = &tp.loc;
                    let annot = &tp.annot;
                    let comments = &tp.comments;
                    let annot_ast = convert_inner(cx, env, &annot.annotation)?;
                    let (_, this_t) = annot_ast.loc();
                    let this_t = this_t.dupe();
                    (
                        this_t,
                        Some(ast::types::function::ThisParam {
                            loc: this_loc.dupe(),
                            annot: ast::types::Annotation {
                                loc: annot.loc.dupe(),
                                annotation: annot_ast,
                            },
                            comments: comments.clone(),
                        }),
                    )
                }
            };
            let reason =
                reason::mk_annot_reason(reason::VirtualReasonDesc::RFunctionType, loc.dupe());
            let (rest_param, rest_param_ast) = match rest {
                Some(rp) => {
                    let rest_loc = &rp.loc;
                    let param = &rp.argument;
                    let param_loc = &param.loc;
                    let (name, annot, _optional) =
                        flow_parser::ast_utils::function_type_param_parts(&param.param);
                    let annot_ast = convert_inner(cx, env, annot)?;
                    let (_, rest_t) = annot_ast.loc();
                    let rest_t = rest_t.dupe();
                    let rest_name = name.map(ident_name);
                    let rest_param = Some(type_::FunRestParam(
                        rest_name,
                        type_util::loc_of_t(&rest_t).dupe(),
                        rest_t.dupe(),
                    ));
                    let rest_param_ast_inner =
                        typed_function_param_ast(&param.param, rest_t, annot_ast);
                    let rest_ast = Some(ast::types::function::RestParam {
                        loc: rest_loc.dupe(),
                        argument: ast::types::function::Param {
                            loc: param_loc.dupe(),
                            param: rest_param_ast_inner,
                        },
                        comments: rp.comments.dupe(),
                    });
                    (rest_param, rest_ast)
                }
                None => (None, None),
            };
            let params_ast_node = ast::types::function::Params {
                loc: params_loc.dupe(),
                params: param_asts.into(),
                rest: rest_param_ast,
                this: this_param_ast,
                comments: params_comments.clone(),
            };
            let fparams = params;
            let (return_t, return_ast, type_guard) = convert_return_annotation_inner(
                cx,
                env,
                MethodKind::FunctionKind,
                &inner.params,
                &fparams,
                &inner.return_,
            )?;
            env.tparams_map = saved_tparams_map;
            let statics_reason = reason.dupe().replace_desc(None);
            let fun_proto_t = Type::new(type_::TypeInner::FunProtoT(statics_reason.dupe()));
            let statics_t = flow_typing_flow_common::obj_type::mk_with_proto(
                cx,
                statics_reason,
                type_::ObjKind::Inexact,
                None,
                None,
                None,
                None,
                fun_proto_t,
                false,
            );
            let effect_flag = match effect_ {
                ast::function::Effect::Hook => type_::ReactEffectType::HookAnnot,
                ast::function::Effect::Arbitrary => type_::ReactEffectType::ArbitraryEffect,
            };
            let ft = Type::new(type_::TypeInner::DefT(
                reason.dupe(),
                type_::DefT::new(type_::DefTInner::FunT(
                    statics_t,
                    Rc::new(type_::FunType {
                        this_t: (this_t, type_::ThisStatus::ThisFunction),
                        params: fparams.into(),
                        rest_param,
                        return_t,
                        type_guard,
                        def_reason: reason.dupe(),
                        function_reference_kind:
                            flow_common::error_ref::FunctionReferenceKind::FunctionType,
                        effect_: effect_flag,
                        strictness_kind: cx.type_strictness_kind(),
                        literal: false,
                    }),
                )),
            ));
            let t = match tparams {
                None => ft,
                Some((ref tparams_loc, ref tparams_nel)) => {
                    let id = cx.make_source_poly_id(false, tparams_loc);
                    type_util::poly_type(
                        id,
                        tparams_loc.dupe(),
                        tparams_nel.clone(),
                        ft,
                        cx.type_strictness_kind(),
                        PolyTKind::Regular,
                    )
                }
            };
            ast::types::Type::new(TypeInner::Function {
                loc: (loc.dupe(), t),
                inner: ast::types::Function {
                    tparams: tparams_ast,
                    params: params_ast_node,
                    return_: return_ast,
                    comments: func_comments.clone(),
                    effect: effect_.clone(),
                }
                .into(),
            })
        }
        TypeInner::Component { loc, inner } => {
            let reason = reason::mk_reason(None, loc.dupe());
            let (t, tparams_ast, params_ast, renders_ast) = mk_component(
                cx,
                env,
                reason,
                None,
                inner.tparams.as_ref(),
                &inner.params,
                &inner.renders,
            )?;
            ast::types::Type::new(TypeInner::Component {
                loc: (loc.dupe(), t),
                inner: (ast::types::Component {
                    tparams: tparams_ast,
                    params: params_ast,
                    renders: renders_ast,
                    comments: inner.comments.dupe(),
                })
                .into(),
            })
        }
        TypeInner::Object {
            loc: obj_loc,
            inner,
        } if inner.properties.len() == 1
            && let ast::types::object::Property::MappedType(mt) = &inner.properties[0] =>
        {
            let exact = inner.exact;
            let inexact = inner.inexact;
            let comments = &inner.comments;
            let mapped_type_loc = &mt.loc;
            let key_tparam = &mt.key_tparam;
            let prop_type = &mt.prop_type;
            let source_type = &mt.source_type;
            let name_type = &mt.name_type;
            let variance = &mt.variance;
            let variance_op = &mt.variance_op;
            let optional = mt.optional;
            let mapped_type_comments = &mt.comments;
            if exact || inexact {
                // Mapped types are implemented with the following limitations:
                // 1. Mapped types cannot be declared with additional properties
                // 2. Mapped types do not support explicit exact or inexact modifiers
                // 3. Mapped types must use an inline keyof
                // All of these conditions are checked in this case, and the extra properties
                // case is additionally checked in the normal object type case. If any of these
                // conditions are violated then the result is Any
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EInvalidMappedType {
                        loc: obj_loc.dupe(),
                        kind: flow_typing_errors::error_message::InvalidMappedTypeErrorKind::ExplicitExactOrInexact,
                    },
                );
                {
                    let Ok(v) = polymorphic_ast_mapper::type_(&mut typed_ast_utils::ErrorMapper, t);
                    v
                }
            } else if (variance_op.is_some() || name_type.is_some()) && !cx.tslib_syntax() {
                if variance_op.is_some() {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EUnsupportedSyntax(Box::new((
                            mapped_type_loc.dupe(),
                            flow_typing_errors::intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                                flow_typing_errors::intermediate_error_types::TsLibSyntaxKind::ReadonlyMappedTypeVarianceOp,
                            ),
                        ))),
                    );
                }
                if name_type.is_some() {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EUnsupportedSyntax(Box::new((
                            obj_loc.dupe(),
                            flow_typing_errors::intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                                flow_typing_errors::intermediate_error_types::TsLibSyntaxKind::MappedTypeKeyRemapping,
                            ),
                        ))),
                    );
                }
                {
                    let Ok(v) = polymorphic_ast_mapper::type_(&mut typed_ast_utils::ErrorMapper, t);
                    v
                }
            } else {
                use flow_parser::ast::types::object::MappedTypeOptionalFlag;
                let mapped_type_optionality = match optional {
                    MappedTypeOptionalFlag::PlusOptional | MappedTypeOptionalFlag::Optional => {
                        type_::MappedTypeOptionality::MakeOptional
                    }
                    MappedTypeOptionalFlag::MinusOptional => {
                        type_::MappedTypeOptionality::RemoveOptional
                    }
                    MappedTypeOptionalFlag::NoOptionalFlag => {
                        type_::MappedTypeOptionality::KeepOptionality
                    }
                };
                let (homomorphic, source_type_t, source_ast, distributive_tparam_name) = {
                    use flow_parser::ast::types::TypeInner;
                    use flow_parser::ast::types::generic::Identifier as GenericId;
                    match source_type.deref() {
                        TypeInner::Generic {
                            inner: gen_inner, ..
                        } if let GenericId::Unqualified(ident) = &gen_inner.id => {
                            let gen_name = &ident.name;
                            let subst_name = SubstName::name(gen_name.dupe());
                            use flow_typing_type::type_::TypeInner as TI;

                            match env.tparams_map.get(&subst_name).map(|v| v.dupe()) {
                                Some(ref tparam_type)
                                    if let TI::GenericT(box GenericTData { bound, .. }) =
                                        tparam_type.deref()
                                        && let TI::KeysT(_, obj_t) = bound.deref() =>
                                {
                                    let source_ast = convert_inner(cx, env, source_type)?;
                                    let (_, selected_keys) = source_ast.loc();
                                    let selected_keys = selected_keys.dupe();
                                    let distributive_tparam_name = match obj_t.deref() {
                                        TI::GenericT(box GenericTData {
                                            name: obj_name,
                                            reason: obj_reason,
                                            ..
                                        }) => use_distributive_tparam_name(
                                            cx,
                                            obj_name.string_of_subst_name(),
                                            obj_reason.def_loc().dupe(),
                                            &mut env.tparams_map,
                                        ),
                                        _ => None,
                                    };
                                    (
                                        type_::MappedTypeHomomorphicFlag::SemiHomomorphic(
                                            selected_keys,
                                        ),
                                        obj_t.dupe(),
                                        source_ast,
                                        distributive_tparam_name,
                                    )
                                }
                                _ => {
                                    let source_ast = convert_inner(cx, env, source_type)?;
                                    let (_, source_type_t) = source_ast.loc();
                                    let source_type_t = source_type_t.dupe();
                                    (
                                        type_::MappedTypeHomomorphicFlag::Unspecialized,
                                        source_type_t,
                                        source_ast,
                                        None,
                                    )
                                }
                            }
                        }
                        TypeInner::Keyof {
                            loc: keyof_loc,
                            inner: keyof_inner,
                        } => {
                            let argument = &keyof_inner.argument;
                            let keyof_comments = &keyof_inner.comments;
                            let arg_ast = convert_inner(cx, env, argument)?;
                            let (_, source_type_t) = arg_ast.loc();
                            let source_type_t = source_type_t.dupe();
                            let source_ast = ast::types::Type::new(TypeInner::Keyof {
                                loc: (keyof_loc.dupe(), source_type_t.dupe()),
                                inner: (ast::types::Keyof {
                                    argument: arg_ast,
                                    comments: keyof_comments.clone(),
                                })
                                .into(),
                            });
                            let distributive_tparam_name = use_distributive_tparam_name_from_ast(
                                cx,
                                argument,
                                &mut env.tparams_map,
                            );
                            (
                                type_::MappedTypeHomomorphicFlag::Homomorphic,
                                source_type_t,
                                source_ast,
                                distributive_tparam_name,
                            )
                        }
                        _ => {
                            let source_ast = convert_inner(cx, env, source_type)?;
                            let (_, source_type_t) = source_ast.loc();
                            let source_type_t = source_type_t.dupe();
                            (
                                type_::MappedTypeHomomorphicFlag::Unspecialized,
                                source_type_t,
                                source_ast,
                                None,
                            )
                        }
                    }
                };
                let (tparam_ast, tparam, tparam_t) = mk_type_param_inner(
                    cx,
                    env,
                    flow_parser::ast_visitor::TypeParamsContext::ObjectMappedType,
                    key_tparam,
                )?;
                let tparam_name = tparam.name.dupe();
                env.tparams_map.insert(tparam_name.dupe(), tparam_t);
                let prop_type_ast = convert_inner(cx, env, prop_type)?;
                let (prop_loc, prop_type_t) = prop_type_ast.loc();
                let prop_loc = prop_loc.dupe();
                let prop_type_t = prop_type_t.dupe();
                let tparam_for_prop = tparam.clone();
                let poly_prop_type = type_util::poly_type_of_tparams(
                    cx.make_source_poly_id(false, &prop_loc),
                    Some((key_tparam.loc.dupe(), vec1::Vec1::new(tparam_for_prop))),
                    prop_type_t,
                    cx.type_strictness_kind(),
                    PolyTKind::MappedType,
                );
                let (poly_name_type, name_type_ast) = match name_type {
                    None => (None, None),
                    Some(nt) => {
                        let nt_ast = convert_inner(cx, env, nt)?;
                        let (name_loc, name_t) = nt_ast.loc();
                        let name_loc = name_loc.dupe();
                        let name_t = name_t.dupe();
                        let poly_nt = type_util::poly_type_of_tparams(
                            cx.make_source_poly_id(false, &name_loc),
                            Some((key_tparam.loc.dupe(), vec1::Vec1::new(tparam.clone()))),
                            name_t,
                            cx.type_strictness_kind(),
                            PolyTKind::MappedType,
                        );
                        (Some(poly_nt.dupe()), Some(nt_ast))
                    }
                };
                let reason = reason::mk_reason(None, obj_loc.dupe());
                let mapped_type_variance = {
                    use flow_parser::ast::types::object::MappedTypeVarianceOp;
                    match (variance, variance_op) {
                        (Some(v), Some(MappedTypeVarianceOp::Add))
                            if v.kind == flow_parser::ast::VarianceKind::Readonly =>
                        {
                            type_::MappedTypeVariance::OverrideVariance(Polarity::Positive)
                        }
                        (Some(v), None) if v.kind == flow_parser::ast::VarianceKind::Readonly => {
                            type_::MappedTypeVariance::OverrideVariance(Polarity::Positive)
                        }
                        (Some(v), Some(MappedTypeVarianceOp::Remove))
                            if v.kind == flow_parser::ast::VarianceKind::Readonly =>
                        {
                            type_::MappedTypeVariance::RemoveVariance(Polarity::Positive)
                        }
                        (Some(v), Some(MappedTypeVarianceOp::Add))
                            if v.kind == flow_parser::ast::VarianceKind::Writeonly =>
                        {
                            type_::MappedTypeVariance::OverrideVariance(Polarity::Negative)
                        }
                        (Some(v), None) if v.kind == flow_parser::ast::VarianceKind::Writeonly => {
                            type_::MappedTypeVariance::OverrideVariance(Polarity::Negative)
                        }
                        (Some(v), Some(MappedTypeVarianceOp::Remove))
                            if v.kind == flow_parser::ast::VarianceKind::Writeonly =>
                        {
                            type_::MappedTypeVariance::RemoveVariance(Polarity::Negative)
                        }
                        _ => {
                            let pol = polarity(
                                            cx,
                                            flow_typing_errors::intermediate_error_types::VarianceSigilParent::Property,
                                            variance.as_ref(),
                                        );
                            if pol == Polarity::Neutral {
                                type_::MappedTypeVariance::KeepVariance
                            } else {
                                type_::MappedTypeVariance::OverrideVariance(pol)
                            }
                        }
                    }
                };
                let destructor =
                    type_::Destructor::MappedType(Box::new(DestructorMappedTypeData {
                        homomorphic,
                        property_type: poly_prop_type.dupe(),
                        name_type: poly_name_type,
                        mapped_type_flags: type_::MappedTypeFlags {
                            optional: mapped_type_optionality,
                            variance: mapped_type_variance,
                        },
                        distributive_tparam_name,
                    }));
                let eval_id = EvalId::generate_id();
                let use_op =
                    mapped_type_use_op(&reason, &source_type_t, &destructor, eval_id.dupe());
                let eval_t = FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                    cx,
                    use_op,
                    &reason,
                    &source_type_t,
                    &destructor,
                    eval_id,
                )
                .expect("mk_type_destructor not in speculation");
                let prop_ast =
                    ast::types::object::Property::MappedType(ast::types::object::MappedType {
                        loc: mapped_type_loc.dupe(),
                        source_type: source_ast,
                        prop_type: prop_type_ast,
                        key_tparam: tparam_ast,
                        name_type: name_type_ast,
                        variance: variance.clone(),
                        variance_op: *variance_op,
                        optional,
                        comments: mapped_type_comments.clone(),
                    });
                ast::types::Type::new(TypeInner::Object {
                    loc: (obj_loc.dupe(), eval_t),
                    inner: (ast::types::Object {
                        exact,
                        inexact,
                        properties: vec![prop_ast].into(),
                        comments: comments.clone(),
                    })
                    .into(),
                })
            }
        }
        TypeInner::Object { loc, inner } => {
            let exact = inner.exact;
            let inexact = inner.inexact;
            let properties = &inner.properties;
            let comments = &inner.comments;
            let exact_type = exact || !inexact;
            let mut has_indexer = false;
            let mut mapped_type_loc: Option<ALoc> = None;
            let mut has_construct_sig = false;
            let mut has_spread = false;
            for property in properties.iter() {
                match property {
                    ast::types::object::Property::Indexer(_) => {
                        has_indexer = true;
                    }
                    ast::types::object::Property::MappedType(mt) => {
                        mapped_type_loc = Some(mt.loc.dupe());
                    }
                    ast::types::object::Property::NormalProperty(np) => {
                        has_construct_sig = has_construct_sig || is_construct_signature(np);
                    }
                    ast::types::object::Property::SpreadProperty(_) => {
                        has_spread = true;
                    }
                    _ => {}
                }
            }
            match mapped_type_loc {
                Some(_) => {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EInvalidMappedType {
                            loc: loc.dupe(),
                            kind: flow_typing_errors::error_message::InvalidMappedTypeErrorKind::ExtraProperties,
                        },
                    );
                    {
                        let Ok(v) =
                            polymorphic_ast_mapper::type_(&mut typed_ast_utils::ErrorMapper, t);
                        v
                    }
                }
                None => {
                    let mut env = env.clone();
                    env.tparams_map
                        .remove(&SubstName::name(FlowSmolStr::new_inline("this")));
                    // An `ObjT` has a call slot but no construct slot, so an object
                    // type carrying `new(): T` is lowered through the interface path
                    // instead — the same desugar `new (...) => T` gets above. A
                    // spread has no interface equivalent ([add_interface_properties]
                    // rejects it), so those keep the object lowering and the `new`
                    // member stays an ordinary method.
                    //
                    // Gated on the file the annotation is written in: TypeScript and
                    // library definitions spell a construct signature this way, whereas
                    // Flow source has always read `{new(): T}` as an object with a method
                    // named `new`. A Flow file consuming such a declaration only reads the
                    // resulting type, so it constructs just fine.
                    let (obj_t, properties_ast) = if has_construct_sig
                        && !has_spread
                        && (flow_common::files::has_ts_ext(cx.file()) || cx.is_global_lib_context())
                    {
                        let reason = reason::mk_annot_reason(
                            reason::VirtualReasonDesc::RObjectType,
                            loc.dupe(),
                        );
                        mk_inline_interface_type(cx, &mut env, loc, reason, Vec::new(), properties)?
                    } else {
                        convert_object(cx, &mut env, loc.dupe(), exact_type, properties)?
                    };
                    if !exact && !inexact && !has_indexer {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EAmbiguousObjectType(loc.dupe()),
                        );
                    }
                    ast::types::Type::new(TypeInner::Object {
                        loc: (loc.dupe(), obj_t),
                        inner: (ast::types::Object {
                            exact,
                            inexact,
                            properties: properties_ast.into(),
                            comments: comments.clone(),
                        })
                        .into(),
                    })
                }
            }
        }
        TypeInner::Interface { loc, inner } => {
            let (ref body_loc, ref body) = inner.body;
            let properties = &body.properties;
            let body_exact = body.exact;
            let object_comments = &body.comments;
            let extends = &inner.extends;
            let iface_comments = &inner.comments;
            let reason = reason::mk_annot_reason(None, loc.dupe());
            let mut env = env.clone();
            env.tparams_map
                .remove(&SubstName::name(FlowSmolStr::new_inline("this")));
            let mut extends_supers = Vec::new();
            let mut extend_asts = Vec::new();
            for ext in extends.iter() {
                let (super_, ext_ast) = mk_interface_super(cx, &mut env, ext)?;
                extends_supers.push(super_);
                extend_asts.push(ext_ast);
            }
            let (iface_t, property_asts) =
                mk_inline_interface_type(cx, &mut env, loc, reason, extends_supers, properties)?;
            ast::types::Type::new(TypeInner::Interface {
                loc: (loc.dupe(), iface_t),
                inner: ast::types::Interface {
                    body: (
                        body_loc.dupe(),
                        ast::types::Object {
                            exact: body_exact,
                            inexact: false,
                            properties: property_asts.into(),
                            comments: object_comments.clone(),
                        },
                    ),
                    extends: extend_asts.into(),
                    comments: iface_comments.clone(),
                }
                .into(),
            })
        }
        TypeInner::TemplateLiteral { loc, inner } => {
            if !cx.tslib_syntax() {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EUnsupportedSyntax(Box::new((
                        loc.dupe(),
                        intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                            TsLibSyntaxKind::TemplateLiteralType,
                        ),
                    ))),
                );
                let Ok(v) = polymorphic_ast_mapper::type_(&mut typed_ast_utils::ErrorMapper, t);
                v
            } else {
                let quasis = inner.quasis.iter().map(|q| q.value.cooked.dupe()).collect();
                let (ts, type_asts) = convert_list_inner(cx, env, &inner.types)?;
                let result_t = template_literal_type::resolve(quasis, ts, loc.dupe(), cx);
                ast::types::Type::new(TypeInner::TemplateLiteral {
                    loc: (loc.dupe(), result_t),
                    inner: ast::types::TypeTemplateLiteral {
                        quasis: inner.quasis.clone(),
                        types: type_asts.into(),
                        comments: inner.comments.clone(),
                    }
                    .into(),
                })
            }
        }
        TypeInner::ConstructorType {
            loc,
            abstract_,
            inner: func,
        } => {
            if !cx.tslib_syntax() {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EUnsupportedSyntax(Box::new((
                        loc.dupe(),
                        intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                            TsLibSyntaxKind::ConstructorType,
                        ),
                    ))),
                );
                let Ok(v) = polymorphic_ast_mapper::type_(&mut typed_ast_utils::ErrorMapper, t);
                v
            } else {
                // Flow has no notion of abstract classes; flag `abstract new (...) => T`
                // the same way an interface body's `abstract m(): T` is flagged in
                // [add_interface_properties].
                if *abstract_ && !cx.tslib_syntax() {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::ETSSyntax(Box::new(
                            flow_typing_errors::error_message::ETSSyntaxData {
                                kind:
                                    flow_typing_errors::error_message::TSSyntaxKind::AbstractMethod,
                                loc: loc.dupe(),
                            },
                        )),
                    );
                }
                // Desugar `new (params) => T` to an inline interface whose [inst_construct_t]
                // carries the constructor signature.
                let reason = reason::mk_annot_reason(None, loc.dupe());
                let id = cx.make_aloc_id(loc);
                let super_ = func_class_sig_types::class::Super::Interface(
                    func_class_sig_types::class::InterfaceSuper {
                        inline: true,
                        extends: vec![],
                        extends_binding_kinds: vec![],
                        function_like: false,
                        this_tparam: None,
                        this_t: None,
                    },
                );
                let iface_sig: func_class_sig_types::class::Class<FuncTypeParamsConfig> =
                    class_sig::empty(
                        id,
                        None,
                        loc.dupe(),
                        reason.dupe(),
                        None,
                        env.tparams_map
                            .iter()
                            .map(|(k, v)| (k.dupe(), v.dupe()))
                            .collect(),
                        super_,
                    );
                let (fsig, func_ast) = mk_method_func_sig(
                    cx,
                    env,
                    MethodKind::MethodKind { is_static: false },
                    loc.dupe(),
                    func,
                )?;
                class_sig::check_signature_compatibility(cx, reason.dupe(), &iface_sig);
                // Store the raw method-bound funtype produced by
                // [Func_type_sig.methodtype]; consumers in [flow_js/] normalize on
                // read via [read_construct_t] (parallel to how [type_sig_merge.rs]
                // also stores raw and lets the reader normalize).
                let ctor_t = crate::func_sig::methodtype(
                    cx,
                    None,
                    flow_typing_type::type_::dummy_this(loc.dupe()),
                    &fsig,
                );
                let construct_id = cx.make_call_prop(ctor_t);
                let iface_t = class_sig::thistype(cx, &iface_sig);
                let mark_abstract = *abstract_ && cx.tslib_syntax();
                let with_construct = {
                    use std::rc::Rc;

                    use flow_typing_type::type_::DefT;
                    use flow_typing_type::type_::DefTInner;
                    use flow_typing_type::type_::InstType;
                    use flow_typing_type::type_::InstTypeInner;
                    use flow_typing_type::type_::InstanceT;
                    use flow_typing_type::type_::InstanceTInner;
                    use flow_typing_type::type_::ThisInstanceTData;
                    use flow_typing_type::type_::TypeInner as T;
                    match &*iface_t {
                        T::DefT(r, def_t) if let DefTInner::InstanceT(instance) = &**def_t => {
                            let inner: &InstanceTInner = instance;
                            let new_inst = InstType::new(InstTypeInner {
                                inst_construct_t: Some(construct_id),
                                inst_abstract: mark_abstract || inner.inst.inst_abstract,
                                ..(*inner.inst).clone()
                            });
                            let new_instance = InstanceT::new(InstanceTInner {
                                inst: new_inst,
                                ..inner.clone()
                            });
                            Type::new(T::DefT(
                                r.dupe(),
                                DefT::new(DefTInner::InstanceT(Rc::new(new_instance))),
                            ))
                        }
                        T::ThisInstanceT(data) => {
                            let inner: &InstanceTInner = &data.instance;
                            let new_inst = InstType::new(InstTypeInner {
                                inst_construct_t: Some(construct_id),
                                inst_abstract: mark_abstract || inner.inst.inst_abstract,
                                ..(*inner.inst).clone()
                            });
                            let new_instance = InstanceT::new(InstanceTInner {
                                inst: new_inst,
                                ..inner.clone()
                            });
                            Type::new(T::ThisInstanceT(Box::new(ThisInstanceTData {
                                reason: data.reason.dupe(),
                                instance: new_instance,
                                is_this: data.is_this,
                                subst_name: data.subst_name.dupe(),
                            })))
                        }
                        _ => {
                            // Unreachable; defensive. [Class_type_sig.thistype] on a
                            // freshly-built inline interface always returns one of the two
                            // shapes above. Fire an internal error and recover with an any
                            // rather than crashing the typechecker if the invariant is ever
                            // violated.
                            flow_js_utils::add_output_non_speculating(
                                cx,
                                ErrorMessage::EInternal(Box::new((
                                    loc.dupe(),
                                    flow_typing_errors::error_message::InternalError::UnexpectedInlineInterfaceType,
                                ))),
                            );
                            any_t::at(AnySource::AnyError(None), loc.dupe())
                        }
                    }
                };
                ast::types::Type::new(TypeInner::ConstructorType {
                    loc: (loc.dupe(), with_construct),
                    abstract_: *abstract_,
                    inner: func_ast.into(),
                })
            }
        }
        TypeInner::Exists { loc, comments } => {
            flow_js_utils::add_output_non_speculating(
                cx,
                ErrorMessage::EUnsupportedSyntax(Box::new((
                    loc.dupe(),
                    intermediate_error_types::UnsupportedSyntax::ExistsType,
                ))),
            );
            let rt = any_t::at(AnySource::AnnotatedAny, loc.dupe());
            ast::types::Type::new(TypeInner::Exists {
                loc: (loc.dupe(), rt),
                comments: comments.clone(),
            })
        }
    })
}

fn convert_list_inner<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    asts: &[ast::types::Type<ALoc, ALoc>],
) -> Result<
    (Vec<Type>, Vec<ast::types::Type<ALoc, (ALoc, Type)>>),
    flow_utils_concurrency::job_error::JobError,
> {
    let mut ts = Vec::new();
    let mut tasts = Vec::new();
    for ast in asts {
        let tast = convert_inner(cx, env, ast)?;
        let (_, t) = tast.loc();
        ts.push(t.dupe());
        tasts.push(tast);
    }
    Ok((ts, tasts))
}

pub fn convert_opt<'a>(
    cx: &mut Context<'a>,
    tparams_map: &FlowOrdMap<SubstName, Type>,
    ast_opt: Option<&ast::types::Type<ALoc, ALoc>>,
) -> Result<
    (Option<Type>, Option<ast::types::Type<ALoc, (ALoc, Type)>>),
    flow_utils_concurrency::job_error::JobError,
> {
    let mut env = ConvertEnv::new(None, None, None, tparams_map.dupe(), BindsSingleValue::No);
    let tast_opt = match ast_opt {
        Some(ast) => Some(convert_inner(cx, &mut env, ast)?),
        None => None,
    };
    let t_opt = tast_opt.as_ref().map(|tast| {
        let (_, t) = tast.loc();
        t.dupe()
    });
    Ok((t_opt, tast_opt))
}

pub fn convert_qualification<'a>(
    cx: &mut Context<'a>,
    id: &ast::types::generic::Identifier<ALoc, ALoc>,
) -> Result<
    (Type, ast::types::generic::Identifier<ALoc, (ALoc, Type)>),
    flow_utils_concurrency::job_error::JobError,
> {
    convert_qualification_with_lookup_mode(cx, None, id)
}

fn convert_qualification_with_lookup_mode<'a>(
    cx: &mut Context<'a>,
    lookup_mode: Option<type_env::LookupMode>,
    id: &ast::types::generic::Identifier<ALoc, ALoc>,
) -> Result<
    (Type, ast::types::generic::Identifier<ALoc, (ALoc, Type)>),
    flow_utils_concurrency::job_error::JobError,
> {
    use flow_parser::ast::types::generic::Identifier;
    let lookup_mode = lookup_mode.unwrap_or(type_env::LookupMode::ForType);
    Ok(match id {
        Identifier::Qualified(q) => {
            let (m, qualification_ast) =
                convert_qualification_with_lookup_mode(cx, Some(lookup_mode), &q.qualification)?;
            let id_loc = q.id.loc.dupe();
            let id_name = &q.id;
            let name = &id_name.name;
            let qname = FlowSmolStr::from(qualified_name(id));
            let id_reason = reason::mk_reason(
                reason::VirtualReasonDesc::RType(qname.dupe()),
                id_loc.dupe(),
            );
            let op_reason =
                reason::mk_reason(reason::VirtualReasonDesc::RType(qname), q.loc.dupe());
            let use_op =
                type_::UseOp::Op(Arc::new(type_::RootUseOp::GetProperty(op_reason.dupe())));
            let t = if lookup_mode == type_env::LookupMode::ForType {
                crate::type_annotation_cons_gen::qualify_type(
                    cx,
                    use_op,
                    id_reason,
                    op_reason,
                    Name::new(name.dupe()),
                    m,
                )
            } else {
                crate::type_annotation_cons_gen::get_prop(
                    cx,
                    use_op,
                    id_reason,
                    None,
                    Name::new(name.dupe()),
                    m,
                )
            };
            let result_ast = Identifier::Qualified(
                ast::types::generic::Qualified {
                    loc: q.loc.dupe(),
                    qualification: qualification_ast,
                    id: ast::Identifier::new(ast::IdentifierInner {
                        loc: (id_loc, t.dupe()),
                        name: name.dupe(),
                        comments: id_name.comments.dupe(),
                    }),
                }
                .into(),
            );
            (t, result_ast)
        }
        Identifier::Unqualified(ident) => {
            let t =
                type_env::lookup_type_identifier(lookup_mode, cx, &ident.name, ident.loc.dupe())?;
            (
                t.dupe(),
                Identifier::Unqualified(ast::Identifier::new(ast::IdentifierInner {
                    loc: (ident.loc.dupe(), t),
                    name: ident.name.dupe(),
                    comments: ident.comments.dupe(),
                })),
            )
        }
        Identifier::ImportTypeAnnot(import) => {
            if !cx.tslib_syntax() {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EUnsupportedSyntax(Box::new((
                        import.loc.dupe(),
                        intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                            TsLibSyntaxKind::ImportTypeAnnotation,
                        ),
                    ))),
                );
                let t = type_::any_t::at(type_::AnySource::AnyError(None), import.loc.dupe());
                (
                    t.dupe(),
                    Identifier::ImportTypeAnnot(
                        (ast::types::generic::ImportType {
                            loc: (import.loc.dupe(), t),
                            source: import.source.clone(),
                            comments: import.comments.dupe(),
                        })
                        .into(),
                    ),
                )
            } else {
                // Resolve the module
                let module_name = &import.source.1.value;
                let mref = Userland::from_smol_str(module_name.dupe());
                let source_module = match flow_js_utils::import_export_utils::get_module_type_or_any(
                    cx,
                    false,
                    Some(type_::ImportKind::ImportType),
                    import.source.0.dupe(),
                    mref.dupe(),
                ) {
                    Ok(v) => v,
                    Err(
                        flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c),
                    ) => return Err(flow_utils_concurrency::job_error::JobError::Canceled(c)),
                    Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => {
                        return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
                    }
                    Err(err) => panic!("Should not be under speculation: {:?}", err),
                };
                let reason = reason::mk_reason(
                    reason::VirtualReasonDesc::RModule(mref.dupe()),
                    import.loc.dupe(),
                );
                let namespace_symbol =
                    Symbol::mk_module_symbol(FlowSmolStr::new(mref.as_str()), import.loc.dupe());
                let t = match flow_js_utils::import_export_utils::get_module_namespace_type(
                    cx,
                    reason,
                    namespace_symbol,
                    &source_module,
                ) {
                    Ok(v) => v,
                    Err(
                        flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c),
                    ) => return Err(flow_utils_concurrency::job_error::JobError::Canceled(c)),
                    Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => {
                        return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
                    }
                    Err(err) => panic!("Should not be under speculation: {:?}", err),
                };
                (
                    t.dupe(),
                    Identifier::ImportTypeAnnot(
                        ast::types::generic::ImportType {
                            loc: (import.loc.dupe(), t),
                            source: import.source.clone(),
                            comments: import.comments.dupe(),
                        }
                        .into(),
                    ),
                )
            }
        }
    })
}

fn convert_typeof<'a>(
    cx: &mut Context<'a>,
    target: &ast::types::typeof_::Target<ALoc, ALoc>,
) -> Result<
    (Type, ast::types::typeof_::Target<ALoc, (ALoc, Type)>),
    flow_utils_concurrency::job_error::JobError,
> {
    use flow_parser::ast::types::typeof_::Target;
    Ok(match target {
        Target::Qualified(q) => {
            let (m, qualification_ast) = convert_typeof(cx, &q.qualification)?;
            let id_loc = q.id.loc.dupe();
            let id_name = &q.id;
            let name = &id_name.name;
            let tname = typeof_name(target);
            let id_reason = reason::mk_reason(None, id_loc.dupe());
            let use_op =
                type_::UseOp::Op(Arc::new(type_::RootUseOp::GetProperty(reason::mk_reason(
                    reason::VirtualReasonDesc::RType(FlowSmolStr::new(&tname)),
                    q.loc.dupe(),
                ))));
            let t = crate::type_annotation_cons_gen::get_prop(
                cx,
                use_op,
                id_reason,
                None,
                Name::new(name.dupe()),
                m,
            );
            let final_ast = Target::Qualified(
                ast::types::typeof_::Qualified {
                    loc: (q.loc.dupe(), t.dupe()),
                    qualification: qualification_ast,
                    id: ast::Identifier::new(ast::IdentifierInner {
                        loc: (id_loc, t.dupe()),
                        name: name.dupe(),
                        comments: id_name.comments.dupe(),
                    }),
                }
                .into(),
            );
            (t, final_ast)
        }
        Target::Unqualified(ident) => {
            if ident.name == "this" && !cx.tslib_syntax() {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EUnsupportedSyntax(Box::new((
                        ident.loc.dupe(),
                        intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                            TsLibSyntaxKind::TypeofThis,
                        ),
                    ))),
                );
                let t = type_::any_t::at(type_::AnySource::AnyError(None), ident.loc.dupe());
                let Ok(mapped) = polymorphic_ast_mapper::typeof_expression(
                    &mut typed_ast_utils::ErrorMapper,
                    target,
                );
                (t, mapped)
            } else {
                let t = if flow_typing_flow_js::type_inference_hooks_js::dispatch_id_hook(
                    &ident.name,
                    ident.loc.dupe(),
                ) {
                    type_::unsoundness::at(type_::UnsoundnessKind::InferenceHooks, ident.loc.dupe())
                } else {
                    type_env::lookup_type_identifier(
                        type_env::LookupMode::ForTypeof,
                        cx,
                        &ident.name,
                        ident.loc.dupe(),
                    )?
                };
                (
                    t.dupe(),
                    Target::Unqualified(ast::Identifier::new(ast::IdentifierInner {
                        loc: (ident.loc.dupe(), t),
                        name: ident.name.dupe(),
                        comments: ident.comments.dupe(),
                    })),
                )
            }
        }
        Target::Import(import) => {
            if !cx.tslib_syntax() {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EUnsupportedSyntax(Box::new((
                        import.loc.dupe(),
                        intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                            TsLibSyntaxKind::TypeofImport,
                        ),
                    ))),
                );
                let t = type_::any_t::at(type_::AnySource::AnyError(None), import.loc.dupe());
                let Ok(mapped) = polymorphic_ast_mapper::typeof_expression(
                    &mut typed_ast_utils::ErrorMapper,
                    target,
                );
                (t, mapped)
            } else {
                // Resolve the module
                let module_name = &import.source.1.value;
                let mref = Userland::from_smol_str(module_name.dupe());
                let source_module = match flow_js_utils::import_export_utils::get_module_type_or_any(
                    cx,
                    false,
                    Some(type_::ImportKind::ImportValue),
                    import.source.0.dupe(),
                    mref.dupe(),
                ) {
                    Ok(v) => v,
                    Err(
                        flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c),
                    ) => return Err(flow_utils_concurrency::job_error::JobError::Canceled(c)),
                    Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => {
                        return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
                    }
                    Err(err) => panic!("Should not be under speculation: {:?}", err),
                };
                let reason = reason::mk_reason(
                    reason::VirtualReasonDesc::RModule(mref.dupe()),
                    import.loc.dupe(),
                );
                let namespace_symbol =
                    Symbol::mk_module_symbol(FlowSmolStr::new(mref.as_str()), import.loc.dupe());
                let t = match flow_js_utils::import_export_utils::get_module_namespace_type(
                    cx,
                    reason,
                    namespace_symbol,
                    &source_module,
                ) {
                    Ok(v) => v,
                    Err(
                        flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c),
                    ) => return Err(flow_utils_concurrency::job_error::JobError::Canceled(c)),
                    Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => {
                        return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
                    }
                    Err(err) => panic!("Should not be under speculation: {:?}", err),
                };
                (
                    t.dupe(),
                    Target::Import(
                        ast::types::generic::ImportType {
                            loc: (import.loc.dupe(), t),
                            source: import.source.clone(),
                            comments: import.comments.dupe(),
                        }
                        .into(),
                    ),
                )
            }
        }
    })
}

fn convert_render_type_inner<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    loc: ALoc,
    renders: &ast::types::Renders<ALoc, ALoc>,
) -> Result<
    (Type, ast::types::Renders<ALoc, (ALoc, Type)>),
    flow_utils_concurrency::job_error::JobError,
> {
    let old_in_renders_arg = env.in_renders_arg;
    env.in_renders_arg = true;
    let t_ast = convert_inner(cx, env, &renders.argument)?;
    env.in_renders_arg = old_in_renders_arg;
    let (_, t) = t_ast.loc();
    let t = t.dupe();
    let reason = reason::mk_reason(type_util::reason_of_t(&t).desc(true).cloned(), loc.dupe());
    Ok(
        match type_util::mk_possibly_generic_render_type(
            renders.variant.clone(),
            reason.dupe(),
            t.dupe(),
        ) {
            Some(t_prime) => {
                let node = flow_js::get_builtin_react_type_non_speculating(
                    cx,
                    &reason,
                    None,
                    intermediate_error_types::ExpectedModulePurpose::ReactModuleForReactNodeType,
                )?;
                let use_op =
                    type_::UseOp::Op(Arc::new(type_::RootUseOp::RenderTypeInstantiation {
                        render_type: reason.dupe(),
                    }));
                cx.add_post_inference_subtyping_check(t, use_op, node);
                (
                    t_prime,
                    ast::types::Renders {
                        operator_loc: renders.operator_loc.dupe(),
                        comments: renders.comments.dupe(),
                        argument: t_ast,
                        variant: renders.variant.clone(),
                    },
                )
            }
            None => {
                let renders_variant = match renders.variant {
                    ast::types::RendersVariant::Normal => type_::RendersVariant::RendersNormal,
                    ast::types::RendersVariant::Maybe => type_::RendersVariant::RendersMaybe,
                    ast::types::RendersVariant::Star => type_::RendersVariant::RendersStar,
                };
                fn concretize_fn<'cx>(
                    cx_arg: &mut Context<'cx>,
                    t_arg: &Type,
                ) -> Result<Vec<Type>, flow_utils_concurrency::job_error::JobError>
                {
                    let r = type_util::reason_of_t(t_arg).dupe();
                    match FlowJs::possible_concrete_types_for_inspection(cx_arg, &r, t_arg) {
                        Ok(v) => Ok(v),
                        Err(
                            flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(
                                c,
                            ),
                        ) => Err(flow_utils_concurrency::job_error::JobError::Canceled(c)),
                        Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(
                            t,
                        )) => Err(flow_utils_concurrency::job_error::JobError::TimedOut(t)),
                        Err(err) => panic!("Should not be under speculation: {:?}", err),
                    }
                }
                fn is_iterable_fn<'cx>(
                    cx_arg: &mut Context<'cx>,
                    t_arg: &Type,
                ) -> Result<bool, flow_utils_concurrency::job_error::JobError> {
                    let r = type_util::reason_of_t(t_arg).dupe();
                    let error_r = type_::any_t::error(r.dupe());
                    let iterable_t = FlowJs::get_builtin_typeapp(
                        cx_arg,
                        &r,
                        None,
                        "Iterable",
                        vec![error_r.dupe(), error_r.dupe(), error_r],
                    );
                    FlowJs::speculative_subtyping_succeeds(cx_arg, t_arg, &iterable_t)
                }
                let concretize = concretize_fn;
                let is_iterable_for_better_error = is_iterable_fn;
                let renders_t = flow_js_utils::render_types::mk_non_generic_render_type(
                    cx,
                    reason,
                    renders_variant,
                    true,
                    concretize,
                    is_iterable_for_better_error,
                    t,
                );
                (
                    renders_t,
                    ast::types::Renders {
                        operator_loc: renders.operator_loc.dupe(),
                        comments: renders.comments.dupe(),
                        argument: t_ast,
                        variant: renders.variant.clone(),
                    },
                )
            }
        },
    )
}

fn convert_object<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    loc: ALoc,
    exact: bool,
    properties: &[ast::types::object::Property<ALoc, ALoc>],
) -> Result<
    (Type, Vec<ast::types::object::Property<ALoc, (ALoc, Type)>>),
    flow_utils_concurrency::job_error::JobError,
> {
    use flow_parser::ast::types::object::Property;
    use flow_typing_type::type_::*;

    let obj_proto_t = obj_proto::make(reason::locationless_reason(None));
    let fun_proto_t = Type::new(type_::TypeInner::FunProtoT(reason::locationless_reason(
        None,
    )));

    #[derive(Clone)]
    enum AccElement {
        Spread(Type),
        Slice {
            dict: Option<DictType>,
            pmap: type_::properties::PropertiesMap,
        },
    }

    #[derive(Clone)]
    struct Acc {
        dict: Option<DictType>,
        pmap: type_::properties::PropertiesMap,
        tail: Vec<AccElement>,
        proto: Option<Type>,
        calls: Vec<Type>,
    }

    impl Acc {
        fn empty() -> Self {
            Self {
                dict: None,
                pmap: type_::properties::PropertiesMap::new(),
                tail: Vec::new(),
                proto: None,
                calls: Vec::new(),
            }
        }

        fn head_slice(&self) -> Option<AccElement> {
            if self.dict.is_none() && self.pmap.is_empty() {
                None
            } else {
                Some(AccElement::Slice {
                    dict: self.dict.clone(),
                    pmap: self.pmap.dupe(),
                })
            }
        }

        fn add_call(&mut self, c: Type) -> Result<(), intermediate_error_types::UnsupportedSyntax> {
            if self.proto.is_some() {
                return Err(intermediate_error_types::UnsupportedSyntax::ExplicitCallAfterProto);
            }
            self.calls.push(c);
            Ok(())
        }

        fn add_dict(
            &mut self,
            d: DictType,
        ) -> Result<(), intermediate_error_types::UnsupportedSyntax> {
            if self.dict.is_some() {
                return Err(intermediate_error_types::UnsupportedSyntax::MultipleIndexers);
            }
            self.dict = Some(d);
            Ok(())
        }

        fn add_prop(&mut self, f: impl FnOnce(&mut type_::properties::PropertiesMap)) {
            f(&mut self.pmap);
        }

        fn add_proto(
            &mut self,
            p: Type,
        ) -> Result<(), intermediate_error_types::UnsupportedSyntax> {
            if self.proto.is_some() {
                return Err(intermediate_error_types::UnsupportedSyntax::MultipleProtos);
            }
            if !self.calls.is_empty() {
                return Err(intermediate_error_types::UnsupportedSyntax::ExplicitProtoAfterCall);
            }
            self.proto = Some(p);
            Ok(())
        }

        fn add_spread(&mut self, t: Type) {
            let mut tail = std::mem::take(&mut self.tail);
            if let Some(slice) = self.head_slice() {
                tail.push(slice);
            }
            tail.push(AccElement::Spread(t));
            self.dict = None;
            self.pmap = type_::properties::PropertiesMap::new();
            self.tail = tail;
        }

        fn elements_rev(self) -> (Vec<AccElement>, AccElement) {
            let head = self.head_slice();
            let mut tail = self.tail;
            tail.reverse();
            match head {
                Some(slice) => (tail, slice),
                None => match tail.len() {
                    0 => (
                        Vec::new(),
                        AccElement::Slice {
                            dict: None,
                            pmap: type_::properties::PropertiesMap::new(),
                        },
                    ),
                    _ => {
                        let first = tail.remove(0);
                        (tail, first)
                    }
                },
            }
        }

        fn get_proto(&self, obj_proto_t: &Type, fun_proto_t: &Type) -> Type {
            if let Some(ref t) = self.proto {
                t.dupe()
            } else if !self.calls.is_empty() {
                fun_proto_t.dupe()
            } else {
                obj_proto_t.dupe()
            }
        }
    }

    fn mk_object<'a>(
        cx: &mut Context<'a>,
        loc: ALoc,
        src_loc: bool,
        exact: bool,
        call: Option<Type>,
        dict: Option<DictType>,
        pmap: type_::properties::PropertiesMap,
        proto: Type,
    ) -> (Reason, ObjType) {
        let strictness_kind = cx.type_strictness_kind();
        let pmap_props = pmap;
        let pmap_id = if src_loc && type_env::in_toplevel_scope(cx) {
            cx.make_source_property_map(pmap_props, false, &loc)
        } else {
            cx.generate_property_map(pmap_props)
        };
        let call_id = call.map(|c| cx.make_call_prop(c));
        let obj_kind = match dict {
            Some(d) => ObjKind::Indexed(d),
            None => {
                if exact {
                    ObjKind::Exact
                } else {
                    ObjKind::Inexact
                }
            }
        };
        let flags = Flags {
            obj_kind,
            react_dro: None,
        };
        let reason = reason::mk_annot_reason(reason::VirtualReasonDesc::RObjectType, loc);
        let obj_t = type_::mk_objecttype_with_strictness(
            Some(flags),
            None,
            call_id,
            pmap_id,
            proto,
            strictness_kind,
            false,
        );
        (reason, obj_t)
    }

    fn mk_object_annot<'a>(
        cx: &mut Context<'a>,
        loc: ALoc,
        exact: bool,
        call: Option<Type>,
        dict: &Option<DictType>,
        pmap: &type_::properties::PropertiesMap,
        proto: &Type,
    ) -> Type {
        let exact = exact && dict.is_none();
        let (reason_obj, obj_t) = mk_object(
            cx,
            loc.dupe(),
            true,
            exact,
            call,
            dict.clone(),
            pmap.clone(),
            proto.dupe(),
        );
        if exact {
            let reason_op = reason::mk_annot_reason(reason::VirtualReasonDesc::RObjectType, loc);
            type_util::make_exact_object(reason_obj, Rc::new(obj_t), &reason_op)
        } else {
            Type::new(type_::TypeInner::DefT(
                reason_obj,
                DefT::new(DefTInner::ObjT(Rc::new(obj_t))),
            ))
        }
    }

    fn named_property<'a>(
        cx: &mut Context<'a>,
        env: &mut ConvertEnv,
        acc: &mut Acc,
        prop: &ast::types::object::NormalProperty<ALoc, ALoc>,
    ) -> Result<
        ast::types::object::NormalProperty<ALoc, (ALoc, Type)>,
        flow_utils_concurrency::job_error::JobError,
    > {
        use flow_parser::ast::expression::object::Key;
        use flow_typing_errors::error_message::ETSSyntaxData;
        use flow_typing_errors::error_message::ErrorMessage;
        use flow_typing_type::type_::*;
        Ok(match &prop.value {
            ast::types::object::PropertyValue::Init(Some(value)) => {
                let key = &prop.key;
                let optional = prop.optional;
                let variance = &prop.variance;
                let method = prop.method;
                let abstract_ = prop.abstract_;
                if abstract_ && !cx.tslib_syntax() {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::ETSSyntax(Box::new(ETSSyntaxData {
                            kind: flow_typing_errors::error_message::TSSyntaxKind::AbstractMethod,
                            loc: prop.loc.dupe(),
                        })),
                    );
                }
                if optional && method && !cx.tslib_syntax() {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EUnsupportedSyntax(Box::new((
                            prop.loc.dupe(),
                            intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                                TsLibSyntaxKind::OptionalShorthandMethod,
                            ),
                        ))),
                    );
                }
                let prop_of_name = |cx: &mut Context<'a>,
                                    env: &mut ConvertEnv,
                                    acc: &mut Acc,
                                    key_loc: ALoc,
                                    name: FlowSmolStr|
                 -> Result<
                    ast::types::object::NormalProperty<ALoc, (ALoc, Type)>,
                    flow_utils_concurrency::job_error::JobError,
                > {
                    check_unique_symbol_read_only(
                        cx,
                        value,
                        typed_ast_utils::polarity(variance.as_ref()),
                    );
                    let value_ast = convert_single_binding_value(cx, env, value, Some(&name))?;
                    let t = value_ast.loc().1.dupe();
                    let make_prop_ast = |t: Type| {
                        let key_ast = match key {
                            Key::StringLiteral((_, lit)) => {
                                Key::StringLiteral(((key_loc.dupe(), t.dupe()), lit.clone()))
                            }
                            Key::NumberLiteral((_, lit)) => {
                                Key::NumberLiteral(((key_loc.dupe(), t.dupe()), lit.clone()))
                            }
                            Key::Identifier(ident) => {
                                Key::Identifier(ast::Identifier::new(ast::IdentifierInner {
                                    loc: (key_loc.dupe(), t.dupe()),
                                    name: name.dupe(),
                                    comments: ident.comments.dupe(),
                                }))
                            }
                            _ => panic!("branch invariant"),
                        };
                        ast::types::object::NormalProperty {
                            loc: prop.loc.dupe(),
                            key: key_ast,
                            value: ast::types::object::PropertyValue::Init(Some(value_ast.dupe())),
                            optional,
                            static_: prop.static_,
                            proto: prop.proto,
                            method,
                            abstract_: prop.abstract_,
                            override_: prop.override_,
                            variance: prop.variance.clone(),
                            ts_accessibility: prop.ts_accessibility.as_ref().map(|tsa| {
                                ast::class::ts_accessibility::TSAccessibility {
                                    loc: tsa.loc.dupe(),
                                    kind: tsa.kind,
                                    comments: tsa.comments.dupe(),
                                }
                            }),
                            init: None,
                            comments: prop.comments.dupe(),
                        }
                    };
                    if name.as_str() == "__proto__" && !(method || optional) && variance.is_none() {
                        let reason = reason::mk_reason(None, value.loc().dupe());
                        let proto =
                            crate::type_annotation_cons_gen::obj_test_proto(cx, reason.dupe(), t);
                        let typeof_annot = type_util::typeof_annotation(reason, proto.dupe(), None);
                        if let Err(err) = acc.add_proto(typeof_annot) {
                            flow_js_utils::add_output_non_speculating(
                                cx,
                                ErrorMessage::EUnsupportedSyntax(Box::new((key_loc.dupe(), err))),
                            );
                        }
                        Ok(make_prop_ast(proto))
                    } else {
                        let t = if optional {
                            type_util::optional(t, None, false)
                        } else {
                            t
                        };
                        let prop_name = Name::new(name.dupe());
                        if method {
                            let key_loc_clone = key_loc.dupe();
                            let t_clone = t.dupe();
                            acc.add_prop(move |pmap| {
                                pmap.insert(
                                    prop_name,
                                    type_::Property::new(type_::PropertyInner::Method {
                                        key_loc: Some(key_loc_clone),
                                        type_: t_clone,
                                    }),
                                );
                            });
                        } else {
                            let pol = polarity(
                                cx,
                                flow_typing_errors::intermediate_error_types::VarianceSigilParent::Property,
                                variance.as_ref(),
                            );
                            let key_loc_clone = key_loc.dupe();
                            let t_clone = t.dupe();
                            acc.add_prop(move |pmap| {
                                pmap.insert(
                                    prop_name,
                                    type_::Property::new(type_::PropertyInner::Field(Box::new(
                                        FieldData {
                                            preferred_def_locs: None,
                                            key_loc: Some(key_loc_clone),
                                            type_: t_clone,
                                            polarity: pol,
                                        },
                                    ))),
                                );
                            });
                        }
                        Ok(make_prop_ast(t))
                    }
                };
                match key {
                    Key::StringLiteral((key_loc, lit)) => {
                        prop_of_name(cx, env, acc, key_loc.dupe(), lit.value.dupe())?
                    }
                    Key::Identifier(ident) => {
                        prop_of_name(cx, env, acc, ident.loc.dupe(), ident.name.dupe())?
                    }
                    Key::NumberLiteral((key_loc, lit)) => match variance {
                        Some(ast::Variance {
                            kind: ast::VarianceKind::Plus,
                            ..
                        })
                        | Some(ast::Variance {
                            kind: ast::VarianceKind::Minus,
                            ..
                        }) => {
                            flow_js_utils::add_output_non_speculating(
                                cx,
                                ErrorMessage::EAmbiguousNumericKeyWithVariance(key_loc.dupe()),
                            );
                            {
                                let Ok(v) = polymorphic_ast_mapper::object_property_type(
                                    &mut typed_ast_utils::ErrorMapper,
                                    prop,
                                );
                                v
                            }
                        }
                        _ => {
                            if flow_common::js_number::is_float_safe_integer(lit.value) {
                                let name_str =
                                    flow_common::js_number::ecma_string_of_float(lit.value);
                                prop_of_name(
                                    cx,
                                    env,
                                    acc,
                                    key_loc.dupe(),
                                    FlowSmolStr::new(&name_str),
                                )?
                            } else {
                                flow_js_utils::add_output_non_speculating(
                                    cx,
                                    ErrorMessage::EUnsupportedKeyInObject {
                                        loc: key_loc.dupe(),
                                        obj_kind: intermediate_error_types::ObjKind::Type,
                                        key_error_kind: InvalidObjKey::kind_of_num_value(lit.value),
                                    },
                                );
                                {
                                    let Ok(v) = polymorphic_ast_mapper::object_property_type(
                                        &mut typed_ast_utils::ErrorMapper,
                                        prop,
                                    );
                                    v
                                }
                            }
                        }
                    },
                    Key::BigIntLiteral((key_loc, _)) => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EUnsupportedKeyInObject {
                                loc: key_loc.dupe(),
                                obj_kind: intermediate_error_types::ObjKind::Type,
                                key_error_kind: InvalidObjKey::Other,
                            },
                        );
                        {
                            let Ok(v) = polymorphic_ast_mapper::object_property_type(
                                &mut typed_ast_utils::ErrorMapper,
                                prop,
                            );
                            v
                        }
                    }
                    Key::PrivateName(pn) => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EUnsupportedKeyInObject {
                                loc: pn.loc.dupe(),
                                obj_kind: intermediate_error_types::ObjKind::Type,
                                key_error_kind: InvalidObjKey::Other,
                            },
                        );
                        {
                            let Ok(v) = polymorphic_ast_mapper::object_property_type(
                                &mut typed_ast_utils::ErrorMapper,
                                prop,
                            );
                            v
                        }
                    }
                    Key::Computed(comp) => {
                        let (typed_expr, resolved_name) =
                            resolve_computed_key_name(cx, &comp.expression)?;
                        match resolved_name {
                            Some(name) => {
                                // A computed key names one property just as a
                                // written one does, so the same rule applies. A
                                // symbol key has no name to render the value
                                // after, hence `as_smol_str_opt`.
                                check_unique_symbol_read_only(
                                    cx,
                                    value,
                                    typed_ast_utils::polarity(variance.as_ref()),
                                );
                                let value_ast = convert_single_binding_value(
                                    cx,
                                    env,
                                    value,
                                    name.as_smol_str_opt(),
                                )?;
                                let t = value_ast.loc().1.dupe();
                                let t = if optional {
                                    type_util::optional(t, None, false)
                                } else {
                                    t
                                };
                                let id_loc = comp.expression.loc().dupe();
                                if method {
                                    let id_loc_clone = id_loc.dupe();
                                    let t_clone = t.dupe();
                                    acc.add_prop(move |pmap| {
                                        pmap.insert(
                                            name,
                                            type_::Property::new(type_::PropertyInner::Method {
                                                key_loc: Some(id_loc_clone),
                                                type_: t_clone,
                                            }),
                                        );
                                    });
                                } else {
                                    let pol = polarity(
                                        cx,
                                        flow_typing_errors::intermediate_error_types::VarianceSigilParent::Property,
                                        variance.as_ref(),
                                    );
                                    let id_loc_clone = id_loc.dupe();
                                    let t_clone = t.dupe();
                                    acc.add_prop(move |pmap| {
                                        pmap.insert(
                                            name,
                                            type_::Property::new(type_::PropertyInner::Field(
                                                Box::new(FieldData {
                                                    preferred_def_locs: None,
                                                    key_loc: Some(id_loc_clone),
                                                    type_: t_clone,
                                                    polarity: pol,
                                                }),
                                            )),
                                        );
                                    });
                                }
                                ast::types::object::NormalProperty {
                                    loc: prop.loc.dupe(),
                                    key: Key::Computed(ast::ComputedKey {
                                        loc: comp.loc.dupe(),
                                        expression: typed_expr,
                                        comments: comp.comments.dupe(),
                                    }),
                                    value: ast::types::object::PropertyValue::Init(Some(value_ast)),
                                    optional,
                                    static_: false,
                                    proto: false,
                                    method,
                                    abstract_,
                                    override_: prop.override_,
                                    variance: prop.variance.clone(),
                                    ts_accessibility: None,
                                    init: None,
                                    comments: None,
                                }
                            }
                            None => {
                                flow_js_utils::add_output_non_speculating(
                                    cx,
                                    ErrorMessage::EUnsupportedKeyInObject {
                                        loc: comp.loc.dupe(),
                                        obj_kind: intermediate_error_types::ObjKind::Type,
                                        key_error_kind: InvalidObjKey::Other,
                                    },
                                );
                                let Ok(v) = polymorphic_ast_mapper::object_property_type(
                                    &mut typed_ast_utils::ErrorMapper,
                                    prop,
                                );
                                v
                            }
                        }
                    }
                }
            }
            // unsafe getter property
            ast::types::object::PropertyValue::Get(getter_loc, getter_fn) => match &prop.key {
                Key::Identifier(id_name) => {
                    let id_loc = &id_name.loc;
                    let name = &id_name.name;
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EUnsafeGettersSetters(getter_loc.dupe()),
                    );
                    let (function_type, getter_ast) = mk_function_type_annotation_inner(
                        cx,
                        env,
                        &(getter_loc.dupe(), getter_fn.clone()),
                    )?;
                    let return_t = extract_getter_type(&function_type);
                    let prop_name = Name::new(name.dupe());
                    let id_loc_clone = id_loc.dupe();
                    let return_t_clone = return_t.dupe();
                    acc.add_prop(move |pmap| {
                        let new_prop = match pmap.get(&prop_name).map(|p| p.deref()) {
                            Some(type_::PropertyInner::Set {
                                key_loc: set_key_loc,
                                type_: set_type,
                            }) => type_::Property::new(type_::PropertyInner::GetSet(Box::new(
                                GetSetData {
                                    get_key_loc: Some(id_loc_clone),
                                    get_type: return_t_clone,
                                    set_key_loc: set_key_loc.dupe(),
                                    set_type: set_type.dupe(),
                                },
                            ))),
                            _ => type_::Property::new(type_::PropertyInner::Get {
                                key_loc: Some(id_loc_clone),
                                type_: return_t_clone,
                            }),
                        };
                        pmap.insert(prop_name, new_prop);
                    });
                    ast::types::object::NormalProperty {
                        loc: prop.loc.dupe(),
                        key: Key::Identifier(ast::Identifier::new(ast::IdentifierInner {
                            loc: (id_loc.dupe(), return_t),
                            name: name.dupe(),
                            comments: id_name.comments.dupe(),
                        })),
                        value: ast::types::object::PropertyValue::Get(getter_ast.0, getter_ast.1),
                        optional: prop.optional,
                        static_: prop.static_,
                        proto: prop.proto,
                        method: prop.method,
                        abstract_: prop.abstract_,
                        override_: prop.override_,
                        variance: prop.variance.clone(),
                        ts_accessibility: prop.ts_accessibility.as_ref().map(|tsa| {
                            ast::class::ts_accessibility::TSAccessibility {
                                loc: tsa.loc.dupe(),
                                kind: tsa.kind,
                                comments: tsa.comments.dupe(),
                            }
                        }),
                        init: None,
                        comments: prop.comments.dupe(),
                    }
                }
                _ => {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EUnsupportedSyntax(Box::new((
                            prop.loc.dupe(),
                            intermediate_error_types::UnsupportedSyntax::ObjectPropertyGetSet,
                        ))),
                    );
                    {
                        let Ok(v) = polymorphic_ast_mapper::object_property_type(
                            &mut typed_ast_utils::ErrorMapper,
                            prop,
                        );
                        v
                    }
                }
            },
            // unsafe setter property
            ast::types::object::PropertyValue::Set(setter_loc, setter_fn) => match &prop.key {
                Key::Identifier(id_name) => {
                    let id_loc = &id_name.loc;
                    let name = &id_name.name;
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EUnsafeGettersSetters(setter_loc.dupe()),
                    );
                    let (function_type, setter_ast) = mk_function_type_annotation_inner(
                        cx,
                        env,
                        &(setter_loc.dupe(), setter_fn.clone()),
                    )?;
                    let param_t = extract_setter_type(&function_type);
                    let prop_name = Name::new(name.dupe());
                    let id_loc_clone = id_loc.dupe();
                    let param_t_clone = param_t.dupe();
                    acc.add_prop(move |pmap| {
                        let new_prop = match pmap.get(&prop_name).map(|p| p.deref()) {
                            Some(type_::PropertyInner::Get {
                                key_loc: get_key_loc,
                                type_: get_type,
                            }) => type_::Property::new(type_::PropertyInner::GetSet(Box::new(
                                GetSetData {
                                    get_key_loc: get_key_loc.dupe(),
                                    get_type: get_type.dupe(),
                                    set_key_loc: Some(id_loc_clone),
                                    set_type: param_t_clone,
                                },
                            ))),
                            _ => type_::Property::new(type_::PropertyInner::Set {
                                key_loc: Some(id_loc_clone),
                                type_: param_t_clone,
                            }),
                        };
                        pmap.insert(prop_name, new_prop);
                    });
                    ast::types::object::NormalProperty {
                        loc: prop.loc.dupe(),
                        key: Key::Identifier(ast::Identifier::new(ast::IdentifierInner {
                            loc: (id_loc.dupe(), param_t),
                            name: name.dupe(),
                            comments: id_name.comments.dupe(),
                        })),
                        value: ast::types::object::PropertyValue::Set(setter_ast.0, setter_ast.1),
                        optional: prop.optional,
                        static_: prop.static_,
                        proto: prop.proto,
                        method: prop.method,
                        abstract_: prop.abstract_,
                        override_: prop.override_,
                        variance: prop.variance.clone(),
                        ts_accessibility: prop.ts_accessibility.as_ref().map(|tsa| {
                            ast::class::ts_accessibility::TSAccessibility {
                                loc: tsa.loc.dupe(),
                                kind: tsa.kind,
                                comments: tsa.comments.dupe(),
                            }
                        }),
                        init: None,
                        comments: prop.comments.dupe(),
                    }
                }
                _ => {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EUnsupportedSyntax(Box::new((
                            prop.loc.dupe(),
                            intermediate_error_types::UnsupportedSyntax::ObjectPropertyGetSet,
                        ))),
                    );
                    {
                        let Ok(v) = polymorphic_ast_mapper::object_property_type(
                            &mut typed_ast_utils::ErrorMapper,
                            prop,
                        );
                        v
                    }
                }
            },
            ast::types::object::PropertyValue::Init(None) => {
                let Ok(v) = polymorphic_ast_mapper::object_property_type(
                    &mut typed_ast_utils::ErrorMapper,
                    prop,
                );
                v
            }
        })
    }

    // ========================================
    // Process properties
    // ========================================
    let mut acc = Acc::empty();
    let mut prop_asts: Vec<Property<ALoc, (ALoc, Type)>> = Vec::new();
    let accessor_pair_last_locs = accessor_pair_last_locs(properties);

    for prop in properties {
        match prop {
            Property::CallProperty(cp) => {
                let cp_loc = &cp.value.0;
                let (t, (_, fn_ast)) = mk_function_type_annotation_inner(
                    cx,
                    env,
                    &(cp_loc.dupe(), cp.value.1.clone()),
                )?;
                let call_ast = ast::types::object::CallProperty {
                    loc: cp.loc.dupe(),
                    value: (cp.value.0.dupe(), fn_ast),
                    static_: cp.static_,
                    comments: cp.comments.dupe(),
                };
                if let Err(err) = acc.add_call(t) {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EUnsupportedSyntax(Box::new((cp_loc.dupe(), err))),
                    );
                }
                prop_asts.push(Property::CallProperty(call_ast));
            }
            Property::Indexer(indexer) => {
                // `{[K]: V}` is ambiguous between a computed key and an index
                // signature, and the way the key is written decides (see
                // `object_type_key`). The labeled form `[label: K]` is an index
                // signature outright. Convert the key before the optional gate,
                // so an optional computed key `['a']?` becomes an optional named
                // property instead of being rejected as an optional indexer,
                // which is TSLib-only syntax.
                let key_form = if indexer.id.is_some() {
                    ObjectTypeKeyForm::IndexSignature
                } else {
                    object_type_key::object_type_key_form(&indexer.key)
                };
                let (key_ast, key_read) = match key_form {
                    ObjectTypeKeyForm::Literal(name) => (
                        convert_inner(cx, env, &indexer.key)?,
                        KeyRead::Named(Name::new(name)),
                    ),
                    ObjectTypeKeyForm::Name { loc, generic, head }
                        if type_env::heads_computed_key(cx, &head.loc) =>
                    {
                        let (key_ast, name) = convert_value_object_type_key(cx, loc, generic)?;
                        // The value names no one property, so there is no
                        // member to add. An index signature is not a sound
                        // reading either: the author asked for one key.
                        let key_read = match name {
                            Some(name) => KeyRead::Named(name),
                            None if key_is_type_name(cx, &key_ast) => {
                                KeyRead::IllegalName(InvalidObjKey::ComputedTypeName)
                            }
                            None => KeyRead::IllegalName(InvalidObjKey::ComputedNotLiteral),
                        };
                        (key_ast, key_read)
                    }
                    ObjectTypeKeyForm::Name { .. } | ObjectTypeKeyForm::IndexSignature => (
                        convert_inner(cx, env, &indexer.key)?,
                        KeyRead::IndexSignature,
                    ),
                };
                // A rejected key adds no member at all. Past here the key is
                // either a name or an index signature, so the two are the only
                // cases the rest has to answer for.
                let named = match key_read {
                    KeyRead::IllegalName(key_error_kind) => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EUnsupportedKeyInObject {
                                loc: indexer.key.loc().dupe(),
                                obj_kind: intermediate_error_types::ObjKind::Type,
                                key_error_kind,
                            },
                        );
                        let Ok(error_prop) = polymorphic_ast_mapper::object_type_property(
                            &mut typed_ast_utils::ErrorMapper,
                            prop,
                        );
                        prop_asts.push(error_prop);
                        continue;
                    }
                    KeyRead::IndexSignature if indexer.optional && !cx.ts_syntax() => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EUnsupportedSyntax(Box::new((
                                indexer.loc.dupe(),
                                intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                                    intermediate_error_types::TsLibSyntaxKind::OptionalIndexer,
                                ),
                            ))),
                        );
                        let Ok(error_prop) = polymorphic_ast_mapper::object_type_property(
                            &mut typed_ast_utils::ErrorMapper,
                            prop,
                        );
                        prop_asts.push(error_prop);
                        continue;
                    }
                    // A getter and setter pair is one member, ordered by its
                    // last half, so a pair that ends after this key wins over
                    // it and the key adds nothing.
                    KeyRead::Named(name)
                        if accessor_pair_last_locs
                            .get(&name)
                            .is_some_and(|last| last.quick_compare(indexer.key.loc()).is_gt()) =>
                    {
                        let Ok(error_prop) = polymorphic_ast_mapper::object_type_property(
                            &mut typed_ast_utils::ErrorMapper,
                            prop,
                        );
                        prop_asts.push(error_prop);
                        continue;
                    }
                    KeyRead::Named(name) => Some(name),
                    KeyRead::IndexSignature => None,
                };
                // A key that names one property declares one member, so the
                // same `unique symbol` rule applies to it as to a written key.
                // An index signature stands for every key it matches, so a
                // `unique symbol` under one stays rejected. A symbol key has no
                // name to render the value after, hence `as_smol_str_opt`.
                let value_ast = match &named {
                    Some(name) => {
                        check_unique_symbol_read_only(
                            cx,
                            &indexer.value,
                            typed_ast_utils::polarity(indexer.variance.as_ref()),
                        );
                        convert_single_binding_value(
                            cx,
                            env,
                            &indexer.value,
                            name.as_smol_str_opt(),
                        )?
                    }
                    None => convert_inner(cx, env, &indexer.value)?,
                };
                let annot_loc = value_ast.loc().0.dupe();
                let value_t = value_ast.loc().1.dupe();
                let value_t = if indexer.optional {
                    type_util::optional(value_t, Some(annot_loc), false)
                } else {
                    value_t
                };
                let dict_polarity = polarity(
                    cx,
                    flow_typing_errors::intermediate_error_types::VarianceSigilParent::Property,
                    indexer.variance.as_ref(),
                );
                match named {
                    Some(name) => {
                        let key_loc = indexer.key.loc().dupe();
                        acc.add_prop(move |pmap| {
                            pmap.insert(
                                name,
                                type_::Property::new(type_::PropertyInner::Field(Box::new(
                                    FieldData {
                                        preferred_def_locs: None,
                                        key_loc: Some(key_loc),
                                        type_: value_t,
                                        polarity: dict_polarity,
                                    },
                                ))),
                            );
                        });
                    }
                    None => {
                        let d = DictType {
                            dict_name: indexer.id.as_ref().map(ident_name),
                            key: key_ast.loc().1.dupe(),
                            value: value_t,
                            dict_polarity,
                        };
                        if let Err(err) = acc.add_dict(d) {
                            flow_js_utils::add_output_non_speculating(
                                cx,
                                ErrorMessage::EUnsupportedSyntax(Box::new((
                                    indexer.loc.dupe(),
                                    err,
                                ))),
                            );
                        }
                    }
                }
                prop_asts.push(Property::Indexer(ast::types::object::Indexer {
                    loc: indexer.loc.dupe(),
                    id: indexer.id.as_ref().map(|id| {
                        let Ok(v) = polymorphic_ast_mapper::t_identifier(
                            &mut typed_ast_utils::ErrorMapper,
                            id,
                        );
                        v
                    }),
                    key: key_ast,
                    value: value_ast,
                    static_: indexer.static_,
                    variance: indexer.variance.clone(),
                    optional: indexer.optional,
                    comments: indexer.comments.dupe(),
                }));
            }
            Property::NormalProperty(obj_prop) => {
                let prop_ast = named_property(cx, env, &mut acc, obj_prop)?;
                prop_asts.push(Property::NormalProperty(prop_ast));
            }
            Property::InternalSlot(slot) => {
                let slot_name = &slot.id.name;
                if slot_name.as_str() == "call" {
                    let value_ast = convert_inner(cx, env, &slot.value)?;
                    let mut t = value_ast.loc().1.dupe();
                    if slot.optional {
                        t = type_util::optional(t, None, false);
                    }
                    if let Err(err) = acc.add_call(t) {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EUnsupportedSyntax(Box::new((slot.loc.dupe(), err))),
                        );
                    }
                    let slot_ast = ast::types::object::InternalSlot {
                        loc: slot.loc.dupe(),
                        id: ast::Identifier::new(ast::IdentifierInner {
                            loc: slot.id.loc.dupe(),
                            name: slot.id.name.dupe(),
                            comments: slot.id.comments.dupe(),
                        }),
                        value: value_ast,
                        optional: slot.optional,
                        static_: slot.static_,
                        method: slot.method,
                        comments: slot.comments.dupe(),
                    };
                    prop_asts.push(Property::InternalSlot(slot_ast));
                } else {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EUnsupportedSyntax(Box::new((
                            slot.loc.dupe(),
                            intermediate_error_types::UnsupportedSyntax::UnsupportedInternalSlot {
                                name: slot_name.dupe(),
                                static_: false,
                            },
                        ))),
                    );
                    let Ok(mapped) = polymorphic_ast_mapper::object_type_property(
                        &mut typed_ast_utils::ErrorMapper,
                        prop,
                    );
                    prop_asts.push(mapped);
                }
            }
            Property::SpreadProperty(sp) => {
                let argument_ast = convert_inner(cx, env, &sp.argument)?;
                let t = argument_ast.loc().1.dupe();
                acc.add_spread(t);
                prop_asts.push(Property::SpreadProperty(
                    ast::types::object::SpreadProperty {
                        loc: sp.loc.dupe(),
                        argument: argument_ast,
                        comments: sp.comments.dupe(),
                    },
                ));
            }
            Property::MappedType(_) => {
                panic!("Unreachable until we support mapped types with additional properties");
            }
            Property::PrivateField(_) => {
                panic!("Unreachable: private fields only appear in declare class bodies");
            }
        }
    }

    // ========================================
    // Final assembly
    // ========================================
    let proto = acc.get_proto(&obj_proto_t, &fun_proto_t);
    let calls: Vec<Type> = acc.calls.clone();

    let (tail, head) = acc.elements_rev();
    let t = match (&head, tail.len()) {
        (AccElement::Slice { dict, pmap }, 0) => {
            let ts: Vec<Type> = calls
                .into_iter()
                .map(|call| mk_object_annot(cx, loc.dupe(), exact, Some(call), dict, pmap, &proto))
                .collect();
            match ts.len() {
                0 => mk_object_annot(cx, loc.dupe(), exact, None, dict, pmap, &proto),
                1 => ts.into_iter().next().unwrap(),
                _ => {
                    let mut iter = ts.into_iter();
                    let t0 = iter.next().unwrap();
                    let t1 = iter.next().unwrap();
                    let rest: Vec<Type> = iter.collect();
                    let callable_reason = reason::mk_annot_reason(None, loc.dupe());
                    let rep = type_::inter_rep::make(t0, t1, rest.into());
                    Type::new(type_::TypeInner::IntersectionT(callable_reason, rep))
                }
            }
        }
        _ => {
            let reason = reason::mk_reason(reason::VirtualReasonDesc::RObjectType, loc.dupe());
            let target = object::spread::Target::Annot {
                make_exact: exact,
                strictness_kind: cx.type_strictness_kind(),
            };

            fn acc_element_to_operand(el: &AccElement, reason: &Reason) -> object::spread::Operand {
                match el {
                    AccElement::Spread(t) => object::spread::Operand::Type(t.dupe()),
                    AccElement::Slice { dict, pmap } => object::spread::Operand::Slice(
                        object::spread::OperandSlice::new(object::spread::OperandSliceInner {
                            reason: reason.dupe(),
                            prop_map: pmap.dupe(),
                            dict: dict.clone(),
                            generics: flow_typing_generics::spread_empty(),
                            reachable_targs: Rc::from([]),
                        }),
                    ),
                }
            }

            match (&head, &tail[..]) {
                (AccElement::Spread(t), ts) => {
                    let operands: flow_data_structure_wrapper::list::FlowOcamlList<
                        object::spread::Operand,
                    > = ts
                        .iter()
                        .map(|el| acc_element_to_operand(el, &reason))
                        .collect();
                    FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                        cx,
                        type_::unknown_use(),
                        &reason,
                        &t.dupe(),
                        &Destructor::SpreadType(Box::new(DestructorSpreadTypeData(
                            target, operands, None,
                        ))),
                        eval::Id::generate_id(),
                    )
                    .expect("mk_type_destructor should not fail")
                }
                (
                    AccElement::Slice {
                        dict: head_dict,
                        pmap: head_pmap,
                    },
                    rest,
                ) if !rest.is_empty()
                    && let AccElement::Spread(spread_t_ref) = &rest[0] =>
                {
                    let spread_t = spread_t_ref.dupe();
                    let head_slice_val =
                        object::spread::OperandSlice::new(object::spread::OperandSliceInner {
                            reason: reason.dupe(),
                            prop_map: head_pmap.dupe(),
                            dict: head_dict.clone(),
                            generics: flow_typing_generics::spread_empty(),
                            reachable_targs: Rc::from([]),
                        });
                    let operands: flow_data_structure_wrapper::list::FlowOcamlList<
                        object::spread::Operand,
                    > = rest[1..]
                        .iter()
                        .map(|el| acc_element_to_operand(el, &reason))
                        .collect();
                    FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                        cx,
                        type_::unknown_use(),
                        &reason,
                        &spread_t,
                        &Destructor::SpreadType(Box::new(DestructorSpreadTypeData(
                            target,
                            operands,
                            Some(head_slice_val),
                        ))),
                        eval::Id::generate_id(),
                    )
                    .expect("mk_type_destructor should not fail")
                }
                _ => {
                    panic!("Invariant Violation: spread list has two slices in a row");
                }
            }
        }
    };

    Ok((t, prop_asts))
}

fn convert_tuple_element<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    element: &ast::types::tuple::Element<ALoc, ALoc>,
) -> Result<
    (
        type_::UnresolvedParam,
        ast::types::tuple::Element<ALoc, (ALoc, Type)>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    use flow_parser::ast::types::tuple::Element;
    let loc = element.loc().dupe();
    Ok(match element {
        Element::UnlabeledElement {
            annot, optional, ..
        } => {
            if *optional && !cx.ts_syntax() {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EUnsupportedSyntax(Box::new((
                        loc.dupe(),
                        intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                            TsLibSyntaxKind::OptionalUnlabeledTupleElement,
                        ),
                    ))),
                );
                let Ok(element_ast) = polymorphic_ast_mapper::tuple_element(
                    &mut typed_ast_utils::ErrorMapper,
                    element,
                );
                let reason = reason::mk_reason(None, loc.dupe());
                let te = type_util::mk_tuple_element(
                    reason,
                    type_::any_t::at(type_::AnySource::AnyError(None), loc),
                    None,
                    *optional,
                    flow_common::polarity::Polarity::Neutral,
                );
                (
                    type_::UnresolvedParam::UnresolvedArg(Box::new(type_::UnresolvedArgData(
                        te, None,
                    ))),
                    element_ast,
                )
            } else {
                let annot_ast = convert_inner(cx, env, annot)?;
                let (_, annot_t) = annot_ast.loc();
                let annot_t = annot_t.dupe();
                let t = if *optional {
                    type_util::optional(annot_t, None, false)
                } else {
                    annot_t
                };
                let element_ast = Element::UnlabeledElement {
                    loc: loc.dupe(),
                    annot: annot_ast,
                    optional: *optional,
                };
                let reason = reason::mk_reason(None, loc);
                let te = type_util::mk_tuple_element(
                    reason,
                    t,
                    None,
                    *optional,
                    flow_common::polarity::Polarity::Neutral,
                );
                (
                    type_::UnresolvedParam::UnresolvedArg(Box::new(type_::UnresolvedArgData(
                        te, None,
                    ))),
                    element_ast,
                )
            }
        }
        Element::LabeledElement {
            element:
                ast::types::tuple::LabeledElement {
                    name: label_name,
                    annot,
                    variance,
                    optional,
                },
            ..
        } => {
            let annot_ast = convert_inner(cx, env, annot)?;
            let (_, annot_t) = annot_ast.loc();
            let annot_t = annot_t.dupe();
            let t = if *optional {
                type_util::optional(annot_t.dupe(), None, false)
            } else {
                annot_t.dupe()
            };
            let name_loc = label_name.loc.dupe();
            let str_name = &label_name.name;
            let id_name = ast::Identifier::new(ast::IdentifierInner {
                loc: (name_loc, t.dupe()),
                name: str_name.dupe(),
                comments: label_name.comments.dupe(),
            });
            let element_ast = Element::LabeledElement {
                loc: loc.dupe(),
                element: ast::types::tuple::LabeledElement {
                    name: id_name,
                    annot: annot_ast,
                    variance: variance.clone(),
                    optional: *optional,
                },
            };
            let reason = reason::mk_reason(None, loc);
            let te = type_::TupleElement {
                name: Some(str_name.dupe()),
                t,
                polarity: polarity(
                    cx,
                    flow_typing_errors::intermediate_error_types::VarianceSigilParent::Property,
                    variance.as_ref(),
                ),
                optional: *optional,
                reason,
            };
            (
                type_::UnresolvedParam::UnresolvedArg(Box::new(type_::UnresolvedArgData(te, None))),
                element_ast,
            )
        }
        Element::SpreadElement {
            element:
                ast::types::tuple::SpreadElement {
                    name: spread_name,
                    annot,
                },
            ..
        } => {
            let annot_ast = convert_inner(cx, env, annot)?;
            let (_, t) = annot_ast.loc();
            let t = t.dupe();
            let name_typed: Option<ast::Identifier<ALoc, (ALoc, Type)>> =
                spread_name.as_ref().map(|n| {
                    ast::Identifier::new(ast::IdentifierInner {
                        loc: (n.loc.dupe(), t.dupe()),
                        name: n.name.dupe(),
                        comments: n.comments.dupe(),
                    })
                });
            let element_ast = Element::SpreadElement {
                loc: loc.dupe(),
                element: ast::types::tuple::SpreadElement {
                    name: name_typed,
                    annot: annot_ast,
                },
            };
            (type_::UnresolvedParam::UnresolvedSpreadArg(t), element_ast)
        }
    })
}

fn check_guard_type<'a>(
    cx: &mut Context<'a>,
    fparams: &[type_::FunParam],
    guard_name: &FlowSmolStr,
    guard_t: &Type,
) {
    let param_t = fparams
        .iter()
        .find_map(|type_::FunParam(name, t)| match name {
            Some(n) if n == guard_name => Some(t),
            _ => None,
        });
    if let Some(param_t) = param_t {
        let use_op = type_::UseOp::Op(Arc::new(type_::RootUseOp::TypeGuardIncompatibility(
            Box::new(type_::TypeGuardIncompatibilityData {
                guard_type_loc: type_util::loc_of_t(guard_t).dupe(),
                guard_type_reference_loc: type_util::ref_loc_of_t(guard_t).dupe(),
                guard_type: flow_js_utils::type_or_type_desc_for_error(guard_t),
                param_name: guard_name.dupe(),
            }),
        )));
        cx.add_post_inference_subtyping_check(guard_t.dupe(), use_op, param_t.dupe());
    }
}

fn has_guard_payload(guard: &ast::types::TypeGuard<ALoc, ALoc>) -> bool {
    guard.guard.1.is_some() || guard.kind == ast::types::TypeGuardKind::Asserts
}

fn type_guard_return_t(kind: ast::types::TypeGuardKind, loc: &ALoc) -> Type {
    match kind {
        ast::types::TypeGuardKind::Asserts => type_::void::at(loc.dupe()),
        _ => type_::bool_module_t::at(loc.dupe()),
    }
}

fn convert_type_guard_inner<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    fparams: &[type_::FunParam],
    gloc: ALoc,
    kind: ast::types::TypeGuardKind,
    id_name: &ast::Identifier<ALoc, ALoc>,
    t: Option<&ast::types::Type<ALoc, ALoc>>,
    comments: Option<&ast::Syntax<ALoc, std::sync::Arc<[ast::Comment<ALoc>]>>>,
) -> Result<
    (
        Type,
        ast::types::TypeGuard<ALoc, (ALoc, Type)>,
        Option<type_::TypeGuard>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    let name_loc = id_name.loc.dupe();
    let name = &id_name.name;
    let t_prime = match t {
        Some(t) => Some(convert_inner(cx, env, t)?),
        None => None,
    };
    let type_guard_t = t_prime.as_ref().map(|t_prime| {
        let (_, type_guard_t) = t_prime.loc();
        type_guard_t.dupe()
    });
    let return_t = type_guard_return_t(kind, &gloc);
    let id_name_typed: ast::Identifier<ALoc, ALoc> = ast::Identifier::new(ast::IdentifierInner {
        loc: id_name.loc.dupe(),
        name: id_name.name.dupe(),
        comments: id_name.comments.dupe(),
    });
    let guard_prime = ast::types::TypeGuard {
        loc: gloc.dupe(),
        kind,
        guard: (id_name_typed, t_prime),
        comments: comments.cloned(),
    };
    let reason = reason::mk_reason(None, gloc);
    if let Some(type_guard_t) = &type_guard_t {
        check_guard_type(cx, fparams, name, type_guard_t);
    }
    let type_guard = Some(type_::TypeGuard::new(type_::TypeGuardInner {
        reason,
        kind,
        inferred: false,
        param_name: (name_loc, name.dupe()),
        type_guard: type_guard_t,
    }));
    Ok((return_t, guard_prime, type_guard))
}

fn error_type_guard<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    loc: ALoc,
    x: &ast::Identifier<ALoc, ALoc>,
    t: Option<&ast::types::Type<ALoc, ALoc>>,
    kind: ast::types::TypeGuardKind,
    comments: Option<&ast::Syntax<ALoc, std::sync::Arc<[ast::Comment<ALoc>]>>>,
    msg: ErrorMessage<ALoc>,
) -> Result<
    (
        Type,
        ast::types::TypeGuard<ALoc, (ALoc, Type)>,
        Option<type_::TypeGuard>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    flow_js_utils::add_output_non_speculating(cx, msg);
    let t_ast = match t {
        Some(t) => Some(convert_inner(cx, env, t)?),
        None => None,
    };
    let x_typed = ast::Identifier::new(ast::IdentifierInner {
        loc: x.loc.dupe(),
        name: x.name.dupe(),
        comments: x.comments.dupe(),
    });
    let guard_prime = ast::types::TypeGuard {
        loc: loc.dupe(),
        kind,
        guard: (x_typed, t_ast),
        comments: comments.cloned(),
    };
    let return_t = type_guard_return_t(kind, &loc);
    Ok((return_t, guard_prime, None))
}

fn convert_return_annotation_inner<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    meth_kind: MethodKind,
    params: &ast::types::function::Params<ALoc, ALoc>,
    fparams: &[type_::FunParam],
    return_annot: &ast::types::function::ReturnAnnotation<ALoc, ALoc>,
) -> Result<
    (
        Type,
        ast::types::function::ReturnAnnotation<ALoc, (ALoc, Type)>,
        Option<type_::TypeGuard>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    use flow_parser::ast::types::function::ReturnAnnotation;
    Ok(match return_annot {
        ReturnAnnotation::Available(t_ast) => {
            let t_ast_prime = convert_inner(cx, env, &t_ast.annotation)?;
            let (_, t_prime) = t_ast_prime.loc();
            let t_prime = t_prime.dupe();
            (
                t_prime,
                ReturnAnnotation::Available(ast::types::Annotation {
                    loc: t_ast.loc.dupe(),
                    annotation: t_ast_prime,
                }),
                None,
            )
        }
        ReturnAnnotation::TypeGuard(guard)
            if guard.guard.0.name.as_str() == "this" && has_guard_payload(guard) =>
        {
            let x = &guard.guard.0;
            let t = guard.guard.1.as_ref();
            let gloc = guard.loc.dupe();
            let kind = guard.kind;
            let comments = guard.comments.as_ref();
            if allows_this_type_guards(meth_kind) {
                let (bool_t, guard_prime, predicate) =
                    convert_type_guard_inner(cx, env, fparams, gloc, kind, x, t, comments)?;
                (bool_t, ReturnAnnotation::TypeGuard(guard_prime), predicate)
            } else {
                let name_loc = x.loc.dupe();
                let msg = ErrorMessage::ETypeGuardThisParam(name_loc);
                let (bool_t, guard_prime, predicate) =
                    error_type_guard(cx, env, gloc, x, t, kind, comments, msg)?;
                (bool_t, ReturnAnnotation::TypeGuard(guard_prime), predicate)
            }
        }
        ReturnAnnotation::TypeGuard(guard) if has_guard_payload(guard) => {
            let x = &guard.guard.0;
            let t = guard.guard.1.as_ref();
            let gloc = guard.loc.dupe();
            let kind = guard.kind;
            let comments = guard.comments.as_ref();
            let name_loc = x.loc.dupe();
            let name = &x.name;
            // Check that type guard variable is not a rest param
            let is_rest_param_conflict = params.rest.as_ref().and_then(|rest| {
                let arg = &rest.argument;
                match &arg.param {
                    ast::types::function::ParamKind::Labeled {
                        name: param_name, ..
                    } => {
                        if param_name.name.as_str() == name.as_str() {
                            Some(param_name.loc.dupe())
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            });
            if let Some(rloc) = is_rest_param_conflict {
                let msg = ErrorMessage::ETypeGuardInvalidParameter(Box::new(
                    ETypeGuardInvalidParameterData {
                        type_guard: TypeGuardParameterData {
                            loc: name_loc,
                            name: name.dupe(),
                        },
                        binding_loc: rloc,
                        binding_kind: TypeGuardBindingKind::RestParameter,
                    },
                ));
                let (bool_t, guard_prime, predicate) =
                    error_type_guard(cx, env, gloc, x, t, kind, comments, msg)?;
                (bool_t, ReturnAnnotation::TypeGuard(guard_prime), predicate)
            } else if !allows_type_guards(meth_kind) {
                // Check that type guard variable appears in parameter list
                let msg = ErrorMessage::ETypeGuardIncompatibleWithFunctionKind(Box::new(
                    ETypeGuardIncompatibleWithFunctionKindData {
                        loc: gloc.dupe(),
                        kind: FlowSmolStr::new(method_kind_to_string(meth_kind)),
                    },
                ));
                let (bool_t, guard_prime, predicate) =
                    error_type_guard(cx, env, gloc, x, t, kind, comments, msg)?;
                (bool_t, ReturnAnnotation::TypeGuard(guard_prime), predicate)
            } else if params.params.iter().all(|p| match &p.param {
                ast::types::function::ParamKind::Labeled { name: pname, .. } => {
                    pname.name.as_str() != name.as_str()
                }
                _ => true,
            }) {
                let msg = ErrorMessage::ETypeGuardParamUnbound(TypeGuardParameterData {
                    loc: name_loc,
                    name: name.dupe(),
                });
                let (bool_t, guard_prime, predicate) =
                    error_type_guard(cx, env, gloc, x, t, kind, comments, msg)?;
                (bool_t, ReturnAnnotation::TypeGuard(guard_prime), predicate)
            } else {
                let (bool_t, guard_prime, predicate) =
                    convert_type_guard_inner(cx, env, fparams, gloc, kind, x, t, comments)?;
                (bool_t, ReturnAnnotation::TypeGuard(guard_prime), predicate)
            }
        }
        ReturnAnnotation::TypeGuard(guard) => {
            let loc = guard.loc.dupe();
            let kind = guard.kind;
            flow_js_utils::add_output_non_speculating(
                cx,
                ErrorMessage::EUnsupportedSyntax(Box::new((
                    loc.dupe(),
                    intermediate_error_types::UnsupportedSyntax::UserDefinedTypeGuards { kind },
                ))),
            );
            let any_t = type_::any_t::at(type_::AnySource::AnyError(None), loc);
            let Ok(mapped_ret) = polymorphic_ast_mapper::function_type_return_annotation(
                &mut typed_ast_utils::ErrorMapper,
                return_annot,
            );
            (any_t, mapped_ret, None)
        }
        ReturnAnnotation::Missing(loc) => {
            let is_ts_file = flow_common::files::has_ts_ext(cx.file());
            match meth_kind {
                MethodKind::ConstructorKind => {
                    if !cx.tslib_syntax() {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EUnsupportedSyntax(Box::new((
                                loc.dupe(),
                                intermediate_error_types::UnsupportedSyntax::DeclareClassMethodMissingReturnType,
                            ))),
                        );
                    }
                    let void_t = type_::void::at(loc.dupe());
                    (void_t, ReturnAnnotation::Missing(loc.dupe()), None)
                }
                MethodKind::SetterKind => {
                    let void_t = type_::void::at(loc.dupe());
                    (void_t, ReturnAnnotation::Missing(loc.dupe()), None)
                }
                MethodKind::MethodKind { .. } => {
                    if !is_ts_file {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EUnsupportedSyntax(Box::new((
                                loc.dupe(),
                                intermediate_error_types::UnsupportedSyntax::DeclareClassMethodMissingReturnType,
                            ))),
                        );
                    }
                    let any_source = if is_ts_file {
                        type_::AnySource::AnnotatedAny
                    } else {
                        type_::AnySource::AnyError(None)
                    };
                    let any_t = type_::any_t::at(any_source, loc.dupe());
                    (any_t, ReturnAnnotation::Missing(loc.dupe()), None)
                }
                MethodKind::GetterKind | MethodKind::FunctionKind => {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EUnsupportedSyntax(Box::new((
                            loc.dupe(),
                            intermediate_error_types::UnsupportedSyntax::DeclareClassMethodMissingReturnType,
                        ))),
                    );
                    let any_t = type_::any_t::at(type_::AnySource::AnyError(None), loc.dupe());
                    (any_t, ReturnAnnotation::Missing(loc.dupe()), None)
                }
            }
        }
    })
}

/// Build typed AST for a function type parameter after type-checking.
pub fn typed_function_param_ast(
    param: &ast::types::function::ParamKind<ALoc, ALoc>,
    t: Type,
    annot_ast: ast::types::Type<ALoc, (ALoc, Type)>,
) -> ast::types::function::ParamKind<ALoc, (ALoc, Type)> {
    match param {
        ast::types::function::ParamKind::Labeled { name, optional, .. } => {
            let typed_name = ast::Identifier::new(ast::IdentifierInner {
                loc: (name.loc.dupe(), t),
                name: name.name.dupe(),
                comments: name.comments.dupe(),
            });
            ast::types::function::ParamKind::Labeled {
                name: typed_name,
                annot: annot_ast,
                optional: *optional,
            }
        }
        ast::types::function::ParamKind::Anonymous(_) => {
            ast::types::function::ParamKind::Anonymous(annot_ast)
        }
        ast::types::function::ParamKind::Destructuring(pattern) => {
            // Destructuring patterns in function type params are not runtime bindings —
            // they exist purely for documentation in .d.ts files. Map them with
            // unimplemented_mapper to assign placeholder types to the pattern nodes.
            let Ok(mapped) = polymorphic_ast_mapper::pattern(
                &mut typed_ast_utils::UnimplementedMapper,
                None,
                pattern,
            );
            ast::types::function::ParamKind::Destructuring(mapped)
        }
    }
}

fn mk_method_func_sig<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    meth_kind: MethodKind,
    loc: ALoc,
    func: &ast::types::Function<ALoc, ALoc>,
) -> Result<
    (
        func_class_sig_types::func::Func<FuncTypeParamsConfig>,
        ast::types::Function<ALoc, (ALoc, Type)>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    fn add_param<'a>(
        cx: &mut Context<'a>,
        env: &mut ConvertEnv,
        x: &mut func_class_sig_types::param::Param<FuncTypeParamsConfig>,
        param: &ast::types::function::Param<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let param_loc = param.loc.dupe();
        let (_name, annot, _optional) =
            flow_parser::ast_utils::function_type_param_parts(&param.param);
        let annot_ast = convert_inner(cx, env, annot)?;
        let (_, t) = annot_ast.loc();
        let t = t.dupe();
        let param_ast = typed_function_param_ast(&param.param, t.dupe(), annot_ast);
        let typed_param = ast::types::function::Param {
            loc: param_loc,
            param: param_ast,
        };
        crate::func_params::add_param::<FuncTypeParamsConfig>((t, typed_param), x);
        Ok(())
    }

    fn add_rest<'a>(
        cx: &mut Context<'a>,
        env: &mut ConvertEnv,
        x: &mut func_class_sig_types::param::Param<FuncTypeParamsConfig>,
        rest_param: &ast::types::function::RestParam<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let rest_loc = rest_param.loc.dupe();
        let argument = &rest_param.argument;
        let arg_loc = argument.loc.dupe();
        let (_name, annot, _optional) =
            flow_parser::ast_utils::function_type_param_parts(&argument.param);
        let annot_ast = convert_inner(cx, env, annot)?;
        let (_, t) = annot_ast.loc();
        let t = t.dupe();
        let rest_param_ast_inner = typed_function_param_ast(&argument.param, t.dupe(), annot_ast);
        let typed_rest = ast::types::function::RestParam {
            loc: rest_loc,
            argument: ast::types::function::Param {
                loc: arg_loc,
                param: rest_param_ast_inner,
            },
            comments: rest_param.comments.dupe(),
        };
        crate::func_params::add_rest::<FuncTypeParamsConfig>((t, typed_rest), x);
        Ok(())
    }

    fn add_this<'a>(
        cx: &mut Context<'a>,
        env: &mut ConvertEnv,
        x: &mut func_class_sig_types::param::Param<FuncTypeParamsConfig>,
        this_param: &ast::types::function::ThisParam<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let this_loc = this_param.loc.dupe();
        let annot = &this_param.annot;
        let annot_ast = convert_inner(cx, env, &annot.annotation)?;
        let (_, t) = annot_ast.loc();
        let t = t.dupe();
        let typed_this = ast::types::function::ThisParam {
            loc: this_loc,
            annot: ast::types::Annotation {
                loc: annot.loc.dupe(),
                annotation: annot_ast,
            },
            comments: this_param.comments.dupe(),
        };
        crate::func_params::add_this::<FuncTypeParamsConfig>((t, typed_this), x);
        Ok(())
    }

    fn convert_params<'a>(
        cx: &mut Context<'a>,
        env: &mut ConvertEnv,
        params_node: &ast::types::function::Params<ALoc, ALoc>,
    ) -> Result<
        (
            func_class_sig_types::param::Param<FuncTypeParamsConfig>,
            ast::types::function::Params<ALoc, (ALoc, Type)>,
        ),
        flow_utils_concurrency::job_error::JobError,
    > {
        let params_loc = params_node.loc.dupe();
        let comments = params_node.comments.dupe();
        let mut fparams = crate::func_params::empty::<FuncTypeParamsConfig>(Rc::new(
            move |params, rest, this_| {
                Some(ast::types::function::Params {
                    loc: params_loc.dupe(),
                    params: Arc::from(params),
                    rest,
                    this: this_,
                    comments: comments.clone(),
                })
            },
        ));
        for param in params_node.params.iter() {
            add_param(cx, env, &mut fparams, param)?;
        }
        if let Some(rest) = &params_node.rest {
            add_rest(cx, env, &mut fparams, rest)?;
        }
        if let Some(this) = &params_node.this {
            add_this(cx, env, &mut fparams, this)?;
        }
        let params_ast = crate::func_params::eval::<FuncTypeParamsConfig, _>(
            cx,
            &fparams.params,
            fparams.rest.as_ref(),
            fparams.this_.as_ref(),
            &*fparams.reconstruct,
        )
        .expect("eval should not fail for type annotation params");
        Ok((
            fparams,
            params_ast.expect("reconstruct always returns Some"),
        ))
    }

    error_on_unsupported_variance_annotation(
        cx,
        method_kind_to_string(meth_kind),
        func.tparams.as_ref(),
    );
    let saved_tparams_map = env.tparams_map.dupe();
    let (tparams, tparams_ast) = mk_type_param_declarations_inner(
        cx,
        env,
        flow_parser::ast_visitor::TypeParamsContext::FunctionType,
        func.tparams.as_ref(),
    )?;
    let (fparams, params_ast) = convert_params(cx, env, &func.params)?;
    let fparams_value = crate::func_params::value::<FuncTypeParamsConfig>(&fparams.params);
    let (return_t, return_ast, type_guard) = convert_return_annotation_inner(
        cx,
        env,
        meth_kind,
        &func.params,
        &fparams_value,
        &func.return_,
    )?;
    env.tparams_map = saved_tparams_map;
    let kind = match type_guard {
        None => func_class_sig_types::func::Kind::Ordinary,
        Some(g) => func_class_sig_types::func::Kind::TypeGuard(g),
    };
    let reason = reason::mk_annot_reason(reason::VirtualReasonDesc::RFunctionType, loc.dupe());
    // Methods can't be hooks
    let effect_flag = type_::ReactEffectType::ArbitraryEffect;
    let ret_annot_loc = type_util::loc_of_t(&return_t).dupe();
    let func_sig = func_class_sig_types::func::Func {
        reason,
        function_reference_kind: flow_common::error_ref::FunctionReferenceKind::FunctionType,
        kind,
        tparams,
        fparams,
        body: None,
        return_t: type_::AnnotatedOrInferred::Annotated(return_t),
        ret_annot_loc,
        statics: None,
        effect_: effect_flag,
        literal: false,
    };
    let func_ast = ast::types::Function {
        tparams: tparams_ast,
        params: params_ast,
        return_: return_ast,
        effect: func.effect.clone(),
        comments: None,
    };
    Ok((func_sig, func_ast))
}

fn mk_type_available_annotation_inner<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    annotation: &ast::types::Annotation<ALoc, ALoc>,
) -> Result<
    (Type, ast::types::Annotation<ALoc, (ALoc, Type)>),
    flow_utils_concurrency::job_error::JobError,
> {
    let binds_single_value = env.take_binds_single_value();
    let node_cache = cx.node_cache();
    let loc = &annotation.loc;
    let annot = &annotation.annotation;
    let is_tparam_generic = match annot.deref() {
        ast::types::TypeInner::Generic { inner, .. } => match &inner.id {
            ast::types::generic::Identifier::Unqualified(id) => env
                .tparams_map
                .contains_key(&SubstName::name(id.name.dupe())),
            _ => false,
        },
        _ => false,
    };
    let convert_annot = |cx: &mut Context<'a>, env: &mut ConvertEnv| match &binds_single_value {
        BindsSingleValue::Yes(name_hint) => {
            convert_single_binding_value(cx, env, annot, name_hint.as_ref())
        }
        BindsSingleValue::No => convert_inner(cx, env, annot),
    };
    // `binds_single_value` only changes the result for a bare `unique symbol`,
    // and the node cache is keyed on the annotation's location alone, so a
    // cached conversion may have been made for a different position: a variable
    // declaration's annotation is resolved once for its binding and again here,
    // and a destructuring pattern shares one annotation with every name it
    // binds. Convert such an annotation fresh so the position asking for it
    // decides. The symbol's identity is derived from `loc`, so converting twice
    // still names the same symbol.
    let binds_by_position = bare_unique_symbol_loc(annot).is_some();
    let annot_ast = if is_tparam_generic || binds_by_position {
        // If the type we're converting is in the tparams map, we prefer that over
        // the node cache
        convert_annot(cx, env)?
    } else {
        match node_cache.get_annotation(loc) {
            Some(cached) => {
                flow_typing_debug::verbose::print_if_verbose_lazy(cx, None, None, None, || {
                    use flow_parser::loc_sig::LocSig;
                    vec![format!(
                        "Annotation cache hit at {}",
                        loc.debug_to_string(true)
                    )]
                });
                cached.annotation
            }
            None => convert_annot(cx, env)?,
        }
    };
    let (_, t) = annot_ast.loc();
    let t = t.dupe();
    Ok((
        t,
        ast::types::Annotation {
            loc: loc.dupe(),
            annotation: annot_ast,
        },
    ))
}

fn mk_function_type_annotation_inner<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    func: &(ALoc, ast::types::Function<ALoc, ALoc>),
) -> Result<
    (Type, (ALoc, ast::types::Function<ALoc, (ALoc, Type)>)),
    flow_utils_concurrency::job_error::JobError,
> {
    let (ref loc, ref f) = *func;
    let ast_type = ast::types::Type::new(ast::types::TypeInner::Function {
        loc: loc.dupe(),
        inner: std::sync::Arc::new(f.clone()),
    });
    let result = convert_inner(cx, env, &ast_type)?;
    Ok(match result.deref() {
        ast::types::TypeInner::Function {
            loc: (_, function_type),
            inner: f_ast,
        } => (function_type.dupe(), (loc.dupe(), f_ast.as_ref().clone())),
        _ => panic!("convert of Function should return Function"),
    })
}

fn mk_singleton_string(loc: ALoc, key: &str) -> Type {
    let key = FlowSmolStr::new(key);
    let reason = reason::mk_annot_reason(reason::VirtualReasonDesc::RStringLit(key.dupe()), loc);
    Type::new(type_::TypeInner::DefT(
        reason,
        type_::DefT::new(type_::DefTInner::SingletonStrT {
            from_annot: true,
            value: key,
        }),
    ))
}

fn mk_singleton_number(loc: ALoc, num: f64, raw: &str) -> Type {
    let raw_str = FlowSmolStr::new(raw);
    let reason =
        reason::mk_annot_reason(reason::VirtualReasonDesc::RNumberLit(raw_str.dupe()), loc);
    Type::new(type_::TypeInner::DefT(
        reason,
        type_::DefT::new(type_::DefTInner::SingletonNumT {
            from_annot: true,
            value: type_::NumberLiteral(num, raw_str),
        }),
    ))
}

fn mk_singleton_boolean(loc: ALoc, b: bool) -> Type {
    let reason = reason::mk_annot_reason(reason::VirtualReasonDesc::RBooleanLit(b), loc);
    Type::new(type_::TypeInner::DefT(
        reason,
        type_::DefT::new(type_::DefTInner::SingletonBoolT {
            from_annot: true,
            value: b,
        }),
    ))
}

fn mk_singleton_bigint(loc: ALoc, num: Option<i64>, raw: &str) -> Type {
    let raw_str = FlowSmolStr::new(raw);
    let reason =
        reason::mk_annot_reason(reason::VirtualReasonDesc::RBigIntLit(raw_str.dupe()), loc);
    Type::new(type_::TypeInner::DefT(
        reason,
        type_::DefT::new(type_::DefTInner::SingletonBigIntT {
            from_annot: true,
            value: type_::BigIntLiteral(num, raw_str),
        }),
    ))
}

/// Given the type of expression C and type arguments T1...Tn, return the type of
/// values described by C<T1,...,Tn>, or C when there are no type arguments.
fn mk_nominal_type_inner<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    reason: Reason,
    c: Type,
    targs: Option<&ast::types::TypeArgs<ALoc, ALoc>>,
) -> Result<
    (Type, Option<ast::types::TypeArgs<ALoc, (ALoc, Type)>>),
    flow_utils_concurrency::job_error::JobError,
> {
    let annot_loc = reason.loc().dupe();
    Ok(match targs {
        None => {
            let reason = reason.annotate(annot_loc);
            let type_t_kind = if env.in_renders_arg {
                type_::TypeTKind::RenderTypeKind
            } else {
                type_::TypeTKind::InstanceKind
            };
            let t = crate::type_annotation_cons_gen::mk_instance(
                cx,
                reason,
                c,
                Some(type_t_kind),
                None,
            );
            (t, None)
        }
        Some(targs_node) => {
            let (targs_ts, targs_ast) = convert_list_inner(cx, env, &targs_node.arguments)?;
            let t = type_util::typeapp_annot(false, false, annot_loc, c, targs_ts);
            if type_subst::free_var_finder(cx, None, &t).is_empty() {
                match t.deref() {
                    type_::TypeInner::TypeAppT(box TypeAppTData {
                        reason: app_reason,
                        use_op,
                        type_: type_t,
                        targs: app_targs,
                        ..
                    }) => {
                        cx.add_post_inference_validation_flow(
                            type_t.dupe(),
                            type_::UseT::new(type_::UseTInner::SpecializeT(Box::new(
                                type_::SpecializeTData {
                                    use_op: use_op.dupe(),
                                    reason: app_reason.dupe(),
                                    reason2: app_reason.dupe(),
                                    targs: Some(app_targs.clone()),
                                    upper: None,
                                },
                            ))),
                        );
                    }
                    _ => panic!("typeapp_annot should create a TypeAppT"),
                }
            }
            (
                t,
                Some(ast::types::TypeArgs::<ALoc, (ALoc, Type)> {
                    loc: targs_node.loc.dupe(),
                    arguments: targs_ast.into(),
                    comments: targs_node.comments.dupe(),
                }),
            )
        }
    })
}

fn mk_type_param_inner<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    kind: flow_parser::ast_visitor::TypeParamsContext,
    type_param: &ast::types::TypeParam<ALoc, ALoc>,
) -> Result<
    (ast::types::TypeParam<ALoc, (ALoc, Type)>, TypeParam, Type),
    flow_utils_concurrency::job_error::JobError,
> {
    use flow_parser::ast_visitor::TypeParamsContext;
    let node_cache = cx.node_cache();
    let loc = &type_param.loc;
    if let Some(cached) = node_cache.get_tparam(loc) {
        return Ok(cached);
    }

    let id = &type_param.name;
    let name_loc = id.loc.dupe();
    let name = &id.name;
    let bound = &type_param.bound;
    let bound_kind = &type_param.bound_kind;
    let variance = type_param.variance.as_ref();
    let default = &type_param.default;
    let const_mod = &type_param.const_;

    let reason = reason::mk_annot_reason(
        reason::VirtualReasonDesc::RType(name.dupe()),
        name_loc.dupe(),
    );
    let pol = polarity(
        cx,
        flow_typing_errors::intermediate_error_types::VarianceSigilParent::TypeParam,
        variance,
    );
    let is_const = match (const_mod, kind) {
        (Some(_), TypeParamsContext::Class)
        | (Some(_), TypeParamsContext::Function)
        | (Some(_), TypeParamsContext::DeclareFunction)
        | (Some(_), TypeParamsContext::DeclareClass)
        | (Some(_), TypeParamsContext::DeclareComponent)
        | (Some(_), TypeParamsContext::ComponentDeclaration)
        | (Some(_), TypeParamsContext::ComponentType)
        | (Some(_), TypeParamsContext::FunctionType)
        | (Some(_), TypeParamsContext::Record) => true,
        (Some(_), _) => {
            flow_js_utils::add_output_non_speculating(
                cx,
                ErrorMessage::ETypeParamConstInvalidPosition(Box::new((
                    name_loc.dupe(),
                    name.dupe(),
                ))),
            );
            false
        }
        (None, _) => false,
    };
    if *bound_kind == ast::types::type_param::BoundKind::Colon
        && matches!(bound, ast::types::AnnotationOrHint::Available(_))
        && !matches!(kind, TypeParamsContext::Infer)
    {
        flow_js_utils::add_output_non_speculating(
            cx,
            ErrorMessage::ETSSyntax(Box::new(ETSSyntaxData {
                kind: flow_typing_errors::error_message::TSSyntaxKind::DeprecatedTypeParamColon,
                loc: loc.dupe(),
            })),
        );
    }
    let (bound_t, bound_ast) = match bound {
        ast::types::AnnotationOrHint::Missing(missing_loc) => {
            let t = type_::Type::new(type_::TypeInner::DefT(
                reason
                    .dupe()
                    .replace_desc(reason::VirtualReasonDesc::RMixed),
                type_::DefT::new(type_::DefTInner::MixedT(
                    type_::MixedFlavor::MixedEverything,
                )),
            ));
            (
                t.dupe(),
                ast::types::AnnotationOrHint::Missing((missing_loc.dupe(), t)),
            )
        }
        ast::types::AnnotationOrHint::Available(annot) => {
            let bound_loc = &annot.loc;
            let u = &annot.annotation;
            let annot_ast = convert_inner(cx, env, u)?;
            let (_, bound_val) = annot_ast.loc();
            let bound_val = bound_val.dupe();
            let bound_ast = ast::types::AnnotationOrHint::Available(ast::types::Annotation {
                loc: bound_loc.dupe(),
                annotation: annot_ast,
            });
            (bound_val, bound_ast)
        }
    };
    let (default_t, default_ast) = match default {
        None => (None, None),
        Some(default_annot) => {
            let annot_ast = convert_inner(cx, env, default_annot)?;
            let (_, t) = annot_ast.loc();
            let t = t.dupe();
            cx.add_post_inference_subtyping_check(t.dupe(), type_::unknown_use(), bound_t.dupe());
            (Some(t), Some(annot_ast))
        }
    };
    let subst_name = if matches!(kind, TypeParamsContext::Infer)
        && env.tparams_map.contains_key(&SubstName::name(name.dupe()))
    {
        let keys: FlowOrdSet<SubstName> = env.tparams_map.keys().cloned().collect();
        alpha_rename::subst_name(&SubstName::name(name.dupe()), &keys)
    } else {
        SubstName::name(name.dupe())
    };
    let tparam = TypeParam::new(type_::TypeParamInner {
        reason: reason.dupe(),
        name: subst_name,
        bound: bound_t,
        polarity: pol,
        default: default_t,
        is_this: false,
        is_const,
    });
    let t = flow_js_utils::generic_of_tparam(cx, |_, x: &Type| x.dupe(), &tparam);
    let name_ast = ast::Identifier::new(ast::IdentifierInner {
        loc: (name_loc, t.dupe()),
        name: id.name.dupe(),
        comments: id.comments.dupe(),
    });
    let ast = ast::types::TypeParam {
        loc: loc.dupe(),
        name: name_ast,
        bound: bound_ast,
        bound_kind: bound_kind.clone(),
        variance: variance.cloned(),
        default: default_ast,
        const_: const_mod
            .as_ref()
            .map(|cm| ast::types::type_param::ConstModifier {
                loc: cm.loc.dupe(),
                comments: cm.comments.dupe(),
            }),
    };
    Ok((ast, tparam, t))
}

/// take a list of AST type param declarations,
/// do semantic checking and create types for them.
fn mk_type_param_declarations_inner<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    kind: flow_parser::ast_visitor::TypeParamsContext,
    tparams: Option<&ast::types::TypeParams<ALoc, ALoc>>,
) -> Result<
    (
        type_::TypeParams,
        Option<ast::types::TypeParams<ALoc, (ALoc, Type)>>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    Ok(match tparams {
        None => (None, None),
        Some(tparams_node) => {
            let tparams_loc = &tparams_node.loc;
            let comments = &tparams_node.comments;
            let mut tparams: Vec<TypeParam> = Vec::new();
            let mut bounds_map: FlowOrdMap<SubstName, Type> = FlowOrdMap::default();
            let mut asts: Vec<ast::types::TypeParam<ALoc, (ALoc, Type)>> = Vec::new();
            for tp in tparams_node.params.iter() {
                let (ast, tparam, t) = mk_type_param_inner(cx, env, kind, tp)?;
                let name = tparam.name.dupe();
                let bound = tparam.bound.dupe();
                tparams.push(tparam);
                env.tparams_map.insert(name.dupe(), t);
                let subst_bound = type_subst::subst(
                    cx,
                    None,
                    true,
                    false,
                    type_subst::Purpose::Normal,
                    &bounds_map,
                    bound,
                );
                bounds_map.insert(name, subst_bound);
                asts.push(ast);
            }
            let tparams_ast = Some(ast::types::TypeParams::<ALoc, (ALoc, Type)> {
                loc: tparams_loc.dupe(),
                params: asts.into(),
                comments: comments.clone(),
            });
            let tparams = if tparams.is_empty() {
                None
            } else {
                let mut iter = tparams.into_iter();
                let hd = iter.next().unwrap();
                let mut nel = vec1::Vec1::new(hd);
                nel.extend(iter);
                Some((tparams_loc.dupe(), nel))
            };
            (tparams, tparams_ast)
        }
    })
}

fn type_identifier<'a>(
    cx: &mut Context<'a>,
    name: &FlowSmolStr,
    loc: ALoc,
) -> Result<Type, flow_utils_concurrency::job_error::JobError> {
    let t = type_env::lookup_type_identifier(type_env::LookupMode::ForType, cx, name, loc.dupe())?;
    Ok(type_util::mod_reason_of_t(
        &|r: Reason| r.reposition(loc.dupe()),
        &t,
    ))
}

/// The unapplied generic named by an annotation head that desugars to a
/// builtin (e.g. `Array` in `Array<T>`), recorded on the identifier for
/// hover. `None` when the name has no binding, in which case the identifier
/// keeps the applied type. The lookup itself would report an unbound name,
/// so it only runs when the global resolves.
fn unapplied_type_identifier_opt<'a>(
    cx: &mut Context<'a>,
    name: &FlowSmolStr,
    loc: ALoc,
) -> Result<Option<Type>, flow_utils_concurrency::job_error::JobError> {
    if cx.builtin_type_opt(name.as_str()).is_some() {
        Ok(Some(type_identifier(cx, name, loc)?))
    } else {
        Ok(None)
    }
}

pub fn binding_kind_of_generic_id_pre_convert(
    cx: &mut Context,
    id: &ast::types::generic::Identifier<ALoc, ALoc>,
) -> ClassLikeBindingKind {
    use ast::types::generic::Identifier;

    fn leaf(id: &ast::types::generic::Identifier<ALoc, ALoc>) -> Option<ALoc> {
        match id {
            Identifier::Unqualified(ident) => Some(ident.loc.dupe()),
            Identifier::Qualified(q) => leaf(&q.qualification),
            Identifier::ImportTypeAnnot(_) => None,
        }
    }

    match leaf(id).and_then(|loc| type_env::local_classlike_binding_kind_at_loc(cx, loc)) {
        None => ClassLikeBindingKind::BindingUnknown,
        Some(type_env::LocalClasslikeBindingKind::Mono) => ClassLikeBindingKind::ClassLikeMono,
        Some(type_env::LocalClasslikeBindingKind::Poly) => ClassLikeBindingKind::ClassLikePoly,
        Some(type_env::LocalClasslikeBindingKind::Other) => ClassLikeBindingKind::BindingOther,
    }
}

pub fn binding_kind_of_generic_id_post_convert(
    cx: &mut Context,
    id_pre_convert: &ast::types::generic::Identifier<ALoc, ALoc>,
    converted_t: &Type,
) -> Result<ClassLikeBindingKind, flow_utils_concurrency::job_error::JobError> {
    use ast::types::generic::Identifier;

    fn leaf(id: &ast::types::generic::Identifier<ALoc, ALoc>) -> Option<ALoc> {
        match id {
            Identifier::Unqualified(ident) => Some(ident.loc.dupe()),
            Identifier::Qualified(q) => leaf(&q.qualification),
            Identifier::ImportTypeAnnot(_) => None,
        }
    }

    let initial = binding_kind_of_generic_id_pre_convert(cx, id_pre_convert);
    match initial {
        ClassLikeBindingKind::BindingUnknown => {
            match leaf(id_pre_convert).and_then(|loc| type_env::import_info_at_loc(cx, loc)) {
                Some((import_kind_at_loc, import_spec, source, source_loc)) => {
                    let module_name = Userland::from_smol_str(source.dupe());
                    let import_kind_for_untyped_import_validation = Some(
                        flow_js_utils::import_export_utils::type_kind_of_kind(&import_kind_at_loc),
                    );
                    let source_module =
                        match flow_js_utils::import_export_utils::get_module_type_or_any(
                            cx,
                            false,
                            import_kind_for_untyped_import_validation,
                            source_loc.dupe(),
                            module_name.dupe(),
                        ) {
                            Ok(v) => v,
                            Err(flow_js_utils::FlowJsException::WorkerCanceled(c)) => {
                                return Err(flow_utils_concurrency::job_error::JobError::Canceled(
                                    c,
                                ));
                            }
                            Err(flow_js_utils::FlowJsException::TimedOut(t)) => {
                                return Err(flow_utils_concurrency::job_error::JobError::TimedOut(
                                    t,
                                ));
                            }
                            Err(err) => panic!("Should not be under speculation: {:?}", err),
                        };
                    let import_reason = type_util::reason_of_t(converted_t).dupe();
                    let raw_t_opt = match import_spec {
                        NameDefImport::Named {
                            kind,
                            remote,
                            local: _,
                        } => {
                            let import_kind = kind.as_ref().unwrap_or(&import_kind_at_loc);
                            match flow_js_utils::import_export_utils::import_named_specifier_type_for_extends(
                                cx,
                                import_reason,
                                import_kind,
                                module_name,
                                &source_module,
                                &remote,
                            ) {
                                Ok(t) => Some(t),
                                Err(flow_js_utils::FlowJsException::WorkerCanceled(c)) => {
                                    return Err(
                                        flow_utils_concurrency::job_error::JobError::Canceled(c),
                                    );
                                }
                                Err(flow_js_utils::FlowJsException::TimedOut(t)) => {
                                    return Err(
                                        flow_utils_concurrency::job_error::JobError::TimedOut(t),
                                    );
                                }
                                Err(err) => panic!("Should not be under speculation: {:?}", err),
                            }
                        }
                        NameDefImport::Default(local_name) => {
                            match flow_js_utils::import_export_utils::import_default_specifier_type_for_extends(
                                cx,
                                import_reason,
                                &import_kind_at_loc,
                                module_name,
                                &source_module,
                                &local_name,
                            ) {
                                Ok(t) => Some(t),
                                Err(flow_js_utils::FlowJsException::WorkerCanceled(c)) => {
                                    return Err(
                                        flow_utils_concurrency::job_error::JobError::Canceled(c),
                                    );
                                }
                                Err(flow_js_utils::FlowJsException::TimedOut(t)) => {
                                    return Err(
                                        flow_utils_concurrency::job_error::JobError::TimedOut(t),
                                    );
                                }
                                Err(err) => panic!("Should not be under speculation: {:?}", err),
                            }
                        }
                        NameDefImport::Namespace(_) => None,
                    };
                    match raw_t_opt {
                        None => Ok(ClassLikeBindingKind::BindingUnknown),
                        Some(raw_t) => {
                            Ok(match flow_js_utils::polymorphic_class_kind(cx, &raw_t) {
                                flow_js_utils::PolymorphicClassKind::Mono => {
                                    ClassLikeBindingKind::ClassLikeRawMono(raw_t)
                                }
                                flow_js_utils::PolymorphicClassKind::Poly => {
                                    ClassLikeBindingKind::ClassLikeRawPoly(raw_t)
                                }
                                flow_js_utils::PolymorphicClassKind::Other => {
                                    ClassLikeBindingKind::BindingUnknown
                                }
                            })
                        }
                    }
                }
                None => Ok(ClassLikeBindingKind::BindingUnknown),
            }
        }
        ClassLikeBindingKind::BindingOther => {
            if matches!(id_pre_convert, Identifier::Qualified(_)) {
                Ok(
                    match flow_js_utils::polymorphic_class_kind(cx, converted_t) {
                        flow_js_utils::PolymorphicClassKind::Mono => {
                            ClassLikeBindingKind::ClassLikeRawMono(converted_t.dupe())
                        }
                        flow_js_utils::PolymorphicClassKind::Poly => {
                            ClassLikeBindingKind::ClassLikeRawPoly(converted_t.dupe())
                        }
                        flow_js_utils::PolymorphicClassKind::Other => {
                            ClassLikeBindingKind::BindingOther
                        }
                    },
                )
            } else {
                Ok(ClassLikeBindingKind::BindingOther)
            }
        }
        ClassLikeBindingKind::ClassLikeMono | ClassLikeBindingKind::ClassLikePoly => {
            if matches!(id_pre_convert, Identifier::Qualified(_)) {
                Ok(
                    match flow_js_utils::polymorphic_class_kind(cx, converted_t) {
                        flow_js_utils::PolymorphicClassKind::Mono => {
                            ClassLikeBindingKind::ClassLikeRawMono(converted_t.dupe())
                        }
                        flow_js_utils::PolymorphicClassKind::Poly => {
                            ClassLikeBindingKind::ClassLikeRawPoly(converted_t.dupe())
                        }
                        flow_js_utils::PolymorphicClassKind::Other => {
                            ClassLikeBindingKind::BindingOther
                        }
                    },
                )
            } else {
                Ok(initial)
            }
        }
        ClassLikeBindingKind::ClassLikeRawMono(_) | ClassLikeBindingKind::ClassLikeRawPoly(_) => {
            Ok(initial)
        }
    }
}

fn mk_interface_super<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    generic_with_loc: &(ALoc, ast::types::Generic<ALoc, ALoc>),
) -> Result<
    (
        ((ALoc, Type, Option<Vec<Type>>), ClassLikeBindingKind),
        (ALoc, ast::types::Generic<ALoc, (ALoc, Type)>),
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    let (loc, generic) = generic_with_loc;
    let id = &generic.id;
    let targs = &generic.targs;
    let comments = &generic.comments;
    let id_pre_convert = id;
    let (c, id_typed) = convert_qualification(cx, id)?;
    let binding_kind = binding_kind_of_generic_id_post_convert(cx, id_pre_convert, &c)?;
    let (typeapp, targs_typed) = match targs {
        None => ((loc.dupe(), c, None), None),
        Some(targs_node) => {
            let (ts, targs_ast) = convert_list_inner(cx, env, &targs_node.arguments)?;
            (
                (loc.dupe(), c, Some(ts)),
                Some(ast::types::TypeArgs::<ALoc, (ALoc, Type)> {
                    loc: targs_node.loc.dupe(),
                    arguments: targs_ast.into(),
                    comments: targs_node.comments.dupe(),
                }),
            )
        }
    };
    Ok((
        (typeapp, binding_kind),
        (
            loc.dupe(),
            ast::types::Generic {
                id: id_typed,
                targs: targs_typed,
                comments: comments.dupe(),
            },
        ),
    ))
}

/// A construct signature uses the keyword `new` as a syntactic marker — it's not
/// a property whose name happens to be "new". Restrict to:
/// - a method (`new(): T`, not `new: () => T`);
/// - non-static (construct sigs live on the instance side only);
/// - an unquoted Identifier key (`"new"()` is an ordinary method);
/// - not optional (`new?()` makes no sense for a construct signature).
///
/// Mirror predicate in [type_sig_parse.rs] [interface_props.prop] and
/// [object_type].
fn is_construct_signature(np: &ast::types::object::NormalProperty<ALoc, ALoc>) -> bool {
    np.method
        && !np.static_
        && !np.optional
        && matches!(
            &np.key,
            ast::expression::object::Key::Identifier(id) if id.name == "new"
        )
}

/// Whether the interface's values are functions at runtime — a call property or
/// a construct signature both make them so. See [InterfaceSuper::function_like].
fn is_function_like(properties: &[ast::types::object::Property<ALoc, ALoc>]) -> bool {
    properties.iter().any(|prop| match prop {
        ast::types::object::Property::CallProperty(cp) => !cp.static_,
        ast::types::object::Property::NormalProperty(np) => is_construct_signature(np),
        _ => false,
    })
}

/// Lower an inline `interface { ... }` body to its [InstanceT]. Also used for
/// object types that carry a construct signature: [ObjT] has no slot to hold one
/// (only [class_types::Class] builds [inst_construct_t]), so those are lowered
/// through the interface path too. Callers pass an `env` whose `this` tparam has
/// already been removed, and are responsible for building the typed AST node
/// that matches their own syntax.
fn mk_inline_interface_type<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    loc: &ALoc,
    reason: Reason,
    extends: Vec<((ALoc, Type, Option<Vec<Type>>), ClassLikeBindingKind)>,
    properties: &[ast::types::object::Property<ALoc, ALoc>],
) -> Result<
    (Type, Vec<ast::types::object::Property<ALoc, (ALoc, Type)>>),
    flow_utils_concurrency::job_error::JobError,
> {
    let id = cx.make_aloc_id(loc);
    let (extends_types, extends_binding_kinds): (Vec<_>, Vec<_>) = extends.into_iter().unzip();
    let (this_tparam, this) =
        class_sig::mk_this(type_::mixed_t::make(reason.dupe()), cx, reason.dupe());
    env.tparams_map.insert(
        SubstName::name(FlowSmolStr::new_inline("this")),
        this.dupe(),
    );
    let this_t = Some(this.dupe());
    let super_ = func_class_sig_types::class::Super::Interface(
        func_class_sig_types::class::InterfaceSuper {
            inline: true,
            extends: extends_types,
            extends_binding_kinds,
            function_like: is_function_like(properties),
            this_tparam: Some(this_tparam),
            this_t,
        },
    );
    let iface_sig = class_sig::empty(
        id,
        None,
        loc.dupe(),
        reason.dupe(),
        None,
        env.tparams_map
            .iter()
            .map(|(k, v)| (k.dupe(), v.dupe()))
            .collect(),
        super_,
    );
    let (iface_sig, property_asts) = add_interface_properties(
        cx,
        env,
        None,
        false,
        intermediate_error_types::ObjKind::Interface,
        this,
        properties,
        iface_sig,
    )?;
    class_sig::check_signature_compatibility(cx, reason, &iface_sig);
    Ok((class_sig::thistype(cx, &iface_sig), property_asts))
}

/// The value type of a bracketed member of an interface or a `declare class`
/// body, and its typed node, whichever way its key is read. The key is taken
/// already converted, since a computed key is converted as a value while an
/// index signature key is converted as a type.
/// `named` is the one property the key names, or `None` for an index signature,
/// which stands for every key it matches and so may not carry a bare `unique
/// symbol`. A symbol key has no name to render the value after, hence
/// `as_smol_str_opt`.
fn convert_indexer_value(
    cx: &mut Context,
    env: &mut ConvertEnv,
    indexer: &ast::types::object::Indexer<ALoc, ALoc>,
    key_ast: ast::types::Type<ALoc, (ALoc, Type)>,
    named: Option<&Name>,
) -> Result<
    (Type, ast::types::object::Indexer<ALoc, (ALoc, Type)>),
    flow_utils_concurrency::job_error::JobError,
> {
    let value_ast = match named {
        Some(name) => {
            convert_single_binding_value(cx, env, &indexer.value, name.as_smol_str_opt())?
        }
        None => convert_inner(cx, env, &indexer.value)?,
    };
    let (annot_loc, v) = value_ast.loc();
    let annot_loc = annot_loc.dupe();
    let v = v.dupe();
    let v = if indexer.optional {
        type_util::optional(v, Some(annot_loc), false)
    } else {
        v
    };
    let indexer_ast = ast::types::object::Indexer {
        loc: indexer.loc.dupe(),
        id: indexer.id.as_ref().map(|id| {
            ast::Identifier::new(ast::IdentifierInner {
                loc: (id.loc.dupe(), v.dupe()),
                name: id.name.dupe(),
                comments: id.comments.dupe(),
            })
        }),
        key: key_ast,
        value: value_ast,
        static_: indexer.static_,
        variance: indexer.variance.clone(),
        optional: indexer.optional,
        comments: indexer.comments.dupe(),
    };
    Ok((v, indexer_ast))
}

fn convert_indexer_internal(
    cx: &mut Context,
    env: &mut ConvertEnv,
    indexer: &ast::types::object::Indexer<ALoc, ALoc>,
) -> Result<
    (
        type_::DictType,
        ast::types::object::Indexer<ALoc, (ALoc, Type)>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    let key_ast = convert_inner(cx, env, &indexer.key)?;
    let (_, k) = key_ast.loc();
    let k = k.dupe();
    // Always a dictionary, never one named member.
    let (v, indexer_ast) = convert_indexer_value(cx, env, indexer, key_ast, None)?;
    let p = polarity(
        cx,
        flow_typing_errors::intermediate_error_types::VarianceSigilParent::Property,
        indexer.variance.as_ref(),
    );
    let dict = type_::DictType {
        dict_name: indexer.id.as_ref().map(|id| id.name.dupe()),
        key: k,
        value: v,
        dict_polarity: p,
    };
    Ok((dict, indexer_ast))
}

/// How a bracketed key in an interface or a `declare class` body is read. The
/// reading only depends on the way the key is written and on the kind of
/// binding its head name has, so it is settled before anything is converted,
/// and the gates that only an index signature has to pass still run first.
enum InterfaceKeyForm<'a> {
    /// A computed key naming the property the literal spells.
    Literal(FlowSmolStr),
    /// A computed key written as the name of a value, read as that value's
    /// type. It names a property when that type is a single literal.
    ValueName {
        loc: &'a ALoc,
        generic: &'a ast::types::Generic<ALoc, ALoc>,
    },
    IndexSignature,
}

fn interface_key_form<'a>(
    cx: &mut Context,
    indexer: &'a ast::types::object::Indexer<ALoc, ALoc>,
) -> InterfaceKeyForm<'a> {
    // The labeled form `[label: K]` is an index signature outright.
    if indexer.id.is_some() {
        return InterfaceKeyForm::IndexSignature;
    }
    match object_type_key::object_type_key_form(&indexer.key) {
        ObjectTypeKeyForm::Literal(name) => InterfaceKeyForm::Literal(name),
        ObjectTypeKeyForm::Name { loc, generic, head }
            if type_env::heads_computed_key(cx, &head.loc) =>
        {
            InterfaceKeyForm::ValueName { loc, generic }
        }
        ObjectTypeKeyForm::Name { .. } | ObjectTypeKeyForm::IndexSignature => {
            InterfaceKeyForm::IndexSignature
        }
    }
}

/// What a computed key has to lose the name to, from
/// [`interface_shadowing_member_last_locs`]. `members` covers methods and
/// accessors, which is what a key adding a field loses to. `accessors` narrows
/// that to accessors alone: a key adding a method does not lose to a later
/// method, since the two become overloads of one name in both pipelines, but it
/// does lose to a later accessor, which cannot share a name with a method.
struct ShadowingMemberLastLocs {
    members: HashMap<(bool, Name), ALoc>,
    accessors: HashMap<(bool, Name), ALoc>,
}

/// The last key location of each method and accessor in an interface or
/// `declare class` body, by the side it sits on and the name it spells. The
/// signature pipeline stores every member of one name as a single entry
/// ordered by that location, so a computed key has to lose to a name that goes
/// on being written after it, whether that is the other half of a getter and
/// setter pair or a further overload of a method. A plain property needs no
/// entry: it is one member with one location, so folding in source order
/// already puts it on the same side of the key in both pipelines. Unlike an
/// object type body, this one names a member by a quoted or numeric key as
/// well, so all three spellings are collected here.
///
/// Only a member that shares a map with the property a computed key adds can
/// take the name from it. An interface body and the static side of a
/// `declare class` keep one map, so a method or an accessor there counts. The
/// instance side of a `declare class` keeps its fields apart from its methods
/// and accessors, with a field shadowing whatever the proto side holds, so
/// nothing there does.
fn interface_shadowing_member_last_locs(
    obj_kind: &intermediate_error_types::ObjKind,
    properties: &[ast::types::object::Property<ALoc, ALoc>],
) -> ShadowingMemberLastLocs {
    use ast::types::object::Property;
    use ast::types::object::PropertyValue;
    use flow_parser::ast::expression::object::Key;

    fn member_key(key: &ast::expression::object::Key<ALoc, ALoc>) -> Option<(Name, &ALoc)> {
        match key {
            Key::Identifier(id) => Some((Name::new(id.name.dupe()), &id.loc)),
            Key::StringLiteral((loc, lit)) => Some((Name::new(lit.value.dupe()), loc)),
            Key::NumberLiteral((loc, lit))
                if flow_common::js_number::is_float_safe_integer(lit.value) =>
            {
                let name = flow_common::js_number::ecma_string_of_float(lit.value);
                Some((Name::new(FlowSmolStr::new(name)), loc))
            }
            _ => None,
        }
    }

    fn record(last_locs: &mut HashMap<(bool, Name), ALoc>, key: (bool, Name), key_loc: &ALoc) {
        last_locs
            .entry(key)
            .and_modify(|last| {
                if key_loc.quick_compare(last).is_gt() {
                    *last = key_loc.dupe();
                }
            })
            .or_insert_with(|| key_loc.dupe());
    }

    let mut out = ShadowingMemberLastLocs {
        members: HashMap::new(),
        accessors: HashMap::new(),
    };
    for prop in properties {
        let Property::NormalProperty(p) = prop else {
            continue;
        };
        let shares_map = p.static_ || *obj_kind == intermediate_error_types::ObjKind::Interface;
        let is_accessor = matches!(p.value, PropertyValue::Get(..) | PropertyValue::Set(..));
        let shadows = shares_map && (p.method || is_accessor);
        if !shadows {
            continue;
        }
        let Some((name, key_loc)) = member_key(&p.key) else {
            continue;
        };
        record(&mut out.members, (p.static_, name.dupe()), key_loc);
        if is_accessor {
            record(&mut out.accessors, (p.static_, name), key_loc);
        }
    }
    out
}

/// Whether the signature pipeline only learns this computed key's name at
/// merge, so it appends the member after every member the body names outright.
/// A literal key, and the well-known-symbol spelling that reads as one, is
/// folded there by name in source order instead, exactly as it is here.
fn computed_key_appends_at_merge(expr: &ast::expression::Expression<ALoc, ALoc>) -> bool {
    use ast::expression::ExpressionInner;

    if flow_parser::ast_utils::well_known_symbol_name(expr).is_some() {
        return false;
    }
    matches!(
        &*expr.0,
        ExpressionInner::Identifier { .. } | ExpressionInner::Member { .. }
    )
}

/// A computed-key method held back from the class signature until the body's
/// own names are in, so its overload lands where the signature pipeline puts
/// it. See [`computed_key_appends_at_merge`].
struct DeferredMethod {
    static_: bool,
    abstract_: bool,
    override_: bool,
    name: Name,
    id_loc: ALoc,
    func_sig: func_class_sig_types::func::Func<FuncTypeParamsConfig>,
}

impl DeferredMethod {
    fn apply(self, s: &mut func_class_sig_types::class::Class<FuncTypeParamsConfig>) {
        class_sig::append_method(
            self.static_,
            self.abstract_,
            self.override_,
            self.name,
            self.id_loc,
            None,
            false,
            self.func_sig,
            None,
            None,
            s,
        );
    }
}

fn add_interface_properties<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    record_for_interface: Option<ALoc>,
    enclosing_class_abstract: bool,
    obj_kind: intermediate_error_types::ObjKind,
    this: Type,
    properties: &[ast::types::object::Property<ALoc, ALoc>],
    mut s: func_class_sig_types::class::Class<FuncTypeParamsConfig>,
) -> Result<
    (
        func_class_sig_types::class::Class<FuncTypeParamsConfig>,
        Vec<ast::types::object::Property<ALoc, (ALoc, Type)>>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    let record_field = |cx: &mut Context<'a>, name: &Name, t: &Type| {
        if let Some(id_loc) = &record_for_interface {
            cx.record_interface_field(id_loc.dupe(), name.dupe(), t.dupe());
        }
    };
    use flow_parser::ast::types::object::Property;

    let mut prop_asts: Vec<Property<ALoc, (ALoc, Type)>> = Vec::new();
    let shadowing_member_last_locs = interface_shadowing_member_last_locs(&obj_kind, properties);
    let mut deferred_methods: Vec<DeferredMethod> = Vec::new();
    for prop in properties {
        match prop {
            Property::CallProperty(cp) => {
                let (t, value_ast) = mk_function_type_annotation_inner(
                    cx,
                    env,
                    &(cp.value.0.dupe(), cp.value.1.clone()),
                )?;
                class_sig::append_call(cp.static_, t, &mut s);
                prop_asts.push(Property::CallProperty(ast::types::object::CallProperty {
                    loc: cp.loc.dupe(),
                    value: value_ast,
                    static_: cp.static_,
                    comments: cp.comments.dupe(),
                }));
            }
            Property::Indexer(idx) => {
                // `[K]: V` is ambiguous between a computed key and an index
                // signature, and the way the key is written decides (see
                // `object_type_key`). Classify before the two gates below, so a
                // computed key is neither counted as an indexer nor rejected
                // for carrying a `?`, which is TSLib-only syntax on an indexer.
                let key_form = interface_key_form(cx, idx);
                let is_index_signature = matches!(key_form, InterfaceKeyForm::IndexSignature);
                if is_index_signature && class_sig::has_indexer(idx.static_, &s) {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EUnsupportedSyntax(Box::new((
                            idx.loc.dupe(),
                            intermediate_error_types::UnsupportedSyntax::MultipleIndexers,
                        ))),
                    );
                    let Ok(error_prop) = polymorphic_ast_mapper::object_type_property(
                        &mut typed_ast_utils::ErrorMapper,
                        prop,
                    );
                    prop_asts.push(error_prop);
                    continue;
                }
                if is_index_signature && idx.optional && !cx.ts_syntax() {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EUnsupportedSyntax(Box::new((
                            idx.loc.dupe(),
                            intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                                intermediate_error_types::TsLibSyntaxKind::OptionalIndexer,
                            ),
                        ))),
                    );
                    let Ok(error_prop) = polymorphic_ast_mapper::object_type_property(
                        &mut typed_ast_utils::ErrorMapper,
                        prop,
                    );
                    prop_asts.push(error_prop);
                    continue;
                }
                let (key_ast, named) = match key_form {
                    InterfaceKeyForm::Literal(name) => {
                        (convert_inner(cx, env, &idx.key)?, Some(Name::new(name)))
                    }
                    InterfaceKeyForm::ValueName { loc, generic } => {
                        let (key_ast, name) = convert_value_object_type_key(cx, loc, generic)?;
                        match name {
                            // Every member of one name is a single member in
                            // the signature pipeline, ordered by the last of
                            // them, so a name still being written after this
                            // key wins over it and the key adds nothing. Only a
                            // value name needs this: a literal key is folded
                            // there by name in source order, so it splits a
                            // getter and setter pair there as it does here.
                            Some(name)
                                if shadowing_member_last_locs
                                    .members
                                    .get(&(idx.static_, name.dupe()))
                                    .is_some_and(|last| {
                                        last.quick_compare(idx.key.loc()).is_gt()
                                    }) =>
                            {
                                let Ok(error_prop) = polymorphic_ast_mapper::object_type_property(
                                    &mut typed_ast_utils::ErrorMapper,
                                    prop,
                                );
                                prop_asts.push(error_prop);
                                continue;
                            }
                            Some(name) => (key_ast, Some(name)),
                            // The value names no one property, so there is no
                            // member to add. An index signature is not a sound
                            // reading either: the author asked for one key.
                            None => {
                                let key_error_kind = if key_is_type_name(cx, &key_ast) {
                                    InvalidObjKey::ComputedTypeName
                                } else {
                                    InvalidObjKey::ComputedNotLiteral
                                };
                                flow_js_utils::add_output_non_speculating(
                                    cx,
                                    ErrorMessage::EUnsupportedKeyInObject {
                                        loc: idx.key.loc().dupe(),
                                        obj_kind: obj_kind.clone(),
                                        key_error_kind,
                                    },
                                );
                                let Ok(error_prop) = polymorphic_ast_mapper::object_type_property(
                                    &mut typed_ast_utils::ErrorMapper,
                                    prop,
                                );
                                prop_asts.push(error_prop);
                                continue;
                            }
                        }
                    }
                    InterfaceKeyForm::IndexSignature => (convert_inner(cx, env, &idx.key)?, None),
                };
                let key_t = key_ast.loc().1.dupe();
                // A key that names one member is subject to the same
                // `unique symbol` rule as a written one.
                if named.is_some() {
                    check_unique_symbol_member(
                        cx,
                        &idx.value,
                        &obj_kind,
                        idx.static_,
                        typed_ast_utils::polarity(idx.variance.as_ref()),
                    );
                }
                let (value_t, indexer_ast) =
                    convert_indexer_value(cx, env, idx, key_ast, named.as_ref())?;
                let prop_polarity = polarity(
                    cx,
                    flow_typing_errors::intermediate_error_types::VarianceSigilParent::Property,
                    idx.variance.as_ref(),
                );
                match named {
                    Some(name) => {
                        // Keep the full `Name` so a `unique symbol` indexer key
                        // retains its nominal identity in the (now `Name`-keyed)
                        // class Signature, matching the computed-key path.
                        record_field(cx, &name, &value_t);
                        class_sig::add_field(
                            idx.static_,
                            false,
                            false,
                            name,
                            idx.key.loc().dupe(),
                            prop_polarity,
                            func_class_sig_types::class::Field::Annot(value_t),
                            &mut s,
                        );
                    }
                    None => {
                        let dict = type_::DictType {
                            dict_name: idx.id.as_ref().map(|id| id.name.dupe()),
                            key: key_t,
                            value: value_t,
                            dict_polarity: prop_polarity,
                        };
                        class_sig::add_indexer(idx.static_, dict, &mut s);
                    }
                }
                prop_asts.push(Property::Indexer(indexer_ast));
            }
            Property::MappedType(mt) => {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EInvalidMappedType {
                        loc: mt.loc.dupe(),
                        kind: flow_typing_errors::error_message::InvalidMappedTypeErrorKind::InterfaceOrDeclaredClass,
                    },
                );
                let Ok(error_prop) = polymorphic_ast_mapper::object_type_property(
                    &mut typed_ast_utils::ErrorMapper,
                    prop,
                );
                prop_asts.push(error_prop);
            }
            Property::NormalProperty(np) => {
                // The [override] modifier is only meaningful on classes /
                // [declare class]. For pure object-type / interface bodies
                // (the [ObjKind::Interface] case) it has no TS-aligned semantics, so
                // keep rejecting it unconditionally. For [ObjKind::DeclareClass]
                // under the [tslib_syntax] gate, thread it through to the
                // same per-side [override_members] tracking that regular
                // classes use; [check_override_obligations] will fire any
                // errors. With the gate off, fall back to the
                // [EUnsupportedSyntax] rejection (matches what regular
                // classes do via [statement.rs]).
                let override_on_declare_class = np.override_
                    && obj_kind == intermediate_error_types::ObjKind::DeclareClass
                    && cx.tslib_syntax();
                if np.override_ && !override_on_declare_class {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EUnsupportedSyntax(Box::new((
                            np.loc.dupe(),
                            intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                                TsLibSyntaxKind::OverrideModifier,
                            ),
                        ))),
                    );
                }
                let init_ = &np.init;
                let is_ts_private = match &np.ts_accessibility {
                    Some(acc) => {
                        if !cx.ts_syntax() {
                            flow_js_utils::add_output_non_speculating(
                                cx,
                                ErrorMessage::ETSSyntax(Box::new(ETSSyntaxData {
                                    kind: flow_typing_errors::error_message::TSSyntaxKind::TSClassAccessibility(acc.kind),
                                    loc: acc.loc.dupe(),
                                })),
                            );
                        }
                        matches!(
                            acc.kind,
                            flow_parser::ast::class::ts_accessibility::Kind::Private
                        )
                    }
                    None => false,
                };
                let is_literal_init = |expr: &ast::expression::Expression<ALoc, ALoc>| -> bool {
                    match &*expr.0 {
                        ast::expression::ExpressionInner::StringLiteral { .. }
                        | ast::expression::ExpressionInner::NumberLiteral { .. }
                        | ast::expression::ExpressionInner::BigIntLiteral { .. }
                        | ast::expression::ExpressionInner::BooleanLiteral { .. } => true,
                        ast::expression::ExpressionInner::Unary { inner, .. }
                            if inner.operator == ast::expression::UnaryOperator::Minus =>
                        {
                            matches!(
                                &*inner.argument.0,
                                ast::expression::ExpressionInner::NumberLiteral { .. }
                                    | ast::expression::ExpressionInner::BigIntLiteral { .. }
                            )
                        }
                        _ => false,
                    }
                };
                match (init_, &np.value) {
                    (Some(_), _) if !cx.ts_syntax() => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EUnsupportedSyntax(Box::new((
                                np.loc.dupe(),
                                intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                                    TsLibSyntaxKind::PropertyValueInitializer,
                                ),
                            ))),
                        );
                    }
                    (Some(init_expr), ast::types::object::PropertyValue::Init(Some(_))) => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EUnsupportedSyntax(Box::new((
                                init_expr.loc().dupe(),
                                intermediate_error_types::UnsupportedSyntax::DeclareClassProperty(
                                    intermediate_error_types::DeclareClassPropKind::AnnotationAndInit,
                                ),
                            ))),
                        );
                    }
                    (Some(init_expr), ast::types::object::PropertyValue::Init(None)) => {
                        let is_readonly = matches!(
                            &np.variance,
                            Some(ast::Variance {
                                kind: ast::VarianceKind::Readonly | ast::VarianceKind::Plus,
                                ..
                            })
                        );
                        if !is_readonly {
                            flow_js_utils::add_output_non_speculating(
                                cx,
                                ErrorMessage::EUnsupportedSyntax(Box::new((
                                    init_expr.loc().dupe(),
                                    intermediate_error_types::UnsupportedSyntax::DeclareClassProperty(
                                        intermediate_error_types::DeclareClassPropKind::InitWithoutReadonly,
                                    ),
                                ))),
                            );
                        }
                        if !is_literal_init(init_expr) {
                            flow_js_utils::add_output_non_speculating(
                                cx,
                                ErrorMessage::EUnsupportedSyntax(Box::new((
                                    init_expr.loc().dupe(),
                                    intermediate_error_types::UnsupportedSyntax::DeclareClassProperty(
                                        intermediate_error_types::DeclareClassPropKind::NonLiteralInit,
                                    ),
                                ))),
                            );
                        }
                    }
                    (None, ast::types::object::PropertyValue::Init(None)) if !is_ts_private => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EUnsupportedSyntax(Box::new((
                                np.loc.dupe(),
                                intermediate_error_types::UnsupportedSyntax::DeclareClassProperty(
                                    intermediate_error_types::DeclareClassPropKind::MissingAnnotationOrInit,
                                ),
                            ))),
                        );
                    }
                    _ => {}
                }
                if is_ts_private {
                    // Private members are excluded from the declare class's public
                    // interface, so we skip adding them to the class signature.
                    let Ok(error_prop) = polymorphic_ast_mapper::object_type_property(
                        &mut typed_ast_utils::UncheckedMapper,
                        prop,
                    );
                    prop_asts.push(error_prop);
                } else {
                    let abstract_on = np.abstract_ && cx.tslib_syntax();
                    if np.abstract_ && !cx.tslib_syntax() {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::ETSSyntax(Box::new(ETSSyntaxData {
                                kind:
                                    flow_typing_errors::error_message::TSSyntaxKind::AbstractMethod,
                                loc: np.loc.dupe(),
                            })),
                        );
                    }
                    if abstract_on && !enclosing_class_abstract {
                        let member_name = match &np.key {
                            ast::expression::object::Key::Identifier(id) => id.name.dupe(),
                            ast::expression::object::Key::StringLiteral((_, str_lit)) => {
                                str_lit.value.dupe()
                            }
                            _ => FlowSmolStr::new("<unknown>"),
                        };
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EAbstractClass(Box::new(
                                flow_typing_errors::error_message::EAbstractClassData {
                                    kind:
                                        flow_typing_errors::intermediate_error_types::AbstractErrorKind::AbstractMemberOnNonAbstractClass {
                                            member_name,
                                        },
                                    loc: np.loc.dupe(),
                                },
                            )),
                        );
                    }
                    if np.optional && np.method && !cx.tslib_syntax() {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EUnsupportedSyntax(Box::new((
                                np.loc.dupe(),
                                intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                                    TsLibSyntaxKind::OptionalShorthandMethod,
                                ),
                            ))),
                        );
                        let Ok(error_prop) = polymorphic_ast_mapper::object_type_property(
                            &mut typed_ast_utils::ErrorMapper,
                            prop,
                        );
                        prop_asts.push(error_prop);
                        continue;
                    }
                    use flow_parser::ast::expression::object::Key;
                    let ambiguous_numeric_key = matches!(
                        (&np.key, np.variance.as_ref()),
                        (
                            Key::NumberLiteral(_),
                            Some(ast::Variance {
                                kind: ast::VarianceKind::Plus | ast::VarianceKind::Minus,
                                ..
                            })
                        )
                    );
                    let optional_method = np.optional
                        && (cx.is_global_lib_context()
                            || cx.type_strictness_kind().is_typescript_loose());
                    let polarity = if ambiguous_numeric_key {
                        typed_ast_utils::polarity(np.variance.as_ref())
                    } else {
                        polarity(
                            cx,
                            flow_typing_errors::intermediate_error_types::VarianceSigilParent::Property,
                            np.variance.as_ref(),
                        )
                    };
                    let add_field_or_proto = |cx: &mut Context<'a>,
                                              name: Name,
                                              id_loc: ALoc,
                                              t: Type,
                                              s: &mut func_class_sig_types::class::Class<
                        FuncTypeParamsConfig,
                    >| {
                        if np.proto {
                            class_sig::add_proto_field(
                                abstract_on,
                                override_on_declare_class,
                                name,
                                id_loc,
                                polarity,
                                func_class_sig_types::class::Field::Annot(t),
                                s,
                            );
                        } else {
                            record_field(cx, &name, &t);
                            class_sig::add_field(
                                np.static_,
                                abstract_on,
                                override_on_declare_class,
                                name,
                                id_loc,
                                polarity,
                                func_class_sig_types::class::Field::Annot(t),
                                s,
                            );
                        }
                    };
                    let mut handle_init_only_property =
                        |cx: &mut Context<'a>,
                         name: Name,
                         id_loc: ALoc,
                         rebuild_key: &dyn Fn(Type) -> ast::expression::object::Key<ALoc, (ALoc, Type)>|
                         -> Option<ast::types::object::Property<ALoc, (ALoc, Type)>> {
                            match init_ {
                                Some(init_expr)
                                    if cx.ts_syntax() && is_literal_init(init_expr) =>
                                {
                                    let init_ast = crate::statement::expression(
                                        None,
                                        None,
                                        Some(true),
                                        cx,
                                        init_expr,
                                    )
                                    .unwrap();
                                    let t = init_ast.loc().1.dupe();
                                    let t = if np.optional {
                                        type_util::optional(t, None, false)
                                    } else {
                                        t
                                    };
                                    add_field_or_proto(
                                        cx,
                                        name.dupe(),
                                        id_loc.dupe(),
                                        t.dupe(),
                                        &mut s,
                                    );
                                    Some(Property::NormalProperty(
                                        ast::types::object::NormalProperty {
                                            loc: np.loc.dupe(),
                                            key: rebuild_key(t),
                                            value: ast::types::object::PropertyValue::Init(None),
                                            optional: np.optional,
                                            static_: np.static_,
                                            proto: np.proto,
                                            method: np.method,
                                            abstract_: np.abstract_,
                                            override_: np.override_,
                                            variance: np.variance.clone(),
                                            ts_accessibility: np.ts_accessibility.clone(),
                                            init: Some(init_ast),
                                            comments: np.comments.dupe(),
                                        },
                                    ))
                                }
                                _ => None,
                            }
                        };
                    match &np.key {
                        Key::BigIntLiteral((loc, _)) => {
                            flow_js_utils::add_output_non_speculating(
                                cx,
                                ErrorMessage::EUnsupportedSyntax(Box::new((
                                    loc.dupe(),
                                    intermediate_error_types::UnsupportedSyntax::IllegalName,
                                ))),
                            );
                            let Ok(error_prop) = polymorphic_ast_mapper::object_property_type(
                                &mut typed_ast_utils::ErrorMapper,
                                np,
                            );
                            prop_asts.push(Property::NormalProperty(error_prop));
                            continue;
                        }
                        Key::PrivateName(pn) => {
                            flow_js_utils::add_output_non_speculating(
                                cx,
                                ErrorMessage::EUnsupportedSyntax(Box::new((
                                    pn.loc.dupe(),
                                    intermediate_error_types::UnsupportedSyntax::IllegalName,
                                ))),
                            );
                            let Ok(error_prop) = polymorphic_ast_mapper::object_property_type(
                                &mut typed_ast_utils::ErrorMapper,
                                np,
                            );
                            prop_asts.push(Property::NormalProperty(error_prop));
                            continue;
                        }
                        Key::Computed(ck) => {
                            let (typed_expr, resolved_name) =
                                resolve_computed_key_name(cx, &ck.expression)?;
                            match resolved_name {
                                Some(name) => {
                                    // Keep the full `Name` so a `unique symbol` computed
                                    // key retains its nominal identity in the (now
                                    // `Name`-keyed) class Signature.
                                    let id_loc = ck.expression.loc().dupe();
                                    if np.method {
                                        match &np.value {
                                            ast::types::object::PropertyValue::Init(Some(
                                                value,
                                            )) if let ast::types::TypeInner::Function {
                                                loc: func_loc,
                                                inner: func,
                                            } = &**value =>
                                            {
                                                let meth_kind = MethodKind::MethodKind {
                                                    is_static: np.static_,
                                                };
                                                let (fsig, func_ast) = mk_method_func_sig(
                                                    cx,
                                                    env,
                                                    meth_kind,
                                                    np.loc.dupe(),
                                                    func,
                                                )?;
                                                let this_write_loc = None;
                                                let ft = crate::func_sig::methodtype(
                                                    cx,
                                                    this_write_loc,
                                                    if optional_method {
                                                        type_::mixed_t::make(
                                                            type_util::reason_of_t(&this).dupe(),
                                                        )
                                                    } else {
                                                        this.dupe()
                                                    },
                                                    &fsig,
                                                );
                                                if optional_method {
                                                    add_field_or_proto(
                                                        cx,
                                                        name.dupe(),
                                                        id_loc.dupe(),
                                                        type_util::optional(ft.dupe(), None, false),
                                                        &mut s,
                                                    );
                                                } else {
                                                    let append = DeferredMethod {
                                                        static_: np.static_,
                                                        abstract_: abstract_on,
                                                        override_: override_on_declare_class,
                                                        name: name.dupe(),
                                                        id_loc: id_loc.dupe(),
                                                        func_sig: fsig,
                                                    };
                                                    if !computed_key_appends_at_merge(
                                                        &ck.expression,
                                                    ) {
                                                        append.apply(&mut s);
                                                    } else if !shadowing_member_last_locs
                                                        .accessors
                                                        .get(&(np.static_, name.dupe()))
                                                        .is_some_and(|last| {
                                                            last.quick_compare(&id_loc).is_gt()
                                                        })
                                                    {
                                                        deferred_methods.push(append);
                                                    }
                                                }
                                                prop_asts.push(Property::NormalProperty(
                                                    ast::types::object::NormalProperty {
                                                        loc: np.loc.dupe(),
                                                        key: Key::Computed(ast::ComputedKey {
                                                            loc: ck.loc.dupe(),
                                                            expression: typed_expr,
                                                            comments: ck.comments.dupe(),
                                                        }),
                                                        value:
                                                            ast::types::object::PropertyValue::Init(
                                                                Some(ast::types::Type::new(
                                                                    ast::types::TypeInner::Function {
                                                                        loc: (func_loc.dupe(), ft),
                                                                        inner: func_ast.into(),
                                                                    },
                                                                )),
                                                            ),
                                                        optional: np.optional,
                                                        static_: np.static_,
                                                        proto: np.proto,
                                                        method: np.method,
                                                        abstract_: np.abstract_,
                                                        override_: np.override_,
                                                        variance: np.variance.clone(),
                                                        ts_accessibility: np.ts_accessibility.clone(),
                                                        init: None,
                                                        comments: np.comments.dupe(),
                                                    },
                                                ));
                                            }
                                            _ => {
                                                flow_js_utils::add_output_non_speculating(
                                                    cx,
                                                    ErrorMessage::EUnsupportedSyntax(Box::new((
                                                        ck.loc.dupe(),
                                                        intermediate_error_types::UnsupportedSyntax::IllegalName,
                                                    ))),
                                                );
                                                let Ok(error_prop) =
                                                    polymorphic_ast_mapper::object_property_type(
                                                        &mut typed_ast_utils::ErrorMapper,
                                                        np,
                                                    );
                                                prop_asts
                                                    .push(Property::NormalProperty(error_prop));
                                            }
                                        }
                                    } else {
                                        match &np.value {
                                            ast::types::object::PropertyValue::Init(Some(
                                                value,
                                            )) => {
                                                // A computed key names one property just
                                                // as a written one does, so the same
                                                // `unique symbol` rule applies. A symbol
                                                // key has no name to render the value
                                                // after, hence `as_smol_str_opt`.
                                                check_unique_symbol_member(
                                                    cx,
                                                    value,
                                                    &obj_kind,
                                                    np.static_,
                                                    typed_ast_utils::polarity(np.variance.as_ref()),
                                                );
                                                let value_ast = convert_single_binding_value(
                                                    cx,
                                                    env,
                                                    value,
                                                    name.as_smol_str_opt(),
                                                )?;
                                                let (_, t) = value_ast.loc();
                                                let t = t.dupe();
                                                let t_with_optional = if np.optional {
                                                    type_util::optional(t.dupe(), None, false)
                                                } else {
                                                    t.dupe()
                                                };
                                                add_field_or_proto(
                                                    cx,
                                                    name.dupe(),
                                                    id_loc.dupe(),
                                                    t_with_optional.dupe(),
                                                    &mut s,
                                                );
                                                let init_ast = init_.as_ref().map(|init_expr| {
                                                    crate::statement::expression(
                                                        None,
                                                        None,
                                                        Some(true),
                                                        cx,
                                                        init_expr,
                                                    )
                                                    .unwrap()
                                                });
                                                prop_asts.push(Property::NormalProperty(
                                                    ast::types::object::NormalProperty {
                                                        loc: np.loc.dupe(),
                                                        key: Key::Computed(ast::ComputedKey {
                                                            loc: ck.loc.dupe(),
                                                            expression: typed_expr,
                                                            comments: ck.comments.dupe(),
                                                        }),
                                                        value:
                                                            ast::types::object::PropertyValue::Init(
                                                                Some(value_ast),
                                                            ),
                                                        optional: np.optional,
                                                        static_: np.static_,
                                                        proto: np.proto,
                                                        method: np.method,
                                                        abstract_: np.abstract_,
                                                        override_: np.override_,
                                                        variance: np.variance.clone(),
                                                        ts_accessibility: np
                                                            .ts_accessibility
                                                            .clone(),
                                                        init: init_ast,
                                                        comments: np.comments.dupe(),
                                                    },
                                                ));
                                            }
                                            ast::types::object::PropertyValue::Init(None) => {
                                                let typed_expr_clone = typed_expr.clone();
                                                let ck_loc = ck.loc.dupe();
                                                let ck_comments = ck.comments.dupe();
                                                if let Some(prop_ast) = handle_init_only_property(
                                                    cx,
                                                    name.dupe(),
                                                    id_loc.dupe(),
                                                    &|_t| {
                                                        Key::Computed(ast::ComputedKey {
                                                            loc: ck_loc.dupe(),
                                                            expression: typed_expr_clone.clone(),
                                                            comments: ck_comments.dupe(),
                                                        })
                                                    },
                                                ) {
                                                    prop_asts.push(prop_ast);
                                                } else {
                                                    let Ok(error_prop) =
                                                        polymorphic_ast_mapper::object_property_type(
                                                            &mut typed_ast_utils::ErrorMapper,
                                                            np,
                                                        );
                                                    prop_asts
                                                        .push(Property::NormalProperty(error_prop));
                                                }
                                            }
                                            _ => {
                                                flow_js_utils::add_output_non_speculating(
                                                    cx,
                                                    ErrorMessage::EUnsupportedSyntax(Box::new((
                                                        ck.loc.dupe(),
                                                        intermediate_error_types::UnsupportedSyntax::IllegalName,
                                                    ))),
                                                );
                                                let Ok(error_prop) =
                                                    polymorphic_ast_mapper::object_property_type(
                                                        &mut typed_ast_utils::ErrorMapper,
                                                        np,
                                                    );
                                                prop_asts
                                                    .push(Property::NormalProperty(error_prop));
                                            }
                                        }
                                    }
                                }
                                None => {
                                    flow_js_utils::add_output_non_speculating(
                                        cx,
                                        ErrorMessage::EUnsupportedSyntax(Box::new((
                                            ck.loc.dupe(),
                                            intermediate_error_types::UnsupportedSyntax::IllegalName,
                                        ))),
                                    );
                                    let Ok(error_prop) =
                                        polymorphic_ast_mapper::object_property_type(
                                            &mut typed_ast_utils::ErrorMapper,
                                            np,
                                        );
                                    prop_asts.push(Property::NormalProperty(error_prop));
                                }
                            }
                            continue;
                        }
                        _ => {}
                    };
                    type RebuildKey = Box<dyn Fn(Type) -> Key<ALoc, (ALoc, Type)>>;
                    let resolve_named_key =
                        |cx: &mut Context<'a>,
                         check_variance: bool|
                         -> Option<(FlowSmolStr, ALoc, RebuildKey)> {
                            let mut check_num =
                                |key_loc: ALoc,
                                 num_value: f64,
                                 num_lit: ast::NumberLiteral<ALoc>|
                                 -> Option<(FlowSmolStr, ALoc, RebuildKey)> {
                                    if flow_common::js_number::is_float_safe_integer(num_value) {
                                        let name = flow_common::js_number::ecma_string_of_float(
                                            num_value,
                                        );
                                        let key_loc_clone = key_loc.dupe();
                                        Some((
                                            FlowSmolStr::new(&name),
                                            key_loc,
                                            Box::new(move |t| {
                                                Key::NumberLiteral((
                                                    (key_loc_clone.dupe(), t),
                                                    num_lit.clone(),
                                                ))
                                            }),
                                        ))
                                    } else {
                                        flow_js_utils::add_output_non_speculating(
                                            cx,
                                            ErrorMessage::EUnsupportedKeyInObject {
                                                loc: key_loc,
                                                obj_kind: obj_kind.clone(),
                                                key_error_kind:
                                                    InvalidObjKey::kind_of_num_value(num_value),
                                            },
                                        );
                                        None
                                    }
                                };
                            match &np.key {
                                Key::Identifier(id_name) => {
                                    let id_loc = id_name.loc.dupe();
                                    let id_name_clone = id_name.clone();
                                    Some((
                                        id_name.name.dupe(),
                                        id_loc.dupe(),
                                        Box::new(move |t| {
                                            Key::Identifier(ast::Identifier::new(
                                                ast::IdentifierInner {
                                                    loc: (id_loc.dupe(), t),
                                                    name: id_name_clone.name.dupe(),
                                                    comments: id_name_clone.comments.dupe(),
                                                },
                                            ))
                                        }),
                                    ))
                                }
                                Key::StringLiteral((key_loc, str_lit)) => {
                                    let name = str_lit.value.dupe();
                                    let key_loc = key_loc.dupe();
                                    let str_lit_clone = str_lit.clone();
                                    Some((
                                        name,
                                        key_loc.dupe(),
                                        Box::new(move |t| {
                                            Key::StringLiteral((
                                                (key_loc.dupe(), t),
                                                str_lit_clone.clone(),
                                            ))
                                        }),
                                    ))
                                }
                                Key::NumberLiteral((key_loc, num_lit)) => {
                                    if check_variance && ambiguous_numeric_key {
                                        flow_js_utils::add_output_non_speculating(
                                            cx,
                                            ErrorMessage::EAmbiguousNumericKeyWithVariance(
                                                key_loc.dupe(),
                                            ),
                                        );
                                        None
                                    } else {
                                        check_num(key_loc.dupe(), num_lit.value, num_lit.clone())
                                    }
                                }
                                _ => None,
                            }
                        };
                    // method
                    if np.method {
                        match &np.value {
                            ast::types::object::PropertyValue::Init(Some(value))
                                if let ast::types::TypeInner::Function {
                                    loc: func_loc,
                                    inner: func,
                                } = &**value =>
                            {
                                match resolve_named_key(cx, false) {
                                    None => {
                                        let Ok(error_prop) =
                                            polymorphic_ast_mapper::object_property_type(
                                                &mut typed_ast_utils::ErrorMapper,
                                                np,
                                            );
                                        prop_asts.push(Property::NormalProperty(error_prop));
                                    }
                                    Some((name, key_loc, rebuild_key)) => {
                                        // Declare-class methods named `new` are ordinary
                                        // methods, so the shared [is_construct_signature]
                                        // predicate is further restricted to interfaces —
                                        // including the object types that [convert] routes
                                        // here because they carry a construct signature.
                                        let is_construct_sig = is_construct_signature(np)
                                            && obj_kind
                                                == intermediate_error_types::ObjKind::Interface;
                                        let meth_kind = match name.as_str() {
                                            "constructor" => MethodKind::ConstructorKind,
                                            _ => MethodKind::MethodKind {
                                                is_static: np.static_,
                                            },
                                        };
                                        let (fsig, func_ast) = mk_method_func_sig(
                                            cx,
                                            env,
                                            meth_kind,
                                            np.loc.dupe(),
                                            func,
                                        )?;
                                        let this_write_loc = None;
                                        let ft = crate::func_sig::methodtype(
                                            cx,
                                            this_write_loc,
                                            if optional_method {
                                                type_::mixed_t::make(
                                                    type_util::reason_of_t(&this).dupe(),
                                                )
                                            } else {
                                                this.dupe()
                                            },
                                            &fsig,
                                        );
                                        if optional_method {
                                            add_field_or_proto(
                                                cx,
                                                Name::new(name.dupe()),
                                                key_loc.dupe(),
                                                type_util::optional(ft.dupe(), None, false),
                                                &mut s,
                                            );
                                        } else if is_construct_sig {
                                            // Construct sigs are stored on the instance side
                                            // of an interface only. Stored raw; [read_construct_t]
                                            // normalizes on read.
                                            class_sig::append_construct(ft.dupe(), &mut s);
                                        } else {
                                            match (np.static_, &meth_kind) {
                                                (false, MethodKind::ConstructorKind) => {
                                                    class_sig::append_constructor(
                                                        Some(key_loc.dupe()),
                                                        fsig,
                                                        None,
                                                        None,
                                                        &mut s,
                                                    );
                                                }
                                                _ => {
                                                    class_sig::append_method(
                                                        np.static_,
                                                        abstract_on,
                                                        override_on_declare_class,
                                                        Name::new(name.dupe()),
                                                        key_loc.dupe(),
                                                        None,
                                                        false,
                                                        fsig,
                                                        None,
                                                        None,
                                                        &mut s,
                                                    );
                                                }
                                            }
                                        }
                                        prop_asts.push(Property::NormalProperty(
                                            ast::types::object::NormalProperty {
                                                loc: np.loc.dupe(),
                                                key: rebuild_key(ft.dupe()),
                                                value: ast::types::object::PropertyValue::Init(
                                                    Some(ast::types::Type::new(
                                                        ast::types::TypeInner::Function {
                                                            loc: (func_loc.dupe(), ft),
                                                            inner: func_ast.into(),
                                                        },
                                                    )),
                                                ),
                                                optional: np.optional,
                                                static_: np.static_,
                                                proto: np.proto,
                                                method: np.method,
                                                abstract_: np.abstract_,
                                                override_: np.override_,
                                                variance: np.variance.clone(),
                                                ts_accessibility: np.ts_accessibility.clone(),
                                                init: None,
                                                comments: np.comments.dupe(),
                                            },
                                        ));
                                    }
                                }
                            }
                            // method not a function
                            _ => {
                                flow_js_utils::add_output_non_speculating(
                                    cx,
                                    ErrorMessage::EInternal(Box::new((
                                        np.loc.dupe(),
                                        InternalError::MethodNotAFunction,
                                    ))),
                                );
                                let Ok(error_prop) = polymorphic_ast_mapper::object_property_type(
                                    &mut typed_ast_utils::ErrorMapper,
                                    np,
                                );
                                prop_asts.push(Property::NormalProperty(error_prop));
                            }
                        }
                    } else {
                        match &np.value {
                            // field with annotation
                            ast::types::object::PropertyValue::Init(Some(value)) => {
                                match resolve_named_key(cx, true) {
                                    None => {
                                        let Ok(error_prop) =
                                            polymorphic_ast_mapper::object_property_type(
                                                &mut typed_ast_utils::ErrorMapper,
                                                np,
                                            );
                                        prop_asts.push(Property::NormalProperty(error_prop));
                                    }
                                    Some((name, key_loc, rebuild_key)) => {
                                        // A class field must also be `static` to stand for one
                                        // symbol, since an instance field declares one member
                                        // per instance. An interface or object type property
                                        // need only be read-only, matching TypeScript, which
                                        // restricts only classes.
                                        check_unique_symbol_member(
                                            cx,
                                            value,
                                            &obj_kind,
                                            np.static_,
                                            typed_ast_utils::polarity(np.variance.as_ref()),
                                        );
                                        let value_ast = convert_single_binding_value(
                                            cx,
                                            env,
                                            value,
                                            Some(&name),
                                        )?;
                                        let (_, t) = value_ast.loc();
                                        let t = t.dupe();
                                        let t_with_optional = if np.optional {
                                            type_util::optional(t.dupe(), None, false)
                                        } else {
                                            t.dupe()
                                        };
                                        add_field_or_proto(
                                            cx,
                                            Name::new(name.dupe()),
                                            key_loc.dupe(),
                                            t_with_optional.dupe(),
                                            &mut s,
                                        );
                                        let init_ast = init_.as_ref().map(|init_expr| {
                                            crate::statement::expression(
                                                None,
                                                None,
                                                Some(true),
                                                cx,
                                                init_expr,
                                            )
                                            .unwrap()
                                        });
                                        prop_asts.push(Property::NormalProperty(
                                            ast::types::object::NormalProperty {
                                                loc: np.loc.dupe(),
                                                key: rebuild_key(t_with_optional),
                                                value: ast::types::object::PropertyValue::Init(
                                                    Some(value_ast),
                                                ),
                                                optional: np.optional,
                                                static_: np.static_,
                                                proto: np.proto,
                                                method: np.method,
                                                abstract_: np.abstract_,
                                                override_: np.override_,
                                                variance: np.variance.clone(),
                                                ts_accessibility: np.ts_accessibility.clone(),
                                                init: init_ast,
                                                comments: np.comments.dupe(),
                                            },
                                        ));
                                    }
                                }
                            }
                            // field without annotation
                            ast::types::object::PropertyValue::Init(None) => {
                                match resolve_named_key(cx, true) {
                                    None => {
                                        let Ok(error_prop) =
                                            polymorphic_ast_mapper::object_property_type(
                                                &mut typed_ast_utils::ErrorMapper,
                                                np,
                                            );
                                        prop_asts.push(Property::NormalProperty(error_prop));
                                    }
                                    Some((name, key_loc, rebuild_key)) => {
                                        if let Some(prop_ast) = handle_init_only_property(
                                            cx,
                                            Name::new(name),
                                            key_loc,
                                            &*rebuild_key,
                                        ) {
                                            prop_asts.push(prop_ast);
                                        } else {
                                            let Ok(error_prop) =
                                                polymorphic_ast_mapper::object_property_type(
                                                    &mut typed_ast_utils::ErrorMapper,
                                                    np,
                                                );
                                            prop_asts.push(Property::NormalProperty(error_prop));
                                        }
                                    }
                                }
                            }
                            // getter
                            ast::types::object::PropertyValue::Get(get_loc, func) => {
                                match resolve_named_key(cx, false) {
                                    None => {
                                        let Ok(error_prop) =
                                            polymorphic_ast_mapper::object_property_type(
                                                &mut typed_ast_utils::ErrorMapper,
                                                np,
                                            );
                                        prop_asts.push(Property::NormalProperty(error_prop));
                                    }
                                    Some((name, key_loc, rebuild_key)) => {
                                        flow_js_utils::add_output_non_speculating(
                                            cx,
                                            ErrorMessage::EUnsafeGettersSetters(np.loc.dupe()),
                                        );
                                        let (fsig, func_ast) = mk_method_func_sig(
                                            cx,
                                            env,
                                            MethodKind::GetterKind,
                                            np.loc.dupe(),
                                            func,
                                        )?;
                                        let prop_t = type_util::type_t_of_annotated_or_inferred(
                                            &fsig.return_t,
                                        )
                                        .dupe();
                                        class_sig::add_getter(
                                            np.static_,
                                            abstract_on,
                                            override_on_declare_class,
                                            Name::new(name.dupe()),
                                            key_loc.dupe(),
                                            None,
                                            fsig,
                                            None,
                                            None,
                                            &mut s,
                                        );
                                        prop_asts.push(Property::NormalProperty(
                                            ast::types::object::NormalProperty {
                                                loc: np.loc.dupe(),
                                                key: rebuild_key(prop_t),
                                                value: ast::types::object::PropertyValue::Get(
                                                    get_loc.dupe(),
                                                    func_ast,
                                                ),
                                                optional: np.optional,
                                                static_: np.static_,
                                                proto: np.proto,
                                                method: np.method,
                                                abstract_: np.abstract_,
                                                override_: np.override_,
                                                variance: np.variance.clone(),
                                                ts_accessibility: np.ts_accessibility.clone(),
                                                init: None,
                                                comments: np.comments.dupe(),
                                            },
                                        ));
                                    }
                                }
                            }
                            // setter
                            ast::types::object::PropertyValue::Set(set_loc, func) => {
                                match resolve_named_key(cx, false) {
                                    None => {
                                        let Ok(error_prop) =
                                            polymorphic_ast_mapper::object_property_type(
                                                &mut typed_ast_utils::ErrorMapper,
                                                np,
                                            );
                                        prop_asts.push(Property::NormalProperty(error_prop));
                                    }
                                    Some((name, key_loc, rebuild_key)) => {
                                        flow_js_utils::add_output_non_speculating(
                                            cx,
                                            ErrorMessage::EUnsafeGettersSetters(np.loc.dupe()),
                                        );
                                        let (fsig, func_ast) = mk_method_func_sig(
                                            cx,
                                            env,
                                            MethodKind::SetterKind,
                                            np.loc.dupe(),
                                            func,
                                        )?;
                                        let prop_t = if fsig.tparams.is_none() {
                                            let params =
                                                crate::func_params::value::<FuncTypeParamsConfig>(
                                                    &fsig.fparams.params,
                                                );
                                            if params.len() == 1 {
                                                params.into_iter().next().unwrap().1
                                            } else {
                                                type_::any_t::at(
                                                    type_::AnySource::AnyError(None),
                                                    key_loc.dupe(),
                                                )
                                            }
                                        } else {
                                            type_::any_t::at(
                                                type_::AnySource::AnyError(None),
                                                key_loc.dupe(),
                                            )
                                        };
                                        class_sig::add_setter(
                                            np.static_,
                                            abstract_on,
                                            override_on_declare_class,
                                            Name::new(name.dupe()),
                                            key_loc.dupe(),
                                            None,
                                            fsig,
                                            None,
                                            None,
                                            &mut s,
                                        );
                                        prop_asts.push(Property::NormalProperty(
                                            ast::types::object::NormalProperty {
                                                loc: np.loc.dupe(),
                                                key: rebuild_key(prop_t),
                                                value: ast::types::object::PropertyValue::Set(
                                                    set_loc.dupe(),
                                                    func_ast,
                                                ),
                                                optional: np.optional,
                                                static_: np.static_,
                                                proto: np.proto,
                                                method: np.method,
                                                abstract_: np.abstract_,
                                                override_: np.override_,
                                                variance: np.variance.clone(),
                                                ts_accessibility: np.ts_accessibility.clone(),
                                                init: None,
                                                comments: np.comments.dupe(),
                                            },
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Property::InternalSlot(is) => {
                let name = &is.id.name;
                if name.as_str() == "call" {
                    let value_ast = convert_inner(cx, env, &is.value)?;
                    let (_, t) = value_ast.loc();
                    let t = t.dupe();
                    let t = if is.optional {
                        type_util::optional(t, None, false)
                    } else {
                        t
                    };
                    class_sig::append_call(is.static_, t, &mut s);
                    prop_asts.push(Property::InternalSlot(ast::types::object::InternalSlot {
                        loc: is.loc.dupe(),
                        id: ast::Identifier::new(ast::IdentifierInner {
                            loc: is.id.loc.dupe(),
                            name: is.id.name.dupe(),
                            comments: is.id.comments.dupe(),
                        }),
                        value: value_ast,
                        optional: is.optional,
                        static_: is.static_,
                        method: is.method,
                        comments: is.comments.dupe(),
                    }));
                } else {
                    // Unsupported internal slot
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EUnsupportedSyntax(Box::new((
                            is.loc.dupe(),
                            intermediate_error_types::UnsupportedSyntax::UnsupportedInternalSlot {
                                name: name.dupe(),
                                static_: is.static_,
                            },
                        ))),
                    );
                    let Ok(error_prop) = polymorphic_ast_mapper::object_type_property(
                        &mut typed_ast_utils::ErrorMapper,
                        prop,
                    );
                    prop_asts.push(error_prop);
                }
            }
            Property::SpreadProperty(sp) => {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EInternal(Box::new((
                        sp.loc.dupe(),
                        InternalError::InterfaceTypeSpread,
                    ))),
                );
                let Ok(error_prop) = polymorphic_ast_mapper::object_type_property(
                    &mut typed_ast_utils::ErrorMapper,
                    prop,
                );
                prop_asts.push(error_prop);
            }
            Property::PrivateField(pf) => {
                if !cx.ts_syntax() {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EUnsupportedSyntax(Box::new((
                            pf.loc.dupe(),
                            intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                                TsLibSyntaxKind::PrivateClassField,
                            ),
                        ))),
                    );
                }
                // Private fields are intentionally ignored — they are not part of the
                // public interface, so skipping them is correct behavior.
                let Ok(error_prop) = polymorphic_ast_mapper::object_type_property(
                    &mut typed_ast_utils::UncheckedMapper,
                    prop,
                );
                prop_asts.push(error_prop);
            }
        }
    }
    // Reversed: the signature pipeline folds the computed members of one name
    // into its single entry back to front, so the last one written is the first
    // overload among them.
    for method in deferred_methods.into_iter().rev() {
        method.apply(&mut s);
    }
    Ok((s, prop_asts))
}

fn optional_indexed_access<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    loc: ALoc,
    ia: &ast::types::OptionalIndexedAccess<ALoc, ALoc>,
) -> Result<(Type, ast::types::Type<ALoc, (ALoc, Type)>), flow_utils_concurrency::job_error::JobError>
{
    let reason = reason::mk_reason(None, loc.dupe());
    let _object = &ia.indexed_access.object;
    let index = &ia.indexed_access.index;
    let comments = &ia.indexed_access.comments;
    let index_ast = convert_inner(cx, env, index)?;
    let (_, index_type) = index_ast.loc();
    let index_type = index_type.dupe();
    let (object_t, object_ast) = match _object.deref() {
        ast::types::TypeInner::OptionalIndexedAccess {
            loc: obj_loc,
            inner,
        } => optional_indexed_access(cx, env, obj_loc.dupe(), inner)?,
        _ => {
            let object_ast = convert_inner(cx, env, _object)?;
            let (_, object_t) = object_ast.loc();
            let object_t = object_t.dupe();
            (object_t, object_ast)
        }
    };
    let lhs_reason = type_util::reason_of_t(&object_t).dupe();
    let use_op = type_::UseOp::Op(Arc::new(type_::RootUseOp::IndexedTypeAccess(Box::new(
        type_::IndexedTypeAccessData {
            index_loc: type_util::loc_of_t(&index_type).dupe(),
            object: flow_js_utils::type_or_type_desc_for_error(&object_t),
            index: flow_js_utils::type_or_type_desc_for_error(&index_type),
        },
    ))));
    let non_maybe_destructor = match index_ast.deref() {
        ast::types::TypeInner::StringLiteral { literal, .. } => {
            let name = Name::new(literal.value.dupe());
            if ia.optional {
                type_::Destructor::OptionalIndexedAccessNonMaybeType {
                    index: type_::OptionalIndexedAccessIndex::OptionalIndexedAccessStrLitIndex(
                        name,
                    ),
                }
            } else {
                type_::Destructor::PropertyType { name }
            }
        }
        _ => {
            if ia.optional {
                type_::Destructor::OptionalIndexedAccessNonMaybeType {
                    index: type_::OptionalIndexedAccessIndex::OptionalIndexedAccessTypeIndex(
                        index_type.dupe(),
                    ),
                }
            } else {
                type_::Destructor::ElementType {
                    index_type: index_type.dupe(),
                }
            }
        }
    };
    let non_maybe_result_t = match FlowJs::mk_possibly_evaluated_destructor_for_annotations(
        cx,
        use_op.dupe(),
        &reason,
        &object_t,
        &non_maybe_destructor,
        mk_eval_id(cx, loc.dupe()),
    ) {
        Ok(v) => v,
        Err(flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c)) => {
            return Err(flow_utils_concurrency::job_error::JobError::Canceled(c));
        }
        Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => {
            return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
        }
        Err(err) => panic!("Should not be under speculation: {:?}", err),
    };
    let void_reason = lhs_reason.replace_desc(reason::VirtualReasonDesc::RVoid);
    let result_t = match FlowJs::mk_possibly_evaluated_destructor_for_annotations(
        cx,
        type_::unknown_use(),
        &reason,
        &non_maybe_result_t,
        &type_::Destructor::OptionalIndexedAccessResultType { void_reason },
        type_::eval::Id::generate_id(),
    ) {
        Ok(v) => v,
        Err(flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c)) => {
            return Err(flow_utils_concurrency::job_error::JobError::Canceled(c));
        }
        Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => {
            return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
        }
        Err(err) => panic!("Should not be under speculation: {:?}", err),
    };
    Ok((
        non_maybe_result_t,
        ast::types::Type::new(ast::types::TypeInner::OptionalIndexedAccess {
            loc: (loc, result_t),
            inner: (ast::types::OptionalIndexedAccess {
                indexed_access: ast::types::IndexedAccess {
                    object: object_ast,
                    index: index_ast,
                    comments: comments.clone(),
                },
                optional: ia.optional,
            })
            .into(),
        }),
    ))
}

fn mk_component<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    reason: Reason,
    id_opt: Option<&ast::Identifier<ALoc, ALoc>>,
    tparams_node: Option<&ast::types::TypeParams<ALoc, ALoc>>,
    params: &ast::types::component_params::Params<ALoc, ALoc>,
    renders: &ast::types::ComponentRendersAnnotation<ALoc, ALoc>,
) -> Result<
    (
        Type,
        Option<ast::types::TypeParams<ALoc, (ALoc, Type)>>,
        ast::types::component_params::Params<ALoc, (ALoc, Type)>,
        ast::types::ComponentRendersAnnotation<ALoc, (ALoc, Type)>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    let saved_tparams_map = env.tparams_map.dupe();
    let (tparams, tparam_asts) = mk_type_param_declarations_inner(
        cx,
        env,
        flow_parser::ast_visitor::TypeParamsContext::ComponentDeclaration,
        tparams_node,
    )?;
    let mk_param = |cx_ref: &mut Context<'a>,
                    env_ref: &mut ConvertEnv,
                    param: &ast::types::component_params::Param<ALoc, ALoc>|
     -> Result<
        (
            Type,
            ast::types::component_params::Param<ALoc, (ALoc, Type)>,
        ),
        flow_utils_concurrency::job_error::JobError,
    > {
        let (t, annot_ast) = mk_type_available_annotation_inner(cx_ref, env_ref, &param.annot)?;
        use flow_parser::ast::statement::component_params::ParamName;
        let name_ast = match &param.name {
            ParamName::StringLiteral((l, n)) => ParamName::StringLiteral((l.dupe(), n.clone())),
            ParamName::Identifier(ident) => {
                ParamName::Identifier(ast::Identifier::new(ast::IdentifierInner {
                    loc: (ident.loc.dupe(), t.dupe()),
                    name: ident.name.dupe(),
                    comments: ident.comments.dupe(),
                }))
            }
        };
        Ok((
            t.dupe(),
            ast::types::component_params::Param {
                loc: (param.loc.dupe(), t),
                name: name_ast,
                annot: annot_ast,
                optional: param.optional,
            },
        ))
    };
    let mk_rest = |cx_ref: &mut Context<'a>,
                   env_ref: &mut ConvertEnv,
                   rest: &ast::types::component_params::RestParam<ALoc, ALoc>|
     -> Result<
        (
            Type,
            ast::types::component_params::RestParam<ALoc, (ALoc, Type)>,
        ),
        flow_utils_concurrency::job_error::JobError,
    > {
        let annot_ast = convert_inner(cx_ref, env_ref, &rest.annot)?;
        let (_, t) = annot_ast.loc();
        let t = t.dupe();
        let argument_ast = rest.argument.as_ref().map(|arg| {
            ast::Identifier::new(ast::IdentifierInner {
                loc: (arg.loc.dupe(), t.dupe()),
                name: arg.name.dupe(),
                comments: arg.comments.dupe(),
            })
        });
        Ok((
            t.dupe(),
            ast::types::component_params::RestParam {
                loc: (rest.loc.dupe(), t),
                argument: argument_ast,
                annot: annot_ast,
                optional: rest.optional,
                comments: rest.comments.dupe(),
            },
        ))
    };
    let mut typed_params = Vec::new();
    let mut param_types: Vec<(
        Type,
        ast::types::component_params::Param<ALoc, (ALoc, Type)>,
    )> = Vec::new();
    for p in params.params.iter() {
        let (t, param_ast) = mk_param(cx, env, p)?;
        param_types.push((t, param_ast.clone()));
        typed_params.push(param_ast);
    }
    let mut typed_rest = None;
    let mut rest_type_opt: Option<(
        Type,
        ast::types::component_params::RestParam<ALoc, (ALoc, Type)>,
    )> = None;
    if let Some(ref r) = params.rest {
        let (t, rest_ast) = mk_rest(cx, env, r)?;
        rest_type_opt = Some((t, rest_ast.clone()));
        typed_rest = Some(rest_ast);
    }
    let params_ast = ast::types::component_params::Params {
        loc: {
            let Ok(v) = typed_ast_utils::ErrorMapper.on_type_annot(&params.loc);
            v
        },
        params: typed_params.into(),
        rest: typed_rest,
        comments: params.comments.dupe(),
    };
    let (_ren_loc, renders_t, renders_ast) = match renders {
        ast::types::ComponentRendersAnnotation::AvailableRenders(loc, annot) => {
            let (t, renders_ast) = convert_render_type_inner(cx, env, loc.dupe(), annot)?;
            (
                loc.dupe(),
                t,
                ast::types::ComponentRendersAnnotation::AvailableRenders(loc.dupe(), renders_ast),
            )
        }
        ast::types::ComponentRendersAnnotation::MissingRenders(loc) => {
            let reason = reason::mk_annot_reason(
                reason::VirtualReasonDesc::RType(FlowSmolStr::new("React.Node")),
                loc.dupe(),
            );
            let renders_t = Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::RendersT(Rc::new(
                    type_::CanonicalRendersForm::DefaultRenders,
                ))),
            ));
            (
                loc.dupe(),
                renders_t.dupe(),
                ast::types::ComponentRendersAnnotation::MissingRenders((loc.dupe(), renders_t)),
            )
        }
    };
    env.tparams_map = saved_tparams_map;
    let t = crate::component_sig::component_type::<TypeAnnotationConfig>(
        cx,
        id_opt.is_none(),
        &reason,
        tparams,
        &param_types,
        rest_type_opt.as_ref(),
        renders_t,
        id_opt.map(|id| (&id.loc, &id.name)),
    )?;
    Ok((t, tparam_asts, params_ast, renders_ast))
}

fn mk_super_inner<'a>(
    cx: &mut Context<'a>,
    env: &mut ConvertEnv,
    loc: ALoc,
    c: Type,
    targs: Option<&ast::types::TypeArgs<ALoc, ALoc>>,
) -> Result<
    (
        (ALoc, Type, Option<Vec<Type>>),
        Option<ast::types::TypeArgs<ALoc, (ALoc, Type)>>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    Ok(match targs {
        None => ((loc, c, None), None),
        Some(targs_node) => {
            let (ts, targs_ast) = convert_list_inner(cx, env, &targs_node.arguments)?;
            (
                (loc, c, Some(ts)),
                Some(ast::types::TypeArgs {
                    loc: targs_node.loc.dupe(),
                    arguments: targs_ast.into(),
                    comments: targs_node.comments.dupe(),
                }),
            )
        }
    })
}

pub fn mk_interface_sig<'a>(
    cx: &mut Context<'a>,
    intf_loc: ALoc,
    reason: Reason,
    decl: &ast::statement::Interface<ALoc, ALoc>,
) -> Result<
    (
        Type,
        func_class_sig_types::class::Class<FuncTypeParamsConfig>,
        ast::statement::Interface<ALoc, (ALoc, Type)>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    let id_loc = &decl.id.loc;
    let id_name = &*decl.id;
    let (ref body_loc, ref body) = decl.body;
    let mut env = ConvertEnv::new(None, None, None, FlowOrdMap::new(), BindsSingleValue::No);
    let (tparams, tparams_ast) = mk_type_param_declarations_inner(
        cx,
        &mut env,
        flow_parser::ast_visitor::TypeParamsContext::Interface,
        decl.tparams.as_ref(),
    )?;
    cx.record_interface_tparams(id_loc.dupe(), &tparams, &env.tparams_map);
    let mut extends = Vec::new();
    let mut extends_binding_kinds = Vec::new();
    let mut extends_ast = Vec::new();
    for ext in decl.extends.iter() {
        let ((typeapp, binding_kind), ext_ast) = mk_interface_super(cx, &mut env, ext)?;
        extends.push(typeapp);
        extends_binding_kinds.push(binding_kind);
        extends_ast.push(ext_ast);
    }
    let (this_tparam, this_t) =
        class_sig::mk_this(type_::mixed_t::make(reason.dupe()), cx, reason.dupe());
    env.tparams_map.insert(
        SubstName::name(FlowSmolStr::new_inline("this")),
        this_t.dupe(),
    );
    let class_name = &id_name.name;
    let id = cx.make_aloc_id(id_loc);
    let super_ = func_class_sig_types::class::Super::Interface(
        func_class_sig_types::class::InterfaceSuper {
            inline: false,
            extends,
            extends_binding_kinds,
            function_like: is_function_like(&body.properties),
            this_tparam: Some(this_tparam),
            this_t: Some(this_t.dupe()),
        },
    );
    let mut iface_sig = class_sig::empty(
        id,
        Some(class_name.dupe()),
        intf_loc.dupe(),
        reason.dupe(),
        tparams,
        env.tparams_map
            .iter()
            .map(|(k, v)| (k.dupe(), v.dupe()))
            .collect(),
        super_,
    );
    // TODO: interfaces don't have a name field, or even statics
    class_sig::add_name_field(&mut iface_sig);
    let (iface_sig, properties_ast) = add_interface_properties(
        cx,
        &mut env,
        Some(id_loc.dupe()),
        false,
        intermediate_error_types::ObjKind::Interface,
        this_t,
        &body.properties,
        iface_sig,
    )?;
    let (_t_internal, t, (own_props, proto_props)) = class_sig::classtype(
        cx,
        true,
        type_::InstanceKind::InterfaceKind {
            inline: false,
            reject_primitives: false,
        },
        &iface_sig,
    );
    cx.add_interface_prop_ids(id_loc.dupe(), own_props, proto_props);
    Ok((
        t.dupe(),
        iface_sig,
        ast::statement::Interface {
            id: ast::Identifier::new(ast::IdentifierInner {
                loc: (id_loc.dupe(), t),
                name: id_name.name.dupe(),
                comments: id_name.comments.dupe(),
            }),
            tparams: tparams_ast,
            extends: extends_ast.into(),
            body: (
                body_loc.dupe(),
                ast::types::Object {
                    exact: body.exact,
                    inexact: false,
                    properties: properties_ast.into(),
                    comments: body.comments.dupe(),
                },
            ),
            comments: decl.comments.dupe(),
        },
    ))
}

pub fn mk_declare_component_sig<'a>(
    cx: &mut Context<'a>,
    loc: ALoc,
    component: &ast::statement::DeclareComponent<ALoc, ALoc>,
) -> Result<
    (Type, ast::statement::DeclareComponent<ALoc, (ALoc, Type)>),
    flow_utils_concurrency::job_error::JobError,
> {
    let id_loc = &component.id.loc;
    let id_name = &*component.id;
    let name = &id_name.name;
    let reason = reason::mk_reason(
        reason::VirtualReasonDesc::RComponent(name.dupe()),
        loc.dupe(),
    );
    let mut env = ConvertEnv::new(None, None, None, FlowOrdMap::new(), BindsSingleValue::No);
    let (tparams, tparam_asts) = mk_type_param_declarations_inner(
        cx,
        &mut env,
        flow_parser::ast_visitor::TypeParamsContext::DeclareComponent,
        component.tparams.as_ref(),
    )?;
    let params_loc = &component.params.loc;
    let params_comments = &component.params.comments;

    // Helper to convert annotation_or_hint to annotation
    fn convert_annot_or_hint<'a>(
        cx: &mut Context<'a>,
        env: &mut ConvertEnv,
        _loc: &ALoc,
        annot_or_hint: &ast::types::AnnotationOrHint<ALoc, ALoc>,
    ) -> Result<
        (Type, ast::types::AnnotationOrHint<ALoc, (ALoc, Type)>),
        flow_utils_concurrency::job_error::JobError,
    > {
        Ok(match annot_or_hint {
            ast::types::AnnotationOrHint::Available(annot) => {
                let (t, annot_ast) = mk_type_available_annotation_inner(cx, env, annot)?;
                (t, ast::types::AnnotationOrHint::Available(annot_ast))
            }
            ast::types::AnnotationOrHint::Missing(hint_loc) => {
                let t = type_::any_t::error(reason::mk_reason(None, hint_loc.dupe()));
                (
                    t.dupe(),
                    ast::types::AnnotationOrHint::Missing((hint_loc.dupe(), t)),
                )
            }
        })
    }
    // Process each param: convert statement param to type param for Component_type_params,
    // and build typed statement param for output
    fn process_param<'a>(
        cx: &mut Context<'a>,
        env: &mut ConvertEnv,
        param: &ast::statement::component_params::Param<ALoc, ALoc>,
    ) -> Result<
        (
            Type,
            ast::types::component_params::Param<ALoc, (ALoc, Type)>,
            ast::statement::component_params::Param<ALoc, (ALoc, Type)>,
        ),
        flow_utils_concurrency::job_error::JobError,
    > {
        use flow_parser::ast::statement::component_params::ParamName;

        let p_loc = &param.loc;
        let param_name = &param.name;
        let local = &param.local;
        let default = &param.default;
        let shorthand = param.shorthand;

        match local {
            ast::pattern::Pattern::Identifier {
                inner: id_inner,
                loc: local_loc,
                ..
            } => {
                let local_id = &id_inner.name;
                let local_id_loc = &local_id.loc;
                let annot = &id_inner.annot;
                let optional = id_inner.optional;
                let (t, typed_annot) = convert_annot_or_hint(cx, env, p_loc, annot)?;
                // Build typed param_name for statement params
                let typed_stmt_name = match param_name {
                    ParamName::StringLiteral(sl) => ParamName::StringLiteral(sl.clone()),
                    ParamName::Identifier(ident) => {
                        ParamName::Identifier(ast::Identifier::new(ast::IdentifierInner {
                            loc: (ident.loc.dupe(), t.dupe()),
                            name: ident.name.dupe(),
                            comments: ident.comments.dupe(),
                        }))
                    }
                };
                // Build typed statement param
                let typed_local = ast::pattern::Pattern::Identifier {
                    loc: (local_loc.dupe(), t.dupe()),
                    inner: (ast::pattern::Identifier {
                        name: ast::Identifier::new(ast::IdentifierInner {
                            loc: (local_id_loc.dupe(), t.dupe()),
                            name: local_id.name.dupe(),
                            comments: local_id.comments.dupe(),
                        }),
                        annot: typed_annot.clone(),
                        optional,
                    })
                    .into(),
                };
                let typed_default = default.as_ref().map(|d| {
                    let Ok(v) =
                        polymorphic_ast_mapper::expression(&mut typed_ast_utils::ErrorMapper, d);
                    v
                });
                let typed_param = ast::statement::component_params::Param {
                    loc: (p_loc.dupe(), t.dupe()),
                    name: typed_stmt_name,
                    local: typed_local,
                    default: typed_default,
                    shorthand,
                };
                // Build type component param for Component_type_params
                let typed_tc_name = match param_name {
                    ParamName::StringLiteral(sl) => ParamName::StringLiteral(sl.clone()),
                    ParamName::Identifier(ident) => {
                        ParamName::Identifier(ast::Identifier::new(ast::IdentifierInner {
                            loc: (ident.loc.dupe(), t.dupe()),
                            name: ident.name.dupe(),
                            comments: ident.comments.dupe(),
                        }))
                    }
                };
                let type_param_annot = match &typed_annot {
                    ast::types::AnnotationOrHint::Available(annot) => annot.clone(),
                    ast::types::AnnotationOrHint::Missing((hint_loc, hint_t)) => {
                        ast::types::Annotation {
                            loc: hint_loc.dupe(),
                            annotation: ast::types::Type::new(ast::types::TypeInner::Any {
                                loc: (hint_loc.dupe(), hint_t.dupe()),
                                comments: None,
                            }),
                        }
                    }
                };
                let type_param = ast::types::component_params::Param {
                    loc: (p_loc.dupe(), t.dupe()),
                    name: typed_tc_name,
                    annot: type_param_annot,
                    optional,
                };
                Ok((t, type_param, typed_param))
            }
            _ => {
                // Non-identifier patterns (e.g., object/array destructuring via `as`) are errors,
                // but we still need to produce something for the typed AST
                let t = type_::any_t::error(reason::mk_reason(None, p_loc.dupe()));
                let typed_local = {
                    let Ok(v) = polymorphic_ast_mapper::pattern(
                        &mut typed_ast_utils::ErrorMapper,
                        None,
                        local,
                    );
                    v
                };
                let typed_default = default.as_ref().map(|d| {
                    let Ok(v) =
                        polymorphic_ast_mapper::expression(&mut typed_ast_utils::ErrorMapper, d);
                    v
                });
                // Build typed param_name for statement params
                let typed_stmt_name = match param_name {
                    ParamName::StringLiteral(sl) => ParamName::StringLiteral(sl.clone()),
                    ParamName::Identifier(ident) => {
                        ParamName::Identifier(ast::Identifier::new(ast::IdentifierInner {
                            loc: (ident.loc.dupe(), t.dupe()),
                            name: ident.name.dupe(),
                            comments: ident.comments.dupe(),
                        }))
                    }
                };
                let typed_param = ast::statement::component_params::Param {
                    loc: (p_loc.dupe(), t.dupe()),
                    name: typed_stmt_name,
                    local: typed_local,
                    default: typed_default,
                    shorthand,
                };
                // Build type component param for Component_type_params
                let typed_tc_name = match param_name {
                    ParamName::StringLiteral(sl) => ParamName::StringLiteral(sl.clone()),
                    ParamName::Identifier(ident) => {
                        ParamName::Identifier(ast::Identifier::new(ast::IdentifierInner {
                            loc: (ident.loc.dupe(), t.dupe()),
                            name: ident.name.dupe(),
                            comments: ident.comments.dupe(),
                        }))
                    }
                };
                let type_param = ast::types::component_params::Param {
                    loc: (p_loc.dupe(), t.dupe()),
                    name: typed_tc_name,
                    annot: ast::types::Annotation {
                        loc: p_loc.dupe(),
                        annotation: ast::types::Type::new(ast::types::TypeInner::Any {
                            loc: (p_loc.dupe(), t.dupe()),
                            comments: None,
                        }),
                    },
                    optional: false,
                };
                Ok((t, type_param, typed_param))
            }
        }
    }

    // Process rest param
    fn process_rest<'a>(
        cx: &mut Context<'a>,
        env: &mut ConvertEnv,
        rest_param: &ast::statement::component_params::RestParam<ALoc, ALoc>,
    ) -> Result<
        (
            Type,
            ast::types::component_params::RestParam<ALoc, (ALoc, Type)>,
            ast::statement::component_params::RestParam<ALoc, (ALoc, Type)>,
        ),
        flow_utils_concurrency::job_error::JobError,
    > {
        let r_loc = &rest_param.loc;
        let argument = &rest_param.argument;
        let rest_comments = &rest_param.comments;
        match argument {
            ast::pattern::Pattern::Identifier {
                inner: id_inner,
                loc: arg_loc,
                ..
            } => {
                let arg_id = &id_inner.name;
                let arg_id_loc = &arg_id.loc;
                let annot = &id_inner.annot;
                let optional = id_inner.optional;
                let (t, typed_annot) = convert_annot_or_hint(cx, env, r_loc, annot)?;
                // Build typed statement rest param
                let typed_argument = ast::pattern::Pattern::Identifier {
                    loc: (arg_loc.dupe(), t.dupe()),
                    inner: ast::pattern::Identifier {
                        name: ast::Identifier::new(ast::IdentifierInner {
                            loc: (arg_id_loc.dupe(), t.dupe()),
                            name: arg_id.name.dupe(),
                            comments: arg_id.comments.dupe(),
                        }),
                        annot: typed_annot.clone(),
                        optional,
                    }
                    .into(),
                };
                let typed_rest = ast::statement::component_params::RestParam {
                    loc: (r_loc.dupe(), t.dupe()),
                    argument: typed_argument,
                    comments: rest_comments.dupe(),
                };
                // Build type component rest param for Component_type_params
                let type_rest_annot = match &typed_annot {
                    ast::types::AnnotationOrHint::Available(annot) => annot.annotation.clone(),
                    ast::types::AnnotationOrHint::Missing((hint_loc, hint_t)) => {
                        ast::types::Type::new(ast::types::TypeInner::Any {
                            loc: (hint_loc.dupe(), hint_t.dupe()),
                            comments: None,
                        })
                    }
                };
                let type_rest = ast::types::component_params::RestParam {
                    loc: (r_loc.dupe(), t.dupe()),
                    argument: Some(ast::Identifier::new(ast::IdentifierInner {
                        loc: (arg_id_loc.dupe(), t.dupe()),
                        name: arg_id.name.dupe(),
                        comments: arg_id.comments.dupe(),
                    })),
                    annot: type_rest_annot,
                    optional,
                    comments: rest_comments.clone(),
                };
                Ok((t, type_rest, typed_rest))
            }
            _ => {
                // Non-identifier patterns are errors, produce error typed AST
                let t = type_::any_t::error(reason::mk_reason(None, r_loc.dupe()));
                let Ok(typed_argument) = polymorphic_ast_mapper::pattern(
                    &mut typed_ast_utils::ErrorMapper,
                    None,
                    argument,
                );
                let typed_rest = ast::statement::component_params::RestParam {
                    loc: (r_loc.dupe(), t.dupe()),
                    argument: typed_argument,
                    comments: rest_comments.clone(),
                };
                let type_rest = ast::types::component_params::RestParam {
                    loc: (r_loc.dupe(), t.dupe()),
                    argument: None,
                    annot: ast::types::Type::new(ast::types::TypeInner::Any {
                        loc: (r_loc.dupe(), t.dupe()),
                        comments: None,
                    }),
                    optional: false,
                    comments: rest_comments.clone(),
                };
                Ok((t, type_rest, typed_rest))
            }
        }
    }

    // Process all params
    let processed_params: Vec<_> = try_map_vec(&component.params.params, |param| {
        process_param(cx, &mut env, param)
    })?;
    let processed_rest = component
        .params
        .rest
        .as_ref()
        .map(|rest| process_rest(cx, &mut env, rest))
        .transpose()?;
    // Build Component_type_params
    let type_params: Vec<_> = processed_params
        .iter()
        .map(|(t, tp, _)| (t.dupe(), tp.clone()))
        .collect();
    let type_rest_opt = processed_rest
        .as_ref()
        .map(|(t, tr, _)| (t.dupe(), tr.clone()));
    let typed_params = {
        let stmt_params: Vec<_> = processed_params.into_iter().map(|(_, _, sp)| sp).collect();
        let stmt_rest = processed_rest.map(|(_, _, sr)| sr);
        ast::statement::component_params::Params {
            loc: {
                let Ok(v) = typed_ast_utils::ErrorMapper.on_type_annot(params_loc);
                v
            },
            params: stmt_params.into(),
            rest: stmt_rest,
            comments: params_comments.clone(),
        }
    };
    // Process renders
    let (_ren_loc, renders_t, renders_ast) = match &component.renders {
        ast::types::ComponentRendersAnnotation::AvailableRenders(r_loc, annot) => {
            let (t, r_ast) = convert_render_type_inner(cx, &mut env, r_loc.dupe(), annot)?;
            (
                r_loc.dupe(),
                t,
                ast::types::ComponentRendersAnnotation::AvailableRenders(r_loc.dupe(), r_ast),
            )
        }
        ast::types::ComponentRendersAnnotation::MissingRenders(r_loc) => {
            let r = reason::mk_annot_reason(
                reason::VirtualReasonDesc::RType(FlowSmolStr::new("React.Node")),
                r_loc.dupe(),
            );
            let renders_t = Type::new(type_::TypeInner::DefT(
                r,
                type_::DefT::new(type_::DefTInner::RendersT(Rc::new(
                    type_::CanonicalRendersForm::DefaultRenders,
                ))),
            ));
            (
                r_loc.dupe(),
                renders_t.dupe(),
                ast::types::ComponentRendersAnnotation::MissingRenders((r_loc.dupe(), renders_t)),
            )
        }
    };
    let t = crate::component_sig::component_type::<TypeAnnotationConfig>(
        cx,
        false,
        &reason,
        tparams,
        &type_params,
        type_rest_opt.as_ref(),
        renders_t,
        Some((id_loc, name)),
    )?;
    Ok((
        t.dupe(),
        ast::statement::DeclareComponent {
            tparams: tparam_asts,
            params: typed_params,
            id: ast::Identifier::new(ast::IdentifierInner {
                loc: (id_loc.dupe(), t),
                name: id_name.name.dupe(),
                comments: id_name.comments.dupe(),
            }),
            renders: renders_ast,
            comments: component.comments.dupe(),
        },
    ))
}

pub fn mk_declare_class_sig<'a>(
    cx: &mut Context<'a>,
    class_loc: ALoc,
    class_name: &FlowSmolStr,
    reason: Reason,
    decl: &ast::statement::DeclareClass<ALoc, ALoc>,
) -> Result<
    (
        Type,
        func_class_sig_types::class::Class<FuncTypeParamsConfig>,
        ast::statement::DeclareClass<ALoc, (ALoc, Type)>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    use flow_typing_type::type_::*;

    fn mk_mixins<'a>(
        cx: &mut Context<'a>,
        env: &mut ConvertEnv,
        generic_with_loc: &(ALoc, ast::types::Generic<ALoc, ALoc>),
    ) -> Result<
        (
            (ALoc, Type, Option<Vec<Type>>),
            (ALoc, ast::types::Generic<ALoc, (ALoc, Type)>),
        ),
        flow_utils_concurrency::job_error::JobError,
    > {
        let (loc, generic) = generic_with_loc;
        let loc = loc.dupe();
        let name = qualified_name(&generic.id);
        let r = reason::mk_annot_reason(
            reason::VirtualReasonDesc::RType(FlowSmolStr::new(&name)),
            loc.dupe(),
        );
        let (i, id_ast) = convert_qualification_with_lookup_mode(
            cx,
            Some(type_env::LookupMode::ForValue),
            &generic.id,
        )?;
        let props_bag = crate::type_annotation_cons_gen::mixin(cx, r, i);
        let (t, targs_ast) =
            mk_super_inner(cx, env, loc.dupe(), props_bag, generic.targs.as_ref())?;
        Ok((
            t,
            (
                loc.dupe(),
                ast::types::Generic {
                    id: id_ast,
                    targs: targs_ast,
                    comments: generic.comments.dupe(),
                },
            ),
        ))
    }

    fn is_object_builtin_libdef(cx: &mut Context<'_>, loc: &ALoc, name: &str) -> bool {
        name == "Object"
            && match loc.source() {
                None => false,
                Some(source) => cx.is_global_libdef(source),
            }
    }

    use std::cell::OnceCell;
    use std::rc::Rc;

    type SigResult = Result<
        (
            Type,
            Type,
            func_class_sig_types::class::Class<FuncTypeParamsConfig>,
            ast::statement::DeclareClass<ALoc, (ALoc, Type)>,
        ),
        flow_utils_concurrency::job_error::JobError,
    >;

    fn mk_declare_class_sig_with_self<'a>(
        cx: &mut Context<'a>,
        class_loc: ALoc,
        class_name: &FlowSmolStr,
        reason: Reason,
        self_: Type,
        decl: &ast::statement::DeclareClass<ALoc, ALoc>,
    ) -> SigResult {
        let id_loc = &decl.id.loc;
        let id_name = &*decl.id;
        let mut env = ConvertEnv::new(None, None, None, FlowOrdMap::new(), BindsSingleValue::No);
        let (tparams, tparam_asts) = mk_type_param_declarations_inner(
            cx,
            &mut env,
            flow_parser::ast_visitor::TypeParamsContext::DeclareClass,
            decl.tparams.as_ref(),
        )?;
        cx.record_interface_tparams(id_loc.dupe(), &tparams, &env.tparams_map);
        let (this_tparam, this_t) = class_sig::mk_this(self_, cx, reason.dupe());
        let id = cx.make_aloc_id(id_loc);
        let (extends, extends_ast) = {
            use ast::statement::DeclareClassExtends;
            match &decl.extends {
                Some((loc, DeclareClassExtends::ExtendsIdent(generic))) => {
                    let (i, id_ast) = convert_qualification_with_lookup_mode(
                        cx,
                        Some(type_env::LookupMode::ForValue),
                        &generic.id,
                    )?;
                    let (t, targs_ast) =
                        mk_super_inner(cx, &mut env, loc.dupe(), i, generic.targs.as_ref())?;
                    (
                        Some(t),
                        Some((
                            loc.dupe(),
                            DeclareClassExtends::ExtendsIdent(ast::types::Generic {
                                id: id_ast,
                                targs: targs_ast,
                                comments: generic.comments.dupe(),
                            }),
                        )),
                    )
                }
                Some((loc, ext @ DeclareClassExtends::ExtendsCall { .. })) => {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EUnsupportedSyntax(Box::new((
                            loc.dupe(),
                            intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                                TsLibSyntaxKind::ClassExtendsCall,
                            ),
                        ))),
                    );
                    let Ok(ext_ast) = polymorphic_ast_mapper::declare_class_extends(
                        &mut typed_ast_utils::ErrorMapper,
                        ext,
                    );
                    (None, Some((loc.dupe(), ext_ast)))
                }
                None => (None, None),
            }
        };
        let (mixins_list, mixins_ast_list): (Vec<_>, Vec<_>) =
            try_map_pair_vecs(&decl.mixins, |m| mk_mixins(cx, &mut env, m))?;
        let (implements_list, implements_binding_kinds, implements_ast) = match &decl.implements {
            None => (Vec::new(), Vec::new(), None),
            Some(impls) => {
                let (impl_list, ifaces_ast): (Vec<_>, Vec<_>) = try_map_pair_vecs(
                    &impls.interfaces,
                    |iface| -> Result<_, flow_utils_concurrency::job_error::JobError> {
                        let id = &iface.id;
                        match id {
                            ast::types::generic::Identifier::Qualified(_)
                            | ast::types::generic::Identifier::ImportTypeAnnot(_)
                                if !cx.tslib_syntax() =>
                            {
                                flow_js_utils::add_output_non_speculating(
                                    cx,
                                    ErrorMessage::EUnsupportedSyntax(Box::new((
                                        iface.loc.dupe(),
                                        intermediate_error_types::UnsupportedSyntax::TSLibSyntax(
                                            TsLibSyntaxKind::ImplementsDottedPath,
                                        ),
                                    ))),
                                );
                            }
                            _ => {}
                        }
                        let id_pre_convert = id;
                        let (c, id) = convert_qualification(cx, id)?;
                        let binding_kind =
                            binding_kind_of_generic_id_post_convert(cx, id_pre_convert, &c)?;
                        let (typeapp, targs_ast) = match &iface.targs {
                            None => ((iface.loc.dupe(), c.dupe(), None), None),
                            Some(targs) => {
                                let (ts, targs_ast) =
                                    convert_list_inner(cx, &mut env, &targs.arguments)?;
                                (
                                    (iface.loc.dupe(), c.dupe(), Some(ts)),
                                    Some(ast::types::TypeArgs::<ALoc, (ALoc, Type)> {
                                        loc: targs.loc.dupe(),
                                        arguments: targs_ast.into(),
                                        comments: targs.comments.dupe(),
                                    }),
                                )
                            }
                        };
                        Ok((
                            (typeapp, binding_kind),
                            ast::class::implements::Interface {
                                loc: iface.loc.dupe(),
                                id,
                                targs: targs_ast,
                            },
                        ))
                    },
                )?;
                let (implements, implements_binding_kinds): (Vec<_>, Vec<_>) =
                    impl_list.into_iter().unzip();
                (
                    implements,
                    implements_binding_kinds,
                    Some(ast::class::Implements {
                        loc: impls.loc.dupe(),
                        interfaces: ifaces_ast.into(),
                        comments: impls.comments.dupe(),
                    }),
                )
            }
        };
        let super_kind = {
            let extends_kind = match extends {
                None => func_class_sig_types::class::Extends::Implicit {
                    null: is_object_builtin_libdef(cx, id_loc, &id_name.name),
                },
                Some(e) => func_class_sig_types::class::Extends::Explicit(e),
            };
            func_class_sig_types::class::Super::Class(func_class_sig_types::class::ClassSuper {
                extends: extends_kind,
                mixins: mixins_list,
                implements: implements_list,
                implements_binding_kinds,
                this_t: this_t.dupe(),
                this_tparam,
            })
        };
        let mut iface_sig = class_sig::empty(
            id,
            Some(class_name.dupe()),
            class_loc.dupe(),
            reason.dupe(),
            tparams,
            env.tparams_map
                .iter()
                .map(|(k, v)| (k.dupe(), v.dupe()))
                .collect(),
            super_kind,
        );
        // All classes have a static "name" property.
        class_sig::add_name_field(&mut iface_sig);
        let abstract_on = decl.abstract_ && cx.tslib_syntax();
        if abstract_on {
            iface_sig.abstract_ = true;
        }
        let tparams_map_with_this = {
            let mut m = env.tparams_map.dupe();
            m.insert(SubstName::name(FlowSmolStr::new("this")), this_t.dupe());
            m
        };
        let mut env_with_this = env.clone();
        env_with_this.tparams_map = tparams_map_with_this;
        let this_type = type_::mixed_t::make(type_util::reason_of_t(&this_t).dupe());
        let (ref body_loc, ref body) = decl.body;
        let (updated_sig, properties_typed) = add_interface_properties(
            cx,
            &mut env_with_this,
            Some(id_loc.dupe()),
            abstract_on,
            intermediate_error_types::ObjKind::DeclareClass,
            this_type,
            &body.properties,
            iface_sig,
        )?;
        iface_sig = updated_sig;
        let body_props = properties_typed;

        // Add a default ctor if we don't have a ctor and won't inherit one from a super
        if !class_sig::mem_constructor(&iface_sig)
            && decl.extends.is_none()
            && decl.mixins.is_empty()
        {
            let ctor_reason = reason.replace_desc(None);
            class_sig::add_default_constructor(ctor_reason, &mut iface_sig);
        }
        // Flag this sig as built from a [declare class] form so the
        // implicit-override check is skipped for it (ambient declarations
        // don't need to redeclare the [override] modifier).
        iface_sig.is_declare = true;

        let (t_internal, t, (own_props, proto_props)) =
            class_sig::classtype(cx, true, type_::InstanceKind::ClassKind, &iface_sig);
        cx.add_interface_prop_ids(id_loc.dupe(), own_props, proto_props);

        let tast = ast::statement::DeclareClass {
            id: ast::Identifier::new(ast::IdentifierInner {
                loc: (id_loc.dupe(), t.dupe()),
                name: id_name.name.dupe(),
                comments: id_name.comments.dupe(),
            }),
            tparams: tparam_asts,
            body: (
                body_loc.dupe(),
                ast::types::Object {
                    exact: body.exact,
                    inexact: false,
                    properties: body_props.into(),
                    comments: body.comments.dupe(),
                },
            ),
            extends: extends_ast,
            mixins: mixins_ast_list.into(),
            implements: implements_ast,
            abstract_: decl.abstract_,
            comments: decl.comments.dupe(),
        };

        Ok((t_internal, t, iface_sig, tast))
    }

    // The self type of the class is the internal class type of its signature, which is only
    // made once the self type is in place.
    let class_t_internal: Rc<OnceCell<Result<Type, flow_utils_concurrency::job_error::JobError>>> =
        Rc::new(OnceCell::new());
    let self_ = flow_typing_tvar::mk_fully_resolved_lazy(cx, reason.dupe(), true, {
        let class_t_internal = class_t_internal.dupe();
        move |_cx: &mut Context<'_>| {
            class_t_internal
                .get()
                .expect("the class signature is made before its self type is forced")
                .clone()
        }
    });
    let result = mk_declare_class_sig_with_self(cx, class_loc, class_name, reason, self_, decl);
    class_t_internal
        .set(result.as_ref().map(|r| r.0.dupe()).map_err(|e| e.dupe()))
        .expect("the class signature is made once");
    let (_, t, iface_sig, tast) = result?;
    Ok((t, iface_sig, tast))
}

// =========================================================================
// Public wrapper functions that create env and delegate
// =========================================================================

pub fn convert<'a>(
    cx: &mut Context<'a>,
    tparams_map: FlowOrdMap<SubstName, Type>,
    t: &ast::types::Type<ALoc, ALoc>,
) -> Result<ast::types::Type<ALoc, (ALoc, Type)>, flow_utils_concurrency::job_error::JobError> {
    let mut env = ConvertEnv::new(None, None, None, tparams_map, BindsSingleValue::No);
    convert_inner(cx, &mut env, t)
}

pub fn convert_list<'a>(
    cx: &mut Context<'a>,
    tparams_map: FlowOrdMap<SubstName, Type>,
    asts: &[ast::types::Type<ALoc, ALoc>],
) -> Result<
    (Vec<Type>, Vec<ast::types::Type<ALoc, (ALoc, Type)>>),
    flow_utils_concurrency::job_error::JobError,
> {
    let mut env = ConvertEnv::new(None, None, None, tparams_map, BindsSingleValue::No);
    convert_list_inner(cx, &mut env, asts)
}

pub fn convert_render_type<'a>(
    cx: &mut Context<'a>,
    tparams_map: FlowOrdMap<SubstName, Type>,
    loc: ALoc,
    renders: &ast::types::Renders<ALoc, ALoc>,
) -> Result<
    (Type, ast::types::Renders<ALoc, (ALoc, Type)>),
    flow_utils_concurrency::job_error::JobError,
> {
    let mut env = ConvertEnv::new(None, None, None, tparams_map, BindsSingleValue::No);
    convert_render_type_inner(cx, &mut env, loc, renders)
}

pub fn convert_type_guard<'a>(
    cx: &mut Context<'a>,
    tparams_map: FlowOrdMap<SubstName, Type>,
    fparams: &[type_::FunParam],
    gloc: ALoc,
    kind: ast::types::TypeGuardKind,
    id_name: &ast::Identifier<ALoc, ALoc>,
    t: Option<&ast::types::Type<ALoc, ALoc>>,
    comments: Option<&ast::Syntax<ALoc, std::sync::Arc<[ast::Comment<ALoc>]>>>,
) -> Result<
    (
        Type,
        ast::types::TypeGuard<ALoc, (ALoc, Type)>,
        Option<type_::TypeGuard>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    let mut env = ConvertEnv::new(None, None, None, tparams_map, BindsSingleValue::No);
    convert_type_guard_inner(cx, &mut env, fparams, gloc, kind, id_name, t, comments)
}

pub fn convert_indexer(
    cx: &mut Context,
    tparams_map: &FlowOrdMap<SubstName, Type>,
    indexer: &ast::types::object::Indexer<ALoc, ALoc>,
) -> Result<
    (
        type_::DictType,
        ast::types::object::Indexer<ALoc, (ALoc, Type)>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    let mut env = ConvertEnv::new(None, None, None, tparams_map.dupe(), BindsSingleValue::No);
    convert_indexer_internal(cx, &mut env, indexer)
}

pub fn convert_return_annotation<'a>(
    cx: &mut Context<'a>,
    meth_kind: MethodKind,
    tparams_map: FlowOrdMap<SubstName, Type>,
    params: &ast::types::function::Params<ALoc, ALoc>,
    fparams: &[type_::FunParam],
    return_annot: &ast::types::function::ReturnAnnotation<ALoc, ALoc>,
) -> Result<
    (
        Type,
        ast::types::function::ReturnAnnotation<ALoc, (ALoc, Type)>,
        Option<type_::TypeGuard>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    let mut env = ConvertEnv::new(None, None, None, tparams_map, BindsSingleValue::No);
    convert_return_annotation_inner(cx, &mut env, meth_kind, params, fparams, return_annot)
}

pub fn mk_empty_interface_type<'a>(
    cx: &mut Context<'a>,
    loc: ALoc,
) -> Result<Type, flow_utils_concurrency::job_error::JobError> {
    let interface_type = empty_interface_annot(loc);
    let mut env = ConvertEnv::new(
        None,
        None,
        None,
        FlowOrdMap::default(),
        BindsSingleValue::No,
    );
    let result = convert_inner(cx, &mut env, &interface_type)?;
    let (_, t) = result.loc();
    Ok(t.dupe())
}

pub fn mk_super<'a>(
    cx: &mut Context<'a>,
    tparams_map: FlowOrdMap<SubstName, Type>,
    loc: ALoc,
    c: Type,
    targs: Option<&ast::types::TypeArgs<ALoc, ALoc>>,
) -> Result<
    (
        (ALoc, Type, Option<Vec<Type>>),
        Option<ast::types::TypeArgs<ALoc, (ALoc, Type)>>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    let mut env = ConvertEnv::new(None, None, None, tparams_map, BindsSingleValue::No);
    mk_super_inner(cx, &mut env, loc, c, targs)
}

/// `binds_single_value` says whether the annotation is the type of a position
/// that binds exactly one value, which a variable declaration and a static
/// class field do. Only such a position may spell out a `unique symbol`.
pub fn mk_type_available_annotation<'a>(
    cx: &mut Context<'a>,
    tparams_map: FlowOrdMap<SubstName, Type>,
    annotation: &ast::types::Annotation<ALoc, ALoc>,
    binds_single_value: BindsSingleValue,
) -> Result<
    (Type, ast::types::Annotation<ALoc, (ALoc, Type)>),
    flow_utils_concurrency::job_error::JobError,
> {
    let mut env = ConvertEnv::new(None, None, None, tparams_map, binds_single_value);
    mk_type_available_annotation_inner(cx, &mut env, annotation)
}

pub fn mk_function_type_annotation<'a>(
    cx: &mut Context<'a>,
    tparams_map: FlowOrdMap<SubstName, Type>,
    func: &(ALoc, ast::types::Function<ALoc, ALoc>),
) -> Result<
    (Type, (ALoc, ast::types::Function<ALoc, (ALoc, Type)>)),
    flow_utils_concurrency::job_error::JobError,
> {
    let mut env = ConvertEnv::new(None, None, None, tparams_map, BindsSingleValue::No);
    mk_function_type_annotation_inner(cx, &mut env, func)
}

pub fn mk_nominal_type<'a>(
    cx: &mut Context<'a>,
    reason: Reason,
    tparams_map: FlowOrdMap<SubstName, Type>,
    c: Type,
    targs: Option<&ast::types::TypeArgs<ALoc, ALoc>>,
) -> Result<
    (Type, Option<ast::types::TypeArgs<ALoc, (ALoc, Type)>>),
    flow_utils_concurrency::job_error::JobError,
> {
    let mut env = ConvertEnv::new(None, None, None, tparams_map, BindsSingleValue::No);
    mk_nominal_type_inner(cx, &mut env, reason, c, targs)
}

pub fn mk_type_param<'a>(
    cx: &mut Context<'a>,
    tparams_map: FlowOrdMap<SubstName, Type>,
    kind: flow_parser::ast_visitor::TypeParamsContext,
    type_param: &ast::types::TypeParam<ALoc, ALoc>,
) -> Result<
    (ast::types::TypeParam<ALoc, (ALoc, Type)>, TypeParam, Type),
    flow_utils_concurrency::job_error::JobError,
> {
    let mut env = ConvertEnv::new(None, None, None, tparams_map, BindsSingleValue::No);
    mk_type_param_inner(cx, &mut env, kind, type_param)
}

pub fn mk_type_param_declarations<'a>(
    cx: &mut Context<'a>,
    kind: flow_parser::ast_visitor::TypeParamsContext,
    tparams_map: Option<FlowOrdMap<SubstName, Type>>,
    tparams: Option<&ast::types::TypeParams<ALoc, ALoc>>,
) -> Result<
    (
        type_::TypeParams,
        FlowOrdMap<SubstName, Type>,
        Option<ast::types::TypeParams<ALoc, (ALoc, Type)>>,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    let tparams_map = tparams_map.unwrap_or_default();
    let mut env = ConvertEnv::new(None, None, None, tparams_map, BindsSingleValue::No);
    let (type_params, tast) = mk_type_param_declarations_inner(cx, &mut env, kind, tparams)?;
    Ok((type_params, env.tparams_map, tast))
}
