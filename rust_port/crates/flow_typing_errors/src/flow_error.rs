/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::reason::Reason;
use flow_common_errors::error_codes::ErrorCode;
use flow_common_errors::error_utils::ErrorKind;
use flow_data_structure_wrapper::ord_set::ConsumingIter;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::ord_set::Iter as OrdSetIter;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc;
use flow_parser::loc_sig::LocSig;

use super::error_message::ErrorMessage;
use super::error_message::TypeOrTypeDesc;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct FlowError<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub loc: Option<L>,
    pub msg: ErrorMessage<L>,
    pub source_file: FileKey,
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> FlowError<L> {
    pub fn loc_of_error(&self) -> Option<&L> {
        self.loc.as_ref()
    }

    pub fn msg_of_error(&self) -> &ErrorMessage<L> {
        &self.msg
    }

    pub fn code_of_error(&self) -> Option<ErrorCode> {
        self.msg.error_code_of_message()
    }

    pub fn source_file(&self) -> &FileKey {
        &self.source_file
    }

    pub fn kind_of_error(&self) -> ErrorKind {
        self.msg.kind_of_msg()
    }

    pub fn map_loc_of_error<F, M>(f: F, error: FlowError<L>) -> FlowError<M>
    where
        F: Fn(L) -> M + Copy,
        L: Clone,
        M: Clone + Dupe + PartialEq + Eq + PartialOrd + Ord,
    {
        FlowError {
            loc: error.loc.map(f),
            msg: ErrorMessage::map_loc_of_error_message(f, error.msg),
            source_file: error.source_file,
        }
    }

    pub fn convert_type_to_type_desc(
        f: &mut dyn FnMut(TypeOrTypeDesc<L>) -> TypeOrTypeDesc<L>,
        error: FlowError<L>,
    ) -> FlowError<L>
    where
        L: Clone,
    {
        FlowError {
            loc: error.loc,
            msg: ErrorMessage::convert_type_to_type_desc(f, error.msg),
            source_file: error.source_file,
        }
    }
}

pub fn error_of_msg<L>(source_file: FileKey, msg: ErrorMessage<L>) -> FlowError<L>
where
    L: Clone + Dupe + PartialEq + Eq + PartialOrd + Ord,
{
    let loc = msg.loc_of_msg();
    FlowError {
        loc,
        msg,
        source_file,
    }
}

// Decide reason order based on UB's flavor and blamability.
// If the order is unchanged, maintain reference equality.
pub fn ordered_reasons(
    reasons: (Reason, Reason),
    is_lib: impl Fn(&FileKey) -> bool,
) -> (Reason, Reason) {
    let is_blamable = |reason: &Reason| {
        *reason.loc() != ALoc::none() && !reason.loc().source().is_some_and(&is_lib)
    };
    let (rl, ru) = reasons;
    if is_blamable(&ru) && !is_blamable(&rl) {
        (ru, rl)
    } else {
        (rl, ru)
    }
}

#[derive(Debug, Clone, Dupe, PartialEq)]
pub struct ErrorSet(FlowOrdSet<FlowError<ALoc>>);

impl serde::Serialize for ErrorSet {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.elements().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for ErrorSet {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let errors = Vec::<FlowError<ALoc>>::deserialize(deserializer)?;
        Ok(errors.into_iter().collect())
    }
}

impl Default for ErrorSet {
    fn default() -> Self {
        Self::new()
    }
}

impl ErrorSet {
    pub fn new() -> Self {
        thread_local! {
            static CACHED: FlowOrdSet<FlowError<ALoc>> = FlowOrdSet::new();
        }
        Self(CACHED.with(|c| c.clone()))
    }

    pub fn empty() -> Self {
        Self::new()
    }

    pub fn singleton(error: FlowError<ALoc>) -> Self {
        let mut set = Self::new();
        set.0.insert(error);
        set
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }

    pub fn add(&mut self, error: FlowError<ALoc>) {
        self.0.insert(error);
    }

    pub fn mem(&self, error: &FlowError<ALoc>) -> bool {
        self.0.contains(error)
    }

    pub fn remove(&mut self, error: &FlowError<ALoc>) {
        self.0.remove(error);
    }

    pub fn union(&self, other: &Self) -> Self {
        if other.0.is_empty() {
            return self.dupe();
        }
        if self.0.is_empty() {
            return other.dupe();
        }
        if self.0.ptr_eq(&other.0) {
            return self.dupe();
        }
        Self(
            self.0
                .dupe()
                .into_inner()
                .union(other.0.dupe().into_inner())
                .into(),
        )
    }

    pub fn union_mut(&mut self, other: Self) {
        for error in other.0 {
            self.0.insert(error);
        }
    }

    pub fn inter(&self, other: &Self) -> Self {
        Self(
            self.0
                .dupe()
                .into_inner()
                .intersection(other.0.dupe().into_inner())
                .into(),
        )
    }

    pub fn diff(&self, other: &Self) -> Self {
        Self(
            self.0
                .dupe()
                .into_inner()
                .relative_complement(other.0.dupe().into_inner())
                .into(),
        )
    }

    pub fn iter(&self) -> impl Iterator<Item = &FlowError<ALoc>> {
        self.0.iter()
    }

    pub fn fold<A, F>(self, init: A, f: F) -> A
    where
        F: FnMut(A, FlowError<ALoc>) -> A,
    {
        self.0.into_iter().fold(init, f)
    }

    pub fn for_all<F>(&self, f: F) -> bool
    where
        F: Fn(&FlowError<ALoc>) -> bool,
    {
        self.0.iter().all(f)
    }

    pub fn exists<F>(&self, f: F) -> bool
    where
        F: Fn(&FlowError<ALoc>) -> bool,
    {
        self.0.iter().any(f)
    }

    pub fn filter<F>(self, f: F) -> Self
    where
        F: Fn(&FlowError<ALoc>) -> bool,
    {
        // Collecting allocates a new set, even an empty one.
        if self.0.is_empty() {
            return self;
        }
        Self(self.0.into_iter().filter(|e| f(e)).collect())
    }

    pub fn filter_ref<F>(&self, f: F) -> Self
    where
        F: Fn(&FlowError<ALoc>) -> bool,
    {
        Self(self.0.iter().filter(|e| f(e)).cloned().collect())
    }

    pub fn partition<F>(self, f: F) -> (Self, Self)
    where
        F: Fn(&FlowError<ALoc>) -> bool,
    {
        let (yes, no): (Vec<_>, Vec<_>) = self.0.into_iter().partition(|e| f(e));
        (yes.into_iter().collect(), no.into_iter().collect())
    }

    pub fn map<F>(self, f: F) -> Self
    where
        F: FnMut(FlowError<ALoc>) -> FlowError<ALoc>,
    {
        Self(self.0.into_iter().map(f).collect())
    }

    pub fn elements(&self) -> Vec<FlowError<ALoc>> {
        self.0.iter().cloned().collect()
    }

    pub fn is_lint_only_errorset(&self) -> bool {
        self.0.iter().all(|e| e.msg.defered_in_speculation())
    }

    pub fn into_inner(self) -> FlowOrdSet<FlowError<ALoc>> {
        self.0
    }
}

impl FromIterator<FlowError<ALoc>> for ErrorSet {
    fn from_iter<I: IntoIterator<Item = FlowError<ALoc>>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl IntoIterator for ErrorSet {
    type Item = FlowError<ALoc>;
    type IntoIter = ConsumingIter<FlowError<ALoc>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a ErrorSet {
    type Item = &'a FlowError<ALoc>;
    type IntoIter = OrdSetIter<'a, FlowError<ALoc>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl Extend<FlowError<ALoc>> for ErrorSet {
    fn extend<I: IntoIterator<Item = FlowError<ALoc>>>(&mut self, iter: I) {
        for error in iter {
            self.add(error);
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConcreteErrorSet(FlowOrdSet<FlowError<Loc>>);

impl Dupe for ConcreteErrorSet {}

impl Default for ConcreteErrorSet {
    fn default() -> Self {
        Self::new()
    }
}

impl ConcreteErrorSet {
    pub fn new() -> Self {
        thread_local! {
            static CACHED: FlowOrdSet<FlowError<Loc>> = FlowOrdSet::new();
        }
        Self(CACHED.with(|c| c.clone()))
    }

    pub fn empty() -> Self {
        Self::new()
    }

    pub fn singleton(error: FlowError<Loc>) -> Self {
        let mut set = Self::new();
        set.0.insert(error);
        set
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn add(&mut self, error: FlowError<Loc>) {
        self.0.insert(error);
    }

    pub fn mem(&self, error: &FlowError<Loc>) -> bool {
        self.0.contains(error)
    }

    pub fn remove(&mut self, error: &FlowError<Loc>) {
        self.0.remove(error);
    }

    pub fn union(&self, other: &Self) -> Self {
        if self.0.is_empty() {
            return other.dupe();
        }
        if other.0.is_empty() {
            return self.dupe();
        }
        if self.0.ptr_eq(&other.0) {
            return self.dupe();
        }
        Self(
            self.0
                .dupe()
                .into_inner()
                .union(other.0.dupe().into_inner())
                .into(),
        )
    }

    pub fn union_mut(&mut self, other: Self) {
        for error in other.0 {
            self.0.insert(error);
        }
    }

    pub fn inter(&self, other: &Self) -> Self {
        Self(
            self.0
                .dupe()
                .into_inner()
                .intersection(other.0.dupe().into_inner())
                .into(),
        )
    }

    pub fn diff(&self, other: &Self) -> Self {
        Self(
            self.0
                .dupe()
                .into_inner()
                .relative_complement(other.0.dupe().into_inner())
                .into(),
        )
    }

    pub fn iter(&self) -> impl Iterator<Item = &FlowError<Loc>> {
        self.0.iter()
    }

    pub fn fold<A, F>(self, init: A, f: F) -> A
    where
        F: FnMut(A, FlowError<Loc>) -> A,
    {
        self.0.into_iter().fold(init, f)
    }

    pub fn for_all<F>(&self, f: F) -> bool
    where
        F: Fn(&FlowError<Loc>) -> bool,
    {
        self.0.iter().all(f)
    }

    pub fn exists<F>(&self, f: F) -> bool
    where
        F: Fn(&FlowError<Loc>) -> bool,
    {
        self.0.iter().any(f)
    }

    pub fn filter<F>(self, f: F) -> Self
    where
        F: Fn(&FlowError<Loc>) -> bool,
    {
        Self(self.0.into_iter().filter(|e| f(e)).collect())
    }

    pub fn filter_ref<F>(&self, f: F) -> Self
    where
        F: Fn(&FlowError<Loc>) -> bool,
    {
        Self(self.0.iter().filter(|e| f(e)).cloned().collect())
    }

    pub fn partition<F>(self, f: F) -> (Self, Self)
    where
        F: Fn(&FlowError<Loc>) -> bool,
    {
        let (yes, no): (Vec<_>, Vec<_>) = self.0.into_iter().partition(|e| f(e));
        (yes.into_iter().collect(), no.into_iter().collect())
    }

    pub fn map<F>(self, f: F) -> Self
    where
        F: Fn(FlowError<Loc>) -> FlowError<Loc>,
    {
        Self(self.0.into_iter().map(f).collect())
    }

    pub fn elements(&self) -> Vec<FlowError<Loc>> {
        self.0.iter().cloned().collect()
    }

    pub fn into_inner(self) -> FlowOrdSet<FlowError<Loc>> {
        self.0
    }
}

impl FromIterator<FlowError<Loc>> for ConcreteErrorSet {
    fn from_iter<I: IntoIterator<Item = FlowError<Loc>>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl IntoIterator for ConcreteErrorSet {
    type Item = FlowError<Loc>;
    type IntoIter = ConsumingIter<FlowError<Loc>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a ConcreteErrorSet {
    type Item = &'a FlowError<Loc>;
    type IntoIter = OrdSetIter<'a, FlowError<Loc>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl Extend<FlowError<Loc>> for ConcreteErrorSet {
    fn extend<I: IntoIterator<Item = FlowError<Loc>>>(&mut self, iter: I) {
        for error in iter {
            self.add(error);
        }
    }
}
