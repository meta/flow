/* @flow */
declare const iterator: IteratorObject<number>;
declare const predicate: (value: number) => boolean;

// Basic usage
iterator.some(predicate) as boolean; // OK
