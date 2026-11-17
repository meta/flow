//@flow

// If the entire expression has an annot available then we should be able to push
// the annotation of the return type down to the return expression, even
// through do/while loops 
let a = ((x => {
  do {
    const x = (y) => 3; // non-return, not covered!
    return y => 3; // Covered by the annot
  }
  while ((x) => 3); // guard is not covered!
  return (y) => 3;
}) as (x: number) => (y: number) => number);
