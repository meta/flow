// ECMAScript chooses the even shortest decimal when two names are equally close.
{
  const o = {1000000000000000.25: true}; // OK
  o['1000000000000000.2'] as boolean; // OK
  o[1000000000000000.25] as boolean; // OK
  o['1000000000000000.3']; // ERROR: a distinct string name despite equal numeric parsing
  o as {readonly [key: number]: boolean}; // OK
  ({'1000000000000000.2': true}) as {readonly [key: number]: unknown}; // OK
  ({'1000000000000000.3': true}) as {readonly [key: number]: unknown}; // ERROR
}

// Negative numbers use the same canonical tie decision with the sign preserved.
{
  const o = {[-1000000000000000.25]: 'negative'}; // OK
  o['-1000000000000000.2'] as string; // OK
  o['-1000000000000000.3']; // ERROR
}
