// The satisfies target supplies parameter hints without replacing the result type.
{
  const handler = ((text) => text.toUpperCase()) satisfies (text: string) => string;
  handler('hello') as string; // OK
  handler(123); // ERROR
}

// Hints also apply to ordinary functions, destructuring, and rest parameters.
{
  const handler = (function (text) {
    return text.toUpperCase();
  }) satisfies (text: string) => string;
  handler('hello') as string; // OK
}
{
  const handler = (({text}) => text.toUpperCase()) satisfies (
    input: {readonly text: string},
  ) => string;
  handler({text: 'hello'}) as string; // OK
}
{
  const handler = ((...texts) => texts.join(' ')) satisfies (
    ...texts: Array<string>
  ) => string;
  handler('hello', 'world') as string; // OK
  handler(123); // ERROR
}

// Existing decomposition propagates hints through objects, arrays, and conditionals.
{
  const handlers = {
    upper: (text) => text.toUpperCase(),
  } satisfies {readonly [string]: (text: string) => string};
  handlers.upper('hello') as string; // OK
  handlers.missing; // ERROR: the indexer target does not add unknown keys
}
{
  const handlers = [(text) => text.toUpperCase()] satisfies Array<
    (text: string) => string,
  >;
  handlers[0]('hello') as string; // OK
}
{
  declare const flag: boolean;
  const handler = (
    flag ? (text) => text.toUpperCase() : (text) => text.toLowerCase()
  ) satisfies (text: string) => string;
  handler('hello') as string; // OK
}

// The target is available even when the satisfies expression has an outer hint.
{
  const handler: (text: string) => string = ((text) =>
    text.toUpperCase()) satisfies (text: string) => string;
  handler('hello') as string; // OK
}

// Explicit annotations retain their narrower return type and remain checked.
{
  const handler = ((text): 42 => {
    text.toUpperCase();
    return 42;
  }) satisfies (text: string) => number;
  handler('hello') as 42; // OK
  handler(123); // ERROR
}
{
  ((text: number) => text.toString()) satisfies (text: string) => string; // ERROR
}

// The contextual parameter type is checked in the body and in the final contract.
{
  ((text) => text.toFixed()) satisfies (text: string) => string; // ERROR
  ((text) => text.length) satisfies (text: string) => string; // ERROR
}

// Satisfies supplies a return hint to implicit instantiation.
{
  declare function make<T>(): T;
  const value = make() satisfies string;
  value as string; // OK
  value as number; // ERROR
}

// Annotation hints can refer to type parameters in the enclosing scope.
{
  function apply<T>(value: T): T {
    const identity = ((input) => input) satisfies (input: T) => T;
    return identity(value);
  }
  apply('hello') as string; // OK
}

// A non-function target cannot infer an unannotated function parameter.
{
  ((text) => text) satisfies unknown; // ERROR
}

// Contextual typing can retain literals rather than the context-free widened type.
{
  const value = {flag: true} satisfies {readonly flag: boolean};
  value.flag as true; // OK
  value.flag = true; // OK: the target does not make the operand readonly
  value.flag = false; // ERROR
}

// Satisfies and as share contextual inference, but only as replaces the result type.
{
  const satisfied = ((text): 42 => 42) satisfies (text: string) => number;
  const cast = ((text): 42 => 42) as (text: string) => number;
  satisfied('hello') as 42; // OK
  cast('hello') as 42; // ERROR
}
