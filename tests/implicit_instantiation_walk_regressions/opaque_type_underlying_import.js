import type {Op} from './opaque_type_underlying';

declare function f<T>(x: Op<T>): T;

// Elsewhere, the underlying type of an opaque type is hidden.
f({v: 1}) as empty; // error: the object is not an `Op`
