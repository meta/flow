function test_spread_into_rest_param() {
  declare function f<T extends Array<1 | boolean | string>>(...args: T): T;

  const x1 = f(...[1]); // okay
  const x2 = f(true, ...[1, 'foo']); // okay
}

function test_spread_into_fixed_params() {
  declare function f(value: number, cb: (x: number) => void): void;

  f(...[1, (x) => { x as string; }]); // error number ~> string
  f(1, ...[(x) => { x as string; }]); // error number ~> string
}

function test_spread_after_spread() {
  declare function f(...rest: Array<(x: number) => void>): void;
  declare function g(first: string, ...rest: Array<(x: number) => void>): void;

  f(...[(a) => { a as number; }], ...[(b) => { b as number; }]); // okay
  g(...['s'], ...[(b) => { b as number; }]); // okay
}

function test_spread_past_all_params() {
  declare const xs: Array<{brace: number}>;

  xs.splice(0, xs.length, ...xs.filter(x => x.brace !== 0)); // okay
}

function test_spread_into_new() {
  declare class C {
    constructor(...fns: Array<(x: number) => void>): void;
  }

  new C(...[(x) => { x as string; }]); // error number ~> string
}

function test_spread_of_call_into_rest() {
  declare function assign<O extends {...}>(object: O, ...others: Array<?Partial<O>>): O;
  declare const personas: ReadonlyArray<{name: string}>;

  const form: {[key: string]: boolean} = assign( // okay
    {},
    ...personas.map(persona => ({[persona.name]: true})),
  );
}

function test_spread_of_call_into_fixed_params() {
  declare function useState<S>(
    initialState: S | (() => S),
  ): [S, (action: ((prev: S) => S) | S) => void];
  declare function render(
    value: number,
    setValue: (action: number | ((prev: number) => number)) => void,
  ): void;

  render(...useState(0)); // okay
}

function test_forwarded_rest_args() {
  type Callback = (value: string, count?: number) => void;
  declare const target: Callback;

  const wrapper: Callback = (...args) => target(...args); // okay
}
