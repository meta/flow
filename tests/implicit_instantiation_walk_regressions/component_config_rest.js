import * as React from 'react';

type Inner<TProps extends {...}> = Readonly<{innerProps: TProps, onDone: () => void}>;
type Props<A, B = {}, C = {}, D = null> = Readonly<{a: A, b: B, c: C, d: D | null}>;
type Loader<in Params, out Comp> = Readonly<{getParams: (Params) => unknown, root: Comp}>;

declare component Child(...props: Props<{}, {}, Inner<{readonly content: React.Node}>>);
declare const loader: Loader<{}, typeof Child>;
declare hook useLoader<
  TParams extends {...},
  TA extends {...},
  TB extends {...},
  TInner extends {...},
>(
  loader: Loader<TParams, component(...Props<TA, TB, Inner<TInner>, null>)>,
  params: TParams,
  innerProps: TInner | null,
): void;

component Parent(content: React.Node) {
  useLoader(loader, {}, {content});
  return null;
}
