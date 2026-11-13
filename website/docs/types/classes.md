---
title: Classes
slug: /types/classes
description: "How to type classes in Flow, including class fields, methods, generics, and the relationship between class types and interface types."
---

import {SinceVersion} from '../../components/VersionTags';

JavaScript [classes](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Classes)
in Flow operate both as a value and a type. You can use the name of the class as the type of its instances:

```js flow-check
class MyClass {
  // ...
}

const myInstance: MyClass = new MyClass(); // Works!
```

This is because classes in Flow are [nominally typed](../lang/nominal-structural.md).

This means two classes with identical shapes are not compatible:

```js flow-check
class A {
  x: number;
}
class B {
  x: number;
}
const foo: B = new A(); // Error!
const bar: A = new B(); // Error!
```

You also cannot use an [object type](./objects.md) to describe an instance of a class — Flow reports this as `[class-object-subtyping]` ("Class instances are not subtypes of object types; consider rewriting object type as an interface"):

```js flow-check
class MyClass {
  x: number;
}
const foo: {x: number, ...} = new MyClass(); // Error!
```

You can use [interfaces](./interfaces.md) to accomplish this instead — interfaces accept both plain objects and class instances:

```js flow-check
class A {
  x: number;
}
class B {
  x: number;
}

interface WithXNum {
  x: number;
}

const foo: WithXNum = new A(); // Works!
const bar: WithXNum = new B(); // Works!

const n: number = foo.x; // Works!
```

:::info TypeScript comparison
TypeScript types classes structurally, while Flow types them [nominally](../flow-vs-typescript.md#toc-classes-nominal) — two distinct classes with the same shape are different types. Flow also [preserves an instance method's receiver requirement](../flow-vs-typescript.md#toc-method-extraction) when the method is extracted, while TypeScript drops it. And several TS-only [class syntax extensions](../flow-vs-typescript.md#toc-class-extensions) — parameter properties, access modifiers — are not adopted in Flow, write the equivalent JS instead.
:::

## When to use this {#toc-when-to-use}

Use classes when you need methods, inheritance, or [nominal typing](../lang/nominal-structural.md) — two classes with the same shape are distinct types. When you only need to describe data shape, use [object types](./objects.md). When you need structural compatibility across classes, use [interfaces](./interfaces.md).

## Class Syntax {#toc-class-syntax}

Classes in Flow are just like normal JavaScript classes, but with added types.

### Class Methods {#toc-class-methods}

Just like in [functions](./functions.md), class methods can have annotations for both parameters
(input) and returns (output):

```js flow-check
class MyClass {
  method(value: string): number {
    return 0;
  }
}
```

Also just like regular functions, class methods may have `this` annotations as well.
For instance methods without one, Flow infers the class instance type instead of `unknown`. For static methods,
Flow infers the class type when the method body uses `this`; otherwise it uses `unknown`, which lets the static
method be extracted and called without a receiver. When an explicit `this` parameter is provided, it must be a
[supertype](../lang/subtypes.md) of the class instance type (or class type for static methods).

```js flow-check
class MyClass {
  method(this: interface {x: string}) { /* ... */ } // Error!
}
```

The `this` type can also appear in a method signature — typically as a return type (`add(x: number): this`) for fluent APIs that preserve the subclass type through chained calls. Flow allows `this` only in covariant positions (return types and `readonly` fields); see [The `this` type is restricted to covariant positions](../lang/variance.md#toc-this-covariant) for the input/mutable-field rules and rewrites.

Methods are considered [read-only](../lang/variance.md):

```js flow-check
class MyClass {
  method() {}
}

const a = new MyClass();
a.method = function() {}; // Error!
```

Flow supports [private methods](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Classes/Private_class_fields),
a feature of ES2022. Private methods start with a hash symbol `#`:

```js flow-check
class MyClass {
  #internalMethod() {
    return 1;
  }
  publicApi() {
    return this.#internalMethod();
  }
}

const a = new MyClass();
a.#internalMethod(); // Error!
a.publicApi(); // Works!
```

Flow requires return type annotations on methods in most cases.
This is because it is common to reference `this` inside of a method, and `this` is typed as the instance of the class -
but to know the type of the class we need to know the return type of its methods!

```js flow-check
class MyClass {
  foo() { // Error!
    return this.bar();
  }
  bar() { // Error!
    return 1;
  }
}
```
```js flow-check
class MyClassFixed {
  foo(): number { // Works!
    return this.bar();
  }
  bar(): number { // Works!
    return 1;
  }
}
```

### Class Fields (Properties) {#toc-class-fields-properties}

Whenever you want to use a class field in Flow you must first give it an
annotation:

```js flow-check
class MyClass {
  method() {
    this.prop = 42; // Error!
  }
}
```

Fields are annotated within the body of the class with the field name followed
by a colon `:` and the type:

```js flow-check
class MyClass {
  prop: number;
  method() {
    this.prop = 42;
  }
}
```

Fields added outside of the class definition need to be annotated within the body
of the class:

```js flow-check
function func(x: number): number {
  return x + 1;
}

class MyClass {
  static constant: number;
  static helper: (x: number) => number;
  prop: (x: number) => number;
}
MyClass.helper = func
MyClass.constant = 42
MyClass.prototype.prop = func
```

Flow also supports using the [class properties syntax](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Classes#field_declarations):

```js flow-check
class MyClass {
  prop = 42;
}
```

When using this syntax, you are not required to give it a type annotation. But
you still can if you need to:

```js flow-check
class MyClass {
  prop: number = 42;
}
```

You can mark a class field as read-only (or write-only) using [variance](../lang/variance.md) annotations.
These can only be written to in the constructor:

```js flow-check
class MyClass {
  readonly prop: number;

  constructor() {
    this.prop = 1; // Works!
  }

  method() {
    this.prop = 1; // Error!
  }
}

const a = new MyClass();
const n: number = a.prop; // Works!
a.prop = 1; // Error!
```

Flow supports [private fields](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Classes/Private_class_fields),
a feature of ES2022. Private fields start with a hash symbol `#`:

```js flow-check
class MyClass {
  #internalValue: number;

  constructor() {
    this.#internalValue = 1;
  }

  publicApi() {
    return this.#internalValue;
  }
}

const a = new MyClass();
const x: number = a.#internalValue; // Error!
const y: number = a.publicApi(); // Works!
```

### Extending classes and implementing interfaces

You can optionally `extend` one other class:

```js flow-check
class Base {
  x: number;
}

class MyClass extends Base {
  y: string;
}
```

And also implement multiple [interfaces](./interfaces.md):

```js flow-check
interface WithXNum {
  x: number;
}
interface Readable {
  read(): string;
}

class MyClass implements WithXNum, Readable {
  x: number;
  read(): string {
    return String(this.x);
  }
}
```

You don't need to `implement` an interface to be a subtype of it, but doing so enforces that your class meets the requirements:

```js flow-check
interface WithXNum {
  x: number;
}

class MyClass implements WithXNum { // Error!
}
```

The right-hand side of `implements` and `extends` is constrained — see [Common Issues](#toc-cannot-implement) below.

### Abstract classes and members <SinceVersion version="0.339" /> {#toc-abstract-classes}

An `abstract` class is a base class that you cannot instantiate directly. Besides ordinary implemented members, it can declare `abstract` fields and methods, which every concrete subclass must implement.

```js flow-check
abstract class Animal {
  abstract name: string;
  abstract makeSound(): string;

  describe(): string {
    return this.name + ' says ' + this.makeSound();
  }
}

class Dog extends Animal {
  name: string = 'Rex';
  makeSound(): string {
    return 'woof';
  }
}

function introduce(animal: Animal): string {
  return animal.describe(); // OK
}

introduce(new Dog()); // OK
new Animal(); // Error!
```

An abstract class name still works as the type of its instances, so an `Animal` annotation accepts an instance of any subclass. An abstract field has a type annotation but no initializer. An abstract method has a signature but no body. You can declare `abstract` members only inside an `abstract` class.

A subclass that isn't abstract must implement every abstract field and method it inherits:

```js flow-check
abstract class Animal {
  abstract name: string;
  abstract makeSound(): string;
}

class Cat extends Animal { // Error! `makeSound` is not implemented
  name: string = 'Tom';
}

class Mute extends Animal { // Error! `name` is not implemented
  makeSound(): string {
    return '';
  }
}
```

An abstract subclass can leave inherited obligations to its own subclasses and add new ones. A concrete class can satisfy an obligation with an implementation it inherits from an intermediate class. Like any abstract class, an abstract subclass cannot be instantiated:

```js flow-check
abstract class Shape {
  abstract area(): number;
  abstract label(): string;
}

abstract class Polygon extends Shape {
  abstract sides(): number;
  label(): string {
    return String(this.sides()) + '-sided polygon';
  }
}

class Square extends Polygon { // OK: inherits `label` from `Polygon`
  area(): number {
    return 1;
  }
  sides(): number {
    return 4;
  }
}

class UnitSquare extends Square {} // OK: inherits every implementation

class Triangle extends Polygon { // Error! `area` is not implemented
  sides(): number {
    return 3;
  }
}

new UnitSquare().label() as string; // OK
new Polygon(); // Error!
```

Because an abstract member has no implementation, you cannot access it through `super`, even from a subclass that implements it. Concrete inherited members are still available through `super`:

```js flow-check
abstract class Logger {
  abstract write(message: string): void;
  log(message: string): void {
    this.write('[log] ' + message);
  }
}

class ConsoleLogger extends Logger {
  write(message: string): void {
    console.log(message);
  }
  info(message: string): void {
    super.log(message); // OK
    super.write(message); // Error!
  }
}
```

Abstract classes and members are ordinary class syntax, so you can use them in regular source files. Declaration files and [library definitions](../libdefs/creation.md) can also use `declare abstract class`. When types are stripped, `abstract` modifiers and abstract members are removed like other type annotations, so Flow enforces these rules only during type checking.

An abstract member must be a public instance member that a subclass can implement, so you cannot combine `abstract` with `static` or with a `#private` name.

:::info TypeScript comparison
Abstract classes and members follow TypeScript's model of an abstract base class. Classes are still [nominally typed](../lang/nominal-structural.md) in Flow. An implementation must also be compatible with the abstract member's type: for example, an implementation of `abstract m(x: string | number): void` cannot narrow its parameter to `string`.
:::

#### Adoption {#toc-abstract-classes-adoption}

To use this syntax, set `experimental.tslib_syntax=true` in `.flowconfig` and use Flow and flow-parser 0.339.0+ and Prettier 3.10.0+ with its built-in [`flow` parser](../tools/prettier.md).

### Class Constructors {#toc-class-fields-constructors}

You can initialize your class properties in class constructors:

```js flow-check
class MyClass {
  foo: number;

  constructor() {
    this.foo = 1;
  }
}
```

You must first call `super(...)` in a derived class before you can access `this` and `super`:

```js flow-check
class Base {
  bar: number;
}

class MyClass extends Base {
  foo: number;

  constructor() {
    this.foo; // Error
    this.bar; // Error
    super.bar; // Error
    super();
    this.foo; // OK
    this.bar; // OK
    super.bar; // OK
  }
}
```

However, Flow will not enforce that all class properties are initialized in constructors:

```js flow-check
class MyClass {
  foo: number;
  bar: number;

  constructor() {
    this.foo = 1;
  }

  useBar() {
    this.bar as number; // No errors.
  }
}
```

### Class Generics {#toc-class-generics}

Classes can also have their own [generics](./generics.md):

```js flow-check
class MyClass<A, B, C> {
  property: A;
  method(val: B): C {
    throw new Error();
  }
}
```

Class generics are [parameterized](./generics.md#toc-parameterized-generics).
When you use a class as a type you need to pass parameters for each of its
generics:

```js flow-check
class MyClass<A, B, C> {
  constructor(arg1: A, arg2: B, arg3: C) {
    // ...
  }
}

const val: MyClass<number, boolean, string> = new MyClass(1, true, 'three');
```

## Classes in annotations {#toc-classes-in-annotations}

When you use the name of your class in an annotation, it means an _instance_ of your class:

```js flow-check
class MyClass {}

const b: MyClass = new MyClass(); // Works!
const a: MyClass = MyClass; // Error!
```

See [here](./utilities.md#toc-class) for details on `Class<T>`, which allows you
to refer to the type of the class in an annotation.

## Common Issues {#toc-common-issues}

### Extracting methods {#toc-method-extraction}

Flow allows you to extract an instance method as a function value, but preserves the method's receiver requirement. Calling the extracted function without a receiver is therefore an error: at runtime `this` would be `undefined`, so any `this.field` access would crash.

```js flow-check
class Counter {
  count: number = 0;
  increment(): number { return ++this.count; }
}
const counter = new Counter();
const tick = counter.increment; // Works: `tick` retains its receiver requirement
tick(); // ERROR: the required receiver is missing
```

Destructuring behaves the same way:

```js flow-check
class MyClass { method() {} }
const a = new MyClass();
const {method} = a; // Works!
method(); // Error: the required receiver is missing
```

Keep the call bound (`counter.increment()`), bind the method to its original receiver (`counter.increment.bind(counter)`), or wrap it with an arrow (`const tick = () => counter.increment()`).

Instance methods retain a receiver requirement even when their bodies do not reference `this`. An unannotated static method retains one only when its body uses `this`; an explicit `this` parameter always applies.

Method shorthand on a plain [object type](./objects.md#toc-object-methods) has no implicit receiver type because usage of `this` in object literals is banned. Such methods can be extracted and called directly unless they declare an explicit `this` parameter.

### Rebinding `this` with `call`, `apply` and `bind` {#toc-this-rebinding}

`call`, `apply` and `bind` cannot safely accept a receiver merely because its type is compatible. A subclass method can rely on a more specific `this` than is visible at the call site:

```js
class X {
  x: string = 'x';
  foo(this: X): string { return this.x; }
}

class Y extends X {
  y: string = 'y';
  foo(this: Y): string { return this.y; }
}

const x: X = new Y(); // Fine: Y is a subtype of X
x.foo.call(new X()); // Would run Y.prototype.foo with an X as this
```

At runtime, `x.foo` is `Y.prototype.foo`, so rebinding it to `new X()` would evaluate `this.y` on an object with no `y`. Flow therefore only lets `call`, `apply` and `bind` re-supply the receiver from which the method was read. A different receiver reports `invalid-this-arg`:

```js flow-check
type Counter = {
  count: number,
  increment(this: Counter): number,
};

declare const counter: Counter;
declare const other: Counter;

counter.increment.call(counter); // Works
counter.increment.call(other); // ERROR: [invalid-this-arg]
```

`invalid-this-arg` is a [lint](../linting/rule-reference.md#toc-invalid-this-arg) that is enabled as an error by default; `// flowlint invalid-this-arg:off` turns it off for a region of a file.

### `implements` and `extends` RHS must be an interface or class {#toc-cannot-implement}

The right-hand side of `implements` (on a class) or `extends` (on an interface) must name an interface or class — passing an object-type alias errors:

- `class C implements ObjType` errors with `[cannot-implement]` ("…is not an interface").
- `interface I extends ObjType` errors with `[incompatible-use]` ("…is not inheritable").

Mapped or utility types applied to interfaces produce object types, which also won't be accepted in either clause. The fix is to introduce a named interface (or inline the members directly) instead.

## See Also {#toc-see-also}

- [Interfaces](./interfaces.md) — structural typing for classes, allowing different classes to be used interchangeably
- [Nominal & Structural Typing](../lang/nominal-structural.md) — why classes are compared by name, not shape
- [Generics](./generics.md) — parameterized types, used with classes, functions, and type aliases
- [Variance](../lang/variance.md) — controlling read-only and write-only properties
- [Utility Types](./utilities.md) — `Class<T>` for referring to the class type itself (not instances)
