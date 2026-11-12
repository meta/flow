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
