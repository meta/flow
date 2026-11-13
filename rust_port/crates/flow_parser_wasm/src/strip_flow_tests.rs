/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use flow_parser::ast;
use flow_parser::ast::function;
use flow_parser::ast::statement::StatementInner;
use flow_parser::loc::Loc;
use flow_parser_utils_output::js_layout_generator;
use flow_parser_utils_output::pretty_printer;

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

#[test]
fn runnable_erasure_preserves_defaults_patterns_and_value_locations() {
    let source = r#""use strict";
type T = number;
declare function declared(): void;
function choose<T>(this: Object, x: T = make(), y?: string): T {
  const z: T = (x: T);
  return z as T;
}
const arrow = <T>(value: T = make()): T => value;
const {value}: Props = getProps();
const [first]: Array<number> = array;
const instance = new Box<number>(first);
const result = choose<number>(first) satisfies number;
"#;
    let lowered = super::lower_program(&parse(source));
    assert_eq!(
        print(&lowered),
        r#""use strict";

function choose(x = make(), y) {
  const z = x;
  return z;
}
const arrow = (value = make()) => value;
const {value} = getProps();
const [first] = array;
const instance = new Box(first);
const result = choose(first);
"#
    );
    let StatementInner::FunctionDeclaration { inner, .. } = &*lowered.statements[1] else {
        panic!("expected remaining function");
    };
    let function::Body::BodyBlock((_, body)) = &inner.body else {
        panic!("expected function block");
    };
    let StatementInner::Return { inner, .. } = &*body.body[1] else {
        panic!("expected return");
    };
    let value = inner.argument.as_ref().expect("expected return value");
    assert_eq!((value.loc().start.line, value.loc().start.column), (6, 9));
}

#[test]
fn runnable_erasure_preserves_value_imports_and_side_effects() {
    let source = r#"import "effects";
import {} from "empty";
import type T from "types";
import typeof U from "types2";
import {type A, typeof B} from "type-only";
import D, {type C, value as local} from "mixed";
import Default, {type E} from "default";
import * as ns from "namespace";
export type Alias = number;
export type * from "type-exports";
export {type A, local};
export {};
export * from "values";
export default local;
"#;
    assert_eq!(
        print(&super::lower_program(&parse(source))),
        r#"import "effects";
import {} from "empty";

import D, { value as local } from "mixed";
import Default from "default";
import * as ns from "namespace";

export { local };
export {};
export * from "values";
export default local;
"#
    );
}

#[test]
fn runnable_erasure_preserves_class_runtime_members_and_optional_chains() {
    let source = r#"abstract class Box<T> extends Base<T> implements I {
  declare ghost: number;
  abstract method(): void;
  +empty: number;
  #secret: number = 1;
  static value: number = init();
  method<T>(x: T = make()): T { return x; }
}
const C = class<T> { field?: number; };
const a = (value as const)!;
const b = obj!.field;
const c = obj?.field!;
const d = fn!<number>(1);
const e = obj?.method!<number>(1);
"#;
    assert_eq!(
        print(&super::lower_program(&parse(source))),
        r#"class Box
  extends Base {
  empty;
  #secret = 1;
  static value = init();
  method(x = make()) {
    return x;
  }
}
const C = class {
  field;
};
const a = value;
const b = obj.field;
const c = obj?.field;
const d = fn(1);
const e = obj?.method(1);
"#
    );
}

#[test]
fn runnable_erasure_removes_definite_assignment_assertions() {
    let source = r#"let a!: number;
class Box {
  value!: number;
  #secret!: string;
}
"#;
    assert_eq!(
        print(&super::lower_program(&parse(source))),
        r#"let a;
class Box {
  value;
  #secret;
}
"#
    );
}

#[test]
fn runnable_erasure_removes_ambient_declarations_and_keeps_module_values() {
    let source = r#"declare enum E {}
declare namespace N {}
declare module "ambient" {
  import A = require("ambient");
  export = A;
}
import type T = require("types");
import A = require("values");
import B = A.member;
export import C = require("exported");
export {type T} from "types";
export = A as Object;
"#;
    assert_eq!(
        print(&super::lower_program(&parse(source))),
        r#"const A = require("values");
var B = A.member;
export const C = require("exported");

module.exports = A;
"#
    );
}

#[test]
fn runnable_erasure_removes_jsx_type_arguments_and_erases_nested_values() {
    let source = "const element = <Foo<number> value={value as number} />;";
    assert_eq!(
        print(&super::lower_program(&parse(source))),
        "const element = <Foo value={value} />;\n"
    );
}
