declare function identity<T>(x: T): T;
declare function getMaybeUnion<P extends {...}>(): {p: P} | null | void;

// A union with `null` and `void` members whose other members are placeholders drops out whole,
// as a maybe type does.
function throughIdentityOfUnion<P extends {...}>(): {p: P} | null | void {
  return identity(getMaybeUnion()); // OK
}
