{
  class Done {
    done: true = true;
    a: number = 0;
  }
  class NotDone {
    done: false = false;
    b: string = '';
  }
  declare const x: Done | NotDone;
  if (x.done) {
    x as empty; // ERROR: only Done
    x as Done; // OK
  } else {
    x as empty; // ERROR: only NotDone
    x as NotDone; // OK
  }
}

{
  // TypeScript's IteratorResult
  interface IteratorYieldResult<T> {
    done?: false;
    value: T;
  }
  interface IteratorReturnResult<R> {
    done: true;
    value: R;
  }
  declare const r: IteratorYieldResult<number> | IteratorReturnResult<string>;
  if (r.done) {
    r.value as string; // OK
  } else {
    r.value as number; // OK
  }
  if (!r.done) {
    r.value as number; // OK
  }
}

{
  interface NullP {
    p: null;
    a: number;
  }
  interface StrP {
    p: 'b';
    b: string;
  }
  interface UndefinedP {
    p: undefined;
    c: boolean;
  }
  declare const x: NullP | StrP | UndefinedP;
  if (x.p != null) {
    x as StrP; // OK
  } else {
    x as NullP | UndefinedP; // OK
  }
}

{
  class Maybe {
    done: true = true;
  }
  declare const x: ?Maybe;
  if (!x?.done) {
    x as null | void; // OK
  }
}

{
  // Non-readable
  interface WriteOnly {
    writeonly done: true;
    a: number;
  }
  interface ReadOnly {
    done: false;
    b: string;
  }
  declare const x: WriteOnly | ReadOnly;
  if (x.done) { // ERROR: prop not readable
    x as ReadOnly; // ERROR
  }
}

{
  // A standalone instance is not refined, e.g. a lazily initialized field
  class Lazy {
    ready: true;
    m() {
      if (!this.ready) {
        this as empty; // ERROR
      }
    }
  }
}

{
  // Methods do not refine, e.g. feature detection
  class WithMethod {
    m(): void {}
  }
  class WithoutMethod {
    m: void;
  }
  declare const x: WithMethod | WithoutMethod;
  if (!x.m) {
    x as WithoutMethod; // ERROR
  }
}

{
  // Inherited fields do not refine
  class Parent {
    done: true = true;
  }
  class Child extends Parent {}
  class Other {
    done: false = false;
  }
  declare const x: Child | Other;
  if (!x.done) {
    x as Other; // ERROR
  }
}

{
  // Fields with non-literal types do not refine
  class ObjP {
    p: {...} = {};
  }
  class FalseP {
    p: false = false;
  }
  declare const x: ObjP | FalseP;
  if (!x.p) {
    x as FalseP; // ERROR
  }
}
