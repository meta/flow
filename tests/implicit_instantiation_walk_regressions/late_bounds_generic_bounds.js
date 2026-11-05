import * as React from 'react';

function case1() {
  type Other<T> = Readonly<{onHide: () => void, ...$Exact<T>}>;

  declare function useModal<T extends {...}, R extends Readonly<{...$Exact<T>}>>(
    c: (Other<T>) => void,
    props: R,
  ): void;
  declare const c: (Omit<Readonly<{onHide: () => void, s: string, r: string}>, 'r'>) => void;

  useModal(c, {s: 'x', onHide: () => {}});
}

function case2() {
  declare function memoize<
    TArgs extends ReadonlyArray<unknown>,
    TReturn,
    Fn extends (...TArgs) => TReturn,
  >(f: Fn, hash: (...TArgs) => string): Fn;

  type Obj = {kind?: ?string, ...};

  declare function filter<T extends Obj>(xs: Array<T>): Array<T>;

  memoize(filter, (s: ReadonlyArray<Obj>) => '');
}

function case3() {
  type Base = Readonly<{id: string, meta?: ?string}>;
  type A = Readonly<{...Base, items: ReadonlyArray<number>}>;
  type B = Readonly<{...Base, text: string}>;

  declare function makeNode<T extends Base | A | B>(node: Omit<T, 'id'>): T;

  function getA(items: ReadonlyArray<number>): A {
    return makeNode({items});
  }

  declare function makeNode2<T>(node: Omit<T, 'id'>): T;

  function getC(items: Array<number>): {id: string, items: Array<number>} {
    return makeNode2({items});
  }
}

function case4() {
  type Router = {history: string};

  declare function withRouter<Config extends {...}>(
    W: component(...{...$Exact<Config>, ...Router}),
  ): component(...$Exact<Config>);

  type OwnProps = Readonly<{...Router, a: number}>;
  type Props = Readonly<{...OwnProps, b: string}>;

  declare class Connected<OP> extends React.Component<OP> {}
  declare class Page extends React.Component<Props> {}
  declare var C: Class<Connected<OwnProps>> & typeof Page;

  withRouter(C) as component(...Omit<OwnProps, keyof Router>);
}
