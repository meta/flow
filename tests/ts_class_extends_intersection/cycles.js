import {Cycle, First, Twice, WithMixin} from './cycles';

declare const cycle: Cycle;
cycle.c as number; // ok
new Twice();
declare const withMixin: WithMixin;
withMixin.mi;
declare const first: First;
first.mi;

// The same shapes for classes checked in this file.
declare class Other {
  o: number;
}
interface MI {
  mi: boolean;
}
interface MC {
  new (...args: Array<any>): MI;
}
declare const ROther: Class<LocalCycle> & Class<Other>;
class LocalCycle extends ROther {
  c: number;
}
new LocalCycle().c as number; // ok
declare const RMixin: Class<LocalMixin> & MC;
class LocalMixin extends RMixin {}
new LocalMixin().mi;
declare const RIndirect: Class<LocalSecond> & MC;
class LocalFirst extends RIndirect {}
class LocalSecond extends LocalFirst {}
new LocalFirst().mi;
