/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict
 * @format
 */

/**
 * This transform strips all Flow types.
 *
 * It is expected that all transforms create valid ESTree AST output. If
 * the transform requires outputting Babel specific AST nodes then it
 * should live in `ConvertESTreeToBabel.js`
 */

'use strict';

import type {ParserOptions} from '../ParserOptions';
import type {Program, ESNode} from 'flow-estree';

import {SimpleTransform} from '../transform/SimpleTransform';

const nodeWith = SimpleTransform.nodeWith;

export function transformProgram(
  program: Program,
  _options: ParserOptions,
): Program {
  return SimpleTransform.transformProgram(program, {
    transform(node: ESNode) {
      switch (node.type) {
        case 'AsExpression':
        case 'AsConstExpression':
        case 'TypeCastExpression':
        case 'NonNullExpression': {
          return node.expression;
        }

        case 'CallExpression':
        case 'NewExpression':
        case 'TaggedTemplateExpression': {
          if (node.typeArguments != null) {
            return nodeWith(node, {typeArguments: null});
          }
          return node;
        }

        case 'ObjectPattern':
        case 'ArrayPattern':
        case 'Identifier': {
          if (node.typeAnnotation != null || node.optional === true) {
            return nodeWith(node, {typeAnnotation: null, optional: false});
          }
          return node;
        }

        case 'VariableDeclarator': {
          if (node.definite === true) {
            return nodeWith(node, {definite: undefined});
          }
          return node;
        }

        case 'DeclareClass':
        case 'DeclareFunction':
        case 'DeclareInterface':
        case 'DeclareModule':
        case 'DeclareModuleExports':
        case 'DeclareNamespace':
        case 'DeclareOpaqueType':
        case 'DeclareTypeAlias':
        case 'DeclareVariable':
        case 'InterfaceDeclaration':
        case 'OpaqueType':
        case 'TypeAlias': {
          return null;
        }

        case 'FunctionDeclaration':
        case 'ArrowFunctionExpression':
        case 'FunctionExpression': {
          const newParams = [];
          for (let i = 0; i < node.params.length; i++) {
            if (
              i === 0 &&
              node.params[0].type === 'Identifier' &&
              node.params[0].name === 'this'
            ) {
              continue;
            }

            newParams.push(node.params[i]);
          }

          return nodeWith(node, {
            params: newParams,
            returnType: null,
            typeParameters: null,
            predicate: null,
          });
        }

        case 'ClassDeclaration':
        case 'ClassExpression': {
          return nodeWith(node, {
            typeParameters: null,
            superTypeArguments: null,
            implements: [],
            decorators: [],
          });
        }

        case 'PropertyDefinition': {
          return nodeWith(node, {
            typeAnnotation: null,
            variance: null,
            declare: false,
            optional: false,
            definite: undefined,
          });
        }

        case 'ImportDeclaration': {
          if (node.importKind === 'type' || node.importKind === 'typeof') {
            return null;
          }
          const nonTypeSpecifiers = node.specifiers.filter(
            s =>
              s.type !== 'ImportSpecifier' ||
              (s.importKind !== 'type' && s.importKind !== 'typeof'),
          );
          if (nonTypeSpecifiers.length === node.specifiers.length) {
            return node;
          }
          if (nonTypeSpecifiers.length === 0) {
            return null;
          }

          return nodeWith(node, {
            specifiers: nonTypeSpecifiers,
          });
        }

        case 'ExportAllDeclaration':
        case 'ExportDefaultDeclaration':
        case 'ExportNamedDeclaration': {
          if (node.exportKind === 'type') {
            return null;
          }
          return node;
        }

        default: {
          return node;
        }
      }
    },
  });
}
