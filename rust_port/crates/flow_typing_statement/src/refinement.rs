/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Type refinements on expressions - wraps Env API
//! if expression is syntactically eligible for type refinement,
//! return Some (access key), otherwise None.
//! Eligible expressions are simple ids and chains of property|index
//! lookups from an id base

use std::ops::Deref;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::js_number::numeric_property_name;
use flow_parser::ast::expression::Expression;
use flow_parser::ast::expression::ExpressionInner;
use flow_parser::ast::expression::member;
use flow_typing_context::Context;
use flow_typing_key::Base;
use flow_typing_key::Key;
use flow_typing_key::Proj;
use flow_typing_type::type_::Type;
use flow_typing_utils::type_env;

pub mod keys {
    use super::*;

    pub fn key<M: Dupe, T: Dupe>(allow_optional: bool, expr: &Expression<M, T>) -> Option<Key> {
        match expr.deref() {
            // treat this as a property chain, in terms of refinement lifetime
            ExpressionInner::This { .. } => Some(Key(Base::This, vec![])),
            // treat this as a property chain, in terms of refinement lifetime
            ExpressionInner::Super { .. } => Some(Key(Base::Super, vec![])),
            ExpressionInner::Identifier { inner, .. } => key_of_identifier(inner),
            ExpressionInner::OptionalMember { inner, .. } if allow_optional => {
                key_of_member(allow_optional, &inner.member)
            }
            ExpressionInner::Member { inner, .. } => key_of_member(allow_optional, inner),
            // other LHSes unsupported currently/here
            _ => None,
        }
    }

    fn key_of_identifier<M: Dupe, T: Dupe>(id: &flow_parser::ast::Identifier<M, T>) -> Option<Key> {
        let name = &id.name;
        if name.as_str() == "undefined" {
            None
        } else {
            Some(Key(Base::OrdinaryIdentifier(name.dupe()), vec![]))
        }
    }

    fn key_of_member<M: Dupe, T: Dupe>(
        allow_optional: bool,
        member: &flow_parser::ast::expression::Member<M, T>,
    ) -> Option<Key> {
        let object = &member.object;
        let property = &member.property;

        match property {
            member::Property::PropertyIdentifier(id) => {
                let name = id.name.dupe();
                match key(allow_optional, object) {
                    Some(Key(base, mut chain)) => {
                        chain.push(Proj::Prop(name));
                        Some(Key(base, chain))
                    }
                    None => None,
                }
            }
            member::Property::PropertyExpression(prop_expr) => {
                match prop_expr.deref() {
                    ExpressionInner::StringLiteral { inner, .. } => {
                        let name = inner.value.dupe();
                        match key(allow_optional, object) {
                            Some(Key(base, mut chain)) => {
                                chain.push(Proj::Prop(name));
                                Some(Key(base, chain))
                            }
                            None => None,
                        }
                    }
                    ExpressionInner::NumberLiteral { inner, .. } => {
                        if let Some(name) = numeric_property_name(inner.value) {
                            match key(allow_optional, object) {
                                Some(Key(base, mut chain)) => {
                                    chain.push(Proj::Prop(name));
                                    Some(Key(base, chain))
                                }
                                None => None,
                            }
                        } else {
                            None
                        }
                    }
                    _ => {
                        match key(allow_optional, object) {
                            // foo.bar[baz] -> Chain [Index baz; Id bar; Id foo]
                            Some(Key(base, mut chain)) => match key(allow_optional, prop_expr) {
                                Some(k) => {
                                    chain.push(Proj::Elem(Box::new(k)));
                                    Some(Key(base, chain))
                                }
                                None => None,
                            },
                            None => None,
                        }
                    }
                }
            }
            member::Property::PropertyPrivateName(pn) => {
                let name = pn.name.dupe();
                match key(allow_optional, object) {
                    Some(Key(base, mut chain)) => {
                        chain.push(Proj::PrivateField(name));
                        Some(Key(base, chain))
                    }
                    None => None,
                }
            }
        }
    }
}

/// get type refinement for expression, if it exists
pub fn get<'a, M: Dupe, T: Dupe>(
    allow_optional: bool,
    cx: &mut Context<'a>,
    expr: &Expression<M, T>,
    loc: ALoc,
) -> Result<Option<Type>, flow_utils_concurrency::job_error::JobError> {
    match keys::key(allow_optional, expr) {
        Some(k) => type_env::get_refinement(cx, k.reason_desc(), loc),
        None => Ok(None),
    }
}
