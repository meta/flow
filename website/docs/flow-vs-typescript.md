---
title: "Flow for TypeScript Users"
slug: /flow-vs-typescript
description: "Flow vs. TypeScript: how object exactness, variance defaults, `as` casts, type guards, and Flow's `component`/`hook`/`renders` syntax differ from TypeScript."
---

Flow and TypeScript share most of the same syntax, much of the same vocabulary, and a large set of overlapping concepts (conditional types, mapped types, type guards, `keyof`, `as const`, `unknown`, `Readonly`). Flow's syntax has shifted to align with TypeScript's over the past several years. If you know TypeScript, your intuition will get you most of the way through a Flow program.

Is this Flow or TypeScript? You can't tell.
```js flow-check
type User = {
  readonly name: string,
  readonly age: number,
  readonly metadata: unknown,
};

function get<K extends keyof User>(
  user: User,
  key: K,
): User[K] {
  return user[key];
}

declare const user: User;
const age: number = get(user, 'age');
```

Where the two diverge, the divergence is usually a deliberate Flow choice in favor of stronger static guarantees. Flow rejects a number of patterns that TypeScript accepts but that can throw at runtime, silently corrupt values, or cause logic bugs. Flow also ships several features with no built-in TypeScript counterpart, covered in [Flow-only concepts](#toc-flow-only) below. The most prominent is React: Flow has its own first-class [`component`](./react/component-syntax.md), [`hook`](./react/hook-syntax.md), and [`renders`](./react/render-types.md) syntax.

This page is organized in four sections:

- [Concepts that transfer cleanly from TypeScript](#toc-transfer-cleanly) - start here for syntax and concepts you can reuse directly.
- [Shared concepts, different rules](#toc-shared-concepts-different-rules) - if TypeScript-shaped code is rejected by Flow, these are the most likely culprits.
- [Flow-only concepts with no built-in TypeScript equivalent](#toc-flow-only).
- [TypeScript-only features that do not exist in Flow](#toc-ts-only).

Additional reference sections cover [upcoming TS-aligned work](#toc-coming-soon), [legacy Flow syntax convergence](#toc-convergence), [shared config options](#toc-shared-options), and [external declaration mechanisms](#toc-declaration-mechanisms).

> **Scope note:** Unlike `tsc`, the `flow` binary itself is a typechecker only: it doesn't emit JavaScript. Compiling Flow syntax to runtime JS is handled by separate [build tools](./getting-started.md#toc-installation). TypeScript bundles both into one tool. *TypeScript claims below verified against version 6.0.3 with `strict` enabled*.

## Concepts that transfer cleanly from TypeScript {#toc-transfer-cleanly}

The features below are close enough in syntax and semantics that you can reuse your TypeScript intuition more or less directly.

- [Conditional types](./types/conditional.md) with `infer`, including the `infer T extends Bound` constraint form.
- [Mapped types](./types/mapped-types.md) including optionality removal `-?`.
- [Type guards](./types/type-guards.md) of the form `param is T` (with additional validations), including inferred type predicates.
- [Assertion functions](./types/assertion-functions.md) of the form `asserts param is T` or `asserts param` (with additional validations).
- [`T[K]`](./types/indexed-access.md) indexed access types.
- [`keyof T`](./types/utilities.md) operator.
- [`as const`](./types/const-expression.md) assertions.
- [`const` type parameters](./types/const-expression.md#const-type-parameters) - `function f<const T>(x: T): T`.
- [`unknown`](./types/unknown.md) top type.
- [Generic bounds](./types/generics.md#toc-adding-types-to-generics) with `<T extends Bound>`.
- [All-defaulted generic type arguments](#toc-generic-default-omission) can be omitted.
- [Utility types](./types/utilities.md): `Readonly`, `ReadonlyArray`, `ReadonlyMap`, `ReadonlySet`, `Pick`, `Omit`, `Record`, `Partial`, `Required`, `Exclude`, `Extract`, `NonNullable`, `Parameters`, `ReturnType`, `NoInfer`, `Awaited`, `ThisParameterType`, `OmitThisParameter`, `ConstructorParameters`, `InstanceType`, `Uppercase`, `Lowercase`, `Capitalize`, `Uncapitalize`.
- [Type-only imports and exports](./types/modules.md#toc-importing-and-exporting-types) - `import type` and `export type`.
- [Function-overload encoding via intersection](./types/intersections.md#toc-intersection-of-function-types) - `((x: number) => string) & ((x: string) => number)` resolves the per-call return type by argument type in both languages.
- [Ambient declaration forms](./libdefs/creation.md) like `declare const`, `declare let`, `declare class`, and `declare function`.

Two more syntax forms match TypeScript directly even though the underlying semantics diverge - see [Variance](#toc-variance).

- [Array](#toc-variance-arrays) shorthand `T[]` (in addition to `Array<T>`).
- [Variance keywords](#toc-variance-keywords): `readonly` on properties and `in` / `out` on type parameters.

### All-defaulted generic type arguments can be omitted {#toc-generic-default-omission}

Like TypeScript, Flow lets you omit a generic type's argument list when every type parameter has a default. The bare `Foo` form applies the defaults just like `Foo<>` does. If any type parameter lacks a default, you still need to supply the required arguments.

```js flow-check
type Foo<T = string> = {x: T};
type A = Foo;   // OK - uses the default `T = string`
type B = Foo<>; // OK - equivalent explicit form

const a: A = {x: "hello"};
const b: B = a;
```

## Shared concepts, different rules {#toc-shared-concepts-different-rules}

This section is where Flow most often surprises a reader coming from TypeScript. Both languages have these concepts (objects, classes, variance, refinement, generics, module exports, suppressions), but Flow's rules diverge in ways that might not be apparent by reading the code.

In most subsections the syntax is the same but the semantics differ: code that's accepted by TypeScript but rejected by Flow as wrong, unsound, or unsafe - often throwing at runtime, silently corrupting values, or causing logic bugs. The rest are renamed spellings, validations Flow adds at module boundaries, or a different suppression form. The subsection headers tell you which case applies.

- [Objects, classes, and interfaces](#toc-shape-rules) - exact objects, nominal classes, asymmetric class/object/interface subtyping
- [Variance](#toc-variance) - `readonly`, `writeonly`, `in`, and `out`
- [Type spellings](#toc-type-spellings) - `void` vs `undefined`, `?T`, `empty`, `unknown`
- [Refinement and module-level validation](#toc-validation) - validated type-guard and assertion-function bodies, refinement invalidation, module-boundary annotations
- [Explicit type controls](#toc-explicit-controls) - `as` casts and error suppressions

:::info Nominal vs. structural typing.
TypeScript is primarily structural - two types with the same public shape are interchangeable, with narrow nominal carve-outs (`#private` fields, `private`/`protected` modifiers, and `unique symbol`). Flow is structural for plain objects and functions, but deliberately *nominal* for [classes](#toc-classes-nominal), [opaque types](#toc-opaque-types), and [Flow Enums](#toc-flow-enums).

Flow chose nominal typing here because identity carries real information in those constructs: for example, a `UserId` and a `PostId` can have the same underlying representation but are not interchangeable. Treating identity nominally rather than structurally lets the type system catch entire categories of logic bugs (right shape, wrong meaning) and lets users model their domain at the level of *what something is*, not just *what it looks like*.
:::

### Objects, classes, and interfaces {#toc-shape-rules}

How Flow types objects, classes, and interfaces: the exact-by-default object rule, nominal class identity, the asymmetric subtyping rules, primitive-to-interface assignability, type-level spread, the `this`-binding rules for class methods and object literals, and tuple spread.

| Surface | TypeScript | Flow |
|---|---|---|
| [Object exactness](#toc-object-exactness) | `{x: number}` allows extra properties except for fresh object-literal excess-property checks. | `{x: number}` is exact by default; use `{x: number, ...}` when extra properties are allowed. |
| [Class / interface / object subtyping](#toc-classes-nominal) | Classes, interfaces, and object types are mostly interchangeable by structure. | Classes are nominal; object types accept only themselves; interfaces can accept all three |
| [`implements` / `extends` clauses](#toc-implements-extends-rhs) | Can target object-shaped utility types like `Pick<T, K>` or `Omit<T, K>`. | Must name an interface or class, not an object type alias. |
| [Primitives vs. interfaces](#toc-primitives-interfaces) | Primitives satisfy object/interface shapes that exist on the boxed prototype. | Primitives are not subtypes of object types or interfaces. |
| [Object combination](#toc-type-spread) | Intersections are the standard way to combine object types. | Use object type spread (`{...A, ...B}`); intersections, while supported for inexact objects, don't work for exact objects. |
| [`this` bindings](#toc-method-extraction) | An implicit method receiver is lost when the method is extracted; `this` in object literals is allowed. | Extracted class instance and interface methods retain their receiver requirement; `this` in object literals is banned. |
| [Tuple spread after an optional element](#toc-tuple-spread-optional) | Allowed; the resulting tuple type doesn't match the runtime layout when the optional element is absent. | Rejected when the source tuple's arity isn't statically fixed. |

#### Object exactness {#toc-object-exactness}

In Flow, object types are *exact by default*: `{x: number}` admits exactly the property `x` and no others. To allow additional properties, write the inexact form with a trailing `...`: `{x: number, ...}`. In TypeScript, object types are open at the type-system level - `{x: number}` allows additional properties, and the rule that catches extras is the *excess-property check*, which fires only on direct object-literal assignment.

That distinction matters because it explains why TypeScript code can look like it agrees with Flow's exactness when it doesn't: `const o: {x: number} = {x: 1, y: 2}` errors in TS too, but only because the literal is inlined. Indirect cases (assigning the literal to a variable first, passing it through a function, or other paths where the literal's "fresh" status is lost) type-check in TS and would not be exactness violations there. Flow's exactness applies uniformly regardless of binding shape, and shows up in two TS-shaped patterns below.

**• Pattern: extra properties slip through indirect assignment.**

```ts
// TypeScript:
type Settings = {
  volume: number,
  brightness: number,
};
const raw = {
  volume: 0.5,
  brightness: 0.8,
  theme: "dark",
};
const settings: Settings = raw;
Object.values(settings).map(v =>
  v.toFixed(2), // Runtime crash! `Settings` types every value as `number`, so `.toFixed` runs on the `"dark"` string
);
```

TypeScript accepts this: `raw` is inferred as `{volume: number, brightness: number, theme: string}`, which is structurally a subtype of `{volume: number, brightness: number}` because TS object types are open. The excess-property check does not fire on indirect assignment. Flow rejects the same code:

```js flow-check
// Flow:
type Settings = {
  volume: number,
  brightness: number,
};
const raw = {
  volume: 0.5,
  brightness: 0.8,
  theme: "dark",
};
const settings: Settings = raw; // ERROR
```

When the extra properties are intentional, the fix is the inexact form - write `{volume: number, brightness: number, ...}` so the type explicitly admits unknown additional properties:

```js flow-check
// Flow:
type Settings = {
  volume: number,
  brightness: number,
  ...
};
const raw = {
  volume: 0.5,
  brightness: 0.8,
  theme: "dark",
};
const settings: Settings = raw; // OK
```

**• Pattern: a property is forgotten and re-introduced at a different type.** Typical when modeling a Flow function on a TypeScript signature that takes a "looser" type and adds optional fields. TypeScript allows the property to be forgotten via inexact subtyping and then re-introduced at a different (optional) type - leaving `timestamp` typed as `number | undefined` while it actually holds the string `'2026-01-15'`:

```ts
// TypeScript:
type LogEntry = {
  id: string,
  timestamp: string,
};
type WithUnixTime = {
  id: string,
  timestamp?: number,
};

const e: LogEntry = {
  id: 'a',
  timestamp: '2026-01-15',
};
const base: {id: string} = e;       // `timestamp` "forgotten"
const widened: WithUnixTime = base; // `timestamp` re-introduced at a new type
widened.timestamp?.toFixed(); // Runtime crash! ?. only short-circuits on nullish; the value
                              // is the string '2026-01-15', so .toFixed runs and throws.
```

Flow blocks this path in two places: the literal translation fails at the forget step (`{id: string}` is exact by default), and modeling the forget with an inexact target pushes the failure to the re-introduce step. Exactness gates both directions: a property can only be forgotten when the target is inexact, and re-introduced only when the source is exact.

```js flow-check
// Flow:
type LogEntry = {
  id: string,
  timestamp: string,
};
type WithUnixTime = {
  id: string,
  timestamp?: number,
};

declare const e: LogEntry;

// Literal Flow translation: fails at the forget step because `{id: string}` is exact.
const baseExact: {id: string} = e; // ERROR

// Modeled with inexact target: forget step passes, but re-introduction fails.
const baseInexact: {id: string, ...} = e;     // OK
const widened: WithUnixTime = baseInexact;    // ERROR
```

#### Classes are nominal; the object/interface/class subtyping is asymmetric {#toc-classes-nominal}

TypeScript types classes structurally: `interface`, `type`, and class instances are largely interchangeable as long as the shapes match. In Flow, an *object type* describes a plain object (specifically the shape produced by an object literal `{...}`), while an [*interface*](./types/interfaces.md) describes a contract that any plain object, class instance, or interface-typed value can satisfy. From those definitions, combined with Flow's [nominal typing](./lang/nominal-structural.md) for classes (two distinct classes with the same members are not interchangeable), the one-way subtyping rules follow directly:

- An object literal flows into an object type or an interface - its shape and kind are both fully known at the point of construction.
- A class instance flows into an interface (the contract makes no claim about what backs it) but **not** into an object type (object types describe object literals, not instances).
- An interface-typed value flows into another interface but **not** into an object type - it could be backed by a class instance, and the object type wouldn't accept that backing value.

Inexactness on the target (`{a: number, ...}`) widens which *plain-object* properties are allowed but doesn't widen the *kinds of values* accepted - object types only describe plain objects. Class instances and interfaces are different kinds, and are themselves always inexact (interfaces by definition; class instances because a subclass could add members), so they still fail against an inexact object type, just with a different diagnostic:

- Against an *exact* object type (the default), the error is `[incompatible-exact]`.
- Against an *inexact* object type (`{a: number, ...}`), the error is `[class-object-subtyping]` with text "Class instances are not subtypes of object types; consider rewriting object type as an interface."

```js flow-check
// Flow:
class Foo {
  a: number = 1;
}
interface I { a: number }
type Obj = {a: number, ...};

declare function acceptsInterface(
  x: I,
): void;
declare function acceptsObj(
  x: Obj,
): void;
declare const someI: I;

acceptsInterface(new Foo()); // OK
acceptsInterface({a: 1});    // OK
acceptsObj({a: 1});          // OK
acceptsObj(new Foo());       // ERROR
acceptsObj(someI);           // ERROR: same [class-object-subtyping] code
```

The standard fix when you hit this in Flow is to switch the parameter type from object type to interface.

A further consequence of the kind distinction concerns `this`: Flow rejects `this` inside object-literal methods with `[object-this-reference]`, so those methods have no implicit receiver requirement. Class instance and interface methods do have one, and an extracted method [preserves it](#toc-method-extraction). Allowing a class or interface value to flow into an object type would erase that requirement, so Flow keeps the kinds distinct.

One more note: TypeScript's structural treatment of classes is *almost* total: `const c: C = {x: 1}` type-checks in TS even though `c` is annotated as a class instance. TS has a handful of nominal carve-outs on top of the structural default: ECMAScript `#private` fields, the `private` / `protected` access modifiers (both of which block assignability across distinct declarations), and `unique symbol` - but they're exceptions to an otherwise structural model. Classes are nominal in Flow by default; you don't need an additional marker to make distinct classes incompatible. This is why Flow's class/object error is a frequent surprise for TS users.

#### `implements` and `extends` clauses must name an interface or class {#toc-implements-extends-rhs}

The right-hand side of an `implements` or `extends` clause has stricter shape rules in Flow than in TypeScript. TypeScript lets you write `class C implements Omit<HTMLAttrs, "k">` or `interface I extends Pick<Y, K>` - any object type works. Flow rejects object types in those clauses with their own diagnostics:

- `class C implements ObjType` errors with `[cannot-implement]` "Cannot implement `ObjType` because it is not an interface."
- `interface I extends ObjType` errors with `[incompatible-use]` "Cannot extend `ObjType` ... because `ObjType` is not inheritable."

The standard Flow rewrite is to introduce an interface (`interface I { a: number; b: string }` then `class C implements I`) or to inline the members directly. Mapped/utility types applied to interfaces work in Flow but the result is an object type, which is exactly what these clauses won't accept - so the rewrite needs to land at an interface, not at an object-typed alias.

```js flow-check
// Flow:
type ObjType = {a: number, b: string};
class C implements ObjType { // ERROR: [cannot-implement]
  a: number = 1;
  b: string = "hi";
}
interface I extends ObjType { // ERROR: [incompatible-use]
  c: boolean;
}
```

#### Primitives are not subtypes of interfaces or object types {#toc-primitives-interfaces}

TypeScript treats `string` / `number` / `boolean` as structurally assignable to any interface or object type they satisfy: the primitive is checked against the members of its corresponding boxed prototype (`String.prototype` / `Number.prototype` / `Boolean.prototype`), so a `string` satisfies any interface whose members exist on `String.prototype` (e.g. `{length: number}`, `Iterable<string>`). No runtime boxing is implied: the compatibility is purely at the type level. Flow does not perform that check in `.js` files: a primitive flowing into an interface errors with `[incompatible-type]` ("Cannot use string as a subtype of interface"), and a primitive flowing into an object type errors with the generic `[incompatible-type]` code.

The runtime consequence is usually a logic bug. `Iterable<string>` is satisfied by both `Array<string>` and `string`: TS accepts the latter, so a caller can silently pass a bare string where a list of strings was intended. A `for...of` loop then yields characters instead of items.

```ts
// TypeScript:
function logAll(
  items: Iterable<string>,
) {
  for (const x of items) console.log(x);
}
logAll(["foo", "bar"]); // logs "foo" then "bar"
logAll("fb");           // String.prototype[Symbol.iterator] yields characters,
                        // so the loop walks code points: logs "f" then "b" instead of items
```

TypeScript accepts this via wrapper-promotion: `String.prototype[Symbol.iterator]` satisfies `Iterable<string>`, so a `string` flows in just as an array does. Flow rejects:

```js flow-check
// Flow:
function logAll(
  items: Iterable<string>,
) {
  for (const x of items) console.log(x);
}
const msgs = "foo";
logAll(msgs); // ERROR: `string` is not a subtype of `Iterable<string>`
```

The Flow rewrite is at the call site: if a one-element list was intended, wrap the string in `[s]`; otherwise the call was the wrong shape.

#### Object spread typing {#toc-type-spread}

Flow lifts `{...A, b: T}` to the type level: `type C = {...A, b: T}` is a real type annotation that combines `A`'s properties with `b: T`. TypeScript has no type-level spread; it uses intersection (`type C = A & {b: T}`) instead.

This isn't a stylistic choice: it falls out of [exact object types](./types/objects.md#exact-and-inexact-object-types). Because Flow's exact object types forbid unlisted properties, intersecting two exact object types produces an *impossible* type: a value would have to be exactly `A` *and* exactly `B` simultaneously, which is impossible to satisfy as soon as `A` and `B` differ at all (see [impossible intersection types](./types/intersections.md#toc-impossible-intersection-types)). So Flow needs a different operation to combine exact object types. Type-level spread (`{...A, ...B}`) is that operation, and it mirrors the runtime semantics of value-level spread directly: own properties only (so [interfaces can't be spread](./types/objects.md#object-type-spread), since they don't track own-vs-prototype), later keys overwrite earlier ones, and spreading an inexact type requires the result to also be marked inexact (with the spread before any named properties); otherwise Flow rejects the spread, since unknown properties from the source can't be soundly admitted into an exact target.

```js flow-check
// Flow:
type A = {x: number, y: string};
type C = {...A, z: boolean};
const c: C = {x: 1, y: "hi", z: true};
```

Flow's typing of value-level spread is also safer. TypeScript types `{...c}` as if it copies everything in the interface, but if the value is backed by a class, prototype methods aren't actually on the spread result at runtime, so the typing is unsound:

```ts
// TypeScript:
interface Counter {
  count: number;
  incr(): number;
}
class Impl {
  count = 0;
  incr(): number { return ++this.count; }
}
const c: Counter = new Impl();
const o = {...c};
o.incr(); // Runtime crash! `incr` is on `Impl.prototype`, not on the spread result
```

Flow rejects with `[cannot-spread-interface]` because interfaces don't track own-vs-prototype, so the result type can't be sound:

```js flow-check
// Flow:
interface Counter {
  count: number;
  incr(): number;
}
declare const c: Counter;
const o = {...c}; // ERROR: [cannot-spread-interface]
```

#### Extracted methods retain `this`; object literals ban it {#toc-method-extraction}

Flow's `this`-related rules head off the same runtime crash: a method body running with `this` undefined. Flow allows extracting class instance and interface methods, but preserves their receiver requirement so an unsafe call fails. For object literals, Flow rejects `this` references so there is no implicit receiver requirement to preserve.

|   | `this` usage | Method extraction |
|---|---|---|
| **Class instance methods** | Allowed | Allowed; receiver requirement retained |
| **Class static methods** | Allowed | Allowed; receiver retained only when the method uses `this` or declares it explicitly |
| **Object literals** | *Banned* | Allowed |

**• Method extraction from a class instance.** TypeScript treats methods as plain function values and lets them be extracted silently: calling the extracted version runs the method body with `this` undefined, and any access through `this.field` crashes.

```ts
// TypeScript:
class Counter {
  count = 0;
  incr(): number {
    return ++this.count;
  }
}
const counter = new Counter();
const tick = counter.incr;
tick(); // Runtime crash! `this` is undefined, so `++this.count` throws
```

Flow also tracks the `this` binding on method-shorthand properties of a class, but retains that requirement on the extracted function instead of rejecting the extraction:

```js flow-check
// Flow:
class Counter {
  count: number = 0;
  incr(): number {
    return ++this.count;
  }
}
const counter = new Counter();
const tick = counter.incr; // OK: `tick` retains its receiver requirement
tick(); // ERROR: the required receiver is missing
const tickFixed = () =>
  counter.incr(); // OK - arrow captures `this`
tickFixed(); // OK
```

Keep the call bound, bind the method to the original receiver, or wrap it with an arrow function as shown by `tickFixed` above.

Instance methods retain a receiver requirement even when their bodies do not reference `this`. An unannotated static method retains one only when its body uses `this`; an explicit `this` parameter always applies.

**• `this` inside an object literal.** TypeScript allows `this` references inside object-literal methods (TS infers `this` as the enclosing literal's type); the same extraction hazard then applies at the call site.

```ts
// TypeScript:
const counter = {
  count: 0,
  incr(): number {
    return ++this.count;
  }
};
const tick = counter.incr;
tick(); // Runtime crash! `this` is undefined, so `++this.count` throws
```

Flow forbids the construct itself with `[object-this-reference]`, so the unsafe extraction never gets the chance to be attempted:

```js flow-check
// Flow:
const counter = {
  count: 0,
  incr(): number { return ++this.count; } // ERROR: [object-this-reference]
};
```

The Flow rewrite is to use the name of the object literal binding directly instead of `this`.

```js flow-check
// Flow:
const counter = {
  count: 0,
  incr(): number {
    return ++counter.count;
  } // Use object name directly instead of `this`
};
const tick = counter.incr; // Extraction allowed: no `this` in the function
tick(); // OK
```

Method-shorthand on plain object *types* (`{m(x: number): number}`) carries no `this` context to lose, so extraction is fine there; this is also why a class instance can't flow into an object type but an object literal can.

#### Tuple spread after an optional element is banned {#toc-tuple-spread-optional}

Spreading a tuple type with optional elements into another tuple is allowed in TypeScript but produces an inaccurate tuple type: if the optional element is absent at runtime, the trailing positions shift left and the slots TS computed don't match the runtime layout.

```ts
// TypeScript:
const middle: [middleName?: string] = [];
const fullName: [
  string,
  string | undefined,
  string,
] = ["Ada", ...middle, "Lovelace"];
fullName[2].toUpperCase(); // Runtime crash! `fullName[2]` is undefined, not "Lovelace"
```

The TS type is statically known (`fullName` is annotated as `[string, string | undefined, string]` and that annotation is accepted), but it is unsound: when `middle` is empty at runtime, the value at position `1` is `"Lovelace"` (shifted) and position `2` is absent, so the tuple TS computed does not match the runtime layout. Flow rejects the spread with `[invalid-tuple-arity]` ("array literal has an unknown number of elements") because the source tuple's arity is not statically fixed, so no sound static tuple shape can be produced for the result:

```js flow-check
// Flow:
const middle: [middleName?: string] = [];
const fullName: [
  string,
  string | void,
  string,
] = ["Ada", ...middle, "Lovelace"]; // ERROR
```

The Flow rewrite is to branch explicitly on whether the optional element is present and assemble each shape on its own arm.

### Variance {#toc-variance}

Flow's variance defaults are stricter than TypeScript's. The subsections below cover the keyword syntax for opting in or out and the positions where the defaults diverge.

| Surface | TypeScript | Flow |
|---|---|---|
| [Variance keywords](#toc-variance-keywords) | Uses `readonly` properties and `in` / `out` type parameters; can also spell explicit invariance as `<in out T>`. | Uses `readonly` / `writeonly` properties and `in` / `out` type parameters; default type-parameter variance is invariant. |
| [Mutable object properties](#toc-variance-mutable-props) | Covariant. | Invariant. |
| [`readonly` property assignability](#toc-variance-readonly-props) | `readonly` and mutable properties are assignable in ways that can drop the read-only constraint. | `readonly` cannot be dropped by assigning to a mutable-property type. |
| [Mutable arrays](#toc-variance-arrays) | Covariant. | Invariant. |
| [Generic type arguments](#toc-variance-generics) | Variance is inferred from usage with compatibility-oriented exceptions. | Invariant by default unless declared `out` or `in`. |
| [Method parameters](#toc-variance-methods) | Bivariant for method syntax; function-typed fields are contravariant. | Contravariant. |
| [`this` type positions](#toc-this-variance) | Allowed in input and invariant (mutable field) positions. | Restricted to covariant positions (return types and `readonly` fields). |

:::info Variance - a quick overview.
*Variance* describes how subtyping flows through a position where a type `T` appears - for example, the property type in `{x: T}`, a function parameter or return type, or a generic argument like `Container<T>`. Given that `Sub` is a subtype of `Super`, that position is:

- **Covariant:** preserves direction. A `{readonly x: Sub}` is a subtype of `{readonly x: Super}`. The right choice for read-only positions and function return types.
- **Contravariant:** reverses direction. A function `(x: Super) => void` is a subtype of `(x: Sub) => void` - a callee that accepts wider inputs satisfies a caller passing narrower ones.
- **Invariant:** neither direction; the position can't soundly widen or narrow. The required default whenever a slot is both read *and* written (e.g., a mutable `{x: T}`), since covariance breaks writes and contravariance breaks reads.
- **Bivariant:** both directions accepted. Usually unsound; TypeScript permits it in a few places (notably method parameters). Flow never uses bivariance.

Flow defaults each position to the strictest sound choice; TypeScript defaults to looser ones at several positions.
:::

#### Variance keywords (`readonly` / `writeonly`, `in` / `out`) {#toc-variance-keywords}

Flow's standard syntax for variance uses the TS-aligned keyword forms: `readonly` / `writeonly` on properties and `in` / `out` on type parameters. Note that `writeonly` is Flow-specific: TypeScript has no write-only equivalent.

In the other direction, TypeScript's combined `<in out T>` (explicit invariance) has no Flow counterpart. TypeScript infers variance from usage and preserves several compatibility-oriented exceptions, so users sometimes need to opt *back into* invariance to recover the stricter guarantee they wanted; Flow's default is invariance, so the stricter choice is what you get when you write nothing. See [Generic type arguments](#toc-variance-generics) below for the defaults contrast in detail.

Beyond the spelling, Flow validates that a type parameter declared `out T` (or `in T`) is only used in body positions that match the declared variance - `out T` in an input position errors `[incompatible-variance]` "Cannot use `T` in an input position because `T` is expected to occur only in output positions." TypeScript also validates `in` / `out` against the body in many positions (e.g. `interface Box<out T> { set: (t: T) => void }` errors in TS too, since the function-typed *field* puts `T` contravariantly - function inputs flip variance). The narrower gap is that TS keeps *method shorthand* bivariant even under an `out`/`in` annotation, so the Flow form below (written with method shorthand) errors in Flow but compiles in TS.

```js flow-check
// Flow:
type Box<out T> = {
  set(t: T): void; // ERROR: [incompatible-variance]
};
```

This subsection is about the *syntax*; for the much more important *semantic* divergence in how variance is enforced at each position, see the next subsection. See the [variance docs](./lang/variance.md) for full mechanics.

Each subsection below is a place where Flow picks the stricter sound default and TypeScript picks the looser one. Together they are a large cluster of TypeScript code that type-checks but relies on weaker static guarantees - every example accepts a program that can throw at runtime or silently corrupt values.

#### Mutable object properties are invariant in Flow, covariant in TS {#toc-variance-mutable-props}

Assigning `{price: number}` to `{price: number | string}` widens the slot's read type but also widens what can be written into it, so a downstream `p.price = "Free!"` lands a string in the caller's `number`-typed property.

```ts
// TypeScript:
function markFree(p: {
  price: number | string,
}) {
  p.price = "Free!";
}
const item: {price: number} = {
  price: 9.99,
};
markFree(item);
item.price.toFixed(2); // Runtime crash! `.toFixed` is not a function on `"Free!"`
```

TypeScript allows the call: the property is covariant, so `{price: number}` is treated as a subtype of `{price: number | string}`. The mutation inside `markFree` then writes a string into the caller's `number`-typed slot. Flow rejects:

```js flow-check
// Flow:
function markFree(p: {
  price: number | string,
}) {
  p.price = "Free!";
}
const item: {price: number} = {
  price: 9.99,
};
markFree(item); // ERROR: property `price` is invariantly typed
```

The fix depends on whether the function genuinely needs to mutate. If it doesn't, make the parameter read-only (either with the [`Readonly<T>`](./types/utilities.md#toc-readonly) utility or the `readonly` property modifier), and removing the possibility of mutation through `p` makes the widening safe. If it does (as `markFree` here), the caller has to provide a source whose annotation already includes the wider type. TypeScript supports the same `readonly` property modifier, but see the next sub-bullet for how the two languages diverge on enforcing it.

```js flow-check
// Flow:
function logPrice(
  p: Readonly<{price: number | string}>,
) {}
const item: {price: number} = {
  price: 9.99,
};
logPrice(item); // OK
```

#### `readonly` properties are interchangeable with mutable in TS, but not in Flow {#toc-variance-readonly-props}

Assigning a `{readonly value: T}` to `{value: T}` would let a caller drop the read-only constraint and mutate the underlying object.

```ts
// TypeScript:
function f(obj: {value: number}) {
  obj.value = 99; // Silent mutation of a slot the caller annotated `readonly`
}
const o: {readonly value: number} = {
  value: 1,
};
f(o);
```

TypeScript allows the call; the mutation through `f` succeeds at runtime. TypeScript enforces `readonly` at direct write sites, but assignability can drop the readonly constraint. Flow enforces these property modifiers during type assignment and rejects this call:

```js flow-check
// Flow:
function f(obj: {value: number}) {
  obj.value = 99;
}
const o: {readonly value: number} = {
  value: 1,
};
f(o); // ERROR: [incompatible-variance]
```

The fix is to also mark the target read-only (`{readonly value: number}`) - once `f` declares it won't mutate, dropping the constraint is no longer at issue and the call succeeds. If `f` genuinely needs to mutate, the caller has to provide a mutable source instead.

```js flow-check
// Flow:
function f(
  obj: {readonly value: number},
) {}
const o: {readonly value: number} = {
  value: 1,
};
f(o); // OK
```

#### Mutable arrays are invariant in Flow, covariant in TS {#toc-variance-arrays}

Treating `Array<string>` as `Array<string | Error>` lets a `.push(new Error(...))` inside the callee plant an `Error` into the caller's string-typed array.

```ts
// TypeScript:
function appendError(
  es: Array<string | Error>,
) {
  es.push(new Error("oops"));
}
const errors: Array<string> = [];
appendError(errors);
errors[0].toUpperCase(); // Runtime crash! `errors[0].toUpperCase` is not a function
```

TypeScript allows the call. The push goes through, and the typed-as-`string` element at index 0 is actually an `Error` instance at runtime. Flow rejects:

```js flow-check
// Flow:
function appendError(
  es: Array<string | Error>,
) {
  es.push(new Error("oops"));
}
const errors: Array<string> = [];
appendError(errors); // ERROR: `Array<string>` is invariantly typed
```

The fix depends on whether the callee needs to mutate. If it doesn't, switch the parameter to `ReadonlyArray<T>` - removing the possibility of mutation through `es` makes the widening safe. `ReadonlyArray` exists in Flow precisely *because* the mutable form is invariant - a fact often missed when reaching for the covariant TS pattern. If the callee genuinely needs to mutate (as `appendError` does), the caller has to supply an array whose element type already includes the wider option.

```js flow-check
// Flow:
function logErrors(
  es: ReadonlyArray<string | Error>,
) {}
const errors: Array<string> = [];
logErrors(errors); // OK
```

#### Generic type arguments are invariant by default {#toc-variance-generics}

Flow defaults generic parameters to invariance and asks the user to opt into co/contravariance with `out T` / `in T`. TypeScript infers variance from usage and preserves compatibility-oriented exceptions, which can leave read-write fields with weaker static guarantees than the Flow default.

```ts
// TypeScript:
class Box<T> {
  value: T;
  constructor(value: T) {
    this.value = value;
  }
}
function widen(
  b: Box<number | string>,
) {
  b.value = "oh no";
}
const box: Box<number> = new Box(1);
widen(box);
box.value.toFixed(); // Runtime crash! `box.value` is now "oh no", not a number
```

TypeScript allows the call; `widen` then writes a string into the caller's number-typed slot. Flow rejects:

```js flow-check
// Flow:
class Box<T> {
  value: T;
  constructor(value: T) {
    this.value = value;
  }
}
function widen(
  b: Box<number | string>,
) {}
const box: Box<number> = new Box(1);
widen(box); // ERROR
```

When writing a Flow generic in this shape: either the field is genuinely read-only (mark it `readonly` and the parameter `out`) or it is not, in which case Flow's invariance is correct.

#### Method parameters are contravariant in Flow but bivariant in TS {#toc-variance-methods}

TypeScript's method-parameter variance is bivariant: `{compare(a: string, b: string): number}` is treated as a subtype of `{compare(a: string | number, b: string | number): number}`, so the wider type can pass numbers into a body that only handles strings, crashing at runtime. (TypeScript's function-typed *fields* are contravariant: this method-vs-field asymmetry is itself a TS-only wrinkle.)

```ts
// TypeScript:
type StringComparator = {
  compare(a: string, b: string): number,
};
type Comparator = {
  compare(
    a: string | number,
    b: string | number,
  ): number,
};

function callCompare(c: Comparator) {
  c.compare(1, 2); // Runtime crash! body calls .localeCompare on a number
}
const stringComparator: StringComparator = {
  compare(a, b) {
    return a.localeCompare(b);
  }
};
callCompare(stringComparator);
```

The bivariance hole is invisible at the call site: TS users see no error until runtime. Flow rejects the call statically, catching the bug before it can crash:

```js flow-check
// Flow:
type StringComparator = {
  compare(a: string, b: string): number,
};
type Comparator = {
  compare(
    a: string | number,
    b: string | number,
  ): number,
};

function callCompare(c: Comparator) {}
const stringComparator: StringComparator = {
  compare(a, b) {
    return a.localeCompare(b);
  }
};
callCompare(stringComparator); // ERROR
```

Flow's specific error depends on whether `compare` is method shorthand or a function-typed field. Method shorthand (as above) fails *contravariance* (`[incompatible-type]` "the first parameter: string is incompatible with number"): function inputs flip variance, so widening them is unsound.

Switching from method shorthand to a mutable function field makes the check stricter rather than looser. The property itself is now mutable, so the error becomes *invariance* (the property is invariantly typed), which blocks the *opposite* (safe) direction too.

Adding `readonly compare` restores that safe direction (a `Comparator`-typed value flowing into a `StringComparator` slot) by making the property covariant, but it does not fix the example above: function-input contravariance is still what blocks widening the inputs, and the only way to accept wider inputs is to declare `compare` with those wider inputs to begin with.

See the [variance docs](./lang/variance.md) and the [subtyping docs](./lang/depth-subtyping.md) for the full mechanics.

#### `this` type is restricted to covariant positions {#toc-this-variance}

The `this` type is used for fluent APIs and polymorphic method receivers. In covariant positions (return types, and `readonly` fields in Flow), both languages accept it: a method declared `m(): this` preserves the subclass type through fluent chains, so `new SubBuilder().add(1).extra()` keeps its `SubBuilder` type. The divergence is in input and invariant (mutable field) positions. TypeScript accepts `this` there too, but a writable slot typed `this` lets a caller write a parent-class instance into a subclass-typed field; later access that calls a subclass-only method crashes:

```ts
// TypeScript:
class Builder {
  parent: this | null = null;
}
class SubBuilder extends Builder {
  extra(): this { return this; }
}
function stash(b: Builder) {
  b.parent = new Builder(); // `b.parent` is `Builder | null` here, so the write is allowed
}
const sb = new SubBuilder();
stash(sb); // `SubBuilder` flows into `Builder`; the write inside `stash` lands on `sb.parent`
if (sb.parent !== null) {
  // The null check only excludes null; it doesn't verify the class identity.
  // The unsound write in `stash` put a `Builder` in a slot typed `this` (= `SubBuilder`).
  sb.parent.extra(); // Runtime crash! `sb.parent` is a `Builder`, not a `SubBuilder`
}
```

Flow rejects `this` in both input and invariant (mutable field) positions with `[incompatible-variance]`. This falls out of the same variance model that makes [mutable object properties](#toc-variance-mutable-props) and [mutable arrays](#toc-variance-arrays) invariant:

```js flow-check
// Flow:
class Builder {
  add(x: number): this { return this; } // OK: return type is covariant
  readonly origin: this | null = null; // OK: readonly field is covariant
  takesSelf(other: this): void {} // ERROR: input position
  parent: this | null = null; // ERROR: invariant field
}
```

The rewrite when you hit this is to name the class explicitly in the input/field position (`other: Builder`, `parent: Builder | null`) and accept the loss of the subclass type at that slot, or to make the field `readonly` so the position becomes covariant.

### Type spellings {#toc-type-spellings}

How Flow spells absent or nullable types and the top and bottom of the type hierarchy. These are name-only divergences: same concepts, different spellings.

| Concept | TypeScript | Flow | Note |
|---|---|---|---|
| Type inhabited only by `undefined` | `undefined` | `void` | Flow has no separate `undefined` type. ([details](#toc-void-vs-undefined)) |
| "No useful value" return marker | `void` | `void` | Same name; Flow has *only* `void` (see above). |
| Nullable value (`T \| null \| undefined`) | `T \| null \| undefined` | [`?T`](./types/maybe.md) (shorthand for `T \| null \| void`) | `T \| void` alone lacks `null`. |
| Bottom type | `never` | [`empty`](./types/empty.md) | `never` is the natural TS reach when Flow expects `empty`. |
| Top type | `unknown` | [`unknown`](./types/unknown.md) | Same name. |

See [type hierarchy](./lang/type-hierarchy.md) for where these sit relative to the rest of Flow's types.

#### `void` vs `undefined` {#toc-void-vs-undefined}

`undefined` as an annotation is a hard error in Flow:

```js flow-check
// Flow:
function f(): undefined { // ERROR: [unsupported-syntax]
  return undefined;
}
```

```js flow-check
// Flow:
function f(): void {
  return undefined; // OK - `undefined` is the value inhabiting `void`
}
```

This comes up most often when a TS-shaped function signature gets typed in Flow verbatim, and on TS utility-typed code (`Exclude<T, undefined>`, `T extends undefined ? ...`) where the `undefined` literal type appears inside a generic. Standard Flow forms: `undefined` → `void` for annotations; `T | undefined` → `?T` if `null` is also intended (most JS APIs) or `T | void` if only the absent case is intended.

A related TS quirk worth flagging: TypeScript's `() => undefined` and `() => void` are assignably asymmetric (`undefined` returns satisfy `void` slots but not vice versa). Flow has no equivalent since there's only `void`.

A parameter type that includes `void` (whether spelled `T | void`, `?T`, or `T | null | void`) makes the argument implicitly optional, so callers can omit it entirely. This differs from TypeScript, where `(x: T | undefined)` still requires the call site to pass `undefined`.

```js flow-check
// Flow:
function f(x: ?number) {}
f(null);      // OK
f(undefined); // OK
f();          // OK - `?T` includes `void`, which makes the arg optional
```

### Refinement and module-level validation {#toc-validation}

How Flow validates the bodies of type guards and assertion functions, when refinements are invalidated by intervening code, and the validation Flow performs at module boundaries (annotation requirements and the value/type seam).

#### User-defined type guard bodies are validated {#toc-type-guard-validation}

TypeScript checks the *signature* of an `x is T` predicate - it requires the predicate type to be assignable to the parameter type, so `function f(x: string): x is number` is rejected at declaration. But TypeScript does **not** check that the function body actually implements the claimed refinement. The body is trusted, so the following type-checks in TypeScript even though `typeof x === "object" && x !== null` is true for *any* non-null object (`{}` included), and any caller relying on this guard will be lied to:

```ts
// TypeScript:
type User = {id: string, name: string};
function isUser(x: unknown): x is User {
  return typeof x === "object" && x !== null;
}
const data: unknown = {};
if (isUser(data)) {
  data.name.toUpperCase(); // Runtime crash! `data` has no `name` property
}
```

Flow validates the body of a type-guard function in **both directions**. Each direction surfaces with its own diagnostic.

**• Positive direction (`[incompatible-type-guard]`).** At every `return` expression, the type of the refined parameter must be a subtype of the guard type. So the equivalent of the TypeScript example above is rejected:

```js flow-check
// Flow:
type User = {id: string, name: string};
function isUser(x: unknown): x is User {
  return typeof x === "object" && x !== null; // ERROR: refined type isn't a subtype of `User`
}
```

**• Negative direction (`[incompatible-type-guard]`).** When the predicate returns `false`, the negation must completely refine away the guard type from the parameter - otherwise a caller could see a value that *should* have been excluded. A predicate typed as `x is A` that actually checks `x instanceof B` (a strict subtype) is rejected for this reason:

```js flow-check
// Flow:
class A {}
class B extends A {}
function isA(x: unknown): x is A {
  return x instanceof B; // ERROR: negation does not refine `A` away
}
```

The diagnostic explicitly suggests the escape hatch: "Consider using a one-sided type-guard (`implies x is T`)." [One-sided guards](./types/type-guards.md) (`implies x is T`) skip exactly this negation check - they refine the parameter to `T` when the function returns `true` and leave it unchanged when it returns `false`, which is the right shape when only the positive direction holds.

See the [type guards docs](./types/type-guards.md#toc-consistency-checks-of-type-guard-functions) for the full consistency rules, and the [one-sided type guard](#toc-one-sided-guards) section below for the `implies` form.

#### Assertion functions {#toc-assertion-functions}

Flow supports both `asserts x is T` and `asserts x`. An [assertion function](./types/assertion-functions.md) returns `void` and narrows a value after the call returns normally.

Unlike TypeScript, Flow validates that paths where the asserted type or truthiness does not hold exit abnormally, for example by throwing an error. See [body validation](./types/assertion-functions.md#toc-validating-the-function-body) and [call restrictions](./types/assertion-functions.md#toc-call-restrictions).

#### Refinement invalidation rules differ {#toc-refinement-invalidation}

Both Flow and TypeScript narrow types via `typeof`, `instanceof`, equality, type guards, etc. - but the rules for when a refinement is dropped diverge in ways that aren't apparent from reading the code. Flow invalidates a refinement when intervening code could have changed the underlying value at that storage location:

- A write to the refined binding or property (`x = ...`, `obj.k = ...`).
- A refinement on an object property where the property is reachable through aliasing or could be mutated by a callee.
- A refinement on a binding captured by a closure that an intervening call could invoke.

A bare call to a function that does not visibly touch the refined location does **not** by itself drop a refinement on a local: that is the most common over-correction. TypeScript's narrowing has its own (also non-trivial) invalidation model that does not agree with Flow's in detail; the same code may type-check in TS and not in Flow, or vice versa. See [refinement invalidations](./lang/refinements.md#toc-refinement-invalidations) for the full rule set.

```js flow-check
// Flow:
declare function sideEffect(): void;

function localCase(x: ?number) {
  if (x != null) {
    sideEffect();        // bare call does NOT drop the refinement on a local
    const a: number = x; // OK
  }
}

function propertyCase(obj: {
  x: ?number,
}) {
  if (obj.x != null) {
    sideEffect();            // bare call DROPS the refinement on a property
    const a: number = obj.x; // ERROR: callee could have mutated `obj.x`
  }
}

function writeCase(x: ?number) {
  if (x != null) {
    x = null;
    const a: number = x; // ERROR: `x` is now typed `null` after the write
  }
}
```

The standard fix for the property case is to extract the refined value to a local before any intervening code - once it's a local, the bare-call exemption above applies and the refinement survives. The write case is fixed by not reassigning the refined binding; use a separate local for the new value instead.

```js flow-check
// Flow:
declare function sideEffect(): void;

function propertyCaseFixed(obj: {
  x: ?number,
}) {
  const {x} = obj;
  if (x != null) {
    sideEffect();
    const a: number = x; // OK - local refinement survives the call
  }
}
```

#### Annotations are required at module boundaries {#toc-annotations-boundaries}

Flow [requires annotations](./lang/annotation-requirement.md) on function parameters, exports, and other key boundaries, and reports `[signature-verification-failure]` if a module's exports cannot be typed from annotations alone. If you're used to leaving exports unannotated and letting the typechecker infer them across modules, that will not work in Flow: the annotations have to be there. TypeScript offers an opt-in equivalent (`--isolatedDeclarations`, TS 5.5+), but it's not part of `strict` and is off by default.

This is a deliberate design choice that enables Flow to scale to repositories with millions of files. Because each module's exports are fully described by its annotations, Flow can extract a "typed interface" for the module without analyzing the module body, then typecheck every other module against that interface in parallel.

See the [annotation requirement docs](./lang/annotation-requirement.md) and the [Module Exports](./lang/annotation-requirement.md#toc-module-exports) subsection for full mechanics.

```js
// Flow:
// ERROR: return type inferred, not annotated.
export function getUser(id: string) { // [signature-verification-failure]
  return {id, name: 'Alice', age: 30};
}

// OK - annotate the return so the module's typed interface is self-contained.
export function getUser(id: string): {
  id: string,
  name: string,
  age: number,
} {
  return {id, name: 'Alice', age: 30};
}
```

#### Use type-only imports and exports for type-only bindings {#toc-type-export-validation}

TypeScript's `import type` and `export type` are, by default, erasable annotations: the underlying classification happens at use site, so a value-position `import {Foo}` of a type-only export silently resolves as a type. (Under `--verbatimModuleSyntax`, TS does require `import type` at the import site and errors otherwise - but that's an opt-in mode, not the default.) Flow validates the value/type kind at the import and export site unconditionally, with two distinct diagnostics:

- A value-position `import {Foo}` from a module that only `export type`d `Foo` errors `[import-type-as-value]` "Cannot import the type `Foo` as a value. Use `import type` instead."
- A value-position `export {Foo}` where `Foo` is a type-only binding in the current module errors `[type-as-value]` "Cannot use type `Foo` as a value. Types are erased and don't exist at runtime."

The fix in both cases is the explicit type form: `import type {Foo}` or `export type {Foo}`. Flow needs this distinction when extracting a module's signature: the typed interface a module exposes has to be unambiguous about which exports are types and which are values, since dependents are checked against that interface in parallel without analyzing the module body.

```js
// Flow:
// mod.js
export type Foo = {x: number};
```

```js
// Flow:
// consumer.js
import {Foo} from './mod'; // ERROR: [import-type-as-value]
import type {
  Foo as FooType,
} from './mod'; // OK
```

### Explicit type controls {#toc-explicit-controls}

Two places where TypeScript accepts a looser surface spelling and Flow requires the explicit form: `as` casts and error suppressions.

#### `as` casts are safer in Flow {#toc-as-casts}

Flow's `as` only widens or asserts (e.g. `42 as number`, `42 as 42`), and rejects unsafe downcasts at the type level - `{id: 1} as {id: number, name: string}` is a Flow error, not a cast. TypeScript's `as` accepts any cast where the two types are assignable in *either* direction, which lets it silently approve unsafe downcasts. The same TypeScript line type-checks even though the cast invents a `name: string` property that doesn't exist at runtime, and accessing it then crashes at runtime.

```ts
// TypeScript:
const u = {id: 1} as {
  id: number,
  name: string,
};
u.name.toUpperCase(); // Runtime crash! `u.name` is undefined
```

Flow accepts the widening and assertion uses, but rejects unsafe downcasts:

```js flow-check
// Flow:
const n = 42 as number; // OK - widens the literal `42` to `number`
const exact = 42 as 42; // OK - asserts at the literal type
const u = {id: 1} as {id: number, name: string}; // ERROR - unsafe downcast
```

Flow's escape hatch for a forced cast is the explicit `value as any as T` two-step; TypeScript's idiom is `value as unknown as T`.

#### Error suppressions are coded and scoped {#toc-suppressions}

Flow's `$FlowFixMe[code]` (and `$FlowExpectedError[code]`) suppresses **only** the named error code at that location: any other error on the same line still surfaces, and the suppression itself is reported as an unused-suppression warning if the targeted code doesn't fire. TypeScript's `// @ts-ignore` silences every error on the next line indiscriminately, and `// @ts-expect-error` similarly silences everything but errors when nothing was suppressed. The Flow form is strictly more granular and keeps suppression debt easy to track. See the [errors docs](./errors/index.md).

```js flow-check
// Flow:
const o: {x: number} = {x: 1};
// $FlowFixMe[prop-missing] - intentional for demo
const y = o.nonexistent;
```

## Flow-only concepts with no built-in TypeScript equivalent {#toc-flow-only}

These are the constructs Flow has built that TypeScript hasn't built into the language or typechecker: most of them because the underlying problem (React component shapes, hook rules, render constraints, exhaustive pattern matching, nominal abstraction across module boundaries, runtime-and-type-level enums) is one TypeScript leaves to framework/library patterns, lint rules, or user code. There is no built-in TypeScript equivalent to translate from, only a Flow concept to learn fresh.

### `component` syntax {#toc-component-syntax}

Flow ships [first-class `component` syntax](./react/component-syntax.md) for declaring React components. There is no built-in TypeScript equivalent; TypeScript models components as plain functions. What the `component` keyword buys over a plain function:

- **Individual named params instead of a props object.** Removes the destructuring-and-typing duplication of `({name}: {name: string})` and the need to wrap props in `Readonly<{...}>` - component params are read-only by default.
- **No return type annotation.** Flow infers and enforces `React.Node`, and rejects components that implicitly return.
- **Optional [`renders`](./react/render-types.md) clause** that constrains what JSX shape the component is allowed to produce - covered in [its own section](#toc-renders-types) below.
- **Structural rules enforced at parse/type-check:** no `this`, no nested component definitions, and components can only be rendered as JSX, not called as plain functions.

```js flow-check
// Flow:
import * as React from 'react';

component Greeting(
  name: string,
  greeting: string = "Hello",
) {
  return <p>{greeting}, {name}!</p>;
}

component App() {
  return <Greeting name="World" />;
}
```

TypeScript models the same shape with a plain function component plus a props type. Unlike Flow's `component`, the resulting value is also callable as a plain function:

```tsx
// TypeScript:
import * as React from 'react';

type GreetingProps = {
  name: string,
  greeting?: string,
};
function Greeting({
  name,
  greeting = "Hello",
}: GreetingProps) {
  return <p>{greeting}, {name}!</p>;
}

function App() {
  return <Greeting name="World" />;
}

const elem = Greeting({name: "World"}); // No error: TS treats components as callable
```

Nested component definitions and function-style calls surface as `[nested-component]` and `[react-rule-call-component]` respectively:

```js flow-check
// Flow:
import * as React from 'react';

component Outer() {
  component Inner() { // ERROR: components may not be nested within other components or hooks
    return <div />;
  }
  return <Inner />;
}

const x = Outer({}); // ERROR: components cannot be called; use JSX (<Outer />) instead
```

### `hook` syntax {#toc-hook-syntax}

[`hook` syntax](./react/hook-syntax.md) is a first-class Flow keyword for declaring React hooks. Flow uses the keyword to enforce the [Rules of React](https://react.dev/reference/rules) at the type level on hook call sites. TypeScript has no equivalent: hook rules in TS are enforced by ESLint (`eslint-plugin-react-hooks`), which operates on AST patterns without type information or whole-program analysis.

Flow tracks hook-ness as part of the function type, which lets it catch violations that an AST-based linter can't reach. Below, `useToggle` is passed where a non-hook function is expected; Flow errors with `[react-rule-hook-incompatible]`. The call site has no syntactic clue that `fn` will be invoked as a hook inside the callee, so ESLint's AST pattern matching can't flag it:

```js flow-check
// Flow:
import {useState} from 'react';

hook useToggle(
  initial: boolean,
): [boolean, () => void] {
  const [v, sv] = useState(initial);
  return [v, () => sv(x => !x)];
}

function callIt(
  fn: (boolean) => [boolean, () => void],
): [boolean, () => void] {
  return fn(false);
}

callIt(useToggle); // ERROR: `useToggle` is a hook; `fn` is not
```

TypeScript has no concept of hook-ness; `useToggle` is just a function. The call below type-checks even though `callIt` invokes `useToggle` outside a component or hook at runtime, and `eslint-plugin-react-hooks` doesn't catch it either - the call site has no syntactic clue that `fn` will be invoked as a hook inside the callee:

```ts
// TypeScript:
import {useState} from 'react';

function useToggle(
  initial: boolean,
): [boolean, () => void] {
  const [v, sv] = useState(initial);
  return [v, () => sv(x => !x)];
}

function callIt(
  fn: (b: boolean) => [boolean, () => void],
): [boolean, () => void] {
  return fn(false);
}

callIt(useToggle); // No type error; rule-of-hooks violation surfaces at runtime
```

### `renders` types {#toc-renders-types}

[Render types](./react/render-types.md) (`renders`, `renders?`, `renders*`) declare composition contracts for components: what slots accept and what components produce. Design systems and libraries can constrain composition across wrapper components and HOCs, with the type checker rejecting violations. There is no built-in TypeScript equivalent.

```js flow-check
// Flow:
import * as React from 'react';

component Header(
  text: string,
  color: string,
) {
  return (
    <div style={{color}}>{text}</div>
  );
}
component MainHeader(
  text: string,
) renders Header {
  return (
    <Header text={text} color="red" />
  );
}

component Layout(
  header: renders Header,
) {
  return (
    <div>
      {header}
      <section>Content</section>
    </div>
  );
}

const ok = (
  <Layout
    header={<MainHeader text="Flow" />}
  />
);
const bad = <Layout header={<footer />} />; // ERROR: `<footer />` does not satisfy `renders Header`
```

TypeScript has no equivalent. The closest is typing slots as `React.ReactNode`, which accepts any node - no composition contract is expressible, so the `<footer />` case below is accepted just like the intended `<MainHeader />`:

```tsx
// TypeScript:
import * as React from 'react';

function Header({
  text,
  color,
}: {text: string, color: string}) {
  return (
    <div style={{color}}>{text}</div>
  );
}
function MainHeader({text}: {
  text: string,
}) {
  return (
    <Header text={text} color="red" />
  );
}

function Layout({header}: {
  header: React.ReactNode,
}) {
  return (
    <div>
      {header}
      <section>Content</section>
    </div>
  );
}

const ok = (
  <Layout
    header={<MainHeader text="TS" />}
  />
);
const bad = (
  <Layout header={<footer />} />
); // No type error
```

### `match` expressions and statements {#toc-match}

Flow has [`match` expressions and statements](./match/index.md) for pattern matching with structural patterns, guards, and exhaustiveness checking. TypeScript has no `match`; the closest equivalent for the statement form is a hand-coded discriminated-union `switch` with an `assertNever` fallthrough. The expression form has no direct TS equivalent at all, because `switch` is statement-only in JavaScript - TS users typically reach for nested ternaries or an IIFE wrapping a `switch`, both of which lose the structural patterns, guards, and exhaustiveness checks `match` provides.

```js flow-check
// Flow:
type Action =
  | {type: 'add', text: string}
  | {type: 'toggle', id: string}
  | {type: 'remove', id: string}
  | {type: 'filter', mode: 'all' | 'active' | 'done'};

declare const action: Action;

const description: string = match (action) {
  {type: 'add', const text} =>
    `Add: ${text}`,
  {type: 'toggle', const id} =>
    `Toggle ${id}`,
  {type: 'remove', const id} =>
    `Remove ${id}`,
  {type: 'filter', mode: 'all'} =>
    'Show all',
  {type: 'filter', mode: 'active'} =>
    'Show active',
  {type: 'filter', mode: 'done'} =>
    'Show done',
};
```

TypeScript has no `match`. Nested ternaries are the closest expression-form equivalent - structural patterns, in-pattern variable extraction, and exhaustiveness checks are all gone:

```ts
// TypeScript:
type Action =
  | {type: 'add', text: string}
  | {type: 'toggle', id: string}
  | {type: 'remove', id: string}
  | {type: 'filter', mode: 'all' | 'active' | 'done'};

declare const action: Action;

const description: string =
  action.type === 'add' ? `Add: ${action.text}` :
  action.type === 'toggle' ? `Toggle ${action.id}` :
  action.type === 'remove' ? `Remove ${action.id}` :
  action.type === 'filter' && action.mode === 'all' ? 'Show all' :
  action.type === 'filter' && action.mode === 'active' ? 'Show active' :
  action.type === 'filter' && action.mode === 'done' ? 'Show done' :
  (() => { throw new Error('unreachable'); })();
```

`match` is exhaustively checked: omitting a case is `[match-not-exhaustive]`, and the error names the missing pattern. Adding a new action type or a new filter mode then surfaces every site that hasn't handled it:

```js flow-check
// Flow:
type Action =
  | {type: 'add', text: string}
  | {type: 'toggle', id: string}
  | {type: 'remove', id: string}
  | {type: 'filter', mode: 'all' | 'active' | 'done'};

declare const action: Action;

const description: string = match (action) { // ERROR: missing `{type: 'filter', mode: 'done'}`
  {type: 'add', const text} =>
    `Add: ${text}`,
  {type: 'toggle', const id} =>
    `Toggle ${id}`,
  {type: 'remove', const id} =>
    `Remove ${id}`,
  {type: 'filter', mode: 'all'} =>
    'Show all',
  {type: 'filter', mode: 'active'} =>
    'Show active',
};
```

### Opaque types {#toc-opaque-types}

[Opaque type aliases](./types/opaque-types.md) hide their underlying type outside the file in which they are defined, enforcing nominal abstraction across module boundaries. TypeScript has no native equivalent; the common idiom there is "branded types" using intersection with a (typically `unique symbol`-keyed) marker property, which is a userland pattern rather than a language feature.

The boundary the branded types idiom enforces is weaker than Flow's file-scoped abstraction: a single `as` cast is enough to produce a branded value. `42 as UserId` type-checks in TypeScript because the source (`number`) and the target (`number & {readonly [brand]: 'UserId'}`) overlap on `number`, and TS only rejects an `as` cast when the two sides are disjoint.

Flow's opaque types, by contrast, are sealed by the module boundary itself: outside the defining file, the underlying type is not visible at all, so `as` widening cannot produce the opaque type from its underlying representation.

```js flow-check
// Flow:
opaque type UserId = number;

declare function makeUserId(
  n: number,
): UserId;
declare function lookupUser(
  id: UserId,
): string;

const id: UserId = makeUserId(42);
lookupUser(id); // OK
// In another file, `42` is not a `UserId` and a `UserId` is not a `number`.
// Inside this file (where the underlying type is visible) the conversion is allowed:
const n: number = id;
```

The view from another file: outside the defining module, the underlying type is sealed. `declare opaque type UserId` (the shape the importer sees) makes the seal visible - both the structural construction `42` and the projection back to `number` are rejected:

```js flow-check
// Flow (importer's view, as if `UserId` and functions were imported from another file):
declare opaque type UserId;
declare function makeUserId(
  n: number,
): UserId;
declare function lookupUser(
  id: UserId,
): string;

const id: UserId = makeUserId(42);
lookupUser(id); // OK
const forged: UserId = 42;    // ERROR: number is not a UserId
const n: number = id;         // ERROR: UserId is not a number
```

The TypeScript branded types encoding, with the unsafe `as` cast at the bottom:

```ts
// TypeScript:
declare const brand: unique symbol;
type UserId = number & {
  readonly [brand]: 'UserId',
};

declare function makeUserId(
  n: number,
): UserId;
declare function lookupUser(
  id: UserId,
): string;

const id: UserId = makeUserId(42);
lookupUser(id); // OK

// `as` cast: source and target overlap on `number`, so TS accepts.
const forgedByCast = 42 as UserId;
lookupUser(forgedByCast);
```


### Flow Enums {#toc-flow-enums}

[Flow Enums](./enums/index.md) and TypeScript `enum` look superficially similar but are very different in detail.

| Aspect | TypeScript | Flow |
|---|---|---|
| Exhaustive `switch` | No built-in diagnostic; encoded with `never` or lint. | Built-in: `[invalid-exhaustive-check]` if a member is forgotten. |
| Implicit coercion to/from underlying primitive | Permits number → number-enum slots (except non-matching literals) and freely coerces enums to numbers. | Blocked both directions; use `.cast()` to convert in, and `value as <representation type>` (for example `as string` / `as number`) to convert out. |
| Default member values | Number enums auto-number from `0`. | Number-enum members must be explicitly initialized; string enums default to mirroring member names. |
| Re-declaration | Allowed; can collide with default values silently. | `[name-already-bound]`. |
| Reverse mapping | Number enums get a runtime reverse-map; string enums error on the same access. | `.getName(value)` works for both number and string enums. |
| Iterating members | `Object.values(Status)` is the natural value-iteration form, but on a number enum it returns both the values *and* the member-name strings from the runtime reverse-map. | `Status.members()` returns just the values. |
| Symbol enums | None. | Supported (`enum X of symbol { ... }`). |
| Definition restrictions | Permits heterogeneous initializers, non-literal initializers, and lowercase-leading member names. | All three error. |

A few of these have rationales worth knowing:

- The default-value rule exists because adding or removing a member from the middle of an auto-numbered enum silently renumbers everything after it, which is a serialization/logging hazard.
- The TS string-enum reverse-mapping error is structural: `StatusStr.Off` has literal type `"off"` (the value), not `"Off"` (the key), so `StatusStr[StatusStr.Off]` resolves to a non-existent `StatusStr["off"]`.
- TS `Object.values` over a 3-member number enum produces `[ 'Active', 'Paused', 'Off', 0, 1, 2 ]` - both halves of the runtime reverse-map land in the result (and `for...in` over the same enum exposes the same duplication on its keys).
- Lowercase-leading member names are reserved because Flow Enums expose lowercase methods like `.cast` and `.members`.

Exhaustiveness is built in: omitting a case in a `switch` over a Flow Enum is `[invalid-exhaustive-check]`, with the missing member named. Adding a new member then surfaces every site that hasn't handled it:

```js flow-check
// Flow:
enum Status {
  Active,
  Paused,
  Off,
}

declare const st: Status;

let label: string;
switch (st) { // ERROR: member `Off` has not been considered
  case Status.Active:
    label = 'on';
    break;
  case Status.Paused:
    label = 'wait';
    break;
}
```

See the [Flow Enums docs](./enums/index.md) for full mechanics.

### One-sided type guards (`implies`) {#toc-one-sided-guards}

A predicate function whose return type is `implies param is T` refines the parameter to `T` only when the function returns `true`, and leaves it unchanged when the function returns `false`. This is the escape hatch for the [body-validation rule](#toc-type-guard-validation) covered above when only the positive direction holds. TypeScript has no equivalent.

For example, `isPositive(n: ?number)` returns `true` only for positive numbers. A two-sided `n is number` would be unsound here: the false branch can be `null`, `void`, or a non-positive number like `0` or `-1`, but `?number` minus `number` only leaves `null | void` - silently losing the non-positive case. The `implies` form says only the positive direction refines, so the else branch keeps the original `?number` type.

```js flow-check
// Flow:
function isPositive(
  n: ?number,
): implies n is number {
  return n != null && n > 0;
}

declare const n: ?number;
if (isPositive(n)) {
  n as number; // OK: refined to number
} else {
  n as ?number; // OK: stays ?number (not narrowed to null | void)
}
```

TypeScript has only two-sided type guards. The else branch always refines the predicate type away, even when that refinement isn't sound. The code below type-checks, but the inferred else-branch type is wrong - `n` could actually be a non-positive number at runtime:

```ts
// TypeScript:
function isPositive(
  n: number | null | undefined,
): n is number {
  return n != null && n > 0;
}

declare const n:
  | number
  | null
  | undefined;
if (isPositive(n)) {
  // n: number
} else {
  // TS narrows n to `null | undefined` here, even though a non-positive
  // number (e.g. 0, -1) would also reach this branch at runtime.
}
```

### `import typeof` {#toc-import-typeof}

Both Flow and TypeScript support `import type` for importing a type export. Flow also supports `import typeof`, which imports the type of a value export so you can use it in a type annotation.

TypeScript's nearest equivalent for `import typeof` is `typeof import('./m')`, but the binding shape differs: TypeScript produces the namespace shape and is usually combined with indexed access (`typeof import('./m')['Foo']`), while `import typeof Foo from './m'` binds a single value's type as a top-level type binding.

The example below uses both forms: `import type {Node}` pulls in a type export directly as a type annotation, while `import typeof {useState}` pulls in the type of a value export. A generic value's type stays parameterizable through `import typeof` - you can write `useState<number>` against the imported type, instantiating the imported function type to its `number` specialization, so the hook parameter is callable as a `number`-typed `useState`.

```js flow-check
// Flow:
import type {
  Node as ReactNode,
} from 'react';
const node: ReactNode = "hello, world";

import typeof {useState} from 'react';
hook useCounter(
  useStateNum: useState<number>,
): number {
  const [count, setCount] =
    useStateNum(0);
  setCount(c => c + 1);
  return count;
}
```

### Flow-only utility types {#toc-flow-only-utilities}

A handful of [utility types](./types/utilities.md) have no TypeScript counterpart. The closest TS spellings (where one exists) are noted below; the rest have no native TS form and are typically encoded with userland patterns.

- [`Class<T>`](./types/utilities.md#toc-class) - the type of the class constructor for an instance type `T`. No TS native form; the usual TS encoding is `new (...args: any[]) => T` or `typeof T` for a specific class.
- [`Values<T>`](./types/utilities.md#toc-values) - the union of value types of `T`'s properties. TS spelling is the indexed access `T[keyof T]`.

### Flow-only syntactic forms {#toc-flow-only-syntax}

A handful of Flow type-annotation forms have no TypeScript spelling - `tsc` rejects them at parse time. They are alternate syntax for existing Flow concepts.

- **Inline `interface` type annotation** - `type T = interface { foo: number }`. Lets an interface appear inside a type expression instead of as a top-level declaration. TypeScript requires a separate `interface I { ... }` statement.
- **Optional indexed access type** - `Obj?.['prop']` mirrors the runtime `?.` operator at the type level: if `Obj` is nullish, the result is `void`; otherwise it is `Obj['prop']`. TypeScript has no type-level `?.`.
- **Anonymous function-type parameters** - `type F = string => void`. Flow lets you omit the parameter name when it carries no information; TypeScript requires `(x: string) => void`.
- **Anonymous indexer parameters** - `type O = {[string]: number}`. Same shape: Flow omits the index-key name when it isn't referenced; TypeScript requires `{[k: string]: number}`.

```js flow-check
// Flow:
type Inline = interface { foo: number };
type Opt = ?{foo: number};
type Pulled = Opt?.['foo']; // number | void
type Fn = string => void;
type Dict = {[string]: number};
```

### Relay / GraphQL integration {#toc-relay-integration}

Setting `relay_integration=true` in `[options]` makes Flow natively understand `graphql` tagged template literals and infer their types from the Relay compiler's emitted artifacts, so users can omit explicit type parameters on `useFragment`, `usePreloadedQuery`, etc. Companion options: `relay_integration.esmodules` (resolve artifacts as ES module default exports rather than CommonJS) and `relay_integration.excludes` (per-directory opt-out). See the [docs for this option](./config/options.md#toc-relay-integration).

TypeScript has no typechecker-level equivalent. TypeScript users either pass the generated type explicitly (`useFragment<MyFragment$key>(...)`), use a TypeScript *language service plugin* for editor hints (not typechecking), or use document-node patterns like `graphql-typed-document-node` / `gql.tada` that require explicit imports of generated types.

With `relay_integration=true`, Flow reads the Relay compiler's emitted artifact for `MyFragment` and infers the key and result types from the `graphql` tag - no type parameter, no generated-type import:

```js
// Flow:
import {
  graphql,
  useFragment,
} from 'react-relay';
declare const userRef: MyFragment$key;

const data = useFragment(
  graphql`fragment MyFragment on User { name }`,
  userRef,
);
```

TypeScript requires the explicit type parameter and a generated-type import:

```ts
// TypeScript:
import {
  graphql,
  useFragment,
} from 'react-relay';
import type {
  MyFragment$key,
} from './__generated__/MyFragment.graphql';
declare const userRef: MyFragment$key;

const data = useFragment<MyFragment$key>(
  graphql`fragment MyFragment on User { name }`,
  userRef,
);
```

## TypeScript-only features that do not exist in Flow {#toc-ts-only}

These are TypeScript features that have no Flow equivalent today. Some Flow has deliberately not adopted, either because they overlap a Flow feature with different (usually more conservative) defaults or because they introduce footguns Flow's design avoids. Others are simply not implemented yet (see the separate [Coming soon](#toc-coming-soon) section for features in progress or pending a release gate). Reaching for any of the items below in Flow code won't work, and in some cases the TypeScript syntax will parse, so the failure shows up later than expected.

### TS-only syntactic forms {#toc-ts-only-syntax}

A handful of TS surface-syntax forms have no Flow spelling, but the *concept* is available in Flow under a different name. Flow rejects the TS form at parse/type-check time with a diagnostic that points at the Flow rewrite directly.

- **Angle-bracket type assertion** - TS `<T>x` → Flow `x as T`.
- **Optional unlabeled tuple elements** - TS `[number, string?]` → Flow `[a: number, b?: string]`. Flow requires the labeled variant for optional elements.
- **`readonly` type operator on tuples** - TS `readonly [T, S]` → Flow `Readonly<[T, S]>`.
- **`readonly` type operator on array shorthand** - TS `readonly T[]` → Flow `ReadonlyArray<T>`.

Note that `readonly` as a *property modifier* (`{readonly x: T}`) and on type parameters (`out T`) works the same way in both languages - see [Variance keywords](#toc-variance-keywords). The two `readonly` forms above are uses of `readonly` as a *type operator* (a prefix on a structural type), which is a TS-only spelling: Flow uses the wrapper utility instead.

### Decorators {#toc-decorators}

Flow parses decorator syntax but does not type-check it: the decorator's type is never applied to the underlying value, and the decoration is silently erased. TypeScript supports two incompatible modes: *TC39 decorators* (the default, with a context-object parameter) and *legacy decorators* (under `--experimentalDecorators`, with the old `(target, key)` signature).

### TypeScript class syntax extensions {#toc-class-extensions}

TypeScript has several class-syntax extensions Flow has deliberately not adopted, asking users to write the equivalent JS instead.

- **Parameter properties** (`constructor(public x: number)`) - a TS-only shorthand that emits runtime code: it auto-declares the field and assigns it from the constructor argument. Flow's diagnostic: "Flow does not support TypeScript parameter properties. To fix, declare the property in the class body and assign it in the constructor."

- **`public` / `protected` / `private` access modifiers** - TS-checked access control. These are type-checker-only in TypeScript (the field is still publicly accessible at runtime), so dropping them is safe. Flow rejects all three; drop them (`public` / `protected` carry no runtime effect) or migrate `private foo` to `#foo`. The `#private` rewrite lands at a different runtime shape from the TS form: ECMAScript `#private` fields are nominally private at runtime, while TS `private` is erased.

- **`accessor` auto-accessors** (`class C { accessor x: T = init }`) - a TC39 proposal that desugars to a paired getter/setter backed by a private field. Flow does not parse the form. Write the getter and setter explicitly with a `#private` backing field, or use a plain field if no accessor wrapping is needed.

### Runtime `namespace` blocks {#toc-namespace-blocks}

No source-level `namespace { ... }` blocks. Flow has [`declare namespace`](./libdefs/creation.md) for ambient declarations inside libdefs, but not source-level namespace blocks that produce runtime values.

### `const enum` {#toc-const-enum}

No equivalent. Flow's transforms emit Flow Enums as a runtime object rather than inlining them at the use site like `tsc` does for `const enum`. Their restrictions (literal-only values, no redeclaration, no default number values) still make build-system inlining straightforward if you want it.

### `ThisType<T>` utility {#toc-this-type-utility}

TypeScript's `ThisType<T>` is a marker used inside a contextual type to rewire `this` to `T` within the methods of an object literal. Flow does not implement that rewiring. The two language differences that make `ThisType<T>` useful in TypeScript are absent in Flow:

- Object literals [reject `this` references](#toc-method-extraction) outright, so there is no object-literal method body whose `this` Flow could rewire.
- Class instance and interface methods retain their [`this` requirement](#toc-method-extraction) when extracted, which cannot be reassigned by an external marker.

### Expressions with type arguments {#toc-expression-type-args}

TypeScript accepts type arguments on a *value expression*: `Foo<string>` as a standalone expression specializes the generic and can be bound to a name. Flow does not parse the form and errors with a `ParseError` shortly after the closing `>`.

```ts
// TypeScript:
declare class Foo<T> {
  value: T;
}
const StringFoo = Foo<string>;
```

The Flow rewrite is to supply the type arguments at the instantiation or call site (`new Foo<string>()`, `f<string>(x)`) rather than naming a pre-specialized binding.

### Sentinel refinement through destructured values {#toc-sentinel-destructure}

TypeScript narrows destructured properties of a discriminated union together: refining the sentinel binding also refines the other bindings extracted in the same destructuring.

```ts
// TypeScript:
type FormField =
  | {kind: 'text', value: string}
  | {kind: 'number', value: number};

declare const field: FormField;
const {kind, value} = field;
if (kind === 'text') {
  const s: string = value; // OK - TS narrows `value` based on `kind`
} else {
  const n: number = value;
}
```

Flow refines sentinel-tagged unions through the *original* value (`if (field.kind === 'text') { ... field.value ... }`), but each destructured binding carries its full union type independent of the others, so the same code fails:

```js flow-check
// Flow:
type FormField =
  | {kind: 'text', value: string}
  | {kind: 'number', value: number};

declare const field: FormField;
const {kind, value} = field;
if (kind === 'text') {
  const s: string = value; // ERROR: `value` keeps its full `string | number` type
}
```

The Flow rewrite is to refine through the original value rather than destructure. Alternatively, a [`match` expression or statement](#toc-match) handles matching and destructuring in one form - each arm matches the sentinel and binds `value` at its per-arm type:

```js flow-check
// Flow:
type FormField =
  | {kind: 'text', value: string}
  | {kind: 'number', value: number};

declare const field: FormField;
match (field) {
  {kind: 'text', const value} => {
    const s: string = value;
  }
  {kind: 'number', const value} => {
    const n: number = value;
  }
}
```

### User-side module augmentation {#toc-module-augmentation}

No equivalent at the source level. TypeScript users routinely re-open third-party modules from source code via `declare module 'name' { ... }` to add types. Flow's `declare module` is only used inside *libdefs* under `flow-typed/`, not from arbitrary source files.

## Coming soon {#toc-coming-soon}

The following TypeScript features have type-checking support implemented in Flow, but are not enabled by default. They are enabled in the [Flow playground](../../../try).

- Template literal types - e.g. `` `${'a' | 'b'}-${'x' | 'y'}` ``.
- Mapped type modifiers - variance removal `-readonly` and `as` key remapping.
- [Abstract classes and members](./types/classes.md#toc-abstract-classes).
- `override` on class members.
- Constructor types - `type Ctor = new (x: number) => R`.
- `satisfies` expression - validates an expression against a type without widening the inferred type.
- Inline `import()` type expression - `type A = import('./m').A`.
- `import X = require('foo')` and `export = X` - CommonJS-style import and export bindings.
- `unique symbol` and support for symbol-keyed property accesses.
- Definite assignment assertions - `let x!: T` and class fields `x!: T`, which tell Flow that something assigns the variable or field before it is read.

## Syntax convergence with TypeScript {#toc-convergence}

This table maps legacy Flow forms to their modern Flow replacements. Some rows are syntax renames; others are older utilities or features with TS-aligned equivalents. New code should use the right-hand form.

| Legacy Flow | Modern Flow (TS-aligned) |
|---|---|
| `mixed` | `unknown` |
| `$Keys<T>` | `keyof T` |
| `$ReadOnly<T>` | `Readonly<T>` |
| `$NonMaybeType<T>` | `NonNullable<T>` |
| `$ReadOnlyArray<T>` | `ReadonlyArray<T>` |
| `<T: Bound>` | `<T extends Bound>` |
| `(x: T)` cast | `x as T` |
| `{\| a: number \|}` exact | `{a: number}` (exact is the default) |
| `+foo` / `-foo` property variance | `readonly foo` / `writeonly foo` (`writeonly` is Flow-specific) |
| `+T` / `-T` type parameter variance | `out T` / `in T` |
| `%checks` predicate functions | user-defined [type guards](./types/type-guards.md) (`function isString(x: unknown): x is string`) |
| `$ObjMap<O, F>` / `$ObjMapi<O, F>` / `$TupleMap<T, F>` / `$TupleMapi<T, F>` | [mapped types](./types/mapped-types.md) (`{[K in keyof O]: ...}`) with the function body inlined |
| `$PropertyType<T, K>` / `$ElementType<T, K>` | [indexed access](./types/indexed-access.md) (`T[K]`) |
| `$Call<F, ...Args>` | [`ReturnType<F>`](./types/utilities.md) plus indexed access, or a [conditional type](./types/conditional.md) with `infer` |
| `$Diff<A, B>` / `$Rest<A, B>` | typically [`Omit<A, keyof B>`](./types/utilities.md), case by case (not always semantically identical) |

For the full picture see [Modernizing Legacy Flow Syntax](./modernizing-legacy-syntax.md).

## Config options aligned with TypeScript {#toc-shared-options}

A few Flow [`.flowconfig`](./config/index.md) `[options]` toggles correspond directly to TypeScript `compilerOptions` strictness flags - same semantics, but different defaults. In the TypeScript `strict` baseline used on this page, `useUnknownInCatchVariables` is enabled through `strict`, while `noUncheckedIndexedAccess` is not part of `strict` and stays opt-in. Flow has no `strict` options umbrella - both flags are opt-in individually and default to `false`, so porting from a TS project with `strict` enabled means turning `use_unknown_in_catch_variables` on to match.

| TypeScript option | Flow option | Description |
| --- | --- | --- |
| `noUncheckedIndexedAccess` | `no_unchecked_indexed_access` | Indexed access through an array or dictionary widens the result type with `undefined` (Flow: `void`), so reading `arr[i]` or `dict[k]` returns `T \| void` instead of `T` and forces the caller to refine before use. Tuple access with a *literal* index is unaffected in both languages. See [docs](./config/options.md#toc-no-unchecked-indexed-access) for more. |
| `useUnknownInCatchVariables` | `use_unknown_in_catch_variables` | Changes the default type of an un-annotated `catch` binding from `any` to `unknown`. The caller has to narrow the value (`instanceof Error`, `typeof e === 'string'`, …) before using it. See [docs](./config/options.md#toc-use-unknown-in-catch-variables) for more. |

## Typing external code {#toc-declaration-mechanisms}

TypeScript's `.d.ts` files cover two distinct concerns: typing third-party npm packages and typing first-party (or vendored) code that must remain plain JavaScript. Flow splits these into two mechanisms.

| TypeScript use case | Flow mechanism | Placement and resolution |
|---|---|---|
| Third-party package declarations, including `@types/*` packages and package-level `.d.ts` files. | [**Library definitions (libdefs)**](./libdefs/index.md). | Plain `.js` files in `flow-typed/` that usually name packages with `declare module 'pkg' { ... }`. |
| A sibling declaration file next to a JavaScript implementation. | [**Declaration files**](./declarations/index.md). | Colocated `.js.flow` or `.json.flow` files (for example, `Misc.js.flow` next to `Misc.js`) that shadow the implementation file. |
| Source-level module augmentation with `declare module 'pkg' { ... }` from arbitrary project files. | No source-level equivalent. | Flow's `declare module 'pkg' { ... }` form is for libdefs under `flow-typed/`, not for reopening modules from ordinary source files or colocated `.js.flow` declaration files. |

The two Flow mechanisms share much of TypeScript's ambient declaration syntax, but where you put the declarations matters. `declare class`, `declare function`, `declare const`, and related forms can describe ambient values in libdefs, declaration files, or inline declarations; `declare module 'name' { ... }` is the named-package form used by libdefs. Declaration files usually describe the colocated module's exports directly, for example with `declare export ...` or `declare module.exports`.

See [User-side module augmentation](#toc-module-augmentation) for the declaration-file pattern TypeScript supports that Flow does not.

### Declaration merging is partially supported {#toc-declaration-merging}

Flow uses the same split-namespace model as TypeScript and supports a subset of [declaration merging](./lang/declaration-merging.md). Each name independently inhabits a *value* namespace and a *type* namespace, so a single identifier can be both a value and a type without colliding: `const A = 1; interface A {}` is accepted, value-side uses of `A` resolve to the const, type-side uses resolve to the interface. Constructs usable in both namespaces (classes, enums) register once in the value namespace and the type side falls back to it.

What Flow merges:

- `interface` + `interface` - members union (compatible duplicates allowed; conflicting members error). In library definition files, `extends` lists also concatenate, call signatures overload as intersections, and type-param arity mismatches error. In regular source files, the merge is limited to members; `extends` lists and call signatures don't combine across declarations, and same-named interfaces can't both be exported from a file.
- `declare class` + `interface` - interface members fold into the class (either order).
- `function` / `declare function` + `declare namespace` - the namespace's type members fold into the function (either order), accessible as `fn.T`.
- `class` / `declare class` + `declare namespace` - the namespace's type members fold into the class (either order), accessible as `Cls.T`.

What Flow does *not* do:

- *Runtime-merging*. Only the *type* members of a `declare namespace` reliably propagate to a sibling function or class; value members are not treated as runtime properties on the host. TS-style `function` + `namespace` value-side merging where the namespace contributes runtime members is not supported.
- *Multi-block `declare module` merging.* Multiple `declare module 'name' { ... }` blocks for the same module do not union; the second is treated as an override of the first. A libdef for a given module should live in one place.
- *User-side `declare module 'name' { ... }` source-level augmentation* (see [User-side module augmentation](#toc-module-augmentation)).

### Generating declaration files {#toc-generating-declarations}

The mechanisms above are about declarations as *input*: typing code the typechecker can't otherwise see. The reverse direction is emitting declaration files *from* source. TypeScript handles this in the compiler itself: `tsc --declaration` emits a `.d.ts` alongside each compiled `.ts`, and `--emitDeclarationOnly` produces declarations without the corresponding `.js`. Flow has no equivalent built into the `flow` binary; the separate [`flow-api-translator`](https://www.npmjs.com/package/flow-api-translator) NPM package fills this gap, producing `.js.flow` or `.d.ts` files from a Flow source file.

## See also {#toc-see-also}

- [Glossary](./glossary.md) - carries a one-line TypeScript note on concepts that have one, and serves as a quick index when you only need to look up a single term.
- [Modernizing Legacy Flow Syntax](./modernizing-legacy-syntax.md) - the full reference for migrating Flow's legacy `$`-prefixed utilities and other older syntactic forms to their modern (often TS-aligned) equivalents.
