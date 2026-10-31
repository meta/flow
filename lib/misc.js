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
declare var undefined: void;

type $NotNullOrVoid =
| number
| string
| boolean
| interface {}
| ReadonlyArray<unknown>
| symbol
| bigint
| EnumValue<>
| Enum<>;

declare class CallSite {
    getThis(): any;
    getTypeName(): string;
    getFunction(): ?((...any) => any);
    getFunctionName(): string;
    getMethodName(): string;
    getFileName(): ?string;
    getLineNumber(): ?number;
    getColumnNumber(): ?number;
    getEvalOrigin(): ?CallSite;
    getScriptNameOrSourceURL(): ?string;
    isToplevel(): boolean;
    isEval(): boolean;
    isNative(): boolean;
    isConstructor(): boolean;
    toString(): string;
}

declare function $iterate<T>(p: Iterable<T>): T;

declare class ReadonlyWeakMap<K extends WeaklyReferenceable, out V> {
    get(key: K): V | void;
    has(key: K): boolean;
}
type $ReadOnlyWeakMap<K extends WeaklyReferenceable, out V> = ReadonlyWeakMap<K, V>;


declare class ReadonlyWeakSet<T extends WeaklyReferenceable> {
    has(value: T): boolean;
}
type $ReadOnlyWeakSet<T extends WeaklyReferenceable> = ReadonlyWeakSet<T>;

/* CommonJS */

declare var global: any;

declare var module: {
    exports: any,
    require(id: string): any,
    id: string,
    filename: string,
    loaded: boolean,
    parent: any,
    children: Array<any>,
    path: string,
    paths: Array<string>,
    isPreloading: boolean,
    ...
};
declare var require: {
    (id: string): any,
    resolve: (id: string, options?: { paths?: Array<string>, ... }) => string,
    cache: any,
    main: typeof module,
    ...
};
declare var exports: {writeonly [key: string]: unknown};

/* Opaque type for module reference magic strings */
declare opaque type $Flow$ModuleRef<out T>;
declare opaque type $Flow$EsmModuleMarkerWrapperInModuleRef<out T>: T;
type $EnumProto<TEnum, TEnumValue, TRepresentationType> = {
  cast(this: TEnum, input: ?TRepresentationType): void | TEnumValue,
  getName(this: TEnum, input: TEnumValue): string,
  isValid(this: TEnum, input: ?TRepresentationType | TEnumValue): boolean,
  members(this: TEnum): IteratorObject<TEnumValue>,
  __proto__: null,
}

/**
 * String type whose values begin with the literal prefix `P`. The optional
 * second argument constrains what may follow the prefix.
 *
 * `StringPrefix<'data-'>` matches any string starting with `'data-'`.
 * `StringPrefix<'data-', 'foo' | 'bar'>` matches `'data-foo'` and `'data-bar'`.
 */
type StringPrefix<out P extends string, out R extends string = string> = `${P}${R}`;

/**
 * String type whose values end with the literal suffix `S`. The optional
 * second argument constrains what may precede the suffix.
 *
 * `StringSuffix<'-end'>` matches any string ending with `'-end'`.
 * `StringSuffix<'-end', 'foo' | 'bar'>` matches `'foo-end'` and `'bar-end'`.
 */
type StringSuffix<out S extends string, out R extends string = string> = `${R}${S}`;

/**
 * The possible underlying representation types of a Flow Enum.
 * For example, `enum E {A = 1}` has the representation type `number`.
 */
type EnumRepresentationTypes = string | number | symbol | boolean | bigint;

/**
 * A generic Flow Enum value. `EnumValue<>` represents any Flow Enum value.
 * You can supply a type argument to restrict to Flow Enums of a certain
 * representation type. For example:
 * ```
 * enum E {A = 1}
 * const b: EnumValue<number> = E.A; // Works
 * const a: EnumValue<> = E.A; // Works
 * ```
 */
type EnumValue<
  out TRepresentationType extends EnumRepresentationTypes = EnumRepresentationTypes
> = $EnumValue<TRepresentationType>;

type $EnumValueProto<TEnumObject, TRepresentationType> = {
  /**
   * Casts the Flow Enum value to its representation type.
   */
  valueOf(this: TEnumObject): TRepresentationType,
  __proto__: null,
};

/**
 * Represents a generic Flow Enum - the enum itself rather than its values.
 * You can supply a type argument to restrict to Flow Enums with a certain
 * enum value (take a look at the `EnumValue` type for more).
 * You can use the Flow Enum methods like `.cast` and `.members` on a value
 * of this type, but it does not have any specific members which you can access.
 */
type Enum<out TEnumValue extends EnumValue<> = EnumValue<>> = $Enum<TEnumValue>;

type Object$Assign = (target: any, ...source: ReadonlyArray<any>) => any;

/**
 * You can use this type instead of `any` to avoid triggering `unclear-type` error.
 * However, it's still a clear signal that you should use a better type.
 */
type $FlowFixMe = any;

type WithImplicitCoercion<T> = T | { valueOf(): T, ... };

type DistributiveOmit<T, K extends PropertyKey> = T extends any ? Omit<T, K> : empty;
type TupleIterator<out T> = IteratorObject<T>;

// Aliases for the well-known symbols, retained so libdefs that reference these
// names keep resolving. Each maps to the corresponding `Symbol.*` member type.
type $SymbolHasInstance = typeof Symbol.hasInstance;
type $SymbolIsConcatSpreadable = typeof Symbol.isConcatSpreadable;
type $SymbolIterator = typeof Symbol.iterator;
type $SymbolMatch = typeof Symbol.match;
type $SymbolMatchAll = typeof Symbol.matchAll;
type $SymbolReplace = typeof Symbol.replace;
type $SymbolSearch = typeof Symbol.search;
type $SymbolSpecies = typeof Symbol.species;
type $SymbolSplit = typeof Symbol.split;
type $SymbolToPrimitive = typeof Symbol.toPrimitive;
type $SymbolToStringTag = typeof Symbol.toStringTag;
type $SymbolUnscopables = typeof Symbol.unscopables;
type $SymbolDispose = typeof Symbol.dispose;
type $SymbolAsyncDispose = typeof Symbol.asyncDispose;

type $SharedIntegerTypedArray =
  | Int8Array<ArrayBufferLike>
  | Uint8Array<ArrayBufferLike>
  | Int16Array<ArrayBufferLike>
  | Uint16Array<ArrayBufferLike>
  | Int32Array<ArrayBufferLike>
  | Uint32Array<ArrayBufferLike>;

type Object$FromEntries = <K, V>(entries: Iterable<[K, V] | {
    '0': K,
    '1': V,
    ...
  }>) => { [K]: V, ... };

interface ObjectConstructor {
  hasOwn(obj: $NotNullOrVoid, prop: unknown): boolean;
}

interface Array<T> {
  toSpliced<S>(start: number, deleteCount?: number, ...items: S[]): (T | S)[];
}

interface ReadonlyArray<T> {
  toSpliced<S>(start: number, deleteCount?: number, ...items: S[]): (T | S)[];
}

interface Promise<R> {
  finally(onfinally?: (() => unknown) | undefined | null): Promise<R>;
}

interface String {
  matchAll(regexp: string | RegExp): RegExpStringIterator<RegExpMatchArray>;
}

interface SymbolConstructor {
  readonly dispose: '@@dispose';
  readonly asyncDispose: '@@asyncDispose';
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

interface Proxy<T> {}

type Proxy$traps<T> = ProxyHandler<T & object>;

type Proxy$revocable<T> = {
  proxy: T,
  revoke(): void,
  ...
};

interface ReadonlyMap<K, out V> {
  readonly [Symbol.toStringTag]: string;
}

interface ReadonlySet<out T> {
  readonly [Symbol.toStringTag]: string;
}

interface ReadonlyWeakMap<K extends WeaklyReferenceable, out V> {
  readonly [Symbol.toStringTag]: string;
}

interface ReadonlyWeakSet<T extends WeaklyReferenceable> {
  readonly [Symbol.toStringTag]: string;
}

type $AsyncIterator<Yield, Return, Next> = AsyncIterator<Yield, Return, Next>;
type $AsyncIterable<Yield, Return, Next> = AsyncIterable<Yield, Return, Next>;

type $SettledPromiseResult<T> = PromiseSettledResult<T>;

interface SymbolConstructor {
  readonly asyncIterator: '@@asyncIterator';
}

interface Promise<R> {
  then<TResult1 = R, TResult2 = empty>(
    onfulfilled?: ((value: R) => TResult1 | PromiseLike<TResult1>) | null | void,
    onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null | void,
  ): Promise<TResult1 | TResult2>;
}

interface Generator<out T = unknown, out TReturn = any, in TNext = any> {}

type $Generator<out Yield, out Return, in Next> = Generator<Yield, Return, Next>;

interface Map<K, V> extends ReadonlyMap<K, V> {
  getOrInsert(key: K, value: V): V;
  getOrInsertComputed(key: K, callbackfn: (key: K) => V): V;
}

type $ReadOnlyMap<K, out V> = ReadonlyMap<K, V>;

interface WeakMap<K extends WeakKey, V> extends ReadonlyWeakMap<K, V> {
  getOrInsert(key: K, value: V): V;
  getOrInsertComputed(key: K, callbackfn: (key: K) => V): V;
}

interface Set<T> extends ReadonlySet<T> {}

type $ReadOnlySet<out T> = ReadonlySet<T>;

interface WeakSet<T extends WeakKey> extends ReadonlyWeakSet<T> {}

interface SymbolConstructor {
  readonly iterator: '@@iterator';
}

interface Iterator<out T, out TReturn = void, in TNext = void> {}

interface Iterable<out T, out TReturn = void, in TNext = void> {}

interface IterableIterator<out T, out TReturn = void, in TNext = void> {}

interface ArrayIterator<out T> {}

interface MapIterator<out T> {}

interface SetIterator<out T> {}

interface StringIterator<out T> {}

type $IteratorProtocol<out Yield, out Return = void, in Next = void> = Iterator<Yield, Return, Next>;

type $Iterator<out Yield, out Return, in Next> = IteratorObject<Yield, Return, Next>;

type $Iterable<out Yield, out Return, in Next> = Iterable<Yield, Return, Next>;

interface IteratorObject<out T, out TReturn = void, in TNext = void> {
  forEach(callbackfn: (value: T, index: number) => unknown): void;
}

interface Error {
  // note: microsoft only
  description?: string;
  number?: number;

  // note: mozilla only
  fileName?: string;
  lineNumber?: number;
  columnNumber?: number;
}

interface ErrorConstructor {
  // note: v8 only (node/chrome)
  captureStackTrace(target: interface { [any] : any }, constructor?: any): void;

  stackTraceLimit: number;
  prepareStackTrace: (err: Error, stack: CallSite[]) => unknown;
}

interface RegExpConstructor {
  escape(string: string): string;
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

interface ArrayBufferView<out TArrayBuffer extends ArrayBufferLike = ArrayBufferLike> {}

interface DataView<out TArrayBuffer extends ArrayBufferLike = ArrayBufferLike> {}
