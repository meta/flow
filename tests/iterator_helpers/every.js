/* @flow */
declare const iterator: IteratorObject<number>;
declare const predicate: (value: number) => boolean;

// Basic usage
iterator.every(predicate) as boolean; // OK
