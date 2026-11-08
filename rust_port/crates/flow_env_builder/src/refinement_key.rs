/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::fmt;
use std::ops::Deref;
use std::rc::Rc;

use dupe::Dupe;
use flow_common::js_number::numeric_property_name;
use flow_common::reason::Name;
use flow_common::reason::VirtualReasonDesc;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_data_structure_wrapper::vector::FlowVector;
use flow_parser::ast::Identifier;
use flow_parser::ast::expression::Expression;
use flow_parser::ast::expression::ExpressionInner;
use flow_parser::ast::expression::ExpressionOrSpread;
use flow_parser::ast::expression::member;
use flow_parser::loc_sig::LocSig;

#[derive(Debug, Clone, Dupe, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Proj {
    Prop(FlowSmolStr),
    Elem(Rc<Lookup>),
    PrivateField(FlowSmolStr),
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Lookup {
    pub base: FlowSmolStr,
    pub projections: FlowVector<Proj>,
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq)]
pub struct RefinementKey<L> {
    pub loc: L,
    pub lookup: Lookup,
}

impl Lookup {
    fn debug_string(&self) -> String {
        let mut result = self.base.to_string();
        for proj in self.projections.iter() {
            match proj {
                Proj::Prop(name) => result.push_str(&format!(".{}", name)),
                Proj::PrivateField(name) => result.push_str(&format!("private.{}", name)),
                Proj::Elem(lookup) => result.push_str(&format!("[{}]", lookup.debug_string())),
            }
        }
        result
    }

    fn uses_propname(&self, propname: &str, private_: bool) -> bool {
        Self::proj_uses_propname(&self.projections, propname, private_)
    }

    pub fn proj_uses_propname(
        projections: &FlowVector<Proj>,
        propname: &str,
        private_: bool,
    ) -> bool {
        projections.iter().any(|proj| match proj {
            Proj::Prop(name) => name.as_str() == propname && !private_,
            Proj::PrivateField(name) => name.as_str() == propname && private_,
            Proj::Elem(lookup) => lookup.uses_propname(propname, private_),
        })
    }

    /* These functions are adapted from refinement.rs. Eventually, this will be the only place
     * where refinement logic lives, so jmbrown is ok with this temporary duplication while he is
     * fleshing out the refinement features of EnvBuilder
     *
     * The purpose of these functions is to extract _what_ is being refined when we have something like
     * expr != null. What in expr does this refine? */
    pub fn of_expression<M: Dupe, T: Dupe>(
        expr: &Expression<M, T>,
        allow_optional: bool,
    ) -> Option<Self> {
        match expr.deref() {
            ExpressionInner::Identifier { inner, .. } => Some(Self::of_identifier(inner)),
            ExpressionInner::This { .. } => Some(Self::of_this()),
            ExpressionInner::Super { .. } => Some(Self::of_super()),
            ExpressionInner::OptionalMember { inner, .. } if allow_optional => {
                Self::of_member(&inner.member, allow_optional)
            }
            ExpressionInner::Member { inner, .. } => Self::of_member(inner, allow_optional),
            /* other LHSes unsupported currently/here */
            _ => None,
        }
    }

    pub fn of_member<M: Dupe, T: Dupe>(
        member: &flow_parser::ast::expression::Member<M, T>,
        allow_optional: bool,
    ) -> Option<Self> {
        match &member.property {
            member::Property::PropertyIdentifier(id) => {
                let name = id.name.dupe();
                Self::of_expression(&member.object, allow_optional).map(|mut lookup| {
                    lookup.projections.push_back(Proj::Prop(name));
                    lookup
                })
            }
            member::Property::PropertyExpression(prop_expr) => {
                // Check for string literal property
                if let ExpressionInner::StringLiteral { inner, .. } = prop_expr.deref() {
                    let name = inner.value.dupe();
                    return Self::of_expression(&member.object, allow_optional).map(
                        |mut lookup| {
                            lookup.projections.push_back(Proj::Prop(name));
                            lookup
                        },
                    );
                }
                if let ExpressionInner::NumberLiteral { inner, .. } = prop_expr.deref() {
                    if let Some(name) = numeric_property_name(inner.value) {
                        return Self::of_expression(&member.object, allow_optional).map(
                            |mut lookup| {
                                lookup.projections.push_back(Proj::Prop(name));
                                lookup
                            },
                        );
                    }
                }
                /* foo.bar[baz] -> Chain [Id foo; Id bar; Index baz] */
                let obj_lookup = Self::of_expression(&member.object, allow_optional)?;
                let idx_lookup = Self::of_expression(prop_expr, allow_optional)?;
                let mut lookup = obj_lookup;
                lookup
                    .projections
                    .push_back(Proj::Elem(Rc::new(idx_lookup)));
                Some(lookup)
            }
            member::Property::PropertyPrivateName(pn) => {
                let name = pn.name.dupe();
                Self::of_expression(&member.object, allow_optional).map(|mut lookup| {
                    lookup.projections.push_back(Proj::PrivateField(name));
                    lookup
                })
            }
        }
    }

    fn of_identifier<M: Dupe, T: Dupe>(id: &Identifier<M, T>) -> Self {
        Lookup {
            base: id.name.dupe(),
            projections: FlowVector::new(),
        }
    }

    fn of_this() -> Self {
        Self::of_name(FlowSmolStr::new_inline("this"))
    }

    fn of_super() -> Self {
        Self::of_name(FlowSmolStr::new_inline("super"))
    }

    pub fn of_name(name: FlowSmolStr) -> Self {
        Lookup {
            base: name,
            projections: FlowVector::new(),
        }
    }

    pub fn of_name_with_projections(base: FlowSmolStr, projections: FlowVector<Proj>) -> Self {
        Lookup { base, projections }
    }
}

impl<L: LocSig> RefinementKey<L> {
    pub fn debug_string(&self) -> String {
        format!(
            "{{loc = {:?}; lookup = {}}}",
            self.loc.debug_to_string(false),
            self.lookup.debug_string()
        )
    }
}

impl<L: Dupe> RefinementKey<L> {
    pub fn of_expression<M: Dupe>(expr: &Expression<M, L>) -> Option<Self> {
        let loc = expr.loc().dupe();
        Lookup::of_expression(expr, true).map(|lookup| RefinementKey { loc, lookup })
    }

    pub fn of_argument<M: Dupe>(arg: &ExpressionOrSpread<M, L>) -> Option<Self> {
        match arg {
            ExpressionOrSpread::Spread { .. } => None,
            ExpressionOrSpread::Expression(e) => Self::of_expression(e),
        }
    }

    pub fn of_name(name: FlowSmolStr, loc: L) -> Self {
        RefinementKey {
            loc,
            lookup: Lookup::of_name(name),
        }
    }

    pub fn of_optional_chain<M: Dupe>(expr: &Expression<M, L>) -> Option<Self> {
        match expr.deref() {
            ExpressionInner::Call { .. } | ExpressionInner::Member { .. } => None,
            ExpressionInner::OptionalMember { inner, .. } => {
                // Check if the object is a simple identifier
                if let ExpressionInner::Identifier { loc, inner: id } = inner.member.object.deref()
                {
                    return Some(RefinementKey {
                        loc: loc.dupe(),
                        lookup: Lookup::of_name(id.name.dupe()),
                    });
                }
                // Otherwise recurse into the object
                Self::of_optional_chain(&inner.member.object)
            }
            ExpressionInner::OptionalCall { inner, .. } => {
                Self::of_optional_chain(&inner.call.callee)
            }
            _ => None,
        }
    }

    pub fn reason_desc<DescL: Dupe>(&self) -> VirtualReasonDesc<DescL> {
        if self.lookup.projections.is_empty() {
            VirtualReasonDesc::RIdentifier(self.lookup.base.dupe())
        } else {
            // Get the last projection
            match self.lookup.projections.last() {
                Some(Proj::Prop(x)) => VirtualReasonDesc::RProperty(Some(Name::new(x.dupe()))),
                Some(Proj::PrivateField(x)) => VirtualReasonDesc::RPrivateProperty(x.dupe()),
                Some(Proj::Elem(_)) => VirtualReasonDesc::RProperty(None),
                None => {
                    // This shouldn't happen since we checked is_empty()
                    // but handle it gracefully by treating as identifier
                    VirtualReasonDesc::RIdentifier(self.lookup.base.dupe())
                }
            }
        }
    }
}

impl fmt::Display for Lookup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.debug_string())
    }
}
