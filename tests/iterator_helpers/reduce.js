/* @flow */
declare const iterator: IteratorObject<number>;
declare const numberReducer: (acc: number,  value: number) => number;
declare const stringReducer: (acc: string,  value: number) => string;
declare const flexibleReducer: (acc: string | number,  value: number) => string;

// Basic usage
iterator.reduce(numberReducer) as number; // OK
iterator.reduce(flexibleReducer) as string | number; // ERROR: without an initial value, the reducer must return a number
iterator.reduce(stringReducer, '') as string; // OK

// Return type is discarded: https://tc39.es/proposal-iterator-helpers/#sec-iteratorprototype.reduce
// "ii. If value is done, return undefined."
declare const iteratorWithReturn: $Iterator<number, number, void>;
iteratorWithReturn.map(numberReducer) as $Iterator<number, void, void>; // OK
