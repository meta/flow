//@flow

type A = {a: number};
type B = {b: number};
declare const u: A | B;
declare const m: ?{c: number};
({...u, ...m}); // Error, mentions `A | B` once
