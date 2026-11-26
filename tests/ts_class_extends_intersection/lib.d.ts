export declare class A {
  a: number;
  static s: string;
}

// What tsc emits for a class extending `Object.assign(A, {extra: 1})`.
declare const Assigned: typeof A & { extra: number };
export declare class FromAssigned extends Assigned {
  constructor();
}

// What tsc emits for a class extending a mixin applied to `A`: the mixin's
// construct signature takes `...args: any[]`, so what it returns is intersected
// with `A`, and `A`'s constructor constructs the class.
export interface M {
  m: boolean;
}
declare const FromMixin_base: {
  new (...args: any[]): M;
  s2: string;
} & typeof A;
export declare class FromMixin extends FromMixin_base {}

// tsc emits the instance a mixin returns as an object type, which is mixed in
// too.
declare const FromObjectMixin_base: {
  new (...args: any[]): { o: string };
} & typeof A;
export declare class FromObjectMixin extends FromObjectMixin_base {}

// Without a mixin, the first construct signature gives the instance, and its
// member's signatures construct the class.
export interface X {
  x: number;
}
declare const Overloads: { new (): X } & { new (s: string): X };
export declare class FromOverloads extends Overloads {}

// A generic class is specialized as when a class extends it directly.
export declare class G<T = string> {
  g: T;
}
declare const Generic: typeof G & { extra: number };
export declare class FromGeneric extends Generic {}

// A mixin whose construct signature returns a class instance: the class it is
// applied to still constructs.
export declare class A5 {
  a: number;
  constructor(x: number);
}
export declare class MI {
  mi: boolean;
}
interface MIC {
  new (...args: any[]): MI;
}
declare const MixinOverClass: MIC & typeof A5;
export declare class FromMixinOverClass extends MixinOverClass {}

// A class extending an intersection is a value with a construct signature.
export declare function make<T>(c: new () => T): T;
export declare class Creates extends FromMixin_base {
  static create<T>(this: new () => T): T;
}

// A class whose constructor is a mixin's is a mixin too.
export declare class CM {
  cm: boolean;
  constructor(...args: any[]);
}
declare const ClassMixin: typeof CM & typeof A5;
export declare class FromClassMixin extends ClassMixin {}

// The parts of a base come in the order the members are written, as
// TypeScript intersects them, so the earlier part's `m` wins.
export interface MOverrides {
  m(): string;
}
interface MOverridesCtor {
  new (...args: any[]): MOverrides;
}
export declare class AOverridden {
  m(): number;
  constructor(x: number);
}
declare const MixinThenClass: MOverridesCtor & typeof AOverridden;
declare const ClassThenMixin: typeof AOverridden & MOverridesCtor;
export declare class FromMixinThenClass extends MixinThenClass {}
export declare class FromClassThenMixin extends ClassThenMixin {}
