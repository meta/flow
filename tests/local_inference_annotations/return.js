//@flow

// If the entire expression has an annot available then we should be able to push
// the annotation of the return type down to the return expression.
let a = ((x => y => 3) as (x: number) => (y: number) => number);

let b = ((x => y => (z => 3)(3)) as (x: number) => (y: number) => number); // annot missing on z

let c = ((x => { return y => { return 3 }}) as (x: number) => (y: number) => number);

let d = ((x => { return y => { return (z => 3)(3) }}) as (x: number) => (y: number) => number); // annot missing on z

let e = ((x => {
  // Non-return statements do not have the annot available
  const xx = (x) => 3; // Missing annot on x
  return y => {
    const z = (x) => 3; // Missing annot on x
  return 3;
}}) as (x: number) => (y: number) => number);

let f = ((function () { return x => 3 }) as (() => (x: number) => number));
