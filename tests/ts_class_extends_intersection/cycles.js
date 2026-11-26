import {Cycle, First, MixesSelf, Twice, WithMixin, WrapsSelf} from './cycles';
import {C, State} from './record';

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

// A walk up a chain that cycles stops where it cycles, and moves on to the
// other members of an intersection it passed, whose chains report a property
// that no class declares (tsc: TS2339).
Cycle.nope; // error
LocalCycle.nope; // error
LibCycle.nope; // error
State.nope; // error
C.nope; // error
// With nothing left to try, it ends without an answer (tsc: TS2339).
new LocalCycle().nope;

// A mixin whose construct signature returns the class extending it (tsc:
// TS2310).
declare const mixesSelf: MixesSelf;
mixesSelf.o as number; // ok
mixesSelf.nope; // error
interface ReturnsLocalMixesSelf {
  new (...args: Array<any>): LocalMixesSelf;
}
declare const RLocalMixesSelf: ReturnsLocalMixesSelf & Class<Other>;
class LocalMixesSelf extends RLocalMixesSelf {}
new LocalMixesSelf().o as number; // ok
new LocalMixesSelf().nope; // error
new LocalMixesSelf().o = 1; // ok
new LocalMixesSelf() as Other; // ok
new LocalMixesSelf() as MI; // error
class ExtendsMixesSelf extends LocalMixesSelf {
  o: string; // error: string ~> number
}

// A class extending a generic class whose base is a copy of the class itself
// still constructs with that copy's constructor (tsc: the construct signature
// of the base value).
class SubWraps extends WrapsSelf<string> {}
new SubWraps(1).w as number; // error: string ~> number
new SubWraps('bad'); // error: string ~> number

// Two classes mixing each other in (tsc: TS2310 on each).
interface ReturnsMixA {
  new (...args: Array<any>): MixA;
}
interface ReturnsMixB {
  new (...args: Array<any>): MixB;
}
declare const RMixA: ReturnsMixB & Class<Other>;
declare const RMixB: ReturnsMixA & Class<Other>;
class MixA extends RMixA {}
class MixB extends RMixB {}
new MixA().o as number; // ok

// A mixin returning an interface that extends the class (tsc: TS2310).
interface ExtendsMixesInterface extends MixesInterface {
  i: number;
}
interface ReturnsInterface {
  new (...args: Array<any>): ExtendsMixesInterface;
}
declare const RInterface: ReturnsInterface & Class<Other>;
class MixesInterface extends RInterface {}
new MixesInterface().o as number; // ok

// Interfaces that extend each other (tsc: TS2310).
interface X {
  x: number;
}
interface Y {
  y: number;
}
interface CycleA extends CycleB, X {}
interface CycleB extends CycleA, Y {}
declare const cycleA: CycleA;
cycleA.x as number; // ok
cycleA.y as number; // ok
cycleA.nope; // error
cycleA as Other; // error

// A chain that does not cycle but reaches a class twice, through two members of
// an intersection, still ends at `Object.prototype`.
interface DiamondA extends X {}
interface DiamondB extends X {}
interface Diamond extends DiamondA, DiamondB {}
declare const diamond: Diamond;
diamond.toString() as string; // ok
diamond as interface {}; // ok

// A chain that passes two instances of one generic class does not cycle.
type Id<T> = T;
interface Wraps<T extends interface {}> extends Id<T> {
  w: number;
}
declare const wrapped: Wraps<Wraps<X>>;
wrapped.x as number; // ok
wrapped.nope; // error
wrapped as Y; // error
