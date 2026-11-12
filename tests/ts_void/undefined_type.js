// `undefined` is the type of the value `undefined`, without `ts_syntax`.
const u: undefined = undefined; // OK
declare const v: void;
const x: undefined = v; // ERROR

declare function maybeUndefined(): string | undefined;
maybeUndefined() as ?string; // OK
