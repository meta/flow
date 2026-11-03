declare opaque type Box<out T>;

declare const render: ((overrides?: number) => void) &
  (<R>((props: {readonly s: string}) => R) => R);

type Unbox<B> = B extends Box<infer C> ? (C extends {readonly s: unknown} ? C : empty) : empty;
declare function renderUnbox<S>(props: Unbox<Box<{readonly s: S}>>): S;
render(renderUnbox) as string; // ok
render(renderUnbox) as number; // error: string ~> number

type ParamOf<B> =
  B extends Box<(x: infer A) => void> ? (A extends {readonly s: unknown} ? A : empty) : empty;
declare function renderParam<S>(props: ParamOf<Box<(x: {readonly s: S}) => void>>): S;
render(renderParam) as string; // ok
render(renderParam) as number; // error: string ~> number
