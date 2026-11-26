declare const s: symbol;

// Creation
{
  const x: symbol = Symbol(); // OK
  const y: symbol = Symbol('bar'); // OK

  Symbol('foo', 'bar'); // Error: unused argument
}

// Properties and methods
{
  const x: string = s.toString(); // OK
  const y: ?symbol = s.valueOf(); // OK
  const z: string | undefined = s.description; // OK
}

// Refinement
{
  const x: symbol | boolean = true;
  if (typeof x === "symbol") {
    x as symbol; // OK
    x as boolean; // Error
  } else {
    x as boolean; // OK
    x as symbol; // Error
  }
}

// Well-known symbols
{
  Symbol.hasInstance as symbol; // OK
  Symbol.isConcatSpreadable as symbol; // OK
  Symbol.match as symbol; // OK
  Symbol.matchAll as symbol; // OK
  Symbol.metadata as symbol; // OK
  Symbol.replace as symbol; // OK
  Symbol.search as symbol; // OK
  Symbol.species as symbol; // OK
  Symbol.split as symbol; // OK
  Symbol.toPrimitive as symbol; // OK
  Symbol.toStringTag as symbol; // OK
  Symbol.unscopables as symbol; // OK

  const x: typeof Symbol.match = Symbol.match; // OK
  const y: typeof Symbol.match = Symbol.toPrimitive; // Error

  // Deprecated aliases retained for backward compatibility.
  Symbol.toPrimitive as $SymbolToPrimitive; // OK
  Symbol.iterator as $SymbolIterator; // OK
}

// Non-strict equality
{
  // Comparison with symbol is allowed
  const s: symbol = Symbol();
  const y: symbol = Symbol();
  s == y; // OK
  y == s; // OK
  s != y; // OK
  y != s; // OK
}
{
  const s: symbol = Symbol();
  // Cannot compare against non-symbols
  const x: [] = [];
  x == s; // Error
  s == x; // Error
  x != s; // Error
  s != x; // Error
}
{
  const s: symbol = Symbol();
  // Other than null/void which we always allow
  s == null; // OK
  null == s; // OK
  s == undefined; // OK
  undefined == s; // OK
}
