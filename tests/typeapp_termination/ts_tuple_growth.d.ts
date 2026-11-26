type BuildTuple<L extends number, T extends readonly unknown[] = []> = T extends {readonly length: L}
  ? T
  : BuildTuple<L, [unknown, ...T]>;
type Subtract<A extends number, B extends number> =
  BuildTuple<A> extends [...(infer U), ...BuildTuple<B>] ? U['length'] : never;
type Id<N extends number> = N;
export type Decrement<N extends number> = Id<Subtract<N, 1>>;
