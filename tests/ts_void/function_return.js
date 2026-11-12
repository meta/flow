// A function type returning `void` accepts functions with any return type.
declare const returnsNumber: () => number;
returnsNumber as () => void; // OK
(() => 1) as () => void; // OK
type V = void;
returnsNumber as () => V; // OK
type Callback<R> = () => R;
returnsNumber as Callback<void>; // OK
declare function takesCallback(cb: (x: number) => void): void;
takesCallback(x => x * 2); // OK
({m: () => 1}) as {m(): void}; // OK
interface I {
  m(): void;
}
class C implements I {
  m(): number {
    return 1;
  }
}

// Only `void` itself.
returnsNumber as () => undefined; // ERROR
(() => 'a') as () => void | number; // ERROR

// The parameters are still checked.
((x: string) => 1) as (x: number) => void; // ERROR

// A function annotated as returning `void` still returns no value.
function annotated(): void {
  return 1; // ERROR
}
