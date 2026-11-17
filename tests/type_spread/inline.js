//@flow
type A1 = {p: number};
type B1 = {...A1, [key: number]: string};
({p: 3}) as B1;

type A2 = {p: string};
type B2 = {...A2, p?: string, ...};
declare const x: {p?: string, ...};
x as B2;

type A3 = {p: number, ...};
type B3 = {...A3, [key: number]: string, ...};
({p: 3}) as B3; // Ok! even though A3 is inexact.
// You can't specify an indexer inline at runtime, so this is sound!

type A4 = {p: string, ...};
type B4 = {...A4, p?: string, ...};
declare const x2: {p?: string, ...};
x2 as B4; // Ok for the same reason as above, except for optional props

// Ensuring we still error in these cases when the RHS is a spread
// is handled by all_cases.js
