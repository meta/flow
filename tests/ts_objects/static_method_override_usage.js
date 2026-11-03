// @flow
//
// Cross-file: `.js` subclass of a class declared in
// `static_method_override_lib.ts`. Like TypeScript, an override of a method
// declared in TypeScript is checked bivariantly, on the static side as on the
// instance side.

import {Node} from './static_method_override_lib';

class TextNode extends Node {
  static clone(node: TextNode): TextNode { // OK
    return new TextNode();
  }
  static importJSON(json: {type: string, text: string}): TextNode { // OK
    return new TextNode();
  }
  update(node: TextNode): void {} // OK
}

class BadNode extends Node {
  static importJSON(json: number): BadNode { // ERROR: unrelated parameter
    return new BadNode();
  }
}

// Flow-declared classes keep checking overrides contravariantly.
declare class FlowNode {
  static clone(data: unknown): FlowNode;
}
class FlowTextNode extends FlowNode {
  static clone(node: FlowTextNode): FlowTextNode { // ERROR: narrower parameter
    return new FlowTextNode();
  }
}
