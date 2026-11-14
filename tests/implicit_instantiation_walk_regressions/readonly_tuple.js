// `Readonly` of a tuple type meets what flows to it as the read-only tuple, whose elements are
// compared covariantly, as in subtyping: the type argument is the union of the elements that
// meet it, not the first of them.
declare function f<T>(o: ReadonlyArray<Readonly<[T, string, ?string]>>): T;
const bools = [[false, 'No', null], [true, 'Yes', null]] as const;
f(bools) as boolean; // ok
const nums = [[0, 'P0', null], [1, 'P1', null]] as const;
f(nums) as 0 | 1; // ok

// `Readonly` of an array type, and of an alias of a tuple type.
declare function g<T>(o: ReadonlyArray<Readonly<Array<T>>>): T;
declare const no: Array<false>;
declare const yes: Array<true>;
g([no, yes]) as boolean; // ok

type Row<T> = [T, string, ?string];
declare function h<T>(o: ReadonlyArray<Readonly<Row<T>>>): T;
h(bools) as boolean; // ok

// `Partial` of a tuple type: the partial tuple, whose elements are optional, so the type
// argument does not take their `void`.
declare class Obs<T> {}
declare function mk<T>(o: ?Partial<[T, string]>): Obs<T>;
function k<V>(o: Partial<[V, string]>): Obs<V> {
  return mk(o); // ok
}
