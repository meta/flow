// @flow

declare function random(): boolean;

let foo = (): number | string => "";
if (random()) {
  foo = (): number | string => 0;
}
foo;
//^?

let bar = (x: string) => {};
if (random()) {
  bar = (x: string) => {};
}
bar;
//^?

declare const numObj: { readonly f: number, ... };
declare const strObj: { readonly f: string, ... };

let obj;
if (random()) {
  obj = strObj;
} else {
  obj = numObj;
}
obj;
//^?

class G<X> {
  x: X;
  constructor(x: X) {
    this.x = x;
  }
  get_X(): X {
    return this.x;
  }
  set_X(x: X): void {
    this.x = x;
  }
}

var sg: G<string> = new G("");
var ig = new G(1);

var g = (0 < 1) ? sg : ig;
g.get_X()
// ^?
g.set_X(0 as any);
// ^?

// exhibits use of Ty_normalizer.simplify_unions_inters_visitor

declare const top : unknown | unknown | number;
top;
//^?

declare const top_g : { g: unknown | unknown | number, ...};
top_g;
// ^?

type A = { f: number, ... }
var a1 = { f: 1 };
var a2 = { f: 2 };
var a = (0<1) ? a1 : a2;
//  ^?

declare const maybe_empty: ?empty;
//            ^?

declare opaque type Poly<X>;
declare opaque type K;
declare opaque type L;
declare opaque type P<X>;

declare function test1<TValue>(
  p1: K | L,
// ^?
  p2: ?P<TValue> | L,
// ^?
  p3: Poly<K | L>,
// ^?
  p4: Poly<(K, L) => void>,
// ^?
  p5: Poly<?(K | L)>,
// ^?
): void;

declare opaque type LongLongLongLongLongLongLongLongName;
declare opaque type AnotherLongLongLongLongLongLongLongLongName;
declare opaque type LongLongLongLongLongLongLongLongNameP<X>;

declare function test2<TValue>(
  p1: LongLongLongLongLongLongLongLongName | AnotherLongLongLongLongLongLongLongLongName,
// ^?
  p2: ?LongLongLongLongLongLongLongLongNameP<TValue> | AnotherLongLongLongLongLongLongLongLongName,
// ^?
  p3: Poly<LongLongLongLongLongLongLongLongName | AnotherLongLongLongLongLongLongLongLongName>,
// ^?
  p4: Poly<(LongLongLongLongLongLongLongLongName, LongLongLongLongLongLongLongLongName) => void>,
// ^?
  p5: Poly<?(LongLongLongLongLongLongLongLongName | AnotherLongLongLongLongLongLongLongLongName)>,
// ^?
): void;

declare opaque type LongLongLongLongLongLongLongLongName1;
declare opaque type LongLongLongLongLongLongLongLongName2;
declare opaque type LongLongLongLongLongLongLongLongName3;
declare opaque type LongLongLongLongLongLongLongLongName4;

type LongUnion = LongLongLongLongLongLongLongLongName1 | LongLongLongLongLongLongLongLongName2 | LongLongLongLongLongLongLongLongName3 | LongLongLongLongLongLongLongLongName4;
//   ^?

type LongUnionPoly = Poly<LongLongLongLongLongLongLongLongName1 | LongLongLongLongLongLongLongLongName2 | LongLongLongLongLongLongLongLongName3 | LongLongLongLongLongLongLongLongName4>;
//   ^?

// A union after a `:` or `=>` starts its members on lines of their own.
declare function longParamAndReturn(
  x: LongLongLongLongLongLongLongLongName1 | LongLongLongLongLongLongLongLongName2,
): LongLongLongLongLongLongLongLongName3 | LongLongLongLongLongLongLongLongName4;
longParamAndReturn;
// ^?
declare const longProperty: {
  p: LongLongLongLongLongLongLongLongName1 | LongLongLongLongLongLongLongLongName2,
};
longProperty;
// ^?
declare const longArray: Array<LongLongLongLongLongLongLongLongName1 | LongLongLongLongLongLongLongLongName2 | LongLongLongLongLongLongLongLongName3>;
longArray;
// ^?
// A member's own lines are indented past its `| `.
declare const multilineMembers:
  | {a: LongLongLongLongLongLongLongLongName1, b: LongLongLongLongLongLongLongLongName2}
  | {c: LongLongLongLongLongLongLongLongName3, d: LongLongLongLongLongLongLongLongName4};
multilineMembers;
// ^?
