function test0() {
    declare function outer(...x: Array<(x: number) => void>): void
    declare function inner<A>(iter: Array<A>): Array<A>;

    outer(...[(x) => { // okay no annotation required
        x as string; // error number ~> string
    }]);

    outer(...inner([(x) => { // okay no annotation required
        x as string; // error number ~> string
    }]));
}

// Regression tests from support post
function test1() {
    declare const arrayBuffer: ArrayBuffer;
    String.fromCharCode(...Array.from(new Uint8Array(arrayBuffer))); // okay - correct overload of Array.from selected
}

function test2() {
    declare function foo(
      fn: (
          ...args: Parameters<() => Set<(x: string) => void>>
      ) => ReturnType<() => Set<(x: string) => void>>,
    ): void;

    foo(() => new Set([
      x => {
        x as number; // error string ~> number
      }
    ]));
}

function test3() {
    declare function f<Args extends ReadonlyArray<unknown>, Return>(
        fn: () => (...args: Args) => Return,
    ): (...args: Args) => Return;

    f(() => (a) => {}) as (a: string) => void; // ok
}
