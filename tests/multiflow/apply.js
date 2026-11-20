function apply<Args extends ReadonlyArray<unknown>, Ret>(
  fn: (...args: Args) => Ret,
  args: Args,
): Ret {
  return fn(...args);
}

function noRest1(x: 'hi', y: 123): true { return true; }
apply(noRest1, ['hi', 123]); // No error
apply(noRest1, ['hi', 456]); // Error - 456 ~> 123
apply(noRest1, ['hi']); // Error - too few args
apply(noRest1, ['hi', 123, false]); // No error - too many args is fine

// withRest behaves the same as noRest
function withRest1(...rest: ['hi', 123]): true { return true; }
apply(withRest1, ['hi', 123]); // No error
apply(withRest1, ['hi', 456]); // Error - 456 ~> 123
apply(withRest1, ['hi']); // Error - too few args
apply(withRest1, ['hi', 123, false]); // No error - too many args is fine

// Same thing, but with types instead of functions
declare const applyType: <Args extends ReadonlyArray<unknown>, Ret>(
  fn: (...args: Args) => Ret,
  args: Args,
) => Ret;

function noRest2(x: 'hi', y: 123): true { return true; }
applyType(noRest2, ['hi', 123]); // No error
applyType(noRest2, ['hi', 456]); // Error - 456 ~> 123
applyType(noRest2, ['hi']); // Error - too few args
applyType(noRest2, ['hi', 123, false]); // No error - too many args is fine

// withRest behaves the same as noRest
function withRest2(...rest: ['hi', 123]): true { return true; }
applyType(withRest2, ['hi', 123]); // No error
applyType(withRest2, ['hi', 456]); // Error - 456 ~> 123
applyType(withRest2, ['hi']); // Error - too few args
applyType(withRest2, ['hi', 123, false]); // No error - too many args is fine
