type T = {[key: string]?: number};
declare const t: T;
t["foo"] as number; // ERROR - undefined is not number

type S = {readonly [key: string]?: number};
declare const s: S;
s["foo"] as number; // ERROR - undefined is not number

declare class C {
  [key: string]?: number;
}
declare const c: C;
c["foo"] as number; // ERROR - undefined is not number

interface I {
  [key: string]?: number;
}
declare const i: I;
i["foo"] as number; // ERROR - undefined is not number
