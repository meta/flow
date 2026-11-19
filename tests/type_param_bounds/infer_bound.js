type Box<T extends number> = {v: T};
type NeedsNum<N extends number> = N;
export type NumOf<B> = B extends Box<infer X> ? NeedsNum<X> : 'no'; // ok
