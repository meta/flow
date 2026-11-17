import * as React from 'react';

const Enum = Object.freeze({A:'a', B: 'b'});

type A = { __type__: 'a', arr: Array<string> } | { __type__: 'b', arr: Array<number> };
type F = { __type__: 'a', f: (v: string) => string } | { __type__: 'b', f: (v: number) => number };
declare function CompAProp(x: A): React.MixedElement;
declare function CompFProp(x: F): React.MixedElement;

// OK

const a1: A = { __type__: 'a', arr: [] }; // ok
const a2: A = { arr: [], __type__: 'a' }; // ok
const a3: A = { __type__: 'b', arr: [] }; // ok
const a4: A = { arr: [], __type__: 'b' }; // ok
const a5: A = { __type__: Enum.A, arr: [] }; // ok
const a6: A = { arr: [], __type__: Enum.A }; // ok
const a7: A = { __type__: Enum.B, arr: [] }; // ok
const a8: A = { arr: [], __type__: Enum.B }; // ok

const f1: F = { __type__: 'a', f: (v) => v as string }; // ok
const f2: F = { f: (v) => v as string, __type__: 'a' }; // ok
const f3: F = { __type__: 'b', f: (v) => v as number }; // ok
const f4: F = { f: (v) => v as number, __type__: 'b' }; // ok
const f5: F = { __type__: Enum.A, f: (v) => v as string }; // ok
const f6: F = { f: (v) => v as string, __type__: Enum.A }; // ok
const f7: F = { __type__: Enum.B, f: (v) => v as number }; // ok
const f8: F = { f: (v) => v as number, __type__: Enum.B }; // ok

<CompAProp __type__="a" arr={[]} />; // ok
<CompAProp arr={[]} __type__="a" />; // ok
<CompAProp __type__="b" arr={[]} />; // ok
<CompAProp arr={[]} __type__="b" />; // ok
<CompAProp __type__={Enum.A} arr={[]} />; // ok
<CompAProp arr={[]} __type__={Enum.A} />; // ok
<CompAProp __type__={Enum.B} arr={[]} />; // ok
<CompAProp arr={[]} __type__={Enum.B} />; // ok
<CompFProp __type__="a" f={(v) => v as string} />; // ok
<CompFProp f={(v) => v as string} __type__="a" />; // ok
<CompFProp __type__="b" f={(v) => v as number} />; // ok
<CompFProp f={(v) => v as number} __type__="b" />; // ok
<CompFProp __type__={Enum.A} f={(v) => v as string} />; // ok
<CompFProp f={(v) => v as string} __type__={Enum.A} />; // ok
<CompFProp __type__={Enum.B} f={(v) => v as number} />; // ok
<CompFProp f={(v) => v as number} __type__={Enum.B} />; // ok

declare function a(): 'a';
const b1: A = { __type__: a(), arr: [] };
<CompAProp __type__={a()} arr={[]} />;

// Errors
const e1: F = { __type__: a(), f: (v) => v as string }; // error
<CompFProp __type__={a()} f={(v) => v as string} />; // error

function numeric_sentinel() {
  type T = { n: 1 | 2, fn: (x: number) => void };
  const t: T = { n: 1, fn: (x) => {} }; // ok, sentinel `1` should match part of above union
}

function function_statics() {
  const f: {
    (x: unknown): void,
    type?: void,
    ...
  } = x => {}; // okay `x` inferred as `mixed`
}

// Numeric literal key names
type B = {0: 'a', f: (x: string) => string} | {0: 'b', f: (x: number) => number};
{
  const b: B = {f: (v) => (v as string), 0: 'a'}; // OK
}
