/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ops::Deref;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocMap;
use flow_analysis::infer_type_hoister;
use flow_analysis::infer_type_hoister::ImpliedBound;
use flow_common::enclosing_context::EnclosingContext;
use flow_common::hint::Hint;
use flow_common::hint::HintDecomposition;
use flow_common::hint::HintDecompositionInner;
use flow_common::hint::HintKind;
use flow_common::hint::PredicateKind;
use flow_common::js_number;
use flow_common::reason::VirtualReason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::reason::func_reason;
use flow_common::reason::mk_reason;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_data_structure_wrapper::vector::FlowVector;
use flow_parser::ast;
use flow_parser::ast::IdentifierInner;
use flow_parser::ast::expression::Expression;
use flow_parser::ast::expression::object;
use flow_parser::ast::types::AnnotationOrHint;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::loc_sig::LocSig;
use flow_parser_utils::type_param_analysis::analyze_type_params;

use crate::assertion_call_target::AssertionInfo;
use crate::assertion_call_target::AssertionTarget;
use crate::assertion_call_target::bare_asserted_argument;
use crate::env_api::AutocompleteHooks;
use crate::env_api::DefLocType;
use crate::env_api::EnvInfo;
use crate::env_api::EnvInvariant;
use crate::env_api::EnvInvariantFailure;
use crate::env_api::EnvKey;
use crate::env_api::EnvMap;
use crate::env_api::EnvSet;
use crate::env_api::ValKind;
use crate::env_api::has_assigning_write;
use crate::name_def_types::*;
use crate::nonvoid_return;
use crate::provider_api;
use crate::selector::Selector;

/// Trait for provider API access, used by DefFinder to check provider information.
/// This mirrors OCaml's Env_api.Provider_api module signature.
pub trait ProviderApi {
    fn is_provider(&self, loc: &ALoc) -> bool;
    fn providers_of_def(&self, loc: &ALoc) -> Option<&provider_api::DefProviders<ALoc>>;
}

impl ProviderApi for provider_api::Info<ALoc> {
    fn is_provider(&self, loc: &ALoc) -> bool {
        self.is_provider(loc)
    }
    fn providers_of_def(&self, loc: &ALoc) -> Option<&provider_api::DefProviders<ALoc>> {
        self.providers_of_def(loc)
    }
}

mod destructure {
    use super::*;

    pub(super) fn type_of_pattern(
        pattern: &ast::pattern::Pattern<ALoc, ALoc>,
    ) -> Option<&ast::types::Annotation<ALoc, ALoc>> {
        use ast::pattern::Pattern;
        match pattern {
            Pattern::Array { inner, .. } => match &inner.annot {
                AnnotationOrHint::Available(t) => Some(t),
                AnnotationOrHint::Missing(_) => None,
            },
            Pattern::Object { inner, .. } => match &inner.annot {
                AnnotationOrHint::Available(t) => Some(t),
                AnnotationOrHint::Missing(_) => None,
            },
            Pattern::Identifier { inner, .. } => match &inner.annot {
                AnnotationOrHint::Available(t) => Some(t),
                AnnotationOrHint::Missing(_) => None,
            },
            Pattern::Expression { .. } => None,
        }
    }

    fn array_element(parent_loc: ALoc, acc: Binding, index: usize, has_default: bool) -> Binding {
        let selector = Selector::Elem { index, has_default };
        Binding::Select {
            selector,
            parent: (parent_loc, Box::new(acc)),
        }
    }

    fn array_rest_element(parent_loc: ALoc, acc: Binding, i: usize) -> Binding {
        let selector = Selector::ArrRest(i);
        Binding::Select {
            selector,
            parent: (parent_loc, Box::new(acc)),
        }
    }

    fn object_named_property(
        parent_loc: ALoc,
        acc: Binding,
        prop_loc: ALoc,
        prop: FlowSmolStr,
        has_default: bool,
    ) -> Binding {
        let selector = Selector::Prop {
            prop,
            prop_loc,
            has_default,
        };
        Binding::Select {
            selector,
            parent: (parent_loc, Box::new(acc)),
        }
    }

    fn object_computed_property(
        parent_loc: ALoc,
        acc: Binding,
        e: ast::expression::Expression<ALoc, ALoc>,
        has_default: bool,
    ) -> Binding {
        let selector = Selector::Computed {
            expression: e,
            has_default,
        };
        Binding::Select {
            selector,
            parent: (parent_loc, Box::new(acc)),
        }
    }

    fn object_rest_property(
        parent_loc: ALoc,
        acc: Binding,
        xs: Vec<FlowSmolStr>,
        has_computed: bool,
    ) -> Binding {
        let selector = Selector::ObjRest {
            used_props: xs,
            after_computed: has_computed,
        };
        Binding::Select {
            selector,
            parent: (parent_loc, Box::new(acc)),
        }
    }

    fn object_property(
        parent_loc: ALoc,
        acc: Binding,
        xs: &mut Vec<FlowSmolStr>,
        key: &ast::pattern::object::Key<ALoc, ALoc>,
        has_default: bool,
    ) -> (Binding, bool) {
        use ast::pattern::object::Key;
        match key {
            Key::Identifier(id) => {
                let new_acc = object_named_property(
                    parent_loc,
                    acc,
                    id.loc.dupe(),
                    id.name.dupe(),
                    has_default,
                );
                xs.push(id.name.dupe());
                (new_acc, false)
            }
            Key::StringLiteral((loc, lit)) => {
                let new_acc = object_named_property(
                    parent_loc,
                    acc,
                    loc.dupe(),
                    lit.value.dupe(),
                    has_default,
                );
                xs.push(lit.value.dupe());
                (new_acc, false)
            }
            Key::Computed(computed) => {
                let new_acc = object_computed_property(
                    parent_loc,
                    acc,
                    computed.expression.dupe(),
                    has_default,
                );
                (new_acc, true)
            }
            Key::NumberLiteral((loc, num)) => {
                if let Some(name) = js_number::numeric_property_name(num.value) {
                    let new_acc = object_named_property(
                        parent_loc,
                        acc,
                        loc.dupe(),
                        name.dupe(),
                        has_default,
                    );
                    xs.push(name);
                    (new_acc, false)
                } else {
                    (acc, false)
                }
            }
            Key::BigIntLiteral(_) => (acc, false),
        }
    }

    fn identifier<R>(
        record_identifier: &mut impl FnMut(ALoc, &FlowSmolStr, Binding) -> R,
        acc: Binding,
        id: &ast::Identifier<ALoc, ALoc>,
    ) -> R {
        record_identifier(id.loc.dupe(), &id.name, acc)
    }

    pub(super) fn fold_pattern<R: Clone>(
        record_identifier: &mut impl FnMut(ALoc, &FlowSmolStr, Binding) -> R,
        record_destructuring_intermediate: &mut impl FnMut(ALoc, Binding),
        visit_default_expression: &mut impl FnMut(&AstHints, &ast::expression::Expression<ALoc, ALoc>),
        join: fn(R, R) -> R,
        default: R,
        acc: Binding,
        pattern: &ast::pattern::Pattern<ALoc, ALoc>,
    ) -> R {
        use ast::pattern::Pattern;
        match pattern {
            Pattern::Array { loc, inner } => {
                record_destructuring_intermediate(loc.dupe(), acc.clone());
                array_elements(
                    record_identifier,
                    record_destructuring_intermediate,
                    visit_default_expression,
                    join,
                    default,
                    (loc.dupe(), acc),
                    &inner.elements,
                )
            }
            Pattern::Object { loc, inner } => {
                record_destructuring_intermediate(loc.dupe(), acc.clone());
                object_properties(
                    record_identifier,
                    record_destructuring_intermediate,
                    visit_default_expression,
                    join,
                    default,
                    (loc.dupe(), acc),
                    &inner.properties,
                )
            }
            Pattern::Identifier { inner, .. } => identifier(record_identifier, acc, &inner.name),
            Pattern::Expression { .. } => default,
        }
    }

    fn pattern_hint(pattern: &ast::pattern::Pattern<ALoc, ALoc>) -> AstHints {
        use ast::pattern::Pattern;
        let loc = pattern.loc().dupe();
        match pattern {
            Pattern::Identifier { .. } => {
                vec![Hint::HintT(
                    HintNode::WriteLocHint(DefLocType::OrdinaryNameLoc, loc),
                    HintKind::ExpectedTypeHint,
                )]
            }
            _ => {
                vec![Hint::HintT(
                    HintNode::WriteLocHint(DefLocType::PatternLoc, loc),
                    HintKind::ExpectedTypeHint,
                )]
            }
        }
    }

    fn array_elements<R: Clone>(
        record_identifier: &mut impl FnMut(ALoc, &FlowSmolStr, Binding) -> R,
        record_destructuring_intermediate: &mut impl FnMut(ALoc, Binding),
        visit_default_expression: &mut impl FnMut(&AstHints, &ast::expression::Expression<ALoc, ALoc>),
        join: fn(R, R) -> R,
        default: R,
        (parent_loc, acc): (ALoc, Binding),
        elts: &[ast::pattern::array::Element<ALoc, ALoc>],
    ) -> R {
        use ast::pattern::array::Element;
        let mut result = default.clone();
        for (i, elt) in elts.iter().enumerate() {
            let res = match elt {
                Element::Hole(_) => default.clone(),
                Element::NormalElement(elem) => {
                    let p = &elem.argument;
                    let hints = pattern_hint(p);
                    if let Some(d) = &elem.default {
                        visit_default_expression(&hints, d);
                    }
                    let new_acc =
                        array_element(parent_loc.dupe(), acc.clone(), i, elem.default.is_some());
                    fold_pattern(
                        record_identifier,
                        record_destructuring_intermediate,
                        visit_default_expression,
                        join,
                        default.clone(),
                        new_acc,
                        p,
                    )
                }
                Element::RestElement(rest) => {
                    let p = &rest.argument;
                    let new_acc = array_rest_element(parent_loc.dupe(), acc.clone(), i);
                    fold_pattern(
                        record_identifier,
                        record_destructuring_intermediate,
                        visit_default_expression,
                        join,
                        default.clone(),
                        new_acc,
                        p,
                    )
                }
            };
            result = join(result, res);
        }
        result
    }

    fn object_properties<R: Clone>(
        record_identifier: &mut impl FnMut(ALoc, &FlowSmolStr, Binding) -> R,
        record_destructuring_intermediate: &mut impl FnMut(ALoc, Binding),
        visit_default_expression: &mut impl FnMut(&AstHints, &ast::expression::Expression<ALoc, ALoc>),
        join: fn(R, R) -> R,
        default: R,
        (parent_loc, acc): (ALoc, Binding),
        props: &[ast::pattern::object::Property<ALoc, ALoc>],
    ) -> R {
        use ast::pattern::object::Property;

        fn prop<R: Clone>(
            record_identifier: &mut impl FnMut(ALoc, &FlowSmolStr, Binding) -> R,
            record_destructuring_intermediate: &mut impl FnMut(ALoc, Binding),
            visit_default_expression: &mut impl FnMut(
                &AstHints,
                &ast::expression::Expression<ALoc, ALoc>,
            ),
            join: fn(R, R) -> R,
            default: R,
            (parent_loc, acc): (ALoc, Binding),
            xs: &mut Vec<FlowSmolStr>,
            has_computed: bool,
            p: &ast::pattern::object::Property<ALoc, ALoc>,
        ) -> (R, bool) {
            match p {
                Property::NormalProperty(property) => {
                    let pattern = &property.pattern;
                    let hints = pattern_hint(pattern);
                    if let Some(d) = &property.default {
                        visit_default_expression(&hints, d);
                    }
                    let (new_acc, has_computed_new) = object_property(
                        parent_loc,
                        acc,
                        xs,
                        &property.key,
                        property.default.is_some(),
                    );
                    let res = fold_pattern(
                        record_identifier,
                        record_destructuring_intermediate,
                        visit_default_expression,
                        join,
                        default,
                        new_acc,
                        pattern,
                    );
                    (res, has_computed || has_computed_new)
                }
                Property::RestElement(rest) => {
                    let pattern = &rest.argument;
                    let new_acc = object_rest_property(parent_loc, acc, xs.clone(), has_computed);
                    let res = fold_pattern(
                        record_identifier,
                        record_destructuring_intermediate,
                        visit_default_expression,
                        join,
                        default,
                        new_acc,
                        pattern,
                    );
                    (res, false)
                }
            }
        }

        let mut result = default.clone();
        let mut xs: Vec<FlowSmolStr> = Vec::new();
        let mut has_computed = false;
        for p in props {
            let (res, new_has_computed) = prop(
                record_identifier,
                record_destructuring_intermediate,
                visit_default_expression,
                join,
                default.clone(),
                (parent_loc.dupe(), acc.clone()),
                &mut xs,
                has_computed,
                p,
            );
            result = join(result, res);
            has_computed = new_has_computed;
        }
        result
    }

    pub(super) fn pattern(
        record_identifier: &mut impl FnMut(ALoc, &FlowSmolStr, Binding),
        record_destructuring_intermediate: &mut impl FnMut(ALoc, Binding),
        visit_default_expression: &mut impl FnMut(&AstHints, &ast::expression::Expression<ALoc, ALoc>),
        acc: Binding,
        pattern: &ast::pattern::Pattern<ALoc, ALoc>,
    ) {
        fold_pattern(
            &mut |loc, name, binding| record_identifier(loc, name, binding),
            record_destructuring_intermediate,
            visit_default_expression,
            |_, _| (),
            (),
            acc,
            pattern,
        );
    }
}

mod match_pattern {
    use super::*;

    fn array_element(acc: (ALoc, Binding), index: usize) -> Binding {
        let selector = Selector::Elem {
            index,
            has_default: false,
        };
        Binding::Select {
            selector,
            parent: (acc.0, Box::new(acc.1)),
        }
    }

    fn array_rest(acc: (ALoc, Binding), i: usize) -> Binding {
        let selector = Selector::ArrRest(i);
        Binding::Select {
            selector,
            parent: (acc.0, Box::new(acc.1)),
        }
    }

    fn object_property(
        acc: (ALoc, Binding),
        key: &ast::match_pattern::object_pattern::Key<ALoc, ALoc>,
    ) -> (Binding, FlowSmolStr) {
        use ast::match_pattern::object_pattern::Key;
        match key {
            Key::Identifier(id) => {
                let selector = Selector::Prop {
                    prop: id.name.dupe(),
                    prop_loc: id.loc.dupe(),
                    has_default: false,
                };
                (
                    Binding::Select {
                        selector,
                        parent: (acc.0, Box::new(acc.1)),
                    },
                    id.name.dupe(),
                )
            }
            Key::StringLiteral((loc, lit)) => {
                let selector = Selector::Prop {
                    prop: lit.value.dupe(),
                    prop_loc: loc.dupe(),
                    has_default: false,
                };
                (
                    Binding::Select {
                        selector,
                        parent: (acc.0, Box::new(acc.1)),
                    },
                    lit.value.dupe(),
                )
            }
            Key::NumberLiteral((loc, num)) => {
                if let Some(prop) = js_number::numeric_property_name(num.value) {
                    let selector = Selector::Prop {
                        prop: prop.dupe(),
                        prop_loc: loc.dupe(),
                        has_default: false,
                    };
                    (
                        Binding::Select {
                            selector,
                            parent: (acc.0, Box::new(acc.1)),
                        },
                        prop,
                    )
                } else {
                    (acc.1, FlowSmolStr::default())
                }
            }
            Key::BigIntLiteral(_) => (acc.1, FlowSmolStr::default()),
        }
    }

    fn object_rest(acc: (ALoc, Binding), used_props: Vec<FlowSmolStr>) -> Binding {
        let selector = Selector::ObjRest {
            used_props,
            after_computed: false,
        };
        Binding::Select {
            selector,
            parent: (acc.0, Box::new(acc.1)),
        }
    }

    fn binding(
        visit_binding: &mut impl FnMut(ALoc, &FlowSmolStr, Binding),
        acc: Binding,
        id: &ast::Identifier<ALoc, ALoc>,
    ) {
        visit_binding(id.loc.dupe(), &id.name, acc);
    }

    pub(super) fn visit_pattern(
        visit_binding: &mut impl FnMut(ALoc, &FlowSmolStr, Binding),
        visit_non_binding_leaf: &mut impl FnMut(ALoc, Binding),
        visit_expression: &mut impl FnMut(&ast::expression::Expression<ALoc, ALoc>),
        visit_intermediate: &mut impl FnMut(ALoc, Binding),
        acc: Binding,
        pattern: &ast::match_pattern::MatchPattern<ALoc, ALoc>,
    ) {
        use ast::expression::ExpressionInner as ExprInner;
        use ast::match_pattern::MatchPattern;

        match pattern {
            MatchPattern::BindingPattern { inner, .. } => {
                binding(visit_binding, acc, &inner.id);
            }
            MatchPattern::NumberPattern { loc, inner } => {
                visit_non_binding_leaf(loc.dupe(), acc);
                visit_expression(&Expression::new(ExprInner::NumberLiteral {
                    loc: loc.dupe(),
                    inner: Arc::new((**inner).clone()),
                }));
            }
            MatchPattern::BigIntPattern { loc, inner } => {
                visit_non_binding_leaf(loc.dupe(), acc);
                visit_expression(&Expression::new(ExprInner::BigIntLiteral {
                    loc: loc.dupe(),
                    inner: Arc::new((**inner).clone()),
                }));
            }
            MatchPattern::StringPattern { loc, inner } => {
                visit_non_binding_leaf(loc.dupe(), acc);
                visit_expression(&Expression::new(ExprInner::StringLiteral {
                    loc: loc.dupe(),
                    inner: Arc::new((**inner).clone()),
                }));
            }
            MatchPattern::BooleanPattern { loc, inner } => {
                visit_non_binding_leaf(loc.dupe(), acc);
                visit_expression(&Expression::new(ExprInner::BooleanLiteral {
                    loc: loc.dupe(),
                    inner: Arc::new((**inner).clone()),
                }));
            }
            MatchPattern::NullPattern { loc, inner } => {
                visit_non_binding_leaf(loc.dupe(), acc);
                visit_expression(&Expression::new(ExprInner::NullLiteral {
                    loc: loc.dupe(),
                    inner: Arc::new((**inner).clone()),
                }));
            }
            MatchPattern::IdentifierPattern { loc, inner } => {
                visit_non_binding_leaf(loc.dupe(), acc);
                visit_expression(&Expression::new(ExprInner::Identifier {
                    loc: loc.dupe(),
                    inner: (**inner).dupe(),
                }));
            }
            MatchPattern::WildcardPattern { loc, .. } => {
                visit_non_binding_leaf(loc.dupe(), acc);
            }
            MatchPattern::UnaryPattern { loc, inner } => {
                use ast::expression::Unary as ExprUnary;
                use ast::expression::UnaryOperator as ExprUnaryOp;
                use ast::match_pattern::unary_pattern::Argument;
                use ast::match_pattern::unary_pattern::Operator;

                let operator = match inner.operator {
                    Operator::Plus => ExprUnaryOp::Plus,
                    Operator::Minus => ExprUnaryOp::Minus,
                };
                let (arg_loc, arg_inner) = &inner.argument;
                let argument: Expression<ALoc, ALoc> = match arg_inner {
                    Argument::NumberLiteral(lit) => Expression::new(ExprInner::NumberLiteral {
                        loc: arg_loc.dupe(),
                        inner: Arc::new(lit.clone()),
                    }),
                    Argument::BigIntLiteral(lit) => Expression::new(ExprInner::BigIntLiteral {
                        loc: arg_loc.dupe(),
                        inner: Arc::new(lit.clone()),
                    }),
                };
                visit_non_binding_leaf(loc.dupe(), acc);
                visit_expression(&Expression::new(ExprInner::Unary {
                    loc: loc.dupe(),
                    inner: Arc::new(ExprUnary {
                        operator,
                        argument,
                        comments: inner.comments.dupe(),
                    }),
                }));
            }
            MatchPattern::MemberPattern { loc, inner } => {
                visit_non_binding_leaf(loc.dupe(), acc);
                flow_parser::ast_utils::expression_of_match_member_pattern(visit_expression, inner);
            }
            MatchPattern::ArrayPattern { loc, inner } => {
                array_pattern(
                    visit_binding,
                    visit_non_binding_leaf,
                    visit_expression,
                    visit_intermediate,
                    loc.dupe(),
                    acc,
                    inner,
                );
            }
            MatchPattern::ObjectPattern { loc, inner } => {
                object_pattern(
                    visit_binding,
                    visit_non_binding_leaf,
                    visit_expression,
                    visit_intermediate,
                    loc.dupe(),
                    acc,
                    inner,
                );
            }
            MatchPattern::InstancePattern { loc, inner } => {
                use ast::match_pattern::InstancePatternConstructor;
                match &inner.constructor {
                    InstancePatternConstructor::IdentifierConstructor(id) => {
                        visit_expression(&Expression::new(
                            ast::expression::ExpressionInner::Identifier {
                                loc: id.loc.dupe(),
                                inner: id.dupe(),
                            },
                        ));
                    }
                    InstancePatternConstructor::MemberConstructor(mem) => {
                        flow_parser::ast_utils::expression_of_match_member_pattern(
                            visit_expression,
                            mem,
                        );
                    }
                }
                let (_, properties) = &inner.properties;
                object_pattern(
                    visit_binding,
                    visit_non_binding_leaf,
                    visit_expression,
                    visit_intermediate,
                    loc.dupe(),
                    acc,
                    properties,
                );
            }
            MatchPattern::OrPattern { inner, .. } => {
                for p in inner.patterns.iter() {
                    visit_pattern(
                        visit_binding,
                        visit_non_binding_leaf,
                        visit_expression,
                        visit_intermediate,
                        acc.clone(),
                        p,
                    );
                }
            }
            MatchPattern::AsPattern { inner, .. } => {
                visit_pattern(
                    visit_binding,
                    visit_non_binding_leaf,
                    visit_expression,
                    visit_intermediate,
                    acc.clone(),
                    &inner.pattern,
                );
                use ast::match_pattern::as_pattern::Target;
                match &inner.target {
                    Target::Binding { pattern: bp, .. } => {
                        binding(visit_binding, acc, &bp.id);
                    }
                    Target::Identifier(id) => {
                        binding(visit_binding, acc, id);
                    }
                }
            }
        }
    }

    fn array_pattern(
        visit_binding: &mut impl FnMut(ALoc, &FlowSmolStr, Binding),
        visit_non_binding_leaf: &mut impl FnMut(ALoc, Binding),
        visit_expression: &mut impl FnMut(&ast::expression::Expression<ALoc, ALoc>),
        visit_intermediate: &mut impl FnMut(ALoc, Binding),
        loc: ALoc,
        acc: Binding,
        pattern: &ast::match_pattern::ArrayPattern<ALoc, ALoc>,
    ) {
        visit_intermediate(loc.dupe(), acc.clone());
        let used_elements = array_elements(
            visit_binding,
            visit_non_binding_leaf,
            visit_expression,
            visit_intermediate,
            (loc.dupe(), acc.clone()),
            &pattern.elements,
        );
        if let Some(rest) = &pattern.rest {
            if let Some((_, bp)) = &rest.argument {
                let new_acc = array_rest((loc, acc), used_elements);
                binding(visit_binding, new_acc, &bp.id);
            }
        }
    }

    fn array_elements(
        visit_binding: &mut impl FnMut(ALoc, &FlowSmolStr, Binding),
        visit_non_binding_leaf: &mut impl FnMut(ALoc, Binding),
        visit_expression: &mut impl FnMut(&ast::expression::Expression<ALoc, ALoc>),
        visit_intermediate: &mut impl FnMut(ALoc, Binding),
        acc: (ALoc, Binding),
        elements: &[ast::match_pattern::array_pattern::Element<ALoc, ALoc>],
    ) -> usize {
        let mut i = 0;
        for elem in elements.iter() {
            let new_acc = array_element(acc.clone(), i);
            visit_pattern(
                visit_binding,
                visit_non_binding_leaf,
                visit_expression,
                visit_intermediate,
                new_acc,
                &elem.pattern,
            );
            i += 1;
        }
        i
    }

    fn object_pattern(
        visit_binding: &mut impl FnMut(ALoc, &FlowSmolStr, Binding),
        visit_non_binding_leaf: &mut impl FnMut(ALoc, Binding),
        visit_expression: &mut impl FnMut(&ast::expression::Expression<ALoc, ALoc>),
        visit_intermediate: &mut impl FnMut(ALoc, Binding),
        loc: ALoc,
        acc: Binding,
        pattern: &ast::match_pattern::ObjectPattern<ALoc, ALoc>,
    ) {
        visit_intermediate(loc.dupe(), acc.clone());
        let used_props = object_properties(
            visit_binding,
            visit_non_binding_leaf,
            visit_expression,
            visit_intermediate,
            (loc.dupe(), acc.clone()),
            &pattern.properties,
        );
        if let Some(rest) = &pattern.rest {
            if let Some((_, bp)) = &rest.argument {
                let new_acc = object_rest((loc, acc), used_props);
                binding(visit_binding, new_acc, &bp.id);
            }
        }
    }

    fn object_properties(
        visit_binding: &mut impl FnMut(ALoc, &FlowSmolStr, Binding),
        visit_non_binding_leaf: &mut impl FnMut(ALoc, Binding),
        visit_expression: &mut impl FnMut(&ast::expression::Expression<ALoc, ALoc>),
        visit_intermediate: &mut impl FnMut(ALoc, Binding),
        acc: (ALoc, Binding),
        properties: &[ast::match_pattern::object_pattern::Property<ALoc, ALoc>],
    ) -> Vec<FlowSmolStr> {
        use ast::match_pattern::object_pattern::Property;
        let mut used_props = Vec::new();
        for prop in properties.iter() {
            match prop {
                Property::Valid { property, .. } => {
                    let (new_acc, prop_name) = object_property(acc.clone(), &property.key);
                    visit_pattern(
                        visit_binding,
                        visit_non_binding_leaf,
                        visit_expression,
                        visit_intermediate,
                        new_acc,
                        &property.pattern,
                    );
                    used_props.push(prop_name);
                }
                Property::InvalidShorthand { .. } => {
                    // Skip invalid shorthand
                }
            }
        }
        used_props
    }
}

pub fn pattern_has_annot(p: &ast::pattern::Pattern<ALoc, ALoc>) -> bool {
    destructure::type_of_pattern(p).is_some()
}

fn func_is_synthesizable_from_annotation(
    assertion_calls: &BTreeMap<ALoc, AssertionInfo>,
    f: &ast::function::Function<ALoc, ALoc>,
) -> FunctionSynthKind {
    use ast::function::ReturnAnnot;
    match &f.return_ {
        ReturnAnnot::Available { .. } | ReturnAnnot::TypeGuard { .. } => {
            FunctionSynthKind::FunctionSynthesizable
        }
        ReturnAnnot::Missing(loc) => {
            if nonvoid_return::might_have_nonvoid_return(&ALoc::none(), f, assertion_calls)
                || f.generator
            {
                FunctionSynthKind::MissingReturn(loc.dupe())
            } else {
                FunctionSynthKind::FunctionSynthesizable
            }
        }
    }
}

fn obj_this_write_locs(obj: &ast::expression::Object<ALoc, ALoc>) -> EnvSet<ALoc> {
    use ast::expression::ExpressionInner;
    use ast::expression::object::NormalProperty;
    use ast::expression::object::Property;

    let mut acc = EnvSet::empty();
    for prop in obj.properties.iter() {
        match prop {
            Property::NormalProperty(normal_prop) => match normal_prop {
                NormalProperty::Method { .. } => {
                    // this-in-object is banned.
                }
                NormalProperty::Init { value, .. } => {
                    match value.deref() {
                        ExpressionInner::ArrowFunction { .. } => {
                            // Arrow functions don't bind `this`.
                        }
                        ExpressionInner::Function { loc: f_loc, inner }
                            if inner.params.this_.is_none() =>
                        {
                            acc.insert(EnvKey::new(DefLocType::FunctionThisLoc, f_loc.dupe()));
                        }
                        _ => {
                            // Everything else is impossible due to obj_properties_synthesizable check.
                        }
                    }
                }
                NormalProperty::Get { .. } | NormalProperty::Set { .. } => {
                    // Everything else is impossible due to obj_properties_synthesizable check
                }
            },
            Property::SpreadProperty(_) => {
                // Everything else is impossible due to obj_properties_synthesizable check
            }
        }
    }
    acc
}

fn obj_properties_synthesizable(
    assertion_calls: &BTreeMap<ALoc, AssertionInfo>,
    this_write_locs: EnvSet<ALoc>,
    obj: &ast::expression::Object<ALoc, ALoc>,
) -> ObjectSynthKind {
    use ast::expression::ExpressionInner;
    use ast::expression::object::Property;

    fn handle_fun(acc: &mut Vec<ObjectMissingAnnot>, synth: FunctionSynthKind) -> Result<(), ()> {
        match synth {
            FunctionSynthKind::FunctionSynthesizable => Ok(()),
            FunctionSynthKind::MissingReturn(loc) => {
                acc.push(ObjectMissingAnnot::FuncMissingAnnot(loc));
                Ok(())
            }
        }
    }

    fn synthesizable_expression(
        assertion_calls: &BTreeMap<ALoc, AssertionInfo>,
        acc: &mut Vec<ObjectMissingAnnot>,
        this_write_locs: &mut EnvSet<ALoc>,
        expr: &ast::expression::Expression<ALoc, ALoc>,
    ) -> Result<(), ()> {
        let elem_loc = expr.loc().dupe();
        match expr.deref() {
            ExpressionInner::StringLiteral { .. }
            | ExpressionInner::NumberLiteral { .. }
            | ExpressionInner::BooleanLiteral { .. }
            | ExpressionInner::NullLiteral { .. }
            | ExpressionInner::BigIntLiteral { .. }
            | ExpressionInner::RegExpLiteral { .. }
            | ExpressionInner::ModuleRefLiteral { .. }
            | ExpressionInner::Identifier { .. }
            | ExpressionInner::TypeCast { .. }
            | ExpressionInner::AsExpression { .. } => Ok(()),
            ExpressionInner::Member { inner, .. } => {
                let is_simple_member = matches!(
                    (inner.object.deref(), &inner.property),
                    (
                        ExpressionInner::Identifier { .. }
                            | ExpressionInner::TypeCast { .. }
                            | ExpressionInner::AsExpression { .. },
                        ast::expression::member::Property::PropertyIdentifier(_)
                    )
                );
                if is_simple_member {
                    Ok(())
                } else {
                    acc.push(ObjectMissingAnnot::OtherMissingAnnot(elem_loc));
                    Ok(())
                }
            }
            ExpressionInner::ArrowFunction { inner, .. }
            | ExpressionInner::Function { inner, .. } => handle_fun(
                acc,
                func_is_synthesizable_from_annotation(assertion_calls, inner),
            ),
            ExpressionInner::Object { inner, .. } => {
                match obj_properties_synthesizable(
                    assertion_calls,
                    obj_this_write_locs(inner),
                    inner,
                ) {
                    ObjectSynthKind::ObjectSynthesizable {
                        this_write_locs: new_this_write_locs,
                    } => {
                        for key in new_this_write_locs.iter() {
                            this_write_locs.insert(key.dupe());
                        }
                        Ok(())
                    }
                    ObjectSynthKind::MissingMemberAnnots { locs } => {
                        for loc in locs.iter() {
                            acc.push(loc.clone());
                        }
                        Ok(())
                    }
                    ObjectSynthKind::Unsynthesizable => {
                        acc.push(ObjectMissingAnnot::OtherMissingAnnot(elem_loc));
                        Ok(())
                    }
                }
            }
            ExpressionInner::Array { inner, .. } => {
                use ast::expression::ArrayElement;
                let acc_savepoint = acc.len();
                let original_this_write_locs = this_write_locs.dupe();

                for elem in inner.elements.iter() {
                    let recursion_result = match elem {
                        ArrayElement::Expression(exp) => {
                            synthesizable_expression(assertion_calls, acc, this_write_locs, exp)
                        }
                        ArrayElement::Spread(spread) => synthesizable_expression(
                            assertion_calls,
                            acc,
                            this_write_locs,
                            &spread.argument,
                        ),
                        ArrayElement::Hole(_) => Err(()),
                    };
                    if recursion_result.is_err() {
                        acc.truncate(acc_savepoint);
                        *this_write_locs = original_this_write_locs;
                        acc.push(ObjectMissingAnnot::OtherMissingAnnot(elem_loc));
                        return Ok(());
                    }
                }
                Ok(())
            }
            _ => {
                acc.push(ObjectMissingAnnot::OtherMissingAnnot(elem_loc));
                Ok(())
            }
        }
    }

    // Main loop over properties
    use ast::expression::object::NormalProperty;

    let mut acc: Vec<ObjectMissingAnnot> = Vec::new();
    let mut current_this_write_locs = this_write_locs;

    for prop in obj.properties.iter() {
        match prop {
            Property::SpreadProperty(spread_prop) => {
                match spread_prop.argument.deref() {
                    ExpressionInner::Identifier { .. } => {
                        // Ok - continue
                    }
                    _ => {
                        return ObjectSynthKind::Unsynthesizable;
                    }
                }
            }
            Property::NormalProperty(normal_prop) => match normal_prop {
                NormalProperty::Init { key, value, .. } => {
                    if let object::Key::Identifier(id) = key {
                        if id.name.as_str() == "__proto__" {
                            return ObjectSynthKind::Unsynthesizable;
                        }
                    }
                    if let object::Key::Identifier(_) = key {
                        if synthesizable_expression(
                            assertion_calls,
                            &mut acc,
                            &mut current_this_write_locs,
                            value,
                        )
                        .is_err()
                        {
                            return ObjectSynthKind::Unsynthesizable;
                        }
                    } else {
                        return ObjectSynthKind::Unsynthesizable;
                    }
                }
                NormalProperty::Method {
                    key: object::Key::Identifier(_),
                    value,
                    ..
                } => {
                    let (_, fn_inner) = value;
                    if handle_fun(
                        &mut acc,
                        func_is_synthesizable_from_annotation(assertion_calls, fn_inner),
                    )
                    .is_err()
                    {
                        return ObjectSynthKind::Unsynthesizable;
                    }
                }
                _ => {
                    return ObjectSynthKind::Unsynthesizable;
                }
            },
        }
    }

    match vec1::Vec1::try_from_vec(acc) {
        Ok(locs) => ObjectSynthKind::MissingMemberAnnots { locs },
        Err(_) => ObjectSynthKind::ObjectSynthesizable {
            this_write_locs: current_this_write_locs,
        },
    }
}

struct ReturnedExpressionCollector {
    acc: Vec<ast::expression::Expression<ALoc, ALoc>>,
}

impl ReturnedExpressionCollector {
    fn new() -> Self {
        Self { acc: Vec::new() }
    }
}

impl ast_visitor::AstVisitor<'_, ALoc> for ReturnedExpressionCollector {
    fn normalize_loc(loc: &ALoc) -> &ALoc {
        loc
    }

    fn normalize_type(type_: &ALoc) -> &ALoc {
        type_
    }

    fn return_(&mut self, _loc: &ALoc, ret: &ast::statement::Return<ALoc, ALoc>) -> Result<(), !> {
        if let Some(arg) = &ret.argument {
            self.acc.push(arg.clone());
        }
        Ok(())
    }
}

fn function_params_all_annotated(
    allow_unannotated_this: bool,
    params: &ast::function::Params<ALoc, ALoc>,
    body: &ast::function::Body<ALoc, ALoc>,
) -> bool {
    params.params.iter().all(|param| match param {
        ast::function::Param::RegularParam { argument, .. } => {
            destructure::type_of_pattern(argument).is_some()
        }
        ast::function::Param::ParamProperty { property, .. } => match &property.annot {
            ast::types::AnnotationOrHint::Available(_) => true,
            ast::types::AnnotationOrHint::Missing(_) => false,
        },
    }) && params
        .rest
        .as_ref()
        .is_none_or(|rest| destructure::type_of_pattern(&rest.argument).is_some())
        && !flow_parser_utils::signature_utils::this_finder::missing_this_annotation(
            !allow_unannotated_this,
            body,
            params,
        )
}

fn identifier_has_autocomplete(
    autocomplete_hooks: &AutocompleteHooks<'_, ALoc>,
    id: &ast::Identifier<ALoc, ALoc>,
) -> bool {
    (autocomplete_hooks.id_hook)(&id.name, &id.loc)
}

fn literal_has_autocomplete(autocomplete_hooks: &AutocompleteHooks<'_, ALoc>, loc: &ALoc) -> bool {
    (autocomplete_hooks.literal_hook)(loc)
}

fn expression_has_autocomplete(
    autocomplete_hooks: &AutocompleteHooks<'_, ALoc>,
    expr: &ast::expression::Expression<ALoc, ALoc>,
) -> bool {
    use ast::expression::ExpressionInner;
    match expr.deref() {
        ExpressionInner::Identifier { inner, .. } => {
            identifier_has_autocomplete(autocomplete_hooks, inner)
        }
        ExpressionInner::StringLiteral { loc, .. } => {
            literal_has_autocomplete(autocomplete_hooks, loc)
        }
        _ => false,
    }
}

pub fn expression_is_definitely_synthesizable(
    autocomplete_hooks: &AutocompleteHooks<'_, ALoc>,
    expr: &ast::expression::Expression<ALoc, ALoc>,
) -> bool {
    use ast::expression::ExpressionInner;

    fn func_is_synthesizable(
        autocomplete_hooks: &AutocompleteHooks<'_, ALoc>,
        allow_unannotated_this: bool,
        fn_: &ast::function::Function<ALoc, ALoc>,
    ) -> bool {
        let params = &fn_.params;
        let body = &fn_.body;
        let return_ = &fn_.return_;

        if function_params_all_annotated(allow_unannotated_this, params, body) {
            use ast::function::ReturnAnnot;
            match (return_, body) {
                (ReturnAnnot::Available(_), _) | (ReturnAnnot::TypeGuard(_), _) => true,
                (ReturnAnnot::Missing(_), ast::function::Body::BodyExpression(expr)) => {
                    synthesizable(autocomplete_hooks, expr)
                }
                (ReturnAnnot::Missing(_), ast::function::Body::BodyBlock((block_loc, block))) => {
                    use flow_parser::ast_visitor::AstVisitor;
                    let mut collector = ReturnedExpressionCollector::new();
                    let Ok(()) = collector.block(block_loc, block);
                    collector
                        .acc
                        .iter()
                        .all(|e| synthesizable(autocomplete_hooks, e))
                }
            }
        } else {
            false
        }
    }

    fn synthesizable(
        autocomplete_hooks: &AutocompleteHooks<'_, ALoc>,
        expr: &ast::expression::Expression<ALoc, ALoc>,
    ) -> bool {
        let _loc = expr.loc().dupe();
        match expr.deref() {
            ExpressionInner::ArrowFunction { inner, .. } => {
                func_is_synthesizable(autocomplete_hooks, true, inner)
            }
            ExpressionInner::Function { inner, .. } => {
                func_is_synthesizable(autocomplete_hooks, false, inner)
            }
            ExpressionInner::Array { inner, .. } => {
                use ast::expression::ArrayElement;
                let elements = &inner.elements;
                if elements.is_empty() {
                    return false;
                }
                elements.iter().all(|elem| match elem {
                    ArrayElement::Expression(e) => synthesizable(autocomplete_hooks, e),
                    ArrayElement::Spread(s) => synthesizable(autocomplete_hooks, &s.argument),
                    ArrayElement::Hole(_) => true,
                })
            }
            ExpressionInner::Object { inner, .. } => {
                use ast::expression::object::NormalProperty;
                use ast::expression::object::Property;
                inner.properties.iter().all(|prop| match prop {
                    Property::NormalProperty(normal_prop) => match normal_prop {
                        NormalProperty::Init { key, value, .. } => {
                            use ast::expression::object::Key;
                            match key {
                                Key::Identifier(id) => {
                                    // Autocompletion in LTI will use hints to find the expected type of the object
                                    // we are completing. There are similar case for identifiers and literals below.
                                    if (autocomplete_hooks.obj_prop_decl_hook)(&id.name, &id.loc) {
                                        return false;
                                    }
                                    synthesizable(autocomplete_hooks, value)
                                }
                                Key::StringLiteral((loc, lit)) => {
                                    if (autocomplete_hooks.obj_prop_decl_hook)(&lit.value, loc) {
                                        return false;
                                    }
                                    synthesizable(autocomplete_hooks, value)
                                }
                                Key::NumberLiteral((loc, num)) => {
                                    if let Some(name) = js_number::numeric_property_name(num.value)
                                    {
                                        if (autocomplete_hooks.obj_prop_decl_hook)(&name, loc) {
                                            return false;
                                        }
                                        synthesizable(autocomplete_hooks, value)
                                    } else {
                                        synthesizable(autocomplete_hooks, value)
                                    }
                                }
                                Key::Computed(_) => false,
                                Key::BigIntLiteral(_) => synthesizable(autocomplete_hooks, value),
                                Key::PrivateName(_) => synthesizable(autocomplete_hooks, value),
                            }
                        }
                        NormalProperty::Method { value, .. } => {
                            let (_, fn_) = value;
                            func_is_synthesizable(autocomplete_hooks, true, fn_)
                        }
                        NormalProperty::Get { value, .. } => {
                            let (_, fn_) = value;
                            func_is_synthesizable(autocomplete_hooks, true, fn_)
                        }
                        NormalProperty::Set { value, .. } => {
                            let (_, fn_) = value;
                            func_is_synthesizable(autocomplete_hooks, true, fn_)
                        }
                    },
                    Property::SpreadProperty(spread_prop) => {
                        synthesizable(autocomplete_hooks, &spread_prop.argument)
                    }
                })
            }
            ExpressionInner::Record { inner, .. } => match &inner.targs {
                Some(call_type_args) => {
                    use ast::expression::CallTypeArg;
                    call_type_args
                        .arguments
                        .iter()
                        .all(|arg| matches!(arg, CallTypeArg::Explicit(_)))
                }
                None => false,
            },
            ExpressionInner::Logical { inner, .. } => {
                synthesizable(autocomplete_hooks, &inner.left)
                    && synthesizable(autocomplete_hooks, &inner.right)
            }
            ExpressionInner::Conditional { inner, .. } => {
                synthesizable(autocomplete_hooks, &inner.consequent)
                    && synthesizable(autocomplete_hooks, &inner.alternate)
            }
            ExpressionInner::Unary { inner, .. } => {
                use ast::expression::UnaryOperator;
                match inner.operator {
                    UnaryOperator::Await => synthesizable(autocomplete_hooks, &inner.argument),
                    _ => true,
                }
            }
            ExpressionInner::Call { inner, .. } => match &inner.targs {
                Some(call_type_args) => {
                    use ast::expression::CallTypeArg;
                    call_type_args
                        .arguments
                        .iter()
                        .all(|arg| matches!(arg, CallTypeArg::Explicit(_)))
                }
                None => false,
            },
            ExpressionInner::OptionalCall { inner, .. } => match &inner.call.targs {
                Some(call_type_args) => {
                    use ast::expression::CallTypeArg;
                    call_type_args
                        .arguments
                        .iter()
                        .all(|arg| matches!(arg, CallTypeArg::Explicit(_)))
                }
                None => false,
            },
            ExpressionInner::New { inner, .. } => match &inner.targs {
                Some(call_type_args) => {
                    use ast::expression::CallTypeArg;
                    call_type_args
                        .arguments
                        .iter()
                        .all(|arg| matches!(arg, CallTypeArg::Explicit(_)))
                }
                None => false,
            },
            // Implicit instantiation might happen in these nodes, and we might have underconstrained targs.
            ExpressionInner::JSXElement { .. } | ExpressionInner::JSXFragment { .. } => false,
            ExpressionInner::Match { inner, .. } => inner
                .cases
                .iter()
                .all(|case| synthesizable(autocomplete_hooks, &case.body)),
            // TaggedTemplates are function calls! They are not automatically synthesizable
            ExpressionInner::TaggedTemplate { .. } => false,
            ExpressionInner::Identifier { inner, .. } => {
                !identifier_has_autocomplete(autocomplete_hooks, inner)
            }
            ExpressionInner::StringLiteral { loc, .. } => {
                !literal_has_autocomplete(autocomplete_hooks, loc)
            }
            ExpressionInner::AsConstExpression { inner, .. } => {
                synthesizable(autocomplete_hooks, &inner.expression)
            }
            // All these are synthesizable
            ExpressionInner::NumberLiteral { .. }
            | ExpressionInner::BooleanLiteral { .. }
            | ExpressionInner::NullLiteral { .. }
            | ExpressionInner::RegExpLiteral { .. }
            | ExpressionInner::BigIntLiteral { .. }
            | ExpressionInner::ModuleRefLiteral { .. }
            | ExpressionInner::Assignment { .. }
            | ExpressionInner::Binary { .. }
            | ExpressionInner::Class { .. }
            | ExpressionInner::Import { .. }
            | ExpressionInner::MetaProperty { .. }
            | ExpressionInner::Member { .. }
            | ExpressionInner::OptionalMember { .. }
            | ExpressionInner::Sequence { .. }
            | ExpressionInner::Super { .. }
            | ExpressionInner::TemplateLiteral { .. }
            | ExpressionInner::This { .. }
            | ExpressionInner::TypeCast { .. }
            | ExpressionInner::AsExpression { .. }
            | ExpressionInner::TSSatisfies { .. }
            | ExpressionInner::Update { .. }
            | ExpressionInner::Yield { .. } => true,
        }
    }

    synthesizable(autocomplete_hooks, expr)
}

fn def_of_function(
    assertion_calls: &BTreeMap<ALoc, AssertionInfo>,
    tparams_map: TparamsMap,
    hints: AstHints,
    has_this_def: bool,
    function_loc: ALoc,
    statics: BTreeMap<FlowSmolStr, EnvKey<ALoc>>,
    namespace_types: BTreeMap<FlowSmolStr, EnvKey<ALoc>>,
    arrow: bool,
    function_: ast::function::Function<ALoc, ALoc>,
) -> Def {
    Def::Function(Box::new(FunctionDefData {
        hints,
        synthesizable_from_annotation: func_is_synthesizable_from_annotation(
            assertion_calls,
            &function_,
        ),
        arrow,
        has_this_def,
        function_loc,
        function_,
        tparams_map,
        statics,
        namespace_types,
    }))
}

fn def_of_declared_function(
    declarations: Vec<(ALoc, ast::statement::DeclareFunction<ALoc, ALoc>)>,
    statics: BTreeMap<FlowSmolStr, EnvKey<ALoc>>,
    namespace_types: BTreeMap<FlowSmolStr, EnvKey<ALoc>>,
) -> Def {
    Def::DeclaredFunction(Box::new(DeclaredFunctionDefData {
        declarations,
        statics,
        namespace_types,
    }))
}

fn def_of_component(
    tparams_map: TparamsMap,
    component_loc: ALoc,
    component: ast::statement::ComponentDeclaration<ALoc, ALoc>,
) -> Def {
    Def::Component(Box::new(ComponentDefData {
        tparams_map,
        component_loc,
        component,
    }))
}

fn func_scope_kind(
    key: Option<&ast::expression::object::Key<ALoc, ALoc>>,
    function: &ast::function::Function<ALoc, ALoc>,
) -> ScopeKind {
    use ast::expression::object::Key;

    let is_constructor = if let Some(Key::Identifier(id)) = key {
        id.name.as_str() == "constructor"
    } else {
        false
    };

    match (function.async_, function.generator, is_constructor) {
        (false, false, true) => ScopeKind::Ctor,
        (true, true, _) => ScopeKind::AsyncGenerator,
        (true, false, _) => ScopeKind::Async,
        (false, true, _) => ScopeKind::Generator,
        (false, false, false) => match function.effect_ {
            ast::function::Effect::Hook => ScopeKind::ComponentOrHookBody,
            ast::function::Effect::Arbitrary => ScopeKind::Ordinary,
        },
    }
}

// Existing own properties on `Function` as defined in `lib/core.js`. We don't
// want to shadow these when creating function statics.
static FUNC_OWN_PROPS: std::sync::LazyLock<BTreeSet<&'static str>> =
    std::sync::LazyLock::new(|| {
        let mut set = BTreeSet::new();
        set.insert("toString");
        set.insert("arguments");
        set.insert("caller");
        set.insert("length");
        set.insert("name");
        set
    });

// Type alias for function statics map
type StaticsMap = BTreeMap<FlowSmolStr, EnvKey<ALoc>>;
type NamespaceTypesMap = BTreeMap<FlowSmolStr, EnvKey<ALoc>>;

fn add_namespace_member(
    map: &mut BTreeMap<FlowSmolStr, EnvKey<ALoc>>,
    name: &FlowSmolStr,
    loc: &ALoc,
) {
    map.entry(name.dupe())
        .or_insert_with(|| EnvKey::new(DefLocType::OrdinaryNameLoc, loc.dupe()));
}

fn collect_declared_namespace_members(
    statements: &[ast::statement::Statement<ALoc, ALoc>],
    env_info: &EnvInfo<ALoc>,
) -> (StaticsMap, NamespaceTypesMap) {
    use ast::pattern::Pattern;
    use ast::statement::StatementInner;

    let mut values = BTreeMap::new();
    let mut types = BTreeMap::new();

    fn collect_one(
        stmt: &ast::statement::StatementInner<ALoc, ALoc>,
        values: &mut StaticsMap,
        types: &mut NamespaceTypesMap,
        env_info: &EnvInfo<ALoc>,
    ) {
        match stmt {
            StatementInner::DeclareNamespace { inner, .. } => {
                if let ast::statement::declare_namespace::Id::Local(id) = &inner.id {
                    add_namespace_member(values, &id.name, &id.loc);
                }
            }
            StatementInner::DeclareTypeAlias { inner, .. }
            | StatementInner::TypeAlias { inner, .. } => {
                add_namespace_member(types, &inner.id.name, &inner.id.loc);
            }
            StatementInner::DeclareOpaqueType { inner, .. }
            | StatementInner::OpaqueType { inner, .. } => {
                add_namespace_member(types, &inner.id.name, &inner.id.loc);
            }
            StatementInner::DeclareInterface { inner, .. }
            | StatementInner::InterfaceDeclaration { inner, .. } => {
                add_namespace_member(types, &inner.id.name, &inner.id.loc);
            }
            StatementInner::DeclareVariable { inner, .. } => {
                for decl in inner.declarations.iter() {
                    if let Pattern::Identifier { inner: pat_id, .. } = &decl.id {
                        add_namespace_member(values, &pat_id.name.name, &pat_id.name.loc);
                    }
                }
            }
            StatementInner::DeclareFunction { inner, .. } => {
                if let Some(id) = &inner.id {
                    add_namespace_member(values, &id.name, &id.loc);
                }
            }
            StatementInner::DeclareClass { inner, .. } => {
                // Classes introduce both value and type bindings. Adding to types
                // prevents a declaration-merged interface from supplying its location,
                // which may lack an env entry. insert overwrites any earlier
                // interface entry so the class location always wins.
                add_namespace_member(values, &inner.id.name, &inner.id.loc);
                types.insert(
                    inner.id.name.dupe(),
                    EnvKey::new(DefLocType::OrdinaryNameLoc, inner.id.loc.dupe()),
                );
            }
            StatementInner::ClassDeclaration { inner, .. } => {
                if let Some(id) = &inner.id {
                    // Classes introduce both value and type bindings.
                    add_namespace_member(values, &id.name, &id.loc);
                    types.insert(
                        id.name.dupe(),
                        EnvKey::new(DefLocType::OrdinaryNameLoc, id.loc.dupe()),
                    );
                }
            }
            StatementInner::DeclareComponent { inner, .. } => {
                add_namespace_member(values, &inner.id.name, &inner.id.loc);
            }
            StatementInner::DeclareEnum { inner, .. }
            | StatementInner::EnumDeclaration { inner, .. } => {
                add_namespace_member(values, &inner.id.name, &inner.id.loc);
            }
            // `export interface I {}`, `export type T = ...`, `export declare const x: ...`
            // inside a `declare namespace` body should be treated the same as the
            // unwrapped form for merging purposes. Common in .d.ts files.
            StatementInner::ExportNamedDeclaration { inner, .. } => {
                if let Some(decl) = &inner.declaration {
                    collect_one(decl.deref(), values, types, env_info);
                }
                if inner.source.is_none()
                    && let Some(
                        ast::statement::export_named_declaration::Specifier::ExportSpecifiers(
                            specifiers,
                        ),
                    ) = &inner.specifiers
                {
                    for specifier in specifiers {
                        let Some(read) = env_info.env_values.get_ordinary(&specifier.local.loc)
                        else {
                            continue;
                        };
                        let Some(def_loc) = &read.def_loc else {
                            continue;
                        };
                        let key = EnvKey::ordinary(def_loc.dupe());
                        if !has_assigning_write(key.dupe(), &env_info.env_entries) {
                            continue;
                        }
                        let kind = flow_parser::ast_utils::effective_export_kind(
                            inner.export_kind,
                            specifier.export_kind,
                        );
                        let type_only = kind == ast::statement::ExportKind::ExportType
                            || matches!(read.val_kind, ValKind::Type { .. });
                        let exported = specifier.exported.as_ref().unwrap_or(&specifier.local);
                        let members = if type_only { &mut *types } else { &mut *values };
                        members.entry(exported.name.dupe()).or_insert(key);
                    }
                }
            }
            StatementInner::DeclareExportDeclaration { inner, .. } => {
                use ast::statement::declare_export_declaration::Declaration;
                match &inner.declaration {
                    Some(Declaration::Variable { declaration, .. }) => {
                        for decl in declaration.declarations.iter() {
                            if let Pattern::Identifier { inner: pat_id, .. } = &decl.id {
                                add_namespace_member(values, &pat_id.name.name, &pat_id.name.loc);
                            }
                        }
                    }
                    Some(Declaration::Function { declaration, .. }) => {
                        if let Some(id) = &declaration.id {
                            add_namespace_member(values, &id.name, &id.loc);
                        }
                    }
                    Some(Declaration::Class { declaration, .. }) => {
                        add_namespace_member(values, &declaration.id.name, &declaration.id.loc);
                        types.insert(
                            declaration.id.name.dupe(),
                            EnvKey::ordinary(declaration.id.loc.dupe()),
                        );
                    }
                    Some(Declaration::NamedType { declaration, .. }) => {
                        add_namespace_member(types, &declaration.id.name, &declaration.id.loc)
                    }
                    Some(Declaration::NamedOpaqueType { declaration, .. }) => {
                        add_namespace_member(types, &declaration.id.name, &declaration.id.loc)
                    }
                    Some(Declaration::Interface { declaration, .. }) => {
                        add_namespace_member(types, &declaration.id.name, &declaration.id.loc)
                    }
                    Some(Declaration::Component { declaration, .. }) => {
                        add_namespace_member(values, &declaration.id.name, &declaration.id.loc)
                    }
                    Some(Declaration::Enum { declaration, .. }) => {
                        add_namespace_member(values, &declaration.id.name, &declaration.id.loc)
                    }
                    Some(Declaration::Namespace { declaration, .. }) => {
                        if let ast::statement::declare_namespace::Id::Local(id) = &declaration.id {
                            add_namespace_member(values, &id.name, &id.loc);
                        }
                    }
                    Some(Declaration::DefaultType { .. }) | None => {}
                }
            }
            _ => {}
        }
    }

    for stmt in statements {
        collect_one(stmt.deref(), &mut values, &mut types, env_info);
    }
    // A declaration is a member only if it binds its name. A redeclaration that the name
    // resolver rejected, e.g. `namespace Foo` after `interface Foo`, has no assigning write,
    // so there is no env entry to read the member from.
    values.retain(|_, key| has_assigning_write(key.dupe(), &env_info.env_entries));
    types.retain(|_, key| has_assigning_write(key.dupe(), &env_info.env_entries));

    (values, types)
}

fn fail(loc: ALoc, str: &str) -> EnvInvariant<ALoc> {
    EnvInvariant::new(
        Some(loc),
        EnvInvariantFailure::ASTStructureOverride(str.into()),
    )
}

fn definition_reference_kind(def: &Def) -> DefinitionReferenceKind {
    match def {
        Def::Function(data) => DefinitionReferenceKind::Function {
            async_: data.function_.async_,
            generator: data.function_.generator,
        },
        Def::DeclaredFunction(_) => DefinitionReferenceKind::Function {
            async_: false,
            generator: false,
        },
        Def::Component(data) => DefinitionReferenceKind::Component(data.component.id.name.dupe()),
        Def::DeclaredComponent(_, component) => {
            DefinitionReferenceKind::Component(component.id.name.dupe())
        }
        Def::Enum { name, .. } => DefinitionReferenceKind::Enum(name.dupe()),
        Def::Interface(..) => DefinitionReferenceKind::Interface,
        Def::MissingThisAnnot => DefinitionReferenceKind::This,
        Def::Class(data) => data
            .class_
            .id
            .as_ref()
            .map(|id| DefinitionReferenceKind::Name(id.name.dupe()))
            .unwrap_or(DefinitionReferenceKind::Other),
        Def::Record(data) => DefinitionReferenceKind::Name(data.record.id.name.dupe()),
        Def::DeclaredClass(data) => DefinitionReferenceKind::Name(data.decl.id.name.dupe()),
        Def::TypeAlias(_, alias) => DefinitionReferenceKind::Name(alias.id.name.dupe()),
        Def::OpaqueType(_, opaque) => DefinitionReferenceKind::Name(opaque.id.name.dupe()),
        Def::TypeParam(data) => DefinitionReferenceKind::Name(data.tparam.1.name.name.dupe()),
        Def::DeclaredNamespace(_, namespace) => match &namespace.id {
            ast::statement::declare_namespace::Id::Local(id) => {
                DefinitionReferenceKind::Name(id.name.dupe())
            }
            ast::statement::declare_namespace::Id::Global(_) => DefinitionReferenceKind::Other,
        },
        Def::Import(data) => match &data.import {
            Import::Named { local, .. } | Import::Namespace(local) | Import::Default(local) => {
                DefinitionReferenceKind::Name(local.dupe())
            }
        },
        Def::ExpressionDef(data) => match data.expr.deref() {
            ast::expression::ExpressionInner::ArrowFunction { inner, .. }
            | ast::expression::ExpressionInner::Function { inner, .. } => {
                DefinitionReferenceKind::Function {
                    async_: inner.async_,
                    generator: inner.generator,
                }
            }
            _ => DefinitionReferenceKind::Other,
        },
        Def::Binding(_)
        | Def::MatchCasePattern(_)
        | Def::MemberAssign(_)
        | Def::OpAssign(_)
        | Def::Update { .. }
        | Def::GeneratorNext(_) => DefinitionReferenceKind::Other,
    }
}

struct DefFinder<'a> {
    autocomplete_hooks: &'a AutocompleteHooks<'a, ALoc>,
    react_jsx: bool,
    assertion_calls: BTreeMap<ALoc, AssertionInfo>,
    env_info: &'a EnvInfo<ALoc>,

    // Accumulator (the result being built)
    env_map: EnvEntriesMap,
    hint_map: HintMap,
    tparams: TparamsMap,
    scope_kind: ScopeKind,
    class_stack: ClassStack,
    return_hint_stack: Vec<AstHints>,
    predicate_kind: Option<PredicateKind>,
}

impl<'a> DefFinder<'a> {
    fn new(
        autocomplete_hooks: &'a AutocompleteHooks<'a, ALoc>,
        react_jsx: bool,
        assertion_calls: BTreeMap<ALoc, AssertionInfo>,
        env_info: &'a EnvInfo<ALoc>,
        toplevel_scope: ScopeKind,
    ) -> Self {
        Self {
            autocomplete_hooks,
            react_jsx,
            assertion_calls,
            env_info,
            env_map: EnvMap::empty(),
            hint_map: ALocMap::new(),
            tparams: FlowOrdMap::new(),
            scope_kind: toplevel_scope,
            class_stack: FlowVector::new(),
            return_hint_stack: Vec::new(),
            predicate_kind: None,
        }
    }

    fn acc(&self) -> (&EnvEntriesMap, &HintMap) {
        (&self.env_map, &self.hint_map)
    }

    fn into_acc(self) -> (EnvEntriesMap, HintMap) {
        (self.env_map, self.hint_map)
    }

    fn add_tparam(&mut self, loc: ALoc, name: FlowSmolStr) {
        self.tparams.insert(loc, name);
    }

    fn add_type_param(
        &mut self,
        kind: &ast_visitor::TypeParamsContext,
        implied_bound: ImpliedBound,
        tparam: &ast::types::TypeParam<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let name_loc = tparam.name.loc.dupe();
        let name = &tparam.name.name;

        self.force_add_binding(
            EnvKey::ordinary(name_loc.dupe()),
            mk_reason(VirtualReasonDesc::RType(name.dupe()), name_loc.dupe()),
            Def::TypeParam(Box::new(TypeParamData {
                tparams_map: self.tparams.clone(),
                kind: *kind,
                implied_bound,
                tparam: (tparam.loc.dupe(), tparam.clone()),
            })),
        );
        self.add_tparam(name_loc, name.dupe());

        ast_visitor::type_param_default(self, kind, tparam)
    }

    // Compute the sibling environments needed by each parameter.
    fn type_param_info(&self, tparams: &ast::types::TypeParams<ALoc, ALoc>) -> Vec<TparamsMap> {
        let analysis = analyze_type_params(tparams);
        analysis
            .dependencies
            .iter()
            .map(|dependencies| {
                let mut map = self.tparams.clone();
                for dependency in dependencies.keys() {
                    let tparam = &tparams.params[*dependency];
                    map.insert(tparam.name.loc.dupe(), tparam.name.name.dupe());
                }
                map
            })
            .collect()
    }

    fn record_hint(&mut self, loc: ALoc, hint: AstHints) {
        self.hint_map.insert(loc, hint);
    }

    fn has_hint(&self, loc: &ALoc) -> bool {
        self.hint_map.contains_key(loc)
    }

    fn force_add_binding(
        &mut self,
        kind_and_loc: EnvKey<ALoc>,
        reason: VirtualReason<ALoc>,
        src: Def,
    ) {
        let definition_kind = definition_reference_kind(&src);
        self.force_add_binding_with_kind(kind_and_loc, reason, definition_kind, src);
    }

    fn force_add_binding_with_kind(
        &mut self,
        kind_and_loc: EnvKey<ALoc>,
        reason: VirtualReason<ALoc>,
        definition_kind: DefinitionReferenceKind,
        src: Def,
    ) {
        self.env_map.insert(
            kind_and_loc,
            (
                src,
                self.scope_kind,
                self.class_stack.dupe(),
                reason,
                definition_kind,
            ),
        );
    }

    fn add_binding(&mut self, kind_and_loc: EnvKey<ALoc>, reason: VirtualReason<ALoc>, src: Def) {
        if has_assigning_write(kind_and_loc.dupe(), &self.env_info.env_entries) {
            self.force_add_binding(kind_and_loc, reason, src);
        }
    }

    fn add_binding_with_kind(
        &mut self,
        kind_and_loc: EnvKey<ALoc>,
        reason: VirtualReason<ALoc>,
        definition_kind: DefinitionReferenceKind,
        src: Def,
    ) {
        if has_assigning_write(kind_and_loc.dupe(), &self.env_info.env_entries) {
            self.force_add_binding_with_kind(kind_and_loc, reason, definition_kind, src);
        }
    }

    fn add_ordinary_binding(&mut self, loc: ALoc, reason: VirtualReason<ALoc>, src: Def) {
        self.add_binding(EnvKey::new(DefLocType::OrdinaryNameLoc, loc), reason, src);
    }

    fn add_ordinary_binding_with_kind(
        &mut self,
        loc: ALoc,
        reason: VirtualReason<ALoc>,
        definition_kind: DefinitionReferenceKind,
        src: Def,
    ) {
        self.add_binding_with_kind(
            EnvKey::new(DefLocType::OrdinaryNameLoc, loc),
            reason,
            definition_kind,
            src,
        );
    }

    fn add_named_ordinary_binding(
        &mut self,
        loc: ALoc,
        name: FlowSmolStr,
        reason: VirtualReason<ALoc>,
        src: Def,
    ) {
        self.add_ordinary_binding_with_kind(loc, reason, DefinitionReferenceKind::Name(name), src);
    }

    fn add_destructure_binding(&mut self, loc: ALoc, binding: Binding) {
        self.add_binding_with_kind(
            EnvKey::new(DefLocType::PatternLoc, loc.dupe()),
            mk_reason(None, loc),
            DefinitionReferenceKind::Destructuring,
            Def::Binding(Box::new(binding)),
        );
    }

    fn add_destructure_bindings(
        &mut self,
        root: Root,
        pattern: &(ALoc, ast::pattern::Pattern<ALoc, ALoc>),
    ) -> Result<(), EnvInvariant<ALoc>> {
        use std::cell::RefCell;
        enum Action {
            Identifier(ALoc, FlowSmolStr, Binding),
            Intermediate(ALoc, Binding),
            Expression(AstHints, (ALoc, ast::expression::Expression<ALoc, ALoc>)),
        }
        let actions = RefCell::new(Vec::new());

        destructure::pattern(
            &mut |loc, name, binding| {
                actions
                    .borrow_mut()
                    .push(Action::Identifier(loc, name.dupe(), binding));
            },
            &mut |loc, binding| {
                actions
                    .borrow_mut()
                    .push(Action::Intermediate(loc, binding));
            },
            &mut |hints: &AstHints, expr: &ast::expression::Expression<ALoc, ALoc>| {
                actions.borrow_mut().push(Action::Expression(
                    hints.clone(),
                    (expr.loc().dupe(), expr.dupe()),
                ));
            },
            Binding::Root(root),
            &pattern.1,
        );

        for action in actions.into_inner() {
            match action {
                Action::Identifier(loc, name, binding) => {
                    let binding = self.mk_hooklike_if_necessary(
                        flow_parser::ast_utils::hook_name(&name),
                        binding,
                    );
                    self.add_named_ordinary_binding(
                        loc.dupe(),
                        name.dupe(),
                        mk_reason(VirtualReasonDesc::RIdentifier(name), loc),
                        Def::Binding(Box::new(binding)),
                    );
                }
                Action::Intermediate(loc, binding) => {
                    self.add_destructure_binding(loc, binding);
                }
                Action::Expression(hints, expr) => {
                    self.visit_expression(EnclosingContext::NoContext, &hints, &expr.1)?;
                }
            }
        }
        Ok(())
    }

    fn mk_hooklike_if_necessary(&self, hooklike: bool, binding: Binding) -> Binding {
        if hooklike {
            Binding::Hooklike(Box::new(binding))
        } else {
            binding
        }
    }

    fn in_scope<T, F>(&mut self, scope: ScopeKind, f: F) -> T
    where
        F: FnOnce(&mut Self) -> T,
    {
        let scope0 = self.scope_kind;
        self.scope_kind = scope;
        let res = f(self);
        self.scope_kind = scope0;
        res
    }

    fn in_new_tparams_env<T, F>(&mut self, keep: bool, f: F) -> T
    where
        F: FnOnce(&mut Self) -> T,
    {
        let old_tparams = std::mem::take(&mut self.tparams);
        if keep {
            self.tparams = old_tparams.clone();
        }
        let result = f(self);
        self.tparams = old_tparams;
        result
    }

    fn visit_function_param(
        &mut self,
        hints: &AstHints,
        effect_: ast::function::Effect,
        param: &ast::function::Param<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let (loc, argument, default_expression) = match param {
            ast::function::Param::RegularParam {
                loc,
                argument,
                default,
            } => (loc, argument, default),
            ast::function::Param::ParamProperty { .. } => {
                // Parameter properties are not supported
                return Ok(());
            }
        };

        let optional = match argument {
            ast::pattern::Pattern::Identifier { inner, .. } => inner.optional,
            _ => false,
        };

        let param_loc = argument.loc();
        let annot = destructure::type_of_pattern(argument);
        let source = match annot {
            Some(annot_val) => {
                if let Some(default_expr) = default_expression {
                    let hint = Hint::HintT(
                        HintNode::AnnotationHint(self.tparams.clone(), annot_val.clone()),
                        HintKind::ExpectedTypeHint,
                    );
                    self.visit_expression(EnclosingContext::NoContext, &vec![hint], default_expr)?;
                }
                Root::Annotation(Box::new(AnnotationData {
                    tparams_map: self.tparams.clone(),
                    optional,
                    has_default_expression: default_expression.is_some(),
                    react_deep_read_only: if effect_ == ast::function::Effect::Hook {
                        Some(DroAnnot::Hook)
                    } else {
                        None
                    },
                    param_loc: Some(param_loc.dupe()),
                    annot: (annot_val.loc.dupe(), annot_val.annotation.clone()),
                    concrete: None,
                }))
            }
            None => {
                if let Some(default_expr) = default_expression {
                    self.visit_expression(EnclosingContext::NoContext, &vec![], default_expr)?;
                }
                let reason = match argument {
                    ast::pattern::Pattern::Identifier { inner, .. } => {
                        let name = &inner.name.name;
                        mk_reason(
                            VirtualReasonDesc::RParameter(Some(name.dupe())),
                            param_loc.dupe(),
                        )
                    }
                    _ => mk_reason(None, param_loc.dupe()),
                };
                self.record_hint(param_loc.dupe(), hints.clone());
                Root::Contextual(Box::new(ContextualData {
                    reason,
                    hints: hints.clone(),
                    optional,
                    default_expression: default_expression
                        .as_ref()
                        .map(|e| (e.loc().dupe(), e.dupe())),
                }))
            }
        };

        struct CollectedOps {
            identifiers: Vec<(ALoc, FlowSmolStr, Binding)>,
            intermediates: Vec<(ALoc, Binding)>,
            expressions: Vec<(AstHints, (ALoc, ast::expression::Expression<ALoc, ALoc>))>,
        }

        let mut ops = CollectedOps {
            identifiers: Vec::new(),
            intermediates: Vec::new(),
            expressions: Vec::new(),
        };

        let source_clone = source.clone();
        let found = destructure::fold_pattern(
            &mut |id_loc, name, binding| {
                ops.identifiers.push((id_loc, name.dupe(), binding));
                true
            },
            &mut |id_loc, binding| {
                ops.intermediates.push((id_loc, binding));
            },
            &mut |inner_hints, expr| {
                ops.expressions
                    .push((inner_hints.clone(), (expr.loc().dupe(), expr.dupe())));
            },
            |a, b| a || b,
            false,
            Binding::Root(source_clone),
            argument,
        );

        for (id_loc, name, binding) in ops.identifiers {
            self.add_named_ordinary_binding(
                id_loc.dupe(),
                name.dupe(),
                mk_reason(VirtualReasonDesc::RIdentifier(name.dupe()), id_loc),
                Def::Binding(Box::new(binding)),
            );
        }

        for (id_loc, binding) in ops.intermediates {
            self.add_destructure_binding(id_loc, binding);
        }

        for (hints, expr) in ops.expressions {
            self.visit_expression(EnclosingContext::NoContext, &hints, &expr.1)?;
        }

        if !found && annot.is_none() {
            self.add_binding_with_kind(
                EnvKey::new(DefLocType::FunctionParamLoc, loc.dupe()),
                mk_reason(None, loc.dupe()),
                DefinitionReferenceKind::Destructuring,
                Def::Binding(Box::new(Binding::Root(source))),
            );
        }

        ast_visitor::function_param_default(
            self,
            &ast::function::Param::RegularParam {
                loc: loc.dupe(),
                argument: argument.clone(),
                default: None,
            },
        )?;
        Ok(())
    }

    fn visit_function_rest_param(
        &mut self,
        hints: &AstHints,
        effect_: ast::function::Effect,
        rest_param: &ast::function::RestParam<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let argument = &rest_param.argument;
        let param_loc = argument.loc();

        let source = match destructure::type_of_pattern(argument) {
            Some(annot) => Root::Annotation(Box::new(AnnotationData {
                tparams_map: self.tparams.clone(),
                optional: false,
                has_default_expression: false,
                react_deep_read_only: if effect_ == ast::function::Effect::Hook {
                    Some(DroAnnot::Hook)
                } else {
                    None
                },
                param_loc: Some(param_loc.dupe()),
                annot: (annot.loc.dupe(), annot.annotation.clone()),
                concrete: None,
            })),
            None => {
                let reason = match argument {
                    ast::pattern::Pattern::Identifier { .. } => mk_reason(None, param_loc.dupe()),
                    _ => {
                        // TODO: This should be a parse error, but we only produce an internal
                        // error in statement.rs.
                        mk_reason(None, param_loc.dupe())
                    }
                };
                self.record_hint(param_loc.dupe(), hints.clone());
                Root::Contextual(Box::new(ContextualData {
                    reason,
                    hints: hints.clone(),
                    optional: false,
                    default_expression: None,
                }))
            }
        };

        self.add_destructure_bindings(source, &(argument.loc().dupe(), argument.clone()))?;
        ast_visitor::function_rest_param_default(self, rest_param)?;
        Ok(())
    }

    fn visit_component_declaration(
        &mut self,
        loc: ALoc,
        stmt: &ast::statement::ComponentDeclaration<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let id = &stmt.id;
        let sig_loc = &stmt.sig_loc;

        self.in_new_tparams_env(false, |this| {
            this.visit_component(stmt)?;
            let name = &id.name;
            let reason = mk_reason(VirtualReasonDesc::RComponent(name.dupe()), sig_loc.dupe());
            let def = def_of_component(this.tparams.clone(), loc.dupe(), stmt.clone());
            let id_loc = id.loc.dupe();
            this.add_ordinary_binding(id_loc, reason, def);
            Ok(())
        })?;
        Ok(())
    }

    fn visit_component(
        &mut self,
        stmt: &ast::statement::ComponentDeclaration<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.in_scope(ScopeKind::ComponentOrHookBody, |this| {
            let params_list = &stmt.params.params;
            let rest = &stmt.params.rest;
            let body = &stmt.body;
            let renders = &stmt.renders;
            let component_tparams = &stmt.tparams;

            if let Some(tparams) = component_tparams {
                this.type_params(
                    &ast_visitor::TypeParamsContext::ComponentDeclaration,
                    tparams,
                )?;
            }
            for param in params_list.iter() {
                this.visit_component_param(param)?;
            }
            if let Some(rest) = rest {
                this.visit_component_rest_param(rest)?;
            }
            this.component_renders_annotation(renders)?;
            let renders_loc = match renders {
                ast::types::ComponentRendersAnnotation::AvailableRenders(loc, _) => loc.dupe(),
                ast::types::ComponentRendersAnnotation::MissingRenders(loc) => loc.dupe(),
            };
            let renders_hint = match renders {
                ast::types::ComponentRendersAnnotation::AvailableRenders(loc, renders_type) => {
                    let annot = ast::types::Annotation {
                        loc: loc.dupe(),
                        annotation: ast::types::Type::new(ast::types::TypeInner::Renders {
                            loc: loc.dupe(),
                            inner: Arc::new(renders_type.clone()),
                        }),
                    };
                    vec![Hint::HintT(
                        HintNode::AnnotationHint(this.tparams.clone(), annot),
                        HintKind::ExpectedTypeHint,
                    )]
                }
                ast::types::ComponentRendersAnnotation::MissingRenders(_) => {
                    vec![Hint::HintT(
                        HintNode::ReactNodeType,
                        HintKind::ExpectedTypeHint,
                    )]
                }
            };
            this.record_hint(renders_loc, renders_hint.clone());
            this.return_hint_stack.push(renders_hint);
            if let Some((body_loc, block)) = body {
                this.block(body_loc, block)?;
            }
            this.return_hint_stack.pop();
            Ok(())
        })?;
        Ok(())
    }

    fn visit_component_param(
        &mut self,
        param: &ast::statement::component_params::Param<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let loc = &param.loc;
        let local = &param.local;
        let default_expression = &param.default;

        let optional = match local {
            ast::pattern::Pattern::Identifier { inner, .. } => inner.optional,
            _ => false,
        };

        let param_loc = local.loc();
        let annot = destructure::type_of_pattern(local);

        let source = match &annot {
            Some(annot_val) => {
                if let Some(default_expr) = default_expression {
                    let hint = Hint::HintT(
                        HintNode::AnnotationHint(self.tparams.clone(), (*annot_val).clone()),
                        HintKind::ExpectedTypeHint,
                    );
                    self.visit_expression(EnclosingContext::NoContext, &vec![hint], default_expr)?;
                }
                Root::Annotation(Box::new(AnnotationData {
                    tparams_map: self.tparams.clone(),
                    optional,
                    has_default_expression: default_expression.is_some(),
                    react_deep_read_only: Some(DroAnnot::Comp),
                    param_loc: Some(param_loc.dupe()),
                    annot: (annot_val.loc.dupe(), annot_val.annotation.clone()),
                    concrete: None,
                }))
            }
            None => {
                if let Some(default_expr) = default_expression {
                    self.visit_expression(EnclosingContext::NoContext, &vec![], default_expr)?;
                }
                let reason = match local {
                    ast::pattern::Pattern::Identifier { inner, .. } => {
                        let name = &inner.name.name;
                        mk_reason(
                            VirtualReasonDesc::RParameter(Some(name.dupe())),
                            param_loc.dupe(),
                        )
                    }
                    _ => mk_reason(None, param_loc.dupe()),
                };
                Root::UnannotatedParameter(reason)
            }
        };

        struct CollectedOps {
            identifiers: Vec<(ALoc, FlowSmolStr, Binding)>,
            intermediates: Vec<(ALoc, Binding)>,
            expressions: Vec<(AstHints, (ALoc, ast::expression::Expression<ALoc, ALoc>))>,
        }

        let mut ops = CollectedOps {
            identifiers: Vec::new(),
            intermediates: Vec::new(),
            expressions: Vec::new(),
        };

        let source_clone = source.clone();
        let found = destructure::fold_pattern(
            &mut |id_loc, name, binding| {
                ops.identifiers.push((id_loc, name.dupe(), binding));
                true
            },
            &mut |id_loc, binding| {
                ops.intermediates.push((id_loc, binding));
            },
            &mut |inner_hints, expr| {
                ops.expressions
                    .push((inner_hints.clone(), (expr.loc().dupe(), expr.dupe())));
            },
            |a, b| a || b,
            false,
            Binding::Root(source_clone),
            local,
        );

        for (id_loc, name, binding) in ops.identifiers {
            self.add_named_ordinary_binding(
                id_loc.dupe(),
                name.dupe(),
                mk_reason(VirtualReasonDesc::RIdentifier(name.dupe()), id_loc),
                Def::Binding(Box::new(binding)),
            );
        }

        for (id_loc, binding) in ops.intermediates {
            self.add_destructure_binding(id_loc, binding);
        }

        for (hints, expr) in ops.expressions {
            self.visit_expression(EnclosingContext::NoContext, &hints, &expr.1)?;
        }

        if !found && annot.is_none() {
            self.add_binding_with_kind(
                EnvKey::new(DefLocType::FunctionParamLoc, loc.dupe()),
                mk_reason(None, loc.dupe()),
                DefinitionReferenceKind::Destructuring,
                Def::Binding(Box::new(Binding::Root(source))),
            );
        }
        ast_visitor::component_param_default(
            self,
            &ast::statement::component_params::Param {
                loc: loc.dupe(),
                name: param.name.clone(),
                local: local.clone(),
                default: None,
                shorthand: param.shorthand,
            },
        )?;
        Ok(())
    }

    fn visit_component_rest_param(
        &mut self,
        param: &ast::statement::component_params::RestParam<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let loc = &param.loc;
        let argument = &param.argument;

        let optional = match argument {
            ast::pattern::Pattern::Identifier { inner, .. } => inner.optional,
            _ => false,
        };

        let param_loc = argument.loc();
        let annot = destructure::type_of_pattern(argument);

        let source = match &annot {
            Some(annot_val) => Root::Annotation(Box::new(AnnotationData {
                tparams_map: self.tparams.clone(),
                optional,
                has_default_expression: false,
                react_deep_read_only: Some(DroAnnot::Comp),
                param_loc: Some(param_loc.dupe()),
                annot: (annot_val.loc.dupe(), annot_val.annotation.clone()),
                concrete: None,
            })),
            None => {
                let reason = match argument {
                    ast::pattern::Pattern::Identifier { inner, .. } => {
                        let name = &inner.name.name;
                        mk_reason(
                            VirtualReasonDesc::RParameter(Some(name.dupe())),
                            param_loc.dupe(),
                        )
                    }
                    _ => mk_reason(None, param_loc.dupe()),
                };
                Root::UnannotatedParameter(reason)
            }
        };

        struct CollectedOps {
            identifiers: Vec<(ALoc, FlowSmolStr, Binding)>,
            intermediates: Vec<(ALoc, Binding)>,
            expressions: Vec<(AstHints, (ALoc, ast::expression::Expression<ALoc, ALoc>))>,
        }

        let mut ops = CollectedOps {
            identifiers: Vec::new(),
            intermediates: Vec::new(),
            expressions: Vec::new(),
        };

        let source_clone = source.clone();
        let found = destructure::fold_pattern(
            &mut |id_loc, name, binding| {
                ops.identifiers.push((id_loc, name.dupe(), binding));
                true
            },
            &mut |id_loc, binding| {
                ops.intermediates.push((id_loc, binding));
            },
            &mut |inner_hints, expr| {
                ops.expressions
                    .push((inner_hints.clone(), (expr.loc().dupe(), expr.dupe())));
            },
            |a, b| a || b,
            false,
            Binding::Root(source_clone),
            argument,
        );

        for (id_loc, name, binding) in ops.identifiers {
            self.add_named_ordinary_binding(
                id_loc.dupe(),
                name.dupe(),
                mk_reason(VirtualReasonDesc::RIdentifier(name.dupe()), id_loc),
                Def::Binding(Box::new(binding)),
            );
        }

        for (id_loc, binding) in ops.intermediates {
            self.add_destructure_binding(id_loc, binding);
        }

        for (hints, expr) in ops.expressions {
            self.visit_expression(EnclosingContext::NoContext, &hints, &expr.1)?;
        }

        if !found && annot.is_none() {
            self.add_binding_with_kind(
                EnvKey::new(DefLocType::FunctionParamLoc, loc.dupe()),
                mk_reason(None, loc.dupe()),
                DefinitionReferenceKind::Destructuring,
                Def::Binding(Box::new(Binding::Root(source))),
            );
        }
        ast_visitor::component_rest_param_default(self, param)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_function_expr(
        &mut self,
        func_hints: &AstHints,
        func_return_hints: &AstHints,
        has_this_def: bool,
        var_assigned_to: Option<&ast::Identifier<ALoc, ALoc>>,
        statics: &StaticsMap,
        namespace_types: &NamespaceTypesMap,
        arrow: bool,
        hooklike: bool,
        function_loc: ALoc,
        expr: &ast::function::Function<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let id = &expr.id;
        let async_ = expr.async_;
        let generator = expr.generator;
        let sig_loc = &expr.sig_loc;
        let params = &expr.params;
        let body = &expr.body;

        let scope_kind = func_scope_kind(None, expr);
        if flow_parser_utils::signature_utils::this_finder::missing_this_annotation(
            has_this_def,
            body,
            params,
        ) {
            self.add_binding(
                EnvKey::new(DefLocType::FunctionThisLoc, function_loc.dupe()),
                mk_reason(VirtualReasonDesc::RThis, function_loc.dupe()),
                Def::MissingThisAnnot,
            );
        }
        let func_hints_clone = func_hints.to_vec();
        let func_return_hints_clone = func_return_hints.to_vec();
        let statics_clone = statics.clone();
        let namespace_types_clone = namespace_types.clone();
        self.in_new_tparams_env(false, |this| {
            this.visit_function(
                &scope_kind,
                &func_hints_clone,
                &func_return_hints_clone,
                expr,
            )?;

            if let Some(name_ident) = var_assigned_to {
                let name = &name_ident.name;
                let name_loc = name_ident.loc.dupe();

                let binding = Binding::Root(Root::FunctionValue(Box::new(FunctionValueData {
                    hints: func_hints_clone.clone(),
                    synthesizable_from_annotation: func_is_synthesizable_from_annotation(
                        &this.assertion_calls,
                        expr,
                    ),
                    function_loc: function_loc.dupe(),
                    function_: expr.clone(),
                    statics: statics_clone.clone(),
                    arrow,
                    tparams_map: this.tparams.clone(),
                })));
                let binding = this.mk_hooklike_if_necessary(hooklike, binding);
                this.add_named_ordinary_binding(
                    name_loc,
                    name.dupe(),
                    mk_reason(
                        VirtualReasonDesc::RIdentifier(name.dupe()),
                        name_ident.loc.dupe(),
                    ),
                    Def::Binding(Box::new(binding)),
                );
            }

            let add_def = |this_ref: &mut DefFinder<'a>, binding_loc: ALoc| {
                let reason_loc = if arrow {
                    function_loc.dupe()
                } else {
                    sig_loc.dupe()
                };
                let reason = func_reason(async_, generator, reason_loc);
                let def = def_of_function(
                    &this_ref.assertion_calls,
                    this_ref.tparams.clone(),
                    func_hints_clone.clone(),
                    has_this_def,
                    function_loc.dupe(),
                    statics_clone.clone(),
                    namespace_types_clone.clone(),
                    false,
                    expr.clone(),
                );
                this_ref.add_ordinary_binding(binding_loc, reason, def);
            };

            match id {
                Some(id_ident) => add_def(this, id_ident.loc.dupe()),
                None if has_this_def => add_def(this, function_loc.dupe()),
                None => {}
            }
            Ok(())
        })?;
        Ok(())
    }

    fn visit_function_declaration(
        &mut self,
        statics: &StaticsMap,
        namespace_types: &NamespaceTypesMap,
        loc: ALoc,
        expr: &ast::function::Function<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let id = &expr.id;
        let async_ = expr.async_;
        let generator = expr.generator;
        let sig_loc = &expr.sig_loc;
        let params = &expr.params;
        let body = &expr.body;

        let scope_kind = func_scope_kind(None, expr);

        self.in_new_tparams_env(false, |this| {
            if flow_parser_utils::signature_utils::this_finder::missing_this_annotation(
                true, // needs_this_param:true
                body, params,
            ) {
                this.add_binding(
                    EnvKey::new(DefLocType::FunctionThisLoc, loc.dupe()),
                    mk_reason(VirtualReasonDesc::RThis, loc.dupe()),
                    Def::MissingThisAnnot,
                );
            }

            this.visit_function(&scope_kind, &vec![], &vec![], expr)?;
            let reason = func_reason(async_, generator, sig_loc.dupe());
            let def = def_of_function(
                &this.assertion_calls,
                this.tparams.clone(),
                vec![],
                true, // has_this_def:true
                loc.dupe(),
                statics.clone(),
                namespace_types.clone(),
                false, // arrow:false
                expr.clone(),
            );
            match id {
                Some(id_ident) => {
                    this.add_ordinary_binding(id_ident.loc.dupe(), reason, def);
                }
                None => {
                    this.add_ordinary_binding(loc.dupe(), reason, def);
                }
            }
            Ok(())
        })?;
        Ok(())
    }

    fn visit_declared_function(
        &mut self,
        loc: ALoc,
        decl: &ast::statement::DeclareFunction<ALoc, ALoc>,
        declarations: &[(ALoc, ast::statement::DeclareFunction<ALoc, ALoc>)],
        statics: &StaticsMap,
        namespace_types: &NamespaceTypesMap,
    ) -> Result<(), EnvInvariant<ALoc>> {
        if let Some(id) = &decl.id {
            self.add_ordinary_binding(
                id.loc.dupe(),
                func_reason(false, false, loc.dupe()),
                def_of_declared_function(
                    declarations.to_vec(),
                    statics.clone(),
                    namespace_types.clone(),
                ),
            );
        }
        ast_visitor::declare_function_default(self, &loc, decl)?;
        Ok(())
    }

    fn visit_merged_declare_namespace_body(
        &mut self,
        namespace: &ast::statement::DeclareNamespace<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let (body_loc, body_block) = &namespace.body;
        self.in_scope(ScopeKind::DeclareNamespace, |this| {
            ast_visitor::block_default(this, body_loc, body_block)?;
            Ok(())
        })?;
        Ok(())
    }

    fn hint_pred_kind(
        &self,
        params: &ast::function::Params<ALoc, ALoc>,
        body: &ast::function::Body<ALoc, ALoc>,
        return_: &ast::function::ReturnAnnot<ALoc, ALoc>,
    ) -> Option<PredicateKind> {
        let single_param_opt = if params.params.len() == 1 && params.rest.is_none() {
            let param = &params.params[0];
            match param {
                ast::function::Param::RegularParam { argument, .. } => match argument {
                    ast::pattern::Pattern::Identifier { inner, .. } => {
                        Some((inner.name.loc.dupe(), inner.name.name.dupe()))
                    }
                    _ => None,
                },
                ast::function::Param::ParamProperty { .. } => None,
            }
        } else {
            None
        };

        match return_ {
            ast::function::ReturnAnnot::TypeGuard(tg_annot) => {
                let guard_id = &tg_annot.guard.guard.0;
                Some(PredicateKind::TypeGuardKind(
                    guard_id.loc.dupe(),
                    guard_id.name.dupe(),
                ))
            }
            ast::function::ReturnAnnot::Missing(_loc) => {
                if let ast::function::Body::BodyExpression(_) = body {
                    if let Some((param_loc, name)) = single_param_opt {
                        if self
                            .env_info
                            .type_guard_consistency_maps
                            .contains_key(&param_loc)
                        {
                            return Some(PredicateKind::TypeGuardKind(param_loc, name));
                        }
                    }
                }
                None
            }
            ast::function::ReturnAnnot::Available(_) => None,
        }
    }

    fn name_of_param(param: &ast::function::Param<ALoc, ALoc>) -> Option<FlowSmolStr> {
        match param {
            ast::function::Param::RegularParam { argument, .. } => match argument {
                ast::pattern::Pattern::Identifier { inner, .. } => Some(inner.name.name.dupe()),
                _ => None,
            },
            ast::function::Param::ParamProperty { property, .. } => match &property.key {
                ast::expression::object::Key::Identifier(id) => Some(id.name.dupe()),
                _ => None,
            },
        }
    }

    fn params_list_to_str_opt(
        params: &[ast::function::Param<ALoc, ALoc>],
    ) -> Vec<Option<FlowSmolStr>> {
        params.iter().map(Self::name_of_param).collect()
    }

    fn visit_function(
        &mut self,
        scope_kind: &ScopeKind,
        func_hints: &AstHints,
        func_return_hints: &AstHints,
        expr: &ast::function::Function<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.in_scope(scope_kind.clone(), |this| {
            let params = &expr.params;
            let params_list = &params.params;
            let rest = &params.rest;
            let this_ = &params.this_;
            let body = &expr.body;
            let async_ = expr.async_;
            let generator = expr.generator;
            let effect_ = expr.effect_;
            let predicate = &expr.predicate;
            let return_ = &expr.return_;
            let fun_tparams = &expr.tparams;

            if let Some(tparams) = fun_tparams {
                this.type_params(&ast_visitor::TypeParamsContext::Function, tparams)?;
            }

            if let Some(this_param) = this_ {
                this.function_this_param(this_param)?;
            }

            let param_str_list = Self::params_list_to_str_opt(params_list);
            let pred = this.hint_pred_kind(params, body, return_);

            let old_predicate_kind = this.predicate_kind.clone();
            this.predicate_kind = pred.clone();

            for (i, param) in params_list.iter().enumerate() {
                let decomp = HintDecomposition::new(HintDecompositionInner::DecompFuncParam(
                    param_str_list.clone(),
                    i as i32,
                    pred.clone(),
                ));
                let hints = Hint::decompose(decomp, func_hints.clone());
                this.visit_function_param(&hints, effect_, param)?;
            }
            if let Some(rest_param) = rest {
                let decomp = HintDecomposition::new(HintDecompositionInner::DecompFuncRest(
                    param_str_list.clone(),
                    pred.clone(),
                ));
                let hints = Hint::decompose(decomp, func_hints.clone());
                this.visit_function_rest_param(&hints, effect_, rest_param)?;
            }
            this.function_return_annotation(return_)?;
            let return_loc = match return_ {
                ast::function::ReturnAnnot::Available(annot) => annot.loc.dupe(),
                ast::function::ReturnAnnot::TypeGuard(tg) => tg.loc.dupe(),
                ast::function::ReturnAnnot::Missing(loc) => loc.dupe(),
            };
            let return_hint = if generator {
                // Return hints do not apply to generators
                vec![]
            } else {
                match return_ {
                    ast::function::ReturnAnnot::Available(annot) => {
                        vec![Hint::HintT(
                            HintNode::AnnotationHint(this.tparams.clone(), annot.clone()),
                            HintKind::ExpectedTypeHint,
                        )]
                    }
                    ast::function::ReturnAnnot::TypeGuard(_) => vec![],
                    ast::function::ReturnAnnot::Missing(_) => {
                        let decomp =
                            HintDecomposition::new(HintDecompositionInner::DecompFuncReturn);
                        Hint::decompose(decomp, func_return_hints.clone())
                    }
                }
            };

            let return_hint = match scope_kind {
                ScopeKind::Async => {
                    let decomp = HintDecomposition::new(HintDecompositionInner::DecompPromise);
                    Hint::decompose(decomp, return_hint)
                }
                _ => return_hint,
            };

            this.record_hint(return_loc.dupe(), return_hint.clone());
            this.return_hint_stack.push(return_hint.clone());
            let body_loc = match body {
                ast::function::Body::BodyBlock((loc, block)) => {
                    this.block(loc, block)?;
                    loc.dupe()
                }
                ast::function::Body::BodyExpression(expr) => {
                    let loc = expr.loc().dupe();
                    let cond = if pred.is_some() {
                        EnclosingContext::OtherTestContext
                    } else {
                        EnclosingContext::NoContext
                    };
                    this.visit_expression(cond, &return_hint, expr)?;
                    loc
                }
            };

            this.return_hint_stack.pop();
            this.predicate_kind = old_predicate_kind;

            if generator {
                let (loc, gen_val) = match return_ {
                    ast::function::ReturnAnnot::Missing(loc)
                    | ast::function::ReturnAnnot::TypeGuard(ast::types::TypeGuardAnnotation {
                        loc,
                        ..
                    }) => (loc.dupe(), None),
                    ast::function::ReturnAnnot::Available(return_annot) => {
                        let gen_val = GeneratorAnnot {
                            tparams_map: this.tparams.clone(),
                            return_annot: (
                                return_annot.loc.dupe(),
                                return_annot.annotation.clone(),
                            ),
                            async_,
                        };
                        (return_annot.loc.dupe(), Some(gen_val))
                    }
                };
                this.add_ordinary_binding(
                    loc,
                    mk_reason(None, body_loc),
                    Def::GeneratorNext(Box::new(gen_val)),
                );
            }

            if let Some(predicate_val) = predicate {
                if let ast::types::PredicateKind::Declared(expr) = &predicate_val.kind {
                    this.visit_expression(EnclosingContext::NoContext, &vec![], expr)?;
                }
            }
            Ok(())
        })?;
        Ok(())
    }

    fn class_internal(
        &mut self,
        kind: ClassKind,
        namespace_members: &ClassNamespaceMembers,
        loc: ALoc,
        expr: &ast::class::Class<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let id = &expr.id;
        let body = &expr.body;
        let class_tparams = &expr.tparams;
        let extends = &expr.extends;
        let implements = &expr.implements;
        let class_decorators = &expr.class_decorators;

        self.in_new_tparams_env(false, |this| {
            this.class_stack.push(loc.dupe());
            this.in_scope(ScopeKind::Ordinary, |this_inner| {
                if let Some(id_ident) = id {
                    this_inner.class_identifier(id_ident)?;
                }
                if let Some(tparams) = class_tparams {
                    this_inner.type_params(&ast_visitor::TypeParamsContext::Class, tparams)?;
                }

                let this_tparam_loc = id.as_ref().map_or(loc.dupe(), |ident| ident.loc.dupe());
                this_inner.add_tparam(this_tparam_loc, "this".into());
                this_inner.class_body(body)?;
                if let Some(extends_val) = extends {
                    this_inner.class_extends(extends_val)?;
                }
                if let Some(impl_val) = implements {
                    this_inner.class_implements(impl_val)?;
                }
                for decorator in class_decorators.iter() {
                    this_inner.class_decorator(decorator)?;
                }
                Ok(())
            })?;
            this.class_stack.pop();

            let mut this_super_write_locs = EnvSet::empty();
            for member in body.body.iter() {
                match member {
                    ast::class::BodyElement::Property(prop) => {
                        if matches!(prop.annot, ast::types::AnnotationOrHint::Missing { .. }) {
                            if let ast::class::property::Value::Initialized(init_expr) = &prop.value
                            {
                                if matches!(
                                    init_expr.deref(),
                                    ast::expression::ExpressionInner::Function { .. }
                                ) {
                                    this_super_write_locs.insert(EnvKey::new(
                                        DefLocType::FunctionThisLoc,
                                        init_expr.loc().dupe(),
                                    ));
                                }
                            }
                        }
                    }
                    ast::class::BodyElement::PrivateField(pf) => {
                        if matches!(pf.annot, ast::types::AnnotationOrHint::Missing { .. }) {
                            if let ast::class::property::Value::Initialized(init_expr) = &pf.value {
                                if matches!(
                                    init_expr.deref(),
                                    ast::expression::ExpressionInner::Function { .. }
                                ) {
                                    this_super_write_locs.insert(EnvKey::new(
                                        DefLocType::FunctionThisLoc,
                                        init_expr.loc().dupe(),
                                    ));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            this_super_write_locs.insert(EnvKey::new(DefLocType::ClassSelfLoc, loc.dupe()));
            this_super_write_locs.insert(EnvKey::new(DefLocType::ClassInstanceThisLoc, loc.dupe()));
            this_super_write_locs.insert(EnvKey::new(DefLocType::ClassStaticThisLoc, loc.dupe()));
            this_super_write_locs
                .insert(EnvKey::new(DefLocType::ClassInstanceSuperLoc, loc.dupe()));
            this_super_write_locs.insert(EnvKey::new(DefLocType::ClassStaticSuperLoc, loc.dupe()));

            match id {
                Some(id_ident) => {
                    let name = id_ident.name.dupe();
                    let reason = mk_reason(VirtualReasonDesc::RType(name), id_ident.loc.dupe());
                    this.add_ordinary_binding(
                        id_ident.loc.dupe(),
                        reason,
                        Def::Class(Box::new(ClassDefData {
                            class_loc: loc.dupe(),
                            class_: expr.clone(),
                            this_super_write_locs,
                            kind,
                            namespace_members: namespace_members.clone(),
                        })),
                    );
                }
                None => {
                    let reason = mk_reason(
                        VirtualReasonDesc::RType("<<anonymous class>>".into()),
                        loc.dupe(),
                    );
                    this.add_ordinary_binding(
                        loc.dupe(),
                        reason,
                        Def::Class(Box::new(ClassDefData {
                            class_loc: loc.dupe(),
                            class_: expr.clone(),
                            this_super_write_locs,
                            kind,
                            namespace_members: namespace_members.clone(),
                        })),
                    );
                }
            }
            Ok(())
        })?;
        Ok(())
    }

    fn visit_declare_class(
        &mut self,
        namespace_members: &ClassNamespaceMembers,
        loc: ALoc,
        decl: &ast::statement::DeclareClass<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let id_loc = decl.id.loc.dupe();
        let name = &decl.id.name;

        self.add_ordinary_binding(
            id_loc,
            mk_reason(VirtualReasonDesc::RIdentifier(name.dupe()), loc.dupe()),
            Def::DeclaredClass(Box::new(DeclaredClassDefData {
                loc: loc.dupe(),
                decl: decl.clone(),
                namespace_members: namespace_members.clone(),
            })),
        );
        ast_visitor::declare_class_default(self, &loc, decl)?;
        Ok(())
    }

    fn visit_class_property_value(
        &mut self,
        hints: &AstHints,
        value: &ast::class::property::Value<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        match value {
            ast::class::property::Value::Declared => {}
            ast::class::property::Value::Uninitialized => {}
            ast::class::property::Value::Initialized(x) => {
                self.visit_expression(EnclosingContext::NoContext, hints, x)?;
            }
        }
        Ok(())
    }

    fn record_declaration_internal(
        &mut self,
        loc: ALoc,
        record: &ast::statement::RecordDeclaration<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let id = &record.id;
        let id_loc = id.loc.dupe();
        let record_name = &id.name;
        let tparams = &record.tparams;
        let implements = &record.implements;
        let body = &record.body;

        let defaulted_props = flow_parser_utils::record_utils::defaulted_props_of_record(record);

        self.in_new_tparams_env(false, |this| {
            this.class_stack.push(loc.dupe());
            this.in_scope(ScopeKind::Ordinary, |this_inner| {
                this_inner.class_identifier(id)?;
                if let Some(tparams_val) = tparams {
                    this_inner.type_params(&ast_visitor::TypeParamsContext::Class, tparams_val)?;
                }
                this_inner.add_tparam(id_loc.dupe(), "this".into());
                this_inner.record_declaration_internal_body(body)?;
                if let Some(impl_val) = implements {
                    this_inner.class_implements(impl_val)?;
                }
                Ok(())
            })?;
            this.class_stack.pop();

            let mut this_super_write_locs = EnvSet::empty();
            for element in body.body.iter() {
                match element {
                    ast::statement::record_declaration::BodyElement::Property(prop) => {
                        if let Some(default_value) = &prop.default_value {
                            if matches!(
                                default_value.deref(),
                                ast::expression::ExpressionInner::Function { .. }
                            ) {
                                this_super_write_locs.insert(EnvKey::new(
                                    DefLocType::FunctionThisLoc,
                                    default_value.loc().dupe(),
                                ));
                            }
                        }
                    }
                    ast::statement::record_declaration::BodyElement::StaticProperty(sp) => {
                        if matches!(
                            sp.value.deref(),
                            ast::expression::ExpressionInner::Function { .. }
                        ) {
                            this_super_write_locs.insert(EnvKey::new(
                                DefLocType::FunctionThisLoc,
                                sp.value.loc().dupe(),
                            ));
                        }
                    }
                    _ => {}
                }
            }

            this_super_write_locs.insert(EnvKey::new(DefLocType::ClassSelfLoc, loc.dupe()));
            this_super_write_locs.insert(EnvKey::new(DefLocType::ClassInstanceThisLoc, loc.dupe()));
            this_super_write_locs.insert(EnvKey::new(DefLocType::ClassStaticThisLoc, loc.dupe()));
            this_super_write_locs
                .insert(EnvKey::new(DefLocType::ClassInstanceSuperLoc, loc.dupe()));
            this_super_write_locs.insert(EnvKey::new(DefLocType::ClassStaticSuperLoc, loc.dupe()));

            let reason = mk_reason(VirtualReasonDesc::RType(record_name.dupe()), id_loc.dupe());
            this.add_ordinary_binding(
                id_loc.dupe(),
                reason,
                Def::Record(Box::new(RecordDefData {
                    record_loc: loc.dupe(),
                    record: record.clone(),
                    this_super_write_locs,
                    defaulted_props,
                })),
            );
            Ok(())
        })?;
        Ok(())
    }

    fn record_declaration_internal_body(
        &mut self,
        body: &ast::statement::record_declaration::Body<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        for element in body.body.iter() {
            match element {
                ast::statement::record_declaration::BodyElement::Method(method_) => {
                    self.class_method(method_)?;
                }
                ast::statement::record_declaration::BodyElement::Property(prop) => {
                    self.type_annotation(&prop.annot)?;
                    if let Some(default_value) = &prop.default_value {
                        self.visit_expression(EnclosingContext::NoContext, &vec![], default_value)?;
                    }
                }
                ast::statement::record_declaration::BodyElement::StaticProperty(static_prop) => {
                    self.type_annotation(&static_prop.annot)?;
                    self.visit_expression(
                        EnclosingContext::NoContext,
                        &vec![],
                        &static_prop.value,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn visit_assignment_expression(
        &mut self,
        is_function_statics_assignment: bool,
        loc: ALoc,
        expr: &ast::expression::Assignment<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        use ast::expression::AssignmentOperator;
        use ast::expression::member::Property;
        use ast::pattern::Pattern;
        use flow_common::reason::mk_expression_reason;
        use flow_common::reason::mk_pattern_reason;
        use flow_parser::ast_utils::is_module_dot_exports;
        use flow_parser::ast_utils::unwrap_nonnull_lhs;

        let operator = &expr.operator;
        let left_pat = &expr.left;
        let right = &expr.right;

        let (left_cow, assertion) = unwrap_nonnull_lhs(left_pat);
        let lhs_loc = left_cow.loc().dupe();
        // Create the (ALoc, Pattern) tuple for APIs that need it
        let left_tuple = (lhs_loc.dupe(), left_cow.clone().into_owned());
        let lhs_node = &*left_cow;

        // Helper to compute expression_pattern_hints
        let expression_pattern_hints = |e: &ast::expression::Expression<ALoc, ALoc>,
                                        class_stack: &ClassStack|
         -> AstHints {
            match e.deref() {
                ast::expression::ExpressionInner::Member {
                    loc: _,
                    inner: member,
                } if !is_function_statics_assignment && !is_module_dot_exports(e) => {
                    let _object = &member.object;
                    let base_hints: AstHints = vec![Hint::HintT(
                        HintNode::ValueHint(EnclosingContext::NoContext, _object.clone()),
                        HintKind::ExpectedTypeHint,
                    )];

                    match &member.property {
                        Property::PropertyIdentifier(id) => {
                            let name = &id.name;
                            let decomp = HintDecomposition::new(
                                HintDecompositionInner::DecompObjProp(name.dupe()),
                            );
                            Hint::decompose(decomp, base_hints)
                        }
                        Property::PropertyPrivateName(pn) => {
                            let name = &pn.name;
                            let decomp =
                                HintDecomposition::new(HintDecompositionInner::DecompPrivateProp(
                                    name.dupe(),
                                    class_stack.clone(),
                                ));
                            Hint::decompose(decomp, base_hints)
                        }
                        Property::PropertyExpression(expr) => {
                            let reason = mk_expression_reason(expr);
                            let decomp = HintDecomposition::new(
                                HintDecompositionInner::DecompObjComputed(reason),
                            );
                            Hint::decompose(decomp, base_hints)
                        }
                    }
                }
                ast::expression::ExpressionInner::Member { .. } => vec![],
                _ => {
                    vec![Hint::HintT(
                        HintNode::AnyErrorHint(mk_reason(None, lhs_loc.dupe())),
                        HintKind::ExpectedTypeHint,
                    )]
                }
            }
        };

        fn other_pattern_hint_opt<P: ProviderApi>(
            providers: &P,
            pat: &ast::pattern::Pattern<ALoc, ALoc>,
        ) -> Option<HintNode> {
            use ast::pattern::Pattern;
            match pat {
                Pattern::Identifier { inner, .. } => {
                    let id_loc = &inner.name.loc;
                    if !providers.is_provider(id_loc) {
                        let provider_locs: Vec<ALoc> = providers
                            .providers_of_def(id_loc)
                            .map(|def_providers| &def_providers.providers)
                            .unwrap_or(&Vec::new())
                            .iter()
                            .map(|p| p.reason.loc().dupe())
                            .collect();
                        vec1::Vec1::try_from_vec(provider_locs)
                            .ok()
                            .map(HintNode::ProvidersHint)
                    } else {
                        None
                    }
                }
                // Unsupported syntax like [foo.bar] = expr
                Pattern::Expression { loc, .. } => {
                    Some(HintNode::AnyErrorHint(mk_reason(None, loc.dupe())))
                }
                Pattern::Array { loc, inner } => {
                    let elements = &inner.elements;
                    let mut acc = Vec::new();
                    for elem in elements.iter() {
                        match elem {
                            ast::pattern::array::Element::NormalElement(e) => {
                                let h = other_pattern_hint_opt(providers, &e.argument)?;
                                acc.push(ArrayElementPatternHint::ArrayElementPatternHint(h))
                            }
                            ast::pattern::array::Element::RestElement(r) => {
                                let h = other_pattern_hint_opt(providers, &r.argument)?;
                                acc.push(ArrayElementPatternHint::ArrayRestElementPatternHint(h))
                            }
                            ast::pattern::array::Element::Hole(_) => return None,
                        }
                    }
                    Some(HintNode::ComposedArrayPatternHint(loc.dupe(), acc))
                }
                Pattern::Object { loc, inner } => {
                    let properties = &inner.properties;
                    let mut acc = Vec::new();
                    for prop in properties.iter() {
                        match prop {
                            ast::pattern::object::Property::NormalProperty(p) => {
                                let hint_opt = other_pattern_hint_opt(providers, &p.pattern);
                                match (&p.key, hint_opt) {
                                    (ast::pattern::object::Key::Identifier(id), Some(h)) => {
                                        acc.push(ObjectPropPatternHint::ObjectPropPatternHint(
                                            id.name.dupe(),
                                            id.loc.dupe(),
                                            h,
                                        ));
                                    }
                                    (
                                        ast::pattern::object::Key::StringLiteral((sl_loc, sl)),
                                        Some(h),
                                    ) => {
                                        acc.push(ObjectPropPatternHint::ObjectPropPatternHint(
                                            sl.value.dupe(),
                                            sl_loc.dupe(),
                                            h,
                                        ));
                                    }
                                    (
                                        ast::pattern::object::Key::NumberLiteral((nl_loc, nl)),
                                        Some(h),
                                    ) if let Some(name) =
                                        js_number::numeric_property_name(nl.value) =>
                                    {
                                        acc.push(ObjectPropPatternHint::ObjectPropPatternHint(
                                            name,
                                            nl_loc.dupe(),
                                            h,
                                        ));
                                    }
                                    _ => return None,
                                }
                            }
                            ast::pattern::object::Property::RestElement(r) => {
                                let h = other_pattern_hint_opt(providers, &r.argument)?;
                                acc.push(ObjectPropPatternHint::ObjectSpreadPropPatternHint(h))
                            }
                        }
                    }
                    Some(HintNode::ComposedObjectPatternHint(loc.dupe(), acc))
                }
            }
        }

        let other_pattern_hints = |p: &ast::pattern::Pattern<ALoc, ALoc>| -> AstHints {
            match other_pattern_hint_opt(self.env_info.providers.as_ref(), p) {
                Some(h) => vec![Hint::HintT(h, HintKind::ExpectedTypeHint)],
                None => vec![],
            }
        };

        match (operator, lhs_node) {
            (None, Pattern::Expression { loc: _, inner })
                if matches!(
                    inner.deref().deref(),
                    ast::expression::ExpressionInner::Member { .. }
                ) =>
            {
                let expr_inner = inner.deref().deref();
                if let ast::expression::ExpressionInner::Member {
                    loc: member_loc,
                    inner: member,
                } = expr_inner
                {
                    // Use super member to visit sub-expressions to avoid record a read of the member
                    ast_visitor::member_default(self, member_loc, member)?;

                    self.add_ordinary_binding(
                        member_loc.dupe(),
                        mk_pattern_reason(&left_tuple.1),
                        Def::MemberAssign(Box::new(MemberAssignData {
                            member_loc: member_loc.dupe(),
                            member: member.as_ref().clone(),
                            rhs: (right.loc().dupe(), right.clone()),
                        })),
                    );
                    let hints = expression_pattern_hints(inner.deref(), &self.class_stack);
                    let right_tuple = (right.loc().dupe(), right.clone());
                    self.visit_expression(EnclosingContext::NoContext, &hints, &right_tuple.1)?;
                }
            }
            (None, Pattern::Expression { loc: _, inner }) => {
                let e = inner.as_ref();
                self.assignment_pattern(&left_tuple.1)?;
                let value_root = mk_value(None, None, false, right.clone());
                self.add_destructure_bindings(value_root, &left_tuple)?;
                let hints = expression_pattern_hints(e, &self.class_stack);
                self.visit_expression(EnclosingContext::NoContext, &hints, right)?;
            }
            (None, _) => {
                self.assignment_pattern(&left_tuple.1)?;
                let value_root = mk_value(None, None, false, right.clone());
                self.add_destructure_bindings(value_root, &left_tuple)?;
                let hints = other_pattern_hints(&left_tuple.1);
                self.visit_expression(EnclosingContext::NoContext, &hints, right)?;
            }
            (Some(op), Pattern::Identifier { inner, .. }) => {
                let id_loc = inner.name.loc.dupe();
                let name = &inner.name.name;
                self.add_named_ordinary_binding(
                    id_loc.dupe(),
                    name.dupe(),
                    mk_reason(VirtualReasonDesc::RIdentifier(name.dupe()), id_loc.dupe()),
                    Def::OpAssign(Box::new(OpAssignData {
                        exp_loc: loc.dupe(),
                        lhs: left_tuple.clone(),
                        op: op.clone(),
                        rhs: (right.loc().dupe(), right.clone()),
                        assertion,
                    })),
                );
                let hints = other_pattern_hints(&left_tuple.1);
                self.visit_expression(EnclosingContext::NoContext, &hints, right)?;
            }
            (Some(op), Pattern::Expression { loc: _, inner }) => {
                let e = inner.as_ref();
                // In op_assign, the LHS will also be read.
                let cond = match op {
                    AssignmentOperator::AndAssign | AssignmentOperator::OrAssign => {
                        EnclosingContext::OtherTestContext
                    }
                    _ => EnclosingContext::NoContext,
                };
                let e_tuple = (e.loc().dupe(), e.clone());
                self.visit_expression(cond, &vec![], &e_tuple.1)?;
                self.add_ordinary_binding(
                    e.loc().dupe(),
                    mk_pattern_reason(&left_tuple.1),
                    Def::OpAssign(Box::new(OpAssignData {
                        exp_loc: loc.dupe(),
                        lhs: left_tuple.clone(),
                        op: op.clone(),
                        rhs: (right.loc().dupe(), right.clone()),
                        assertion,
                    })),
                );
                let hints = expression_pattern_hints(e, &self.class_stack);
                self.visit_expression(EnclosingContext::NoContext, &hints, right)?;
            }
            (Some(_), Pattern::Array { .. } | Pattern::Object { .. }) => {
                // [a] += 1;
                // ({b} += 1);
                // will have invalid-lhs errors, we shouldn't visit the LHS pattern.
                let hints = other_pattern_hints(&left_tuple.1);
                self.visit_expression(EnclosingContext::NoContext, &hints, right)?;
            }
        }
        Ok(())
    }

    fn visit_call_expression(
        &mut self,
        hints: &AstHints,
        cond: EnclosingContext,
        visit_callee: impl FnOnce(
            &mut Self,
            &AstHints,
            &ast::expression::Expression<ALoc, ALoc>,
        ) -> Result<(), EnvInvariant<ALoc>>,
        loc: ALoc,
        expr: &ast::expression::Call<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        use flow_common::reason::mk_expression_reason;
        use flow_parser::ast_utils::is_call_to_is_array;
        use flow_parser::ast_utils::is_call_to_object_dot_freeze;
        use flow_parser::ast_utils::is_call_to_require;
        use flow_parser::ast_utils::is_specially_typed_object_call;

        let callee = &expr.callee;
        let targs = &expr.targs;
        let arg_list = &expr.arguments;
        let arguments = &arg_list.arguments;

        // Provide hint in the very special case of immediate function execution
        let callee_hints: AstHints = match (callee.deref(), arguments.deref()) {
            (
                ast::expression::ExpressionInner::ArrowFunction { inner, .. }
                | ast::expression::ExpressionInner::Function { inner, .. },
                [],
            ) if inner.tparams.is_none()
                && inner.params.this_.is_none()
                && inner.params.params.is_empty()
                && inner.params.rest.is_none() =>
            {
                let decomp = HintDecomposition::new(HintDecompositionInner::CompImmediateFuncCall);
                Hint::decompose(decomp, hints.clone())
            }
            _ => vec![],
        };

        visit_callee(self, &callee_hints, callee)?;

        if let Some(targs) = targs {
            self.call_type_args(targs)?;
        }

        // A proven bare assertion call checks the asserted argument as a
        // condition (see `call_arg_list`), so record that argument under
        // conditional context like legacy `invariant`. Otherwise standalone
        // resolution checks member accesses strictly and the cached result
        // poisons the lenient statement-level check.
        let test_arg_index = self
            .assertion_calls
            .get(callee.loc())
            .copied()
            .filter(|assertion| bare_asserted_argument(*assertion, arguments).is_some())
            .and_then(|assertion| match assertion.target {
                AssertionTarget::Parameter(index) => Some(index),
                AssertionTarget::Receiver => None,
            });

        match &**arguments {
            [ast::expression::ExpressionOrSpread::Expression(expr)]
                if is_call_to_is_array(callee) =>
            {
                self.visit_expression(cond.clone(), &vec![], expr)?;
            }
            [ast::expression::ExpressionOrSpread::Expression(expr)]
                if is_call_to_require(callee) =>
            {
                self.visit_expression(cond.clone(), &vec![], expr)?;
            }
            [ast::expression::ExpressionOrSpread::Expression(expr)]
                if is_call_to_object_dot_freeze(callee) =>
            {
                self.visit_expression(EnclosingContext::NoContext, hints, expr)?;
            }
            _ if is_specially_typed_object_call(callee, targs.is_some(), arguments) => {
                for arg in arguments.iter() {
                    match arg {
                        ast::expression::ExpressionOrSpread::Expression(expr) => {
                            self.visit_expression(EnclosingContext::NoContext, &vec![], expr)?;
                        }
                        ast::expression::ExpressionOrSpread::Spread(spread) => {
                            self.visit_expression(
                                EnclosingContext::NoContext,
                                &vec![],
                                &spread.argument,
                            )?;
                        }
                    }
                }
            }
            _ => {
                let build_member_hints = |member: &ast::expression::Member<ALoc, ALoc>,
                                          class_stack: &ClassStack|
                 -> AstHints {
                    let base_hint: AstHints = vec![Hint::HintT(
                        HintNode::ValueHint(EnclosingContext::NoContext, member.object.clone()),
                        HintKind::ExpectedTypeHint,
                    )];
                    match &member.property {
                        ast::expression::member::Property::PropertyIdentifier(id) => {
                            let decomp = HintDecomposition::new(
                                HintDecompositionInner::DecompMethodName(id.name.dupe()),
                            );
                            Hint::decompose(decomp, base_hint)
                        }
                        ast::expression::member::Property::PropertyPrivateName(pn) => {
                            let decomp = HintDecomposition::new(
                                HintDecompositionInner::DecompMethodPrivateName(
                                    pn.name.dupe(),
                                    class_stack.clone(),
                                ),
                            );
                            Hint::decompose(decomp, base_hint)
                        }
                        ast::expression::member::Property::PropertyExpression(_) => {
                            let decomp =
                                HintDecomposition::new(HintDecompositionInner::DecompMethodElem);
                            Hint::decompose(decomp, base_hint)
                        }
                    }
                };

                let call_arguments_hints: AstHints = match callee.deref() {
                    // Use the type of the callee directly as hint if the member access is refined
                    ast::expression::ExpressionInner::Member {
                        loc: callee_loc,
                        inner: member,
                    } if self.env_info.env_values.get_ordinary(callee_loc).is_none() => {
                        build_member_hints(member, &self.class_stack)
                    }
                    ast::expression::ExpressionInner::OptionalMember {
                        loc: callee_loc,
                        inner: opt_member,
                    } if self.env_info.env_values.get_ordinary(callee_loc).is_none() => {
                        build_member_hints(&opt_member.member, &self.class_stack)
                    }
                    ast::expression::ExpressionInner::Super { .. } => {
                        let decomp =
                            HintDecomposition::new(HintDecompositionInner::DecompCallSuper);
                        let base_hint: AstHints = vec![Hint::HintT(
                            HintNode::ValueHint(EnclosingContext::NoContext, callee.clone()),
                            HintKind::ExpectedTypeHint,
                        )];
                        Hint::decompose(decomp, base_hint)
                    }
                    _ => {
                        vec![Hint::HintT(
                            HintNode::ValueHint(EnclosingContext::NoContext, callee.clone()),
                            HintKind::ExpectedTypeHint,
                        )]
                    }
                };
                let call_reason = mk_expression_reason(&Expression::new(
                    ast::expression::ExpressionInner::Call {
                        loc: loc.dupe(),
                        inner: Arc::new(expr.clone()),
                    },
                ));
                self.visit_call_arguments(
                    &call_reason,
                    &call_arguments_hints,
                    hints,
                    arg_list,
                    targs,
                    test_arg_index,
                )?;
            }
        }
        Ok(())
    }

    fn visit_call_arguments(
        &mut self,
        call_reason: &VirtualReason<ALoc>,
        call_arguments_hints: &AstHints,
        return_hints: &AstHints,
        arg_list: &ast::expression::ArgList<ALoc, ALoc>,
        targs: &Option<ast::expression::CallTypeArgs<ALoc, ALoc>>,
        test_arg_index: Option<usize>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let arguments = &arg_list.arguments;

        let decomp =
            HintDecomposition::new(HintDecompositionInner::SimplifyCallee(call_reason.clone()));
        let call_arguments_hints = Hint::decompose(decomp, call_arguments_hints.clone());

        let param_str_list: Vec<Option<FlowSmolStr>> = (0..arguments.len()).map(|_| None).collect();

        for (i, arg) in arguments.iter().enumerate() {
            match arg {
                ast::expression::ExpressionOrSpread::Expression(expr) => {
                    let expr_tuple = (expr.loc().dupe(), expr.dupe());
                    if Some(i) == test_arg_index {
                        self.visit_expression(
                            EnclosingContext::OtherTestContext,
                            &vec![],
                            &expr_tuple.1,
                        )?;
                    } else {
                        let arg_list_clone = arg_list.clone();
                        let targs_clone = targs.clone();
                        let return_hints_clone = return_hints.clone();
                        let hints = {
                            let instantiate_decomp =
                                HintDecomposition::new(HintDecompositionInner::InstantiateCallee(
                                    flow_common::hint::FunCallImplicitInstantiationHints {
                                        reason: call_reason.clone(),
                                        return_hints: std::rc::Rc::new(
                                            flow_lazy::Lazy::new_forced(return_hints_clone),
                                        ),
                                        targs: std::rc::Rc::new(flow_lazy::Lazy::new_forced(
                                            targs_clone,
                                        )),
                                        arg_list: std::rc::Rc::new(flow_lazy::Lazy::new_forced(
                                            arg_list_clone,
                                        )),
                                        arg_index: i as i32,
                                    },
                                ));
                            let hints =
                                Hint::decompose(instantiate_decomp, call_arguments_hints.clone());
                            let func_param_decomp =
                                HintDecomposition::new(HintDecompositionInner::DecompFuncParam(
                                    param_str_list.clone(),
                                    i as i32,
                                    None,
                                ));
                            Hint::decompose(func_param_decomp, hints)
                        };
                        self.visit_expression(EnclosingContext::NoContext, &hints, &expr_tuple.1)?;
                    }
                }
                ast::expression::ExpressionOrSpread::Spread(spread) => {
                    let spread_tuple = (spread.argument.loc().dupe(), spread.argument.clone());
                    self.visit_expression(EnclosingContext::NoContext, &vec![], &spread_tuple.1)?;
                }
            }
        }
        Ok(())
    }

    fn visit_optional_call_expression(
        &mut self,
        hints: &AstHints,
        cond: EnclosingContext,
        loc: ALoc,
        expr: &ast::expression::OptionalCall<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let call = &expr.call;
        self.visit_call_expression(
            hints,
            cond,
            |this, callee_hints, callee| {
                this.visit_expression(EnclosingContext::NoContext, callee_hints, callee)?;
                Ok(())
            },
            loc,
            call,
        )?;
        Ok(())
    }

    fn visit_new_expression(
        &mut self,
        hints: &AstHints,
        loc: ALoc,
        expr: &ast::expression::New<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        use flow_common::reason::mk_expression_reason;

        let callee = &expr.callee;
        let targs = &expr.targs;
        let arguments = &expr.arguments;

        self.visit_expression(EnclosingContext::NoContext, &vec![], callee)?;
        if let Some(targs) = targs {
            self.call_type_args(targs)?;
        }

        let decomp = HintDecomposition::new(HintDecompositionInner::DecompCallNew);
        let base_hint: AstHints = vec![Hint::HintT(
            HintNode::ValueHint(EnclosingContext::NoContext, callee.clone()),
            HintKind::ExpectedTypeHint,
        )];
        let call_arguments_hints = Hint::decompose(decomp, base_hint);

        let default_arg_list = ast::expression::ArgList {
            loc: callee.loc().dupe(),
            arguments: Vec::new().into(),
            comments: None,
        };
        let arg_list = arguments.as_ref().unwrap_or(&default_arg_list);

        let call_reason =
            mk_expression_reason(&Expression::new(ast::expression::ExpressionInner::New {
                loc: loc.dupe(),
                inner: Arc::new(expr.clone()),
            }));
        self.visit_call_arguments(
            &call_reason,
            &call_arguments_hints,
            hints,
            arg_list,
            targs,
            None,
        )?;
        Ok(())
    }

    fn visit_member_expression(
        &mut self,
        cond: EnclosingContext,
        hints: &AstHints,
        loc: ALoc,
        mem: &ast::expression::Member<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        if let Some(crate::env_api::EnvEntry::AssigningWrite(reason)) =
            self.env_info.env_entries.get_ordinary(&loc)
        {
            self.add_ordinary_binding(
                loc.dupe(),
                reason.dupe(),
                Def::ExpressionDef(Box::new(ExpressionDef {
                    cond_context: cond,
                    expr: Expression::new(ast::expression::ExpressionInner::Member {
                        loc: loc.dupe(),
                        inner: Arc::new(mem.clone()),
                    }),
                    hints: hints.clone(),
                    chain: true,
                })),
            );
        }
        ast_visitor::member_default(self, &loc, mem)?;
        Ok(())
    }

    fn visit_optional_member_expression(
        &mut self,
        cond: EnclosingContext,
        hints: &AstHints,
        loc: ALoc,
        mem: &ast::expression::OptionalMember<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        if let Some(crate::env_api::EnvEntry::AssigningWrite(reason)) =
            self.env_info.env_entries.get_ordinary(&loc)
        {
            self.add_ordinary_binding(
                loc.dupe(),
                reason.dupe(),
                Def::ExpressionDef(Box::new(ExpressionDef {
                    cond_context: cond,
                    expr: Expression::new(ast::expression::ExpressionInner::OptionalMember {
                        loc: loc.dupe(),
                        inner: Arc::new(mem.clone()),
                    }),
                    hints: hints.clone(),
                    chain: true,
                })),
            );
        }
        ast_visitor::member_default(self, &loc, &mem.member)?;
        Ok(())
    }

    fn cast(
        &mut self,
        annot: &ast::types::Annotation<ALoc, ALoc>,
        expression: &ast::expression::Expression<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let hints = vec![Hint::HintT(
            HintNode::AnnotationHint(FlowOrdMap::new(), annot.clone()),
            HintKind::ExpectedTypeHint,
        )];
        self.visit_expression(EnclosingContext::NoContext, &hints, expression)?;
        self.type_annotation(annot)?;
        Ok(())
    }

    fn visit_unary_expression(
        &mut self,
        hints: &AstHints,
        expr: &ast::expression::Unary<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let (hints, cond) = match expr.operator {
            ast::expression::UnaryOperator::Not => (vec![], EnclosingContext::OtherTestContext),
            ast::expression::UnaryOperator::Await => {
                let decomposed = Hint::decompose(
                    HintDecomposition::new(HintDecompositionInner::DecompAwait),
                    hints.clone(),
                );
                (decomposed, EnclosingContext::NoContext)
            }
            _ => (vec![], EnclosingContext::NoContext),
        };
        let arg_tuple = (expr.argument.loc().dupe(), expr.argument.clone());
        self.visit_expression(cond, &hints, &arg_tuple.1)?;
        Ok(())
    }

    fn visit_jsx_expression(
        &mut self,
        hints: &AstHints,
        expr: &ast::jsx::ExpressionContainer<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        match &expr.expression {
            ast::jsx::expression_container::Expression::Expression(e) => {
                self.visit_expression(EnclosingContext::NoContext, hints, e)?;
            }
            ast::jsx::expression_container::Expression::EmptyExpression => {}
        }
        Ok(())
    }

    fn visit_jsx_children(
        &mut self,
        hints: &AstHints,
        children: &[ast::jsx::Child<ALoc, ALoc>],
    ) -> Result<(), EnvInvariant<ALoc>> {
        let filtered_children: Vec<_> = children
            .iter()
            .filter(|child| match child {
                ast::jsx::Child::Text { loc, inner } => {
                    flow_common_utils::utils_jsx::trim_jsx_text(
                        loc.to_loc_exn().dupe(),
                        &inner.value,
                    )
                    .is_some()
                }
                _ => true,
            })
            .collect();

        let single_child = filtered_children.len() == 1;
        for (i, child) in filtered_children.iter().enumerate() {
            let hints = if single_child {
                hints.clone()
            } else {
                let decomp = HintDecomposition::new(HintDecompositionInner::DecompArrElement(
                    Some(i as i32),
                ));
                Hint::decompose(decomp, hints.clone())
            };

            let loc = child.loc().dupe();
            self.record_hint(loc.dupe(), hints.clone());

            match child {
                ast::jsx::Child::Element { loc, inner } => {
                    self.jsx_element(loc, inner)?;
                }
                ast::jsx::Child::Fragment { loc, inner } => {
                    self.jsx_fragment(loc, inner)?;
                }
                ast::jsx::Child::ExpressionContainer { inner, .. } => {
                    self.visit_jsx_expression(&hints, inner)?;
                }
                ast::jsx::Child::SpreadChild { .. } => {}
                ast::jsx::Child::Text { .. } => {}
            }
        }
        Ok(())
    }

    fn visit_expression(
        &mut self,
        cond: EnclosingContext,
        hints: &AstHints,
        expr: &ast::expression::Expression<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let loc = expr.loc();

        let hints = {
            let key = EnvKey::new(DefLocType::ArrayProviderLoc, loc.dupe());
            if let Some(crate::env_api::EnvEntry::AssigningWrite(reason)) =
                self.env_info.env_entries.get(&key)
            {
                self.add_binding(
                    key,
                    reason.clone(),
                    Def::ExpressionDef(Box::new(ExpressionDef {
                        cond_context: cond.clone(),
                        expr: expr.dupe(),
                        hints: vec![],
                        chain: false,
                    })),
                );
                vec![]
            } else {
                hints.clone()
            }
        };
        let hints_before_synthesizable_check = hints.clone();
        let hints = if expression_is_definitely_synthesizable(self.autocomplete_hooks, expr) {
            vec![]
        } else {
            hints
        };
        {
            let key = EnvKey::new(DefLocType::ExpressionLoc, loc.dupe());
            if let Some(crate::env_api::EnvEntry::AssigningWrite(reason)) =
                self.env_info.env_entries.get(&key)
            {
                self.add_binding(
                    key,
                    reason.clone(),
                    Def::ExpressionDef(Box::new(ExpressionDef {
                        cond_context: cond.clone(),
                        expr: expr.dupe(),
                        hints: hints.clone(),
                        chain: false,
                    })),
                );
            }
        }

        match expr.deref() {
            // Member expressions are synthesizable, but retaining their hints is
            // necessary for contextual inference of computed object properties.
            ast::expression::ExpressionInner::Member { .. }
            // The following kinds of expressions are also typically synthesizable,
            // but it is often unseful for Natural Inference to have hint information
            // to decide if we are going to generalize singleton types or not.
            | ast::expression::ExpressionInner::Array { .. }
            | ast::expression::ExpressionInner::ArrowFunction { .. }
            | ast::expression::ExpressionInner::Function { .. }
            | ast::expression::ExpressionInner::Object { .. }
            | ast::expression::ExpressionInner::Logical { .. }
            | ast::expression::ExpressionInner::Conditional { .. }
            | ast::expression::ExpressionInner::Match { .. }
            | ast::expression::ExpressionInner::Identifier { .. }
            | ast::expression::ExpressionInner::StringLiteral { .. }
            | ast::expression::ExpressionInner::NumberLiteral { .. }
            | ast::expression::ExpressionInner::BooleanLiteral { .. }
            | ast::expression::ExpressionInner::BigIntLiteral { .. }
            | ast::expression::ExpressionInner::Unary { .. }
            | ast::expression::ExpressionInner::TemplateLiteral { .. } => {
                self.record_hint(loc.dupe(), hints_before_synthesizable_check.clone());
            }
            _ => {
                self.record_hint(loc.dupe(), hints.clone());
            }
        }

        match &**expr {
            ast::expression::ExpressionInner::Array {
                loc: arr_loc,
                inner,
                ..
            } => {
                self.visit_array_expression(&hints, arr_loc.dupe(), inner)?;
            }
            ast::expression::ExpressionInner::ArrowFunction { inner, .. } => {
                let inner_fn: ast::function::Function<ALoc, ALoc> = (**inner).clone();
                let scope_kind = func_scope_kind(None, &inner_fn);
                let hints_clone = hints.clone();
                let hints_before = hints_before_synthesizable_check.clone();
                let loc = loc.dupe();
                self.in_new_tparams_env(false, |this| {
                    this.visit_function(&scope_kind, &hints_clone, &hints_before, &inner_fn)?;
                    if let Some(crate::env_api::EnvEntry::AssigningWrite(reason)) =
                        this.env_info.env_entries.get_ordinary(&loc)
                    {
                        let def = def_of_function(
                            &this.assertion_calls,
                            this.tparams.clone(),
                            hints_clone.clone(),
                            false,
                            loc.dupe(),
                            BTreeMap::new(),
                            BTreeMap::new(),
                            true,
                            inner_fn.clone(),
                        );
                        this.add_ordinary_binding(loc.dupe(), reason.clone(), def);
                    }
                    Ok(())
                })?;
            }
            ast::expression::ExpressionInner::Assignment { inner, .. } => {
                self.visit_assignment_expression(false, loc.dupe(), inner)?;
            }
            ast::expression::ExpressionInner::Function { inner, loc: fn_loc } => {
                self.visit_function_expr(
                    &hints,
                    &hints_before_synthesizable_check,
                    true,
                    None,
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    false,
                    false,
                    fn_loc.dupe(),
                    inner,
                )?;
            }
            ast::expression::ExpressionInner::Object { inner, .. } => {
                self.visit_object_expression(&hints_before_synthesizable_check, inner)?;
            }
            ast::expression::ExpressionInner::Record { inner, .. } => {
                self.visit_record_expression(&hints_before_synthesizable_check, inner)?;
            }
            ast::expression::ExpressionInner::Member { loc: m_loc, inner } => {
                self.visit_member_expression(cond, &hints, m_loc.dupe(), inner)?;
            }
            ast::expression::ExpressionInner::OptionalMember { loc: m_loc, inner } => {
                self.visit_optional_member_expression(cond, &hints, m_loc.dupe(), inner)?;
            }
            ast::expression::ExpressionInner::Binary { inner, .. } => {
                self.visit_binary_expression(cond, inner)?;
            }
            ast::expression::ExpressionInner::Logical { inner, .. } => {
                self.visit_logical_expression(&hints, cond, inner)?;
            }
            ast::expression::ExpressionInner::Call { inner, loc } => {
                self.visit_call_expression(
                    &hints,
                    cond,
                    |this, callee_hints, callee_expr| {
                        this.visit_expression(
                            EnclosingContext::NoContext,
                            callee_hints,
                            callee_expr,
                        )?;
                        Ok(())
                    },
                    loc.dupe(),
                    inner,
                )?;
            }
            ast::expression::ExpressionInner::OptionalCall { inner, loc } => {
                self.visit_optional_call_expression(&hints, cond, loc.dupe(), inner)?;
            }
            ast::expression::ExpressionInner::New { inner, .. } => {
                self.visit_new_expression(&hints, loc.dupe(), inner)?;
            }
            ast::expression::ExpressionInner::Unary { inner, .. } => {
                self.visit_unary_expression(&hints, inner)?;
            }
            ast::expression::ExpressionInner::Conditional { inner, .. } => {
                self.visit_conditional(&hints, inner)?;
            }
            ast::expression::ExpressionInner::AsConstExpression { inner, .. } => {
                self.visit_expression(cond, &hints, &inner.expression)?;
            }
            ast::expression::ExpressionInner::Match { inner, .. } => {
                self.visit_match_expression(&hints, inner.as_ref())?;
            }
            ast::expression::ExpressionInner::Class { .. }
            | ast::expression::ExpressionInner::Identifier { .. }
            | ast::expression::ExpressionInner::Import { .. }
            | ast::expression::ExpressionInner::JSXElement { .. }
            | ast::expression::ExpressionInner::JSXFragment { .. }
            | ast::expression::ExpressionInner::StringLiteral { .. }
            | ast::expression::ExpressionInner::NumberLiteral { .. }
            | ast::expression::ExpressionInner::BooleanLiteral { .. }
            | ast::expression::ExpressionInner::NullLiteral { .. }
            | ast::expression::ExpressionInner::RegExpLiteral { .. }
            | ast::expression::ExpressionInner::BigIntLiteral { .. }
            | ast::expression::ExpressionInner::ModuleRefLiteral { .. }
            | ast::expression::ExpressionInner::MetaProperty { .. }
            | ast::expression::ExpressionInner::Sequence { .. }
            | ast::expression::ExpressionInner::Super { .. }
            | ast::expression::ExpressionInner::TaggedTemplate { .. }
            | ast::expression::ExpressionInner::TemplateLiteral { .. }
            | ast::expression::ExpressionInner::This { .. }
            | ast::expression::ExpressionInner::TypeCast { .. }
            | ast::expression::ExpressionInner::AsExpression { .. }
            | ast::expression::ExpressionInner::TSSatisfies { .. }
            | ast::expression::ExpressionInner::Update { .. }
            | ast::expression::ExpressionInner::Yield { .. } => {
                ast_visitor::expression_default(self, expr)?;
            }
        }
        Ok(())
    }

    fn visit_array_expression(
        &mut self,
        array_hints: &AstHints,
        loc: ALoc,
        expr: &ast::expression::Array<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        use HintDecompositionInner::*;
        use ast::expression::ArrayElement;

        let elements = &expr.elements;

        // Fold with seen_spread state
        let mut seen_spread = false;
        for (i, element) in elements.iter().enumerate() {
            match element {
                ArrayElement::Expression(expr_inner) => {
                    let hints = if seen_spread {
                        vec![]
                    } else {
                        let index = Some(i as i32);
                        let decomp = HintDecomposition::new(DecompArrElement(index));
                        Hint::decompose(decomp, array_hints.clone())
                    };
                    let expr_tuple = (expr_inner.loc().dupe(), expr_inner.clone());
                    self.visit_expression(EnclosingContext::NoContext, &hints, &expr_tuple.1)?;
                }
                ArrayElement::Spread(spread_elem) => {
                    let hints = if seen_spread {
                        vec![]
                    } else {
                        let decomp = HintDecomposition::new(DecompArrSpread(i as i32));
                        Hint::decompose(decomp, array_hints.clone())
                    };
                    let argument = &spread_elem.argument;
                    let arg_tuple = (argument.loc().dupe(), argument.clone());
                    self.visit_expression(EnclosingContext::NoContext, &hints, &arg_tuple.1)?;
                    seen_spread = true;
                }
                ArrayElement::Hole(_) => {}
            }
        }

        if elements.is_empty() {
            // We overwrite the hint on empty array with the hint on empty array element.
            // Since only the hint on empty array element will be read, this is safe.
            let decomp = HintDecomposition::new(DecompEmptyArrayElement);
            self.record_hint(loc, Hint::decompose(decomp, array_hints.clone()));
        }
        Ok(())
    }

    fn visit_conditional(
        &mut self,
        hints: &AstHints,
        expr: &ast::expression::Conditional<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let test = &expr.test;
        let consequent = &expr.consequent;
        let alternate = &expr.alternate;

        self.visit_expression(EnclosingContext::OtherTestContext, &vec![], test)?;

        if expression_is_definitely_synthesizable(self.autocomplete_hooks, alternate) {
            // Special-case for expressions like `cond ? [] : [exp]`
            let mut consequent_hints = hints.clone();
            consequent_hints.push(Hint::HintT(
                HintNode::ValueHint(EnclosingContext::NoContext, alternate.clone()),
                HintKind::BestEffortHint,
            ));
            self.visit_expression(EnclosingContext::NoContext, &consequent_hints, consequent)?;

            self.visit_expression(EnclosingContext::NoContext, hints, alternate)?;
        } else {
            self.visit_expression(EnclosingContext::NoContext, hints, consequent)?;

            let mut alternate_hints = hints.clone();
            alternate_hints.push(Hint::HintT(
                HintNode::ValueHint(EnclosingContext::NoContext, consequent.clone()),
                HintKind::BestEffortHint,
            ));
            self.visit_expression(EnclosingContext::NoContext, &alternate_hints, alternate)?;
        }
        Ok(())
    }

    fn visit_binary_expression(
        &mut self,
        cond: EnclosingContext,
        expr: &ast::expression::Binary<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        use ast::expression::BinaryOperator;

        use crate::eq_test::visit_eq_test;

        let operator = &expr.operator;
        let left = &expr.left;
        let right = &expr.right;

        match (operator, &cond) {
            (BinaryOperator::Instanceof, EnclosingContext::OtherTestContext) => {
                self.visit_expression(cond.clone(), &vec![], left)?;
                self.visit_expression(EnclosingContext::NoContext, &vec![], right)?;
            }
            (
                BinaryOperator::Equal
                | BinaryOperator::NotEqual
                | BinaryOperator::StrictEqual
                | BinaryOperator::StrictNotEqual,
                EnclosingContext::OtherTestContext,
            ) => {
                // Define an enum to capture what action to take
                enum EqTestAction {
                    TypeOf(
                        ast::expression::Expression<ALoc, ALoc>,
                        ast::expression::Expression<ALoc, ALoc>,
                    ),
                    Literal(
                        ast::expression::Expression<ALoc, ALoc>,
                        ast::expression::Expression<ALoc, ALoc>,
                    ),
                    Null(
                        ast::expression::Expression<ALoc, ALoc>,
                        ast::expression::Expression<ALoc, ALoc>,
                    ),
                    Void(
                        ast::expression::Expression<ALoc, ALoc>,
                        ast::expression::Expression<ALoc, ALoc>,
                    ),
                    MemberOther(
                        ast::expression::Expression<ALoc, ALoc>,
                        ast::expression::Expression<ALoc, ALoc>,
                    ),
                    OtherMember(
                        ast::expression::Expression<ALoc, ALoc>,
                        ast::expression::Expression<ALoc, ALoc>,
                    ),
                    Other(
                        ast::expression::Expression<ALoc, ALoc>,
                        ast::expression::Expression<ALoc, ALoc>,
                    ),
                }

                let action: EqTestAction = visit_eq_test(
                    |_loc, expr, value, _s, _b| EqTestAction::TypeOf(expr.dupe(), value.clone()),
                    |_strict, _sense, _loc, expr, _refine_kind, value| {
                        EqTestAction::Literal(expr.dupe(), value.clone())
                    },
                    |_sense, _strict, _loc, expr, value| {
                        EqTestAction::Null(expr.dupe(), value.clone())
                    },
                    |_sense, _strict, _check, _loc, expr, value| {
                        EqTestAction::Void(expr.dupe(), value.clone())
                    },
                    |expr, value| EqTestAction::MemberOther(expr.dupe(), value.clone()),
                    |value, expr| EqTestAction::OtherMember(value.clone(), expr.dupe()),
                    false, // is_switch_cond_context
                    |left, right| EqTestAction::Other(left.clone(), right.clone()),
                    false, // strict
                    false, // sense
                    ALoc::none(),
                    left,
                    right,
                );

                // Now execute the action with mutable self
                match action {
                    EqTestAction::TypeOf(ref expr, ref value) => {
                        let expr_tuple = (expr.loc().dupe(), expr.dupe());
                        self.visit_expression(cond, &vec![], &expr_tuple.1)?;
                        // For TypeOf: visit the value under NoContext
                        let value_tuple = (value.loc().dupe(), value.clone());
                        self.visit_expression(
                            EnclosingContext::NoContext,
                            &vec![],
                            &value_tuple.1,
                        )?;
                    }
                    EqTestAction::Literal(ref expr, ref value) => {
                        self.visit_expression(cond, &vec![], expr)?;
                        self.visit_expression(
                            EnclosingContext::LiteralTestContext,
                            &vec![],
                            value,
                        )?;
                    }
                    EqTestAction::Null(ref expr, ref value)
                    | EqTestAction::Void(ref expr, ref value) => {
                        let expr_tuple = (expr.loc().dupe(), expr.dupe());
                        self.visit_expression(cond, &vec![], &expr_tuple.1)?;
                        // For Null/Void: visit the value under NoContext
                        let value_tuple = (value.loc().dupe(), value.clone());
                        self.visit_expression(
                            EnclosingContext::NoContext,
                            &vec![],
                            &value_tuple.1,
                        )?;
                    }
                    EqTestAction::MemberOther(ref expr, ref value)
                    | EqTestAction::OtherMember(ref value, ref expr) => {
                        let expr_tuple = (expr.loc().dupe(), expr.dupe());
                        self.visit_expression(cond.clone(), &vec![], &expr_tuple.1)?;
                        let value_tuple = (value.loc().dupe(), value.clone());
                        self.visit_expression(cond, &vec![], &value_tuple.1)?;
                    }
                    EqTestAction::Other(ref left, ref right) => {
                        let left_tuple = (left.loc().dupe(), left.clone());
                        self.visit_expression(cond.clone(), &vec![], &left_tuple.1)?;
                        let right_tuple = (right.loc().dupe(), right.clone());
                        self.visit_expression(cond, &vec![], &right_tuple.1)?;
                    }
                }
            }
            _ => {
                let left_tuple = (left.loc().dupe(), left.clone());
                self.visit_expression(EnclosingContext::NoContext, &vec![], &left_tuple.1)?;
                let right_tuple = (right.loc().dupe(), right.clone());
                self.visit_expression(EnclosingContext::NoContext, &vec![], &right_tuple.1)?;
            }
        }
        Ok(())
    }

    fn visit_logical_expression(
        &mut self,
        hints: &AstHints,
        cond: EnclosingContext,
        expr: &ast::expression::Logical<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        use HintDecompositionInner::*;
        use ast::expression::LogicalOperator;

        let operator = &expr.operator;
        let left = &expr.left;
        let right = &expr.right;

        let (left_cond, left_hints, right_hints) = match operator {
            LogicalOperator::And => (
                EnclosingContext::OtherTestContext,
                hints.clone(),
                hints.clone(),
            ),
            LogicalOperator::Or => {
                let decomp = HintDecomposition::new(CompMaybeT);
                let left_hints = Hint::decompose(decomp, hints.clone());
                let mut right_hints = hints.clone();
                right_hints.push(Hint::HintT(
                    HintNode::ValueHint(EnclosingContext::NoContext, left.clone()),
                    HintKind::BestEffortHint,
                ));
                (EnclosingContext::OtherTestContext, left_hints, right_hints)
            }
            LogicalOperator::NullishCoalesce => {
                let decomp = HintDecomposition::new(CompMaybeT);
                let left_hints = Hint::decompose(decomp, hints.clone());
                let mut right_hints = hints.clone();
                right_hints.push(Hint::HintT(
                    HintNode::ValueHint(EnclosingContext::NoContext, left.clone()),
                    HintKind::BestEffortHint,
                ));
                (cond.clone(), left_hints, right_hints)
            }
        };

        self.visit_expression(left_cond, &left_hints, left)?;
        self.visit_expression(cond, &right_hints, right)?;
        Ok(())
    }

    fn visit_object_expression(
        &mut self,
        object_hints: &AstHints,
        expr: &ast::expression::Object<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        use HintDecompositionInner::*;
        use ast::expression::object::Key;
        use ast::expression::object::NormalProperty;
        use ast::expression::object::Property;
        use flow_common::js_number;
        use flow_common::reason::mk_expression_reason;

        let properties = &expr.properties;

        let has_autocomplete = properties.iter().any(|prop| match prop {
            Property::NormalProperty(normal_prop) => match normal_prop {
                NormalProperty::Init { key, value, .. } => match key {
                    Key::Identifier(id) => {
                        (self.autocomplete_hooks.obj_prop_decl_hook)(&id.name, &id.loc)
                    }
                    Key::StringLiteral((loc, sl)) => {
                        (self.autocomplete_hooks.obj_prop_decl_hook)(&sl.value, loc)
                    }
                    Key::NumberLiteral((loc, nl)) => {
                        if let Some(name) = js_number::numeric_property_name(nl.value) {
                            if (self.autocomplete_hooks.obj_prop_decl_hook)(&name, loc) {
                                true
                            } else {
                                expression_has_autocomplete(self.autocomplete_hooks, value)
                            }
                        } else {
                            expression_has_autocomplete(self.autocomplete_hooks, value)
                        }
                    }
                    _ => expression_has_autocomplete(self.autocomplete_hooks, value),
                },
                _ => false,
            },
            Property::SpreadProperty(_) => false,
        });

        let object_hints = if has_autocomplete {
            // During autocomplete, we are working with ASTs with placeholder values,
            // which can make sentinel refinements refine to empty. In these cases, it's better to
            // have a coarser set of results instead of nothing.
            object_hints.clone()
        } else {
            let checks =
                crate::eq_test::object_properties_possible_sentinel_refinements(properties);
            let decomp = HintDecomposition::new(DecompSentinelRefinement(checks));
            Hint::decompose(decomp, object_hints.clone())
        };

        let visit_object_key_and_compute_hint = |this: &mut Self,
                                                 key: &Key<ALoc, ALoc>|
         -> Result<AstHints, EnvInvariant<ALoc>> {
            match key {
                Key::StringLiteral((_, sl)) => {
                    let decomp = HintDecomposition::new(DecompObjProp(sl.value.dupe()));
                    Ok(Hint::decompose(decomp, object_hints.clone()))
                }
                Key::NumberLiteral((_, nl))
                    if let Some(name) = js_number::numeric_property_name(nl.value) =>
                {
                    let decomp = HintDecomposition::new(DecompObjProp(name));
                    Ok(Hint::decompose(decomp, object_hints.clone()))
                }
                Key::NumberLiteral(_) | Key::BigIntLiteral(_) => Ok(vec![]),
                Key::Identifier(id) if &*id.name == "__proto__" => Ok(vec![]),
                Key::Identifier(id) => {
                    let decomp = HintDecomposition::new(DecompObjProp(id.name.dupe()));
                    Ok(Hint::decompose(decomp, object_hints.clone()))
                }
                Key::PrivateName(_) => Ok(vec![]),
                Key::Computed(computed) => {
                    let expression = &computed.expression;
                    let expr_tuple = (expression.loc().dupe(), expression.clone());
                    this.visit_expression(EnclosingContext::IndexContext, &vec![], &expr_tuple.1)?;
                    let reason = mk_expression_reason(expression);
                    let decomp = HintDecomposition::new(DecompObjComputed(reason));
                    Ok(Hint::decompose(decomp, object_hints.clone()))
                }
            }
        };

        for prop in properties.iter() {
            match prop {
                Property::NormalProperty(normal_prop) => match normal_prop {
                    NormalProperty::Init { key, value, .. } => {
                        let hints = visit_object_key_and_compute_hint(self, key)?;
                        let value_tuple = (value.loc().dupe(), value.clone());
                        self.visit_expression(EnclosingContext::NoContext, &hints, &value_tuple.1)?;
                        if matches!(key, Key::Computed(_)) {
                            // We will be using this as hint for computed values.
                            // See Statement.create_computed_prop.
                            let loc = value.loc().dupe();
                            if !self.has_hint(&loc) {
                                self.record_hint(loc, hints);
                            }
                        }
                    }
                    NormalProperty::Method { key, value, .. } => {
                        let func_hints = visit_object_key_and_compute_hint(self, key)?;
                        let (fn_loc, fn_expr) = value;
                        self.visit_function_expr(
                            &func_hints,
                            &func_hints,
                            false,
                            None,
                            &BTreeMap::new(),
                            &BTreeMap::new(),
                            false,
                            false,
                            fn_loc.dupe(),
                            fn_expr,
                        )?;
                    }
                    NormalProperty::Get { key, value, .. } => {
                        let func_hints = visit_object_key_and_compute_hint(self, key)?;
                        let (fn_loc, fn_expr) = value;
                        self.visit_function_expr(
                            &func_hints,
                            &func_hints,
                            false,
                            None,
                            &BTreeMap::new(),
                            &BTreeMap::new(),
                            false,
                            false,
                            fn_loc.dupe(),
                            fn_expr,
                        )?;
                    }
                    NormalProperty::Set { key, value, .. } => {
                        let func_hints = visit_object_key_and_compute_hint(self, key)?;
                        let (fn_loc, fn_expr) = value;
                        self.visit_function_expr(
                            &func_hints,
                            &func_hints,
                            false,
                            None,
                            &BTreeMap::new(),
                            &BTreeMap::new(),
                            false,
                            false,
                            fn_loc.dupe(),
                            fn_expr,
                        )?;
                    }
                },
                Property::SpreadProperty(spread_prop) => {
                    let argument = &spread_prop.argument;
                    let decomp = HintDecomposition::new(DecompObjSpread);
                    let hints = Hint::decompose(decomp, object_hints.clone());
                    let arg_tuple = (argument.loc().dupe(), argument.clone());
                    self.visit_expression(EnclosingContext::NoContext, &hints, &arg_tuple.1)?;
                }
            }
        }
        Ok(())
    }

    fn visit_record_expression(
        &mut self,
        record_hints: &AstHints,
        record: &ast::expression::Record<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        use HintDecompositionInner::*;

        let constructor = &record.constructor;
        let targs = &record.targs;
        let (props_loc, props) = &record.properties;

        let constructor_loc = constructor.loc().dupe();

        self.visit_expression(EnclosingContext::NoContext, &vec![], constructor)?;
        if let Some(ta) = targs {
            self.call_type_args(ta)?;
        }

        let decomp = HintDecomposition::new(DecompCallNew);
        let base_hints = vec![Hint::HintT(
            HintNode::ValueHint(EnclosingContext::NoContext, constructor.clone()),
            HintKind::ExpectedTypeHint,
        )];
        let call_arguments_hints = Hint::decompose(decomp, base_hints);
        let arg = ast::expression::ExpressionOrSpread::Expression(Expression::new(
            ast::expression::ExpressionInner::Object {
                loc: props_loc.dupe(),
                inner: Arc::new(props.clone()),
            },
        ));
        let arg_list = ast::expression::ArgList {
            loc: constructor_loc.dupe(),
            arguments: vec![arg].into(),
            comments: None,
        };
        let call_reason = mk_reason(None, constructor_loc.dupe());

        self.visit_call_arguments(
            &call_reason,
            &call_arguments_hints,
            record_hints,
            &arg_list,
            targs,
            None,
        )?;
        Ok(())
    }

    fn visit_match_expression(
        &mut self,
        hints: &AstHints,
        x: &ast::match_::Match<ALoc, ALoc, ast::expression::Expression<ALoc, ALoc>>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let ast::match_::Match {
            arg,
            cases,
            match_keyword_loc,
            ..
        } = x;

        self.visit_expression(EnclosingContext::NoContext, &vec![], arg)?;
        self.add_ordinary_binding_with_kind(
            match_keyword_loc.dupe(),
            mk_reason(None, match_keyword_loc.dupe()),
            DefinitionReferenceKind::Match,
            Def::Binding(Box::new(Binding::Root(mk_value(
                None,
                None,
                true,
                arg.clone(),
            )))),
        );

        let value_hints: BTreeMap<usize, _> = cases
            .iter()
            .enumerate()
            .filter_map(|(i, case)| {
                let body_expr = &case.body;
                if expression_is_definitely_synthesizable(self.autocomplete_hooks, body_expr) {
                    let hint = Hint::HintT(
                        HintNode::ValueHint(EnclosingContext::NoContext, body_expr.dupe()),
                        HintKind::BestEffortHint,
                    );
                    Some((i, hint))
                } else {
                    None
                }
            })
            .collect();

        let mut prev_pattern_loc: Option<ALoc> = None;
        for (i, case) in cases.iter().enumerate() {
            let ast::match_::Case {
                pattern,
                body,
                guard,
                case_match_root_loc,
                ..
            } = case;
            let acc = Root::MatchCaseRoot(Box::new(MatchCaseRootData {
                case_match_root_loc: case_match_root_loc.dupe(),
                root_pattern_loc: pattern.loc().dupe(),
                prev_pattern_loc: prev_pattern_loc.dupe(),
            }));
            self.add_match_destructure_bindings(
                case_match_root_loc.dupe(),
                guard.is_some(),
                prev_pattern_loc.dupe(),
                acc,
                pattern,
            )?;
            self.match_pattern(pattern)?;
            if let Some(guard_expr) = guard {
                let guard_tuple = (guard_expr.loc().dupe(), guard_expr.dupe());
                self.visit_expression(EnclosingContext::OtherTestContext, &vec![], &guard_tuple.1)?;
            }
            // We use best-effort value hints for cases other than the current case.
            // Hints are ordered as the cases are in source, top to bottom.
            let other_case_hints: Vec<_> = value_hints
                .iter()
                .filter(|&(&idx, _)| idx != i)
                .map(|(_, hint)| hint.clone())
                .collect();
            let mut combined_hints = hints.clone();
            combined_hints.extend(other_case_hints);
            let body_tuple = (body.loc().dupe(), body.clone());
            self.visit_expression(EnclosingContext::NoContext, &combined_hints, &body_tuple.1)?;

            prev_pattern_loc = Some(pattern.loc().dupe());
        }
        Ok(())
    }

    fn add_match_destructure_bindings(
        &mut self,
        case_match_root_loc: ALoc,
        has_guard: bool,
        prev_pattern_loc: Option<ALoc>,
        root: Root,
        pattern: &ast::match_pattern::MatchPattern<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        use crate::env_api::DefLocType::MatchCasePatternLoc;
        use crate::env_api::EnvKey;

        let pattern_loc = pattern.loc().dupe();

        self.add_binding(
            EnvKey::new(MatchCasePatternLoc, pattern_loc.dupe()),
            mk_reason(None, pattern_loc.dupe()),
            Def::MatchCasePattern(Box::new(MatchCasePatternData {
                case_match_root_loc,
                has_guard,
                pattern: (pattern_loc.dupe(), pattern.clone()),
                prev_pattern_loc,
            })),
        );

        use std::cell::RefCell;
        enum MatchPatternAction {
            Binding(ALoc, FlowSmolStr, Binding),
            NonBindingLeaf(ALoc, Binding),
            Expression(ast::expression::Expression<ALoc, ALoc>),
            Intermediate(ALoc, Binding),
        }
        let actions = RefCell::new(Vec::new());

        match_pattern::visit_pattern(
            &mut |loc, name, binding| {
                actions
                    .borrow_mut()
                    .push(MatchPatternAction::Binding(loc, name.dupe(), binding));
            },
            &mut |loc, binding| {
                actions
                    .borrow_mut()
                    .push(MatchPatternAction::NonBindingLeaf(loc, binding));
            },
            &mut |expr| {
                actions
                    .borrow_mut()
                    .push(MatchPatternAction::Expression(expr.dupe()));
            },
            &mut |loc, binding| {
                actions
                    .borrow_mut()
                    .push(MatchPatternAction::Intermediate(loc, binding));
            },
            Binding::Root(root),
            pattern,
        );

        for action in actions.into_inner() {
            match action {
                MatchPatternAction::Binding(loc, name, binding) => {
                    let binding = self.mk_hooklike_if_necessary(
                        flow_parser::ast_utils::hook_name(&name),
                        binding,
                    );
                    self.add_named_ordinary_binding(
                        loc.dupe(),
                        name.dupe(),
                        mk_reason(VirtualReasonDesc::RIdentifier(name.dupe()), loc),
                        Def::Binding(Box::new(binding)),
                    );
                }
                MatchPatternAction::NonBindingLeaf(loc, binding) => {
                    self.add_destructure_binding(loc, binding);
                }
                MatchPatternAction::Expression(expr) => {
                    let expr_tuple = (expr.loc().dupe(), expr);
                    self.visit_expression(EnclosingContext::MatchPattern, &vec![], &expr_tuple.1)?;
                }
                MatchPatternAction::Intermediate(loc, binding) => {
                    self.add_destructure_binding(loc, binding);
                }
            }
        }
        Ok(())
    }

    fn enum_declaration_impl(
        &mut self,
        declared: bool,
        loc: &ALoc,
        enum_decl: &ast::statement::EnumDeclaration<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let id_loc = enum_decl.id.loc.dupe();
        let name = &enum_decl.id.name;
        let body = &enum_decl.body;

        self.add_ordinary_binding(
            id_loc.dupe(),
            mk_reason(
                VirtualReasonDesc::REnum {
                    name: Some(name.dupe()),
                },
                id_loc.dupe(),
            ),
            Def::Enum {
                loc: loc.dupe(),
                name: name.dupe(),
                declared,
                body: body.clone(),
            },
        );

        ast_visitor::enum_declaration_default(self, loc, enum_decl)
    }
}

// ============================================================================
// Phase 13: Public Functions
// ============================================================================

// Note: show_scope_kind is already defined earlier in this file (line 1328)

// ============================================================================
// AstVisitor Trait Implementation for DefFinder
// ============================================================================
//
// Note: This is a partial implementation of the AstVisitor trait for DefFinder.
// The OCaml class inherits from scope_info_iterator and overrides many methods.
// This Rust implementation overrides key methods to dispatch to the visit_* helpers.
// TODO: Complete implementation of all method overrides for full functionality.

impl<'a, 'ast> AstVisitor<'ast, ALoc, ALoc, &'ast ALoc, EnvInvariant<ALoc>> for DefFinder<'a> {
    fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
        loc
    }

    fn normalize_type(type_: &'ast ALoc) -> &'ast ALoc {
        type_
    }

    fn statement_list(
        &mut self,
        stmts: &[ast::statement::Statement<ALoc, ALoc>],
    ) -> Result<(), EnvInvariant<ALoc>> {
        use ast::expression::ExpressionInner;
        use ast::expression::member::Property;
        use ast::pattern::Pattern;
        use ast::statement::ExportKind;
        use ast::statement::Statement;
        use ast::statement::StatementInner;
        use ast::types::AnnotationOrHint;
        use flow_common::reason::mk_pattern_reason;
        use flow_parser::ast_utils::hoist_function_and_component_declarations;

        // Function statics
        let stmts = hoist_function_and_component_declarations(stmts);

        // Pre-scan for declare namespace names. Only classes/declare classes whose
        // name matches a declare namespace should be deferred for merging. Deferring
        // all classes causes their static property assignments (ClassName.prop = value)
        // to be intercepted by the function statics pattern, which loses contextual
        // typing and produces spurious errors.
        let namespace_names: BTreeSet<FlowSmolStr> = stmts
            .iter()
            .filter_map(|stmt| {
                if let StatementInner::DeclareNamespace { inner, .. } = stmt.deref() {
                    if let ast::statement::declare_namespace::Id::Local(id) = &inner.id {
                        return Some(id.name.dupe());
                    }
                }
                None
            })
            .collect();

        // Pre-scan for class/function/declare-class/declare-function names. The
        // namespace arm uses this to decide whether to merge a namespace into a
        // sibling function/class regardless of source order. Without this set,
        // a `declare namespace X { ... }` that appears BEFORE its sibling
        // `declare class X { ... }` would be visited normally and its members
        // would never reach the class's namespace_types
        fn name_of_decl(stmt: &StatementInner<ALoc, ALoc>) -> Option<FlowSmolStr> {
            match stmt {
                StatementInner::ClassDeclaration { inner, .. } => {
                    inner.id.as_ref().map(|id| id.name.dupe())
                }
                StatementInner::FunctionDeclaration { inner, .. } => {
                    inner.id.as_ref().map(|id| id.name.dupe())
                }
                StatementInner::DeclareClass { inner, .. } => Some(inner.id.name.dupe()),
                StatementInner::DeclareFunction { inner, .. } => {
                    inner.id.as_ref().map(|id| id.name.dupe())
                }
                StatementInner::ExportNamedDeclaration { inner, .. }
                    if matches!(inner.export_kind, ast::statement::ExportKind::ExportValue) =>
                {
                    inner
                        .declaration
                        .as_ref()
                        .and_then(|d| name_of_decl(d.deref()))
                }
                StatementInner::ExportDefaultDeclaration { inner, .. } => {
                    if let ast::statement::export_default_declaration::Declaration::Declaration(
                        decl,
                    ) = &inner.declaration
                    {
                        name_of_decl(decl.deref())
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        let function_or_class_names: BTreeSet<FlowSmolStr> = stmts
            .iter()
            .filter_map(|stmt| name_of_decl(stmt.deref()))
            .collect();

        enum DeferredVisit {
            FunctionDecl {
                loc: ALoc,
                func: ast::function::Function<ALoc, ALoc>,
            },
            FunctionExpr {
                loc: ALoc,
                func: ast::function::Function<ALoc, ALoc>,
                arrow: bool,
                var_id: ast::Identifier<ALoc, ALoc>,
                hooklike: bool,
            },
            ClassDecl {
                loc: ALoc,
                class_: ast::class::Class<ALoc, ALoc>,
                kind: ClassKind,
            },
            DeclareClassDecl {
                loc: ALoc,
                decl: ast::statement::DeclareClass<ALoc, ALoc>,
            },
        }

        struct FunctionState {
            statics: BTreeMap<FlowSmolStr, Option<EnvKey<ALoc>>>,
            namespace_types: NamespaceTypesMap,
            deferred_visits: Vec<DeferredVisit>,
            declared_functions: Vec<(ALoc, ast::statement::DeclareFunction<ALoc, ALoc>)>,
            merge_candidate_loc: Option<ALoc>,
        }

        impl FunctionState {
            fn new() -> Self {
                Self {
                    statics: BTreeMap::new(),
                    namespace_types: BTreeMap::new(),
                    deferred_visits: Vec::new(),
                    declared_functions: Vec::new(),
                    merge_candidate_loc: None,
                }
            }

            fn record_merge_candidate(&mut self, loc: &ALoc) {
                match &self.merge_candidate_loc {
                    Some(existing) if existing <= loc => {}
                    _ => self.merge_candidate_loc = Some(loc.dupe()),
                }
            }
        }

        let mut state: BTreeMap<FlowSmolStr, FunctionState> = BTreeMap::new();

        // Helper to check if a statement matches the f.a = <e> pattern
        fn extract_func_static_assign<'a>(
            stmt: &'a Statement<ALoc, ALoc>,
        ) -> Option<(
            &'a ALoc,                        // assign_loc
            &'a ALoc,                        // member_loc
            &'a Pattern<ALoc, ALoc>,         // left
            &'a ast::Identifier<ALoc, ALoc>, // obj_id
            &'a ast::Identifier<ALoc, ALoc>, // prop_id
            &'a Expression<ALoc, ALoc>,      // init
        )> {
            if let StatementInner::Expression { inner, .. } = stmt.deref() {
                if let ExpressionInner::Assignment {
                    loc: assign_loc,
                    inner: assign_inner,
                } = &*inner.expression
                {
                    if assign_inner.operator.is_none() {
                        if let left @ Pattern::Expression {
                            inner: pat_inner, ..
                        } = &assign_inner.left
                        {
                            if let ExpressionInner::Member {
                                loc: member_loc,
                                inner: member_inner,
                            } = pat_inner.deref().deref()
                            {
                                if let ExpressionInner::Identifier {
                                    inner: obj_id_inner,
                                    ..
                                } = member_inner.object.deref()
                                {
                                    if let Property::PropertyIdentifier(prop_id) =
                                        &member_inner.property
                                    {
                                        return Some((
                                            assign_loc,
                                            member_loc,
                                            left,
                                            obj_id_inner,
                                            prop_id,
                                            &assign_inner.right,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            None
        }

        for stmt in stmts.iter() {
            // f.a = <e>;
            if let Some((assign_loc, member_loc, left, obj_id, prop_id, init)) =
                extract_func_static_assign(stmt)
            {
                if state.contains_key(&obj_id.name)
                    && !FUNC_OWN_PROPS.contains(prop_id.name.as_str())
                {
                    let obj_name = &obj_id.name;
                    let prop_name = &prop_id.name;

                    self.visit_assignment_expression(
                        true,
                        assign_loc.dupe(),
                        &ast::expression::Assignment {
                            operator: None,
                            left: left.clone(),
                            right: init.clone(),
                            comments: None,
                        },
                    )?;

                    if let Some(function_state) = state.get(obj_name) {
                        if !function_state.statics.contains_key(prop_name) {
                            // Only first assignment sets the type.
                            let key = if let ExpressionInner::Function { inner, .. } = &**init {
                                if let Some(id) = &inner.id {
                                    let fun_loc = id.loc.dupe();
                                    EnvKey::new(DefLocType::OrdinaryNameLoc, fun_loc)
                                } else {
                                    EnvKey::new(DefLocType::OrdinaryNameLoc, init.loc().dupe())
                                }
                            } else if let ExpressionInner::ArrowFunction { loc: fun_loc, .. } =
                                &**init
                            {
                                EnvKey::new(DefLocType::OrdinaryNameLoc, fun_loc.dupe())
                            } else {
                                self.add_ordinary_binding(
                                    member_loc.dupe(),
                                    mk_pattern_reason(left),
                                    Def::ExpressionDef(Box::new(ExpressionDef {
                                        cond_context: EnclosingContext::NoContext,
                                        hints: vec![],
                                        chain: false,
                                        expr: init.clone(),
                                    })),
                                );
                                EnvKey::new(DefLocType::OrdinaryNameLoc, member_loc.dupe())
                            };
                            let (entries, _) = self.acc();
                            let key_opt = if entries.contains_key(&key) {
                                Some(key)
                            } else {
                                None
                            };
                            if let Some(function_state) = state.get_mut(obj_name) {
                                function_state.statics.insert(prop_name.dupe(), key_opt);
                            }
                        }
                    }
                    continue;
                }
            }

            match stmt.deref() {
                StatementInner::FunctionDeclaration { loc, inner } if inner.id.is_some() => {
                    let func = inner.as_ref();
                    let Some(id) = func.id.as_ref() else {
                        return Err(EnvInvariant::new(
                            Some(loc.dupe()),
                            EnvInvariantFailure::Impossible(
                                "Expected function declaration id".into(),
                            ),
                        ));
                    };
                    let id_loc = id.loc.dupe();
                    let name = &id.name;
                    if has_assigning_write(
                        EnvKey::new(DefLocType::OrdinaryNameLoc, id_loc.dupe()),
                        &self.env_info.env_entries,
                    ) {
                        let deferred = DeferredVisit::FunctionDecl {
                            loc: loc.dupe(),
                            func: func.clone(),
                        };
                        let function_state =
                            state.entry(name.dupe()).or_insert_with(FunctionState::new);
                        function_state.record_merge_candidate(&id_loc);
                        function_state.deferred_visits.push(deferred);
                    } else {
                        self.statement(stmt)?;
                    }
                }

                // export function f() {}
                StatementInner::ExportNamedDeclaration { inner, .. }
                    if matches!(
                        inner.as_ref(),
                        ast::statement::ExportNamedDeclaration {
                            declaration: Some(decl),
                            export_kind: ExportKind::ExportValue,
                            specifiers: None,
                            source: None,
                            ..
                        } if matches!(decl.deref() , StatementInner::FunctionDeclaration { inner: func_inner, .. } if func_inner.id.is_some())
                    ) =>
                {
                    let export_decl = inner.as_ref();
                    if let Some(decl) = &export_decl.declaration {
                        if let StatementInner::FunctionDeclaration {
                            loc,
                            inner: func_inner,
                        } = decl.deref()
                        {
                            let func = func_inner.as_ref();
                            let Some(id) = func.id.as_ref() else {
                                return Err(EnvInvariant::new(
                                    Some(loc.dupe()),
                                    EnvInvariantFailure::Impossible(
                                        "Expected function declaration id".into(),
                                    ),
                                ));
                            };
                            let id_loc = id.loc.dupe();
                            let name = &id.name;
                            if has_assigning_write(
                                EnvKey::new(DefLocType::OrdinaryNameLoc, id_loc.dupe()),
                                &self.env_info.env_entries,
                            ) {
                                let deferred = DeferredVisit::FunctionDecl {
                                    loc: loc.dupe(),
                                    func: func.clone(),
                                };
                                let function_state =
                                    state.entry(name.dupe()).or_insert_with(FunctionState::new);
                                function_state.record_merge_candidate(&id_loc);
                                function_state.deferred_visits.push(deferred);
                            } else {
                                self.statement(stmt)?;
                            }
                        }
                    }
                }

                //  export default function f() {}
                StatementInner::ExportDefaultDeclaration { inner, .. }
                    if matches!(
                        inner.as_ref(),
                        ast::statement::ExportDefaultDeclaration {
                            declaration: ast::statement::export_default_declaration::Declaration::Declaration(
                                decl
                            ),
                            ..
                        } if matches!(decl.deref() , StatementInner::FunctionDeclaration { inner: func_inner, .. } if func_inner.id.is_some())
                    ) =>
                {
                    let export_decl = inner.as_ref();
                    if let ast::statement::export_default_declaration::Declaration::Declaration(
                        decl,
                    ) = &export_decl.declaration
                    {
                        if let StatementInner::FunctionDeclaration {
                            loc,
                            inner: func_inner,
                        } = decl.deref()
                        {
                            let func = func_inner.as_ref();
                            let Some(id) = func.id.as_ref() else {
                                return Err(EnvInvariant::new(
                                    Some(loc.dupe()),
                                    EnvInvariantFailure::Impossible(
                                        "Expected function declaration id".into(),
                                    ),
                                ));
                            };
                            let id_loc = id.loc.dupe();
                            let name = &id.name;
                            if has_assigning_write(
                                EnvKey::new(DefLocType::OrdinaryNameLoc, id_loc.dupe()),
                                &self.env_info.env_entries,
                            ) {
                                let deferred = DeferredVisit::FunctionDecl {
                                    loc: loc.dupe(),
                                    func: func.clone(),
                                };
                                let function_state =
                                    state.entry(name.dupe()).or_insert_with(FunctionState::new);
                                function_state.record_merge_candidate(&id_loc);
                                function_state.deferred_visits.push(deferred);
                            } else {
                                self.statement(stmt)?;
                            }
                        }
                    }
                }

                StatementInner::DeclareFunction { loc, inner } if inner.id.is_some() => {
                    let decl = inner.as_ref();
                    let Some(id) = decl.id.as_ref() else {
                        return Err(EnvInvariant::new(
                            Some(loc.dupe()),
                            EnvInvariantFailure::Impossible("Expected declare function id".into()),
                        ));
                    };
                    let id_loc = id.loc.dupe();
                    let name = &id.name;
                    if has_assigning_write(
                        EnvKey::new(DefLocType::OrdinaryNameLoc, id_loc.dupe()),
                        &self.env_info.env_entries,
                    ) {
                        let function_state =
                            state.entry(name.dupe()).or_insert_with(FunctionState::new);
                        function_state.record_merge_candidate(&id_loc);
                        function_state
                            .declared_functions
                            .push((loc.dupe(), decl.clone()));
                    } else {
                        self.statement(stmt)?;
                    }
                }

                // class Foo {}
                StatementInner::ClassDeclaration { loc, inner } if inner.id.is_some() => {
                    let class_ = inner.as_ref();
                    let Some(id) = class_.id.as_ref() else {
                        return Err(EnvInvariant::new(
                            Some(loc.dupe()),
                            EnvInvariantFailure::Impossible("Expected class declaration id".into()),
                        ));
                    };
                    let id_loc = id.loc.dupe();
                    let name = &id.name;
                    if namespace_names.contains(name)
                        && crate::env_api::has_non_global_assigning_write(
                            EnvKey::new(DefLocType::OrdinaryNameLoc, id_loc.dupe()),
                            &self.env_info.env_entries,
                        )
                    {
                        let deferred = DeferredVisit::ClassDecl {
                            loc: loc.dupe(),
                            class_: class_.clone(),
                            kind: ClassKind::Class,
                        };
                        let function_state =
                            state.entry(name.dupe()).or_insert_with(FunctionState::new);
                        function_state.record_merge_candidate(&id_loc);
                        function_state.deferred_visits.push(deferred);
                    } else {
                        self.statement(stmt)?;
                    }
                }

                // export class Foo {}
                StatementInner::ExportNamedDeclaration { inner, .. }
                    if matches!(
                        inner.as_ref(),
                        ast::statement::ExportNamedDeclaration {
                            declaration: Some(decl),
                            export_kind: ExportKind::ExportValue,
                            specifiers: None,
                            source: None,
                            ..
                        } if matches!(decl.deref() , StatementInner::ClassDeclaration { inner: class_inner, .. } if class_inner.id.is_some())
                    ) =>
                {
                    let export_decl = inner.as_ref();
                    if let Some(decl) = &export_decl.declaration {
                        if let StatementInner::ClassDeclaration {
                            loc,
                            inner: class_inner,
                        } = decl.deref()
                        {
                            let class_ = class_inner.as_ref();
                            let Some(id) = class_.id.as_ref() else {
                                return Err(EnvInvariant::new(
                                    Some(loc.dupe()),
                                    EnvInvariantFailure::Impossible(
                                        "Expected class declaration id".into(),
                                    ),
                                ));
                            };
                            let id_loc = id.loc.dupe();
                            let name = &id.name;
                            if namespace_names.contains(name)
                                && crate::env_api::has_non_global_assigning_write(
                                    EnvKey::new(DefLocType::OrdinaryNameLoc, id_loc.dupe()),
                                    &self.env_info.env_entries,
                                )
                            {
                                let deferred = DeferredVisit::ClassDecl {
                                    loc: loc.dupe(),
                                    class_: class_.clone(),
                                    kind: ClassKind::Class,
                                };
                                let function_state =
                                    state.entry(name.dupe()).or_insert_with(FunctionState::new);
                                function_state.record_merge_candidate(&id_loc);
                                function_state.deferred_visits.push(deferred);
                            } else {
                                self.statement(stmt)?;
                            }
                        }
                    }
                }

                // export default class Foo {}
                StatementInner::ExportDefaultDeclaration { inner, .. }
                    if matches!(
                        inner.as_ref(),
                        ast::statement::ExportDefaultDeclaration {
                            declaration: ast::statement::export_default_declaration::Declaration::Declaration(
                                decl
                            ),
                            ..
                        } if matches!(decl.deref() , StatementInner::ClassDeclaration { inner: class_inner, .. } if class_inner.id.is_some())
                    ) =>
                {
                    let export_decl = inner.as_ref();
                    if let ast::statement::export_default_declaration::Declaration::Declaration(
                        decl,
                    ) = &export_decl.declaration
                    {
                        if let StatementInner::ClassDeclaration {
                            loc,
                            inner: class_inner,
                        } = decl.deref()
                        {
                            let class_ = class_inner.as_ref();
                            let Some(id) = class_.id.as_ref() else {
                                return Err(EnvInvariant::new(
                                    Some(loc.dupe()),
                                    EnvInvariantFailure::Impossible(
                                        "Expected class declaration id".into(),
                                    ),
                                ));
                            };
                            let id_loc = id.loc.dupe();
                            let name = &id.name;
                            if namespace_names.contains(name)
                                && crate::env_api::has_non_global_assigning_write(
                                    EnvKey::new(DefLocType::OrdinaryNameLoc, id_loc.dupe()),
                                    &self.env_info.env_entries,
                                )
                            {
                                let deferred = DeferredVisit::ClassDecl {
                                    loc: loc.dupe(),
                                    class_: class_.clone(),
                                    kind: ClassKind::Class,
                                };
                                let function_state =
                                    state.entry(name.dupe()).or_insert_with(FunctionState::new);
                                function_state.record_merge_candidate(&id_loc);
                                function_state.deferred_visits.push(deferred);
                            } else {
                                self.statement(stmt)?;
                            }
                        }
                    }
                }

                // declare class Foo {}
                StatementInner::DeclareClass { loc, inner } => {
                    let decl = inner.as_ref();
                    let id_loc = decl.id.loc.dupe();
                    let name = &decl.id.name;
                    if namespace_names.contains(name)
                        && crate::env_api::has_non_global_assigning_write(
                            EnvKey::new(DefLocType::OrdinaryNameLoc, id_loc.dupe()),
                            &self.env_info.env_entries,
                        )
                    {
                        let deferred = DeferredVisit::DeclareClassDecl {
                            loc: loc.dupe(),
                            decl: decl.clone(),
                        };
                        let function_state =
                            state.entry(name.dupe()).or_insert_with(FunctionState::new);
                        function_state.record_merge_candidate(&id_loc);
                        function_state.deferred_visits.push(deferred);
                    } else {
                        self.statement(stmt)?;
                    }
                }

                StatementInner::DeclareNamespace { inner, .. } => {
                    if let ast::statement::declare_namespace::Id::Local(id) = &inner.id {
                        // Decide whether to merge this namespace into a sibling function/class:
                        //
                        // - Type-only namespace + sibling function/class (any order): merge.
                        //   The namespace contributes only types; folding them into the
                        //   sibling via `wrap_with_namespace_types` is the whole point of
                        //   TS-style declaration merging.
                        // - Value-bodied namespace + sibling function/class, with the
                        //   function/class appearing FIRST: merge (preserves existing behavior).
                        // - Value-bodied namespace + sibling function/class, with the
                        //   namespace appearing FIRST: do NOT merge. The namespace binds as
                        //   `DeclaredConst` on the value side and gets its own
                        //   `DeclaredNamespace` def. Forcing a merge here would drop that
                        //   def and leave the namespace's value-side AssigningWrite
                        //   unresolved, crashing with `MissingEnvWrite`.
                        let is_type_only_ns =
                            flow_parser::ast_utils::is_type_only_declaration_statement(stmt);
                        let has_sibling = function_or_class_names.contains(&id.name);
                        let function_first = state
                            .get(&id.name)
                            .and_then(|function_state| function_state.merge_candidate_loc.as_ref())
                            .is_some_and(|function_loc| function_loc < &id.loc);
                        let has_namespace_value_write = has_assigning_write(
                            EnvKey::ordinary(id.loc.dupe()),
                            &self.env_info.env_entries,
                        );
                        let should_merge = has_sibling
                            && (is_type_only_ns || (function_first && !has_namespace_value_write));
                        if should_merge {
                            let (namespace_values, namespace_types) =
                                collect_declared_namespace_members(
                                    &inner.body.1.body,
                                    self.env_info,
                                );
                            // Initialize a fresh function_state if the namespace appears
                            // before its sibling function/class — the sibling's arm will
                            // add its deferred_visit (and set merge_candidate_loc) later.
                            let function_state = state
                                .entry(id.name.dupe())
                                .or_insert_with(FunctionState::new);
                            for (member_name, env_key) in namespace_values {
                                function_state
                                    .statics
                                    .entry(member_name)
                                    .or_insert(Some(env_key));
                            }
                            for (member_name, env_key) in namespace_types {
                                function_state
                                    .namespace_types
                                    .entry(member_name)
                                    .or_insert(env_key);
                            }
                            self.visit_merged_declare_namespace_body(inner)?;
                            continue;
                        }
                    }
                    self.statement(stmt)?;
                }

                // const f = () => {}; const f = function () {};
                StatementInner::VariableDeclaration { inner, .. }
                    if inner.declarations.len() == 1
                        && matches!(
                            &inner.declarations[0].id,
                            Pattern::Identifier { inner: pat_id, .. }
                                if !pat_id.optional
                                    && matches!(pat_id.annot, AnnotationOrHint::Missing(_))
                                    && inner.declarations[0].init.as_ref().is_some_and(|e| {
                                        matches!(e.deref() , ExpressionInner::ArrowFunction { .. } | ExpressionInner::Function { .. })
                                    })
                        ) =>
                {
                    let declarations = &inner.declarations;
                    let decl = &declarations[0];
                    let Pattern::Identifier {
                        inner: pat_id_arc, ..
                    } = &decl.id
                    else {
                        return Err(EnvInvariant::new(
                            Some(decl.loc.dupe()),
                            EnvInvariantFailure::Impossible("Invalid AST structure".into()),
                        ));
                    };
                    let pat_id = pat_id_arc.as_ref();
                    let Some(init) = decl.init.as_ref() else {
                        return Err(EnvInvariant::new(
                            Some(decl.loc.dupe()),
                            EnvInvariantFailure::Impossible(
                                "Expected variable declarator init".into(),
                            ),
                        ));
                    };
                    let (func_loc, func, arrow) = match init.deref() {
                        ExpressionInner::ArrowFunction { loc, inner } => {
                            (loc.dupe(), inner.as_ref(), true)
                        }
                        ExpressionInner::Function { loc, inner } => {
                            (loc.dupe(), inner.as_ref(), false)
                        }
                        _ => {
                            return Err(EnvInvariant::new(
                                Some(decl.loc.dupe()),
                                EnvInvariantFailure::Impossible("Invalid AST structure".into()),
                            ));
                        }
                    };
                    let var_id = &pat_id.name;
                    let id_loc = var_id.loc.dupe();
                    let name = &var_id.name;

                    if has_assigning_write(
                        EnvKey::new(DefLocType::OrdinaryNameLoc, id_loc),
                        &self.env_info.env_entries,
                    ) {
                        let deferred = DeferredVisit::FunctionExpr {
                            loc: func_loc,
                            func: func.clone(),
                            arrow,
                            var_id: var_id.clone(),
                            hooklike: flow_parser::ast_utils::hook_name(name),
                        };
                        state
                            .entry(name.dupe())
                            .or_insert_with(FunctionState::new)
                            .deferred_visits
                            .push(deferred);
                    } else {
                        self.statement(stmt)?;
                    }
                }

                StatementInner::ExportNamedDeclaration {
                    inner: export_inner,
                    ..
                } if matches!(
                    export_inner.as_ref(),
                    ast::statement::ExportNamedDeclaration {
                        declaration: Some(decl),
                        export_kind: ExportKind::ExportValue,
                        specifiers: None,
                        source: None,
                        ..
                    } if matches!(decl.deref() , StatementInner::VariableDeclaration { inner: var_decl, .. }
                        if var_decl.declarations.len() == 1
                            && matches!(
                                &var_decl.declarations[0].id,
                                Pattern::Identifier { inner: pat_id, .. }
                                    if !pat_id.optional
                                        && matches!(pat_id.annot, AnnotationOrHint::Missing(_))
                                        && var_decl.declarations[0].init.as_ref().is_some_and(|e| {
                                            matches!(&**e, ExpressionInner::ArrowFunction { .. } | ExpressionInner::Function { .. })
                                        })
                            ))
                ) =>
                {
                    let export_decl = export_inner.as_ref();
                    if let Some(decl) = &export_decl.declaration {
                        if let StatementInner::VariableDeclaration {
                            inner: var_decl, ..
                        } = decl.deref()
                        {
                            let decl = &var_decl.declarations[0];
                            let Pattern::Identifier {
                                inner: pat_id_arc, ..
                            } = &decl.id
                            else {
                                return Err(EnvInvariant::new(
                                    Some(decl.loc.dupe()),
                                    EnvInvariantFailure::Impossible("Invalid AST structure".into()),
                                ));
                            };
                            let pat_id = pat_id_arc.as_ref();
                            let Some(init) = decl.init.as_ref() else {
                                return Err(EnvInvariant::new(
                                    Some(decl.loc.dupe()),
                                    EnvInvariantFailure::Impossible(
                                        "Expected variable declarator init".into(),
                                    ),
                                ));
                            };
                            let (func_loc, func, arrow) = match &**init {
                                ExpressionInner::ArrowFunction { loc, inner } => {
                                    (loc.dupe(), inner.as_ref(), true)
                                }
                                ExpressionInner::Function { loc, inner } => {
                                    (loc.dupe(), inner.as_ref(), false)
                                }
                                _ => {
                                    return Err(EnvInvariant::new(
                                        Some(decl.loc.dupe()),
                                        EnvInvariantFailure::Impossible(
                                            "Invalid AST structure".into(),
                                        ),
                                    ));
                                }
                            };
                            let var_id = &pat_id.name;
                            let id_loc = var_id.loc.dupe();
                            let name = &var_id.name;

                            if has_assigning_write(
                                EnvKey::new(DefLocType::OrdinaryNameLoc, id_loc),
                                &self.env_info.env_entries,
                            ) {
                                let deferred = DeferredVisit::FunctionExpr {
                                    loc: func_loc,
                                    func: func.clone(),
                                    arrow,
                                    var_id: var_id.clone(),
                                    hooklike: flow_parser::ast_utils::hook_name(name),
                                };
                                state
                                    .entry(name.dupe())
                                    .or_insert_with(FunctionState::new)
                                    .deferred_visits
                                    .push(deferred);
                            } else {
                                self.statement(stmt)?;
                            }
                        }
                    }
                }

                _ => {
                    self.statement(stmt)?;
                }
            }
        }

        for (_, function_state) in state {
            let statics: StaticsMap = function_state
                .statics
                .into_iter()
                .filter_map(|(k, v)| v.map(|env_key| (k, env_key)))
                .collect();
            let namespace_types = function_state.namespace_types;
            let declarations = function_state.declared_functions;
            for deferred in function_state.deferred_visits {
                match deferred {
                    DeferredVisit::FunctionDecl { loc, func } => {
                        self.visit_function_declaration(&statics, &namespace_types, loc, &func)?;
                    }
                    DeferredVisit::FunctionExpr {
                        loc,
                        func,
                        arrow,
                        var_id,
                        hooklike,
                    } => {
                        self.visit_function_expr(
                            &vec![],
                            &vec![],
                            !arrow,
                            Some(&var_id),
                            &statics,
                            &BTreeMap::new(),
                            arrow,
                            hooklike,
                            loc,
                            &func,
                        )?;
                    }
                    DeferredVisit::ClassDecl { loc, class_, kind } => {
                        self.class_internal(
                            kind,
                            &ClassNamespaceMembers {
                                values: statics.clone(),
                                types: namespace_types.clone(),
                            },
                            loc,
                            &class_,
                        )?;
                    }
                    DeferredVisit::DeclareClassDecl { loc, decl } => {
                        self.visit_declare_class(
                            &ClassNamespaceMembers {
                                values: statics.clone(),
                                types: namespace_types.clone(),
                            },
                            loc.dupe(),
                            &decl,
                        )?;
                    }
                }
            }
            if namespace_types.is_empty() && statics.is_empty() {
                for (loc, decl) in declarations.iter() {
                    self.declare_function(loc, decl)?;
                }
            } else {
                for (loc, decl) in declarations.iter() {
                    self.visit_declared_function(
                        loc.dupe(),
                        decl,
                        &declarations,
                        &statics,
                        &namespace_types,
                    )?;
                }
            }
        }

        Ok(())
    }

    fn variable_declarator(
        &mut self,
        kind: ast::VariableKind,
        declarator: &ast::statement::variable::Declarator<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        use ast::expression::ExpressionInner;
        use ast::pattern::Pattern;

        let id = &declarator.id;
        let init = &declarator.init;

        let concrete: Option<Root> = if let (
            Some(arr_expr),
            Pattern::Identifier {
                inner: id_inner, ..
            },
        ) = (init, id)
            && let ExpressionInner::Array {
                loc: arr_loc,
                inner: arr_inner,
            } = arr_expr.deref()
            && arr_inner.elements.is_empty()
        {
            let name_loc = &id_inner.name.loc;
            let array_providers = self
                .env_info
                .providers
                .providers_of_def(name_loc)
                .map(|def_providers| def_providers.array_providers.dupe())
                .unwrap_or_else(FlowOrdSet::new);
            Some(Root::EmptyArray(Box::new(EmptyArrayData {
                array_providers,
                arr_loc: arr_loc.dupe(),
            })))
        } else if let Some(obj_expr) = init
            && let ExpressionInner::Object {
                loc,
                inner: obj_inner,
            } = &**obj_expr
            && !obj_inner.properties.is_empty()
        {
            let this_write_locs = obj_this_write_locs(obj_inner.as_ref());
            match obj_properties_synthesizable(
                &self.assertion_calls,
                this_write_locs,
                obj_inner.as_ref(),
            ) {
                ObjectSynthKind::Unsynthesizable => Some(mk_value(
                    None,
                    Some(kind),
                    false,
                    Expression::new(ExpressionInner::Object {
                        loc: loc.dupe(),
                        inner: obj_inner.clone(),
                    }),
                )),
                synthesizable => Some(Root::ObjectValue(Box::new(ObjectValueData {
                    synthesizable,
                    obj_loc: loc.dupe(),
                    obj: obj_inner.as_ref().clone(),
                }))),
            }
        } else {
            init.as_ref()
                .map(|init_expr| mk_value(None, Some(kind), false, init_expr.dupe()))
        };
        let (source, hints): (Option<Root>, AstHints) = match destructure::type_of_pattern(id) {
            Some(annot) => {
                let annot_tuple = (annot.loc.dupe(), annot.annotation.clone());
                (
                    Some(Root::Annotation(Box::new(AnnotationData {
                        tparams_map: FlowOrdMap::new(),
                        optional: false,
                        has_default_expression: false,
                        react_deep_read_only: None,
                        param_loc: None,
                        annot: annot_tuple.clone(),
                        concrete: concrete.map(Box::new),
                    }))),
                    vec![Hint::HintT(
                        HintNode::AnnotationHint(FlowOrdMap::new(), annot.clone()),
                        HintKind::ExpectedTypeHint,
                    )],
                )
            }
            None => (concrete, Vec::new()),
        };
        if let Some(root) = source {
            self.add_destructure_bindings(root, &(id.loc().dupe(), id.clone()))?;
        }
        self.variable_declarator_pattern(kind, id)?;
        if let Some(init_expr) = init {
            self.visit_expression(EnclosingContext::NoContext, &hints, init_expr)?;
        }
        Ok(())
    }

    fn declare_variable(
        &mut self,
        loc: &ALoc,
        decl: &ast::statement::DeclareVariable<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let kind = decl.kind;
        for declarator in decl.declarations.iter() {
            if let ast::pattern::Pattern::Identifier { inner, .. } = &declarator.id {
                let id_loc = inner.name.loc.dupe();
                let name = inner.name.name.as_str();
                let hook_like = flow_parser::ast_utils::hook_name(name);

                match (&inner.annot, &declarator.init) {
                    (ast::types::AnnotationOrHint::Available(annot), _) => {
                        let binding = self.mk_hooklike_if_necessary(
                            hook_like,
                            Binding::Root(Root::Annotation(Box::new(AnnotationData {
                                tparams_map: FlowOrdMap::new(),
                                optional: false,
                                has_default_expression: false,
                                react_deep_read_only: None,
                                param_loc: None,
                                annot: (annot.loc.dupe(), annot.annotation.clone()),
                                concrete: None,
                            }))),
                        );

                        let reason =
                            mk_reason(VirtualReasonDesc::RIdentifier(name.into()), id_loc.dupe());

                        self.add_named_ordinary_binding(
                            id_loc,
                            name.into(),
                            reason,
                            Def::Binding(Box::new(binding)),
                        );
                    }
                    (ast::types::AnnotationOrHint::Missing(_), Some(init_expr)) => {
                        let binding = self.mk_hooklike_if_necessary(
                            hook_like,
                            Binding::Root(mk_value(None, Some(kind), true, init_expr.clone())),
                        );

                        let reason =
                            mk_reason(VirtualReasonDesc::RIdentifier(name.into()), id_loc.dupe());

                        self.add_named_ordinary_binding(
                            id_loc,
                            name.into(),
                            reason,
                            Def::Binding(Box::new(binding)),
                        );
                    }
                    (ast::types::AnnotationOrHint::Missing(_), None) => {
                        // Error case: no annotation and no init. Type checker will report the error.
                        // We still need to add a binding for completeness.
                        let reason =
                            mk_reason(VirtualReasonDesc::RIdentifier(name.into()), id_loc.dupe());

                        self.add_named_ordinary_binding(
                            id_loc,
                            name.into(),
                            reason,
                            Def::Binding(Box::new(Binding::Root(
                                Root::DeclareVariableMissingAnnotationAndInit,
                            ))),
                        );
                    }
                }
            }
        }
        ast_visitor::declare_variable_default(self, loc, decl)?;
        Ok(())
    }

    fn pattern_array_element(
        &mut self,
        kind: Option<ast::VariableKind>,
        elem: &ast::pattern::array::NormalElement<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let modified = ast::pattern::array::NormalElement {
            loc: elem.loc.dupe(),
            argument: elem.argument.clone(),
            default: None,
        };
        // Default should already be visited during destructuring visit.
        ast_visitor::pattern_array_element_default(self, kind, &modified)?;
        Ok(())
    }

    fn pattern_object_property(
        &mut self,
        kind: Option<ast::VariableKind>,
        prop: &ast::pattern::object::NormalProperty<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let modified = ast::pattern::object::NormalProperty {
            loc: prop.loc.dupe(),
            key: prop.key.clone(),
            pattern: prop.pattern.clone(),
            default: None,
            shorthand: prop.shorthand,
        };
        // Default should already be visited during destructuring visit.
        ast_visitor::pattern_object_property_default(self, kind, &modified)?;
        Ok(())
    }

    fn function_param(
        &mut self,
        param: &ast::function::Param<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let loc = match param {
            ast::function::Param::RegularParam { loc, .. } => loc.dupe(),
            ast::function::Param::ParamProperty { loc, .. } => loc.dupe(),
        };
        Err(fail(loc, "Should be visited by visit_function_param"))
    }

    fn function_this_param(
        &mut self,
        this_param: &ast::function::ThisParam<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let loc = &this_param.loc;
        let annot = &this_param.annot;

        self.add_ordinary_binding_with_kind(
            loc.dupe(),
            mk_reason(VirtualReasonDesc::RThis, loc.dupe()),
            DefinitionReferenceKind::This,
            Def::Binding(Box::new(Binding::Root(Root::Annotation(Box::new(
                AnnotationData {
                    tparams_map: self.tparams.clone(),
                    optional: false,
                    has_default_expression: false,
                    react_deep_read_only: None,
                    param_loc: None,
                    annot: (annot.loc.dupe(), annot.annotation.clone()),
                    concrete: None,
                },
            ))))),
        );
        ast_visitor::function_this_param_default(self, this_param)?;
        Ok(())
    }

    fn catch_clause_pattern(
        &mut self,
        pat: &ast::pattern::Pattern<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let source = match pat {
            ast::pattern::Pattern::Identifier { inner, .. } => match &inner.annot {
                ast::types::AnnotationOrHint::Available(annot) => match annot.annotation.deref() {
                    ast::types::TypeInner::Any { .. }
                    | ast::types::TypeInner::Mixed { .. }
                    | ast::types::TypeInner::Unknown { .. } => {
                        match destructure::type_of_pattern(pat) {
                            Some(annot) => Root::Annotation(Box::new(AnnotationData {
                                tparams_map: FlowOrdMap::new(),
                                optional: false,
                                has_default_expression: false,
                                react_deep_read_only: None,
                                param_loc: None,
                                annot: (annot.loc.dupe(), annot.annotation.clone()),
                                concrete: None,
                            })),
                            None => Root::CatchUnannotated,
                        }
                    }
                    _ => Root::CatchUnannotated,
                },
                _ => Root::CatchUnannotated,
            },
            _ => Root::CatchUnannotated,
        };

        self.add_destructure_bindings(source, &(pat.loc().dupe(), pat.clone()))?;
        ast_visitor::catch_clause_pattern_default(self, pat)?;
        Ok(())
    }

    fn component_declaration(
        &mut self,
        loc: &ALoc,
        stmt: &ast::statement::ComponentDeclaration<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.visit_component_declaration(loc.dupe(), stmt)?;
        Ok(())
    }

    fn function_expression(
        &mut self,
        loc: &ALoc,
        func: &ast::function::Function<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.visit_function_expr(
            &vec![],
            &vec![],
            true,
            None,
            &BTreeMap::new(),
            &BTreeMap::new(),
            false,
            false,
            loc.dupe(),
            func,
        )?;
        Ok(())
    }

    fn function_declaration(
        &mut self,
        loc: &ALoc,
        func: &ast::function::Function<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.visit_function_declaration(&BTreeMap::new(), &BTreeMap::new(), loc.dupe(), func)?;
        Ok(())
    }

    fn function_type(
        &mut self,
        ft: &ast::types::Function<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.in_new_tparams_env(true, |this| ast_visitor::function_type_default(this, ft))?;
        Ok(())
    }

    fn object_mapped_type_property(
        &mut self,
        mt: &ast::types::object::MappedType<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.in_new_tparams_env(true, |this| {
            ast_visitor::object_mapped_type_property_default(this, mt)?;
            Ok(())
        })?;
        Ok(())
    }

    fn function_(
        &mut self,
        _loc: &ALoc,
        expr: &ast::function::Function<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let scope_kind = func_scope_kind(None, expr);
        self.in_new_tparams_env(false, |this| {
            this.visit_function(&scope_kind, &vec![], &vec![], expr)?;
            Ok(())
        })?;
        Ok(())
    }

    fn class_(
        &mut self,
        loc: &ALoc,
        cls: &ast::class::Class<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.class_internal(
            ClassKind::Class,
            &ClassNamespaceMembers::default(),
            loc.dupe(),
            cls,
        )?;
        Ok(())
    }

    fn class_property(
        &mut self,
        prop: &ast::class::Property<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let key = &prop.key;
        let value = &prop.value;
        let annot = &prop.annot;
        let variance = &prop.variance;
        let decorators = &prop.decorators;

        for decorator in decorators.iter() {
            self.class_decorator(decorator)?;
        }
        self.object_key(key)?;
        self.type_annotation_hint(annot)?;
        let hints = match annot {
            ast::types::AnnotationOrHint::Available(annot) => {
                vec![Hint::HintT(
                    HintNode::AnnotationHint(FlowOrdMap::new(), annot.clone()),
                    HintKind::ExpectedTypeHint,
                )]
            }
            ast::types::AnnotationOrHint::Missing(_) => vec![],
        };
        self.visit_class_property_value(&hints, value)?;
        self.variance_opt(variance.as_ref())?;
        Ok(())
    }

    fn class_private_field(
        &mut self,
        pf: &ast::class::PrivateField<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let key = &pf.key;
        let value = &pf.value;
        let annot = &pf.annot;
        let variance = &pf.variance;
        let decorators = &pf.decorators;

        for decorator in decorators.iter() {
            self.class_decorator(decorator)?;
        }
        self.private_name(key)?;
        self.type_annotation_hint(annot)?;
        let hints = match annot {
            ast::types::AnnotationOrHint::Available(annot) => {
                vec![Hint::HintT(
                    HintNode::AnnotationHint(FlowOrdMap::new(), annot.clone()),
                    HintKind::ExpectedTypeHint,
                )]
            }
            ast::types::AnnotationOrHint::Missing(_) => vec![],
        };
        self.visit_class_property_value(&hints, value)?;
        self.variance_opt(variance.as_ref())?;
        Ok(())
    }

    fn class_method(
        &mut self,
        meth: &ast::class::Method<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let key = &meth.key;
        let (_, value) = &meth.value;
        let decorators = &meth.decorators;

        self.object_key(key)?;
        let scope_kind = func_scope_kind(Some(key), value);
        self.in_new_tparams_env(true, |this| {
            this.visit_function(&scope_kind, &vec![], &vec![], value)?;
            Ok(())
        })?;
        for decorator in decorators.iter() {
            self.class_decorator(decorator)?;
        }
        Ok(())
    }

    fn record_declaration(
        &mut self,
        loc: &ALoc,
        record: &ast::statement::RecordDeclaration<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.record_declaration_internal(loc.dupe(), record)?;
        Ok(())
    }

    fn declare_function(
        &mut self,
        loc: &ALoc,
        decl: &ast::statement::DeclareFunction<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        if let Some(id) = &decl.id {
            let id_loc = id.loc.dupe();
            let name = &id.name;
            let annot = &decl.annot;

            let is_hooklike = flow_parser::ast_utils::hook_name(name);

            let binding = Binding::Root(Root::Annotation(Box::new(AnnotationData {
                tparams_map: FlowOrdMap::new(),
                optional: false,
                has_default_expression: false,
                react_deep_read_only: None,
                param_loc: None,
                annot: (annot.loc.dupe(), annot.annotation.clone()),
                concrete: None,
            })));

            let binding = self.mk_hooklike_if_necessary(is_hooklike, binding);

            self.add_ordinary_binding_with_kind(
                id_loc,
                func_reason(false, false, loc.dupe()),
                DefinitionReferenceKind::Function {
                    async_: false,
                    generator: false,
                },
                Def::Binding(Box::new(binding)),
            );
        }

        ast_visitor::declare_function_default(self, loc, decl)
    }

    fn declare_class(
        &mut self,
        loc: &ALoc,
        decl: &ast::statement::DeclareClass<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.visit_declare_class(&ClassNamespaceMembers::default(), loc.dupe(), decl)?;
        Ok(())
    }

    fn declare_component(
        &mut self,
        loc: &ALoc,
        decl: &ast::statement::DeclareComponent<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let id_loc = decl.id.loc.dupe();
        let name = &decl.id.name;

        self.add_ordinary_binding(
            id_loc,
            mk_reason(VirtualReasonDesc::RComponent(name.dupe()), loc.dupe()),
            Def::DeclaredComponent(loc.dupe(), decl.clone()),
        );
        ast_visitor::declare_component_default(self, loc, decl)
    }

    fn assignment(
        &mut self,
        loc: &ALoc,
        _expr: &ast::expression::Assignment<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        Err(fail(
            loc.dupe(),
            "Should be visited by visit_assignment_expression",
        ))
    }

    fn update_expression(
        &mut self,
        loc: &ALoc,
        expr: &ast::expression::Update<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        if let ast::expression::ExpressionInner::Identifier { inner: id, .. } =
            expr.argument.deref()
        {
            let id_loc = id.loc.dupe();
            let name = &id.name;

            self.add_named_ordinary_binding(
                id_loc.dupe(),
                name.dupe(),
                mk_reason(VirtualReasonDesc::RIdentifier(name.dupe()), id_loc.dupe()),
                Def::Update {
                    exp_loc: loc.dupe(),
                    op: expr.operator.clone(),
                },
            );
        }

        ast_visitor::update_expression_default(self, loc, expr)
    }

    fn return_(
        &mut self,
        _loc: &ALoc,
        stmt: &ast::statement::Return<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        if let Some(argument) = &stmt.argument {
            let hints = self.return_hint_stack.last().cloned().unwrap_or_default();
            let cond = if self.predicate_kind.is_some() {
                EnclosingContext::OtherTestContext
            } else {
                EnclosingContext::NoContext
            };
            let arg_tuple = (argument.loc().dupe(), argument.clone());
            self.visit_expression(cond, &hints, &arg_tuple.1)?;
        }
        Ok(())
    }

    fn for_of_statement(
        &mut self,
        loc: &ALoc,
        stmt: &ast::statement::ForOf<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let await_ = stmt.await_;
        let right = &stmt.right;
        let right_tuple = (right.loc().dupe(), right.clone());

        match &stmt.left {
            ast::statement::for_of::Left::LeftDeclaration((
                _,
                ast::statement::VariableDeclaration { declarations, .. },
            )) if declarations.len() == 1 => {
                let decl = &declarations[0];
                let id = &decl.id;
                let id_tuple = (id.loc().dupe(), id.clone());
                let forof = Root::For(Box::new((ForKind::Of { await_ }, right_tuple.clone())));
                let source = match destructure::type_of_pattern(id) {
                    Some(annot) => Root::Annotation(Box::new(AnnotationData {
                        tparams_map: TparamsMap::default(),
                        optional: false,
                        has_default_expression: false,
                        react_deep_read_only: None,
                        param_loc: None,
                        annot: (annot.loc.dupe(), annot.annotation.clone()),
                        concrete: Some(Box::new(forof)),
                    })),
                    None => forof,
                };
                self.add_destructure_bindings(source, &id_tuple)?;
            }
            ast::statement::for_of::Left::LeftDeclaration(_) => {
                return Err(EnvInvariant::new(
                    Some(loc.dupe()),
                    EnvInvariantFailure::Impossible("Invalid AST structure".into()),
                ));
            }
            ast::statement::for_of::Left::LeftPattern(pat) => {
                let pat_tuple = (pat.loc().dupe(), pat.clone());
                self.add_destructure_bindings(
                    Root::For(Box::new((ForKind::Of { await_ }, right_tuple))),
                    &pat_tuple,
                )?;
            }
        }

        ast_visitor::for_of_statement_default(self, loc, stmt)?;
        Ok(())
    }

    fn for_in_statement(
        &mut self,
        loc: &ALoc,
        stmt: &ast::statement::ForIn<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let right = &stmt.right;
        let right_tuple = (right.loc().dupe(), right.clone());

        match &stmt.left {
            ast::statement::for_in::Left::LeftDeclaration((
                _,
                ast::statement::VariableDeclaration { declarations, .. },
            )) if declarations.len() == 1 => {
                let decl = &declarations[0];
                let id = &decl.id;
                let id_tuple = (id.loc().dupe(), id.clone());
                let forin = Root::For(Box::new((ForKind::In, right_tuple.clone())));
                let source = match destructure::type_of_pattern(id) {
                    Some(annot) => Root::Annotation(Box::new(AnnotationData {
                        tparams_map: TparamsMap::default(),
                        optional: false,
                        has_default_expression: false,
                        react_deep_read_only: None,
                        param_loc: None,
                        annot: (annot.loc.dupe(), annot.annotation.clone()),
                        concrete: Some(Box::new(forin)),
                    })),
                    None => forin,
                };
                self.add_destructure_bindings(source, &id_tuple)?;
            }
            ast::statement::for_in::Left::LeftDeclaration(_) => {
                return Err(EnvInvariant::new(
                    Some(loc.dupe()),
                    EnvInvariantFailure::Impossible("Invalid AST structure".into()),
                ));
            }
            ast::statement::for_in::Left::LeftPattern(pat) => {
                let pat_tuple = (pat.loc().dupe(), pat.clone());
                self.add_destructure_bindings(
                    Root::For(Box::new((ForKind::In, right_tuple))),
                    &pat_tuple,
                )?;
            }
        }

        ast_visitor::for_in_statement_default(self, loc, stmt)?;
        Ok(())
    }

    fn for_statement(
        &mut self,
        _loc: &ALoc,
        stmt: &ast::statement::For<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        if let Some(init) = &stmt.init {
            match init {
                ast::statement::for_::Init::InitDeclaration((var_loc, var_decl)) => {
                    self.variable_declaration(var_loc, var_decl)?;
                }
                ast::statement::for_::Init::InitExpression(expr) => {
                    self.visit_expression(EnclosingContext::NoContext, &vec![], expr)?;
                }
            }
        }
        if let Some(test) = &stmt.test {
            self.visit_expression(EnclosingContext::OtherTestContext, &vec![], test)?;
        }
        if let Some(update) = &stmt.update {
            self.visit_expression(EnclosingContext::OtherTestContext, &vec![], update)?;
        }
        self.statement(&stmt.body)
    }

    fn while_(
        &mut self,
        _loc: &ALoc,
        stmt: &ast::statement::While<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.visit_expression(EnclosingContext::OtherTestContext, &vec![], &stmt.test)?;
        self.statement(&stmt.body)
    }

    fn do_while(
        &mut self,
        _loc: &ALoc,
        stmt: &ast::statement::DoWhile<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.statement(&stmt.body)?;
        self.visit_expression(EnclosingContext::OtherTestContext, &vec![], &stmt.test)?;
        Ok(())
    }

    fn if_statement(
        &mut self,
        _loc: &ALoc,
        stmt: &ast::statement::If<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let test = &stmt.test;
        let consequent = &stmt.consequent;
        let alternate = &stmt.alternate;

        self.visit_expression(EnclosingContext::OtherTestContext, &vec![], test)?;
        let has_else = alternate.is_some();
        self.if_consequent_statement(has_else, consequent)?;
        if let Some(altern) = alternate {
            self.if_alternate_statement(altern)?;
        }
        Ok(())
    }

    fn type_alias(
        &mut self,
        loc: &ALoc,
        alias: &ast::statement::TypeAlias<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let id_loc = alias.id.loc.dupe();
        let name = &alias.id.name;

        self.add_ordinary_binding(
            id_loc.dupe(),
            mk_reason(VirtualReasonDesc::RType(name.dupe()), id_loc.dupe()),
            Def::TypeAlias(loc.dupe(), alias.clone()),
        );

        self.in_new_tparams_env(false, |this| {
            ast_visitor::type_alias_default(this, loc, alias)
        })
    }

    fn opaque_type(
        &mut self,
        loc: &ALoc,
        otype: &ast::statement::OpaqueType<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let id_loc = otype.id.loc.dupe();
        let name = &otype.id.name;

        self.add_ordinary_binding(
            id_loc.dupe(),
            mk_reason(VirtualReasonDesc::ROpaqueType(name.dupe()), id_loc.dupe()),
            Def::OpaqueType(loc.dupe(), otype.clone()),
        );

        self.in_new_tparams_env(false, |this| {
            ast_visitor::opaque_type_default(this, loc, otype)
        })
    }

    fn type_param(
        &mut self,
        kind: &ast_visitor::TypeParamsContext,
        tparam: &ast::types::TypeParam<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.add_type_param(kind, ImpliedBound::Mixed, tparam)
    }

    fn conditional_type(
        &mut self,
        conditional: &ast::types::Conditional<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let ast::types::Conditional {
            check_type,
            extends_type,
            true_type,
            false_type,
            comments,
        } = conditional;
        self.type_(check_type)?;
        for (_, infer, implied_bound) in infer_type_hoister::hoist_infer_types(extends_type) {
            self.add_type_param(
                &ast_visitor::TypeParamsContext::Infer,
                implied_bound,
                &infer.tparam,
            )?;
        }
        self.type_(extends_type)?;
        self.type_(true_type)?;
        self.type_(false_type)?;
        self.syntax_opt(comments.as_ref())
    }

    // The type parameter of an `infer` type is added by its conditional type.
    fn infer_type(
        &mut self,
        _infer: &ast::types::Infer<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        Ok(())
    }

    fn type_params(
        &mut self,
        kind: &ast_visitor::TypeParamsContext,
        tparams: &ast::types::TypeParams<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let tparams_maps = self.type_param_info(tparams);

        for (tparam, tparams_map) in tparams.params.iter().zip(tparams_maps) {
            let name_loc = tparam.name.loc.dupe();
            self.force_add_binding(
                EnvKey::ordinary(name_loc.dupe()),
                mk_reason(VirtualReasonDesc::RType(tparam.name.name.dupe()), name_loc),
                Def::TypeParam(Box::new(TypeParamData {
                    tparams_map,
                    kind: *kind,
                    implied_bound: ImpliedBound::Mixed,
                    tparam: (tparam.loc.dupe(), tparam.clone()),
                })),
            );
        }

        for tparam in tparams.params.iter() {
            self.add_tparam(tparam.name.loc.dupe(), tparam.name.name.dupe());
        }

        for tparam in tparams.params.iter() {
            ast_visitor::type_param_default(self, kind, tparam)?;
        }
        Ok(())
    }

    fn interface(
        &mut self,
        loc: &ALoc,
        interface: &ast::statement::Interface<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let name_loc = interface.id.loc.dupe();
        let tparams = &interface.tparams;
        let extends = &interface.extends;
        let body = &interface.body;
        // Force-register the binding so the synthetic [this] tparam (added below
        // at [name_loc]) always resolves. In declaration-merging cases like
        // [declare namespace X { type T = ... }] + [interface X { ... }] (TS
        // pattern, common in .d.ts files), the env_builder may not register an
        // AssigningWrite at [name_loc] for the interface, which would make the
        // conditional [add_ordinary_binding] a no-op and crash [init_type_param]
        // when it walks the interface's [tparams_locs] for any inner method.
        self.force_add_binding(
            EnvKey::ordinary(name_loc.dupe()),
            mk_reason(None, loc.dupe()),
            Def::Interface(loc.dupe(), interface.clone()),
        );

        self.in_new_tparams_env(false, |this| {
            // Walk tparams and extends without [this] in scope (matching the
            // lexical-scope rule: [extends Holder<this>] still errors). Then
            // add [this] to the tparams env so method bodies — including their
            // own type-parameter constraints/defaults — can reference it.
            if let Some(tparams) = tparams {
                this.type_params(&ast_visitor::TypeParamsContext::Interface, tparams)?;
            }
            for (_eloc, e) in extends.iter() {
                this.generic_type(e)?;
            }
            this.add_tparam(name_loc, FlowSmolStr::new("this"));
            let (_bloc, b) = body;
            this.object_type(b)?;
            Ok(())
        })
    }

    fn declare_module(
        &mut self,
        loc: &ALoc,
        m: &ast::statement::DeclareModule<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.in_scope(ScopeKind::DeclareModule, |this| {
            ast_visitor::declare_module_default(this, loc, m)?;
            Ok(())
        })?;
        Ok(())
    }

    fn declare_namespace(
        &mut self,
        loc: &ALoc,
        n: &ast::statement::DeclareNamespace<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let id = &n.id;
        match id {
            ast::statement::declare_namespace::Id::Global(_) => {}
            ast::statement::declare_namespace::Id::Local(ident) => {
                let name_loc = ident.loc.dupe();
                self.add_ordinary_binding(
                    name_loc.dupe(),
                    mk_reason(None, name_loc),
                    Def::DeclaredNamespace(loc.dupe(), n.clone()),
                );
            }
        }
        self.in_scope(ScopeKind::DeclareNamespace, |this| {
            ast_visitor::declare_namespace_default(this, loc, n)?;
            Ok(())
        })?;
        Ok(())
    }

    fn enum_declaration(
        &mut self,
        loc: &ALoc,
        enum_decl: &ast::statement::EnumDeclaration<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.enum_declaration_impl(false, loc, enum_decl)
    }

    // `declare enum` is ambient regardless of file extension.
    fn declare_enum(
        &mut self,
        loc: &ALoc,
        enum_decl: &ast::statement::EnumDeclaration<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.enum_declaration_impl(true, loc, enum_decl)
    }

    // The base mapper routes `declare export enum` through [enum_declaration], which
    // would mark it non-ambient; intercept it here so it is treated as ambient.
    fn declare_export_declaration_decl(
        &mut self,
        decl: &ast::statement::declare_export_declaration::Declaration<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        match decl {
            ast::statement::declare_export_declaration::Declaration::Enum { loc, declaration } => {
                self.enum_declaration_impl(true, loc, declaration)?;
            }
            _ => {
                ast_visitor::declare_export_declaration_decl_default(self, decl)?;
            }
        }
        Ok(())
    }

    fn import_declaration(
        &mut self,
        loc: &ALoc,
        decl: &ast::statement::ImportDeclaration<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let import_kind = decl.import_kind;
        let (source_loc, source_lit) = &decl.source;
        let source = &source_lit.value;
        let source_userland =
            flow_common::flow_import_specifier::Userland::from_smol_str(source.dupe());
        let specifiers = &decl.specifiers;
        let default = &decl.default;

        match specifiers {
            Some(ast::statement::import_declaration::Specifier::ImportNamedSpecifiers(
                specifiers,
            )) => {
                for spec in specifiers {
                    let rem_id_loc = spec.remote.loc.dupe();
                    let remote = &spec.remote.name;

                    let (id_loc, name) = match &spec.local {
                        Some(local_id) => (local_id.loc.dupe(), local_id.name.dupe()),
                        None => (rem_id_loc.dupe(), remote.dupe()),
                    };

                    self.add_ordinary_binding(
                        id_loc.dupe(),
                        mk_reason(
                            VirtualReasonDesc::RNamedImportedType(
                                source_userland.dupe(),
                                name.dupe(),
                            ),
                            rem_id_loc.dupe(),
                        ),
                        Def::Import(Box::new(ImportData {
                            import_kind,
                            source: source.dupe(),
                            source_loc: source_loc.dupe(),
                            import: Import::Named {
                                kind: spec.kind,
                                remote: remote.dupe(),
                                local: name.dupe(),
                            },
                        })),
                    );
                }
            }
            Some(ast::statement::import_declaration::Specifier::ImportNamespaceSpecifier((
                _,
                id,
            ))) => {
                let id_loc = id.loc.dupe();
                let name = &id.name;

                let import_reason_desc = match import_kind {
                    ast::statement::ImportKind::ImportType => {
                        VirtualReasonDesc::RImportStarType(name.dupe())
                    }
                    ast::statement::ImportKind::ImportTypeof => {
                        VirtualReasonDesc::RImportStarTypeOf(name.dupe())
                    }
                    ast::statement::ImportKind::ImportValue => {
                        VirtualReasonDesc::RImportStar(name.dupe())
                    }
                };

                self.add_ordinary_binding(
                    id_loc.dupe(),
                    mk_reason(import_reason_desc, id_loc.dupe()),
                    Def::Import(Box::new(ImportData {
                        import_kind,
                        source: source.dupe(),
                        source_loc: source_loc.dupe(),
                        import: Import::Namespace(name.dupe()),
                    })),
                );
            }
            None => {}
        }

        if let Some(default_spec) = default {
            let id_loc = default_spec.identifier.loc.dupe();
            let name = &default_spec.identifier.name;

            self.add_ordinary_binding(
                id_loc.dupe(),
                mk_reason(
                    VirtualReasonDesc::RDefaultImportedType(name.dupe(), source_userland.dupe()),
                    id_loc.dupe(),
                ),
                Def::Import(Box::new(ImportData {
                    import_kind,
                    source: source.dupe(),
                    source_loc: source_loc.dupe(),
                    import: Import::Default(name.dupe()),
                })),
            );
        }

        ast_visitor::import_declaration_default(self, loc, decl)
    }

    fn import_equals_declaration(
        &mut self,
        _loc: &ALoc,
        decl: &ast::statement::ImportEqualsDeclaration<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let id_loc = decl.id.loc.dupe();
        let local_name = &decl.id.name;
        let module_reference = &decl.module_reference;
        let import_kind = decl.import_kind;
        match module_reference {
            ast::statement::import_equals_declaration::ModuleReference::ExternalModuleReference(
                source_loc,
                source_lit,
            ) => {
                let source = &source_lit.value;
                let import_reason = mk_reason(
                    VirtualReasonDesc::RDefaultImportedType(
                        local_name.dupe(),
                        flow_common::flow_import_specifier::Userland::from_smol_str(source.dupe()),
                    ),
                    id_loc.dupe(),
                );
                self.add_ordinary_binding(
                    id_loc.dupe(),
                    import_reason,
                    Def::Import(Box::new(ImportData {
                        import_kind,
                        source: source.dupe(),
                        source_loc: source_loc.dupe(),
                        import: Import::Default(local_name.dupe()),
                    })),
                );
            }
            // Foo = A.B.C: qualified name access is not resolved through
            // name_def/env_resolution. The binding is handled by the type_sig layer.
            ast::statement::import_equals_declaration::ModuleReference::Identifier(_) => {}
        }
        Ok(())
    }

    fn call(
        &mut self,
        loc: &ALoc,
        _expr: &ast::expression::Call<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        Err(fail(
            loc.dupe(),
            "Should be visited by visit_call_expression",
        ))
    }

    fn optional_call(
        &mut self,
        loc: &ALoc,
        _expr: &ast::expression::OptionalCall<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        Err(fail(
            loc.dupe(),
            "Should be visited by visit_optional_call_expression",
        ))
    }

    fn new(
        &mut self,
        loc: &ALoc,
        _expr: &ast::expression::New<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        Err(fail(
            loc.dupe(),
            "Should be visited by visit_new_expression",
        ))
    }

    fn member(
        &mut self,
        loc: &ALoc,
        _expr: &ast::expression::Member<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        Err(fail(
            loc.dupe(),
            "Should be visited by visit_member_expression",
        ))
    }

    fn member_property_expression(
        &mut self,
        expr: &ast::expression::Expression<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.visit_expression(EnclosingContext::IndexContext, &Vec::new(), expr)?;
        Ok(())
    }

    fn optional_member(
        &mut self,
        loc: &ALoc,
        _expr: &ast::expression::OptionalMember<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        Err(fail(
            loc.dupe(),
            "Should be visited by visit_optional_member_expression",
        ))
    }

    fn type_cast(
        &mut self,
        _loc: &ALoc,
        expr: &ast::expression::TypeCast<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.cast(&expr.annot, &expr.expression)?;
        Ok(())
    }

    fn as_expression(
        &mut self,
        _loc: &ALoc,
        expr: &ast::expression::AsExpression<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.cast(&expr.annot, &expr.expression)?;
        Ok(())
    }

    fn ts_satisfies(
        &mut self,
        _loc: &ALoc,
        expr: &ast::expression::TSSatisfies<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        self.cast(&expr.annot, &expr.expression)?;
        Ok(())
    }

    fn unary_expression(
        &mut self,
        loc: &ALoc,
        _expr: &ast::expression::Unary<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        Err(fail(
            loc.dupe(),
            "Should be visited by visit_unary_expression",
        ))
    }

    fn jsx_element(
        &mut self,
        loc: &ALoc,
        expr: &ast::jsx::Element<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let this = &mut *self;
        let loc_element = loc.dupe();
        use flow_common::hint::JsxImplicitInstantiationHints;

        let opening_name = &expr.opening_element.name;
        let targs = &expr.opening_element.targs;
        let opening_attributes = &expr.opening_element.attributes;
        let children = &expr.children;

        let hints: AstHints = if this.react_jsx {
            match opening_name {
                ast::jsx::Name::Identifier(id) => {
                    let name = &id.name;
                    if name.as_str() == "fbs" || name.as_str() == "fbt" {
                        vec![]
                    } else if name
                        .chars()
                        .next()
                        .is_some_and(|c| c == c.to_ascii_uppercase())
                    {
                        // Capitalized: treat as component
                        vec![Hint::HintT(
                            HintNode::ValueHint(
                                EnclosingContext::JsxTitleNameContext,
                                Expression::new(ast::expression::ExpressionInner::Identifier {
                                    loc: id.loc.dupe(),
                                    inner: ast::Identifier::new(IdentifierInner {
                                        loc: id.loc.dupe(),
                                        name: name.dupe(),
                                        comments: id.comments.dupe(),
                                    }),
                                }),
                            ),
                            HintKind::ExpectedTypeHint,
                        )]
                    } else {
                        // Lowercase: string literal type
                        vec![Hint::HintT(
                            HintNode::StringLiteralType(name.dupe()),
                            HintKind::ExpectedTypeHint,
                        )]
                    }
                }
                ast::jsx::Name::NamespacedName(_) => vec![],
                ast::jsx::Name::MemberExpression(member) => {
                    fn jsx_title_member_to_expression(
                        member: &ast::jsx::MemberExpression<ALoc, ALoc>,
                    ) -> ast::expression::Expression<ALoc, ALoc> {
                        let object = match &member.object {
                            ast::jsx::member_expression::Object::MemberExpression(inner_member) => {
                                jsx_title_member_to_expression(inner_member)
                            }
                            ast::jsx::member_expression::Object::Identifier(id) => {
                                if id.name.as_str() == "this" {
                                    Expression::new(ast::expression::ExpressionInner::This {
                                        loc: id.loc.dupe(),
                                        inner: Arc::new(ast::expression::This {
                                            comments: id.comments.dupe(),
                                        }),
                                    })
                                } else {
                                    Expression::new(ast::expression::ExpressionInner::Identifier {
                                        loc: id.loc.dupe(),
                                        inner: ast::Identifier::new(IdentifierInner {
                                            loc: id.loc.dupe(),
                                            name: id.name.dupe(),
                                            comments: id.comments.dupe(),
                                        }),
                                    })
                                }
                            }
                        };
                        let property = ast::Identifier::new(IdentifierInner {
                            loc: member.property.loc.dupe(),
                            name: member.property.name.dupe(),
                            comments: member.property.comments.dupe(),
                        });
                        Expression::new(ast::expression::ExpressionInner::Member {
                            loc: member.loc.dupe(),
                            inner: Arc::new(ast::expression::Member {
                                object,
                                property: ast::expression::member::Property::PropertyIdentifier(
                                    property,
                                ),
                                comments: None,
                            }),
                        })
                    }
                    let expr = jsx_title_member_to_expression(member);
                    vec![Hint::HintT(
                        HintNode::ValueHint(EnclosingContext::NoContext, expr),
                        HintKind::ExpectedTypeHint,
                    )]
                }
            }
        } else {
            vec![]
        };

        fn jsx_title_member_to_string(member: &ast::jsx::MemberExpression<ALoc, ALoc>) -> String {
            let property_name = member.property.name.as_str();
            match &member.object {
                ast::jsx::member_expression::Object::MemberExpression(inner_member) => {
                    format!(
                        "{}.{}",
                        jsx_title_member_to_string(inner_member),
                        property_name
                    )
                }
                ast::jsx::member_expression::Object::Identifier(id) => {
                    format!("{}.{}", id.name.as_str(), property_name)
                }
            }
        }

        fn jsx_title_namespaced_name_to_string(
            ns: &ast::jsx::NamespacedName<ALoc, ALoc>,
        ) -> String {
            format!("{}{}", ns.namespace.name.as_str(), ns.name.name.as_str())
        }

        let jsx_name: FlowSmolStr = match opening_name {
            ast::jsx::Name::Identifier(id) => id.name.dupe(),
            ast::jsx::Name::MemberExpression(member) => jsx_title_member_to_string(member).into(),
            ast::jsx::Name::NamespacedName(ns) => jsx_title_namespaced_name_to_string(ns).into(),
        };

        let jsx_reason = mk_reason(None, loc_element.dupe());
        let targs_for_hints = targs.clone();
        let jsx_props_and_children = (opening_attributes.clone(), children.clone());
        let instantiate_decomp = HintDecomposition::new(
            HintDecompositionInner::InstantiateComponent(JsxImplicitInstantiationHints {
                jsx_reason,
                jsx_name: jsx_name.dupe(),
                jsx_targs: std::rc::Rc::new(flow_lazy::Lazy::new_forced(targs_for_hints)),
                jsx_props_and_children,
                jsx_hints: std::rc::Rc::new(flow_lazy::Lazy::new_forced(Vec::new())),
            }),
        );
        let hints = Hint::decompose(instantiate_decomp, hints);

        let decomp_jsx_props = HintDecomposition::new(HintDecompositionInner::DecompJsxProps);
        let hints = Hint::decompose(decomp_jsx_props, hints);

        let has_autocomplete = opening_attributes.iter().any(|attr| match attr {
            ast::jsx::OpeningAttribute::Attribute(a) => {
                a.value.as_ref().is_some_and(|value| match value {
                    ast::jsx::attribute::Value::StringLiteral((loc, _)) => {
                        literal_has_autocomplete(this.autocomplete_hooks, loc)
                    }
                    ast::jsx::attribute::Value::ExpressionContainer((_, container)) => {
                        match &container.expression {
                            ast::jsx::expression_container::Expression::EmptyExpression => false,
                            ast::jsx::expression_container::Expression::Expression(e) => {
                                expression_has_autocomplete(this.autocomplete_hooks, e)
                            }
                        }
                    }
                })
            }
            ast::jsx::OpeningAttribute::SpreadAttribute(spread) => {
                expression_has_autocomplete(this.autocomplete_hooks, &spread.argument)
            }
        });

        let hints = if has_autocomplete {
            hints
        } else {
            let checks =
                crate::eq_test::jsx_attributes_possible_sentinel_refinements(opening_attributes);
            let decomp =
                HintDecomposition::new(HintDecompositionInner::DecompSentinelRefinement(checks));
            Hint::decompose(decomp, hints)
        };

        for attr in opening_attributes.iter() {
            match attr {
                ast::jsx::OpeningAttribute::Attribute(a) => {
                    let (attr_hints, name_loc): (AstHints, Option<ALoc>) = match &a.name {
                        ast::jsx::attribute::Name::Identifier(id) => {
                            let decomp = HintDecomposition::new(
                                HintDecompositionInner::DecompObjProp(id.name.dupe()),
                            );
                            (Hint::decompose(decomp, hints.clone()), Some(id.loc.dupe()))
                        }
                        ast::jsx::attribute::Name::NamespacedName(_) => (vec![], None),
                    };
                    if let Some(loc) = name_loc {
                        this.record_hint(loc, attr_hints.clone());
                    }
                    if let Some(value) = &a.value {
                        match value {
                            ast::jsx::attribute::Value::StringLiteral((loc, _)) => {
                                this.record_hint(loc.dupe(), attr_hints);
                            }
                            ast::jsx::attribute::Value::ExpressionContainer((_, container)) => {
                                this.visit_jsx_expression(&attr_hints, container)?;
                            }
                        }
                    }
                }
                ast::jsx::OpeningAttribute::SpreadAttribute(spread) => {
                    let decomp = HintDecomposition::new(HintDecompositionInner::DecompObjSpread);
                    let spread_hints = Hint::decompose(decomp, hints.clone());
                    this.visit_expression(
                        EnclosingContext::NoContext,
                        &spread_hints,
                        &spread.argument,
                    )?;
                }
            }
        }

        let children_decomp =
            HintDecomposition::new(HintDecompositionInner::DecompObjProp("children".into()));
        let children_hints = Hint::decompose(children_decomp, hints);
        this.visit_jsx_children(&children_hints, &children.1)?;
        Ok(())
    }

    fn jsx_fragment(
        &mut self,
        loc: &ALoc,
        expr: &ast::jsx::Fragment<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let this = &mut *self;
        let _loc = loc.dupe();
        let frag_children = &expr.frag_children;

        let base_hints: AstHints = vec![Hint::HintT(
            HintNode::ReactFragmentType,
            HintKind::ExpectedTypeHint,
        )];

        let func_param_decomp =
            HintDecomposition::new(HintDecompositionInner::DecompFuncParam(vec![None], 0, None));
        let hints = Hint::decompose(func_param_decomp, base_hints);

        let children_decomp =
            HintDecomposition::new(HintDecompositionInner::DecompObjProp("children".into()));
        let hints = Hint::decompose(children_decomp, hints);

        this.visit_jsx_children(&hints, &frag_children.1)?;
        Ok(())
    }

    fn expression(
        &mut self,
        expr: &ast::expression::Expression<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        use flow_common::enclosing_context::EnclosingContext;
        self.visit_expression(EnclosingContext::NoContext, &vec![], expr)?;
        Ok(())
    }

    fn array(
        &mut self,
        loc: &ALoc,
        _expr: &ast::expression::Array<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        Err(fail(
            loc.dupe(),
            "Should be visited by visit_array_expression",
        ))
    }

    fn conditional(
        &mut self,
        loc: &ALoc,
        _expr: &ast::expression::Conditional<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        Err(fail(loc.dupe(), "Should be visited by visit_conditional"))
    }

    fn binary(
        &mut self,
        loc: &ALoc,
        _expr: &ast::expression::Binary<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        Err(fail(
            loc.dupe(),
            "Should be visited by visit_binary_expression",
        ))
    }

    fn logical(
        &mut self,
        loc: &ALoc,
        _expr: &ast::expression::Logical<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        Err(fail(
            loc.dupe(),
            "Should be visited by visit_logical_expression",
        ))
    }

    fn object(
        &mut self,
        loc: &ALoc,
        _expr: &ast::expression::Object<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        Err(fail(
            loc.dupe(),
            "Should be visited by visit_object_expression",
        ))
    }

    fn switch(
        &mut self,
        loc: &ALoc,
        stmt: &ast::statement::Switch<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let discriminant = &stmt.discriminant;
        let cases = &stmt.cases;

        ast_visitor::switch_default(self, loc, stmt)?;
        // Overwrite the (probably empty) hints on the case expressions recorded by super#switch
        for case in cases.iter() {
            if let Some(test) = &case.test {
                let test_loc = test.loc().dupe();
                self.record_hint(
                    test_loc,
                    vec![Hint::HintT(
                        HintNode::ValueHint(EnclosingContext::NoContext, discriminant.clone()),
                        HintKind::ExpectedTypeHint,
                    )],
                );
            }
        }
        Ok(())
    }

    fn switch_case(
        &mut self,
        case: &ast::statement::switch::Case<ALoc, ALoc>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        if let Some(test) = &case.test {
            self.visit_expression(EnclosingContext::OtherTestContext, &vec![], test)?;
        }
        self.statement_list(&case.consequent)
    }

    fn match_expression(
        &mut self,
        loc: &ALoc,
        _expr: &ast::match_::Match<ALoc, ALoc, ast::expression::Expression<ALoc, ALoc>>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        Err(fail(
            loc.dupe(),
            "Should be visited by visit_match_expression",
        ))
    }

    fn match_statement(
        &mut self,
        _loc: &ALoc,
        x: &ast::match_::Match<ALoc, ALoc, ast::statement::Statement<ALoc, ALoc>>,
    ) -> Result<(), EnvInvariant<ALoc>> {
        let ast::match_::Match {
            arg,
            cases,
            match_keyword_loc,
            comments: _,
        } = x;
        self.visit_expression(EnclosingContext::NoContext, &Vec::new(), arg)?;
        self.add_ordinary_binding_with_kind(
            match_keyword_loc.dupe(),
            mk_reason(None, match_keyword_loc.dupe()),
            DefinitionReferenceKind::Match,
            Def::Binding(Box::new(Binding::Root(mk_value(
                None,
                None,
                true,
                arg.clone(),
            )))),
        );
        let mut prev_pattern_loc: Option<ALoc> = None;
        for case in cases.iter() {
            let ast::match_::Case {
                loc: _,
                pattern,
                body,
                guard,
                case_match_root_loc,
                comments: _,
                invalid_syntax: _,
            } = case;
            let acc = Root::MatchCaseRoot(Box::new(MatchCaseRootData {
                case_match_root_loc: case_match_root_loc.dupe(),
                root_pattern_loc: pattern.loc().dupe(),
                prev_pattern_loc: prev_pattern_loc.dupe(),
            }));
            self.add_match_destructure_bindings(
                case_match_root_loc.dupe(),
                guard.is_some(),
                prev_pattern_loc.dupe(),
                acc,
                pattern,
            )?;
            self.match_pattern(pattern)?;
            if let Some(guard_expr) = guard {
                self.visit_expression(EnclosingContext::OtherTestContext, &Vec::new(), guard_expr)?;
            }
            self.statement(body)?;
            prev_pattern_loc = Some(pattern.loc().dupe());
        }
        Ok(())
    }
}

pub fn find_defs(
    autocomplete_hooks: &AutocompleteHooks<'_, ALoc>,
    react_jsx: bool,
    assertion_calls: BTreeMap<ALoc, AssertionInfo>,
    env_info: &EnvInfo<ALoc>,
    toplevel_scope_kind: ScopeKind,
    ast: &ast::Program<ALoc, ALoc>,
) -> Result<(EnvEntriesMap, HintMap), EnvInvariant<ALoc>> {
    let mut finder = DefFinder::new(
        autocomplete_hooks,
        react_jsx,
        assertion_calls,
        env_info,
        toplevel_scope_kind,
    );
    finder.program(ast)?;
    Ok(finder.into_acc())
}
