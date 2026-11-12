/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

declare class TsAnimal {}
declare class TsDog extends TsAnimal {
  bark(): void;
}

declare class TsBuiltinBox<T> {
  value: T;
}

declare var flowDogTsBox: TsBuiltinBox<FlowDog>;
declare var tsDogFlowBox: FlowBuiltinBox<TsDog>;

interface TsBaseMap {
  mousedown: number;
}
interface TsDerivedMap extends TsBaseMap {
  custom: string;
}
declare var tsKeys: keyof TsDerivedMap;

interface TsBaseMethods {
  inherited(): void;
}
interface TsMethods extends TsBaseMethods {
  own(): void;
}
declare var tsMethodKeys: keyof TsMethods;

interface TsListener {
  on<K extends keyof TsDerivedMap>(type: K, value: TsDerivedMap[K]): void;
  on(type: string, value: string): void;
}
declare var tsListener: TsListener;

declare function tsFn(x: number): void;
declare namespace tsFn {
  let f: number;
}
declare class TsStatics {
  static s: number;
}
declare namespace TsNs {
  let v: number;
}
interface TsIface {
  x: number;
}
declare var tsIfaceClass: Class<TsIface>;
interface MergedIface {
  y: string;
}
declare function over(x: string): void;
declare namespace over {
  let f: number;
}

declare class TsBuiltinNode {
  static clone(data: unknown): TsBuiltinNode;
}
