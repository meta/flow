declare const B: Class<C>;
class C extends B {}

C.missing; // ERROR
const {missing} = C; // ERROR
