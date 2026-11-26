/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::hash::Hash;
use std::ops::Deref;

use dupe::Dupe;
use flow_analysis::bindings::Bindings;
use flow_analysis::bindings::Kind;
use flow_analysis::scope_builder;
use flow_analysis::scope_builder::WithBindings;
use flow_common_tarjan::dependency_components;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::types::TypeInner;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::ast_visitor::object_type_property_default;
use flow_parser::ast_visitor::type_args_default;
use flow_parser::ast_visitor::type_default;
use vec1::Vec1;

pub struct TypeParamAnalysis<Loc> {
    /// Direct valid sibling dependencies, with one map per source parameter.
    pub dependencies: Vec<BTreeMap<usize, Loc>>,
    /// The dependencies a parameter needs resolved before its own: those of its default, and
    /// those of its bound in positions where TypeScript resolves the constraint of the type found
    /// there to check that a constraint is not circular: the bound itself, the members of unions
    /// and intersections, the object and index of an indexed access, the check type and branches
    /// of a conditional type, template literals, the keys of mapped types, and object spreads. The
    /// other references of a bound, e.g. type arguments, object members and `keyof` operands, are
    /// deferred, so a bound may refer to its own parameter through them, as in
    /// `T extends Array<T>` or `T extends Record<keyof T, V>`.
    immediate_dependencies: Vec<BTreeMap<usize, Loc>>,
    /// The dependencies of each bound.
    bound_dependencies: Vec<BTreeSet<usize>>,
}

/// The cycles among the parameters of one list.
pub struct TypeParamCycles<Loc> {
    /// Cycles of immediate dependencies, e.g. `<A extends B, B extends A>`, which are invalid,
    /// each with a reference to its first member.
    pub invalid: Vec<(Vec1<usize>, Loc)>,
    /// The parameters in a cycle through a deferred dependency but in no invalid cycle, e.g. `T`
    /// in `<T extends Array<T>>`. Their bounds refer to themselves, so they are materialized
    /// lazily.
    pub recursive: BTreeSet<usize>,
}

impl<Loc> TypeParamCycles<Loc> {
    /// The parameters of invalid cycles.
    pub fn cyclic(&self) -> BTreeSet<usize> {
        self.invalid
            .iter()
            .flat_map(|(members, _)| members.iter().copied())
            .collect()
    }
}

impl<Loc: Dupe> TypeParamAnalysis<Loc> {
    pub fn cycles(&self) -> TypeParamCycles<Loc> {
        let immediate_dependencies = self
            .immediate_dependencies
            .iter()
            .map(|dependencies| dependencies.keys().copied().collect())
            .collect::<Vec<_>>();
        let invalid = dependency_components(&immediate_dependencies)
            .into_iter()
            .filter(|component| component.cyclic)
            .map(|component| {
                let target = *component.members.first();
                let reference_loc = component
                    .members
                    .iter()
                    .find_map(|source| self.immediate_dependencies[*source].get(&target))
                    .expect("cyclic component should have an edge to its first member")
                    .dupe();
                (component.members, reference_loc)
            })
            .collect::<Vec<_>>();
        let cyclic = invalid
            .iter()
            .flat_map(|(members, _)| members.iter().copied())
            .collect::<BTreeSet<_>>();
        let dependencies = self
            .dependencies
            .iter()
            .map(|dependencies| dependencies.keys().copied().collect())
            .collect::<Vec<_>>();
        let recursive = dependency_components(&dependencies)
            .into_iter()
            .filter(|component| component.cyclic)
            .flat_map(|component| component.members)
            .filter(|index| !cyclic.contains(index))
            .collect();
        TypeParamCycles { invalid, recursive }
    }

    /// Returns dependency-first bound initialization order. The parameters of invalid cycles are
    /// initialized first with error bounds, and dependencies on them and on recursive parameters,
    /// whose bounds are materialized lazily, do not constrain the order.
    pub fn bound_order(&self, cycles: &TypeParamCycles<Loc>) -> Vec<usize> {
        let cyclic = cycles.cyclic();
        let dependencies = self
            .bound_dependencies
            .iter()
            .enumerate()
            .map(|(source_index, dependencies)| {
                if cyclic.contains(&source_index) {
                    BTreeSet::new()
                } else {
                    dependencies
                        .iter()
                        .filter(|dependency| {
                            !cyclic.contains(dependency) && !cycles.recursive.contains(dependency)
                        })
                        .copied()
                        .collect()
                }
            })
            .collect::<Vec<_>>();
        let order = dependency_components(&dependencies);
        assert!(
            order.iter().all(|component| !component.cyclic),
            "excluding cyclic and recursive type parameters should make bound order acyclic"
        );
        cyclic
            .iter()
            .copied()
            .chain(
                order
                    .iter()
                    .rev()
                    .flat_map(|component| component.members.iter().copied())
                    .filter(|index| !cyclic.contains(index)),
            )
            .collect()
    }
}

struct DependencyVisitor<'a, Loc> {
    sibling_indices: &'a BTreeMap<FlowSmolStr, usize>,
    source_index: usize,
    in_default: bool,
    /// Whether a reference at the current position of the bound is an immediate dependency.
    immediate: bool,
    shadowed: Vec<BTreeSet<FlowSmolStr>>,
    dependencies: BTreeMap<usize, Loc>,
    immediate_dependencies: BTreeMap<usize, Loc>,
    bound_dependencies: BTreeSet<usize>,
}

impl<Loc> DependencyVisitor<'_, Loc> {
    fn deferred(&mut self, visit: impl FnOnce(&mut Self) -> Result<(), !>) -> Result<(), !> {
        let immediate = std::mem::replace(&mut self.immediate, false);
        let result = visit(self);
        self.immediate = immediate;
        result
    }
}

impl<Loc: Dupe> WithBindings<Loc, !> for DependencyVisitor<'_, Loc> {
    fn with_bindings<T>(
        &mut self,
        _lexical: bool,
        _loc: Loc,
        bindings: Bindings<Loc>,
        visit: impl FnOnce(&mut Self) -> Result<T, !>,
    ) -> Result<T, !> {
        let shadowed = bindings
            .to_assoc()
            .into_iter()
            .filter_map(|(key, (kind, _))| match kind {
                Kind::Type { .. } | Kind::TypeParam => key.into_name(),
                _ => None,
            })
            .collect();
        self.shadowed.push(shadowed);
        let result = visit(self);
        self.shadowed.pop();
        result
    }
}

impl<'ast, Loc> AstVisitor<'ast, Loc> for DependencyVisitor<'_, Loc>
where
    Loc: Dupe + Eq + Hash + Default + Clone,
{
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn type_identifier_reference(&mut self, id: &'ast ast::Identifier<Loc, Loc>) -> Result<(), !> {
        if self
            .shadowed
            .iter()
            .rev()
            .any(|scope| scope.contains(&id.name))
        {
            return Ok(());
        }
        let Some(index) = self.sibling_indices.get(&id.name).copied() else {
            return Ok(());
        };
        if self.in_default && index >= self.source_index {
            return Ok(());
        }
        self.dependencies
            .entry(index)
            .or_insert_with(|| id.loc.dupe());
        if self.in_default || self.immediate {
            self.immediate_dependencies
                .entry(index)
                .or_insert_with(|| id.loc.dupe());
        }
        if !self.in_default {
            self.bound_dependencies.insert(index);
        }
        Ok(())
    }

    fn type_(&mut self, t: &'ast ast::types::Type<Loc, Loc>) -> Result<(), !> {
        match t.deref() {
            TypeInner::Union { .. }
            | TypeInner::Intersection { .. }
            | TypeInner::Nullable { .. }
            | TypeInner::IndexedAccess { .. }
            | TypeInner::OptionalIndexedAccess { .. }
            | TypeInner::Conditional { .. }
            | TypeInner::TemplateLiteral { .. }
            | TypeInner::Object { .. }
            | TypeInner::Generic { .. } => type_default(self, t),
            _ => self.deferred(|visitor| type_default(visitor, t)),
        }
    }

    fn type_args(&mut self, targs: &'ast ast::types::TypeArgs<Loc, Loc>) -> Result<(), !> {
        self.deferred(|visitor| type_args_default(visitor, targs))
    }

    fn object_type_property(
        &mut self,
        p: &'ast ast::types::object::Property<Loc, Loc>,
    ) -> Result<(), !> {
        match p {
            ast::types::object::Property::SpreadProperty(_)
            | ast::types::object::Property::MappedType(_) => object_type_property_default(self, p),
            _ => self.deferred(|visitor| object_type_property_default(visitor, p)),
        }
    }

    fn function_type(&mut self, ft: &'ast ast::types::Function<Loc, Loc>) -> Result<(), !> {
        scope_builder::function_type(self, true, ft, &|visitor, visit| visit(visitor))
    }

    fn component_type(
        &mut self,
        _loc: &'ast Loc,
        component: &'ast ast::types::Component<Loc, Loc>,
    ) -> Result<(), !> {
        scope_builder::component_type(self, true, component, &|visitor, visit| visit(visitor))
    }

    fn object_mapped_type_property(
        &mut self,
        mapped_type: &'ast ast::types::object::MappedType<Loc, Loc>,
    ) -> Result<(), !> {
        self.type_(&mapped_type.source_type)?;
        self.deferred(|visitor| {
            scope_builder::object_mapped_type_property(
                visitor,
                true,
                mapped_type,
                &|visitor, visit| visit(visitor),
            )
        })
    }

    fn conditional_type(
        &mut self,
        conditional: &'ast ast::types::Conditional<Loc, Loc>,
    ) -> Result<(), !> {
        scope_builder::conditional_type(
            self,
            conditional,
            |visitor, extends_type| visitor.deferred(|visitor| visitor.type_(extends_type)),
            |visitor, loc, tparams, in_tparam_scope| {
                scope_builder::scoped_infer_type_params(
                    visitor,
                    true,
                    loc,
                    tparams,
                    |visitor, id| visitor.binding_type_identifier(id),
                    in_tparam_scope,
                )
            },
        )
    }

    fn infer_type(&mut self, _infer: &'ast ast::types::Infer<Loc, Loc>) -> Result<(), !> {
        Ok(())
    }
}

/// Analyzes sibling references in one syntactic type-parameter list.
pub fn analyze_type_params<Loc>(
    tparams: &ast::types::TypeParams<Loc, Loc>,
) -> TypeParamAnalysis<Loc>
where
    Loc: Dupe + Eq + Hash + Default + Clone,
{
    let sibling_indices = tparams
        .params
        .iter()
        .enumerate()
        .map(|(index, tparam)| (tparam.name.name.dupe(), index))
        .collect();
    let mut dependencies = Vec::with_capacity(tparams.params.len());
    let mut immediate_dependencies = Vec::with_capacity(tparams.params.len());
    let mut bound_dependencies = Vec::with_capacity(tparams.params.len());

    for (source_index, tparam) in tparams.params.iter().enumerate() {
        let mut visitor = DependencyVisitor {
            sibling_indices: &sibling_indices,
            source_index,
            in_default: false,
            immediate: true,
            shadowed: Vec::new(),
            dependencies: BTreeMap::new(),
            immediate_dependencies: BTreeMap::new(),
            bound_dependencies: BTreeSet::new(),
        };
        let Ok(()) = visitor.type_annotation_hint(&tparam.bound);
        if let Some(default) = &tparam.default {
            visitor.in_default = true;
            let Ok(()) = visitor.type_(default);
        }
        dependencies.push(visitor.dependencies);
        immediate_dependencies.push(visitor.immediate_dependencies);
        bound_dependencies.push(visitor.bound_dependencies);
    }

    TypeParamAnalysis {
        dependencies,
        immediate_dependencies,
        bound_dependencies,
    }
}
