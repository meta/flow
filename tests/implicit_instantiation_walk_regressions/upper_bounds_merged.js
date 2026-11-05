import * as React from 'react';

function reversal() {
  type Props = Readonly<{a: number, b: string, ...}>;

  component C(a: number, b: string) {
    return null;
  }

  function eq(x: Props, y: Props): boolean {
    return x.a === y.a;
  }

  // The config of `C`, an upper bound from a reversal, is more specific than `Props`.
  React.memo(C, eq);
}

function any() {
  type Sel<out V, in A = unknown> = {(v: A): V, ...};

  declare function structured<
    SelectArg,
    StrictSelectors extends {readonly [key: string]: Sel<unknown, SelectArg>},
  >(obj: StrictSelectors): Sel<unknown, SelectArg>;

  type Group = {readonly a: number, ...};
  type Item = {readonly a: number, readonly b: string, ...};

  declare const ids: Sel<Array<string>, Group>;
  declare const name: Sel<string, Item>;
  declare const anything: Sel<boolean, any>;

  // An upper bound of `any` bounds nothing: `SelectArg` is `Item` in either order.
  structured({name, anything, ids});
  structured({ids, anything, name});
}
