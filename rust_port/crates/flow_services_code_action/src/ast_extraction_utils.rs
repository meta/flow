/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Port of `ast_extraction_utils.rs`

pub mod insertion_point_collectors {
    use dupe::Dupe;
    use dupe::IterDupedExt;
    use flow_aloc::ALoc;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_parser::ast;
    use flow_parser::loc::Loc;
    use flow_parser::polymorphic_ast_mapper::LocMapper;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeParam;

    pub struct FunctionInsertionPoint {
        pub function_name: FlowSmolStr,
        pub body_loc: Loc,
        pub is_method: bool,
        pub tparams_rev: Vec<TypeParam>,
    }

    #[derive(Clone)]
    pub struct ClassInsertionPoint {
        pub class_name: Option<FlowSmolStr>,
        pub body_loc: Loc,
        pub tparams_rev: Vec<TypeParam>,
    }

    fn not_this_typeparam(tp: &TypeParam) -> bool {
        !tp.is_this
    }

    // Extracted as a standalone function since Rust lacks class/multiple inheritance.
    pub fn make_typeparam(tparam: &ast::types::TypeParam<ALoc, (ALoc, Type)>) -> TypeParam {
        use flow_common::reason;
        use flow_common::reason::VirtualReasonDesc;
        use flow_common::subst_name::SubstName;
        use flow_typing_type::type_::TypeParamInner;
        use flow_typing_utils::typed_ast_utils;

        let (name_loc, _) = &tparam.name.loc;
        let name = &tparam.name.name;
        let reason =
            reason::mk_annot_reason(VirtualReasonDesc::RType(name.dupe()), name_loc.dupe());
        let bound = match &tparam.bound {
            ast::types::AnnotationOrHint::Missing((_, t)) => t.dupe(),
            ast::types::AnnotationOrHint::Available(annotation) => {
                let (_, t) = annotation.annotation.loc();
                t.dupe()
            }
        };
        let default = tparam.default.as_ref().map(|ty| {
            let (_, t) = ty.loc();
            t.dupe()
        });
        TypeParam::new(TypeParamInner {
            reason,
            name: SubstName::name(name.dupe()),
            bound,
            polarity: tparam
                .variance
                .as_ref()
                .map(|variance| typed_ast_utils::polarity(Some(variance))),
            default,
            is_this: false,
            is_const: tparam.const_.is_some(),
        })
    }

    pub fn make_class_this(cls: &ast::class::Class<ALoc, (ALoc, Type)>) -> TypeParam {
        use flow_common::polarity::Polarity;
        use flow_common::reason;
        use flow_common::reason::VirtualReasonDesc;
        use flow_common::subst_name::SubstName;
        use flow_data_structure_wrapper::smol_str::FlowSmolStr;
        use flow_typing_type::type_::TypeParamInner;
        use flow_typing_type::type_::mixed_t;
        use flow_typing_type::type_util;

        let body_loc = &cls.body.loc;
        let bound = match &cls.id {
            Some(id) => {
                let (_, t) = &id.loc;
                t.dupe()
            }
            None => {
                let reason = reason::mk_reason(None, body_loc.dupe());
                mixed_t::make(reason)
            }
        };
        TypeParam::new(TypeParamInner {
            name: SubstName::name(FlowSmolStr::new("this")),
            reason: type_util::reason_of_t(&bound)
                .dupe()
                .replace_desc(VirtualReasonDesc::RThisType),
            bound,
            polarity: Some(Polarity::Positive),
            default: None,
            is_this: true,
            is_const: false,
        })
    }

    // Combined collector: traverses typed AST with type parameter tracking,
    // intercepts function/method/class declarations to find insertion points.
    struct FunctionMethodCollector<'a> {
        loc_of_aloc: &'a dyn Fn(&ALoc) -> Loc,
        extracted_loc: Loc,
        rev_bound_tparams: Vec<TypeParam>,
        acc: Vec<FunctionInsertionPoint>,
    }

    impl<'a> FunctionMethodCollector<'a> {
        fn function_with_name(
            &mut self,
            name: Option<&ast::Identifier<ALoc, (ALoc, Type)>>,
            is_method: bool,
            function_declaration: &ast::function::Function<ALoc, (ALoc, Type)>,
        ) {
            if let ast::function::Body::BodyBlock((block_loc, _)) = &function_declaration.body {
                let id = &function_declaration.id;
                let tparams = &function_declaration.tparams;
                let fn_name = match (id, name) {
                    (None, None) => return,
                    (Some(id), _) => id.name.dupe(),
                    (None, Some(name)) => name.name.dupe(),
                };
                let saved_tparams = self.rev_bound_tparams.clone();
                // Process type params: add them to rev_bound_tparams
                if let Some(tp) = tparams {
                    for param in tp.params.iter() {
                        let tparam = make_typeparam(param);
                        self.rev_bound_tparams.insert(0, tparam);
                    }
                }
                let body_loc = (self.loc_of_aloc)(block_loc);
                if body_loc.contains(&self.extracted_loc) {
                    self.acc.insert(
                        0,
                        FunctionInsertionPoint {
                            function_name: fn_name.dupe(),
                            body_loc,
                            is_method,
                            tparams_rev: self
                                .rev_bound_tparams
                                .iter()
                                .filter(|tp| not_this_typeparam(tp))
                                .duped()
                                .collect(),
                        },
                    );
                }
                // Restore tparams
                self.rev_bound_tparams = saved_tparams;
            }
        }
    }

    impl<'a> LocMapper<ALoc, (ALoc, Type), ALoc, (ALoc, Type), !> for FunctionMethodCollector<'a> {
        fn on_loc_annot(&mut self, loc: &ALoc) -> Result<ALoc, !> {
            Ok(loc.dupe())
        }

        fn on_type_annot(&mut self, x: &(ALoc, Type)) -> Result<(ALoc, Type), !> {
            Ok(x.dupe())
        }
    }

    // We need custom traversal to intercept function_, variable_declarator,
    // class_property, class_method, and class_ (for type param tracking).
    // In OCaml, the collector inherits from Flow_polymorphic_ast_mapper.mapper and
    // overrides specific methods. The mapper's super#statement/super#function_ etc.
    // recursively traverse the AST and dispatch to the overridden methods for nested
    // nodes. We replicate this by recursively walking the AST and intercepting the
    // relevant declarations.
    fn collect_program(
        collector: &mut FunctionMethodCollector<'_>,
        typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    ) {
        // Walk all statements
        for stmt in typed_ast.statements.iter() {
            collect_statement(collector, stmt);
        }
    }

    // Recursively walk all statements within a block/body,
    // dispatching to collect_statement for each.
    fn collect_statements(
        collector: &mut FunctionMethodCollector<'_>,
        stmts: &[ast::statement::Statement<ALoc, (ALoc, Type)>],
    ) {
        for stmt in stmts.iter() {
            collect_statement(collector, stmt);
        }
    }

    fn collect_statement(
        collector: &mut FunctionMethodCollector<'_>,
        stmt: &ast::statement::Statement<ALoc, (ALoc, Type)>,
    ) {
        use ast::statement::StatementInner;
        // Recursively visit statements, intercepting function/class/variable declarations.
        // For each case, we first do the custom interception, then recurse into
        // nested statements (replicating OCaml's super#statement / super#function_ etc.).
        match &**stmt {
            StatementInner::FunctionDeclaration { inner, .. } => {
                collector.function_with_name(None, false, inner);
                collect_function_body(collector, inner);
            }
            // variable declarations
            StatementInner::VariableDeclaration { inner, .. } => {
                for decl in inner.declarations.iter() {
                    if let ast::pattern::Pattern::Identifier { inner: pat_id, .. } = &decl.id {
                        if let Some(init_expr) = &decl.init {
                            use ast::expression::ExpressionInner;
                            match &**init_expr {
                                ExpressionInner::ArrowFunction { inner: f, .. }
                                | ExpressionInner::Function { inner: f, .. } => {
                                    collector.function_with_name(Some(&pat_id.name), false, f);
                                }
                                _ => {}
                            }
                        }
                    }
                    if let Some(init_expr) = &decl.init {
                        collect_expression(collector, init_expr);
                    }
                }
            }
            // class declarations
            StatementInner::ClassDeclaration { inner, .. } => {
                let this_tparam = make_class_this(inner);
                let saved = collector.rev_bound_tparams.clone();
                collector.rev_bound_tparams.insert(0, this_tparam);
                // Add class-level tparams (forward iteration + insert(0,..) to match OCaml :: prepend order)
                if let Some(tp) = &inner.tparams {
                    for param in tp.params.iter() {
                        let tparam = make_typeparam(param);
                        collector.rev_bound_tparams.insert(0, tparam);
                    }
                }
                // Process class body for methods/properties (intercepts + recursion)
                collect_class_body(collector, inner);
                collector.rev_bound_tparams = saved;
            }
            // For block statements, recurse into nested statements
            StatementInner::Block { inner, .. } => {
                collect_statements(collector, &inner.body);
            }
            // if/else
            StatementInner::If { inner, .. } => {
                collect_expression(collector, &inner.test);
                collect_statement(collector, &inner.consequent);
                if let Some(alternate) = &inner.alternate {
                    collect_statement(collector, &alternate.body);
                }
            }
            // while
            StatementInner::While { inner, .. } => {
                collect_expression(collector, &inner.test);
                collect_statement(collector, &inner.body);
            }
            // do-while
            StatementInner::DoWhile { inner, .. } => {
                collect_statement(collector, &inner.body);
                collect_expression(collector, &inner.test);
            }
            // for
            StatementInner::For { inner, .. } => {
                collect_statement(collector, &inner.body);
            }
            // for-in
            StatementInner::ForIn { inner, .. } => {
                collect_expression(collector, &inner.right);
                collect_statement(collector, &inner.body);
            }
            // for-of
            StatementInner::ForOf { inner, .. } => {
                collect_expression(collector, &inner.right);
                collect_statement(collector, &inner.body);
            }
            // switch
            StatementInner::Switch { inner, .. } => {
                collect_expression(collector, &inner.discriminant);
                for case in inner.cases.iter() {
                    collect_statements(collector, &case.consequent);
                }
            }
            // try-catch
            StatementInner::Try { inner, .. } => {
                collect_statements(collector, &inner.block.1.body);
                if let Some(handler) = &inner.handler {
                    collect_statements(collector, &handler.body.1.body);
                }
                if let Some(finalizer) = &inner.finalizer {
                    collect_statements(collector, &finalizer.1.body);
                }
            }
            // labeled
            StatementInner::Labeled { inner, .. } => {
                collect_statement(collector, &inner.body);
            }
            // return
            StatementInner::Return { inner, .. } => {
                if let Some(arg) = &inner.argument {
                    collect_expression(collector, arg);
                }
            }
            // expression statement
            StatementInner::Expression { inner, .. } => {
                collect_expression(collector, &inner.expression);
            }
            StatementInner::With { inner, .. } => {
                collect_expression(collector, &inner.object);
                collect_statement(collector, &inner.body);
            }
            // export default declaration
            StatementInner::ExportDefaultDeclaration { inner, .. } => {
                use ast::statement::export_default_declaration::Declaration;
                match &inner.declaration {
                    // Declaration wraps a Statement (which could be FunctionDeclaration or ClassDeclaration)
                    Declaration::Declaration(stmt) => {
                        collect_statement(collector, stmt);
                    }
                    Declaration::Expression(expr) => {
                        collect_expression(collector, expr);
                    }
                }
            }
            // export named declaration
            StatementInner::ExportNamedDeclaration { inner, .. } => {
                if let Some(decl) = &inner.declaration {
                    collect_statement(collector, decl);
                }
            }
            // For all other statements, no nested content that needs interception
            _ => {}
        }
    }

    // Recurse into function body to find nested declarations.
    // In OCaml, super#function_ calls type_params_opt which adds the function's
    // tparams to rev_bound_tparams during body traversal, then restores.
    fn collect_function_body(
        collector: &mut FunctionMethodCollector<'_>,
        func: &ast::function::Function<ALoc, (ALoc, Type)>,
    ) {
        if let ast::function::Body::BodyBlock((_, block)) = &func.body {
            let saved = collector.rev_bound_tparams.clone();
            if let Some(tp) = &func.tparams {
                for param in tp.params.iter() {
                    let tparam = make_typeparam(param);
                    // OCaml uses :: (prepend) so each tparam goes to front
                    collector.rev_bound_tparams.insert(0, tparam);
                }
            }
            collect_statements(collector, &block.body);
            collector.rev_bound_tparams = saved;
        }
    }

    // Recurse into expressions to find nested function/class expressions.
    // In OCaml, super#expression recursively traverses sub-expressions,
    // dispatching to overridden methods for nested functions/classes.
    fn collect_expression(
        collector: &mut FunctionMethodCollector<'_>,
        expr: &ast::expression::Expression<ALoc, (ALoc, Type)>,
    ) {
        use ast::expression::ExpressionInner;
        match &**expr {
            // Arrow/function expressions may contain nested declarations
            ExpressionInner::ArrowFunction { inner: f, .. }
            | ExpressionInner::Function { inner: f, .. } => {
                collect_function_body(collector, f);
            }
            // Class expressions
            ExpressionInner::Class { inner: cls, .. } => {
                // OCaml uses :: (prepend) for adding to rev_bound_tparams
                let this_tparam = make_class_this(cls);
                let saved = collector.rev_bound_tparams.clone();
                collector.rev_bound_tparams.insert(0, this_tparam);
                if let Some(tp) = &cls.tparams {
                    for param in tp.params.iter() {
                        let tparam = make_typeparam(param);
                        collector.rev_bound_tparams.insert(0, tparam);
                    }
                }
                collect_class_body(collector, cls);
                collector.rev_bound_tparams = saved;
            }
            // Recurse into sub-expressions that may contain nested functions/classes
            ExpressionInner::Assignment { inner, .. } => {
                collect_expression(collector, &inner.right);
            }
            ExpressionInner::Conditional { inner, .. } => {
                collect_expression(collector, &inner.consequent);
                collect_expression(collector, &inner.alternate);
            }
            ExpressionInner::Logical { inner, .. } => {
                collect_expression(collector, &inner.left);
                collect_expression(collector, &inner.right);
            }
            ExpressionInner::Binary { inner, .. } => {
                collect_expression(collector, &inner.left);
                collect_expression(collector, &inner.right);
            }
            ExpressionInner::Unary { inner, .. } => {
                collect_expression(collector, &inner.argument);
            }
            ExpressionInner::Sequence { inner, .. } => {
                for e in inner.expressions.iter() {
                    collect_expression(collector, e);
                }
            }
            ExpressionInner::Call { inner, .. } => {
                collect_expression(collector, &inner.callee);
                for arg in inner.arguments.arguments.iter() {
                    match arg {
                        ast::expression::ExpressionOrSpread::Expression(e) => {
                            collect_expression(collector, e);
                        }
                        ast::expression::ExpressionOrSpread::Spread(s) => {
                            collect_expression(collector, &s.argument);
                        }
                    }
                }
            }
            ExpressionInner::OptionalCall { inner, .. } => {
                collect_expression(collector, &inner.call.callee);
                for arg in inner.call.arguments.arguments.iter() {
                    match arg {
                        ast::expression::ExpressionOrSpread::Expression(e) => {
                            collect_expression(collector, e);
                        }
                        ast::expression::ExpressionOrSpread::Spread(s) => {
                            collect_expression(collector, &s.argument);
                        }
                    }
                }
            }
            ExpressionInner::New { inner, .. } => {
                collect_expression(collector, &inner.callee);
            }
            ExpressionInner::Member { inner, .. } => {
                collect_expression(collector, &inner.object);
            }
            ExpressionInner::OptionalMember { inner, .. } => {
                collect_expression(collector, &inner.member.object);
            }
            ExpressionInner::Array { inner, .. } => {
                for elem in inner.elements.iter() {
                    match elem {
                        ast::expression::ArrayElement::Expression(e) => {
                            collect_expression(collector, e);
                        }
                        ast::expression::ArrayElement::Spread(s) => {
                            collect_expression(collector, &s.argument);
                        }
                        ast::expression::ArrayElement::Hole(_) => {}
                    }
                }
            }
            ExpressionInner::Object { inner, .. } => {
                for prop in inner.properties.iter() {
                    match prop {
                        ast::expression::object::Property::NormalProperty(np) => match np {
                            ast::expression::object::NormalProperty::Init { value, .. } => {
                                collect_expression(collector, value);
                            }
                            ast::expression::object::NormalProperty::Method {
                                value: (_, f),
                                ..
                            }
                            | ast::expression::object::NormalProperty::Get {
                                value: (_, f), ..
                            }
                            | ast::expression::object::NormalProperty::Set {
                                value: (_, f), ..
                            } => {
                                collect_function_body(collector, f);
                            }
                        },
                        ast::expression::object::Property::SpreadProperty(s) => {
                            collect_expression(collector, &s.argument);
                        }
                    }
                }
            }
            ExpressionInner::Yield { inner, .. } => {
                if let Some(arg) = &inner.argument {
                    collect_expression(collector, arg);
                }
            }
            ExpressionInner::TaggedTemplate { inner, .. } => {
                collect_expression(collector, &inner.tag);
            }
            ExpressionInner::JSXElement { inner, .. } => {
                collect_jsx_children(collector, &inner.children.1);
            }
            ExpressionInner::JSXFragment { inner, .. } => {
                collect_jsx_children(collector, &inner.frag_children.1);
            }
            ExpressionInner::TypeCast { inner, .. } => {
                collect_expression(collector, &inner.expression);
            }
            ExpressionInner::AsExpression { inner, .. } => {
                collect_expression(collector, &inner.expression);
            }
            ExpressionInner::AsConstExpression { inner, .. } => {
                collect_expression(collector, &inner.expression);
            }
            ExpressionInner::TSSatisfies { inner, .. } => {
                collect_expression(collector, &inner.expression);
            }
            // Terminals: identifiers, literals, this, super, update, etc.
            _ => {}
        }
    }

    fn collect_jsx_children(
        collector: &mut FunctionMethodCollector<'_>,
        children: &[ast::jsx::Child<ALoc, (ALoc, Type)>],
    ) {
        for child in children.iter() {
            match child {
                ast::jsx::Child::Element { inner, .. } => {
                    collect_jsx_children(collector, &inner.children.1);
                }
                ast::jsx::Child::Fragment { inner, .. } => {
                    collect_jsx_children(collector, &inner.frag_children.1);
                }
                ast::jsx::Child::ExpressionContainer { inner, .. } => {
                    if let ast::jsx::expression_container::Expression::Expression(e) =
                        &inner.expression
                    {
                        collect_expression(collector, e);
                    }
                }
                ast::jsx::Child::SpreadChild { inner, .. } => {
                    collect_expression(collector, &inner.expression);
                }
                ast::jsx::Child::Text { .. } => {}
            }
        }
    }

    fn collect_class_body(
        collector: &mut FunctionMethodCollector<'_>,
        cls: &ast::class::Class<ALoc, (ALoc, Type)>,
    ) {
        use ast::class::BodyElement;
        use ast::expression::ExpressionInner;
        use ast::expression::object;
        let body = &cls.body;
        for element in body.body.iter() {
            match element {
                BodyElement::Method(method) => {
                    // Intercept: collect function_with_name
                    if let object::Key::Identifier(name) = &method.key {
                        let (_, ref f) = method.value;
                        collector.function_with_name(Some(name), true, f);
                    }
                    let (_, ref f) = method.value;
                    collect_function_body(collector, f);
                }
                BodyElement::Property(prop) => {
                    // Intercept: collect function_with_name
                    if let object::Key::Identifier(name) = &prop.key {
                        if let ast::class::property::Value::Initialized(init_expr) = &prop.value {
                            match &**init_expr {
                                ExpressionInner::ArrowFunction { inner: f, .. }
                                | ExpressionInner::Function { inner: f, .. } => {
                                    collector.function_with_name(Some(name), true, f);
                                }
                                _ => {}
                            }
                        }
                    }
                    if let ast::class::property::Value::Initialized(init_expr) = &prop.value {
                        collect_expression(collector, init_expr);
                    }
                }
                // Static blocks contain statements
                BodyElement::StaticBlock(block) => {
                    collect_statements(collector, &block.body);
                }
                _ => {}
            }
        }
    }

    pub fn collect_function_method_inserting_points(
        typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
        loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
        extracted_loc: Loc,
    ) -> Vec<FunctionInsertionPoint> {
        let mut collector = FunctionMethodCollector {
            loc_of_aloc,
            extracted_loc,
            rev_bound_tparams: Vec::new(),
            acc: Vec::new(),
        };
        collect_program(&mut collector, typed_ast);
        collector.acc
    }

    struct ClassCollector<'a> {
        loc_of_aloc: &'a dyn Fn(&ALoc) -> Loc,
        extracted_loc: Loc,
        rev_bound_tparams: Vec<TypeParam>,
        acc: Option<ClassInsertionPoint>,
    }

    impl<'a> LocMapper<ALoc, (ALoc, Type), ALoc, (ALoc, Type), !> for ClassCollector<'a> {
        fn on_loc_annot(&mut self, loc: &ALoc) -> Result<ALoc, !> {
            Ok(loc.dupe())
        }

        fn on_type_annot(&mut self, x: &(ALoc, Type)) -> Result<(ALoc, Type), !> {
            Ok(x.dupe())
        }
    }

    fn class_collector_visit_class(
        collector: &mut ClassCollector<'_>,
        cls: &ast::class::Class<ALoc, (ALoc, Type)>,
    ) {
        let body_aloc = &cls.body.loc;
        let body_loc = (collector.loc_of_aloc)(body_aloc);
        if collector.extracted_loc.contains(&body_loc) {
            // class is nested inside extracted range — stop recursing
        } else if body_loc.contains(&collector.extracted_loc) {
            let id = cls.id.as_ref().map(|id| id.name.dupe());
            let saved = collector.rev_bound_tparams.clone();
            // Add tparams if present, prepending each so the list stays reversed
            if let Some(tp) = &cls.tparams {
                for param in tp.params.iter() {
                    let tparam = make_typeparam(param);
                    collector.rev_bound_tparams.insert(0, tparam);
                }
            }
            collector.acc = Some(ClassInsertionPoint {
                class_name: id,
                body_loc,
                tparams_rev: collector
                    .rev_bound_tparams
                    .iter()
                    .filter(|tp| not_this_typeparam(tp))
                    .duped()
                    .collect(),
            });
            collector.rev_bound_tparams = saved;
            // Continue traversing the class body with the "this" tparam and the class tparams bound
            let this_tparam = make_class_this(cls);
            let saved = collector.rev_bound_tparams.clone();
            collector.rev_bound_tparams.insert(0, this_tparam);
            if let Some(tp) = &cls.tparams {
                for param in tp.params.iter() {
                    let tparam = make_typeparam(param);
                    collector.rev_bound_tparams.insert(0, tparam);
                }
            }
            class_collector_visit_class_body(collector, cls);
            collector.rev_bound_tparams = saved;
        } else {
            // Continue traversing the class body with the "this" tparam and the class tparams bound
            let this_tparam = make_class_this(cls);
            let saved = collector.rev_bound_tparams.clone();
            collector.rev_bound_tparams.insert(0, this_tparam);
            if let Some(tp) = &cls.tparams {
                for param in tp.params.iter() {
                    let tparam = make_typeparam(param);
                    collector.rev_bound_tparams.insert(0, tparam);
                }
            }
            class_collector_visit_class_body(collector, cls);
            collector.rev_bound_tparams = saved;
        }
    }

    // In OCaml, the superclass `class_` does the full polymorphic_ast_mapper traversal, which
    // recursively visits ALL nested content (statements in methods, expressions, etc.)
    // and dispatches to the overridden class_ method for nested classes. We replicate
    // this by walking into the class body, method bodies, expressions, etc., looking
    // for nested class declarations.
    fn class_collector_visit_class_body(
        collector: &mut ClassCollector<'_>,
        cls: &ast::class::Class<ALoc, (ALoc, Type)>,
    ) {
        use ast::class::BodyElement;
        for element in cls.body.body.iter() {
            match element {
                BodyElement::Method(method) => {
                    // Recurse into method body
                    let (_, ref f) = method.value;
                    if let ast::function::Body::BodyBlock((_, block)) = &f.body {
                        class_collector_visit_statements(collector, &block.body);
                    }
                }
                BodyElement::Property(prop) => {
                    if let ast::class::property::Value::Initialized(init_expr) = &prop.value {
                        class_collector_visit_expression(collector, init_expr);
                    }
                }
                BodyElement::StaticBlock(block) => {
                    class_collector_visit_statements(collector, &block.body);
                }
                _ => {}
            }
        }
    }

    fn class_collector_visit_program(
        collector: &mut ClassCollector<'_>,
        typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    ) {
        // Walk all statements looking for class declarations
        class_collector_visit_statements(collector, &typed_ast.statements);
    }

    fn class_collector_visit_statements(
        collector: &mut ClassCollector<'_>,
        stmts: &[ast::statement::Statement<ALoc, (ALoc, Type)>],
    ) {
        for stmt in stmts.iter() {
            class_collector_visit_statement(collector, stmt);
        }
    }

    fn class_collector_visit_statement(
        collector: &mut ClassCollector<'_>,
        stmt: &ast::statement::Statement<ALoc, (ALoc, Type)>,
    ) {
        use ast::statement::StatementInner;
        match &**stmt {
            StatementInner::ClassDeclaration { inner, .. } => {
                class_collector_visit_class(collector, inner);
            }
            // For other statements, recursively walk into nested content to find classes
            StatementInner::FunctionDeclaration { inner, .. } => {
                if let ast::function::Body::BodyBlock((_, block)) = &inner.body {
                    class_collector_visit_statements(collector, &block.body);
                }
            }
            StatementInner::Block { inner, .. } => {
                class_collector_visit_statements(collector, &inner.body);
            }
            StatementInner::If { inner, .. } => {
                class_collector_visit_statement(collector, &inner.consequent);
                if let Some(alternate) = &inner.alternate {
                    class_collector_visit_statement(collector, &alternate.body);
                }
            }
            StatementInner::While { inner, .. } => {
                class_collector_visit_statement(collector, &inner.body);
            }
            StatementInner::DoWhile { inner, .. } => {
                class_collector_visit_statement(collector, &inner.body);
            }
            StatementInner::For { inner, .. } => {
                class_collector_visit_statement(collector, &inner.body);
            }
            StatementInner::ForIn { inner, .. } => {
                class_collector_visit_statement(collector, &inner.body);
            }
            StatementInner::ForOf { inner, .. } => {
                class_collector_visit_statement(collector, &inner.body);
            }
            StatementInner::Switch { inner, .. } => {
                for case in inner.cases.iter() {
                    class_collector_visit_statements(collector, &case.consequent);
                }
            }
            StatementInner::Try { inner, .. } => {
                class_collector_visit_statements(collector, &inner.block.1.body);
                if let Some(handler) = &inner.handler {
                    class_collector_visit_statements(collector, &handler.body.1.body);
                }
                if let Some(finalizer) = &inner.finalizer {
                    class_collector_visit_statements(collector, &finalizer.1.body);
                }
            }
            StatementInner::Labeled { inner, .. } => {
                class_collector_visit_statement(collector, &inner.body);
            }
            StatementInner::VariableDeclaration { inner, .. } => {
                for decl in inner.declarations.iter() {
                    if let Some(init_expr) = &decl.init {
                        class_collector_visit_expression(collector, init_expr);
                    }
                }
            }
            StatementInner::Expression { inner, .. } => {
                class_collector_visit_expression(collector, &inner.expression);
            }
            StatementInner::ExportDefaultDeclaration { inner, .. } => {
                use ast::statement::export_default_declaration::Declaration;
                match &inner.declaration {
                    Declaration::Declaration(stmt) => {
                        class_collector_visit_statement(collector, stmt);
                    }
                    Declaration::Expression(expr) => {
                        class_collector_visit_expression(collector, expr);
                    }
                }
            }
            StatementInner::ExportNamedDeclaration { inner, .. } => {
                if let Some(decl) = &inner.declaration {
                    class_collector_visit_statement(collector, decl);
                }
            }
            StatementInner::With { inner, .. } => {
                class_collector_visit_statement(collector, &inner.body);
            }
            StatementInner::Return { inner, .. } => {
                if let Some(arg) = &inner.argument {
                    class_collector_visit_expression(collector, arg);
                }
            }
            _ => {}
        }
    }

    // Recurse into expressions looking for nested class expressions
    fn class_collector_visit_expression(
        collector: &mut ClassCollector<'_>,
        expr: &ast::expression::Expression<ALoc, (ALoc, Type)>,
    ) {
        use ast::expression::ExpressionInner;
        match &**expr {
            ExpressionInner::Class { inner: cls, .. } => {
                class_collector_visit_class(collector, cls);
            }
            ExpressionInner::ArrowFunction { inner: f, .. }
            | ExpressionInner::Function { inner: f, .. } => {
                if let ast::function::Body::BodyBlock((_, block)) = &f.body {
                    class_collector_visit_statements(collector, &block.body);
                }
            }
            ExpressionInner::Assignment { inner, .. } => {
                class_collector_visit_expression(collector, &inner.right);
            }
            ExpressionInner::Conditional { inner, .. } => {
                class_collector_visit_expression(collector, &inner.consequent);
                class_collector_visit_expression(collector, &inner.alternate);
            }
            ExpressionInner::Logical { inner, .. } => {
                class_collector_visit_expression(collector, &inner.left);
                class_collector_visit_expression(collector, &inner.right);
            }
            ExpressionInner::Binary { inner, .. } => {
                class_collector_visit_expression(collector, &inner.left);
                class_collector_visit_expression(collector, &inner.right);
            }
            ExpressionInner::Sequence { inner, .. } => {
                for e in inner.expressions.iter() {
                    class_collector_visit_expression(collector, e);
                }
            }
            ExpressionInner::Call { inner, .. } => {
                class_collector_visit_expression(collector, &inner.callee);
                for arg in inner.arguments.arguments.iter() {
                    match arg {
                        ast::expression::ExpressionOrSpread::Expression(e) => {
                            class_collector_visit_expression(collector, e);
                        }
                        ast::expression::ExpressionOrSpread::Spread(s) => {
                            class_collector_visit_expression(collector, &s.argument);
                        }
                    }
                }
            }
            ExpressionInner::Object { inner, .. } => {
                for prop in inner.properties.iter() {
                    match prop {
                        ast::expression::object::Property::NormalProperty(np) => match np {
                            ast::expression::object::NormalProperty::Init { value, .. } => {
                                class_collector_visit_expression(collector, value);
                            }
                            ast::expression::object::NormalProperty::Method {
                                value: (_, f),
                                ..
                            }
                            | ast::expression::object::NormalProperty::Get {
                                value: (_, f), ..
                            }
                            | ast::expression::object::NormalProperty::Set {
                                value: (_, f), ..
                            } => {
                                if let ast::function::Body::BodyBlock((_, block)) = &f.body {
                                    class_collector_visit_statements(collector, &block.body);
                                }
                            }
                        },
                        ast::expression::object::Property::SpreadProperty(s) => {
                            class_collector_visit_expression(collector, &s.argument);
                        }
                    }
                }
            }
            ExpressionInner::Array { inner, .. } => {
                for elem in inner.elements.iter() {
                    match elem {
                        ast::expression::ArrayElement::Expression(e) => {
                            class_collector_visit_expression(collector, e);
                        }
                        ast::expression::ArrayElement::Spread(s) => {
                            class_collector_visit_expression(collector, &s.argument);
                        }
                        ast::expression::ArrayElement::Hole(_) => {}
                    }
                }
            }
            _ => {}
        }
    }

    pub fn find_closest_enclosing_class(
        typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
        loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
        extracted_loc: Loc,
    ) -> Option<ClassInsertionPoint> {
        let mut collector = ClassCollector {
            loc_of_aloc,
            extracted_loc,
            rev_bound_tparams: Vec::new(),
            acc: None,
        };
        class_collector_visit_program(&mut collector, typed_ast);
        collector.acc
    }
}

pub mod ast_extractor {
    use dupe::Dupe;
    use flow_parser::TokenSinkResult;
    use flow_parser::ast;
    use flow_parser::ast::expression::ExpressionInner;
    use flow_parser::ast::statement::StatementInner;
    use flow_parser::ast_visitor;
    use flow_parser::ast_visitor::AstVisitor;
    use flow_parser::loc::LOC_NONE;
    use flow_parser::loc::Loc;

    #[derive(Debug, PartialEq)]
    pub struct ConstantInsertionPoint {
        pub title: String,
        pub function_body_loc: Option<Loc>,
        pub statement_loc: Loc,
    }

    pub struct ExpressionWithConstantInsertionPoints {
        pub constant_insertion_points: Vec<ConstantInsertionPoint>, /* non-empty */
        pub expression: ast::expression::Expression<Loc, Loc>,
    }

    pub struct TypeWithStatementLoc {
        pub directly_containing_statement_loc: Loc,
        pub type_: ast::types::Type<Loc, Loc>,
    }

    pub struct Extracted {
        pub extracted_statements: Option<Vec<ast::statement::Statement<Loc, Loc>>>,
        pub extracted_expression: Option<ExpressionWithConstantInsertionPoints>,
        pub extracted_type: Option<TypeWithStatementLoc>,
    }

    pub fn tokens(
        parse_options: Option<flow_parser::ParseOptions>,
        filename: Option<flow_parser::file_key::FileKey>,
        file_contents: &str,
    ) -> Vec<TokenSinkResult> {
        let mut rev_tokens = Vec::new();
        let mut token_sink = |token_data: TokenSinkResult| {
            rev_tokens.push(token_data);
        };
        match filename {
            Some(file_name) => {
                flow_parser::parse_program_file::<()>(
                    false,
                    Some(&mut token_sink),
                    parse_options,
                    file_name,
                    Ok(file_contents),
                );
            }
            None => {
                flow_parser::parse_program_without_file(
                    false,
                    Some(&mut token_sink),
                    parse_options,
                    Ok(file_contents),
                );
            }
        }
        // OCaml returns the list in reverse order (due to prepend). Preserve same order.
        rev_tokens.reverse();
        rev_tokens
    }

    struct Collector<'a> {
        tokens: &'a [TokenSinkResult],
        extract_range: Loc,
        constant_insertion_points: Vec<ConstantInsertionPoint>,
        collected_expression: Option<ExpressionWithConstantInsertionPoints>,
        collected_type: Option<TypeWithStatementLoc>,
        collected_statements: Option<Vec<ast::statement::Statement<Loc, Loc>>>,
    }

    impl<'a> Collector<'a> {
        fn new(tokens: &'a [TokenSinkResult], extract_range: Loc) -> Self {
            Collector {
                tokens,
                extract_range,
                constant_insertion_points: vec![ConstantInsertionPoint {
                    title: "Extract to constant in module scope".to_string(),
                    function_body_loc: None,
                    statement_loc: LOC_NONE,
                }],
                collected_expression: None,
                collected_type: None,
                collected_statements: Some(Vec::new()),
            }
        }

        fn visit_named_function(&mut self, is_method: bool, function_name: &str, body_loc: &Loc) {
            if body_loc.contains(&self.extract_range) {
                self.constant_insertion_points.insert(
                    0,
                    ConstantInsertionPoint {
                        title: format!(
                            "Extract to constant in {} '{}'",
                            if is_method { "method" } else { "function" },
                            function_name,
                        ),
                        function_body_loc: Some(body_loc.dupe()),
                        statement_loc: LOC_NONE,
                    },
                );
            }
        }

        fn function_with_name(
            &mut self,
            name: Option<&ast::Identifier<Loc, Loc>>,
            is_method: bool,
            function_declaration: &ast::function::Function<Loc, Loc>,
        ) {
            if let ast::function::Body::BodyBlock((block_loc, _)) = &function_declaration.body {
                let id = &function_declaration.id;
                let fn_name = match (id, name) {
                    (None, None) => return,
                    (Some(id), _) => &id.name,
                    (None, Some(name)) => &name.name,
                };
                self.visit_named_function(is_method, fn_name, block_loc);
            }
        }

        fn collect_statement(&mut self, stmt: &ast::statement::Statement<Loc, Loc>) {
            if let Some(ref mut acc) = self.collected_statements {
                acc.push(stmt.dupe());
            }
        }

        fn extracted(self) -> Extracted {
            let extracted_statements = match self.collected_statements {
                None => None,
                Some(ref stmts) if stmts.is_empty() => None,
                Some(stmts) => Some(stmts),
            };
            Extracted {
                extracted_statements,
                extracted_expression: self.collected_expression,
                extracted_type: self.collected_type,
            }
        }

        fn valid_single_selection(&self, node_loc: &Loc) -> bool {
            self.extract_range == *node_loc
                || (self.extract_range.contains(node_loc)
                    && self.tokens.iter().all(|token| {
                        let token_loc = &token.token_loc;
                        node_loc.contains(token_loc)
                            || self.extract_range.end <= token_loc.start
                            || token_loc.end <= self.extract_range.start
                    }))
        }

        // Rust-only helper: OCaml's immutable list allows implicit sharing when saving/restoring state.
        // Rust's Vec requires explicit cloning for save/restore.
        fn clone_constant_insertion_points(&self) -> Vec<ConstantInsertionPoint> {
            self.constant_insertion_points
                .iter()
                .map(|cip| ConstantInsertionPoint {
                    title: cip.title.clone(),
                    function_body_loc: cip.function_body_loc.dupe(),
                    statement_loc: cip.statement_loc.dupe(),
                })
                .collect()
        }
    }

    impl<'a, 'ast> AstVisitor<'ast, Loc> for Collector<'a> {
        fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
            loc
        }

        fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
            type_
        }

        fn statement(&mut self, stmt: &'ast ast::statement::Statement<Loc, Loc>) -> Result<(), !> {
            let statement_loc = stmt.loc();
            let saved_constant_insertion_points = self.clone_constant_insertion_points();
            if let Some(first) = self.constant_insertion_points.first_mut() {
                first.statement_loc = statement_loc.dupe();
            }
            if self.extract_range.contains(statement_loc) {
                self.collect_statement(stmt);
                if let StatementInner::Expression { inner, .. } = &**stmt {
                    let expr_loc = inner.expression.loc();
                    if expr_loc == statement_loc {
                        self.collected_expression = Some(ExpressionWithConstantInsertionPoints {
                            constant_insertion_points: self.clone_constant_insertion_points(),
                            expression: inner.expression.dupe(),
                        });
                    }
                }
            } else if statement_loc.contains(&self.extract_range) {
                ast_visitor::statement_default(self, stmt)?;
            } else if self.extract_range.intersects(statement_loc) {
                self.collected_statements = None;
            }
            self.constant_insertion_points = saved_constant_insertion_points;
            Ok(())
        }

        fn expression(
            &mut self,
            expr: &'ast ast::expression::Expression<Loc, Loc>,
        ) -> Result<(), !> {
            let expression_loc = expr.loc();
            if self.valid_single_selection(expression_loc) {
                self.collected_expression = Some(ExpressionWithConstantInsertionPoints {
                    constant_insertion_points: self.clone_constant_insertion_points(),
                    expression: expr.dupe(),
                });
            } else if expression_loc.contains(&self.extract_range) {
                ast_visitor::expression_default(self, expr)?;
            }
            Ok(())
        }

        fn jsx_child(&mut self, child: &'ast ast::jsx::Child<Loc, Loc>) -> Result<(), !> {
            match child {
                ast::jsx::Child::Element { loc, inner } => {
                    self.jsx_element(loc, inner)?;
                }
                ast::jsx::Child::Fragment { loc, inner } => {
                    self.jsx_fragment(loc, inner)?;
                }
                ast::jsx::Child::ExpressionContainer { inner, .. } => {
                    self.jsx_expression(inner)?;
                }
                ast::jsx::Child::SpreadChild { inner, .. } => {
                    self.jsx_spread_child(inner)?;
                }
                ast::jsx::Child::Text { .. } => {}
            }
            Ok(())
        }

        fn jsx_element(
            &mut self,
            loc: &'ast Loc,
            elem: &'ast ast::jsx::Element<Loc, Loc>,
        ) -> Result<(), !> {
            if self.valid_single_selection(loc) {
                self.collected_expression = Some(ExpressionWithConstantInsertionPoints {
                    constant_insertion_points: self.clone_constant_insertion_points(),
                    expression: ast::expression::Expression::new(ExpressionInner::JSXElement {
                        loc: loc.dupe(),
                        inner: std::sync::Arc::new(elem.clone()),
                    }),
                });
            } else if loc.contains(&self.extract_range) {
                ast_visitor::jsx_element_default(self, loc, elem)?;
            }
            Ok(())
        }

        fn jsx_fragment(
            &mut self,
            loc: &'ast Loc,
            frag: &'ast ast::jsx::Fragment<Loc, Loc>,
        ) -> Result<(), !> {
            if self.valid_single_selection(loc) {
                self.collected_expression = Some(ExpressionWithConstantInsertionPoints {
                    constant_insertion_points: self.clone_constant_insertion_points(),
                    expression: ast::expression::Expression::new(ExpressionInner::JSXFragment {
                        loc: loc.dupe(),
                        inner: std::sync::Arc::new(frag.clone()),
                    }),
                });
            } else if loc.contains(&self.extract_range) {
                ast_visitor::jsx_fragment_default(self, loc, frag)?;
            }
            Ok(())
        }

        fn type_(&mut self, t: &'ast ast::types::Type<Loc, Loc>) -> Result<(), !> {
            let type_loc = t.loc();
            if self.valid_single_selection(type_loc) {
                let statement_loc = self
                    .constant_insertion_points
                    .first()
                    .map(|cip| cip.statement_loc.dupe())
                    .unwrap_or(LOC_NONE);
                self.collected_type = Some(TypeWithStatementLoc {
                    directly_containing_statement_loc: statement_loc,
                    type_: t.dupe(),
                });
            } else if type_loc.contains(&self.extract_range) {
                ast_visitor::type_default(self, t)?;
            }
            Ok(())
        }

        // Inherited from function_and_method_insertion_point_visitor
        fn function_declaration(
            &mut self,
            _loc: &'ast Loc,
            func: &'ast ast::function::Function<Loc, Loc>,
        ) -> Result<(), !> {
            self.function_with_name(None, false, func);
            ast_visitor::function_declaration_default(self, _loc, func)
        }

        fn variable_declarator(
            &mut self,
            kind: ast::VariableKind,
            declarator: &'ast ast::statement::variable::Declarator<Loc, Loc>,
        ) -> Result<(), !> {
            let id = &declarator.id;
            let init = &declarator.init;
            if let ast::pattern::Pattern::Identifier { inner, .. } = id {
                if let Some(init_expr) = init {
                    match &**init_expr {
                        ExpressionInner::ArrowFunction { inner: f, .. }
                        | ExpressionInner::Function { inner: f, .. } => {
                            self.function_with_name(Some(&inner.name), false, f);
                        }
                        _ => {}
                    }
                }
            }
            ast_visitor::variable_declarator_default(self, kind, declarator)
        }

        fn class_property(&mut self, prop: &'ast ast::class::Property<Loc, Loc>) -> Result<(), !> {
            let key = &prop.key;
            let value = &prop.value;
            if let ast::expression::object::Key::Identifier(name) = key {
                if let ast::class::property::Value::Initialized(init_expr) = value {
                    match &**init_expr {
                        ExpressionInner::ArrowFunction { inner: f, .. }
                        | ExpressionInner::Function { inner: f, .. } => {
                            self.function_with_name(Some(name), true, f);
                        }
                        _ => {}
                    }
                }
            }
            ast_visitor::class_property_default(self, prop)
        }

        fn class_method(&mut self, method: &'ast ast::class::Method<Loc, Loc>) -> Result<(), !> {
            let key = &method.key;
            let (_, ref f) = method.value;
            if let ast::expression::object::Key::Identifier(name) = key {
                self.function_with_name(Some(name), true, f);
            }
            ast_visitor::class_method_default(self, method)
        }
    }

    pub fn extract(
        tokens: &[TokenSinkResult],
        ast: &ast::Program<Loc, Loc>,
        extract_range: Loc,
    ) -> Extracted {
        let mut collector = Collector::new(tokens, extract_range);
        let Ok(()) = collector.program(ast);
        collector.extracted()
    }
}
