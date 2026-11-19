//@flow

import type {Config} from './exp.js';
declare function f<T>(): Config<T>; // error: T is not known to be an element type
