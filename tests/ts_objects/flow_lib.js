// @flow

export declare class FlowBox {
  v: number;
}

export type FlowExactBox = {v: number};
export type FlowExactEmpty = {};

export declare function flowFunction(): void;

export type FlowExtra = {a: number, b: string};
export type FlowJustA = {a: number};
export declare const flowExtra: FlowExtra;

export declare class FlowPoint {
  x: number;
  y: number;
}

export type FlowPointShape = {x: number, y: number};
export declare const flowPointShape: FlowPointShape;
