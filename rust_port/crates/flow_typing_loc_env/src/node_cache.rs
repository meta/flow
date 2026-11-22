/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::rc::Rc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocMap;
use flow_data_structure_wrapper::multi_level_map::MultiLevelMap;
use flow_lazy::ContextFamily;
use flow_parser::ast;
use flow_parser::ast::types::TypeParam as AstTypeParam;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeParam;
use flow_typing_type::type_::UnresolvedParam;

use crate::component_sig_types;
use crate::func_class_sig_types;
use crate::match_pattern_ir;

pub type DeferredTypeGuardCheck<CX> = Option<
    Rc<
        dyn for<'a> Fn(
            &mut <CX as ContextFamily>::Context<'a>,
        ) -> Result<(), flow_utils_concurrency::job_error::JobError>,
    >,
>;

struct Cache<CX: ContextFamily + Clone> {
    annotations: ALocMap<ast::types::Annotation<ALoc, (ALoc, Type)>>,
    expressions: MultiLevelMap<ALoc, ast::expression::Expression<ALoc, (ALoc, Type)>>,
    statements: ALocMap<ast::statement::Statement<ALoc, (ALoc, Type)>>,
    jsx_body: MultiLevelMap<
        ALoc,
        (
            Vec<UnresolvedParam>,
            (ALoc, Vec<ast::jsx::Child<ALoc, (ALoc, Type)>>),
        ),
    >,
    functions: ALocMap<(Type, ast::function::Function<ALoc, (ALoc, Type)>)>,
    function_sigs: ALocMap<(
        func_class_sig_types::func::Func<func_class_sig_types::StmtConfigTypes>,
        func_class_sig_types::func::Reconstruct,
        DeferredTypeGuardCheck<CX>,
    )>,
    aliases: ALocMap<(Type, ast::statement::TypeAlias<ALoc, (ALoc, Type)>)>,
    opaques: ALocMap<(Type, ast::statement::OpaqueType<ALoc, (ALoc, Type)>)>,
    interfaces: ALocMap<(Type, ast::statement::Interface<ALoc, (ALoc, Type)>)>,
    declared_classes: ALocMap<(Type, ast::statement::DeclareClass<ALoc, (ALoc, Type)>)>,
    declared_components: ALocMap<(Type, ast::statement::DeclareComponent<ALoc, (ALoc, Type)>)>,
    declared_namespaces: ALocMap<(Type, ast::statement::DeclareNamespace<ALoc, (ALoc, Type)>)>,
    class_sigs: ALocMap<(
        Type,
        Type,
        func_class_sig_types::class::Class<func_class_sig_types::StmtConfigTypes>,
        Rc<
            dyn for<'a> Fn(
                &mut CX::Context<'a>,
                Type,
            ) -> Result<
                ast::class::Class<ALoc, (ALoc, Type)>,
                flow_utils_concurrency::job_error::JobError,
            >,
        >,
    )>,
    record_sigs: ALocMap<(
        Type,
        Type,
        func_class_sig_types::class::Class<func_class_sig_types::StmtConfigTypes>,
        Rc<
            dyn for<'a> Fn(
                &mut CX::Context<'a>,
                Type,
            ) -> Result<
                ast::statement::RecordDeclaration<ALoc, (ALoc, Type)>,
                flow_utils_concurrency::job_error::JobError,
            >,
        >,
    )>,
    tparams: ALocMap<(AstTypeParam<ALoc, (ALoc, Type)>, TypeParam, Type)>,
    component_sigs: ALocMap<(
        component_sig_types::component_sig::ComponentSig,
        component_sig_types::component_sig::ReconstructComponent,
    )>,
    match_patterns: ALocMap<(ast::match_pattern::MatchPattern<ALoc, (ALoc, Type)>, bool)>,
    match_pattern_value_unions: ALocMap<match_pattern_ir::value_union::ValueUnion<CX>>,
    // Intermediate PatternUnion state for incremental building
    match_pattern_unions: ALocMap<(match_pattern_ir::pattern_union::PatternUnion, i32)>,
}

pub struct NodeCache<CX: ContextFamily + Clone>(Cache<CX>);

impl<CX: ContextFamily + Clone> NodeCache<CX> {
    pub fn mk_empty() -> NodeCache<CX> {
        NodeCache(Cache {
            annotations: ALocMap::new(),
            expressions: MultiLevelMap::new(),
            statements: ALocMap::new(),
            jsx_body: MultiLevelMap::new(),
            functions: ALocMap::new(),
            function_sigs: ALocMap::new(),
            aliases: ALocMap::new(),
            opaques: ALocMap::new(),
            interfaces: ALocMap::new(),
            declared_classes: ALocMap::new(),
            declared_components: ALocMap::new(),
            declared_namespaces: ALocMap::new(),
            class_sigs: ALocMap::new(),
            record_sigs: ALocMap::new(),
            tparams: ALocMap::new(),
            component_sigs: ALocMap::new(),
            match_patterns: ALocMap::new(),
            match_pattern_value_unions: ALocMap::new(),
            match_pattern_unions: ALocMap::new(),
        })
    }

    /// Expressions and JSX children cached after this call are dropped by the matching
    /// `pop_level`.
    pub fn push_level(&mut self) {
        self.0.expressions.push_level();
        self.0.jsx_body.push_level();
    }

    pub fn pop_level(&mut self) {
        self.0.expressions.pop_level();
        self.0.jsx_body.pop_level();
    }

    pub fn set_annotation(&mut self, anno: ast::types::Annotation<ALoc, (ALoc, Type)>) {
        let loc = anno.loc.dupe();
        self.0.annotations.insert(loc, anno);
    }

    pub fn set_expression(&mut self, exp: ast::expression::Expression<ALoc, (ALoc, Type)>) {
        let (loc, _) = exp.loc();
        let loc = loc.clone();
        self.0.expressions.insert(loc, exp);
    }

    pub fn set_statement(&mut self, stmt: ast::statement::Statement<ALoc, (ALoc, Type)>) {
        let loc = stmt.loc().clone();
        self.0.statements.insert(loc, stmt);
    }

    pub fn set_jsx_children(
        &mut self,
        children: (
            Vec<UnresolvedParam>,
            (ALoc, Vec<ast::jsx::Child<ALoc, (ALoc, Type)>>),
        ),
    ) {
        let (_, (loc, _)) = &children;
        let loc = loc.clone();
        self.0.jsx_body.insert(loc, children);
    }

    pub fn set_function(
        &mut self,
        loc: ALoc,
        func: (Type, ast::function::Function<ALoc, (ALoc, Type)>),
    ) {
        self.0.functions.insert(loc, func);
    }

    pub fn set_function_sig(
        &mut self,
        loc: ALoc,
        func_sig: (
            func_class_sig_types::func::Func<func_class_sig_types::StmtConfigTypes>,
            func_class_sig_types::func::Reconstruct,
            DeferredTypeGuardCheck<CX>,
        ),
    ) {
        self.0.function_sigs.insert(loc, func_sig);
    }

    pub fn set_alias(
        &mut self,
        loc: ALoc,
        alias: (Type, ast::statement::TypeAlias<ALoc, (ALoc, Type)>),
    ) {
        self.0.aliases.insert(loc, alias);
    }

    pub fn set_opaque(
        &mut self,
        loc: ALoc,
        opaque: (Type, ast::statement::OpaqueType<ALoc, (ALoc, Type)>),
    ) {
        self.0.opaques.insert(loc, opaque);
    }

    pub fn set_interface(
        &mut self,
        loc: ALoc,
        inter: (Type, ast::statement::Interface<ALoc, (ALoc, Type)>),
    ) {
        self.0.interfaces.insert(loc, inter);
    }

    pub fn set_declared_component(
        &mut self,
        loc: ALoc,
        component: (Type, ast::statement::DeclareComponent<ALoc, (ALoc, Type)>),
    ) {
        self.0.declared_components.insert(loc, component);
    }

    pub fn set_declared_class(
        &mut self,
        loc: ALoc,
        class_: (Type, ast::statement::DeclareClass<ALoc, (ALoc, Type)>),
    ) {
        self.0.declared_classes.insert(loc, class_);
    }

    pub fn set_declared_namespace(
        &mut self,
        loc: ALoc,
        ns: (Type, ast::statement::DeclareNamespace<ALoc, (ALoc, Type)>),
    ) {
        self.0.declared_namespaces.insert(loc, ns);
    }

    pub fn set_class_sig(
        &mut self,
        loc: ALoc,
        class_: (
            Type,
            Type,
            func_class_sig_types::class::Class<func_class_sig_types::StmtConfigTypes>,
            Rc<
                dyn for<'a> Fn(
                    &mut CX::Context<'a>,
                    Type,
                ) -> Result<
                    ast::class::Class<ALoc, (ALoc, Type)>,
                    flow_utils_concurrency::job_error::JobError,
                >,
            >,
        ),
    ) {
        self.0.class_sigs.insert(loc, class_);
    }

    pub fn set_record_sig(
        &mut self,
        loc: ALoc,
        record: (
            Type,
            Type,
            func_class_sig_types::class::Class<func_class_sig_types::StmtConfigTypes>,
            Rc<
                dyn for<'a> Fn(
                    &mut CX::Context<'a>,
                    Type,
                ) -> Result<
                    ast::statement::RecordDeclaration<ALoc, (ALoc, Type)>,
                    flow_utils_concurrency::job_error::JobError,
                >,
            >,
        ),
    ) {
        self.0.record_sigs.insert(loc, record);
    }

    pub fn set_tparam(&mut self, param: (AstTypeParam<ALoc, (ALoc, Type)>, TypeParam, Type)) {
        let (ast_tparam, _, _) = &param;
        let loc = ast_tparam.loc.clone();
        self.0.tparams.insert(loc, param);
    }

    pub fn set_component_sig(
        &mut self,
        loc: ALoc,
        c: (
            component_sig_types::component_sig::ComponentSig,
            component_sig_types::component_sig::ReconstructComponent,
        ),
    ) {
        self.0.component_sigs.insert(loc, c);
    }

    pub fn set_match_pattern(
        &mut self,
        loc: ALoc,
        p: (ast::match_pattern::MatchPattern<ALoc, (ALoc, Type)>, bool),
    ) {
        self.0.match_patterns.insert(loc, p);
    }

    pub fn set_match_pattern_value_union(
        &mut self,
        loc: ALoc,
        v: match_pattern_ir::value_union::ValueUnion<CX>,
    ) {
        self.0.match_pattern_value_unions.insert(loc, v);
    }

    pub fn set_match_pattern_union(
        &mut self,
        loc: ALoc,
        v: (match_pattern_ir::pattern_union::PatternUnion, i32),
    ) {
        self.0.match_pattern_unions.insert(loc, v);
    }

    pub fn get_annotation(&self, loc: &ALoc) -> Option<ast::types::Annotation<ALoc, (ALoc, Type)>> {
        self.0.annotations.get(loc).cloned()
    }

    pub fn get_expression(
        &self,
        loc: &ALoc,
    ) -> Option<ast::expression::Expression<ALoc, (ALoc, Type)>> {
        self.0.expressions.get(loc).cloned()
    }

    pub fn get_statement(
        &self,
        loc: &ALoc,
    ) -> Option<ast::statement::Statement<ALoc, (ALoc, Type)>> {
        self.0.statements.get(loc).cloned()
    }

    pub fn get_jsx_children(
        &self,
        loc: &ALoc,
    ) -> Option<(
        Vec<UnresolvedParam>,
        (ALoc, Vec<ast::jsx::Child<ALoc, (ALoc, Type)>>),
    )> {
        self.0.jsx_body.get(loc).cloned()
    }

    pub fn get_function_sig(
        &self,
        loc: &ALoc,
    ) -> Option<(
        func_class_sig_types::func::Func<func_class_sig_types::StmtConfigTypes>,
        func_class_sig_types::func::Reconstruct,
        DeferredTypeGuardCheck<CX>,
    )> {
        self.0.function_sigs.get(loc).cloned()
    }

    pub fn get_function(
        &self,
        loc: &ALoc,
    ) -> Option<(Type, ast::function::Function<ALoc, (ALoc, Type)>)> {
        self.0.functions.get(loc).cloned()
    }

    pub fn get_alias(
        &self,
        loc: &ALoc,
    ) -> Option<(Type, ast::statement::TypeAlias<ALoc, (ALoc, Type)>)> {
        self.0.aliases.get(loc).cloned()
    }

    pub fn get_opaque(
        &self,
        loc: &ALoc,
    ) -> Option<(Type, ast::statement::OpaqueType<ALoc, (ALoc, Type)>)> {
        self.0.opaques.get(loc).cloned()
    }

    pub fn get_interface(
        &self,
        loc: &ALoc,
    ) -> Option<(Type, ast::statement::Interface<ALoc, (ALoc, Type)>)> {
        self.0.interfaces.get(loc).cloned()
    }

    pub fn get_declared_class(
        &self,
        loc: &ALoc,
    ) -> Option<(Type, ast::statement::DeclareClass<ALoc, (ALoc, Type)>)> {
        self.0.declared_classes.get(loc).cloned()
    }

    pub fn get_declared_component(
        &self,
        loc: &ALoc,
    ) -> Option<(Type, ast::statement::DeclareComponent<ALoc, (ALoc, Type)>)> {
        self.0.declared_components.get(loc).cloned()
    }

    pub fn get_declared_namespace(
        &self,
        loc: &ALoc,
    ) -> Option<(Type, ast::statement::DeclareNamespace<ALoc, (ALoc, Type)>)> {
        self.0.declared_namespaces.get(loc).cloned()
    }

    pub fn get_class_sig(
        &self,
        loc: &ALoc,
    ) -> Option<(
        Type,
        Type,
        func_class_sig_types::class::Class<func_class_sig_types::StmtConfigTypes>,
        Rc<
            dyn for<'a> Fn(
                &mut CX::Context<'a>,
                Type,
            ) -> Result<
                ast::class::Class<ALoc, (ALoc, Type)>,
                flow_utils_concurrency::job_error::JobError,
            >,
        >,
    )> {
        self.0.class_sigs.get(loc).cloned()
    }

    pub fn get_record_sig(
        &self,
        loc: &ALoc,
    ) -> Option<(
        Type,
        Type,
        func_class_sig_types::class::Class<func_class_sig_types::StmtConfigTypes>,
        Rc<
            dyn for<'a> Fn(
                &mut CX::Context<'a>,
                Type,
            ) -> Result<
                ast::statement::RecordDeclaration<ALoc, (ALoc, Type)>,
                flow_utils_concurrency::job_error::JobError,
            >,
        >,
    )> {
        self.0.record_sigs.get(loc).cloned()
    }

    pub fn get_tparam(
        &self,
        loc: &ALoc,
    ) -> Option<(AstTypeParam<ALoc, (ALoc, Type)>, TypeParam, Type)> {
        self.0.tparams.get(loc).cloned()
    }

    pub fn get_component_sig(
        &self,
        loc: &ALoc,
    ) -> Option<(
        component_sig_types::component_sig::ComponentSig,
        component_sig_types::component_sig::ReconstructComponent,
    )> {
        self.0.component_sigs.get(loc).cloned()
    }

    pub fn get_match_pattern(
        &self,
        loc: &ALoc,
    ) -> Option<(ast::match_pattern::MatchPattern<ALoc, (ALoc, Type)>, bool)> {
        self.0.match_patterns.get(loc).cloned()
    }

    pub fn get_match_pattern_value_union(
        &self,
        loc: &ALoc,
    ) -> Option<match_pattern_ir::value_union::ValueUnion<CX>> {
        self.0.match_pattern_value_unions.get(loc).cloned()
    }

    pub fn get_match_pattern_union(
        &self,
        loc: &ALoc,
    ) -> Option<(match_pattern_ir::pattern_union::PatternUnion, i32)> {
        self.0.match_pattern_unions.get(loc).cloned()
    }
}
