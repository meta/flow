declare function compute(): string;

class AssignedOnlyInLoop {
  last: number; // error
  constructor(items: Array<number>) {
    for (const item of items) {
      this.last = item;
    }
  }
}

class DoWhileLoop {
  value: number;
  constructor(cond: boolean) {
    do {
      this.value = 1;
    } while (cond);
  }
}

class SwitchWithDefault {
  value: string;
  constructor(kind: string) {
    switch (kind) {
      case 'a':
        this.value = 'A';
        break;
      default:
        this.value = 'other';
    }
  }
}

class SwitchWithoutDefault {
  value: string; // error
  constructor(kind: string) {
    switch (kind) {
      case 'a':
        this.value = 'A';
        break;
    }
  }
}

class TryCatch {
  value: string;
  constructor() {
    try {
      this.value = compute();
    } catch (e) {
      this.value = 'fallback';
    }
  }
}

class Outer {
  value: string;
  constructor() {
    class Inner {
      value: string; // error
    }
    this.value = 'hi';
  }
}

class ReturnThroughEmptyFinally {
  value: string; // error
  constructor(ready: boolean) {
    try {
      if (!ready) return;
    } finally {}
    this.value = 'hi';
  }
}

class ReturnThroughAssigningFinally {
  value: string;
  constructor(ready: boolean) {
    try {
      if (!ready) return;
    } finally {
      this.value = 'hi';
    }
    this.value = 'bye';
  }
}
