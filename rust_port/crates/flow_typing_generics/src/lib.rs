/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Generic type parameter handling for Flow type checker.
//!
//! This module handles the sealing and unsealing of generics, including:
//! - Generic type parameter representation
//! - Bounds and spreads for type variables
//! - ID comparison and satisfaction checking for type flow
//! - ArraySpread submodule for array spread generic handling

#![allow(dead_code)]

use std::fmt;

use flow_aloc::ALocId;
use flow_common::subst_name::OpKind;
use flow_common::subst_name::SubstName;
use flow_common::subst_name::SubstNameInner;
use vec1::Vec1;

/// A generic is the ALoc.id of a type variable definition site (or class
/// definition site for `this`.)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generic {
    pub id: ALocId,
    pub name: SubstName,
}

/// A bound corresponds to the upper bound annotated on a type variable,
/// but it can also include more information if that annotation is itself a
/// type variable. This lets us compress variables bounded by other variables
/// into a single data structure. For example:
///     function f<X: number>(x: X) { ... }
/// The ID for `X` is its definition location and name, and nothing else
/// (because the fact that it is bounded by the type `number` is handled by the
/// `bound` type field in a `GenericT`), but compare:
///     function f<X, Y: X>(y: Y) { ... }
/// Here, `Y` is bounded by `X` which also has an ID. We compress these together
/// for `X` to avoid having nested `GenericT`s, and so the ID for `Y` is its own
/// definition and name, followed by `X`'s ID.
///
/// In the comments of this module, I write such complex ids like "Y:X".
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GenericBound {
    pub generic: Generic,
    pub super_: Option<Box<GenericId>>,
}

/// A spread is a nonempty list of bounds, each of which corresponds to an object
/// spread operator applied to a generic with that bound. For example, in a program
/// like
///     function f<X: {}, Y: {}>(x: X, y: Y): {...X, ...Y} { return {...x, ...y} }
/// the type {...X, ...Y} will be represented as a GenericT, whose type upper bound is
/// {} and whose [GenericId] is `Op(OpKind::Spread, [Y, X])`. (Note that the ordering of this list of
/// generics is reversed from how it appears in values). This information can then be used to enforce
/// that the only values that are well-typed lower bounds for the type are other
/// spreads from the same generics, in the same order--as opposed to the unsoundnesses that
/// currently (e.g. around v0.130) exist:
///
/// ```js
///     function f<X, Y>(x: X, y: Y): {...X, ...Y} {
///      return {...y, ...x}; // out of order!
///     }
/// ```
pub type Spread = Vec1<GenericBound>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GenericId {
    Op(OpKind, Spread),
    Bound(GenericBound),
}

/// Used in the object_kit representation of objects, representing whatever generics,
/// if any, have been spread into an object
pub type SpreadId = Vec<GenericBound>;

/// Result of checking if one generic ID satisfies another's requirements
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GenericSatResult {
    Satisfied,
    Lower(GenericId),
    Upper(GenericId),
}

impl GenericBound {
    fn to_string(&self, code: bool) -> String {
        let s = match &self.super_ {
            None => SubstName::string_of_subst_name(&self.generic.name).to_string(),
            Some(id) => format!(
                "{}:{}",
                SubstName::string_of_subst_name(&self.generic.name),
                id
            ),
        };
        if code { format!("`{}`", s) } else { s }
    }

    fn equal(&self, other: &GenericBound) -> bool {
        if self.generic.id != other.generic.id {
            return false;
        }
        match (&self.super_, &other.super_) {
            (None, None) => true,
            (Some(sup1), Some(sup2)) => sup1.equal(sup2),
            _ => false,
        }
    }
}

impl GenericId {
    /// The definition site of a bound generic; operations have no definition site.
    pub fn definition_id(&self) -> Option<&ALocId> {
        match self {
            GenericId::Bound(bound) => Some(&bound.generic.id),
            GenericId::Op(_, _) => None,
        }
    }

    fn all_subst_names(&self) -> Vec<SubstName> {
        match self {
            GenericId::Bound(bound) => bound.all_subst_names(),
            GenericId::Op(_, bounds) => bounds.iter().flat_map(|b| b.all_subst_names()).collect(),
        }
    }

    fn to_kind(&self) -> Option<OpKind> {
        match self {
            GenericId::Bound(_) => None,
            GenericId::Op(op_kind, _) => Some(op_kind.clone()),
        }
    }

    pub fn subst_name(&self) -> SubstName {
        match self {
            GenericId::Bound(bound) if bound.super_.is_none() => bound.generic.name.clone(),
            _ => SubstName::synthetic(
                self.to_string().into(),
                self.to_kind(),
                self.all_subst_names(),
            ),
        }
    }

    pub fn equal(&self, other: &GenericId) -> bool {
        match (self, other) {
            (GenericId::Bound(b1), GenericId::Bound(b2)) => b1.equal(b2),
            (GenericId::Op(k1, bs1), GenericId::Op(k2, bs2)) => {
                (*k1 == OpKind::Keys) == (*k2 == OpKind::Keys)
                    && equal_spreads(bs1.as_slice(), bs2.as_slice())
            }
            _ => false,
        }
    }

    pub fn collapse(&self, u: &GenericId) -> Option<GenericId> {
        match self {
            GenericId::Bound(GenericBound {
                generic,
                super_: None,
            }) => Some(GenericId::Bound(GenericBound {
                generic: generic.clone(),
                super_: Some(Box::new(u.clone())),
            })),
            GenericId::Bound(GenericBound {
                generic,
                super_: Some(u_prime),
            }) => u_prime.collapse(u).map(|super_| {
                GenericId::Bound(GenericBound {
                    generic: generic.clone(),
                    super_: Some(Box::new(super_)),
                })
            }),
            GenericId::Op(_, _) => None,
        }
    }

    pub fn make_spread(&self) -> SpreadId {
        match self {
            GenericId::Op(_, spread) => spread.to_vec(),
            GenericId::Bound(bound) => vec![bound.clone()],
        }
    }

    pub fn make_bound_id(aloc_id: ALocId, name: SubstName) -> GenericId {
        GenericId::Bound(GenericBound {
            generic: Generic { id: aloc_id, name },
            super_: None,
        })
    }

    pub fn make_op_id(op_kind: OpKind, opt: SpreadId) -> Option<GenericId> {
        Vec1::try_from_vec(opt)
            .ok()
            .map(|spread| GenericId::Op(op_kind, spread))
    }

    pub fn fold_ids<F, A>(&self, f: F, acc: A) -> A
    where
        F: Fn(ALocId, &SubstName, A) -> A + Copy,
    {
        let test_bound = |acc: A, bound: &GenericBound| -> A {
            match &bound.super_ {
                Some(super_id) => {
                    let acc = super_id.fold_ids(f, acc);
                    f(bound.generic.id.clone(), &bound.generic.name, acc)
                }
                None => f(bound.generic.id.clone(), &bound.generic.name, acc),
            }
        };
        match self {
            GenericId::Bound(bound) => test_bound(acc, bound),
            GenericId::Op(_, spread) => spread.iter().fold(acc, test_bound),
        }
    }

    /// When flowing one generic into another, determine if the lower generic's ID
    /// satisfies the requirements of the upper, allowing the type check to be inclusion
    /// rather than bound-to-bound. In the simple case where all generic
    /// IDs were just ALoc.ids, this would just be equality and return a boolean success
    /// or failure, and in the failure case, we'd flow the generic bound of the lower
    /// type into the upper generic.
    ///
    /// === Complex bounds ===
    ///
    /// Since IDs now can represent multiple generics, we need to do more work, and there's
    /// multiple possible results. For example, suppose X and Y are generics, and Y's upper
    /// bound is X. Then the Generic.id for Y is "Y:X". When a GenericT for Y flows into a
    /// GenericT for X, we check "Y:X" against "X" to see if it's satisfied. Y =/= X,
    /// so we "strip off" Y from Y:X and check X against X, which succeds, so we return
    /// Satisfied.
    ///
    /// If Z:W is not bounded by X, and we check if Z:W satisfies X, we first strip off
    /// Z, then see that W doesn't satisfy X. This then returns "Upper X"--this means
    /// that after matching generics against each other as much as possible, X was still
    /// unsatisfied. In flow_js, the caller, this means that we'll flow the upper bound
    /// of the lower GenericT into empty, and presumably raise an error.
    ///
    /// On the other hand, what if we have a program like:
    ///
    ///    function g<X, Y: X | number>(a: {y: Y}): {y: Y} {
    ///      if (typeof a.y !== 'number') {
    ///        return a;
    ///      }
    ///      // ...
    ///    }
    ///
    /// When we return a, we check the type of `a.y` against Y invariantly.
    /// Because we've refined a.y, we're able to collapse it into a single GenericT
    /// with id `Y:X`. However, the `Y` in the return type hasn't been refined, so it's
    /// a GenericT with id `Y`, whose bound is the union of NumT and a GenericT for X.
    ///
    /// When we flow the type of a.y into Y, we're comparing the ids `Y:X` and `Y`. We see
    /// that they match, but then there's a hanging `X` on the left. We therefore return
    /// Lower X, which tells flow_js that it needs to flow a GenericT with id X on the left
    /// into the bound of the GenericT on the right--in this case, X | number.
    ///
    /// === Spreads ===
    ///
    /// This function is also used to determine whether two spreaded generics are
    /// compatible. For the most part, this logic is orthogonal to compatibility of
    /// bounds, although a generic can flow into a spread that contains only
    /// itself--this should be safe:
    ///     function f<X: {}>(x: X): {...X} {
    ///       return x;
    ///     }
    ///
    /// The rules below show examples of what scenario triggers them.
    pub fn satisfies(&self, printer: &dyn Fn(&[String]), other: &GenericId) -> GenericSatResult {
        fn opt_satisfies(
            printer: &dyn Fn(&[String]),
            o1: Option<&GenericId>,
            o2: Option<&GenericId>,
        ) -> GenericSatResult {
            match (o1, o2) {
                // everything in id2 was satisfied by id1 with nothing left
                (None, None) => {
                    printer(&["Generics satisfied".to_string()]);
                    GenericSatResult::Satisfied
                }
                // everything in id2 was satisfied, but there's generics on the left still
                // unmatched
                (Some(id), None) => {
                    printer(&[format!(
                        "Generics satisfied, with {} unmatched on lower bound",
                        id
                    )]);
                    GenericSatResult::Lower(id.clone())
                }
                // something in id2 wasn't satisfied
                (None, Some(id)) => {
                    printer(&[format!(
                        "Generics unsatisfied, with {} unmatched on upper bound",
                        id
                    )]);
                    GenericSatResult::Upper(id.clone())
                }
                // continue to the next layer
                (Some(id1), Some(id2)) => id1.satisfies(printer, id2),
            }
        }

        fn bound_satisfies(
            printer: &dyn Fn(&[String]),
            b1: &GenericBound,
            b2: &GenericBound,
        ) -> GenericSatResult {
            if b1.generic.id == b2.generic.id {
                // if top-level ids match, strip them off and compare the next levels, if they exist
                opt_satisfies(printer, b1.super_.as_deref(), b2.super_.as_deref())
            } else {
                match &b1.super_ {
                    Some(super_id) => {
                        // if the top-level ids don't match but the LHS has more to look at, strip it off and try again
                        super_id.satisfies(printer, &GenericId::Bound(b2.clone()))
                    }
                    None => {
                        // something in id2 wasn't satisfied
                        printer(&[format!(
                            "Generics unsatisfied, with {} unmatched on upper bound",
                            b2.to_string(false)
                        )]);
                        GenericSatResult::Upper(GenericId::Bound(b2.clone()))
                    }
                }
            }
        }

        printer(&[format!(
            "Checking generics compatibility: {} ~> {}",
            self, other
        )]);
        match (self, other) {
            // `keyof` of a generic is not interchangeable with the generic, only with itself.
            (GenericId::Op(OpKind::Keys, s1), GenericId::Op(OpKind::Keys, s2))
                if equal_spreads(s1.as_slice(), s2.as_slice()) =>
            {
                printer(&["Generics satisfied".to_string()]);
                GenericSatResult::Satisfied
            }
            (GenericId::Op(OpKind::Keys, _), _) | (_, GenericId::Op(OpKind::Keys, _)) => {
                printer(&[format!(
                    "Generics unsatisfied, with {} unmatched on upper bound",
                    other
                )]);
                GenericSatResult::Upper(other.clone())
            }
            (GenericId::Bound(bound1), GenericId::Bound(bound2)) => {
                bound_satisfies(printer, bound1, bound2)
            }
            // A generic is interchangeable with a spread of itself,
            // as long as the spread doesn't contain other generics, and as long as the type bounds
            // are compatible too. Example:
            //
            //   function f<X: {}>(x: X): {...X} {
            //     return x; // ok
            //   }
            (GenericId::Bound(bound1), GenericId::Op(_, bounds2)) if bounds2.len() == 1 => {
                bound_satisfies(printer, bound1, &bounds2[0])
            }
            // A 'bound' generic only represents one bound, so it can't
            // satisfy a spread of more than one generic on its own. We can see if it's bounded by a
            // 'spread', though. Example:
            //
            //  function f<X: {}, Y: {}, Z: {...X, ...Y}>(z: Z): {...X, ...Y} {
            //    return z; // ok
            //  }
            (
                GenericId::Bound(GenericBound {
                    super_: Some(id1), ..
                }),
                GenericId::Op(_, _),
            ) => id1.satisfies(printer, other),
            // As above, but if it's not bounded by a spread, we can't do
            // anything but strip off the generic from the lower type.
            //
            //  function f<X: {}, Y: {}>(y: Y): {...X, ...Y} {
            //    return y; // should error
            //  }
            (GenericId::Bound(GenericBound { super_: None, .. }), GenericId::Op(_, _)) => {
                printer(&[
                    "Generics unsatisfied: single bound cannot satisfy spread with multiple elements"
                        .to_string(),
                ]);
                GenericSatResult::Upper(other.clone())
            }
            // A spread can flow into a bound if the LAST generic that was
            // spread matches the lower bound (and assuming the types match).
            //
            //  function fa<X: {}, Y: {}>(y: {...X, ...Y}): Y {
            //    return y;
            //  }
            //
            //  function fb<X: {}, Y: {}>(y: {...Y, ...X}): Y {
            //    return y; // should error (but didn't previously with old generics)
            //  }
            (GenericId::Op(_, bounds), GenericId::Bound(_)) => {
                let bound1 = bounds.last();
                GenericId::Bound(bound1.clone()).satisfies(printer, other)
            }
            // If an upper bound spread expects more generic components in the spread than are provided in
            // the lower bound, it clearly can't be satisfied
            (GenericId::Op(_, s1), GenericId::Op(_, s2)) if s2.len() > s1.len() => {
                printer(&[
                    "Generics unsatisfied: more elements in upper bound than lower bound"
                        .to_string(),
                ]);
                GenericSatResult::Upper(other.clone())
            }
            // When comparing two spreads, we drop the tail elements of the lower bound so that
            // its length matches the upper bound, and then we compare the elements pairwise. If they're
            // all satisfied, then the spreads are satisfied. Recall the invariant that any generic exists
            // only once in the spread list.
            //
            //  function a<X: {}, Y: {}, Z: {}>(x: X, y: Y) {
            //   ({...x, ...y}: {...X}); // should be error
            //   ({...y, ...x}: {...X}); // yup
            //   ({...x}: {...Y, ...X}); // nope
            //   ({...y, ...x}: {...X, ...Y}); // should be error
            //   ({...x, ...y}: {...X, ...Y}); // yup
            //   ({...x, ...y}: {...X, ...Y, ...Y}); // yup
            //   ({...x, ...y}: {...Y, ...X, ...Y}); // yup
            //   ({...x, ...y}: {...X, ...Z, ...Y}); // nope
            // }
            (GenericId::Op(_, s1), GenericId::Op(_, s2)) => {
                let s1_slice = &s1.as_slice()[s1.len() - s2.len()..];
                let is_satisfied = |sat: &GenericSatResult| {
                    matches!(
                        sat,
                        GenericSatResult::Satisfied | GenericSatResult::Lower(_)
                    )
                };

                if s1_slice.iter().zip(s2.iter()).all(|(b1, b2)| {
                    printer(&[format!(
                        "Checking generics compatibility (from spread): {} ~> {}",
                        b1.to_string(false),
                        b2.to_string(false)
                    )]);
                    is_satisfied(&bound_satisfies(printer, b1, b2))
                }) {
                    printer(&["Generics satisfied: all elements of spread satisfied".to_string()]);
                    GenericSatResult::Satisfied
                } else {
                    printer(&[
                        "Generics unsatisfied: at least one element of spread unsatisfied"
                            .to_string(),
                    ]);
                    GenericSatResult::Upper(other.clone())
                }
            }
        }
    }
}

impl fmt::Display for GenericId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenericId::Bound(bound) => write!(f, "{}", bound.to_string(true)),
            GenericId::Op(OpKind::Spread, ids) => {
                let bounds_str = ids
                    .iter()
                    .map(|b| b.to_string(false))
                    .collect::<Vec<_>>()
                    .join(", ...");
                write!(f, "`{{ ...{} }}`", bounds_str)
            }
            GenericId::Op(OpKind::Partial, ids) if ids.len() == 1 => {
                write!(f, "`Partial<{}>", ids[0].to_string(false))
            }
            GenericId::Op(OpKind::Required, ids) if ids.len() == 1 => {
                write!(f, "`Required<{}>", ids[0].to_string(false))
            }
            GenericId::Op(OpKind::ReadOnly, ids) if ids.len() == 1 => {
                write!(f, "`$ReadOnly<{}>", ids[0].to_string(false))
            }
            GenericId::Op(_, ids) => {
                let bounds_str = ids
                    .iter()
                    .map(|b| b.to_string(true))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "type including generic type(s) {}", bounds_str)
            }
        }
    }
}

impl GenericBound {
    fn all_subst_names(&self) -> Vec<SubstName> {
        match &*self.generic.name {
            SubstNameInner::Synthetic { ts, .. } if self.super_.is_none() => ts.clone(),
            _ if self.super_.is_none() => vec![self.generic.name.clone()],
            SubstNameInner::Synthetic { ts, .. } => {
                let mut result = ts.clone();
                if let Some(ref super_id) = self.super_ {
                    result.extend(super_id.all_subst_names());
                }
                result
            }
            _ => {
                let mut result = vec![self.generic.name.clone()];
                if let Some(ref super_id) = self.super_ {
                    result.extend(super_id.all_subst_names());
                }
                result
            }
        }
    }
}

fn equal_spreads(s1: &[GenericBound], s2: &[GenericBound]) -> bool {
    if s1.len() != s2.len() {
        return false;
    }
    s1.iter().zip(s2.iter()).all(|(a, b)| a.equal(b))
}

pub fn spread_empty() -> SpreadId {
    Vec::new()
}

pub fn spread_subtract(id1: &SpreadId, id2: &SpreadId) -> SpreadId {
    id1.iter()
        .filter(|a| !id2.iter().any(|b| a.equal(b)))
        .cloned()
        .collect()
}

/// We here enforce the invariant that no bound appears more than once
/// in the spread list.
pub fn spread_append(id1: &SpreadId, id2: &SpreadId) -> SpreadId {
    let mut result = spread_subtract(id1, id2);
    result.extend(id2.iter().cloned());
    result
}

pub fn spread_exists(spread: &SpreadId) -> bool {
    !spread.is_empty()
}

/// ArraySpread submodule for handling array spread generics
pub mod array_spread {
    use super::*;

    /// Read-only status of array spread
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum RoStatus {
        NonROSpread,
        ROSpread,
    }

    /// Simple lattice for seeing if all elements of a spread have the same generic ID
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum T {
        Bottom,
        Top,
        Generic(GenericId, RoStatus),
    }

    /// A generic array spread of a read-only array type can only be reconstructed into a generic if the
    /// spread consists of exactly one element, which is the read-only array.
    fn merge_ro(t: T, s: RoStatus) -> T {
        match (&t, s) {
            (T::Bottom, _) | (T::Top, _) => t,
            (T::Generic(_, _), RoStatus::ROSpread) => T::Top,
            (T::Generic(id, _), s) => T::Generic(id.clone(), s),
        }
    }

    pub fn merge(
        printer: &dyn Fn(&[String]),
        t: T,
        g: Option<&GenericId>,
        ro_prime: RoStatus,
    ) -> T {
        match (t, g) {
            (T::Bottom, Some(stat)) => T::Generic(stat.clone(), ro_prime),
            (T::Generic(_, _), None) | (T::Top, _) => T::Top,
            (T::Generic(id, _), Some(id_prime)) if id.equal(id_prime) => {
                merge_ro(T::Generic(id, ro_prime), ro_prime)
            }
            (T::Generic(id, ro), Some(id_prime)) => {
                match (
                    id_prime.satisfies(printer, &id),
                    id.satisfies(printer, id_prime),
                ) {
                    (GenericSatResult::Upper(_), GenericSatResult::Upper(_)) => T::Top,
                    (GenericSatResult::Upper(_), _) => {
                        merge_ro(T::Generic(id_prime.clone(), ro_prime), ro)
                    }
                    _ => merge_ro(T::Generic(id, ro_prime), ro_prime),
                }
            }
            _ => T::Top,
        }
    }

    pub fn to_option(t: &T) -> Option<&GenericId> {
        match t {
            T::Generic(id, _) => Some(id),
            _ => None,
        }
    }
}
