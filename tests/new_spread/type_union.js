declare class T {}
declare const x: T;

declare class U {}
declare let y: U;

type O1 = {...{p:T, ...}|{q:U, ...}, ...};
declare const o1: O1;
o1 as {readonly p?:T, ...}|{readonly q?:U, ...}; // ok

type Union1 = {[key: string]: number} | {foo: number}
type Union2 = {bar: number} | {[key: number]: string};

declare const x1: {...Union1, ...Union2, ...}; // Error, indexer on right
x1 as {...};

declare const x5: {...Union1, ...{...}, ...} // OK
x5 as {...};

y = {}; // unsealed

type UnsealedInUnion = Union1 | Union2 | typeof y;
declare const x2: {...UnsealedInUnion, ...}; // Error, unsealed
x2 as {...};

type Union3 = {foo: number} | {bar: number};
type Union4 = {baz: number } | {qux: number};
declare const x3: {...Union3, ...Union4};
(x3 as {foo: number, baz: number} // Should consider erroring instead of calculating combinatorial blowup
   | {foo: number, qux: number}
   | {bar: number, baz: number}
   | {bar: number, qux: number}
);

interface I1 {}
type Union5 = I1 | Union3 | Union4;
declare const x4: {...Union5, ...}; // OK
x4 as {...};
