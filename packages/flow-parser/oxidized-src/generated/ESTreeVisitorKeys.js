/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @noflow
 * @format
 * @generated
 */

/*
 * !!! GENERATED FILE !!!
 *
 * Any manual changes to this file will be overwritten. To regenerate run `buck run fbcode//flow/rust_port/crates/flow_parser_wasm:codegen -- --estree-visitor-keys`.
 */

// lint directives to let us do some basic validation of generated files
/* eslint no-undef: 'error', no-unused-vars: ['error', {vars: "local"}], no-redeclare: 'error' */
/* global NonNullable, Partial, Readonly, ReadonlyArray, $FlowFixMe */

'use strict';

module.exports = {
  AbstractMethodDefinition: [
    'key',
    'value',
  ],
  AbstractPropertyDefinition: [
    'key',
    'value',
    'typeAnnotation',
    'variance',
  ],
  AnyTypeAnnotation: [],
  ArrayExpression: [
    'elements',
  ],
  ArrayPattern: [
    'elements',
    'typeAnnotation',
  ],
  ArrayTypeAnnotation: [
    'elementType',
  ],
  ArrowFunctionExpression: [
    'params',
    'body',
    'typeParameters',
    'returnType',
    'predicate',
  ],
  AsConstExpression: [
    'expression',
  ],
  AsExpression: [
    'expression',
    'typeAnnotation',
  ],
  AssignmentExpression: [
    'left',
    'right',
  ],
  AssignmentPattern: [
    'left',
    'right',
  ],
  AwaitExpression: [
    'argument',
  ],
  BigIntLiteralTypeAnnotation: [],
  BigIntTypeAnnotation: [],
  BinaryExpression: [
    'left',
    'right',
  ],
  BlockStatement: [
    'body',
  ],
  BooleanLiteralTypeAnnotation: [],
  BooleanTypeAnnotation: [],
  BreakStatement: [
    'label',
  ],
  CallExpression: [
    'callee',
    'typeArguments',
    'arguments',
  ],
  CatchClause: [
    'param',
    'body',
  ],
  ChainExpression: [
    'expression',
  ],
  ClassBody: [
    'body',
  ],
  ClassDeclaration: [
    'id',
    'typeParameters',
    'superClass',
    'superTypeArguments',
    'implements',
    'decorators',
    'body',
  ],
  ClassExpression: [
    'id',
    'typeParameters',
    'superClass',
    'superTypeArguments',
    'implements',
    'decorators',
    'body',
  ],
  ClassImplements: [
    'id',
    'typeArguments',
  ],
  ComponentDeclaration: [
    'id',
    'params',
    'body',
    'typeParameters',
    'rendersType',
  ],
  ComponentParameter: [
    'name',
    'local',
  ],
  ComponentTypeAnnotation: [
    'params',
    'rest',
    'typeParameters',
    'rendersType',
  ],
  ComponentTypeParameter: [
    'name',
    'typeAnnotation',
  ],
  ConditionalExpression: [
    'test',
    'consequent',
    'alternate',
  ],
  ConditionalTypeAnnotation: [
    'checkType',
    'extendsType',
    'trueType',
    'falseType',
  ],
  ConstructorTypeAnnotation: [
    'params',
    'returnType',
    'rest',
    'typeParameters',
  ],
  ContinueStatement: [
    'label',
  ],
  DebuggerStatement: [],
  DeclareClass: [
    'id',
    'typeParameters',
    'extends',
    'implements',
    'mixins',
    'body',
  ],
  DeclareClassExtendsCall: [
    'callee',
    'argument',
  ],
  DeclareComponent: [
    'id',
    'params',
    'rest',
    'typeParameters',
    'rendersType',
  ],
  DeclareEnum: [
    'id',
    'body',
  ],
  DeclareExportAllDeclaration: [
    'source',
  ],
  DeclareExportDeclaration: [
    'declaration',
    'specifiers',
    'source',
  ],
  DeclareFunction: [
    'id',
    'typeAnnotation',
    'predicate',
  ],
  DeclareHook: [
    'id',
  ],
  DeclareInterface: [
    'id',
    'typeParameters',
    'extends',
    'body',
  ],
  DeclareMethodDefinition: [
    'key',
    'value',
  ],
  DeclareModule: [
    'id',
    'body',
  ],
  DeclareModuleExports: [
    'typeAnnotation',
  ],
  DeclareNamespace: [
    'id',
    'body',
  ],
  DeclareOpaqueType: [
    'id',
    'typeParameters',
    'impltype',
    'lowerBound',
    'upperBound',
    'supertype',
  ],
  DeclareTypeAlias: [
    'id',
    'typeParameters',
    'right',
  ],
  DeclareVariable: [
    'declarations',
  ],
  DeclaredPredicate: [
    'value',
  ],
  Decorator: [
    'expression',
  ],
  DoWhileStatement: [
    'body',
    'test',
  ],
  EmptyStatement: [],
  EmptyTypeAnnotation: [],
  EnumBigIntMember: [
    'id',
    'init',
  ],
  EnumBody: [
    'members',
  ],
  EnumBooleanMember: [
    'id',
    'init',
  ],
  EnumDeclaration: [
    'id',
    'body',
  ],
  EnumDefaultedMember: [
    'id',
  ],
  EnumNumberMember: [
    'id',
    'init',
  ],
  EnumStringMember: [
    'id',
    'init',
  ],
  ExistsTypeAnnotation: [],
  ExportAllDeclaration: [
    'exported',
    'source',
    'attributes',
  ],
  ExportAssignment: [
    'expression',
  ],
  ExportDefaultDeclaration: [
    'declaration',
  ],
  ExportNamedDeclaration: [
    'declaration',
    'specifiers',
    'source',
    'attributes',
  ],
  ExportNamespaceSpecifier: [
    'exported',
  ],
  ExportSpecifier: [
    'exported',
    'local',
  ],
  ExpressionStatement: [
    'expression',
  ],
  ExternalModuleReference: [
    'expression',
  ],
  ForInStatement: [
    'left',
    'right',
    'body',
  ],
  ForOfStatement: [
    'left',
    'right',
    'body',
  ],
  ForStatement: [
    'init',
    'test',
    'update',
    'body',
  ],
  FunctionDeclaration: [
    'id',
    'params',
    'body',
    'typeParameters',
    'returnType',
    'predicate',
  ],
  FunctionExpression: [
    'id',
    'params',
    'body',
    'typeParameters',
    'returnType',
    'predicate',
  ],
  FunctionTypeAnnotation: [
    'params',
    'this',
    'returnType',
    'rest',
    'typeParameters',
  ],
  FunctionTypeParam: [
    'name',
    'typeAnnotation',
  ],
  GenericTypeAnnotation: [
    'id',
    'typeArguments',
  ],
  HookDeclaration: [
    'id',
    'params',
    'body',
    'typeParameters',
    'returnType',
  ],
  HookTypeAnnotation: [
    'params',
    'returnType',
    'rest',
    'typeParameters',
  ],
  Identifier: [
    'typeAnnotation',
  ],
  IfStatement: [
    'test',
    'consequent',
    'alternate',
  ],
  ImportAttribute: [
    'key',
    'value',
  ],
  ImportDeclaration: [
    'specifiers',
    'source',
    'attributes',
  ],
  ImportDefaultSpecifier: [
    'local',
  ],
  ImportEqualsDeclaration: [
    'id',
    'moduleReference',
  ],
  ImportExpression: [
    'source',
    'options',
  ],
  ImportNamespaceSpecifier: [
    'local',
  ],
  ImportSpecifier: [
    'imported',
    'local',
  ],
  ImportType: [
    'source',
  ],
  IndexedAccessType: [
    'objectType',
    'indexType',
  ],
  InferTypeAnnotation: [
    'typeParameter',
  ],
  InferredPredicate: [],
  InterfaceDeclaration: [
    'id',
    'typeParameters',
    'extends',
    'body',
  ],
  InterfaceExtends: [
    'id',
    'typeArguments',
  ],
  InterfaceTypeAnnotation: [
    'extends',
    'body',
  ],
  InterpreterDirective: [],
  IntersectionTypeAnnotation: [
    'types',
  ],
  JSXAttribute: [
    'name',
    'value',
  ],
  JSXClosingElement: [
    'name',
  ],
  JSXClosingFragment: [],
  JSXElement: [
    'openingElement',
    'children',
    'closingElement',
  ],
  JSXEmptyExpression: [],
  JSXExpressionContainer: [
    'expression',
  ],
  JSXFragment: [
    'openingFragment',
    'children',
    'closingFragment',
  ],
  JSXIdentifier: [],
  JSXMemberExpression: [
    'object',
    'property',
  ],
  JSXNamespacedName: [
    'namespace',
    'name',
  ],
  JSXOpeningElement: [
    'name',
    'attributes',
    'typeArguments',
  ],
  JSXOpeningFragment: [],
  JSXSpreadAttribute: [
    'argument',
  ],
  JSXSpreadChild: [
    'expression',
  ],
  JSXText: [],
  KeyofTypeAnnotation: [
    'argument',
  ],
  LabeledStatement: [
    'label',
    'body',
  ],
  LogicalExpression: [
    'left',
    'right',
  ],
  MatchArrayPattern: [
    'elements',
    'rest',
  ],
  MatchAsPattern: [
    'pattern',
    'target',
  ],
  MatchBindingPattern: [
    'id',
  ],
  MatchExpression: [
    'argument',
    'cases',
  ],
  MatchExpressionCase: [
    'pattern',
    'body',
    'guard',
  ],
  MatchIdentifierPattern: [
    'id',
  ],
  MatchInstanceObjectPattern: [
    'properties',
    'rest',
  ],
  MatchInstancePattern: [
    'targetConstructor',
    'properties',
  ],
  MatchLiteralPattern: [
    'literal',
  ],
  MatchMemberPattern: [
    'base',
    'property',
  ],
  MatchObjectPattern: [
    'properties',
    'rest',
  ],
  MatchObjectPatternProperty: [
    'key',
    'pattern',
  ],
  MatchOrPattern: [
    'patterns',
  ],
  MatchRestPattern: [
    'argument',
  ],
  MatchStatement: [
    'argument',
    'cases',
  ],
  MatchStatementCase: [
    'pattern',
    'body',
    'guard',
  ],
  MatchUnaryPattern: [
    'argument',
  ],
  MatchWildcardPattern: [],
  MemberExpression: [
    'object',
    'property',
  ],
  MetaProperty: [
    'meta',
    'property',
  ],
  MethodDefinition: [
    'key',
    'value',
    'decorators',
  ],
  MixedTypeAnnotation: [],
  NamespaceExportDeclaration: [
    'id',
  ],
  NeverTypeAnnotation: [],
  NewExpression: [
    'callee',
    'typeArguments',
    'arguments',
  ],
  NonNullExpression: [
    'expression',
  ],
  NullLiteralTypeAnnotation: [],
  NullableTypeAnnotation: [
    'typeAnnotation',
  ],
  NumberLiteralTypeAnnotation: [],
  NumberTypeAnnotation: [],
  ObjectExpression: [
    'properties',
  ],
  ObjectPattern: [
    'properties',
    'typeAnnotation',
  ],
  ObjectTypeAnnotation: [
    'properties',
    'indexers',
    'callProperties',
    'internalSlots',
  ],
  ObjectTypeCallProperty: [
    'value',
  ],
  ObjectTypeIndexer: [
    'id',
    'key',
    'value',
    'variance',
  ],
  ObjectTypeInternalSlot: [
    'id',
    'value',
  ],
  ObjectTypeMappedTypeProperty: [
    'keyTparam',
    'propType',
    'sourceType',
    'nameType',
    'variance',
  ],
  ObjectTypePrivateField: [
    'key',
  ],
  ObjectTypeProperty: [
    'key',
    'value',
    'variance',
    'init',
  ],
  ObjectTypeSpreadProperty: [
    'argument',
  ],
  OpaqueType: [
    'id',
    'typeParameters',
    'impltype',
    'lowerBound',
    'upperBound',
    'supertype',
  ],
  OptionalIndexedAccessType: [
    'objectType',
    'indexType',
  ],
  ParameterProperty: [
    'key',
    'value',
    'typeAnnotation',
    'variance',
    'decorators',
  ],
  PrivateIdentifier: [],
  Program: [
    'body',
  ],
  Property: [
    'key',
    'value',
  ],
  PropertyDefinition: [
    'key',
    'value',
    'decorators',
    'variance',
    'typeAnnotation',
  ],
  QualifiedTypeIdentifier: [
    'qualification',
    'id',
  ],
  QualifiedTypeofIdentifier: [
    'qualification',
    'id',
  ],
  RecordDeclaration: [
    'id',
    'typeParameters',
    'implements',
    'body',
  ],
  RecordDeclarationBody: [
    'elements',
  ],
  RecordDeclarationImplements: [
    'id',
    'typeArguments',
  ],
  RecordDeclarationProperty: [
    'key',
    'typeAnnotation',
    'defaultValue',
  ],
  RecordDeclarationStaticProperty: [
    'key',
    'typeAnnotation',
    'value',
  ],
  RecordExpression: [
    'recordConstructor',
    'typeArguments',
    'properties',
  ],
  RecordExpressionProperties: [
    'properties',
  ],
  RestElement: [
    'argument',
  ],
  ReturnStatement: [
    'argument',
  ],
  SatisfiesExpression: [
    'expression',
    'typeAnnotation',
  ],
  SequenceExpression: [
    'expressions',
  ],
  SpreadElement: [
    'argument',
  ],
  StaticBlock: [
    'body',
  ],
  StringLiteralTypeAnnotation: [],
  StringTypeAnnotation: [],
  Super: [],
  SwitchCase: [
    'test',
    'consequent',
  ],
  SwitchStatement: [
    'discriminant',
    'cases',
  ],
  SymbolTypeAnnotation: [],
  TaggedTemplateExpression: [
    'tag',
    'typeArguments',
    'quasi',
  ],
  TemplateElement: [],
  TemplateLiteral: [
    'quasis',
    'expressions',
  ],
  TemplateLiteralTypeAnnotation: [
    'quasis',
    'types',
  ],
  ThisExpression: [],
  ThisTypeAnnotation: [],
  ThrowStatement: [
    'argument',
  ],
  TryStatement: [
    'block',
    'handler',
    'finalizer',
  ],
  TupleTypeAnnotation: [
    'elementTypes',
  ],
  TupleTypeElement: [
    'elementType',
  ],
  TupleTypeLabeledElement: [
    'label',
    'elementType',
    'variance',
  ],
  TupleTypeSpreadElement: [
    'label',
    'typeAnnotation',
  ],
  TypeAlias: [
    'id',
    'typeParameters',
    'right',
  ],
  TypeAnnotation: [
    'typeAnnotation',
  ],
  TypeCastExpression: [
    'expression',
    'typeAnnotation',
  ],
  TypeOperator: [
    'typeAnnotation',
  ],
  TypeParameter: [
    'bound',
    'variance',
    'default',
  ],
  TypeParameterDeclaration: [
    'params',
  ],
  TypeParameterInstantiation: [
    'params',
  ],
  TypePredicate: [
    'parameterName',
    'typeAnnotation',
  ],
  TypeofTypeAnnotation: [
    'argument',
    'typeArguments',
  ],
  UnaryExpression: [
    'argument',
  ],
  UndefinedTypeAnnotation: [],
  UnionTypeAnnotation: [
    'types',
  ],
  UnknownTypeAnnotation: [],
  UpdateExpression: [
    'argument',
  ],
  VariableDeclaration: [
    'declarations',
  ],
  VariableDeclarator: [
    'id',
    'init',
  ],
  Variance: [],
  VoidTypeAnnotation: [],
  WhileStatement: [
    'test',
    'body',
  ],
  WithStatement: [
    'object',
    'body',
  ],
  YieldExpression: [
    'argument',
  ],
  OptionalMemberExpression: [
    'object',
    'property',
  ],
  OptionalCallExpression: [
    'callee',
    'typeArguments',
    'arguments',
  ],
  Literal: [],
};
