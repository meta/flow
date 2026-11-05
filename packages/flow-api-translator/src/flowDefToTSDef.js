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
import type {ScopeManager} from 'flow-eslint';

import * as FlowESTree from 'flow-estree';
import * as TSESTree from './utils/ts-estree-ast-types';
import {
  cloneJSDocCommentsToNewNode as cloneJSDocCommentsToNewNodeOriginal,
  makeCommentOwnLine as makeCommentOwnLineOriginal,
} from 'flow-transform';
import {
  buildCodeFrame,
  translationError as translationErrorBase,
  unexpectedTranslationError as unexpectedTranslationErrorBase,
} from './utils/ErrorUtils';
import {removeAtFlowFromDocblock} from './utils/DocblockUtils';
import {
  ensureTypeIncludesUndefined,
  extractPropertyKeyLiterals,
  getDefaultExportBindingName,
} from './utils/TSNodeUtils';
import {EOL} from 'os';

type DeclarationOrUnsupported<T> = T | TSESTree.TSTypeAliasDeclaration;

const DUMMY_LOC: FlowESTree.SourceLocation = {
  start: {line: 1, column: 0},
  end: {line: 1, column: 0},
};

type LooseOmit<O extends interface {}, K extends keyof $FlowFixMe> = Pick<
  O,
  Exclude<keyof O, K>,
>;
function constructFlowNode<T extends FlowESTree.BaseNode>(
  node: LooseOmit<NoInfer<T>, 'parent'>,
): T {
  return node as $FlowFixMe;
}

const cloneJSDocCommentsToNewNode =
  // $FlowExpectedError[incompatible-type] - trust me this re-type is 100% safe
  cloneJSDocCommentsToNewNodeOriginal as (unknown, unknown) => void;

const makeCommentOwnLine =
  // $FlowExpectedError[incompatible-type] - trust me this re-type is 100% safe
  makeCommentOwnLineOriginal as (string, unknown) => string;

/**
 * `DeclareComponent` reaches us in one of two shapes: parsed nodes carry
 * `ComponentParameter`/`RestElement` in `params` with a null `rest` (the
 * `ComponentDeclaration` shape the Flow parser reuses here), while detached
 * nodes built with `flow-transform` use `ComponentTypeParameter` + `rest`.
 * Normalize both onto the latter so the shared `ComponentTypeParameters`
 * translation — which `ComponentTypeAnnotation` also uses — sees one shape.
 */
function normalizeDeclareComponentParams(
  params: FlowESTree.DeclareComponent['params'],
  rest: FlowESTree.DeclareComponent['rest'],
  onUnexpected: (node: ObjectWithLoc, message: string) => Error,
): [
  ReadonlyArray<FlowESTree.ComponentTypeParameter>,
  FlowESTree.ComponentTypeParameter | null,
] {
  const normalizedParams: Array<FlowESTree.ComponentTypeParameter> = [];
  let normalizedRest: FlowESTree.ComponentTypeParameter | null = rest;

  const toComponentTypeParameter = (
    name: FlowESTree.ComponentTypeParameter['name'],
    local: FlowESTree.ESNode,
    source: FlowESTree.BaseToken,
  ): FlowESTree.ComponentTypeParameter => {
    if (local.type !== 'Identifier' || local.typeAnnotation == null) {
      throw onUnexpected(
        source,
        `DeclareComponent: expected an annotated identifier parameter, got "${local.type}"`,
      );
    }
    return constructFlowNode<FlowESTree.ComponentTypeParameter>({
      type: 'ComponentTypeParameter',
      name,
      typeAnnotation: local.typeAnnotation.typeAnnotation,
      optional: local.optional,
      loc: source.loc,
      range: source.range,
    });
  };

  for (const param of params) {
    switch (param.type) {
      case 'ComponentTypeParameter':
        normalizedParams.push(param);
        break;
      case 'ComponentParameter':
        normalizedParams.push(
          toComponentTypeParameter(param.name, param.local, param),
        );
        break;
      case 'RestElement':
        normalizedRest = toComponentTypeParameter(null, param.argument, param);
        break;
    }
  }

  return [normalizedParams, normalizedRest];
}

const VALID_REACT_IMPORTS = new Set<string>(['React', 'react']);

function isValidReactImportOrGlobal(id: FlowESTree.Identifier): boolean {
  return VALID_REACT_IMPORTS.has(id.name) || id.name.startsWith('React$');
}

let shouldAddReactImport: boolean | null = null;

// Returns appropriate Identifier for `React` import.
// If a global is in use, set a flag to indicate that we should add the import.
function getReactIdentifier(hasReactImport: boolean): TSESTree.EntityName {
  if (shouldAddReactImport !== false) {
    shouldAddReactImport = !hasReactImport;
  }

  return {
    type: 'Identifier',
    loc: DUMMY_LOC,
    name: `React`,
  };
}

export function flowDefToTSDef(
  originalCode: string,
  ast: FlowESTree.Program,
  scopeManager: ScopeManager,
  opts: TranslationOptions,
): [TSESTree.Program, string] {
  const tsBody: Array<TSESTree.ProgramStatement> = [];
  const tsProgram: TSESTree.Program = {
    type: 'Program',
    body: tsBody,
    sourceType: ast.sourceType,
    loc: ast.loc,
    docblock:
      ast.docblock == null ? null : removeAtFlowFromDocblock(ast.docblock),
  };

  shouldAddReactImport = null;

  const [transform, code] = getTransforms(originalCode, scopeManager, opts);

  for (const node of ast.body) {
    if (node.type in transform) {
      const result: $FlowFixMe | Array<$FlowFixMe> = transform[
        // $FlowExpectedError[prop-missing]
        // $FlowFixMe[incompatible-type]
        node.type
      ](
        // $FlowExpectedError[incompatible-type]
        node,
      );
      tsBody.push(...(Array.isArray(result) ? result : [result]));
    } else {
      throw unexpectedTranslationErrorBase(
        node,
        `Unexpected node type ${node.type}`,
        {code},
      );
    }
  }

  if (shouldAddReactImport === true) {
    tsBody.unshift({
      type: 'ImportDeclaration',
      assertions: [],
      loc: DUMMY_LOC,
      source: {
        type: 'Literal',
        loc: DUMMY_LOC,
        value: 'react',
        raw: "'react'",
      },
      specifiers: [
        {
          type: 'ImportNamespaceSpecifier',
          loc: DUMMY_LOC,
          local: {
            type: 'Identifier',
            loc: DUMMY_LOC,
            name: 'React',
          },
        },
      ],
      importKind: 'value',
    });
  }

  return [tsProgram, code];
}

const getTransforms = (
  originalCode: string,
  scopeManager: ScopeManager,
  opts: TranslationOptions,
) => {
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
  function addErrorComment(node: TSESTree.Node, message: string): void {
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
  ): TSESTree.TSAnyKeyword {
    const message = unsupportedFeatureMessage(thing);
    if (opts.recoverFromErrors) {
      const codeFrame = buildCodeFrameForComment(node, message);
      const newNode: TSESTree.TSAnyKeyword = {
        type: 'TSAnyKeyword',
        loc: DUMMY_LOC,
      };
      addErrorComment(newNode, codeFrame);
      return newNode;
    }

    throw translationError(node, message);
  }
  function unsupportedDeclaration(
    node: ObjectWithLoc,
    thing: string,
    id: FlowESTree.Identifier,
    declare: boolean = false,
    typeParameters: FlowESTree.TypeParameterDeclaration | null = null,
  ): TSESTree.TSTypeAliasDeclaration {
    const message = unsupportedFeatureMessage(thing);
    if (opts.recoverFromErrors) {
      const codeFrame = buildCodeFrameForComment(node, message);
      const newNode: TSESTree.TSTypeAliasDeclaration = {
        type: 'TSTypeAliasDeclaration',
        loc: DUMMY_LOC,
        declare,
        id: transform.Identifier(id, false),
        typeAnnotation: {
          type: 'TSAnyKeyword',
          loc: DUMMY_LOC,
        },
        typeParameters:
          typeParameters == null
            ? undefined
            : transform.TypeParameterDeclaration(typeParameters),
      };
      addErrorComment(newNode, codeFrame);
      return newNode;
    }

    throw translationError(node, message);
  }

  const topScope = (() => {
    const globalScope = scopeManager.globalScope;
    if (
      globalScope.childScopes.length > 0 &&
      globalScope.childScopes[0].type === 'module'
    ) {
      return globalScope.childScopes[0];
    }
    return globalScope;
  })();

  function isReactImport(scopeNode: FlowESTree.ESNode, name: string): boolean {
    let currentScope = (() => {
      let scope = null;
      let node: FlowESTree.ESNode = scopeNode;
      while (!scope && node) {
        scope = scopeManager.acquire(node, true);
        node = node.parent;
      }

      return scope;
    })();

    if (currentScope == null) {
      throw new Error('unable to resolve scope');
    }

    const variableDef = (() => {
      while (currentScope != null) {
        for (const variable of currentScope.variables) {
          if (variable.defs.length && variable.name === name) {
            return variable;
          }
        }
        currentScope = currentScope.upper;
      }
    })();

    // No variable found, it is not imported.
    // It could be a global though if isValidReactImportOrGlobal returns true.
    if (variableDef == null) {
      return false;
    }

    const def = variableDef.defs[0];
    // Detect:
    switch (def.type) {
      // import React from 'react';
      // import * as React from 'react';
      case 'ImportBinding': {
        if (
          def.node.type === 'ImportDefaultSpecifier' ||
          def.node.type === 'ImportNamespaceSpecifier'
        ) {
          return VALID_REACT_IMPORTS.has(def.parent.source.value);
        }
        return false;
      }

      // Globals
      case 'ImplicitGlobalVariable': {
        return VALID_REACT_IMPORTS.has(name);
      }

      // TODO Handle:
      // const React = require('react');
      // const Something = React;
    }

    return false;
  }

  function EnumImpl(
    node: FlowESTree.EnumDeclaration | FlowESTree.DeclareEnum,
  ): DeclarationOrUnsupported<
    [TSESTree.TSEnumDeclaration, TSESTree.TSModuleDeclaration],
  > {
    const body = node.body;
    const firstMember = body.members[0];
    const enumType =
      body.explicitType ??
      (firstMember?.type === 'EnumBooleanMember'
        ? 'boolean'
        : firstMember?.type === 'EnumNumberMember'
          ? 'number'
          : firstMember?.type === 'EnumBigIntMember'
            ? 'bigint'
            : 'string');
    if (enumType === 'symbol') {
      /*
      There's unfortunately no way for us to support this in a clean way.
      We can get really close using this code:
      ```
      declare namespace SymbolEnum {
          export const member1: unique symbol;
          export type member1 = typeof member1;

          export const member2: unique symbol;
          export type member2 = typeof member2;
      }
      type SymbolEnum = typeof SymbolEnum[keyof typeof SymbolEnum];
      ```

      However as explained in https://github.com/microsoft/TypeScript/issues/43657:
      "A unique symbol type is never transferred from one declaration to another through inference."
      This intended behaviour in TS means that the usage of the fake-enum would look like this:
      ```
      const value: SymbolEnum.member1 = SymbolEnum.member1;
      //           ^^^^^^^^^^^^^^^^^^ required to force TS to retain the information
      ```
      Which is really clunky and shitty. It definitely works, but ofc it's not good.
      We can go with this design if users are okay with it!

      Considering how rarely used symbol enums are ATM, let's just put a pin in it for now.
      */
      return unsupportedDeclaration(
        node,
        'symbol enums',
        node.id,
        FlowESTree.isDeclareEnum(node),
      );
    }
    if (enumType === 'boolean') {
      /*
      TODO - TS enums only allow strings or numbers as their values - not booleans.
      This means we need a non-ts-enum representation of the enum.
      We can support boolean enums using a construct like this:
      ```ts
      declare namespace BooleanEnum {
          export const member1: true;
          export type member1 = typeof member1;

          export const member2: false;
          export type member2 = typeof member1;
      }
      declare type BooleanEnum = boolean;
      ```

      But it's pretty clunky and ugly.
      Considering how rarely used boolean enums are ATM, let's just put a pin in it for now.
      */
      return unsupportedDeclaration(
        node,
        'boolean enums',
        node.id,
        FlowESTree.isDeclareEnum(node),
      );
    }
    if (enumType === 'bigint') {
      return unsupportedDeclaration(
        node,
        'bigint enums',
        node.id,
        FlowESTree.isDeclareEnum(node),
      );
    }

    const members: Array<TSESTree.TSEnumMemberNonComputedName> = [];
    for (const member of body.members) {
      const memberId =
        member.id.type === 'Identifier'
          ? transform.Identifier(member.id, false)
          : transform.StringLiteral(member.id);
      switch (member.type) {
        case 'EnumDefaultedMember': {
          if (enumType === 'number') {
            // this should be impossible!
            throw unexpectedTranslationError(
              member,
              'Unexpected defaulted number enum member',
            );
          }
          members.push({
            type: 'TSEnumMember',
            loc: DUMMY_LOC,
            computed: false,
            id: memberId,
            initializer:
              member.id.type === 'Identifier'
                ? ({
                    type: 'Literal',
                    loc: DUMMY_LOC,
                    raw: `"${member.id.name}"`,
                    value: member.id.name,
                  } as TSESTree.StringLiteral)
                : transform.StringLiteral(member.id),
          });
          break;
        }

        case 'EnumNumberMember':
        case 'EnumStringMember':
          members.push({
            type: 'TSEnumMember',
            loc: DUMMY_LOC,
            computed: false,
            id: memberId,
            initializer:
              member.init.literalType === 'string'
                ? transform.StringLiteral(member.init)
                : transform.NumericLiteral(member.init),
          });
          break;
        case 'EnumBigIntMember':
        case 'EnumBooleanMember':
          throw unexpectedTranslationError(
            member,
            `Unexpected ${member.type} in ${enumType} enum`,
          );
      }
    }

    const bodyRepresentationType: TSESTree.TypeNode =
      enumType === 'number'
        ? {type: 'TSNumberKeyword', loc: DUMMY_LOC}
        : {type: 'TSStringKeyword', loc: DUMMY_LOC};

    const enumName = transform.Identifier(node.id, false);
    return [
      {
        type: 'TSEnumDeclaration',
        loc: DUMMY_LOC,
        const: false,
        declare: true,
        id: enumName,
        body: {
          type: 'TSEnumBody',
          members,
          loc: DUMMY_LOC,
        },
        members,
      },
      // flow also exports `.cast`, `.isValid`, `.members` and `.getName` for enums
      // we can use declaration merging to declare these functions on the enum:
      /*
      declare enum Foo {
        A = 1,
        B = 2,
      }
      declare namespace Foo {
        export function cast(value: number | null | undefined): Foo;
        export function isValid(value: number | null | undefined): value is Foo;
        export function members(): IterableIterator<Foo>;
        export function getName(value: Foo): string;
      }
      */
      {
        type: 'TSModuleDeclaration',
        loc: DUMMY_LOC,
        declare: true,
        id: enumName,
        kind: 'namespace',
        body: {
          type: 'TSModuleBlock',
          loc: DUMMY_LOC,
          body: [
            // export function cast(value: number | null | undefined): Foo
            {
              type: 'ExportNamedDeclaration',
              loc: DUMMY_LOC,
              declaration: {
                type: 'TSDeclareFunction',
                loc: DUMMY_LOC,
                id: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: 'cast',
                },
                generator: false,
                expression: false,
                async: false,
                params: [
                  {
                    type: 'Identifier',
                    loc: DUMMY_LOC,
                    name: 'value',
                    typeAnnotation: {
                      type: 'TSTypeAnnotation',
                      loc: DUMMY_LOC,
                      typeAnnotation: {
                        type: 'TSUnionType',
                        loc: DUMMY_LOC,
                        types: [
                          bodyRepresentationType,
                          {
                            type: 'TSNullKeyword',
                            loc: DUMMY_LOC,
                          },
                          {
                            type: 'TSUndefinedKeyword',
                            loc: DUMMY_LOC,
                          },
                        ],
                      },
                    },
                  },
                ],
                returnType: {
                  type: 'TSTypeAnnotation',
                  loc: DUMMY_LOC,
                  typeAnnotation: {
                    type: 'TSTypeReference',
                    loc: DUMMY_LOC,
                    typeName: enumName,
                  },
                },
              },
              specifiers: [],
              source: null,
              exportKind: 'value',
              assertions: [],
            },
            // export function isValid(value: number | null | undefined): value is Foo;
            {
              type: 'ExportNamedDeclaration',
              loc: DUMMY_LOC,
              declaration: {
                type: 'TSDeclareFunction',
                loc: DUMMY_LOC,
                id: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: 'isValid',
                },
                generator: false,
                expression: false,
                async: false,
                params: [
                  {
                    type: 'Identifier',
                    loc: DUMMY_LOC,
                    name: 'value',
                    typeAnnotation: {
                      type: 'TSTypeAnnotation',
                      loc: DUMMY_LOC,
                      typeAnnotation: {
                        type: 'TSUnionType',
                        loc: DUMMY_LOC,
                        types: [
                          bodyRepresentationType,
                          {
                            type: 'TSNullKeyword',
                            loc: DUMMY_LOC,
                          },
                          {
                            type: 'TSUndefinedKeyword',
                            loc: DUMMY_LOC,
                          },
                        ],
                      },
                    },
                  },
                ],
                returnType: {
                  type: 'TSTypeAnnotation',
                  loc: DUMMY_LOC,
                  typeAnnotation: {
                    type: 'TSTypePredicate',
                    loc: DUMMY_LOC,
                    asserts: false,
                    parameterName: {
                      type: 'Identifier',
                      loc: DUMMY_LOC,
                      name: 'value',
                    },
                    typeAnnotation: {
                      type: 'TSTypeAnnotation',
                      loc: DUMMY_LOC,
                      typeAnnotation: {
                        type: 'TSTypeReference',
                        loc: DUMMY_LOC,
                        typeName: enumName,
                      },
                    },
                  },
                },
              },
              specifiers: [],
              source: null,
              exportKind: 'value',
              assertions: [],
            },
            // export function members(): IterableIterator<Foo>;
            {
              type: 'ExportNamedDeclaration',
              loc: DUMMY_LOC,
              declaration: {
                type: 'TSDeclareFunction',
                loc: DUMMY_LOC,
                id: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: 'members',
                },
                generator: false,
                expression: false,
                async: false,
                params: [],
                returnType: {
                  type: 'TSTypeAnnotation',
                  loc: DUMMY_LOC,
                  typeAnnotation: {
                    type: 'TSTypeReference',
                    loc: DUMMY_LOC,
                    typeName: {
                      type: 'Identifier',
                      loc: DUMMY_LOC,
                      name: 'IterableIterator',
                    },
                    typeArguments: {
                      type: 'TSTypeParameterInstantiation',
                      loc: DUMMY_LOC,
                      params: [
                        {
                          type: 'TSTypeReference',
                          loc: DUMMY_LOC,
                          typeName: enumName,
                        },
                      ],
                    },
                  },
                },
              },
              specifiers: [],
              source: null,
              exportKind: 'value',
              assertions: [],
            },
            // export function getName(value: Foo): string;
            {
              type: 'ExportNamedDeclaration',
              loc: DUMMY_LOC,
              declaration: {
                type: 'TSDeclareFunction',
                loc: DUMMY_LOC,
                id: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: 'getName',
                },
                generator: false,
                expression: false,
                async: false,
                params: [
                  {
                    type: 'Identifier',
                    loc: DUMMY_LOC,
                    name: 'value',
                    typeAnnotation: {
                      type: 'TSTypeAnnotation',
                      loc: DUMMY_LOC,
                      typeAnnotation: {
                        type: 'TSTypeReference',
                        loc: DUMMY_LOC,
                        typeName: enumName,
                      },
                    },
                  },
                ],
                returnType: {
                  type: 'TSTypeAnnotation',
                  loc: DUMMY_LOC,
                  typeAnnotation: {
                    type: 'TSStringKeyword',
                    loc: DUMMY_LOC,
                  },
                },
              },
              specifiers: [],
              source: null,
              exportKind: 'value',
              assertions: [],
            },
          ],
        },
      },
    ];
  }

  const getPlaceholderNameForTypeofImport: () => string = (() => {
    let typeof_import_count = 0;
    return () => `$$IMPORT_TYPEOF_${++typeof_import_count}$$`;
  })();

  function transformImportTypeReference(
    node:
      | FlowESTree.ImportType
      | FlowESTree.Identifier
      | FlowESTree.QualifiedTypeIdentifier
      | FlowESTree.QualifiedTypeofIdentifier,
    typeArguments: TSESTree.TSTypeParameterInstantiation | null = null,
  ): TSESTree.TSImportType | null {
    const names: Array<FlowESTree.Identifier> = [];
    let current = node;
    while (
      current.type === 'QualifiedTypeIdentifier' ||
      current.type === 'QualifiedTypeofIdentifier'
    ) {
      names.unshift(current.id);
      current = current.qualification;
    }
    if (current.type !== 'ImportType') {
      return null;
    }

    let qualifier: TSESTree.EntityName | null = null;
    for (const name of names) {
      const identifier: TSESTree.Identifier = {
        type: 'Identifier',
        loc: DUMMY_LOC,
        name: name.name,
      };
      qualifier =
        qualifier == null
          ? identifier
          : {
              type: 'TSQualifiedName',
              loc: DUMMY_LOC,
              left: qualifier,
              right: identifier,
            };
    }

    const source: TSESTree.StringLiteral = {
      type: 'Literal',
      loc: DUMMY_LOC,
      raw: current.source.raw,
      value: current.source.value,
    };
    return {
      type: 'TSImportType',
      loc: DUMMY_LOC,
      argument: {
        type: 'TSLiteralType',
        loc: DUMMY_LOC,
        literal: source,
      },
      options: null,
      qualifier,
      source,
      typeArguments,
    };
  }

  const transformTypeAnnotationType = (
    node: FlowESTree.TypeAnnotationType,
  ): TSESTree.TypeNode => {
    switch (node.type) {
      case 'AnyTypeAnnotation':
        return transform.AnyTypeAnnotation(node);
      case 'ArrayTypeAnnotation':
        return transform.ArrayTypeAnnotation(node);
      case 'BigIntLiteralTypeAnnotation':
        return transform.BigIntLiteralTypeAnnotation(node);
      case 'BigIntTypeAnnotation':
        return transform.BigIntTypeAnnotation(node);
      case 'BooleanLiteralTypeAnnotation':
        return transform.BooleanLiteralTypeAnnotation(node);
      case 'BooleanTypeAnnotation':
        return transform.BooleanTypeAnnotation(node);
      case 'EmptyTypeAnnotation':
        return transform.EmptyTypeAnnotation(node);
      case 'ExistsTypeAnnotation':
        return transform.ExistsTypeAnnotation(node);
      case 'FunctionTypeAnnotation':
        return transform.FunctionTypeAnnotation(node);
      case 'GenericTypeAnnotation':
        return transform.GenericTypeAnnotation(node);
      case 'IndexedAccessType':
        return transform.IndexedAccessType(node);
      case 'InterfaceTypeAnnotation':
        return transform.InterfaceTypeAnnotation(node);
      case 'IntersectionTypeAnnotation':
        return transform.IntersectionTypeAnnotation(node);
      case 'MixedTypeAnnotation':
        return transform.MixedTypeAnnotation(node);
      case 'NullLiteralTypeAnnotation':
        return transform.NullLiteralTypeAnnotation(node);
      case 'NullableTypeAnnotation':
        return transform.NullableTypeAnnotation(node);
      case 'NumberLiteralTypeAnnotation':
        return transform.NumberLiteralTypeAnnotation(node);
      case 'NumberTypeAnnotation':
        return transform.NumberTypeAnnotation(node);
      case 'ObjectTypeAnnotation':
        return transform.ObjectTypeAnnotation(node);
      case 'OptionalIndexedAccessType':
        return transform.OptionalIndexedAccessType(node);
      case 'QualifiedTypeIdentifier':
        return transform.QualifiedTypeIdentifier(node);
      case 'StringLiteralTypeAnnotation':
        return transform.StringLiteralTypeAnnotation(node);
      case 'StringTypeAnnotation':
        return transform.StringTypeAnnotation(node);
      case 'SymbolTypeAnnotation':
        return transform.SymbolTypeAnnotation(node);
      case 'ThisTypeAnnotation':
        return transform.ThisTypeAnnotation(node);
      case 'TemplateLiteralTypeAnnotation':
        return transform.TemplateLiteralTypeAnnotation(node);
      case 'TupleTypeAnnotation':
        return transform.TupleTypeAnnotation(node);
      case 'TupleTypeElement':
        return transform.TupleTypeElement(node);
      case 'TypeofTypeAnnotation':
        return transform.TypeofTypeAnnotation(node);
      case 'UnionTypeAnnotation':
        return transform.UnionTypeAnnotation(node);
      case 'VoidTypeAnnotation':
        return transform.VoidTypeAnnotation(node);
      case 'TypePredicate':
        return transform.TypePredicateAnnotation(node);
      case 'ConditionalTypeAnnotation':
        return transform.ConditionalTypeAnnotation(node);
      case 'ConstructorTypeAnnotation':
        return transform.ConstructorTypeAnnotation(node);
      case 'InferTypeAnnotation':
        return transform.InferTypeAnnotation(node);
      case 'KeyofTypeAnnotation':
        return transform.KeyofTypeAnnotation(node);
      case 'TypeOperator':
        return transform.TypeOperator(node);
      case 'ComponentTypeAnnotation':
        return transform.ComponentTypeAnnotation(node);
      case 'NeverTypeAnnotation':
        return transform.NeverTypeAnnotation(node);
      case 'UndefinedTypeAnnotation':
        return transform.UndefinedTypeAnnotation(node);
      case 'UnknownTypeAnnotation':
        return transform.UnknownTypeAnnotation(node);
      default:
        throw unexpectedTranslationError(node, `Unhandled type ${node.type}`);
    }
  };

  /*
  Flow can export a bare type, eg
  ```
  declare export default TypeName;
  declare module.exports: TypeName;
  ```
  but TS can only export values, so declare a temporary variable to hold the
  type and reference that in the export declaration:
  ```
  declare const SPECIFIER: TypeName;
  declare type SPECIFIER = typeof SPECIFIER;
  ```
  */
  const declareTypeAsVariable = (
    specifier: string,
    type: FlowESTree.TypeAnnotationType,
  ): [TSESTree.VariableDeclaration, TSESTree.TSTypeAliasDeclaration] => [
    {
      type: 'VariableDeclaration',
      loc: DUMMY_LOC,
      declarations: [
        {
          type: 'VariableDeclarator',
          loc: DUMMY_LOC,
          id: {
            type: 'Identifier',
            loc: DUMMY_LOC,
            name: specifier,
            typeAnnotation: {
              type: 'TSTypeAnnotation',
              loc: DUMMY_LOC,
              typeAnnotation: transformTypeAnnotationType(type),
            },
          },
          init: null,
        },
      ],
      declare: true,
      kind: 'const',
    },
    {
      type: 'TSTypeAliasDeclaration',
      declare: true,
      id: {
        type: 'Identifier',
        decorators: [],
        name: specifier,
        optional: false,
        loc: DUMMY_LOC,
      },
      typeAnnotation: {
        type: 'TSTypeQuery',
        exprName: {
          type: 'Identifier',
          decorators: [],
          name: specifier,
          optional: false,
          loc: DUMMY_LOC,
        },
        loc: DUMMY_LOC,
      },
      loc: DUMMY_LOC,
    },
  ];

  // Whether `name` is bound at the top level by a class declaration, which is
  // the one case where `typeof name` can be re-exported as `name` itself and
  // keep both its value and its type meaning.
  const isDeclaredClass = (name: string): boolean => {
    const exportedVar = topScope.set.get(name);
    return (
      exportedVar != null &&
      exportedVar.defs.length === 1 &&
      exportedVar.defs[0].type === 'ClassName'
    );
  };

  const wellKnownSymbols = new Set([
    'iterator',
    'asyncIterator',
    'dispose',
    'asyncDispose',
  ]);

  const transform = {
    AnyTypeAnnotation(
      _node: FlowESTree.AnyTypeAnnotation,
    ): TSESTree.TSAnyKeyword {
      return {
        type: 'TSAnyKeyword',
        loc: DUMMY_LOC,
      };
    },
    ArrayTypeAnnotation(
      node: FlowESTree.ArrayTypeAnnotation,
    ): TSESTree.TSArrayType {
      return {
        type: 'TSArrayType',
        loc: DUMMY_LOC,
        elementType: transformTypeAnnotationType(node.elementType),
      };
    },
    BigIntLiteral(node: FlowESTree.BigIntLiteral): TSESTree.BigIntLiteral {
      return {
        type: 'Literal',
        loc: DUMMY_LOC,
        bigint: node.bigint,
        raw: node.raw,
        value: node.value,
      };
    },
    BigIntLiteralTypeAnnotation(
      node: FlowESTree.BigIntLiteralTypeAnnotation,
    ): TSESTree.TSLiteralType {
      // technically hermes doesn't support this yet
      // but future proofing amirite
      const bigint =
        // $FlowExpectedError[prop-missing]
        node.bigint ??
        node.raw
          // estree spec is to not have a trailing `n` on this property
          // https://github.com/estree/estree/blob/db962bb417a97effcfe9892f87fbb93c81a68584/es2020.md#bigintliteral
          .replace(/n$/, '')
          // `BigInt` doesn't accept numeric separator and `bigint` property should not include numeric separator
          .replace(/_/, '');
      return {
        type: 'TSLiteralType',
        loc: DUMMY_LOC,
        literal: {
          type: 'Literal',
          loc: DUMMY_LOC,
          value: node.value,
          raw: node.raw,
          bigint,
        } as TSESTree.BigIntLiteral,
      };
    },
    BigIntTypeAnnotation(
      _node: FlowESTree.BigIntTypeAnnotation,
    ): TSESTree.TSBigIntKeyword {
      return {
        type: 'TSBigIntKeyword',
        loc: DUMMY_LOC,
      };
    },
    BooleanLiteral(node: FlowESTree.BooleanLiteral): TSESTree.BooleanLiteral {
      return {
        type: 'Literal',
        loc: DUMMY_LOC,
        raw: node.raw,
        value: node.value,
      };
    },
    BooleanLiteralTypeAnnotation(
      node: FlowESTree.BooleanLiteralTypeAnnotation,
    ): TSESTree.TSLiteralType {
      return {
        type: 'TSLiteralType',
        loc: DUMMY_LOC,
        literal: {
          type: 'Literal',
          loc: DUMMY_LOC,
          value: node.value,
          raw: node.raw,
        } as TSESTree.BooleanLiteral,
      };
    },
    BooleanTypeAnnotation(
      _node: FlowESTree.BooleanTypeAnnotation,
    ): TSESTree.TSBooleanKeyword {
      return {
        type: 'TSBooleanKeyword',
        loc: DUMMY_LOC,
      };
    },
    ClassImplements(
      node: FlowESTree.ClassImplements,
    ): TSESTree.TSClassImplements {
      return {
        type: 'TSClassImplements',
        loc: DUMMY_LOC,
        expression:
          node.id.type === 'QualifiedTypeIdentifier'
            ? transform.QualifiedTypeIdentifier(node.id)
            : transform.Identifier(node.id, false),
        typeArguments: transform.TypeParameterInstantiation(
          node.typeParameters,
        ),
      };
    },
    DeclareClass(
      node: FlowESTree.DeclareClass,
    ): DeclarationOrUnsupported<TSESTree.ClassDeclarationWithName> {
      const classMembers: Array<TSESTree.ClassElement> = [];
      const transformedBody = transform.ObjectTypeAnnotation(node.body);
      if (transformedBody.type !== 'TSTypeLiteral') {
        return unsupportedDeclaration(
          node.body,
          'Spreads in declare class are not allowed',
          node.id,
          true,
          node.typeParameters,
        );
      }
      for (const member of transformedBody.members) {
        // TS uses the real ClassDeclaration AST so we need to
        // make the signatures real methods/properties
        switch (member.type) {
          case 'TSIndexSignature': {
            classMembers.push(member);
            break;
          }

          case 'TSMethodSignature': {
            // flow just creates a method signature like any other for a constructor
            // but the proper AST has `kind = 'constructor'`
            const isConstructor = (() => {
              if (member.computed === true) {
                return false;
              }

              return (
                (member.key.type === 'Identifier' &&
                  member.key.name === 'constructor') ||
                (member.key.type === 'Literal' &&
                  member.key.value === 'constructor')
              );
            })();
            if (isConstructor) {
              const newNode: TSESTree.MethodDefinitionAmbiguous = {
                type: 'MethodDefinition',
                loc: DUMMY_LOC,
                accessibility: undefined,
                computed: false,
                key: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: 'constructor',
                },
                kind: 'constructor',
                optional: false,
                override: false,
                static: false,
                value: {
                  type: 'TSEmptyBodyFunctionExpression',
                  loc: DUMMY_LOC,
                  async: false,
                  body: null,
                  declare: false,
                  expression: false,
                  generator: false,
                  id: null,
                  params: member.params,
                  // constructors explicitly have no return type
                  returnType: undefined,
                  typeParameters: member.typeParameters,
                },
              };
              cloneJSDocCommentsToNewNode(member, newNode);
              classMembers.push(newNode);
            } else {
              const [key, computed] = ((): [TSESTree.PropertyName, boolean] => {
                const _key = member.key;
                if (_key.type === 'Identifier' && _key.name.startsWith('@@')) {
                  const name = _key.name.slice(2);
                  if (wellKnownSymbols.has(name)) {
                    return [
                      {
                        type: 'MemberExpression',
                        computed: false,
                        object: {
                          type: 'Identifier',
                          name: 'Symbol',
                          optional: false,
                          loc: DUMMY_LOC,
                        },
                        optional: false,
                        property: {
                          type: 'Identifier',
                          name,
                          optional: false,
                          loc: DUMMY_LOC,
                        },
                        loc: DUMMY_LOC,
                      },
                      true,
                    ];
                  }
                }

                return [member.key, member.computed];
              })();

              const newNode: TSESTree.MethodDefinitionAmbiguous = {
                type: 'MethodDefinition',
                loc: DUMMY_LOC,
                accessibility: member.accessibility,
                computed: computed ?? false,
                key,
                kind: member.kind,
                optional: member.optional,
                override: false,
                static: member.static ?? false,
                value: {
                  type: 'TSEmptyBodyFunctionExpression',
                  loc: DUMMY_LOC,
                  async: false,
                  body: null,
                  declare: false,
                  expression: false,
                  generator: false,
                  id: null,
                  params: member.params,
                  returnType: member.returnType,
                  typeParameters: member.typeParameters,
                },
              };
              cloneJSDocCommentsToNewNode(member, newNode);
              classMembers.push(newNode);
            }
            break;
          }

          case 'TSPropertySignature': {
            const newNode: TSESTree.PropertyDefinitionAmbiguous = {
              type: 'PropertyDefinition',
              loc: DUMMY_LOC,
              accessibility: member.accessibility,
              computed: member.computed ?? false,
              declare: false,
              key: member.key,
              optional: member.optional,
              readonly: member.readonly,
              static: member.static ?? false,
              typeAnnotation: member.typeAnnotation,
              value: null,
            };
            cloneJSDocCommentsToNewNode(member, newNode);
            classMembers.push(newNode);
            break;
          }

          case 'TSCallSignatureDeclaration': {
            /*
            TODO - callProperties
            It's not valid to directly declare a call property on a class in TS
            You can do it, but it's a big complication in the AST:
            ```ts
            declare Class {
              // ...
            }
            interface ClassConstructor {
              new (): Class;
              // call sigs
              (): Type;
            }
            ```
            Let's put a pin in it for now and deal with it later if the need arises.
            */
            return unsupportedDeclaration(
              node.body.callProperties[0] ?? node.body,
              'call signatures on classes',
              node.id,
              true,
              node.typeParameters,
            );
          }

          default:
            throw unexpectedTranslationError(
              node.body,
              `Unexpected member type ${member.type}`,
            );
        }
      }

      const superClass = node.extends.length > 0 ? node.extends[0] : undefined;
      let superClassExpression: TSESTree.LeftHandSideExpression | null = null;
      let superTypeArguments: TSESTree.TSTypeParameterInstantiation | void;
      if (superClass != null) {
        switch (superClass.type) {
          case 'DeclareClassExtendsCall':
            return unsupportedDeclaration(
              superClass,
              'mixin class extends clauses',
              node.id,
              true,
              node.typeParameters,
            );
          case 'InterfaceExtends':
            superClassExpression =
              superClass.id.type === 'QualifiedTypeIdentifier'
                ? transform.QualifiedTypeIdentifier(superClass.id)
                : transform.Identifier(superClass.id as $FlowFixMe, false);
            superTypeArguments = transform.TypeParameterInstantiation(
              superClass.typeParameters,
            );
            break;
        }
      }

      return {
        type: 'ClassDeclaration',
        loc: DUMMY_LOC,
        body: {
          type: 'ClassBody',
          loc: DUMMY_LOC,
          body: classMembers,
        },
        declare: true,
        id: transform.Identifier(node.id, false),
        implements:
          node.implements == null
            ? undefined
            : node.implements.map(transform.ClassImplements),
        superClass: superClassExpression,
        superTypeArguments,
        typeParameters:
          node.typeParameters == null
            ? undefined
            : transform.TypeParameterDeclaration(node.typeParameters),
        // TODO - mixins??
      };
    },
    DeclareExportDeclaration(
      node: FlowESTree.DeclareExportDeclaration,
    ):
      | TSESTree.ExportNamedDeclaration
      | Array<TSESTree.ExportNamedDeclaration>
      | TSESTree.ExportDefaultDeclaration
      | TSESTree.ExportAllDeclaration
      | [
          (
            | TSESTree.VariableDeclaration
            | TSESTree.ClassDeclaration
            | TSESTree.TSDeclareFunction
            | TSESTree.TSTypeAliasDeclaration
          ),
          TSESTree.ExportDefaultDeclaration,
        ]
      | [
          (
            | TSESTree.VariableDeclaration
            | TSESTree.ClassDeclaration
            | TSESTree.TSDeclareFunction
            | TSESTree.TSTypeAliasDeclaration
          ),
          TSESTree.TSTypeAliasDeclaration,
          TSESTree.ExportDefaultDeclaration,
        ] {
      if (node.default === true) {
        const declaration = node.declaration;

        switch (declaration.type) {
          // TS doesn't support direct default export for declare'd classes
          case 'DeclareClass': {
            const classDecl = transform.DeclareClass(declaration);
            const name = declaration.id.name;
            return [
              classDecl,
              {
                type: 'ExportDefaultDeclaration',
                loc: DUMMY_LOC,
                declaration: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name,
                },
                exportKind: 'value',
              },
            ];
          }

          // TS doesn't support direct default export for declare'd functions
          case 'DeclareFunction': {
            const functionDecl = transform.DeclareFunction(declaration);
            if (declaration.id == null) {
              return {
                type: 'ExportDefaultDeclaration',
                loc: DUMMY_LOC,
                declaration: functionDecl,
                exportKind: 'value',
              };
            }
            const name = declaration.id.name;
            return [
              functionDecl,
              {
                type: 'ExportDefaultDeclaration',
                loc: DUMMY_LOC,
                declaration: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name,
                },
                exportKind: 'value',
              },
            ];
          }

          // TS doesn't support direct default export for declare'd functions
          case 'DeclareComponent': {
            const functionDecl = transform.DeclareComponent(declaration);
            const name = declaration.id.name;
            return [
              functionDecl,
              {
                type: 'ExportDefaultDeclaration',
                loc: DUMMY_LOC,
                declaration: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name,
                },
                exportKind: 'value',
              },
            ];
          }

          // TS doesn't support direct default export for declare'd functions
          case 'DeclareHook': {
            const functionDecl = transform.DeclareHook(declaration);
            const name = declaration.id.name;
            return [
              functionDecl,
              {
                type: 'ExportDefaultDeclaration',
                loc: DUMMY_LOC,
                declaration: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name,
                },
                exportKind: 'value',
              },
            ];
          }

          // Flow's declare export default Identifier is ambiguous.
          // the Identifier might reference a type, or it might reference a value
          // - If it's a value, then that's all good, TS supports that.
          // - If it's a type, that's a problem - TS only allows value variables to be exported
          //   so we need to create an intermediate variable to hold the type.

          case 'GenericTypeAnnotation': {
            const referencedId = declaration.id;

            // QualifiedTypeIdentifiers are types so cannot be handled without the intermediate variable so
            // only Identifiers can be handled here.
            if (referencedId.type === 'Identifier') {
              const exportedVar = topScope.set.get(referencedId.name);
              if (exportedVar != null && exportedVar.defs.length === 1) {
                const def = exportedVar.defs[0];
                switch (def.type) {
                  case 'ImportBinding': {
                    // `import type { Wut } from 'mod'; declare export default Wut;`
                    // `import { type Wut } from 'mod'; declare export default Wut;`
                    // these cases should be wrapped in a variable because they're exporting a type, not a value
                    const specifier = def.node;
                    if (
                      specifier.importKind === 'type' ||
                      specifier.parent.importKind === 'type'
                    ) {
                      // fallthrough to the "default" handling
                      break;
                    }

                    // intentional fallthrough to the "value" handling
                  }
                  case 'ClassName':
                  case 'Enum':
                  case 'FunctionName':
                  case 'ImplicitGlobalVariable':
                  case 'Variable':
                    // there's already a variable defined to hold the type
                    return {
                      type: 'ExportDefaultDeclaration',
                      loc: DUMMY_LOC,
                      declaration: {
                        type: 'Identifier',
                        loc: DUMMY_LOC,
                        name: referencedId.name,
                      },
                      exportKind: 'value',
                    };

                  case 'CatchClause':
                  case 'Parameter':
                  case 'TypeParameter':
                    throw translationError(
                      def.node,
                      `Unexpected variable def type: ${def.type}`,
                    );

                  case 'Type':
                    // fallthrough to the "default" handling
                    break;
                }
              }
            }

            // intentional fallthrough to the "default" handling
          }

          case 'TypeofTypeAnnotation': {
            if (
              declaration.type === 'TypeofTypeAnnotation' &&
              declaration.argument.type === 'Identifier' &&
              isDeclaredClass(declaration.argument.name)
            ) {
              return {
                type: 'ExportDefaultDeclaration',
                declaration: {
                  type: 'Identifier',
                  decorators: [],
                  name: declaration.argument.name,
                  optional: false,
                  loc: DUMMY_LOC,
                },
                exportKind: 'value',
                loc: DUMMY_LOC,
              };
            }

            // intentional fallthrough to the "default" handling
          }

          default: {
            const SPECIFIER = getDefaultExportBindingName(
              declaration,
              opts.useSemanticDefaultExportNames === true,
            );
            return [
              ...declareTypeAsVariable(SPECIFIER, declaration),
              {
                type: 'ExportDefaultDeclaration',
                loc: DUMMY_LOC,
                declaration: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: SPECIFIER,
                },
                exportKind: 'value',
              },
            ];
          }
        }
      } else {
        // eslint-disable-next-line eqeqeq
        if (node.source === null) {
          // eslint-disable-next-line eqeqeq
          if (node.declaration === null) {
            return {
              type: 'ExportNamedDeclaration',
              loc: DUMMY_LOC,
              // flow does not currently support attributes
              assertions: [],
              declaration: null,
              // flow does not support declared type exports with specifiers
              exportKind: 'value',
              source: null,
              specifiers: node.specifiers.map(specifier => {
                if (specifier.type === 'ExportNamespaceSpecifier') {
                  throw unexpectedTranslationError(
                    specifier,
                    'Unexpected export namespace specifier without a source',
                  );
                }
                return transform.ExportSpecifier(specifier);
              }),
            } as TSESTree.ExportNamedDeclarationWithoutSourceWithMultiple;
          }

          const declarations = ((): Array<{
            declaration: TSESTree.NamedExportDeclarations,
            exportKind: TSESTree.ExportKind,
          }> => {
            switch (node.declaration.type) {
              case 'DeclareClass':
                return [
                  {
                    declaration: transform.DeclareClass(node.declaration),
                    exportKind: 'value',
                  },
                ];
              case 'DeclareComponent':
                return [
                  {
                    declaration: transform.DeclareComponent(node.declaration),
                    exportKind: 'value',
                  },
                ];
              case 'DeclareHook':
                return [
                  {
                    declaration: transform.DeclareHook(node.declaration),
                    exportKind: 'value',
                  },
                ];
              case 'DeclareFunction':
                return [
                  {
                    declaration: transform.DeclareFunction(node.declaration),
                    exportKind: 'value',
                  },
                ];
              case 'DeclareInterface':
                return [
                  {
                    declaration: transform.DeclareInterface(node.declaration),
                    exportKind: 'type',
                  },
                ];
              case 'InterfaceDeclaration':
                return [
                  {
                    declaration: transform.InterfaceDeclaration(
                      node.declaration,
                    ),
                    exportKind: 'type',
                  },
                ];
              case 'DeclareNamespace':
                return [
                  {
                    declaration: unsupportedDeclaration(
                      node.declaration,
                      'namespaces',
                      node.declaration.id,
                    ),
                    exportKind: 'value',
                  },
                ];
              case 'DeclareOpaqueType':
                return [
                  {
                    declaration: transform.DeclareOpaqueType(node.declaration),
                    exportKind: 'type',
                  },
                ];
              case 'TypeAlias':
                return [
                  {
                    declaration: transform.TypeAlias(node.declaration),
                    exportKind: 'type',
                  },
                ];
              case 'DeclareVariable':
                return [
                  {
                    declaration: transform.DeclareVariable(node.declaration),
                    exportKind: 'value',
                  },
                ];
              case 'DeclareEnum': {
                const result = transform.DeclareEnum(node.declaration);
                return Array.isArray(result)
                  ? [
                      {
                        declaration: result[0],
                        exportKind: 'type',
                      },
                      {
                        declaration: result[1],
                        exportKind: 'type',
                      },
                    ]
                  : [{declaration: result, exportKind: 'type'}];
              }
            }
          })();

          const mappedDeclarations: Array<
            | TSESTree.ExportNamedDeclaration
            | Array<TSESTree.ExportNamedDeclaration>,
          > = declarations.map(({declaration, exportKind}) => {
            if (
              declaration.type === 'VariableDeclaration' &&
              declaration.declarations.length === 1
            ) {
              const ident = declaration.declarations[0].id;
              if (ident.type === 'Identifier') {
                const name = ident.name;
                return [
                  {
                    type: 'ExportNamedDeclaration',
                    loc: DUMMY_LOC,
                    // flow does not currently support attributes
                    assertions: [],
                    declaration,
                    exportKind,
                    source: null,
                    specifiers: [],
                  },
                  {
                    type: 'ExportNamedDeclaration',
                    declaration: {
                      type: 'TSTypeAliasDeclaration',
                      declare: true,
                      id: {
                        type: 'Identifier',
                        decorators: [],
                        name,
                        optional: false,
                        loc: DUMMY_LOC,
                      },
                      typeAnnotation: {
                        type: 'TSTypeQuery',
                        exprName: {
                          type: 'Identifier',
                          decorators: [],
                          name,
                          optional: false,
                          loc: DUMMY_LOC,
                        },
                        loc: DUMMY_LOC,
                      },
                      loc: DUMMY_LOC,
                    },
                    source: null,
                    loc: DUMMY_LOC,
                    specifiers: [],
                    exportKind: 'type',
                    // flow does not currently support attributes
                    assertions: [],
                  },
                ];
              }
            }

            const exportNamedDeclaration: TSESTree.ExportNamedDeclarationWithoutSourceWithSingle =
              {
                type: 'ExportNamedDeclaration',
                loc: DUMMY_LOC,
                // flow does not currently support attributes
                assertions: [],
                declaration,
                exportKind,
                source: null,
                specifiers: [],
              };
            return exportNamedDeclaration;
          });

          return mappedDeclarations.flat();
        } else {
          const namespaceSpecifier = node.specifiers.find(
            specifier => specifier.type === 'ExportNamespaceSpecifier',
          );
          if (namespaceSpecifier != null) {
            return {
              type: 'ExportAllDeclaration',
              loc: DUMMY_LOC,
              assertions: [],
              exportKind: 'value',
              source: transform.StringLiteral(node.source),
              exported: transform.Identifier(namespaceSpecifier.exported),
            };
          }
          return {
            type: 'ExportNamedDeclaration',
            loc: DUMMY_LOC,
            // flow does not currently support attributes
            assertions: [],
            declaration: null,
            // flow does not support declared type exports with a source
            exportKind: 'value',
            source: transform.StringLiteral(node.source),
            specifiers: node.specifiers.map(specifier => {
              if (specifier.type === 'ExportNamespaceSpecifier') {
                throw unexpectedTranslationError(
                  specifier,
                  'Unexpected export namespace specifier',
                );
              }
              return transform.ExportSpecifier(specifier);
            }),
          } as TSESTree.ExportNamedDeclarationWithSource;
        }
      }
    },
    DeclareComponent(
      node: FlowESTree.DeclareComponent,
    ): TSESTree.TSDeclareFunction {
      const id = transform.Identifier(node.id, false);

      const typeParameters =
        node.typeParameters == null
          ? undefined
          : transform.TypeParameterDeclaration(node.typeParameters);

      const [declaredParams, declaredRest] = normalizeDeclareComponentParams(
        node.params,
        node.rest,
        unexpectedTranslationError,
      );
      const params = transform.ComponentTypeParameters(
        declaredParams,
        declaredRest,
      );

      // TS cannot support `renderType` so we always use ReactNode as the return type.
      const hasReactImport = isReactImport(node, 'React');
      const returnType: TSESTree.TSTypeAnnotation = {
        type: 'TSTypeAnnotation',
        loc: DUMMY_LOC,
        // If no rendersType we assume its ReactNode type.
        typeAnnotation: {
          type: 'TSTypeReference',
          loc: DUMMY_LOC,
          typeName: {
            type: 'TSQualifiedName',
            loc: DUMMY_LOC,
            left: getReactIdentifier(hasReactImport),
            right: {
              type: 'Identifier',
              loc: DUMMY_LOC,
              name: `ReactNode`,
            },
          },
          typeArguments: undefined,
        },
      };

      return {
        type: 'TSDeclareFunction',
        loc: DUMMY_LOC,
        async: false,
        body: undefined,
        declare: true,
        expression: false,
        generator: false,
        id: {
          type: 'Identifier',
          loc: DUMMY_LOC,
          name: id.name,
        },
        params,
        returnType: returnType,
        typeParameters: typeParameters,
      };
    },
    ComponentTypeParameters(
      params: ReadonlyArray<FlowESTree.ComponentTypeParameter>,
      rest: FlowESTree.ComponentTypeParameter | null,
    ): ReadonlyArray<TSESTree.Parameter> {
      if (params.length === 0 && rest != null) {
        return [
          {
            type: 'Identifier',
            loc: DUMMY_LOC,
            name: 'props',
            typeAnnotation: {
              type: 'TSTypeAnnotation',
              loc: DUMMY_LOC,
              typeAnnotation: transformTypeAnnotationType(rest.typeAnnotation),
            },
            optional: false,
          },
        ];
      }

      const flowPropsType: Array<
        FlowESTree.ObjectTypeProperty | FlowESTree.ObjectTypeSpreadProperty,
      > = [];

      if (rest != null) {
        flowPropsType.push(
          constructFlowNode<FlowESTree.ObjectTypeSpreadProperty>({
            type: 'ObjectTypeSpreadProperty',
            argument: rest.typeAnnotation,
            range: rest.range,
            loc: rest.loc,
          }),
        );
      }

      for (let i = 0; i < params.length; i++) {
        const param = params[i];
        flowPropsType.push(
          constructFlowNode<FlowESTree.ObjectTypePropertySignature>({
            type: 'ObjectTypeProperty',
            kind: 'init',
            method: false,
            optional: param.optional,
            variance: null,
            proto: false,
            static: false,
            key:
              param.name ??
              constructFlowNode<FlowESTree.Identifier>({
                type: 'Identifier',
                name: `$$PARAM_${i}$$`,
                optional: false,
                typeAnnotation: null,
              }),
            value: param.typeAnnotation,
            range: param.range,
            loc: param.loc,
          }),
        );
      }
      const tsPropsObjectType = transform.ObjectTypeAnnotation(
        constructFlowNode<FlowESTree.ObjectTypeAnnotation>({
          type: 'ObjectTypeAnnotation',
          inexact: false,
          exact: true,
          properties: flowPropsType,
          indexers: [],
          callProperties: [],
          internalSlots: [],
        }),
      );
      return [
        {
          type: 'Identifier',
          loc: DUMMY_LOC,
          name: 'props',
          typeAnnotation: {
            type: 'TSTypeAnnotation',
            loc: DUMMY_LOC,
            typeAnnotation: tsPropsObjectType,
          },
          optional: false,
        },
      ];
    },
    DeclareHook(node: FlowESTree.DeclareHook): TSESTree.TSDeclareFunction {
      // the hook params/returnType are stored as an annotation on the ID...
      const id = transform.Identifier(node.id, false);
      const functionInfo = transform.FunctionTypeAnnotation(
        node.id.typeAnnotation.typeAnnotation,
      );

      return {
        type: 'TSDeclareFunction',
        loc: DUMMY_LOC,
        async: false,
        body: undefined,
        declare: true,
        expression: false,
        generator: false,
        id: {
          type: 'Identifier',
          loc: DUMMY_LOC,
          name: id.name,
        },
        params: functionInfo.params,
        returnType: functionInfo.returnType,
        typeParameters: functionInfo.typeParameters,
      };
    },
    DeclareFunction(
      node: FlowESTree.DeclareFunction,
    ): TSESTree.TSDeclareFunction {
      const annotation = node.id?.typeAnnotation ?? node.typeAnnotation;
      if (annotation == null) {
        throw translationError(node, 'Declare function is missing its type');
      }
      const functionType = annotation.typeAnnotation;
      const functionInfo = transform.FunctionTypeAnnotation(functionType);

      return {
        type: 'TSDeclareFunction',
        loc: DUMMY_LOC,
        async: false,
        body: undefined,
        declare: true,
        expression: false,
        generator: false,
        id:
          node.id == null
            ? null
            : {
                type: 'Identifier',
                loc: DUMMY_LOC,
                name: node.id.name,
              },
        params: functionInfo.params,
        returnType: functionInfo.returnType,
        typeParameters: functionInfo.typeParameters,
      };
    },
    DeclareInterface(
      node: FlowESTree.DeclareInterface | FlowESTree.InterfaceDeclaration,
    ): TSESTree.TSInterfaceDeclaration {
      const transformedBody = transform.ObjectTypeAnnotation(node.body);
      if (transformedBody.type !== 'TSTypeLiteral') {
        throw translationError(
          node.body,
          'Spreads in interfaces are not allowed',
        );
      }

      return {
        type: 'TSInterfaceDeclaration',
        loc: DUMMY_LOC,
        body: {
          type: 'TSInterfaceBody',
          loc: DUMMY_LOC,
          body: transformedBody.members,
        },
        declare: node.type !== 'InterfaceDeclaration',
        extends: node.extends.map(transform.InterfaceExtends),
        id: transform.Identifier(node.id, false),
        typeParameters:
          node.typeParameters == null
            ? undefined
            : transform.TypeParameterDeclaration(node.typeParameters),
      };
    },
    DeclareTypeAlias(
      node: FlowESTree.DeclareTypeAlias | FlowESTree.TypeAlias,
    ): TSESTree.TSTypeAliasDeclaration {
      return {
        type: 'TSTypeAliasDeclaration',
        loc: DUMMY_LOC,
        declare: node.type === 'DeclareTypeAlias',
        id: transform.Identifier(node.id, false),
        typeAnnotation: transformTypeAnnotationType(node.right),
        typeParameters:
          node.typeParameters == null
            ? undefined
            : transform.TypeParameterDeclaration(node.typeParameters),
      };
    },
    DeclareOpaqueType(
      node: FlowESTree.DeclareOpaqueType | FlowESTree.OpaqueType,
    ): TSESTree.TSTypeAliasDeclaration {
      // TS doesn't currently have nominal types - https://github.com/Microsoft/Typescript/issues/202
      // TODO - we could simulate this in a variety of ways
      // Examples - https://basarat.gitbook.io/typescript/main-1/nominaltyping

      if (node.supertype == null && node.typeParameters == null) {
        const name = `__${node.id.name}__`;
        return {
          type: 'TSTypeAliasDeclaration',
          loc: DUMMY_LOC,
          declare: true,
          id: transform.Identifier(node.id, false),
          typeAnnotation: {
            type: 'TSIntersectionType',
            types: [
              {
                type: 'TSSymbolKeyword',
                loc: DUMMY_LOC,
              },
              {
                type: 'TSTypeLiteral',
                loc: DUMMY_LOC,
                members: [
                  {
                    type: 'TSPropertySignature',
                    computed: false,
                    loc: DUMMY_LOC,
                    key: {
                      type: 'Identifier',
                      name: name,
                      loc: DUMMY_LOC,
                    },
                    typeAnnotation: {
                      type: 'TSTypeAnnotation',
                      loc: DUMMY_LOC,
                      typeAnnotation: {
                        type: 'TSStringKeyword',
                        loc: DUMMY_LOC,
                      },
                    },
                  },
                ],
              },
            ],
            loc: DUMMY_LOC,
          },
        };
      }

      return {
        type: 'TSTypeAliasDeclaration',
        loc: DUMMY_LOC,
        declare: true,
        id: transform.Identifier(node.id, false),
        typeAnnotation:
          node.supertype == null
            ? {
                type: 'TSUnknownKeyword',
                loc: DUMMY_LOC,
              }
            : transformTypeAnnotationType(node.supertype),
        typeParameters:
          node.typeParameters == null
            ? undefined
            : transform.TypeParameterDeclaration(node.typeParameters),
      };
    },
    DeclareVariable(
      node: FlowESTree.DeclareVariable,
    ): TSESTree.VariableDeclaration {
      return {
        type: 'VariableDeclaration',
        loc: DUMMY_LOC,
        declare: true,
        declarations: node.declarations.map(declaration => {
          if (declaration.id.type !== 'Identifier') {
            throw translationError(
              declaration.id,
              `DeclareVariable: unsupported declarator of type "${declaration.id.type}"`,
            );
          }
          return {
            type: 'VariableDeclarator',
            loc: DUMMY_LOC,
            declare: true,
            id: transform.Identifier(declaration.id, true),
            init: null,
          };
        }),
        kind: node.kind,
      };
    },
    DeclareEnum(
      node: FlowESTree.DeclareEnum,
    ): DeclarationOrUnsupported<
      [TSESTree.TSEnumDeclaration, TSESTree.TSModuleDeclaration],
    > {
      return EnumImpl(node);
    },
    EmptyTypeAnnotation(
      node: FlowESTree.EmptyTypeAnnotation,
    ): TSESTree.TypeNode {
      // Flow's `empty` type doesn't map well to any types in TS.
      // The closest is `never`, but `never` has a number of different semantics
      // In reality no human code should ever directly use the `empty` type in flow
      // So let's put a pin in it for now
      return unsupportedAnnotation(node, 'empty type');
    },
    EnumDeclaration(
      node: FlowESTree.EnumDeclaration,
    ): DeclarationOrUnsupported<
      [TSESTree.TSEnumDeclaration, TSESTree.TSModuleDeclaration],
    > {
      return EnumImpl(node);
    },
    DeclareModuleExports(
      node: FlowESTree.DeclareModuleExports,
    ):
      | TSESTree.TSExportAssignment
      | [
          TSESTree.VariableDeclaration,
          TSESTree.TSTypeAliasDeclaration,
          TSESTree.TSExportAssignment,
        ] {
      // TS forbids `export = ` in a module that has any other export, so a
      // module combining the two has no faithful translation.
      if (
        node.parent.type === 'Program' &&
        node.parent.body.some(
          statement =>
            statement.type === 'DeclareExportAllDeclaration' ||
            statement.type === 'DeclareExportDeclaration' ||
            statement.type === 'ExportAllDeclaration' ||
            statement.type === 'ExportDefaultDeclaration' ||
            statement.type === 'ExportNamedDeclaration',
        )
      ) {
        throw translationError(
          node,
          'CommonJS exports cannot be combined with ES module exports.',
        );
      }

      // `declare module.exports: T` is TS's `export = <value>`, which - like
      // `export default` - can only name a value.
      const type = node.typeAnnotation.typeAnnotation;

      if (
        type.type === 'TypeofTypeAnnotation' &&
        type.argument.type === 'Identifier' &&
        isDeclaredClass(type.argument.name)
      ) {
        return {
          type: 'TSExportAssignment',
          loc: DUMMY_LOC,
          expression: {
            type: 'Identifier',
            loc: DUMMY_LOC,
            name: type.argument.name,
          },
        };
      }

      const SPECIFIER = '$$MODULE_EXPORTS$$';
      return [
        ...declareTypeAsVariable(SPECIFIER, type),
        {
          type: 'TSExportAssignment',
          loc: DUMMY_LOC,
          expression: {
            type: 'Identifier',
            loc: DUMMY_LOC,
            name: SPECIFIER,
          },
        },
      ];
    },
    ExistsTypeAnnotation(
      node: FlowESTree.ExistsTypeAnnotation,
    ): TSESTree.TypeNode {
      // The existential type does not map to any types in TS
      // It's also super deprecated - so let's not ever worry
      return unsupportedAnnotation(node, 'existential type');
    },
    ExportAllDeclaration(
      node: FlowESTree.ExportAllDeclaration,
    ): TSESTree.ExportAllDeclaration {
      return {
        type: 'ExportAllDeclaration',
        loc: DUMMY_LOC,
        // flow does not currently support import/export attributes
        assertions: [],
        exportKind: node.exportKind,
        source: transform.StringLiteral(node.source),
        exported:
          node.exported == null ? null : transform.Identifier(node.exported),
      };
    },
    ExportNamedDeclaration(
      node: FlowESTree.ExportNamedDeclaration,
    ):
      | TSESTree.ExportNamedDeclarationAmbiguous
      | [TSESTree.ExportNamedDeclarationAmbiguous, ?TSESTree.Node] {
      if (node.source != null || node.specifiers.length > 0) {
        // can never have a declaration with a source
        return {
          type: 'ExportNamedDeclaration',
          loc: DUMMY_LOC,
          // flow does not currently support import/export attributes
          assertions: [],
          declaration: null,
          exportKind: node.exportKind,
          source:
            node.source == null ? null : transform.StringLiteral(node.source),
          specifiers: node.specifiers.map(transform.ExportSpecifier),
        };
      }

      const [exportedDeclaration, mergedDeclaration] = (() => {
        if (node.declaration == null) {
          return [null, null];
        }

        switch (node.declaration.type) {
          case 'ClassDeclaration':
          case 'ComponentDeclaration':
          case 'RecordDeclaration':
          case 'HookDeclaration':
          case 'FunctionDeclaration':
          case 'VariableDeclaration':
            // These cases shouldn't happen in flow defs because they have their own special
            // AST node (DeclareClass, DeclareFunction, DeclareVariable)
            throw unexpectedTranslationError(
              node.declaration,
              `Unexpected named declaration found ${node.declaration.type}`,
            );

          case 'EnumDeclaration': {
            const result = transform.EnumDeclaration(node.declaration);
            return Array.isArray(result) ? result : [result, null];
          }
          case 'InterfaceDeclaration':
            return [transform.InterfaceDeclaration(node.declaration), null];
          case 'OpaqueType':
            return [transform.OpaqueType(node.declaration), null];
          case 'TypeAlias':
            return [transform.TypeAlias(node.declaration), null];
        }
      })();

      const mainExport = {
        type: 'ExportNamedDeclaration' as 'ExportNamedDeclaration',
        loc: DUMMY_LOC,
        assertions: [],
        declaration: exportedDeclaration,
        exportKind: node.exportKind,
        source: null,
        specifiers: [],
      };

      if (mergedDeclaration != null) {
        // for cases where there is a merged declaration, TS enforces BOTH are exported
        return [
          mainExport,
          {
            type: 'ExportNamedDeclaration',
            loc: DUMMY_LOC,
            assertions: [],
            declaration: mergedDeclaration,
            exportKind: node.exportKind,
            source: null,
            specifiers: [],
          },
        ];
      }

      return mainExport;
    },
    ExportSpecifier(
      node: FlowESTree.ExportSpecifier,
    ): TSESTree.ExportSpecifier {
      return {
        type: 'ExportSpecifier',
        loc: DUMMY_LOC,
        exported: transform.Identifier(node.exported, false),
        local: transform.Identifier(node.local, false),
        exportKind: node.exportKind,
      };
    },
    ConstructorTypeAnnotation(
      node: FlowESTree.ConstructorTypeAnnotation,
    ): TSESTree.TSConstructorType {
      const params = node.params.map(transform.FunctionTypeParam);
      if (node.rest != null) {
        const rest = node.rest;
        params.push({
          type: 'RestElement',
          loc: DUMMY_LOC,
          argument:
            rest.name == null
              ? {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: '$$REST$$',
                }
              : transform.Identifier(rest.name, false),
          typeAnnotation: {
            type: 'TSTypeAnnotation',
            loc: DUMMY_LOC,
            typeAnnotation: transformTypeAnnotationType(rest.typeAnnotation),
          },
        });
      }

      return {
        type: 'TSConstructorType',
        loc: DUMMY_LOC,
        abstract: node.abstract,
        params,
        returnType: {
          type: 'TSTypeAnnotation',
          loc: DUMMY_LOC,
          typeAnnotation: transformTypeAnnotationType(node.returnType),
        },
        typeParameters:
          node.typeParameters == null
            ? undefined
            : transform.TypeParameterDeclaration(node.typeParameters),
      };
    },
    FunctionTypeAnnotation(
      node: FlowESTree.FunctionTypeAnnotation | FlowESTree.HookTypeAnnotation,
    ): TSESTree.TSFunctionType {
      const params = node.params.map(transform.FunctionTypeParam);
      if (node.type === 'FunctionTypeAnnotation' && node.this != null) {
        params.unshift({
          type: 'Identifier',
          loc: DUMMY_LOC,
          name: 'this',
          typeAnnotation: {
            type: 'TSTypeAnnotation',
            loc: DUMMY_LOC,
            typeAnnotation: transformTypeAnnotationType(
              node.this.typeAnnotation,
            ),
          },
        });
      }
      if (node.rest != null) {
        const rest = node.rest;
        params.push({
          type: 'RestElement',
          loc: DUMMY_LOC,
          argument:
            rest.name == null
              ? {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: '$$REST$$',
                }
              : transform.Identifier(rest.name, false),
          typeAnnotation: {
            type: 'TSTypeAnnotation',
            loc: DUMMY_LOC,
            typeAnnotation: transformTypeAnnotationType(rest.typeAnnotation),
          },
        });
      }

      return {
        type: 'TSFunctionType',
        loc: DUMMY_LOC,
        params,
        returnType: {
          type: 'TSTypeAnnotation',
          loc: DUMMY_LOC,
          typeAnnotation:
            node.returnType == null
              ? {type: 'TSAnyKeyword', loc: DUMMY_LOC}
              : transformTypeAnnotationType(node.returnType),
        },
        typeParameters:
          node.typeParameters == null
            ? undefined
            : transform.TypeParameterDeclaration(node.typeParameters),
      };
    },
    FunctionTypeParam(
      node: FlowESTree.FunctionTypeParam,
      idx: number = 0,
    ): TSESTree.Parameter {
      return {
        type: 'Identifier',
        loc: DUMMY_LOC,
        name: node.name == null ? `$$PARAM_${idx}$$` : node.name.name,
        typeAnnotation: {
          type: 'TSTypeAnnotation',
          loc: DUMMY_LOC,
          typeAnnotation: transformTypeAnnotationType(node.typeAnnotation),
        },
        optional: node.optional,
      };
    },
    GenericTypeAnnotation(
      node: FlowESTree.GenericTypeAnnotation,
    ): TSESTree.TypeNode {
      const importType = transformImportTypeReference(
        node.id,
        transform.TypeParameterInstantiation(node.typeParameters) ?? null,
      );
      if (importType != null) {
        return importType;
      }

      const [fullTypeName, baseId] = (() => {
        let names: Array<string> = [];
        let currentNode = node.id;

        while (currentNode != null) {
          switch (currentNode.type) {
            case 'Identifier': {
              names.unshift(currentNode.name);
              return [names.join('.'), currentNode];
            }
            case 'QualifiedTypeIdentifier': {
              names.unshift(currentNode.id.name);
              currentNode = currentNode.qualification;
              break;
            }
          }
        }

        throw translationError(
          node,
          `Invalid program state, types should only contain 'Identifier' and 'QualifiedTypeIdentifier' nodes.`,
        );
      })();

      const assertHasExactlyNTypeParameters = (
        count: number,
      ): ReadonlyArray<TSESTree.TypeNode> => {
        if (node.typeParameters != null) {
          if (node.typeParameters.params.length !== count) {
            throw translationError(
              node,
              `Expected exactly ${count} type parameter${
                count > 1 ? 's' : ''
              } with \`${fullTypeName}\``,
            );
          }

          const res = [];
          for (const param of node.typeParameters.params) {
            res.push(transformTypeAnnotationType(param));
          }
          return res;
        }

        if (count !== 0) {
          throw translationError(
            node,
            `Expected no type parameters with \`${fullTypeName}\``,
          );
        }

        return [];
      };

      function assertHasTypeParametersInRange(
        min: number,
        max: number,
      ): ReadonlyArray<TSESTree.TypeNode> {
        const {typeParameters} = node;
        if (typeParameters == null) {
          if (min > 0) {
            throw translationError(
              node,
              `Expected between ${min} and ${max} type parameters with \`${fullTypeName}\`, but got none.`,
            );
          }
          return [];
        }
        const params = typeParameters.params;
        if (params.length < min || params.length > max) {
          throw translationError(
            node,
            `Expected between ${min} and ${max} type parameters with \`${fullTypeName}\`, but got ${params.length}.`,
          );
        }

        return typeParameters.params.map(transformTypeAnnotationType);
      }

      switch (fullTypeName) {
        case '$Call':
        case '$ObjMap':
        case '$ObjMapConst':
        case '$ObjMapi':
        case '$TupleMap': {
          /*
          TODO - I don't think it's possible to make these types work in the generic case.

          TS has no utility types that allow you to generically mimic this functionality.
          You really need intimiate knowledge of the user's intent in order to correctly
          transform the code.

          For example the simple example for $Call from the flow docs:
          ```
          type ExtractPropType = <T>({prop: T}) => T;
          type Obj = {prop: number};
          type PropType = $Call<ExtractPropType, Obj>;
          // expected -- typeof PropType === number
          ```

          The equivalent in TS would be:
          ```
          type ExtractPropType<T extends { prop: any }> = (arg: T) => T['prop'];
          type Obj = { prop: number };
          type PropType = ReturnType<ExtractPropType<Obj>>; // number
          ```
          */
          return unsupportedAnnotation(node, fullTypeName);
        }

        case '$ArrayBufferView': {
          // `$ArrayBufferView` => `ArrayBufferView`
          return {
            type: 'TSTypeReference',
            loc: DUMMY_LOC,
            typeName: {
              type: 'Identifier',
              loc: DUMMY_LOC,
              name: 'ArrayBufferView',
            },
          };
        }

        case '$ArrayLike': {
          // `$ArrayLike<T>` => `ArrayLike<T>`
          return {
            type: 'TSTypeReference',
            loc: DUMMY_LOC,
            typeName: {
              type: 'Identifier',
              loc: DUMMY_LOC,
              name: 'ArrayLike',
            },
            typeArguments: {
              type: 'TSTypeParameterInstantiation',
              loc: DUMMY_LOC,
              params: assertHasExactlyNTypeParameters(1),
            },
          };
        }

        case '$Diff':
        case '$Rest': {
          // `$Diff<A, B>` => `Pick<A, Exclude<keyof A, keyof B>>`
          const params = assertHasExactlyNTypeParameters(2);
          return {
            type: 'TSTypeReference',
            loc: DUMMY_LOC,
            typeName: {
              type: 'Identifier',
              loc: DUMMY_LOC,
              name: 'Pick',
            },
            typeArguments: {
              type: 'TSTypeParameterInstantiation',
              loc: DUMMY_LOC,
              params: [
                params[0],
                {
                  type: 'TSTypeReference',
                  loc: DUMMY_LOC,
                  typeName: {
                    type: 'Identifier',
                    loc: DUMMY_LOC,
                    name: 'Exclude',
                  },
                  typeArguments: {
                    type: 'TSTypeParameterInstantiation',
                    loc: DUMMY_LOC,
                    params: [
                      {
                        type: 'TSTypeOperator',
                        loc: DUMMY_LOC,
                        operator: 'keyof',
                        typeAnnotation: params[0],
                      },
                      {
                        type: 'TSTypeOperator',
                        loc: DUMMY_LOC,
                        operator: 'keyof',
                        typeAnnotation: params[1],
                      },
                    ],
                  },
                },
              ],
            },
          };
        }

        case '$ElementType':
        case '$PropertyType': {
          // `$ElementType<T, K>` => `T[K]`
          const params = assertHasExactlyNTypeParameters(2);
          return {
            type: 'TSIndexedAccessType',
            loc: DUMMY_LOC,
            objectType: params[0],
            indexType: params[1],
          };
        }

        case '$Exact': {
          // `$Exact<T>` => `T`
          // TS has no concept of exact vs inexact types
          return assertHasExactlyNTypeParameters(1)[0];
        }

        case '$Exports': {
          // `$Exports<'module'>` => `typeof import('module')`
          const moduleName = assertHasExactlyNTypeParameters(1)[0];
          if (
            moduleName.type !== 'TSLiteralType' ||
            moduleName.literal.type !== 'Literal' ||
            typeof moduleName.literal.value !== 'string'
          ) {
            throw translationError(
              node,
              '$Exports must have a string literal argument',
            );
          }

          // New AST format for `typeof import('module')`
          return {
            type: 'TSTypeQuery',
            loc: DUMMY_LOC,
            exprName: {
              type: 'TSImportType',
              loc: DUMMY_LOC,
              argument: moduleName,
              qualifier: null,
              options: null,
              source: moduleName.literal,
              typeArguments: null,
            },
          } as $FlowFixMe;
        }

        case '$FlowFixMe': {
          // `$FlowFixMe` => `any`
          return {
            type: 'TSAnyKeyword',
            loc: DUMMY_LOC,
          };
        }

        case '$KeyMirror': {
          // `$KeyMirror<T>` => `{[K in keyof T]: K}`
          return {
            type: 'TSMappedType',
            loc: DUMMY_LOC,
            typeParameter: {
              type: 'TSTypeParameter',
              loc: DUMMY_LOC,
              name: {
                type: 'Identifier',
                loc: DUMMY_LOC,
                name: 'K',
              },
              constraint: {
                type: 'TSTypeOperator',
                loc: DUMMY_LOC,
                operator: 'keyof',
                typeAnnotation: assertHasExactlyNTypeParameters(1)[0],
              },
              in: false,
              out: false,
            },
            key: {
              type: 'Identifier',
              loc: DUMMY_LOC,
              name: 'K',
            },
            constraint: {
              type: 'TSTypeOperator',
              loc: DUMMY_LOC,
              operator: 'keyof',
              typeAnnotation: assertHasExactlyNTypeParameters(1)[0],
            },
            nameType: null,
            typeAnnotation: {
              type: 'TSTypeReference',
              loc: DUMMY_LOC,
              typeName: {
                type: 'Identifier',
                loc: DUMMY_LOC,
                name: 'K',
              },
            },
          };
        }

        case '$Keys': {
          // `$Keys<T>` => `keyof T`
          return {
            type: 'TSTypeOperator',
            loc: DUMMY_LOC,
            operator: 'keyof',
            typeAnnotation: assertHasExactlyNTypeParameters(1)[0],
          };
        }

        case '$NonMaybeType': {
          // `$NonMaybeType<T>` => `NonNullable<T>`
          // Not a great name because `NonNullable` also excludes `undefined`
          return {
            type: 'TSTypeReference',
            loc: DUMMY_LOC,
            typeName: {
              type: 'Identifier',
              loc: DUMMY_LOC,
              name: 'NonNullable',
            },
            typeArguments: {
              type: 'TSTypeParameterInstantiation',
              loc: DUMMY_LOC,
              params: assertHasExactlyNTypeParameters(1),
            },
          };
        }

        case '$ReadOnly': {
          // `$ReadOnly<T>` => `Readonly<T>`
          return {
            type: 'TSTypeReference',
            loc: DUMMY_LOC,
            typeName: {
              type: 'Identifier',
              loc: DUMMY_LOC,
              name: 'Readonly',
            },
            typeArguments: {
              type: 'TSTypeParameterInstantiation',
              loc: DUMMY_LOC,
              params: assertHasExactlyNTypeParameters(1),
            },
          };
        }

        case '$ReadOnlyArray': {
          // `$ReadOnlyArray<T>` => `ReadonlyArray<T>`
          //
          // we could also do    => `readonly T[]`
          // TODO - maybe a config option?
          return {
            type: 'TSTypeReference',
            loc: DUMMY_LOC,
            typeName: {
              type: 'Identifier',
              loc: DUMMY_LOC,
              name: 'ReadonlyArray',
            },
            typeArguments: {
              type: 'TSTypeParameterInstantiation',
              loc: DUMMY_LOC,
              params: assertHasExactlyNTypeParameters(1),
            },
          };
        }

        case '$ReadOnlyMap': {
          return {
            type: 'TSTypeReference',
            loc: DUMMY_LOC,
            typeName: {
              type: 'Identifier',
              loc: DUMMY_LOC,
              name: 'ReadonlyMap',
            },
            typeArguments: {
              type: 'TSTypeParameterInstantiation',
              loc: DUMMY_LOC,
              params: assertHasExactlyNTypeParameters(2),
            },
          };
        }

        case '$ReadOnlySet': {
          return {
            type: 'TSTypeReference',
            loc: DUMMY_LOC,
            typeName: {
              type: 'Identifier',
              loc: DUMMY_LOC,
              name: 'ReadonlySet',
            },
            typeArguments: {
              type: 'TSTypeParameterInstantiation',
              loc: DUMMY_LOC,
              params: assertHasExactlyNTypeParameters(1),
            },
          };
        }

        case '$Values':
        case 'Values': {
          // `$Values<T>` / `Values<T>` => `T[keyof T]`
          const transformedType = assertHasExactlyNTypeParameters(1)[0];
          return {
            type: 'TSIndexedAccessType',
            loc: DUMMY_LOC,
            objectType: transformedType,
            indexType: {
              type: 'TSTypeOperator',
              loc: DUMMY_LOC,
              operator: 'keyof',
              typeAnnotation: transformedType,
            },
          };
        }

        case 'Class': {
          // `Class<T>` => `new (...args: any[]) => T`
          const param = assertHasExactlyNTypeParameters(1)[0];
          if (param.type !== 'TSTypeReference') {
            throw translationError(
              node,
              'Expected a type reference within Class<T>',
            );
          }

          return {
            type: 'TSConstructorType',
            loc: DUMMY_LOC,
            abstract: false,
            params: [
              {
                type: 'RestElement',
                loc: DUMMY_LOC,
                argument: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: 'args',
                },
                typeAnnotation: {
                  type: 'TSTypeAnnotation',
                  loc: DUMMY_LOC,
                  typeAnnotation: {
                    type: 'TSArrayType',
                    loc: DUMMY_LOC,
                    elementType: {
                      type: 'TSAnyKeyword',
                      loc: DUMMY_LOC,
                    },
                  },
                },
              },
            ],
            returnType: {
              type: 'TSTypeAnnotation',
              loc: DUMMY_LOC,
              typeAnnotation: param,
            },
          };
        }

        case 'StringPrefix': {
          // `StringPrefix<foo>` => `foo${string}`
          // `StringPrefix<foo, T>` => `foo${T}`
          const params = assertHasTypeParametersInRange(1, 2);
          const prefix = params[0];
          if (
            prefix.type !== 'TSLiteralType' ||
            typeof prefix.literal.value !== 'string'
          ) {
            throw translationError(
              node,
              'Expected a string literal for the first type parameter.',
            );
          }
          const prefixStr = prefix.literal.value;
          const remainder = params[1] ?? {
            type: 'TSStringKeyword',
            loc: DUMMY_LOC,
          };

          return {
            type: 'TSTemplateLiteralType',
            loc: DUMMY_LOC,
            quasis: [
              {
                type: 'TemplateElement',
                loc: DUMMY_LOC,
                value: {
                  raw: prefixStr,
                  cooked: prefixStr,
                },
                tail: false,
              },
              {
                type: 'TemplateElement',
                loc: DUMMY_LOC,
                value: {
                  raw: '',
                  cooked: '',
                },
                tail: true,
              },
            ],
            types: [remainder],
          };
        }

        case 'StringSuffix': {
          // `StringSuffix<foo>` => `${string}foo`
          // `StringSuffix<foo, T>` => `${T}foo`
          const params = assertHasTypeParametersInRange(1, 2);
          const suffix = params[0];
          if (
            suffix.type !== 'TSLiteralType' ||
            typeof suffix.literal.value !== 'string'
          ) {
            throw translationError(
              node,
              'Expected a string literal for the first type parameter.',
            );
          }
          const suffixStr = suffix.literal.value;
          const remainder = params[1] ?? {
            type: 'TSStringKeyword',
            loc: DUMMY_LOC,
          };

          return {
            type: 'TSTemplateLiteralType',
            loc: DUMMY_LOC,
            quasis: [
              {
                type: 'TemplateElement',
                loc: DUMMY_LOC,
                value: {
                  raw: '',
                  cooked: '',
                },
                tail: false,
              },
              {
                type: 'TemplateElement',
                loc: DUMMY_LOC,
                value: {
                  raw: suffixStr,
                  cooked: suffixStr,
                },
                tail: true,
              },
            ],
            types: [remainder],
          };
        }
      }

      // React special conversion:
      const validReactImportOrGlobal = isValidReactImportOrGlobal(baseId);
      const hasReactImport = isReactImport(baseId, baseId.name);
      if (validReactImportOrGlobal || hasReactImport) {
        switch (fullTypeName) {
          // TODO: In flow this is `ChildrenArray<T> = T | ReadonlyArray<ChildrenArray<T>>`.
          // The recursive nature of it is rarely needed, so we're simplifying this for now
          // but omitting that aspect. Once we're able to provide utility types for our translations,
          // we should update this.
          // React.ChildrenArray<T> -> T | ReadonlyArray<T>
          // React$ChildrenArray<T> -> T | ReadonlyArray<T>
          case 'React.ChildrenArray':
          case 'React$ChildrenArray': {
            const [param] = assertHasExactlyNTypeParameters(1);
            return {
              type: 'TSUnionType',
              loc: DUMMY_LOC,
              types: [
                param,
                {
                  type: 'TSTypeReference',
                  loc: DUMMY_LOC,
                  typeName: {
                    type: 'Identifier',
                    loc: DUMMY_LOC,
                    name: 'ReadonlyArray',
                  },
                  typeArguments: {
                    type: 'TSTypeParameterInstantiation',
                    loc: DUMMY_LOC,
                    params: [param],
                  },
                },
              ],
            };
          }
          // React.Component<A,B> -> React.Component<A,B>
          // React$Component<A,B> -> React.Component<A,B>
          case 'React.Component':
          case 'React$Component': {
            const typeParameters = node.typeParameters;
            if (typeParameters == null || typeParameters.params.length === 0) {
              throw translationError(
                node,
                `Expected at least 1 type parameter with \`${fullTypeName}\``,
              );
            }
            const params = typeParameters.params;
            if (params.length > 2) {
              throw translationError(
                node,
                `Expected at no more than 2 type parameters with \`${fullTypeName}\``,
              );
            }

            return {
              type: 'TSTypeReference',
              loc: DUMMY_LOC,
              typeName: {
                type: 'TSQualifiedName',
                loc: DUMMY_LOC,
                left: getReactIdentifier(hasReactImport),
                right: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: 'Component',
                },
              },
              typeArguments: {
                type: 'TSTypeParameterInstantiation',
                loc: DUMMY_LOC,
                params: params.map(param => transformTypeAnnotationType(param)),
              },
            };
          }

          // React.Context<A> -> React.Context<A>
          // React$Context<A> -> React.Context<A>
          case 'React$Context':
          case 'React.Context':
            return {
              type: 'TSTypeReference',
              loc: DUMMY_LOC,
              typeName: {
                type: 'TSQualifiedName',
                loc: DUMMY_LOC,
                left: getReactIdentifier(hasReactImport),
                right: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: `Context`,
                },
              },
              typeArguments: {
                type: 'TSTypeParameterInstantiation',
                loc: DUMMY_LOC,
                params: assertHasExactlyNTypeParameters(1),
              },
            };
          // React.Key -> React.Key
          // React$Key -> React.Key
          case 'React.Key':
          case 'React$Key':
            assertHasExactlyNTypeParameters(0);
            return {
              type: 'TSTypeReference',
              loc: DUMMY_LOC,
              typeName: {
                type: 'TSQualifiedName',
                loc: DUMMY_LOC,
                left: getReactIdentifier(hasReactImport),
                right: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: 'Key',
                },
              },
            };
          // React.ElementType -> React.ElementType
          // React$ElementType -> React.ElementType
          case 'React$ElementType':
          case 'React.ElementType': {
            assertHasExactlyNTypeParameters(0);
            return {
              type: 'TSTypeReference',
              loc: DUMMY_LOC,
              typeName: {
                type: 'TSQualifiedName',
                loc: DUMMY_LOC,
                left: getReactIdentifier(hasReactImport),
                right: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: `ElementType`,
                },
              },
              typeArguments: undefined,
            };
          }
          // React.Node -> React.ReactNode
          case 'React$Node':
          case 'React.Node': {
            assertHasExactlyNTypeParameters(0);
            return {
              type: 'TSTypeReference',
              loc: DUMMY_LOC,
              typeName: {
                type: 'TSQualifiedName',
                loc: DUMMY_LOC,
                left: getReactIdentifier(hasReactImport),
                right: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: `ReactNode`,
                },
              },
              typeArguments: undefined,
            };
          }
          // React.Element<typeof Component> -> React.ReactElement<typeof Component>
          case 'React$Element':
          case 'React.Element': {
            return {
              type: 'TSTypeReference',
              loc: DUMMY_LOC,
              typeName: {
                type: 'TSQualifiedName',
                loc: DUMMY_LOC,
                left: getReactIdentifier(hasReactImport),
                right: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: `ReactElement`,
                },
              },
              typeArguments: {
                type: 'TSTypeParameterInstantiation',
                loc: DUMMY_LOC,
                params: assertHasExactlyNTypeParameters(1),
              },
            };
          }
          // React.ElementRef<typeof Component> -> React.ComponentRef<typeof Component>
          // React$ElementRef<typeof Component> -> React.ComponentRef<typeof Component>
          case 'React$ElementRef':
          case 'React.ElementRef':
            return {
              type: 'TSTypeReference',
              loc: DUMMY_LOC,
              typeName: {
                type: 'TSQualifiedName',
                loc: DUMMY_LOC,
                left: getReactIdentifier(hasReactImport),
                right: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: `ComponentRef`,
                },
              },
              typeArguments: {
                type: 'TSTypeParameterInstantiation',
                loc: DUMMY_LOC,
                params: assertHasExactlyNTypeParameters(1),
              },
            };
          // React$Fragment -> React.Fragment
          // React.Fragment -> React.Fragment
          case 'React$FragmentType':
          case 'React.Fragment':
            assertHasExactlyNTypeParameters(0);
            return {
              type: 'TSTypeReference',
              loc: DUMMY_LOC,
              typeName: {
                type: 'TSQualifiedName',
                loc: DUMMY_LOC,
                left: getReactIdentifier(hasReactImport),
                right: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: `Fragment`,
                },
              },
            };
          // React.MixedElement -> React.JSX.Element
          case 'React$MixedElement':
          case 'React.MixedElement': {
            assertHasExactlyNTypeParameters(0);
            return {
              type: 'TSTypeReference',
              loc: DUMMY_LOC,
              typeName: {
                type: 'TSQualifiedName',
                loc: DUMMY_LOC,
                left: {
                  type: 'TSQualifiedName',
                  loc: DUMMY_LOC,
                  left: getReactIdentifier(hasReactImport),
                  right: {
                    type: 'Identifier',
                    loc: DUMMY_LOC,
                    name: 'JSX',
                  },
                },
                right: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: 'Element',
                },
              },
              typeArguments: undefined,
            };
          }
          // React.ComponentType<Config> -> React.ComponentType<Config>
          // React$ComponentType<Config> -> React.ComponentType<Config>
          case 'React.ComponentType':
          case 'React$ComponentType': {
            return {
              type: 'TSTypeReference',
              loc: DUMMY_LOC,
              typeName: {
                type: 'TSQualifiedName',
                loc: DUMMY_LOC,
                left: getReactIdentifier(hasReactImport),
                right: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: 'ComponentType',
                },
              },
              typeArguments: {
                type: 'TSTypeParameterInstantiation',
                loc: DUMMY_LOC,
                params: assertHasExactlyNTypeParameters(1),
              },
            };
          }
          // React.AbstractComponent<Config> -> React.ComponentType<Config>
          // React$AbstractComponent<Config> -> React.ComponentType<Config>
          // React.AbstractComponent<Config, Instance> -> React.ComponentType<Config & React.RefAttributes<Instance>>
          // React$AbstractComponent<Config, Instance> -> React.ComponentType<Config & React.RefAttributes<Instance>>
          case 'React.AbstractComponent':
          case 'React$AbstractComponent': {
            const typeParameters = node.typeParameters;
            if (typeParameters == null || typeParameters.params.length === 0) {
              throw translationError(
                node,
                `Expected at least 1 type parameter with \`${fullTypeName}\``,
              );
            }
            const params = typeParameters.params;
            if (params.length > 3) {
              throw translationError(
                node,
                `Expected at no more than 3 type parameters with \`${fullTypeName}\``,
              );
            }

            const newParams = ((): ReadonlyArray<TSESTree.TypeNode> => {
              if (params.length === 1) {
                return assertHasExactlyNTypeParameters(1);
              }

              const props = transformTypeAnnotationType(params[0]);
              const ref = transformTypeAnnotationType(params[1]);

              return [
                {
                  type: 'TSIntersectionType',
                  loc: DUMMY_LOC,
                  types: [
                    props,
                    {
                      type: 'TSTypeReference',
                      loc: DUMMY_LOC,
                      typeName: {
                        type: 'TSQualifiedName',
                        loc: DUMMY_LOC,
                        left: {
                          type: 'Identifier',
                          loc: DUMMY_LOC,
                          name: 'React',
                        },
                        right: {
                          type: 'Identifier',
                          loc: DUMMY_LOC,
                          name: 'RefAttributes',
                        },
                      },
                      typeArguments: {
                        type: 'TSTypeParameterInstantiation',
                        loc: DUMMY_LOC,
                        params: [ref],
                      },
                    },
                  ],
                },
              ];
            })();

            return {
              type: 'TSTypeReference',
              loc: DUMMY_LOC,
              typeName: {
                type: 'TSQualifiedName',
                loc: DUMMY_LOC,
                left: getReactIdentifier(hasReactImport),
                right: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: 'ComponentType',
                },
              },
              typeArguments: {
                type: 'TSTypeParameterInstantiation',
                loc: DUMMY_LOC,
                params: newParams,
              },
            };
          }
          // React.ElementProps<A> -> React.ComponentProps<A>
          // React$ElementProps<A> -> React.ComponentProps<A>
          case 'React.ElementProps':
          case 'React$ElementProps': {
            return {
              type: 'TSTypeReference',
              loc: DUMMY_LOC,
              typeName: {
                type: 'TSQualifiedName',
                loc: DUMMY_LOC,
                left: getReactIdentifier(hasReactImport),
                right: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: 'ComponentProps',
                },
              },
              typeArguments: {
                type: 'TSTypeParameterInstantiation',
                loc: DUMMY_LOC,
                params: assertHasExactlyNTypeParameters(1),
              },
            };
          }
          // React.ComponentProps<A> -> React.JSX.LibraryManagedAttributes<A, React.ComponentProps<A>>
          case 'React.ComponentProps':
          case 'React.ElementConfig':
          case 'React$ElementConfig': {
            const [param] = assertHasExactlyNTypeParameters(1);
            return {
              type: 'TSTypeReference',
              loc: DUMMY_LOC,
              typeName: {
                type: 'TSQualifiedName',
                loc: DUMMY_LOC,
                left: {
                  type: 'TSQualifiedName',
                  loc: DUMMY_LOC,
                  left: getReactIdentifier(hasReactImport),
                  right: {
                    type: 'Identifier',
                    loc: DUMMY_LOC,
                    name: 'JSX',
                  },
                },
                right: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: 'LibraryManagedAttributes',
                },
              },
              typeArguments: {
                type: 'TSTypeParameterInstantiation',
                loc: DUMMY_LOC,
                params: [
                  param,
                  {
                    type: 'TSTypeReference',
                    loc: DUMMY_LOC,
                    typeName: {
                      type: 'TSQualifiedName',
                      loc: DUMMY_LOC,
                      left: getReactIdentifier(hasReactImport),
                      right: {
                        type: 'Identifier',
                        loc: DUMMY_LOC,
                        name: `ComponentProps`,
                      },
                    },
                    typeArguments: {
                      type: 'TSTypeParameterInstantiation',
                      loc: DUMMY_LOC,
                      params: [param],
                    },
                  },
                ],
              },
            };
          }
          // React.RefSetter<C> -> React.Ref<C>
          // React$RefSetter<C> -> React.Ref<C>
          case 'React.RefSetter':
          case 'React$RefSetter':
            return {
              type: 'TSTypeReference',
              loc: DUMMY_LOC,
              typeName: {
                type: 'TSQualifiedName',
                loc: DUMMY_LOC,
                left: getReactIdentifier(hasReactImport),
                right: {
                  type: 'Identifier',
                  loc: DUMMY_LOC,
                  name: 'Ref',
                },
              },
              typeArguments: {
                type: 'TSTypeParameterInstantiation',
                loc: DUMMY_LOC,
                params: assertHasExactlyNTypeParameters(1),
              },
            };
          default:
            return unsupportedAnnotation(node, fullTypeName);
        }
      }

      const typeName =
        node.id.type === 'Identifier'
          ? transform.Identifier(node.id, false)
          : node.id.type === 'QualifiedTypeIdentifier'
            ? transform.QualifiedTypeIdentifier(node.id)
            : null;
      if (typeName == null) {
        throw unexpectedTranslationError(node, 'Unreachable import type');
      }
      return {
        type: 'TSTypeReference',
        loc: DUMMY_LOC,
        typeName,
        typeArguments: transform.TypeParameterInstantiation(
          node.typeParameters,
        ),
      };
    },
    Identifier(
      node: FlowESTree.Identifier,
      includeTypeAnnotation: boolean = true,
    ): TSESTree.Identifier {
      return {
        type: 'Identifier',
        loc: DUMMY_LOC,
        name: node.name,
        ...(includeTypeAnnotation && node.typeAnnotation != null
          ? {
              typeAnnotation: transform.TypeAnnotation(node.typeAnnotation),
            }
          : {}),
      };
    },
    IndexedAccessType(
      node: FlowESTree.IndexedAccessType | FlowESTree.OptionalIndexedAccessType,
    ): TSESTree.TSIndexedAccessType {
      return {
        type: 'TSIndexedAccessType',
        loc: DUMMY_LOC,
        objectType: transformTypeAnnotationType(node.objectType),
        indexType: transformTypeAnnotationType(node.indexType),
      };
    },
    InterfaceDeclaration(
      node: FlowESTree.InterfaceDeclaration,
    ): TSESTree.TSInterfaceDeclaration {
      return transform.DeclareInterface(node);
    },
    ImportAttribute(
      node: FlowESTree.ImportAttribute,
    ): TSESTree.ImportAttribute {
      return {
        type: 'ImportAttribute',
        loc: DUMMY_LOC,
        key:
          node.key.type === 'Identifier'
            ? transform.Identifier(node.key)
            : transform.Literal(node.key),
        value: transform.Literal(node.value),
      };
    },
    ImportDeclaration(
      node: FlowESTree.ImportDeclaration,
    ): Array<DeclarationOrUnsupported<TSESTree.ImportDeclaration>> {
      const importKind = node.importKind;

      const specifiers: Array<TSESTree.ImportClause> = [];
      const unsupportedSpecifiers: Array<TSESTree.TSTypeAliasDeclaration> = [];
      node.specifiers.forEach(spec => {
        let id = (() => {
          if (node.importKind === 'typeof' || spec.importKind === 'typeof') {
            const id: TSESTree.Identifier = {
              type: 'Identifier',
              loc: DUMMY_LOC,
              name: getPlaceholderNameForTypeofImport(),
            };

            unsupportedSpecifiers.push({
              type: 'TSTypeAliasDeclaration',
              loc: DUMMY_LOC,
              id: transform.Identifier(spec.local, false),
              typeAnnotation: {
                type: 'TSTypeQuery',
                loc: DUMMY_LOC,
                exprName: id,
              },
            });

            return id;
          }

          return transform.Identifier(spec.local, false);
        })();

        switch (spec.type) {
          case 'ImportDefaultSpecifier':
            specifiers.push({
              type: 'ImportDefaultSpecifier',
              loc: DUMMY_LOC,
              local: id,
            });
            return;

          case 'ImportNamespaceSpecifier':
            specifiers.push({
              type: 'ImportNamespaceSpecifier',
              loc: DUMMY_LOC,
              local: id,
            });
            return;

          case 'ImportSpecifier':
            specifiers.push({
              type: 'ImportSpecifier',
              loc: DUMMY_LOC,
              importKind:
                spec.importKind === 'typeof' || spec.importKind === 'type'
                  ? 'type'
                  : null,
              imported: transform.Identifier(spec.imported, false),
              local: id,
            });
            return;
        }
      });

      const out: Array<TSESTree.ImportDeclaration> = specifiers.length
        ? [
            {
              type: 'ImportDeclaration',
              loc: DUMMY_LOC,
              assertions: node.attributes.map(transform.ImportAttribute),
              importKind:
                importKind === 'typeof' ? 'type' : (importKind ?? 'value'),
              source: transform.StringLiteral(node.source),
              specifiers,
            },
          ]
        : [];

      return [...out, ...unsupportedSpecifiers];
    },
    InterfaceExtends(
      node: FlowESTree.InterfaceExtends,
    ): TSESTree.TSInterfaceHeritage {
      return {
        type: 'TSInterfaceHeritage',
        loc: DUMMY_LOC,
        expression:
          node.id.type === 'QualifiedTypeIdentifier'
            ? transform.QualifiedTypeIdentifier(node.id)
            : transform.Identifier(node.id, false),
        typeArguments: transform.TypeParameterInstantiation(
          node.typeParameters,
        ),
      };
    },
    InterfaceTypeAnnotation(
      node: FlowESTree.InterfaceTypeAnnotation,
    ): TSESTree.TypeNode {
      if (node.extends != null && node.extends.length > 0) {
        // type T = interface extends U, V { ... }
        // to
        // type T = U & V & { ... }
        return {
          type: 'TSIntersectionType',
          loc: DUMMY_LOC,
          types: [
            ...node.extends.map(ex => ({
              type: 'TSTypeReference',
              loc: DUMMY_LOC,
              typeName:
                ex.id.type === 'QualifiedTypeIdentifier'
                  ? transform.QualifiedTypeIdentifier(ex.id)
                  : transform.Identifier(ex.id, false),
              typeArguments: transform.TypeParameterInstantiation(
                ex.typeParameters,
              ),
            })),
            transform.ObjectTypeAnnotation(node.body),
          ],
        };
      }

      return transform.ObjectTypeAnnotation(node.body);
    },
    IntersectionTypeAnnotation(
      node: FlowESTree.IntersectionTypeAnnotation,
    ): TSESTree.TSIntersectionType {
      return {
        type: 'TSIntersectionType',
        loc: DUMMY_LOC,
        types: node.types.map(transformTypeAnnotationType),
      };
    },
    Literal(node: FlowESTree.Literal): TSESTree.Literal {
      switch (node.literalType) {
        case 'bigint':
          return transform.BigIntLiteral(node);
        case 'boolean':
          return transform.BooleanLiteral(node);
        case 'null':
          return transform.NullLiteral(node);
        case 'numeric':
          return transform.NumericLiteral(node);
        case 'regexp':
          return transform.RegExpLiteral(node);
        case 'string':
          return transform.StringLiteral(node);
      }
    },
    MixedTypeAnnotation(
      _node: FlowESTree.MixedTypeAnnotation,
    ): TSESTree.TSUnknownKeyword {
      return {
        type: 'TSUnknownKeyword',
        loc: DUMMY_LOC,
      };
    },
    NullLiteral(_node: FlowESTree.NullLiteral): TSESTree.NullLiteral {
      return {
        type: 'Literal',
        loc: DUMMY_LOC,
        raw: 'null',
        value: null,
      };
    },
    NullLiteralTypeAnnotation(
      _node: FlowESTree.NullLiteralTypeAnnotation,
    ): TSESTree.TSNullKeyword {
      return {
        type: 'TSNullKeyword',
        loc: DUMMY_LOC,
      };
    },
    NullableTypeAnnotation(
      node: FlowESTree.NullableTypeAnnotation,
    ): TSESTree.TSUnionType {
      // TS doesn't support the maybe type, so have to explicitly union in `null | undefined`
      // `?T` becomes `null | undefined | T`
      return {
        type: 'TSUnionType',
        loc: DUMMY_LOC,
        types: [
          {
            type: 'TSNullKeyword',
            loc: DUMMY_LOC,
          },
          {
            type: 'TSUndefinedKeyword',
            loc: DUMMY_LOC,
          },
          transformTypeAnnotationType(node.typeAnnotation),
        ],
      };
    },
    NumberLiteralTypeAnnotation(
      node: FlowESTree.NumberLiteralTypeAnnotation,
    ): TSESTree.TSLiteralType {
      return {
        type: 'TSLiteralType',
        loc: DUMMY_LOC,
        literal: {
          type: 'Literal',
          loc: DUMMY_LOC,
          value: node.value,
          raw: node.raw,
        } as TSESTree.NumberLiteral,
      };
    },
    NumberTypeAnnotation(
      _node: FlowESTree.NumberTypeAnnotation,
    ): TSESTree.TSNumberKeyword {
      return {
        type: 'TSNumberKeyword',
        loc: DUMMY_LOC,
      };
    },
    NumericLiteral(node: FlowESTree.NumericLiteral): TSESTree.NumberLiteral {
      return {
        type: 'Literal',
        loc: DUMMY_LOC,
        raw: node.raw,
        value: node.value,
      };
    },
    ObjectTypeAnnotation(
      node: FlowESTree.ObjectTypeAnnotation,
    ):
      | TSESTree.TSTypeLiteral
      | TSESTree.TSIntersectionType
      | TSESTree.TSAnyKeyword
      | TSESTree.TSMappedType {
      if (
        node.properties.length === 1 &&
        node.properties[0].type === 'ObjectTypeMappedTypeProperty'
      ) {
        // Mapped Object Object types must not have other object properties.
        const prop: FlowESTree.ObjectTypeMappedTypeProperty =
          node.properties[0];
        const tsProp: TSESTree.TSMappedType = {
          type: 'TSMappedType',
          loc: DUMMY_LOC,
          typeParameter: {
            type: 'TSTypeParameter',
            loc: DUMMY_LOC,
            name: {
              type: 'Identifier',
              loc: DUMMY_LOC,
              name: prop.keyTparam.name,
            },
            constraint: transformTypeAnnotationType(prop.sourceType),
            in: false,
            out: false,
          },
          key: {
            type: 'Identifier',
            loc: DUMMY_LOC,
            name: prop.keyTparam.name,
          },
          constraint: transformTypeAnnotationType(prop.sourceType),
          readonly:
            prop.varianceOp ??
            (prop.variance?.kind === 'plus' ||
              prop.variance?.kind === 'readonly'),
          optional: prop.optional === 'Optional',
          typeAnnotation: transformTypeAnnotationType(prop.propType),
          nameType:
            prop.nameType == null
              ? null
              : transformTypeAnnotationType(prop.nameType),
        };

        return tsProp;
      }

      // we want to preserve the source order of the members
      // unfortunately flow has unordered properties storing things
      // so store all elements with their start index and sort the
      // list afterward
      const members: Array<{start: number, node: TSESTree.TypeElement}> = [];

      for (const callProp of node.callProperties) {
        members.push({
          start: callProp.range[0],
          node: transform.ObjectTypeCallProperty(callProp),
        });
      }

      for (const indexer of node.indexers) {
        members.push({
          start: indexer.range[0],
          node: transform.ObjectTypeIndexer(indexer),
        });
      }

      /*
      TODO - internalSlots
      I don't think there's anything analogous in TS.
      They're really rarely used (if ever) - so let's just ignore them for now
      */
      if (node.internalSlots.length > 0) {
        return unsupportedAnnotation(node.internalSlots[0], 'internal slots');
      }

      if (!node.properties.find(FlowESTree.isObjectTypeSpreadProperty)) {
        for (const property of node.properties) {
          if (property.type === 'ObjectTypeSpreadProperty') {
            // this is imposible due to the above find condition
            // this check is purely to satisfy flow
            throw unexpectedTranslationError(property, 'Impossible state');
          }

          if (property.type === 'ObjectTypeMappedTypeProperty') {
            return unsupportedAnnotation(
              property,
              'object type with mapped type property along with other properties',
            );
          }

          if (property.type === 'ObjectTypePrivateField') {
            return unsupportedAnnotation(property, 'private type fields');
          }

          members.push({
            start: property.range[0],
            node: transform.ObjectTypeProperty(property),
          });
        }

        const tsBody = members
          .sort((a, b) => a.start - b.start)
          .map(({node}) => node);

        return {
          type: 'TSTypeLiteral',
          loc: DUMMY_LOC,
          members: tsBody,
        };
      } else {
        /*
        spreads are a complicate thing for us to handle, sadly.
        in flow type spreads are modelled after object spreads; meaning that for
        { ...A, ...B } - all properties in B will explicitly replace any properties
        in A of the same name.
        ```
        type T1 = { a: string };
        type T2 = { a: number };
        type T3 = { ...T1, ...T2 };
        type A = T3['a'] // === number
        ```

        however in TS there are no object type spreads - you can only merge
        objects either via the intersection operator or via interface extends.

        For an interface extends - `interface B extends A { ... }` - TS enforces
        that the properties of B are all covariantly related to the same named
        properties in A.
        So we can't use an interface extends.

        For a type intersection - `type T = A & B;` - TS will (essentially) merge
        the types by intersecting each same named property in each type to calculate
        the resulting type. This has the effect of enforcing the same constraint
        as the interface case, however instead of an error it causes properties to
        become `never`:
        ```
        type T1 = { a: string };
        type T2 = { a: number };
        type T3 = T1 & T2;
        type A = T3['a'] // === string & number === never
        ```

        So in order for us to model flow's spreads we have to explicitly omit the keys
        from the proceeding type that might clash. We can do this pretty easily using
        TS's utility types:
        ```
        type T1 = { a: string };
        type T2 = { a: number };
        type T3 = Omit<T1, keyof T2> & T2;
        type A = T3['a'] // === number
        ```

        Unfortunately because we need to solve for the general case object type usage,
        it's going to be a bit ugly and complicated, sadly.

        If we had access to type information we would be able to skip some ugliness by
        checking to see if there is any overlap and skipping the omit step if there isn't.
        But alas - we're working purely syntactically.

        ```
        type T = { ...T1, b: string  };
        // becomes
        type T = Omit<T1, 'b'> & { b: string };
        ```
        ```
        type T = { ...T1, ...T2, ...T3, b: string  };
        // becomes
        type T =
          & Omit<T1, keyof T2 | keyof T3 | 'b'>
          & Omit<T2, keyof T3 | 'b'>
          & Omit<T3, 'b'>
          & { b: string };
        ```

        Note that because it's going to be super ugly and complicated - for now we're going to disallow:
        - spreads in the middle
        - spreads at the end
        - spreads of things that aren't "Identifiers"
        */

        if (members.length > 0) {
          return unsupportedAnnotation(
            node,
            'object types with spreads, indexers and/or call properties at the same time',
          );
        }

        const typesToIntersect = [];
        for (const property of node.properties) {
          if (property.type === 'ObjectTypeSpreadProperty') {
            if (members.length > 0) {
              return unsupportedAnnotation(
                property,
                'object types with spreads in the middle or at the end',
              );
            }

            const spreadType = transformTypeAnnotationType(property.argument);
            if (
              spreadType.type !== 'TSTypeReference' &&
              spreadType.type !== 'TSTypeQuery'
            ) {
              return unsupportedAnnotation(
                property,
                'object types with complex spreads',
              );
            }

            typesToIntersect.push(spreadType);
          } else if (property.type === 'ObjectTypeMappedTypeProperty') {
            // TODO - Support mapped types
            return unsupportedAnnotation(
              property,
              'object type with mapped type property',
            );
          } else if (property.type === 'ObjectTypePrivateField') {
            return unsupportedAnnotation(property, 'private type fields');
          } else {
            members.push({
              start: property.range[0],
              node: transform.ObjectTypeProperty(property),
            });
          }
        }

        const tsBody = members
          .sort((a, b) => a.start - b.start)
          .map(({node}) => node);
        const objectType: TSESTree.TypeNode = {
          type: 'TSTypeLiteral',
          loc: DUMMY_LOC,
          members: tsBody,
        };

        // When the non-spread properties have statically known key
        // names, emit them as string literal types directly instead of
        // wrapping in `keyof`. Falls back to `keyof objectType` when
        // any key is computed or otherwise non-extractable.
        const objectKeyTypes: ReadonlyArray<TSESTree.TypeNode> =
          extractPropertyKeyLiterals(tsBody) ?? [
            {
              type: 'TSTypeOperator',
              loc: DUMMY_LOC,
              operator: 'keyof',
              typeAnnotation: objectType,
            },
          ];

        const intersectionMembers: Array<TSESTree.TypeNode> = [];
        for (let i = 0; i < typesToIntersect.length; i += 1) {
          const currentType = typesToIntersect[i];
          const remainingTypes = typesToIntersect.slice(i + 1);
          const omitKeyTypes: Array<TSESTree.TypeNode> = [
            ...remainingTypes.map(t => ({
              type: 'TSTypeOperator',
              loc: DUMMY_LOC,
              operator: 'keyof',
              typeAnnotation: t,
            })),
            ...objectKeyTypes,
          ];
          intersectionMembers.push({
            type: 'TSTypeReference',
            loc: DUMMY_LOC,
            typeName: {
              type: 'Identifier',
              loc: DUMMY_LOC,
              name: 'Omit',
            },
            typeArguments: {
              type: 'TSTypeParameterInstantiation',
              loc: DUMMY_LOC,
              params: [
                currentType,
                omitKeyTypes.length === 1
                  ? omitKeyTypes[0]
                  : {
                      type: 'TSUnionType',
                      loc: DUMMY_LOC,
                      types: omitKeyTypes,
                    },
              ],
            },
          });
        }
        intersectionMembers.push(objectType);

        return {
          type: 'TSIntersectionType',
          loc: DUMMY_LOC,
          types: intersectionMembers,
        };
      }
    },
    ObjectTypeCallProperty(
      node: FlowESTree.ObjectTypeCallProperty,
    ): TSESTree.TSCallSignatureDeclaration {
      // the info is stored on the "value"
      const func = transform.FunctionTypeAnnotation(node.value);
      return {
        type: 'TSCallSignatureDeclaration',
        loc: DUMMY_LOC,
        params: func.params,
        returnType: func.returnType,
        typeParameters: func.typeParameters,
      };
    },
    ObjectTypeIndexer(
      node: FlowESTree.ObjectTypeIndexer,
    ): TSESTree.TSIndexSignature | TSESTree.TSPropertySignatureComputedName {
      if (node.key.type === 'GenericTypeAnnotation') {
        if (node.key.id.type === 'ImportType') {
          throw translationError(node, 'Unsupported import type indexer key');
        }
        const ident =
          node.key.id.type === 'Identifier' ? node.key.id : node.key.id.id;
        return {
          type: 'TSPropertySignature',
          computed: true,
          loc: DUMMY_LOC,
          key: {
            type: 'BinaryExpression',
            operator: 'in',
            loc: DUMMY_LOC,
            left: {
              type: 'Identifier',
              name: node.id == null ? '$$Key$$' : node.id.name,
              loc: DUMMY_LOC,
            },
            right: {
              type: 'Identifier',
              name: ident.name,
              loc: DUMMY_LOC,
            },
          },
          readonly:
            node.variance?.kind === 'plus' ||
            node.variance?.kind === 'readonly',
          static: node.static,
          typeAnnotation: {
            type: 'TSTypeAnnotation',
            loc: DUMMY_LOC,
            typeAnnotation: transformTypeAnnotationType(node.value),
          },
        };
      }
      return {
        type: 'TSIndexSignature',
        loc: DUMMY_LOC,
        parameters: [
          {
            type: 'Identifier',
            loc: DUMMY_LOC,
            name: node.id == null ? '$$Key$$' : node.id.name,
            typeAnnotation: {
              type: 'TSTypeAnnotation',
              loc: DUMMY_LOC,
              typeAnnotation: transformTypeAnnotationType(node.key),
            },
          },
        ],
        readonly:
          node.variance?.kind === 'plus' || node.variance?.kind === 'readonly',
        static: node.static,
        typeAnnotation: {
          type: 'TSTypeAnnotation',
          loc: DUMMY_LOC,
          typeAnnotation: transformTypeAnnotationType(node.value),
        },
      };
    },
    ObjectTypeProperty(
      node: FlowESTree.ObjectTypeProperty,
    ): TSESTree.TSPropertySignature | TSESTree.TSMethodSignature {
      const value = node.value;
      if (value == null) {
        throw translationError(
          node,
          'Object type property is missing its type',
        );
      }
      const [key, computed] = ((): [TSESTree.PropertyName, boolean] => {
        if (node.key.type === 'Identifier' && node.key.name.startsWith('@@')) {
          const name = node.key.name.slice(2);
          if (node.method === true && wellKnownSymbols.has(name)) {
            return [
              {
                type: 'MemberExpression',
                computed: false,
                object: {
                  type: 'Identifier',
                  name: 'Symbol',
                  optional: false,
                  loc: DUMMY_LOC,
                },
                optional: false,
                property: {
                  type: 'Identifier',
                  name,
                  optional: false,
                  loc: DUMMY_LOC,
                },
                loc: DUMMY_LOC,
              },
              true,
            ];
          }
        }

        const key =
          node.key.type === 'Identifier'
            ? transform.Identifier(node.key)
            : node.key.type === 'Literal' && node.key.literalType === 'string'
              ? transform.StringLiteral(node.key)
              : null;

        if (key == null) {
          throw unexpectedTranslationError(node, 'Unsupported key type');
        }

        return [key, false];
      })();

      if (node.method === true) {
        // flow has just one node for all object properties and relies upon the method flag
        // TS has separate nodes for methods and properties
        const func = transform.FunctionTypeAnnotation(node.value);
        // $FlowFixMe[incompatible-type] `computed` is set dynamically
        return {
          type: 'TSMethodSignature',
          loc: DUMMY_LOC,
          computed,
          key,
          kind: node.kind === 'init' ? 'method' : node.kind,
          optional: node.optional,
          params: func.params,
          returnType: func.returnType,
          static: node.static,
          typeParameters: func.typeParameters,
        };
      }

      if (node.kind === 'get' || node.kind === 'set') {
        // flow treats getters/setters as true property signatures (method === false)
        // TS treats them as method signatures
        const func = transform.FunctionTypeAnnotation(node.value);
        // $FlowFixMe[incompatible-type] `computed` is set dynamically
        return {
          type: 'TSMethodSignature',
          loc: DUMMY_LOC,
          computed,
          key,
          kind: node.kind,
          optional: false,
          params: func.params,
          // TS setters must not have a return type
          returnType: node.kind === 'set' ? undefined : func.returnType,
          static: node.static,
          // TS accessors cannot have type parameters
          typeParameters: undefined,
        };
      }

      // $FlowFixMe[incompatible-type] `computed` is set dynamically
      return {
        type: 'TSPropertySignature',
        loc: DUMMY_LOC,
        computed,
        key,
        optional: node.optional,
        readonly:
          node.variance?.kind === 'plus' || node.variance?.kind === 'readonly',
        static: node.static,
        typeAnnotation: {
          type: 'TSTypeAnnotation',
          loc: DUMMY_LOC,
          typeAnnotation:
            node.optional === true
              ? ensureTypeIncludesUndefined(transformTypeAnnotationType(value))
              : transformTypeAnnotationType(value),
        },
      };
    },
    OpaqueType(node: FlowESTree.OpaqueType): TSESTree.TSTypeAliasDeclaration {
      return transform.DeclareOpaqueType(node);
    },
    OptionalIndexedAccessType(
      node: FlowESTree.OptionalIndexedAccessType,
    ): TSESTree.TSIndexedAccessType {
      // Foo?.[A][B]
      // ^^^^^^^^    optional = true
      // ^^^^^^^^^^^ optional = false
      if (node.optional === false) {
        return transform.IndexedAccessType(node);
      }

      // TS doesn't support optional index access so we have to wrap the object:
      // `T?.[K]` becomes `NonNullable<T>[K]`
      return {
        type: 'TSIndexedAccessType',
        loc: DUMMY_LOC,
        objectType: {
          type: 'TSTypeReference',
          loc: DUMMY_LOC,
          typeName: {
            type: 'Identifier',
            loc: DUMMY_LOC,
            name: 'NonNullable',
          },
          typeArguments: {
            type: 'TSTypeParameterInstantiation',
            loc: DUMMY_LOC,
            params: [transformTypeAnnotationType(node.objectType)],
          },
        },
        indexType: transformTypeAnnotationType(node.indexType),
      };
    },
    QualifiedTypeIdentifier(
      node: FlowESTree.QualifiedTypeIdentifier,
    ): TSESTree.TSQualifiedName {
      const qual = node.qualification;
      if (qual.type === 'ImportType') {
        throw unexpectedTranslationError(qual, 'Unreachable import type');
      }
      const left =
        qual.type === 'Identifier'
          ? transform.Identifier(qual, false)
          : transform.QualifiedTypeIdentifier(qual);

      return {
        type: 'TSQualifiedName',
        loc: DUMMY_LOC,
        left,
        right: transform.Identifier(node.id, false),
      };
    },
    QualifiedTypeofIdentifier(
      node: FlowESTree.QualifiedTypeofIdentifier,
    ): TSESTree.TSQualifiedName {
      const qual = node.qualification;
      if (qual.type === 'ImportType') {
        throw unexpectedTranslationError(qual, 'Unreachable import type');
      }
      const left =
        qual.type === 'Identifier'
          ? transform.Identifier(qual, false)
          : transform.QualifiedTypeofIdentifier(qual);

      return {
        type: 'TSQualifiedName',
        loc: DUMMY_LOC,
        left,
        right: transform.Identifier(node.id, false),
      };
    },
    RegExpLiteral(node: FlowESTree.RegExpLiteral): TSESTree.RegExpLiteral {
      return {
        type: 'Literal',
        loc: DUMMY_LOC,
        raw: node.raw,
        regex: {
          pattern: node.regex.pattern,
          flags: node.regex.pattern,
        },
        value: node.value,
      };
    },
    StringLiteral(node: FlowESTree.StringLiteral): TSESTree.StringLiteral {
      return {
        type: 'Literal',
        loc: DUMMY_LOC,
        raw: node.raw,
        value: node.value,
      };
    },
    StringLiteralTypeAnnotation(
      node: FlowESTree.StringLiteralTypeAnnotation,
    ): TSESTree.TSLiteralType {
      return {
        type: 'TSLiteralType',
        loc: DUMMY_LOC,
        literal: {
          type: 'Literal',
          loc: DUMMY_LOC,
          value: node.value,
          raw: node.raw,
        } as TSESTree.StringLiteral,
      };
    },
    StringTypeAnnotation(
      _node: FlowESTree.StringTypeAnnotation,
    ): TSESTree.TSStringKeyword {
      return {
        type: 'TSStringKeyword',
        loc: DUMMY_LOC,
      };
    },
    SymbolTypeAnnotation(
      _node: FlowESTree.SymbolTypeAnnotation,
    ): TSESTree.TSSymbolKeyword {
      return {
        type: 'TSSymbolKeyword',
        loc: DUMMY_LOC,
      };
    },
    ThisTypeAnnotation(
      _node: FlowESTree.ThisTypeAnnotation,
    ): TSESTree.TSThisType {
      return {
        type: 'TSThisType',
        loc: DUMMY_LOC,
      };
    },
    TemplateLiteralTypeAnnotation(
      node: FlowESTree.TemplateLiteralTypeAnnotation,
    ): TSESTree.TSTemplateLiteralType {
      return {
        type: 'TSTemplateLiteralType',
        loc: DUMMY_LOC,
        quasis: node.quasis.map(quasi => ({
          type: 'TemplateElement',
          loc: DUMMY_LOC,
          tail: quasi.tail,
          value: {
            cooked: quasi.value.cooked,
            raw: quasi.value.raw,
          },
        })),
        types: node.types.map(transformTypeAnnotationType),
      };
    },
    TupleTypeAnnotation(
      node: FlowESTree.TupleTypeAnnotation,
    ): TSESTree.TSTupleType | TSESTree.TSTypeOperator {
      const allReadOnly =
        node.elementTypes.length > 0 &&
        node.elementTypes.every(
          element =>
            element.type === 'TupleTypeLabeledElement' &&
            element.variance != null &&
            (element.variance.kind === 'plus' ||
              element.variance.kind === 'readonly'),
        );
      const elems = node.elementTypes.map((element): TSESTree.TypeNode => {
        switch (element.type) {
          case 'TupleTypeLabeledElement':
            if (!allReadOnly && element.variance != null) {
              return unsupportedAnnotation(
                element,
                'tuple type element variance annotations',
              );
            }
            return {
              type: 'TSNamedTupleMember',
              loc: DUMMY_LOC,
              label: transform.Identifier(element.label),
              optional: element.optional,
              elementType: transformTypeAnnotationType(element.elementType),
            };
          case 'TupleTypeSpreadElement': {
            const annot = transformTypeAnnotationType(element.typeAnnotation);
            return {
              type: 'TSRestType',
              loc: DUMMY_LOC,
              typeAnnotation:
                element.label != null
                  ? {
                      type: 'TSNamedTupleMember',
                      loc: DUMMY_LOC,
                      label: transform.Identifier(element.label),
                      optional: false,
                      elementType: annot,
                    }
                  : annot,
            };
          }
          default:
            return transformTypeAnnotationType(element);
        }
      });

      const elementTypes: Array<TSESTree.TypeNode> = node.inexact
        ? [
            ...elems,
            {
              type: 'TSRestType',
              loc: DUMMY_LOC,
              typeAnnotation: {
                type: 'TSArrayType',
                loc: DUMMY_LOC,
                elementType: {
                  type: 'TSUnknownKeyword',
                  loc: DUMMY_LOC,
                },
              },
            },
          ]
        : elems;

      const tupleAnnot: TSESTree.TSTupleType = {
        type: 'TSTupleType',
        loc: DUMMY_LOC,
        elementTypes,
      };
      return allReadOnly
        ? {
            type: 'TSTypeOperator',
            loc: DUMMY_LOC,
            operator: 'readonly',
            typeAnnotation: tupleAnnot,
          }
        : tupleAnnot;
    },
    TupleTypeElement(node: FlowESTree.TupleTypeElement): TSESTree.TypeNode {
      const elementType = transformTypeAnnotationType(node.elementType);
      return node.optional
        ? {
            type: 'TSOptionalType',
            loc: DUMMY_LOC,
            typeAnnotation: elementType,
          }
        : elementType;
    },
    TypeAlias(node: FlowESTree.TypeAlias): TSESTree.TSTypeAliasDeclaration {
      return transform.DeclareTypeAlias(node);
    },
    TypeAnnotation(node: FlowESTree.TypeAnnotation): TSESTree.TSTypeAnnotation {
      return {
        type: 'TSTypeAnnotation',
        loc: DUMMY_LOC,
        typeAnnotation: transformTypeAnnotationType(node.typeAnnotation),
      };
    },
    TypeofTypeAnnotation(
      node: FlowESTree.TypeofTypeAnnotation,
    ): TSESTree.TSTypeQuery {
      if (node.argument.type === 'ImportType') {
        const importType = transformImportTypeReference(node.argument);
        if (importType == null) {
          throw unexpectedTranslationError(node, 'Invalid import type');
        }
        return {
          type: 'TSTypeQuery',
          loc: DUMMY_LOC,
          exprName: importType,
          typeArguments: undefined,
        };
      }
      const importType =
        node.argument.type === 'QualifiedTypeofIdentifier'
          ? transformImportTypeReference(node.argument)
          : null;
      if (importType != null) {
        return {
          type: 'TSTypeQuery',
          loc: DUMMY_LOC,
          exprName: importType,
          typeArguments: undefined,
        };
      }
      switch (node.argument.type) {
        case 'Identifier':
          return {
            type: 'TSTypeQuery',
            loc: DUMMY_LOC,
            exprName: transform.Identifier(node.argument),
            typeArguments: undefined,
          };
        case 'QualifiedTypeofIdentifier':
          return {
            type: 'TSTypeQuery',
            loc: DUMMY_LOC,
            exprName: transform.QualifiedTypeofIdentifier(node.argument),
            typeArguments: undefined,
          };
        default:
          throw unexpectedTranslationError(
            node.argument,
            'Unexpected typeof annotation argument',
          );
      }
    },
    TypeParameter(node: FlowESTree.TypeParameter): TSESTree.TSTypeParameter {
      /*
      TODO - flow models variance as explicit syntax, but but TS resolves it automatically
      TS does have syntax for explicit variance, but you can introduce a TS error if the
      marked parameter isn't used in the location that TS expects them to be in.

      To make it easier for now let's just rely on TS's auto-resolution.

      ```
      const variance =
        new Set(
          node.variance == null
            ? // by default flow generics act invariantly
              ['in', 'out']
            : node.variance.kind === 'plus'
            ? // covariant
              ['out']
            : // contravariant
              ['in'],
        );
      ```
      */
      return {
        type: 'TSTypeParameter',
        loc: DUMMY_LOC,
        name: {
          type: 'Identifier',
          loc: DUMMY_LOC,
          name: node.name,
        },
        constraint:
          node.bound == null
            ? undefined
            : transformTypeAnnotationType(node.bound),
        default:
          node.default == null
            ? undefined
            : transformTypeAnnotationType(node.default),
        in: false,
        out: false,
        // in: variance.has('in'),
        // out: variance.has('out'),
      };
    },
    TypeParameterDeclaration(
      node: FlowESTree.TypeParameterDeclaration,
    ): TSESTree.TSTypeParameterDeclaration {
      return {
        type: 'TSTypeParameterDeclaration',
        loc: DUMMY_LOC,
        params: node.params.map(transform.TypeParameter),
      };
    },
    TypeParameterInstantiation(
      node: ?FlowESTree.TypeParameterInstantiation,
    ): TSESTree.TSTypeParameterInstantiation | void {
      // Empty parameters in Flow are valid, but TS requires at least one parameter.
      // This ensures empty type parameters are not created in TS
      if (node == null || node.params.length === 0) {
        return undefined;
      }

      return {
        type: 'TSTypeParameterInstantiation',
        loc: DUMMY_LOC,
        params: node.params.map(transformTypeAnnotationType),
      };
    },
    UnionTypeAnnotation(
      node: FlowESTree.UnionTypeAnnotation,
    ): TSESTree.TSUnionType {
      return {
        type: 'TSUnionType',
        loc: DUMMY_LOC,
        types: node.types.map(transformTypeAnnotationType),
      };
    },
    VoidTypeAnnotation(
      _node: FlowESTree.VoidTypeAnnotation,
    ): TSESTree.TSVoidKeyword {
      return {
        type: 'TSVoidKeyword',
        loc: DUMMY_LOC,
      };
    },
    NeverTypeAnnotation(
      _node: FlowESTree.NeverTypeAnnotation,
    ): TSESTree.TSNeverKeyword {
      return {
        type: 'TSNeverKeyword',
        loc: DUMMY_LOC,
      };
    },
    UndefinedTypeAnnotation(
      _node: FlowESTree.UndefinedTypeAnnotation,
    ): TSESTree.TSUndefinedKeyword {
      return {
        type: 'TSUndefinedKeyword',
        loc: DUMMY_LOC,
      };
    },
    UnknownTypeAnnotation(
      _node: FlowESTree.UnknownTypeAnnotation,
    ): TSESTree.TSUnknownKeyword {
      return {
        type: 'TSUnknownKeyword',
        loc: DUMMY_LOC,
      };
    },
    ConditionalTypeAnnotation(
      node: FlowESTree.ConditionalTypeAnnotation,
    ): TSESTree.TSConditionalType {
      return {
        type: 'TSConditionalType',
        loc: DUMMY_LOC,
        checkType: transformTypeAnnotationType(node.checkType),
        extendsType: transformTypeAnnotationType(node.extendsType),
        trueType: transformTypeAnnotationType(node.trueType),
        falseType: transformTypeAnnotationType(node.falseType),
      };
    },
    TypePredicateAnnotation(
      node: FlowESTree.TypePredicate,
    ): TSESTree.TSTypePredicate {
      return {
        type: 'TSTypePredicate',
        loc: DUMMY_LOC,
        asserts: node.kind != null && node.kind === 'asserts',
        parameterName: transform.Identifier(node.parameterName, false),
        typeAnnotation: node.typeAnnotation && {
          type: 'TSTypeAnnotation',
          loc: DUMMY_LOC,
          typeAnnotation: transformTypeAnnotationType(node.typeAnnotation),
        },
      };
    },
    InferTypeAnnotation(
      node: FlowESTree.InferTypeAnnotation,
    ): TSESTree.TSInferType {
      return {
        type: 'TSInferType',
        loc: DUMMY_LOC,
        typeParameter: transform.TypeParameter(node.typeParameter),
      };
    },
    KeyofTypeAnnotation(
      node: FlowESTree.KeyofTypeAnnotation,
    ): TSESTree.TSTypeOperator {
      return {
        type: 'TSTypeOperator',
        loc: DUMMY_LOC,
        operator: 'keyof',
        typeAnnotation: transformTypeAnnotationType(node.argument),
      };
    },
    TypeOperator(node: FlowESTree.TypeOperator): TSESTree.TypeNode {
      switch (node.operator) {
        case 'readonly':
        case 'unique':
          return {
            type: 'TSTypeOperator',
            loc: DUMMY_LOC,
            operator: node.operator,
            typeAnnotation: transformTypeAnnotationType(node.typeAnnotation),
          };
        case 'renders':
        case 'renders?':
        case 'renders*': {
          const hasReactImport = isReactImport(node, 'React');
          return {
            type: 'TSTypeReference',
            loc: DUMMY_LOC,
            typeName: {
              type: 'TSQualifiedName',
              loc: DUMMY_LOC,
              left: getReactIdentifier(hasReactImport),
              right: {
                type: 'Identifier',
                loc: DUMMY_LOC,
                name: `ReactNode`,
              },
            },
            typeArguments: undefined,
          };
        }
      }
    },
    ComponentTypeAnnotation(
      node: FlowESTree.ComponentTypeAnnotation,
    ): TSESTree.TSFunctionType {
      const typeParameters =
        node.typeParameters == null
          ? undefined
          : transform.TypeParameterDeclaration(node.typeParameters);

      const params = transform.ComponentTypeParameters(node.params, node.rest);

      // TS cannot support `renderType` so we always use ReactNode as the return type.
      const hasReactImport = isReactImport(node, 'React');
      const returnType: TSESTree.TSTypeAnnotation = {
        type: 'TSTypeAnnotation',
        loc: DUMMY_LOC,
        // If no rendersType we assume its ReactNode type.
        typeAnnotation: {
          type: 'TSTypeReference',
          loc: DUMMY_LOC,
          typeName: {
            type: 'TSQualifiedName',
            loc: DUMMY_LOC,
            left: getReactIdentifier(hasReactImport),
            right: {
              type: 'Identifier',
              loc: DUMMY_LOC,
              name: `ReactNode`,
            },
          },
          typeArguments: undefined,
        },
      };

      return {
        type: 'TSFunctionType',
        loc: DUMMY_LOC,
        typeParameters,
        params,
        returnType,
      };
    },
  };

  // wrap each transform so that we automatically preserve jsdoc comments
  // this just saves us manually wiring up every single case
  for (const key of Object.keys(transform)) {
    const originalFn: $FlowFixMe = transform[key];
    // $FlowExpectedError[cannot-write]
    // $FlowExpectedError[missing-local-annot]
    transform[key] = (node, ...args) => {
      const result = originalFn(node, ...args);
      if (node != null && result != null) {
        if (Array.isArray(result)) {
          cloneJSDocCommentsToNewNode(node, result[0]);
        } else {
          cloneJSDocCommentsToNewNode(node, result);
        }
      }
      return result;
    };
  }

  return [transform, code];
};
