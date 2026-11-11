---
title: Using enums
slug: /enums/using-enums
description: "How to use Flow Enums: accessing members, using as type annotations, exhaustive checking, methods, and more."
---

Flow Enums are not a syntax for [union types](../types/unions.md). They are their own type, and each member of a Flow Enum has the same type.
Large union types can cause performance issues, as Flow has to consider each member as a separate type. With Flow Enums, no matter how large your enum is,
Flow will always exhibit good performance as it only has one type to keep track of.

### Accessing enum members {#toc-accessing-enum-members}
Access members with the dot syntax. You can’t use computed access:

```js flow-check
enum Status {
  Active,
  Paused,
  Off,
}

// Access members with the dot syntax:
const status = Status.Active;

// You can’t use computed access:
const x = "Active";
Status[x]; // Error: computed access on enums is not allowed
```

### Using as a type annotation {#toc-using-as-a-type-annotation}
The enum declaration defines both a value (from which you can access the enum members and methods) and a type of the same name, which is the type of the enum members.

```js flow-check
enum Status {Active, Paused, Off}

declare function calculateStatus(): Status;

const status: Status = calculateStatus();
```

### Casting to representation type {#toc-casting-to-representation-type}
Enums do not implicitly coerce to their representation type or vice-versa.
If you want to convert from the enum type to the representation type, you can use an explicit cast `x as string`:

```js flow-check
enum Status {Active, Paused, Off}

const s: string = Status.Active; // Error: 'Status' is not compatible with 'string'

declare const status: Status;
const statusString: string = status as string;
```

You can also call `.valueOf()` on the enum value.
This is particularly useful if you are dealing with [abstract enums](#toc-abstract-enums) and don't know the representation type.

```js flow-check
enum Status {Active, Paused, Off}

declare const status: Status;
const statusString: string = status.valueOf();
```

To convert from a nullable enum type to nullable representation type, you can do:

```js flow-check
enum Status {Active, Paused, Off}
declare const maybeStatus: ?Status;

// Using `as` cast and `&&`:
const maybeStatusString1: ?string = maybeStatus && (maybeStatus as string);

// Using `.valueOf()` and optional chaining:
const maybeStatusString2: ?string = maybeStatus?.valueOf();
```

If you want to convert from the representation type (e.g. `string`) to an enum type (if valid), check out the [cast method](#toc-cast).

### Methods {#toc-methods}
Enum declarations also define some helpful methods.

Below, `TEnum` is the type of the enum (e.g. `Status`), and `TRepresentationType` is the type of the representation type for that enum (e.g. `string`).

#### .cast {#toc-cast}
Type: `cast(input: ?TRepresentationType): TEnum | void`

The `cast` method allows you to safely convert a primitive value, like a `string`, to the enum type (if it is a valid value of the enum), and `undefined` otherwise. You can set a default value in one line with the `??` operator:

```js flow-check
enum Status {Active, Paused, Off}
declare function getData(): string;
const data: string = getData();

const maybeStatus: Status | void = Status.cast(data);
if (maybeStatus != null) {
  const status: Status = maybeStatus;
  // do stuff with status
}

// Set a default value in one line with the `??` operator:
const statusWithDefault: Status = Status.cast(data) ?? Status.Off;
```

The type of the argument of `cast` depends on the type of enum. If it is a [string enum](./defining-enums.md#toc-string-enums), the type of the argument will be `string`.
If it is a [number enum](./defining-enums.md#toc-number-enums), the type of the argument will be `number`, and so on.
If you wish to cast an `unknown` value, first use a `typeof` refinement:
```js flow-check
enum Status {Active, Paused, Off}
declare const data: unknown;

if (typeof data === 'string') {
  const maybeStatus: Status | void = Status.cast(data);
}
```

`cast` uses `this` (representing the object of enum members), so if you want to pass the function itself as a value, you should use an arrow function. For example:
```js flow-check
enum Status {Active, Paused, Off}
declare const strings: Array<string>;

// WRONG: const statuses: Array<?Status> = strings.map(Status.cast);
const statuses: Array<?Status> = strings.map((input) => Status.cast(input)); // Correct
```

Runtime cost: For [mirrored string enums](./defining-enums.md#toc-string-enums) (e.g `enum E {A, B}`), as the member names are the same as the values, the runtime cost is constant -
equivalent to calling `.hasOwnProperty`. For other enums, a `Map` is created on the first call, and subsequent calls simply call `.has` on the cached map.
Thus the cost is amortized constant.

#### .isValid {#toc-isvalid}
Type: `isValid(input: ?TRepresentationType): boolean`

The `isValid` method is like `cast`, but simply returns a boolean: `true` if the input supplied is a valid enum value, and `false` if it is not.

```js flow-check
enum Status {Active, Paused, Off}
declare function getData(): string;

const data: string = getData();
const isStatus: boolean = Status.isValid(data);
```

`isValid` uses `this` (representing the object of enum members), so if you want to pass the function itself as a value, you should use an arrow function. For example:

```js flow-check
enum Status {Active, Paused, Off}
declare const strings: Array<string>;

// WRONG: const statusStrings = strings.filter(Status.isValid);
const statusStrings = strings.filter((input) => Status.isValid(input)); // Correct
```

Runtime cost: The same as described under `.cast` above.

#### .members {#toc-members}
Type: `members(): Iterator<TEnum>`

The `members` method returns an [iterator](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Iterators_and_generators#iterators) (that is iterable) of all the enum members.

```js flow-check
enum Status {Active, Paused, Off}
declare function getButtonForStatus(status: Status): string;

const buttons = [];
for (const status of Status.members()) {
  buttons.push(getButtonForStatus(status));
}
```
The iteration order is guaranteed to be the same as the order of the members in the declaration.

The enum is not enumerable or iterable itself (e.g. a for-in/for-of loop over the enum will not iterate over its members), you have to use the `.members()` method for that purpose.

You can convert the iterable into an `Array` using: `Array.from(Status.members())`.
You can make use of [`Array.from`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/from)'s second argument to map over the values at
the same time you construct the array: e.g.

```js flow-check
enum Status {Active, Paused, Off}
declare function getButtonForStatus(status: Status): string;

const buttonArray = Array.from(
  Status.members(),
  status => getButtonForStatus(status),
);
```

#### .getName {#toc-getname}
Type: `getName(value: TEnum): string`

The `getName` method maps enum values to the string name of that value's enum member. When using `number`/`boolean`/`symbol` enums,
this can be useful for debugging and for generating internal CRUD UIs. For example:
```js flow-check
enum Status {
  Active = 1,
  Paused = 2,
  Off = 3,
}
declare const status: Status;

console.log(Status.getName(status));
// Will print a string, either "Active", "Paused", or "Off" depending on the value.
```
Runtime cost: The same as described under `.cast` above. A single cached reverse map from enum value to enum name is used for `.cast`, `.isValid`, and `.getName`.
The first call of any of those methods will create this cached map.


### Exhaustively checking enums with a `switch` {#toc-exhaustively-checking-enums-with-a-switch}
When checking an enum value in a `switch` statement, we enforce that you check against all possible enum members, and don’t include redundant cases. A missing case is reported as `[invalid-exhaustive-check]` and the error names the specific member you forgot.
This helps ensure you consider all possibilities when writing code that uses enums. It especially helps with refactoring when adding or removing members,
by pointing out the different places you need to update. If you have [match](../match/index.md) enabled, use `match` expressions and statements instead of `switch` statements.

```js flow-check
enum Status {Active, Paused, Off}
declare const status: Status;

// All members checked:
switch (status) {
  case Status.Active:
    break;
  case Status.Paused:
    break;
  case Status.Off:
    break;
}

// You can use `default` to match all members not checked so far:
switch (status) {
  case Status.Active:
    break;
  default: // When `Status.Paused` or `Status.Off`
    break;
}

// You can check multiple enum members in one switch case:
switch (status) {
  case Status.Active:
  case Status.Paused:
    break;
  case Status.Off:
    break;
}
```

You must match against all of the members of the enum (or supply a `default` case):
```js flow-check
enum Status {
  Active = 1,
  Paused = 2,
  Off = 3,
}
const status: Status = Status.Active;

switch (status) { // Error: you haven't checked 'Status.Off' in the switch
  case Status.Active:
    break;
  case Status.Paused:
    break;
}
```

You can’t repeat cases (as this would be dead code!):
```js flow-check
enum Status {
  Active = 1,
  Paused = 2,
  Off = 3,
}
const status: Status = Status.Active;

switch (status) {
  case Status.Active:
    break;
  case Status.Paused:
    break;
  case Status.Off:
    break;
  case Status.Paused: // Error: you already checked for 'Status.Paused'
    break;
}
```

A `default` case is redundant if you’ve already matched all cases:
```js flow-check
enum Status {
  Active = 1,
  Paused = 2,
  Off = 3,
}
const status: Status = Status.Active;

switch (status) {
  case Status.Active:
    break;
  case Status.Paused:
    break;
  case Status.Off:
    break;
  default: // Error: you've already checked all cases, the 'default' is redundant
    break;
}
// The following is OK because the `default` covers the `Status.Off` case:
switch (status) {
  case Status.Active:
    break;
  case Status.Paused:
    break;
  default:
    break;
}
```
Except if you are switching over an enum with [unknown members](./defining-enums.md#toc-flow-enums-with-unknown-members).

If you nest exhaustively checked switches inside exhaustively checked switches, and are returning from each branch, you must add a `break;` after the nested switch:
```js flow-check
enum Status {Active, Paused, Off}
declare const status: Status;
declare const otherStatus: Status;

function getCode(): number {
  switch (status) {
    case Status.Active:
      return 1;
    case Status.Paused:
      return 2;
    case Status.Off:
      switch (otherStatus) {
        case Status.Active:
          return 1;
        case Status.Paused:
          return 2;
        case Status.Off:
          return 3;
      }
      break;
  }
}
```

Remember, you can add blocks to your switch cases. They are useful if you want to use local variables:

```js flow-check
enum Status {Active, Paused, Off}
declare const status: Status;
declare function f(): number;
declare function g(): number;

switch (status) {
  case Status.Active: {
    const x = f();
    break;
  }
  case Status.Paused: {
    const x = g();
    break;
  }
  case Status.Off: {
    const y = 3;
    break;
  }
}
```
If you didn't add blocks in this example, the two declarations of `const x` would conflict and result in an error.

Enums are not checked exhaustively in `if` statements, or other contexts other than `switch` statements and `match` expressions/statements.

### Exhaustively checking enums with a `match`
All values are exhaustively checked in a [match](../match/index.md), including Flow Enums. You can use a [wildcard](../match/patterns.md#wildcard-patterns) `_` to match all members not checked so far, and check [multiple](../match/patterns.md#or-patterns) enum members in one `match` case using an "or" pattern:

```js flow-check
enum Status {Active, Paused, Off}
declare const status: Status;

// All members checked:
match (status) {
  Status.Active => {}
  Status.Paused => {}
  Status.Off => {}
}

// Using a wildcard `_` to match all members not checked so far:
match (status) {
  Status.Active => {}
  _ => {} // When `Status.Paused` or `Status.Off`
}

// Checking multiple enum members in one `match` case:
match (status) {
  Status.Active | Status.Paused => {}
  Status.Off => {}
}
```

You must match against all of the members of the enum (or supply a wildcard `_` case):
```js flow-check
enum Status {
  Active = 1,
  Paused = 2,
  Off = 3,
}
const status: Status = Status.Active;

match (status) { // Error: you haven't checked 'Status.Off' in the match
  Status.Active => {}
  Status.Paused => {}
}
```

### Exhaustive checking with unknown members {#toc-exhaustive-checking-with-unknown-members}
If your enum has [unknown members](./defining-enums.md#toc-flow-enums-with-unknown-members) (specified with the `...`), e.g.

```js flow-check
enum Status {
  Active,
  Paused,
  Off,
  ...
}
```

Then a `default` is always required when switching over the enum, and a wildcard `_` is always required when using a [`match`](../match/index.md). The `default`/`_` checks for "unknown" members you haven't explicitly listed.

```js flow-check
enum Status {
  Active,
  Paused,
  Off,
  ...
}
declare const status: Status;

switch (status) {
  case Status.Active:
    break;
  case Status.Paused:
    break;
  case Status.Off:
    break;
  default:
    // Checks for members not explicitly listed
}

match (status) {
  Status.Active => {}
  Status.Paused => {}
  Status.Off => {}
  _ => {
    // Checks for members not explicitly listed
  }
}
```

You can use the `require-explicit-enum-switch-cases` [Flow Lint](../linting/flowlint-comments.md) for `switch` or the `require-explicit-enum-checks` lint for `match` to require that all known members are explicitly listed as cases. For example:

```js flow-check
enum Status {
  Active = 1,
  Paused = 2,
  Off = 3,
  ...
}
const status: Status = Status.Active;

// flowlint-next-line require-explicit-enum-switch-cases:error
switch (status) { // Error
  case Status.Active:
    break;
  case Status.Paused:
    break;
  default:
    break;
}

// flowlint-next-line require-explicit-enum-checks:error
match (status) { // Error
  Status.Active => {}
  Status.Paused => {}
  _ => {}
}
```

You can fix if by doing:
```js flow-check
enum Status {
  Active = 1,
  Paused = 2,
  Off = 3,
  ...
}
const status: Status = Status.Active;

// flowlint-next-line require-explicit-enum-switch-cases:error
switch (status) {
  case Status.Active:
    break;
  case Status.Paused:
    break;
  case Status.Off: // Added the missing `Status.Off` case
    break;
  default:
    break;
}

// flowlint-next-line require-explicit-enum-checks:error
match (status) {
  Status.Active => {}
  Status.Paused => {}
  Status.Off => {} // Added the missing `Status.Off` case
  _ => {}
}
```
The `require-explicit-enum-switch-cases`/`require-explicit-enum-checks` lints are not enabled globally, but rather on a per-`switch`/`match` basis when you want the behavior.
With normal enums, for each `switch` statement or `match`, you can either provide a `default`/`_` or not, and thus decide if you want to require each case explicitly listed or not.
Similarly for Flow Enums with unknown members, you can also enable this lint on a per-`switch`/`match` basis.

The `require-explicit-enum-switch-cases` lint works for switches of regular Flow Enum types as well.
It in effect bans the usage of `default` in that `switch` statement, by requiring the explicit listing of all enum members as cases.

### Mapping enums to other values {#toc-mapping-enums-to-other-values}
There are a variety of reasons you may want to map an enum value to another value, e.g. a label, icon, element, and so on.

With previous patterns, it was common to use object literals for this purpose, however with Flow Enums we prefer functions which contain a switch, or better yet [match](../match/index.md) expressions if enabled, as we can exhaustively check these.

Instead of:
```js flow-check
enum Status {Active, Paused, Off}
declare const status: Status;

const STATUS_ICON: {[key: Status]: string} = {
  [Status.Active]: 'green-checkmark',
  [Status.Paused]: 'grey-pause',
  [Status.Off]: 'red-x',
};
const icon = STATUS_ICON[status];
```

Which doesn't actually guarantee that we are mapping each `Status` to some value, use:
```js flow-check
enum Status {
  Active,
  Paused,
  Off,
}
declare const status: Status;

function getStatusIcon(status: Status): string {
  switch (status) {
    case Status.Active:
      return 'green-checkmark';
    case Status.Paused:
      return 'grey-pause';
    case Status.Off:
      return 'red-x';
  }
}
const icon = getStatusIcon(status);
```

Or with `match`:
```js flow-check
enum Status {
  Active,
  Paused,
  Off,
}
declare const status: Status;

const icon = match(status) {
  Status.Active => 'green-checkmark',
  Status.Paused => 'grey-pause',
  Status.Off => 'red-x',
};
```

In the future if you add or remove an enum member, Flow will tell you to update the `switch` or `match` as well so it's always accurate.

If you actually want a dictionary which is not exhaustive, you can use a [`Map`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Map):
```js flow-check
enum Status {Active, Paused, Off}
const counts = new Map<Status, number>([
  [Status.Active, 2],
  [Status.Off, 5],
]);
const activeCount: number | void = counts.get(Status.Active);
```

You can use enum members as computed keys in object literals if you type the object with an [indexer type](../types/objects.md#toc-objects-as-maps), as [explained later on this page](#toc-enum-members-as-distinct-object-keys).


### Enums in a union {#toc-enums-in-a-union}
If your enum value is in a union (e.g. `?Status`), first refine to only the enum type:

```js flow-check
enum Status {Active, Paused, Off}
declare const status: ?Status;

if (status != null) {
  status as Status; // 'status' is refined to 'Status' at this point
  switch (status) {
    case Status.Active: break;
    case Status.Paused: break;
    case Status.Off: break;
  }
}
```
If you want to refine *to* the enum value, you can use `typeof` with the representation type, for example:

```js flow-check
enum Status {Active, Paused, Off}
declare const val: Status | number;

// 'Status' is a string enum
if (typeof val === 'string') {
  val as Status; // 'val' is refined to 'Status' at this point
  switch (val) {
    case Status.Active: break;
    case Status.Paused: break;
    case Status.Off: break;
  }
}
```


### Exporting enums {#toc-exporting-enums}
An enum is a type and a value (like a class is). To export both the type and the value, export it like you would a value:

```js flow-check
export enum Status {}
```
Or, as the default export (note: you must always specify an enum name, `export default enum {}` is not allowed):

```js flow-check
export default enum Status {}
```

Using CommonJS:
```js flow-check
enum Status {}
module.exports = Status;
```

To export **only** its type, but not the value, you can do:

```js flow-check
enum Status {}
export type {Status};
```
Other functions within the file will still have access to the enum implementation.


### Importing enums {#toc-importing-enums}
If you have exported an enum like this:

```js flow-check
// status.js
export default enum Status {
  Active,
  Paused,
  Off,
}
```

You can import it as both a value and a type like this:
```js
import Status from 'status';

const x: Status /* used as type */ = Status.Active /* used as value */;
```

If you only need to use the type, you can import it as a type:
```js
import type Status from 'status';

function printStatus(status: Status) {
  ...
}
```

Using CommonJS:
```js
const Status = require('status');
```


### Abstract enums {#toc-abstract-enums}

You can write code that operates on Flow Enums in a generic way using two types: `Enum<>` and `EnumValue<>`, which accept any Flow Enum and its values.

#### EnumValue&lt;&gt;
`EnumValue<>` accepts any Flow Enum value. Want to narrow it down to enum values with a specific representation type? Just add a type argument:

```js flow-check
enum Status {Active, Paused, Off}

Status.Active as EnumValue<>; // OK
Status.Active as EnumValue<string>; // OK - its a string enum
Status.Active as EnumValue<number>; // ERROR - not a number enum
```

The new `EnumRepresentationTypes` type represents all the valid representation types: `string`, `number`, etc.

Flow Enum values don’t implicitly coerce to their representation type, so we allow [explicit conversion](#toc-casting-to-representation-type)
with casts like `Status.Active as string`. With `EnumValue<>` you might not know the specific representation type,
so we now allow casts using `.valueOf()` directly on the enum value. For example:

```js flow-check
function f(e: EnumValue<>) {
  const x: EnumRepresentationTypes = e.valueOf(); // OK
}
```

#### Enum&lt;&gt;
`Enum<>` accepts any Flow Enum - the enum itself, rather than its value. You can optionally supply a type argument to
restrict it to enums with a certain enum value, which you can pair with the `EnumValue` type above:

```js flow-check
enum Status {Active, Paused, Off}

Status as Enum<>; // OK
Status as Enum<Status>; // OK
Status as Enum<EnumValue<string>>; // OK
```

With the `Enum<>` type you can use all the Flow Enum [methods](#toc-methods) like `.cast` and `.members`.
This lets you craft code that generically handles Flow Enums, for example creating a full React selector component from only a Flow Enum:

```js flow-check
import * as React from 'react';
import {useCallback, useMemo, useState} from 'react';

component Selector<
  TEnumValue extends EnumValue<string>,
>(
  items: Enum<TEnumValue>,
  callback: (TEnumValue) => void,
) {
  // Typing the `useState` selector with the enum value generic
  const [value, setValue] = useState<?TEnumValue>(null);

  const handleChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    // Using the `.cast` method
    const value = items.cast(e.target.value);
    if (value == null) { throw new Error("Invalid value"); }
    setValue(value);
    callback(value);
  }, [items, callback]);

  return <ul>
    {Array.from(items.members(), item => // Using the `.members()` method
      <li>
        <label>
          {// Using the `.getName` method:
            items.getName(item)
          }:
          <input
            type="radio"
            // Casting to the representation type using `.valueOf()`
            value={item.valueOf()}
            checked={item === value}
            onChange={handleChange} />
         </label>
      </li>
    )}
  </ul>;
}

// Usage:
enum Status {
  Active,
  Paused,
  Off,
}
declare function doStuff(value: Status): void;

<Selector items={Status} callback={doStuff} />;
```

### When to not use enums {#toc-when-to-not-use-enums}
Enums are designed to cover many use cases and exhibit certain benefits. The design makes a variety of trade-offs to make this happen, and in certain situations,
these trade-offs might not be right for you. In these cases, you can continue to use existing patterns to satisfy your use cases.


#### Enum members as distinct object keys {#toc-enum-members-as-distinct-object-keys}
You can use enum members as computed property keys in object literals. The resulting object is typed with an indexer type `{[key: E]: T}`:

```js flow-check
enum Status {
  Active,
  Paused,
  Off,
}

const o = {
  [Status.Active]: 0,
};

o as {[key: Status]: number}; // OK
o[Status.Active] as number; // OK
o[Status.Paused] = 1; // OK
```

However, you can't use enum members as **distinct** object keys that each map to a different value type. With legacy frozen-object patterns, each key has a distinct literal type, so Flow can track which key maps to which value:

```js flow-check
const LegacyStatus = Object.freeze({
  Active: 'Active',
  Paused: 'Paused',
  Off: 'Off',
});
const o = {
  [LegacyStatus.Active]: "hi",
  [LegacyStatus.Off]: 1,
};
const x: string = o[LegacyStatus.Active]; // OK
const y: number = o[LegacyStatus.Off]; // OK
const z: boolean = o[LegacyStatus.Active]; // Error - as expected
```

This works because `LegacyStatus.Active` has the literal type `'Active'` and `LegacyStatus.Off` has the literal type `'Off'`. With Flow Enums, all members share the same enum type, so Flow can't distinguish between keys — the object can only have a single value type for all enum keys (via the indexer `{[key: E]: T}`).

If you need to map each enum member to a different value type, use a [function with an exhaustively-checked switch instead](#toc-mapping-enums-to-other-values).


#### Disjoint object unions {#toc-disjoint-object-unions}
A defining feature of enums is that unlike unions, each enum member does not form its own separate type. Every member has the same type, the enum type.
This allows enum usage to be analyzed by Flow in a consistently fast way, however it means that in certain situations which require separate types, we can’t use enums.
Consider the following union, following the [disjoint object union](../types/unions.md#toc-disjoint-object-unions) pattern:

```js flow-check
type Action =
  | {type: 'Upload', data: string}
  | {type: 'Delete', id: number};
```
Each object type in the union has a single common field (`type`) which is used to distinguish which object type we are dealing with.

We can’t use enum types for this field, because for this mechanism to work, the type of that field must be different in each member of the union,
but enum members all have the same type.

In the future, we might add the ability for enums to encapsulate additional data besides a key and a primitive value - this would allow us to replace disjoint object unions.


#### Guaranteed inlining {#toc-guaranteed-inlining}
Flow Enums are designed to allow for inlining (e.g. [member values must be literals](./defining-enums.md#toc-literal-member-values),
[enums are frozen](./defining-enums.md#toc-fixed-at-declaration)), however the inlining itself needs to be part of the build system (whatever you use) rather than Flow itself.

While enum member access (e.g. `Status.Active`) can be inlined (other than [symbol enums](./defining-enums.md#toc-symbol-enums) which cannot be inlined due to the nature of symbols),
usage of its methods (e.g. `Status.cast(x)`) cannot be inlined.

## See Also {#toc-see-also}

- [Unions](../types/unions.md) — an alternative to enums for some use cases
- [Match](../match/index.md) — pattern matching works well with enums for exhaustive checking
