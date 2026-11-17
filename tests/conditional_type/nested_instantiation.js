{
  declare function poly<T>(a: T extends number ? number : string, b: T): T;
  poly(3, 3); // ok
}

{
  declare function poly<T>(x: T extends number ? number : string): T;
  poly(3); // underconstrained
}

{
  declare function poly<T>(value: T, x: T extends number ? number : string): T;
  const result = poly("foo", "bar");
  result as string;
}

{
  declare function poly<T, U = number>(
    value: T,
    x: T extends number ? {readonly a: U, ...} : {readonly b: U, ...},
  ): U;
  const both = {a: "not number", b: 42};
  const result = poly(3, both);
  result as string;
  result as number; // error: string ~> number
}

{
  declare function poly<U = number, T = number>(
    x: T extends number ? {readonly a: U, ...} : {readonly b: U, ...},
  ): U;
  const both = {a: "not number", b: 42};
  const result = poly(both);
  result as string;
  result as number; // error: string ~> number
}

{
  declare function poly<T extends number, U = number>(
    value: T,
    x: T extends number ? {readonly a: U, ...} : {readonly b: U, ...},
  ): U;
  const both = {a: "string", b: 42};
  const result = poly(3, both);
  result as string;
}

{
  declare function poly<T extends number | string>(
    value: T,
    x: T extends number ? number : string,
  ): T;
  poly(3, 3); // ok: the bound does not make the conditional definitely false

  declare function nested<T extends number | string>(
    value: T,
    x: {readonly value: T, ...} extends {readonly value: number, ...}
      ? number
      : string,
  ): T;
  nested(3, 3); // ok: nested frozen tvars are also generic for this check
}

{
  declare function poly<T extends number | string, U = number>(
    value: T,
    x: T extends number ? {readonly a: U, ...} : {readonly b: U, ...},
  ): U;
  const both = {a: "not number", b: 42};
  const result = poly(3, both);
  result as string;
  result as number; // error: string ~> number
}

{
  declare function nested<T extends number | string, U = number>(
    value: T,
    x: {readonly value: T, ...} extends {readonly value: number, ...}
      ? {readonly a: U, ...}
      : {readonly b: U, ...},
  ): U;
  const both = {a: "not number", b: 42};
  const result = nested(3, both);
  result as string;
  result as number; // error: string ~> number
}

{
  declare function preserveController<T extends number | string, U = number>(
    value: T,
    x: T extends number
      ? {readonly a: U, readonly value: T, ...}
      : {readonly b: U, readonly value: T, ...},
  ): [T, U];
  const both = {a: "not number", b: 42, value: "wrong"};
  preserveController(3, both); // error: string ~> number
}

{
  declare function definitelyFalse<T, U = number>(
    value: T,
    x: {readonly kind: "a", readonly value: T, ...} extends {
      readonly kind: "b",
      readonly value: number,
      ...
    }
      ? {readonly a: U, ...}
      : {readonly b: U, ...},
  ): U;
  const result = definitelyFalse(3, {b: "not number"});
  result as string;
  result as number; // error: string ~> number
}

{
  declare function id<T>(x: Array<T>): Array<T>;
  type Id<T> = T extends Array<infer E> ? Array<E> : T;
  declare const a: Id<Array<string>>;
  id(a); // ok
}

{
  type AnyFunction = (x: {...}) => void;
  type Modulish<T> = T | {readonly default: T};
  type ModuleDefault<T> = T['default'];
  declare function unwrapModule<
    TModule extends Modulish<AnyFunction>,
    TIsESModule extends boolean,
  >(
    isESModule: TIsESModule,
  ): TIsESModule extends true // $FlowFixMe[prop-missing]
    ? ModuleDefault<TModule>
    : TModule;
  type ExactFunction = (x: {foo: string}) => void;
  unwrapModule(true) as ExactFunction;
}

{
  type Unwrap<T> = T extends {readonly v: infer V | null} ? V : empty;
  declare function f<P>(x: Unwrap<{readonly v: P}>): P;
  const r = f(1);
  r as string; // error: number ~> string
}

{
  type UnwrapMaybe<T> = T extends {readonly v: ?infer V} ? V : empty;
  declare function f<P>(x: UnwrapMaybe<{readonly v: P}>): P;
  const r = f(1);
  r as string; // error: number ~> string
}

{
  type Encoding<O> = O extends {readonly encoding: infer E, ...} ? E : number;
  declare function takesString(s: string): void;
  declare function exec(): number;
  declare function exec<O extends {readonly encoding?: string, ...}>(
    options: O,
  ): Encoding<O>;
  takesString(exec({encoding: 'utf8'})); // ok
  exec({encoding: 'utf8'}) as number; // error: 'utf8' ~> number
}
