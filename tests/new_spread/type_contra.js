declare class T {};

type O1 = {...{writeonly p:T, ...}, ...};
declare const o1: O1;
o1 as {p?:unknown, ...}; // ok
o1 as {p?:T, ...}; // error: unknown ~> T
o1.p as T; // errors: undefined ~> T, unknown ~> T

type O2 = {...{writeonly [key: string]:T}, ...};
declare const o2: O2;
o2 as {[key: string]:unknown}; // ok
o2 as {[key: string]:T}; // error: unknown ~> T
o2.p as T; // errors: unknown ~> T

type O3 = {...{}, writeonly p: T, ...};
declare const o3: O3;
o3 as {p:unknown, ...}; // ok
o3 as {p:T, ...}; // error: unknown ~> T
o3.p as T; // errors: unknown ~> T
