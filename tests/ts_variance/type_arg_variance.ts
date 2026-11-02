// Generic with a read-write tparam slot (`T` appears in `value: T`,
// so Flow infers Neutral polarity). TS treats these as covariant in
// instantiation; Flow matches that in .ts.

declare class Animal {}
declare class Dog extends Animal {
  bark(): void;
}

declare class Box<T> {
  value: T;
}

declare const dogBox: Box<Dog>;
const a: Box<Animal> = dogBox; // OK in .ts

// The gate replaces invariance with covariance, not bivariance --
// genuinely incompatible type args must still error.
declare const numBox: Box<number>;
const b: Box<string> = numBox; // ERROR: number is not assignable to string

// Positive (out T) tparam: T appears only in covariant positions. `Dog`
// has a public member not present on `Animal`, so the reverse direction is
// a real structural mismatch in both Flow and TS.
// The .ts gate must NOT relax the wrong direction here -- it gates only
// the Neutral arm of type_app_variance_check.
declare class Producer<out T> {
  produce(): T;
}
declare const dogProducer: Producer<Dog>;
const p_ok: Producer<Animal> = dogProducer; // OK: standard covariance, not affected by gate
declare const animalProducer: Producer<Animal>;
const p_err: Producer<Dog> = animalProducer; // ERROR: Animal not assignable to Dog (covariance)

// Negative (in T) tparam: T appears only in contravariant positions.
// Again, the .ts gate must NOT relax the wrong direction here.
declare class Consumer<in T> {
  consume(t: T): void;
}
declare const animalConsumer: Consumer<Animal>;
const c_ok: Consumer<Dog> = animalConsumer; // OK: standard contravariance, not affected by gate
declare const dogConsumer: Consumer<Dog>;
const c_err: Consumer<Animal> = dogConsumer; // ERROR: Dog not assignable to Animal (contravariance)

// Explicit `in out` tparam: invariant, as in TS. The gate does not apply.
declare class InvariantBox<in out T> {
  value: T;
}
declare const dogInvariantBox: InvariantBox<Dog>;
const i_err: InvariantBox<Animal> = dogInvariantBox; // ERROR: invariant
declare class DogInvariantBox extends InvariantBox<Dog> {}
declare const dogInvariantBoxSub: DogInvariantBox;
const i_sub_err: InvariantBox<Animal> = dogInvariantBoxSub; // ERROR: invariant

type InvariantValue<in out T> = {value: T};
declare const dogValue: InvariantValue<Dog>;
const v_err: InvariantValue<Animal> = dogValue; // ERROR: invariant
