// @flow

import {
  BaseNode,
  takeAB,
  takeNodes,
  takeNumbers,
  takeRows,
  takeUnknowns,
  tsNodes,
  type Node,
  type PartialNode,
} from 'repro';

declare const flowNodes: {[string]: Node};
declare const node: {type: string, version: number};
declare const rows: Array<{name: string, count: number}>;
declare const abc: {a: number, b: number, c: number};

// Like TypeScript, a mapped type declared in TypeScript builds a TypeScript
// object: its values are checked covariantly and it is not exact.
takeNodes(flowNodes); // OK
takeUnknowns(node); // OK
takeRows(rows); // OK
tsNodes as {[string]: PartialNode | Node}; // OK
takeAB(abc); // OK

class MyNode extends BaseNode {
  static importJSON(json: {name: string, type: string, version: number}): MyNode { // OK
    return new MyNode();
  }
  exportJSON(): {...Node, name: string} { // OK
    return {type: 'my', version: 1, name: ''};
  }
}

// Values that are incompatible with the mapped type are still errors.
declare const strings: {[string]: string};
declare const a: {a: number};
takeNumbers(strings); // ERROR: string vs number
takeNumbers(node); // ERROR: `type` is a string
takeAB(a); // ERROR: `b` is missing

// Mapped types declared in Flow keep Flow's checks.
type FlowDict<K extends PropertyKey, T> = {[P in K]: T};
flowNodes as {[string]: PartialNode | Node}; // ERROR: invariant
flowNodes as FlowDict<string, PartialNode | Node>; // ERROR: invariant
abc as FlowDict<'a' | 'b', number>; // ERROR: `c` is extra

// A homomorphic mapped type has the strictness of its declaration, not of the
// object or array it maps.
import type {Mapped, NodeArray, NodePair, NodePairInterface, OmittedNodes} from 'repro';

type FlowMapped<O> = {[K in keyof O]: O[K]};
declare const tsMapped: Mapped<{a: Node, b: Node}>;
declare const flowMapped: FlowMapped<NodePair>;
declare const flowMappedArray: FlowMapped<NodeArray>;
declare const flowMappedInterface: FlowMapped<NodePairInterface>;
tsMapped as {a: PartialNode | Node, b: Node}; // OK
flowMapped as {a: PartialNode | Node, b: Node}; // ERROR: invariant
flowMappedArray as Array<PartialNode | Node>; // ERROR: invariant
flowMappedInterface as interface {a: PartialNode | Node, b: Node}; // ERROR: invariant

// `Omit` keeps the strictness of the object it omits from.
declare const tsOmitted: OmittedNodes;
declare const flowOmitted: Omit<{a: Node, b: Node, c: Node}, 'c'>;
tsOmitted as {a: PartialNode | Node, b: Node}; // OK
flowOmitted as {a: PartialNode | Node, b: Node}; // ERROR: invariant
