class A<T> {
  p: T;
  constructor(p: T) {
    this.p = p;
  }
}

// Test out simple defaults
class B<T = string> extends A<T> {}

var b_number: B<number> = new B(123);
var b_undefined: B<undefined> = new B();
var b_default: B<> = new B('hello');

b_number.p as boolean; // Error number ~> boolean
b_undefined.p as boolean; // Error undefined ~> boolean
b_default.p as boolean; // Error string ~> boolean

class C<T extends ?string = string> extends A<T> {}

var c_number: C<number> = new C(123); // Error number ~> ?string
var c_undefined: C<undefined> = new C();
var c_default: C<> = new C('hello');

c_undefined.p as boolean; // Error undefined ~> boolean
c_default.p as boolean; // Error string ~> boolean

class D<S, T = string> extends A<T> {}
var d_number: D<unknown, number> = new D(123);
var d_undefined: D<unknown, undefined> = new D();
var d_default: D<unknown> = new D('hello');
var d_too_few_args: D<> = new D('hello'); // Error too few tparams
var d_too_many: D<unknown, string, string> = new D('hello'); // Error too many tparams

d_number.p as boolean; // Error number ~> boolean
d_undefined.p as boolean; // Error undefined ~> boolean
d_default.p as boolean; // Error string ~> boolean

class E<S extends string, T extends number = S> {} // Error: string ~> number
class F<S extends string, T extends S = number> {} // Error: number ~> string

class G<S extends string, T = S> extends A<T> {}

var g_default: G<string> = new G('hello');

g_default.p as boolean; // Error string ~> boolean

class H<S = T, T = string> {} // Error - can't refer to T before it's defined
