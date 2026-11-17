/* @flow */
declare const iterator: IteratorObject<number>;
declare const predicate: (value: number) => boolean;

// Basic usage
iterator.find(predicate) as ?number; // OK

// Propagates type guards
declare const mixedIterator: IteratorObject<number | string>;
declare function guarded(x: number | string): implies x is number;
mixedIterator.find(guarded) as ?number; // OK
