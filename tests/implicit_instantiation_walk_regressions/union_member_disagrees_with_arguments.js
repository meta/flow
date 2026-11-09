// A union that the operation's result meets, of which only one member can match, gives no evidence
// when what that member gives disagrees with the arguments, as the flow-based solve's speculative
// match of the member fails. The hint of the inner call below is `number | Thenable<number>`, from
// the default `R1 = T` of the outer call before its callback is checked: it does not make `R2`
// of the inner call `number`.
declare interface Thenable<T> {
  then<R1 = T, R2 = empty>(
    onFulfilled?: ?(value: T) => R1 | Thenable<R1>,
    onRejected?: ?(reason: unknown) => R2 | Thenable<R2>,
  ): Thenable<R1 | R2>;
}

declare interface Task<T> {
  then<R1 = T, R2 = empty>(
    onFulfilled?: ?(value: T) => R1 | Thenable<R1>,
    onRejected?: ?(reason: unknown) => R2 | Thenable<R2>,
  ): Task<R1 | R2>;
}

declare const numbers: Task<number>;
declare const booleans: Task<boolean>;

const task = numbers.then(() => booleans.then(() => booleans));
task as Task<boolean>; // ok
task as Task<number>; // error

// Nor when the arguments do not fit that member even with the markers open: the hint of the call
// to `recover` is `void | Thenable<void>`, which `Task<number | R>` does not fit, so it does not
// make `R` `void`.
declare interface Recoverable<T> {
  recover<R = empty>(onRejected: (reason: unknown) => R | Thenable<R>): Task<T | R>;
}
declare const voids: Task<void>;
declare const recoverable: Recoverable<number>;
declare function rejected<T = empty>(): Task<T>;

const recovered = voids.then(() => recoverable.recover(() => rejected()));
recovered as Task<number>; // ok

// A member that fits with the evidence it gives keeps it: through the hint `?Mock<[string], void>`,
// `TArguments` is `[string]`, not its default.
type Mock<TArguments extends ReadonlyArray<unknown>, TReturn> = {
  (...args: TArguments): TReturn,
  mock: {calls: Array<TArguments>, ...},
  mockImplementation(f: (...args: TArguments) => TReturn): Mock<TArguments, TReturn>,
  ...
};
declare function mock<
  TArguments extends ReadonlyArray<unknown> = ReadonlyArray<any>,
  TReturn = any,
>(
  implementation?: (...args: TArguments) => TReturn,
): Mock<TArguments, TReturn>;
declare const overrides: ?{onChange?: Mock<[string], void>};
const onChange = overrides?.onChange ?? mock();
onChange as Mock<[string], void>; // ok

// A function type against a hint of a callable interface or `null`: the parameters of the member
// give `TArgs` its elements, which a callback without parameters accepts.
declare interface Handler {
  (event: string, source?: string): unknown;
}
declare function guard<TArgs extends ReadonlyArray<unknown>>(
  f: (...args: TArgs) => void,
): (...args: TArgs) => void;
declare const handler: {onerror: Handler | null};
handler.onerror = guard(() => {}); // ok
