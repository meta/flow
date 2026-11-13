type Pick2<T, K extends keyof T> = {[P in K]: T[P]};
type Exclude2<T, U> = T extends U ? empty : T;
type Omit2<T, K> = Pick2<T, Exclude2<keyof T, K>>;

// Each member of a union is resolved where the union started, not inside the
// expansions of the members before it.
type Id<X> = X;
type B<T> = T extends unknown ? Id<Exclude2<'a' | 'b', 'a'>> : empty;
'b' as B<1 | 2 | 3 | 4 | 5>; // ok
'a' as B<1 | 2 | 3 | 4 | 5>; // error

type DistributiveOmit<T, K> = T extends unknown ? Omit2<T, K> : empty;
type Omitted = DistributiveOmit<
  {k: 1, a: 1} | {k: 2, b: 1} | {k: 3, c: 1} | {k: 4, d: 1} | {k: 5, e: 1},
  'k',
>;
({e: 1}) as Omitted; // ok
({k: 5}) as Omitted; // error

// So is each operand of a spread.
type O = {a: number, b: string};
type Spread5 = {
  ...Omit2<O, 'a'>,
  ...Omit2<O, 'a'>,
  ...Omit2<O, 'a'>,
  ...Omit2<O, 'a'>,
  ...Omit2<O, 'a'>,
};
declare const s: Spread5;
s.b as string; // ok
s.a; // error

declare const o: Omit2<O, 'a'>;
const v = {...o, ...o, ...o, ...o, ...o};
v.b as string; // ok
v.a; // error
