// @flow

//flowlint-next-line unnecessary-optional-chain:error
var x: {a: number} = {a: 0};

//flowlint-next-line unnecessary-optional-chain:error
x?.a; //Error

//flowlint-next-line unnecessary-optional-chain:warn
x?.a; //Warning
