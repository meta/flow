// Empty record
{
  record R {}

  declare const x: R;

  x as R; // OK
  x.xxx; // ERROR: property not found
}

// Basic field access
{
  record R {
    a: number,
  }

  declare const x: R;

  x.a as number; // OK
  x.a as empty; // ERROR: `number`
}

// Multiple fields
{
  record R {
    a: number,
    b: string,
    c: boolean,
  }

  declare const x: R;

  x.a as number; // OK
  x.b as string; // OK
  x.c as boolean; // OK
  x.xxx; // ERROR: property not found
}

// Read-only
{
  record R {
    a: number,
  }

  declare const x: R;

  x.a = 1; // ERROR: not writable
}

// Type alias with records
{
  record R {
    value: number,
  }

  type Alias = R;
  declare const x: Alias;

  x as R; // OK
  x.value as number; // OK
}

// Destructuring
{
  record R {
    a: number,
    b: string,
    c: boolean,
  }

  declare const x: R;

  const {a: renamed} = x;
  renamed as number; // OK

  const {a, ...rest} = x;
  a as number; // OK
  rest.b as string; // OK
  rest.c as boolean; // OK

  const {...asObj} = x;
  asObj as {a: number, b: string, c: boolean}; // OK
}

// Methods
{
  record R {
    a: number,

    double(): number {
      return this.a ** 2;
    }

    // Referencing other method
    quadruple(): number {
      return this.double() * 2; // OK
    }

    equals(other: R): boolean {
      // Using `this`
      return this.a === this.a; // OK
    }
  }

  declare const x: R;

  x.double() as number; // OK
  x.quadruple() as number; // OK

  declare const y: R;
  x.equals(y) as boolean; // OK
}

// Static methods
{
  record R {
    a: number,

    static sum(x: R, y: R): number {
      return x.a + y.a;
    }
    static random(): R {
      declare const x: R;
      return x;
    }
  }

  declare const x: R;
  declare const y: R;

  R.sum(x, y) as number; // OK
  R.random() as R; // OK
  R.random = () => x; // ERROR: method not writable
}

// Static values
{
  record R {
    static zero: number = 0,
  }

  R.zero as number; // OK
  R.zero as empty; // ERROR: `number`
  R.zero = 1; // ERROR: static not writable
}

// Static methods with generics
{
  record R<T> {
    value: T,

    static create<U>(val: U): R<U> {
      declare const result: R<U>;
      return result;
    }
  }

  R.create(42) as R<number>; // OK
  R.create("test") as R<string>; // OK
}

// Equality
{
  record R {
    a: number,
  }

  declare const x: R;

  if (x === x) {} // OK
  if (x === 1) {} // ERROR
}

// Definitions for below
interface Equatable<T> {
  equals(other: T): boolean;
}

// Subtyping with interfaces
{
  record R {
    a: number,

    equals(other: R): boolean {
      return this.a === this.a;
    }
  }

  declare const x: R;

  x as Equatable<R>; // OK

  interface A {
    readonly a: number;
  }
  x as A; // OK

  interface E {
    xxx: number,
  }
  x as E; // ERROR
}

// Subtyping with objects not allowed
{
  record R {
    a: number,
  }

  declare const x: R;

  x as {readonly a: number}; // ERROR
  x as {readonly a: number, ...}; // ERROR
}

// Generics and implements
{
  record R<T> implements Equatable<R<T>> {
    foo: string,
    bar: T,
    equals(other: R<T>): boolean {
      return this.foo === other.foo && this.bar === other.bar;
    }
    static yes(): boolean {
      return true;
    }
  }
  declare const x: R<number>;
  declare const y: R<number>;

  x.equals(y) as boolean; // OK
  R.yes() as boolean; // OK
}

// `instanceof` refinement and `match` instance patterns
{
  record RN {
    a: number,
  }

  record RS {
    a: string,
  }

  declare const x: RN | RS;

  if (x instanceof RN) {
    x.a as number; // OK
  } else if (x instanceof RS) {
    x.a as string; // OK
  } else {
    x as empty; // OK
  }

  match (x) {
    RN {const a} => {
      a as number; // OK
    }
    RS {const a} => {
      a as string; // OK
    }
  }
}

// `typeof` refinement
{
  record R {
    a: number,
  }

  declare const x: R | void;

  if (typeof x === 'object') {
    x as R; // OK
  } else {
    x as void; // OK
  }
}

// Spread
{
  record R {
    a: number,

    double(): number {
      return this.a ** 2;
    }
  }

  declare const x: R;

  const o: {a: number} = {...x}; // OK
}

// Record itself
{
  record R {
    a: number,
  }

  const O = {
    r: R,
  };

  declare const x: O.r;

  x.a as number; // OK

  R = null; // ERROR: cannot reassign
}

// Multiple generic parameters
{
  record R<T, U> {
    first: T,
    second: U,

    swap(): R<U, T> {
      declare const result: R<U, T>;
      return result;
    }
  }

  declare const x: R<number, string>;

  x.first as number; // OK
  x.second as string; // OK
  x.swap() as R<string, number>; // OK
}

// Multiple implements
{
  interface I1 {
    foo(): string;
  }

  interface I2 {
    bar(): number;
  }

  record R implements I1, I2 {
    foo(): string {
      return "test";
    }
    bar(): number {
      return 42;
    }
  }

  declare const x: R;

  x as I1; // OK
  x as I2; // OK
  x.foo() as string; // OK
  x.bar() as number; // OK
}

// Generic constraints
{
  record R<T extends number | string> {
    value: T,
  }

  declare const x: R<number>; // OK
  declare const y: R<string>; // OK
  declare const z: R<boolean>; // ERROR

  x.value as number; // OK
  y.value as string; // OK
}

// Nested record types
{
  record Inner {
    value: number,
  }

  record Outer {
    inner: Inner,
    name: string,
  }

  declare const x: Outer;

  x.inner as Inner; // OK
  x.inner.value as number; // OK
  x.name as string; // OK
}

// Records cannot be extended or implemented
{
  record R {
    a: number,
  }

  class C extends R { // ERROR
  }

  class D implements R { // ERROR
  }
}

// Banned utility types
{
  record R {
    a: number,
  }

  declare const x: R;

  type ReadonlyR = Readonly<R>; // ERROR
  type PartialR = Partial<R>; // ERROR
  type RequiredR = Required<R>; // ERROR
  type PickR = Pick<R, 'a'>; // ERROR
  x as PickR;
  type OmitR = Omit<R, 'a'>; // ERROR
  x as OmitR;
  type ExactR = $Exact<R>; // ERROR

  declare function fun<T>(x: Omit<T, 'a'>): T;
  fun(x); // ERROR
}

// Spread is allowed
{
  record R {
    a: number,
    b: string,
  }

  type ObjR = {...R}; // OK
  ({a: 1, b: 's'}) as ObjR; // OK

  type OmitObjR = Omit<{...R}, 'a'>; // OK
  ({a: 1, b: 's'}) as OmitObjR; // ERROR: property `a` is extra
  ({ b: 's'}) as OmitObjR; // OK
}

// keyof and Values are allowed
{
  record R {
    a: number,
    b: string,
  }

  type KeyR = keyof R; // OK
  'a' as KeyR; // OK
  'b' as KeyR; // OK

  type ValuesR = Values<R>; // OK
  0 as ValuesR; // OK
  's' as ValuesR; // OK
}

// new
{
  record R {
    a: number,
    b: string,
  }

  const oEmpty = {};
  const oB = {b: 's'};
  const oAll = {a: 1, b: 's'};

  new R(oAll); // OK
  new R(oB); // ERROR: missing `a`
  new R(oEmpty); // ERROR: missing `a` and `b`

  record ROpt {
    a: number = 0,
    b: string = '',
  }

  new ROpt(oEmpty); // OK
  new ROpt(oB); // OK
  new ROpt(oAll); // OK

  const oExtra = {a: 1, b: 's', xxx: false};
  new R(oExtra); // ERROR
  new ROpt(oExtra); // ERROR
}

// Duplicate property name
{
  record R {
    a: number,
    a: number, // ERROR
  }
}
{
  record R {
    '10': number,
    10: number, // ERROR
    10n: number, // ERROR
  }
}

// Invalid default values
{
  record R {
    a: number = true, // ERROR
  }
}

// Key names
{
  record R {
    42: number,
    'foo': string,
    class: boolean,
  }

  declare const x: R;
  x[42] as number; // OK
  x['foo'] as string; // OK
  x.foo as string; // OK
  x.class as boolean; // OK
}
{
  record R {
    static 42: number = 0,
    static 'foo': string = '',
    static class: boolean = false,
  }
  R[42] as number; // OK
  R['foo'] as string; // OK
  R.foo as string; // OK
  R.class as boolean; // OK
}
