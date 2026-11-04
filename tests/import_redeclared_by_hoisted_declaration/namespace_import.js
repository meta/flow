import * as helpers from './exports'; // ERROR

export function helpers(): void {}

function read() {
  return helpers;
}
