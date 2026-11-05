/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::hash::Hash;
use std::sync::Arc;

use dupe::Dupe;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;

use crate::bindings::Bindings;
use crate::bindings::Entry;
use crate::bindings::Kind;
use crate::bindings::VarKey;
use crate::hoister::Hoister;
use crate::hoister::LexicalHoister;
use crate::infer_type_hoister;
use crate::scope_api::Def;
use crate::scope_api::Scope;
use crate::scope_api::ScopeId;
use crate::scope_api::ScopeInfo;

/// Extracts the top-level annotation from a pattern and returns both the annotation
/// and a new pattern with the annotation replaced by Missing.
/// Returns (annotation, new_pattern).
pub fn pattern_with_toplevel_annot_removed<Loc: Default + Dupe>(
    pattern: &ast::pattern::Pattern<Loc, Loc>,
) -> (
    ast::types::AnnotationOrHint<Loc, Loc>,
    ast::pattern::Pattern<Loc, Loc>,
) {
    match pattern {
        ast::pattern::Pattern::Object { loc, inner } => {
            let annot = inner.annot.clone();
            let new_inner = ast::pattern::Object {
                properties: inner.properties.dupe(),
                annot: ast::types::AnnotationOrHint::Missing(Loc::default()),
                optional: inner.optional,
                comments: inner.comments.dupe(),
            };
            (
                annot,
                ast::pattern::Pattern::Object {
                    loc: loc.dupe(),
                    inner: Arc::new(new_inner),
                },
            )
        }
        ast::pattern::Pattern::Array { loc, inner } => {
            let annot = inner.annot.clone();
            let new_inner = ast::pattern::Array {
                elements: inner.elements.dupe(),
                annot: ast::types::AnnotationOrHint::Missing(Loc::default()),
                optional: inner.optional,
                comments: inner.comments.dupe(),
            };
            (
                annot,
                ast::pattern::Pattern::Array {
                    loc: loc.clone(),
                    inner: Arc::new(new_inner),
                },
            )
        }
        ast::pattern::Pattern::Identifier { loc, inner } => {
            let annot = inner.annot.clone();
            let new_inner = ast::pattern::Identifier {
                name: inner.name.dupe(),
                annot: ast::types::AnnotationOrHint::Missing(Loc::default()),
                optional: inner.optional,
            };
            (
                annot,
                ast::pattern::Pattern::Identifier {
                    loc: loc.clone(),
                    inner: Arc::new(new_inner),
                },
            )
        }
        ast::pattern::Pattern::Expression { loc: _, inner: _ } => (
            ast::types::AnnotationOrHint::Missing(Loc::default()),
            pattern.clone(),
        ),
    }
}

fn pattern_toplevel_annot<Loc: Default + Dupe>(
    pattern: &ast::pattern::Pattern<Loc, Loc>,
) -> ast::types::AnnotationOrHint<Loc, Loc> {
    match pattern {
        ast::pattern::Pattern::Object { inner, .. } => inner.annot.clone(),
        ast::pattern::Pattern::Array { inner, .. } => inner.annot.clone(),
        ast::pattern::Pattern::Identifier { inner, .. } => inner.annot.clone(),
        ast::pattern::Pattern::Expression { .. } => {
            ast::types::AnnotationOrHint::Missing(Loc::default())
        }
    }
}

struct RemoveDefaultVisitor;

impl<Loc: Dupe + PartialEq> AstVisitor<'_, Loc> for RemoveDefaultVisitor {
    fn normalize_loc(loc: &Loc) -> &Loc {
        loc
    }

    fn normalize_type(type_: &Loc) -> &Loc {
        type_
    }

    fn map_default_opt(
        &mut self,
        _default: Option<&ast::expression::Expression<Loc, Loc>>,
    ) -> Option<ast::expression::Expression<Loc, Loc>> {
        None
    }

    fn map_function_param(
        &mut self,
        param: &ast::function::Param<Loc, Loc>,
    ) -> ast::function::Param<Loc, Loc> {
        match param {
            ast::function::Param::RegularParam {
                loc,
                argument,
                default,
            } => {
                let argument_ = self.map_pattern(None, argument);
                let default_ = self.map_default_opt(default.as_ref());
                ast::function::Param::RegularParam {
                    loc: loc.dupe(),
                    argument: argument_,
                    default: default_,
                }
            }
            ast::function::Param::ParamProperty { loc, property } => {
                let property_ = self.map_class_property(property);
                ast::function::Param::ParamProperty {
                    loc: loc.dupe(),
                    property: property_,
                }
            }
        }
    }

    fn map_pattern_object_property(
        &mut self,
        kind: Option<ast::VariableKind>,
        prop: &ast::pattern::object::Property<Loc, Loc>,
    ) -> ast::pattern::object::Property<Loc, Loc> {
        match prop {
            ast::pattern::object::Property::NormalProperty(p) => {
                let ast::pattern::object::NormalProperty {
                    loc,
                    key,
                    pattern,
                    default,
                    shorthand,
                } = p;
                let key_ = self.map_pattern_object_property_key(kind, key);
                let pattern_ = self.map_pattern_object_property_pattern(kind, pattern);
                let default_ = self.map_default_opt(default.as_ref());
                let shorthand_ = *shorthand && {
                    match (&key_, &pattern_) {
                        (
                            ast::pattern::object::Key::Identifier(key_id),
                            ast::pattern::Pattern::Identifier { loc: _, inner },
                        ) => {
                            let key_name = &key_id.name;
                            let value_name = &inner.name.name;
                            key_name == value_name
                        }
                        _ => false,
                    }
                };
                ast::pattern::object::Property::NormalProperty(
                    ast::pattern::object::NormalProperty {
                        loc: loc.dupe(),
                        key: key_,
                        pattern: pattern_,
                        default: default_,
                        shorthand: shorthand_,
                    },
                )
            }
            ast::pattern::object::Property::RestElement(r) => {
                let r_ = self.map_pattern_object_rest_property(kind, r);
                ast::pattern::object::Property::RestElement(r_)
            }
        }
    }

    fn map_pattern_array_element(
        &mut self,
        kind: Option<ast::VariableKind>,
        elem: &ast::pattern::array::Element<Loc, Loc>,
    ) -> ast::pattern::array::Element<Loc, Loc> {
        match elem {
            ast::pattern::array::Element::Hole(loc) => {
                ast::pattern::array::Element::Hole(loc.dupe())
            }
            ast::pattern::array::Element::NormalElement(e) => {
                let ast::pattern::array::NormalElement {
                    loc,
                    argument,
                    default,
                } = e;
                let argument_ = self.map_pattern_array_element_pattern(kind, argument);
                let default_ = self.map_default_opt(default.as_ref());
                ast::pattern::array::Element::NormalElement(ast::pattern::array::NormalElement {
                    loc: loc.dupe(),
                    argument: argument_,
                    default: default_,
                })
            }
            ast::pattern::array::Element::RestElement(r) => {
                let r_ = self.map_pattern_array_rest_element(kind, r);
                ast::pattern::array::Element::RestElement(r_)
            }
        }
    }
}

// Returns (has_defaults, params_without_defaults).
fn remove_params_default<Loc: Dupe + PartialEq>(
    params: &ast::function::Params<Loc, Loc>,
) -> (bool, ast::function::Params<Loc, Loc>) {
    let mut visitor = RemoveDefaultVisitor;
    let params_ = visitor.map_function_params(params);
    let has_default = params_ != *params;
    (has_default, params_)
}

pub fn for_in_statement<Loc: Dupe, E, V: WithBindings<Loc, E>>(
    visitor: &mut V,
    enable_enums: bool,
    loc: &Loc,
    stmt: &ast::statement::ForIn<Loc, Loc>,
    scoped: impl FnOnce(&mut V, &Loc, &ast::statement::ForIn<Loc, Loc>) -> Result<(), E>,
) -> Result<(), E> {
    let ast::statement::ForIn {
        left,
        right: _,
        body: _,
        comments: _,
    } = stmt;
    let mut lexical_hoist = LexicalHoister::new(enable_enums);
    let lexical_bindings = match left {
        ast::statement::for_in::Left::LeftDeclaration((decl_loc, decl)) => {
            let Ok(()) = lexical_hoist.variable_declaration(decl_loc, decl);
            lexical_hoist.into_bindings()
        }
        ast::statement::for_in::Left::LeftPattern(_) => Bindings::empty(),
    };
    visitor.with_bindings(true, loc.dupe(), lexical_bindings, |this| {
        scoped(this, loc, stmt)
    })
}

pub fn for_of_statement<Loc: Dupe, E, V: WithBindings<Loc, E>>(
    visitor: &mut V,
    enable_enums: bool,
    loc: &Loc,
    stmt: &ast::statement::ForOf<Loc, Loc>,
    scoped: impl FnOnce(&mut V, &Loc, &ast::statement::ForOf<Loc, Loc>) -> Result<(), E>,
) -> Result<(), E> {
    let ast::statement::ForOf {
        left,
        right: _,
        body: _,
        await_: _,
        comments: _,
    } = stmt;
    let mut lexical_hoist = LexicalHoister::new(enable_enums);
    let lexical_bindings = match left {
        ast::statement::for_of::Left::LeftDeclaration((decl_loc, decl)) => {
            let Ok(()) = lexical_hoist.variable_declaration(decl_loc, decl);
            lexical_hoist.into_bindings()
        }
        ast::statement::for_of::Left::LeftPattern(_) => Bindings::empty(),
    };
    visitor.with_bindings(true, loc.dupe(), lexical_bindings, |this| {
        scoped(this, loc, stmt)
    })
}

pub fn for_statement<Loc: Dupe, E, V: WithBindings<Loc, E>>(
    visitor: &mut V,
    enable_enums: bool,
    loc: &Loc,
    stmt: &ast::statement::For<Loc, Loc>,
    scoped: impl FnOnce(&mut V, &Loc, &ast::statement::For<Loc, Loc>) -> Result<(), E>,
) -> Result<(), E> {
    let ast::statement::For {
        init,
        test: _,
        update: _,
        body: _,
        comments: _,
    } = stmt;
    let mut lexical_hoist = LexicalHoister::new(enable_enums);
    let lexical_bindings = match init {
        Some(ast::statement::for_::Init::InitDeclaration((decl_loc, decl))) => {
            let Ok(()) = lexical_hoist.variable_declaration(decl_loc, decl);
            lexical_hoist.into_bindings()
        }
        _ => Bindings::empty(),
    };
    visitor.with_bindings(true, loc.dupe(), lexical_bindings, |this| {
        scoped(this, loc, stmt)
    })
}

pub fn catch_clause<Loc: Dupe, E, V: WithBindings<Loc, E>>(
    visitor: &mut V,
    enable_enums: bool,
    clause: &ast::statement::try_::CatchClause<Loc, Loc>,
    scoped: impl FnOnce(&mut V, &ast::statement::try_::CatchClause<Loc, Loc>) -> Result<(), E>,
) -> Result<(), E> {
    let ast::statement::try_::CatchClause {
        loc,
        param,
        body: _,
        comments: _,
    } = clause;
    let mut lexical_hoist = LexicalHoister::new(enable_enums);
    let lexical_bindings = match param {
        Some(p) => {
            let Ok(()) = lexical_hoist.catch_clause_pattern(p);
            lexical_hoist.into_bindings()
        }
        None => Bindings::empty(),
    };
    visitor.with_bindings(true, loc.dupe(), lexical_bindings, |this| {
        scoped(this, clause)
    })
}

pub fn scoped_infer_type_params<
    Loc: Dupe,
    E,
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
>(
    visitor: &mut V,
    with_types: bool,
    loc: &Loc,
    tps: &[ast::types::TypeParam<Loc, Loc>],
    binding_infer_type_identifier: impl Fn(&mut V, &ast::Identifier<Loc, Loc>) -> Result<(), E>,
    in_tparam_scope: impl Fn(&mut V) -> Result<(), E>,
) -> Result<(), E> {
    if !with_types {
        return in_tparam_scope(visitor);
    }

    // Unlike other tparams, infer types' tparams don't have an obvious order,
    // so bounds in infer cannot refer to other infer types in the same hoisted list.
    for tp in tps {
        let ast::types::TypeParam {
            loc: _,
            name: _,
            bound,
            bound_kind: _,
            variance: _,
            default,
            const_: _,
        } = tp;
        visitor.type_annotation_hint(bound)?;
        if let Some(t) = default {
            visitor.type_(t)?;
        }
    }

    let mut bindings = Bindings::empty();
    for tp in tps {
        bindings.add(Entry {
            loc: tp.name.loc.dupe(),
            key: VarKey::Name(tp.name.name.dupe()),
            kind: Kind::Type {
                imported: false,
                type_only_namespace: false,
            },
        });
    }

    visitor.with_bindings(false, loc.dupe(), bindings, |s| {
        for tp in tps {
            binding_infer_type_identifier(s, &tp.name)?;
        }
        in_tparam_scope(s)
    })
}

pub fn scoped_type_params<Loc: Dupe + Eq + Hash + Default, E, V>(
    visitor: &mut V,
    with_types: bool,
    tparams: Option<&ast::types::TypeParams<Loc, Loc>>,
    hoist_annotation: bool,
    hoist_annotations: &dyn Fn(&mut V, &mut dyn FnMut(&mut V) -> Result<(), E>) -> Result<(), E>,
    in_tparam_scope: &dyn Fn(&mut V) -> Result<(), E>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
{
    if !with_types {
        return in_tparam_scope(visitor);
    }

    let Some(tparams) = tparams else {
        return in_tparam_scope(visitor);
    };

    let mut bindings = Bindings::empty();
    for tparam in tparams.params.iter() {
        bindings.add(Entry {
            loc: tparam.name.loc.dupe(),
            key: VarKey::Name(tparam.name.name.dupe()),
            kind: Kind::TypeParam,
        });
    }

    visitor.with_type_params(tparams, |visitor| {
        visitor.with_bindings(false, tparams.loc.dupe(), bindings, |visitor| {
            for tparam in tparams.params.iter() {
                if let Some(default) = &tparam.default {
                    visitor.with_type_param_default(|visitor| {
                        if hoist_annotation {
                            hoist_annotations(visitor, &mut |v| AstVisitor::type_(v, default))
                        } else {
                            AstVisitor::type_(visitor, default)
                        }
                    })?;
                }
                visitor.binding_type_identifier(&tparam.name)?;
            }

            for tparam in tparams.params.iter() {
                if hoist_annotation {
                    hoist_annotations(visitor, &mut |v| v.type_annotation_hint(&tparam.bound))?;
                } else {
                    visitor.type_annotation_hint(&tparam.bound)?;
                }
            }

            in_tparam_scope(visitor)
        })
    })
}

pub fn match_case<
    'ast,
    Loc: Dupe,
    E,
    V: AstVisitor<'ast, Loc, Loc, &'ast Loc, E> + WithBindings<Loc, E>,
    B,
>(
    visitor: &mut V,
    enable_enums: bool,
    case: &'ast ast::match_::Case<Loc, Loc, B>,
    on_case_body: &mut impl FnMut(&mut V, &'ast B) -> Result<(), E>,
) -> Result<(), E> {
    let loc = case.loc.dupe();
    let pattern = &case.pattern;

    let mut lexical_hoist = LexicalHoister::new(enable_enums);
    let Ok(()) = lexical_hoist.match_pattern(pattern);
    let bindings = lexical_hoist.into_bindings();
    visitor.with_bindings(true, loc, bindings, |this| {
        ast_visitor::match_case_default(this, case, on_case_body)
    })
}

pub fn import_named_specifier<Loc: Dupe, E, V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E>>(
    visitor: &mut V,
    with_types: bool,
    import_kind: ast::statement::ImportKind,
    spec: &ast::statement::import_declaration::NamedSpecifier<Loc, Loc>,
) -> Result<(), E> {
    match spec {
        ast::statement::import_declaration::NamedSpecifier {
            local: Some(ident),
            remote: _,
            remote_name_def_loc: _,
            kind,
            kind_loc: _,
        }
        | ast::statement::import_declaration::NamedSpecifier {
            local: None,
            remote: ident,
            remote_name_def_loc: _,
            kind,
            kind_loc: _,
        } => {
            use ast::statement::ImportKind;
            // when `with_types` is false, only visit values, not types. `import_declaration`
            // avoids visiting specifiers for `import type` and `import typeof`, so
            // `kind = None` must mean a value here.
            match (import_kind, kind) {
                (ImportKind::ImportType | ImportKind::ImportTypeof, _)
                | (_, Some(ImportKind::ImportType | ImportKind::ImportTypeof)) => {
                    if with_types {
                        visitor.binding_type_identifier(ident)?;
                    }
                }
                _ => {
                    visitor.pattern_identifier(Some(ast::VariableKind::Const), ident)?;
                }
            }
        }
    }
    Ok(())
}

pub fn type_alias<Loc: Dupe + Eq + Hash + Default, E, V>(
    visitor: &mut V,
    with_types: bool,
    alias: &ast::statement::TypeAlias<Loc, Loc>,
    hoist_annotations: &dyn Fn(&mut V, &mut dyn FnMut(&mut V) -> Result<(), E>) -> Result<(), E>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
{
    if !with_types {
        return Ok(());
    }
    visitor.binding_type_identifier(&alias.id)?;
    scoped_type_params(
        visitor,
        with_types,
        alias.tparams.as_ref(),
        false,
        hoist_annotations,
        &|this| this.type_(&alias.right),
    )
}

pub fn opaque_type<Loc: Dupe + Eq + Hash + Default, E, V>(
    visitor: &mut V,
    with_types: bool,
    opaque: &ast::statement::OpaqueType<Loc, Loc>,
    hoist_annotations: &dyn Fn(&mut V, &mut dyn FnMut(&mut V) -> Result<(), E>) -> Result<(), E>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
{
    if !with_types {
        return Ok(());
    }
    visitor.binding_type_identifier(&opaque.id)?;
    scoped_type_params(
        visitor,
        with_types,
        opaque.tparams.as_ref(),
        false,
        hoist_annotations,
        &|this| {
            if let Some(ref t) = opaque.impl_type {
                this.type_(t)?;
            }
            if let Some(ref t) = opaque.lower_bound {
                this.type_(t)?;
            }
            if let Some(ref t) = opaque.upper_bound {
                this.type_(t)?;
            }
            if let Some(ref t) = opaque.legacy_upper_bound {
                this.type_(t)?;
            }
            Ok(())
        },
    )
}

pub fn interface<Loc: Dupe + Eq + Hash + Default, E, V>(
    visitor: &mut V,
    with_types: bool,
    interface_decl: &ast::statement::Interface<Loc, Loc>,
    hoist_annotations: &dyn Fn(&mut V, &mut dyn FnMut(&mut V) -> Result<(), E>) -> Result<(), E>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
{
    if !with_types {
        return Ok(());
    }
    visitor.binding_type_identifier(&interface_decl.id)?;
    let mut extends_targs = Vec::new();
    for (_loc, generic) in interface_decl.extends.iter() {
        visitor.generic_identifier_type(&generic.id)?;
        if let Some(targs) = generic.targs.as_ref() {
            extends_targs.push(targs);
        }
    }
    scoped_type_params(
        visitor,
        with_types,
        interface_decl.tparams.as_ref(),
        false,
        hoist_annotations,
        &|this| {
            for targs in &extends_targs {
                this.type_args(targs)?;
            }
            this.object_type(&interface_decl.body.1)
        },
    )
}

pub fn function_type<Loc: Dupe + Eq + Hash + Default, E, V>(
    visitor: &mut V,
    with_types: bool,
    ft: &ast::types::Function<Loc, Loc>,
    hoist_annotations: &dyn Fn(&mut V, &mut dyn FnMut(&mut V) -> Result<(), E>) -> Result<(), E>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
{
    let ast::types::Function {
        params,
        return_,
        tparams,
        effect: _,
        comments: _,
    } = ft;
    let in_tparam_scope = |this: &mut V| {
        if let Some(this_param) = &params.this {
            this.function_this_param_type(this_param)?;
        }
        for param in params.params.iter() {
            this.function_param_type(param)?;
        }
        if let Some(rest) = &params.rest {
            this.function_rest_param_type(rest)?;
        }
        this.function_type_return_annotation(return_)
    };
    scoped_type_params(
        visitor,
        with_types,
        tparams.as_ref(),
        false,
        hoist_annotations,
        &in_tparam_scope,
    )
}

pub fn component_type<Loc: Dupe + Eq + Hash + Default, E, V>(
    visitor: &mut V,
    with_types: bool,
    t: &ast::types::Component<Loc, Loc>,
    hoist_annotations: &dyn Fn(&mut V, &mut dyn FnMut(&mut V) -> Result<(), E>) -> Result<(), E>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
{
    let ast::types::Component {
        tparams,
        params,
        renders,
        comments: _,
    } = t;
    let in_tparam_scope = |this: &mut V| {
        this.component_type_params(params)?;
        this.component_renders_annotation(renders)
    };
    scoped_type_params(
        visitor,
        with_types,
        tparams.as_ref(),
        false,
        hoist_annotations,
        &in_tparam_scope,
    )
}

pub fn object_mapped_type_property<Loc: Dupe + Eq + Hash + Default + Clone, E, V>(
    visitor: &mut V,
    with_types: bool,
    mapped_type: &ast::types::object::MappedType<Loc, Loc>,
    hoist_annotations: &dyn Fn(&mut V, &mut dyn FnMut(&mut V) -> Result<(), E>) -> Result<(), E>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
{
    let ast::types::object::MappedType {
        loc: _,
        key_tparam,
        prop_type,
        source_type,
        name_type,
        variance: _,
        variance_op: _,
        optional: _,
        comments: _,
    } = mapped_type;
    let tparams = ast::types::TypeParams {
        loc: key_tparam.loc.dupe(),
        params: vec![key_tparam.clone()].into(), // clone needed: TypeParam is complex AST structure
        comments: None,
    };
    visitor.type_(source_type)?;
    // Visit `name_type` (the `as` clause) inside the `key_tparam` scope: the `as`
    // clause's expression refers to the key tparam (`K in keyof T as K extends ...`),
    // and may contain conditional types with `infer X` patterns whose writes must
    // be registered. Without this, downstream typing fires `MissingEnvWrite` on
    // those infer locations. Key remapping itself may be gated behind a flag, but
    // the env entries must always be tracked so the parser-accepted AST can be
    // traversed safely.
    let in_tparam_scope = |this: &mut V| {
        this.type_(prop_type)?;
        if let Some(nt) = name_type {
            this.type_(nt)?;
        }
        Ok(())
    };
    scoped_type_params(
        visitor,
        with_types,
        Some(&tparams),
        false,
        hoist_annotations,
        &in_tparam_scope,
    )
}

pub fn conditional_type<Loc: Dupe + Eq + Hash + Default, E, V>(
    visitor: &mut V,
    conditional: &ast::types::Conditional<Loc, Loc>,
    extends_in_infer_type: impl Fn(&mut V, &ast::types::Type<Loc, Loc>) -> Result<(), E>,
    mut scoped_infer_type_params: impl FnMut(
        &mut V,
        &Loc,
        &[ast::types::TypeParam<Loc, Loc>],
        &dyn Fn(&mut V) -> Result<(), E>,
    ) -> Result<(), E>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E>,
{
    let ast::types::Conditional {
        check_type,
        extends_type,
        true_type,
        false_type,
        comments: _,
    } = conditional;
    visitor.type_(check_type)?;
    extends_in_infer_type(visitor, extends_type)?;
    let tparams: Vec<_> = infer_type_hoister::hoist_infer_types(extends_type)
        .into_iter()
        .map(|(_, infer, _)| infer.tparam)
        .collect();
    scoped_infer_type_params(visitor, extends_type.loc(), &tparams, &|this| {
        this.type_(true_type)
    })?;
    visitor.type_(false_type)
}

pub fn declare_function<Loc: Dupe, E, V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E>>(
    visitor: &mut V,
    loc: &Loc,
    expr: &ast::statement::DeclareFunction<Loc, Loc>,
    hoist_annotations: &dyn Fn(&mut V, &mut dyn FnMut(&mut V) -> Result<(), E>) -> Result<(), E>,
) -> Result<(), E> {
    ast_visitor::declare_function_default(visitor, loc, expr)?;
    let ast::statement::DeclareFunction { annot, .. } = expr;
    hoist_annotations(visitor, &mut |this| this.type_annotation(annot))
}

#[allow(clippy::too_many_arguments)]
pub fn lambda<Loc: Dupe + Eq + Hash + Default, E, V>(
    visitor: &mut V,
    enable_enums: bool,
    with_types: bool,
    _is_arrow: bool,
    _fun_loc: &Loc,
    _generator_return_loc: Option<&Loc>,
    params: &ast::function::Params<Loc, Loc>,
    return_: &ast::function::ReturnAnnot<Loc, Loc>,
    predicate: Option<&ast::types::Predicate<Loc, Loc>>,
    body: &ast::function::Body<Loc, Loc>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
{
    let body_loc = match body {
        ast::function::Body::BodyExpression(e) => e.loc(),
        ast::function::Body::BodyBlock((loc, _)) => loc,
    };

    let (has_default_parameters, params_without_defaults) = remove_params_default(params);
    if has_default_parameters {
        // We need to create a second scope when we have default parameters.
        // See https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Functions/Default_parameters#scope_effects
        let mut hoist = Hoister::new(enable_enums, with_types);
        let Ok(()) = hoist.function_params(params);
        let param_bindings = hoist.into_bindings();
        // We need to visit function param default expressions without bindings
        // inside the function body.
        visitor.with_bindings(true, body_loc.dupe(), param_bindings, |this| {
            this.function_params(params)
        })?;
    }
    // We have already visited the defaults in their own scope. Now visit the
    // parameter types, each in a separate scope that includes bindings for each
    // parameter name that has been seen before in the parameter list.
    let ast::function::Params {
        loc,
        params,
        this_,
        rest,
        comments: params_comments,
    } = params_without_defaults;
    if let Some(ast::function::ThisParam {
        loc: _,
        annot,
        comments: _,
    }) = &this_
    {
        visitor.with_bindings(false, annot.loc.dupe(), Bindings::empty(), |this| {
            this.type_annotation(annot)
        })?;
    }

    let mut hoist = Hoister::new(enable_enums, with_types);
    let mut new_params = Vec::with_capacity(params.len());
    // Parameter list
    for param in params.iter() {
        // Only process RegularParam - ParamProperty doesn't have pattern bindings
        let new_param = match param {
            ast::function::Param::RegularParam {
                loc,
                argument,
                default,
            } => {
                let (annot, new_argument) = pattern_with_toplevel_annot_removed(argument);
                let new_param = ast::function::Param::RegularParam {
                    loc: loc.clone(),
                    argument: new_argument,
                    default: default.clone(),
                };
                match annot {
                    ast::types::AnnotationOrHint::Available(annot) => {
                        visitor.with_bindings(
                            false,
                            annot.loc.dupe(),
                            hoist.bindings_so_far(),
                            |this| {
                                for p in &new_params {
                                    this.function_param(p)?;
                                }
                                this.type_annotation(&annot)
                            },
                        )?;
                    }
                    ast::types::AnnotationOrHint::Missing(_) => {}
                }
                new_param
            }
            ast::function::Param::ParamProperty { .. } => param.clone(),
        };
        let Ok(()) = hoist.function_param(&new_param);
        new_params.push(new_param);
    }
    // Rest param
    let rest = rest.clone();
    let rest = if let Some(rest) = rest.as_ref() {
        let (annot, new_argument) = pattern_with_toplevel_annot_removed(&rest.argument);
        let new_rest = ast::function::RestParam {
            loc: rest.loc.dupe(),
            argument: new_argument,
            comments: rest.comments.dupe(),
        };
        match annot {
            ast::types::AnnotationOrHint::Available(annot) => {
                visitor.with_bindings(
                    false,
                    annot.loc.dupe(),
                    hoist.bindings_so_far(),
                    |this| {
                        for p in &new_params {
                            this.function_param(p)?;
                        }
                        this.type_annotation(&annot)
                    },
                )?;
            }
            ast::types::AnnotationOrHint::Missing(_) => {}
        }
        let Ok(()) = hoist.function_rest_param(&new_rest);
        Some(new_rest)
    } else {
        None
    };
    let params_without_toplevel_annots = ast::function::Params {
        loc,
        params: new_params.into(),
        this_,
        rest,
        comments: params_comments,
    };
    // return
    match return_ {
        ast::function::ReturnAnnot::Missing(_) => {}
        ast::function::ReturnAnnot::Available(ast::types::Annotation { loc, .. })
        | ast::function::ReturnAnnot::TypeGuard(ast::types::TypeGuardAnnotation { loc, .. }) => {
            visitor.with_bindings(false, loc.dupe(), hoist.bindings_so_far(), |this| {
                if with_types {
                    this.function_params(&params_without_toplevel_annots)?;
                    this.function_return_annotation(return_)?;
                }
                Ok(())
            })?;
        }
    }
    // Finally, visit the body in a scope that includes bindings for all params
    // and hoisted body declarations.
    let bindings = {
        let Ok(()) = hoist.function_body_any(body);
        hoist.into_bindings()
    };
    // We have already visited the defaults in their own scope. Now visit the
    // params without the defaults in the function's scope.
    visitor.with_bindings(false, body_loc.dupe(), bindings, |this| {
        this.function_params(&params_without_toplevel_annots)?;
        if let Some(p) = predicate {
            this.predicate(p)?;
        }
        this.function_body_any(body)
    })
}

pub fn function_expression_without_name<Loc: Dupe + Eq + Hash + Default, E, V>(
    visitor: &mut V,
    enable_enums: bool,
    with_types: bool,
    is_arrow: bool,
    loc: &Loc,
    expr: &ast::function::Function<Loc, Loc>,
    hoist_annotations: &dyn Fn(&mut V, &mut dyn FnMut(&mut V) -> Result<(), E>) -> Result<(), E>,
    this_binding_function_id_opt: impl Fn(
        &mut V,
        &Loc,
        bool,
        Option<&ast::Identifier<Loc, Loc>>,
    ) -> Result<(), E>,
    lambda_callback: impl Fn(
        &mut V,
        bool,
        bool,
        bool,
        &Loc,
        Option<&Loc>,
        &ast::function::Params<Loc, Loc>,
        &ast::function::ReturnAnnot<Loc, Loc>,
        Option<&ast::types::Predicate<Loc, Loc>>,
        &ast::function::Body<Loc, Loc>,
    ) -> Result<(), E>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
{
    let ast::function::Function {
        id,
        params,
        body,
        return_,
        tparams,
        async_: _,
        generator,
        effect_: _,
        predicate,
        sig_loc: _,
        comments: _,
    } = expr;
    let bindings = match id {
        Some(name) => Bindings::singleton(Entry {
            loc: name.loc.dupe(),
            key: VarKey::Name(name.name.dupe()),
            kind: Kind::Function,
        }),
        None => Bindings::empty(),
    };
    let generator_return_loc = match (generator, return_) {
        (false, _) => None,
        (
            true,
            ast::function::ReturnAnnot::Available(ast::types::Annotation { loc, .. })
            | ast::function::ReturnAnnot::Missing(loc)
            | ast::function::ReturnAnnot::TypeGuard(ast::types::TypeGuardAnnotation { loc, .. }),
        ) => Some(loc),
    };
    visitor.with_bindings(true, loc.dupe(), bindings, |this| {
        if is_arrow {
            if let Some(id) = id {
                this.function_identifier(id)?;
            }
        } else {
            this_binding_function_id_opt(this, loc, params.this_.is_some(), id.as_ref())?;
        }
        // This function is not hoisted, so we just traverse the signature
        scoped_type_params(
            this,
            with_types,
            tparams.as_ref(),
            false,
            hoist_annotations,
            &|this2| {
                lambda_callback(
                    this2,
                    enable_enums,
                    with_types,
                    is_arrow,
                    loc,
                    generator_return_loc,
                    params,
                    return_,
                    predicate.as_ref(),
                    body,
                )
            },
        )
    })
}

pub fn function_declaration<Loc: Dupe + Eq + Hash + Default, E, V>(
    visitor: &mut V,
    enable_enums: bool,
    with_types: bool,
    loc: &Loc,
    expr: &ast::function::Function<Loc, Loc>,
    hoist_annotations: &dyn Fn(&mut V, &mut dyn FnMut(&mut V) -> Result<(), E>) -> Result<(), E>,
    this_binding_function_id_opt: impl Fn(
        &mut V,
        &Loc,
        bool,
        Option<&ast::Identifier<Loc, Loc>>,
    ) -> Result<(), E>,
    lambda_callback: impl Fn(
        &mut V,
        bool,
        bool,
        bool,
        &Loc,
        Option<&Loc>,
        &ast::function::Params<Loc, Loc>,
        &ast::function::ReturnAnnot<Loc, Loc>,
        Option<&ast::types::Predicate<Loc, Loc>>,
        &ast::function::Body<Loc, Loc>,
    ) -> Result<(), E>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
{
    let ast::function::Function {
        id,
        params,
        body,
        return_,
        tparams,
        async_: _,
        generator,
        effect_: _,
        predicate,
        sig_loc: _,
        comments: _,
    } = expr;
    this_binding_function_id_opt(visitor, loc, params.this_.is_some(), id.as_ref())?;
    let generator_return_loc = match (generator, return_) {
        (false, _) => None,
        (
            true,
            ast::function::ReturnAnnot::Available(ast::types::Annotation { loc, .. })
            | ast::function::ReturnAnnot::Missing(loc)
            | ast::function::ReturnAnnot::TypeGuard(ast::types::TypeGuardAnnotation { loc, .. }),
        ) => Some(loc),
    };
    scoped_type_params(
        visitor,
        with_types,
        tparams.as_ref(),
        true,
        hoist_annotations,
        &|this| {
            lambda_callback(
                this,
                enable_enums,
                with_types,
                false,
                loc,
                generator_return_loc,
                params,
                return_,
                predicate.as_ref(),
                body,
            )
        },
    )
}

pub fn component_body_with_params<Loc: Dupe + Eq + Hash + Default, E, V>(
    visitor: &mut V,
    enable_enums: bool,
    with_types: bool,
    body: &(Loc, ast::statement::Block<Loc, Loc>),
    params: &ast::statement::component_params::Params<Loc, Loc>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
{
    // In component syntax param types and defaults cannot reference other params, so we visit
    // the types and defaults without including the param bindings.
    for param in params.params.iter() {
        visitor.default_opt(param.default.as_ref())?;
    }
    for param in params.params.iter() {
        let annot = pattern_toplevel_annot(&param.local);
        visitor.type_annotation_hint(&annot)?;
    }
    if let Some(rest) = &params.rest {
        let annot = pattern_toplevel_annot(&rest.argument);
        visitor.type_annotation_hint(&annot)?;
    }
    let bindings = {
        let mut hoist = Hoister::new(enable_enums, with_types);
        let Ok(()) = hoist.component_params(params);
        let Ok(()) = hoist.component_body(body);
        hoist.into_bindings()
    };
    let mut params_without_annots_and_defaults = Vec::new();
    for param in params.params.iter() {
        let (_, local) = pattern_with_toplevel_annot_removed(&param.local);
        let new_param = ast::statement::component_params::Param {
            loc: param.loc.dupe(),
            name: param.name.clone(),
            local,
            default: None,
            shorthand: param.shorthand,
        };
        params_without_annots_and_defaults.push(new_param);
    }
    let rest_param_without_annots_and_defaults = if let Some(rest) = &params.rest {
        let (_, argument) = pattern_with_toplevel_annot_removed(&rest.argument);
        Some(ast::statement::component_params::RestParam {
            loc: rest.loc.dupe(),
            argument,
            comments: rest.comments.dupe(),
        })
    } else {
        None
    };
    let params_without_annots_and_defaults = ast::statement::component_params::Params {
        loc: params.loc.dupe(),
        params: params_without_annots_and_defaults.into(),
        rest: rest_param_without_annots_and_defaults,
        comments: params.comments.dupe(),
    };
    visitor.with_bindings(false, body.0.dupe(), bindings, |this| {
        // Visit params to ensure they are declared
        this.component_params(&params_without_annots_and_defaults)?;
        this.component_body(body)
    })
}

pub fn component_declaration<Loc: Dupe + Eq + Hash + Default, E, V>(
    visitor: &mut V,
    with_types: bool,
    loc: &Loc,
    component: &ast::statement::ComponentDeclaration<Loc, Loc>,
    hoist_annotations: &dyn Fn(&mut V, &mut dyn FnMut(&mut V) -> Result<(), E>) -> Result<(), E>,
    component_body_with_params_fn: impl Fn(
        &mut V,
        &(Loc, ast::statement::Block<Loc, Loc>),
        &ast::statement::component_params::Params<Loc, Loc>,
    ) -> Result<(), E>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
{
    let ast::statement::ComponentDeclaration {
        id,
        tparams,
        params,
        body,
        renders,
        comments: _,
        sig_loc: _,
        async_: _,
    } = component;
    visitor.component_identifier(id)?;
    scoped_type_params(
        visitor,
        with_types,
        tparams.as_ref(),
        true,
        hoist_annotations,
        &|this| {
            match body {
                None => {
                    // For ambient components, still process params but no body
                    let empty_body = (
                        loc.dupe(),
                        ast::statement::Block {
                            body: Vec::new().into(),
                            comments: None,
                        },
                    );
                    component_body_with_params_fn(this, &empty_body, params)?;
                }
                Some(body) => {
                    component_body_with_params_fn(this, body, params)?;
                }
            }
            if with_types {
                hoist_annotations(this, &mut |this2| {
                    this2.component_renders_annotation(renders)
                })?;
            }
            Ok(())
        },
    )
}

pub fn declare_component<Loc: Dupe + Eq + Hash + Default, E, V>(
    visitor: &mut V,
    with_types: bool,
    decl: &ast::statement::DeclareComponent<Loc, Loc>,
    hoist_annotations: &dyn Fn(&mut V, &mut dyn FnMut(&mut V) -> Result<(), E>) -> Result<(), E>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
{
    let ast::statement::DeclareComponent {
        id,
        tparams,
        params,
        renders,
        comments: _,
    } = decl;
    visitor.component_identifier(id)?;
    scoped_type_params(
        visitor,
        with_types,
        tparams.as_ref(),
        true,
        hoist_annotations,
        &|this| {
            // Visit type annotations without creating bindings, same as regular components
            for param in params.params.iter() {
                let annot = pattern_toplevel_annot(&param.local);
                this.type_annotation_hint(&annot)?;
            }
            if let Some(rest) = &params.rest {
                let annot = pattern_toplevel_annot(&rest.argument);
                this.type_annotation_hint(&annot)?;
            }
            hoist_annotations(this, &mut |this2| {
                this2.component_renders_annotation(renders)
            })
        },
    )
}

pub fn class_expression<Loc: Dupe + Eq + Hash + Default, E, V>(
    visitor: &mut V,
    loc: &Loc,
    cls: &ast::class::Class<Loc, Loc>,
    on_cls: impl FnOnce(&mut V, &Loc, &ast::class::Class<Loc, Loc>) -> Result<(), E>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
{
    let ast::class::Class { id, .. } = cls;
    let bindings = match id {
        Some(name) => Bindings::singleton(Entry {
            loc: name.loc.dupe(),
            key: VarKey::Name(name.name.dupe()),
            kind: Kind::Class,
        }),
        None => Bindings::empty(),
    };
    visitor.with_bindings(true, loc.dupe(), bindings, |this| on_cls(this, loc, cls))
}

pub fn class_<Loc: Dupe + Eq + Hash + Default, E, V>(
    visitor: &mut V,
    with_types: bool,
    loc: &Loc,
    cls: &ast::class::Class<Loc, Loc>,
    class_identifier_opt: impl Fn(&mut V, &Loc, Option<&ast::Identifier<Loc, Loc>>) -> Result<(), E>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
{
    let ast::class::Class {
        id,
        body,
        tparams,
        extends,
        implements,
        class_decorators,
        abstract_: _,
        comments: _,
    } = cls;
    for decorator in class_decorators.iter() {
        visitor.class_decorator(decorator)?;
    }
    let extends_targs = if let Some(extends_inner) = extends.as_ref() {
        visitor.expression(&extends_inner.expr)?;
        extends_inner.targs.as_ref()
    } else {
        None
    };
    class_identifier_opt(visitor, loc, id.as_ref())?;
    let mut implements_targs = Vec::new();
    if let Some(implements_inner) = implements.as_ref() {
        for interface in implements_inner.interfaces.iter() {
            visitor.generic_identifier_type(&interface.id)?;
            if let Some(targs) = interface.targs.as_ref() {
                implements_targs.push(targs);
            }
        }
    }
    // No hoisting: `hoist_annotation` is false and the hoist op just runs the function directly.
    scoped_type_params(
        visitor,
        with_types,
        tparams.as_ref(),
        false,
        &|_v, f| f(_v), // identity hoist_op
        &|this| {
            if let Some(targs) = extends_targs {
                this.type_args(targs)?;
            }
            for targs in &implements_targs {
                this.type_args(targs)?;
            }
            this.class_body(body)
        },
    )
}

pub fn declare_class<Loc: Dupe + Eq + Hash + Default, E, V>(
    visitor: &mut V,
    with_types: bool,
    decl: &ast::statement::DeclareClass<Loc, Loc>,
    hoist_annotations: &dyn Fn(&mut V, &mut dyn FnMut(&mut V) -> Result<(), E>) -> Result<(), E>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
{
    let ast::statement::DeclareClass {
        id,
        tparams,
        body,
        extends,
        mixins,
        implements,
        abstract_: _,
        comments: _,
    } = decl;
    visitor.class_identifier(id)?;
    let extends_targs = {
        fn extends_targs_of<'a, Loc: Dupe + Eq + Hash + Default, E, V>(
            visitor: &mut V,
            ext: &'a ast::statement::DeclareClassExtends<Loc, Loc>,
        ) -> Result<Option<&'a ast::types::TypeArgs<Loc, Loc>>, E>
        where
            V: for<'b> AstVisitor<'b, Loc, Loc, &'b Loc, E> + WithBindings<Loc, E>,
        {
            match ext {
                ast::statement::DeclareClassExtends::ExtendsIdent(generic) => {
                    visitor.generic_identifier_type(&generic.id)?;
                    Ok(generic.targs.as_ref())
                }
                ast::statement::DeclareClassExtends::ExtendsCall { callee: _, arg } => {
                    extends_targs_of(visitor, &arg.1)
                }
            }
        }
        if let Some((_loc, ext)) = extends.as_ref() {
            extends_targs_of(visitor, ext)?
        } else {
            None
        }
    };
    let mut mixins_targs = Vec::new();
    for (_, generic) in mixins.iter() {
        visitor.generic_identifier_type(&generic.id)?;
        if let Some(targs) = generic.targs.as_ref() {
            mixins_targs.push(targs);
        }
    }
    let mut implements_targs = Vec::new();
    if let Some(implements_inner) = implements.as_ref() {
        for interface in implements_inner.interfaces.iter() {
            visitor.generic_identifier_type(&interface.id)?;
            if let Some(targs) = interface.targs.as_ref() {
                implements_targs.push(targs);
            }
        }
    }
    scoped_type_params(
        visitor,
        with_types,
        tparams.as_ref(),
        false,
        hoist_annotations,
        &|this| {
            if let Some(targs) = extends_targs {
                this.type_args(targs)?;
            }
            for targs in &mixins_targs {
                this.type_args(targs)?;
            }
            for targs in &implements_targs {
                this.type_args(targs)?;
            }
            let (_loc, obj_type) = body;
            this.object_type(obj_type)
        },
    )
}

pub fn record_declaration<Loc: Dupe + Eq + Hash + Default, E, V>(
    visitor: &mut V,
    with_types: bool,
    record: &ast::statement::RecordDeclaration<Loc, Loc>,
    hoist_annotations: &dyn Fn(&mut V, &mut dyn FnMut(&mut V) -> Result<(), E>) -> Result<(), E>,
) -> Result<(), E>
where
    V: for<'a> AstVisitor<'a, Loc, Loc, &'a Loc, E> + WithBindings<Loc, E>,
{
    let ast::statement::RecordDeclaration {
        id,
        tparams,
        implements,
        body,
        comments: _,
        invalid_syntax: _,
    } = record;
    visitor.pattern_identifier(Some(ast::VariableKind::Const), id)?;
    let implements_targs = if let Some(implements_inner) = implements.as_ref() {
        implements_inner
            .interfaces
            .iter()
            .filter_map(|interface| {
                let _ = visitor.generic_identifier_type(&interface.id);
                interface.targs.as_ref()
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    scoped_type_params(
        visitor,
        with_types,
        tparams.as_ref(),
        false,
        hoist_annotations,
        &|this| {
            for targs in &implements_targs {
                this.type_args(targs)?;
            }
            this.record_declaration_body(body)
        },
    )
}

struct Env<Loc> {
    stack: Vec<BTreeMap<FlowSmolStr, Def<Loc>>>,
}

impl<Loc: Dupe> Env<Loc> {
    fn empty() -> Self {
        Self { stack: Vec::new() }
    }

    fn get(&self, name: &str) -> Option<&Def<Loc>> {
        for map in self.stack.iter().rev() {
            if let Some(def) = map.get(name) {
                return Some(def);
            }
        }
        None
    }
}

// The main scope builder visitor
pub struct ScopeBuilder<Loc> {
    allow_declare_global_captures: bool,
    enable_enums: bool,
    with_types: bool,
    info: ScopeInfo<Loc>,
    env: Env<Loc>,
    current_scope: Option<ScopeId>,
    scope_counter: u32,
    counter: u32,
    uses: Vec<(Loc, FlowSmolStr)>,
}

impl<Loc: Dupe + Eq + Ord + Hash + Default> ScopeBuilder<Loc> {
    pub fn new(enable_enums: bool, with_types: bool, allow_declare_global_captures: bool) -> Self {
        Self {
            allow_declare_global_captures,
            enable_enums,
            with_types,
            info: ScopeInfo::empty(),
            env: Env::empty(),
            current_scope: None,
            scope_counter: 0,
            counter: 0,
            uses: Vec::new(),
        }
    }

    pub fn into_info(mut self) -> ScopeInfo<Loc> {
        self.info.finalize();
        self.info
    }

    pub fn in_toplevel_scope(&self) -> bool {
        self.current_scope == Some(ScopeId(0))
    }

    fn new_scope(&mut self) -> ScopeId {
        let id = self.scope_counter;
        self.scope_counter += 1;
        ScopeId(id)
    }

    fn next(&mut self) -> ScopeId {
        let name = self.counter;
        self.counter += 1;
        self.info.max_distinct = self.info.max_distinct.max(self.counter);
        ScopeId(name)
    }

    fn hoist_annotations(
        &mut self,
        mut f: impl FnMut(&mut Self) -> Result<(), !>,
    ) -> Result<(), !> {
        f(self)
    }

    fn switch_cases(
        &mut self,
        _loc: &Loc,
        _discriminant: &ast::expression::Expression<Loc, Loc>,
        cases: &[ast::statement::switch::Case<Loc, Loc>],
    ) -> Result<(), !> {
        for case in cases {
            let Ok(()) = self.switch_case(case);
        }
        Ok(())
    }

    fn scoped_for_in_statement(
        &mut self,
        loc: &Loc,
        stmt: &ast::statement::ForIn<Loc, Loc>,
    ) -> Result<(), !> {
        ast_visitor::for_in_statement_default(self, loc, stmt)
    }

    fn scoped_for_of_statement(
        &mut self,
        loc: &Loc,
        stmt: &ast::statement::ForOf<Loc, Loc>,
    ) -> Result<(), !> {
        ast_visitor::for_of_statement_default(self, loc, stmt)
    }

    fn scoped_for_statement(
        &mut self,
        loc: &Loc,
        stmt: &ast::statement::For<Loc, Loc>,
    ) -> Result<(), !> {
        ast_visitor::for_statement_default(self, loc, stmt)
    }

    fn extends_in_infer_type(&mut self, t: &ast::types::Type<Loc, Loc>) -> Result<(), !> {
        self.type_(t)
    }

    fn this_binding_function_id_opt(
        &mut self,
        _fun_loc: &Loc,
        _has_this_annot: bool,
        ident: Option<&ast::Identifier<Loc, Loc>>,
    ) -> Result<(), !> {
        if let Some(id) = ident {
            let Ok(()) = self.function_identifier(id);
        }
        Ok(())
    }

    fn function_expression_without_name(
        &mut self,
        is_arrow: bool,
        loc: &Loc,
        expr: &ast::function::Function<Loc, Loc>,
    ) -> Result<(), !> {
        let enable_enums = self.enable_enums;
        let with_types = self.with_types;
        function_expression_without_name(
            self,
            enable_enums,
            with_types,
            is_arrow,
            loc,
            expr,
            &|v, f| v.hoist_annotations(f),
            |v, loc, has_this, id| v.this_binding_function_id_opt(loc, has_this, id),
            // Default lambda callback just calls scope_builder::lambda
            |v,
             enable_enums_inner,
             with_types_inner,
             is_arrow_inner,
             fun_loc,
             generator_return_loc,
             params,
             return_,
             predicate,
             body| {
                lambda(
                    v,
                    enable_enums_inner,
                    with_types_inner,
                    is_arrow_inner,
                    fun_loc,
                    generator_return_loc,
                    params,
                    return_,
                    predicate,
                    body,
                )
            },
        )
    }

    fn class_identifier_opt(
        &mut self,
        _class_loc: &Loc,
        id: Option<&ast::Identifier<Loc, Loc>>,
    ) -> Result<(), !> {
        if let Some(id) = id {
            let Ok(()) = self.class_identifier(id);
        }
        Ok(())
    }

    fn binding_infer_type_identifier(&mut self, id: &ast::Identifier<Loc, Loc>) -> Result<(), !> {
        self.binding_type_identifier(id)
    }
}

pub trait WithBindings<Loc: Dupe, E> {
    fn with_bindings<T>(
        &mut self,
        lexical: bool,
        loc: Loc,
        bindings: Bindings<Loc>,
        visit: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E>;

    fn with_isolated_bindings<T>(
        &mut self,
        lexical: bool,
        loc: Loc,
        bindings: Bindings<Loc>,
        visit: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        self.with_bindings(lexical, loc, bindings, visit)
    }

    fn with_type_param_default<T>(
        &mut self,
        visit: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        visit(self)
    }

    fn with_type_params<T>(
        &mut self,
        _tparams: &ast::types::TypeParams<Loc, Loc>,
        visit: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        visit(self)
    }
}

impl<Loc: Dupe + Eq + Ord + Hash + Default, E> WithBindings<Loc, E> for ScopeBuilder<Loc> {
    fn with_bindings<T>(
        &mut self,
        lexical: bool,
        loc: Loc,
        bindings: Bindings<Loc>,
        visit: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        let save_counter = self.counter;
        let save_uses = std::mem::take(&mut self.uses);
        let parent = self.current_scope;
        let child = self.new_scope();
        self.current_scope = Some(child);
        let new_env = {
            let mut env = BTreeMap::new();
            let named = bindings
                .to_assoc()
                .into_iter()
                .filter_map(|(key, value)| Some((key.into_name()?, value)));
            for (x, (kind, locs)) in named {
                let name = match self.env.get(&x) {
                    Some(def) => def.name,
                    None => self.next(),
                };
                env.insert(
                    x.dupe(),
                    Def {
                        locs,
                        name,
                        actual_name: x,
                        kind,
                    },
                );
            }
            env
        };
        self.env.stack.push(new_env);
        let result = visit(self);
        let scope = {
            let mut locals = BTreeMap::new();
            let mut globals_set = std::collections::HashSet::new();
            let mut globals = Vec::new();
            for use_id in &self.uses {
                match self.env.get(&use_id.1) {
                    Some(def) => {
                        locals.insert(use_id.0.dupe(), def.clone());
                    }
                    None => {
                        if globals_set.insert(use_id.1.dupe()) {
                            globals.push(use_id.1.dupe());
                        }
                    }
                }
            }
            let defs = self.env.stack.pop().unwrap();
            for def in defs.values() {
                for loc_ref in &def.locs {
                    locals.insert(loc_ref.dupe(), def.clone());
                }
            }
            Scope {
                lexical,
                parent,
                defs,
                locals,
                globals,
                loc,
            }
        };
        self.info.scopes.insert(child, scope);
        self.uses = save_uses;
        self.current_scope = parent;
        self.counter = save_counter;
        result
    }

    fn with_isolated_bindings<T>(
        &mut self,
        lexical: bool,
        loc: Loc,
        bindings: Bindings<Loc>,
        visit: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        let outer_env = std::mem::replace(&mut self.env, Env::empty());
        let result = self.with_bindings(lexical, loc, bindings, visit);
        self.env = outer_env;
        result
    }
}

impl<'ast, Loc: Dupe + Eq + Ord + Hash + Default> AstVisitor<'ast, Loc> for ScopeBuilder<Loc> {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn identifier(&mut self, id: &ast::Identifier<Loc, Loc>) -> Result<(), !> {
        self.uses.push((id.loc.dupe(), id.name.dupe()));
        Ok(())
    }

    fn jsx_element_name_identifier(
        &mut self,
        ident: &ast::jsx::Identifier<Loc, Loc>,
    ) -> Result<(), !> {
        self.uses.push((ident.loc.dupe(), ident.name.dupe()));
        Ok(())
    }

    fn type_alias(
        &mut self,
        _loc: &Loc,
        alias: &ast::statement::TypeAlias<Loc, Loc>,
    ) -> Result<(), !> {
        let with_types = self.with_types;
        type_alias(self, with_types, alias, &|v, f| v.hoist_annotations(f))
    }

    fn opaque_type(
        &mut self,
        _loc: &Loc,
        opaque: &ast::statement::OpaqueType<Loc, Loc>,
    ) -> Result<(), !> {
        let with_types = self.with_types;
        opaque_type(self, with_types, opaque, &|v, f| v.hoist_annotations(f))
    }

    fn interface(
        &mut self,
        _loc: &Loc,
        interface_decl: &ast::statement::Interface<Loc, Loc>,
    ) -> Result<(), !> {
        let with_types = self.with_types;
        interface(self, with_types, interface_decl, &|v, f| {
            v.hoist_annotations(f)
        })
    }

    // don't rename the `foo` in `x.foo`
    fn member_property_identifier(&mut self, id: &ast::Identifier<Loc, Loc>) -> Result<(), !> {
        let _ = id;
        Ok(())
    }

    // don't rename the `foo` in `typeof x.foo`
    fn typeof_member_identifier(&mut self, ident: &ast::Identifier<Loc, Loc>) -> Result<(), !> {
        let _ = ident;
        Ok(())
    }

    // don't rename the `ComponentType` in `React.ComponentType`
    fn member_type_identifier(&mut self, id: &ast::Identifier<Loc, Loc>) -> Result<(), !> {
        let _ = id;
        Ok(())
    }

    // don't rename the `foo` in `const {foo: bar} = x`
    fn pattern_object_property_identifier_key(
        &mut self,
        _kind: Option<ast::VariableKind>,
        id: &ast::Identifier<Loc, Loc>,
    ) -> Result<(), !> {
        let _ = id;
        Ok(())
    }

    fn match_object_pattern_property_key(
        &mut self,
        _key: &ast::match_pattern::object_pattern::Key<Loc, Loc>,
    ) -> Result<(), !> {
        Ok(())
    }

    fn match_object_pattern_property(
        &mut self,
        prop: &ast::match_pattern::object_pattern::Property<Loc, Loc>,
    ) -> Result<(), !> {
        match prop {
            ast::match_pattern::object_pattern::Property::Valid { .. } => {
                let Ok(()) = ast_visitor::match_object_pattern_property_default(self, prop);
            }
            ast::match_pattern::object_pattern::Property::InvalidShorthand { .. } => {}
        }
        Ok(())
    }

    // don't rename the `Foo` in `enum E { Foo }`
    fn enum_member_identifier(&mut self, id: &ast::Identifier<Loc, Loc>) -> Result<(), !> {
        let _ = id;
        Ok(())
    }

    // don't rename the `foo` in `{ foo: ... }`
    fn object_key_identifier(&mut self, id: &ast::Identifier<Loc, Loc>) -> Result<(), !> {
        let _ = id;
        Ok(())
    }

    // don't rename the `foo` in `component C(foo: number) {}`
    fn component_param_name(
        &mut self,
        param_name: &ast::statement::component_params::ParamName<Loc, Loc>,
    ) -> Result<(), !> {
        let _ = param_name;
        Ok(())
    }

    fn import_declaration(
        &mut self,
        loc: &Loc,
        decl: &ast::statement::ImportDeclaration<Loc, Loc>,
    ) -> Result<(), !> {
        // when `with_types` is false, don't visit `import type ...` or `import typeof ...`
        match (self.with_types, decl.import_kind) {
            (
                false,
                ast::statement::ImportKind::ImportType | ast::statement::ImportKind::ImportTypeof,
            ) => {}
            _ => {
                let Ok(()) = ast_visitor::import_declaration_default(self, loc, decl);
            }
        }
        Ok(())
    }

    fn import_named_specifier(
        &mut self,
        import_kind: ast::statement::ImportKind,
        spec: &ast::statement::import_declaration::NamedSpecifier<Loc, Loc>,
    ) -> Result<(), !> {
        let with_types = self.with_types;
        import_named_specifier(self, with_types, import_kind, spec)
    }

    // don't rename the `bar` in `export {foo as bar}`
    fn export_named_declaration_specifier(
        &mut self,
        spec: &ast::statement::export_named_declaration::ExportSpecifier<Loc, Loc>,
    ) -> Result<(), !> {
        let ast::statement::export_named_declaration::ExportSpecifier {
            loc: _,
            local,
            exported: _,
            export_kind: _,
            from_remote: _,
            imported_name_def_loc: _,
        } = spec;
        let Ok(()) = self.identifier(local);
        Ok(())
    }

    fn block(&mut self, loc: &Loc, stmt: &ast::statement::Block<Loc, Loc>) -> Result<(), !> {
        let mut lexical_hoist = LexicalHoister::new(self.enable_enums);
        let Ok(()) = lexical_hoist.block(loc, stmt);
        let lexical_bindings = lexical_hoist.into_bindings();
        self.with_bindings(true, loc.dupe(), lexical_bindings, |this| {
            ast_visitor::block_default(this, loc, stmt)
        })
    }

    fn function_body(&mut self, body: &(Loc, ast::statement::Block<Loc, Loc>)) -> Result<(), !> {
        let (loc, block) = body;
        ast_visitor::block_default(self, loc, block)
    }

    fn component_body(&mut self, body: &(Loc, ast::statement::Block<Loc, Loc>)) -> Result<(), !> {
        let (loc, block) = body;
        ast_visitor::block_default(self, loc, block)
    }

    fn match_case<B>(
        &mut self,
        case: &'ast ast::match_::Case<Loc, Loc, B>,
        on_case_body: &mut impl FnMut(&mut Self, &'ast B) -> Result<(), !>,
    ) -> Result<(), !> {
        let enable_enums = self.enable_enums;
        match_case(self, enable_enums, case, on_case_body)
    }

    fn switch(&mut self, loc: &Loc, switch: &ast::statement::Switch<Loc, Loc>) -> Result<(), !> {
        let ast::statement::Switch {
            discriminant,
            cases,
            comments: _,
            exhaustive_out: _,
        } = switch;
        let Ok(()) = self.expression(discriminant);
        let mut lexical_hoist = LexicalHoister::new(self.enable_enums);
        for case in switch.cases.iter() {
            let Ok(()) = lexical_hoist.statement_list(&case.consequent);
        }
        let lexical_bindings = lexical_hoist.into_bindings();
        self.with_bindings(true, loc.dupe(), lexical_bindings, |this| {
            this.switch_cases(loc, discriminant, cases)
        })
    }

    fn for_in_statement(
        &mut self,
        loc: &Loc,
        stmt: &ast::statement::ForIn<Loc, Loc>,
    ) -> Result<(), !> {
        let enable_enums = self.enable_enums;
        for_in_statement(self, enable_enums, loc, stmt, Self::scoped_for_in_statement)
    }

    fn for_of_statement(
        &mut self,
        loc: &Loc,
        stmt: &ast::statement::ForOf<Loc, Loc>,
    ) -> Result<(), !> {
        let enable_enums = self.enable_enums;
        for_of_statement(self, enable_enums, loc, stmt, Self::scoped_for_of_statement)
    }

    fn for_statement(&mut self, loc: &Loc, stmt: &ast::statement::For<Loc, Loc>) -> Result<(), !> {
        let enable_enums = self.enable_enums;
        for_statement(self, enable_enums, loc, stmt, Self::scoped_for_statement)
    }

    fn catch_clause(
        &mut self,
        clause: &ast::statement::try_::CatchClause<Loc, Loc>,
    ) -> Result<(), !> {
        let enable_enums = self.enable_enums;
        catch_clause(self, enable_enums, clause, |this, clause| {
            ast_visitor::catch_clause_default(this, clause)
        })
    }

    fn declare_module(
        &mut self,
        _loc: &Loc,
        m: &ast::statement::DeclareModule<Loc, Loc>,
    ) -> Result<(), !> {
        let ast::statement::DeclareModule {
            id: _,
            body,
            comments: _,
        } = m;
        let (loc, block) = body;
        let bindings = {
            let mut hoist = Hoister::new(self.enable_enums, self.with_types);
            let Ok(()) = hoist.block(loc, block);
            hoist.into_bindings()
        };
        self.with_bindings(false, loc.dupe(), bindings, |this| this.block(loc, block))
    }

    fn declare_namespace(
        &mut self,
        _loc: &Loc,
        n: &ast::statement::DeclareNamespace<Loc, Loc>,
    ) -> Result<(), !> {
        let ast::statement::DeclareNamespace {
            id,
            body,
            implicit_declare: _,
            keyword: _,
            comments: _,
        } = n;
        match id {
            ast::statement::declare_namespace::Id::Global(_) => {}
            ast::statement::declare_namespace::Id::Local(id) => {
                let Ok(()) = self.pattern_identifier(Some(ast::VariableKind::Const), id);
            }
        }
        let (loc, block) = body;
        let bindings = {
            let mut hoist = Hoister::new(self.enable_enums, self.with_types);
            let Ok(()) = hoist.block(loc, block);
            hoist.into_bindings()
        };
        if matches!(id, ast::statement::declare_namespace::Id::Global(_))
            && !self.allow_declare_global_captures
        {
            self.with_isolated_bindings(false, loc.dupe(), bindings, |this| this.block(loc, block))
        } else {
            self.with_bindings(false, loc.dupe(), bindings, |this| this.block(loc, block))
        }
    }

    fn conditional_type(
        &mut self,
        conditional: &ast::types::Conditional<Loc, Loc>,
    ) -> Result<(), !> {
        let with_types = self.with_types;
        conditional_type(
            self,
            conditional,
            |v, t| v.extends_in_infer_type(t),
            |this, loc, tps, in_tparam_scope| {
                scoped_infer_type_params(
                    this,
                    with_types,
                    loc,
                    tps,
                    |s, id| s.binding_infer_type_identifier(id),
                    in_tparam_scope,
                )
            },
        )
    }

    fn infer_type(&mut self, _infer: &ast::types::Infer<Loc, Loc>) -> Result<(), !> {
        // Visits of infer type are skipped, because they are handled in conditional type above
        Ok(())
    }

    fn component_declaration(
        &mut self,
        loc: &Loc,
        component: &ast::statement::ComponentDeclaration<Loc, Loc>,
    ) -> Result<(), !> {
        let with_types = self.with_types;
        let enable_enums = self.enable_enums;
        component_declaration(
            self,
            with_types,
            loc,
            component,
            &|v, f| v.hoist_annotations(f),
            |v, body, params| component_body_with_params(v, enable_enums, with_types, body, params),
        )
    }

    fn declare_component(
        &mut self,
        _loc: &Loc,
        decl: &ast::statement::DeclareComponent<Loc, Loc>,
    ) -> Result<(), !> {
        let with_types = self.with_types;
        declare_component(self, with_types, decl, &|v, f| v.hoist_annotations(f))
    }

    fn function_declaration(
        &mut self,
        loc: &Loc,
        expr: &ast::function::Function<Loc, Loc>,
    ) -> Result<(), !> {
        let enable_enums = self.enable_enums;
        let with_types = self.with_types;
        function_declaration(
            self,
            enable_enums,
            with_types,
            loc,
            expr,
            &|v, f| v.hoist_annotations(f),
            |v, loc, has_this, id| v.this_binding_function_id_opt(loc, has_this, id),
            |v,
             enable_enums_inner,
             with_types_inner,
             is_arrow_inner,
             fun_loc,
             generator_return_loc,
             params,
             return_,
             predicate,
             body| {
                lambda(
                    v,
                    enable_enums_inner,
                    with_types_inner,
                    is_arrow_inner,
                    fun_loc,
                    generator_return_loc,
                    params,
                    return_,
                    predicate,
                    body,
                )
            },
        )
    }

    fn function_(&mut self, loc: &Loc, expr: &ast::function::Function<Loc, Loc>) -> Result<(), !> {
        self.function_expression_without_name(false, loc, expr)
    }

    fn arrow_function(
        &mut self,
        loc: &Loc,
        expr: &ast::function::Function<Loc, Loc>,
    ) -> Result<(), !> {
        self.function_expression_without_name(true, loc, expr)
    }

    fn declare_variable(
        &mut self,
        _loc: &Loc,
        decl: &ast::statement::DeclareVariable<Loc, Loc>,
    ) -> Result<(), !> {
        let kind = decl.kind;
        for declarator in decl.declarations.iter() {
            if let ast::pattern::Pattern::Identifier { inner, .. } = &declarator.id {
                let Ok(()) = self.pattern_identifier(Some(kind), &inner.name);
            }
        }
        self.hoist_annotations(|this| {
            for declarator in decl.declarations.iter() {
                if let ast::pattern::Pattern::Identifier { inner, .. } = &declarator.id {
                    if let ast::types::AnnotationOrHint::Available(annot) = &inner.annot {
                        let Ok(()) = this.type_annotation(annot);
                    }
                    if let Some(init) = &declarator.init {
                        let Ok(()) = this.expression(init);
                    }
                }
            }
            Ok(())
        })
    }

    fn declare_function(
        &mut self,
        loc: &Loc,
        expr: &ast::statement::DeclareFunction<Loc, Loc>,
    ) -> Result<(), !> {
        declare_function(self, loc, expr, &|v, f| v.hoist_annotations(f))
    }

    fn function_type(&mut self, ft: &ast::types::Function<Loc, Loc>) -> Result<(), !> {
        let with_types = self.with_types;
        function_type(self, with_types, ft, &|v, f| v.hoist_annotations(f))
    }

    fn component_type(&mut self, _loc: &Loc, t: &ast::types::Component<Loc, Loc>) -> Result<(), !> {
        let with_types = self.with_types;
        component_type(self, with_types, t, &|v, f| v.hoist_annotations(f))
    }

    fn object_mapped_type_property(
        &mut self,
        mapped_type: &ast::types::object::MappedType<Loc, Loc>,
    ) -> Result<(), !> {
        let with_types = self.with_types;
        object_mapped_type_property(self, with_types, mapped_type, &|v, f| {
            v.hoist_annotations(f)
        })
    }

    fn class_expression(&mut self, loc: &Loc, cls: &ast::class::Class<Loc, Loc>) -> Result<(), !> {
        class_expression(self, loc, cls, |this, loc, cls| {
            class_(this, this.with_types, loc, cls, |v, loc, id| {
                v.class_identifier_opt(loc, id)
            })
        })
    }

    fn class_(&mut self, loc: &Loc, cls: &ast::class::Class<Loc, Loc>) -> Result<(), !> {
        let with_types = self.with_types;
        class_(self, with_types, loc, cls, |v, loc, id| {
            v.class_identifier_opt(loc, id)
        })
    }

    fn declare_class(
        &mut self,
        _loc: &Loc,
        decl: &ast::statement::DeclareClass<Loc, Loc>,
    ) -> Result<(), !> {
        let with_types = self.with_types;
        declare_class(self, with_types, decl, &|v, f| v.hoist_annotations(f))
    }

    fn record_declaration(
        &mut self,
        _loc: &Loc,
        record: &ast::statement::RecordDeclaration<Loc, Loc>,
    ) -> Result<(), !> {
        let with_types = self.with_types;
        record_declaration(self, with_types, record, &|v, f| v.hoist_annotations(f))
    }

    fn enum_declaration(
        &mut self,
        loc: &Loc,
        enum_decl: &ast::statement::EnumDeclaration<Loc, Loc>,
    ) -> Result<(), !> {
        if !self.enable_enums {
            return Ok(());
        }
        ast_visitor::enum_declaration_default(self, loc, enum_decl)
    }
}

pub fn program<Loc: Dupe + Eq + Ord + Hash + Default>(
    enable_enums: bool,
    with_types: bool,
    allow_declare_global_captures: bool,
    program: &ast::Program<Loc, Loc>,
) -> ScopeInfo<Loc> {
    let loc = &program.loc;
    let mut walk = ScopeBuilder::new(enable_enums, with_types, allow_declare_global_captures);
    let bindings = {
        let mut hoist = Hoister::new(enable_enums, with_types);
        let Ok(()) = hoist.program(program);
        hoist.into_bindings()
    };
    let Ok(()) = walk.with_bindings(false, loc.dupe(), bindings, |walk| walk.program(program));
    walk.into_info()
}
