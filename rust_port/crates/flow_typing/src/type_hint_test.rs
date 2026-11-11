/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#[cfg(test)]
mod tests {
    use std::cell::LazyCell;
    use std::cell::RefCell;
    use std::ops::Deref;
    use std::rc::Rc;
    use std::sync::Arc;

    use dupe::Dupe;
    use flow_aloc::ALoc;
    use flow_aloc::ALocTable;
    use flow_aloc::LazyALocTable;
    use flow_common::flow_import_specifier::FlowImportSpecifier;
    use flow_common::hint::Hint;
    use flow_common::hint::HintDecompositionInner::*;
    use flow_common::hint::HintKind::ExpectedTypeHint;
    use flow_common::options::Options;
    use flow_common::polarity::Polarity;
    use flow_common::reason::Name;
    use flow_common::reason::Reason;
    use flow_common::reason::locationless_reason;
    use flow_common::verbose::Verbose;
    use flow_data_structure_wrapper::ord_map::FlowOrdMap;
    use flow_data_structure_wrapper::ord_set::FlowOrdSet;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_data_structure_wrapper::vector::FlowVector;
    use flow_env_builder::env_api::EnvMap;
    use flow_env_builder_resolver::name_resolver;
    use flow_lint_settings::lint_settings::LintSettings;
    use flow_lint_settings::severity::Severity;
    use flow_lint_settings::strict_mode_settings::StrictModeSettings;
    use flow_parser::PERMISSIVE_PARSE_OPTIONS;
    use flow_parser::ast;
    use flow_parser::file_key::FileKey;
    use flow_parser::file_key::FileKeyInner;
    use flow_parser::loc::Loc;
    use flow_parser::loc_sig::LocSig;
    use flow_parser_utils::file_sig::FileSig;
    use flow_type_sig::type_sig_options::TypeSigOptions;
    use flow_typing_context::Context;
    use flow_typing_context::ContextArena;
    use flow_typing_context::ContextStore;
    use flow_typing_context::MasterContext;
    use flow_typing_context::Metadata;
    use flow_typing_context::ResolveRequire;
    use flow_typing_context::ResolvedRequire;
    use flow_typing_loc_env::loc_env::LocEnv;
    use flow_typing_statement::type_annotation;
    use flow_typing_type::type_::ClassBinding;
    use flow_typing_type::type_::DefT;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::FieldData;
    use flow_typing_type::type_::FunType;
    use flow_typing_type::type_::HintEvalResult;
    use flow_typing_type::type_::InstType;
    use flow_typing_type::type_::InstTypeInner;
    use flow_typing_type::type_::InstanceKind;
    use flow_typing_type::type_::InstanceT;
    use flow_typing_type::type_::InstanceTInner;
    use flow_typing_type::type_::Property;
    use flow_typing_type::type_::PropertyInner;
    use flow_typing_type::type_::ReactEffectType;
    use flow_typing_type::type_::ThisStatus;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::dummy_static;
    use flow_typing_type::type_::unsoundness;
    use flow_typing_type::type_::void;
    use flow_typing_utils::type_hint;
    use flow_typing_utils::type_hint::ConcrHint;
    use flow_typing_utils::type_hint::ConcrHintDecompositionInner;
    use vec1::Vec1;

    use crate::merge;
    use crate::ty_normalizer_flow;
    use crate::type_inference;

    // Local DepSigsContext and FlowJsUtilsFlow (mirrors type_inference.rs private types)
    struct DepSigsContext<'a, 'cx>(&'a mut Context<'cx>);

    impl flow_env_builder_resolver::dependency_sigs::Context for DepSigsContext<'_, '_> {
        fn enable_enums(&self) -> bool {
            self.0.enable_enums()
        }
        fn file(&self) -> FileKey {
            self.0.file().dupe()
        }
        fn jsx(&self) -> flow_common::options::JsxMode {
            self.0.jsx().clone()
        }
        fn react_runtime(&self) -> flow_common::options::ReactRuntime {
            self.0.react_runtime()
        }
        fn enable_const_params(&self) -> bool {
            self.0.enable_const_params()
        }

        fn strict_property_initialization(&self) -> bool {
            self.0.strict_property_initialization()
        }

        fn stylex_shorthand_prop(&self) -> Option<&str> {
            self.0.stylex_shorthand_prop()
        }
        fn add_exhaustive_check(&mut self, loc: ALoc, cases: (Vec<ALoc>, bool)) {
            self.0.add_exhaustive_check(loc, cases);
        }
        fn exhaustive_check(&self, loc: &ALoc) -> Option<(Vec<ALoc>, bool)> {
            self.0.exhaustive_check(loc)
        }
    }

    struct FlowJsUtilsFlow<'a, 'cx>(std::marker::PhantomData<(&'a (), &'cx ())>);

    impl<'a, 'cx: 'a> flow_env_builder_resolver::dependency_sigs::Flow for FlowJsUtilsFlow<'a, 'cx> {
        type Cx = DepSigsContext<'a, 'cx>;
        fn add_output(
            cx: &mut Self::Cx,
            error: flow_typing_errors::error_message::ErrorMessage<ALoc>,
        ) {
            flow_typing_flow_common::flow_js_utils::add_output_non_speculating(cx.0, error);
        }
    }

    fn metadata() -> Metadata {
        let mut m = Metadata::default();
        m.overridable.checked = true;
        // Provide some useful error traces for debugging when some tests fail.
        let mut frozen = Rc::try_unwrap(m.frozen).unwrap();
        frozen.verbose = Some(Arc::new(Verbose {
            indent: 2,
            depth: 10,
            enabled_during_flowlib: false,
            focused_files: None,
        }));
        frozen.hook_compatibility = true;
        frozen.component_syntax = true;
        frozen.enable_enums = true;
        frozen.strip_root = true;
        frozen.recursion_limit = 10000;
        frozen.type_expansion_recursion_limit = 3;
        frozen.ts_syntax = true;
        m.frozen = Rc::new(frozen);
        m
    }

    fn dummy_filename() -> FileKey {
        FileKey::new(FileKeyInner::SourceFile("".to_string()))
    }

    fn type_sig_options() -> TypeSigOptions {
        TypeSigOptions::of_options(
            &Options {
                component_syntax: true,
                enums: true,
                hook_compatibility: true,
                ts_syntax: true,
                ..Default::default()
            },
            false,
            Vec::new(),
            &dummy_filename(),
            false,
        )
    }

    #[allow(dead_code)]
    fn dummy_loc() -> ALoc {
        ALoc::of_loc(Loc {
            source: Some(dummy_filename()),
            start: flow_parser::loc::Position { line: 0, column: 0 },
            end: flow_parser::loc::Position { line: 0, column: 0 },
        })
    }

    fn dummy_reason() -> Reason {
        locationless_reason(None)
    }

    mod type_parser {
        use super::*;

        fn parse_type(content: &str) -> ast::types::Annotation<ALoc, ALoc> {
            let parse_options = Some(PERMISSIVE_PARSE_OPTIONS);
            // the parser expects a colon
            let content = format!(": {}", content);
            let (t, errs) = flow_parser::parse_annotation::<()>(
                parse_options,
                Some(dummy_filename()),
                &content,
            );
            assert!(errs.is_empty());
            flow_aloc::loc_to_aloc_type_annotation_owned(t)
        }

        pub fn parse<'cx>(cx: &mut Context<'cx>, content: &str) -> Type {
            let ast::types::Annotation {
                loc: program_loc,
                annotation: t_ast,
            } = parse_type(content);
            // The object type converter will peek the scope, so we need to have a non-empty scope list.
            let fake_program = ast::Program {
                loc: program_loc.dupe(),
                statements: Arc::from([ast::statement::Statement::new(
                    ast::statement::StatementInner::TypeAlias {
                        loc: ALoc::none(),
                        inner: Arc::new(ast::statement::TypeAlias {
                            id: ast::Identifier::new(ast::IdentifierInner {
                                loc: ALoc::none(),
                                name: FlowSmolStr::from("_"),
                                comments: None,
                            }),
                            tparams: None,
                            right: t_ast.clone(),
                            comments: None,
                        }),
                    },
                )]),
                interpreter: None,
                comments: None,
                all_comments: Arc::from([]),
            };
            let (_, info) = name_resolver::program_with_scope::<DepSigsContext, FlowJsUtilsFlow>(
                &mut DepSigsContext(cx),
                false,
                FlowOrdSet::new(),
                &FileSig::empty(),
                &fake_program,
            );
            let env = LocEnv::with_info(
                flow_env_builder::name_def_types::ScopeKind::Global,
                flow_aloc::ALocMap::new(),
                EnvMap::empty(),
                Rc::new(info.to_env_info()),
                FlowOrdMap::new(),
                EnvMap::empty(),
            );
            *cx.environment_mut() = env;
            flow_typing_utils::type_env::init_env(
                cx,
                flow_env_builder::name_def_types::ScopeKind::Global,
            );
            let result = type_annotation::convert(cx, FlowOrdMap::new(), &t_ast)
                .expect("convert should not be canceled in test");
            let (_, t) = result.loc();
            t.dupe()
        }
    }

    mod type_loader {
        use super::*;

        fn parse_content(file: &FileKey, content: &str) -> ast::Program<Loc, Loc> {
            let parse_options = Some(PERMISSIVE_PARSE_OPTIONS);
            let (ast, _) = flow_parser::parse_program_file::<()>(
                false,
                None,
                parse_options,
                file.dupe(),
                Ok(content),
            );
            ast
        }

        // No verbose mode during libdef init.
        fn loader_metadata() -> Metadata {
            let mut m = metadata();
            let mut frozen = Rc::try_unwrap(m.frozen).unwrap();
            frozen.verbose = None;
            m.frozen = Rc::new(frozen);
            m
        }

        fn init_master_cx() -> Arc<MasterContext> {
            let asts: Vec<(
                flow_common::type_strictness::TypeStrictnessKind,
                Arc<ast::Program<Loc, Loc>>,
            )> = flow_flowlib::contents_list(flow_flowlib::BuiltinLib::Flowlib)
                .into_iter()
                .rev()
                .map(|(filename, lib_content)| {
                    let lib_file = FileKey::new(FileKeyInner::LibFile(filename.to_string()));
                    let ast = parse_content(&lib_file, lib_content);
                    (
                        flow_common::type_strictness::TypeStrictnessKind::from_is_typescript(
                            flow_common::files::has_ts_ext(&lib_file),
                        ),
                        Arc::new(ast),
                    )
                })
                .collect();
            let sig_opts = TypeSigOptions {
                munge: false,
                facebook_key_mirror: false,
                enable_relay_integration: false,
                relay_integration_module_prefix: None,
                facebook_fbt: None,
                enable_custom_error: false,
                enable_enums: true,
                enable_component_syntax: true,
                component_syntax_enabled_in_config: true,
                enable_ts_syntax: true,
                hook_compatibility: true,
                enable_records: true,
                for_builtins: true,
                locs_to_dirtify: vec![],
                is_ts_file: false,
                is_dts_file: false,
                tslib_syntax: true,
                is_lib_file: true,
            };
            let (_, master_cx) = merge::merge_lib_files(&sig_opts, Arc::default(), &asts, &[]);
            Arc::new(master_cx)
        }

        thread_local! {
            static MASTER_CX: RefCell<Option<Arc<MasterContext>>> = const { RefCell::new(None) };
        }

        pub fn get_master_cx() -> Arc<MasterContext> {
            MASTER_CX.with(|cell| {
                let mut borrow = cell.borrow_mut();
                match borrow.deref() {
                    Some(master_cx) => master_cx.clone(),
                    None => {
                        let master_cx = init_master_cx();
                        *borrow = Some(master_cx.clone());
                        master_cx
                    }
                }
            })
        }

        fn get_typed_ast(cx: &mut Context, content: &str) -> ast::Program<ALoc, (ALoc, Type)> {
            let metadata = loader_metadata();
            let ast = parse_content(&dummy_filename(), content);
            let aloc_ast = flow_aloc::loc_to_aloc_ast(&ast);
            let lint_severities = merge::get_lint_severities(
                &metadata,
                &StrictModeSettings::empty(),
                LintSettings::<Severity>::empty_severities(),
            );
            type_inference::infer_ast(
                &lint_severities,
                cx,
                &dummy_filename(),
                Arc::new(FileSig::empty()),
                &metadata,
                &ast.all_comments,
                &ast,
                aloc_ast,
                &type_sig_options(),
                None,
            )
            .expect("infer_ast should not be canceled in test")
        }

        pub fn get_type_of_last_expression(cx: &mut Context, content: &str) -> Type {
            let typed_ast = get_typed_ast(cx, content);
            match typed_ast.statements.last() {
                Some(stmt) => match stmt.deref() {
                    ast::statement::StatementInner::Expression { inner, .. } => {
                        let (_, ref t) = *inner.expression.loc();
                        t.dupe()
                    }
                    _ => panic!("Must have a last statement that's an expression"),
                },
                _ => panic!("Must have a last statement that's an expression"),
            }
        }
    }

    fn fun_t(params: &[flow_typing_type::type_::FunParam], return_t: Type) -> Type {
        Type::new(TypeInner::DefT(
            dummy_reason(),
            DefT::new(DefTInner::FunT(
                dummy_static(dummy_reason()),
                Rc::new(FunType {
                    this_t: (
                        unsoundness::unresolved_any(dummy_reason()),
                        ThisStatus::ThisFunction,
                    ),
                    params: Rc::from(params),
                    rest_param: None,
                    return_t,
                    type_guard: None,
                    def_reason: dummy_reason(),
                    function_reference_kind:
                        flow_common::error_ref::FunctionReferenceKind::FunctionType,
                    effect_: ReactEffectType::AnyEffect,
                    strictness_kind: flow_common::type_strictness::TypeStrictnessKind::Flow,
                    literal: false,
                }),
            )),
        ))
    }

    fn mk_cx<'cx>(arena: &'cx mut ContextArena, verbose: bool) -> Context<'cx> {
        let aloc_table: LazyALocTable = Rc::new(LazyCell::new(Box::new({
            let file = dummy_filename();
            move || Rc::new(ALocTable::empty(file))
        })
            as Box<dyn FnOnce() -> Rc<ALocTable>>));
        let md = if verbose {
            metadata()
        } else {
            let mut m = metadata();
            let mut frozen = Rc::try_unwrap(m.frozen).unwrap();
            frozen.verbose = None;
            m.frozen = Rc::new(frozen);
            m
        };
        let resolve_require: ResolveRequire = Rc::new(
            |cx: &mut Context<'_>, spec: &FlowImportSpecifier| match spec {
                FlowImportSpecifier::Userland(mref) => match cx.builtin_module_opt(mref) {
                    Some((_reason, m)) => {
                        ResolvedRequire::TypedModule(Rc::new(move |_cx: &mut Context<'_>| {
                            Ok(m.as_already_forced().dupe())
                        }))
                    }
                    None => ResolvedRequire::MissingModule,
                },
            },
        );
        let master_cx = type_loader::get_master_cx();
        let ccx = arena.add_component();
        let mk_builtins = merge::mk_builtins(&md, &master_cx, arena);
        let cx_id = arena.add_context(
            ccx,
            md.clone(),
            dummy_filename(),
            Arc::default(),
            aloc_table,
            resolve_require,
            mk_builtins,
            flow_utils_concurrency::check_budget::CheckBudget::new(None),
        );
        Context::new(ContextStore::new(vec![arena]), cx_id)
    }

    fn mk_hint(base_t: Type, ops: Vec<ConcrHintDecompositionInner>) -> ConcrHint {
        use flow_common::hint::HintDecomposition;

        let indexed_ops: Vec<(usize, HintDecomposition<_, _, _, _, _>)> = ops
            .into_iter()
            .enumerate()
            .map(|(i, op)| (i, HintDecomposition::new(op)))
            .collect();
        match Vec1::try_from_vec(indexed_ops) {
            Err(_) => Hint::HintT(Ok(base_t), ExpectedTypeHint),
            Ok(l) => Hint::HintDecomp(l, Ok(base_t), ExpectedTypeHint),
        }
    }

    fn string_of_hint_eval_result(cx: &mut Context, result: &HintEvalResult) -> String {
        match result {
            HintEvalResult::HintAvailable(t, _) => ty_normalizer_flow::debug_string_of_t(cx, t),
            HintEvalResult::NoHint => "NoHint".to_string(),
            HintEvalResult::EncounteredPlaceholder(_) => "EncounteredPlaceholder".to_string(),
            HintEvalResult::DecompositionError => "DecompositionError".to_string(),
        }
    }

    fn mk_eval_hint_test(expected: &str, base: &str, ops: Vec<ConcrHintDecompositionInner>) {
        let mut arena = ContextArena::new();
        let mut cx = mk_cx(&mut arena, false);
        let actual = {
            let hint = mk_hint(type_parser::parse(&mut cx, base), ops);
            let result = type_hint::evaluate_hint(&mut cx, false, None, &dummy_reason(), hint)
                .expect("evaluate_hint should not be canceled in test");
            string_of_hint_eval_result(&mut cx, &result)
        };
        // DEBUGGING TIP: set verbose:true above to print traces
        assert_eq!(expected, actual);
    }

    fn mk_private_method_eval_hint_test(
        expected: &str,
        static_: bool,
        private_field: Option<Property>,
        private_static_field: Option<Property>,
        private_method: Option<Property>,
        private_static_method: Option<Property>,
    ) {
        let mut arena = ContextArena::new();
        let mut cx = mk_cx(&mut arena, false);
        let mk_properties = |cx: &mut Context<'_>,
                             prop: Option<Property>|
         -> flow_typing_type::type_::properties::Id {
            match prop {
                None => cx.generate_property_map(
                    flow_typing_type::type_::properties::PropertiesMap::new(),
                ),
                Some(property) => {
                    let mut pmap = flow_typing_type::type_::properties::PropertiesMap::new();
                    pmap.insert(Name::new("bar"), property);
                    cx.generate_property_map(pmap)
                }
            }
        };
        let class_bindings = ClassBinding {
            class_binding_id: flow_aloc::ALocId::none(),
        };
        let class_stack_loc = ALoc::none();
        {
            let env = cx.environment_mut();
            let mut bindings = EnvMap::empty();
            bindings.insert(
                flow_env_builder::env_api::EnvKey::ordinary(class_stack_loc.dupe()),
                class_bindings,
            );
            env.class_bindings = bindings;
        }
        let empty_props =
            cx.generate_property_map(flow_typing_type::type_::properties::PropertiesMap::new());
        let base = Type::new(TypeInner::DefT(
            dummy_reason(),
            DefT::new(DefTInner::InstanceT(Rc::new(InstanceT::new(
                InstanceTInner {
                    inst: InstType::new(InstTypeInner {
                        class_name: None,
                        class_id: flow_aloc::ALocId::none(),
                        type_args: Rc::from([]),
                        own_props: empty_props.dupe(),
                        proto_props: empty_props.dupe(),
                        inst_call_t: None,
                        inst_construct_t: None,
                        initialized_fields: FlowOrdSet::new(),
                        initialized_static_fields: FlowOrdSet::new(),
                        inst_kind: InstanceKind::ClassKind,
                        inst_dict: None,
                        class_private_fields: mk_properties(&mut cx, private_field),
                        class_private_static_fields: mk_properties(&mut cx, private_static_field),
                        class_private_methods: mk_properties(&mut cx, private_method),
                        class_private_static_methods: mk_properties(&mut cx, private_static_method),
                        inst_react_dro: None,
                        inst_abstract: false,
                        inst_abstract_props: FlowOrdSet::new(),
                        strictness_kind: flow_common::type_strictness::TypeStrictnessKind::Flow,
                    }),
                    static_: Type::new(TypeInner::ObjProtoT(dummy_reason())),
                    super_: Type::new(TypeInner::ObjProtoT(dummy_reason())),
                    implements: Rc::from([]),
                },
            )))),
        ));
        let base = if static_ {
            Type::new(TypeInner::DefT(
                dummy_reason(),
                DefT::new(DefTInner::ClassT(base)),
            ))
        } else {
            base
        };
        let actual = {
            let hint = mk_hint(
                base,
                vec![DecompMethodPrivateName(
                    FlowSmolStr::from("bar"),
                    FlowVector::unit(class_stack_loc),
                )],
            );
            let result = type_hint::evaluate_hint(&mut cx, false, None, &dummy_reason(), hint)
                .expect("evaluate_hint should not be canceled in test");
            string_of_hint_eval_result(&mut cx, &result)
        };
        assert_eq!(expected, actual);
    }

    fn mk_eval_hint_test_with_type_setup(
        expected: &str,
        type_setup_code: &str,
        ops: Vec<ConcrHintDecompositionInner>,
    ) {
        let mut arena = ContextArena::new();
        let mut cx = mk_cx(&mut arena, false);
        let base_t = type_loader::get_type_of_last_expression(&mut cx, type_setup_code);
        let actual = {
            let hint = mk_hint(base_t, ops);
            let result = type_hint::evaluate_hint(&mut cx, false, None, &dummy_reason(), hint)
                .expect("evaluate_hint should not be canceled in test");
            string_of_hint_eval_result(&mut cx, &result)
        };
        // DEBUGGING TIP: set verbose:true above to print traces
        assert_eq!(expected, actual);
    }

    #[test]
    fn hint_t_num() {
        mk_eval_hint_test("number", "number", vec![]);
    }

    #[test]
    fn hint_t_array() {
        mk_eval_hint_test("Array<number>", "Array<number>", vec![]);
    }

    #[test]
    fn array_element_decomp_general() {
        mk_eval_hint_test("number", "Array<number>", vec![DecompArrElement(None)]);
    }

    #[test]
    fn array_element_decomp_specific() {
        mk_eval_hint_test(
            "string",
            "[number, string]",
            vec![DecompArrElement(Some(1))],
        );
    }

    #[test]
    fn array_element_decomp_specific_none() {
        mk_eval_hint_test(
            "number | string",
            "[number, string]",
            vec![DecompArrElement(None)],
        );
    }

    #[test]
    fn array_element_decomp_specific_nonexistent() {
        mk_eval_hint_test(
            "DecompositionError",
            "[number, string]",
            vec![DecompArrElement(Some(2))],
        );
    }

    #[test]
    fn array_spread_decomp_with_general() {
        mk_eval_hint_test("Array<number>", "Array<number>", vec![DecompArrSpread(0)]);
    }

    #[test]
    fn array_spread_decomp_with_tuple_full() {
        mk_eval_hint_test(
            "[number, string]",
            "[number, string]",
            vec![DecompArrSpread(0)],
        );
    }

    #[test]
    fn array_spread_decomp_with_tuple_single() {
        mk_eval_hint_test("[string]", "[number, string]", vec![DecompArrSpread(1)]);
    }

    #[test]
    fn array_spread_decomp_with_tuple_part() {
        mk_eval_hint_test(
            "[string, number]",
            "[number, string, number]",
            vec![DecompArrSpread(1)],
        );
    }

    #[test]
    fn await_decomp() {
        mk_eval_hint_test("string | Promise<string>", "string", vec![DecompAwait]);
    }

    #[test]
    fn fun_decomp_simple_return() {
        mk_eval_hint_test(
            "number",
            "(string, number) => number",
            vec![DecompFuncReturn],
        );
    }

    #[test]
    fn fun_decomp_multi_spec_return() {
        mk_eval_hint_test(
            "number",
            "?(string, number) => number",
            vec![DecompFuncReturn],
        );
    }

    #[test]
    fn fun_decomp_simple_on_first_argument_of_hint() {
        mk_eval_hint_test(
            "string",
            "(string, number) => number",
            vec![DecompFuncParam(vec![None], 0, None)],
        );
    }

    #[test]
    fn fun_decomp_union() {
        mk_eval_hint_test(
            "string",
            "(string) => number | (string) => number",
            vec![DecompFuncParam(vec![None], 0, None)],
        );
    }

    #[test]
    fn fun_decomp_simple_on_second_argument_of_hint() {
        mk_eval_hint_test(
            "number",
            "(string, number) => number",
            vec![DecompFuncParam(vec![None, None], 1, None)],
        );
    }

    #[test]
    fn fun_decomp_on_nonexistent_argument_of_hint() {
        mk_eval_hint_test(
            "void",
            "() => number",
            vec![DecompFuncParam(vec![None], 0, None)],
        );
    }

    #[test]
    fn fun_decomp_on_rest_arguments_of_hint() {
        mk_eval_hint_test(
            "number",
            "(...Array<number>) => number",
            vec![DecompFuncParam(vec![None], 0, None)],
        );
    }

    #[test]
    fn fun_decomp_rest_arguments_matching_number_of_normal_parameters() {
        mk_eval_hint_test(
            "Array<number>",
            "(string, number, ...Array<number>) => number",
            vec![DecompFuncRest(vec![None, None], None)],
        );
    }

    #[test]
    fn fun_decomp_rest_arguments_with_additional_normal_parameters() {
        mk_eval_hint_test(
            "Array<string>",
            "(string, number, ...Array<string>) => number",
            vec![DecompFuncRest(vec![None, None, None], None)],
        );
    }

    #[test]
    fn fun_decomp_rest_arguments_overlap_with_normal_parameters() {
        mk_eval_hint_test(
            "[number, ...Array<string>]",
            "(string, number, ...Array<string>) => number",
            vec![DecompFuncRest(vec![None], None)],
        );
    }

    // When we try to extract the hint for the annotated parameter in
    // ```
    // declare function f<T>(T): void;
    // f((a) => ...);
    // ```
    // It should fail because we cannot destruct a type parameter without any bounds.
    #[test]
    fn fun_decomp_cannot_decomp_no_bound_type_parameter() {
        mk_eval_hint_test(
            "DecompositionError",
            "<T>(T) => void",
            vec![
                DecompFuncParam(vec![None], 0, None),
                DecompFuncParam(vec![None], 0, None),
            ],
        );
    }

    // When we try to extract the hint for the lambda in
    // ```
    // declare function f<T>(T => void) => void;
    // f((a) => {});
    // ```
    // In Pierce's algorithm, this is the case when the lambda must be fully annotated. However, we
    // can still choose to assign the lambda parameter a type by using the most conservative type we
    // can: mixed. If the users want something more precise, they will have to annotate.
    #[test]
    fn fun_decomp_unsolved_type_parameter() {
        mk_eval_hint_test(
            "(T) => void",
            "<T>(T => void) => void",
            vec![DecompFuncParam(vec![None], 0, None)],
        );
    }

    #[test]
    fn obj_prop_from_record_neutral_polarity() {
        mk_eval_hint_test(
            "number",
            "{foo: number}",
            vec![DecompObjProp(FlowSmolStr::from("foo"))],
        );
    }

    #[test]
    fn obj_prop_from_record_positive_polarity() {
        mk_eval_hint_test(
            "number",
            "{+foo: number}",
            vec![DecompObjProp(FlowSmolStr::from("foo"))],
        );
    }

    #[test]
    fn obj_prop_from_record_positive_polarity_from_readonly() {
        mk_eval_hint_test(
            "number",
            "$ReadOnly<{foo: number}>",
            vec![DecompObjProp(FlowSmolStr::from("foo"))],
        );
    }

    #[test]
    fn obj_prop_from_record_negative_polarity() {
        mk_eval_hint_test(
            "DecompositionError",
            "{-foo: number}",
            vec![DecompObjProp(FlowSmolStr::from("foo"))],
        );
    }

    #[test]
    fn obj_prop_from_dict_neutral_polarity() {
        mk_eval_hint_test(
            "number",
            "{[string]: number}",
            vec![DecompObjProp(FlowSmolStr::from("foo"))],
        );
    }

    #[test]
    fn obj_prop_from_dict_positive_polarity() {
        mk_eval_hint_test(
            "number",
            "{+[string]: number}",
            vec![DecompObjProp(FlowSmolStr::from("foo"))],
        );
    }

    #[test]
    fn obj_prop_from_dict_negative_polarity() {
        mk_eval_hint_test(
            "DecompositionError",
            "{-[string]: number}",
            vec![DecompObjProp(FlowSmolStr::from("foo"))],
        );
    }

    #[test]
    fn obj_prop_union() {
        mk_eval_hint_test(
            "number | string",
            "{foo: number} | {[string]: string}",
            vec![DecompObjProp(FlowSmolStr::from("foo"))],
        );
    }

    #[test]
    fn obj_prop_union_some_without_prop() {
        mk_eval_hint_test(
            "number | string | void",
            "{foo: number} | {[string]: string} | {bar: string}",
            vec![DecompObjProp(FlowSmolStr::from("foo"))],
        );
    }

    #[test]
    fn obj_prop_union_some_failed() {
        mk_eval_hint_test(
            "number | string",
            "{foo: number} | {[string]: string} | number",
            vec![DecompObjProp(FlowSmolStr::from("foo"))],
        );
    }

    #[test]
    fn obj_prop_optional() {
        mk_eval_hint_test(
            "number",
            "?{foo: number}",
            vec![DecompObjProp(FlowSmolStr::from("foo"))],
        );
    }

    #[test]
    fn obj_prop_intersection() {
        mk_eval_hint_test(
            "number",
            "{bar: string} & {foo: number} & {[string]: string}",
            vec![DecompObjProp(FlowSmolStr::from("foo"))],
        );
    }

    #[test]
    fn obj_prop_from_prototype() {
        mk_eval_hint_test(
            "number",
            "string",
            vec![DecompObjProp(FlowSmolStr::from("length"))],
        );
    }

    #[test]
    fn obj_rest_from_record_neutral_polarity() {
        mk_eval_hint_test("{foo: number}", "{foo: number}", vec![DecompObjSpread]);
    }

    #[test]
    fn obj_rest_from_record_positive_polarity() {
        mk_eval_hint_test("{foo: number}", "{+foo: number}", vec![DecompObjSpread]);
    }

    #[test]
    fn obj_rest_from_dict_neutral_polarity() {
        mk_eval_hint_test(
            "{[string]: number}",
            "{[string]: number}",
            vec![DecompObjSpread],
        );
    }

    #[test]
    fn obj_rest_from_dict_positive_polarity() {
        mk_eval_hint_test(
            "{readonly [string]: number}",
            "{+[string]: number}",
            vec![DecompObjSpread],
        );
    }

    #[test]
    fn obj_rest_from_dict_negative_polarity() {
        mk_eval_hint_test(
            "{writeonly [string]: number}",
            "{-[string]: number}",
            vec![DecompObjSpread],
        );
    }

    #[test]
    fn method_name_from_instance() {
        mk_eval_hint_test(
            "() => string",
            "string",
            vec![DecompMethodName(FlowSmolStr::from("toString"))],
        );
    }

    #[test]
    fn method_name_from_object() {
        mk_eval_hint_test(
            "() => number",
            "{foo: () => number}",
            vec![DecompMethodName(FlowSmolStr::from("foo"))],
        );
    }

    #[test]
    fn method_elem_from_dict() {
        mk_eval_hint_test(
            "() => number",
            "{[string]: () => number}",
            vec![DecompMethodElem],
        );
    }

    #[test]
    fn instance_private_method_call_from_method() {
        mk_private_method_eval_hint_test(
            "() => void",
            false,
            None,
            None,
            Some(Property::new(PropertyInner::Method {
                key_loc: Some(ALoc::none()),
                type_: fun_t(&[], void::make(dummy_reason())),
            })),
            None,
        );
    }

    #[test]
    fn instance_private_method_call_from_static_method() {
        mk_private_method_eval_hint_test(
            "() => void",
            true,
            None,
            None,
            None,
            Some(Property::new(PropertyInner::Method {
                key_loc: Some(ALoc::none()),
                type_: fun_t(&[], void::make(dummy_reason())),
            })),
        );
    }

    #[test]
    fn instance_private_method_call_from_field() {
        mk_private_method_eval_hint_test(
            "() => void",
            false,
            Some(Property::new(PropertyInner::Field(Box::new(FieldData {
                preferred_def_locs: None,
                key_loc: Some(ALoc::none()),
                type_: fun_t(&[], void::make(dummy_reason())),
                polarity: Polarity::Neutral,
            })))),
            None,
            None,
            None,
        );
    }

    #[test]
    fn instance_private_method_call_from_static_field() {
        mk_private_method_eval_hint_test(
            "() => void",
            true,
            None,
            Some(Property::new(PropertyInner::Field(Box::new(FieldData {
                preferred_def_locs: None,
                key_loc: Some(ALoc::none()),
                type_: fun_t(&[], void::make(dummy_reason())),
                polarity: Polarity::Neutral,
            })))),
            None,
            None,
        );
    }

    #[test]
    fn call_new_from_class() {
        mk_eval_hint_test_with_type_setup(
            "(bar: number, baz: boolean) => Foo",
            "class Foo { constructor(bar: number, baz: boolean) {} }; Foo",
            vec![DecompCallNew],
        );
    }

    #[test]
    fn call_new_from_class_default_ctor() {
        mk_eval_hint_test_with_type_setup("() => Foo", "class Foo { }; Foo", vec![DecompCallNew]);
    }

    #[test]
    fn call_new_from_class_polymorphic() {
        mk_eval_hint_test_with_type_setup(
            "<A, T>(a: A, t: T) => Foo<A>",
            "declare class Foo<A> { constructor<T>(a: A, t: T): void }; Foo",
            vec![DecompCallNew],
        );
    }

    #[test]
    fn call_new_from_class_overload() {
        mk_eval_hint_test_with_type_setup(
            "(<A>(a: A, t: number) => Foo<A>) & (<A>(a: A) => Foo<A>)",
            "declare class Foo<A> { constructor(a: A, t: number): void; constructor(a: A): void; }; Foo",
            vec![DecompCallNew],
        );
    }

    #[test]
    fn call_new_from_class_polymorphic_overload() {
        mk_eval_hint_test_with_type_setup(
            "(<A, T>(a: A, t: T) => Foo<A>) & (<A, T>(a: A) => Foo<A>)",
            "declare class Foo<A> { constructor<T>(a: A, t: T): void; constructor<T>(a: A): void; }; Foo",
            vec![DecompCallNew],
        );
    }

    #[test]
    fn call_super() {
        mk_eval_hint_test_with_type_setup(
            "(bar: number, baz: boolean) => Foo",
            "class Foo { constructor(bar: number, baz: boolean) {} }; new Foo()",
            vec![DecompCallSuper],
        );
    }

    #[test]
    fn jsx_props_of_class_component() {
        mk_eval_hint_test_with_type_setup(
            "ReactManagedAttributes<Class<MyComponent>, {bar: string, foo: number, ref?: RefSetter<MyComponent>}>",
            "import * as React from 'react'; class MyComponent extends React.Component<{bar: string, foo: number}> {}; MyComponent",
            vec![DecompJsxProps],
        );
    }

    #[test]
    fn jsx_props_of_function_component() {
        mk_eval_hint_test(
            "{bar: string, foo: number}",
            "({bar: string, foo: number}) => number",
            vec![DecompJsxProps],
        );
    }

    #[test]
    fn jsx_props_of_abstract_component() {
        mk_eval_hint_test(
            "{readonly bar: string, readonly foo: number, readonly ref: RefSetter<unknown>}",
            "component(ref: React.RefSetter<mixed>, bar: string, foo: number) ",
            vec![DecompJsxProps],
        );
    }

    #[test]
    fn jsx_props_select() {
        mk_eval_hint_test(
            "number",
            "({bar: string, foo: number}) => number",
            vec![DecompJsxProps, DecompObjProp(FlowSmolStr::from("foo"))],
        );
    }

    #[test]
    fn jsx_props_spread() {
        mk_eval_hint_test(
            "{bar: string, foo: number}",
            "({bar: string, foo: number}) => number",
            vec![DecompJsxProps, DecompObjSpread],
        );
    }

    #[test]
    fn jsx_fragment_ref() {
        mk_eval_hint_test_with_type_setup(
            "void",
            "import {Fragment} from 'react'; Fragment",
            vec![DecompJsxProps, DecompObjProp(FlowSmolStr::from("children"))],
        );
    }
}
