// Optional member access and calls require the parenthesized escape hatch.
@decorators?.member class Member {} // ERROR
@decorator?.() class Call {} // ERROR
