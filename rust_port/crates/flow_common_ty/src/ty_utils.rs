/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::loc_sig::LocSig;

use crate::ty::ALocTy;
use crate::ty::BotKind;
use crate::ty::BuiltinOrSymbol;
use crate::ty::Decl;
use crate::ty::DeclEnumDeclData;
use crate::ty::DeclNominalComponentDeclData;
use crate::ty::Elt;
use crate::ty::GenKind;
use crate::ty::Ty;
use crate::ty::TyEndoTy;
use crate::ty::UpperBoundKind;
use crate::ty::Utility;
use crate::ty_ancestors::StructuralMismatch;
use crate::ty_ancestors::TyEndoBase;
use crate::ty_ancestors::TyIter2Base;
use crate::ty_symbol::Provenance;
use crate::ty_symbol::Symbol;

pub const MAX_SIZE: usize = 10000;

pub fn patch_up_react_symbol(symbol: &Symbol<ALoc>) -> Option<Symbol<ALoc>> {
    if let Some(name) = symbol.sym_name.strip_prefix("React$") {
        return Some(Symbol {
            sym_name: FlowSmolStr::from(format!("React.{name}")),
            ..symbol.clone()
        });
    }

    let is_unimported_react_symbol = !symbol.sym_anonymous
        && matches!(
            &symbol.sym_provenance,
            Provenance::Library(ri) if ri.imported_as.is_none()
        )
        && symbol.sym_def_loc.source().is_some_and(|file_key| {
            std::path::Path::new(file_key.as_str())
                .file_name()
                .is_some_and(|name| name == "react.js")
        });
    if !is_unimported_react_symbol {
        return None;
    }

    let name = symbol.sym_name.as_str();
    if !matches!(
        name,
        "ChildrenArray"
            | "ComponentProps"
            | "ComponentType"
            | "Context"
            | "MixedElement"
            | "ElementConfig"
            | "ElementProps"
            | "ElementRef"
            | "ElementType"
            | "Key"
            | "Node"
            | "Portal"
            | "RefObject"
            | "RefSetter"
            | "PropsOf"
            | "PropOf"
            | "RefOf"
    ) {
        return None;
    }

    Some(Symbol {
        sym_name: FlowSmolStr::from(format!("React.{name}")),
        ..symbol.clone()
    })
}

struct PatchUpReactTypes;

impl TyEndoBase<(), ALoc> for PatchUpReactTypes {}

impl TyEndoTy<ALoc, ()> for PatchUpReactTypes {
    fn on_t(&mut self, env: &(), t: ALocTy) -> ALocTy {
        match t.as_ref() {
            Ty::Generic(box (symbol, kind, args)) => match patch_up_react_symbol(symbol) {
                Some(symbol) => Arc::new(Ty::Generic(Box::new((
                    symbol,
                    *kind,
                    args.as_ref()
                        .map(|args| args.iter().map(|arg| self.on_t(env, arg.dupe())).collect()),
                )))),
                None => self.default_on_t(env, t),
            },
            Ty::Utility(Utility::ReactElementConfigType(type_)) => {
                Arc::new(Ty::Generic(Box::new((
                    Symbol {
                        sym_provenance: Provenance::Builtin,
                        sym_def_loc: ALoc::none(),
                        sym_name: FlowSmolStr::new("React.ElementConfig"),
                        sym_anonymous: false,
                    },
                    GenKind::TypeAliasKind,
                    Some(vec![self.on_t(env, type_.dupe())].into()),
                ))))
            }
            _ => self.default_on_t(env, t),
        }
    }
}

pub fn patch_up_react_types(t: ALocTy) -> ALocTy {
    PatchUpReactTypes.on_t(&(), t)
}

pub mod size {
    use dupe::Dupe;

    use crate::ty::Elt;
    use crate::ty::Ty;
    use crate::ty::TyIterTy;
    use crate::ty_ancestors::TyIterBase;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BoundedInt {
        Exactly(usize),
        GreaterThan(usize),
    }

    impl BoundedInt {
        pub fn debug_to_string(&self) -> String {
            match self {
                BoundedInt::Exactly(x) => format!("{}", x),
                BoundedInt::GreaterThan(x) => format!("(Greater than {})", x),
            }
        }
    }

    pub struct SizeEnv {
        pub max: usize,
    }

    pub struct SizeCounter {
        pub size: usize,
        pub cutoff: bool,
    }

    impl SizeCounter {
        pub fn new() -> Self {
            SizeCounter {
                size: 0,
                cutoff: false,
            }
        }
    }

    impl Default for SizeCounter {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<L> TyIterBase<SizeEnv, L> for SizeCounter {}

    impl<L: Dupe + PartialEq> TyIterTy<L, SizeEnv> for SizeCounter {
        fn on_t(&mut self, env: &SizeEnv, t: &Ty<L>) {
            if self.cutoff {
                return;
            }
            self.size += 1;
            if self.size > env.max {
                self.cutoff = true;
                return;
            }
            self.default_on_t(env, t);
        }
    }

    pub fn of_type<L: Dupe + PartialEq>(max: usize, t: &Ty<L>) -> BoundedInt {
        let env = SizeEnv { max };
        let mut counter = SizeCounter::new();
        counter.on_t(&env, t);
        if counter.cutoff {
            BoundedInt::GreaterThan(max)
        } else {
            BoundedInt::Exactly(counter.size)
        }
    }

    pub fn of_elt<L: Dupe + PartialEq>(max: usize, elt: &Elt<L>) -> BoundedInt {
        let env = SizeEnv { max };
        let mut counter = SizeCounter::new();
        counter.on_elt(&env, elt);
        if counter.cutoff {
            BoundedInt::GreaterThan(max)
        } else {
            BoundedInt::Exactly(counter.size)
        }
    }
}

/// Returns the number of nodes in a type. Will return None if the number of nodes
/// exceeds the max parameter.
pub fn size_of_type<L: Dupe + PartialEq>(max: Option<usize>, t: &Ty<L>) -> Option<usize> {
    let max = max.unwrap_or(MAX_SIZE);
    match size::of_type(max, t) {
        size::BoundedInt::GreaterThan(_) => None,
        size::BoundedInt::Exactly(s) => Some(s),
    }
}

pub fn size_of_elt<L: Dupe + PartialEq>(max: Option<usize>, elt: &Elt<L>) -> Option<usize> {
    let max = max.unwrap_or(MAX_SIZE);
    match size::of_elt(max, elt) {
        size::BoundedInt::GreaterThan(_) => None,
        size::BoundedInt::Exactly(s) => Some(s),
    }
}

pub mod simplify {
    use dupe::Dupe;

    use super::*;
    use crate::ty::AnyKind;
    use crate::ty::TyIter2Ty;
    use crate::ty_symbol::ImportMode;
    use crate::ty_symbol::ImportedIdent;
    use crate::ty_symbol::RemoteInfo;

    pub struct Config {
        pub is_bot: fn(&ALocTy) -> bool,
        pub is_top: fn(&ALocTy) -> bool,
        pub compare: fn(&ALocTy, &ALocTy) -> i32,
        pub sort: bool,
    }

    pub struct ComparatorBase;

    fn compare_imported_ident<Env>(
        cmp: &mut impl TyIter2Base<Env, ALoc>,
        env: &Env,
        id1: &ImportedIdent<ALoc>,
        id2: &ImportedIdent<ALoc>,
    ) -> Result<(), StructuralMismatch> {
        let ImportedIdent(loc1, name1, mode1) = id1;
        let ImportedIdent(loc2, name2, mode2) = id2;
        cmp.on_aloc(env, loc1, loc2)?;
        cmp.on_string(env, name1, name2)?;
        StructuralMismatch::assert0(match (mode1, mode2) {
            (ImportMode::ValueMode, ImportMode::ValueMode)
            | (ImportMode::TypeMode, ImportMode::TypeMode)
            | (ImportMode::TypeofMode, ImportMode::TypeofMode) => 0,
            (ImportMode::ValueMode, _) => -1,
            (_, ImportMode::ValueMode) => 1,
            (ImportMode::TypeMode, _) => -1,
            (_, ImportMode::TypeMode) => 1,
        })
    }

    fn compare_remote_info<Env>(
        cmp: &mut impl TyIter2Base<Env, ALoc>,
        env: &Env,
        ri1: &RemoteInfo<ALoc>,
        ri2: &RemoteInfo<ALoc>,
    ) -> Result<(), StructuralMismatch> {
        match (&ri1.imported_as, &ri2.imported_as) {
            (None, None) => Ok(()),
            (Some(id1), Some(id2)) => compare_imported_ident(cmp, env, id1, id2),
            (None, Some(_)) => Err(StructuralMismatch::Difference(-1)),
            (Some(_), None) => Err(StructuralMismatch::Difference(1)),
        }
    }

    fn compare_provenance<Env>(
        cmp: &mut impl TyIter2Base<Env, ALoc>,
        env: &Env,
        p1: &Provenance<ALoc>,
        p2: &Provenance<ALoc>,
    ) -> Result<(), StructuralMismatch> {
        match (p1, p2) {
            (Provenance::Local, Provenance::Local) | (Provenance::Builtin, Provenance::Builtin) => {
                Ok(())
            }
            (Provenance::Remote(ri1), Provenance::Remote(ri2))
            | (Provenance::Library(ri1), Provenance::Library(ri2)) => {
                compare_remote_info(cmp, env, ri1, ri2)
            }
            (Provenance::Local, _) => Err(StructuralMismatch::Difference(-1)),
            (_, Provenance::Local) => Err(StructuralMismatch::Difference(1)),
            (Provenance::Remote(_), _) => Err(StructuralMismatch::Difference(-1)),
            (_, Provenance::Remote(_)) => Err(StructuralMismatch::Difference(1)),
            (Provenance::Library(_), _) => Err(StructuralMismatch::Difference(-1)),
            (_, Provenance::Library(_)) => Err(StructuralMismatch::Difference(1)),
        }
    }

    /* comparator_base inherits from comparator_ty in OCaml and only overrides
     * on_aloc (uses quick_compare) and on_symbol (adds Library provenance check).
     * Everything else uses trait defaults. */
    impl<Env> TyIter2Base<Env, ALoc> for ComparatorBase {
        fn on_aloc(
            &mut self,
            _env: &Env,
            loc1: &ALoc,
            loc2: &ALoc,
        ) -> Result<(), StructuralMismatch> {
            /* This comparator uses [ALoc.quick_compare] instead of the default
             * [ALoc.compare]. The latter may throw "Unable to compare a keyed
             * location with a concrete one" exceptions, which, despite being rare,
             * are typically hard to address. The tradeoff here is that we're giving
             * up some completeness in the case where a concrete location is compared
             * against an abstract one and the two locations correspond to the same
             * source location. This case is exercised in tests/type_at_pos_comp_concr_loc_to_aloc.
             * This test causes a type to be compared with itself through a cyclic
             * dependency.
             *
             * NOTE: completeness can be recovered if the caller to the comparator
             * transform all locations to concrete (or abstract) before running the
             * comparison.
             */
            use std::cmp::Ordering;
            let n = loc1.quick_compare(loc2);
            if n == Ordering::Equal {
                Ok(())
            } else {
                let diff = match n {
                    Ordering::Less => -1,
                    Ordering::Greater => 1,
                    Ordering::Equal => 0,
                };
                Err(StructuralMismatch::Difference(diff))
            }
        }

        fn on_symbol(
            &mut self,
            env: &Env,
            name1: &Symbol<ALoc>,
            name2: &Symbol<ALoc>,
        ) -> Result<(), StructuralMismatch> {
            /* It's possible for two symbols to have the same provenance and name but different locations
            due to certain hardcoded fixes, e.g. FbtElement -> Fbt. We should consider these
            to be the same symbol regardless. */
            if name1.sym_name == name2.sym_name
                && name1.sym_def_loc.source() == name2.sym_def_loc.source()
            {
                match (&name1.sym_provenance, &name2.sym_provenance) {
                    (Provenance::Library(l1), Provenance::Library(l2)) if l1 == l2 => Ok(()),
                    _ => {
                        compare_provenance(
                            self,
                            env,
                            &name1.sym_provenance,
                            &name2.sym_provenance,
                        )?;
                        self.on_aloc(env, &name1.sym_def_loc, &name2.sym_def_loc)?;
                        self.on_string(env, &name1.sym_name, &name2.sym_name)?;
                        self.on_bool(env, name1.sym_anonymous, name2.sym_anonymous)
                    }
                }
            } else {
                compare_provenance(self, env, &name1.sym_provenance, &name2.sym_provenance)?;
                self.on_aloc(env, &name1.sym_def_loc, &name2.sym_def_loc)?;
                self.on_string(env, &name1.sym_name, &name2.sym_name)?;
                self.on_bool(env, name1.sym_anonymous, name2.sym_anonymous)
            }
        }
    }

    /* All fail_* and on_obj_t use trait defaults — inherits from comparator_ty */
    impl TyIter2Ty<ALoc, ()> for ComparatorBase {
        fn on_bound_locs(
            &mut self,
            env: &(),
            l1: &ALoc,
            l2: &ALoc,
        ) -> Result<(), StructuralMismatch> {
            self.on_aloc(env, l1, l2)
        }
    }

    pub struct ComparatorMerge;

    /* ComparatorMerge inherits from comparator_base in OCaml. It only overrides
     * on_aloc and on_symbol (delegating to ComparatorBase). */
    impl<Env> TyIter2Base<Env, ALoc> for ComparatorMerge {
        fn on_aloc(
            &mut self,
            env: &Env,
            loc1: &ALoc,
            loc2: &ALoc,
        ) -> Result<(), StructuralMismatch> {
            ComparatorBase.on_aloc(env, loc1, loc2)
        }

        fn on_symbol(
            &mut self,
            env: &Env,
            name1: &Symbol<ALoc>,
            name2: &Symbol<ALoc>,
        ) -> Result<(), StructuralMismatch> {
            ComparatorBase.on_symbol(env, name1, name2)
        }
    }

    /* ComparatorMerge inherits from comparator_base which inherits from comparator_ty.
     * It only overrides on_bot_kind and on_any_kind — all bot/any kinds are equivalent. */
    impl TyIter2Ty<ALoc, ()> for ComparatorMerge {
        // All Bot kinds are equivalent
        fn on_bot_kind(
            &mut self,
            _env: &(),
            _bk1: &BotKind<ALoc>,
            _bk2: &BotKind<ALoc>,
        ) -> Result<(), StructuralMismatch> {
            Ok(())
        }

        fn on_any_kind(
            &mut self,
            _env: &(),
            _ak1: &AnyKind<ALoc>,
            _ak2: &AnyKind<ALoc>,
        ) -> Result<(), StructuralMismatch> {
            Ok(())
        }

        fn on_bound_locs(
            &mut self,
            env: &(),
            l1: &ALoc,
            l2: &ALoc,
        ) -> Result<(), StructuralMismatch> {
            self.on_aloc(env, l1, l2)
        }
    }

    /* When merge_kinds is set to true then all kinds of Any (resp. Bot) types
     * are considered equivalent when comparing types. Specifically for the Bot type
     * we implement a predicate 'is_bot' that determines when the type should be
     * considered the empty element. */
    pub fn mk_config(merge_kinds: bool, sort: bool) -> Config {
        fn is_top(t: &ALocTy) -> bool {
            matches!(t.as_ref(), Ty::Top)
        }

        fn is_bot_merge(t: &ALocTy) -> bool {
            matches!(t.as_ref(), Ty::Bot(_))
        }

        fn is_bot_no_merge(t: &ALocTy) -> bool {
            fn is_bot_upper_kind(kind: &UpperBoundKind<ALoc>) -> bool {
                matches!(kind, UpperBoundKind::NoUpper)
            }

            fn is_bot_kind(kind: &BotKind<ALoc>) -> bool {
                match kind {
                    BotKind::EmptyType => true,
                    BotKind::NoLowerWithUpper(upper) => is_bot_upper_kind(upper),
                }
            }

            match t.as_ref() {
                Ty::Bot(kind) => is_bot_kind(kind),
                _ => false,
            }
        }

        let is_bot: fn(&ALocTy) -> bool = if merge_kinds {
            is_bot_merge
        } else {
            is_bot_no_merge
        };

        fn compare_impl_base(t1: &ALocTy, t2: &ALocTy) -> i32 {
            let mut comparator = ComparatorBase;
            match comparator.on_t(&(), t1, t2) {
                Ok(()) => 0,
                Err(StructuralMismatch::Difference(n)) => n,
                Err(StructuralMismatch::Mismatch) => {
                    panic!("unexpected StructuralMismatch::Mismatch")
                }
            }
        }

        fn compare_impl_merge(t1: &ALocTy, t2: &ALocTy) -> i32 {
            let mut comparator = ComparatorMerge;
            match comparator.on_t(&(), t1, t2) {
                Ok(()) => 0,
                Err(StructuralMismatch::Difference(n)) => n,
                Err(StructuralMismatch::Mismatch) => {
                    panic!("unexpected StructuralMismatch::Mismatch")
                }
            }
        }

        let compare: fn(&ALocTy, &ALocTy) -> i32 = if merge_kinds {
            compare_impl_merge
        } else {
            compare_impl_base
        };

        Config {
            is_top,
            is_bot,
            compare,
            sort,
        }
    }

    /* Simplify union/intersection types, by
     * A. removing equal nodes from union and intersection types, and
     * B. removing the neutral element for union (resp. intersection) types,
     *    which is the bottom (resp. top) type.
     *
     *  WARNING: This visitor will do a deep type traversal.
     */

    fn flatten_union<L: Clone>(types: &[Arc<Ty<L>>]) -> Vec<Arc<Ty<L>>> {
        let mut result = Vec::new();
        for t in types {
            match t.as_ref() {
                Ty::Union(_, t1, t2, rest) => {
                    let mut nested = vec![t1.clone(), t2.clone()];
                    nested.extend(rest.iter().cloned());
                    result.extend(flatten_union(&nested));
                }
                _ => result.push(t.clone()),
            }
        }
        result
    }

    fn flatten_inter<L: Clone>(types: &[Arc<Ty<L>>]) -> Vec<Arc<Ty<L>>> {
        let mut result = Vec::new();
        for t in types {
            match t.as_ref() {
                Ty::Inter(t1, t2, rest) => {
                    let mut nested = vec![t1.clone(), t2.clone()];
                    nested.extend(rest.iter().cloned());
                    result.extend(flatten_inter(&nested));
                }
                _ => result.push(t.clone()),
            }
        }
        result
    }

    fn simplify_list<L, F1, F2>(
        is_zero: &F1,
        is_one: &F2,
        acc: Vec<Arc<Ty<L>>>,
        list: &[Arc<Ty<L>>],
    ) -> Vec<Arc<Ty<L>>>
    where
        F1: Fn(&Ty<L>) -> bool,
        F2: Fn(&Ty<L>) -> bool,
    {
        let mut kept = Vec::new();
        for t in list {
            if is_zero(t.as_ref()) {
                return vec![t.dupe()];
            } else if !is_one(t.as_ref()) {
                kept.push(t.dupe());
            }
        }
        kept.reverse();
        kept.extend(acc);
        kept
    }

    fn simplify_nel<L, F1, F2>(is_zero: &F1, is_one: &F2, list: &[Arc<Ty<L>>]) -> Vec<Arc<Ty<L>>>
    where
        F1: Fn(&Ty<L>) -> bool,
        F2: Fn(&Ty<L>) -> bool,
    {
        let simplified = simplify_list(is_zero, is_one, Vec::new(), list);
        if simplified.is_empty() {
            vec![list[0].dupe()]
        } else {
            simplified
        }
    }

    pub fn run_type(merge_kinds: bool, sort: bool, t: ALocTy) -> ALocTy {
        let config = mk_config(merge_kinds, sort);
        let mut simplifier = Simplifier { config };
        simplifier.on_t(&(), t)
    }

    pub fn run_decl(merge_kinds: bool, sort: bool, d: Decl<ALoc>) -> Decl<ALoc> {
        let config = mk_config(merge_kinds, sort);
        let mut simplifier = Simplifier { config };
        simplifier.on_decl(&(), d)
    }

    pub fn run_elt(merge_kinds: bool, sort: bool, e: Elt<ALoc>) -> Elt<ALoc> {
        let config = mk_config(merge_kinds, sort);
        let mut simplifier = Simplifier { config };
        simplifier.on_elt(&(), e)
    }

    struct Simplifier {
        config: Config,
    }

    impl TyEndoBase<(), ALoc> for Simplifier {}

    impl TyEndoTy<ALoc, ()> for Simplifier {
        fn on_t(&mut self, env: &(), t: Arc<Ty<ALoc>>) -> Arc<Ty<ALoc>> {
            match t.as_ref() {
                Ty::Union(from_annotation, t1, t2, rest) => {
                    self.simplify_union(env, *from_annotation, t1.dupe(), t2.dupe(), rest)
                }
                Ty::Inter(t1, t2, rest) => self.simplify_inter(env, t1.dupe(), t2.dupe(), rest),
                _ => self.default_on_t(env, t),
            }
        }
    }

    impl Simplifier {
        fn simplify_union(
            &mut self,
            env: &(),
            from_annotation: bool,
            t1: ALocTy,
            t2: ALocTy,
            rest: &[ALocTy],
        ) -> ALocTy {
            let t1_new = self.on_t(env, t1);
            let t2_new = self.on_t(env, t2);
            let rest_new: Vec<ALocTy> = rest.iter().map(|t| self.on_t(env, t.dupe())).collect();

            let mut all_types = vec![t1_new, t2_new];
            all_types.extend(rest_new);
            let len1 = all_types.len();

            let flattened = flatten_union(&all_types);
            let len2 = flattened.len();

            let (ts2, len2) = if len1 != len2 {
                (flattened, len2)
            } else {
                (all_types, len1)
            };

            let is_bot_fn = self.config.is_bot;
            let is_top_fn = self.config.is_top;

            let simplified = simplify_nel(
                &|t: &Ty<ALoc>| is_top_fn(&Arc::new((*t).clone())),
                &|t: &Ty<ALoc>| is_bot_fn(&Arc::new((*t).clone())),
                &ts2,
            );

            let deduplicated = self.deduplicate(&simplified);

            // Use deduplicated result if:
            // 1. sort is true, OR
            // 2. simplify_list + dedup removed elements (length changed)
            let final_types = if self.config.sort || deduplicated.len() != len2 {
                deduplicated
            } else {
                // Revert to ts2 (before simplify+dedup) to preserve original order
                ts2
            };

            match final_types.len() {
                0 => Arc::new(Ty::Bot(BotKind::EmptyType)),
                1 => final_types.into_iter().next().unwrap(),
                _ => {
                    let mut iter = final_types.into_iter();
                    let t1 = iter.next().unwrap();
                    let t2 = iter.next().unwrap();
                    let rest: Vec<ALocTy> = iter.collect();
                    Arc::new(Ty::Union(from_annotation, t1, t2, Arc::from(rest)))
                }
            }
        }

        fn simplify_inter(&mut self, env: &(), t1: ALocTy, t2: ALocTy, rest: &[ALocTy]) -> ALocTy {
            let t1_new = self.on_t(env, t1);
            let t2_new = self.on_t(env, t2);
            let rest_new: Vec<ALocTy> = rest.iter().map(|t| self.on_t(env, t.dupe())).collect();

            let mut all_types = vec![t1_new, t2_new];
            all_types.extend(rest_new);
            let len1 = all_types.len();

            let flattened = flatten_inter(&all_types);
            let len2 = flattened.len();

            let (ts2, len2) = if len1 != len2 {
                (flattened, len2)
            } else {
                (all_types, len1)
            };

            let is_bot_fn = self.config.is_bot;
            let is_top_fn = self.config.is_top;

            let simplified = simplify_nel(
                &|t: &Ty<ALoc>| is_bot_fn(&Arc::new((*t).clone())),
                &|t: &Ty<ALoc>| is_top_fn(&Arc::new((*t).clone())),
                &ts2,
            );

            let deduplicated = self.deduplicate(&simplified);

            // Use deduplicated result if:
            // 1. sort is true, OR
            // 2. simplify_list + dedup removed elements (length changed)
            let final_types = if self.config.sort || deduplicated.len() != len2 {
                deduplicated
            } else {
                // Revert to ts2 (before simplify+dedup) to preserve original order
                ts2
            };

            match final_types.len() {
                0 => Arc::new(Ty::Top),
                1 => final_types.into_iter().next().unwrap(),
                _ => {
                    let mut iter = final_types.into_iter();
                    let t1 = iter.next().unwrap();
                    let t2 = iter.next().unwrap();
                    let rest: Vec<ALocTy> = iter.collect();
                    Arc::new(Ty::Inter(t1, t2, Arc::from(rest)))
                }
            }
        }

        fn deduplicate(&self, types: &[ALocTy]) -> Vec<ALocTy> {
            if types.is_empty() {
                return vec![];
            }

            let compare = self.config.compare;
            let mut result: Vec<ALocTy> = types.to_vec();
            result.sort_by(|t1, t2| compare(t1, t2).cmp(&0));
            let mut deduplicated = Vec::with_capacity(result.len());
            for t in result {
                if let Some(prev) = deduplicated.last()
                    && compare(prev, &t) == 0
                {
                    continue;
                }
                deduplicated.push(t);
            }
            deduplicated
        }
    }
}

pub fn simplify_type(merge_kinds: bool, sort: Option<bool>, t: ALocTy) -> ALocTy {
    let sort = sort.unwrap_or(false);
    simplify::run_type(merge_kinds, sort, t)
}

pub fn simplify_decl(merge_kinds: bool, sort: Option<bool>, d: Decl<ALoc>) -> Decl<ALoc> {
    let sort = sort.unwrap_or(false);
    simplify::run_decl(merge_kinds, sort, d)
}

pub fn simplify_elt(merge_kinds: bool, sort: Option<bool>, e: Elt<ALoc>) -> Elt<ALoc> {
    let sort = sort.unwrap_or(false);
    simplify::run_elt(merge_kinds, sort, e)
}

pub fn unmaybe_ty(t: ALocTy) -> ALocTy {
    match t.as_ref() {
        Ty::Union(a, t1, t2, ts) => {
            let all_types: Vec<Arc<Ty<ALoc>>> = std::iter::once(t1)
                .chain(std::iter::once(t2))
                .chain(ts.iter())
                .filter_map(|ty| match ty.as_ref() {
                    Ty::Null | Ty::Void | Ty::Undefined | Ty::TsVoid => None,
                    _ => Some(unmaybe_ty(ty.dupe())),
                })
                .collect();

            match all_types.len() {
                0 => Arc::new(Ty::Bot(BotKind::EmptyType)),
                1 => all_types.into_iter().next().unwrap(),
                _ => {
                    let mut iter = all_types.into_iter();
                    let t1 = iter.next().unwrap();
                    let t2 = iter.next().unwrap();
                    let ts: Vec<Arc<Ty<ALoc>>> = iter.collect();
                    Arc::new(Ty::Union(a.dupe(), t1, t2, Arc::from(ts)))
                }
            }
        }
        _ => t,
    }
}

/// Utility useful for codemods/type insertion. When the element we infered is a
/// declaration we can't directly print/insert in code. This utility helps convert
/// it to an equivalent type. For example it will convert `class C` to `typeof C`,
/// `enum E` to `typeof E`.
pub fn typify_elt(elt: Elt<ALoc>) -> Option<ALocTy> {
    match elt {
        Elt::Type(ty) => Some(ty),
        Elt::Decl(Decl::ClassDecl(box (s, _)))
        | Elt::Decl(Decl::EnumDecl(box DeclEnumDeclData { name: s, .. }))
        | Elt::Decl(Decl::NominalComponentDecl(box DeclNominalComponentDeclData {
            name: s, ..
        })) => Some(Arc::new(Ty::TypeOf(Box::new((
            BuiltinOrSymbol::TSymbol(s),
            None,
        ))))),
        Elt::Decl(_) => None,
    }
}

pub fn reinterpret_elt_as_type_identifier(elt: Elt<ALoc>) -> Elt<ALoc> {
    match elt {
        Elt::Decl(Decl::NominalComponentDecl(box DeclNominalComponentDeclData {
            name,
            tparams,
            targs,
            props,
            renders,
            ..
        })) => Elt::Decl(Decl::NominalComponentDecl(Box::new(
            DeclNominalComponentDeclData {
                name,
                tparams,
                targs,
                props,
                renders,
                is_type: true,
            },
        ))),
        elt => elt,
    }
}
