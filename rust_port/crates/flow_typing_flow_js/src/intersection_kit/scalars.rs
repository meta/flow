/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;

use dupe::Dupe;
use flow_typing_context::Context;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::Literal;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::empty_t;
use flow_typing_type::type_util;

use super::Normalizer;
use super::disjoint;

fn is_scalar(t: &Type) -> bool {
    match t.deref() {
        TypeInner::DefT(_, def_t) => matches!(
            def_t.deref(),
            DefTInner::StrGeneralT(Literal::AnyLiteral)
                | DefTInner::SingletonStrT { .. }
                | DefTInner::NumGeneralT(Literal::AnyLiteral)
                | DefTInner::SingletonNumT { .. }
                | DefTInner::BigIntGeneralT(Literal::AnyLiteral)
                | DefTInner::SingletonBigIntT { .. }
                | DefTInner::BoolGeneralT
                | DefTInner::SingletonBoolT { .. }
        ),
        _ => false,
    }
}

/// Scalar members with no value in common, such as two domains or two distinct
/// singletons, make the result `empty`, even if other members are not scalars.
/// Otherwise a singleton absorbs the general type of its domain (`string & 'a'`
/// is `'a'`).
pub(super) fn normalize<'cx>(
    cx: &mut Context<'cx>,
    normalizer: &Normalizer<'_>,
    members: &[Type],
) -> Option<Type> {
    let scalars: Vec<&Type> = members.iter().filter(|t| is_scalar(t)).collect();
    let has_disjoint_scalars = scalars.iter().enumerate().any(|(i, t1)| {
        scalars[i + 1..]
            .iter()
            .any(|t2| disjoint::tags_differ(cx, t1, t2) || disjoint::ground_types_differ(t1, t2))
    });
    if has_disjoint_scalars {
        return Some(empty_t::make(normalizer.reason.dupe()));
    }
    if scalars.len() < members.len() {
        return None;
    }
    scalars
        .iter()
        .find(|t1| {
            scalars
                .iter()
                .all(|t2| type_util::quick_subtype(None::<&mut fn(&Type)>, t1, t2))
        })
        .map(|t| (*t).dupe())
}
