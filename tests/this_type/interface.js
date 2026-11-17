// Fluent builder pattern — this in output position, used through interface
// receiver returns the interface type.
interface Builder {
  setName(name: string): this;
  clone(): this;
}

declare const b: Builder;
b.clone() as Builder; // OK
b.setName('x') as Builder; // OK

// Event emitter pattern
interface EventEmitter {
  on(event: string, listener: (...args: ReadonlyArray<unknown>) => void): this;
}
declare const ee: EventEmitter;
ee.on('x', (..._args) => {}) as EventEmitter; // OK — fluent `this` rebound to EventEmitter

// Class implementing interface with `this`: the implements check substitutes
// the interface's `this` with the implementing class's `this`, so a class
// method declared as `(): this` satisfies an interface method `(): this`.
class MyBuilder implements Builder {
  name: string = '';
  setName(name: string): this { this.name = name; return this; }
  clone(): this { return this; }
}

// this in covariant generic — should work
interface AsyncInit {
  init(): Promise<this>;
}
declare const ai: AsyncInit;
ai.init() as Promise<AsyncInit>; // OK — `this` rebound inside Promise<>

// Generic interface with this
interface Box<T> {
  map<U>(f: (x: T) => U): Box<U>;
  clone(): this;
  value(): T;
}

declare const box: Box<number>;
box.value() as number; // OK
box.clone() as Box<number>; // OK — `this` rebound to Box<number>

// Interface extending another interface that uses this.
interface Cloneable {
  clone(): this;
}
interface MyBox extends Cloneable {
  contents: number;
}
declare const mb: MyBox;
mb.clone() as Cloneable; // OK
mb.clone() as MyBox; // OK — inherited `this` rebound to MyBox

// Type alias as parent: `interface MyBox extends Alias` where
// `type Alias = Cloneable`.
//
// KNOWN LIMITATION: The line below currently errors. Type-alias parents
// don't preserve the polymorphic `this` shape through the alias's
// `mk_type_reference` unwrap. See `import.js:61` for the cross-module
// equivalent of this gap.
type CloneableAlias = Cloneable;
interface MyBoxViaAlias extends CloneableAlias {
  contents: number;
}
declare const mba: MyBoxViaAlias;
mba.clone() as Cloneable; // OK
mba.clone() as MyBoxViaAlias; // OK — inherited `this` rebound through the alias (KNOWN LIMITATION: errors)

// Parameterized parent: `Box<number>` is specialized first, then `this`
// is rebound to the child.
interface NumBox extends Box<number> {
  label: string;
}
declare const nb: NumBox;
nb.clone() as NumBox; // OK
nb.value() as number; // OK

// Generic child extending generic parent: `this` rebinding composes with
// type-arg substitution.
interface ChildBox<T> extends Box<T> {
  tag: string;
}
declare const cb: ChildBox<number>;
cb.clone() as ChildBox<number>; // OK
cb.value() as number; // OK

// Transitive: `A extends B extends C` where C declares `(): this`.
interface Grandchild extends MyBox {
  meta: boolean;
}
declare const gc: Grandchild;
gc.clone() as Grandchild; // OK — `this` resolves all the way to Grandchild

// `class C implements Child` where `Child extends Base` and `Base.clone(): this`
class GoodCloneChild implements MyBox {
  contents: number = 0;
  clone(): this { return this; } // OK — `this` from class matches inherited `this`
}
class BadCloneChild implements MyBox {
  contents: number = 0;
  clone(): Cloneable { return this; } // ERROR: returning `Cloneable` doesn't satisfy `(): this`
}

// Returning the implementing class itself doesn't satisfy `(): this`
class BadCloneSelf implements Cloneable {
  clone(): BadCloneSelf { return this; } // ERROR
}

// Same thing through inherited `this` from a parent interface
class BadInherited implements MyBox {
  contents: number = 0;
  clone(): BadInherited { return this; } // ERROR
}

// Subclass-leak case: without the polymorphic-`this` substitution this
// would typecheck, and `Subclass#clone()` would return `LeakBase` instead
// of `Subclass` — breaking the polymorphic-this contract for subclasses.
// Now caught at the implements site.
class LeakBase implements Cloneable {
  clone(): LeakBase { return this; } // ERROR
}
class LeakSubclass extends LeakBase {
  extra: string = '';
}

// this inside callable signature
interface Callable {
  (x: number): this;
  reset(): this;
}
declare const cl: Callable;
cl(0) as Callable; // OK — call signature returns `this`
cl.reset() as Callable; // OK

// this inside indexer / dict — covariant value position should work
interface Dict {
  readonly [key: string]: this;
}
declare const dict: Dict;
dict['k'] as Dict; // OK — indexer value rebound to Dict

// this in extends targs — should NOT resolve (no this in scope at extends).
interface Holder<T> { value: T }
interface BadExtends extends Holder<this> { } // ERROR: this not in scope here

// `this` inside a nested object type within an interface body is NOT in scope,
// matching Flow's behavior for the same shape in classes. An inline interface
// introduces its own `this` binding, so `OuterNestedInterface` is valid while
// the other nested object types below should still error.
interface OuterNestedObject {
  readonly nested: { clone(): this }; // ERROR: `this` not in scope inside nested type
}
interface OuterNestedInterface {
  readonly nested: interface { clone(): this }; // OK: this belongs to the inner inline interface
}
interface OuterWithThisAndNestedObject {
  self(): this; // OK — top-level `this` is fine
  readonly nested: { clone(): this }; // ERROR
}
interface OuterWithDeepNestedThis {
  self(): this; // OK
  readonly list: ReadonlyArray<{ readonly item: this }>; // ERROR
  factory(): { make(): this }; // ERROR
}
interface OuterDoublyNested {
  self(): this; // OK
  readonly doubly: { readonly a: { readonly b: this } }; // ERROR
}

// Inherited members from each branch of the intersection must
// still flow through to the extending interface.
interface IA { a(): number }
interface IB { b(): string }
type IAB = IA & IB;
interface ChildOfIntersection extends IAB {
  c(): boolean;
}
declare const ci: ChildOfIntersection;
ci.a() as number;  // OK — inherited from IA via the intersection alias
ci.b() as string;  // OK — inherited from IB via the intersection alias
ci.c() as boolean; // OK — own member

// A property that doesn't exist on the child. Accessing it on
// `ci` should error at the call site
ci.zzz; // ERROR: prop-missing at this line

// `this` in interface method type-parameter constraints/defaults — Flow now
// resolves `this` in this position, matching how class methods already do.
// The `[illegal-this]` error must NOT fire for these uses. Variance errors
// (`[incompatible-variance]`) are a separate, pre-existing subsystem and
// will fire as expected because tparam bounds/defaults are checked at
// neutral polarity by `check_polarity_typeparam`. The `class Bar` block
// below uses the same shapes to confirm parity with the class side.
interface WithMethodTparamThis {
  refine<Ch extends (arg: this) => unknown>(check: Ch): void; // [incompatible-variance], not [illegal-this]
  factory<T = this>(): T;                                   // [incompatible-variance], not [illegal-this]
}

class WithClassMethodTparamThis {
  refine<Ch extends (arg: this) => unknown>(check: Ch): void {} // same: [incompatible-variance]
  factory<T = this>(): T { return (this: $FlowFixMe); }       // same: [incompatible-variance]
}

// `this` in tparam constraints OUTSIDE an interface/class body must STILL
// error — `this` is not in scope for free-function or type-alias tparams.
function freeFn<T extends this>(x: T): void {} // ERROR: [illegal-this]
type AliasOfThis<T extends this> = T;          // ERROR: [illegal-this]

// `class C implements PolyInterface` without supplying type args must report
// `[missing-type-arg]`, not silently instantiate. The implements check arm
// for [ClassLikePoly] dispatches on whether targs are present: with [Some]
// it wraps in [this_typeapp]; with [None] it routes through [mk_instance]
// to fire the arity error (mirrors the parallel arm in [supertype]).
interface IPolyArity<T> {
  value(): T;
  refresh(): this;
}
class CMissingArity implements IPolyArity { // ERROR: [missing-type-arg]
  value(): number { return 0; }
  refresh(): this { return this; }
}
