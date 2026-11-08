// Nonfinite numeric literals remain invalid named keys, with a finite-number diagnostic.
const literal = {1e309: true}; // ERROR
const computed = {[1e309]: true}; // ERROR
const negative = {[-1e309]: true}; // ERROR

type Nonfinite = {1e309: boolean}; // ERROR
interface NonfiniteInterface { 1e309: boolean } // ERROR
declare class NonfiniteClass { 1e309: boolean } // ERROR

// Reads and writes cannot use a nonfinite numeric literal to name an object property.
{
  declare const o: {'Infinity': boolean};
  o[1e309]; // ERROR
  o[1e309] = true; // ERROR
  o['Infinity'] as boolean; // OK
}

// Quoted nonfinite spellings remain ordinary string names, not number-dictionary keys.
{
  const o = {'NaN': true, 'Infinity': false, '-Infinity': true}; // OK
  o as {[key: string]: boolean}; // OK
  o as {readonly [key: number]: unknown}; // ERROR
}

// Destructuring and match object-property restrictions now describe finite literals.
{
  declare const o: {'Infinity': boolean} | {known: boolean};
  const {1e309: value} = o; // ERROR
  const result = match (o) {
    {1e309: _} => true, // ERROR
    _ => false,
  };
}

// Rejected numeric keys do not reserve a legitimate quoted property name.
{
  declare const input: {'Infinity': string} | {known: number};
  const result = match (input) {
    {1e309: _, 'Infinity': const value} => value, // ERROR: invalid numeric key only
    _ => 'fallback',
  };
  result as string; // OK
  result as number; // ERROR
}

// Repeated rejected keys do not create duplicate named-property diagnostics.
{
  declare const input: {'Infinity': string} | {known: number};
  const result = match (input) {
    {1e309: _, 1e309: _} => 1, // ERROR: each nonfinite key is invalid
    _ => 3,
  };
}
{
  declare const input: {'1n': string} | {known: number};
  const result = match (input) {
    {1n: _, '1n': _} => 2, // ERROR: bigint key is invalid, not a named duplicate
    _ => 3,
  };
}

// Valid canonical keys still participate in duplicate-property detection.
{
  declare const input: {'Infinity': string, '1.5': string} | {known: number};
  const numeric = match (input) {
    {1.5: _, '1.5': _, ...} => 1, // ERROR: duplicate canonical key
    _ => 2,
  };
  const quoted = match (input) {
    {'Infinity': _, 'Infinity': _, ...} => 1, // ERROR: duplicate quoted key
    _ => 2,
  };
}
