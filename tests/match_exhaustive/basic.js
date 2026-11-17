// Literal strings, numbers, bigint, & unary
{
  declare const x: 'foo' | 1 | -2 | 3n;

  match (x) { // OK
    'foo' => {}
    1 => {}
    -2 => {}
    3n => {}
  }

  match (x) { // ERROR: missing `'foo'`
    1 => {}
    -2 => {}
    3n => {}
  }

  match (x) { // ERROR: missing `1`, `-2`, `3n`
    'foo' => {}
  }
}

// Negative bigint
{
  declare const x: -3n;

  match (x) { // OK
    -3n => {}
  }

  match (x) { // ERROR: missing `-3n` (positive `3n` is a different value)
    3n => {} // ERROR: unused pattern, `3n` is not part of input type `-3n`
  }
}

// Negative and positive bigint are distinct
{
  declare const x: -3n | 3n;

  match (x) { // OK
    -3n => {}
    3n => {}
  }

  match (x) { // ERROR: missing `3n`
    -3n => {}
  }
}

// Boolean
{
  declare const x: boolean;

  match (x) { // OK
    true => {}
    false => {}
  }

  match (x) { // ERROR: missing `false`
    true => {}
  }
}

// Null and void
{
  declare const x: null | void;

  match (x) { // OK
    null => {}
    undefined => {}
  }

  match (x) { // ERROR: missing `undefined`
    null => {}
  }
}

// Optional
{
  declare const obj: {prop?: 1};

  match (obj.prop) { // OK
    1 => {}
    undefined => {}
  }

  match (obj.prop) { // ERROR: missing `undefined`
    1 => {}
  }
}

// Maybe
{
  declare const x: ?1;

  match (x) { // OK
    1 => {}
    null => {}
    undefined => {}
  }

  match (x) { // ERROR: missing `null`
    1 => {}
    undefined => {}
  }

  match (x) { // ERROR: missing `undefined`
    1 => {}
    null => {}
  }
}

// Identifier patterns
{
  declare const str: 'foo';
  declare const num: 1;
  declare const neg: -2;
  declare const big: 3n;
  declare const t: true;
  declare const f: false;
  declare const nll: null;
  declare const u: void;

  declare const x: 'foo' | 1 | -2 | 3n | boolean | null | void;

  match (x) { // OK
    str => {}
    num => {}
    neg => {}
    big => {}
    t => {}
    f => {}
    nll => {}
    u => {}
  }

  match (x) { // ERROR: missing `true`
    str => {}
    num => {}
    neg => {}
    big => {}
    f => {}
    nll => {}
    u => {}
  }
}

// Member patterns
{
  declare const obj: {
    str: 'foo',
    num: 1,
    neg: -2,
    big: 3n,
    t: true,
    f: false,
    nll: null,
    u: void,
  }

  declare const x: 'foo' | 1 | -2 | 3n | boolean | null | void;

  match (x) { // OK
    obj.str => {}
    obj.num => {}
    obj.neg => {}
    obj.big => {}
    obj.t => {}
    obj.f => {}
    obj.nll => {}
    obj.u => {}
  }

  match (x) { // ERROR: missing `true`
    obj.str => {}
    obj.num => {}
    obj.neg => {}
    obj.big => {}
    obj.f => {}
    obj.nll => {}
    obj.u => {}
  }
}

// Wildcard and binding patterns: match everything
{
  declare const x: 1 | 2;

  match (x) { // OK
    _ => {}
  }

  match (x) { // OK
    1 => {}
    _ => {}
  }

  match (x) { // ERROR: unnecessary wildcard
    1 => {}
    2 => {}
    _ => {}
  }

  match (x) { // OK
    const a => {}
  }

  match (x) { // OK
    1 => {}
    const a => {}
  }

  match (x) { // ERROR: unnecessary wildcard
    1 => {}
    2 => {}
    const a => {}
  }
}

// As patterns: match associated pattern
{
  declare const x: 1 | 2;

  match (x) { // OK
    1 as a => {}
    2 as a => {}
  }

  match (x) { // ERROR: missing `2`
    1 as a => {}
  }
}

// Or patterns
{
  declare const x: 1 | 2 | 3;

  match (x) { // OK
    1 | 2 => {}
    3 => {}
  }

  match (x) { // OK
    1 | 2 | 3 => {}
  }
}

// Object keys
{
  declare const obj: {
    foo: 0,
    bar: 0,
  };

  declare const x: keyof typeof obj;

  match (x) { // OK
    'foo' => {}
    'bar' => {}
  }

  match (x) { // ERROR: missing `'foo'`
    'bar' => {}
  }
}

// Object keys with indexer
{
  declare const obj: {
    foo: 0,
    bar: 0,
    [key: string]: boolean,
  };

  declare const x: keyof typeof obj;

  match (x) { // ERROR
    'foo' => {}
    'bar' => {}
  }

  match (x) { // OK
    'foo' => {}
    'bar' => {}
    _ => {}
  }
}

// Indirect
type O = {
  foo: 1;
  bar: 2;
};
type Mapped = {[K in keyof O]: [O[K]]};
{
  declare const x: Values<Mapped>[0];

  match (x) { // OK
    1 => {}
    2 => {}
  }

  match (x) { // ERROR: missing `2`
    1 => {}
  }

  declare const obj: Mapped;

  match (x) { // OK
    obj.foo[0] => {}
    obj.bar[0] => {}
  }

  match (x) { // ERROR: missing `2`
    obj.foo[0] => {}
  }
}

// Many missing patterns
{
  declare const x: 1 | 2 | 3 | 4 | 5 | 6 | 7;

  match (x) { // ERROR: missing 1..5 and 2 others
  }
}

// Unnecessary basic
{
  declare const x: 1 | 2;

  match (x) {
    1 => {}
    'foo' => {} // ERROR
    2 => {}
    true => {} // ERROR
  }
}

// Invalid any-typed pattern
{

  declare const invalid: any;

  declare const x: 0;

  match (x) {
    invalid => {} // OK
    0 => {}
  }
}

// Generics
{
  function f<T>(x: T) {
    match (x) { // OK
      1 => {} // OK: used
      _ => {}
    }

    match (x) {} // ERROR: missing wildcard `_`
  }

  function g<T extends 'foo' | 'bar'>(x: T) {
    match (x) {} // ERROR

    match (x) { // OK
      'foo' => {}
      'bar' => {}
    }
  }
}

// Intersection
{
  declare const x: {foo: 0, ...} & {bar: 1, ...};

  match (x) {} // ERROR

  match (x) { // OK
    {foo: _, ...} => {}
  }

  match (x) { // OK
    _ => {}
  }
}

// `undefined` is always marked as used
{
  const obj: {[key: string]: 0} = {};

  match (obj['foo']) { // OK
    0 => {}
    undefined => {} // OK
  }
}

// Invalid leaf with unnecessary pattern
{
  declare const x: 0 | 1;

  match (x) { // OK
    0 => {}
    1 => {}
    +1n => {} // ERROR: invalid pattern (doesn't error for unnecessary pattern)
    999 => {} // ERROR: unnecessary pattern
  }
}

// String leaf that needs proper quoting
{
  declare const x: "baz'bort" | 'foo"bar'

  match (x) {} // ERROR: properly quoted
}
