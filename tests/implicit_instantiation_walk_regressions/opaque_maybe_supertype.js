// An opaque type flows to a maybe type with null and void dropped from its supertype, as
// subtyping narrows it: `T` is the opaque type with the narrowed supertype.
import type {MaybeObj} from './opaque_maybe_supertype_def';
declare function nonNull<T>(x: ?T): T;
declare const m: MaybeObj;
nonNull(m) as MaybeObj; // ok
nonNull(m).foo as string; // ok
