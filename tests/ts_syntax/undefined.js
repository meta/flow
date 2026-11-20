type T = undefined; // OK

const x = undefined; // OK

type S = typeof undefined; // OK

// `undefined` is allowed in library definitions, where it means `void`
libReturnsUndefined() as string | void; // OK
libReturnsUndefined() as string; // ERROR
