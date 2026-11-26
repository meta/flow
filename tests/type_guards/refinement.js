function basic(
  fn: (x: unknown) => x is number,
  y: unknown,
) {
  if (fn(y)) {
    y as number; // okay
    y as string; // error number ~> string
  }
}

function nonMaybe(
  fn: <T>(x: ?T) => x is T,
  x1: string,
  x2: string | null,
  x3: string | undefined,
  x4: ?string,
) {
  if (fn(x1)) {
    x1 as string;
    x1 as number; // error string ~> number
  }
  if (fn(x2)) {
    x2 as string;
    x2 as number; // error string ~> number
  }
  if (fn(x3)) {
    x3 as string;
    x3 as number; // error string ~> number
  }
  if (fn(x4)) {
    x4 as string;
    x4 as number; // error string ~> number
  }
}

function array_filter() {

  declare class ReadOnlyArray_<out T> {
    filter(callbackfn: typeof Boolean): Array<NonNullable<T>>;
    filter<This, S extends T>(predicate: (this: This, value: T, index: number, array: ReadonlyArray<T>) => value is S, thisArg?: This): Array<S>;
    filter<This>(callbackfn: (this : This, value: T, index: number, array: ReadonlyArray<T>) => unknown, thisArg : This): Array<T>;
  }

  declare const arr: ReadOnlyArray_<unknown>;

  const arr1: Array<number> = arr.filter((x: unknown): x is number => { return typeof x === "number"; });
  const arr2: Array<string> = arr.filter((x: unknown): x is number => { return typeof x === "number"; }); // error
  const arr3: Array<string> = arr.filter((x: unknown) => { return typeof x === "number"; }); // error no refinement
}


function sentinel_simple() {
  type A = { tag: 'A', foo: number };
  type B = { tag: 'B', foo: string };

  declare const x: A | B;

  declare function sentinel_A(x: unknown): x is {tag: 'A', ...};
  if (sentinel_A(x)) {
      x as A; // okay A ~> A
      x as B; // error A ~> B
  }

  declare function invalid_sentinel_val(x: unknown): x is {tag: 'C', ...};
  if (invalid_sentinel_val(x)) {
      x as A; // okay empty ~> A
      x as B; // okay empty ~> B
  }

  declare function invalid_sentinel_key(x: unknown): x is {tag_: 'A', ...};
  if (invalid_sentinel_key(x)) {
      x as A; // error B & {tag_: 'A',...} ~> A
      x as B; // error A & {tag_: 'A',...} ~> B
  }
}

function sentinel_readonly() {
  type A = Readonly<{ tag: 'A', foo: number }>;
  type B = Readonly<{ tag: 'B', foo: string }>;

  declare const x: A | B;

  declare function sentinel_A(x: unknown): x is Readonly<{tag: 'A', ...}>;
  if (sentinel_A(x)) {
      x as A; // okay A ~> A
      x as B; // error A ~> B
  }

  declare function invalid_sentinel_val(x: unknown): x is Readonly<{tag: 'C', ...}>;
  if (invalid_sentinel_val(x)) {
      x as A; // okay empty ~> A
      x as B; // okay empty ~> B
  }

  declare function invalid_sentinel_key(x: unknown): x is Readonly<{tag_: 'A', ...}>;
  if (invalid_sentinel_key(x)) {
      x as A; // error B & {+tag_: 'A',...} ~> A
      x as B; // error A & {+tag_: 'A',...} ~> B
  }
}

function sentinel_multi_tag() {
  type A = Readonly<{ tag1: 'A', tag2: 1, foo: number }>;
  type B = Readonly<{ tag1: 'B', tag2: 1, foo: string }>;

  declare const x: A | B;

  declare function unknown_sentinel(x: unknown): x is Readonly<{tag: 'C', ...}>;
  if (unknown_sentinel(x)) {
      x as A; // error B & {+tag: 'C', ...} ~> A
      x as B; // error A & {+tag: 'C', ...} ~> B
  }

  declare function sentinel_A_1(x: unknown): x is Readonly<{ tag1: 'A', tag2: 1, ...}>;
  if (sentinel_A_1(x)) {
      x as A; // okay A ~> A
      x as B; // error A ~> B
  }

  declare function sentinel_1(x: unknown): x is Readonly<{ tag2: 1, ...}>;
  if (sentinel_1(x)) {
      x as A; // error B ~> A
      x as B; // error A ~> B
  }
}


function negation1() {
  declare function isNumber(x: unknown): x is number;
  declare function isString(x: unknown): x is string;

  declare const x: number | string;

  if (isNumber(x)) {
    x as number; // okay
    x as string; // error number ~> string
  } else {
    x as string; // okay
    x as number; // error string ~> number
  }

  if (!isNumber(x)) {
    x as string; // okay
    x as number; // error string ~> number
  } else {
    x as string; // error string ~> number
    x as number; // okay
  }

  if (isNumber(x) || isString(x)) {
    x as number; // error string ~> number
    x as string; // error number ~> string
  } else {
    x as string; // okay empty ~> string
    x as number; // okay empty ~> number
  }
}

function any_input() {
  declare function isNotMaybe<T>(value: ?T): value is T;

  declare const maybeAny: ?any;

  if (isNotMaybe(maybeAny)) {
    maybeAny as empty; // okay, we're filtering null and undefined
  }
}

function any_filter() {
  declare function isAny(value: unknown): value is any;

  declare const maybeNumber: number;

  if (isAny(maybeNumber)) {
    maybeNumber as empty; // error number ~> empty
  }
}

function refine_any_to_type() {
  declare function isNumber(value: unknown): value is number;

  declare const anyVal: any;

  if (isNumber(anyVal)) {
    anyVal as empty; // error number ~> empty
  }
}

function opaque_types() {
  declare function isTruthy<A>(value: ?A): value is A;

  declare class $FbtResultBase {}
  declare opaque type FbtString: string;
  declare type FbtElement = $FbtResultBase;
  declare type FbtWithoutString = FbtString | FbtElement;
  declare type Fbt = string | FbtWithoutString;

  declare const x: undefined | Fbt;

  if (isTruthy(x)) {
    x as Fbt; // okay
    x as empty; // error Fbt ~> empty
  }
}

import type {OpaqueOrString} from './opaque_exports';

function opaque_types_imported() {
  declare function isTruthy<A>(value: ?A): value is A;

  declare const x: undefined | OpaqueOrString;

  if (isTruthy(x)) {
    x as OpaqueOrString; // okay
  }
}

function union_with_any() {
  declare function isTruthy<A>(value: ?A): value is A;

  declare const x: undefined | string | any;

  if (isTruthy(x)) {
    x as string; // okay, type guard filters out undefined
    x as empty; // error string ~> empty
  }
}

function one_sided(
  value: number | string,
  fn: (x: unknown) => implies x is number,
) {
  if (fn(value)) {
    value as number; // okay
    value as string; // error number ~> string
  } else {
    value as string; // error number ~> string
    value as number; // error string ~> number
  }

  declare class ReadOnlyArray_<out T> {
    filter(callbackfn: typeof Boolean): Array<NonNullable<T>>;
    filter<This, S extends T>(predicate: (this: This, value: T, index: number, array: ReadonlyArray<T>) => implies value is S, thisArg?: This): Array<S>;
    filter<This>(callbackfn: (this : This, value: T, index: number, array: ReadonlyArray<T>) => unknown, thisArg : This): Array<T>;
  }

  declare const arr: ReadOnlyArray_<unknown>;
  arr.filter(fn) as Array<number>; // okay
  arr.filter(fn) as Array<string>; // error number ~> string

  declare const ro_arr: ReadonlyArray<unknown>;
  ro_arr.filter(fn) as Array<number>; // okay
  ro_arr.filter(fn) as Array<string>; // error number ~> string

  declare const rw_arr: ReadonlyArray<unknown>;
  rw_arr.filter(fn) as Array<number>; // okay
  rw_arr.filter(fn) as Array<string>; // error number ~> string
}

function sentinel_refinement() {
  type TypeA = Readonly<{ type: 'TypeA', id: string }>;
  type TypeB = Readonly<{ type: 'TypeB', id: string }>;
  type TypeC = Readonly<{ type: 'TypeC', id: string }>;

  type AllTypes = TypeA | TypeB | TypeC;
  type SpecialTypes = TypeA | TypeB;

  declare function isSpecial(item: AllTypes): item is SpecialTypes;

  function ManipulateOnlyIfSpecial(item: AllTypes) {
    if (isSpecial(item)) {
      // item is SpecialTypes = TypeA | TypeB
      item as SpecialTypes; // okay
      item as AllTypes; // okay
      item as TypeA; // error
      item as TypeB; // error
      item as TypeC; // error
    } else {
      // item is TypeC
      item as SpecialTypes; // error TypeC ~> SpecialType
      item as AllTypes; // okay
      item as TypeA; // error
      item as TypeB; // error
      item as TypeC; // okay
    }
  }
}

import type {Animal, Raccoon} from './opaque_exports';
import {isAnimal, isRaccoon} from './opaque_exports';

function getRaccoon(s: string): ?Raccoon {
  if (!isAnimal(s)) return null;
  if (isRaccoon(s)) {
    return s; // okay
  }
  return null;
}

function negation2() {
  declare const isA: (x: unknown) => x is 'a';
  declare const x: 'a' | 'c';

  if (isA(x)) { return; }
  x as 'c'; // okay
}

function negation3() {
  declare function isFalsey(value: ?unknown): value is null | undefined | false | '' | 0;

  function foo(arr: ?Array<unknown> | ReadonlyArray<unknown>): void {
    if (isFalsey(arr)) {
      return;
    }
    arr as Array<unknown> | ReadonlyArray<unknown>; // okay
  }

  foo([]); // triggrers union optimization
}

function refine_any_elements(
  arr: Array<any> | string,
  obj: {data: any},
  map: Map<string, any>,
) {
  declare function isStrings(value: unknown): value is Array<string>;
  if (isStrings(arr)) {
    arr as Array<string>; // okay
    arr as Array<number>; // error string ~> number
  }

  declare function hasStringData(value: unknown): value is {data: string};
  if (hasStringData(obj)) {
    obj.data as string; // okay
    obj.data as number; // error string ~> number
  }

  declare function isStringMap(value: unknown): value is Map<string, string>;
  if (isStringMap(map)) {
    map.get('a') as ?string; // okay
    map.get('a') as ?number; // error string ~> number
  }
}

type AnyAlias = any;
declare const anyVal: any;

function refine_resolved_any_props(obj: {data: AnyAlias}) {
  declare function hasStringData(value: unknown): value is {data: string};
  if (hasStringData(obj)) {
    obj.data as number; // error string ~> number
  }

  const lit = {data: anyVal};
  if (hasStringData(lit)) {
    lit.data as number; // error string ~> number
  }
}
