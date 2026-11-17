//@flow

// If the entire expression has an annot available then we should be able to push
// the annotation of the return type down to the return expression, even
// through try..catch..finally statements 
let a = ((x => {
  try { 
    const z = (x) => 3; // Non-return statements are not covered by the annot
    return y => 3;
  } catch (err) {
    const z = (x) => 3; // Non-return statements are not covered by the annot
    return y => 3;
  } finally {
    const z = (x) => 3; // Non-return statements are not covered by the annot
    return y => 3;
  };
}) as (x: number) => (y: number) => number);
