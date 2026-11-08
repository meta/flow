/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// TypeScript declarations route direct, quoted and computed names into signatures.
interface TSFractional {
  readonly 0.5: string; // OK
  '1.5': number; // OK
  '-0.5': boolean; // OK
  2.5(value: string): number; // OK
}
type TSFractionalObject = {
  0.5: string; // OK
  [1.5]: number; // OK
  '-0.5': boolean; // OK
};
declare const tsLibraryObject: TSFractionalObject;
declare function fractionalRoundtrip(value: TSFractionalObject): TSFractionalObject;

// Readonly fractional class fields and methods use the same canonical names.
declare class TSFractionalClass {
  readonly [0.5]: string; // OK
  '1.5': number; // OK
  '-0.5'(value: string): number; // OK
  static readonly '-1.5': boolean; // OK
}

// Finite exponent and rounded large tokens are canonicalized in type signatures.
interface TSFinite {
  5e-1: string; // OK
  1e-7: boolean; // OK
  1e21: number; // OK
  9007199254740993: string; // OK
}

// TypeScript homomorphic modifiers retain the declared property-name identity.
type TSIdentity<O> = {[K in keyof O]: O[K]}; // OK
type TSOptional<O> = {[K in keyof O]?: O[K]}; // OK
type TSReadonly<O> = {readonly [K in keyof O]: O[K]}; // OK
