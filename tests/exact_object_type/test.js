declare const exact: {a: number};
declare const exactEmpty: {};
declare const implicitExact: {a: number};
declare const inexact: {a: number, ...};
declare const indexed: {[key: string]: number};
declare class C {
  a: number;
}
declare const instance: C;

// Every exact object type is an `ExactObjectType`
exact as ExactObjectType; // ok
exactEmpty as ExactObjectType; // ok
implicitExact as ExactObjectType; // ok
({a: 1}) as ExactObjectType; // ok
inexact as ExactObjectType; // error
indexed as ExactObjectType; // error
instance as ExactObjectType; // error
1 as ExactObjectType; // error

declare const union: {a: number} | {b: string};
union as ExactObjectType; // ok
declare const maybe: ?{a: number};
maybe as ExactObjectType; // error

// `ExactObjectType` is an inexact object type
declare const x: ExactObjectType;
x as ExactObjectType; // ok
x as {...}; // ok
x as unknown; // ok
x as {}; // error
x as {a?: number, ...}; // error
x.a; // error

// Spreading keeps it exact
({...x}) as ExactObjectType; // ok
({...x, ...exact}) as ExactObjectType; // ok
({...x, ...inexact}) as ExactObjectType; // error
({...x}) as {}; // error
const withA = {...x, a: 1};
withA as ExactObjectType; // ok
withA.a as number; // ok
let reassigned = {...x, a: 1};
reassigned = {...x}; // error

// Refinement
declare var y: null | ExactObjectType;
declare function foo(x: unknown): x is {p: 1};
if (foo(y)) {
  y as ExactObjectType; // ok
  y as {p: 1}; // ok
}

// As a bound
function f<T extends ExactObjectType>(t: T): T {
  t as {...}; // ok
  t as {}; // error
  ({...t}) as ExactObjectType; // ok
  return t;
}
f(exact); // ok
f(inexact); // error

type Arity = ExactObjectType<number>; // error

// Through a signature
import {g} from './exported';
g(exact) as ExactObjectType; // ok
g(inexact); // error
g(exact) as {}; // error
