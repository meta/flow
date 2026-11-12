declare const v: void;
declare const u: undefined;

// `undefined` is assignable to `void`.
v as void; // OK
u as void; // OK
undefined as void; // OK

// `void` is assignable only to `void`, `any` and `unknown`.
v as undefined; // ERROR
v as ?number; // ERROR
v as number | void; // OK
v as number | undefined; // ERROR
v as unknown; // OK
v as any; // OK
null as void; // ERROR
({f: v}) as {f?: number}; // ERROR

function f(x?: number) {}
f(v); // ERROR
f(undefined); // OK

function annotatedReturn(): void {
  return 1; // ERROR
}
function returnsUndefined(): void {
  return undefined; // OK
}
function returnsNothing(): void {
  return; // OK
}
