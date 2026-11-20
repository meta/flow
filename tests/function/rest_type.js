/* regression tests */

type rest_array = <T>(...xs: Array<T>) => T; // Ok, arrays can be rest params

type rest_tuple = <T>(...xs: [T]) => T; // Ok, tuples can be rest params

type rest_ro_array = <T>(...xs: ReadonlyArray<T>) => T;  // Ok

type rest_any = (...xs: any) => any; // Ok, any can be a rest param

type rest_t = <U, T extends Array<U>>(...xs: T) => U; // Ok, bounded targ can be rest

type unbound_rest_t = <T>(...xs: T) => void; // Should be error but no way to check yet :(
function test_unbound_rest(f: <T>(x: T, ...xs: T) => void) {
  f(123); // Error - number ~> array - luckily this errors
}

type string_rest_t = (...xs: string) => void; // Should be error but no way to check yet :(
function test_string_rest(f: string_rest_t) {
  f('hello'); // Error - string ~> array - luckily this errors
}

type Rest = Array<string>;
type rest_alias = (...xs: Rest) => void; // Ok

type rest_union = (...xs: [1,2] | Array<number>) => number; // OK

type rest_intersection = (...xs: { x: number, ... } & [1,2]) => number; // OK

type empty_rest = <T extends Array<unknown>>(...xs: T) => T; // OK
((f: empty_rest) => f() as empty); // Error Array ~> empty

type OneNumber = (value: number) => void;
type OneString = (value: string) => void;
type OneNumberOrString = (value: number | string) => void;

declare const acceptsNumbersOrStrings: (...args: Array<number | string>) => void;
acceptsNumbersOrStrings as (value: number) => void;
acceptsNumbersOrStrings as OneNumber;
acceptsNumbersOrStrings as (value: string) => void;
acceptsNumbersOrStrings as OneString;

declare const acceptsOnlyNumbers: (...args: Array<number>) => void;
acceptsOnlyNumbers as (value: number | string) => void; // Error: string ~> number
acceptsOnlyNumbers as OneNumberOrString; // Error: string ~> number

// A rest param of a tuple type of fixed length is a param per element.
declare const noArgs: (...args: []) => void;
noArgs as (done: () => void) => void; // OK
noArgs(1); // Error: too many args
declare const oneString: (...args: [string]) => void;
oneString as (s: string, done: () => void) => void; // OK
oneString as (s: number) => void; // Error: number ~> string
declare const twoArgs: (...args: [string, number]) => void;
twoArgs as (s: string) => void; // Error: too few args
declare const restElement: (...args: [string, ...Array<number>]) => void;
restElement as (s: string, n: number, m: string) => void; // Error: string ~> number

declare function it(name: string, fn: ((done: () => void) => void) | (() => PromiseLike<unknown>)): void;
function check(text: string, expected: boolean): void {}
it('a', check.bind(null, '', false)); // OK
