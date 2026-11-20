// unnecessary-optional-chain

// non-nullish object
function f(x: { p: number, ... }) {
  x?.p; /* unnecessary */
}

// non-nullish prop
function g(o: { p: { q: number, ... }, ... }) {
  o.p?.q; /* unnecessary */
}

// maybe
function h(x: ?{ p: number, ... }) {
  x?.p; /* NOT unnecessary */
}

// unused-promise

declare function promise(): Promise<void>;

function k() {
  promise(); /* unused */
}

async function l() {
  promise(); /* unused */
}

function m() {
  const p = promise(); /* NOT unused */
}

async function n() {
  await promise(); /* NOT unused */
}
