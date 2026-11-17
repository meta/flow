//@flow

declare const x: ?{a: number, b?: {c: number, ...}, ...};
declare const y: ?{[key: string]: number};
declare const z: {d: ?{c: number, ...}, ...};
declare const w: ?{g: {e: ?number, ...}, ...};
declare const w2: {g: {e: ?number, ...}, ...};

delete x?.a; // error from writing undefined to number, otherwise ok
delete x?.b?.c; // error from writing undefined to number, otherwise ok
delete y?.['a']; // ok
delete z.d?.c; // error from writing undefined to number, otherwise ok
delete z?.d; // unnecessary chain
delete w?.g.e; // ok
delete w?.g?.e; // one unnecessary chain
delete w2?.g?.e; // two unnecessary chains

declare const a: ?{a: ?number, ...};
delete a?.a;
a.a as empty; // don't refine a.a to definitely exist

// optional chain in parens doesn't short-circuit
(w?.g).e = 42; // should fail
delete (w?.g).e; // should fail
