// Non-positional numeric strings cannot borrow the builtin Array numeric indexer.
{
  declare const a: Array<string>;
  a['0'] as string; // OK
  a['1.5']; // ERROR
  a['1e-7']; // ERROR
  a['1e+21']; // ERROR
  a['9007199254740992']; // ERROR
  a['1.5'] = 'fraction'; // ERROR
  a['9007199254740992'] = 'large'; // ERROR
  a.length as number; // OK
  a.map(value => value.length) as Array<number>; // OK
  a[Symbol.iterator](); // OK
  declare const functions: Array<() => number>;
  functions['0']() as number; // OK
  functions['1.5'](); // ERROR
}

// Readonly arrays and tuples keep their existing positional and named-access split.
{
  declare const a: ReadonlyArray<string>;
  a['1'] as string; // OK
  a['0.5']; // ERROR
  a['9007199254740992']; // ERROR
  declare const functions: ReadonlyArray<() => number>;
  functions['0.5'](); // ERROR
  declare const tuple: [string, number];
  tuple['0'] as string; // OK
  tuple['0.5']; // ERROR
  tuple[0.5]; // ERROR
  tuple['9007199254740992']; // ERROR
}

// Generic and nominal safe-integer aliases still reach the element type.
function safeAlias<K extends '1'>(a: Array<string>, key: K): void {
  a[key] as string; // OK
  a[key] as number; // ERROR
}
opaque type One: '1' = '1'; // OK
{
  declare const a: Array<string>;
  declare const key: One;
  a[key] as string; // OK
  a[key] as number; // ERROR
}

// Explicit named fields remain discoverable through intersections in either order.
type Named = {'0.5': number, '9007199254740992': boolean}; // OK
{
  declare const left: Array<string> & Named;
  declare const right: Named & Array<string>;
  left['0.5'] as number; // OK
  right['0.5'] as number; // OK
  left['9007199254740992'] as boolean; // OK
  right['9007199254740992'] as boolean; // OK
  left['0.5'] as string; // ERROR
  right['0.5'] as string; // ERROR
}

// User classes retain their own declarations and number dictionaries.
declare class Custom extends Array<string> { '0.5': number } // OK
declare class Dictionary { [key: number]: string } // OK
{
  declare const custom: Custom;
  custom['0.5'] as number; // OK
  custom['0.5'] as string; // ERROR
  custom['0.6']; // ERROR
  declare const dictionary: Dictionary;
  dictionary['0.5'] as string; // OK
  dictionary['1e-7'] as string; // OK
  dictionary['9007199254740992'] as string; // OK
  dictionary['0.5'] as number; // ERROR
}
