/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

declare var undefined: void;

type TypedPropertyDescriptor<T> = any;

declare class Object {}

declare class Function {}

interface CallableFunction extends Function {}

declare class Boolean {}

declare class Number {}

declare class String {
  @@iterator(): Iterator<string>;
}

declare class RegExp {}

declare class ReadonlyArray<out T> {
  @@iterator(): Iterator<T>;
}
type $ReadOnlyArray<out T> = ReadonlyArray<T>;
declare class Array<T> extends ReadonlyArray<T> {
  constructor(arrayLength?: number): void;
}

interface ArrayLike<out T> {
  readonly [indexer: number]: T,
  readonly length: number,
}
type $ArrayLike<T> = ArrayLike<T>;

interface TaggedTemplateLiteralArray extends ReadonlyArray<string> {
  readonly raw: ReadonlyArray<string>;
}

// Promise

interface Promise<out R> {}

interface PromiseConstructor {}

declare var Promise: PromiseConstructor;

// Iterable/Iterator/Generator

interface Iterator<out Yield,out Return=void,in Next=void> {
  @@iterator(): Iterator<Yield,Return,Next>;
}
type $IteratorProtocol<out Yield,out Return=void,in Next=void> = Iterator<Yield,Return,Next>;
type $Iterator<out Yield,out Return,in Next> = Iterator<Yield,Return,Next>;

interface Iterable<out Yield,out Return=void,in Next=void> {
  @@iterator(): Iterator<Yield,Return,Next>;
}
type $Iterable<out Yield,out Return,in Next> = Iterable<Yield,Return,Next>;

interface Generator<out Yield,out Return,in Next> {
  @@iterator(): Iterator<Yield,Return,Next>;
}

declare function $iterate<T>(p: Iterable<T>): T;

// Async Iterable/Iterator/Generator

interface AsyncIterator<out Yield,out Return=void,in Next=void> {
  @@asyncIterator(): AsyncIterator<Yield,Return,Next>;
}
type $AsyncIterator<out Yield,out Return,in Next> = AsyncIterator<Yield,Return,Next>;

interface AsyncIterable<out Yield,out Return=void,in Next=void> {
  @@asyncIterator(): AsyncIterator<Yield,Return,Next>;
}
type $AsyncIterable<out Yield,out Return,in Next> = AsyncIterable<Yield,Return,Next>;

interface AsyncGenerator<out Yield,out Return,in Next> {
  @@asyncIterator(): AsyncIterator<Yield,Return,Next>;
}

/* Type used internally for inferring the type of the yield delegate */
type $IterableOrAsyncIterableInternal<Input, out Yield, out Return, in Next> =
  Input extends AsyncIterable<any, any, any>
    ? AsyncIterable<Yield, Return, Next>
    : Iterable<Yield, Return, Next>;

declare opaque type $Flow$ModuleRef<out T>;
declare opaque type $Flow$EsmModuleMarkerWrapperInModuleRef<out T>: T;
declare opaque type React$CreateElement;

declare var module: {
  exports: any,
  ...
};

declare var exports: {writeonly [key: string]: unknown};

declare module 'react' {
  type Node = any;
  type RefSetter<T> = any;
}

/**
 * You can use this type instead of `any` to avoid triggering `unclear-type` error.
 * However, it's still a clear signal that you should use a better type.
 */
type $FlowFixMe = any;
