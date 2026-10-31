/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;

use dupe::Dupe;
use flow_typing_context::Context;
use flow_typing_flow_common::concrete_type_eq;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::Literal;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::empty_t;

use super::Normalizer;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScalarDomain {
    String,
    Number,
    BigInt,
    Boolean,
}

fn scalar_domain(t: &Type) -> Option<(ScalarDomain, bool)> {
    match t.deref() {
        TypeInner::DefT(_, def_t) => match def_t.deref() {
            DefTInner::StrGeneralT(Literal::AnyLiteral) => Some((ScalarDomain::String, false)),
            DefTInner::SingletonStrT { .. } => Some((ScalarDomain::String, true)),
            DefTInner::NumGeneralT(Literal::AnyLiteral) => Some((ScalarDomain::Number, false)),
            DefTInner::SingletonNumT { .. } => Some((ScalarDomain::Number, true)),
            DefTInner::BigIntGeneralT(Literal::AnyLiteral) => Some((ScalarDomain::BigInt, false)),
            DefTInner::SingletonBigIntT { .. } => Some((ScalarDomain::BigInt, true)),
            DefTInner::BoolGeneralT => Some((ScalarDomain::Boolean, false)),
            DefTInner::SingletonBoolT { .. } => Some((ScalarDomain::Boolean, true)),
            _ => None,
        },
        _ => None,
    }
}

/// A singleton absorbs the general type of its domain (`string & 'a'` is
/// `'a'`). Two domains or two distinct singletons share no value, so the result
/// is `empty`, even if a later member is not a scalar.
pub(super) fn normalize<'cx>(
    cx: &mut Context<'cx>,
    normalizer: &Normalizer<'_>,
    members: &[Type],
) -> Option<Type> {
    let mut domain_seen = None;
    let mut singleton = None;
    for member in members {
        let (domain, is_singleton) = scalar_domain(member)?;
        if domain_seen.is_some_and(|seen| seen != domain) {
            return Some(empty_t::make(normalizer.reason.dupe()));
        }
        domain_seen = Some(domain);
        if is_singleton {
            if singleton.as_ref().is_some_and(|previous| {
                !concrete_type_eq::eq_with_env(cx, normalizer.env, previous, member)
            }) {
                return Some(empty_t::make(normalizer.reason.dupe()));
            }
            singleton = Some(member.dupe());
        }
    }
    singleton.or_else(|| members.first().map(Dupe::dupe))
}
