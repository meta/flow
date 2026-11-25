// Basic constructor type
type Ctor = new () => {x: number};
declare const c: Ctor;
const inst = new c();
inst.x as number; // OK

// Constructor type with params
type CtorWithArgs = new (x: number) => {v: string};
declare const c2: CtorWithArgs;
new c2(1); // OK
new c2("bad"); // ERROR: string incompatible with number

// Assignability with interface construct signature
interface ICtor { new (): {x: number} }
declare const ic: ICtor;
ic as Ctor; // should be compatible

// Non-constructable interface should still error
interface NotConstructable {
    bar(): number;
}
declare const nc: NotConstructable;
new nc(); // ERROR: invalid-constructor

// =====================================================================
// Main use-case tests: passing classes into construct-sig parameters
// (factories, DI containers). Note: class instances are not subtypes of
// object types in Flow — return positions that should accept class
// instances must use `interface`, not `{...}`.
// =====================================================================

// 1. Generic factory. T is inferred from the class argument; `Class<A1>`
//    flows into `new () => T` with T = A1.
function create<T>(C: new () => T): T { return new C(); }
class A1 { a: number = 1 }
class B1 { b: string = "x" }
create(A1).a as number;   // OK — T inferred as A1
create(B1).b as string;   // OK — T inferred as B1
create(A1).b;             // ERROR — nominal: A1 has no `b`

// 2. Factory with args, threading tuple inference through `(...args: Args)`.
function instantiate<T, Args extends ReadonlyArray<unknown>>(
    C: new (...args: Args) => T, ...args: Args
): T { return new C(...args); }
class CtorArgs { constructor(x: number, y: string) {} }
instantiate(CtorArgs, 1, "s");     // OK
instantiate(CtorArgs, "bad", "s"); // ERROR — number expected

// 3. Nominal-identity preservation through a construct-sig parameter.
//    Flow-specific: same-shape classes stay distinct on the construct side.
class Same1 { v: number = 0 }
class Same2 { v: number = 0 }
declare function makeOne<T>(C: new () => T): T;
const same1ok: Same1 = makeOne(Same1);    // OK
const same1bad: Same1 = makeOne(Same2);   // ERROR — Same2 is not Same1

// 4. The Class<T> / construct-sig bridge goes BOTH ways: reverse
//    (new () => T -> Class<T>) is checked on the construct signature's return
//    type, which is what TypeScript compares for `typeof T`. This was once
//    pinned as an error because `Class<T>` is the full statics surface, so
//    accepting a value with no statics loses static-access capability. That
//    hazard is real and stays unchecked: `Class<T>`'s statics are an object
//    type whose prototype identity no interface can satisfy, so checking
//    against them reports prototype mismatches instead of missing statics.
//    `Class<T> -> Class<T>` ignores statics too, so the gap is not new here.
declare const ctorVal: new () => A1;
const ctorAsClass: Class<A1> = ctorVal; // OK — the construct signature returns A1

// 5. Array of constructors — DI / plugin pattern. Return is an interface
//    (not an object type) because class instances satisfy interfaces, not
//    object types.
interface ITagged { tag: string }
const ctors: Array<new () => ITagged> = [
    class { tag: string = "x" },
    class { tag: string = "y" },
];
ctors.map(C => new C().tag as string);

// 6. Required ctor args vs zero-arg slot — arity mismatch on params side.
class NeedsArg { constructor(x: number) {} }
const fNeeds: new () => NeedsArg = NeedsArg;  // ERROR — arity mismatch

// 7. Asymmetry pin: a construct sig returning a class-instance type rejects a
//    value whose ctor returns an object type of the same shape. TS accepts
//    both directions; Flow accepts class -> object, not object -> class.
class C8 { v: number = 0 }
declare const ctorObj: new () => {v: number};
const ctorC8: new () => C8 = ctorObj;  // ERROR — {v: number} is not C8 nominally

// 8. `Readonly` keeps an interface's `constructor` method with its own props.
declare const ReadonlyCtor: Class<Readonly<interface {constructor(x: number): void}>>;
ReadonlyCtor as new (x: number) => unknown; // OK
