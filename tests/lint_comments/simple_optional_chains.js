//setting up for next test
/*flowlint unnecessary-optional-chain:off*/ //unused suppression

//error: not a real lint or setting, but unnecessary-optional-chain is still enabled
/*flowlint not-a-real-lint : not-a-real-setting   ,
    unnecessary-optional-chain:error */

// Optional member access checks

// object
function f(x: {a: number}) {
  //not suppressed because the suppression comes after it on the line
  x?.a; /* unnecessary */ /* flowlint unnecessary-optional-chain:off */
}

// union
function h(x: {a: number}|{a: string}) {
  x?.a; /* unnecessary; suppressed */
}

// maybe
function g(x: ?{a: number}) {
  x?.a; /* NOT unnecessary */
}

// union with null
function k(x: null | {a: number}) {
  x?.a; /* NOT unnecessary */
}

//multi-line test

/*

	 flowlint

  	 unnecessary-optional-chain:error,

  	 uninitialized-instance-property:error

 	 */

var x1: {a: boolean} = {a: false};
x1?.a; /* unnecessary */

//docblock-style tests

/*
 * flowlint
 * unnecessary-optional-chain:off,
 * uninitialized-instance-property:off
 *
 */

var x2: {a: boolean} = {a: false};
x2?.a; /* unnecessary; suppressed */

/*
 * flowlint
 * unnecessary-optional-chain:
 *    error,
 * unused-promise:
 *    off
 */ //The unused-promise suppression is unused

var x3: {a: string} = {a: ""};
x3?.a; /* unnecessary */

// Optional chain on a property checks

// required prop
function l(o: { p: {a: number}, ... }) {
  o.p?.a; /* unnecessary; suppressed */ //flowlint-line unnecessary-optional-chain:off
}

// required method
function m(o: { p: () => void, ... }) {
  /* flowlint-next-line unnecessary-optional-chain:off */
  o.p?.(); /* unnecessary; suppressed */
}

// union
function n(o: { p: {a: number}|{a: string}, ... }) {
  /*flowlint unnecessary-optional-chain:off*/ //Unused suppression
  // flowlint-next-line unnecessary-optional-chain:error
  o.p?.a; /* unnecessary */
  // flowlint unnecessary-optional-chain:error
}

function q(o: { p?: {a: number}, ... }) {
  o.p?.a; /* NOT unnecessary */
}

// Assignment

/*
Line suppressions don't have unexpected effects on later lines (the
unsuppression at the end of the line doesn't affect the existing range setting.)
*/
function z(x: {a: string}) {
  var assignee;
  //flowlint unnecessary-optional-chain:off
  //flowlint-next-line unnecessary-optional-chain:off
  (assignee = x)?.a; /* unnecessary; suppressed */
  (assignee = x)?.a; /* unnecessary; suppressed */
  //flowlint unnecessary-optional-chain:error
}

var value: {a: number} = {a: 0};
var maybeValue: ?{a: number} = null;
var valToUse = value?./* flowlint-line unnecessary-optional-chain:off */a; /* unnecessary; suppressed */

var maybeA = maybeValue?.a; /* NOT unnecessary */
var maybeComputedA = maybeValue?.['a']; /* NOT unnecessary */

//flowlint unnecessary-optional-chain:error
var val2: {a: number} = {a: 0};
/*flowlint unnecessary-optional-chain:off*/ //Unused suppression

var computedA = val2?.['a']; /* unnecessary */ /*flowlint-next-line unnecessary-optional-chain:error*/ /* flowlint-line unnecessary-optional-chain:error */
var a2 = val2?.a; /* unnecessary */

//Malformed Rule Tests

//Extra commas
// flowlint ,unnecessary-optional-chain:error,,uninitialized-instance-property:error,

//Missing commas
// flowlint unnecessary-optional-chain:error uninitialized-instance-property:off

//Typo on flowlint (Not checked for at the moment. Perhaps in a future diff?)
// flowline unnecessary-optional-chain:error

//Flowlint inside a comment (Not checked for at the moment. Perhaps in a future diff?)
/*
 * stuff, stuff, stuff
 * flowlint unnecessary-optional-chain:error
 */
