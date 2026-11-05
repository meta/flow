//@flow

// The parameter type spreads a type of the type parameters, so the argument gives them no bounds.
// Both are pinned to their bounds, and the bound of `TPartial` says nothing about `TProps`.
declare function scramble<TProps extends {...}, TPartial extends Readonly<Partial<TProps>>>(
  props: {c: (p: TProps) => void, p: TPartial, ...Omit<NoInfer<TProps>, keyof TPartial>},
): void;
declare const ad: (p: {a: string}) => void;
scramble({c: ad, p: {a: 'x'}}); // error: `{...}` is incompatible with `{a: string}`

// Nothing flows into `B`, which only has an upper bound, so its bound says nothing about `A`.
declare function h<A, B extends Readonly<A>>(cb: (b: B) => void): A;
h((b: {x: number}) => {}); // error: `A` is underconstrained
