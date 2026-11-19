// A union meets each member of an intersection whole, as in subtyping: the spread of `T` gets the
// union, and `T` is the union of both objects, not only the first.
type LoadState = 'LOADED' | 'ERROR';
declare function toLoadObject<T>(
  raw: Readonly<{...T, ...}> & interface {readonly loadState: LoadState},
): T;
declare const raw:
  | {data: Array<empty>, loadState: 'LOADED'}
  | {data: ReadonlyArray<unknown>, loadState: LoadState};

const loaded = toLoadObject(raw); // ok
loaded.data as ReadonlyArray<unknown>; // ok
loaded.data as Array<empty>; // error: the second object's `data` is read-only
