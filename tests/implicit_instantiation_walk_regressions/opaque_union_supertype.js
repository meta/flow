// An opaque type whose supertype is a union without null or void flows to a maybe type through
// its inner type, as subtyping does: `T` is the opaque type, not its supertype's members.
import type {Epoch} from './opaque_union_supertype_def';
declare function nullthrows<T>(x: ?T): T;
declare const maybeEpoch: null | Epoch;
const epoch: Epoch = nullthrows(maybeEpoch); // ok
