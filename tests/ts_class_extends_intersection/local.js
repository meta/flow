import type {OpaqueClass} from './opaque';

// The same rules for a class checked in this file.
declare class A {
  a: number;
  static s: string;
}
interface M {
  m: boolean;
}
interface MixinCtor {
  new (...args: Array<any>): M;
  s2: string;
}
declare const Mixed: MixinCtor & typeof A;
class FromMixin extends Mixed {}
const mixed = new FromMixin();
mixed.m as boolean; // ok
mixed.a as empty; // error: number ~> empty
FromMixin.s2 as string; // ok
FromMixin.s as empty; // error: string ~> empty

// An intersection nested through an alias contributes its members.
type WithMore = typeof A & {more: string};
declare const Nested: WithMore & {extra: number};
class FromNested extends Nested {}
new FromNested().a as empty; // error: number ~> empty
FromNested.extra as number; // ok

declare class G<T = string> {
  g: T;
}
declare const Generic: typeof G & {extra: number};
class FromGeneric extends Generic {}
new FromGeneric().g as empty; // error: string ~> empty

declare const NotCtor: {a: number} & {b: string};
class FromNotCtor extends NotCtor {} // error: no construct signature

// A union is a constructor when all of its alternatives are.
declare class B {
  b: number;
}
declare const UnionWithObject: (Class<A> | {y: string}) & {x: number};
class FromUnionWithObject extends UnionWithObject {} // error (tsc: TS2507)
declare const UnionOfClasses: (Class<A> | Class<B>) & {x: number};
class FromUnionOfClasses extends UnionOfClasses {} // ok (tsc: ok)

// A mixin returning an object type is not mixed in: `H` is the base.
declare class H {
  h: number;
}
interface ObjectMixin {
  new (...args: Array<any>): {readonly m: number};
}
declare const HWithObjectMixin: Class<H> & ObjectMixin;
class FromObjectMixin extends HWithObjectMixin {} // ok (tsc: ok)
new FromObjectMixin().h as number; // ok

// Only the instance's own class constructs (tsc: TS2510 on the declaration).
declare class A4 {
  a: number;
  constructor(): void;
}
declare class B4 {
  b: number;
  constructor(s: string): void;
}
declare const DifferentReturns: Class<A4> & Class<B4>;
class FromDifferentReturns extends DifferentReturns {}
new FromDifferentReturns('s'); // error: no argument expected

// With the mixin first, the class's constructor still constructs.
declare class A5 {
  a: number;
  constructor(x: number): void;
}
declare const MixinFirst: MixinCtor & Class<A5>;
class FromMixinFirst extends MixinFirst {}
new FromMixinFirst('s'); // error: string ~> number (tsc: TS2345)
new FromMixinFirst(1).m as boolean; // ok
new FromMixinFirst(1).a as number; // ok

// A mixin applied to a class whose constructor takes an argument.
declare class MIClass {
  mi: boolean;
}
interface MIClassCtor {
  new (...args: Array<any>): MIClass;
}
declare const MixinOverClass: MIClassCtor & Class<A5>;
class FromMixinOverClass extends MixinOverClass {}
new FromMixinOverClass(1).mi as boolean; // ok
new FromMixinOverClass(); // error: `A5`'s constructor takes `x` (tsc: TS2554)

// An opaque or generic member stands for its bound, as when a class extends it
// alone.
declare const Opaque: OpaqueClass & {x: number};
class FromOpaque extends Opaque {}
new FromOpaque().a as empty; // error: number ~> empty
function fromGeneric<T extends OpaqueClass>(b: T & {x: number}) {
  class FromGenericMember extends b {}
  new FromGenericMember().a as empty; // error: number ~> empty
}

// As in TypeScript, a union with `any` makes the intersection `any`.
declare const UnionWithAny: (Class<A> | any) & {x: number};
class FromUnionWithAny extends UnionWithAny {} // ok

// An intersection without construct signatures is extended as other uses of an
// intersection are.
declare const MaybeClass: ?Class<A> & {x: number};
class FromMaybeClass extends MaybeClass {} // error
