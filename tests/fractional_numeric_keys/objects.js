// Direct, quoted and computed fractional names identify the same properties.
{
  const direct = {0.5: 'half', 1.5: 15}; // OK
  const quoted = {'0.5': 'half', '1.5': 15}; // OK
  const computed = {[0.5]: 'half', [1.5]: 15}; // OK
  direct as {'0.5': string, '1.5': number}; // OK
  quoted as {0.5: string, 1.5: number}; // OK
  computed as {0.5: string, 1.5: number}; // OK
  direct[0.5] as string; // OK
  computed['1.5'] as number; // OK
  direct[0.5] as number; // ERROR
  computed['1.5'] as string; // ERROR
  quoted[2.5]; // ERROR
  ({}) as {0.5: string}; // ERROR
}

// Signed computed keys preserve the sign, except for numeric negative zero.
{
  const signed = {[-0.5]: true, [-1.5]: 'negative', [0.5]: 5}; // OK
  signed['-0.5'] as boolean; // OK
  signed[-1.5] as string; // OK
  signed[0.5] as number; // OK
  signed[-0.5] as string; // ERROR
  signed[1.5]; // ERROR
  const zero = {[-0]: 'zero', '-0': false}; // OK
  zero[0] as string; // OK
  zero[-0] as string; // OK
  zero['0'] as string; // OK
  zero['-0'] as boolean; // OK
  zero[-0] as boolean; // ERROR
  zero['-0'] as string; // ERROR
}

// Different literal spellings use ECMAScript's canonical finite-number names.
{
  const o = {
    .5: 'half', // OK
    [15e-1]: 'one and a half', // OK
    1e-7: 'tiny', // OK
    [1e-6]: 'decimal', // OK
    1e20: 'large decimal', // OK
    [1e21]: 'exponent', // OK
    9007199254740993: 'rounded', // OK
    [-1e100]: 'negative exponent', // OK
    1.7976931348623157e308: 'largest finite', // OK
  };
  o['0.5'] as string; // OK
  o[1.5] as string; // OK
  o['1e-7'] as string; // OK
  o['0.000001'] as string; // OK
  o['100000000000000000000'] as string; // OK
  o['1e+21'] as string; // OK
  o['9007199254740992'] as string; // OK
  o['-1e+100'] as string; // OK
  o['1.7976931348623157e+308'] as string; // OK
  o as {readonly [key: number]: unknown}; // OK
  o['0.5'] as number; // ERROR
  o['1e+21'] as number; // ERROR
  o['5e-1']; // ERROR
  o['1e-6']; // ERROR
  o['1e21']; // ERROR
  o['9007199254740993']; // ERROR
}

// Fractional member names work for methods and accessors as well as fields.
{
  const members = {
    0.5(value: string): string { return value; }, // OK
    [1.5](value: number): number { return value; }, // OK
    get '-0.5'(): boolean { return true; }, // OK
    set '-0.5'(value: boolean): void {}, // OK
  };
  members['0.5']('value') as string; // OK
  members[1.5](1) as number; // OK
  members[-0.5] as boolean; // OK
  members['-0.5'] = false; // OK
  members[0.5](1); // ERROR
  members['1.5']('value'); // ERROR
  members[-0.5] = 'value'; // ERROR
}

// Non-finite literal expressions are not valid numeric property names.
{
  const computed = {[1e309]: true}; // ERROR
  const direct = {1e309: true}; // ERROR
  type NonFinite = {1e309: boolean}; // ERROR
}

// Array and tuple position semantics do not become object-name semantics.
{
  declare const arr: Array<string>;
  arr[0.5] as string; // OK
  arr[0.5] as number; // ERROR
  arr['0.5']; // ERROR: named missing property, not a numeric array position
  declare const tuple: [string, number];
  tuple[0] as string; // OK
  tuple[1] as number; // OK
  tuple[0.5]; // ERROR
  tuple['0.5']; // ERROR
  type FractionalPosition = [string, number][0.5]; // ERROR
}
