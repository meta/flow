declare class FlowAnimal {}
declare class FlowDog extends FlowAnimal {
  bark(): void;
}

declare class FlowBuiltinBox<T> {
  value: T;
}

declare interface FlowBaseMap {
  mousedown: number;
}
declare interface FlowDerivedMap extends FlowBaseMap {
  custom: string;
}
// $FlowFixMe[deprecated-utility]
declare var flowKeys: $Keys<FlowDerivedMap>;

declare class FlowStatics {
  static s: number;
}
declare namespace FlowNs {
  declare var v: number;
}
declare interface MergedIface {
  arr: Array<number>;
}
declare function over(x: number): void;

declare class FlowBuiltinNode {
  static clone(data: unknown): FlowBuiltinNode;
}
