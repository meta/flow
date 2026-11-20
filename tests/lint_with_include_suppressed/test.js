/* @flow */

//never enabled suppression; not added by --include-suppressed
var a: {p: number} = {p: 0};
a?.p;

//flowlint unnecessary-optional-chain:warn
var b: {p: number} = {p: 0};
//flowlint-next-line unnecessary-optional-chain:off
b?.p; //warning included by --include-suppressed (reported as an error)

//flowlint unnecessary-optional-chain:error
var c: {p: number} = {p: 0};
//flowlint-next-line unnecessary-optional-chain:off
c?.p; //error included by --include-suppressed
