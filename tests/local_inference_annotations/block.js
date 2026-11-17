//@flow

// If the entire expression has an annot available then we should be able to push
// the annotation of the return type down to the return expression, even
// through blocks
let a = ((x => {
  { return y => 3 };
}) as (x: number) => (y: number) => number);

// Other non-return statements in the block are not annotated by
// the outer expression's annotation
let b = ((x => {
  { 
    const x = (y) => 3;  // error y missing an annotation
    return y => 3;
  };
}) as (x: number) => (y: number) => number);
