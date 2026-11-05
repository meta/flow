# flow-api-translator

`flow-api-translator` enables creating Flow and TypeScript compatible libraries from Flow source code.

## Usage

This package provides a toolkit of different translation tools to support a number of different JavaScript based language targets from Flow source code.

An important distinction with this translator and others is this is not intended as a migration tool away from Flow, instead the expectation is the source code will continue to be checked by Flow but this translator provides the ability to seamlessly integrate with external Flow, TypeScript and JavaScript based projects.

Below are some common use cases:

### Flow type AST -> TS type AST

`translateFlowTypeToTSType` synchronously converts a single
`flow-estree.TypeAnnotationType` into a TypeScript ESTree type node. It does not
parse or print source, mutate the input AST, resolve bindings, or add imports.
For example, an inline Flow interface becomes a TypeScript object type:

```js
const {parse} = require('flow-parser');
const {translateFlowTypeToTSType} = require('flow-api-translator');

const program = parse('type T = interface { foo: number };');
const tsType = translateFlowTypeToTSType(program.body[0].right);
// tsType.type === 'TSTypeLiteral', representing { foo: number }
```

Pass the type node itself, not its surrounding `TypeAnnotation` wrapper. Nested
types are translated recursively using the same rules as the definition-file
translator. Inline interfaces with bases become intersections of those bases
and the translated body. The input must use the `flow-estree` dialect, including
its location and range metadata; the returned node's locations are placeholders,
not positions in generated TypeScript source.

This API always throws for unsupported translations instead of recovering with
`any`, including generic indexer keys that need mapped-type lowering. It also
throws if translation would require adding a React import; use
`translateFlowDefToTSDef` for translations requiring file-level import handling.
Generic names otherwise remain references without resolving their bindings.
Optional `sourceText` enriches errors with a source code frame:

```js
translateFlowTypeToTSType(flowType, {sourceText});
```

### Flow -> Flow Lib

Creating a Flow type definition and runtime files from Flow source code is simple. Just get the source text then pass it to the respective translators:

```
const sourceFile = await fs.readFile('myFile.js', 'utf8');
await fs.writeFile('output/myFile.js.flow', translate.translateFlowToFlowDef(sourceFile, prettierConfig));
await fs.writeFile('output/myFile.js', translate.translateFlowToJS(sourceFile, prettierConfig));
```

### Flow -> TS Lib

Further conversion to TypeScript definition files is supported. To do this, get the source text and pass it to the respective translators:
```
const sourceFile = await fs.readFile('myFile.js', 'utf8');
await fs.writeFile('output/myFile.d.ts', translate.translateFlowToTSDef(sourceFile, prettierConfig));
await fs.writeFile('output/myFile.js', translate.translateFlowToJS(sourceFile, prettierConfig));
```

By default, documentation remains attached to the declaration it originally
described. To associate that documentation with a generated default-export
symbol, pass `defaultExportDocPlacement` to either definition translator:

```
await fs.writeFile(
  'output/myFile.d.ts',
  translate.translateFlowToTSDef(sourceFile, prettierConfig, {
    defaultExportDocPlacement: 'export',
  }),
);
```

The supported placements are `declaration` (the default), `export`, and `both`.
`both` retains the documentation on the original declaration as well as
attaching it to the default export.

### Flow (with Haste modules) -> TS Lib

Haste module paths are commonly used with Flow libraries. TypeScript does not support this so they needs to be converted back to relative paths first:
```
const sourceFile = await fs.readFile('myFile.js', 'utf8');
const mappedSourceFile = translate.translateFlowImportsTo(
  fileContents,
  prettierConfig,
  {sourceMapper: ({module}) => path.relative(currentLocation, ModuleMap.getModulePath(module))},
);
await fs.writeFile('output/myFile.js.flow', translate.translateFlowToTSDef(mappedSourceFile, prettierConfig));
await fs.writeFile('output/myFile.js', translate.translateFlowToJS(mappedSourceFile, prettierConfig));
```
