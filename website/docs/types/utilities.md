---
title: Utility Types
slug: /types/utilities
description: "Reference for Flow's built-in utility types: keyof, Partial, Required, Readonly, Pick, Omit, Exclude, Extract, and more."
---

import {SinceVersion} from '../../components/VersionTags';

Flow provides a set of utility types to operate on other types to create new types.

:::info TypeScript comparison
Most utility types align directly with TypeScript: `Readonly`, `Pick`, `Omit`, `Record`, `Partial`, `Required`, `Exclude`, `Extract`, `NonNullable`, `Parameters`, `ReturnType`, `ConstructorParameters`, `InstanceType`, `Awaited`, `ThisParameterType`, `OmitThisParameter`, `NoInfer`, `Uppercase`, `Lowercase`, `Capitalize`, `Uncapitalize`, and `keyof`. See [Concepts that transfer cleanly from TypeScript](../flow-vs-typescript.md#toc-transfer-cleanly) for more.
:::

## `keyof T` {#toc-keys}

You can extract the type of the keys from an [object type](./objects.md). Typically this will be a [union](./unions.md) of [string literal](./literals.md) types:

```js flow-check
const countries = {
  US: "United States",
  IT: "Italy",
  FR: "France"
};

type Country = keyof typeof countries;

const italy: Country = 'IT'; // Works
const nope: Country = 'nope'; // Error!
```

In the example above, the type of `Country` is equivalent to `type Country = 'US' | 'IT' | 'FR'`, but Flow was able to extract it from the keys of `countries`.

`keyof` also includes [symbol keys](./objects.md#toc-symbol-keyed-properties), each as its own unique symbol type. In contrast, `Object.keys` and `Object.entries` omit symbol keys during runtime enumeration, matching JavaScript's behavior:

```js flow-check
const id = Symbol('id');
const tagged = {[id]: 1, name: 'tagged'};

type Keys = keyof typeof tagged; // 'name' | typeof id
id as Keys; // Works
'name' as Keys; // Works
Symbol() as Keys; // Error!

Object.keys(tagged) as Array<'name'>; // Works: no symbol keys at runtime
```

If you want to create an enum type, [Flow Enums](../enums/index.md) might be a better fit for your use-case.

## `Values<T>` {#toc-values}

`Values<T>` represents the union type of all the value types of the properties in an [object type](./objects.md),
or the elements of an [array](./arrays.md) or [tuple](./tuples.md) type:

```js flow-check
type Props = {
  name: string,
  age: number,
};

// The following two types are equivalent:
type PropValues = string | number;
type PropValues2 = Values<Props>;

const name: PropValues2 = 'Jon';  // Works
const age: PropValues2 = 42;  // Works
const fn: PropValues2 = true; // Error!
```

For arrays and tuples:
```js flow-check
type Arr = Array<string>;
type Str = Values<Arr>; //=> string
's' as Str; // OK

type Tup = [1, 2];
type Num = Values<Tup>; //=> 1 | 2
1 as Num; // OK
```

Note that using `Values` on the [`typeof`](./typeof.md) an object or array literal will result in a type more general than you might expect:

```js flow-check
const obj = {
  foo: 1,
  bar: 2,
};

function acceptsValues(x: Values<typeof obj>) { /* ... */ }

acceptsValues(1); // Works
acceptsValues(3); // Works, because the type was interpreted as `number`.
```

This behavior changes if you use `Object.freeze` on the object literal expression:

```js flow-check
const obj = Object.freeze({
  foo: 1,
  bar: 2,
});

function acceptsValues(x: Values<typeof obj>) { /* ... */ }

acceptsValues(1); // Works
acceptsValues(3); // Error! Because the type was interpreted as `1 | 2`.
```

If you want to create an enum type, [Flow Enums](../enums/index.md) might be a better fit for your use-case.

:::info TypeScript comparison
TypeScript has no `Values<T>` utility; the equivalent is the indexed access `T[keyof T]`.
:::

## `Readonly<T>` {#toc-readonly}

`Readonly<T>` is a type that represents the read-only version of a given [object type](./objects.md)
or [tuple type](./tuples.md) `T`.
A read-only object type is an object type whose keys are all [read-only](./objects.md#read-only-object-properties).
Similarly, a read-only tuple is one where each element is [read-only](./tuples.md#variance-annotations-and-read-only-tuples).

This means that the following are equivalent:
```js flow-check
type ReadOnlyObj = {
  readonly key: number,  // read-only field, marked by the `readonly` keyword
};
type ReadOnlyTuple = [readonly foo: number];
```
&rarr;
```js flow-check
type ReadOnlyObj = Readonly<{
  key: number,
}>;
type ReadOnlyTuple = Readonly<[number]>;
```

This is useful when you need to use a read-only version of an object type you've already defined, without manually having to re-define and annotate each key as read-only. For example:

```js flow-check
type Props = {
  name: string,
  age: number,
};

type ReadOnlyProps = Readonly<Props>;

function render(props: ReadOnlyProps) {
  const {name, age} = props;  // Works
  props.age = 42;             // Error!
}
```

Additionally, other utility types, such as [spread](./objects.md#object-type-spread), may strip any read/write annotations, so `Readonly<T>` is a handy way to quickly make the object read-only again after operating on it:

```js flow-check
type Obj = {
  readonly key: number,
};

type MappedObj = Readonly<{...Obj, foo: string}> // Still read-only
```

The `Readonly` utility works on object and tuple types.
If you want to make other types read-only, you can use one of the following:
- `Array<T>` -> [`ReadonlyArray<T>`](./arrays.md#toc-readonlyarray)
- `Set<T>` -> `ReadonlySet<T>`
- `Map<K, V>` -> `ReadonlyMap<K, V>`
- `WeakSet<T>` -> `ReadonlyWeakSet<T>`
- `WeakMap<K, V>` -> `ReadonlyWeakMap<K, V>`

## `Partial<T>` {#toc-partial}

This utility converts all of an object or interface's named fields to be optional,
while maintaining all the object's other properties (e.g. exactness, variance).

It also converts all of a tuple type's elements to be [optional](./tuples.md#optional-tuple-elements).

Example for objects:
```js flow-check
type Person = {
  name: string,
  age: number,
};
type PartialPerson = Partial<Person>;
// Above equivalent to `{name?: string, age?: number}`

const a: PartialPerson = {}; // OK
const b: PartialPerson = {name: 'George'}; // OK
const c: PartialPerson = {name: 'George', age: 123}; // OK

c as Person; // ERROR: `PartialPerson` is not a `Person`
```

For tuples:
```js flow-check
type AllRequired = [number, string];
[] as Partial<AllRequired>; // OK: like `[a?: number, b?: string]` now
```

An object or tuple of type `T` cannot be supplied to `Partial<T>`, due to mutability. You can resolve this by making the object [read-only](#toc-readonly):

```js flow-check
type Person = {
  name: string,
  age: number,
};

const person: Person = {name: 'George', age: 123};

function noPerson(o: Partial<Person>) {
  // Can mutate:
  o.name = undefined;
}
noPerson(person); // Error!

function okPerson(o: Readonly<Partial<Person>>) {
  // Can't mutate - it's read-only!
}
okPerson(person); // Works
```

## `Required<T>` {#toc-required}

The `Required` utility type is the opposite of [`Partial`](#toc-partial):
it converts all of an object or interface’s optional fields to be required.

It also converts all of a tuple type's elements to be [required](./tuples.md#optional-tuple-elements).

Example for objects:
```js flow-check
type PartialPerson = {
  name?: string,
  age?: number,
};
type Person = Required<PartialPerson>;
// Above equivalent to `{name: string, age: number}`

const a: Person = {name: 'George', age: 123}; // OK
const b: Person = {age: 123}; // ERROR: missing `name` property
```

For tuples:
```js flow-check
type AllOptional = [a?: number, b?: string];
[] as Required<AllOptional>; // ERROR: like `[a: number, b: string]` now
```

## `Pick<O, Keys>` {#toc-pick}

This utility type allows you to generate an object type using a subset of the fields from
another object type.

```js flow-check
type O = {foo: number, bar: string, baz: boolean};
type FooAndBar = Pick<O, 'foo' | 'bar'>;

declare const fooAndBar: FooAndBar;
fooAndBar.baz; // error: baz is missing
fooAndBar.foo as number; // ok
fooAndBar.bar as string; // ok
```

## `Omit<O, Keys>` {#toc-omit}

This utility type allows you to generate an object type by omitting the specified fields from
another object type.
```js flow-check
type O = {foo: number, bar: string, baz: boolean};
type JustBaz= Omit<O, 'foo' | 'bar'>;

declare const justBaz: JustBaz;
justBaz.baz as boolean; // ok
justBaz.foo; // error: missing foo
justBaz.bar; // error: missing bar
```

`Pick`, `Omit`, and `Partial` preserve the distinct identities of [symbol keys](./objects.md#toc-symbol-keyed-properties). To select or remove a specific symbol-keyed property, use `typeof key`:

```js flow-check
const id = Symbol('id');
type Tagged = {[id]: number, name: string};

declare const withoutId: Omit<Tagged, typeof id>;
withoutId.name as string; // Works
withoutId[id]; // Error! `id` was omitted

declare const onlyId: Pick<Tagged, typeof id>;
onlyId[id] as number; // Works

declare const partial: Partial<Tagged>;
partial[id] as number | void; // Works
```

## `Record<Keys, Type>` {#toc-record}

This utility type allows you to generate an object type from a union of keys with the given
`Type` for each field.
```js flow-check
type NumberRecord = Record<'foo' | 'bar', number>;
declare const numberRecord: NumberRecord;
numberRecord.foo as number; // ok
numberRecord.bar as number; // ok
numberRecord.baz; // error
```

Unique symbol keys also create required properties:
```js flow-check
const width = Symbol('width');
const height = Symbol('height');

type Size = Record<typeof width | typeof height, number>;
const size: Size = {[width]: 1}; // Error! Missing the `height` property
```

Note that `Record` is different than using an indexer:
```js flow-check
type NumberRecord = Record<'foo' | 'bar', number>;
type IndexedObject = {['foo' | 'bar']: number};

// Record uses explicit fields, which means they are all required
const rec: NumberRecord = {}; // error
// Indexers do not have this same requirement
const idx: IndexedObject = {}; // no error
```

## `Exclude<T, U>` {#toc-exclude}

This utility type excludes all subtypes of `U` from `T`.

```js flow-check
type T = Exclude<1 | 2 | 3 | 4, 1 | 3>; // evaluates to 2 | 4
1 as T; // error
2 as T; // ok
3 as T; // error
4 as T; // ok
```

## `Extract<T, U>` {#toc-extract}

This utility type retains only subtypes of `U` from `T`.

```js flow-check
declare class Car {}
declare class Animal {}
declare class Dog extends Animal {}
declare class Cat extends Animal {}
type T = Extract<Car | Dog | Cat, Animal>; // evaluates to Dog | Cat
new Car() as T; // error
new Dog() as T; // ok
new Cat() as T; // ok
```

## `ReturnType<F>` {#toc-return-type}

This utility type extracts the return type from a given function type.

```js flow-check
declare function f(s: string, n: number): boolean;
type Bool = ReturnType<typeof f>;
true as Bool;
1 as Bool; // Error: number is not boolean
```

## `Parameters<F>` {#toc-parameters}

This utility type extracts the parameter types from a given function type into a [tuple type](./tuples.md).

```js flow-check
declare function f(s: string, n: number): boolean;
type Tuple = Parameters<typeof f>; // Evaluates to [string, number]
's' as Tuple[0];
1 as Tuple[1];
false as Tuple[2]; // Error: tuple type only has two elements
```

## `ConstructorParameters<C>` <SinceVersion version="0.317" /> {#toc-constructor-parameters}

This utility type extracts the parameter types of a class's constructor into a [tuple type](./tuples.md). Pass the class itself via [`typeof`](./typeof.md):

```js flow-check
class Point {
  x: number;
  y: number;
  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
  }
}

type PointArgs = ConstructorParameters<typeof Point>; // Evaluates to [number, number]
[1, 2] as PointArgs; // OK
'x' as PointArgs[0]; // Error: string is not number
```

[`Constructor types`](./functions.md#toc-constructor-types) also work with this utility:

```js flow-check
type ItemConstructor = new (name: string) => interface {name: string};
type ItemArgs = ConstructorParameters<ItemConstructor>; // [string]
type Item = InstanceType<ItemConstructor>; // interface {name: string}

['book'] as ItemArgs; // OK
[123] as ItemArgs; // Error! The constructor parameter is a string
declare const item: Item;
item.name as string; // OK
```

For overloaded constructors, Flow currently extracts parameters from the **first** overload, whereas TypeScript uses the last overload. This difference affects parameter extraction, not which overloads you can call with `new`.

A class with no explicit constructor uses its synthesized default constructor, which takes no arguments:

```js flow-check
class Empty {}
type EmptyArgs = ConstructorParameters<typeof Empty>; // Evaluates to []
[] as EmptyArgs; // OK
[1] as EmptyArgs; // Error: default constructor takes no arguments
```

A class without its own constructor inherits the parameters of its parent's constructor:

```js flow-check
class Animal {
  name: string;
  constructor(name: string) {
    this.name = name;
  }
}
class Dog extends Animal {} // no explicit constructor

type DogArgs = ConstructorParameters<typeof Dog>; // Inherited [string]
['Rex'] as DogArgs; // OK
[1] as DogArgs; // Error: number is not string
```

## `InstanceType<C>` <SinceVersion version="0.317" /> {#toc-instance-type}

This utility type extracts the instance type produced by a class's constructor. Pass the class itself via [`typeof`](./typeof.md). You can also pass a [constructor type](./functions.md#toc-constructor-types) to extract its result type:

```js flow-check
class User {
  name: string;
  constructor(name: string) {
    this.name = name;
  }
}

type UserInstance = InstanceType<typeof User>; // Evaluates to User
declare const u: UserInstance;
u.name as string; // OK
u as User; // OK
```

This is useful when you have the type of a class (the static side, e.g. from a [generic](./generics.md) bound or a [`Class<T>`](#toc-class)) and need the type of the values it constructs.

For a [generic](./generics.md) class with no inference context, the class's type parameters resolve to their bounds (defaulting to [`unknown`](./unknown.md)):

```js flow-check
class Box<T> {
  value: T;
  constructor(value: T) {
    this.value = value;
  }
}

type BoxInstance = InstanceType<typeof Box>; // Box<unknown>
declare const b: BoxInstance;
b.value as unknown; // OK
```

## `ThisParameterType<F>` {#toc-this-parameter-type}

This utility type extracts the type of the `this` parameter of a given function type.

```js flow-check
type T = ThisParameterType<(this: number, bar: string) => void>; // Evaluates to number
'1' as T; // error
2 as T; // ok
```

## `OmitThisParameter<F>` {#toc-omit-this-parameter-type}

This utility type removes the `this` parameter from a given function type.

```js flow-check
type HasThisParamFun = (this: number, bar: string) => void;
type NoThisParamFun = OmitThisParameter<HasThisParamFun> // Evaluates to (bar: string) => void
declare const hasThisParam: HasThisParamFun;
declare const noThisParam: NoThisParamFun;

hasThisParam(''); // error: global object is not number
noThisParam(''); // ok: no this type requirement
```

## `Awaited<T>` {#toc-awaited}

`Awaited<T>` unwraps a `Promise` type to extract the type of its resolved value. If `T` is not a `Promise`, it returns `T` unchanged.

```js flow-check
type Response = Awaited<Promise<string>>; // Evaluates to string
'hello' as Response; // OK
42 as Response; // Error: number is not string
```

This is useful when you need to refer to the resolved type of a `Promise` without manually unwrapping it. If `T` is not a `Promise`, `Awaited<T>` simply evaluates to `T`:

```js flow-check
type A = Awaited<Promise<number>>; // number
type B = Awaited<string>; // string (non-Promise types are returned as-is)

42 as A; // OK
'hello' as B; // OK
'hello' as A; // Error: string is not number
42 as B; // Error: number is not string
```

You can combine `Awaited` with other utility types like [`ReturnType`](#toc-return-type) to extract the resolved type from an async function's return type:

```js flow-check
async function fetchUser(): Promise<{name: string}> {
  return {name: 'George'};
}

type User = Awaited<ReturnType<typeof fetchUser>>;

const user: User = {name: 'George'}; // OK
const bad: User = 'not a user'; // Error
```

## `NonNullable<T>` {#toc-nonmaybe}

`NonNullable<T>` converts a type `T` to a non-[maybe type](./maybe.md).
In other words, the values of `NonNullable<T>` are the values of `T` except for `null` and `undefined`.

```js flow-check
type MaybeName = ?string;
type Name = NonNullable<MaybeName>;

'Gabriel' as MaybeName; // Works
null as MaybeName; // Works
'Gabriel' as Name; // Works
null as Name; // Error! `null` can't be annotated as Name because Name is not a maybe type
```

## `NoInfer<T>` {#toc-noinfer}

`NoInfer<T>` prevents a type parameter from being inferred from the wrapped position.
When Flow infers a type parameter `T`, it normally considers every position where `T` appears.
Wrapping a position in `NoInfer` excludes it from inference, so `T` is determined only by the remaining (unwrapped) positions.
The resulting type is still `T` — `NoInfer` only affects inference, not the type itself.

This is useful when a generic function has multiple parameters that share a type parameter, but you want only some of them to drive inference. Without `NoInfer`, Flow may widen the type parameter to accommodate all arguments, allowing mismatches to go undetected.

Consider a function where both parameters use the same type parameter `T`:

```js flow-check
declare function assertEqual<T>(actual: T, expected: T): void;

assertEqual('hello', 42); // No error: T is inferred as string | number
```

Because both arguments contribute to inference, Flow widens `T` to `string | number`, and the call succeeds even though the two arguments have different types.

By wrapping the `expected` parameter with `NoInfer`, you can ensure `T` is inferred only from `actual`:

```js flow-check
declare function assertEqual<T>(actual: T, expected: NoInfer<T>): void;

assertEqual('hello', 'world'); // OK
assertEqual('hello', 42); // Error!
```

Now `T` is inferred as `string` from the first argument alone, and `42` fails to match.

**Nested types**

`NoInfer` also works when `T` appears inside a more complex type. Any occurrence of the type parameter within `NoInfer` is excluded from inference:

```js flow-check
declare function createSignal<T>(
  initialValue: T,
  defaultValue: NoInfer<T>,
): void;

createSignal('hello', 'default'); // OK
createSignal('hello', 42); // Error!
```

**Callbacks**

You can use `NoInfer` in callback parameter positions to prevent the callback from influencing the type parameter while still receiving the inferred type:

```js flow-check
declare function filter<T>(
  items: Array<T>,
  predicate: (item: NoInfer<T>) => boolean,
): Array<T>;

const numbers: Array<number> = [1, 2, 3];
filter(numbers, (item) => item > 1); // OK
```

Here `T` is inferred as `number` from the `items` argument. The `predicate` callback receives `item` typed as `number` but does not participate in inferring `T`.

## `$KeyMirror<O>` {#toc-keymirror}

`$KeyMirror<Obj>` maps each property of an object
to the type of the property key. For example:
```js flow-check
const obj = {
  a: true,
  b: 'foo'
};

declare function run<O extends {...}>(o: O): $KeyMirror<O>;

// newObj is of type {a: 'a', b: 'b'}
const newObj = run(obj);

newObj.a as 'a'; // Works
newObj.b as 'a'; // Error! String 'b' is incompatible with 'a'
```


## `Class<T>` {#toc-class}

Given a type `T` representing instances of a class `C`, the type `Class<T>` is the type of the class `C`.
For example:

```js flow-check
class Store {}
class ExtendedStore extends Store {}
class Model {}

function makeStore(storeClass: Class<Store>) {
  return new storeClass();
}

makeStore(Store) as Store;
makeStore(ExtendedStore) as Store;
makeStore(Model) as Model; // Error!
```

For classes that take type parameters, you must also provide the parameter. For example:

```js flow-check
class ParamStore<T> {
  constructor(data: T) {}
}

function makeParamStore<T>(storeClass: Class<ParamStore<T>>, data: T): ParamStore<T> {
  return new storeClass(data);
}
makeParamStore(ParamStore, 1) as ParamStore<number>;
makeParamStore(ParamStore, 1) as ParamStore<boolean>; // Error!
```

:::info TypeScript comparison
TypeScript has no native equivalent of `Class<T>`; the usual encodings are `new (...args: any[]) => T`, or `typeof C` for a specific class.
:::

## `$Exports<T>` {#toc-exports}

The following are functionally equivalent

```js
import typeof * as T from 'my-module';
```

```js
type T = $Exports<'my-module'>;
```

The advantage of the `$Exports` syntax is that you can `export` the type on the same line
```js
export type T = $Exports<'my-module'>;
```

where as you would otherwise need to export an alias in the `import typeof` case
```js
import typeof * as T from 'my-module';
export type MyModuleType = T;
```

## `StringPrefix` and `StringSuffix` {#toc-string-prefix-suffix}

Prefer [template literal types](./template-literal-types.md) for new code. They express prefix and suffix patterns directly:

```js flow-check
// Strings with a data- prefix.
type DataProp = `data-${string}`;
'data-name' as DataProp; // OK
'other-name' as DataProp; // ERROR: wrong prefix

// Strings with an ! suffix.
type Exclaim = `${string}!`;
'yay!' as Exclaim; // OK
'yay' as Exclaim; // ERROR: missing suffix
```

The `StringPrefix` and `StringSuffix` types represent strings with the specified prefix or suffix, respectively.
Their first type argument must be a string literal type, representing the prefix or suffix.

You could use `StringPrefix` to define a type that accepts strings which start with `data-`:

```js flow-check
type DataProp = StringPrefix<'data-'>;
'data-foo' as DataProp; // OK
'data-bar' as DataProp; // OK
'random string' as DataProp; // ERROR
```

You could use `StringSuffix` to define a type that accepts strings which end with `!`:

```js flow-check
type Exclaim = StringSuffix<'!'>;
'yay!' as Exclaim; // OK
'woo!' as Exclaim; // OK
'random string' as Exclaim; // ERROR
```

You can combine these with [intersection types](./intersections.md):

```js flow-check
type CSSVar = StringPrefix<'var(--'> & StringSuffix<')'>;
'var(--color)' as CSSVar; // OK
'random string' as CSSVar; // ERROR
```

Both utilities accept an optional second type argument, which is the type of the remainder:

```js flow-check
type Price = StringPrefix<'$', '1' | '2'>;
'$1' as Price; // OK
'$2' as Price; // OK
'$999' as Price; // ERROR
```

When not specified, the type of the remainder is just `string`.

## `Uppercase<T>`, `Lowercase<T>`, `Capitalize<T>`, `Uncapitalize<T>` <SinceVersion version="0.316" /> {#toc-string-casing}

These four utility types transform the casing of a string type. You can also [combine them with template literal types](./template-literal-types.md#toc-string-casing):

- `Uppercase<T>` converts every character to upper case.
- `Lowercase<T>` converts every character to lower case.
- `Capitalize<T>` converts the first character to upper case, leaving the rest unchanged.
- `Uncapitalize<T>` converts the first character to lower case, leaving the rest unchanged.

The argument must be a [string](./primitives.md#toc-strings) type. Applied to a [string literal](./literals.md) type, the result is the transformed literal:

```js flow-check
type Greeting = Uppercase<'hello'>; // 'HELLO'
'HELLO' as Greeting; // OK
'hello' as Greeting; // ERROR
```

```js flow-check
type Tag = Lowercase<'WARNING'>; // 'warning'
'warning' as Tag; // OK
'WARNING' as Tag; // ERROR
```

`Capitalize` and `Uncapitalize` only change the first character:

```js flow-check
type Name = Capitalize<'flow'>; // 'Flow'
'Flow' as Name; // OK
'flow' as Name; // ERROR
'FLOW' as Name; // ERROR
```

```js flow-check
type Field = Uncapitalize<'FullName'>; // 'fullName'
'fullName' as Field; // OK
'FullName' as Field; // ERROR
'fullname' as Field; // ERROR (the tail is left untouched)
```

They distribute over [union](./unions.md) types, transforming each member independently:

```js flow-check
type Direction = Uppercase<'north' | 'south'>; // 'NORTH' | 'SOUTH'
'NORTH' as Direction; // OK
'SOUTH' as Direction; // OK
'south' as Direction; // ERROR
```

You can nest them to compose transformations:

```js flow-check
type RoundTrip = Uncapitalize<Capitalize<'foo'>>;
'foo' as RoundTrip; // OK

type Nested = Uppercase<Lowercase<'AbC'>>;
'ABC' as Nested; // OK
```

Applied to the general `string` type, the result is still a `string`, but it represents only the strings that are already in the transformed form. A concrete literal must already be in that form to be a member:

```js flow-check
type Loud = Uppercase<string>;
declare const x: Loud;
x as string; // OK
'ANYTHING' as Loud; // OK
'anything' as Loud; // ERROR (not already upper case)
```

Use a casing utility inside a template to construct names from a union:

```js flow-check
// Construct handler names from each event.
type Event = 'click' | 'change';
type EventName = `on${Capitalize<Event>}`;

'onClick' as EventName; // OK
'onChange' as EventName; // OK
'onclick' as EventName; // ERROR: wrong capitalization
```

## `$Exact<T>` (Discouraged) {#toc-exact}

:::warning
Avoid `$Exact<T>`. Start with an exact object type and derive inexact variants from it using [object type spread](./objects.md#object-type-spread).
:::

`$Exact` makes an [inexact object type](./objects.md#exact-and-inexact-object-types) exact:

```js flow-check
type InexactUser = {name: string, ...};
type ExactUser = $Exact<InexactUser>;

const user = {name: 'John Wilkes Booth'};
// These will both be satisfied because they are equivalent:
const a: ExactUser = user;
const b: {name: string} = user;
```

Prefer the inverse: define the exact type and spread it into the inexact one.

```js flow-check
type ExactUser = {name: string};
type InexactUser = {...ExactUser, ...};

const user = {name: 'John Wilkes Booth'};
const a: ExactUser = user;
```

## See Also {#toc-see-also}

- [Mapped Types](./mapped-types.md) — a general mechanism for transforming object types (generalizes many utility types)
- [Conditional Types](./conditional.md) — type-level conditionals for more complex transformations
- [Generics](./generics.md) — most utility types are generic; understanding generics helps use them effectively
- [Indexed Access Types](./indexed-access.md) — extracting property types from objects and tuples
- [Modernizing Legacy Flow Syntax](../modernizing-legacy-syntax.md) — migrating legacy `$`-prefixed utilities such as `$ObjMap`, `$PropertyType`, and `$NonMaybeType` to their modern equivalents
