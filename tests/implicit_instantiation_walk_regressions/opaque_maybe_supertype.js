// An opaque type flows to a maybe type with null and undefined dropped from its supertype, as
// subtyping narrows it: `T` is the opaque type with null and undefined dropped from its supertype, which
// what flows in through its lower bound must fit, including where an alias hides them.
import type {MaybeObj, MaybeStr, MaybeAlias} from './opaque_maybe_supertype_def';
declare function nonNull<T>(x: ?T): T;
declare const m: MaybeObj;
nonNull(m) as MaybeObj; // ok
nonNull(m).foo as string; // ok
declare const s: MaybeStr;
let r = nonNull(s);
r = 'a'; // ok
r = null; // error
declare const a: MaybeAlias;
let ra = nonNull(a);
ra = 'a'; // ok
ra = null; // error
// So does it to `T | null | undefined`.
declare function nonNullOfUnion<T>(x: T | null | undefined): T;
let ru = nonNullOfUnion(s);
ru = 'a'; // ok
ru = null; // error
