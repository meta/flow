//@flow

function spread<T>(x: T): {...T, ...{}, ...} {
  return null as any;
}
declare function spreadExact<T>(x: T): {...T, ...{}};
let x;

if (true) {
  x = {foo: 3};
} else if (true) {
  x = {qux: 3};
} else if (true) {
  x = {foo: 3, baz: 3};
} else {
  x = {foo: 3, bar: 3};
}

type AllOpt = {readonly foo?: number, readonly bar?: number, readonly baz?: number, readonly qux?: number, ...};

const a = spreadExact(x);

if (a.bar != null) {
} // bar appears in one of the branches
if (a.baz != null) {
} // baz appears in one of the branches
if (a.foo != null) {
} // foo appears in one of the branches
if (a.qux != null) {
} // qux appears in one of the branches

a.foo as number; // Error, foo does not appear in all branches
a.bar as number; // Error, bar does not appear in all branches
a.baz as number; // Error, baz does not appear in all branches
a.qux as number; // Error, qux does not appear in all branches

a as AllOpt;

let y;
if (true) {
  y = {foo: 3, bar: 3};
} else if (true) {
  y = {foo: 3, baz: 3};
} else if (true) {
  y = {qux: 3};
} else {
  y = {foo: 3};
}

y as AllOpt;

const b = spreadExact(y);

if (b.bar != null) {
} // bar appears in one of the branches
if (b.baz != null) {
} // baz appears in one of the branches
if (b.foo != null) {
} // foo appears in one of the branches
if (b.qux != null) {
} // qux appears in one of the branches
b.foo as number; // Error, foo does not appear in all branches
b.bar as number; // Error, bar does not appear in all branches
b.baz as number; // Error, baz does not appear in all branches
b.qux as number; // Error, qux does not appear in all branches

b as AllOpt;

let z;

if (true) {
  z = {foo: 3};
} else if (true) {
  z = {foo: 3};
} else if (true) {
  z = {bar: 3};
} else if (true) {
  z = {baz: 3};
} else {
  z = {qux: 3};
}

const c = spread(z);

if (c.bar != null) {
} // bar appears in one of the branches
if (c.baz != null) {
} // baz appears in one of the branches
if (c.foo != null) {
} // foo appears in one of the branches
if (c.qux != null) {
} // qux appears in one of the branches
c.foo as number; // Error, foo does not appear in all branches
c.bar as number; // Error, bar does not appear in all branches
c.baz as number; // Error, baz does not appear in all branches
c.qux as number; // Error, qux does not appear in all branches

// Equal keys create tvars with lower bounds
let union;

if (true) {
  union = {foo: 3};
} else {
  union = {foo: 'string'};
}

// We use read only fields to avoid unification
const union_spread: {readonly foo: number | string, ...} = spread(union);

let optional;

if (true) {
  optional = {foo: 3} as {foo?: number, ...};
} else if (true) {
  optional = {bar: 3} as {bar?: number, ...};
} else if (true) {
  optional = {baz: 3} as {baz?: number, ...};
} else {
  optional = {qux: 3} as {qux?: number, ...};
}

const optional_spread = spread(optional);
if (optional_spread.bar != null) {
} // ok
if (optional_spread.baz != null) {
} // ok
if (optional_spread.foo != null) {
} // ok
if (optional_spread.qux != null) {
} // ok
optional_spread.foo as number; // Error, foo does not appear in all branches, might be undefined
optional_spread.bar as number; // Error, bar does not appear in all branches, might be undefined
optional_spread.baz as number; // Error, baz does not appear in all branches, might be undefined
optional_spread.qux as number; // Error, qux does not appear in all branches, might be undefined

let optional2;

if (true) {
  optional2 = {foo: 3};
} else {
  optional2 = {foo: 3} as {foo?: number, ...};
}

const optional2_spread = spread(optional2); // Ok {foo?: number}

let indexer;
if (true) {
  indexer = {foo: 3} as {[key: string]: number};
} else if (true) {
  indexer = {bar: 3} as {[key: string]: number, bar: number};
} else if (true) {
  indexer = {baz: 3} as {[key: string]: number, baz: number};
} else {
  indexer = {qux: 3} as {[key: string]: number, qux: number};
}

const indexer_spread = spread(indexer);
// All ok because of the indexer. Adding bar, baz,and qux to {[string]: number} doesn't even
// change the type, since bar, baz, and qux all <: number!
if (indexer_spread.bar != null) {
}
if (indexer_spread.baz != null) {
}
if (indexer_spread.foo != null) {
}
if (indexer_spread.qux != null) {
}
indexer_spread.foo as number;
indexer_spread.bar as number;
indexer_spread.baz as number;
indexer_spread.qux as number;

let indexer2;
if (true) {
  indexer2 = {foo: 3} as {[key: string]: number};
} else if (true) {
  indexer2 = {bar: ''} as {[key: string]: number, bar: string};
} else if (true) {
  indexer2 = {baz: ''} as {[key: string]: number, baz: string};
} else {
  indexer2 = {qux: ''} as {[key: string]: number, qux: string};
}
const indexer_spread2 = spread(indexer2);
// All ok because of the indexer
if (indexer_spread2.bar != null) {
}
if (indexer_spread2.baz != null) {
}
if (indexer_spread2.foo != null) {
}
if (indexer_spread2.qux != null) {
}
indexer_spread2.foo as string; // Error, number ~> string
indexer_spread2.bar as string; // Error, possibly number or undefined
indexer_spread2.baz as string; // Error, possibly number or undefined
indexer_spread2.qux as string; // Error, Possibly number or undefined

let indexer_err;

if (true) {
  indexer_err = {foo: 'string'} as {[key: string]: string};
} else {
  indexer_err = {foo: 3} as {[key: string]: number};
}

const indexer_err_spread = spread(indexer_err); // Error, string and number don't unify

let indexer_on_second;
if (true) {
  indexer_on_second = {foo: 'string'};
} else {
  indexer_on_second = {bar: 3} as {[key: string]: number};
}

const indexer_on_second_spread = spread(indexer_on_second);
indexer_on_second_spread.foo as number; // Error, may be void or string
indexer_on_second_spread.bar as number; // Error, no indexer if it's only in one branch
indexer_on_second_spread.baz as number; // Error, no indexer if it's only in one branch
indexer_on_second_spread.qux as number; // Error, no indexer if it's only in one branch

let inexact;
if (true) {
  inexact = {foo: 3};
} else {
  inexact = {foo: 3} as {foo: number, ...};
}

declare function inexactSpread<T>(x: T): {bar: 3, ...T, ...{}, ...};
const inexact_spread = inexactSpread(inexact); // OK
