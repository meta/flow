// @flow
//
// Cross-file: `.js` consumer of classes declared in `constructor_field_lib.ts`.
// The `constructor` field types `x.constructor`. It doesn't shadow the
// constructor that `new` and `super()` call.

import {
  AnyNode,
  BaseNode,
  CallableNode,
  MidNode,
  PlainTsNode,
  TabNode,
  TextNode,
  plainCtor,
} from './constructor_field_lib';
import type {Ctor} from './constructor_field_lib';

new BaseNode(); // OK
new TextNode('text'); // OK
new TabNode('tab'); // OK
new MidNode('mid'); // OK
new AnyNode(1); // OK
new TextNode(1); // ERROR: number vs string

class EmojiNode extends TextNode {
  constructor(text: string) {
    super(text); // OK
  }
}
new EmojiNode('x'); // OK

class BadNode extends TextNode {
  constructor() {
    super(1); // ERROR: number vs string
  }
}

declare const text: TextNode;
declare const emoji: EmojiNode;
text.constructor as Ctor<TextNode>; // OK
text.constructor as number; // ERROR
emoji.constructor as number; // ERROR: inherited field
text.getKey() as string; // OK

class FlowNode {
  ['constructor']: number;
  constructor(x: string) {}
}
new FlowNode('x'); // OK
new FlowNode('x').constructor as number; // OK

class PlainNode {}
new PlainNode().constructor as Class<PlainNode>; // OK
plainCtor as Class<PlainTsNode>; // OK

declare const callable: CallableNode;
callable.constructor() as number; // OK: calls the field
callable.constructor() as string; // ERROR: number

text as interface {constructor: Ctor<TextNode>}; // OK
text.constructor = TextNode; // OK: the field is writable
