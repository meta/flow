---
title: Getting Started
slug: /getting-started
description: Get up and running with Flow, a typed dialect of JavaScript, in a few minutes. Set up a compiler, install flow-bin, and start catching errors with static types.
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

Flow is a typed dialect of JavaScript. It checks your code for errors through
static type annotations. These _types_ allow you to tell Flow how you want your
code to work, and Flow will make sure it does work that way.

```js flow-check
function square(n: number): number {
  return n * n;
}

square("2"); // Error!
```

If you already know TypeScript, Flow's syntax will look [familiar](./flow-vs-typescript.md):
it shares `keyof`, `readonly` properties, `unknown`, indexed access types `T[K]`,
conditional types, mapped types, and type guards. On top of that common ground,
Flow adds first-class language constructs of its own, such as
[`component`](./react/component-syntax.md) syntax for React and
[`match`](./match/index.md) for pattern matching.

## Installation {#toc-installation}

### Setup Compiler {#toc-setup-compiler}

First you'll need to setup a compiler to strip away Flow types. You can
choose between [Babel](https://babeljs.io/) and
[flow-remove-types](https://github.com/meta/flow/tree/main/packages/flow-remove-types).

<Tabs
  defaultValue="babel"
  values={[
    {label: 'Babel', value: 'babel'},
    {label: 'flow-remove-types', value: 'flow-remove-types'},
  ]}>
<TabItem value="babel">

[Babel](https://babeljs.io/) is a compiler for JavaScript code that has
support for Flow. Babel will take your Flow code and strip out any type
annotations. If you already use Babel in your project, see [using Babel](./tools/babel.md).

First install `@babel/core`, `@babel/cli`, `@babel/preset-flow`, and `flow-parser` with
either [Yarn](https://yarnpkg.com/) or [npm](https://www.npmjs.com/).

```bash npm2yarn
npm install --save-dev @babel/core @babel/cli @babel/preset-flow flow-parser
```

Next you need to create a `.babelrc` file at the root of your project with the `@babel/preset-flow` preset and `flow-parser/babel-plugin` plugin.

```json
{
  "presets": ["@babel/preset-flow"],
  "plugins": ["flow-parser/babel-plugin"]
}
```

If you then put all your source files in a `src` directory you can compile them
to another directory by running:

```bash
./node_modules/.bin/babel src/ -d lib/
```

You can add this to your `package.json` scripts easily, alongside your `"devDependencies"` on Babel.

```json
{
  "name": "my-project",
  "main": "lib/index.js",
  "scripts": {
    "build": "babel src/ -d lib/",
    "prepublishOnly": "yarn run build"
  }
}
```

> **Note:** You'll probably want to add a `prepublishOnly` script that runs this
> transform as well, so that it runs before you publish your code to the npm
> registry.

</TabItem>
<TabItem value="flow-remove-types">

[flow-remove-types](https://github.com/meta/flow/tree/main/packages/flow-remove-types) is a small
CLI tool for stripping Flow type annotations from files. It's a lighter-weight
alternative to Babel for projects that don't need everything Babel provides.

First install `flow-remove-types` with either
[Yarn](https://yarnpkg.com/) or [npm](https://www.npmjs.com/).

```bash npm2yarn
npm install --save-dev flow-remove-types
```

If you then put all your source files in a `src` directory you can compile them
to another directory by running:

```sh
./node_modules/.bin/flow-remove-types src/ -d lib/
```

You can add this to your `package.json` scripts easily, alongside your `"devDependencies"` on `flow-remove-types`.

```json
{
  "name": "my-project",
  "main": "lib/index.js",
  "scripts": {
    "build": "flow-remove-types src/ -d lib/",
    "prepublishOnly": "yarn run build"
  }
}
```

> **Note:** You'll probably want to add a `prepublishOnly` script that runs this
> transform as well, so that it runs before you publish your code to the npm
> registry.

</TabItem>
</Tabs>

### Setup Flow {#toc-setup-flow}

Flow works best when installed per-project with explicit versioning rather than
globally.

Luckily, if you're already familiar with `npm` or `yarn`, this process should
be pretty familiar!

#### Add a `devDependency` on the `flow-bin` npm package {#toc-add-flow-bin}

```bash npm2yarn
npm install --save-dev flow-bin
```

#### Add a `"flow"` script to your `package.json` {#toc-add-flow-script}

```json
{
  "name": "my-flow-project",
  "version": "1.0.0",
  "devDependencies": {
    "flow-bin": "^0.339.0"
  },
  "scripts": {
    "flow": "flow"
  }
}
```

#### Run Flow {#toc-run-flow}

The first time, run:

```bash npm2yarn
npm run flow init
```

```
> my-flow-project@1.0.0 flow /Users/Projects/my-flow-project
> flow "init"
```

After running `flow` with `init` the first time, run:

```bash npm2yarn
npm run flow
```

```
> my-flow-project@1.0.0 flow /Users/Projects/my-flow-project
> flow

No errors!
```

### Setup Library Definitions {#toc-setup-library-definitions}

Most library definitions are not bundled with Flow.
They are managed by [flow-typed](https://github.com/flow-typed/flow-typed).

You should have a `flow-typed.config.json` in the root of your project with the
following content:

```json
{
  "env": ["node", "geometry", "html", "streams", "web-animations", "dom", "bom", "cssom", "serviceworkers", "jsx"]
}
```

This tells [flow-typed](https://github.com/flow-typed/flow-typed) which environment
definitions to include. You can adjust the list based on which environments your
project targets.

### Setup ESLint {#toc-setup-eslint}

If you use ESLint, you can read [our page on ESLint](./tools/eslint.md) to set it up to support Flow.

## Usage {#toc-usage}

### Initialize Your Project {#toc-initialize-your-project}

Preparing a project for Flow requires only one command:

```sh
flow init
```

Run this command at the top level of your project to create one, empty file called [`.flowconfig`](./config/index.md). At its most basic level, `.flowconfig` tells the Flow background process the root of where to begin checking Flow code for errors.

And that is it. Your project is now Flow-enabled.

> It is common to have an empty `.flowconfig` file for your project. However, you can [configure and customize Flow](./config/index.md) in many ways through options available to be added to `.flowconfig`.

### Run the Flow Background Process {#toc-run-the-flow-background-process}

The core benefit to Flow is its ability to quickly check your code for errors. Once you have enabled your project for Flow, you can start the process that allows Flow to check your code incrementally and with great speed.

```sh
flow status
```

This command first starts a background process that will check all Flow files for errors. The background process continues running, monitoring changes to your code and checking those changes incrementally for errors.

> You can also type `flow` to accomplish the same effect as `status` is the default flag to the `flow` binary.

> Only one background process will be running at any given time, so if you run `flow status` multiple times, it will use the same process.

> To stop the background process, run `flow stop`.

### Prepare Your Code for Flow {#toc-prepare-your-code-for-flow}

The Flow background process monitors all Flow files. However, how does it know which files are Flow files and, thus, should be checked? Placing the following **before any code** in a JavaScript file is the flag the process uses to answer that question.

```js
// @flow
```

This flag is in the form of a normal JavaScript comment annotated with `@flow`. The Flow background process gathers all the files with this flag and uses the type information available from all of these files to ensure consistency and error free programming.

> You can also use the form `/* @flow */` for the flag as well.

> For files in your project without this flag, the Flow background process skips and ignores the code (unless you call `flow full-check --all`, which is beyond the scope of basic usage).

### Write Flow Code {#toc-write-flow-code}

Now that all the setup and initialization is complete, you are ready to write actual Flow code. For each file that you have flagged with `// @flow`, you now have the full power of Flow and its type-checking available to you. Here is an example Flow file:

```js flow-check
function foo(x: ?number): string {
  if (x) {
    return x; // Error
  }
  return "default string";
}
```

Notice the types added to the parameter of the function along with a return type at the end of the function. You might be able to tell from looking at this code that there is an error in the return type since the function can also return a `number`. However, you do not need to visually inspect the code since the Flow background process will be able to catch this error for you when you [check your code](#toc-check-your-code).

### Check Your Code {#toc-check-your-code}

The great thing about Flow is that you can get near real-time feedback on the state of your code. At any point that you want to check for errors, just run:

```sh
# equivalent to `flow status`
flow
```

The first time this is run, the [Flow background process](#toc-run-the-flow-background-process) will be spawned and all of your Flow files will be checked. Then, as you continue to iterate on your project, the background process will continuously monitor your code such that when you run `flow` again, the updated result will be near instantaneous.

For the [code above](#toc-write-flow-code), running `flow` will yield:

```
Error ------------------------------------------------------------------- example.js:4:12

Cannot return `x` because number [1] is incompatible with string [2]. [incompatible-type]

   example.js:4:12
   4|     return x; // Error
                 ^

References:
   example.js:2:18
   2| function foo(x: ?number): string {
                       ^^^^^^ [1]
   example.js:2:27
   2| function foo(x: ?number): string {
                                ^^^^^^ [2]



Found 1 error
```

## See Also {#toc-see-also}

- [Configuration](./config/index.md) — customizing Flow behavior with `.flowconfig`
- [Type Annotations](./types/index.md) — all the types you can use in Flow
- [Editor Setup](./editors/index.md) — integrating Flow into your IDE
- [Babel](./tools/babel.md) — detailed Babel setup for stripping Flow types
- [ESLint](./tools/eslint.md) — setting up ESLint to work with Flow
- [Flow for TypeScript Users](./flow-vs-typescript.md) — what transfers from TypeScript, what differs, and what each language has that the other doesn't
