---
title: Match Patterns
description: "Match patterns both define a condition that must be matched, and new variables that are extracted (like destructuring)."
slug: /match/patterns
---

Match patterns both define a condition that must be matched, and new variables that are extracted (like destructuring).

## Primitive value patterns

Primitive value patterns include string literals (e.g. `'light'`), number literals (e.g. `42`), BigInt literals (e.g. `10n`), boolean literals (e.g. `true`), `null`, and `undefined`.

You can use variables (e.g. `name`) or property accesses (e.g. `Status.Active`) which are either typed as a [literal type](../types/literals.md), or a [Flow Enum](../enums/index.md) member. Using variables that are a general type like `string` or `number` is not allowed for match patterns \- they must be a literal type like `'light'`. You can add a type annotation or [`as const`](../types/const-expression.md) to your string value to type it with a literal type. Computed properties in match patterns only allow literals like `foo['bar']` or `foo[2]`.

The identifier `_` is special cased for “Wildcard patterns”, if you want to match against a value with that name, rename it to something else first. If you want to create a new variable, take a look at “variable declaration patterns” below \- it is done by doing `const x`.

You can use a number literal prefixed with  `+` or `-`, or BigInt literals prefixed with `-` (`+` on a BigInt is an error in JS). `+0` and `-0` are not allowed (Flow doesn’t differentiate these type-wise from `0`). `NaN` is special-cased, since `NaN === NaN` is always `false`. It is matched using `Number.isNaN`.

Other types of expressions are not supported. To match against an arbitrary expression (which has a literal type), first assign it to a variable, and then match against that variable.

Example:
```js flow-check
const Status = {active: 'active', paused: 'paused'} as const;
declare const ranks: [10, 20];
declare const fallbackLabel: 'unknown';

declare const value: unknown;

match (value) {
  0 => {}
  'archived' => {}
  null => {}
  -1 => {}
  fallbackLabel => {} // a literal-typed variable
  Status.active => {} // a property access
  Status['paused'] => {} // a computed string member
  ranks[0] => {} // a computed number member
  _ => {}
}
```

Flow will never output an “unused pattern” error for the `undefined` pattern, so you can always add it even if the input type does not contain `undefined`. This is to cover the case where Flow does not compute a type that is 100% accurate to the runtime value (for example, from indexed access of an array past its length).

At runtime, these checks are done using triple equals `===`.

## Wildcard patterns

Wildcard patterns, which are a single underscore `_`, match everything. If you want to match against the value of a variable named `_`, assign it to a different name first.

If part of your input type cannot be matched exhaustively (e.g. `string`), then the `match` will require a wildcard.

Example:
```js flow-check
declare const username: string;

match (username) {
  _ => {}
}
```

## Variable declaration patterns

Variable declaration patterns, like `const name`, take whatever value is at that position and assign it to a new variable. Conditional check wise, they act like a wildcard and match everything.

Example:
```js flow-check
declare const input: string;

const greeting = match (input) {
  const name => `Hello, ${name}!`,
};
```

While `let` variables are also supported by the runtime, these are a type error for now and only `const` variables are allowed. If you have a use case, please share it with the team. `var` is not supported.

## Object patterns

Object patterns match object values with the same structure. For example, the pattern `{type: 'light', num: 42}` matches objects which have the property `type` with value `'light'`, and a `num` property with value `42`.  If the object value has additional unlisted properties, or is inexact, you need to make your pattern inexact using `...`, for example `{type: 'light', num: 42, ...}`.

Like destructuring, a variable declaration pattern nested inside an object pattern creates a new variable with the value of that property. E.g. `match (arg) { {prop: const x} => x }` will initialize `x` with the value `arg.prop`. You can use object rest to gather the rest of the object’s own properties: `{foo: 1, ...const rest}`.

Doing just `{name}` is ambiguous \- it could mean `{name: name}` (matching against the value of variable `name`) or `{name: const name}` (extracting the property’s value as a new variable called `name`), so it’s not allowed. If you want a shorthand for creating new variables, you can use `{const name}` to mean `{name: const name}`.

If you have a property with either a wildcard or variable declaration pattern, e.g. `{foo: _}` or `{foo: const x}`, it is checked that the property is [in](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/in) the object (checks own and non-own properties).

When checking objects with optional properties, in order to make the check exhaustive you must include a pattern that doesn’t include the optional properties. For example, for `{name: string, age?: number}`, if you first match with the pattern `{name: _, age: _}`, you still need to match with the pattern `{name: _, ...}` in order to handle the cases where the `age` property doesn’t exist.

Property names can be identifiers (e.g. `foo: pattern`), string literals (e.g. `'foo': <pattern>`), or number literals (e.g. `2: <pattern>`). Repeated object keys are banned, and BigInts are not supported as object keys (Flow doesn’t yet support them).

Example:
```js flow-check
declare const event:
  | {kind: 'click', x: number, y: number}
  | {message: string}
  | {kind: 'scroll', delta: number};

const summary = match (event) {
  {kind: 'click', x: const x, y: const y} => `click at ${x},${y}`,
  {const message} => message,
  {kind: 'scroll', ...const rest} => `scroll ${rest.delta}`,
};
```

[Getters](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Functions/get) are also not supported \- at runtime they will be evaluated multiple times, once for each conditional check done against them.

## Instance patterns

Instance patterns match instances of a class. The pattern is a class name followed by an object pattern, for example `Point {x: 1, ...}` matches values that are instances of `Point` whose `x` property is `1`. At runtime this is like checking `p instanceof Point && p.x === 1`.

The object pattern part works just like a regular [object pattern](#object-patterns): you can extract properties with variable declaration patterns, use the `{const x}` shorthand, and gather the rest with object rest.

Instances are always inexact, since a class can be extended with additional properties, so you must end the pattern with `...`.

Example:
```js flow-check
class Point {
  x: number;
  y: number;
}
declare const p: Point;

const sum = match (p) {
  Point {const x, const y, ...} => x + y,
};
```

If you leave off the `...`, Flow errors, because the instance could have additional properties:
```js flow-check
class Point {
  x: number;
  y: number;
}
declare const p: Point;

match (p) {
  Point {const x, const y} => {} // ERROR: instances are inexact, so `...` is required
}
```

Different classes in a union can each be matched by their own pattern, and the body is refined to that class:
```js flow-check
class Shape {
  area(): number {
    return 0;
  }
}
class Circle extends Shape {
  radius: number;
}
class Square extends Shape {
  side: number;
}

declare const shape: Circle | Square;

const size = match (shape) {
  Circle {const radius, ...} => radius, // matches `Circle` instances
  Square {const side, ...} => side, // matches `Square` instances
};
```

Matching follows the prototype chain like `instanceof`: a pattern naming a superclass matches instances of any subclass.
```js flow-check
class Shape {
  area(): number {
    return 0;
  }
}
class Circle extends Shape {
  radius: number;
}
class Square extends Shape {
  side: number;
}

declare const shape: Circle | Square;

const out = match (shape) {
  Shape {...} => shape.area(), // a superclass pattern matches any subclass instance
};
```

[Object patterns](#object-patterns) are structural and also match instances, so `{const x, const y, ...}` matches a `Point`. An instance pattern, on the other hand, only matches instances of the named class, and never a plain object.
```js flow-check
class Point {
  x: number;
  y: number;
}
declare const p: Point;

const sum = match (p) {
  {const x, const y, ...} => x + y, // an object pattern matches the instance
};
```

The constructor must reference a single class. Using a type, interface, or other value is an error.
```js flow-check
class Point {
  x: number;
  y: number;
}

declare const p: Point;
declare const NotAClass: string;

match (p) {
  NotAClass {...} => {} // ERROR: constructor must reference a single class
  _ => {}
}
```

Instance patterns are available by default since Flow v0.317.

## Array patterns

Array patterns match both tuple and array values. For example, the pattern `['small', true]` matches array values whose length is `2` and whose elements match the pattern’s elements. If you want a looser length check, you can make the pattern inexact using `...`, for example `['small', true, ...]` will match arrays whose length is `>= 2` and whose first elements match the pattern’s elements. You can match any array or tuple with `[...]`.

Like destructuring, a variable declaration pattern nested inside an array pattern creates a new variable with the value of that element.  E.g. `match (arg) { [const x] => x }` will initialize `x` with the value `arg[0]`. You can use array rest at the end of the pattern to gather the remaining elements, and check that the length is greater or equal to the pattern length: e.g. `[1, 2, ...const rest]`.

Array patterns match values which pass `Array.isArray`, so they won’t match array-like objects that aren’t actually arrays, and won’t match iterables. Use `Array.from` on those types of values first if you want to match them with array patterns.

Example:
```js flow-check
declare const color: [0, 0, 0] | [255, 0, 0, 1] | [0, 0, 255, 0.5];

const css = match (color) {
  [0, 0, 0] => 'black',
  [255, 0, 0, ...] => 'opaque red',
  [0, 0, 255, ...const rest] => `blue at ${rest[0]} alpha`,
};
```

## “Or” patterns

Or patterns allow you to combine multiple patterns using `|`, for example `'active' | 'paused'` will match either string literal.

[Variable declaration patterns](#variable-declaration-patterns) inside of "or" patterns are not yet supported.

Example:
```js flow-check
declare const reply: 'yes' | 'yeah' | 'no' | 'nope';

const answer = match (reply) {
  'yes' | 'yeah' => true,
  'no' | 'nope' => false,
};
```

## “As” patterns

As patterns both match a pattern and create a new variable. For example, `[_, _] as pair` will first match any arrays whose length is `2`, and then assign that value to a new variable called `pair`.

Example:
```js flow-check
declare const result: ['ok', number] | ['error', string];

const handled = match (result) {
  ['ok', _] as success => success,
  ['error', _] as failure => failure,
};
```

## See Also {#toc-see-also}

- [Tuples](../types/tuples.md) — tuple patterns in match expressions
- [Objects](../types/objects.md) — object patterns in match expressions
- [Classes](../types/classes.md) — instance patterns in match expressions
