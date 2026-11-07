// The parenthesized escape hatch ends at its closing parenthesis.
@(decorator).member class Member {} // ERROR
@(decorator)() class Call {} // ERROR
