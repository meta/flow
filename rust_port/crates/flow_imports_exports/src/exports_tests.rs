/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use flow_parser::ParseOptions;
use flow_parser::parse_program_without_file;
use flow_type_sig::type_sig_options::TypeSigOptions;
use flow_type_sig::type_sig_utils;
use pretty_assertions::assert_eq;

use crate::exports::Export;
use crate::exports::of_builtins;
use crate::exports::of_module;

fn dedent_trim(s: &str) -> String {
    let lines: Vec<&str> = s.lines().filter(|line| !line.trim().is_empty()).collect();

    if lines.is_empty() {
        return String::new();
    }

    let min_indent = lines
        .iter()
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|line| {
            if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_options() -> ParseOptions {
    ParseOptions {
        enums: true,
        components: true,
        ..Default::default()
    }
}

fn sig_opts() -> TypeSigOptions {
    TypeSigOptions {
        munge: false,
        facebook_key_mirror: false,
        facebook_fbt: None,
        enable_custom_error: false,
        enable_enums: true,
        enable_component_syntax: true,
        component_syntax_enabled_in_config: true,
        enable_ts_syntax: true,
        enable_ts_utility_syntax: true,
        hook_compatibility: true,
        enable_records: true,
        enable_relay_integration: false,
        relay_integration_module_prefix: None,
        for_builtins: false,
        locs_to_dirtify: vec![],
        is_ts_file: false,
        is_dts_file: false,
        tslib_syntax: false,
        is_lib_file: false,
    }
}

fn print_module(contents_indent: &str) -> Vec<Export> {
    let contents = dedent_trim(contents_indent);
    let (ast, _errors) =
        parse_program_without_file(false, None, Some(parse_options()), Ok(&contents));
    let arena = bumpalo::Bump::new();
    let (_errors, _locs, packed_sig) =
        type_sig_utils::parse_and_pack_module(&sig_opts(), &arena, true, None, None, &ast);
    let exports = of_module(&packed_sig);
    exports.iter().cloned().collect()
}

fn print_builtins(contents_indent: &str) -> Vec<Export> {
    let contents = dedent_trim(contents_indent);
    let (ast, _errors) =
        parse_program_without_file(false, None, Some(parse_options()), Ok(&contents));
    let arena = bumpalo::Bump::new();
    let mut opts = sig_opts();
    opts.for_builtins = true;
    let asts = [(&ast, flow_common::type_strictness::TypeStrictnessKind::Flow)];
    let (_errors, _locs, packed_sig) =
        type_sig_utils::parse_and_pack_builtins(&opts, &arena, &asts, &[]);
    let exports = of_builtins(&packed_sig);
    exports.iter().cloned().collect()
}

#[test]
fn es6_named_const() {
    let exports = print_module(
        r#"
        export const x : string = "foo"
    "#,
    );
    assert_eq!(exports, vec![Export::Named("x".into())]);
}

#[test]
fn cjs_named_const() {
    let exports = print_module(
        r#"
        const x : string = "foo";
        exports.x = x;
    "#,
    );
    assert_eq!(
        exports,
        vec![Export::Default(None), Export::Named("x".into())]
    );
}

#[test]
fn es6_default_string_literal() {
    let exports = print_module(
        r#"
        export default "foo";
    "#,
    );
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn cjs_default_string_literal() {
    let exports = print_module(
        r#"
        module.exports = "foo";
    "#,
    );
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn es6_default_number_literal() {
    let exports = print_module(
        r#"
        export default 0;
    "#,
    );
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn cjs_default_number_literal() {
    let exports = print_module(
        r#"
        module.exports = 0;
    "#,
    );
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn es6_named_type_ref() {
    let exports = print_module(
        r#"
        type T = string;
        export type U = T;
        export default 0; // need an exported value to force ES6 modules
    "#,
    );
    assert_eq!(
        exports,
        vec![Export::Default(None), Export::NamedType("U".into())]
    );
}

#[test]
fn cjs_named_type_ref() {
    let exports = print_module(
        r#"
        type T = string;
        export type U = T;
    "#,
    );
    assert_eq!(exports, vec![Export::NamedType("U".into())]);
}

#[test]
fn es6_named_type_binding() {
    let exports = print_module(
        r#"
        export type T = string;
        export default 0; // need an exported value to force ES6 modules
    "#,
    );
    assert_eq!(
        exports,
        vec![Export::Default(None), Export::NamedType("T".into())]
    );
}

#[test]
fn cjs_named_type_binding() {
    let exports = print_module(
        r#"
        export type T = string;
    "#,
    );
    assert_eq!(exports, vec![Export::NamedType("T".into())]);
}

#[test]
fn es6_named_opaque_type_binding() {
    let exports = print_module(
        r#"
        export opaque type T = string;
        export default 0; // need an exported value to force ES6 modules
    "#,
    );
    assert_eq!(
        exports,
        vec![Export::Default(None), Export::NamedType("T".into())]
    );
}

#[test]
fn cjs_named_opaque_type_binding() {
    let exports = print_module(
        r#"
        export opaque type T = string;
    "#,
    );
    assert_eq!(exports, vec![Export::NamedType("T".into())]);
}

#[test]
fn es6_default_class() {
    let exports = print_module(
        r#"
        export default class Foo {
          static foo(): void {}
          bar(): void {}
        };
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::DefaultType(Some("Foo".into())),
            Export::Default(Some("Foo".into()))
        ]
    );
}

#[test]
fn cjs_default_class_ref() {
    let exports = print_module(
        r#"
        class Foo {
          static foo(): void {}
          bar(): void {}
        };
        module.exports = Foo;
    "#,
    );
    // TODO: also DefaultType
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn cjs_default_class_expr() {
    let exports = print_module(
        r#"
        module.exports = class Foo {
          static foo(): void {}
          bar(): void {}
        };
    "#,
    );
    // TODO: also DefaultType
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn es6_enum() {
    let exports = print_module(
        r#"
        export enum E {
          FOO,
          BAR,
        };
    "#,
    );
    assert_eq!(
        exports,
        vec![Export::NamedType("E".into()), Export::Named("E".into())]
    );
}

#[test]
fn cjs_enum() {
    let exports = print_module(
        r#"
        enum E {
          FOO,
          BAR,
        };
        exports.E = E;
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::Default(None),
            Export::NamedType("E".into()),
            Export::Named("E".into())
        ]
    );
}

#[test]
fn es6_type_enum() {
    let exports = print_module(
        r#"
        enum EImpl {}
        export type E = EImpl;
        export default 0; // need an exported value to force ES6 modules
    "#,
    );
    assert_eq!(
        exports,
        vec![Export::Default(None), Export::NamedType("E".into())]
    );
}

#[test]
fn cjs_type_enum() {
    let exports = print_module(
        r#"
        enum EImpl {}
        export type E = EImpl;
    "#,
    );
    assert_eq!(exports, vec![Export::NamedType("E".into())]);
}

#[test]
fn es6_default_enum() {
    let exports = print_module(
        r#"
        export default enum E {
          FOO,
          BAR,
        };
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::DefaultType(Some("E".into())),
            Export::Default(Some("E".into()))
        ]
    );
}

#[test]
fn cjs_default_enum() {
    let exports = print_module(
        r#"
        enum E {
          FOO,
          BAR,
        };
        module.exports = E;
    "#,
    );
    // TODO: also DefaultType
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn cjs_default_named_enum() {
    let exports = print_module(
        r#"
        enum E {
          FOO,
          BAR,
        };
        module.exports = { E };
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::Default(None),
            Export::NamedType("E".into()),
            Export::Named("E".into())
        ]
    );
}

#[test]
fn es6_named_class_ref() {
    let exports = print_module(
        r#"
        class Foo {}
        export { Foo };
    "#,
    );
    assert_eq!(
        exports,
        vec![Export::NamedType("Foo".into()), Export::Named("Foo".into())]
    );
}

#[test]
fn cjs_named_class_ref() {
    let exports = print_module(
        r#"
        class Foo {}
        exports.Foo = Foo;
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::Default(None),
            Export::NamedType("Foo".into()),
            Export::Named("Foo".into())
        ]
    );
}

#[test]
fn es6_named_class_ref_ref() {
    let exports = print_module(
        r#"
        class Foo {}
        const Bar = Foo;
        export { Bar };
    "#,
    );
    assert_eq!(
        exports,
        vec![Export::NamedType("Bar".into()), Export::Named("Bar".into())]
    );
}

#[test]
fn cjs_named_class_ref_inline() {
    let exports = print_module(
        r#"
        exports.Foo = class Foo {};
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::Default(None),
            Export::NamedType("Foo".into()),
            Export::Named("Foo".into())
        ]
    );
}

#[test]
fn es6_named_class_binding() {
    let exports = print_module(
        r#"
        export class Foo {};
    "#,
    );
    assert_eq!(
        exports,
        vec![Export::NamedType("Foo".into()), Export::Named("Foo".into())]
    );
}

#[test]
fn es6_default_obj() {
    let exports = print_module(
        r#"
        export default {
          foo: 123,
          bar: "bar",
          baz(): void {},
        };
    "#,
    );
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn es6_default_variable() {
    let exports = print_module(
        r#"
        const foo = 3;
        export default foo;
    "#,
    );
    assert_eq!(exports, vec![Export::Default(Some("foo".into()))]);
}

#[test]
fn es6_declare_export_default_typeof_variable() {
    let exports = print_module(
        r#"
        const foo = 3;
        declare export default typeof foo;
    "#,
    );
    assert_eq!(exports, vec![Export::Default(Some("foo".into()))]);
}

#[test]
fn cjs_default_obj() {
    let exports = print_module(
        r#"
        module.exports = {
          foo: 123,
          bar: "bar",
          baz(): void {},
        };
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::Default(None),
            Export::Named("bar".into()),
            Export::Named("baz".into()),
            Export::Named("foo".into())
        ]
    );
}

#[test]
fn cjs_default_obj_ref() {
    let exports = print_module(
        r#"
        const O = {
          foo: 123,
          bar: "bar",
        };
        module.exports = O;
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::Default(None),
            Export::Named("bar".into()),
            Export::Named("foo".into())
        ]
    );
}

#[test]
fn cjs_default_obj_ref_annot() {
    let exports = print_module(
        r#"
        var O: {bar: string, foo: number,...} = {
          foo: 123,
          bar: "bar",
        };
        module.exports = O;
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::Default(None),
            Export::Named("bar".into()),
            Export::Named("foo".into())
        ]
    );
}

#[test]
fn cjs_default_obj_ref_annot_ref() {
    let exports = print_module(
        r#"
        type T = { bar: string, foo: number };
        var O: T = { foo: 123, bar: "bar" };
        module.exports = O;
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::Default(None),
            Export::Named("bar".into()),
            Export::Named("foo".into())
        ]
    );
}

#[test]
fn es6_default_obj_type() {
    let exports = print_module(
        r#"
        export default ({
          foo: 123,
          bar: "bar",
        }: { foo: number, bar: string });
    "#,
    );
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn cjs_default_obj_type() {
    let exports = print_module(
        r#"
        module.exports = ({
          foo: 123,
          bar: "bar",
        }: { foo: number, bar: string });
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::Default(None),
            Export::Named("bar".into()),
            Export::Named("foo".into())
        ]
    );
}

#[test]
fn cjs_default_obj_type_app() {
    let exports = print_module(
        r#"
        type O<T> = { foo: T }
        module.exports = ({ foo: 123 }: O<number>)
    "#,
    );
    assert_eq!(
        exports,
        vec![Export::Default(None), Export::Named("foo".into())]
    );
}

#[test]
fn cjs_default_obj_type_ref() {
    let exports = print_module(
        r#"
        type O = { foo: number, bar: string }
        module.exports = ({ foo: 123, bar: "bar"} : O);
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::Default(None),
            Export::Named("bar".into()),
            Export::Named("foo".into())
        ]
    );
}

#[test]
fn es6_default_obj_type_opaque() {
    let exports = print_module(
        r#"
        opaque type T = { foo: number, bar: string };
        export default ({
          foo: 123,
          bar: "bar",
        }: T);
    "#,
    );
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn cjs_default_obj_type_opaque() {
    let exports = print_module(
        r#"
        opaque type T = { foo: number, bar: string };
        module.exports = ({
          foo: 123,
          bar: "bar",
        }: T);
    "#,
    );
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn es6_default_obj_with_class_expr() {
    // can't destructure the default export, so don't include its props
    let exports = print_module(
        r#"
        export default {
          Foo: class {}
        };
    "#,
    );
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn cjs_default_obj_with_class_expr() {
    let exports = print_module(
        r#"
        module.exports = {
          Foo: class {}
        };
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::Default(None),
            Export::NamedType("Foo".into()),
            Export::Named("Foo".into())
        ]
    );
}

#[test]
fn cjs_default_obj_with_class_ref() {
    let exports = print_module(
        r#"
        class Foo {}
        module.exports = { Foo };
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::Default(None),
            Export::NamedType("Foo".into()),
            Export::Named("Foo".into())
        ]
    );
}

#[test]
fn cjs_default_obj_with_class_ref_inline() {
    let exports = print_module(
        r#"
        module.exports = {
          Foo: class Foo {}
        };
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::Default(None),
            Export::NamedType("Foo".into()),
            Export::Named("Foo".into())
        ]
    );
}

#[test]
fn cjs_default_obj_with_string_literal() {
    // string literals currently can't be imported via destructuring
    let exports = print_module(
        r#"
        module.exports = { 'Foo bar': 123 }
    "#,
    );
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn cjs_default_pattern_ref() {
    let exports = print_module(
        r#"
        const {Foo} = {
          Foo: { foo: 123 }
        };
        module.exports = Foo;
    "#,
    );
    assert_eq!(
        exports,
        vec![Export::Default(None), Export::Named("foo".into())]
    );
}

#[test]
fn cjs_default_pattern_ref_of_ref() {
    let exports = print_module(
        r#"
        const T = {
          Foo: { foo: 123 }
        };
        const {Foo} = T;
        module.exports = Foo;
    "#,
    );
    assert_eq!(
        exports,
        vec![Export::Default(None), Export::Named("foo".into())]
    );
}

#[test]
fn cjs_default_pattern_tyref() {
    let exports = print_module(
        r#"
        const {Foo} = {
          Foo: class {}
        };
        module.exports = (new Foo(): Foo);
    "#,
    );
    // TODO: also DefaultType
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn cjs_default_eval_getprop() {
    let exports = print_module(
        r#"
        const O = { prop: { foo: 123 } };
        module.exports = O.prop;
    "#,
    );
    assert_eq!(
        exports,
        vec![Export::Default(None), Export::Named("foo".into())]
    );
}

#[test]
fn cjs_default_eval_getprop_method() {
    let exports = print_module(
        r#"
        const O = { prop(): void {} };
        module.exports = O.prop;
    "#,
    );
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn cjs_default_eval_getprop_of_variable() {
    let exports = print_module(
        r#"
        const O = { prop: { foo: 123 } };
        const T = O.prop;
        module.exports = T;
    "#,
    );
    assert_eq!(
        exports,
        vec![Export::Default(None), Export::Named("foo".into())]
    );
}

#[test]
fn cjs_default_nested_typealias() {
    let exports = print_module(
        r#"
        type T<U> = { foo: U, bar: string };
        type O = T<number>;
        const X : O = { foo: 123, bar: "bar" };
        module.exports = X;
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::Default(None),
            Export::Named("bar".into()),
            Export::Named("foo".into())
        ]
    );
}

#[test]
fn cjs_default_nested_ref() {
    let exports = print_module(
        r#"
        const O = { foo: 123, bar: "bar" };
        const X = O;
        const Y = X;
        module.exports = Y;
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::Default(None),
            Export::Named("bar".into()),
            Export::Named("foo".into())
        ]
    );
}

#[test]
fn es6_default_function() {
    let exports = print_module(
        r#"
        export default function() {}
    "#,
    );
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn cjs_default_function() {
    let exports = print_module(
        r#"
        module.exports = function foo(): void {}
    "#,
    );
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn cjs_default_function_ref() {
    let exports = print_module(
        r#"
        function foo(): void {}
        foo.bar = 123;
        module.exports = foo;
    "#,
    );
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn es6_named_function() {
    let exports = print_module(
        r#"
        export function foo(): string {}
    "#,
    );
    assert_eq!(exports, vec![Export::Named("foo".into())]);
}

#[test]
fn es6_named_alias() {
    let exports = print_module(
        r#"
        const foo = 123;
        export {foo as bar};
    "#,
    );
    assert_eq!(exports, vec![Export::Named("bar".into())]);
}

#[test]
fn es6_export_alias() {
    let exports = print_module(
        r#"
        export {foo as bar} from 'baz';
        export type {Foo as Bar} from 'baz';
    "#,
    );
    assert_eq!(
        exports,
        vec![Export::Named("bar".into()), Export::NamedType("Bar".into())]
    );
}

#[test]
fn cjs_export_type_alias() {
    let exports = print_module(
        r#"
        export type {Foo as Bar} from 'baz';
    "#,
    );
    assert_eq!(exports, vec![Export::NamedType("Bar".into())]);
}

#[test]
fn cjs_default_remote_ref() {
    let exports = print_module(
        r#"
        import type {T} from './foo';
        module.exports = ({ foo: 123, bar: "bar" }: T);
    "#,
    );
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn cjs_recursive_type() {
    let exports = print_module(
        r#"
        export type T = T;
    "#,
    );
    assert_eq!(exports, vec![Export::NamedType("T".into())]);
}

#[test]
fn cjs_recursive_ref() {
    let exports = print_module(
        r#"
        const T : T = { foo: 123, bar: "bar" };
        module.exports = T;
    "#,
    );
    assert_eq!(exports, vec![Export::Default(None)]);
}

#[test]
fn es6_declare_function() {
    let exports = print_module(
        r#"
        declare function foo(): string;
        export {foo}
    "#,
    );
    assert_eq!(exports, vec![Export::Named("foo".into())]);
}

#[test]
fn es6_declare_class() {
    let exports = print_module(
        r#"
        declare class Foo {};
        export {Foo}
    "#,
    );
    assert_eq!(
        exports,
        vec![Export::NamedType("Foo".into()), Export::Named("Foo".into())]
    );
}

#[test]
fn es6_export_default_from() {
    let exports = print_module(
        r#"
        export {default as View} from 'react-native';
    "#,
    );
    assert_eq!(exports, vec![Export::Named("View".into())]);
}

#[test]
fn es6_interface() {
    let exports = print_module(
        r#"
        export interface Foo { bar: string }
        export default 0; // need an exported value to force ES6 modules
    "#,
    );
    assert_eq!(
        exports,
        vec![Export::Default(None), Export::NamedType("Foo".into())]
    );
}

#[test]
fn cjs_interface() {
    let exports = print_module(
        r#"
        export interface Foo { bar: string }
    "#,
    );
    assert_eq!(exports, vec![Export::NamedType("Foo".into())]);
}

#[test]
fn lib_class() {
    let exports = print_builtins(
        r#"
        declare class Foo {}
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::NamedType("Foo".into()),
            Export::Named("Foo".into()),
            Export::Named("globalThis".into())
        ]
    );
}

#[test]
fn lib_type() {
    let exports = print_builtins(
        r#"
        declare type T = string;
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::Named("globalThis".into()),
            Export::NamedType("T".into())
        ]
    );
}

#[test]
fn lib_value() {
    let exports = print_builtins(
        r#"
        declare var foo : string;
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::Named("foo".into()),
            Export::Named("globalThis".into())
        ]
    );
}

#[test]
fn react_dot_ct_1() {
    let exports = print_module(
        r#"
        import * as React from 'react';
        export const x: React.ComponentType<{}> = null;
        export const y: React.ComponentType<{}> = null;
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::NamedType("x".into()),
            Export::Named("x".into()),
            Export::NamedType("y".into()),
            Export::Named("y".into())
        ]
    );
}

#[test]
fn react_dot_ct_2() {
    let exports = print_module(
        r#"
        import React from 'react';
        export const x: React.ComponentType<{}> = null;
        export const y: React.ComponentType<{}> = null;
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::NamedType("x".into()),
            Export::Named("x".into()),
            Export::NamedType("y".into()),
            Export::Named("y".into())
        ]
    );
}

#[test]
fn react_builtin_ct_1() {
    let exports = print_module(
        r#"
        export const x: React.ComponentType<{}> = null;
        export const y: React.ComponentType<{}> = null;
    "#,
    );
    assert_eq!(
        exports,
        vec![
            Export::NamedType("x".into()),
            Export::Named("x".into()),
            Export::NamedType("y".into()),
            Export::Named("y".into())
        ]
    );
}
