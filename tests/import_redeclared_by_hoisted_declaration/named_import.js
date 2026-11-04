import {helper} from './exports'; // ERROR

function helper(x: number): number {
  return x;
}

function read() {
  return helper(1);
}
