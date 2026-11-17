declare function find<T, NSV>(
  values: Array<T>,
  predicate: (value: T) => boolean,
  notSetValue?: NSV,
): T | NSV;
const found = find([1, 2], value => value > 0);
found as number | void;
found as string; // ERROR

const withFallback = find([1, 2], value => value > 0, 'none');
withFallback as number | string;
withFallback as number; // ERROR

declare function defaulted<T = string>(value?: T): T;
const omitted = defaulted();
omitted as string;
omitted as number; // ERROR

const supplied = defaulted(1);
supplied as number;
supplied as string; // ERROR

declare function required<T extends number = 1>(value: T): T;
required(); // ERROR

declare function identity<T = string>(value: T): T;
const suppliedVoid = identity(undefined);
suppliedVoid as void;
suppliedVoid as string; // ERROR

type Identity<T> = T;
declare function aliased<T = string>(value?: Identity<T>): T;
const aliasDefault = aliased();
aliasDefault as string;
aliasDefault as number; // ERROR

declare function nullable<T = string>(value: ?T): T;
const nullableDefault = nullable();
nullableDefault as string;
nullableDefault as number; // ERROR

declare function multiple<T = string>(first?: T, second?: T): T;
const actualArgument = multiple(1);
actualArgument as number;
actualArgument as string; // ERROR

declare const bottom: empty;
const actualEmpty = identity(bottom);
actualEmpty as empty; // ERROR

declare function parameterized<P = string>(): (value: P) => number;
const contextual: () => number = parameterized();

declare function contextualOptional<P = string>(): (value?: P) => P;
const contextualDefault: () => string = contextualOptional();
