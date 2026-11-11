/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Port of `services/code_action/__tests__/refactor_extract_utils_tests.ml`

use flow_analysis::scope_builder;
use flow_analysis::ssa_builder;
use flow_common::options::Options;
use flow_parser::ast;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::FileKeyInner;
use flow_parser::loc::Loc;
use flow_parser::loc::Position;
use flow_parser::parse_program_file;
use flow_parser_utils::file_sig::FileSig;
use flow_parser_utils::file_sig::FileSigOptions;
use flow_parser_utils_output::js_layout_generator;
use flow_parser_utils_output::pretty_printer;
use flow_parser_utils_output::source::Source;
use flow_type_sig::type_sig_options::TypeSigOptions;

use crate::ast_extraction_utils::ast_extractor;
use crate::refactor_extract_utils::variable_analysis;

fn pretty_print(layout: &flow_parser_utils_output::layout::LayoutNode) -> String {
    let source: Source = pretty_printer::print(true, layout);
    source.contents()
}

fn dummy_filename() -> FileKey {
    FileKey::new(FileKeyInner::SourceFile("".to_string()))
}

fn type_sig_options() -> TypeSigOptions {
    TypeSigOptions::of_options(
        &Options::default(),
        false,
        Vec::new(),
        &dummy_filename(),
        false,
    )
}

fn parse(contents: &str) -> ast::Program<Loc, Loc> {
    let (ast, _) = parse_program_file::<()>(false, None, None, dummy_filename(), Ok(contents));
    ast
}

fn mk_loc(start_line: i32, start_col: i32, end_line: i32, end_col: i32) -> Loc {
    Loc {
        source: Some(dummy_filename()),
        start: Position {
            line: start_line,
            column: start_col,
        },
        end: Position {
            line: end_line,
            column: end_col,
        },
    }
}

// ========== extract_tests ==========

#[cfg(test)]
mod extract_tests {
    use super::*;

    fn assert_extracted(
        expected_statements: Option<&str>,
        expected_expression: Option<(Vec<ast_extractor::ConstantInsertionPoint>, &str)>,
        expected_type: Option<(Loc, &str)>,
        source: &str,
        extract_range: Loc,
    ) {
        let ast = parse(source);
        let tokens = ast_extractor::tokens(None, Some(dummy_filename()), source);
        let ast_extractor::Extracted {
            extracted_statements,
            extracted_expression,
            extracted_type,
        } = ast_extractor::extract(&tokens, &ast, extract_range);
        let extracted_statements_str = extracted_statements.map(|stmts| {
            let opts = js_layout_generator::default_opts();
            stmts
                .iter()
                .map(|stmt| {
                    let layout = js_layout_generator::statement(&opts, false, stmt);
                    pretty_print(&layout)
                })
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
        });
        let actual_extracted_expression = extracted_expression.map(|e| {
            let opts = js_layout_generator::default_opts();
            let layout = js_layout_generator::expression(&opts, None, &e.expression);
            (
                e.constant_insertion_points,
                pretty_print(&layout).trim().to_string(),
            )
        });
        let expected_extracted_expression =
            expected_expression.map(|(points, expr)| (points, expr.trim().to_string()));
        let actual_extracted_type = extracted_type.map(|t| {
            let opts = js_layout_generator::default_opts();
            let layout = js_layout_generator::type_(&opts, &t.type_);
            (
                t.directly_containing_statement_loc,
                pretty_print(&layout).trim().to_string(),
            )
        });
        let expected_extracted_type = expected_type.map(|(loc, t)| (loc, t.trim().to_string()));
        assert_eq!(
            expected_statements.map(|s| s.trim().to_string()),
            extracted_statements_str,
            "statements mismatch"
        );
        assert_eq!(
            expected_extracted_expression, actual_extracted_expression,
            "expression mismatch"
        );
        assert_eq!(
            expected_extracted_type, actual_extracted_type,
            "type mismatch"
        );
    }

    #[test]
    fn extract_statements_linear_exact_one_statement() {
        // Exactly select the first statement.
        let source = "
        const a = 3;
        let b = 4;
        console.log(\"I should not be selected\");
      ";
        assert_extracted(
            Some("const a = 3;"),
            None,
            None,
            source,
            mk_loc(2, 8, 2, 20),
        );
    }

    #[test]
    fn extract_statements_linear_exact_multiple_statements() {
        // Exactly select the first two statements.
        let source = "
        const a = 3;
        let b = 4;
        console.log(\"I should not be selected\");
      ";
        let expected_statements = "
const a = 3;
let b = 4;
      ";
        assert_extracted(
            Some(expected_statements),
            None,
            None,
            source,
            mk_loc(2, 8, 3, 18),
        );
    }

    #[test]
    fn extract_statements_linear_multiple_statements_with_whitespaces() {
        let source = "
        const a = 3;
        let b = 4;
        console.log(\"I should not be selected\");
      ";
        let expected_statements = "
const a = 3;
let b = 4;
      ";
        assert_extracted(
            Some(expected_statements),
            None,
            None,
            source,
            mk_loc(2, 0, 4, 0),
        );
    }

    #[test]
    fn extract_statements_linear_partial() {
        // Partially select a statement is not allowed.
        let source = "
        const a = 3;
        let b = 4;
        console.log(\"I should not be selected\");
      ";
        assert_extracted(None, None, None, source, mk_loc(2, 10, 3, 18));
    }

    #[test]
    fn extract_statements_with_nested() {
        // Exactly select the first 3 statements, where the second one is a nested block.
        let source = "
        const a = 3;
        {
          let b = 4;
        }
        foo(a + b);
        console.log(\"I should not be selected\");
      ";
        let expected_statements = "
const a = 3;
{
  let b = 4;
}
foo(a + b);
      ";
        assert_extracted(
            Some(expected_statements),
            None,
            None,
            source,
            mk_loc(2, 8, 6, 19),
        );
    }

    #[test]
    fn extract_statements_from_nested() {
        // Select statements from nested block
        let source = "
        const a = 3;
        {
          let b = 4;
        }
        foo(a + b);
        console.log(\"I should not be selected\");
      ";
        assert_extracted(Some("let b = 4;"), None, None, source, mk_loc(4, 0, 4, 20));
    }

    #[test]
    fn extract_statements_from_nested_partial() {
        // Selecting part of the statements from a nested block is not allowed
        let source = "
        const a = 3;
        {
          let b = 4;
          foo(a + b);
        }
        console.log(\"I should not be selected\");
      ";
        assert_extracted(None, None, None, source, mk_loc(2, 8, 4, 20));
    }

    #[test]
    fn extract_expression_basic() {
        assert_extracted(
            None,
            Some((
                vec![ast_extractor::ConstantInsertionPoint {
                    title: "Extract to constant in module scope".to_string(),
                    function_body_loc: None,
                    statement_loc: mk_loc(1, 0, 1, 11),
                }],
                "1",
            )),
            None,
            "const a = 1",
            mk_loc(1, 10, 1, 11),
        );
    }

    #[test]
    fn extract_expression_nested() {
        // When statement selection is empty, the chosen expression is good.
        assert_extracted(
            None,
            Some((
                vec![ast_extractor::ConstantInsertionPoint {
                    title: "Extract to constant in module scope".to_string(),
                    function_body_loc: None,
                    statement_loc: mk_loc(1, 0, 1, 15),
                }],
                "1 + 1",
            )),
            None,
            "const a = 1 + 1",
            mk_loc(1, 10, 1, 15),
        );
    }

    #[test]
    fn extract_expression_statement_without_semicolon() {
        // When the selection is both an expression and a statement
        assert_extracted(
            Some("\"hello\";"),
            Some((
                vec![ast_extractor::ConstantInsertionPoint {
                    title: "Extract to constant in module scope".to_string(),
                    function_body_loc: None,
                    statement_loc: mk_loc(1, 0, 1, 7),
                }],
                "\"hello\"",
            )),
            None,
            "'hello'",
            mk_loc(1, 0, 1, 7),
        );
    }

    #[test]
    fn extract_expression_statement_with_semicolon() {
        // Same as above case, but adding a semicolon makes the selection only statement.
        assert_extracted(
            Some("\"hello\";"),
            None,
            None,
            "'hello';",
            mk_loc(1, 0, 1, 8),
        );
    }

    #[test]
    fn extract_expression_ban_multiple() {
        assert_extracted(
            None,
            None,
            None,
            "const a = 1; const b = 2;",
            mk_loc(1, 9, 1, 25),
        );
    }

    #[test]
    fn extract_expression_allow_whitespace() {
        assert_extracted(
            None,
            Some((
                vec![ast_extractor::ConstantInsertionPoint {
                    title: "Extract to constant in module scope".to_string(),
                    function_body_loc: None,
                    statement_loc: mk_loc(1, 0, 1, 16),
                }],
                "1",
            )),
            None,
            "const a =  1   ;",
            mk_loc(1, 10, 1, 14),
        );
    }

    #[test]
    fn extract_expression_overlap_disallowed() {
        assert_extracted(None, None, None, "let a = 1;", mk_loc(1, 6, 1, 9));
        assert_extracted(None, None, None, "let a = 1;", mk_loc(1, 7, 1, 10));
    }

    #[test]
    fn extract_expression_ban_partial_statement() {
        // Selecting `1;`.
        // Although expression is contained, illegal selection of statement invalidates it.
        assert_extracted(
            None,
            None,
            None,
            "const a = 1; const b = 2;",
            mk_loc(1, 8, 1, 11),
        );
        assert_extracted(
            None,
            None,
            None,
            "const a = 1; const b = 2;",
            mk_loc(1, 10, 1, 16),
        );
    }

    #[test]
    fn ban_expression_only_in_class() {
        // Expression only exists in a class, not in any statement.
        assert_extracted(
            None,
            Some((
                vec![ast_extractor::ConstantInsertionPoint {
                    title: "Extract to constant in module scope".to_string(),
                    function_body_loc: None,
                    statement_loc: mk_loc(1, 0, 1, 17),
                }],
                "3",
            )),
            None,
            "class A { a = 3 }",
            mk_loc(1, 14, 1, 15),
        );
        assert_extracted(
            None,
            Some((
                vec![ast_extractor::ConstantInsertionPoint {
                    title: "Extract to constant in module scope".to_string(),
                    function_body_loc: None,
                    statement_loc: mk_loc(1, 0, 1, 33),
                }],
                "3",
            )),
            None,
            "class A { constructor(a = 3) {} }",
            mk_loc(1, 26, 1, 27),
        );
    }

    #[test]
    fn expression_in_class_statements() {
        // Expression exists in a class, but also in a statement.
        assert_extracted(
            None,
            Some((
                vec![
                    ast_extractor::ConstantInsertionPoint {
                        title: "Extract to constant in method 'foo'".to_string(),
                        function_body_loc: Some(mk_loc(1, 16, 1, 32)),
                        statement_loc: mk_loc(1, 18, 1, 30),
                    },
                    ast_extractor::ConstantInsertionPoint {
                        title: "Extract to constant in module scope".to_string(),
                        function_body_loc: None,
                        statement_loc: mk_loc(1, 0, 1, 34),
                    },
                ],
                "3",
            )),
            None,
            "class A { foo() { const a = 3; } }",
            mk_loc(1, 28, 1, 29),
        );
    }

    #[test]
    fn expression_in_many_scopes() {
        let source = "
        class A {
          foo() {
            function bar() {
              const a = 3;
            }
          }
        }
        ";
        assert_extracted(
            None,
            Some((
                vec![
                    ast_extractor::ConstantInsertionPoint {
                        title: "Extract to constant in function 'bar'".to_string(),
                        function_body_loc: Some(mk_loc(4, 27, 6, 13)),
                        statement_loc: mk_loc(5, 14, 5, 26),
                    },
                    ast_extractor::ConstantInsertionPoint {
                        title: "Extract to constant in method 'foo'".to_string(),
                        function_body_loc: Some(mk_loc(3, 16, 7, 11)),
                        statement_loc: mk_loc(4, 12, 6, 13),
                    },
                    ast_extractor::ConstantInsertionPoint {
                        title: "Extract to constant in module scope".to_string(),
                        function_body_loc: None,
                        statement_loc: mk_loc(2, 8, 8, 9),
                    },
                ],
                "3",
            )),
            None,
            source,
            mk_loc(5, 24, 5, 25),
        );
    }

    #[test]
    fn extract_types() {
        assert_extracted(
            None,
            None,
            Some((mk_loc(1, 0, 1, 20), "number")),
            "const a: number = 1;",
            mk_loc(1, 9, 1, 15),
        );
        assert_extracted(
            None,
            None,
            Some((mk_loc(1, 0, 1, 30), "number")),
            "const a: [number, number] = 1;",
            mk_loc(1, 10, 1, 16),
        );
        assert_extracted(
            None,
            None,
            Some((mk_loc(1, 0, 1, 30), "[number, number]")),
            "const a: [number, number] = 1;",
            mk_loc(1, 9, 1, 25),
        );
    }
}

// ========== collect_function_method_inserting_points_tests ==========

#[cfg(test)]
mod collect_function_method_inserting_points_tests {
    use std::rc::Rc;
    use std::sync::Arc;

    use dupe::Dupe;
    use flow_aloc::ALoc;
    use flow_aloc::ALocTable;
    use flow_aloc::LazyALocTable;
    use flow_data_structure_wrapper::ord_map::FlowOrdMap;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_lint_settings::lint_settings::LintSettings;
    use flow_lint_settings::severity::Severity;
    use flow_parser::loc_sig::LocSig;
    use flow_parser_utils::file_sig::FileSig;
    use flow_typing::type_inference;
    use flow_typing_builtins::builtins::Builtins;
    use flow_typing_context::Context;
    use flow_typing_context::ContextArena;
    use flow_typing_context::ContextStore;
    use flow_typing_context::Cx;
    use flow_typing_context::Metadata;
    use flow_typing_context::ResolveRequire;
    use flow_typing_context::ResolvedRequire;
    use flow_typing_type::type_::AnyErrorKind;
    use flow_typing_type::type_::AnySource;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;

    use super::*;
    use crate::ast_extraction_utils::insertion_point_collectors;

    fn dummy_context<'cx>(arena: &'cx mut ContextArena) -> Context<'cx> {
        let reason = flow_common::reason::mk_reason(None, ALoc::none());
        let any_t = |r: &flow_common::reason::Reason| -> Type {
            Type::new(TypeInner::AnyT(
                r.dupe(),
                AnySource::AnyError(Some(AnyErrorKind::UnresolvedName)),
            ))
        };
        let mut builtins_values: FlowOrdMap<FlowSmolStr, flow_typing_builtins::LazyVal<Cx>> =
            FlowOrdMap::new();
        for name in &[
            "console",
            "Object",
            "Generator",
            "AsyncGenerator",
            "Promise",
            "promise",
        ] {
            let r = reason.dupe();
            let t = any_t(&r);
            let val = Rc::new(flow_lazy::Lazy::new(move |_cx: &mut Context<'_>| {
                (ALoc::none(), t)
            }));
            builtins_values.insert(FlowSmolStr::from(*name), val);
        }
        let mut builtin_types: FlowOrdMap<FlowSmolStr, flow_typing_builtins::LazyVal<Cx>> =
            FlowOrdMap::new();
        for name in &[
            "AsyncIterable",
            "Iterable",
            "React$JSXIntrinsicElements",
            "React$CreateElement",
            "ExactReactElement_DEPRECATED",
            "React$Key",
        ] {
            let r = reason.dupe();
            let t = any_t(&r);
            let val = Rc::new(flow_lazy::Lazy::new(move |_cx: &mut Context<'_>| {
                (ALoc::none(), t)
            }));
            builtin_types.insert(FlowSmolStr::from(*name), val);
        }
        let mut metadata = Metadata::default();
        metadata.overridable.checked = true;
        let file = dummy_filename();
        let aloc_table: LazyALocTable = Rc::new(std::cell::LazyCell::new(Box::new({
            let file = file.dupe();
            move || Rc::new(ALocTable::empty(file))
        })
            as Box<dyn FnOnce() -> Rc<ALocTable>>));
        let resolve_reason = reason.dupe();
        let resolve_require: ResolveRequire = Rc::new(
            move |_cx: &mut Context<'_>,
                  _: &flow_common::flow_import_specifier::FlowImportSpecifier| {
                let r = resolve_reason.dupe();
                ResolvedRequire::TypedModule(Rc::new(move |_cx: &mut Context<'_>| {
                    Err(Type::new(TypeInner::AnyT(
                        r.dupe(),
                        AnySource::AnyError(Some(AnyErrorKind::UnresolvedName)),
                    )))
                }))
            },
        );
        let builtins_values_clone = builtins_values.dupe();
        let builtin_types_clone = builtin_types.dupe();
        let ccx = arena.add_component();
        let cx_id = arena.add_context(
            ccx,
            metadata,
            file,
            Arc::default(),
            aloc_table,
            resolve_require,
            Rc::new(move |_cx: &mut Context<'_>| {
                Builtins::of_name_map(
                    builtins_values_clone.dupe(),
                    builtin_types_clone.dupe(),
                    FlowOrdMap::new(),
                )
            }),
            flow_utils_concurrency::check_budget::CheckBudget::new(None),
        );
        Context::new(ContextStore::new(vec![arena]), cx_id)
    }

    fn typed_ast_of_ast(
        cx: &mut Context,
        ast: &ast::Program<Loc, Loc>,
    ) -> ast::Program<ALoc, (ALoc, Type)> {
        let comments = &ast.all_comments;
        let aloc_ast = flow_aloc::loc_to_aloc_ast(ast);
        let metadata = cx.metadata().clone();
        type_inference::infer_ast(
            &LintSettings::<Severity>::empty_severities(),
            cx,
            &dummy_filename(),
            Arc::new(FileSig::empty()),
            &metadata,
            comments,
            ast,
            aloc_ast,
            &type_sig_options(),
            None,
        )
        .expect("infer_ast should not be canceled in test")
    }

    #[test]
    fn basic_test() {
        let handle = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                let source = "
console.log(\"0\");
function foo<A>() {
  function bar<B>() {
    class Foo {
      baz1 = () => {
        class Bar {
          baz2() {
            console.log(\"2\"); // selected
          }
        }
      }
    }
  }
}
        ";
                let ast = parse(source);
                let mut arena = ContextArena::new();
                let mut cx = dummy_context(&mut arena);
                let typed_ast = typed_ast_of_ast(&mut cx, &ast);
                let extracted_loc = mk_loc(9, 12, 9, 42);
                let actual: Vec<(FlowSmolStr, Loc, bool, Vec<String>)> =
                    insertion_point_collectors::collect_function_method_inserting_points(
                        &typed_ast,
                        // loc_of_aloc: in test context, ALoc is identity-mapped from Loc
                        &|aloc: &ALoc| aloc.to_loc_exn().clone(),
                        extracted_loc,
                    )
                    .into_iter()
                    .map(|point| {
                        let tparam_names: Vec<String> = point
                            .tparams_rev
                            .iter()
                            .map(|tp| tp.name.string_of_subst_name().to_string())
                            .collect();
                        (
                            point.function_name,
                            point.body_loc,
                            point.is_method,
                            tparam_names,
                        )
                    })
                    .collect();
                let expected: Vec<(FlowSmolStr, Loc, bool, Vec<String>)> = vec![
                    (
                        "baz2".into(),
                        mk_loc(8, 17, 10, 11),
                        true,
                        vec!["B".to_string(), "A".to_string()],
                    ),
                    (
                        "baz1".into(),
                        mk_loc(6, 19, 12, 7),
                        true,
                        vec!["B".to_string(), "A".to_string()],
                    ),
                    (
                        "bar".into(),
                        mk_loc(4, 20, 14, 3),
                        false,
                        vec!["B".to_string(), "A".to_string()],
                    ),
                    (
                        "foo".into(),
                        mk_loc(3, 18, 15, 1),
                        false,
                        vec!["A".to_string()],
                    ),
                ];
                let printer = |result: &[(FlowSmolStr, Loc, bool, Vec<String>)]| -> String {
                    result
                        .iter()
                        .map(|(id, loc, is_method, typeparams)| {
                            format!(
                                "{}: {:?}: <{}>. is_method: {}",
                                id,
                                loc,
                                typeparams.join(","),
                                is_method,
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                assert_eq!(
                    printer(&expected),
                    printer(&actual),
                    "\nExpected:\n{}\n\nActual:\n{}",
                    printer(&expected),
                    printer(&actual),
                );
            })
            .expect("failed to spawn thread");
        handle.join().expect("test thread panicked");
    }
}

// ========== find_closest_enclosing_class_tests ==========

#[cfg(test)]
mod find_closest_enclosing_class_tests {
    use std::rc::Rc;
    use std::sync::Arc;

    use dupe::Dupe;
    use flow_aloc::ALoc;
    use flow_aloc::ALocTable;
    use flow_aloc::LazyALocTable;
    use flow_data_structure_wrapper::ord_map::FlowOrdMap;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_lint_settings::lint_settings::LintSettings;
    use flow_lint_settings::severity::Severity;
    use flow_parser::loc_sig::LocSig;
    use flow_parser_utils::file_sig::FileSig;
    use flow_typing::type_inference;
    use flow_typing_builtins::builtins::Builtins;
    use flow_typing_context::Context;
    use flow_typing_context::ContextArena;
    use flow_typing_context::ContextStore;
    use flow_typing_context::Cx;
    use flow_typing_context::Metadata;
    use flow_typing_context::ResolveRequire;
    use flow_typing_context::ResolvedRequire;
    use flow_typing_type::type_::AnyErrorKind;
    use flow_typing_type::type_::AnySource;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;

    use super::*;
    use crate::ast_extraction_utils::insertion_point_collectors;

    fn dummy_context<'cx>(arena: &'cx mut ContextArena) -> Context<'cx> {
        let reason = flow_common::reason::mk_reason(None, ALoc::none());
        let any_t = |r: &flow_common::reason::Reason| -> Type {
            Type::new(TypeInner::AnyT(
                r.dupe(),
                AnySource::AnyError(Some(AnyErrorKind::UnresolvedName)),
            ))
        };
        let mut builtins_values: FlowOrdMap<FlowSmolStr, flow_typing_builtins::LazyVal<Cx>> =
            FlowOrdMap::new();
        for name in &[
            "console",
            "Object",
            "Generator",
            "AsyncGenerator",
            "Promise",
            "promise",
        ] {
            let r = reason.dupe();
            let t = any_t(&r);
            let val = Rc::new(flow_lazy::Lazy::new(move |_cx: &mut Context<'_>| {
                (ALoc::none(), t)
            }));
            builtins_values.insert(FlowSmolStr::from(*name), val);
        }
        let mut builtin_types: FlowOrdMap<FlowSmolStr, flow_typing_builtins::LazyVal<Cx>> =
            FlowOrdMap::new();
        for name in &[
            "AsyncIterable",
            "Iterable",
            "React$JSXIntrinsicElements",
            "React$CreateElement",
            "ExactReactElement_DEPRECATED",
            "React$Key",
        ] {
            let r = reason.dupe();
            let t = any_t(&r);
            let val = Rc::new(flow_lazy::Lazy::new(move |_cx: &mut Context<'_>| {
                (ALoc::none(), t)
            }));
            builtin_types.insert(FlowSmolStr::from(*name), val);
        }
        let mut metadata = Metadata::default();
        metadata.overridable.checked = true;
        let file = dummy_filename();
        let aloc_table: LazyALocTable = Rc::new(std::cell::LazyCell::new(Box::new({
            let file = file.dupe();
            move || Rc::new(ALocTable::empty(file))
        })
            as Box<dyn FnOnce() -> Rc<ALocTable>>));
        let resolve_reason = reason.dupe();
        let resolve_require: ResolveRequire = Rc::new(
            move |_cx: &mut Context<'_>,
                  _: &flow_common::flow_import_specifier::FlowImportSpecifier| {
                let r = resolve_reason.dupe();
                ResolvedRequire::TypedModule(Rc::new(move |_cx: &mut Context<'_>| {
                    Err(Type::new(TypeInner::AnyT(
                        r.dupe(),
                        AnySource::AnyError(Some(AnyErrorKind::UnresolvedName)),
                    )))
                }))
            },
        );
        let builtins_values_clone = builtins_values.dupe();
        let builtin_types_clone = builtin_types.dupe();
        let ccx = arena.add_component();
        let cx_id = arena.add_context(
            ccx,
            metadata,
            file,
            Arc::default(),
            aloc_table,
            resolve_require,
            Rc::new(move |_cx: &mut Context<'_>| {
                Builtins::of_name_map(
                    builtins_values_clone.dupe(),
                    builtin_types_clone.dupe(),
                    FlowOrdMap::new(),
                )
            }),
            flow_utils_concurrency::check_budget::CheckBudget::new(None),
        );
        Context::new(ContextStore::new(vec![arena]), cx_id)
    }

    fn typed_ast_of_ast(
        cx: &mut Context,
        ast: &ast::Program<Loc, Loc>,
    ) -> ast::Program<ALoc, (ALoc, Type)> {
        let comments = &ast.all_comments;
        let aloc_ast = flow_aloc::loc_to_aloc_ast(ast);
        let metadata = cx.metadata().clone();
        type_inference::infer_ast(
            &LintSettings::<Severity>::empty_severities(),
            cx,
            &dummy_filename(),
            Arc::new(FileSig::empty()),
            &metadata,
            comments,
            ast,
            aloc_ast,
            &type_sig_options(),
            None,
        )
        .expect("infer_ast should not be canceled in test")
    }

    fn assert_closest_enclosing_class_scope(
        expected: Option<(Option<&str>, Loc, Vec<&str>)>,
        source: &str,
        extracted_loc: Loc,
    ) {
        let ast = parse(source);
        let mut arena = ContextArena::new();
        let mut cx = dummy_context(&mut arena);
        let typed_ast = typed_ast_of_ast(&mut cx, &ast);
        let actual: Option<(Option<FlowSmolStr>, Loc, Vec<String>)> =
            insertion_point_collectors::find_closest_enclosing_class(
                &typed_ast,
                &|aloc: &ALoc| aloc.to_loc_exn().clone(),
                extracted_loc,
            )
            .map(|point| {
                let tparam_names: Vec<String> = point
                    .tparams_rev
                    .iter()
                    .map(|tp| tp.name.string_of_subst_name().to_string())
                    .collect();
                (point.class_name, point.body_loc, tparam_names)
            });
        let printer = |result: &Option<(Option<FlowSmolStr>, Loc, Vec<String>)>| -> String {
            match result {
                None => "None".to_string(),
                Some((id, loc, typeparams)) => {
                    format!(
                        "{}: {:?}: <{}>",
                        id.as_deref().unwrap_or("anonymous"),
                        loc,
                        typeparams.join(","),
                    )
                }
            }
        };
        let expected_owned: Option<(Option<FlowSmolStr>, Loc, Vec<String>)> =
            expected.map(|(name, loc, tparams)| {
                (
                    name.map(|s: &str| FlowSmolStr::from(s)),
                    loc,
                    tparams.iter().map(|s| s.to_string()).collect(),
                )
            });
        assert_eq!(
            printer(&expected_owned),
            printer(&actual),
            "\nExpected:\n{}\n\nActual:\n{}",
            printer(&expected_owned),
            printer(&actual),
        );
    }

    // Shared source for all find_closest_enclosing_class tests
    fn source() -> &'static str {
        "
console.log(\"0\");
export default class <A> {
  test1() {
    console.log(\"1\");
    class Level2<B> {
      test2(v: B) {
        console.log(\"2\");
      }
    }
  }
}
  "
    }

    #[test]
    fn without_enclosing_class_scope() {
        assert_closest_enclosing_class_scope(None, source(), mk_loc(2, 0, 2, 17));
    }

    #[test]
    fn mid_enclosing_class_scope() {
        assert_closest_enclosing_class_scope(
            Some((None, mk_loc(3, 25, 12, 1), vec!["A"])),
            source(),
            mk_loc(5, 4, 10, 5),
        );
    }

    #[test]
    fn inner_most_enclosing_class_scope() {
        assert_closest_enclosing_class_scope(
            Some((Some("Level2"), mk_loc(6, 20, 10, 5), vec!["B", "A"])),
            source(),
            mk_loc(8, 8, 8, 25),
        );
    }
}

#[cfg(test)]
mod collect_relevant_defs_with_scope_tests {
    use super::*;

    fn assert_relevant_defs(
        expected_defs_of_local_uses: &[&str],
        expected_vars_with_shadowed_local_reassignments: &[&str],
        source: &str,
        extracted_loc: Loc,
    ) {
        let ast = parse(source);
        let file_sig = FileSig::from_program(
            &dummy_filename(),
            &ast,
            &FileSigOptions {
                enable_enums: true,
                ..Default::default()
            },
        );
        let scope_info = scope_builder::program(true, true, file_sig.requires().is_empty(), &ast);
        let (_abnormal_completion_state, (ssa_values, _possible_globals)) =
            ssa_builder::program_with_scope(true, file_sig.requires().is_empty(), &ast);
        let result = variable_analysis::collect_relevant_defs_with_scope(
            &scope_info,
            &ssa_values,
            &extracted_loc,
        );
        let mut actual_defs_of_local_uses: Vec<String> = result
            .defs_with_scopes_of_local_uses
            .iter()
            .map(|(def, scope)| {
                assert!(
                    scope.defs.contains_key(&def.actual_name),
                    "scope does not contain def"
                );
                def.actual_name.to_string()
            })
            .collect();
        actual_defs_of_local_uses.sort();
        let mut actual_vars_with_shadowed_local_reassignments: Vec<String> = result
            .vars_with_shadowed_local_reassignments
            .iter()
            .map(|(name, _)| name.to_string())
            .collect();
        actual_vars_with_shadowed_local_reassignments.sort();
        let mut expected_defs: Vec<String> = expected_defs_of_local_uses
            .iter()
            .map(|s| s.to_string())
            .collect();
        expected_defs.sort();
        let mut expected_vars: Vec<String> = expected_vars_with_shadowed_local_reassignments
            .iter()
            .map(|s| s.to_string())
            .collect();
        expected_vars.sort();
        assert_eq!(
            expected_defs, actual_defs_of_local_uses,
            "defs_of_local_uses mismatch"
        );
        assert_eq!(
            expected_vars, actual_vars_with_shadowed_local_reassignments,
            "vars_with_shadowed_local_reassignments mismatch"
        );
    }

    #[test]
    fn used_defs_toplevel() {
        let source = "
        const a = 3;
        let b = 4;
        console.log(a + b);
      ";
        assert_relevant_defs(&["a", "b"], &[], source, mk_loc(4, 8, 4, 27));
    }

    #[test]
    fn used_defs_in_function() {
        let source = "
        const a = 3;
        let b = 4;
        function test() {
          return a + b;
        }
        console.log(\"I should not be selected\");
      ";
        assert_relevant_defs(&["a", "b"], &[], source, mk_loc(5, 10, 5, 23));
    }

    #[test]
    fn used_defs_in_many_scopes() {
        let source = "
        const a = 3;
        function level1() {
          const b = 4;
          function level2() {
            function level3() {
              const d = 6;
              function level4() {
                const e = 7;
                return a + b + c + d + e;
              }
            }
            var c = 5;
          }
        }
        console.log(\"I should not be selected\");
      ";
        assert_relevant_defs(
            &["a", "b", "c", "d", "e"],
            &[],
            source,
            mk_loc(10, 16, 10, 49),
        );
    }

    #[test]
    fn basic_def_with_shadowed_local_reassignment() {
        let source = "
        let a = 4;
        a = 3; // selected
        console.log(a);
      ";
        assert_relevant_defs(&["a"], &["a"], source, mk_loc(3, 8, 3, 26));
    }

    #[test]
    fn basic_def_without_shadowed_local_reassignment() {
        let source = "
        let a = 4; // selected
        a = 3; // selected
        console.log(a);
      ";
        assert_relevant_defs(&["a"], &[], source, mk_loc(2, 8, 3, 26));
    }
}

// ========== undefined_variables_after_extraction_tests ==========

#[cfg(test)]
mod undefined_variables_after_extraction_tests {
    use super::*;

    fn assert_undefined_variables(
        expected: &[&str],
        source: &str,
        new_function_target_scope_loc: Option<Loc>,
        extracted_loc: Loc,
    ) {
        let ast = parse(source);
        let file_sig = FileSig::from_program(
            &dummy_filename(),
            &ast,
            &FileSigOptions {
                enable_enums: true,
                ..Default::default()
            },
        );
        let scope_info = scope_builder::program(true, true, file_sig.requires().is_empty(), &ast);
        let (_abnormal_completion_state, (ssa_values, _possible_globals)) =
            ssa_builder::program_with_scope(true, file_sig.requires().is_empty(), &ast);
        let result = variable_analysis::collect_relevant_defs_with_scope(
            &scope_info,
            &ssa_values,
            &extracted_loc,
        );
        let mut actual: Vec<String> = variable_analysis::undefined_variables_after_extraction(
            &scope_info,
            &result.defs_with_scopes_of_local_uses,
            new_function_target_scope_loc.as_ref(),
            &extracted_loc,
        )
        .iter()
        .map(|(name, _)| name.to_string())
        .collect();
        actual.sort();
        let mut expected: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        expected.sort();
        assert_eq!(expected, actual);
    }

    #[test]
    fn toplevel_only_extract() {
        let source = "
        const a = 3;
        let b = a;
        console.log(b + c); // selected
      ";
        assert_undefined_variables(&[], source, None, mk_loc(4, 2, 4, 28));
    }

    #[test]
    fn basic_function_extract_to_toplevel() {
        let source = "
        const a = 3;
        let b = a;
        function test() {
          const c = 4;
          console.log(b + c);
        }
      ";
        assert_undefined_variables(&["c"], source, None, mk_loc(6, 10, 6, 29));
    }

    #[test]
    fn extract_use_of_toplevel_function_parameter_to_toplevel() {
        let source = "
        function test(a: number) {
          console.log(a);
        }
      ";
        assert_undefined_variables(&["a"], source, None, mk_loc(3, 10, 3, 25));
    }

    #[test]
    fn basic_function_extract_to_function() {
        let source = "
        const a = 3;
        let b = a;
        function test() {
          const c = 4;
          console.log(b + c);
        }
      ";
        assert_undefined_variables(&[], source, Some(mk_loc(4, 24, 7, 9)), mk_loc(6, 10, 6, 29));
    }

    #[test]
    fn deeply_nested_extract_to_inner_function() {
        let source = "
        const a = 3;
        let b = a;
        function level1() {
          const c = 4;
          function level2() {
            const e = d;
            {
              const f = 3;
              const level3 = () => {
                return b + c + d + e + f + level2() + level3();
              }
            }
          }
          var d = b + c;
          return level2();
        }
      ";
        assert_undefined_variables(
            &["f", "level3"],
            source,
            Some(mk_loc(6, 28, 14, 11)),
            mk_loc(11, 16, 11, 63),
        );
    }
}

// ========== escaping_locally_defined_variables_tests ==========

#[cfg(test)]
mod escaping_locally_defined_variables_tests {
    use super::*;

    fn assert_escaping_locally_defined_variables(
        expected_variables: &[&str],
        expected_has_external_writes: bool,
        source: &str,
        extracted_statements_loc: Loc,
    ) {
        let ast = parse(source);
        let file_sig = FileSig::from_program(
            &dummy_filename(),
            &ast,
            &FileSigOptions {
                enable_enums: true,
                ..Default::default()
            },
        );
        let scope_info = scope_builder::program(true, true, file_sig.requires().is_empty(), &ast);
        let (_abnormal_completion_state, (ssa_values, _possible_globals)) =
            ssa_builder::program_with_scope(true, file_sig.requires().is_empty(), &ast);
        let result = variable_analysis::collect_escaping_local_defs(
            &scope_info,
            &ssa_values,
            &extracted_statements_loc,
        );
        let mut actual_variables: Vec<String> = result
            .escaping_variables
            .iter()
            .map(|(name, _)| name.to_string())
            .collect();
        actual_variables.sort();
        let mut expected_vars: Vec<String> =
            expected_variables.iter().map(|s| s.to_string()).collect();
        expected_vars.sort();
        assert_eq!(
            expected_vars, actual_variables,
            "escaping variables mismatch"
        );
        assert_eq!(
            expected_has_external_writes, result.has_external_writes,
            "has_external_writes mismatch"
        );
    }

    #[test]
    fn non_escaping() {
        let source = "
        // selected start
        const a = 3;
        const b = a;
        // selected end
        console.log(\"not using a and b\");
      ";
        assert_escaping_locally_defined_variables(&[], false, source, mk_loc(2, 8, 4, 20));
    }

    #[test]
    fn all_escaping() {
        let source = "
        // selected start
        const a = 3;
        const b = a;
        // selected end
        console.log(a+b);
      ";
        assert_escaping_locally_defined_variables(&["a", "b"], false, source, mk_loc(2, 8, 4, 20));
    }

    #[test]
    fn escaping_with_hoisting() {
        let source = "
        console.log(test());
        // selected start
        const a = 3;
        function test() { return a; }
        // selected end
      ";
        assert_escaping_locally_defined_variables(&["test"], false, source, mk_loc(3, 8, 5, 37));
    }

    #[test]
    fn escaping_with_hoisting_in_function() {
        let source = "
        function foo() {
          console.log(test());
          // selected start
          const a = 3;
          function test() { return a; }
          // selected end
        }
      ";
        assert_escaping_locally_defined_variables(&["test"], false, source, mk_loc(4, 10, 6, 39));
    }

    #[test]
    fn escaping_with_shadowing() {
        let source = "
        const a = 3;
        function test() { const a = 3; return a; }
      ";
        assert_escaping_locally_defined_variables(&[], false, source, mk_loc(2, 8, 2, 20));
    }

    #[test]
    fn escaping_with_hoising_shadowing_nested_scopes() {
        let source = "
        const a = 3;
        function level1() {
          const b = 4;
          const c = b + 4;
          const d = c + b;
          {
            return c + level2();
          }

          function level2() {
            const d = 5;
            return c + d;
          }
        }
      ";
        assert_escaping_locally_defined_variables(&["c"], false, source, mk_loc(4, 22, 6, 26));
    }

    #[test]
    fn escaping_with_external_assignments() {
        let source = "
        let a = 3, b = 2; // selected
        a = 2;
        console.log(b);
      ";
        assert_escaping_locally_defined_variables(&["a", "b"], true, source, mk_loc(2, 8, 2, 37));
    }
}

// ========== type_synthesizer_tests ==========

#[cfg(test)]
mod type_synthesizer_tests {
    use std::collections::BTreeSet;
    use std::rc::Rc;
    use std::sync::Arc;

    use dupe::Dupe;
    use flow_aloc::ALoc;
    use flow_aloc::ALocTable;
    use flow_aloc::LazyALocTable;
    use flow_data_structure_wrapper::ord_map::FlowOrdMap;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_lint_settings::lint_settings::LintSettings;
    use flow_lint_settings::severity::Severity;
    use flow_parser::loc_sig::LocSig;
    use flow_parser_utils::file_sig::FileSig;
    use flow_parser_utils::file_sig::FileSigOptions;
    use flow_parser_utils_output::js_layout_generator;
    use flow_typing::type_inference;
    use flow_typing_builtins::builtins::Builtins;
    use flow_typing_context::Context;
    use flow_typing_context::ContextArena;
    use flow_typing_context::ContextStore;
    use flow_typing_context::Cx;
    use flow_typing_context::Metadata;
    use flow_typing_context::ResolveRequire;
    use flow_typing_context::ResolvedRequire;
    use flow_typing_type::type_::AnyErrorKind;
    use flow_typing_type::type_::AnySource;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;

    use super::*;
    use crate::insert_type;
    use crate::refactor_extract_utils::type_synthesizer;

    fn dummy_context<'cx>(arena: &'cx mut ContextArena) -> Context<'cx> {
        let reason = flow_common::reason::mk_reason(None, ALoc::none());
        let any_t = |r: &flow_common::reason::Reason| -> Type {
            Type::new(TypeInner::AnyT(
                r.dupe(),
                AnySource::AnyError(Some(AnyErrorKind::UnresolvedName)),
            ))
        };
        let mut builtins_values: FlowOrdMap<FlowSmolStr, flow_typing_builtins::LazyVal<Cx>> =
            FlowOrdMap::new();
        for name in &[
            "console",
            "Object",
            "Generator",
            "AsyncGenerator",
            "Promise",
            "promise",
            "$IterableOrAsyncIterableInternal",
        ] {
            let r = reason.dupe();
            let t = any_t(&r);
            let val = Rc::new(flow_lazy::Lazy::new(move |_cx: &mut Context<'_>| {
                (ALoc::none(), t)
            }));
            builtins_values.insert(FlowSmolStr::from(*name), val);
        }
        let mut builtin_types: FlowOrdMap<FlowSmolStr, flow_typing_builtins::LazyVal<Cx>> =
            FlowOrdMap::new();
        for name in &[
            "AsyncIterable",
            "Iterable",
            "React$JSXIntrinsicElements",
            "React$CreateElement",
            "ExactReactElement_DEPRECATED",
            "React$Key",
        ] {
            let r = reason.dupe();
            let t = any_t(&r);
            let val = Rc::new(flow_lazy::Lazy::new(move |_cx: &mut Context<'_>| {
                (ALoc::none(), t)
            }));
            builtin_types.insert(FlowSmolStr::from(*name), val);
        }
        let mut metadata = Metadata::default();
        metadata.overridable.checked = true;
        let file = dummy_filename();
        let aloc_table: LazyALocTable = Rc::new(std::cell::LazyCell::new(Box::new({
            let file = file.dupe();
            move || Rc::new(ALocTable::empty(file))
        })
            as Box<dyn FnOnce() -> Rc<ALocTable>>));
        let resolve_reason = reason.dupe();
        let resolve_require: ResolveRequire = Rc::new(
            move |_cx: &mut Context<'_>,
                  _: &flow_common::flow_import_specifier::FlowImportSpecifier| {
                let r = resolve_reason.dupe();
                ResolvedRequire::TypedModule(Rc::new(move |_cx: &mut Context<'_>| {
                    Err(Type::new(TypeInner::AnyT(
                        r.dupe(),
                        AnySource::AnyError(Some(AnyErrorKind::UnresolvedName)),
                    )))
                }))
            },
        );
        let builtins_values_clone = builtins_values.dupe();
        let builtin_types_clone = builtin_types.dupe();
        let ccx = arena.add_component();
        let cx_id = arena.add_context(
            ccx,
            metadata,
            file,
            Arc::default(),
            aloc_table,
            resolve_require,
            Rc::new(move |_cx: &mut Context<'_>| {
                Builtins::of_name_map(
                    builtins_values_clone.dupe(),
                    builtin_types_clone.dupe(),
                    FlowOrdMap::new(),
                )
            }),
            flow_utils_concurrency::check_budget::CheckBudget::new(None),
        );
        Context::new(ContextStore::new(vec![arena]), cx_id)
    }

    fn typed_ast_of_ast(
        cx: &mut Context,
        ast: &ast::Program<Loc, Loc>,
    ) -> ast::Program<ALoc, (ALoc, Type)> {
        let comments = &ast.all_comments;
        let aloc_ast = flow_aloc::loc_to_aloc_ast(ast);
        let metadata = cx.metadata().clone();
        type_inference::infer_ast(
            &LintSettings::<Severity>::empty_severities(),
            cx,
            &dummy_filename(),
            Arc::new(FileSig::empty()),
            &metadata,
            comments,
            ast,
            aloc_ast,
            &type_sig_options(),
            None,
        )
        .expect("infer_ast should not be canceled in test")
    }

    fn file_sig_of_ast(ast: &ast::Program<Loc, Loc>) -> FileSig {
        FileSig::from_program(&dummy_filename(), ast, &FileSigOptions::default())
    }

    fn test_loc_of_aloc(aloc: &ALoc) -> Loc {
        aloc.to_loc_exn().clone()
    }
    fn test_get_ast(_: &FileKey) -> Option<ast::Program<Loc, Loc>> {
        None
    }
    fn test_get_haste(_: &FileKey) -> Option<flow_common_modulename::HasteModuleInfo> {
        None
    }
    fn test_get_type_sig(
        _: &FileKey,
    ) -> Option<
        flow_type_sig::packed_type_sig::Module<
            flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
        >,
    > {
        None
    }

    fn create_context(
        cx: &mut Context,
        source: &str,
        locs: Vec<Loc>,
    ) -> type_synthesizer::SynthesizerContext<'static> {
        let ast = parse(source);
        let typed_ast = typed_ast_of_ast(cx, &ast);
        let file_sig = file_sig_of_ast(&ast);
        let locs_set: BTreeSet<Loc> = locs.into_iter().collect();
        type_synthesizer::create_synthesizer_context(
            dummy_filename(),
            Arc::new(file_sig),
            typed_ast,
            &test_loc_of_aloc,
            &test_get_ast,
            &test_get_haste,
            &test_get_type_sig,
            &locs_set,
        )
    }

    fn pretty_print_type(
        cx: &mut Context,
        type_param_synthesizer: &dyn Fn(
            &mut Context<'_>,
            &flow_typing_type::type_::TypeParam,
        ) -> Result<
            ast::types::TypeParam<Loc, Loc>,
            insert_type::Expected,
        >,
        result: Result<
            Option<(
                Vec<flow_typing_type::type_::TypeParam>,
                ast::types::Type<Loc, Loc>,
            )>,
            insert_type::Expected,
        >,
    ) -> String {
        match result {
            Ok(Some((tparams_rev, type_))) => {
                let opts = js_layout_generator::default_opts();
                let layout = js_layout_generator::type_(&opts, &type_);
                let type_string = pretty_print(&layout);
                let typeparams_string = match tparams_rev.as_slice() {
                    [] => String::new(),
                    _ => {
                        let pretty_print_typeparam =
                            |tparam: &flow_typing_type::type_::TypeParam| -> String {
                                match type_param_synthesizer(cx, tparam) {
                                    Ok(synthesized) => {
                                        // type_param is pub(crate) in js_layout_generator,
                                        // so we manually format: name [: bound] [= default]
                                        let opts = js_layout_generator::default_opts();
                                        let mut parts = vec![synthesized.name.name.to_string()];
                                        match &synthesized.bound {
                                            ast::types::AnnotationOrHint::Available(annot) => {
                                                let layout = js_layout_generator::type_(
                                                    &opts,
                                                    &annot.annotation,
                                                );
                                                parts.push(format!(": {}", pretty_print(&layout)));
                                            }
                                            _ => {}
                                        }
                                        if let Some(default) = &synthesized.default {
                                            let layout = js_layout_generator::type_(&opts, default);
                                            parts.push(format!(" = {}", pretty_print(&layout)));
                                        }
                                        parts.join("")
                                    }
                                    Err(expected) => {
                                        format!(
                                            "<Error: {}>",
                                            insert_type::error_to_string(
                                                &insert_type::Errors::Expected(expected)
                                            )
                                        )
                                    }
                                }
                            };
                        format!(
                            "<{}>: ",
                            tparams_rev
                                .iter()
                                .map(pretty_print_typeparam)
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    }
                };
                format!("{}{}", typeparams_string, type_string)
            }
            Ok(None) => "<missing>".to_string(),
            Err(expected) => {
                format!(
                    "<Error: {}>",
                    insert_type::error_to_string(&insert_type::Errors::Expected(expected))
                )
            }
        }
    }

    #[test]
    fn basic_test() {
        let source = "
        const a = 3;
        const b = \"2\";
        class C<T> {}
        const c = new C<number>();
        function foo<A, B, C:B, D:B=C>(d: D): D {
          return d;
        }
        ";
        let loc_of_a = mk_loc(2, 14, 2, 15);
        let loc_of_b = mk_loc(3, 14, 3, 15);
        let loc_of_c = mk_loc(5, 14, 5, 15);
        let loc_of_d = mk_loc(6, 39, 6, 40);
        let loc_of_missing = mk_loc(100, 14, 100, 15);
        let mut arena = ContextArena::new();
        let mut cx = dummy_context(&mut arena);
        let context = create_context(
            &mut cx,
            source,
            vec![
                loc_of_a.clone(),
                loc_of_b.clone(),
                loc_of_c.clone(),
                loc_of_d.clone(),
                loc_of_missing.clone(),
            ],
        );
        let type_synth = type_synthesizer::create_type_synthesizer_with_import_adder(&cx, context);
        let synthesized_a = (type_synth.type_synthesizer)(&mut cx, &loc_of_a);
        assert_eq!(
            "number",
            pretty_print_type(&mut cx, &*type_synth.type_param_synthesizer, synthesized_a,),
        );
        let synthesized_b = (type_synth.type_synthesizer)(&mut cx, &loc_of_b);
        assert_eq!(
            "string",
            pretty_print_type(&mut cx, &*type_synth.type_param_synthesizer, synthesized_b,),
        );
        let synthesized_c = (type_synth.type_synthesizer)(&mut cx, &loc_of_c);
        assert_eq!(
            "C<number>",
            pretty_print_type(&mut cx, &*type_synth.type_param_synthesizer, synthesized_c,),
        );
        let synthesized_d = (type_synth.type_synthesizer)(&mut cx, &loc_of_d);
        assert_eq!(
            "<D: B = C, C: B, B>: D",
            pretty_print_type(&mut cx, &*type_synth.type_param_synthesizer, synthesized_d,),
        );
        let synthesized_missing = (type_synth.type_synthesizer)(&mut cx, &loc_of_missing);
        assert_eq!(
            "<missing>",
            pretty_print_type(
                &mut cx,
                &*type_synth.type_param_synthesizer,
                synthesized_missing,
            ),
        );
    }
}
