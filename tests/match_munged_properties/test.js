// Munged inherited members stay private while public fields remain readable.
class Base {
  _private: string;
  publicValue: number;
}
class Derived extends Base {}
declare const value: Derived;
declare const maybeValue: ?Derived;

// Ordinary access rejects inherited munged properties.
value._private; // ERROR: inherited munged property

// Nested patterns reject inherited munged properties on class inputs.
const privateProperty = match ([value]) {
  [{_private: const privateValue, ...}] => privateValue, // ERROR: inherited munged property
};

// Nullable nested inputs keep inherited munged properties inaccessible.
const maybePrivateProperty = match ([maybeValue]) {
  [{_private: const privateValue, ...}] => privateValue, // ERROR: inherited munged property
  _ => null, // OK
};

// Public inherited fields retain their declared type in nullable nested patterns.
match ([maybeValue]) {
  [{const publicValue, ...}] => { // OK
    publicValue as number; // OK
    publicValue as string; // ERROR: a public inherited property remains readable
  },
  _ => {}, // OK
}
