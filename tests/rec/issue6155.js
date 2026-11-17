// @flow

type A = {kind: 'a', e: Type, ...};
type B = {kind: 'b', k: Type, v: Type, ...};
type C = {kind: 'c', ...};
type Type = A | B | C;

type TypeCases<R> = {
    a: (a: A) => R,
    b: (b: B) => R,
    c: (c: C) => R
};

function matcher<R>(cases: TypeCases<R>): (t: Type) => R {
    return (type) => cases[type.kind](type);
}

const f: (t: Type) => Array<string> = matcher({
    a: (a: A) => [...f(a.e)],
    b: (b: B) => [...f(b.k), ...f(b.v)],
    c: () =>  ['']
});
