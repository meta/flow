declare const v: void;
declare function f(): void;
declare const cb: ?() => void;
declare const vu: void | undefined;
declare const vn: void | null;

// A `void` value may be anything, so only `void` alone is an error when tested
// for truthiness, as TS1345 in TypeScript.
if (v) {} // ERROR
v && 1; // ERROR
const t = v ? 1 : 2; // ERROR

f() ?? 1; // OK: `??` tests nullishness
if (cb?.()) {} // OK
if (vu) {} // OK
if (vn) {} // OK
