//@flow

// If the entire expression has an annot available then we should be able to push
// the annotation of the return type down to the return expression, even
// through while loops 
let a = ((x => {
  while ((x) => 3) { // Only the body is covered, missing annot!
    const x = (y) => 3; // non-return, not covered!
    return y => 3; // Covered by the annot
  }
  return (y) => 3;
}) as (x: number) => (y: number) => number);
