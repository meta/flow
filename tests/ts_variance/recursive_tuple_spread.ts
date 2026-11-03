// Recursive tuple decomposition accumulates the required prefix.
type Parts<T extends readonly unknown[], Prefix extends unknown[] = []> =
  T extends readonly [infer Head, ...infer Tail]
    ? Parts<Tail, [...Prefix, Head]>
    : {required: Prefix}; // OK

// Recursive spreads reached through a type argument terminate during variance checking.
type SubTuples<T> = T extends readonly [infer Head, ...infer Rest]
  ? SubTuples<Rest> | [Head, ...SubTuples<Rest>] // ERROR
  : [];
type Sampled<T extends readonly unknown[]> = [...SubTuples<Parts<T>['required']>]; // OK

// Concrete instantiations keep every possible ordered subtuple.
const both: Sampled<[number, string]> = [1, 'a']; // OK
const numberOnly: Sampled<[number, string]> = [1]; // OK
const stringOnly: Sampled<[number, string]> = ['a']; // OK
const neither: Sampled<[number, string]> = []; // OK
const invalid: Sampled<[number, string]> = [false]; // ERROR
