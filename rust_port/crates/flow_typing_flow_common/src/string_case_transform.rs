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
use flow_common::reason;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_typing_context::Context;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::StringMappingKind;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::union_rep;
use flow_typing_type::type_util;

use crate::flow_js_utils::FlowJsException;

/// Concretization is a `flow_js` operation, so the caller closes over the
/// [`Context`] and the `flow_js` env it should run under.
pub type PossibleConcreteTypesForInspection<'a> =
    dyn Fn(&mut Context<'_>, &Reason, &Type) -> Result<Vec<Type>, FlowJsException> + 'a;

pub fn kind_of_name(s: &str) -> Option<StringMappingKind> {
    match s {
        "Uppercase" => Some(StringMappingKind::StringMappingUppercase),
        "Lowercase" => Some(StringMappingKind::StringMappingLowercase),
        "Capitalize" => Some(StringMappingKind::StringMappingCapitalize),
        "Uncapitalize" => Some(StringMappingKind::StringMappingUncapitalize),
        _ => None,
    }
}

pub fn name_of_kind(kind: StringMappingKind) -> &'static str {
    match kind {
        StringMappingKind::StringMappingUppercase => "Uppercase",
        StringMappingKind::StringMappingLowercase => "Lowercase",
        StringMappingKind::StringMappingCapitalize => "Capitalize",
        StringMappingKind::StringMappingUncapitalize => "Uncapitalize",
    }
}

// ASCII-only casing. We do not match tsc's full Unicode `String.prototype.toUpperCase`
//    semantics here (e.g. tsc returns `'STRASSE'` for `Uppercase<'straße'>` because JS
//    expands `ß` to `SS`), since OCaml's stdlib has no Unicode case-folding table.
//    ASCII-only is a deliberate, soundness-preserving choice for v1: tests that
//    exercise non-ASCII input cannot rely on a particular result.
pub fn transform_string(kind: StringMappingKind, s: &str) -> String {
    // ASCII-only by design. OCaml uses byte-indexed string access and slicing,
    // which is safe on non-ASCII because OCaml strings are byte sequences and
    // `Char.uppercase_ascii` returns non-ASCII bytes unchanged. Rust `&str` must
    // be valid UTF-8, so we step by `char` (which is a UTF-8-boundary-safe
    // operation) instead. The observable result still matches OCaml:
    // `char::to_ascii_uppercase`/`to_ascii_lowercase` only rewrite ASCII
    // letters, so a non-ASCII leading character is preserved verbatim.
    match kind {
        StringMappingKind::StringMappingUppercase => s.to_ascii_uppercase(),
        StringMappingKind::StringMappingLowercase => s.to_ascii_lowercase(),
        StringMappingKind::StringMappingCapitalize => {
            let mut chars = s.chars();
            match chars.next() {
                None => s.to_string(),
                Some(first) => {
                    let head = first.to_ascii_uppercase();
                    let rest = chars.as_str();
                    let mut out = String::with_capacity(s.len());
                    out.push(head);
                    out.push_str(rest);
                    out
                }
            }
        }
        StringMappingKind::StringMappingUncapitalize => {
            let mut chars = s.chars();
            match chars.next() {
                None => s.to_string(),
                Some(first) => {
                    let head = first.to_ascii_lowercase();
                    let rest = chars.as_str();
                    let mut out = String::with_capacity(s.len());
                    out.push(head);
                    out.push_str(rest);
                    out
                }
            }
        }
    }
}

// Predicate matching `transform_string(kind, s) == s`. Used in subtyping to test
//    whether a literal `s` is a member of `Kind<arg>` — the literal must already
//    be in canonical form for the kind.
pub fn is_canonical(kind: StringMappingKind, s: &str) -> bool {
    transform_string(kind, s) == s
}

// Like [is_canonical] but the empty string is NEVER canonical for cap/uncap.
//    Used by `template_literal_type::lexical_validator_of` to reject an empty
//    placeholder match for `Capitalize`/`Uncapitalize`: in placeholder context,
//    an empty match means the leading character comes from a neighboring quasi
//    or interpolation, so the cap/uncap constraint must not "discharge" on the
//    empty match.
pub fn is_canonical_for_placeholder(kind: StringMappingKind, s: &str) -> bool {
    match kind {
        StringMappingKind::StringMappingUppercase | StringMappingKind::StringMappingLowercase => {
            is_canonical(kind, s)
        }
        StringMappingKind::StringMappingCapitalize
        | StringMappingKind::StringMappingUncapitalize => !s.is_empty() && is_canonical(kind, s),
    }
}

// Apply Capitalize/Uncapitalize quasi rewriting. Uppercase/Lowercase touch
//    every quasi; the cap/uncap kinds only affect the very first character of the
//    produced string, which is the first character of quasis[0] — _unless_
//    quasis[0] is empty, in which case the leading character lives in an
//    interpolated type and the caller must push the transform into types[0]
//    instead.
pub fn transform_quasis(kind: StringMappingKind, quasis: Vec<FlowSmolStr>) -> Vec<FlowSmolStr> {
    match kind {
        StringMappingKind::StringMappingUppercase => quasis
            .into_iter()
            .map(|q| FlowSmolStr::from(q.to_ascii_uppercase()))
            .collect(),
        StringMappingKind::StringMappingLowercase => quasis
            .into_iter()
            .map(|q| FlowSmolStr::from(q.to_ascii_lowercase()))
            .collect(),
        StringMappingKind::StringMappingCapitalize
        | StringMappingKind::StringMappingUncapitalize => match quasis.split_first() {
            None => vec![],
            Some((first, rest)) => {
                let first_prime = transform_string(kind, first.as_str());
                let mut out = Vec::with_capacity(rest.len() + 1);
                out.push(FlowSmolStr::from(first_prime));
                out.extend(rest.iter().cloned());
                out
            }
        },
    }
}

fn mk_singleton_str(loc: ALoc, s: String) -> Type {
    let r = reason::mk_annot_reason(VirtualReasonDesc::RStringLit(s.as_str().into()), loc);
    Type::new(TypeInner::DefT(
        r,
        DefT::new(DefTInner::SingletonStrT {
            from_annot: true,
            value: s.as_str().into(),
        }),
    ))
}

fn mk_deferred(reason: Reason, kind: StringMappingKind, arg: Type) -> Type {
    Type::new(TypeInner::StringMappingT { reason, kind, arg })
}

// Conservative predicate: can `t`, viewed as a template literal placeholder,
//    stringify to the empty string? `string` and `''` can; `number`, `bigint`,
//    `boolean`, `null`, `undefined`, and any non-empty string literal cannot.
//    Unions: yes iff any member can. Generics: defer to their bound. Anything
//    we can't analyze (opens, evals, etc.) is treated as "yes" — that's the
//    safe direction for soundness (we'll widen the result type, never narrow).
fn can_be_empty_string(t: &Type) -> bool {
    match t.deref() {
        TypeInner::DefT(_, def) => match def.deref() {
            DefTInner::StrGeneralT(_) => true,
            DefTInner::SingletonStrT { value, .. } => value.is_empty(),
            DefTInner::NumGeneralT(_)
            | DefTInner::SingletonNumT { .. }
            | DefTInner::NumericStrKeyT { .. }
            | DefTInner::BigIntGeneralT(_)
            | DefTInner::SingletonBigIntT { .. }
            | DefTInner::BoolGeneralT
            | DefTInner::SingletonBoolT { .. }
            | DefTInner::NullT
            | DefTInner::VoidT
            | DefTInner::TsVoidT => false,
            _ => true,
        },
        TypeInner::UnionT(_, rep) => rep.members_iter().any(can_be_empty_string),
        TypeInner::GenericT(data) => can_be_empty_string(&data.bound),
        _ => true,
    }
}

// Concretizing variant. When [possible_concrete_types_for_inspection] is
//    provided, resolve indirection (`EvalT` for indexed/property access, `OpenT`,
//    `AnnotT`, `TypeAppT`, ...) before applying the syntactic predicate, so that
//    e.g. `['Hello'][0]` is recognized as the non-empty literal it is rather than
//    falling through to the conservative `true` fallback. Without it (e.g. from
//    `type_sig_merge`, which deliberately blocks Flow_js), the syntactic check
//    stands. On concretization error, fall back to the syntactic check so this
//    function stays infallible (matches OCaml).
fn can_be_empty_string_concretized(
    possible_concrete_types_for_inspection: Option<&PossibleConcreteTypesForInspection<'_>>,
    cx: &mut Context<'_>,
    t: &Type,
) -> bool {
    match possible_concrete_types_for_inspection {
        None => can_be_empty_string(t),
        Some(f) => match f(cx, type_util::reason_of_t(t), t) {
            Ok(ts) => ts.iter().any(can_be_empty_string),
            Err(_) => can_be_empty_string(t),
        },
    }
}

// Build the result type for `Kind<arg>`. Eager when `arg` is a shape whose
//    strings we can enumerate; deferred (a `StringMappingT`) otherwise so the
//    dependency on a generic / `string` / unresolved EvalT / etc. is preserved
//    through substitution and surfaces in subtyping. The subtyping rule for
//    `_ <: StringMappingT` concretizes `arg` lazily before checking, which is
//    where opaque shapes (EvalT for indexed/property access, OpenT, AnnotT)
//    get a chance to reduce to a literal.
pub fn resolve<'cx>(
    possible_concrete_types_for_inspection: Option<&PossibleConcreteTypesForInspection<'_>>,
    cx: &mut Context<'cx>,
    kind: StringMappingKind,
    loc: ALoc,
    arg: Type,
) -> Type {
    let result_reason = reason::mk_annot_reason(
        VirtualReasonDesc::RType(name_of_kind(kind).into()),
        loc.dupe(),
    );
    match arg.deref() {
        TypeInner::DefT(_, def) => match def.deref() {
            DefTInner::SingletonStrT { value, .. } => {
                mk_singleton_str(loc, transform_string(kind, value.as_str()))
            }
            DefTInner::EmptyT => {
                let r = reason::mk_annot_reason(None, loc);
                Type::new(TypeInner::DefT(r, DefT::new(DefTInner::EmptyT)))
            }
            _ => mk_deferred(result_reason, kind, arg),
        },
        TypeInner::UnionT(_, rep) => {
            let transformed: Vec<Type> = rep
                .members_iter()
                .cloned()
                .map(|m| {
                    resolve(
                        possible_concrete_types_for_inspection,
                        cx,
                        kind,
                        loc.dupe(),
                        m,
                    )
                })
                .collect();
            match transformed.as_slice() {
                [] => {
                    let r = reason::mk_annot_reason(None, loc);
                    Type::new(TypeInner::DefT(r, DefT::new(DefTInner::EmptyT)))
                }
                [t] => t.dupe(),
                [t0, t1, rest @ ..] => {
                    let rest_rc: Rc<[Type]> = rest.iter().cloned().collect();
                    Type::new(TypeInner::UnionT(
                        result_reason,
                        union_rep::make(
                            Some(cx.make_aloc_id(&loc)),
                            union_rep::UnionKind::UnknownKind,
                            t0.dupe(),
                            t1.dupe(),
                            rest_rc,
                        ),
                    ))
                }
            }
        }
        TypeInner::TemplateLiteralT { quasis, types, .. } => {
            let r = reason::mk_annot_reason(None, loc.dupe());
            match kind {
                StringMappingKind::StringMappingUppercase
                | StringMappingKind::StringMappingLowercase => {
                    let quasis_prime = transform_quasis(kind, quasis.clone());
                    let types_prime: Vec<Type> = types
                        .iter()
                        .map(|t| {
                            resolve(
                                possible_concrete_types_for_inspection,
                                cx,
                                kind,
                                loc.dupe(),
                                t.dupe(),
                            )
                        })
                        .collect();
                    Type::new(TypeInner::TemplateLiteralT {
                        reason: r,
                        from_annot: true,
                        quasis: quasis_prime,
                        types: types_prime,
                    })
                }
                StringMappingKind::StringMappingCapitalize
                | StringMappingKind::StringMappingUncapitalize => {
                    // Only the leading character of the produced string changes. That char
                    //    lives in quasis[0] when quasis[0] is non-empty; otherwise it's the
                    //    leading char of types[0] when types[0] is non-empty at runtime, OR
                    //    the leading char of quasis[1] when types[0] stringifies to "". When
                    //    types[0] can be the empty string, we must produce a union of both
                    //    arms so subtyping enforces the right thing on either runtime value.
                    match (quasis.split_first(), types.split_first()) {
                        (Some((first, _)), _) if !first.as_str().is_empty() => {
                            let quasis_prime = transform_quasis(kind, quasis.clone());
                            Type::new(TypeInner::TemplateLiteralT {
                                reason: r,
                                from_annot: true,
                                quasis: quasis_prime,
                                types: types.clone(),
                            })
                        }
                        (Some((_, rest_quasis)), Some((t0, rest_types)))
                            if can_be_empty_string_concretized(
                                possible_concrete_types_for_inspection,
                                cx,
                                t0,
                            ) =>
                        {
                            // Non-empty arm: push transform into types[0]; quasis unchanged.
                            let non_empty_arm = {
                                let mut types_prime = Vec::with_capacity(rest_types.len() + 1);
                                types_prime.push(resolve(
                                    possible_concrete_types_for_inspection,
                                    cx,
                                    kind,
                                    loc.dupe(),
                                    t0.dupe(),
                                ));
                                types_prime.extend(rest_types.iter().cloned());
                                Type::new(TypeInner::TemplateLiteralT {
                                    reason: r.dupe(),
                                    from_annot: true,
                                    quasis: quasis.clone(),
                                    types: types_prime,
                                })
                            };
                            // Empty arm: treat as the suffix template literal (quasis[1:], types[1:])
                            //    with the transform applied to it. Recursing handles nested empty
                            //    interpolations correctly.
                            let suffix = Type::new(TypeInner::TemplateLiteralT {
                                reason: r.dupe(),
                                from_annot: true,
                                quasis: rest_quasis.to_vec(),
                                types: rest_types.to_vec(),
                            });
                            let empty_arm = resolve(
                                possible_concrete_types_for_inspection,
                                cx,
                                kind,
                                loc.dupe(),
                                suffix,
                            );
                            Type::new(TypeInner::UnionT(
                                result_reason,
                                union_rep::make(
                                    Some(cx.make_aloc_id(&loc)),
                                    union_rep::UnionKind::UnknownKind,
                                    empty_arm,
                                    non_empty_arm,
                                    Rc::from([]),
                                ),
                            ))
                        }
                        (Some(_), Some((t0, rest_types))) => {
                            // types[0] cannot be empty; just push the transform into it.
                            let mut types_prime = Vec::with_capacity(rest_types.len() + 1);
                            types_prime.push(resolve(
                                possible_concrete_types_for_inspection,
                                cx,
                                kind,
                                loc.dupe(),
                                t0.dupe(),
                            ));
                            types_prime.extend(rest_types.iter().cloned());
                            Type::new(TypeInner::TemplateLiteralT {
                                reason: r,
                                from_annot: true,
                                quasis: quasis.clone(),
                                types: types_prime,
                            })
                        }
                        _ => {
                            // No types or arity invariant violated; preserve original.
                            let quasis_prime = transform_quasis(kind, quasis.clone());
                            Type::new(TypeInner::TemplateLiteralT {
                                reason: r,
                                from_annot: true,
                                quasis: quasis_prime,
                                types: types.clone(),
                            })
                        }
                    }
                }
            }
        }
        _ => mk_deferred(result_reason, kind, arg),
    }
}
