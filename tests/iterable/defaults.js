declare const iterator: Iterator<number>;
iterator.next().value as number; // ok: `TReturn` defaults to `any`

declare const iteratorObject: IteratorObject<number>;
iteratorObject.next().value as number; // error: `TReturn` defaults to `unknown`

[1].values().next().value as number; // error: built-in iterators return `undefined` when done
