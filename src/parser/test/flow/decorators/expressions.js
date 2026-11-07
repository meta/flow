// Bare decorators allow identifier references, dotted members, and one final call.
@decorator // OK
@decorators.member.default // OK
@decorators.member(value, ...rest,) // OK
class Declaration {}

// Decorated class expressions use the same restricted expression grammar.
const Expression = @decorators.member(value) class {}; // OK
const Instance = new (@decorator(make()) class {})(); // OK

// Arguments and parenthesized expressions allow `in` even in a for initializer.
for (class { @decorator(key in object) method() {} };;) {} // OK
for (class { @(key in object) method() {} };;) {} // OK
