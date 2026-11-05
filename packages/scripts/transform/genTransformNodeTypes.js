/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict
 * @format
 */

import {
  GetTransformESTreeJSON,
  TransformESTreePackage,
  TransformPackage,
  TransformReadonly,
  formatAndWriteSrcArtifact,
  LITERAL_TYPES,
  EXCLUDE_PROPERTIES_FROM_NODE,
} from './utils/scriptUtils';

const imports: Array<string> = [];
const nodeTypeFunctions: Array<string> = [];
const nodePropTypes: Array<string> = [];

const FIXED_NODE_PROPERTIES: ReadonlyMap<
  string,
  ReadonlyMap<string, string>,
> = new Map([
  [
    'ExportDefaultDeclaration',
    new Map([
      [
        'exportKind',
        "isNode(props.declaration) && props.declaration.type === 'InterfaceDeclaration' ? 'type' : 'value'",
      ],
    ]),
  ],
  ['FunctionDeclaration', new Map([['expression', 'false']])],
  ['FunctionExpression', new Map([['expression', 'false']])],
  [
    'PrivateIdentifier',
    new Map([
      ['typeAnnotation', 'null'],
      ['optional', 'false'],
    ]),
  ],
]);

// These nodes are implemented in flow-transform's special-case node builders.
const NODES_WITH_SPECIAL_HANDLING = new Set([
  'ArrowFunctionExpression',
  'BigIntLiteral',
  'BlockStatement',
  'BooleanLiteral',
  'ClassDeclaration',
  'DeclareExportDeclaration',
  'DeclareFunction',
  'DeclareHook',
  'ExportNamedDeclaration',
  'Identifier',
  'MemberExpression',
  'NullLiteral',
  'NumericLiteral',
  'ObjectTypeProperty',
  'Program',
  'RegExpLiteral',
  'StringLiteral',
  'TemplateElement',
]);

for (const node of GetTransformESTreeJSON()) {
  if (NODES_WITH_SPECIAL_HANDLING.has(node.name)) {
    continue;
  }

  imports.push(node.name);

  const type = LITERAL_TYPES.has(node.name) ? 'Literal' : node.name;

  if (node.arguments.length === 0) {
    nodePropTypes.push(`\
export type ${node.name}Props = {};
`);
    nodeTypeFunctions.push(
      `\
export function ${node.name}(props: {
  ${TransformReadonly}parent?: ESNode,
} = {...null}): DetachedNode<${node.name}Type> {
  return detachedProps<${node.name}Type>(props.parent as $FlowFixMe, {
    type: '${type}',
  });
}
`,
    );
  } else {
    nodePropTypes.push(
      `\
export type ${node.name}Props = {
  ${node.arguments
    .map(arg => {
      if (
        EXCLUDE_PROPERTIES_FROM_NODE.get(node.name)?.has(arg.name) ||
        FIXED_NODE_PROPERTIES.get(node.name)?.has(arg.name)
      ) {
        return null;
      }
      const baseType = `${node.name}Type['${arg.name}']`;
      let type = baseType;
      if (arg.type === 'NodePtr') {
        type = `MaybeDetachedNode<${type}>`;
      } else if (arg.type === 'NodeList') {
        type = `ReadonlyArray<MaybeDetachedNode<${type}[number]>>`;
      }

      if (arg.optional) {
        return `${TransformReadonly}${arg.name}?: ?${type}`;
      }
      return `${TransformReadonly}${arg.name}: ${type}`;
    })
    .filter(Boolean)
    .join(',\n')},
};
`,
    );
    nodeTypeFunctions.push(
      `\
export function ${node.name}(props: {
  ...${node.name}Props,
  ${TransformReadonly}parent?: ESNode,
}): DetachedNode<${node.name}Type> {
  const node = detachedProps<${node.name}Type>(props.parent as $FlowFixMe, {
    type: '${type}',
    ${node.arguments
      .map(arg => {
        if (EXCLUDE_PROPERTIES_FROM_NODE.get(node.name)?.has(arg.name)) {
          return null;
        }
        const fixedValue = FIXED_NODE_PROPERTIES.get(node.name)?.get(arg.name);
        if (fixedValue != null) {
          return `${arg.name}: ${fixedValue}`;
        }
        switch (arg.type) {
          case 'NodePtr':
            return `${arg.name}: asDetachedNodeForCodeGen(props.${arg.name})`;
          case 'NodeList':
            return `${arg.name}: props.${arg.name}${
              arg.optional ? '?.' : '.'
            }map(n => asDetachedNodeForCodeGen(n))`;
          default:
            return `${arg.name}: props.${arg.name}`;
        }
      })
      .filter(Boolean)
      .join(',\n')},
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}
`,
    );
  }
}

const fileContents = `\
import type {
ESNode,
${imports.map(imp => `${imp} as ${imp}Type`).join(',\n')}
} from '${TransformESTreePackage}';
import type {DetachedNode, MaybeDetachedNode} from '../detachedNode';

import {isNode} from 'flow-parser';
import {
  asDetachedNodeForCodeGen,
  detachedProps,
  setParentPointersInDirectChildren,
} from '../detachedNode';

${nodePropTypes.join('\n')}

${nodeTypeFunctions.join('\n')}

export * from './special-case-node-types';
`;

formatAndWriteSrcArtifact({
  code: fileContents,
  package: TransformPackage,
  file: 'generated/node-types.js',
  flow: 'strict-local',
});
