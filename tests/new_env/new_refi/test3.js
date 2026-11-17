const x: (val: ?number) => (() => number) = val => {
  if (val == null) {
    return () => 42;
  }
  return () => val; // OK, since val cannot be null
}

function param_annot(x: ?string): ?(() => string) {
  if (x != null) {
    return () => x;
  }
}

function rest(...x: ?Array<unknown>): ?(() => Array<unknown>) {
  if (x != null) {
    return () => x;
  }
}
