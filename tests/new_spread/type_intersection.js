declare class T {}
declare class U {}

declare class A {}
declare class B extends A {}

type O1 = {...{p:T, ...}&{q:U, ...}, ...};
declare const o1: O1;
o1 as {readonly p?:T, readonly q?:U, ...}; // ok

type O2 = {...{p:A, ...}&{p:B, ...}, ...};
declare const o2: O2;
o2 as {readonly p?:B, ...}; // ok
({p: new B} as O2); // ok
({p: new A} as O2); // error: A ~> B

type O3 = {...{p:A, ...}&{[key: string]:B}, ...};
declare const o3: O3;
o3 as {p:B,[key: string]:B};// ok: A&B = B
o3.q as B; // ok

type O4 = {...{[key: string]:A}&{p:B, ...}, ...};
declare const o4: O4;
o4 as {p:B,[key: string]:A}; // ok: A&B = B

type O5 = {...{[key: string]:A}&{[key: string]:B}, ...};
declare const o5: O5;
o5 as {[key: string]:B}; // ok: A&B = B
