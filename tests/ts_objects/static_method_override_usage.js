// @flow
//
// Cross-file: `.js` subclass of a class declared in
// `static_method_override_lib.ts`. Like TypeScript, an override of a method
// declared in TypeScript is checked bivariantly, on the static side as on the
// instance side.

import {ElementNode, Node} from './static_method_override_lib';
import type {NodeShape} from './static_method_override_lib';
import {TsMiddleNode} from './static_method_override_ts_middle';

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

// A Flow class in between doesn't change which member is overridden: it is
// still the one `Node` declares in TypeScript.
class WrapperNode extends ElementNode {}
class ConfiguredNode extends WrapperNode {
  static clone(node: WrapperNode): ConfiguredNode { // OK
    return new ConfiguredNode();
  }
  update(node: ConfiguredNode): void {} // OK
}
class BadConfiguredNode extends WrapperNode {
  static importJSON(json: number): BadConfiguredNode { // ERROR: unrelated parameter
    return new BadConfiguredNode();
  }
}

// A member that a Flow class redeclares is checked contravariantly in its
// subclasses.
class FlowWrapperNode extends ElementNode {
  static clone(node: FlowWrapperNode): FlowWrapperNode { // OK
    return new FlowWrapperNode();
  }
  update(node: FlowWrapperNode): void {} // OK
}
class NarrowerNode extends FlowWrapperNode {
  static clone(node: NarrowerNode): NarrowerNode { // ERROR: narrower parameter
    return new NarrowerNode();
  }
  update(node: NarrowerNode): void {} // ERROR: narrower parameter
}

// So is a member that a Flow class declares, even through a TypeScript class.
class FlowThroughTsNode extends TsMiddleNode {
  static clone(node: FlowThroughTsNode): FlowThroughTsNode { // ERROR: narrower parameter
    return new FlowThroughTsNode();
  }
  update(node: FlowThroughTsNode): void {} // ERROR: narrower parameter
}

// The same holds when the TypeScript super is an object type.
interface NodeShapeWithOwnCheck extends NodeShape {
  hasOwnProperty(key: number): boolean; // ERROR: `Object` declares it in Flow
}
