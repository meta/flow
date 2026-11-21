import type {M, N} from './ts_type_arg_growth';

declare const m: M<'x'>;
m as N<string>; // ok
m as N<'y'>; // error
