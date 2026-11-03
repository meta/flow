---
title: Prettier
slug: /tools/prettier
description: "How to set up Prettier to format Flow-typed JavaScript code, including support for the latest Flow features."
---

The latest version of [Prettier](https://prettier.io/) supports Flow syntax, including pattern matching, component syntax, and type guards.
Prettier includes [flow-parser](https://www.npmjs.com/package/flow-parser) by default, so no separate parser package or plugin is needed.
See [these instructions](https://prettier.io/docs/install) on how to install Prettier to your project.

To use the built-in `flow` parser instead of Prettier's default `babel` parser for JavaScript files, add this override to your `.prettierrc`:

```json
{
  "overrides": [
    {
      "files": ["*.js", "*.jsx"],
      "options": {"parser": "flow"}
    }
  ]
}
```

## See Also {#toc-see-also}

- [Getting Started](../getting-started.md) — full setup guide for Flow projects
