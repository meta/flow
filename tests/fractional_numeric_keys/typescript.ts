/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// TS source object literals accept direct, quoted, computed and signed fractions.
const direct = {0.5: 'half', '1.5': 15, [-0.5]: true}; // OK
const computed = {[5e-1]: 'half', [1.5]: 15, '-0.5': true}; // OK
direct as TSFractionalObject; // OK
computed as TSFractionalObject; // OK
direct['0.5'] as string; // OK
computed[1.5] as number; // OK
direct[-0.5] as boolean; // OK
direct[0.5] as number; // ERROR
computed['-0.5'] as string; // ERROR
computed[2.5]; // ERROR
({}) as TSFractionalObject; // ERROR

// Local TS interfaces include computed and readonly fractional fields.
interface Local {
  readonly '0.5': string; // OK
  [1.5]: number; // OK
  readonly '-0.5': boolean; // OK
}
declare const local: Local;
local[0.5] as string; // OK
local['1.5'] as number; // OK
local[-0.5] as boolean; // OK
local['0.5'] = 'new'; // ERROR
local[-0.5] = false; // ERROR
local['1.5'] = 'new'; // ERROR

// Ambient TS fields and methods survive the explicit library signature boundary.
declare const library: TSFractional;
library[0.5] as string; // OK
library['1.5'] as number; // OK
library[-0.5] as boolean; // OK
library['0.5'] = 'new'; // ERROR
library[2.5]('value') as number; // OK
library[2.5](1); // ERROR
fractionalRoundtrip(computed) as TSFractionalObject; // OK
fractionalRoundtrip({0.5: 'half', 1.5: 15}); // ERROR
fractionalRoundtrip({0.5: 5, 1.5: 15, '-0.5': true}); // ERROR
tsLibraryObject[0.5] as string; // OK
tsLibraryObject['0.5'] as number; // ERROR

// Declared TS classes distinguish instance and static fractional members.
declare const instance: TSFractionalClass;
instance['0.5'] as string; // OK
instance[1.5] as number; // OK
instance[-0.5]('value') as number; // OK
TSFractionalClass[-1.5] as boolean; // OK
instance[0.5] = 'new'; // ERROR
instance['-0.5'](1); // ERROR
instance[-1.5]; // ERROR

// Implemented TS classes use computed field and method names.
class Members {
  [0.5]: string = 'half'; // OK
  [1.5](value: number): number { return value; } // OK
}
const members = new Members(); // OK
members['0.5'] as string; // OK
members[1.5](15) as number; // OK
members[0.5] as number; // ERROR
members['1.5']('value'); // ERROR

// TS indexed access and keyof do not expose numeric-valued property keys.
type Numeric = Local[0.5]; // OK
type Quoted = Local['0.5']; // OK
'half' as Numeric; // OK
'half' as Quoted; // OK
5 as Numeric; // ERROR
'0.5' as keyof Local; // OK
'-0.5' as keyof Local; // OK
0.5 as keyof Local; // ERROR
Object.keys(direct) as ReadonlyArray<string>; // OK
Object.keys(direct) as ReadonlyArray<number>; // ERROR
'0.5' as number; // ERROR

// Mapped, optional and readonly TS types keep fractional properties and values.
declare const identity: TSIdentity<TSFractionalObject>;
identity as TSFractionalObject; // OK
identity[0.5] as string; // OK
identity[1.5] as string; // ERROR
declare const readonly: TSReadonly<TSFractionalObject>;
readonly['0.5'] as string; // OK
readonly[0.5] = 'new'; // ERROR
declare const builtinReadonly: Readonly<TSFractionalObject>;
builtinReadonly[-0.5] as boolean; // OK
builtinReadonly['-0.5'] = false; // ERROR
declare const optional: TSOptional<TSFractionalObject>;
optional[0.5] as string | undefined; // OK
optional['0.5'] as string; // ERROR
({}) as TSOptional<TSFractionalObject>; // OK
({0.5: 5}) as TSOptional<TSFractionalObject>; // ERROR
({1.5: 15, '-0.5': true}) as TSIdentity<TSFractionalObject>; // ERROR

// TypeScript declaration tokens use ECMAScript finite-number stringification.
declare const finite: TSFinite;
finite['0.5'] as string; // OK
finite['1e-7'] as boolean; // OK
finite['1e+21'] as number; // OK
finite['9007199254740992'] as string; // OK
finite['1e+21'] as string; // ERROR
finite['5e-1']; // ERROR
finite['1e21']; // ERROR
finite['9007199254740993']; // ERROR

// Numeric -0 and the distinct quoted '-0' remain distinct in TS sources.
const zero = {[-0]: 'zero', '-0': false}; // OK
zero[-0] as string; // OK
zero['-0'] as boolean; // OK
zero['-0'] as string; // ERROR
({'0.5': true}) as {readonly [key: number]: unknown}; // OK
declare const numericDictionary: {[key: number]: unknown};
numericDictionary['-0']; // ERROR
numericDictionary['5e-1']; // ERROR

// A JS module's exported annotation and inferred signature are usable from TS.
import {exported, roundtrip} from './signatures';
import type {NumericValue, QuotedValue} from './signatures';
exported[0.5] as string; // OK
exported['-1.5'] as boolean; // OK
roundtrip({0.5: 'half', 1.5: 15}); // OK
roundtrip({0.5: 'half'}); // ERROR
'half' as NumericValue; // OK
5 as QuotedValue; // ERROR
