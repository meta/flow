// A rest parameter receives an array, with a tuple view when the number of arguments is known, as
// the call builds it: `TArgs` from `...rest: []` fits `Array<any>`.
declare function withLogging<E, TArgs extends Array<any>>(
  callback: ?(v: E, ...rest: TArgs) => unknown,
  children: (v: void | ((v: E, ...rest: TArgs) => void)) => unknown,
): TArgs;
declare var onClick: ?(v: number) => void;
withLogging(onClick, (cb: void | ((v: number, ...rest: []) => void)) => 1); // ok

// A rest parameter of unknown length leaves the elements after the known ones to the reversal.
type Store = {s: number};
declare function update(isFavorite: boolean, id: string, store: Store): void;
declare var id: string;
const updater: (store: Store, ...ignored: Array<unknown>) => void = update.bind(null, true, id); // ok
