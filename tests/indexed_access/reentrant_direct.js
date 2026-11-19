type Args = [C['x'], null]; // OK

class B<T extends ReadonlyArray<unknown>> {
  x: number = 0;
}

class C extends B<Args> {}

declare const a: Args;
a[0];
