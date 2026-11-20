// Optional member access

// object
function f(x: {a: number}) {
  x?.a; /* unnecessary */
}

// union
function h(x: {a: number}|{a: string}) {
  x?.a; /* unnecessary */
}

// maybe
function g(x: ?{a: number}) {
  x?.a; /* NOT unnecessary */
}

// union with null
function k(x: null | {a: number}) {
  x?.a; /* NOT unnecessary */
}

// Optional chain on a property

// required prop
function l(o: { p: {a: number}, ... }) {
  o.p?.a; /* unnecessary */
}

// required method
function m(o: { p: () => void, ... }) {
  o.p?.(); /* unnecessary */
}

// union
function n(o: { p: {a: number}|{a: string}, ... }) {
  o.p?.a; /* unnecessary */
}

function q(o: { p?: {a: number}, ... }) {
  o.p?.a; /* NOT unnecessary */
}

// Assignment

function z(x: {a: string}) {
  var assignee;
  (assignee = x)?.a; /* unnecessary */
}

var value: {a: number} = {a: 0};
var maybeValue: ?{a: number} = null;
var valToUse = value?.a; /* unnecessary */

var maybeA = maybeValue?.a; /* NOT unnecessary */
var maybeComputedA = maybeValue?.['a']; /* NOT unnecessary */

var computedA = value?.['a']; /* unnecessary */
var calledA = (() => value)?.(); /* unnecessary */

// Enum switch with only a default case

enum E {A, B}
declare const e: E;
switch (e) { default: } /* implicit cases */
