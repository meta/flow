declare const maybeNotify: ?(value: number) => void;
declare const maybePair: ?(first: number, second: number) => void;
declare const maybeObject: ?{run(value: number): void};
declare const maybeMap: ?{[key: string]: number};
declare const maybeDeletableMap: ?{[key: string]: ?number};
declare const maybeFactory: ?() => {[key: string]: number};
declare const maybeNested: ?{inner: ?{[key: string]: number}};
declare const maybeDouble: ?(value: number) => number;
declare const maybeChain: ?{first(value: number): {second(value: number): void}};

class AssignedInOptionalCallArgument {
  value: number; // error
  constructor() {
    maybeNotify?.((this.value = 1));
  }
}

class ReadAfterOptionalCallArgument {
  value: number; // error
  constructor() {
    maybeNotify?.((this.value = 1));
    this.value; // error: unassigned when `maybeNotify` is null or undefined
  }
}

class AssignedInOptionalMethodArgument {
  value: number; // error
  constructor() {
    maybeObject?.run((this.value = 1));
  }
}

class AssignedInOptionalKey {
  value: number; // error
  constructor() {
    maybeMap?.[((this.value = 1), 'key')];
  }
}

class AssignedInDeletedOptionalKey {
  value: number; // error
  constructor() {
    delete maybeDeletableMap?.[((this.value = 1), 'key')];
  }
}

class AssignedInOptionalCallOfOwnField {
  value: number; // error
  onReady: ?(value: number) => void = null;
  constructor() {
    this.onReady?.((this.value = 1));
  }
}

class AssignedInLaterKeyOfChain {
  value: number; // error
  constructor() {
    maybeFactory?.()[((this.value = 1), 'key')];
  }
}

class AssignedAfterLaterOptionalLink {
  value: number; // error
  constructor() {
    maybeNested?.inner?.[((this.value = 1), 'key')];
  }
}

class ReadInNestedOptionalChain {
  value: number; // error
  constructor() {
    maybePair?.(maybeDouble?.((this.value = 1)) ?? 0, this.value); // error: the inner call may be skipped
  }
}

class AssignedBeforeQuestionDot {
  value: number; // ok: what comes before `?.` always runs
  constructor() {
    ((this.value = 1), maybePair)?.(this.value, this.value);
  }
}

class ReadLaterInTheSameChain {
  value: number; // error
  constructor() {
    maybeChain?.first((this.value = 1)).second(this.value); // ok: `second` runs only after `first`
  }
}
