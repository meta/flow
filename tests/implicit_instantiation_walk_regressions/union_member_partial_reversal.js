// What flows to `Partial<C>` says only which properties `C` may have. It does not make a member of
// a union disagree with the direct evidence for `C`.

type Box<C> = {readonly c: C};
declare var box: Box<{a: number, b: string}>;

declare function f<C extends {...}>(...args: [Box<C>] | [Box<C>, Partial<C>]): C;
const r1 = f(box, {a: 1}); // was an error
r1 as {a: number, b: string};
const r2 = f(box);
r2 as {a: number, b: string};
f(box, {c: 1}); // error: `c` is not a property of the config

declare function g<C extends {...}>(x: {k: 1, b: Box<C>} | {k: 2, b: Box<C>, p: Partial<C>}): C;
const r3 = g({k: 2, b: box, p: {a: 1}}); // was an error
r3 as {a: number, b: string};
