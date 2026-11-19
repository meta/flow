// A function flowed to another function flows what it returns to what the other returns, so the
// literals in its return are kept precise where the other needs them: against an upper bound, and
// against the bound of its type parameter.
type Impl = 'native' | 'hybrid';
type Handle = Readonly<{getImpl: () => ?Impl}>;
declare function useHandle<T>(set: (instance: T) => void, create: () => T): void;
declare const set: (instance: Handle) => void;
useHandle(set, () => ({getImpl: () => 'hybrid'})); // ok
useHandle(set, () => ({getImpl: () => 'other'})); // error

type DispatchAPI<A> = (action: A) => A;
type ActionCreator<A, B> = (...args: Array<B>) => A;
declare function bind<A, C extends ActionCreator<A, any>, D extends DispatchAPI<A>>(
  creator: C,
  dispatch: D,
): C;
type Action = {type: 'A', ...} | {readonly type: 'B', ...};
declare const dispatch: DispatchAPI<Action>;
bind((n: number) => ({type: 'A'}), dispatch); // ok
