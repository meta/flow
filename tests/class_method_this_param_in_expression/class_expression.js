const C = class {
  m(this: unknown): void {}
};

new C();
