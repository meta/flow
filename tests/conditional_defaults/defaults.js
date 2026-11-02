declare const object: {value?: number};
declare const array: Array<number | void>;
declare function invariant(value: boolean): asserts value;
declare const kind: string;

function objectDefault() {
  let x: number;
  const {value = (x = 1)} = object;
  x as number; // error
}

function arrayDefault() {
  let x: number;
  const [value = (x = 1)] = array;
  x as number; // error
}

function objectAssignmentDefault() {
  let x: number;
  let value: number;
  ({value = (x = 1)} = object);
  x as number; // error
}

function parameterDefault(first: number | string, second: number = (first = 1)) {
  first as number; // error
}

function defaultWriteKeepsSkippedType() {
  let x: number | string = 'before';
  const {value = (x = 1)} = object;
  x as number; // error
}

function successfulDefaultReads() {
  let x: number;
  const {value = (x = 1, x as number)} = object;
  x as number; // error only outside the default
}

function defaultMaySkipThrow() {
  const {value = (match (kind) { _ => invariant(false) }, 1)} = object;
  let x: number = 1;
  x as number;
}

function throwingParameterDefault(value: number = match (kind) { _ => invariant(false) }) {
  value as string; // error: the body is checked
}
throwingParameterDefault as number; // error: a function is not a number
throwingParameterDefault('not a number'); // error

function throwingDestructuringDefault() {
  const {value = match (kind) { _ => invariant(false) }} = object;
  value as string; // error: `value` is a number when the default is skipped
}
