/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::RefCell;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_typing_context::Context;
use flow_typing_errors::intermediate_error_types::ComponentReferenceData;
use flow_typing_loc_env::component_sig_types::component_sig::ComponentParamsTast;
use flow_typing_loc_env::component_sig_types::component_sig::ComponentSig;
use flow_typing_loc_env::component_sig_types::declaration_body_config;
use flow_typing_type::type_;
use flow_typing_type::type_::PolyTKind;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_util;
use flow_typing_utils::abnormal::CheckExprError;
use flow_utils_concurrency::job_error::JobError;

use crate::component_params;
use crate::statement;

pub mod component_declaration_body {
    use flow_common::reason;
    use flow_parser::polymorphic_ast_mapper;
    use flow_typing_errors::error_message;
    use flow_typing_flow_common::flow_js_utils;
    use flow_typing_flow_js::flow_js;
    use flow_typing_type::type_;
    use flow_typing_utils::type_operation_utils;
    use flow_typing_utils::typed_ast_utils;

    use super::*;

    struct ComponentScopeVisitor<'a, 'cx, 'b> {
        cx: &'a mut Context<'cx>,
        body_loc: ALoc,
        renders_t: &'b Type,
        exhaust: &'b Option<(RefCell<Vec<Type>>, Vec<ALoc>)>,
    }

    impl<'ast> AstVisitor<'ast, ALoc, (ALoc, Type), &'ast ALoc, JobError>
        for ComponentScopeVisitor<'_, '_, '_>
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
            if let Some((exhaustive_ts, exhaust_locs)) = self.exhaust {
                if exhaust_locs.contains(exhaust_loc) {
                    exhaustive_ts.borrow_mut().push(t.dupe());
                }
            }
            ast_visitor::switch_default(self, loc, switch)
        }

        // Override statement so that we have the loc for renders
        fn statement(
            &mut self,
            stmt: &'ast ast::statement::Statement<ALoc, (ALoc, Type)>,
        ) -> Result<(), JobError> {
            match &**stmt {
                ast::statement::StatementInner::Return {
                    inner: return_stmt, ..
                } => {
                    self.custom_return(return_stmt)?;
                }
                _ => {}
            }
            ast_visitor::statement_default(self, stmt)
        }
    }

    impl ComponentScopeVisitor<'_, '_, '_> {
        fn visit(
            &mut self,
            statements: &[ast::statement::Statement<ALoc, (ALoc, Type)>],
        ) -> Result<(), JobError> {
            self.statement_list(statements)
        }

        fn custom_return(
            &mut self,
            return_stmt: &ast::statement::Return<ALoc, (ALoc, Type)>,
        ) -> Result<(), JobError> {
            let (_, ref t) = return_stmt.return_out;
            let use_op = UseOp::Op(std::sync::Arc::new(type_::RootUseOp::FunReturnStatement {
                value: match &return_stmt.argument {
                    Some(expr) => {
                        let mut mapper = typed_ast_utils::UntypedAstMapper;
                        let Ok(untyped_expr) =
                            polymorphic_ast_mapper::expression(&mut mapper, expr);
                        type_::ValueReference::Expression(Box::new(
                            reason::mk_expression_reference(&untyped_expr),
                        ))
                    }
                    None => type_::ValueReference::Reason(type_util::reason_of_t(t).dupe()),
                },
            }));
            self.cx
                .add_inferred_component_return(self.body_loc.dupe(), t.dupe());
            flow_js::flow_non_speculating(
                self.cx,
                (
                    t,
                    &type_::UseT::new(type_::UseTInner::UseT(use_op, self.renders_t.dupe())),
                ),
            )?;
            Ok(())
        }
    }

    pub fn eval<'a>(
        cx: &mut Context<'a>,
        component: ComponentReferenceData<ALoc>,
        renders_t: Type,
        body: declaration_body_config::Body<ALoc>,
    ) -> Result<declaration_body_config::Body<(ALoc, Type)>, JobError> {
        let (body_loc, body_block) = body;
        let statements = &body_block.body;

        // statement visit pass
        let statements_ast = statement::statement_list(cx, statements)?;

        let exhaust = {
            let (exhaustive, undeclared) = {
                match cx.exhaustive_check(&body_loc) {
                    Some(result) => result,
                    None => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            error_message::ErrorMessage::EInternal(Box::new((
                                body_loc.dupe(),
                                error_message::InternalError::MissingSwitchExhaustiveCheck,
                            ))),
                        );
                        (vec![], false)
                    }
                }
            };
            if undeclared {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    error_message::ErrorMessage::EComponentMissingReturn(component.dupe()),
                );
                None
            } else {
                Some((RefCell::new(vec![]), exhaustive))
            }
        };
        let body_ast = ast::statement::Block {
            body: Arc::from(statements_ast),
            comments: body_block.comments.dupe(),
        };
        ComponentScopeVisitor {
            cx,
            body_loc: body_loc.dupe(),
            renders_t: &renders_t,
            exhaust: &exhaust,
        }
        .visit(&body_ast.body)?;
        if let Some((maybe_exhaustively_checked_ts, _)) = &exhaust {
            let ts = maybe_exhaustively_checked_ts.borrow();
            if type_operation_utils::type_assertions::non_exhaustive(cx, &ts)? {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    error_message::ErrorMessage::EComponentMissingReturn(component),
                );
            }
        }
        Ok((body_loc, body_ast))
    }
}

pub fn toplevels<'a>(
    cx: &mut Context<'a>,
    x: &ComponentSig,
) -> Result<
    (
        ComponentParamsTast,
        declaration_body_config::Body<(ALoc, Type)>,
    ),
    CheckExprError,
> {
    let ComponentSig {
        reason: reason_cmp,
        cparams,
        body,
        ret_annot_loc: _,
        renders_t,
        id_opt,
        ..
    } = x;
    let renders_t = renders_t.dupe();
    let component = ComponentReferenceData {
        loc: reason_cmp.loc().dupe(),
        name: id_opt.as_ref().map(|(_, name)| name.dupe()),
    };
    // add param bindings
    let params_ast = component_params::eval::<component_params::DeclarationConfig, _>(
        cx,
        &cparams.params,
        cparams.rest.as_ref(),
        &*cparams.reconstruct,
    )?;
    let body_ast = component_declaration_body::eval(cx, component, renders_t, body.clone())?;
    Ok((params_ast, body_ast))
}

pub fn component_type<'a, C: crate::component_params_intf::Config>(
    cx: &mut Context<'a>,
    in_annotation: bool,
    reason: &flow_common::reason::Reason,
    tparams: type_::TypeParams,
    params: &[C::Param],
    rest: Option<&C::Rest>,
    renders_t: Type,
    id_opt: Option<(&ALoc, &FlowSmolStr)>,
) -> Result<Type, JobError> {
    let config_reason = reason.dupe().replace_desc(None);
    let config = component_params::config::<C>(cx, in_annotation, &config_reason, params, rest)?;
    let component_kind = match id_opt {
        None => type_::ComponentKind::Structural,
        Some((id_loc, name)) => {
            let nominal_id = cx.make_aloc_id(id_loc);
            type_::ComponentKind::Nominal(nominal_id, name.dupe(), None)
        }
    };
    let t = Type::new(type_::TypeInner::DefT(
        reason.dupe(),
        type_::DefT::new(type_::DefTInner::ReactAbstractComponentT(Box::new(
            type_::ReactAbstractComponentTData {
                config,
                renders: renders_t,
                component_kind,
            },
        ))),
    ));
    Ok(type_util::poly_type_of_tparams(
        type_::poly::Id::generate_id(),
        tparams,
        t,
        cx.type_strictness_kind(),
        PolyTKind::Regular,
    ))
}
