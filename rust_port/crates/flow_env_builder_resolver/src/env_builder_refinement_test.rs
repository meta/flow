/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;

use flow_aloc::ALoc;
use flow_analysis::test_utils::parse;
use flow_analysis::test_utils::parse_with_alocs;
use flow_common::options::JsxMode;
use flow_common::options::ReactRuntime;
use flow_common::reason::string_of_desc;
use flow_common::refinement_invalidation::RefinementInvalidation;
use flow_common::refinement_invalidation::string_of_reason;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_env_builder::assertion_call_target;
use flow_env_builder::assertion_call_target::AssertionInfo;
use flow_env_builder::assertion_call_target::AssertionKind;
use flow_env_builder::assertion_call_target::AssertionTarget;
use flow_env_builder::env_api::Refinement;
use flow_env_builder::env_api::Values;
use flow_env_builder::env_api::WriteLoc;
use flow_env_builder::env_api::show_refinement_kind_without_locs;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::FileKeyInner;
use flow_parser::loc_sig::LocSig;
use flow_parser::parse_jsx_pragma_expression;
use flow_parser_utils::file_sig::FileSig;
use flow_parser_utils::file_sig::FileSigOptions;

use crate::dependency_sigs::Context;
use crate::dependency_sigs::Flow;
use crate::name_resolver;

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

struct TestFlow;

impl Flow for TestFlow {
    type Cx = TestCx;

    fn add_output(
        _cx: &mut Self::Cx,
        _error: flow_typing_errors::error_message::ErrorMessage<ALoc>,
    ) {
    }
}

fn print_values<F>(
    refinement_of_id: F,
) -> impl Fn(&Values<ALoc>, &FlowOrdMap<ALoc, RefinementInvalidation>) -> String
where
    F: Fn(i32) -> Refinement<ALoc>,
{
    fn print_value<G>(refinement_of_id: &G, write_loc: &WriteLoc<ALoc>) -> String
    where
        G: Fn(i32) -> Refinement<ALoc>,
    {
        match write_loc {
            WriteLoc::Uninitialized(_) => "(uninitialized)".to_string(),
            WriteLoc::Undeclared(_, _) => "(undeclared)".to_string(),
            WriteLoc::Projection(l) => format!("projection at {}", l.debug_to_string(false)),
            WriteLoc::Write(reason) => {
                let loc = reason.loc();
                let desc = reason.desc(true).map(string_of_desc).unwrap_or_default();
                format!("{}: ({})", loc.debug_to_string(false), desc)
            }
            WriteLoc::EmptyArray { reason, .. } => {
                let loc = reason.loc();
                let desc = reason.desc(true).map(string_of_desc).unwrap_or_default();
                format!("(empty array) {}: ({})", loc.debug_to_string(false), desc)
            }
            WriteLoc::IllegalWrite(reason) => {
                format!("illegal write at {}", reason.loc().debug_to_string(false))
            }
            WriteLoc::Refinement {
                refinement_id,
                writes,
                ..
            } => {
                let refinement = refinement_of_id(*refinement_id);
                let refinement_str = show_refinement_kind_without_locs(&refinement.kind);
                let writes_str: Vec<String> = writes
                    .iter()
                    .map(|w| print_value(refinement_of_id, w))
                    .collect();
                format!(
                    "{{refinement = {}; writes = {}}}",
                    refinement_str,
                    writes_str.join(",")
                )
            }
            WriteLoc::FunctionThis(_) => "This(function)".to_string(),
            WriteLoc::GlobalThis(_) => "This(global)".to_string(),
            WriteLoc::IllegalThis(_) => "This(illegal)".to_string(),
            WriteLoc::ClassInstanceThis(_) => "This(instance)".to_string(),
            WriteLoc::ClassStaticThis(_) => "This(static)".to_string(),
            WriteLoc::ClassInstanceSuper(_) => "Super(instance)".to_string(),
            WriteLoc::ClassStaticSuper(_) => "Super(static)".to_string(),
            WriteLoc::ModuleScoped(name) => format!("ModuleScoped {}", name),
            WriteLoc::Global(name) => format!("Global {}", name),
            WriteLoc::Unreachable(_) => "unreachable".to_string(),
            WriteLoc::Undefined(_) => "undefined".to_string(),
            WriteLoc::Number(_) => "number".to_string(),
            WriteLoc::DeclaredFunction(l) => {
                format!("declared function {}", l.debug_to_string(false))
            }
        }
    }

    let print_invalidation = |map: &RefinementInvalidation| -> String {
        map.iter()
            .map(|(loc, reason)| {
                let reason_str = string_of_reason(*reason);
                format!("{} at {}", reason_str, loc.debug_to_string(false))
            })
            .collect::<Vec<_>>()
            .join(",\n    ")
    };

    move |values: &Values<ALoc>, invalidations: &FlowOrdMap<ALoc, RefinementInvalidation>| {
        let mut strlist: Vec<String> = values
            .iter_ordinary()
            .map(|(read_loc, read_entry)| {
                let writes_str: Vec<String> = read_entry
                    .write_locs
                    .iter()
                    .map(|w| print_value(&refinement_of_id, w))
                    .collect();
                format!(
                    "{} => {{\n    {}\n  }}",
                    read_loc.debug_to_string(false),
                    writes_str.join(",\n    ")
                )
            })
            .collect();

        strlist.extend(invalidations.iter().map(|(read_loc, invalidation)| {
            format!(
                "{} => invalidated refinement by {{\n    {}\n  }}",
                read_loc.debug_to_string(false),
                print_invalidation(invalidation)
            )
        }));
        format!("[\n  {}]", strlist.join(";\n  "))
    }
}

fn print_ssa_test(
    custom_jsx: Option<&str>,
    react_runtime_automatic: bool,
    lib: Option<bool>,
    exclude_syms: Option<FlowOrdSet<FlowSmolStr>>,
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
    let aloc_ast = parse_with_alocs(contents);
    let is_lib = lib.unwrap_or(false);
    let exclude = exclude_syms.unwrap_or_default();
    let file_sig = FileSig::from_program(&cx.file(), &parse(contents), &FileSigOptions::default());
    let result =
        name_resolver::program::<TestCx, TestFlow>(&mut cx, is_lib, exclude, &file_sig, &aloc_ast);
    print_values(|id| result.refinement_of_id.get(id))(
        &result.env_values,
        &result.env_refinement_invalidation_info,
    )
}

/// Models a proven `asserts condition` callee named `invariant`.
fn print_ssa_assertion_test(contents: &str) -> String {
    let mut cx = TestCx::default();
    let ast = parse_with_alocs(contents);
    let file_sig = FileSig::from_program(&cx.file(), &parse(contents), &FileSigOptions::default());
    let (loc, scopes, ssa_values, unbound_names, providers) =
        name_resolver::prepass(&cx, &file_sig, &ast);
    let assertion_name = FlowSmolStr::new("invariant");
    let assertion_calls =
        assertion_call_target::collect(&ast, &scopes, &providers, BTreeMap::new())
            .into_iter()
            .filter(|target| target.root_name == assertion_name && target.property_path.is_empty())
            .map(|target| {
                (
                    target.callee_loc,
                    AssertionInfo {
                        target: AssertionTarget::Parameter(0),
                        kind: AssertionKind::Bare,
                    },
                )
            })
            .collect();
    let (_, result) = name_resolver::walk_with_prepass::<TestCx, TestFlow>(
        &mut cx,
        false,
        FlowOrdSet::new(),
        loc,
        scopes,
        ssa_values,
        unbound_names,
        providers,
        assertion_calls,
        &file_sig,
        &ast,
        &ast,
    );
    print_values(|id| result.refinement_of_id.get(id))(
        &result.env_values,
        &result.env_refinement_invalidation_info,
    )
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn test_conj_refinement_empty() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function foo(e: Error) {
  if (e instanceof Error && true) {}
  else { e }
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 16) to (2, 21) => {
    Global Error
  };
  (3, 6) to (3, 7) => {
    (2, 13) to (2, 14): (`e`)
  };
  (3, 19) to (3, 24) => {
    Global Error
  };
  (4, 9) to (4, 10) => {
    (2, 13) to (2, 14): (`e`)
  }]"#
    );
}

#[test]
fn test_logical_expr() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = null;
let y = null;
(x && (y = x)) + x"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 1) to (3, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (3, 11) to (3, 12) => {
    {refinement = Truthy; writes = (1, 4) to (1, 5): (`x`)}
  };
  (3, 17) to (3, 18) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_logical_expr_successive() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = null;
x && (x && x)"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 0) to (2, 1) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 6) to (2, 7) => {
    {refinement = Truthy; writes = (1, 4) to (1, 5): (`x`)}
  };
  (2, 11) to (2, 12) => {
    {refinement = Truthy; writes = {refinement = Truthy; writes = (1, 4) to (1, 5): (`x`)}}
  }]"#
    );
}

#[test]
fn test_logical_or() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = null;
x || x"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 0) to (2, 1) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 5) to (2, 6) => {
    {refinement = Not (Truthy); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_logical_nc_and() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = null;
(x ?? x) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 6) to (2, 7) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  };
  (2, 12) to (2, 13) => {
    {refinement = Or (And (Not (Maybe), Truthy), Truthy); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_logical_nc_no_key() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = null;
((x != null) ?? x) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 2) to (2, 3) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 16) to (2, 17) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 22) to (2, 23) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_logical_nested_right() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = null;
x || (x || x)"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 0) to (2, 1) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 6) to (2, 7) => {
    {refinement = Not (Truthy); writes = (1, 4) to (1, 5): (`x`)}
  };
  (2, 11) to (2, 12) => {
    {refinement = Not (Truthy); writes = {refinement = Not (Truthy); writes = (1, 4) to (1, 5): (`x`)}}
  }]"#
    );
}

#[test]
fn test_logical_nested() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = null;
(x || (x != null)) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 7) to (2, 8) => {
    {refinement = Not (Truthy); writes = (1, 4) to (1, 5): (`x`)}
  };
  (2, 22) to (2, 23) => {
    {refinement = Or (Truthy, Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_logical_nested2() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = null;
(x && x) || (x && x)"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 6) to (2, 7) => {
    {refinement = Truthy; writes = (1, 4) to (1, 5): (`x`)}
  };
  (2, 13) to (2, 14) => {
    {refinement = Or (Not (Truthy), Not (Truthy)); writes = (1, 4) to (1, 5): (`x`)}
  };
  (2, 18) to (2, 19) => {
    {refinement = Truthy; writes = {refinement = Or (Not (Truthy), Not (Truthy)); writes = (1, 4) to (1, 5): (`x`)}}
  }]"#
    );
}

#[test]
fn test_logical_assignment_and() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = null;
x &&= x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 0) to (2, 1) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 6) to (2, 7) => {
    {refinement = Truthy; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_logical_assignment_or() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = null;
x ||= x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 0) to (2, 1) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 6) to (2, 7) => {
    {refinement = Not (Truthy); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_logical_assignment_nullish() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = null;
x ??= x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 0) to (2, 1) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 6) to (2, 7) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_assignment_truthy() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = null;
(x = null) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 14) to (2, 15) => {
    {refinement = Truthy; writes = (2, 1) to (2, 2): (`x`)}
  }]"#
    );
}

#[test]
fn test_eq_null() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = null;
(x == null) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 15) to (2, 16) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_neq_null() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = null;
(x != null) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 15) to (2, 16) => {
    {refinement = Not (Maybe); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_strict_eq_null() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = null;
(x === null) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 16) to (2, 17) => {
    {refinement = Null; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_strict_neq_null() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = null;
(x !== null) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 16) to (2, 17) => {
    {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_strict_eq_null_sentinel() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if (o.err === null) {
  o
} else {
  o;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => {
    Global o
  };
  (3, 2) to (3, 3) => {
    {refinement = SentinelR err; writes = Global o}
  };
  (5, 2) to (5, 3) => {
    {refinement = Not (SentinelR err); writes = Global o}
  }]"#
    );
}

#[test]
fn test_strict_neq_null_sentinel() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if (o.err !== null) {
  o
} else {
  o;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => {
    Global o
  };
  (3, 2) to (3, 3) => {
    {refinement = Not (SentinelR err); writes = Global o}
  };
  (5, 2) to (5, 3) => {
    {refinement = SentinelR err; writes = Global o}
  }]"#
    );
}

#[test]
fn test_eq_undefined() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x == undefined) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 6) to (2, 15) => {
    Global undefined
  };
  (2, 20) to (2, 21) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_neq_undefined() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x != undefined) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 6) to (2, 15) => {
    Global undefined
  };
  (2, 20) to (2, 21) => {
    {refinement = Not (Maybe); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_strict_eq_undefined() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x === undefined) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 7) to (2, 16) => {
    Global undefined
  };
  (2, 21) to (2, 22) => {
    {refinement = Not (Not (Undefined)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_strict_neq_undefined() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x !== undefined) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 7) to (2, 16) => {
    Global undefined
  };
  (2, 21) to (2, 22) => {
    {refinement = Not (Undefined); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_strict_eq_undefined_sentinel() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if (o.err === undefined) {
  o
} else {
  o;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => {
    Global o
  };
  (2, 14) to (2, 23) => {
    Global undefined
  };
  (3, 2) to (3, 3) => {
    {refinement = Not (Not (SentinelR err)); writes = Global o}
  };
  (5, 2) to (5, 3) => {
    {refinement = Not (SentinelR err); writes = Global o}
  }]"#
    );
}

#[test]
fn test_strict_neq_undefined_sentinel() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if (o.err !== undefined) {
  o
} else {
  o;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => {
    Global o
  };
  (2, 14) to (2, 23) => {
    Global undefined
  };
  (3, 2) to (3, 3) => {
    {refinement = Not (SentinelR err); writes = Global o}
  };
  (5, 2) to (5, 3) => {
    {refinement = Not (Not (SentinelR err)); writes = Global o}
  }]"#
    );
}

#[test]
fn test_undefined_already_bound() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let undefined = 3;
let x = null;
(x !== undefined) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 1) to (3, 2) => {
    (2, 4) to (2, 5): (`x`)
  };
  (3, 7) to (3, 16) => {
    (1, 4) to (1, 13): (`undefined`)
  };
  (3, 21) to (3, 22) => {
    (2, 4) to (2, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_eq_void() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x == void 0) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 17) to (2, 18) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_neq_void() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x != void 0) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 17) to (2, 18) => {
    {refinement = Not (Maybe); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_strict_eq_void() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x === void 0) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 18) to (2, 19) => {
    {refinement = Not (Not (Undefined)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_strict_neq_void() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x !== void 0) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 18) to (2, 19) => {
    {refinement = Not (Undefined); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_instanceof() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x instanceof Object) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 14) to (2, 20) => {
    Global Object
  };
  (2, 25) to (2, 26) => {
    {refinement = instanceof; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_array_is_array() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(Array.isArray(x)) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 6) => {
    Global Array
  };
  (2, 15) to (2, 16) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 22) to (2, 23) => {
    {refinement = isArray; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_unary_negation() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(!Array.isArray(x)) && x;
!x && x;
!(x || x) && x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 2) to (2, 7) => {
    Global Array
  };
  (2, 16) to (2, 17) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 23) to (2, 24) => {
    {refinement = Not (isArray); writes = (1, 4) to (1, 5): (`x`)}
  };
  (3, 1) to (3, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (3, 6) to (3, 7) => {
    {refinement = Not (Truthy); writes = (1, 4) to (1, 5): (`x`)}
  };
  (4, 2) to (4, 3) => {
    (1, 4) to (1, 5): (`x`)
  };
  (4, 7) to (4, 8) => {
    {refinement = Not (Truthy); writes = (1, 4) to (1, 5): (`x`)}
  };
  (4, 13) to (4, 14) => {
    {refinement = And (Not (Truthy), Not (Truthy)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_typeof_bool() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x == "boolean") && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 27) to (2, 28) => {
    {refinement = bool; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_not_typeof_bool() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x != "boolean") && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 27) to (2, 28) => {
    {refinement = Not (bool); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_typeof_number() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x == "number") && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 26) to (2, 27) => {
    {refinement = number; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_not_typeof_number() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x != "number") && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 26) to (2, 27) => {
    {refinement = Not (number); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_typeof_function() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x == "function") && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 28) to (2, 29) => {
    {refinement = function; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_not_typeof_function() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x != "function") && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 28) to (2, 29) => {
    {refinement = Not (function); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_typeof_object() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x == "object") && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 26) to (2, 27) => {
    {refinement = object; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_not_typeof_object() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x != "object") && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 26) to (2, 27) => {
    {refinement = Not (object); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_typeof_string() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x == "string") && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 26) to (2, 27) => {
    {refinement = string; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_not_typeof_string() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x != "string") && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 26) to (2, 27) => {
    {refinement = Not (string); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_typeof_symbol() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x == "symbol") && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 26) to (2, 27) => {
    {refinement = symbol; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_not_typeof_symbol() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x != "symbol") && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 26) to (2, 27) => {
    {refinement = Not (symbol); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_typeof_bool_template() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x == `boolean`) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 27) to (2, 28) => {
    {refinement = bool; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_not_typeof_bool_template() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x != `boolean`) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 27) to (2, 28) => {
    {refinement = Not (bool); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_typeof_number_template() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x == `number`) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 26) to (2, 27) => {
    {refinement = number; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_not_typeof_number_template() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x != `number`) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 26) to (2, 27) => {
    {refinement = Not (number); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_typeof_function_template() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x == `function`) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 28) to (2, 29) => {
    {refinement = function; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_not_typeof_function_template() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x != `function`) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 28) to (2, 29) => {
    {refinement = Not (function); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_typeof_object_template() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x == `object`) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 26) to (2, 27) => {
    {refinement = object; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_not_typeof_object_template() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x != `object`) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 26) to (2, 27) => {
    {refinement = Not (object); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_typeof_string_template() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x == `string`) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 26) to (2, 27) => {
    {refinement = string; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_not_typeof_string_template() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x != `string`) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 26) to (2, 27) => {
    {refinement = Not (string); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_typeof_symbol_template() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x == `symbol`) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 26) to (2, 27) => {
    {refinement = symbol; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_not_typeof_symbol_template() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(typeof x != `symbol`) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 26) to (2, 27) => {
    {refinement = Not (symbol); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_singleton_bool() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x === true) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 16) to (2, 17) => {
    {refinement = true; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_singleton_str() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x === "str") && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 17) to (2, 18) => {
    {refinement = str; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_singleton_str_template() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x === `str`) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 17) to (2, 18) => {
    {refinement = str; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_singleton_num() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x === 3) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 13) to (2, 14) => {
    {refinement = 3; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_singleton_num_neg() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x === -3) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 14) to (2, 15) => {
    {refinement = -3; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_sentinel_lit() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x.foo === 3) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 17) to (2, 18) => {
    {refinement = SentinelR foo; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_sentinel_lit_indexed() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x["foo"] === 3) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 20) to (2, 21) => {
    {refinement = SentinelR foo; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_sentinel_nonlit() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
let y = undefined;
(x.foo === y) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 17) => {
    Global undefined
  };
  (3, 1) to (3, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (3, 11) to (3, 12) => {
    (2, 4) to (2, 5): (`y`)
  };
  (3, 17) to (3, 18) => {
    {refinement = SentinelR foo; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_sentinel_nonlit_indexed() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
let y = undefined;
(x["foo"] === y) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 17) => {
    Global undefined
  };
  (3, 1) to (3, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (3, 14) to (3, 15) => {
    (2, 4) to (2, 5): (`y`)
  };
  (3, 20) to (3, 21) => {
    {refinement = SentinelR foo; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_refined_call_in_member_expressions() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
if (x.foo != null && x.foo.bar()) {}"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 4) to (2, 5) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 21) to (2, 22) => {
    {refinement = Not (PropNullishR foo); writes = (1, 4) to (1, 5): (`x`)}
  };
  (2, 21) to (2, 26) => {
    {refinement = Not (Maybe); writes = projection at (2, 4) to (2, 9)}
  }]"#
    );
}

#[test]
fn test_refined_call_in_unrefinable_member_expressions() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
if (x.foo != null && x.foo.bar()[0] === BAZ) {}"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 4) to (2, 5) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 21) to (2, 22) => {
    {refinement = Not (PropNullishR foo); writes = (1, 4) to (1, 5): (`x`)}
  };
  (2, 21) to (2, 26) => {
    {refinement = Not (Maybe); writes = projection at (2, 4) to (2, 9)}
  };
  (2, 40) to (2, 43) => {
    Global BAZ
  }]"#
    );
}

#[test]
fn test_optional_chain_lit() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x?.foo === 3) ? x : x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 17) to (2, 18) => {
    {refinement = And (And (Not (Maybe), Not (PropNullishR foo)), SentinelR foo); writes = (1, 4) to (1, 5): (`x`)}
  };
  (2, 21) to (2, 22) => {
    {refinement = Or (Or (Not (Not (Maybe)), Not (Not (PropNullishR foo))), Not (SentinelR foo)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_optional_chain_not_lit() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x?.foo !== 3) ? x : x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 17) to (2, 18) => {
    {refinement = Or (Or (Not (Not (Maybe)), Not (Not (PropNullishR foo))), Not (SentinelR foo)); writes = (1, 4) to (1, 5): (`x`)}
  };
  (2, 21) to (2, 22) => {
    {refinement = And (And (Not (Maybe), Not (PropNullishR foo)), SentinelR foo); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_optional_chain_bad_prop_truthy_r() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
  function withResult1(result: {ok: true} | {ok: false}): string {
    if(result?.ok === false) {
        return result as empty; // bad: result is not empty
    }
    return "Hello"
  }
  function withResult2(result: {ok: true} | {ok: void}): string {
    if(result?.ok === undefined) {
        return result as empty; // good error: result not refined to empty
    }
    return "Hello"
  }
  function withResult3(result: {ok: true} | {ok: null}): string {
    if(result?.ok === null) {
        return result as empty; // bad: result is not empty
    }
    return "Hello"
  }
  function withResult4(result: {ok: true} | {ok: null}): string {
    if(result?.ok == null) {
        return result as empty; // good error: result not refined to empty
    }
    return "Hello"
  }
  function withResult5(result: {ok: true} | {ok: null}): string {
    if(result?.ok == undefined) {
        return result as empty; // good error: result not refined to empty
    }
    return "Hello"
  }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 7) to (3, 13) => {
    (2, 23) to (2, 29): (`result`)
  };
  (4, 15) to (4, 21) => {
    {refinement = And (And (Not (Maybe), Not (PropNullishR ok)), SentinelR ok); writes = (2, 23) to (2, 29): (`result`)}
  };
  (9, 7) to (9, 13) => {
    (8, 23) to (8, 29): (`result`)
  };
  (9, 22) to (9, 31) => {
    Global undefined
  };
  (10, 15) to (10, 21) => {
    {refinement = Or (Or (Not (Not (Maybe)), Not (PropNonVoidR ok)), Not (Not (SentinelR ok))); writes = (8, 23) to (8, 29): (`result`)}
  };
  (15, 7) to (15, 13) => {
    (14, 23) to (14, 29): (`result`)
  };
  (16, 15) to (16, 21) => {
    {refinement = And (And (Not (Maybe), PropIsExactlyNullR ok), SentinelR ok); writes = (14, 23) to (14, 29): (`result`)}
  };
  (21, 7) to (21, 13) => {
    (20, 23) to (20, 29): (`result`)
  };
  (22, 15) to (22, 21) => {
    {refinement = Or (Or (Not (Not (Maybe)), Not (Not (PropNullishR ok))), Not (Not (PropNullishR ok))); writes = (20, 23) to (20, 29): (`result`)}
  };
  (27, 7) to (27, 13) => {
    (26, 23) to (26, 29): (`result`)
  };
  (27, 21) to (27, 30) => {
    Global undefined
  };
  (28, 15) to (28, 21) => {
    {refinement = Or (Or (Not (Not (Maybe)), Not (Not (PropNullishR ok))), Not (Not (PropNullishR ok))); writes = (26, 23) to (26, 29): (`result`)}
  }]"#
    );
}

#[test]
fn test_optional_chain_member_base() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x.foo?.bar === 3) ? x : x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 21) to (2, 22) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 25) to (2, 26) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_optional_chain_with_call() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x?.foo().bar === 3) ? x : x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 23) to (2, 24) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 27) to (2, 28) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_optional_multiple_chains() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x?.foo?.bar.baz?.qux === 3) ? x : x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 31) to (2, 32) => {
    {refinement = And (Not (Maybe), Not (PropNullishR foo)); writes = (1, 4) to (1, 5): (`x`)}
  };
  (2, 35) to (2, 36) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_optional_base_call() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x?.().foo?.bar.baz?.qux === 3) ? x : x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 34) to (2, 35) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 38) to (2, 39) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_sentinel_standalone() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
x.foo && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 0) to (2, 1) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 9) to (2, 10) => {
    {refinement = PropTruthyR (foo); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_optional_chain_standalone() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
x?.foo && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 0) to (2, 1) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 10) to (2, 11) => {
    {refinement = And (Not (Maybe), PropTruthyR (foo)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_no_sentinel_in_non_strict() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
var x : {p:?string} = {p:"xxx"};
if (x.p != null) {
  alert("");
  x.p;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 4) to (3, 5) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 2) to (4, 7) => {
    Global alert
  };
  (5, 2) to (5, 3) => {
    {refinement = Not (PropNullishR p); writes = (2, 4) to (2, 5): (`x`)}
  };
  (5, 2) to (5, 5) => invalidated refinement by {
    function call at (4, 2) to (4, 11)
  }]"#
    );
}

#[test]
fn test_conditional_expression() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
(x ? x: x) && x"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 1) to (2, 2) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 5) to (2, 6) => {
    {refinement = Truthy; writes = (1, 4) to (1, 5): (`x`)}
  };
  (2, 8) to (2, 9) => {
    {refinement = Not (Truthy); writes = (1, 4) to (1, 5): (`x`)}
  };
  (2, 14) to (2, 15) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_if_else_statement() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
if (x) {
  x;
} else {
  x;
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 4) to (2, 5) => {
    (1, 4) to (1, 5): (`x`)
  };
  (3, 2) to (3, 3) => {
    {refinement = Truthy; writes = (1, 4) to (1, 5): (`x`)}
  };
  (5, 2) to (5, 3) => {
    {refinement = Not (Truthy); writes = (1, 4) to (1, 5): (`x`)}
  };
  (7, 0) to (7, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_if_no_else_statement() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
if (x) {
  x;
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 4) to (2, 5) => {
    (1, 4) to (1, 5): (`x`)
  };
  (3, 2) to (3, 3) => {
    {refinement = Truthy; writes = (1, 4) to (1, 5): (`x`)}
  };
  (5, 0) to (5, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_if_no_else_statement_with_assignment_simple() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = 0;
if (true) {
  x = null;
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 0) to (5, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_if_no_else_statement_with_assignment_simple_with_annotation() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x: number = 0;
if (true) {
  x = null;
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 0) to (5, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_if_no_else_statement_with_assignment_uninitialized_with_annotation() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x: number;
if (true) {
  x = 1;
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 0) to (5, 1) => {
    (uninitialized),
    (3, 2) to (3, 3): (`x`)
  }]"#
    );
}

#[test]
fn test_if_no_else_statement_with_assignment_with_refinements() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
if (x !== null) {
  x = null;
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 4) to (2, 5) => {
    (1, 4) to (1, 5): (`x`)
  };
  (5, 0) to (5, 1) => {
    (3, 2) to (3, 3): (`x`),
    {refinement = Null; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_if_throw_else_statement() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
if (x) {
  throw 'error';
} else {
  x;
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 4) to (2, 5) => {
    (1, 4) to (1, 5): (`x`)
  };
  (5, 2) to (5, 3) => {
    {refinement = Not (Truthy); writes = (1, 4) to (1, 5): (`x`)}
  };
  (7, 0) to (7, 1) => {
    {refinement = Not (Truthy); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_if_else_throw_statement() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
if (x) {
  x;
} else {
  throw 'error';
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 4) to (2, 5) => {
    (1, 4) to (1, 5): (`x`)
  };
  (3, 2) to (3, 3) => {
    {refinement = Truthy; writes = (1, 4) to (1, 5): (`x`)}
  };
  (7, 0) to (7, 1) => {
    {refinement = Truthy; writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_if_return_else_statement() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"function f() {
  let x = undefined;
  if (x) {
    return;
  } else {
    x;
  }
  x;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 10) to (2, 19) => {
    Global undefined
  };
  (3, 6) to (3, 7) => {
    (2, 6) to (2, 7): (`x`)
  };
  (6, 4) to (6, 5) => {
    {refinement = Not (Truthy); writes = (2, 6) to (2, 7): (`x`)}
  };
  (8, 2) to (8, 3) => {
    {refinement = Not (Truthy); writes = (2, 6) to (2, 7): (`x`)}
  }]"#
    );
}

#[test]
fn test_if_else_return_statement() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"function f() {
  let x = undefined;
  if (x) {
    x;
  } else {
    return;
  }
  x;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 10) to (2, 19) => {
    Global undefined
  };
  (3, 6) to (3, 7) => {
    (2, 6) to (2, 7): (`x`)
  };
  (4, 4) to (4, 5) => {
    {refinement = Truthy; writes = (2, 6) to (2, 7): (`x`)}
  };
  (8, 2) to (8, 3) => {
    {refinement = Truthy; writes = (2, 6) to (2, 7): (`x`)}
  }]"#
    );
}

#[test]
fn test_nested_if_else_statement() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
if (x) {
  if (x === null) {
    throw 'error';
  }
  x;
} else {
  if (x === null) {
    x;
  } else {
    throw 'error';
  }
  x;
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 4) to (2, 5) => {
    (1, 4) to (1, 5): (`x`)
  };
  (3, 6) to (3, 7) => {
    {refinement = Truthy; writes = (1, 4) to (1, 5): (`x`)}
  };
  (6, 2) to (6, 3) => {
    {refinement = Not (Null); writes = {refinement = Truthy; writes = (1, 4) to (1, 5): (`x`)}}
  };
  (8, 6) to (8, 7) => {
    {refinement = Not (Truthy); writes = (1, 4) to (1, 5): (`x`)}
  };
  (9, 4) to (9, 5) => {
    {refinement = Null; writes = {refinement = Not (Truthy); writes = (1, 4) to (1, 5): (`x`)}}
  };
  (13, 2) to (13, 3) => {
    {refinement = Null; writes = {refinement = Not (Truthy); writes = (1, 4) to (1, 5): (`x`)}}
  };
  (15, 0) to (15, 1) => {
    {refinement = Not (Null); writes = {refinement = Truthy; writes = (1, 4) to (1, 5): (`x`)}},
    {refinement = Null; writes = {refinement = Not (Truthy); writes = (1, 4) to (1, 5): (`x`)}}
  }]"#
    );
}

#[test]
fn test_while() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
while (x != null) {
  x;
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 7) to (2, 8) => {
    (1, 4) to (1, 5): (`x`)
  };
  (3, 2) to (3, 3) => {
    {refinement = Not (Maybe); writes = (1, 4) to (1, 5): (`x`)}
  };
  (5, 0) to (5, 1) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_while_assign() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
declare var x: string;
while (x = x) { x }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 11) to (3, 12) => {
    (2, 12) to (2, 13): (`x`)
  };
  (3, 16) to (3, 17) => {
    {refinement = Truthy; writes = (3, 7) to (3, 8): (`x`)}
  }]"#
    );
}

#[test]
fn test_while_throw() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
while (x != null) {
  throw 'error';
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 7) to (2, 8) => {
    (1, 4) to (1, 5): (`x`)
  };
  (5, 0) to (5, 1) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_while_break_with_control_flow_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
let y = undefined;
while (x != null) {
  if (y == null) {
    break;
  }
  y;
}
y;
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 17) => {
    Global undefined
  };
  (3, 7) to (3, 8) => {
    (1, 4) to (1, 5): (`x`)
  };
  (4, 6) to (4, 7) => {
    (2, 4) to (2, 5): (`y`)
  };
  (7, 2) to (7, 3) => {
    {refinement = Not (Maybe); writes = (2, 4) to (2, 5): (`y`)}
  };
  (9, 0) to (9, 1) => {
    (2, 4) to (2, 5): (`y`)
  };
  (10, 0) to (10, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_while_with_runtime_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
let y = undefined;
while (x != null) {
  if (y == null) {
    x = 2;
  }
  y;
}
y;
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 17) => {
    Global undefined
  };
  (3, 7) to (3, 8) => {
    (1, 4) to (1, 5): (`x`)
  };
  (4, 6) to (4, 7) => {
    (2, 4) to (2, 5): (`y`)
  };
  (7, 2) to (7, 3) => {
    (2, 4) to (2, 5): (`y`)
  };
  (9, 0) to (9, 1) => {
    (2, 4) to (2, 5): (`y`)
  };
  (10, 0) to (10, 1) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_while_with_var_write() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
while (true) {
  var a = function() {}
}
a()"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 0) to (5, 1) => {
    (uninitialized),
    (3, 6) to (3, 7): (`a`)
  }]"#
    );
}

#[test]
fn test_while_continue() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
while (x != null) {
  continue;
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 7) to (2, 8) => {
    (1, 4) to (1, 5): (`x`)
  };
  (5, 0) to (5, 1) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_while_continue_with_control_flow_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
let y = undefined;
while (x != null) {
  if (y == null) {
    continue;
  }
  y;
}
y;
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 17) => {
    Global undefined
  };
  (3, 7) to (3, 8) => {
    (1, 4) to (1, 5): (`x`)
  };
  (4, 6) to (4, 7) => {
    (2, 4) to (2, 5): (`y`)
  };
  (7, 2) to (7, 3) => {
    {refinement = Not (Maybe); writes = (2, 4) to (2, 5): (`y`)}
  };
  (9, 0) to (9, 1) => {
    (2, 4) to (2, 5): (`y`)
  };
  (10, 0) to (10, 1) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_while_phi_node_refinement() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
while (x != null) {
  if (x === 3) {
    continue;
  }
  x;
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 7) to (2, 8) => {
    (1, 4) to (1, 5): (`x`)
  };
  (3, 6) to (3, 7) => {
    {refinement = Not (Maybe); writes = (1, 4) to (1, 5): (`x`)}
  };
  (6, 2) to (6, 3) => {
    {refinement = Not (3); writes = {refinement = Not (Maybe); writes = (1, 4) to (1, 5): (`x`)}}
  };
  (8, 0) to (8, 1) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_do_while() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
do {
  x;
} while (x != null);
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (3, 2) to (3, 3) => {
    (1, 4) to (1, 5): (`x`)
  };
  (4, 9) to (4, 10) => {
    (1, 4) to (1, 5): (`x`)
  };
  (5, 0) to (5, 1) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_do_while_break_with_control_flow_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
let y = undefined;
do {
  if (y == null) {
    break;
  }
  y;
} while (x != null);
y;
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 17) => {
    Global undefined
  };
  (4, 6) to (4, 7) => {
    (2, 4) to (2, 5): (`y`)
  };
  (7, 2) to (7, 3) => {
    {refinement = Not (Maybe); writes = (2, 4) to (2, 5): (`y`)}
  };
  (8, 9) to (8, 10) => {
    (1, 4) to (1, 5): (`x`)
  };
  (9, 0) to (9, 1) => {
    {refinement = Not (Maybe); writes = (2, 4) to (2, 5): (`y`)},
    {refinement = Not (Not (Maybe)); writes = (2, 4) to (2, 5): (`y`)}
  };
  (10, 0) to (10, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_do_while_with_runtime_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
let y = undefined;
do {
  if (y == null) {
    x = 2;
  }
  y;
} while (x != null);
y;
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 17) => {
    Global undefined
  };
  (4, 6) to (4, 7) => {
    (2, 4) to (2, 5): (`y`)
  };
  (7, 2) to (7, 3) => {
    (2, 4) to (2, 5): (`y`)
  };
  (8, 9) to (8, 10) => {
    (1, 4) to (1, 5): (`x`)
  };
  (9, 0) to (9, 1) => {
    (2, 4) to (2, 5): (`y`)
  };
  (10, 0) to (10, 1) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_do_while_continue() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
do {
  continue;
} while (x != null);
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (4, 9) to (4, 10) => {
    (1, 4) to (1, 5): (`x`)
  };
  (5, 0) to (5, 1) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_do_while_continue_with_control_flow_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
let y = undefined;
do {
  if (y == null) {
    continue;
  }
  y;
} while (x != null);
y;
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 17) => {
    Global undefined
  };
  (4, 6) to (4, 7) => {
    (2, 4) to (2, 5): (`y`)
  };
  (7, 2) to (7, 3) => {
    {refinement = Not (Maybe); writes = (2, 4) to (2, 5): (`y`)}
  };
  (8, 9) to (8, 10) => {
    (1, 4) to (1, 5): (`x`)
  };
  (9, 0) to (9, 1) => {
    {refinement = Not (Maybe); writes = (2, 4) to (2, 5): (`y`)},
    {refinement = Not (Not (Maybe)); writes = (2, 4) to (2, 5): (`y`)}
  };
  (10, 0) to (10, 1) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_do_while_phi_node_refinement() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
do {
  if (x === 3) {
    continue;
  }
  x;
} while (x != null);
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (3, 6) to (3, 7) => {
    (1, 4) to (1, 5): (`x`)
  };
  (6, 2) to (6, 3) => {
    {refinement = Not (3); writes = (1, 4) to (1, 5): (`x`)}
  };
  (7, 9) to (7, 10) => {
    {refinement = 3; writes = (1, 4) to (1, 5): (`x`)},
    {refinement = Not (3); writes = (1, 4) to (1, 5): (`x`)}
  };
  (8, 0) to (8, 1) => {
    {refinement = Not (Not (Maybe)); writes = {refinement = 3; writes = (1, 4) to (1, 5): (`x`)},{refinement = Not (3); writes = (1, 4) to (1, 5): (`x`)}}
  }]"#
    );
}

#[test]
fn test_for_no_init_no_update() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
for (;x != null;) {
  x;
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 6) to (2, 7) => {
    (1, 4) to (1, 5): (`x`)
  };
  (3, 2) to (3, 3) => {
    {refinement = Not (Maybe); writes = (1, 4) to (1, 5): (`x`)}
  };
  (5, 0) to (5, 1) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_for_no_init_no_update_throw() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
for (;x != null;) {
  throw 'error';
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 6) to (2, 7) => {
    (1, 4) to (1, 5): (`x`)
  };
  (5, 0) to (5, 1) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_for_no_init_no_update_break_with_control_flow_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
let y = undefined;
for (; x != null; ) {
  if (y == null) {
    break;
  }
  y;
}
y;
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 17) => {
    Global undefined
  };
  (3, 7) to (3, 8) => {
    (1, 4) to (1, 5): (`x`)
  };
  (4, 6) to (4, 7) => {
    (2, 4) to (2, 5): (`y`)
  };
  (7, 2) to (7, 3) => {
    {refinement = Not (Maybe); writes = (2, 4) to (2, 5): (`y`)}
  };
  (9, 0) to (9, 1) => {
    (2, 4) to (2, 5): (`y`)
  };
  (10, 0) to (10, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_for_no_init_no_update_with_runtime_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
let y = undefined;
for (;x != null;) {
  if (y == null) {
    x = 2;
  }
  y;
}
y;
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 17) => {
    Global undefined
  };
  (3, 6) to (3, 7) => {
    (1, 4) to (1, 5): (`x`)
  };
  (4, 6) to (4, 7) => {
    (2, 4) to (2, 5): (`y`)
  };
  (7, 2) to (7, 3) => {
    (2, 4) to (2, 5): (`y`)
  };
  (9, 0) to (9, 1) => {
    (2, 4) to (2, 5): (`y`)
  };
  (10, 0) to (10, 1) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_for_no_init_no_update_continue() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
for (; x != null; ) {
  continue;
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 7) to (2, 8) => {
    (1, 4) to (1, 5): (`x`)
  };
  (5, 0) to (5, 1) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_for_no_init_no_update_continue_with_control_flow_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
let y = undefined;
for (;x != null;) {
  if (y == null) {
    continue;
  }
  y;
}
y;
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 17) => {
    Global undefined
  };
  (3, 6) to (3, 7) => {
    (1, 4) to (1, 5): (`x`)
  };
  (4, 6) to (4, 7) => {
    (2, 4) to (2, 5): (`y`)
  };
  (7, 2) to (7, 3) => {
    {refinement = Not (Maybe); writes = (2, 4) to (2, 5): (`y`)}
  };
  (9, 0) to (9, 1) => {
    (2, 4) to (2, 5): (`y`)
  };
  (10, 0) to (10, 1) => {
    {refinement = Not (Not (Maybe)); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_for_no_init_no_update_phi_refinement() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
for (; x != null; ) {
  if (x === 3) {
    break;
  }
  x;
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 7) to (2, 8) => {
    (1, 4) to (1, 5): (`x`)
  };
  (3, 6) to (3, 7) => {
    {refinement = Not (Maybe); writes = (1, 4) to (1, 5): (`x`)}
  };
  (6, 2) to (6, 3) => {
    {refinement = Not (3); writes = {refinement = Not (Maybe); writes = (1, 4) to (1, 5): (`x`)}}
  };
  (8, 0) to (8, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_for_shadow() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
for (let x = null; x != null; x++) {
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 19) to (2, 20) => {
    (2, 9) to (2, 10): (`x`)
  };
  (2, 30) to (2, 31) => {
    {refinement = Not (Maybe); writes = (2, 9) to (2, 10): (`x`)}
  };
  (4, 0) to (4, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_for() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"for (let x = 3; x != null; x++) {
  x;
}"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 16) to (1, 17) => {
    (1, 9) to (1, 10): (`x`)
  };
  (1, 27) to (1, 28) => {
    {refinement = Not (Maybe); writes = (1, 9) to (1, 10): (`x`)}
  };
  (2, 2) to (2, 3) => {
    {refinement = Not (Maybe); writes = (1, 9) to (1, 10): (`x`)}
  }]"#
    );
}

#[test]
fn test_for_throw() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"for (let x = 3; x != null; x++) {
  throw 'error';
}"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 16) to (1, 17) => {
    (1, 9) to (1, 10): (`x`)
  };
  (1, 27) to (1, 28) => {
    unreachable
  }]"#
    );
}

#[test]
fn test_for_break_with_control_flow_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let y = undefined;
for (let x = 3; x != null; x++) {
  if (y == null) {
    break;
  }
  y;
}
y;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 16) to (2, 17) => {
    (2, 9) to (2, 10): (`x`)
  };
  (2, 27) to (2, 28) => {
    {refinement = Not (Maybe); writes = (2, 9) to (2, 10): (`x`)}
  };
  (3, 6) to (3, 7) => {
    (1, 4) to (1, 5): (`y`)
  };
  (6, 2) to (6, 3) => {
    {refinement = Not (Maybe); writes = (1, 4) to (1, 5): (`y`)}
  };
  (8, 0) to (8, 1) => {
    (1, 4) to (1, 5): (`y`)
  }]"#
    );
}

#[test]
fn test_for_with_runtime_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let y = undefined;
for (let x = 3; x != null; x++) {
  if (y == null) {
    x = 2;
  }
  y;
}
y;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 16) to (2, 17) => {
    (2, 9) to (2, 10): (`x`)
  };
  (2, 27) to (2, 28) => {
    (4, 4) to (4, 5): (`x`),
    {refinement = Not (Maybe); writes = (2, 9) to (2, 10): (`x`)}
  };
  (3, 6) to (3, 7) => {
    (1, 4) to (1, 5): (`y`)
  };
  (6, 2) to (6, 3) => {
    (1, 4) to (1, 5): (`y`)
  };
  (8, 0) to (8, 1) => {
    (1, 4) to (1, 5): (`y`)
  }]"#
    );
}

#[test]
fn test_for_continue() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"for (let x = 3; x != null; x++) {
  continue;
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 16) to (1, 17) => {
    (1, 9) to (1, 10): (`x`)
  };
  (1, 27) to (1, 28) => {
    {refinement = Not (Maybe); writes = (1, 9) to (1, 10): (`x`)}
  };
  (4, 0) to (4, 1) => {
    Global x
  }]"#
    );
}

#[test]
fn test_for_continue_with_control_flow_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let y = undefined;
for (let x = 3; x != null; x++) {
  if (y == null) {
    continue;
  }
  y;
}
y;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 16) to (2, 17) => {
    (2, 9) to (2, 10): (`x`)
  };
  (2, 27) to (2, 28) => {
    {refinement = Not (Maybe); writes = (2, 9) to (2, 10): (`x`)}
  };
  (3, 6) to (3, 7) => {
    (1, 4) to (1, 5): (`y`)
  };
  (6, 2) to (6, 3) => {
    {refinement = Not (Maybe); writes = (1, 4) to (1, 5): (`y`)}
  };
  (8, 0) to (8, 1) => {
    (1, 4) to (1, 5): (`y`)
  }]"#
    );
}

#[test]
fn test_no_havoc_before_write_seen() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"function f() { return 42 }
var x: number;
x;
x = 42;"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 0) to (3, 1) => {
    (uninitialized)
  }]"#
    );
}

#[test]
fn test_havoc_before_write_seen() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"function f() { return 42 }
f();
var x: number;
x;
x = 42;"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 0) to (2, 1) => {
    (1, 9) to (1, 10): (`f`)
  };
  (4, 0) to (4, 1) => {
    (uninitialized)
  }]"#
    );
}

#[test]
fn test_dont_havoc_to_uninit_in_function() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"function f() { return 42 }
function g() { return f() }"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 22) to (2, 23) => {
    (1, 9) to (1, 10): (`f`)
  }]"#
    );
}

#[test]
fn test_switch_decl() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"switch ('') { case '': const foo = ''; foo; };"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 39) to (1, 42) => {
    (1, 29) to (1, 32): (`foo`)
  }]"#
    );
}

#[test]
fn test_switch_weird_decl() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"switch ('') { case l: 0; break; case '': let l };"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 19) to (1, 20) => {
    (undeclared)
  }]"#
    );
}

#[test]
fn test_switch_shadow() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"function switch_scope(x) {
  switch (x) {
    default:
      let x;
      x = ""; // doesn't refine outer x
      x
  }
  x
}"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 10) to (2, 11) => {
    (1, 22) to (1, 23): (`x`)
  };
  (6, 6) to (6, 7) => {
    (5, 6) to (5, 7): (`x`)
  };
  (8, 2) to (8, 3) => {
    (1, 22) to (1, 23): (`x`)
  }]"#
    );
}

#[test]
fn test_switch_nested_block_shadow() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"function switch_scope() {
  switch ('foo') {
    case 'foo': {
      const bar = 3;
      break;
    }
  }
  bar;
  const {bar} = {};
  bar;
}"#,
    );
    assert_eq!(
        result,
        r#"[
  (8, 2) to (8, 5) => {
    (undeclared)
  };
  (10, 2) to (10, 5) => {
    (9, 9) to (9, 12): (`bar`)
  }]"#
    );
}

#[test]
fn test_for_nested_block_shadow() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"function for_scope() {
  for (;;) {
    const bar = 3;
    break;
  }
  bar;
  const {bar} = {};
  bar;
}"#,
    );
    assert_eq!(
        result,
        r#"[
  (6, 2) to (6, 5) => {
    (undeclared)
  };
  (8, 2) to (8, 5) => {
    (7, 9) to (7, 12): (`bar`)
  }]"#
    );
}

#[test]
fn test_for_in() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let stuff = {}
for (let thing in stuff) {
  thing
}"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 18) to (2, 23) => {
    (1, 4) to (1, 9): (`stuff`)
  };
  (3, 2) to (3, 7) => {
    (2, 9) to (2, 14): (`thing`)
  }]"#
    );
}

#[test]
fn test_for_in_reassign() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let stuff = {}
for (let thing in stuff) {
  thing;
  thing = 3;
}"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 18) to (2, 23) => {
    (1, 4) to (1, 9): (`stuff`)
  };
  (3, 2) to (3, 7) => {
    (2, 9) to (2, 14): (`thing`)
  }]"#
    );
}

#[test]
fn test_for_in_destructure() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let stuff = {}
for (let {thing} in stuff) {
  thing;
}"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 20) to (2, 25) => {
    (1, 4) to (1, 9): (`stuff`)
  };
  (3, 2) to (3, 7) => {
    (2, 10) to (2, 15): (`thing`)
  }]"#
    );
}

#[test]
fn test_for_in_destructure_reassign() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let stuff = {}
for (let {thing} in stuff) {
  thing;
  thing = 3;
}"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 20) to (2, 25) => {
    (1, 4) to (1, 9): (`stuff`)
  };
  (3, 2) to (3, 7) => {
    (2, 10) to (2, 15): (`thing`)
  }]"#
    );
}

#[test]
fn test_for_in_shadow() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let thing = undefined;
let stuff = {}
for (let thing in stuff) {
  thing;
}
thing;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 12) to (1, 21) => {
    Global undefined
  };
  (3, 18) to (3, 23) => {
    (2, 4) to (2, 9): (`stuff`)
  };
  (4, 2) to (4, 7) => {
    (3, 9) to (3, 14): (`thing`)
  };
  (6, 0) to (6, 5) => {
    (1, 4) to (1, 9): (`thing`)
  }]"#
    );
}

#[test]
fn test_for_in_throw() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
for (let thing in {}) {
  throw 'error';
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (5, 0) to (5, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_for_in_break_with_control_flow_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let y = undefined;
for (let thing in {}) {
  if (y == null) {
    break;
  }
  y;
}
y;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (3, 6) to (3, 7) => {
    (1, 4) to (1, 5): (`y`)
  };
  (6, 2) to (6, 3) => {
    {refinement = Not (Maybe); writes = (1, 4) to (1, 5): (`y`)}
  };
  (8, 0) to (8, 1) => {
    (1, 4) to (1, 5): (`y`)
  }]"#
    );
}

#[test]
fn test_for_in_with_runtime_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
let y = undefined;
for (let thing in {}) {
  if (y == null) {
    x = 2;
  }
  y;
}
y;
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 17) => {
    Global undefined
  };
  (4, 6) to (4, 7) => {
    (2, 4) to (2, 5): (`y`)
  };
  (7, 2) to (7, 3) => {
    (2, 4) to (2, 5): (`y`)
  };
  (9, 0) to (9, 1) => {
    (2, 4) to (2, 5): (`y`)
  };
  (10, 0) to (10, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_for_in_continue() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
for (let thing in {}) {
  continue;
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (5, 0) to (5, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_for_in_continue_with_control_flow_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let y = undefined;
for (let thing in {}) {
  if (y == null) {
    continue;
  }
  y;
}
y;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (3, 6) to (3, 7) => {
    (1, 4) to (1, 5): (`y`)
  };
  (6, 2) to (6, 3) => {
    {refinement = Not (Maybe); writes = (1, 4) to (1, 5): (`y`)}
  };
  (8, 0) to (8, 1) => {
    (1, 4) to (1, 5): (`y`)
  }]"#
    );
}

#[test]
fn test_for_in_reassign_right() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let stuff = {};
for (let thing in stuff) {
  stuff = [];
}
stuff;"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 18) to (2, 23) => {
    (1, 4) to (1, 9): (`stuff`)
  };
  (5, 0) to (5, 5) => {
    (1, 4) to (1, 9): (`stuff`)
  }]"#
    );
}

#[test]
fn test_for_of() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let stuff = {}
for (let thing of stuff) {
  thing
}"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 18) to (2, 23) => {
    (1, 4) to (1, 9): (`stuff`)
  };
  (3, 2) to (3, 7) => {
    (2, 9) to (2, 14): (`thing`)
  }]"#
    );
}

#[test]
fn test_for_of_reassign() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let stuff = {}
for (let thing of stuff) {
  thing;
  thing = 3;
}"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 18) to (2, 23) => {
    (1, 4) to (1, 9): (`stuff`)
  };
  (3, 2) to (3, 7) => {
    (2, 9) to (2, 14): (`thing`)
  }]"#
    );
}

#[test]
fn test_for_of_destructure() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let stuff = {}
for (let {thing} of stuff) {
  thing;
}"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 20) to (2, 25) => {
    (1, 4) to (1, 9): (`stuff`)
  };
  (3, 2) to (3, 7) => {
    (2, 10) to (2, 15): (`thing`)
  }]"#
    );
}

#[test]
fn test_for_of_destructure_reassign() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let stuff = {}
for (let {thing} of stuff) {
  thing;
  thing = 3;
}"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 20) to (2, 25) => {
    (1, 4) to (1, 9): (`stuff`)
  };
  (3, 2) to (3, 7) => {
    (2, 10) to (2, 15): (`thing`)
  }]"#
    );
}

#[test]
fn test_for_of_shadow() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let thing = undefined;
let stuff = {}
for (let thing of stuff) {
  thing;
}
thing;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 12) to (1, 21) => {
    Global undefined
  };
  (3, 18) to (3, 23) => {
    (2, 4) to (2, 9): (`stuff`)
  };
  (4, 2) to (4, 7) => {
    (3, 9) to (3, 14): (`thing`)
  };
  (6, 0) to (6, 5) => {
    (1, 4) to (1, 9): (`thing`)
  }]"#
    );
}

#[test]
fn test_for_of_throw() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
for (let thing of {}) {
  throw 'error';
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (5, 0) to (5, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_for_of_break_with_control_flow_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let y = undefined;
for (let thing of {}) {
  if (y == null) {
    break;
  }
  y;
}
y;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (3, 6) to (3, 7) => {
    (1, 4) to (1, 5): (`y`)
  };
  (6, 2) to (6, 3) => {
    {refinement = Not (Maybe); writes = (1, 4) to (1, 5): (`y`)}
  };
  (8, 0) to (8, 1) => {
    (1, 4) to (1, 5): (`y`)
  }]"#
    );
}

#[test]
fn test_for_of_with_runtime_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
let y = undefined;
for (let thing of {}) {
  if (y == null) {
    x = 2;
  }
  y;
}
y;
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 17) => {
    Global undefined
  };
  (4, 6) to (4, 7) => {
    (2, 4) to (2, 5): (`y`)
  };
  (7, 2) to (7, 3) => {
    (2, 4) to (2, 5): (`y`)
  };
  (9, 0) to (9, 1) => {
    (2, 4) to (2, 5): (`y`)
  };
  (10, 0) to (10, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_for_of_continue() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
for (let thing of {}) {
  continue;
}
x;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (5, 0) to (5, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_for_of_continue_with_control_flow_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let y = undefined;
for (let thing of {}) {
  if (y == null) {
    continue;
  }
  y;
}
y;"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (3, 6) to (3, 7) => {
    (1, 4) to (1, 5): (`y`)
  };
  (6, 2) to (6, 3) => {
    {refinement = Not (Maybe); writes = (1, 4) to (1, 5): (`y`)}
  };
  (8, 0) to (8, 1) => {
    (1, 4) to (1, 5): (`y`)
  }]"#
    );
}

#[test]
fn test_for_of_reassign_right() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let stuff = {};
for (let thing of stuff) {
  stuff = [];
}
stuff;"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 18) to (2, 23) => {
    (1, 4) to (1, 9): (`stuff`)
  };
  (5, 0) to (5, 5) => {
    (1, 4) to (1, 9): (`stuff`)
  }]"#
    );
}

#[test]
fn test_invariant_name_without_classification() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
invariant(x != null, "other arg");
x;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 0) to (2, 9) => {
    Global invariant
  };
  (2, 10) to (2, 11) => {
    (1, 4) to (1, 5): (`x`)
  };
  (3, 0) to (3, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_invariant() {
    let result = print_ssa_assertion_test(
        r#"let x = undefined;
invariant(x != null, "other arg");
x;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 0) to (2, 9) => {
    Global invariant
  };
  (2, 10) to (2, 11) => {
    (1, 4) to (1, 5): (`x`)
  };
  (3, 0) to (3, 1) => {
    {refinement = Not (Maybe); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_invariant_false() {
    let result = print_ssa_assertion_test(
        r#"let x = undefined;
if (x === 3) {
  invariant(false);
}
x;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 4) to (2, 5) => {
    (1, 4) to (1, 5): (`x`)
  };
  (3, 2) to (3, 11) => {
    Global invariant
  };
  (5, 0) to (5, 1) => {
    {refinement = Not (3); writes = (1, 4) to (1, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_invariant_reassign() {
    let result = print_ssa_assertion_test(
        r#"let x = undefined;
if (true) {
  invariant(false, x = 3);
}
x;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (3, 2) to (3, 11) => {
    Global invariant
  };
  (5, 0) to (5, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_try_catch_invariant_reassign() {
    let result = print_ssa_assertion_test(
        r#"let x = undefined;

try {
  if (true) {
    invariant(false, x = 3);
  }
} finally {
  x;
}"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (5, 4) to (5, 13) => {
    Global invariant
  };
  (8, 2) to (8, 3) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_switch_empty() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
switch (x) {};
x;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 0) to (2, 13) => {
    (1, 4) to (1, 5): (`x`)
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (3, 0) to (3, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_switch_only_default() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
switch (x) {
  default: {
    x;
  }
};
x;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (4, 4) to (4, 5) => {
    (1, 4) to (1, 5): (`x`)
  };
  (7, 0) to (7, 1) => {
    (1, 4) to (1, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_switch_break_every_case() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
switch (x) {
  case null:
    x;
    break;
  case 3:
    x;
    break;
  case false:
    x;
    break;
  default: {
    x;
  }
};
x;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (4, 4) to (4, 5) => {
    {refinement = Null; writes = (1, 4) to (1, 5): (`x`)}
  };
  (7, 4) to (7, 5) => {
    {refinement = 3; writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}
  };
  (10, 4) to (10, 5) => {
    {refinement = false; writes = {refinement = Not (3); writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}}
  };
  (13, 4) to (13, 5) => {
    {refinement = Not (false); writes = {refinement = Not (3); writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}}
  };
  (16, 0) to (16, 1) => {
    {refinement = Null; writes = (1, 4) to (1, 5): (`x`)},
    {refinement = 3; writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}},
    {refinement = false; writes = {refinement = Not (3); writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}},
    {refinement = Not (false); writes = {refinement = Not (3); writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}}
  }]"#
    );
}

#[test]
fn test_switch_prop_return_every_case() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"function foo() {
let x = undefined;
switch (x) {
  case 1:
    return;
  case 2:
    return;
  default:
    return;
};
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 8) to (2, 17) => {
    Global undefined
  };
  (3, 8) to (3, 9) => {
    (2, 4) to (2, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_switch_with_fallthroughs() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
switch (x) {
  case null:
    x;
  case 3:
    x;
    break;
  case true:
  case false:
    x;
  default: {
    x;
  }
}
x;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (4, 4) to (4, 5) => {
    {refinement = Null; writes = (1, 4) to (1, 5): (`x`)}
  };
  (6, 4) to (6, 5) => {
    {refinement = Null; writes = (1, 4) to (1, 5): (`x`)},
    {refinement = 3; writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}
  };
  (10, 4) to (10, 5) => {
    {refinement = true; writes = {refinement = Not (3); writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}},
    {refinement = false; writes = {refinement = Not (true); writes = {refinement = Not (3); writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}}}
  };
  (12, 4) to (12, 5) => {
    {refinement = true; writes = {refinement = Not (3); writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}},
    {refinement = false; writes = {refinement = Not (true); writes = {refinement = Not (3); writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}}},
    {refinement = Not (false); writes = {refinement = Not (true); writes = {refinement = Not (3); writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}}}
  };
  (15, 0) to (15, 1) => {
    {refinement = Null; writes = (1, 4) to (1, 5): (`x`)},
    {refinement = 3; writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}},
    {refinement = true; writes = {refinement = Not (3); writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}},
    {refinement = false; writes = {refinement = Not (true); writes = {refinement = Not (3); writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}}},
    {refinement = Not (false); writes = {refinement = Not (true); writes = {refinement = Not (3); writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}}}
  }]"#
    );
}

#[test]
fn test_switch_merge_all_breaks() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x;
switch (1) {
  case 1:
    x = 1;
    break;
  default:
    x = 2;
    break;
};
x;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (10, 0) to (10, 1) => {
    (4, 4) to (4, 5): (`x`),
    (7, 4) to (7, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_switch_throw_in_default() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"let x = undefined;
switch (x) {
  case null:
    x;
  case 3:
    x;
    break;
  case true:
  case false:
    x;
    break;
  default: {
    throw 'error'
  }
};
x;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 8) to (1, 17) => {
    Global undefined
  };
  (2, 8) to (2, 9) => {
    (1, 4) to (1, 5): (`x`)
  };
  (4, 4) to (4, 5) => {
    {refinement = Null; writes = (1, 4) to (1, 5): (`x`)}
  };
  (6, 4) to (6, 5) => {
    {refinement = Null; writes = (1, 4) to (1, 5): (`x`)},
    {refinement = 3; writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}
  };
  (10, 4) to (10, 5) => {
    {refinement = true; writes = {refinement = Not (3); writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}},
    {refinement = false; writes = {refinement = Not (true); writes = {refinement = Not (3); writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}}}
  };
  (16, 0) to (16, 1) => {
    {refinement = Null; writes = (1, 4) to (1, 5): (`x`)},
    {refinement = 3; writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}},
    {refinement = true; writes = {refinement = Not (3); writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}},
    {refinement = false; writes = {refinement = Not (true); writes = {refinement = Not (3); writes = {refinement = Not (Null); writes = (1, 4) to (1, 5): (`x`)}}}}
  }]"#
    );
}

#[test]
fn test_arguments_eval_read() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
arguments;
eval;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 0) to (2, 9) => {
    ModuleScoped arguments
  };
  (3, 0) to (3, 4) => {
    ModuleScoped eval
  }]"#
    );
}

#[test]
fn test_arguments_shadowed() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function foo(arguments) {
  arguments;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 2) to (3, 11) => {
    (2, 13) to (2, 22): (`arguments`)
  }]"#
    );
}

#[test]
fn test_global_refinement() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
Map != null && Map
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 0) to (2, 3) => {
    Global Map
  };
  (2, 15) to (2, 18) => {
    {refinement = Not (Maybe); writes = Global Map}
  }]"#
    );
}

#[test]
fn test_global_refinement_control_flow() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if (Map != null) {
  throw 'error';
}
Map;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 7) => {
    Global Map
  };
  (5, 0) to (5, 3) => {
    {refinement = Not (Not (Maybe)); writes = Global Map}
  }]"#
    );
}

#[test]
fn test_global_overwrite() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if (true) {
  undefined = null;
}
undefined;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 0) to (5, 9) => {
    Global undefined,
    (3, 2) to (3, 11): (`undefined`)
  }]"#
    );
}

#[test]
fn test_havoc_from_uninitialized() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
var x: number;
function havoc() {
    x = 42;
}
havoc();
(x: void)
"#,
    );
    assert_eq!(
        result,
        r#"[
  (6, 0) to (6, 5) => {
    (3, 9) to (3, 14): (`havoc`)
  };
  (7, 1) to (7, 2) => {
    (uninitialized),
    (2, 4) to (2, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_captured_havoc() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"function g() {
  var xx : { p : number } | null = { p : 4 };
  if (xx) {
    return function () {
       xx.p = 3;
    }
  }
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 6) to (3, 8) => {
    (2, 6) to (2, 8): (`xx`)
  };
  (5, 7) to (5, 9) => {
    {refinement = Truthy; writes = (2, 6) to (2, 8): (`xx`)}
  }]"#
    );
}

#[test]
fn test_no_providers() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
var x;
function fn() {
    x;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (4, 4) to (4, 5) => {
    (uninitialized)
  }]"#
    );
}

#[test]
fn test_class_expr() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let y = 42;
let x = class y { m() { x; y } };
y;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 24) to (3, 25) => {
    (3, 4) to (3, 5): (`x`)
  };
  (3, 27) to (3, 28) => {
    (3, 14) to (3, 15): (`y`)
  };
  (4, 0) to (4, 1) => {
    (2, 4) to (2, 5): (`y`)
  }]"#
    );
}

#[test]
fn test_havoc() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = 3;
function f() { x = 'string'}
let y = 3;
if (x != null && y != null) {
  f();
  x;
  y;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 4) to (5, 5) => {
    (2, 4) to (2, 5): (`x`)
  };
  (5, 17) to (5, 18) => {
    (4, 4) to (4, 5): (`y`)
  };
  (6, 2) to (6, 3) => {
    (3, 9) to (3, 10): (`f`)
  };
  (7, 2) to (7, 3) => {
    (2, 4) to (2, 5): (`x`)
  };
  (8, 2) to (8, 3) => {
    {refinement = Not (Maybe); writes = (4, 4) to (4, 5): (`y`)}
  }]"#
    );
}

#[test]
fn test_provider_closure_havoc_1() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
var x = null;
function havoc() { x = 21 }

if (typeof x === 'number') {
  x;
  havoc();
  x;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 11) to (5, 12) => {
    (2, 4) to (2, 5): (`x`)
  };
  (6, 2) to (6, 3) => {
    {refinement = number; writes = (2, 4) to (2, 5): (`x`)}
  };
  (7, 2) to (7, 7) => {
    (3, 9) to (3, 14): (`havoc`)
  };
  (8, 2) to (8, 3) => {
    (3, 19) to (3, 20): (`x`),
    {refinement = number; writes = (2, 4) to (2, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_provider_closure_havoc_2() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
var y = null;
function havoc() { y = 31 }
function havoc2() { y = 42 }

if (typeof y === 'number') {
  y;
  havoc();
  y;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (6, 11) to (6, 12) => {
    (2, 4) to (2, 5): (`y`)
  };
  (7, 2) to (7, 3) => {
    {refinement = number; writes = (2, 4) to (2, 5): (`y`)}
  };
  (8, 2) to (8, 7) => {
    (3, 9) to (3, 14): (`havoc`)
  };
  (9, 2) to (9, 3) => {
    (2, 4) to (2, 5): (`y`),
    (3, 19) to (3, 20): (`y`)
  }]"#
    );
}

#[test]
fn test_provider_closure_havoc_3() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
var z = null;
function havocz() {
  z = 42;
}

havocz();
z;

if (typeof z === 'number'){
  havocz();
  z;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (7, 0) to (7, 6) => {
    (3, 9) to (3, 15): (`havocz`)
  };
  (8, 0) to (8, 1) => {
    (2, 4) to (2, 5): (`z`),
    (4, 2) to (4, 3): (`z`)
  };
  (10, 11) to (10, 12) => {
    (2, 4) to (2, 5): (`z`),
    (4, 2) to (4, 3): (`z`)
  };
  (11, 2) to (11, 8) => {
    (3, 9) to (3, 15): (`havocz`)
  };
  (12, 2) to (12, 3) => {
    (4, 2) to (4, 3): (`z`),
    {refinement = number; writes = (2, 4) to (2, 5): (`z`),(4, 2) to (4, 3): (`z`)}
  }]"#
    );
}

#[test]
fn test_latent_refinements() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = 3;
function f() { x = 'string'}
let y = 3;
if (f(x, y)) {
  x;
  y;
}
if (y.f(x, y)) {
  // No refinements on either
  x;
  y;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 4) to (5, 5) => {
    (3, 9) to (3, 10): (`f`)
  };
  (5, 6) to (5, 7) => {
    (2, 4) to (2, 5): (`x`)
  };
  (5, 9) to (5, 10) => {
    (4, 4) to (4, 5): (`y`)
  };
  (6, 2) to (6, 3) => {
    {refinement = LatentR (index = 0); writes = (2, 4) to (2, 5): (`x`)}
  };
  (7, 2) to (7, 3) => {
    {refinement = LatentR (index = 1); writes = (4, 4) to (4, 5): (`y`)}
  };
  (9, 4) to (9, 5) => {
    (4, 4) to (4, 5): (`y`)
  };
  (9, 8) to (9, 9) => {
    (2, 4) to (2, 5): (`x`)
  };
  (9, 11) to (9, 12) => {
    (4, 4) to (4, 5): (`y`)
  };
  (11, 2) to (11, 3) => {
    {refinement = LatentR (index = 0); writes = (2, 4) to (2, 5): (`x`)}
  };
  (12, 2) to (12, 3) => {
    {refinement = LatentR (index = 1); writes = (4, 4) to (4, 5): (`y`)}
  }]"#
    );
}

#[test]
fn test_loop_read_from_havoc() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
type T = string;

function switch_fn() {
  declare const n: number;

  switch (n) {
    case 2:
      ('a': T);
      return 1;

    case 3:
      if (String(('a': T))) {}
  }
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (7, 2) to (14, 3) => {
    {refinement = Not (3); writes = {refinement = Not (2); writes = (5, 16) to (5, 17): (`n`)}}
  };
  (7, 10) to (7, 11) => {
    (5, 16) to (5, 17): (`n`)
  };
  (9, 12) to (9, 13) => {
    (2, 5) to (2, 6): (`T`)
  };
  (13, 10) to (13, 16) => {
    Global String
  };
  (13, 23) to (13, 24) => {
    (2, 5) to (2, 6): (`T`)
  }]"#
    );
}

#[test]
fn test_read_merged_value_instead_of_captured_value() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let a: string | number = '';

const f = () => {
  if (true) {
    a = 1;
  } else {
    a = 2;
  }
  a;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (10, 2) to (10, 3) => {
    (6, 4) to (6, 5): (`a`),
    (8, 4) to (8, 5): (`a`)
  }]"#
    );
}

#[test]
fn test_loop_read_from_havoc_2() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let data: any = [];

function asObjectList(length: number) {
  while (data) {
    data = data.concat(data);
  }
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 9) to (5, 13) => {
    (2, 4) to (2, 8): (`data`)
  };
  (6, 11) to (6, 15) => {
    {refinement = Truthy; writes = (2, 4) to (2, 8): (`data`)}
  };
  (6, 23) to (6, 27) => {
    {refinement = Truthy; writes = (2, 4) to (2, 8): (`data`)}
  }]"#
    );
}

#[test]
fn test_heap_refinement_basic() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = {};
if (x.foo === 3) {
  x.foo;
  if (x.foo === 4) {
    x.foo;
  }
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 4) to (3, 5) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 2) to (4, 3) => {
    {refinement = SentinelR foo; writes = (2, 4) to (2, 5): (`x`)}
  };
  (4, 2) to (4, 7) => {
    {refinement = 3; writes = projection at (3, 4) to (3, 9)}
  };
  (5, 6) to (5, 7) => {
    {refinement = SentinelR foo; writes = (2, 4) to (2, 5): (`x`)}
  };
  (5, 6) to (5, 11) => {
    {refinement = 3; writes = projection at (3, 4) to (3, 9)}
  };
  (6, 4) to (6, 5) => {
    {refinement = SentinelR foo; writes = {refinement = SentinelR foo; writes = (2, 4) to (2, 5): (`x`)}}
  };
  (6, 4) to (6, 9) => {
    {refinement = 4; writes = {refinement = 3; writes = projection at (3, 4) to (3, 9)}}
  }]"#
    );
}

#[test]
fn test_heap_refinement_this_basic() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if (this.foo === 3) {
  this.foo;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 8) => {
    This(global)
  };
  (3, 2) to (3, 6) => {
    {refinement = SentinelR foo; writes = This(global)}
  };
  (3, 2) to (3, 10) => {
    {refinement = 3; writes = projection at (2, 4) to (2, 12)}
  }]"#
    );
}

#[test]
fn test_heap_refinement_super() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
class A {
  static foo() {
    if (super.baz === 3) {
      super.baz;
    }
  }
  bar() {
    if (super.baz === 3) {
      super.baz;
    }
  }
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (4, 8) to (4, 13) => {
    Super(static)
  };
  (5, 6) to (5, 11) => {
    {refinement = SentinelR baz; writes = Super(static)}
  };
  (5, 6) to (5, 15) => {
    {refinement = 3; writes = projection at (4, 8) to (4, 17)}
  };
  (9, 8) to (9, 13) => {
    Super(instance)
  };
  (10, 6) to (10, 11) => {
    {refinement = SentinelR baz; writes = Super(instance)}
  };
  (10, 6) to (10, 15) => {
    {refinement = 3; writes = projection at (9, 8) to (9, 17)}
  }]"#
    );
}

#[test]
fn test_heap_refinement_basic_2() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = {};
if (x.foo === 3) {
  x.foo;
  if (x.foo === 4) {
    x.foo;
  }
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 4) to (3, 5) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 2) to (4, 3) => {
    {refinement = SentinelR foo; writes = (2, 4) to (2, 5): (`x`)}
  };
  (4, 2) to (4, 7) => {
    {refinement = 3; writes = projection at (3, 4) to (3, 9)}
  };
  (5, 6) to (5, 7) => {
    {refinement = SentinelR foo; writes = (2, 4) to (2, 5): (`x`)}
  };
  (5, 6) to (5, 11) => {
    {refinement = 3; writes = projection at (3, 4) to (3, 9)}
  };
  (6, 4) to (6, 5) => {
    {refinement = SentinelR foo; writes = {refinement = SentinelR foo; writes = (2, 4) to (2, 5): (`x`)}}
  };
  (6, 4) to (6, 9) => {
    {refinement = 4; writes = {refinement = 3; writes = projection at (3, 4) to (3, 9)}}
  }]"#
    );
}

#[test]
fn test_heap_refinement_destructure() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = {};
if (x.foo === 3) {
  const { foo } = x;
  foo;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 4) to (3, 5) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 10) to (4, 13) => {
    {refinement = 3; writes = projection at (3, 4) to (3, 9)}
  };
  (4, 18) to (4, 19) => {
    {refinement = SentinelR foo; writes = (2, 4) to (2, 5): (`x`)}
  };
  (5, 2) to (5, 5) => {
    {refinement = 3; writes = (4, 10) to (4, 13): (`foo`)}
  }]"#
    );
}

#[test]
fn test_heap_refinement_from_assign_destructure() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = {};
x.foo = 3;
const { foo } = x;
foo;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 0) to (3, 1) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 8) to (4, 11) => {
    (3, 0) to (3, 5): ()
  };
  (4, 16) to (4, 17) => {
    (2, 4) to (2, 5): (`x`)
  };
  (5, 0) to (5, 3) => {
    (4, 8) to (4, 11): (`foo`)
  }]"#
    );
}

#[test]
fn test_heap_refinement_deep_destructure() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = {};
if (x.bar.baz.hello.world === 4) {
  const { bar: { baz: { hello: {world: hi} } } } = x;
  hi;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 4) to (3, 5) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 24) to (4, 29) => {
    {refinement = SentinelR world; writes = projection at (3, 4) to (3, 19)}
  };
  (4, 32) to (4, 37) => {
    {refinement = 4; writes = projection at (3, 4) to (3, 25)}
  };
  (4, 51) to (4, 52) => {
    (2, 4) to (2, 5): (`x`)
  };
  (5, 2) to (5, 4) => {
    {refinement = 4; writes = (4, 39) to (4, 41): (`hi`)}
  }]"#
    );
}

#[test]
fn test_heap_refinement_from_assign_deep_destructure() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = {};
x.bar.baz.hello.world = 4;
const { bar: { baz: { hello: {world: hi} } } } = x;
hi;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 0) to (3, 1) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 30) to (4, 35) => {
    (3, 0) to (3, 21): ()
  };
  (4, 49) to (4, 50) => {
    (2, 4) to (2, 5): (`x`)
  };
  (5, 0) to (5, 2) => {
    (4, 37) to (4, 39): (`hi`)
  }]"#
    );
}

#[test]
fn test_heap_refinement_merge_branches() {
    let result = print_ssa_assertion_test(
        r#"
declare var invariant: any;
let x = {};
if (true) {
  invariant(x.foo === 3);
} else {
  invariant(x.foo === 4);
}
x.foo;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 2) to (5, 11) => {
    (2, 12) to (2, 21): (`invariant`)
  };
  (5, 12) to (5, 13) => {
    (3, 4) to (3, 5): (`x`)
  };
  (7, 2) to (7, 11) => {
    (2, 12) to (2, 21): (`invariant`)
  };
  (7, 12) to (7, 13) => {
    (3, 4) to (3, 5): (`x`)
  };
  (9, 0) to (9, 1) => {
    {refinement = SentinelR foo; writes = (3, 4) to (3, 5): (`x`)},
    {refinement = SentinelR foo; writes = (3, 4) to (3, 5): (`x`)}
  };
  (9, 0) to (9, 5) => {
    {refinement = 3; writes = projection at (5, 12) to (5, 17)},
    {refinement = 4; writes = projection at (7, 12) to (7, 17)}
  }]"#
    );
}

#[test]
fn test_heap_refinement_normalize_from_and() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = {};
if (x.foo === 3 && x.foo === 3) {
  x.foo;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 4) to (3, 5) => {
    (2, 4) to (2, 5): (`x`)
  };
  (3, 19) to (3, 20) => {
    {refinement = SentinelR foo; writes = (2, 4) to (2, 5): (`x`)}
  };
  (3, 19) to (3, 24) => {
    {refinement = 3; writes = projection at (3, 4) to (3, 9)}
  };
  (4, 2) to (4, 3) => {
    {refinement = And (SentinelR foo, SentinelR foo); writes = (2, 4) to (2, 5): (`x`)}
  };
  (4, 2) to (4, 7) => {
    {refinement = And (3, 3); writes = projection at (3, 4) to (3, 9)}
  }]"#
    );
}

#[test]
fn test_heap_refinement_normalize_from_or() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = {};
if (x.foo === 3 || x.foo === 4) {
  x.foo;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 4) to (3, 5) => {
    (2, 4) to (2, 5): (`x`)
  };
  (3, 19) to (3, 20) => {
    {refinement = Not (SentinelR foo); writes = (2, 4) to (2, 5): (`x`)}
  };
  (3, 19) to (3, 24) => {
    {refinement = Not (3); writes = projection at (3, 4) to (3, 9)}
  };
  (4, 2) to (4, 3) => {
    {refinement = Or (SentinelR foo, SentinelR foo); writes = (2, 4) to (2, 5): (`x`)}
  };
  (4, 2) to (4, 7) => {
    {refinement = Or (3, 4); writes = projection at (3, 4) to (3, 9)}
  }]"#
    );
}

#[test]
fn test_heap_refinement_one_branch() {
    let result = print_ssa_assertion_test(
        r#"
declare var invariant: any;
let x = {};
if (true) {
  invariant(x.foo === 3);
} else {
}
x.foo;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 2) to (5, 11) => {
    (2, 12) to (2, 21): (`invariant`)
  };
  (5, 12) to (5, 13) => {
    (3, 4) to (3, 5): (`x`)
  };
  (8, 0) to (8, 1) => {
    (3, 4) to (3, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_heap_refinement_while_loop_subject_changed() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = {};
while (x.foo === 3) {
  x.foo;
  x = {};
  break;
}
x.foo;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 7) to (3, 8) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 2) to (4, 3) => {
    {refinement = SentinelR foo; writes = (2, 4) to (2, 5): (`x`)}
  };
  (4, 2) to (4, 7) => {
    {refinement = 3; writes = projection at (3, 7) to (3, 12)}
  };
  (8, 0) to (8, 1) => {
    (2, 4) to (2, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_heap_refinement_while_loop_call_invalidates_projection() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
declare var mutateObj: any;
let x = {};
while (x.foo === 3) {
  mutateObj(x.foo === 4);
  x.foo;
  break;
}
x.foo;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (4, 7) to (4, 8) => {
    (3, 4) to (3, 5): (`x`)
  };
  (5, 2) to (5, 11) => {
    (2, 12) to (2, 21): (`mutateObj`)
  };
  (5, 12) to (5, 13) => {
    {refinement = SentinelR foo; writes = (3, 4) to (3, 5): (`x`)}
  };
  (5, 12) to (5, 17) => {
    {refinement = 3; writes = projection at (4, 7) to (4, 12)}
  };
  (6, 2) to (6, 3) => {
    {refinement = SentinelR foo; writes = (3, 4) to (3, 5): (`x`)}
  };
  (9, 0) to (9, 1) => {
    (3, 4) to (3, 5): (`x`)
  };
  (6, 2) to (6, 7) => invalidated refinement by {
    function call at (5, 2) to (5, 24)
  };
  (9, 0) to (9, 5) => invalidated refinement by {
    function call at (5, 2) to (5, 24)
  }]"#
    );
}

#[test]
fn test_heap_refinement_while_loop_projection_changed() {
    let result = print_ssa_assertion_test(
        r#"
declare var invariant: any;
let x = {};
while (x.foo === 3) {
  invariant(x.foo === 4);
  x.foo;
  break;
}
x.foo;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (4, 7) to (4, 8) => {
    (3, 4) to (3, 5): (`x`)
  };
  (5, 2) to (5, 11) => {
    (2, 12) to (2, 21): (`invariant`)
  };
  (5, 12) to (5, 13) => {
    {refinement = SentinelR foo; writes = (3, 4) to (3, 5): (`x`)}
  };
  (5, 12) to (5, 17) => {
    {refinement = 3; writes = projection at (4, 7) to (4, 12)}
  };
  (6, 2) to (6, 3) => {
    {refinement = SentinelR foo; writes = {refinement = SentinelR foo; writes = (3, 4) to (3, 5): (`x`)}}
  };
  (6, 2) to (6, 7) => {
    {refinement = 4; writes = {refinement = 3; writes = projection at (4, 7) to (4, 12)}}
  };
  (9, 0) to (9, 1) => {
    (3, 4) to (3, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_heap_refinement_while_loop_negated() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = {};
while (x.foo === 3) {
  x.foo;
}
x.foo;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 7) to (3, 8) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 2) to (4, 3) => {
    {refinement = SentinelR foo; writes = (2, 4) to (2, 5): (`x`)}
  };
  (4, 2) to (4, 7) => {
    {refinement = 3; writes = projection at (3, 7) to (3, 12)}
  };
  (6, 0) to (6, 1) => {
    {refinement = Not (SentinelR foo); writes = (2, 4) to (2, 5): (`x`)}
  };
  (6, 0) to (6, 5) => {
    {refinement = Not (3); writes = projection at (3, 7) to (3, 12)}
  }]"#
    );
}

#[test]
fn test_heap_refinement_loop_control_flow_write() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = {};
while (x.foo === 3) {
  if (x.foo === 4) {
    x.foo;
  } else { throw 'error'}
  x.foo;
}
x.foo;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 7) to (3, 8) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 6) to (4, 7) => {
    {refinement = SentinelR foo; writes = (2, 4) to (2, 5): (`x`)}
  };
  (4, 6) to (4, 11) => {
    {refinement = 3; writes = projection at (3, 7) to (3, 12)}
  };
  (5, 4) to (5, 5) => {
    {refinement = SentinelR foo; writes = {refinement = SentinelR foo; writes = (2, 4) to (2, 5): (`x`)}}
  };
  (5, 4) to (5, 9) => {
    {refinement = 4; writes = {refinement = 3; writes = projection at (3, 7) to (3, 12)}}
  };
  (7, 2) to (7, 3) => {
    {refinement = SentinelR foo; writes = {refinement = SentinelR foo; writes = (2, 4) to (2, 5): (`x`)}}
  };
  (7, 2) to (7, 7) => {
    {refinement = 4; writes = {refinement = 3; writes = projection at (3, 7) to (3, 12)}}
  };
  (9, 0) to (9, 1) => {
    {refinement = Not (SentinelR foo); writes = (2, 4) to (2, 5): (`x`)}
  };
  (9, 0) to (9, 5) => {
    {refinement = Not (3); writes = projection at (3, 7) to (3, 12),{refinement = 4; writes = projection at (3, 7) to (3, 12)}}
  }]"#
    );
}

#[test]
fn test_heap_refinement_write() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = {};
x.foo = 3;
x.foo;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 0) to (3, 1) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 0) to (4, 1) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 0) to (4, 5) => {
    (3, 0) to (3, 5): ()
  }]"#
    );
}

#[test]
fn test_heap_refinement_write_havoc_member() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = {};
x.foo = 3;
let y = x;
y.foo = 3;
x.foo;
y.foo;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 0) to (3, 1) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 8) to (4, 9) => {
    (2, 4) to (2, 5): (`x`)
  };
  (5, 0) to (5, 1) => {
    (4, 4) to (4, 5): (`y`)
  };
  (6, 0) to (6, 1) => {
    (2, 4) to (2, 5): (`x`)
  };
  (7, 0) to (7, 1) => {
    (4, 4) to (4, 5): (`y`)
  };
  (7, 0) to (7, 5) => {
    (5, 0) to (5, 5): ()
  };
  (6, 0) to (6, 5) => invalidated refinement by {
    property assignment at (5, 0) to (5, 9)
  }]"#
    );
}

#[test]
fn test_heap_refinement_write_havoc_elem() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = {};
x.foo = 3;
let y = x;
y.foo = 3;
y[x] = 4;
x.foo;
y.foo;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 0) to (3, 1) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 8) to (4, 9) => {
    (2, 4) to (2, 5): (`x`)
  };
  (5, 0) to (5, 1) => {
    (4, 4) to (4, 5): (`y`)
  };
  (6, 0) to (6, 1) => {
    (4, 4) to (4, 5): (`y`)
  };
  (6, 2) to (6, 3) => {
    (2, 4) to (2, 5): (`x`)
  };
  (7, 0) to (7, 1) => {
    (2, 4) to (2, 5): (`x`)
  };
  (8, 0) to (8, 1) => {
    (4, 4) to (4, 5): (`y`)
  };
  (7, 0) to (7, 5) => invalidated refinement by {
    property assignment at (6, 0) to (6, 8)
  };
  (8, 0) to (8, 5) => invalidated refinement by {
    property assignment at (6, 0) to (6, 8)
  }]"#
    );
}

#[test]
fn test_heap_refinement_havoc_on_write() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = {};
x.foo = 3;
x.foo;
x = {};
x.foo;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 0) to (3, 1) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 0) to (4, 1) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 0) to (4, 5) => {
    (3, 0) to (3, 5): ()
  };
  (6, 0) to (6, 1) => {
    (5, 0) to (5, 1): (`x`)
  }]"#
    );
}

#[test]
fn test_unreachable_code() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
x = 4;
x;
throw new Error();
var x = 3;
x;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 0) to (3, 1) => {
    (2, 0) to (2, 1): (`x`)
  };
  (4, 10) to (4, 15) => {
    Global Error
  };
  (6, 0) to (6, 1) => {
    unreachable
  }]"#
    );
}

#[test]
fn test_function_hoisted_typeof() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = null;
if (Math.random()) {
  x = 'string';
} else {
  x = 4;
}

x = 5;
// Note that the typeofs here report all the providers and not x = 5
function f<T: typeof x = typeof x>(y: typeof x): typeof x { return null; }
declare function f<T: typeof x = typeof x>(y: typeof x): typeof x;
// The return here should not be hoisted, but the param is because
// we havoc before we visit the params.
let y = (z: typeof x): typeof x => 3;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 4) to (3, 8) => {
    Global Math
  };
  (11, 21) to (11, 22) => {
    (2, 4) to (2, 5): (`x`),
    (4, 2) to (4, 3): (`x`),
    (6, 2) to (6, 3): (`x`)
  };
  (11, 32) to (11, 33) => {
    (2, 4) to (2, 5): (`x`),
    (4, 2) to (4, 3): (`x`),
    (6, 2) to (6, 3): (`x`)
  };
  (11, 45) to (11, 46) => {
    (2, 4) to (2, 5): (`x`),
    (4, 2) to (4, 3): (`x`),
    (6, 2) to (6, 3): (`x`)
  };
  (11, 56) to (11, 57) => {
    (2, 4) to (2, 5): (`x`),
    (4, 2) to (4, 3): (`x`),
    (6, 2) to (6, 3): (`x`)
  };
  (12, 29) to (12, 30) => {
    (2, 4) to (2, 5): (`x`),
    (4, 2) to (4, 3): (`x`),
    (6, 2) to (6, 3): (`x`)
  };
  (12, 40) to (12, 41) => {
    (2, 4) to (2, 5): (`x`),
    (4, 2) to (4, 3): (`x`),
    (6, 2) to (6, 3): (`x`)
  };
  (12, 43) to (12, 44) => {
    (15, 4) to (15, 5): (`y`)
  };
  (12, 53) to (12, 54) => {
    (2, 4) to (2, 5): (`x`),
    (4, 2) to (4, 3): (`x`),
    (6, 2) to (6, 3): (`x`)
  };
  (12, 64) to (12, 65) => {
    (2, 4) to (2, 5): (`x`),
    (4, 2) to (4, 3): (`x`),
    (6, 2) to (6, 3): (`x`)
  };
  (15, 19) to (15, 20) => {
    (2, 4) to (2, 5): (`x`),
    (4, 2) to (4, 3): (`x`),
    (6, 2) to (6, 3): (`x`)
  };
  (15, 30) to (15, 31) => {
    (2, 4) to (2, 5): (`x`),
    (4, 2) to (4, 3): (`x`),
    (6, 2) to (6, 3): (`x`)
  }]"#
    );
}

#[test]
fn test_hoisted_global_refinement() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if (global != null) {
  global;
  function f(x: typeof global) { }
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 10) => {
    Global global
  };
  (3, 2) to (3, 8) => {
    {refinement = Not (Maybe); writes = Global global}
  };
  (4, 23) to (4, 29) => {
    Global global
  }]"#
    );
}

#[test]
fn test_this_in_object() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
({
  foo() {
    this;
  },
  get bar() {
    this;
  },
  set baz(f) {
    this;
  }
});
"#,
    );
    assert_eq!(
        result,
        r#"[
  (4, 4) to (4, 8) => {
    This(illegal)
  };
  (7, 4) to (7, 8) => {
    This(illegal)
  };
  (10, 4) to (10, 8) => {
    This(illegal)
  }]"#
    );
}

#[test]
fn test_type_alias() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
type t = number;
let x: t = 42;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 7) to (3, 8) => {
    (2, 5) to (2, 6): (`t`)
  }]"#
    );
}

#[test]
fn test_type_alias_global() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x: t = 42;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 7) to (2, 8) => {
    Global t
  }]"#
    );
}

#[test]
fn test_type_alias_refine() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if (t) {
  let x: t = 42;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => {
    Global t
  };
  (3, 9) to (3, 10) => {
    {refinement = Truthy; writes = Global t}
  }]"#
    );
}

#[test]
fn test_type_alias_no_init() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
type t = number;
let x: t;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 7) to (3, 8) => {
    (2, 5) to (2, 6): (`t`)
  }]"#
    );
}

#[test]
fn test_type_alias_lookup() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
import * as React from 'react';
type T = React.ComponentType;
var C: React.ComponentType;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 9) to (3, 14) => {
    (2, 12) to (2, 17): (`React`)
  };
  (4, 7) to (4, 12) => {
    (2, 12) to (2, 17): (`React`)
  }]"#
    );
}

#[test]
fn test_conditional_type_with_infer() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
// TS behavior: https://www.typescriptlang.org/play?#code/C4TwDgpgBAYg9nKBeKBLAdgMwgJygQSggA9gJ0ATAZygAoNs8AhI086tLXKAYSgH5OjKABEoALiHcAogEoBUANr4ANExU8VIldIC6EpavWbtegNxA
type Foo = infer A
    extends (infer B extends infer C ? infer D : infer E)
    ? [A,B,C,D,E]
    : [A,B,C,D,E];
"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 7) to (5, 8) => {
    Global A
  };
  (5, 9) to (5, 10) => {
    (4, 19) to (4, 20): (`B`)
  };
  (5, 11) to (5, 12) => {
    Global C
  };
  (5, 13) to (5, 14) => {
    (4, 45) to (4, 46): (`D`)
  };
  (5, 15) to (5, 16) => {
    (4, 55) to (4, 56): (`E`)
  };
  (6, 7) to (6, 8) => {
    Global A
  };
  (6, 9) to (6, 10) => {
    Global B
  };
  (6, 11) to (6, 12) => {
    Global C
  };
  (6, 13) to (6, 14) => {
    Global D
  };
  (6, 15) to (6, 16) => {
    Global E
  }]"#
    );
}

#[test]
fn test_no_cyclic_infer() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
// TS behavior: https://www.typescriptlang.org/play?#code/C4TwDgpgBAYg9nKBeKBnYAnAlgOwOZQQAewEOAJqlAN5QCGAXFLgGYQZQCChJZlUAIQDcUAEZNW7QT1IUq3AL5QA-Gky4CTHAFcAtqPZCAUEA
type Foo = string extends { a: infer A extends B, b: infer B extends A } ? string : number;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 47) to (3, 48) => {
    Global B
  };
  (3, 69) to (3, 70) => {
    Global A
  }]"#
    );
}

#[test]
fn test_new_type_arg() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
type A = number;
new Set<A>();
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 4) to (3, 7) => {
    Global Set
  };
  (3, 8) to (3, 9) => {
    (2, 5) to (2, 6): (`A`)
  }]"#
    );
}

#[test]
fn test_class_as_type() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
class C { }
var x: C = new C();
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 7) to (3, 8) => {
    (2, 6) to (2, 7): (`C`)
  };
  (3, 15) to (3, 16) => {
    (2, 6) to (2, 7): (`C`)
  }]"#
    );
}

#[test]
fn test_interface_as_type() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
interface C { }
var x: C;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 7) to (3, 8) => {
    (2, 10) to (2, 11): (`C`)
  }]"#
    );
}

#[test]
fn test_hoist_type() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
var x: T;
type T = number;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 7) to (2, 8) => {
    (3, 5) to (3, 6): (`T`)
  }]"#
    );
}

#[test]
fn test_hoist_interface() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
var x: C;
interface C { }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 7) to (2, 8) => {
    (3, 10) to (3, 11): (`C`)
  }]"#
    );
}

#[test]
fn test_opaque() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
var x: T;
opaque type T: C = C;
interface C { }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 7) to (2, 8) => {
    (3, 12) to (3, 13): (`T`)
  };
  (3, 15) to (3, 16) => {
    (4, 10) to (4, 11): (`C`)
  };
  (3, 19) to (3, 20) => {
    (4, 10) to (4, 11): (`C`)
  }]"#
    );
}

#[test]
fn test_mutual() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
type A = Array<B>;
type B = { a: A };
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 9) to (2, 14) => {
    Global Array
  };
  (2, 15) to (2, 16) => {
    (3, 5) to (3, 6): (`B`)
  };
  (3, 14) to (3, 15) => {
    (2, 5) to (2, 6): (`A`)
  }]"#
    );
}

#[test]
fn test_fun_tparam() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function f<X, Y: X = number>(x: X, y: Y) {
  (x: Y);
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 17) to (2, 18) => {
    (2, 11) to (2, 12): (`X`)
  };
  (2, 32) to (2, 33) => {
    (2, 11) to (2, 12): (`X`)
  };
  (2, 38) to (2, 39) => {
    (2, 14) to (2, 15): (`Y`)
  };
  (3, 3) to (3, 4) => {
    (2, 29) to (2, 30): (`x`)
  };
  (3, 6) to (3, 7) => {
    (2, 14) to (2, 15): (`Y`)
  }]"#
    );
}

#[test]
fn test_fun_tparam_return() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function f<X, Y: X = number>(x: X, y: Y): Y {
  (x: Y);
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 17) to (2, 18) => {
    (2, 11) to (2, 12): (`X`)
  };
  (2, 32) to (2, 33) => {
    (2, 11) to (2, 12): (`X`)
  };
  (2, 38) to (2, 39) => {
    (2, 14) to (2, 15): (`Y`)
  };
  (2, 42) to (2, 43) => {
    (2, 14) to (2, 15): (`Y`)
  };
  (3, 3) to (3, 4) => {
    (2, 29) to (2, 30): (`x`)
  };
  (3, 6) to (3, 7) => {
    (2, 14) to (2, 15): (`Y`)
  }]"#
    );
}

#[test]
fn test_fun_tparam_global_bound() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function f<Z extends G>() { }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 21) to (2, 22) => {
    Global G
  }]"#
    );
}

#[test]
fn test_fun_tparam_self_bound() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function f<Z extends Z>() { }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 21) to (2, 22) => {
    (2, 11) to (2, 12): (`Z`)
  }]"#
    );
}

#[test]
fn test_fun_inline_tparam() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = function f<X, Y: X = number>(x: X, y: Y) {
  (x: Y);
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 25) to (2, 26) => {
    (2, 19) to (2, 20): (`X`)
  };
  (2, 40) to (2, 41) => {
    (2, 19) to (2, 20): (`X`)
  };
  (2, 46) to (2, 47) => {
    (2, 22) to (2, 23): (`Y`)
  };
  (3, 3) to (3, 4) => {
    (2, 37) to (2, 38): (`x`)
  };
  (3, 6) to (3, 7) => {
    (2, 22) to (2, 23): (`Y`)
  }]"#
    );
}

#[test]
fn test_arrow_fun_tparam() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = <X, Y: X = number>(x: X, y: Y) => {
  (x: Y);
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 15) to (2, 16) => {
    (2, 9) to (2, 10): (`X`)
  };
  (2, 30) to (2, 31) => {
    (2, 9) to (2, 10): (`X`)
  };
  (2, 36) to (2, 37) => {
    (2, 12) to (2, 13): (`Y`)
  };
  (3, 3) to (3, 4) => {
    (2, 27) to (2, 28): (`x`)
  };
  (3, 6) to (3, 7) => {
    (2, 12) to (2, 13): (`Y`)
  }]"#
    );
}

#[test]
fn test_type_tparam() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
type T<X, Y: X = number> = Array<[X, Y]>
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 13) to (2, 14) => {
    (2, 7) to (2, 8): (`X`)
  };
  (2, 27) to (2, 32) => {
    Global Array
  };
  (2, 34) to (2, 35) => {
    (2, 7) to (2, 8): (`X`)
  };
  (2, 37) to (2, 38) => {
    (2, 10) to (2, 11): (`Y`)
  }]"#
    );
}

#[test]
fn test_opaque_tparam() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
opaque type T<X>: X = X
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 18) to (2, 19) => {
    (2, 14) to (2, 15): (`X`)
  };
  (2, 22) to (2, 23) => {
    (2, 14) to (2, 15): (`X`)
  }]"#
    );
}

#[test]
fn test_opaque_tparam_2() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
declare opaque type T<X>: X;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 26) to (2, 27) => {
    (2, 22) to (2, 23): (`X`)
  }]"#
    );
}

#[test]
fn test_interface_tparam() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
interface T<X, Y:X> {
  x: X;
  y: Y;
};
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 17) to (2, 18) => {
    (2, 12) to (2, 13): (`X`)
  };
  (3, 5) to (3, 6) => {
    (2, 12) to (2, 13): (`X`)
  };
  (4, 5) to (4, 6) => {
    (2, 15) to (2, 16): (`Y`)
  }]"#
    );
}

#[test]
fn test_interface_miss() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
interface T<X> extends X { };
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 23) to (2, 24) => {
    Global X
  }]"#
    );
}

#[test]
fn test_interface_extends_tparam() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
type X<S> = S;

interface T<X> extends X<X> {
  x: X
};
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 12) to (2, 13) => {
    (2, 7) to (2, 8): (`S`)
  };
  (4, 23) to (4, 24) => {
    (2, 5) to (2, 6): (`X`)
  };
  (4, 25) to (4, 26) => {
    (4, 12) to (4, 13): (`X`)
  };
  (5, 5) to (5, 6) => {
    (4, 12) to (4, 13): (`X`)
  }]"#
    );
}

#[test]
fn test_fun_type_tparam() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
type T<W> = <X: W, Y: X>(X) => Y
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 16) to (2, 17) => {
    (2, 7) to (2, 8): (`W`)
  };
  (2, 22) to (2, 23) => {
    (2, 13) to (2, 14): (`X`)
  };
  (2, 25) to (2, 26) => {
    (2, 13) to (2, 14): (`X`)
  };
  (2, 31) to (2, 32) => {
    (2, 19) to (2, 20): (`Y`)
  }]"#
    );
}

#[test]
fn test_class_tparam() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
class C<X, Y:X> extends X<X> implements Y<Y> {
  f<Z:Y>(x:X, y:Y) {
    let z: Z;
  }
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 13) to (2, 14) => {
    (2, 8) to (2, 9): (`X`)
  };
  (2, 24) to (2, 25) => {
    Global X
  };
  (2, 26) to (2, 27) => {
    (2, 8) to (2, 9): (`X`)
  };
  (2, 40) to (2, 41) => {
    Global Y
  };
  (2, 42) to (2, 43) => {
    (2, 11) to (2, 12): (`Y`)
  };
  (3, 6) to (3, 7) => {
    (2, 11) to (2, 12): (`Y`)
  };
  (3, 11) to (3, 12) => {
    (2, 8) to (2, 9): (`X`)
  };
  (3, 16) to (3, 17) => {
    (2, 11) to (2, 12): (`Y`)
  };
  (4, 11) to (4, 12) => {
    (3, 4) to (3, 5): (`Z`)
  }]"#
    );
}

#[test]
fn test_declare_class_tparam() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
declare class C<X, Y:X, Z = X> extends X<X> mixins Z<Z> implements Y<Y> {
  f<Z:Y>(X, Y): Z;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 21) to (2, 22) => {
    (2, 16) to (2, 17): (`X`)
  };
  (2, 28) to (2, 29) => {
    (2, 16) to (2, 17): (`X`)
  };
  (2, 39) to (2, 40) => {
    Global X
  };
  (2, 41) to (2, 42) => {
    (2, 16) to (2, 17): (`X`)
  };
  (2, 51) to (2, 52) => {
    Global Z
  };
  (2, 53) to (2, 54) => {
    (2, 24) to (2, 25): (`Z`)
  };
  (2, 67) to (2, 68) => {
    Global Y
  };
  (2, 69) to (2, 70) => {
    (2, 19) to (2, 20): (`Y`)
  };
  (3, 6) to (3, 7) => {
    (2, 19) to (2, 20): (`Y`)
  };
  (3, 9) to (3, 10) => {
    (2, 16) to (2, 17): (`X`)
  };
  (3, 12) to (3, 13) => {
    (2, 19) to (2, 20): (`Y`)
  };
  (3, 16) to (3, 17) => {
    (3, 4) to (3, 5): (`Z`)
  }]"#
    );
}

#[test]
fn test_class_expr_tparam() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
var w = class <X, Y:X> extends X<X> implements Y<Y> {
  f<Z:Y>(x:X, y:Y) {
    let z: Z;
  }
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 20) to (2, 21) => {
    (2, 15) to (2, 16): (`X`)
  };
  (2, 31) to (2, 32) => {
    Global X
  };
  (2, 33) to (2, 34) => {
    (2, 15) to (2, 16): (`X`)
  };
  (2, 47) to (2, 48) => {
    Global Y
  };
  (2, 49) to (2, 50) => {
    (2, 18) to (2, 19): (`Y`)
  };
  (3, 6) to (3, 7) => {
    (2, 18) to (2, 19): (`Y`)
  };
  (3, 11) to (3, 12) => {
    (2, 15) to (2, 16): (`X`)
  };
  (3, 16) to (3, 17) => {
    (2, 18) to (2, 19): (`Y`)
  };
  (4, 11) to (4, 12) => {
    (3, 4) to (3, 5): (`Z`)
  }]"#
    );
}

#[test]
fn test_import_type() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
var x: S;
import { type S } from '';
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 7) to (2, 8) => {
    (3, 14) to (3, 15): (`S`)
  }]"#
    );
}

#[test]
fn test_import_mix() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
var x: S = t;
var a: W;
import { type S, t, w as y, typeof w as W } from '';
t;
y;
w;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 7) to (2, 8) => {
    (4, 14) to (4, 15): (`S`)
  };
  (2, 11) to (2, 12) => {
    (undeclared)
  };
  (3, 7) to (3, 8) => {
    (4, 40) to (4, 41): (`W`)
  };
  (5, 0) to (5, 1) => {
    (4, 17) to (4, 18): (`t`)
  };
  (6, 0) to (6, 1) => {
    (4, 25) to (4, 26): (`y`)
  };
  (7, 0) to (7, 1) => {
    Global w
  }]"#
    );
}

#[test]
fn test_import_def() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
(NS: NST);
(ps: ns);
(ps: ms);
(1: a);
(2: b);
import type {a, b} from ''
import typeof ns from '';
import type ms from '';
import ps from ''
import * as NS from ''
import typeof * as NST from ''
(NS: NST);
(ps: ns);
(ps: ms);
(1: a);
(2: b);
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 3) => {
    (undeclared)
  };
  (2, 5) to (2, 8) => {
    (12, 19) to (12, 22): (`NST`)
  };
  (3, 1) to (3, 3) => {
    (undeclared)
  };
  (3, 5) to (3, 7) => {
    (8, 14) to (8, 16): (`ns`)
  };
  (4, 1) to (4, 3) => {
    (undeclared)
  };
  (4, 5) to (4, 7) => {
    (9, 12) to (9, 14): (`ms`)
  };
  (5, 4) to (5, 5) => {
    (7, 13) to (7, 14): (`a`)
  };
  (6, 4) to (6, 5) => {
    (7, 16) to (7, 17): (`b`)
  };
  (13, 1) to (13, 3) => {
    (11, 12) to (11, 14): (`NS`)
  };
  (13, 5) to (13, 8) => {
    (12, 19) to (12, 22): (`NST`)
  };
  (14, 1) to (14, 3) => {
    (10, 7) to (10, 9): (`ps`)
  };
  (14, 5) to (14, 7) => {
    (8, 14) to (8, 16): (`ns`)
  };
  (15, 1) to (15, 3) => {
    (10, 7) to (10, 9): (`ps`)
  };
  (15, 5) to (15, 7) => {
    (9, 12) to (9, 14): (`ms`)
  };
  (16, 4) to (16, 5) => {
    (7, 13) to (7, 14): (`a`)
  };
  (17, 4) to (17, 5) => {
    (7, 16) to (7, 17): (`b`)
  }]"#
    );
}

#[test]
fn test_inc() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = 0;
++x;
x++;
x;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 2) to (3, 3) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 0) to (4, 1) => {
    (3, 2) to (3, 3): (`x`)
  };
  (5, 0) to (5, 1) => {
    (4, 0) to (4, 1): (`x`)
  }]"#
    );
}

#[test]
fn test_inc_heap() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = { a: 0 };
++x.a;
x.a++;
x.a;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 2) to (3, 3) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 0) to (4, 1) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 0) to (4, 3) => {
    number
  };
  (5, 0) to (5, 1) => {
    (2, 4) to (2, 5): (`x`)
  };
  (5, 0) to (5, 3) => {
    number
  }]"#
    );
}

#[test]
fn test_op_assign_heap() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = { a: 0 };
x.a += 42;
x.a;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 0) to (3, 1) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 0) to (4, 1) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 0) to (4, 3) => {
    (3, 0) to (3, 3): ()
  }]"#
    );
}

#[test]
fn test_class1() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
(C: void); // as a value, C is undefined and in the TDZ

declare var c: C; // as a type, C refers to the class below
c.foo;

class C {
    foo: number;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 2) => {
    (undeclared)
  };
  (4, 15) to (4, 16) => {
    (7, 6) to (7, 7): (`C`)
  };
  (5, 0) to (5, 1) => {
    (4, 12) to (4, 13): (`c`)
  }]"#
    );
}

#[test]
fn test_class2() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
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
        r#"[
  (3, 7) to (3, 8) => {
    (5, 6) to (5, 7): (`D`)
  };
  (5, 16) to (5, 17) => {
    (2, 6) to (2, 7): (`C`)
  }]"#
    );
}

#[test]
fn test_class3() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
C;
class C {
  x: C;
}
C;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 0) to (2, 1) => {
    (undeclared)
  };
  (4, 5) to (4, 6) => {
    (3, 6) to (3, 7): (`C`)
  };
  (6, 0) to (6, 1) => {
    (3, 6) to (3, 7): (`C`)
  }]"#
    );
}

#[test]
fn test_class4() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"

function havoced() {
  C;
}

class C {
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (4, 2) to (4, 3) => {
    (7, 6) to (7, 7): (`C`)
  }]"#
    );
}

#[test]
fn test_class5() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"

function havoc() {
  C = 42;
}

havoc();
C;

class C {
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (7, 0) to (7, 5) => {
    (3, 9) to (3, 14): (`havoc`)
  };
  (8, 0) to (8, 1) => {
    (undeclared)
  }]"#
    );
}

#[test]
fn test_deps_recur_broken_init() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
type T = number;
let x: T;
function f() {
  x = x;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 7) to (3, 8) => {
    (2, 5) to (2, 6): (`T`)
  };
  (5, 6) to (5, 7) => {
    (3, 4) to (3, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_class3_2() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
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
        r#"[
  (3, 7) to (3, 8) => {
    (5, 6) to (5, 7): (`D`)
  };
  (5, 16) to (5, 17) => {
    (2, 6) to (2, 7): (`C`)
  }]"#
    );
}

#[test]
fn test_enum() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function havoced() {
  var x: E = E.Foo
}
enum E {
  Foo
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 9) to (3, 10) => {
    (5, 5) to (5, 6): (`E`)
  };
  (3, 13) to (3, 14) => {
    (5, 5) to (5, 6): (`E`)
  }]"#
    );
}

#[test]
fn test_react_jsx() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
const React = require('react');

<div />;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 14) to (2, 21) => {
    Global require
  };
  (4, 0) to (4, 7) => {
    (2, 6) to (2, 11): (`React`)
  };
  (4, 1) to (4, 4) => {
    Global div
  }]"#
    );
}

#[test]
fn test_unreachable_jsx() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
const React = require('react');
throw new Error();
<Component />;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 14) to (2, 21) => {
    Global require
  };
  (3, 10) to (3, 15) => {
    Global Error
  };
  (4, 0) to (4, 13) => {
    unreachable
  };
  (4, 1) to (4, 10) => {
    unreachable
  }]"#
    );
}

#[test]
fn test_custom_jsx_pragma() {
    let result = print_ssa_test(
        Some("createMikesCoolElement"),
        false,
        None,
        None,
        r#"
  let createMikesCoolElement = (null: any);
<FirstElement />
function time_to_create_some_elements_bro() {
  <SecondElement />
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 0) to (3, 16) => {
    (2, 6) to (2, 28): (`createMikesCoolElement`)
  };
  (3, 1) to (3, 13) => {
    Global FirstElement
  };
  (5, 2) to (5, 19) => {
    (2, 6) to (2, 28): (`createMikesCoolElement`)
  };
  (5, 3) to (5, 16) => {
    Global SecondElement
  }]"#
    );
}

#[test]
fn test_jsx_pragma_member_expr() {
    let result = print_ssa_test(
        Some("Test.f"),
        false,
        None,
        None,
        r#"
function Component() {}
<Component />
"#,
    );
    assert_eq!(
        result,
        r#"[
  (1, 0) to (1, 4) => {
    Global Test
  };
  (3, 1) to (3, 10) => {
    (2, 9) to (2, 18): (`Component`)
  }]"#
    );
}

#[test]
fn test_automatic_react_runtime() {
    let result = print_ssa_test(
        None,
        true,
        None,
        None,
        r#"
  let createMikesCoolElement = (null: any);
<FirstElement />
function time_to_create_some_elements_bro() {
  <SecondElement />
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 1) to (3, 13) => {
    Global FirstElement
  };
  (5, 3) to (5, 16) => {
    Global SecondElement
  }]"#
    );
}

#[test]
fn test_react_jsx_2() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
const React = require('react');

<div />;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 14) to (2, 21) => {
    Global require
  };
  (4, 0) to (4, 7) => {
    (2, 6) to (2, 11): (`React`)
  };
  (4, 1) to (4, 4) => {
    Global div
  }]"#
    );
}

#[test]
fn test_unreachable_jsx_2() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
const React = require('react');
throw new Error();
<Component />;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 14) to (2, 21) => {
    Global require
  };
  (3, 10) to (3, 15) => {
    Global Error
  };
  (4, 0) to (4, 13) => {
    unreachable
  };
  (4, 1) to (4, 10) => {
    unreachable
  }]"#
    );
}

#[test]
fn test_switch_reread_discriminant() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let y = {};
switch (y.x) { // Does not report a Projection
    case 'ONE': break;
    case 'TWO': break;
    default:
      (y.x: empty);
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 8) to (3, 9) => {
    (2, 4) to (2, 5): (`y`)
  };
  (4, 4) to (4, 22) => {
    (2, 4) to (2, 5): (`y`)
  };
  (5, 4) to (5, 22) => {
    {refinement = Not (SentinelR x); writes = (2, 4) to (2, 5): (`y`)}
  };
  (7, 7) to (7, 8) => {
    {refinement = Not (SentinelR x); writes = {refinement = Not (SentinelR x); writes = (2, 4) to (2, 5): (`y`)}}
  };
  (7, 7) to (7, 10) => {
    {refinement = Not (TWO); writes = {refinement = Not (ONE); writes = projection at (3, 8) to (3, 11)}}
  }]"#
    );
}

#[test]
fn test_no_refinement_write_on_indexed() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = {};
let y = 'str';
x[y] = 3;
x[y]; // Should not report an entry
"#,
    );
    assert_eq!(
        result,
        r#"[
  (4, 0) to (4, 1) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 2) to (4, 3) => {
    (3, 4) to (3, 5): (`y`)
  };
  (5, 0) to (5, 1) => {
    (2, 4) to (2, 5): (`x`)
  };
  (5, 2) to (5, 3) => {
    (3, 4) to (3, 5): (`y`)
  }]"#
    );
}

#[test]
fn test_switch_no_default() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let y = 'ONE';
switch (y) {
    case 'ONE': break;
    case 'TWO': break;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 0) to (6, 1) => {
    {refinement = Not (TWO); writes = {refinement = Not (ONE); writes = (2, 4) to (2, 5): (`y`)}}
  };
  (3, 8) to (3, 9) => {
    (2, 4) to (2, 5): (`y`)
  }]"#
    );
}

#[test]
fn test_switch_exhaustive_return() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let obj = {};
  switch (obj.k) {
    case 'a':
      throw 0;
    case 'b':
      throw 1;
  }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 2) to (8, 3) => {
    {refinement = Not (b); writes = {refinement = Not (a); writes = projection at (3, 10) to (3, 15)}}
  };
  (3, 10) to (3, 13) => {
    (2, 4) to (2, 7): (`obj`)
  };
  (4, 4) to (5, 14) => {
    (2, 4) to (2, 7): (`obj`)
  };
  (6, 4) to (7, 14) => {
    {refinement = Not (SentinelR k); writes = (2, 4) to (2, 7): (`obj`)}
  }]"#
    );
}

#[test]
fn test_switch_exhaustive_fallthrough_return() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let obj = {};
  switch (obj.k) {
    case 'a':
    case 'b':
      throw 1;
  }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 2) to (7, 3) => {
    {refinement = Not (b); writes = {refinement = Not (a); writes = projection at (3, 10) to (3, 15)}}
  };
  (3, 10) to (3, 13) => {
    (2, 4) to (2, 7): (`obj`)
  };
  (4, 4) to (4, 13) => {
    (2, 4) to (2, 7): (`obj`)
  };
  (5, 4) to (6, 14) => {
    {refinement = Not (SentinelR k); writes = (2, 4) to (2, 7): (`obj`)}
  }]"#
    );
}

#[test]
fn test_reference_before_declaration() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
  _const;
  _let;
  _var;
  _func1;
  _func2;
  _func3;
  _func4;
  _class;
  E;

  const _const = 3;
  let _let = 3;
  var _var = 3;
  function _func1() {}
  export function _func2() {}
  export default function _func3() {}
  declare export function _func4(): void
class _class {}
  enum E { A }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 2) to (2, 8) => {
    (undeclared)
  };
  (3, 2) to (3, 6) => {
    (undeclared)
  };
  (4, 2) to (4, 6) => {
    (uninitialized)
  };
  (5, 2) to (5, 8) => {
    (15, 11) to (15, 17): (`_func1`)
  };
  (6, 2) to (6, 8) => {
    (16, 18) to (16, 24): (`_func2`)
  };
  (7, 2) to (7, 8) => {
    (17, 26) to (17, 32): (`_func3`)
  };
  (8, 2) to (8, 8) => {
    declared function (18, 26) to (18, 32)
  };
  (9, 2) to (9, 8) => {
    (undeclared)
  };
  (10, 2) to (10, 3) => {
    (undeclared)
  }]"#
    );
}

#[test]
fn test_declaration_declares_undeclared() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
x;
x = 3; // Does not count as a write until LHS is declared
x;
let x;
x;
x = 3;
x;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 0) to (2, 1) => {
    (undeclared)
  };
  (4, 0) to (4, 1) => {
    (undeclared)
  };
  (6, 0) to (6, 1) => {
    (uninitialized)
  };
  (8, 0) to (8, 1) => {
    (7, 0) to (7, 1): (`x`)
  }]"#
    );
}

#[test]
fn test_undeclared_havoc_no_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
console.log('foo');
x;
let x;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 0) to (2, 7) => {
    Global console
  };
  (3, 0) to (3, 1) => {
    (undeclared)
  }]"#
    );
}

#[test]
fn test_undeclared_havoc_function_writes() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function f() { x = 3 }
console.log('foo');
x;
let x;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 0) to (3, 7) => {
    Global console
  };
  (4, 0) to (4, 1) => {
    (undeclared)
  }]"#
    );
}

#[test]
fn test_undeclared_enter_function_scope() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function f() { x; }
let x;
x = 3;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 15) to (2, 16) => {
    (4, 0) to (4, 1): (`x`)
  }]"#
    );
}

#[test]
fn test_default_switch_refinement() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
type Enum = 'ONE' | 'TWO';
type Selection = { x: 'ONE' } | { x: 'TWO' } | { x: 'NONE' }

type Rule = {
  x: Enum,
  y: Selection,
}

function foo(r: Rule) {
  const x = r.x;
  const y = r.y;
  if (y.x === x) {
    switch (y.x) {
      case 'ONE': break;
      case 'TWO': break;
      default: (y.x: empty);
    }
  }
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (6, 5) to (6, 9) => {
    (2, 5) to (2, 9): (`Enum`)
  };
  (7, 5) to (7, 14) => {
    (3, 5) to (3, 14): (`Selection`)
  };
  (10, 16) to (10, 20) => {
    (5, 5) to (5, 9): (`Rule`)
  };
  (11, 12) to (11, 13) => {
    (10, 13) to (10, 14): (`r`)
  };
  (12, 12) to (12, 13) => {
    (10, 13) to (10, 14): (`r`)
  };
  (13, 6) to (13, 7) => {
    (12, 8) to (12, 9): (`y`)
  };
  (13, 14) to (13, 15) => {
    (11, 8) to (11, 9): (`x`)
  };
  (14, 12) to (14, 13) => {
    {refinement = SentinelR x; writes = (12, 8) to (12, 9): (`y`)}
  };
  (14, 12) to (14, 15) => {
    {refinement = EqR; writes = projection at (13, 6) to (13, 9)}
  };
  (15, 6) to (15, 24) => {
    {refinement = SentinelR x; writes = (12, 8) to (12, 9): (`y`)}
  };
  (16, 6) to (16, 24) => {
    {refinement = Not (SentinelR x); writes = {refinement = SentinelR x; writes = (12, 8) to (12, 9): (`y`)}}
  };
  (17, 16) to (17, 17) => {
    {refinement = Not (SentinelR x); writes = {refinement = Not (SentinelR x); writes = {refinement = SentinelR x; writes = (12, 8) to (12, 9): (`y`)}}}
  };
  (17, 16) to (17, 19) => {
    {refinement = Not (TWO); writes = {refinement = Not (ONE); writes = {refinement = EqR; writes = projection at (13, 6) to (13, 9)}}}
  }]"#
    );
}

#[test]
fn test_prop_exists() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
const x = {foo: 3};

if (x.foo) {
  x;
  x.foo;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (4, 4) to (4, 5) => {
    (2, 6) to (2, 7): (`x`)
  };
  (5, 2) to (5, 3) => {
    {refinement = PropTruthyR (foo); writes = (2, 6) to (2, 7): (`x`)}
  };
  (6, 2) to (6, 3) => {
    {refinement = PropTruthyR (foo); writes = (2, 6) to (2, 7): (`x`)}
  };
  (6, 2) to (6, 7) => {
    {refinement = Truthy; writes = projection at (4, 4) to (4, 9)}
  }]"#
    );
}

#[test]
fn test_try_catch_env_merging() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function maz2() {
  let x: number[] = [];
  try {
    throw new Error(`just capturing a stack trace`);
  } catch (e) {
    var a: string = x[0];
  }
  var c: string = x[0]; // reachable
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 14) to (5, 19) => {
    Global Error
  };
  (7, 20) to (7, 21) => {
    (3, 6) to (3, 7): (`x`)
  };
  (9, 18) to (9, 19) => {
    (3, 6) to (3, 7): (`x`)
  }]"#
    );
}

#[test]
fn test_try_catch_catch_throws() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function bar(response) {
    var payload;
    try {
        payload = JSON.parse(response);
    } catch (e) {
        throw new Error('...');
    }
    // here via [try] only.
    if (payload.error) {    // ok
        // ...
    }
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 18) to (5, 22) => {
    Global JSON
  };
  (5, 29) to (5, 37) => {
    (2, 13) to (2, 21): (`response`)
  };
  (7, 18) to (7, 23) => {
    Global Error
  };
  (10, 8) to (10, 15) => {
    (5, 8) to (5, 15): (`payload`)
  }]"#
    );
}

#[test]
fn test_try_catch_throw_in_both_then_finally() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function bar(response) {
    var payload;
    try {
      throw new Error();
    } catch (e) {
      throw new Error();
    } finally {
      payload = 3;
    }
    payload; // Dead
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 16) to (5, 21) => {
    Global Error
  };
  (7, 16) to (7, 21) => {
    Global Error
  };
  (11, 4) to (11, 11) => {
    unreachable
  }]"#
    );
}

#[test]
fn test_try_throw_catch_finally() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function bar(response) {
    var payload;
    try {
      throw new Error();
    } catch (e) {
      payload = 4;
    } finally {
      payload;
    }
    payload;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 16) to (5, 21) => {
    Global Error
  };
  (9, 6) to (9, 13) => {
    (uninitialized),
    (7, 6) to (7, 13): (`payload`)
  };
  (11, 4) to (11, 11) => {
    (7, 6) to (7, 13): (`payload`)
  }]"#
    );
}

#[test]
fn test_try_catch_throw_finally() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function bar(response) {
    let payload;
    try {
      payload = 3;
    } catch (e) {
      payload = 4;
      throw new Error();
    } finally {
      payload;
    }
    payload; // = 3
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (8, 16) to (8, 21) => {
    Global Error
  };
  (10, 6) to (10, 13) => {
    (uninitialized),
    (5, 6) to (5, 13): (`payload`),
    (7, 6) to (7, 13): (`payload`)
  };
  (12, 4) to (12, 11) => {
    (5, 6) to (5, 13): (`payload`)
  }]"#
    );
}

#[test]
fn test_try_catch_finally_throw() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function bar(response) {
    let payload;
    try {
      payload = 3;
    } catch (e) {
      payload = 4;
    } finally {
      payload = 5;
      throw new Error();
    }
    payload; // Dead
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (10, 16) to (10, 21) => {
    Global Error
  };
  (12, 4) to (12, 11) => {
    unreachable
  }]"#
    );
}

#[test]
fn test_try_throw_catch_throw_finally_throw() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function bar(response) {
    let payload;
    try {
      payload = 3;
      throw new Error()
    } catch (e) {
      payload = 4;
      throw new Error()
    } finally {
      payload = 5;
      throw new Error();
    }
    payload; // Dead
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (6, 16) to (6, 21) => {
    Global Error
  };
  (9, 16) to (9, 21) => {
    Global Error
  };
  (12, 16) to (12, 21) => {
    Global Error
  };
  (14, 4) to (14, 11) => {
    unreachable
  }]"#
    );
}

#[test]
fn test_try_throw_catch_throw_finally() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function bar(response) {
    let payload;
    try {
      try {
        payload = 3;
        throw new Error()
      } catch (e) {
        payload = 4;
        throw new Error()
      } finally {
        payload = 5;
      }
      payload; // Dead
    } catch (e) {
      payload; // = 5 | uninitialized
    }
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (7, 18) to (7, 23) => {
    Global Error
  };
  (10, 18) to (10, 23) => {
    Global Error
  };
  (14, 6) to (14, 13) => {
    unreachable
  };
  (16, 6) to (16, 13) => {
    (uninitialized),
    (12, 8) to (12, 15): (`payload`)
  }]"#
    );
}

#[test]
fn test_exports_special() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
exports.foo = 1;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 0) to (2, 7) => {
    Global exports
  }]"#
    );
}

#[test]
fn test_module_dot_export_special() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
module.exports;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 0) to (2, 6) => {
    Global module
  };
  (2, 0) to (2, 14) => {
    Global exports
  }]"#
    );
}

#[test]
fn test_exports_as_global() {
    let result = print_ssa_test(
        None,
        false,
        Some(true),
        None,
        r#"
exports.foo = 1;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 0) to (2, 7) => {
    Global exports
  }]"#
    );
}

#[test]
fn test_module_dot_export_as_global() {
    let result = print_ssa_test(
        None,
        false,
        Some(true),
        None,
        r#"
module.exports;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 0) to (2, 6) => {
    Global module
  }]"#
    );
}

#[test]
fn test_import_havoc() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
import {func} from './a';

function f() {
  func();
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 2) to (5, 6) => {
    (2, 8) to (2, 12): (`func`)
  }]"#
    );
}

#[test]
fn test_test27() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if (!x.a) { x.c; } else { x.b; }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 5) to (2, 6) => {
    Global x
  };
  (2, 12) to (2, 13) => {
    {refinement = Not (PropTruthyR (a)); writes = Global x}
  };
  (2, 26) to (2, 27) => {
    {refinement = PropTruthyR (a); writes = Global x}
  }]"#
    );
}

#[test]
fn test_conjunct() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if (x.a && x.b)
  { x.a; x.b }
else
  { x.a; x.b }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => {
    Global x
  };
  (2, 11) to (2, 12) => {
    {refinement = PropTruthyR (a); writes = Global x}
  };
  (3, 4) to (3, 5) => {
    {refinement = And (PropTruthyR (a), PropTruthyR (b)); writes = Global x}
  };
  (3, 4) to (3, 7) => {
    {refinement = Truthy; writes = projection at (2, 4) to (2, 7)}
  };
  (3, 9) to (3, 10) => {
    {refinement = And (PropTruthyR (a), PropTruthyR (b)); writes = Global x}
  };
  (3, 9) to (3, 12) => {
    {refinement = Truthy; writes = projection at (2, 11) to (2, 14)}
  };
  (5, 4) to (5, 5) => {
    {refinement = Or (Not (PropTruthyR (a)), Not (PropTruthyR (b))); writes = Global x}
  };
  (5, 9) to (5, 10) => {
    {refinement = Or (Not (PropTruthyR (a)), Not (PropTruthyR (b))); writes = Global x}
  }]"#
    );
}

#[test]
fn test_disjunct() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if (x.a || x.b)
  { x.a; x.b }
else
  { x.a; x.b }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => {
    Global x
  };
  (2, 11) to (2, 12) => {
    {refinement = Not (PropTruthyR (a)); writes = Global x}
  };
  (3, 4) to (3, 5) => {
    {refinement = Or (PropTruthyR (a), PropTruthyR (b)); writes = Global x}
  };
  (3, 9) to (3, 10) => {
    {refinement = Or (PropTruthyR (a), PropTruthyR (b)); writes = Global x}
  };
  (5, 4) to (5, 5) => {
    {refinement = And (Not (PropTruthyR (a)), Not (PropTruthyR (b))); writes = Global x}
  };
  (5, 4) to (5, 7) => {
    {refinement = Not (Truthy); writes = projection at (2, 4) to (2, 7)}
  };
  (5, 9) to (5, 10) => {
    {refinement = And (Not (PropTruthyR (a)), Not (PropTruthyR (b))); writes = Global x}
  };
  (5, 9) to (5, 12) => {
    {refinement = Not (Truthy); writes = projection at (2, 11) to (2, 14)}
  }]"#
    );
}

#[test]
fn test_complex() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if ((x.a || x.b) && x.c)
  { x.a; x.b; x.c }
else
  { x.a; x.b; x.c }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 5) to (2, 6) => {
    Global x
  };
  (2, 12) to (2, 13) => {
    {refinement = Not (PropTruthyR (a)); writes = Global x}
  };
  (2, 20) to (2, 21) => {
    {refinement = Or (PropTruthyR (a), PropTruthyR (b)); writes = Global x}
  };
  (3, 4) to (3, 5) => {
    {refinement = And (Or (PropTruthyR (a), PropTruthyR (b)), PropTruthyR (c)); writes = Global x}
  };
  (3, 9) to (3, 10) => {
    {refinement = And (Or (PropTruthyR (a), PropTruthyR (b)), PropTruthyR (c)); writes = Global x}
  };
  (3, 14) to (3, 15) => {
    {refinement = And (Or (PropTruthyR (a), PropTruthyR (b)), PropTruthyR (c)); writes = Global x}
  };
  (3, 14) to (3, 17) => {
    {refinement = Truthy; writes = projection at (2, 20) to (2, 23)}
  };
  (5, 4) to (5, 5) => {
    {refinement = Or (And (Not (PropTruthyR (a)), Not (PropTruthyR (b))), Not (PropTruthyR (c))); writes = Global x}
  };
  (5, 9) to (5, 10) => {
    {refinement = Or (And (Not (PropTruthyR (a)), Not (PropTruthyR (b))), Not (PropTruthyR (c))); writes = Global x}
  };
  (5, 14) to (5, 15) => {
    {refinement = Or (And (Not (PropTruthyR (a)), Not (PropTruthyR (b))), Not (PropTruthyR (c))); writes = Global x}
  }]"#
    );
}

#[test]
fn test_changeset() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if (x && x.a)
  { x.a; }
else
  { x.a; }
x.a;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => {
    Global x
  };
  (2, 9) to (2, 10) => {
    {refinement = Truthy; writes = Global x}
  };
  (3, 4) to (3, 5) => {
    {refinement = And (Truthy, PropTruthyR (a)); writes = Global x}
  };
  (3, 4) to (3, 7) => {
    {refinement = Truthy; writes = projection at (2, 9) to (2, 12)}
  };
  (5, 4) to (5, 5) => {
    {refinement = Or (Not (Truthy), Not (PropTruthyR (a))); writes = Global x}
  };
  (6, 0) to (6, 1) => {
    Global x
  }]"#
    );
}

#[test]
fn test_no_changeset() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if (x.a)
  { x.a; }
else
  { x.a; }
x.a;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => {
    Global x
  };
  (3, 4) to (3, 5) => {
    {refinement = PropTruthyR (a); writes = Global x}
  };
  (3, 4) to (3, 7) => {
    {refinement = Truthy; writes = projection at (2, 4) to (2, 7)}
  };
  (5, 4) to (5, 5) => {
    {refinement = Not (PropTruthyR (a)); writes = Global x}
  };
  (5, 4) to (5, 7) => {
    {refinement = Not (Truthy); writes = projection at (2, 4) to (2, 7)}
  };
  (6, 0) to (6, 1) => {
    Global x
  }]"#
    );
}

#[test]
fn test_changeset_update() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if (x.a)
  { x.a = 42 }
x.a;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => {
    Global x
  };
  (3, 4) to (3, 5) => {
    {refinement = PropTruthyR (a); writes = Global x}
  };
  (4, 0) to (4, 1) => {
    Global x
  };
  (4, 0) to (4, 3) => {
    (3, 4) to (3, 7): (),
    {refinement = Not (Truthy); writes = projection at (2, 4) to (2, 7)}
  }]"#
    );
}

#[test]
fn test_changeset_pre_exist() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if(x && x.a) {
  if(x && x.a) {}
  else {
    x.a;
  }
  x.a;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 3) to (2, 4) => {
    Global x
  };
  (2, 8) to (2, 9) => {
    {refinement = Truthy; writes = Global x}
  };
  (3, 5) to (3, 6) => {
    {refinement = And (Truthy, PropTruthyR (a)); writes = Global x}
  };
  (3, 10) to (3, 11) => {
    {refinement = Truthy; writes = {refinement = And (Truthy, PropTruthyR (a)); writes = Global x}}
  };
  (3, 10) to (3, 13) => {
    {refinement = Truthy; writes = projection at (2, 8) to (2, 11)}
  };
  (5, 4) to (5, 5) => {
    {refinement = Or (Not (Truthy), Not (PropTruthyR (a))); writes = {refinement = And (Truthy, PropTruthyR (a)); writes = Global x}}
  };
  (5, 4) to (5, 7) => {
    {refinement = Truthy; writes = projection at (2, 8) to (2, 11)}
  };
  (7, 2) to (7, 3) => {
    {refinement = And (Truthy, PropTruthyR (a)); writes = Global x}
  };
  (7, 2) to (7, 5) => {
    {refinement = Truthy; writes = projection at (2, 8) to (2, 11)}
  }]"#
    );
}

#[test]
fn test_optional_refi() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
var x: ?Array<number> = null;
x?.[x[0]];
if(x?.[x[0]]) { x; }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 8) to (2, 13) => {
    Global Array
  };
  (3, 0) to (3, 1) => {
    (2, 4) to (2, 5): (`x`)
  };
  (3, 4) to (3, 5) => {
    {refinement = Not (Maybe); writes = (2, 4) to (2, 5): (`x`)}
  };
  (4, 3) to (4, 4) => {
    (2, 4) to (2, 5): (`x`)
  };
  (4, 7) to (4, 8) => {
    {refinement = Not (Maybe); writes = (2, 4) to (2, 5): (`x`)}
  };
  (4, 16) to (4, 17) => {
    {refinement = Not (Maybe); writes = (2, 4) to (2, 5): (`x`)}
  }]"#
    );
}

#[test]
fn test_optional_refi2() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
declare var x: ?{y: {f: (mixed) => void, z?: {w: mixed => void}}};
x?.y.f(x);
x?.y.z?.w(x.y.z);
(x?.y).f(x);
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 0) to (3, 1) => {
    (2, 12) to (2, 13): (`x`)
  };
  (3, 7) to (3, 8) => {
    {refinement = And (Not (Maybe), Not (PropNullishR y)); writes = (2, 12) to (2, 13): (`x`)}
  };
  (4, 0) to (4, 1) => {
    (2, 12) to (2, 13): (`x`)
  };
  (4, 10) to (4, 11) => {
    {refinement = And (Not (Maybe), Not (PropNullishR y)); writes = (2, 12) to (2, 13): (`x`)}
  };
  (4, 10) to (4, 13) => {
    {refinement = Not (PropNullishR z); writes = projection at (4, 0) to (4, 4)}
  };
  (4, 10) to (4, 15) => {
    {refinement = And (Not (Maybe), Not (PropNullishR w)); writes = projection at (4, 0) to (4, 6)}
  };
  (5, 1) to (5, 2) => {
    (2, 12) to (2, 13): (`x`)
  };
  (5, 9) to (5, 10) => {
    (2, 12) to (2, 13): (`x`)
  };
  (4, 0) to (4, 4) => invalidated refinement by {
    function call at (3, 0) to (3, 9)
  };
  (5, 1) to (5, 5) => invalidated refinement by {
    function call at (4, 0) to (4, 16)
  }]"#
    );
}

#[test]
fn test_optional_refi3() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
declare var x: mixed;
if (x?.a === 42) {
  x;
  x.a;
} else {
  x;
  x.a;
}
x;
x.a;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 4) to (3, 5) => {
    (2, 12) to (2, 13): (`x`)
  };
  (4, 2) to (4, 3) => {
    {refinement = And (And (Not (Maybe), Not (PropNullishR a)), SentinelR a); writes = (2, 12) to (2, 13): (`x`)}
  };
  (5, 2) to (5, 3) => {
    {refinement = And (And (Not (Maybe), Not (PropNullishR a)), SentinelR a); writes = (2, 12) to (2, 13): (`x`)}
  };
  (5, 2) to (5, 5) => {
    {refinement = 42; writes = projection at (3, 4) to (3, 8)}
  };
  (7, 2) to (7, 3) => {
    {refinement = Or (Or (Not (Not (Maybe)), Not (Not (PropNullishR a))), Not (SentinelR a)); writes = (2, 12) to (2, 13): (`x`)}
  };
  (8, 2) to (8, 3) => {
    {refinement = Or (Or (Not (Not (Maybe)), Not (Not (PropNullishR a))), Not (SentinelR a)); writes = (2, 12) to (2, 13): (`x`)}
  };
  (10, 0) to (10, 1) => {
    (2, 12) to (2, 13): (`x`)
  };
  (11, 0) to (11, 1) => {
    (2, 12) to (2, 13): (`x`)
  }]"#
    );
}

#[test]
fn test_dead_code_inc() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function f() {
    return;
    x += y;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (4, 4) to (4, 5) => {
    unreachable
  };
  (4, 9) to (4, 10) => {
    unreachable
  }]"#
    );
}

#[test]
fn test_instanceof_mem() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if (x instanceof A.B) {
  x;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => {
    Global x
  };
  (2, 17) to (2, 18) => {
    Global A
  };
  (3, 2) to (3, 3) => {
    {refinement = instanceof; writes = Global x}
  }]"#
    );
}

#[test]
fn test_instanceof_mem_2() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function test() {
  try {
    return;
  } catch {}
  for (let i = 0; ; i++) { }
}

"#,
    );
    assert_eq!(
        result,
        r#"[
  (6, 20) to (6, 21) => {
    (6, 11) to (6, 12): (`i`)
  }]"#
    );
}

#[test]
fn test_gen_next() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function *f() {
  yield;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 2) to (3, 7) => {
    (2, 13) to (2, 13): ()
  }]"#
    );
}

#[test]
fn test_emp_array() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
var x = [];
x;
function f() {
  x;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 0) to (3, 1) => {
    (empty array) (2, 4) to (2, 5): (`x`)
  };
  (5, 2) to (5, 3) => {
    (empty array) (2, 4) to (2, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_emp_array2() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
var x = [];
x = [10];
x;
function f() {
  x;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (4, 0) to (4, 1) => {
    (3, 0) to (3, 1): (`x`)
  };
  (6, 2) to (6, 3) => {
    (3, 0) to (3, 1): (`x`),
    (empty array) (2, 4) to (2, 5): (`x`)
  }]"#
    );
}

#[test]
fn test_declare_module() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
declare module F { }
"#,
    );
    assert_eq!(
        result,
        r#"[
  ]"#
    );
}

#[test]
fn test_declare_namespace() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
const c = F.v;
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
        r#"[
  (2, 10) to (2, 11) => {
    (3, 18) to (3, 19): (`F`)
  };
  (7, 10) to (7, 11) => {
    (3, 18) to (3, 19): (`F`)
  };
  (8, 9) to (8, 10) => {
    (3, 18) to (3, 19): (`F`)
  }]"#
    );
}

#[test]
fn test_declare_type_only_namespace() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
declare namespace F1 {
  declare type T = string;
}
declare namespace F2 {
  type T = string;
}
type T1 = F1.T;
type T2 = F2.T;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (8, 10) to (8, 12) => {
    (2, 18) to (2, 20): (`F1`)
  };
  (9, 10) to (9, 12) => {
    (5, 18) to (5, 20): (`F2`)
  }]"#
    );
}

#[test]
fn test_delete_member() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
delete foo.bar;
foo.bar;
invalidation();
foo.bar;
foo.bar = 3;
foo.bar;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 7) to (2, 10) => {
    Global foo
  };
  (3, 0) to (3, 3) => {
    Global foo
  };
  (3, 0) to (3, 7) => {
    undefined
  };
  (4, 0) to (4, 12) => {
    Global invalidation
  };
  (5, 0) to (5, 3) => {
    Global foo
  };
  (6, 0) to (6, 3) => {
    Global foo
  };
  (7, 0) to (7, 3) => {
    Global foo
  };
  (7, 0) to (7, 7) => {
    (6, 0) to (6, 7): ()
  }]"#
    );
}

#[test]
fn test_exclude_syms() {
    let exclude_syms = FlowOrdSet::from_iter([FlowSmolStr::new("foo"), FlowSmolStr::new("Bar")]);
    // Ensure that defs and writes on excluded names have no effect.
    let result = print_ssa_test(
        None,
        false,
        None,
        Some(exclude_syms),
        r#"
let foo1 = 0
foo1

let foo = 0
foo
foo = 1
foo
foo++
foo
delete foo
foo
type Bar = string;
declare var b1: Bar;
declare var b2: {bar: Bar};
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 0) to (3, 4) => {
    (2, 4) to (2, 8): (`foo1`)
  };
  (6, 0) to (6, 3) => {
    Global foo
  };
  (8, 0) to (8, 3) => {
    Global foo
  };
  (9, 0) to (9, 3) => {
    Global foo
  };
  (10, 0) to (10, 3) => {
    Global foo
  };
  (11, 7) to (11, 10) => {
    Global foo
  };
  (12, 0) to (12, 3) => {
    Global foo
  };
  (14, 16) to (14, 19) => {
    Global Bar
  };
  (15, 22) to (15, 25) => {
    Global Bar
  }]"#
    );
}

#[test]
fn test_match_guard_with_abrupt_body_visits_guard_before_body() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function f(val: unknown) {
  match (val) {
    'productTags' if (Array.isArray(val)) => {
      return;
    }
    _ => {}
  }
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 2) to (3, 7) => {
    (3, 2) to (3, 7): (`<match_root>`)
  };
  (3, 9) to (3, 12) => {
    (2, 11) to (2, 14): (`val`)
  };
  (4, 4) to (4, 4) => {
    {refinement = productTags; writes = (3, 2) to (3, 7): (`<match_root>`)}
  };
  (4, 22) to (4, 27) => {
    Global Array
  };
  (4, 36) to (4, 39) => {
    (2, 11) to (2, 14): (`val`)
  };
  (7, 4) to (7, 4) => {
    {refinement = Not (ImpossibleR); writes = (3, 2) to (3, 7): (`<match_root>`)}
  }]"#,
    );
}

#[test]
fn test_annot_this() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function f(this: number) {
  this;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 2) to (3, 6) => {
    (2, 11) to (2, 23): (this)
  }]"#
    );
}

#[test]
fn test_decl_enum() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
type Props = X
var y = X;
export enum X {
  AGE,
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 13) to (2, 14) => {
    (4, 12) to (4, 13): (`X`)
  };
  (3, 8) to (3, 9) => {
    (undeclared)
  }]"#
    );
}

#[test]
fn test_logic_op_assign_repeat() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
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
        r#"[
  (3, 2) to (3, 3) => {
    (2, 44) to (2, 45): (`o`)
  };
  (4, 2) to (4, 3) => {
    (2, 44) to (2, 45): (`o`)
  };
  (4, 2) to (4, 5) => {
    (3, 2) to (3, 5): ()
  };
  (5, 2) to (5, 3) => {
    (2, 44) to (2, 45): (`o`)
  };
  (5, 2) to (5, 5) => {
    (4, 2) to (4, 5): ()
  }]"#
    );
}

#[test]
fn test_logic_op_assign_repeat_existing_refi() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function member_op_assignment_refinement_ok(o: {p: ?number}) {
  if (o.p === 0) {
    o.p ??= 3;
    o.p &&= (o.p && 3);
    o.p ||= (o.p && 3);
  }
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 6) to (3, 7) => {
    (2, 44) to (2, 45): (`o`)
  };
  (4, 4) to (4, 5) => {
    {refinement = SentinelR p; writes = (2, 44) to (2, 45): (`o`)}
  };
  (4, 4) to (4, 7) => {
    {refinement = 0; writes = projection at (3, 6) to (3, 9)}
  };
  (5, 4) to (5, 5) => {
    {refinement = SentinelR p; writes = (2, 44) to (2, 45): (`o`)}
  };
  (5, 4) to (5, 7) => {
    (4, 4) to (4, 7): ()
  };
  (5, 13) to (5, 14) => {
    {refinement = PropTruthyR (p); writes = {refinement = SentinelR p; writes = (2, 44) to (2, 45): (`o`)}}
  };
  (5, 13) to (5, 16) => {
    {refinement = Truthy; writes = (4, 4) to (4, 7): ()}
  };
  (6, 4) to (6, 5) => {
    {refinement = SentinelR p; writes = (2, 44) to (2, 45): (`o`)}
  };
  (6, 4) to (6, 7) => {
    (5, 4) to (5, 7): ()
  };
  (6, 13) to (6, 14) => {
    {refinement = Not (PropTruthyR (p)); writes = {refinement = SentinelR p; writes = (2, 44) to (2, 45): (`o`)}}
  };
  (6, 13) to (6, 16) => {
    {refinement = Not (Truthy); writes = (5, 4) to (5, 7): ()}
  }]"#
    );
}

#[test]
fn test_logic_op_assign_repeat_2() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
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
        r#"[
  (3, 2) to (3, 3) => {
    (2, 44) to (2, 45): (`o`)
  };
  (4, 2) to (4, 3) => {
    (2, 44) to (2, 45): (`o`)
  };
  (4, 2) to (4, 5) => {
    (3, 2) to (3, 5): ()
  };
  (5, 2) to (5, 3) => {
    (2, 44) to (2, 45): (`o`)
  };
  (5, 2) to (5, 5) => {
    (4, 2) to (4, 5): ()
  }]"#
    );
}

#[test]
fn test_logic_op_assign_repeat_existing_refi_2() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
function member_op_assignment_refinement_ok(o: {p: ?number}) {
  if (o.p === 0) {
    o.p ??= 3;
    o.p &&= (o.p && 3);
    o.p ||= (o.p && 3);
  }
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 6) to (3, 7) => {
    (2, 44) to (2, 45): (`o`)
  };
  (4, 4) to (4, 5) => {
    {refinement = SentinelR p; writes = (2, 44) to (2, 45): (`o`)}
  };
  (4, 4) to (4, 7) => {
    {refinement = 0; writes = projection at (3, 6) to (3, 9)}
  };
  (5, 4) to (5, 5) => {
    {refinement = SentinelR p; writes = (2, 44) to (2, 45): (`o`)}
  };
  (5, 4) to (5, 7) => {
    (4, 4) to (4, 7): ()
  };
  (5, 13) to (5, 14) => {
    {refinement = PropTruthyR (p); writes = {refinement = SentinelR p; writes = (2, 44) to (2, 45): (`o`)}}
  };
  (5, 13) to (5, 16) => {
    {refinement = Truthy; writes = (4, 4) to (4, 7): ()}
  };
  (6, 4) to (6, 5) => {
    {refinement = SentinelR p; writes = (2, 44) to (2, 45): (`o`)}
  };
  (6, 4) to (6, 7) => {
    (5, 4) to (5, 7): ()
  };
  (6, 13) to (6, 14) => {
    {refinement = Not (PropTruthyR (p)); writes = {refinement = SentinelR p; writes = (2, 44) to (2, 45): (`o`)}}
  };
  (6, 13) to (6, 16) => {
    {refinement = Not (Truthy); writes = (5, 4) to (5, 7): ()}
  }]"#
    );
}

#[test]
fn test_vals() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
declare var values: mixed;

if (values.bxxxx === values.axxxx) {
  values;
  values.axxxx;
  values.bxxxx;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (4, 4) to (4, 10) => {
    (2, 12) to (2, 18): (`values`)
  };
  (4, 21) to (4, 27) => {
    (2, 12) to (2, 18): (`values`)
  };
  (5, 2) to (5, 8) => {
    {refinement = SentinelR bxxxx; writes = (2, 12) to (2, 18): (`values`)}
  };
  (6, 2) to (6, 8) => {
    {refinement = SentinelR bxxxx; writes = (2, 12) to (2, 18): (`values`)}
  };
  (7, 2) to (7, 8) => {
    {refinement = SentinelR bxxxx; writes = (2, 12) to (2, 18): (`values`)}
  }]"#
    );
}

#[test]
fn test_destruct_default() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
let x = 42;
let y;

({x = x, y = x} = {});
([x = x, y = x] = []);
"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 6) to (5, 7) => {
    (2, 4) to (2, 5): (`x`)
  };
  (5, 13) to (5, 14) => {
    (5, 2) to (5, 3): (`x`)
  };
  (6, 6) to (6, 7) => {
    (5, 2) to (5, 3): (`x`)
  };
  (6, 13) to (6, 14) => {
    (6, 2) to (6, 3): (`x`)
  }]"#
    );
}

#[test]
fn test_component_declaration() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
  Foo;
  component Foo(param: T) { param }
  Foo;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 2) to (2, 5) => {
    (3, 12) to (3, 15): (`Foo`)
  };
  (3, 23) to (3, 24) => {
    Global T
  };
  (3, 28) to (3, 33) => {
    (3, 16) to (3, 21): (`param`)
  };
  (4, 2) to (4, 5) => {
    (3, 12) to (3, 15): (`Foo`)
  }]"#
    );
}

#[test]
fn test_component_declaration_conflicting_defs() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
  component Foo(param: T) {
    const param = 1;
    param
  }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 23) to (2, 24) => {
    Global T
  };
  (4, 4) to (4, 9) => {
    (2, 16) to (2, 21): (`param`)
  }]"#
    );
}

#[test]
fn test_dead_component_declaration() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
  throw 'lol';
  component Foo(param: T) { param } // hoisted
  Foo;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 23) to (3, 24) => {
    Global T
  };
  (3, 28) to (3, 33) => {
    (3, 16) to (3, 21): (`param`)
  };
  (4, 2) to (4, 5) => {
    unreachable
  }]"#
    );
}

#[test]
fn test_component_refinement_scope() {
    let result = print_ssa_assertion_test(
        r#"
  let x = null;
  invariant(x == null);
  x; // refined
  component Foo() {
    x // not refined
  }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 2) to (3, 11) => {
    Global invariant
  };
  (3, 12) to (3, 13) => {
    (2, 6) to (2, 7): (`x`)
  };
  (4, 2) to (4, 3) => {
    {refinement = Not (Not (Maybe)); writes = (2, 6) to (2, 7): (`x`)}
  };
  (6, 4) to (6, 5) => {
    (2, 6) to (2, 7): (`x`)
  }]"#
    );
}

#[test]
fn test_component_param_scoping() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
  component Foo(
    x: number,
    y: typeof x, // x not in scope, so global
    z = y, // y not in scope, so global
  ) {
  }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (4, 14) to (4, 15) => {
    Global x
  };
  (5, 8) to (5, 9) => {
    Global y
  }]"#
    );
}

#[test]
fn test_type_guard_scoping() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
  const x = 1;
  function f(x: mixed): x is number {
    return typeof x === "number";
  }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 24) to (3, 25) => {
    (3, 13) to (3, 14): (`x`)
  };
  (4, 18) to (4, 19) => {
    (3, 13) to (3, 14): (`x`)
  }]"#
    );
}

#[test]
fn test_declare_component() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
  const x = 42;
  declare component Foo(x: number, y: x);
  x;
  Foo;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 38) to (3, 39) => {
    (2, 8) to (2, 9): (`x`)
  };
  (4, 2) to (4, 3) => {
    (2, 8) to (2, 9): (`x`)
  };
  (5, 2) to (5, 5) => {
    (3, 20) to (3, 23): (`Foo`)
  }]"#
    );
}

#[test]
fn test_declare_component2() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
declare component Y();

component X() {
    return <Y />;
}

<Y />;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (5, 11) to (5, 16) => {
    Global React
  };
  (5, 12) to (5, 13) => {
    (2, 18) to (2, 19): (`Y`)
  };
  (8, 0) to (8, 5) => {
    Global React
  };
  (8, 1) to (8, 2) => {
    (2, 18) to (2, 19): (`Y`)
  }]"#
    );
}

#[test]
fn test_lowercase_jsx() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
  const div = 42;
  <div />;
  <floop />;
"#,
    );
    assert_eq!(
        result,
        r#"[
  (3, 2) to (3, 9) => {
    Global React
  };
  (3, 3) to (3, 6) => {
    (2, 8) to (2, 11): (`div`)
  };
  (4, 2) to (4, 11) => {
    Global React
  };
  (4, 3) to (4, 8) => {
    Global floop
  }]"#
    );
}

#[test]
fn test_component_forwardref() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
const React = require('react');
component Foo(ref: any) { } // should read
declare component Foo(ref: any); // should not read
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 14) to (2, 21) => {
    Global require
  };
  (3, 14) to (3, 17) => {
    (2, 6) to (2, 11): (`React`)
  }]"#
    );
}

#[test]
fn test_component_type_alias() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
component Foo() {
    type Bar = string;
    const b: Bar = 'hi';
    return <div />;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (4, 13) to (4, 16) => {
    (3, 9) to (3, 12): (`Bar`)
  };
  (5, 11) to (5, 18) => {
    Global React
  };
  (5, 12) to (5, 15) => {
    Global div
  }]"#
    );
}

#[test]
fn test_nullish_props() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
if (x.y == null) {
  x;
  x.y;
}

if (x.y == undefined) {
  x;
  x.y;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 4) to (2, 5) => {
    Global x
  };
  (3, 2) to (3, 3) => {
    {refinement = Not (Not (PropNullishR y)); writes = Global x}
  };
  (4, 2) to (4, 3) => {
    {refinement = Not (Not (PropNullishR y)); writes = Global x}
  };
  (4, 2) to (4, 5) => {
    {refinement = Not (Not (Maybe)); writes = projection at (2, 4) to (2, 7)}
  };
  (7, 4) to (7, 5) => {
    Global x
  };
  (7, 11) to (7, 20) => {
    Global undefined
  };
  (8, 2) to (8, 3) => {
    {refinement = Not (Not (PropNullishR y)); writes = Global x}
  };
  (9, 2) to (9, 3) => {
    {refinement = Not (Not (PropNullishR y)); writes = Global x}
  };
  (9, 2) to (9, 5) => {
    {refinement = Not (Not (Maybe)); writes = projection at (7, 4) to (7, 7)}
  }]"#
    );
}

#[test]
fn test_component_type_alias_2() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
import * as React from 'react';
import {useRef} from 'react';

component Component() {
  const ref1 = useRef<?number>(null);
  if (ref1.current === null) {
    ref1.current = 42; // ok
  }
  ref1.current; // error

  const ref2 = useRef<?number>(null);
  if (ref2.current === null) {
    ref2.current; // error
  }

  const ref3 = useRef<?number>(null);
  if (ref3.current === null && ref1) {
    ref3.current = 42; // ok
  }

  const ref4 = useRef<?number>(null);
  if (ref4.current === null || ref1) {
    ref4.current = 42; // error
  }

  const ref5 = useRef<?number>(null);
  if (ref5.current == undefined) {
    ref5.current = 42; // ok
  }

  const ref6 = useRef<?number>(null);
  if (!ref6.current) {
    ref6.current = 42; // ok
  }
  return null;
}
"#,
    );
    assert_eq!(
        result,
        r#"[
  (6, 15) to (6, 21) => {
    (3, 8) to (3, 14): (`useRef`)
  };
  (7, 6) to (7, 10) => {
    (6, 8) to (6, 12): (`ref1`)
  };
  (8, 4) to (8, 8) => {
    {refinement = SentinelR current; writes = (6, 8) to (6, 12): (`ref1`)}
  };
  (10, 2) to (10, 6) => {
    (6, 8) to (6, 12): (`ref1`)
  };
  (10, 2) to (10, 14) => {
    (8, 4) to (8, 16): (),
    {refinement = Not (Null); writes = projection at (7, 6) to (7, 18)}
  };
  (12, 15) to (12, 21) => {
    (3, 8) to (3, 14): (`useRef`)
  };
  (13, 6) to (13, 10) => {
    (12, 8) to (12, 12): (`ref2`)
  };
  (14, 4) to (14, 8) => {
    {refinement = SentinelR current; writes = (12, 8) to (12, 12): (`ref2`)}
  };
  (14, 4) to (14, 16) => {
    {refinement = Null; writes = projection at (13, 6) to (13, 18)}
  };
  (17, 15) to (17, 21) => {
    (3, 8) to (3, 14): (`useRef`)
  };
  (18, 6) to (18, 10) => {
    (17, 8) to (17, 12): (`ref3`)
  };
  (18, 31) to (18, 35) => {
    (6, 8) to (6, 12): (`ref1`)
  };
  (19, 4) to (19, 8) => {
    {refinement = SentinelR current; writes = (17, 8) to (17, 12): (`ref3`)}
  };
  (22, 15) to (22, 21) => {
    (3, 8) to (3, 14): (`useRef`)
  };
  (23, 6) to (23, 10) => {
    (22, 8) to (22, 12): (`ref4`)
  };
  (23, 31) to (23, 35) => {
    (6, 8) to (6, 12): (`ref1`)
  };
  (24, 4) to (24, 8) => {
    (22, 8) to (22, 12): (`ref4`)
  };
  (27, 15) to (27, 21) => {
    (3, 8) to (3, 14): (`useRef`)
  };
  (28, 6) to (28, 10) => {
    (27, 8) to (27, 12): (`ref5`)
  };
  (28, 22) to (28, 31) => {
    Global undefined
  };
  (29, 4) to (29, 8) => {
    {refinement = Not (Not (PropNullishR current)); writes = (27, 8) to (27, 12): (`ref5`)}
  };
  (32, 15) to (32, 21) => {
    (3, 8) to (3, 14): (`useRef`)
  };
  (33, 7) to (33, 11) => {
    (32, 8) to (32, 12): (`ref6`)
  };
  (34, 4) to (34, 8) => {
    {refinement = Not (PropTruthyR (current)); writes = (32, 8) to (32, 12): (`ref6`)}
  }]"#
    );
}

#[test]
fn test_strict_eq_ident() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
  if (x === y) {
    x;
  } else {
    x;
  }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 6) to (2, 7) => {
    Global x
  };
  (2, 12) to (2, 13) => {
    Global y
  };
  (3, 4) to (3, 5) => {
    {refinement = EqR; writes = Global x}
  };
  (5, 4) to (5, 5) => {
    {refinement = Not (EqR); writes = Global x}
  }]"#
    );
}

#[test]
fn test_strict_eq_member() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
  if (x === y.foo) {
    x;
  } else {
    x;
  }
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 6) to (2, 7) => {
    Global x
  };
  (2, 12) to (2, 13) => {
    Global y
  };
  (3, 4) to (3, 5) => {
    {refinement = EqR; writes = Global x}
  };
  (5, 4) to (5, 5) => {
    {refinement = Not (EqR); writes = Global x}
  }]"#
    );
}

#[test]
fn test_match_object_pattern() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
(match (x) {
  {type: 'foo', value: const a} => a as number,
  {type: 'bar'} => 1,
});
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 6) => {
    (2, 1) to (2, 6): (`<match_root>`)
  };
  (2, 8) to (2, 9) => {
    Global x
  };
  (3, 2) to (3, 2) => {
    {refinement = And (And (And (object, Not (Null)), SentinelR type), PropExistsR (value)); writes = (2, 1) to (2, 6): (`<match_root>`)}
  };
  (3, 35) to (3, 36) => {
    (3, 29) to (3, 30): (`a`)
  };
  (4, 2) to (4, 2) => {
    {refinement = And (And (object, Not (Null)), SentinelR type); writes = (2, 1) to (2, 6): (`<match_root>`)}
  }]"#
    );
}

#[test]
fn test_match_array_pattern() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
(match (x) {
  [      'foo',        const a] => a as number,
  [      'bar'] => 1,
});
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 6) => {
    (2, 1) to (2, 6): (`<match_root>`)
  };
  (2, 8) to (2, 9) => {
    Global x
  };
  (3, 2) to (3, 2) => {
    {refinement = And (And (isArray, array length === 2), SentinelR 0); writes = (2, 1) to (2, 6): (`<match_root>`)}
  };
  (3, 35) to (3, 36) => {
    (3, 29) to (3, 30): (`a`)
  };
  (4, 2) to (4, 2) => {
    {refinement = And (And (isArray, array length === 1), SentinelR 0); writes = (2, 1) to (2, 6): (`<match_root>`)}
  }]"#
    );
}

#[test]
fn test_match_instance_pattern() {
    let result = print_ssa_test(
        None,
        false,
        None,
        None,
        r#"
(match (x) {
  Foo {a: true, const b} => b,
  Foo {b: false, const b} => b,
});
"#,
    );
    assert_eq!(
        result,
        r#"[
  (2, 1) to (2, 6) => {
    (2, 1) to (2, 6): (`<match_root>`)
  };
  (2, 8) to (2, 9) => {
    Global x
  };
  (3, 2) to (3, 2) => {
    {refinement = And (instanceof, SentinelR a); writes = (2, 1) to (2, 6): (`<match_root>`)}
  };
  (3, 2) to (3, 5) => {
    Global Foo
  };
  (3, 28) to (3, 29) => {
    (3, 22) to (3, 23): (`b`)
  };
  (4, 2) to (4, 2) => {
    {refinement = And (instanceof, SentinelR b); writes = (2, 1) to (2, 6): (`<match_root>`)}
  };
  (4, 2) to (4, 5) => {
    Global Foo
  };
  (4, 29) to (4, 30) => {
    (4, 23) to (4, 24): (`b`)
  }]"#
    );
}
