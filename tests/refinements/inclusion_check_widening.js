// Flow checks that, in conditional expressions, the refining (sentinel) value is
// a subtype of the type of the value that is being refined. This check is not
// necessary for soundness, but it helps prevent some common bugs. These checks
// are also not necessary for propagating true value flow.
//
// A potential pitfall when checking for these violations, is to have these checks
// affect the result of inference, by flowing the "sentinel" type to the refined
// type. This may cause these types to flow downstream, far from the conditional
// cite, causing spurious errors.
//
// The tests in the file exercise scenarios, where having these checks as part of
// inference produces surprising results.


// Example 1

declare function useState<S>(state: (() => S) | S): [S, (action: ((x: S) => S) | S) => void];
const [value, setter] = useState<string | null>(null);
declare const Enum: {A: 'A', B: 'B', ...};

// The conditional below may be seen as a refinement on `Enum`. In that case, we
// make sure that `value` is a valid type for `Enum.A` (see matching prop).
// This check should not affect the inferred type of `value` or `setter`, and the
// subsequent call.
if (value === Enum.A) {}
setter('B');  // okay


// Example 2

const x = null;
const y = x && x.f.g; // error: x is always null
// `null` may reach `y` here, and so this should be reported as an error ('val'
// cannot refine `null`).
if (y === 'val') {} // error, invalid-compare. LHS is null but RHS does not include null


// Example 3

type V<T extends {...}> = { key2: keyof T, ... };
declare const column: V<{...}>;
const {key2} = column;
if (key2 === 'a') {} // error (key2 does not include 'a')


// Example 4

class C {}
type State = { prop: ?C, ... };

class D {
  state: State = { prop: null };
  m(): void {
    const {prop} = this.state;
    if (
        prop !== undefined &&
        prop !== null &&
        prop !== 0 // error (0 incompatible with C)
       ) {}
  }
}


// Example 5

const {key5} = {key5: 'a'};
const _ = key5 === 'b' ? null : null;
key5 as 'a'; // error string ~> 'a'
