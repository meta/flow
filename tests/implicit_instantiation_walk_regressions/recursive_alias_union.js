// An alias that expands to an application of itself, as a member of a union.
type F<T> = F<Array<T>>;

declare function f<T>(x: F<number> | T): T;

f(1); // error: underconstrained, as in the flow-based solve
