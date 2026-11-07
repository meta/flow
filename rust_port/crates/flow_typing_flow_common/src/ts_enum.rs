/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::flow_symbol;
use flow_common::flow_symbol::Symbol;
use flow_common::js_number;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::reason::mk_annot_reason;
use flow_common::reason::mk_reason;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_type_sig::ts_enum_sig;
use flow_type_sig::type_sig::TsEnumMemberValue;
use flow_typing_context::Context;
use flow_typing_errors::error_message::EnumErrorKind;
use flow_typing_errors::error_message::EnumReferenceData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::TSEnumInvalidMemberData;
use flow_typing_errors::error_message::TSEnumInvalidSyntaxData;
use flow_typing_errors::intermediate_error_types::TsEnumInvalidMemberKind;
use flow_typing_errors::intermediate_error_types::TsEnumInvalidSyntaxKind;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::Literal;
use flow_typing_type::type_::NamedSymbol;
use flow_typing_type::type_::NumberLiteral;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeStrictnessKind;
use flow_typing_type::type_::TypeTKind;
use flow_typing_type::type_util;

use crate::flow_js_utils;

// Construction of the [NamespaceT] representation for a TypeScript enum (in a
// .ts/.d.ts file). The enum is modeled as a [NamespaceT] whose symbol kind is
// [SymbolEnum]: its values_type is an exact object of literal-typed members and
// its types_tmap maps each member to a TypeT-wrapped singleton. The recognizer
// side of this representation (and the value->type transform that turns the
// namespace into the bare union of its member literals) lives in flow_js_utils.rs
// ([is_ts_enum_symbol] / [ts_enum_member_union]), which is consumed by the core
// constraint engine. This module holds the producers: the local-checking path
// ([mk_ts_enum_namespace]) and a shared helper ([mk_ts_enum_namespace_type]) also
// used by the signature path (type_sig_merge.rs's merge_ts_enum_namespace).

// The singleton literal type for one resolved TS enum member, located at [loc]. A
// computed member (an uninitialized member of an ambient enum, or a member flagged
// as a TS error) has no known literal value and is typed as its numeric
// representation; it therefore widens the bare-type union to [number] rather than
// contributing a literal.
pub fn ts_enum_member_type(loc: ALoc, value: &TsEnumMemberValue) -> Type {
    match value {
        TsEnumMemberValue::TSEnumMemberNumber(v, raw) => Type::new(TypeInner::DefT(
            mk_annot_reason(VirtualReasonDesc::RNumberLit(raw.dupe()), loc),
            DefT::new(DefTInner::SingletonNumT {
                from_annot: true,
                value: NumberLiteral(*v, raw.dupe()),
            }),
        )),
        TsEnumMemberValue::TSEnumMemberString(s) => Type::new(TypeInner::DefT(
            mk_annot_reason(VirtualReasonDesc::RStringLit(s.dupe()), loc),
            DefT::new(DefTInner::SingletonStrT {
                from_annot: true,
                value: s.dupe(),
            }),
        )),
        TsEnumMemberValue::TSEnumMemberComputed => Type::new(TypeInner::DefT(
            mk_annot_reason(VirtualReasonDesc::RNumber, loc),
            DefT::new(DefTInner::NumGeneralT(Literal::AnyLiteral)),
        )),
    }
}

// Build the [NamespaceT] (tagged with [namespace_symbol], which must be a
// [SymbolEnum]) for a TS enum from its resolved members, each given as
// [(name, loc, value)]. This is the single source of truth shared by the local
// checking path ([mk_ts_enum_namespace]) and the signature path
// (type_sig_merge.rs's merge_ts_enum_namespace):
// - [values_type] is an exact, read-only object whose members are typed as their
//   singleton literal value (so `E` is a value and `E.A` reads the literal),
// - [types_tmap] maps each member to the same singleton wrapped in a [TypeT] (so
//   `E.A` works as a qualified type), and
// - numeric members additionally contribute a reverse-mapping value property
//   (`<decimal-value-string> -> string`), matching the TS runtime where `E[1]`
//   yields the member-name string (TS types this lookup as `string`). String and
//   computed members have no reverse mapping.
pub fn mk_ts_enum_namespace_type<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    namespace_symbol: Symbol,
    members: &[(FlowSmolStr, ALoc, TsEnumMemberValue)],
    strictness_kind: TypeStrictnessKind,
) -> Type {
    let mk_named_symbol =
        |loc: ALoc, type_: Type| -> NamedSymbol { NamedSymbol::new(Some(loc), None, type_, None) };
    let mut values_map: BTreeMap<Name, NamedSymbol> = BTreeMap::new();
    let mut types_map: BTreeMap<Name, NamedSymbol> = BTreeMap::new();
    for (name, loc, value) in members.iter() {
        let t = ts_enum_member_type(loc.dupe(), value);
        values_map.insert(
            Name::new(name.dupe()),
            mk_named_symbol(loc.dupe(), t.dupe()),
        );
        types_map.insert(
            Name::new(name.dupe()),
            mk_named_symbol(
                loc.dupe(),
                Type::new(TypeInner::DefT(
                    type_util::reason_of_t(&t).dupe(),
                    DefT::new(DefTInner::TypeT(TypeTKind::TypeAliasKind, t.dupe())),
                )),
            ),
        );
        match value {
            TsEnumMemberValue::TSEnumMemberNumber(v, _) => {
                let key = Name::new(FlowSmolStr::from(js_number::ecma_string_of_float(*v)));
                let str_t = Type::new(TypeInner::DefT(
                    mk_reason(VirtualReasonDesc::RString, loc.dupe()),
                    DefT::new(DefTInner::StrGeneralT(Literal::AnyLiteral)),
                ));
                values_map.insert(key, mk_named_symbol(loc.dupe(), str_t));
            }
            _ => {}
        }
    }
    flow_js_utils::namespace_type(
        cx,
        reason,
        namespace_symbol,
        &values_map,
        &types_map,
        strictness_kind,
    )
}

// Build the type of a TypeScript enum (in a .ts/.d.ts file). The result is a
// [NamespaceT] tagged with [SymbolEnum]; the representation and its consequences
// (value object of literal members, `E.A` as a type, the bare-type union, and
// numeric reverse mappings) live in [mk_ts_enum_namespace_type], shared with the
// signature path type_sig_merge.rs's merge_ts_enum_namespace. Member values follow TS
// numbering: defaulted members auto-increment from 0, except in an ambient enum (a
// `declare enum`, or any enum in a .d.ts file), where uninitialized members are
// computed and typed as their representation rather than known literals. Members
// that are invalid in TypeScript (boolean/bigint initializers, or a non-ambient
// defaulted member that does not follow a numeric constant) are reported here as
// errors; the model degrades them to a computed value.
pub fn mk_ts_enum_namespace<'cx>(
    cx: &mut Context<'cx>,
    declared: bool,
    enum_reason: Reason,
    name_loc: ALoc,
    enum_name: FlowSmolStr,
    body: &ast::statement::enum_declaration::Body<ALoc>,
) -> Type {
    let ast::statement::enum_declaration::Body {
        loc: _,
        members,
        explicit_type,
        has_unknown_members,
        comments: _,
    } = body;
    let enum_reference = || EnumReferenceData {
        loc: name_loc.dupe(),
        name: Some(enum_name.dupe()),
    };
    // Flow Enums allow syntax that a TypeScript enum does not: unknown members
    // (`...`) and an explicit representation type (`enum E of string`). These reach
    // this path because [enable_enums] is forced on for .ts/.d.ts files, so report
    // them rather than silently ignoring them.
    if let Some(loc) = has_unknown_members {
        flow_js_utils::add_output_non_speculating(
            cx,
            ErrorMessage::EEnumError(EnumErrorKind::TSEnumInvalidSyntax(Box::new(
                TSEnumInvalidSyntaxData {
                    loc: loc.dupe(),
                    enum_: enum_reference(),
                    kind: TsEnumInvalidSyntaxKind::TSEnumUnknownMembers,
                },
            ))),
        );
    }
    if let Some((loc, _)) = explicit_type {
        flow_js_utils::add_output_non_speculating(
            cx,
            ErrorMessage::EEnumError(EnumErrorKind::TSEnumInvalidSyntax(Box::new(
                TSEnumInvalidSyntaxData {
                    loc: loc.dupe(),
                    enum_: enum_reference(),
                    kind: TsEnumInvalidSyntaxKind::TSEnumExplicitType,
                },
            ))),
        );
    }
    // `ambient` mirrors the signature path's predicate in type_sig_parse.rs (which
    // reads the precomputed [opts.is_dts_file], itself [Files.has_dts_ext]); keep
    // the two in sync.
    let ambient = flow_parser::file_key::has_dts_ext(cx.file().as_str()) || declared;
    let resolved = ts_enum_sig::ts_enum_member_values(ambient, members);
    for (member_name, member_loc, init_loc, _value, error) in resolved.iter() {
        match error {
            None => {}
            Some(ts_enum_sig::TsEnumMemberError::TSEnumMemberInvalidLiteral) => {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EEnumError(EnumErrorKind::TSEnumInvalidMember(Box::new(
                        TSEnumInvalidMemberData {
                            loc: init_loc.dupe(),
                            enum_: enum_reference(),
                            member_name: member_name.to_string(),
                            kind: TsEnumInvalidMemberKind::TSEnumMemberInvalidLiteral,
                        },
                    ))),
                );
            }
            Some(ts_enum_sig::TsEnumMemberError::TSEnumMemberMissingInitializer) => {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EEnumError(EnumErrorKind::TSEnumInvalidMember(Box::new(
                        TSEnumInvalidMemberData {
                            loc: member_loc.dupe(),
                            enum_: enum_reference(),
                            member_name: member_name.to_string(),
                            kind: TsEnumInvalidMemberKind::TSEnumMemberMissingInitializer,
                        },
                    ))),
                );
            }
            Some(ts_enum_sig::TsEnumMemberError::TSEnumMemberNumericName) => {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EEnumError(EnumErrorKind::TSEnumInvalidMember(Box::new(
                        TSEnumInvalidMemberData {
                            loc: member_loc.dupe(),
                            enum_: enum_reference(),
                            member_name: member_name.to_string(),
                            kind: TsEnumInvalidMemberKind::TSEnumMemberNumericName,
                        },
                    ))),
                );
            }
        }
    }
    let members: Vec<(FlowSmolStr, ALoc, TsEnumMemberValue)> = resolved
        .into_iter()
        .map(|(name, member_loc, _init_loc, value, _error)| (name, member_loc, value))
        .collect();
    let namespace_symbol = flow_symbol::Symbol::mk_enum_symbol(enum_name, name_loc);
    let strictness_kind = cx.type_strictness_kind();
    mk_ts_enum_namespace_type(cx, enum_reason, namespace_symbol, &members, strictness_kind)
}
