// @flow

import type {Collapsed} from './tuple_spread';

declare const collapsed: Collapsed;
collapsed as Array<string | number | boolean>; // OK: a TypeScript array is not invariant
