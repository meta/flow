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
