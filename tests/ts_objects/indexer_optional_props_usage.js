// @flow
//
// Cross-file: `.js` consumer of object types declared in
// `indexer_optional_props_lib.ts`. Like TypeScript, the source's indexer is not
// checked against optional properties of the target that the source doesn't
// declare. This is keyed on the target's declaration.

import type {AllOptional, SomeRequired} from './indexer_optional_props_lib';

declare const dict: {[key: string]: string | number};

// Optional `a` and `b` are not checked against the indexer.
dict as AllOptional; // OK

// Required `a` is still checked against the indexer; optional `b` is not.
dict as SomeRequired; // ERROR: number vs string in `a`

// Flow-declared targets keep checking the indexer against optional properties.
dict as {readonly a?: string, readonly b?: string, ...}; // ERROR: number vs string in `a` and `b`

// Properties the source declares are still checked.
({a: 1}) as AllOptional; // ERROR: number vs string in `a`
