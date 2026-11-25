/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::RefCell;
use std::rc::Rc;

use flow_aloc::ALoc;
use flow_common::error_ref::ExpressionReferenceData;
use flow_common::reason::Reason;
use flow_typing_type::type_::CallArg;
use flow_typing_type::type_::FuncallType;
use flow_typing_type::type_::MethodCallType;
use flow_typing_type::type_::PropRef;
use flow_typing_type::type_::SpecializedCallee;
use flow_typing_type::type_::SpeculationHintState;
use flow_typing_type::type_::Targ;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeParam;
use flow_typing_type::type_::TypeStrictnessKind;
use flow_typing_type::type_::UseOp;
use vec1::Vec1;

/// The type parameters, the body and the strictness of a polymorphic type.
pub type PolyT = (ALoc, Vec1<TypeParam>, Type, TypeStrictnessKind);

/// Call inputs retained while solving implicit type arguments.
#[derive(Debug, Clone)]
pub struct Call {
    pub call_this_t: Type,
    pub call_targs: Option<Rc<[Targ]>>,
    pub call_args_tlist: Rc<[CallArg]>,
    pub call_strict_arity: bool,
    pub call_speculation_hint_state: Option<Rc<RefCell<SpeculationHintState>>>,
    pub call_specialized_callee: Option<SpecializedCallee>,
}

impl Call {
    pub fn from_funcall(call: FuncallType) -> Self {
        Self {
            call_this_t: call.call_this_t,
            call_targs: call.call_targs,
            call_args_tlist: call.call_args_tlist,
            call_strict_arity: call.call_strict_arity,
            call_speculation_hint_state: call.call_speculation_hint_state,
            call_specialized_callee: call.call_specialized_callee,
        }
    }
}

/// Optional call inputs retained while solving a polymorphic receiver.
#[derive(Debug, Clone)]
pub struct MethodOptionalChain {
    pub exp_reason: Reason,
    pub lhs_reason: Reason,
    pub lhs_expression: ExpressionReferenceData<ALoc>,
}

#[derive(Debug, Clone)]
pub enum Operation {
    SubtypeLowerPoly(Type),
    Method {
        prop_reason: Reason,
        propref: PropRef,
        call: MethodCallType,
        optional_chain: Option<MethodOptionalChain>,
    },
    Call(Call),
    Constructor(Option<Rc<[Targ]>>, Rc<[CallArg]>),
    ReactJSX {
        jsx_props: Type,
        targs: Option<Rc<[Targ]>>,
    },
}

#[derive(Debug, Clone)]
pub struct ImplicitInstantiationCheck {
    pub lhs: Type,
    pub poly_t: PolyT,
    pub operation: (UseOp, Reason, Operation),
}

impl ImplicitInstantiationCheck {
    pub fn of_call(lhs: Type, poly_t: PolyT, use_op: UseOp, reason: Reason, call: Call) -> Self {
        Self {
            lhs,
            poly_t,
            operation: (use_op, reason, Operation::Call(call)),
        }
    }

    pub fn of_ctor(
        lhs: Type,
        poly_t: PolyT,
        use_op: UseOp,
        reason_op: Reason,
        targs: Option<Rc<[Targ]>>,
        args: Rc<[CallArg]>,
    ) -> Self {
        Self {
            lhs,
            poly_t,
            operation: (use_op, reason_op, Operation::Constructor(targs, args)),
        }
    }

    pub fn of_react_jsx(
        lhs: Type,
        poly_t: PolyT,
        use_op: UseOp,
        reason_op: Reason,
        jsx_props: Type,
        targs: Option<Rc<[Targ]>>,
    ) -> Self {
        Self {
            lhs,
            poly_t,
            operation: (use_op, reason_op, Operation::ReactJSX { targs, jsx_props }),
        }
    }
}
