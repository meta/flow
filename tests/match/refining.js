// Primitive value patterns
{
  declare const x: 1 | -2 | 3n | 's' | false | null;

  const e = match (x) {
    1 => 0,
    false => 0,
    3n => 0,
    null => 0,
    const test => test as empty, // ERROR: `-2 | 's'`
  };
}

// Negative bigint patterns refine distinctly from positive
{
  declare const x: -3n | 3n;

  const e1 = match (x) {
    -3n => 0,
    const test => test as 3n, // OK: only `3n` remains after removing `-3n`
  };

  const e2 = match (x) {
    -3n => 0,
    const test => test as empty, // ERROR: `3n`
  };

  const e3 = match (x) {
    3n => 0,
    const test => test as -3n, // OK: only `-3n` remains after removing `3n`
  };
}

// Identifier patterns
{
  declare const x: 1 | 2;

  declare const one: 1;
  declare const two: 2;

  const e = match (x) {
    one => 0,
    const test => test as empty, // ERROR: `2`
  };
}

// `undefined`
{
  declare const x: 1 | void;

  const e = match (x) {
    1 => 0,
    const test => test as empty, // ERROR: void
  };
}

// Maybe types
{
  declare const x: ?1;

  const e = match (x) {
    1 => 0,
    const test => test as empty, // `null | undefined`
  };
}

// Member patterns
{
  declare const x: 1 | 2;

  declare const o: {
    one: 1,
    two: 2,
  };

  const e = match (x) {
    o.one => 0,
    const test => test as empty, // ERROR: `2`
  };
}

// Or pattern
{
  declare const x: 1 | 2 | 3;

  const e = match (x) {
    1 | 2 => true,
    const test => test as empty, // ERROR `3`
  };
}

// Patterns with guard could match or not match
{
  declare const x: 1 | 2;

  declare function f(): boolean;

  const e = match (x) {
    1 => 0,
    2 if (f()) => 0,
    const test => test as empty, // ERROR: `2`
  };
}

// Property exists
{
  declare const x: {foo: void, a: 0} | {bar: void, a: 1};

  const e = match (x) {
    {foo: _, const a} => a as 0,
    const test => test as empty, // ERROR: `{bar: void, a: 1}`
  };
}

// Disjoint object union
{
  declare const x: {type: 'foo', val: number}
                 | {type: 'bar', val: string}
                 | {type: 'baz', val: boolean};

  const e1 = match (x) {
    {type: 'foo', val: const a} => a as number, // OK
    {type: 'bar', val: const a} => a as string, // OK
    const test => test as empty, // ERROR: `{type: 'baz', val: boolean}`
  };

  // Using idents as pattern
  declare const foo: 'foo';
  declare const bar: 'bar';
  declare const baz: 'baz';
  const e2 = match (x) {
    {type: foo, val: const a} => a as number, // OK
    {type: bar, val: const a} => a as string, // OK
    const test => test as empty, // ERROR: `{type: 'baz', val: boolean}`
  };

  // Using members as pattern
  declare const o: {
    foo: 'foo',
    bar: 'bar',
    baz: 'baz',
  };
  const e3 = match (x) {
    {type: o.foo, val: const a} => a as number, // OK
    {type: o.bar, val: const a} => a as string, // OK
    const test => test as empty, // ERROR: `{type: 'baz', val: boolean}`
  };
}

// Combo union of object with sentinel property and primitive value
{
  declare const x: null | {type: 'bar', val: number};

  const e = match (x) {
    {type: 'bar', val: const a} => a as number, // OK
    const test => test as empty, // ERROR: `null`
  };
}

// Or pattern: objects
{
  declare const x: {type: 'foo', val: number}
                 | {type: 'bar', val: string}
                 | {type: 'baz', val: boolean};

  const e = match (x) {
    {type: 'foo', ...} | {type: 'baz', ...} => 0,
    const test => test as empty, // ERROR: `{type: 'bar', val: string}`
  };
}

// Disjoint tuple union
{
  declare const x: ['foo', number]
                 | ['bar', string]
                 | ['baz', boolean];

  const e = match (x) {
    ['foo', const a] => a as number, // OK
    ['bar', const a] => a as string, // OK
    const test => test as empty, // ERROR: `['baz', boolean]`
  };
}

// Combo union of tuples with sentinel property and primitive value
{
  declare const x: null | ['bar', number] | ['foo', string];

  const e = match (x) {
    ['bar', const a] => a as number, // OK
    null => 0,
    const test => test as empty, // ERROR: `['foo', string]`
  };
}

// Tuple length refinements
{
  declare const x: [number]
                 | [string, string]
                 | [boolean, boolean, boolean];

  const e = match (x) {
    [const a] => a as number, // OK
    [const a, _] => a as string, // OK
    const test => test as empty, // ERROR: [boolean, boolean, boolean]
  };
}
{
  declare const x: [number] | Array<string>;

  const e = match (x) {
    [_] => 0,
    const d => d as empty, // ERROR: `Array<string>`
  };
}

// Type parameters keep their identity
{
  function f<T extends {readonly a: 1} | {readonly b: 2}>(x: T): T {
    return match (x) {
      const y => y, // OK
    };
  }

  function g<T extends 'a' | 'b'>(x: T): T {
    match (x) {
      'a' => {
        return x;
      }
      const y => {
        y as 'b'; // OK
        return y; // OK
      }
    }
  }
}

// Opaque types keep their identity
{
  declare opaque type URI: Readonly<{}> | Readonly<{uri: string}>;
  declare function byURI(uri: URI): void;

  declare const u: URI;
  match (u) {
    const x => {
      byURI(x); // OK
    }
  }

  declare const s: {readonly e: URI} | {readonly b: null};
  match (s) {
    {e: const e} => {
      byURI(e); // OK
    }
    _ => {}
  }
}
