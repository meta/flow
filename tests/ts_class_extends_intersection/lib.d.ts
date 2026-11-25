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
