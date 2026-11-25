// We make conditional types where we cannot decide which branch to take uninhabitable.
function generic_in_check_type<T>(x: T) : [T] extends [readonly _K:string] ? boolean : number {
  if (typeof x === 'string') {
    return true; // error
  } else {
    return 1; // error
  }
}

generic_in_check_type('') as boolean; // ok
generic_in_check_type(1) as number; // ok
generic_in_check_type('') as empty; // error
generic_in_check_type(1) as empty; // error

function generic_in_extends_type<T>(x: T): [string] extends [T] ? boolean : number {
  if (typeof x === 'string') {
    return true; // error
  } else {
    return 1; // error
  }
}

declare const s: string;
declare const n: number;

generic_in_extends_type(s) as boolean; // ok
generic_in_extends_type(n) as number; // ok
generic_in_extends_type(s) as empty; // error
generic_in_extends_type(n) as empty; // error

function assign_from_generic_conditional_type<T>(x: string extends T ? boolean : number): void {
  const y: string = x; // error
}
assign_from_generic_conditional_type<number>(0); // ok

function generic_conditional_type_subtyping<T, S>(
  x: string extends T ? boolean : number,
  y: string extends S ? boolean : number,
  z: string extends T ? boolean : number,
) {
  x = y; // expected error
  x = z; // ok
}

function definitely_assignable_choose_true_branch<T>(x: T): Array<T> extends ReadonlyArray<infer X> ? X : number {
  return x; // ok
}

definitely_assignable_choose_true_branch('') as string; // ok
definitely_assignable_choose_true_branch('') as number; // error: string ~> number

function definitely_not_assignable_choose_false_branch<T>(x: T): Set<T> extends Array<infer X> ? X : string {
  return ''; // ok
}

definitely_not_assignable_choose_false_branch('') as string; // ok
definitely_not_assignable_choose_false_branch('') as number; // error: string ~> number

function union_as_upper_bound() {
  type ConditionalWithoutInfer<T> =
    T extends string
    ? 'string'
    : 'other';

  declare function getConditionalWithoutInfer<T>(x: T): ConditionalWithoutInfer<T>;
  <T>(t:T): 'string' | 'other' =>  getConditionalWithoutInfer(t); // ok
  <T extends number>(t:T): 'string' | 'other' => getConditionalWithoutInfer(t); // ok
  <T extends string>(t:T): 'string' | 'other' => getConditionalWithoutInfer(t); // ok

  type ConditionalWithInfer<T> =
    T extends [infer X]
    ? X
    : 'other';

  declare function getConditionalWithInfer<T>(v: T): ConditionalWithInfer<T>;
  <T>(t:T): T[0] | 'other' => getConditionalWithInfer(t); // error
}

// In the true branch, the check type is known to satisfy the extends type.
type Upper<T> = T extends string ? Uppercase<T> : empty; // ok
type UpperOfTuple<T> = [T] extends [string] ? Uppercase<T> : empty; // ok
type UpperOfFalseBranch<T> = T extends string ? empty : Uppercase<T>; // error
type UpperOfAny<T> = T extends any ? Uppercase<T> : empty; // error
type UpperOfOptional<T> = [a?: T] extends [b?: string] ? Uppercase<T> : empty; // error

// So it is where a distributive one is evaluated over a type parameter.
type UpperOfDistributed<T> = Uppercase<T extends string ? T : 'a'>; // ok
type UpperOfDistributedFalseBranch<T> = Uppercase<T extends string ? 'a' : T>; // error
type StringOf<T> = T extends string ? T : 'a';
type UpperOfStringOf<U> = Uppercase<StringOf<U>>; // ok
type Names = {a: 'x', b: 'y'};
type UpperOfName<K> = Uppercase<K extends keyof Names ? Names[K] : 'z'>; // ok
type Untyped = any;
type OfUntyped<T> = T extends Untyped ? T : empty;
function decidedByAnAliasOfAny<T>(x: OfUntyped<T>): string {
  return x; // error: `T` is `unknown`
}
type ExtractChain<T> = Extract<Extract<Extract<Extract<Extract<Extract<Extract<Extract<
  Extract<Extract<Extract<Extract<Extract<Extract<Extract<Extract<T,
  string>, string>, string>, string>, string>, string>, string>, string>,
  string>, string>, string>, string>, string>, string>, string>, string>;
function extractChain<T>(x: ExtractChain<T>): string {
  return x; // ok
}
// Only the type parameter that the conditional type checks is narrowed.
type IfString<X, Then> = X extends string ? Then : string;
function narrowsOnlyTheCheckType<T>(v: IfString<T, T>): string {
  return v; // error
}
function narrowsOnlyTheCheckTypeInArray<T>(v: IfString<T, ReadonlyArray<T>>): ReadonlyArray<string> | string {
  return v; // error
}
declare class StringIfString<T> {
  m<U>(u: U): IfString<U, T>;
}
function narrowsOnlyTheCheckTypeOfTheSameName<T>(b: StringIfString<T>) {
  function inner<T>(x: T): T | string {
    return b.m(x); // error
  }
}
// The narrowed type parameter is not the type parameter where it must be the same type.
type ArrayOfString<X> = X extends string ? Array<X> : null;
function narrowedInArray<T>(x: ArrayOfString<T>): Array<T> | null {
  return x; // error, though TypeScript accepts it
}
declare function arrayOfString<X>(x: X): X extends string ? Array<X> : null;
function narrowedInReturnedArray<T>(t: T): Array<T> | null {
  return arrayOfString(t); // error, though TypeScript accepts it
}

// A conditional type decides `keyof T` key by key over the keys of the bound of `T`, and as a
// whole when the bound is not an object.
type Excl<T, U> = T extends U ? empty : T;
type Pck<T, K extends keyof T> = {[P in K]: T[P]};
type TSOmit<T, K extends string> = Pck<T, Excl<keyof T, K>>; // ok
({b: ''}) as TSOmit<{a: number, b: string}, 'a'>; // ok
({a: 1, b: ''}) as TSOmit<{a: number, b: string}, 'a'>; // error: a is extra
function keysOfUnboundedTypeParam<T, K>(
  x: Excl<keyof T, K>,
  y: keyof T extends K ? empty : keyof T,
): Array<string | number | symbol> {
  return [x, y]; // ok
}
function keysOfUnboundedTypeParamNotDecided<T>(): keyof T extends string ? 1 : 2 {
  return 1; // error: `keyof T` may be a number or a symbol
}
function keysOfBoundedTypeParam<T extends {a: number, b: string}>(
  x: Excl<keyof T, 'a'>,
  o: TSOmit<T, 'a'>,
): 'b' {
  o.b as string; // ok
  return x; // ok
}
function keysOfUnboundedTypeParamInfer<T>(): keyof T extends infer U ? U : empty {
  return 'zzz'; // error: `U` is `keyof T`
}
function keysOfUnboundedTypeParamInUnion<T>(k: keyof T): keyof T | 'c' {
  return k; // ok
}
function keysOfUnboundedTypeParamInMaybe<T>(k: keyof T): ?keyof T {
  return k; // ok
}
function keysOfUnboundedTypeParamOrKey<T>(k: keyof T | 'c'): 'a' {
  return k; // error x4: neither the string, number or symbol of `keyof T` nor 'c' is 'a'
}
function keysOfUnboundedTypeParamInferUnderMaybe<T>(): keyof T extends ?infer U ? U : empty {
  return 'zzz'; // error: `U` is `keyof T`
}
function omitOfUnboundedTypeParam<T>(o: TSOmit<T, 'a'>): {...} {
  return o; // ok
}
