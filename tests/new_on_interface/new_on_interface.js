// Basic construct signature
interface Foo {
    new (): Foo;
    bar(): 3;
}
declare const F: Foo;
const v = new F().bar();
v as 3;

// Construct signature with arguments
interface Bar {
    new (x: number): Bar;
}
declare const B: Bar;
new B(42);
new B("hello"); // ERROR

// Construct signature with no return annotation
interface Baz {
    new (x: string); // ERROR: return annotation required, but works
}
declare const Z: Baz;
const z = new Z("ok");
z as Baz; // ok
new Z(42); // ERROR

// Non-constructable interface should still error
interface NotConstructable {
    bar(): number;
}
declare const nc: NotConstructable;
new nc(); // ERROR

// Generic construct signature
interface Gen {
    new <T, U>(x: T): Gen;
}
declare const G: Gen;
new G<string>("a"); // ERROR: fewer than 2 type arguments
