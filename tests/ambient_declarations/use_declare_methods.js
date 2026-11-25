import {
  MyClass,
  WithRestParam,
  WithDestructuringParams,
} from './declare_methods';
import {WithConstructor, WithOverloadedConstructor} from './declare_constructors';

const instance = new MyClass();
instance.method(42);
MyClass.staticMethod();

// Type errors
instance.method("not a number"); // ERROR: string ~> number
const badStatic: number = MyClass.staticMethod(); // ERROR: string ~> number

// Overloaded methods form an intersection across all overloads.
const strResult: string = instance.overloaded("hello");
const numResult: number = instance.overloaded(42);

// Generics
const genResult: number = instance.generic<number>(42);
const badGeneric: string = instance.generic<number>(42); // ERROR: number ~> string

// Declared constructor replaces default zero-arg constructor
const withCtor = new WithConstructor("hello");
const badCtor = new WithConstructor(); // ERROR: missing argument

// Overloaded constructors form an intersection across all overloads.
const overCtorStr = new WithOverloadedConstructor("hello");
const overCtorNum = new WithOverloadedConstructor(42);
const badOverCtor = new WithOverloadedConstructor(); // ERROR: missing argument (both overloads require one)

// Rest params on declared methods
const restInstance = new WithRestParam();
restInstance.method(1, 2, 3);
restInstance.method("not a number"); // ERROR: string ~> number

// Destructuring params and optional destructuring
const destInstance = new WithDestructuringParams();
destInstance.objectParam({a: 1});
destInstance.arrayParam([1]);
destInstance.optionalObjectParam(); // OK: optional
destInstance.optionalArrayParam(); // OK: optional

// `new` of a declared constructor gives the instance
withCtor as empty; // ERROR: WithConstructor ~> empty
overCtorNum as empty; // ERROR: WithOverloadedConstructor ~> empty
