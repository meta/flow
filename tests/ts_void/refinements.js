declare const sv: string | void;
declare const v: void;

if (sv !== undefined) {
  sv as string; // OK
}
if (sv != null) {
  sv as string; // OK
}
if (sv) {
  sv as string; // OK
}
if (sv === undefined) {
  sv as void; // OK
  sv as undefined; // ERROR: `void` stays `void`
}
sv?.length as number | void; // OK
sv?.length as number | undefined; // OK: the short-circuited chain is `undefined`
(sv ?? 'x') as string; // OK
declare const nnsv: NonNullable<string | void>;
nnsv as string; // OK

// TypeScript rejects testing `void` for truthiness.
if (v) {
} // ERROR

declare const nn: NonNullable<void>;
nn as empty; // OK

// A `void` property is tested against a literal as `undefined` is. The test
// prelude declares `undefined` as `void`, so `void 0` stands in for it.
declare const ab: {a: string, type?: void} | {type: 'b'};
declare const cb: {type: void, c: string} | {type: 'b'};
if (ab.type !== void 0) {
  ab as {type: 'b'}; // OK
}
if (cb.type === 'b') {
  cb as {type: 'b'}; // OK
}
if (cb.type === void 0) {
  cb.c as string; // OK
}
declare const bu: 'b' | undefined;
if (cb.type === bu) {
  cb.c as string | undefined; // OK
}
