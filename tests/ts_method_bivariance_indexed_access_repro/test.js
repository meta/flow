import type {Emitter, EvCtor, I, J, NodeStatics} from 'repro';
import {Ev, InputEv, Node, registerNodes} from 'repro';

// tsc accepts: a method has bivariant params, also read through `I['m']`.
const m: I['m'] = (x: InputEv) => {};
// tsc accepts: a param is compared with the rest element at its position.
const on: Emitter['on'] = (e: string) => {};
// tsc errors: a function-typed property is not a method.
const f: J['f'] = (x: InputEv) => {}; // ERROR
// tsc errors: a construct signature is not a method.
class InputCtor extends Ev {
  constructor(x: InputEv) {
    super();
  }
}
const ctor: EvCtor = InputCtor; // ERROR
// Flow methods keep contravariant params.
interface FlowI {
  m(x: Ev): void;
}
const flowM: FlowI['m'] = (x: InputEv) => {}; // ERROR

class EmojiNode extends Node {
  static clone(node: EmojiNode): EmojiNode {
    return new EmojiNode();
  }
}
// tsc accepts: `clone` stays a method through the mapped type.
const statics: NodeStatics = EmojiNode;
// tsc accepts, as in Lexical's `createEditor({nodes: [EmojiNode]})`.
registerNodes([EmojiNode]);
