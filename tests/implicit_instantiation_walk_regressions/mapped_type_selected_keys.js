type Pick<O, K extends keyof O> = {[P in K]: O[P]};

declare function pickInput<O extends {...}, Keys extends keyof O>(
  o: O,
  x: Pick<O, Keys>,
  ...keys: ReadonlyArray<Keys>
): void;

pickInput({foo: 3, bar: 3}, {foo: 3}, 'foo'); // ok
pickInput({foo: 3, bar: 3}, {bar: 3}, 'foo'); // ERROR 2x: `Keys` is `'foo'`, not `'bar' | 'foo'`

declare function pickOf<Keys extends 'foo' | 'bar'>(
  x: Pick<{foo: number, bar: number}, Keys>,
  key: Keys,
): void;

pickOf({bar: 3}, 'foo'); // ERROR 2x
