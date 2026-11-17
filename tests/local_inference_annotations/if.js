//@flow

// If the entire expression has an annot available then we should be able to push
// the annotation of the return type down to the return expression, even
// through if statements 
let a = ((x => {
  if (((x) => true) ()){ // Guards are not covered by the annot
    return y => 3;
  } else if (true) {
    const z = (x) => 3; // Non-return statements are not covered by the annot
    return y => 3;
  } else {
    return y => 3;
  }
}) as (x: number) => (y: number) => number);

let b = ((x => {
  // Blocks are not necessary
  if (true) return (y) => 3
  else return (y) => 3
}) as (x: number) => (y: number) => number);
