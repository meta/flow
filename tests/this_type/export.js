export class A1 {
  foo(): this { return this; }
  bar(): this { return this; }
}

export class A2<X> {
  foo(): this { return this; }
  bar(): this { return this; }
  qux(x: X): X { return x; }
}

export class A3<X> extends A2<X> {}

declare export class A4 {
  foo(): this;
}

// Interface variants — exercise the type-sig path for `this` in interfaces.
export interface IBuilder {
  clone(): this;
  init(): Promise<this>;
  on(event: string, listener: () => void): this;
}

export interface IBad {
  xs: Array<this>; // ERROR: invariant position
}

// Generic interface with this — exercises tparams + this through type-sig
export interface IBox<T> {
  map<U>(f: (x: T) => U): IBox<U>;
  clone(): this;
  value(): T;
}

// Interface extending another exported interface
export interface ICloneable { clone(): this; }
export interface IExtended extends ICloneable {
  extra: string;
}

// Type alias of an interface, used as the parent of another interface.
export type ICloneableAlias = ICloneable;
export interface IExtendedViaAlias extends ICloneableAlias {
  extra: string;
}

// Generic child extending generic parent — round-trips through the
// type-sig path with both type-arg substitution and `this` rebinding.
export interface IChildBox<T> extends IBox<T> {
  tag: string;
}

export interface IHolder<T> { value: T; }
export interface IBadExtends extends IHolder<this> {} // ERROR

// Parent is a type alias whose RHS is an INTERSECTION of interfaces.
export interface IA { a(): number }
export interface IB { b(): string }
export type IAB = IA & IB;
export interface IChildOfIntersection extends IAB {
  c(): boolean;
}

export default { A4 };
