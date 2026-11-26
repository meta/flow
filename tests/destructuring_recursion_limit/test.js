declare const B: Class<C>;
class C extends B {}

// The statics chain cycles: the lookup ends where it does, without an answer.
C.missing;
const {missing} = C;
