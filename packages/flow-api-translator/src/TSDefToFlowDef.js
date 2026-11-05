/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

'use strict';

import type {ObjectWithLoc} from 'flow-estree';
import type {TranslationOptions} from './utils/TranslationUtils';

import * as FlowESTree from 'flow-estree';
import * as TSESTree from './utils/ts-estree-ast-types';
import {makeCommentOwnLine as makeCommentOwnLineOriginal} from 'flow-transform';
import {
  buildCodeFrame,
  translationError as translationErrorBase,
  unexpectedTranslationError as unexpectedTranslationErrorBase,
} from './utils/ErrorUtils';
import {EOL} from 'os';

const DUMMY_LOC: FlowESTree.SourceLocation = null as $FlowFixMe;
const DUMMY_RANGE: [number, number] = [0, 0];
const DUMMY_PARENT: $FlowFixMe = null;
const DUMMY_COMMON = {
  loc: DUMMY_LOC,
  range: DUMMY_RANGE,
  parent: DUMMY_PARENT,
};

type LooseOmit<O extends interface {}, K extends keyof $FlowFixMe> = Pick<
  O,
  Exclude<keyof O, K>,
>;
function constructFlowNode<T extends FlowESTree.BaseNode>(
  node: LooseOmit<NoInfer<T>, 'parent'>,
): T {
  return {
    ...(node as $FlowFixMe),
    ...DUMMY_COMMON,
  };
}

const makeCommentOwnLine =
  // $FlowExpectedError[incompatible-type] - trust me this re-type is 100% safe
  makeCommentOwnLineOriginal as (string, unknown) => string;

export function TSDefToFlowDef(
  originalCode: string,
  ast: TSESTree.Program,
  opts: TranslationOptions,
): [FlowESTree.Program, string] {
  const flowBody: Array<FlowESTree.Statement | FlowESTree.ModuleDeclaration> =
    [];
  const flowProgram: FlowESTree.Program = {
    ...DUMMY_COMMON,
    type: 'Program',
    body: flowBody,
    comments: [],
    sourceType: ast.sourceType,
    interpreter: null,
    tokens: [],
    loc: ast.loc,
    docblock: null,
  };

  const [transform, code] = getTransforms(originalCode, opts);

  for (const node of ast.body) {
    const result = transform.AllStatement(node);
    flowBody.push(...(Array.isArray(result) ? result : [result]));
  }

  return [flowProgram, code];
}

// Note: The implementation here is still in early stage. If something is not supported, it doesn't
// necessarily mean that it cannot be. It might just mean that it's not priortized yet. If something
// is translated in way that is wrong, then it's likely wrong.
const getTransforms = (originalCode: string, opts: TranslationOptions) => {
  let code = originalCode;
  function translationError(node: ObjectWithLoc, message: string) {
    return translationErrorBase(node, message, {code});
  }
  function unexpectedTranslationError(node: ObjectWithLoc, message: string) {
    return unexpectedTranslationErrorBase(node, message, {code});
  }
  function unsupportedFeatureMessage(thing: string) {
    return `Unsupported feature: Translating "${thing}" is currently not supported.`;
  }
  function buildCodeFrameForComment(node: ObjectWithLoc, message: string) {
    return buildCodeFrame(node, message, code, false);
  }
  function addErrorComment(node: FlowESTree.ESNode, message: string): void {
    const comment: TSESTree.Comment = {
      type: 'Block',
      loc: DUMMY_LOC,
      value: `*${EOL} * ${message.replace(
        new RegExp(EOL, 'g'),
        `${EOL} * `,
      )}${EOL}*`,
      leading: true,
      printed: false,
    };

    code = makeCommentOwnLine(code, comment);

    // $FlowExpectedError[prop-missing]
    // $FlowExpectedError[cannot-write]
    node.comments ??= [];
    // $FlowExpectedError[incompatible-type]
    (node.comments as Array<TSESTree.Comment>).push(comment);
  }
  function unsupportedAnnotation(
    node: ObjectWithLoc,
    thing: string,
  ): FlowESTree.AnyTypeAnnotation {
    const message = unsupportedFeatureMessage(thing);
    if (opts.recoverFromErrors) {
      const codeFrame = buildCodeFrameForComment(node, message);
      const newNode: FlowESTree.AnyTypeAnnotation = {
        ...DUMMY_COMMON,
        type: 'AnyTypeAnnotation',
      };
      addErrorComment(newNode, codeFrame);
      return newNode;
    }

    throw translationError(node, message);
  }
  function unsupportedDeclaration(
    node: ObjectWithLoc,
    thing: string,
    id: TSESTree.Identifier,
    typeParameters: TSESTree.TSTypeParameterDeclaration | null = null,
  ): FlowESTree.TypeAlias {
    const message = unsupportedFeatureMessage(thing);
    if (opts.recoverFromErrors) {
      const codeFrame = buildCodeFrameForComment(node, message);
      const newNode: FlowESTree.TypeAlias = {
        ...DUMMY_COMMON,
        type: 'TypeAlias',
        id: Transform.Identifier(id, false),
        right: {
          ...DUMMY_COMMON,
          type: 'AnyTypeAnnotation',
        },
        typeParameters: Transform.TSTypeParameterDeclarationOpt(typeParameters),
      };
      addErrorComment(newNode, codeFrame);
      return newNode;
    }

    throw translationError(node, message);
  }

  class Transform {
    static BlockStatement(
      node: TSESTree.BlockStatement,
    ): FlowESTree.BlockStatement {
      return constructFlowNode<FlowESTree.BlockStatement>({
        type: 'BlockStatement',
        body: node.body.flatMap(node => Transform.Statement(node)),
      });
    }

    static ClassDeclarationWithName(
      node: TSESTree.ClassDeclarationWithName,
    ): FlowESTree.DeclareClass {
      return constructFlowNode<FlowESTree.DeclareClass>({
        type: 'DeclareClass',
        id: Transform.Identifier(node.id),
        typeParameters: Transform.TSTypeParameterDeclarationOpt(
          node.typeParameters,
        ),
        implements: (node.implements || []).map(impl =>
          Transform.ClassImplements(impl),
        ),
        extends: Transform.ClassDeclarationSuperClass(
          node.superClass,
          node.superTypeArguments,
        ),
        mixins: [],
        body: Transform.ClassDeclarationBody(node.body),
      });
    }

    static ClassDeclarationBody({
      body,
    }: TSESTree.ClassBody): FlowESTree.ObjectTypeAnnotation {
      const properties: Array<FlowESTree.ObjectTypeProperty> = [];
      const indexers: Array<FlowESTree.ObjectTypeIndexer> = [];
      for (const classItem of body) {
        switch (classItem.type) {
          case 'StaticBlock':
            break;
          case 'TSIndexSignature':
            break;
          case 'TSAbstractPropertyDefinition':
          case 'PropertyDefinition':
            // $FlowFixMe[incompatible-type] ambiguous node
            Transform._translateIntoObjectProp(classItem, properties, indexers);
            break;
          case 'MethodDefinition':
          case 'TSAbstractMethodDefinition':
            // $FlowFixMe[incompatible-type] ambiguous node
            Transform._translateIntoObjectMethod(classItem, properties);
            break;
        }
      }
      return constructFlowNode<FlowESTree.ObjectTypeAnnotation>({
        type: 'ObjectTypeAnnotation',
        properties,
        indexers,
        callProperties: [],
        internalSlots: [],
        exact: false,
        inexact: false,
      });
    }

    static ClassDeclarationSuperClass(
      superClass: ?TSESTree.LeftHandSideExpression,
      superTypeArguments: ?TSESTree.TSTypeParameterInstantiation,
    ): [FlowESTree.InterfaceExtends] | [] {
      if (superClass == null) {
        return [];
      }
      const id = Transform._expressionToIdOrQualifiedTypeId(
        superClass,
        'superClass',
      );
      return [
        constructFlowNode<FlowESTree.InterfaceExtends>({
          type: 'InterfaceExtends',
          id,
          typeArguments:
            Transform.TSTypeParameterInstantiationOpt(superTypeArguments),
        }),
      ];
    }

    static ClassImplements(
      node: TSESTree.TSClassImplements,
    ): FlowESTree.ClassImplements {
      if (node.expression.type !== 'Identifier') {
        throw unexpectedTranslationError(
          node,
          'Expected expression to be an Identifier',
        );
      }
      return constructFlowNode<FlowESTree.ClassImplements>({
        type: 'ClassImplements',
        id: Transform.Identifier(node.expression),
        typeArguments: Transform.TSTypeParameterInstantiationOpt(
          node.typeArguments,
        ),
      });
    }

    static DebuggerStatement(): FlowESTree.DebuggerStatement {
      return constructFlowNode<FlowESTree.DebuggerStatement>({
        type: 'DebuggerStatement',
      });
    }

    static EntityNameToTypeIdentifier(
      node: TSESTree.EntityName,
    ): FlowESTree.Identifier | FlowESTree.QualifiedTypeIdentifier {
      switch (node.type) {
        case 'Identifier':
          return Transform.Identifier(node);
        case 'TSQualifiedName':
          return Transform.TSQualifiedNameToQualifiedTypeIdentifier(node);
        case 'ThisExpression':
          return constructFlowNode<FlowESTree.Identifier>({
            type: 'Identifier',
            name: 'this',
            typeAnnotation: null,
            optional: false,
          });
      }
    }

    static EntityNameToTypeofIdentifier(
      node: TSESTree.EntityName,
    ): FlowESTree.Identifier | FlowESTree.QualifiedTypeofIdentifier {
      switch (node.type) {
        case 'Identifier':
          return Transform.Identifier(node);
        case 'TSQualifiedName':
          return Transform.TSQualifiedNameToQualifiedTypeofIdentifier(node);
        case 'ThisExpression':
          return constructFlowNode<FlowESTree.Identifier>({
            type: 'Identifier',
            name: 'this',
            typeAnnotation: null,
            optional: false,
          });
      }
    }

    static ExportAllDeclaration(
      node: TSESTree.ExportAllDeclaration,
    ): FlowESTree.ExportAllDeclaration {
      return constructFlowNode<FlowESTree.ExportAllDeclaration>({
        type: 'ExportAllDeclaration',
        source: constructFlowNode<FlowESTree.StringLiteral>({
          type: 'Literal',
          literalType: 'string',
          value: node.source.value,
          raw: node.source.raw,
        }),
        assertions: [],
        exportKind: node.exportKind,
        exported:
          node.exported != null ? Transform.Identifier(node.exported) : null,
      });
    }

    static ExportDefaultDeclaration(
      node: TSESTree.ExportDefaultDeclaration,
    ): FlowESTree.DeclareExportDefaultDeclaration {
      let declaration: FlowESTree.DeclareExportDefaultDeclaration['declaration'];
      switch (node.declaration.type) {
        case 'ClassDeclaration':
          declaration = Transform.ClassDeclarationWithName(
            // possibly missing id
            node.declaration as $FlowFixMe,
          );
          break;
        case 'FunctionDeclaration':
          declaration = Transform.FunctionDeclarationWithName(
            // possibly missing id
            node.declaration as $FlowFixMe,
          );
          break;
        case 'TSDeclareFunction':
          declaration = Transform.TSDeclareFunction(node.declaration);
          break;
        case 'Identifier':
          declaration = constructFlowNode<FlowESTree.TypeofTypeAnnotation>({
            type: 'TypeofTypeAnnotation',
            argument: Transform.Identifier(node.declaration),
          });
          break;
        default:
          throw translationError(
            node.declaration,
            `Unsupported export declaration: ${node.declaration.type}`,
          );
      }
      return constructFlowNode<FlowESTree.DeclareExportDefaultDeclaration>({
        type: 'DeclareExportDeclaration',
        declaration,
        default: true,
        source: null,
        specifiers: [],
      });
    }

    static ExportNamedDeclaration(
      node: TSESTree.ExportNamedDeclaration,
    ):
      | FlowESTree.ExportNamedDeclaration
      | FlowESTree.DeclareExportDeclarationNamedWithDeclaration
      | FlowESTree.DeclareExportDeclarationNamedWithSpecifiers
      | ReadonlyArray<
          | FlowESTree.DeclareExportDeclarationNamedWithDeclaration
          | FlowESTree.TypeAlias
          | FlowESTree.DeclareNamespace
          | FlowESTree.DeclareModule
          | FlowESTree.ExportNamedDeclaration,
        > {
      if (node.declaration == null) {
        const source: FlowESTree.StringLiteral =
          node.source == null
            ? (null as $FlowFixMe)
            : constructFlowNode<FlowESTree.StringLiteral>({
                type: 'Literal',
                literalType: 'string',
                value: node.source.value,
                raw: node.source.raw,
              });
        const specifiers = node.specifiers.map(specifier =>
          constructFlowNode<FlowESTree.ExportSpecifier>({
            type: 'ExportSpecifier',
            local: Transform.Identifier(specifier.local),
            exported: Transform.Identifier(specifier.exported),
            exportKind: specifier.exportKind ?? 'value',
          }),
        );
        return constructFlowNode<FlowESTree.DeclareExportDeclarationNamedWithSpecifiers>(
          {
            type: 'DeclareExportDeclaration',
            declaration: null,
            default: false,
            source,
            specifiers,
          },
        );
      }
      switch (node.declaration.type) {
        case 'ClassDeclaration':
          return constructFlowNode<FlowESTree.DeclareExportDeclarationNamedWithDeclaration>(
            {
              type: 'DeclareExportDeclaration',
              declaration: Transform.ClassDeclarationWithName(
                // possibly missing id
                node.declaration as $FlowFixMe,
              ),
              default: false,
              source: null,
              specifiers: [],
            },
          );
        case 'FunctionDeclaration':
          return constructFlowNode<FlowESTree.DeclareExportDeclarationNamedWithDeclaration>(
            {
              type: 'DeclareExportDeclaration',
              declaration: Transform.FunctionDeclarationWithName(
                // possibly missing id
                node.declaration as $FlowFixMe,
              ),
              default: false,
              source: null,
              specifiers: [],
            },
          );
        case 'TSDeclareFunction':
          return constructFlowNode<FlowESTree.DeclareExportDeclarationNamedWithDeclaration>(
            {
              type: 'DeclareExportDeclaration',
              declaration: Transform.TSDeclareFunction(node.declaration),
              default: false,
              source: null,
              specifiers: [],
            },
          );
        case 'TSEnumDeclaration':
          throw translationError(
            node.declaration,
            `Unsupported export declaration: ${node.declaration.type}`,
          );
        case 'TSModuleDeclaration': {
          const decl = Transform.TSModuleDeclaration(node.declaration);
          if (decl.id.type !== 'Identifier') {
            throw translationError(
              decl.id,
              `Unsupported module declaration id`,
            );
          }
          return [
            decl,
            constructFlowNode<FlowESTree.ExportNamedDeclaration>({
              type: 'ExportNamedDeclaration',
              declaration: null,
              source: null,
              exportKind: 'value',
              specifiers: [
                constructFlowNode<FlowESTree.ExportSpecifier>({
                  type: 'ExportSpecifier',
                  local: decl.id,
                  exported: decl.id,
                  exportKind: 'value',
                }),
              ],
            }),
          ];
        }
        case 'TSInterfaceDeclaration': {
          const decl = Transform.TSInterfaceDeclaration(node.declaration);
          return constructFlowNode<FlowESTree.DeclareExportDeclarationNamedWithDeclaration>(
            {
              type: 'DeclareExportDeclaration',
              declaration: constructFlowNode<FlowESTree.DeclareInterface>({
                type: 'DeclareInterface',
                id: decl.id,
                typeParameters: decl.typeParameters,
                body: decl.body,
                extends: decl.extends,
              }),
              default: false,
              source: null,
              specifiers: [],
            },
          );
        }
        case 'TSTypeAliasDeclaration': {
          const decl = Transform.TSTypeAliasDeclaration(node.declaration);
          return constructFlowNode<FlowESTree.ExportNamedDeclaration>({
            type: 'ExportNamedDeclaration',
            declaration: decl,
            source: null,
            exportKind: 'type',
            specifiers: [],
          });
        }
        case 'VariableDeclaration': {
          return Transform.VariableDeclaration(node.declaration).map(
            declaration =>
              constructFlowNode<FlowESTree.DeclareExportDeclarationNamedWithDeclaration>(
                {
                  type: 'DeclareExportDeclaration',
                  declaration,
                  default: false,
                  source: null,
                  specifiers: [],
                },
              ),
          );
        }
      }
    }

    static FunctionDeclarationWithName(
      node: TSESTree.FunctionDeclarationWithName,
    ): FlowESTree.DeclareFunction {
      const {thisParam, restParam, params} =
        Transform._partitionAndTranslateTSFunctionParams(node.params);
      const fnAnnot = constructFlowNode<FlowESTree.FunctionTypeAnnotation>({
        type: 'FunctionTypeAnnotation',
        typeParameters: Transform.TSTypeParameterDeclarationOpt(
          node.typeParameters,
        ),
        params,
        rest: restParam,
        returnType:
          node.returnType?.typeAnnotation == null
            ? unsupportedAnnotation(node, 'missing return type')
            : Transform.TSTypeAnnotation(node.returnType.typeAnnotation),
        this: thisParam,
      });
      return constructFlowNode<FlowESTree.DeclareFunction>({
        type: 'DeclareFunction',
        id: {
          ...DUMMY_COMMON,
          type: 'Identifier',
          name: node.id.name,
          typeAnnotation: {
            ...DUMMY_COMMON,
            type: 'TypeAnnotation',
            typeAnnotation: fnAnnot,
          },
          optional: false,
        },
        predicate: null,
      });
    }

    static Identifier(
      node: TSESTree.Identifier,
      optional?: boolean,
    ): FlowESTree.Identifier {
      return constructFlowNode<FlowESTree.Identifier>({
        type: 'Identifier',
        name: node.name,
        typeAnnotation:
          node.typeAnnotation != null
            ? Transform.TSTypeAnnotationNode(node.typeAnnotation)
            : null,
        optional: Boolean(optional ?? node.optional),
      });
    }

    static ImportDeclaration(
      node: TSESTree.ImportDeclaration,
    ): FlowESTree.ImportDeclaration {
      const specifiers = node.specifiers.map(specifier => {
        switch (specifier.type) {
          case 'ImportNamespaceSpecifier':
            return constructFlowNode<FlowESTree.ImportNamespaceSpecifier>({
              type: 'ImportNamespaceSpecifier',
              local: Transform.Identifier(specifier.local),
            });
          case 'ImportDefaultSpecifier':
            return constructFlowNode<FlowESTree.ImportDefaultSpecifier>({
              type: 'ImportDefaultSpecifier',
              local: Transform.Identifier(specifier.local),
            });
          case 'ImportSpecifier':
            return constructFlowNode<FlowESTree.ImportSpecifier>({
              type: 'ImportSpecifier',
              local: Transform.Identifier(specifier.local),
              imported: Transform.Identifier(specifier.imported),
              importKind:
                specifier.importKind === 'value'
                  ? null
                  : (specifier.importKind ?? null),
            });
        }
      });
      return constructFlowNode<FlowESTree.ImportDeclaration>({
        type: 'ImportDeclaration',
        source: constructFlowNode<FlowESTree.StringLiteral>({
          type: 'Literal',
          literalType: 'string',
          value: node.source.value,
          raw: node.source.raw,
        }),
        importKind:
          // `import type React from 'react'` in TS means `import typeof React from react` in Flow
          specifiers.some(s => s.type === 'ImportDefaultSpecifier') &&
          node.importKind === 'type'
            ? 'typeof'
            : node.importKind,
        attributes: [],
        phase: null,
        specifiers,
      });
    }

    static LabeledStatement(
      node: TSESTree.LabeledStatement,
    ): FlowESTree.LabeledStatement {
      const body = Transform.Statement(node.body);
      if (Array.isArray(body)) {
        throw translationError(node.body, 'Unexpected array of statements');
      }
      return constructFlowNode<FlowESTree.LabeledStatement>({
        type: 'LabeledStatement',
        label: Transform.Identifier(node.label),
        body,
      });
    }

    static Literal(node: TSESTree.Literal): FlowESTree.Literal {
      if (typeof node.value === 'number') {
        return constructFlowNode<FlowESTree.NumericLiteral>({
          type: 'Literal',
          literalType: 'numeric',
          value: Number(node.raw),
          raw: node.raw,
        });
      } else if (typeof node.value === 'string') {
        return constructFlowNode<FlowESTree.StringLiteral>({
          type: 'Literal',
          literalType: 'string',
          value: node.value,
          raw: node.raw,
        });
      } else if (typeof node.value === 'boolean') {
        return constructFlowNode<FlowESTree.BooleanLiteral>({
          type: 'Literal',
          literalType: 'boolean',
          value: node.value,
          raw: node.value ? 'true' : 'false',
        });
      } else if (typeof node.value === 'bigint') {
        return constructFlowNode<FlowESTree.BigIntLiteral>({
          type: 'Literal',
          literalType: 'bigint',
          value: node.value,
          raw: node.raw,
          bigint: node.raw,
        });
      } else if (node.value instanceof RegExp) {
        return constructFlowNode<FlowESTree.RegExpLiteral>({
          type: 'Literal',
          literalType: 'regexp',
          value: node.value,
          regex: null as $FlowFixMe,
          raw: node.raw,
        });
      } else if (node.value == null) {
        return constructFlowNode<FlowESTree.NullLiteral>({
          type: 'Literal',
          literalType: 'null',
          value: node.value,
          raw: 'null',
        });
      } else {
        throw translationError(
          node,
          `Unsupported literal type ${typeof node.value}`,
        );
      }
    }

    static LiteralType(node: TSESTree.Literal): FlowESTree.TypeAnnotationType {
      const literal = Transform.Literal(node);
      switch (literal.literalType) {
        case 'boolean':
          return constructFlowNode<FlowESTree.BooleanLiteralTypeAnnotation>({
            type: 'BooleanLiteralTypeAnnotation',
            value: literal.value,
            raw: literal.raw,
          });
        case 'numeric':
          return constructFlowNode<FlowESTree.NumberLiteralTypeAnnotation>({
            type: 'NumberLiteralTypeAnnotation',
            value: literal.value,
            raw: literal.raw,
          });
        case 'string':
          return constructFlowNode<FlowESTree.StringLiteralTypeAnnotation>({
            type: 'StringLiteralTypeAnnotation',
            value: literal.value,
            raw: literal.raw,
          });
        case 'bigint':
          return constructFlowNode<FlowESTree.BigIntLiteralTypeAnnotation>({
            type: 'BigIntLiteralTypeAnnotation',
            value: literal.value,
            bigint: literal.bigint,
            raw: literal.raw,
          });
        case 'null':
          return constructFlowNode<FlowESTree.NullLiteralTypeAnnotation>({
            type: 'NullLiteralTypeAnnotation',
            value: literal.value,
            raw: literal.raw,
          });
        case 'regexp':
          return unsupportedAnnotation(node, 'regexp literal type');
        default:
          literal as empty;
          throw 'unreachable';
      }
    }

    static AllStatement(
      node: TSESTree.Statement,
    ):
      | FlowESTree.Statement
      | FlowESTree.ModuleDeclaration
      | ReadonlyArray<FlowESTree.Statement | FlowESTree.ModuleDeclaration> {
      switch (node.type) {
        case 'BlockStatement':
          return Transform.BlockStatement(node);
        case 'ClassDeclaration':
          return Transform.ClassDeclarationWithName(node);
        case 'DebuggerStatement':
          return Transform.DebuggerStatement();
        case 'ExportAllDeclaration':
          return Transform.ExportAllDeclaration(node);
        case 'ExportDefaultDeclaration':
          return Transform.ExportDefaultDeclaration(node);
        case 'ExportNamedDeclaration':
          return Transform.ExportNamedDeclaration(node);
        case 'FunctionDeclaration':
          return Transform.FunctionDeclarationWithName(node);
        case 'ImportDeclaration':
          return Transform.ImportDeclaration(node);
        case 'LabeledStatement':
          return Transform.LabeledStatement(node);
        case 'TSDeclareFunction':
          return Transform.TSDeclareFunction(node);
        case 'TSEnumDeclaration':
          return Transform.TSEnumDeclaration(node);
        case 'TSExportAssignment':
          return Transform.TSExportAssignment(node);
        case 'TSImportEqualsDeclaration':
          return Transform.TSImportEqualsDeclaration(node);
        case 'TSInterfaceDeclaration':
          return Transform.TSInterfaceDeclaration(node);
        case 'TSModuleDeclaration':
          return Transform.TSModuleDeclaration(node);
        case 'TSNamespaceExportDeclaration':
          // Flow will never support `export as namespace` since we can't allow a normal file to
          // introduce a global out of nowhere, and because it's only useful for legacy module
          // system However, it's very reasonable to completely ignore it under some mode, so that
          // people using these libdefs won't have a lot of pain.
          return [];
        case 'TSTypeAliasDeclaration':
          return Transform.TSTypeAliasDeclaration(node);
        case 'VariableDeclaration':
          return Transform.VariableDeclaration(node);
        case 'ExpressionStatement':
          throw translationError(node, 'Unsupported expression statement');
        case 'WithStatement':
          throw translationError(node, 'Unsupported with statement');
        case 'BreakStatement':
        case 'ContinueStatement':
        case 'DoWhileStatement':
        case 'ForInStatement':
        case 'ForOfStatement':
        case 'ForStatement':
        case 'IfStatement':
        case 'ReturnStatement':
        case 'SwitchStatement':
        case 'ThrowStatement':
        case 'TryStatement':
        case 'WhileStatement':
          throw translationError(node, 'Unsupported control flow statement');
      }
    }

    static Statement(
      node: TSESTree.Statement,
    ): FlowESTree.Statement | ReadonlyArray<FlowESTree.Statement> {
      return Transform.AllStatement(node) as $FlowFixMe;
    }

    static TSAnyType(): FlowESTree.AnyTypeAnnotation {
      return constructFlowNode<FlowESTree.AnyTypeAnnotation>({
        type: 'AnyTypeAnnotation',
      });
    }

    static TSArrayType(
      node: TSESTree.TSArrayType,
    ): FlowESTree.ArrayTypeAnnotation {
      return constructFlowNode<FlowESTree.ArrayTypeAnnotation>({
        type: 'ArrayTypeAnnotation',
        elementType: Transform.TSTypeAnnotation(node.elementType),
      });
    }

    static TSBigIntType(): FlowESTree.BigIntTypeAnnotation {
      return constructFlowNode<FlowESTree.BigIntTypeAnnotation>({
        type: 'BigIntTypeAnnotation',
      });
    }

    static TSBooleanType(): FlowESTree.BooleanTypeAnnotation {
      return constructFlowNode<FlowESTree.BooleanTypeAnnotation>({
        type: 'BooleanTypeAnnotation',
      });
    }

    static TSConditionalType(
      node: TSESTree.TSConditionalType,
    ): FlowESTree.ConditionalTypeAnnotation {
      return constructFlowNode<FlowESTree.ConditionalTypeAnnotation>({
        type: 'ConditionalTypeAnnotation',
        checkType: Transform.TSTypeAnnotation(node.checkType),
        extendsType: Transform.TSTypeAnnotation(node.extendsType),
        trueType: Transform.TSTypeAnnotation(node.trueType),
        falseType: Transform.TSTypeAnnotation(node.falseType),
      });
    }

    static TSConstructorType(
      node: TSESTree.TSConstructorType,
    ): FlowESTree.AnyTypeAnnotation {
      return unsupportedAnnotation(node, 'constructor types');
    }

    static TSDeclareFunction(
      node: TSESTree.TSDeclareFunction,
    ): FlowESTree.DeclareFunction {
      if (node.id == null) {
        throw translationError(node, 'Missing function name');
      }
      const name = node.id.name;
      const {thisParam, restParam, params} =
        Transform._partitionAndTranslateTSFunctionParams(node.params);
      const fnAnnot = constructFlowNode<FlowESTree.FunctionTypeAnnotation>({
        type: 'FunctionTypeAnnotation',
        typeParameters: Transform.TSTypeParameterDeclarationOpt(
          node.typeParameters,
        ),
        params,
        rest: restParam,
        returnType:
          node.returnType?.typeAnnotation == null
            ? unsupportedAnnotation(node, 'missing return type')
            : Transform.TSTypeAnnotation(node.returnType.typeAnnotation),
        this: thisParam,
      });
      return constructFlowNode<FlowESTree.DeclareFunction>({
        type: 'DeclareFunction',
        id: {
          ...DUMMY_COMMON,
          type: 'Identifier',
          name,
          typeAnnotation: {
            ...DUMMY_COMMON,
            type: 'TypeAnnotation',
            typeAnnotation: fnAnnot,
          },
          optional: false,
        },
        predicate: null,
      });
    }

    static TSEnumDeclaration(
      node: TSESTree.TSEnumDeclaration,
    ): FlowESTree.TypeAlias {
      return unsupportedDeclaration(node, 'enums', node.id);
    }

    static TSExportAssignment(
      node: TSESTree.TSExportAssignment,
    ): FlowESTree.DeclareModuleExports {
      let typeAnnotation: FlowESTree.TypeAnnotationType;
      if (node.expression.type === 'Identifier') {
        typeAnnotation = constructFlowNode<FlowESTree.TypeofTypeAnnotation>({
          type: 'TypeofTypeAnnotation',
          argument: Transform.Identifier(node.expression),
        });
      } else if (node.expression.type === 'Literal') {
        typeAnnotation = Transform.LiteralType(node.expression);
      } else {
        throw translationError(
          node,
          `Unsupported export assignment expression ${node.expression.type}`,
        );
      }
      return constructFlowNode<FlowESTree.DeclareModuleExports>({
        type: 'DeclareModuleExports',
        typeAnnotation: constructFlowNode<FlowESTree.TypeAnnotation>({
          type: 'TypeAnnotation',
          typeAnnotation,
        }),
      });
    }

    static TSFunctionType(
      node: TSESTree.TSFunctionType,
      allowMissingReturn: boolean = false,
    ): FlowESTree.FunctionTypeAnnotation {
      const {thisParam, restParam, params} =
        Transform._partitionAndTranslateTSFunctionParams(node.params);
      return constructFlowNode<FlowESTree.FunctionTypeAnnotation>({
        type: 'FunctionTypeAnnotation',
        typeParameters: Transform.TSTypeParameterDeclarationOpt(
          node.typeParameters,
        ),
        params: params,
        rest: restParam,
        returnType:
          node.returnType?.typeAnnotation == null
            ? allowMissingReturn
              ? constructFlowNode<FlowESTree.VoidTypeAnnotation>({
                  type: 'VoidTypeAnnotation',
                })
              : unsupportedAnnotation(node, 'missing return type')
            : Transform.TSTypeAnnotation(node.returnType.typeAnnotation),
        this: thisParam,
      });
    }

    static _partitionAndTranslateTSFunctionParams(
      tsParams: ReadonlyArray<TSESTree.Parameter>,
    ): {
      thisParam: FlowESTree.FunctionTypeParam | null,
      restParam: FlowESTree.FunctionTypeParam | null,
      params: Array<FlowESTree.FunctionTypeParam>,
    } {
      const params = [...tsParams];
      const firstParam = params[0];
      let thisParam: FlowESTree.FunctionTypeParam | null = null;
      let restParam: FlowESTree.FunctionTypeParam | null = null;
      if (
        firstParam != null &&
        firstParam.type === 'Identifier' &&
        firstParam.name === 'this'
      ) {
        thisParam = constructFlowNode<FlowESTree.FunctionTypeParam>({
          type: 'FunctionTypeParam',
          name: constructFlowNode<FlowESTree.Identifier>({
            type: 'Identifier',
            name: 'this',
            optional: false,
            typeAnnotation: null,
          }),
          optional: false,
          typeAnnotation: Transform.TSTypeAnnotationOpt(
            firstParam.typeAnnotation?.typeAnnotation,
          ),
        });
        params.shift();
      }
      const lastParam = params[params.length - 1];
      if (lastParam != null && lastParam.type === 'RestElement') {
        restParam = constructFlowNode<FlowESTree.FunctionTypeParam>({
          type: 'FunctionTypeParam',
          name: constructFlowNode<FlowESTree.Identifier>({
            type: 'Identifier',
            name: '$$rest$$',
            optional: false,
            typeAnnotation: null,
          }),
          optional: false,
          typeAnnotation: Transform.TSTypeAnnotationOpt(
            lastParam.typeAnnotation?.typeAnnotation,
          ),
        });
        params.pop();
      }
      return {
        thisParam,
        restParam,
        params: params.map((param, i) => {
          if (param.type === 'Identifier') {
            return constructFlowNode<FlowESTree.FunctionTypeParam>({
              type: 'FunctionTypeParam',
              name: constructFlowNode<FlowESTree.Identifier>({
                type: 'Identifier',
                name: param.name,
                optional: false,
                typeAnnotation: null,
              }),
              optional: Boolean(param.optional),
              typeAnnotation: Transform.TSTypeAnnotationOpt(
                param.typeAnnotation?.typeAnnotation,
              ),
            });
          } else if (
            param.type === 'ArrayPattern' ||
            param.type === 'ObjectPattern'
          ) {
            return constructFlowNode<FlowESTree.FunctionTypeParam>({
              type: 'FunctionTypeParam',
              name: constructFlowNode<FlowESTree.Identifier>({
                type: 'Identifier',
                name: `$$param${i}$`,
                optional: false,
                typeAnnotation: null,
              }),
              optional: Boolean(param.optional),
              typeAnnotation: Transform.TSTypeAnnotationOpt(
                param.typeAnnotation?.typeAnnotation,
              ),
            });
          } else {
            throw new Error(`Unexpected function parameter ${param.type}`);
          }
        }),
      };
    }

    static TSImportType(
      node: TSESTree.TSImportType,
    ): FlowESTree.TypeAnnotationType {
      const source =
        node.argument ??
        (node.source == null
          ? null
          : ({
              type: 'TSLiteralType',
              loc: node.source.loc,
              literal: node.source,
            } as TSESTree.TSLiteralType));
      if (source == null) {
        return unsupportedAnnotation(node, 'import types without a source');
      }

      let base: FlowESTree.TypeAnnotationType =
        constructFlowNode<FlowESTree.GenericTypeAnnotation>({
          type: 'GenericTypeAnnotation',
          id: constructFlowNode<FlowESTree.Identifier>({
            type: 'Identifier',
            name: '$Exports',
            optional: false,
            typeAnnotation: null,
          }),
          typeArguments:
            constructFlowNode<FlowESTree.TypeParameterInstantiation>({
              type: 'TypeParameterInstantiation',
              params: [Transform.TSTypeAnnotation(source)],
            }),
        });
      if (node.typeArguments != null) {
        return unsupportedAnnotation(node, 'import types with type arguments');
      }
      if (node.qualifier == null) {
        return base;
      }
      let qualifier:
        FlowESTree.Identifier | FlowESTree.QualifiedTypeIdentifier =
        Transform.EntityNameToTypeIdentifier(node.qualifier);
      const namesRev: Array<string> = [];
      while (qualifier.type !== 'Identifier') {
        namesRev.push(qualifier.id.name);
        if (qualifier.qualification.type === 'ImportType') {
          return unsupportedAnnotation(
            node,
            'nested import types in import type qualifiers',
          );
        }
        qualifier = qualifier.qualification;
      }
      namesRev.push(qualifier.name);
      while (namesRev.length > 0) {
        const name = namesRev.pop();
        base = constructFlowNode<FlowESTree.IndexedAccessType>({
          type: 'IndexedAccessType',
          objectType: base,
          indexType: constructFlowNode<FlowESTree.StringLiteralTypeAnnotation>({
            type: 'StringLiteralTypeAnnotation',
            // $FlowFixMe[incompatible-type]
            value: name,
            // $FlowFixMe[incompatible-type]
            raw: `'${name}'`,
          }),
        });
      }
      return base;
    }

    static TSImportEqualsDeclaration(
      node: TSESTree.TSImportEqualsDeclaration,
    ): FlowESTree.VariableDeclaration | FlowESTree.TypeAlias {
      if (
        node.moduleReference.type === 'ThisExpression' ||
        node.moduleReference.type === 'TSQualifiedName'
      ) {
        return unsupportedDeclaration(
          node,
          'import equals declaration with weird module reference',
          node.id,
        );
      }
      let moduleName: string;
      if (node.moduleReference.type === 'TSExternalModuleReference') {
        if (node.moduleReference.expression.type === 'Literal') {
          moduleName = String(node.moduleReference.expression.value);
        } else {
          return unsupportedDeclaration(
            node,
            'import equals declaration with weird module reference',
            node.id,
          );
        }
      } else {
        moduleName = node.moduleReference.name;
      }
      return constructFlowNode<FlowESTree.VariableDeclaration>({
        type: 'VariableDeclaration',
        kind: 'const',
        declarations: [
          constructFlowNode<FlowESTree.VariableDeclarator>({
            type: 'VariableDeclarator',
            id: constructFlowNode<FlowESTree.Identifier>({
              type: 'Identifier',
              name: node.id.name,
              optional: false,
              typeAnnotation: null,
            }),
            init: constructFlowNode<FlowESTree.CallExpression>({
              type: 'CallExpression',
              callee: constructFlowNode<FlowESTree.Identifier>({
                type: 'Identifier',
                name: 'require',
                optional: false,
                typeAnnotation: null,
              }),
              arguments: [
                constructFlowNode<FlowESTree.StringLiteral>({
                  type: 'Literal',
                  literalType: 'string',
                  value: moduleName,
                  raw: `"${moduleName}"`,
                }),
              ],
              optional: false,
              typeArguments: null,
            }),
          }),
        ],
      });
    }

    static TSIndexedAccessType(
      node: TSESTree.TSIndexedAccessType,
    ): FlowESTree.IndexedAccessType {
      return constructFlowNode<FlowESTree.IndexedAccessType>({
        type: 'IndexedAccessType',
        objectType: Transform.TSTypeAnnotation(node.objectType),
        indexType: Transform.TSTypeAnnotation(node.indexType),
      });
    }

    static TSInferType(
      node: TSESTree.TSInferType,
    ): FlowESTree.InferTypeAnnotation {
      return constructFlowNode<FlowESTree.InferTypeAnnotation>({
        type: 'InferTypeAnnotation',
        typeParameter: Transform.TSTypeParameter(node.typeParameter),
      });
    }

    static TSInterfaceDeclaration(
      node: TSESTree.TSInterfaceDeclaration,
    ): FlowESTree.InterfaceDeclaration {
      const body = Transform.TSTypeLiteralOrInterfaceBody(node.body);
      // $FlowFixMe[cannot-write]
      body.inexact = false;
      return constructFlowNode<FlowESTree.InterfaceDeclaration>({
        type: 'InterfaceDeclaration',
        id: Transform.Identifier(node.id),
        typeParameters: Transform.TSTypeParameterDeclarationOpt(
          node.typeParameters,
        ),
        body,
        extends: (node.extends || []).map(e =>
          Transform.TSInterfaceHeritage(e),
        ),
      });
    }

    static TSInterfaceHeritage(
      node: TSESTree.TSInterfaceHeritage,
    ): FlowESTree.InterfaceExtends {
      return constructFlowNode<FlowESTree.InterfaceExtends>({
        type: 'InterfaceExtends',
        id: Transform._expressionToIdOrQualifiedTypeId(
          node.expression,
          'interface extends',
        ),
        typeArguments: Transform.TSTypeParameterInstantiationOpt(
          node.typeArguments,
        ),
      });
    }

    static _expressionToIdOrQualifiedTypeId(
      node: TSESTree.Expression,
      kind: string,
    ): FlowESTree.Identifier | FlowESTree.QualifiedTypeIdentifier {
      if (node.type === 'Identifier') {
        return Transform.Identifier(node);
      } else if (
        node.type === 'MemberExpression' &&
        node.property.type === 'Identifier'
      ) {
        const id = Transform.Identifier(node.property);
        return constructFlowNode<FlowESTree.QualifiedTypeIdentifier>({
          type: 'QualifiedTypeIdentifier',
          qualification: Transform._expressionToIdOrQualifiedTypeId(
            node.object,
            kind,
          ),
          id,
        });
      } else {
        throw unexpectedTranslationError(
          node,
          `Expected ${kind} to be an Identifier or Member`,
        );
      }
    }

    static TSIntersectionType(
      node: TSESTree.TSIntersectionType,
    ): FlowESTree.IntersectionTypeAnnotation {
      return constructFlowNode<FlowESTree.IntersectionTypeAnnotation>({
        type: 'IntersectionTypeAnnotation',
        types: node.types.map(node => Transform.TSTypeAnnotation(node)),
      });
    }

    static TSLiteralType(
      node: TSESTree.TSLiteralType,
    ): FlowESTree.TypeAnnotationType {
      switch (node.literal.type) {
        case 'TemplateLiteral':
          return unsupportedAnnotation(node, 'template literals');
        case 'Literal':
          return Transform.LiteralType(node.literal);
        case 'UnaryExpression':
          return unsupportedAnnotation(node, 'UnaryExpression literal type');
        case 'UpdateExpression':
          return unsupportedAnnotation(node, 'UpdateExpression literal type');
      }
    }

    static TSMappedType(
      node: TSESTree.TSMappedType,
    ): FlowESTree.ObjectTypeAnnotation {
      const keyTparam = Transform.TSTypeParameter(node.typeParameter);
      const sourceType = Transform.TSTypeAnnotationOpt(
        node.typeParameter.constraint,
      );
      // $FlowFixMe[cannot-write]
      keyTparam.bound = null;
      const prop = constructFlowNode<FlowESTree.ObjectTypeMappedTypeProperty>({
        type: 'ObjectTypeMappedTypeProperty',
        keyTparam,
        propType: Transform.TSTypeAnnotationOpt(node.typeAnnotation),
        sourceType,
        nameType:
          node.nameType == null
            ? null
            : Transform.TSTypeAnnotation(node.nameType),
        variance:
          node.readonly != null && node.readonly !== false
            ? constructFlowNode<FlowESTree.Variance>({
                type: 'Variance',
                kind: 'plus',
              })
            : null,
        varianceOp:
          node.readonly === '+' ? '+' : node.readonly === '-' ? '-' : null,
        optional:
          node.optional === '+'
            ? 'PlusOptional'
            : node.optional === '-'
              ? 'MinusOptional'
              : // eslint-disable-next-line no-extra-boolean-cast
                Boolean(node.optional)
                ? 'Optional'
                : null,
      });
      return constructFlowNode<FlowESTree.ObjectTypeAnnotation>({
        type: 'ObjectTypeAnnotation',
        properties: [prop],
        indexers: [],
        callProperties: [],
        internalSlots: [],
        exact: false,
        inexact: false,
      });
    }

    static TSModuleDeclaration(
      node: TSESTree.TSModuleDeclaration,
    ):
      | FlowESTree.TypeAlias
      | FlowESTree.DeclareNamespace
      | FlowESTree.DeclareModule {
      const body =
        node.body == null
          ? constructFlowNode<FlowESTree.BlockStatement>({
              type: 'BlockStatement',
              body: [],
            })
          : node.body.type === 'TSModuleDeclaration'
            ? (() => {
                throw translationError(node, 'nested module declarations');
              })()
            : constructFlowNode<FlowESTree.BlockStatement>({
                type: 'BlockStatement',
                body: node.body.body.flatMap(s => Transform.Statement(s)),
              });
      if (node.id.type === 'Literal') {
        return constructFlowNode<FlowESTree.DeclareModule>({
          type: 'DeclareModule',
          id: Transform.Literal(node.id) as $FlowFixMe,
          body,
        });
      }
      if (node.global ?? false) {
        return unsupportedDeclaration(node, 'global declaration', node.id);
      }
      return constructFlowNode<FlowESTree.DeclareNamespace>({
        type: 'DeclareNamespace',
        global: false,
        id: Transform.Identifier(node.id),
        body,
        keyword: 'namespace',
      });
    }

    static TSNamedTupleMember(
      node: TSESTree.TSNamedTupleMember,
    ): FlowESTree.TupleTypeLabeledElement | FlowESTree.TupleTypeSpreadElement {
      let optional = false;
      let elementType: FlowESTree.TypeAnnotationType;
      if (node.elementType.type === 'TSRestType') {
        const child = node.elementType;
        return constructFlowNode<FlowESTree.TupleTypeSpreadElement>({
          type: 'TupleTypeSpreadElement',
          label: Transform.Identifier(node.label),
          typeAnnotation: Transform.TSTypeAnnotation(child.typeAnnotation),
          optional: false,
          variance: null,
        });
      } else if (node.elementType.type === 'TSOptionalType') {
        optional = true;
        elementType = Transform.TSTypeAnnotation(
          node.elementType.typeAnnotation,
        );
      } else {
        elementType = Transform.TSTypeAnnotation(node.elementType);
      }
      return constructFlowNode<FlowESTree.TupleTypeLabeledElement>({
        type: 'TupleTypeLabeledElement',
        label: Transform.Identifier(node.label),
        elementType,
        optional,
        variance: null,
      });
    }

    static TSNeverType(): FlowESTree.EmptyTypeAnnotation {
      return constructFlowNode<FlowESTree.EmptyTypeAnnotation>({
        type: 'EmptyTypeAnnotation',
      });
    }

    static TSNullType(): FlowESTree.NullLiteralTypeAnnotation {
      return constructFlowNode<FlowESTree.NullLiteralTypeAnnotation>({
        type: 'NullLiteralTypeAnnotation',
      });
    }

    static TSNumberType(): FlowESTree.NumberTypeAnnotation {
      return constructFlowNode<FlowESTree.NumberTypeAnnotation>({
        type: 'NumberTypeAnnotation',
      });
    }

    static TSObjectType(): FlowESTree.InterfaceTypeAnnotation {
      return constructFlowNode<FlowESTree.InterfaceTypeAnnotation>({
        type: 'InterfaceTypeAnnotation',
        body: constructFlowNode<FlowESTree.ObjectTypeAnnotation>({
          type: 'ObjectTypeAnnotation',
          inexact: false,
          exact: false,
          properties: [],
          indexers: [],
          callProperties: [],
          internalSlots: [],
        }),
        extends: [],
      });
    }

    static TSQualifiedNameToQualifiedTypeIdentifier(
      node: TSESTree.TSQualifiedName,
    ): FlowESTree.QualifiedTypeIdentifier {
      return constructFlowNode<FlowESTree.QualifiedTypeIdentifier>({
        type: 'QualifiedTypeIdentifier',
        qualification: Transform.EntityNameToTypeIdentifier(node.left),
        id: Transform.Identifier(node.right),
      });
    }

    static TSQualifiedNameToQualifiedTypeofIdentifier(
      node: TSESTree.TSQualifiedName,
    ): FlowESTree.QualifiedTypeofIdentifier {
      return constructFlowNode<FlowESTree.QualifiedTypeofIdentifier>({
        type: 'QualifiedTypeofIdentifier',
        qualification: Transform.EntityNameToTypeofIdentifier(node.left),
        id: Transform.Identifier(node.right),
      });
    }

    static TSStringType(): FlowESTree.StringTypeAnnotation {
      return constructFlowNode<FlowESTree.StringTypeAnnotation>({
        type: 'StringTypeAnnotation',
      });
    }

    static TSSymbolType(): FlowESTree.SymbolTypeAnnotation {
      return constructFlowNode<FlowESTree.SymbolTypeAnnotation>({
        type: 'SymbolTypeAnnotation',
      });
    }

    static TSTemplateLiteralType(
      node: TSESTree.TSTemplateLiteralType,
    ): FlowESTree.AnyTypeAnnotation {
      return unsupportedAnnotation(node, 'constructor types');
    }

    static TSThisType(
      _node: TSESTree.TSThisType,
    ): FlowESTree.GenericTypeAnnotation {
      return constructFlowNode<FlowESTree.GenericTypeAnnotation>({
        type: 'GenericTypeAnnotation',
        id: constructFlowNode<FlowESTree.Identifier>({
          type: 'Identifier',
          name: 'this',
          typeAnnotation: null,
          optional: false,
        }),
        typeArguments: null,
      });
    }

    static TSTupleType(
      node: TSESTree.TSTupleType,
    ): FlowESTree.TupleTypeAnnotation {
      return constructFlowNode<FlowESTree.TupleTypeAnnotation>({
        type: 'TupleTypeAnnotation',
        elementTypes: node.elementTypes.map(node =>
          Transform.TSTypeAnnotation(node),
        ),
        inexact: false,
      });
    }

    static TSTypeAliasDeclaration(
      node: TSESTree.TSTypeAliasDeclaration,
    ): FlowESTree.TypeAlias {
      return constructFlowNode<FlowESTree.TypeAlias>({
        type: 'TypeAlias',
        id: Transform.Identifier(node.id),
        typeParameters: Transform.TSTypeParameterDeclarationOpt(
          node.typeParameters,
        ),
        right: Transform.TSTypeAnnotation(node.typeAnnotation),
      });
    }

    static TSTypeAnnotation(
      node: TSESTree.TypeNode,
    ): FlowESTree.TypeAnnotationType {
      switch (node.type) {
        case 'TSOptionalType':
        case 'TSQualifiedName':
        case 'TSRestType':
          return unsupportedAnnotation(
            node,
            'unexpected toplevel ' + node.type,
          );
        case 'TSAbstractKeyword':
        case 'TSAsyncKeyword':
        case 'TSDeclareKeyword':
        case 'TSExportKeyword':
        case 'TSPrivateKeyword':
        case 'TSProtectedKeyword':
        case 'TSPublicKeyword':
        case 'TSReadonlyKeyword':
        case 'TSStaticKeyword':
          return unsupportedAnnotation(node, 'wat keyword ' + node.type);
        case 'TSAnyKeyword':
          return Transform.TSAnyType();
        case 'TSArrayType':
          return Transform.TSArrayType(node);
        case 'TSBigIntKeyword':
          return Transform.TSBigIntType();
        case 'TSBooleanKeyword':
          return Transform.TSBooleanType();
        case 'TSConditionalType':
          return Transform.TSConditionalType(node);
        case 'TSConstructorType':
          return Transform.TSConstructorType(node);
        case 'TSFunctionType':
          return Transform.TSFunctionType(node);
        case 'TSImportType':
          return Transform.TSImportType(node);
        case 'TSIndexedAccessType':
          return Transform.TSIndexedAccessType(node);
        case 'TSInferType':
          return Transform.TSInferType(node);
        case 'TSIntersectionType':
          return Transform.TSIntersectionType(node);
        case 'TSIntrinsicKeyword':
          return unsupportedAnnotation(node, 'intrinsic keyword');
        case 'TSLiteralType':
          return Transform.TSLiteralType(node);
        case 'TSMappedType':
          return Transform.TSMappedType(node);
        case 'TSNamedTupleMember':
          return Transform.TSNamedTupleMember(node);
        case 'TSNeverKeyword':
          return Transform.TSNeverType();
        case 'TSNullKeyword':
          return Transform.TSNullType();
        case 'TSNumberKeyword':
          return Transform.TSNumberType();
        case 'TSObjectKeyword':
          return Transform.TSObjectType();
        case 'TSStringKeyword':
          return Transform.TSStringType();
        case 'TSSymbolKeyword':
          return Transform.TSSymbolType();
        case 'TSTemplateLiteralType':
          return Transform.TSTemplateLiteralType(node);
        case 'TSThisType':
          return Transform.TSThisType(node);
        case 'TSTupleType':
          return Transform.TSTupleType(node);
        case 'TSTypeLiteral':
          return Transform.TSTypeLiteralOrInterfaceBody(node);
        case 'TSTypeOperator':
          return Transform.TSTypeOperator(node);
        case 'TSTypePredicate':
          return Transform.TSTypePredicate(node);
        case 'TSTypeQuery':
          return Transform.TSTypeQuery(node);
        case 'TSTypeReference':
          return Transform.TSTypeReference(node);
        case 'TSUndefinedKeyword':
        case 'TSVoidKeyword':
          return Transform.TSUndefinedOrVoidType();
        case 'TSUnionType':
          return Transform.TSUnionType(node);
        case 'TSUnknownKeyword':
          return Transform.TSUnknownType();
      }
    }

    static TSTypeAnnotationOpt(
      node: ?TSESTree.TypeNode,
    ): FlowESTree.TypeAnnotationType {
      return node == null
        ? constructFlowNode<FlowESTree.AnyTypeAnnotation>({
            type: 'AnyTypeAnnotation',
          })
        : Transform.TSTypeAnnotation(node);
    }

    static TSTypeAnnotationNode(
      node: TSESTree.TSTypeAnnotation,
    ): FlowESTree.TypeAnnotation {
      return constructFlowNode<FlowESTree.TypeAnnotation>({
        type: 'TypeAnnotation',
        typeAnnotation: Transform.TSTypeAnnotation(node.typeAnnotation),
      });
    }

    /** A very confusingly named object type */
    static TSTypeLiteralOrInterfaceBody(
      node: TSESTree.TSTypeLiteral | TSESTree.TSInterfaceBody,
    ): FlowESTree.ObjectTypeAnnotation {
      const properties: Array<FlowESTree.ObjectTypeProperty> = [];
      const indexers: Array<FlowESTree.ObjectTypeIndexer> = [];
      const callProperties: Array<FlowESTree.ObjectTypeCallProperty> = [];
      for (const prop of node.type === 'TSTypeLiteral'
        ? node.members
        : node.body) {
        switch (prop.type) {
          case 'TSPropertySignature': {
            Transform._translateIntoObjectProp(prop, properties, indexers);
            break;
          }
          case 'TSMethodSignature': {
            Transform._translateIntoObjectMethod(prop, properties);
            break;
          }
          case 'TSCallSignatureDeclaration':
            callProperties.push(
              constructFlowNode<FlowESTree.ObjectTypeCallProperty>({
                type: 'ObjectTypeCallProperty',
                method: false,
                optional: false,
                static: false,
                proto: false,
                variance: null,
                value: Transform.TSFunctionType({
                  type: 'TSFunctionType',
                  loc: prop.loc,
                  params: prop.params,
                  returnType: prop.returnType,
                  typeParameters: prop.typeParameters,
                }),
              }),
            );
            break;
          case 'TSIndexSignature': {
            // eslint-disable-next-line no-extra-boolean-cast
            const variance = Boolean(prop.readonly)
              ? constructFlowNode<FlowESTree.Variance>({
                  type: 'Variance',
                  kind: 'plus',
                })
              : null;
            indexers.push(
              constructFlowNode<FlowESTree.ObjectTypeIndexer>({
                type: 'ObjectTypeIndexer',
                kind: 'init',
                method: false,
                optional: false,
                static: Boolean(prop.static),
                proto: false,
                variance,
                id: null,
                key: constructFlowNode<FlowESTree.StringTypeAnnotation>({
                  type: 'StringTypeAnnotation',
                }),
                value: Transform.TSTypeAnnotationOpt(
                  prop.typeAnnotation?.typeAnnotation,
                ),
              }),
            );
            break;
          }
          case 'TSConstructSignatureDeclaration':
            properties.push(
              constructFlowNode<FlowESTree.ObjectTypeMethodSignature>({
                type: 'ObjectTypeProperty',
                kind: 'init',
                method: true,
                optional: false,
                static: false,
                proto: false,
                variance: null,
                key: constructFlowNode<FlowESTree.Identifier>({
                  type: 'Identifier',
                  name: 'constructor',
                  optional: false,
                  typeAnnotation: null,
                }),
                value: Transform.TSFunctionType(
                  {
                    type: 'TSFunctionType',
                    loc: prop.loc,
                    params: prop.params,
                    returnType: prop.returnType,
                    typeParameters: prop.typeParameters,
                  },
                  true,
                ),
              }),
            );
            break;
        }
      }
      return constructFlowNode<FlowESTree.ObjectTypeAnnotation>({
        type: 'ObjectTypeAnnotation',
        properties,
        indexers,
        callProperties,
        internalSlots: [],
        exact: false,
        inexact: true,
      });
    }

    static _translateIntoObjectProp(
      prop:
        | TSESTree.TSPropertySignatureComputedName
        | TSESTree.TSPropertySignatureNonComputedName
        | TSESTree.TSAbstractPropertyDefinitionComputedName
        | TSESTree.TSAbstractPropertyDefinitionNonComputedName
        | TSESTree.PropertyDefinitionComputedName
        | TSESTree.PropertyDefinitionNonComputedName,
      properties: Array<FlowESTree.ObjectTypeProperty>,
      indexers: Array<FlowESTree.ObjectTypeIndexer>,
    ): void {
      // eslint-disable-next-line no-extra-boolean-cast
      const variance = Boolean(prop.readonly)
        ? constructFlowNode<FlowESTree.Variance>({
            type: 'Variance',
            kind: 'plus',
          })
        : null;
      if (prop.computed === false) {
        const key = prop.key;
        properties.push(
          constructFlowNode<FlowESTree.ObjectTypePropertySignature>({
            type: 'ObjectTypeProperty',
            kind: 'init',
            method: false,
            optional: Boolean(prop.optional),
            static: false,
            proto: false,
            variance,
            key:
              key.type === 'Identifier'
                ? Transform.Identifier(key, false)
                : key.type === 'PrivateIdentifier'
                  ? (constructFlowNode<FlowESTree.PrivateIdentifier>({
                      type: 'PrivateIdentifier',
                      name: key.name,
                      optional: false,
                      typeAnnotation: null,
                    }) as $FlowFixMe)
                  : constructFlowNode<FlowESTree.StringLiteral>({
                      type: 'Literal',
                      literalType: 'string',
                      value: String(key.value),
                      raw: JSON.stringify(String(key.value)),
                    }),
            value: Transform.TSTypeAnnotationOpt(
              prop.typeAnnotation?.typeAnnotation,
            ),
          }),
        );
      } else {
        indexers.push(
          constructFlowNode<FlowESTree.ObjectTypeIndexer>({
            type: 'ObjectTypeIndexer',
            kind: 'init',
            method: false,
            optional: Boolean(prop.optional),
            static: false,
            proto: false,
            variance,
            id: null,
            key: constructFlowNode<FlowESTree.StringTypeAnnotation>({
              type: 'StringTypeAnnotation',
            }),
            value: Transform.TSTypeAnnotationOpt(
              prop.typeAnnotation?.typeAnnotation,
            ),
          }),
        );
      }
    }

    static _translateIntoObjectMethod(
      prop:
        | TSESTree.TSMethodSignatureComputedName
        | TSESTree.TSMethodSignatureNonComputedName
        | TSESTree.MethodDefinitionComputedName
        | TSESTree.MethodDefinitionNonComputedName
        | TSESTree.TSAbstractMethodDefinitionComputedName
        | TSESTree.TSAbstractMethodDefinitionNonComputedName,
      properties: Array<FlowESTree.ObjectTypeProperty>,
    ): void {
      if (prop.computed === true) {
        throw translationError(prop, 'computed method signature');
      }
      const originalKey = prop.key;
      const key: FlowESTree.Identifier | FlowESTree.StringLiteral =
        originalKey.type === 'Identifier'
          ? Transform.Identifier(originalKey, false)
          : originalKey.type === 'PrivateIdentifier'
            ? (constructFlowNode<FlowESTree.PrivateIdentifier>({
                type: 'PrivateIdentifier',
                name: originalKey.name,
                optional: false,
                typeAnnotation: null,
              }) as $FlowFixMe)
            : constructFlowNode<FlowESTree.StringLiteral>({
                type: 'Literal',
                literalType: 'string',
                value: String(originalKey.value),
                raw: JSON.stringify(String(originalKey.value)),
              });
      const value = Transform.TSFunctionType(
        {
          type: 'TSFunctionType',
          loc: prop.loc,
          params:
            prop.type === 'MethodDefinition' ||
            prop.type === 'TSAbstractMethodDefinition'
              ? prop.value.params
              : prop.params,
          returnType:
            prop.type === 'MethodDefinition' ||
            prop.type === 'TSAbstractMethodDefinition'
              ? prop.value.returnType
              : prop.returnType,
          typeParameters: prop.typeParameters,
        },
        true,
      );
      if (prop.kind === 'method' || prop.kind === 'constructor') {
        properties.push(
          constructFlowNode<FlowESTree.ObjectTypeMethodSignature>({
            type: 'ObjectTypeProperty',
            kind: 'init',
            method: true,
            optional: false,
            static: false,
            proto: false,
            variance: null,
            key,
            value,
          }),
        );
      } else {
        properties.push(
          constructFlowNode<FlowESTree.ObjectTypeAccessorSignature>({
            type: 'ObjectTypeProperty',
            kind: prop.kind,
            method: false,
            optional: false,
            static: false,
            proto: false,
            variance: null,
            key,
            value,
          }),
        );
      }
    }

    static TSTypeOperator(
      node: TSESTree.TSTypeOperator,
    ): FlowESTree.TypeAnnotationType {
      switch (node.operator) {
        case 'unique':
          return unsupportedAnnotation(node, 'unique operator');
        case 'keyof':
          return constructFlowNode<FlowESTree.KeyofTypeAnnotation>({
            type: 'KeyofTypeAnnotation',
            argument: Transform.TSTypeAnnotationOpt(node.typeAnnotation),
          });
        case 'readonly': {
          const child = node.typeAnnotation;
          switch (child?.type) {
            case 'TSArrayType':
              return constructFlowNode<FlowESTree.GenericTypeAnnotation>({
                type: 'GenericTypeAnnotation',
                id: constructFlowNode<FlowESTree.Identifier>({
                  type: 'Identifier',
                  name: '$ReadOnlyArray',
                  optional: false,
                  typeAnnotation: null,
                }),
                typeArguments:
                  constructFlowNode<FlowESTree.TypeParameterInstantiation>({
                    type: 'TypeParameterInstantiation',
                    params: [Transform.TSTypeAnnotation(child.elementType)],
                  }),
              });
            case 'TSTupleType':
              return constructFlowNode<FlowESTree.GenericTypeAnnotation>({
                type: 'GenericTypeAnnotation',
                id: constructFlowNode<FlowESTree.Identifier>({
                  type: 'Identifier',
                  name: '$ReadOnly',
                  optional: false,
                  typeAnnotation: null,
                }),
                typeArguments:
                  constructFlowNode<FlowESTree.TypeParameterInstantiation>({
                    type: 'TypeParameterInstantiation',
                    params: [Transform.TSTypeAnnotation(child)],
                  }),
              });
            default:
              return unsupportedAnnotation(
                node,
                'readonly operator with inner type: ' + (child?.type || 'null'),
              );
          }
        }
      }
    }

    static TSTypeParameter(
      node: TSESTree.TSTypeParameter,
    ): FlowESTree.TypeParameter {
      return constructFlowNode<FlowESTree.TypeParameter>({
        type: 'TypeParameter',
        name: node.name.name,
        bound:
          node.constraint == null
            ? null
            : Transform.TSTypeAnnotation(node.constraint),
        const: false,
        default:
          node.default == null
            ? null
            : Transform.TSTypeAnnotation(node.default),
        usesExtendsBound: node.constraint != null,
        variance:
          (node.in && node.out) || (!node.in && !node.out)
            ? null
            : constructFlowNode<FlowESTree.Variance>({
                type: 'Variance',
                kind: node.out ? 'plus' : 'minus',
              }),
      });
    }

    static TSTypeParameterDeclaration(
      node: TSESTree.TSTypeParameterDeclaration,
    ): FlowESTree.TypeParameterDeclaration {
      return constructFlowNode<FlowESTree.TypeParameterDeclaration>({
        type: 'TypeParameterDeclaration',
        params: node.params.map(node => Transform.TSTypeParameter(node)),
      });
    }

    static TSTypeParameterDeclarationOpt(
      node: ?TSESTree.TSTypeParameterDeclaration,
    ): FlowESTree.TypeParameterDeclaration | null {
      return node != null ? Transform.TSTypeParameterDeclaration(node) : null;
    }

    static TSTypeParameterInstantiation(
      node: TSESTree.TSTypeParameterInstantiation,
    ): FlowESTree.TypeParameterInstantiation {
      return constructFlowNode<FlowESTree.TypeParameterInstantiation>({
        type: 'TypeParameterInstantiation',
        params: node.params.map(node => Transform.TSTypeAnnotation(node)),
      });
    }

    static TSTypeParameterInstantiationOpt(
      node: ?TSESTree.TSTypeParameterInstantiation,
    ): FlowESTree.TypeParameterInstantiation | null {
      return node != null ? Transform.TSTypeParameterInstantiation(node) : null;
    }

    static TSTypePredicate(
      node: TSESTree.TSTypePredicate,
    ): FlowESTree.TypePredicate {
      return constructFlowNode<FlowESTree.TypePredicate>({
        type: 'TypePredicate',
        parameterName:
          node.parameterName.type === 'TSThisType'
            ? constructFlowNode<FlowESTree.Identifier>({
                type: 'Identifier',
                name: 'this',
                optional: false,
                typeAnnotation: null,
              })
            : Transform.Identifier(node.parameterName, false),
        kind: node.asserts ? 'asserts' : null,
        typeAnnotation:
          node.typeAnnotation == null
            ? null
            : Transform.TSTypeAnnotation(node.typeAnnotation.typeAnnotation),
      });
    }

    static TSTypeQuery(
      node: TSESTree.TSTypeQuery,
    ): FlowESTree.TypeAnnotationType {
      if (node.exprName.type === 'TSImportType') {
        if (node.typeArguments != null) {
          return unsupportedAnnotation(
            node,
            'type query imports with type arguments',
          );
        }
        return Transform.TSImportType(node.exprName);
      }

      return constructFlowNode<FlowESTree.TypeofTypeAnnotation>({
        type: 'TypeofTypeAnnotation',
        argument: Transform.EntityNameToTypeofIdentifier(node.exprName),
        typeArguments:
          Transform.TSTypeParameterInstantiationOpt(node.typeArguments) ??
          undefined,
      });
    }

    static TSTypeReference(
      node: TSESTree.TSTypeReference,
    ): FlowESTree.GenericTypeAnnotation {
      return constructFlowNode<FlowESTree.GenericTypeAnnotation>({
        type: 'GenericTypeAnnotation',
        id: Transform.EntityNameToTypeIdentifier(node.typeName),
        typeArguments: Transform.TSTypeParameterInstantiationOpt(
          node.typeArguments,
        ),
      });
    }

    static TSUndefinedOrVoidType(): FlowESTree.VoidTypeAnnotation {
      return constructFlowNode<FlowESTree.VoidTypeAnnotation>({
        type: 'VoidTypeAnnotation',
      });
    }

    static TSUnionType(
      node: TSESTree.TSUnionType,
    ): FlowESTree.UnionTypeAnnotation {
      return constructFlowNode<FlowESTree.UnionTypeAnnotation>({
        type: 'UnionTypeAnnotation',
        types: node.types.map(node => Transform.TSTypeAnnotation(node)),
      });
    }

    static TSUnknownType(): FlowESTree.MixedTypeAnnotation {
      return constructFlowNode<FlowESTree.MixedTypeAnnotation>({
        type: 'MixedTypeAnnotation',
      });
    }

    static VariableDeclaration(
      node: TSESTree.VariableDeclaration,
    ): ReadonlyArray<FlowESTree.DeclareVariable> {
      return node.declarations.map(decl => {
        if (decl.id.type !== 'Identifier') {
          throw translationError(
            decl.id,
            'Non-identifier variable declaration',
          );
        }
        const id = Transform.Identifier(decl.id);
        if (id.typeAnnotation == null) {
          // $FlowExpectedError[cannot-write]
          id.typeAnnotation = constructFlowNode<FlowESTree.TypeAnnotation>({
            type: 'TypeAnnotation',
            typeAnnotation: constructFlowNode<FlowESTree.AnyTypeAnnotation>({
              type: 'AnyTypeAnnotation',
            }),
          });
        }
        return constructFlowNode<FlowESTree.DeclareVariable>({
          type: 'DeclareVariable',
          declarations: [
            constructFlowNode<FlowESTree.VariableDeclarator>({
              type: 'VariableDeclarator',
              id,
              init: null,
            }),
          ],
          kind: node.kind,
        });
      });
    }
  }

  return [Transform, code];
};
