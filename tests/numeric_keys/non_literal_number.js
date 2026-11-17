// Access with `number`
{
  declare const o: {foo: 1};
  declare const n: number;
  o[n]; // ERROR
}

// Creation with `number`
{
  declare const n: number;
  const o = {
    [n]: true, // OK
  };
  o as {[key: number]: boolean}; // OK
  o[n] as boolean; // OK
}
{
  declare const n: number;
  declare const o1: {[key: number]: boolean};
  // Creation with spread and `number` key
  const o2 = {...o1, [n]: true};
  o2 as {readonly [key: number]: true}; // OK
}
