function test1() {
  declare function poly<T>(a: (x: string) => void, b: T): T;
  declare function expectString(x: string): string;
  poly((v) => {}, expectString(3)); // Error: incompatible-type, but v can still be contextually typed
}

function test2() {
  ((): THIS_SHOULD_ERROR => new Set([]))
}
