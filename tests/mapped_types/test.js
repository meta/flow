type O = { foo: number }
type Arr = Array<number>;
type ROArr = ReadonlyArray<number>;
type Tuple = [a: number, readonly b?: string];
type Box<T> = {contents: T};

type WithIndexer = {
  foo: number,
  [key: string]: string,
};


type Mapped<O extends {...} | ReadonlyArray<unknown>> = {
  [key in keyof O]: Box<O[key]>,
};

// MappedType ~> ObjT
{
  declare const o: Mapped<O>;
  o as {foo: {contents: number}}; // OK
}

// MappedType ~> array & tuple
{
  declare const a: Mapped<Arr>;
  declare const b: Mapped<ROArr>;
  declare const c: Mapped<Tuple>;
  a as Array<{contents: number}>; // OK
  b as ReadonlyArray<{contents: number}>; // OK
  c as [a: {contents: number}, readonly b?: {contents: string | undefined}]; // OK
  a as empty; // ERROR
  b as empty; // ERROR
  c as empty; // ERROR
}

// ObjT ~> MappedType
{
  declare const o: {foo: {contents: number}};
  o as Mapped<O>; // OK

  declare const badKey: {bar: {contents: number}};
  badKey as Mapped<O>; // ERROR

  declare const badVal: {foo: {contents: string}};
  badVal as Mapped<O>; // ERROR
}

// array & tuple ~> MappedType
{
  declare const a: Array<{contents: number}>;
  declare const b: ReadonlyArray<{contents: number}>;
  declare const c: [a: {contents: number}, readonly b?: {contents: string | undefined}];
  declare const badA: Array<{contents: string}>;
  declare const badB: ReadonlyArray<{contents: string}>;
  declare const badC: [a: {contents: number}, readonly b: {contents: string}];
  a as Mapped<Arr>; // OK
  b as Mapped<ROArr>; // OK
  c as Mapped<Tuple>; // OK
  badA as Mapped<Arr>; // ERROR
  badB as Mapped<ROArr>; // ERROR
  badC as Mapped<Tuple>; // ERROR
}

// No mapped types in declared classes or interfaces
{
  declare class DeclaredClass {
    [x in keyof O]: number; // ERROR
  }
  interface I {
    [x in keyof O]: number; // ERROR
  }
}

// Mapped types with indexers
{
  type MappedIndexer = Mapped<WithIndexer>;
  declare const mappedIndexer: MappedIndexer;

  mappedIndexer.foo as number; // ERROR
  mappedIndexer.bar as string; // ERROR
  mappedIndexer as {foo: Box<number>, [key: string]: Box<string>}; // OK

  declare const indexer: {foo: Box<number>, [key: string]: Box<string>};
  indexer as MappedIndexer; // OK
}

// Variance
{
  type ReadOnly<T extends {...}> = {readonly [key in keyof T]: T[key]};
  declare const readonly: ReadOnly<O>;
  readonly.foo as number; // OK
  readonly.foo = 3; // ERROR

  type WriteOnly<T extends {...}> = {writeonly [key in keyof T]: T[key]};
  declare const writeonly: WriteOnly<O>;
  writeonly.foo as number; // ERROR
  writeonly.foo = 3; // OK

  type ReadOnlyIndexer = ReadOnly<WithIndexer>;
  declare const readonlyIndexer: ReadOnlyIndexer;
  readonlyIndexer.qux as string; // OK
  readonlyIndexer.qux = 'str'; // ERROR

  type _ReadOnlyArr = {readonly [key in keyof Arr]: Arr[key]}; // OK
}

// Optionality
{
  type Partial<T extends {...} | ReadonlyArray<unknown>> = {[key in keyof T]?: T[key]};
  declare const partial: Partial<O>;
  partial.foo as number; // ERROR
  partial.foo as number | void; // OK

  declare const partialIndexer: Partial<WithIndexer>;
  partialIndexer.qux as string; // ERROR
  partialIndexer.qux as string | undefined; // OK

  declare const partialArr: Partial<Arr>;
  partialArr[0] as number; // ERROR;
  declare const partialTuple: Partial<Tuple>;
  partialTuple[0] as number; // ERROR;
}

// Error positioning
{
  type ConstrainedBox<T extends string> = Box<T>;
  type MappedConstrained<O extends {...}> = {
    [key in keyof O]: ConstrainedBox<O[key]>, // LIMITATION: not an error, as `keyof O` only has the keys of the bound of `O`
  };

  declare const constrained: MappedConstrained<O>;
  constrained as {foo: {contents: number}}; // OK
}

// Error positioning
{
  type UnconstrainedKey<T> = {[key in T]: number};
  type BadKeys = UnconstrainedKey<boolean>; // ERROR HERE, NOT LINE ABOVE
  declare const badKeys: BadKeys;
  badKeys as empty; // ERROR
}
