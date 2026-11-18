/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

use dupe::Dupe;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;

/// Represents the kind of operation for synthetic names
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum OpKind {
    Spread,
    MakeExact,
    ReadOnly,
    MappedObject,
    MappedArray,
    Partial,
    Required,
    CheckConfig,
    CreateElement,
    ReactConfig,
    Keys,
}

impl fmt::Display for OpKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpKind::Spread => write!(f, "Spread"),
            OpKind::MakeExact => write!(f, "MakeExact"),
            OpKind::ReadOnly => write!(f, "ReadOnly"),
            OpKind::MappedObject => write!(f, "MappedObject"),
            OpKind::MappedArray => write!(f, "MappedArray"),
            OpKind::Partial => write!(f, "Partial"),
            OpKind::Required => write!(f, "Required"),
            OpKind::CheckConfig => write!(f, "CheckConfig"),
            OpKind::CreateElement => write!(f, "CreateElement"),
            OpKind::ReactConfig => write!(f, "ReactConfig"),
            OpKind::Keys => write!(f, "Keys"),
        }
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum SubstNameInner {
    Name(FlowSmolStr),
    Id(i32, FlowSmolStr),
    Synthetic {
        name: FlowSmolStr,
        op_kind: Option<OpKind>,
        ts: Vec<SubstName>,
    },
}

/// Subst names are used to represent the names of type parameters.
/// - A `Name` is just a typical name, originating directly from the source.
/// - An `Id` is used when we perform a substitution where a variable needs
///   to be renamed to avoid capture--this is the classic capture avoiding
///   substitution problem in the lambda calculus. When we rename a variable,
///   we rename it to an `Id` with the original name but also an integer,
///   to remove it from free variables.
/// - A `Synthetic` name is used when converting from more complex generic
///   encodings such as spreads, where a single GenericT can represent more
///   than one source-level generic. Such GenericTs should never be the targets
///   of substitution, because they only arise through constraint solving (rather
///   than in signatures where they can be replaced in substitutions). We enforce
///   this by erroring if a Synthetic name is substituted.
#[derive(Debug, Clone)]
pub struct SubstName(Arc<SubstNameInner>);

impl SubstName {
    pub fn new(inner: SubstNameInner) -> Self {
        SubstName(Arc::new(inner))
    }

    pub fn name(n: FlowSmolStr) -> Self {
        Self::new(SubstNameInner::Name(n))
    }

    pub fn id(id: i32, n: FlowSmolStr) -> Self {
        Self::new(SubstNameInner::Id(id, n))
    }

    pub fn synthetic(name: FlowSmolStr, op_kind: Option<OpKind>, ts: Vec<SubstName>) -> Self {
        Self::new(SubstNameInner::Synthetic { name, op_kind, ts })
    }

    pub fn string_of_subst_name(&self) -> &FlowSmolStr {
        match self.deref() {
            SubstNameInner::Synthetic { name, .. }
            | SubstNameInner::Name(name)
            | SubstNameInner::Id(_, name) => name,
        }
    }

    /// non-synthetic names.
    pub fn formatted_string_of_subst_name(&self) -> String {
        match self.deref() {
            SubstNameInner::Synthetic { name, .. } => name.as_str().to_owned(),
            SubstNameInner::Name(name) | SubstNameInner::Id(_, name) => format!("`{}`", name),
        }
    }
}

impl Deref for SubstName {
    type Target = SubstNameInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Dupe for SubstName {}

impl PartialEq for SubstName {
    fn eq(&self, other: &Self) -> bool {
        // First try pointer equality for fast path
        Arc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

impl Eq for SubstName {}

impl PartialOrd for SubstName {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SubstName {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if Arc::ptr_eq(&self.0, &other.0) {
            std::cmp::Ordering::Equal
        } else {
            self.0.cmp(&other.0)
        }
    }
}

impl std::hash::Hash for SubstName {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

impl serde::Serialize for SubstName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for SubstName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(SubstName(Arc::new(SubstNameInner::deserialize(
            deserializer,
        )?)))
    }
}

impl fmt::Display for SubstName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.deref() {
            SubstNameInner::Name(n) => write!(f, "Name({})", n),
            SubstNameInner::Id(id, n) => write!(f, "Id({}, {})", id, n),
            SubstNameInner::Synthetic { name, op_kind, ts } => {
                write!(f, "Synthetic(name: {}, op_kind: ", name)?;
                match op_kind {
                    Some(ok) => write!(f, "Some({})", ok)?,
                    None => write!(f, "None")?,
                }
                write!(f, ", ts: [")?;
                for (i, t) in ts.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", t)?;
                }
                write!(f, "])")
            }
        }
    }
}
