declare interface TestInterface {
  // $FlowFixMe[unsafe-getters-setters]
  get foo(): string;
  // $FlowFixMe[unsafe-getters-setters]
  set foo(value: number): void;
}
