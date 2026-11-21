---
title: Linting Overview
slug: /linting
description: Learn how to configure Flow's linter to find potentially harmful code.
---

Flow contains a linting framework that can tell you about more than just type errors. This framework is highly configurable in order to show you the information you want and hide the information you don't.

### Configuring Lints in the `.flowconfig` {#toc-configuring-lints-in-the-flowconfig}

Lint settings can be specified in the `[lints]` section of the `.flowconfig` as a list of `rule=severity` pairs. These settings apply globally to the entire project.

```
[lints]
all=warn
unused-promise=error
sketchy-null-bool=off
```

### Configuring Lints from the CLI {#toc-configuring-lints-from-the-cli}

Lint settings can be specified using the `--lints` flag of a Flow server command as a comma-delimited list of `rule=severity` pairs. These settings apply globally to the entire project.

```
flow start --lints "all=warn, unused-promise=error, sketchy-null-bool=off"
```

### Configuring Lints with Comments {#toc-configuring-lints-with-comments}

Lint settings can be specified inside a file using `flowlint` comments. These
settings apply to a region of a file, or a single line, or part of a line. For
more details see [Flowlint Comments](./flowlint-comments.md).

```js flow-check
// flowlint sketchy-null:error
const x: ?number = 0;

if (x) {} // Error

// flowlint-next-line sketchy-null:off
if (x) {} // No Error

if (x) {} /* flowlint-line sketchy-null:off */ // No Error

// flowlint sketchy-null:off
if (x) {} // No Error
if (x) {} // No Error
```

### Lint Settings Precedence {#toc-lint-settings-precedence}

Lint settings in `flowlint` comments have the highest priority, followed by lint rules in the `--lints` flag, followed by the `.flowconfig`.
This order allows you to use `flowlint` comments for fine-grained linting control, the `--lints` flag for trying out new lint settings, and the `.flowconfig` for stable project-wide settings.

Within the `--lints` flag and the `.flowconfig`, rules lower down override rules higher up, allowing you to write things like
```
[lints]
# warn on all sketchy-null checks
sketchy-null=warn
# ... except for booleans
sketchy-null-bool=off
```

Flow reports redundant or overwritten lint rules and unused `flowlint` suppressions.

### Severity Levels and Meanings {#toc-severity-levels-and-meanings}

**off:**
The lint is ignored. Setting a lint to `off` is similar to suppressing a type error with a suppression comment, except with much more granularity.

**warn:**
Warnings are a severity level introduced by the linting framework. They are treated differently than errors in a couple of ways:
* Warnings don't affect the exit code of Flow. If Flow finds warnings but no errors, it still returns 0.
* Warnings aren't shown on the CLI by default, to avoid spew. CLI warnings can be
    enabled by passing the `--include-warnings` flag to the Flow server or the
    Flow client, or by setting `include_warnings=true` in the `.flowconfig`.
    This is good for smaller projects that want to see all project warnings at once.

**error:**
Lints with severity `error` are treated exactly the same as any other Flow error.

## See Also {#toc-see-also}

- [`.flowconfig [lints]`](../config/lints.md) — configuring lint severities in your `.flowconfig`
- [Flow Strict](../strict/index.md) — enabling stricter type checking with `@flow strict`
- [ESLint](../tools/eslint.md) — using ESLint alongside Flow for additional linting
