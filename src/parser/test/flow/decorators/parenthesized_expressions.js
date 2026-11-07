// Parentheses allow arbitrary expressions, including computed access and call chains.
@(decorators[key]) // OK
@(decorators().member()()) // OK
@(decorators?.member) // OK
@(new Decorator()) // OK
@(this.decorator) // OK
@(decorator`value`) // OK
@(flag ? first : second) // OK
@(first, second) // OK
@((value) => value) // OK
@(decorator<T>()) // OK
class Parenthesized {}

// Parenthesized expressions retain the enclosing await/yield context.
async function asyncContext() {
  @(await decorator()) class Async {} // OK
}
function* generatorContext() {
  @(yield decorator()) class Generator {} // OK
}
