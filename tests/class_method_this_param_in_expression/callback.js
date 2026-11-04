import Base from './base';

declare function describe(name: string, f: () => void): void;

describe('a class declared in a callback', () => {
  class D extends Base {
    m(this: D): void {}
  }
});
