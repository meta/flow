import type { Op } from './defs';
import { op } from './defs';

const obj = {[op]: 'abc'};

obj as Readonly<{[key: Op]: string}>;
obj as {[key: Op]: string};

obj as {[key: string]: string}; // error invariant position
obj as {['special']: string}; // error Op is opaque - no knowledge of specific value
