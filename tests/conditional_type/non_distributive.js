function avoid_unnecessary_type_app_loop_regression_test() {
  type Obj = {readonly [key: string]: unknown};
  opaque type Opaque<out _V> = string;

  type Mapped<out O extends Obj> = {
    readonly [Key in keyof O]: O[Key] extends Obj
      ? Opaque<O[Key]>
      : Opaque<O[Key]>,
  };

  declare const o: {readonly foo: '1'};
  o as Mapped<{ foo?: unknown }>; // ok
  o as Mapped<{[K in 'foo']?: unknown}>; // ok
}

function no_longer_always_distribute() {
  type Mapped<out O extends {readonly foo: unknown, ...}> = O['foo'] extends string
    ? string
    : number

  declare const v: Mapped<{foo: string | number}>;
  v as number; // ok
  v as string; // error: number ~> string
}
