// As in TypeScript, `SymbolConstructor` has no construct signature.
new Symbol(); // Error: `Symbol` can't be constructed
const asClass: Class<Symbol> = Symbol; // Error: no construct signature

// `instanceof` narrows through `SymbolConstructor`'s `prototype`.
declare const x: Symbol | string;
if (x instanceof Symbol) {
  x as string; // Error: narrowed to `Symbol`, not `empty`
} else {
  x as string;
}

// A declared subclass inherits the `prototype` instance, but constructing it
// throws at runtime, and is an error.
declare class SymbolSubclass extends Symbol {}
declare const subclassInstance: SymbolSubclass;
subclassInstance.description as string | undefined;
subclassInstance as Symbol;
new SymbolSubclass(); // Error: no constructor to inherit
