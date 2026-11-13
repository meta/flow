---
title: .flowconfig [options]
slug: /config/options
description: "Reference for all available .flowconfig [options] settings, including type checking behavior, module resolution, and more."
---

import {SinceVersion} from '../../components/VersionTags';

The `[options]` section in a `.flowconfig` file can contain several key-value
pairs of the form:

```
[options]
keyA=valueA
keyB=valueB
```

Any options that are omitted will use their default values. Some options can be
overridden with command line flags.

## Available options {#toc-available-options}

### all {#toc-all}

Type: `boolean`

Set this to `true` to check all files, not just those with `@flow`.

The default value for `all` is `false`.

### autoimports {#toc-autoimports}

Type: `boolean`

When enabled, IDE autocomplete suggests the exports of other files, and the
necessary `import` statements are automatically inserted. A "quick fix" code
action is also provided on undefined variables that suggests matching imports.

The default value for `autoimports` is `true`.

### babel_loose_array_spread {#toc-babel-loose-array-spread}

Type: `boolean`

Set this to `true` to check that array spread syntax is only used with arrays,
not arbitrary iterables (such as `Map` or `Set`). This is useful if you
transform your code with Babel in
[loose mode](https://babeljs.io/docs/babel-plugin-transform-spread)
which makes this non-spec-compliant assumption at runtime.

For example:

```js
const set = new Set();
const values = [...set]; // Valid ES2015, but Set is not compatible with ReadonlyArray in Babel loose mode
```

The default value for `babel_loose_array_spread` is `false`.

### ban_spread_key_props {#toc-ban-spread-key-props}

Type: `boolean`

Set this to `true` to error when a JSX spread attribute contains an object with a
`key` property. React does not support spreading `key` — it is always determined by
the call site, so a `key` inside a spread object is silently ignored at runtime. Enabling
this option surfaces those cases as Flow errors with the `invalid-spread-prop` error code.

For example:

```js
const props = {key: 'item-1', name: 'Alice'};
<Component {...props} />; // Error: Cannot spread an object that contains a `key` property
```

Flow also detects `key` properties inside union types and nested objects that are spread:

```js
declare const props: {name: string} | {key: string};
<Component {...props} />; // Error
```

To fix these errors, pass `key` directly as a JSX attribute instead of spreading it:

```js
const props = {name: 'Alice'};
<Component key="item-1" {...props} />; // OK
```

The default value for `ban_spread_key_props` is `false`.

### builtin_lib {#toc-builtin-lib}

Type: `default | core-only | prelude | experimental.tslib`

Selects the builtin library definitions that Flow loads:

- `default` loads Flow's core and React definitions together
  with TypeScript's DOM, DOM iterable, and DOM async iterable definitions.
- `core-only` loads Flow's core and React definitions without DOM definitions.
- `prelude` loads only the minimal definitions that Flow needs to run.
- `experimental.tslib` loads the TypeScript standard library definitions.

You can override this setting for a command with `--builtin-lib`.

The default value is `default`.

### component_syntax {#toc-component-syntax}

Type: `boolean`

Enables Flow's [Component Syntax](../react/component-syntax.md) and
[Hook Syntax](../react/hook-syntax.md), the dedicated `component` and `hook`
primitives for writing React components and hooks. Enabling this option also
turns on Flow's React rules, which catch misuses of components and hooks.
Setting it to `false` disables both the syntax and those rules.

The default value for `component_syntax` is `true`.

### emoji {#toc-emoji}

Type: `boolean`

Set this to `true` to add emoji to the status messages that Flow outputs when
it's busy checking your project.

The default value for `emoji` is `false`.

### enums {#toc-enums}

Type: `boolean`

Enables [Flow Enums](../enums/index.md). When enabled, Flow understands `enum`
declarations. [Additional setup steps](../enums/index.md#toc-enabling-enums) are
required beyond just this `.flowconfig` option (a Babel transform and a runtime
package). Set this to `false` to disable enum support.

The default value for `enums` is `true`.

### experimental.const_params {#toc-experimental-const-params}

Type: `boolean`

Setting this to `true` makes Flow treat all function parameters as const
bindings. Reassigning a param is an error which lets Flow be less conservative
with refinements.

The default value is `false`.

### experimental.strict_es6_import_export {#toc-experimental-strict-es6-import-export}

Type: `boolean`

Set this to `true` to enable a collection of lint rules that enforce strict ES6
module import and export practices. When enabled, Flow detects several patterns
that can cause confusing behavior or interop issues in ES modules.

These lint rules only produce errors when this config option is enabled.
Without `experimental.strict_es6_import_export=true`, the rules have no effect
even if they are configured in the `[lints]` section. Each rule can be set to
`error`, `warn`, or `off` independently.

See the [strict import/export lints](../linting/strict-import-export-lints.md)
page for the full list of rules, detailed examples, and explanations.

The default value for `experimental.strict_es6_import_export` is `false`.

### format.bracket_spacing {#toc-format-bracket-spacing}

Type: `boolean`

Controls whether Flow inserts spaces between brackets and content in object
literals, object type annotations, and import/export specifiers in generated
code (such as autofixes and code actions).

When `true` (the default), Flow generates code with spaces inside braces:

```js
import { Foo } from "./foo";
type T = { bar: number };
```

When `false`, Flow omits those spaces:

```js
import {Foo} from "./foo";
type T = {bar: number};
```

The default value for `format.bracket_spacing` is `true`.

### format.single_quotes {#toc-format-single-quotes}

Type: `boolean`

Controls whether Flow prefers single-quoted strings over double-quoted strings
in generated code (such as autofixes and code actions).

When `true`, Flow generates string literals with single quotes:

```js
import Foo from './foo';
```

When `false` (the default), Flow uses double quotes:

```js
import Foo from "./foo";
```

If the chosen quote character appears in the string value, Flow automatically
uses the other quote style to minimize escaping.

The default value for `format.single_quotes` is `false`.

### include_warnings {#toc-include-warnings}

Type: `boolean`

Setting this to `true` makes Flow commands include warnings in the error output.
Warnings are hidden by default in the CLI to avoid console spew. (An IDE is a
much better interface to show warnings.)

The default value is `false`.

### jest_integration {#toc-jest-integration}

Type: `boolean`

Set this to `true` to enable special support for
[Jest module mocking functions](https://jestjs.io/docs/jest-object#mock-modules).
When enabled, Flow resolves the module name argument passed to the following
`jest` methods, just like it does for `require()` and `import`:

- `jest.createMockFromModule`
- `jest.mock`
- `jest.unmock`
- `jest.deepUnmock`
- `jest.doMock`
- `jest.dontMock`
- `jest.setMock`
- `jest.requireActual`
- `jest.requireMock`

If the module cannot be resolved, Flow reports a `cannot-resolve-module` error:

```js
jest.mock('./utils'); // OK — module exists
jest.mock('./nonexistent'); // Error: Cannot resolve module `./nonexistent`.
```

Without this option, Flow treats `jest` as a regular value and does not perform
any module resolution on the string arguments passed to these functions.

> **Note:** This option only takes effect when `jest` is a global (not a local
> variable). If you have a local binding named `jest` (e.g. `const jest = ...`),
> Flow will not apply module resolution to its method calls.

The default value for `jest_integration` is `false`.

### lazy_mode {#toc-lazy-mode}

Type: `boolean`

Setting `lazy_mode` in the `.flowconfig` will cause new Flow servers for that
root to use lazy mode (or no lazy mode if set to `false`). This option can be
overridden from the CLI using the `--lazy-mode` flag.

The default value is `false`.

In lazy mode, Flow classifies files into four categories:

1. **Focused files** — files the user is actively working on.
2. **Dependent files** — files that depend on focused files (changes to focused files might cause errors here).
3. **Dependency files** — files needed to typecheck the focused and dependent files.
4. **Unchecked files** — everything else. These are parsed but not typechecked.

Flow determines focused files by watching for changes on disk (using Watchman or
its built-in file watcher). For files that changed while Flow was not running,
Flow uses your version control system (Git or Mercurial) to find all files that
have changed since the mergebase with your main branch. You can configure which
branch to diff against with `file_watcher.mergebase_with` (e.g. set it to
`origin/master` if working from a clone).

The result is that lazy mode finds the same errors as a full check, as long as
there are no errors upstream. For example, if CI ensures no errors on your main
branch, it's redundant for Flow to recheck all unchanged files.

You can manually force Flow to treat specific files as focused:

```bash
flow force-recheck --focus path/to/A.js path/to/B.js
```

### max_header_tokens {#toc-max-header-tokens}

Type: `integer`

Flow tries to avoid parsing non-flow files. This means Flow needs to start
lexing a file to see if it has `@flow` or `@noflow` in it. This option lets you
configure how much of the file Flow lexes before it decides there is no relevant
docblock.

- Neither `@flow` nor `@noflow` - Parse this file with Flow syntax disallowed
  and do not typecheck it.
- `@flow` - Parse this file with Flow syntax allowed and typecheck it.
- `@noflow` - Parse this file with Flow syntax allowed and do not typecheck it.
  This is meant as an escape hatch to suppress Flow in a file without having to
  delete all the Flow-specific syntax.

The default value of `max_header_tokens` is 10.

### merge_timeout {#toc-merge-timeout}

Type: `unsigned integer`

The maximum time in seconds to attempt to typecheck a file or cycle of files.
If a file exceeds this timeout during the merge (type checking) phase, Flow
will skip it and log a timeout error.

Setting this to `0` disables the timeout entirely.

The default value for `merge_timeout` is `100`.

This can be overridden from the CLI using the `--merge-timeout` flag or the
`FLOW_MERGE_TIMEOUT` environment variable.

### module.declaration_dirnames {#toc-module-declaration-dirnames}

Type: `string`

By default, Flow looks for declaration (`.flow`) files in the `@flowtyped` directory at the
project root. Declaration files in these directories let you provide type definitions for
third-party modules without modifying the original packages (see
[declaring a module in the `@flowtyped` directory](../libdefs/creation.md#toc-declaring-a-module-in-at-flowtyped)).

You can use `module.declaration_dirnames` to customize which directories Flow searches for
declaration files. When you explicitly set this option, it **replaces** the default
`@flowtyped` directory entirely — `@flowtyped` will no longer be searched unless you
include it in your list.

For example, if you do:

```
[options]
module.declaration_dirnames=<PROJECT_ROOT>/decl1
module.declaration_dirnames=<PROJECT_ROOT>/decl2
```

Then Flow will look for declaration files in the `decl1` and `decl2` directories instead of
`@flowtyped`. A module specifier like `foo` will resolve to `decl1/foo.js.flow` or
`decl2/foo.js.flow` (or their `index.js.flow` equivalents) before falling back to
`node_modules`.

> **Note:** You can specify `module.declaration_dirnames` multiple times to search multiple
> directories. The values `.` and `..` are not allowed.

The default value is `<PROJECT_ROOT>/@flowtyped`.

### module.file_ext {#toc-module-file-ext}

By default, Flow will look for files with the extensions `.js`, `.jsx`, `.mjs`,
`.cjs` and `.json`. You can override this behavior with this option.

For example, if you do:

```
[options]
module.file_ext=.foo
module.file_ext=.bar
```

Then Flow will instead look for the file extensions `.foo` and `.bar`.

> **Note:** you can specify `module.file_ext` multiple times

### module.ignore_non_literal_requires {#toc-module-ignore-non-literal-requires}

Type: `boolean`

Set this to `true` and Flow will no longer complain when you use `require()`
with something other than a string literal.

The default value is `false`.

### module.missing_module_generators {#toc-module-missing-module-generators}

Type: `regex -> string`

Specify a regular expression to match against module names, and a command to
suggest, separated by a `->`.

When Flow encounters a `cannot-resolve-module` error for an import whose module
name matches one of the configured patterns, it appends a hint to the error
message suggesting that you run the associated command to generate the missing
module.

For example, if you add the following to your `.flowconfig`:

```
[options]
module.missing_module_generators='.*\.css$' -> 'build-css-types'
```

Then when Flow reports a missing module error for `require('./Button.css')`, the
error message will include:

```
Cannot resolve module `./Button.css`. Try running the command `build-css-types` to generate the missing module.
```

Without a matching generator, the error is simply:

```
Cannot resolve module `./Button.css`.
```

This is useful in codebases that use code generation to produce type definitions
(e.g. for CSS modules, GraphQL queries, or other non-JS assets). The generator
hint tells developers which command to run to fix the error.

The patterns are
[OCaml regular expressions](http://caml.inria.fr/pub/docs/manual-ocaml/libref/Str.html#TYPEregexp).

> **Note:** You can specify `module.missing_module_generators` multiple times.

### module.name_mapper {#toc-module-name-mapper}

Type: `regex -> string`

Specify a regular expression to match against module names, and a replacement
pattern, separated by a `->`.

For example:

```
module.name_mapper='^image![a-zA-Z0-9$_]+$' -> 'ImageStub'
```

This makes Flow treat `require('image!foo.jpg')` as if it were
`require('ImageStub')`.

These are
[OCaml regular expressions](http://caml.inria.fr/pub/docs/manual-ocaml/libref/Str.html#TYPEregexp).
Use `\(` and `\)` (slashes required!) to create a capturing group, which you can
refer to in the replacement pattern as `\1` (up to `\9`).

> **Note:** you can specify `module.name_mapper` multiple times

### module.name_mapper.extension {#toc-module-name-mapper-extension}

Type: `string -> string`

Specify a file extension to match, and a replacement module name, separated by a
`->`.

> **Note:** This is just shorthand for
> `module.name_mapper='^\(.*\)\.EXTENSION$' -> 'TEMPLATE'`)

For example:

```
module.name_mapper.extension='css' -> '<PROJECT_ROOT>/CSSFlowStub.js.flow'
```

Makes Flow treat `require('foo.css')` as if it were
`require(PROJECT_ROOT + '/CSSFlowStub')`.

> **Note:** You can specify `module.name_mapper.extension` multiple times for
> different extensions.

### module.system {#toc-module-system}

Type: `node | haste`

The module system to use to resolve `import` and `require`. Haste mode is used
by Meta.

The default is `node`.

### module.system.node.allow_root_relative {#toc-module-system-node-allow-root-relative}

Type: `boolean`

Set this to `true` to let Flow resolve imports relative to the project root, so
`require('foo/bar.js')` is resolved starting from the root directory. This
matches the behavior of bundlers like Webpack that can be configured to resolve
modules from the root.

Use `module.system.node.root_relative_dirname` to control which directories
under the root are searched. By default the root itself is used.

The default value for `module.system.node.allow_root_relative` is `false`.

### module.system.node.main_field {#toc-module-system-node-main-field}

Type: `string`

Flow reads `package.json` files for the `"name"` and `"main"` fields to figure
out the name of the module and which file should be used to provide that module.

So if Flow sees this in the `.flowconfig`:

```
[options]
module.system.node.main_field=foo
module.system.node.main_field=bar
module.system.node.main_field=baz
```

and then it comes across a `package.json` with

```
{
  "name": "kittens",
  "main": "main.js",
  "bar": "bar.js",
  "baz": "baz.js"
}
```

Flow will use `bar.js` to provide the `"kittens"` module.

If this option is unspecified, Flow will always use the `"main"` field.

See
[this GitHub issue for the original motivation](https://github.com/meta/flow/issues/5725)

### module.system.node.package_export_condition {#toc-module-system-node-package-export-condition}

Type: `string`

Specify an export condition that Flow uses when resolving the
[`"exports"` field](https://nodejs.org/api/packages.html#conditional-exports) of
a `package.json`. Conditional exports let a package map the same module specifier
to different files depending on the active conditions (for example `import`,
`require`, `node`, or `default`).

For example, if you do:

```
[options]
module.system.node.package_export_condition=import
module.system.node.package_export_condition=node
```

Then Flow resolves `"exports"` entries using the `import` and `node` conditions.

> **Note:** you can specify `module.system.node.package_export_condition` multiple times.

### module.system.node.resolve_dirname {#toc-module-system-node-resolve-dirname}

Type: `string`

By default, Flow will look in directories named `node_modules` for node modules.
You can configure this behavior with this option.

For example, if you do:

```
[options]
module.system.node.resolve_dirname=node_modules
module.system.node.resolve_dirname=custom_node_modules
```

Then Flow will look in directories named `node_modules` or
`custom_node_modules`.

> **Note:** you can specify `module.system.node.resolve_dirname` multiple times

### module.use_strict {#toc-module-use-strict}

Type: `boolean`

Set this to `true` if you use a transpiler that adds `"use strict";` to the top
of every module.

The default value is `false`.

### munge_underscores {#toc-munge-underscores}

Type: `boolean`

Set this to `true` to have Flow treat underscore-prefixed class properties and
methods as private. Use in conjunction with any transformer that enforces
underscore-prefixed-private at runtime (e.g. a Babel plugin).

The default value is `false`.

### name {#toc-name}

Type: `string`

Set the name of your Flow project. The name is used to distinguish between
different Flow projects in editor status messages. When set, LSP status
messages are prefixed with `Flow (<name>)` instead of just `Flow`.

For example, if you set:

```
[options]
name=my-app
```

Then LSP status messages will appear as `Flow (my-app): ...` instead of
`Flow: ...`. This is useful when you work on multiple Flow projects at the same
time in your editor, as it makes it clear which project each status message
refers to.

The name is also included in Flow's internal logging for diagnostic purposes.

There is no default value for `name`.

### no_implicit_override <SinceVersion version="0.339" /> {#toc-no-implicit-override}

Type: `boolean`

Set this to `true` to require an explicit [`override` modifier](../types/classes.md#toc-override) on concrete class members that replace inherited concrete members. Flow applies this requirement to instance and static methods, fields, and accessors.

Enable the syntax as well as the requirement in `.flowconfig`:

```
[options]
experimental.tslib_syntax=true
no_implicit_override=true
```

With these options enabled:

```js
class Base {
  greet(): string {
    return 'hello';
  }
}

class Unmarked extends Base {
  greet(): string { // Error! Add `override`
    return 'hi';
  }
}

class Marked extends Base {
  override greet(): string { // OK
    return 'hi';
  }
}
```

You do not need `override` when implementing an inherited abstract member or redeclaring a member as abstract. Flow also skips this requirement for ambient declarations, including `declare class` bodies.

Flow still checks that every explicit `override` modifier has a valid inherited target, even in an ambient declaration. Members must also remain compatible with their inherited types.

The default value for `no_implicit_override` is `false`. See [override adoption](../types/classes.md#toc-override-adoption) for the required syntax option and tooling versions.

### no_unchecked_indexed_access {#toc-no-unchecked-indexed-access}

Type: `boolean`

Set this to `true` to make Flow include `void` in the result type of indexing
into an array or object with a dictionary. This catches cases where you access
an element that may not exist at runtime, such as an out-of-bounds array index
or an absent dictionary key.

By default, accessing an element of an `Array<string>` returns `string`. With
this option enabled, the same access returns `string | void`, forcing you to
handle the `undefined` case:

```js
const items: Array<string> = ["hello", "world"];
const item = items[0]; // type is `string | void` when enabled

// You must refine before using it as a string:
if (item != null) {
  item.toUpperCase(); // OK
}
```

This option affects the following indexed access patterns:

**Arrays** — accessing elements by index on `Array<T>` or `ReadonlyArray<T>`
returns `T | void` instead of `T`:

```js
declare const arr: Array<string>;
const x = arr[0]; // string | void
const y = arr[someIndex]; // string | void
```

**Objects with indexers** — accessing properties on dictionary objects (objects
with an indexer like `{[string]: T}`) returns `T | void` instead of `T`, for
both bracket and dot access on keys not explicitly present in the type:

```js
declare const dict: {[string]: number};
const a = dict["key"]; // number | void
const b = dict.key; // number | void
```

**Tuples with dynamic keys** — accessing a tuple with a non-literal key returns
`T | void`:

```js
declare const tuple: [number, string, boolean];
declare const i: number;
const val = tuple[i]; // number | string | boolean | void
```

Note that tuple access with a *literal* index is unaffected — `tuple[0]`
still returns the precise element type (or an out-of-bounds error):

```js
declare const tuple: [number, string, boolean];
tuple[0]; // number (not number | void)
tuple[1]; // string
```

**Type-level indexed access is not affected.** Using indexed access types like
`T[K]` in type annotations continues to return the element type without `void`:

```js
type Items = Array<string>;
type Item = Items[number]; // string, not string | void
```

Writing `undefined` into an array or dictionary that does not include `void` in
its value type is still an error, regardless of this option. The option only
affects *reads*.

The default value for `no_unchecked_indexed_access` is `false`.

:::info TypeScript comparison
TypeScript's `noUncheckedIndexedAccess` and Flow's `no_unchecked_indexed_access` are direct equivalents. See [Config options aligned with TypeScript](../flow-vs-typescript.md#toc-shared-options) for the full comparison.
:::

### pattern_matching {#toc-pattern-matching}

Type: `boolean`

Enables Flow's [`match` expressions and statements](../match/index.md), which
match an input value against a series of patterns to conditionally check its
structure and extract values. Set this to `false` to disable `match` support.

The default value for `pattern_matching` is `true`.

### react.custom_jsx_typing {#toc-react-custom-jsx-typing}

Type: `boolean`

Set this to `true` to replace Flow's built-in React JSX type checking with a
user-defined function type. When enabled, Flow type-checks JSX element creation
by calling your globally-defined `React$CustomJSXFactory` type instead of the
built-in `React.createElement` typing.

This is useful when working in repositories (such as the React repo itself) where
components and elements do not conform to Flow's built-in React component model.
By defining your own factory type, you control exactly how the component, props,
and children are checked.

#### Defining `React$CustomJSXFactory`

When `react.custom_jsx_typing` is enabled, you must define a global type called
`React$CustomJSXFactory`. This type should be a function type whose parameters
correspond to the arguments of a JSX element creation call:

1. **component** -- the component being rendered (the JSX tag)
2. **props** -- an object type for the element's attributes
3. **children** -- one parameter per child passed between the opening and closing tags

The return type determines the type of the JSX expression.

For example, a permissive definition that accepts anything:

```js
declare type React$CustomJSXFactory = (
  component: any,
  props: any,
  ...children: Array<any>
) => React.MixedElement;
```

A more restrictive definition that enforces specific types:

```js
declare type React$CustomJSXFactory = (
  component: React.ElementType,
  props: {name: string},
  child1: number,
  child2: string,
) => React.MixedElement;
```

With the restrictive definition above, Flow checks that JSX elements match the
exact parameter types -- the component must be a `React.ElementType`, props must
have a `name` property of type `string`, and exactly two children of types
`number` and `string` must be provided.

#### How it works

When Flow encounters a JSX expression like `<Com foo="bar">{1}{2}</Com>`, it
desugars the element creation into a call to your `React$CustomJSXFactory` type:

```js
// JSX:
<Com foo="bar">{1}{2}</Com>

// Checked as if calling:
React$CustomJSXFactory(Com, {foo: "bar"}, 1, 2)
```

Each child is passed as a separate argument (not collected into an array), so
your factory type can enforce per-child types if needed.

Flow still validates that `React.createElement` is in scope and is compatible
with your `React$CustomJSXFactory` type when using the classic React runtime.
If `React` or `React.createElement` is missing or incompatible, Flow reports
an error in addition to checking the JSX against your custom factory type.

#### Configuration

Add the following to your `.flowconfig`:

```
[options]
react.custom_jsx_typing=true
```

Then define `React$CustomJSXFactory` as a global type in a
[library definition](../libdefs/creation.md) file (`.js.flow`):

```js
// flow-typed/custom-jsx.js.flow
declare type React$CustomJSXFactory = (
  component: any,
  props: any,
  ...children: Array<any>
) => React.MixedElement;
```

Flow exposes the `React` namespace as a global, so no import is needed in `.js.flow` files.

The default value for `react.custom_jsx_typing` is `false`.

### react.runtime {#toc-react-runtime}

Type: `automatic | classic`

Set this to `automatic` if you are using React's automatic runtime in
`@babel/plugin-transform-react-jsx`. Otherwise, use `classic`.
[See the babel documentation](https://babeljs.io/docs/en/babel-plugin-transform-react-jsx)
for details about the transform.

The default value is `classic`.

### server.max_workers {#toc-server-max-workers}

Type: `integer`

The maximum number of workers the Flow server can start. By default, the server
will use all available cores.

### relay_integration {#toc-relay-integration}

Type: `boolean`

This option enables Flow's [Relay](https://relay.dev) integration. With the
integration enabled Flow will infer the types of `graphql` tagged template
literals as being the types that the Relay compiler emitted for that
fragment/mutation/query/etc. This allows users to omit type parameters from
common Relay APIs like `useFragment` and `usePreloadedQuery`.

### relay_integration.esmodules {#toc-relay-integration-esmodules}

Type: `boolean`

When this option is enabled along with `relay_integration`, Flow treats the
types of `graphql` tagged template literals as ES module default exports rather
than CommonJS `require` imports. Use this when your Relay compiler is configured
to output ES modules (e.g. with the `eagerEsModules` option).

By default, Flow's Relay integration assumes that Relay artifact files use
CommonJS (`module.exports = ...`). When `relay_integration.esmodules` is
enabled, Flow instead resolves the types as `import default` from the artifact
module, matching the `export default ...` syntax that Relay emits in ESM mode.

```
[options]
relay_integration=true
relay_integration.esmodules=true
```

### relay_integration.excludes {#toc-relay-integration-excludes}

Type: `string`

This option allows you to exclude some directories in the project from using
Flow's Relay integration. For example

```
relay_integration=true
relay_integration.excludes=<PROJECT_ROOT>/dirA
relay_integration.excludes=<PROJECT_ROOT>/dirB
```

### unsuppressable_error_codes {#toc-unsuppressable-error-codes}

Type: `string`

Specify an [error code](../errors/index.md) that cannot be silenced with a suppression
comment (such as `$FlowFixMe`). Errors with these codes are always reported,
even when a suppression comment would otherwise apply. This is useful for
enforcing that certain classes of errors are never ignored. See
[Unsuppressable Errors](../errors/index.md#toc-unsuppressable-errors) for an example.

> **Note:** you can specify `unsuppressable_error_codes` multiple times.

By default, no error codes are unsuppressable.

### use_unknown_in_catch_variables {#toc-use-unknown-in-catch-variables}

Type: `boolean`

Changes the default type of `catch` variables from [`any`](../types/any.md) to
[`unknown`](../types/unknown.md). E.g.

```js flow-check
try {
} catch (e) {}
```

in the above example, if the option is `true`, `catch` will be typed as
`unknown` as it lacks an explicit type annotation.

:::info TypeScript comparison
TypeScript's `useUnknownInCatchVariables` and Flow's `use_unknown_in_catch_variables` are direct equivalents. See [Config options aligned with TypeScript](../flow-vs-typescript.md#toc-shared-options) for the full comparison.
:::

## Deprecated and removed options {#toc-deprecated-and-removed-options}

:::warning
If your `.flowconfig` still sets any of the options below, remove or update
those settings. They are deprecated or no longer supported, and some now cause
Flow to error on startup.
:::

The following `[options]` settings are listed here so existing `.flowconfig`
files can be migrated. Each entry notes the Flow version in which it changed.

### exact_by_default {#toc-exact-by-default}

Type: `boolean`

Objects are exact by default, which is now the only supported behavior:

```js flow-check
type O1 = {foo: number}; // exact
type O2 = {| foo: number |}; // exact (explicit syntax)
type O3 = {foo: number, ...}; // inexact (explicit `...`)
```

Exact object types have been the default since 2023. Setting
`exact_by_default=false` (which made object types inexact unless written with the
explicit exact `{| |}` syntax) was **deprecated in Flow 0.314.0**. The option is
now unsupported, including `exact_by_default=true`. Delete it from your
`.flowconfig` when migrating. See the
[objects documentation](../types/objects.md) for more.

### casting_syntax {#toc-casting-syntax}

Type: `as | colon | both`

**Removed in Flow 0.319.0.** The [`as` casting syntax](../types/casting.md) is
now the only supported casting syntax, so this option no longer has any effect
(the `colon` value, for the legacy `(x: T)` syntax, was removed earlier in Flow
0.281.0). See [Modernizing legacy syntax](../modernizing-legacy-syntax.md) for
migration guidance.

### suppress_type {#toc-suppress-type}

Type: `string`

**Removed in Flow 0.280.0.** The supported suppression comments are now fixed
(`$FlowFixMe` and `$FlowExpectedError`, each with an error code); custom
suppression identifiers are no longer configurable. See
[error suppressions](../errors/index.md) for the current syntax.

### max_literal_length {#toc-max-literal-length}

Type: `unsigned integer`

**Removed in Flow 0.288.0.** Flow now decides whether to infer a precise or
general type for a string literal using its natural inference algorithm, so this
length threshold no longer applies.
