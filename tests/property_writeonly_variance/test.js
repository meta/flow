// `writeonly` is a textual alias for `-` in property variance.

// 1. Object types — writeonly equivalent to -
type Wo = { writeonly p: number };
type Mn = { writeonly p: number };
declare const wo: Wo;
declare const mn: Mn;
wo as Mn; // ok
mn as Wo; // ok

// 2. Class properties
class C {
  writeonly prop: number;
  constructor(p: number) {
    this.prop = p;
  }
}

// 3. Interfaces
interface I {
  writeonly prop: string;
}

// 4. Indexer
type Idx = { writeonly [key: string]: number };
type IdxMinus = { writeonly [key: string]: number };
declare const ix: Idx;
ix as IdxMinus; // ok

// 5. Tuples
type Tup = [writeonly a: string];

// 6. `writeonly` still usable as identifier name (regression)
const writeonly = 1;
type R = { writeonly: string };

// 7. writeonly property: reads error, writes succeed
type W = { writeonly p: number };
declare const w: W;
w.p as number; // error: write-only
w.p = 0; // ok

// 8. Mapped types: +writeonly / writeonly add Negative variance
type Src = { foo: number, bar: string };
type AllWriteonly<T extends {...}> = { +writeonly [K in keyof T]: T[K] };
declare const aw: AllWriteonly<Src>;
aw.foo as number; // error: writeonly property is not readable
aw.foo = 3; // ok

// 9. Mapped types: -writeonly removes Negative variance
type WriteOnlyO = { writeonly foo: number, writeonly bar: string };
type RemoveWriteonly<T extends {...}> = { -writeonly [K in keyof T]: T[K] };
declare const rw: RemoveWriteonly<WriteOnlyO>;
rw.foo as number; // ok — writeonly was removed
rw.foo = 3; // ok
