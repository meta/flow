//@flow

// If the entire expression has an annot available then we should be able to push
// the annotation of the return type down to the return expression, even
// through for/of loops 
let a = ((x => {
  for (let x of ((x) => 3)) { // Only the body is covered, missing annot! Also a type error
    const x = (y) => 3; // non-return, not covered!
    return y => 3; // Covered by the annot
  }
  return (y) => 3;
}) as (x: number) => (y: number) => number);
