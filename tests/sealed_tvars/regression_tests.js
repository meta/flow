function no_missing_env_entry_for_delete() {
  declare const foo: {bar: string | void};
  delete foo.bar; // ok
  foo.bar as void; // ok
}

function no_missing_env_entry_for_illegal_type_binding() {
  type A = number;
  type A = number; // error, but no MissingEnvEntry internal error.
}

function unresolved_class_self_tvar() {
  class C { } // no weird unresolved tvar error
}

exports.a = 1; // Read of exports points to a fully resolved exports type.

class MissingAnnotations {
  constructor () {
    return 42;
  }
  a;
  b: number = 42;
  c = 42;
  d = 42 + 42;
  e = (x: number): number => x;
  f = (x: number) => { }
  g = (x: number) => { return 42 }
  h() {}
  i(): number { return 42 }
  j() { return 42 }
  k = function (x: number) { }
  l = null; // error
}

class MoreMA {
  constructor(): void { }
}

class MoreMA2 {
  constructor() { }
}

function UnannotParams(x, {a, b}, ...y) { }

UnannotParams((x) => x);


type ModScope = string
declare module 'declared-module' {
  declare type ModScope = ModScopeExternal
}
type ModScopeExternal = number
42 as any as ModScope as empty; // string ~> empty, not num ~> empty

declare module DeclaredModule2 {
  declare type ModScope2 = number
  declare type DependsOnModScope2 = ModScope2
}

declare const NonexistentType: ModScope2;

declare const AString: string;
const ResultOfError = -AString;

let UninitializedVar;

class ClassMethodThisAnnotations {
  method1(this: unknown) {
    this as unknown;
  }
}

{
  let inexact = {foo: 3} as {foo: number, ...};

  declare function inexactSpread<T>(x: T): {bar: 3, ...T, ...};
  const inexact_spread = inexactSpread(inexact); // OK
}

function TestArrayProvider() {
  var x = [];
  x[0] = 10;
}

{
  class MyClass<T> {
  }

  declare const c: MyClass;
}

interface InterfaceWithMethodThisAnnotation {
  test(this: unknown): void // ok
}

function anonymous_functions_this_type() {
  let foo1 = function () {}; // ok
  let foo2 = function (this: unknown) {}; // ok
  let foo3 = function () { this; }; // error: missing annot on this
  let foo4 = function (this: unknown) { this; }; // ok
}

function non_assigning_member_assigns() {
  declare const foo: {[key: string | number]: unknown};
  foo[1 + 2] = 1; // ok
  foo['1' + 2] = 3; // ok
  (1).toString = 3; // error: toString missing in number, but no missing env entry error
  declare const bar: {baz: number};
  bar.baz += 1; // ok
  bar.baz++; // ok
  bar.baz as number; // ok
  bar.baz--; // ok
  bar.baz as number; // ok
}

{
  var x: number = (x) => x;
}

{
  for (const x of x) {} // Error on second x, no underconstrained error on the first one.
}

{
  const S = ()=> {
    const x = useStyle();
  };

  type Styles = 1

  const useStyle = (): Styles => { return 1; }
}

{
  declare const rule: {title_label: string};
  const titlesAdlabels = [];
  rule.title_label = "a";
  titlesAdlabels.push(rule.title_label);
}

{
  declare const values: {
    greeting: string,
  };

  if (values.greeting === values) {  // constant-condition error

  }
}

{
  class A {
    B(defaultValue: boolean = (x: A) => 42): void {}
  }
}

{
  class C {
   w({ foo = C, bar}: { bar: number, foo?: number }): void { }
  }
}

{
  declare function poly<T>(f: (x: T)=>T, val: T): T;
  () => {
    poly(v => v, [foo1, foo2.bar, (v: number) => v, {foo: 3, bar: foo2.bar}, foo1 = '', 1 + 2]); // ok
    poly(v => v, foo3()); // ok
  };
  declare let foo1: string;
  declare const foo2: {bar: string};
  declare const foo3: () => string;
}

{
  const tuple = [0];
  // $FlowExpectedError[unsupported-syntax]
  [tuple[0]] = [1]; // no additional internal error
}

{
  type S = <W>(w: W) => W;

  let s: ?S = null;
  if (s != null) {
      const h = s;
      h(42);
  }
}

{
  declare const array_non_maybe_any: Array<NonNullable<any>>;
  const non_maybe_any = array_non_maybe_any[0];
  type Non_maybe_any = typeof non_maybe_any;
  1 as Non_maybe_any; // okay 1 ~> any
}
