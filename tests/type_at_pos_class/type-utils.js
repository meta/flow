// @flow

declare class C<P> {};

declare const typeof_C: typeof C;
//            ^
declare const class_of_C_number: Class<C<number>>;
//            ^

declare class A {};

declare const class_a: Class<A>;
//            ^
declare const class_class_a: Class<Class<A>>;
//            ^
declare const class_class_class_a: Class<Class<Class<A>>>;
//            ^
