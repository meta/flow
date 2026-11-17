declare function f<in A>(x: A): void; // ERROR - in / out not allowed in function parameters
declare function f<out A>(): A; // ERROR - in / out not allowed in function parameters
declare function f<in out A>(x: A): A; // ERROR

declare function f<in>(x: in): void; // OK
declare function f<out>(x: out): void; // OK
const out = 1; // OK

type Contravariant<in A> = (x: A) => void; // OK
type Covariant<out A> = {readonly prop: A}; // OK
interface ContravariantInterface<in A> { method(x: A): void } // OK
interface CovariantInterface<out A> { readonly prop: A } // OK
type ContravariantWriteonly<in A> = { writeonly prop: A }; // OK

type BadOut<out A> = (x: A) => void; // ERROR: out in contravariant position
type BadIn<in A> = {readonly prop: A}; // ERROR: in in covariant position
interface BadOutInterface<out A> { method(x: A): void } // ERROR: out in contravariant position
interface BadInInterface<in A> { readonly prop: A } // ERROR: in in covariant position
type BadInWriteonly<out A> = { writeonly prop: A }; // ERROR: out in contravariant position

type InOut<in out A> = A; // ERROR: in out is always unsupported
interface InOutInterface<in out A> { prop: A } // ERROR: in out is always unsupported

// Function type annotations (distinct from type aliases evaluating to function types)
type FnIn = <in T>(x: T) => void; // ERROR - in / out not allowed on function type parameters
type FnOut = <out T>() => T; // ERROR - in / out not allowed on function type parameters

// Method type parameters in classes
class MethodHost {
  contraMethod<in T>(x: T): void {} // ERROR - in / out not allowed on method type parameters
  coMethod<out T>(): T { throw 0; } // ERROR - in / out not allowed on method type parameters
}

// Method type parameters in interfaces
interface MethodInterface {
  contraMethod<in T>(x: T): void; // ERROR - in / out not allowed on method type parameters
  coMethod<out T>(): T; // ERROR - in / out not allowed on method type parameters
}

// Same restriction applies to the +/- variance spellings (no flag required)
type FnPlus = <out T>(x: T) => void; // ERROR - variance modifiers not allowed on function type parameters
type FnMinus = <in T>(x: T) => void; // ERROR - variance modifiers not allowed on function type parameters

class MethodHostShortForm {
  contraMethod<in T>(x: T): void {} // ERROR - variance modifiers not allowed on method type parameters
  coMethod<out T>(): T { throw 0; } // ERROR - variance modifiers not allowed on method type parameters
}

interface MethodInterfaceShortForm {
  contraMethod<in T>(x: T): void; // ERROR - variance modifiers not allowed on method type parameters
  coMethod<out T>(): T; // ERROR - variance modifiers not allowed on method type parameters
}
