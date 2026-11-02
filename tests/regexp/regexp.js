var patt=/Hello/g
var match:number = patt.test("Hello world!");

declare const regExp: RegExp;
regExp[Symbol.matchAll] as (str: string) => Iterator<RegExp$matchResult>; // error: the method's `this` is `RegExp`
regExp[Symbol.match] as (str: string) => Iterator<RegExp$matchResult>; // error: the method's `this` is `RegExp` and it returns `RegExpMatchArray | null`

var escaped: string = RegExp.escape("hello[world]");

// `RegExpStringIterator<T>` is declared without variance, so `T` is invariant.
const matches = "abc".matchAll(/b/g);
matches as Iterator<RegExpExecArray | null>; // ok: `Iterator`'s element type is covariant
matches as RegExpStringIterator<RegExpExecArray | null>; // error: invariant
matches as RegExpStringIterator<Array<string>>; // error: invariant, so `Array<string>` needs `index` and `input`

// A string pattern, accepted at runtime, and a `string | RegExp` union fall back to Flow's signature.
"abc".matchAll("b") as RegExpStringIterator<RegExpExecArray>; // ok
declare const pattern: string | RegExp;
"abc".matchAll(pattern) as RegExpStringIterator<RegExpExecArray>; // ok
"abc".matchAll(1); // error

// The locale methods also take `Intl.LocalesArgument`.
"a".localeCompare("b", new Intl.Locale("en"), {sensitivity: "base"}) as number; // ok
declare const locales: ReadonlyArray<string>;
"a".toLocaleUpperCase(locales) as string; // ok
"a".toLocaleLowerCase(1); // error
