import * as React from 'react';
import {useState} from 'react';

// A class with a static call signature matches the function member of a union, as subtyping
// calls it: `S` is `boolean`, not the class.
component C() {
  const [flag] = useState(Boolean);
  flag as boolean; // ok
  return null;
}

// An instance of a class that extends `Array` matches the array member of a union: `U` is
// `string`, not the instance.
declare const match: RegExpMatchArray;
declare const xs: Array<string>;
const flat = xs.flatMap(x => match);
flat as Array<string>; // ok
