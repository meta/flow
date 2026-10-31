/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::error_ref::FunctionReferenceKind;
use flow_common::reason;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::function::Body as FunctionBody;
use flow_parser::ast::function::Function;
use flow_parser::ast::function::ReturnAnnot;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::loc_sig::LocSig;
use flow_parser::polymorphic_ast_mapper;
use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_errors::error_message;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_common::obj_type;
use flow_typing_flow_js::flow_js;
use flow_typing_loc_env::func_class_sig_types::ConfigTypes;
use flow_typing_loc_env::func_class_sig_types::func::Func;
use flow_typing_loc_env::func_class_sig_types::func::Kind;
use flow_typing_loc_env::func_class_sig_types::param;
use flow_typing_type::type_;
use flow_typing_type::type_::AnnotatedOrInferred;
use flow_typing_type::type_::FunParam;
use flow_typing_type::type_::PolyTKind;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_util;
use flow_typing_utils::abnormal::CheckExprError;
use flow_typing_utils::type_env;
use flow_typing_utils::type_operation_utils;
use flow_utils_concurrency::job_error::JobError;

use crate::func_params;
use crate::statement;

struct FuncScopeVisitor<'a, 'cx, 'b> {
    cx: &'a mut Context<'cx>,
    has_return_annot: bool,
    ret_annot_loc: ALoc,
    return_t: &'b Type,
    yield_t: &'b Type,
    next_t: &'b Type,
    body_loc: ALoc,
    kind: &'b Kind,
    exhaust: &'b Option<(RefCell<Vec<Type>>, Vec<ALoc>, (Type, type_::UseT<Cx>))>,
    no_return: bool,
    has_throw: bool,
    no_yield: bool,
}

impl<'ast> AstVisitor<'ast, ALoc, (ALoc, Type), &'ast ALoc, JobError>
    for FuncScopeVisitor<'_, '_, '_>
{
    fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
        loc
    }

    fn normalize_type(type_: &'ast (ALoc, Type)) -> &'ast ALoc {
        &type_.0
    }

    fn function_(
        &mut self,
        _loc: &'ast ALoc,
        _expr: &'ast ast::function::Function<ALoc, (ALoc, Type)>,
    ) -> Result<(), JobError> {
        Ok(())
    }

    fn component_declaration(
        &mut self,
        _loc: &'ast ALoc,
        _component: &'ast ast::statement::ComponentDeclaration<ALoc, (ALoc, Type)>,
    ) -> Result<(), JobError> {
        Ok(())
    }

    fn switch(
        &mut self,
        loc: &'ast ALoc,
        switch: &'ast ast::statement::Switch<ALoc, (ALoc, Type)>,
    ) -> Result<(), JobError> {
        let (ref exhaust_loc, ref t) = switch.exhaustive_out;
        if let Some((exhaustive_ts, exhaust_locs, _)) = self.exhaust {
            if exhaust_locs.contains(exhaust_loc) {
                exhaustive_ts.borrow_mut().push(t.dupe());
            }
        }
        ast_visitor::switch_default(self, loc, switch)
    }

    fn yield_(
        &mut self,
        loc: &'ast (ALoc, Type),
        yield_expr: &'ast ast::expression::Yield<ALoc, (ALoc, Type)>,
    ) -> Result<(), JobError> {
        let (_, ref t) = yield_expr.result_out;
        let use_op = UseOp::Op(Arc::new(type_::RootUseOp::GeneratorYield {
            value: match &yield_expr.argument {
                Some(expr) => {
                    let mut mapper = flow_typing_utils::typed_ast_utils::UntypedAstMapper;
                    let Ok(untyped_expr) = polymorphic_ast_mapper::expression(&mut mapper, expr);
                    reason::mk_expression_reason(&untyped_expr)
                }
                None => type_util::reason_of_t(t).dupe(),
            },
        }));
        flow_js::flow_non_speculating(
            self.cx,
            (
                t,
                &type_::UseT::new(type_::UseTInner::UseT(use_op, self.yield_t.dupe())),
            ),
        )?;
        self.no_yield = false;
        ast_visitor::yield_default(self, loc, yield_expr)
    }

    // Override statement so that we have the loc for return
    fn statement(
        &mut self,
        stmt: &'ast ast::statement::Statement<ALoc, (ALoc, Type)>,
    ) -> Result<(), JobError> {
        match &**stmt {
            ast::statement::StatementInner::Return {
                inner: return_stmt, ..
            } => {
                let loc = stmt.loc().dupe();
                self.custom_return(loc, return_stmt)?;
            }
            ast::statement::StatementInner::Throw { .. } => {
                self.has_throw = true;
            }
            _ => {}
        }
        ast_visitor::statement_default(self, stmt)
    }
}

impl FuncScopeVisitor<'_, '_, '_> {
    fn visit(
        &mut self,
        statements: &[ast::statement::Statement<ALoc, (ALoc, Type)>],
    ) -> Result<(), JobError> {
        self.statement_list(statements)?;
        if !self.has_return_annot {
            if self.no_return && self.has_throw {
                let reason = reason::mk_reason(None, self.body_loc.dupe());
                let t = type_::empty_t::make(reason);
                flow_js::flow_non_speculating(
                    self.cx,
                    (
                        &t,
                        &type_::UseT::new(type_::UseTInner::UseT(
                            type_::unknown_use(),
                            self.return_t.dupe(),
                        )),
                    ),
                )?;
            }
            if self.no_yield {
                flow_js::flow_t_non_speculating(
                    self.cx,
                    (
                        &type_::void::make(reason::mk_reason(
                            reason::VirtualReasonDesc::RVoid,
                            self.body_loc.dupe(),
                        )),
                        self.yield_t,
                    ),
                )?;
            }
        }
        Ok(())
    }

    fn custom_return(
        &mut self,
        loc: ALoc,
        return_stmt: &ast::statement::Return<ALoc, (ALoc, Type)>,
    ) -> Result<(), JobError> {
        let (_, ref t) = return_stmt.return_out;
        let t = match self.kind {
            Kind::Async => {
                // Convert the return expression's type T to Promise<T>. If the
                // expression type is itself a Promise<T>, ensure we still return
                // a Promise<T> via Promise.resolve.
                let async_return_reason = reason::mk_reason(None, loc.dupe());
                let awaited =
                    type_operation_utils::promise::await_(self.cx, &async_return_reason, t)?;
                let t_prime = flow_js::FlowJs::get_builtin_typeapp(
                    self.cx,
                    &reason::mk_reason(type_util::desc_of_t(t).cloned(), loc.dupe()),
                    None,
                    "Promise",
                    vec![awaited],
                );
                flow_js::reposition_non_speculating(self.cx, loc.dupe(), t_prime)?
            }
            Kind::Generator { .. } => {
                // Convert the return expression's type R to Generator<Y,R,N>, where
                // Y and R are internals, installed earlier.
                let t_prime = flow_js::FlowJs::get_builtin_typeapp(
                    self.cx,
                    &reason::mk_reason(type_util::desc_of_t(t).cloned(), loc.dupe()),
                    None,
                    "Generator",
                    vec![self.yield_t.dupe(), t.dupe(), self.next_t.dupe()],
                );
                flow_js::reposition_non_speculating(self.cx, loc.dupe(), t_prime)?
            }
            Kind::AsyncGenerator { .. } => {
                let t_prime = flow_js::FlowJs::get_builtin_typeapp(
                    self.cx,
                    &reason::mk_reason(type_util::desc_of_t(t).cloned(), loc.dupe()),
                    None,
                    "AsyncGenerator",
                    vec![self.yield_t.dupe(), t.dupe(), self.next_t.dupe()],
                );
                flow_js::reposition_non_speculating(self.cx, loc.dupe(), t_prime)?
            }
            _ => t.dupe(),
        };
        let use_op = UseOp::Op(Arc::new(type_::RootUseOp::FunReturnStatement {
            value: match &return_stmt.argument {
                Some(expr) => {
                    let mut mapper = flow_typing_utils::typed_ast_utils::UntypedAstMapper;
                    let Ok(untyped_expr) = polymorphic_ast_mapper::expression(&mut mapper, expr);
                    reason::mk_expression_reason(&untyped_expr)
                }
                None => type_util::reason_of_t(&t).dupe(),
            },
        }));
        if !self.has_return_annot {
            self.cx
                .add_missing_local_annot_lower_bound(self.ret_annot_loc.dupe(), t.dupe());
        }
        flow_js::flow_non_speculating(
            self.cx,
            (
                &t,
                &type_::UseT::new(type_::UseTInner::UseT(use_op, self.return_t.dupe())),
            ),
        )?;
        self.no_return = false;
        Ok(())
    }
}

pub fn this_param<C: crate::func_params_intf::Config>(fparams: &param::Param<C>) -> Option<Type> {
    func_params::this::<C>(fparams.this_.as_ref())
}

pub fn default_constructor<C: ConfigTypes>(reason: Reason) -> Func<C> {
    Func {
        reason: reason.dupe(),
        function_reference_kind: FunctionReferenceKind::DefaultConstructor,
        kind: Kind::Ctor,
        tparams: None,
        fparams: func_params::empty(Rc::new(|_, _, _| None)),
        body: None,
        return_t: AnnotatedOrInferred::Annotated(type_::void::why(reason.dupe())),
        effect_: type_::ReactEffectType::ArbitraryEffect,
        ret_annot_loc: reason.loc().dupe(),
        statics: None,
        literal: false,
    }
}

pub fn field_initializer<C: ConfigTypes>(
    reason: Reason,
    name: FlowSmolStr,
    expr: ast::expression::Expression<ALoc, ALoc>,
    annot_loc: ALoc,
    return_annot_or_inferred: AnnotatedOrInferred,
    fills_annotated_symbol: bool,
) -> Func<C> {
    Func {
        reason,
        function_reference_kind: FunctionReferenceKind::FunctionType,
        kind: Kind::FieldInit {
            name,
            expr,
            fills_annotated_symbol,
        },
        tparams: None,
        fparams: func_params::empty(Rc::new(|_, _, _| None)),
        body: None,
        return_t: return_annot_or_inferred,
        effect_: type_::ReactEffectType::ArbitraryEffect,
        ret_annot_loc: annot_loc,
        statics: None,
        literal: false,
    }
}

pub fn functiontype<'a, C: crate::func_params_intf::Config>(
    cx: &mut Context<'a>,
    arrow: bool,
    func_loc: Option<ALoc>,
    this_default: Type,
    x: &Func<C>,
) -> Type {
    let Func {
        reason,
        kind,
        tparams,
        fparams,
        return_t,
        statics,
        effect_,
        ..
    } = x;
    let this_type = func_params::this::<C>(fparams.this_.as_ref()).unwrap_or(this_default.dupe());
    let return_t = match return_t {
        AnnotatedOrInferred::Inferred(t)
            if !matches!(
                cx.typing_mode(),
                flow_typing_context::TypingMode::CheckingMode
            ) && !func_params::all_params_annotated::<C>(
                &fparams.params,
                fparams.rest.as_ref(),
            ) =>
        {
            cx.mk_placeholder(type_util::reason_of_t(t).dupe())
        }
        _ => type_util::type_t_of_annotated_or_inferred(return_t).dupe(),
    };
    let type_guard = match kind {
        Kind::TypeGuard(p) => Some(p.dupe()),
        _ => None,
    };
    let funtype = type_::FunType {
        this_t: (this_type.dupe(), type_::ThisStatus::ThisFunction),
        params: func_params::value::<C>(&fparams.params).into(),
        rest_param: func_params::rest::<C>(fparams.rest.as_ref()),
        return_t,
        effect_: effect_.clone(),
        type_guard,
        def_reason: reason.dupe(),
        function_reference_kind: x.function_reference_kind,
        strictness_kind: cx.type_strictness_kind(),
        literal: x.literal,
    };
    let statics_t = match statics {
        Some(t) => t.dupe(),
        None => obj_type::mk_with_proto(
            cx,
            reason.dupe(),
            type_::ObjKind::Inexact,
            None,
            None,
            None,
            None,
            Type::new(type_::TypeInner::FunProtoT(reason.dupe())),
            x.literal,
        ),
    };
    let t = Type::new(type_::TypeInner::DefT(
        reason.dupe(),
        type_::DefT::new(type_::DefTInner::FunT(statics_t, Rc::new(funtype))),
    ));
    if !arrow {
        if let Some(loc) = func_loc {
            type_env::bind_function_this(cx, this_type, loc);
        }
    }
    type_util::poly_type_of_tparams(
        type_::poly::Id::generate_id(),
        tparams.clone(),
        t,
        cx.type_strictness_kind(),
        PolyTKind::Regular,
    )
}

pub fn methodtype<'a, C: crate::func_params_intf::Config>(
    cx: &mut Context<'a>,
    method_this_loc: Option<ALoc>,
    this_default: Type,
    x: &Func<C>,
) -> Type {
    let Func {
        reason,
        kind,
        tparams,
        fparams,
        return_t,
        effect_,
        ..
    } = x;
    let params = func_params::value::<C>(&fparams.params);
    let mut params_names: Vec<Option<Name>> = Vec::new();
    let mut params_tlist = Vec::new();
    for type_::FunParam(name, t) in params {
        params_names.push(name.map(Name::new));
        params_tlist.push(t);
    }
    let rest_param = func_params::rest::<C>(fparams.rest.as_ref());
    let this_anno_t = func_params::this::<C>(fparams.this_.as_ref());
    if let (Some(t), Some(loc)) = (&this_anno_t, method_this_loc) {
        type_env::bind_function_this(cx, t.dupe(), loc);
    }
    let param_this_t = this_anno_t.unwrap_or(this_default);
    let type_guard = match kind {
        Kind::TypeGuard(g) => Some(g.dupe()),
        _ => None,
    };
    let mut funtype = type_::mk_boundfunctiontype(
        param_this_t,
        Some(effect_.clone()),
        params_tlist,
        rest_param,
        reason.dupe(),
        x.function_reference_kind,
        Some(params_names),
        type_guard,
        type_util::type_t_of_annotated_or_inferred(return_t).dupe(),
    );
    funtype.strictness_kind = cx.type_strictness_kind();
    if fparams.this_.is_some() {
        funtype.this_t.1 = type_::ThisStatus::ThisMethodWithExplicitThis;
    }
    funtype.literal = x.literal;
    let t = Type::new(type_::TypeInner::DefT(
        reason.dupe(),
        type_::DefT::new(type_::DefTInner::FunT(
            type_::dummy_static(reason.dupe()),
            Rc::new(funtype),
        )),
    ));
    type_util::poly_type_of_tparams(
        type_::poly::Id::generate_id(),
        tparams.clone(),
        t,
        cx.type_strictness_kind(),
        PolyTKind::Regular,
    )
}

pub fn gettertype<C: ConfigTypes>(x: &Func<C>) -> Type {
    type_util::type_t_of_annotated_or_inferred(&x.return_t).dupe()
}

pub fn settertype<C: crate::func_params_intf::Config>(x: &Func<C>) -> Type {
    let params = func_params::value::<C>(&x.fparams.params);
    match params.as_slice() {
        // setters must have exactly one param. more than one is a syntax error,
        // so ignore any extras
        [FunParam(_, param_t), ..] => param_t.dupe(),
        [] => type_::any_t::error(x.reason.dupe()),
    }
}

pub fn toplevels<'a, C: crate::func_params_intf::Config>(
    cx: &mut Context<'a>,
    x: &Func<C>,
) -> Result<
    (
        Option<C::Ast>,
        Option<FunctionBody<ALoc, (ALoc, Type)>>,
        Option<ast::expression::Expression<ALoc, (ALoc, Type)>>,
    ),
    CheckExprError,
> {
    let Func {
        reason: reason_fn,
        kind,
        fparams,
        body,
        ret_annot_loc,
        return_t,
        ..
    } = x;
    let ret_annot_loc = ret_annot_loc.dupe();
    let body_loc = match body {
        Some(FunctionBody::BodyBlock((loc, _))) => loc.dupe(),
        Some(FunctionBody::BodyExpression(expr)) => expr.loc().dupe(),
        None => ALoc::none(),
    };
    let reason = reason::mk_reason(None, body_loc.dupe());

    // Set the scope early so default exprs can reference earlier params
    let prev_scope_kind = {
        let var_scope_kind = match kind {
            Kind::Ordinary | Kind::FieldInit { .. } | Kind::TypeGuard(_) => {
                flow_env_builder::name_def_types::ScopeKind::Ordinary
            }
            Kind::Async => flow_env_builder::name_def_types::ScopeKind::Async,
            Kind::Generator { .. } => flow_env_builder::name_def_types::ScopeKind::Generator,
            Kind::AsyncGenerator { .. } => {
                flow_env_builder::name_def_types::ScopeKind::AsyncGenerator
            }
            Kind::Ctor => flow_env_builder::name_def_types::ScopeKind::Ctor,
        };
        type_env::set_scope_kind(cx, var_scope_kind)
    };

    let params_ast = func_params::eval::<C, _>(
        cx,
        &fparams.params,
        fparams.rest.as_ref(),
        fparams.this_.as_ref(),
        &*fparams.reconstruct,
    )?;

    let (yield_t, next_t) = match kind {
        Kind::Generator { .. } | Kind::AsyncGenerator { .. } => {
            let yield_t = flow_typing_tvar::mk(cx, reason.dupe().replace_desc(None));
            let next_t = match return_t {
                AnnotatedOrInferred::Annotated(_) => {
                    flow_typing_tvar::mk(cx, reason.dupe().replace_desc(None))
                }
                AnnotatedOrInferred::Inferred(_) => {
                    type_::void::make(reason.dupe().replace_desc(reason::VirtualReasonDesc::RVoid))
                }
            };
            let return_targ = flow_typing_tvar::mk(cx, reason.dupe());
            let (iterable, generator) = match kind {
                Kind::Generator { .. } => ("Iterable", "Generator"),
                Kind::AsyncGenerator { .. } => ("AsyncIterable", "AsyncGenerator"),
                _ => unreachable!("Bad kind"),
            };
            {
                let t = flow_js::FlowJs::get_builtin_typeapp(
                    cx,
                    &reason,
                    None,
                    iterable,
                    vec![yield_t.dupe(), return_targ.dupe(), next_t.dupe()],
                );
                let t = flow_js::reposition_non_speculating(
                    cx,
                    type_util::reason_of_t(type_util::type_t_of_annotated_or_inferred(return_t))
                        .loc()
                        .dupe(),
                    t,
                )?;
                flow_js::flow_t_non_speculating(
                    cx,
                    (type_util::type_t_of_annotated_or_inferred(return_t), &t),
                )?;
            }
            {
                let t = flow_js::FlowJs::get_builtin_typeapp(
                    cx,
                    &reason,
                    None,
                    generator,
                    vec![yield_t.dupe(), return_targ, next_t.dupe()],
                );
                let t = flow_js::reposition_non_speculating(
                    cx,
                    type_util::reason_of_t(type_util::type_t_of_annotated_or_inferred(return_t))
                        .loc()
                        .dupe(),
                    t,
                )?;
                flow_js::flow_t_non_speculating(
                    cx,
                    (&t, type_util::type_t_of_annotated_or_inferred(return_t)),
                )?;
            }
            (yield_t, next_t)
        }
        _ => (
            Type::new(type_::TypeInner::DefT(
                reason.dupe().replace_desc(None),
                type_::DefT::new(type_::DefTInner::MixedT(
                    type_::MixedFlavor::MixedEverything,
                )),
            )),
            Type::new(type_::TypeInner::DefT(
                reason.dupe().replace_desc(None),
                type_::DefT::new(type_::DefTInner::MixedT(
                    type_::MixedFlavor::MixedEverything,
                )),
            )),
        ),
    };

    enum ReconstructBody {
        None,
        BodyBlock(ALoc, Option<ast::Syntax<ALoc, Arc<[ast::Comment<ALoc>]>>>),
        BodyExpression,
    }
    let (statements, reconstruct_body, maybe_implicit_void_return): (
        Vec<ast::statement::Statement<ALoc, ALoc>>,
        ReconstructBody,
        bool,
    ) = match body {
        None => (vec![], ReconstructBody::None, true),
        Some(FunctionBody::BodyBlock((loc, block))) => (
            block.body.to_vec(),
            ReconstructBody::BodyBlock(loc.dupe(), block.comments.dupe()),
            true,
        ),
        Some(FunctionBody::BodyExpression(expr)) => {
            let return_stmt =
                ast::statement::Statement(Arc::new(ast::statement::StatementInner::Return {
                    loc: expr.loc().dupe(),
                    inner: Arc::new(ast::statement::Return {
                        argument: Some(expr.dupe()),
                        comments: None,
                        return_out: expr.loc().dupe(),
                    }),
                }));
            (vec![return_stmt], ReconstructBody::BodyExpression, false)
        }
    };

    let (has_return_annot, return_t) = match return_t {
        AnnotatedOrInferred::Inferred(t) => (false, t.dupe()),
        AnnotatedOrInferred::Annotated(t) => (true, t.dupe()),
    };

    // statement visit pass
    let statements_ast = statement::statement_list(cx, &statements)?;

    let body_ast = match reconstruct_body {
        ReconstructBody::None => None,
        ReconstructBody::BodyBlock(loc, comments) => Some(FunctionBody::BodyBlock((
            loc,
            ast::statement::Block {
                body: Arc::from(statements_ast.as_slice()),
                comments,
            },
        ))),
        ReconstructBody::BodyExpression => match statements_ast.as_slice() {
            [stmt] => match &**stmt {
                ast::statement::StatementInner::Return { inner: ret, .. }
                    if let Some(argument) = &ret.argument =>
                {
                    Some(FunctionBody::BodyExpression(argument.clone()))
                }
                ast::statement::StatementInner::Expression {
                    inner: expr_stmt, ..
                } => Some(FunctionBody::BodyExpression(expr_stmt.expression.clone())),
                _ => panic!("expected return body"),
            },
            _ => panic!("expected return body"),
        },
    };

    // build return type for void funcs
    let (init_ast, exhaust) = if maybe_implicit_void_return {
        let loc = type_util::loc_of_t(&return_t).dupe();
        let (use_op, void_t, init_ast): (
            UseOp,
            Type,
            Option<ast::expression::Expression<ALoc, (ALoc, Type)>>,
        ) = match kind {
            Kind::Ordinary | Kind::Ctor => {
                let t = type_::void::at(loc.dupe());
                let use_op = UseOp::Op(Arc::new(type_::RootUseOp::FunImplicitReturn(Box::new(
                    type_::FunImplicitReturnData {
                        fn_: reason_fn.dupe(),
                        upper: type_util::reason_of_t(&return_t).dupe(),
                        upper_t: flow_js_utils::type_or_type_desc_for_error(&return_t),
                        type_guard: false,
                    },
                ))));
                (use_op, t, None)
            }
            Kind::Async => {
                let reason = reason::mk_annot_reason(
                    reason::VirtualReasonDesc::RType("Promise".into()),
                    loc.dupe(),
                );
                let void_t_val = type_::void::at(loc.dupe());
                let t = flow_js::FlowJs::get_builtin_typeapp(
                    cx,
                    &reason,
                    None,
                    "Promise",
                    vec![void_t_val],
                );
                let use_op = UseOp::Op(Arc::new(type_::RootUseOp::FunImplicitReturn(Box::new(
                    type_::FunImplicitReturnData {
                        fn_: reason_fn.dupe(),
                        upper: type_util::reason_of_t(&return_t).dupe(),
                        upper_t: flow_js_utils::type_or_type_desc_for_error(&return_t),
                        type_guard: false,
                    },
                ))));
                let use_op = UseOp::Frame(
                    Arc::new(type_::FrameUseOp::ImplicitTypeParam),
                    Arc::new(use_op),
                );
                (use_op, t, None)
            }
            Kind::Generator { .. } => {
                let reason = reason::mk_annot_reason(
                    reason::VirtualReasonDesc::RType("Generator".into()),
                    loc.dupe(),
                );
                let void_t_val = type_::void::at(loc.dupe());
                let t = flow_js::FlowJs::get_builtin_typeapp(
                    cx,
                    &reason,
                    None,
                    "Generator",
                    vec![yield_t.dupe(), void_t_val, next_t.dupe()],
                );
                let use_op = UseOp::Op(Arc::new(type_::RootUseOp::FunImplicitReturn(Box::new(
                    type_::FunImplicitReturnData {
                        fn_: reason_fn.dupe(),
                        upper: type_util::reason_of_t(&return_t).dupe(),
                        upper_t: flow_js_utils::type_or_type_desc_for_error(&return_t),
                        type_guard: false,
                    },
                ))));
                let use_op = UseOp::Frame(
                    Arc::new(type_::FrameUseOp::ImplicitTypeParam),
                    Arc::new(use_op),
                );
                (use_op, t, None)
            }
            Kind::AsyncGenerator { .. } => {
                let reason = reason::mk_annot_reason(
                    reason::VirtualReasonDesc::RType("AsyncGenerator".into()),
                    loc.dupe(),
                );
                let void_t_val = type_::void::at(loc.dupe());
                let t = flow_js::FlowJs::get_builtin_typeapp(
                    cx,
                    &reason,
                    None,
                    "AsyncGenerator",
                    vec![yield_t.dupe(), void_t_val, next_t.dupe()],
                );
                let use_op = UseOp::Op(Arc::new(type_::RootUseOp::FunImplicitReturn(Box::new(
                    type_::FunImplicitReturnData {
                        fn_: reason_fn.dupe(),
                        upper: type_util::reason_of_t(&return_t).dupe(),
                        upper_t: flow_js_utils::type_or_type_desc_for_error(&return_t),
                        type_guard: false,
                    },
                ))));
                let use_op = UseOp::Frame(
                    Arc::new(type_::FrameUseOp::ImplicitTypeParam),
                    Arc::new(use_op),
                );
                (use_op, t, None)
            }
            Kind::FieldInit {
                name,
                expr: e,
                fills_annotated_symbol,
            } => {
                let ast = statement::expression(
                    Some(flow_common::enclosing_context::EnclosingContext::NoContext),
                    None,
                    None,
                    cx,
                    e,
                )?;
                // The field is annotated with a bare `unique symbol` and this
                // initializer is what produces it, so the call stands for the
                // annotated symbol rather than one of its own. See the same
                // rule on a variable declaration in `statement::variable`.
                let t = if *fills_annotated_symbol && statement::is_symbol_constructor_call(cx, e) {
                    return_t.dupe()
                } else {
                    ast.loc().1.dupe()
                };
                let body = reason::mk_expression_reason(e);
                let use_op = UseOp::Op(Arc::new(type_::RootUseOp::InitField(Box::new(
                    type_::InitFieldData {
                        op: reason_fn.dupe(),
                        field: name.dupe(),
                        body,
                    },
                ))));
                (use_op, t, Some(ast))
            }
            Kind::TypeGuard(_) => {
                let t = type_::void::at(loc.dupe());
                let use_op = UseOp::Op(Arc::new(type_::RootUseOp::FunImplicitReturn(Box::new(
                    type_::FunImplicitReturnData {
                        fn_: reason_fn.dupe(),
                        upper: type_util::reason_of_t(&return_t).dupe(),
                        upper_t: flow_js_utils::type_or_type_desc_for_error(&return_t),
                        type_guard: true,
                    },
                ))));
                (use_op, t, None)
            }
        };

        let exhaust = match body {
            None => {
                flow_js::flow_non_speculating(
                    cx,
                    (
                        &void_t,
                        &type_::UseT::new(type_::UseTInner::UseT(use_op.dupe(), return_t.dupe())),
                    ),
                )?;
                None
            }
            Some(_) => {
                let (exhaustive, undeclared) = {
                    match cx.exhaustive_check(&body_loc) {
                        Some((locs, undeclared)) => (locs, undeclared),
                        None => {
                            flow_js_utils::add_output_non_speculating(
                                cx,
                                error_message::ErrorMessage::EInternal(Box::new((
                                    loc.dupe(),
                                    error_message::InternalError::MissingSwitchExhaustiveCheck,
                                ))),
                            );
                            (vec![], false)
                        }
                    }
                };
                Some((
                    RefCell::new(if undeclared {
                        vec![type_::void::at(body_loc.dupe())]
                    } else {
                        vec![]
                    }),
                    exhaustive,
                    (
                        void_t,
                        type_::UseT::new(type_::UseTInner::<Cx>::UseT(use_op, return_t.dupe())),
                    ),
                ))
            }
        };
        (init_ast, exhaust)
    } else {
        (None, None)
    };

    FuncScopeVisitor {
        cx,
        has_return_annot,
        ret_annot_loc: ret_annot_loc.dupe(),
        return_t: &return_t,
        yield_t: &yield_t,
        next_t: &next_t,
        body_loc: body_loc.dupe(),
        kind,
        exhaust: &exhaust,
        no_return: true,
        has_throw: false,
        no_yield: true,
    }
    .visit(&statements_ast)?;

    if let Some((maybe_exhaustively_checked_ts, _, flow_on_non_exhaustive)) = &exhaust {
        let ts = maybe_exhaustively_checked_ts.borrow();
        if type_operation_utils::type_assertions::non_exhaustive(cx, &ts)? {
            match flow_js::flow(cx, (&flow_on_non_exhaustive.0, &flow_on_non_exhaustive.1)) {
                Ok(()) => {}
                Err(FlowJsException::WorkerCanceled(c)) => {
                    return Err(c.into());
                }
                Err(FlowJsException::TimedOut(c)) => {
                    return Err(c.into());
                }
                Err(err) => panic!("Should not be under speculation: {:?}", err),
            }
        }
    }

    type_env::set_scope_kind(cx, prev_scope_kind);

    Ok((params_ast, body_ast, init_ast))
}

pub fn to_ctor_sig<C: ConfigTypes>(mut f: Func<C>) -> Func<C> {
    f.kind = Kind::Ctor;
    f
}

pub fn return_loc(func: &Function<ALoc, ALoc>) -> ALoc {
    match &func.return_ {
        ReturnAnnot::Available(annot) => annot.annotation.loc().dupe(),
        ReturnAnnot::TypeGuard(tg) => tg.loc.dupe(),
        ReturnAnnot::Missing(_) => match &func.body {
            FunctionBody::BodyExpression(expr) => expr.loc().dupe(),
            FunctionBody::BodyBlock((loc, _)) => ALoc::of_loc(loc.to_loc_exn().char_before()),
        },
    }
}
