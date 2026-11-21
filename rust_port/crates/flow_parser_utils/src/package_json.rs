/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::HashMap;
use std::ops::Deref;

use dupe::Dupe;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::expression::ExpressionInner;
use flow_parser::ast::expression::Object;
use flow_parser::loc::Loc;

use crate::package_exports::PackageExports;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackageJson {
    name: Option<FlowSmolStr>,
    main: Option<FlowSmolStr>,
    types: Option<FlowSmolStr>,
    haste_commonjs: bool,
    exports: Option<PackageExports>,
}

impl PackageJson {
    pub fn empty() -> PackageJson {
        PackageJson {
            name: None,
            main: None,
            types: None,
            haste_commonjs: false,
            exports: None,
        }
    }

    pub fn create(
        name: Option<FlowSmolStr>,
        main: Option<FlowSmolStr>,
        types: Option<FlowSmolStr>,
        haste_commonjs: bool,
        exports: Option<PackageExports>,
    ) -> PackageJson {
        PackageJson {
            name,
            main,
            types,
            haste_commonjs,
            exports,
        }
    }

    pub fn name(&self) -> Option<FlowSmolStr> {
        self.name.clone()
    }

    pub fn main(&self) -> Option<FlowSmolStr> {
        self.main.clone()
    }

    pub fn types(&self) -> Option<FlowSmolStr> {
        self.types.clone()
    }

    pub fn haste_commonjs(&self) -> bool {
        self.haste_commonjs
    }

    pub fn exports(&self) -> Option<&PackageExports> {
        self.exports.as_ref()
    }

    pub fn parse(node_main_fields: &[FlowSmolStr], object: &Object<Loc, Loc>) -> Self {
        let mut prop_map = HashMap::new();
        for property in object.properties.iter() {
            extract_property(&mut prop_map, property);
        }
        let name = string_opt(prop_map.get("name"));
        let main = find_main_property(&prop_map, node_main_fields);
        let types = match string_opt(prop_map.get("types")) {
            Some(t) => Some(t),
            None => string_opt(prop_map.get("typings")),
        };
        let haste_commonjs = bool_opt(prop_map.get("haste_commonjs")).unwrap_or(false);
        let exports = package_exports_opt(prop_map.get("exports"));
        Self {
            name,
            main,
            types,
            haste_commonjs,
            exports,
        }
    }
}

fn string_opt(expr: Option<&ast::expression::Expression<Loc, Loc>>) -> Option<FlowSmolStr> {
    match expr {
        Some(expr) => match expr.deref() {
            ExpressionInner::StringLiteral { inner, .. } => Some(inner.value.dupe()),
            _ => None,
        },
        None => None,
    }
}

fn bool_opt(expr: Option<&ast::expression::Expression<Loc, Loc>>) -> Option<bool> {
    match expr {
        Some(expr) => match expr.deref() {
            ExpressionInner::BooleanLiteral { inner, .. } => Some(inner.value),
            _ => None,
        },
        None => None,
    }
}

fn package_exports_opt(
    expr: Option<&ast::expression::Expression<Loc, Loc>>,
) -> Option<PackageExports> {
    expr.and_then(PackageExports::parse)
}

/// Loosely extract a JSON property into `map`, a map from property names to expressions.
/// We aren't looking to validate the file, and don't currently care about any non-literal
/// properties, so we skip over everything else.
fn extract_property(
    map: &mut HashMap<FlowSmolStr, ast::expression::Expression<Loc, Loc>>,
    property: &ast::expression::object::Property<Loc, Loc>,
) {
    if let ast::expression::object::Property::NormalProperty(
        ast::expression::object::NormalProperty::Init {
            key: ast::expression::object::Key::StringLiteral((_loc, lit)),
            value,
            ..
        },
    ) = property
    {
        map.insert(lit.value.dupe(), value.clone());
    }
}

// prop_map is [ "main" ] by default but could be something like [ "foo", "bar" ]. In that case
// we treat the "foo" property like the main property if it exists. If not, we fall back to the
// "bar" property
//
// Spec'd on https://github.com/meta/flow/issues/5725
fn find_main_property(
    prop_map: &HashMap<FlowSmolStr, ast::expression::Expression<Loc, Loc>>,
    node_main_fields: &[FlowSmolStr],
) -> Option<FlowSmolStr> {
    for prop in node_main_fields {
        let ret = prop_map.get(prop);
        if ret.is_none() {
            continue;
        }
        return string_opt(ret);
    }
    None
}
