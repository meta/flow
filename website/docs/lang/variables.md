---
title: Variable Declarations
slug: /lang/variables
description: "How Flow types var, let, and const variable declarations, including reassignment behavior."
---

When you are declaring a new variable, you may optionally declare its type.

JavaScript has three ways of declaring local variables:

- `var` - declares a variable, optionally assigning a value.
  ([MDN](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Statements/var))
- `let` - declares a block-scoped variable, optionally assigning a value.
  ([MDN](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Statements/let))
- `const` - declares a block-scoped variable, assigning a value that cannot be
  re-assigned.
  ([MDN](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Statements/const))

In Flow these fall into two groups:

- `let` and `var` - variables that **can** be reassigned.
- `const` - variables that **cannot** be reassigned.

```js flow-check
var varVariable = 1;
let letVariable = 1;
const constVariable = 1;

varVariable = 2;   // Works!
letVariable = 2;   // Works!
constVariable = 2; // Error!
```

## `const` {#toc-const}

Since a `const` variable cannot be re-assigned at a later time it is fairly
simple.

Flow can either infer the type from the value you are assigning to it or you can
provide it with a type.

```js flow-check
const foo /* : number */ = 1;
const bar: number = 2;
```

## `var` and `let` {#toc-var-and-let}

Since `var` and `let` can be re-assigned, there's a few more rules you'll need
to know about.

When you provide a type, you will be able to re-assign the value, but it must
always be of a compatible type.

```js flow-check
let foo: number = 1;
foo = 2; // Works!
foo = '3'; // Error!
```

When the variable has no annotation, Flow infers a precise type based on their
initializer or initial assignment. All subsequent assignments to that variable
will be constrained by this type. This section shows some examples of how Flow
determines what type an unannotated variable is inferred to have.

**If you want a variable to have a different type than what Flow infers for it,
you can always add a type annotation to the variable’s declaration. That will
override everything discussed in this page!**

### Variables initialized at their declarations {#toc-variables-initialized-at-their-declarations}

The common case for unannotated variables is very straightforward: when a
variable is declared with an initializer that is not the literal `null`, that
variable will from then on have the type of the initializer, and future writes
to the variable will be constrained by that type.

```js flow-check
import * as React from 'react';

type Props = Readonly<{prop: string}>;

declare const x: number;
declare const y: number;
declare const props: Props;

let product = Math.sqrt(x) + y;
// `product` has type `number`

let Component = ({prop}: Props): React.Node => {
  return <div />;
};
// `Component` has type `component(prop: string)`

let element = <Component {...props} />;
// `element` has type `React.MixedElement`

/* Let's define a new component */

type OtherProps = Readonly<{...Props, extra_prop: number}>;
declare const OtherComponent: (props: OtherProps) => React.Node;
declare const other_props: OtherProps;

/* Any subsequent assignments to `product`, `Component`, or `element` will be
 * checked against the types that Flow infers for the initializers, and if
 * conflicting types are assigned, Flow will signal an error. */

product = 'Our new product is...'; // Error
Component = ({prop}: OtherProps): React.Node => { // Error
  return <div />;
};
element = <OtherComponent {...other_props} />;
```

If you want these examples to typecheck, and for Flow to realize that different
kinds of values can be written to these variables, you must add a type
annotation reflecting this more general type to their declarations:

```js flow-check
import * as React from 'react';

let product: number | string = 1;
let Component: unknown = null; // No good type to represent this! Consider restructuring
let element: React.Node = <div />;
```

### Variables declared without initializers {#toc-variables-declared-without-initializers}

Often variables are declared without initializers. In such cases, Flow will try
to choose the "first" assignment or assignments to the variable to define its
type. "First" here means both top-to-bottom and nearer-scope to
deeper-scope—we’ll try to choose an assignment that happens in the same function
scope as the variable’s declaration, and only look inside nested functions if we
don’t find any assignments locally:

```js flow-check
let topLevelAssigned;
function helper() {
  topLevelAssigned = 42; // Error: `topLevelAssigned` has type `string`
}
topLevelAssigned = 'Hello world'; // This write determines the var's type
topLevelAssigned = true; // Error: `topLevelAssigned` has type `string`
```

If there are two or more possible "first assignments," due to an `if`- or
`switch`-statement, they’ll both count—this is one of the few ways that Flow
will still infer unions for variable types:

```js flow-check
let myNumberOrString;
declare const condition: boolean;
if (condition) {
  myNumberOrString = 42; // Determines type
} else {
  myNumberOrString = 'Hello world'; // Determines type
}
myNumberOrString = 21; // fine, compatible with type
myNumberOrString = 'Goodbye'; // fine, compatible with type
myNumberOrString = false; // Error: `myNumberOrString` has type `number | string`
```

This only applies when the variable is written to in both branches, however. If
only one branch contains a write, that write becomes the type of the variable
afterwards (though Flow will still check to make sure that the variable is
definitely initialized):

```js flow-check
let oneBranchAssigned;
declare const condition: boolean;
if (condition) {
  oneBranchAssigned = 'Hello world!';
}
oneBranchAssigned.toUpperCase(); // Error: `oneBranchAssigned` may be uninitialized
oneBranchAssigned = 42; // Error: `oneBranchAssigned` has type `string`
```

### Variables initialized to `null` {#toc-variables-initialized-to-null}

Finally, the one exception to the general principle that variable’s types are
determined by their first assignment(s) is when a variable is initialized as (or
whose first assignment is) the literal value `null`. In such cases, the _next_
non-null assignment (using the same rules as above) determines the rest of the
variable’s type, and the overall type of the variable becomes a union of `null`
and the type of the subsequent assignment. This supports the common pattern
where a variable starts off as `null` before getting assigned by a value of some
other type:

```js flow-check
function findIDValue<T>(dict: {[key: string]: T}): T {
  let idVal = null; // initialized as `null`
  for (const key in dict) {
    if (key === 'ID') {
      idVal = dict[key]; // Infer that `idVal` has type `null | T`
    }
  }
  if (idVal === null) {
    throw new Error('No entry for ID!');
  }
  return idVal;
}
```

## Catch variables

If a `catch` variable does not have an annotation, its default type is
[`any`](../types/any.md).

You can optionally annotate it with exactly [`unknown`](../types/unknown.md) or
`any`. E.g.

```js flow-check
try {
} catch (e: unknown) {
  if (e instanceof TypeError) {
    e as TypeError; // OK
  } else if (e instanceof Error) {
    e as Error; // OK
  } else {
    throw e;
  }
}
```

By using `unknown`, you can improve your safety and Flow
[coverage](../cli/coverage.md), at the trade-off of increased runtime checks.

You can change the default type of `catch` variables when there is no annotation
by setting the
[`use_unknown_in_catch_variables`](../config/options.md#toc-use-unknown-in-catch-variables)
option to true.

## See Also {#toc-see-also}

- [Annotation Requirement](./annotation-requirement.md) — when Flow requires explicit type annotations
- [Refinements](./refinements.md) — how variable types are narrowed by conditional checks
- [Const Expressions](../types/const-expression.md) — using `as const` to infer narrower types for variable initializers
- [Type Casting](../types/casting.md) — using `as` to widen or assert variable types
