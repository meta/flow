// @flow
import {A, B, C, type Statics} from 'lib';

const inherited: keyof typeof B = 'clone'; // ok
const own: keyof typeof B = 'extra'; // ok
const twoLevels: keyof typeof C = 'getType'; // ok
const missing: keyof typeof C = 'nope'; // error

declare const statics: Statics<typeof C>;
statics.clone as () => A; // ok
statics.extra as () => void; // ok
statics.nope; // error

const spread = {...B};
spread.count as number; // ok

class FlowA {
  static x: number = 1;
}
class FlowB extends FlowA {}
const flowInherited: keyof typeof FlowB = 'x'; // error: a Flow class's keys are its own statics
