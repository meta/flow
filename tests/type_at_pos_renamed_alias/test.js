declare opaque type O;
type A = O;
type B = A;
type C = B;
type Obj = {x: number};
type R = Obj;
type R2 = R;
declare class K {}
type KAlias = K;
type P<T> = {p: T};
type PString = P<string>;

declare const c: C;
//               ^
declare const r: R;
//               ^
declare const r2: R2;
//                ^
declare const k: KAlias;
//               ^
type KAlias2 = KAlias;
declare const k2: KAlias2;
//                ^
declare const p: PString;
//               ^
type PAlias = PString;
declare const pa: PAlias;
//                ^
/** @flowCustomError */
type CE = {p: number};
type CEAlias = CE;
type CEAlias2 = CEAlias;
declare const ce: CEAlias2;
//                ^

declare namespace NS {
  type N = B;
  type M = R;
}
declare const n: NS.N;
//                  ^
declare const m: NS.M;
//                  ^

type X = Y;
type Y = X;
declare const x: X;
//               ^
