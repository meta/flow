### 0.339.0

Breaking Changes:
* Rename `typeParameters` to `typeArguments` on ESTree `GenericTypeAnnotation`, `InterfaceExtends`, and `ClassImplements`, including WASM `flow-parser` output. Declaration parameters and Babel output retain their existing names.
* `PropertyDescriptor` and `Object.getOwnPropertyDescriptor` are no longer generic. Remove type arguments from `PropertyDescriptor<T>` annotations and `Object.getOwnPropertyDescriptor<T>(...)` calls.

Likely to cause new Flow errors:
* Hook calls in classes now use `react-rule-hook-class`, and calls in deeper nested anonymous callbacks inside components or hooks use `react-rule-hook-conditional`, instead of `react-rule-hook`. Update suppressions to the new codes. Hook calls directly in class property initializers are also rejected when the enclosing function would otherwise permit them.
* Assignments and throws in function or component parameter defaults and destructuring defaults are now treated as conditional, because a default only runs when the value it replaces is `undefined`.
* Property increments and decrements (`o.x++`, `o.x--`, `++o.x`, and `--o.x`) now invalidate narrowings of the property through aliases, as assignments already do.
* Reads in an assertion call's condition now use the types at the point where they run, rather than types after the call and its later arguments. This removes false errors and reports reads before a later argument assigns a variable.
* `break`, `continue`, `return`, and `throw` paths through `finally` are no longer dropped. Flow now reports variables left unassigned and narrowings that do not hold on these paths.
* Optional class fields (`x?: T`, with `experimental.tslib_syntax`) now have type `T | void`. Reads may return `undefined`, and assigning `undefined` is allowed.
* Type parameters explicitly declared `in out` in TypeScript files are now invariant rather than covariant.
* Functions named `use*` declared in TypeScript files are now treated as hooks. This fixes calls from Flow hook syntax, but calls outside a component or hook can now report Rules of Hooks errors.
* `parseInt`, `parseFloat`, and `eval` now require a `string`; `isNaN` and `isFinite` require a `number`; and builtin Error constructors require a `string` message, matching TypeScript.
* `Object.getOwnPropertyDescriptor`, `Object.defineProperty`, `Object.prototype.hasOwnProperty`, and `Object.prototype.propertyIsEnumerable` now require `PropertyKey` (`string | number | symbol`) keys. `PropertyDescriptorMap` is also keyed by `PropertyKey`.
* `DataView.byteLength` and `byteOffset`, numeric typed array `byteLength`, `byteOffset`, and `length`, and `RegExp.source`, `global`, `ignoreCase`, and `multiline` are now read-only. `RegExp.prototype.compile` now requires a pattern.
* `Array.prototype.splice` uses separate removal and insertion overloads, so inserting items requires a `deleteCount`.
* `Array` and `ReadonlyArray` `reduce` and `reduceRight` now have an overload whose initial value and accumulator use the element type. Calls intending a different accumulator type may need an explicit type argument, e.g. `reduce<number>(...)`.
* `Promise.prototype.then` and `catch` now use TypeScript-style signatures. `then` has separate fulfillment and rejection type parameters; calls whose inferred result is unsuitable may need explicit type arguments.
* `RegExpMatchArray.index` and `input` are now optional, and its first element is declared as `0: string`. Check optional fields on `match` results, and use `RegExpExecArray` for `exec` and `matchAll` results.
* `Error.stack` is now optional, matching TypeScript.
* Numeric typed array `subarray` methods, including `Uint8Array.prototype.subarray`, return their corresponding typed array rather than `this`. Subclasses that return their own type should declare their own override.

New Features:
* Format code with Prettier in the Try Flow playground, and copy JSON from its AST view.
* Object type spreads allow optional properties after an object with an indexer. Value spreads retain their existing checks.
* Add `translateFlowTypeToTSType` to `flow-api-translator` for type-node translation.
* Functions with any return type are accepted where a TypeScript-declared function type returning `void` is expected, matching TypeScript. Flow-declared function types retain their existing return checks.

Notable bug fixes:
* Fixed Windows commands such as `flow check` hanging when their output is piped or captured, even after the command exits ([#9519](https://github.com/meta/flow/issues/9519)).
* Preserve imported TypeScript template literal types without requiring `experimental.tslib_syntax`.
* Type guards refine distinct bigint literal types to `empty`, as they already do for string, number, and boolean literals.
* Fixed an internal error when a function parameter default always throws; the function is now typed and checked.
* JavaScript objects with extra properties can be checked against TypeScript object types whose index signature applies to only some keys, and against interfaces extending an object type with an index signature.
* Accept literal initializers such as `declare const x = -1`.
* Improve implicit instantiation through conditional types without `infer`, conditional types nested inside distributive conditional types, and type applications that expand to types mentioning the inferred parameters.
* Fixed a crash when a mapped type's key has the same name as a type parameter spread in the mapped object type.
* Imports such as `.//foo` and `..//foo` resolve like `./foo` and `../foo`.
* Prevent stack overflows and spurious internal errors when checking recursive tuple spreads in generic type arguments.
* JavaScript subclasses can override static methods of TypeScript classes with narrower parameters, as on the instance side.
* Fixed a crash on a `match` in unreachable code.
* Fixed a crash when a namespace merged with a class or function redeclares one of its members.
* Fixed crashes when a recheck is canceled while checking a component declaration, or when cancellation or a timeout interrupts a type cast from an annotated source.
* Fixed a crash on a method with a `this` parameter in a class expression or a class declared in a callback.
* Fixed a crash when a file imports a name that it also declares as a function, component, or hook.
* Fixed crashes when a library definition stops parsing while the server is running, or contains an ignored statement that reads a name, such as `f();`.
* TypeScript enum checking no longer requires `enums=true`.
* Fixed a crash ("Non speculating: LimitExceeded") when destructuring a value whose type lookup exceeds the recursion limit.
* A type parameter whose bound mentions another type parameter no longer leaks unresolved into the other parameter's solution.
* A field named `constructor`, including TypeScript's `['constructor']` fields, now types `x.constructor` without shadowing the constructor used by `new` and `super()`.
* Named imports work from CommonJS modules whose `module.exports` has multiple call signatures or is an overloaded function merged with a namespace.
* Mapped types with template literal keys now produce individual properties when their placeholders resolve to string literals, instead of retaining a template literal indexer. Generic placeholders and unsupported intersection or recursive-alias cases retain their existing behavior.
* `flow-upgrade` transformations preserve type arguments with both legacy and current parser AST fields.

Misc:
* Implements errors print the implemented interface's type and type arguments instead of generic descriptions such as "exports".
* Try Flow disables Clear and Copy code when the editor is empty, keeps the Format button stable as saved-state controls change, and places the AST format selector after its hide options.
* Tooling documentation recommends `flow-parser` and Prettier's built-in Flow parser without an external parser plugin.

Parser:
* `Program` source locations and ranges span the entire input, including whitespace and comments.
* `flow-parser` preserves optionality on constructor parameter properties.
* Correct `flow-estree` types for unannotated object type properties, readonly type operators, combined `in out` variance, optional `Program.tokens`, sparse-array elements, and JSX namespaced attributes. Sparse-array parent repair and traversal preserve holes.
* Omit absent `typeof` type arguments from ESTree output, including WASM-decoded nodes and detached builders.
* Default interface exports have the correct type-only export kind and AST typings. Type stripping removes the complete export wrapper rather than leaving a dangling `export default`.
* `flow-parser` type stripping preserves function parameter defaults and side-effect imports.
* `flow-transform` preserves leading `#!` lines.
* `flow-eslint` scope analysis handles TypeScript parameter properties, class member decorators, and `declare global`.
* Type translation preserves readonly array and tuple operands, inline interface bodies, and qualified interface bases.
* Type translation tracks required React imports for JSX-qualified types.
* Strict TypeScript translation rejects unsupported generic indexer keys instead of silently losing qualification or type arguments. Recovering string-API behavior is unchanged.

IDE:
* Hover on calls to polymorphic functions, including functions bound by `const`, properties, or parameters, and on construction of polymorphic classes shows the instantiated type arguments.
* Multiline unions and intersections in hover types are indented under their annotation, one member per line.
* Hover, `dump-types`, and error messages print `any` without its kind, e.g. `any` instead of `any (explicit)`.

Library Definitions:
* `encodeURIComponent` also accepts numbers and booleans.
* `Symbol.prototype.valueOf` returns `symbol` instead of `?symbol`.
* `arguments.callee` and `Function.prototype.caller` are typed as `Function`, and function `prototype` properties are typed as `any`.
* `TemplateStringsArray` is an interface extending `ReadonlyArray<string>`.
* Add `ImportCallOptions`, `ImportAssertions`, and `ImportAttributes` interfaces.
* `Date.prototype.toString` declares a `Date` receiver rather than inheriting an `Object` receiver.
* Locale-sensitive methods of `String`, `Number`, and `Date` use overloads and `Intl` option types.
* Calling `EvalError`, `RangeError`, `ReferenceError`, `SyntaxError`, `TypeError`, or `URIError` returns its own type rather than `Error`.
* `JSON.parse` reviver and `JSON.stringify` replacer callbacks accept any `this` and receive a `string` key.
* `JSON.stringify` returns `string`; its replacer is a function or a read-only array of strings and numbers.
* Declare legacy `RegExp` statics such as `RegExp.$1` and `RegExp.lastMatch`.
* `ArrayBuffer.isView` is a type guard that refines its argument to `ArrayBufferView`.
* `ArrayBufferView` is a structural interface rather than a union of typed arrays and `DataView`. Libraries can extend `ArrayBufferTypes` to add buffer types to `ArrayBufferLike`.
* Add `PromiseConstructorLike` and `ConcatArray`.
* Add the `ThisType<T>` marker interface. Flow does not give it TypeScript's contextual `this` behavior.
* Numeric typed array callbacks accept `thisArg?: any`, and their static `from` methods accept mapping functions from any element type.
* Libraries can redeclare `ReactDOM$UserVisibleString` to change the type accepted by user-visible React DOM string attributes such as `aria-label`, `title`, `alt`, and `placeholder`.
* `Boolean()` accepts no argument.
* `Object.defineProperty` and `Object.defineProperties` return their first argument's type.
* `Object.getPrototypeOf`, `Object.getOwnPropertyDescriptor`, `Object.getOwnPropertyNames`, `Object.getOwnPropertySymbols`, `Object.isSealed`, `Object.isFrozen`, and `Object.isExtensible` accept any argument.
* `Object.prototype.valueOf` returns `Object` instead of `unknown`.
* `String.prototype.matchAll` yields `RegExpExecArray` for string patterns as well as regular expressions.

### 0.338.0

Breaking Changes:
* Flowconfig option `experimental.assertion_functions` is removed. Assertion functions have been on by default and the option only accepted `true` since 0.337.
* Flowconfig option `experimental.disable_invariant_special_casing` is removed. It has been on by default and only accepted `true` since 0.337.
* Flowconfig option `experimental.function_and_object_type_is_no_longer_any` is removed. It has been on by default and only accepted `true` for a while.
* Flowconfig option `experimental.builtin_ts_libdef_dynamic_discovery` is removed. It has been on by default and only accepted `true`.
* Flowconfig option `experimental.declare_global_support` is removed. It has been on by default and only accepted `true`.

Likely to cause new Flow errors:
* `Readonly`, `Partial`, `Required`, `$Exact`, `Omit` and mapped types over an interface now include its methods and the members it inherits.
* Type guards now refine `Array<any>`, `{data: any}`, `Map<string, any>` and similar types to the guarded type, e.g. `Array<string>`, which can surface errors in code that relied on `any`.
* Static members can no longer reference the type parameters of their class, like in TypeScript, and reading a static member of a generic class no longer infers them.
* `keyof` of an object with a template literal indexer no longer widens the indexer key to `string` in conditional types like `Exclude`.

New Features:
* Mapped types over arrays and tuples support `readonly` and `writeonly` modifiers.
* Property truthiness checks such as `if (result.done)` now refine unions of classes and interfaces by fields with literal types, as `===` checks already did.
* Type guards narrow through optional calls such as `isString?.(x)`. When the callee or receiver is nullable, only the true branch narrows.
* Support optional assertion calls when their declared types guarantee the call runs.
* Added the intrinsic `ExactObjectType`, the supertype of every exact object type and a subtype of `{...}`.

Notable bug fixes:
* Fix crashes and false recursive-definition errors from cyclic type star exports in TypeScript declarations.
* Support namespace export aliases and ambient exported members in TypeScript definitions.
* Allow classes to extend a constructor whose overloads are not all generic, such as `Uint8Array<ArrayBuffer>`.
* Casting a value whose type intersects an enum with other types (e.g. `E & E`) to the enum's representation type no longer errors.
* Arrays can be assigned to variables and parameters whose type was inferred from `new Array<T>(...)`.
* Report delegated yield type errors at the yield expression.
* Align Try Flow's default lints and `declare global` support with regular Flow.
* Improve inference for generic values passed to polymorphic operations.
* Fix Try Flow initialization after the async builtin migration while preserving older releases' library definitions.
* Report conditional type instantiation errors at the caller.
* Apply type-parameter defaults when omitted optional or nullable arguments are the only evidence for them.
* An object with an indexer is no longer checked against optional properties it doesn't declare of a TypeScript-declared (`.d.ts`) object type, matching TypeScript.
* Fewer `underconstrained-implicit-instantiation` errors for generic calls passed as arguments to other generic calls.
* Fixed a crash when printing long types that contain non-ASCII names in error messages.
* Tuple types with a spread keep the variance of their own elements, e.g. `a` in `[readonly a: unknown, ...T]` stays read-only.
* `First` in `[infer First, ...infer Rest]` is now inferred instead of `unknown`.
* Fixed a deadlock in the Flow server monitor that could make all subsequent commands fail with "Out of retries, exiting!".
* Type guards with a union type now refine union members precisely, e.g. a falsy-value guard on `string | number` refines to `"" | 0`.
* Type guards now drop union members that are instances of classes unrelated to the guarded class, e.g. `x: A | B` refined by `x is A` is now `A` rather than `A | (A & B)`.
* The negative branch of a type guard now refines away generics that satisfy the guard, e.g. `x: T | number` refined by `!isT(x)` with `isT: (x: unknown) => x is T` is now `number`.
* In the negative branch of a type guard, `any` now stays `any` instead of becoming `empty`, and `boolean` is refined to the literal the guard does not cover.
* Correct stdin behavior for `flow check-contents` and `flow autocomplete`.
* Error messages print code that contains backticks, such as template literals, as markdown code spans.
* Instances of TypeScript-declared generics, such as `React.MouseEvent<T>`, now compare type parameters without a variance annotation covariantly, as type applications already did.
* Type guards now drop tuple members of a different length and array members with disjoint element types.
* Type guards now drop object and interface union members whose properties conflict with the guard type.
* `x instanceof C` no longer refines a value of an intersection type to `empty`.
* Linux release builds use jemalloc, which keeps the memory of long-running servers from growing after large rechecks.
* Fixed a race that could leave a file unmerged after a recheck, which made the server recheck forever while an open file imported it.

Misc:
* Print rest arrays as array types in errors.
* Print receiver and merged declaration types in errors.
* Simplify missing this annotation diagnostics.
* Print both types in inheritance errors.
* Identify missing parameter annotations directly at the parameter location.
* Remove empty filename metadata from the Try Flow AST display.
* Add a default-on option to hide locations in the Try Flow AST pane.
* Show node types first in the Try Flow AST pane.
* Add a default-on option to hide null and undefined fields in the Try Flow AST pane.
* Move Try Flow's JSON error output into an Errors format pill bar.
* Add JSON AST output to Try Flow.

Parser:
* Accept explicit semicolons on shorthand ambient modules.
* Accept keyword members in qualified type names and import types.
* Allow repeated inline interface exports in TypeScript files.
* Fixed `flow-parser` taking minutes to parse JS files over ~1 MB.
* Support destructured rest parameters in function type signatures.
* Support import attributes on re-export declarations.
* Preserve TypeScript accessibility modifiers on bodyless class method signatures.
* Preserve computed type-literal keys as properties in TypeScript source files.
* Allow newline-separated type members in TypeScript source files.
* Use `"in out"` for the ESTree variance kind of `in out` type parameters.
* Parse postfix non-null assertions with type syntax enabled, preserve ASI, and emit TypeScript-compatible `NonNullExpression` nodes.
* Respect JavaScript line terminators and Unicode whitespace when recovering source locations.
* Parse default interface exports and report unsupported syntax during type checking.
* Correct inferred ESTree source type for default interfaces, inline type specifiers, and exported import aliases.
* Exclude default interface exports and all-type specifier lists from inferred Babel module source type.
* Support top-level await for explicit modules and `.mjs`/`.mts` files in flow-parser.
* Always enable type parsing for TypeScript filenames.
* Reject type syntax in untyped parser mode and report precise diagnostics.
* Fix declaration method signature locations to include the complete signature.
* Include the `const` keyword in const enum declaration locations.

IDE:
* Hover and error messages no longer print redundant parentheses around function types in maybe types, e.g. `?() => void` instead of `?(() => void)`.

Library Definitions:
* Align well-known symbol members (`Symbol.toStringTag`, `Symbol.match`, `Symbol.toPrimitive`, ...) with TypeScript.
* `Map` and `WeakMap` `set`, and `Set` and `WeakSet` `add`, now return `this`.
* `RegExp` `flags`/`sticky`/`unicode` and `Number.EPSILON`/`MAX_SAFE_INTEGER`/`MIN_SAFE_INTEGER` are now read-only; `String.prototype.normalize` accepts the normalization form literals and the legacy HTML methods (`bold`, `link`, ...) are declared.
* `Array.prototype.copyWithin` returns `this`, so subclasses of `Array` keep their type.
* `Array.prototype.sort` returns `this`, so subclasses of `Array` keep their type.
* `Math` constants (`Math.PI`, `Math.E`, ...) are read-only.
* `Number.MAX_VALUE`, `MIN_VALUE`, `NaN`, `NEGATIVE_INFINITY` and `POSITIVE_INFINITY` are read-only.
* `String#split` requires a separator, `String#replace` has separate overloads for a string and a function replacement, and `String#length` is read-only, as in TypeScript.
* Use TypeScript's ES2018 async iterator and iterable definitions.
* Use TypeScript's ES2018 async generator definitions.
* Use TypeScript's ES2019 array definitions while preserving Flow's existing `flatMap` union order and inference.
* Use TypeScript's ES2020 promise definitions, including mutable settled results.
* Use TypeScript's ES2016 array `includes` definitions while keeping Flow's element refinement.
* Use TypeScript's ES2015 generator definitions. `Generator.prototype.return` now requires a value of the generator's return type.
* Add `React.CSSProperties`, typed by `csstype`.
* Use TypeScript's ES2015 promise definitions, adapted to Flow's inference.
* Use TypeScript's ESNext set method definitions, including `Set.prototype.union`.
* Use TypeScript's ES2015 `Map`, `Set`, `WeakMap` and `WeakSet` definitions. `size` is now read-only.
* `done` is now optional in the non-final branch of `IteratorResult`, as in TypeScript.
* Use TypeScript's ES2015 iterable definitions. `ArrayIterator`, `MapIterator`, `SetIterator` and `StringIterator` are now interfaces, and `StringIterator` takes a type argument.
* Add React's DOM types from `@types/react`, such as `React.MouseEvent`, `React.HTMLAttributes` and `React.JSX.IntrinsicElements`.
* `React.ComponentProps` of a string that is not a JSX intrinsic is `empty` in roots that define their own `$JSXIntrinsics`.
* Use TypeScript's ESNext iterator helper definitions. `reduce` without an initial value now requires the reducer to return the iterator's element type.
* `Array`, `ReadonlyArray` and typed array `toLocaleString` now accept locales and formatting options.
* Use TypeScript's ES2015 core definitions.

### 0.337.0

Likely to cause new Flow errors:
* `Function.prototype.bind` is now typed with TypeScript's `CallableFunction.bind` overloads, so errors in bound arguments are reported at `bind`, and a bound function's remaining parameters are a tuple type.
* `Function` and `Object` no longer mean `any` and no longer trigger `unclear-type` errors. They are now interpreted as the prototypes of functions and objects. Code that assumed they meant `any` may get new errors.
* Internal type `Function$Prototype$Bind` is removed.
* Class method `bind` now uses the declared `Function.prototype.bind` overloads. Bound methods may now report tuple arity errors when used as callbacks that ignore extra arguments or omit nullable parameters, because their inferred parameters are represented as tuple rest parameters.

New Features:
* Support [assertion functions](https://flow.org/en/docs/types/assertion-functions/). If you relied on Flow's special handling of `invariant`, give it an `asserts` signature.
* Execute runtime Flow syntax in CommonJS and ES modules with original stack locations through `flow-node`.
* Support assertion narrowing through `this` and `super`, including [receiver assertions](https://flow.org/en/docs/types/assertion-functions/#toc-asserting-the-receiver).
* `Readonly` now supports `Array` and `ReadonlyArray` types.

Notable bug fixes:
* Honor opaque upper bounds in template interpolation and nullable subtyping.
* Preserve nominal opaque inference when checking nullable targets.
* Fixed spurious `Recursion limit exceeded` errors when checking conditions, `match` and other expressions whose type is a trivially recursive generic type alias.
* Prevent unsound assertion narrowing when annotations refer to shadowed local declarations.
* Qualified type references through a union of namespaces (e.g. `(cond ? A : B).T` via a local) now look up the type in each namespace instead of reporting it as missing.
* Prevent saved-state startup and incremental match checking from crashing when a recheck is canceled.

Misc:
* Document assertion functions.

Parser:
* Add missing AST fields and node types to the Flow parser output. `implicitDeclare` is now omitted unless `true`.
* Emit `TypeParameter.bound` as a bare type node or `null` in ESTree output. Babel output retains the `TypeAnnotation` wrapper, and the TypeScript-to-Flow API translator generates `extends` bounds.
* ESTree enum declarations now use a unified `EnumBody` node with string-or-null `explicitType`. Babel output retains its existing enum body types.
* Parse typed function overload signatures in TypeScript implementation files.
* Parse destructured parameters in TypeScript function types outside declaration files.
* Parse optional class method signatures in JavaScript and TypeScript files.
* Parse optional TypeScript constructor parameter properties.
* Parse namespace export aliases and ambient exported members in TypeScript definitions.
* Fix parsing of class modifiers before synchronous generator methods.
* Support `readonly T[]` in unlabeled tuple elements and require parentheses when optional.

Library Definitions:
* Add `Intl.Segmenter` and `Intl.supportedValuesOf`.
* Add `Intl.NumberFormat.prototype.formatRange` and `formatRangeToParts`, and the ES2023 `NumberFormat` options; `format` accepts `bigint` and numeric strings.
* `String.prototype.localeCompare`, `toLocaleLowerCase` and `toLocaleUpperCase` accept `Intl.LocalesArgument`; `matchAll` with a `RegExp` returns `RegExpExecArray` matches.
* `Float16Array.prototype.toLocaleString` takes `Intl.NumberFormatOptions`.
* Add ES2024 `ArrayBuffer` resizing, transfer, and constructor options.
* Align `Reflect` APIs with TypeScript property, descriptor, key, and constructor definitions.
* Reuse TypeScript's `Float16Array` declarations while preserving Flow covariance and callback compatibility.
* Align `Proxy` APIs with TypeScript and check array elements against numeric interface indexers.

### 0.336.0

Likely to cause new Flow errors:
* Calls such as `Array.from` can now infer narrower literal types, requiring explicit annotations where broader types are needed. Unconstrained class type arguments now use their bounds instead of `empty`.
* Flow now checks `ref` compatibility for JSX tags typed as `string`, exposing refs that assume a more specific element type.
* Explicit `this` parameters on methods in library definitions no longer need to accept the instance type.
* Generic rest-parameter inference can now expose previously hidden parameter-type errors. For example, `(input: string, ...args: Array<unknown>) => void` no longer satisfies `(...args: TArgs) => R` when `TArgs` is inferred as `Array<unknown>` ([Try Flow example](https://flow.org/try/#0C4TwDgpgBAwg9gWwQQwHYBMoF4oAoCWqYArsAFxQDOwAToQOYA0UAdG8jfZRQII03IQAHmKoA1qjgB3VAD4AlNllQAbnHzoA3ACht6CAGMANh2gAzUQeD44qKEbj16EdEIAqfLlAgAPYBAxKKAAlCGR0WyMQPgFhUQlpOWZg2VxtKCgECGAACzh0Clw2Fg4uCg9OSkUsZWDGbXlC4tLuKAquatqdPUMTGmgDW2ooQaQ0AthEFAwdBycXXFHp9HlNKAB6dagAUX44GgoAA3jJGUOofCDCUbBkawAjI2gpfFyoQ+o6VHpD7SA)).
* Rest destructuring of exact tuples, e.g. `const [first, ...rest] = t`, now excludes preceding elements' types from `rest`.
* Variance annotations are now checked through tuple type spreads, so `type T<out X> = [string, ...[X]]` is an error.

New Features:
* Support tuple types with trailing rest elements, e.g. `[string, ...Array<number>]`. Array literals ending in array spreads and arguments collected by rest parameters can flow to these types.

Notable bug fixes:
* Declaration merging for interfaces and `declare class` now preserves variance annotations.
* Returns inside functions nested in a type guard function are no longer checked against the enclosing type guard.
* References inside `declare namespace` blocks now see members merged across blocks and library files.
* Functions and constructors are now accepted as builtin `Function` and `Object` values.
* Fixed spurious indexed-access errors involving recursive class type-argument bounds.
* Type arguments used in later tuple spreads, e.g. `B` in `[...A, ...B]`, are now inferred instead of left underconstrained.
* Conditional types using `infer` on rest parameters now preserve the matched function's optional parameters and rest element in the inferred tuple, e.g. `[number, ...Array<boolean>]`. Matching functions with fewer parameters than the pattern and no rest parameter no longer produces underconstrained errors.

Misc:
* `flow-upgrade` now uses Flow parser packages and requires Node 18 and Prettier 3.

Parser:
* Correct JavaScript source columns after characters represented by surrogate pairs.

IDE:
* Hover `Go to` links no longer include the hovered declaration itself.

Library Definitions:
* `Intl` is now a builtin namespace with TypeScript-compatible definitions through ES2020, including `Collator`, `NumberFormat`, `DateTimeFormat`, `PluralRules`, `RelativeTimeFormat`, `Locale`, `DisplayNames`, `getCanonicalLocales`, and `formatToParts` for dates and numbers.
* The legacy `Intl$CollatorOptions`, `Intl$DateTimeFormatOptions`, and `Intl$NumberFormatOptions` aliases now match TypeScript. `Intl.DateTimeFormatPartTypes` includes `'unknown'`, and locale-aware `Date` methods accept `Intl.LocalesArgument` and `Intl.DateTimeFormatOptions`.
* The global `LocalesArgument` type is now `Intl.LocalesArgument`. Library declarations such as flow-typed's `declare var Intl` hide the builtin namespace and must be removed.
* `React.ElementConfig` is removed; use `React.ComponentProps` instead.
* Typed arrays gain `findLast`, `findLastIndex`, `toReversed`, `toSorted`, and `with`, and their constructors can be called without arguments. For types other than `Float16Array`, `findLast` narrows its result with a type-guard callback. Callbacks to `findLast` and `findLastIndex` can no longer annotate `this`.
* `Float16Array` now matches TypeScript's declaration, including its `from` overloads. `length`, `byteLength`, and `byteOffset` are read-only, `find` and `findIndex` predicates must return `boolean`, and callbacks can no longer annotate `this`.
* Add RegExp indices and flags, well-formed string methods, bigint Atomics, `Error.isError`, `Promise.try`, `Atomics.pause`, and additional ArrayBuffer and Date constructor signatures.
* Add TypeScript signatures for `Array.fromAsync`.
* `AggregateError.prototype.errors` is now typed `any[]`, matching TypeScript.
* Decorator definitions now match TypeScript: `ClassDecorator` accepts classes, decorator context and metadata types are available, and `Function` includes `[Symbol.metadata]`.
* Add `DisposableStack`, `AsyncDisposableStack`, `SuppressedError`, `Disposable`, and `AsyncDisposable`. `AsyncGenerator` now extends `AsyncIteratorObject` and includes `[Symbol.asyncDispose]`.

### 0.335.0

Breaking Changes:
* Removed the legacy `builtin_lib=flowlib` and `builtin_lib=flowlib-with-lib-dom-d-ts` options.
* Removed the Flow config options `exact_by_default`, `experimental.always_generalize_jsx`, `experimental.fast_symlink_resolution`, `experimental.importable_global_libdefs`, `experimental.interface_dictionary_typing_fix`, `experimental.new_this_typing`, `experimental.opaque_type_new_bound_syntax`, `saved_state.allow_reinit`, `saved_state.direct_serialization`, and `saved_state.reinit_on_lib_change`. These options had been enabled by default and only accepted `true`.

Likely to cause new Flow errors:
* `Symbol` and `Promise` now use TypeScript-style instance and constructor interfaces. Code relying on their previous class typing may see different errors.
* Some JSX expressions that were previously considered underconstrained now infer `unknown`.
* Fixed type argument inference that incorrectly used a bound or `unknown`. More underconstrained errors may appear.
* `String.prototype.replaceAll` now has separate overloads for a replacement string and a replacer function, matching TypeScript. Passing a union of these types is an error.
* `RegExpMatchArray.groups` is now optional and cannot be `null`, and `RegExp.prototype.dotAll` is read-only, matching TypeScript.
* `new $ReadOnlyArray` now causes an error.
* `Symbol()` now only accepts a `string | number` description, and `Symbol.keyFor` returns `string | void`, matching TypeScript.
* Deprecated syntax now consistently causes errors, including in library definition files.
* Type arguments of a polymorphic value flowing to a non-polymorphic type are now solved, which can surface previously hidden errors.
* `React.PropsOf` now uses `React.ComponentProps` to find component props. Additional errors may occur.

New Features:
* Support assertion functions exported through CommonJS, assertion calls in return position, and assertion calls in arrow expression bodies.
* The `undefined` type can be used in library definitions, where it means `void`.
* `flow-remove-types` supports abstract classes, abstract members, override modifiers, `satisfies`, optional class fields, and additional type-only imports and exports.
* Template literals infer precise types in `const` declarations and annotated initializers.

Notable bug fixes:
* `as const` on template literals infers template literal types like TypeScript.
* Implicit instantiation can infer a type argument through `React.ComponentProps` and other conditional types with `infer`.
* Interfaces with construct signatures no longer print a spurious `this` parameter in error messages and hovers.
* `this`-typed methods inherited from builtins such as typed arrays now return the subclass.
* Exported `declare class` declarations extending builtins such as `Error`, `Map`, `Set`, or `Date` now inherit the builtin's statics and constructor.
* Arguments of `new X(...)` are now contextually typed when `X` is annotated with the `typeof` of a constructor interface value.
* `typeof x === 'function'` now refines builtin constructors such as `Map` and `Symbol`, and interfaces with call signatures, as functions.
* User-defined assertion conditions check member access in test context.
* Recognize ESM default imports of CommonJS assertion functions and callable objects with `asserts` call signatures.
* `instanceof` and `declare class ... extends` now use the `prototype` of a constructor interface that has no construct signature, instead of narrowing to `empty` or reporting "not inheritable".
* Fixed a memory leak in type signature parsing that inflated server memory on large codebases.
* Fixed checking interfaces that extend classes, including object values and multiple generic class supers.

Misc:
* `React.ElementConfig` is deprecated in this version and will be removed in the next version. Use `React.ComponentProps` instead.
* The `unnecessary-invariant` lint is renamed to `unnecessary-assertion`; the old name is still accepted.
* Constructor errors and variance check errors now print types instead of descriptions such as "object type", "module", or "exports".
* Unary arithmetic on a maybe-typed operand reports `null` and `void` separately instead of "null or undefined".
* Record type utility errors name the utility (`Partial`, `Required`, or mapped type).

Parser:
* Store abstract property annotations in `typeAnnotation` and emit `value: null`.

IDE:
* Hover on flow.org/try now shows types framed as declarations, matching the VS Code extension.
* Autocomplete and go-to-definition on instances of classes extending builtins such as `Map`, `Set`, `Error`, and `Date` now show the builtin's instance members.
* JSX attribute autocomplete on a union or intersection of components offers the props of the components the element is checked against.
* Go-to-definition on a JSX attribute of a union or intersection of components uses the props of the components the element is checked against.
* Find-references on a JSX attribute of an intersection of components finds the attributes passed to the branch the element is checked against.
* Qualify `React.ComponentProps` in generated type annotations.

Library Definitions:
* `React.ComponentProps<'div'>` now includes the optional ref prop.
* Added `Promise.withResolvers`.
* The type parameters of `Map.groupBy` are now ordered `<K, T>`, matching TypeScript.
* `RegExp.prototype[Symbol.matchAll]` is now a method and `RegExpStringIterator` an interface, matching TypeScript.

### 0.334.0

Likely to cause new Flow errors:
* Improved generic type inference with conditional types. Some places might infer a different type and cause new errors.
* `Number`, `String`, `RegExp`, `Date`, native errors, maps, and sets now use TypeScript-style instance and constructor interfaces, while Math, JSON, and Atomics use named interfaces. Existing code relying on their previous class or object typing may produce new errors.

New Features:
* Allow object type spreads over interfaces and inexact object types.

Notable bug fixes:
* Allow classes to extend generic construct-signature interfaces.
* Generic values re-exported through annotated CommonJS getters retain their polymorphic types.
* Preserve record lowering source locations.
* Preserve match lowering source locations.

Parser:
* Support semicolonless abstract method declarations.

### 0.333.0

Likely to cause new Flow errors:
* Imports of `react` module will always resolve to Flow's builtin react type definitions. If you use a custom react typing module, you might see errors.

Notable bug fixes:
* Preserve Flow WASM linker settings, including memory growth.
* Correct parser ranges for `unique symbol` types and signed mapped-property variance.

Parser:
* Rename the `ImportType` ESTree field from `argument` to `source`.
* Preserve computed object type property keys in parser output.

Library Definitions:
* Flow will now include DOM typing from TypeScript's `lib.dom.d.ts` by default. If your project is not supposed to run in the browser, you can switch back to the old behavior with `builtin_lib=core-only` in flowconfig's `[options]` section.
* Flow's `Iterable` and `ArrayLike` type now align with those from TypeScript. You might see errors when your existing code assumes the old typing.

### 0.332.0

Breaking Changes:
* `no_flowlib` option is removed. Use `builtin_lib=flowlib` (default) or `builtin_lib=prelude` instead.

Likely to cause new Flow errors:
* Render types no longer accept exact react element as arguments. See https://flow.org/en/docs/react/render-types/ for supported cases
* Flow will now always infer a react jsx element to be a render type if it's an element of a component-syntax component, or `React.MixedElement` otherwise. Legacy code that requires exact react element will see new errors.
* Add an option for rejecting dynamic imports in Haste modules.

New Features:
* `Symbol()` and `Symbol.for()` create a `unique symbol` when they initialize a `const`, so symbols can be used as property keys without explicit declarations.

Notable bug fixes:
* Bound error type normalization to prevent excessive memory use
* Release temporary signature-hash graphs after Flow merges.
* Using `import typeof` of a generic value without type arguments keeps its polymorphism instead of reporting missing-type-args, and `renders` accepts `import typeof` of a generic component.

Parser:
* Parse [deferred imports](https://github.com/tc39/proposal-defer-import-eval): `import defer * as ns from "m"` and `import.defer("m")` now set the ESTree `phase` property to `"defer"`.

IDE:
* Hover on members accessed through `super` shows the declaration
* Hover on destructuring keys shows the member declaration
* Hover on enum members shows the member declaration
* Hover on signature labels and indexer keys shows the parameter
* Hover on component attribute names shows the prop declaration
* Hover on import/export sources shows the module
* Hover on this-parameters shows the parameter
* Hover on indexed-access literals shows the member
* Hover on polymorphic JSX elements no longer exposes overly precise literal types when no annotation demands that precision
* Hover on a call to an overloaded function reports how many other signatures the function has
* Hover lists the signatures of an overloaded declaration instead of printing their intersection
* Component hovers no longer drop indexer rest props
* Align multiline union members in hover type declarations
* Render hover definition links in a compact Rust-style navigation row.
* Expand rest-prop aliases in top-level component hovers.

Library Definitions:
* React ref callbacks now have a stricter return type
* `import.meta` will no longer have `url` property and arbitrary unknown property. To add them back, you can do so through declaration merging (e.g. adding `interface ImportMeta { url: string }` to your global library definitions.
* `ArrayLike` and `ArrayBuffer` now follow TypeScript definitions
* Stricter typing for `Array(length)` and `Array(items)`

### 0.331.0

Likely to cause new Flow errors:
* Fixed unsound subtyping of instances against indexer in interfaces ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sAD0bZKcEYIncAdJCgkJ+RlQxjO3PgMXLV6rRJINaUDfMnTZIFhjATN0p9ZQQDCEJCBEMCTgwEH4hMnwhKABrKAgAdyh8OEywCLAAC3wANyo4LFxscWQuHnc2bCL8aFxaTQlaJXwWZlNa-nrmbFyJbDYAWnalXvMSJOwJAtox4VxcafdZ+cXl0RxKddioMAqqCY7sA5BEqCHKkiol7JKVDCdL69uSe8oliEZI6DcMYFDDZd66DAwbBjBDzEaUEhjMgSeFVGrmRJxUZjZhhOBkS4KJQqNROLQYL7KfR-ZQRCD7apmbAAX34kHqgXqxyo2AABNyvryAGLEXnAXmYSjIXl6FRQBC85kAbgAOkdoHpeTBRQBeXk3NLC4gACgAlKrYKKKbzsvCuGA+cAANpJaWy7IIAC60rQxBOUGVvIA9EH9RBecpKPSJdhtSxeBHqNHUmk1TEQEVlCQ4NBAkUAAyaADMRcL+ZAzKAA)).
* Flow will no longer emit `method-unbinding` errors and instead perform stricter this-type checking ([example](https://flow.org/try/?fbclid=IwY2xjawUGknpwZG9mA2V4dG4DYWVtAjExAGJyaWQRMXN4dWFRdEN4c0RkRDFMYlBzcnRjBmFwcF9pZAEwAAEeMSGTzT2DFoEa38osZQ3wNnAikRq8kJRJFv_rTctJA4iYeWGisqquP3bCM3w_aem_yffePSXvBRLunq68HFKcYw#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sAD0bZKcEYIncAdJCgkJ+RlQxjO3PgMXLV6rRJINaUDfMnTZIFhjATN0p9ZQQDCEJCBEMCTgwEH4hMnwhKABrKAgAdyh8OEywCLAAC3wANyo4LFxscWQuHnc2bCL8aFxaTQlaJXwWZlNa-nrmbFyJbDYAWnalXvMSJOwJAtox4VxcafdZ+cXl0RxKddioMAqqCY7sA5BEqCHKkiol7JKVDCdL69uSe8oliEZI6DcMYFDDZd66DAwbBjBDzEaUEhjMgSeFVGrmRJxUZjZhhOBkS4KJQqNROLQYL7KfR-ZQRCD7apmbAAX34kHqgWOFJIAAIAGLEHnAAA6UB54p5mEoAAoAJRC5mixVQUU6PSSqg8gC8PJuaX5xDlmilAG4eQB6c08wZFOAQOItHlqCT5CDjRJobJsbIIHnKSj03i69LBv3UekkUVSuVmy3Wli2+0kR2pMMByhB1L6-0R0VRzUUnly7UAPh5RQgcDYsatNrtDtooZzGeD2fDlB5bBkPNCPL0Km8yh7+XxPY6Pp5opiICKyhIdqggXCemUIGZQA)).

IDE:
* Improve the primary type displayed by hover
* Show a single type in `type-at-pos` and hover
* Truncate oversized hover types and their reference lists

Library Definitions:
* `React.ReactElement` now matches latest type in `@types/react`. Added missing `React.JSX.Element` that makes the type in `@types/react `.

### 0.330.0

Likely to cause new Flow errors:
* Flow will now restrict the allowed syntax forms of `.call/apply/bind` calls. Only expressions of the form `<expr>.m.call(<expr>, ...)`, `<expr>.m.apply(<expr>, ...)` and `<expr>.m.bind(<expr>, ...)` are allowed.
* `[untyped]` section of flowconfig will use the glob system. See https://flow.org/en/docs/config/untyped/

Notable bug fixes:
* Fixed https://github.com/meta/flow/issues/9407.
* Support `instanceof` refinement of constructor objects.
* Operations like `Partial` no longer strip readonly-ness of dictionaries. ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sAD0bZKcEYIncAdJCgkJ+RlQxjO3PgMXLV6rRJINaUDfMnTZIFhjATN0p9ZQQDCEJCBEMCTgwEH4hMnwhKABrKAgAdyh8OEywCLAAC3wANyo4LFxscWQuHnc2bCL8aFxaTQlaJXwWZlNa-nrmbFyJbDYAWnalXvMSJOwJAtox4VxcafdZ+cXl0RxKddioMAqqCY7sA5BEqCHKkiol7JKVDCdL69uSe8oliEZI6DcMYFDDZd66DAwbBjBDzEaUEhjMgSeFVGrmRJxUZjZhhOBkS4KJQqNROLQYL7KfR-ZQRCD7apmbAAX34kHqgXqxyo2AABDo9LyYMRkLzgMKIKK9CooAhmQBuAA6sGIvIpvIAClRItwADzATxsZq0XkAbSSUqk2QQAF1RYkUukoMyAHzy3kAeg9vMGRTgEDivOUlHpvF5qTSvIgSWVXNwPP50EFmEooq1lB1uH1huNZotvOl1rtvIdEedbt5ypTpoA5BgazbeQBeXkAZndXp9LD9AZIUaSYYjQeo9OVMRARWUJH9UEC4T0yhAzKAA))

### 0.329.0

Likely to cause new Flow errors:
* More subtyping errors will print the types that are incompatible. Some errors might have new locations.
* A `unique symbol` annotation is now an error unless it is the type of a `const` variable, of a `readonly` object type or interface property, or of a `static readonly` class field.
* Flow will error on legacy variance syntax in library definition files

New Features:
* We have released `flow-parser` and `flow-eslint` packages that are drop-in replacement for `hermes-parser` and `hermes-eslint`. [Babel docs](https://flow.org/en/docs/tools/babel/) and [ESLint docs](https://flow.org/en/docs/tools/eslint/) have been updated.
* Object properties, class fields, methods, and the members of an interface or `declare class` imported from another module, can be keyed by a `unique symbol`.

Notable bug fixes:
* Prevent a race that can leave server hanging
* Support TypeScript constructor objects in Flow class positions.
* `keyof` over a `symbol` index signature is now `symbol` rather than `string`.
* A class can extend a value with a construct signature.
* `x !== undefined` no longer stops refining after a call that passes `undefined` as an argument: (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sAD0bZKcEYIncAdJCgkJ+RlQxjO3PgMXLV6rRJINaUDfMnTZIFhjATN0p9ZQQDCEJCBEMCTgwEH4hMnwhKABrKAgAdyh8OEywCLAAC3wANyo4LFxscWQuHnc2bCL8aFxaTQlaJXwWZlNa-nrmbFyJbDYAWnalXvMSJOwJAtox4VxcafdZ+cXl0RxKddioMAqqCY7sA5BEqCHKkiol7JKVDCdL69uSe8oliEZI6DcMYFDDZd66DAwbBjBDzEaUEhjMgSeFVGrmRJxUZjZhhOBkS4KJQqNROLQYL7KfR-ZQRCD7apmbAAX34kHqgRgiW8cGgAAI2PjDAt8gAKeQAfmQfOEIj2AEo+cA+SwJEJKFA+VIZHzmQBuAA6UCNXKOAM1MFFJGlehUUAQfIAPnyihA4Gx5dLXe6lUa+Xy4DA+aLBUw8mLEvV4DcPXyAGRxvkkPkAQgAvGm+ZHsNHRorgH7-UnNBV7RJ8nq+QB6KtJxjq3lxaUAA1LCHLzYDyZE+JI2Qd2SzUCj2VGheZRonUBiICKyj70ECRQADJoAMwAJgAHJplyBmUA))

Library Definitions:
* Well-known symbols on `Symbol` are typed as `unique symbol`, and `Symbol.metadata` is now available.

### 0.328.0

Breaking changes:
* `[include]` and `[ignore]` sections in flowconfig should use globs instead. Read https://flow.org/en/docs/config/include/ and https://flow.org/en/docs/config/ignore/ for more information.

New Features:
* `this` type guards can now be used on non-static methods of regular classes, not just `declare class` and interface methods. The method body is checked for consistency against the guard type.

Notable bug fixes:
* Prevent cached IDE results from blocking rechecks.

Library Definitions:
* Allow symbols to be used as `WeakMap` keys

### 0.327.0

New Features:
* In an object type, a bracketed key written as a string or number literal, or as the name of a value, is now a computed key naming a single property rather than an index signature, so several such keys can coexist. A key written as a type-only name is unchanged, and the labeled form `[label: K]` keeps index-signature semantics. Breaking: a key headed by a value binding is read as a computed key. Thus a class or enum key such as `{[C]: V}` is now an error, and a qualified type exposed through a class/namespace merge, as in `{[C.K]: V}`, must use the labeled form `{[key: C.K]: V}` to remain an index signature.
* In an interface or a `declare class` body, a bracketed key written as a string or number literal, or as the name of a value, is now a computed key naming a single property rather than an index signature, so several such keys can coexist, and `static` puts the property on the static side. A key written as a type-only name is unchanged, and the labeled form `[label: K]` keeps index-signature semantics. Breaking: a key headed by a value binding is read as a computed key, so a class or enum key such as `interface I {[C]: V}` is now an error and must use `[key: C]` to remain an index signature.
* A computed method key `[expr](): T` that resolves to a single literal string or safe-integer number is now accepted in interface, `declare class`, and object type bodies (previously a parse error outside `.d.ts` files) and reads as a named method.
* A bracketed access on an array with a string-literal key (for example `arr['map']` or `arr[Symbol.iterator]`) now reads the corresponding named member instead of erroring that the key is not an array index, and a string key holding a safe integer (`arr['0']`) indexes the array like the equivalent numeric key.

Notable bug fixes:
* `keyof any` now evaluates to `any` as expected. Spurious errors related to the bug will no longer fire.
* Fix a server crash ("thread 'flow-tokio-runtime' has overflowed its stack") when rechecking a file with an attached LSP client

Parser:
* Parse `component` and `hook` type annotations as the declaration of `declare export default`

Library Definitions:
* Add ES2026 Map methods

### 0.326.0

Breaking changes:
* The following flags are removed since they no longer change anything
  * `experimental.channel_mode`
  * `experimental.channel_mode.windows`
  * `experimental.long_lived_workers`
  * `experimental.long_lived_workers.windows`
  * `gc.worker.custom_major_ratio`
  * `gc.worker.custom_minor_max_size`
  * `gc.worker.custom_minor_ratio`
  * `gc.worker.major_heap_increment`
  * `gc.worker.minor_heap_size`
  * `gc.worker.space_overhead`
  * `gc.worker.window_size`
  * `sharedmemory.hash_table_pow`
  * `sharedmemory.heap_size`

Likely to cause new Flow errors:
* General subtyping errors will print the types that are incompatible. Some errors might have new locations.

### 0.325.0

No behavioral changes in this release

### 0.324.0

New Features:
* .flowconfig's `[include]` and `[ignore]` sections can now be configured using globs, in addition to the current regex based system. Right now, glob items must be specified with `glob:` prefix. Globs do not support `<PROJECT_ROOT>`, since they are assumed to start from the project root.
* Class fields and methods whose computed key resolves to a literal string or number are now supported and type-checked as named members.

### 0.323.0

No behavioral changes in this release

### 0.322.0

Notable bug fixes:
* Fixed https://github.com/meta/flow/issues/9448
* Fixed a crash in pattern matching. ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sAD0bZKcEYIncAdJCgkJ+RlQxjO3PgMXLV6rRJINaUDfMnTZIFhjATN0p9ZQQDCEJCBEMCTgwEH4hMnwhKABrKAgAdyh8OEywCLAAC3wANyo4LFxscWQuHnc2bCL8aFxaTQlaJXwWZlNa-nrmbFyJbDYAWnalXvMSJOwJAtox4VxcafdZ+cXl0RxKddioMAqqCY7sA5BEqCHKkiol7JKVDCdL69uSe8oliEZI6DcMYFDDZd66DAwbBjBDzEaUEhjMgSeFVGrmRJxUZjZhhOBkS4KJQqNROLQYL7KfR-ZQRCD7apmbAAX34kHqgRgiW8cGgAAIRnp8AghFQ2Pg0nAJIUsNJ-vg0BA2LQABQlXDIPmJFLpKAASk1RQgcDYfOAAB0oHy+eEFvk+WruHqzZbrdaAOQwYjuvlwGAOgCC1DomnxQZoqvVeudAF4AHxmvkAeiTfJI+QgQlwptSEj5YBo6ddbutLAkQkoUAA3MXrcza3z8Hz42b61a+W22zEQEVlCReVBAkUAAyaADMACYAIyaYcgZlAA))
* Refining a union by `instanceof` no longer leaks `any` from members that cannot match the guard (e.g. `Foo<string> | Bar<number>` refined by `instanceof Foo` now yields `Foo<string>` instead of `Foo<string> | Foo<any>`). This may surface real errors that were previously masked.

### 0.321.0

Parser:
* `flow-parser` is now backed by the Rust port parser. It will have public API similar to that of `hermes-parser`.

### 0.320.0

Likely to cause new Flow errors:
* Object operations on class instance types now produce plain object types instead of preserving the class instance's nominal identity. This affects object type spreads and utilities such as `Partial`, `Required`, `Readonly`, `Pick`, and `Omit`. [Example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sAD0bZKcEYIncAdJCgkJ+RlQxjO3PgMXLV6rRJINaUDfMnTZIFhjATN0p9ZQQDCEJCBEMCTgwEH4hMnwhKABrKAgAdyh8OEywCLAAC3wANyo4LFxscWQuHnc2bCL8aFxaTQlaJXwWZlNa-nrmbFyJbDYAWnalXvMSJOwJAtox4VxcafdZ+cXl0RxKddioMAqqCY7sA5BEqCHKkiol7JKVDCdL69uSe8oliEZI6DcMYFDDZd66DAwbBjBDzEaUEhjMgSeFVGrmRJxUZjZhhOBkS4KJQqNROLQYL7KfR-ZQRCD7apmbAAX34kHqgWOFJIAAIAMI84AAHSgPJ58mQPOEIj2PIAvDyAAwAbhFYpEAAoAJSSooQOBsQXMkXGqAi+pclg8nR6HmGSiRbiSgAKVEduAAPHyAHyqqD2908in85U8gD0YZ5AFFqPTJQADV0Osqen3xnn4oOiiBoABWQwkPMmsilEELGB58b56eyeleYGwmhFIuLPIAyoN2AKFcBNH2+bweX3NMy-RbcFRsNboLamJ42JKO-O+X656xDcGV+HIzHKHGeWv2NkEFmM7oNEcp63cWwhA3ea8eTn894i+cm2anOceQApOISAANeUeWdKIki9QcAHJ5Eg31zSGCcrRtQtc3-ADJT-PQAL9VCsM0TUtVDCNo1jSgE1AsBwIHHloNg9NmloacyVrIt8inRgwNGJ8MjtPdiXaD8YhAIplBIOBoECIpFU0ABmABGABOTRFRAZkgA)

### 0.319.0

Likely to cause new Flow errors:
* The config for `casting_syntax` is removed. The `as` casting syntax is the only supported casting syntax. See [Modernizing legacy syntax](https://flow.org/en/docs/modernizing-legacy-syntax/) for migration guidance.
* Flow will now error on `T: B` in favor of `T extends B` in type parameters, and error on `+`/`-` in favor of `readonly`/`writeonly` . See [Modernizing legacy syntax](https://flow.org/en/docs/modernizing-legacy-syntax/) for migration guidance.

New Features:
* Support the global `NaN` as a `match` pattern

Notable bug fixes:
* Fixed negative bigint literal patterns in `match` being treated as their positive counterpart
* `ConstructorParameters` and `InstanceType` now work on `Class<C>`, not just `typeof C`
* `match` instance patterns whose constructor is a subclass of the scrutinee type are no longer incorrectly reported as unused.

### 0.318.0

Likely to cause new Flow errors:
* Flow will now error on all deprecated utility types. See [Modernizing legacy syntax](https://flow.org/en/docs/modernizing-legacy-syntax/) for migration guidance.

### 0.317.0

New Features:
* Allow `-?` (optionality removal) in mapped types.
* Adds support for utility types `ConstructorParameters` and `InstanceType`

Misc:
* The `.flowconfig` options `component_syntax`, `enums`, and `pattern_matching` now default to `true`. To keep the previous behavior, set the corresponding option to `=false` in your `.flowconfig`.

### 0.316.0

Likely to cause new Flow errors:
* `deprecated-type` and `deprecated-type-bool` errors are now always on (the lints were on by default already). Update any references of the type `bool` to `boolean`, and delete references to the lints in your `.flowconfig` and `flowlint` comments — the setting is a no-op for this release and will be removed in a future release.

### 0.315.0

Misc:
* Variance keywords (`readonly`, `writeonly`, `in`, `out`) on type parameters and object/class properties are now enabled by default. The `experimental.allow_variance_keywords` is no longer needed to opt in.
* The deprecated variance sigils error message now suggests the contextually correct keyword (`readonly`/`writeonly` for property positions, `out`/`in` for type parameter positions) instead of listing both alternatives.

### 0.314.0

Likely to cause new Flow errors:
* Deleted the `implicit-inexact-object` lint, it no longer does anything since `exact_by_default=false` was deprecated.
* `exact_by_default=false` is now deprecated in `.flowconfig`. Read this [blog post](https://medium.com/flow-type/exact-object-types-by-default-by-default-cc559af6f69) on how to migrate, and our [docs on objects](https://flow.org/en/docs/types/objects/) for more. `exact_by_default=true` is the default (since 2023) so simply delete the option when migrating.

Notable bug fixes:
* Fixed incorrect enum cast suggestion when using an enum as a type argument

### 0.313.0

Likely to cause new Flow errors:
* Flow has changed its casting syntax from `(x: T)` to `x as T`. The default value of the option `casting_syntax` is now `as`, and Flow will error on the old syntax. If you want to preserve the old defualt value, add `casting_syntax=both` to your `.flowconfig`. You can use the npm package `flow-upgrade` to upgade: `yarn run flow-codemod typeCastToAsExpression`. Read the [docs](https://flow.org/en/docs/types/casting/) for more.

New Features:
* Exported class setters no longer require an explicit type annotation: `void` is used if omitted

Parser:
* The flow-parser estree output now aligns more with hermes-parser: `ArrayExpression` gains a `trailingComma` field, optional chaining is wrapped in a `ChainExpression`, `AssertNonnull` (`expr!`) emits a `NonNullExpression` instead of an optional node, type-position `this` emits `ThisTypeAnnotation`, and `TypeParameter.bound` is now a bare type node without the `TypeAnnotation` wrapper.

### 0.312.1

* Fix in saved state checks after a rebase

### 0.312.0

Likely to cause new Flow errors:
* Flow will now error on variance modifiers (`+T`, `-T`, `in T`, `out T`) on interface method type parameters, matching the existing rejection on class methods and function types

New Features:
* Declaration merging between `declare class` and `interface` of the same name in the same scope.
* Flow now separates value namespace and type namespace, so code like `const A = 1; interface A {}` is now accepted.

### 0.311.0

New Features:
* Added support for string and number literal properties in `declare class` and `interface`

Library Definitions:
* Change the `Record<K, T>` utility type's key bound from `K: string` to `K: PropertyKey`, allowing `number` and `symbol` keys in addition to `string`
* Added some TS built-in types that were missing from Flow's core lib

Parser:
* Added support for `writeonly` property variance keyword

Misc:
* When `experimental.allow_variance_keywords` is set to `true`, we support the keywords `in` / `out` / `readonly` / `writeonly` along with `+`/`-` for variance

### 0.310.0

New Features:
* Support constructor type signatures (`new (...) => Type`) in interfaces and `declare class`

Misc:
* When `allow_variance_keywords` is set to true, Flow stops erroring for `in` / `out` / `readonly`

### 0.309.0

No significant changes.

### 0.308.0

Parser:
* The 5 enum body ESTree node types (`EnumBooleanBody`, `EnumNumberBody`, `EnumStringBody`, `EnumSymbolBody`, `EnumBigIntBody`) have been replaced with a single `EnumBody` node. Its `members` property is a flat array of per-type member nodes (`EnumBooleanMember`, `EnumNumberMember`, `EnumStringMember`, `EnumBigIntMember`, `EnumDefaultedMember`). The `explicitType` property is now a string (`"boolean"`, `"number"`, `"string"`, `"symbol"`, `"bigint"`) or `null` (previously a boolean). `hasUnknownMembers` remains a boolean. Several enum validation errors are no longer reported by the parser and are instead reported by the type checker.

Library Definitions:
* Add type definition for `RegExp.escape()`

### 0.307.1

* Misc: Store exports index in saved state

### 0.307.0

Notable bug fixes:
* Support numeric literal object keys when importing/exporting object literals
* Allow type keywords like `infer` to appear as member names in qualified type access (e.g. `type S = O.infer;`).
* Allow type keywords (`readonly`, `keyof`, `infer`, etc.) as function type parameter names

Parser:
* It is now a parse error to have a `?` optional modifier in various places this does not make sense, e.g. `const x? = 1;`

Misc:
* API change: `flow check` becomes an alias of `flow status`. You can use `flow full-check` to trigger a full foreground check of a Flow root.

### 0.306.1

* Fix in Meta-internal saved state logic

### 0.306.0

New Features:
* Support for using well-known symbols as property names in types - the form should be exactly `Symbol.iterator`.

Notable bug fixes:
* Report prop-missing errors for unbound module properties in explicit type args used with implicit instantiation (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfCEoAGsoCAB3KHw4KrBMsAALfAA3KjgsXGxxZC4eAw1m-GhcWn9aY3wWZlc+-g1mbDqJbDYAWglxxJ63AxJi7AlG2lXhXFwZ9z2Do5PRHEoLgyKwTqp1zce8qChFrpIqY41VqmDA+T4gIo-MB-AGrCCMLLQbirRoYGrgor-GDYVYIA7LSgkVZkCQE7q9dxFfIrVbMdJwMjghSeBIhPwYEhkSgqeEmTIQB7bPoAX34kA0SQAOlAzIwBRIAAQAKgVHIVACVWMoFTBKOkFQByKLKA0Abml0o0Lyo2B1z0RUAVGmx1BWABkIOwAMLpOU-HwAHmlCoVPtgiGQCuAVhjouDGsEGkJkc10SsADkIBpeNKAHwACnjzpMLDYmpIECElGhkcgIj9FnzMasYfgCAAlAqWFAkyQEz2TCQc1B27XfdBG83W4hO93e-3e+aoJbFrgbQq6w2fAqAGLEfPtpcrl2lj3e8f+iQB1PKKwAWQAkgBlJ8P9MAcXwAFV0+qAKJPgA8m6ABqf4ACL4H+6rqoB6oBgAjLmvAKvgBZ7hAh4KgA9NhCrlHAZwKmU5QKiWAq5CAzSDnA0BJM0AAMVgAMwMQArFYDEgMKQA))

Library Definitions:
* Updated core.js library definitions to accept readonly tuples in Map constructor

### 0.305.1

* Fix in saved state on projects including files outside the project root.

### 0.305.0

Misc:
* Add autofix for colon-style type parameter bounds to extends-style type parameter bounds

### 0.304.0

Notable bug fixes:
* Fixed analysis of number literal keys in object destructuring rest type computation

### 0.303.0

New Features:
* Adds a new `stylex_shorthand_prop` flowconfig option that enables syntactic sugar for StyleX on lowercase (intrinsic) JSX elements. When set to a prop name (e.g. `stylex_shorthand_prop=sx`), `<div sx={[styles.foo, styles.bar]} />` desugars to `<div {...stylex.props(styles.foo, styles.bar)} />`.

### 0.302.0

Notable bug fixes:
* Fix https://github.com/meta/flow/issues/9357

Misc:
* Flow no longer errors on colon-style extends and now supports extends by default, previously gated by the ts_syntax option.
* Add format, and test commands to LLM rules

IDE:
* Fixed https://github.com/meta/flow/issues/9350

### 0.301.0

IDE:
* For `React` autoimport, it will now always be in the form of `import * as React from 'react'`

### 0.300.0

Misc:
* Minor changes to the flow-upgrade script

### 0.299.0

Parser:
* Support `export declare`, like `declare export`
* For `component` in implicit declare context, the ast we produce will include `implicitDeclare: true`

### 0.298.0

Likely to cause new Flow errors:
* Imported functions that conform to hook naming conventions will always be treated like hooks. As a result, new errors might be surfaced. e.g. conditional hook call

Parser:
* flow_parser.js now allows you to pass in `filename` in the option object

### 0.297.0

New Features:
* `declare` keyword can be omitted for `declare function` under declaration context (.flow file, declare module, declare namespace)

### 0.296.1

Parser:
* Fix parsing of experimental record expressions that are multiline

### 0.296.0

Declarations:
* The `declare` keyword can be omitted for `declare const/let/var variable: type` and `declare component` under a declaration context (libdef file, .js.flow file, declare module, declare namespace)
* `declare component` now requires the rest props to be named - matching normal `component` declarations

Misc:
* The `Omit<O, K>` utility type will no longer enforce that the provided keys `K` need to be part of `O`

### 0.295.0

New Features:
* Support 4 well-known symbols (`Symbol.iterator`, `Symbol.asyncIterator`, `Symbol.dispose`, `Symbol.asyncDispose`) in class methods. For now, the computed method name needs to be exactly in the form of `Symbol.iterator` (or other 3 names) to be recognized.

Notable bug fixes:
* Flow will now error when a function parameter appears in the default expression for the same binding. (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiSMAeyjg0AABDAABTyZDS4QiG7SgC80vkAEppcABdKAPSG6UAdzgx2lJkovKV8mlLExLEe7WlElq2GljhoImlOAqCGlGieVFKAB1YBL4VBpQg5cAMAKlcDaBrpRhdfqjSbzZbrbb0w7NCZBJCSG6PV6nL7-VBA8HcKG2BHpbkQA0TCQpVAkg0AAxWABMAE4ACxWPsgAVAA))

Misc:
* Introduced a codemod for migrating legacy utility types to modern equivalents (`yarn run flow-codemod convertLegacyUtilityTypes`)

Parser:
* Flow now parses import attributes. For now, we don't do any semantic validation yet


### 0.294.0

Parser:

* Support second argument of import expressions

Bug Fixes:

* Fixed bug in computing type hints under JSX expressions ([Example of bug fixed](https://flow.org/try/?fbclid=IwY2xjawOmd_ZleHRuA2FlbQIxMQBicmlkETFZSW90ZXdkRDI3VmVPdlgxc3J0YwZhcHBfaWQBMAABHkEUKLd3cdSRgavQGBPkpxgj0TfIrMesuc5iRdMMY6RjWZ_x7tFHT_ydJdOd_aem_8zXmYmpGc_0_68XPBGJNOw#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiSAHoAFQSgAEAAEYEQAO7SiVigA6UA1K2M0oAanAOUJuABhCDHObwqD67CK00iRjQCwAHgAKgAZOgQVIABUosJIyGlwCsIYFAD5pQBeaWQe2OnxOjXS6UAahdAEkFiJeBqwwAKJMx2pwXBsFhQQN5uBZwMZrO8aU7WiB+ymKAIACUUYjACVWMorAA5CAaHNQZPV7BiQMAEj77AA8lBBgBBah0V2ZqdhscdgDcGsPUA0Tyo2Bj0HseoNEiNuFNuFEFevhpNZrayjg0GttvSDq+CbAOGB6ao8l4SNKk7TtKc6sGwS6ruutBOsAWQSG0LZSBUCDhlG0oANoALogRqsb-hY0oAKL2isbq0hIlGKPabR5l2wCFiwt6UOOBbjsm0pOvqr73maT6QVmJCRsAUEkOGhb8UGUHdgJTDAmG0lZv41ZtAKTpiqpUBhgK8kCWKQm3m+j4iIZhb7hqAq5CADQmCQX5QEkDQAAxWAATAAHN5nkgAKQA))

### 0.293.0

Config:
* `use_mixed_in_catch_variables` option was renamed `use_unknown_in_catch_variables` because `mixed` is being renamed to `unknown`

### 0.292.0

Likely to cause new Flow errors:
* Flow will start to validate variance for declare classes.

Parser:
* Rename ClassDeclaration property 'superTypeParameters' to 'superTypeArguments'

### 0.291.0

IDE:
* Flow now provides a code action to fix all `invalid-type-cast-syntax` errors in the file.

### 0.290.0

Likely to cause new Flow errors:
* flow will now validate variance in interface declarations
* Bad cyclic object types are now banned. e.g. `type O_loop = { p: 0, __proto__: O_loop }` will cause an error.
* Flow is now stricter with generic mapped type. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiSMAeyjg0AABDAADwAFQAfAAKRjIaUABSoWW4iqVAEoNQrpcAADpQaVW6UsCRCSiWxgAbmlAHpXdKoBAAO7SkyUXkkC0Ci25EANEwkKVQJIZewmEACoA)

Notable bug fixes:
* We improved the refinement system of match. Some code that previously gets a general type will now get a refined type. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiStnsAAIYMRkBKAPz2UxQBASgC8EoA5NKIBqANwAHUe0ElmAAXrLFRUVeqMntagAKADa2oAugBKCXAQ0Sn0Sx1HXASgA+EoeGngXzYLrVAD5NdgRHDaBreN7fY7xRIpcRo6q49rU1ABYbciAGiYSHBoEkGgAGKwAJgAHABOKy1kACoA)
* allow opaque types to be used as keys to computed object properties
* autocomplete on `import { | } from 'a';` will only suggest values, not types.
* flow-dev-tools now properly detects unused suppressions

IDE:
* Flow now provide auto-import code actions for all unbound names, even through they might resolve to a global. It can be helpful when the global is not what you want.

Library Definitions:
* Add bigint functions to DataView

### 0.289.0

Likely to cause new Flow errors:
* For a poorly bounded generic indexed access types, we will no longer generate an error for it when it's being first subtyped against. Instead, we will make it opaque so that when it's being subtyped against, it will error. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiSMAeyjg0AABAt7AAeAAqAD4ABQAHSg0u10oayGlioA2gByGDEY0AXV40oA9DbpVAINKTJRedKcGaWNbHc7qG7HQB3TUASn1RtN5ot0uAtvtLrdHt5sgdTvjlBTQa10pYEiElC1AEYANyx32u9OB72pv3pxMsTUCzW5EANEwkKVQJINAAMVgATAAOADsVm7IAFQA)
* Flow will error when you are trying to redefine the following reserved types: `$NonMaybeType`, `NonNullable`, `$ReadOnly`, `Readonly`, `$Keys`, `$Values`, `Values`, `$ReadOnlyMap`, `ReadonlyMap`, `$ReadOnlySet`, `ReadonlySet`.

Notable bug fixes:
* We no longer incorrectly report conditional hook call for loops with a break. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiStWIWwABJSAEJRLYASSgAFUoLgsHhSgAxXkAGWIjAAFABKSXAAA6UElkpgvMlhraEklcElAF5JQAGADczslAB5JQBGb3OgDUodNFqt1slaHlXst1oFielZAAypYFiafQB6HOSiBbS3JqC5EANEwkODQJIZewmEACoA)
* Fixed a potential crash when evaluating a conditional type. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiSK2MAAIAHLQADycLgCNwAGlsLQSAAeAAqAD4JQBeCUAEgAatwZJrgAAdKASiUAbR2tAlFQlTogMAlWoAusgJQ0IHA2BKFAsoGwSF7HWrvRKAPwhkRw51+p28G0CnUAbht4uwEtlaAAVnMJAB1OASWoyqDy+FQbiq2javWG622h1Ol22mt1xUNlVqzW631Rp3e9NQAU5nyrCUAQSgECrJkLJeU2rXpdbxoASqw2LKoIMNe27VYL1vlBWq72FUqm6ebXa7dHna73Z6tVeJKPv8XSzfCdnwlTNJ0zLNchABoTBIfskgyewTBAAUgA)
* Flow will now allow computed keys of type `StringPrefix`/`StringSuffix` to be included in object initializers with other non-overlapping keys (example [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiSGieVGwAAJAVKdrQAMpSCoIAAKLHg8mQUqVpigao1xQAPAByLBgE0APgA3AAdKD22z2KXyKUAXilwA0MG1AAZeFKANry3Uq9WaYoAXT9AutUoA9PGpQB1ODHKVQCAAdylOClEC2dCli0WGYg+YaJlwGEYUoqcuwtBI9vtrtZnu92uEIhuAcDK2MEBgDcVyv14c10Yz1xMsYTSYLdHt4urLBlVCltG1Xs0XZnlD7A+wQ5HofHhvkU+7N1j9toUvbR5P8jjialWbTuFz0sXtBbUHkKxvQfEhpx7ExXyTVN0zzX9Wysc0QLAm5IJTT9v3zQs-wAqx5FoAAvVDoK-PNgSlExKF5bVqU8FYpRNPD8JNKUijZFVchASt8TgaAkgaX0rAAJgADmEqxfRAAUgA))

Library Definitions:
* Update React API types from recent releases
Added
- `act`: 19.0: https://react.dev/reference/react/act
- `captureOwnerStack`: 19.1: See https://react.dev/reference/react/captureOwnerStack
- `useEffectEvent`: 19.2: https://react.dev/reference/react/useEffectEvent
- `<Activity />`: 19.2: See https://react.dev/reference/react/Activity
Updated
- `useDeferredValue`: 19.0: Accepts a second argument for initial value
- `useTransition`: 19.0: Accepts an async action function
- `startTransition`: 19.0: Accepts an async action function

### 0.288.0

Likely to cause new Flow errors:
* [`React.ElementProps`](https://flow.org/en/docs/react/types/#toc-react-elementprops) is removed. You should use [`React.ElementConfig`](https://flow.org/en/docs/react/types/#toc-react-elementconfig) instead.
* For a function named as a hook but not annotated as a hook, it will no longer be treated like a hook when you try to use it from property access or property destructuring in component syntax components and hook syntax hooks. They will now cause `react-rule-hook-non-hook-syntax` errors. To fix, annotate them with hook annotation. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiStnsAAIAKpZHgAIXYEoAvBLgAAdKASiWUgCytAAEsQtsgJQAKACUyoAfKqBRq7VANZARIxoBYJQAVdoSACMpqsAeAuoNRoFJuA2rIesNEGNZstSptwAFEoFlvVmq10tlJAVbCswZjWwtAG4JQB6csSkyUXkarOR7DRo2litVmt1zNalgSISUKAl+0a2pGxte+x+oNRkOxsOqxvN2Mmi3W22p9P1rMyuDy9gF6dF1uV6vUTsNwst81l48dyib7u7PsDoeO18j2MLmfF9MO8USbM7iQADixBsMqqqbhesYvk66Sul8Piet6ABM-qBlBWxzhGGEmu+WzxquybrhBXYATwIEQPmGFHlW0GkdRV5tietZ3qRPZPoOUAOnhY4oaaU5Nl+WGfkWuGjiuiZrmmJENtu5GgfugmHoxx50eeB6XtetFbPeErsf2nEOrkIANCYJBwNASQZPYJggAKQA)
* Calling functions passed to function and component with hook like name but not annotated as hook will now error. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiSkBEjGgFgABAAxYgAIXYAApKQBZWgACWIW2Q0uVAEppQBeAB80oASqxlFYAHIQDRG4AAHSg0vd0pYEiElDd6q1OsNAG5pQB6UPSkyUXmugWu13iyVfHxy4gAcWIbFVZA12oguultR1+qNZst1pi9sd0pdbo9Xp9fpzAfzwdjrtyIAaJhIcGgSQy9hMIAFQA).
* `React$RefSetter<...>` and and `React$Context` are removed. It continues to be available in the form of `React.RefSetter<...>` and `React.Context<...>`, regardless of whether you have imported `React`.
* `React$ElementRef<...>` was removed. It continues to be available in the form of `React.ElementRef<...>`, regardless of whether you have imported `React`.

Breaking Changes:
* The config option `max_literal_length` was removed. Flow will always decide whether to infer a specific or general  type of string literal based on the [natural inference algorithm](https://medium.com/flow-type/flow-natural-inference-for-primitives-df27149109bb).

Notable bug fixes:
* You can now use `@@dispose` and `@@asyncDispose` in your library typings. You can access the respective properties with `obj[Symbol.dispose]` and `obj[Symbol.asyncDispose]`.
* Allow comparisons of bigint singletons

Library Definitions:
* The second type argument of `ExactReactElement` will be populated by `React.ElementConfig<C>` instead of `React.ElementProps<C>`. Most of the code that doesn't depend on exact react element types won't be affected.

### 0.287.0

Breaking Changes:
*  We will stop publishing new versions of flow-parser-bin. If you want to parse flow code with native-like performance, you should use `hermes-parser` package, which is running on wasm.

Likely to cause new Flow errors:
* The deprecated `React$Element` type is now removed. You should use `React.Node` or `React.MixedElement` in most cases. If you really want exact react elements, you can use `ExactReactElement_DEPRECATED`

New Features:
* Flow has full React 19 ref-as-prop support. Since this version, you are allowed to spread ref prop in component syntax components.

IDE:
* We now provide a rewrite refactor to convert arrays, map, set, and object types into their readonly forms.

### 0.286.0

Breaking Changes:
* Starting from this version, `flow-bin` will only contain ARM64 binaries for macOS.

Likely to cause new Flow errors:
* Flow now errors on constant conditions and invalid comparisons. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiSGieVGwAAIJWypQARCAIKXAAUAbgAOo9oPYpWhiLgpQBeKVSGSa7V2CRSmDEY1Sr4AdwVSoAFABKC1wGBS136iC4d0qgVSgD0oalJkovK1tl1aHt-sNRpTNuIarDEajMaguRADRMJDg0CSGXsJhAAqAA) (D83014262 samzhou19815)
* Flow will no longer infer an unsound type for array and object literals passed to a general function. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiSGieVGwAAIYA9lHBoFK4GwADwAFQAfAAKBrIKVqgCUerVAG4ADpQC22exSiBoABWUoAvEq2FrgDBiHqAMwCg3mqB2x2sqUer1S+ymKAIKUAHylwhENwFJqlAHo0wmIFKTJReVKcJ6WLwc9R84GAO4Wi25EANEwkBVQJIZewmEACoA)
* `React.ElementConfig<...>` and `React.PropsOf<...>` will now always include the ref prop in the props object type.
* The config option for an earlier mode `react.ref_as_prop=partial_support` is removed, and Flow now has better support for React 19's ref-as-prop behavior:
  1. All the ref prop in component syntax components must be made optional
  2. You might need to rewrite some of your higher order components. e.g. previously an identity HOC must be written as
```
declare function id<Props: {...}, Ref>(c: component(ref?: Ref, ...Props)): component(ref?: Ref...Props)
```
now it must be written as
```
declare function id<Props: {...}>(c: component(...Props)): component(...Props)
```

### 0.285.0

Library Definitions:
* Fix type of `useActionState` hook in React. Type should be `Awaited` instead of `Promise` (the opposite operation).
* Added types for `Object.groupBy` and `Map.groupBy`
  - https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Object/groupBy
  - https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Map/groupBy

Bug fixes:
* Fixed a bug that when `experimental.ts_utility_syntax=true`, imported `Readonly`, `ReadonlyArray`, `NonNullable`, `ReadonlyMap`, `ReadonlySet` types are considered any.

### 0.284.0

Likely to cause new Flow errors:
* Flow now strictly enforces that Array.includes, .indexOf, and .lastIndexOf take only values that could potentially be present in the array -- it is now an error to check if arrays include values whose types are unrelated to the array's element type. This catches many cases where developers accidentally test the wrong thing, which may lead to errors at runtime.
* Flow will infer general types for react jsx elements. The new behavior is described [in the docs](https://flow.org/en/docs/types/const-expression/#toc-const-expression-jsx).

### 0.283.0

Likely to cause new Flow errors:
* Invalid suppressions like `// $FlowFixMe[incompatible` will now cause an error on the suppression

### 0.282.0

Likely to cause new Flow errors:
* Multiple object property invariant subtyping errors on the same object will be combined into one error. Some error locations might be moved.

Notable bug fixes:
* Fixes crash happening on exported functions with signatures of the form `<X>(x: X, y: typeof x): T`

IDE:
* We now provide a best effort quickfix for some invariant subtyping errors.

Library Definitions:
* `Array.find` can now return a more precise value if the predicate is inferred to be a type guard. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiSGieVGwAAIJWypQBBKBmbhS4ACgA6UHFuElMp1coAIhAEFKFAstSQFUqMrhVRqtXMdSwZdB7FKqJRkArqHQADyK5W4AB8AG5NbY3WxjQB5OFSgC87uoVngWoAFA0E0GpZmKvZgZCIDApUaEABKMNQAD0ValdfrUoAelKAAosBpwCD5QZSgAGAZtUoAPjmIHA2L3eFKoBAAO590vD0fj3ua3IgBomEidqBJDL2EwgAVAA)

### 0.281.0

Breaking Changes:
* Support for the `casting_syntax=colon` is removed. This means that the [as casting syntax](https://flow.org/en/docs/types/casting/#toc-type-cast-expression-syntax) is always enabled.
* `experimental.error_code_migration` config option is removed. In the previously version, the only allowed value was `new`.

Likely to cause new Flow errors:
* We have fixed more source of object literal unsoundness. More errors will be exposed. ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiStnsAAIYMQJQBeCXATCUZASgDkqoFAG4ADqwGWshVKlX2UxQBASgA+EuEIhuWolAHoHRLZg04BB8oNrRAJSZKLzeN6AO6+6i88T8BomEjuqBJDL2EwgAVAA))
* For most of the common invariant subtyping errors (arrays, objects with mutable fields), we will now emit a single subtyping error with explanation how to fix. Error locations might change. See https://flow.org/en/docs/lang/variance/#toc-invariance to learn more about variance of types
* Suppressions without error code will now be an error instead of a warning.
* Suppressions without error code will no longer apply. You can download and use [this script](https://github.com/meta/flow/blob/7551ae4431f17b0ed93657ed7b3c73d7e8654486/scripts/add-specific-error-code.js#L12) to automatically migrate most of the suppressions.
* Only `$FlowFixMe` and `$FlowExpectedError` are supported suppression comments. Existing `$FlowIgnore` and `$FlowIssue` will no longer suppress anything.

New Features:
* Added a Flow lint `require-explicit-enum-checks` which works like `require-explicit-enum-switch-cases` but for `match`.

Notable bug fixes:
* Various codemods will now insert parentheses around expressions in class extends if they are changed. Previously, it might result in invalid code like `class Foo extends myExpr() as Bar`
* Fixes an inconsistency in recording the this-type guard on a regular (non-`declare`) method in the same vs different files.

IDE:
* Added a quickfix to convert type to value imports on type-as-value errors.

### 0.280.0

Likely to cause new Flow errors:
* Accessing missing exports on namespaced import will now trigger `missing-export` error instead of `prop-missing` error. ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiSZkYvIkAAIAFRS1lSgBKrGUUpglHSUoA5FFlFqANwAHSgyuiVnw+AAygBRADCiutABV8ABJAByjutirdAEEADKW-AAEQA8vg3SHnQBVG34EOK-AATRDUfwAHUXX6-fgAELW-AAMRdDqD+qlAHpy1KimyKghFgzJVKTBrKLwpbMGnAIPlBh2NYxFjWSHXm9RecbciAGiYR9Akhl7CYQAKgA))
* The only supported `suppress_type` `$FlowFixMe` is now just a type alias of `any`. For most of the code, there will be no functional differences. However, you might see new errors if you have any local definitions of `$FlowFixMe`, or you used the undocumented `$FlowFixMe<arbitrary type arguments>`.
* Support for `suppress_type` config has been removed. The only supported `supress_type` now is `$FlowFixMe`. If you want other variants, you can add
```
type MySuppressType = any
```
in your global library definitions.
* Many subtyping type errors' error codes have been standardized into `incompatible-type`, so some previously suppressed errors will reappear until you change the suppression error code into `incompatible-type`. The change was announced in the previous version, with option to enable it via `experimental.error_code_migration=new`. Now the only valid option to `experimental.error_code_migration` is `new`. You can run `flow codemod error-code-migration --write .` with the previous version of Flow to help migrate, since the codemod is removed in this version.

### 0.279.0

New Features:
* In the next release (0.280.0) of Flow, we intend to standardize the error code for various subtyping errors into `incompatible-type`. You can add `experimental.error_code_migration=new` in your flowconfig to enable the new behavior now. We also provide a codemod `flow codemod error-code-migration --write .` that you can run over your codebase to automatically change the error code. Both the flowconfig option and the codemod will be removed in the next version.

Notable bug fixes:
* Improved precision of error messages when inferred primitive types are checked against other incompatible primitive types (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiStnsAAIoBKALwSgCMAG4ADqwB7KODQCWYSgACnkyAlACYAJQS4AClVQHW6qAmxUSgD0jolJkovIlAHc4McpRBPRLvPDpRJapkpdcTBLcHA8dwJQADeUJiW01OPdKOLJoNpe2O1CMiG7R2Pc3CJo0J3IgBomEiaqBJBoABisRoA7AAOKzNkACoA))

Misc:
* Thanks @JamBalaya56562 for fixing various typos across the codebase!

### 0.278.0

Likely to cause new Flow errors:
* Hooks calls inside normal functions in component or hooks as conditional calls. They will get `react-rule-hook-conditional` error instead of `react-rule-hook` error.
* Array literals that cannot be contextually typed will be inferred as an actual array type. It might cause additional errors. [example](https://flow.org/try/?fbclid=IwY2xjawLuPH5leHRuA2FlbQIxMQBicmlkETFpd1VmMlphSWVjQlIxOHhmAR6d0gQ9lDWShsIz_pxY8yeHKIg3er7dGn5hu3VMeSyhg54__AqVwQKsxcgLsg_aem_FQMAAHw5tPKK9ybGNCdifQ#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiStnsAAIqJQJQBeCUAbQAOiAYMQVVKSBLxRIALoSgD0BolAHc4McJVAICaJbUMA1sBKVsYJQBBah0AA8KrVEBVAD4lVBpZqJQASABKrDYAHkoIN3TRaJ77KYoAh-QBuQ3GiBbIMh1mK1MVBC67NGiUmSi8gvUUOJr0l9NZnMS2YNOAQfKDCV53iW61V6i1qC5EAO-FdsfIEAZewmEACoA)

### 0.277.1

Notable bug fixes:
* Fixed windows builds.

### 0.277.0

Likely to cause new Flow errors:
* Array literals that cannot be contextually typed can no longer be passed to mutable tuples with a more general element type. It might cause additional errors. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiStnsAAIqJQJQBeCUAbQAOiAYMQVVKSBLxRIALoSgD0BolAHc4McJVAICaJbUMA1sBKVsYJQBBah0AA8KrVEBVAD4lVBpZqJQASABKrDYAHkoIN3TRaJ77KYoAh-QBuQ3GiBbIMh1mK1MVBC67NGiWzBpwCD5QYSvO8S3WiUmSi8yt5gvUUOJr0l9NZnMS+zm3AS9F1qBsBvdqC5EAO-G1hfIEAZewmEACoA)

### 0.276.0

Likely to cause new Flow errors:
* Hook calls inside anonymous functions bound to a variable will get `react-rule-hook-definitely-not-in-component-or-hook` error, if the variable name doesn't conform to hook naming convention. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiSZkYvIkAAIAEqsZRSmCUdJSgDkUWUqoA3AAdKB62z2RXEKUAXilAAoAJRmgB8UuAeqlztl8pilIAypYFtatVKAPT+qWzBpwCD5QZSjUSRbSNqLWrELZSkzKyi8KVQCAAdyjbtj2ixiYgW0WGngUDgC0GhwgMYqSPSEq+PhhlATSZT1F5eoFetyIAaJhIYagSQaAAYrAAmADsABYZyABUA)
*

IDE:
* Added support for workspace symbol feature

Misc:
* Thanks @jbroma for improving `as` casts with `function` generics and `component` generics!

### 0.275.0

Likely to cause new Flow errors:

* For all object literals in positions that cannot be contextually typed, we will infer a stricter type for them. It will cause new errors in code like

```
const foo = {baz: new Dog()};
type Foo = {bar?: string, baz: Animal};
declare function acceptFoo(foo: Foo): void;
acceptFoo(foo); // error
```

To fix the error, you can either annotate the object

```
const foo: Foo = {baz: new Dog()};

type Foo = {bar?: string, baz: Animal};
declare function acceptFoo(foo: Foo): void;

acceptFoo(foo);
```

or make the call site accepts readonly objects:

```
const foo = {baz: new Dog()};

type Foo = $ReadOnly<{bar?: string, baz: Animal}>;
declare function acceptFoo(foo: Foo): void;

acceptFoo(foo);
```

We provide a codemod to automate the annotation process. `flow codemod annotate-literal-declaration --write --max-type-size 5`. (You can adjust the max type size based on your needs).

IDE:
* Support rename on private properties and methods.

Library Definitions:
* `React$MixedElement` is removed from builtin libdef. It will cause `internal-type` error since `v0.258.0`. You should use `React.MixedElement` instead.


### 0.274.2
* Bug fixes for `match`

### 0.274.1

New Features:
* Support for experimental `match` feature using option `experimental.pattern_matching=true`

### 0.274.0

Likely to cause new Flow errors:
* Unannotated object literals reachable from exports will now be inferred to have all mutable fields when being imported. Previously, it has unsound types, so new errors might appear.
* When comparing two object types whose properties have variance incompatibilities, Flow will raise a single error that will summarize the properties with incompatible variances, instead of a single error for each property. (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiSK2MAAIACoARglAF4JcAANpbCUVCUAchgMs1AB9tQAmfXagDMJpgABYLQBWTUAXWQEuEIhuAoA3AAdKDejRPKjYCWAiXyGVOgAkACVWGwAPJQQYAHllAD4vVBQxLWdKZe6JQB6fMSgCqZDYErSEpotMDNolJkovJIvGdEAA7hK23BjlWUWQJXKG7zciAGiYSHBoEkGgAGKyGgDspqsMpAAqAA))
* When an object with extra properties is passed to a place that expect an exact object, we will now generate a single error with all extra properties. The error message will list the extra properties, and state that "Exact objects do not accept extra props". In rare cases, the error locations might be moved.
* Flow will error more consistently with sketchy-bool on nullable boolean types (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxwzZ292oPhERxKU7yoMDaqZdXJ9yEoKDn2kioDipNUwYHzvAyfb5gX7-RYQRhZaDcRZ1DAVMEPP4wbCLBC7BaUEiLMgSfEdLofYRkNiLZjpOBkdEeEwJEJ+DAkMiUFRwkyZCB3dbdAC+-EgGiSAB0JBLYJ9lHBoAACEgVBBtNJQABCxGeUAAYhsABTyZCKo64RUAH0V5OwVsVAEFqHQADxFEpsAB8AEpFcAZYrFXAYIrjb7gELFQB6KOKgByAFEAOqmgDucGOZogqcVCYASnmAPJ5xXpiS1ZXnfaKtA61hQGVCmUymByhFQZWq9XQOOiAAMxtN5vtfftTpotDdxVKPr9AaDIbDfsjMdzBeLld21eujebsse7c7UDVu2g2oguoAKvpB2btBbrVIZGPnZP3TPw-Pg6H5OGV7G40LS9NwuWhdwbfd5SVFVj27KBexEABGW9h2tRCXwnKcPVnf0OwXH8-2jACgJA-ZG1yEAGhMFVoCSBo+ysAAmAB2RimJAIUgA))

Library Definitions:
* All properties in the builtin `PropertyDescriptor` type are marked as readonly. If you need a mutable version, you can introduce something like `type MutablePropertyDescriptor<T> = {...$Exact<PropertyDescriptor<T>>, ...}`

### 0.273.1

Notable bug fixes:
* Fixed windows builds.
* Fixed crash when loading saved state.

### 0.273.0

Likely to cause new Flow errors:
* We are announcing Natural Inference for Flow, an improved way to infer types for primitive values, that resolves a long-standing correctness gap and source of confusion. See more in [this post](https://medium.com/flow-type/flow-natural-inference-for-primitives-df27149109bb).
* Added `nested-hook` and `nested-component` lint errors which detect nested hook or component syntax within component or hook syntax. This is on by default.

Notable bug fixes:
* For default imports, the autoimport ranking will now consider the names of the importing side. (e.g. Previously we completely ignored the name of `foo` in `import foo from './bar'`, but now we will count foo. If the pattern of `import foo from './bar'` happens a lot, then the autoimport algorithm will be more likely to suggest `import foo from './bar'` rather than `import bar from './bar'`).
* Flow will infer a correct type when viewing the type of an object literal as a dictionary type. For example, the error in [this try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkUEKTwJEJ+DAkMiUFSwkyZCC3dbdAC+-EgGiSGieVGwAAIYA9lHBoFKcRIANLYWgkAA8ABU1bQAHwACnkyClwAA2jtaKbdeqALqmoolNgCgCUpoAgtQ6Dq9fqANwAHSgwds9il8ilAF4lbs9SRDcAMKaAOQYFO8KVoVNoFNuoMhqBwGCGqQyV1m4NSiPmgAMdujUpTYBT-qlAHp21KAOqsqVQCASKXAqUmSi8rOaXmyKUAdzgx37EFnk+HUFH1F5wYFweD8jrDb7aZTAB8U7m252pRAtnRJzBp2u2MOYHjd1BciAGiYSAqP8gv1rKwACYAHZgJAkABSAA) will be raised.
* Previously, we undercounted some imports during indexing, which causes autoimport ranking to behave incorrectly. The issue is now fixed.
* Flow will no longer emit `react-rule-hook-conditional` error for hooks called in a conditionally defined nested component.

### 0.272.2

Notable bug fixes:
* Prevent non-termination when computing code actions.

### 0.272.1

Notable bug fixes:
* Fixed https://flow.org/try. It was broken from v0.270.0 to v0.272.0.
* Documentation in hover now preserves indentations. Therefore, code blocks in jsdoc will no longer lose indentation.

### 0.272.0

Likely to cause new Flow errors:
* When component syntax is enabled, hook calls happen inside an upper case function that doesn't have a `props` param that's a subtype of `interface {}` will get `react-rule-hook-definitely-not-in-component-or-hook` error.
* Calling a function that's a union of hook and non-hook will now get `react-rule-hook-mixed-with-non-hook` error instead of `react-rule-hook` error. Calling functions named like hook but not in hook syntax will now get `react-rule-hook-non-hook-syntax` instead of `react-rule-hook` error.

Notable bug fixes:
* Go-to-definition on default import of `module.exports` will correctly jump to the exporting file

Parser:
* Fix crash on `''#!/usr/bin/env node\n''` when generating token list

Library Definitions:
* [`React.forwardRef`](https://react.dev/reference/react/forwardRef) is marked as deprecated. We might remove it from our builtin libdef in the future.

### 0.271.0

Notable bug fixes:
* Multiple levels of `export *` will now be correctly indexed so that they won't be missing in autoimport results.

Library Definitions
* Add `Float16Array` type

### 0.270.0

Likely to cause new Flow errors:
* When component syntax support is enabled, upper case functions with component-like name but doesn't return `React.Node` will no longer be treated like components. Thus, all the hooks call in such functions will have `react-rule-hook-definitely-not-in-component-or-hook` errors.
* Usage of `Object.assign` will now trigger `unsafe-object-assign` lint error that's on by default. The type checking behavior for `Object.assign` will otherwise stay the same for now.
* When Flow decides that the hook call definitely doesn't happen within component or hook body, it will emit errors with code `react-rule-hook-definitely-not-in-component-or-hook`.

Parser:
* Parse the TS nonnull assertion operator.

### 0.269.1

It should have the same behavior as 0.269.0.

### 0.269.0

Likely to cause new Flow errors:
* Using `$Omit` will trigger an `internal-type` error.
* There might be some error code differences around destructuring
* `Object.assign` now has stricter behavior over maybe type and optional type inputs. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SRWxgABABBT4QSzwuzsgC87IAClQstwADzAAA6UHZ7PspigCGQ7IA-MAANpKioIAC6avVupVvHZVgtjLlCoqEiN2pNBqNwhENzNFqsVqg9IAfABuOWB2APZRwaDs9hsblQXmZMN2AAU1vZlkoOLtXJ5fPjJF4yZIECElEhaujsf5ublAEo1Q0IHA2OzZfL2QB5NAAKzmMR2tBICYLRchVasMAglAAokpagngTHs9AACqrAByzmwQp9TeTCtT6a1c-L8eXxjX5n1Qrbne7VgwJBIiCgCfZAHoX+yvgB3dkmSjjncKimVD7oeC5QCe2Bntg+p5i2gGKoWxbYAeWZxkuq7rjBAFVgGLb0jhcr0nKuQgA0JgPtASQZPYJggPSQA)

New Features:
* You can now use `this is T` as type-guard annotation on a method `m`, to refine `x` when calling `if (x.m()) {}`. This-type guards can only be used on declare classes and interfaces. (E.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABOySCROQBBTnAAA6UE5nNpACEABQASmQnIktVpEr5koA3KL6aLRWzcBzufreZzJZyFAsoGw+YLgNqoLq5vqWJzAZyMAr+ZqoHAYJzpRgrFK5bKhaLxRg3Wr1ZyAPQxzkQLZ0LVm7qhsVuyMC6NxhNJ2hhzMYKOx+MmSgQSgCiWPdKOLJoNqcgDucCVJq1uRADRMJDg0CSDQADFYAEwANgAHFYhyB6UA))
* Added support for `const` type parameters that allow you to specify that an argument to a function is always treated as a const-expression, by annotating the type parameter with the `const` modifier. This eliminates the need to use the `as const` syntax when calling functions with this annotation. See more in [our docs](https://flow.org/en/docs/types/const-expression/#const-type-parameters).

Notable bug fixes:
* Auto-import for modules from `module.system.node.root_relative_dirname` will have correct import path instead of relative import path.
* Auto-import for modules from `@flowtyped` will have correct import path instead of relative import path.
* Generic render types are now allowed everywhere.
* Fixed potential issue that makes hover on `Omit<...>` to fail.

Parser:
* Parse class static blocks. The feature is not yet supported for type-checking.

### 0.268.0

Breaking:
* The Linux x86 build is now built from `ubuntu-22.04`. It might make Flow not runnable on older linux distributions.

Likely to cause new Flow errors:
* Code like the ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SVqxC2AAJKQAxYgACgAlBzgAAdKAcjkAISoAH4rJTpZRBQBuMX0sVitkQTkKqiC4XqqBijRPKjYDm2exSqjIDky4C6yi2lbGCAwLlkRWG3IgBomEhwaBJBoABisACYAGwAdisIZA9KAA)) will have `[react-rule-hook-conditional]` instead of `[react-rule-hook-naming-convention]` errors.
* `$Diff` support is removed. If you have trouble migrating, you can try to polyfill it by [this](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SRWxgABAASAAicBgMAAPAB5ZDsip4riQ9nARnsgDSos5cuwtBIAs5ADEiAB3DXFACy2AAfEb2QBedkAHSg7PZQvZCgWUDYJDFPhMkuwMvZAH52QAFbJbYW8dkAUXkTyEGnVytVwqNoblJvZoqFAG5rbkQA0TCQ4NAkg0AAxWABMADYAOxWYsgelAA). However, `$Diff` has surprising behavior with regard to optional props in the second type parameter, which cannot be easily polyfilled ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SRWxgABAAxYgARnZAF52QASAAicBgMAAPMAYMRkOz7KYoAhGeyZcQAPzyxUVFUAPgA3OyAPTGtWyiBahVSXX0gA6PlWXOIACYBcKxRLpRarTrlar1ZbtTb-YaTWbA76QyreOyLezqYx2VAIBJ2WxKLDjGwAIQO3IgBomEhwaBJBoABisLoAbAB2KwVkD0oA)).

New Features:
* We now allow you to configure certain error code to be unsuppressable. For example, to make `react-rule-hook-naming-convention` and `react-rule-hook-conditional` errors unsuppressable, you can add the following to the `[options]` section in flowconfig:
```
unsuppressable_error_codes=react-rule-hook-naming-convention
unsuppressable_error_codes=react-rule-hook-conditional
```

### 0.267.0

Likely to cause new Flow errors:
* We have updated the way type parameters are instantiated in generic calls, specifically when using upper bounds:
  - We will no longer infer synthetic intersection types.
  - If multiple upper bounds are available, we pick the smallest type based on subtyping ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABDAHso4NBOYwIIMADwAFQAfAAKGAARmQnKlYoAlJyALwSzkNCBwNi8bkAJgVStVGq1OrYyoVYoA3AAdKBs3AcznskgkTkAQSgZm4nOA9IdTpdbo9ABEIAhOQoFo6Pd7fbh-YHHtB7JyWCRtBJ1YLhbQpVKGgqExlcKbNQH9UWFRGEBXk8qHbkQA0TCR+VAkhl7CYQPSgA)).
* Support for `$Rest` is removed. `Omit` should be used instead. If you still have many instances of `$Rest`, you can replace them with `$Diff` as a temporary measure, but note that we intend to eventually remove `$Diff` as well.

* React-rule hook errors related to conditional hook calls will now have `react-rule-hook-conditional` error code.
* React-rule hook errors related to naming convention issues will now have `react-rule-hook-naming-convention` error code.

Notable bug fixes:
* We are rolling out the initial phase of a fix to a fundamental soundness issue related to primitive literal type inference. This unsoundness has allowed invalid code like: `const x = 'a'; 'b' as typeof x as 'a';` ([try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SVs9gABBh2QBedkAcgwfIA3AAdR7QDloZD8wU8-loPmckjslbGCAwTmiqC5EANEwkODQJINAAMVgATAA2S1WACMIHpQA)) to type check without errors. With this fix, Flow will infer [singleton literal types](https://flow.org/en/docs/types/literals/) for primitive literals in contexts where such precision is required. Examples of this are: const-declarations (e.g. in `const x = 42` will infer the type `42` for `x`), annotation positions (e.g. `typeof x` is equivalent to the type `42`), conditionals (e.g. in `if (x.tag === 42) {}` Flow will infer the type `42` for the value `42`, instead of `number`). In this part of the rollout, whenever this precision is not required Flow will infer the unsound type it used to infer before (a hybrid between the singleton and general type). Eliminating this unsound type completely will be done soon.
* `flow-remove-types` now handles the removal of empty imports after removing type/typeof imports (thanks @jbroma)

### 0.266.1

* Fix a bug that causes fixed `libdef-override` error to stick around after an incremental recheck.

### 0.266.0

Likely to cause new Flow errors:
* Support for `$PropertyType` and `$ElementType` has been removed. Now referencing these types will just resolve to a global type if you have your own definition for them, and result in a `cannot-resolve-name` error otherwise. These types have been replaced by index access types for a long time. You can migrate to index access types by enabling use-indexed-access-type from https://www.npmjs.com/package/eslint-plugin-fb-flow and running the quickfixes. If you are unable to migrate, you can add the following to your global libdefs:
```
type $PropertyType<T, K> = T[K];
type $ElementType<T, K> = T[K];
```
* Now given the subtyping check `component()<: component(ref?: ref_prop)`, Flow will ensure that `ref_prop` is a subtype of `void` instead of a subtype of `React.RefSetter<void>`.
* `React$ComponentType`, which was previously given `[internal-type]` error on every usage, is now removed.
* `React.ComponentType<Props>` is now only an alias of `component(...Props)`, instead of some special cased types. This comes with stricter checks and conversions, such as making Props readonly, erroring on the presence the ref prop instead of silently ignoring them, and ensures that `Props` is a subtype of `{...}`. In addition, the `React$AbstractComponent` type is removed.

Notable bug fixes:
* fixed a subtle unsoundness in the inference of computed-property dictionary object creation (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABIDOfJkJz7KYoAgANwAHSgbNwHO5VE5tH5goqoolEts9k5EDQACsAIz84AAbSVwoAuvzdfTOQBeTnACWczmG+Tmzm63gOp20V0AZl4nIA9AHOQB3ODHTkmSgQSgS+ki3IgBomEhwaBJBoABisACYAGwAVisPpA9KAA))

Library Definitions:
* `React.lazy` and `React.memo` is now generic over the presence or absence of ref prop.

### 0.265.3

* Make `@flowtyped` resolution support work on Windows.

### 0.265.2

Notable bug fixes:
* We fixed a bug that caused suppressions to not apply for `libdef-override` errors.
* We fixed a bug that caused `libdef-override` errors not being raised even if it's turned on.

### 0.265.1

Notable bug fixes:
* Make `libdef-override=off` in `[lints]` section actually turn off all `libdef-override` errors.

### 0.265.0

Likely to cause new Flow errors:
* Overriding already defined names and modules in library definitions will now error with code `[libdef-override]`. The error cannot be suppressed without specific error code like `$FlowFixMe[libdef-override]`. It can be turned off by turning off the `libdef-override` lint.

New Features:
* The `Number` static methods `Number.isFinite`, `Number.isInteger`, `Number.isNaN`, and `Number.isSafeInteger` now apply a refinement that their input is a `number`. Note that the top level `isNaN` and `isFinite` functions (not off of `Number`) do not apply the same refinement as they first coerce their input to number.

Notable bug fixes:
* Fix a potential crash in libdef files with illegal import export. Thanks @techieshark for the repro.

### 0.264.0

Misc:
 * Version skipped.

### 0.263.0

Likely to cause new Flow errors:
* Signature verification errors will now show up for libdef files

New Features:
* Declaration merging for `declare namespace` is now supported in toplevel library definitions.

Notable bug fixes:
* In component type annotation, the ref prop can now have any type.

Library Definitions:
* Since the last version, most of the bundled libdefs will no longer be maintained and shipped with Flow. Going forward, they should be downloaded from flow-typed. Starting from this version, we will also no longer ship a set of precise typing definition for jsx intrinsics. To maintain the same behavior as before, you should have a `flow-typed.config.json` in the root of your project with the following content:
```
{
  "env": ["node", "dom", "bom", "intl", "cssom", "indexeddb", "serviceworkers", "webassembly", "jsx"]
}
```

### 0.262.0

Likely to cause new Flow errors:
* Referencing `React$Component` directly will now be an error.
* Use of `React$ComponentType`, `React$Context` and `React$RefSetter` will now trigger `internal-type` errors.

New Features:
* Added types for JSX intrinsics, which will cause new errors and show autocomplete for DOM intrinsics like `div`

Notable bug fixes:
* Replace incorrect definition for `ClientRect` and `ClientRectList` with aliases to `DOMRect` and `DOMRectList`
This fixes the type definitions for the DOM APIs in Flow, but replacing an nonexistent globals `ClientRect` and `ClientRectList` types with just an alias to `DOMRect` and `DOMRectList`.
* fixed a bug that caused spurious errors on rest array assignments (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABIDOVRKMhOQBtYQiG4AH3spigCAAugBuAA6UEFVlVfJlnIAvEKAIzyzkAegNnIA7nBjpyoBAJJycLyoJyTJQIJR7bROSIXdglbkQA0TCQ4NAkg0AAxWABMADYdZGQPSgA)).
* When a library definition file has changed, Flow will no longer just stop the server. Instead, Flow will properly recheck everything, even under lazy mode.
* We now allow something of type `React.ComponentType<{}>` to be a subtype of `component()`

IDE:
* Component syntax components will now show up in document symbols

Library Definitions:
* Most of the bundled libdefs will no longer be maintained and shipped with Flow. Going forward, they should be downloaded from flow-typed. To maintain the same behavior as before, you should have a `flow-typed.config.json` in the root of your project with the following content:
```
{
  "env": ["node", "dom", "bom", "intl", "cssom", "indexeddb", "serviceworkers", "webassembly"]
}
```

### 0.261.2

Notable bug fixes:
* Fixed a bug that causes incorrect updates to our index that tracks usage of exports, which leads to incorrect autoimport-ranked-by-usage results.

### 0.261.1

Notable bug fixes:
* Fixed a crash after initialization with saved state if the typing for `react` module is provided as a userland module.

### 0.261.0

New Features:
* Bigint values inside of template literals now coerce to strings, e.g. ``` `${1n}` ```. Addition with string (e.g. `1n + 'blah'`) is still banned.
* Add ability to resolve node package entrypoints/exports
* Flow now allows inference of type guards for simple expression arrow functions with one parameter. For example, in `["a", null].filter(s => s != null)` it will automatically infer that the arrow function encodes a `x is string` type guard and will apply it on the input array resulting in a`Array<string>`. ([try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQAOhJWY9oPYAATyHkAXh5AG1WckxbweUdcABdKzwHQmAAUJEFAD4eaqAIRC6UASgA3Jz+RhVQBBah0AA89lMUAQaoNPIA9M6eQB3ODHKUQd08nA8iBbOic3IgBomEhwaBJBoABisACYAKwATisAEYQPSgA))
* `module.system.node.root_relative_dirname` will allow conditional mapping like `module.system.node.root_relative_dirname='<PROJECT_ROOT>/foo' -> 'bar'`. Under such config, `import 'a'` will only be resolved to `<PROJECT_ROOT>/bar/a` if `import 'a'` is in a file in the `<PROJECT_ROOT>/foo` directory. This feature will be helpful if you want to combine two flow roots with different `module.system.node.root_relative_dirname` config.

Likely to cause new Flow errors:
* When an opaque type is refined to be not null/undefined, we now refine the opaque type's upper bound rather than returning the opaque type unmodified.

Notable bug fixes:
* Flow will error more consistently on incompatible uses of refined string, number and boolean literal types (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQAOrAHso4NAAAQLewARgAFBlaDgAMpSZA8gD8AHIMHKAJQ84Dsnk8uAwHkiugSqQ8gCEAF4eUdcCq1VANRqFXKeRgSHzVhBtaL9ZQANw8gD0Pp5EC26ptfp57uwksoPNNwdtaHtjudxldYb1Eak3tDAHc4MczRAszyTJQIFHi6WeXL4zyAH4APkriuD9PZLag7JgnPh1v5EgATEKMNK7bweWhh-HLcG2hIefJow7ZTKx572cGhUKVcaG1abTySDm9rUdfIp9abdUyGPkLGbfPTXKwHLM-6AOq53D5wvlqMkCqQvlamwTVYGLUok2AmAK3nHM8xwRt7QAH0reNbw1NAoi2Vdzw1NtcKVTdsODedEztHlkKrZ9W1yEAGhMP9oCSBoAAYrD7ABWABOKwBRAekgA))
* Fixed some cases of type filtering during type guard refinement (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQAOhJWVANE8qNgAAQwB7KODQPm0gCCEEoACEABTyZB8gDkGCVAB8lWh1UqwEqAJSK+RikjK1V8tXKrUAbk5nO5uF5fMBfIVpu1uptUDgMD5solUrl8j1er5wE5fJdfIwJpVSqtfIA9Am+QB1ODHPlQCAAdz5OD5EC2dHDkejlrjieTJkoUrdfIAfgA+cslo1lnUVpN86u12MN5sdzn03IgBomEgiqBJBoABisACYAKwATisAEYQPSgA))
* Fixed a bug of missing errors when certain functions were checked against interfaces (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SSeGBIJAABABlTnAAA6UE5nPsmWynLQQjguDYAAoAJTIHl8wXC4UsCRCShCr4Adx5CoA3KrOfTBWaoILbPYJVKZZyALw8qyS6VsY1QWWumVKip4riQvn0+WGzkAejDnIA8gBpQVeu1sX0+EwB7B8mDEJVoYjPKDB0MRzkAdWluE5UAg+oAogAlWtR2u5EANEwkODQJINAAMVgATABWACcVgAjCB6UA))
* Fixed negation of refinement of type-guard functions with multiple parameters (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQABKyNE8qNhWYDWbSAGL2AAKThEADlRMhWQAKJq4GTSoolNi8VkKKQYRrcRWs5WlACUrIAvAA+Xk6nm01nCEQ3ADcAB0oM6YA9lHBoKyYMQZfJpbabqyAD6s+ymKAINW0APXEwhsNSCoII3AZ3O9lwGCywUisWSkR+6MG1MZ9mslgSISUKBOqDs+ll+SsjAkRMRhD21kAem7rIA7nBjjaIP3WThWRAtnR01BG1BciAGiYSJ6F8glwAGKwAJgArABOKwARhA9KAA))
* Relaxed type-guard consistency checks when the function returns with `true` or `false`. (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SRgD2UcGgAAIWBIhJRKmTyA8NJQqtA2HB4VAABTyZBc4QiG5cgA+XPspigCDVXLQxGeUAAlAr5FzaXqDawoFzgAAdG3mmBcmUrYwQZ1mgC8Pq5AHIlTc-UbbQ6ueGebt+TahQBuLkAegTiogEi5wK5JkoEEo6agtC5Ihz2DDXPpme6TpdbuwHq53p9fs1FQQwdDjvDvOjXLjieTUFTecz1BzeYLRZYpfLeDI7Yjkb5Aq5Uhk8aTKbTGazo+B4+LU4d9NyIAaJhInKgSQaAAYrAAmACsAE4rABGED0oA))

Parser:
* We now provide parsing support for `declare global {...}` (using it will still result in an error). The AST will have the shape of
```
{
    "type":"DeclareNamespace",
    "loc": ...,
    "range": ...,
    "global":true,
    "id":{
        "type":"Identifier",
        "loc": ...,
        "range":...,
        "name":"global",
        "typeAnnotation":null,
        "optional":false
    },
    "body":{
        "type":"BlockStatement",
        ...
    },
}
```

Library Definitions:
* The global `React$PureComponent` is removed. If you want to refer to it, it needs to be imported from `react`.

### 0.260.0

New Features:
* You may now write object literals with Flow Enum values as computed keys. This will result in a dictionary type, keyed by the Flow Enum type. Note that all possible Flow Enum representation types are valid object keys, so we just support all of them.
* You can use `number` typed keys (rather than just number literals) to create objects, these will be treated as dictionaries `{[number]: T}`

Notable bug fixes:
* Fixed soundness hole in subtyping of type guard functions (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQAOrAHso4NAAAQwYgACnkyB5RRKbAAlCL5DzaTzhCIbjzgOyeWqeSwJEJKFAeStjBAYDyZQBeM081kgBU3S0AbnZ9PZ7I0Tyo2B5gONIutJh5AB8efZTFAEPaoOy4EaBQBCfkQIUSiXK1XGnkYEjy64mW08gD0uZ5JkoEEoaYzCmMylKDqdj2g9j5xAAjCKhSKxaUkyaAHypuUdtg8k2NiA5-M8gUAOQAogB1JMAdzgx3lEAXPJwad1RZLtcjE9jzYTSZVutT6cziuzeYLaTTDQgcEH6IgDzYgzTxzXFQQetqcuqewa3DKBx2gLESCfUo9VWHkECEKg2AzNhoAAcgkHlkRDbBkFrWwGzjAAmVthVFYpOyHXszEYXA4HaPsMwHIcRzHAslxXexl1wDd3QgLY6CsQS9yjQ8IEI49kzPGULx9ShWJ5QSrBwapKR5Bd3RQqB0I1TQKndCRandPAyEWNAaEeWpgNyEAGhMSDoCSBoAAYrEIgBWABOKwmxAekgA))
* Fixed a bug that caused spurious errors when type guards were used in private methods (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SSeGBIJAABAAFUxNBb4ameFac4AAHSgnM5AGIhchOQAKJq4GTyoolNgASk5AF4AHycsyMXBwdqc5UyQ1c4QiG4AbglEqlC3sAEYldxVZz1aVNfKjSazRbsFbOfZTFAEKLOQB6GOcgDqcGOnKgEAA7pycJzgZyTJQIJR5TabpyAH4G8MVKPQTmzNjZTLYJ1SzksCRCSiSiS1WlWWUFxgelXYTV22PxtN1weLIoc6t56iFlv0x2SzkuiQAJmHXp9Wv9ImNpq5wdDJZM0bjieTuFTGazIYgWzoLal7c73d7JH7Qt3o-Ha8pyFWdaRIBd82XddVygelchABoTHA6AkgaAAGKwtwAVgADisV0QHpIA))
* Flow will no longer invalidate refinements made before the loop for const-like variables within a loop ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABDAHso4NBuQAKBrITn2UxQBAASlFaGIzygAG4ADpQVU8x7wqCchb2YWi8UVBCcgA+nKOuClnOAqs5nLgME5ws5AF4XebtJbrba7ZyWBIhJQlT76T6AO61OBtJ1SGRWm3a30Op0wYVS+OcgD0mc5swa-Pyg05JkoEEovHNEDDnIgW1FUGgV2Ofs0FWw3gk9pInJ2cJ9vpbAaDKsTdtDifH49yIAaJhI-KgSQy9hMIHpQA))

IDE:
* For identifiers of array patterns bind to a require call, go-to-definition will jump to itself.

Library Definitions:
* The internal type `React$FragmentType` is removed from Flow's builtin global type definitions.

### 0.259.1

Notable bug fixes:
* Fixed a bug that causes experimental auto-import-on-paste to miss imports for component names used in jsx.

### 0.259.0

Likely to cause new Flow errors:
* When a function without statics is passed to a place that expects inexact objects, the error will just say "function without statics is incompatible with object", instead of listing all the missing props in statics in errors. Error code might change.
* When a non-callable object is passed to a place expecting functions, we will have a clearer message saying "non-callable object is incompatible with function" instead of the current error talking about the callable property. Some error code might change, which requires new suppressions.

Notable bug fixes:
* For errors involving unions and intersections, we will now show a list of possible causes in a sorted order: the ones that are mostly likely to be the cause will be shown first.

IDE:
* We slightly changed the go-to-definition behavior for export from statements:
  - For `bar` in `export {foo as bar} from '...'`, we will always jump to itself
  - For `foo` in `export {foo} from '...'` or `export {foo as bar} from '...'`, we will always jump to the name at the export, if this statement is well typed.
  - For `foo` in `export {foo} from '...'` or `export {foo as bar} from '...'`, we will jump to itself if the statement is not well-typed.
* Go-to-definition on intrinsic jsx elements will jump to nowhere instead of jump to itself.
* Go-to-definition for `require('...')` expression will now jump to the default export of an ESM module if available, or the first export of an ESM module.

Library Definitions:
* Type for `React.Context` has been updated to support [React 19's context as provider model](https://react.dev/blog/2024/12/05/react-19#context-as-a-provider). Given this change, you might need additional annotations on exports. e.g.

### 0.258.1

IDE:
* Fixed go-to-definition for Meta-specific module references.

### 0.258.0

Likely to cause new Flow errors:
* method-unbinding errors will not affect choice over intersection members. (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQAOlAKniuJCAAQAWV2tQAMnAdjzgDyRAAKACUyB5whENx59PZnJM3OwPIACpRYSKxRKRPLZTyALwAPh59lMUAQAG4Vez2RonlQtbZ7DyIGgAFbygUSYWirUAMh1esYBuw9udUB9vqsIh5GBIPNNloV1xMjoA9LmeQB3ODHBUQQs8nApyWCiBsRYPNAVNgVBA8kx6yjshNJ02prNKnPs3IgBomEhwaBJBoABisACYAKwAdisM5A9KAA))
* We now disallow coercions between functions and class statics.
* The ability to configure `react.ref_as_prop=disabled` is removed.
* Use of `React$Node` and `React$MixedElement` outside of libdefs will now trigger `internal-type` errors.

Library Definitions:
* Improved definition of `Array.concat()`.

### 0.257.1

Misc:
* Flow language server will stop advertising that we can handle all code actions kinds prefixed with `"source"`. It can help to prevent VSCode sending flow irrelevant code actions to handle during save.

### 0.257.0

Likely to cause new Flow errors:
* We have improved inference for exported primitive literal values.
  * Before this change the inferred type for `export const obj = {f: "foo"};` was `{f: str<"foo">}`, where `str<"foo">` was a type that opportunistically and unsoundly behaved either as `string` or as the singleton type `"foo"`.
  * Flow will now infer the type `{f: string}` for `obj`.
  * For const like exports, e.g. `export const x = "foo";` Flow will export the type `"foo"`.
  * To fix errors that arise due to this change, you can provide annotations around exported literals or use the [`as const` modifier](https://flow.org/en/docs/types/const-expression/).
* We fixed a major unsoundness with regards to dictionary object creation. Previously, the computed property will just be ignored for `string` or `any` keys. Now, if the computed property is added to some existing objects (e.g. `{foo: string, [stringTypedKey]: v}`), then we will error on the property. Otherwise, the `{[key]: value}` will be `{[typeof key}: typeof value}`
* We fixed a bug that causes opaque type's underlying representation to leak beyond the file it's defined. You might see more errors as a result.
* Flow will now perform a complete function call check even when `[react-rule-unsafe-mutation]` errors are raised.
* Previously we incorrectly distribute-over-union for maybe or optional input types of conditional type. (e.g. `(?string) extends string ? true : false` is evaluated to `true | false` instead of `false`). This is now fixed, and code that depends on the bug might have new errors.
* If you have refiend a value based on `typeof x === 'function'`, and then do `typeof x === 'object'`, the result is now `empty` as that is impossible.
* Singleton types are now also considered in conditional equality checks on member expressions. (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQA9KyAAQAARgRAA7gAdKBC2z2DkAQWQHIA5OLpRyALwyuUAbhF0DFACEpdLNfKlbrpWrHhqJByAMI6836mXW41CjRPKjYDmAjnRODQKXAIUcv0clbGHVy3i+-0YHUYaWhqD0jkAHw5Pqg-oDqx1epjqbQOrQ0aF9PtsAeyk9KYW9gAjAAKACUSbDfpIfLge1qHOrHugw2M9eTqdT1TIEuQjYHaCiW2NA-9Q5dlo57I5AHU4McOVAIHyOSZKBBKLwOSQKpCLRzaRuIGaT7ghBo2OeU7L5YnDWPs5Pp-76QWhUKYCW8Llu0EgAEx1g2Kb+nAMAdl2UA9i6CrIRafZxkuq7rpu267vuh7Ho887niQl7Xo8t73o+yovjKeq-lAuQgA0JjHtASQNAADFYoEAKw8VYHEgPSQA))
* `React.Config` type, deprecated in v0.254, is now removed.

New Features:
* `'key' in x` now refines the type of `x` to objects which have the property `key`.
  - `in` checks for both own and non-own properties.
  - If we find that a key does not exist in an inexact object or instance/interface, then the negation is not refined since the property may or may not exist.
  - An optional property is also considered as if it may or may not exist.
  - If a proto/super is a union, every member of the union must have it
  - If the input to the refinement is an intersection, one member of that intersection must have it
  - We don't refine arrays since it's not useful (check for `.length` instead), and also because Flow doesn't handle array holes.
* Flow now supports `no_unchecked_indexed_access=true` in flowconfig, which is equivalent to [noUncheckedIndexedAccess](https://www.typescriptlang.org/tsconfig/#noUncheckedIndexedAccess) from TS. It will add `| void` to every indexed access with general string key on dictionary objects or number key on arrays.
* Support for React 19's ref-as-prop model is now available via `react.ref_as_prop=partial_support`, and this is now the default. (To disable it, use `react.ref_as_prop=disabled`.) Under this mode, ref prop in jsx will be treated as a regular prop for function components, but utility types like `React.ElementConfig<...>` still won't include the regular ref prop yet.
* Flow now allows using union of strings as the type of computed property keys.
* You can now destructure object properties using number literal keys (which are int-like).

Notable bug fixes:
* Refining type guards with opaque types will now refine the general type (example of code that used to fail before, but now passes: [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQAOhJWT5VgACACCAB4ACoAPm5AF5ucBuQBqAAkspWxmQ3IAjLxuU1cDJlYLufTuQAfSUy+WK7DKgBM6s12u5uvpAG5OWbuQAhIWiiUCkVOqAaJ5UbDc2EYACOMm5LoA8h7fS6AMIe8VupNGmM+zmc-24QPcmAPZRwaDc2nukUACkgIkYqXN3MTIoAlMqqzWFiWSCmM1BOfnHvCoJH2hIVeWbXXE0USmxhc2NRA4GxJZzuSWYNzy6Wx9wZI3G8vB6vubZ7Nz8Crk+PuRhO+6p6VhQ7VwB6Z-cylLtLcnDBrZ0FdHieEhnhayp3sUD6Xju2BPtyr7clAEDAcC3ImJQECUNeUC0NyIgYdgAH0pyRE9rABYDkO9gWuW+7AABq4ugAylIFQIPGyaJvYphQAgj70ZGPLMdxCCusm7pcaxfGHqufaFsWaDblqdZCax8a0fxR5rhuW7jnuB6aQZgHQKe55QUp16dipPGurB8GIchg5oRhWE4XhLAaYZx7GcB+CgdyVkiWZMi2W+9lYah1DOcCrn4R5q4kZpJH0rkIANCYJBFlASQNAADFYFoAKwAGxWDlID0kAA))
* Allow more flexibility in checks involving super type of opaque type (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQAOhJWT5VgACACCAB4ACoAPm5AF5ucBuQBqAAkspWxmQ3IAjLxuU1cDJlYLufTuQAfSUy+WK7DKgBM6s12u5uvpAG5ORonlRsNzYRgAI4yblm7kAeWVAvspigCGFTqgzrmuDdGqoHuVAajEG5GBIfP5oYqEYd3IA9AXuQB3ODHblQCASbk4dNQbkmSgQSj12jckQt7Cc3IgBomEhwaBJBoABisFoArAA2KyjkD0oA))
* Fixed a bug when using opaque types in type guards in files other than the one where the type guard function was defined.
* We now allow a union of generic or opaque string typed values to be used as a key of computed property, if it's bounded by a string type.

Library Definitions:
* Add libdef types for `getNotifications` and `showNotification` API for `ServiceWorkerRegistration`

### 0.256.0

Notable bug fixes:
* Fixed a limitation when checking intersections of polymorphic types (e.g. [tryFlow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SRWxgABABBdkAXnZwHZlgQyHZAEZ2fSANwAHR8q3ZACFefzBRhheyAEwSmVQWVs7DsgAKAB4ACoAPmV3IAPoqdRonlQDYD2fIRSbgbRLQAyflS2XydkYEhG42e82S9kAeij7IA7nBjuyoBAJOycEGoOyTJQIJRkxA47kQA0TCQ4NAkg0AAxWDUAVnrVmrIHpQA))
* Fixed an internal error raised when using `super` in an object method. Such cases are now user-facing errors.

### 0.255.0

Likely to cause new Flow errors:
* Referencing a react-component typed value in type position will now create a `React$RendersExactly<typeof Component>` type instead of `React$Element<typeof Component>` type. All existing usages intended to create an exact react element will now error as a result. Using it as type passed to render type, `React.PropsOf`, `React.PropOf` and `React.RefOf` is unaffected and will not be affected in the future. If you want exact react elements, the only way that doesn't trigger Flow error is by `ExactReactElement_DEPRECATED<typeof Component>`.

Library Definitions:
* `React.Config` type is marked as deprecated and will be removed soon. You can create your own equivalent type with
```js
type ReactConfigShim<Props, DefaultProps> = $ReadOnly<{
  ...Omit<Props, $Keys<DefaultProps>>, ...Partial<DefaultProps>
}>;
```

### 0.254.2

Misc:
 * Add option to disable experimental configuration


### 0.254.1

Notable bug fixes:
* Fixed a potential crash when experimental multiplatform support is enabled.

### 0.254.0

Likely to cause new Flow errors:
* Previously we incorrectly applied truthy refinement on optional chaining where it shouldn't be applied. The issue is now fixed, and you might see change in behavior similar to [these examples](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SRgD2UcGgAAIAO5wCS1ABK7W0EgAjAAKFgkUXILnACBbOVSGT0rkAH3lirlZOw9IAlPKADpQLlmrlwGBcqUinQAfisiq5AF5XVzdYbgCbzT6udLRVyMCQudgRHDaABuLkAemjXNmDU5+UGfttElT8C+bC5aRDYZWvC5UAgPIzFVKOYgWqV7o29O95vrUCbbMe8NNfIFwplOgATDaexI5Qqayq9Rrq3KjrgDcbTebLdb-fbHVsXW7p56G77U4PA8HQ+Go7H4yxExBk7RdwGWJmK7nDwWiyWy1nK5Oi9oZ9uuU2m2bWw5blOyFNMAGYB1lD8GggOA2DVTURynL9Zy9eczUXSCVydV1nS5B4NDvNgt3Q31l3TIM8yPGM4wTJMSBTapPggdNb3LQti1LNi31zJCuRguD-0bE16VyEAGhMEhOSgJIMnsEwQHpIA)

Library Definitions:
* Improve Array `toSpliced` method type (thanks @aph!)

### 0.253.0

Likely to cause new Flow errors:
* `React$ElementRef` is now implemented with a conditional type instead of a builtin Flow utility type. You might see new errors with generic `React$ElementRef`. Most of the times, you should replace `React.ElementRef<T>` with just `T` if there are no other uses of generic `T`.
* We've increased safety in checks involving recursive types.

New Features:
* You can now set multiple [`@flowtyped`](https://flow.org/en/docs/libdefs/creation/#toc-declaring-a-module-in-at-flowtyped) like directories via `module.declaration_dirnames` config. e.g.
```
module.declaration_dirnames=<PROJECT_ROOT>/decl1
module.declaration_dirnames=<PROJECT_ROOT>/decl2
```
Please note that when you set this, `@flowtyped` at the root will no longer be considered special, and you will need to explicitly add it like
```
module.declaration_dirnames=<PROJECT_ROOT>/@flowtyped
```

Notable bug fixes:
* A generic type as input to conditional type will have type parameters pinned with their default or upper bound, instead of being `empty`.
* We fixed a cache collision issue that might cause instantiation of a value-imported generic component type to interfere with a typeof-imported generic component type.

IDE:
* Flow now respects `includeDeclaration` from LSP request for find-references command. In practice, this means that "find all references" in vscode will continue to return both declaration and all uses, while "go to references" will exclude the declaration.

### 0.252.0

Likely to cause new Flow errors:
* Flow will no longer apply type transformation to function components' props when you extract props through the utility types. [For example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABDAHso4NBOQAxYgACmpjBIyE5wBgxCl9lMUAQ9IAlFKAEqsZRWAByEA0AG4ADo+VZC4icgC8nM10QAJABRNreCQAYWg8AQAB4VsYIDBzRAAHwGzkAejDnNmDX5+UG0oA1LKIPKpBVlbxOVAIAB3aXJ1OK5Um3IgBomEj8qBJBoABisACYAKz12sgelAA), function components' props won't automatically get `$ReadOnly` applied, which might cause new errors.

New Features:
* We have removed the restriction of not allowing generics with ref prop in component syntax

Notable bug fixes:
* Files in `[declarations]` section will have all lint errors silenced.
* Fixed an issue that caused errors in hover type on unions of evaluated types ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQAOhIAEzs7lQVk+VYAAgAggKALwC4ArYzIAUAcgwsvpfKl2AFACExRKVTLZWhFcrBQBhTWS1Y6sD6-nGAUAERN2rlbEtButQo14pFAB91QBuF2qw128XG702v28q2qt2BzVugXewPhlXCtWBgAqgo9qZtAG1WSAVfmALrhgD0pYFlYAegLKWwBWkBSRahAAO4CgAGyejNoz1qzgbzBdWxdjGoTNo7fPLldnFXsrHrEBgne72b7qoHufzhZARc1+Yw+fjAvzaGP3vzYAvp5AbHzU4jMAeyjg0AFMGIAB40zKewA+AAKeQZTTIddyLABKGUGggOB62AJUoBnWdULQ9CBRrOsGwgJsW3bDswJ3Ec9zHQNHxQjCqLQ+cFnYAVl07Ijh2MUdxUPG8z048BuPvEBH1yEAGhMEg3ygJIGgABisDkAFYAEYrEkkB6SAA))
* Flow allows unions of strings to be used as the key in creation of objects with computed properties.

### 0.251.1

Misc:
 * Performance fix

### 0.251.0

Likely to cause new Flow errors:
* Remove the deprecated `React.AbstractComponent` type. We recommend replacing them with [component types](https://flow.org/en/docs/react/component-types/). Since v0.249.0, we have provided a codemod via the `flow-upgrade` package to aid larger codebases with this conversion: `yarn run flow-codemod eliminateAbstractComponent path/to/src`.
* In addition to the removal of `React.AbstractComponent` type, we also removed support for the internal `React$AbstractComponent` when more than 1 type argument has been passed. Similarly, you should migrate to use component types if you want to specify ref props and renders.

New Features:
* We now support a new way to easily specify library definitions for npm modules. For all files in `@flowtyped` directory at the root, they can be resolved by the relative path without leading `./` relative to `@flowtyped`. e.g. `<PROJECT_ROOT>/@flowtyped/react.js.flow` can be used to type the `react` package, and `<PROJECT_ROOT>/@flowtyped/react/jsx-runtime.js.flow` can be used to type `react/jsx-runtime` module. We now recommend using this approach instead of using `declare module` in global libdef.

Notable bug fixes:
* `any` will now correctly propagate to inferred render type. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQAOhINE8qNgAAQwB7KODQHkAJUEGnxAEEoLQABQASmQPOBtAA3KyoJzcNy+QL4VAeQopNExZqTCQADwipVi6JWAByEA0AD5ZWAlZARIxoBZZSwYAB+G2sZQAEjFMAAyriTBaiiU2M7eDyrKnsF6VvKeSwzfjRYrRerHtB7Ib5MblKVpbRTRKeQBeMsViS182y1tSmXy1U8gD0vZ5swaQvygx5DwltnsNAqpWTUAgAHceRAthqjTRK2xqx3lSRDRm1X2B6ulSqeQA-Z0HuG0DW5EANc1CqBJBoABisACYAKyf98gPSQA).
* Fixed a bug with regard to jsx type inference with generic callable objects. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABLZ7JyAAoQQYAeWQnOAAB0oJzpZyADwAFQAfAAKamMEii4AwYii+W8TkwKCi5XygCUnIAvIrOQ0IHA2PTTaKAEqsZRWAByEA0vEl9IA3JK2bgOdzoLyBYMAGJGuVK1WUWEasXaiC6-WG41my3W232x05zmu6Ke73YQNQSWyyO0IUG4gW4AAZnpBqgjeVDXNVrFDU5GBInPspigCH9nIA9AAqIe1CDaNickyJyic6DcgcSfWzBpweckQZrqV9xw0EScqcTzn01sTxUV6uC2gx+sQRsttsdruF4B9gdDqQKjHScZxIOcFyXagIFXddql5S9r1ve9JVyEAGhMEg9ygJIMnsEwQHpIA)
* Fixed a soundness hole that allowed frozen objects to be incompatible with non-readonly objects. This [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SVs9gABDBKBAAF6CADyaAAVnMJOyALzswUi5RWLnYbB8gAUwBgyHZABYAEz0gCUAG4ADqPaAckSpVrYaWi9Wq9XCEQ3dkAH3Z9lMUAQ9IlnO5fKg1uU+vZAHoQ+yAO5wY7sqAQCPsnDs4HskzcyjG82WNBtQMxGA+w0gYiMEhF-W5EANEwkODQJINAAMVi1GoAHFYAIwgelAA) will now produce an error.

Library Definitions:
* We have updated some core React type definitions to use the new [component type](https://flow.org/en/docs/react/component-types/) instead of `React.AbstractComponent`. Most of the code won't be affected, but you might have to sometimes make the Props readonly.

### 0.250.0

Likely to cause new Flow errors:
* Fix a bug where scope analysis of exported component type is done incorrectly, which causes generics mentioned within component type to be incorrectly typed as any.

Notable bug fixes:
* We will now remove previous props in generic component types in type argument inference. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABJARIxoBZOQAhAAUMGIyE59lMUAQvE5mEoEuEIhuAEoANycgD0Ws5swacAg+Xl7E5RSYuCUpU5JkoEEonNqJmwAB0oGzLSxOTAHspDVBOSwRBAGtgAMrpbAABTtjBIAB4Y7CSBLgFZ0-SAHzCsASnl8r4+UXiyVSCqyznpqxJuOq1USmskdVu2z2TlRzkAXkD2GDoYj5kbwsFGu1uv1hvygzlHdqGBInJWxk5wAA1GKIBKpeW5auFUrriZGZyoBAAO4rvdUA8qo9u3IgUP4-1JDL2EwgelAA)
* When `all` option is specified in the config of `flow-remove-types`, we now respect it and will correctly handle Flow-typed files without `@flow` pragma.

### 0.249.0

Likely to cause new Flow errors:
* Uses of `React$AbstractComponent` outside library definitions will now trigger `internal-type` lint error, which is on by default. `React.AbstractComponent` is also marked as deprecated. We recommend replacing them with [component types](https://flow.org/en/docs/react/component-types/). We have provided a codemod via the `flow-upgrade` package to aid larger codebases with this conversion: `yarn run flow-codemod eliminateAbstractComponent path/to/src`.

Notable bug fixes:
* Fixed issue with `StringPrefix` and `StringSuffix` when used as a component syntax prop.
* Fixed an issue that causes type argument inference on component type to be incorrectly under-constrained. ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABDAHso4NBOXA2ABhdKMaAWAA8AAVKLCSMhOcArCrGZyAJJ2SyPWScgBKgg0+MVBuiVgAchANAA+AAUAB0oJznZywIrICJxV8fLaWDATaxlFYDTAAMq4kySzX2YGQ628Tkqqyy+UASk5LCgRpI+sNJhIvEdqfdYolPr9AbNIfDxMj0e1cYTSZTjBI6cz2dzWfzAG5HY62bgOa7S96JJyAIK21N97vslgjz1l8cAIV9mkrQerEcokprkvspigCGt8e5xEVh4qCATmEoiuEIhu7bz+IA-JPZ47bPZOcLOQAvIKIqjhYtorjOnIAPRQZyDxGj+Ugol8bCcjgMAQCwCYAO6YVsOZQBA2H9lAuQgA0+b8qRyAgBk9gmCA9JAA))
* Fixed `as` cast support in `flow-remove-types`.

Library Definitions:
* React string refs are no longer supported in `React.cloneElement`. e.g. `React.cloneElement(e, {ref: 'foo'})` will be an error.
* The deprecated `React$Ref` type and various aliases of it are removed.

### 0.248.1

IDE:
* `React.Component` annotated exported will now also show up in auto-imports in a type position.

### 0.248.0

Breaking changes:
* Support for long deprecated predicate function (`%checks`) is removed. It will still parse, but all of them will error with unsupported-syntax, and `%checks` will be completely ignored for type checking purpose as if it doesn't exist.
* `$TupleMap` support is now removed. `$TupleMap` will now resolve to the global definition is there is one, or it will fail and the type becomes any.

Likely to cause new Flow errors:
* Support for the unsound `$TEMPORARY$*` types is dropped. If your codebase happens to have any of these types, you can replace them with there sound and well-documented equivalents:
  - Replace `$TEMPORARY$object<{props}>` with `$ReadOnly<{props}>` or `{props}`
  - Replace `$TEMPORARY$array<T>` with `$ReadOnlyArray<T>` or `Array<T>`
  - Replace `$TEMPORARY$number<42>` with `number` or `'42'`
  - Replace `$TEMPORARY$string<"foo">` with `string` or `"foo"`
  - We have provided a flow-runner codemod via the `flow-upgrade` package to aid larger codebases with this conversion: `yarn run flow-codemod replaceTemporaryTypes path/to/src`.
* The inferred type for `Object.freeze({ A: 'a', B: 'b' })` is now `{+A:'a',+B:'b'}` both locally within a file and when the object is being exported. This replaces an earlier unsound behavior where the type of A would opportunistically behaved either as `string` or `'a'` depending on the context where it appeared.
* [React string refs](https://legacy.reactjs.org/docs/refs-and-the-dom.html#legacy-api-string-refs) are now banned.
* `contextTypes` and `childContextTypes` in react class components are now empty typed, so declaring [legacy context](https://legacy.reactjs.org/docs/legacy-context.html) in React will be an error.
* Component syntax component and component types' ref prop now must have `React.RefSetter<...>` type.

Parser:
* Component type in parentheses can now be correctly parsed. e.g. `type Foo = (component(x: number) renders Bar);`
* Trailing comma is now allowed after rest parameter in component syntax components and component types.
* The v regex flag is now supported.

IDE:
* We now provide a code action to stub out a react component, at the location of an unbound JSX identifier.
* Component declaration/type without renders clause will no longer show `renders React.Node` on hover.
* Hovering on components will now consistently show its props and renders information. Previously, the information is omitted for component declarations.
* If we see `: renders <annot>` at the position that expects a render declaration, the quickfix will suggest removing `:` instead of replacing `:` with `renders`.

Library Definitions:
* Added type for `util.stripVTControlCharacters` for NodeJS.

### 0.247.1

Misc:
* Performance optimization for unions


### 0.247.0

Breaking changes:
* Support for the deprecated `$Call` type is removed. `$Call` will now resolve to whatever `$Call` points to in the global libdef. If you need more time to migrate, you can create a shim like `$Call<F, T> = $TupleMap<[T], F>[0]`, but please note that we intend to remove support for `$TupleMap` eventually.
* `deprecated-type-dollar-call` lint is removed, since the support for `$Call` is removed.
* `react.disable_function_components_default_props` config option is removed. It is on by default since v0.238.

Likely to cause new Flow errors:
* `$TupleMap` is deprecated and will be soon removed, now that mapped type works on array inputs. Existing `$TupleMap` will still work in this release, but every use will trigger a `deprecated-type` lint that is on by default.
* Flow now performs literal subtyping checks for strict equality conditions in non-refinement contexts. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABLZ7JyYMRkJyAOT8iBCgDcAB1YMROQBeBXCzCUCWcgD0as5JkoEEonKgEAA7tLciAGiYSHBoEkMvYTCB6UA)
* Fixed destructuring with invalid literal defaults. The following now errors properly: [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SRgD2UcGgAAIYAAKYBQZzYLkAXi5AHJ5FLxfTkFyBUK5eKYMRxVyAD4SzCUGUASnlAB0oPTciAGiYSJyoEkMvYTCB6UA)
* Using string ref on components that are definitely not a class component is now an error. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SRgD2UcGgAAIAGLEAAUAEoucB6QAdKASgA8fIgXJYMAAvGKQBAhHUVVyAPQAPgA3NqtVzZg1OflBlyIFteFyoBAAO5ckyUCC3fgNEwkTlQJIZewmED0oA)
* React utility types will no longer incorrectly accept hook types when they expect component types. e.g. `type A = React$ElementConfig<hook (props: {foo: string}) => void>;` will now error.

New Features:
* Refinements against negated bigint literals should now work.

Notable bug fixes:
* Mapped type on generic arrays is now supported. Previously it will fail with array is not an object error.
* `flow-remove-types` now correctly handles `as` cast with generics.

IDE:
* On hover, values that have `React.AbstractComponent` type will be shown in the [component type](https://flow.org/en/docs/react/component-types/) syntax
* Flow now offers a code action to insert inferred render type when you hover on the name of the component.
* Flow now provides keyword completion for component type, hook types and render types.

Library Definitions:
* Overly restrictive typing of queueMicrotask is now fixed.


### 0.246.0

Likely to cause new Flow errors:
* Support for $ObjMap has been completely removed. This change means that:
  * `$ObjMap` will resolve to a user-defined `$ObjMap` global if it's available in your libdef. You can use `type $ObjMap<O, F> = {[K in keyof O]: $Call<F, O[K]>}` to get most of the same behavior, except that in the mapped type version `O[K]` will include void for optional props. Please note that this should be considered as a permanent shim, because we intend to eventually remove `$Call`.
  * `flow codemod key-mirror` command was removed, since this codemod tries to convert `$ObjMap` to `$KeyMirror`
  * `deprecated-type-objmap` lint was removed.
* Component syntax components without ref prop will have `void` as the instance, so `React.ElementRef<component>` of such component will return void.

New Features:
* Mapped types now support array and tuple types in the form of `{[K in keyof <array or tuple type>: <mapped type>}`.
  * With this support, we intend to deprecate $TupleMap in the next release and remove the support for $TupleMap eventually
* You can now refine against variables and member expressions with a literal type, not just literal themselves.

Notable bug fixes:
* Flow now only reports one error when two disjoint large enum-like unions are compared ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQAOlANE8qNgAASAnn4ZA84G0ADc7PZMAeyjg0B5C3sAEYABQASh5wHZPO18tWPIAKoqeQBeHkAcgwirNPIAPua0FbbeawFbxVAdbrjAaAEwm80Yb3Wu1mtCBp0hgDMZrdWp1+GFJANRowif13tFPIA9JmeQAhTQQFjy2q0nmUtjyiA8lgSISUd0RnkmSiFxOLHlQCAAdx5cAkPK7cGOPIGtB5OBHX1j2vjKZ9CaTGezeYLRYkJcT5cr1d2dYbTeorZ57c7Pb7A6HuEng3HvOg2HZ9PZuRADRMJFlUCSDQADFZvQALAArFYiogPSQA)).

IDE:
* Code actions on component syntax components will produce smaller targeted edits, rather than reprint the entire component.
* Deprecated utility type like $Call are no longer suggested in autocomplete.
* Flow will now provide a quick fix when the spread prop of a component with component syntax redefines some already defined props.

Library Definitions:
* `React.Ref` is marked as deprecated. Use `React.RefSetter` instead for ref props, and `React.RefObject` for returns of `useRef`.
* `React.RefSetter` now includes null and void.
* `Promise.all` and `Promise.allSettled` have been updated to use mapped type instead of `$TupleMap`. The requirement on the type arguments are slightly changed.

### 0.245.2

Misc:
* The language for invalidated refinement is slightly tweaked. Instead of saying refactoring to a constant, we now say refactoring to a const variable.


### 0.245.1

Website:
* Refined expressions are now highlighted in try-flow playground. Hovering on refined expressions will show where it's refined, and hovering on some invalidated property refinements will show the location and reason of the invalidation.

IDE:
* Flow now responds to `textDocument/prepareName` LSP request. This ensures that Flow's rename support can be surfaced when multiple VSCode extension adds rename support for JS files.

### 0.245.0

Likely to cause new Flow errors:
* `React.Element` type, deprecated in 0.243.0, is now removed.
* Fixed a bug where refinements are incorrectly not invalidated. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZlduuSMTBJC-Mgz4sHwNSBos6A6u9w1mbGqJUoBaCRHEzrcDEgBrbAk62mPhXFxJ91v7x+fRHEp3gxCKBgNpUU7nAF5KBQA7tEhUJ4VJqmDA+SEgIEwsBwhHHCCMTZQbjHOoYCrooHwmDYY4Ie5HSgkY5kCQM7ZXKH5E7MdJwMgAgC+-EgGiSa1wVGwAAIYEDlHBoDKABRFEpsACUyClaGIoKgAG4ADpQcWSqW2ewy4hagD8wEwlC19lMUAQAqNUGNcBgUqVxqlUoA9IGpQASABiRAA7uHigBZbD+q0QKwOqUAQgAvFKXrgpQAyfPKmDEVNUdUFotJksptPBnMQKUmSgQSjazSt2RN6ithtR40V4DG40C425EANEwkBVQJIZewmEACoA)
* Under custom jsx pragma, children is no longer incorrectly added to props. Some spurious errors will be gone. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZlduuSMTBJC-Mgz4sHwNSBos6A6u9w1mbGqJUoBaCRHEzrcDEgBrbAk62mPhXFxJ91v7x+fRHEp3gxCKBgNpUU7nAF5KBQA7tEhUJ4VJqmDA+SEgIEwsBwhHHCCMTZQbjHOoYCrooHwmDYY4Ie5HSgkY5kCQM7ZXKH5E7MdJwMgAgC+-EgGiSAHoxQACAACACsSPJJWB8mkRAApBUAHSg2rWuCo2ElyMlADFiMhJajaABuXUHfUsSUwIHKODQJUq9Ia+QAHgAKgA+AAUYAtVt4kp5jBIFuAmEoFoA5ABmRNCpW1OC4NgsKAWv0ASnztp1wK2EklLBI2grAF5JT6zRBJfHa5qQMn2wHgMmBT6xU2A9bJRLJRAbpKoBAAO7a3IgBomEhuqBJDL2EwgAVAA)

New Features:
* Added LSP signature help support for JSX attributes. The feature is triggered with the opening `{` of a JSX attribute value.
* Flow now allows to configure the typing of jsx creation under the new option `react.custom_jsx_typing=true`. Under this flag, Flow will type check jsx by through your self-defined global type `React$CustomJSXFactory` type, which should be a function type that accepts arguments according to jsx spec. e.g. You can write a loose one like `type React$CustomJSXFactory = (component: any, props: any, ...children: any) => React.MixedElement`

Notable bug fixes:
* Fixed jsdoc attachment to signature help results for overloaded functions. ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZlduuSMTBJC-Mgz4sHwNSBos6A6u9w1mbGqJUoBaCRHEzrcDEgBrbAk62mPhXFxJ91v7x+fRHEp3gxCKBgNpUU7nAF5KBQA7tEhUJ4VJqmDA+SEgIEwsBwhHHCCMTZQbjHOoYCrooHwmDYY4Ie5HSgkY5kCQM7ZXKH5E7MdJwMgAgC+-EgGiSAB0JGKoAB6ABUsqlsoABPBGRIlRAGiYiOxFUqAAKOGgiJXyJXHJUARnsSqNzg1MBVcDVGq1lB1bD1hqcJto5qVACYoGxbT6HU6XZrtRBdVBZdKpWtcFRsCqgco4NAVcQABTyZBWgAMvCVtALlstAEpywBuKVyhVxpVkWwhqPumOepve42m-3W9V2k0QR0t6Btt0er1D0v+oMhmcj5sHceu6Ox+OJg7Jlhp4GE7MQPMFgPF0snqsnutQevSpUAZUQRIkQl3AAk8IwlQB3OCvJVQBA37NrUQFKhItSpqYCC1OqABS94ACIQGASoVEqrB1OGEGpjAECvEBFQIEq1RkCQyBSnhR5ykqkG7vGlbXlROaWmeNF0amDFMbmp4luxJicdKjFSrkIBuiQmZQEkDSFlYAYACzyVYhYgAKQA))
* Signature help on overloaded functions will show information in the correct order of signatures
* Labels of autocomplete on members will indicate if the property is optional with a "?"
* Fixed a bug where a type-only namespace within a namespace is accidentally dropped. This bug used to make `globalThis.React.Node` being incorrectly unavailable.
* Fixed poor interaction of "Add missing attributes" quickfix when Linked Editing Range is enabled.
* Clicking on related symbol location on hover will jump to the right location in VSCode. Previously it incorrectly jumped to the position 1 character left of the right location.

IDE:
* Elements of component syntax components  (e.g. typeof `<A />` where A is in component syntax) will no longer be shown in hover with shorthand syntax like `A`. Instead, it will be shown as `React$Element<typeof A>`

### 0.244.0

Likely to cause new Flow errors:
* The `inexact_tuple_types_syntax` option (which was on by default) is now deleted. Make sure you are using `hermes-parser` and related packages at version >= 0.23.0
* Render types will no longer allow specific renders wrapping more general renders like `renders React.Element<React.AbstractComponent<{}, mixed, renders* A>>`. Invalid render types will behave like any.
* Some previously allowed generic render types are no longer allowed ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZlduuSMTBJC-Mgz4sHwNSBos6A6u9w1mbGqJUoBaCRHEzrcDEgBrbAk62mPhXFxJ91v7x+fRHEp3gxCKBgNpUU7nAF5KBQA7tEhUJ4VJqmDA+SEgIEwsBwhHHCCMTZQbjHOoYCrooHwmDYY4Ie5HSgkY5kCQM7ZXKH5E7MdJwMgAgC+-EgGiSa1wVGwAAJICJGNALFKAGLEACMAB4ACrIKUAJVYyisADkIBoAHwACgAlFKWFANIypZqANxSgD0bql9jgryl3CIAHdSgAde0HCUsGXpeUwnzK4gAJi1Ov10WNpuwlptdodJAA-E7XR6vVlff6IEG2KHxZKo3KFXGVRAAMzJvUGmIm83W22CXMAKkL7s93rLrwrIbDINrspjiqbABYtaqUx30xpeE6E6u013Mz2cyYSIOLZrVVKAD5bq1Fkel3B+8eV6vhmfRhsSKUAISobdThr3LNezDR1T0veMIFVG9hylKAKylTBoUnXIQAaI84GgJIMnsEwQAFIA)). This ensures that generic render types are only used to model transparent components like `React.Fragment`.
* `renders* T` where `T` is generic is now banned.

Notable bug fixes:
* Render type with union type arguments will be correctly wrapped with parentheses.

### 0.243.0

Likely to cause new Flow errors:
* All `deprecated-type` and `untyped-type-import` lint are on and at error level by default.
* Use of some internal types of Flow that have other identical public-facing variants (e.g. `React$ElementConfig` vs `React.ElementConfig`) are now a lint error that's enabled by default. To disable it, set `internal-type=off` in the lints section of the flowconfig. For now, these types are still allowed in libdef files, but we plan to also disallow it in the future. To codemod existing code, get the latest `flow-upgrade` package and run `yarn run flow-codemod replaceReactDollarUtilityTypes`
* Direct use of `React$Element` type is banned via the `internal-type` lint. The `React.Element` alias still exists for now, but it is marked with `@deprecated` in the jsdoc. Please read the jsdoc for better alternatives. A global alias `ExactReactElement_DEPRECATED` is added to help you clearly mark the use of exact React element types, and you should use it to replace `React.Element` that you cannot replace without compromising runtime type safety.
* Flow now consistently errors on bad comparison with enums [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SUEogABABBdnAekAHS+whE7IAQjz+T5VuyACoARnZAF4ueyAD6igDcApWxhlACZFaLVVzNVABRonlRsOzbPZ2RJZcgZbKTebcJbrdBbRJdY7pbqTfbFQqlfb1eyAPTh9mzBpwCD5QbsqAQdkmSgQSi8JMQADuqeoGZIWvlwZD-ojUbTGYFuRADRMJDjUCSGXsJhA9KAA)

New Features:
* Allow generic bound by inexact tuple as function rest args. [This example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZlduuSMTBJC-Mgz4sHwNSBos6A6u9w1mbGqJUoBaCRHEzrcDEgBrbAk62mPhXFxJ91v7x+fRHEp3gxCKBgNpUU7nAF5KBQA7tEhUJ4VJqmDA+SEgIEwsBwhHHCCMTZQbjHOoYCrooHwmDYY4Ie5HSgkY5kCQM7ZXKH5E7MdJwMgAgC+-EgGiSMCByjg0AABBgADwAFWQ0oA2lZ1QBdAB8AAoqAgSMqFbxpTBlTr1VYFQBKaUAXi10oaEDgbFtwAAOlBpaaLer9SRrQBuaUAelD0oACiwGlL8oNZd6TJQIJQTVAIAB3aUAeQA0l6BV7ciAGiYSFKoEkMvYTCABUA) now works.
* Support for `declare component` statement and `component` type is enabled by default.
* Flow now provide the "insert inferred type as type cast" code action. You only need to select the expression in the editor to trigger the code action.

Notable bug fixes:
* Fixed a bug that causes an internal error. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABLZ7JzYfC7MhOcAANr2UxQBAAXSFADVuDJ6QBuAA6UDVsIwAEcZJyVsZOfLcDIhcIRDdOQBeTkAEiNMhIAB5OQB6F2ckyUCCUIVgWhPbK8TlQCASTkVPFQbge6jetWcvWrCAwPlwuDQEi8NUAPiVuRADRMJHTUCSGXsJhA9KAA)

Misc:
* `experimental.namespaces` config option is removed. Namespaces support is enabled by default in the previous release.

Library Definitions:
* Add `.replaceChildren` method type definition for dom nodes.
* `Object.getPrototypeof` is still special cased, but the special-casing is only available for a syntactic `Object.getPrototypeof(...)` call. If you try to use `Object.getPrototypeof` in some other ways (e.g. `const f = Object.getPrototypeof; f(...)`), you will get a a less precise and accurate typing for it. The `Object$GetPrototypeOf` type, which was used to back the special case behavior, is removed.

### 0.242.1

New Features:
* For `Pick` utility type with string literals in the second type argument, go-to-definition on the string literal will jump to corresponding prop's location

Notable bug fixes:
* Re-exported types will show in autoimports from files without any import statement.

### 0.242.0

Likely to cause new Flow errors:
* Support for special function type `$Flow$DebugPrint` was removed. To see the internal representation of type, use the `flow type-at-pos` command with flag `--debug-print-internal-repr`. Note that this is for debugging purpose only, and you should never depend on the output of `--debug-print-internal-repr`
* More invalid compare errors will be consistently caught when using `==` and `!=`.
* `invalid-render` errors can no longer be suppressed without specific error code.
* Flow can now detect more bad cyclic types, including across module boundaries. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SRWxgABABhWhPbIAdTgElqABIAGrcGQkdkAXnZwAAOlB2ezICJvBJkOz7KYoAheIrlQBtHa0TXaioIAC6mrFEvaAB5ubywAKhbbcJKAHy8dkAel97MwbBVPNw2XZbNk7KgEAA7irgYHsOyNAtlKVFfSANyKxUaJ5UZO2ezspq4G3ij0Op1hl2CkWVr05qD53CFlXQEskM1SC3NgAUwHZhpI1tL3HZ9IAlOyMFKa-z6+7JVm-QGTJQILd+A0TCQ4NAkhl7CYQPSgA)
* You may see some errors moved around when `$Exact<...>` is involved. Some invalid `$Exact<...>` type will now have `not-an-object` error code.
* Fixed a bug that makes some exported types accidentally any. Previously hidden errors might be exposed.

New Features:
* `declare namespace` support is enabled by default.
* Added "Add Missing Attributes" quickfix for JSX. This is available when the cursor is over the name of a JSX element that is missing a required attribute.
* Added a `StringSuffix` type, which goes along with the recently added `StringPrefix` type. `StringSuffix` can be used to type strings with a specified suffix. E.g. `StringSuffix<'!'>` allows for `'yay!'` and `'woo!'`. The type argument for the suffix must be a string literal. The second, optional, type argument can be used for the type of the remainder of the string after the suffix is removed.

Notable bug fixes:
* Fix a bug that caused Flow to infer incorrect render types for elements of polymorphic components under niche circumstances

IDE:
* Flow now supports symlinked node_modules in autoimports better. If allowed by the node resolution algorithm, autoimport will insert import specifier like `my-package` or `my-package/foo` instead of a relative path import.

Library Definitions:
* CredMgmtCredentialRequestOptions properties are now optional
* `Reflect.setPrototypeof` are no longer special-cased, and the backing special type `Object$SetPrototypeOf` is removed. The new typing is intentionally loose. Similar to `Object.setPrototypeof`, this is inherently unsafe since Flow won't track prototype mutations.
* `Object.assign` is still special cased, but the special-casing is only available for a syntactic `Object.assign(...)` call. If you try to use Object.assign in some other ways (e.g. `const f = Object.assign; f(...)`), you will get a a less precise and accurate typing for it. If you don't like the default fallback type, you can override the `Object$Assign` type. While the special-casing behavior will stay for the syntactic call, you should migrate your code to use the spread syntax instead.

### 0.241.0

No changes from 0.240.0

### 0.240.0

Likely to cause new Flow errors:
* We fixed a bug that causes Flow to incorrectly believe that the LHS of logical operators are not nullable. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SRWxgABAA5bS4VrYAA8ABUAHzsgC87MF7IAPuyjrgANwAHSgGieVGw7Ns9jlPL5yC5erQbX59lMUAQwuVj2gOvlfIAylIKggAPKUbkiG7i3XHPnsgD8AfZAAZrfbjdgnea3R7riZ2RgSOyzS6FeyAPQZ9kmSgQSgq3IgBomEhwaBJBohqwAJgAzABOKwARhA9KAA)
* Flow can now detect more kinds of bad cyclic types. ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABLZ7JyACokZCcgCC1DoAB4+QA+ADcnIA9PLOSZKBAWGxOTgiAB3AA6PlW-M5AF5OQAKFbGCAwfkkACUAG1hCIbgBdOWK5XUNVCsC0J7ZXIgBomEhwaBJDL2EwgelAA))
* Flow will now consistently catch some invalid comparison of the kind `obj.type === 'foo'`. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SRWxgABAAxYjsgC87IA5NyIAL2QAfQUAISoAoA3AAdHyrdkAeT57IAJAAlVhsFVQQYAHmAEGQmp17H1RuAbOwZoA-ML6QA+F0KqCK2z2VWSgBS6uAnJVKrNApgxAF9PdnvSjGgFnZABV2hIABQQNAAKwdKoA2gKRQBdACU7OAivZlfZcBg7PTWftww5vJbPt9ViDKtLwHZAHpe+yoBB2SZKBBKOycOGWLwR9Rx4OIAB3CtVtcZzPutf01eVlgSISUKDundQXIgBomEhwaBJBoABisACYAMwADisL5A9KAA)
* Fixed a bug in the type guard refinement that involves sentinel properties (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQAOhJWbAHso4NAAARkeJfAgseBfbwSAAUAEo+cBOXy+StjHyACqrACCfIAvHyACQAJVYbAA8lBBgAeYBK1bIPkAcnVxg19t4fLgbDt9lMUAQfPpAD4ANwKm0qp3YABCOv1RvYZst1uV2DtjtWkdd7s9AqkFT9gZDUEVybVqwAwjHDcaE7QrWGUw6I2XMx6vbnff7g5zQyWNccIyQYxGtQAfUvGaNjpuF4urPkAZWMYBauAHQ81fKn6cLoY0Tyo2D5MG58KLtMXcxXkrgCxEdr7q9WJGldpv2BE7sHF+X3AHO6LR4nryRYALLAnAjDaJk2A1gAkjA35Xm+d58g+A6yvKAGKnAMB8teJCIdw163tKGGhoqioAPSUe6t58gA7nAxx8lAED0XyODjtgo5cZGbo4DAEAsLRDEYIOABW+QSFxGrkRRyF8mJMlBny1F8gAcmxDFMbgHGHsCfImJQQkUYqcn0oZ3RynJVE0QptJcWWNm0e+ilfkuK5-qpNFGSZTZ8gAfgGC4eb+qzOQpSloU+KlqRAWx0KZZlYf6ob0py9K5CADQmCQwFJA0AAMVgAEwAMwAJxWIVID0kAA))
* `Function.prototype.apply` and `Function.prototype.call` are now typed using userland types instead of hardcoded builtin support. You might see slight inference differences, error message and code changing as a result.
* Flow now enforces that only function types are assigned to the `.constructor` property of object types. (D58538540 pvekris)
* Fix subtyping checks of unions of bigint types against other unions of bigints.
* Make array holes be typed as `void`, not `empty`.

New Features:
* You can now pass in an (optional) second type argument to the `StringPrefix` utility type. This is the type of the remainder of the string after the prefix is removed. For example, `"$1"`, `"$2"` are subtypes of `StringPrefix<'$', '1' | '2'>`, but `"$999"` is not. `StringPrefix<X>` is equivalent to `StringPrefix<X, string>`
* You can use the `$Values` utility type to get the type of the elements of an array type or tuple type.
* You can now do sentinel refinements on tuples, like you can already do for objects. This is especially useful for function arguments (you can type the `...args` as a union of tuples). [[example]](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABLZ7Jz5MhOQBtADkGGFvE5whENwAupyAD5C4VocWc+ymKAIGUAbgAOlA4DBOQAKeSCgAMcoAvDbOaLhQBKTnAfWct18wUARjlGBIkuuJm1nIA9MHOQB5ADS+vpnLwZGdrvdZu9nN9aqkFQQQdDEejUHp+v1uYAolA2MsIItBGw44oRIw2pz8lnOTAHso4NA05QECR9e3HvCoNzuGBjVZJ1Q+wKRVgSKqpbKFUr2GxFwHKBKlyYZQ6BTvKImR5zDSbpyQLdbbaK0AunS6T26WBIhJQRwBZTK1Kzz40Xr09z1E9Y3jLlH3dTkXzfEcANTABqHs+0FAAmHUk0LAsi0eMdjVvBcJUWFCHRzMMo31apcHHfDVSIiVPRIkMwxLAAlFjwxY7DKOotdVQAZglAAWRjc3InCqLw3iJQATlkkTmLYjjchABoTBILsoCSDJ7BMEB6SAA)
* Sentinel refinements now work when using number keys on objects [[example]](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABLZ7Jz5MhOcAAAwCgDkGFFvE5AEYBcIRDd6ZyAD6CkWc0VoSUygX2UxQBD0gDcAB0oGa4DBOQAKeQAbSFAF1OQBeN0aiUASkFZs5fr5dulzowJE58puRs5AHoo5yAPIAaTNSrwZB9UH9AaDnJDnL1FQQkZj8aTUHpuRADRMJDg0CSGXsJhA9KAA)
* Flow now keep original type after invalid object prop-exists refinement on write-only property.

Notable bug fixes:
* Some operations can now be performed on an opaque type with appropriate bounds. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQAOhJWVBgJyAAQ8jRPKjYHmwjAARxkPJWxh5ABUAIzIHnAGDEJX2UxQBD0gDcvP5c1wQp5tnsIqVCr1UD5AApgFYHRB6QBKHkYEjK1UQdVSCranU8gD0gZFW31dodVidrvdPOwIjhtADwbj1AglE59M5nO51oNgpYIscEuF0uFsoATEqAEqsZRWADC6UY0AsstWAB4VWqeRq-fSAHxWvkCo2F00SHlemt1mIAUTa3gkTdgiA7laH+q9bo93e9vd9Wt1QZDEDDee3sfjieTIZMlHTmezXP1o+NopLUtWcoAzEqAOSYJQ-7DvmY7ChOU49sAQE+pq-qvoaxqQTstAWj+oFegA2qhAC6O4HvBt6hluxA4dgtD4VeCYrMR96PlA9K5CADQmCQcDQEkGT2CYID0kAA)
* Fixed a bug that occurs when you have a tuple type spread after two or more non-spread arguments. Those will now be resolved in the correct order.

Library Definitions:
* `String.codePointAt` now returns `number | void`

### 0.239.1

Notable bug fixes:
* Fixed a bug where IDE services doesn't respect `files.implicitly_include_root=false` config.

### 0.239.0

Likely to cause new Flow errors:
* We now detect errors when props of React components using component syntax, or hook argument/return values of React hooks using hook syntax, contain arrays, sets, or maps that are mutated using their methods (`Array.push`, `Set.add`, `Map.set`, etc). These values are expected to be read-only, and we previously errored on directly setting their props; this release extends this enforcement to method calls as well.
* We are adding more strict checking for type guard functions to account for the refinement happening in the else branch of conditionals that use them (see [docs](https://flow.org/en/docs/types/type-guards/#predicate-type-is-consistent-with-the-parameter-type) for more information).  You might see new errors appear in the declaration of type guards. One way to address these is by turning the type guard definition to [one-sided](https://flow.org/en/docs/types/type-guards/#one-sided-type-guards), by adding `implies` before the type guard (example [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SRgD2UcGgAAJaQAFUhwLINbAACigyC5AH5hCIbgBKCVQHkkLkym5c4BcgD0Wq5AHc4MdVRA9VyTJQIJQADpKrksCRCShKpUAQgAvKrtLguQAyH2qrkAPi5AAYANw2+k2m1sx7wpV8gVC7AAMWKpTFEul1xMCp5IkYuDg7QDtM9spMGu1uvgJTYXPyFQQXIABmZC8WSC2bVy7btHc6ue7PUa-QHg+HI7kQML8ZyoEkGiGrAAmADMAA4rABGED0oA))

New Features:
* The `StringPrefix` type represents strings which being with the specified prefix. E.g. `StringPrefix<'data-'>` allows for `'data-floo'` and `'data-bar'`. The type argument for this type must be a string literal. [[example]](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SRWxgABAARTIYAAKlFh7IAvOyAMpSCoIfmaYoAHgA5GweYt5QA+ADcAB0oNrFcr5PL2RgSFyefzYer2QB6K3sgDyAGldfJFkrLIsDUaTdzLObGJabeyAKIAJRDdpDuRADRMJDg0CSGXsJhA9KAA)
* Flow now supports [`globalThis`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/globalThis).

Notable bug fixes:
* Fixed an issue since 0.232.0 that will cause failure to connect to Flow server if libdef has parse errors.
* Made the "extract to function" refactoring more robust to errors during code synthesis.
* Fixed a bug that can cause hover to hang forever for recursive namespaces. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABFBnO1HJDOQBlTnAAA6UE5nLZuA5nNs9k5JGQnJWxggMCFAG5xfTciAGiYSHBoEkMvYTCB6UA)
* Go-to-definition on `new C()` will jump to definition of `C` instead of the constructor definition. Hovering on `C` will still show the jsdoc on the constructor.
* Strip `as const` casts and `as` casts in `flow-remove-types`

IDE:
* Hover will show a list of all the symbols found in the inferred type and the locations where they are defined. VSCode LSP and CLI are supported. The LSP version includes a link to the definition. The CLI version only shows the name of the file (no path)

Library Definitions:
* Calling `shift()` and `pop()` on an `Array<T>` will now return `T | undefined`.
* Add `cause` property on Error instance; Support error cause in error constructor options
* Add type definition for `FinalizationRegistry`
* Add type definition for `CSSSupportsRule`
* Add `closeAllConnections`, `closeIdleConnections` to `https$Server`

### 0.238.3

Misc:
* In v0.238.1 and v0.238.2, we have bumped the required GLIBC version to v2.35 on Linux. We have now reduced the requirement to v2.31 for x86_64 build of Linux.

### 0.238.2

Notable bug fixes:
* Fixed a bug where a recheck triggered by `flow force-recheck` doesn't respect `files.implicitly_include_root=false` config.
* Fixed a bug that causes missing results in find-ref and rename.

### 0.238.1

This release is used to test the release automation powered by GitHub Actions. No changes are expected.

### 0.238.0

Likely to cause new Flow errors:
* In v0.237.2, we shipped the flag `react.disable_function_components_default_props` that will make Flow ignore `defaultProps` on function components for React component typing purposes. This flag is now on by default. If this is too disruptive for your codebase, you can turn this off for now. Note that we do not intend to keep this flag forever, so you should try to turn this flag on as soon as possible.
* We made `React.createElement` have an opaque type, which means calling this function directly will be an error. This will be helpful to prepare for [React 19 changes](https://react.dev/blog/2024/04/25/react-19-upgrade-guide#new-jsx-transform-is-now-required). Note that JSX typing is unaffected. If you are not ready for this change, you can override your library definition like [this](https://github.com/meta/flow/blob/194f63ebb7829f404d9c445c12921ae7a258b0f8/tests/react_create_element/lib.js#L1-L38) to approximate the previous behavior as much as possible.
* When there is an `invalid-component-prop` error, Flow will no longer make the entire props to be any type. Instead, Flow will create a Props type as if these invalid props do not exist. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SUgIkY0AsAAIAMIAChgxGQXPspigCF4XKs0vwwuAgogwuEIhu9IAlFzgAAdKBcvVclgSISUKAAbh19J1OoAPDyuQqALzAKQyelcgD0AD5TR73VyoBAAO5ckyUCD43IgBomEhwaBJBoABisACYAMwAdlTIHpQA)
* We are making the typing of the exported object of a module more strict. If you see errors when casting the exported type of a module to a writable object, try either casting it to a readonly version instead, or casting the exported object to the writable object type.
* We now infer the type for `Object.freeze({ A: "a", B: "b" })` as `{+A: "a", +B: "b"}`, ie we use readonly properties and singleton types for literal initializers. Casts like [this example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SVs9gABBA0AArdkAXnZAHkeXMYjAWNgAF7YAAUwHZAB0QABBJXIRUqpXs+kASgA3AqoFzeRgSOz5cr1fZTFAENqDVBciAGiYSHBoEkGgAGKwAJgAzAB2P0gelAA) will now be an error.
* We now error more consistently when a property is missing from an intersection of objects (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABIDOTBiMhOcB6ZyAGSC+kAbgAOrBiFY0BLOQB6JWckyUCCUTkYEicgDuXJItQg2jYMr5ECsACFFSr9Vy2HA2JyoBAJGrqJrObUTNgZbkQA0TCQ4NAkg0AAxWABMAGYAOwxkD0oA))
* `untyped-import` errors are now on the import source rather than imported names.

New Features:
* Under `files.implicitly_include_root=false` (default is true, which is the current behavior), Flow will no longer include everything under the directory where the flowconfig is in by default.

Notable bug fixes:
* Fixed a category of spurious errors when using `Array.reduce` and returning a union type (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABIDOQ84NBkJyAILUOgAHnspigCAAfJyAD7C0W0MXCEQ3GUAbgAOlA2bgOZzbPZOVRKIKRTQVZKKrKdVBdcaJJyWCRtBIAIycgC8puosVKQkhAApg0owBblRKpLaZbxOfJBTbpQBKH1y4CcgD0Wc5AHdoAByZ04U1QTkmSgQShl2giavYXWc5su3ZCSjlvnQe2MzkAbQAuin7Y7oCbXe6AEw+v2UANsIPYUPhyNW6NS2XxxOc5MINPejPZ3MFqDFzml4EV6jV2v1lhNltwGCc4MABk5Ys5HrTwAfLZbLASO25Z9lYYHhgO9r-vSf6tkBHb9mBVjhlukG6r2g4prquQgA0JgkPyUBJA0r5WJOADMADsZEgPSQA))
* Explicit type arguments in JSX are now considered for contextual typing. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABDAHso4NBOQAxYgAHgAKgA+AAUwBgyE5UrFAEpOQBeCWchoQOBselK+VanUAbgAOlAzSLhRARfZTFAEBqYKrgFKGir1ZzgPT6ZyAPQa32+zkmSgQSicnAwMOyTkAdzDWxInKgEFjZtyIAaJhI-KgSQaAAYrAAmADMAHYSyB6UA)
* Fixed spread of empty array when calculating tuple elements, e.g. `[...[]] as []` now works.
* `nested-component` errors can now be suppressed.

IDE:
* Flow now suggests properties of objects that are contextually typed with optional types. For example, in `({ foo: { |  } } as ?{ foo: { x: number }})` it will populate `x`, when calling autocomplete at the point of the cursor `|`. ([try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQAOhJWT5VgACABixG5AF5ucBufJkNzhCIbtz6QBuTmclbGbkAISoQu5AH5RTBiBL+RBZQqoJzbPZufgJerKJrdfqRdynfTjZyAPRup1e70+31OgB6fKIAHducG4MdJRBQyQhAgcRaAAbyRPc2omRL8BomEhwaBJBoABisACYAMwAdlLIHpQA))
* We now provide autocomplete for types in the global `React` type-only namespace.
* We now support go-to-definition for JSX member expression like `<Foo.Bar />`.
* Re-exported values from `export {Foo} from 'bar'` statements will now appear in autoimport results.
* `autoimports_ranked_by_usage` is now enabled by default.
* Flow will now show the type of the constructor of a class (instantiated if it is generic) when hovering over the "new" keyword in `new C(e)`
* Hover types will now show the specialized version of a polymorphic component used at a JSX expression
* Hover types won't add unnecessary parentheses around union types.

Library Definitions:
* Add React 19 `useOptimistic` and `useActionState` API definitions
* Add libdef for `TextDecoderStream`

### 0.237.2

New Features:
* We added a flag to help prepare your codebase for React 19's defaultProps change. [defaultProps on function components will no longer work in React 19](https://react.dev/blog/2024/04/25/react-19-upgrade-guide#removed-proptypes-and-defaultprops). Under `
react.disable_function_components_default_props=true`, Flow will ignore `defaultProps` on function components for React typing purposes. Note that we do not intend to keep this flag forever, so you should try to turn this flag on as soon as possible.

### 0.237.1

Notable bug fixes:
* Fixed an issue that might cause connecting to flow server to fail with out of retries.

### 0.237.0

Likely to cause new Flow errors:
* Flow will no longer break up a union argument when calling an overloaded function. The code in this [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABDAHso4NBucQABTyZCc4QiG4ASmQDQgcDYAG4ADpQNm4Dnc3nwqCCiAisX2UxQBAyuUKlVQVXqzWAzmi8XXEycgA+nKNFQQlpgwvkUsVnIA9IHOQB3ODHcUQUOckyUCC3fgNEwkflQJINAAMVgATABmABsVkzIHpQA) will now be an error.

New Features:
* Under `jest_integration=true`, Flow will now error on some jest mocking APIs (e.g. `jest.mock('./my-module')`) if the module doesn't exist.

Notable bug fixes:
* Fixed spread of empty array when calculating tuple elements, e.g. `[...[]] as []` now works.
* Fixed inferred type of `React.ElementConfig/Props` when passed in a component syntax component (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABJARIxoBZOQAxYgACigyE5whENwAlABuAA6UCVbNwHM5gO50HgCAlACVWMorABRNreCQAYW1iAAPCtjBAYELiAA+RWPa0ITkYEic4DiyXXEycgA+nPspigCHpcs5AHo45yAO5wY6SiBJzkmSgQSgSqCc2npiScqJsGFQQaFgv+-NByih8NSCrRpUquZqlgaqic6mMEj6w0xU3Yc0ABRz-btq0dzogbqVfd9Pr9AalN0bEZbMfjiZTaagGaz1Fz+cLvsPJbLFarFVXdelwbDW6j9KVuRADRMJDg0CSGXsEwQHpIA))

### 0.236.0

Likely to cause new Flow errors:
* Ensure React class component constructors call `super` with a props object. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0STMjAglAkAAIAFRcjAkLkAJVYyi5MEo6S5AB0QFFlLKANzSqAqlbGLkAMWIAAVJYxBQBeLnAGDEZBc+ymKAIenK1VfIwc7lPAWC7UQLkKBZQNiCkXRKwAYXS7K+PgAPB69bCSAA+E0qrlc2xWoTKDkACmpBot0f1JAAlImoMnkyQhJ5M4Wk2WAPR1rkAPS5ABE4GwfFzHCRBTmSLX6Sqhw6VQp2ZyU7g3VriAAmL3yH1+4WimIhkRhixR3UFhPAWupqTptKUbMFvO72PFg+lssVqv9wv25Mj+kyqC5EANEwkODQJIMnsEwQHpIA)
* Modified typing of `React.cloneElement`. You might see errors being shifted around, and some advanced patterns no longer supported. [The API is not encouraged by React](https://react.dev/reference/react/cloneElement), so migrating away from it is recommended.
* For JSX elements under `react.runtime=classic`, we now type check the JSX unconditionally using the right `React.createElement` typing, and check whether the locally defined React has the right `React.createElement` definition. If you have some JSX that's already invalid, you might see different errors. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFHBcDBISODAQfiEyfBE4eWw2fDgofDAMCTAAC3wANyo4LFxscWQuHgMNOvxoXFp-WmN8FmZXTv4NZmwKiSKAWgkhxPa3AxIAa2wq6tp54VxccfdN7Zq94REcSmODISgwFqpF5dvsqCgZ1pIqPdKG0wYHxvED3T5gb6-eYQRgSODQbjzGoYUog+4-GDYeYIbZzSgkeZkCR4toddz3HILZgQfJkW4AX34kA0SWAAB0oAACbmcgA8bDgdU5AHoAHyc1KcgBKrGUABIAKItbwSXkAciYQLVooA3ByeSLhZyAEKaCAsTkABisACYAMwANmQnL6tE5Jko5pdXNp6SgCGlsok+p5wqN1vtTvd1C90E5vtKAZl0V4EqgbED0UVyos6oFdW1fOdyflSuwKvVmqg2o59I5HPZXJ5tnsmeUnIAvJzgGAonMyyrnWrqnA1fS9U3ufzBSLxZKSxJs+XcxrHNXdSHuWGTWaLRHHc77KZ-Zy4CROVAIBIJZyYPdlPCoJvDVbbQfo57KN7OZgMwubH22ADhYqZAn+QZLhWar5oWvLFhBwE+JWa41lAdZQFkIB1CY6TQEkIipHiID0kAA)
* `React$CreateClass` type, aliased to any since v0.176, is removed.
* Fix typing of `isValid` Flow Enums method, reverting back to behavior from before v0.232.
* Support for `$CharSet`, an undocumented feature, has been removed. `RegExp$flags`, which depends on `$CharSet`, is also removed. In most cases, you should replace these types with `string`.
* Support for `--traces` and the corresponding flowconfig option has been removed.
* `React.createFactory` libdef is removed. This API is already deprecated and will be removed in React 19.

Notable bug fixes:
* Fixed interactions of various types with specialized versions of `mixed` (such as `$NonMaybeType<mixed>`). ([try-Flow examples](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SRgD2UcGgAAIFvZ8AA5aAAWToOAAFABKLnAAA6UC5XIALAAmLkYEhc4QiG5cgA+mu0uDVGoAJIKoCLaDgACqrAA8RRKbAAfABuLkAeg9XJMlAglANxy5AD9nVyhcVStauWLzZacBK5XKFSrjQbtSY9VyGhA4Gw02bhaLsLbjA7Iy73V6fdR-VyHhp4F986HwxXo7Gi1bsImoMmlar1VyAPxanVDwsW4ul7Dlp1uz3e-kAUQA6jW-QGjkb9Q3NBVSiGwxGnR248Xe-Sk7B2fD5byVGzHnexTAoMguY7SlLZfKuXAYBjFZjAgQC3y5ABeKCuRlEAnw5aBYJ-fsFQVWx7C5aBsEgrkAEY02A7BQK5N8qyXNcNzrMdM1bE8oxjOiyngu9e1Q1CsKsDBGEYQZXX7K8oAEuVmM5e92hUKRUlqWgxXkD8vzYZC-wAmN5CUti0OgDCyXIHCdII1ZiPkMiuRXddfTrfTaPbBiK3wSSJGk1iNJUsUAEIdPwH9Fx5WpBC5NAaEeWoeQwHY+z-DTUM8tNsBEOFaD4yLUIEhUBIE3IQAaEwSFEpIGgABisZUAGYAFYrFwkB6SAA))

Misc:
* Support removing `export type *` (ExportAllDeclaration) nodes with `flow-remove-types` (thanks @jbroma!)

Parser:
* Updated bigint literal AST output for `bigint` property to match ESTree spec. Numeric separators are removed (`_`), and should contain only decimal digits.

IDE:
* Under `component_syntax=true`, autocomplete will provide `component` and `hook` keyword in appropriate places.

Library Definitions:
* `$asyncIterator`, which is never a real global, is removed from global libdef.
* We removed `$await` in the libdef. If you are depending on it like `typeof $await`, you can replace it with `<T>(Promise<T> | T) => T`.

### 0.235.1

Misc:
* By default, `flow ast` will now parse component syntax rather than reject it. You can pass `--no-component-syntax` to get the old behavior (D56501290 samzhou19815)

### 0.235.0

Likely to cause new Flow errors:
* Flow's react-rule enforcement now detects reads from `ref.current` within nested functions of hooks and components that are called during rendering.
* `obj[key]` where `obj` is not a dictionary object and `key` is a string used to not error and just silently return `any`. We now error on this pattern.

### 0.234.0

Likely to cause new Flow errors:
* Flow might catch more inexact incompatible with exact errors. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SQ0Tyo2AABLZ7JyKgposhOQAKYBWcX0gCUnIAvAA+TkNCBwNgAbgAOlBNWzcBzudBeQLlABGIXC42chQLKBsEici0Afk5AAUqFluAAeYD0hVC43S+WK5Vq7VzXUsfV2CSWxTKABMZu9AYVSpVGq+sYkFowdpWxggMD5Geiqs5AHoy5yoBBLdQIJROTgYPXZLXKPWqxAAO6ao0SOOcnOcvPYAtFvuliuRrLCLlpNv1zW5EANEwkODQJIZewmED0oA)
* Fixed a bug that leads to Flow sometimes ignoring differences in call props of an object. New errors might be exposed. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SRWxgABAAVWoPLac1YAHgASgA+dkAXnZwHZAAoAJTIdlC9n0gDcAB0fKt2QBZWjc3n84zCsWSg1QPmC0UaqAaJ5UbDs2z2XX6nkWxV682W43CEQ3EU2r3urbsjAkLkho3YAX2UxQBCB9kAemT7JMlAglEVfpu7IAfmK4xUkPwGiYSHBoEkMvYTCB6UA)
* We rewrite the way we generate types for annotations. We will now detect and error on trivially recursive types like `type T = T` or `type Foo = typeof foo; const foo: Foo = ...`. In addition to this, you might see some errors being moved around.
* Fixed Flow Enums exhaustive checking logic when input is a generic.
* Invalid indexed access types with string index, like {foo: string}[string], will now error instead of silently making it `any`.

New Features:
* You can have a tuple spread of a generic in the parameter of a function and not have to supply the relevant type argument if that spread is the only spread in that tuple, and it is the last element of that tuple.
* Negative numbers are now allowed in Flow Enum values.
* Allow Flow Enums to be cast to their representation type when the cast expression is typed as a generic type.
* Added a new global type `EnumValue<>`, which represents all Flow Enum values. You can constrain the enum values to those with a particular representation type by supplying the type argument, e.g. `EnumValue<string>`.
* Added a new global type `Enum<>`, which represents all Flow Enums (this is a complement to the new `EnumValue` type). You can constrain the enums to ones which have a particular enum value, e.g. `Enum<>` is the same as `Enum<EnumValue<>>`, and the type of all enums with string representation types is `Enum<EnumValue<string>>`. These "abstract enums" have no know members, so you can't access members or exhaustively check them, but you can use methods such as `.members()`, `.cast()`, etc.
* Allow `===` comparison of abstract Flow Enums as well as Enums themselves with the same ID.
* You can now cast Flow Enum values to their representation type using `.valueOf()`, e.g. `enum E {A, B}; const s: string = E.A.valueOf()`.

Notable bug fixes:
* We now error more reliably in the matching property tests, for example when the object part is a complex expression. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCLWMAAIADJwQHcLEAXixAHIYMRSQBuAA6UDpMAWyjg0CxkBEjFK8nwClq1xZUCxAAo6VisfJkMKAJTEgB8uPxJm4vFFWIgkqFMqJ8uAjEleIJuBRqpEkt1ZoAjvrFTQjcaoDLgKq4DBhfJNcSiSTyZTHUjna6he6tV6yZhKKS-ViAPTRrFQCAAdyxJkoECBAbdHq9JIAjFHY-GkynqOmSHTM0KIJqrIxPd6KRBI1jgP7BViXcLq1La-Ww1Rm62Y3GE8nU2XK93ezmsfmW0jh0Wx6WM-T252hSJa1YLX2fU2-ZWt4wd3vw4OF4XRyW06uxRvj6eZ3Oh1fi+PV0iYiB8iYSAKgnyAAGKwACYAGZwKsICQCRIA)
* Fixed spurious errors in propagating type hints. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCAB0oBollRsAACWz2fERRjIfHAfEAbXspigCAAuuSAILUOgAHlp6QQAD58UiANxYrE43B4-HHfFQZzYclc+n4gA++OwIgetCFUFJVOl5gZ+IAvNSGQL8QB6M34gDucGGUogEhV1AglHxp1oJJd2GF2LqYpYEqoUplrMoLLZtE5Um5Sql0xMPM1uuwoasMBdAFElDkABTJw184BY-H4uAwfE5lrGCDl-MG+v4jEgeUIJsASgpxZLJIwjB1Mv1RqpJvNlptdqgDqdlBdbqgHpEXq7SKxSLbApiIHyJhIcGgQXyAAYrAAmADMZ6sh5ASKAA)
* Infer type in a conditional type is now allowed to be underconstrained. When the true branch is taken, this underconstrained infer type will be pinned to the upper bound. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+h46fNRLuKxJIGWh8MeT0ZfhYlCStpHzNsFBAMIQkIEQwJODAQfiEyfBE4eWw2fDgofDBMsAALfAA3KjgsXGxxZC4eAw0G-GhcWn9aY3wWZldu-g1mbGqJUoBaCRHEzrcDEgBrbAk62kXhXFxJ923d-cPRHEpTgyEoMDaqZdW7vKgoOfaSKgOKpqmDA+d4gB5fMA-P6LCCMLLQbiLOoYCqgh6-GDYRYIXYLSgkRZkCR4jpddwPfJLZjpOBkO4AX34kA0SRWxgABAAJPCeACMAB4ACoAPnZAF52YL2QoFlA2CR2UdcOyAD7sgDaFUxlHZAA0ALrsgD8evZyEV1xMAG4ADo+VbsgBixHFXJ5JgFSuFVvZAHpfeyIFteOzZg04BB8oNpdQIJQ7bz2RgFc6ID7-YGtubU66iiU2Ha7WzsG7cJ4AExC0USqUywTyi3HVUarUmU11uUK+ymKAIQ0m3Vmi0iG62+0cgBCVFd3LLJkrXvTAaDIbDEajtBjlDjCaTCqnlCXW7j5oPru7FQQdtyIAaJhIEagSQy9hMIHpQA)
* Add `Enum` and `EnumValue` to `$NotNullOrVoid` - previously there was no supertype for all enum values or enums.

IDE:
* We now show jsdoc information on component props with component syntax.

### 0.233.0

Likely to cause new Flow errors:
  * Flow will now error on invalid type applications at the declaration site, if the type application does not use another generic type from an outer scope. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCLWMAAIACpYgC8WPspigCAA3AAdPxrLEAQQJuIAPMT0ggAHxkrEAei5WJMlAglCxUAgAHcYiB8iYSHBoEEIvYTCAkUA)
  * We fixed a bug where we incorrectly unwrapped the opaque type to the underlying representation in refinements
  * Predicate functions (%checks) can no longer be recursive
  * Fixed subtyping of indexers in instances/interfaces

Notable bug fixes:
  * We fixed some spurious errors regarding opaque type refinements. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCMAWyjg0AABA17PgWPAdvgwvghrgETkABQASlxwAAOlBcbiNEsqNhcfcMABHGT4ta4gBixAA3Cy2RzcFzcbZ7LiYMRkLiAPxiiCSqBS3FwGC4mnKiC4gC8ptxlIZzNZbKVxFxGBI6vspigCHFuIA9F6eV1eLiYWx8SaTJQIJRdUiWdGoDEQPkTCQcXHkCAIvYTCAkUA)
  * We made a change to method-unbinding errors so that it no longer errors when in an any/mixed context. [This allows us to better support jest](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCGiWVGwAAIYAtlHBoDiFMZlAAKeTIHGnWgASmp+QgcDYAG4ADpQTlY644gBixBxwE5OJxmEo5LpQpxSM5sq5i2g9jxgoAvDidgB3fnESUchWkuoSckwYhWcV01k4gD01pxmrgww1EBxRCg-0oJOoEFm-HyJhIRKgQXyAAYrAAmADMEasoZASKAA)
  * We fixed an issue where explicitly supplied JSX type arguments were ignored
  * We fixed bigint Flow Enums that do not explicitly speicy `of bigint` during parsing. We were previously not correctly inferring that this was a bigint enum.
  * flow.org/try now supports go-to-definition, autocomplete, and signature help

Parser:
  * Previously Flow outputs `ReadOnlyTypeAnnotation` in parser output for annotations like `readonly string[]`. Now we output it as `TypeOperator` with `"operator": "readonly"`

IDE:
  * Fix printer output of casting syntax when LHS of `as` or `as const` has low precedence

Library Definitions:
  * We migrated the built-in React library definitions to use component and hook syntax for various React-provided APIs. Look out for a blog post on our medium account for more details and see the [React section in our docs](https://flow.org/en/docs/react/component-syntax/)

### 0.232.0

Likely to cause new Flow errors:

* Support for `$Compose` and `$ComposeReverse` types are removed. We recommend to use overloads to approximate their behavior instead. e.g.

```
declare export function compose(): <T>(a: T) => T;
declare export function compose<F: (...$ReadOnlyArray<empty>) => mixed>(
  f: F,
): F;
declare export function compose<A, T: $ReadOnlyArray<any>, R>(
  f1: (a: A) => R,
  f2: (...T) => A,
): (...T) => R;
declare export function compose<A, B, T: $ReadOnlyArray<any>, R>(
  f1: (b: B) => R,
  f2: (a: A) => B,
  f3: (...T) => A,
): (...T) => R;
declare export function compose<A, B, C, T: $ReadOnlyArray<any>, R>(
  f1: (c: C) => R,
  f2: (b: B) => C,
  f3: (a: A) => B,
  f4: (...T) => A,
): (...T) => R;
declare export function compose<R>(
  f1: (b: any) => R,
  ...funcs: $ReadOnlyArray<(...$ReadOnlyArray<empty>) => mixed>
): (...$ReadOnlyArray<any>) => R;
declare export function compose<R>(
  ...funcs: $ReadOnlyArray<(...$ReadOnlyArray<empty>) => mixed>
): (...$ReadOnlyArray<any>) => R;
```

* You might see more errors around type application. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCLWMAAIAGLEAA8ABUAHxYgC8WOAWJgxGQWMJWKRAG4ADp+NZYgBCVCJpIpVMwlDpDOZbLZmOwXIwXSJ5KxiWSbD5uIJ3MovOJrKgbI0Syokts9ix8jp3JlmrZ8ixGBIUpl2BED1omqxAHpXVioBAsSZKBBKFj0liAAxWABMAGYAIxWYO8H3Uf2eiAAdzpCpSWPxAD9SQ6nWyYiB8iYSHBoEF8qHIzHgyAkUA)
* Fix subtyping of indexers in instances/interfaces. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCGiWVGwAAJbPYcfJkDj0oCuGBccAANr2UxQBAAXWJwhEMxxAB8cbT0ggkQBuAA6UHkOIwJBJfhM5MpNKkPKZOJZM35OIA9KqcQA5CAAdxxJkoECBMRA+RMJDg0CCEXsJhASKAA)

New Features:
* Updated the `isValid` Flow Enums method to use a type guard, allowing it to refine its input to the enum type in a conditional context.
E.g.

```
enum Status {Active, Off}
const s = "Active";
if (Status.isValid(s)) {
  s as Status; // Should work
}
```

* `export type Foo = ...` and `export interface Bar {...}` statements are now allowed in `declare module` and `declare namespace` bodies.
* We added a new codemod `flow codemod remove-unnecessary-react-import` which can help remove unnecessary react imports under `react.runtime=automatic`

Notable bug fixes:
* Fixed issue when explicitly supplied JSX type arguments are ignored
* Fixed code action output of casting syntax when LHS of `as` or `as const` has low precedence

Library Definitions:
* Added definitions for the following APIs:
  * `window.showOpenFilePicker()`: [MDN](https://developer.mozilla.org/en-US/docs/Web/API/window/showOpenFilePicker), [WICG](https://wicg.github.io/file-system-access/#api-showopenfilepicker)
  * `window.showSaveFilePicker()`: [MDN](https://developer.mozilla.org/en-US/docs/Web/API/Window/showSaveFilePicker), [WICG](https://wicg.github.io/file-system-access/#api-showsavefilepicker)
  * `window.showDirectoryPicker()`: [MDN](https://developer.mozilla.org/en-US/docs/Web/API/window/showDirectoryPicker), [WICG](https://wicg.github.io/file-system-access/#api-showdirectorypicker)
  * `DataTransferItem.getAsFileSystemHandle()`: [MDN](https://developer.mozilla.org/en-US/docs/Web/API/DataTransferItem/getAsFileSystemHandle), [WICG](https://wicg.github.io/file-system-access/#dom-datatransferitem-getasfilesystemhandle)
  * `StorageManager.getDirectory()`: [MDN](https://developer.mozilla.org/en-US/docs/Web/API/StorageManager/getDirectory), [WHATWG](https://fs.spec.whatwg.org/#dom-storagemanager-getdirectory)

### 0.231.0

Likely to cause new Flow errors:
* Fixed subtyping of inline interfaces, e.g. [try-flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCLWMAAIACpYgC8WPSgK4YGwWOAMGIyCxwhEMyRAG4ADpQDRLKjk2z2LHyGk4llQeRYjAkIl+Eyk8nATCUGloYjLKBMrEAelVWIAyjkINo2FiTJQILN+PkTCQ4NAgvkAAxWABMAGY7TaQEigA).

New Features:
* No longer trigger `method-unbinding` errors with indexed access types.
* Add `relay_integration.esmodules` option. When this option, along with `relay_integration`, is enabled, Flow will treat `graphql` fragments/queries/etc. as `import default` rather than `require`. Use this when you output ESM from Relay's codegen (e.g. `eagerEsModules`).

Notable bug fixes:
* Don't error when returning an array, map, etc when an `AsyncGenerator<T, void, void>` type is expected (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCAB0JBioBgSLRFgACGALZRwaCEgBU-ygAEYABQASmQhIAgvjFgBxQQmSIQSgAHmEIhmvEJ+QgcDYYolUoAfITgDjCSrCbQ4Hg2JTCQBtWligBMYoAzABdADchIA9FbCQB3aAAcgkhJMlH5hNOtBE-OwOKROJxeIJYGJpMeUCpNINTJZ7JD3J2NDCguFovFkulGfliuVqvVmu1GDtbxdAAV3YkyFZuLh6XrDSbTYzLTb7U6XW6PV6fSx-YHcRzQyTFhGo4JjbG2UPE7yUwKdfZTFAEGK0yZTTLM1uc0rI-mNbgtYSdnbCQBZDCMAVL9Krk-TExypmt20OqDO13UbtQb2+-tQDEID5CYJDkoByDAQADFYBoGgAnLBIBIkAA))

IDE:
* Fixed a bug where `AUTO332` appeared in the result of string literal type completion.

Library Definitions:
* Added definitions for `FileSystem` APIs (thanks @ZelJin).
* Added constructor to the `StorageManager` class (thanks @ZelJin).
* Removed `checkPropTypes`, `DOM`, `createClass`, `ConcurrentMode` from `react` module.
* Types in react module can now also be used when React is default imported like `import React from 'react'`, they can also be used without imports, e.g. `type N = React.Node`.

### 0.230.0

Likely to cause new Flow errors:
* We now put value exports and type exports into two different namespaces. It means that we will now consistently error on places where you try to import, re-export, or use a imported type as a value. This change also makes certain patterns that happen to work in the past no longer work, e.g.

```
// a.js
export type Foo = string;
// b.js
const A = require('./a');
module.exports = {...A};
// c.js
// previously allowed, no longer allowed
import type {Foo} from './b';
```

* We have allowed function parameters to refer to each other for a while now. However, we didn't implement the analysis correctly for exported functions, which causes inconsistent behaviors. This is now fixed e.g.

```
// a.js
// we incorrectly resolve x to refer to
// be external to the function params,
// which is a global in this case
export function foo(x: string, y: typeof x) {}
// b.js
import {foo} from './a';
foo("", 3); // no error before, errors now
```

* Previously, we allowed assignment to any globals, regardless of whether it's a type or value. Therefore, both were allowed in `Array = 3; $ReadOnlyArray = 2;` Now the latter is banned.
* Flow now consistently bans referring a type-only global as value
* Some bad annotations that cause `value-as-type` or `recursive-definition` errors will no longer affect which branch to take in overload resolution.

New Features:
* Flow now supports typeof with type arguments `type T = typeof foo<string>`. This syntax is supported in prettier since v3.1. If you previously enabled the flag `typeof_with_type_arguments=true`, you need to remove it.
* Flow now supports explicit type arguments on JSX. e.g. `<Foo<string, _, number >propA='d' />`. Note that some type arguments can be inferred using the `_` syntax.
* Flow now supports the same `NoInfer` intrinsic that will be available in TypeScript 5.4.
* Under `experimental.ts_syntax=true`, Flow will
  * Automatically translate TypeScript's readonly on tuple and array types into Flow equivalent `$ReadOnly<[...]>` and `$ReadOnlyArray<...>` without error.
  * Automatically translate TypeScript's keyof into Flow equivalent `$Keys` without error. Note that there are behavior differences between `$Keys<>` and `keyof` right now ([flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCSww1wABABBbHAAA6UGxpOxMGIyGx9lMUAQ2IAvNiAOTMgDcxKRxOJmJxACFsQoGlA2CQ8QTiWTsZhKFSael6UzWRyoFyoMSWsZsQBpRnYgAk2uwtBIAB4+QA+FXMikQZnYrE660y+2O7UqmIgfImEhwaBBfIABisACYQwBOUMgJFAA), [TS](https://www.typescriptlang.org/play?ssl=7&ssc=2&pln=1&pc=1#code/MYGwhgzhAECC0G8BQ1XQGYHtMC5oQBcAnASwDsBzaAXmgHI6BuJAXySVEhgCFoBTAB4E+ZACYx4yNNABGYInkKlKNek1bsCATwAOfaAGlVAaz5bM6aN2Z0smOvjAESEdCT4wDNuUQcQnLm4ehsxAA)), so use it with caution.
  * Automatically translate TypeScript's `unknown` `never` and `undefined` into Flow equivalent `mixed` `empty` and `void` without error.
  * Support TypeScript's variance annotation `readonly` `in` `out`, `in out` without error.

Misc:
* `experimental.ts_syntax` is now always on in https://flow.org/try.

### 0.229.2

Misc:
* Fixed a potential crash under the experimental flag `experimental.blocking_worker_communication=false`.

### 0.229.1

Misc:

* Bug fixes in preparation of new feature rollout

### 0.229.0

Likely to cause new Flow errors:
* `invalid-recursive-exported-annotation` will now have error code of `recursive-definition`.
* Previously we would emit confusing errors when you try to use a value that has union types as a type. Now it will consistently emit `value-as-type` errors. ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCGiWVGwAAIsdccQBhACMOOAAB0oDicTBkDjhCIZgBuSlIymUzG4bF4rkEwkAJjJlOptJx9lMUAQLKgbKglNs9jxOIAvDipDIcQB+ImkukC6UtYw4gCCKrxTJxAHpLTjavk4BA4rhaDj0pARI5-GhSqsjSZKBBKLx6RAAO50wq4GSNDDAw24-2BykxED5EwkB1QIL5AAMVn5-IA7FYcyAkUA)).
* Unsupported statements like loops within `declare module` will no longer cause parse errors. Instead, they will be errored on during type checking.
* `declare export type` and `declare export interface` statements in toplevel are no longer parser errors. Instead, they will now be errored during type checking.
* Previous `toplevel-library-import` errors and `unsupported-statement-in-lib` errors will all have `unsupported-syntax` error code now.

New Features:
* We introduced `experimental.ts_syntax` flag that you can set to `true`. When this flag is on, Flow will no longer complain about certain TypeScript syntax, and will automatically turn them into the closest Flow equivalent types. Initially, we support the following: `Readonly`, `ReadonlyArray`, `ReadonlyMap`, `ReadonlySet`, `NonNullable`, and using `extends` to specify type parameter bounds. In the future, more kinds of types that we currently parse but error on might be added. (e.g. keyof). Please note that the support here is best effort. We make no guarantee that these types have the same semantics as those in TypeScript 100% of the time, but for common cases, it might be good enough for you.

Notable bug fixes:
* We now allow the use of saved state for [glean](https://glean.software/) indexer, if saved state is enabled in flowoconfig or CLI arguments.
* Improved the way we compute contextual hints for spread arguments of calls (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCGiWVGwAAJjjiqDRaAAhIQwH6UZA4gCC1DopPJJgA3AAdKAAZSk6QQVhglHCAGEclQBRANAAKKxS2lE3n8kTinYAdxxAFV0hIABwyujiwn0skUgCUJqZOIA9OacUroAByCQ4kz8ygEqC0HEiCAsGIgfImEhwaBBfIABisACZwwB2KwhkBIoA)).
* In `declare module`, all the values will be auto-exported if we cannot decide whether the module is in CJS or ESM. However, support for `declare const` and `declare let` was missing. This has now been fixed.

Misc:
* Make `both` the default value for the .flowconfig `casting_syntax` option, if not supplied.

### 0.228.0

Likely to cause new Flow errors:
* Some new errors might be raised when using `React.cloneElement` with polymorphic components. Eg. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCAB0oGZGBBKBIAAQAKgJGBIBIASqxlASYJRwgSAOSeZSMgDcWKxSzJ5IAggAeAAqAD4CQoGlA2OSqV4rABhcK4nZ+fnAGDEZAEwW8AlWPUogn5CBwNii4BIzmSuq4KjYQ1UUmamXKAAkAFFSuYVS1jBAYATeTq1RqCfZTFAEDq9VYkcKOVAsc7vEtoNgPdgvRIABQYACUpOl1Ik7s9Fn5PuwfoDQfVEE1YfSkd1+rjBIA9G2CQBVMhsAlhMXUPEEnDqliJos2Ig7dOZnP5sn9tZVjBs9udnspfsQQf0ygjzR47CWpNT1OzizztcdrU5BEEgDucGGpJ4O5Me4JUAgD6xMRA+QmCQcDQEE+QAAxWAATFBADsVjgSASJAA). Note that React.cloneElement is [deprecated](https://react.dev/reference/react/cloneElement)
* Nested `declare module` is no longer a parser error, but a type checking error
* `declare module` is now explicitly banned in nested scope and outside of library definition files. It already does nothing under those contexts.

New Features:
* All kinds of imports are now supported in library definition files
* We will no longer require annotations on class properties, if the class properties are already initialized with "annotation-like" expressions. e.g. We can now infer the type directly for properties like `prop = (foo: string): number => 3`.

Notable bug fixes:
* Fixed a bug that causes the first run of `flow check` to always return 0 errors, when some parts of temporary directory is a symlink.
* Hover type on react elements now show the type of the component and props, instead of just `React$Element`. (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCAB0oGZGBBKBIAAQAKgJGBIBIASqxlASYJRwgSAOSeZSMgDcWKxLWMBIACvTGCQADwAFQAfASALwE4BYgkEuGMZAEkUcqDyqyarFItVYpZk8kAYXCuJ2flFEoUDSgbHJVK8VmNIlNFiF-PuwvFEuASM5UD10HsBLw2HMfilBKFTpd4cVkoxIHspigCATAHoxWq02n5fKAHoEgDucGGBJIOQghcp1IkABIAKKlMMSIXc7AQGAE6PQCy8PkC4VJ9IIMVirExED5EwkODQIL5AAMVgATMuAOxWBcgJFAA))
* For missing annotation on exports, we now correctly say some annotation is missing on an identifier instead of array pattern. e.g. for `export function f(a) {}`, we now say "Missing type annotation at identifier" instead of "Missing type annotation at array pattern".

Parser:
* Flow can now parse `declare namespace Foo {...}` statements. The body of the declare namespace statement shares the same restrictions of `declare module`.
* Invalid exports in `declare module` are no longer parse errors. They are now type checker errors.

### 0.227.0

Likely to cause new Flow errors:
* Some errors related to empty arrays might have their error locations or codes changed.
* Flow will now interpret trivially recursive types, such as `type T = T` as `empty`, instead of `any`. (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCAB0-GsAAQAQRxAF4cQAhADcWJaxlJRPxFKgWIADDiMCR8Sy2SSOTjsCIHrQyTiAPRCnEAdzgwxxUAgYpxOBZUB51AglBxzIAfgA+Hl8lo4gAU3GKbLxAEoYiB8iYSHBoEF8oyrAAmZ0AVisAEYQEigA)).
* We now error when calling `.call()` on polymorphic functions where the return type is under-constrained (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCGiWVGwAAIYAtlHBoDjQQBRagQSgAHgAKgA+AAUJkolOQONOtAAlGyaQBuAA6i2g9hxzJxAF4Sb1ySzKDZuLgGRIcgjeDiAIyc3k4gD0OpxAHc4MMcVAIAbRRTKIa4MqcQsNJQXsKpG8dmwcTTBYLbCL4Mk2DK2cIRDMJVKJDLKfLhkqVSQ1ZrtXq8UkbXaWBIhJQoOzthBLI8oIKYiB8iYSESoEF8gAGKwAJgbAFYrOqQEigA)). To fix add an annotation to the return result.

Parser:
* DeclareModule AST will no longer have a kind field

Library Definitions:
* Removed redundant cases in `Array.from` overloads. It might cause some error locations and codes to change.
* Add types for the Storage API, available via `window.navigator.storage` (thanks @ZelJin).

### 0.226.0

Likely to cause new Flow errors:
* $ObjMap and $TupleMap become stricter. The following code now errors: [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCLWMAAIAGIAHgAKgA+LEAXixABIAEqsNgAeSguFoeOAAB0oFisUQEMgsQAKPkASjJJIJwtJJPyEDgbF47M5VkV7KRRIA3Oz2RollRsFiYAtlHBoFiIowAIx4um84CKqwqvkQM28umC3kUuloABWAFkMIxLbwsXiAGpEvn40OCiWh9VQLW4HV6g2PDmmgBMeIAgm6aewGUzM9Q6HjEsk2ESwxA07yAAqUeFkLNE12UglCRilX3+zOBkNhiPN6NqjWLaD2PXEMlYj2eureGAsbAAL2wfOAmEo1u5vL5MB34pJ9lMUAQB6xwCRl8F7Ns48dU9NZt3xGFGBI543vOEIhmSNjN7HCQsTQSdyTrBtsCsbhcD5ABtGBiFggByDckIAXTQwVY1vICqwfP00z5ECIFfd9YIAanyL9phMNDYxiEB8hMEgjSgIJ8gABisNM0wAVisM0QCRIA)

IDE:
* Linked JSX editing will now only trigger if you edit the opening tag, instead of both the opening and the closing tag.
* We now provide IDE services in user-defined library definitions.

Notable bug fixes:
* Fixed a bug where recursive types defined across multiple files caused spurious errors and led to unsound types
* Add .databases() to IDBFactory (thanks @bobrovnikov)
* Mark `DOMStringList` as iterable, so that it works with `Array.from(...)` (thanks @bobrovnikov)

### 0.225.1

Notable bug fixes:

* [fix] disable aggressive behavior of linkedEditingRange requests when there are parse errors
* [fix] Only print "evaluated" hover type results if the type is of a reasonable size (less than 100 nodes)
* [perf] faster LSP commands through skipping redundant parsing
* [fix][ide] Find-all-references and rename will no longer return stale results on the file that triggers the request. Instead, we will compute the most up-to-date result at the time of the request for the triggering file.

### 0.225.0

Notable bug fixes:
* We improved the inference support for function rest parameters in a generic function. [This example](https://flow.org/try#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCGiWVGwAAIYAtlHBoHiFgBBSgIABKZQkAB5qfYAHwAChgyBxrLgQIk7OwIgetF4OLItjYvP5LSFVmlVAQJHZDIkAEocQBeRk4xLJNhKhU0gDcAB0oJjcNiSYtHlA8cz7KYoAghcIRDMhWhiMsoEKACKRbC6nH5CBwNhGxbQex4gBMaot5KpNNZSv1OIA9KmcS1jDjcnUuiQcVAIAB3GIgfImEhEqBBCL2EwgJFAA) can now type check. You can also extract the parameters of a function excluding the first parameters now: ([example](https://flow.org/try#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCLWMAAIAEplCQABScYgAPAAVZBYgAUVlpVAQJEpABI8ewAPJQXC0ACC1DoJOwIgetAAfABKLEAXhFWMSyTYMslAB0oFisWSsQoGlA2CRqTB7JTTrReFjaVZ6YysekfpQsbyGRLpVjjViAPz2ygMrGUoa4ADcKo0Syo2CxMAWyjg0HDxEwlCpwhEM1NaGIyygpvspigCFNSZmYsp+QgcDYgcW0HsWIA2mgAIxZxtYqD1gC6lLx9iJNFJmOwEBgsYg8cVtakMlNAHIcngiFPTfWAEwAZjb-qxAHpN1iIF0VbZq3Wl1mTy2lx3cfie84SCT+4Ph6OpbXlyvTRPZFiZ3OIFP11uO4mJQECUCqMQgPkJgkNGUBBBE9gmCASJAA))
* In hover type, when a name is defined in another file but is imported with a different name, Flow will use the locally aliased name.
* fixed a crash happening when a class was defined in a scope where a binding with the same name already existed ([try-Flow example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCSww1wABAAhKjY4BIgA6UFJmJx+MohNJ2Oxtns0mUEEoAAoYMRkNjGekEABKQkkqBCmIgfImEhwaBBfIABisACYFQAWKyykBIoA))

### 0.224.0

Likely to cause new Flow errors:
* `%checks` is now deprecated. It will still work as before (for now), but if you enable the `deprecated-type` Flow lint, you will see additional errors. Instead, use [type guards](https://flow.org/en/docs/types/type-guards/).
* `$Call`, `$ObjMap`, `$ObjMapi`, `$ObjMapConst` are now deprecated. They will still work as before (for now), but if you enable the `deprecated-type` Flow lint, you will see additional errors. Since Flow 0.209, you can now use builtin types like `Parameters`, `ReturnType`, `Pick`, `Omit` to easily extract and transform types. For more advanced use cases, you can read the docs of conditional type and mapped type to understand how to write it in equivalent ways.
* Ban creation of object literal computed properties with non-literal `number` keys
* Ban accessing objects with non-literal `number` keys
* You can now create and access object keys with int-like number literals, these act as their string equivalents.
* Union of exact objects allow computed access as long as one of the members of the union have the property

New Features:
* Add support for number literals as keys in object literals, e.g. `const obj = {1: true, 2: false}`
* Support numeric keys in object type annotations. They are equivalent to the string version.

Notable bug fixes:
* Removed spurious underconstrained error with mapped type, if the relevant type parameter already has a default. ([example](https://flow.org/try#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCGiWVGwAAIYAtlHBoHiADwAFRxAF4ccAkQA+AAUAEpkDSANoAaRx6RxPVoEBgOLJAF1WRykQBuAA6UFKEhx8ipeOZEpxAHo1TiAKLUCBAnE4GB63EABisACZzQAWKwm3g4qAQHFEKD-Sg4kyUPUkblQHFmy02k0ymIgfImEhEqBBfIBy22kBIoA))
* Support calling `Array.from` with string input and also mapping function, e.g. `Array.from('str', x => x.length)`

IDE:
* [ide] Go-to-definition for a barrel-exported type name will now jump to the original definition location, instead of incorrectly jumping to itself.

Library Definitions:
* Add type stubs for global `crypto` object, `crypto.getRandomValues`, and `crypto.randomUUID`
* We removed `StatelessFunctionalComponent` type from the 'react' module and the `React$StatelessFunctionalComponent` type. The name might give an incorrect assumption that Flow can actually check that a function component doesn't use stateful hooks or can be called directly. If you have code that still uses this type, we recommend you use the `React.AbstractComponent` type, since it forces you to not make any assumption of what kind of React component you are dealing with.

### 0.223.3

Before Flow 0.223, the autoimport system uses the filename as a hint of the name for default export. e.g. if the filename is `foo.react.js` or `foo.js`, regardless of whether the default export in that file has a name or not, or even the default name is `bar`, autoimport will always import it as `import foo from 'foo'`.

However, in the new system starting from Flow 0.223, we will use the name of the default export as a stronger hint. In a lot of files that just give a default export a random generic name, they are relying on the old system to get reasonable auto imports.

It's not practical to update every Flow codebase to avoid the UX regression, so now auto-import will provide both options.

### 0.223.2

Notable bug fixes:
* Fixed issue with saved state generation used to power auto imports.

### 0.223.1

Notable bug fixes:
Improved precision of server states, that are visible either in vscode status bar, or through the `flow status` command.

### 0.223.0

Notable bug fixes:
* Mitigates a bug in Linked Editing Range where typing characters quickly on one side would throw off sync on the other side. With this fix, even if some characters are skipped Flow will be able to later recover the matching of open and close tag.
* Fixed an issue that will cause some nested conditional types to be evaluated to `any`. (Example: Issue #9107)

IDE:
* Auto-import for default-exported values will prefer the name of the exported value over the name inferred from the module.

Misc:
* Error collation is incremental only. The `incremental_error_collation` flowconfig option is not recognized anymore.

Library Definitions:
* Remove `status` global from dom.js
* Delete declared `React` module. You should always import react with lowercase like `require('react')` or `import React from 'react'`

### 0.222.0

New Features:
* Go-to-definition now works for private names
* Find references and rename now work for private names

Notable bug fixes:
* Documentation will now show up for `declare export` statements in hover and autocomplete. This means that jsdoc for exports in `declare module` will finally show up in IDE services.

Library Definitions:
* Added `USBConnectionEvent`
* Added `SecurityPolicyViolationEvent`
* Added `crypto.randomUUID`, `http$Server.closeAllConnections`, `http$Server.closeIdleConnections`

### 0.221.0

Likely to cause new Flow errors:
* Changed the signature of `React.Fragment` to only accept children and the key prop.
* Changed the signature of `React.useRef` to return an opaque type.
* Fixed a case where, in some cases, contextually-typed callbacks being passed as props would be typed as any rather than producing underconstrained-implicit-instantiation errors. This will result in new underconstrained errors, but removes a source of untracked unsoundness.
* Banned non-tuple arrays from being spread into tuple types. It previously resulted in `any` without error.

Notable bug fixes:
* Improved signature help support for optional chaining calls and intersection types. Now we will show all possible signature possibilities for overloaded functions.

Performance:
* Incremental error collation, which helps IDE performance when you have a lot of suppressed errors, is now the default.
* Optimized the way we generate global types for normal source files. You might see a reduction in checking times (3-9% as measured internally at Meta). (D51055838 samzhou19815)
* Optimized the way we analyze syntactic environment required for type checking. On a large file like [this](https://github.com/facebook/metro/blob/fcd8867d5b2ccf4133e1c12f224695e315d09b6f/flow-typed/npm/babel-types_v7.x.x.js#L3194), it improves the checking time from 4s to 2s. (D50850606 samzhou19815)
* Optimized substitution performance when conditional type and mapped types are operating on large and deep objects.

Library Definitions:
* Updated `IntersectionObserver` types.
* `IndexedDB` transaction now allows consuming `storeNames` as `DOMStringList`.
* Added `options` to IndexedDB transaction.
* Added `durability` to IDBTransaction.

### 0.220.1

Notable bug fixes:
* We will now provide auto type import for default-exported classes.
* We no longer suggest defined values when doing auto complete in object keys when we are under a type annotation.

### 0.220.0

Likely to cause new Flow errors:
* Fixed an issue where access of optional tuple element was not considered `| void`
* We rewrote how we generate types for library definitions, which makes the validation of libdef files stricter. You might see more errors as a result of the stricter validation. e.g. all unbound names in libdef files will cause an error, instead of the first reference of the unbound name.

Notable bug fixes:
* Fixed a bug in "refactor to react component" code actions. Previously, it would incorrectly replace all jsx children with the newly refactored JSX elements
* Fixed signature help when the function called is typed as an alias (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCAB0-GsAAQAMRxAF4cQAKeTIHH2UxQBAASiJAD4cfkIHA2ABuLFYjRLKjYHG2ew4hAUvGcmkkrE46U4gD0ACocfBkiRKYgoJEhCwcTk8IwdSZ+fLZVjaeyYiB8iYSHBoEF8gAGKwAJgAjAAOKyukBIoA))
* Fixed some spurious errors in generic calls when the solution involves unions (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCLWMAAIAKJYgC8WIA5AAxADyZKJWIAPsSAEIAQQASkSANwAHT8ayxABUADwAagAGgA+AlYoUcqAaJZUbBYmALZRwaBY-4SADS2FoJD5GpFAAp5MheXqRQBKE0aqWcmW4OVY2z2LHYE04qVOiRY2hu8XqrU6g3Yc2srEAejDWLYEDKWKgEC9JkoEEoWNOtBEKcC-HyJhIKqgQXyAAYrAAmACMAE5yyAkUA))
* Fixed a bug that caused weird errors when you try to use indexed access in export positions. [example](https://flow.org/try#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCAHoMQACAACMCIAHcADpQUktYzYgBixAAjNiALzY4AwYjIZmYSjs4CYABe7OEIhmSJFAG5SRollRsNjbPZsZzaeyaRBaQBtADkrIgGoAuprOXrxTsjBBKBJZdB5Xz6UzFVY+cbyWtqcQAEyM5lq+ymKAIXXc71SdL+wM+kMB7GC4ViiV1XDSy12C2ct3K91B33+zMR42ShMsJPynq0dnhv3GhSMM0WuUpjC8j12qhutUl3VOqBY7EwtjYsLYuFsIRgGUSHIy1nDCCEkPYkyUM0kUkYgBUpIAotQzdjGnv9wfD0fjyfT2fzxeT-PCgR0hpkqklKPrlYAFYkZAAVmQbtppNJADCpxQBAFowoOi7GOatDYgABnysHYtAcEACQAAqQSYLQACprIhaq0rq2LpNipzzqa5qDqQcCPFAVjYmhyxkBBED5HAGikVA2IABTcICUCRHA+TYAAlJxIGWLR2KspQpJwlBLRwQh2Jqm6ur0WqRzcOxjRVjWTTAaBgnQLq-5cdi17cGkUD3ikFBgM+JBvh+36-qSFnYp+AA+5HVpRdYKg2tqBZQtIOg2xoeVF0UxbF2IAHqJUliUqWpZkAEqaCYgjPsg7kWdgN7WbZj4OXsznIAALD+f7mdilU+fmiYBYq6aqpq2p6gaVBGvlcX9bFyVDcNSUqYRZmbtulC7pes1zfNC2npZt42QodlPuV77ILSAAMNVmUB2ygT2zGQCIjChCkLEKTByGwShG6lOYfi4cY+GEcRXFkXplHViQNFwNA9GMawzFwmxHFkbxOgmAJ-jCWJhmSYDXEyaSZ0XQ0fbyVhMGqepKlabgOk-VjjRI8ZUCmWSdWFVZd5raVjkVbt+11btPmk0W9aNp6qZttgtAdn1A2ix5I0pfjGVZSwixlHltNFQzD72czW0ABxs1F6uNfGzVWvWlBpq6ECtuW2bm8LdVi2LEt26NBHU6uGKkjEIDCUCKNBPkO1WL+6tWLSIBIkAA)


### 0.219.5

Misc:

* Bug fixes in preparation of new feature rollout


### 0.219.4

Misc:

* Bug fixes in preparation of new feature rollout

### 0.219.3

Misc:

* Bug fixes in preparation of new feature rollout


### 0.219.2

Misc:

* Bug fixes in preparation of new feature rollout

### 0.219.1

Misc:

* Bug fixes in preparation of new feature rollout

### 0.219.0

Likely to cause new Flow errors:
* We changed the upper bound of `React.Element` to be an inexact empty object, effectively making it opaque.
React doesn't recommend inspecting children and React.Element, so we make the change to increase friction. See https://react.dev/reference/react/Children#alternatives for alternatives.
To keep existing code type safe, you can add a function like the following to explicitly expose the internal of `React.Element`:
```
export default function DangerouslyAccessReactElementInternals_DO_NOT_USE_IN_NEW_CODE<
  E: React$ElementType,
  P,
>(e: React$Element<E, P>): {
  +key: React$Key | null,
  +props: P,
  +ref: any,
  +type: C,
} {
  return e;
}
```
* Conditional types will now only distribute over a union if `A` in `A extends B ? C : D` is a generic type. This is a behavior change that might cause new errors ([example](https://flow.org/try/?fbclid=IwAR0GHHxGLEnES-u0e6uoOYFIHeRPkoD87dlo-yuBUxSYUWdgOfpvU2JpwD0#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCLWMAAIALIYRjGNgAHgA1AB5ZBY4AkmDESmJZJsXhYqyspEAPixAF4sWSANoAclpEAFAF0sQoGlA2CQsfZTFAEAAdKBYrEAfjlUnSytVWMpwhEMxVKo0Syo2CxtnsWIglOAcMYlLxBJSROAwsp8p1WIAPljDTMOUiANxYgD04axtXycAgcVwtAlhVwQkiZSxYSpjq92sVfoD0xMKIDEAA7snuGmGrKsw7KPcDUXKEiVQAKO3ZhtOrUKhAFwPFgCUYcj0ZYsfjJETtq6zKgEAkEuoECBKpiIHyJhIcagQXyAAYrAAmACMAA4rAeQEigA)). Previously, we distribute over unions in more cases, which contradicts [what we said in the docs](https://flow.org/en/docs/types/conditional/?fbclid=IwAR3Yhr7j5FxH1dBgkxK-XWEj3uOj3VX7A7rqvEz0u5uwkb1v2q9H6V26vK8#toc-distributive-conditional-type), and caused some [weird behavior](https://flow.org/try/?fbclid=IwAR1Ea6RhpVT8OX3wQvWbdXznHuIhuyps0bhQmnjGtCt0xn739htLvPQzuIY#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCLWMAAIAPJoABWWIAvFjgABqADa9lMUAQAF1kFjEsk2EiANwAHSg9wwAEcZFjMdhcY5+dgADxk-AANQAfMSsdT0ghOVAuUKsQBZDCMYxsSU4xl4-HyknALlYrGUgDS2FoWPSWJ6tAgMFxDNxFNttDpWIUDSgbBIuIJFstWIA-CK+TJxTivXa6bKw5ajaLY-HvUneFz2VyuRollRhbZ7FiIIzyTBiIyAOQARlreagAAoK1qdXrxcAvQ6oFja9WILW6RHGcyUkjZQBKNlYgD086xAHdsHBKGw-dQIJQV3AJDk-SIHrReFj4Cy+1iAAxWABM9YAnFZr1yYiB8iYSHBoEF8reHwADhfEAkSAA) that's fixed in this version.
* If an exported class contains unannotated `propTypes`, Flow will now emit `signature-verification-failure`.

New Features:
* Hovering on enum members in IDE now show documentation if it's available.
* Added `casting_syntax` option which can be `colon` (current), `as`, or `both`. If setting is `both`, both `(x: T)` and `x as T` are allowed. If set to `colon` or `as`, only `(x: T)` and `x as T` are allowed respectively.

Notable bug fixes:
* Previously, when Flow cannot decide which branch to take in a generic conditional type, Flow will produce a abstract type that's unusable almost everywhere. Now Flow will still produce an abstract type, but it will be bounded above by a union type of both branches of conditional type. [example](https://flow.org/try#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCLWMAAIAMLQNhwR5QbgAHgAKgA+LEAXixAB0oFimViyViFA0oGwSFj7KYoAh6YzmQB+LEAcl56QQYsFzKxyHFEAkORMYoA3AyGRollRsFiYAtlHBoFj-hI8ZzCcbibhyRSABRkgCUCotBKJpMpGqgDLt9okyGdColUilMoAPorlaqaVTgAy5SwJEJKIyzW6rU9cP6nWqsQB6fP6t48LE4GAQFi8LGOa5SrFQCAAdwZSM1UHJCuEIhmDoDQfFkv5EajKsoMupcYTzKTKbTvQzHuzElzBaLXDgpfLldkNYwdf5DebrfbnZ5of5fcDLsHF+lWMjYqVY4nU6FTNnqdNC-xmZtObzQs9wPAVtxYLFTjYCCYEBE8oBiEB8hMEhrSCCJ7BMEAkSAA).

Misc:
* Flow's insert type quickfix in IDE will stop inserting very specific `React.Element` types. Instead, it will insert general types like `React.MixedElement`.

Library Definitions:
* Add type definition for `CanvasRenderingContext2D.roundRect`.
* Add `tagName` to DOM element declarations (thanks @aselbie).

### 0.218.1

New Features:
* Error collation during rechecks should become faster thanks to incremental computation. (Available with the flag `--incremental-error-collation`.)

### 0.218.0

Likely to cause new Flow errors:
* We start to validate variance inside `$ReadOnly`, which is previously completely unchecked. It also results in Flow complaining about some incorrect bounds. [example](https://flow.org/try#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCLWMAAIAGrcOBsACMAB4ANQAFQAfFiALxYgAkACVWGwAPJQXC0YnAGDEZBY8lIykAbixAHoxViIF0ADp+Na4-FsABMxMaVNpDOZ7HZnO5vIg-IAFOSAJS06n5CAEoWiiVS2Xy7EASSghVwBJJFOpdKZLN1XJ5fKxJvNNMt1rYtvFkpMlAglDlmOwWNd7oJqvVPq1-o5gYN-MFIpjWLjCblSYV5NMCH+QIAou6yeT+cArO2hZq-Tq89z21Yi3bY9QE-z0govFj8iRS4plHKYiB8iYSHBoEEIvYTCAkUA)

New Features:
* Hover types will now show a specialized version of the callee expression at a call to a generic or intersection-typed function.

Notable bug fixes:
* improve coverage accuracy for the element access portion on element calls, `obj[m](arg)`.

Misc:
* We slightly optimized Flow's performance of subtyping check between two union types.

Library Definitions:
* Add definitions `Array.prototype.findLastIndex()`, `String.prototype.at()`, `TypedArray.prototype.at()` and `Object.hasOwn()` (thanks @pascalduez).
* Update `URL` definitions (thanks @pascalduez).
* Add typing for cause option in Error constructor.

### 0.217.2

This is same as 0.217.1, but we have some issue publishing 0.217.1 to npm, so we will publish this new version instead.

### 0.217.1

Notable bug fixes:
* Previously, while global find ref/rename (`experimental.global_find_ref=true`/`experimental.global_rename=true`) is ongoing, all other IDE services will be blocked. We fixed it so that other IDE requests can still be served.

### 0.217.0

Likely to cause new Flow errors:
* Flow will now ban variance annotations in function or function type parameters. [example](https://flow.org/try#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCMAWyjg0AABFQEAAFUgAHgA1AAVAB8AAp5MhcRSAJQM-IQOBsXHAJG4gD0vNxmEubAAOlAYiB8iYSDjxchJQAGKwAJgAjAA2KyqkBIoA)
* `React.Element` is now opaque, but we still expose some of its internal structure via the bound on the opaque type. You should not depend on the internal structure. In a future release, we plan to make `React.Element` truly opaque.
* Fixed an issue with implicit instantiation in operations over unions that caused many errors to be missed.
* Changed the ref params on `forwardRef` and `useImperativeHandle` to be write-only. Patterns that rely on refining these params to an object and reading the ref field are misusing these APIs. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCZkYEEoEgABAAqXEYEi4gBKrGUuJglHCuIA5J5lHSANwAHSg7I0Syo2Fx3OuuIAEgAVACyABkAKKlcx+XHAJHs9kwBbKODQXEAYXCWJ2fgA6nAJDlyTAABTs3G4uGMEjIeUo3GW3EsGDIZ1WgA+8rAQmoFntQ1wuO9Iol0uwsokvFxVjjiqgVq9uLNZqDIaFYqlMosAEpcQBeAB8uMSyTYud47PzwCVidxcBgKddNj9LDlAEIC7igzXcQB6fu4gDucGGuJMNMoHY9Ls0rf9fmZTvrCatLAkfsTQbZUAT7PJXisMGxw6obFNZu1Il1FkNxtNud3MRA+RMJHVUCCEXsJhASKAA)
* `React.ElementProps` is now more precise when applied to `AbstractComponent`s, resulting in the config type of the component rather than `mixed`. This will cause errors when elements of components with different props are passed into `React.Element<>` types.

Parser:
* We now parse type arguments after `typeof` annotations. e.g. `typeof MyGenericClass<string, number>`. Type checking for this syntax is still not supported.

Library Definitions:
* Update `URLSearchParams` definitions (thanks @pascalduez).
* Update `AbortSignal` definitions (thanks @pascalduez).
* Add `Array` copying methods (`toReversed`, `toSorted`, `toSpliced` and `with`) the `$ReadOnlyArray` class definitions (thanks @pascalduez).

### 0.216.1

Notable bug fixes:
* We fixed local find references for namespace imported variables. It will no longer error or return empty results.

### 0.216.0

Likely to cause new Flow errors:
* The ability to configure `module.system.haste.use_name_reducers=false` is removed. For now this option defaults to `true` and can only be set to `true`. The option to set the only possible value will be removed in a future release.

Notable bug fixes:
* Fixed cases where hovering over a parser error could cause the Flow server to appear to hang.

Library Definitions:
* Add [`USB` web API](https://developer.mozilla.org/en-US/docs/Web/API/USBDevice) definitions.

### 0.215.1

Notable bug fixes:
* We fixed a bug that causes us to insert extra quote in autocomplete string literal in indexed access types.

### 0.215.0

Likely to cause new Flow errors:
* Improved analysis determining if a function has an implicit void return. As a result, some incorrectly typed code will no longer type check. ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCMAWyjg0AABDBiAAKeTIXHCEQzXi42iktDEXAASlJ5JmuOAAB0oLjcSQAO5wPo5XHEhlsznc7lZMi4gCMyHFEu5cBgwtooo5XMVErQni6AG4FYqkbi8NKNVrcUjDbiNFodPLNYqWBIhJQuTKDZqrVAkTEQPkTCQcVAghF7CYQEigA))
* Fixed a bug where global assignments in generics wouldn't flag errors. ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAIwIZoKYBsD6uEEAztvhgE6UYCe+JADpdhgCYowa5kA0I2KAFcAtiRQAXSkOz9sADwxgJ+NPTbYuQ3BMnTZA+Y2yU4IwRO4A6SFBIrGVDGM7c+IFkolXpUCWewUEAwhCQgRDH8wEH4hMnwROHlsNnw4KHwwSLAAC3wANyo4LFxscWQuHgMNZmwsiRSAWglaY1cq-hIAa2wJXNpG4Vxcdvdu3v7B0RxKUYMhKDBSqmbWwIq3eagoOrKSKgH0wtMMPznY7d2SfcoBiEZ-aG5G3Ix085AF-ZhsRoRehqUEiNMgSQHlSruBZxJrMcJwMhzAC+-EgGiCMAWyjg0AABAB3HJEgA8AA0AHwACgAlLjgAAdKC43FQaAKBGWPwAcSImFwuIAvLiACwAJgA3LiAPRS3EmSgQSiMpExED5EwkHFQIIRewmEBIoA))

New features:
* Flow now supports contextual typing for class private property assignments. ([example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602QF92kEdTB1GAAgBiECPuAAdKPv0BiXljTJ9AQTRos-ADyLyvDGgA+fQBefQBtAF0AbktLaxgTAAoASjM4631cAAs4egA6OwcQ8Oj0tUs1NhAAN38maGpqgAY8gCYARgAWPKaQNSA))

IDE:
* Local find all references will now return a more comprehensive result list.
* Go to definition now supports indexed access types.

Library Definitions:
* Added [inert attribute](https://developer.mozilla.org/en-US/docs/Web/HTML/Global_attributes/inert), [`showPicker()` method](https://developer.mozilla.org/en-US/docs/Web/API/HTMLInputElement/showPicker) and
[`hasPointerCapture()` method](https://developer.mozilla.org/en-US/docs/Web/API/Element/hasPointerCapture)

### 0.214.0

Likely to cause new Flow errors:
* Flow now has stricter behavior with `$Call`, `$ObjMap`, `$ObjMapi`, `$ObjMapConst`, `$TupleMap`. Previously hidden type errors might be revealed now.
* You can no longer disable `tuple_enhancements`, `conditional_type`, `mapped_type`, `type_guards` in flowconfig, since these options have been removed. They were enabled by default since v0.212.0.

IDE:
* We now provide a new refactor that extracts JSX elements into a new React component.
* Extract to constant refactors now works for nested JSX elements and fragments.

Notable bug fixes:
* Added default type arguments for the built-in `React$AbstractComponent` to match the `React.AbstractComponent` type alias in our libdefs. [[try-flow]](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602e0hQhcMtCw18ufgAcqyGUTkhyWIyaEHjp85cMn8CIljRCVZ2QF91CBFqVwwAAgBBAGFwgF5wgCVsPAASSN56GR5oiHIjaAVcAB5gIzQII2RwxXJeDDR-AD4AbnCAenbwgHc4GhoaiHCaaAQG8IaKtDYQADcGpmhqWYAGADoAJgBGAGY1rZB-IA)

Library Definitions:
* Added `parseArgs` to the Node.js `util` module library definition

### 0.213.1

Notable bug fixes:
* Fixed a bug where local find references do not return references in jsx props.

### 0.213.0

Likely to cause new Flow errors:
* Builtin idx support is removed. idx functions can now be typed with conditional type and mapped type. [The v3 release](https://github.com/facebook/idx/blob/main/CHANGELOG.md#300--2023-07-06) of idx contains these typing changes.
* When using `module.system=haste`, it will now be an error if a `.js.flow` file shadows a `.js` file with a different path prefix.

Notable bug fixes:
* Fixed a bug that used to cause spurious internal errors when you have non-binding patterns. [example](https://flow.org/try#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602e0hQhcMtCw18ufgAcqyGUTkhyWIyaEHjp85cMn8CIljRCVZ2QF91CBFqETA6TAACADcvSPJkSKwofgBuAB0oTJgSPDhoSJgACgBKSOBMyMiAbXIAOhgICABdSIBeSIBGDKgqzFwiNF7uzP9MthBojDQmaGpogAY6gCZO5bqFkH8gA)

Parser:
* Fixed parsing of multiple `as`/`satisfies` casts in a row.

Library Definitions:
* Added support for `Intl.Locale` class

### 0.212.0

Likely to cause new Flow errors:
* Flow is now stricter with regards to type checking of spreads, `$NonMaybeType`, and `$ReadOnly`. You might see new errors revealed as a result.
* Experimental utility types `$Pred` and `$Refine` have been deleted. These are replaced by type guards.

New Features:
* Updated `Pick` and `Omit` to work on interfaces and instances.
* Conditional types, mapped types, type guards, and tuple enhancements are now enabled by default.

Notable bug fixes:
* Prevented hover and go-to-definition requests that infinitely loop.

IDE:
- For imported names using `require`, go-to-definition will now jump to the location of the original definition's name.

### 0.211.1

Notable bug fixes:
* Improve fuzzy matching of autoimports

### 0.211.0

Likely to cause new Flow errors:
* We fixed an issue where the intersection of two identical object types are turned into empty in spreads. As a result, more errors might be revealed. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602e0hQhcMtCw18ufgAcqyMTXpyQ5LEZNCDx0+cvzOJtBQW49AOkMm+AhEWGhCKmZ6lgC+6hAi1CJgdJgABBBGWACORBipAXkA8pk5GAA8ALQAKgB8ANwAOlBNBakAkgAiZbWpALypVY3NmhjJoXkwJHhw0KkA7mi2AOraABZtQmUACgCMyKnAvkfRNQAUxdm52zs1AJT7F6Vlndc19U1NSSkTUzpQqfQjJgsEI2rhtgAmfaHY5nJqpVIAYQg5CM0G8DxKVy2EJqrHhAJMYH2OL6NVS5DgnAwQia91SjyuMN8OJOQyaAHoOQNVnkABKFRH0VJYXgQABueXGqV4DGYeho-FSGnFEBoRFwNNScBEpG0SsmUGm0HoR18H1ITlSADEIBA+gcANQwO37egyOBQBDRIZfaXi0IMzwIT16ZGo9GkTGXcq2iDvYYad3tIRLRZ2LX9BbLNYbc7B0M0cNoqDeW7so0m3CpADKQOwIiEDsBwNBuFOBI2ads9nx-1SpyMtzJB1idIrpzrrZp0aewBO5dSXIpcEYWowaDQEDQ0OiqQAfuS4002CBJWgmNBqOKAAy+CE7O8QkDRIA)

Notable bug fixes:
* We fixed an eager evaluation bug that can lead to some bad interactions between conditional type and mapped type. ([example](https://flow.org/try#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602e0hQhcMtCw18ufgAcqyGUTkhyWIyaEHjp85a4m0FBbj0A6QyfwIRFhoQirIYjT0GAC+6hAi1H4YAAQAIhgYRgAKwWR6AMpeuBgAPAAqAHzJALwAOlDJyQD0TckAMtgAbilG7uTacN30yXrQGPWNZclcxZrDAPwAFIrkvBhoyQA+yfQycFAIW8m8EBA02FAAlBONyfPJZTeNyMkAJABK2EIA8lA0-CVgE9bgBtADSyX2yQA1hh+BAYA8ALovdKZHJoPI0QpYYolDFYkqvACi3Dw5QqFXBSIqrGB0Qq9XqIjAdEwyU6wRGqIy2VycAKRVKKzWaAqAG5mq0AGJwTgYISQhoABm8ACYAIwa7zK+qLLAvEXrS7i+psEDdNBMaDUTqqzWqjUgaJAA))

Parser:
* We fixed the parser size regression since 0.210.0.

Library Definitions:
* We rewrote the `Omit` type implementation. Now it will preserve the optionality and the variance of the input object. [example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602e0hQhcMtCw18ufgAcqyMTXpyQ5LEZNCDx0+cvzOJtBQW49AOkMm+AhEWGhCKmZ6lgC+6hAi1AEYAAQA6toAFgDyRjpQeskAvMnAADpQyckwEBAA-MjJ9DJwUAgA3OWVvKENiuS8GGgdUNHl5RpNyRANWeTaADzpuNm5cLo0rMmlIN1o2wB8RSXRbckA9GfJg2gQaMkD1ZibUBDJNNAIg1doN2j05WwQAA3QZMaDUGxNQYgaJAA)

### 0.210.2

Notable bug fixes:
* Fix a bug where autocompleting a keyword deleted the next 7 characters on the same line
* "Fix" sorting of autoimports. An experimental option (`autoimports_ranked_by_usage=true`) to sort autoimports by how often they are imported was influencing the sorting even when the option was not enabled. This was likely an improvement, but it was a bug to not obey the option.

### 0.210.1

Misc:
* Improvements in preparation of new feature rollout

### 0.210.0

Likely to cause new Flow errors:
* We further strictified React.Element and React refs related typing. You might also see some existing errors related to React.Element and React ref moved around. [Example](https://flow.org/try#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602e0hQhcMtCw18ufgAcqyMTXpyOnE2goLcegHTksRk0IPHT5y-JsYdpSkzoYm+AhEWGhCKmZ6lgC+6hAi1CJgdJgABABu0dlYyNkASth4ACQAojQYwbhlMAA8AORauS0AfADcADqaGJnRGHkFvMVlPNW19Y2t9EZYUF19UAAURdlhGBAw2bwAlN3ZAPQn2VAQAO7ZgWgQaPT9a+Nb3ruFR6fnlzd3D08oGwQLlAkxoNRXPRcIEQIkgA)

New Features:
* Added Pick, Omit, and Record utility types. [[try-flow]](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602e0hQhcMtCw18ufgAcqyMTXpyOnE2goLcegHTksRk0IPHT5y-JsYdpSkzoYm+AhEWGhCKmZ6lgC+6hAi1GEYAAQA8pkAvJnAMBAQyJmK5LyBrJm80WUVVWiJANwAOlAdGZkACswA1gBiJfm9AwA82TUA5MUQ0wB87aTeOeTawxCj2eu4kzNzi8sdImB0mJka9LiZRgMYQmV9YEMlywAUdy8PZUUlDUomokAJTHTQYM7RLJXG4QXa4H5rDZvDrvOHaBGPQp1NAAyqBEFg7oAJQhEBigzQcIAyjI4FAEPRRqTIDFxrMStNMgAfTLTHHTGqNQJLDonCHnaHQa6ZTCsrEs8lCSk0ukM+gfOVK35zPFNGo4vUE0FsEAAN0CTGg1DNAAYnAAmW0ATictpAiSAA)

Notable bug fixes:
* Fixed a bug where `void` would appear in autocomplete suggestions more than once

Misc:
* Improved keyword autocomplete ordering

Parser:

Library Definitions:
* Added missing CSSOM `replace` and `replaceSync`
* Added `arrayBuffers` to Node libdefs for `process.memoryUsage()`'s return type

### 0.209.1

Misc:

* Bug fixes in preparation of new feature rollout

### 0.209.0

Likely to cause new Flow errors:
* We removed support for React PropTypes. All related Flow builtin types like `React$PropType$Primitive` are deleted. React has deprecated PropTypes since [v15.5](https://legacy.reactjs.org/blog/2017/04/07/react-v15.5.0.html).
* We will now catch errors related to generic indexed access types. [Example](https://flow.org/try#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602e0hQhcMtCw18ufgAcqyMTXpyOnE2goLcegHTksRk0IPHT5y-JsYdpSkzoYm+AhEWGhCKmZ6lgC+6hAi1DAkeHDQAAQIGLgAPADyyDnATpXJOQDSZQAkNRj89CUAfG0AFBC8AFZlxaw5ANZlNQCUAwDaNQC65QA6UDk5mLhEaMsAzADcOQD0+zkA6nA0NDmBaBBoOVAQAO5LiUtsIABugUzQ1O8ADE4AEx-AAcTgAjCBEkA)
* Fixed issues with indexed access on interfaces/instances. Access with string literals will now access named properties. [example](https://flow.org/try#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602fM4AHDGgoLcWGgDpIUIXDLQD+XPy0oxNenI6btu0gcPksGrUOu2VMgOTupaOpQeRjZa+AhEWGhCKsEGTgC+7JAi1HCk2mJgGAAEAJLFwAA6UMXFMBAQyMWK5LzaANzVtQDa9DJ5CAC6TbwNNNhQnVDp1SJgdJjFJn3FnE2lU9UAFJzdAOT1EHvDzUptaACU7cUA9DfFAEKiEIu4ABZw9EtvWFAIcsUAO4MYq-YraNAvarVGIlADSxQAvMUDg09lNYcUACpIsrdOGDa53YoAVScX3eJTyIk42lYzQguGKlOaWEoQmKGkh4Rs2wAjE0sVdbvcnvVXh8vmAfn8AcCvmCIS82CAAG7aJjQaiqgAMhgATDqAOyGHUgdJAA)
* We now require annotation for computed access of instances, when that access is directly exported.
* The return annotation of a function can now reference a parameter name (e.g [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602fM4AHDGgoLcWGgDpIUIXDLQD+XPy0oxNenI6btu0gcPksGrUOu2VMgOTupaOpQeRjZa+AhEWGhCKsgyRBgAvuyQItQAOrAkeHDQAAQwEBAAFJzIpYrkvNoAlHUxGBAwpZylwKUA9P31EKU00AjapVhQpdpoEGgFpaWYuERoM5wA3AUZBftQbCAAbtpM0NTe9LjaIBlAA)). This may cause new errors if the return type used to reference a name that aliases a parameter name. This will now be captured by the parameter and cause a [value-as-type] error.
* We now error when using the name of a variable in its own annotation (e.g. `const x: typeof x = ...`). An exception is when the name of the variable appears under an object type constructor. For example, the annotation `const o: { f: typeof o } = ...` is allowed.

Notable bug fixes:
* Fix a bug in the previous version that makes declare let/const function scoped. We fixed it to be lexically scoped again.

Library Definitions:
* Array.flat will now have better typing support for flattening heterogeneous inputs. `e.g. [1, [1,2]].flat()` will now correctly have inferred type of `Array<number>`.
* We added support for the following builtin types that are also found in TypeScript:
  * [`ReturnType`](https://flow.org/en/docs/types/utilities/#toc-return-type)
  * [`Parameters`](https://flow.org/en/docs/types/utilities/#toc-parameters)
  * [`Exclude`](https://flow.org/en/docs/types/utilities/#toc-exclude)
  * [`Extract`](https://flow.org/en/docs/types/utilities/#toc-extract)
  * [`ThisParameterType`](https://flow.org/en/docs/types/utilities/#toc-this-parameter-type)
  * [`OmitThisParameter`](https://flow.org/en/docs/types/utilities/#toc-omit-this-parameter-type)

### 0.208.1

Notable bug fixes:
* Fixed a crash when we are trying to produce a quick fix for missing imports. The autofix for missing imports broken in the previous release would be working again now.

### 0.208.0

Likely to cause new Flow errors:
* Flow will now error on unsupported statements in library definitions, instead of silently ignoring them.
* Flow will error more consistently when variables of type any and empty are used as types (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602QF92kEdQA6uHbBJ440AAQksUKBFxZcGIfgAOWNFnL4G+XPycYAFJwAlKbApgD04aZcfngO0WhoEGim5HCMcFAIALQ0EGBYNNmW1vpQpqYwRmRmMBAQ-gYVFZzIppysEVH0ABYQEkIJSSkA7nC4PaYAboVEGMX02T5+Tc38bR1dpr39NIMYicmmYxPTs-MMS74YqyHAagYPUAZVUMZmGOROPvgzaJ70bzXfx3VYiMB0TBnFIYNqfb78ADcq1e73KdQarWidzUWx2AyGRxOkxmNDmCyuKygajYICmByY0GoUwADAA6ABMLIAbGyWSA1EA))

IDE:
* Get definition request will now consistently jump to the name of the definition for ES module exports.
* In find local references, if a reference appears in a named import, it will no longer highlight the entire import as reference. Instead, it will only highlight the relevant name.

Notable bug fixes:
* Parameter types can now refer to previous parameters in the same parameter list (e.g. `function f(x: number, y: typeof x) {}`)
* Report error when a function parameter is redeclared in its body in functions that have default params (e.g. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602QF92kEdRgk8caAAIYUABSdkhxeV4Y0rQ-0vXbaQwF5DARgCUh4AA6UIaGNBi4hpwehgAMANyGAPSJhgDucDQ0VhCphraGWMF2aBBBamwgAG52TNDUlTEAdABMMQBsjTEgakA))
* For all declared names (declare class/var/let/const), we will allow full forward reference. All reads of these values will be considered to be initialized.

Misc:
* Changed React.Element to support a second type argument, as supported by React$Element

Library Definitions:
* Updated dom libdefs to allow Trusted Type objects

### 0.207.0

Likely to cause new Flow errors:
* When you pass a generic component type to some React utility type like `React.ElementConfig<typeof some_generic_component>`, we will replace all the type parameters with their defaults or bounds. It can potentially cause downstream errors. ([Example](https://flow.org/try#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602QF92kEdRFg6mAAQwSeONAMBhADwAVAHwAKYDAgRkBgIJo0WfrbtqAJTuAG4QcEIA3AA6ULr6GAYhWGgGELwAVu7Oru5ePn70MnBQCAExUA7pWQYASth4ACQAojQYlKQW0PAIVrj8AA4YEDCWdoGRBgD0UwZCEBj0UADkuAYY3hCpvKJbcuubqVAQAO6xbCAhG0zQ1CEADAB0AEz3AMyPAIwgakA))

New Features:
* Add built-in (intrisic) element names like `div` to JSX autocomplete
* Mapped types now support operating directly over keys, like TypeScript's Pick and Omit ([Example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602fM4AHDGgoLcWGgDpIUIXDLQD+XPy0oxNenI6btu0gcPksGrUOu2VMgOTupaOpQeRjZa+AhEWGhCKsEGTgC+7JAi1DEYAAQA8vkAvPnAMBAQyPmK5LzarPm8iTV1DWhNLQBebUod6QA6UMN5+QAKzADWADyFNcCGS5n5ANIY-PQ1ACTrm3MAfAel5QDaUxv5cFBrG-QAujWF5xv36QDcw6OB+QBiVQBBUwAIUSJ0mYFmhSaAHJKhAYfkAD75GEtNAwg6fKAACgqVRqAGYuq18oT0jV-hAgUJQWgAJTvfIAemZ+QA6lUAIRsEAAN20TGg1G89Fw2hA6SAA)). Mapped type support is still experimental; enable `experimental.mapped_type=true` to try them.

Notable bug fixes:
* Don't check `@noflow` files in "all" mode
* Fixed a source of spurious errors when using refined string keys as computed properties in objects ([Example](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602QF92kEdRgk8caAAJcGergAUAawz9khgPxm0cKAgCUh4AB0ohw3BhDKxtDAF5Qw0UaGg9vXz9DTFwiNCgAbh8-NUzDSCgzQwheACswz0MAbWt+AF07LxAGwzU0wwB6NsMAdzhcAAtjPoxDeE5WQbh6brhoyIhDGmgEDDRDFbQINB9sqDYQADcVpmhqfYAGADoAJjOANguzkDUgA))
* Improved Go to Definition on `export` statements

Library Definitions:
* Added `rel` as an attribute to `HTMLFormElement`
* Added `WeakRef` class definition
* Added `ariaHidden` DOM property

### 0.206.0

Likely to cause new Flow errors:
* `React.Element` will now accept a second type argument that specifies the props type. JSX will populate this type argument with actual props passed to component, instead of deriving it from the signature. The change will reveal some broken code, e.g. [try-Flow example](https://flow.org/try#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602QF92kEdQoAHCGlwACAFRGGRgErY8RmGgjkjAckw9nAbgA6UHyLB0mEYBDPRGAMKO+lAKuAA8ACoAfEZcuApCYdY8ACSR5NGxcQAUwDAQEMhGCWoAlCnAaj4+MCR4cNCpnLoYePmFpADyMADKMsW1Vdl4OQCiNBiUpHHhdIwrUdBF9DJwUAhJh0bAPkZGmLhEaFBGGwVbpHYVALzAAMxqRgD0SR7fX0YoBAAO6pNAONBhYFwXAAC0BSl4GDQRj2kAKWDIvAWRmhcKMOzQewQPiaUDYIAAbsimNBqOQGOk0CA1EA).
* `$Shape` is deprecated, use `Partial` instead. Read [this blog post](https://medium.com/flow-type/announcing-partial-required-flow-utility-types-catch-annotations-3a32f0bf2a20) for more.
* Add a (temporary) option `tuple_enhancements` to gate our tuple type enhancements (labeled tuple elements, which can be optional or have variance annotations), so we can show a Flow error rather than having users just get syntax errors from various other tooling (e.g. Prettier, ESLint), until all that tooling is updated.
* `experimental.abstract_locations` is removed. Setting this config to true improves Flow's performance. The config defaults to true since 0.139.

New Features:
* Allow opaque types with `string` supertypes to be used as keys in a dictionary, and have that opaque type be preserved when using `Object.keys`.
* To help people familiar with TS, error on and supply a quickfix to the Flow equivalent for the TS types: `Readonly`, `ReadonlyArray`, `ReadonlyMap`, `ReadonlySet`, `NonNullable`.

Notable bug fixes:
* Error more consistently with `[invalid-computed-prop]` errors. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602QF92kEdVz8ADhgAEAWSx7DAXkMASAErYhAeSg1+AHmCGA2vRlwoCAC6yIa+aP4IhmoAfADcADpQiTAkeHDQhgDCEOR60Aq4bgAqpnohpdEAFACUhsCJhoYiYHSYhpBQvobkZiElZglJUI0pUGkZWEJCRRClxQDSGPwh1ov89MUVVQDWS31rrIYAblg0RBh9pV5Fa4HVl2Z1DY2GmLhEaMP1wy+NAHQAnp6VjPX5eXb8YLHU7nUFRZ5qRLPXQGQxregAQRocAYm16JjM0UsNnReL0cWeo3Gw0m01mZgATAs9milpjsbj+uSdiybktDiczhdDFzrrd7iLSk8fo03h8vnD-oCzCCZS9wUsoYLzoYAPS6wxuAC0JsMuAAFnB6IYAO5wGg0QxQCCGGjQBAYNCGXhGLDDT1oCBoOGIn6h0NsEBHT1MaDUI4ABj+DITAFY-gmQGogA).
* Fixed cases where uses of `Partial`/`Required` caused errors to be positioned poorly.

Misc:
* To improve clarity of error messages we will display `mixed` instead of a generic type if that generic type has a default bound of `mixed`. [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602QF92kEdRgk8caAAIYAHgAqAPgAUnZIbMBKO4vK8MaQ8AA6UQ38OYuERovpwA3D5qbCAAbu5M0NQxAAwAdABMyQCsqQCMIGpAA).
* Object property type incompatibility errors will now contain additional explanations when the error only arises due to [object property invariant subtyping](https://flow.org/en/docs/faq/#why-cant-i-pass-a-string-to-a-function-that-takes-a-string-number).

### 0.205.1

* Fix over-eager IDE autocompletion of types after typing a space.

### 0.205.0

Likely to cause new Flow errors:
* Remove the `flow get-imports` CLI command.
* Some duplicate errors with `Arrays are invariantly typed` explanations are now correctly removed.

New Features:
* Added type-aware autocomplete in IDE for switch cases.
* Added signature help to IDE for constructor calls.
* Go-to-definition in the IDE can now provide multiple options if the target is defined in multiple locations (for example, in a union).
* Add `Required` and `Partial` to list of utility types to autocomplete in the IDE.
* Added quick fix in the IDE for missing "this." on class member access.
* To use the extract to expression/type alias IDE refactors, you no longer need to do exact selections to trigger it. Any selections that include extra whitespaces will be OK too.

Notable bug fixes:
* Fix go-to-definition in IDE for constructors (e.g., `new Foo()`).
* When considering a function as a callable object type, we now also check the statics of the function against the object. This may cause new errors. For example, the code in this [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602QF92kEdRgk8caAAIYACgCUh4GoA6sAHQwIEQwF5DARgDct2yZjJLcwCANwg4IU9jJwD6GTgoBFZDOxS1M082EGCMNCZoamCABjsAJkKAZjt3EDUgA) will now error.
* We now error more consistently when nested union types are being spread. Before, a bug was causing legitimate errors to not be surfaced. Example with [cannot-spread-inexact] errors: [try-Flow](https://flow.org/try#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602QF92kEdRFg6mAAQA3LGgMB5AEIApZAeBY79GXCgI1AbgA6UH7v0YxqYGAO4QaADW9ACMdsAAdIlqBgA+xhBwQt5Q-qaBJmZhkfQATHGJ8clpRhlCqemZ2bmGBQYQRGAAFuVJ9QAUNZn1g0IAlNk+fQmJVtasBhVFUdFq4wYA9OsGXAAOGHgYQnZgWFBQELgAtPQ7mFhCl65cPJPT8bPzi+FRJaseG1tdvtcIdjqdzlcbncHk9uHhXhUPgtEu0un8AQYALIASQAyrjsQA5ADiYLOF2ut2wMKgzzwBj6JDINGMBgADPESmyAKzxNmjHxsEBGDBoJjQahGDlcgAs8WiIDUQA). Example with type incompatibility: [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602QF92kEdVz8ADhgAEAQUNmAvIeCHcyQwHIs9w2oDcAHVL6jAIQDCFlY2dva8zmqGAD5Btg5g4R5eBib+hpbWsY7hUTEhYS45GSHxLomeImB0mIYAblhohpwAjHbG-mVQkFD0uIYQLYap6YYAdGPNpYYA9FOGGGhoEGj0rIYMc5wGeEKeu1AVVUZ1DZwATK057Z5dPX3ngwHDYyNnkzODAKoA4oZQEHMLJaGEhkGi1AAMI1O4IArCNwXtPGwQDV5kxoNQapDoQAWEZNEBqIA).
* Some spurious underconstrained-implicit-instantiation errors might be removed. Previously, some classes that extend `any` might have caused this issue.
* Fix over-eager IDE autocompletion of keywords after typing a space.

Library Definitions:
* Added NodeJS `process.hrtime.bigint()`.
* Many built-in react components and intrinsics props are now readonly.

### 0.204.1

Notable bug fixes:
* Fix a crash that could happen when canceling a recheck during module resolution

### 0.204.0

Likely to cause new Flow errors:
* Some heap refinements might be invalidated more aggressively, when method calls appear in conditionals. This brings us closer to what happens in the case of the regular function call. ([Example](https://flow.org/try/#1N4Igxg9gdgZglgcxALmAXwDTggEwKYogA6U+YANgIYBOeABAG411wAueAtsncCztwH4Azq2pwoCOmgDcJMlVqNmHSgAduvABaUh3ABQAPbpSgBPAJR0AvAD46AIwgRyeE1NlQScGHT1tOAHRwONZWdFAAruTkdAA+sXQAhCqqAdpCfuwcQTjmluh0eORC9MAkdHSQUCIsWQCSACLcImIS1rWBwdJ0APQ9dAAqmnBCLKNQEKx0bnjU1BDUdKy4lKYYDhFTAO5w0Q70E1skaCQkVTXp7SlpOh7evv7ZwaHhUTHxSemZnbn5aIXFUrlSrQGqPRrNUTiSRhR45bp9QbDUZbHTTKCFOYLabkWiUHCmY4kEBYBizIRwaCEBgABgCACYafSAgBGEBoIA))
* Always error on the deprecated existential type `*`, it's just an alias for `any`

New Features:
* Experimental support for Mapped Types like `{[k in keyof T]: ?T[k]}`! ([Example](https://flow.org/try#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602QF92kEdVz8ADhgAEAFUMBeQ8EMwIEZIfoy4UBIbUAdUvqMBVc5cMAbQBrQ2dDYIx+CBgTAF17AH5jELi3TwAKKxs7Q0UaGjd7YwBKAG5DAHpKwww0NAg0TOzbe3zCtXsfcqqaiGCAQjYQADc6pmhqcgZcOpA1IA)). Look out for a blog post, documentation and support in other tools in the near future. You can try mapped types in your project by setting the `experimental.mapped_type=true` flag in the `options` section of your flowconfig.

Notable bug fixes:
* Fix autocomplete for super members (e.g., `super.|`)
* Fix hover type of JSX attribute names
* Fix showing documentation in signature help for member expressions (e.g., `o.m(|)`)
* Fix `unused-promise` lint false positives for logical and conditional expressions
* Fix spurious error in overloaded generic calls when a contextual type is available ([Example](https://flow.org/try#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602QF92kEdRFg6mAAQwSeONCMQIACgCUyAwAU0EcnHoYAPPRlwoCAHwA3AA6ULr6GAYAblhoBrz2vJY02FAhUKGQUN4GnAYAvPEGAPwGAMwG9lYwlrYAdGBYuGAAFlaR+f4GwGo2NoFsIFEYaEzQ1FEADHUATJNldZMgakA))
* Properly remove the `:` with inferred predicates in `flow-remove-types`

Parser:
* Error on classes with `static prototype` fields
* Fix missing parse error for class constructors that are async, generators or accessors. The former two were previously type errors rather than syntax errors.

Library Definitions:
* Add NodeJS stream promises (thanks @moroine)

### 0.203.1

Misc:

* Add useful information to poorly located error messages. For example, for code like this [try-Flow](https://flow.org/try/#1N4Igxg9gdgZglgcxALlAJwKYEMwBcD6aArlLnALYYrgA2WAzvXGCADQgYAeOBARgJ74AJhhhYiNXClzEM7DFCLl602QF92kEdS4AHCGlwACXP10YjARSJZDGNEYC8RgCQAlbEIDyUGvwA8wGoAfADcADpQImB0mEYAbrZGAI42dmj0yEYAgmhoWAHWtrj2YZGRNBjG9AYlQkXp9AAycPTGzl68AFYYeAB0iTREGPQAFKnF9vQAlH0wdLij0xFQNXb1aSUZLW1ORgDafUdrdQ1bza24ALp9J6ORRuO4AIysKbgATNNOwQ9G-8kXrdIOZ8FAsJQ+jQIGAsJUAMIQci6WwYJ4fYEQUHgyjfAA+ePezz6cCiXCMAFp3hjSSJOKxIss2CB4lM4NBqPEAAx9D5cgDMfS5IDUQA) where the error points to a library definition, the error message will show the file that caused the error.

### 0.203.0

Likely to cause new Flow errors:
* `$Partial` has been renamed to `Partial`, use the latter instead. [Read the blog post for more details](https://medium.com/flow-type/announcing-partial-required-flow-utility-types-catch-annotations-3a32f0bf2a20).
* Support for `inference_mode` config and related infra such as codemods were removed. You should follow the instructions in the [Local Type Inference for Flow blog post](https://medium.com/flow-type/local-type-inference-for-flow-aaa65d071347) to switch to use LTI before upgrading if you haven't already done so.
* We replaced the implementation which checks nested `$Call` and `$ObjMap` during the check of a polymorphic function call. As a result, more underconstrained errors or type incompatibility errors might appear.
* Removed the `sharedmemory.log_level` flowconfig option and the `--sharedmemory-log-level` command line option.

IDE:
* Implement linked editing for JSX elements.
* Added code actions for adding/removing braces from arrow functions.
* Adds a quick fix from the deprecated `$Partial` to `Partial`.

Notable bug fixes:
* Fixed a bug that caused instances of nested utility types to be considered equivalent to `empty`. e.g. [try Flow example](https://flow.org/try/#0JAKALgngDgpgBAFTgXjgbwIYC44H40BGO+AHjgM5gBOAlgHYDmANHBDnQK4C2BMVAvoIDcIcNHgAhFHAAkAOQD2dALIYIvBOIA8CANoByDPoC6uAHQGCJgHwiAxkspwicKajRk4+gBY19LNjgAZn4hOAB6cLg6BTg+KgUqUUhYOAB5AEZpGQARGgAzfK0JFjRArhoSGAATflsQBzonBQIAKwycTOkPHB8-UIiomLiqBKSxVLSAJmy8wq15JVV1GE1YHQMjUwt9K2NrUvLKmrqRSLgLgD1XAFUAcRwwX3I4FPgAdwwXxvIaar4anEuFBIA1HGA4C1WlNOjN3J4+voBudyLEnjQXl9fgw6FwYHQIVQMBiYJi6CMxkA).
* Make Flow's behavior more consistent by not erroring on `.constructor` accesses. e.g. [try Flow example](https://flow.org/try/#0MYGwhgzhAEDC0G8BQ1rAPYDsIBcBOArsDungBQCUiKqaWu0AHtALzQ4AWAlhAHQbZ8REngDc0APQT23GAHdI0MCDwBTMABMAntEzocSzNFV48pGqi4AzaGU49+9IcVKsWbWFQSTpeuTJ5oQOUIdF19Q2NTc1paKWheRItoAF8aNJSgA).
* Fixed the file watcher not reacting to any changes on Windows.
* Improve the error messages when `underconstrained-implicit-instantiation` errors are raised in overloaded function calls. e.g. [try Flow example](https://flow.org/try/#0C4TwDgpgBAShDOBXANsAyhYAeGB7A7gHxQC8UA3gFBQ1QDUATgQMK6IB2wAXFO4gLYAjCAwA01WowLweAQQYMAhiBwFC4gL4BuSpQCWnEQDNFAY2hpEg-nuDBFg5NCq0oSa7Z4AKAJSliAG64egAmmrohEKbIigzQ0Yrw8FDMyHoQnBQSNKa47PDADIimwLgMvjxBoeKuAI6IIiowhF7ZrlC57EZ6AOY8BQwGPTW0PjwACkw28BA4CCjomDiE6m31jVgAijyWHnYOTi1trp3dfVCbIzRjF+GUkQlxUAGxHTyp6Zw6lKYAdOsMEBeADkaAAogAZMHMAAqUAAjMCfFooAB6VFQERMBi8AhQRJIfgIKD4aBgPSmADWEBCUGAAAtoN0GAUoLgAiJkLhFCEfv8GoCvOR3DZuFBfP4oBxIt12DSNMjKEA)
* We now propagate `any` to instance types' type arguments. As a result, some spurious errors under LTI might be fixed (e.g. [try Flow example](https://flow.org/try/#0CYUwxgNghgTiAEA3W8AeBnAXPAgjGUAngDxQB2hAfANwBQtU6hZY8AZgK4sAuAlgPZl2-fgAoAlPADetePDCD03eLwC2UAOYgAEowAWILLnxFiSmLzIbK8ALzwoAdyi9lABRj9VvdCAB0UBAQohji1PAA9BHwZPzwqvxw8OgADhwW-Bzo8CD4iei0AL5AA)), and some new errors might be revealed.
* Fixed the hover type result when an enum object type appears nested in another type structure. e.g. [try Flow example](https://flow.org/try/#0FAUwdgrgtgBAojA3gX2AYwPZgM4BcYgwC8MuAThIQPzwwBcMADANzAD0bMXMAejAO6EwGfjGwALETAAGuAJ4AHEBgBmtAD4xIUAEYgy0mOP0ggA).
* Fixed the hover type on certain kinds of union types. e.g. [try Flow example](https://flow.org/try/#0KYOwrgtgBAolDeBBANFAQgXwFAGMD2IAzgC5Q5QC8sAdDgIYkAUA5IswJRQD8XNiA3FgD0QqGKgA9KMQCeAB2BQAliABmeAE4Q6xJQSj6ABjkPLCUEHgDuUVUoAewACZA).
* Fixed the hover type on class constructors.

Misc:
* [Try Flow's](https://flow.org/try) behavior can now be configured.

Parser:
* Leading `(`, `&` and `|` are now included in intersection and union ranges, respectively.
* Improve error recovery for mismatched JSX tags.

Library Definitions:
* Change `setState` in React lib defs to use `$ReadOnly<Partial<T>>` instead of `$Shape<T>`.
* Make `$ReadOnlySet` entry type covariant.
* Expand the `Performance` typing with the `eventCounts` property, [according to the W3C standard](https://www.w3.org/TR/event-timing/#sec-event-counts).
* Adjust the type definitions of the `mark` and `measure` properties on `Performance` interface according to the W3C standard.

### 0.202.1

Misc:
* Fix a crash when using `experimental.channel_mode=socket` on Windows

Parser:
* Add "ObjectTypeMappedTypeProperty" node for mapped types
* Change "ConditionalType" and "InferType" nodes to "ConditionalTypeAnnotation" and "InferTypeAnnotation"

### 0.202.0

Likely to cause new Flow errors:
* Don't error for tuple element type incompatibility if we have already errored for invalid tuple arity.
* Fix a bug that causes certain errors in annotations to be hidden. As a result, more legitimate errors might appear. Example [here](https://flow.org/try#0BTCUC4AIBUAkEkDKB9RsDyBVAMgEWQKIBKR6RkAvAHyQB2ApgO6SL0AuwA2gLqigBQQA).

New Features:
* Add support for optional tuple elements, e.g. `['s']` and `['s', 1]` are both valid values for `[foo: string, bar?: number]`.
* Add autocomplete for keywords to the LSP.
* Add a code action that inserts jsdoc on a function.
* `exact_by_default=true` is now the default value if the option is omitted from the `.flowconfig`. Read this [blog post](https://medium.com/flow-type/exact-object-types-by-default-by-default-cc559af6f69) for more details.

Notable bug fixes:
* Fix LSP rename command to consider aliased named exports.
* If a contextually typed parameter has a default, we will filter out `null` and `void` from the inferred type of the parameter. This removes spurious errors like [this](https://flow.org/try/#0MYewdgzgLgBAZiEAuGAKAhisBXAtgIwFMAnAGhnwH4s8jiBKGAXgD4YcCTm11z9uAjAAYhjVjACy6KAAsAdLgCWYDH3oBuGAHotMAArFCAN0UhsEADYBPPjEUQ7YOCUMATGFBAVC7WlwA+MEYgiq7kYCAA7nZQAOQOHHRAA).
* Some spurious `prop-missing` errors might be removed. Example [here](https://flow.org/try#0CYUwxgNghgTiAEAzArgOzAFwJYHtVJxwAoBvAHzIF8BKALngDcctgBuAKEUNICNZ7UIAO7wAyiAxEA2gHIZAXWo1W8APSr4GABYg48AM5acyCMHg8EeCAE94eBAAcYOBwFoAtln36sqAObwus4wQA)
* Remove some spurious internal errors of Flow. Example [here](https://flow.org/try#0MYewdgzgLgBASgU2AVwE4QJYDcEHkBGAVjALwwDeMA9FTGCDALYioIwZhQKpgCGANjG6oWEAFAw64AMoBPTgAsEmAF698-BAAURABwBcMAIykyRgDRiJMAGYAKAJSGsIDABMK1yYhTpseIgA6eyhUZDYAfmMYQwBtAF0HAG5rAF9LVKSgA)

Misc:
* Add option for `module.system.haste.module_ref_prefix_LEGACY_INTEROP` in the config to allow migration from the `automatic_require_default` flag and to ES module syntax.

Parser:
* Include InterpreterDirective in the AST.
* Fix loc/range of postfix arrays that start with a parenthesis, like `(A | B)[]`.
* Fix missing error when arrow functions have duplicate params.

### 0.201.0

Local Type Inference:
* Local Type Inference is a rewrite of Flow’s type inference algorithm. This change makes Flow’s inference behavior more reliable and predictable. It replaces a global inference scheme that Flow has relied on since its inception and that has often been the culprit of confusing action-at-a-distance behavior, in exchange for modestly increased type annotations.
  It is now enabled by default! See [this blog post](https://medium.com/flow-type/local-type-inference-for-flow-aaa65d071347) for more details on the new system and steps to upgrade your project.

Likely to cause new Flow errors:
* When a variable is initialized at declaration, subsequent assignment in a conditional branch will no longer cause later reads to get a union type. This is already the case if the variable is annotated. As a result of this change, you might get new type errors. [Example](https://flow.org/try/#0DYUwLgBAJghmMQLwQEQEYUG4BQBLAZhABRgBOAriAJTRwLJo6zyYQD0bEADqSAG64A9uQDOwAJ4RckAO65gwCAAsYfEBDDiu6gHbkAtgCMQpCAB8IIsrh0BzADQQdgmVNnzFKtRq3qrpG1tsIA)
* When a variable is uninitialized or initialized to `null`, we use all the assignments to the variables as its initial type. We now no longer consider assignments from a child generic function. As a result, you might get new `invalid-declaration` errors. [Example](https://flow.org/try/#0DYUwLgBAHg3BD08IDsQHcIEtkDcCGwmAJgLREgDGweATnmJgPbIQg02M0BQXAZgK7IKDZhF4AeACoA+ABQBPAFwRJASggBvLhGgQAvBHkwuAXx6hIALy5A)
* Fix object value rest. It will now error if attempting to read a write-only or setter-only property (it reads while copying the properties). Additionally, the resulting rest object will no longer preserve read-only variance (as it is a copy).
* The type utility `$Partial` will now preserve the variance of the input object type's properties. For example, if the the input was a read-only object, now that output will also be a read-only object (with optional properties).
* `string` and `symbol` are no longer subtypes of an interface
* Error on `with` statements in non-strict mode. It was already a syntax error in strict mode.

New Features:
* A native Apple Silicon precompiled binary is now available via the GitHub Releases page. It is not yet available via flow-bin. Internally, we see about 2X faster performance than using Rosetta, and 4X faster performance than on actual Intel Macs.
* Add local variable renaming capabilities to the LSP
* Add "Find All References" functionality to the LSP for showing local references
* JSDoc autocomplete will automatically trigger after typing `/**`
* Add option for automatically closing JSX tags in the IDE
* Hovering in the IDE now shows both the unevaluated and evaluated type of the target expression.
* Add `use_mixed_in_catch_variables` config option that makes `catch` variables default to `mixed` instead of `any`
* Rename `$Partial` utility type to `Partial`. `$Partial` will continue working as an alias for at least one more version - use that period of time to codemod it to `Partial`.
* Add the `Required` utility type. It does the opposite of `Partial`: it makes optional properties required in object types.
* Improve error when attempting to call `Object.values` on a Flow Enum, to point the user to using `.members()`
* Improve error when using `number` and `boolean` primitives as a subtype of an interface
* Improve error on tuple arity mismatch

Notable bug fixes:
* Fix a bug preventing annotation hints from being used in tagged template expressions. This fix may get rid of some `underconstrained-implicit-instantiation` errors that were otherwise impossible to fix.
* Fix error when autocompleting the method stub for a function type without parameter names
* Document highlight in the IDE now correctly highlights destructured object keys
* Autocomplete can suggest literals when the target type is `$Keys`

[`flow-upgrade`](https://www.npmjs.com/package/flow-upgrade):
* Add codemod to convert usage of the `$Shape` utility type to `Partial`. It is possible that this will create type errors that you will have to resolve.
* Add codemod to rename `$Partial` to `Partial`

Misc:
* Remove `unused-promise-in-async-scope` and `unused-promise-in-sync-scope`. They can be enabled with `unused-promise`.
* Remove `--evaluate-type-destructors` flag from `type-at-pos` command
* Add `flow save-state --scm` flag to ease creation of SCM-aware saved states

Parser:
* Fix missing decorators on class fields
* Make `infer` a reserved word in types

### 0.200.1

Notable bug fixes:
We fixed a bug that causes spurious `Array is not an array` errors in try-flow. [Example](https://flow.org/try/#0PQKgBAAgZgNg9gdzCYAoVAXAngBwKZgCGATsWALxgCCphWAPAHYCuAtgEZ7EB8A3OjDwYiALiKkKYANoBdfqkIA6HMwDOACwAUARgCUvIA)

### 0.200.0

Likely to cause new Flow errors:
* We now require all generic functions to be fully annotated to prevent [generic-escape](https://medium.com/flow-type/flows-improved-handling-of-generic-types-b5909cc5e3c5) issues. Some escaped-generic errors will be removed with `missing-local-annot` errors on return type annotation positions.
* Under LTI, we will no longer silently give unannotated parameters in destructuring assignment `any` type. Instead, they will be properly contextually typed. e.g. [try Flow example](https://flow.org/try/#0CYUwxgNghgTiAEA3W8oC54AoB2BXAtgEYgwCU8AvAHzz4CWAHiMANwBQA2lALqXwc5y1LNgwh8ABwAuAT1LcW8APRL4AdzoQI8EjAD2MIA).
* Some additional errors might be revealed after a correctness fix in implicit instantiation. e.g. [try Flow example](https://flow.org/try/#0PQKgBAAgZgNg9gdzCYAoVBLAtgBzgJwBcwAlAUwEMBjYqfOLMAcn0pqYG5UoBXAOxoY4fMAGEGePmT6EAPABUAXGD48sAIzL4AfAAocOejgDOygN4VF8gL4BKZeWqEAJADk4AEzJgz19MGAwKAoMGFRZDwwAN21ZcVxhaWIKAF4zQnweMmswYFjgSJiuIA)
* The `exact_by_default` option is now required to be set in the `[options]` section of the `.flowconfig`. Add either `exact_by_default=true` or `exact_by_default=false` to your `[options]` section. Previously the absence of the option was equivalent to `exact_by_default=false`. In the future, we will make the absence of the option equivalent to `exact_by_default=true`. Read more about the option here: https://flow.org/en/docs/config/options/#toc-exact-by-default-boolean. To create a valid new `.flowconfig`, use `flow init`.
* Changes to type variable unification may cause new errors to surface or old errors to be shifted to new locations.
* Fixed a bug where an unannotated parameter does not cause a `missing-local-annot` error under LTI. e.g. [try Flow example](https://flow.org/try/#0CYUwxgNghgTiAEAzArgOzAFwJYHtX1ACMc0wQAeAFQEEYBzAZwC54ASAJRCmAHlUIAnrRhQB5ALZYAHiGAA+OQApFAOjU16DAJTwAvHPgA3HFmBaWq9bUY79Rk8ADcAKCIl0IZYhw5bBgN4AvlqO8AD0YfAgMDA4MEA)
* Flow will stop emitting some spurious errors and uncover some hidden bugs under local type inference, as a result of removing a cache that no longer makes sense.
* The `this` type of exported classes' methods are no longer unsoundly typed as `any`. As a result, more underconstrained implicit instantiation errors might show up in LTI.
* Relational comparisons between dates and numbers are no longer allowed.

New Features:
* We added `flow autofix missing-local-annot` command that will try to add annotations to autofix all `missing-local-annot` errors in a given file.
* We now provide autofixes in IDE for all `missing-local-annot` errors where we can locally infer a type. Previously, this kind of autofix is only provided for unannotated parameters.
* Added an `unused-promise-in-sync-scope` lint which will detect unused promises in sync scopes (like `unused-promise-in-async-scope` does for async scopes). Enabling `unused-promise` will enable both of these rules. The individual rules will be removed in a future release.
* Pragmas (e.g. `// @flow strict`, `// @jsx custom_jsx_fun`) are now supported in try-flow.

IDE:
* Added autocomplete for jsdoc comments on functions.
* Added autocomplete for `$FlowFixMe` comments.
* Autocomplete will no longer suggest existing type names for type binding identifiers (e.g., in the name of a type alias).

Notable bug fixes:
* We will no longer warn about `missing-local-annot` when there is a proper typing context in `super` call. e.g. `super(x => x)` will now pass type checking under LTI if the super constructor call expects such function.
* Builtin types like `$ReadOnly` will no longer shadow your local definitions with the same name.
* We will no longer emit spurious errors when we are doing contextual typing while trying to resolve an overload. e.g. [try Flow example](https://flow.org/try/#0CYUwxgNghgTiAEAzArgOzAFwJYHtXwFsoBzLMAHgBUA+AClWQggEoAueSgbgChRJYEKdNjyESZKnTYce3APRz4AZwAOyGLmRL4IGDBwx4cAjgBuIYOwDCUVKhwZ4YKEzGkw8AEbgoWhCv0VXQwATyQcHHgsbQJopSxUYij8BIxdRCgwBFCg+ABtAEYAXQA6fISwHAIVKGxPCBAAWmcmIu5K1CVHAH12VPTMhABvPIBrEBD2Lo1EovZbEIBfeABeeCHwnHYid1pmeEWeIA)
* We fixed a bug that over-zealously uses contextual type information from sibling nodes to type generic function calls. As a result, examples like the following no longer error: [try Flow example](https://flow.org/try/#0CYUwxgNghgTiAEkoGdnwJIB4AqA+eA3gL4BQoSc8AZgK4B2YALgJYD2d8AtgNZZ4AUADwBc8bAEpRfXAG4SEEI3gBPeAF54jGDQQB+Lr34BGcfFE90-AERXxM+AHoH8Oq3gR2AcxAx4PmKwwyACEQA)
* fixed a bug where optional chaining on a value typed as a type application would not filter out the null or undefined value. e.g. [try Flow example](https://flow.org/try/#0C4TwDgpgBAYgPAFQHxQLxQPwINwChSRQCCascAzsAE4CWAdgOZJ4AmEAxgDYCGV0AbryjcAXMTzcMAOk4RGwABbYoAehVQ6AewDuUAGY0AHhBa584aAEkWiFOhznCAIVLW4GSrUbNcbLrwEhACMxJzwg6Vl5JVV1LV0DY1NHaABhUg9qegZWDh4+KEEqKHYxVLx2SLkGRWU1KG1NKgBrclwgA)
* Some spurious errors might be removed as a result of Flow doing proper cache invalidation.
* Fix sorting of auto-imports with the same similarity score and length.

Parser:
* Parse and error on JSX type arguments.
* Add custom parse error for abstract classes.
* Add custom parse error when attempting to use template literal types in Flow.
* Add custom parse errors for TS class visibility modifiers public/private/protected.
* Remove `function` as an alias to `Function`, and don't allow `function` as a type name in general. (closes [issue #9000](https://github.com/meta/flow/issues/9000)).

### 0.199.1

Notable bug fixes:
* Fix crash related to deleted files in Haste


### 0.199.0

Likely to cause new Flow errors:
*  Support for spread argument in the builtin special-cased $Compose function is dropped. If you need this, you can write it yourself:
```
declare function compose<T>(
  fns:...$ReadOnlyArray<T=>T>,
): T;
```
* Flow now consistently disallows implicit coercion of dates to numbers. Prefer explicit conversion with `getTime()`.
* When a type argument is inferred from type argument bound or default, and such inference causes error downstream, we will provide better explanations in the error message where the type is coming from. Examples in [try Flow](https://flow.org/try/#0MYewdgzgLgBADjAvDMBTA7jACgJxAWwEsJUAKUnVCEAGwDdUBKJAPhkuvrICJvHGA3AChScAFzY8REgB5oOQmADmLQTAD06mKhx4cQoQBNUwGgENKMAGYBXMMCiFwMfGaWFgAYTM0aARhkAFRZSQNYXQgAPVENGCToQQkNhY1MLVGs7BycwFzcPb18AJiCkFBt8ACMdELDENiJo2IlA4SFXdy8ff1IIcN6JeUUlfgENLR09dvyu4t7+iEGoBWVR8e1dEH0Ogu6-efqYCAA6M0rgWLHNDamd2Zoig7YTs4u1a8mtoA).
* We fixed a bug under LTI that causes us to incorrectly skip checking for some function expressions [Example](https://flow.org/try/#0CYUwxgNghgTiAEAzArgOzAFwJYHtVJxwB4A1APgAoA3ALnhIBok6KSBKeAXjPipy2Bs6fAQG4AUOMSEK4+PADe8EBBABbEKgwBnOgG0AuvAC+DOfGpQIyEHSUr1mnXQCCMGFACeRbRhhZUAHMeYw5uRQsqKxs6dQAHDE82URMmAHo0+FQcAHdldxwYbXE2cSA). Some previously hidden errors might be revealed.
* We fixed a bug that causes some empty arrays to be incorrectly inferred as `Array<empty>` under LTI. e.g. `Array.from(nullableStringArray ?? [])`. As a result, previously hidden errors might be revealed.
* Errors for bad == or === comparisons will now consistently show up. Previously, some errors might be hidden when some optimizations are hit, which heavily depends on the implementation details. [Example]( https://flow.org/try/#0N4AgUCICYKYMYBsCGAnGIBuqQA8CMAXCAHYCuAtgEYwogA+IAzgC4oCWxA5gNwTTzI0mbAE9CJCtVoMW7Lr0hwA9sRYg4IALy48IAITax3EAHoTJJQHcQNFEpSMwAXzBhQfZaua4ATFpAAFOIA-GRUNACU9AwARDEK6ipqIn7aQUShkpHRIHEJnmoa2jh+BiApxmYgAA5oGGxKpIwIIjYodg7OYEA)
* More `missing-local-annot` error might be shown in cases when we cannot resolve overload. [Example]( https://flow.org/try/#0CYUwxgNghgTiAEAzArgOzAFwJYHtVJxwAoAPALnlWQFsAjEGAGiQqKroYEp4BeAPngA3HFmCcKw0QG4AUKEiwEKdNjwFi5eAGcMMLKgDmzRKx17D3fkJFiJN2XPDQ4Q2PE3t6MeAB9tu-QNZREJSZgBPXgEAbwBfTil4AHok+AYYHG9qLC0tQPgoVFQcDChVfBDvcKA)
* We fixed a bug where an unannotated parameter does not cause a missing-local-annot error under LTI. e.g.
```
declare function id<T>(T): T;
id([(item) => 1]); // now errors
```

New Features:
* Improved behavior of string literal autocomplete. Results will be provided regardless of the quote style used, and extra quotes won't be inserted when one is already typed.
* Add support for `declare let` and `declare const`. These work like `declare var`, but follow the rules of `let` and `const` respectively.

Notable bug fixes:
* We will no longer emit escaped-generic errors for predicate function bodies.
* `$Compose` now works under LTI mode.
* Remove spurious `illegal-this` error when a this annotation is used in contextual typing.
* Under LTI mode, we will no longer emit spurious incompatibility errors in invalid predicate function like `function f({a: b}): boolean %checks { return typeof b === 'string'; }`. Instead, you will only get error on this unsupported syntax.
* Fix type created by tagged template literals and `String.raw`. (Closes #7580. Fixes #5705. Fixes #2616.)
* We will emit fewer `underconstrained-implicit-instantiation` errors, when we decide that using type parameter default or bound won't cause downstream errors.
* Fixed a bug in LTI where some errors in utility types were not properly shown. [Example](https://flow.org/try/#0CYUwxgNghgTiAEAzArgOzAFwJYHtVIEYAeAeQD4AKHALnhIEpaASABRhwAcQYMBPAFV5ciAbwC+AGngByDtLIBueAHpl8AKIx2MeAGcAFjmQRg8DjiyoM8fdxAA6AFCPQkWAgBuseDXjiFjhSIBFSM8CAAthx89AFAA)
* Fix IDE services that stopped working while a file contained a setter with the wrong number of parameters.
* Fix an issue with lazy mode where deleting a file before the server starts did not check files that were depending on the deleted file.

Misc:
* [Try Flow](https://flow.org/try) is now using the new [local type inference](https://medium.com/flow-type/local-type-inference-for-flow-aaa65d071347) algorithm.

Parser:
* Parse bigint object keys (but type checking is not support yet).


### 0.198.2

Notable bug fixes:
* Fix a crash on Windows
* Fix a bug that could possibly result in a crash

Parser:
* Fix bugs concerning `await` in async formal parameters (including a regression in 0.198.0)

### 0.198.1

* Fix a regression in 0.198.0 that can lead to a crash

### 0.198.0

Likely to cause new Flow errors:
* Implicit coercion of booleans to numbers is consistently disallowed. Previously, booleans could be implicitly coerced to numbers in addition expressions. Prefer explicitly converting booleans to numbers.
* Binary arithmetic errors are now reported on the expression rather than the operands, and use error code `unsafe-arithmetic`.
* `keyof`, `undefined`, `never`, `unknown`, and `readonly` are now reserved types. If you have a user-defined types of these names, rename them to something else.

New Features:
* TypeScript syntax helpers. To help people familiar with TypeScript syntax ramp up on similar Flow syntax:
  * Parse and error on `keyof` type operator, recommending `$Keys<...>` instead.
  * Parse and error on `undefined` type, recommending `void` instead.
  * Parse and error on `never` type, recommending `empty` instead.
  * Parse and error on `unknown` type, recommending `mixed` instead.
  * Parse and error on `readonly` applied to tuple and array type syntax.
  * Parse and error on usage of `in`/`out`/`in out` variance annotations.
  * Parse and error on `as`, `as const`, and `satisfies` cast expressions.
* Add a codemod to fix underconstrained-implicit-instantiation errors only detectable under LTI. You can run `flow codemod annotate-implicit-instantiations --include-lti --ignore-suppressed --write .`

Notable bug fixes:
* Fix a bug in LTI mode where some errors were hidden in implicit instantiation calls (e.g. [Try Flow](https://flow.org/try/#0CYUwxgNghgTiAEIAeAHA9jALvAZgVwDsxMBLNA+PAZxAGUQJxMMAeWgGnltoD4AKAFDx4NRsQwAueHyqYomEFNoBKeAF4eXDgOVLaAbgECw5WfBTrKNemOYwZches2z5IAHQpl++AHpf8ACSFAAyACqB8AAGrgpR8CRUCQQ4IDBwwPBQSVEAtiRIIMDxUASZVGjwmAAWiVlgYCBUSVTVaHgQmQBGCKWI6RhAA))
* Fix unsound behavior of sentinel refinement with unions of numeric types as tags (e.g. [Try Flow](https://flow.org/try/#0CYUwxgNghgTiAEA3W8AeAuAUPeAfeA3gC4CeADiOvAIwB0ADHjbdQDTxlUB2ArgLYAjEDAC+2JsXKV4AJnad4AZyIwAllwDmIgNyZMqgGbwAFKlqkK8ALw3m9ewEpC407QXK1mh9vgB6X0oAFgD2PBDA8MIwwTDwvILC8AB+AHxKKuoamGJAA))
* No annotation is required in LTI for callback parameters when the callee is a refined object member (e.g. [Try Flow](https://flow.org/try/#0CYUwxgNghgTiAEA3W8AeAueBveBbA-JgBRgBGxAdphQK66kgwCU8AvAHxID2AlsCx2594AXwDcAKFQA6XPABk8tLKI02nHCKZigA))

IDE:
* When typing in an object literal, entire method signatures from the object type will be suggested.
* Method signature autocomplete items are now visually distinguished from other kinds of autocomplete items.
* Fix manually triggering autocomplete on the last empty line of a file giving no results.
* Members from `Object.prototype` are no longer included in autocomplete results.

Parser:
* Fix parsing of `for await (async of ...)` (regressed in 0.197.0)
* Fix parsing of `for (let in x)` in non-strict mode
* Fix parsing of class fields named `static`
* Fix missing parse errors on keywords containing escape sequences (regressed in 0.180.0)
* Fix missing parse error when using `await` as a parameter in an async arrow function
* Fix missing parse error for `export { ReservedWord as ... }`
* Fix incorrect parse error on `export * as ReservedWord from ...`, which is now allowed
* Treat `&&` and `||` as tokens when parsing types to improve error messages
* Parse the argument of the `typeof` type argument as a value, not a type, as it is a value

Library Definitions:
* Add `Element.getAttributeNames(): Array<string>`

### 0.197.0

Local Type Inference:

  We are releasing a new inference mode called [local type inference](https://medium.com/flow-type/introducing-local-type-inference-for-flow-6af65b7830aa). You can enable this new mode in your codebase by adding `inference_mode=lti` to the `[options]` section in the flowconfig. We will describe more details on this inference mode in an upcoming blog post.

Likely to cause new Flow errors:
* Unannotated class properties initialized with null will now errors.
* We now require all generic functions to be fully annotated.
* `bool` is a deprecated alias for `boolean`. Added a `deprecated-type-bool` lint and `flow fix --error-codes deprecated-type` codemod to help migrate to `boolean`. This lint is also included in the `deprecated-type` lint; if `deprecated-type` is enabled, you can opt out with `deprecated-type-bool=off`

New Features:
* Tuple improvements:
  * `$ReadOnly` utility makes tuples read-only. E.g. `$ReadOnly<[string, number]>` is the same as `[+a: string, +b: number]`.
  * Tuple elements can now have their variance annotated, when the element is labeled. E.g. `type T = [+foo: number, +bar: string]` creates a read-only tuple.
  * You can now label tuple elements (like TypeScript allows). The labels have no type-checking effect, they just help with self documentation (like indexer labels). When a function argument rest element is a tuple type, the labels work like parameter names in signature help.
  * We now parse tuple spread elements, but type checking of this feature is still unsupported.
  * These new features are now supported in Flow, but we have not upgraded related tools yet. For example, Prettier won't be able to format your code that uses variance annotations on tuples.
* Allow `mixed` or `any` as an annotation to a `catch` parameter (but not anything else)
* When typing in a class body, method signatures from super classes and interfaces will be suggested to make overriding/implementing these methods easier.

Notable bug fixes:
* Improve the parser's error recovery when in the middle of adding type parameters.
* Trigger autocomplete in JSX tags, JSX attribute values, and private class fields.
* Improves performance of bigints in big union checks

Misc:
* Inferred recursive types are shown as `any` (instead of `V$n`) in hover types.

Parser:
* `symbol` is now a reserved type
* `for (async of [])` is now a parse error

Library Definitions:
* Updated `AbortController` and `AbortSignal` signatures
* Actually expose `ReactSetStateFunction` type introduced in the last release.

### 0.196.3

* Fix a bug that could cause IDE commands to fail while a recheck is ongoing

### 0.196.2

🎁 This is ~~(hopefully)~~ our ~~last~~second to last release of the year. Happy holidays! 🎁

* Improve performance when unions flow to intersections
* Fix a crash when using the experimental `saved_state.allow_reinit` option

### 0.196.1

* Fix some missing autocomplete results (regression in 0.196.0)

### 0.196.0

Likely to cause new Flow errors:
* Instead of `mixed`, type the result of `Object.values` and `Object.entries` on a dictionary to be the dictionary values, and `Object.entries` keys to behave like `Object.keys`. Closes #2174, #2221, #4771, #4997, #5838.
* Ban array inputs to `Object.values` and `Object.entries`.
* Function's `name` and `length` property are now not writable.
* Fix `Object.keys`, `$Values`, and `$Keys` on interfaces with indexers.
* Delete `$Subtype` and `$SuperType` utilities.
* Delete `deprecated-utility` Flow lint.

New Features:
* An annotation is no longer required when exporting + expressions (e.g., `export default (3 + 3 : number)` can now just be `export default 3 + 3;`).

Notable bug fixes:
* Fix unary and binary arithmetic operations on bigints being exported/imported across modules as `any` or `number`.
* Fix autocomplete of string literals in bracket syntax, after a quote has already been typed.
* Improve sorting of results when autocompleting types.
* Boolean CLI flags must now be `1` or `true` to be truthy. Before, anything other than `0` and `false` was considered truthy.

Misc:
* Autocomplete results are now sorted case-insensitively.
* Autocomplete results are now sorted using a fuzzy score, in the same manner as VSCode.

Library Definitions:
* Add `ReactSetStateFunction` type alias in react. This is the type of `setVal` in `const [val, setVal] = React.useState(...)`.

### 0.195.2

Misc:
*  Adds an `--experimental.run-post-inference-implicit-instantiation true/false` CLI flag
to override the respective experimental flowconfig option.

### 0.195.1

Misc:
* Improved performance for the experimental post-inference implicit instantiation checks.



### 0.195.0

* Add a quick fix for the `unused-promise-in-async-scope` lint which will insert `void`.
* Add `symbol` and `bigint` to `$NotNullOrVoid`
* Add `findLast` to `Array` definition

### 0.194.0

Likely to cause new Flow errors:
* Casting an enum to `empty` will now properly error, e.g. `(E.A: empty)`.

New Features:
* Added a quick fix for the `unused-promise-in-async-scope` lint which will insert `await`.
* Added support for a number of additional bigint features. These include allowing comparison (`<`, `>`, `<=`, `>=`), update (`++`, `--`) and arithmetic operators (`+`, `-`, `~`, `*`) to be used with bigints.
* Added `sketchy-null-bigint` lint to match the behavior of the other `sketchy-null-*` lints for bigints.
* Added Flow Enums support for `declare enum` and `declare export enum`. Keep in mind that users need to have enabled Flow Enums if you wish to create library definitions that use them.

Parser:
* Switch to dune to build and install the `flow_parser` OPAM package. Some modules in `src/parser/` like `Comment_utils` are newly available.
* Decrease the size of `flow_parser.js` by roughly 25%, to 768kb.
* Fix a bug where `declare opaque type` was parsed as an `OpaqueType` when interned comments are disabled.

### 0.193.0

New Features:
* Add support for bigint. This includes the bigint type itself, subtyping with literals, libdefs, typeof, and sentinel refinement. For now, literal types are limited to 64 bit ints. Later changes may lift this restriction as well as implement support for operators.

Notable bug fixes:
* Fix a few bugs related to the Haste package resolver. The Haste resolver falls back to the Node resolver when resolving packages. It now obeys the `module.system.node.root_relative_dirname` setting when doing so. Also, scoped package names are now supported in the Haste module resolver.
* Allow private readonly properties to be written to in constructors, just like regular readonly properties.

Misc:
* Update team members in CONTRIBUTING.md.
* When using `module.system=haste`, importing packages in `node_modules` folders now obeys Node's resolution algorithm, rather than being able to be required from anywhere in the project.

Library Definitions:
* Add a default generic value for getOwnPropertyDescriptor.

### 0.192.0

Notable changes:
* The `exact_empty_objects` flag and the `annotate_empty_object` codemod are now deleted, See https://medium.com/flow-type/improved-handling-of-the-empty-object-in-flow-ead91887e40c for more details.
* We have announced in a [previous blog post](https://medium.com/flow-type/introducing-local-type-inference-for-flow-6af65b7830aa) that we are working on a new inference mode in Flow. Now the inference mode is mostly working, but there might still be some bugs remaining. You can try it out by adding `inference_mode=experimental.lti` to your flowconfig.

Likely to cause new Flow errors:
* Unknown computed properties assignments are now completely banned. Previously, they were still allowed if the object is inexact.
* `cannot-resolve-name` errors now are emitted for every use of an undefined variable, not just the first.

New Features:
* Added an experimental flow lint for unused promises in async scope. You can enable it by adding `unused-promise-in-async-scope` in the lint section of your flowconfig.

Misc:
* `annotate-use-state` and `annotate-use-callback` codemod commands have been merged into `annotate-react-hooks`4
* Added an experimental "scm" saved state fetcher, which selects a saved state based on your currently source control revision.

Library Definitions:
* Added support for Array.prototype.at
* Fixed WeakSet type parameter bound to be `interface {}`

### 0.191.0

Likely to cause new Flow errors:
* The `exact_empty_objects` option is now enabled by default. Details about this change and how to update your codebase are covered [on our blog](https://medium.com/flow-type/improved-handling-of-the-empty-object-in-flow-ead91887e40c).
* The `typeof X` type annotation now respects the initialization state of `X`: if `X` hasn't been declared it will be an error to get its type.

Notable changes:
* The `enforce_local_inference_annotations` config option was removed. To migrate your codebase, run the following codemod to add the required annotations: `flow codemod annotate-lti`. More details on the changes are explained in the [Requiring More Annotations to Functions and Classes in Flow blog post](https://medium.com/flow-type/requiring-more-annotations-to-functions-and-classes-in-flow-e8aa9b1d76bd).
* Literals can be suggested by autocomplete in more cases
* Fixed autocompleting specifiers in an import statement

### 0.190.1

* Fix a bug on Windows where the server would exit unexpectedly after `hg update`
* Performance improvements

### 0.190.0

Note:
Due to large number of likely new Flow errors that will be introduced in the next release, we decided to create an extra release to ease the upgrade process.
We announced in the recent [blog post](https://medium.com/flow-type/requiring-more-annotations-to-functions-and-classes-in-flow-e8aa9b1d76bd) that we would remove the `enforce_local_inference_annotations` option in flowconfigs, as it became the default in v0.189.0. This change will be delayed to v0.191.0 instead.

Likely to cause new Flow errors:
* Flow will now error on indexed write to exact objects without indexed type.
* Flow now requires that the target of `extends` clauses in class definitions are either variables, casts, or member expressions, rather than allowing arbitrary expressions.
* We fixed a bug in which type arguments were not passed to the type parameters of static callable methods.
* We unblocked constraints involving `any` and type applications like `Promise<T>`, which may cause new downstream errors to be discovered.

Notable bug fixes:
* Fix a bug that can cause the Flow server to unnecessarily restart when Watchman restarts

### 0.189.0

Likely to cause new Flow errors:
* `enforce_local_inference_annotations` is now on by default. To migrate your codebase run the following codemod to add the required annotations: `flow codemod annotate-lti`. More details on the changes are explained in the [Requiring More Annotations to Functions and Classes in Flow blog post](https://medium.com/flow-type/requiring-more-annotations-to-functions-and-classes-in-flow-e8aa9b1d76bd).

New Features:
* Class members can be suggested by autocomplete even if `this` is not typed explicitly.

Notable bug fixes:
* We will now consistently emit `object-this-reference` errors. Previously these errors were missing when the object is an argument of `Object.freeze()`, `Object.defineProperties()` or `Object.create()`.
* Fix crash when walking directories that can't be accessed.
* Fixed a potential crash when two type identifiers are declared in the same block.

Library Definitions:
* Improve `fs.readFile` type definition when the `encoding` config option is supplied.

### 0.188.2

Bug fixes:
* Fix crash during incremental recheck

Performance:
* Improve performance of auto-imports index update for large codebases

### 0.188.1

Bug fixes:
* Fix a crash when querying hg

Misc:
* We renamed the codemod command `flow codemod annotate-lti` to `flow codemod annotate-functions-and-classes` to clarify that this is not the only codemod necessary for enabling LTI eventually.

### 0.188.0

Likely to cause new Flow errors:
* Previously we supported a weird JS runtime rule for constructor returns when you call `new SomeClass`:
- If the returned value is primitive, then it's ignored and the instance will be returned instead.
- If the returned value is object like, then the return object will be the eval result of `new SomeClass`.
This is confusing, and we dropped support for this feature in this release. Now we enforce that all class constructors must return void.
* Fixed a bug preventing refinements from being invalidated in inline `&&` expressions: examples like `typeof x === 'number' && reassignXToString() && (x: number)` were passing even if `reassignXToString()` assigned a a string value to `x`.

New Features:
* `flow codemod annotate-lti` is now provided to help prepare your codebase to enable the `enforce_local_inference_annotations=true` flag. In order to eventually enable local type inference, we will require additional annotations in places we can't contextually type your code. You can enable the annotation enforcement now with `enforce_local_inference_annotations=true` in flowconfig. This flag enables the behaviors of all the following experimental flags, which will be deleted in the next release:
```
experimental.enforce_local_inference_annotations=true
experimental.enforce_this_annotations=true
experimental.enforce_class_annotations=true
```
To migrate your codebase, you can run the following codemod to prepare your codebase for the new requirement: `flow codemod annotate-lti`. Check out an upcoming blog post for more details.

Misc:
* Remove the `file_watcher.watchman.survive_restarts` flowconfig option, which was deprecated in 0.158.0. We will always try to survive Watchman restarts.
* Remove the `experimental.refactor` flowconfig option. It has been `true` by default since v0.158.0.

Library Definitions:
* Updates the typing of 'fs.promises.readdir()' to reflect the possibility of passing 'withFileTypes: true' and receiving an array of Dirent objects

### 0.187.1

* Fix a crash introduced in 0.187.0 when the file walker fails to open a directory

### 0.187.0

Likely to cause new Flow errors:
* JSX expressions in a file with jsx pragma of member expressions (e.g. `@jsx Foo.bar`) will no longer be incorrectly considered as unreachable. This might uncover previously hidden errors.
* Functions statics (i.e. assigning properties to functions) are safer, but also a bit more restricted. Function statics should be assigned in the same statement list as the function (i.e. not conditionally).

New Features:
* When autocompleting in the middle of a word, autocomplete now inserts the suggestion, instead of replacing the entire word. For example, if you are completing in `foo|Baz` (cursor at `|`) and choose `fooBar`, you'll now get `fooBarBaz` instead of `fooBar`. In VS Code, you can press shift-enter to replace instead of insert.

Bug fixes:
* Fix Go to Definition on certain kinds of import statements
* Improve location of error references pointing at imported namespaces (D38978756 mroch)
* Fix some missing references in CLI errors (D38977164 mroch)
* Improve location of error messages referring to `declare module` exports (D38977152 mroch)
* Change `flow get-def --json` to write `{"error": message}` to stdout on error, instead of writing non-JSON to stderr
* Fix a garbage collector bug that rarely could lead to crashes

Library Definitions:
* Allow access of arbitrary properties from `import.meta`. They are typed as `mixed`
* Add support for MediaSourceHandle

### 0.186.0

Notable breaking changes:
* Support for `inference_mode=classic` has been removed. The default is now `inference_mode=constrain_writes`. Please refer to the [blog post](https://medium.com/flow-type/new-flow-language-rule-constrained-writes-4c70e375d190) for migration paths. Additionally we have removed all codemod commands that depend on the classic inference mode. This includes the `annotate-declarations`, `rename-redefinitions` and `annotate-escaped-generics` codemods.

Likely to cause new Flow errors:
* Exported functions can only have properties assigned in the same scope as the function.
* Improve behavior of index property access with union types ([try-Flow](https://flow.org/try/#0C4TwDgpgBAslC8UDeAoKUCGAuKA7ArgLYBGEATgDRpTE4DOwZAlrgOZUC+A3CgCYQBjADYYy0AG6io4nDADaAcgwKoAHygLiCgLo9xCKAGYuUAPSmodABYB7fEN64FwKOTI2ymXCEIeIKfUQAIiCTc0tbe0dnVzJ3TwxvXzEgA) example). May cause new errors in interaction with computed properties.
* Referencing `this` in object getters and setters are now banned. Referencing `this` in object methods are already banned, and this change makes Flow's behavior more consistent.

Notable bug fixes:
* Fixed a regression in destructuring checking with computed properties ([issue](https://github.com/meta/flow/issues/8921)).

Library Definitions:
* Added `PerformanceServerTiming` definition and the `serverTiming` attribute to `PerformanceResourceTiming`.

### 0.185.2

New Features:
* Flow now supports the `@jsxRuntime` pragma in the top docblock of each JS module. Allowed values are `classic` and `automatic`.

### 0.185.1

Misc:
* Improve the experimental mode to rank autoimport suggestions by how often they're imported (`autoimports_ranked_by_usage=true`)

### 0.185.0

Likely to cause new Flow errors:
* Unbound names in jsx-pragma when the jsx-pragma is not a simple identifier will now consistently point to the jsx pragma.
* Errors previously hidden by method-unbinding will reappear.
* Fix a bug that makes assignments in loop guards like `x = some expression containing x` have a type that's effectively `any`. If you turned on `inference_mode=constrain_writes` since the last release, there will be new errors.
* Remove `esproposal_export_star_as` as an option and make this the default as it made it into the JS standard in 2018.
* Additional `value-as-type` errors might be emitted, as a result of existing importing errors.
* Object rest on instances no longer results in an unsealed object (that can have properties added after the fact)

New Features:
* `inference_mode=constrain_writes` will be the default. `inference_mode=classic`, which is the current default, will be removed in the next release. Please refer to the [blog post](https://medium.com/flow-type/new-flow-language-rule-constrained-writes-4c70e375d190) for migration paths.

Misc:
* Fix flow-remove-types/jest transformer to be compatible with Jest 28. (Thanks @carloscuesta)
* Add codemod to remove annotations in destructuring - which we banned in 0.176

Library Definitions:
* Add missing `.x` and `.y` properties to DOMRect. (Thanks @pmer)

### 0.184.0

Upcoming Breaking Changes:

* New Inference Mode:

  We are releasing a new inference mode that marks significant progress on our way to local type inference. You can enable this new mode in your codebase by adding `inference_mode=constrain_writes` to the `[options]` section in the flowconfig.

  We will make `inference_mode` default to `constrain_writes` in v0.185, and remove the `classic` option in v0.186. To migrate your codebase, you can run the following codemods to prepare your codebase for the new inference mode:

  ```
  flow codemod rename-redefinitions
  flow codemod annotate-declarations
  ```

  We will describe more details on this inference mode in an upcoming blog post.

* Additional Annotation Requirements:

  In order to eventually enable local type inference, we will require additional annotations in places we can't contextually type your code. You can enable the annotation enforcement now with the following flags in flowconfig:

  ```
  experimental.enforce_local_inference_annotations=true
  experimental.enforce_this_annotations=true
  experimental.enforce_class_annotations=true
  ```

  To migrate your codebase, you can run the following codemod to prepare your codebase for the new requirement: `flow codemod annotate-lti-experimental`

Likely to cause new Flow errors:

* `Object.create` now results in a sealed object. Arbitrary properties can no longer be read/written from/to it. You can use the `__proto__` property in object annotations to create an annotation if you need to.

Notable bug fixes:
* Fix spurious object subtyping errors when an object literal is passed into rest arguments with tuple type
  ```
  function emit(...args: [{+bar: string}]): void {}
  emit({ bar: "" }); // no longer emits incompatible-variance
  ```
* Fix a bug on Windows that could lead to a crash if files are changed faster than they can be rechecked

`flow-upgrade`:
* Add a codemod to collapse multiline object initialization into one object literal - useful for enabling the `exact_empty_objects` flowconfig option

Library Definitions:
* Add DOMTokenList index signature

### 0.183.1

* Fix a regression in 0.183.0 on Windows where inconsistent handling of slashes within paths led to mismatches like "duplicate provider" errors

### 0.183.0

Likely to cause new Flow errors:
* We now emit new errors in places where an object literal has literal types, but doesn't have an annotation. You can usually fix these errors by adding annotations to object literals.

New Features:
* Allow export of empty object without annotation when `exact_empty_objects` is enabled.
* Improve annotate `useState` codemod to support `null`/`undefined`/empty-array arguments.

Notable bug fixes:
* Allow flowconfigs with blank lines before the first config section.
* Fix location of error message about legacy octal number types.
* Fix inconsistent blank lines around auto-imports.
* Fix bug where Watchman events might be ignored on Windows.

Parser:
* Fix parser exception on negative binary and octal number types that contain whitespace after the minus sign.

Library Definitions:
* Improved `fs.readdir` and `fs.readdirSync` to allow better overload resolution.
* Add `String.prototype.replaceAll()` (Thanks @dav-s).

### 0.182.0

Likely to cause new Flow errors:
* `var` re-declarations are now banned. You should use `let` or `const` instead.
* Flow will now emit escaped-generic errors when the return type of a generic predicate function contains type parameters defined by the function.

Notable bug fixes:
* Improve ordering of autoimport suggestions, so that globals are suggested above named imports.

Misc:
* Errors related to class expressions might have new locations.


### 0.181.2

* Fix the rendering of classes in autocomplete results
* Fix the ordering of class methods in autocomplete results
* Improve ability to recover from Watchman restarting (the fix in 0.181.0 was incomplete)
* Fix a bug where warnings would be incorrectly reported when using saved state, and could cause a crash.
* Add `--saved-state-verify` flag to check that a saved state matches what is expected to be on disk.

### 0.181.1

Notable bug fixes:
* The JS build for try-flow is now fixed. It's recently broken since the 0.181.0 release.

### 0.181.0

Notable bug fixes:
* Fix unsound spreading of empty array literals ([example](https://flow.org/try/#0G4QwTgBCELwQ2gXQNwCgQDoAOBXAzgBYAUAjAJRqiQBGAXBAIJhggCeAPHgC5gCWAdgHMAfLAQYJIFBAD0MiIQD2OADYATCAFNmisEA))
* Fix a race condition that could cause individual requests from the IDE to hang indefinitely
* Improve ability to recover from Watchman restarting

Parser:
* Fix parsing of invalid UTF-8 to raise the correct "Malformed unicode" parse error
* Fix line numbers when regex character classes contain an (invalid) newline, which could've led to a crash

Library Definitions:
* Add AudioContext constructor options
* Update Node's `v8` module

### 0.180.1

Notable bug fixes:
Reverted changes that cause Flow to generate a corrupted saved-state when libdef changes.

### 0.180.0

Likely to cause new Flow errors:
* Generator functions without return type annotations now default the type of the `next` parameter to `void`. If a non-void `next` type is desired for a function, it will have to be annotated.
* When `experimental.enforce_local_inference_annotations` is enabled, Flow now requires annotations on functions returned from async or generator functions.
* Return statements that return objects no longer widen the object type to match the return annotation, if present. This may lead to additional errors, which can be addressed by adding annotations onto variables that are returned.

New Features:
* Optimize initialization from saved state.
* When `experimental.enforce_local_inference_annotations` is enabled, Flow now has a code action to auto-annotate parameters of functions that have `missing-local-annot` errors.

Notable bug fixes:
* Fix broken IDE services when the file contains certain invalid type annotations, `import type * ...`, `export async ...` statements or nameless named exports.
* Fix some potential deadlocks on Windows.

Misc:
* Add `--no-enums` flag to `flow ast`.

Parser:
* Fix a runtime exception when parsing regexes via the JS version of the parser.
* Fix crash when the input is a lone quote.
* Fix missing syntax errors when functions with non-simple params contain `"use strict"`

Library Definitions:
* Add type signature for `Navigator.canShare()`.

### 0.179.0

Likely to cause new Flow errors:
* Improve the type checking of function parameters that use destructuring, have an annotation, and also have a default value.

Notable bug fixes:
* Fix IDE services in the presence of certain types of parse errors.
* Fix crash when encountering an unexpected `\/`. This could have broken IDE integrations while typing.
* Fix crash with invalid for-await loops.

Parser:
*  Add parser support for ES2022 `d` RegExp flag (see [RegExp Match Indices]((https://github.com/tc39/proposal-regexp-match-indices))).

Library Definitions:
* Add void to the React$Node type.

### 0.178.1

Notable Bug Fixes:
* [fix] Fix a crash during incremental recheck when an empty string import (`e.g. import ''`) is added.
* [fix] Fix a stackoverflow caused by extremely large objects
* [fix] Fix an incremental update issue where Flow would sometimes resolve requires to stale Haste modules which have no provider files

### 0.178.0

Likely to cause new Flow errors:
* Improve type incompatibility error messages on default function parameters
* Invalidate refinements on calls of `require`, since requiring a module can have side effects

New Features:
* Add types for React 18 hooks: `useId`, `useInsertionEffect` and `useSyncExternalStore`

Notable bug fixes:
* `x instanceof A` when `x` is typed as `any` refines it to `A` instead of `empty`
* Fix an issue with `[include]` entries containing both globs and relative paths on Windows
* Fix crash when declaring a destructured binding in a library definition
* Fix missing autocomplete results for JSX component properties when the component's type is recursive
* Fix analysis of unreachable += expressions
* Fix incremental typechecking bug when adding files in Haste mode

Parser:
* Fix parser exception when parsing `export interface ...` or `export enum ...`

### 0.177.0

Likely to cause new Flow errors:
* Fix a bug where refinements should be invalidated when going through multiple control flow branches (thanks @gnprice)
* Make catch parameters explicitly `any` typed
* Ban object spreads of numbers and strings
* Fix a bug that makes Flow consider some function names to have the empty type
* Numbers, booleans, and enums are no longer subtypes of the empty interface `interface {}`
* Previously incorrectly missing errors might appear in expressions of the form `name === 'literal'` and `name.prop === 'literal'` complaining that `literal` is incompatible with the type of `name` or `name.prop`.
* Update input type of Object.{values,entries} to be `interface {}` rather than `$NotNullOrVoid`; `number` and `boolean` inputs will now error

New Features:
* Add codemod to add explicit type arguments to `useState` when it is called with an empty object. Use with `flow codemod annotate-use-state`
* Continue type checking files with indeterminate module types.

Notable bug fixes:
* Do not add further errors when we already errored on the illegal reassignment.
* Flow now checks whether `literal` is a subtype of `expr` in `expr === literal`, regardless of whether `expr === literal` can be applied as a refinement
* Fix a crash when deleting a file or saving a file with parse errors in certain circumstances
* Fix a rare crash when files are deleted during rechecks
* Fix a parser bug that allowed missing commas between object properties (regression in 0.175.0)

Parser:
* Improve error recovery when in the middle of adding a property to an object type literal

Library Definitions:
* Add NotificationEvent to service workers API

### 0.176.3

Notable bug fixes:
* The Haste module resolver now chooses Haste modules before node_modules, to match what Metro does. That is, if you use `module.system=haste` and have both a `@providesModule foo` file (or use "haste paths" to derive the module name from its filename) and a `node_modules/foo` folder, `require('foo')` will now resolve to the Haste module, even when `node_modules/foo` is in that file's parent directories. This is also a significant perf improvement because resolving a Haste module is much cheaper than searching for node_modules.
* Fix several bugs responsible for crashes
* Fix bugs responsible for some LSP "server is stopped" errors

### 0.176.2

Revert changes that are causing bugs in non-lazy mode.

### 0.176.1

Fix crash when comparing abstract and concrete locations when simplifying normalized types.

### 0.176.0

Likely to cause new Flow errors:
* Banned usage of `new` on functions. Move usages of this pattern to use ES6 classes instead. If the pattern exists in third-party code that cannot be changed directly, you can use a [declaration file](https://flow.org/en/docs/declarations/) to type the legacy pattern with a `declare class`.
* Error on type annotations nested inside of destructuring - these were always invalid, we just ignored them before

Notable changes:
* Removed special support for `React.createClass` from Flow. It is now just typed as `any`. Migrate any components using it to class components or function components.
* Added the `module.missing_module_generators` option, which can be used (multiple times) to specify `'regex' -> 'command'` pairs. When a module is missing, if its name matches one of the regexes, we will add a suggestion in the error message to run `command` to generate it (and resolve the error).
* Fixed bugs responsible for some LSP "server is stopped" errors
* Fixed a bug that could cause the server to become unresponsive on Windows

### 0.175.1

Bug fixes:
* Fix an incremental bug which would sometimes cause Flow to choose the wrong provider for a haste module, most commonly when one of those providers is a .js.flow file.
* Fix ability of `flow stop` to kill stuck servers
* Fix rare crashes when processes exit unexpectedly

### 0.175.0

Notable changes:
* Improve error messages when using a type as a value
* Improve error messages in `for` loops
* Correctly determine the scope of default expressions in function params
* Add `FLOW_CONFIG_NAME` env as alternative to passing `--flowconfig-name`
* Add `file_watcher.mergebase_with.{git,hg}` configs to support projects accessible via multiple VCSs (e.g. git mirrors of hg repositories)

IDE integration:
* Surface deprecated autocompletion results so they appear with a strikethrough in VS Code
* Fix a bug where changing libdefs, package.json files or the .flowconfig multiple times will cause the IDE to report "server is stopped".
* Fix a bug where the IDE is unable to start the server if it is downgraded
* Improve suggestions when autocompleting keys in an object literal
* Fix an exception when autocompleting a result that would add to an import to an existing `require()`

flow-remove-types:
* Fix handling of `this` param with trailing commas

### 0.174.1

Fix crash when Flow receives duplicate file deletion notifications introduced in v0.174.

### 0.174.0

Likely to cause new Flow errors:
* Flow now detects sketchy null errors that arise via optional chaining, and which involve opaque types, type aliases, or generics.
* Fixed an issue that caused us to previously miss some errors when the type involved was an intersection.

New Features:
* Access from a union of exact objects, on a property that is on one but not all of the objects, now results in the type of that property or `void`, instead of an error.
* Add type checking support for logical assignment operators.

Notable bug fixes:
* Fix a crash when using the default file watcher (dfind) on Windows if an [include] directory doesn't exist.
* Do not attempt to unify the type of a `declare` function that comes after a function declaration of the same name ([try-Flow](https://flow.org/try/#0GYVwdgxgLglg9mABACwBQEoBciDOUBOMYA5ogN6L4CmUI+SARA4gL4BQAJlRADYCG1RKEiwEKDNgBGcODwDcQA)).
* Improve wording of class-extends errors ([try-Flow](https://flow.org/try/#0FAEwpgxgNghgTmABAN3og9gIwFYC5EDeAvgNzDQwDOliAIomAB4AuYAdiDVtoUUA)).
* Don't report `this-in-exported-function` for functions with a `this` parameter.

Misc:
* Added `--list-files` flag to `status` and `check` commands. This will dump a list of files that contain errors instead of the error messages.
* The default file watcher (dfind) now logs to a `.dfind_log` file alongside the `.log` and `.monitor_log` files.
* Changed the way Flow computes coverage for type parameters with uncovered upper bounds. Previously, a type parameter was always considered covered when it appeared in a function signature, class declaration, or type alias, even if the type parameter's upper bound was `any`. With this change, such type parameters are now considered to be uncovered. This results in Flow now computing a lower coverage percentage on modules where this pattern is present or that import types from other modules that use this pattern.

### 0.173.0

Likely to cause new Flow errors:
* Modified the behavior of `instanceof` checks and other refinements to more consistently detect when impossible or invalid refinements are made. This will result in new errors in such cases.

New Features:
* Improved error messages by providing a suggested property when properties are missing in a conditional context.

Notable bug fixes:
* Fix "matching property" checks with generic inputs ([try-Flow](https://flow.org/try/#0GYVwdgxgLglg9mABMAjAHgCoC5EHIDOuiAPngA64B8AFALa04DeiUAnmQKY4aIC+AlIkYAoRIhjBEdWgDo2nRAF5leQvwDciAPRbEIfBwAmLOIg4Anc3HPDew4fI6IAsh3z4AhgHMnioS3YuVVwAGkQANxwwEFoAIws+En9HHFwKMIAPHHwocxgwLz51e1BIWARkACY0ZxxXd28OGloAfWsYLzrBETEIBBzEejqlQba8r2KxCSl6OUClFQJcDW1dfSMTM0trWyA)). Thanks @gnprice for bringing up this issue and exploring potential causes!
* Fixed a bug where the use of certain utility types in polymorphic functions caused errors to not be reported (e.g. [try-Flow](https://flow.org/try/#0FAMwrgdgxgLglgewgAkgJQKYEMAmB5CAGwE8AeAFQD4AKADwC5kASTXAkiygSkfOQG9kAJwwwwQlLQDcyAL7AoSAM4xkIBAkYts+ImX7rNyFULgQA5rMrIAvAMOMARACMsQx7KkLlq1zhLkxAAOGDiM-K4AXowmZpa2qBCsuiTUhlwyAPSZyOQAFnBKxnkIYIQ4yBhCQghCwEA))
* Fix a bug where the [method-unbinding] error was reported twice

Misc:
* OCaml version 4.12.1 is required to build Flow
* React default exports are now inexact, as technically this can include unknown properties (experimental features)

Parser:
*  Fixed a bug where type parameter instantiations that start with `<` (e.g., `f<<T>()=>S>()`) would be incorrectly parsed as the left shift operator.

Library Definitions:
* Add bindings for NodeJS [`util.types`](https://nodejs.org/docs/latest-v10.x/api/util.html#util_util_types)

### 0.172.0

Bug Fixes:
* Exact objects are treated as truthy in logical expressions
* Add support for operations on `$Values<>` types in an export position (e.g. [Try Flow](https://flow.org/try/#0CYUwxgNghgTiAEA3W8D2AjAVgLngb3gDNcByQkgGngHNTqT4BfAbgChRJYFkZ4BrXABIAalAgBXEAGcAPABcAngAcQqQmiwA+NiAAeS1DDnxFK+ABV4AXhPLV6vgDoIIAHbU5AC2ZA))
* Respect the order of `module.file_ext` when resolving imports. Before, they were tried alphabetically.

Misc:
* Change `signature-verification-failure` from a lint to a normal error. It is no longer valid in the `[lints]` section of `.flowconfig`.
* Improve performance of IDE commands when Flow is busy typechecking

### 0.171.0

Likely to cause new Flow errors:
* Disallow duplicate member names in classes.
* Error if an interface is imported as a value.
* The `experimental.new_merge` option becomes `true` by default. This is an internal change in the way file signatures are computed, but may cause new `[invalid-exported-annotation]` errors when a file exports types that include certain kinds of complex operations, such as projections over complex utility types like `$ObjMap`. These errors can be fixed by replacing these complex types with simpler ones. In certain cases, the newly added `$KeyMirror` and `$ObjMapConst` types (that are exempt of this restriction) can be used instead.

Notable bug fixes:
* Fix Watchman file watcher on Windows when an include path doesn't exist.
* Fix a parse error when a newline appears after a class method definition's name.

Parser:
* Add support for parsing logical assignment operators (thanks @strub).
* Fix compliance with estree: `export * as x from 'y'` is now an `ExportAllDeclaration` with an `exported` key.
* Permanently enable parsing of nullish coalescing, and remove the `esproposal_nullish_coalescing` option.
* Permanently enable parsing of optional chaining, and remove the `esproposal_optional_chaining` option.
* Permanently enable parsing of class static fields, and remove the `esproposal_class_static_fields` option.
* Permanently enable parsing of class instance fields, and remove the `esproposal_class_instance_fields` option.

Library Definitions:
* Make some `options` parameters in node's `fsPromises` optional.

### 0.170.0

Another light release due to the new year. Changes are mainly internal --
refactoring or progress towards future features.

Notable breaking changes:
* Disallow duplicate member names in classes

### 0.169.0

This release is light on changes due to the holidays. Happy New Year!

* Improve performance

### 0.168.0

Notable bug fixes:
* Improve performance of IDE requests when using lazy mode and saved state
* Improve performance of checking union types by optimizing enums more aggressively

### 0.167.1

Notable bug fixes:
* Fix crash when moving a module introduced in v0.167.0

### 0.167.0

Notable bug fixes:
* Error on invalid `@flow` modes

Misc:
* Remove the `--profile`, `--json` and `--pretty` flags from `flow force-recheck`
* Reduce usage of shared heap space

Library Definitions:
* Add support for Node inspector
* Improve Document definition
* Add document.contentType
* Add 'navigate' as valid request ModeType (Thanks @comp615)

### 0.166.1

* Fix a crash introduced in 0.166.0

### 0.166.0

* Improve recheck performance on large projects
* No longer support `flow check-contents` on non-Flow files by default; pass `--all` to force it.
* No longer attempt to provide LSP documentHighlight on non-Flow files.
* Remove legacy "weak" mode. The `--weak` CLI flag and `weak=true` flowconfig option have been removed.

### 0.165.1

Notable bug fixes:
* Improve performance of signature help
* Fix a bug that caused rechecks to be unnecessarily invalidated and restarted

### 0.165.0

Likely to cause new Flow errors:
* The Flow parser now requires that variables declared using the `declare var` syntax be annotated with a type. `declare var x;` is now illegal.

Notable bug fixes:
* No longer attempt to provide autocomplete or `flow autocomplete` on non-Flow files.
* Fix a crash when deleting a file in lazy mode
* Fix a bug that prevented some code actions from adding type imports for `export default class` definitions

### 0.164.0

New Features:
* `exports`/`module.exports` can no longer be read from in a module (e.g. `exports.foo`) to eliminate this usage of unsealed objects - you can refactor your code to directly reference `foo` instead, or just use ES modules (e.g. `export const foo = ...`)
* Improved autocomplete for the targets of `typeof` type annotations.
* Support for finding references and renaming has been removed. While these features often worked well, they also caused extremely bad performance and crashes. We hope to reimplement these features as soon as we can.
* When displaying types, always use indexed access types for `$ElementType` and `$PropertyType` utilities.
* Remove `indexed_access` option, it's always enabled now (was on by default before if the option was omitted).

Notable bug fixes:
* Fix an issue where the `refactor` code-action would hang on large inputs.
* The exports of a file with no value exports are typed as an exact empty object rather than an unsealed object. Common errors which resulted from this are accidentally doing `import typeof Foo` rather than `import type {Foo}`
* Fix error causing LSP requests to fail.
* Fix calling `watchman` on Windows.
* Fix soundness issue when comparing polymorphic types (example [try-Flow](https://flow.org/try/#0GYVwdgxgLglg9mABBOBTAThVAeAggGkQCEA+ACgA8AuRXAShqMQG8AoRZBAZykQHNEAXkRk4NbABVydISVoMR2AMrTZiJTMFy4AbnaJ0qKCHRI+ZSaq2IKdMnT0BfVqxRgeiAIY0wIALYARhhCnBhYZADknhEOrJ4AdFBwAGIwFKgAJvY6iAD0uQbgsH6oiBjocOiEASC8YHCIyQA2cADuZegV6F5gAJ6tABYYqEA)).
* Fix handling of duplicate bindings in signature builder.
* Improve error messages and type-at-pos results around `$ExactT<...>` type annotations.
* Fix referencing `this` in a JSX title member.
* `flow coverage` now returns 0% for non-@flow files; pass `--all` to also check non-@flow files.
* No longer attempt to provide definition or `flow get-def` on non-Flow files.
* No longer attempt to provide hover or `flow type-at-pos` on non-Flow files
* Update to latest LSP 3.17 beta definition of `CompletionItemLabelDetail`

Library Definitions:
* Fix `navigator.clipboard.write` type and add `ClipboardItem` (thanks @Egrodo).
* Add node.js `fs.copyFile` overload to support the case where the `mode` param is not specified (thanks @Brianzchen).

Misc:
* Removed the `flow suggest` command.
* Improve codemod CLI error handling (Thanks @gnprice).
* Changed scheduling of LSP commands in parallel with typechecking, to prioritize commands.
* Removed `flow find-refs` command.
* Removed the `--expand-type-aliases` flag from all Flow commands, it has caused several performance and non-termination issues without providing much value.
* Improved codebase quality (thanks @gnprice).

Parser:
* Invalid `typeof` type annotations, where the argument to `typeof` is a not a variable or member expression, are now parse errors rather than type errors.

### 0.163.0

New Features:
* The deprecated `*` type is now an alias to `any`. You can use `flow codemod replace-existentials`, which is available in previous versions of Flow, to upgrade your codebase to not use `*`. This codemod command will not be available in the current or future versions of Flow.
* Add (experimental) codemod to annotate variables that cause constrained write errors in future Flow versions.
* Add (experimental) codemod rename variable names that are reused with different types.
* Add --include-comments and --include-locs flags to the `flow ast` command.
* Add $ObjMapConst<O, T> builtin type, alias for $ObjMap<O, () => T>.

Notable bug fixes:
* Fix bug in `$KeyMirror` so that it respects the optionality of its arguments.
* Type `JSON` as object rather than a class. This prevents code such as `new JSON()` which is a runtime error.

Library Definitions:
* Add `InputEvent.dataTransfer`
* Declare `AggregateError` class - fixes #8764 (thanks @lachlanhunt).
* Improve `UIEvent`, `InputEvent` and `KeyboardEvent` types.

Misc:
* Fix typo in CONTRIBUTING.md (thanks @jereef).
* Clarify comments for ConcretizeTypeAppsT logic after Subtyping_kit refactor (thanks gnprice).

Parser:
* Fix a bug that allowed comments after the `#` in private identifiers.

### 0.162.2

* Fixes a bug with `experimental.prioritize_dependency_checks`

### 0.162.1

* Fix a bug where errors in `node_modules` were shown in lazy mode. It is now consistent with non-lazy mode.

### 0.162.0

Likely to cause new Flow errors:
* As part of our work on [Local Type Inference](https://medium.com/flow-type/introducing-local-type-inference-for-flow-6af65b7830aa), unannotated variables now require a non-null value to be assigned to them at some point in the program. If a variable is intentionally only null, it can be annotated with `null` (`let x: null = null`); if only undefined, then it can be annotated with `void` (`let x: void;`).
* Function and class names can no longer be reassigned, even when not exported
* Fix positioning of [missing-type-arg] errors

New Features:
* Add `$Partial` utility type. This utility converts all of an object's or interface's named fields to be optional, while maintaining all the object's other properties (exactness, sealedness, etc).

Notable bug fixes:
* Improve performance of `strict_es6_import_export`
* Fix an infinite recheck loop when deleting a file when using saved state
* Fix a bug where the server would stop after running a CLI command if the server was starting with `--autostop`

Misc:
* Type `import.meta` as object
* Fix jsdoc support on imported `declare function`
* Make string literal autocomplete obey `format.single_quotes` setting
* Improve reliability of "refactor" code actions

### 0.161.1

* Fixes a bug with `experimental.prioritize_dependency_checks`

### 0.161.0

Likely to cause new Flow errors:
* Only consider variables as "constlike" if their assignments are within their declarations. This fixes cases of unsoundness. [example](https://flow.org/try/#0G4QwTgBAHgXBB2BXAtgIwKZgNwCgBmi8AxgC4CWA9vBABYjAVEAUAlBAN44TfQQC8EACwAmXAF8cdBsxa4msCAzIATFkA).
New Features:
* Add hierarchical document symbol support to the LSP. Improves VS Code's Outline pane, breadcrumbs and symbol search.
* Autocomplete now includes primitive types like `string` and `number`, as well as utility types like `$Call` and `$Keys`.

Notable changes:
* Significantly improved overall performance.

Notable bug fixes:
* Unknown or redundant settings in the flowconfig `[lints]` section are now ignored when passing `--ignore-version`.
* Improve results of type-at-pos on `$ReadOnly` types.

`flow-remove-types`:
* Added a Jest transformer, `flow-remove-types/jest`

### 0.160.2

* The fix in 0.160.1 didn't fully fix the regression in 0.160.0. It, and the original "fix" from 0.160.0 are reverted.

### 0.160.1

* Fix a regression in 0.160.0 that caused suppressions to be ignored upon editing files in some cases when using lazy mode.

### 0.160.0

New Features:
* The `ide`, `fs` and `watchman` lazy modes have been merged into a single mode. The new mode is now SCM-aware with or without Watchman; when the server starts, it queries Git or Mercurial and checks all existing local changes (changes since the mergebase).

  The `file_watcher.watchman.mergebase_with` config option has been renamed to `file_watcher.mergebase_with`; set this to the name of your default *remote* branch (e.g. `origin/main`).

  Flowconfig setting migration:
  * `lazy_mode=watchman` -> `lazy_mode=true`, `file_watcher=watchman`
  * `lazy_mode=fs` -> `lazy_mode=true`
  * `lazy_mode=ide` -> `lazy_mode=true`
  * `lazy_mode=none` -> `lazy_mode=false`
* Added parser support for `import.meta`
* Added type checking support for `new.target` and `import.meta` as mixed

Library Definitions:
* Add type for `queueMicrotask`

Notable bug fixes:
* Improve codemod performance on large codebases
* Fix a bug that could cause "unused suppression" warnings to appear incorrectly when using lazy mode

### 0.159.0

New Features:
* Add support for git to watchman lazy mode. Git projects using `file_watcher=watchman` and `lazy_mode=fs` can now upgrade to `lazy_mode=watchman`.
* Add codemod to convert `$ObjMapi<T, <K>(K) => K>` to `$KeyMirror<T>`. Run it via `flow codemod key-mirror [OPTION]... [FILE]`.

Library Definitions:
* Add `devicePixelRatio` global (thanks @Brianzchen)

### 0.158.0

New Features:
* Add support for private class methods, implementing both the [instance](https://github.com/tc39/proposal-private-methods) and [static](https://github.com/tc39/proposal-static-class-features/) proposals! (thanks to our intern, @SamChou19815)
* Add an "Organize Imports" code action which sorts and groups the imports at the top of the file.
* Add an "Add all missing imports" code action which auto-imports all unambiguously undefined variables in the given file.

Notable changes:
* Fix a bug where private class fields like `#foo: () => 'foo'` could not be called, leading to workarounds like `this.#foo.call(this)`
* Fix a crash when hovering over a type annotation that is recursive in a parameter default
* Fix a crash when autocompleting inside an object literal with a recursive type
* Fix order of auto-import code actions
* Fix issue with utility types nested in `$NonMaybeType<...>`
* Fix Go to Definition on module reference strings (e.g. if `module.system.haste.module_ref_prefix=m#`, `"m#Foo"` will jump to the `Foo` module)
* The `file_watcher.watchman.survive_restarts` setting is now `true` by default. This setting is deprecated and will be removed in a future version.
* The `--retry-if-init` CLI flag has been removed. Setting it to `false` previously caused the command to error if the server was already running but not yet ready. Instead, use `--timeout` to avoid waiting too long.

Parser:
* Significantly improved parser performance
* Private methods are no longer a parse error

`flow-remove-types`:
* Fix to remove variance sigils on class properties (thanks @mischnic)

### 0.157.0

Likely to cause new Flow errors:
* Add a new error for unreachable code occurring in a loop after a conditional with mixed `break` and `continue` branches.

New Features:
* LSP extract to function/method/constant/class fields/type alias is enabled by default. These refactors will show up under after selecting some code. They can be disabled by adding `experimental.refactor=false` to the `.flowconfig`. (thanks to our intern, @SamChou19815)
* Add an eslint plugin, `eslint-plugin-fb-flow`, for eslint rules from the Flow team. The first rule is `use-indexed-access-type` to encourage the use of indexed access types (https://flow.org/en/docs/types/indexed-access/).
* Add another eslint rule to lint against explicit exact by default syntax (fixes #8612).
* Add a command to `update-suppressions` to the flow tool (fixes #8384).

Library Definitions:
* Better coverage of the MediaTrackSettings API.
* Improve types for ResizeObserver (fixes #8693).

Misc:
* Upgrade to OCaml 4.10.2.

### 0.156.0

New Features:
* Cross-module autocompletion and code actions that fix missing import declarations are now enabled by default. This was experimental since `0.143.0`. Can be disabled per project with the `autoimports=false` flowconfig option, or per user with the `flow.suggest.autoImports` LSP configuration.

Library Definitions:
* Enhance the builtin Node.js module types (thanks @isker).

### 0.155.1

Fix bug in shared memory garbage collection which caused Flow to crash.

### 0.155.0

Likely to cause new Flow errors:
* Ban the use of `this` inside of object methods (see https://medium.com/flow-type/sound-typing-for-this-in-flow-d62db2af969e).

Notable bug fixes:
* Fix a bug with name collisions in declared modules (fixes #8604).
* Prevent showing identifiers for autocomplete of method names.
* Preserve interfaces when used with the `$ReadOnly` utility

Library Definitions:
* Add support for fs promises (thanks @atabel).

### 0.154.0

Likely to cause new Flow errors:
* Added type checking for `this` parameters of functions (see https://medium.com/flow-type/sound-typing-for-this-in-flow-d62db2af969e).

New Features:
* Added codemod to cleanup the use of soon to be deprecated existential type (e.g. `*`). It can be run via `flow codemod replace-existentials --write .`.
* Added IDE quickfix for optional chaining when accessing an object that is nullable.

Notable bug fixes:
* Fixed crash related to a missing method unbinding error case.
* Fixed crash from calling the flow binary with an unknown argument.

Library Definitions:
* Fix MediaSource readyState to match the spec, "opened" -> "open" (thanks @sompylasar).

Misc:
* Remove `experimental.this_annot` config option and permanently enable now that our core lib defs rely on this feature.
* Remove previously defaulted on `new_check` config option for the new check mode that uses significantly less RAM for large projects.

### 0.153.0

Likely to cause new Flow errors:
* Enforce that interfaces are supertypes of objects and classes. Also prevent objects from being a supertype of a class.
* Improve soundness of `this` typing by banning the unbinding of class methods (see https://medium.com/flow-type/sound-typing-for-this-in-flow-d62db2af969e).

New Features:
* You can require an exhaustive check of an enum in a switch statement that includes a default branch. This is enabled with the comment `// flowlint-next-line require-explicit-enum-switch-cases:error`.
* Autocomplete for object keys.
* Introduce a `flow fix` command to apply autofixes from the command line to a list of files.
* Enable a new check mode that uses significantly less RAM for large projects.

Notable bug fixes:
* Fix a bug that allowed nested anonymous functions inside a default export.
* Using the `--ignore-version` flag also prevents Flow from crashing when an invalid option is in the `.flowconfig`.

Library Definitions:
* Support the `options` parameter for `require.resolve`. (thanks @skeggse)
* Return a TypedArray instead of `void` from TypedArray `sort` function. (thanks @goodmind)
* Return a `DOMStringList` instead of a string array from `IDBDatabase.objectStoreNames`. (thanks @bnelo12)

Misc:
* Add `--binary` to the `flow version` command to see the path to the current binary.
* Prevent the LSP from exiting when an invalid option is in the `.flowconfig`.
* Autofix for a `method-unbinding` error to replace the method with an arrow function.
* Autofix for a `class-object-subtyping` error to replace the object type with an interface.

### 0.152.0

Likely to cause new Flow errors:
* Fixed an issue with refinement invalidation for variables without initializers or that are written to before they're declared. You may see both new errors from improved invalidations, and some unnecessary invalidations will be removed.

Notable bug fixes:
* Fixed the behavior of unary not operator on type applications

Library Definitions:
* Add `Symbol.asyncIterator` (thanks @goodmind)
* Add missing methods to `HTMLTableSectionElement` (thanks @vitoreiji)
* Fix nullability of various DOM table properties
* Fix type of `MediaRecorder` constructor options
* Improve node `crypto` types (thanks @juodumas)
* Improve node `cluster.worker` type (thanks @magicmark)

### 0.151.0

New Features:
* Add Smart Select IDE feature. This allows you to expand or shrink your selection based on the surrounding code. For example you might expand from an identifier to the containing expression, to the whole statement. In vscode, you can do this via ctrl+shift+cmd+(left or right arrow).
* Add Enum `getName(value: TEnum): string` method. Given a value of the enum type, this returns the string representation of the name of the member. You will also need to update to the latest `flow-enums-runtime` package.

Notable bug fixes:
* Update `annotate-exports` codemod to add lowercase `react` instead of `React` (thanks @meandmax!).

Misc:
* Improve developer setup on Windows by exposing internal setup scripts and updating the Windows setup `README.md`.
* Enable `cache_live_errors_artifacts` and `cache_signature_help_artifacts` config options by default.
* Improve performance and reliability of Watchman integration.
* Error for use of `super` outside of `class` methods added in `0.148.0` has been removed.

Library Definitions:
* Update React types for React 18 release.
* Update the `Map`, `WeakMap`, `Set` and `WeakSet` constructors' iterable param to be optional as per the [ECMAScript spec](https://tc39.es/ecma262/#sec-map-iterable).
* Add typing for `PageTransitionEvent` as per the [HTML spec](https://html.spec.whatwg.org/multipage/browsing-the-web.html#pagetransitionevent) (thanks @bripkens!).
* Added support for `stepUp` and `stepDown` in `HTMLInputElement` declaration as per the [HTML spec](https://html.spec.whatwg.org/multipage/input.html#dom-input-stepup).

### 0.150.0

New Features:

* Suggest string literals in autocomplete in positions where string literal unions are expected.

Notable bug fixes:

* Use bracket syntax for autocomplete when autocompleting a property name that is not a valid identifier.
* Require the "main" property of `package.json` to have a valid Flow extension for the file to be considered.
* Fall back to `.js` extensions even when a module name looks like a resource file (e.g. `./a.svg` will look to `./a.svg.js`).
* Fix a crash that occurred when checking a `this` parameter with a generic.
* Make `$NonMaybeType<empty>` evaluate to `empty`.
* Honor format config options in more cases in code actions and codemods.
* Fix `PointerCapture` library definitions (thanks @malectro!).

Misc:

* Remove experimental `observedBits` param to React's `useContext` libdef.
* Add caching to live errors and signature help, to speed up IDE interactions (currently behind temporary flags).
* Introduce `format.bracket_spacing` option which affects code output (such as code actions).
* Improve the sorting of autoimport suggestions that have the same name, so that the module's file extension is not considered.

### 0.149.0

New Features:
* Added autocomplete for member expressions that use bracket notation

Notable bug fixes:
* Improved typechecking for expressions like `obj[Symbol.iterator]`. This is now correctly typed as the object type's `@@iterator` property.

Misc:
* Removed `esproposal.class_instance_fields`, `esproposal.class_static_fields`, `esproposal.decorators`, `esproposal.export_star_as`, `esproposal.nullish_coalescing` and `esproposal.optional_chaining` flowconfig options which have been deprecated since v0.135.

### 0.148.0

Likely to cause new Flow errors:
* We now error if the RHS of instanceof is not an object, e.g. `x instanceof null`. This catches `'instanceof' is not an object` runtime errors.
* Using `super` outside of class methods, e.g. in arrow and function properties, is now an error.

Library Definitions:
* Add `static from` to `stream$Readable` (thanks @agalatan).

Notable bug fixes:
* Fixed autoimports to not suggest vars already in scope.
* Fixed autoimports to reuse existing imports if they exist.
* Fixed autoimports to correctly sort imports of the same name.
* Fixed `null` from being a subtype of an empty interface e.g. `(null: interface {}) // Error` (thanks @mrtnzlml for the report).

Misc:
* `flow refactor` experimental command removed. Instead, use the LSP `textDocument/rename` request.
* Removed the `experimental.new_signatures` flowconfig option as well as the `--old-signatures` opt-out command line flag.

### 0.147.0

Likely to cause new Flow errors:
* This release includes some significant architectural changes that fix bugs, improves performance, and may cause Flow to find more errors.
* Fixed a bug where "[maybe types](https://flow.org/en/docs/types/maybe/)" like `?{c: number}` did not properly error when used with utility types [`$PropertyType`](https://flow.org/en/docs/types/utilities/#toc-propertytype) and [`$ElementType`](https://flow.org/en/docs/types/utilities/#toc-elementtype). `null` and `undefined` do not have properties nor elements.
* `Object.prototype` properties like `toString` and `hasOwnProperty` are no longer allowed to be accessed as global variables, which was previously allowed because `window` is an object.

New Features:
* Added Linux ARM64 support. A precompiled binary is provided, including through `flow-bin`.

Notable bug fixes:
* Improved the error message when getting properties on `null` and `undefined`
* Fixed running `flow lsp` on symlinked root directories

Misc:
* Removed support for `@flow weak`, which was a rarely-used mode in which missing annotations were treated as `any`. It is equivalent to suppressing any type errors that result from just using `@flow`. So to migrate, suppress the new errors and remove `weak`.
* Fixed a bug in which the built-in library definitions could be missing
* Fixed an obscure crash when using Watchman and Mercurial
* Added support for a `flow.suggest.autoImports` LSP setting to disable autoimport suggestions in autocomplete (currently requires `autoimports=true` to be enabled in `.flowconfig`)

Library definitions:
* Added support for node's `assert` [strict mode](https://nodejs.org/api/assert.html#assert_strict_assertion_mode) (thanks @goodmind)
* Added support for `.webp` resources (thanks @TomasBarry)
* Added missing Brotli APIs to the zlib module (thanks @isker)
* Added missing methods to `tty.WriteStream` (thanks @reyronald)
* Changed `process.exit` to return `empty`

Parser:
* Fixed a regression in 0.146 where flow_parser.js defined some global variables

### 0.146.0

Likely to cause new Flow errors:
- The rules around when refinements are invalidated have been strengthened for increased accuracy, and Flow will now invalidate refinements in a number of locations where they were not previously invalidated. This may expose new Flow errors due to variables that were previously refined to a more specific type now being treated as the unrefined, more general type.
- Object literals now have covariant subtyping with interfaces
- Interface optional properties are no longer unsafely covariant in all cases
- Interfaces with indexed properties are now properly supertypes of classes/objects with named properties matching the indexer
- `interface {}` is no longer a supertype of `void` and `mixed`

Notable bug fixes:
- Properties that begin with a `.` were sometimes being ignored by Flow, which has now been fixed

Library Definitions:
- Added missing attributes to `PerformanceResourceTiming` library definition (thanks @MIGreenberg)
- Added `withFileTypes` option to `readdir` and `readdirSync` (thanks @mrtnzlml)
- Added `composedPath` function to `Event` library definition (thanks @Brianzchen)
- Added JSDocs to `Event` library definition and made most properties read-only

Misc:
- New flowconfig options to set garbage collector parameters for workers. The new flowconfig options are `gc.worker.custom_major_ratio`, `gc.worker.custom_minor_ratio`, `gc.worker.custom_minor_max_size`, `gc.worker.minor_heap_size`, `gc.worker.major_heap_increment`, `gc.worker.space_overhead`, and `gc.worker.window_size`.

### 0.145.0

Likely to cause new Flow errors:
- Correctly typecheck destructuring defaults in function params. e.g. `function i({name = 1}: {| name?: string |}) {}` was previously not an error.

New Features:
- New options added to `flow_parser.js` to support configuring comment output. `comments` enables comment attachment and `all_comments` enables the legacy list of all comments.

Notable bug fixes:
- Correctly handle import paths within `node_modules` for auto imports.
- Fixed a bug in Go to Definition on imported values that skipped all the way to the value's type definition instead of the value's definition.

### 0.144.1

* Fixed a bug in autoimport code actions that resulted in `import type { type T } ...`

### 0.144.0

- [Improved generic type checking](https://medium.com/flow-type/flows-improved-handling-of-generic-types-b5909cc5e3c5) launched in v0.140. The deprecated implementation and the temporary `generate_tests=true` flowconfig option have now been removed.
- Fixed an issue with logical operators (`&&`, `||`, `??`) and union types
- [Object rest properties](https://github.com/tc39/proposal-object-rest-spread) (`let {foo, ...rest} = obj`) now retain the indexer of the object being destructured. In this example, if `obj` is `{[string]: string}`, then `rest` is also `{[string]: string}`.
- Made the parser recover gracefully in some cases when in the middle of typing, allowing language services to work better in the rest of the file
- Improved experimental cross-module autocomplete to include globals and built-in modules. Can be enabled with the `autoimports=true` flowconfig option.
- Fixed a bug so that the server no longer exits when merely `touch`-ing the `.flowconfig` without changing it

### 0.143.1

Notable bug fixes:
- Fixed [crash on Windows](https://github.com/meta/flow/issues/8574) and [crash on Linux](https://github.com/meta/flow/issues/8577) introduced in 0.143.0

### 0.143.0

Likely to cause new Flow errors:
- Support for Classic mode has been dropped and Types-First mode is now always enabled (Types-First has been the default mode since v0.134). The `types_first` and `well_formed_exports` flowconfig options are no longer recognized. See https://medium.com/flow-type/types-first-a-scalable-new-architecture-for-flow-3d8c7ba1d4eb/ for more about Types-First mode.
- Previously, errors in library files were sometimes being missed due to a bug. This has been fixed, which may expose errors in library files that were not previously being reported.
- Import statements are no longer allowed at the toplevel of library files. To use import statements in library files they must appear within a "declare module".

New Features:
- Added experimental support for cross-module autocompletion and code actions that fix missing import declarations. Can be enabled with the `autoimports=true` flowconfig option.
- Added `--sharedmem-heap-size` CLI flag and `FLOW_SHAREDMEM_HEAP_SIZE` environment variable, which can be used instead of the `sharedmem.heap_size` flowconfig option for setting the amount of shared memory available.

Misc:
- Added `ErrorEvent` library definition (thanks @kegluneq)

### 0.142.0

Likely to cause new Flow errors:
* Disallow flowing functions or inexact objects to indexed objects to improve object soundness. This can cause errors if you are passing a function or inexact objects when an indexed object is expected.
* Flow now processes imports before checking the body of a file. In some rare cases this can expose previously skipped errors due to the processing order.

Notable bug fixes:
* Fix `No available version of ocaml-base-compiler satisfies the constraints` error from `make all-homebrew` (thanks @bayandin).

### 0.141.0

* Improved inference of chained generic method calls, such as `Array` methods. For example, given `[1, 2].map(a => a).forEach(b => b)`, Flow now infers that `b` is a `number` rather than `any | number`.
* Fixed non-termination bugs involving recursive types
* Fixed a non-termination bug involving implicit instantiation with `_`
* Fixed autocomplete so it no longer inserts a `=` in JSX attributes that already have one
* Hovering over a use of an opaque type now includes the type's documentation

### 0.140.0

Likely to cause new Flow errors:
* New generic type checking is now enabled by default, and has to be explicitly disabled with `generate_tests=true` in a flowconfig if desired. See https://medium.com/flow-type/flows-improved-handling-of-generic-types-b5909cc5e3c5 for more about Flow's new handling of generic types.

Notable bug fixes:
* Fixed "Could not locate flowlib files" errors when multiple users run Flow on the same machine
* Fixed autocomplete and hover for imported enum types
* Fixed autocomplete suggesting types in value positions
* Fixed a bug where correct non-boolean predicate functions were rejected

Parser:
* ESTree AST now uses Literal node for init of boolean enum members

Misc:
* Updated `ShadowRoot` library definition (thanks @Brianzchen)

### 0.139.0

New Features:
* Support for `this` annotations in functions, like `function f(this: {foo: string}, param1: string): string { return this.foo; }`
* The `experimental.abstract_locations` config option is now `true` by default, as it enables significant performance improvements. This option is now deprecated and will be removed in a coming version.

Notable bug fixes:
* Fixed a false positive when a bounded generic like `K: string` flows into `$Keys<{[K]: ...}>`
* Fixed a false positive when a bounded generic like `K: 'literal'` is checked against itself like `k === 'literal'`
* Fixed autocomplete inside of JSX attribute values
* Fixed autocomplete of properties of interfaces

Misc:
* Updated `flow-remove-types` to support `this` parameters
* Added SpeechRecognition definitions (thanks @ayshiff)

### 0.138.0

Likely to cause new Flow errors:
* Improved positioning of existing errors involving unions or intersections. Will not cause new errors, but may require suppressions to be moved.

Bug Fixes:
* Fixed soundness bug involving union and intersection types
* Improved performance of typechecking exact objects against `$Exact`

Editor Integration:
* Fixed hover and go-to-definition over the LSP when the cursor is at the end of a token
* Fixed type coverage over the LSP when using `all=true` (thanks @mochja)
* Fixed an issue with running the `flow` CLI at the same time as VS Code caused by case-insensitivity of Windows paths

Library Definitions:
* Added `inputType` to `InputEvent` (thanks @Brianzchen)
* Added `intersectsNode` to `Range` (thanks @Brianzchen)

Misc:
* Built-in library definitions are now extracted to a consistent temp directory, like `/tmp/flow/flowlibs_<HASH>`, to take up less space.

### 0.137.0

New Features:
* Show code actions in the IDE for some parse errors with suggested fixes.

Notable bug fixes:
* Fixed infinite recursion case involving object rest destructuring.

Library Definitions:
* Changed return type of `Buffer.write` to `number` (thanks @fedotov).

Misc:
* Saved state files generated on one platform can now be used on another platform using the same version of Flow.
* Improved saved state memory use and startup time with more efficient handling of file paths.
* Improved error message when attempting to bind a class type.
* Minimum supported MacOS version is now 10.13.

### 0.136.0

Likely to cause new Flow errors:

* Flow now raises errors when generic type variables escape out of the scope in which they were defined.

Notable bug fixes:

* Fix a race condition related to saved state and cancelable rechecks that caused internal errors of `Requires_not_found` or `Sig_requires_not_found`.

Misc:

* Added documentation to core.js builtins.
* Add optional `propTypes` to `React.AbstractComponentStatics` (thanks @brianzchen).

### 0.135.0

Likely to cause new Flow errors:

* Turned the untyped-type-import lint into an error by default. In a later release, we will turn this into a regular type error instead of a lint error.

Misc:

* Improved exhaustiveness checking in switch statements.
* Improved autocomplete results to show documentation in more cases.
* Removed esproposal configuration options from the .flowconfig format.
* Added library definition for MediaRecorder API.
* Fixed Object.freeze to no longer incorrectly convert an inexact object into an exact object.
* Fixed handling of exported objects with spreads in types-first mode.

### 0.134.0

Likely to cause new Flow errors:

* Types-first is now the default mode, and has to be explicitly disabled with `types_first=false` in a flowconfig if desired. See https://medium.com/flow-type/types-first-a-scalable-new-architecture-for-flow-3d8c7ba1d4eb for more about types-first.

New Features:

* Added a `file_watcher.watchman.mergebase_with` flowconfig option and `--file-watcher-mergebase-with` CLI option to choose a tracking bookmark for Watchman to compute the mergebase against.
* Show JSDoc documentation in type declarations.

Misc:

* Improved the consistency of what operation is pointed to in error messages, to ensure they aren't dependent on the order in which constraints are generated.
* Filtered "did you mean" code actions to only send actions on code under cursor.
* Changed the return type of fileURLToPath (thanks @aaronasachimp)

### 0.133.0

Likely to cause new Flow errors:

* Improve reliability of inclusion check in strict equality conditionals.

New Features:

* Show JSDoc documentation in hover, autocomplete, and signature help.

Notable bug fixes:

* Fix a bug related to saved state that could cause crashes during initialization.
* Fix a bug related to saved state that could cause internal errors during rechecks.
* Fix crash with `flow focus-check` when a focused file has a syntax error.

Misc:

* Remove the `null-void-addition` lint now that it has been subsumed by an ordinary type error.
* Improved signature help for inlined tuple rest params.
* Enable the following language features by default, since they have reached stage 4:
  * `export *`
  * Nullish coalescing
  * Optional chaining
* Remove deprecated, nonstandard `itemType` and `inlineDetail` fields from the LSP `completionItem` implementation.
* Exit the worker processes gracefully when the main process crashes.

### 0.132.0

Likely to cause new Flow errors:
* Disallow using any-typed values type annotations. [example](https://flow.org/try/#0CYUwxgNghgTiAEA3W8AeAueUB2BPA3AFCFgD22AzgC7y6arwC88AzPkA)

New Features:
* Added warnings against suppressions without error codes

Notable bug fixes:
* Fixed some crashes when Flow could not connect to Watchman

Misc:
* Fix syntax errors in README badges (thanks @jamesisaac)

### 0.131.0

* "Go to Definition" on a JSX attribute name now jumps to the prop's type.
* Improved error messages for non-strict equality checks
* Fixed a bug that prevented the use of `module` in ES modules when `well_formed_exports` is enabled.
* Fixed an issue that allowed types-first mode to be enabled (`types_first=true`) when using `well_formed_exports.include`. For types-first to function properly, the entire project's exports must be well-formed, not just specific paths.
* Fixed a similar issue so that `--types-first` on the command line overrides `well_formed_exports=false` in `.flowconfig`. This makes it easier to test types-first mode.
* The `unsafe-addition` lint now defaults to being an error. It will be converted from a lint to a type error in the future.
* Removed `dynamic-export` lint which does not work in types-first mode
* Improved initialization performance

### 0.130.0

New Features:
* Improved IDE get definition behavior of imported names by jumping past `module.exports = ...` to the actual definition.

Library Definitions:
* Made `offset` an optional argument for the sized Buffer read/writes (thanks @isker!)

Misc:
* Removed deprecated blacklist/whitelist aliases for config options, use `includes`/`excludes` instead (see updated docs: for [Types First](https://flow.org/en/docs/lang/types-first/#toc-prepare-your-codebase-for-types-first) and [flowconfig options](https://flow.org/en/docs/config/options/)).

### 0.129.0

Likely to cause new Flow errors:
* $Keys<...> will no longer include non-own instance properties, matching the behavior of Object.keys.

Notable bug fixes:
* Fixed an incremental rechecking issue caused by the hashes of type destructors

Misc:
* Updated the type of `cast` and `isValid` to take in optional representation type for enums
* Added missing links to Medium articles from flow.org (thanks @jamesisaac)

### 0.128.0

New Features:
* Enable LSP support for autofix exports by default.
* Added a lint rule to ensure exports named `default` were exported with `export default` syntax.
* Enabled support for JSDoc in some LSP results.

Library Definitions:
* Add parameter to MediaStreamTrack.applyConstraints()

Parser:
* Fix parsing async arrow functions with multiple type parameters

Config:
* Remove types-first flag aliases prefixed with "experimental"
* Replace well_formed_exports.whitelist with well_formed_exports.includes

### 0.127.0

Misc:

* Improvements to editor in flow.org/try, including syntax highlighting
* [Fix broken links in docs](https://github.com/meta/flow/pull/8388)
* Standardized error suppression syntax, added ability to suppress errors based on error codes

Library Definitions:

* [Add KD functions](https://github.com/meta/flow/pull/7376)
* Add onended callback to OscillatorNode
* Make 3rd parameter of node.js symlinkSync optional


### 0.126.1

* Fixed an issue where changing `.flowconfig` or `package.json` and then running `flow status` would get stuck in a restart loop, if using lazy mode with Watchman and Mercurial.

### 0.126.0

* Allow indexers in exact object annotations
* Fixed type-at-pos in flow.org/try
* Improved language of LSP `window/showStatus` responses
* Improved control flow handing in logical expressions
* Allow applying utility types to opaque types in the defining module
* Added `HeadersInit` and `signal` to Fetch API (thanks @andretshurotshka!)
* Added `replace` method to `DOMTokenList` (thanks @w01fgang!)

### 0.125.1

Publishing of `flow-bin` v0.125.0 failed to include the binaries. Oops!

### 0.125.0

Likely to cause new Flow errors:

* Fixed signatures generated in types-first for classes with non-trivial expressions
  in their extends clauses, which were before treated like extending `any`.

New features:

* Types-first is no longer an experimental mode, but is now fully supported!

Notable bug fixes:

* Fixed a soundness bug that allowed refined property expressions to be treated as
  `empty` in some circumstances.

Misc:

* Added `$FlowIssue` and `$FlowExpectedError` as default suppression comments.
* Improved quality of AST and comment printing and layout.
* Improved quality of type printing for `type-at-pos` and friends.
* Add '.cjs' as a default extension.

Parser:

* Fixed ranges of sequence and assignment expressions that contain grouping parentheses.
* Disallowed newline after `type` keyword in type aliases.

Library definitions:

* Added headers timeout to http & https server (thanks, @mattconde!)
* Added flushHeaders to http$ServerResponse (thanks, @gajus!)
* Changed `thisArg` parameter for array callback functions (e.g. `map`, `reduce`) to `mixed`.
* Added more precise typing for common cases of `Array.flat`.

### 0.124.0

Library Definitions:

* Add `StorageEvent` overloads.

Misc:

* Use `textEdit` in identifier completion item responses over the LSP, to avoid possible behavior differences with different LSP clients.
* Various improvements to the pretty-printer.
* A small improvement in completeness when handling the "not" operation in the presence of unions of literal types.
* Wrap the usage message to 100 chars.
* Fix a poorly-positioned error related to polymorphic function types.

Parser:

* Improved some parser error messages by treating the end of file as a valid end of object property.
* Updated ESTree output for import expressions to match the spec.

`flow-dev-tools`:

* Add the `update-suppressions` subcommand.

### 0.123.0

New Features:
* Added support for the babel react jsx transform with the `react.runtime` flowconfig
option. When set to `automatic` the transform auto-imports jsx (the replacement for
`createElement`) under a fresh name (unique of all existing names in the source).
When set to `classic` it will continue transpiling jsx to `React.createElement`.

Notable bug fixes:
* We are now treating `$Exact<T|U>` as `$Exact<T>|$Exact<U>`. Before, when we checked
against this type we would treat it like `$Exact<T> & $Exact<U>`, instead.
* Fix to prevent infinitely expanding recursive type applications of arrays ([example](https://flow.org/try/#0C4TwDgpgBAqgPAFQHxQLxQIICcsEMSIoA+sc2eBySA3FFAFAAUA2gLpQBcpAdgK4C2AIwhYkASmpA)).

Misc:
* Improvements in comment attachment in various kinds of AST nodes. These will
help improve the accuracy of error suppression and various services that inspect
comments (printing, showing documentation, etc.).
* `$Exports<'m'>` will now lookup module 'm' directly in builtins, instead of the
environment.
* Extend union optimizations used on maybe predicate to also be applied in exists
predicates.

Parser:
* The range around type cast expression now includes the parentheses. This follows
the pattern of other constructs with mandatory punctuation like blocks, objects or arrays.

### 0.122.0

Likely to cause new Flow errors:
* Moved `react-dom` modules out of built-in libdefs. They are now available on [flow-typed](https://github.com/flow-typed/flow-typed/pull/3716).

  To install the libdefs, run `flow-typed install --flow-version=0.122.0 react-dom@16.13.0`

  This diff is necessary to allow React to make breaking changes to react-dom without force-pinning Flow users to the latest version of react-dom.
* Fixed an unsoundness in union and intersection spreads
* Improved object spread-related error messages, which may cause suppressed errors to become unsuppressed because they moved to better locations

Improved Editor Integration:
* Removed completion of function param snippets. Before, `f<tab>` might complete `foo(aParam)`, which is problematic if you don't want to call the function. Now it just completes `foo`. These snippets are further obviated by signature help, which appears when you type `(`.
* Improved accuracy of "Did You Mean?" Quick Fixes for object property type errors
* Added support for Go to Definition on export-from declarations (`export { HERE } from ...`)
* Fixed tracking of open files in LSP clients which could cause stale errors in the IDE

Misc:
* Added `HTMLUnknownElement` and support for custom elements to `document.createElement` (thanks @YevhenKap!)
* Fixed `--saved-state-no-fallback` flag so that Flow exits when saved state is not found
* Fixed an issue where an invalid package.json would prevent generating saved state
* Fixed a potential deadlock when communicating with Watchman
* Fixed types-first signatures for CJS requires introduced with var
* Enabling `experimental.types_first` now implies `experimental.well_formed_exports`

Parser:
* Renamed `RestProperty` to `RestElement` to match [estree](https://github.com/estree/estree/blob/master/es2018.md#patterns)
* Changed 'for await' nodes from `ForAwaitStatement` to `ForOf` with `await: true` to match [estree](https://github.com/estree/estree/blob/master/es2018.md#statements)

### 0.121.0

Highlights:
* Made several improvements to errors:
  * Restricted errors to only be suppressible at the error's primary location (see [blog post](https://medium.com/flow-type/making-flow-error-suppressions-more-specific-280aa4e3c95c))
  * Fixed the error grouping logic to no longer group unrelated errors that happen to share a location
  * Changed the order of locations printed in error messages so that the primary location is always printed first (which is the one you need to suppress if you want to do that)

Misc:
* Fixed the `add-comments` script to add comments on `JSXText`
* Fixed a crash when TMPDIR exceeded 83 characters
* Stopped parsing `.flow` files without `@flow`

Lib defs:
* Improved definitions for Node's `url.parse` function (thanks @chicoxyzzy)
* Added missing `console` methods (thanks @goodmind)
* Added `Element.hasAttributes()`
* Added `string` index to `NamedNodeMap`
* Fixed `Element.querySelector()` and `Element.querySelectorAll()` overloads
* Added type definition for `Promise.allSettled()`

### 0.120.1

Likely to cause new Flow errors:

* `$Shape` types now carry more information about the locations of errors that involve them, which may invalidate old suppressions.
* `any`-typed values are now refined by primitive `typeof` checks ([example](https://flow.org/try/#0PTAEAEDMBsHsHcBQjoFMAuoCGA7AnqALygAUOArtNKAFzb4CUA3MgJaSnp4AOqsHuAoWGgARAGd0AJ1Y4A5qIagA3olClBtUBQC2AI1RTmoEPQKtxOAOSYs28vsNqN+LZJnzjp2AGtEAX1BUaHFUFWcSTTpdAyMmEzBJViozAOQgA)).

New Features:

* "Did You Mean?" IDE feature that suggests corrections to your code as you type. For example, if you write foo.bar on an object foo that doesn’t have a field named bar but does have a field named baz, the quick fix will apply that suggestion.

* `declare` can now be used on class fields. When Flow implemented the class fields proposal, uninitialized class fields (e.g. `class C { foo }`) were not allowed, so we used that syntax for type-only declarations (`class C { foo: string }`). Since then, they now are initialized to `undefined` and Babel 8 will start to leave them. To maintain existing behavior, change uninitialized fields to use the `declare` keyword, signalling to Babel that they should be stripped (`class C { declare foo: string }`).

Notable bug fixes:

* Fixed possible stack overflow from overly long traces.
* Fixed infinite recursion case involving polymorphic types.
* Various improvements to the output of `type-at-pos` and `signatureHelp`

Misc:

* Fixed libdefs of `Notification` to have read-only fields

### 0.119.1

Notable bug fixes:
* Fixed a bug that would cause Flow to crash on some rechecks.

### 0.119.0

New Features:
* Implemented LSP textDocument/signatureHelp, which shows parameter hints when you are within an argument list in a call or `new` expression.

Misc:
* Removed the `minimal_merge` flag
* Added optional callback for node dgram socket.close function (thanks @davidnaas!)

Parser:
* Made arguments on `new` expressions optional

### 0.118.0

New Features:
* Autocomplete for nullable objects will now suggest completions with optional chaining syntax
* Added a new lint, `unsafe-addition`, which warns if either operand of an addition is `null` or `void`

Breaking change:
* The output of `flow autocomplete --json` no longer includes location information. This might affect some IDE integrations which have not yet updated to Flow's LSP server. The LSP integration is unaffected.

Misc:
* Added the `dir` field to the `Document` libdef (Thanks, @lukeapage!)

### 0.117.1

* Improved timeouts related to Watchman that could cause the server to fail to start if Watchman is slow to respond. Added a `file_watcher_timeout` .flowconfig option and `--file-watcher-timeout` argument to `flow start` and `flow server`, which defaults to 120 seconds.

### 0.117.0

Notable bug fixes:
* Improved the behavior of `$Diff` and other type destructors when applied to unions.

Misc:
* Improved error messages when attempting to use watchman without watchman installed.
* Removed uses of `Symbol` from libdefs in favor of `symbol`.
* Fixed definition of `fs.promises.mkdir` (thanks @gabrielrumiranda!)

### 0.116.1

Notable bug fixes:

* Fix #8259.

### 0.116.0

New Features:

* Flow now suggests similar names (if applicable) when issuing missing property errors.

Misc:

* Added a rudimentary libdef for `Array.prototype.flat` (#8237, thanks @nnmrts!).
* Fixed a bug that in certain specific cases led to an expression getting typed as `empty` rather than `any`.

Parser:

* `>` and `}` in JSX child text is a parse error.

### 0.115.0

Likely to cause new Flow errors:
* In [Types First](https://medium.com/flow-type/what-the-flow-team-has-been-up-to-54239c62004f) mode, exported classes and functions cannot be reassigned.
* Added similar checking of `symbol` in non-strict equality as other primitive types ([example](https://flow.org/try/#0MYewdgzgLgBBBccCeBbARiANjAvDAyqhpgBQCUA3AFAD0NMAwgIZhgiygoAOTATgKYwmAcyYBLSLDZgAtBCJYIVUJJgAPRAG0Aurhg7qa3HggUYdGAFFevELyoRj6sxeu37RgIQmX9N3YcYb2dzPxsAoA)). We allow for non-strict equality checks between two symbol types, and between a symbol and null/void ([example](https://flow.org/try/#0N4KABGD0lgwg9gWwA4EMBOBLAzvAdmAO6YAuAFmNgJ6IBG8ANmDmKgw-IQKYAm4YAY3zYSlAFyUa9JgF4wAZSmMAFAEoA3PyF4RYKhOp1GYOYqMM1miNhNyq6qDADyAaX5VblB9DCv+NgEI7b2c3CA8gr0dfNwBfEFAIbV1sAyVZBXTLfh8nci50MHJUAjwAV3ZIADd4TB4iMkwBCm5WBkJUKhs2DkJ-T3L2EJj+Qdk5bGG-a08yvB4uADNMPF4psLA5heXV+pkJ9ZBYoA)).
* We now write properties to unsealed objects recorded during speculative checking, after speculation has succeeded.
* Added more complete handling for indexers in spreads:
  - Use the indexer on the right, when the left is empty, exact and has no indexer. [[example](https://flow.org/try/#0MYewdgzgLgBAJgLhgbwD6oL4wLwoHQFgCuANiRgNwwD01KWAfgHwrpYCmATpyJxADQwIIIQAdO7AIZwAlmADmMYmRgAoUJFjskyANrROc+QF0kAWxkAPdnCy5kldeGgwAZjv1RDC0zAvXbHHwCOEECPHZKGjoAZQALEFI4GAAjdhhJMhAAdxsgA)]
  - Allow matching indexers to be spread. [[example](https://flow.org/try/#0MYewdgzgLgBAhgJgFwwN4G1oCcCWYDmAuigLY4AeApgCYC+MAvGrQNwBQoksARsmugHI4AmAB8YA7gOIwyVOo2btO0GMD4ZseIqQo16TVADoTiADQwTR3qxgB6OzADyAaxhA)]
* Removed the `non-array-spread` lint, and replaced it with a new .flowconfig option, `babel_loose_array_spread`.

Notable bug fixes:
* Improved union optimization for non-enum union equality checks.
* Fixed a bug in which `$Exact<T>` failed when `T` resolved to a `Shape<T>`. [[example](https://flow.org/try/#0C4TwDgpgBAygFgQ0jYDjQLxQCTyRAHgG8AfAKCigDMB7GgLigGdgAnASwDsBzAGjJIBfAHwBuMmQAmEAMYAbBK2hUArpxnB2NTlBVhJaCACYAFBSgAjRYxPYAogA8EGgnmSp0wgJRQMwqJwqALYWEKz8XowAbjTskuJkegbopiZgSlEohoxuEFnoPn5QAAxeokA)]

Misc:
* Improved performance of calculating the set of files that need to be checked.
* Cached the typed AST as part of the persistent connection client info, to speed up requests coming from an LSP client (DefinitionRequest, Hover, TypeCoverage).
* Improved dependency graph construction times, by using Hashtbl and mutable back edges.
* Improved `tool add-comments` by
  - using `flowlint-next-line` instead of adding full `$FlowFixMe` suppressions on lints, and
  - making the rule for inserting comments inside JSX elements also apply to JSX fragments.
* Upgraded to sedlex 2.1.

### 0.114.1

Upgrade internal dependencies sedlex and lwt. Lwt includes a fix to stack traces that we need to debug crash logs.

### 0.114.0

Likely to cause new Flow errors:
* Fixed the order of evaluation of JSX attributes and children to match runtime, so that refinements work properly. This can lead to new errors if refined values are used as children, and a function is called within an attribute, because the refinements are now invalidated in a different order. On the other hand, calling a function in a child no longer incorrectly invalidates values used as attributes. [[example](https://flow.org/try/#0JYWwDg9gTgLgBAJQKYEMDG8BmUIjgIilQ3wG4AoczAVwDsNgJa4AxACgG8APAGjgE8AvgC443UbWogARkih9+EqbKiCAlEoA2msXCIxqUZpO1xBlAG4oocLqID8kmXLgBeOABYATBSs3FcI7KLu7eFMCYcGxccACE7iY6AGRJAnEJ1NpqYuRwcAA8LLau3HAA9ABUcADOABYQmQAmcLJwEADWcBVlggIl-G5wiXzeggB8uXliMRna5lNibAFOKtmVNfVNQxDwrR1dPZP5ZSwTgkA)]
* Added support for refinements on [optional chains](https://github.com/tc39/proposal-optional-chaining)! Now, examples like `if (x?.y)` and `if (x?.y === 42)` understand that `x` is truthy in the consequent. Flow also now refines subexpressions of optional chains. For example, given `obj?.fun(obj.value)`, the second `obj` is known to be truthy since it is only reachable if the earlier `obj?.fun` is also truthy; therefore, you don't need to optional-chain it again (`obj?.value`).

New Features:
* Added typechecking support for `symbol`! Note that just like `number` is not a `Number`, and vice versa, `symbol` is not a `Symbol`. In the core lib defs, we change usage of `Symbol` to `symbol & Symbol`. Eventually, it will just be `symbol`. The `symbol & Symbol` is temporary for one version to allow for existing usage of the `: Symbol` to be codemodded to `: symbol`.

Bug fixes:
* Optimized typechecking pointwise-equal unions, from O(n^2) to O(n)
* Optimized reverse dependency computation, greatly reducing the time spent computing what needs to be rechecked after an edit
* Fixed an error message that pointed at the definition of a write-only property instead of the callsite that tried to read it [[example](https://flow.org/try/#0CYUwxgNghgTiAEA3W8AqARASgLngbwFowBXGOAOwBdc8B7AM3oGcRKB1AS2EoAtdziAWwBGIGAF9xAbgBQHevAAUGTADoSZEFQCU+eOJlA)]
* Fixed an issue where coverage results could be incorrect in [Types First](https://medium.com/flow-type/what-the-flow-team-has-been-up-to-54239c62004f) mode when a file is rechecked
* Fixed an issue when Flow prints evaluated types like `$ElementType`, that led to printing `empty` instead. This improves hover tooltips.
* Fixed several issues causing various processes to die uncleanly while exiting.


### 0.113.0
Bug fixes:
* Fixed autocomplete when at the end of file
* Stopped filtering lints (e.g. `signature-verification-failure`) out when running `flow check-contents`

Parser:
* Added support for `export default class implements Foo {}` [[example]](https://flow.org/try/#0JYOwLgpgTgZghgYwgAgJIDED2nkG8C+AUIRAB4AOmUYyAJhPAK4A2NCzcAzp8sALblmEPhHA8M2PESA)
* Added support for type parameters in anonymous class expressions [[example]](https://flow.org/try/#0BQYwNghgzlAEA8ANAfLA3gXwJQG4g) (thanks @nicolo-ribaudo)

Library definitions:
* Fixed type of `navigator.mediaDevices.getUserMedia` (thanks @thecotne)
* Added missing `InputEvent` to `removeEventListener`
* Updated type of `ServiceWorkerContainer.getRegistration`

Misc:
* Added `module.system.node.main_field` option (see #8128 for more details)

### 0.112.0

Likely to cause new Flow errors:
* JSX expressions now use the new spread semantics added in v0.111.0
* Attempting to write to or update a read-only property using operator assignment is now a type error

New Features:
* New `ambiguous-object-type` lint warning against use of `{}` object types (prefer `{||}` or `{...}` instead, even when `exact-by-default` is enabled).
* Improved support for optional chaining

Notable bug fixes:
* Malformed type annotations no longer trigger the `unclear-type` lint error
* Builtin classes can no longer be extended

Misc:
* Improved performance for utility types and refinements in unions
* Various improvements to autocomplete and get-def IDE services
* Add `useDeferredValue` and `useTransition` to React library definitions
* Add `bytesWritten` function to library definition (thanks @farzonl)

### 0.111.3

Notable bug fixes:
* 0.111.2 did not include the commit that bumped the Flow version inside of Flow. (@jbrown215 was a clown)

### 0.111.2

Notable bug fixes:
* Spread performance improvements that can prevent timeouts in extreme cases.

### 0.111.1

New features:
* Add `module.system.node.root_relative_dirname` to allow you to configure where root relative paths resolve to

Notable bug fixes:
* Fixed the regex generator that handles the <VERSION> magic string in Flow suppression comments
* Allow spreads of bools, strings, and numbers to support common React Native patterns

### 0.111.0

Likely to cause new Flow errors:

* Fixes to object spread. See https://medium.com/flow-type/spreads-common-errors-fixes-9701012e9d58

New features:
* Introduced an experimental flag (`experimental.minimal_merge`) that speeds up rechecks when the experimental types-first mode is in use. This flag will be turned on by default and then removed in future releases.
* Turned on `experimental.allow_skip_direct_dependents` by default. The flag will be removed next release. This flag speeds up rechecks when the experimental types-first mode is in use.

Notable bug fixes:
* Fixed a bug where the ocaml representation of union types caused crashes in rare scenarios
* Fixed a bug in node module resolution which allowed `module.system.node.resolve_dirname=.`. For those who relied on this bug to import modules using root-relative paths, you can now use `module.system.node.allow_root_relative=true`. See [#8156](https://github.com/meta/flow/issues/8156) for more details.

Misc:
* Add getElementById to DocumentFragment
* Add missing methods to Blob (thanks @lyleunderwood!)
* Allow clients of `flow status`, `flow check`, etc. to choose character offset style
* Support length refinement on tuples (thanks @ilya-bobyr)

Parser:
* Allow => in objects in return types of arrow functions

### 0.110.1

Notable bug fixes:
* No longer show live Flow errors for files without @flow, unless `all=true` in the `.flowconfig`
* No longer show live Flow errors for ignored files
* Re-generate live errors after a recheck

### 0.110.0

New Features:
* Flow will now send type errors as you type to LSP clients. To disable this behavior, add `experimental.disable_live_non_parse_errors=true` to the `[options]` section of your `.flowconfig`.

Notable bug fixes:
* Fixed file descriptor leak leading to LSP connection refusal in some cases
* Improved the behavior of the `[declarations]` configuration in the presence of dependency cycles (thanks @STRML!)

Misc:
* Added `--evaluate-type-destructors` to `type-at-pos` command.
* Added `--evaluate-type-destructors` and `--expand-type-aliases` to `dump-types` command (thanks @goodmind!)
* Changed `process.env` values from `?string` to `string|void` (thanks @FireyFly!)
* Improved detection of rebases when using watchman file watcher
* Improved positions for error messages involving the deprecated `*` type

### 0.109.0

Likely to cause new Flow errors:
- Flow was previously not typechecking `delete`, but now does
- `Object.defineProperty` and similar methods now adhere more closely to the spec
- Allow defaults for properties that may not exist in React components

Notable bug fixes:
- `$NonMaybeType<mixed>` no longer includes null and undefined (Thanks @goodmind)

Misc:
- Deprecated `$Supertype` and `$Subtype` utilities are now removed entirely
- flow-upgrade should now use https over git (Thanks @lukeapage)
- Autocomplete now fires on spaces in JSX
- Better error messages when a value is used as a type

Performance:
- Reduced memory usage by using a more compact representation for code locations
- Types-first should no longer check direct dependents of files where the signature has not changed

Library Definitions
- Add a number of CSSOM interfaces + fix `HTMLStyleElement.sheet` type (Thanks @kof)
- Add `AsyncIterator` to `node.js` (Thanks @goodmind)
- Add `undefined` to prelude (Thanks @goodmind)
- Add String.prototype.matchAll (Thanks @goodmind)

### 0.108.0

Notable bug fixes:

* Batch coverage info now persists through rechecks.
* When a file with @preventMunge is in a cycle with a file that does not, the un-munged file no longer has exported munged properties checked for annotations.
* Fixed a bug where autocomplete would not be triggered in a file with Windows-style line endings.
* Improved error messages for uses of `$ObjMap`, `$ObjMapi`, `$TupleMap`, and `$Call` with incorrect arity.

Misc:

* The deprecated `flow ide` command and associated machinery have been removed.

Library definitions:

* Added the missing parts of the Pointer Lock spec to libdefs.
* Thanks to @lyleunderwood for adding scrolling support to libdefs.
* Thanks to @goodmind for adding a definition for `undefined` to the prelude.

### 0.107.0
New Features:

* Implement type refinements for property accesses through brackets (#7597, thanks @goodmind).

Notable bug fixes:

* Fix several issues with autocomplete that prevented optional properties, type aliases to utility types, types with default type arguments, and several other edge cases from being autocompleted.
* Fix Not_expects_bounds crash with `%checks` (#7863).

Misc:

* Make the LSP `textDocument/documentHighlight` request serviceable while Flow is in the middle of a recheck.
* Fix minor off-by-one error which caused some parts of traces to be pruned when using the `--traces` flag.
* A minor improvement in error messages when `undefined` is involved.
* Reduce memory usage slightly when abstract locations are enabled.
* Prevent log spew when `flow lsp` is started while the server is initializing.
* Improve completeness of sighashing. We recently observed an incremental bug caused by incomplete sighashing. This speculatively addresses similar potentially problematic cases.

Library Definitions:

* Add webkitGetAsEntry to DataTransferItem.
* Switch several properties to optional in Notification and NotificationOptions (#8032, thanks @pauldijou).
* Map/Set fix symbols (#7560, thanks @goodmind).
* Fix Array#reduce and Array#reduceRight (#7902, thanks @goodmind).

### 0.106.3

We found and fixed a bug introduced in 0.105.0. Some internal code was using a hashing function and assumed collisions were far less likely than they proved to be. This could lead to random nonsensical errors which would then disappear, usually involving missing object properties. This likely only affected extremely large projects.

### 0.106.2

Fixed the stack overflow reported by [#8037](https://github.com/meta/flow/issues/8037). Thanks [@lukeapage](https://github.com/lukeapage) for isolating the repro!

### 0.106.1

Forgot to cherry-pick `[rollouts]` (an experimental new .flowconfig section) into v0.106.0

### 0.106.0

Likely to cause new Flow errors:
* We're starting to make changes to how Flow models object spreads. For more [see this announcement](https://medium.com/flow-type/coming-soon-changes-to-object-spreads-73204aef84e1)
* Updated parsing of the experimental nullish coalescing `??` operator. It now has a lower precedence than `||` and `&&`, and parentheses are required to nest it with them.
* Flow wasn't typechecking the properties of certain obscure JSX usage (namespaced identifiers, member expression with @jsx / @csx), so would miss type errors in their expressions (e.g. `<a:b prop={"hello" * 10} />` now errors)

Notable bug fixes:
* Fixed a bug where merge or check jobs would crash when a parse error was added to a file in a cycle. The crash was silent but unintended.
* Types-first no longer ignores the `munge_underscores` flowconfig option

Misc:
* Various libdef updates. Thanks for all the PRs!

Parser:
* Improved error messages for missing semicolon
* Comments are now correctly attached to `break` statements and array patterns
* `libflowparser` now supports `esproposal_nullish_coalescing` as an option

### 0.105.2

v0.105.0 started running the Flow server in a cgroup on Linux distros that support [cgroup v2](https://www.kernel.org/doc/Documentation/cgroup-v2.txt). However, some versions of `systemd`, which manages cgroups, contain a bug that caused the Flow server to fail to start. This release avoids using `cgroup` on these systems. (#8012)

### 0.105.1

This was an npm-only release to fix a packaging issue. No updated binaries were published.

### 0.105.0

Likely to cause new Flow errors:

* Types for `FileReader` properties and methods are now more precise (e.g., some parameters typed
  `any` are now typed `ProgressEvent`; some properties now have `null` added to their types). Thanks, @nwoltman!

* The value type parameter `V` of `$ReadOnlyMap` and `$ReadOnlyWeakMap` is now covariant. Thanks, @goodmind!

* Types for the `vm` module in node.js are now more precise. Thanks, @goodmind!

* The deprecated `$Enum<...>` utility type has now been deleted. Use `$Keys<...>` instead.

* Indexing tuples with floats is no longer allowed.

New Features:

* Added support for `React.Profiler` (React v16.9+). Thanks, @bvaughn!

* Added a `--types` flag to `flow graph dep-graph` to output only "type" dependencies: the subset of
  imports that the types of a module's exports depends on. (Without the flag, we output "code"
  dependencies: the set of all imports of a module.)

* Preliminary support for automatically inserting annotations on a module's exports through
  LSP. Thanks to @akuhlens (summer intern with the Flow team)!

* Preliminary support for definite assignment checking of class instance properties. Thanks to
  @pzp1997 (summer intern with the Flow team)!

* Added an option to `.flowconfig` for exact-by-default objects.

Perf fixes:

* Fixed a non-termination issue with a recursive use of mapped types.
* Fixed an exponential-blowup issue with a combined use of spreads and unions.
* Fixed an exponential-blowup issue with recursive use of array spreads.

Misc:

* Fixed LSP init to say codeLens is not supported.
* Fixed lots of cases of bad error positioning, unblocking improvements to error suppressions and
  error streaming. Thanks to @mvcccccc (summer intern with the Flow team)!

Parser:

* Improved a bunch of "unexpected" parse errors, providing what was expected in the error message.
* Fixed a bug in parsing of params in function types.

### 0.104.0

Likely to cause new Flow errors:
* Fixed the definition of `Function.prototype.apply` to only accept array-like objects, not any iterable.
* Improved error positioning, which may cause previously-suppressed errors to become unsuppressed.

New Features:
* `non-array-spread` lint rule: Fires when a non-array iterable is spread. This is useful for modeling the `loose: true` mode of `@babel/plugin-transform-spread`, where such code causes a runtime error.

Notable Bug Fixes:
* Fixed a performance regression when computing dependencies in large projects
* Fixed built-in library definitions that needed to be explicitly inexact to pass the `implicit-inexact-object` linter.
* Improved libdefs for many browser APIs (e.g. Media Streams, MIDI, Permissions, Workers) (#7737, #7805, #7806, thanks @goodmind!)

Misc:
* Various improvements to the types-first signature generator
* Improved the name of the server master process in `ps`
* Improved the output of `flow check --profile`

`flow-remove-types`:
* Remove opaque types and `declare export`

Parser:
* Fixed a bug allowing `await` to be a parameter in async functions
* Several improvements to the experimental comment attachment algorithm


### 0.103.0

New Features:
* Added `--types` flag to `flow cycle`, when given only type dependencies are used to compute cycles

Notable bug fixes:
* Fixed a bug when destructuring unions gave spurious errors [[example]](https://flow.org/try/#0GYVwdgxgLglg9mABMAFAbwA6ILyIAwC+AXIpiWCALYBGApgE4A+AzlPTGAOYECUpBAKFCRYCRJ3QZipDOSp16BRIxklW7Lr35A)
* Updated for-in/for-of head expressions to be evaluated in the correct scope [[example]](https://flow.org/try/#0MYewdgzgLgBAHgLhgQQE6oIYE8A8YCuAtgEYCmqAfDALwwDaAugNwBQAZiKjABSiSxwYINvACUMAN4sY8VgF8gA)

Performance:
* Improved the calculation for what are dependents of a file, reducing work during rechecks

Library definitions:
* Added support for Array#flatMap (thanks @goodmind)
* Replaced usages of `Object` and `Function` (which are aliases for `any`) in library definitions with `any`
* Removed some usages of `any` from library definitions
* Updated Function#apply to accept strictly two arguments (thanks @goodmind)
* Added SpeechSynthesis definitions (thanks @goodmind)

Misc:
* Updated/added HTML spec URLs in comments (thanks @kevinSuttle)
* Fixed parsing of anonymous class implements clause (thanks @goodmind)
* Added support for printing mixins and implements (thanks @goodmind)

### 0.102.0

Likely to cause new Flow errors:
* Function components with no arguments get a sealed empty object type as props.
* Moved `MixedElement` export into the module declaration, so it will now need to be qualified as `React.MixedElement`.

Notable bug fixes:
* Fixed error positioning around utility types (e.g. `$ObjMap`).
* Omit reporting error stack traces to end users over LSP.
* Fixed bug where Flow would crash when variable has same name as a type (fixes #7825)

Misc:
* Refactored coverage computation to use the typed AST. This enables coverage results over more locations that earlier.
* Improved server and monitor error logging.
* In typing object types as react components, account for the `defaultProps` property and make them compatible with `React.AbstractComponent`.
* Optimized the way module exports are populated to prevent recursion limiter exceptions.
* Improved error messages for invalid `BigInt`s. (thanks, @goodmind!)
* Hovering over an imported type alias returns its definition. (thanks, @vicapow!)
* Fixed semver comparison to allow for suffixes such as `rc`.

Libdefs:
* Remove `Object` type (equivalent to `any`) from `WeakSet` and `$ReadOnlyWeakSet`. (thanks, @goodmind!)
* Add methods to Node HTTP ServerResponse type definition. (thanks, @chrislloyd!)
* Add definitions for the Web Animations API. (thanks, @goodmind!)

### 0.101.1

Notable bug fixes:
* Fixed a bug with suppressions in the experimental types-first mode.

### 0.101.0

Likely to cause new Flow errors:
  * `$Keys` now produces a more precise type, which may find errors where incompatible strings were passed to something expecting the `$Keys` of some object.

New Features:
  * We released a new implicit-inexact-object lint to detect when an inexact object is used without explicitly adding `...` to the
    end of the props list. See [here](https://medium.com/flow-type/on-the-roadmap-exact-objects-by-default-16b72933c5cf) for context.
  * Function type parameters may now use default arguments. This is not yet supported by babel.

Notable bug fixes:
  * Fixed a bug with ranges returned by autocomplete
  * Fixed a bug where errors with bad locations reported over the LSP could cause the editor to clear all errors.

Misc:
  * `React.memo` and `React.lazy` now both allow you to specify an instance type via `React.AbstractComponent`.
  * Various performance improvements to union types.
  * Various libdef fixes and improvements.
  * Various improvements to error positioning.
  * The recursion limit is now configurable in the .flowconfig via `recursion_limit`. Most projects will not need to override this value.

Parser:
  * Forbid private fields named `#constructor`
  * Fix duplicate private class field validation for getters/setters
  * Fix parsing of private getters and setters
  * Function type parameters may now use default arguments.

### 0.100.0

Likely to cause new Flow errors:

* The `React$ElementType` annotation, which was previously unsafe, is now strict. Before you could create an element given a component with this type using arbitrary props. To annotate any component that accepts some given props, use `React$ComponentType` instead. [Try Flow example](https://flow.org/try/#0JYWwDg9gTgLgBAKjgQwM5wEoFNkGN4BmUEIcARFDvmQNwBQdMAnmFnAArFjoC8cA3gB84AO2QgsALjioYUYCIDmcQQF96dAgFcR+YBBFwA4pSwwsUABRguqaZwjcAlALpw4lGFqiGAPGAA+AAksABtQiAAaOAASfhtHVAA6MQlVAEJfAHpA+lUGbV0YfUMdVGQCLABhA1koLXxLGvADLBEYaWw8GBiAUVCsCXaAFRYsF343OCysuF6oYihpKuRDEQh4XFq5BvhVuDDBtr30CAI4ADEIgHc4ADcABiSARgenh6nfZsgRY7hrgAWyHMdwsPDITAgWjI-1WMHBAIsWBhWQCeQYZQq1W29UaJiwZgsTg0hT0BhkWJqIjquyaJB+x06VB631aIzGvgc3ACEymMzmC2g0mGiNQbGQlDgMER-2ISjgCW46U+rN+7X+QJBYIhUJh1zhCKRKLRUz5swA8gBpYWi8WS6VsRXoaXAuAAA3xhKgboOAA9WPhUCr6Wz4KksOCAOrQUIAE2NdHydHKlSpNLxpnMUGJQA)
* The `React$ComponentType` annotation is now strict when used with refs. Before, it was possible to pass a `ref` having any type when creating an element from a component using this type. If you need to describe components that accept refs, use the `React$AbstractComponent` type instead. [Try Flow example](https://flow.org/try/#0JYWwDg9gTgLgBAKjgQwM5wEoFNkGN4BmUEIcARFDvmQNwBQdMAnmFnAArFjoC8cA3gB84AO2QgsALjioYUYCIDmcQQF96dXABs06AOKUsMLFDhYAHsZEATdNjwwAdAGESkEVhEwAPJwjcAPgE6ODhKGxMACgBKYNDQyhgAVygROG8wAIAJLC0tCAAaOAASfhgAC2BURzAuarEJVQBCbwB6TPpQ1RC4HpAYgW7uzQgRWTgAQWl7fBc3Uc8YABUWLF86oL4DLCMTDVbWuABRKGIoaQADSgILuCq4ZlZrFHQLkGBzLGtbtDgIAjgADF8gB3OAANwADI4AIyQ+F0bwTUTiLA8MgAdWgWmsZDCWAIPH41zgPCCJIAZBT8QRHP1oqo4K0AgxcKNxgAhaZUJwTABGsigDlc4AWXnW-lQRW2uygmzgMuMUH2hwA8gBpIpXAm3e6PL4vFBwC6Kky6sYwZAiXBYREclESdFYqA4vHXIkksk0uBUml0mKM5lAA)
* The `$Enum` built-in type annotation is now deprecated. Please use the semantically equivalent `$Keys` type instead.
* Destructuring patterns could previously include missing properties if the resulting binding was unused. This is now an error even when unused. [Try Flow example](https://flow.org/try/#0C4TwDgpgBA8lC8UDeUwC4oGdgCcCWAdgOZQC+A3AFAD01UAojjgPY4YAqAFhDtHplALNULSDlBQABgEdJUQlFCQpMSQBooAQwHMAZlABiAG2YB3KADcADADoAjFdtXKugK4EAxsDzMCUXQAUSGAa0qQYMACUyKRAA)

New Features:

* You can now use the built-in type `React$MixedElement` as the sound superclass of all React elements. This is a type alias for `React$Element<React$ElementType>`.

Misc:

* Add `decode` method to `HTMLImageElement` (thanks, @vicapow!)

Parser:

* Handle NonOctalDecimalIntegerLiteral
* Remove U+180e (Mongolian vowel separator) from list of valid whitespace code points
* Remove support for legacy octal literals with numeric separators
* Remove support for legacy octal bigints
* Fix various issues related to automatic semicolon insertion (ASI) for class properties

### 0.99.1

Notable bug fixes:

* Fix bug where well-formed-exports errors were reported for unchecked files

### 0.99.0

Likely to cause new Flow errors:

* The statics of function types used to be `any` but are now typed as an empty object.
* Recursive calls of named function expressions were previously unchecked, but are now checked.
* `$call` property syntax, deprecated in Flow v0.75, has finally been removed.

Notable bug fixes:

* Fix an issue where Flow would not catch certain errors involving React function components with unannotated props.
* Fix React synthetic mouse events for drag, wheel, pointer events to give the specific native event type. (Thanks, @Kiwka!)

Misc:

* Improved performance of starting a server from a saved state.

Parser:

* Fix parsing of function types inside tuples inside arrow function return types.

### 0.98.1

Notable bug fixes:

* Do not report bad module uses in unchecked files

### 0.98.0

Likely to cause new Flow errors:

* Infer `void` before typechecking starts for functions without a `return` statement, lessening the impact of a union typechecking bug (#7322).
* Fix a bug which prevented Flow from asking for required type annotations.
* Turn the `deprecated-utility` lint on by default.
* Two related changes to type refinements to fix unsoundness:
  * `mixed` refined to an array produces a read-only array.
  * `mixed` refined to an object produces a read-only object.

New Features:

* Add the ability to exclude paths included by a previous pattern in a `.flowconfig` (#7317).

Notable bug fixes:

* Fix a bug that led IDEs to report all code as uncovered (#7654).
* Fix the `untyped-import` lint rule so that `export [type] * from` triggers it.
* Flow now recognizes refinements against negative number literals.

Misc:

* Exclude `deprecated-utility` and `dynamic-export` lints when applying all=setting rules (#7473).
* Improve client/server version mismatch behavior so that the newest of the two is preserved, rather than the client version.
* Preserve exactness of the input type when using `$ObjMap` or `$ObjMapi` (#7642).
* Minor changes to metadata in the results of `flow type-at-pos --json`.
* Batch `DidOpen` notifications from the IDE in order to make checking in IDE lazy mode more efficient.
* When `flow lsp` automatically starts a server, it prefers the lazy mode set in a `.flowconfig` to the lazy mode passed on the CLI.
* Allow lints to be explicitly set to their defaults (normally redundant lint settings are disallowed).
* Fix spurious missing annotation errors when the `this` type is used incorrectly.
* Fix a bug that made `React.Element` behave differently than `React$Element`.
* Fix an edge case where object property assignments were typechecked incorrectly (#7618).
* Fix an unsoundness with addition or logical operators when combined with generics (#6671, #7070).
* Fix an issue which allowed read-only arrays to be written to if the index was of type `any`.
* Fix a bug which stymied typechecking after try/catch blocks (#7530).

Libdefs:

* Add `document.elementsFromPoint()` (#7540).
* Add `ConstantSourceNode` (#7543).
* Remove `React.Suspense` `maxDuration` attribute (#7613).

### 0.97.0

Likely to cause new Flow errors:

* Refining a variable of type `mixed` with `instanceof A` produces type `A` instead `empty` which was produced before.
* Types imported in a `declare module` are no longer automatically exported from that module as well.

New Features:

* #7518 Adds support for LSP function parameter completion (thanks @vicapow)

Notable bug fixes:

* Return a better error message when `flow coverage` is passed an invalid input path.
* Fixed a bug in which Flow crashed on very long directory paths.
* Fixed type-at-pos results when reporting the type of a callable object.

Misc:

* Improvements in AST utilities: The differ got improved support in several kinds of type annotations (literals, generic identifiers, `typeof`, tuples and interface types). The mapper got support for qualified identifiers.
* Introduces a resizable array data structure that is used in union-find.
* Improved error messages around callable and indexer.
* Type-at-pos now shows results without evaluating type destructors like object spread, `$Diff`, etc. This should lead to more compact results.
* Various refactorings in the internal type language and environment.

Library definition improvements:

* Updates in Node definitions. The stream definitions were updated to the latest version, and the readline.createInterface definitions were also updated.

Parser:

* #7471 Adds support for parsing of BigInt (Arbitrary precision integers) (thanks @goodmind)

### 0.96.1

* Object literals with spreads can be described by object types with spreads in the signature verifier/builder.

### 0.96.0

Likely to cause new Flow errors:

* Recently the `Object` and `Function` types changed their meaning from "any function type" to "any
  type."  Accordingly, various `Object` and `Function` annotations that made sense before this
  change have been updated in various library definitions.

* Various other PRs making improvements in library definitions have been merged in this
  release. These include core definitions like `Date` and `Object` as well as other DOM and Node
  definitions.

* We now issue a error when a value that is clearly not a type could be exported as such.

* We now issue an error when a function is imported as a type.

Notable bug fixes:

* Some commands are not expected to update server state. But if such a command is cancelled and we
  run a recheck before rerunning the command, not updating the server state would make it seem like
  that recheck never happened (and lead to spurious future rechecks). This has now been fixed.

* Fixed node_modules filter for lint warnings, which didn't work on Windows, and didn't respect the
  node_resolver_dirnames config option.

Misc:

* Results of `batch-coverage` in lazy mode can be misleading, as it does not account for the fact
  that the currently checked files might not be the ones the user is querying for. Running
  `batch-coverage` in lazy mode is now disallowed.

* Fixed an issue with `flow lsp` where logs would not be flushed.

### 0.95.2

* The inferred statics object type of `React.createClass({})` will contain `defaultProps: void`, instead of `defaultProps: {||}` (unsealed empty object).
* Bug fix in internal cache mechanism

### 0.95.1

* Added an overload for `JSON.stringify` allowing `mixed` input, which returns `string | void`. Without this, you can't call `JSON.stringify` on a `mixed` value at all, because while Flow does allow refining `mixed` to "not void" (e.g. `x === undefined ? undefined : JSON.stringify(x)`), it does not support refining `mixed` to "not a function" (e.g. imagine you could do `x === undefined || typeof x == 'function' ? undefined : JSON.stringify(x)`). This rolls back some of the more restrictive behavior introduced in v0.95.0, but is still more restrictive and more accurate than in <= v0.94.0.

### 0.95.0

Likely to cause new Flow errors:
* Disallow `undefined` and functions in `JSON.stringify`: `JSON.stringify(undefined)` returns `undefined` instead of `string`. Rather than make it always return `string | void`, or use overloads to return `void` on those inputs, we instead disallow those inputs since they are rarely the intended behavior. (#7447)

New features:
* `flow batch-coverage`: A new command to compute aggregate coverage for directories and file lists. Instead of producing the coverage at each location, it sums them and reports the per-file percentage and the aggregate percentage.

Bug fixes:
* Fixed incorrect reporting of signature verification lint errors in unchecked files

Other improvements:
* #7459 Add type for Symbol.prototype.description (thanks @dnalborczyk)
* #7452 Add types for String.prototype.trimStart/trimEnd (thanks @dnalborczyk)
* #7500 The "kind" of an autocomplete result is now reported over the Language Server Protocol, improving the autocomplete UI (thanks @vicapow)

### 0.94.0

Bug fixes:
* Fixed `dynamic-exports` lint's spurious errors on exported classes and functions
* Handle package.json files that are valid JSON but invalid packages

Performance:
* Reduce memory usage by filtering suppressed lint errors before formatting the errors for printing
* Quicker responses to cancellation requests

Many libdef fixes and other improvements from the open source community:
* #3209 Fix autocomplete for generic type aliases (thanks @vkurchatkin!)
* #6750 Remove shadowed generics in `Proxy$traps` (thanks @talbenari1!)
* #6000 Document async function return type (thanks @callumlocke!)
* #7448 Tweaks to built-in http module (thanks @STRML!)
* #4570 Update types for Web Audio API (thanks @fand!)
* #5836 Fix examples in libdefs/creation page (thanks @tomasz-sodzawiczny!)

Additional lib def improvements:
* Make `current` write-only in `React.Ref` - allowing union types for ref
* Add `setMediaKeys` API to definition of `HTMLMediaElement`
* Make type parameter to `http$Agent` covariant

### 0.93.0

Likely to cause new Flow errors:

* Removed a constraint involving `any` types and React proptypes for efficiency. This may result in some errors no longer being reported.

New Features:

* A new lint (`dynamic-export`) which when enabled will warn when most dynamic types are exported from a file.
* Flow now distinguishes between `any` and `empty` when computing line coverage. `empty` types are colored blue and `any` types red when using the `--color` option. Note that this may cause new expressions to be considered uncovered.

Notable bug fixes:

* Fixed a non-termination condition during `this`-substitution.
* Fixed an issue where `inexact-spread` lint errors could appear in the wrong position.

Many, many libdef fixes and improvements! Many thanks to the open source community for these, and to @nmote and @jbrown215 for reviewing and merging so many of these!

* #4388 add missing `InputEvent(Listener|Handler|Types|)` (thanks @keithamus!)
* #4664 Fix `IntersectionObserver` constructor definition (thanks @apostolos!)
* #4858 Make `ServiceWorkerMessageEvent` extend `ExtendableEvent` (thanks @keyiiiii!)
* #4772 add indexer property to string lib def (thanks @zacharygolba!)
* #5529 Add `module.builtinModules` to core libdef (thanks @simenB!)
* #5574 fix return parameter for `writable.setDefaultEncoding()` (thanks @dnalborczyk!)
* #5578 add `util.callbackify` to node type def (thanks @dnalborczyk!)
* #5628 Add lib declaration for `BroadcastChannel` (thanks @schmatz!)
* #5866 Add definition for `timingSafeEqual()` (thanks @rolftimmermans!)
* #5988 add `destroy` method to streams (thanks @hiikezoe!)
* #6091 Fix static declarations for `XMLHttpRequest` (thanks @robin-pham!)
* #6339 Fix parent of `AnimationEvent` (thanks @ngyikp!)
* #6367 Add types for `Object.getOwnPropertyDescriptors` (thanks @disnet!)
* #6471 Actualize node's `EventEmitter` API definition (thanks @antongolub!)
* #6535 add `Element.prototype.toggleAttribute` (thanks @keithamus!)
* #6614 Add `TransitionEvent` to dom libdef (thanks @koddsson!)
* #6785 Allow specifying encoding as string in options field of `appendFile`, `appendFileSync` (thanks @cappslock!)
* #6963 Add Audio declaration (thanks @vldvel!)
* #7011 Use more specific type for `navigator.serviceWorker` (thanks @dhui!)
* #7097 Add type definitions for message events (thanks @wachino!)
* #7122 Support for `Uint8Array` (thanks @cakoose!)
* #7144 Updated URL modules definitions for Node.js 10 (thanks @MrFranke!)
* #7146 Fix type definition of Node.js `dns.lookup()` (thanks @shuhei!)
* #7215 fix `https` interfaces (thanks @cakoose!)
* #7225 make `createContextualFragment` return a `DocumentFragment` (thanks @wincent!)
* #7342 add Document.queryCommandSupported (thanks @Eazymov!)
* #7358 Add `oncontextmenu` to `HTMLElement` (thanks @jasonLaster!)
* #7363 Add `MediaDeviceInfo` declaration (thanks @ea167!)
* #7367 Add `userSelect` to CSS declaration (thanks @shubhodeep9!)
* #7368 Fix `fs.promises.readFile` being incorrectly overloaded (thanks @Macil!)
* #7381 add `EventSource` to dom libdef. Likely to cause new errors (thanks @SlIdE42!)
* #7386 fix `XDomainRequest` in bom libdef. Likely to cause new errors (thanks @Mouvedia!)
* #7387 Added optional `displayName` property to `React$Context` (thanks @bvaughn!)
* #7405 Basic support for `typeof x === 'symbol'` (thanks @mroch!)
* #7420, #7430 and #7440 Various React improvements (thanks @threepointone!)
* #7423 make `useRef` type non-nullable (thanks @trysound!)
* #7445 add `Stream` type to Node thanks (thanks @goodmind!)

Misc:

* Updated our website and GitHub issue template to make it easier for open source users to contribute to Flow!
* Various improvements to the AST, including the differ, typed AST and the AST mapper

### 0.92.1

Notable bug fixes:
* Fixed a bug introduced in v0.92.0 which could cause the server to crash when using an IDE.
* Fixed `module.system.haste.name_reducers` option for Windows (#7419 - thanks [jamesisaac](https://github.com/jamesisaac))

### 0.92.0

Likely to cause new Flow errors:

Some fixes to tagged template literals will surface a new set of pre-existing errors!

New Features:

This release culminates months of hard work on quality of life improvements for IDE support.
Expect your requests to be faster, and your requests to take a bit less time.

* Several Flow commands can run in parallel now, i.e. you can still get type definitions while rechecking.
Big props to @glevi for this massive QoL improvement!
* Alongside this change, @glevi also released a new lazy-check mode that leverages `watchman` to reduce the number of checked files.
Learn about it [in the docs](https://flow.org/en/docs/lang/lazy-modes/#toc-using-watchman-lazy-mode).

Notable bug fixes:

Thank you to the opensource community for these fixes!

* #7354 Fix MouseEvent type definitions
* #7262 Update types for WeakMap
* #7100 Add missing crypto.randomFillSync and crypto.randomFill methods
* #7356 Add definitions for new debug hook useDebugValue
* #7352 Rename React hook useImperativeMethods -> useImperativeHandle
* #5665 Fix arity of clearInterval, clearTimeout
* `React.memo` now accepts any kind of `Component`

### 0.91.0

Likely to cause new Flow errors:

* Better positioning for React error messages. This may move already existing (and suppressed) errors to new positions.

New Features:

* The `--lazy-mode` flag was added to all commands that may autostart a server

Notable bug fixes:

* Fix a crash when using private class fields (fixes https://github.com/meta/flow/issues/7355)

Misc:

* Various additions and typo fixes on flow.org docs and README (thanks @fschindler, @dominicfraser, @keithamus, and @fterh)
* Misc OCaml tweaks (thanks @rvantonder)
* Huge reduction (~15%) in total memory usage (thanks @nmote)
* Huge reduction in error collation time (thanks @panagosg7)

### 0.90.0

Likely to cause new Flow errors:
* Removed unsafe rule allowing Date instances to be used as a number
* Changed $Shape<> types to reject null and void as subtypes
* Removed unsafe refinement from mixed to a function type using typeof

Pull Requests:
* #7290 Add support for Path2D constructor arguments (thanks @zpao!)
* #7221 use package that works with 7.x in babel doc (thanks @rob2d!)
* #7231 Improve type of 'mkdir' and 'mkdirSync' (thanks @mrtnzlml!)
* #7278 fix: update toc links in hoc pages (thanks @evenchange4!)

Notable bug fixes:
* Fixed issue where errors involving $Shape<> types were positioned poorly

Parser:
* Fixed decoding of html entities at beginning of JSX children
* Fixed offset calculation in estree output to account for multibyte characters

### 0.89.0

Likely to cause new Flow errors:
* Big revamp to React typing with the goal of adding support for `React.forwardRef` and better typing higher-order components. [Docs are available here](https://flow.org/en/docs/react/hoc).

New Features:
* New `deprecated-utility` lint complains about deprecated Flow types. To start off, `$Supertype` and `$Subtype` are now deprecated. They were unsound and can usually be replaced using shapes or bounded generics.
* [`React.AbstractComponent`](https://flow.org/en/docs/react/types/#toc-react-abstractcomponent) makes it [way easier to type React higher-order components](https://flow.org/en/docs/react/hoc).
* [`React.Config`](https://flow.org/en/docs/react/types/#toc-react-config) is also intended to help type React higher-order components.

Notable bug fixes:
* `flow coverage --color` handles multi-byte characters better
* `flow coverage` now supports `--strip-root`

Misc:
* We've deleted `flow gen-flow-files` due to bitrot. We do plan on building a better version in the future.
* Various libdef updates. Thanks for all the PRs!
* `{}` now consistently represents an unsealed object. You can read more in the [documentation](https://flow.org/en/docs/types/objects/#toc-unsealed-objects). [example](https://flow.org/try/#0GYVwdgxgLglg9mABFApgZygCgN7DnALkTBAFsAjFAJwF8BKbGgKCdQx3oG5EB6HxAHJxE1KnCqIAhlEQAOWUwgIMiAB5Fc+IiQrUaiALyJG3PogCiVMROlzZQA)

### 0.88.0

Likely to cause new Flow errors:

* Made `Function` and `Object` types be aliases for `any`. They were always unsafe types, just like `any`, but they had peculiar behavior. This change revealed places where they were handled improperly within Flow, and ended up surfacing type errors that were previously missed.

New Features:

* Added the experimental Watchman lazy mode (`flow start --lazy-mode watchman`). This improves the lazy mode experience for repositories which use Mercurial and Watchman. We will document it more when/if it proves itself.
* Added `flow config check` which validates the `.flowconfig`.

Misc:

* Made miscellaneous improvements to the AST differ, which improves the output of global rename.
* Made `.flowconfig` parsing less strict (in particular, if the `--ignore-version` flag is passed, do not fatal on unrecognized config options).
* Performed a code cleanup in type normalization that caused some types in `type-at-pos` to be displayed differently.
* Removed redundant information in stored ASTs resulting in a modest reduction in memory usage.
* Flow assigns long string literals type `string`, rather than the singleton type of that literal. Now, this fact is surfaced in error messages.
* Fixed stack overflows:
  * When checking a large number of files.
  * When a large number of errors are present.

Libdefs:

* Added `React.Suspense`.
* Removed `React.useMutationEffect` hook.


### 0.87.0

Likely to cause new Flow errors:
* Fixed an error in the `React.createRef` definition. Refs are for any type of value,
not just React elements.

New Features:
* Added support for subcommands to the CLI and a `flow config find` command that
finds the .flowconfig governing path.

Notable bug fixes:
* Fixed file_sig for deep destructured requires.
* Fixed a stack overflow in the `flow ls --json` command.
* Fixed a crash when the module reference prefix is used on an untyped module.

Misc:
* Removed the `flow port` command.
* Various improvements to the AST differ.
* Made progress in shifting from concrete to abstracting locations, by
  - functorizing a number of modules over their use of locations, and
  - updating the core typechecking logic to operate solely on structures containing
  abstract locations.
* Removed the redundant Expression.Member.computed field from the AST.
* Allow configuring path to node in runtests.sh.
* Refactored `JSX.frag_closingElement` of the AST to not be an option.
* Fixed error localization in `import type` and `import typeof` with default.
* Added the location of function signatures to the AST.
* Removed prototype members when autocompleting JSX props.
* Cleaned-up handling of the mixed case from LookupT.
* Optimized the case of enumerations when used as keys.

Library definition changes:
* Made geolocation `PositionOptions` optional.
* Added flow definitions for React hooks.
* Added a `calculateChangedBits` parameter to the `React.createContext` definition.
* Added `React.ConcurrentMode` and `React.StrictMode` definitions.
* Added `React.lazy` and `React.memo` definitions.

Parser:
* Refactored flowconfig option parsing into a list of parsers.

### 0.86.0

Likely to cause new Flow errors:
* New errors may arise from generic definitions which lack the necessary annotations. Flow does not infer generic types, and the types it used to infer in their place occasionally masked errors. The types it now infers are still not generic, but will mask fewer errors.
* Fixed bug in union type checking which caused Flow to miss some errors when used in conjunction with generics and certain [utility types](https://flow.org/en/docs/types/utilities/).
* Improvements to constraints involving `any` may result in new errors.

New Features:
* `type-at-pos` can reconstruct spread objects when evaluation of the spread is not possible.
* `type-at-pos` now supports implicit instantiation with `_`.
* Added core types `$ReadOnlyMap`, `$ReadOnlyWeakMap`, `$ReadOnlySet`, and `$ReadOnlyWeakSet`. These types and their mutable counterparts mostly follow the pattern of `$ReadOnlyArray` and `Array`: the read-only parent class provides non-mutating methods, and the usual mutable class extends it. **Unlike `$ReadOnlyArray`, the new types are _invariant_ in their type parameters.**
* Added the `React.StrictMode` type.
* Added the [`flowtest` package](https://github.com/meta/flow/tree/master/packages/flowtest). `flowtest` is a CLI tool for running tests on Flow types.
* Added the `sharedmemory.heap_size` option.

Notable bug fixes:
* Restructured file signatures to support overloading in exports.
* Allow named exports in exact `module.exports` objects.
* Fixed forward references for `declare function`.
* Various bug fixes to the type normalizer and to `type-at-pos`.
* `flow suggest` no longer outputs `<<anonymous class>>`.
* Imported symbols should no longer appear `Remote` in `type-at-pos`.

Misc:
* Improved formatting in JavaScript output.
* Improved the way the element type of an array is determined.
* Various improvements to the AST differ.
* `WeakMap` keys must now be objects.
* Improved profiling for error collation and formatting.
* When a union lower bound flows into a union upper bound, if both unions are enums, we use the underlying set representation to handle this in O(n log n) time instead of quadratic time.
* The type normalizer properly represents utility types instead of treating them as generics.
* The type normalizer provides more accurate information about generics.
* Flowing a string lower bound into a union upper bound occurs in O(log n) instead of O(n) when the union is an enum.
* CJS modules' namespace objects are now treated as covariant.
* The type normalizer reconstructs literal types more precisely when `preserve_inferred_literal_types` is set. This does not change the behavior of `type-at-pos`.
* Deleted the `experimental.cancelable_rechecks` option.

Parser:
* Removed the deprecated `expression` field from `Function` nodes in the AST.
* Enabled some tail call optimizations in `flow_parser.js` which should cause it to stack overflow in fewer cases.
* The layout generator no longer prints empty statements as `{}` instead of `;` in pretty mode.
* Allow anonymous function parameter types inside generics inside arrow function return types. For example, we disallow `var x = (): (string) => number => 123` because the first `=>` is ambiguous. However, `var x = (): T<(string) => number> => 123` is not ambiguous and we no longer disallow it.

### 0.85.0

Likely to cause new Flow errors:

* Fixed an issue that caused missing annotations errors to be suppressed.

  Please [read the full post](https://medium.com/flow-type/asking-for-required-annotations-64d4f9c1edf8)
  for more information on what changed and tips on dealing with the new errors.

### 0.84.0

Likely to cause new Flow errors:

* Earlier, type constraints between `any` and other types would be dropped. Instead, they are now
  propagated. In some cases, this unblocks further constraint solving and helps find more errors.
* When a variable is equality-checked with a literal, the variable's type is refined. Earlier, if
  the variable's type was incompatible with the literal's type, it would silently be refined to
  `empty`, whereas now this is an error.

New Features:

* Added support for wildcard (`_`) type arguments to function / constructor calls. This is
  especially useful when some type arguments are sufficient to pin down the type of the result; the
  others can simply be `_`.

Notable bug fixes:

* Fixed a case that would crash `get-def` and `find-refs`
* Fixed a bug with unreachability analysis for ternary expressions
* Fixed a bug with refinements merging at the end of switch statements

Misc:

* Fixed various AST printing / layout bugs
* Made various improvements to the AST differ
* Refactored parts of `get-def` and `find-refs` to reuse code
* Made progress on abstracting locations in the core type inference engine
* Made progress on module signature verification and generation
* Merged PRs that improve type declarations: e.g., `getBoundingClient` returns a `DOMRect` instead of a `ClientRect`.

Parser:

* Support for `...` to indicate inexactness in object types

### 0.83.0

Likely to cause new Flow errors:
* Fixed a bug where type precision was lost at module boundaries

Notable bug fixes:
* Prevented an exponential blowup when union types flow into "maybe" (`?T`) or "optional" (essentially `T | void`) types
* Allowed `{p:T}` to be a subtype of `$Shape<{+p:T}>`

Misc:
* Fixed exception when using --traces
* Changed `--verbose` to not log while loading flowlibs by default; pass `--verbose-flowlibs` to override
* Added ability for LSP clients to cancel previously-sent commands
* Improved location of diagnostics via LSP while typing
* Fixed LSP textDocument/definition response when there are no results (prevents bogus jump-to-definition inside comments, for example)
* Limited number of errors returned via LSP to 200, to improve Nuclide and Visual Studio performance
* Fixed an exception when attempting to focus a file that Flow ignored

Library definition changes:
* Added `ResizeObserver` declarations
* Added missing `net$Server.listening` property
* Added `process.cpuUsage()`

Parser:
* Fixed `export default async function` to be a `FunctionDeclaration` not a `FunctionExpression`
* Made instance properties named 'constructor' a syntax error

### 0.82.0

Likely to cause new Flow errors:
* Removed the ability to use functions as type annotations. This ability was
  originally designed to support ES3-style classes, but in practice causes
  confusion and missed errors. We found that the vast majority of added errors
  were detecting legitimate bugs, and the remainder could be easily updated to
  ES6 classes.

New Features:
* Added worker utilization and GC profiling under the `--profile` mode.

Misc:
* Further improved performance of IDE requests on large codebases.
* Tweaked OCaml GC settings to reduce time spent collecting on workers.
* Improved performance and reliability of watchman integration.

### 0.81.0

Likely to cause new Flow errors:

* Fixed a bug which allowed refinements to incorrectly escape outside of conditional expressions.
* Plugged a hole which incorrectly allowed `this` to appear in a constructor before a `super` call in some cases.
* Some errors related to uses of `typeof` may be reported at a different location than they previously were.
* Disallowed shadowing class methods with incompatible properties of the same name.
* Made miscellaneous other bug fixes that manifest only in rare cases.
* Made module properties covariant, meaning that e.g. mutating properties on the object returned by `require()` is not allowed.

Notable bug fixes:

* Improved performance of IDE requests (e.g. autocomplete, get-def) on large codebases roughly 3x.

Misc:

* Made a number of improvements to the diffs generated by global rename.
* Changed how saved-states are loaded.
* Added additional information to error messages for missing annotations for type parameters.
* Disallowed class static properties named `prototype` or `constructor`.

Library definition changes:

* Added type definitions for pointer events.
* Updated DOM `scrollIntoView` to match latest spec.
* Updated `String.prototype.toLocaleLowerCase` and `toLocaleUpperCase` to allow optional `locale` parameter.
* Made `Object.prototype.valueOf` return `mixed` instead of `Object`.
* Removed the indexer from the `Object` libdef.
* Updated `TextDecoder` to properly accept a `BufferSource`.
* Added `MediaStreamTrackEvent` definition.
* Made `String.match` return `RegExp$matchResult`.
* Added additional constants to the `fs` module.

### 0.80.0

Likely to cause new Flow errors:
* We've changed how Flow lint severity is calculated. We used to look at every location mentioned by a lint and we'd use the lowest severity, where Off < Warn < Error. Now we just use the severity at the lint's first location. Flow suppression comments (e.g. `// $FlowFixMe`) can still suppress a lint at any mentioned location. Depending on your setup, this change may expose Flow lints which were turned off by accident. For example, a "Sketchy Null Check" lint in a file with that lint set to error would have been turned off if it also mentions a file with that lint off. Now that lint will show up as an error.
* Some libdef changes may cause a few errors. For example, `Headers.get` and `URLSearchParams.get` are now annotated to return `null | string` instead of `string` and `File.lastModifiedDate: any` was replaced with `File.lastModified: number`.

Notable bug fixes:
* Fixed a crash that could happen when a persistent connection disappears.

Misc:
* Many improvements to the libdefs and docs. Thanks for the PRs, everyone!
* Small perf improvement by making `BoundT` (internal representation of type parameter bounds) smaller.

Parser:
* Small change to the OCaml AST for the `extends` expression in a class declaration. Moved a couple of properties to their own node.

### 0.79.1

Notable bug fixes:
* If a server was not already started, running `flow status --flowconfig-name 'name'`
  would start a server without taking into account the new flowconfig name. The same
  is true for running flow without any command specified. This changes this by
  passing the name to the new server.

### 0.79.0

Likely to cause new Flow errors:
* A fix in requiring annotations on exports: Flow uses type variables for
  unannotated program parts. To facilitate inference, these type variables may
  not appear in input positions of exports. Before, Flow would not complain if a
  type variable appeared in *both* an output and an input position of an export
  (in that order), as the first occurrence marked the use of the type variable
  as legitimate. Now, Flow analyzes exports in a polarity-sensitive way and will
  require an annotation in the above scenario.

New Features:
* Saved state:
  - Added an initialization path, where instead of parsing every file dependency,
    it loads the expected results for each available file from the saved state.
  - A hash is stored for each file in the saved state to determine whether it has been
    modified and therefore needs to be reparsed after the state is loaded. This way,
    saved state servers can skip parsing files that are unchanged.
  - If loading saved state fails, Flow falls back to a full initialization, unless
    `--saved-state-no-fallback` is passed in which case Flow fails.
  - Saved state tests are included in the automated testing framework.
* Profiling was refactored to support hierarchical profiling. That is:
  - Support running timers inside of other timers.
  - Support merging a finished profiling object (like from a recheck) into the
    currently running profiling object (like from handling a command).
* Cancelable workloads (jobs that handle commands). If Flow determines that a file
  has changed while handling a command, it stops, performs a recheck, and then
  re-runs the workload from scratch.
* The Flow diff checker became more fine-grained, by including comparisons at the
  level of loops and variable declarations.
* Added a `name` field in the `[options]` portion of `.flowconfig`.

Notable bug fixes:
* Send the server logging context to the LSP command and use that when logging
  success and failure events instead of the LSP command's context (which is logged
  as the client context).
* Fixed stack overflow when running `flow --json` with a large error output, by
  moving to a tail-recursive implementation.
* Fixed string literal printing in `flow suggest`.

Misc:
* Library definition improvements for `React$Context.Provider` and
  `CanvasRenderingContext2D.imageSmoothingQuality`.
* Typed AST: The constraint generation phase returns a version of the AST that
  includes a type for each program node.
* Added doc details for `$Shape`.
* The changes in saved state were followed by code refactorings in various checking
  modules (`types_js.ml`, `merge_service.ml`, `rechecker.ml`, etc.).

### 0.78.0

New Features:
* Added the [`unnecessary-invariant`](https://flow.org/en/docs/linting/rule-reference/#toc-unnecessary-invariant) lint, which will flag uses of `invariant` when Flow can determine that the predicate must be truthy.
* `find-refs` now traverses union types.
* The `--flowconfig-name` flag now allows you to specify the name of the flowconfig.
* Added a `file_watcher` option for `.flowconfig`.
* Rechecks will now stop and restart as soon as Flow notices other file changes. The previous behavior was to finish the recheck and then start a new recheck. The new behavior is currently off by default and can be enabled by setting `experimental.cancelable_rechecks=true` in `.flowconfig`.
**Note**: This feature is experimental. When it stabilizes, the flag will be removed and this feature will be enabled by default.

Notable bug fixes:
* `flow-upgrade` should no longer trigger `ENOENT` on Windows.
* Running Flow with `--profile` should no longer throw on Windows.
* Suppression comments preceding lint errors for disabled lints should now be correctly considered unused and trigger unused suppression warnings.
* `ArrayBuffer`'s static indexer property is now correctly marked covariant. This should prevent errors like [this one](https://flow.org/try/#0PTAEBUAsEsGdQMYEMCusCm8mgGYBsB7Ad1HQCcyCzRoA7UAF0nUSpf2NABN0c7oG0ArQBQ6AB4AHKg1wpaCQcNAAjJFwAUASlABvEaEOtasWUgpIAnqAC8oWuhIBVOgwAcAQQuWNAFgBMWgDcBkZk6AwoZPQACpQAtnDoAHThsAR4AG7oGuZkVskqKDg45MEiAL4iIkA).
* Fixed a bug in which the polarity of a type parameter was propagated to all subsequent type parameters, causing errors when covariant type parameters followed contravariant or invariant ones.
  This bug is demonstrated by the difference in behavior of the following examples:
  - [Incorrect](https://flow.org/try/#0C4TwDgpgBAKgPAQQDQGoBCA+KBeKBvAQwC4pkoUAjEtAXwG4AoBgMwFcA7AY2AEsB7dlGaIkmABTFSSKFShoAlCXjJM+BlCgAnCMFabBhaRRoMTDCAA8wfTcCgATCMwKsANneZiADNK-y6UAD0gVAAzgAWfG72UABWrKF2FNAAtjyhoTzsAOZCNlJQ7Hx2aEA)
  - [Correct](https://flow.org/try/#0C4TwDgpgBAKgPAagIIBoBCA+KBeKBvBAQwC4pUoAjUtAXwG4AoBgMwFcA7AY2AEsB7dlGZxUmABQkyKStQCUpeKKx4GUKACcIwVusF5C0ijQbGGEAB5g+64FAAmEZoVYAbW8zEAGaZ9l0oAPQBUADOABZ8rnZQAFasIbYU0AC2PCEhPOwA5kLWUGjS7Hy2SEA)

Misc:
* Flow now prints array types as `Array<T>` instead of `T[]` in JavaScript output.
* Flow now prints `{foo:x}` as `{ foo: x }` in JavaScript output, as Prettier does.
* Library definition changes:
  - Added `destroyed` to `net$Socket`.
  - Added `document.scrollingElement`.
  - Added `dgram$Socket$rinfo` to the Node library definitions.
  - Added WebAssembly library definitions.
  - Added the `groups` property to `RegExp$matchResult`.

### 0.77.0

New Features:

* New lint to disallow `a && b` when `a` is a `number`. See https://flow.org/en/docs/linting/rule-reference/#toc-sketchy-number
* Support multi-hop/indirect find-refs in LSP
* Expose renaming through the Flow LSP

Notable bug fixes:

* Fix server crash on persistent-command unhandled exception

Misc:

* Library type definition changes: created explicit type for `Context`, added `readAsBinaryString` to `FileReader`
* AST differ utility to power rename
* More uniform bucketing of jobs to be run in parallel leads to higher CPU utilization
* Improvements to type printing
* Treat getters and setters as proto fields

Parser:

* Optimization when saving comments

### 0.76.0

Likely to cause new Flow errors:
* Flow now only allows subtyping polymorphic types with the same number of type parameters. This added strictness lets Flow avoid some slow typing code and shouldn't cause too many problems.

New Features:
* New `[declarations]` section in the `.flowconfig` which can suppress all errors in 1 or more files. Thanks [@LegNeato](https://github.com/LegNeato)! ([#4916](https://github.com/meta/flow/pull/4916))
* Added `flow server --file-watcher watchman` flag which uses [Watchman](https://facebook.github.io/watchman/) for file watching instead of the builtin file-watcher, dfind
* Added a `--expand-type-aliases` flag to `type-at-pos`. It replaces type aliases with their bodies in the returned type.
* Added `flow save-state` command to generate a saved state file (experimental & loading a saved state isn't ready yet)

Notable bug fixes:
* Fixed `type-at-pos` for `B` in `import type { A as B } from 'M';`
* Fixed `find-refs`, `autocomplete`, `get-def`, etc for `$Facebookism$Idx`

Misc:
* Bunch of improvements to the libdefs. Thanks everyone for the PRs!
* Handful of improvements to the type normalizer, which improves the `type-at-pos` output
* Tweaked error messages for `+` operator to match other binary operators

### 0.75.0

Likely to cause new Flow errors:
* Added return type to RegExp.prototype.match() (thanks @peter-leonov!)
* Fixed callable property lookup rules to no longer walk the prototype chain
* Made it an error to import a type via destructuring on a `require` call (use `import type` instead)
* Made the type of `Array.of` stricter (thanks @wchargin!)

New Features:
* Added typing rules to support `{| |}` as a subtype of `{ +p?: T }`
* Added support for optional catch bindings (thanks @cpojer!)
* Improved performance of enum equality checks
* Added support for `[[call]]` syntax in objects and interfaces
* Many improvements to `flow find-refs` command
* Many improvements to LSP support

Notable bug fixes:
* Fixed crash when refining a non-value

Misc:
* Updated node.js libdefs (thanks @hcz!)
* Fixed some flakiness in the LSP tests
* Added libdefs for ECDH class (thanks @eleith!)
* Added `--check` flag to `flow ast` command
* Added `filter` property to `CanvasRenderingContext2D` libdef
* Added `timingSafeEqual` method to node crypto module (thanks @n-johnson!)

Parser:
* Added `Decorator` node, matching estree

### 0.74.0

Likely to cause new Flow errors:

* Flow now performs exactness checks that were previously skipped in some cases (e.g when `$ObjMap` is used on an exact type).

New Features:

* Significant progress towards supporting the LSP (Language Server Protocol) natively.
* Allow custom module resolvers (experimental, only works with the haste resolver system).
* Flow lints for unnecessary optional chaining.

Notable bug fixes:

* Allow autocomplete to work with optional chaining.
* Only report status as GCing when actually GCing.

Misc:

* Improve the args used when launching child processes to give more information about the process when you run `ps`.
* Add a `max_literal_length` option to `.flowconfig` which replaces a magic constant.
* Multi-hop (indirect) `find-refs` now supports class instances.
* Revamp `find-refs` on ES6 named exports to improve reliability.
* Allow internal slot properties to be optional.
* Improve shutdown of the server monitor process.
* Try to gracefully stop server process when monitor exits.
* Print server status updates on new lines when Flow is not called from a tty.
* Add additional properties to the `react-dom/server` libdef.
* Add 'as' property to the HTMLLinkElement libdef.
* Add a libdef for the Asynchronous Clipboard API.
* Update libdef for `Clients.matchAll` to return an `Array` rather than an `Iterator`.

Parser:
* Rename InterfaceType to InterfaceTypeAnnotation.

### 0.73.0

Likely to cause new Flow errors:

New Features:
* Inline interface types (`var foo: interface { x: string } = bar;`). Will be particularly useful in the future when we revamp object types

Notable bug fixes:
* `flow find-refs` threw an exception when used with an unchecked file. Now it treats the file as if it were checked, like other single-file commands do
* `flow type-at-pos` now returns the class name for `declare class` declarations.

Misc:
* Added `.mjs` to the list of extensions that Flow reads by default
* Perf improvements to calculating the dependency graph during recheck. Should help recheck perf on large repositories.
* `flow find-refs --multi-hop` now parallelizes the work and is much faster
* Support using `flow find-refs` and `flow get-def` with destructuring patterns as a starting location
* Support using `flow find-refs` with `default` (as in `export default ...`) as a starting location
* Bunch of small fixes where `flow find-refs` would miss certain locations
* Tweaked the location returned for `flow get-def` when used for default and named imports
* Lots of libdef updates. Thanks for the PRs!

Parser:
* Inline interface type support

### 0.72.0

Likely to cause new Flow errors:
* We've made explicit the order in which imports are merged into a module during typechecking.
  This fixes an edge case in which lazy modes and non-lazy modes would report different errors.
  This may change the order in which code is typechecked, and therefore may expose errors that Flow
  previously missed.
* Treat `$Exact<empty>` as `empty`. Before, `({p:0}:$Exact<empty>)` was not an error due to
  missing ground subtyping and decomposition cases. It is now an error.
* The `$Either<T>`, `$All<T>` and `$Type<T>` types are not supported any more.

New Features:
* Find-refs now includes references to all types related to an object literal through subtyping,
  and a `multi-hop` mode was added that determines when object types are related through subtyping
  and links them.
* Work towards the new object model:
  - Ensure fields overwrite methods in interface definitions (since properties there are flat).
  - Store proto fields in a separate map than the one used for own properties in classes.
  - Declare `Function.prototype.bind` as a proto field.
* New/call can now be passed explicit type arguments for polymorphic instantiation. Currently this is
  supported by the Flow parser. Babylon support does not exist, but it is planned.
* Made `*` a deprecated type, under the `deprecated-type` strict flag.
* Added support for hover, completion and error reporting (publishDiagnostics) to flow LSP.
* Implemented nullish coalescing as per the [TC39 proposal](https://github.com/tc39/proposal-nullish-coalescing).
* Added a debug flag `--expand-json-output` to print an extended JSON output for `type-at-pos`.
* Updates in typings:
  - Added the definition for `onclose` to the `IDBDatabase` interface in `lib/indexeddb.js`
    ([reference](https://www.w3.org/TR/IndexedDB/#database-interface)).
  - Added `onmessageerror` to `Worker` interface and fixed type of `MessagePort.onmessage`
    in `lib/bom.js`.
  - Added a `swap64()` to the `Buffer` type and a `Buffer` property in `lib/node.js`.
  - Added `Intl` objects for the built-in JS language API (ECMA-402 - Internationalization API).
  - Added tuple types to WebGL `uniform**v` setters in `lib/dom.js`.

Notable bug fixes:
* LSP: Fixed races in reporting exit status over persistent connection and in test.
* Fixed error reporting when accessing statics and simplified error localization.

Misc:
* Added documentation for `Object` type, "Flow for Atom IDE" in Atom plugins, Flow Strict
  and the `nonstrict-import` lint rule.
* LSP supports file edits. Each client now stores the content of files that are opened.
* Added LSP test checking that contents of open files persist after restart.
* Removed Travis from CIs.
* Type normalizer: added option to flag cases where the inferred type parameter is shadowed
  by another parameter with the same name, and fixed support for recursive polymorphic types.
* Removed dependency on ocp-build (windows now uses `ocamlbuild`).
* Introduced a [union-find/disjoint-set data structure](https://en.wikipedia.org/wiki/Disjoint-set_data_structure)
  used in find-refs to maintain sets of related types.
* Fixed return types for `WebGLRenderingContext#is*` methods to all return booleans.
* Rearranged contents of `src/server` directory.
* Refactored find-refs by splitting variable and property handling in separate files,
  and breaking down functions based on their purpose (e.g. local vs global).
* Made `$Subtype` and `$Supertype` "unclear" types when not in a library context.
* `type-at-pos` now prints types at the client-side.
* Minimum OCaml version is increased to 4.05.0.
* Avoid redundant substitution by caching the result of substituting a polymorphic
  definition with a list of type args.

Parser:
* Added support for nullish coalescing `??` (as above).
* Simplified object type parsing. Dangling `+` or `static` inside an object type
  (e.g. `{+}`) are now disallowed.
* Added support for a `proto` modifier in declare class, to specify that the property is
  a prototype property instead of a class one.
* Internal slot properties in object types (e.g. `[[new]]`).
* Explicit type arguments in new and call expressions, e.g., `f<T>(x)`.
* Allow reserved words as optional chain property names.

### 0.71.0

Likely to cause new Flow errors:
* The result of the unary minus (negation) operator would sometimes be incorrectly generalized to
  `number` rather than a literal type. This has been fixed.
* Further restrictions on `module` and `exports`. The disallowed patterns include computed property
  accesses as well as clobbering or aliasing `module` and/or `exports`.
  - These restrictions are only enabled if `experimental.well_formed_exports=true` is specified in
    `.flowconfig`.

New Features:
* `Fragment` has been added to the default exports of the `React` libdef.
* Invoking `find-refs` on a property key in a type declaration will now also yield that key in
  object literals.
* Files can now be marked `@flow strict-local`. This mode is the same as `@flow strict`, except it
  does not require dependencies to also be strict. `@flow strict` is still the recommended mode, but
  `@flow strict-local` allows enabling strict checking for files before all dependencies can be made
  strict. Once all the dependencies are strict, a `@flow strict-local` file can be upgraded to a
  `@flow strict` file. A `@flow strict` file cannot depend on a `@flow strict-local` file.
* Type support for the Stage 1
  [Optional Chaining proposal](https://github.com/tc39/proposal-optional-chaining). To use this
  feature, set `esproposal.optional_chaining=enable` in your `.flowconfig`.
  - **Note**: We currently disallow calls whose callees are member expressions when the call or member
    is part of an optional chain. This restriction will be lifted in the future when we reconcile the
    optional chaining implementation with our implementation of method calls.

Notable bug fixes:
* The type normalizer now correctly tracks the scope of type parameters instead of assuming they
  share the scope of the type definition.
* Test output files are cleared before running tests. This prevents old errors from being printed on
  subsequent failing runs when those runs do not produce errors.
* `estree_translator` properly handles all cases of the `DeclareClass` AST node.
* The `suggest` command has been fixed to work with the new type normalizer.
* When evaluating spreads inside array literals, we determine the element type early, preventing
  exponential complexity and infinite loops.
* Object spread in JSX props now preserves exactness.

Misc:
* Various improvements to `find-refs`.
* Optimizations for polymorphic subtyping.
* Adds CircleCI for continuous integration and migrates the [flow.org](flow.org) build from Travis to Circle.
* New tests for LSP support (disabled by default).
* Certain exceptions will now print backtraces in addition to the exception string.
* Improvements to spreading in array literals.
* Support for Flow coverage reports using [codecov.io](codecov.io).

Parser:
* The AST has been updated to use snake_case for record fields rather than camelCase. Some field
  names have also been updated.
* Support has been added for the
  [Numeric Separators proposal](https://github.com/tc39/proposal-numeric-separator), currently
  Stage 3.
* The AST representation for [Optional Chaining](https://github.com/tc39/proposal-optional-chaining)
  has been updated to use new `OptionalMember` and `OptionalCall` nodes instead of the existing
  `Member` and `Call` nodes, and parentheses now correctly limit the scope of short-circuiting.
  This reflects the current [Babel](https://github.com/babel/babel/issues/7256) implementation.

### 0.70.0

Likely to cause new Flow errors:

* Existing `Promise` libdefs for the cases where `null` or `void` is passed as callbacks to `then`
  and `catch` caused us to miss type errors. These libdefs have now been made stricter.
* Spreading an object of exact type used to result in an unsealed object type, which allowed adding
  new properties not mentioned in the original type. Now, the result has an exact object type, which
  bans adding new properties.
* New missing annotation errors might be reported. These requirements were missed before because of
  implementation bugs. We now use the polarity-sensitive type visitor to walk exported types, which
  reports errors in the expected places. There are several exceptions, which can be exported without
  annotations: e.g., object and array literals; initialized class properties (instance & static);
  and `this` parameters.
* Interactions of `typeof` with speculative typechecking would lead to missed errors, which will now
  be reported as expected.
* Various new restrictions now ban abuses of `module` and `exports`.

API changes:

* The output format with `--json-version 2` includes a breaking change, affecting the "context"
  property in every location. Previously it was just the first line of the context, which made
  printing multi-line errors tricky. Now it includes up to 5 lines of context.
* New version of ocaml-sourcemaps. The API changed slightly, but most of the changes are adding
  support for reading/composing sourcemaps, which we aren't using.

New Features:

* Updated React libdefs with the new `createContext` and `createRef` APIs introduced in [React
 16.3](https://reactjs.org/blog/2018/03/29/react-v-16-3.html).
* Classes can now be spread (copying over their static properties).

Notable bug fixes:

* Lazy_mode_utils.focus_and_check wasn't filtering out files properly when processing updates.
* The monitor would keep restarting segfaulting servers.
* Lint errors would interfere with speculative typechecking (with union / intersection types)
* Switch to using nonblocking file descriptors with Lwt to address Flow hangs
* Fixed a crash in the no-color error printer caused by degenerate locations.
* Improved refinement of truthy/falsy intersections.

Misc:

* Various improvements to `type-at-pos` and friends, including converting normalized types back to
  AST nodes for printing.
* Various improvements to `find-refs`.
* Various CI improvements as part of the move to Circle.
* Various debugging utils.
* Array types are now printed as `Array<T>` instead of `T[]`.

Parser:

* Exposed `implements` and `mixins` fields of the `DeclareClass` AST node.
* Added `tokens` option to JS API: `flow.parse(..., { tokens: true })` will now return the token
  stream, like `flow ast --tokens` does.
* Added options support to the parser's C interface. This change lets you pass a map of options to
  the parser via the C++ API, and from JS via flow-parser-bin. You could already do this from the
  js_of_ocaml parser, so now their APIs match.

### 0.69.0

Notable bug fixes:
* Fixed a couple bugs in pretty printing errors which could cause `flow status` to throw an exception

Misc:
* Lots of internal refactoring/changes this release, so not too much to report!
* A bunch of libdef improvements. Thanks everyone for the PRs!

### 0.68.0

Likely to cause new Flow errors:

Previously, Flow would allow you to write `if (foo.unknownProp) { ... }`.

Now Flow disallows testing unknown properties in conditionals. If `foo` is a
union type like ` { x: string } | { y: number }`, `x` and `y` are known
properties and `z` would be an unknown property

New Features:
* Improve union optimizations
* Add type declarations for new React 16.3 lifecycle methods

Notable bug fixes:
* Fix get-def for declare var
* Fix type-at-pos for opaque types

Misc:
* Remove special-casing of `declare var exports`, long deprecated

### 0.67.1

Restore accidentally-deleted Object.setPrototypeOf library definition.

### 0.67.0

Likely to cause new Flow errors:
* [Replace](https://github.com/meta/flow/commit/8e8f9ffc7f82b7fcb1e9fc01860845905767ac9a) some `any`-typed API definitions with actual types. This may cause errors in code that uses those APIs.

New Features:
* `find-refs` now has support for object type properties.

Notable bug fixes:
* `find-refs` can now find identifiers used as JSX component classes (e.g. `<Foo/>`).
* Fix nontermination that could occur when printing types (e.g. for `type-at-pos`).

Misc:
* Fix `type-at-pos` on method calls that have been affected by type refinements.
* Add `--profile` flag to `flow force-recheck`.
* Fix `--retry-if-init false`.
* Improve `type-at-pos` location for opaque type declarations.
* Add a message to the `flow status` output when the server is in lazy mode.
* Include filename in flow ast output.
* Add typings for `ReactDOM.hydrate()`.
* Make `process.umask`'s argument optional.
* Some miscellaneous improvements to code quality.

Parser:
* [Optional chaining](https://github.com/tc39/proposal-optional-chaining) parser support. This feature does not yet have type system support and should not be used.

### 0.66.0

#### New Features:
* Error message redesign to help you better debug typing issues in your programs.
* Upgrade global `find-refs` from experimental to beta.
* Improve global `find-refs` performance by 2x-20x depending on the workload.
* Support for CommonJS imports/exports with global `find-refs`.
* Find usages of overridden instance methods with `find-refs`.
* Improvements to type reporting services like `type-at-pos` and `coverage` thanks to a type "normalizer" rewrite. Notably, the normalizer and client services like `type-at-pos` now:
  * Make more aggressive use of type aliases, leading to more compact results,
  * Correctly distinguish between class types and their instance counterparts,
  * Report abstract type parameters themselves instead of their bounds, and
  * Precisely report recursive types.

#### Likely to cause new Flow errors:
* Unresolved type variables internally unified with `any` no longer completely resolve to `any`. This may uncover new bugs in your programs where some inferred types silently resolved to `any`.
* Fix type system "stall" for spreads of null and undefined. This meant programs like `({...null}: {p: number});` incorrectly passed Flow. If you were spreading a maybe-object type then Flow may catch some new bugs in your programs.
* Because of the new error messages some suppression comments (configured with `suppress_comment`) will become unused since error locations moved.

#### Misc:
* **3.5x performance improvement** for some Facebook projects that makes heavy use of complicated unions of string literals such as GraphQL enums generated by Relay. (Performance improvements will scale with your usage of large string literal unions.)
* Improve the `nonstrict-import` lint's error message.
* Fix bug in `React.cloneElement` for stateless functional components.
* Add `--max-warnings` flag which allows Flow to exit with a non-zero exit code if warnings are present.
* Add `--file-watcher` flag to ignore file system events.
* Expose `url.format`'s `urlObj` argument type.

### 0.65.0

Likely to cause new Flow errors:

#### New Features:
* Improved inference of `const` bindings (`let`s and `var`s that are never reassigned), and introduced similar inference for function params. This allows Flow to be less pessimistic about refinements, especially inside closures. For example:

  ```js
  function f(val) {
    if (val == null) return () => 42;
    return () => val; // OK, since val cannot be null
  }
  ```

#### Notable bug fixes:
* Fixed regression in recheck performance

#### Misc:
* `implements` now works on `declare class` in addition to `class`
* `declare module` library definitions are now considered `@flow strict`
* Fixed non-`@flow strict` import rule when in a cycle
* Fixed incorrect spreading call properties and indexer properties from interface types
* Fixed `type-at-pos` and `coverage` commands when they encounter `Object.prototype` and `Function.prototype`
* Fixed a crash when hit with more than 1024 concurrent connections, by returning an error instead
* Fixed the --timeout flag to work properly now that it connects instantly to the server monitor
* Added an exit code (5) for when the server goes unused for a week and shuts itself down
* Added a `merge_timeout` .flowconfig option to mirror the `--merge-timeout` CLI flag added in 0.64

#### Parser:
* Fixed location of types containing trailing parens
* Added `implements` to `DeclareClass` nodes


### 0.64.0

Likely to cause new Flow errors:

* `event` is no longer recognized as a pre-declared global

Notable bug fixes:

* An optimization intended to reduce redundant work during rechecks never fired after recent changes.
* The implementation of React.ElementConfig involved a subtyping rule going the wrong way.

Performance improvements:

* Typing info for files were retained in memory even after those files were deleted. This memory is
  now reclaimed.
* Dependency calculations that occur during initialization and every recheck are now faster.

Misc:

* Adds --merge-timeout server flag
* Drops non-strict type args config
* Fixes issue where flow-upgrade would not check .jsx files
* Includes various fixes to docs

Parser:

* Includes thin bindings to allow interfacing with the parser from C++. A prebuilt libflowparser.a is available for Mac and Linux on the GitHub release.
* [flow-parser-bin](https://www.npmjs.com/package/flow-parser-bin) is an experimental node.js extension that provides access to the native (OCaml) parser from JS. [flow-parser](https://www.npmjs.com/package/flow-parser) will be updated to delegate to flow-parser-bin if your platform supports it, and fall back on the slower compiled-to-JS implementation otherwise.
* Trailing commas after a rest element in object patterns is now disallowed following the spec.

### 0.63.1

Bug fix:
* Forgot to cherry-pick a diff to fix `flow init`, which was generated an invalid `.flowconfig`

### 0.63.0

Likely to cause new Flow errors:
* Strict mode now treats function parameters as `const`
* Declaring the exported type of a CommonJS module via `declare var exports: T` is deprecated in favor of `declare module.exports: T`

Notable bug fixes:
* If a single file (or cycle of files) takes more than 100s to merge (which indicates something is horribly wrong), Flow will emit an error, stop merging that file/cycle, and continue merging the rest of the files.
* Better handling of internal Flow errors during merge. A file (or cycle of files) with an internal error during merge will emit the error and set the type of the exports to `any`. This avoids cascading internal errors.

Misc:
* `flow get-def` (used by IDEs for jump to definition) now behaves differently for variables introduced by an `import` or `require()`. Previously, it would show where the variable was created, in the import. Now it looks through the import and shows where the variable was exported.
* The first steps in a large error message revamp are included in this version of Flow. Larger changes will follow in later versions.
* Some small perf improvements
* A bunch of libdef improvements. Thanks everyone for the PRs!

Parser:
* Enforce that the rest property in object destructuring must be last.
* Fixed a bug which banned methods named `static` in object types

### 0.62.0

Likely to cause new Flow errors:

* Removed support for static properties on interfaces. Static properties were never well supported, and in most cases their types were not actually checked.
* Fixed the polarity checker, which was not erroring in many places where it should have, like class statics.
* Removed `unsafe.enable_getters_and_setters` option. Getters and setters are now enabled by default. Use the `unsafe-getters-setters` lint rule to disable.

New Features:

* Improved error message locations and context around type errors in many cases
* Added `flow cycle` subcommand, which prints a `.dot` describing the cycle for a given file.

Notable bug fixes:

* Fixed bug where `[lib]` files outside the Flow root would not be watched for changes
* Fixed a few race conditions, which would mask errors depending on the order of imports in a file.
* Fixed refinements on opaque types with declared bounds.

Misc:

* Added `nonstrict-import` lint rule, which requires that strict files can only depend on other strict files.
* Added `unsafe-getters-setters` lint rule, which replaces the `unsafe.enable_getters_and_setters` flowconfig option.

Parser:

* Added missing `method` property to object type property AST node.
* Added support for properties named `static` for declare class and interfaces.
* Changed to separate `Variance` AST node for +/- annotations, matching Babylon.
* Improved performance by reducing GC pressure.

### 0.61.0

#### New Features:

* Added an `[untyped]` section to `.flowconfig`, which treats all matching files as untyped, as if they did not have `@flow`. This differs from `[ignore]` in that `[ignore]` hides matching files from the module resolver /and/ the type system. This allows you to ignore types from third-party modules that are poorly-typed, typed for a different version of Flow, typed with a `.flowconfig` with different options, etc.
* Experimental: Global find-references for class members.

#### Notable bug fixes:

* Fixed an issue that caused IDE commands (autocomplete, get-def, etc.) to crash in certain cases.
* Fixed an issue that caused IDE commands to return no results when the `this` type is nullable.
* Fixed several bugs in lazy mode to avoid crashes and missed errors.

#### Misc:

* Issue an error at every location where a particular non-existent module is imported, instead of just the first one in each file.
* For the `get-imports` command, show every location where a module is imported instead of just one per file.
* Quite a few libdef improvements.

#### Parser:

* Disallow literals as prop keys in assignment destructuring.

### 0.60.1

Fixed a bug introduced in 0.60.0 in which Flow would not start if its temp directory did not exist.

### 0.60.0

#### Likely to cause new Flow errors:

These changes do not introduce new errors, but error suppressions may need to be moved to the new, more accurate locations.

* Improved positioning of errors involving object types with incompatible indexers.
* Improved positioning of errors involving the arguments of overloaded function types.

#### New Features:

* Introduced a "server monitor" process that acts as an intermediary between client commands and the Flow server.

  Previously, the server was only able to service one client request at a time and multiple connections would block until the server is free, preventing the server from telling the waiting clients why it's busy. Now, the monitor can accept many requests and respond more intelligently.

  It is also able to detect when the server exits (e.g. when a `package.json` changes) and restart it transparently.
* `flow find-refs` can now find local references to class properties.
* New linters:
  * `unclear-type` warns about uses of `any`, `Object` and `Function`, since they unsafely circumvent the type system.
  * `untyped-import` warns when `import`ing or `require`ing a module that does not have `@flow`.

#### Notable bug fixes:

* Made the union created by `$Values` on a frozen object maintain singleton literals. That makes this pattern work:

  ```js
  const Enum = Object.freeze({
    X: 'x',
    Y: 'y',
  });
  type EnumT = $Values<typeof Enum>
  ('a': EnumT); // now errors. previously, EnumT was any string
  ```

* Fixed `Object.keys` and `$Keys` on a dictionary of string literal keys, such that the result is now an array of the string literals instead of a generic string:

  ```js
  function f(dict: {['hi']: mixed}) {
    (Object.keys(dict): Array<'hi'>);
    (Object.keys(dict): Array<'bye'>); // error
  }
  ```

* Simplified the types mentioned in some error messages, like from "type application of polymorphic type: class type: Foo" to just "Foo"
* Fixed `get-def` locations on class and object properties
* Fixed `get-def` on refined object properties like the second `prop` in `if (foo.prop) foo.prop()`, which previously returned no results
* Fixed non-termination bugs for predicates on classes, unions and polymorphic `instanceof`
* Made recursion limit errors unsuppressable. Please report any such errors, they are always Flow bugs!

#### Misc:

* Fixed compilation under ocaml 4.06
* Added dependency on `lwt` from opam
* Fixed error behavior of `flow coverage` in `--quiet` and `--json` modes
* Made `--json` consistently imply `--quiet` in all commands, notably `status`
* Fixed an issue where a new server may end up writing to the `.log.old` file
* Various additions to lib definitions, thanks for your contributions!

#### Parser:

* Implemented [JSX spread children](https://github.com/facebook/jsx/pull/59) syntax
* Made missing commas in export specifiers a parse error
* Made `import type *` a parse error


### 0.59.0

#### New Features:

- Adds a `$ReadOnly<T>` utility type which makes all properties on objects
  read-only. `$ReadOnly<{ a: number, b: string }>` behaves as if you wrote
  `{ +a: number, +b: string }`. [Read more about property variance on
  our blog.][]

```js
type O = { a: number, b: string };

function fn(o: $ReadOnly<O>) {
  o.a = 42; // Error!
}
```

- Allow read-only property initialization in constructors. Covariant properties
  are still read-only everywhere else in the class.

```js
class X {
  +p: number;
  constructor(p: number) {
    this.p = p;
  }
}
```

- **25% performance improvement** on full check time for large projects.
  (Results may vary.)
- Enables lints in Try Flow. [Enable lints with configuration comments like
  `/* flowlint sketchy-null:error */`][try-flow-lint-example].

[Read more about property variance on our blog.]: https://flow.org/blog/2016/10/04/Property-Variance/
[try-flow-lint-example]: https://flow.org/try/#0PQKgBAZgNg9g7lAlgOwC5gM4GsCmqDGAFgJ4C0yArlFAFw4BO9M9YIwAUAG4CGLAHgEYaYAPwAjGDChgAvJG5QMOANztEEMAApBASjABvMKEy4CJVsDABfdkA

#### Notable bug fixes:

- Improves positioning for error messages involving exact objects. Notably, this
  bug caused error messages against `React.Element` to incorrectly point at
  library definitions instead of user code.

#### Misc:

- Experimental implementation of find-all-references.
- Enforces that exported class statics are annotated.
- Improves typings for Node.js HTTP server `listen()` function.
- Removes redundant information from some React error messages.

#### Parser:

- Adds parser support for JSX fragments.
- Various correctness changes to which identifiers error on reserved value
  and type names.
- `declare class` with multiple extends is now a parse error.

### 0.58.0

Likely to cause new Flow errors:

* Detect match failures with disjoint unions. Example:
  ```
  type Foo = { type: 'A', ... } | { type: 'B', ... }`
  function match(foo: Foo) {
    switch (foo.type) {
      case 'C': ... // dead branch, now error! (was allowed previously)
      ...
    }
  }
  ```

New Features:

* Support for user-defined "strict" mode. Using `@flow strict` instead of
  `@flow` in the header will now additionally fire lint rules that can be listed
  in the `[strict]` section of
  `.flowconfig`. [Here](https://flow.org/en/docs/linting/rule-reference) are the
  current set of supported lint rules (more coming, contributions welcome!).

Error reporting:

* Error location improvements for a bunch of common operations

Parser:

* Tighten the AST around import statements, and rewrite their parsing logic
* Improvements to locations of import statements, export statements, and declare statements

Perf:

* Optimize a hot path in lazy mode, speeding up IDE commands
* Optimize calculation of dependents, speeding up rechecks
* Fix exponential blowup for large enums

Reliability:

* Fix deadlock issues with `flow ide`
* Gracefully handle crashes of the file watcher

Misc:

* Don't list ignored files with `flow ls`
* Remove flaky support for $Tainted types
* Remove flaky support for `--raw` type printing
* Various additions to lib definitions, thanks for your contributions!

### 0.57.3

Notable bug fixes:
* Fixed a race condition which was causing the Flow server to hang during merge
* Rebuilt the Windows binary

### 0.57.2

Misc:
* Reverted the change which stopped call properties from flowing to object type dictionaries

### 0.57.1

Notable bug fixes:
* Fixed a crash when a file goes from parsable to unparsable
* Fixed a server crash when a client dies before receiving a response

Misc:
* Added logging to show which components take a long time to merge

### 0.57.0

Likely to cause new Flow errors:
* We've [manually enumerated](https://github.com/meta/flow/blob/1c9d9486c07dd51853107621c686b7b30b7134f8/lib/react-dom.js#L172) all the JSX intrinsics, so Flow might notice if you're misusing an intrinsic.
* `$Diff`'s implementation was rewritten. It should behave mostly the same, but will error on `$Diff<ObjA, {x: string}>` if `ObjA` doesn't have a property `x`

New Features:
* Flow will now only check the files in `node_modules/` which are direct or transitive dependencies of the non-`node_modules` code.

Notable bug fixes:
* A handful of fixes for `flow ide` on Windows
* Fixed a few bugs that would cause `flow server` to crash when `flow ide` exits
* Fixed a regression in v0.56.0 which caused Flow to crash on `import type *` syntax
* Fixed `$ObjMap`'s behavior on optional properties
* Various fixes for type destructors (like `$PropertyType`, `$Diff`, and `$ObjMap`)
* Object type indexers no longer include call properties. So `{[string]: boolean}` is no longer a subtype of `{(number): string}`.
* Fixed a bug where circular type imports would miss errors involving union types in rare cases.

Misc:
* Updated Flow headers and license file from BSD3 to MIT
* `flow server` will now write to a log file in addition to stderr
* `flow ls` will now list the `.flowconfig`
* `flow ls` will now list lib files, even if `--all` is not set
* `flow ls --imaginary` will list nonexistent files. It's useful for speculating whether or not Flow would care about a file if it existed.
* Added `flow force-recheck --focus` which tells a lazy server to start caring about certain files
* Various small error message fixes
* Lots of libdef updates! Thanks everyone for the contributions!

### 0.56.0

New Features:

* Added a `$Rest<A,B>` type, which models the semantics of object rest
* Added support for `null` prototypes, a la Object.create(null)
* Added support `__proto__` property in object literals and object type annotations

Notable bug fixes:

* Improved support for React higher-order components, e.g. Relay fragment containers
* Improved performance of `flow focus-check` for multiple files
* Fixed type-at-post support for $ReadOnlyArray types
* Fixed many cases where error messages were reported far away from the root cause.
* Fixed find-refs for named exports

Misc:

* Added experimental lazy mode for IDEs
* Added `<VERSION>` token for `suppress_comment` option in `.flowconfig`
* Removed support for $Abstract utility type
* Removed support for `flow typecheck-contents --graphml`


### 0.55.0

Likely to cause new Flow errors:

* Fixed a bug that caused unsoundness with `$ObjMap`.

New Features:

* Flow is now capable of servicing some requests while it is rechecking. This should improve the IDE experience on large codebases.
* Added $Call utility type.
* Added $Compose and $ComposeReverse utility types.
* Added support for spreading `mixed` into an object type.

Notable bug fixes:

* Improve results from the find-refs command.
* Allow null and undefined as React.createElement() config (fixes #4658).

Misc:

* Miscellaneous code cleanup.
* Located error messages related to functions at only the signature, rather than the entire range of the function body.
* Improved error messages when `this` types are incompatible.
* Properly check subtype relationships between callable objects.
* Fixed a bug that caused `mixed` not to be properly printed from `type-at-pos`.
* Improved error messages regarding incompatible Array type parameters.
* Preserve some inference information across module boundaries.
* Support assignments to shorthand method properties in object literals.

Typedefs:

* Added captureStream() to HTMLCanvasElement and HTMLMediaElement.
* Added HTMLOptGroupElement return type for document.createElement().
* Added Recoverable and context to the `repl` module.
* Fixed return type for AudioContext.createMediaStreamDestination().

Parser:

* Various fixes to improve test262 compliance:
  * Correctly disallowed various illegal constructs in destructuring patterns.
  * Allow destructuring in `catch`.
  * Added \u2028 and \u2029 to the list of line terminators.
  * Allow unicode escape codes in identifiers.
* Improved parse errors when using private properties outside of classes.
* Disallowed reserved words as function param names in types (e.g. `(switch: number) => void`).

### 0.54.1

Notable bug fixes:
 * Fixed an issue where the server becomes temporarily unresponsive after a recheck and the client consumes all its retries.

### 0.54.0

Likely to cause new Flow errors:
* Extending a polymorphic class must now explicitly specify the parent class's type args. That is, `class A<T> {}; class B extends A {}` is now an error, but `class C extends A<string> {}` or `class D<T> extends A<T> {}` is ok.
* Improved accuracy of type checking calls of built-in methods (e.g. Object, Array, Promise)

Notable Changes:
* Implemented private class fields, part of the [Class Fields](https://github.com/tc39/proposal-class-fields) proposal.
* Implemented "phantom" types (e.g. `type T<Phantom> = any; type X = T<string>; type Y = T<number>`, where `X` and `Y` are incompatible even though `T` doesn't use `Phantom`)
* Unused suppression errors are now warnings instead
* Improved errors involving polymorphic types, so that they now point to the source of the conflict, rather than the nested type args that are incompatible. This was a major source of errors in files other than where the problem was.
* Improved errors involving structural subtyping, so that they now reference the objects that are incompatible in addition to the incompatible properties
* Improved errors when an inexact object type flows into an exact type
* Made rest parameters in object destructuring patterns sealed, and exact if possible
* Improved definitions for some node `fs` functions
* Improved performance by removing unnecessary caching

Misc:
* Improved accuracy of type checking of React children in React.createClass
* Fixed a bug related to instantiating a polymorphic type (e.g. `type t = T<U>`) with empty type args (e.g. `var x: T<>`)
* Fixed polarity checking for property maps
* Fixed a bug where polymorphic types were incorrectly checked for equality
* Improved React definitions
* Added a FLOW_TEMP_DIR env, equivalent to passing --temp-dir
* Added a minimal libdef that defines the few things Flow can't run without, even when using no_flowlibs=true

Parser:
* Added `flow ast --strict` to parse in strict mode without "use strict"
* Added support for the RegExp dotAll ('s' flag) [proposal](https://github.com/tc39/proposal-regexp-dotall-flag)
* Added support for the private class fields [proposal](https://github.com/tc39/proposal-class-fields)
* Added support for destructuring defaults in assignments (e.g. given `({ x = 1 } = {})`, `x` is 1)
* Fixed issues related to `let`, `yield`, `await`, `async`, `super` and other reserved words
* Fixed issues related to declarations in statement positions
* Fixed issues related to destructuring patterns
* Fixed issues related to IdentifierReferences, like shorthand object notation
* Fixed issues related to class parsing, particularly `new.target` and async methods


### 0.53.1

Fixed a bug that sometimes crashed the server during recheck

### 0.53.0

This release includes major changes to Flow's model for React. The following
links contain detailed documentation on the new model.

* [Defining components](https://flow.org/en/docs/react/components/)
* [Event handling](https://flow.org/en/docs/react/events/)
* [ref functions](https://flow.org/en/docs/react/refs/)
* [Typing children](https://flow.org/en/docs/react/children/)
* [Higher-order components](https://flow.org/en/docs/react/hoc/)
* [Utility type reference](https://flow.org/en/docs/react/types/)

Please use the new [flow-upgrade](https://yarnpkg.com/en/package/flow-upgrade)
tool to upgrade your codebase to take advantage of these changes!

Likely to cause new Flow errors:

* We are modifying how you define React class components. The React.Component
  class will now take two type arguments, Props and State (as opposed to the
  three type arguments including DefaultProps that React.Component took
  before). When your component has no state, you only need to pass in a single
  type argument. If your component has default props then add a static
  defaultProps property.

* Flow used to not type React function refs at all, but now that we are typing
  refs code that used to just work may now have errors. One such error which can
  often be overlooked is that the instance React gives you in a function ref may
  sometimes be null.

* Flow used to completely ignore the type of React children in many
  places. Intrinsic elements did not check the type of their children (like
  `<div>`), the type specified by components for React children would be ignored
  when you created React elements, and the React.Children API was typed as
  any.

* In the past when typing children many developers would use an array type
  (Array<T>) often with the React element type
  (Array<React.Element<any>>). However, using arrays is problematic because
  React children are not always an array. To fix this, now use the new
  React.ChildrenArray<T> type.

New Features:

* Modeling advanced React patterns, like higher-order components, is difficult
  today because the types you would need are either not provided or
  undocumented. In this release we added a whole suite of utility types which
  are all documented on our website.

Notable bug fixes:

* Flow used to have a bug where Flow would consider the following code as valid:
```
function MyComponent(props: {foo: number}) {
  // ...
}
<MyComponent foo={undefined} />; // this is now a Flow error
```

* We now allow JSX class components to use exact object types as their props.

* We now allow member expressions when creating JSX components,
  e.g. `<TabBarIOS.Item>`.

* We now allow multiple spreads in a JSX element.

* We have added a type argument to `SyntheticEvents` and correctly typed
  `currentTarget` using that type argument.

Parser:

* We fixed miscellaneous character encoding issues.

Misc:

* This release features a major re-architecture in how Flow typechecks modules
  against their dependencies. Earlier, Flow would do a "local" (per-module)
  typechecking pass followed by a global (cross-module) typechecking pass. Now,
  these passes have been merged. This change vastly improve Flow's memory usage
  on large codebases.

* We found and fixed a couple of subtle bugs in the typechecking engine that
  caused stack overflows in some pathological cases.

* We made various improvements to refinements. We now recognize `var`s that are
  only assigned to once as `const`s, so that we can preserve refinements on them
  through longer stretches of code. Some `typeof` cases have also been fixed.

* We now support focus-checking multiple files. You can use it to debug issues
  easier and faster by telling Flow to focus on files of interest.

* This release also includes lots of improvements to core type
  definitions. Thanks for your contributions!

### 0.52.0

New Features:
* Flowlint - a linter built into Flow that you can configure to complain about things which aren't quite type errors.

Notable bug fixes:
* Flow now enforces polarity on class supers (e.g. Flow will error on `class B<+T> extends A<T> {}` when `A`'s type parameter is not covariant)

### 0.51.1

* Changed linter (experimental, coming soon) to ignore lint errors in node_modules

### 0.51.0

New Features:
* Added support for opaque type aliases
* Added library definitions for the Node.js repl module (thanks @zacharygolba!)

Notable bug fixes:
* Fixed library definitions for the Node.js cluster module (thanks @Corei13!)
* Fixed the return type of EventEmitter#setMaxListeners (thanks @ahutchings!)
* Added missing properties in the Node.js fs module (thanks @ahutchings and @rgbkrk!)
* Fixed the length property on $ReadOnlyArray to be covariant (thanks @popham!)

Misc:
* Improved error handling in our test runner
* Fixed type error in our docs (thanks @stenehall!)
* Fixed broken link in docs (thanks @vasyan!)
* Fixed misleading typo in docs (thanks @joelochlann!)

Parser:
* Fixed end locations of various statement nodes to include terminal rparen
* Added separate DeclareTypeAlias and DeclareInterface AST nodes, matching Babel
* Fixed locations of declared vars, classes, and functions in declare export stmts

### 0.50.0

Likely to cause new Flow errors:

* Fixed a bug that suppressed unrelated errors when a missing annotation error was also suppressed.

New Features:

* Added `$Values` type.

Notable bug fixes:

* Fixed lints appearing in Try Flow.
* Miscellaneous libdef improvements.
* Fixed a couple bugs that could lead to missing push diagnostics when using the persistent connection.
* Made `$ReadOnlyArray` covariant in variance checking.

### 0.49.1

Fixed an issue where `flow init` outputs a `[lints]` section (for experimental linting support, coming soon!) that includes `all=false`, which is already deprecated. Upcoming versions will support `all=off` instead.

### 0.49.0

Notable bug fixes:
* Optimized performance when typechecking classes
* Optimized performance of sentinel property checks involving large enums
* Lots of libdef updates! Thanks everyone for the contributions!

Misc:
* Fixed infinite recursion from array spread
* Added experimental support for sending errors to an IDE over a persistent connection
* Removed unused --libs flag on status, check, and start commands

Parser:
* Fixed parsing scientific notation with decimal but no fractional part
* Added support for parsing opaque types. Type system support coming soon.

### 0.48.0

New Features:

* Experimental support for "lazy" typechecking. On large codebases, the Flow
  server will start up faster when using the --lazy flag with `flow start` or
  `flow server`, by only computing dependency information and not doing any
  typechecking. Instead, as files are touched, they are typechecked
  incrementally along with files that depend on them (and their
  dependencies). Relatedly, running `flow focus-check` on a file will only check
  that file and files that depend on it (and their dependencies).

Notable bug fixes:

* Fixed crash in flow.org/try when using JSX (which was a regression in 0.46)

Misc:
* Libdef updates! Thanks everyone for the contributions!
* Removed the libelf dependency from Flow. This helps a few systems, like the
  standard node docker, that doesn't have libelf available.
* Removed the strip_root .flowconfig option, since it doesn't make sense as a
  project-specific option and complicated IDE integration. Clients (including
  IDEs) can still use the --strip-root CLI flag as needed.
* Improvements to handling of Object.prototype
* Improvements to handling promotion of primitives to objects
* Lots of refactoring to improve stability of commands and the typechecker

### 0.47.0

Likely to cause new Flow errors:
* We are now sealing the object type that is passed to a React component's props. This means Flow will start complaining if you try and access unknown React props.
* Strict function call arity checking is now on for everyone! The `experimental.strict_call_arity` option has been removed. For more, see https://flow.org/blog/2017/05/07/Strict-Function-Call-Arity/


Notable bug fixes:
* There are a bunch of flow commands that will quickly try to check a single file, like autocomplete, get-def, check-contents, etc. We're trying to clean up what happens when these commands are given a file with @flow, @noflow, or no pragma. v0.47.0 contains a change intended to avoid doing extra work for files with @noflow or no pragma at all. v0.48.0 should continue to address this.

Misc:
* Libdef updates! Thanks everyone for the contributions!
* In a bunch of situations we're now propagating `any` types and continuing typechecking rather than stopping altogether.
* Using a function as an object will now give that object a call property

Parser:
* Optimized parsing of huge list of nested ternary expressions
* The JS version of the parser now throws proper JS errors in the rare case that an OCaml exception is uncaught

### 0.46.0

Likely to cause new Flow errors:
* We updated the type for the `React.Component` constructor, which means Flow
can now infer the prop types based on the `super(props)` call in a react
component constructor. This means components that weren't already declaring
their  prop types may start getting errors if there are any issues with their
prop types.
* We fixed the type of `Promise.prototype.catch`, which means code that uses
`.catch` but wasn't properly handling the exceptional behavior might now have
Flow errors

New Features:
* We're updating how get-def (the jump to definition feature) works. It will now
jump straight to the definition, even if it's in another file, rather than to the most recent assignment.
* Starting in v0.47.0 we're going to complain when you call a function with more arguments than it expects. You can try it out in v0.46.0 with the `.flowconfig` option `experimental.strict_call_arity=true`. For more [check out this blog post](https://flow.org/blog/2017/05/07/Strict-Function-Call-Arity/)

Notable bug fixes:
* Fixed a exponential blowup that could happen when using functions as callable objects
* Fixed a bug where calling a function with a rest param wouldn't flow `undefined` to unfulfilled parameters.
* Fixed a bug where `declare class` classes would be given a default constructor even if they extended another class

Misc:
* Flow is faster! There are a bunch of perf wins in this release!
* Lots of updates to library definitions! Thanks to all our contributors!
* When using the Flow CLI, multiline errors will now show multiple lines of context
* Replaced `--no-suppressions` with `--include-suppressed` which is a little bit more well-behaved.
* Added `--pretty` flag to `flow check` for pretty-printed JSON

Parser:
* Import expressions that appear in a statement list are now correctly parsed as import expressions (thanks @[mikaelbr](https://github.com/mikaelbr)!)
* Fixed the location of `declare function` declarations with predicates
* Fixed the location of `yield` expressions with semicolons
* Fixed the location of `declare module` declarations
* Fixed a bug where array pattern with defaults like `[a=1, b] = c` was parsed like `[a=(1, b)] = c`.

### 0.45.0

New Features:
* Flow now has proper unicode support in its parser!
* Support for `import` expressions (Thanks [deecewan](https://github.com/deecewan)!)
* Introducing `export type * from ...`, an analogue of `export * from ...`

Notable bug fixes:
* Fixed incremental checking bug when lib files are added or removed

Misc:
* libdef and docs updates. Thanks for the PRs! Keep 'em coming!

Parser:
* Unicode is finally supported! We've moved our lexer from ocamllex to sedlex, which supports unicode!
* Fixed location of unary operator argument when surrounded by parens
* Fixed a bug around using tagged templates expressions as call or member expressions

### v0.44.1

Notable bug fixes:
* Fixed the definition for `HTMLBRElement`, which extended itself :(

### v0.44.0

New Features:
* Another big perf win!

Notable bug fixes:
* Internally, Flow wasn't always propagating the any type, which could suppress certain errors. We've fixed this, and Flow now notices some errors it was missing before

Misc:
* Lots of libdef and docs PRs merged! Thanks to everyone for the help!
* Minimum OCaml version is now 4.02
* Partial support for object type spreads with generics

### v0.43.1

Features:
* Big perf improvement!

### v0.43.0

Parser:
* Fixed parser to disallow variance sigil for when using object spread.
* Cleaned the flow-parser npm package of cruft. Thanks [@zertosh](http://github.com/zertosh)!

Notable bug fixes:
* Some internal refactoring helps Flow find some errors that it was previously missing.

Misc:
* A ton of libdefs updates and documentation fixes. Thanks for all the PRs!

### v0.42.0

Likely to cause new Flow errors:

New Features:
* Object type spread (`type TypeB = { ...TypeA };`). You can now spread one object type into another! It is designed to mimic the behavior of spreading an object value in an object initializer, where only own properties are copied over.
* Experimental new `flow ide` command, through which an IDE can establish a permanent connection with Flow. The IDE can send commands through this connection and Flow can push changes to the IDE.

Notable bug fixes:
* Fixed a bug that would cause Flow's recheck to never terminate

Parser:
* Support for object type spreads.

Misc:
* Lots of updates to the builtin flow libs! Many thanks to all our contributors!
* Object types with dictionaries are now considered sealed (you cannot add new properties to them). This is a minor change, since you can still write to the dictionary.
* We've relaxed the restriction that `React.PropTypes.oneOf` must take an array of literals. You can now put non-literal values in the array.

### v0.41.0

Notable bug fixes:
* Lots of improvements to typing React components (which may uncover previously missed errors). This release primarily focused on these tweaks to React typing
* Previously Flow did not track much about mixins from `React.createClass` components (including mixed in proptypes). This has been fix and may uncover new proptypes errors in createClass components
* The internal React libdef for `findDOMNode` has been updated to include `null` in its return-type now (since this function can, indeed, return `null`)

Misc:
* `flow get-importers` has been removed. It was rarely used and added an incredible amount of complexity to the codebase
* Lots of updates to builtin flow libs

### v0.40.0

Notable bug fixes:
* Fixed an edge case when moving .flow files around in a haste project
* Fixed bug which allowed you to add extra properties to exact objects
* Fixed pretty printing of methods in type-at-pos and gen-flow-files
* Fixed the @jsx pragma support's handling of trimming whitespace and filtering empty children. It now matches the behavior of the babel transform
* Fixed some merge_strict_job exceptions
* Fixed bug where Flow would ask for annotations in the object passed to `React.createClass()` even when they were not needed.
* Fix so now you can use `this` and `super` in class method parameter defaults.
* Optimization around union types

Parser:
* Fixed a bug where line comments in certain positions were parsed as block comments

Misc:
* Lots of updates to the builtin flow libs! Many thanks to all our contributors!
* Some tweaks to error messages to make them easier to understand.
* We are NOT removing weak mode in this release. See the discussion on [#3316](https://github.com/meta/flow/issues/3316)

### v0.39.0

Likely to cause new Flow errors:
* Previous usage of `ReactElement<*>` as a return type for render functions in React may now cause new errors now that a type propagation bug has been fixed. The general solution for this is to remove the return type for functions that return a React element (return types can be inferred by Flow, and Flow has never had a good way to express the type of a *specific* React element)
* Several significant improvements to the locations Flow points to in error messages about objects and property accesses may move errors that were previously suppressed with a [suppression comment](https://flowtype.org/docs/advanced-configuration.html#options) to a new location

New Features:
* You can now pass `--no-suppressions` to `flow check` in order to see what errors would show up in your codebase if you deleted all the error suppressions
* New advanced debugging feature: You can now use the special `$Flow$DebugPrint` type to make Flow print a JSON representation of a type. Example usage: `declare var flowDebug: $Flow$DebugPrint; flowDebug(someVariable);`
* Flow now supports constant-folding of tuple types. In general this means that Flow can now better understand rest elements and array-spreads when tuples are involved.
* Flow now supports spreading types that are iterable (other than just Arrays)

Notable bug fixes:
* Fixed several issues where Flow might not terminate while typechecking complex code patterns
* Fixed an issue where a return type on a class constructor wouldn't properly define the type of objects the class generates if the type was too complex
* Fixed an issue where [Flow wasn't properly invalidating refinements after a `yield` expression](https://github.com/meta/flow/issues/2778)
* Fixed an issue where `Function.prototype.bind()` wasn't working properly on variables typed as "callable objects" in Flow
* Fixed [some issues where `implements` could cause a `"Did not expect BoundT"` error](https://github.com/meta/flow/issues/3243)
* Fixed an issue where [Flow would previously give a cryptic error if the `TERM` environment variable wasn't set](https://github.com/meta/flow/pull/3305) (Thanks @SamirTalwar!)

Parser:
* Fixed an issue where [the parser previously couldn't handle numbers greater than 2^32-1](https://github.com/meta/flow/issues/3238)
* The parser now errors on invalid variance `+` tokens in object types (like it already did for object literals)
* Fixed an issue where [the parser was incorrectly parsing JS directives (like `"use strict"`)](https://github.com/meta/flow/issues/3234)


Misc:
* A few improvements to error messages relating to React PropTypes
* Various core libdef updates

### v0.38.0
Likely to cause new Flow errors:
* There are some error-position improvements in this release. This means that if you use `// $FlowFixMe` (or any kind of `suppress_comment`), there's a chance the error may have moved out from under the suppression comment. You may see some unused error suppressions and some previously suppressed errors.
* The behavior for tuples has changed to strictly enforce arity. This might cause errors depending on how you use tuples.

New Features:
* `implements` - classes can now implement one or more interfaces and Flow will verify that the class properly implements the interfaces.
* Local find references command: `flow find-refs file line column` - if there is a variable `x` at `file:line:column`, then this command returns the location of `x`'s definition and the location of all reads and writes to `x`.
* New shorthand for importing types in the same declaration that imports values: `import {someValue, type someType, typeof someOtherValue} from 'foo'`
* Autocomplete now works with exact object types (thanks [@vkurchatkin](https://github.com/vkurchatkin)!)
* Tuple types have been revamped. They now strictly enforce arity ([docs](https://flowtype.org/docs/arrays.html#tuples)). This will enable a bunch of future features around spreads and rest parameters. Stay tuned!
* `$ReadOnlyArray<T>` is the common supertype of arrays and tuples. It's basically an array without stuff like `push()`, `pop()`, etc
* A lot of tweaks to error messages to make them easier to understand.
* You can declare getters/setters in `declare class`, `interface`'s, object types, etc. They're still not safely checked, though.
* Set `emoji=true` in the `.flowconfig` options and `flow status` will put emoji in the connection status messages (thanks [@zertosh](https://github.com/zertosh)!)

Notable bug fixes:
* You can now use template strings with `require()`, (e.g. ```require(`foo`)```)
* `Object.keys` no longer returns methods since they are not enumerable
* We've relaxed sentinel checks to allow you to check unknown properties. This is consistent with other conditionals
* Flow recognizes type mismatches between the LHS and RHS of a sentinel check as always failing
* Flow recognizes null and undefined checks as sentinel checks too
* Flow used to assume that an array literal with a single element of type `T` had the type `Array<T>`. Now it's `Array<*>`, which lets you add elements of other types to the array.
* Fixed `$Shape` to ignore "shadow properties", which are properties that don't really exist, but which have been referenced somewhere.
* A few recheck bug fixes. The optimization from 0.37.0 wasn't properly rechecking when property variance changed, when certain locations changed, or when certain files are renamed.
* Fixed mistake where computed getters and setters weren't erroring with `unsafe.enable_getters_and_setters=false`

Misc:
* Lots of updates to the builtin flow libs! Many thanks to all our contributors!
* `make` no longer complains if `js_of_ocaml` is not installed
* `flow check --profile` now includes CPU time
* `flow server --profile` and `flow start --profile` now print profiling info to the logs
* Some optimizations for huge dependency cycles.
* `tests/` which run a flow server now save the `.log` file when they fail
* Many tests now run with `--no-flowlib`, which make them much faster

Parser:
* Parsing support for object spread properties
* Fixed `kind` of a static method named `constructor` to be `"method"` rather than `"constructor"`
* Support for new `import type` shorthand
* OCaml AST for getters and setters is a little cleaner now
* We now support defaults in setters
* The bug fix for trailing commas in array lists was accidentally reverted and has been re-committed.

### v0.37.4

Notable bug fixes:
* 1 more server recheck fix... Fourth time's a charm!

### v0.37.3

Notable bug fixes:
* 1 more server recheck fix... Third time's a charm!

### v0.37.2

Notable bug fixes:
* Fix more issues in the server with rechecking changed files

### v0.37.1

Notable bug fixes:
* Fixed an issue in /try where Flow was using an ocaml regex API that couldn't compile to JS
* Fixed an issue where a changed "literal" type in a module signature wouldn't cause the Flow server to recheck
* Fixed an issue where an update of a module in a cycle may not properly recheck all of its dependencies in the Flow server

### v0.37.0

Likely to cause new Flow errors:
* There are some error-position improvements in this release, which means that if you use `// $FlowFixMe` (or any kind of `suppress_comment`), there's a chance the error may have moved out from under the suppression comment to a different location that's more indicative of the error position.

New Features:
* LOTS of built-in libdef updates.
* It's now possible to use `import type`/`import typeof` inside the body of a `declare module` to import types between libdefs.

Notable bug fixes:
* Fixed an issue where dictionary types couldn't flow into a covariant-compatible version of themselves.
* Fixed an issue where previously `any + number` would yield `number`. Now it correctly yields `any`.
* Don't try to read from the filesystem at all when using `flow check-contents` where input is provided via stdin.

Misc:
* The `--old-output-format` CLI flag is now gone.
* Performance optimization that allows us to skip re-checking any recursive dependencies of a changed file when the file's types haven't changed
* Improved error positions on property assignment type errors.
* The `parse()` function in the `flow-parser` NPM module now takes either 1 or 2 arguments. Previously it would require both arguments in order to work.
* Things typed as "callable objects" now inherit from Function.prototype rather than Object.prototype.

### v0.36.0

Likely to cause new Flow errors:
* We've been working on improving error messages, which often involves moving the errors closer to where the error is likely triggered. If you use error suppressing comments, some of your comments now appear unused, since the error moved.

New Features:
* Lots of small improvements to error messages
* `flow ls --all` will output libs and ignored files too
* `flow ls --explain` will explain why Flow cares or doesn't care about a file
* `flow ls dirA/ dirB file.js` will only list files under `dirA/`, files under `dirB/`, and `file.js`

Notable bug fixes:
* Calling a method through computed properties (`obj['method']()`) is now supported
* The client no longer consumes retries and waits patiently when the server is busy garbage collecting
* Errors in lib files with `--strip-root` set now show context again

Misc:
* Currently `flow coverage` and `flow check-contents` default to treating all input as if it has the `@flow` pragma. We will change this in a future version. To make this transition easier, both commands now have the flags `--all` and `--respect-pragma`, where `--all` is the current behavior and `--respect-pragma` is the future behavior.
* Better error messages for unsupported destructuring.
* Builtin libdef improvements
* Fixed a TDZ issue which would allow `let x = x;`

Parser:
* Fixed a bug where `[...rest, 123]` was incorrectly parsed

### v0.35.0

Likely to cause new Flow errors:
* Flow now knows that calling `addEventListener()` with `'click' and 'dblclick'` will pass a `MouseEvent` to the listener. This means you might need to update `foo.addEventListener('click', (e) => bar())` to `foo.addEventListener('click', (e: MouseEvent) => bar())`.

New Features:
* Better error messages in a bunch of situations
* flowtype.org/try now has an AST tab that shows the parsed AST of the example

Notable bug fixes:
* Bindings that come from imports are now treated as const
* Found and fixed a few situations where Flow was emitting redundant errors
* Some if statement refinements were sticking around after the if statement in some situations when they should not have.

Misc:
* Various libdef fixes and improvements
* Various docs fixes and improvements
* If `foo` has the typed `mixed`, we now allow `foo.bar` if you check that `foo` is not `null` or `undefined`.

Parser:
* Better error message if you try to make a class property optional (currently unsupported)
* Dropped support for `let` statements, which never made it into the spec (thanks [@andreypopp](https://github.com/andreypopp))

### v0.34.0

Likely to cause new Flow errors:
* Dictionary types (i.e. `{[key: string]: ValueType}`) were previously covariant which proved to be a significant source of unsoundness. Dictionary types are now invariant by default in order to fall into consistency with other collection types. It is possible to opt in to explicit covariance with new syntax: `{+[key: string]: ValueType}`, but note that this is now *enforced* covariance -- which means the dictionary can no longer be written into (only read from). For mutable collections, consider using `Map`/`Set`/etc. Please see this [blog post](https://flowtype.org/blog/2016/10/04/Property-Variance.html) for more information on variance.
* Object property types are now invariant by default. New syntax allows explicit opt-in to enforced covariance: `type T = {+covariantProp: string}`. Please see this [blog post](https://flowtype.org/blog/2016/10/04/Property-Variance.html) for more information on variance.
* Object *method* types are now covariant by default. So this: `type T = {covariantMethod(): string}` is the same as `type T = {+covariantMethod: () => string}`. Please see this [blog post](https://flowtype.org/blog/2016/10/04/Property-Variance.html) for more information on variance.

New features:
* New `empty` type annotation. This is the ["bottom type"](https://en.wikipedia.org/wiki/Bottom_type) which is the type that has no possible values. This is mostly useful for asserting impossible types right now (see [the commit description](https://github.com/meta/flow/commit/c603505583993aa953904005f91c350f4b65d6bd) for more details).
* All server commands now have a `--quiet` flag to suppress server-status information that would otherwise be printed to stderr.
* It's now possible to specify an "@jsx" pragma to override the implicit default of `React.createElement`. See [the commit](https://github.com/meta/flow/commit/d2930e135f9c34a89a226c2ca36eb4bcab7b59db) message for more details.
* [async iteration](https://github.com/tc39/proposal-async-iteration) is now Stage 3, so Flow now supports async generators and `for-await-of` statements.

Notable bug fixes:
* Calling `get-def` and `autocomplete` on specifiers in import statements now works properly and links in to where the specifier is exported in the other file.
* `Generator.prototype.return` now returns a possibly-unfinished `IteratorResult` object rather than a definitely-done iterator result. See #2589 for more details.
* Fixed an issue with inferring the proper return-type of iterators coming from a generator with multiple return-types. See #2475 for more details.
* Fixed an issue where a non-polymorphic class-instance used as a CommonJS export wasn't omitting underscore-prefixed members when the `munge_underscores` config option is set to `true`.
* Fixed an issue where Flow would previously not consider non-@flow files when re-calculating type dependencies on a change to the filesystem. This caused sporadic issues where Flow might think that a module is missing that actually is not! This is now fixed.

Misc:
* Significant memory usage optimizations by normalizing common aspects of the many "reason" structures stored in memory to use ocaml variants.
* Significant memory usage optimization by compressing the contents of the shared heap used by the persistent server.
* Parser now allows for duplicate properties and accessors in objects per the latest ES spec.
* Flow parser is now tested against esprima3 tests
* `yield` expressions no longer evaluate to an optional type. This is unsound, but the inconvenience was so prevalent that we decided to relax the issue for now. See #2080 for more details.
* Various core libdef updates.

Parser breaking changes:
* Updated the parser to use the latest `ExportNamedDeclaration` and `ExportDefaultDeclaration` nodes from ESTree rather than the outdated `ExportDeclaration` node.
* `export default class {}` now correctly emits a `ClassDeclaration` rather than `ClassExpression` per [this estree issue](https://github.com/estree/estree/issues/98).
* Updated `ExportSpecifier` property names from `id` -> `local` and `name` -> `exported` per the latest ESTree spec.
* Update `ExportBatchSpecifier` to a `ExportNamespaceSpecifier` node per the latest ESTree spec.
* Renamed `SpreadElementPattern` -> `RestElement` per the latest ESTree spec.
* Node-properties of `ObjectPattern` are now named `Property` and `RestProperty` per the latest ESTree spec.
* Use a `Super` node now instead of an `Identifier` node per the latest ESTree spec.
* `{ImportNamedSpecifier` and `ImportDefaultSpecifier` nodes now use proper `local` and `remote` property names, per the latest ESTree spec.
* The `mixed` type annotation is now represented with a special `MixedTypeAnnotation` node (same as Babylon has been for a while).
* The `NullTypeAnnotation` annotation node is now called `NullLiteralTypeAnnotation` in order to match Babylon.

### v0.33.0

Likely to cause new Flow errors:
* Its now an error to add `mixed` to `number`

New features:
* `suppress_comment` now defaults to matching `// $FlowFixMe` if there are no suppress_comments listed in a .flowconfig
* The Flow server no longer restarts itself when a `package.json` file is changed
* The Flow server no longer restarts itself if a libdef file is touched, but not actually changed
* Added support for using Iterables with `Promise.all()` (thanks @vkurchatkin!)

Notable bug fixes:
* Fixed an issue where some predicates could cause Flow to crash
* Fixed an issue where we weren't properly looking things up on `Function.prototype` and `Object.prototype`
* Fixed an issue where Flow could crash when extracting coverage on empty types
* Fixed an issue where long paths that are ignored could give a bunch of warnings on Windows
* Fixed an issue where `flow get-def` wouldn't hop to the location of a type coming through an `import type`
* Fixed an issue with dictionary types where using an `any`-typed variable as a computed-property lookup results in the wrong property-value type
* Fixed some issues where Flow wouldn't allow definition of properties or methods called "static" on classes
* Fixed an issue where Flow wouldn't permit `throw`s at the toplevel of a module
* Fixed an issue where adding a file to `[libs]` with an extension not listed in `module.file_exts`, it would previously be silently ignored
* Fixed an issue where `import * as` on a `declare module.exports: any;` libdef would not result in a module with every possible named export
* Fixed a parsing issue where `"JSX attributes must only be assigned a non-empty expression"` syntax errors sometimes point to the wrong line
* Fixed parsing of setter methods with destructured parameters
* Fixed a parsing issue where it wasn't possible to use `get` or `set` in object short notation
* Fixed a parsing issue where we previously weren't allowing strings in async method names: `x = { async 123() { await y; } }`
* Fixed an issue where the parser previously wouldn't recognize the `u` regex flag

Misc:
* Various built-in libdef improvements
* Various significant shared memory optimizations
* When a package.json key is read by Flow during module resolution, don't wrap it in quotes
* Added parser (but not yet typechecker) support for `new.target`
* Added `--pretty` flag to all commands that have a `--json` flag
* Flow now prints an exception instead of segfaulting if there is a heap overflow
* Only print JSON for `flow coverage` when `--json` is passed (thanks @aackerman!)

Parser breaking changes:
* Removed 'lexical' property from `SwitchStatement`
* Removed `guardedHandlers` from `TryStatement`
* Use `AssignmentPattern` for function param defaults to match ESTree
* Use `RestElement` for function rest params to match ESTree
* Fixed the location info for `ExpressionStatement`
* Fixed the location info for `CallExpression` and `MemberExpression`

### v0.32.1

Notable bug fixes:
* If Flow runs out of heap space, it now throws an exception instead of segfaulting.

### v0.32.0

Likely to cause new Flow errors:
* If you check that an object `obj` has a property `foo` that Flow doesn't know about, Flow will now refine `obj.foo` to the type `mixed`. If you then try to use `obj.foo` in an unsafe way (like as a function), you might start seeing errors mentioning `` property `foo` of unknown type ``. The fix is to either fix the type of `obj` to include an optional property `foo`, or to rewrite code like `obj.foo && obj.foo(x)` to `typeof obj.foo === "function" && obj.foo(x)`
* We've fixed the type of `Array.prototype.find` to reflect the fact that it can return `undefined`
* We've tightened up the checking of tuple types


New Features:
* New syntax for exact object types: use `{|` and `|}` instead of `{` and `}`. Where `{x: string}` contains at least the property `x`, `{| x: string |}` contains ONLY the property `x`.
* Flow now watches for `.css`, `.jpg`, `.png`, `.gif`, `.eot`, `.svg`, `.ttf`, `.woff`, `.woff2`, `.mp4` and `.webm` files (this list is not configurable at the moment). We call them resource files. If you require a resource file, you get a `string` (except for `.css` files, for which you get an `Object`). You should still be able to use `module.name_mapper` to map a resource file to a mock, if you prefer.
* We're starting work on `flow gen-flow-files`, which consumes Flow code and outputs `.flow` files containing only the types exported. It's alpha-level and we're still iterating on it, so use at your own peril!

Notable bug fixes:
* Fixed a bug where, if a module has a CommonJS export of type `any`, then the types it exported were also given type `any`
* v0.31.0 contained a regression where arrays and promises required more annotations than they should. This is fixed.
* Fixed use of tagged unions with intersections and generics.
* Jump to definition for overridden methods was jumping to the wrong method. This is now fixed.
* `flow coverage` had a non-termination bug. This is now fixed.

Misc:
* Lots of improvements to the builtin libdefs! Thanks for all the pull requests!

### v0.31.0

Likely to cause new Flow errors:
- Fixed an issue where an `any` type could actually prevent errors from showing up in places that actually should surface errors
- `Array.isArray()` now refines arrays to `Array<mixed>` instead of `Array<any>`

New Features:
- When the `munge_underscores` option is set, Flow no longer requires annotations on exported classes with munged methods. (thanks [@pvolok](https://github.com/pvolok)!)
- Added a new "magic" type called `$PropertyType<T, 'x'>`. This utility extracts the type of the property 'x' off of the type T.
- It is now possible to use leading-`|` and `&` on any type annotation, not just in type aliases

Notable bug fixes:
- Significant perf improvements on checking disjoint unions
- Fixed an issue where `flow start` would sometimes hang on Windows
- Fixed an issue where `flow get-def` on a react element would always point to the internal react libdef (rather than the component that defines the element)
- Error messages for builtin types are now more descriptive in more scenarios
- Fixed the order in which destructured function params were processed. This previously caused issues with variable references within the destructuring
- Error messages for union type errors now point to more helpful locations in some cases
- Fixed a long-standing confusing error message blaming property accesses on the "global object" (when the global object isn't actually involved at all)
- Improved error message when trying to import a named export from a module that only has a default export
- Fixed an issue where `Promise.resolve(undefined)` would not always error as it should
- Fixed an issue where async functions that return void do not properly enforce their return type
- Fixed an issue where aliased object types may not error when an invalid property is accessed on them
- Fix `flow coverage` when reporting coverage on empty files
- Printed types in some circumstances (autocomplete, type-at-pos, etc) are now capped on size to prevent overflows in tools that consume them. When the size overflows the cap, `...` will be printed as an overflow placeholder
- Various built-in libdef updates

### v0.30.0

Likely to cause new Flow errors:
- Fixed `React.PureComponent`'s definition, so previously missed errors are now reported
- The definition of `console` in the build-in libdef has been filled out and is no longer `any`.

New Features:
- From now on we're going to start publishing Windows builds with each release. Please report any issues you have!
- Forward references in type annotations: you can now reference a class in a type annotation before the class is declared
- T is now covariant in `Class<T>`. So if class `B` extends class `A`, then `Class<B>` is now a subtype of `Class<A>`
- Flow now lets you destructure objects with computed keys.
- `flow check-contents --respect-pragma` - `check-contents` checks whatever you give it, regardless of `@flow` or `@noflow`. This option changes the behavior to respect the pragma.
- `flow check-contents` will now report parsing errors too (thanks [@nmote](https://github.com/nmote)!)

Notable bug fixes:
- Fixed `--trace` behavior when we unify types
- Fixed situation where the client could spin-wait for the server
- Fixed non-termination triggered by certain calls to `Function.prototype.{apply,call}`
- Fixed issue where "Duplicate module provider" errors would stick around even after being fixed.
- (Windows) Fixed how the flow client tails the flow server's logs
- (Windows) Fixed the log rotating
- (Windows) Fixed issues where Flow would report Process Handles instead of Process IDs, leading to bad messages and a non-functional `flow stop`
- (Windows) Fixed build outside of a git or hg repository
- (Windows) Better error messages when you have paths that are too long

Misc:
- `undefined` is now just a global variable declared in the libdefs
- Various built-in libdef improvements
- Nifty PR from [@nmn](https://github.com/nmn) which teaches flow that `nullableArr.filter(Boolean)` is non-nullable

### v0.29.0

New features:
- Tagged unions of interfaces should now work in the same way that tagged unions of type aliases work.

Notable bug fixes:
- Building Flow outside of a git or hg repo should work now
- If you declare an overloaded function with one overload that handles a `string` argument and one overload that handles a `number` overload, then passing in a union `string | number` should now work.
- Annotations for destructured patterns now work like non-destructured patterns. Types no longer flow through these annotations, rather they are checked against the annotations and then the annotations are used instead.
- Allow `import {type} from "Foo"`
- We had a bug where Flow wouldn't complain about duplicate haste modules. This is now fixed
- Fix for edge case hit when using `experimental.strict_type_args=false` and type parameter upper bounds.

Misc:
- Various built-in libdef improvements
- Some error location fixes
- Lots of Windows support work. Should build and mostly work, but not fully stable yet.
- Some performance wins
- `flow check --json --profile` now includes profiling info in the JSON
  response

### v0.28.0

Likely to cause new Flow errors:
- [Significant fix](https://github.com/meta/flow/commit/2df7671e7bda770b95e6b1eaede96d7a8ab1f2ac) to matching members of union types. This fix very likely surfaces typechecking issues that were missed before. [Check out the blog post for more details](https://flowtype.org/blog/2016/07/01/New-Unions-Intersections.html)
- Variables in template literals are now subject to the same rules as variables being concatenated with a string using `+`. This may surface template strings with variables that may not safely coerce to a string.
- Type arguments are now required to be specified for type annotations that have type arguments. Previously they were implicitly filled in as `any` -- but now this must be explicit. This behavior is toggleable via `experimental.strict_type_args=false`, but this config option will be removed in a near-term future release. [There is also a codemod to help you automatically update old code.](https://github.com/flowtype/flow-codemod/blob/master/transforms/strict-type-args/README.md)

New Features:
- Support for `declare export` within `declare module` bodies. When `declare export` is used, the module being declared is inferred as an ES module. This enables the ability to export both named and default exports using `declare module`.

Notable bug fixes:
- Fixed a bug on OSX where Flow might crash in rare circumstances
- Fixed an issue where automatic semicolon insertion for class properties [wasn't behaving properly in some circumstances](https://github.com/meta/flow/issues/1682).
- It is no longer required to annotate a property on an exported class if an initializer value is present. Instead, the initializer value itself can be directly annotated. This makes annotation of class properties that are functions much less cumbersome.
- Fixed a race-condition where Flow may not properly error when it encounters two modules with the same name (using the "haste" module system)

Misc:
- Flow now looks for `@flow` in all comments at the top of the file (rather than just the first). It's also possible to configure this using a new `max_header_tokens` .flowconfig option.
- Various built-in libdef improvements
- Performance improvements for the `flow coverage` command

### v0.27.0

Notable bug fixes:
- Fixed a filesystem race condition where Flow would note that a directory exists just before it is deleted, then try to read the directory
- Fixed some issues with disjoint unions that have complex definitions
- Fixed an issue where functions that come after a return aren't analyzed (even though they are hoisted)
- A few updates to the Node.js library definitions
- Fixed a race condition when adding specific properties to dictionary types
- Various performance improvements
- `--strip-root` is now applied to the output of `--verbose`
- Fixed an issue where duplicate method declarations weren't understood correctly

### v0.26.0
([@gabelevi](https://github.com/gabelevi) mistakenly listed a few v0.26.0 changes as being in v0.25.0. The Changelog has been updated to reflect reality. Sorry!)

Likely to cause new Flow errors:
- Flow now understands stateless functional React components, which may reveal many errors if you use them heavily.

New Features:
- Support for stateless functional React components!
- Support for the exponentiation operator (`**`)
- New `flow ls` command to list the files that Flow can see.

Notable bug fixes:
- Fixed parsing of `/x/* 5 */y/` - that's no comment!
- Fixed parsing of newlines or comments at the end of a template literal.
- Fixed an incremental bug where Flow didn't notice new JSON files in certain situations
- Fixed a parsing service crash when a file appears and disappears suddenly.

Misc:
- Restored a bunch of deprecated/experimental/browser-specific APIs to the builtin flowlib and made them optional
- Fixed up parser tests and further integrated them into CI.
- Lots of refactoring!

### v0.25.0
Likely to cause new Flow errors:
- [@marudor](https://github.com/marudor) made a tremendous effort to clean up the builtin flowlib definitions, adding missing things, fixing annotations, and removing non-standard and deprecated features. If you're relying on these things, then you may have new errors.
- In the past, generic types could leave off the type arguments. Flow is moving towards making these required. Applying type arguments to a polymorphic type (e.g. `Map<string, number>`) is like calling a function. If writing `my_function` was the same thing as writing `my_function()`, it would be really difficult to pass functions as values. Similarly, by making type arguments required, it frees us up to do more with our polymorphic types. If you have a polymorphic type with default types, like `type Foo<T = number>`, you can now write `Foo<>` to apply 0 type arguments to the polymorphic type `Foo`.

  To ease migration, v0.25.0 still allows you to omit type arguments by default. v0.27.0 will start enforcing that type arguments are always supplied. To enable this behavior in v0.25.0, you can add `experimental.strict_type_args=true` to the `.flowconfig`.

New Features:
- `declare module.exports: type;` <-- this is the new syntax to declare the CommonJS export type. Previously, you had to write `declare var exports: type;`. This can be used in `.flow` files and in `declare module` declarations.

Misc:
- Now Flow builds on OCaml 4.03.0
- Fixed up the parser tests (thanks for the help [@marudor](https://github.com/marudor)!) and have started running those in CI
- A ton of refactoring and clean up

### v0.24.2

- Fixed a bug where Flow might run out of memory in a repository with a lot of non-flow files

### v0.24.1

- Fixed a bug where `autocomplete` can show internal variable names for files that use destructuring

### v0.24.0

New features:
- Many common errors now have more contextual error messages. [Check out the test file changes](https://github.com/meta/flow/commit/7b8c3aed5d852ad9b8290076508658168d0d5fde#diff-839cf9ce7d26ef86255e179b8a539fc7) to see what this looks like!
- If a `<PROJECT_ROOT>/flow-typed/` directory exists, Flow will now assume it is a [libs] directory by default to help reduce the amount of out-of-the-box configuration that is necessary. (Thanks @splodingsocks!)
- Support for specifying a default on a type parameter declaration. For example: `type Iterator<Yield, Return=void, Next=void> = ...`.
  - NOTE: [The pull request to add syntax support for this is pending](https://github.com/babel/babylon/pull/25), so it may be necessary to wait on that to ship before using this feature

Notable bug fixes:
- Fixed the `flow coverage` command.
- Fixed crashes when running `type-at-pos` and `dump-types` over destructuring.
- Fixed a refinement bug when evaluating a method call on a refined variable in some cases.
- Fixed an issue where some system commands could throw if Flow attempts to read a directory that it doesn't have permission to read.
- Fixed a bug where the inferred return type of some functions that *might* return `void` could end up as *exactly* `void` (rather than a union of the possible return types).
- Fixed a bug where the server would crash if a `.json` file is deleted after a server was already running.
- Type applications that don't have the right number of type parameters (i.e. `Map<number>`) now emit a more clear error.
- Lots of dom/bom improvements sent in from contributors! (big thanks to @marudor for lots of these!)

Misc:
- Better locations for logical operator errors
- Better error message sorting:
  - Sorts errors in `.json` files by name coalesced with `.js` files
  - Sorts all internal errors before parse errors before type/inference errors
  - Sorts lib file errors before source file errors
- Perf improvements for some comparisons of polymorphic types

### v0.23.1

- Fixed parsing of JSON files with duplicate object keys

### v0.23.0

Likely to cause new Flow errors:
- When you refine a `mixed` variable with `typeof myVar === 'object'`, we used to refine the type of `myVar` to `null | Object`. Now it is refined to `null | {[key: string]: mixed}`. This means `myVar.prop` has the type `mixed` rather than `any`.
- Removed non-standard Promise methods. Previous versions of Flow specified the type of Promise.prototype.done and Promise.cast, which are not standard or implemented in browsers. If you rely on a polyfill that does provide these methods, you can redeclare the Promise class in your project's local libs folder. Note that you need to copy the entire class declaration from lib/core.js in order to add methods and properties. It's not currently possible to extend the builtin declarations, but it is possible to redefine them.

New features:
- Errors involving union or intersection types now include more information about why the various branches failed
- `flow init` now has more command line flags to specify what should be in the created `.flowconfig`
- `flow ast` now can parse JSON files
- Comments are now supported in `.flowconfig` files. Lines starting with `#` or `;` are ignored
- In the `[ignore]` section of a `.flowconfig` you can now ignore a relative path with `<PROJECT_ROOT>/path/to/ignored`
- Most `flow` commands have an `--ignore-version` flag to skip the version check specified in the `.flowconfig`.
- Added a `module.use_strict` option to the `.flowconfig`. When it is true, Flow will treat every file as if it has the `"use strict";` directive.
- Using strict equality, you can now refine the `number` type into number literal types. (e.g. after `x === 0` Flow now knows that x has the type `0`)
- Flow no longer requires return type annotations for exported functions if Flow can fully infer the type.
- `flow force-recheck FILE1 FILE2` command tells the flow server that `FILE1` and `FILE2` have changed and that it should recheck. This is intended for tooling that might be racing the file system notifications.
- Flow is smarter about what `!x` evaluates to for various types of `x`.
- `<Foo />` is now allowed when `Foo` is a `string`. If `$JSXIntrinsics` is defined, `Foo` must be a subtype of `$Keys<$JSXIntrinsics>`
- Support for class decorators in addition to property decorators (also gated behind the `esproposal.decorators` config option). Thanks @marudor!

Notable bug fixes:
- Disallow `(obj: SomeClass)` except for when `obj instanceof SomeClass`
- Fixed setting temp dir via the `.flowconfig`
- Added missing `all` flag to the `.flowconfig`
- Fixed a bug when destructuring a non-literal object type using a pattern with defaults
- Fixed the `--strip-root` flag for displaying errors
- Classes can now have properties named `async`
- Fixed refinements like `if (foo[0]) { ... }`, which should work like `if (foo["prop"]) { ... }` does. That is, Flow should remember that `foo[0]` exists and is truthy.
- Fixed parsing docblocks with CRLF line endings
- Fixed autocomplete within if statements

Misc:
- Added more info and structure to JSON output, without removing or existing fields
- Loads of improvements to the builtin libraries
- Bunch of perf improvements
- Clarified some errors messages and error locations
- `flow start --json` will start a server and output a JSON blob with info about the new server
- Slowly improving tracking side effects in switch statements
- Some improvements to pretty printing errors
- `Object.values()` and `Object.entries` return `Array<mixed>` and `Array<[string, mixed]>` respectively, since Flow currently is never sure that it knows about every property in an object.

### v0.22.1

- Patch release to fix some JSON parsing issues that went out in v0.22.0

### v0.22.0

Likely to cause new Flow errors:
- Overhaul of Flow's understanding of React APIs. Some of these updates remove old/deprecated React APIs. Check out the [new React docs](http://flowtype.org/docs/react.html) for an overview of how things work.

New features:
- Flow now gives precedence to library definitions over non-@flow implementation files. This means that it should no longer be necessary to specify a `node_modules` dependency in the `[ignore]` section of your `.flowconfig` if you have a library definition defined for that dependency.
- Significant improvements to `Promise.all`: We now preserve the type of each item in the array passed to `Promise.all()` so that it may be propagated through to the resulting `.then()` handler.
- We no longer try to parse files that are not marked with an `@flow` pragma. We anticipate this will improve performance for projects with large, non-Flow node_modules directories.
- Classes with static members are now subtype-compatible with structural object types
- It is now possible to specify a leading `|` or `&` for type aliases of long unions/intersections. This is useful, as one example, for disjoint unions with a large number of members (where each member sits on a new line):

```javascript
type MyDisjointUnion =
  | {type: 'TypeOne', ...}
  | {type: 'TypeTwo', ...}
  | {type: 'TypeThree', ...}
  ...
;
```

Bug fixes:
- Fixed an issue where an intersection of two object types did not always properly combine to match objects with members on both sides (https://github.com/meta/flow/issues/1327)
- Fixed an issue where an object of some intersection type could not be used with the spread operator (https://github.com/meta/flow/issues/1329)
- Fixed an issue where refinement-testing on an object of an intersection type wouldn't always work (https://github.com/meta/flow/issues/1366)
- Fixed an issue where an intersection of function types with a common return type should type check against function types with union of param types
- Fixed an issue where refining `obj['abc']` didn't behave quite the same as refining `obj.abc`
- Fixed an issue where usage of `flow get-def` on the left-hand side of a `require()` wouldn't hop through the `require()` and to the actual location of the definition
- Fixed an issue where Flow would not give a clear error when trying to use `import type` to get a non-type export
- Fixed an issue `flow dump-types --json` was not as robust as it could be against outputting valid JSON in the event of certain kinds of errors
- Fixed an issue where Flow would not give a parse error if multiple ES exports with the same name are exported from a single ES module
- `declare class` declarations now properly define a built-in `name` property (like real class declarations do)

### v0.21.0

Likely to cause new Flow errors:
- ES6 react classes without state should now `extends React.Component<DefaultProps, Props, void>` (previously it was `extends React.Component<DefaultProps, Props, {}>)`
- ES6 react classes with state should declare `state: State;`
- Before, it was possible to test for properties in objects which did not exist (`if (o.noSuchP === foo) { ... }`). These kinds of checks will now cause a type error.

New features:
- Autocomplete for jsx properties
- [Typed JSX intrinsics](https://github.com/meta/flow/commit/e0e44d392d6fa2bff36ea6aee87f965c66ee5b7e). This means you can list which jsx intrinsics exist (like `div`, `span`, etc) and specify which properties they have.
- Syntax for declaring variance at definition. For example, `interface Generator<+Yield,+Return,-Next> {...}`. Still pending transpiler support though.
- Refining `string` and union types with string equality now properly refines the types.
- Support for `export * as` from @leebyron's [Stage1 proposal](https://github.com/leebyron/ecmascript-more-export-from). Babel support [here](http://babeljs.io/docs/plugins/transform-export-extensions/)

Notable bug fixes:
- Fixed bug with class expressions due to `this` type
- Fixed autocomplete for `this`
- Recognizes exhaustiveness in `switch` statements with `default` case.
- Fixed "Did not expect BoundT" errors
- Fixed infinite loop in certain recursive types
- Fixed an incremental mode issue with deleted files
- Fixed an incorrect refinement when assigning an object to a variable.

Misc:
- Some internal errors now will be made user visible instead of silently failing. They generally mean that Flow has some bug or is making an untrue assumption/assertion. If you see these please report them!
- Improvements to how we report certain types (type application, optional types) via our APIs
- Various sentinel improvements, including boolean sentinels
- Various improvements to the builtin flow libraries (thanks everyone for the pull requests!)

### v0.20.1

Bug fixes:
- Ironed out some issues with the `this` type

Misc:
- find package.json using normal parsing phase

### v0.20.0

New features:
- Initial support for a `this` return type for class methods
- Big improvements on error messages for unions and intersections
- `import typeof * as` now allows for concise access to the type of the ModuleNamespace object
- Flow now understands how to `require()`/`import` a .json file

Bug fixes:
- Fixed an issue where nested unions and intersections might not typecheck as expected
- Fixed issue where `declare type` wouldn't work in a `declare module` that has an `exports` entry
- Fixed an issue where the error formatter could fatal in some rare cases
- Fixed a bug where `Function.prototype.bind` would lose the types of the params on the function it output
- Fixed some variance bugs in the built-in Array.prototype library definitions
- Fixed a bug where using a list that doesn't contain ".js" in `module.file_ext` would cause internal flow libs to be ignored
- Fixed autocomplete to work better with general `Function` types
- Fixed some issues with const refinement
- Fixed various issues with `export * from` (it should work fully now)
- Fixed a bug where Flow might crash when an export has a wildcard typeparam

Misc:
- Some improvements to DOM and Node libdefs
- Various error position relevancy improvements
- Significantly improved general understanding of special functions like `Function.prototype.{bind,call,apply}`
- Improved error messages for `import` statements where the remote exports don't exist (or may be typo'd)
- Improvements to understanding of deferred initialization of `let` variables
- `flow get-def` will now hop through lvalues in variable assignments for more fine-grained "hop-tracing" of a variable back to its definition
- Objects with a `callable` signature can now be passed in to type positions that expect a function with a matching signature
- Significant improvements to efficiency/perf when recalculating types based on a change to a file with an already-running Flow server

### v0.19.0

Likely to cause new Flow errors:
- Flow syntax is now disallowed in non-`@flow` files. Use `@noflow` to work around this
- `import type * as Foo` is now disallowed in favor of `import type Foo`
- `require()` can only take a string literal
- ES6 react classes without defaultProps should now `extends React.Component<void, Props, State>` (previously it was `extends React.Component<{}, Props, State>)`
- ES6 react classes with defaultProps should declare `static defaultProps: DefaultProps;`
- Flow notices errors it missed before in `React.createClass()` react components
- Flow is now stricter about using uninitialized variables
- Stricter checking of `in` keyword

New Features:
- `flow coverage` command
- `null` type annotation
- Support for class field initializers, gated by `.flowconfig` options
- You can now override flowlib definitions with local lib files
- Basic support for computed properties
- Declaration files (.flow files). Long story short, if `foo.js` and `foo.js.flow` exist, Flow will prefer the latter and ignore the former.
- `declare export` - a way to declare the exported types in a non-lib file
- Array rest destructuring assignment support

Notable Bug Fixes:
- Fix "package not found" error in some symlink situations
- Object indexer should not imply callable signature
- Default param values can reference earlier params
- Fixed a case where we weren't substituting type parameters properly
- Fixed a situation where Flow would prefer an unchecked module over a library definition

Misc:
- Add `--root` arg to most client commands
- More repositioning of error locations
- `flow get-def`: Jump to module named in import statement
- Lots of fixes to make flow commands smarter about connecting to the server
- Smarter refinement in a bunch of situations
- freeze imports on all modules, and require() on ES6 modules
- You can now spread classes, like `var {x} = new Foo()`
- Interfaces now can be callable
- If you've refined an object property, that refinement survives access through a destructured refinement
- Better autocomplete results for primitives, objects, functions and unions
- `flow server` will write to log file in addition to stdout/stderr

### v0.18.1

Likely to cause new Flow errors:

- Flow is now stricter (and more consistent) about how `null` works when used as an initialization value for object properties that are mutated later. So `let o = {prop: null}; o.prop = 42;` is now an error and requires that `null` be annotated: `let o = {prop: (null: ?number)};`.
- The return type of RegExp.prototype.match() is now properly annotated to return a nullable.

New Features:

- We now take advantage of the guarantee that `const` variables are read-only when doing refinements. This means that refinements of `const` variables now have much fewer caveats than refinements of `let`s or `var`s. For instance, it's safe to depend on the refined type of a `const` within a local function created in scope of the refinement, even if the function escapes.
- We now track which external variables a function actually modifies, and forget refinements to only those variables when a function gets called.
- New config options: `esproposal.class_static_fields=warn|ignore` and `esproposal.class_instance_fields=warn|ignore`. This allows the new [ES class fields](https://github.com/jeffmo/es-class-static-properties-and-fields) syntax to be ignored.
- New config option: `module.system.node.resolve_dirname`. This allows configuration of the name of the `node_modules` directory (or directories) used by the node module system. This is similar in behavior to webpack's [resolve.moduleDirectories](https://webpack.github.io/docs/configuration.html#resolve-modulesdirectories) config option.
- Added support for a `<PROJECT_ROOT>` token in the template string for the `module.name_mapper` config option. This token will be replaced with the absolute path to the current project root before mapping the module name. For example: `module.name_mapper='^\(.*\)$' -> '<PROJECT_ROOT>/src/\1'` would canonicalize an import from `"foo/bar/baz"` to `"/path/to/root/of/project/src/foo/bar/baz"`.


Notable Bug Fixes:

- Fixed a bug where we were enforcing TDZ on reads to let/const, but not on writes.
- Fixed a bug where any refinement clearing that occurred as a result of a function called in an if-statement was forgotten, meaning that lots of refinements were incorrectly allowed to stand.
- Fixed a bug where generator code couldn't previously call `yield` without an argument.
- Several improvements to generators.
- Various improvements and bug fixes for TDZ enforcement.
- Various other small improvements to refinements.
- Fixed an issue where errors could sometimes spew to stdout rather than stderr.

Misc:

- Removed `flow single` as it is effectively redundant with `flow check --max-workers=1`.
- The "server starting" output now gives more insight into progress.
- `flow --version` is now deprecated in favor of `flow version`.

### v0.17.0

New Features:

- New default error message format shows the code inline (thanks [@frantic](https://github.com/frantic)!)
- Autocomplete will keep trying even if the file fails to parse
- You can configure which file extensions Flow looks for (thanks [@eyyub](https://github.com/eyyub)!)
- Support for negative number literal types.

Notable Bug Fixes:

- Incremental `make` on OSX now works when `lib/` files change
- Fixed some issues around const destructuring
- Fixed some issues around destructuring in for-of and for-in loops
- Some emacs plugin fixes
- Object spreads are now handled in the correct order
- Generator `return()` and `throw()` methods are now supported
- Object types now allow keywords as the object keys (thanks [@samwgoldman](https://github.com/samwgoldman)!)
- importing & exporting `default` using named specifiers is now supported
- `Flow` now understands `this` in class static methods and `this.constructor` in class instance methods (thanks [@popham](https://github.com/popham)!)
- Fixed bug with array spreads
- Understand that all classes have a static `name` property

Misc:

- Improved `flow find-module`
- More error location improvements
- `Object` can now be called as a function to cast things to objects
- We've tried to standardize the error codes with which Flow exits. Some exit codes have changed, but the ones you probably use should be the same. At the moment they're [only documented in the code](https://github.com/meta/flow/blob/b352b4c41283c1cb109ee2e8f6ef604ad4ac381b/src/common/flowExitStatus.ml#L63-L86)
- Flow understands the value of a negated number literal

### v0.16.0

Likely to cause new Flow errors:

- Some module exports that didn't require annotations before may now require annotations

New Features:

- Let/const support! Finally! (Huge props to [@samwgoldman](https://github.com/samwgoldman)...again :] )
- Support for `mixins` on `declare class` library definitions

Notable Bug Fixes:

- Improvements to types inferred from switch cases that fall through to a default cause
- Further improvements on symlink support

Misc:

- *Significant* performance improvements (both for initial start-up time and running re-calculation time)
- Improved `--traces` output

### v0.15.0

Likely to cause new Flow errors:

- Flow now treats class identifiers as being let bound. You cannot refer to a class before it is defined.
- If you accidentally use a tuple type instead of an array type for a rest param then Flow will complain
- You cannot use `this` before `super()` in a derived constructor, per ES6 semantics
- Our dictionary property (aka indexer property) support is much more robust and catches things it previously missed.
- We weren't properly enforcing optional properties in `interfaces` and `declare class`. Now we are.

New Features:

- Generators support, courtesy of [@samwgoldman](https://github.com/samwgoldman)
- The number of worker processors is now configurable, defaults to the # of CPUs
- If Flow knows the value of a boolean expression, then it will know the value of that expression negated.
- Flow can remember refinements for things like `if(x.y[a.b])`
- `export type {type1, type2}` syntax

Notable Bug Fixes:

- Fixed issue where Flow would still read package.json for [ignore]'d path
- Fixed some leaky annotations that let data flow through them
- Fixed instance and class types to be considered truthy
- Flow still initializing message now fits in 80 chars, compliments of [@spicyj](https://github.com/spicyj)
- No longer will report hoisted declarations as unreachable code
- Fixed issue with how Flow chooses its tmp dir
- An async function can return a `T` or a `Promise<T>` and it means the same thing
- Fixed Flow forgetting about optional properties after an assignment refinement
- Fixed parser issue around future reserved keywords
- Optional parameters and rest parameters now work better together

Misc:

- Updated error locations. We've spent a lot of time auditing our error locations and trying to move them closer to the actual errors.
- esproposal.decorators option to tell Flow to parse and ignore decorators
- Bunch of updates to the libraries
- Some perf work
- Test output is colorized

### v0.14.0

Likely to cause new Flow errors:

- Assignment now triggers a refinement. If you have a variable that is a `?string` and you assign 'hello' to it, Flow refines its type to 'string'.

Likely to fix old Flow errors:

- We now treat missing type parameters as `any`. For example, previously `ReactElement` was treated as `ReactElement<*, *, *>`. Now it's treated as `ReactElement<any, any, any>`.

Misc:

- Basic unsafe support for getters & setters, gated behind the config option `unsafe.enable_getters_and_setters`
- Support for block comments inside of Flow's comment syntax
- Disable by default munging of class property names that start with an underscore, with an option to enable it
- Lots of small internal fixes and merged PRs
- Basic semver support for the .flowconfig version
- Support for `declare type` in lib files
- Type annotations are now opaque - other types will not flow through them
- You can configure the tmp dir that Flow uses

### v0.13.1

Likely to cause new Flow errors:

- Restricted `+` and `+=` to only allow strings and numbers, and no longer implicitly cast objects, booleans, null or undefined. Use `String(x)` to explicitly cast these values.
- Fixed a few bugs where types shared between modules may have lost precision or weren't enforced.

Misc:

- Added `import typeof` feature that allows you to import the type of a *value* export from another module. It is sugar for: `import MyThing_tmp from "MyModule"; type MyThing = typeof MyThing_tmp;` (except it removes the need for the intermediate `MyThing_tmp` variable)
- Added `flow ast` command to print a serialized JSON [ESTree](https://github.com/estree/estree) AST. (Note that this AST does not include types, just syntactic structure for now)
- Added support for class expressions
- Added support for following symlinks
- Added support for number-literal and boolean-literal annotations. (useful for things like enum types and refinements based on tests of equivalence between two variables)
- Added support for ES6 binary and octal integer literals
- Added support for `export type` within a CommonJS module that also uses the `module.exports = ...` pattern
- Added support for refining some union types down to their disjoint members
- Added support for covariant Promise type parameters
- Added improved support for understanding ES5-style imperative class definitions (i.e. via functions + prototypes)
- Fixed passing `undefined` to optional parameters
- Fixed return-type tracking for tagged template usage
- Fixed an issue where library parse errors would cause the flow server to continuously restart upon initialization without giving an error

### v0.12.0

Likely to cause new Flow errors:

- Fixed a bug where declarations from libraries which are exported from one module are not checked properly in the module into which that module is imported (e.g. if `A.foo()` returns a `Promise`, and module B requires A and calls `A.foo()`, the return type of `A.foo()` was not being checked properly)
- Fixed enforcement of Object and Function type annotation arity, so that `Object<K, V>` errors
- Restricted valid computed properties, such that only strings and numbers are allowed (notably, disallows `null` and `undefined`)

New features:

- Added support for `for-of` and support for `Iterable` interface(s)
- Added support for `async`/`await`
- Added structural subtyping for interfaces -- anything can be an instance of an interface as long as it looks right
- Added support for type annotations of the form `typeof x`, where `x` is the name of an in-scope variable
- Added a new config option `suppress_comment`, a regexp which matches against comments and causes any Flow error on the next line to be suppressed. For example, `suppress_comment=.*\$FlowFixMe.*` will cause `/* $FlowFixMe */\nvar x : number = "oops";` to not raise an error.
- Added a new config option `module.name_mapper`, a regexp -> replacement template tuple to be applied to any matching module names before the Flow system resolves the name and looks it up
- Added a `--color=always|never|auto` CLI option, useful when piping to `less -R`
- Added a `--one-line` CLI option which replaces `\n` with `\\n` in multiline error messages, useful when piping to `grep`

Misc:

- Many improvements to library files, especially node and ES6 APIs
- Improved warnings on unsupported class members [PR #461]
- Added support for `export default class`
- Fixed `if (x instanceof Array)`
- Fixed the type of `x && y` when `x` is an array, object or function
- Fixed the `flow get-def` command, especially around imported types
- Fixed a bug with `==` and improved comparison-related error messages
- Fixed file watching for individual files included via .flowconfig [includes]
- Fixed the build ID, so that the server restarts when accessed from a mismatched client version
- Added a new config option `log.file` which overrides the default log file path


### v0.11.0

- We are now syncing Flow's [commit history](https://github.com/meta/flow/commits/master) to GitHub. No more huge updating diffs. We'll also filter the changelog to the most important things.
- Big React refactoring to support ES6 React classes
- Do you use `any` to workaround things that you want to fix later? Well, now you can use `$FixMe` instead of `any` and easily grep for these workarounds later.
- We now report parsing errors in non-@flow files that you are `require()`'ing/`import`'ing
- Better error messages and better error message positions
- Better error traces, with `flow check --traces N`, where `N` is the trace depth
- Basic support for `Object.freeze()`
- Assorted fixes and updates to the flow libs
- We're trying to be better about commenting the code

### v0.10.0

- Bump version to 0.10.0
- Support import/export type
- [PR #412] More fs implementation
- Support for static overloads
- Fix order of args matched with overloads
- Allow methods to have properties
- [PR #405] Make interface-defined modules work with CommonJS/ES6Module interop
- Refine mixed
- Fix typeof null === "object" refinement
- [PR #397] Facilitate debug builds
- [PR #401] Improve error message clarity in a few places
- Remove 'everything is a bool' implicit cast
- Fix issues with optional properties during InstanceT ~> ObjT
- Fix abnormals in catch blocks

### v0.9.2

- Fix lowercasing issue where "flow Path/To/Root" became "flow path/to/root"
- Fix "My Path/To Flow/flow path/to/root" not autostarting server due to spaces

### v0.9.1

- Unbreak the command line for "flow path/to/root" (thanks samwgoldman for the report!)

### v0.9.0

- Bump version to 0.9.0
- Add Video-, Audio and TextTrackList (+ dependencies)
- Refine switch cases
- Better Not_found error
- [PR #333] Partial fs module implementation
- Update parser to match esprima-fb
- [PR #370] Dom canvas context
- [PR #393] Symbol should be called statically. Fix #391
- Fix havoc_env to copy the `for_type` value
- add bash completion
- rewrite all the commands to use CommandSpec
- json_of_t
- [PR #384] Env printer
- [PR #383] Add some build/test artifacts to .gitignore
- [PR #377] Add missing HTMLImageElement properties
- [PR #367] Re-add merged out oneOfType proptype implementation
- Allow users to pin their .flowconfig to a specific version
- colorize test failure diffs
- Object spread fix
- [PR #322] Add type signature for node "url", "querystring" and "path" modules
- command parsing library
- look for package.json in include paths
- clear module errors properly
- add verbose mode

### v0.8.0

- Bump version to 0.8.0
- [PR #356] Add React.version to lib/react.js
- [PR #331] node http.get() implementation
- Fixes for indexers in object types and objects used as dictionaries
- Flow server speedup
- Support both commas and semicolons in object type patterns
- Fix object assign for unresolved objects
- Add prerr_endlinef
- Fix support for the mixin pattern of specifying super
- Incremental typechecking fix
- Fix up promises interface definition + more tests
- Perf win by loading master_cx once per job
- Support for-in of null and undefined
- Support typeof x === 'boolean'
- Refine true and false literals
- Fix refinement unit test
- Type declarations for Geolocation interface
- [PR #321] Support for React.cloneElement in 0.13
- Refine simple assignments (things like while (x = x.parent) { x.doStuff(); })

### v0.7.0

- Bump version to 0.7.0
- Initial support for ES6 import/export (with CommonJS interop)
- Updates to CSS/CSSOM interface definitions
- propTypes in React components now dictate the concrete type signature for this.props
- Show errors in type-at-pos output
- Flow now watches include paths specified in .flowconfig for file changes
- Interpret `x != undefined` and `x != null` as the same refinement
- Use Object.prototype.hasOwnProperty() calls as a refinement
- Updates to Element and HTMLElement interface definitions

### v0.6.0

- Bump version to 0.6.0
- Also watch for changes in include paths
- Fix the Function type to be any function
- Add Symbol global name to libs
- Support trailing commas in parser
- Make methods immutable
- Remove tuple array length limit
- [PR #315] Implement more PropTypes
- Update componentWillReceive spec
- Unsuppress library errors

### v0.5.0

- Bump version to 0.5.0
- Add HTMLAnchorElement
- [PR #295] Add one of proptype
- Update the parser to work with the new esprima-fb & ast-types
- [PR #299] Declare prompt function (fixes #204)
- [PR #303] Add String.prototype.endsWith()
- quick fix for react/es6 notation
- extend scope of type params to following bounds
- reasonless shortcut
- Add React.findDOMNode to lib/react.js
- move command documentation into each command
- fix path in --help usage example
- fix predicate filtering of null and undefined
- [PR #292] Types for React synthetic events
- basic support for bounded polymorphism
- infer falsiness
- node haste module support
- add Abnormal.string

### v0.4.0

** Flow comments should be ready for use **

- Bump version to 0.4.0
- [PR #259] Add declaration for Document.execCommand()
- [PR #265] flow status --json should exit 2 on errors
- Fix issue with switch statement breaks
- abnormal exit effects of fall-through cases in switch
- Let AnyObjT flow to KeyT
- invariant(false, ...) always throws
- Improve the "Object" annotations
- sealed object literals
- fix return types which could be undefined
- Support type annotations in comments
- [PR #263] disable colors when TERM=dumb
- Add lint checks for TRUE / FALSE / NULL
- class subtyping
- array literal inference
- Add unsafe_xhp typechecker option
- work around false positive conflict markers
- Better dumping types for debugging

### v0.3.0

** Type casts and import type should be ready for use **

- Bump version to 0.3.0
- Move haste module name preprocessing into module_js
- [PR #260] Add SyntheticEvent to lib/react.js
- Add import type to Flow
- [Parser] Add esprima tests for typecasts
- Add support for a module name preprocessor that can re-write require()d module names
- [Parser] Support import types
- Allow absolute paths for [libs] and [include]
- prepare flow-parser to be published to npm
- [PR #247] Resubmit #113 this fixes #155
- [PR #246] Fixes #195

### v0.2.0

- Bump version to 0.2.0
- Fix refinement of optional nullable types
- Typecast expressions (warning - not added to strip out transform yet)
- treat throw as similar to return
- Switch from React to react (React still included until https://github.com/facebook/react/pull/3031)
- Fix types for sessionStorage and Storage
- tighter type for undefined values
- no lib paths fix
- Fix exporting types for builtin modules
- Add Number.isNaN and Number.isSafeInteger type annotations
- Optional properties in objects & optional type revamp

### v0.1.6

- Bump version to 0.1.6
- declare modules that redefine exports instead of exporting multiple things
- Object spread race condition fix
- Fix fallback path to flowlib.tar.gz
- [Parser] Type grouping bug fix
- Add Object.is
- uncomment `delete` method signatures
- Add MAX_SAFE_INTEGER and MIN_SAFE_INTEGER to Number
- make param annotations strict upper bounds

### v0.1.5

- Bump version to 0.1.5
- [PR #223] from commonlisp/master
- [PR #221] from teppeis/Javascript-to-JavaScript
- Better types for React.PropTypes
- Make React a module
- Check PropTypes as a normal object too
- Better support for object call properties
- Add range/offset information to Ast.Loc.t
- ignored react attributes key & ref passed in spread properties
- ignored react attributes key & ref
- Add some checks when upper bound object type has indexer
- fix type arg arity exception
- add traces to .flowconfig
- fix error message when undeclared prop is set by jsx
- [Parser] Actually test ArrayTypeAnnotation
- Fix problem with returns in catch
- JSX with spread attributes
- Incremental mode fix
- Add missing constructors to TypedArray declarations
- [Parser] Update Flow's test suite to match updated esprima

### v0.1.4

- Bump Flow version to 0.1.4
- [Flow] Unbreak the open source build
- [PR #200] from fmahnke/node_http_interface
- [PR #203] from rsolomo/node-net-isip
- [PR #201] from unknownexception/patch-1
- [Flow] Fill out Map definition
- try-catch
- missing annotation errors for type aliases
- Type refinement fixes for early return

### v0.1.3

- Handle strict undefined checks.
- Add theoretical support to print tuple types properly.
- Relax the callback type in Array.prototype.forEach
- Add basic autocomplete support for React classes.
- [Hack] Kill shell.ml entirely.
- Handle spread attributes in JSX syntax.
- [Parser] Allow most keywords as types
- Add Array.from to lib file
- [#161]: String literal keys for object type aliases
- Generalize mock module handling under Haste a bit.
- Improve autocomplete type printing
- misc cleanup
- [Parser] Improve type keywords
- [Parser] Stop vendoring in ast-types
- Add an ocaml wrapper for find path1 ... pathN -name pattern
- Restart if a known lib file changes
- Add ProgressEvent definition and update two onprogress types
- [Hack] Add client time to server start time.
- Improve control flow type refinements
- [Hack] Reduce dependencies on ParserHeap.
- Exit server on config change for now rather than restart to avoid some bugs
- [Parser] ArrowFunction object & array patterns must be wrapped with parens
- Properly functorize logging code too
- [Hack] Make GC parameters configurable.
- Refinements on method calls
- [Hack] Log whether we are loading from a saved state in INIT_END.
- [Hack] Improve error handling for saved states.
- [PR #137] from sverrejoh/master
- [PR #150] from unknownexception/fix/dom.js
- [PR #146] from k-bx/master
- [PR #151] from unknownexception/feature/no_flowlib
- [PR #144] from rsolomo/node-assert
- [PR #191] from Shraymonks/HTMLBaseElement
- [PR #145] from rsolomo/node-util
- [PR #192] from Shraymonks/HTMLScriptElement
- Add ES6 String.prototype.contains
- Add Promise.prototype.done() to Flow lib

### v0.1.2

- [#78] Fallback to index.js if no main attribute in package.json
- [#82] Added ES6 functions to the Math object
- [#94] Start a node stdlib interface file
- [#98] Declare optional parameters according to HTML standard
- [#108] Add buffer class to node.js interface
- [#115] Some more node std libs
- The module system can now be specified in the .flowconfig file
- Libraries can now be specified in the .flowconfig file
- Multiple library paths can be specified
- Added a command to typecheck a file passed on standard input (for IDEs)
- Added HTMLInputElement to the standard library
- Improvements to type printing in flow suggest and other commands
- Fixes to various issues where parser errors were being suppressed

### v0.1.1

- [Issue #4] Typecheck .jsx files
- [Issue #22] Return a nonzero exit code from 'flow check' if there are errors
- [IRC report] Autostart the Flow server on all commands
- Improve the printing of types in 'flow suggest'
- Add a --timeout option to all commands
- [PR #27] Clearer error message when 'flow init' hasn't been run
- [PR #39] Fix to Emacs bindings
- [PR #53] Support node modules that end in .js
- [PR #59] Fix example
- [PR #65] Fix dependencies in flux-chat example
- [PR #66] Add type definitions for HTMLCanvasElement

### v0.1.0

Initial release
