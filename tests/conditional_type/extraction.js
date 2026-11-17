function extract_arr_element() {
  type Extract<T> = T extends Array<infer X> ? X : false;
  0 as Extract<Array<number>>; // ok
  0 as Extract<Array<string>>; // error: number ~> string
  true as Extract<3>; // error: true ~> false
}

function extract_arr_element_with_bound() {
  type Extract<T> = T extends Array<infer X extends number> ? X : boolean;
  0 as Extract<Array<number>>; // ok
  '0' as Extract<Array<number>>; // error: string ~> number
  0 as Extract<Array<string>>; // error: number ~> boolean
}

function extract_return_type() {
  3 as ReturnType<() => number>; // ok
  3 as ReturnType<() => string>; // error: number ~> string
  3 as ReturnType<(x: number) => number>; // ok
  3 as ReturnType<(x: number) => string>; // error: number ~> string
  3 as ReturnType<(a: number, b: string, ...rest: any) => number>; // ok
  3 as ReturnType<(a: number, b: string, ...rest: any) => string>; // error: number ~> string
}

function extract_parameters_type() {
  3 as Parameters<() => number>[0]; // error: invalid-tuple-index
  '' as Parameters<(a: string, b: number) => string>[0]; // ok
  3 as Parameters<(a: string, b: number) => string>[0]; // error: number ~> string
  '' as Parameters<(a: string, b: number) => string>[1]; // error: string ~> number
  3 as Parameters<(a: string, b: number) => string>[1]; // ok
}

function extract_this_parameter_type() {
  3 as ThisParameterType<(this: number, x: string) => void>; // ok
  '' as ThisParameterType<(this: number, x: string) => void>; // error: string ~> number

  declare const implicitThis: ThisParameterType<(a: string, b: number) => string>;
  implicitThis as number; // error: unknown ~> number

  3 as OmitThisParameter<ThisParameterType<(this: number, x: string) => void>>; // ok: ThisParameterType extracts `number`; OmitThisParameter on a non-function falls through unchanged, so target is `number`

  declare const noThisParam: OmitThisParameter<(this: number, x: string) => void>;
  noThisParam(''); // ok: the explicit `this` parameter was removed
}

function recursive_awaited_type() {
  // A simplied version of TS's Awaited type by focusing on nonimal Promise type only,
  // but we still exercise the recursive conditional type behavior.
  type Awaited<T> = T extends Promise<infer F> ? Awaited<F> : T;

  declare const awaited_null: Awaited<null>;
  declare const awaited_n1: Awaited<number>;
  declare const awaited_n2: Awaited<Promise<number>>;
  declare const awaited_n3: Awaited<Promise<Promise<Promise<Promise<number>>>>>;
  awaited_null as empty; // error: null ~> empty
  awaited_n1 as empty; // error: number ~> empty
  awaited_n2 as empty; // error: number ~> empty
  awaited_n3 as empty; // error: number ~> empty
}

function excluded_first_rest_params() {
  type RestParams<T extends (...args: ReadonlyArray<empty>) => unknown> =
  T extends (fst: any, ...args: infer Args) => any ? Args : null;
  declare function foobar(a: number, b: boolean, c: string, d: number): void;
  const [b1, s1, n1]: RestParams<typeof foobar> = [true, 'hello', 123]; // ok
  const [b2, s2, n2]: RestParams<typeof foobar> = [123, true, 'hello']; // error
}
