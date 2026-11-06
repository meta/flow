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
use flow_aloc::ALocId;
use flow_common::js_number;
use flow_common::reason;
use flow_common::reason::VirtualReasonDesc;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_typing_context::Context;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::InternalError;
use flow_typing_errors::error_message::InvalidTemplateLiteralTypeErrorKind;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_common::string_case_transform;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_type::type_::BigIntLiteral;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::DepthTrace;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::Literal;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::union_rep;
use flow_typing_type::type_util;
use flow_utils_concurrency::job_error::JobError;

pub fn extract_strings_aux(expand_generics: bool, t: &Type) -> Option<Vec<FlowSmolStr>> {
    match t.deref() {
        TypeInner::DefT(_, def) => match def.deref() {
            DefTInner::SingletonStrT { value, .. } => Some(vec![value.dupe()]),
            DefTInner::SingletonNumT { value, .. } => {
                Some(vec![js_number::ecma_string_of_float(value.0).into()])
            }
            DefTInner::SingletonBoolT { value, .. } => Some(vec![value.to_string().into()]),
            DefTInner::NullT => Some(vec![FlowSmolStr::new_inline("null")]),
            DefTInner::VoidT => Some(vec![FlowSmolStr::new_inline("undefined")]),
            DefTInner::SingletonBigIntT { value, .. } => {
                let BigIntLiteral(bigint, raw) = value;
                match bigint {
                    Some(v) => Some(vec![v.to_string().into()]),
                    None => {
                        let raw = raw.as_str();
                        let stripped = &raw[..raw.len().saturating_sub(1)];
                        let start = if !stripped.is_empty() && stripped.as_bytes()[0] == b'-' {
                            1
                        } else {
                            0
                        };
                        let is_non_decimal_radix = start + 1 < stripped.len()
                            && stripped.as_bytes()[start] == b'0'
                            && matches!(
                                stripped.as_bytes()[start + 1],
                                b'x' | b'X' | b'o' | b'O' | b'b' | b'B'
                            );
                        if is_non_decimal_radix {
                            None
                        } else {
                            Some(vec![stripped.replace('_', "").into()])
                        }
                    }
                }
            }
            _ => None,
        },
        TypeInner::UnionT(_, rep) => rep
            .members_iter()
            .map(|t| extract_strings_aux(expand_generics, t))
            .collect::<Option<Vec<_>>>()
            .map(|v| v.into_iter().flatten().collect()),
        TypeInner::GenericT(box GenericTData { bound, .. }) if expand_generics => {
            extract_strings_aux(expand_generics, bound)
        }
        // Nested template literal whose placeholders are themselves concrete (or
        // unions of concrete singletons): recursively extract each placeholder's
        // producible strings and take the Cartesian product against the quasis.
        // This is the recursive case that lets `` `a.${Join<U,D>}` `` collapse
        // once `Join<U,D>` evaluates to a literal during conditional-type unrolling.
        TypeInner::TemplateLiteralT { quasis, types, .. } => {
            let string_lists: Vec<Vec<FlowSmolStr>> = types
                .iter()
                .map(|t| extract_strings_aux(expand_generics, t))
                .collect::<Option<Vec<_>>>()?;
            if cross_product_size(&string_lists) > MAX_CROSS_PRODUCT_SIZE {
                return None;
            }
            fn product(
                prefix: String,
                quasis: &[FlowSmolStr],
                string_lists: &[Vec<FlowSmolStr>],
            ) -> Option<Vec<FlowSmolStr>> {
                match (quasis, string_lists) {
                    ([q], []) => Some(vec![format!("{}{}", prefix, q).into()]),
                    ([q, rest_q @ ..], [ss, rest_lists @ ..]) => {
                        let mut out = Vec::new();
                        for s in ss {
                            let next_prefix = format!("{}{}{}", prefix, q, s);
                            let xs = product(next_prefix, rest_q, rest_lists)?;
                            out.extend(xs);
                        }
                        Some(out)
                    }
                    _ => None,
                }
            }
            product(String::new(), quasis, &string_lists)
        }
        _ => None,
    }
}

pub fn extract_strings(t: &Type) -> Option<Vec<FlowSmolStr>> {
    extract_strings_aux(false, t)
}

pub fn is_number_text(s: &str) -> bool {
    fn is_js_whitespace(c: char) -> bool {
        // ASCII whitespace plus the multi-byte JS WhiteSpace/LineTerminator
        // codepoints recognized by `String.prototype.trim`. Matches the
        // OCaml port's `js_whitespace_byte_len`.
        matches!(
            c,
            ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c' | '\u{00a0}' | '\u{1680}' | '\u{2000}'
                ..='\u{200a}'
                    | '\u{2028}'
                    | '\u{2029}'
                    | '\u{202f}'
                    | '\u{205f}'
                    | '\u{3000}'
                    | '\u{feff}'
        )
    }

    if s.is_empty() {
        return false;
    }

    let s = s.trim_matches(is_js_whitespace);
    let n = s.len();
    if n == 0 {
        return true;
    }

    let bytes = s.as_bytes();
    let is_digit = |c: u8| c.is_ascii_digit();
    let is_hex = |c: u8| c.is_ascii_hexdigit();
    let is_oct = |c: u8| (b'0'..=b'7').contains(&c);
    let is_bin = |c: u8| c == b'0' || c == b'1';
    let read_digits = |start: usize| {
        let mut j = start;
        while j < n && is_digit(bytes[j]) {
            j += 1;
        }
        j
    };
    let all_from = |start: usize, ok: &dyn Fn(u8) -> bool| -> bool {
        start < n && (start..n).all(|j| ok(bytes[j]))
    };
    let validate_decimal = |start: usize| {
        let int_end = read_digits(start);
        let has_int = int_end > start;
        let (after_frac, has_frac) = if int_end < n && bytes[int_end] == b'.' {
            let f = read_digits(int_end + 1);
            (f, f > int_end + 1)
        } else {
            (int_end, false)
        };
        if !(has_int || has_frac) {
            false
        } else if after_frac < n && (bytes[after_frac] == b'e' || bytes[after_frac] == b'E') {
            let after_e = after_frac + 1;
            let after_sign = if after_e < n && (bytes[after_e] == b'+' || bytes[after_e] == b'-') {
                after_e + 1
            } else {
                after_e
            };
            let exp_end = read_digits(after_sign);
            exp_end > after_sign && exp_end == n
        } else {
            after_frac == n
        }
    };
    let signed = bytes[0] == b'+' || bytes[0] == b'-';
    let i = if signed { 1 } else { 0 };
    if i >= n {
        false
    } else if !signed && i + 1 < n && bytes[i] == b'0' {
        match bytes[i + 1] {
            b'x' | b'X' => all_from(i + 2, &is_hex),
            b'o' | b'O' => all_from(i + 2, &is_oct),
            b'b' | b'B' => all_from(i + 2, &is_bin),
            _ => validate_decimal(i),
        }
    } else {
        validate_decimal(i)
    }
}

pub fn is_bigint_text(s: &str) -> bool {
    let n = s.len();
    let bytes = s.as_bytes();
    let is_digit = |c: u8| c.is_ascii_digit();
    let is_hex = |c: u8| c.is_ascii_hexdigit();
    let is_oct = |c: u8| (b'0'..=b'7').contains(&c);
    let is_bin = |c: u8| c == b'0' || c == b'1';
    let all_from = |start: usize, ok: &dyn Fn(u8) -> bool| -> bool {
        start < n && (start..n).all(|j| ok(bytes[j]))
    };
    if n == 0 {
        false
    } else {
        let i = if bytes[0] == b'-' { 1 } else { 0 };
        if i >= n {
            false
        } else if bytes[i] == b'0' {
            if i + 1 == n {
                true
            } else if i + 1 < n {
                match bytes[i + 1] {
                    b'x' | b'X' => all_from(i + 2, &is_hex),
                    b'o' | b'O' => all_from(i + 2, &is_oct),
                    b'b' | b'B' => all_from(i + 2, &is_bin),
                    _ => false,
                }
            } else {
                false
            }
        } else if (b'1'..=b'9').contains(&bytes[i]) {
            (i + 1..n).all(|j| is_digit(bytes[j]))
        } else {
            false
        }
    }
}

pub fn is_bool_text(s: &str) -> bool {
    s == "true" || s == "false"
}

pub fn lexical_validator_of(t: &Type) -> Option<Box<dyn Fn(&str) -> bool>> {
    match t.deref() {
        TypeInner::DefT(_, def) => match def.deref() {
            DefTInner::StrGeneralT(_) => Some(Box::new(is_string_text)),
            DefTInner::NumGeneralT(_) => Some(Box::new(is_number_text)),
            DefTInner::BigIntGeneralT(_) => Some(Box::new(is_bigint_text)),
            DefTInner::BoolGeneralT => Some(Box::new(is_bool_text)),
            _ => None,
        },
        // `Uppercase<X>` etc.: the substring is accepted iff it's in the canonical
        //    casing form for `kind` AND it satisfies the lexical validator for `arg`.
        //    This is what makes `'ABC' as Uppercase<\`a${string}\`>` reject 'aBc' but
        //    accept 'ABC' once the prefix is matched: the inner `${Uppercase<string>}`
        //    places `'BC'` against `string`-with-uppercase-canonical-form, and 'BC'
        //    uppercased is itself.
        //
        //    For `Capitalize` / `Uncapitalize`, an EMPTY substring is rejected here
        //    even though the empty string is trivially canonical: in placeholder
        //    context, an empty placeholder means the leading character of the produced
        //    string comes from a neighboring quasi or interpolation, not from this
        //    placeholder — so the cap/uncap constraint must not "discharge" on an
        //    empty match. Callers that want to admit the empty-placeholder case (e.g.
        //    `Capitalize<\`${string}abc\`>` accepting `'Abc'`) handle it by emitting
        //    a separate union arm in `string_case_transform::resolve`.
        TypeInner::StringMappingT { kind, arg, .. } => {
            let kind = *kind;
            match lexical_validator_of(arg) {
                Some(inner) => Some(Box::new(move |s| {
                    string_case_transform::is_canonical_for_placeholder(kind, s) && inner(s)
                })),
                None => None,
            }
        }
        // A mixed union accepts a substring producible through any member:
        // general members contribute their validator, concrete members their
        // stringified values. A member with neither (generics, `any`,
        // objects, ...) keeps the flow-based fallback instead of guessing.
        TypeInner::UnionT(_, rep) => {
            let mut validators: Vec<Box<dyn Fn(&str) -> bool>> = Vec::new();
            let mut concrete: Vec<FlowSmolStr> = Vec::new();
            for member in rep.members_iter() {
                if let Some(validate) = lexical_validator_of(member) {
                    validators.push(validate);
                } else {
                    let ss = extract_strings(member)?;
                    concrete.extend(ss);
                }
            }
            Some(Box::new(move |s: &str| {
                validators.iter().any(|v| v(s)) || concrete.iter().any(|c| c.as_str() == s)
            }))
        }
        _ => None,
    }
}

fn is_string_text(_: &str) -> bool {
    true
}

/// Result of attempting an exact split of a concrete string against a
/// TemplateLiteralT's `quasis`/`types`. Drives the SingletonStrT<:TemplateLiteralT
/// subtyping rule:
/// - `Match` pairs: the split succeeded; each pair is a (substring, placeholder type)
///   that the caller may need to flow.
/// - `Mismatch`: no split works — the string is not a member of the template type.
/// - `InvariantViolation`: quasis/types arities violate the invariant (quasis = types + 1).
pub enum MatchResult {
    Match(Vec<(FlowSmolStr, Type)>),
    Mismatch,
    InvariantViolation,
}

/// Plan a successful match as a list of (substring, type) pairs to flow,
/// without emitting any errors. Returning a result lets the matcher
/// backtrack: try one candidate, and if the remaining match fails, try
/// the next candidate instead of committing to the first that chops.
/// When a type's possible values are concrete, validate the assigned
/// substring is in the set during planning so semantic mismatches also
/// trigger backtracking (otherwise plan would commit to the first
/// structurally-valid split and fail at flow time).
pub fn match_string_against_template(
    s: &str,
    quasis: &[FlowSmolStr],
    types: &[Type],
) -> MatchResult {
    match plan(s, quasis, types) {
        Err(()) => MatchResult::InvariantViolation,
        Ok(None) => MatchResult::Mismatch,
        Ok(Some(pairs)) => MatchResult::Match(pairs),
    }
}

fn pair(part: &str, t: &Type) -> Option<(FlowSmolStr, Type)> {
    match extract_strings(t) {
        Some(strs) => {
            if strs.iter().any(|s| s.as_str() == part) {
                Some((part.into(), t.dupe()))
            } else {
                None
            }
        }
        None => match lexical_validator_of(t) {
            Some(validate) => {
                if validate(part) {
                    Some((part.into(), t.dupe()))
                } else {
                    None
                }
            }
            None => Some((part.into(), t.dupe())),
        },
    }
}

fn cons_pair(
    part: &str,
    t: &Type,
    pairs: Vec<(FlowSmolStr, Type)>,
) -> Option<Vec<(FlowSmolStr, Type)>> {
    match pair(part, t) {
        None => None,
        Some(p) => {
            let mut result = Vec::with_capacity(pairs.len() + 1);
            result.push(p);
            result.extend(pairs);
            Some(result)
        }
    }
}

type PlanResult = Result<Option<Vec<(FlowSmolStr, Type)>>, ()>;

fn plan(s: &str, quasis: &[FlowSmolStr], types: &[Type]) -> PlanResult {
    match (quasis, types) {
        ([q], []) => Ok(if s == q.as_str() { Some(vec![]) } else { None }),
        ([q, qs @ ..], [t, ts @ ..]) => {
            let Some(rest) = s.strip_prefix(q.as_str()) else {
                return Ok(None);
            };
            match qs {
                [last_q] => Ok(match_anchored_to_end(rest, t, last_q.as_str())),
                [next_q, ..] if !next_q.is_empty() => {
                    match_with_delimiter(rest, t, qs, ts, next_q.as_str())
                }
                _ => match extract_strings(t) {
                    Some(strs) => match_concrete_prefix(rest, t, qs, ts, &strs),
                    None => {
                        let next_concrete = ts.first().and_then(extract_strings);
                        match next_concrete {
                            Some(next_strs) => match_lookahead(rest, t, qs, ts, &next_strs),
                            None => match_single_char_step(rest, t, qs, ts),
                        }
                    }
                },
            }
        }
        _ => Err(()),
    }
}

/// Last placeholder before the terminal quasi: chop the trailing quasi off
/// and pair the middle.
fn match_anchored_to_end(rest: &str, t: &Type, last_q: &str) -> Option<Vec<(FlowSmolStr, Type)>> {
    let middle = rest.strip_suffix(last_q)?;
    pair(middle, t).map(|p| vec![p])
}

/// Non-empty quasi between this placeholder and the next: scan the
/// remaining string for it; the prefix before each occurrence is a
/// candidate substring for this placeholder. Backtrack on subsequent
/// failures.
fn match_with_delimiter(
    rest: &str,
    t: &Type,
    qs: &[FlowSmolStr],
    ts: &[Type],
    next_q: &str,
) -> PlanResult {
    let next_q_bytes = next_q.as_bytes();
    let rest_bytes = rest.as_bytes();
    for i in 0..=rest.len() {
        if i + next_q_bytes.len() <= rest_bytes.len()
            && &rest_bytes[i..i + next_q_bytes.len()] == next_q_bytes
        {
            let Some(part) = rest.get(..i) else {
                continue;
            };
            let Some(remainder) = rest.get(i..) else {
                continue;
            };
            if let Some(pairs) = plan(remainder, qs, ts)?
                && let Some(result) = cons_pair(part, t, pairs)
            {
                return Ok(Some(result));
            }
        }
    }
    Ok(None)
}

/// Adjacent placeholders (empty quasi between them) and this placeholder's
/// type is a known concrete-string set. Try each candidate as a prefix and
/// backtrack if the rest fails. Cases like `${'a' | 'ab'}${T}` against
/// 'abc' with T='c' must try 'ab' after 'a' fails.
fn match_concrete_prefix(
    rest: &str,
    t: &Type,
    qs: &[FlowSmolStr],
    ts: &[Type],
    strs: &[FlowSmolStr],
) -> PlanResult {
    for candidate in strs {
        if let Some(remainder) = rest.strip_prefix(candidate.as_str())
            && let Some(pairs) = plan(remainder, qs, ts)?
            && let Some(result) = cons_pair(candidate.as_str(), t, pairs)
        {
            return Ok(Some(result));
        }
    }
    Ok(None)
}

/// Adjacent placeholders, this placeholder is non-concrete but the next is.
/// Scan forward for any occurrence of a next-placeholder candidate; the
/// substring before it is assigned to this placeholder.
fn match_lookahead(
    rest: &str,
    t: &Type,
    qs: &[FlowSmolStr],
    ts: &[Type],
    next_strs: &[FlowSmolStr],
) -> PlanResult {
    // Drop empty candidates: matching at i=0 makes no progress, and any
    // non-empty candidate will be tried at its real offset anyway.
    let next_strs: Vec<&FlowSmolStr> = next_strs.iter().filter(|s| !s.is_empty()).collect();
    let rest_bytes = rest.as_bytes();
    for i in 0..=rest.len() {
        let found = next_strs.iter().any(|candidate| {
            let c = candidate.as_str().as_bytes();
            i + c.len() <= rest_bytes.len() && &rest_bytes[i..i + c.len()] == c
        });
        if found {
            let Some(part) = rest.get(..i) else {
                continue;
            };
            let Some(remainder) = rest.get(i..) else {
                continue;
            };
            if let Some(pairs) = plan(remainder, qs, ts)?
                && let Some(result) = cons_pair(part, t, pairs)
            {
                return Ok(Some(result));
            }
        }
    }
    Ok(None)
}

/// Adjacent placeholders with no concrete anchor on either side: assign
/// one character at a time, or the empty string when [rest] is empty
/// (later placeholders may still match emptily).
fn match_single_char_step(rest: &str, t: &Type, qs: &[FlowSmolStr], ts: &[Type]) -> PlanResult {
    let (char_str, remainder) = if !rest.is_empty() {
        let char_len = rest.chars().next().map_or(0, char::len_utf8);
        (&rest[..char_len], &rest[char_len..])
    } else {
        ("", "")
    };
    match plan(remainder, qs, ts)? {
        Some(pairs) => Ok(cons_pair(char_str, t, pairs)),
        None => Ok(None),
    }
}

/// Fold any placeholder whose type is a single concrete string (or empty union
/// thereof) into the adjacent quasis. This normalizes structures like
/// `${'px'}` to a quasi `"px"` so that pairwise quasi comparison can succeed
/// against an upper bound that has the same characters as literal text rather
/// than as a placeholder. Returns (quasis', types') with the same total
/// length invariant (quasis = types + 1).
pub fn fold_concrete_placeholders(
    quasis: &[FlowSmolStr],
    types: &[Type],
) -> (Vec<FlowSmolStr>, Vec<Type>) {
    let mut acc_q: Vec<FlowSmolStr> = Vec::new();
    let mut acc_t: Vec<Type> = Vec::new();
    let mut pending_q = String::new();
    let mut qi = 0;
    let mut ti = 0;
    while qi < quasis.len() && ti < types.len() {
        let q = quasis[qi].as_str();
        let t = &types[ti];
        match extract_strings(t) {
            Some(ref ss) if ss.len() == 1 => {
                pending_q.push_str(q);
                pending_q.push_str(ss[0].as_str());
            }
            _ => {
                pending_q.push_str(q);
                acc_q.push(pending_q.clone().into());
                acc_t.push(t.dupe());
                pending_q.clear();
            }
        }
        qi += 1;
        ti += 1;
    }
    if qi < quasis.len() {
        pending_q.push_str(quasis[qi].as_str());
        acc_q.push(pending_q.into());
    } else {
        acc_q.push(pending_q.into());
    }
    (acc_q, acc_t)
}

/// True if `t`, when placed in a template literal placeholder, always coerces
/// to a string at runtime: string, number, bigint, boolean, null, undefined,
/// and their literal forms are all stringified. Unions are coercible iff
/// every member is. Generics are coercible iff their bound is. This is used
/// when subtyping `${T}` <: `${string}` — the RHS placeholder accepts any of
/// these without needing strict T <: string.
pub fn placeholder_coerces_to_string(t: &Type) -> bool {
    match t.deref() {
        TypeInner::DefT(_, def) => matches!(
            def.deref(),
            DefTInner::StrGeneralT(_)
                | DefTInner::SingletonStrT { .. }
                | DefTInner::NumGeneralT(_)
                | DefTInner::SingletonNumT { .. }
                | DefTInner::NumericStrKeyT(..)
                | DefTInner::BigIntGeneralT(_)
                | DefTInner::SingletonBigIntT { .. }
                | DefTInner::BoolGeneralT
                | DefTInner::SingletonBoolT { .. }
                | DefTInner::NullT
                | DefTInner::VoidT
        ),
        TypeInner::TemplateLiteralT { .. } => true,
        TypeInner::UnionT(_, rep) => rep.members_iter().all(placeholder_coerces_to_string),
        TypeInner::GenericT(box GenericTData { bound, .. }) => placeholder_coerces_to_string(bound),
        _ => false,
    }
}

pub fn cartesian_product(
    quasis: &[FlowSmolStr],
    string_lists: &[Vec<FlowSmolStr>],
) -> Option<Vec<FlowSmolStr>> {
    fn aux(
        prefix: String,
        quasis: &[FlowSmolStr],
        string_lists: &[Vec<FlowSmolStr>],
    ) -> Option<Vec<FlowSmolStr>> {
        match (quasis, string_lists) {
            ([q], []) => Some(vec![format!("{}{}", prefix, q).into()]),
            ([q, qs @ ..], [ss, rest @ ..]) => {
                let mut acc = Vec::new();
                for s in ss {
                    acc.extend(aux(format!("{}{}{}", prefix, q, s), qs, rest)?);
                }
                Some(acc)
            }
            _ => None,
        }
    }

    aux(String::new(), quasis, string_lists)
}

pub const MAX_CROSS_PRODUCT_SIZE: usize = 10_000;

pub fn try_resolve_eagerly_aux(
    expand_generics: bool,
    types: &[Type],
) -> Option<Vec<Vec<FlowSmolStr>>> {
    types
        .iter()
        .map(|t| extract_strings_aux(expand_generics, t))
        .collect()
}

pub fn try_resolve_eagerly(types: &[Type]) -> Option<Vec<Vec<FlowSmolStr>>> {
    try_resolve_eagerly_aux(false, types)
}

pub fn cross_product_size(string_lists: &[Vec<FlowSmolStr>]) -> usize {
    string_lists
        .iter()
        .fold(1usize, |acc, strs| acc.saturating_mul(strs.len()))
}

/// Post-process a per-type string-lists resolution into the final list of
/// producible strings. Returns [None] when the cross product exceeds the
/// complexity cap, when any union member was empty (which would otherwise
/// silently accept any RHS), or when the resolver itself returned [None].
fn finalize_to_strings(
    quasis: &[FlowSmolStr],
    string_lists: Option<Vec<Vec<FlowSmolStr>>>,
) -> Option<Vec<FlowSmolStr>> {
    let string_lists = string_lists?;
    if cross_product_size(&string_lists) > MAX_CROSS_PRODUCT_SIZE {
        return None;
    }
    match cartesian_product(quasis, &string_lists) {
        Some(strings) if !strings.is_empty() => Some(strings),
        _ => None,
    }
}

pub fn try_resolve_to_strings(
    quasis: &[FlowSmolStr],
    types: &[Type],
    expand_generics: bool,
) -> Option<Vec<FlowSmolStr>> {
    finalize_to_strings(quasis, try_resolve_eagerly_aux(expand_generics, types))
}

/// Subtyping-time variant: concretize each type via Flow_js before extracting.
/// The syntactic `extract_strings_aux` only walks `UnionT` and `GenericT`, so it
/// misses indirected types like `EvalT` (indexed access, mapped/conditional),
/// `KeysT`, `OpenT`, `AnnotT`, and `TypeAppT`. At subtyping time we have
/// Flow_js access and can resolve those to concrete forms first.
/// `possible_concrete_types_for_inspection` is passed as a closure rather
/// than imported directly so this module stays free of Flow_js dependencies
/// (it is also called from annotation-time code paths that deliberately block
/// Flow_js).
pub fn extract_strings_concretized<'cx>(
    possible_concrete_types_for_inspection: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        &reason::Reason,
        &Type,
    ) -> Result<Vec<Type>, FlowJsException>,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    t: &Type,
) -> Result<Option<Vec<FlowSmolStr>>, FlowJsException> {
    let ts = possible_concrete_types_for_inspection(cx, env, type_util::reason_of_t(t), t)?;
    Ok(ts
        .iter()
        .map(|t| extract_strings_aux(true, t))
        .collect::<Option<Vec<_>>>()
        .map(|v| v.into_iter().flatten().collect()))
}

pub fn try_resolve_concretized<'cx>(
    possible_concrete_types_for_inspection: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        &reason::Reason,
        &Type,
    ) -> Result<Vec<Type>, FlowJsException>,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    types: &[Type],
) -> Result<Option<Vec<Vec<FlowSmolStr>>>, FlowJsException> {
    types
        .iter()
        .map(|t| extract_strings_concretized(possible_concrete_types_for_inspection, cx, env, t))
        .collect::<Result<Option<Vec<_>>, _>>()
}

/// The string literal types a template literal type produces, given the concrete types of each
/// of its placeholders (`placeholders`, in order). [None] when a placeholder isn't literal, a union
/// placeholder is empty, or the product exceeds [MAX_CROSS_PRODUCT_SIZE]. A generic placeholder
/// isn't expanded, since its strings aren't known.
pub fn string_literals_of_template_literal(
    reason: &reason::Reason,
    from_annot: bool,
    quasis: &[FlowSmolStr],
    placeholders: &[Vec<Type>],
) -> Option<Vec<Type>> {
    let string_lists = placeholders
        .iter()
        .map(|ts| {
            ts.iter()
                .map(|t| extract_strings_aux(false, t))
                .collect::<Option<Vec<_>>>()
                .map(|v| v.into_iter().flatten().collect())
        })
        .collect::<Option<Vec<Vec<FlowSmolStr>>>>();
    let strings = finalize_to_strings(quasis, string_lists)?;
    Some(
        strings
            .into_iter()
            .map(|s| {
                Type::new(TypeInner::DefT(
                    reason
                        .dupe()
                        .replace_desc(VirtualReasonDesc::RStringLit(s.dupe())),
                    DefT::new(DefTInner::SingletonStrT {
                        from_annot,
                        value: s,
                    }),
                ))
            })
            .collect(),
    )
}

/// Concretizing analogue of `try_resolve_to_strings`.
pub fn try_resolve_to_strings_concretized<'cx>(
    possible_concrete_types_for_inspection: &dyn Fn(
        &mut Context<'cx>,
        &FlowJsEnv,
        &reason::Reason,
        &Type,
    ) -> Result<Vec<Type>, FlowJsException>,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    quasis: &[FlowSmolStr],
    types: &[Type],
) -> Result<Option<Vec<FlowSmolStr>>, FlowJsException> {
    let resolved = try_resolve_concretized(possible_concrete_types_for_inspection, cx, env, types)?;
    Ok(finalize_to_strings(quasis, resolved))
}

pub fn definitely_invalid_placeholder(t: &Type) -> bool {
    match t.deref() {
        TypeInner::DefT(_, def) => matches!(
            def.deref(),
            DefTInner::ObjT(_)
                | DefTInner::ClassT(_)
                | DefTInner::InstanceT(_)
                | DefTInner::FunT(_, _)
                | DefTInner::SymbolT
                | DefTInner::UniqueSymbolT(_)
                | DefTInner::ArrT(_)
                | DefTInner::TypeT(_, _)
                | DefTInner::EnumValueT(_)
                | DefTInner::EnumObjectT { .. }
                | DefTInner::RendersT(_)
                | DefTInner::MixedT(_)
        ),
        _ => false,
    }
}

pub fn definitely_invalid_placeholder_deep(t: &Type) -> bool {
    match t.deref() {
        TypeInner::UnionT(_, rep) => rep.members_iter().any(definitely_invalid_placeholder_deep),
        TypeInner::GenericT(box GenericTData { bound, .. }) => match bound.deref() {
            TypeInner::DefT(_, def) if matches!(def.deref(), DefTInner::MixedT(_)) => false,
            _ => definitely_invalid_placeholder_deep(bound),
        },
        _ => definitely_invalid_placeholder(t),
    }
}

/// Build a [SingletonStrT] (singleton), a [UnionT] of [SingletonStrT]s
/// (multiple strings), or [None] (empty list — callers handle that case
/// context-specifically). [mk_str] constructs the per-element [SingletonStrT]
/// so callers can attach their preferred per-element reason.
fn mk_singleton_or_union(
    union_reason: &reason::Reason,
    union_aloc: ALocId,
    mk_str: impl Fn(&FlowSmolStr) -> Type,
    strings: &[FlowSmolStr],
) -> Option<Type> {
    match strings {
        [] => None,
        [s] => Some(mk_str(s)),
        [s0, s1, rest @ ..] => {
            let t0 = mk_str(s0);
            let t1 = mk_str(s1);
            let ts: Rc<[Type]> = rest.iter().map(mk_str).collect();
            Some(Type::new(TypeInner::UnionT(
                union_reason.dupe(),
                union_rep::make(
                    Some(union_aloc),
                    union_rep::UnionKind::UnknownKind,
                    t0,
                    t1,
                    ts,
                ),
            )))
        }
    }
}

/// Enforce that template literal placeholders are assignable to
/// `string | number | bigint | boolean | null | undefined`.
///
/// Two-phase check:
/// 1. Eagerly: emit immediately for placeholders provably invalid from the
///    shape alone ([definitely_invalid_placeholder_deep]).
/// 2. Deferred: for placeholders we can't prove invalid syntactically, ask
///    FlowJs after main inference to concretize types hidden behind
///    EvalT/OpenT/AnnotT (indexed access, mapped/conditional types, property
///    access) and re-run the predicate on each branch.
fn validate_placeholders<'cx>(cx: &mut Context<'cx>, loc: &ALoc, types: &[Type]) {
    let emit_error = |cx: &mut Context<'cx>, loc: &ALoc| {
        flow_js_utils::add_output_non_speculating(
            cx,
            ErrorMessage::EInvalidTemplateLiteralType {
                loc: loc.dupe(),
                kind: InvalidTemplateLiteralTypeErrorKind::InvalidPlaceholderType,
            },
        );
    };
    for t in types {
        if definitely_invalid_placeholder_deep(t) {
            emit_error(cx, loc);
        } else {
            let t = t.dupe();
            let loc = loc.dupe();
            cx.add_post_inference_validation_callback(Box::new(move |cx: &mut Context<'_>| {
                // Concretization can emit its own errors as a side effect
                // (e.g. "indexed access is not an object" when evaluating
                // `T[K]` under an unresolved generic `T`). Those errors are
                // unrelated to the placeholder-validity question we're
                // asking.
                let concrete = cx.with_suppressed_errors(|cx| {
                    crate::flow_js::FlowJs::possible_concrete_types_for_inspection(
                        cx,
                        type_util::reason_of_t(&t),
                        &t,
                    )
                });
                let concrete = match concrete {
                    Ok(c) => c,
                    Err(FlowJsException::WorkerCanceled(c)) => {
                        return Err(JobError::Canceled(c));
                    }
                    Err(FlowJsException::TimedOut(t)) => {
                        return Err(JobError::TimedOut(t));
                    }
                    Err(FlowJsException::DebugThrow { loc }) => {
                        return Err(JobError::DebugThrow { loc });
                    }
                    // Other speculation-related errors are absorbed (the
                    // syntactic check is a best-effort soundness improvement).
                    Err(_) => return Ok(()),
                };
                if concrete.iter().any(definitely_invalid_placeholder_deep) {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        ErrorMessage::EInvalidTemplateLiteralType {
                            loc: loc.dupe(),
                            kind: InvalidTemplateLiteralTypeErrorKind::InvalidPlaceholderType,
                        },
                    );
                }
                Ok(())
            }));
        }
    }
}

pub fn resolve<'cx>(
    quasis: Vec<FlowSmolStr>,
    types: Vec<Type>,
    loc: ALoc,
    cx: &mut Context<'cx>,
) -> Type {
    let unresolved = || {
        let reason = reason::mk_annot_reason(None, loc.dupe());
        Type::new(TypeInner::TemplateLiteralT {
            reason,
            from_annot: true,
            quasis: quasis.to_vec(),
            types: types.to_vec(),
        })
    };
    validate_placeholders(cx, &loc, &types);
    if types.is_empty() {
        let s: FlowSmolStr = quasis.iter().map(|s| s.as_str()).collect::<String>().into();
        let reason = reason::mk_annot_reason(VirtualReasonDesc::RStringLit(s.dupe()), loc.dupe());
        return Type::new(TypeInner::DefT(
            reason,
            DefT::new(DefTInner::SingletonStrT {
                from_annot: true,
                value: s,
            }),
        ));
    }
    let Some(string_lists) = try_resolve_eagerly(&types) else {
        return unresolved();
    };
    if cross_product_size(&string_lists) > MAX_CROSS_PRODUCT_SIZE {
        flow_js_utils::add_output_non_speculating(
            cx,
            ErrorMessage::EInvalidTemplateLiteralType {
                loc: loc.dupe(),
                kind: InvalidTemplateLiteralTypeErrorKind::TooComplex,
            },
        );
        let reason = reason::mk_annot_reason(None, loc.dupe());
        return Type::new(TypeInner::DefT(
            reason,
            DefT::new(DefTInner::StrGeneralT(Literal::AnyLiteral)),
        ));
    }
    let Some(strings) = cartesian_product(&quasis, &string_lists) else {
        flow_js_utils::add_output_non_speculating(
            cx,
            ErrorMessage::EInternal(Box::new((
                loc.dupe(),
                InternalError::UnexpectedAnnotationInference(FlowSmolStr::new_inline(
                    "TemplateLiteralT quasis/types arity",
                )),
            ))),
        );
        return unresolved();
    };
    let mk_str = |s: &FlowSmolStr| {
        let r = reason::mk_annot_reason(VirtualReasonDesc::RStringLit(s.dupe()), loc.dupe());
        Type::new(TypeInner::DefT(
            r,
            DefT::new(DefTInner::SingletonStrT {
                from_annot: true,
                value: s.dupe(),
            }),
        ))
    };
    let union_reason = reason::mk_annot_reason(None, loc.dupe());
    let union_aloc = cx.make_aloc_id(&loc);
    match mk_singleton_or_union(&union_reason, union_aloc, mk_str, &strings) {
        Some(t) => t,
        // Empty cross product means a union member was empty; fall back to
        // TemplateLiteralT so subtyping doesn't silently accept anything.
        None => unresolved(),
    }
}

/// Value-inference counterpart of [resolve] for precise template literal values.
/// Mirrors TypeScript: `const` declarations and `as const` expressions retain
/// `` `prefix${string}` `` when a placeholder has type `string`, and eagerly fold
/// to a string literal when every placeholder is literal.
///
/// Unlike [resolve], this never emits placeholder or complexity errors: the
/// substituted expressions were already checked for string coercion during
/// statement inference, so erroring here would only duplicate diagnostics.
/// Overly complex literal cross products silently fall back to TemplateLiteralT.
pub fn resolve_for_value<'cx>(
    quasis: Vec<FlowSmolStr>,
    types: Vec<Type>,
    loc: ALoc,
    cx: &mut Context<'cx>,
    from_annot: bool,
) -> Type {
    let unresolved = || {
        let reason = reason::mk_reason(None, loc.dupe());
        Type::new(TypeInner::TemplateLiteralT {
            reason,
            from_annot,
            quasis: quasis.to_vec(),
            types: types.to_vec(),
        })
    };
    if types.is_empty() {
        let s: FlowSmolStr = quasis.iter().map(|s| s.as_str()).collect::<String>().into();
        let reason = reason::mk_annot_reason(VirtualReasonDesc::RStringLit(s.dupe()), loc.dupe());
        return Type::new(TypeInner::DefT(
            reason,
            DefT::new(DefTInner::SingletonStrT {
                from_annot,
                value: s,
            }),
        ));
    }
    let Some(string_lists) = try_resolve_eagerly(&types) else {
        return unresolved();
    };
    if cross_product_size(&string_lists) > MAX_CROSS_PRODUCT_SIZE {
        return unresolved();
    }
    let Some(strings) = cartesian_product(&quasis, &string_lists) else {
        return unresolved();
    };
    let mk_str = |s: &FlowSmolStr| {
        let r = reason::mk_annot_reason(VirtualReasonDesc::RStringLit(s.dupe()), loc.dupe());
        Type::new(TypeInner::DefT(
            r,
            DefT::new(DefTInner::SingletonStrT {
                from_annot,
                value: s.dupe(),
            }),
        ))
    };
    let union_reason = reason::mk_reason(None, loc.dupe());
    let union_aloc = cx.make_aloc_id(&loc);
    match mk_singleton_or_union(&union_reason, union_aloc, mk_str, &strings) {
        Some(t) => t,
        None => unresolved(),
    }
}

// =========================================================================
// Subtyping helpers (callers: subtyping_kit)
// =========================================================================

/// Replace each placeholder type with a singleton/union of its stringified
/// producible values, when concretization reveals it is fully literal. This
/// lets `${T[0]}` (an EvalT indexed access) resolve to `${'1'}` after T is
/// substituted to a concrete tuple, so the matcher compares stringified forms
/// instead of trying to flow a string into the underlying number/bigint.
/// Generic placeholders are NOT expanded — keeping them as-is preserves
/// inference for APIs like `<T extends 'a'|'b'>(x: \`get${T}\`): T`.
pub fn concretize_placeholders<'cx>(
    possible_concrete_types_for_inspection: Option<
        &dyn Fn(
            &mut Context<'cx>,
            &FlowJsEnv,
            &reason::Reason,
            &Type,
        ) -> Result<Vec<Type>, FlowJsException>,
    >,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    upper: &Type,
    types: &[Type],
) -> Result<Vec<Type>, FlowJsException> {
    let Some(concretize) = possible_concrete_types_for_inspection else {
        return Ok(types.to_vec());
    };
    let union_reason = type_util::reason_of_t(upper).dupe();
    let union_aloc = cx.make_aloc_id(union_reason.loc());
    let mk_str = |s: &FlowSmolStr| {
        let r = union_reason
            .dupe()
            .replace_desc(VirtualReasonDesc::RStringLit(s.dupe()));
        Type::new(TypeInner::DefT(
            r,
            DefT::new(DefTInner::SingletonStrT {
                from_annot: true,
                value: s.dupe(),
            }),
        ))
    };
    let mut out = Vec::with_capacity(types.len());
    for t in types {
        if extract_strings(t).is_some() {
            out.push(t.dupe());
            continue;
        }
        match extract_strings_concretized(concretize, cx, env, t)? {
            Some(ss) => {
                match mk_singleton_or_union(&union_reason, union_aloc.dupe(), mk_str, &ss) {
                    Some(t2) => out.push(t2),
                    None => out.push(t.dupe()),
                }
            }
            None => out.push(t.dupe()),
        }
    }
    Ok(out)
}

/// SingletonStrT s <: TemplateLiteralT: decompose the string by quasis and
/// flow each extracted segment to the corresponding interpolated type. We
/// fold concrete-string placeholders into adjacent quasis first so that
/// patterns like `${T}${''}${U}` (which arise when a generic delimiter is
/// instantiated to '') normalize to `${T}${U}` — otherwise the empty
/// placeholder breaks pairwise decomposition and the match falls through.
/// When `possible_concrete_types_for_inspection` is provided, indirected
/// placeholder types (e.g. `T[0]` as an EvalT) are concretized and replaced
/// with their stringified singleton/union form before matching.
pub fn subtype_str_lit_into_template<'cx>(
    possible_concrete_types_for_inspection: Option<
        &dyn Fn(
            &mut Context<'cx>,
            &FlowJsEnv,
            &reason::Reason,
            &Type,
        ) -> Result<Vec<Type>, FlowJsException>,
    >,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    lower: &Type,
    upper: &Type,
    s: &str,
    quasis: &[FlowSmolStr],
    types: &[Type],
) -> Result<(), FlowJsException> {
    let types = concretize_placeholders(
        possible_concrete_types_for_inspection,
        cx,
        env,
        upper,
        types,
    )?;
    let (quasis, types) = fold_concrete_placeholders(quasis, &types);
    flow_js_utils::update_lit_type_from_annot(cx, env, lower);
    match match_string_against_template(s, &quasis, &types) {
        MatchResult::InvariantViolation => flow_js_utils::add_output_with_env(
            cx,
            env,
            ErrorMessage::EInternal(Box::new((
                type_util::reason_of_t(upper).loc().dupe(),
                InternalError::UnexpectedAnnotationInference(FlowSmolStr::new_inline(
                    "TemplateLiteralT quasis/types arity",
                )),
            ))),
        ),
        MatchResult::Mismatch => flow_js_utils::add_output_with_env(
            cx,
            env,
            flow_js_utils::incompatible_types_error(lower, upper, use_op.dupe(), None),
        ),
        MatchResult::Match(pairs) => {
            for (part, t) in pairs {
                // Three planning outcomes: lexical validator matched,
                // concrete-string set matched, or neither (generic-like).
                // Only the third needs a flow — the other two were already
                // validated during planning, and flowing a string literal
                // into a non-string type (NullT/VoidT/etc.) would emit a
                // spurious error.
                if lexical_validator_of(&t).is_none() && extract_strings(&t).is_none() {
                    let part_str: FlowSmolStr = part.as_str().into();
                    let r = type_util::reason_of_t(lower)
                        .dupe()
                        .replace_desc(VirtualReasonDesc::RStringLit(part_str.dupe()));
                    let str_t = Type::new(TypeInner::DefT(
                        r,
                        DefT::new(DefTInner::SingletonStrT {
                            from_annot: true,
                            value: part_str,
                        }),
                    ));
                    crate::flow_js::FlowJs::rec_flow_t_with_env(
                        cx,
                        env,
                        trace,
                        use_op.dupe(),
                        &str_t,
                        &t,
                    )?;
                }
            }
            Ok(())
        }
    }
}

/// TemplateLiteralT <: UnionT: enumerate the strings the LHS could produce
/// (expanding generics) and flow each one into the RHS union independently.
pub fn subtype_template_into_union<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    lower: &Type,
    upper: &Type,
    quasis: &[FlowSmolStr],
    types: &[Type],
) -> Result<(), FlowJsException> {
    let strings = try_resolve_to_strings(quasis, types, true).unwrap_or_default();
    for s in strings {
        let r = type_util::reason_of_t(lower)
            .dupe()
            .replace_desc(VirtualReasonDesc::RStringLit(s.dupe()));
        let str_t = Type::new(TypeInner::DefT(
            r,
            DefT::new(DefTInner::SingletonStrT {
                from_annot: true,
                value: s,
            }),
        ));
        crate::flow_js::FlowJs::rec_flow_t_with_env(cx, env, trace, use_op.dupe(), &str_t, upper)?;
    }
    Ok(())
}

/// Whether the wide-string rule accepts the LHS template against a RHS of
/// shape `prefix${string}suffix`: every LHS placeholder coerces to a string,
/// and the LHS's outer quasis (start/end) lexically match the RHS's
/// prefix/suffix. Uses raw LHS quasis/types (no folding) to preserve the
/// prior inline subtyping-kit guard's semantics; the RHS is folded so that
/// annotations with concrete-string placeholders (e.g.
/// `` `${string}${'dp'}` ``) match the `prefix${string}suffix` shape.
fn is_wide_string_template_match(
    lq_raw: &[FlowSmolStr],
    lt_raw: &[Type],
    rq: &[FlowSmolStr],
    rt: &[Type],
) -> bool {
    if rq.len() != 2 || rt.len() != 1 {
        return false;
    }
    let rp = rq[0].as_str();
    let rs = rq[1].as_str();
    let is_str_general = matches!(
        rt[0].deref(),
        TypeInner::DefT(_, def) if matches!(def.deref(), DefTInner::StrGeneralT(_))
    );
    is_str_general
        && lt_raw.iter().all(placeholder_coerces_to_string)
        && lq_raw.first().is_some_and(|first| first.starts_with(rp))
        && lq_raw.last().is_some_and(|last| last.ends_with(rs))
}

/// Result of [`try_subtype_template_to_template`]. [`NotApplicable`] means
/// no structural rule fired — the caller should fall through to the
/// cross-product enumeration path ([`subtype_template_to_other`]).
pub enum TlToTlResult {
    Handled,
    NotApplicable,
}

/// Combined dispatch for `TemplateLiteralT <: TemplateLiteralT`. Folds each
/// side's quasis/types once, then tries (in order):
/// 1. Pairwise: equal folded quasi shape → per-placeholder subtyping, with
///    `${string}` on the RHS accepting any LHS placeholder that coerces to
///    string.
/// 2. Prefix extension: `lp${lt}` <: `rp${rt}` when lp starts with rp.
/// 3. Suffix extension: mirror of the prefix rule.
/// 4. Wide string: any LHS template whose placeholders all coerce to string
///    and whose outer quasis match the RHS `prefix${string}suffix` shape.
///    Uses raw LHS quasis/types (unfolded), matching the original semantics.
pub fn try_subtype_template_to_template<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    l_reason: &reason::Reason,
    l_from_annot: bool,
    upper: &Type,
    lq_raw: &[FlowSmolStr],
    lt_raw: &[Type],
    rq_raw: &[FlowSmolStr],
    rt_raw: &[Type],
) -> Result<TlToTlResult, FlowJsException> {
    let (lq, lt) = fold_concrete_placeholders(lq_raw, lt_raw);
    let (rq, rt) = fold_concrete_placeholders(rq_raw, rt_raw);
    // If LHS folds to a single concrete string (no remaining placeholders), it
    // is semantically a `SingletonStrT`. Reconstruct it and dispatch to the
    // `SingletonStrT <: TemplateLiteralT` path so the RHS pattern can be
    // decomposed against the concrete string. This is the path recursive
    // types like `ReplaceAll` take when an intermediate iteration's
    // constructed-template-literal argument has become fully concrete.
    if let [s] = lq.as_slice()
        && lt.is_empty()
    {
        let r = l_reason
            .dupe()
            .replace_desc(VirtualReasonDesc::RStringLit(s.dupe()));
        let lower = Type::new(TypeInner::DefT(
            r,
            DefT::new(DefTInner::SingletonStrT {
                from_annot: true,
                value: s.dupe(),
            }),
        ));
        subtype_str_lit_into_template(
            None,
            cx,
            env,
            trace,
            use_op,
            &lower,
            upper,
            s.as_str(),
            &rq,
            &rt,
        )?;
        return Ok(TlToTlResult::Handled);
    }
    if lq == rq {
        for (l, r) in lt.iter().zip(rt.iter()) {
            let skip = matches!(
                r.deref(),
                TypeInner::DefT(_, def) if matches!(def.deref(), DefTInner::StrGeneralT(_))
            ) && placeholder_coerces_to_string(l);
            if !skip {
                crate::flow_js::FlowJs::rec_flow_t_with_env(cx, env, trace, use_op.dupe(), l, r)?;
            }
        }
        return Ok(TlToTlResult::Handled);
    }
    // Prefix extension: lq == [lp, ""], rq == [rp, ""], lp starts with rp.
    if let [lp, lp_tail] = lq.as_slice()
        && lp_tail.is_empty()
        && let [rp, rp_tail] = rq.as_slice()
        && rp_tail.is_empty()
        && lp.starts_with(rp.as_str())
        && lt.len() == 1
        && rt.len() == 1
    {
        let chopped = &lp.as_str()[rp.len()..];
        if chopped.is_empty() {
            crate::flow_js::FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &lt[0], &rt[0])?;
        } else {
            let lower = Type::new(TypeInner::TemplateLiteralT {
                reason: l_reason.dupe(),
                from_annot: l_from_annot,
                quasis: vec![chopped.into(), FlowSmolStr::new_inline("")],
                types: vec![lt[0].dupe()],
            });
            crate::flow_js::FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &lower, &rt[0])?;
        }
        return Ok(TlToTlResult::Handled);
    }
    // Suffix extension: lq == ["", ls], rq == ["", rs], ls ends with rs.
    if let [lp_head, ls] = lq.as_slice()
        && lp_head.is_empty()
        && let [rp_head, rs] = rq.as_slice()
        && rp_head.is_empty()
        && ls.ends_with(rs.as_str())
        && lt.len() == 1
        && rt.len() == 1
    {
        let chopped = &ls.as_str()[..ls.len() - rs.len()];
        if chopped.is_empty() {
            crate::flow_js::FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &lt[0], &rt[0])?;
        } else {
            let lower = Type::new(TypeInner::TemplateLiteralT {
                reason: l_reason.dupe(),
                from_annot: l_from_annot,
                quasis: vec![FlowSmolStr::new_inline(""), chopped.into()],
                types: vec![lt[0].dupe()],
            });
            crate::flow_js::FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &lower, &rt[0])?;
        }
        return Ok(TlToTlResult::Handled);
    }
    if is_wide_string_template_match(lq_raw, lt_raw, &rq, &rt) {
        Ok(TlToTlResult::Handled)
    } else {
        Ok(TlToTlResult::NotApplicable)
    }
}

/// TemplateLiteralT on the left, any non-TemplateLiteralT upper. Enumerates
/// the strings the template could produce (expanding generics) and flows as
/// a singleton/union. Falls back to a generic string type (Truthy when at
/// least one quasi is non-empty, AnyLiteral otherwise) when the cross product
/// can't be resolved.
pub fn subtype_template_to_other<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    reason: &reason::Reason,
    upper: &Type,
    quasis: &[FlowSmolStr],
    types: &[Type],
) -> Result<(), FlowJsException> {
    match try_resolve_to_strings_concretized(
        &|cx, env, r, t| {
            crate::flow_js::FlowJs::possible_concrete_types_for_inspection_with_env(cx, env, r, t)
        },
        cx,
        env,
        quasis,
        types,
    )? {
        Some(strings) => {
            let mk_str = |s: &FlowSmolStr| {
                let r = reason
                    .dupe()
                    .replace_desc(VirtualReasonDesc::RStringLit(s.dupe()));
                Type::new(TypeInner::DefT(
                    r,
                    DefT::new(DefTInner::SingletonStrT {
                        from_annot: true,
                        value: s.dupe(),
                    }),
                ))
            };
            let union_aloc = cx.make_aloc_id(reason.loc());
            let lower =
                mk_singleton_or_union(reason, union_aloc, mk_str, &strings).unwrap_or_else(|| {
                    // Empty cross product is filtered out by
                    // try_resolve_to_strings_concretized.
                    Type::new(TypeInner::DefT(
                        reason.dupe(),
                        DefT::new(DefTInner::StrGeneralT(Literal::AnyLiteral)),
                    ))
                });
            crate::flow_js::FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &lower, upper)
        }
        None => {
            let literal_kind = if quasis.iter().any(|q| !q.is_empty()) {
                Literal::Truthy
            } else {
                Literal::AnyLiteral
            };
            let str_t = Type::new(TypeInner::DefT(
                reason.dupe(),
                DefT::new(DefTInner::StrGeneralT(literal_kind)),
            ));
            crate::flow_js::FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &str_t, upper)
        }
    }
}
