// @flow

declare const x: {p: number};

//flowlint-next-line unnecessary-optional-chain:error
x?.p; // This should be an error

x?.p; // This should be a warning
