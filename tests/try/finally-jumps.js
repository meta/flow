function withoutFinally(skip: boolean): number {
  let value: number;
  L: {
    if (skip) break L;
    value = 1;
  }
  return value; // error
}

function withFinally(skip: boolean): number {
  let value: number;
  L: try {
    if (skip) break L;
    value = 1;
  } finally {}
  return value; // error
}

function nullCheckSkipped(x: ?string): number {
  L: try {
    if (x == null) break L;
  } finally {}
  return x.length; // error
}

function continueThroughFinally(skip: boolean): number {
  let value: number;
  do {
    try {
      if (skip) continue;
    } finally {}
    value = 1;
  } while (false);
  return value; // error
}

function finallyAssigns(skip: boolean): number {
  let value: number;
  L: try {
    if (skip) break L;
  } finally {
    value = 1;
  }
  return value;
}

function finallyReturnsOnNullPath(x: ?string): number {
  L: try {
    if (x == null) break L;
  } finally {
    if (x == null) return 0;
  }
  return x.length;
}

function throwThroughFinally(): void {
  let value: ?string;
  value = 'a';
  try {
    try {
      value = null;
      throw new Error();
    } finally {}
  } catch {
    value.length; // error
  }
}
