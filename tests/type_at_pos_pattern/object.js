// @flow

declare opaque type O;
declare const o: O;

declare const c2: O;
declare const d2: O;
declare const e: string;

var {
  a,
//^
  // A destructured binding is framed by the declaration it resolves to, so `b: b1`
  // reports the bound name rather than the property name.
  b: b1,
//^
//   ^
//^
  c = c2,
//^
  d: d1 = d2,
//^
  [e]: { e1 }
// ^
} = {
  a: o,
  b: o,
  c: o,
  d: o,
  "e": { a: o },
};
