type Obj = {
  foo?: {
    bar: number,
  },
};


1 as Obj['foo']?.['xxx']; // Error - wrong prop

type X = Obj['foo']['bar']; // Error - access `bar` on undefined
1 as X;

type T = Obj['foo']?.['bar'];
1 as T; // OK
true as T; // Error - wrong type

type Z = {c: number};
type Y = {a: ?{b: Z, opt_b: ?Z}};

1 as Y["a"]["b"]?.["c"]; // Error - access `b` on undefined
1 as Y["a"]?.["opt_b"]["c"]; // Error - access `c` on undefined
1 as (Y["a"]?.["b"])["c"]; // Error - access `c` on undefined
1 as Y["a"]?.["xxx"]; // Error - non-existent prop
true as Z?.['c']; // Error - wrong type

1 as Y["a"]?.["b"]["c"]; // OK
undefined as Y["a"]?.["b"]["c"]; // OK
true as Y["a"]?.["b"]["c"]; // Error - wrong type

1 as Y["a"]?.["b"]?.["c"]; // OK
undefined as Y["a"]?.["b"]?.["c"]; // OK
true as Y["a"]?.["b"]?.["c"]; // Error - wrong type

1 as Y["a"]?.["opt_b"]?.["c"]; // OK
undefined as Y["a"]?.["opt_b"]?.["c"]; // OK
true as Y["a"]?.["opt_b"]?.["c"]; // Error - wrong type

1 as Z?.['c']; // OK
true as Z?.['c']; // Error - wrong type
undefined as Z?.['c']; // OK - this behaviour does not match optional chaining at the value level, if in the future this changes to be an error that would be fine

type U = {a: void | {b: Z, opt_b: null | Z}};

1 as U["a"]?.["b"]["c"]; // OK
undefined as U["a"]?.["b"]["c"]; // OK
true as U["a"]?.["b"]["c"]; // Error - wrong type

1 as U["a"]?.["b"]?.["c"]; // OK
undefined as U["a"]?.["b"]?.["c"]; // OK
true as U["a"]?.["b"]?.["c"]; // Error - wrong type

1 as U["a"]?.["opt_b"]?.["c"]; // OK
undefined as U["a"]?.["opt_b"]?.["c"]; // OK
true as U["a"]?.["opt_b"]?.["c"]; // Error - wrong type

export type MaybeZ = ?Z;
1 as MaybeZ?.['c']; // OK
undefined as MaybeZ?.['c']; // OK
true as MaybeZ?.['c']; // Error - wrong type

type VoidZ = void | Z;
1 as VoidZ?.['c']; // OK
undefined as VoidZ?.['c']; // OK
true as VoidZ?.['c']; // Error - wrong type

undefined as null?.['c']; // OK
undefined as void?.['c']; // OK
1 as void?.['c']; // Error

undefined as empty?.['x']; // OK
1 as empty?.['x']; // Error

function f<K extends string, T extends ?{readonly [key: K]: unknown}>(t: T, k: K): T?.[K] {
  return t?.[k];
}

declare const z: Z;
f(z, 'c') as number | void; // OK
f(z, 'c') as true; // Error - wrong type
f(z, 'xxx'); // Error - non-existent prop

declare const mz: MaybeZ;
f(mz, 'c') as number | void; // OK
f(mz, 'c') as true; // Error - wrong type
f(mz, 'xxx'); // Error - non-existent prop

type Before = Later?.['a'];
's' as Before; // Error
0 as Before; // OK
undefined as Before; // OK
type Later = ?{a: number};

type O = ?{
  [key: number]: boolean;
  foo: string;
  bar: number;
  nested: {
    goop: number,
  },
}
1 as O?.['bar']; // OK
true as O?.[number]; // OK
'xx' as O?.['bar']; // Error
1 as O?.['nested']['goop']; // OK

declare class Cx {
  [key: number]: boolean;
  foo: string;
  bar: number;
  nested: {
    goop: number,
  },
}
type C = ?Cx;
1 as C?.['bar']; // OK
true as C?.[number]; // OK
'xx' as C?.['bar']; // Error
1 as C?.['nested']['goop']; // OK

type NonMaybeNumber = NonNullable<Obj['foo']?.['bar']>;
1 as NonMaybeNumber; // OK
true as NonMaybeNumber; // Error
undefined as NonMaybeNumber; // Error
