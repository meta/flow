/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::ops::Deref;
use std::sync::Arc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_aloc::ALocSet;
use flow_common::reason::VirtualReason;
use flow_common_tarjan::topsort;
use flow_common_utils::graph::Graph;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_data_structure_wrapper::vector::FlowVector;
use flow_env_builder::env_api;
use flow_env_builder::env_api::AnnotLoc;
use flow_env_builder::env_api::AutocompleteHooks;
use flow_env_builder::env_api::DefLocType;
use flow_env_builder::env_api::EnvEntry;
use flow_env_builder::env_api::EnvInfo;
use flow_env_builder::env_api::EnvInvariantFailure;
use flow_env_builder::env_api::EnvKey;
use flow_env_builder::env_api::EnvMap;
use flow_env_builder::env_api::EnvSet;
use flow_env_builder::env_api::Refinement;
use flow_env_builder::env_api::RefinementKind;
use flow_env_builder::env_api::Values;
use flow_env_builder::env_api::WriteLoc;
use flow_env_builder::name_def::pattern_has_annot;
use flow_env_builder::name_def_types::AnnotationData;
use flow_env_builder::name_def_types::AstHint;
use flow_env_builder::name_def_types::Binding;
use flow_env_builder::name_def_types::ClassDefData;
use flow_env_builder::name_def_types::ComponentDefData;
use flow_env_builder::name_def_types::ContextualData;
use flow_env_builder::name_def_types::DeclaredClassDefData;
use flow_env_builder::name_def_types::DeclaredFunctionDefData;
use flow_env_builder::name_def_types::Def;
use flow_env_builder::name_def_types::DefinitionReferenceKind;
use flow_env_builder::name_def_types::EmptyArrayData;
use flow_env_builder::name_def_types::ExpressionDef;
use flow_env_builder::name_def_types::FunctionDefData;
use flow_env_builder::name_def_types::FunctionSynthKind;
use flow_env_builder::name_def_types::FunctionValueData;
use flow_env_builder::name_def_types::ImportData;
use flow_env_builder::name_def_types::MatchCasePatternData;
use flow_env_builder::name_def_types::MatchCaseRootData;
use flow_env_builder::name_def_types::MemberAssignData;
use flow_env_builder::name_def_types::ObjectValueData;
use flow_env_builder::name_def_types::OpAssignData;
use flow_env_builder::name_def_types::RecordDefData;
use flow_env_builder::name_def_types::Root;
use flow_env_builder::name_def_types::TypeParamData;
use flow_env_builder::name_def_types::print::string_of_source;
use flow_env_builder::provider_api;
use flow_env_builder::selector::Selector;
use flow_parser::ast;
use flow_parser::ast::expression::ExpressionInner;
use flow_parser::ast::function::Function;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::ast_visitor::TypeParamsContext;
use flow_parser::loc_sig::LocSig;
use flow_typing_errors::error_message::DefinitionReferenceData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::InternalError;
use vec1::Vec1;

use crate::dependency_sigs::Context;
use crate::dependency_sigs::Flow;

#[derive(Debug, Clone)]
pub struct Blame<K> {
    pub payload: K,
    pub definition: DefinitionReferenceData<ALoc>,
    pub annot_locs: Vec<AnnotLoc<ALoc>>,
    pub recursion: Vec<ALoc>,
}

#[derive(Debug, Clone)]
pub enum Element {
    Normal(EnvKey<ALoc>),
    Resolvable(EnvKey<ALoc>),
    Illegal(Blame<EnvKey<ALoc>>),
}

type DeclareNamespaceReadPaths<'a> =
    &'a FlowOrdMap<ALoc, FlowVector<env_api::DeclareNamespaceReadPathElement<ALoc>>>;

fn string_of_element<A: Clone, B: Clone>(
    graph: &EnvMap<ALoc, (Def, A, B, VirtualReason<ALoc>, DefinitionReferenceKind)>,
    element: &Element,
) -> String {
    let print_elt = |key: &EnvKey<ALoc>| -> String {
        match graph.get(key) {
            None => "MISSING DEFINITION".to_string(),
            Some((def, _, _, _, _)) => string_of_source(def),
        }
    };

    match element {
        Element::Normal(EnvKey { def_loc_type, loc }) => {
            format!(
                "[{} ({}): {}]",
                loc.debug_to_string(false),
                def_loc_type.show(),
                print_elt(&EnvKey::new(*def_loc_type, loc.dupe()))
            )
        }
        Element::Resolvable(EnvKey { def_loc_type, loc }) => {
            format!(
                "[recursive {} ({}): {}]",
                loc.debug_to_string(false),
                def_loc_type.show(),
                print_elt(&EnvKey::new(*def_loc_type, loc.dupe()))
            )
        }
        Element::Illegal(Blame {
            payload: EnvKey { def_loc_type, loc },
            ..
        }) => {
            format!(
                "[illegal {} ({}): {}]",
                loc.debug_to_string(false),
                def_loc_type.show(),
                print_elt(&EnvKey::new(*def_loc_type, loc.dupe()))
            )
        }
    }
}

#[derive(Debug, Clone)]
pub enum OrderingResult {
    Singleton(Element),
    ResolvableSCC(Vec1<Element>),
    IllegalSCC(Vec1<(Blame<Element>, bool)>),
}

pub fn string_of_component<A: Clone, B: Clone>(
    graph: &EnvMap<ALoc, (Def, A, B, VirtualReason<ALoc>, DefinitionReferenceKind)>,
    result: &OrderingResult,
) -> String {
    match result {
        OrderingResult::Singleton(elt) => string_of_element(graph, elt),
        OrderingResult::ResolvableSCC(elts) => {
            let elements_str: Vec<String> = elts
                .iter()
                .map(|elt| string_of_element(graph, elt))
                .collect();
            format!("{{(recursive cycle)\n{}\n}}", elements_str.join(",\n"))
        }
        OrderingResult::IllegalSCC(elts) => {
            let elements_str: Vec<String> = elts
                .iter()
                .map(|(Blame { payload: elt, .. }, _)| string_of_element(graph, elt))
                .collect();
            format!("{{(illegal cycle)\n{}\n}}", elements_str.join(",\n"))
        }
    }
}

#[derive(Default)]
struct ToplevelExpressionCollector<'ast> {
    acc: Vec<&'ast ast::expression::Expression<ALoc, ALoc>>,
    element_names: Vec<&'ast ast::jsx::Identifier<ALoc, ALoc>>,
}

impl<'ast> AstVisitor<'ast, ALoc> for ToplevelExpressionCollector<'ast> {
    fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
        loc
    }

    fn normalize_type(type_: &'ast ALoc) -> &'ast ALoc {
        type_
    }

    fn expression(&mut self, expr: &'ast ast::expression::Expression<ALoc, ALoc>) -> Result<(), !> {
        self.acc.push(expr);
        Ok(())
    }

    /// A nested element is a child, not an expression, so `expression` above
    /// never sees its name. The name is still a read, and synthesizing the
    /// enclosing element synthesizes the child too, so it is collected here.
    fn jsx_element_name_identifier(
        &mut self,
        ident: &'ast ast::jsx::Identifier<ALoc, ALoc>,
    ) -> Result<(), !> {
        self.element_names.push(ident);
        Ok(())
    }

    fn class_(
        &mut self,
        _loc: &'ast ALoc,
        _cls: &'ast ast::class::Class<ALoc, ALoc>,
    ) -> Result<(), !> {
        Ok(())
    }

    fn record_declaration(
        &mut self,
        _loc: &'ast ALoc,
        _decl: &'ast ast::statement::RecordDeclaration<ALoc, ALoc>,
    ) -> Result<(), !> {
        Ok(())
    }

    fn function_declaration(
        &mut self,
        _loc: &'ast ALoc,
        _decl: &'ast Function<ALoc, ALoc>,
    ) -> Result<(), !> {
        Ok(())
    }

    fn function_expression(
        &mut self,
        _loc: &'ast ALoc,
        _expr: &'ast Function<ALoc, ALoc>,
    ) -> Result<(), !> {
        Ok(())
    }

    fn arrow_function(
        &mut self,
        _loc: &'ast ALoc,
        _expr: &'ast Function<ALoc, ALoc>,
    ) -> Result<(), !> {
        Ok(())
    }
}

// Helper class for the dependency analysis--traverse the AST nodes
// in a def to determine which variables appear
struct UseVisitor<'a, 'cx, Cx, Fl>
where
    Cx: Context,
    Fl: Flow<Cx = Cx>,
{
    cx: &'cx mut Cx,
    named_only_for_synthesis: bool,
    this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
    env_values: &'a Values<ALoc>,
    env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
    providers: &'a provider_api::Info<ALoc>,
    declare_namespace_read_paths:
        &'a FlowOrdMap<ALoc, FlowVector<env_api::DeclareNamespaceReadPathElement<ALoc>>>,
    refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
    seen: ALocSet,
    acc: EnvMap<ALoc, Vec1<ALoc>>,
    _phantom: std::marker::PhantomData<Fl>,
}

impl<'a, 'cx, Cx, Fl> UseVisitor<'a, 'cx, Cx, Fl>
where
    Cx: Context,
    Fl: Flow<Cx = Cx>,
{
    pub fn new(
        cx: &'cx mut Cx,
        named_only_for_synthesis: bool,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        init: EnvMap<ALoc, Vec1<ALoc>>,
    ) -> Self {
        Self {
            cx,
            named_only_for_synthesis,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            seen: ALocSet::new(),
            acc: init,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn into_acc(self) -> EnvMap<ALoc, Vec1<ALoc>> {
        self.acc
    }

    pub fn add(&mut self, why: ALoc, t: EnvKey<ALoc>) {
        if env_api::has_assigning_write(t.dupe(), self.env_entries) {
            self.acc
                .entry(t)
                .and_modify(|locs| {
                    if !locs.contains(&why) {
                        locs.push(why.dupe());
                    }
                })
                .or_insert_with(|| Vec1::new(why));
        }
    }

    fn add_declare_namespace_prefix_dependencies(&mut self, read_loc: &ALoc) {
        let Some(namespace_path) = self.declare_namespace_read_paths.get(read_loc) else {
            return;
        };
        for namespace in namespace_path.iter().rev() {
            for loc in namespace.locs.iter() {
                self.add(
                    read_loc.dupe(),
                    EnvKey::new(DefLocType::OrdinaryNameLoc, loc.dupe()),
                );
            }
        }
    }

    pub fn add_write_locs(
        &mut self,
        for_type: bool,
        write_locs: &[WriteLoc<ALoc>],
    ) -> Vec<EnvKey<ALoc>> {
        let writes: Vec<EnvKey<ALoc>> = write_locs
            .iter()
            .flat_map(|wl| env_api::writes_of_write_loc(for_type, self.providers, wl))
            .map(|kind_and_loc| {
                self.this_super_dep_loc_map
                    .get(&kind_and_loc)
                    .cloned()
                    .unwrap_or(kind_and_loc)
            })
            .collect();
        let refinements: Vec<RefinementKind<ALoc>> = write_locs
            .iter()
            .flat_map(|wl| env_api::refinements_of_write_loc(self.refinement_of_id, wl))
            .collect();

        fn writes_of_refinement<Cx: Context, Fl: Flow<Cx = Cx>>(
            this: &mut UseVisitor<'_, '_, Cx, Fl>,
            refi: &RefinementKind<ALoc>,
        ) {
            match refi {
                RefinementKind::InstanceOfR { expr, context: _ } => {
                    let Ok(()) = this.expression(expr.as_ref());
                }
                RefinementKind::LatentR {
                    func,
                    targs,
                    arguments,
                    index: _,
                }
                | RefinementKind::AssertionR {
                    func,
                    targs,
                    arguments,
                    target: _,
                } => {
                    let Ok(()) = this.expression(func.as_ref());
                    if let Some(targs) = targs {
                        let Ok(()) = this.call_type_args(targs.as_ref());
                    }
                    let Ok(()) = this.arg_list(arguments.as_ref());
                }
                RefinementKind::LatentThisR {
                    func,
                    targs,
                    arguments,
                } => {
                    let Ok(()) = this.expression(func.as_ref());
                    if let Some(targs) = targs {
                        let Ok(()) = this.call_type_args(targs.as_ref());
                    }
                    let Ok(()) = this.arg_list(arguments.as_ref());
                }
                RefinementKind::SentinelR { prop: _, other_loc } => {
                    this.add(
                        other_loc.dupe(),
                        EnvKey::new(DefLocType::ExpressionLoc, other_loc.dupe()),
                    );
                }
                RefinementKind::EqR(loc) => {
                    this.add(
                        loc.dupe(),
                        EnvKey::new(DefLocType::ExpressionLoc, loc.dupe()),
                    );
                }
                RefinementKind::AndR(l, r) | RefinementKind::OrR(l, r) => {
                    writes_of_refinement(this, l);
                    writes_of_refinement(this, r);
                }
                RefinementKind::NotR(r) => {
                    writes_of_refinement(this, r);
                }
                RefinementKind::TruthyR
                | RefinementKind::NullR
                | RefinementKind::UndefinedR
                | RefinementKind::MaybeR
                | RefinementKind::IsArrayR
                | RefinementKind::ArrLenR { .. }
                | RefinementKind::BoolR(_)
                | RefinementKind::FunctionR
                | RefinementKind::NumberR(_)
                | RefinementKind::BigIntR(_)
                | RefinementKind::ObjectR
                | RefinementKind::StringR(_)
                | RefinementKind::SymbolR(_)
                | RefinementKind::SingletonBoolR { .. }
                | RefinementKind::SingletonStrR { .. }
                | RefinementKind::SingletonNumR { .. }
                | RefinementKind::SingletonBigIntR { .. }
                | RefinementKind::PropExistsR { .. }
                | RefinementKind::PropNullishR { .. }
                | RefinementKind::PropIsExactlyNullR { .. }
                | RefinementKind::PropNonVoidR { .. }
                | RefinementKind::PropTruthyR { .. }
                | RefinementKind::ImpossibleR => {}
            }
        }
        for refi in &refinements {
            writes_of_refinement(self, refi);
        }
        writes
    }

    pub fn find_writes(
        &mut self,
        for_type: bool,
        allow_missing: bool,
        loc: &ALoc,
    ) -> Vec<EnvKey<ALoc>> {
        let write_locs = match self.env_values.get_ordinary(loc) {
            Some(read) => read.write_locs.as_slice(),
            None => {
                if !allow_missing {
                    Fl::add_output(
                        self.cx,
                        ErrorMessage::EInternal(Box::new((
                            loc.dupe(),
                            InternalError::MissingEnvRead(loc.dupe()),
                        ))),
                    );
                }
                &[]
            }
        };
        self.add_write_locs(for_type, write_locs)
    }

    fn jsx_function_call(&mut self, loc: &ALoc) {
        use flow_common::options::JsxMode;
        use flow_common::options::ReactRuntime;

        match (self.cx.react_runtime(), self.cx.jsx()) {
            (ReactRuntime::Classic, JsxMode::JsxReact) => {
                let writes = self.find_writes(false, false, loc);
                for t in writes {
                    self.add(loc.dupe(), t);
                }
            }
            (_, JsxMode::JsxPragma(_, ref expr))
                if matches!(expr.deref(), ExpressionInner::Identifier { .. }) =>
            {
                let writes = self.find_writes(false, false, loc);
                for t in writes {
                    self.add(loc.dupe(), t);
                }
            }
            (ReactRuntime::Classic, JsxMode::JsxPragma(_, ref expr)) => {
                let Ok(()) = self.expression(expr);
            }
            _ => {}
        }
    }

    fn component_ref_param_maybe(
        &mut self,
        param: &ast::statement::component_params::Param<ALoc, ALoc>,
    ) {
        use flow_common::options::JsxMode;
        use flow_common::options::ReactRuntime;

        let name = &param.name;
        match (name, self.cx.react_runtime(), self.cx.jsx()) {
            (
                ast::statement::component_params::ParamName::Identifier(id),
                ReactRuntime::Classic,
                JsxMode::JsxReact,
            ) if id.name.as_str() == "ref" => {
                let loc = &id.loc;
                let writes = self.find_writes(false, false, loc);
                for t in writes {
                    self.add(loc.dupe(), t);
                }
            }
            (
                ast::statement::component_params::ParamName::StringLiteral((
                    loc,
                    ast::StringLiteral { value, .. },
                )),
                ReactRuntime::Classic,
                JsxMode::JsxReact,
            ) if value.as_str() == "ref" => {
                let writes = self.find_writes(false, false, loc);
                for t in writes {
                    self.add(loc.dupe(), t);
                }
            }
            _ => {}
        }
    }

    // For classes/functions that are known to be fully annotated, we skip property bodies
    fn function_def(&mut self, fully_annotated: bool, expr: &Function<ALoc, ALoc>) {
        let params = &expr.params;
        let body = &expr.body;
        let predicate = &expr.predicate;
        let return_ = &expr.return_;
        let tparams = &expr.tparams;
        let Ok(()) = self.function_return_annotation(return_);
        if fully_annotated {
            self.function_params_annotated(params);
        } else {
            let Ok(()) = self.function_body_any(body);
            let Ok(()) = self.function_params(params);
            if let Some(predicate) = predicate {
                let Ok(()) = self.predicate(predicate);
            }
        }
        if let Some(tparams) = tparams {
            let Ok(()) = self.type_params(&TypeParamsContext::Function, tparams);
        }
    }

    fn component_def(&mut self, expr: &'a ast::statement::ComponentDeclaration<ALoc, ALoc>) {
        let params = &expr.params;
        let renders = &expr.renders;
        let tparams = &expr.tparams;
        let Ok(()) = self.component_renders_annotation(renders);
        self.component_params_annotated(params);
        if let Some(tparams) = tparams {
            let Ok(()) = self.type_params(&TypeParamsContext::ComponentDeclaration, tparams);
        }
    }

    fn declare_component_params(
        &mut self,
        params: &ast::statement::component_params::Params<ALoc, ALoc>,
    ) {
        let params_list = &params.params;
        let rest = &params.rest;
        for param in params_list.iter() {
            self.declare_component_param(param);
        }
        if let Some(rest) = rest {
            self.declare_component_rest_param(rest);
        }
    }

    fn declare_component_param(
        &mut self,
        param: &ast::statement::component_params::Param<ALoc, ALoc>,
    ) {
        let local = &param.local;
        match local {
            ast::pattern::Pattern::Identifier { inner, .. } => {
                let Ok(()) = self.type_annotation_hint(&inner.annot);
            }
            ast::pattern::Pattern::Object { inner, .. } => {
                let Ok(()) = self.type_annotation_hint(&inner.annot);
            }
            ast::pattern::Pattern::Array { inner, .. } => {
                let Ok(()) = self.type_annotation_hint(&inner.annot);
            }
            ast::pattern::Pattern::Expression { .. } => {}
        }
    }

    fn declare_component_rest_param(
        &mut self,
        param: &ast::statement::component_params::RestParam<ALoc, ALoc>,
    ) {
        let argument = &param.argument;
        match argument {
            ast::pattern::Pattern::Identifier { inner, .. } => {
                let Ok(()) = self.type_annotation_hint(&inner.annot);
            }
            ast::pattern::Pattern::Object { inner, .. } => {
                let Ok(()) = self.type_annotation_hint(&inner.annot);
            }
            ast::pattern::Pattern::Array { inner, .. } => {
                let Ok(()) = self.type_annotation_hint(&inner.annot);
            }
            ast::pattern::Pattern::Expression { .. } => {}
        }
    }

    fn function_param_pattern_annotated(&mut self, expr: &ast::pattern::Pattern<ALoc, ALoc>) {
        match expr {
            ast::pattern::Pattern::Object { inner, .. } => {
                for prop in inner.properties.iter() {
                    self.function_param_pattern_object_p_annotated(prop);
                }
                let Ok(()) = self.type_annotation_hint(&inner.annot);
            }
            ast::pattern::Pattern::Array { inner, .. } => {
                for elem in inner.elements.iter() {
                    self.function_param_pattern_array_e_annotated(elem);
                }
                let Ok(()) = self.type_annotation_hint(&inner.annot);
            }
            ast::pattern::Pattern::Identifier { inner, .. } => {
                let Ok(()) = self.pattern_identifier(Some(ast::VariableKind::Const), &inner.name);
                let Ok(()) = self.type_annotation_hint(&inner.annot);
            }
            ast::pattern::Pattern::Expression { inner, .. } => {
                let Ok(()) = self.pattern_expression(inner);
            }
        }
    }

    fn visit_annotation_in_pattern(&mut self, expr: &ast::pattern::Pattern<ALoc, ALoc>) {
        match expr {
            ast::pattern::Pattern::Object { inner, .. } => {
                let Ok(()) = self.type_annotation_hint(&inner.annot);
            }
            ast::pattern::Pattern::Array { inner, .. } => {
                let Ok(()) = self.type_annotation_hint(&inner.annot);
            }
            ast::pattern::Pattern::Identifier { inner, .. } => {
                let Ok(()) = self.type_annotation_hint(&inner.annot);
            }
            ast::pattern::Pattern::Expression { .. } => {}
        }
    }

    fn function_param_pattern_object_p_annotated(
        &mut self,
        p: &ast::pattern::object::Property<ALoc, ALoc>,
    ) {
        match p {
            ast::pattern::object::Property::NormalProperty(prop) => {
                self.function_param_pattern_object_property_annotated(prop);
            }
            ast::pattern::object::Property::RestElement(prop) => {
                self.function_param_pattern_object_rest_property_annotated(prop);
            }
        }
    }

    fn function_param_pattern_object_property_annotated(
        &mut self,
        prop: &ast::pattern::object::NormalProperty<ALoc, ALoc>,
    ) {
        let key = &prop.key;
        let pattern = &prop.pattern;
        let Ok(()) = self.pattern_object_property_key(None, key);
        self.function_param_pattern_annotated(pattern);
        // Skip default
    }

    fn function_param_pattern_object_rest_property_annotated(
        &mut self,
        prop: &ast::pattern::RestElement<ALoc, ALoc>,
    ) {
        let argument = &prop.argument;
        self.function_param_pattern_annotated(argument);
    }

    fn function_param_pattern_array_e_annotated(
        &mut self,
        e: &ast::pattern::array::Element<ALoc, ALoc>,
    ) {
        match e {
            ast::pattern::array::Element::Hole(_) => {}
            ast::pattern::array::Element::NormalElement(elem) => {
                self.function_param_pattern_array_element_annotated(elem);
            }
            ast::pattern::array::Element::RestElement(elem) => {
                self.function_param_pattern_array_rest_element_annotated(elem);
            }
        }
    }

    fn function_param_pattern_array_element_annotated(
        &mut self,
        elem: &ast::pattern::array::NormalElement<ALoc, ALoc>,
    ) {
        let argument = &elem.argument;
        self.function_param_pattern_annotated(argument);
        // Skip default
    }

    fn function_param_pattern_array_rest_element_annotated(
        &mut self,
        elem: &ast::pattern::RestElement<ALoc, ALoc>,
    ) {
        let argument = &elem.argument;
        self.function_param_pattern_annotated(argument);
    }

    fn component_params_annotated(
        &mut self,
        params: &ast::statement::component_params::Params<ALoc, ALoc>,
    ) {
        let params_list = &params.params;
        let rest = &params.rest;
        for param in params_list.iter() {
            self.component_param_annotated(param);
        }
        if let Some(rest) = rest {
            self.component_rest_param_annotated(rest);
        }
    }

    fn component_param_annotated(
        &mut self,
        param: &ast::statement::component_params::Param<ALoc, ALoc>,
    ) {
        let local = &param.local;
        self.function_param_pattern_annotated(local);
        self.component_ref_param_maybe(param);
    }

    fn component_rest_param_annotated(
        &mut self,
        param: &ast::statement::component_params::RestParam<ALoc, ALoc>,
    ) {
        let argument = &param.argument;
        self.function_param_pattern_annotated(argument);
    }

    fn function_params_annotated(&mut self, params: &ast::function::Params<ALoc, ALoc>) {
        let params_list = &params.params;
        let rest = &params.rest;
        let this_ = &params.this_;
        for param in params_list.iter() {
            self.function_param_annotated(param);
        }
        if let Some(rest) = rest {
            self.function_rest_param_annotated(rest);
        }
        if let Some(this_) = this_ {
            let Ok(()) = self.function_this_param(this_);
        }
    }

    fn function_param_annotated(&mut self, param: &ast::function::Param<ALoc, ALoc>) {
        let (loc, argument) = match param {
            ast::function::Param::RegularParam { loc, argument, .. } => (loc, argument),
            ast::function::Param::ParamProperty { .. } => {
                // Skip parameter properties
                return;
            }
        };
        // Skip default
        self.function_param_pattern_annotated(argument);
        if !flow_parser::ast_utils::pattern_has_binding(argument) && !pattern_has_annot(argument) {
            self.add(
                loc.dupe(),
                EnvKey::new(DefLocType::FunctionParamLoc, loc.dupe()),
            );
        }
    }

    fn function_rest_param_annotated(&mut self, expr: &ast::function::RestParam<ALoc, ALoc>) {
        let argument = &expr.argument;
        self.function_param_pattern_annotated(argument);
    }

    fn class_extends_sig(&mut self, extends: &ast::class::Extends<ALoc, ALoc>) {
        if let Some(targs) = &extends.targs {
            let Ok(()) = self.type_args(targs);
        }
        // We only visit expressions that is used to generate the class signature.
        fn visit_expr_for_sig<'a, 'cx, Cx, Fl>(
            this: &mut UseVisitor<'a, 'cx, Cx, Fl>,
            expr: &ast::expression::Expression<ALoc, ALoc>,
        ) where
            Cx: Context,
            Fl: Flow<Cx = Cx>,
        {
            match expr.deref() {
                ExpressionInner::Identifier { .. } => {
                    let Ok(()) = this.expression(expr);
                }
                ExpressionInner::Member { inner, .. } => {
                    if matches!(
                        inner.property,
                        ast::expression::member::Property::PropertyIdentifier(_)
                    ) {
                        visit_expr_for_sig(this, &inner.object);
                    }
                }
                ExpressionInner::AsExpression { inner, .. } => {
                    let Ok(()) = this.type_annotation(&inner.annot);
                }
                ExpressionInner::TypeCast { inner, .. } => {
                    let Ok(()) = this.type_annotation(&inner.annot);
                }
                _ => {}
            }
        }
        visit_expr_for_sig(self, &extends.expr);
    }

    fn class_body_annotated(&mut self, cls_body: &ast::class::Body<ALoc, ALoc>) {
        let body = &cls_body.body;
        for elem in body.iter() {
            self.class_element_annotated(elem);
        }
    }

    fn class_element_annotated(&mut self, elem: &ast::class::BodyElement<ALoc, ALoc>) {
        match elem {
            ast::class::BodyElement::Method(meth) => {
                self.class_method_annotated(meth);
            }
            ast::class::BodyElement::Property(prop) => {
                self.class_property_annotated(prop);
            }
            ast::class::BodyElement::PrivateField(field) => {
                self.class_private_field_annotated(field);
            }
            ast::class::BodyElement::StaticBlock(_) => {}
            ast::class::BodyElement::DeclareMethod(ast::class::DeclareMethod {
                key,
                annot,
                ..
            }) => {
                // Even though DeclareMethod has no runtime def, its annotation
                // references types whose env entries must be resolved before this
                // class's body is checked. Without tracking these as dependencies,
                // typing.ml's [mk_declare_method_func_sig] (statement.ml) will read
                // unresolved env entries and crash with [ReadOfUnreachedTvar].
                let Ok(()) = self.object_key(key);
                let Ok(()) = self.type_annotation(annot);
            }
            ast::class::BodyElement::AbstractMethod(meth) => {
                let Ok(()) = self.class_abstract_method(meth);
            }
            ast::class::BodyElement::AbstractProperty(prop) => {
                let Ok(()) = self.class_abstract_property(prop);
            }
            ast::class::BodyElement::IndexSignature(indexer) => {
                let Ok(()) = self.class_indexer(indexer);
            }
        }
    }

    fn class_method_annotated(&mut self, meth: &ast::class::Method<ALoc, ALoc>) {
        let key = &meth.key;
        let value = &meth.value.1;
        let decorators = &meth.decorators;
        for decorator in decorators.iter() {
            let Ok(()) = self.class_decorator(decorator);
        }
        let Ok(()) = self.object_key(key);
        self.function_def(true, value);
    }

    fn class_property_value_annotated(&mut self, value: &ast::class::property::Value<ALoc, ALoc>) {
        match value {
            ast::class::property::Value::Initialized(expr) => match expr.deref() {
                ExpressionInner::ArrowFunction { inner, .. } => {
                    self.function_def(true, inner);
                }
                ExpressionInner::Function { inner, .. } => {
                    self.function_def(true, inner.as_ref());
                }
                _ => {}
            },
            ast::class::property::Value::Declared => {}
            ast::class::property::Value::Uninitialized => {}
        }
    }

    fn class_property_annotated(&mut self, prop: &ast::class::Property<ALoc, ALoc>) {
        let key = &prop.key;
        let value = &prop.value;
        let annot = &prop.annot;
        let decorators = &prop.decorators;
        for decorator in decorators.iter() {
            let Ok(()) = self.class_decorator(decorator);
        }
        let Ok(()) = self.object_key(key);
        let Ok(()) = self.type_annotation_hint(annot);
        match annot {
            ast::types::AnnotationOrHint::Missing(_) => {
                self.class_property_value_annotated(value);
            }
            ast::types::AnnotationOrHint::Available(_) => {}
        }
    }

    fn class_private_field_annotated(&mut self, prop: &ast::class::PrivateField<ALoc, ALoc>) {
        let key = &prop.key;
        let value = &prop.value;
        let annot = &prop.annot;
        let decorators = &prop.decorators;
        for decorator in decorators.iter() {
            let Ok(()) = self.class_decorator(decorator);
        }
        let Ok(()) = self.private_name(key);
        let Ok(()) = self.type_annotation_hint(annot);
        match annot {
            ast::types::AnnotationOrHint::Missing(_) => {
                self.class_property_value_annotated(value);
            }
            ast::types::AnnotationOrHint::Available(_) => {}
        }
    }

    fn record_body_annotated(
        &mut self,
        rec_body: &ast::statement::record_declaration::Body<ALoc, ALoc>,
    ) {
        let body = &rec_body.body;
        for elem in body.iter() {
            self.record_element_annotated(elem);
        }
    }

    fn record_element_annotated(
        &mut self,
        elem: &ast::statement::record_declaration::BodyElement<ALoc, ALoc>,
    ) {
        match elem {
            ast::statement::record_declaration::BodyElement::Method(meth) => {
                self.class_method_annotated(meth);
            }
            ast::statement::record_declaration::BodyElement::Property(prop) => {
                self.record_property_annotated(prop);
            }
            ast::statement::record_declaration::BodyElement::StaticProperty(prop) => {
                self.record_static_property_annotated(prop);
            }
        }
    }

    fn record_property_annotated(
        &mut self,
        prop: &ast::statement::record_declaration::Property<ALoc, ALoc>,
    ) {
        let annot = &prop.annot;
        let default_value = &prop.default_value;
        let Ok(()) = self.type_annotation(annot);
        if let Some(value) = default_value {
            match value.deref() {
                ExpressionInner::ArrowFunction { inner, .. } => {
                    self.function_def(true, inner);
                }
                ExpressionInner::Function { inner, .. } => {
                    self.function_def(true, inner.as_ref());
                }
                _ => {}
            }
        }
    }

    fn record_static_property_annotated(
        &mut self,
        prop: &ast::statement::record_declaration::StaticProperty<ALoc, ALoc>,
    ) {
        let annot = &prop.annot;
        let value = &prop.value;
        let Ok(()) = self.type_annotation(annot);
        match value.deref() {
            ExpressionInner::ArrowFunction { inner, .. } => {
                self.function_def(true, inner);
            }
            ExpressionInner::Function { inner, .. } => {
                self.function_def(true, inner.as_ref());
            }
            _ => {}
        }
    }

    fn visit_expression_for_expression_writes(
        &mut self,
        expr: &ast::expression::Expression<ALoc, ALoc>,
    ) {
        let loc = expr.loc();
        let writes = self.find_writes(false, true, loc);
        for t in writes {
            self.add(loc.dupe(), t);
        }
        let Ok(()) = flow_parser::ast_visitor::expression_default(self, expr);
    }
}

impl<'ast, 'a, 'cx, Cx, Fl> AstVisitor<'ast, ALoc> for UseVisitor<'a, 'cx, Cx, Fl>
where
    Cx: Context,
    Fl: Flow<Cx = Cx>,
{
    fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
        loc
    }

    fn normalize_type(type_: &'ast ALoc) -> &'ast ALoc {
        type_
    }

    // In order to resolve a def containing a variable read, the writes that the
    // name_resolver determines reach the variable must be resolved
    fn identifier(&mut self, id: &ast::Identifier<ALoc, ALoc>) -> Result<(), !> {
        let loc = &id.loc;
        let writes = self.find_writes(false, false, loc);
        for t in writes {
            self.add(loc.dupe(), t);
        }
        self.add_declare_namespace_prefix_dependencies(loc);
        Ok(())
    }

    fn type_identifier_reference(&mut self, id: &ast::Identifier<ALoc, ALoc>) -> Result<(), !> {
        let loc = &id.loc;
        let writes = self.find_writes(true, false, loc);
        for t in writes {
            self.add(loc.dupe(), t);
        }
        self.add_declare_namespace_prefix_dependencies(loc);
        Ok(())
    }

    // In order to resolve a def containing a variable read, the writes that the
    // name_resolver determines reach the variable must be resolved
    fn yield_(&mut self, loc: &ALoc, expr: &ast::expression::Yield<ALoc, ALoc>) -> Result<(), !> {
        let writes = self.find_writes(false, false, loc);
        for t in writes {
            self.add(loc.dupe(), t);
        }
        flow_parser::ast_visitor::yield_default(self, loc, expr)
    }

    fn match_<B>(
        &mut self,
        _loc: &'ast ALoc,
        m: &'ast ast::match_::Match<ALoc, ALoc, B>,
        mut on_case_body: impl FnMut(&mut Self, &'ast B) -> Result<(), !>,
    ) -> Result<(), !> {
        let ast::match_::Match {
            arg,
            cases,
            match_keyword_loc,
            comments: _,
        } = m;
        self.expression(arg)?;
        let match_root_ident = flow_parser::ast_utils::match_root_ident(match_keyword_loc.dupe());
        self.pattern_identifier(Some(ast::VariableKind::Const), &match_root_ident)?;
        for case in cases.iter() {
            let case_match_root_loc = &case.case_match_root_loc;
            let case_root_ident =
                flow_parser::ast_utils::match_root_ident(case_match_root_loc.dupe());
            self.identifier(&case_root_ident)?;
            flow_parser::ast_visitor::match_case_default(self, case, &mut on_case_body)?;
        }
        self.identifier(&match_root_ident)?;
        Ok(())
    }

    // In order to resolve a def containing a variable write, the
    // write itself should first be resolved
    fn pattern_identifier(
        &mut self,
        _kind: Option<ast::VariableKind>,
        ident: &ast::Identifier<ALoc, ALoc>,
    ) -> Result<(), !> {
        let loc = &ident.loc;
        if self.providers.is_provider(loc) {
            let def_providers = match self.providers.providers_of_def(loc) {
                Some(def_providers) => def_providers,
                None => {
                    Fl::add_output(
                        self.cx,
                        ErrorMessage::EInternal(Box::new((
                            loc.dupe(),
                            InternalError::EnvInvariant(EnvInvariantFailure::Impossible(
                                "providers_of_def should exist for provider".into(),
                            )),
                        ))),
                    );
                    return Ok(());
                }
            };
            for provider in &def_providers.providers {
                let r = &provider.reason;
                let key = EnvKey::new(DefLocType::OrdinaryNameLoc, r.loc().dupe());
                self.add(loc.dupe(), key);
            }
        }
        // Ignore cases that don't have bindings in the environment, like `var x;`
        // and illegal or unreachable writes.
        self.add(
            loc.dupe(),
            EnvKey::new(DefLocType::OrdinaryNameLoc, loc.dupe()),
        );
        Ok(())
    }

    fn binding_type_identifier(&mut self, id: &ast::Identifier<ALoc, ALoc>) -> Result<(), !> {
        let loc = &id.loc;
        self.add(
            loc.dupe(),
            EnvKey::new(DefLocType::OrdinaryNameLoc, loc.dupe()),
        );
        Ok(())
    }

    // The default mapper visits a `declare namespace X { ... }`'s id with
    // pattern_identifier, which would add the id as a use-site dep. The
    // namespace's id is a definition site, not a use, and a merged
    // namespace id (when paired with a class/function/interface) has no
    // emitted def, so adding it as a dep produces a NameDefOrderingFailure
    // that aborts type-env initialization. Skip the id and only walk the
    // body.
    fn declare_namespace(
        &mut self,
        _loc: &ALoc,
        namespace: &ast::statement::DeclareNamespace<ALoc, ALoc>,
    ) -> Result<(), !> {
        let Ok(()) = self.block(&namespace.body.0, &namespace.body.1);
        Ok(())
    }

    // The Identifier form of [import_equals_declaration] (e.g. `import X =
    // A.B.C` / `export import X = A.B.C`) is intentionally skipped by
    // [name_resolver] (see name_resolver.ml:5095-5105), so no env entry is
    // registered for the qualified-identifier reference on the RHS. The
    // default visitor would walk into [A.B.C] and call [find_writes] on
    // [A], producing a spurious [MissingEnvRead] InternalError. The LHS
    // [id] also has no env entry. Skip the whole node.
    fn import_equals_declaration(
        &mut self,
        loc: &ALoc,
        decl: &ast::statement::ImportEqualsDeclaration<ALoc, ALoc>,
    ) -> Result<(), !> {
        use ast::statement::import_equals_declaration::ModuleReference;
        match &decl.module_reference {
            ModuleReference::ExternalModuleReference(..) => {
                flow_parser::ast_visitor::import_equals_declaration_default(self, loc, decl)
            }
            ModuleReference::Identifier(..) => Ok(()),
        }
    }

    fn this_expression(
        &mut self,
        loc: &ALoc,
        _expr: &ast::expression::This<ALoc>,
    ) -> Result<(), !> {
        let writes = self.find_writes(false, false, loc);
        for t in writes {
            self.add(loc.dupe(), t);
        }
        Ok(())
    }

    fn super_expression(
        &mut self,
        loc: &ALoc,
        _expr: &ast::expression::Super<ALoc>,
    ) -> Result<(), !> {
        let writes = self.find_writes(false, false, loc);
        for t in writes {
            self.add(loc.dupe(), t);
        }
        Ok(())
    }

    fn jsx_element_name_identifier(
        &mut self,
        ident: &ast::jsx::Identifier<ALoc, ALoc>,
    ) -> Result<(), !> {
        let loc = &ident.loc;
        let writes = self.find_writes(false, false, loc);
        for t in writes {
            self.add(loc.dupe(), t);
        }
        flow_parser::ast_visitor::jsx_identifier_default(self, ident)
    }

    fn jsx_element(&mut self, loc: &ALoc, expr: &ast::jsx::Element<ALoc, ALoc>) -> Result<(), !> {
        let ast::jsx::Element {
            opening_element,
            closing_element,
            ..
        } = expr;
        let call_loc = match closing_element {
            None => &opening_element.loc,
            _ => loc,
        };
        self.jsx_function_call(call_loc);
        flow_parser::ast_visitor::jsx_element_default(self, loc, expr)
    }

    fn jsx_fragment(&mut self, loc: &ALoc, expr: &ast::jsx::Fragment<ALoc, ALoc>) -> Result<(), !> {
        self.jsx_function_call(loc);
        flow_parser::ast_visitor::jsx_fragment_default(self, loc, expr)
    }

    fn component_param(
        &mut self,
        param: &ast::statement::component_params::Param<ALoc, ALoc>,
    ) -> Result<(), !> {
        self.component_ref_param_maybe(param);
        flow_parser::ast_visitor::component_param_default(self, param)
    }

    // Skip names in function parameter types (e.g. declared functions).
    // Only visit the type annotation, not the binding identifiers.
    fn function_param_type_identifier(
        &mut self,
        _id: &ast::Identifier<ALoc, ALoc>,
    ) -> Result<(), !> {
        Ok(())
    }

    // Skip destructuring patterns in function type params (e.g. in
    // `declare function f({a}: T): R`), as they are not runtime bindings.
    // But still visit the type annotation so type references get registered.
    fn function_param_type_pattern(
        &mut self,
        patt: &ast::pattern::Pattern<ALoc, ALoc>,
    ) -> Result<(), !> {
        use ast::pattern::Pattern;
        let annot = match patt {
            Pattern::Object { inner, .. } => &inner.annot,
            Pattern::Array { inner, .. } => &inner.annot,
            Pattern::Identifier { inner, .. } => &inner.annot,
            Pattern::Expression { .. } => return Ok(()),
        };
        let Ok(()) = self.type_annotation_hint(annot);
        Ok(())
    }

    fn member_property_identifier(&mut self, _id: &ast::Identifier<ALoc, ALoc>) -> Result<(), !> {
        Ok(())
    }

    fn typeof_member_identifier(&mut self, _ident: &ast::Identifier<ALoc, ALoc>) -> Result<(), !> {
        Ok(())
    }

    fn member_type_identifier(&mut self, _id: &ast::Identifier<ALoc, ALoc>) -> Result<(), !> {
        Ok(())
    }

    fn pattern_object_property_identifier_key(
        &mut self,
        _kind: Option<ast::VariableKind>,
        _id: &ast::Identifier<ALoc, ALoc>,
    ) -> Result<(), !> {
        Ok(())
    }

    fn match_object_pattern_property(
        &mut self,
        _key: &ast::match_pattern::object_pattern::Property<ALoc, ALoc>,
    ) -> Result<(), !> {
        Ok(())
    }

    fn enum_member_identifier(&mut self, _id: &ast::Identifier<ALoc, ALoc>) -> Result<(), !> {
        Ok(())
    }

    fn object_key_identifier(&mut self, _id: &ast::Identifier<ALoc, ALoc>) -> Result<(), !> {
        Ok(())
    }

    fn remote_identifier(&mut self, _ident: &ast::Identifier<ALoc, ALoc>) -> Result<(), !> {
        Ok(())
    }

    fn component_param_name(
        &mut self,
        _name: &ast::statement::component_params::ParamName<ALoc, ALoc>,
    ) -> Result<(), !> {
        Ok(())
    }

    fn export_named_declaration_specifier(
        &mut self,
        spec: &ast::statement::export_named_declaration::ExportSpecifier<ALoc, ALoc>,
    ) -> Result<(), !> {
        // Ignore renamed export
        let local = &spec.local;
        self.identifier(local)?;
        Ok(())
    }

    fn function_param(&mut self, param: &ast::function::Param<ALoc, ALoc>) -> Result<(), !> {
        let (loc, argument) = match param {
            ast::function::Param::RegularParam { loc, argument, .. } => (loc, argument),
            ast::function::Param::ParamProperty { .. } => {
                // Skip parameter properties
                return Ok(());
            }
        };
        if !flow_parser::ast_utils::pattern_has_binding(argument) && !pattern_has_annot(argument) {
            self.add(
                loc.dupe(),
                EnvKey::new(DefLocType::FunctionParamLoc, loc.dupe()),
            );
        }
        flow_parser::ast_visitor::function_param_default(self, param)
    }

    fn type_guard(&mut self, guard: &ast::types::TypeGuard<ALoc, ALoc>) -> Result<(), !> {
        let (_, t) = &guard.guard;
        // We don't need to include the type guard name here
        if let Some(t) = t {
            self.type_(t)?;
        }
        Ok(())
    }

    fn function_(&mut self, loc: &ALoc, expr: &Function<ALoc, ALoc>) -> Result<(), !> {
        let id = &expr.id;
        let params = &expr.params;
        let return_ = &expr.return_;
        if !self.named_only_for_synthesis {
            match id {
                Some(_) => {}
                None => {
                    self.add(
                        loc.dupe(),
                        EnvKey::new(DefLocType::OrdinaryNameLoc, loc.dupe()),
                    );
                }
            }
            flow_parser::ast_visitor::function_default(self, loc, expr)
        } else {
            // Even if we skip the body of the function,
            // we still need to collect dependencies on the signature of a function.
            let Ok(()) = self.function_return_annotation(return_);
            let params_list = &params.params;
            let rest = &params.rest;
            let this_ = &params.this_;
            for param in params_list.iter() {
                // Only process RegularParam - ParamProperty doesn't have pattern bindings
                if let ast::function::Param::RegularParam { argument, .. } = param {
                    self.visit_annotation_in_pattern(argument);
                }
            }
            if let Some(rest) = rest {
                let argument = &rest.argument;
                self.visit_annotation_in_pattern(argument);
            }
            if let Some(this_) = this_ {
                let annot = &this_.annot;
                let Ok(()) = self.type_annotation(annot);
            }
            Ok(())
        }
    }

    fn declare_component(
        &mut self,
        _loc: &ALoc,
        expr: &ast::statement::DeclareComponent<ALoc, ALoc>,
    ) -> Result<(), !> {
        let params = &expr.params;
        let renders = &expr.renders;
        let tparams = &expr.tparams;
        let Ok(()) = self.component_renders_annotation(renders);
        if let Some(tparams) = tparams {
            let Ok(()) = self.type_params(&TypeParamsContext::DeclareComponent, tparams);
        }
        self.declare_component_params(params);
        Ok(())
    }

    // In order to resolve a def containing a read, the writes that the
    // name_resolver determines reach the variable must be resolved
    fn expression(&mut self, expr: &ast::expression::Expression<ALoc, ALoc>) -> Result<(), !> {
        let loc = expr.loc();
        if self.seen.contains(loc) {
            return Ok(());
        }
        self.seen.insert(loc.dupe());
        // An expression might read an refined value. e.g. if (foo.bar) foo.bar.
        // Therefore, we need to record these writes.
        let writes = self.find_writes(false, true, loc);
        for t in writes {
            self.add(loc.dupe(), t);
        }
        if !self.named_only_for_synthesis {
            self.add(
                loc.dupe(),
                EnvKey::new(DefLocType::OrdinaryNameLoc, loc.dupe()),
            );
            self.add(
                loc.dupe(),
                EnvKey::new(DefLocType::ExpressionLoc, loc.dupe()),
            );
            self.add(
                loc.dupe(),
                EnvKey::new(DefLocType::ArrayProviderLoc, loc.dupe()),
            );
        }
        flow_parser::ast_visitor::expression_default(self, expr)
    }
}

// For all the possible defs, explore the def's structure with the class above
// to find what variables have to be resolved before this def itself can be resolved
fn depends<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
    cx: &'cx mut Cx,
    autocomplete_hooks: &'a AutocompleteHooks<ALoc>,
    this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
    env_values: &'a Values<ALoc>,
    env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
    providers: &'a provider_api::Info<ALoc>,
    declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
    refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
    kind: DefLocType,
    id_loc: ALoc,
    def: &Def,
) -> EnvMap<ALoc, Vec1<ALoc>> {
    fn depends_of_node<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        named_only_for_synthesis: bool,
        state: EnvMap<ALoc, Vec1<ALoc>>,
        mk_visit: impl FnOnce(&mut UseVisitor<'a, 'cx, Cx, Fl>),
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        let mut visitor = UseVisitor::new(
            cx,
            named_only_for_synthesis,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            EnvMap::empty(),
        );
        visitor.acc = state;
        mk_visit(&mut visitor);
        visitor.into_acc()
    }

    fn depends_of_tparams_map<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        tparams_map: &flow_env_builder::name_def_types::TparamsMap,
        state: EnvMap<ALoc, Vec1<ALoc>>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            state,
            |visitor| {
                for (loc, _) in tparams_map.iter() {
                    visitor.add(
                        loc.dupe(),
                        EnvKey::new(DefLocType::OrdinaryNameLoc, loc.dupe()),
                    );
                }
            },
        )
    }

    // depends_of_annotation and of_expression take the `state` parameter from
    // `depends_of_node` above as an additional currried parameter.
    fn depends_of_annotation<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        tparams_map: &flow_env_builder::name_def_types::TparamsMap,
        anno: &ast::types::Annotation<ALoc, ALoc>,
        state: EnvMap<ALoc, Vec1<ALoc>>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        let state = depends_of_tparams_map::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            tparams_map,
            state,
        );
        depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            state,
            |visitor| {
                let Ok(()) = visitor.type_annotation(anno);
            },
        )
    }

    fn depends_of_expression<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        named_only_for_synthesis: bool,
        for_expression_writes: bool,
        expr: &ast::expression::Expression<ALoc, ALoc>,
        state: EnvMap<ALoc, Vec1<ALoc>>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            named_only_for_synthesis,
            state,
            |visitor| {
                if for_expression_writes {
                    visitor.visit_expression_for_expression_writes(expr);
                } else {
                    let Ok(()) = visitor.expression(expr);
                }
            },
        )
    }

    fn depends_of_hint_node<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        hint_node: &flow_env_builder::name_def_types::HintNode,
        state: EnvMap<ALoc, Vec1<ALoc>>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        use flow_env_builder::name_def_types::ArrayElementPatternHint;
        use flow_env_builder::name_def_types::HintNode;
        use flow_env_builder::name_def_types::ObjectPropPatternHint;

        match hint_node {
            HintNode::AnnotationHint(tparams_map, anno) => depends_of_annotation::<Cx, Fl>(
                cx,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                tparams_map,
                anno,
                state,
            ),
            HintNode::ValueHint(_, e) => depends_of_expression::<Cx, Fl>(
                cx,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                false,
                false,
                e,
                state,
            ),
            HintNode::ProvidersHint(providers_nel) => {
                providers_nel.iter().fold(state, |state, loc| {
                    depends_of_node::<Cx, Fl>(
                        cx,
                        this_super_dep_loc_map,
                        env_values,
                        env_entries,
                        providers,
                        declare_namespace_read_paths,
                        refinement_of_id,
                        false,
                        state,
                        |visitor| {
                            visitor.add(
                                loc.dupe(),
                                EnvKey::new(DefLocType::OrdinaryNameLoc, loc.dupe()),
                            );
                        },
                    )
                })
            }
            HintNode::WriteLocHint(hint_kind, loc) => depends_of_node::<Cx, Fl>(
                cx,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                false,
                state,
                |visitor| {
                    visitor.add(loc.dupe(), EnvKey::new(*hint_kind, loc.dupe()));
                },
            ),
            HintNode::StringLiteralType(_) => state,
            HintNode::ReactFragmentType => state,
            HintNode::ReactNodeType => state,
            HintNode::AnyErrorHint(_) => state,
            HintNode::ComposedArrayPatternHint(_, elements) => {
                elements.iter().fold(state, |state, elem| match elem {
                    ArrayElementPatternHint::ArrayElementPatternHint(h)
                    | ArrayElementPatternHint::ArrayRestElementPatternHint(h) => {
                        depends_of_hint_node::<Cx, Fl>(
                            cx,
                            this_super_dep_loc_map,
                            env_values,
                            env_entries,
                            providers,
                            declare_namespace_read_paths,
                            refinement_of_id,
                            h,
                            state,
                        )
                    }
                })
            }
            HintNode::ComposedObjectPatternHint(_, props) => {
                props.iter().fold(state, |state, prop| match prop {
                    ObjectPropPatternHint::ObjectPropPatternHint(_, _, h)
                    | ObjectPropPatternHint::ObjectSpreadPropPatternHint(h) => {
                        depends_of_hint_node::<Cx, Fl>(
                            cx,
                            this_super_dep_loc_map,
                            env_values,
                            env_entries,
                            providers,
                            declare_namespace_read_paths,
                            refinement_of_id,
                            h,
                            state,
                        )
                    }
                })
            }
        }
    }

    fn depends_of_hint<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        autocomplete_hooks: &'a AutocompleteHooks<ALoc>,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        hint: &AstHint,
        state: EnvMap<ALoc, Vec1<ALoc>>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        use flow_common::hint::Hint;

        match hint {
            Hint::HintPlaceholder => state,
            Hint::HintT(hint_node, _) => depends_of_hint_node::<Cx, Fl>(
                cx,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                hint_node,
                state,
            ),
            Hint::HintDecomp(ops, hint_node, _) => {
                fn depends_on_synthesizable_toplevel_expressions<
                    'a,
                    'ast,
                    'cx,
                    Cx: Context,
                    Fl: Flow<Cx = Cx>,
                >(
                    cx: &'cx mut Cx,
                    autocomplete_hooks: &'a AutocompleteHooks<ALoc>,
                    this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
                    env_values: &'a Values<ALoc>,
                    env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
                    providers: &'a provider_api::Info<ALoc>,
                    declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
                    refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
                    acc: EnvMap<ALoc, Vec1<ALoc>>,
                    collect: impl FnOnce(&mut ToplevelExpressionCollector<'ast>),
                ) -> EnvMap<ALoc, Vec1<ALoc>> {
                    let mut collector = ToplevelExpressionCollector::default();
                    collect(&mut collector);
                    let ToplevelExpressionCollector {
                        acc: exprs,
                        element_names,
                    } = collector;
                    let acc = element_names.into_iter().fold(acc, |acc, ident| {
                        depends_of_node::<Cx, Fl>(
                            cx,
                            this_super_dep_loc_map,
                            env_values,
                            env_entries,
                            providers,
                            declare_namespace_read_paths,
                            refinement_of_id,
                            false,
                            acc,
                            |visitor| {
                                let Ok(()) = visitor.jsx_element_name_identifier(ident);
                            },
                        )
                    });
                    exprs.into_iter().fold(acc, |acc, e| {
                        if flow_env_builder::name_def::expression_is_definitely_synthesizable(
                            autocomplete_hooks,
                            e,
                        ) {
                            depends_of_expression::<Cx, Fl>(
                                cx,
                                this_super_dep_loc_map,
                                env_values,
                                env_entries,
                                providers,
                                declare_namespace_read_paths,
                                refinement_of_id,
                                false,
                                false,
                                e,
                                acc,
                            )
                        } else {
                            depends_of_expression::<Cx, Fl>(
                                cx,
                                this_super_dep_loc_map,
                                env_values,
                                env_entries,
                                providers,
                                declare_namespace_read_paths,
                                refinement_of_id,
                                true,
                                false,
                                e,
                                acc,
                            )
                        }
                    })
                }

                let initial = depends_of_hint_node::<Cx, Fl>(
                    cx,
                    this_super_dep_loc_map,
                    env_values,
                    env_entries,
                    providers,
                    declare_namespace_read_paths,
                    refinement_of_id,
                    hint_node,
                    state,
                );
                ops.iter().fold(initial, |acc, (_id, op)| {
                    use flow_common::hint::HintDecompositionInner;
                    match op.inner() {
                        HintDecompositionInner::DecompObjComputed(r) => {
                            let loc = r.loc();
                            depends_of_node::<Cx, Fl>(
                                cx,
                                this_super_dep_loc_map,
                                env_values,
                                env_entries,
                                providers,
                                declare_namespace_read_paths,
                                refinement_of_id,
                                false,
                                acc,
                                |visitor| {
                                    visitor.add(
                                        loc.dupe(),
                                        EnvKey::new(DefLocType::ExpressionLoc, loc.dupe()),
                                    );
                                },
                            )
                        }
                        HintDecompositionInner::DecompSentinelRefinement(checks) => {
                            checks.values().fold(acc, |acc, check| {
                                use flow_common::hint::SentinelRefinement;
                                match check {
                                    SentinelRefinement::Member(r) => {
                                        let loc = r.loc();
                                        depends_of_node::<Cx, Fl>(
                                            cx,
                                            this_super_dep_loc_map,
                                            env_values,
                                            env_entries,
                                            providers,
                                            declare_namespace_read_paths,
                                            refinement_of_id,
                                            false,
                                            acc,
                                            |visitor| {
                                                visitor.add(
                                                    loc.dupe(),
                                                    EnvKey::new(
                                                        DefLocType::ExpressionLoc,
                                                        loc.dupe(),
                                                    ),
                                                );
                                            },
                                        )
                                    }
                                    _ => acc,
                                }
                            })
                        }
                        HintDecompositionInner::InstantiateComponent(hints) => {
                            let (jsx_props, jsx_children) = &hints.jsx_props_and_children;
                            depends_on_synthesizable_toplevel_expressions::<Cx, Fl>(
                                cx,
                                autocomplete_hooks,
                                this_super_dep_loc_map,
                                env_values,
                                env_entries,
                                providers,
                                declare_namespace_read_paths,
                                refinement_of_id,
                                acc,
                                |collector| {
                                    for prop in jsx_props.iter() {
                                        let Ok(()) = collector.jsx_opening_attribute(prop);
                                    }
                                    let Ok(()) =
                                        collector.jsx_children(&jsx_children.0, &jsx_children.1);
                                },
                            )
                        }
                        HintDecompositionInner::InstantiateCallee(hints) => {
                            let return_hints = hints.return_hints.get_forced(&mut ());
                            let arg_list = hints.arg_list.get_forced(&mut ());
                            let arg_index = hints.arg_index;
                            let targs = hints.targs.get_forced(&mut ());
                            let acc = depends_of_node::<Cx, Fl>(
                                cx,
                                this_super_dep_loc_map,
                                env_values,
                                env_entries,
                                providers,
                                declare_namespace_read_paths,
                                refinement_of_id,
                                false,
                                acc,
                                |collector| {
                                    if let Some(targs_val) = targs.as_ref() {
                                        let Ok(()) = collector.call_type_args(targs_val);
                                    }
                                },
                            );
                            let arguments = &arg_list.arguments;
                            let acc = depends_of_hints::<Cx, Fl>(
                                cx,
                                autocomplete_hooks,
                                this_super_dep_loc_map,
                                env_values,
                                env_entries,
                                providers,
                                declare_namespace_read_paths,
                                refinement_of_id,
                                return_hints,
                                acc,
                            );
                            let arg_index = arg_index as usize;
                            let mut acc = acc;
                            for (i, arg) in arguments.iter().enumerate() {
                                if i >= arg_index {
                                    acc = depends_on_synthesizable_toplevel_expressions::<Cx, Fl>(
                                        cx,
                                        autocomplete_hooks,
                                        this_super_dep_loc_map,
                                        env_values,
                                        env_entries,
                                        providers,
                                        declare_namespace_read_paths,
                                        refinement_of_id,
                                        acc,
                                        |collector| {
                                            let Ok(()) = collector.expression_or_spread(arg);
                                        },
                                    );
                                } else {
                                    acc = depends_of_node::<Cx, Fl>(
                                        cx,
                                        this_super_dep_loc_map,
                                        env_values,
                                        env_entries,
                                        providers,
                                        declare_namespace_read_paths,
                                        refinement_of_id,
                                        false,
                                        acc,
                                        |visitor| {
                                            let Ok(()) = visitor.expression_or_spread(arg);
                                        },
                                    );
                                }
                            }
                            acc
                        }
                        _ => acc,
                    }
                })
            }
        }
    }

    fn depends_of_hints<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        autocomplete_hooks: &'a AutocompleteHooks<ALoc>,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        hints: &[AstHint],
        state: EnvMap<ALoc, Vec1<ALoc>>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        hints.iter().fold(state, |state, hint| {
            depends_of_hint::<Cx, Fl>(
                cx,
                autocomplete_hooks,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                hint,
                state,
            )
        })
    }

    fn depends_of_fun<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        autocomplete_hooks: &'a AutocompleteHooks<ALoc>,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        id_loc: ALoc,
        synth: &FunctionSynthKind,
        tparams_map: &flow_env_builder::name_def_types::TparamsMap,
        hints: &[AstHint],
        statics: &BTreeMap<FlowSmolStr, EnvKey<ALoc>>,
        namespace_types: &BTreeMap<FlowSmolStr, EnvKey<ALoc>>,
        function_: &ast::function::Function<ALoc, ALoc>,
        state: EnvMap<ALoc, Vec1<ALoc>>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        let fully_annotated = match synth {
            FunctionSynthKind::FunctionSynthesizable => true,
            _ => false,
        };
        let state = depends_of_hints::<Cx, Fl>(
            cx,
            autocomplete_hooks,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            hints,
            state,
        );
        let state = depends_of_tparams_map::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            tparams_map,
            state,
        );
        let state = depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            state,
            |visitor| {
                visitor.function_def(fully_annotated, function_);
            },
        );
        depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            state,
            |visitor| {
                for env_key in statics.values() {
                    visitor.add(id_loc.dupe(), env_key.dupe());
                }
                for env_key in namespace_types.values() {
                    visitor.add(id_loc.dupe(), env_key.dupe());
                }
            },
        )
    }

    fn depends_of_declared_fun<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        id_loc: ALoc,
        declarations: &[(ALoc, ast::statement::DeclareFunction<ALoc, ALoc>)],
        statics: &BTreeMap<FlowSmolStr, EnvKey<ALoc>>,
        namespace_types: &BTreeMap<FlowSmolStr, EnvKey<ALoc>>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        let state = depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            EnvMap::empty(),
            |visitor| {
                for (loc, declaration) in declarations {
                    let Ok(()) = visitor.declare_function(loc, declaration);
                }
            },
        );
        depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            state,
            |visitor| {
                for env_key in statics.values() {
                    visitor.add(id_loc.dupe(), env_key.dupe());
                }
                for env_key in namespace_types.values() {
                    visitor.add(id_loc.dupe(), env_key.dupe());
                }
            },
        )
    }

    fn depends_of_component<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        tparams_map: &flow_env_builder::name_def_types::TparamsMap,
        component: &ast::statement::ComponentDeclaration<ALoc, ALoc>,
        state: EnvMap<ALoc, Vec1<ALoc>>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        let state = depends_of_tparams_map::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            tparams_map,
            state,
        );
        depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            state,
            |visitor| {
                visitor.component_def(component);
            },
        )
    }

    fn depends_of_class<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        class_: &ast::class::Class<ALoc, ALoc>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            EnvMap::empty(),
            |visitor| {
                visitor.class_body_annotated(&class_.body);
                if let Some(extends) = &class_.extends {
                    visitor.class_extends_sig(extends);
                }
                if let Some(implements) = &class_.implements {
                    let Ok(()) = visitor.class_implements(implements);
                }
                for decorator in class_.class_decorators.iter() {
                    let Ok(()) = visitor.class_decorator(decorator);
                }
                if let Some(tparams) = &class_.tparams {
                    let Ok(()) = visitor.type_params(&TypeParamsContext::Class, tparams);
                }
            },
        )
    }

    fn depends_of_record<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        record: &ast::statement::RecordDeclaration<ALoc, ALoc>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            EnvMap::empty(),
            |visitor| {
                visitor.record_body_annotated(&record.body);
                if let Some(implements) = &record.implements {
                    let Ok(()) = visitor.class_implements(implements);
                }
                if let Some(tparams) = &record.tparams {
                    let Ok(()) = visitor.type_params(&TypeParamsContext::Record, tparams);
                }
            },
        )
    }

    fn depends_of_declared_class<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        decl: &ast::statement::DeclareClass<ALoc, ALoc>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            EnvMap::empty(),
            |visitor| {
                if let Some(tparams) = &decl.tparams {
                    let Ok(()) = visitor.type_params(&TypeParamsContext::DeclareClass, tparams);
                }
                let Ok(()) = visitor.object_type(&decl.body.1);
                if let Some((_loc, ext)) = &decl.extends {
                    fn visit_extends<V>(
                        visitor: &mut V,
                        ext: &ast::statement::DeclareClassExtends<ALoc, ALoc>,
                    ) where
                        V: ?Sized
                            + for<'a> flow_parser::ast_visitor::AstVisitor<
                                'a,
                                ALoc,
                                ALoc,
                                &'a ALoc,
                                !,
                            >,
                    {
                        match ext {
                            ast::statement::DeclareClassExtends::ExtendsIdent(g) => {
                                let Ok(()) = visitor.generic_type(g);
                            }
                            ast::statement::DeclareClassExtends::ExtendsCall {
                                callee: (_callee_loc, callee),
                                arg,
                            } => {
                                let Ok(()) = visitor.generic_type(callee);
                                visit_extends(visitor, &arg.1);
                            }
                        }
                    }
                    visit_extends(visitor, ext);
                }
                for mixin in decl.mixins.iter() {
                    let Ok(()) = visitor.generic_type(&mixin.1);
                }
                if let Some(implements) = &decl.implements {
                    let Ok(()) = visitor.class_implements(implements);
                }
            },
        )
    }

    fn depends_of_declared_component<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        loc: ALoc,
        component: &ast::statement::DeclareComponent<ALoc, ALoc>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            EnvMap::empty(),
            |visitor| {
                let Ok(()) = visitor.declare_component(&loc, component);
            },
        )
    }

    fn depends_of_declared_namespace<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        ns: &ast::statement::DeclareNamespace<ALoc, ALoc>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            EnvMap::empty(),
            |visitor| {
                let Ok(()) = visitor.block(&ns.body.0, &ns.body.1);
            },
        )
    }

    fn depends_of_alias<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        alias: &ast::statement::TypeAlias<ALoc, ALoc>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            EnvMap::empty(),
            |visitor| {
                if let Some(tparams) = &alias.tparams {
                    let Ok(()) = visitor.type_params(&TypeParamsContext::TypeAlias, tparams);
                }
                let Ok(()) = visitor.type_(&alias.right);
            },
        )
    }

    fn depends_of_opaque<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        alias: &ast::statement::OpaqueType<ALoc, ALoc>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            EnvMap::empty(),
            |visitor| {
                if let Some(tparams) = &alias.tparams {
                    let Ok(()) = visitor.type_params(&TypeParamsContext::OpaqueType, tparams);
                }
                if let Some(impl_type) = &alias.impl_type {
                    let Ok(()) = visitor.type_(impl_type);
                }
                if let Some(lower_bound) = &alias.lower_bound {
                    let Ok(()) = visitor.type_(lower_bound);
                }
                if let Some(upper_bound) = &alias.upper_bound {
                    let Ok(()) = visitor.type_(upper_bound);
                }
                if let Some(legacy_upper_bound) = &alias.legacy_upper_bound {
                    let Ok(()) = visitor.type_(legacy_upper_bound);
                }
            },
        )
    }

    fn depends_of_tparam<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        tparams_map: &flow_env_builder::name_def_types::TparamsMap,
        tparam: &(ALoc, ast::types::TypeParam<ALoc, ALoc>),
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        let state = depends_of_tparams_map::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            tparams_map,
            EnvMap::empty(),
        );
        depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            state,
            |visitor| {
                let Ok(()) = visitor.type_annotation_hint(&tparam.1.bound);
                let Ok(()) = visitor.variance_opt(tparam.1.variance.as_ref());
                if let Some(default) = &tparam.1.default {
                    let Ok(()) = visitor.type_(default);
                }
            },
        )
    }

    fn depends_of_interface<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        inter: &ast::statement::Interface<ALoc, ALoc>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            EnvMap::empty(),
            |visitor| {
                if let Some(tparams) = &inter.tparams {
                    let Ok(()) = visitor.type_params(&TypeParamsContext::Interface, tparams);
                }
                for ext in inter.extends.iter() {
                    let Ok(()) = visitor.generic_type(&ext.1);
                }
                let Ok(()) = visitor.object_type(&inter.body.1);
            },
        )
    }

    fn depends_of_hinted_expression<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        autocomplete_hooks: &'a AutocompleteHooks<ALoc>,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        for_expression_writes: bool,
        hints: &[AstHint],
        expr: &ast::expression::Expression<ALoc, ALoc>,
        state: EnvMap<ALoc, Vec1<ALoc>>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        let state = depends_of_expression::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            for_expression_writes,
            expr,
            state,
        );
        depends_of_hints::<Cx, Fl>(
            cx,
            autocomplete_hooks,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            hints,
            state,
        )
    }

    fn depends_on_match_pattern<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        case_match_root_loc: ALoc,
        pattern: &ast::match_pattern::MatchPattern<ALoc, ALoc>,
        state: EnvMap<ALoc, Vec1<ALoc>>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            state,
            |visitor| {
                let Ok(()) = visitor.identifier(&flow_parser::ast_utils::match_root_ident(
                    case_match_root_loc.dupe(),
                ));
                let Ok(()) = visitor.match_pattern(pattern);
            },
        )
    }

    fn depends_of_root<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        autocomplete_hooks: &'a AutocompleteHooks<ALoc>,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        id_loc: ALoc,
        root: &Root,
        state: EnvMap<ALoc, Vec1<ALoc>>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        use flow_env_builder::name_def_types::ObjectSynthKind::ObjectSynthesizable as OS;
        use flow_env_builder::name_def_types::Root;

        match root {
            Root::Annotation(box AnnotationData {
                tparams_map, annot, ..
            }) => {
                let annot_struct = ast::types::Annotation {
                    loc: annot.0.dupe(),
                    annotation: annot.1.clone(),
                };
                depends_of_annotation::<Cx, Fl>(
                    cx,
                    this_super_dep_loc_map,
                    env_values,
                    env_entries,
                    providers,
                    declare_namespace_read_paths,
                    refinement_of_id,
                    tparams_map,
                    &annot_struct,
                    state,
                )
            }
            Root::Value(box flow_env_builder::name_def_types::Value { hints, expr, .. }) => {
                let state = depends_of_hints::<Cx, Fl>(
                    cx,
                    autocomplete_hooks,
                    this_super_dep_loc_map,
                    env_values,
                    env_entries,
                    providers,
                    declare_namespace_read_paths,
                    refinement_of_id,
                    hints,
                    state,
                );
                depends_of_expression::<Cx, Fl>(
                    cx,
                    this_super_dep_loc_map,
                    env_values,
                    env_entries,
                    providers,
                    declare_namespace_read_paths,
                    refinement_of_id,
                    false,
                    false,
                    expr,
                    state,
                )
            }
            Root::MatchCaseRoot(box MatchCaseRootData {
                case_match_root_loc,
                root_pattern_loc: _,
                prev_pattern_loc,
            }) => depends_of_node::<Cx, Fl>(
                cx,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                false,
                state,
                |visitor| {
                    let Ok(()) = visitor.identifier(&flow_parser::ast_utils::match_root_ident(
                        case_match_root_loc.dupe(),
                    ));
                    // Only depend on the immediately previous pattern. Transitive dependencies
                    // ensure proper ordering without O(N) dependencies per pattern.
                    if let Some(prev_loc) = prev_pattern_loc {
                        visitor.add(
                            prev_loc.dupe(),
                            EnvKey::new(DefLocType::MatchCasePatternLoc, prev_loc.dupe()),
                        );
                    }
                },
            ),
            Root::ObjectValue(box ObjectValueData {
                obj,
                obj_loc: _,
                synthesizable: OS { .. },
            }) => {
                fn depends_of_synthesizable_expression<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
                    cx: &'cx mut Cx,
                    autocomplete_hooks: &'a AutocompleteHooks<ALoc>,
                    this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
                    env_values: &'a Values<ALoc>,
                    env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
                    providers: &'a provider_api::Info<ALoc>,
                    declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
                    refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
                    id_loc: ALoc,
                    expression: &ast::expression::Expression<ALoc, ALoc>,
                    state: EnvMap<ALoc, Vec1<ALoc>>,
                ) -> EnvMap<ALoc, Vec1<ALoc>> {
                    use ast::expression::ExpressionInner;
                    match expression.deref() {
                        ExpressionInner::Object { inner: obj, .. } => loop_obj::<Cx, Fl>(
                            cx,
                            autocomplete_hooks,
                            this_super_dep_loc_map,
                            env_values,
                            env_entries,
                            providers,
                            declare_namespace_read_paths,
                            refinement_of_id,
                            id_loc,
                            obj,
                            state,
                        ),
                        ExpressionInner::TypeCast { inner, .. } => depends_of_annotation::<Cx, Fl>(
                            cx,
                            this_super_dep_loc_map,
                            env_values,
                            env_entries,
                            providers,
                            declare_namespace_read_paths,
                            refinement_of_id,
                            &Default::default(),
                            &inner.annot,
                            state,
                        ),
                        ExpressionInner::AsExpression { inner, .. } => {
                            depends_of_annotation::<Cx, Fl>(
                                cx,
                                this_super_dep_loc_map,
                                env_values,
                                env_entries,
                                providers,
                                declare_namespace_read_paths,
                                refinement_of_id,
                                &Default::default(),
                                &inner.annot,
                                state,
                            )
                        }
                        ExpressionInner::Array { inner, .. } => {
                            inner.elements.iter().fold(state, |state, elem| {
                                use ast::expression::ArrayElement;
                                match elem {
                                    ArrayElement::Expression(exp) => {
                                        depends_of_synthesizable_expression::<Cx, Fl>(
                                            cx,
                                            autocomplete_hooks,
                                            this_super_dep_loc_map,
                                            env_values,
                                            env_entries,
                                            providers,
                                            declare_namespace_read_paths,
                                            refinement_of_id,
                                            id_loc.dupe(),
                                            exp,
                                            state,
                                        )
                                    }
                                    ArrayElement::Spread(spread) => {
                                        depends_of_synthesizable_expression::<Cx, Fl>(
                                            cx,
                                            autocomplete_hooks,
                                            this_super_dep_loc_map,
                                            env_values,
                                            env_entries,
                                            providers,
                                            declare_namespace_read_paths,
                                            refinement_of_id,
                                            id_loc.dupe(),
                                            &spread.argument,
                                            state,
                                        )
                                    }
                                    ArrayElement::Hole(_) => state,
                                }
                            })
                        }
                        ExpressionInner::Function { inner: fn_, .. }
                        | ExpressionInner::ArrowFunction { inner: fn_, .. } => {
                            depends_of_fun::<Cx, Fl>(
                                cx,
                                autocomplete_hooks,
                                this_super_dep_loc_map,
                                env_values,
                                env_entries,
                                providers,
                                declare_namespace_read_paths,
                                refinement_of_id,
                                id_loc,
                                &FunctionSynthKind::FunctionSynthesizable,
                                &Default::default(),
                                &[],
                                &Default::default(),
                                &Default::default(),
                                fn_,
                                state,
                            )
                        }
                        ExpressionInner::Member {
                            loc: exp_loc,
                            inner,
                        } if matches!(
                            inner.property,
                            ast::expression::member::Property::PropertyIdentifier(_)
                        ) =>
                        {
                            let mut visitor = UseVisitor::<Cx, Fl>::new(
                                cx,
                                false,
                                this_super_dep_loc_map,
                                env_values,
                                env_entries,
                                providers,
                                declare_namespace_read_paths,
                                refinement_of_id,
                                state,
                            );
                            let writes = visitor.find_writes(false, true, exp_loc);
                            for w in writes {
                                visitor.add(exp_loc.dupe(), w);
                            }
                            let state = visitor.into_acc();
                            depends_of_synthesizable_expression::<Cx, Fl>(
                                cx,
                                autocomplete_hooks,
                                this_super_dep_loc_map,
                                env_values,
                                env_entries,
                                providers,
                                declare_namespace_read_paths,
                                refinement_of_id,
                                id_loc,
                                &inner.object,
                                state,
                            )
                        }
                        _ => depends_of_expression::<Cx, Fl>(
                            cx,
                            this_super_dep_loc_map,
                            env_values,
                            env_entries,
                            providers,
                            declare_namespace_read_paths,
                            refinement_of_id,
                            false,
                            false,
                            expression,
                            state,
                        ),
                    }
                }

                fn loop_obj<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
                    cx: &'cx mut Cx,
                    autocomplete_hooks: &'a AutocompleteHooks<ALoc>,
                    this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
                    env_values: &'a Values<ALoc>,
                    env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
                    providers: &'a provider_api::Info<ALoc>,
                    declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
                    refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
                    id_loc: ALoc,
                    obj: &ast::expression::Object<ALoc, ALoc>,
                    state: EnvMap<ALoc, Vec1<ALoc>>,
                ) -> EnvMap<ALoc, Vec1<ALoc>> {
                    use ast::expression::object::Key;
                    use ast::expression::object::NormalProperty;
                    use ast::expression::object::Property;

                    obj.properties.iter().fold(state, |state, prop| match prop {
                        Property::SpreadProperty(ast::expression::object::SpreadProperty {
                            argument,
                            ..
                        }) => depends_of_expression::<Cx, Fl>(
                            cx,
                            this_super_dep_loc_map,
                            env_values,
                            env_entries,
                            providers,
                            declare_namespace_read_paths,
                            refinement_of_id,
                            false,
                            false,
                            argument,
                            state,
                        ),
                        Property::NormalProperty(NormalProperty::Method {
                            key: Key::Identifier(_),
                            value: (_, fn_),
                            ..
                        }) => depends_of_fun::<Cx, Fl>(
                            cx,
                            autocomplete_hooks,
                            this_super_dep_loc_map,
                            env_values,
                            env_entries,
                            providers,
                            declare_namespace_read_paths,
                            refinement_of_id,
                            id_loc.dupe(),
                            &FunctionSynthKind::FunctionSynthesizable,
                            &Default::default(),
                            &[],
                            &Default::default(),
                            &Default::default(),
                            fn_,
                            state,
                        ),
                        Property::NormalProperty(NormalProperty::Init {
                            key: Key::Identifier(_),
                            value,
                            ..
                        }) => depends_of_synthesizable_expression::<Cx, Fl>(
                            cx,
                            autocomplete_hooks,
                            this_super_dep_loc_map,
                            env_values,
                            env_entries,
                            providers,
                            declare_namespace_read_paths,
                            refinement_of_id,
                            id_loc.dupe(),
                            value,
                            state,
                        ),
                        _ => {
                            Fl::add_output(
                                cx,
                                ErrorMessage::EInternal(Box::new((
                                    id_loc.dupe(),
                                    InternalError::EnvInvariant(EnvInvariantFailure::Impossible(
                                        "Object not synthesizable".into(),
                                    )),
                                ))),
                            );
                            state
                        }
                    })
                }

                loop_obj::<Cx, Fl>(
                    cx,
                    autocomplete_hooks,
                    this_super_dep_loc_map,
                    env_values,
                    env_entries,
                    providers,
                    declare_namespace_read_paths,
                    refinement_of_id,
                    id_loc,
                    obj,
                    state,
                )
            }
            Root::ObjectValue(box ObjectValueData { obj, obj_loc, .. }) => {
                let expr = ast::expression::Expression(Arc::new(
                    ast::expression::ExpressionInner::Object {
                        loc: obj_loc.dupe(),
                        inner: Arc::new(obj.clone()),
                    },
                ));
                depends_of_expression::<Cx, Fl>(
                    cx,
                    this_super_dep_loc_map,
                    env_values,
                    env_entries,
                    providers,
                    declare_namespace_read_paths,
                    refinement_of_id,
                    false,
                    false,
                    &expr,
                    EnvMap::empty(),
                )
            }
            Root::FunctionValue(box FunctionValueData {
                hints,
                synthesizable_from_annotation,
                function_loc: _,
                function_,
                statics,
                arrow: _,
                tparams_map,
            }) => depends_of_fun::<Cx, Fl>(
                cx,
                autocomplete_hooks,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                id_loc,
                synthesizable_from_annotation,
                tparams_map,
                hints,
                statics,
                &Default::default(),
                function_,
                state,
            ),
            Root::EmptyArray(box EmptyArrayData {
                array_providers, ..
            }) => array_providers.iter().fold(state, |mut acc, loc| {
                let key = EnvKey::new(DefLocType::ArrayProviderLoc, loc.dupe());
                let new_value = match acc.get(&key) {
                    None => Vec1::new(id_loc.dupe()),
                    Some(locs) => {
                        let mut v = locs.clone();
                        v.push(id_loc.dupe());
                        v
                    }
                };
                acc.insert(key, new_value);
                acc
            }),
            Root::For(box (_, exp)) => depends_of_expression::<Cx, Fl>(
                cx,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                false,
                false,
                &exp.1,
                state,
            ),
            Root::Contextual(box ContextualData {
                reason: _,
                hints,
                optional: _,
                default_expression,
            }) => {
                let state = match default_expression {
                    Some(e) => depends_of_expression::<Cx, Fl>(
                        cx,
                        this_super_dep_loc_map,
                        env_values,
                        env_entries,
                        providers,
                        declare_namespace_read_paths,
                        refinement_of_id,
                        false,
                        false,
                        &e.1,
                        state,
                    ),
                    None => state,
                };
                depends_of_hints::<Cx, Fl>(
                    cx,
                    autocomplete_hooks,
                    this_super_dep_loc_map,
                    env_values,
                    env_entries,
                    providers,
                    declare_namespace_read_paths,
                    refinement_of_id,
                    hints,
                    state,
                )
            }
            Root::CatchUnannotated => state,
            Root::DeclareVariableMissingAnnotationAndInit => state,
            Root::UnannotatedParameter { .. } => state,
        }
    }

    fn depends_of_selector<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        selector: &Selector<ALoc, ALoc>,
        state: EnvMap<ALoc, Vec1<ALoc>>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        use flow_env_builder::selector::Selector;

        match selector {
            Selector::Computed { expression, .. } => depends_of_expression::<Cx, Fl>(
                cx,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                false,
                false,
                expression,
                state,
            ),
            Selector::Prop { prop_loc, .. } => {
                // In `const {d: {a, b}} = obj`, each prop might be reading from a refined value,
                // which is a write. We need to track these dependencies as well.
                let mut visitor = UseVisitor::<Cx, Fl>::new(
                    cx,
                    false,
                    this_super_dep_loc_map,
                    env_values,
                    env_entries,
                    providers,
                    declare_namespace_read_paths,
                    refinement_of_id,
                    state,
                );
                let writes = visitor.find_writes(false, true, prop_loc);
                for w in writes {
                    visitor.add(prop_loc.dupe(), w);
                }
                visitor.into_acc()
            }
            Selector::Default
            | Selector::Elem { .. }
            | Selector::ObjRest { .. }
            | Selector::ArrRest(_) => state,
        }
    }

    fn depends_of_lhs<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        id_loc: ALoc,
        lhs_member_expression: Option<&ast::expression::Expression<ALoc, ALoc>>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        // When looking at a binding def, like `x = y`, in order to resolve this def we need
        // to have resolved the providers for `x`, as well as the type of `y`, in order to check
        // the type of `y` against `x`. So in addition to exploring the RHS, we also add the providers
        // for `x` to the set of dependencies.
        match lhs_member_expression {
            Some(e) => depends_of_expression::<Cx, Fl>(
                cx,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                false,
                true,
                e,
                EnvMap::empty(),
            ),
            None => match providers.providers_of_def(&id_loc) {
                Some(provider_info) => {
                    if !providers.is_provider(&id_loc) {
                        provider_info.providers.iter().fold(
                            EnvMap::empty(),
                            |mut acc, provider_entry| {
                                let key = EnvKey::new(
                                    DefLocType::OrdinaryNameLoc,
                                    provider_entry.reason.loc().dupe(),
                                );
                                if flow_env_builder::env_api::has_assigning_write(
                                    key.dupe(),
                                    env_entries,
                                ) {
                                    let new_value = match acc.get(&key) {
                                        None => Vec1::new(id_loc.dupe()),
                                        Some(locs) => {
                                            let mut v = locs.clone();
                                            v.push(id_loc.dupe());
                                            v
                                        }
                                    };
                                    acc.insert(key, new_value);
                                }
                                acc
                            },
                        )
                    } else {
                        EnvMap::empty()
                    }
                }
                None => EnvMap::empty(),
            },
        }
    }

    fn depends_of_binding<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        autocomplete_hooks: &'a AutocompleteHooks<ALoc>,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        kind: DefLocType,
        id_loc: ALoc,
        bind: &Binding,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        use flow_env_builder::name_def_types::Binding;

        let state = if kind == DefLocType::PatternLoc || kind == DefLocType::FunctionParamLoc {
            EnvMap::empty()
        } else {
            depends_of_lhs::<Cx, Fl>(
                cx,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                id_loc.dupe(),
                None,
            )
        };

        match bind {
            Binding::Root(root) => depends_of_root::<Cx, Fl>(
                cx,
                autocomplete_hooks,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                id_loc,
                root,
                state,
            ),
            Binding::Hooklike(inner_bind) => depends_of_binding::<Cx, Fl>(
                cx,
                autocomplete_hooks,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                kind,
                id_loc,
                inner_bind,
            ),
            Binding::Select { selector, parent } => {
                let (parent_loc, _) = parent;
                let state = depends_of_selector::<Cx, Fl>(
                    cx,
                    this_super_dep_loc_map,
                    env_values,
                    env_entries,
                    providers,
                    declare_namespace_read_paths,
                    refinement_of_id,
                    selector,
                    state,
                );
                depends_of_node::<Cx, Fl>(
                    cx,
                    this_super_dep_loc_map,
                    env_values,
                    env_entries,
                    providers,
                    declare_namespace_read_paths,
                    refinement_of_id,
                    false,
                    state,
                    |visitor| {
                        visitor.add(
                            parent_loc.dupe(),
                            EnvKey::new(DefLocType::PatternLoc, parent_loc.dupe()),
                        );
                    },
                )
            }
        }
    }

    fn depends_of_update<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        id_loc: ALoc,
        lhs: Option<&ast::expression::Expression<ALoc, ALoc>>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        let state = depends_of_lhs::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            id_loc.dupe(),
            lhs,
        );
        match lhs {
            // assigning to member
            Some(_) => state,
            // assigning to identifier
            None => {
                let mut visitor = UseVisitor::<Cx, Fl>::new(
                    cx,
                    false,
                    this_super_dep_loc_map,
                    env_values,
                    env_entries,
                    providers,
                    declare_namespace_read_paths,
                    refinement_of_id,
                    state,
                );
                let writes = visitor.find_writes(false, false, &id_loc);
                for w in writes {
                    visitor.add(id_loc.dupe(), w);
                }
                visitor.into_acc()
            }
        }
    }

    fn depends_of_op_assign<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        id_loc: ALoc,
        lhs: &ast::pattern::Pattern<ALoc, ALoc>,
        rhs: &ast::expression::Expression<ALoc, ALoc>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        let (unwrapped_lhs, _) = flow_parser::ast_utils::unwrap_nonnull_lhs(lhs);
        let lhs_expr = match unwrapped_lhs.as_ref() {
            ast::pattern::Pattern::Expression { inner, .. } => Some(inner.as_ref()),
            _ => None,
        };
        // reusing depends_of_update, since the LHS of an op-assign is handled identically to an update
        let state = depends_of_update::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            id_loc,
            lhs_expr,
        );
        depends_of_expression::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            false,
            rhs,
            state,
        )
    }

    fn depends_of_member_assign<'a, 'cx, Cx: Context, Fl: Flow<Cx = Cx>>(
        cx: &'cx mut Cx,
        this_super_dep_loc_map: &'a EnvMap<ALoc, EnvKey<ALoc>>,
        env_values: &'a Values<ALoc>,
        env_entries: &'a EnvMap<ALoc, EnvEntry<ALoc>>,
        providers: &'a provider_api::Info<ALoc>,
        declare_namespace_read_paths: DeclareNamespaceReadPaths<'a>,
        refinement_of_id: &'a dyn Fn(i32) -> Refinement<ALoc>,
        member_loc: ALoc,
        member: &ast::expression::Member<ALoc, ALoc>,
        rhs: &ast::expression::Expression<ALoc, ALoc>,
    ) -> EnvMap<ALoc, Vec1<ALoc>> {
        let state = depends_of_node::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            EnvMap::empty(),
            |visitor| {
                let Ok(()) = visitor.member(&member_loc, member);
            },
        );
        depends_of_expression::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            false,
            false,
            rhs,
            state,
        )
    }

    match def {
        Def::Binding(binding) => depends_of_binding::<Cx, Fl>(
            cx,
            autocomplete_hooks,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            kind,
            id_loc,
            binding,
        ),
        Def::MatchCasePattern(box MatchCasePatternData {
            case_match_root_loc,
            has_guard: _,
            pattern,
            prev_pattern_loc,
        }) => {
            // Add dependency on the immediately previous pattern for incremental PatternUnion building
            let state = match prev_pattern_loc {
                Some(prev_loc) => {
                    let mut state = EnvMap::empty();
                    state.insert(
                        EnvKey::new(DefLocType::MatchCasePatternLoc, prev_loc.dupe()),
                        Vec1::new(pattern.0.dupe()),
                    );
                    state
                }
                None => EnvMap::empty(),
            };
            depends_on_match_pattern::<Cx, Fl>(
                cx,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                case_match_root_loc.dupe(),
                &pattern.1,
                state,
            )
        }
        Def::ExpressionDef(box ExpressionDef { expr, hints, .. }) => {
            depends_of_hinted_expression::<Cx, Fl>(
                cx,
                autocomplete_hooks,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                true,
                hints,
                expr,
                EnvMap::empty(),
            )
        }
        Def::Update { .. } => depends_of_update::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            id_loc,
            None,
        ),
        Def::MemberAssign(box MemberAssignData {
            member_loc,
            member,
            rhs,
            ..
        }) => depends_of_member_assign::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            member_loc.dupe(),
            member,
            &rhs.1,
        ),
        Def::OpAssign(box OpAssignData { lhs, rhs, .. }) => depends_of_op_assign::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            id_loc,
            &lhs.1,
            &rhs.1,
        ),
        Def::Function(box FunctionDefData {
            synthesizable_from_annotation,
            arrow: _,
            function_,
            has_this_def: _,
            function_loc: _,
            tparams_map,
            statics,
            namespace_types,
            hints,
        }) => depends_of_fun::<Cx, Fl>(
            cx,
            autocomplete_hooks,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            id_loc,
            synthesizable_from_annotation,
            tparams_map,
            hints,
            statics,
            namespace_types,
            function_,
            EnvMap::empty(),
        ),
        Def::DeclaredFunction(box DeclaredFunctionDefData {
            declarations,
            statics,
            namespace_types,
        }) => depends_of_declared_fun::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            id_loc,
            declarations,
            statics,
            namespace_types,
        ),
        Def::Component(box ComponentDefData {
            tparams_map,
            component,
            component_loc: _,
        }) => depends_of_component::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            tparams_map,
            component,
            EnvMap::empty(),
        ),
        Def::Class(box ClassDefData {
            class_,
            class_loc: _,
            this_super_write_locs: _,
            kind: _,
            namespace_members,
        }) => {
            let state = depends_of_class::<Cx, Fl>(
                cx,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                class_,
            );
            depends_of_node::<Cx, Fl>(
                cx,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                false,
                state,
                |visitor| {
                    for env_key in namespace_members
                        .types
                        .values()
                        .chain(namespace_members.values.values())
                    {
                        visitor.add(id_loc.dupe(), env_key.dupe());
                    }
                },
            )
        }
        Def::Record(box RecordDefData {
            record,
            record_loc: _,
            this_super_write_locs: _,
            defaulted_props: _,
        }) => depends_of_record::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            record,
        ),
        Def::DeclaredClass(box DeclaredClassDefData {
            loc: _,
            decl,
            namespace_members,
        }) => {
            let state = depends_of_declared_class::<Cx, Fl>(
                cx,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                decl,
            );
            depends_of_node::<Cx, Fl>(
                cx,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                false,
                state,
                |visitor| {
                    for env_key in namespace_members
                        .types
                        .values()
                        .chain(namespace_members.values.values())
                    {
                        visitor.add(id_loc.dupe(), env_key.dupe());
                    }
                },
            )
        }
        Def::DeclaredComponent(loc, decl) => depends_of_declared_component::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            loc.dupe(),
            decl,
        ),
        Def::TypeAlias(_, alias) => depends_of_alias::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            alias,
        ),
        Def::OpaqueType(_, alias) => depends_of_opaque::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            alias,
        ),
        Def::TypeParam(box TypeParamData {
            tparams_map,
            kind: _,
            implied_bound: _,
            tparam,
        }) => depends_of_tparam::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            tparams_map,
            tparam,
        ),
        Def::Interface(_, inter) => depends_of_interface::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            inter,
        ),
        Def::GeneratorNext(box Some(gen_annot)) => {
            let return_annot_struct = ast::types::Annotation {
                loc: gen_annot.return_annot.0.dupe(),
                annotation: gen_annot.return_annot.1.clone(),
            };
            depends_of_annotation::<Cx, Fl>(
                cx,
                this_super_dep_loc_map,
                env_values,
                env_entries,
                providers,
                declare_namespace_read_paths,
                refinement_of_id,
                &gen_annot.tparams_map,
                &return_annot_struct,
                EnvMap::empty(),
            )
        }
        Def::DeclaredNamespace(_, ns) => depends_of_declared_namespace::<Cx, Fl>(
            cx,
            this_super_dep_loc_map,
            env_values,
            env_entries,
            providers,
            declare_namespace_read_paths,
            refinement_of_id,
            ns,
        ),
        Def::GeneratorNext(box None) => EnvMap::empty(),
        Def::Enum { .. } => EnvMap::empty(),
        Def::Import(_) => EnvMap::empty(),
        Def::MissingThisAnnot => EnvMap::empty(),
    }
}

// Is the variable defined by this def able to be recursively depended on
// e.g. created as a 0->1 tvar before being resolved?
fn recursively_resolvable(def: &Def) -> bool {
    use flow_env_builder::name_def_types::Binding;
    use flow_env_builder::name_def_types::ExpressionDef;
    use flow_env_builder::name_def_types::FunctionSynthKind;
    use flow_env_builder::name_def_types::Import;

    fn bind_loop(b: &Binding) -> bool {
        use flow_env_builder::name_def_types::ObjectSynthKind;
        use flow_env_builder::name_def_types::Root;
        use flow_env_builder::selector::Selector;
        match b {
            Binding::Root(Root::CatchUnannotated) => true,
            Binding::Root(Root::DeclareVariableMissingAnnotationAndInit) => true,
            Binding::Root(Root::UnannotatedParameter { .. }) => true,
            Binding::Root(Root::Annotation(_)) => true,
            Binding::Root(Root::ObjectValue(box ObjectValueData {
                synthesizable: ObjectSynthKind::ObjectSynthesizable { .. },
                ..
            })) => true,
            Binding::Root(
                Root::For(..)
                | Root::Value(_)
                | Root::MatchCaseRoot(_)
                | Root::FunctionValue(_)
                | Root::Contextual(_)
                | Root::EmptyArray(_)
                | Root::ObjectValue(_),
            ) => false,
            Binding::Select {
                selector: Selector::Computed { .. },
                ..
            } => false,
            Binding::Select { parent, .. } => bind_loop(&parent.1),
            Binding::Hooklike(binding) => bind_loop(binding),
        }
    }

    fn expression_resolvable(expr: &ast::expression::Expression<ALoc, ALoc>) -> bool {
        // A variable read or member expression is assumed to be recursively resolvable if the
        // write that reaches the read is also resolvable, and we can extend this to
        // ExpressionDef nodes as long as they only contain such expressions. These nodes will
        // always depend on the definition of the variable or member, and if the definitions
        // are not resolvable, the entire component won't be either
        match expr.deref() {
            ExpressionInner::StringLiteral { .. }
            | ExpressionInner::NumberLiteral { .. }
            | ExpressionInner::BooleanLiteral { .. }
            | ExpressionInner::NullLiteral { .. }
            | ExpressionInner::BigIntLiteral { .. }
            | ExpressionInner::RegExpLiteral { .. }
            | ExpressionInner::ModuleRefLiteral { .. }
            | ExpressionInner::Identifier { .. } => true,
            ExpressionInner::Member { inner, .. }
                if matches!(
                    inner.property,
                    ast::expression::member::Property::PropertyIdentifier(_)
                ) =>
            {
                expression_resolvable(&inner.object)
            }
            _ => false,
        }
    }

    match def {
        Def::Binding(bind) => bind_loop(bind),
        Def::ExpressionDef(box ExpressionDef { hints, expr, .. }) if hints.is_empty() => {
            expression_resolvable(expr)
        }
        Def::GeneratorNext(_)
        | Def::TypeAlias(..)
        | Def::OpaqueType(..)
        | Def::TypeParam(_)
        | Def::Interface(..)
        // Imports are academic here since they can't be in a cycle anyways, since they depend on nothing
        | Def::Import(box ImportData {
            import_kind: ast::statement::ImportKind::ImportType,
            ..
        })
        | Def::Import(box ImportData {
            import_kind: ast::statement::ImportKind::ImportTypeof,
            ..
        })
        | Def::Import(box ImportData {
            import:
                Import::Named {
                    kind: Some(ast::statement::ImportKind::ImportType),
                    ..
                },
            ..
        })
        | Def::Import(box ImportData {
            import:
                Import::Named {
                    kind: Some(ast::statement::ImportKind::ImportTypeof),
                    ..
                },
            ..
        })
        | Def::Class(_)
        | Def::Record(_)
        | Def::MissingThisAnnot
        | Def::DeclaredComponent(..)
        | Def::DeclaredClass(..)
        | Def::DeclaredFunction(..)
        | Def::DeclaredNamespace(..)
        | Def::Function(box FunctionDefData {
            synthesizable_from_annotation: FunctionSynthKind::FunctionSynthesizable,
            ..
        })
        | Def::Component(_) => true,
        Def::MatchCasePattern(_) | Def::ExpressionDef(_)
        | Def::Update { .. }
        | Def::MemberAssign(_)
        | Def::OpAssign(_)
        | Def::Function(_)
        | Def::Enum { .. }
        | Def::Import(_) => false,
    }
}

fn annotation_locs(
    scopes: &flow_analysis::scope_api::ScopeInfo<ALoc>,
    providers: &provider_api::Info<ALoc>,
    kind: DefLocType,
    loc: ALoc,
    def: &Def,
) -> Vec<AnnotLoc<ALoc>> {
    use flow_env_builder::name_def_types::Binding;
    use flow_env_builder::name_def_types::FunctionSynthKind;

    fn bind_loop(
        scopes: &flow_analysis::scope_api::ScopeInfo<ALoc>,
        providers: &provider_api::Info<ALoc>,
        loc: &ALoc,
        b: &Binding,
    ) -> Vec<AnnotLoc<ALoc>> {
        use flow_env_builder::find_providers::State;
        use flow_env_builder::name_def_types::FunctionSynthKind;
        use flow_env_builder::name_def_types::ObjectMissingAnnot;
        use flow_env_builder::name_def_types::ObjectSynthKind;
        use flow_env_builder::name_def_types::Root;

        match b {
            Binding::Root(Root::CatchUnannotated)
            | Binding::Root(Root::DeclareVariableMissingAnnotationAndInit)
            | Binding::Root(Root::UnannotatedParameter { .. })
            | Binding::Root(Root::Annotation(_))
            | Binding::Root(Root::ObjectValue(box ObjectValueData {
                synthesizable: ObjectSynthKind::ObjectSynthesizable { .. },
                ..
            })) => vec![],

            Binding::Root(Root::FunctionValue(box FunctionValueData {
                synthesizable_from_annotation: FunctionSynthKind::MissingReturn(ret_loc),
                ..
            })) => vec![AnnotLoc::Loc(ret_loc.dupe())],

            Binding::Root(Root::ObjectValue(box ObjectValueData {
                synthesizable: ObjectSynthKind::MissingMemberAnnots { locs },
                ..
            })) => {
                let mut functions: Vec<AnnotLoc<ALoc>> = Vec::new();
                let mut others: Vec<ALoc> = Vec::new();
                for annot in locs.iter() {
                    match annot {
                        ObjectMissingAnnot::OtherMissingAnnot(l) => {
                            others.push(l.dupe());
                        }
                        ObjectMissingAnnot::FuncMissingAnnot(l) => {
                            functions.push(AnnotLoc::Loc(l.dupe()));
                        }
                    }
                }

                if others.is_empty() {
                    functions
                } else {
                    let Some(def_providers) = providers.providers_of_def(loc) else {
                        return functions;
                    };
                    match def_providers.state {
                        State::AnnotatedVar { contextual: false } => functions,
                        _ => {
                            let Some(scope_def) = scopes.def_of_use_opt(loc) else {
                                return functions;
                            };
                            let def_loc = scope_def.locs.first().dupe();
                            let mut result = functions;
                            result.push(AnnotLoc::Object {
                                loc: def_loc,
                                props: others,
                            });
                            result
                        }
                    }
                }
            }

            Binding::Root(
                Root::For(..)
                | Root::Value(_)
                | Root::MatchCaseRoot(_)
                | Root::FunctionValue(_)
                | Root::Contextual(_)
                | Root::EmptyArray(_)
                | Root::ObjectValue(_),
            ) => {
                let Some(def_providers) = providers.providers_of_def(loc) else {
                    return vec![];
                };
                match def_providers.state {
                    State::AnnotatedVar { contextual: false } => vec![],
                    _ => {
                        let Some(scope_def) = scopes.def_of_use_opt(loc) else {
                            return vec![];
                        };
                        let def_loc = scope_def.locs.first().dupe();
                        vec![AnnotLoc::Loc(def_loc)]
                    }
                }
            }

            Binding::Hooklike(binding) => bind_loop(scopes, providers, loc, binding),
            Binding::Select { .. } => vec![],
        }
    }

    if kind == DefLocType::PatternLoc {
        return vec![];
    }

    match def {
        Def::Binding(bind) => bind_loop(scopes, providers, &loc, bind),
        Def::GeneratorNext(box None) => vec![AnnotLoc::Loc(loc)],
        Def::Function(box FunctionDefData {
            synthesizable_from_annotation: FunctionSynthKind::MissingReturn(ret_loc),
            ..
        }) => vec![AnnotLoc::Loc(ret_loc.dupe())],
        Def::Component(_)
        | Def::TypeAlias(..)
        | Def::OpaqueType(..)
        | Def::TypeParam(_)
        | Def::Function(_)
        | Def::Interface(..)
        | Def::Enum { .. }
        | Def::Import(_)
        | Def::Class(_)
        | Def::Record(_)
        | Def::DeclaredClass(..)
        | Def::DeclaredFunction(..)
        | Def::DeclaredComponent(..)
        | Def::MatchCasePattern(_)
        | Def::ExpressionDef(_)
        | Def::DeclaredNamespace(..)
        | Def::MissingThisAnnot
        | Def::GeneratorNext(box Some(_)) => vec![],
        // TODO
        Def::Update { .. } | Def::MemberAssign(_) | Def::OpAssign(_) => vec![],
    }
}

fn dependencies<Cx: Context, F: Flow<Cx = Cx>>(
    cx: &mut Cx,
    autocomplete_hooks: &AutocompleteHooks<ALoc>,
    this_super_dep_loc_map: &EnvMap<ALoc, EnvKey<ALoc>>,
    env: &EnvInfo<ALoc>,
    key: EnvKey<ALoc>,
    def: &Def,
    acc: &mut EnvMap<ALoc, EnvMap<ALoc, Vec1<ALoc>>>,
) -> std::result::Result<(), Box<ErrorMessage<ALoc>>> {
    let depends_result = depends::<Cx, F>(
        cx,
        autocomplete_hooks,
        this_super_dep_loc_map,
        &env.env_values,
        &env.env_entries,
        &env.providers,
        &env.declare_namespace_read_paths,
        &*env.refinement_of_id,
        key.def_loc_type,
        key.loc.dupe(),
        def,
    );
    if acc.contains_key(&key) {
        return Err(Box::new(ErrorMessage::EInternal(Box::new((
            key.loc.dupe(),
            InternalError::EnvInvariant(EnvInvariantFailure::Impossible(
                "Duplicate name defs for the same location".into(),
            )),
        )))));
    }
    acc.insert(key, depends_result);
    Ok(())
}

pub fn build_graph<A: Clone, B: Clone, Cx: Context, F: Flow<Cx = Cx>>(
    cx: &mut Cx,
    autocomplete_hooks: &AutocompleteHooks<ALoc>,
    env: &EnvInfo<ALoc>,
    map: &EnvMap<ALoc, (Def, A, B, VirtualReason<ALoc>, DefinitionReferenceKind)>,
) -> std::result::Result<EnvMap<ALoc, EnvMap<ALoc, Vec1<ALoc>>>, Box<ErrorMessage<ALoc>>> {
    use flow_env_builder::name_def_types::ObjectSynthKind;

    // This is a forwarding map from the def loc of this and super to the def loc of the functions
    // and classes that define this and super. We need this forwarding mechanism, because this and
    // super are not write entries that will be resolved by env_resolution. Instead, they are
    // indirectly resolved when resolving their defining functions and classes. Therefore, when
    // we see a read of `this`/`super`, instead of saying it depends on the write of `this`/`super`,
    // we use this forwarding map to say it actually depends on the functions/classes that define
    // `this`/`super`.
    let mut this_super_dep_loc_map: EnvMap<ALoc, EnvKey<ALoc>> = EnvMap::empty();
    for (kind_and_loc, (def, _, _, _, _)) in map.iter() {
        match def {
            Def::Class(box ClassDefData {
                this_super_write_locs: locs,
                ..
            })
            | Def::Record(box RecordDefData {
                this_super_write_locs: locs,
                ..
            }) => {
                for this_super_kind_and_loc in locs.iter() {
                    this_super_dep_loc_map
                        .insert(this_super_kind_and_loc.dupe(), kind_and_loc.dupe());
                }
            }
            Def::Binding(box Binding::Root(Root::ObjectValue(box ObjectValueData {
                synthesizable:
                    ObjectSynthKind::ObjectSynthesizable {
                        this_write_locs, ..
                    },
                ..
            }))) => {
                for this_super_kind_and_loc in this_write_locs.iter() {
                    this_super_dep_loc_map
                        .insert(this_super_kind_and_loc.dupe(), kind_and_loc.dupe());
                }
            }
            _ => {}
        }
    }

    let mut result: EnvMap<ALoc, EnvMap<ALoc, Vec1<ALoc>>> = EnvMap::empty();
    for (key, (def, _, _, _, _)) in map.iter() {
        dependencies::<Cx, F>(
            cx,
            autocomplete_hooks,
            &this_super_dep_loc_map,
            env,
            key.dupe(),
            def,
            &mut result,
        )?;
    }
    Ok(result)
}

pub fn build_ordering<A: Clone, B: Clone, Cx: Context, F: Flow<Cx = Cx>>(
    cx: &mut Cx,
    autocomplete_hooks: &AutocompleteHooks<ALoc>,
    env: &EnvInfo<ALoc>,
    map: &EnvMap<ALoc, (Def, A, B, VirtualReason<ALoc>, DefinitionReferenceKind)>,
) -> std::result::Result<Vec<OrderingResult>, Box<ErrorMessage<ALoc>>> {
    fn env_map_find<'a, V: Clone>(
        k: &EnvKey<ALoc>,
        map: &'a EnvMap<ALoc, V>,
    ) -> std::result::Result<&'a V, Box<ErrorMessage<ALoc>>> {
        match map.get(k) {
            Some(t) => Ok(t),
            None => Err(Box::new(ErrorMessage::EInternal(Box::new((
                k.loc.dupe(),
                InternalError::EnvInvariant(EnvInvariantFailure::NameDefGraphMismatch),
            ))))),
        }
    }

    let scopes = &env.scopes;
    let providers = &env.providers;

    let graph = build_graph::<A, B, Cx, F>(cx, autocomplete_hooks, env, map)?;
    let order_graph: EnvMap<ALoc, EnvSet<ALoc>> = {
        let mut result = EnvMap::empty();
        for (k, deps) in graph.iter() {
            let mut set = EnvSet::empty();
            for key in deps.keys() {
                set.insert(key.dupe());
            }
            result.insert(k.dupe(), set);
        }
        result
    };
    let roots: EnvSet<ALoc> = {
        let mut set = EnvSet::empty();
        for key in order_graph.keys() {
            set.insert(key.dupe());
        }
        set
    };

    let tarjan_graph: BTreeMap<EnvKey<ALoc>, BTreeSet<EnvKey<ALoc>>> = order_graph
        .iter()
        .map(|(k, v)| (k.dupe(), v.iter().cloned().collect()))
        .collect();
    let tarjan_graph = Graph::of_map(tarjan_graph);

    // The topsort returns components in reverse topological order, so we reverse to get
    // dependencies before dependents.
    let mut sort_result: Vec<Vec1<EnvKey<ALoc>>> = topsort(roots.iter().duped(), &tarjan_graph);
    sort_result.reverse();
    let all_deps: EnvSet<ALoc> = {
        let mut set = EnvSet::empty();
        for deps in order_graph.values() {
            for key in deps.iter() {
                set.insert(key.dupe());
            }
        }
        set
    };
    let missing_roots: Vec<ALoc> = all_deps
        .iter()
        .filter(|l| !roots.contains(l))
        .map(|l| l.loc.dupe())
        .collect();
    if !missing_roots.is_empty() {
        let all: Vec<ALoc> = all_deps.iter().map(|l| l.loc.dupe()).collect();
        let roots_locs: Vec<ALoc> = roots.iter().map(|l| l.loc.dupe()).collect();
        let error_loc = missing_roots[0].dupe();
        return Err(Box::new(ErrorMessage::EInternal(Box::new((
            error_loc,
            InternalError::EnvInvariant(EnvInvariantFailure::NameDefOrderingFailure {
                all,
                missing_roots,
                roots: roots_locs,
            }),
        )))));
    }

    let mut results: Vec<OrderingResult> = Vec::new();
    for component in sort_result {
        let (fst, rest): (EnvKey<ALoc>, Vec<EnvKey<ALoc>>) = {
            let err_loc = component.first().loc.dupe();
            let mut iter = component.into_iter();
            let first = iter.next().ok_or_else(|| {
                Box::new(ErrorMessage::EInternal(Box::new((
                    err_loc,
                    InternalError::EnvInvariant(EnvInvariantFailure::Impossible(
                        "component should be non-empty".into(),
                    )),
                ))))
            })?;
            (first, iter.collect())
        };

        let element_of_loc =
            |key: &EnvKey<ALoc>| -> std::result::Result<Element, Box<ErrorMessage<ALoc>>> {
                let (def, _, _, reason, definition_kind) = env_map_find(key, map)?;
                let self_deps = env_map_find(key, &order_graph)?;
                if self_deps.contains(key) {
                    if recursively_resolvable(def) {
                        Ok(Element::Resolvable(key.dupe()))
                    } else {
                        let depends = env_map_find(key, &graph)?;
                        let self_recursion = env_map_find(key, depends)?;
                        let recursion: Vec<ALoc> =
                            self_recursion.iter().map(|l| l.dupe()).collect();
                        Ok(Element::Illegal(Blame {
                            payload: key.dupe(),
                            definition: DefinitionReferenceData {
                                loc: reason.loc().dupe(),
                                kind: definition_kind.dupe(),
                            },
                            recursion,
                            annot_locs: annotation_locs(
                                scopes,
                                providers,
                                key.def_loc_type,
                                key.loc.dupe(),
                                def,
                            ),
                        }))
                    }
                } else {
                    Ok(Element::Normal(key.dupe()))
                }
            };

        if rest.is_empty() {
            results.push(OrderingResult::Singleton(element_of_loc(&fst)?));
        } else {
            let component_list: Vec<EnvKey<ALoc>> = std::iter::once(fst.dupe())
                .chain(rest.iter().cloned())
                .collect();

            let all_resolvable = component_list.iter().all(|m| {
                if let Ok((def, _, _, _, _)) = env_map_find(m, map) {
                    recursively_resolvable(def)
                } else {
                    false
                }
            });

            if all_resolvable {
                let mut elements = Vec::new();
                for m in &component_list {
                    elements.push(element_of_loc(m)?);
                }
                let elements = Vec1::try_from_vec(elements).map_err(|_| {
                    Box::new(ErrorMessage::EInternal(Box::new((
                        fst.loc.dupe(),
                        InternalError::EnvInvariant(EnvInvariantFailure::Impossible(
                            "component should be non-empty".into(),
                        )),
                    ))))
                })?;
                results.push(OrderingResult::ResolvableSCC(elements));
            } else {
                // BFS search
                fn shortest_cycle(
                    targ: &EnvKey<ALoc>,
                    order_graph: &EnvMap<ALoc, EnvSet<ALoc>>,
                ) -> Option<Vec<EnvKey<ALoc>>> {
                    let mut seen: BTreeSet<EnvKey<ALoc>> = BTreeSet::new();
                    let mut parents: BTreeMap<EnvKey<ALoc>, EnvKey<ALoc>> = BTreeMap::new();
                    let mut q: VecDeque<EnvKey<ALoc>> = VecDeque::new();
                    q.push_back(targ.dupe());

                    // BFS loop
                    while let Some(cur) = q.pop_front() {
                        let adj = match order_graph.get(&cur) {
                            Some(adj) => adj,
                            None => continue,
                        };
                        if adj.contains(targ) {
                            fn path(
                                x: EnvKey<ALoc>,
                                parents: &BTreeMap<EnvKey<ALoc>, EnvKey<ALoc>>,
                            ) -> Vec<EnvKey<ALoc>> {
                                match parents.get(&x) {
                                    Some(y) => {
                                        let mut result = path(y.dupe(), parents);
                                        result.push(x.dupe());
                                        result
                                    }
                                    None => vec![x.dupe()],
                                }
                            }
                            let mut cycle = vec![targ.dupe()];
                            cycle.extend(path(cur, &parents));
                            return Some(cycle);
                        } else {
                            seen.insert(cur.dupe());
                            for next in adj.iter() {
                                if !seen.contains(next) {
                                    q.push_back(next.dupe());
                                    parents.insert(next.dupe(), cur.dupe());
                                }
                            }
                        }
                    }
                    None
                }

                let mut cycle_elts: BTreeSet<EnvKey<ALoc>> = BTreeSet::new();
                for payload in &component_list {
                    if let Some((def, _, _, _, _)) = map.get(payload) {
                        if !recursively_resolvable(def) {
                            if let Some(cycle) = shortest_cycle(payload, &order_graph) {
                                for elt in cycle {
                                    cycle_elts.insert(elt);
                                }
                            }
                        }
                    }
                }

                let mut elements: Vec<(Blame<Element>, bool)> = Vec::new();
                for key in &component_list {
                    let (def, _, _, reason, definition_kind) = env_map_find(key, map)?;
                    let depends = env_map_find(key, &graph)?;
                    let edges: Vec<ALoc> = {
                        let mut acc: Vec<ALoc> = Vec::new();
                        for (k, v) in depends.iter() {
                            if k.loc != key.loc && cycle_elts.contains(k) {
                                let rev_v: Vec<ALoc> = v
                                    .iter()
                                    .rev()
                                    .filter(|l| **l != key.loc)
                                    .map(|l| l.dupe())
                                    .collect();
                                let mut new_acc = rev_v;
                                new_acc.extend(acc);
                                acc = new_acc;
                            }
                        }
                        acc
                    };
                    let display = cycle_elts.contains(key);
                    let blame = Blame {
                        payload: element_of_loc(key)?,
                        definition: DefinitionReferenceData {
                            loc: reason.loc().dupe(),
                            kind: definition_kind.dupe(),
                        },
                        recursion: edges,
                        annot_locs: annotation_locs(
                            scopes,
                            providers,
                            key.def_loc_type,
                            key.loc.dupe(),
                            def,
                        ),
                    };
                    elements.push((blame, display));
                }

                let elements = Vec1::try_from_vec(elements).map_err(|_| {
                    Box::new(ErrorMessage::EInternal(Box::new((
                        fst.loc.dupe(),
                        InternalError::EnvInvariant(EnvInvariantFailure::Impossible(
                            "component should be non-empty".into(),
                        )),
                    ))))
                })?;
                results.push(OrderingResult::IllegalSCC(elements));
            }
        }
    }

    Ok(results)
}
