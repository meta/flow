/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;

use dupe::Dupe;
use flow_parser::ast;
use flow_parser::ast::types::TypeInner;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;

/// The bound of a type parameter without a bound of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImpliedBound {
    Mixed,
    /// `ReadonlyArray<mixed>`. As in TypeScript, an `infer` type that is the type of a rest element
    /// or a rest parameter is an array, unless an `infer` type of the same name has a bound.
    ReadonlyArray,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Position {
    /// The type of a rest element or a rest parameter.
    Rest,
    Other,
}

struct InferTypeHoister<Loc: Dupe> {
    infer_types: Vec<(Loc, ast::types::Infer<Loc, Loc>, Position)>,
}

impl<Loc: Dupe> InferTypeHoister<Loc> {
    pub fn new() -> Self {
        Self {
            infer_types: Vec::new(),
        }
    }

    pub fn into_infer_types(self) -> Vec<(Loc, ast::types::Infer<Loc, Loc>, ImpliedBound)> {
        let implied_bounds: Vec<ImpliedBound> = self
            .infer_types
            .iter()
            .map(|(_, infer, _)| {
                let mut same_name = self
                    .infer_types
                    .iter()
                    .filter(|(_, other, _)| other.tparam.name.name == infer.tparam.name.name);
                if same_name.clone().any(|(_, other, _)| {
                    matches!(
                        other.tparam.bound,
                        ast::types::AnnotationOrHint::Available(_)
                    )
                }) {
                    ImpliedBound::Mixed
                } else if same_name.any(|(_, _, position)| *position == Position::Rest) {
                    ImpliedBound::ReadonlyArray
                } else {
                    ImpliedBound::Mixed
                }
            })
            .collect();
        self.infer_types
            .into_iter()
            .zip(implied_bounds)
            .map(|((loc, infer, _), implied_bound)| (loc, infer, implied_bound))
            .collect()
    }

    fn rest_type(&mut self, t: &ast::types::Type<Loc, Loc>) -> Result<(), !> {
        match t.deref() {
            TypeInner::Infer { loc, inner } => {
                self.infer_types
                    .push((loc.dupe(), inner.as_ref().clone(), Position::Rest));
                ast_visitor::type_default(self, t)
            }
            _ => self.type_(t),
        }
    }
}

impl<Loc: Dupe> AstVisitor<'_, Loc> for InferTypeHoister<Loc> {
    fn normalize_loc(loc: &Loc) -> &Loc {
        loc
    }

    fn normalize_type(type_: &Loc) -> &Loc {
        type_
    }

    fn type_(&mut self, t: &ast::types::Type<Loc, Loc>) -> Result<(), !> {
        match t.deref() {
            TypeInner::Infer { loc, inner } => {
                self.infer_types
                    .push((loc.dupe(), inner.as_ref().clone(), Position::Other));
                ast_visitor::type_default(self, t)
            }
            TypeInner::Conditional { loc: _, inner } => {
                // Visiting of extends_type is intentionally skipped,
                // since it should be internal to the nested conditional type.
                let Ok(()) = self.type_(&inner.check_type);
                let Ok(()) = self.type_(&inner.true_type);
                self.type_(&inner.false_type)
            }
            _ => ast_visitor::type_default(self, t),
        }
    }

    fn tuple_spread_element(
        &mut self,
        spread: &ast::types::tuple::SpreadElement<Loc, Loc>,
    ) -> Result<(), !> {
        let ast::types::tuple::SpreadElement { annot, name: _ } = spread;
        self.rest_type(annot)
    }

    fn function_rest_param_type(
        &mut self,
        frpt: &ast::types::function::RestParam<Loc, Loc>,
    ) -> Result<(), !> {
        match &frpt.argument.param {
            ast::types::function::ParamKind::Anonymous(annot)
            | ast::types::function::ParamKind::Labeled { annot, .. } => self.rest_type(annot),
            ast::types::function::ParamKind::Destructuring(_) => {
                ast_visitor::function_rest_param_type_default(self, frpt)
            }
        }
    }
}

/// The `infer` types of a conditional type's `extends` type, each with the bound it has if it has
/// none of its own.
pub fn hoist_infer_types<Loc: Dupe>(
    t: &ast::types::Type<Loc, Loc>,
) -> Vec<(Loc, ast::types::Infer<Loc, Loc>, ImpliedBound)> {
    let mut hoister = InferTypeHoister::new();
    let Ok(()) = hoister.type_(t);
    hoister.into_infer_types()
}
