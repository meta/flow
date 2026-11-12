declare const returnsNumber: (event: string) => number;

takeTsVoidHandler(returnsNumber); // OK: a TypeScript `=> void` accepts any return type
takeTsUndefinedHandler(returnsNumber); // ERROR: `undefined` is not `void`

tsReturnsVoid() as void; // OK
tsReturnsVoid() as undefined; // ERROR
