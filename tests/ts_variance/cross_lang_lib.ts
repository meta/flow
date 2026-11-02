/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// A .ts-defined generic with a read-write Neutral tparam slot. Its loose
// strictness is retained when the type crosses a module boundary.

export class Animal {}
export class Dog extends Animal {}

export declare class Box<T> {
  value: T;
}

export declare class InvariantBox<in out T> {
  value: T;
}

export type AnimalMethod = {
  cb(x: Animal): void;
};

export type DogMethod = {
  cb(x: Dog): void;
};

export type ReadonlyValue = {readonly value: string};
export type MutableValue = {value: string};

export type MutableTuple = [string];
export type ReadonlyTuple = readonly [string];

export type WideValue = {value: Animal};
export type NarrowValue = {value: Dog};

export type PresentValue = {a: Animal};
export type OptionalValues = {a?: Animal; b?: Animal};
