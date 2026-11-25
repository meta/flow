/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

declare class Base { base: number; }
interface Config<T extends Base> { node: T; }

// `this` in a method's type parameter bound is the class's `this`.
export declare class Node extends Base {
  config<C extends Config<this>>(config: C): C;
  // A static member and the members of an object type have no `this`.
  static create<T extends this>(x: T): T; // ERROR
  nested(): { get<T extends this>(x: T): T }; // ERROR
}

export declare class Sub extends Node { sub: string; }
