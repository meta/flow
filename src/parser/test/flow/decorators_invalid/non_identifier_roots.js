// Bare decorator roots must be IdentifierReferences, not keywords or literals.
@this.decorator class This {} // ERROR
@new Decorator() class Constructed {} // ERROR
@42 class Number {} // ERROR
@"decorator" class String {} // ERROR
@true class Boolean {} // ERROR
@null class Null {} // ERROR
@[decorator] class Array {} // ERROR
@{decorator} class Object {} // ERROR
