import {M1, C1, S1} from './declare_class_method_no_return';

declare const m: M1;
m.foo(1) as string; // OK — foo returns implicit any

new C1() as empty; // ERROR — C1 ~> empty

declare const s: S1;
s.foo = 1; // OK — setter parameter is number
s.foo = 'hello'; // ERROR — string ~> number setter parameter
