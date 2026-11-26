// Classes whose base reaches the class itself: tsc reports TS2502/TS2506 on
// each. They type as a class extending itself through `Class<C>` does.
declare class Other {
  o: number;
}
declare const ROther: typeof Cycle & typeof Other;
export declare class Cycle extends ROther {
  c: number;
}

declare const RSelf: typeof Twice & typeof Twice;
export declare class Twice extends RSelf {}

interface MI {
  mi: boolean;
}
interface MC {
  new (...args: any[]): MI;
}
declare const RMixin: typeof WithMixin & typeof Other & MC;
export declare class WithMixin extends RMixin {}

declare const RIndirect: typeof Second & MC;
export declare class First extends RIndirect {}
export declare class Second extends First {}

// A mixin whose construct signature returns the class that extends it (tsc:
// TS2310).
interface ReturnsMixesSelf {
  new (...args: any[]): MixesSelf;
}
declare const RMixesSelf: ReturnsMixesSelf & typeof Other;
export declare class MixesSelf extends RMixesSelf {}

// A generic class whose construct signature returns the class itself (tsc:
// TS2310). Its base is a re-pointed copy of it, which carries the constructor.
declare const RWraps: new <U>(x: number) => U;
export declare class WrapsSelf<T> extends RWraps<WrapsSelf<T>> {
  w: T;
}
