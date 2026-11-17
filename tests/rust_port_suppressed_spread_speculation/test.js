// Regression test for speculative object-spread checking interacting badly
// with suppressed return-type errors.
//
// The parent Rust CLI panics with:
//   Non speculating: SpeculativeError(...)
// when checking the spread branch below. The fixed commit accepts the file
// with no errors, matching the intended suppression behavior.

type FunctionReturning<out T> = (...args: ReadonlyArray<unknown>) => T;

const emptyFunction: {
  readonly thatReturnsNull: FunctionReturning<null>,
} = {
  thatReturnsNull() {
    return null;
  },
};

type LocalDate = number;
type LocalDateInterval = {
  readonly start: number,
  readonly end: number,
};

type Option = {
  readonly value: string,
  readonly calculateInterval: (date: LocalDate) => LocalDateInterval,
};

const CUSTOM = {
  value: 'custom',
};

declare const selectedOption: ?string;
declare const options: ReadonlyArray<Option>;

const option =
  selectedOption !== CUSTOM.value
    ? // $FlowFixMe[incompatible-type]
      options.find(opt => opt.value === selectedOption)
    : /* $FlowFixMe[cannot-spread-inexact] */
      {
        calculateInterval(_today: LocalDate): LocalDateInterval {
          // $FlowFixMe[incompatible-type]
          return emptyFunction.thatReturnsNull;
        },
        ...CUSTOM,
      };

const interval: ?LocalDateInterval = option ? option.calculateInterval(0) : null;
