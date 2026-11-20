/**
 * @format
 * @flow
 */

/* flowlint unnecessary-optional-chain:warn */

const x: {p: number} = {p: 42};

// Warning: Unnecessary optional chain
x?.p;
