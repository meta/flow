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
 * This transform strips Flow types that are not supported past Babel 7.
 *
 * It is expected that all transforms create valid ESTree AST output. If
 * the transform requires outputting Babel specific AST nodes then it
 * should live in `ConvertESTreeToBabel.js`
 */

'use strict';

import type {ParserOptions} from '../ParserOptions';
import type {
  Program,
  ESNode,
  DeclareEnum,
  DeclareNamespace,
  DeclareVariable,
  AnyTypeAnnotation,
  GenericTypeAnnotation,
  QualifiedTypeIdentifier,
  QualifiedTypeofIdentifier,
  AFunction,
  BindingName,
  VariableDeclarator,
} from 'flow-estree';

import {SimpleTransform} from '../transform/SimpleTransform';
import {createSyntaxError} from '../utils/createSyntaxError';

const nodeWith = SimpleTransform.nodeWith;

// Rely on the mapper to fix up parent relationships.
const EMPTY_PARENT: $FlowFixMe = null;

function createSimpleGenericTypeAnnotation(
  name: string,
  nodeForLoc: ESNode,
): GenericTypeAnnotation {
  return {
    type: 'GenericTypeAnnotation',
    id: {
      type: 'Identifier',
      name,
      optional: false,
      typeAnnotation: null,
      loc: nodeForLoc.loc,
      range: nodeForLoc.range,
      parent: EMPTY_PARENT,
    },
    typeArguments: null,
    loc: nodeForLoc.loc,
    range: nodeForLoc.range,
    parent: nodeForLoc.parent,
  };
}

function createAnyTypeAnnotation(node: ESNode): AnyTypeAnnotation {
  return {
    type: 'AnyTypeAnnotation',
    loc: node.loc,
    range: node.range,
    parent: node.parent,
  };
}

function declareVariableDeclarator(
  id: BindingName,
  node: ESNode,
): VariableDeclarator {
  return {
    type: 'VariableDeclarator',
    id,
    init: null,
    loc: node.loc,
    range: node.range,
    parent: EMPTY_PARENT,
  };
}

/**
 * Convert DeclareEnum nodes to DeclareVariable
 */
function mapDeclareEnum(node: DeclareEnum): DeclareVariable {
  const id = nodeWith(node.id, {
    typeAnnotation: {
      type: 'TypeAnnotation',
      typeAnnotation: createAnyTypeAnnotation(node.body),
      loc: node.body.loc,
      range: node.body.range,
      parent: EMPTY_PARENT,
    },
  });

  return {
    type: 'DeclareVariable',
    kind: 'const',
    declarations: [declareVariableDeclarator(id, node)],
    loc: node.loc,
    range: node.range,
    parent: node.parent,
  };
}

/**
 * Convert DeclareNamespace nodes to DeclareVariable
 */
function mapDeclareNamespace(node: DeclareNamespace): DeclareVariable {
  const id = nodeWith(node.id, {
    typeAnnotation: {
      type: 'TypeAnnotation',
      typeAnnotation: createAnyTypeAnnotation(node.body),
      loc: node.body.loc,
      range: node.body.range,
      parent: EMPTY_PARENT,
    },
  });

  return {
    type: 'DeclareVariable',
    kind: 'const',
    declarations: [declareVariableDeclarator(id, node)],
    loc: node.loc,
    range: node.range,
    parent: node.parent,
  };
}

/**
 * Remove `this` param from functions.
 */
function mapFunction(node: AFunction): AFunction {
  // Remove the first parameter if it is a this-type annotation,
  // which is not recognized by Babel.
  if (node.params.length !== 0 && node.params[0].name === 'this') {
    return nodeWith(node, {
      params: node.params.slice(1),
    });
  }

  return node;
}

/**
 * Convert to QualifiedTypeIdentifier
 */
function mapQualifiedTypeofIdentifier(
  node: QualifiedTypeofIdentifier,
): QualifiedTypeIdentifier {
  return {
    type: 'QualifiedTypeIdentifier',
    qualification:
      node.qualification.type === 'QualifiedTypeofIdentifier'
        ? mapQualifiedTypeofIdentifier(node.qualification)
        : node.qualification,
    id: node.id,
    loc: node.loc,
    range: node.range,
    parent: node.parent,
  };
}

export function transformProgram(
  program: Program,
  _options: ParserOptions,
): Program {
  return SimpleTransform.transformProgram(program, {
    transform(node: ESNode) {
      switch (node.type) {
        case 'SymbolTypeAnnotation': {
          // Convert to simple generic type annotation
          return createSimpleGenericTypeAnnotation('symbol', node);
        }
        case 'BigIntTypeAnnotation': {
          // Convert to simple generic type annotation
          return createSimpleGenericTypeAnnotation('bigint', node);
        }
        case 'ObjectTypeAnnotation': {
          const shouldStrip = node.properties.some(
            prop => prop.type === 'ObjectTypeMappedTypeProperty',
          );
          if (shouldStrip) {
            return createAnyTypeAnnotation(node);
          }

          return node;
        }
        case 'ObjectTypeMappedTypeProperty': {
          throw createSyntaxError(
            node,
            `Invalid AST structure, ObjectTypeMappedTypeProperty found outside of an ObjectTypeAnnotation`,
          );
        }
        case 'IndexedAccessType':
        case 'OptionalIndexedAccessType':
        case 'KeyofTypeAnnotation':
        case 'ConditionalTypeAnnotation':
        case 'InferTypeAnnotation':
        case 'TupleTypeLabeledElement':
        case 'TupleTypeSpreadElement':
        case 'ComponentTypeAnnotation':
        case 'HookTypeAnnotation':
        case 'TypeOperator':
        case 'TypePredicate': {
          // Babel does not support these generic types, so convert to any
          return createAnyTypeAnnotation(node);
        }
        case 'QualifiedTypeofIdentifier': {
          return mapQualifiedTypeofIdentifier(node);
        }
        case 'DeclareEnum': {
          return mapDeclareEnum(node);
        }
        case 'DeclareNamespace': {
          return mapDeclareNamespace(node);
        }
        case 'FunctionDeclaration':
        case 'FunctionExpression': {
          return mapFunction(node);
        }
        default: {
          return node;
        }
      }
    },
  });
}
