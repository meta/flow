// This test ensures that unification does not alter the type of fully resolved tvars
type Foo = {readonly foo?: ReadonlyArray<{
  readonly bar: string,
}>};

declare function useState<S>(
  initialState: (() => S) | S,
): (s: S) => void;

declare const v: Foo;

const setState = useState(
  v?.foo ?? []
);

let x = []; // ERROR
setState(x);

// The unification here should not "leak" to other uses of x
x as Array<{bar: string}>; // ERROR
