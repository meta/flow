// Pure TS: a class whose methods take wider parameters than the overrides in a
// `.js` subclass, the way Lexical's `static clone(node)` does.

export declare class Node {
  constructor(key?: string);
  static clone(data: unknown): Node;
  static importJSON(json: {type: string}): Node;
  update(node: Node): void;
}

export declare class ElementNode extends Node {}

export type NodeShape = {update(node: unknown): void};
