// @flow

import type {Node} from 'repro';
import {spread} from './spread_export';

spread as {a: Node}; // OK: the spread of a TypeScript object is not exact
