// Destructuring canonicalizes numeric patterns and preserves the rest's keys.
{
  const o = {0.5: 'half', 1.5: 15, '-0.5': true}; // OK
  const {0.5: half, ...rest} = o; // OK
  half as string; // OK
  half as number; // ERROR
  rest[1.5] as number; // OK
  rest['-0.5'] as boolean; // OK
  rest['0.5']; // ERROR: removed by the numeric pattern
  rest[1.5] as string; // ERROR
  const {['1.5']: oneAndHalf, '-0.5': negative} = rest; // OK
  oneAndHalf as number; // OK
  negative as boolean; // OK
  const {'2.5': missing} = o; // ERROR
}

// A refinement from one spelling is visible through the other spelling.
function refine(o: {'0.5': string | number}): void {
  if (typeof o[0.5] === 'string') { // OK
    o['0.5'] as string; // OK
    o[0.5] as number; // ERROR
    o['0.5'] = 42; // OK
    o[0.5] as number; // OK
    o[0.5] as string; // ERROR: quoted write invalidates numeric refinement
  }
  if (typeof o['0.5'] === 'string') { // OK
    o[0.5] as string; // OK
    o[0.5] = 42; // OK
    o['0.5'] as string; // ERROR: numeric write invalidates quoted refinement
  }
}

// Calls invalidate a fractional property refinement just like an ordinary field.
declare function mutate(o: {'0.5': string | number}): void;
function callInvalidation(o: {'0.5': string | number}): void {
  if (typeof o['0.5'] === 'string') { // OK
    o[0.5] as string; // OK
    mutate(o); // OK
    o[0.5] as string; // ERROR
  }
}

// The '0' property refines independently of the distinct quoted '-0' property.
function refineZero(o: {'0': string | number, '-0': string | number}): void {
  if (typeof o[0] === 'string') { // OK
    o['0'] as string; // OK
    o['-0'] as string; // ERROR
    o['-0'] = 42; // OK
    // Computed writes conservatively invalidate the object's field refinements.
    o[0] as string | number; // OK
    o[0] as string; // ERROR
  }
}

// Fractional discriminants select the correct union branch across spellings.
type Tagged =
  | {'0.5': 'text', value: string}
  | {'0.5': 'number', value: number}; // OK
function sentinel(o: Tagged): void {
  if (o[0.5] === 'text') { // OK
    o.value as string; // OK
    o.value as number; // ERROR
  } else {
    o.value as number; // OK
    o.value as string; // ERROR
  }
  if (o['0.5'] === 'number') { // OK
    o.value as number; // OK
    o.value as string; // ERROR
  }
}

// Both numeric and quoted 'in' checks refine the same fractional property.
function hasFraction(o: {0.5: string} | {1.5: number}): void {
  if (0.5 in o) { // OK
    o['0.5'] as string; // OK
    o[0.5] as number; // ERROR
  }
  if ('0.5' in o) { // OK
    o[0.5] as string; // OK
    o['0.5'] as number; // ERROR
  }
}

// Contextual hints find callbacks by numeric, quoted and computed names.
{
  const callbacks: {'0.5': (string) => string, '1.5': (number) => number} = {
    0.5: value => {
      value as string; // OK
      value as number; // ERROR
      return value; // OK
    },
    [1.5]: value => {
      value as number; // OK
      value as string; // ERROR
      return value; // OK
    },
  }; // OK
  const quoted: {'0.5': (string) => string} = {
    '0.5': value => {
      value as string; // OK
      value as number; // ERROR
      return value; // OK
    },
  }; // OK
  callbacks[0.5]('half') as string; // OK
  quoted[0.5](5); // ERROR
}
