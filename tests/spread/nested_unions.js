declare const OBJ: {a: string, ...};

declare const works1: {...} | void;
declare const works2: {...} | void | void;
declare const works3: {...} | (void | void);

({...OBJ, ...works1}); // error: cannot-spread-inexact
({...OBJ, ...works2}); // error: cannot-spread-inexact
({...OBJ, ...works3}); // error: cannot-spread-inexact

type A   = { t: 'a', ... };
type BC  = { t: 'b', ... } | { t: 'c', ... };
type ABC = { t: 'a', ... } | { t: 'b', ... } | { t: 'c', ... };

declare const x1: ABC;
declare const x2: A | BC;
const o1: BC = { ...x1 }; // errors
const o2: BC = { ...x2 }; // errors

type P = {p: number};
type Q = {q: string};
declare const unionInIntersection: (P | null | void) & Q;
({...unionInIntersection}).q as string; // ok
type Id<T> = T;
declare const unionInUnion: Q | Id<P | null>;
({...unionInUnion}) as {p?: number}; // error: `q` is extra
declare const intersectionInUnion: Q | (P & {r: boolean});
({...intersectionInUnion}) as {p?: number, r?: boolean}; // error: `q` is extra
