/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::fmt::Display;
use std::fmt::Write;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::reason::Name;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast::types::TypeGuardKind;
use flow_parser_utils_output::layout;
use flow_parser_utils_output::layout::LayoutNode;
use flow_parser_utils_output::pretty_printer;

use crate::ty::*;
use crate::ty_symbol::*;

/***********
 * Utils   *
 ***********/

const CROP_SYMBOL: &str = "...";

struct PrintState<L> {
    remaining: usize,
    printed_symbols: Option<Vec<Symbol<L>>>,
    truncated: bool,
}

pub fn better_quote(prefer_single_quotes: bool, s: &str) -> String {
    let single_quotes = s.chars().filter(|&c| c == '\'').count();
    let double_quotes = s.chars().filter(|&c| c == '"').count();

    let quote_char = if prefer_single_quotes {
        if single_quotes <= double_quotes {
            '\''
        } else {
            '"'
        }
    } else if double_quotes <= single_quotes {
        '"'
    } else {
        '\''
    };
    quote_char.to_string()
}

pub fn property_key_quotes_needed(x: &str) -> bool {
    if x.is_empty() {
        return true;
    }

    let mut chars = x.chars();
    let first = chars.next().unwrap();

    if !first.is_ascii_alphabetic() && first != '$' && first != '_' {
        return true;
    }

    for c in chars {
        if !c.is_ascii_alphanumeric() && c != '$' && c != '_' {
            return true;
        }
    }

    false
}

pub fn utf8_escape(quote: &str, s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        let cp = c as u32;
        let next_cp = chars.peek().map(|next| *next as u32);
        match cp {
            0x0 => {
                let zero = match next_cp {
                    Some(n) if (0x30..=0x39).contains(&n) => "\\x00",
                    _ => "\\0",
                };
                result.push_str(zero);
            }
            0x8 => result.push_str("\\b"),
            0x9 => result.push_str("\\t"),
            0xA => result.push_str("\\n"),
            0xB => result.push_str("\\v"),
            0xC => result.push_str("\\f"),
            0xD => result.push_str("\\r"),
            0x22 if quote == "\"" => result.push_str("\\\""),
            0x27 if quote == "'" => result.push_str("\\'"),
            0x5C => result.push_str("\\\\"),
            n if 0x1F < n && n < 0x7F => result.push(c),
            n if n < 0x100 => {
                write!(result, "\\x{:02x}", n).unwrap();
            }
            n if n < 0x10000 => {
                write!(result, "\\u{:04x}", n).unwrap();
            }
            n => {
                let n2 = n - 0x10000;
                let hi = 0xD800 | (n2 >> 10);
                let lo = 0xDC00 | (n2 & 0x3FF);
                write!(result, "\\u{:4x}", hi).unwrap();
                write!(result, "\\u{:4x}", lo).unwrap();
            }
        }
    }

    result
}

pub fn variance_string(p: Polarity) -> &'static str {
    match p {
        Polarity::Positive => "+",
        Polarity::Negative => "-",
        Polarity::Neutral => "",
    }
}

fn crop_atom<L>(state: &mut PrintState<L>) -> LayoutNode {
    state.truncated = true;
    LayoutNode::atom(CROP_SYMBOL.to_string())
}

fn crop_atom_with_count<L>(
    state: &mut PrintState<L>,
    remaining: usize,
    singular: &str,
    plural: &str,
) -> LayoutNode {
    state.truncated = true;
    let item = if remaining == 1 { singular } else { plural };
    LayoutNode::atom(format!("... {remaining} more {item} ..."))
}

fn record_symbol<L: Clone>(state: &mut PrintState<L>, symbol: &Symbol<L>) {
    if let Some(symbols) = &mut state.printed_symbols {
        symbols.push(symbol.clone());
    }
}

fn symbol_identifier<L: Dupe>(state: &mut PrintState<L>, symbol: &Symbol<L>) -> LayoutNode {
    record_symbol(state, symbol);
    identifier(&local_name_of_symbol(symbol))
}

/// The name in the head of a declaration (`type X = …`, `class X`). A name the
/// declaration is imported under on its own is used, as for references. A
/// qualified name reached through a namespace import (`Types.X`) is not: the
/// head names the declaration itself, so it reads `class X`.
fn head_name_of_symbol<L>(symbol: &Symbol<L>) -> FlowSmolStr {
    let name = local_name_of_symbol(symbol);
    if name.contains('.') {
        symbol.sym_name.dupe()
    } else {
        name
    }
}

fn declaration_identifier<L: Dupe>(state: &mut PrintState<L>, symbol: &Symbol<L>) -> LayoutNode {
    record_symbol(state, symbol);
    identifier(&head_name_of_symbol(symbol))
}

fn in_quotes(prefer_single_quotes: bool, s: &str) -> Vec<LayoutNode> {
    let quote = better_quote(prefer_single_quotes, s);
    let escaped = utf8_escape(&quote, s);
    vec![
        LayoutNode::atom(quote.clone()),
        LayoutNode::atom(escaped),
        LayoutNode::atom(quote),
    ]
}

fn identifier(name: &impl Display) -> LayoutNode {
    LayoutNode::atom(name.to_string())
}

fn property_variance(p: Polarity) -> LayoutNode {
    match p {
        Polarity::Positive => layout::fuse(vec![
            LayoutNode::atom("readonly".to_string()),
            layout::space(),
        ]),
        Polarity::Negative => layout::fuse(vec![
            LayoutNode::atom("writeonly".to_string()),
            layout::space(),
        ]),
        Polarity::Neutral => LayoutNode::empty(),
    }
}

fn type_param_variance(p: Polarity) -> LayoutNode {
    match p {
        Polarity::Positive => {
            layout::fuse(vec![LayoutNode::atom("out".to_string()), layout::space()])
        }
        Polarity::Negative => {
            layout::fuse(vec![LayoutNode::atom("in".to_string()), layout::space()])
        }
        Polarity::Neutral => LayoutNode::empty(),
    }
}

fn wrap_in_parens(node: LayoutNode) -> LayoutNode {
    layout::fuse(vec![
        LayoutNode::atom("(".to_string()),
        node,
        LayoutNode::atom(")".to_string()),
    ])
}

fn local_name_of_symbol<L>(symbol: &Symbol<L>) -> FlowSmolStr {
    // If the type is imported use the local name
    match &symbol.sym_provenance {
        Provenance::Remote(RemoteInfo {
            imported_as: Some(ImportedIdent(_, name, _)),
        })
        | Provenance::Library(RemoteInfo {
            imported_as: Some(ImportedIdent(_, name, _)),
        }) => FlowSmolStr::new(name.as_str()),
        _ => symbol.sym_name.dupe(),
    }
}

fn builtin_value<L: Dupe>(pv: &BuiltinOrSymbol<L>, state: &mut PrintState<L>) -> LayoutNode {
    match pv {
        BuiltinOrSymbol::FunProto => LayoutNode::atom("Function.prototype".to_string()),
        BuiltinOrSymbol::ObjProto => LayoutNode::atom("Object.prototype".to_string()),
        BuiltinOrSymbol::TSymbol(symbol) => {
            record_symbol(state, symbol);
            LayoutNode::atom(local_name_of_symbol(symbol).to_string())
        }
    }
}

fn option<T, F>(f: F, opt: &Option<T>) -> LayoutNode
where
    F: FnOnce(&T) -> LayoutNode,
{
    match opt {
        Some(val) => f(val),
        None => LayoutNode::empty(),
    }
}

/* Main Transformation */

fn layout_of_elt_with_state<L: Dupe>(
    opts: &PrinterOptions,
    elt: &Elt<L>,
    collect_symbols: bool,
) -> (LayoutNode, Vec<Symbol<L>>, bool) {
    let mut state = PrintState {
        remaining: opts.size,
        printed_symbols: collect_symbols.then(Vec::new),
        truncated: false,
    };
    let layout = match elt {
        Elt::Type(t) => type_(opts, 0, t, &mut state),
        Elt::Decl(d) => decl(opts, 0, d, &mut state),
    };
    (
        layout,
        state.printed_symbols.unwrap_or_default(),
        state.truncated,
    )
}

fn layout_of_elt<L: Dupe>(opts: &PrinterOptions, elt: &Elt<L>) -> LayoutNode {
    layout_of_elt_with_state(opts, elt, false).0
}

// The depth parameter is useful for formatting unions: Top-level does not
// get parentheses.
fn type_<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    t: &Ty<L>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    let depth = depth + 1;
    if size.remaining == 0 {
        crop_atom(size)
    } else {
        size.remaining -= 1;
        type_impl(opts, depth, t, size)
    }
}

fn type_impl<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    t: &Ty<L>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    match t {
        Ty::Truncated => crop_atom(size),
        Ty::Bound(data) => {
            let (_, name) = data.as_ref();
            LayoutNode::atom(name.to_string())
        }
        Ty::Any(_) => LayoutNode::atom("any".to_string()),
        Ty::Top => LayoutNode::atom("unknown".to_string()),
        Ty::Bot(_) => LayoutNode::atom("empty".to_string()),
        Ty::Void | Ty::TsVoid => LayoutNode::atom("void".to_string()),
        Ty::Undefined => LayoutNode::atom("undefined".to_string()),
        Ty::Null => LayoutNode::atom("null".to_string()),
        Ty::Num => LayoutNode::atom("number".to_string()),
        Ty::Str => LayoutNode::atom("string".to_string()),
        Ty::Bool => LayoutNode::atom("boolean".to_string()),
        Ty::BigInt => LayoutNode::atom("bigint".to_string()),
        Ty::Symbol => LayoutNode::atom("symbol".to_string()),
        Ty::UniqueSymbol => LayoutNode::atom("unique symbol".to_string()),

        Ty::StrLit(raw) => layout::fuse(in_quotes(opts.prefer_single_quotes, raw)),
        Ty::NumLit(raw) => LayoutNode::atom(raw.to_string()),
        Ty::BoolLit(value) => LayoutNode::atom(if *value { "true" } else { "false" }.to_string()),
        Ty::BigIntLit(raw) => LayoutNode::atom(raw.to_string()),

        Ty::Fun(func) => type_function(opts, depth, &arrow_separator(), func, None, size),
        Ty::Obj(obj) => type_object(opts, depth, obj, size),
        Ty::Arr(arr) => type_array(opts, depth, arr, size),
        Ty::Generic(g) => type_generic(opts, depth, g, size),
        Ty::Union(_, t1, t2, ts) => {
            let mut all = vec![t1.as_ref()];
            all.push(t2.as_ref());
            for t in ts.iter() {
                all.push(t.as_ref());
            }
            type_union(opts, depth, false, &all, size)
        }
        Ty::Inter(t1, t2, ts) => {
            let mut all = vec![t1.as_ref()];
            all.push(t2.as_ref());
            for t in ts.iter() {
                all.push(t.as_ref());
            }
            type_intersection(opts, depth, false, &all, size)
        }

        Ty::Utility(u) => utility(opts, depth, u, size),

        Ty::IndexedAccess {
            _object,
            index,
            optional,
        } => {
            let left_delim = if *optional { "?.[" } else { "[" };
            layout::fuse(vec![
                indexed_object_with_parens(opts, depth, _object.as_ref(), size),
                LayoutNode::atom(left_delim.to_string()),
                type_(opts, depth, index.as_ref(), size),
                LayoutNode::atom("]".to_string()),
            ])
        }

        Ty::Tup { elements, inexact } => {
            let mut elts_rev = Vec::new();
            for (idx, elem) in elements.iter().enumerate() {
                if size.remaining == 0 {
                    elts_rev.push(crop_atom_with_count(
                        size,
                        elements.len() - idx,
                        "tuple element",
                        "tuple elements",
                    ));
                    break;
                }
                elts_rev.push(tuple_element(opts, depth, idx, elem, size));
            }
            if *inexact {
                elts_rev.push(LayoutNode::atom("...".to_string()));
            }
            layout::list(
                None,
                Some((
                    LayoutNode::atom("[".to_string()),
                    LayoutNode::atom("]".to_string()),
                )),
                Some(LayoutNode::atom(",".to_string())),
                false,
                None,
                None,
                elts_rev,
            )
        }

        Ty::InlineInterface(iface) => type_interface(
            opts,
            depth,
            &iface.if_extends,
            &iface.if_props,
            &iface.if_dict,
            size,
        ),

        Ty::TypeOf(data) => {
            let (pv, targs) = data.as_ref();
            layout::fuse(vec![
                LayoutNode::atom("typeof".to_string()),
                layout::space(),
                builtin_value(pv, size),
                option(|args| type_args(opts, depth, args, size), targs),
            ])
        }

        Ty::Conditional {
            check_type,
            extends_type,
            true_type,
            false_type,
        } => layout::group(vec![
            layout::fuse(vec![
                type_with_parens(opts, depth, check_type.as_ref(), size),
                layout::space(),
                LayoutNode::atom("extends".to_string()),
                layout::space(),
                type_with_parens(opts, depth, extends_type.as_ref(), size),
            ]),
            LayoutNode::indent(layout::fuse(vec![
                layout::pretty_line(),
                LayoutNode::atom("?".to_string()),
                layout::pretty_space(),
                type_(opts, depth, true_type.as_ref(), size),
                layout::pretty_line(),
                LayoutNode::atom(":".to_string()),
                layout::pretty_space(),
                type_(opts, depth, false_type.as_ref(), size),
            ])),
        ]),

        Ty::Infer(data) => {
            let (symbol, bound) = data.as_ref();
            record_symbol(size, symbol);
            layout::fuse(vec![
                LayoutNode::atom("infer".to_string()),
                layout::space(),
                layout::fuse(vec![
                    identifier(&symbol.sym_name),
                    option(
                        |t| {
                            layout::fuse(vec![
                                layout::space(),
                                LayoutNode::atom("extends".to_string()),
                                layout::space(),
                                type_(opts, depth, t.as_ref(), size),
                            ])
                        },
                        bound,
                    ),
                ]),
            ])
        }

        Ty::Component {
            regular_props,
            renders,
        } => layout::fuse(vec![
            LayoutNode::atom("component".to_string()),
            type_component_sig(opts, depth, regular_props, renders.as_ref(), size),
        ]),

        Ty::Renders(t, variant) => {
            let renders_str = match variant {
                RendersKind::RendersNormal => "renders",
                RendersKind::RendersMaybe => "renders?",
                RendersKind::RendersStar => "renders*",
            };
            layout::fuse(vec![
                LayoutNode::atom(renders_str.to_string()),
                layout::space(),
                type_with_parens(opts, depth, t.as_ref(), size),
            ])
        }

        Ty::TemplateLiteral { quasis, types } => {
            fn escape_quasi(s: &str) -> String {
                let bytes = s.as_bytes();
                let mut out = String::with_capacity(s.len());
                let mut i = 0;
                while i < bytes.len() {
                    let c = bytes[i] as char;
                    match c {
                        '\\' => out.push_str("\\\\"),
                        '`' => out.push_str("\\`"),
                        '$' if i + 1 < bytes.len() && bytes[i + 1] as char == '{' => {
                            out.push_str("\\$")
                        }
                        _ => out.push(c),
                    }
                    i += 1;
                }
                out
            }
            let mut parts = vec![LayoutNode::atom("`".to_string())];
            let mut quasis_iter = quasis.iter();
            if let Some(q) = quasis_iter.next() {
                parts.push(LayoutNode::atom(escape_quasi(q)));
            }
            for (t, q) in types.iter().zip(quasis_iter) {
                parts.push(LayoutNode::atom("${".to_string()));
                parts.push(type_impl(opts, depth, t.as_ref(), size));
                parts.push(LayoutNode::atom("}".to_string()));
                parts.push(LayoutNode::atom(escape_quasi(q)));
            }
            parts.push(LayoutNode::atom("`".to_string()));
            layout::fuse(parts)
        }
    }
}

fn arrow_separator() -> LayoutNode {
    layout::fuse(vec![
        layout::pretty_space(),
        LayoutNode::atom("=>".to_string()),
    ])
}

fn type_function<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    sep: &LayoutNode,
    func: &FunT<L>,
    targs: Option<&[Arc<Ty<L>>]>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    let signature = function_signature(opts, depth, sep, func, targs, size);
    if matches!(func.fun_effect, FunEffect::Hook) {
        layout::fuse(vec![
            LayoutNode::atom("hook".to_string()),
            layout::pretty_space(),
            signature,
        ])
    } else {
        signature
    }
}

/// A function type without its `hook` keyword: type parameters, parameters,
/// `sep`, and the return type. A signature instantiated by a call shows the
/// call's type arguments `targs` in place of its type parameters.
fn function_signature<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    sep: &LayoutNode,
    func: &FunT<L>,
    targs: Option<&[Arc<Ty<L>>]>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    let mut params = Vec::new();
    let mut limit_reached = false;
    if let Some(t) = &func.fun_this_param {
        if size.remaining == 0 {
            params.push(crop_atom_with_count(
                size,
                1 + func.fun_params.len(),
                "parameter",
                "parameters",
            ));
            limit_reached = true;
        } else {
            params.push(type_function_param(
                opts,
                depth,
                Some("this"),
                t,
                &FunParam {
                    prm_optional: false,
                },
                size,
            ));
        }
    }

    for (idx, (name, t, fun_param)) in func.fun_params.iter().enumerate() {
        if limit_reached {
            break;
        }
        if size.remaining == 0 {
            params.push(crop_atom_with_count(
                size,
                func.fun_params.len() - idx,
                "parameter",
                "parameters",
            ));
            break;
        }
        params.push(type_function_param(
            opts,
            depth,
            name.as_ref().map(|name| name.as_str()),
            t,
            fun_param,
            size,
        ));
    }

    if let Some((name, t)) = &func.fun_rest_param {
        let rest_param = type_function_param(
            opts,
            depth,
            name.as_ref().map(|name| name.as_str()),
            t,
            &FunParam {
                prm_optional: false,
            },
            size,
        );
        params.push(layout::fuse(vec![
            LayoutNode::atom("...".to_string()),
            rest_param,
        ]));
    }

    let type_params = match (targs, &func.fun_type_params) {
        (Some(targs), _) => type_args(opts, depth, targs, size),
        (None, Some(tparams)) => type_parameter(opts, depth, tparams, size),
        (None, None) => LayoutNode::empty(),
    };

    let params_list = layout::list(
        None, // break_mode
        Some((
            LayoutNode::atom("(".to_string()),
            LayoutNode::atom(")".to_string()),
        )), // wrap
        Some(LayoutNode::atom(",".to_string())), // sep
        false, // trailing
        None, // inline
        None, // indent
        params,
    );

    let return_type = match &func.fun_return {
        ReturnT::ReturnType(t) => type_after_separator(opts, depth, t, size),
        ret @ ReturnT::TypeGuard(..) => layout::fuse(vec![
            layout::pretty_space(),
            return_t(opts, depth, ret, size),
        ]),
    };

    layout::fuse(vec![type_params, params_list, sep.clone(), return_type])
}

fn type_function_param<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    name: Option<&str>,
    annot: &std::sync::Arc<Ty<L>>,
    fun_param: &FunParam,
    size: &mut PrintState<L>,
) -> LayoutNode {
    match name {
        Some(id) => layout::fuse(vec![
            LayoutNode::atom(id.to_string()),
            if fun_param.prm_optional {
                LayoutNode::atom("?".to_string())
            } else {
                LayoutNode::empty()
            },
            LayoutNode::atom(":".to_string()),
            type_after_separator(opts, depth, annot.as_ref(), size),
        ]),
        None => type_(opts, depth, annot.as_ref(), size),
    }
}

fn return_t<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    ret: &ReturnT<L>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    match ret {
        ReturnT::ReturnType(t) => type_(opts, depth, t.as_ref(), size),
        ReturnT::TypeGuard(kind, x, t) => {
            let mut elements = Vec::new();
            match kind {
                TypeGuardKind::Default => (),
                TypeGuardKind::Implies => elements.push(LayoutNode::atom("implies".to_string())),
                TypeGuardKind::Asserts => elements.push(LayoutNode::atom("asserts".to_string())),
            }
            elements.push(LayoutNode::atom(x.to_string()));
            if let Some(t) = t {
                elements.push(LayoutNode::atom("is".to_string()));
                elements.push(type_(opts, depth, t.as_ref(), size));
            }
            layout::fuse_with_space(elements)
        }
    }
}

fn type_parameter<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    params: &[TypeParam<L>],
    size: &mut PrintState<L>,
) -> LayoutNode {
    let mut elements = Vec::new();
    for (idx, param) in params.iter().enumerate() {
        if size.remaining == 0 {
            elements.push(crop_atom_with_count(
                size,
                params.len() - idx,
                "type parameter",
                "type parameters",
            ));
            break;
        }
        elements.push(type_param(opts, depth, param, size));
    }

    layout::list(
        None,
        Some((
            LayoutNode::atom("<".to_string()),
            LayoutNode::atom(">".to_string()),
        )),
        Some(LayoutNode::atom(",".to_string())),
        false,
        None,
        None,
        elements,
    )
}

fn type_param<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    param: &TypeParam<L>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    let const_prefix = if param.tp_const {
        layout::fuse(vec![LayoutNode::atom("const".to_string()), layout::space()])
    } else {
        LayoutNode::empty()
    };

    let bound = match &param.tp_bound {
        Some(t) => type_param_bound(opts, depth, t.as_ref(), size),
        _ => LayoutNode::empty(),
    };

    let default = match &param.tp_default {
        Some(t) => layout::fuse(vec![
            layout::pretty_space(),
            LayoutNode::atom("=".to_string()),
            type_after_separator(opts, depth, t.as_ref(), size),
        ]),
        _ => LayoutNode::empty(),
    };

    layout::fuse(vec![
        const_prefix,
        type_param_variance(param.tp_polarity),
        LayoutNode::atom(param.tp_name.to_string()),
        bound,
        default,
    ])
}

fn type_param_bound<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    t: &Ty<L>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    layout::fuse(vec![
        layout::pretty_space(),
        LayoutNode::atom("extends".to_string()),
        type_after_separator(opts, depth, t, size),
    ])
}

fn type_object<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    obj: &ObjT<L>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    let mut props = Vec::new();
    for (idx, prop) in obj.obj_props.iter().enumerate() {
        if size.remaining == 0 {
            props.push(crop_atom_with_count(
                size,
                obj.obj_props.len() - idx,
                "property",
                "properties",
            ));
            break;
        }
        props.push(type_object_property(opts, depth, prop, size));
    }

    match &obj.obj_kind {
        ObjKind::IndexedObj(d) => {
            props.insert(0, type_dict(opts, depth, d, size));
        }
        ObjKind::InexactObj => {
            props.push(LayoutNode::atom("...".to_string()));
        }
        _ => {}
    }

    layout::list(
        None,
        Some((
            LayoutNode::atom("{".to_string()),
            LayoutNode::atom("}".to_string()),
        )),
        Some(LayoutNode::atom(",".to_string())),
        false,
        None,
        None,
        props,
    )
}

fn type_object_property<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    prop: &Prop<L>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    let to_key = |name: &Name| -> LayoutNode {
        // A symbol key is rendered TypeScript-style as a bracketed computed key
        // (e.g. `[sym]`, or `[symbol]` when the name is unknown), never quoted.
        if name.is_symbol() {
            return LayoutNode::atom(name.to_string());
        }
        let name_str = name.to_string();
        if property_key_quotes_needed(&name_str) {
            let quote = better_quote(opts.prefer_single_quotes, &name_str);
            layout::fuse(vec![
                LayoutNode::atom(quote.clone()),
                LayoutNode::atom(utf8_escape(&quote, &name_str)),
                LayoutNode::atom(quote),
            ])
        } else {
            identifier(name)
        }
    };

    match prop {
        Prop::NamedProp {
            name,
            prop: named_prop,
            ..
        } => match named_prop {
            NamedProp::Field {
                t,
                polarity,
                optional,
            } => layout::fuse(vec![
                property_variance(*polarity),
                to_key(name),
                if *optional {
                    LayoutNode::atom("?".to_string())
                } else {
                    LayoutNode::empty()
                },
                LayoutNode::atom(":".to_string()),
                type_after_separator(opts, depth, t.as_ref(), size),
            ]),
            NamedProp::Method {
                fun: func,
                optional,
            } => layout::fuse(vec![
                to_key(name),
                if *optional {
                    LayoutNode::atom("?".to_string())
                } else {
                    LayoutNode::empty()
                },
                type_function(
                    opts,
                    depth,
                    &LayoutNode::atom(":".to_string()),
                    func,
                    None,
                    size,
                ),
            ]),
            NamedProp::Get(t) => layout::group(vec![layout::fuse(vec![
                LayoutNode::atom("get".to_string()),
                layout::space(),
                to_key(name),
                LayoutNode::atom("(".to_string()),
                layout::softline(),
                LayoutNode::atom(")".to_string()),
                LayoutNode::atom(":".to_string()),
                type_after_separator(opts, depth, t.as_ref(), size),
            ])]),
            NamedProp::Set(t) => layout::group(vec![layout::fuse(vec![
                LayoutNode::atom("set".to_string()),
                layout::space(),
                to_key(name),
                layout::wrap_and_indent(
                    (
                        LayoutNode::atom("(".to_string()),
                        LayoutNode::atom(")".to_string()),
                    ),
                    None,
                    vec![type_(opts, depth, t.as_ref(), size)],
                ),
                LayoutNode::atom(":".to_string()),
                layout::pretty_space(),
                type_(opts, depth, &Ty::<L>::Void, size),
            ])]),
        },
        Prop::CallProp(func) => type_function(
            opts,
            depth,
            &LayoutNode::atom(":".to_string()),
            func,
            None,
            size,
        ),
        Prop::SpreadProp(t) => layout::fuse(vec![
            LayoutNode::atom("...".to_string()),
            type_(opts, depth, t.as_ref(), size),
        ]),
        Prop::MappedTypeProp {
            key_tparam,
            source,
            prop,
            name_type,
            flags,
            homomorphic,
        } => {
            let optional_modifier = match flags.optional {
                MappedTypeOptionalFlag::RemoveOptional => LayoutNode::atom("-?".to_string()),
                MappedTypeOptionalFlag::MakeOptional => LayoutNode::atom("?".to_string()),
                MappedTypeOptionalFlag::KeepOptionality => LayoutNode::empty(),
            };
            let variance_token = match flags.variance {
                MappedTypeVariance::OverrideVariance(pol) => property_variance(pol),
                MappedTypeVariance::RemoveVariance(Polarity::Positive) => layout::fuse(vec![
                    LayoutNode::atom("-".to_string()),
                    LayoutNode::atom("readonly ".to_string()),
                ]),
                MappedTypeVariance::RemoveVariance(Polarity::Negative)
                | MappedTypeVariance::RemoveVariance(Polarity::Neutral)
                | MappedTypeVariance::KeepVariance => LayoutNode::empty(),
            };
            let as_clause = match name_type {
                None => LayoutNode::empty(),
                Some(nt) => layout::fuse(vec![
                    LayoutNode::atom(" as ".to_string()),
                    type_(opts, depth, nt.as_ref(), size),
                ]),
            };

            layout::fuse(vec![
                variance_token,
                LayoutNode::atom("[".to_string()),
                LayoutNode::atom(key_tparam.tp_name.to_string()),
                LayoutNode::atom(" in ".to_string()),
                match homomorphic {
                    MappedTypeHomomorphicFlag::Homomorphic => {
                        LayoutNode::atom("keyof ".to_string())
                    }
                    MappedTypeHomomorphicFlag::SemiHomomorphic(_)
                    | MappedTypeHomomorphicFlag::Unspecialized => LayoutNode::empty(),
                },
                type_(opts, depth, source.as_ref(), size),
                as_clause,
                LayoutNode::atom("]".to_string()),
                optional_modifier,
                LayoutNode::atom(":".to_string()),
                type_after_separator(opts, depth, prop.as_ref(), size),
            ])
        }
    }
}

fn type_dict<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    dict: &Dict<L>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    let name_part = match &dict.dict_name {
        Some(id) => layout::fuse(vec![
            LayoutNode::atom(id.to_string()),
            LayoutNode::atom(":".to_string()),
            layout::pretty_space(),
        ]),
        None => LayoutNode::empty(),
    };

    layout::fuse(vec![
        property_variance(dict.dict_polarity),
        LayoutNode::atom("[".to_string()),
        name_part,
        type_(opts, depth, dict.dict_key.as_ref(), size),
        LayoutNode::atom("]".to_string()),
        LayoutNode::atom(":".to_string()),
        type_after_separator(opts, depth, dict.dict_value.as_ref(), size),
    ])
}

fn type_interface<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    extends: &[GenericT<L>],
    props: &[Prop<L>],
    dict: &Option<Dict<L>>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    let extends_node = if extends.is_empty() {
        LayoutNode::empty()
    } else {
        let mut extend_list = Vec::new();
        for (idx, g) in extends.iter().enumerate() {
            if size.remaining == 0 {
                extend_list.push(crop_atom_with_count(
                    size,
                    extends.len() - idx,
                    "parent type",
                    "parent types",
                ));
                break;
            }
            extend_list.push(type_generic(opts, depth, g, size));
        }
        layout::fuse_with_space(vec![
            LayoutNode::atom("extends".to_string()),
            layout::list(
                None,
                None,
                Some(LayoutNode::atom(",".to_string())),
                false,
                None,
                None,
                extend_list,
            ),
        ])
    };

    let mut properties = Vec::new();
    for (idx, prop) in props.iter().enumerate() {
        if size.remaining == 0 {
            properties.push(crop_atom_with_count(
                size,
                props.len() - idx,
                "property",
                "properties",
            ));
            break;
        }
        properties.push(type_object_property(opts, depth, prop, size));
    }

    if let Some(d) = dict {
        properties.push(type_dict(opts, depth, d, size));
    }

    let body = layout::list(
        None,
        Some((
            LayoutNode::atom("{".to_string()),
            LayoutNode::atom("}".to_string()),
        )),
        Some(LayoutNode::atom(";".to_string())),
        false,
        None,
        None,
        properties,
    );

    layout::fuse_with_space(vec![
        LayoutNode::atom("interface".to_string()),
        extends_node,
        body,
    ])
}

fn type_component_sig<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    regular_props: &ComponentProps<L>,
    renders: Option<&Arc<Ty<L>>>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    let to_key = |name: &Name| -> LayoutNode {
        let name_str = name.to_string();
        if property_key_quotes_needed(&name_str) {
            let quote = better_quote(opts.prefer_single_quotes, &name_str);
            layout::fuse(vec![
                LayoutNode::atom(quote.clone()),
                LayoutNode::atom(utf8_escape(&quote, &name_str)),
                LayoutNode::atom(quote),
            ])
        } else {
            LayoutNode::atom(name_str)
        }
    };

    let params = match regular_props {
        ComponentProps::UnflattenedComponentProps(t) => {
            vec![layout::fuse(vec![
                LayoutNode::atom("...".to_string()),
                type_(opts, depth, t.as_ref(), size),
            ])]
        }
        ComponentProps::FlattenedComponentProps {
            props,
            inexact,
            dict,
        } => {
            let mut params_list = Vec::new();
            for (idx, prop) in props.iter().enumerate() {
                if size.remaining == 0 {
                    params_list.push(crop_atom_with_count(
                        size,
                        props.len() - idx,
                        "component parameter",
                        "component parameters",
                    ));
                    break;
                }
                let FlattenedComponentProp::FlattenedComponentProp {
                    name, optional, t, ..
                } = prop;
                params_list.push(layout::fuse(vec![
                    to_key(name),
                    if *optional {
                        LayoutNode::atom("?".to_string())
                    } else {
                        LayoutNode::empty()
                    },
                    LayoutNode::atom(":".to_string()),
                    type_after_separator(opts, depth, t.as_ref(), size),
                ]));
            }
            if *inexact {
                params_list.push(LayoutNode::atom("...{...}".to_string()));
            }
            if let Some(dict) = dict {
                params_list.push(layout::fuse(vec![
                    LayoutNode::atom("...".to_string()),
                    LayoutNode::atom("{".to_string()),
                    type_dict(opts, depth, dict, size),
                    LayoutNode::atom("}".to_string()),
                ]));
            }
            params_list
        }
    };

    let renders_node = match renders {
        None => LayoutNode::empty(),
        Some(t) => match t.as_ref() {
            Ty::Renders(_, _) => {
                layout::fuse(vec![layout::space(), type_(opts, depth, t.as_ref(), size)])
            }
            _ => layout::fuse(vec![
                layout::space(),
                LayoutNode::atom("renders".to_string()),
                layout::space(),
                type_with_parens(opts, depth, t.as_ref(), size),
            ]),
        },
    };

    layout::fuse(vec![
        layout::list(
            None,
            Some((
                LayoutNode::atom("(".to_string()),
                LayoutNode::atom(")".to_string()),
            )),
            Some(LayoutNode::atom(",".to_string())),
            false,
            None,
            None,
            params,
        ),
        renders_node,
    ])
}

fn tuple_element<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    idx: usize,
    elem: &TupleElement<L>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    match elem {
        TupleElement::TupleElement {
            name,
            t,
            polarity,
            optional,
        } => {
            let opt_node = if *optional {
                LayoutNode::atom("?".to_string())
            } else {
                LayoutNode::empty()
            };

            let id_nodes = |id: &str, size: &mut PrintState<L>| -> LayoutNode {
                layout::fuse(vec![
                    LayoutNode::atom(id.to_string()),
                    opt_node.clone(),
                    LayoutNode::atom(":".to_string()),
                    type_after_separator(opts, depth, t.as_ref(), size),
                ])
            };

            let element = match (name, polarity, optional) {
                (Some(n), _, _) => id_nodes(n.as_str(), size),
                (None, Polarity::Neutral, false) => type_(opts, depth, t.as_ref(), size),
                // We cannot omit the name of a non-neutral variance or optional
                // element. If the name is missing, add a default name "_".
                (None, _, _) => id_nodes(&format!("_{}", idx), size),
            };

            layout::fuse(vec![property_variance(*polarity), element])
        }
        TupleElement::TupleSpread { name, t } => {
            let name_part = match name {
                Some(n) => layout::fuse(vec![
                    LayoutNode::atom(n.to_string()),
                    LayoutNode::atom(":".to_string()),
                    layout::pretty_space(),
                ]),
                None => LayoutNode::empty(),
            };
            layout::fuse(vec![
                LayoutNode::atom("...".to_string()),
                name_part,
                type_(opts, depth, t.as_ref(), size),
            ])
        }
    }
}

fn utility<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    u: &Utility<L>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    use crate::ty::string_of_utility_ctor;
    use crate::ty::types_of_utility;

    match u {
        Utility::Keys(t) => layout::fuse(vec![
            LayoutNode::atom("keyof".to_string()),
            layout::space(),
            type_with_parens(opts, depth, t.as_ref(), size),
        ]),
        Utility::ElementType(object, index) => layout::fuse(vec![
            indexed_object_with_parens(opts, depth, object.as_ref(), size),
            LayoutNode::atom("[".to_string()),
            type_(opts, depth, index.as_ref(), size),
            LayoutNode::atom("]".to_string()),
        ]),
        _ => {
            let ctor = string_of_utility_ctor(u);
            let ts = types_of_utility(u);
            type_reference(
                opts,
                depth,
                identifier(&Name::new(ctor)),
                ts.as_deref(),
                size,
            )
        }
    }
}

fn type_array<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    arr: &ArrT<L>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    let arr_name = if arr.arr_readonly {
        "ReadonlyArray"
    } else {
        "Array"
    };
    match arr.arr_elt_t.as_ref() {
        // A union that does not fit needs lines of its own for its members.
        Ty::Union(..) => type_reference(
            opts,
            depth,
            LayoutNode::atom(arr_name.to_string()),
            Some(std::slice::from_ref(&arr.arr_elt_t)),
            size,
        ),
        _ => layout::fuse(vec![
            LayoutNode::atom(arr_name.to_string()),
            LayoutNode::atom("<".to_string()),
            type_(opts, depth, &arr.arr_elt_t, size),
            LayoutNode::atom(">".to_string()),
        ]),
    }
}

fn type_generic<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    g: &GenericT<L>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    let (symbol, _, targs) = g;
    let name = symbol_identifier(size, symbol);
    type_reference(opts, depth, name, targs.as_deref(), size)
}

fn type_reference<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    name: LayoutNode,
    targs: Option<&[std::sync::Arc<Ty<L>>]>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    let targs_layout = match targs {
        Some(args) => type_args(opts, depth, args, size),
        None => LayoutNode::empty(),
    };
    layout::fuse(vec![name, targs_layout])
}

fn type_args<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    targs: &[std::sync::Arc<Ty<L>>],
    size: &mut PrintState<L>,
) -> LayoutNode {
    let mut elements = Vec::new();
    for (idx, arg) in targs.iter().enumerate() {
        if size.remaining == 0 {
            elements.push(crop_atom_with_count(
                size,
                targs.len() - idx,
                "type argument",
                "type arguments",
            ));
            break;
        }
        elements.push(type_(opts, depth, arg.as_ref(), size));
    }
    layout::list(
        None, // break_mode
        Some((
            LayoutNode::atom("<".to_string()),
            LayoutNode::atom(">".to_string()),
        )), // wrap
        Some(LayoutNode::atom(",".to_string())), // sep
        true, // trailing
        None, // inline
        None, // indent
        elements,
    )
}

fn type_union<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    after_separator: bool,
    types: &[&Ty<L>],
    size: &mut PrintState<L>,
) -> LayoutNode {
    let has_null = types.iter().any(|t| matches!(t, Ty::Null));
    let has_void = types.iter().any(|t| matches!(t, Ty::Void | Ty::Undefined));

    let prefix_is_maybe = has_null && has_void && types.len() > 2;
    let (prefix, filtered_types): (LayoutNode, Vec<&Ty<L>>) = if prefix_is_maybe {
        let filtered: Vec<&Ty<L>> = types
            .iter()
            .copied()
            .filter(|t| !matches!(t, Ty::Null | Ty::Void | Ty::Undefined))
            .collect();
        let final_types = if filtered.is_empty() {
            vec![&Ty::Bot(BotKind::EmptyType) as &Ty<L>]
        } else {
            filtered
        };
        (LayoutNode::atom("?".to_string()), final_types)
    } else {
        (LayoutNode::empty(), types.to_vec())
    };

    // A lone type under the maybe prefix has no members to list. `?() => void`
    // already parses as `?(() => void)`, so a lone function needs no parentheses.
    if let [t] = filtered_types.as_slice() {
        let t = match t {
            Ty::Fun(_) => type_(opts, depth, t, size),
            _ => type_with_parens(opts, depth, t, size),
        };
        return if after_separator {
            layout::fuse(vec![layout::pretty_space(), prefix, t])
        } else {
            layout::fuse(vec![prefix, t])
        };
    }

    let elements = type_list_elements(
        opts,
        depth,
        "|",
        "union member",
        "union members",
        &filtered_types,
        size,
    );
    layout_of_members(after_separator, "|", prefix, elements)
}

fn type_intersection<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    after_separator: bool,
    types: &[&Ty<L>],
    size: &mut PrintState<L>,
) -> LayoutNode {
    let elements = type_list_elements(
        opts,
        depth,
        "&",
        "intersection member",
        "intersection members",
        types,
        size,
    );
    layout_of_members(after_separator, "&", LayoutNode::empty(), elements)
}

/// A union or intersection that does not fit lists its members a line each,
/// every one led by `sep`. A member's own lines are indented past its `sep`.
fn layout_of_members(
    after_separator: bool,
    sep: &str,
    prefix: LayoutNode,
    elements: Vec<LayoutNode>,
) -> LayoutNode {
    let elements = elements.into_iter().map(LayoutNode::indent);
    let leading_sep = |flat: LayoutNode| {
        layout::if_break(
            layout::fuse(vec![LayoutNode::atom(sep.to_string()), layout::space()]),
            flat,
        )
    };
    if after_separator {
        // After a separator the members start on a line of their own, indented
        // under it.
        let elements = elements
            .enumerate()
            .map(|(i, element)| {
                if i == 0 {
                    layout::fuse(vec![leading_sep(layout::space()), prefix.dupe(), element])
                } else {
                    element
                }
            })
            .collect();
        layout::list(None, None, None, false, Some((false, true)), None, elements)
    } else {
        // Elsewhere the members line up under the first one, wherever the
        // enclosing layout starts it.
        let elements = elements
            .enumerate()
            .map(|(i, element)| {
                if i == 0 {
                    layout::fuse(vec![
                        leading_sep(LayoutNode::empty()),
                        prefix.dupe(),
                        element,
                    ])
                } else {
                    layout::fuse(vec![layout::pretty_line(), element])
                }
            })
            .collect();
        layout::group(vec![layout::fuse(elements)])
    }
}

fn type_list_elements<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    sep: &str,
    singular: &str,
    plural: &str,
    types: &[&Ty<L>],
    size: &mut PrintState<L>,
) -> Vec<LayoutNode> {
    let mut elts = Vec::new();
    for (i, t) in types.iter().enumerate() {
        if size.remaining == 0 {
            let crop = crop_atom_with_count(size, types.len() - i, singular, plural);
            if i == 0 {
                elts.push(crop);
            } else {
                elts.push(layout::fuse(vec![
                    LayoutNode::atom(sep.to_string()),
                    layout::space(),
                    crop,
                ]));
            }
            break;
        }
        let typ = type_with_parens(opts, depth, t, size);
        if i == 0 {
            elts.push(typ);
        } else {
            elts.push(layout::fuse(vec![
                LayoutNode::atom(sep.to_string()),
                layout::space(),
                typ,
            ]));
        }
    }

    elts
}

fn type_with_parens<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    t: &Ty<L>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    match t {
        // A union or intersection that does not fit puts its members on lines
        // of their own between the parentheses.
        Ty::Union(..) | Ty::Inter(..) => layout::group(vec![
            LayoutNode::atom("(".to_string()),
            LayoutNode::indent(layout::fuse(vec![
                layout::softline(),
                type_(opts, depth, t, size),
            ])),
            layout::softline(),
            LayoutNode::atom(")".to_string()),
        ]),
        Ty::Fun(_) | Ty::Conditional { .. } => wrap_in_parens(type_(opts, depth, t, size)),
        _ => type_(opts, depth, t, size),
    }
}

/// The object of an indexed access `O[K]`: anything that does not end in a
/// primary type would otherwise swallow the `[K]` suffix.
fn indexed_object_with_parens<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    t: &Ty<L>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    match t {
        Ty::Utility(Utility::Keys(_)) | Ty::Renders(_, _) | Ty::Component { .. } | Ty::Infer(_) => {
            wrap_in_parens(type_(opts, depth, t, size))
        }
        _ => type_with_parens(opts, depth, t, size),
    }
}

fn class_decl<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    s: &Symbol<L>,
    type_parameters: &Option<Arc<[TypeParam<L>]>>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    let name = declaration_identifier(size, s);
    layout::fuse(vec![
        LayoutNode::atom("class".to_string()),
        layout::space(),
        name,
        option(
            |tparams: &Arc<[TypeParam<L>]>| type_parameter(opts, depth, tparams, size),
            type_parameters,
        ),
    ])
}

fn interface_decl<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    s: &Symbol<L>,
    type_parameters: &Option<Arc<[TypeParam<L>]>>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    let name = declaration_identifier(size, s);
    layout::fuse(vec![
        LayoutNode::atom("interface".to_string()),
        layout::space(),
        name,
        option(
            |tparams: &Arc<[TypeParam<L>]>| type_parameter(opts, depth, tparams, size),
            type_parameters,
        ),
    ])
}

fn record_decl<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    s: &Symbol<L>,
    tparams: &Option<Arc<[TypeParam<L>]>>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    let name = declaration_identifier(size, s);
    layout::fuse(vec![
        LayoutNode::atom("record".to_string()),
        layout::space(),
        name,
        option(
            |tp: &Arc<[TypeParam<L>]>| type_parameter(opts, depth, tp, size),
            tparams,
        ),
    ])
}

fn nominal_component_decl<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    s: &Symbol<L>,
    type_parameters: &Option<Arc<[TypeParam<L>]>>,
    targs: &Option<Arc<[Arc<Ty<L>>]>>,
    regular_props: &ComponentProps<L>,
    renders: &Option<Ty<L>>,
    is_type: bool,
    size: &mut PrintState<L>,
) -> LayoutNode {
    if is_type {
        let name = symbol_identifier(size, s);
        // Prefer displaying type arguments if they exist
        let type_args_or_params = match targs {
            Some(ts) => type_args(opts, depth, ts, size),
            None => option(
                |tparams: &Arc<[TypeParam<L>]>| type_parameter(opts, depth, tparams, size),
                type_parameters,
            ),
        };

        layout::fuse(vec![
            LayoutNode::atom("React$RendersExactly".to_string()),
            layout::list(
                None, // break_mode
                Some((
                    LayoutNode::atom("<".to_string()),
                    LayoutNode::atom(">".to_string()),
                )), // wrap
                Some(LayoutNode::atom(",".to_string())), // sep
                false, // trailing
                None, // inline
                None, // indent
                vec![layout::fuse(vec![
                    LayoutNode::atom("typeof".to_string()),
                    layout::space(),
                    name,
                    type_args_or_params,
                ])],
            ),
        ])
    } else {
        let name = declaration_identifier(size, s);
        // component ComponentName<...>(props) renders Type
        // Prefer displaying type arguments if they exist
        let type_args_or_params = match targs {
            Some(ts) => type_args(opts, depth, ts, size),
            None => option(
                |tparams: &Arc<[TypeParam<L>]>| type_parameter(opts, depth, tparams, size),
                type_parameters,
            ),
        };

        layout::fuse(vec![
            LayoutNode::atom("component".to_string()),
            layout::space(),
            name,
            type_args_or_params,
            type_component_sig(
                opts,
                depth,
                regular_props,
                renders.as_ref().map(|t| Arc::new(t.clone())).as_ref(),
                size,
            ),
        ])
    }
}

fn type_alias<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    name: &Symbol<L>,
    tparams: &Option<Arc<[TypeParam<L>]>>,
    t_opt: &Option<Arc<Ty<L>>>,
    opaque: Option<&OpaqueBounds<L>>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    let name = declaration_identifier(size, name);
    let tparams_node = option(
        |tp: &Arc<[TypeParam<L>]>| type_parameter(opts, depth, tp, size),
        tparams,
    );
    let bound = |keyword: &str, t: &Option<Arc<Ty<L>>>, size: &mut PrintState<L>| match t {
        Some(t) => layout::fuse(vec![
            layout::space(),
            LayoutNode::atom(keyword.to_string()),
            type_after_separator(opts, depth, t, size),
        ]),
        None => LayoutNode::empty(),
    };
    let (keyword, bounds) = match opaque {
        None => ("type", LayoutNode::empty()),
        Some(OpaqueBounds { lower, upper }) => (
            "opaque type",
            layout::fuse(vec![
                bound("super", lower, size),
                bound("extends", upper, size),
            ]),
        ),
    };
    let body = match t_opt {
        Some(t) => layout::fuse(vec![
            layout::pretty_space(),
            LayoutNode::atom("=".to_string()),
            type_after_separator(opts, depth, t, size),
        ]),
        None => LayoutNode::empty(),
    };

    layout::fuse(vec![
        LayoutNode::atom(keyword.to_string()),
        layout::space(),
        name,
        tparams_node,
        bounds,
        body,
    ])
}

fn type_after_separator<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    t: &Ty<L>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    let depth = depth + 1;
    if size.remaining == 0 {
        return layout::fuse(vec![layout::space(), crop_atom(size)]);
    }
    size.remaining -= 1;

    match t {
        Ty::Union(_, t1, t2, ts) => {
            let types: Vec<_> = [t1.as_ref(), t2.as_ref()]
                .into_iter()
                .chain(ts.iter().map(AsRef::as_ref))
                .collect();
            type_union(opts, depth, true, &types, size)
        }
        Ty::Inter(t1, t2, ts) => {
            let types: Vec<_> = [t1.as_ref(), t2.as_ref()]
                .into_iter()
                .chain(ts.iter().map(AsRef::as_ref))
                .collect();
            type_intersection(opts, depth, true, &types, size)
        }
        _ => layout::fuse(vec![
            layout::pretty_space(),
            type_impl(opts, depth, t, size),
        ]),
    }
}

fn variable_decl<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    name: &Name,
    t: &Ty<L>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    layout::fuse(vec![
        LayoutNode::atom("declare".to_string()),
        layout::space(),
        LayoutNode::atom("var".to_string()),
        layout::space(),
        identifier(name),
        LayoutNode::atom(":".to_string()),
        type_after_separator(opts, depth, t, size),
    ])
}

fn enum_decl<L: Dupe>(
    s: &Symbol<L>,
    members: &Option<Arc<[FlowSmolStr]>>,
    has_unknown_members: bool,
    truncated_members_count: i64,
    state: &mut PrintState<L>,
) -> LayoutNode {
    let base = layout::fuse(vec![
        LayoutNode::atom("enum".to_string()),
        layout::space(),
        declaration_identifier(state, s),
    ]);
    match members {
        None => base,
        Some(ms) => {
            let mut member_nodes: Vec<_> = ms
                .iter()
                .map(|member| LayoutNode::atom(member.to_string()))
                .collect();
            if truncated_members_count > 0 {
                member_nodes.push(LayoutNode::atom(format!(
                    "/* ... {} more members */",
                    truncated_members_count
                )));
            } else if has_unknown_members {
                member_nodes.push(LayoutNode::atom("...".to_string()));
            }
            layout::fuse(vec![
                base,
                layout::space(),
                layout::list(
                    None,
                    Some((
                        LayoutNode::atom("{".to_string()),
                        LayoutNode::atom("}".to_string()),
                    )),
                    Some(LayoutNode::atom(",".to_string())),
                    false,
                    None,
                    None,
                    member_nodes,
                ),
            ])
        }
    }
}

fn namespace<L: Dupe>(name: &Option<Symbol<L>>, state: &mut PrintState<L>) -> LayoutNode {
    let name_node = match name {
        Some(sym) => {
            record_symbol(state, sym);
            layout::fuse(vec![
                layout::space(),
                LayoutNode::atom(sym.sym_name.to_string()),
            ])
        }
        None => LayoutNode::empty(),
    };

    layout::fuse(vec![LayoutNode::atom("namespace".to_string()), name_node])
}

fn module_<L: Dupe>(
    opts: &PrinterOptions,
    name: &Option<Symbol<L>>,
    state: &mut PrintState<L>,
) -> LayoutNode {
    let name_node = match name {
        Some(sym) => {
            record_symbol(state, sym);
            let name_str = sym.sym_name.to_string();
            layout::fuse(vec![
                layout::space(),
                layout::fuse(in_quotes(opts.prefer_single_quotes, &name_str)),
            ])
        }
        None => LayoutNode::empty(),
    };

    layout::fuse(vec![LayoutNode::atom("module".to_string()), name_node])
}

fn decl<L: Dupe>(
    opts: &PrinterOptions,
    depth: usize,
    d: &Decl<L>,
    size: &mut PrintState<L>,
) -> LayoutNode {
    match d {
        Decl::VariableDecl(box (name, t)) => variable_decl(opts, depth, name, t, size),
        Decl::TypeAliasDecl(box DeclTypeAliasDeclData {
            name,
            tparams,
            type_,
            opaque,
            ..
        }) => type_alias(opts, depth, name, tparams, type_, opaque.as_ref(), size),
        Decl::ClassDecl(box (s, ps)) => class_decl(opts, depth, s, ps, size),
        Decl::InterfaceDecl(box (s, ps)) => interface_decl(opts, depth, s, ps, size),
        Decl::RecordDecl(box (s, ps)) => record_decl(opts, depth, s, ps, size),
        Decl::EnumDecl(box DeclEnumDeclData {
            name,
            members,
            has_unknown_members,
            truncated_members_count,
        }) => enum_decl(
            name,
            members,
            *has_unknown_members,
            *truncated_members_count,
            size,
        ),
        Decl::NominalComponentDecl(box DeclNominalComponentDeclData {
            name,
            tparams,
            targs,
            props,
            renders,
            is_type,
        }) => nominal_component_decl(
            opts, depth, name, tparams, targs, props, renders, *is_type, size,
        ),
        Decl::NamespaceDecl(box DeclNamespaceDeclData { name, .. }) => namespace(name, size),
        Decl::ModuleDecl(box DeclModuleDeclData { name, .. }) => module_(opts, name, size),
    }
}

#[derive(Debug, Clone)]
pub struct PrinterOptions {
    pub prefer_single_quotes: bool,
    pub size: usize,
}

impl Default for PrinterOptions {
    fn default() -> Self {
        PrinterOptions {
            prefer_single_quotes: false,
            size: 5000,
        }
    }
}

pub fn string_of_elt<L: Dupe>(elt: &Elt<L>, opts: &PrinterOptions) -> String {
    let layout = layout_of_elt(opts, elt);
    pretty_printer::print(true, &layout).contents()
}

pub fn string_of_t<L: Dupe>(t: &Ty<L>, opts: &PrinterOptions) -> String {
    string_of_elt(&Elt::Type(Arc::new(t.clone())), opts)
}

pub fn string_of_elt_single_line<L: Dupe>(elt: &Elt<L>, opts: &PrinterOptions) -> String {
    let layout = layout_of_elt(opts, elt);
    layout::print_single_line(&layout)
}

pub fn string_of_t_single_line<L: Dupe>(t: &Ty<L>, opts: &PrinterOptions) -> String {
    string_of_elt_single_line(&Elt::Type(Arc::new(t.clone())), opts)
}

pub fn string_of_decl_single_line<L: Dupe>(d: &Decl<L>, opts: &PrinterOptions) -> String {
    string_of_elt_single_line(&Elt::Decl(d.clone()), opts)
}

/// Cites each symbol under the name the type above it was printed with, which for
/// an imported one is the name it was imported as rather than its name at the
/// definition site: a reader who is told about `C` cannot find it in a hover that
/// only ever says `CLocal`.
pub fn string_of_symbol_set<L: Clone + Ord>(
    syms: &std::collections::BTreeSet<Symbol<L>>,
) -> Vec<(String, L)> {
    string_of_symbol_set_with_head(syms, None)
}

/// Like [string_of_symbol_set], but cites `head`, the symbol printed as the head
/// of a declaration, under the name the head prints.
fn string_of_symbol_set_with_head<L: Clone + Ord>(
    syms: &std::collections::BTreeSet<Symbol<L>>,
    head: Option<&Symbol<L>>,
) -> Vec<(String, L)> {
    let mut elems: Vec<_> = syms
        .iter()
        .map(|sym| {
            let name = if head == Some(sym) {
                head_name_of_symbol(sym)
            } else {
                local_name_of_symbol(sym)
            };
            (name, sym)
        })
        .collect();
    elems.sort_by(|(n1, _), (n2, _)| n1.cmp(n2));
    elems
        .into_iter()
        .map(|(name, sym)| (name.to_string(), sym.sym_def_loc.clone()))
        .collect()
}

/// What to print for a `type-at-pos` hit. `binder` is set only when the target is
/// a value binding's own name, in which case the type is framed as a declaration.
pub struct TypeAtPosPrint<'a, R> {
    pub ty: &'a Elt<ALoc>,
    pub refs: Option<&'a std::collections::BTreeSet<Symbol<R>>>,
    pub binder: Option<&'a Binder>,
    pub alias: Option<&'a Alias>,
}

pub fn string_of_type_at_pos_result<R: Dupe + Ord>(
    result: TypeAtPosPrint<'_, R>,
    loc_of_aloc: &dyn Fn(&ALoc) -> R,
    opts: &PrinterOptions,
) -> (String, Option<Vec<(String, R)>>) {
    let mut state = PrintState {
        remaining: opts.size,
        printed_symbols: Some(Vec::new()),
        truncated: false,
    };
    let body = match (result.binder, result.ty) {
        // A `Decl` already prints a declaration of its own (`class A`, `type X = …`),
        // so framing it again would double the head.
        (Some(binder), Elt::Type(t)) => layout_of_binder(opts, binder, t, &mut state),
        (_, Elt::Type(t)) => type_(opts, 0, t, &mut state),
        (_, Elt::Decl(d)) => decl(opts, 0, d, &mut state),
    };
    let printed_symbols = state.printed_symbols.unwrap_or_default();
    let body = match result.alias {
        None => body,
        Some(_) => layout::fuse(vec![
            LayoutNode::atom("(alias)".to_string()),
            layout::space(),
            body,
        ]),
    };
    let layout = match result.alias {
        None => body,
        Some(alias) => layout::fuse(vec![body, layout_of_alias_statement(opts, alias)]),
    };
    let type_str = pretty_printer::print(true, &layout).contents();
    let head = match result.ty {
        Elt::Decl(Decl::NominalComponentDecl(box DeclNominalComponentDeclData {
            is_type: true,
            ..
        }))
        | Elt::Type(_) => None,
        Elt::Decl(d) => {
            head_symbol_of_decl(d).map(|symbol| symbol.map_locs(&|aloc| loc_of_aloc(aloc)))
        }
    };
    let refs = result.refs.map(|refs| {
        if !state.truncated {
            return string_of_symbol_set_with_head(refs, head.as_ref());
        }

        let printed_refs: std::collections::BTreeSet<_> = printed_symbols
            .iter()
            .map(|symbol| symbol.map_locs(&|aloc| loc_of_aloc(aloc)))
            .collect();
        let visible_refs = refs.intersection(&printed_refs).cloned().collect();
        string_of_symbol_set_with_head(&visible_refs, head.as_ref())
    });
    (type_str, refs)
}

/// The declaration head hover shows before the type. Kinds with no JavaScript
/// keyword of their own are parenthesized.
fn layout_of_binder_head(binder: &Binder) -> LayoutNode {
    layout_of_binder_head_with_effect(binder, &FunEffect::Arbitrary)
}

/// The head of a callable binder spliced into a signature. A hook is declared
/// with `hook` in place of `function`.
fn layout_of_binder_head_with_effect(binder: &Binder, effect: &FunEffect) -> LayoutNode {
    let prefix = match binder.kind {
        BinderKind::Function if matches!(effect, FunEffect::Hook) => "hook",
        BinderKind::Var => "var",
        BinderKind::Let => "let",
        BinderKind::Const => "const",
        BinderKind::Function => "function",
        BinderKind::Method => "(method)",
        BinderKind::Constructor => "constructor",
        BinderKind::Getter => "(getter)",
        BinderKind::Setter => "(setter)",
        BinderKind::Property => "(property)",
        BinderKind::Index => "(index)",
        BinderKind::EnumMember => "(enum member)",
        BinderKind::Parameter => "(parameter)",
        BinderKind::TypeParameter => "(type parameter)",
    };
    let name = match &binder.owner {
        Some(owner) => format!("{}.{}", owner, binder.name),
        None => binder.name.to_string(),
    };
    layout::fuse(vec![
        LayoutNode::atom(prefix.to_string()),
        layout::space(),
        LayoutNode::atom(name),
    ])
}

/// The `(+2 overloads)` tail, for a signature that is one of several the
/// declaration provides. Empty when there is only the one being printed.
fn layout_of_overload_count(overloads: u32) -> LayoutNode {
    if overloads == 0 {
        return LayoutNode::empty();
    }
    let plural = if overloads == 1 { "" } else { "s" };
    layout::fuse(vec![
        layout::space(),
        LayoutNode::atom(format!("(+{overloads} overload{plural})")),
    ])
}

/// The signatures of an overload set, if `t` is one: an intersection whose every
/// member is a function. A mixed intersection is not an overload set and has no
/// reading as a list of signatures.
fn overload_signatures(t: &Ty<ALoc>) -> Option<Vec<&FunT<ALoc>>> {
    let Ty::Inter(t1, t2, rest) = t else {
        return None;
    };
    [t1, t2]
        .into_iter()
        .chain(rest.iter())
        .map(|t| match t.as_ref() {
            Ty::Fun(func) => Some(func.as_ref()),
            _ => None,
        })
        .collect()
}

/// Frames `t` as the declaration of `binder`: `const a: A`. A callable bound by
/// `function` or a method is spliced into a signature (`function f(): void`)
/// rather than annotated with an arrow type.
fn layout_of_binder(
    opts: &PrinterOptions,
    binder: &Binder,
    t: &Ty<ALoc>,
    state: &mut PrintState<ALoc>,
) -> LayoutNode {
    let head = layout_of_binder_head(binder);
    match t {
        // A type parameter stands for whatever it is instantiated with, so there is
        // no type to annotate it with. When its declaration is available, identify
        // the generic construct that introduced it instead.
        _ if binder.kind == BinderKind::TypeParameter => {
            let Some(context) = &binder.type_parameter_context else {
                return head;
            };
            let declaration = match &context.declaration {
                Elt::Type(t) => layout_of_binder(
                    opts,
                    &Binder {
                        kind: BinderKind::Function,
                        name: context.name.dupe(),
                        owner: None,
                        type_parameter_context: None,
                        overloads: 0,
                        targs: None,
                    },
                    t,
                    state,
                ),
                Elt::Decl(Decl::TypeAliasDecl(data)) => type_alias(
                    opts,
                    0,
                    &data.name,
                    &data.tparams,
                    &None,
                    data.opaque.as_ref(),
                    state,
                ),
                Elt::Decl(d) => decl(opts, 0, d, state),
            };
            layout::fuse(vec![
                head,
                layout::space(),
                LayoutNode::atom("in".to_string()),
                layout::space(),
                declaration,
            ])
        }
        Ty::Fun(func)
            if matches!(
                binder.kind,
                BinderKind::Function | BinderKind::Method | BinderKind::Constructor
            ) =>
        {
            layout::fuse(vec![
                layout_of_binder_head_with_effect(binder, &func.fun_effect),
                function_signature(
                    opts,
                    0,
                    &LayoutNode::atom(":".to_string()),
                    func,
                    binder.targs.as_deref(),
                    state,
                ),
                layout_of_overload_count(binder.overloads),
            ])
        }
        t if matches!(
            binder.kind,
            BinderKind::Function | BinderKind::Method | BinderKind::Constructor
        ) && let Some(signatures) = overload_signatures(t) =>
        {
            let mut parts = Vec::new();
            for (i, func) in signatures.iter().enumerate() {
                if i > 0 {
                    parts.push(layout::hardline());
                }
                parts.push(layout_of_binder_head_with_effect(binder, &func.fun_effect));
                parts.push(function_signature(
                    opts,
                    0,
                    &LayoutNode::atom(":".to_string()),
                    func,
                    None,
                    state,
                ));
            }
            layout::fuse(parts)
        }
        // A callee bound by `const` or held in a property is an arrow type, with
        // the call's type arguments in place of its type parameters:
        // `const f: <number>(x: number) => number`. The arrow is at the depth
        // `type_after_separator` gives an annotation.
        Ty::Fun(func) if let Some(targs) = &binder.targs => layout::fuse(vec![
            head,
            LayoutNode::atom(":".to_string()),
            layout::pretty_space(),
            type_function(opts, 1, &arrow_separator(), func, Some(targs), state),
        ]),
        _ => layout::fuse(vec![
            head,
            LayoutNode::atom(":".to_string()),
            type_after_separator(opts, 0, t, state),
        ]),
    }
}
/// The statement an alias came from, printed under the declaration it aliases.
fn layout_of_alias_statement(opts: &PrinterOptions, alias: &Alias) -> LayoutNode {
    if let Some(import) = &alias.import {
        let mode = match import.mode {
            ImportMode::ValueMode => LayoutNode::empty(),
            ImportMode::TypeMode => {
                layout::fuse(vec![LayoutNode::atom("type".to_string()), layout::space()])
            }
            ImportMode::TypeofMode => layout::fuse(vec![
                LayoutNode::atom("typeof".to_string()),
                layout::space(),
            ]),
        };
        let specifier = match &import.specifier {
            ImportSpecifier::Default => LayoutNode::atom(alias.name.to_string()),
            ImportSpecifier::Named { remote_name } => {
                let renamed = (remote_name != &alias.name).then(|| {
                    layout::fuse(vec![
                        layout::space(),
                        LayoutNode::atom("as".to_string()),
                        layout::space(),
                        LayoutNode::atom(alias.name.to_string()),
                    ])
                });
                layout::fuse(vec![
                    LayoutNode::atom("{".to_string()),
                    LayoutNode::atom(remote_name.to_string()),
                    renamed.unwrap_or_else(LayoutNode::empty),
                    LayoutNode::atom("}".to_string()),
                ])
            }
            ImportSpecifier::Namespace => layout::fuse(vec![
                LayoutNode::atom("*".to_string()),
                layout::space(),
                LayoutNode::atom("as".to_string()),
                layout::space(),
                LayoutNode::atom(alias.name.to_string()),
            ]),
        };
        return layout::fuse(vec![
            layout::hardline(),
            LayoutNode::atom("import".to_string()),
            layout::space(),
            mode,
            specifier,
            layout::space(),
            LayoutNode::atom("from".to_string()),
            layout::space(),
            layout::fuse(in_quotes(opts.prefer_single_quotes, &import.source)),
        ]);
    }

    let keyword = match alias.kind {
        AliasKind::Import => "import",
        AliasKind::Export => "export",
    };
    layout::fuse(vec![
        layout::hardline(),
        LayoutNode::atom(keyword.to_string()),
        layout::space(),
        LayoutNode::atom(alias.name.to_string()),
    ])
}
