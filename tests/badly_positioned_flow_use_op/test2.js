type T = <A>(f: (x: A) => unknown) => A & A;
type fn = (arg: string) => number;
type arg = fn extends (x: infer A) => unknown ? A & A : empty;
const t: arg = 5;
