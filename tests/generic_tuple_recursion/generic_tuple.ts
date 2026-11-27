// A tuple type written in TypeScript that spreads a type parameter bounded by an array before other
// elements. TypeScript keeps it generic until the type parameter is known; Flow reports
// `tuple-invalid-type-spread` and makes it `any`. TypeScript accepts every case in this file.

type Nums<T extends ReadonlyArray<number>> = T;

type Last<S extends ReadonlyArray<number>> = Nums<[...S]>; // OK
type BeforeElement<S extends ReadonlyArray<number>> = Nums<[...S, number]>; // ERROR (TypeScript accepts)
type BeforeSpread<S extends ReadonlyArray<number>, U extends ReadonlyArray<number>> = Nums<[...S, ...U]>; // ERROR (TypeScript accepts)

// Reduced from `CollectStateConfigs` in Lexical's `LexicalNodeState.d.ts`.
type Collect<Configs> = Configs extends [infer Own, ...infer Parents]
  ? Own extends {configs: infer S}
    ? S extends ReadonlyArray<number>
      ? [...S, ...Collect<Parents>] // ERROR (TypeScript accepts)
      : Collect<Parents>
    : Collect<Parents>
  : [];
type Collected<Configs> = Nums<Collect<Configs>>;
