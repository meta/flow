//@flow
declare class A<T>{
  constructor(): A<T>
}
new A(); // Error: T underconstrained

declare class B<out T>{
  constructor(f: (x: T) => unknown): B<T>;
}
new B((x: number) => 'string'); // Ok: It will error under Pierce, but we also consider upper bounds here.

declare class C<in T>{
  constructor(x: T): C<T>;
}

new C(3); // Ok: It will error under Pierce, but we also consider lower bounds here.

new Array(1); // Error

import {PolyClass1, PolyClass2} from './exported_poly_class';
new PolyClass1(); // Error
new PolyClass2(); // Error
