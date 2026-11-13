/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Rust unit tests for the lowering logic ported from JavaScript.
//! These tests document existing behaviors (including unimplemented/error branches)
//! and are intended to anchor the Rust implementation before any fork/divergence.
//!
//! Most tests lower a fixture and compare the printed result against a golden string,
//! so the expected output in the test *is* the documentation of the transform. Printing
//! goes through `flow_parser_utils_output`, a test-only dependency; its formatting differs
//! from Babel's, so these goldens are not byte-identical to the snapshots in
//! `fbcode/flow/packages/flow-parser/__tests__/*-test.js`, only equivalent modulo layout.
//! Tests for source locations compare checked-in ESTree JSON instead, since printed JavaScript
//! omits them.
//!
//! Error paths and `BabelMetadata` have no printed form and are asserted directly.

use std::convert::Infallible;
use std::sync::Arc;

use dupe::Dupe;
use flow_parser::ast;
use flow_parser::ast::expression;
use flow_parser::ast::expression::ExpressionInner;
use flow_parser::ast::function;
use flow_parser::ast::statement;
use flow_parser::ast::statement::StatementInner;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::estree_translator;
use flow_parser::loc::Loc;
use flow_parser::offset_utils::OffsetTable;
use flow_parser_utils_output::js_layout_generator;
use flow_parser_utils_output::pretty_printer;

use super::BabelLoweringError;
use super::BabelLoweringOptions;
use super::COMPONENT_FUNCTION_FLAG;
use super::COMPONENT_PATTERN_OPTIONAL_FLAG;
use super::COMPONENT_REST_TYPE_ANNOTATION_FLAG;
use super::EnumRuntime;
use super::HOOK_FUNCTION_FLAG;
use super::ReactRuntimeTarget;
use super::component_lowering;
use super::enum_lowering;
use super::match_lowering;
use super::record_lowering;
use super::strip_flow;

type Program = ast::Program<Loc, Loc>;

fn parse(source: &str) -> Program {
    let options = flow_parser::ParseOptions {
        components: true,
        enums: true,
        pattern_matching: true,
        records: true,
        esproposal_decorators: true,
        types: true,
        ambiguous_types: true,
        enable_types_in_comments: false,
        use_strict: false,
        module_ref_prefix: None,
        ambient: false,
        allow_return_outside_function: false,
        oxc_type_feature_parser_rules: false,
    };
    let file_key = flow_parser::file_key::FileKey::new(
        flow_parser::file_key::FileKeyInner::SourceFile("lowering-test.js".to_owned()),
    );
    let (program, errors) =
        flow_parser::parse_program_file::<()>(false, None, Some(options), file_key, Ok(source));
    assert!(errors.is_empty(), "parse errors for {source:?}: {errors:?}");
    program
}

fn print(program: &Program) -> String {
    let opts = js_layout_generator::default_opts();
    let layout = js_layout_generator::program(&opts, false, None, program);
    pretty_printer::print(false, &layout).contents()
}

fn print_estree(source: &str, program: &Program) -> String {
    let offset_table = OffsetTable::make(source);
    let config = estree_translator::Config {
        include_locs: true,
        include_filename: true,
        offset_style: estree_translator::OffsetStyle::JsIndices,
    };
    format!(
        "{:#}\n",
        estree_translator::program(&offset_table, &config, program)
    )
}

fn test_resource_dir(resource: &str, cargo_path: &str) -> std::path::PathBuf {
    match buck_resources::get(resource) {
        Ok(resource_dir) => resource_dir.join(cargo_path),
        Err(_) => {
            let Some(cargo_manifest_dir) = option_env!("CARGO_MANIFEST_DIR") else {
                panic!("test resources should be available from Buck or Cargo");
            };
            std::path::Path::new(cargo_manifest_dir).join(cargo_path)
        }
    }
}

fn fixture_dir() -> std::path::PathBuf {
    test_resource_dir(
        "flow/rust_port/crates/flow_parser_wasm/test_fixtures",
        "tests/fixtures",
    )
}

fn snapshot_dir() -> std::path::PathBuf {
    test_resource_dir(
        "flow/rust_port/crates/flow_parser_wasm/test_snapshots",
        "tests/__snapshots__",
    )
}

fn read_test_data(path: &std::path::Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn assert_snapshot(name: &str, actual: &str) {
    let path = snapshot_dir().join(name);
    let expected = read_test_data(&path);
    assert_eq!(
        actual,
        expected.as_str(),
        "snapshot mismatch for {}",
        path.display()
    );
}

fn assert_lowering_error(
    source: &str,
    expected_message: &str,
    lower: impl FnOnce(&Program) -> Result<Program, BabelLoweringError>,
) {
    let error = lower(&parse(source)).expect_err("lowering should fail");
    match error {
        BabelLoweringError::Syntax { message, loc } => {
            assert_eq!(message, expected_message);
            assert_eq!(loc.start.line, 1, "error should point into the source line");
            assert!(loc.end >= loc.start, "error location should be well formed");
        }
    }
}

#[test]
fn enum_lowering_builds_each_runtime_shape() {
    let program = parse(
        "enum BooleanE {A = true, B = false}\n\
         enum StringValueE {A = 'alpha', B = 'beta'}\n\
         enum StringDefaultE {A, B}\n\
         enum SymbolE of symbol {A, B}\n\
         enum BigIntE of bigint {A = 1n, B = 2n}",
    );
    let lowered = enum_lowering::lower_program(&program, EnumRuntime::Default);

    assert_eq!(
        print(&lowered),
        r#"const BooleanE = require("flow-enums-runtime")({ A: true, B: false });
const StringValueE = require("flow-enums-runtime")({ A: "alpha", B: "beta" });
const StringDefaultE = require("flow-enums-runtime").Mirrored(["A", "B"]);
const SymbolE = require("flow-enums-runtime")(
  { A: Symbol("A"), B: Symbol("B") },
);
const BigIntE = require("flow-enums-runtime")({ A: 1n, B: 2n });
"#
    );
}

#[test]
fn enum_lowering_preserves_source_locations_in_estree() {
    let source = read_test_data(&fixture_dir().join("enum_lowering/source.js"));
    let lowered = enum_lowering::lower_program(&parse(&source), EnumRuntime::Default);
    assert_snapshot("enum_lowering/lowered.js", &print(&lowered));
    assert_snapshot(
        "enum_lowering/estree.json",
        &print_estree(&source, &lowered),
    );
}

#[test]
fn enum_lowering_preserves_exports_and_numbers_custom_runtimes() {
    // The CustomPlaceholder runtime emits a NUL-prefixed identifier per enum, which the
    // wasm caller substitutes later. A raw string cannot hold NUL, hence the escapes.
    let program = parse("export enum A {X = 1}\nexport default enum B {Y = 2}");
    let lowered = enum_lowering::lower_program(&program, EnumRuntime::CustomPlaceholder);

    assert_eq!(
        print(&lowered),
        "export const A = \u{0}flow_enum_runtime_0({ X: 1 });\n\
         const B = \u{0}flow_enum_runtime_1({ Y: 2 });\n\
         export default B;\n"
    );
}

#[test]
fn match_expression_uses_a_conditional_for_simple_patterns() {
    let source = "const result = match (value) { 1 => 'one', NaN => 'nan', _ => 'other' };";
    let lowered = match_lowering::lower_program(source, &parse(source))
        .expect("simple match expression should lower");

    assert_eq!(
        print(&lowered),
        r#"const result = value === 1 ? "one" : Number.isNaN(value) ? "nan" : "other";
"#
    );
}

#[test]
fn match_expression_uses_an_iife_for_calls_and_bindings() {
    let source =
        "const result = match (getValue()) { [const head, ...const tail] => head, _ => null };";
    let lowered = match_lowering::lower_program(source, &parse(source))
        .expect("match expression with bindings should lower");

    assert_eq!(
        print(&lowered),
        r#"const result = ($$gen$m0 => {
  if (Array.isArray($$gen$m0) && $$gen$m0.length >= 1) {
    const head = $$gen$m0[0];
    const tail = $$gen$m0.slice(1);
    return head;
  }
  return null;
})(getValue());
"#
    );
}

#[test]
fn match_statement_uses_a_generated_labeled_block() {
    let source = "match (value) { _ => {} }";
    let lowered = match_lowering::lower_program(source, &parse(source))
        .expect("match statement should lower");

    assert_eq!(
        print(&lowered),
        r#"$$gen$m0: {
  {
    break $$gen$m0;
  }
}
"#
    );
}

#[test]
fn non_exhaustive_match_keeps_the_existing_fallthrough_message() {
    // "succesfully" is a typo in match_lowering.rs; it is reproduced here to match today's
    // output. It is not a spelling to preserve — fix the source and this golden together.
    let source = "const result = match (getValue()) {};";
    let lowered = match_lowering::lower_program(source, &parse(source))
        .expect("empty match expression should lower");

    assert_eq!(
        print(&lowered),
        r#"const result = ($$gen$m0 => {
  throw (
    Error(
      "Match: No case succesfully matched. Make exhaustive or add a wildcard case using '_'. Argument: " + $$gen$m0,
    )
  );
})(getValue());
"#
    );
}

#[test]
fn match_lowering_preserves_source_locations_in_estree() {
    let source = read_test_data(&fixture_dir().join("match_lowering/source.js"));
    let lowered =
        match_lowering::lower_program(&source, &parse(&source)).expect("matches should lower");
    assert_snapshot("match_lowering/lowered.js", &print(&lowered));
    assert_snapshot(
        "match_lowering/estree.json",
        &print_estree(&source, &lowered),
    );
}

#[test]
fn match_lowering_documents_unsupported_and_invalid_patterns() {
    let cases = [
        (
            "const result = match (value) { -0 => 0 };",
            "'+0' and '-0' are not yet supported in match unary patterns.",
        ),
        (
            "const result = match (value) { [const a] | {const a} => 0 };",
            "Bindings in match 'or' patterns are not yet supported.",
        ),
        (
            "const result = match (value) { const x as y => 0 };",
            "Match 'as' patterns are not allowed directly on binding patterns.",
        ),
        (
            "const result = match (value) { var x => 0 };",
            "'var' bindings are not allowed. Use 'const' or 'let'.",
        ),
        (
            "const result = match (value) { [const x, const x] => 0 };",
            "Duplicate variable name 'x' in match case pattern.",
        ),
        (
            "const result = match (value) { {x: 0, const x} => 0 };",
            "Duplicate property name 'x' in match object pattern.",
        ),
    ];

    for (source, message) in cases {
        assert_lowering_error(source, message, |program| {
            match_lowering::lower_program(source, program)
        });
    }
}

#[test]
fn component_lowering_builds_props_and_react_node_return() {
    let program = parse("component Foo(value: string = '', alias as local?: number) {}");
    let (lowered, _) = component_lowering::lower_program(&program, ReactRuntimeTarget::React19)
        .expect("component should lower");

    assert_eq!(
        print(&lowered),
        r#"function Foo(
  {value = "", alias: local}: $ReadOnly<{ value?: string, alias?: number }>,
): React.Node {}
"#
    );
}

#[test]
fn component_ref_lowering_differs_between_react_18_and_19() {
    let program = parse("component Foo(value: string, ref: Ref) {}");

    let (react_18, _) = component_lowering::lower_program(&program, ReactRuntimeTarget::React18)
        .expect("React 18 component should lower");
    assert_eq!(
        print(&react_18),
        r#"const Foo = React.forwardRef(Foo_withRef);
function Foo_withRef(
  {value}: $ReadOnly<{ value: string }>,
  ref: Ref,
): React.Node {}
"#
    );

    let (react_19, _) = component_lowering::lower_program(&program, ReactRuntimeTarget::React19)
        .expect("React 19 component should lower");
    assert_eq!(
        print(&react_19),
        r#"function Foo(
  {value, ref}: $ReadOnly<{ value: string, ref: Ref }>,
): React.Node {}
"#
    );
}

#[test]
fn component_forward_ref_preserves_source_locations() {
    #[derive(Default)]
    struct Identifiers(Vec<ast::Identifier<Loc, Loc>>);

    impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, Infallible> for Identifiers {
        fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
            loc
        }

        fn normalize_type(loc: &'ast Loc) -> &'ast Loc {
            loc
        }

        fn identifier(&mut self, id: &'ast ast::Identifier<Loc, Loc>) -> Result<(), Infallible> {
            self.0.push(id.dupe());
            Ok(())
        }
    }

    for prefix in ["", "export ", "export default "] {
        let program = parse(&format!(
            "\n\n{prefix}component Foo(ref: Ref) {{\n  return null;\n}}"
        ));
        for target in [ReactRuntimeTarget::React18, ReactRuntimeTarget::React19] {
            let (lowered, _) = component_lowering::lower_program(&program, target)
                .expect("component should lower");
            let mut identifiers = Identifiers::default();
            let Ok(()) = identifiers.program(&lowered);
            for id in &identifiers.0 {
                assert_eq!(id.loc.source, program.loc.source, "location of {}", id.name);
                assert!(id.loc.start.line >= 3, "location of {}", id.name);
            }
            let forward_ref = identifiers.0.iter().find(|id| id.name == "forwardRef");
            match target {
                ReactRuntimeTarget::React18 => {
                    let loc = &forward_ref.expect("React 18 requires forwardRef").loc;
                    assert_eq!((loc.start.line, loc.start.column), (3, prefix.len() as i32));
                    assert_eq!((loc.end.line, loc.end.column), (5, 1));
                }
                ReactRuntimeTarget::React19 => assert!(forward_ref.is_none()),
            }
        }
    }
}

#[test]
fn component_lowering_handles_declarations_exports_and_metadata() {
    let source = "declare component Declared(value: string);\n\
                  export component Exported(prop as {value}: Props, ...{other, ...rest}: RestProps) {}\n\
                  hook useValue(): number { return 1; }";
    let program = parse(source);
    let (component_loc, destructured_pattern_loc, nested_rest_loc, hook_loc) = {
        let StatementInner::ExportNamedDeclaration { inner: export, .. } = &*program.statements[1]
        else {
            panic!("expected exported component");
        };
        let Some(declaration) = &export.declaration else {
            panic!("expected exported declaration");
        };
        let StatementInner::ComponentDeclaration {
            loc: component_loc,
            inner: component,
        } = &**declaration
        else {
            panic!("expected component declaration");
        };
        let destructured_pattern_loc = component.params.params[0].local.loc().clone();
        let rest = component
            .params
            .rest
            .as_ref()
            .expect("expected component rest");
        let ast::pattern::Pattern::Object { inner, .. } = &rest.argument else {
            panic!("expected destructured component rest");
        };
        let ast::pattern::object::Property::RestElement(rest) =
            inner.properties.last().expect("expected nested rest")
        else {
            panic!("expected nested rest element");
        };
        let StatementInner::FunctionDeclaration {
            loc: hook_loc,
            inner: hook,
        } = &*program.statements[2]
        else {
            panic!("expected hook declaration");
        };
        assert_eq!(hook.effect_, function::Effect::Hook);
        (
            component_loc.clone(),
            destructured_pattern_loc,
            rest.loc.clone(),
            hook_loc.clone(),
        )
    };
    let lowered = super::lower_program(
        source,
        &program,
        &BabelLoweringOptions {
            lower_enums: true,
            enum_runtime: EnumRuntime::Default,
            react_runtime_target: ReactRuntimeTarget::React19,
        },
    )
    .expect("component declarations, exports, and hooks should lower");

    assert_eq!(
        print(&lowered.program),
        r#"declare const Declared: any;
export function Exported(
  {prop: {value}, other, ...rest}: $ReadOnly<{ ...RestProps, prop: Props }>,
): React.Node {}
function useValue(): number {
  return 1;
}
"#
    );

    // Component and hook identity survives lowering only in BabelMetadata, keyed by the
    // original source locations captured above.
    assert_eq!(
        lowered.metadata.flags_for_loc(&component_loc),
        COMPONENT_FUNCTION_FLAG
    );
    assert_eq!(
        lowered.metadata.flags_for_loc(&hook_loc),
        HOOK_FUNCTION_FLAG
    );
    assert_eq!(
        lowered.metadata.flags_for_loc(&destructured_pattern_loc),
        COMPONENT_PATTERN_OPTIONAL_FLAG
    );
    assert_eq!(
        lowered.metadata.flags_for_loc(&nested_rest_loc),
        COMPONENT_REST_TYPE_ANNOTATION_FLAG
    );
}

#[test]
fn component_lowering_rejects_an_invalid_rest_pattern() {
    let source = "component Foo(...[value]: Props) {}";
    assert_lowering_error(
        source,
        "Invalid component rest parameter pattern.",
        |program| {
            component_lowering::lower_program(program, ReactRuntimeTarget::React19)
                .map(|(program, _)| program)
        },
    );
}

#[test]
fn record_declaration_becomes_a_class_with_constructor_and_members() {
    let source = "record R { default: number, 1: number = 1, value: string, method() {} static item: number = 2, static make() {} }";
    let lowered =
        record_lowering::lower_program(&parse(source)).expect("record declaration should lower");

    assert_eq!(
        print(&lowered),
        r#"class R {
  constructor({default: $$gen$r0, 1: $$gen$r1 = 1, value}) {
    this.default = $$gen$r0;
    this[1] = $$gen$r1;
    this.value = value;
  }
  method() {}
  static item = 2;
  static make() {}
}
"#
    );
}

#[test]
fn record_expressions_become_nested_new_expressions_with_objects_and_spreads() {
    let source = "const result = R {...base, child: S {value: 1}};";
    let lowered =
        record_lowering::lower_program(&parse(source)).expect("record expressions should lower");

    assert_eq!(
        print(&lowered),
        r#"const result = new R({ ...base, child: new S({ value: 1 }) });
"#
    );
}

#[test]
fn record_lowering_preserves_source_locations_in_estree() {
    let source = read_test_data(&fixture_dir().join("record_lowering/source.js"));
    let lowered = record_lowering::lower_program(&parse(&source)).expect("record should lower");
    assert_snapshot("record_lowering/lowered.js", &print(&lowered));
    assert_snapshot(
        "record_lowering/estree.json",
        &print_estree(&source, &lowered),
    );
}

#[test]
fn record_lowering_rejects_computed_property_keys() {
    // `record R { [key]: number }` is rejected by the parser, so the computed key has to be
    // grafted onto a parsed AST to reach the lowering's own check at all.
    let mut program = parse("record R { key: number }");
    let statements = Arc::make_mut(&mut program.statements);
    let StatementInner::RecordDeclaration { inner, .. } = Arc::make_mut(&mut statements[0].0)
    else {
        panic!("expected record declaration");
    };
    let declaration = Arc::make_mut(inner);
    let elements = Arc::make_mut(&mut declaration.body.body);
    let statement::record_declaration::BodyElement::Property(property) = &mut elements[0] else {
        panic!("expected record property");
    };
    let expression::object::Key::Identifier(identifier) = &property.key else {
        panic!("expected identifier key");
    };
    property.key = expression::object::Key::Computed(ast::ComputedKey {
        loc: property.loc.clone(),
        expression: expression::Expression::new(ExpressionInner::Identifier {
            loc: identifier.loc.clone(),
            inner: identifier.clone(),
        }),
        comments: None,
    });

    let error = record_lowering::lower_program(&program).expect_err("lowering should fail");
    match error {
        BabelLoweringError::Syntax { message, loc } => {
            assert_eq!(message, "Computed keys are not valid record property keys.");
            assert_eq!(loc.start.line, 1);
        }
    }
}

#[test]
fn strip_flow_rewrites_types_the_babel_ast_cannot_represent() {
    let program = parse(
        "type SymbolT = symbol;\n\
         type BigIntT = bigint;\n\
         type IndexedT = Obj[Key];\n\
         type OptionalIndexedT = Obj?.[Key];\n\
         type KeyofT = keyof Obj;\n\
         type ConditionalT<T> = T extends string ? number : boolean;\n\
         type ReadOnlyT = readonly Array<number>;\n\
         type MappedT<T> = {[key in keyof Obj]: T};\n\
         type ComponentT = component();\n\
         type RendersT = renders number;\n\
         type HookT = hook () => void;\n\
         type LabeledTupleT = [label: string];\n\
         type SpreadTupleT<S> = [...S];",
    );
    let lowered = strip_flow::lower_program(&program);

    // `symbol` and `bigint` keep their spelling but become plain generic type references.
    assert_eq!(
        print(&lowered),
        r#"type SymbolT = symbol;
type BigIntT = bigint;
type IndexedT = any;
type OptionalIndexedT = any;
type KeyofT = any;
type ConditionalT<T> = any;
type ReadOnlyT = any;
type MappedT<T> = any;
type ComponentT = any;
type RendersT = any;
type HookT = any;
type LabeledTupleT = [any];
type SpreadTupleT<S> = [any];
"#
    );
}

#[test]
fn strip_flow_removes_this_and_rewrites_type_guards_and_declarations() {
    let program = parse(
        "function predicate(this: Context, value: mixed): value is string { return typeof value === 'string'; }\n\
         declare enum E {}\n\
         declare namespace N {}",
    );
    let lowered = strip_flow::lower_program(&program);

    assert_eq!(
        print(&lowered),
        r#"function predicate(value: mixed): any {
  return typeof value === "string";
}
declare const E: any;
declare const N: any;
"#
    );
}

#[test]
fn strip_flow_removes_abstract_class_members() {
    let program = parse(
        "abstract class Base {\n\
         abstract property: string;\n\
         abstract method(value: number): string;\n\
         concrete(): void {}\n\
         }",
    );
    let lowered = strip_flow::lower_program(&program);

    assert_eq!(
        print(&lowered),
        r#"abstract class Base {
  concrete(): void {}
}
"#
    );
}

#[test]
fn babel_adapter_honors_enum_option_and_runs_the_complete_pipeline() {
    let source = "enum E {A}\nrecord R {value: number}\ncomponent Foo(value: R) { const record = R {value}; }";
    let program = parse(source);
    let lower = |lower_enums: bool| {
        super::lower_program(
            source,
            &program,
            &BabelLoweringOptions {
                lower_enums,
                enum_runtime: EnumRuntime::Default,
                react_runtime_target: ReactRuntimeTarget::React19,
            },
        )
        .expect("pipeline should run")
        .program
    };

    // lower_enums gates only the enum stage; record and component still lower.
    assert_eq!(
        print(&lower(false)),
        r#"enum E {
  A,
}
class R {
  constructor({value}) {
    this.value = value;
  }
}
function Foo({value}: $ReadOnly<{ value: R }>): React.Node {
  const record = new R({ value });
}
"#
    );

    assert_eq!(
        print(&lower(true)),
        r#"const E = require("flow-enums-runtime").Mirrored(["A"]);
class R {
  constructor({value}) {
    this.value = value;
  }
}
function Foo({value}: $ReadOnly<{ value: R }>): React.Node {
  const record = new R({ value });
}
"#
    );
}

#[test]
fn runnable_erasure_runs_after_existing_runtime_lowerings() {
    let source = "enum E {A}\nrecord R {value: number}\ncomponent Foo(value: R) { return match (value) { const x => R {value: x} }; }";
    assert_eq!(
        print(&crate::strip_flow::lower_program(
            &super::lower_program(
                source,
                &parse(source),
                &BabelLoweringOptions {
                    lower_enums: true,
                    enum_runtime: EnumRuntime::Default,
                    react_runtime_target: ReactRuntimeTarget::React19,
                },
            )
            .expect("runtime transforms should lower")
            .program,
        )),
        r#"const E = require("flow-enums-runtime").Mirrored(["A"]);
class R {
  constructor({value}) {
    this.value = value;
  }
}
function Foo({value}) {
  return ($$gen$m0 => {
    {
      const x = $$gen$m0;
      return new R({ value: x });
    }
  })(value);
}
"#
    );
}
