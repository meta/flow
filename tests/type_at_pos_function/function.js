// @flow

const a = {
  bar(): void {}
//^
};

const b = {
  bar: function (): void {}
//^
};

const c = {
  m<T>(x: T): T { return x; }
//^
};

const d = {
  m: function<T>(x: T): T { return x; }
//^
};

// A rest parameter is not a `RegularParam`, so it is framed by the binding it
// resolves to rather than by the parameter hook.
function rest(...xs: Array<string>): void {}
//               ^

// A `this` parameter binds no identifier, but hovers as the parameter.
function with_this(this: {z: number}, a: number): void {}
//                 ^
//                        ^
