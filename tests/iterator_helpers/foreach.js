/* @flow */
declare const iterator: IteratorObject<number>;

// Basic usage
iterator.forEach((x: number): void => {}) as void; // OK
