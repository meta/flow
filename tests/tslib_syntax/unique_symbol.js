declare const x: unique symbol;
declare const y: unique symbol;

// Unique symbol flows to general symbol
x as symbol;           // OK

// Unique symbol does not flow to empty
x as empty;            // ERROR

// General symbol does not flow to unique symbol
declare const s: symbol;
s as typeof x;  // ERROR - symbol doesn't flow to unique symbol

// Different unique symbols are incompatible
y as typeof x;         // ERROR - different unique symbols are incompatible

// Same unique symbol is compatible
x as typeof x;         // OK - same unique symbol

// typeof refinement: unique symbol has typeof "symbol"
declare const z: unique symbol;
if (typeof z === "symbol") {
  z as typeof z;  // OK - still the same unique symbol after refinement
}

// unique symbol as object key
type Obj = {[key: typeof x]: string};
declare const obj: Obj;
obj[x] as string; // OK

// unique symbol in union with symbol
declare const u: typeof x | typeof y;
u as symbol; // OK - both branches flow to symbol

// unique symbol in function parameter
declare function takesSymbol(s: symbol): void;
declare function takesX(s: typeof x): void;
takesSymbol(x); // OK - unique symbol flows to symbol param
takesX(x);      // OK - same unique symbol
takesX(y);      // ERROR - different unique symbol

// unique symbol assignability
declare const a: typeof x;
a as typeof x;  // OK
a as typeof y;  // ERROR

// unique symbol vs other primitives
x as string; // ERROR
x as number; // ERROR
x as boolean; // ERROR

// mixed/any behavior
x as unknown; // OK
declare const m: unknown;
m as typeof x; // ERROR - mixed doesn't flow to unique symbol
