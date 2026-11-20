/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::ops::Deref;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::reason::Reason;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::expression;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::loc::LOC_NONE;
use flow_parser::loc::Loc;
use flow_typing::typed_ast_finder;
use flow_typing_context::Context;
use flow_typing_flow_js::flow_js::FlowJs;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::property;
use flow_typing_type::type_util::reason_of_t;

fn id_snippet(snippets_enabled: bool, idx: usize, name: &str) -> expression::Expression<Loc, Loc> {
    let name = if snippets_enabled && idx == 0 {
        format!("${{0:{}}}", name)
    } else {
        name.to_string()
    };
    expression::Expression(Arc::new(expression::ExpressionInner::Identifier {
        loc: LOC_NONE,
        inner: ast::Identifier(Arc::new(ast::IdentifierInner {
            loc: LOC_NONE,
            name: name.into(),
            comments: None,
        })),
    }))
}

fn mk_attribute(
    snippets_enabled: bool,
    idx: usize,
    name: &str,
) -> ast::jsx::OpeningAttribute<Loc, Loc> {
    ast::jsx::OpeningAttribute::Attribute(ast::jsx::Attribute {
        loc: LOC_NONE,
        name: ast::jsx::attribute::Name::Identifier(ast::jsx::Identifier {
            loc: LOC_NONE,
            name: name.into(),
            comments: None,
        }),
        value: Some(ast::jsx::attribute::Value::ExpressionContainer((
            LOC_NONE,
            ast::jsx::ExpressionContainer {
                expression: ast::jsx::expression_container::Expression::Expression(id_snippet(
                    snippets_enabled,
                    idx,
                    name,
                )),
                comments: None,
            },
        ))),
    })
}

fn get_obj_prop_names(
    include_optional: bool,
    cx: &mut Context,
    reason: &Reason,
    t: &Type,
) -> Option<BTreeSet<FlowSmolStr>> {
    let ts = match FlowJs::possible_concrete_types_for_operators_checking(cx, reason, t) {
        Ok(ts) => ts,
        Err(_) => return None,
    };
    match ts.first().map(|t| t.deref()) {
        Some(TypeInner::DefT(_, def_t)) if let DefTInner::ObjT(obj) = def_t.deref() => {
            let props_tmap = obj.props_tmap.dupe();
            Some(cx.fold_props(
                props_tmap,
                |name, prop, mut acc| match property::read_t(prop) {
                    Some(_) if property::is_optional(prop) => {
                        if include_optional && let Some(name) = name.as_smol_str_opt() {
                            acc.insert(name.dupe());
                        }
                        acc
                    }
                    _ => {
                        if let Some(name) = name.as_smol_str_opt() {
                            acc.insert(name.dupe());
                        }
                        acc
                    }
                },
                BTreeSet::new(),
            ))
        }
        _ => None,
    }
}

fn name_of_jsx_id(id: &ast::jsx::Identifier<Loc, Loc>) -> &FlowSmolStr {
    &id.name
}

fn name_of_attribute(attribute: &ast::jsx::OpeningAttribute<Loc, Loc>) -> String {
    match attribute {
        ast::jsx::OpeningAttribute::Attribute(ast::jsx::Attribute {
            name: ast::jsx::attribute::Name::Identifier(id),
            ..
        }) => name_of_jsx_id(id).to_string(),
        ast::jsx::OpeningAttribute::Attribute(ast::jsx::Attribute {
            name: ast::jsx::attribute::Name::NamespacedName(ns_name),
            ..
        }) => {
            format!(
                "{}.{}",
                name_of_jsx_id(&ns_name.namespace),
                name_of_jsx_id(&ns_name.name)
            )
        }
        ast::jsx::OpeningAttribute::SpreadAttribute(_) => "_".to_string(),
    }
}

fn get_existing_attributes_names(
    cx: &mut Context,
    tast: &ast::Program<ALoc, (ALoc, Type)>,
    attributes: &[ast::jsx::OpeningAttribute<Loc, Loc>],
    children: &(Loc, Vec<ast::jsx::Child<Loc, Loc>>),
) -> BTreeSet<FlowSmolStr> {
    let mut acc = if children.1.is_empty() {
        BTreeSet::new()
    } else {
        let mut s = BTreeSet::new();
        s.insert(FlowSmolStr::new("children"));
        s
    };
    for attr in attributes {
        match attr {
            ast::jsx::OpeningAttribute::Attribute(ast::jsx::Attribute {
                name: ast::jsx::attribute::Name::Identifier(id),
                ..
            }) => {
                acc.insert(name_of_jsx_id(id).dupe());
            }
            //  non-react jsx
            ast::jsx::OpeningAttribute::Attribute(ast::jsx::Attribute {
                name: ast::jsx::attribute::Name::NamespacedName(_),
                ..
            }) => {}
            ast::jsx::OpeningAttribute::SpreadAttribute(sa) => {
                let expr_loc = sa.argument.loc();
                match typed_ast_finder::find_exact_match_annotation(
                    tast,
                    ALoc::of_loc(expr_loc.dupe()),
                ) {
                    Some(t) => match get_obj_prop_names(true, cx, reason_of_t(&t), &t) {
                        Some(names) => {
                            acc.extend(names);
                        }
                        None => {}
                    },
                    None => {}
                }
            }
        }
    }
    acc
}

fn attr_compare<K>(
    x: &(K, ast::jsx::OpeningAttribute<Loc, Loc>),
    y: &(K, ast::jsx::OpeningAttribute<Loc, Loc>),
) -> std::cmp::Ordering {
    match (&x.1, &y.1) {
        (
            ast::jsx::OpeningAttribute::SpreadAttribute(_),
            ast::jsx::OpeningAttribute::Attribute(_),
        ) => std::cmp::Ordering::Greater,
        (
            ast::jsx::OpeningAttribute::Attribute(_),
            ast::jsx::OpeningAttribute::SpreadAttribute(_),
        ) => std::cmp::Ordering::Less,
        _ => name_of_attribute(&x.1).cmp(&name_of_attribute(&y.1)),
    }
}

fn loc_of_attr(attr: &ast::jsx::OpeningAttribute<Loc, Loc>) -> Loc {
    match attr {
        ast::jsx::OpeningAttribute::SpreadAttribute(sa) => sa.loc.dupe(),
        ast::jsx::OpeningAttribute::Attribute(a) => a.loc.dupe(),
    }
}

fn concat_and_sort_attrs(
    init_loc: Loc,
    existing_attrs: &[ast::jsx::OpeningAttribute<Loc, Loc>],
    new_attrs: &[ast::jsx::OpeningAttribute<Loc, Loc>],
) -> Vec<(Loc, ast::jsx::OpeningAttribute<Loc, Loc>)> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Existing,
        New,
    }
    let existing_attrs: Vec<_> = existing_attrs
        .iter()
        .map(|a| (Kind::Existing, a.clone()))
        .collect();
    let new_attrs: Vec<_> = new_attrs.iter().map(|a| (Kind::New, a.clone())).collect();
    let is_sorted = existing_attrs
        .windows(2)
        .all(|w| attr_compare(&w[0], &w[1]) != std::cmp::Ordering::Greater);
    let mut attrs: Vec<_> = existing_attrs.into_iter().chain(new_attrs).collect();
    if is_sorted {
        attrs.sort_by(attr_compare);
    }
    let mut prev_loc = init_loc;
    let mut rev_attrs = Vec::new();
    for (k, attr) in attrs {
        match k {
            Kind::Existing => {
                prev_loc = loc_of_attr(&attr);
            }
            Kind::New => {
                rev_attrs.push((prev_loc.end_loc(), attr));
            }
        }
    }
    rev_attrs
}

enum Found {
    Found(Vec<(Loc, ast::jsx::OpeningAttribute<Loc, Loc>)>),
}

struct Mapper<'a, 'cx> {
    cx: &'a mut Context<'cx>,
    snippets_enabled: bool,
    tast: &'a ast::Program<ALoc, (ALoc, Type)>,
    target_loc: Loc,
}

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, Found> for Mapper<'_, '_> {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn jsx_element(
        &mut self,
        loc: &'ast Loc,
        elt: &'ast ast::jsx::Element<Loc, Loc>,
    ) -> Result<(), Found> {
        ast_visitor::jsx_element_default(self, loc, elt)?;
        let opening = &elt.opening_element;
        let name = &opening.name;
        let attributes = &opening.attributes;
        match name {
            ast::jsx::Name::Identifier(id) if id.loc.contains(&self.target_loc) => {
                let id_loc = &id.loc;
                let required_attributes = match typed_ast_finder::find_jsx_props(
                    self.tast,
                    ALoc::of_loc(opening.loc.dupe()),
                ) {
                    Some(props) => get_obj_prop_names(false, self.cx, reason_of_t(&props), &props),
                    None => None,
                };
                match required_attributes {
                    None => Ok(()),
                    Some(required_attributes) => {
                        let existing_attributes_names = get_existing_attributes_names(
                            self.cx,
                            self.tast,
                            attributes,
                            &elt.children,
                        );
                        let diff: BTreeSet<_> = required_attributes
                            .difference(&existing_attributes_names)
                            .cloned()
                            .collect();
                        let mut elements: Vec<_> = diff.into_iter().collect();
                        elements.sort();
                        let new_attrs: Vec<_> = elements
                            .iter()
                            .enumerate()
                            .map(|(idx, name)| mk_attribute(self.snippets_enabled, idx, name))
                            .collect();
                        if !new_attrs.is_empty() {
                            return Err(Found::Found(concat_and_sort_attrs(
                                opening
                                    .targs
                                    .as_ref()
                                    .map_or_else(|| id_loc.end_loc(), |targs| targs.loc.end_loc()),
                                attributes,
                                &new_attrs,
                            )));
                        }
                        Ok(())
                    }
                }
            }
            _ => Ok(()),
        }
    }
}

pub fn fill_props(
    cx: &mut Context,
    snippets_enabled: bool,
    ast: &ast::Program<Loc, Loc>,
    tast: &ast::Program<ALoc, (ALoc, Type)>,
    target_loc: Loc,
) -> Option<Vec<(Loc, ast::jsx::OpeningAttribute<Loc, Loc>)>> {
    let mut mapper = Mapper {
        cx,
        snippets_enabled,
        tast,
        target_loc,
    };
    match mapper.program(ast) {
        Err(Found::Found(xs)) => Some(xs),
        Ok(()) => None,
    }
}
