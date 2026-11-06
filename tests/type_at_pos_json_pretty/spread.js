// @flow

const base = {a: 1, b: "two"};
// Expanded output: structural dump alongside the type string
const extended = {...base, c: true};
//    ^ --pretty --expand-json-output
