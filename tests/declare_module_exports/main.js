import declare_module_exports from "declare_module_exports";
declare_module_exports as number;
declare_module_exports as string; // Error: number ~> string

// Error: Has no named export "str"!
import {str} from "declare_m_e_with_other_value_declares";

import type {str2} from "declare_m_e_with_other_type_declares";
"asdf" as str2;
42 as str2; // Error: number ~> string

/**
 * `declare var exports` is deprecated, so we have a grace period where both
 * syntaxes will work.
 */

import declare_var_exports from "declare_var_exports";
declare_var_exports.exports as number; // ok
declare_var_exports.exports as string; // Error: number ~> string

import declare_m_e_with_declare_var_e from "declare_m_e_with_declare_var_e";
declare_m_e_with_declare_var_e as number;
declare_m_e_with_declare_var_e as string; // Error: number ~> string

import { foo } from "declare_overloaded_function";
foo(0) as number;
foo(0) as string; // Error: number ~> string
foo("") as string;
foo("") as number; // Error: string ~> number

import type {BT} from "B";
42 as BT; // ok
"str" as BT; // Error: string ~> number

import type BDefault from "B";
import BDefaultValue from "B";
new BDefaultValue() as BDefault; // ok
42 as BDefault; // Error: number ~> Def

// import between libdef files
import type {CT} from "C";
import type {DT} from "D"
{
  declare const cVal: CT;
  const dVal = {C: cVal};
  cVal as CT; // ok
  cVal.D as DT; // ok
  dVal as DT; // ok
  dVal.C as CT; // ok
}

import E from "E";
new E() as BDefault; // ok
new E() as empty; // error

/**
 * TODO: At the moment it isn't possible to import a non-libdef module from a
 *       libdef. There's no good reason to ban this, it's just a limitation of
 *       the way Flow handles libdefs at the moment. We should fix this test
 *       to pass at some point.
 */
// import {T} from "DependsOnRealModule";
// (42: T);
// ("str": T);

import {ok as callSignaturesOk} from "declare_m_e_with_call_signatures";
callSignaturesOk as number; // ok
callSignaturesOk as string; // Error: number ~> string

import {ok as overloadedNamespaceOk} from "declare_m_e_overloaded_function_and_namespace";
overloadedNamespaceOk as number; // ok
overloadedNamespaceOk as string; // Error: number ~> string

import {ok as callableValueOk} from "./callable_exports";
callableValueOk as number; // ok
callableValueOk as string; // Error: number ~> string
