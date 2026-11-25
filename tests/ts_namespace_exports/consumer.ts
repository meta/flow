/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import { API, Types, Shadow } from './standalone';
import { API as FunctionAPI, ClassAPI, ValuesOnly, RuntimeAPI } from './merged';
import { BeforeClass, BeforeFunction } from './before';
import CjsAPI, { Member, value } from './cjs';
import CjsFunction, { call } from './cjs_function';

// Value aliases preserve their parameter and return types, including in mixed exports.
API.call('x') satisfies number;
API.call(1); // ERROR
API.call('x') satisfies string; // ERROR
API.mixedCall('x') satisfies number;
API.mixedCall('x') satisfies string; // ERROR

// Both explicit type exports and plain exports of type aliases remain type-only.
const item: API.Item = 1;
const inlineItem: API.InlineItem = 1;
const plainItem: API.PlainItem = 1;
const badItem: API.Item = 'x'; // ERROR
const badInlineItem: API.InlineItem = 'x'; // ERROR
const badPlainItem: API.PlainItem = 'x'; // ERROR
API.Item; // ERROR
API.InlineItem; // ERROR

// Ambient variables and functions retain their declared types after import.
API.value satisfies number;
API.value satisfies string; // ERROR
API.mutable satisfies string;
API.variable satisfies boolean;
API.helper('x') satisfies number;
API.helper(1); // ERROR

// Exported classes provide instance types, instance members, and static methods.
const member: API.Member = new API.Member();
member.method('x') satisfies number;
member.method(1); // ERROR
member.field satisfies string; // ERROR
API.Member.make('x').field satisfies number;

// A namespace containing only types has no runtime value.
const typeItem: Types.Item = 'x';
const typeInlineItem: Types.InlineItem = 'x';
const badTypeItem: Types.Item = 1; // ERROR
const badTypeInlineItem: Types.InlineItem = 1; // ERROR
Types; // ERROR
const typeNamespace = Types; // ERROR

// Aliases resolve shadowed names and forward declarations without exposing local names.
const shadowItem: Shadow.Item = 'x';
const badShadowItem: Shadow.Item = 1; // ERROR
const forwardItem: Shadow.ForwardItem = true;
const badForwardItem: Shadow.ForwardItem = 'x'; // ERROR
const implicitForwardItem: Shadow.ImplicitForwardItem = true;
const badImplicitForwardItem: Shadow.ImplicitForwardItem = 'x'; // ERROR
Shadow.forwardCall('x') satisfies number;
Shadow.forwardCall(1); // ERROR
Shadow.Local; // ERROR

// A merged function remains callable and exposes its namespace's values and types.
FunctionAPI('x') satisfies number;
FunctionAPI.call('x') satisfies number;
FunctionAPI.call(1); // ERROR
const functionItem: FunctionAPI.Item = 1;
const badFunctionItem: FunctionAPI.Item = 'x'; // ERROR
new FunctionAPI.Member().field satisfies number;
new FunctionAPI.Nested().field satisfies number;
FunctionAPI.value satisfies number;
FunctionAPI.helper('x') satisfies number;

// A namespace containing only value exports also merges into a function.
ValuesOnly.call('x') satisfies number;
ValuesOnly.call(1); // ERROR

// A merged class keeps its constructor, namespace statics, and qualified types.
new ClassAPI().field satisfies number;
ClassAPI.call('x') satisfies number;
ClassAPI.call(1); // ERROR
new ClassAPI.Member().field satisfies number;
ClassAPI.value satisfies number;
ClassAPI.value satisfies string; // ERROR
const classItem: ClassAPI.Item = 1;
const badClassItem: ClassAPI.Item = 'x'; // ERROR

// Runtime classes retain generic inference and their own statics alongside namespace members.
new RuntimeAPI('x').field satisfies string;
new RuntimeAPI('x').field satisfies number; // ERROR
RuntimeAPI.call('x') satisfies number;
RuntimeAPI.own('x') satisfies number;

// Namespaces and merged functions/classes can be used as runtime values.
const runtimeNamespace = API;
runtimeNamespace.call('x') satisfies number;
const runtimeFunction = FunctionAPI;
runtimeFunction('x') satisfies number;
const runtimeClass = ClassAPI;
new runtimeClass().field satisfies number;

// Type-only namespaces also merge when declared before their class or function.
new BeforeClass().field satisfies number;
const beforeClassItem: BeforeClass.Item = 1;
BeforeFunction('x') satisfies number;
const beforeFunctionItem: BeforeFunction.Item = 1;

// CommonJS class exports preserve the default constructor and named namespace members.
new CjsAPI().field satisfies number;
new Member().field satisfies number;
value satisfies number;
value satisfies string; // ERROR

// CommonJS function exports preserve the default call signature and named aliases.
CjsFunction('x') satisfies number;
call('x') satisfies number;
call(1); // ERROR

// A class merged with a namespace, used as a type in a signature, is the instance type.
import { viaImportType, viaNamespace, viaName } from './merged_class_signature';
viaImportType().field satisfies number;
viaImportType().field satisfies string; // ERROR
viaNamespace().field satisfies string; // ERROR
viaName().field satisfies string; // ERROR
