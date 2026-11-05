function case1() {
  declare function f<EJ>(
    name: string,
  ): () => NonNullable<EJ | {canonical: string, path: string, ...}>;

  const g = f<_, {path: string}>('x');
  const {canonical, path} = g();
}

function case2() {
  declare function connect<OP, D>(mapState?: null): [OP, D];
  declare function connect<S, D, OP, SP>(mapState: (S, OP) => SP): [S, D, OP, SP];
  declare var mapState: any;

  connect<_, unknown, {}, _>(mapState);
}

function case3() {
  declare function f<T>(g: () => T): T;
  declare const o: {a: string};

  const x = f(o); // error: `o` is not a function
  const y: {a: string} = {...x};
}

function case4() {
  declare var Base: any;

  class State<T> extends Base {
    data: T;
  }

  new State();
}

function case5() {
  type CA<T> = ReadonlyArray<CA<T>> | T;

  declare function map<T>(c: CA<T>, fn: (child: T) => void): void;
  declare const x: any;

  map(x, child => child as string); // error (2): `child` is `unknown`; callback must return `void`
}
