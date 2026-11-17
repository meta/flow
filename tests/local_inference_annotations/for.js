//@flow

// If the entire expression has an annot available then we should be able to push
// the annotation of the return type down to the return expression, even
// through for loops
let a = ((x => {
  for ((x) => true; (x) => true; 1) { // init, and test are all not covered by the annot
    const x = (y) => 3; // non-return, not covered!
    return y => 3; // Covered by the annot
  }
  return (y) => 3;
}) as (x: number) => (y: number) => number);

for (;; (x) => true) {} // update is not covered by the annot

let b = ((x => {
  // Blocks are not necessary
  for (;;)
  return (y) => 3;

  return (y) => 3;
}) as (x: number) => (y: number) => number);
