declare function nullthrows<T>(x: ?T): T;
declare function identity<T>(x: T): T;
declare function get<P extends {...}>(): ?{p: P};

function fromMaybe<P extends {...}>(): {p: P} {
  return nullthrows(get()); // OK
}

function throughIdentity<P extends {...}>(): ?{p: P} {
  return identity(get()); // OK
}

declare function getMaybeUnion<P extends {...}>(): {p: P} | null | undefined;

// A union with `null` and `undefined` members whose other members are placeholders drops out whole,
// as a maybe type does.
function throughIdentityOfUnion<P extends {...}>(): {p: P} | null | undefined {
  return identity(getMaybeUnion()); // OK
}
