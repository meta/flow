/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::rc::Rc;

use dupe::Dupe;
use flow_common::js_number::numeric_property_name;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast::expression::Expression;
use flow_parser::ast::function;
use flow_parser::ast::pattern;
use flow_parser::ast::pattern::Pattern;

use crate::selector::Selector;

#[derive(Debug, Clone)]
pub enum Binding<L: Dupe, T: Dupe> {
    Root,
    Rest,
    Select {
        selector: Selector<L, T>,
        parent: (L, Rc<Binding<L, T>>),
    },
}

pub fn array_element<L: Dupe, T: Dupe>(
    parent_loc: L,
    bind: Rc<Binding<L, T>>,
    index: usize,
    direct_default: bool,
) -> Rc<Binding<L, T>> {
    let selector = Selector::Elem {
        index,
        has_default: direct_default,
    };
    Rc::new(Binding::Select {
        selector,
        parent: (parent_loc, bind),
    })
}

pub fn array_rest_element<L: Dupe, T: Dupe>(
    parent_loc: L,
    bind: Rc<Binding<L, T>>,
    index: usize,
) -> Rc<Binding<L, T>> {
    let selector = Selector::ArrRest(index);
    Rc::new(Binding::Select {
        selector,
        parent: (parent_loc, bind),
    })
}

pub fn object_named_property<L: Dupe, T: Dupe>(
    parent_loc: L,
    bind: Rc<Binding<L, T>>,
    prop_loc: L,
    name: FlowSmolStr,
    has_default: bool,
) -> Rc<Binding<L, T>> {
    let selector = Selector::Prop {
        prop: name,
        prop_loc,
        has_default,
    };
    Rc::new(Binding::Select {
        selector,
        parent: (parent_loc, bind),
    })
}

pub fn object_computed_property<L: Dupe, T: Dupe>(
    parent_loc: L,
    bind: Rc<Binding<L, T>>,
    expression: Expression<L, T>,
    has_default: bool,
) -> Rc<Binding<L, T>> {
    let selector = Selector::Computed {
        expression,
        has_default,
    };
    Rc::new(Binding::Select {
        selector,
        parent: (parent_loc, bind),
    })
}

pub fn object_rest_property<L: Dupe, T: Dupe>(
    parent_loc: L,
    bind: Rc<Binding<L, T>>,
    used_props: &[FlowSmolStr],
    after_computed: bool,
) -> Rc<Binding<L, T>> {
    let selector = Selector::ObjRest {
        used_props: used_props.iter().map(|s| s.dupe()).collect(),
        after_computed,
    };
    Rc::new(Binding::Select {
        selector,
        parent: (parent_loc, bind),
    })
}

pub fn object_property<L: Dupe>(
    parent_loc: L,
    bind: Rc<Binding<L, L>>,
    mut used_props: Vec<FlowSmolStr>,
    key: &pattern::object::Key<L, L>,
    has_default: bool,
) -> (Rc<Binding<L, L>>, Vec<FlowSmolStr>, bool) {
    match key {
        pattern::object::Key::Identifier(id) => {
            let name = id.name.dupe();
            let bind =
                object_named_property(parent_loc, bind, id.loc.dupe(), name.dupe(), has_default);
            used_props.push(name);
            (bind, used_props, false)
        }
        pattern::object::Key::StringLiteral((loc, lit)) => {
            let name = lit.value.dupe();
            let bind =
                object_named_property(parent_loc, bind, loc.dupe(), name.dupe(), has_default);
            used_props.push(name);
            (bind, used_props, false)
        }
        pattern::object::Key::Computed(computed) => {
            let bind = object_computed_property(
                parent_loc,
                bind,
                computed.expression.clone(),
                has_default,
            );
            (bind, used_props, true)
        }
        pattern::object::Key::NumberLiteral((loc, lit)) => {
            if let Some(name) = numeric_property_name(lit.value) {
                let bind =
                    object_named_property(parent_loc, bind, loc.dupe(), name.dupe(), has_default);
                used_props.push(name);
                (bind, used_props, false)
            } else {
                (Rc::new(Binding::Root), used_props, false)
            }
        }
        pattern::object::Key::BigIntLiteral(_) => (Rc::new(Binding::Root), used_props, false),
    }
}

fn identifier<L: Dupe + Ord>(
    mut acc: BTreeMap<FlowSmolStr, (L, Rc<Binding<L, L>>)>,
    bind: Rc<Binding<L, L>>,
    name_loc: L,
    name: FlowSmolStr,
) -> BTreeMap<FlowSmolStr, (L, Rc<Binding<L, L>>)> {
    acc.insert(name, (name_loc, bind));
    acc
}

pub fn fold_pattern<L: Ord + Dupe>(
    acc: BTreeMap<FlowSmolStr, (L, Rc<Binding<L, L>>)>,
    bind: Rc<Binding<L, L>>,
    pattern: &Pattern<L, L>,
) -> BTreeMap<FlowSmolStr, (L, Rc<Binding<L, L>>)> {
    match pattern {
        Pattern::Array { loc, inner, .. } => array_elements(acc, loc.dupe(), bind, &inner.elements),
        Pattern::Object { loc, inner, .. } => {
            object_properties(acc, loc.dupe(), bind, &inner.properties)
        }
        Pattern::Identifier { inner, .. } => {
            let name_loc = inner.name.loc.dupe();
            let name = inner.name.name.dupe();
            identifier(acc, bind, name_loc, name)
        }
        Pattern::Expression { .. } => acc,
    }
}

fn array_elements<L: Ord + Dupe>(
    mut acc: BTreeMap<FlowSmolStr, (L, Rc<Binding<L, L>>)>,
    parent_loc: L,
    bind: Rc<Binding<L, L>>,
    elements: &[pattern::array::Element<L, L>],
) -> BTreeMap<FlowSmolStr, (L, Rc<Binding<L, L>>)> {
    for (i, elt) in elements.iter().enumerate() {
        acc = match elt {
            pattern::array::Element::Hole(_) => acc,
            pattern::array::Element::NormalElement(elem) => {
                let has_default = elem.default.is_some();
                let child_bind = array_element(parent_loc.dupe(), bind.dupe(), i, has_default);
                fold_pattern(acc, child_bind, &elem.argument)
            }
            pattern::array::Element::RestElement(rest) => {
                let child_bind = array_rest_element(parent_loc.dupe(), bind.dupe(), i);
                fold_pattern(acc, child_bind, &rest.argument)
            }
        };
    }
    acc
}

fn object_properties<L: Ord + Dupe>(
    mut acc: BTreeMap<FlowSmolStr, (L, Rc<Binding<L, L>>)>,
    parent_loc: L,
    bind: Rc<Binding<L, L>>,
    properties: &[pattern::object::Property<L, L>],
) -> BTreeMap<FlowSmolStr, (L, Rc<Binding<L, L>>)> {
    let mut used_props: Vec<FlowSmolStr> = Vec::new();
    let mut has_computed = false;

    for prop in properties {
        match prop {
            pattern::object::Property::NormalProperty(normal) => {
                let prop_has_default = normal.default.is_some();
                let (prop_bind, new_used_props, new_has_computed) = object_property(
                    parent_loc.dupe(),
                    bind.dupe(),
                    used_props,
                    &normal.key,
                    prop_has_default,
                );
                used_props = new_used_props;
                has_computed = has_computed || new_has_computed;
                acc = fold_pattern(acc, prop_bind, &normal.pattern);
            }
            pattern::object::Property::RestElement(rest) => {
                let rest_bind =
                    object_rest_property(parent_loc.dupe(), bind.dupe(), &used_props, has_computed);
                acc = fold_pattern(acc, rest_bind, &rest.argument);
                has_computed = false;
            }
        }
    }
    acc
}

pub fn bindings_of_params<L: Ord + Dupe>(
    params: &function::Params<L, L>,
) -> BTreeMap<FlowSmolStr, (L, Rc<Binding<L, L>>)> {
    let mut acc = BTreeMap::new();
    for param in params.params.iter() {
        match param {
            function::Param::RegularParam { argument, .. } => {
                acc = fold_pattern(acc, Rc::new(Binding::Root), argument);
            }
            function::Param::ParamProperty { .. } => {
                // Skip param properties - they are not supported
            }
        }
    }

    if let Some(rest) = &params.rest {
        acc = fold_pattern(acc, Rc::new(Binding::Rest), &rest.argument);
    }

    acc
}

pub fn bindings_of_pattern<L: Ord + Dupe>(
    pattern: &Pattern<L, L>,
) -> BTreeMap<FlowSmolStr, (L, Rc<Binding<L, L>>)> {
    fold_pattern(BTreeMap::new(), Rc::new(Binding::Root), pattern)
}
