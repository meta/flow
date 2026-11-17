type M = {
  a: number,
  b: string,
}

declare let v: M['a' | 'b'];
v as string; // error
v as number; // error
v as number | string; // ok

v = 3;
v = "";
v = true; // error

declare const union: number | string;
union as M['a' | 'b'];

type W = M['a' | 'b'];
declare const w: W;
w as W;
w as M['a']; // error


/* Termination - the following code produces a recursive union type of the form
 *
 * type r = r | Array<t>
 *
 * To avoid termination issues here, we use ConstFoldExpansion.guard. */

declare export function useState<S>(
  initialState: (() => S) | S,
): [S, (action: ((state: S) => S) | S) => void];
declare export function useRef<T>(initialValue: T): {current: T};
declare const NUM: number;
const [state, setState] = useState(1);
useRef([] as Array<number>).current[state];
setState(st => st === NUM ? st : st);


/* Regressions due to interaction with unions */

type Key = 'a' | 'b';
type T = {
  a: Array<Key>,
  b: Array<Key>
};
['a'] as T[Key]; // Expected [speculation-ambiguous] error
