/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;
use std::rc::Rc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocFuzzy;
use flow_aloc::ALocFuzzySet;
use flow_aloc::ALocMap;
use flow_aloc::ALocSet;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_env_builder::env_api;
use flow_env_builder::env_api::RefinementKind;
use flow_env_builder::name_def_types;
use flow_env_builder::name_def_types::AnnotationData;
use flow_env_builder::name_def_types::Binding;
use flow_env_builder::name_def_types::ClassDefData;
use flow_env_builder::name_def_types::ContextualData;
use flow_env_builder::name_def_types::Def;
use flow_env_builder::name_def_types::FunctionDefData;
use flow_env_builder::name_def_types::FunctionValueData;
use flow_env_builder::name_def_types::MemberAssignData;
use flow_env_builder::name_def_types::OpAssignData;
use flow_env_builder::name_def_types::Root;
use flow_parser::ast;
use flow_parser::ast::expression::ExpressionInner;
use flow_parser::ast::expression::ExpressionOrSpread;
use flow_parser::ast::expression::member::Property;
use flow_parser::ast::expression::object::NormalProperty;
use flow_parser::ast::pattern::Pattern;
use flow_parser::ast::statement::export_default_declaration::Declaration;
use flow_parser::ast_utils;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_errors::error_message;
use flow_typing_errors::error_message::EHookRuleViolationData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::RefInRenderKind;
use flow_typing_errors::intermediate_error_types;
use flow_typing_errors::intermediate_error_types::ExpressionReferenceData;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_js::flow_js;
use flow_typing_type::type_;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::DroType;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::ReactEffectType;
use flow_typing_type::type_::TvarSeenSet;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::nominal;
use flow_typing_utils::type_env;
use flow_typing_utils::typed_ast_utils;

use crate::type_annotation;

pub fn check_ref_use<'a>(
    cx: &mut Context<'a>,
    rrid: Option<&type_::nominal::Id>,
    in_hook: bool,
    usage: ExpressionReferenceData<ALoc>,
    kind: RefInRenderKind,
    t: Type,
) -> Vec<ErrorMessage<ALoc>> {
    fn recur_id<'a>(
        cx: &mut Context<'a>,
        rrid: Option<&nominal::Id>,
        in_hook: bool,
        usage: &ExpressionReferenceData<ALoc>,
        kind: &RefInRenderKind,
        seen: &mut TvarSeenSet<u32>,
        t: &Type,
    ) -> Vec<ErrorMessage<ALoc>> {
        match t.deref() {
            TypeInner::DefT(_, def_t)
                if let DefTInner::ObjT(obj) = def_t.deref()
                    && matches!(&obj.flags.react_dro, Some(dro)
                        if matches!(dro.1, DroType::HookReturn | DroType::HookArg | DroType::Props))
                    && obj.call_t.is_none()
                    && *kind == RefInRenderKind::Access =>
            {
                let props = cx.find_props(obj.props_tmap.dupe());
                if props.len() == 1 {
                    // Catch only cases that look like { current: T }
                    vec![ErrorMessage::EReactRefInRender {
                        usage: usage.dupe(),
                        kind: *kind,
                        in_hook,
                    }]
                } else {
                    vec![]
                }
            }
            TypeInner::NominalT { nominal_type, .. }
                if rrid.is_some_and(|rrid| rrid == &nominal_type.nominal_id) =>
            {
                vec![ErrorMessage::EReactRefInRender {
                    usage: usage.dupe(),
                    kind: *kind,
                    in_hook,
                }]
            }
            TypeInner::NominalT { nominal_type, .. } => {
                let mut result = match &nominal_type.underlying_t {
                    nominal::UnderlyingT::OpaqueWithLocal { t } => {
                        recur_id(cx, rrid, in_hook, usage, kind, seen, t)
                    }
                    nominal::UnderlyingT::CustomError(box nominal::CustomErrorData {
                        t, ..
                    }) => recur_id(cx, rrid, in_hook, usage, kind, seen, t),
                    nominal::UnderlyingT::FullyOpaque => vec![],
                };
                if let Some(upper_t) = &nominal_type.upper_t {
                    result.extend(recur_id(cx, rrid, in_hook, usage, kind, seen, upper_t));
                }
                result
            }
            TypeInner::OpenT(tvar) if seen.contains(&tvar.id()) => {
                vec![]
            }
            TypeInner::OpenT(tvar) => seen.with_added(tvar.id(), |seen| {
                flow_js_utils::possible_types(cx, tvar.id() as i32)
                    .iter()
                    .flat_map(|t| recur_id(cx, rrid, in_hook, usage, kind, seen, t))
                    .collect()
            }),
            TypeInner::UnionT(_, rep) => rep
                .members_iter()
                .flat_map(|t| recur_id(cx, rrid, in_hook, usage, kind, seen, t))
                .collect(),
            TypeInner::IntersectionT(_, rep) => rep
                .members_iter()
                .flat_map(|t| recur_id(cx, rrid, in_hook, usage, kind, seen, t))
                .collect(),
            TypeInner::MaybeT(_, inner_t) => {
                recur_id(cx, rrid, in_hook, usage, kind, seen, inner_t)
            }
            TypeInner::OptionalT { type_: inner_t, .. } => {
                recur_id(cx, rrid, in_hook, usage, kind, seen, inner_t)
            }
            TypeInner::AnnotT(_, inner_t, _) => {
                recur_id(cx, rrid, in_hook, usage, kind, seen, inner_t)
            }
            TypeInner::TypeAppT(box TypeAppTData { type_: inner_t, .. }) => {
                recur_id(cx, rrid, in_hook, usage, kind, seen, inner_t)
            }
            TypeInner::GenericT(box GenericTData { bound: inner_t, .. }) => {
                recur_id(cx, rrid, in_hook, usage, kind, seen, inner_t)
            }
            TypeInner::DefT(_, def_t)
                if let DefTInner::PolyT(box PolyTData { t_out: inner_t, .. }) = def_t.deref() =>
            {
                recur_id(cx, rrid, in_hook, usage, kind, seen, inner_t)
            }
            TypeInner::DefT(_, def_t) if let DefTInner::TypeT(_, inner_t) = def_t.deref() => {
                recur_id(cx, rrid, in_hook, usage, kind, seen, inner_t)
            }
            _ => vec![],
        }
    }

    let mut seen = TvarSeenSet::new();
    recur_id(cx, rrid, in_hook, &usage, &kind, &mut seen, &t)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HookCallKind {
    UseMemo,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Permissiveness {
    Permissive,
    Strict,
    Pattern,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HookResult {
    HookCallee(ALocFuzzySet),
    MaybeHookCallee {
        hooks: ALocFuzzySet,
        non_hooks: ALocFuzzySet,
    },
    NotHookCallee(ALocFuzzySet),
    AnyCallee,
}

fn hook_callee<'a>(cx: &mut Context<'a>, t: Type) -> HookResult {
    fn merge(l: HookResult, r: HookResult) -> HookResult {
        match (l, r) {
            (HookResult::AnyCallee, other) | (other, HookResult::AnyCallee) => other,
            (HookResult::NotHookCallee(l), HookResult::NotHookCallee(r)) => {
                HookResult::NotHookCallee(&l | &r)
            }
            (HookResult::HookCallee(l), HookResult::HookCallee(r)) => {
                HookResult::HookCallee(&l | &r)
            }
            (
                HookResult::MaybeHookCallee {
                    hooks: lh,
                    non_hooks: ln,
                },
                HookResult::MaybeHookCallee {
                    hooks: rh,
                    non_hooks: rn,
                },
            ) => HookResult::MaybeHookCallee {
                hooks: &lh | &rh,
                non_hooks: &ln | &rn,
            },
            (
                HookResult::MaybeHookCallee {
                    hooks: mh,
                    non_hooks: mn,
                },
                HookResult::HookCallee(h),
            )
            | (
                HookResult::HookCallee(h),
                HookResult::MaybeHookCallee {
                    hooks: mh,
                    non_hooks: mn,
                },
            ) => HookResult::MaybeHookCallee {
                hooks: &mh | &h,
                non_hooks: mn,
            },
            (
                HookResult::MaybeHookCallee {
                    hooks: mh,
                    non_hooks: mn,
                },
                HookResult::NotHookCallee(h),
            )
            | (
                HookResult::NotHookCallee(h),
                HookResult::MaybeHookCallee {
                    hooks: mh,
                    non_hooks: mn,
                },
            ) => HookResult::MaybeHookCallee {
                hooks: mh,
                non_hooks: &mn | &h,
            },
            (HookResult::HookCallee(h), HookResult::NotHookCallee(n))
            | (HookResult::NotHookCallee(n), HookResult::HookCallee(h)) => {
                HookResult::MaybeHookCallee {
                    non_hooks: n,
                    hooks: h,
                }
            }
        }
    }

    fn set_of_reason(r: &Reason) -> ALocFuzzySet {
        [ALocFuzzy::new(r.def_loc().dupe())].into_iter().collect()
    }

    fn recur_id<'a>(cx: &mut Context<'a>, seen: &mut TvarSeenSet<u32>, t: &Type) -> HookResult {
        match t.deref() {
            TypeInner::DefT(r, def_t)
                if matches!(
                    def_t.deref(),
                    DefTInner::FunT(_, fun_t)
                        if matches!(fun_t.effect_, ReactEffectType::HookDecl(_) | ReactEffectType::HookAnnot)
                ) =>
            {
                HookResult::HookCallee(set_of_reason(r))
            }
            TypeInner::DefT(_, def_t)
                if matches!(
                    def_t.deref(),
                    DefTInner::FunT(_, fun_t) if fun_t.effect_ == ReactEffectType::AnyEffect
                ) =>
            {
                HookResult::AnyCallee
            }
            TypeInner::DefT(r, def_t)
                if matches!(
                    def_t.deref(),
                    DefTInner::FunT(_, fun_t) if fun_t.effect_ == ReactEffectType::ArbitraryEffect
                ) =>
            {
                HookResult::NotHookCallee(set_of_reason(r))
            }
            TypeInner::NominalT { nominal_type, .. } => {
                match (&nominal_type.underlying_t, &nominal_type.upper_t) {
                    (nominal::UnderlyingT::OpaqueWithLocal { t }, _) => recur_id(cx, seen, t),
                    (
                        nominal::UnderlyingT::CustomError(box nominal::CustomErrorData {
                            t, ..
                        }),
                        _,
                    ) => recur_id(cx, seen, t),
                    (nominal::UnderlyingT::FullyOpaque, Some(t)) => recur_id(cx, seen, t),
                    _ => HookResult::AnyCallee,
                }
            }
            TypeInner::OpenT(tvar) if seen.contains(&tvar.id()) => HookResult::AnyCallee,
            TypeInner::OpenT(tvar) => seen.with_added(tvar.id(), |seen| {
                flow_js_utils::possible_types(cx, tvar.id() as i32)
                    .iter()
                    .map(|t| recur_id(cx, seen, t))
                    .fold(HookResult::AnyCallee, merge)
            }),
            TypeInner::UnionT(_, rep) => {
                rep.members_iter().fold(HookResult::AnyCallee, |acc, t| {
                    merge(recur_id(cx, seen, t), acc)
                })
            }
            // We can't easily handle intersections with the HooklikeT destructor, so if we're
            // in compatibility mode, let's just punt on enforcement
            TypeInner::IntersectionT(_, _) if cx.hook_compatibility() => HookResult::AnyCallee,
            TypeInner::IntersectionT(rs, rep) => {
                // Not tracking locations of hooks through unions
                let inv_merge = |l: HookResult, r: HookResult| -> HookResult {
                    match (&l, &r) {
                        (HookResult::AnyCallee, _) => r,
                        (_, HookResult::AnyCallee) => l,
                        _ if l == r => l,
                        (HookResult::HookCallee(_), _) | (_, HookResult::HookCallee(_)) => {
                            HookResult::HookCallee(set_of_reason(rs))
                        }
                        (HookResult::MaybeHookCallee { .. }, _)
                        | (_, HookResult::MaybeHookCallee { .. }) => HookResult::MaybeHookCallee {
                            hooks: set_of_reason(rs),
                            non_hooks: set_of_reason(rs),
                        },
                        (HookResult::NotHookCallee(_), HookResult::NotHookCallee(_)) => {
                            HookResult::NotHookCallee(set_of_reason(rs))
                        }
                    }
                };
                rep.members_iter().fold(HookResult::AnyCallee, |acc, t| {
                    inv_merge(recur_id(cx, seen, t), acc)
                })
            }
            TypeInner::MaybeT(_, inner_t) => recur_id(cx, seen, inner_t),
            TypeInner::OptionalT { type_: inner_t, .. } => recur_id(cx, seen, inner_t),
            TypeInner::AnnotT(_, inner_t, _) => recur_id(cx, seen, inner_t),
            TypeInner::TypeAppT(box TypeAppTData { type_: inner_t, .. }) => {
                recur_id(cx, seen, inner_t)
            }
            TypeInner::GenericT(box GenericTData { bound: inner_t, .. }) => {
                recur_id(cx, seen, inner_t)
            }
            TypeInner::DefT(_, def_t)
                if let DefTInner::PolyT(box PolyTData { t_out: inner_t, .. }) = def_t.deref() =>
            {
                recur_id(cx, seen, inner_t)
            }
            TypeInner::DefT(_, def_t) if let DefTInner::TypeT(_, inner_t) = def_t.deref() => {
                recur_id(cx, seen, inner_t)
            }
            TypeInner::DefT(_, def_t)
                if let DefTInner::ObjT(obj) = def_t.deref()
                    && let Some(call_id) = obj.call_t =>
            {
                let call_t = cx.find_call(call_id);
                recur_id(cx, seen, &call_t)
            }
            _ => HookResult::AnyCallee,
        }
    }

    let mut seen = TvarSeenSet::new();
    recur_id(cx, &mut seen, &t)
}

fn hook_error<'cx>(
    cx: &mut Context<'cx>,
    call_loc: ALoc,
    callee_loc: ALoc,
    kind: error_message::HookRule<ALoc>,
) {
    if cx.react_rule_enabled(flow_common::options::ReactRule::RulesOfHooks) {
        flow_js_utils::add_output_non_speculating(
            cx,
            ErrorMessage::EHookRuleViolation(Box::new(EHookRuleViolationData {
                hook_rule: kind,
                callee_loc,
                call_loc,
            })),
        );
    }
}

fn compatibility_call<M: dupe::Dupe, T: dupe::Dupe>(call: &ast::expression::Call<M, T>) -> bool {
    let name = match &*call.callee {
        ExpressionInner::Identifier { inner: ident, .. } => Some(&ident.name),
        ExpressionInner::Member { inner: member, .. } => {
            match (&*member.object, &member.property) {
                (
                    ExpressionInner::Identifier {
                        inner: obj_ident, ..
                    },
                    Property::PropertyIdentifier(prop_ident),
                ) if obj_ident.name.as_str() == "React" => Some(&prop_ident.name),
                _ => None,
            }
        }
        _ => None,
    };
    match name {
        Some(n) if n.as_str() == "forwardRef" || n.as_str() == "memo" => true,
        _ => match &*call.callee {
            ExpressionInner::Identifier { inner: ident, .. }
                if ident.name.as_str() == "renderHook"
                    || ident.name.as_str() == "renderHookAsync" =>
            {
                true
            }
            _ => false,
        },
    }
}

fn componentlike_name(name: &str) -> bool {
    let trimmed = name.trim_start_matches('_');
    trimmed
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase())
}

fn bare_use<M: dupe::Dupe, T: dupe::Dupe>(call: &ast::expression::Call<M, T>) -> bool {
    let callee = &call.callee;
    match callee.deref() {
        ExpressionInner::Identifier { inner: ident, .. } if ident.name.as_str() == "use" => true,
        ExpressionInner::Member { inner: member, .. } => match &member.property {
            Property::PropertyIdentifier(prop_ident) if prop_ident.name.as_str() == "use" => true,
            _ => false,
        },
        _ => false,
    }
}

mod conditional_state {
    use dupe::Dupe;
    use flow_data_structure_wrapper::ord_map::FlowOrdMap;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;

    #[derive(Debug, Clone, Copy, Dupe, PartialEq, Eq)]
    pub(super) enum TryState {
        NotInTry,
        InTry,
        InTryPostCall,
    }

    #[derive(Debug, Clone, Dupe)]
    pub(super) struct ConditionalState {
        pub(super) conditional_context: bool,
        pub(super) broken: bool,
        pub(super) return_seen: bool,
        pub(super) label_scopes: FlowOrdMap<FlowSmolStr, bool>,
        pub(super) switch_or_loop_scope: bool,
        pub(super) try_state: TryState,
    }

    pub(super) type SavedCond = bool;

    pub(super) type SavedSwitch = bool;

    pub(super) type SavedTry = TryState;

    fn max_try(t1: TryState, t2: TryState) -> TryState {
        match (t1, t2) {
            (TryState::InTryPostCall, _) | (_, TryState::InTryPostCall) => TryState::InTryPostCall,
            (TryState::InTry, _) | (_, TryState::InTry) => TryState::InTry,
            (TryState::NotInTry, TryState::NotInTry) => TryState::NotInTry,
        }
    }

    impl ConditionalState {
        pub fn init() -> Self {
            Self {
                conditional_context: false,
                broken: false,
                return_seen: false,
                label_scopes: FlowOrdMap::new(),
                switch_or_loop_scope: false,
                try_state: TryState::NotInTry,
            }
        }

        pub fn conditional(&self) -> bool {
            self.conditional_context
        }

        pub(super) fn enter_conditional(&mut self) -> SavedCond {
            let saved = self.conditional_context;
            self.conditional_context = true;
            saved
        }

        pub(super) fn reset_conditional(&mut self, conditional: SavedCond) {
            self.conditional_context = self.broken
                || self.return_seen
                || conditional
                || self.try_state == TryState::InTryPostCall;
        }

        pub(super) fn do_return(&mut self) {
            self.conditional_context = true;
            self.return_seen = true;
        }

        pub(super) fn enter_label(&mut self, s: FlowSmolStr) -> SavedCond {
            let saved = self.conditional_context;
            self.label_scopes.insert(s, false);
            saved
        }

        fn reset_broken(&mut self) {
            self.broken = self
                .label_scopes
                .values()
                .fold(self.switch_or_loop_scope, |acc, &v| acc || v);
        }

        pub(super) fn reset_label(&mut self, s: &FlowSmolStr, conditional: SavedCond) {
            let label_broken = self.label_scopes.get(s).copied().unwrap_or(false);
            self.label_scopes.remove(s);
            if label_broken {
                self.reset_broken();
                self.reset_conditional(conditional);
            }
        }

        pub(super) fn enter_switch_or_loop(&mut self) -> (SavedCond, SavedSwitch) {
            let saved = (self.conditional_context, self.switch_or_loop_scope);
            self.switch_or_loop_scope = false;
            saved
        }

        pub(super) fn reset_switch_or_loop(
            &mut self,
            conditional: SavedCond,
            cur_switch_or_loop: SavedSwitch,
        ) {
            let cur = self.switch_or_loop_scope;
            self.switch_or_loop_scope = cur_switch_or_loop;
            if cur {
                self.reset_broken();
                self.reset_conditional(conditional);
            }
        }

        pub(super) fn do_break(&mut self, s: Option<FlowSmolStr>) {
            self.broken = true;
            self.conditional_context = true;
            match s {
                None => {
                    self.switch_or_loop_scope = true;
                }
                Some(s) => {
                    self.label_scopes.insert(s, true);
                }
            }
        }

        pub(super) fn enter_try(&mut self) -> (SavedCond, SavedTry) {
            let saved = (self.conditional_context, self.try_state);
            self.try_state = max_try(self.try_state, TryState::InTry);
            saved
        }

        pub(super) fn reset_try(&mut self, conditional: SavedCond, cur_try_state: SavedTry) {
            let cur = self.try_state;
            self.try_state = cur_try_state;
            if cur == TryState::InTryPostCall {
                self.reset_conditional(conditional);
            }
        }

        pub(super) fn throwable(&mut self) {
            if self.try_state == TryState::InTry {
                self.conditional_context = true;
                self.try_state = TryState::InTryPostCall;
            }
        }

        pub(super) fn merge(a: &Self, b: &Self) -> Self {
            let mut label_scopes = a.label_scopes.dupe();
            for (k, &v) in &b.label_scopes {
                let existing = label_scopes.get(k).copied().unwrap_or(false);
                label_scopes.insert(k.dupe(), existing || v);
            }
            Self {
                conditional_context: a.conditional_context || b.conditional_context,
                broken: a.broken || b.broken,
                return_seen: a.return_seen || b.return_seen,
                label_scopes,
                switch_or_loop_scope: a.switch_or_loop_scope || b.switch_or_loop_scope,
                try_state: max_try(a.try_state, b.try_state),
            }
        }
    }
}

fn strip_use_callback<'a, M: dupe::Dupe, T: dupe::Dupe>(
    e: &'a ast::expression::Expression<M, T>,
) -> &'a ast::expression::Expression<M, T> {
    match e.deref() {
        ExpressionInner::Call { inner, .. } => {
            let name = match inner.callee.deref() {
                ExpressionInner::Identifier { inner: ident, .. } => Some(&ident.name),
                ExpressionInner::Member { inner: member, .. } => match &member.property {
                    Property::PropertyIdentifier(prop_ident) => Some(&prop_ident.name),
                    _ => None,
                },
                _ => None,
            };
            if let Some(name) = name {
                if name.as_str() == "useCallback" {
                    if let Some(ExpressionOrSpread::Expression(first_arg)) =
                        inner.arguments.arguments.first()
                    {
                        return first_arg;
                    }
                }
            }
            e
        }
        _ => e,
    }
}

fn downstream_effects<'cx>(
    ev_cx: &EffectVisitorContext,
    cx: &mut Context<'cx>,
    seen: &mut ALocSet,
    loc: ALoc,
) -> Result<Vec<ErrorMessage<ALoc>>, flow_utils_concurrency::job_error::JobError> {
    if seen.contains(&loc) {
        return Ok(vec![]);
    }

    let env_values = &ev_cx.var_info.env_values;
    let providers = &ev_cx.var_info.providers;
    let name_defs = &ev_cx.name_defs;

    fn visit_func<'cx>(
        ev_cx: &EffectVisitorContext,
        cx: &mut Context<'cx>,
        seen: &mut ALocSet,
        loc: &ALoc,
        func: &ast::function::Function<ALoc, ALoc>,
    ) -> Result<Vec<ErrorMessage<ALoc>>, flow_utils_concurrency::job_error::JobError> {
        if func.effect_ == ast::function::Effect::Hook {
            return Ok(vec![]);
        }
        let inserted = seen.insert(loc.dupe());
        let result = {
            let mut visitor = EffectVisitor::new(ev_cx, cx, false, seen);
            visitor.function_entry(&func.body)
        };
        if inserted {
            seen.remove(loc);
        }
        result
    }

    fn visit_class<'cx>(
        ev_cx: &EffectVisitorContext,
        cx: &mut Context<'cx>,
        seen: &mut ALocSet,
        loc: &ALoc,
        cls: &ast::class::Class<ALoc, ALoc>,
    ) -> Result<Vec<ErrorMessage<ALoc>>, flow_utils_concurrency::job_error::JobError> {
        let inserted = seen.insert(loc.dupe());
        let result = {
            let mut visitor = EffectVisitor::new(ev_cx, cx, false, seen);
            visitor.class_entry(&cls.body)
        };
        if inserted {
            seen.remove(loc);
        }
        result
    }

    match env_values.get_ordinary(&loc) {
        Some(read) => {
            let write_keys: Vec<env_api::EnvKey<ALoc>> = read
                .write_locs
                .iter()
                .flat_map(|wl| env_api::writes_of_write_loc(false, providers, wl))
                .collect();
            let defs: Vec<_> = write_keys
                .iter()
                .filter_map(|x| match name_defs.get(x) {
                    Some(d) => Some(d),
                    None => {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            ErrorMessage::EInternal(Box::new((
                                x.loc.dupe(),
                                error_message::InternalError::EnvInvariant(
                                    env_api::EnvInvariantFailure::NameDefGraphMismatch,
                                ),
                            ))),
                        );
                        None
                    }
                })
                .collect();
            let mut acc = Vec::new();
            for (def, _, _, _, _) in defs {
                match def {
                    Def::ExpressionDef(box name_def_types::ExpressionDef { expr, .. })
                    | Def::MemberAssign(box MemberAssignData { rhs: (_, expr), .. })
                    | Def::OpAssign(box OpAssignData { rhs: (_, expr), .. }) => {
                        let stripped = strip_use_callback(expr);
                        match stripped.deref() {
                            ExpressionInner::ArrowFunction { inner: func, .. }
                            | ExpressionInner::Function { inner: func, .. } => {
                                acc.extend(visit_func(ev_cx, cx, seen, &loc, func)?);
                            }
                            ExpressionInner::Class { inner: cls, .. } => {
                                acc.extend(visit_class(ev_cx, cx, seen, &loc, cls)?);
                            }
                            _ => {}
                        }
                    }
                    Def::Function(box FunctionDefData {
                        function_: func, ..
                    }) => {
                        acc.extend(visit_func(ev_cx, cx, seen, &loc, func)?);
                    }
                    Def::Component(_) => {}
                    Def::Class(box ClassDefData { class_: cls, .. }) => {
                        acc.extend(visit_class(ev_cx, cx, seen, &loc, cls)?);
                    }
                    // Records don't contain React hooks/rules violation
                    Def::Record(_) => {}
                    Def::Binding(bind) => {
                        fn handle_binding<'cx>(
                            bind: &Binding,
                            ev_cx: &EffectVisitorContext,
                            cx: &mut Context<'cx>,
                            seen: &mut ALocSet,
                            loc: &ALoc,
                        ) -> Result<
                            Vec<ErrorMessage<ALoc>>,
                            flow_utils_concurrency::job_error::JobError,
                        > {
                            match bind {
                                Binding::Select {
                                    parent: (_, bind), ..
                                }
                                | Binding::Hooklike(bind) => {
                                    handle_binding(bind, ev_cx, cx, seen, loc)
                                }
                                Binding::Root(Root::Annotation(box AnnotationData {
                                    concrete: Some(root),
                                    ..
                                })) => {
                                    let root_bind = Binding::Root(*root.clone());
                                    handle_binding(&root_bind, ev_cx, cx, seen, loc)
                                }
                                Binding::Root(Root::Value(box name_def_types::Value {
                                    expr,
                                    ..
                                })) => {
                                    let stripped = strip_use_callback(expr);
                                    match stripped.deref() {
                                        ExpressionInner::ArrowFunction { inner: func, .. }
                                        | ExpressionInner::Function { inner: func, .. } => {
                                            visit_func(ev_cx, cx, seen, loc, func)
                                        }
                                        ExpressionInner::Class { inner: cls, .. } => {
                                            visit_class(ev_cx, cx, seen, loc, cls)
                                        }
                                        _ => Ok(vec![]),
                                    }
                                }
                                Binding::Root(Root::Contextual(box ContextualData {
                                    default_expression: Some((_, expr)),
                                    ..
                                })) => {
                                    let stripped = strip_use_callback(expr);
                                    match stripped.deref() {
                                        ExpressionInner::ArrowFunction { inner: func, .. }
                                        | ExpressionInner::Function { inner: func, .. } => {
                                            visit_func(ev_cx, cx, seen, loc, func)
                                        }
                                        ExpressionInner::Class { inner: cls, .. } => {
                                            visit_class(ev_cx, cx, seen, loc, cls)
                                        }
                                        _ => Ok(vec![]),
                                    }
                                }
                                Binding::Root(Root::FunctionValue(box FunctionValueData {
                                    function_: func,
                                    ..
                                })) => visit_func(ev_cx, cx, seen, loc, func),
                                _ => Ok(vec![]),
                            }
                        }
                        acc.extend(handle_binding(bind, ev_cx, cx, seen, &loc)?);
                    }
                    _ => {}
                }
            }
            Ok(acc)
        }
        None => Ok(vec![]),
    }
}

fn is_initializing(ev_cx: &EffectVisitorContext, cx: &mut Context<'_>, loc: &ALoc) -> bool {
    let env_values = &ev_cx.var_info.env_values;
    let refinement_of_id = &ev_cx.var_info.refinement_of_id;

    let read = match env_values.get_ordinary(loc) {
        Some(read) => read,
        None => return false,
    };
    let write_locs = &read.write_locs;
    if write_locs.is_empty() {
        return false;
    }

    fn is_nullish<'a>(cx: &mut Context<'a>, seen: &mut TvarSeenSet<u32>, t: &Type) -> bool {
        match t.deref() {
            TypeInner::OpenT(tvar) if seen.contains(&tvar.id()) => false,
            TypeInner::OpenT(tvar) => seen.with_added(tvar.id(), |seen| {
                let possible = flow_js_utils::possible_types(cx, tvar.id() as i32);
                !possible.is_empty() && possible.iter().all(|t| is_nullish(cx, seen, t))
            }),
            TypeInner::UnionT(_, rep) => rep.members_iter().all(|t| is_nullish(cx, seen, t)),
            TypeInner::DefT(_, def_t)
                if matches!(def_t.deref(), DefTInner::NullT | DefTInner::VoidT) =>
            {
                true
            }
            _ => false,
        }
    }

    fn refines_safe<'a>(cx: &mut Context<'a>, refi: &RefinementKind<ALoc>) -> bool {
        match refi {
            RefinementKind::AndR(l, r) => refines_safe(cx, l) || refines_safe(cx, r),
            RefinementKind::OrR(l, r) => refines_safe(cx, l) && refines_safe(cx, r),
            RefinementKind::NotR(r) => !not_refines_safe(cx, r),
            RefinementKind::SentinelR { prop, other_loc } if prop.as_str() == "current" => {
                let reason = flow_common::reason::mk_reason(
                    VirtualReasonDesc::RMember {
                        object_: "ref".into(),
                        property: "current".into(),
                    },
                    other_loc.dupe(),
                );
                let t = type_env::find_write(cx, env_api::DefLocType::ExpressionLoc, reason);
                let mut seen = TvarSeenSet::new();
                is_nullish(cx, &mut seen, &t)
            }
            RefinementKind::PropNullishR { propname, .. } if propname.as_str() == "current" => true,
            _ => false,
        }
    }

    fn not_refines_safe<'a>(cx: &mut Context<'a>, refi: &RefinementKind<ALoc>) -> bool {
        match refi {
            RefinementKind::AndR(l, r) => not_refines_safe(cx, l) && not_refines_safe(cx, r),
            RefinementKind::OrR(l, r) => not_refines_safe(cx, l) || not_refines_safe(cx, r),
            RefinementKind::NotR(r) => !refines_safe(cx, r),
            RefinementKind::PropTruthyR { propname, .. } if propname.as_str() == "current" => false,
            _ => true,
        }
    }

    write_locs.iter().all(|write| {
        let refis = env_api::refinements_of_write_loc(refinement_of_id, write);
        refis.iter().any(|refi| refines_safe(cx, refi))
    })
}

struct EffectVisitorContext {
    is_hook: bool,
    rrid: Option<type_::nominal::Id>,
    type_map: ALocMap<Type>,
    var_info: std::rc::Rc<flow_env_builder::env_api::EnvInfo<ALoc>>,
    name_defs: flow_env_builder::name_def_types::EnvEntriesMap,
}

struct EffectVisitor<'ev, 'b, 'cx, 'seen> {
    ev_cx: &'ev EffectVisitorContext,
    cx: &'b mut Context<'cx>,
    effects: Vec<ErrorMessage<ALoc>>,
    in_target: bool,
    seen: &'seen mut ALocSet,
    toplevel: bool,
}

impl<'ev, 'b, 'cx, 'seen> EffectVisitor<'ev, 'b, 'cx, 'seen> {
    fn new(
        ev_cx: &'ev EffectVisitorContext,
        cx: &'b mut Context<'cx>,
        toplevel: bool,
        seen: &'seen mut ALocSet,
    ) -> Self {
        Self {
            ev_cx,
            cx,
            effects: vec![],
            in_target: false,
            seen,
            toplevel,
        }
    }

    fn function_entry(
        &mut self,
        body: &ast::function::Body<ALoc, ALoc>,
    ) -> Result<Vec<ErrorMessage<ALoc>>, flow_utils_concurrency::job_error::JobError> {
        ast_visitor::function_body_any_default(self, body)?;
        Ok(std::mem::take(&mut self.effects))
    }

    fn component_entry(
        &mut self,
        body: &(ALoc, ast::statement::Block<ALoc, ALoc>),
    ) -> Result<Vec<ErrorMessage<ALoc>>, flow_utils_concurrency::job_error::JobError> {
        ast_visitor::component_body_default(self, body)?;
        Ok(std::mem::take(&mut self.effects))
    }

    fn class_entry(
        &mut self,
        body: &ast::class::Body<ALoc, ALoc>,
    ) -> Result<Vec<ErrorMessage<ALoc>>, flow_utils_concurrency::job_error::JobError> {
        ast_visitor::class_body_default(self, body)?;
        Ok(std::mem::take(&mut self.effects))
    }

    fn in_target_scope<R>(&mut self, t: bool, f: impl FnOnce(&mut Self) -> R) -> R {
        let cur_in_target = self.in_target;
        self.in_target = t;
        let res = f(self);
        self.in_target = cur_in_target;
        res
    }

    fn base_expression(
        &mut self,
        e: &ast::expression::Expression<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        if self.in_target {
            match e.deref() {
                ExpressionInner::Identifier { loc, .. } => {
                    let new_effects =
                        downstream_effects(self.ev_cx, self.cx, &mut *self.seen, loc.dupe())?;
                    self.effects.extend(new_effects);
                }
                ExpressionInner::ArrowFunction { loc, inner: func }
                | ExpressionInner::Function { loc, inner: func }
                    if !self.seen.contains(loc) =>
                {
                    self.seen.insert(loc.dupe());
                    let new_effects = {
                        let mut visitor =
                            EffectVisitor::new(self.ev_cx, self.cx, false, &mut *self.seen);
                        visitor.function_entry(&func.body)?
                    };
                    self.effects.extend(new_effects);
                    self.seen.remove(loc);
                }
                _ => {}
            }
        }
        ast_visitor::expression_default(self, e)
    }

    fn permissive_expression(
        &mut self,
        expr: &ast::expression::Expression<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        match expr.deref() {
            ExpressionInner::Member { inner: mem, .. } => {
                self.visit_member(Permissiveness::Permissive, mem)?;
            }
            ExpressionInner::OptionalMember { inner: mem, .. } => {
                self.visit_optional_member(Permissiveness::Permissive, mem)?;
            }
            ExpressionInner::Unary { inner, .. }
                if inner.operator == ast::expression::UnaryOperator::Not =>
            {
                self.permissive_expression(&inner.argument)?;
            }
            ExpressionInner::Binary { inner, .. }
                if matches!(
                    inner.operator,
                    ast::expression::BinaryOperator::Equal
                        | ast::expression::BinaryOperator::StrictEqual
                        | ast::expression::BinaryOperator::NotEqual
                        | ast::expression::BinaryOperator::StrictNotEqual
                ) =>
            {
                self.permissive_expression(&inner.left)?;
                self.permissive_expression(&inner.right)?;
            }
            ExpressionInner::Logical { inner, .. } => {
                self.permissive_expression(&inner.left)?;
                self.permissive_expression(&inner.right)?;
            }
            _ => {
                self.expression(expr)?;
            }
        }
        Ok(())
    }

    fn target_expression(
        &mut self,
        expr: &ast::expression::Expression<ALoc, ALoc>,
        err_kind: RefInRenderKind,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let loc = expr.loc().dupe();
        let usage = ExpressionReferenceData {
            loc: loc.dupe(),
            kind: flow_js_utils::expression_reference_kind_for_error(expr),
        };
        if self
            .cx
            .react_rule_enabled(flow_common::options::ReactRule::ValidateRefAccessDuringRender)
        {
            if let Some(ty) = self.ev_cx.type_map.get(&loc) {
                let new_effects = check_ref_use(
                    self.cx,
                    self.ev_cx.rrid.as_ref(),
                    self.ev_cx.is_hook,
                    usage,
                    err_kind,
                    ty.dupe(),
                );
                self.effects.extend(new_effects);
            }
        }
        self.expression(expr)
    }

    fn visit_arg_list(
        &mut self,
        hook_call: Option<HookCallKind>,
        arg_list: &ast::expression::ArgList<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        for (i, arg) in arg_list.arguments.iter().enumerate() {
            match arg {
                ExpressionOrSpread::Expression(exp) => match hook_call {
                    Some(HookCallKind::UseMemo) if i == 0 => {
                        self.in_target_scope(true, |this| this.expression(exp))?;
                    }
                    Some(_) => {
                        self.in_target_scope(false, |this| this.expression(exp))?;
                    }
                    None => {
                        self.in_target_scope(false, |this| {
                            this.target_expression(exp, RefInRenderKind::Argument)
                        })?;
                    }
                },
                ExpressionOrSpread::Spread(spread) if hook_call.is_some() => {
                    self.in_target_scope(false, |this| {
                        this.target_expression(&spread.argument, RefInRenderKind::Argument)
                    })?;
                }
                ExpressionOrSpread::Spread(spread) => {
                    self.in_target_scope(false, |this| this.spread_element(spread))?;
                }
            }
        }
        Ok(())
    }

    fn visit_optional_member(
        &mut self,
        permissive: Permissiveness,
        expr: &ast::expression::OptionalMember<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        self.visit_member(permissive, &expr.member)
    }

    fn visit_member(
        &mut self,
        permissive: Permissiveness,
        expr: &ast::expression::Member<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let _object = &expr.object;
        let loc = _object.loc().dupe();
        let property = &expr.property;
        self.member_property(property)?;
        match property {
            Property::PropertyIdentifier(prop_ident)
                if prop_ident.name.as_str() == "current"
                    && (permissive == Permissiveness::Strict
                        || (permissive == Permissiveness::Pattern
                            && !is_initializing(self.ev_cx, self.cx, &loc))) =>
            {
                self.target_expression(_object, RefInRenderKind::Access)?;
            }
            _ => {
                self.expression(_object)?;
            }
        }
        if self.in_target {
            let new_effects = downstream_effects(self.ev_cx, self.cx, &mut *self.seen, loc)?;
            self.effects.extend(new_effects);
        }
        Ok(())
    }
}

impl<'ast>
    ast_visitor::AstVisitor<
        'ast,
        ALoc,
        ALoc,
        &'ast ALoc,
        flow_utils_concurrency::job_error::JobError,
    > for EffectVisitor<'_, '_, '_, '_>
{
    fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
        loc
    }

    fn normalize_type(type_: &'ast ALoc) -> &'ast ALoc {
        type_
    }

    fn member(
        &mut self,
        _loc: &'ast ALoc,
        _expr: &'ast ast::expression::Member<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        panic!("Call visit_member")
    }

    fn optional_member(
        &mut self,
        _loc: &'ast ALoc,
        _expr: &'ast ast::expression::OptionalMember<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        panic!("Call visit_optional_member")
    }

    fn expression(
        &mut self,
        expr: &'ast ast::expression::Expression<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        match expr.deref() {
            ExpressionInner::Member { inner: mem, .. } => {
                self.visit_member(Permissiveness::Strict, mem)?;
            }
            ExpressionInner::OptionalMember { inner: mem, .. } => {
                self.visit_optional_member(Permissiveness::Strict, mem)?;
            }
            _ => {
                self.base_expression(expr)?;
            }
        }
        Ok(())
    }

    fn pattern_expression(
        &mut self,
        expr: &'ast ast::expression::Expression<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        match expr.deref() {
            ExpressionInner::Member { inner: mem, .. } => {
                self.visit_member(Permissiveness::Pattern, mem)?;
            }
            ExpressionInner::OptionalMember { inner: mem, .. } => {
                self.visit_optional_member(Permissiveness::Pattern, mem)?;
            }
            _ => {
                self.base_expression(expr)?;
            }
        }
        Ok(())
    }

    fn predicate_expression(
        &mut self,
        expr: &'ast ast::expression::Expression<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        self.permissive_expression(expr)
    }

    fn arg_list(
        &mut self,
        arg_list: &'ast ast::expression::ArgList<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        self.visit_arg_list(None, arg_list)?;
        Ok(())
    }

    fn call(
        &mut self,
        _loc: &'ast ALoc,
        expr: &'ast ast::expression::Call<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let callee = &expr.callee;
        let callee_loc = callee.loc().dupe();
        let targs = &expr.targs;
        let arguments = &expr.arguments;

        let ev_cx = self.ev_cx;
        let callee_ty =
            |cx: &mut Context<'_>| -> Result<Type, flow_utils_concurrency::job_error::JobError> {
                match callee.deref() {
                    ExpressionInner::Identifier { inner: ident, .. }
                        if cx.hook_compatibility() && ident.name.as_str() != "require" =>
                    {
                        //  If we're in compatibility mode, we want to bail on intersections. But
                        // the typed AST records the type of the overload we've selected, so we
                        // never see the intersection to realize we need to bail! Instead in this
                        // case we read from the environment.
                        type_env::var_ref(None, cx, None, &ident.name, callee_loc.dupe())
                    }
                    _ => Ok(ev_cx
                        .type_map
                        .get(&callee_loc)
                        .cloned()
                        .expect("callee_loc not found in type_map")),
                }
            };
        let callee_is_nonhook = {
            !self.toplevel || {
                let callee_t = callee_ty(self.cx)?;
                match hook_callee(self.cx, callee_t) {
                    HookResult::HookCallee(_) => false,
                    HookResult::MaybeHookCallee { .. } => true,
                    HookResult::NotHookCallee(_) => true,
                    HookResult::AnyCallee => false,
                }
            }
        };
        let hook_call = {
            let name = match callee.deref() {
                ExpressionInner::Identifier { inner: ident, .. } => Some(&ident.name),
                ExpressionInner::Member { inner: member, .. } => match &member.property {
                    Property::PropertyIdentifier(prop_ident) => Some(&prop_ident.name),
                    _ => None,
                },
                _ => None,
            };
            match name {
                Some(name) => {
                    if name.as_str() == "useMemo" {
                        Some(HookCallKind::UseMemo)
                    } else if callee_is_nonhook {
                        None
                    } else {
                        Some(HookCallKind::Other)
                    }
                }
                None if callee_is_nonhook => None,
                None => Some(HookCallKind::Other),
            }
        };
        self.in_target_scope(true, |this| this.expression(callee))?;
        if let Some(targs) = targs {
            self.call_type_args(targs)?;
        }
        self.visit_arg_list(hook_call, arguments)?;
        Ok(())
    }

    fn function_body_any(
        &mut self,
        _body: &'ast ast::function::Body<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        Ok(())
    }

    fn class_body(
        &mut self,
        _cls_body: &'ast ast::class::Body<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        Ok(())
    }

    fn component_body(
        &mut self,
        _body: &'ast (ALoc, ast::statement::Block<ALoc, ALoc>),
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        Ok(())
    }
}

struct EffectVisitorFactory<'b, 'cx> {
    cx: &'b mut Context<'cx>,
    ev_cx: EffectVisitorContext,
}

impl<'b, 'cx> EffectVisitorFactory<'b, 'cx> {
    fn new(
        cx: &'b mut Context<'cx>,
        is_hook: bool,
        rrid: Option<type_::nominal::Id>,
        tast: &ast::Program<ALoc, (ALoc, Type)>,
    ) -> Self {
        let env = cx.environment();
        let var_info = env.var_info.dupe();
        let name_defs = env.name_defs.dupe();
        let type_map = typed_ast_utils::typed_ast_to_map(tast);
        Self {
            cx,
            ev_cx: EffectVisitorContext {
                is_hook,
                rrid,
                type_map,
                var_info,
                name_defs,
            },
        }
    }

    fn run_function_entry(
        &mut self,
        body: &ast::function::Body<ALoc, ALoc>,
    ) -> Result<Vec<ErrorMessage<ALoc>>, flow_utils_concurrency::job_error::JobError> {
        let mut seen = ALocSet::new();
        let mut visitor = EffectVisitor::new(&self.ev_cx, self.cx, true, &mut seen);
        visitor.function_entry(body)
    }

    fn run_component_entry(
        &mut self,
        body: &(ALoc, ast::statement::Block<ALoc, ALoc>),
    ) -> Result<Vec<ErrorMessage<ALoc>>, flow_utils_concurrency::job_error::JobError> {
        let mut seen = ALocSet::new();
        let mut visitor = EffectVisitor::new(&self.ev_cx, self.cx, true, &mut seen);
        visitor.component_entry(body)
    }
}

fn emit_effect_errors<'a>(cx: &mut Context<'a>, errors: Vec<ErrorMessage<ALoc>>) {
    for error in errors {
        flow_js_utils::add_output_non_speculating(cx, error);
    }
}

#[derive(Debug, Clone, Copy, Dupe, PartialEq, Eq)]
enum HookCallContext {
    /// e.g. Calling hooks in things like `function fetchData(...) {...}`
    HookCallDefinitelyNotAllowed,
    /// e.g. Calling hooks in things like `() => {...}` where we are not sure whether it can be a
    /// component or hook.  
    HookCallNotAllowedUnderUnknownContext,
    /// Same as HookCallNotAllowedUnderUnknownContext, but the function is inside component or hooks.
    /// In this case, we should treat the hook call as conditional.  
    HookCallNotAllowedUnderNormalFunctionInComponentOrHooks,
    /// e.g. Calling hooks in things like `function Foo(...) {...}` or `function useFoo(...) {...}`,
    /// permissively allowed because hook compatibility mode is on  
    HookCallPermissivelyAllowedUnderCompatibilityMode,
    /// e.g. Calling hooks in things like `function Foo(...) {...}` or `function useFoo(...) {...}`,
    /// strictly disallowed because hook compatibility mode is off
    HookCallStrictlyDisallowedWithoutCompatibilityMode,
    /// e.g. Calling hooks in a class constructor, method or property initializer, or in an
    /// anonymous function nested in one.
    HookCallNotAllowedInClass,
}

struct WholeAstVisitor<'b, 'cx, 't> {
    tast: &'t ast::Program<ALoc, (ALoc, Type)>,
    under_function_or_class_body: bool,
    cx: &'b mut Context<'cx>,
    rrid: Option<type_::nominal::Id>,
    hook_call_context: Rc<
        flow_lazy::Lazy<Cx, Result<HookCallContext, flow_utils_concurrency::job_error::JobError>>,
    >,
    in_context_possibly_expecting_fn_component_or_hook: bool,
}

impl<'b, 'cx, 't> WholeAstVisitor<'b, 'cx, 't> {
    fn new(
        tast: &'t ast::Program<ALoc, (ALoc, Type)>,
        under_function_or_class_body: bool,
        initial_hook_call_context: HookCallContext,
        cx: &'b mut Context<'cx>,
        rrid: Option<type_::nominal::Id>,
    ) -> Self {
        Self {
            tast,
            under_function_or_class_body,
            cx,
            rrid,
            hook_call_context: Rc::new(flow_lazy::Lazy::new_forced(Ok(initial_hook_call_context))),
            in_context_possibly_expecting_fn_component_or_hook: false,
        }
    }

    fn visit_function(
        &mut self,
        loc_for_hint: Option<ALoc>,
        fn_: &'t ast::function::Function<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        self.visit_function_with_id(loc_for_hint, fn_, None)
    }

    /// Visit a function with an optional effective id override.
    /// When `effective_id` is Some, it is used for hook detection instead of `fn_.id`.
    /// The original `fn_` AST node (with 't lifetime) is still used for traversal.
    fn visit_function_with_id(
        &mut self,
        loc_for_hint: Option<ALoc>,
        fn_: &'t ast::function::Function<ALoc, (ALoc, Type)>,
        effective_id: Option<&ast::Identifier<ALoc, (ALoc, Type)>>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let id = effective_id.or(fn_.id.as_ref());
        let params = &fn_.params;
        let params_list = &params.params;
        let rest = &params.rest;
        let body = &fn_.body;
        let effect_ = &fn_.effect_;
        let return_ = &fn_.return_;

        if *effect_ == ast::function::Effect::Hook {
            let effects = {
                let Ok(untyped_body) = flow_parser::polymorphic_ast_mapper::function_body_any(
                    &mut typed_ast_utils::UntypedAstMapper,
                    body,
                );
                let mut factory =
                    EffectVisitorFactory::new(self.cx, true, self.rrid.clone(), self.tast);
                factory.run_function_entry(&untyped_body)?
            };
            emit_effect_errors(self.cx, effects);
            if let Some(ident) = &fn_.id {
                self.function_identifier(ident)?;
            }
            if let Some(tparams) = &fn_.tparams {
                self.type_params(&ast_visitor::TypeParamsContext::Function, tparams)?;
            }
            {
                let mut cav = ComponentAstVisitor::new(self.tast, self.cx, self.rrid.clone());
                cav.function_params(params)?;
            }
            self.function_return_annotation(return_)?;
            {
                let mut cav = ComponentAstVisitor::new(self.tast, self.cx, self.rrid.clone());
                cav.function_component_body(body)?;
            }
            if let Some(predicate) = &fn_.predicate {
                let mut cav = ComponentAstVisitor::new(self.tast, self.cx, self.rrid.clone());
                cav.predicate(predicate)?;
            }
        } else {
            let cur_hook_call_context = self.hook_call_context.dupe();
            let is_probably_function_component = id
                .map(|ident| componentlike_name(ident.name.as_str()))
                .unwrap_or(false)
                && params_list.len() <= 2
                && rest.is_none();

            let is_hook_function = id.is_some_and(|ident| ast_utils::hook_name(&ident.name));
            let possibly_in_context_allow_hook_call = self
                .in_context_possibly_expecting_fn_component_or_hook
                || is_probably_function_component
                || is_hook_function;
            let saved_in_context = self.in_context_possibly_expecting_fn_component_or_hook;
            // Above, we pull out some reads of mutable class fields out of lazy block.

            // Pre-extract owned data for lazy closure
            let id_name = id.map(|ident| ident.name.dupe());
            // Return type data: None means TypeGuard (ret_check = true)
            let return_t = match return_ {
                ast::function::ReturnAnnot::Missing((_, t)) => Some(t.dupe()),
                ast::function::ReturnAnnot::Available(annot) => {
                    Some(annot.annotation.loc().1.dupe())
                }
                ast::function::ReturnAnnot::TypeGuard(_) => None,
            };
            // Params data for is_definitely_non_component_due_to_typing
            let params_definitely_not_component = match params_list.deref() {
                [] => false,
                [p] | [p, _] => matches!(p, ast::function::Param::ParamProperty { .. }),
                _ => true,
            };
            let props_data = match params_list.deref() {
                [p] | [p, _] => match p {
                    ast::function::Param::RegularParam { argument, .. } => {
                        let (ref props_loc, ref props_t) = *argument.loc();
                        Some((props_loc.dupe(), props_t.dupe()))
                    }
                    _ => None,
                },
                _ => None,
            };

            let enclosing_hook_call_context = cur_hook_call_context.dupe();

            self.hook_call_context = Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                let is_definitely_non_component_due_to_typing = |cx: &mut Context<'_>| -> Result<
                    bool,
                    flow_utils_concurrency::job_error::JobError,
                > {
                    // Not returning `React.Node`
                    let ret_check = match &return_t {
                        Some(t) => {
                            let reason = flow_typing_type::type_util::reason_of_t(t);
                            let react_node_t = flow_js::get_builtin_react_type_non_speculating(
                                    cx,
                                    reason,
                                    None,
                                    intermediate_error_types::ExpectedModulePurpose::ReactModuleForReactNodeType,
                                )?;
                            !flow_js::FlowJs::speculative_subtyping_succeeds(cx, t, &react_node_t)?
                        }
                        None => true, // TypeGuard
                    };
                    Ok(ret_check
                        || params_definitely_not_component
                        || match &props_data {
                            // function Component(props: {...})
                            // forwardRef(props: {...}, ref: ...)
                            Some((props_loc, props_t)) => {
                                let empty_iface =
                                    type_annotation::mk_empty_interface_type(cx, props_loc.dupe())?;
                                !flow_js::FlowJs::speculative_subtyping_succeeds(
                                    cx,
                                    props_t,
                                    &empty_iface,
                                )?
                            }
                            None => false,
                        })
                };

                let is_definitely_component_due_to_hint = |cx: &mut Context<'_>| -> Result<
                    bool,
                    flow_utils_concurrency::job_error::JobError,
                > {
                    Ok(match &loc_for_hint {
                        None => false,
                        Some(hint_loc) => {
                            let lazy_hint = type_env::get_hint(cx, hint_loc.dupe());
                            let reason = flow_common::reason::mk_reason(
                                VirtualReasonDesc::RFunctionType,
                                hint_loc.dupe(),
                            );
                            match (lazy_hint.1)(cx, true, Some(true), reason.dupe())? {
                                    type_::HintEvalResult::NoHint
                                    | type_::HintEvalResult::EncounteredPlaceholder(_)
                                    | type_::HintEvalResult::DecompositionError => false,
                                    type_::HintEvalResult::HintAvailable(hint_t, _) => {
                                        match flow_js::FlowJs::singleton_concrete_type_for_inspection(
                                            cx, &reason, &hint_t,
                                        ) {
                                            Ok(t) => matches!(
                                                t.deref(),
                                                TypeInner::DefT(_, def_t) if matches!(
                                                    def_t.deref(),
                                                    flow_typing_type::type_::DefTInner::ReactAbstractComponentT(_)
                                                )
                                            ),
                                            Err(flow_typing_flow_common::flow_js_utils::FlowJsException::WorkerCanceled(c)) => return Err(flow_utils_concurrency::job_error::JobError::Canceled(c)),
                                            Err(flow_typing_flow_common::flow_js_utils::FlowJsException::TimedOut(t)) => return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t)),
                                            Err(_) => false,
                                        }
                                    }
                                }
                        }
                    })
                };

                Ok(if possibly_in_context_allow_hook_call {
                    if cx.hook_compatibility() || is_definitely_component_due_to_hint(cx)? {
                        HookCallContext::HookCallPermissivelyAllowedUnderCompatibilityMode
                    } else if match id_name.as_ref() {
                        Some(name) => {
                            !is_hook_function
                                && componentlike_name(name.as_str())
                                && is_definitely_non_component_due_to_typing(cx)?
                        }
                        None => false,
                    } {
                        HookCallContext::HookCallDefinitelyNotAllowed
                    } else {
                        HookCallContext::HookCallStrictlyDisallowedWithoutCompatibilityMode
                    }
                } else if match id_name.as_ref() {
                    Some(name) => {
                        !saved_in_context
                            && !is_hook_function
                            && (!componentlike_name(name.as_str())
                                || is_definitely_non_component_due_to_typing(cx)?)
                    }
                    None => false,
                } {
                    HookCallContext::HookCallDefinitelyNotAllowed
                } else {
                    // An anonymous function inside a component, hook or class keeps that context.
                    match enclosing_hook_call_context.get_forced(cx) {
                        Ok(
                            enclosing @ (HookCallContext::HookCallNotAllowedUnderNormalFunctionInComponentOrHooks
                            | HookCallContext::HookCallNotAllowedInClass),
                        ) => *enclosing,
                        Ok(_) => HookCallContext::HookCallNotAllowedUnderUnknownContext,
                        Err(e) => return Err(e.dupe()),
                    }
                })
            }));
            ast_visitor::function_default(self, &fn_.sig_loc, fn_)?;
            self.hook_call_context = cur_hook_call_context;
        }
        Ok(())
    }
}

impl<'b, 'cx, 't>
    ast_visitor::AstVisitor<
        't,
        ALoc,
        (ALoc, Type),
        &'t ALoc,
        flow_utils_concurrency::job_error::JobError,
    > for WholeAstVisitor<'b, 'cx, 't>
{
    fn normalize_loc(loc: &'t ALoc) -> &'t ALoc {
        loc
    }

    fn normalize_type(type_: &'t (ALoc, Type)) -> &'t ALoc {
        &type_.0
    }

    fn component_declaration(
        &mut self,
        _loc: &'t ALoc,
        cmp: &'t ast::statement::ComponentDeclaration<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let effects = match &cmp.body {
            None => vec![],
            Some(body) => {
                let Ok(untyped_body) = flow_parser::polymorphic_ast_mapper::component_body(
                    &mut typed_ast_utils::UntypedAstMapper,
                    body,
                );
                let mut factory =
                    EffectVisitorFactory::new(self.cx, false, self.rrid.clone(), self.tast);
                factory.run_component_entry(&untyped_body)?
            }
        };
        emit_effect_errors(self.cx, effects);
        let mut cav = ComponentAstVisitor::new(self.tast, self.cx, self.rrid.clone());
        cav.visit_toplevel_component(cmp)?;
        Ok(())
    }

    fn expression(
        &mut self,
        expr: &'t ast::expression::Expression<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        match expr.deref() {
            ExpressionInner::ArrowFunction { loc, inner: fn_ } => {
                let hint_loc = loc.0.dupe();
                self.visit_function(Some(hint_loc), fn_)?;
            }
            ExpressionInner::Function { loc, inner: fn_ } => {
                let hint_loc = loc.0.dupe();
                self.visit_function(Some(hint_loc), fn_)?;
            }
            _ => {
                ast_visitor::expression_default(self, expr)?;
            }
        }
        Ok(())
    }

    fn function_(
        &mut self,
        _loc: &'t ALoc,
        fn_: &'t ast::function::Function<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        self.visit_function(None, fn_)?;
        Ok(())
    }

    fn call(
        &mut self,
        annot: &'t (ALoc, Type),
        expr: &'t ast::expression::Call<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let call_loc = &annot.0;
        let callee = &expr.callee;
        let callee_loc = &callee.loc().0;
        let callee_ty = &callee.loc().1;

        let callee_ty = match callee.deref() {
            ExpressionInner::Identifier { inner: ident, .. }
                if self.cx.hook_compatibility() && ident.name.as_str() != "require" =>
            {
                // If we're in compatibility mode, we want to bail on intersections. But
                // the typed AST records the type of the overload we've selected, so we
                // never see the intersection to realize we need to bail! Instead in this
                // case we read from the environment.
                type_env::var_ref(None, self.cx, None, &ident.name, callee_loc.dupe())?
            }
            _ => callee_ty.dupe(),
        };

        let is_bare_use = ast_utils::hook_call(expr) && bare_use(expr);
        match hook_callee(self.cx, callee_ty) {
            HookResult::HookCallee(_) | HookResult::MaybeHookCallee { .. }
                if !(is_bare_use && self.under_function_or_class_body) =>
            {
                match self.hook_call_context.get_forced(self.cx) {
                    Ok(HookCallContext::HookCallDefinitelyNotAllowed) => {
                        hook_error(
                            self.cx,
                            call_loc.dupe(),
                            callee_loc.dupe(),
                            error_message::HookRule::HookDefinitelyNotInComponentOrHook,
                        );
                    }
                    Ok(HookCallContext::HookCallNotAllowedUnderUnknownContext) => {
                        hook_error(
                            self.cx,
                            call_loc.dupe(),
                            callee_loc.dupe(),
                            if is_bare_use {
                                error_message::HookRule::BareUseInUnknownContext
                            } else {
                                error_message::HookRule::HookInUnknownContext
                            },
                        );
                    }
                    Ok(
                        HookCallContext::HookCallNotAllowedUnderNormalFunctionInComponentOrHooks,
                    ) => {
                        hook_error(
                            self.cx,
                            call_loc.dupe(),
                            callee_loc.dupe(),
                            error_message::HookRule::ConditionalHook,
                        );
                    }
                    Ok(HookCallContext::HookCallStrictlyDisallowedWithoutCompatibilityMode) => {
                        hook_error(
                            self.cx,
                            call_loc.dupe(),
                            callee_loc.dupe(),
                            error_message::HookRule::HookNotInComponentSyntaxComponentOrHookSyntaxHook,
                        );
                    }
                    Ok(HookCallContext::HookCallNotAllowedInClass) => {
                        hook_error(
                            self.cx,
                            call_loc.dupe(),
                            callee_loc.dupe(),
                            error_message::HookRule::HookInClass,
                        );
                    }
                    Ok(HookCallContext::HookCallPermissivelyAllowedUnderCompatibilityMode) => {}
                    Err(c) => return Err(c.dupe()),
                }
            }
            _ => {}
        }

        let cur_in_context = self.in_context_possibly_expecting_fn_component_or_hook;
        // Within call like `React.memo`, we can pass fn components.
        self.in_context_possibly_expecting_fn_component_or_hook =
            compatibility_call(expr) || cur_in_context;
        ast_visitor::call_default(self, annot, expr)?;
        self.in_context_possibly_expecting_fn_component_or_hook = cur_in_context;
        Ok(())
    }

    fn export_default_declaration_decl(
        &mut self,
        decl: &'t ast::statement::export_default_declaration::Declaration<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let cur_in_context = self.in_context_possibly_expecting_fn_component_or_hook;
        let file_key = self.cx.file();
        let filename_str = file_key.as_str();
        let filename = std::path::Path::new(filename_str)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(filename_str);
        let next_in_context = if componentlike_name(filename) || ast_utils::hook_name(filename) {
            match decl {
                Declaration::Expression(expr)
                    if matches!(
                        expr.deref(),
                        ExpressionInner::ArrowFunction { .. } | ExpressionInner::Function { .. }
                    ) =>
                {
                    true
                }
                Declaration::Declaration(stmt)
                    if matches!(
                        stmt.deref(),
                        ast::statement::StatementInner::FunctionDeclaration { .. }
                    ) =>
                {
                    true
                }
                _ => false,
            }
        } else {
            false
        };
        self.in_context_possibly_expecting_fn_component_or_hook = next_in_context || cur_in_context;
        ast_visitor::export_default_declaration_decl_default(self, decl)?;
        self.in_context_possibly_expecting_fn_component_or_hook = cur_in_context;
        Ok(())
    }

    fn object_property(
        &mut self,
        prop: &'t ast::expression::object::NormalProperty<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let cur_in_context = self.in_context_possibly_expecting_fn_component_or_hook;
        let next_in_context = match prop {
            NormalProperty::Method {
                key: ast::expression::object::Key::Identifier(ident),
                ..
            } => ast_utils::hook_name(ident.name.as_str()) || ident.name.as_str() == "render",
            NormalProperty::Init {
                key: ast::expression::object::Key::Identifier(ident),
                value,
                ..
            } if matches!(
                value.deref(),
                ExpressionInner::ArrowFunction { .. } | ExpressionInner::Function { .. }
            ) =>
            {
                ast_utils::hook_name(ident.name.as_str()) || ident.name.as_str() == "render"
            }
            _ => false,
        };
        // We permissively assume that it's a hook stored in object property.
        self.in_context_possibly_expecting_fn_component_or_hook = next_in_context;
        ast_visitor::object_property_default(self, prop)?;
        self.in_context_possibly_expecting_fn_component_or_hook = cur_in_context;
        Ok(())
    }

    fn class_body(
        &mut self,
        cls_body: &'t ast::class::Body<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let cur_hook_call_context = self.hook_call_context.dupe();
        self.hook_call_context = Rc::new(flow_lazy::Lazy::new_forced(Ok(
            HookCallContext::HookCallNotAllowedInClass,
        )));
        ast_visitor::class_body_default(self, cls_body)?;
        self.hook_call_context = cur_hook_call_context;
        Ok(())
    }

    fn return_(
        &mut self,
        loc: &'t ALoc,
        ret: &'t ast::statement::Return<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let cur_in_context = self.in_context_possibly_expecting_fn_component_or_hook;
        let next_in_context = match &ret.argument {
            Some(expr)
                if matches!(
                    expr.deref(),
                    ExpressionInner::ArrowFunction { .. } | ExpressionInner::Function { .. }
                ) =>
            {
                self.cx.hook_compatibility()
            }
            _ => false,
        };
        // We permissively assume that we are returning a fn component or hook.
        self.in_context_possibly_expecting_fn_component_or_hook = next_in_context;
        ast_visitor::return_default(self, loc, ret)?;
        self.in_context_possibly_expecting_fn_component_or_hook = cur_in_context;
        Ok(())
    }

    fn body_expression(
        &mut self,
        expr: &'t ast::expression::Expression<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let cur_in_context = self.in_context_possibly_expecting_fn_component_or_hook;
        let next_in_context = match expr.deref() {
            ExpressionInner::ArrowFunction { .. } | ExpressionInner::Function { .. } => {
                self.cx.hook_compatibility()
            }
            _ => false,
        };
        // We permissively assume that the anonymous function can be a fn component or hook.
        self.in_context_possibly_expecting_fn_component_or_hook = next_in_context;
        ast_visitor::body_expression_default(self, expr)?;
        self.in_context_possibly_expecting_fn_component_or_hook = cur_in_context;
        Ok(())
    }

    fn variable_declarator(
        &mut self,
        kind: ast::VariableKind,
        decl: &'t ast::statement::variable::Declarator<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let id_pat = &decl.id;
        let init = &decl.init;
        self.variable_declarator_pattern(kind, id_pat)?;
        let cur_in_context = self.in_context_possibly_expecting_fn_component_or_hook;
        let (next_in_context, effective_id) = match (id_pat, init) {
            (
                Pattern::Identifier {
                    inner: pat_ident, ..
                },
                Some(init_expr),
            ) if let Some((_fn_loc, f)) = match init_expr.deref() {
                ExpressionInner::ArrowFunction { loc, inner: f }
                | ExpressionInner::Function { loc, inner: f } => Some((loc, f)),
                _ => None,
            } =>
            {
                let name = pat_ident.name.name.as_str();
                let id = &pat_ident.name;
                let next = self.cx.hook_compatibility()
                    && (ast_utils::hook_name(name) || componentlike_name(name));
                /*
                If the name of the function is missing, we give it the name of the variable it binds to.
                Later, we will visit the function. This helps to catch bad code like:

                ```
                const badHookName = () => {
                  useState()
                }
                ```
                */
                let new_id = f.id.as_ref().unwrap_or(id).dupe();
                (next, Some(new_id))
            }
            _ => (false, None),
        };
        self.in_context_possibly_expecting_fn_component_or_hook = next_in_context;
        match (&effective_id, init) {
            (Some(_), Some(init_expr)) => {
                // Visit the function directly using the original AST (which has 'a lifetime)
                // but with the effective id for hook detection purposes.
                match init_expr.deref() {
                    ExpressionInner::ArrowFunction { loc, inner: f }
                    | ExpressionInner::Function { loc, inner: f } => {
                        let hint_loc = loc.0.dupe();
                        self.visit_function_with_id(Some(hint_loc), f, effective_id.as_ref())?;
                    }
                    _ => unreachable!(),
                }
            }
            _ => {
                if let Some(init_expr) = &decl.init {
                    self.expression(init_expr)?;
                }
            }
        }
        self.in_context_possibly_expecting_fn_component_or_hook = cur_in_context;
        Ok(())
    }
}

struct ComponentAstVisitor<'b, 'cx, 't> {
    tast: &'t ast::Program<ALoc, (ALoc, Type)>,
    cx: &'b mut Context<'cx>,
    rrid: Option<type_::nominal::Id>,
    conditional_state: conditional_state::ConditionalState,
}

impl<'b, 'cx, 't> ComponentAstVisitor<'b, 'cx, 't> {
    fn new(
        tast: &'t ast::Program<ALoc, (ALoc, Type)>,
        cx: &'b mut Context<'cx>,
        rrid: Option<type_::nominal::Id>,
    ) -> Self {
        Self {
            tast,
            cx,
            rrid,
            conditional_state: conditional_state::ConditionalState::init(),
        }
    }

    fn visit_toplevel_component(
        &mut self,
        cmp: &'t ast::statement::ComponentDeclaration<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        ast_visitor::component_declaration_default(self, &cmp.sig_loc, cmp)
    }

    fn in_conditional<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let cur = self.conditional_state.enter_conditional();
        let res = f(self);
        self.conditional_state.reset_conditional(cur);
        res
    }

    fn visit_switch_case(
        &mut self,
        is_last: bool,
        case: &'t ast::statement::switch::Case<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        if case.test.is_some() || !is_last {
            self.in_conditional(|this| this.switch_case(case))?;
        } else {
            self.switch_case(case)?;
        }
        Ok(())
    }

    fn try_block(
        &mut self,
        block: &'t ast::statement::Block<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let (cur, cur_try) = self.conditional_state.enter_try();
        self.statement_list(&block.body)?;
        self.conditional_state.reset_try(cur, cur_try);
        Ok(())
    }

    fn function_component_body(
        &mut self,
        body: &'t ast::function::Body<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        ast_visitor::function_body_any_default(self, body)
    }
}

impl<'b, 'cx, 't>
    ast_visitor::AstVisitor<
        't,
        ALoc,
        (ALoc, Type),
        &'t ALoc,
        flow_utils_concurrency::job_error::JobError,
    > for ComponentAstVisitor<'b, 'cx, 't>
{
    fn normalize_loc(loc: &'t ALoc) -> &'t ALoc {
        loc
    }

    fn normalize_type(type_: &'t (ALoc, Type)) -> &'t ALoc {
        &type_.0
    }

    // While nested components are bad, we already emit `nested-component` lint errors by default.
    // There is no need to emit react-rule-hook related errors again by assuming hooks in the nested
    // components might be called conditionally.
    fn component_declaration(
        &mut self,
        _loc: &'t ALoc,
        cmp: &'t ast::statement::ComponentDeclaration<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let mut cav = ComponentAstVisitor::new(self.tast, self.cx, self.rrid.clone());
        cav.visit_toplevel_component(cmp)?;
        Ok(())
    }

    fn call(
        &mut self,
        annot: &'t (ALoc, Type),
        expr: &'t ast::expression::Call<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let call_loc = &annot.0;
        let callee = &expr.callee;
        let callee_loc = &callee.loc().0;
        let callee_ty = &callee.loc().1;

        let callee_ty = match callee.deref() {
            ExpressionInner::Identifier { inner: ident, .. }
                if self.cx.hook_compatibility() && ident.name.as_str() != "require" =>
            {
                // If we're in compatibility mode, we want to bail on intersections. But
                // the typed AST records the type of the overload we've selected, so we
                // never see the intersection to realize we need to bail! Instead in this
                // case we read from the environment.
                type_env::var_ref(None, self.cx, None, &ident.name, callee_loc.dupe())?
            }
            _ => callee_ty.dupe(),
        };
        let do_hook_error = |cx: &mut Context<'cx>, kind: error_message::HookRule<ALoc>| {
            hook_error(cx, call_loc.dupe(), callee_loc.dupe(), kind);
        };
        match hook_callee(self.cx, callee_ty) {
            HookResult::HookCallee(_) => {
                if matches!(callee.deref(), ExpressionInner::OptionalMember { .. }) {
                    do_hook_error(self.cx, error_message::HookRule::ConditionalHook);
                } else if ast_utils::hook_call(expr) {
                    if self.conditional_state.conditional() && !bare_use(expr) {
                        do_hook_error(self.cx, error_message::HookRule::ConditionalHook);
                    }
                } else {
                    do_hook_error(self.cx, error_message::HookRule::HookHasIllegalName);
                }
            }
            HookResult::MaybeHookCallee { hooks, non_hooks } => {
                do_hook_error(
                    self.cx,
                    error_message::HookRule::MaybeHook {
                        hooks: hooks.into_iter().map(|hook| hook.0).collect(),
                        non_hooks: non_hooks.into_iter().map(|non_hook| non_hook.0).collect(),
                    },
                );
            }
            HookResult::NotHookCallee(_) => {
                if ast_utils::hook_call(expr) {
                    do_hook_error(self.cx, error_message::HookRule::NotHookSyntaxHook);
                }
            }
            HookResult::AnyCallee => {}
        }

        ast_visitor::call_default(self, annot, expr)?;
        self.conditional_state.throwable();
        Ok(())
    }

    fn new(
        &mut self,
        loc: &'t (ALoc, Type),
        expr: &'t ast::expression::New<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        ast_visitor::new_default(self, loc, expr)?;
        self.conditional_state.throwable();
        Ok(())
    }

    fn throw(
        &mut self,
        loc: &'t ALoc,
        throw: &'t ast::statement::Throw<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        ast_visitor::throw_default(self, loc, throw)?;
        self.conditional_state.throwable();
        Ok(())
    }

    fn return_(
        &mut self,
        loc: &'t ALoc,
        ret: &'t ast::statement::Return<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        ast_visitor::return_default(self, loc, ret)?;
        self.conditional_state.do_return();
        Ok(())
    }

    fn labeled_statement(
        &mut self,
        loc: &'t ALoc,
        stmt: &'t ast::statement::Labeled<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let name = stmt.label.name.dupe();
        let cur = self.conditional_state.enter_label(name.dupe());
        ast_visitor::labeled_statement_default(self, loc, stmt)?;
        self.conditional_state.reset_label(&name, cur);
        Ok(())
    }

    fn switch(
        &mut self,
        _loc: &'t ALoc,
        stmt: &'t ast::statement::Switch<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let (cur, cur_switch) = self.conditional_state.enter_switch_or_loop();
        self.expression(&stmt.discriminant)?;
        let cases_len = stmt.cases.len();
        for (i, case) in stmt.cases.iter().enumerate() {
            self.visit_switch_case(i == cases_len - 1, case)?;
        }
        self.conditional_state.reset_switch_or_loop(cur, cur_switch);
        Ok(())
    }

    fn break_(
        &mut self,
        loc: &'t ALoc,
        break_stmt: &'t ast::statement::Break<ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let s = break_stmt.label.as_ref().map(|ident| ident.name.dupe());
        self.conditional_state.do_break(s);
        ast_visitor::break_default(self, loc, break_stmt)
    }

    fn try_catch(
        &mut self,
        _loc: &'t ALoc,
        stmt: &'t ast::statement::Try<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let block = &stmt.block.1;
        let pre_cond = self.conditional_state.dupe();
        self.try_block(block)?;
        if let Some(handler) = &stmt.handler {
            self.in_conditional(|this| this.catch_clause(handler))?;
        }
        let post_cond = self.conditional_state.dupe();
        self.conditional_state = pre_cond;
        if let Some(finalizer) = &stmt.finalizer {
            self.block(&finalizer.0, &finalizer.1)?;
        }
        self.conditional_state =
            conditional_state::ConditionalState::merge(&post_cond, &self.conditional_state);
        Ok(())
    }

    fn if_consequent_statement(
        &mut self,
        has_else: bool,
        stmt: &'t ast::statement::Statement<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        self.in_conditional(|this| {
            ast_visitor::if_consequent_statement_default(this, has_else, stmt)
        })?;
        Ok(())
    }

    fn if_alternate_statement(
        &mut self,
        altern: &'t ast::statement::if_::Alternate<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        self.in_conditional(|this| ast_visitor::if_alternate_statement_default(this, altern))?;
        Ok(())
    }

    fn conditional(
        &mut self,
        _loc: &'t (ALoc, Type),
        expr: &'t ast::expression::Conditional<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        self.predicate_expression(&expr.test)?;
        self.in_conditional(|this| this.expression(&expr.consequent))?;
        self.in_conditional(|this| this.expression(&expr.alternate))?;
        Ok(())
    }

    fn logical(
        &mut self,
        _loc: &'t (ALoc, Type),
        expr: &'t ast::expression::Logical<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        self.expression(&expr.left)?;
        self.in_conditional(|this| this.expression(&expr.right))?;
        Ok(())
    }

    fn for_in_statement(
        &mut self,
        _loc: &'t ALoc,
        stmt: &'t ast::statement::ForIn<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        self.for_in_statement_lhs(&stmt.left)?;
        self.expression(&stmt.right)?;
        let (cur, cur_loop) = self.conditional_state.enter_switch_or_loop();
        self.in_conditional(|this| this.statement(&stmt.body))?;
        self.conditional_state.reset_switch_or_loop(cur, cur_loop);
        Ok(())
    }

    fn for_of_statement(
        &mut self,
        _loc: &'t ALoc,
        stmt: &'t ast::statement::ForOf<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        self.for_of_statement_lhs(&stmt.left)?;
        self.expression(&stmt.right)?;
        let (cur, cur_loop) = self.conditional_state.enter_switch_or_loop();
        self.in_conditional(|this| this.statement(&stmt.body))?;
        self.conditional_state.reset_switch_or_loop(cur, cur_loop);
        Ok(())
    }

    fn for_statement(
        &mut self,
        _loc: &'t ALoc,
        stmt: &'t ast::statement::For<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        if let Some(init) = &stmt.init {
            self.for_statement_init(init)?;
        }
        let (cur, cur_loop) = self.conditional_state.enter_switch_or_loop();
        if let Some(test) = &stmt.test {
            self.predicate_expression(test)?;
        }
        if let Some(update) = &stmt.update {
            self.in_conditional(|this| this.expression(update))?;
        }
        self.in_conditional(|this| this.statement(&stmt.body))?;
        self.conditional_state.reset_switch_or_loop(cur, cur_loop);
        Ok(())
    }

    fn while_(
        &mut self,
        _loc: &'t ALoc,
        stmt: &'t ast::statement::While<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let (cur, cur_loop) = self.conditional_state.enter_switch_or_loop();
        self.predicate_expression(&stmt.test)?;
        self.in_conditional(|this| this.statement(&stmt.body))?;
        self.conditional_state.reset_switch_or_loop(cur, cur_loop);
        Ok(())
    }

    fn function_(
        &mut self,
        _loc: &'t ALoc,
        expr: &'t ast::function::Function<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        if let Some(ident) = &expr.id {
            self.function_identifier(ident)?;
        }
        if let Some(tparams) = &expr.tparams {
            self.type_params(&ast_visitor::TypeParamsContext::Function, tparams)?;
        }
        self.function_params(&expr.params)?;
        self.function_return_annotation(&expr.return_)?;
        {
            let initial_ctx = if expr.effect_ == ast::function::Effect::Hook {
                HookCallContext::HookCallPermissivelyAllowedUnderCompatibilityMode
            } else {
                HookCallContext::HookCallNotAllowedUnderNormalFunctionInComponentOrHooks
            };
            let mut wav =
                WholeAstVisitor::new(self.tast, true, initial_ctx, self.cx, self.rrid.clone());
            wav.function_body_any(&expr.body)?;
        }
        if let Some(predicate) = &expr.predicate {
            self.predicate(predicate)?;
        }
        Ok(())
    }

    fn class_body(
        &mut self,
        cls_body: &'t ast::class::Body<ALoc, (ALoc, Type)>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let mut wav = WholeAstVisitor::new(
            self.tast,
            true,
            HookCallContext::HookCallNotAllowedInClass,
            self.cx,
            self.rrid.clone(),
        );
        wav.class_body(cls_body)
    }

    fn match_case<B>(
        &mut self,
        case: &'t ast::match_::Case<ALoc, (ALoc, Type), B>,
        on_case_body: &mut impl FnMut(
            &mut Self,
            &'t B,
        )
            -> Result<(), flow_utils_concurrency::job_error::JobError>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        self.in_conditional(|this| ast_visitor::match_case_default(this, case, on_case_body))
    }
}

pub fn check_react_rules<'cx>(
    cx: &mut Context<'cx>,
    ast: &ast::Program<ALoc, (ALoc, Type)>,
) -> Result<(), flow_utils_concurrency::job_error::JobError> {
    let rrid = {
        let get_t = |cx, pair: &(ALoc, Type)| -> Type {
            let (_, t) = pair;
            match t.deref() {
                TypeInner::OpenT(tvar) => {
                    let r = flow_typing_type::type_util::reason_of_t(t);
                    flow_js_utils::merge_tvar(
                        cx,
                        false,
                        |_cx, r| type_::unsoundness::merged_any(r.dupe()),
                        r,
                        tvar.id() as i32,
                    )
                }
                _ => t.dupe(),
            }
        };
        let lhs = cx.builtin_type_opt("React$RefObject");
        if let Some(pair) = lhs.as_ref()
            && let t = get_t(cx, pair)
            && let TypeInner::DefT(_, def_t) = t.deref()
            && let DefTInner::PolyT(box PolyTData { t_out, .. }) = def_t.deref()
            && let TypeInner::DefT(_, inner_def_t) = t_out.deref()
            && let DefTInner::TypeT(_, inner_t) = inner_def_t.deref()
            && let TypeInner::NominalT { nominal_type, .. } = inner_t.deref()
        {
            Some(nominal_type.nominal_id.clone())
        } else {
            None
        }
    };

    let mut visitor = WholeAstVisitor::new(
        ast,
        false,
        HookCallContext::HookCallNotAllowedUnderUnknownContext,
        cx,
        rrid,
    );
    visitor.program(ast)
}
