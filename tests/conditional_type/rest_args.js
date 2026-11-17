declare function choose<R extends ReadonlyArray<number | string>>(...args: R):
  R extends ReadonlyArray<number> ? number : string;

choose(1, 2) as number;
choose(1, 'a') as string;
choose(1, 2) as string; // ERROR
choose(1, 'a') as number; // ERROR
choose() as number;

declare const numbers: Array<number>;
declare const strings: Array<string>;
declare const pair: [number, string];
choose(...numbers) as number;
choose(...strings) as string;
choose(...pair) as string;
choose(1, ...numbers) as number;
choose(...numbers, 'a') as string;
choose(...numbers) as string; // ERROR
choose(...strings) as number; // ERROR

declare const iterableNumbers: Iterable<number>;
choose(...iterableNumbers) as number;
choose(...iterableNumbers, 'a') as string;
choose(...iterableNumbers) as string; // ERROR

declare function collect<T>(...args: Array<T>): Array<T>;
declare function apply<T>(callback: (value: T) => Array<T>, value: T): Array<T>;
apply(value => collect(value), 1) as Array<number>;
apply(value => collect(value), 'a') as Array<string>;
apply(value => collect(value), 1) as Array<string>; // ERROR

declare function applySpread<T>(
  callback: (values: ReadonlyArray<T>) => Array<T>,
  value: T,
): Array<T>;
applySpread(values => collect(...values), 1) as Array<number>;
applySpread(values => collect(...values), 'a') as Array<string>;
applySpread(values => collect(...values), 1) as Array<string>; // ERROR

declare function takesNumbers(label: string, ...values: Array<number>): void;
declare function describe<R extends ReadonlyArray<unknown>>(callback: (...args: R) => void):
  R extends ReadonlyArray<number> ? number : string;
const partial = takesNumbers.bind(null, 'label');
partial();
partial(1, 2);
partial('a'); // ERROR
describe(partial) as number;
describe(partial) as string; // ERROR

type TailKind<T extends ReadonlyArray<unknown>> =
  T extends [unknown, ...infer R]
    ? R extends [string, boolean] ? 'tail' : 'bad'
    : 'none';
declare const tailKind: TailKind<[number, string, boolean]>;
tailKind as 'tail'; // ERROR: tuple elements are invariant, so `number` is not `unknown`
tailKind as 'bad'; // ERROR

function bindGeneric<T>(callback: (label: string, ...values: Array<T>) => void):
  (...values: Array<T>) => void {
  return callback.bind(null, 'label');
}

import {Emitter} from './rest_args_emitter';

class Controller extends Emitter<{[key: string]: Array<unknown>}> {
  setup(): void {
    const proxy = (event: string) => this.emit.bind(this, event);
  }
}
