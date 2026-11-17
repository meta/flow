declare class MySet<out T> {}

type $ElementOfSet<SetType> =
  SetType extends MySet<infer ElementType>
    ? ElementType
    : empty;

class Entry<out X> {}

type SetOfEntries = MySet<Entry<unknown>>;

function test1() {
  type FilterEntry = $ElementOfSet<SetOfEntries>;

  declare const x: MySet<FilterEntry>;
  x as SetOfEntries; // no error
}

function test2() {
  type FilterEntry = $ElementOfSet<SetOfEntries>;

  declare const y: SetOfEntries;
  y as MySet<FilterEntry>; // no error
}

/**
 * This is a regression test for IntersectionT normalization.
 * Without the normalization, we will hit the issue of TypeAppExpansion being too conservative.
 */
function test3() {
  declare opaque type Opaque<in T>;

  type ID<T> = T;

  declare function wrapWithId<P1 extends {...}>(x: Opaque<P1>): Opaque<ID<P1>>;

  declare function spreadIt<P2 extends {...}>(
    Component: Opaque<P2>,
    spec: (props: P2) => unknown
  ): Opaque<{...P2, ...}>;

  // The HOCs above are basically convoluted identity functions...

  type Foo = {readonly foo: string, ...};
  declare const OriginalComponent: Opaque<Foo>;

  const IdWrapped = wrapWithId(OriginalComponent);

  const Spreaded = spreadIt(
    IdWrapped,
    (p) => {},
  );

  Spreaded as Opaque<{...}>; // error: {} ~> Foo
}
