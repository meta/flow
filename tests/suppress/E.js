// It's possible to suppress a lint error with a normal suppression comment, but
// a suppression comment over a lint which was never enabled should not be
// considered "used." Instead, it should show up as an unused suppression.

// Note: this test assumes that unnecessary-optional-chain is never explicitly enabled

declare const x: {p: string};
// $FlowFixMe[unnecessary-optional-chain] - this is unused because unnecessary-optional-chain is off
x?.p;
