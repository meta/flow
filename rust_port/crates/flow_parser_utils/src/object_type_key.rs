/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! How to read an unlabeled bracketed key in an object type body.
//!
//! `{[K]: V}` is ambiguous between a computed key, as in the value-level
//! `{[k]: v}`, and an index signature. The distinction is made from the way the
//! key is written, so both the checking and the signature pipeline reach the
//! same answer for the same source without resolving anything:
//!
//! * a string or number literal names the property it spells,
//! * a name that binds a value is a computed key, read as the value's type,
//! * everything else, including every name that binds a type, is an index
//!   signature.
//!
//! Only the last step needs to know what a name binds, and each pipeline
//! answers that from the bindings it has already collected.

use std::ops::Deref;

use dupe::Dupe;
use flow_common::js_number::numeric_property_name;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast::Identifier;
use flow_parser::ast::types;

/// The way an unlabeled bracketed key is written.
pub enum ObjectTypeKeyForm<'a, M: Dupe, T: Dupe> {
    /// A string or number literal, naming the property it spells.
    Literal(FlowSmolStr),
    /// A bare or qualified name. It is a computed key when `head` binds a
    /// value and an index signature when it binds a type, which only the
    /// caller can tell. `loc` and `generic` are the node the name was read
    /// from, so a caller that reads it as a value need not match on it again.
    Name {
        loc: &'a T,
        generic: &'a types::Generic<M, T>,
        head: &'a Identifier<M, T>,
    },
    /// An index signature whatever the surrounding bindings are.
    IndexSignature,
}

/// Classify an unlabeled bracketed object-type key by the way it is written.
/// A labeled key, `[label: K]`, is an index signature outright and must not
/// reach here.
pub fn object_type_key_form<'a, M: Dupe, T: Dupe>(
    key: &'a types::Type<M, T>,
) -> ObjectTypeKeyForm<'a, M, T> {
    match key.deref() {
        types::TypeInner::StringLiteral { literal, .. } => {
            ObjectTypeKeyForm::Literal(literal.value.dupe())
        }
        types::TypeInner::NumberLiteral { literal, .. }
            if let Some(name) = numeric_property_name(literal.value) =>
        {
            ObjectTypeKeyForm::Literal(name)
        }
        // Type arguments make this the application of a generic type, never a
        // value reference.
        types::TypeInner::Generic { loc, inner } if inner.targs.is_none() => {
            match generic_head(&inner.id) {
                Some(head) => ObjectTypeKeyForm::Name {
                    loc,
                    generic: inner,
                    head,
                },
                None => ObjectTypeKeyForm::IndexSignature,
            }
        }
        _ => ObjectTypeKeyForm::IndexSignature,
    }
}

/// The leftmost identifier of a type name, whose binding decides whether the
/// whole name refers to a value or to a type. `import('m').X` has no such
/// identifier.
fn generic_head<'a, M: Dupe, T: Dupe>(
    id: &'a types::generic::Identifier<M, T>,
) -> Option<&'a Identifier<M, T>> {
    let mut id = id;
    loop {
        match id {
            types::generic::Identifier::Unqualified(head) => return Some(head),
            types::generic::Identifier::Qualified(qualified) => id = &qualified.qualification,
            types::generic::Identifier::ImportTypeAnnot(_) => return None,
        }
    }
}
