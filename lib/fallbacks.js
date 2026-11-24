/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * Copyright (c) Microsoft Corporation. All rights reserved.
 * Modifications Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy of the
 * License at http://www.apache.org/licenses/LICENSE-2.0
 * THIS CODE IS PROVIDED ON AN *AS IS* BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, EITHER EXPRESS OR IMPLIED, INCLUDING WITHOUT LIMITATION ANY IMPLIED
 * WARRANTIES OR CONDITIONS OF TITLE, FITNESS FOR A PARTICULAR PURPOSE,
 * MERCHANTABILITY OR NON-INFRINGEMENT.
 * See the Apache Version 2.0 License for specific language governing permissions
 * and limitations under the License.
 *
 * @flow
 */
// @lint-ignore-every LICENSELINT

// Flow's fallback overloads for methods of the TypeScript libraries. This file is merged before
// them, so these overloads are tried after theirs.

interface ObjectConstructor {
  hasOwn(obj: $NotNullOrVoid, prop: unknown): boolean;
}

interface Array<T> {
  toSpliced<S>(start: number, deleteCount?: number, ...items: S[]): (T | S)[];
}

interface ReadonlyArray<T> {
  toSpliced<S>(start: number, deleteCount?: number, ...items: S[]): (T | S)[];
}

interface Promise<out T = unknown> {
  finally(onfinally?: (() => unknown) | undefined | null): Promise<T>;
}

interface String {
  matchAll(regexp: string | RegExp): RegExpStringIterator<RegExpExecArray>;
}

interface Float16Array<out TArrayBuffer extends ArrayBufferLike = ArrayBufferLike> {
  forEach(callbackfn: (value: number, index: number, array: this) => unknown, thisArg?: any): void;
}

interface BigInt64Array<out TArrayBuffer extends ArrayBufferLike = ArrayBufferLike> {
  forEach(callbackfn: (value: bigint, index: number, array: this) => unknown, thisArg?: any): void;
}

interface BigUint64Array<out TArrayBuffer extends ArrayBufferLike = ArrayBufferLike> {
  forEach(callbackfn: (value: bigint, index: number, array: this) => unknown, thisArg?: any): void;
}

interface IteratorObject<out T, out TReturn = void, in TNext = void> {
  forEach(callbackfn: (value: T, index: number) => unknown): void;
}

interface Int8Array<out TArrayBuffer extends ArrayBufferLike = ArrayBufferLike> {
  forEach(callbackfn: (value: number, index: number, array: this) => unknown, thisArg?: any): void;
}

interface Uint8Array<out TArrayBuffer extends ArrayBufferLike = ArrayBufferLike> {
  forEach(callbackfn: (value: number, index: number, array: this) => unknown, thisArg?: any): void;
}

interface Uint8ClampedArray<out TArrayBuffer extends ArrayBufferLike = ArrayBufferLike> {
  forEach(callbackfn: (value: number, index: number, array: this) => unknown, thisArg?: any): void;
}

interface Int16Array<out TArrayBuffer extends ArrayBufferLike = ArrayBufferLike> {
  forEach(callbackfn: (value: number, index: number, array: this) => unknown, thisArg?: any): void;
}

interface Uint16Array<out TArrayBuffer extends ArrayBufferLike = ArrayBufferLike> {
  forEach(callbackfn: (value: number, index: number, array: this) => unknown, thisArg?: any): void;
}

interface Int32Array<out TArrayBuffer extends ArrayBufferLike = ArrayBufferLike> {
  forEach(callbackfn: (value: number, index: number, array: this) => unknown, thisArg?: any): void;
}

interface Uint32Array<out TArrayBuffer extends ArrayBufferLike = ArrayBufferLike> {
  forEach(callbackfn: (value: number, index: number, array: this) => unknown, thisArg?: any): void;
}

interface Float32Array<out TArrayBuffer extends ArrayBufferLike = ArrayBufferLike> {
  forEach(callbackfn: (value: number, index: number, array: this) => unknown, thisArg?: any): void;
}

interface Float64Array<out TArrayBuffer extends ArrayBufferLike = ArrayBufferLike> {
  forEach(callbackfn: (value: number, index: number, array: this) => unknown, thisArg?: any): void;
}
