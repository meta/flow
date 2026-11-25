/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use flow_aloc::ALoc;
use flow_analysis::test_utils::parse;
use flow_analysis::test_utils::parse_with_alocs;
use flow_common::options::JsxMode;
use flow_common::options::ReactRuntime;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_env_builder::env_api;
use flow_env_builder::env_api::AutocompleteHooks;
use flow_env_builder::env_api::DefLocType;
use flow_env_builder::name_def;
use flow_env_builder::name_def_types::EnvEntriesMap;
use flow_env_builder::name_def_types::ScopeKind;
use flow_env_builder::name_def_types::print::string_of_source;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::FileKeyInner;
use flow_parser::loc_sig::LocSig;
use flow_parser::parse_jsx_pragma_expression;
use flow_parser_utils::file_sig::FileSig;
use flow_parser_utils::file_sig::FileSigOptions;

use crate::dependency_sigs::Context;
use crate::dependency_sigs::Flow;
use crate::name_def_ordering;
use crate::name_def_ordering::Element;
use crate::name_def_ordering::OrderingResult;
use crate::name_resolver;

struct TestFlow;

impl Flow for TestFlow {
    type Cx = TestCx;

    fn add_output(
        _cx: &mut Self::Cx,
        _error: flow_typing_errors::error_message::ErrorMessage<ALoc>,
    ) {
    }
}

struct TestCx {
    jsx_mode: JsxMode,
    react_runtime: ReactRuntime,
}

impl Default for TestCx {
    fn default() -> Self {
        Self {
            jsx_mode: JsxMode::JsxReact,
            react_runtime: ReactRuntime::Classic,
        }
    }
}

impl Context for TestCx {
    fn enable_enums(&self) -> bool {
        true
    }

    fn file(&self) -> FileKey {
        FileKey::new(FileKeyInner::SourceFile("test.js".to_string()))
    }

    fn jsx(&self) -> JsxMode {
        self.jsx_mode.clone()
    }

    fn react_runtime(&self) -> ReactRuntime {
        self.react_runtime
    }

    fn enable_const_params(&self) -> bool {
        false
    }

    fn strict_property_initialization(&self) -> bool {
        false
    }

    fn stylex_shorthand_prop(&self) -> Option<&str> {
        None
    }

    fn add_exhaustive_check(&mut self, _loc: ALoc, _cases: (Vec<ALoc>, bool)) {}

    fn exhaustive_check(&self, _loc: &ALoc) -> Option<(Vec<ALoc>, bool)> {
        None
    }
}

fn autocomplete_hooks() -> AutocompleteHooks<'static, ALoc> {
    AutocompleteHooks {
        id_hook: Box::new(|_, _| false),
        literal_hook: Box::new(|_| false),
        obj_prop_decl_hook: Box::new(|_, _| false),
    }
}

fn print_values(values: &EnvEntriesMap) -> String {
    let kvlist: Vec<_> = values.iter().collect();
    let strlist: Vec<String> = kvlist
        .iter()
        .map(|(key, (init, _, _, _, _))| {
            format!(
                "{} => {}",
                key.loc.debug_to_string(false),
                string_of_source(init)
            )
        })
        .collect();
    if strlist.is_empty() {
        "[\n\n]".to_string()
    } else {
        format!("[\n  {}\n]", strlist.join(";\n  "))
    }
}

fn print_order(lst: &[OrderingResult]) -> String {
    fn print_kind_and_loc(def_loc_type: DefLocType, loc: &ALoc) -> String {
        if def_loc_type == DefLocType::OrdinaryNameLoc {
            loc.debug_to_string(false)
        } else {
            format!(
                "{} (Env_api.Make.{})",
                loc.debug_to_string(false),
                def_loc_type.show()
            )
        }
    }

    fn msg_of_elt(elt: &Element) -> String {
        match elt {
            Element::Normal(env_api::EnvKey { def_loc_type, loc })
            | Element::Resolvable(env_api::EnvKey { def_loc_type, loc }) => {
                print_kind_and_loc(*def_loc_type, loc)
            }
            Element::Illegal(name_def_ordering::Blame {
                payload: env_api::EnvKey { def_loc_type, loc },
                ..
            }) => {
                format!(
                    "illegal self-cycle ({})",
                    print_kind_and_loc(*def_loc_type, loc)
                )
            }
        }
    }

    fn loc_of_elt(elt: &Element) -> &ALoc {
        match elt {
            Element::Normal(env_api::EnvKey { loc, .. })
            | Element::Resolvable(env_api::EnvKey { loc, .. })
            | Element::Illegal(name_def_ordering::Blame {
                payload: env_api::EnvKey { loc, .. },
                ..
            }) => loc,
        }
    }

    fn compare_elt(a: &Element, b: &Element) -> std::cmp::Ordering {
        let c = loc_of_elt(a).cmp(loc_of_elt(b));
        if c == std::cmp::Ordering::Equal {
            msg_of_elt(a).cmp(&msg_of_elt(b))
        } else {
            c
        }
    }

    fn compare_blame(
        a: &(name_def_ordering::Blame<Element>, bool),
        b: &(name_def_ordering::Blame<Element>, bool),
    ) -> std::cmp::Ordering {
        let a_elt = &a.0.payload;
        let b_elt = &b.0.payload;
        let c = loc_of_elt(a_elt).cmp(loc_of_elt(b_elt));
        if c == std::cmp::Ordering::Equal {
            msg_of_elt(a_elt).cmp(&msg_of_elt(b_elt))
        } else {
            c
        }
    }

    let msg: Vec<String> = lst
        .iter()
        .map(|result| match result {
            OrderingResult::Singleton(elt) => msg_of_elt(elt),
            OrderingResult::IllegalSCC(keys) => {
                let mut sorted: Vec<_> = keys.iter().collect();
                sorted.sort_by(|a, b| compare_blame(a, b));
                let strs: Vec<String> = sorted
                    .iter()
                    .map(|(blame, _)| msg_of_elt(&blame.payload))
                    .collect();
                format!("illegal scc: (({}))", strs.join("); ("))
            }
            OrderingResult::ResolvableSCC(elts) => {
                let mut sorted: Vec<_> = elts.iter().collect();
                sorted.sort_by(|a, b| compare_elt(a, b));
                let strs: Vec<String> = sorted.iter().map(|elt| msg_of_elt(elt)).collect();
                format!("legal scc: (({}))", strs.join("); ("))
            }
        })
        .collect();
    msg.join(" =>\n")
}

fn print_init_test(contents: &str) -> String {
    let ast = parse_with_alocs(contents);
    let mut cx = TestCx::default();
    let file_sig = FileSig::from_program(&cx.file(), &parse(contents), &FileSigOptions::default());
    let (_, env) = name_resolver::program_with_scope::<TestCx, TestFlow>(
        &mut cx,
        false,
        FlowOrdSet::new(),
        &file_sig,
        &ast,
    );
    let env = env.to_env_info();
    let hooks = autocomplete_hooks();
    let (inits, _) = name_def::find_defs(
        &hooks,
        true,
        Default::default(),
        &env,
        ScopeKind::Module,
        &ast,
    )
    .expect("name def should not hit an internal error in test");
    print_values(&inits)
}

fn print_order_test(
    custom_jsx: Option<&str>,
    react_runtime_automatic: bool,
    contents: &str,
) -> String {
    let mut cx = TestCx::default();
    if react_runtime_automatic {
        cx.react_runtime = ReactRuntime::Automatic;
    }
    match custom_jsx {
        None => {}
        Some(s) => {
            let (ast, _errors) = parse_jsx_pragma_expression::<()>(None, s).unwrap();
            let aloc_ast = flow_aloc::loc_to_aloc_expression_owned(ast);
            cx.jsx_mode = JsxMode::JsxPragma(s.to_string(), aloc_ast);
        }
    }
    let ast = parse_with_alocs(contents);
    let file_sig = FileSig::from_program(&cx.file(), &parse(contents), &FileSigOptions::default());
    let (_, env) = name_resolver::program_with_scope::<TestCx, TestFlow>(
        &mut cx,
        false,
        FlowOrdSet::new(),
        &file_sig,
        &ast,
    );
    let env = env.to_env_info();
    let hooks = autocomplete_hooks();
    let (inits, _) = name_def::find_defs(
        &hooks,
        true,
        Default::default(),
        &env,
        ScopeKind::Module,
        &ast,
    )
    .expect("name def should not hit an internal error in test");
    let order =
        name_def_ordering::build_ordering::<_, _, TestCx, TestFlow>(&mut cx, &hooks, &env, &inits)
            .unwrap_or_else(|_| Vec::new());
    print_order(&order)
}

#[test]
fn test_decl() {
    let result = print_init_test(
        r#"
let x = 42;
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => val (2, 8) to (2, 10)
]"#
    );
}

#[test]
fn test_decl_annot() {
    let result = print_init_test(
        r#"
let x: string = 42;
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => annot (2, 5) to (2, 13)
]"#
    );
}

#[test]
fn test_decl_annot_no_init() {
    let result = print_init_test(
        r#"
let x: number;
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => annot (2, 5) to (2, 13)
]"#
    );
}

#[test]
fn test_decl_nothing() {
    let result = print_init_test(
        r#"
let x;
  "#,
    );
    assert_eq!(
        result,
        r#"[

]"#
    );
}

#[test]
fn test_assign() {
    let result = print_init_test(
        r#"
x = 42;
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 0) to (2, 1) => val (2, 4) to (2, 6)
]"#
    );
}

#[test]
fn test_elems() {
    let result = print_init_test(
        r#"
let [a,b] = 42;
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 5) to (2, 6) => (val (2, 12) to (2, 14))[0];
  (2, 7) to (2, 8) => (val (2, 12) to (2, 14))[1];
  (2, 4) to (2, 9) => val (2, 12) to (2, 14)
]"#
    );
}

#[test]
fn test_elems_hole() {
    let result = print_init_test(
        r#"
let [a,,b] = 42;
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 5) to (2, 6) => (val (2, 13) to (2, 15))[0];
  (2, 8) to (2, 9) => (val (2, 13) to (2, 15))[2];
  (2, 4) to (2, 10) => val (2, 13) to (2, 15)
]"#
    );
}

#[test]
fn test_elems_rest() {
    let result = print_init_test(
        r#"
let [a,,b,...c] = 42;
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 5) to (2, 6) => (val (2, 18) to (2, 20))[0];
  (2, 8) to (2, 9) => (val (2, 18) to (2, 20))[2];
  (2, 13) to (2, 14) => (val (2, 18) to (2, 20))[...];
  (2, 4) to (2, 15) => val (2, 18) to (2, 20)
]"#
    );
}

#[test]
fn test_elems_def() {
    let result = print_init_test(
        r#"
let [a=42] = 42;
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 5) to (2, 6) => (val (2, 13) to (2, 15))[0];
  (2, 4) to (2, 10) => val (2, 13) to (2, 15)
]"#
    );
}

#[test]
fn test_props() {
    let result = print_init_test(
        r#"
let {a, b} = 42;
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 5) to (2, 6) => (val (2, 13) to (2, 15)).a;
  (2, 8) to (2, 9) => (val (2, 13) to (2, 15)).b;
  (2, 4) to (2, 10) => val (2, 13) to (2, 15)
]"#
    );
}

#[test]
fn test_props_lit() {
    let result = print_init_test(
        r#"
let {a, '42':b} = 42;
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 5) to (2, 6) => (val (2, 18) to (2, 20)).a;
  (2, 13) to (2, 14) => (val (2, 18) to (2, 20)).42;
  (2, 4) to (2, 15) => val (2, 18) to (2, 20)
]"#
    );
}

#[test]
fn test_props_comp_rest() {
    let result = print_init_test(
        r#"
let {a, [foo()]: b, ...c} = 42;
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 5) to (2, 6) => (val (2, 28) to (2, 30)).a;
  (2, 17) to (2, 18) => (val (2, 28) to (2, 30)).[computed];
  (2, 23) to (2, 24) => (val (2, 28) to (2, 30)){ ... };
  (2, 4) to (2, 25) => val (2, 28) to (2, 30)
]"#
    );
}

#[test]
fn test_props_comp_rest2() {
    let result = print_order_test(
        None,
        false,
        r#"
let x: { [number]: number => number } = { [42]: v => v };
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 4) to (2, 5) =>
(2, 43) to (2, 45) (Env_api.Make.ExpressionLoc) =>
(2, 48) to (2, 49)"#
    );
}

#[test]
fn test_function_def() {
    let result = print_init_test(
        r#"
function f(y, z: number) {
  x = 42;
}
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 9) to (2, 10) => fun f;
  (2, 11) to (2, 12) => contextual;
  (2, 14) to (2, 15) => annot (2, 15) to (2, 23);
  (3, 2) to (3, 3) => val (3, 6) to (3, 8)
]"#
    );
}

#[test]
fn test_function_exp() {
    let result = print_init_test(
        r#"
var w = function f(y, z: number) {
  x = 42;
}
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => function val (2, 8) to (4, 1);
  (2, 17) to (2, 18) => fun f;
  (2, 19) to (2, 20) => contextual;
  (2, 22) to (2, 23) => annot (2, 23) to (2, 31);
  (3, 2) to (3, 3) => val (3, 6) to (3, 8)
]"#
    );
}

#[test]
fn test_fun_tparam() {
    let result = print_init_test(
        r#"
var w = function <X, Y:number>() { }
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => function val (2, 8) to (2, 36);
  (2, 8) to (2, 36) => fun <anonymous>;
  (2, 18) to (2, 19) => tparam (2, 18) to (2, 19);
  (2, 21) to (2, 22) => tparam (2, 21) to (2, 29)
]"#
    );
}

#[test]
fn test_type_alias() {
    let result = print_init_test(
        r#"
type T = number;
type P<X> = X;
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 5) to (2, 6) => alias (2, 9) to (2, 15);
  (3, 5) to (3, 6) => alias (3, 12) to (3, 13);
  (3, 7) to (3, 8) => tparam (3, 7) to (3, 8)
]"#
    );
}

#[test]
fn test_deps() {
    let result = print_order_test(
        None,
        false,
        r#"
let x = 1;
let y = x;
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 4) to (2, 5) =>
(3, 4) to (3, 5)"#
    );
}

#[test]
fn test_deps_on_type() {
    let result = print_order_test(
        None,
        false,
        r#"
type T = number;
let x: T = 1;
let y = x;
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 5) to (2, 6) =>
(3, 4) to (3, 5) =>
(4, 4) to (4, 5)"#
    );
}

#[test]
fn test_deps_recur() {
    let result = print_order_test(
        None,
        false,
        r#"
type T = number;
let x: T;
function f() {
  x = x;
  return 42;
}
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 5) to (2, 6) =>
(3, 4) to (3, 5) =>
(5, 2) to (5, 3) =>
(4, 9) to (4, 10)"#
    );
}

#[test]
fn test_recur_func() {
    let result = print_order_test(
        None,
        false,
        r#"
function f() {
  return f();
}
  "#,
    );
    assert_eq!(result, r#"illegal self-cycle ((2, 9) to (2, 10))"#);
}

#[test]
fn test_recur_func_anno() {
    let result = print_order_test(
        None,
        false,
        r#"
function f(): void {
  return f();
}
  "#,
    );
    assert_eq!(result, r#"(2, 9) to (2, 10)"#);
}

#[test]
fn test_recur_fun_typeof() {
    let result = print_order_test(
        None,
        false,
        r#"
var x = f ();
function f(): typeof x {
}
  "#,
    );
    assert_eq!(
        result,
        r#"illegal scc: (((2, 4) to (2, 5)); ((3, 9) to (3, 10)))"#
    );
}

#[test]
fn test_function_param_contextual() {
    let result = print_order_test(
        None,
        false,
        r#"
var x = 1;
x = (a) => a;
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 4) to (2, 5) =>
(3, 5) to (3, 6) =>
(3, 0) to (3, 1)"#
    );
}

#[test]
fn test_deps1() {
    let result = print_order_test(
        None,
        false,
        r#"
type T = number;
let x: T;
function f() {
  x = x;
  return 42;
}
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 5) to (2, 6) =>
(3, 4) to (3, 5) =>
(5, 2) to (5, 3) =>
(4, 9) to (4, 10)"#
    );
}

#[test]
fn test_deps2() {
    let result = print_order_test(
        None,
        false,
        r#"
type T = Array<T>;
type S = W;
type W = S;
let x, y, z;
function nested() {
  x = y;
  y = z;
  z = x;
  return 42;
}
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 5) to (2, 6) =>
legal scc: (((3, 5) to (3, 6)); ((4, 5) to (4, 6))) =>
illegal scc: (((7, 2) to (7, 3)); ((8, 2) to (8, 3)); ((9, 2) to (9, 3))) =>
(6, 9) to (6, 15)"#
    );
}

#[test]
fn test_deps2a() {
    let result = print_order_test(
        None,
        false,
        r#"
let x = f();
function f() {
  return x;
}
  "#,
    );
    assert_eq!(
        result,
        r#"illegal scc: (((2, 4) to (2, 5)); ((3, 9) to (3, 10)))"#
    );
}

#[test]
fn test_deps3() {
    let result = print_order_test(
        None,
        false,
        r#"
function invalidate_x() {
  x = null; return x;
}

var x = null;
invalidate_x();
invariant(typeof x !== 'number');
(x: null);
x = 42;
(x: number);
  "#,
    );
    assert_eq!(
        result,
        r#"(6, 4) to (6, 5) =>
(10, 0) to (10, 1) =>
(3, 2) to (3, 3) =>
(2, 9) to (2, 21) =>
(8, 10) to (8, 31) (Env_api.Make.ExpressionLoc)"#
    );
}

#[test]
fn test_typeof1() {
    let result = print_order_test(
        None,
        false,
        r#"
var x = 42;
type T = typeof x;
var z: T = 100;
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 4) to (2, 5) =>
(3, 5) to (3, 6) =>
(4, 4) to (4, 5)"#
    );
}

#[test]
fn test_typeof2() {
    let result = print_order_test(
        None,
        false,
        r#"
var x = (42: T);
type T = typeof x;
  "#,
    );
    assert_eq!(
        result,
        r#"illegal scc: (((2, 4) to (2, 5)); ((3, 5) to (3, 6)))"#
    );
}

#[test]
fn test_func_exp() {
    let result = print_order_test(
        None,
        false,
        r#"
var y = function f(): number {
  return 42;
}
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 4) to (2, 5) =>
(2, 17) to (2, 18)"#
    );
}

#[test]
fn test_record_def() {
    let result = print_init_test(
        r#"
let x;
record R {
  foo() { x = 42; return 42 }
}
x = 10;
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 7) to (3, 8) => record R;
  (4, 10) to (4, 11) => val (4, 14) to (4, 16);
  (6, 0) to (6, 1) => val (6, 4) to (6, 6)
]"#
    );
}

#[test]
fn test_class_def() {
    let result = print_init_test(
        r#"
let x;
class C {
  foo() { x = 42; return 42 }
}
x = 10;
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 6) to (3, 7) => class C;
  (4, 10) to (4, 11) => val (4, 14) to (4, 16);
  (6, 0) to (6, 1) => val (6, 4) to (6, 6)
]"#
    );
}

#[test]
fn test_class_def2() {
    let result = print_init_test(
        r#"
var x = 42;
let foo = class C<Y: typeof x> { };
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => val (2, 8) to (2, 10);
  (3, 4) to (3, 7) => val (3, 10) to (3, 34);
  (3, 16) to (3, 17) => class C;
  (3, 18) to (3, 19) => tparam (3, 18) to (3, 29)
]"#
    );
}

#[test]
fn test_class1() {
    let result = print_order_test(
        None,
        false,
        r#"
let x;
class C {
  foo() { x = 42; return 42; }
}
x = 10;
  "#,
    );
    assert_eq!(
        result,
        r#"(3, 6) to (3, 7) =>
(6, 0) to (6, 1) =>
(4, 10) to (4, 11)"#
    );
}

#[test]
fn test_class2() {
    let result = print_order_test(
        None,
        false,
        r#"
var x = 42;
let foo = class C<Y: typeof x> { };
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 4) to (2, 5) =>
(3, 18) to (3, 19) =>
(3, 16) to (3, 17) =>
(3, 4) to (3, 7)"#
    );
}

#[test]
fn test_class3() {
    let result = print_order_test(
        None,
        false,
        r#"
class C {
  foo: D;
}
class D extends C {
  bar;
}
  "#,
    );
    assert_eq!(
        result,
        r#"legal scc: (((2, 6) to (2, 7)); ((5, 6) to (5, 7)))"#
    );
}

#[test]
fn test_class3_anno() {
    let result = print_order_test(
        None,
        false,
        r#"
class C {
  foo: D;
}
class D extends C {
  bar: C;
}
  "#,
    );
    assert_eq!(
        result,
        r#"legal scc: (((2, 6) to (2, 7)); ((5, 6) to (5, 7)))"#
    );
}

#[test]
fn test_class_extends_cast() {
    let result = print_order_test(
        None,
        false,
        r#"
type Ref = { children: Array<Node> };

declare const referencedInClassExtends: Ref;

declare function f(v: mixed): mixed;

class Node extends (f(referencedInClassExtends) as any) {}
  "#,
    );
    assert_eq!(
        result,
        r#"(8, 6) to (8, 10) =>
(2, 5) to (2, 8) =>
(4, 14) to (4, 38) =>
(6, 17) to (6, 18) =>
(8, 22) to (8, 46) (Env_api.Make.ExpressionLoc)"#
    );
}

#[test]
fn test_enum() {
    let result = print_order_test(
        None,
        false,
        r#"
function havoced() {
  var x: E = E.Foo; return 42;
}
enum E {
  Foo
}
  "#,
    );
    assert_eq!(
        result,
        r#"(5, 5) to (5, 6) =>
(3, 6) to (3, 7) =>
(2, 9) to (2, 16)"#
    );
}

#[test]
fn test_interface() {
    let result = print_order_test(
        None,
        false,
        r#"
interface I extends J { x: J }
interface J { h: number }
  "#,
    );
    assert_eq!(
        result,
        r#"(3, 10) to (3, 11) =>
(2, 10) to (2, 11)"#
    );
}

#[test]
fn test_interface_class_anno_cycle() {
    let result = print_order_test(
        None,
        false,
        r#"
interface I extends J { x: J }
interface J { h: C }
class C implements I { }
  "#,
    );
    assert_eq!(
        result,
        r#"legal scc: (((2, 10) to (2, 11)); ((3, 10) to (3, 11)); ((4, 6) to (4, 7)))"#
    );
}

#[test]
fn test_import() {
    let result = print_init_test(
        r#"
import typeof B, * as A from 'x';
import type C, * as D from 'x';
import E from 'x';
import F, {type G, typeof H, J } from 'x';
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 14) to (2, 15) => import typeof default from x;
  (2, 22) to (2, 23) => import typeof namespace from x;
  (3, 12) to (3, 13) => import type default from x;
  (3, 20) to (3, 21) => import type namespace from x;
  (4, 7) to (4, 8) => import default from x;
  (5, 7) to (5, 8) => import default from x;
  (5, 16) to (5, 17) => import type G from x;
  (5, 26) to (5, 27) => import typeof H from x;
  (5, 29) to (5, 30) => import J from x
]"#
    );
}

#[test]
fn test_refi_instanceof() {
    let result = print_order_test(
        None,
        false,
        r#"
class C {
  foo() { return y; }
}
declare var x: mixed;
var y;

if (x instanceof C) {
  y = x;
}
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 6) to (2, 7) =>
(5, 12) to (5, 13) =>
(8, 17) to (8, 18) (Env_api.Make.ExpressionLoc) =>
(9, 2) to (9, 3)"#
    );
}

#[test]
fn test_refi_latent() {
    let result = print_order_test(
        None,
        false,
        r#"
function f() { return y; }
declare var x: mixed;
var y;

if (f(x)) {
  y = x;
}
  "#,
    );
    assert_eq!(
        result,
        r#"(3, 12) to (3, 13) =>
(6, 6) to (6, 7) (Env_api.Make.ExpressionLoc) =>
illegal scc: (((2, 9) to (2, 10)); ((7, 2) to (7, 3)))"#
    );
}

#[test]
fn test_refi_latent_complex() {
    let result = print_order_test(
        None,
        false,
        r#"
function f() { return (x: any) => y; }
declare var x: mixed;
var y;

if (f()(x)) {
  y = x;
}
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 23) to (2, 24) =>
(3, 12) to (3, 13) =>
(6, 8) to (6, 9) (Env_api.Make.ExpressionLoc) =>
illegal scc: (((2, 9) to (2, 10)); ((7, 2) to (7, 3)))"#
    );
}

#[test]
fn test_refi_sentinel() {
    let result = print_order_test(
        None,
        false,
        r#"
declare var x: mixed;
let y;

if (x.type === 1) {
  y = x;
}
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 12) to (2, 13) =>
(5, 4) to (5, 10) =>
(5, 15) to (5, 16) (Env_api.Make.ExpressionLoc) =>
(6, 2) to (6, 3)"#
    );
}

#[test]
fn test_declare_class() {
    let result = print_order_test(
        None,
        false,
        r#"
declare class C mixins S { }
var f = new C();
type S = typeof f;
  "#,
    );
    assert_eq!(
        result,
        r#"illegal scc: (((2, 14) to (2, 15)); ((3, 4) to (3, 5)); ((4, 5) to (4, 6)))"#
    );
}

#[test]
fn test_declare_class2() {
    let result = print_order_test(
        None,
        false,
        r#"
declare class C<S> {
  foo<T>(x: T): P;
}
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 16) to (2, 17) =>
legal scc: (((2, 14) to (2, 15)); ((3, 6) to (3, 7)))"#
    );
}

#[test]
fn test_opaque() {
    let result = print_order_test(
        None,
        false,
        r#"
type Y<W> = T<W>
type S<T> = T
opaque type T<X>: S<X> = Y<X>
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 7) to (2, 8) =>
(3, 7) to (3, 8) =>
(3, 5) to (3, 6) =>
(4, 14) to (4, 15) =>
legal scc: (((2, 5) to (2, 6)); ((4, 12) to (4, 13)))"#
    );
}

#[test]
fn test_unannotated_catch() {
    let result = print_init_test(
        r#"
var x;
try {} catch (e) { x = e }
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 14) to (3, 15) => unannotated catch param;
  (3, 19) to (3, 20) => val (3, 23) to (3, 24)
]"#
    );
}

#[test]
fn test_catch_with_mixed() {
    let result = print_init_test(
        r#"
var x;
try {} catch (e: mixed) { x = e }
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 14) to (3, 15) => annot (3, 15) to (3, 22);
  (3, 26) to (3, 27) => val (3, 30) to (3, 31)
]"#
    );
}

#[test]
fn test_for1() {
    let result = print_init_test(
        r#"
for (var x = 0;;) { }
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 9) to (2, 10) => val (2, 13) to (2, 14)
]"#
    );
}

#[test]
fn test_for2() {
    let result = print_init_test(
        r#"
var x;
for (x = 0;;) { }
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 5) to (3, 6) => val (3, 9) to (3, 10)
]"#
    );
}

#[test]
fn test_for3() {
    let result = print_init_test(
        r#"
for (var [x,y] = [1,2];;) { }
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 10) to (2, 11) => (val (2, 17) to (2, 22))[0];
  (2, 12) to (2, 13) => (val (2, 17) to (2, 22))[1];
  (2, 9) to (2, 14) => val (2, 17) to (2, 22)
]"#
    );
}

#[test]
fn test_for4() {
    let result = print_init_test(
        r#"
for (var [x,y]: T = [1,2];;) { }
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 10) to (2, 11) => (annot (2, 14) to (2, 17))[0];
  (2, 12) to (2, 13) => (annot (2, 14) to (2, 17))[1];
  (2, 9) to (2, 17) => annot (2, 14) to (2, 17)
]"#
    );
}

#[test]
fn test_for_in1() {
    let result = print_init_test(
        r#"
for (var x in foo) { }
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 9) to (2, 10) => for in (2, 14) to (2, 17)
]"#
    );
}

#[test]
fn test_for_in2() {
    let result = print_init_test(
        r#"
var x;
for (x in foo) { }
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 5) to (3, 6) => for in (3, 10) to (3, 13)
]"#
    );
}

#[test]
fn test_for_in3() {
    let result = print_init_test(
        r#"
for (var [x,y] in foo) { }
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 10) to (2, 11) => (for in (2, 18) to (2, 21))[0];
  (2, 12) to (2, 13) => (for in (2, 18) to (2, 21))[1];
  (2, 9) to (2, 14) => for in (2, 18) to (2, 21)
]"#
    );
}

#[test]
fn test_for_in4() {
    let result = print_init_test(
        r#"
for (var [x,y]: T in foo) { }
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 10) to (2, 11) => (annot (2, 14) to (2, 17))[0];
  (2, 12) to (2, 13) => (annot (2, 14) to (2, 17))[1];
  (2, 9) to (2, 17) => annot (2, 14) to (2, 17)
]"#
    );
}

#[test]
fn test_for_of1() {
    let result = print_init_test(
        r#"
for (var x of foo) { }
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 9) to (2, 10) => for of (2, 14) to (2, 17)
]"#
    );
}

#[test]
fn test_for_of2() {
    let result = print_init_test(
        r#"
var x;
for (x of foo) { }
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 5) to (3, 6) => for of (3, 10) to (3, 13)
]"#
    );
}

#[test]
fn test_for_of3() {
    let result = print_init_test(
        r#"
for (var [x,y] of foo) { }
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 10) to (2, 11) => (for of (2, 18) to (2, 21))[0];
  (2, 12) to (2, 13) => (for of (2, 18) to (2, 21))[1];
  (2, 9) to (2, 14) => for of (2, 18) to (2, 21)
]"#
    );
}

#[test]
fn test_for_of4() {
    let result = print_init_test(
        r#"
for (var [x,y]: T of foo) { }
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 10) to (2, 11) => (annot (2, 14) to (2, 17))[0];
  (2, 12) to (2, 13) => (annot (2, 14) to (2, 17))[1];
  (2, 9) to (2, 17) => annot (2, 14) to (2, 17)
]"#
    );
}

#[test]
fn test_heap_init() {
    let result = print_init_test(
        r#"
if (x.y) { x.y }
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 7) => exp (2, 4) to (2, 7) (hint = [])
]"#
    );
}

#[test]
fn test_destructuring_init() {
    let result = print_init_test(
        r#"
const {foo: [bar, baz = 3], hello: {world: flow}} = global;
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 13) to (2, 16) => ((val (2, 52) to (2, 58)).foo)[0];
  (2, 18) to (2, 21) => ((val (2, 52) to (2, 58)).foo)[1];
  (2, 43) to (2, 47) => ((val (2, 52) to (2, 58)).hello).world;
  (2, 6) to (2, 49) => val (2, 52) to (2, 58);
  (2, 12) to (2, 26) => (val (2, 52) to (2, 58)).foo;
  (2, 35) to (2, 48) => (val (2, 52) to (2, 58)).hello
]"#
    );
}

#[test]
fn test_destructuring_param_empty_1() {
    let result = print_order_test(
        None,
        false,
        r#"
function f([]) {}
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 11) to (2, 13) (Env_api.Make.FunctionParamLoc) =>
(2, 9) to (2, 10)"#
    );
}

#[test]
fn test_destructuring_param_empty_2() {
    let result = print_order_test(
        None,
        false,
        r#"
function f([]) { return 1 }
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 11) to (2, 13) (Env_api.Make.FunctionParamLoc) =>
(2, 9) to (2, 10)"#
    );
}

#[test]
fn test_destructuring_param_empty_3() {
    let result = print_order_test(
        None,
        false,
        r#"
function f([]: mixed) {}
  "#,
    );
    assert_eq!(result, r#"(2, 9) to (2, 10)"#);
}

#[test]
fn test_destructuring_param_empty_4() {
    let result = print_order_test(
        None,
        false,
        r#"
function f([]: mixed) { return 1 }
  "#,
    );
    assert_eq!(result, r#"(2, 9) to (2, 10)"#);
}

#[test]
fn test_destructuring_assignment_empty() {
    let result = print_order_test(
        None,
        false,
        r#"
function f() {
  [] = 1;
  return 1;
}
  "#,
    );
    assert_eq!(result, r#"(2, 9) to (2, 10)"#);
}

#[test]
fn test_destructuring_assignment_illegal() {
    let result = print_order_test(
        None,
        false,
        r#"
function f() {
  [m.foo] = 1;
  return 1;
}
  "#,
    );
    assert_eq!(result, r#"(2, 9) to (2, 10)"#);
}

#[test]
fn test_destructuring_order() {
    let result = print_order_test(
        None,
        false,
        r#"
const {foo: [bar, baz = 3], hello: {world: flow}} = global;
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 6) to (2, 49) (Env_api.Make.PatternLoc) =>
(2, 12) to (2, 26) (Env_api.Make.PatternLoc) =>
(2, 13) to (2, 16) =>
(2, 18) to (2, 21) =>
(2, 35) to (2, 48) (Env_api.Make.PatternLoc) =>
(2, 43) to (2, 47)"#
    );
}

#[test]
fn test_refi_recorded_read() {
    let result = print_order_test(
        None,
        false,
        r#"
if (x.prop) {
  const y = x.prop;
}
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 4) to (2, 10) =>
(3, 8) to (3, 9)"#
    );
}

#[test]
fn test_sentinel_refi() {
    let result = print_order_test(
        None,
        false,
        r#"
if (obj.d.type === "a") {
  let {d: {payload, type}} = obj;
}
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 4) to (2, 9) =>
(2, 4) to (2, 14) =>
(3, 6) to (3, 26) (Env_api.Make.PatternLoc) =>
(2, 19) to (2, 22) (Env_api.Make.ExpressionLoc) =>
(3, 10) to (3, 25) (Env_api.Make.PatternLoc) =>
(3, 11) to (3, 18) =>
(3, 20) to (3, 24)"#
    );
}

#[test]
fn test_inc() {
    let result = print_order_test(
        None,
        false,
        r#"
let x;
var y;
function f() {
  y = x; return 42;
}
x++;
  "#,
    );
    assert_eq!(
        result,
        r#"(7, 0) to (7, 1) =>
(5, 2) to (5, 3) =>
(4, 9) to (4, 10)"#
    );
}

#[test]
fn test_opassign() {
    let result = print_order_test(
        None,
        false,
        r#"
let x;
var y;
function f() {
  y = x; return 42;
}
function h() {
  x += y; return 42;
}
  "#,
    );
    assert_eq!(
        result,
        r#"illegal scc: (((5, 2) to (5, 3)); (illegal self-cycle ((8, 2) to (8, 3)))) =>
(4, 9) to (4, 10) =>
(7, 9) to (7, 10)"#
    );
}

#[test]
fn test_type_alias_order() {
    let result = print_order_test(
        None,
        false,
        r#"
type $unwrap = <T>(l: JSResourceReference<T>) => T;

class JSResourceReference<+T> {
  static loadAll<I: Array<JSResourceReference<mixed>>>(
    loaders: I,
    callback: I,
  ): void {
    // ...load the modules and then pass them to the callback
  }
}

  "#,
    );
    assert_eq!(
        result,
        r#"(2, 16) to (2, 17) =>
(4, 27) to (4, 28) =>
legal scc: (((4, 6) to (4, 25)); ((5, 17) to (5, 18)); ((6, 4) to (6, 11)); ((7, 4) to (7, 12))) =>
(2, 5) to (2, 12)"#
    );
}

#[test]
fn test_type_params_dep() {
    let result = print_order_test(
        None,
        false,
        r#"
function foo<A, B: A, C = A>() {}
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 13) to (2, 14) =>
(2, 16) to (2, 17) =>
(2, 22) to (2, 23) =>
(2, 9) to (2, 12)"#
    );
}

#[test]
fn test_refi() {
    let result = print_order_test(
        None,
        false,
        r#"
function havoc(x) {
  let y;
  if (x instanceof R.Y) {
    y = x;
  }
}
import * as R from 'foo';
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 15) to (2, 16) =>
(2, 9) to (2, 14) =>
(8, 12) to (8, 13) =>
(4, 19) to (4, 22) (Env_api.Make.ExpressionLoc) =>
(5, 4) to (5, 5)"#
    );
}

#[test]
fn test_object_prop_assign() {
    let result = print_order_test(
        None,
        false,
        r#"
function test(obj) {
  obj.prop = 1;
}
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 14) to (2, 17) =>
(2, 9) to (2, 13) =>
(3, 2) to (3, 10)"#
    );
}

#[test]
fn test_array_index_assign() {
    let result = print_order_test(
        None,
        false,
        r#"
function test(arr, i) {
  arr[i++] = 1;
}
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 14) to (2, 17) =>
(2, 19) to (2, 20) =>
(2, 9) to (2, 13) =>
(3, 6) to (3, 7) =>
(3, 6) to (3, 9) (Env_api.Make.ExpressionLoc)"#
    );
}

#[test]
fn test_this() {
    let result = print_order_test(
        None,
        false,
        r#"
class C {
  f() { return this.g() }
  g() { return 42; }
}
  "#,
    );
    assert_eq!(result, r#"(2, 6) to (2, 7)"#);
}

#[test]
fn test_class_question() {
    let result = print_order_test(
        None,
        false,
        r#"
class C {
  f(x: C): boolean { }
}
  "#,
    );
    assert_eq!(
        result,
        r#"legal scc: (((2, 6) to (2, 7)); ((3, 4) to (3, 5)))"#
    );
}

#[test]
fn test_arr() {
    let result = print_order_test(
        None,
        false,
        r#"
var x = []
function w() {
  var z = x; return 42;
}
function g() {
  x = 42; return 42;
}
x.push(42);
  "#,
    );
    assert_eq!(
        result,
        r#"(9, 7) to (9, 9) (Env_api.Make.ArrayProviderLoc) =>
(2, 4) to (2, 5) =>
(4, 6) to (4, 7) =>
(3, 9) to (3, 10) =>
(7, 2) to (7, 3) =>
(6, 9) to (6, 10)"#
    );
}

#[test]
fn test_declare_module() {
    let result = print_order_test(
        None,
        false,
        r#"
declare module 'a' {
  declare type T = S;
}
type S = number;
declare module B {
  declare function f(S): S;
}
  "#,
    );
    assert_eq!(
        result,
        r#"(5, 5) to (5, 6) =>
(3, 15) to (3, 16) =>
(7, 19) to (7, 20)"#
    );
}

#[test]
fn test_declare_module_crash_regression() {
    let result = print_order_test(
        None,
        false,
        r#"
declare module 'a' {
  declare type T = number;
  declare type T = number;
}
  "#,
    );
    assert_eq!(result, r#"(3, 15) to (3, 16)"#);
}

#[test]
fn test_declare_namespace() {
    let result = print_order_test(
        None,
        false,
        r#"
declare namespace a {
  declare type T = S;
}
type S = number;
declare namespace B {
  declare function f(S): S;
}
declare namespace F {
  declare type T = string;
  declare const v: string;
}
const c = F.v;
type T = F.T;
  "#,
    );
    assert_eq!(
        result,
        r#"(5, 5) to (5, 6) =>
legal scc: (((2, 18) to (2, 19)); ((3, 15) to (3, 16))) =>
legal scc: (((6, 18) to (6, 19)); ((7, 19) to (7, 20))) =>
(10, 15) to (10, 16) =>
(11, 16) to (11, 17) =>
(9, 18) to (9, 19) =>
(13, 6) to (13, 7) =>
(14, 5) to (14, 6)"#
    );
}

#[test]
fn test_declare_namespace_duplicate_names() {
    let result = print_order_test(
        None,
        false,
        r#"
declare namespace a {
  declare type T = number;
  declare type T = number;
}
  "#,
    );
    assert_eq!(
        result,
        r#"(3, 15) to (3, 16) =>
(2, 18) to (2, 19)"#
    );
}

#[test]
fn test_empty_arr() {
    let result = print_order_test(
        None,
        false,
        r#"
var x = [];
x.push(42);
  "#,
    );
    assert_eq!(
        result,
        r#"(3, 7) to (3, 9) (Env_api.Make.ArrayProviderLoc) =>
(2, 4) to (2, 5)"#
    );
}

#[test]
fn test_left_to_right_deps() {
    let result = print_order_test(
        None,
        false,
        r#"
pipe(
  "",
  s => 1,
  n => "",
);
  "#,
    );
    assert_eq!(
        result,
        r#"(3, 2) to (3, 4) (Env_api.Make.ExpressionLoc) =>
(4, 2) to (4, 3) =>
(4, 2) to (4, 8) (Env_api.Make.ExpressionLoc) =>
(5, 2) to (5, 3) =>
(5, 2) to (5, 9) (Env_api.Make.ExpressionLoc)"#
    );
}

#[test]
fn test_left_to_right_cycles() {
    let result = print_order_test(
        None,
        false,
        r#"
let x;
pipe(
  "",
  s => x,
  n => x = n,
);
  "#,
    );
    assert_eq!(
        result,
        r#"(4, 2) to (4, 4) (Env_api.Make.ExpressionLoc) =>
(5, 2) to (5, 3) =>
illegal scc: (((5, 2) to (5, 8) (Env_api.Make.ExpressionLoc)); ((6, 2) to (6, 3)); ((6, 7) to (6, 8))) =>
(6, 2) to (6, 12) (Env_api.Make.ExpressionLoc)"#
    );
}

#[test]
fn test_inner_outer_class() {
    let result = print_order_test(
        None,
        false,
        r#"
class Outer {
 constructor() {
   var Inner = class {
      constructor() {
        var x = new Outer();
      }
   };
 }
}
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 6) to (2, 11) =>
(4, 15) to (8, 4) =>
(6, 12) to (6, 13) =>
(4, 7) to (4, 12)"#
    );
}

#[test]
fn test_arr_cycle() {
    let result = print_order_test(
        None,
        false,
        r#"
function foo(arr: $ReadOnlyArray<Object>) {
    return arr.map(foo)
        .reduce((acc, item) => acc.concat(item), [])
  }
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 13) to (2, 16) =>
illegal scc: ((illegal self-cycle ((2, 9) to (2, 12))); ((3, 19) to (3, 22) (Env_api.Make.ExpressionLoc)); ((4, 16) to (4, 47) (Env_api.Make.ExpressionLoc)); ((4, 17) to (4, 20)); ((4, 22) to (4, 26)); ((4, 42) to (4, 46) (Env_api.Make.ExpressionLoc)); ((4, 49) to (4, 51) (Env_api.Make.ExpressionLoc)))"#
    );
}

#[test]
fn test_new_cycle() {
    let result = print_order_test(
        None,
        false,
        r#"
function foo(arr: $ReadOnlyArray<Object>) {
    return arr.map(foo)
        .reduce((acc, item) => acc.concat(item), new Set())
  }
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 13) to (2, 16) =>
illegal scc: ((illegal self-cycle ((2, 9) to (2, 12))); ((3, 19) to (3, 22) (Env_api.Make.ExpressionLoc)); ((4, 16) to (4, 47) (Env_api.Make.ExpressionLoc)); ((4, 17) to (4, 20)); ((4, 22) to (4, 26)); ((4, 42) to (4, 46) (Env_api.Make.ExpressionLoc)); ((4, 49) to (4, 58) (Env_api.Make.ExpressionLoc)))"#
    );
}

#[test]
fn test_lit_cycle() {
    let result = print_order_test(
        None,
        false,
        r#"
function foo(arr: $ReadOnlyArray<Object>) {
    return arr.map(foo)
        .reduce((acc, item) => acc.concat(item), 42)
  }
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 13) to (2, 16) =>
(4, 49) to (4, 51) (Env_api.Make.ExpressionLoc) =>
illegal scc: ((illegal self-cycle ((2, 9) to (2, 12))); ((3, 19) to (3, 22) (Env_api.Make.ExpressionLoc)); ((4, 16) to (4, 47) (Env_api.Make.ExpressionLoc)); ((4, 17) to (4, 20)); ((4, 22) to (4, 26)); ((4, 42) to (4, 46) (Env_api.Make.ExpressionLoc)))"#
    );
}

#[test]
fn test_obj_cycle() {
    let result = print_order_test(
        None,
        false,
        r#"
const RecursiveObj = {
  nonSynthesizableProp: 1 == 1,

  f(): void {
    RecursiveObj.f(true ? 1 : []);
  },
};
  "#,
    );
    assert_eq!(
        result,
        r#"illegal scc: ((illegal self-cycle ((2, 6) to (2, 18))); ((6, 19) to (6, 32) (Env_api.Make.ExpressionLoc)))"#
    );
}

#[test]
fn test_react_fwd() {
    let result = print_order_test(
        None,
        false,
        r#"
import * as React from 'react';

const RC = <T />

function T(): void {}
"#,
    );
    assert_eq!(
        result,
        r#"(2, 12) to (2, 17) =>
(6, 9) to (6, 10) =>
(4, 6) to (4, 8)"#
    );
}

#[test]
fn test_std_jsx() {
    let result = print_order_test(
        None,
        false,
        r#"
<FirstElement />
function time_to_create_some_elements_bro() {
  return <SecondElement />
}
function createMikesCoolElement() { return 42 };
import * as React from 'react';
"#,
    );
    assert_eq!(
        result,
        r#"(7, 12) to (7, 17) =>
(3, 9) to (3, 41) =>
(6, 9) to (6, 31)"#
    );
}

#[test]
fn test_custom_jsx_pragma() {
    let result = print_order_test(
        Some("createMikesCoolElement"),
        false,
        r#"
<FirstElement />
function time_to_create_some_elements_bro() {
  return <SecondElement />
}
function createMikesCoolElement() { return 42 };
import * as React from 'react';
"#,
    );
    assert_eq!(
        result,
        r#"(6, 9) to (6, 31) =>
(3, 9) to (3, 41) =>
(7, 12) to (7, 17)"#
    );
}

#[test]
fn test_jsx_pragma_member_expr() {
    let result = print_order_test(
        Some("Test.f"),
        false,
        r#"
var x = <Component />
function Component() {}
import * as React from 'react';
"#,
    );
    assert_eq!(
        result,
        r#"(3, 9) to (3, 18) =>
(2, 4) to (2, 5) =>
(4, 12) to (4, 17)"#
    );
}

#[test]
fn test_automatic_react_runtime() {
    let result = print_order_test(
        None,
        true,
        r#"
<FirstElement />
function time_to_create_some_elements_bro() {
  return <SecondElement />
}
function createMikesCoolElement() { return 42 };
import * as React from 'react';
"#,
    );
    assert_eq!(
        result,
        r#"(3, 9) to (3, 41) =>
(6, 9) to (6, 31) =>
(7, 12) to (7, 17)"#
    );
}

#[test]
fn test_fully_annotated_function_expr_and_arrow_self_recursive() {
    let result = print_order_test(
        None,
        false,
        r#"
const foo = (): number => foo();
const bar = function (): number { return bar() };
const baz = function _(): number { return baz() };
"#,
    );
    assert_eq!(
        result,
        r#"(2, 6) to (2, 9) =>
(3, 6) to (3, 9) =>
(3, 12) to (3, 48) =>
(4, 6) to (4, 9) =>
(4, 21) to (4, 22)"#
    );
}

#[test]
fn test_return_annot() {
    let result = print_order_test(
        None,
        false,
        r#"
const S = ()=> {
  const x = useStyle();
};

type Styles = 1

const useStyle = (): Styles => { return 1; }
"#,
    );
    assert_eq!(
        result,
        r#"(2, 6) to (2, 7) =>
(6, 5) to (6, 11) =>
(8, 6) to (8, 14) =>
(3, 8) to (3, 9)"#
    );
}

#[test]
fn test_provider_refi() {
    let result = print_order_test(
        None,
        false,
        r#"
declare var rule: {title_label: string};
const titlesAdlabels = [];
rule.title_label = "a";
titlesAdlabels.push(rule.title_label);
"#,
    );
    assert_eq!(
        result,
        r#"(2, 12) to (2, 16) =>
(4, 0) to (4, 16) =>
(5, 20) to (5, 36) (Env_api.Make.ArrayProviderLoc) =>
(3, 6) to (3, 20)"#
    );
}

#[test]
fn test_logic_op_assign_repeat() {
    let result = print_order_test(
        None,
        false,
        r#"
function member_op_assignment_refinement_ok(o: {p: ?number}) {
  o.p &&= 3;
  o.p &&= 3;
  o.p &&= 3;
}
"#,
    );
    assert_eq!(
        result,
        r#"(2, 44) to (2, 45) =>
(2, 9) to (2, 43) =>
(3, 2) to (3, 5) =>
(4, 2) to (4, 5) =>
(5, 2) to (5, 5)"#
    );
}

#[test]
fn test_logic_op_assign_repeat_2() {
    let result = print_order_test(
        None,
        false,
        r#"
declare var values: mixed;

if (values.b === values.a) {
}
"#,
    );
    assert_eq!(
        result,
        r#"(2, 12) to (2, 18) =>
(4, 17) to (4, 25) (Env_api.Make.ExpressionLoc)"#
    );
}

#[test]
fn test_def_class() {
    let result = print_order_test(
        None,
        false,
        r#"
class A {
  B(defaultValue: boolean = false): void {}
}
"#,
    );
    assert_eq!(
        result,
        r#"legal scc: (((2, 6) to (2, 7)); ((3, 4) to (3, 16)))"#
    );
}

#[test]
fn test_def_class_fn_def() {
    let result = print_order_test(
        None,
        false,
        r#"
class A {
  B<T>(
    localize: mixed = (name: T): string => f(name),
   ) { }
}
declare var f: any;
"#,
    );
    assert_eq!(
        result,
        r#"legal scc: (((2, 6) to (2, 7)); ((3, 4) to (3, 5)); ((4, 4) to (4, 12))) =>
(4, 23) to (4, 27) =>
(7, 12) to (7, 13) =>
(4, 45) to (4, 49) (Env_api.Make.ExpressionLoc)"#
    );
}

#[test]
fn test_type_param_forward_bound_order() {
    let result = print_order_test(
        None,
        false,
        r#"
type F = <A extends B, B extends C, C extends string>() => void;
"#,
    );
    assert_eq!(
        result,
        r#"(2, 36) to (2, 37) =>
(2, 23) to (2, 24) =>
(2, 10) to (2, 11) =>
(2, 5) to (2, 6)"#
    );
}

#[test]
fn test_type_param_bound_cycle_order() {
    let result = print_order_test(
        None,
        false,
        r#"
type F = <A extends B, B extends A>() => void;
"#,
    );
    assert_eq!(
        result,
        r#"legal scc: (((2, 10) to (2, 11)); ((2, 23) to (2, 24))) =>
(2, 5) to (2, 6)"#
    );
}

#[test]
fn test_statics_cycle() {
    let result = print_order_test(
        None,
        false,
        r#"
function Dialog(): void { };

Dialog.Prop = function(): void {
    return (Dialog: any)
}
"#,
    );
    assert_eq!(
        result,
        r#"(4, 14) to (6, 1) =>
(2, 9) to (2, 15) =>
(4, 0) to (4, 11)"#
    );
}

#[test]
fn test_callee_hint_type_args() {
    let result = print_order_test(
        None,
        false,
        r#"
generic_fn<annot>((s) => s);
type annot = string;
"#,
    );
    assert_eq!(
        result,
        r#"(3, 5) to (3, 10) =>
(2, 19) to (2, 20) =>
(2, 18) to (2, 26) (Env_api.Make.ExpressionLoc)"#
    );
}

#[test]
fn test_callee_hint_cycle() {
    let result = print_order_test(
        None,
        false,
        r#"
g(function h() { return f() })
"#,
    );
    assert_eq!(
        result,
        r#"(2, 11) to (2, 12) =>
(2, 2) to (2, 29) (Env_api.Make.ExpressionLoc)"#
    );
}

#[test]
fn test_callee_hint_cycle_2() {
    let result = print_order_test(
        None,
        false,
        r#"
g(function () { return f() })
"#,
    );
    assert_eq!(
        result,
        r#"(2, 2) to (2, 28) =>
(2, 2) to (2, 28) (Env_api.Make.ExpressionLoc)"#
    );
}

#[test]
fn test_instantiate_hint_cycle() {
    let result = print_order_test(
        None,
        false,
        r#"
import * as React from 'react';
<F attr={function () { return f() }} />
"#,
    );
    assert_eq!(
        result,
        r#"(2, 12) to (2, 17) =>
(3, 9) to (3, 35)"#
    );
}

#[test]
fn test_synthesis_depends_on_annot() {
    let result = print_order_test(
        None,
        false,
        r#"
poly((foo: a1, bar): a2 => {})
type a1 = string;
type a2 = number;
"#,
    );
    assert_eq!(
        result,
        r#"(3, 5) to (3, 7) =>
(2, 6) to (2, 9) =>
(4, 5) to (4, 7) =>
(2, 15) to (2, 18) =>
(2, 5) to (2, 29) (Env_api.Make.ExpressionLoc)"#
    );
}

#[test]
fn test_fwd_ref_provider() {
    let result = print_order_test(
        None,
        false,
        r#"
let cc;
function foo() {
    cc = 1;
    return 42;
}
cc = null;
"#,
    );
    assert_eq!(
        result,
        r#"(4, 4) to (4, 6) =>
(7, 0) to (7, 2) =>
(3, 9) to (3, 12)"#
    );
}

#[test]
fn test_declare_function_overload_read_by_typeof() {
    let result = print_order_test(
        None,
        false,
        r#"
type T = typeof foo;
declare function foo(): void;
declare function foo(): void;
"#,
    );
    assert_eq!(
        result,
        r#"(3, 17) to (3, 20) =>
(4, 17) to (4, 20) =>
(2, 5) to (2, 6)"#
    );
}

#[test]
fn test_order_synthesizable_obj_read() {
    let result = print_order_test(
        None,
        false,
        r#"
declare var pair: { values: Array<string> };

if (pair.values.length !== 0) {
  const obj = {
    an_object_prop: pair.values,
  }
}

"#,
    );
    assert_eq!(
        result,
        r#"(2, 12) to (2, 16) =>
(4, 4) to (4, 15) =>
(4, 4) to (4, 22) =>
(4, 27) to (4, 28) (Env_api.Make.ExpressionLoc) =>
(5, 8) to (5, 11) =>
(6, 20) to (6, 31) (Env_api.Make.ExpressionLoc)"#
    );
}

#[test]
fn test_component_ordering() {
    let result = print_order_test(
        None,
        false,
        r#"
let x;
component F(param) {
  x = param;
}
x();
"#,
    );
    assert_eq!(
        result,
        r#"(3, 12) to (3, 17) =>
(3, 10) to (3, 11) =>
(4, 2) to (4, 3)"#
    );
}

#[test]
fn test_component_ordering_inc() {
    let result = print_order_test(
        None,
        false,
        r#"
let x;
var y;
component F() {
  y = x; return 42;
}
x++;
"#,
    );
    assert_eq!(
        result,
        r#"(4, 10) to (4, 11) =>
(7, 0) to (7, 1) =>
(5, 2) to (5, 3)"#
    );
}

#[test]
fn test_illegal_contextual_with_reference_to_param_in_return_annot() {
    let result = print_order_test(
        None,
        false,
        r#"
declare var f: any;
f((x): typeof x => x);
"#,
    );
    assert_eq!(
        result,
        r#"(2, 12) to (2, 13) =>
illegal self-cycle ((3, 3) to (3, 4)) =>
(3, 2) to (3, 20) (Env_api.Make.ExpressionLoc)"#
    );
}

#[test]
fn test_contextual_type_guard() {
    let result = print_order_test(
        None,
        false,
        r#"
declare var f: any;
f((x): x is number => true);
"#,
    );
    assert_eq!(
        result,
        r#"(2, 12) to (2, 13) =>
(3, 3) to (3, 4) =>
(3, 2) to (3, 26) (Env_api.Make.ExpressionLoc)"#
    );
}

#[test]
fn test_declare_component_ordering() {
    let result = print_order_test(
        None,
        false,
        r#"
declare component F(x: T);
const w = 42;
type T = typeof w;
"#,
    );
    assert_eq!(
        result,
        r#"(3, 6) to (3, 7) =>
(4, 5) to (4, 6) =>
(2, 18) to (2, 19)"#
    );
}

#[test]
fn test_match_pattern_binding() {
    let result = print_init_test(
        r#"
(match (x) {
  const a => a,
});
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 6) => val (2, 8) to (2, 9);
  (3, 8) to (3, 9) => match root for case at (3, 2) to (3, 2);
  (3, 2) to (3, 9) => match pattern (3, 2) to (3, 9)
]"#
    );
}

#[test]
fn test_match_pattern_tuple() {
    let result = print_init_test(
        r#"
(match (x) {
  [const a] => a,
});
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 6) => val (2, 8) to (2, 9);
  (3, 9) to (3, 10) => (match root for case at (3, 2) to (3, 2))[0];
  (3, 2) to (3, 11) => match root for case at (3, 2) to (3, 2);
  (3, 2) to (3, 11) => match pattern (3, 2) to (3, 11)
]"#
    );
}

#[test]
fn test_match_pattern_tuple_rest() {
    let result = print_init_test(
        r#"
(match (x) {
  [_, ...const b] => b,
});
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 6) => val (2, 8) to (2, 9);
  (3, 15) to (3, 16) => (match root for case at (3, 2) to (3, 2))[...];
  (3, 2) to (3, 17) => match root for case at (3, 2) to (3, 2);
  (3, 3) to (3, 4) => (match root for case at (3, 2) to (3, 2))[0];
  (3, 2) to (3, 17) => match pattern (3, 2) to (3, 17)
]"#
    );
}

#[test]
fn test_match_pattern_object() {
    let result = print_init_test(
        r#"
(match (x) {
  {foo: const a} => a,
});
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 6) => val (2, 8) to (2, 9);
  (3, 14) to (3, 15) => (match root for case at (3, 2) to (3, 2)).foo;
  (3, 2) to (3, 16) => match root for case at (3, 2) to (3, 2);
  (3, 2) to (3, 16) => match pattern (3, 2) to (3, 16)
]"#
    );
}

#[test]
fn test_match_pattern_object_shorthand() {
    let result = print_init_test(
        r#"
(match (x) {
  {const foo} => foo,
});
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 6) => val (2, 8) to (2, 9);
  (3, 9) to (3, 12) => (match root for case at (3, 2) to (3, 2)).foo;
  (3, 2) to (3, 13) => match root for case at (3, 2) to (3, 2);
  (3, 2) to (3, 13) => match pattern (3, 2) to (3, 13)
]"#
    );
}

#[test]
fn test_match_pattern_object_rest() {
    let result = print_init_test(
        r#"
(match (x) {
  {foo: _, ...const rest} => rest,
});
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 6) => val (2, 8) to (2, 9);
  (3, 20) to (3, 24) => (match root for case at (3, 2) to (3, 2)){ ... };
  (3, 2) to (3, 25) => match root for case at (3, 2) to (3, 2);
  (3, 8) to (3, 9) => (match root for case at (3, 2) to (3, 2)).foo;
  (3, 2) to (3, 25) => match pattern (3, 2) to (3, 25)
]"#
    );
}

#[test]
fn test_match_pattern_as() {
    let result = print_init_test(
        r#"
(match (x) {
  {foo: 1 as a} => a,
});
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 6) => val (2, 8) to (2, 9);
  (3, 13) to (3, 14) => (match root for case at (3, 2) to (3, 2)).foo;
  (3, 2) to (3, 15) => match root for case at (3, 2) to (3, 2);
  (3, 8) to (3, 9) => (match root for case at (3, 2) to (3, 2)).foo;
  (3, 2) to (3, 15) => match pattern (3, 2) to (3, 15);
  (3, 8) to (3, 9) => exp (3, 8) to (3, 9) (hint = [])
]"#
    );
}

#[test]
fn test_match_pattern_nested() {
    let result = print_init_test(
        r#"
(match (x) {
  {foo: [_, _, {bar: const x}]} => x,
});
  "#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 6) => val (2, 8) to (2, 9);
  (3, 27) to (3, 28) => (((match root for case at (3, 2) to (3, 2)).foo)[2]).bar;
  (3, 2) to (3, 31) => match root for case at (3, 2) to (3, 2);
  (3, 8) to (3, 30) => (match root for case at (3, 2) to (3, 2)).foo;
  (3, 9) to (3, 10) => ((match root for case at (3, 2) to (3, 2)).foo)[0];
  (3, 12) to (3, 13) => ((match root for case at (3, 2) to (3, 2)).foo)[1];
  (3, 15) to (3, 29) => ((match root for case at (3, 2) to (3, 2)).foo)[2];
  (3, 2) to (3, 31) => match pattern (3, 2) to (3, 31)
]"#
    );
}

#[test]
fn test_match_pattern_order() {
    let result = print_order_test(
        None,
        false,
        r#"
(match (x) {
  [const a] => a,
  [const b] => b,
});
  "#,
    );
    assert_eq!(
        result,
        r#"(2, 1) to (2, 6) =>
(3, 2) to (3, 11) (Env_api.Make.PatternLoc) =>
(3, 9) to (3, 10) =>
(3, 2) to (3, 11) (Env_api.Make.MatchCasePatternLoc) =>
(4, 2) to (4, 11) (Env_api.Make.PatternLoc) =>
(4, 9) to (4, 10) =>
(4, 2) to (4, 11) (Env_api.Make.MatchCasePatternLoc)"#
    );
}
