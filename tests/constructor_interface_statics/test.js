// An interface with a construct signature types a constructor, and its methods
// are statics: like the statics of a class, they may be read off it unbound.

import * as Exports from './exports';

[1, 2].map(Number.isFinite); // OK
const parse: (s: string) => number = Date.parse; // OK
const {fromCharCode} = String; // OK
fromCharCode(65); // OK
Exports.parse as (s: string) => number; // OK
Exports.fromCharCode(65); // OK

interface Point {
  x: number;
}
interface PointConstructor {
  new (x: number): Point;
  origin(): Point;
}
declare const Point: PointConstructor;
const origin: () => Point = Point.origin; // OK
origin(); // OK

// Without a construct signature, the methods still require their receiver.
interface Namespace {
  f(): void;
}
declare const ns: Namespace;
const f: () => void = ns.f; // ERROR

// The instance a class extends is not a constructor, even when its base is one.
class D extends Date {
  m(): void {
    const getTime: () => number = super.getTime; // ERROR
  }
}
