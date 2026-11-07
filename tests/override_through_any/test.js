// @flow
//
// A member that no class declares up to an `any` superclass is checked against
// `any`, which accepts anything: a member that can only be read or only be
// written passes like one that can be both.

import {TsOverAny} from './lib';

declare var AnyBase: any;
class FlowOverAny extends AnyBase {}

class FlowChild extends FlowOverAny {
  get r(): number { return 1; } // OK
  set w(v: number) {} // OK
  rw: number = 1; // OK
}

declare class DeclaredChild extends FlowOverAny {
  readonly r: number; // OK
  writeonly w: number; // OK
}

class TsChild extends TsOverAny {
  get r(): number { return 1; } // OK
  set w(v: number) {} // OK
  rw: number = 1; // OK
}
