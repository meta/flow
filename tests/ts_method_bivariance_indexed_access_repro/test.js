import type {I, NodeStatics} from 'repro';
import {InputEv, Node, registerNodes} from 'repro';

// tsc accepts: a method has bivariant params, also read through `I['m']`.
const m: I['m'] = (x: InputEv) => {};

class EmojiNode extends Node {
  static clone(node: EmojiNode): EmojiNode {
    return new EmojiNode();
  }
}
// tsc accepts: `clone` stays a method through the mapped type.
const statics: NodeStatics = EmojiNode;
// tsc accepts, as in Lexical's `createEditor({nodes: [EmojiNode]})`.
registerNodes([EmojiNode]);
