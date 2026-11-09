// The return type `MySpec<A, B>` is an alias of an application of `Spec`: it gives `P` the type
// `Props<A, B>`, which instantiates the generic `renderer` with `A` and `B`. The expansion of
// `Spec` is a union, from the spread of `Result`.
type Std<R> = Readonly<{value: R}>;
type Renderer<R, P> = (props: P & Std<R>) => unknown;
type FieldRenderer<R, P> = (input: Omit<Readonly<{...P, ...Std<R>}>, keyof Std<R>>) => unknown;
type Result<V, E> = Readonly<{ok: true, value: V}> | Readonly<{ok: false, error: E}>;
type Metadata<R, V> = Readonly<{...Result<V, string>, rawValue: R, isModified: boolean}>;
type Actions<R, Rd> = Readonly<{render: Rd, setValue: (v: R) => unknown}>;
type FieldSpec<R, V, Rd> = Readonly<{...Metadata<R, V>, ...Actions<R, Rd>}>;
type BasicSpec<R> = FieldSpec<R, NonNullable<R>, (v: (x: R) => unknown) => unknown>;
type Spec<R, P> = Readonly<{...BasicSpec<R>, render: FieldRenderer<R, P>}>;
declare function useField<R, P>(def: R, r: Renderer<R, P>): Spec<R, P>;

type Props<A, B> = {a: A, cb: (Array<B>) => void};
declare function renderer<A, B>(props: Props<A, B>): unknown;

type MySpec<A, B> = Spec<?B, Props<A, B>>;
function use<A, B>(def: ?B): MySpec<A, B> {
  return useField(def, renderer); // ok
}
