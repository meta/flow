declare opaque type O;
type A = O;
type B = A;
type Obj = {x: number};
type R = Obj;

declare namespace NS {
  type N = B;
  type M = R;
  type S = {y: string};
  type Array = number;
}

type T1 = NS.N;
//           ^
type T2 = NS.M;
//           ^
type T3 = NS.S;
//           ^
type T4 = NS.Array;
//            ^

import * as ns from './helpers/exports_annot';

type T5 = ns.X;
//           ^
