declare function register(c: unknown): void;

register(
  class {
    m(this: unknown): void {}
  },
);
