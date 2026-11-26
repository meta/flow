---
title: Annotation Requirement
slug: /lang/annotation-requirement
description: "Understand when Flow requires explicit type annotations under Local Type Inference and when types can be inferred automatically."
---

Flow uses [Local Type Inference](https://medium.com/flow-type/local-type-inference-for-flow-aaa65d071347) as its inference algorithm. The rules in this section reflect the main design features in this inference scheme.

Flow tries to avoid requiring type annotation for parts of programs where types can easily
be inferred from the immediate context of an expression, variable, parameter, etc.

:::info TypeScript comparison
TypeScript infers types across module boundaries by default, while Flow requires annotations at the boundary (function parameters, exports) to enable parallel checks. TypeScript offers an opt-in equivalent (`--isolatedDeclarations`, TS 5.5+), but it's not part of `strict` and is off by default. See [Annotations are required at module boundaries](../flow-vs-typescript.md#toc-annotations-boundaries) for the full comparison.
:::

## Variable declarations

Take for example the following variable definition
```js flow-check
const len = "abc".length;
```
All information necessary to infer the type of `len` is included in the initializer
`"abc".length`. Flow will first determine that `"abc"` is a string, and then that the
`length` property of a string is a number.

The same logic can be applied for all `const`-like initializations. Where things
get a little more complicated is when variable initialization spans across multiple statements,
for example in
```js flow-check
declare const maybeString: ?string;

let len;
if (typeof maybeString === "string") {
  len = maybeString.length;
} else {
  len = 0;
}
```
Flow can still determine that `len` is a `number`, but in order to do so it looks
ahead to multiple initializer statements. See section on [variable declarations](./variables.md)
for details on how various initializer patterns determine the type of a variable,
and when an annotation on a variable declaration is necessary.

## Function Parameters

Unlike variable declarations, this kind of "lookahead" reasoning cannot be used to determine
the type of function parameters. Consider the function
```js
function getLength(x) {
  return x.length;
}
```
There are many kinds of `x` on which we could access and return a `length` property:
an object with a `length` property, or a string, just to name a few. If later on in
the program we had the following calls to `getLength`
```js
getLength("abc");
getLength({length: 1});
```
one possible inference would be that `x` is a `string | { length: number }`. What this implies,
however, is that the type of `getLength` is determined by any part of the current
program. This kind of global reasoning can lead to surprising action-at-a-distance
behavior, and so is avoided. Instead, Flow requires that function parameters are annotated. Failure to
provide such a type annotation manifests as a `[missing-local-annot]` error on the parameter `x`,
and the body of the function is checked with `x: any`:
```js flow-check
function getLength(x) { // Error
  return x.length;
}

const n = getLength(1); // no error since getLength's parameter type is 'any'
```

To fix this error, one can simply annotate `x` as
```js flow-check
function getLength(x: string) {
  return x.length;
}
```
The same requirement holds for class methods
```js flow-check
class WrappedString {
  data: string;
  setStringNoAnnotation(x) { // Error
    this.data = x;
  }
  setString(x: string) {
    this.data = x;
  }
}
```

## Contextual Typing {#toc-contextual-typing}

Function parameters do not always need to be explicitly annotated. In the case of a
callback function to a function call, the parameter type can easily
be inferred from the immediate context. Consider for example the following code
```js flow-check
const arr = [0, 1, 2];
const arrPlusOne = arr.find(x => x % 2 === 1);
```
Flow infers that the type of `arr` is `Array<number>`. Combining this with the builtin
information for `Array.find`, Flow can determine that the type of `x => x % 2 === 1`
needs to be `number => unknown`. This type acts as a *hint* for Flow and provides enough
information to determine the type of `x` as `number`.

A surrounding type annotation can also help Flow infer a function parameter's type. For example:
```js flow-check
const fn1: (x: number) => number = x => x + 1;
```
However, it is also possible that an annotation cannot be used as a function
parameter hint:
```js flow-check
const fn2: unknown = x => x + 1; // Error
```
In this example the `unknown` type simply does not include enough information to
extract a candidate type for `x`.

Flow can infer the types for unannotated parameters even when they are nested within
other expressions like objects. For example in
```js flow-check
const fn3: {f: (x: number) => void} = {f: (x) => {x as string}}; // Error
```
Flow will infer `number` as the type of `x`, and so the cast fails.


## Function Return Types {#toc-function-return-types}

Unlike function parameters, a function's return type does not need to be annotated in general.
So the above definition of `getLength` won't raise any Flow errors.

There are, however, a couple of notable exceptions to this rule. The first one is
class methods. If we included to the `WrappedString` class a `getString` method
that returns the internal `data` property:
```js flow-check
class WrappedString {
  data: string;
  getString(x: string) { // Error
    return this.data;
  }
}
```
Flow would complain that `getString` is missing an annotation on the return.

The second exception is recursive definitions. A trivial example of this would be
```js flow-check
function foo() { // Error
  return bar();
}

function bar() {
  return foo();
}
```
The above code raises a `[definition-cycle]` error, which points to the two locations
that form a dependency cycle, the two missing return annotations. Adding
a return annotation to either function would resolve the issue.

Effectively, the requirement on an annotation for method returns is a special-case
of the recursive definition restriction. The recursion is possible through access on
`this`.

## Generic Calls {#toc-generic-calls}

In calls to [generic functions](../types/generics.md) the type of the result may
depend on the types of the values passed in as arguments.
This section discusses how this result is computed, when type arguments are not
explicitly provided.

Consider for example the definition of `map`, together with a potential call with arguments `x => x + 1` and `[1, 2, 3]`:
```js flow-check
declare function map<T, U>(
  f: (x: T) => U,
  array: ReadonlyArray<T>,
): Array<U>;

map(x => x + 1, [1, 2, 3]);
```
Here Flow infers that the type of `x` is `number`.

Some other common examples of generic calls are calling the constructor of the generic
[`Set` class](https://github.com/meta/flow/blob/82f88520f2bfe0fa13748b5ead711432941f4cb9/lib/core.js#L1799-L1801)
or calling `useState` from the React library:
```js flow-check
import {useState} from 'react';

const set = new Set([1, 2, 3]);

component Example() {
  const [num, setNum] = useState(42);
  return null;
}
```
Flow here infers that the type of `set` is `Set<number>`, and that `num` and `setNum`
are `number` and `(number) => void`, respectively.

### Computing a Solution

Computing the result of a generic call amounts to:
1. coming up with a solution for `T` and `U` that does not contain generic parts,
2. replacing `T` and `U` with the solution in the signature of `map`, and
3. performing a call to this new signature of `map`.

This process is designed with two goals in mind:
* *Soundness*. The results need to lead to a correct call when we reach step (3).
* *Completeness*. The types Flow produces need to be as precise and informative as possible,
to ensure that other parts of the program will be successfully checked.

Let's see how these two goals come into play in the `map` example from above.

Flow detects that `ReadonlyArray<T>` needs to be compatible with the type of `[1, 2, 3]`.
It can therefore infer that `T` is `number`.

With the knowledge of `T` it can now successfully check `x => x + 1`. The parameter `x`
is contextually typed as `number`, and thus the result `x + 1` is also a number.
This final constraint allows us to compute `U` as a `number` too.

The new signature of `map` after replacing the generic parts with the above solution
is
```js
(f: (number) => number, array: ReadonlyArray<number>) => Array<number>
```
It is easy to see that the call would be successfully checked.

### Errors during Polymorphic Calls

If the above process goes on smoothly, you should not be seeing any errors associated with the call.
What happens though when this process fails?

There are two reasons why this process could fail:

#### Under-constrained Type Parameters

There are cases where Flow might not have enough information to decide the type of a type parameter.
Let's examine again a call to the builtin generic
[`Set` class](https://github.com/meta/flow/blob/82f88520f2bfe0fa13748b5ead711432941f4cb9/lib/core.js#L1799-L1801)
constructor, this time without passing any arguments:
```js flow-check
const set = new Set(); // Error
set.add("abc");
```
During the call to `new Set`, we are not providing enough information for Flow to
determine the type for `T`, even though the subsequent call to `set.add` clearly
implies that `T` will be a string. Remember that inference of type arguments is
local to the call, so Flow will not attempt to look ahead in later statements
to determine this.

In the absence of information, Flow would be at liberty to infer *any* type
as `T`: `any`, `mixed`, `empty`, etc.
This kind of decision is undesirable, as it can lead to surprising results.
For example, if we silently decided on `Set<empty>` then the call to `set.add("abc")` would
fail with an incompatibility between `string` and `empty`, without a clear indication
of where the `empty` came from.

So instead, in situations like this, you'll get an `[underconstrained-implicit-instantiation]` error.
The way to fix this error is by adding a type annotation. There a few potential ways to do this:

- Add an annotation at the call-site in one of two ways:
  * an explicit type argument
    ```js
    const set = new Set<string>();
    ```
  * an annotation on the initialization variable:
    ```js
    const set: Set<string> = new Set();
    ```

- Add a default type on the type parameter `T` at the definition of the class:
    ```js
    declare class SetWithDefault<T = string> extends ReadonlySet<T> {
      constructor(iterable?: ?Iterable<T>): void;
      // more methods ...
    }
    ```
  In the absence of any type information at the call-site, Flow will use the default
  type of `T` as the inferred type argument:
  ```js
  const defaultSet = new SetWithDefault(); // defaultSet is SetWithDefault<string>
  ```

#### Incompatibility Errors

Even when Flow manages to infer non-generic types for the type parameters in a generic
call, these types might still lead to incompatibilities either in the current call or in
code later on.

For example, if we had the following call to `map`:
```js flow-check
declare function map<T, U>(f: (x: T) => U, array: ReadonlyArray<T>): Array<U>;
map(x => x + 1, [{}]); // Error
```
Flow will infer `T` as `{}`, and therefore type `x` as `{}`. This will cause an error when checking the arrow function
since the `+` operation is not allowed on objects.

Finally, a common source of errors is the case where the inferred type in a generic
call is correct for the call itself, but not indicative of the expected use later in the code.
For example, consider
```js flow-check
import {useState} from 'react';

component Example() {
  const [str, setStr] = useState("");

  declare const maybeString: ?string;
  setStr(maybeString); // Error
  return null;
}
```
Passing the string `""` to the call to `useState` makes Flow infer `string` as the type
of the state. So `setStr` will also expect a `string` as input when called later on,
and therefore passing a `?string` will be an error.

Again, to fix this error it suffices to annotate the expected "wider" type of state
when calling `useState`:
```js flow-check
import {useState} from 'react';

component Example() {
  const [str, setStr] = useState<?string>("");
  return null;
}
```

## Module Exports {#toc-module-exports}

Flow builds type signatures for each module based solely on the module's exports,
without analyzing the module's internal implementation. This process is called
**signature extraction**: Flow constructs a "typed interface" for each module — a
summary of every exported value's type — by looking only at the export site, not
the module body. This is what enables Flow to typecheck modules in parallel and
provide fast IDE feedback: each module's type can be understood without
typechecking its dependencies' implementations.

For example, an exported function needs parameter and return annotations:

```js flow-check
// Works: Flow can build a signature from the annotations
export function getLength(x: string): number {
  return x.length;
}

export function getLength2(x) { // Error: Flow cannot determine the parameter or return type
  return x.length;
}
```

Exported variables typically don't need annotations when their type can be
determined from the initializer:

```js flow-check
declare const items: Array<string>;

export const name = "Alice"; // OK: type is clearly 'string'
export const count = items.length; // OK: type determined from .length
```

If Flow cannot determine the type of an export, you'll see a `[signature-verification-failure]`
error with the message "Cannot build a typed interface for this module." The fix
is to add a type annotation to the export.

### Unsupported expression forms in exports {#toc-unsupported-expression-forms}

Beyond missing annotations, certain expression forms are not supported in
exported positions because Flow cannot determine their types during signature
extraction. These typically produce a `[signature-verification-failure]` error
with a specific sub-message:

- **Unexpected object key** — exported objects must use simple literal keys (string
  or number literals, identifiers). Computed keys prevent Flow from knowing the
  shape of the object at the export site.
  ```js flow-check
  const key = "x";
  export const obj = {[key]: 1}; // Error: Expected simple key in object
  ```
  Fix: annotate the export, e.g., `export const obj: {x: number} = {[key]: 1}`.

- **Unexpected array spread** — spreading into an exported array literal is not
  supported because Flow would need to evaluate the spread to determine the
  array's element type.
  ```js flow-check
  const xs = [1, 2];
  export const ys = [...xs, 3]; // Error: Unexpected spread in array
  ```
  Fix: add a type annotation, e.g., `export const ys: Array<number> = [...xs, 3]`.

- **Array hole** — exported arrays with holes (elisions like `[1, , 3]`) are
  not supported.
  ```js flow-check
  export const arr = [1, , 3]; // Error: Unexpected array hole
  ```
  Fix: fill in the hole or annotate the export, e.g., `export const arr: Array<number | undefined> = [1, , 3]`.

- **Empty array** — an empty array `[]` in an export position has no elements
  for Flow to infer an element type from.
  ```js flow-check
  export const items = []; // Error
  ```
  Fix: add a type annotation, e.g., `export const items: Array<string> = []`.

- **Unsupported expression** — some complex expressions (e.g., conditionals,
  function calls, logical expressions) cannot be evaluated during signature
  extraction.
  ```js flow-check
  declare const condition: boolean;
  export const value = condition ? 1 : "a"; // Error
  ```
  Fix: add a type annotation, e.g., `export const value: number | string = condition ? 1 : "a"`.

### Generic Context {#toc-generic-context}

When a variable declared in an outer scope is only assigned inside a generic
function, Flow cannot safely infer the variable's type. The assignments happen
in a "generic context" — they involve type parameters that could take on any
concrete type at each call site. Since the variable lives outside the generic
function, Flow needs to know its type independently of any particular call.

You'll see an `[invalid-declaration]` error like: "Variable X should be
annotated, because it is only initialized in a generic context."

```js flow-check
let lastSeen; // Error: only initialized in a generic context
function remember<T>(value: T): T {
  lastSeen = value;
  return value;
}
console.log(lastSeen);
```

The fix is to add a type annotation to the variable:

```js flow-check
let lastSeen: unknown; // Works!
function remember<T>(value: T): T {
  lastSeen = value;
  return value;
}
console.log(lastSeen);
```

This also applies when a variable is initialized to `null` and only otherwise
assigned in a generic context — you'll see: "Variable X should be annotated,
because it is only ever assigned to by `null` and in generic context."

```js flow-check
let cached = null; // Error: only assigned by null and in generic context
function cache<T>(value: T): T {
  cached = value; // Error: T is incompatible with null
  return value;
}
console.log(cached);
```

As with the previous example, the fix is to annotate the variable:

```js flow-check
let cached: unknown = null; // Works!
function cache<T>(value: T): T {
  cached = value;
  return value;
}
console.log(cached);
```

## Empty Array Literals {#toc-empty-array-literals}
Empty array literals (`[]`) are handled specially in Flow. You can read about their [behavior and requirements](../types/arrays.md#toc-empty-array-literals).

## See Also {#toc-see-also}

- [Variables](./variables.md) — how variable declarations determine when annotations are needed
- [Generics](../types/generics.md) — generic return types are a common trigger for annotation requirements
- [Functions](../types/functions.md) — function parameters typically require annotations
