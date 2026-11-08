// A call checked against the library signature of `Object.defineProperty` gives the descriptor a
// contextual type.

declare const o: {...};
declare const key: string;

Object.defineProperty(o, key, {set: v => {}}); // okay
