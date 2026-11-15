// A generic value among the props of an object literal argument is instantiated after the other
// arguments and the hint, as one that is an argument: `getUniformValue` is instantiated with
// `TProps` = `void`, which the omitted `props` passes, rather than with the upper bound `unknown`
// that `thatReturnsNull` gives it, which made `TOutput` `unknown`.
declare class UniformValue<T> {
  getValue(): T;
}
declare class EmptyValue<T> {}
type BulkValue<T> = UniformValue<T> | EmptyValue<T>;

declare function getUniformValue<TValue, TDefault>(
  value: BulkValue<TValue>,
  mixedFallbackValue: TDefault,
): TValue | TDefault | void;
declare const thatReturnsNull: (...args: ReadonlyArray<unknown>) => null;

type Handlers<in TValue, in TProps, out TOutput> = Readonly<{
  loaded: (value: TValue, props: TProps) => TOutput,
  loading: (value: ?TValue, error: ?Error, props: TProps) => TOutput,
}>;
declare class LoadObject<out TValue> {
  match<TProps, TOutput>(handlers: Handlers<TValue, TProps, TOutput>, props: TProps): TOutput;
}
declare const loadObject: LoadObject<BulkValue<?string>>;

loadObject.match({loading: thatReturnsNull, loaded: getUniformValue}) as ?string; // was an error
loadObject.match({loading: thatReturnsNull, loaded: getUniformValue}) as ?number; // error
