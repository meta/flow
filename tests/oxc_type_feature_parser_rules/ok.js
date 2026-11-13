declare function named(x: string, ...rest: Array<number>): void;
type Named = (x: number) => void;
type InUnion = string | (() => void);
type Indexer = {[key: string]: number};
type Mapped<O extends {...}> = {[K in keyof O]: O[K]};
type Members = {
  a: string
  b: number
};
declare class C {
  a: string
  b: number;
}
const f = (cb: (x: string) => void): void => {};

libUnnamed('');
