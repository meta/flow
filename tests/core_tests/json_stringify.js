declare const bad1: unknown;
declare const bad2: () => {};
declare const good1: number;
declare const good2: {foo: string};

// ok: as in TypeScript, the result is typed `string`, even for values that stringify to `undefined`
JSON.stringify(bad1) as string;
JSON.stringify(bad2) as string;

JSON.stringify(good1) as string; // ok
JSON.stringify(good2) as string; // ok
