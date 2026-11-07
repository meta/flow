// Structural interfaces support explicit, inherited, and structural implementations.
interface NamedThing {
  name: string;
}
class ActualThing implements NamedThing {
  name: string;
  detail: number;
}
class OtherThing implements NamedThing {
  name: string;
}
class InheritedThing extends ActualThing {}
class StructuralThing {
  name: string;
}

declare const maybeNamedThing: ?NamedThing;
declare const namedThing: NamedThing;
declare const other: boolean;

// A class pattern can match an interface, but does not consume the interface.
match (maybeNamedThing) {
  ActualThing {const name, const detail, ...} => { // OK
    name as string; // OK
    detail as number; // OK
    detail as string; // ERROR: class-specific binding is not empty
  },
  {const name, ...} => { // OK
    name as string; // OK
  },
  _ => {}, // OK
}

// Explicit and structural implementations remain reachable after sibling class arms.
match (namedThing) {
  ActualThing {...} => {}, // OK
  OtherThing {...} => {}, // OK
  StructuralThing {const name, ...} => { // OK
    name as string; // OK
  },
  {...} => {}, // OK
}

// Explicit implementation is not required for a structural interface.
match (namedThing) {
  InheritedThing {const detail, ...} => { // OK
    detail as number; // OK
  },
  {...} => {}, // OK
}

// Known implementations and nullish values do not exhaust an interface input.
match (maybeNamedThing) { // ERROR: the interface is still uncovered
  ActualThing {...} => {}, // OK
  OtherThing {...} => {}, // OK
  null | undefined => {}, // OK
}

// A total structural arm still makes later class arms unused.
match (namedThing) {
  {...} => {}, // OK
  ActualThing {...} => {}, // ERROR: unused
}

// Total class arms cover duplicate or narrower class patterns, not siblings.
match (namedThing) {
  ActualThing {...} => {}, // OK
  ActualThing {...} => {}, // ERROR: unused
  InheritedThing {...} => {}, // ERROR: unused
  OtherThing {...} => {}, // OK
  {...} => {}, // OK
}

// A subclass arm leaves a broader class arm reachable.
match (namedThing) {
  InheritedThing {...} => {}, // OK
  ActualThing {...} => {}, // OK: the broader class is still reachable
  {...} => {}, // OK
}

// Guards and partial property patterns do not cover a later total class arm.
match (namedThing) {
  ActualThing {...} if (other) => {}, // OK
  ActualThing {detail: 0, ...} => {}, // OK
  ActualThing {...} => {}, // OK
  {...} => {}, // OK
}

// Class-specific property validation and inexactness are preserved.
match (namedThing) {
  ActualThing {const detail} => {}, // ERROR: requires rest
  {...} => {}, // OK
}

// An impossible property pattern leaves a later total class arm reachable.
match (namedThing) {
  ActualThing {detail: null, ...} => {}, // ERROR: impossible property pattern
  ActualThing {...} => {}, // OK
  {...} => {}, // OK
}

// Class bindings preserve properties introduced by an extended interface.
interface ExtendedNamedThing extends NamedThing {
  tag: boolean;
}
class ExtendedThing implements ExtendedNamedThing {
  name: string;
  tag: boolean;
}
declare const extended: ExtendedNamedThing;
match (extended) {
  ExtendedThing {const tag, ...} => { // OK
    tag as boolean; // OK
  },
  {...} => {}, // OK
}

// Bindings retain types substituted through generic interface inheritance.
interface HasValue<T> {
  value: T;
}
interface NamedValue extends NamedThing, HasValue<number> {}
class NumberThing implements NamedValue {
  name: string;
  value: number;
}
declare const generic: HasValue<number>;
match (generic) {
  NumberThing {const value, ...} => { // OK
    value as number; // OK
  },
  {...} => {}, // OK
}

// Unrelated classes remain nominally disjoint despite their structural shape.
declare const otherThing: OtherThing;
match (otherThing) {
  StructuralThing {...} => {}, // ERROR: unused
  OtherThing {...} => {}, // OK
}

// Interface/class overlap remains conservative, without implying exhaustiveness.
class NumberNamedThing {
  name: number;
}
match (namedThing) { // ERROR: the interface is still uncovered
  NumberNamedThing {...} => {}, // OK
}

// Nullable class inputs keep their existing nominal exhaustiveness.
declare const maybeActualThing: ?ActualThing;
match (maybeActualThing) {
  ActualThing {...} => {}, // OK
  {...} => {}, // ERROR: unused
  _ => {}, // OK
}
