/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocTable;
use flow_aloc::LazyALocTable;
use flow_common::options::Options;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_lint_settings::lint_settings::LintSettings;
use flow_lint_settings::severity::Severity;
use flow_parser::ast;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::FileKeyInner;
use flow_parser::loc::Loc;
use flow_parser::loc::Position;
use flow_parser::loc_sig::LocSig;
use flow_parser_utils::file_sig::FileSig;
use flow_parser_utils::file_sig::FileSigOptions;
use flow_parser_utils_output::js_layout_generator;
use flow_parser_utils_output::pretty_printer;
use flow_type_sig::type_sig_options::TypeSigOptions;
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

use crate::refactor_extract;

fn pretty_print(layout: &flow_parser_utils_output::layout::LayoutNode) -> String {
    let source = pretty_printer::print(true, layout);
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
    let (ast, _errors) = flow_parser::parse_program_file::<()>(
        false,
        None,
        Some(flow_parser::PERMISSIVE_PARSE_OPTIONS),
        dummy_filename(),
        Ok(contents),
    );
    ast
}

fn file_sig_of_ast(ast: &ast::Program<Loc, Loc>) -> FileSig {
    FileSig::from_program(&dummy_filename(), ast, &FileSigOptions::default())
}

fn mk_loc(start: (i32, i32), end_: (i32, i32)) -> Loc {
    Loc {
        source: Some(dummy_filename()),
        start: Position {
            line: start.0,
            column: start.1,
        },
        end: Position {
            line: end_.0,
            column: end_.1,
        },
    }
}

fn dummy_context<'cx>(arena: &'cx mut ContextArena) -> Context<'cx> {
    let reason = flow_common::reason::mk_reason(None, ALoc::none());
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
        let t = Type::new(TypeInner::AnyT(
            r.dupe(),
            AnySource::AnyError(Some(AnyErrorKind::UnresolvedName)),
        ));
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
        let t = Type::new(TypeInner::AnyT(
            r.dupe(),
            AnySource::AnyError(Some(AnyErrorKind::UnresolvedName)),
        ));
        let val = Rc::new(flow_lazy::Lazy::new(move |_cx: &mut Context<'_>| {
            (ALoc::none(), t)
        }));
        builtin_types.insert(FlowSmolStr::from(*name), val);
    }
    let frozen = flow_typing_context::FrozenMetadata {
        component_syntax: false,
        ..Default::default()
    };
    let metadata = Metadata {
        frozen: Rc::new(frozen),
        overridable: flow_typing_context::OverridableMetadata {
            checked: true,
            ..Default::default()
        },
    };
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

fn remove_blank_lines(s: &str) -> String {
    s.split('\n')
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_refactored(
    support_experimental_snippet_text_edit: bool,
    expected: &[(&str, &str)],
    source: &str,
    extract_range: Loc,
) {
    let ast = parse(source);
    let tokens = crate::ast_extraction_utils::ast_extractor::tokens(
        Some(flow_parser::PERMISSIVE_PARSE_OPTIONS),
        Some(dummy_filename()),
        source,
    );
    let mut arena = ContextArena::new();
    let mut cx = dummy_context(&mut arena);
    let typed_ast = typed_ast_of_ast(&mut cx, &ast);
    let file_sig = file_sig_of_ast(&ast);
    let actual: Vec<(String, String)> = refactor_extract::provide_available_refactors(
        &tokens,
        &ast,
        &mut cx,
        &dummy_filename(),
        Arc::new(file_sig),
        &typed_ast,
        // loc_of_aloc: in test context, ALoc is identity-mapped from Loc
        &|aloc: &ALoc| aloc.to_loc_exn().clone(),
        // get_ast_from_heap: no shared mem in tests
        &|_: &FileKey| None,
        // get_haste_module_info: no haste in tests
        &|_: &FileKey| None,
        // get_type_sig: no type sig in tests
        &|_: &FileKey| None,
        support_experimental_snippet_text_edit,
        &extract_range,
    )
    .into_iter()
    .map(|refactor| {
        let title = refactor.title;
        let layout = js_layout_generator::program(
            &js_layout_generator::default_opts(),
            true,
            None,
            &refactor.new_ast,
        );
        let printed = pretty_print(&layout);
        let trimmed = remove_blank_lines(printed.trim());
        (title, trimmed)
    })
    .collect();
    let expected: Vec<(String, String)> = expected
        .iter()
        .map(|(title, s)| (title.to_string(), remove_blank_lines(s.trim())))
        .collect();
    let printer = |refactors: &[(String, String)]| -> String {
        refactors
            .iter()
            .map(|(title, program_string)| format!("// {}\n\n{}\n", title, program_string))
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
}

#[cfg(test)]
mod provide_available_refactor_tests {
    use super::*;

    #[test]
    fn single_line_extract() {
        let source = r#"
        function test() {
          console.log("I should not be selected");
          const a = 3;
          let b = 4;
          console.log("I should not be selected");
        }
      "#;
        let expected = vec![
            (
                "Extract to function in module scope",
                r#"
function test() {
  console.log("I should not be selected");
  newFunction();
  let b = 4;
  console.log("I should not be selected");
}
function newFunction(): void {
  const a = 3;
}
      "#,
            ),
            (
                "Extract to inner function in function 'test'",
                r#"
function test() {
  console.log("I should not be selected");
  newFunction();
  let b = 4;
  console.log("I should not be selected");
  function newFunction(): void {
    const a = 3;
  }
}
"#,
            ),
        ];
        assert_refactored(false, &expected, source, mk_loc((4, 10), (4, 22)));
    }

    #[test]
    fn multi_line_extract() {
        let source = r#"
        function test() {
          console.log("I should not be selected");
          const a = 3;
          let b = 4;
          let c = 5;
          let d = 6;
          console.log("I should not be selected");
        }
      "#;
        let expected = vec![
            (
                "Extract to function in module scope",
                r#"
function test() {
  console.log("I should not be selected");
  newFunction();
  console.log("I should not be selected");
}
function newFunction(): void {
  const a = 3;
  let b = 4;
  let c = 5;
  let d = 6;
}
      "#,
            ),
            (
                "Extract to inner function in function 'test'",
                r#"
function test() {
  console.log("I should not be selected");
  newFunction();
  console.log("I should not be selected");
  function newFunction(): void {
    const a = 3;
    let b = 4;
    let c = 5;
    let d = 6;
  }
}
"#,
            ),
        ];
        assert_refactored(false, &expected, source, mk_loc((4, 10), (7, 20)));
    }

    #[test]
    fn extract_use_toplevel_function_param() {
        let source = r#"
        function foo(a: number) {
          console.log(a);
        }
        "#;
        let expected = vec![
            (
                "Extract to function in module scope",
                r#"
function foo(a: number) {
  newFunction(a);
}
function newFunction(a: number): void {
  console.log(a);
}
            "#,
            ),
            (
                "Extract to inner function in function 'foo'",
                r#"
function foo(a: number) {
  newFunction();
  function newFunction(): void {
    console.log(a);
  }
}
            "#,
            ),
        ];
        assert_refactored(false, &expected, source, mk_loc((3, 0), (3, 30)));
    }

    #[test]
    fn single_escaping_def_extract() {
        let source = r#"
        const a = 3;
        console.log(a);
      "#;
        let expected = vec![(
            "Extract to function in module scope",
            r#"
const a = newFunction();
console.log(a);
function newFunction(): number {
  const a = 3;
  return a;
}
      "#,
        )];
        assert_refactored(false, &expected, source, mk_loc((2, 8), (2, 20)));
    }

    #[test]
    fn local_reassignment_single_return_extract() {
        let source = r#"
        let fooo = 3;
        const a = 3;
        fooo = a + 2; // selected
        console.log(a + fooo);
      "#;
        let expected = vec![(
            "Extract to function in module scope",
            r#"
let fooo = 3;
const a = 3;
fooo = newFunction();
console.log(a + fooo);
function newFunction(): number {
  fooo = a + 2; // selected
  return fooo;
}

      "#,
        )];
        assert_refactored(false, &expected, source, mk_loc((4, 8), (4, 33)));
    }

    #[test]
    fn local_reassignment_mixed_return_extract() {
        let source = r#"
        let fooo = 3;
        const a = 3; // selected
        fooo = a + 2; // selected
        console.log(a + fooo);
      "#;
        let expected = vec![(
            "Extract to function in module scope",
            r#"
let fooo = 3;
let a;

({a, fooo} = newFunction());
console.log(a + fooo);
function newFunction(): { a: number, fooo: number } {
  const a = 3; // selected
  fooo = a + 2; // selected
  return { a, fooo };
}
      "#,
        )];
        assert_refactored(false, &expected, source, mk_loc((3, 8), (4, 33)));
    }

    #[test]
    fn external_reassignment_single_return_extract() {
        let source = r#"
        let fooo = 3; // selected
        const a = 3;
        fooo = a + 2;
      "#;
        let expected = vec![(
            "Extract to function in module scope",
            r#"
let fooo = newFunction();
const a = 3;
fooo = a + 2;
function newFunction(): number {
  let fooo = 3; // selected
  return fooo;
}
      "#,
        )];
        assert_refactored(false, &expected, source, mk_loc((2, 8), (2, 33)));
    }

    #[test]
    fn external_reassignment_multiple_returns_extract() {
        let source = r#"
        let fooo = 3; // selected
        const a = 3; // selected
        fooo = a + 2;
      "#;
        let expected = vec![(
            "Extract to function in module scope",
            r#"
let {a, fooo} = newFunction();
fooo = a + 2;
function newFunction(): { a: number, fooo: number } {
  let fooo = 3; // selected
  const a = 3; // selected
  return { a, fooo };
}
      "#,
        )];
        assert_refactored(false, &expected, source, mk_loc((2, 8), (3, 32)));
    }

    #[test]
    fn async_expression_extract() {
        let source = r#"
        const test = async () => {
          // selection start
          const a = 3;
          let b = 4;
          await b;
          let d = 6;
        };
      "#;
        let expected = vec![
            (
                "Extract to function in module scope",
                r#"
const test = async () => {
  await newFunction();
};
async function newFunction(): Promise<void> {
  // selection start
  const a = 3;
  let b = 4;
  await b;
  let d = 6;
}
            "#,
            ),
            (
                "Extract to inner function in function 'test'",
                r#"
const test = async () => {
  await newFunction();
  async function newFunction(): Promise<void> {
    // selection start
    const a = 3;
    let b = 4;
    await b;
    let d = 6;
  }
};
            "#,
            ),
        ];
        assert_refactored(false, &expected, source, mk_loc((4, 10), (7, 20)));
    }

    #[test]
    fn async_for_of_extract() {
        let source = r#"
        const test = async () => {
          // selection start
          const a = 3;
          { for await (const b of []) {} }
          let d = 6;
        };
      "#;
        let expected = vec![
            (
                "Extract to function in module scope",
                r#"
const test = async () => {
  await newFunction();
};
async function newFunction(): Promise<void> {
  // selection start
  const a = 3;
  {
    for await (const b of []) {}
  }
  let d = 6;
}
            "#,
            ),
            (
                "Extract to inner function in function 'test'",
                r#"
const test = async () => {
  await newFunction();
  async function newFunction(): Promise<void> {
    // selection start
    const a = 3;
    {
      for await (const b of []) {}
    }
    let d = 6;
  }
};
            "#,
            ),
        ];
        assert_refactored(false, &expected, source, mk_loc((4, 10), (6, 20)));
    }

    #[test]
    fn await_in_async_function_extract() {
        let source = "const test = async () => await promise;";
        let expected = vec![(
            "Extract to function in module scope",
            r#"
newFunction();
function newFunction(): void {
  const test = async () => await promise;
}
"#,
        )];
        assert_refactored(false, &expected, source, mk_loc((1, 0), (1, 39)));
    }

    #[test]
    fn return_no_extract() {
        let source = r#"
        function test() {
          // selection start
          const a = 1;
          const b = 2;
          {
            return;
          }
          // selection end
        }
      "#;
        assert_refactored(false, &[], source, mk_loc((4, 10), (8, 22)));
    }

    #[test]
    fn yield_no_extract() {
        let source = r#"
        function* test() {
          // selection start
          const a = 1;
          {
            yield;
          }
          const b = 2;
          // selection end
        }
      "#;
        assert_refactored(false, &[], source, mk_loc((4, 10), (8, 22)));
    }

    #[test]
    fn label_no_extract() {
        assert_refactored(
            false,
            &[],
            "const a = 1; {label:test();} function test() {}",
            mk_loc((1, 0), (1, 60)),
        );
    }

    #[test]
    fn simple_break_continue_no_extract() {
        assert_refactored(
            false,
            &[],
            "while (true) {break;}",
            mk_loc((1, 12), (1, 30)),
        );
        assert_refactored(
            false,
            &[],
            "while (true) {continue;}",
            mk_loc((1, 12), (1, 30)),
        );
    }

    #[test]
    fn continue_in_switch_no_extract() {
        let source = r#"
      while (true) {
        // selection start
        switch (true) {default:continue;}
        // seletion end
      }

      "#;
        assert_refactored(false, &[], source, mk_loc((3, 8), (5, 23)));
    }

    #[test]
    fn wrapped_break_continue_with_label_no_extracts() {
        assert_refactored(
            false,
            &[],
            "label:while (true) {break label;}",
            mk_loc((1, 0), (1, 50)),
        );
        assert_refactored(
            false,
            &[],
            "label:while (true) {continue label;}",
            mk_loc((1, 0), (1, 50)),
        );
    }

    #[test]
    fn wrapped_break_continue_switch_has_extracts() {
        let expected = vec![(
            "Extract to function in module scope",
            r#"
newFunction();
function newFunction(): void {
  while (true) {
    break;
  }
}
"#,
        )];
        assert_refactored(
            false,
            &expected,
            "while (true) {break;}",
            mk_loc((1, 0), (1, 30)),
        );
        let expected = vec![(
            "Extract to function in module scope",
            r#"
newFunction();
function newFunction(): void {
  while (true) {
    continue;
  }
}
"#,
        )];
        assert_refactored(
            false,
            &expected,
            "while (true) {continue;}",
            mk_loc((1, 0), (1, 30)),
        );
        let expected = vec![(
            "Extract to function in module scope",
            r#"
newFunction();
function newFunction(): void {
  switch (true) {
    default:
      break;
  }
}
"#,
        )];
        assert_refactored(
            false,
            &expected,
            "switch (true) {default:break;}",
            mk_loc((1, 0), (1, 40)),
        );
    }

    #[test]
    fn basic_class_method_extract() {
        // First case: this.test1()
        assert_refactored(
            false,
            &[(
                "Extract to method in class 'A'",
                r#"
class A {
  test1() {
    this.newMethod();
  }
  newMethod(): void {
    this.test1();
  }
}
        "#,
            )],
            "class A { test1() { this.test1(); } }",
            mk_loc((1, 20), (1, 33)),
        );
        // Second case: super.test1()
        assert_refactored(
            false,
            &[(
                "Extract to method in class 'A'",
                r#"
class A {
  test1() {
    this.newMethod();
  }
  newMethod(): void {
    super.test1();
  }
}
        "#,
            )],
            "class A { test1() { super.test1(); } }",
            mk_loc((1, 20), (1, 34)),
        );
        // Third case: console.log() — has more extract options
        assert_refactored(
            false,
            &[
                (
                    "Extract to method in class 'A'",
                    r#"
class A {
  test1() {
    this.newMethod();
  }
  newMethod(): void {
    console.log();
  }
}
        "#,
                ),
                (
                    "Extract to function in module scope",
                    r#"
class A {
  test1() {
    newFunction();
  }
}
function newFunction(): void {
  console.log();
}
          "#,
                ),
                (
                    "Extract to inner function in method 'test1'",
                    r#"
class A {
  test1() {
    newFunction();
    function newFunction(): void {
      console.log();
    }
  }
}
            "#,
                ),
            ],
            "class A { test1() { console.log(); } }",
            mk_loc((1, 20), (1, 34)),
        );
    }

    #[test]
    fn class_method_with_parameters_and_return_extract() {
        let source = r#"
        export default class {
          constructor() { this.v = 1; }
          test1() {
            class B {
              constructor(v1: number, v2: number) {
                console.log(v1, v2);
              }
              test2() {
                console.log('test');
              }
            }
            const a = 2;
            const b = new B(this.v, a); // selected
            b.test2();
          }
        }
      "#;
        let expected = vec![(
            "Extract to method in anonymous class declaration",
            r#"
export default class {
  constructor() {
    this.v = 1;
  }
  test1() {
    class B {
      constructor(v1: number, v2: number) {
        console.log(v1, v2);
      }
      test2() {
        console.log("test");
      }
    }
    const a = 2;
    const b = this.newMethod(B, a);
    b.test2();
  }
  newMethod(B: typeof B, a: number): B {
    const b = new B(this.v, a); // selected
    return b;
  }
}
      "#,
        )];
        assert_refactored(false, &expected, source, mk_loc((14, 12), (14, 51)));
    }

    // Test that all the constraints of generic type parameters are added,
    // and unused ones (A) are removed.
    #[test]
    fn type_parameters_extract() {
        let source = r#"
        function foo<A, B, C:B=B>(c: C) {
          function bar<D:C, E:D, F:D=E>(f: F, e: E, d: D): F {
            console.log(c); // selected
            const f_ = f; // selected
            const e_ = e; // selected
            const d_ = d; // selected
            return f_;
          }
        }
        "#;
        let expected = vec![
            (
                "Extract to function in module scope",
                r#"
function foo<A, B, C: B = B>(c: C) {
  function bar<D: C, E: D, F: D = E>(f: F, e: E, d: D): F {
    const f_ = newFunction(c, f, e, d);
    return f_;
  }
}
function newFunction<
  B,
  C extends B = B,
  D extends C,
  E extends D,
  F extends D = E,
>(c: C, f: F, e: E, d: D): F {
  console.log(c); // selected
  const f_ = f; // selected
  const e_ = e; // selected
  const d_ = d; // selected
  return f_;
}
"#,
            ),
            (
                "Extract to inner function in function 'bar'",
                r#"
function foo<A, B, C: B = B>(c: C) {
  function bar<D: C, E: D, F: D = E>(f: F, e: E, d: D): F {
    const f_ = newFunction();
    return f_;
    function newFunction(): F {
      console.log(c); // selected
      const f_ = f; // selected
      const e_ = e; // selected
      const d_ = d; // selected
      return f_;
    }
  }
}
"#,
            ),
            (
                "Extract to inner function in function 'foo'",
                r#"
function foo<A, B, C: B = B>(c: C) {
  function bar<D: C, E: D, F: D = E>(f: F, e: E, d: D): F {
    const f_ = newFunction(f, e, d);
    return f_;
  }
  function newFunction<D extends C, E extends D, F extends D = E>(
    f: F,
    e: E,
    d: D,
  ): F {
    console.log(c); // selected
    const f_ = f; // selected
    const e_ = e; // selected
    const d_ = d; // selected
    return f_;
  }
}
"#,
            ),
        ];
        assert_refactored(false, &expected, source, mk_loc((4, 0), (7, 50)));
    }

    // Run in a thread with a large stack - deeply nested AST traversal is recursive
    // (OCaml uses a resizable stack; Rust needs explicit configuration)
    #[test]
    fn very_nested_extract() {
        let handle = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                let source = r#"
        function level1() {
          function level2() {
            function level3() {
              function level4() {
                console.log("I should not be selected");
                const a = 3;
                let b = 4;
                let c = 5;
                let d = 6;
                console.log("I should not be selected");
              }
            }
          }
        }
      "#;
                let expected = vec![
                    (
                        "Extract to function in module scope",
                        r#"
function level1() {
  function level2() {
    function level3() {
      function level4() {
        console.log("I should not be selected");
        newFunction();
        console.log("I should not be selected");
      }
    }
  }
}
function newFunction(): void {
  const a = 3;
  let b = 4;
  let c = 5;
  let d = 6;
}
"#,
                    ),
                    (
                        "Extract to inner function in function 'level4'",
                        r#"
function level1() {
  function level2() {
    function level3() {
      function level4() {
        console.log("I should not be selected");
        newFunction();
        console.log("I should not be selected");
        function newFunction(): void {
          const a = 3;
          let b = 4;
          let c = 5;
          let d = 6;
        }
      }
    }
  }
}
"#,
                    ),
                    (
                        "Extract to inner function in function 'level3'",
                        r#"
function level1() {
  function level2() {
    function level3() {
      function level4() {
        console.log("I should not be selected");
        newFunction();
        console.log("I should not be selected");
      }
      function newFunction(): void {
        const a = 3;
        let b = 4;
        let c = 5;
        let d = 6;
      }
    }
  }
}
"#,
                    ),
                    (
                        "Extract to inner function in function 'level2'",
                        r#"
function level1() {
  function level2() {
    function level3() {
      function level4() {
        console.log("I should not be selected");
        newFunction();
        console.log("I should not be selected");
      }
    }
    function newFunction(): void {
      const a = 3;
      let b = 4;
      let c = 5;
      let d = 6;
    }
  }
}
"#,
                    ),
                    (
                        "Extract to inner function in function 'level1'",
                        r#"
function level1() {
  function level2() {
    function level3() {
      function level4() {
        console.log("I should not be selected");
        newFunction();
        console.log("I should not be selected");
      }
    }
  }
  function newFunction(): void {
    const a = 3;
    let b = 4;
    let c = 5;
    let d = 6;
  }
}
"#,
                    ),
                ];
                assert_refactored(false, &expected, source, mk_loc((7, 16), (10, 26)));
            })
            .expect("failed to spawn thread");
        handle.join().expect("test thread panicked");
    }

    // Run in a thread with a large stack - deeply nested AST traversal is recursive
    // (OCaml uses a resizable stack; Rust needs explicit configuration)
    #[test]
    fn very_nested_extract_with_variables() {
        let handle = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                let source = r#"
        const a = 3;
        function level1() {
          let b = 4;
          function level2() {
            let c = 5;
            function level3() {
              let d = 6;
              {
                let e = 7;
                function level4() {
                  let f = 8;
                  const g = 3;
                  const h = 4;
                  console.log(a + b + c + d + e + f + g + h);
                  return f + g + h;
                }
              }
            }
          }
        }
      "#;
                let expected = vec![
                    (
                        "Extract to function in module scope",
                        r#"
const a = 3;
function level1() {
  let b = 4;
  function level2() {
    let c = 5;
    function level3() {
      let d = 6;
      {
        let e = 7;
        function level4() {
          let f = 8;
          const {g, h} = newFunction(b, c, d, e, f);
          return f + g + h;
        }
      }
    }
  }
}
function newFunction(
  b: number,
  c: number,
  d: number,
  e: number,
  f: number,
): { g: number, h: number } {
  const g = 3;
  const h = 4;
  console.log(a + b + c + d + e + f + g + h);
  return { g, h };
}
"#,
                    ),
                    (
                        "Extract to inner function in function 'level4'",
                        r#"
const a = 3;
function level1() {
  let b = 4;
  function level2() {
    let c = 5;
    function level3() {
      let d = 6;
      {
        let e = 7;
        function level4() {
          let f = 8;
          const {g, h} = newFunction();
          return f + g + h;
          function newFunction(): { g: number, h: number } {
            const g = 3;
            const h = 4;
            console.log(a + b + c + d + e + f + g + h);
            return { g, h };
          }
        }
      }
    }
  }
}
"#,
                    ),
                    (
                        "Extract to inner function in function 'level3'",
                        r#"
const a = 3;
function level1() {
  let b = 4;
  function level2() {
    let c = 5;
    function level3() {
      let d = 6;
      {
        let e = 7;
        function level4() {
          let f = 8;
          const {g, h} = newFunction(e, f);
          return f + g + h;
        }
      }
      function newFunction(e: number, f: number): { g: number, h: number } {
        const g = 3;
        const h = 4;
        console.log(a + b + c + d + e + f + g + h);
        return { g, h };
      }
    }
  }
}
"#,
                    ),
                    (
                        "Extract to inner function in function 'level2'",
                        r#"
const a = 3;
function level1() {
  let b = 4;
  function level2() {
    let c = 5;
    function level3() {
      let d = 6;
      {
        let e = 7;
        function level4() {
          let f = 8;
          const {g, h} = newFunction(d, e, f);
          return f + g + h;
        }
      }
    }
    function newFunction(
      d: number,
      e: number,
      f: number,
    ): { g: number, h: number } {
      const g = 3;
      const h = 4;
      console.log(a + b + c + d + e + f + g + h);
      return { g, h };
    }
  }
}
"#,
                    ),
                    (
                        "Extract to inner function in function 'level1'",
                        r#"
const a = 3;
function level1() {
  let b = 4;
  function level2() {
    let c = 5;
    function level3() {
      let d = 6;
      {
        let e = 7;
        function level4() {
          let f = 8;
          const {g, h} = newFunction(c, d, e, f);
          return f + g + h;
        }
      }
    }
  }
  function newFunction(
    c: number,
    d: number,
    e: number,
    f: number,
  ): { g: number, h: number } {
    const g = 3;
    const h = 4;
    console.log(a + b + c + d + e + f + g + h);
    return { g, h };
  }
}
"#,
                    ),
                ];
                assert_refactored(false, &expected, source, mk_loc((13, 18), (15, 61)));
            })
            .expect("failed to spawn thread");
        handle.join().expect("test thread panicked");
    }

    // A simple constant extraction that can go to all possible scopes.
    #[test]
    fn simple_constant_extract() {
        let source = "function test() { const a = 1; }";
        let expected = vec![
            (
                "Extract to constant in function 'test'",
                r#"
function test() {
  const newLocal = 1;
  const a = newLocal;
}
            "#,
            ),
            (
                "Extract to constant in module scope",
                r#"
const newLocal = 1;
function test() {
  const a = newLocal;
}
            "#,
            ),
        ];
        assert_refactored(false, &expected, source, mk_loc((1, 28), (1, 29)));
    }

    #[test]
    fn jsx_extract_toplevel() {
        let source = "import React from 'react';\nfunction test() {\n  return <div />;\n}";
        let expected = vec![
            (
                "Extract to react component",
                r#"
import React from "react";
function test() {
  return <NewComponent />;
}
function NewComponent() {
  return <div />;
}
            "#,
            ),
            (
                "Extract to constant in function 'test'",
                r#"
import React from "react";
function test() {
  const newLocal = <div />;
  return newLocal;
}
            "#,
            ),
            (
                "Extract to constant in module scope",
                r#"
import React from "react";
const newLocal = <div />;
function test() {
  return newLocal;
}
            "#,
            ),
        ];
        assert_refactored(false, &expected, source, mk_loc((3, 9), (3, 16)));
    }

    #[test]
    fn jsx_extract_with_generic_props() {
        let source = "import React from 'react';\nfunction test<T>(world: T): any {\n  const name = \"Flow\";\n  return <div>Hello {world}, I am {name}.</div>;\n}";
        let expected = vec![
            (
                "Extract to react component",
                r#"
import React from "react";
function test<T>(world: T): any {
  const name = "Flow";
  return <NewComponent world={world} name={name} />;
}
function NewComponent<T>({world, name}: Readonly<{ world: T, name: string }>) {
  return <div>Hello {world}, I am {name}.</div>;
}
            "#,
            ),
            (
                "Extract to constant in function 'test'",
                r#"
import React from "react";
function test<T>(world: T): any {
  const name = "Flow";
  const newLocal = <div>Hello {world}, I am {name}.</div>;
  return newLocal;
}
            "#,
            ),
        ];
        assert_refactored(false, &expected, source, mk_loc((4, 9), (4, 47)));
    }

    #[test]
    fn jsx_extract_nested_element() {
        let source = "import React from 'react';\nfunction test() {\n  return (\n    <div>\n      <div>hi</div>\n      <div>hi</div>\n      <div>hi</div>\n    </div>\n  );\n}";
        let expected = vec![
            (
                "Extract to react component",
                r#"
import React from "react";
function test() {
  return (
    <div>
      <NewComponent />
      <div>hi</div>
      <div>hi</div>
    </div>
  );
}
function NewComponent() {
  return <div>hi</div>;
}
            "#,
            ),
            (
                "Extract to constant in function 'test'",
                r#"
import React from "react";
function test() {
  const newLocal = <div>hi</div>;
  return (
    <div>
      {newLocal}
      <div>hi</div>
      <div>hi</div>
    </div>
  );
}
            "#,
            ),
            (
                "Extract to constant in module scope",
                r#"
import React from "react";
const newLocal = <div>hi</div>;
function test() {
  return (
    <div>
      {newLocal}
      <div>hi</div>
      <div>hi</div>
    </div>
  );
}
            "#,
            ),
        ];
        assert_refactored(false, &expected, source, mk_loc((5, 6), (5, 19)));
    }

    #[test]
    fn jsx_extract_nested_fragment() {
        let source = "import React from 'react';\nfunction test() {\n  return (\n    <div>\n      <>\n        <div>hi</div>\n        <div>hi</div>\n      </>\n    </div>\n  );\n}";
        let expected = vec![
            (
                "Extract to react component",
                r#"
import React from "react";
function test() {
  return (
    <div>
      <NewComponent />
    </div>
  );
}
function NewComponent() {
  return <>
    <div>hi</div>
    <div>hi</div>
  </>;
}
            "#,
            ),
            (
                "Extract to constant in function 'test'",
                r#"
import React from "react";
function test() {
  const newLocal = <>
    <div>hi</div>
    <div>hi</div>
  </>;
  return (
    <div>
      {newLocal}
    </div>
  );
}
            "#,
            ),
            (
                "Extract to constant in module scope",
                r#"
import React from "react";
const newLocal = <>
  <div>hi</div>
  <div>hi</div>
</>;
function test() {
  return (
    <div>
      {newLocal}
    </div>
  );
}
            "#,
            ),
        ];
        assert_refactored(false, &expected, source, mk_loc((5, 6), (8, 9)));
    }

    // Testing that we won't extract constant to scopes that will result in undefined variables.
    #[test]
    fn constant_extract_with_scoping_issues() {
        let source = r#"
        function foo() {
          const a = 3;
          function bar() {
            const b = 4;
            function baz() {
              const c = 5;
              // selected a + b + c
              return a + b + c;
            }
          }
        }
        "#;
        let expected = vec![(
            "Extract to constant in function 'baz'",
            r#"
function foo() {
  const a = 3;
  function bar() {
    const b = 4;
    function baz() {
      const c = 5;
      const newLocal = a + b + c;
      // selected a + b + c
      return newLocal;
    }
  }
}
          "#,
        )];
        assert_refactored(false, &expected, source, mk_loc((9, 21), (9, 30)));
    }

    // A simple field extraction that can only go inside the class.
    #[test]
    fn field_extract_class_only() {
        let source = "class A { test() { const a = this.test(); } }";
        let expected = vec![
            (
                "Extract to field in class 'A'",
                r#"
class A {
  newProperty = this.test();
  test() {
    const a = this.newProperty;
  }
}
            "#,
            ),
            (
                "Extract to constant in method 'test'",
                r#"
class A {
  test() {
    const newLocal = this.test();
    const a = newLocal;
  }
}
            "#,
            ),
        ];
        assert_refactored(false, &expected, source, mk_loc((1, 29), (1, 40)));
    }

    // A simple type alias extraction without any type variables.
    #[test]
    fn simple_type_extract() {
        let source = "const a: number = 1;";
        let expected = vec![(
            "Extract to type alias",
            "type NewType = number;\nconst a: NewType = 1;",
        )];
        assert_refactored(false, &expected, source, mk_loc((1, 9), (1, 15)));
    }

    // Selected type uses generic type parameters
    #[test]
    fn type_extract_with_generic_type_parameters() {
        let source = r#"
        function foo<A, B>() {
          class Bar<C, D> {
            baz<E, F>(): [A, B, C, D, E, F] { }
          }
        }
        "#;
        let expected = vec![(
            "Extract to type alias",
            r#"
function foo<A, B>() {
  type NewType<C, D, E, F> = [A, B, C, D, E, F];
  class Bar<C, D> {
    baz<E, F>(): NewType<C, D, E, F> {}
  }
}
            "#,
        )];
        assert_refactored(false, &expected, source, mk_loc((4, 25), (4, 43)));
    }

    // Generated name has conflicts.
    #[test]
    fn generated_name_conflicts() {
        let source = r#"
        const newFunction = 1, newFunction1 = 1;
        const newMethod = 1, newMethod1 = 1;
        const newLocal = 1, newLocal1 = 1;
        const newProperty = 1, newProperty1 = 1;
        class A {
          test() {
            1 + 1
          }
        }
        "#;
        let expected = vec![
            (
                "Extract to method in class 'A'",
                r#"
const newFunction = 1,
  newFunction1 = 1;
const newMethod = 1,
  newMethod1 = 1;
const newLocal = 1,
  newLocal1 = 1;
const newProperty = 1,
  newProperty1 = 1;
class A {
  test() {
    this.${0:newMethod2}();
  }
  ${0:newMethod2}(): void {
    1 + 1;
  }
}
            "#,
            ),
            (
                "Extract to function in module scope",
                r#"
const newFunction = 1,
  newFunction1 = 1;
const newMethod = 1,
  newMethod1 = 1;
const newLocal = 1,
  newLocal1 = 1;
const newProperty = 1,
  newProperty1 = 1;
class A {
  test() {
    ${0:newFunction2}();
  }
}
function ${0:newFunction2}(): void {
  1 + 1;
}
            "#,
            ),
            (
                "Extract to inner function in method 'test'",
                r#"
const newFunction = 1,
  newFunction1 = 1;
const newMethod = 1,
  newMethod1 = 1;
const newLocal = 1,
  newLocal1 = 1;
const newProperty = 1,
  newProperty1 = 1;
class A {
  test() {
    ${0:newFunction2}();
    function ${0:newFunction2}(): void {
      1 + 1;
    }
  }
}
            "#,
            ),
            (
                "Extract to field in class 'A'",
                r#"
const newFunction = 1,
  newFunction1 = 1;
const newMethod = 1,
  newMethod1 = 1;
const newLocal = 1,
  newLocal1 = 1;
const newProperty = 1,
  newProperty1 = 1;
class A {
  ${0:newProperty2} = 1 + 1;
  test() {
    this.${0:newProperty2};
  }
}
            "#,
            ),
            (
                "Extract to constant in method 'test'",
                r#"
const newFunction = 1,
  newFunction1 = 1;
const newMethod = 1,
  newMethod1 = 1;
const newLocal = 1,
  newLocal1 = 1;
const newProperty = 1,
  newProperty1 = 1;
class A {
  test() {
    const ${0:newLocal2} = 1 + 1;
    ${0:newLocal2};
  }
}
            "#,
            ),
            (
                "Extract to constant in module scope",
                r#"
const newFunction = 1,
  newFunction1 = 1;
const newMethod = 1,
  newMethod1 = 1;
const newLocal = 1,
  newLocal1 = 1;
const newProperty = 1,
  newProperty1 = 1;
const ${0:newLocal2} = 1 + 1;
class A {
  test() {
    ${0:newLocal2};
  }
}
            "#,
            ),
        ];
        assert_refactored(true, &expected, source, mk_loc((8, 12), (8, 17)));
        // Also tests type alias name conflicts
        let source = r#"
        type NewType = number;
        type NewType1 = NewType;
      "#;
        let expected = vec![(
            "Extract to type alias",
            r#"
type ${0:NewType2} = number;
type NewType = ${0:NewType2};
type NewType1 = NewType;
            "#,
        )];
        assert_refactored(true, &expected, source, mk_loc((2, 23), (2, 29)));
    }
}
