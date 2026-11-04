import defaultHelper from './exports'; // ERROR

function defaultHelper(x: number): number {
  return x;
}

const read = () => defaultHelper(1);
