// A recursive conditional type used generically evaluates itself again while it is evaluated, and
// Flow resolves what that evaluation still feeds to `empty` too early. TypeScript accepts every case
// in this file.

type B6<T> = T extends [infer H] ? (H extends 1 ? [0, ...B6<H>] : [1]) : [];
function b6<X>(x: B6<X>): ReadonlyArray<0 | 1> {
  return x; // OK (Flow errors at `B6<X>`)
}

// A spread of `S` that is last, after a recursive spread through the bound check.
type Nums<T extends ReadonlyArray<number>> = T;
type C2<Configs> = Configs extends [infer Own, ...infer Parents]
  ? Own extends {s: infer S}
    ? S extends [number]
      ? [...S, ...C2<Parents>]
      : C2<Parents>
    : C2<Parents>
  : [];
type UseC2<X> = Nums<C2<X>>; // OK (Flow errors at the inner conditional type)

// Both branches spread the recursion after a constant element.
type Bits<X> = X extends [infer O, ...infer P] ? (O extends 1 ? [0, ...Bits<P>] : [1, ...Bits<P>]) : [];
function bits<X>(x: Bits<X>): ReadonlyArray<0 | 1> {
  return x; // OK (Flow errors at the inner conditional type)
}

// The recursion inside another conditional type.
type ExcludeRecur<T> =
  T extends [infer H] ? (H extends 1 ? Exclude<ExcludeRecur<H>, {b: 1}> : {b: 1}) : {b: 2};
function excludeRecur<X>(x: ExcludeRecur<X>): {...} {
  return x; // OK (Flow errors in `Exclude`)
}
