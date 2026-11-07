// Decorated class expressions remain gated by the decorators parse option.
const Expression = @decorator class {}; // ERROR
