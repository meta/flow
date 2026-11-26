interface TypeInfo<V, H> {
  coerce(value: unknown): V;
  serialize(value: V): H;
}

declare class EnumInfo<T> {
  constructor(entries: {[key: string]: T}): void;
  coerce(value: unknown): string;
  serialize(value: string): T;
}

declare class NullableInfo<V, H, T extends TypeInfo<V, H>> {
  constructor(info: T): void;
  coerce(value: unknown): V | null;
  serialize(value: V | null): H | null;
}

type Args<T> = T extends TypeInfo<infer V, infer H> ? [V, H] : [];
type Nullable<T> = NullableInfo<Args<T>[0], Args<T>[1], T>; // error: T is not known to be a TypeInfo

interface Field<out T> {
  readonly info: T;
}

declare class FieldInfo<T> {
  constructor(info: T): void;
  readonly info: T;
}

declare class Creator<T> {
  static from(field: T): this; // ERROR - static members cannot reference `T`
  create(): T;
}

declare class EnumCreator<T> extends Creator<Field<EnumInfo<T> | Nullable<EnumInfo<T>>>> {}

// `from` does not constrain `T`, so `EnumCreator`'s `T` is `unknown`.
const info = new NullableInfo(new EnumInfo({DOG: 1}));
const field = new FieldInfo(info);
const result = EnumCreator.from(field).create();

const serialized: number | null = result.info.serialize('DOG'); // ERROR
result.info.serialize('DOG') as string; // ERROR

const optionalResult = EnumCreator.from?.(field)?.create();
const optionalSerialized: number | null | undefined = optionalResult?.info.serialize('DOG'); // ERROR
optionalResult?.info.serialize('DOG') as string; // ERROR

const plainField = new FieldInfo(new EnumInfo({DOG: 1}));
const plainResult = EnumCreator.from(plainField).create();
const plainSerialized: number | null = plainResult.info.serialize('DOG'); // ERROR
plainResult.info.serialize('DOG') as string; // ERROR

const stringField = new FieldInfo(new NullableInfo(new EnumInfo({DOG: 'dog'})));
const stringResult = EnumCreator.from(stringField).create();
const stringSerialized: string | null = stringResult.info.serialize('DOG'); // ERROR
stringResult.info.serialize('DOG') as number; // ERROR
