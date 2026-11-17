//@flow

let a = ((x) => 3) as (x: number) => number; // annot available

let b = (((x)  => { // This x has an annot available
  const b = ((x) => 3)(3); // This x does not
  const c = (x) => 3; // Neither does this one
}) as (x: number) => void);

let c = (function(x) { return 3 }) as (x: number) => number; // annot available

let d = ((function (x){ // This x has an annot available
  const b = ((x) => 3)(3); // This x does not
  const c = (x) => 3; // Neither does this one
}) as (x: number) => void);
