import {
  getProp,
  getElem,
  spread,
  exactProp,
  proto,
  not,
  keyMirrorProp,
  keysLength,
  nonHomomorphicProp,
  optionalProp,
  pickedF,
  pickedG,
  stringKeysProp,
  requiredF,
  requiredG,
  requiredH,
} from './export_values';

getProp as string; // OK
getProp as number; // ERROR
// Computed access in an export position is `any`, as it is for object types.
getElem as empty; // OK
spread.f as string; // OK
spread.f as number; // ERROR
exactProp as number; // ERROR
proto.f as number; // ERROR
not as boolean; // OK
not as string; // ERROR
keyMirrorProp as 'f'; // OK
keyMirrorProp as 'g'; // ERROR
keysLength as string; // ERROR
nonHomomorphicProp as number; // OK
nonHomomorphicProp as string; // ERROR
optionalProp as string | void; // OK
optionalProp as string; // ERROR
pickedF as number; // OK
pickedF as string; // ERROR
pickedG as string | void; // OK
pickedG as string; // ERROR 2x
stringKeysProp as number; // OK
stringKeysProp as string; // ERROR
requiredF as number; // OK
requiredF as string; // ERROR
requiredG as string; // OK
requiredG as number; // ERROR
requiredH as string | null; // OK
requiredH as string; // ERROR

import {selfKeyedProp} from './export_values';

selfKeyedProp as number; // OK
selfKeyedProp as string; // ERROR
