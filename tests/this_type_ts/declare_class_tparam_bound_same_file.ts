/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

declare class Base { base: number; }
interface Config<T extends Base> { node: T; }

declare class Node extends Base {
  config<C extends Config<this>>(config: C): C;
}

declare class Sub extends Node { sub: string; }

declare const node: Node;
declare const sub: Sub;
sub.config({ node: sub }); // OK
sub.config({ node: node }); // ERROR

export {};
