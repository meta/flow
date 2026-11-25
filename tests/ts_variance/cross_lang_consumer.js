// @flow
// .js consumer of the same .ts-defined generic. The imported type retains
// TypeScript's covariant treatment of its neutral parameter.

import {
  type Animal,
  type AnimalMethod,
  type BoxArgs,
  type Dog,
  type DogMethod,
  type MutableTuple,
  type MutableValue,
  type NarrowTuple,
  type NarrowValue,
  type OptionalValues,
  type PresentValue,
  type ReadonlyTuple,
  type ReadonlyValue,
  type WideTuple,
  type WideValue,
  Box,
  InvariantBox,
  setFirst,
  takesA,
} from "./cross_lang_lib";

declare const dogBox: Box<Dog>;
const a: Box<Animal> = dogBox; // OK: Box comes from .ts

type BoxView<out T> = Box<T>; // OK: Box comes from .ts
type InvariantBoxView<out T> = InvariantBox<T>; // ERROR: `in out` is invariant

declare const dogMethod: DogMethod;
const animalMethod: AnimalMethod = dogMethod; // OK: methods come from .ts

declare const readonlyValue: ReadonlyValue;
const mutableValue: MutableValue = readonlyValue; // OK: properties come from .ts

declare const mutableTuple: MutableTuple;
const readonlyTuple: ReadonlyTuple = mutableTuple; // OK: tuples come from .ts

declare const narrowValue: NarrowValue;
const wideValue: WideValue = narrowValue; // OK: properties come from .ts

declare const presentValue: PresentValue;
const optionalValues: OptionalValues = presentValue; // OK: optional properties come from .ts

declare const narrowTuple: NarrowTuple;
const wideTuple: WideTuple = narrowTuple; // OK: tuples come from .ts
const narrowTuple2: NarrowTuple = wideTuple; // ERROR: `string` is not `"a"`

declare const argsA: BoxArgs<"a">;
const argsString: Box<string> | BoxArgs<string> = argsA; // OK: tuples come from .ts

declare const arrayA: Array<"a">;
setFirst(arrayA, "b"); // OK: arrays come from .ts

type Params<F> = F extends (...args: infer A) => unknown ? A : empty;
declare const paramsA: Params<typeof takesA>;
const paramsString: [string] = paramsA; // OK: the function comes from .ts
