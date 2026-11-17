/* A function is arguably compatible with both branches of the union below, but
 * we should choose the first branch, even without annotations on the function
 * parameter and return types. */
type T<X> = ((x: X) => X) | X;
type D = { [key: string]: number };
(x => x) as T<D>;
