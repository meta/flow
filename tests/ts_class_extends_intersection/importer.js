import {A, FromAssigned, FromGeneric, FromMixin, FromObjectMixin, FromOverloads} from './lib';

declare const assigned: FromAssigned;
assigned.a as number; // ok
assigned.a as empty; // error: number ~> empty
assigned as A; // ok
FromAssigned.extra as number; // ok: statics are the whole intersection
FromAssigned.s as string; // ok
FromAssigned.nope; // error

const mixed = new FromMixin();
mixed.m as boolean; // ok
mixed.a as number; // ok
mixed.a as empty; // error: number ~> empty
FromMixin.s2 as string; // ok
FromMixin.s as string; // ok
new FromMixin(1); // error: `A`'s constructor takes no argument

new FromOverloads().x as number; // ok
new FromOverloads('s'); // error: only the first member's signatures construct (tsc: ok)
new FromOverloads(1); // error

declare const generic: FromGeneric;
generic.g as empty; // error: string ~> empty
FromGeneric.extra as number; // ok

declare const objectMixed: FromObjectMixin;
objectMixed.o as string; // ok
objectMixed.a as empty; // error: number ~> empty
FromObjectMixin.s as string; // ok
class OverridesObjectMixin extends FromObjectMixin {
  o: number; // error: number ~> string
}
