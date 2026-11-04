// A value of one type alias checked against an application of another alias that expands to a
// type mentioning `T` gives `T` a lower bound.
type Column<U> = Array<U>;
type Props<T> = {column: Column<T>, focus: boolean};
declare function f<T>(p: Pick<Props<T>, 'column'>): T;

declare const aliased: Column<number>;
const r = f({column: aliased});
r as number; // ok
r as empty; // error: number ~> empty

declare const plain: Array<number>;
const s = f({column: plain});
s as number; // ok
