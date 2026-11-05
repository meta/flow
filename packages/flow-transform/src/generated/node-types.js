/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 * @generated
 */

/*
 * !!! GENERATED FILE !!!
 *
 * Any manual changes to this file will be overwritten. To regenerate run `yarn build`.
 */

// lint directives to let us do some basic validation of generated files
/* eslint no-undef: 'error', no-unused-vars: ['error', {vars: "local"}], no-redeclare: 'error' */
/* global NonNullable, Partial, Readonly, ReadonlyArray, $FlowFixMe */

'use strict';

import type {
  ESNode,
  AbstractMethodDefinition as AbstractMethodDefinitionType,
  AbstractPropertyDefinition as AbstractPropertyDefinitionType,
  AnyTypeAnnotation as AnyTypeAnnotationType,
  ArrayExpression as ArrayExpressionType,
  ArrayPattern as ArrayPatternType,
  ArrayTypeAnnotation as ArrayTypeAnnotationType,
  AsConstExpression as AsConstExpressionType,
  AsExpression as AsExpressionType,
  AssignmentExpression as AssignmentExpressionType,
  AssignmentPattern as AssignmentPatternType,
  AwaitExpression as AwaitExpressionType,
  BigIntLiteralTypeAnnotation as BigIntLiteralTypeAnnotationType,
  BigIntTypeAnnotation as BigIntTypeAnnotationType,
  BinaryExpression as BinaryExpressionType,
  BooleanLiteralTypeAnnotation as BooleanLiteralTypeAnnotationType,
  BooleanTypeAnnotation as BooleanTypeAnnotationType,
  BreakStatement as BreakStatementType,
  CallExpression as CallExpressionType,
  CatchClause as CatchClauseType,
  ChainExpression as ChainExpressionType,
  ClassBody as ClassBodyType,
  ClassExpression as ClassExpressionType,
  ClassImplements as ClassImplementsType,
  ComponentDeclaration as ComponentDeclarationType,
  ComponentParameter as ComponentParameterType,
  ComponentTypeAnnotation as ComponentTypeAnnotationType,
  ComponentTypeParameter as ComponentTypeParameterType,
  ConditionalExpression as ConditionalExpressionType,
  ConditionalTypeAnnotation as ConditionalTypeAnnotationType,
  ConstructorTypeAnnotation as ConstructorTypeAnnotationType,
  ContinueStatement as ContinueStatementType,
  DebuggerStatement as DebuggerStatementType,
  DeclareClass as DeclareClassType,
  DeclareClassExtendsCall as DeclareClassExtendsCallType,
  DeclareComponent as DeclareComponentType,
  DeclaredPredicate as DeclaredPredicateType,
  DeclareEnum as DeclareEnumType,
  DeclareExportAllDeclaration as DeclareExportAllDeclarationType,
  DeclareInterface as DeclareInterfaceType,
  DeclareModule as DeclareModuleType,
  DeclareModuleExports as DeclareModuleExportsType,
  DeclareNamespace as DeclareNamespaceType,
  DeclareOpaqueType as DeclareOpaqueTypeType,
  DeclareTypeAlias as DeclareTypeAliasType,
  DeclareVariable as DeclareVariableType,
  Decorator as DecoratorType,
  DoWhileStatement as DoWhileStatementType,
  EmptyStatement as EmptyStatementType,
  EmptyTypeAnnotation as EmptyTypeAnnotationType,
  EnumBigIntMember as EnumBigIntMemberType,
  EnumBody as EnumBodyType,
  EnumBooleanMember as EnumBooleanMemberType,
  EnumDeclaration as EnumDeclarationType,
  EnumDefaultedMember as EnumDefaultedMemberType,
  EnumNumberMember as EnumNumberMemberType,
  EnumStringMember as EnumStringMemberType,
  ExistsTypeAnnotation as ExistsTypeAnnotationType,
  ExportAssignment as ExportAssignmentType,
  ExportAllDeclaration as ExportAllDeclarationType,
  ExportDefaultDeclaration as ExportDefaultDeclarationType,
  ExportSpecifier as ExportSpecifierType,
  ExpressionStatement as ExpressionStatementType,
  ExternalModuleReference as ExternalModuleReferenceType,
  ForInStatement as ForInStatementType,
  ForOfStatement as ForOfStatementType,
  ForStatement as ForStatementType,
  FunctionDeclaration as FunctionDeclarationType,
  FunctionExpression as FunctionExpressionType,
  FunctionTypeAnnotation as FunctionTypeAnnotationType,
  FunctionTypeParam as FunctionTypeParamType,
  GenericTypeAnnotation as GenericTypeAnnotationType,
  HookDeclaration as HookDeclarationType,
  HookTypeAnnotation as HookTypeAnnotationType,
  IfStatement as IfStatementType,
  ImportAttribute as ImportAttributeType,
  ImportDeclaration as ImportDeclarationType,
  ImportDefaultSpecifier as ImportDefaultSpecifierType,
  ImportEqualsDeclaration as ImportEqualsDeclarationType,
  ImportExpression as ImportExpressionType,
  ImportNamespaceSpecifier as ImportNamespaceSpecifierType,
  ImportSpecifier as ImportSpecifierType,
  ImportType as ImportTypeType,
  IndexedAccessType as IndexedAccessTypeType,
  InferredPredicate as InferredPredicateType,
  InferTypeAnnotation as InferTypeAnnotationType,
  InterfaceDeclaration as InterfaceDeclarationType,
  InterfaceExtends as InterfaceExtendsType,
  InterfaceTypeAnnotation as InterfaceTypeAnnotationType,
  IntersectionTypeAnnotation as IntersectionTypeAnnotationType,
  JSXAttribute as JSXAttributeType,
  JSXClosingElement as JSXClosingElementType,
  JSXClosingFragment as JSXClosingFragmentType,
  JSXElement as JSXElementType,
  JSXEmptyExpression as JSXEmptyExpressionType,
  JSXExpressionContainer as JSXExpressionContainerType,
  JSXFragment as JSXFragmentType,
  JSXIdentifier as JSXIdentifierType,
  JSXMemberExpression as JSXMemberExpressionType,
  JSXNamespacedName as JSXNamespacedNameType,
  JSXOpeningElement as JSXOpeningElementType,
  JSXOpeningFragment as JSXOpeningFragmentType,
  JSXSpreadAttribute as JSXSpreadAttributeType,
  JSXSpreadChild as JSXSpreadChildType,
  JSXText as JSXTextType,
  KeyofTypeAnnotation as KeyofTypeAnnotationType,
  LabeledStatement as LabeledStatementType,
  LogicalExpression as LogicalExpressionType,
  MatchArrayPattern as MatchArrayPatternType,
  MatchAsPattern as MatchAsPatternType,
  MatchBindingPattern as MatchBindingPatternType,
  MatchExpression as MatchExpressionType,
  MatchExpressionCase as MatchExpressionCaseType,
  MatchIdentifierPattern as MatchIdentifierPatternType,
  MatchInstanceObjectPattern as MatchInstanceObjectPatternType,
  MatchInstancePattern as MatchInstancePatternType,
  MatchLiteralPattern as MatchLiteralPatternType,
  MatchMemberPattern as MatchMemberPatternType,
  MatchObjectPattern as MatchObjectPatternType,
  MatchObjectPatternProperty as MatchObjectPatternPropertyType,
  MatchOrPattern as MatchOrPatternType,
  MatchRestPattern as MatchRestPatternType,
  MatchStatement as MatchStatementType,
  MatchStatementCase as MatchStatementCaseType,
  MatchUnaryPattern as MatchUnaryPatternType,
  MatchWildcardPattern as MatchWildcardPatternType,
  MetaProperty as MetaPropertyType,
  MethodDefinition as MethodDefinitionType,
  MixedTypeAnnotation as MixedTypeAnnotationType,
  NamespaceExportDeclaration as NamespaceExportDeclarationType,
  NeverTypeAnnotation as NeverTypeAnnotationType,
  NewExpression as NewExpressionType,
  NullableTypeAnnotation as NullableTypeAnnotationType,
  NullLiteralTypeAnnotation as NullLiteralTypeAnnotationType,
  NumberLiteralTypeAnnotation as NumberLiteralTypeAnnotationType,
  NumberTypeAnnotation as NumberTypeAnnotationType,
  ObjectExpression as ObjectExpressionType,
  ObjectPattern as ObjectPatternType,
  ObjectTypeAnnotation as ObjectTypeAnnotationType,
  ObjectTypeCallProperty as ObjectTypeCallPropertyType,
  ObjectTypeIndexer as ObjectTypeIndexerType,
  ObjectTypeInternalSlot as ObjectTypeInternalSlotType,
  ObjectTypeMappedTypeProperty as ObjectTypeMappedTypePropertyType,
  ObjectTypePrivateField as ObjectTypePrivateFieldType,
  ObjectTypeSpreadProperty as ObjectTypeSpreadPropertyType,
  OpaqueType as OpaqueTypeType,
  OptionalIndexedAccessType as OptionalIndexedAccessTypeType,
  PrivateIdentifier as PrivateIdentifierType,
  Property as PropertyType,
  PropertyDefinition as PropertyDefinitionType,
  QualifiedTypeIdentifier as QualifiedTypeIdentifierType,
  QualifiedTypeofIdentifier as QualifiedTypeofIdentifierType,
  RecordDeclaration as RecordDeclarationType,
  RecordDeclarationBody as RecordDeclarationBodyType,
  RecordDeclarationImplements as RecordDeclarationImplementsType,
  RecordDeclarationProperty as RecordDeclarationPropertyType,
  RecordDeclarationStaticProperty as RecordDeclarationStaticPropertyType,
  RecordExpression as RecordExpressionType,
  RecordExpressionProperties as RecordExpressionPropertiesType,
  RestElement as RestElementType,
  ReturnStatement as ReturnStatementType,
  SatisfiesExpression as SatisfiesExpressionType,
  SequenceExpression as SequenceExpressionType,
  SpreadElement as SpreadElementType,
  StaticBlock as StaticBlockType,
  StringLiteralTypeAnnotation as StringLiteralTypeAnnotationType,
  StringTypeAnnotation as StringTypeAnnotationType,
  Super as SuperType,
  SwitchCase as SwitchCaseType,
  SwitchStatement as SwitchStatementType,
  SymbolTypeAnnotation as SymbolTypeAnnotationType,
  TaggedTemplateExpression as TaggedTemplateExpressionType,
  TemplateLiteral as TemplateLiteralType,
  TemplateLiteralTypeAnnotation as TemplateLiteralTypeAnnotationType,
  ThisExpression as ThisExpressionType,
  ThisTypeAnnotation as ThisTypeAnnotationType,
  ThrowStatement as ThrowStatementType,
  TryStatement as TryStatementType,
  TupleTypeAnnotation as TupleTypeAnnotationType,
  TupleTypeElement as TupleTypeElementType,
  TupleTypeLabeledElement as TupleTypeLabeledElementType,
  TupleTypeSpreadElement as TupleTypeSpreadElementType,
  TypeAlias as TypeAliasType,
  TypeAnnotation as TypeAnnotationType,
  TypeCastExpression as TypeCastExpressionType,
  TypeofTypeAnnotation as TypeofTypeAnnotationType,
  TypeOperator as TypeOperatorType,
  TypeParameter as TypeParameterType,
  TypeParameterDeclaration as TypeParameterDeclarationType,
  TypeParameterInstantiation as TypeParameterInstantiationType,
  TypePredicate as TypePredicateType,
  UnaryExpression as UnaryExpressionType,
  UndefinedTypeAnnotation as UndefinedTypeAnnotationType,
  UnionTypeAnnotation as UnionTypeAnnotationType,
  UnknownTypeAnnotation as UnknownTypeAnnotationType,
  UpdateExpression as UpdateExpressionType,
  VariableDeclaration as VariableDeclarationType,
  VariableDeclarator as VariableDeclaratorType,
  Variance as VarianceType,
  VoidTypeAnnotation as VoidTypeAnnotationType,
  WhileStatement as WhileStatementType,
  WithStatement as WithStatementType,
  YieldExpression as YieldExpressionType,
} from 'flow-estree';
import type {DetachedNode, MaybeDetachedNode} from '../detachedNode';

import {
  asDetachedNodeForCodeGen,
  detachedProps,
  setParentPointersInDirectChildren,
} from '../detachedNode';

export type AbstractMethodDefinitionProps = {
  readonly key: MaybeDetachedNode<AbstractMethodDefinitionType['key']>,
  readonly value: MaybeDetachedNode<AbstractMethodDefinitionType['value']>,
  readonly computed: AbstractMethodDefinitionType['computed'],
  readonly override?: ?AbstractMethodDefinitionType['override'],
  readonly tsAccessibility?: ?AbstractMethodDefinitionType['tsAccessibility'],
};

export type AbstractPropertyDefinitionProps = {
  readonly key: MaybeDetachedNode<AbstractPropertyDefinitionType['key']>,
  readonly value?: ?MaybeDetachedNode<AbstractPropertyDefinitionType['value']>,
  readonly typeAnnotation?: ?MaybeDetachedNode<
    AbstractPropertyDefinitionType['typeAnnotation'],
  >,
  readonly computed: AbstractPropertyDefinitionType['computed'],
  readonly variance?: ?MaybeDetachedNode<
    AbstractPropertyDefinitionType['variance'],
  >,
  readonly override?: ?AbstractPropertyDefinitionType['override'],
  readonly tsAccessibility?: ?AbstractPropertyDefinitionType['tsAccessibility'],
};

export type AnyTypeAnnotationProps = {};

export type ArrayExpressionProps = {
  readonly elements: ReadonlyArray<
    MaybeDetachedNode<ArrayExpressionType['elements'][number]>,
  >,
  readonly trailingComma: ArrayExpressionType['trailingComma'],
};

export type ArrayPatternProps = {
  readonly elements: ReadonlyArray<
    MaybeDetachedNode<ArrayPatternType['elements'][number]>,
  >,
  readonly typeAnnotation?: ?MaybeDetachedNode<
    ArrayPatternType['typeAnnotation'],
  >,
  readonly optional?: ?ArrayPatternType['optional'],
};

export type ArrayTypeAnnotationProps = {
  readonly elementType: MaybeDetachedNode<
    ArrayTypeAnnotationType['elementType'],
  >,
};

export type AsConstExpressionProps = {
  readonly expression: MaybeDetachedNode<AsConstExpressionType['expression']>,
};

export type AsExpressionProps = {
  readonly expression: MaybeDetachedNode<AsExpressionType['expression']>,
  readonly typeAnnotation: MaybeDetachedNode<
    AsExpressionType['typeAnnotation'],
  >,
};

export type AssignmentExpressionProps = {
  readonly operator: AssignmentExpressionType['operator'],
  readonly left: MaybeDetachedNode<AssignmentExpressionType['left']>,
  readonly right: MaybeDetachedNode<AssignmentExpressionType['right']>,
};

export type AssignmentPatternProps = {
  readonly left: MaybeDetachedNode<AssignmentPatternType['left']>,
  readonly right: MaybeDetachedNode<AssignmentPatternType['right']>,
};

export type AwaitExpressionProps = {
  readonly argument: MaybeDetachedNode<AwaitExpressionType['argument']>,
};

export type BigIntLiteralTypeAnnotationProps = {
  readonly raw: BigIntLiteralTypeAnnotationType['raw'],
};

export type BigIntTypeAnnotationProps = {};

export type BinaryExpressionProps = {
  readonly left: MaybeDetachedNode<BinaryExpressionType['left']>,
  readonly right: MaybeDetachedNode<BinaryExpressionType['right']>,
  readonly operator: BinaryExpressionType['operator'],
};

export type BooleanLiteralTypeAnnotationProps = {
  readonly value: BooleanLiteralTypeAnnotationType['value'],
  readonly raw: BooleanLiteralTypeAnnotationType['raw'],
};

export type BooleanTypeAnnotationProps = {};

export type BreakStatementProps = {
  readonly label?: ?MaybeDetachedNode<BreakStatementType['label']>,
};

export type CallExpressionProps = {
  readonly callee: MaybeDetachedNode<CallExpressionType['callee']>,
  readonly typeArguments?: ?MaybeDetachedNode<
    CallExpressionType['typeArguments'],
  >,
  readonly arguments: ReadonlyArray<
    MaybeDetachedNode<CallExpressionType['arguments'][number]>,
  >,
};

export type CatchClauseProps = {
  readonly param?: ?MaybeDetachedNode<CatchClauseType['param']>,
  readonly body: MaybeDetachedNode<CatchClauseType['body']>,
};

export type ChainExpressionProps = {
  readonly expression: MaybeDetachedNode<ChainExpressionType['expression']>,
};

export type ClassBodyProps = {
  readonly body: ReadonlyArray<
    MaybeDetachedNode<ClassBodyType['body'][number]>,
  >,
};

export type ClassExpressionProps = {
  readonly id?: ?MaybeDetachedNode<ClassExpressionType['id']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    ClassExpressionType['typeParameters'],
  >,
  readonly superClass?: ?MaybeDetachedNode<ClassExpressionType['superClass']>,
  readonly superTypeArguments?: ?MaybeDetachedNode<
    ClassExpressionType['superTypeArguments'],
  >,
  readonly implements: ReadonlyArray<
    MaybeDetachedNode<ClassExpressionType['implements'][number]>,
  >,
  readonly decorators: ReadonlyArray<
    MaybeDetachedNode<ClassExpressionType['decorators'][number]>,
  >,
  readonly body: MaybeDetachedNode<ClassExpressionType['body']>,
  readonly abstract?: ?ClassExpressionType['abstract'],
};

export type ClassImplementsProps = {
  readonly id: MaybeDetachedNode<ClassImplementsType['id']>,
  readonly typeArguments?: ?MaybeDetachedNode<
    ClassImplementsType['typeArguments'],
  >,
};

export type ComponentDeclarationProps = {
  readonly id: MaybeDetachedNode<ComponentDeclarationType['id']>,
  readonly params: ReadonlyArray<
    MaybeDetachedNode<ComponentDeclarationType['params'][number]>,
  >,
  readonly body: MaybeDetachedNode<ComponentDeclarationType['body']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    ComponentDeclarationType['typeParameters'],
  >,
  readonly rendersType?: ?MaybeDetachedNode<
    ComponentDeclarationType['rendersType'],
  >,
  readonly async: ComponentDeclarationType['async'],
};

export type ComponentParameterProps = {
  readonly name: MaybeDetachedNode<ComponentParameterType['name']>,
  readonly local: MaybeDetachedNode<ComponentParameterType['local']>,
  readonly shorthand: ComponentParameterType['shorthand'],
};

export type ComponentTypeAnnotationProps = {
  readonly params: ReadonlyArray<
    MaybeDetachedNode<ComponentTypeAnnotationType['params'][number]>,
  >,
  readonly rest?: ?MaybeDetachedNode<ComponentTypeAnnotationType['rest']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    ComponentTypeAnnotationType['typeParameters'],
  >,
  readonly rendersType?: ?MaybeDetachedNode<
    ComponentTypeAnnotationType['rendersType'],
  >,
};

export type ComponentTypeParameterProps = {
  readonly name?: ?MaybeDetachedNode<ComponentTypeParameterType['name']>,
  readonly typeAnnotation: MaybeDetachedNode<
    ComponentTypeParameterType['typeAnnotation'],
  >,
  readonly optional: ComponentTypeParameterType['optional'],
};

export type ConditionalExpressionProps = {
  readonly test: MaybeDetachedNode<ConditionalExpressionType['test']>,
  readonly alternate: MaybeDetachedNode<ConditionalExpressionType['alternate']>,
  readonly consequent: MaybeDetachedNode<
    ConditionalExpressionType['consequent'],
  >,
};

export type ConditionalTypeAnnotationProps = {
  readonly checkType: MaybeDetachedNode<
    ConditionalTypeAnnotationType['checkType'],
  >,
  readonly extendsType: MaybeDetachedNode<
    ConditionalTypeAnnotationType['extendsType'],
  >,
  readonly trueType: MaybeDetachedNode<
    ConditionalTypeAnnotationType['trueType'],
  >,
  readonly falseType: MaybeDetachedNode<
    ConditionalTypeAnnotationType['falseType'],
  >,
};

export type ConstructorTypeAnnotationProps = {
  readonly abstract: ConstructorTypeAnnotationType['abstract'],
  readonly params: ReadonlyArray<
    MaybeDetachedNode<ConstructorTypeAnnotationType['params'][number]>,
  >,
  readonly returnType: MaybeDetachedNode<
    ConstructorTypeAnnotationType['returnType'],
  >,
  readonly rest?: ?MaybeDetachedNode<ConstructorTypeAnnotationType['rest']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    ConstructorTypeAnnotationType['typeParameters'],
  >,
};

export type ContinueStatementProps = {
  readonly label?: ?MaybeDetachedNode<ContinueStatementType['label']>,
};

export type DebuggerStatementProps = {};

export type DeclareClassProps = {
  readonly id: MaybeDetachedNode<DeclareClassType['id']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    DeclareClassType['typeParameters'],
  >,
  readonly extends: ReadonlyArray<
    MaybeDetachedNode<DeclareClassType['extends'][number]>,
  >,
  readonly implements: ReadonlyArray<
    MaybeDetachedNode<DeclareClassType['implements'][number]>,
  >,
  readonly mixins: ReadonlyArray<
    MaybeDetachedNode<DeclareClassType['mixins'][number]>,
  >,
  readonly body: MaybeDetachedNode<DeclareClassType['body']>,
  readonly abstract?: ?DeclareClassType['abstract'],
};

export type DeclareClassExtendsCallProps = {
  readonly callee: MaybeDetachedNode<DeclareClassExtendsCallType['callee']>,
  readonly argument: MaybeDetachedNode<DeclareClassExtendsCallType['argument']>,
};

export type DeclareComponentProps = {
  readonly id: MaybeDetachedNode<DeclareComponentType['id']>,
  readonly params: ReadonlyArray<
    MaybeDetachedNode<DeclareComponentType['params'][number]>,
  >,
  readonly rest?: ?MaybeDetachedNode<DeclareComponentType['rest']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    DeclareComponentType['typeParameters'],
  >,
  readonly rendersType?: ?MaybeDetachedNode<
    DeclareComponentType['rendersType'],
  >,
};

export type DeclaredPredicateProps = {
  readonly value: MaybeDetachedNode<DeclaredPredicateType['value']>,
};

export type DeclareEnumProps = {
  readonly id: MaybeDetachedNode<DeclareEnumType['id']>,
  readonly body: MaybeDetachedNode<DeclareEnumType['body']>,
  readonly const?: ?DeclareEnumType['const'],
};

export type DeclareExportAllDeclarationProps = {
  readonly source: MaybeDetachedNode<DeclareExportAllDeclarationType['source']>,
};

export type DeclareInterfaceProps = {
  readonly id: MaybeDetachedNode<DeclareInterfaceType['id']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    DeclareInterfaceType['typeParameters'],
  >,
  readonly extends: ReadonlyArray<
    MaybeDetachedNode<DeclareInterfaceType['extends'][number]>,
  >,
  readonly body: MaybeDetachedNode<DeclareInterfaceType['body']>,
};

export type DeclareModuleProps = {
  readonly id: MaybeDetachedNode<DeclareModuleType['id']>,
  readonly body: MaybeDetachedNode<DeclareModuleType['body']>,
};

export type DeclareModuleExportsProps = {
  readonly typeAnnotation: MaybeDetachedNode<
    DeclareModuleExportsType['typeAnnotation'],
  >,
};

export type DeclareNamespaceProps = {
  readonly id: MaybeDetachedNode<DeclareNamespaceType['id']>,
  readonly body: MaybeDetachedNode<DeclareNamespaceType['body']>,
};

export type DeclareOpaqueTypeProps = {
  readonly id: MaybeDetachedNode<DeclareOpaqueTypeType['id']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    DeclareOpaqueTypeType['typeParameters'],
  >,
  readonly impltype?: ?MaybeDetachedNode<DeclareOpaqueTypeType['impltype']>,
  readonly lowerBound?: ?MaybeDetachedNode<DeclareOpaqueTypeType['lowerBound']>,
  readonly upperBound?: ?MaybeDetachedNode<DeclareOpaqueTypeType['upperBound']>,
  readonly supertype?: ?MaybeDetachedNode<DeclareOpaqueTypeType['supertype']>,
};

export type DeclareTypeAliasProps = {
  readonly id: MaybeDetachedNode<DeclareTypeAliasType['id']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    DeclareTypeAliasType['typeParameters'],
  >,
  readonly right: MaybeDetachedNode<DeclareTypeAliasType['right']>,
};

export type DeclareVariableProps = {
  readonly declarations: ReadonlyArray<
    MaybeDetachedNode<DeclareVariableType['declarations'][number]>,
  >,
  readonly kind: DeclareVariableType['kind'],
  readonly implicitDeclare?: ?DeclareVariableType['implicitDeclare'],
};

export type DecoratorProps = {
  readonly expression: MaybeDetachedNode<DecoratorType['expression']>,
};

export type DoWhileStatementProps = {
  readonly body: MaybeDetachedNode<DoWhileStatementType['body']>,
  readonly test: MaybeDetachedNode<DoWhileStatementType['test']>,
};

export type EmptyStatementProps = {};

export type EmptyTypeAnnotationProps = {};

export type EnumBigIntMemberProps = {
  readonly id: MaybeDetachedNode<EnumBigIntMemberType['id']>,
  readonly init: MaybeDetachedNode<EnumBigIntMemberType['init']>,
};

export type EnumBodyProps = {
  readonly members: ReadonlyArray<
    MaybeDetachedNode<EnumBodyType['members'][number]>,
  >,
  readonly explicitType: EnumBodyType['explicitType'],
  readonly hasUnknownMembers: EnumBodyType['hasUnknownMembers'],
};

export type EnumBooleanMemberProps = {
  readonly id: MaybeDetachedNode<EnumBooleanMemberType['id']>,
  readonly init: MaybeDetachedNode<EnumBooleanMemberType['init']>,
};

export type EnumDeclarationProps = {
  readonly id: MaybeDetachedNode<EnumDeclarationType['id']>,
  readonly body: MaybeDetachedNode<EnumDeclarationType['body']>,
  readonly const?: ?EnumDeclarationType['const'],
};

export type EnumDefaultedMemberProps = {
  readonly id: MaybeDetachedNode<EnumDefaultedMemberType['id']>,
};

export type EnumNumberMemberProps = {
  readonly id: MaybeDetachedNode<EnumNumberMemberType['id']>,
  readonly init: MaybeDetachedNode<EnumNumberMemberType['init']>,
};

export type EnumStringMemberProps = {
  readonly id: MaybeDetachedNode<EnumStringMemberType['id']>,
  readonly init: MaybeDetachedNode<EnumStringMemberType['init']>,
};

export type ExistsTypeAnnotationProps = {};

export type ExportAssignmentProps = {
  readonly expression: MaybeDetachedNode<ExportAssignmentType['expression']>,
};

export type ExportAllDeclarationProps = {
  readonly exported?: ?MaybeDetachedNode<ExportAllDeclarationType['exported']>,
  readonly source: MaybeDetachedNode<ExportAllDeclarationType['source']>,
  readonly exportKind: ExportAllDeclarationType['exportKind'],
};

export type ExportDefaultDeclarationProps = {
  readonly declaration: MaybeDetachedNode<
    ExportDefaultDeclarationType['declaration'],
  >,
};

export type ExportSpecifierProps = {
  readonly exported: MaybeDetachedNode<ExportSpecifierType['exported']>,
  readonly local: MaybeDetachedNode<ExportSpecifierType['local']>,
  readonly exportKind: ExportSpecifierType['exportKind'],
};

export type ExpressionStatementProps = {
  readonly expression: MaybeDetachedNode<ExpressionStatementType['expression']>,
  readonly directive?: ?ExpressionStatementType['directive'],
};

export type ExternalModuleReferenceProps = {
  readonly expression: MaybeDetachedNode<
    ExternalModuleReferenceType['expression'],
  >,
};

export type ForInStatementProps = {
  readonly left: MaybeDetachedNode<ForInStatementType['left']>,
  readonly right: MaybeDetachedNode<ForInStatementType['right']>,
  readonly body: MaybeDetachedNode<ForInStatementType['body']>,
};

export type ForOfStatementProps = {
  readonly left: MaybeDetachedNode<ForOfStatementType['left']>,
  readonly right: MaybeDetachedNode<ForOfStatementType['right']>,
  readonly body: MaybeDetachedNode<ForOfStatementType['body']>,
  readonly await: ForOfStatementType['await'],
};

export type ForStatementProps = {
  readonly init?: ?MaybeDetachedNode<ForStatementType['init']>,
  readonly test?: ?MaybeDetachedNode<ForStatementType['test']>,
  readonly update?: ?MaybeDetachedNode<ForStatementType['update']>,
  readonly body: MaybeDetachedNode<ForStatementType['body']>,
};

export type FunctionDeclarationProps = {
  readonly id?: ?MaybeDetachedNode<FunctionDeclarationType['id']>,
  readonly params: ReadonlyArray<
    MaybeDetachedNode<FunctionDeclarationType['params'][number]>,
  >,
  readonly body: MaybeDetachedNode<FunctionDeclarationType['body']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    FunctionDeclarationType['typeParameters'],
  >,
  readonly returnType?: ?MaybeDetachedNode<
    FunctionDeclarationType['returnType'],
  >,
  readonly predicate?: ?MaybeDetachedNode<FunctionDeclarationType['predicate']>,
  readonly generator: FunctionDeclarationType['generator'],
  readonly async: FunctionDeclarationType['async'],
};

export type FunctionExpressionProps = {
  readonly id?: ?MaybeDetachedNode<FunctionExpressionType['id']>,
  readonly params: ReadonlyArray<
    MaybeDetachedNode<FunctionExpressionType['params'][number]>,
  >,
  readonly body: MaybeDetachedNode<FunctionExpressionType['body']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    FunctionExpressionType['typeParameters'],
  >,
  readonly returnType?: ?MaybeDetachedNode<
    FunctionExpressionType['returnType'],
  >,
  readonly predicate?: ?MaybeDetachedNode<FunctionExpressionType['predicate']>,
  readonly generator: FunctionExpressionType['generator'],
  readonly async: FunctionExpressionType['async'],
};

export type FunctionTypeAnnotationProps = {
  readonly params: ReadonlyArray<
    MaybeDetachedNode<FunctionTypeAnnotationType['params'][number]>,
  >,
  readonly this?: ?MaybeDetachedNode<FunctionTypeAnnotationType['this']>,
  readonly returnType: MaybeDetachedNode<
    FunctionTypeAnnotationType['returnType'],
  >,
  readonly rest?: ?MaybeDetachedNode<FunctionTypeAnnotationType['rest']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    FunctionTypeAnnotationType['typeParameters'],
  >,
};

export type FunctionTypeParamProps = {
  readonly name?: ?MaybeDetachedNode<FunctionTypeParamType['name']>,
  readonly typeAnnotation: MaybeDetachedNode<
    FunctionTypeParamType['typeAnnotation'],
  >,
  readonly optional: FunctionTypeParamType['optional'],
};

export type GenericTypeAnnotationProps = {
  readonly id: MaybeDetachedNode<GenericTypeAnnotationType['id']>,
  readonly typeArguments?: ?MaybeDetachedNode<
    GenericTypeAnnotationType['typeArguments'],
  >,
};

export type HookDeclarationProps = {
  readonly id: MaybeDetachedNode<HookDeclarationType['id']>,
  readonly params: ReadonlyArray<
    MaybeDetachedNode<HookDeclarationType['params'][number]>,
  >,
  readonly body: MaybeDetachedNode<HookDeclarationType['body']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    HookDeclarationType['typeParameters'],
  >,
  readonly returnType?: ?MaybeDetachedNode<HookDeclarationType['returnType']>,
  readonly async: HookDeclarationType['async'],
};

export type HookTypeAnnotationProps = {
  readonly params: ReadonlyArray<
    MaybeDetachedNode<HookTypeAnnotationType['params'][number]>,
  >,
  readonly returnType: MaybeDetachedNode<HookTypeAnnotationType['returnType']>,
  readonly rest?: ?MaybeDetachedNode<HookTypeAnnotationType['rest']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    HookTypeAnnotationType['typeParameters'],
  >,
};

export type IfStatementProps = {
  readonly test: MaybeDetachedNode<IfStatementType['test']>,
  readonly consequent: MaybeDetachedNode<IfStatementType['consequent']>,
  readonly alternate?: ?MaybeDetachedNode<IfStatementType['alternate']>,
};

export type ImportAttributeProps = {
  readonly key: MaybeDetachedNode<ImportAttributeType['key']>,
  readonly value: MaybeDetachedNode<ImportAttributeType['value']>,
};

export type ImportDeclarationProps = {
  readonly specifiers: ReadonlyArray<
    MaybeDetachedNode<ImportDeclarationType['specifiers'][number]>,
  >,
  readonly source: MaybeDetachedNode<ImportDeclarationType['source']>,
  readonly attributes?: ?ReadonlyArray<
    MaybeDetachedNode<ImportDeclarationType['attributes'][number]>,
  >,
  readonly importKind: ImportDeclarationType['importKind'],
  readonly phase: ImportDeclarationType['phase'],
};

export type ImportDefaultSpecifierProps = {
  readonly local: MaybeDetachedNode<ImportDefaultSpecifierType['local']>,
};

export type ImportEqualsDeclarationProps = {
  readonly id: MaybeDetachedNode<ImportEqualsDeclarationType['id']>,
  readonly moduleReference: MaybeDetachedNode<
    ImportEqualsDeclarationType['moduleReference'],
  >,
  readonly importKind: ImportEqualsDeclarationType['importKind'],
  readonly isExport: ImportEqualsDeclarationType['isExport'],
};

export type ImportExpressionProps = {
  readonly source: MaybeDetachedNode<ImportExpressionType['source']>,
  readonly options?: ?MaybeDetachedNode<ImportExpressionType['options']>,
  readonly phase: ImportExpressionType['phase'],
};

export type ImportNamespaceSpecifierProps = {
  readonly local: MaybeDetachedNode<ImportNamespaceSpecifierType['local']>,
};

export type ImportSpecifierProps = {
  readonly imported: MaybeDetachedNode<ImportSpecifierType['imported']>,
  readonly local: MaybeDetachedNode<ImportSpecifierType['local']>,
  readonly importKind: ImportSpecifierType['importKind'],
};

export type ImportTypeProps = {
  readonly source: MaybeDetachedNode<ImportTypeType['source']>,
};

export type IndexedAccessTypeProps = {
  readonly objectType: MaybeDetachedNode<IndexedAccessTypeType['objectType']>,
  readonly indexType: MaybeDetachedNode<IndexedAccessTypeType['indexType']>,
};

export type InferredPredicateProps = {};

export type InferTypeAnnotationProps = {
  readonly typeParameter: MaybeDetachedNode<
    InferTypeAnnotationType['typeParameter'],
  >,
};

export type InterfaceDeclarationProps = {
  readonly id: MaybeDetachedNode<InterfaceDeclarationType['id']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    InterfaceDeclarationType['typeParameters'],
  >,
  readonly extends: ReadonlyArray<
    MaybeDetachedNode<InterfaceDeclarationType['extends'][number]>,
  >,
  readonly body: MaybeDetachedNode<InterfaceDeclarationType['body']>,
};

export type InterfaceExtendsProps = {
  readonly id: MaybeDetachedNode<InterfaceExtendsType['id']>,
  readonly typeArguments?: ?MaybeDetachedNode<
    InterfaceExtendsType['typeArguments'],
  >,
};

export type InterfaceTypeAnnotationProps = {
  readonly extends: ReadonlyArray<
    MaybeDetachedNode<InterfaceTypeAnnotationType['extends'][number]>,
  >,
  readonly body?: ?MaybeDetachedNode<InterfaceTypeAnnotationType['body']>,
};

export type IntersectionTypeAnnotationProps = {
  readonly types: ReadonlyArray<
    MaybeDetachedNode<IntersectionTypeAnnotationType['types'][number]>,
  >,
};

export type JSXAttributeProps = {
  readonly name: MaybeDetachedNode<JSXAttributeType['name']>,
  readonly value?: ?MaybeDetachedNode<JSXAttributeType['value']>,
};

export type JSXClosingElementProps = {
  readonly name: MaybeDetachedNode<JSXClosingElementType['name']>,
};

export type JSXClosingFragmentProps = {};

export type JSXElementProps = {
  readonly openingElement: MaybeDetachedNode<JSXElementType['openingElement']>,
  readonly children: ReadonlyArray<
    MaybeDetachedNode<JSXElementType['children'][number]>,
  >,
  readonly closingElement?: ?MaybeDetachedNode<
    JSXElementType['closingElement'],
  >,
};

export type JSXEmptyExpressionProps = {};

export type JSXExpressionContainerProps = {
  readonly expression: MaybeDetachedNode<
    JSXExpressionContainerType['expression'],
  >,
};

export type JSXFragmentProps = {
  readonly openingFragment: MaybeDetachedNode<
    JSXFragmentType['openingFragment'],
  >,
  readonly children: ReadonlyArray<
    MaybeDetachedNode<JSXFragmentType['children'][number]>,
  >,
  readonly closingFragment: MaybeDetachedNode<
    JSXFragmentType['closingFragment'],
  >,
};

export type JSXIdentifierProps = {
  readonly name: JSXIdentifierType['name'],
};

export type JSXMemberExpressionProps = {
  readonly object: MaybeDetachedNode<JSXMemberExpressionType['object']>,
  readonly property: MaybeDetachedNode<JSXMemberExpressionType['property']>,
};

export type JSXNamespacedNameProps = {
  readonly namespace: MaybeDetachedNode<JSXNamespacedNameType['namespace']>,
  readonly name: MaybeDetachedNode<JSXNamespacedNameType['name']>,
};

export type JSXOpeningElementProps = {
  readonly name: MaybeDetachedNode<JSXOpeningElementType['name']>,
  readonly attributes: ReadonlyArray<
    MaybeDetachedNode<JSXOpeningElementType['attributes'][number]>,
  >,
  readonly selfClosing: JSXOpeningElementType['selfClosing'],
  readonly typeArguments?: ?MaybeDetachedNode<
    JSXOpeningElementType['typeArguments'],
  >,
};

export type JSXOpeningFragmentProps = {};

export type JSXSpreadAttributeProps = {
  readonly argument: MaybeDetachedNode<JSXSpreadAttributeType['argument']>,
};

export type JSXSpreadChildProps = {
  readonly expression: MaybeDetachedNode<JSXSpreadChildType['expression']>,
};

export type JSXTextProps = {
  readonly value: JSXTextType['value'],
  readonly raw: JSXTextType['raw'],
};

export type KeyofTypeAnnotationProps = {
  readonly argument: MaybeDetachedNode<KeyofTypeAnnotationType['argument']>,
};

export type LabeledStatementProps = {
  readonly label: MaybeDetachedNode<LabeledStatementType['label']>,
  readonly body: MaybeDetachedNode<LabeledStatementType['body']>,
};

export type LogicalExpressionProps = {
  readonly left: MaybeDetachedNode<LogicalExpressionType['left']>,
  readonly right: MaybeDetachedNode<LogicalExpressionType['right']>,
  readonly operator: LogicalExpressionType['operator'],
};

export type MatchArrayPatternProps = {
  readonly elements: ReadonlyArray<
    MaybeDetachedNode<MatchArrayPatternType['elements'][number]>,
  >,
  readonly rest?: ?MaybeDetachedNode<MatchArrayPatternType['rest']>,
};

export type MatchAsPatternProps = {
  readonly pattern: MaybeDetachedNode<MatchAsPatternType['pattern']>,
  readonly target: MaybeDetachedNode<MatchAsPatternType['target']>,
};

export type MatchBindingPatternProps = {
  readonly id: MaybeDetachedNode<MatchBindingPatternType['id']>,
  readonly kind: MatchBindingPatternType['kind'],
};

export type MatchExpressionProps = {
  readonly argument: MaybeDetachedNode<MatchExpressionType['argument']>,
  readonly cases: ReadonlyArray<
    MaybeDetachedNode<MatchExpressionType['cases'][number]>,
  >,
};

export type MatchExpressionCaseProps = {
  readonly pattern: MaybeDetachedNode<MatchExpressionCaseType['pattern']>,
  readonly body: MaybeDetachedNode<MatchExpressionCaseType['body']>,
  readonly guard?: ?MaybeDetachedNode<MatchExpressionCaseType['guard']>,
};

export type MatchIdentifierPatternProps = {
  readonly id: MaybeDetachedNode<MatchIdentifierPatternType['id']>,
};

export type MatchInstanceObjectPatternProps = {
  readonly properties: ReadonlyArray<
    MaybeDetachedNode<MatchInstanceObjectPatternType['properties'][number]>,
  >,
  readonly rest?: ?MaybeDetachedNode<MatchInstanceObjectPatternType['rest']>,
};

export type MatchInstancePatternProps = {
  readonly targetConstructor: MaybeDetachedNode<
    MatchInstancePatternType['targetConstructor'],
  >,
  readonly properties: MaybeDetachedNode<
    MatchInstancePatternType['properties'],
  >,
};

export type MatchLiteralPatternProps = {
  readonly literal: MaybeDetachedNode<MatchLiteralPatternType['literal']>,
};

export type MatchMemberPatternProps = {
  readonly base: MaybeDetachedNode<MatchMemberPatternType['base']>,
  readonly property: MaybeDetachedNode<MatchMemberPatternType['property']>,
};

export type MatchObjectPatternProps = {
  readonly properties: ReadonlyArray<
    MaybeDetachedNode<MatchObjectPatternType['properties'][number]>,
  >,
  readonly rest?: ?MaybeDetachedNode<MatchObjectPatternType['rest']>,
};

export type MatchObjectPatternPropertyProps = {
  readonly key: MaybeDetachedNode<MatchObjectPatternPropertyType['key']>,
  readonly pattern: MaybeDetachedNode<
    MatchObjectPatternPropertyType['pattern'],
  >,
  readonly shorthand: MatchObjectPatternPropertyType['shorthand'],
};

export type MatchOrPatternProps = {
  readonly patterns: ReadonlyArray<
    MaybeDetachedNode<MatchOrPatternType['patterns'][number]>,
  >,
};

export type MatchRestPatternProps = {
  readonly argument?: ?MaybeDetachedNode<MatchRestPatternType['argument']>,
};

export type MatchStatementProps = {
  readonly argument: MaybeDetachedNode<MatchStatementType['argument']>,
  readonly cases: ReadonlyArray<
    MaybeDetachedNode<MatchStatementType['cases'][number]>,
  >,
};

export type MatchStatementCaseProps = {
  readonly pattern: MaybeDetachedNode<MatchStatementCaseType['pattern']>,
  readonly body: MaybeDetachedNode<MatchStatementCaseType['body']>,
  readonly guard?: ?MaybeDetachedNode<MatchStatementCaseType['guard']>,
};

export type MatchUnaryPatternProps = {
  readonly argument: MaybeDetachedNode<MatchUnaryPatternType['argument']>,
  readonly operator: MatchUnaryPatternType['operator'],
};

export type MatchWildcardPatternProps = {};

export type MetaPropertyProps = {
  readonly meta: MaybeDetachedNode<MetaPropertyType['meta']>,
  readonly property: MaybeDetachedNode<MetaPropertyType['property']>,
};

export type MethodDefinitionProps = {
  readonly key: MaybeDetachedNode<MethodDefinitionType['key']>,
  readonly value: MaybeDetachedNode<MethodDefinitionType['value']>,
  readonly kind: MethodDefinitionType['kind'],
  readonly computed: MethodDefinitionType['computed'],
  readonly static: MethodDefinitionType['static'],
  readonly decorators: ReadonlyArray<
    MaybeDetachedNode<MethodDefinitionType['decorators'][number]>,
  >,
  readonly override: MethodDefinitionType['override'],
  readonly tsAccessibility?: ?MethodDefinitionType['tsAccessibility'],
};

export type MixedTypeAnnotationProps = {};

export type NamespaceExportDeclarationProps = {
  readonly id: MaybeDetachedNode<NamespaceExportDeclarationType['id']>,
};

export type NeverTypeAnnotationProps = {};

export type NewExpressionProps = {
  readonly callee: MaybeDetachedNode<NewExpressionType['callee']>,
  readonly typeArguments?: ?MaybeDetachedNode<
    NewExpressionType['typeArguments'],
  >,
  readonly arguments: ReadonlyArray<
    MaybeDetachedNode<NewExpressionType['arguments'][number]>,
  >,
};

export type NullableTypeAnnotationProps = {
  readonly typeAnnotation: MaybeDetachedNode<
    NullableTypeAnnotationType['typeAnnotation'],
  >,
};

export type NullLiteralTypeAnnotationProps = {};

export type NumberLiteralTypeAnnotationProps = {
  readonly value: NumberLiteralTypeAnnotationType['value'],
  readonly raw: NumberLiteralTypeAnnotationType['raw'],
};

export type NumberTypeAnnotationProps = {};

export type ObjectExpressionProps = {
  readonly properties: ReadonlyArray<
    MaybeDetachedNode<ObjectExpressionType['properties'][number]>,
  >,
};

export type ObjectPatternProps = {
  readonly properties: ReadonlyArray<
    MaybeDetachedNode<ObjectPatternType['properties'][number]>,
  >,
  readonly typeAnnotation?: ?MaybeDetachedNode<
    ObjectPatternType['typeAnnotation'],
  >,
  readonly optional?: ?ObjectPatternType['optional'],
};

export type ObjectTypeAnnotationProps = {
  readonly properties: ReadonlyArray<
    MaybeDetachedNode<ObjectTypeAnnotationType['properties'][number]>,
  >,
  readonly indexers: ReadonlyArray<
    MaybeDetachedNode<ObjectTypeAnnotationType['indexers'][number]>,
  >,
  readonly callProperties: ReadonlyArray<
    MaybeDetachedNode<ObjectTypeAnnotationType['callProperties'][number]>,
  >,
  readonly internalSlots: ReadonlyArray<
    MaybeDetachedNode<ObjectTypeAnnotationType['internalSlots'][number]>,
  >,
  readonly inexact: ObjectTypeAnnotationType['inexact'],
  readonly exact: ObjectTypeAnnotationType['exact'],
};

export type ObjectTypeCallPropertyProps = {
  readonly value: MaybeDetachedNode<ObjectTypeCallPropertyType['value']>,
  readonly static: ObjectTypeCallPropertyType['static'],
};

export type ObjectTypeIndexerProps = {
  readonly id?: ?MaybeDetachedNode<ObjectTypeIndexerType['id']>,
  readonly key: MaybeDetachedNode<ObjectTypeIndexerType['key']>,
  readonly value: MaybeDetachedNode<ObjectTypeIndexerType['value']>,
  readonly static: ObjectTypeIndexerType['static'],
  readonly variance?: ?MaybeDetachedNode<ObjectTypeIndexerType['variance']>,
  readonly optional: ObjectTypeIndexerType['optional'],
};

export type ObjectTypeInternalSlotProps = {
  readonly id: MaybeDetachedNode<ObjectTypeInternalSlotType['id']>,
  readonly value: MaybeDetachedNode<ObjectTypeInternalSlotType['value']>,
  readonly optional: ObjectTypeInternalSlotType['optional'],
  readonly static: ObjectTypeInternalSlotType['static'],
  readonly method: ObjectTypeInternalSlotType['method'],
};

export type ObjectTypeMappedTypePropertyProps = {
  readonly keyTparam: MaybeDetachedNode<
    ObjectTypeMappedTypePropertyType['keyTparam'],
  >,
  readonly propType: MaybeDetachedNode<
    ObjectTypeMappedTypePropertyType['propType'],
  >,
  readonly sourceType: MaybeDetachedNode<
    ObjectTypeMappedTypePropertyType['sourceType'],
  >,
  readonly nameType?: ?MaybeDetachedNode<
    ObjectTypeMappedTypePropertyType['nameType'],
  >,
  readonly variance?: ?MaybeDetachedNode<
    ObjectTypeMappedTypePropertyType['variance'],
  >,
  readonly varianceOp?: ?ObjectTypeMappedTypePropertyType['varianceOp'],
  readonly optional?: ?ObjectTypeMappedTypePropertyType['optional'],
};

export type ObjectTypePrivateFieldProps = {
  readonly key: MaybeDetachedNode<ObjectTypePrivateFieldType['key']>,
};

export type ObjectTypeSpreadPropertyProps = {
  readonly argument: MaybeDetachedNode<
    ObjectTypeSpreadPropertyType['argument'],
  >,
};

export type OpaqueTypeProps = {
  readonly id: MaybeDetachedNode<OpaqueTypeType['id']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    OpaqueTypeType['typeParameters'],
  >,
  readonly impltype: MaybeDetachedNode<OpaqueTypeType['impltype']>,
  readonly lowerBound?: ?MaybeDetachedNode<OpaqueTypeType['lowerBound']>,
  readonly upperBound?: ?MaybeDetachedNode<OpaqueTypeType['upperBound']>,
  readonly supertype?: ?MaybeDetachedNode<OpaqueTypeType['supertype']>,
};

export type OptionalIndexedAccessTypeProps = {
  readonly objectType: MaybeDetachedNode<
    OptionalIndexedAccessTypeType['objectType'],
  >,
  readonly indexType: MaybeDetachedNode<
    OptionalIndexedAccessTypeType['indexType'],
  >,
  readonly optional: OptionalIndexedAccessTypeType['optional'],
};

export type PrivateIdentifierProps = {
  readonly name: PrivateIdentifierType['name'],
};

export type PropertyProps = {
  readonly key: MaybeDetachedNode<PropertyType['key']>,
  readonly value: MaybeDetachedNode<PropertyType['value']>,
  readonly kind: PropertyType['kind'],
  readonly computed: PropertyType['computed'],
  readonly method: PropertyType['method'],
  readonly shorthand: PropertyType['shorthand'],
};

export type PropertyDefinitionProps = {
  readonly key: MaybeDetachedNode<PropertyDefinitionType['key']>,
  readonly value?: ?MaybeDetachedNode<PropertyDefinitionType['value']>,
  readonly computed: PropertyDefinitionType['computed'],
  readonly static: PropertyDefinitionType['static'],
  readonly decorators: ReadonlyArray<
    MaybeDetachedNode<PropertyDefinitionType['decorators'][number]>,
  >,
  readonly declare: PropertyDefinitionType['declare'],
  readonly optional: PropertyDefinitionType['optional'],
  readonly variance?: ?MaybeDetachedNode<PropertyDefinitionType['variance']>,
  readonly typeAnnotation?: ?MaybeDetachedNode<
    PropertyDefinitionType['typeAnnotation'],
  >,
  readonly override: PropertyDefinitionType['override'],
  readonly tsAccessibility?: ?PropertyDefinitionType['tsAccessibility'],
};

export type QualifiedTypeIdentifierProps = {
  readonly qualification: MaybeDetachedNode<
    QualifiedTypeIdentifierType['qualification'],
  >,
  readonly id: MaybeDetachedNode<QualifiedTypeIdentifierType['id']>,
};

export type QualifiedTypeofIdentifierProps = {
  readonly qualification: MaybeDetachedNode<
    QualifiedTypeofIdentifierType['qualification'],
  >,
  readonly id: MaybeDetachedNode<QualifiedTypeofIdentifierType['id']>,
};

export type RecordDeclarationProps = {
  readonly id: MaybeDetachedNode<RecordDeclarationType['id']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    RecordDeclarationType['typeParameters'],
  >,
  readonly implements: ReadonlyArray<
    MaybeDetachedNode<RecordDeclarationType['implements'][number]>,
  >,
  readonly body: MaybeDetachedNode<RecordDeclarationType['body']>,
};

export type RecordDeclarationBodyProps = {
  readonly elements: ReadonlyArray<
    MaybeDetachedNode<RecordDeclarationBodyType['elements'][number]>,
  >,
};

export type RecordDeclarationImplementsProps = {
  readonly id: MaybeDetachedNode<RecordDeclarationImplementsType['id']>,
  readonly typeArguments?: ?MaybeDetachedNode<
    RecordDeclarationImplementsType['typeArguments'],
  >,
};

export type RecordDeclarationPropertyProps = {
  readonly key: MaybeDetachedNode<RecordDeclarationPropertyType['key']>,
  readonly typeAnnotation: MaybeDetachedNode<
    RecordDeclarationPropertyType['typeAnnotation'],
  >,
  readonly defaultValue?: ?MaybeDetachedNode<
    RecordDeclarationPropertyType['defaultValue'],
  >,
};

export type RecordDeclarationStaticPropertyProps = {
  readonly key: MaybeDetachedNode<RecordDeclarationStaticPropertyType['key']>,
  readonly typeAnnotation: MaybeDetachedNode<
    RecordDeclarationStaticPropertyType['typeAnnotation'],
  >,
  readonly value: MaybeDetachedNode<
    RecordDeclarationStaticPropertyType['value'],
  >,
};

export type RecordExpressionProps = {
  readonly recordConstructor: MaybeDetachedNode<
    RecordExpressionType['recordConstructor'],
  >,
  readonly typeArguments?: ?MaybeDetachedNode<
    RecordExpressionType['typeArguments'],
  >,
  readonly properties: MaybeDetachedNode<RecordExpressionType['properties']>,
};

export type RecordExpressionPropertiesProps = {
  readonly properties: ReadonlyArray<
    MaybeDetachedNode<RecordExpressionPropertiesType['properties'][number]>,
  >,
};

export type RestElementProps = {
  readonly argument: MaybeDetachedNode<RestElementType['argument']>,
};

export type ReturnStatementProps = {
  readonly argument?: ?MaybeDetachedNode<ReturnStatementType['argument']>,
};

export type SatisfiesExpressionProps = {
  readonly expression: MaybeDetachedNode<SatisfiesExpressionType['expression']>,
  readonly typeAnnotation: MaybeDetachedNode<
    SatisfiesExpressionType['typeAnnotation'],
  >,
};

export type SequenceExpressionProps = {
  readonly expressions: ReadonlyArray<
    MaybeDetachedNode<SequenceExpressionType['expressions'][number]>,
  >,
};

export type SpreadElementProps = {
  readonly argument: MaybeDetachedNode<SpreadElementType['argument']>,
};

export type StaticBlockProps = {
  readonly body: ReadonlyArray<
    MaybeDetachedNode<StaticBlockType['body'][number]>,
  >,
};

export type StringLiteralTypeAnnotationProps = {
  readonly value: StringLiteralTypeAnnotationType['value'],
  readonly raw: StringLiteralTypeAnnotationType['raw'],
};

export type StringTypeAnnotationProps = {};

export type SuperProps = {};

export type SwitchCaseProps = {
  readonly test?: ?MaybeDetachedNode<SwitchCaseType['test']>,
  readonly consequent: ReadonlyArray<
    MaybeDetachedNode<SwitchCaseType['consequent'][number]>,
  >,
};

export type SwitchStatementProps = {
  readonly discriminant: MaybeDetachedNode<SwitchStatementType['discriminant']>,
  readonly cases: ReadonlyArray<
    MaybeDetachedNode<SwitchStatementType['cases'][number]>,
  >,
};

export type SymbolTypeAnnotationProps = {};

export type TaggedTemplateExpressionProps = {
  readonly tag: MaybeDetachedNode<TaggedTemplateExpressionType['tag']>,
  readonly quasi: MaybeDetachedNode<TaggedTemplateExpressionType['quasi']>,
  readonly typeArguments?: ?MaybeDetachedNode<
    TaggedTemplateExpressionType['typeArguments'],
  >,
};

export type TemplateLiteralProps = {
  readonly quasis: ReadonlyArray<
    MaybeDetachedNode<TemplateLiteralType['quasis'][number]>,
  >,
  readonly expressions: ReadonlyArray<
    MaybeDetachedNode<TemplateLiteralType['expressions'][number]>,
  >,
};

export type TemplateLiteralTypeAnnotationProps = {
  readonly quasis: ReadonlyArray<
    MaybeDetachedNode<TemplateLiteralTypeAnnotationType['quasis'][number]>,
  >,
  readonly types: ReadonlyArray<
    MaybeDetachedNode<TemplateLiteralTypeAnnotationType['types'][number]>,
  >,
};

export type ThisExpressionProps = {};

export type ThisTypeAnnotationProps = {};

export type ThrowStatementProps = {
  readonly argument: MaybeDetachedNode<ThrowStatementType['argument']>,
};

export type TryStatementProps = {
  readonly block: MaybeDetachedNode<TryStatementType['block']>,
  readonly handler?: ?MaybeDetachedNode<TryStatementType['handler']>,
  readonly finalizer?: ?MaybeDetachedNode<TryStatementType['finalizer']>,
};

export type TupleTypeAnnotationProps = {
  readonly elementTypes: ReadonlyArray<
    MaybeDetachedNode<TupleTypeAnnotationType['elementTypes'][number]>,
  >,
  readonly inexact: TupleTypeAnnotationType['inexact'],
};

export type TupleTypeElementProps = {
  readonly elementType: MaybeDetachedNode<TupleTypeElementType['elementType']>,
  readonly optional: TupleTypeElementType['optional'],
};

export type TupleTypeLabeledElementProps = {
  readonly label: MaybeDetachedNode<TupleTypeLabeledElementType['label']>,
  readonly elementType: MaybeDetachedNode<
    TupleTypeLabeledElementType['elementType'],
  >,
  readonly optional: TupleTypeLabeledElementType['optional'],
  readonly variance?: ?MaybeDetachedNode<
    TupleTypeLabeledElementType['variance'],
  >,
};

export type TupleTypeSpreadElementProps = {
  readonly label?: ?MaybeDetachedNode<TupleTypeSpreadElementType['label']>,
  readonly typeAnnotation: MaybeDetachedNode<
    TupleTypeSpreadElementType['typeAnnotation'],
  >,
};

export type TypeAliasProps = {
  readonly id: MaybeDetachedNode<TypeAliasType['id']>,
  readonly typeParameters?: ?MaybeDetachedNode<TypeAliasType['typeParameters']>,
  readonly right: MaybeDetachedNode<TypeAliasType['right']>,
};

export type TypeAnnotationProps = {
  readonly typeAnnotation: MaybeDetachedNode<
    TypeAnnotationType['typeAnnotation'],
  >,
};

export type TypeCastExpressionProps = {
  readonly expression: MaybeDetachedNode<TypeCastExpressionType['expression']>,
  readonly typeAnnotation: MaybeDetachedNode<
    TypeCastExpressionType['typeAnnotation'],
  >,
};

export type TypeofTypeAnnotationProps = {
  readonly argument: MaybeDetachedNode<TypeofTypeAnnotationType['argument']>,
  readonly typeArguments?: ?MaybeDetachedNode<
    TypeofTypeAnnotationType['typeArguments'],
  >,
};

export type TypeOperatorProps = {
  readonly operator: TypeOperatorType['operator'],
  readonly typeAnnotation: MaybeDetachedNode<
    TypeOperatorType['typeAnnotation'],
  >,
};

export type TypeParameterProps = {
  readonly name: TypeParameterType['name'],
  readonly const: TypeParameterType['const'],
  readonly bound?: ?MaybeDetachedNode<TypeParameterType['bound']>,
  readonly variance?: ?MaybeDetachedNode<TypeParameterType['variance']>,
  readonly default?: ?MaybeDetachedNode<TypeParameterType['default']>,
  readonly usesExtendsBound: TypeParameterType['usesExtendsBound'],
};

export type TypeParameterDeclarationProps = {
  readonly params: ReadonlyArray<
    MaybeDetachedNode<TypeParameterDeclarationType['params'][number]>,
  >,
};

export type TypeParameterInstantiationProps = {
  readonly params: ReadonlyArray<
    MaybeDetachedNode<TypeParameterInstantiationType['params'][number]>,
  >,
};

export type TypePredicateProps = {
  readonly parameterName: MaybeDetachedNode<TypePredicateType['parameterName']>,
  readonly typeAnnotation?: ?MaybeDetachedNode<
    TypePredicateType['typeAnnotation'],
  >,
  readonly kind?: ?TypePredicateType['kind'],
};

export type UnaryExpressionProps = {
  readonly operator: UnaryExpressionType['operator'],
  readonly argument: MaybeDetachedNode<UnaryExpressionType['argument']>,
  readonly prefix: UnaryExpressionType['prefix'],
};

export type UndefinedTypeAnnotationProps = {};

export type UnionTypeAnnotationProps = {
  readonly types: ReadonlyArray<
    MaybeDetachedNode<UnionTypeAnnotationType['types'][number]>,
  >,
};

export type UnknownTypeAnnotationProps = {};

export type UpdateExpressionProps = {
  readonly operator: UpdateExpressionType['operator'],
  readonly argument: MaybeDetachedNode<UpdateExpressionType['argument']>,
  readonly prefix: UpdateExpressionType['prefix'],
};

export type VariableDeclarationProps = {
  readonly kind: VariableDeclarationType['kind'],
  readonly declarations: ReadonlyArray<
    MaybeDetachedNode<VariableDeclarationType['declarations'][number]>,
  >,
};

export type VariableDeclaratorProps = {
  readonly init?: ?MaybeDetachedNode<VariableDeclaratorType['init']>,
  readonly id: MaybeDetachedNode<VariableDeclaratorType['id']>,
};

export type VarianceProps = {
  readonly kind: VarianceType['kind'],
};

export type VoidTypeAnnotationProps = {};

export type WhileStatementProps = {
  readonly body: MaybeDetachedNode<WhileStatementType['body']>,
  readonly test: MaybeDetachedNode<WhileStatementType['test']>,
};

export type WithStatementProps = {
  readonly object: MaybeDetachedNode<WithStatementType['object']>,
  readonly body: MaybeDetachedNode<WithStatementType['body']>,
};

export type YieldExpressionProps = {
  readonly argument?: ?MaybeDetachedNode<YieldExpressionType['argument']>,
  readonly delegate?: ?YieldExpressionType['delegate'],
};

export function AbstractMethodDefinition(props: {
  ...AbstractMethodDefinitionProps,
  readonly parent?: ESNode,
}): DetachedNode<AbstractMethodDefinitionType> {
  const node = detachedProps<AbstractMethodDefinitionType>(
    props.parent as $FlowFixMe,
    {
      type: 'AbstractMethodDefinition',
      key: asDetachedNodeForCodeGen(props.key),
      value: asDetachedNodeForCodeGen(props.value),
      computed: props.computed,
      override: props.override,
      tsAccessibility: props.tsAccessibility,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function AbstractPropertyDefinition(props: {
  ...AbstractPropertyDefinitionProps,
  readonly parent?: ESNode,
}): DetachedNode<AbstractPropertyDefinitionType> {
  const node = detachedProps<AbstractPropertyDefinitionType>(
    props.parent as $FlowFixMe,
    {
      type: 'AbstractPropertyDefinition',
      key: asDetachedNodeForCodeGen(props.key),
      value: asDetachedNodeForCodeGen(props.value),
      typeAnnotation: asDetachedNodeForCodeGen(props.typeAnnotation),
      computed: props.computed,
      variance: asDetachedNodeForCodeGen(props.variance),
      override: props.override,
      tsAccessibility: props.tsAccessibility,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function AnyTypeAnnotation(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<AnyTypeAnnotationType> {
  return detachedProps<AnyTypeAnnotationType>(props.parent as $FlowFixMe, {
    type: 'AnyTypeAnnotation',
  });
}

export function ArrayExpression(props: {
  ...ArrayExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<ArrayExpressionType> {
  const node = detachedProps<ArrayExpressionType>(props.parent as $FlowFixMe, {
    type: 'ArrayExpression',
    elements: props.elements.map(n => asDetachedNodeForCodeGen(n)),
    trailingComma: props.trailingComma,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ArrayPattern(props: {
  ...ArrayPatternProps,
  readonly parent?: ESNode,
}): DetachedNode<ArrayPatternType> {
  const node = detachedProps<ArrayPatternType>(props.parent as $FlowFixMe, {
    type: 'ArrayPattern',
    elements: props.elements.map(n => asDetachedNodeForCodeGen(n)),
    typeAnnotation: asDetachedNodeForCodeGen(props.typeAnnotation),
    optional: props.optional,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ArrayTypeAnnotation(props: {
  ...ArrayTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<ArrayTypeAnnotationType> {
  const node = detachedProps<ArrayTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'ArrayTypeAnnotation',
      elementType: asDetachedNodeForCodeGen(props.elementType),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function AsConstExpression(props: {
  ...AsConstExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<AsConstExpressionType> {
  const node = detachedProps<AsConstExpressionType>(
    props.parent as $FlowFixMe,
    {
      type: 'AsConstExpression',
      expression: asDetachedNodeForCodeGen(props.expression),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function AsExpression(props: {
  ...AsExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<AsExpressionType> {
  const node = detachedProps<AsExpressionType>(props.parent as $FlowFixMe, {
    type: 'AsExpression',
    expression: asDetachedNodeForCodeGen(props.expression),
    typeAnnotation: asDetachedNodeForCodeGen(props.typeAnnotation),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function AssignmentExpression(props: {
  ...AssignmentExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<AssignmentExpressionType> {
  const node = detachedProps<AssignmentExpressionType>(
    props.parent as $FlowFixMe,
    {
      type: 'AssignmentExpression',
      operator: props.operator,
      left: asDetachedNodeForCodeGen(props.left),
      right: asDetachedNodeForCodeGen(props.right),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function AssignmentPattern(props: {
  ...AssignmentPatternProps,
  readonly parent?: ESNode,
}): DetachedNode<AssignmentPatternType> {
  const node = detachedProps<AssignmentPatternType>(
    props.parent as $FlowFixMe,
    {
      type: 'AssignmentPattern',
      left: asDetachedNodeForCodeGen(props.left),
      right: asDetachedNodeForCodeGen(props.right),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function AwaitExpression(props: {
  ...AwaitExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<AwaitExpressionType> {
  const node = detachedProps<AwaitExpressionType>(props.parent as $FlowFixMe, {
    type: 'AwaitExpression',
    argument: asDetachedNodeForCodeGen(props.argument),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function BigIntLiteralTypeAnnotation(props: {
  ...BigIntLiteralTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<BigIntLiteralTypeAnnotationType> {
  const node = detachedProps<BigIntLiteralTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'BigIntLiteralTypeAnnotation',
      raw: props.raw,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function BigIntTypeAnnotation(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<BigIntTypeAnnotationType> {
  return detachedProps<BigIntTypeAnnotationType>(props.parent as $FlowFixMe, {
    type: 'BigIntTypeAnnotation',
  });
}

export function BinaryExpression(props: {
  ...BinaryExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<BinaryExpressionType> {
  const node = detachedProps<BinaryExpressionType>(props.parent as $FlowFixMe, {
    type: 'BinaryExpression',
    left: asDetachedNodeForCodeGen(props.left),
    right: asDetachedNodeForCodeGen(props.right),
    operator: props.operator,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function BooleanLiteralTypeAnnotation(props: {
  ...BooleanLiteralTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<BooleanLiteralTypeAnnotationType> {
  const node = detachedProps<BooleanLiteralTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'BooleanLiteralTypeAnnotation',
      value: props.value,
      raw: props.raw,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function BooleanTypeAnnotation(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<BooleanTypeAnnotationType> {
  return detachedProps<BooleanTypeAnnotationType>(props.parent as $FlowFixMe, {
    type: 'BooleanTypeAnnotation',
  });
}

export function BreakStatement(props: {
  ...BreakStatementProps,
  readonly parent?: ESNode,
}): DetachedNode<BreakStatementType> {
  const node = detachedProps<BreakStatementType>(props.parent as $FlowFixMe, {
    type: 'BreakStatement',
    label: asDetachedNodeForCodeGen(props.label),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function CallExpression(props: {
  ...CallExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<CallExpressionType> {
  const node = detachedProps<CallExpressionType>(props.parent as $FlowFixMe, {
    type: 'CallExpression',
    callee: asDetachedNodeForCodeGen(props.callee),
    typeArguments: asDetachedNodeForCodeGen(props.typeArguments),
    arguments: props.arguments.map(n => asDetachedNodeForCodeGen(n)),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function CatchClause(props: {
  ...CatchClauseProps,
  readonly parent?: ESNode,
}): DetachedNode<CatchClauseType> {
  const node = detachedProps<CatchClauseType>(props.parent as $FlowFixMe, {
    type: 'CatchClause',
    param: asDetachedNodeForCodeGen(props.param),
    body: asDetachedNodeForCodeGen(props.body),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ChainExpression(props: {
  ...ChainExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<ChainExpressionType> {
  const node = detachedProps<ChainExpressionType>(props.parent as $FlowFixMe, {
    type: 'ChainExpression',
    expression: asDetachedNodeForCodeGen(props.expression),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ClassBody(props: {
  ...ClassBodyProps,
  readonly parent?: ESNode,
}): DetachedNode<ClassBodyType> {
  const node = detachedProps<ClassBodyType>(props.parent as $FlowFixMe, {
    type: 'ClassBody',
    body: props.body.map(n => asDetachedNodeForCodeGen(n)),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ClassExpression(props: {
  ...ClassExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<ClassExpressionType> {
  const node = detachedProps<ClassExpressionType>(props.parent as $FlowFixMe, {
    type: 'ClassExpression',
    id: asDetachedNodeForCodeGen(props.id),
    typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
    superClass: asDetachedNodeForCodeGen(props.superClass),
    superTypeArguments: asDetachedNodeForCodeGen(props.superTypeArguments),
    implements: props.implements.map(n => asDetachedNodeForCodeGen(n)),
    decorators: props.decorators.map(n => asDetachedNodeForCodeGen(n)),
    body: asDetachedNodeForCodeGen(props.body),
    abstract: props.abstract,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ClassImplements(props: {
  ...ClassImplementsProps,
  readonly parent?: ESNode,
}): DetachedNode<ClassImplementsType> {
  const node = detachedProps<ClassImplementsType>(props.parent as $FlowFixMe, {
    type: 'ClassImplements',
    id: asDetachedNodeForCodeGen(props.id),
    typeArguments: asDetachedNodeForCodeGen(props.typeArguments),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ComponentDeclaration(props: {
  ...ComponentDeclarationProps,
  readonly parent?: ESNode,
}): DetachedNode<ComponentDeclarationType> {
  const node = detachedProps<ComponentDeclarationType>(
    props.parent as $FlowFixMe,
    {
      type: 'ComponentDeclaration',
      id: asDetachedNodeForCodeGen(props.id),
      params: props.params.map(n => asDetachedNodeForCodeGen(n)),
      body: asDetachedNodeForCodeGen(props.body),
      typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
      rendersType: asDetachedNodeForCodeGen(props.rendersType),
      async: props.async,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ComponentParameter(props: {
  ...ComponentParameterProps,
  readonly parent?: ESNode,
}): DetachedNode<ComponentParameterType> {
  const node = detachedProps<ComponentParameterType>(
    props.parent as $FlowFixMe,
    {
      type: 'ComponentParameter',
      name: asDetachedNodeForCodeGen(props.name),
      local: asDetachedNodeForCodeGen(props.local),
      shorthand: props.shorthand,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ComponentTypeAnnotation(props: {
  ...ComponentTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<ComponentTypeAnnotationType> {
  const node = detachedProps<ComponentTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'ComponentTypeAnnotation',
      params: props.params.map(n => asDetachedNodeForCodeGen(n)),
      rest: asDetachedNodeForCodeGen(props.rest),
      typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
      rendersType: asDetachedNodeForCodeGen(props.rendersType),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ComponentTypeParameter(props: {
  ...ComponentTypeParameterProps,
  readonly parent?: ESNode,
}): DetachedNode<ComponentTypeParameterType> {
  const node = detachedProps<ComponentTypeParameterType>(
    props.parent as $FlowFixMe,
    {
      type: 'ComponentTypeParameter',
      name: asDetachedNodeForCodeGen(props.name),
      typeAnnotation: asDetachedNodeForCodeGen(props.typeAnnotation),
      optional: props.optional,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ConditionalExpression(props: {
  ...ConditionalExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<ConditionalExpressionType> {
  const node = detachedProps<ConditionalExpressionType>(
    props.parent as $FlowFixMe,
    {
      type: 'ConditionalExpression',
      test: asDetachedNodeForCodeGen(props.test),
      alternate: asDetachedNodeForCodeGen(props.alternate),
      consequent: asDetachedNodeForCodeGen(props.consequent),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ConditionalTypeAnnotation(props: {
  ...ConditionalTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<ConditionalTypeAnnotationType> {
  const node = detachedProps<ConditionalTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'ConditionalTypeAnnotation',
      checkType: asDetachedNodeForCodeGen(props.checkType),
      extendsType: asDetachedNodeForCodeGen(props.extendsType),
      trueType: asDetachedNodeForCodeGen(props.trueType),
      falseType: asDetachedNodeForCodeGen(props.falseType),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ConstructorTypeAnnotation(props: {
  ...ConstructorTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<ConstructorTypeAnnotationType> {
  const node = detachedProps<ConstructorTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'ConstructorTypeAnnotation',
      abstract: props.abstract,
      params: props.params.map(n => asDetachedNodeForCodeGen(n)),
      returnType: asDetachedNodeForCodeGen(props.returnType),
      rest: asDetachedNodeForCodeGen(props.rest),
      typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ContinueStatement(props: {
  ...ContinueStatementProps,
  readonly parent?: ESNode,
}): DetachedNode<ContinueStatementType> {
  const node = detachedProps<ContinueStatementType>(
    props.parent as $FlowFixMe,
    {
      type: 'ContinueStatement',
      label: asDetachedNodeForCodeGen(props.label),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function DebuggerStatement(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<DebuggerStatementType> {
  return detachedProps<DebuggerStatementType>(props.parent as $FlowFixMe, {
    type: 'DebuggerStatement',
  });
}

export function DeclareClass(props: {
  ...DeclareClassProps,
  readonly parent?: ESNode,
}): DetachedNode<DeclareClassType> {
  const node = detachedProps<DeclareClassType>(props.parent as $FlowFixMe, {
    type: 'DeclareClass',
    id: asDetachedNodeForCodeGen(props.id),
    typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
    extends: props.extends.map(n => asDetachedNodeForCodeGen(n)),
    implements: props.implements.map(n => asDetachedNodeForCodeGen(n)),
    mixins: props.mixins.map(n => asDetachedNodeForCodeGen(n)),
    body: asDetachedNodeForCodeGen(props.body),
    abstract: props.abstract,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function DeclareClassExtendsCall(props: {
  ...DeclareClassExtendsCallProps,
  readonly parent?: ESNode,
}): DetachedNode<DeclareClassExtendsCallType> {
  const node = detachedProps<DeclareClassExtendsCallType>(
    props.parent as $FlowFixMe,
    {
      type: 'DeclareClassExtendsCall',
      callee: asDetachedNodeForCodeGen(props.callee),
      argument: asDetachedNodeForCodeGen(props.argument),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function DeclareComponent(props: {
  ...DeclareComponentProps,
  readonly parent?: ESNode,
}): DetachedNode<DeclareComponentType> {
  const node = detachedProps<DeclareComponentType>(props.parent as $FlowFixMe, {
    type: 'DeclareComponent',
    id: asDetachedNodeForCodeGen(props.id),
    params: props.params.map(n => asDetachedNodeForCodeGen(n)),
    rest: asDetachedNodeForCodeGen(props.rest),
    typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
    rendersType: asDetachedNodeForCodeGen(props.rendersType),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function DeclaredPredicate(props: {
  ...DeclaredPredicateProps,
  readonly parent?: ESNode,
}): DetachedNode<DeclaredPredicateType> {
  const node = detachedProps<DeclaredPredicateType>(
    props.parent as $FlowFixMe,
    {
      type: 'DeclaredPredicate',
      value: asDetachedNodeForCodeGen(props.value),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function DeclareEnum(props: {
  ...DeclareEnumProps,
  readonly parent?: ESNode,
}): DetachedNode<DeclareEnumType> {
  const node = detachedProps<DeclareEnumType>(props.parent as $FlowFixMe, {
    type: 'DeclareEnum',
    id: asDetachedNodeForCodeGen(props.id),
    body: asDetachedNodeForCodeGen(props.body),
    const: props.const,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function DeclareExportAllDeclaration(props: {
  ...DeclareExportAllDeclarationProps,
  readonly parent?: ESNode,
}): DetachedNode<DeclareExportAllDeclarationType> {
  const node = detachedProps<DeclareExportAllDeclarationType>(
    props.parent as $FlowFixMe,
    {
      type: 'DeclareExportAllDeclaration',
      source: asDetachedNodeForCodeGen(props.source),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function DeclareInterface(props: {
  ...DeclareInterfaceProps,
  readonly parent?: ESNode,
}): DetachedNode<DeclareInterfaceType> {
  const node = detachedProps<DeclareInterfaceType>(props.parent as $FlowFixMe, {
    type: 'DeclareInterface',
    id: asDetachedNodeForCodeGen(props.id),
    typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
    extends: props.extends.map(n => asDetachedNodeForCodeGen(n)),
    body: asDetachedNodeForCodeGen(props.body),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function DeclareModule(props: {
  ...DeclareModuleProps,
  readonly parent?: ESNode,
}): DetachedNode<DeclareModuleType> {
  const node = detachedProps<DeclareModuleType>(props.parent as $FlowFixMe, {
    type: 'DeclareModule',
    id: asDetachedNodeForCodeGen(props.id),
    body: asDetachedNodeForCodeGen(props.body),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function DeclareModuleExports(props: {
  ...DeclareModuleExportsProps,
  readonly parent?: ESNode,
}): DetachedNode<DeclareModuleExportsType> {
  const node = detachedProps<DeclareModuleExportsType>(
    props.parent as $FlowFixMe,
    {
      type: 'DeclareModuleExports',
      typeAnnotation: asDetachedNodeForCodeGen(props.typeAnnotation),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function DeclareNamespace(props: {
  ...DeclareNamespaceProps,
  readonly parent?: ESNode,
}): DetachedNode<DeclareNamespaceType> {
  const node = detachedProps<DeclareNamespaceType>(props.parent as $FlowFixMe, {
    type: 'DeclareNamespace',
    id: asDetachedNodeForCodeGen(props.id),
    body: asDetachedNodeForCodeGen(props.body),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function DeclareOpaqueType(props: {
  ...DeclareOpaqueTypeProps,
  readonly parent?: ESNode,
}): DetachedNode<DeclareOpaqueTypeType> {
  const node = detachedProps<DeclareOpaqueTypeType>(
    props.parent as $FlowFixMe,
    {
      type: 'DeclareOpaqueType',
      id: asDetachedNodeForCodeGen(props.id),
      typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
      impltype: asDetachedNodeForCodeGen(props.impltype),
      lowerBound: asDetachedNodeForCodeGen(props.lowerBound),
      upperBound: asDetachedNodeForCodeGen(props.upperBound),
      supertype: asDetachedNodeForCodeGen(props.supertype),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function DeclareTypeAlias(props: {
  ...DeclareTypeAliasProps,
  readonly parent?: ESNode,
}): DetachedNode<DeclareTypeAliasType> {
  const node = detachedProps<DeclareTypeAliasType>(props.parent as $FlowFixMe, {
    type: 'DeclareTypeAlias',
    id: asDetachedNodeForCodeGen(props.id),
    typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
    right: asDetachedNodeForCodeGen(props.right),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function DeclareVariable(props: {
  ...DeclareVariableProps,
  readonly parent?: ESNode,
}): DetachedNode<DeclareVariableType> {
  const node = detachedProps<DeclareVariableType>(props.parent as $FlowFixMe, {
    type: 'DeclareVariable',
    declarations: props.declarations.map(n => asDetachedNodeForCodeGen(n)),
    kind: props.kind,
    implicitDeclare: props.implicitDeclare,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function Decorator(props: {
  ...DecoratorProps,
  readonly parent?: ESNode,
}): DetachedNode<DecoratorType> {
  const node = detachedProps<DecoratorType>(props.parent as $FlowFixMe, {
    type: 'Decorator',
    expression: asDetachedNodeForCodeGen(props.expression),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function DoWhileStatement(props: {
  ...DoWhileStatementProps,
  readonly parent?: ESNode,
}): DetachedNode<DoWhileStatementType> {
  const node = detachedProps<DoWhileStatementType>(props.parent as $FlowFixMe, {
    type: 'DoWhileStatement',
    body: asDetachedNodeForCodeGen(props.body),
    test: asDetachedNodeForCodeGen(props.test),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function EmptyStatement(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<EmptyStatementType> {
  return detachedProps<EmptyStatementType>(props.parent as $FlowFixMe, {
    type: 'EmptyStatement',
  });
}

export function EmptyTypeAnnotation(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<EmptyTypeAnnotationType> {
  return detachedProps<EmptyTypeAnnotationType>(props.parent as $FlowFixMe, {
    type: 'EmptyTypeAnnotation',
  });
}

export function EnumBigIntMember(props: {
  ...EnumBigIntMemberProps,
  readonly parent?: ESNode,
}): DetachedNode<EnumBigIntMemberType> {
  const node = detachedProps<EnumBigIntMemberType>(props.parent as $FlowFixMe, {
    type: 'EnumBigIntMember',
    id: asDetachedNodeForCodeGen(props.id),
    init: asDetachedNodeForCodeGen(props.init),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function EnumBody(props: {
  ...EnumBodyProps,
  readonly parent?: ESNode,
}): DetachedNode<EnumBodyType> {
  const node = detachedProps<EnumBodyType>(props.parent as $FlowFixMe, {
    type: 'EnumBody',
    members: props.members.map(n => asDetachedNodeForCodeGen(n)),
    explicitType: props.explicitType,
    hasUnknownMembers: props.hasUnknownMembers,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function EnumBooleanMember(props: {
  ...EnumBooleanMemberProps,
  readonly parent?: ESNode,
}): DetachedNode<EnumBooleanMemberType> {
  const node = detachedProps<EnumBooleanMemberType>(
    props.parent as $FlowFixMe,
    {
      type: 'EnumBooleanMember',
      id: asDetachedNodeForCodeGen(props.id),
      init: asDetachedNodeForCodeGen(props.init),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function EnumDeclaration(props: {
  ...EnumDeclarationProps,
  readonly parent?: ESNode,
}): DetachedNode<EnumDeclarationType> {
  const node = detachedProps<EnumDeclarationType>(props.parent as $FlowFixMe, {
    type: 'EnumDeclaration',
    id: asDetachedNodeForCodeGen(props.id),
    body: asDetachedNodeForCodeGen(props.body),
    const: props.const,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function EnumDefaultedMember(props: {
  ...EnumDefaultedMemberProps,
  readonly parent?: ESNode,
}): DetachedNode<EnumDefaultedMemberType> {
  const node = detachedProps<EnumDefaultedMemberType>(
    props.parent as $FlowFixMe,
    {
      type: 'EnumDefaultedMember',
      id: asDetachedNodeForCodeGen(props.id),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function EnumNumberMember(props: {
  ...EnumNumberMemberProps,
  readonly parent?: ESNode,
}): DetachedNode<EnumNumberMemberType> {
  const node = detachedProps<EnumNumberMemberType>(props.parent as $FlowFixMe, {
    type: 'EnumNumberMember',
    id: asDetachedNodeForCodeGen(props.id),
    init: asDetachedNodeForCodeGen(props.init),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function EnumStringMember(props: {
  ...EnumStringMemberProps,
  readonly parent?: ESNode,
}): DetachedNode<EnumStringMemberType> {
  const node = detachedProps<EnumStringMemberType>(props.parent as $FlowFixMe, {
    type: 'EnumStringMember',
    id: asDetachedNodeForCodeGen(props.id),
    init: asDetachedNodeForCodeGen(props.init),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ExistsTypeAnnotation(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<ExistsTypeAnnotationType> {
  return detachedProps<ExistsTypeAnnotationType>(props.parent as $FlowFixMe, {
    type: 'ExistsTypeAnnotation',
  });
}

export function ExportAssignment(props: {
  ...ExportAssignmentProps,
  readonly parent?: ESNode,
}): DetachedNode<ExportAssignmentType> {
  const node = detachedProps<ExportAssignmentType>(props.parent as $FlowFixMe, {
    type: 'ExportAssignment',
    expression: asDetachedNodeForCodeGen(props.expression),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ExportAllDeclaration(props: {
  ...ExportAllDeclarationProps,
  readonly parent?: ESNode,
}): DetachedNode<ExportAllDeclarationType> {
  const node = detachedProps<ExportAllDeclarationType>(
    props.parent as $FlowFixMe,
    {
      type: 'ExportAllDeclaration',
      exported: asDetachedNodeForCodeGen(props.exported),
      source: asDetachedNodeForCodeGen(props.source),
      exportKind: props.exportKind,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ExportDefaultDeclaration(props: {
  ...ExportDefaultDeclarationProps,
  readonly parent?: ESNode,
}): DetachedNode<ExportDefaultDeclarationType> {
  const node = detachedProps<ExportDefaultDeclarationType>(
    props.parent as $FlowFixMe,
    {
      type: 'ExportDefaultDeclaration',
      declaration: asDetachedNodeForCodeGen(props.declaration),
      exportKind: 'value',
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ExportSpecifier(props: {
  ...ExportSpecifierProps,
  readonly parent?: ESNode,
}): DetachedNode<ExportSpecifierType> {
  const node = detachedProps<ExportSpecifierType>(props.parent as $FlowFixMe, {
    type: 'ExportSpecifier',
    exported: asDetachedNodeForCodeGen(props.exported),
    local: asDetachedNodeForCodeGen(props.local),
    exportKind: props.exportKind,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ExpressionStatement(props: {
  ...ExpressionStatementProps,
  readonly parent?: ESNode,
}): DetachedNode<ExpressionStatementType> {
  const node = detachedProps<ExpressionStatementType>(
    props.parent as $FlowFixMe,
    {
      type: 'ExpressionStatement',
      expression: asDetachedNodeForCodeGen(props.expression),
      directive: props.directive,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ExternalModuleReference(props: {
  ...ExternalModuleReferenceProps,
  readonly parent?: ESNode,
}): DetachedNode<ExternalModuleReferenceType> {
  const node = detachedProps<ExternalModuleReferenceType>(
    props.parent as $FlowFixMe,
    {
      type: 'ExternalModuleReference',
      expression: asDetachedNodeForCodeGen(props.expression),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ForInStatement(props: {
  ...ForInStatementProps,
  readonly parent?: ESNode,
}): DetachedNode<ForInStatementType> {
  const node = detachedProps<ForInStatementType>(props.parent as $FlowFixMe, {
    type: 'ForInStatement',
    left: asDetachedNodeForCodeGen(props.left),
    right: asDetachedNodeForCodeGen(props.right),
    body: asDetachedNodeForCodeGen(props.body),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ForOfStatement(props: {
  ...ForOfStatementProps,
  readonly parent?: ESNode,
}): DetachedNode<ForOfStatementType> {
  const node = detachedProps<ForOfStatementType>(props.parent as $FlowFixMe, {
    type: 'ForOfStatement',
    left: asDetachedNodeForCodeGen(props.left),
    right: asDetachedNodeForCodeGen(props.right),
    body: asDetachedNodeForCodeGen(props.body),
    await: props.await,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ForStatement(props: {
  ...ForStatementProps,
  readonly parent?: ESNode,
}): DetachedNode<ForStatementType> {
  const node = detachedProps<ForStatementType>(props.parent as $FlowFixMe, {
    type: 'ForStatement',
    init: asDetachedNodeForCodeGen(props.init),
    test: asDetachedNodeForCodeGen(props.test),
    update: asDetachedNodeForCodeGen(props.update),
    body: asDetachedNodeForCodeGen(props.body),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function FunctionDeclaration(props: {
  ...FunctionDeclarationProps,
  readonly parent?: ESNode,
}): DetachedNode<FunctionDeclarationType> {
  const node = detachedProps<FunctionDeclarationType>(
    props.parent as $FlowFixMe,
    {
      type: 'FunctionDeclaration',
      id: asDetachedNodeForCodeGen(props.id),
      params: props.params.map(n => asDetachedNodeForCodeGen(n)),
      body: asDetachedNodeForCodeGen(props.body),
      typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
      returnType: asDetachedNodeForCodeGen(props.returnType),
      predicate: asDetachedNodeForCodeGen(props.predicate),
      generator: props.generator,
      async: props.async,
      expression: false,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function FunctionExpression(props: {
  ...FunctionExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<FunctionExpressionType> {
  const node = detachedProps<FunctionExpressionType>(
    props.parent as $FlowFixMe,
    {
      type: 'FunctionExpression',
      id: asDetachedNodeForCodeGen(props.id),
      params: props.params.map(n => asDetachedNodeForCodeGen(n)),
      body: asDetachedNodeForCodeGen(props.body),
      typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
      returnType: asDetachedNodeForCodeGen(props.returnType),
      predicate: asDetachedNodeForCodeGen(props.predicate),
      generator: props.generator,
      async: props.async,
      expression: false,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function FunctionTypeAnnotation(props: {
  ...FunctionTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<FunctionTypeAnnotationType> {
  const node = detachedProps<FunctionTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'FunctionTypeAnnotation',
      params: props.params.map(n => asDetachedNodeForCodeGen(n)),
      this: asDetachedNodeForCodeGen(props.this),
      returnType: asDetachedNodeForCodeGen(props.returnType),
      rest: asDetachedNodeForCodeGen(props.rest),
      typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function FunctionTypeParam(props: {
  ...FunctionTypeParamProps,
  readonly parent?: ESNode,
}): DetachedNode<FunctionTypeParamType> {
  const node = detachedProps<FunctionTypeParamType>(
    props.parent as $FlowFixMe,
    {
      type: 'FunctionTypeParam',
      name: asDetachedNodeForCodeGen(props.name),
      typeAnnotation: asDetachedNodeForCodeGen(props.typeAnnotation),
      optional: props.optional,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function GenericTypeAnnotation(props: {
  ...GenericTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<GenericTypeAnnotationType> {
  const node = detachedProps<GenericTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'GenericTypeAnnotation',
      id: asDetachedNodeForCodeGen(props.id),
      typeArguments: asDetachedNodeForCodeGen(props.typeArguments),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function HookDeclaration(props: {
  ...HookDeclarationProps,
  readonly parent?: ESNode,
}): DetachedNode<HookDeclarationType> {
  const node = detachedProps<HookDeclarationType>(props.parent as $FlowFixMe, {
    type: 'HookDeclaration',
    id: asDetachedNodeForCodeGen(props.id),
    params: props.params.map(n => asDetachedNodeForCodeGen(n)),
    body: asDetachedNodeForCodeGen(props.body),
    typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
    returnType: asDetachedNodeForCodeGen(props.returnType),
    async: props.async,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function HookTypeAnnotation(props: {
  ...HookTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<HookTypeAnnotationType> {
  const node = detachedProps<HookTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'HookTypeAnnotation',
      params: props.params.map(n => asDetachedNodeForCodeGen(n)),
      returnType: asDetachedNodeForCodeGen(props.returnType),
      rest: asDetachedNodeForCodeGen(props.rest),
      typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function IfStatement(props: {
  ...IfStatementProps,
  readonly parent?: ESNode,
}): DetachedNode<IfStatementType> {
  const node = detachedProps<IfStatementType>(props.parent as $FlowFixMe, {
    type: 'IfStatement',
    test: asDetachedNodeForCodeGen(props.test),
    consequent: asDetachedNodeForCodeGen(props.consequent),
    alternate: asDetachedNodeForCodeGen(props.alternate),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ImportAttribute(props: {
  ...ImportAttributeProps,
  readonly parent?: ESNode,
}): DetachedNode<ImportAttributeType> {
  const node = detachedProps<ImportAttributeType>(props.parent as $FlowFixMe, {
    type: 'ImportAttribute',
    key: asDetachedNodeForCodeGen(props.key),
    value: asDetachedNodeForCodeGen(props.value),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ImportDeclaration(props: {
  ...ImportDeclarationProps,
  readonly parent?: ESNode,
}): DetachedNode<ImportDeclarationType> {
  const node = detachedProps<ImportDeclarationType>(
    props.parent as $FlowFixMe,
    {
      type: 'ImportDeclaration',
      specifiers: props.specifiers.map(n => asDetachedNodeForCodeGen(n)),
      source: asDetachedNodeForCodeGen(props.source),
      attributes: props.attributes?.map(n => asDetachedNodeForCodeGen(n)),
      importKind: props.importKind,
      phase: props.phase,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ImportDefaultSpecifier(props: {
  ...ImportDefaultSpecifierProps,
  readonly parent?: ESNode,
}): DetachedNode<ImportDefaultSpecifierType> {
  const node = detachedProps<ImportDefaultSpecifierType>(
    props.parent as $FlowFixMe,
    {
      type: 'ImportDefaultSpecifier',
      local: asDetachedNodeForCodeGen(props.local),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ImportEqualsDeclaration(props: {
  ...ImportEqualsDeclarationProps,
  readonly parent?: ESNode,
}): DetachedNode<ImportEqualsDeclarationType> {
  const node = detachedProps<ImportEqualsDeclarationType>(
    props.parent as $FlowFixMe,
    {
      type: 'ImportEqualsDeclaration',
      id: asDetachedNodeForCodeGen(props.id),
      moduleReference: asDetachedNodeForCodeGen(props.moduleReference),
      importKind: props.importKind,
      isExport: props.isExport,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ImportExpression(props: {
  ...ImportExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<ImportExpressionType> {
  const node = detachedProps<ImportExpressionType>(props.parent as $FlowFixMe, {
    type: 'ImportExpression',
    source: asDetachedNodeForCodeGen(props.source),
    options: asDetachedNodeForCodeGen(props.options),
    phase: props.phase,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ImportNamespaceSpecifier(props: {
  ...ImportNamespaceSpecifierProps,
  readonly parent?: ESNode,
}): DetachedNode<ImportNamespaceSpecifierType> {
  const node = detachedProps<ImportNamespaceSpecifierType>(
    props.parent as $FlowFixMe,
    {
      type: 'ImportNamespaceSpecifier',
      local: asDetachedNodeForCodeGen(props.local),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ImportSpecifier(props: {
  ...ImportSpecifierProps,
  readonly parent?: ESNode,
}): DetachedNode<ImportSpecifierType> {
  const node = detachedProps<ImportSpecifierType>(props.parent as $FlowFixMe, {
    type: 'ImportSpecifier',
    imported: asDetachedNodeForCodeGen(props.imported),
    local: asDetachedNodeForCodeGen(props.local),
    importKind: props.importKind,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ImportType(props: {
  ...ImportTypeProps,
  readonly parent?: ESNode,
}): DetachedNode<ImportTypeType> {
  const node = detachedProps<ImportTypeType>(props.parent as $FlowFixMe, {
    type: 'ImportType',
    source: asDetachedNodeForCodeGen(props.source),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function IndexedAccessType(props: {
  ...IndexedAccessTypeProps,
  readonly parent?: ESNode,
}): DetachedNode<IndexedAccessTypeType> {
  const node = detachedProps<IndexedAccessTypeType>(
    props.parent as $FlowFixMe,
    {
      type: 'IndexedAccessType',
      objectType: asDetachedNodeForCodeGen(props.objectType),
      indexType: asDetachedNodeForCodeGen(props.indexType),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function InferredPredicate(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<InferredPredicateType> {
  return detachedProps<InferredPredicateType>(props.parent as $FlowFixMe, {
    type: 'InferredPredicate',
  });
}

export function InferTypeAnnotation(props: {
  ...InferTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<InferTypeAnnotationType> {
  const node = detachedProps<InferTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'InferTypeAnnotation',
      typeParameter: asDetachedNodeForCodeGen(props.typeParameter),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function InterfaceDeclaration(props: {
  ...InterfaceDeclarationProps,
  readonly parent?: ESNode,
}): DetachedNode<InterfaceDeclarationType> {
  const node = detachedProps<InterfaceDeclarationType>(
    props.parent as $FlowFixMe,
    {
      type: 'InterfaceDeclaration',
      id: asDetachedNodeForCodeGen(props.id),
      typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
      extends: props.extends.map(n => asDetachedNodeForCodeGen(n)),
      body: asDetachedNodeForCodeGen(props.body),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function InterfaceExtends(props: {
  ...InterfaceExtendsProps,
  readonly parent?: ESNode,
}): DetachedNode<InterfaceExtendsType> {
  const node = detachedProps<InterfaceExtendsType>(props.parent as $FlowFixMe, {
    type: 'InterfaceExtends',
    id: asDetachedNodeForCodeGen(props.id),
    typeArguments: asDetachedNodeForCodeGen(props.typeArguments),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function InterfaceTypeAnnotation(props: {
  ...InterfaceTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<InterfaceTypeAnnotationType> {
  const node = detachedProps<InterfaceTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'InterfaceTypeAnnotation',
      extends: props.extends.map(n => asDetachedNodeForCodeGen(n)),
      body: asDetachedNodeForCodeGen(props.body),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function IntersectionTypeAnnotation(props: {
  ...IntersectionTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<IntersectionTypeAnnotationType> {
  const node = detachedProps<IntersectionTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'IntersectionTypeAnnotation',
      types: props.types.map(n => asDetachedNodeForCodeGen(n)),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function JSXAttribute(props: {
  ...JSXAttributeProps,
  readonly parent?: ESNode,
}): DetachedNode<JSXAttributeType> {
  const node = detachedProps<JSXAttributeType>(props.parent as $FlowFixMe, {
    type: 'JSXAttribute',
    name: asDetachedNodeForCodeGen(props.name),
    value: asDetachedNodeForCodeGen(props.value),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function JSXClosingElement(props: {
  ...JSXClosingElementProps,
  readonly parent?: ESNode,
}): DetachedNode<JSXClosingElementType> {
  const node = detachedProps<JSXClosingElementType>(
    props.parent as $FlowFixMe,
    {
      type: 'JSXClosingElement',
      name: asDetachedNodeForCodeGen(props.name),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function JSXClosingFragment(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<JSXClosingFragmentType> {
  return detachedProps<JSXClosingFragmentType>(props.parent as $FlowFixMe, {
    type: 'JSXClosingFragment',
  });
}

export function JSXElement(props: {
  ...JSXElementProps,
  readonly parent?: ESNode,
}): DetachedNode<JSXElementType> {
  const node = detachedProps<JSXElementType>(props.parent as $FlowFixMe, {
    type: 'JSXElement',
    openingElement: asDetachedNodeForCodeGen(props.openingElement),
    children: props.children.map(n => asDetachedNodeForCodeGen(n)),
    closingElement: asDetachedNodeForCodeGen(props.closingElement),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function JSXEmptyExpression(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<JSXEmptyExpressionType> {
  return detachedProps<JSXEmptyExpressionType>(props.parent as $FlowFixMe, {
    type: 'JSXEmptyExpression',
  });
}

export function JSXExpressionContainer(props: {
  ...JSXExpressionContainerProps,
  readonly parent?: ESNode,
}): DetachedNode<JSXExpressionContainerType> {
  const node = detachedProps<JSXExpressionContainerType>(
    props.parent as $FlowFixMe,
    {
      type: 'JSXExpressionContainer',
      expression: asDetachedNodeForCodeGen(props.expression),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function JSXFragment(props: {
  ...JSXFragmentProps,
  readonly parent?: ESNode,
}): DetachedNode<JSXFragmentType> {
  const node = detachedProps<JSXFragmentType>(props.parent as $FlowFixMe, {
    type: 'JSXFragment',
    openingFragment: asDetachedNodeForCodeGen(props.openingFragment),
    children: props.children.map(n => asDetachedNodeForCodeGen(n)),
    closingFragment: asDetachedNodeForCodeGen(props.closingFragment),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function JSXIdentifier(props: {
  ...JSXIdentifierProps,
  readonly parent?: ESNode,
}): DetachedNode<JSXIdentifierType> {
  const node = detachedProps<JSXIdentifierType>(props.parent as $FlowFixMe, {
    type: 'JSXIdentifier',
    name: props.name,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function JSXMemberExpression(props: {
  ...JSXMemberExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<JSXMemberExpressionType> {
  const node = detachedProps<JSXMemberExpressionType>(
    props.parent as $FlowFixMe,
    {
      type: 'JSXMemberExpression',
      object: asDetachedNodeForCodeGen(props.object),
      property: asDetachedNodeForCodeGen(props.property),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function JSXNamespacedName(props: {
  ...JSXNamespacedNameProps,
  readonly parent?: ESNode,
}): DetachedNode<JSXNamespacedNameType> {
  const node = detachedProps<JSXNamespacedNameType>(
    props.parent as $FlowFixMe,
    {
      type: 'JSXNamespacedName',
      namespace: asDetachedNodeForCodeGen(props.namespace),
      name: asDetachedNodeForCodeGen(props.name),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function JSXOpeningElement(props: {
  ...JSXOpeningElementProps,
  readonly parent?: ESNode,
}): DetachedNode<JSXOpeningElementType> {
  const node = detachedProps<JSXOpeningElementType>(
    props.parent as $FlowFixMe,
    {
      type: 'JSXOpeningElement',
      name: asDetachedNodeForCodeGen(props.name),
      attributes: props.attributes.map(n => asDetachedNodeForCodeGen(n)),
      selfClosing: props.selfClosing,
      typeArguments: asDetachedNodeForCodeGen(props.typeArguments),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function JSXOpeningFragment(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<JSXOpeningFragmentType> {
  return detachedProps<JSXOpeningFragmentType>(props.parent as $FlowFixMe, {
    type: 'JSXOpeningFragment',
  });
}

export function JSXSpreadAttribute(props: {
  ...JSXSpreadAttributeProps,
  readonly parent?: ESNode,
}): DetachedNode<JSXSpreadAttributeType> {
  const node = detachedProps<JSXSpreadAttributeType>(
    props.parent as $FlowFixMe,
    {
      type: 'JSXSpreadAttribute',
      argument: asDetachedNodeForCodeGen(props.argument),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function JSXSpreadChild(props: {
  ...JSXSpreadChildProps,
  readonly parent?: ESNode,
}): DetachedNode<JSXSpreadChildType> {
  const node = detachedProps<JSXSpreadChildType>(props.parent as $FlowFixMe, {
    type: 'JSXSpreadChild',
    expression: asDetachedNodeForCodeGen(props.expression),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function JSXText(props: {
  ...JSXTextProps,
  readonly parent?: ESNode,
}): DetachedNode<JSXTextType> {
  const node = detachedProps<JSXTextType>(props.parent as $FlowFixMe, {
    type: 'JSXText',
    value: props.value,
    raw: props.raw,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function KeyofTypeAnnotation(props: {
  ...KeyofTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<KeyofTypeAnnotationType> {
  const node = detachedProps<KeyofTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'KeyofTypeAnnotation',
      argument: asDetachedNodeForCodeGen(props.argument),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function LabeledStatement(props: {
  ...LabeledStatementProps,
  readonly parent?: ESNode,
}): DetachedNode<LabeledStatementType> {
  const node = detachedProps<LabeledStatementType>(props.parent as $FlowFixMe, {
    type: 'LabeledStatement',
    label: asDetachedNodeForCodeGen(props.label),
    body: asDetachedNodeForCodeGen(props.body),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function LogicalExpression(props: {
  ...LogicalExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<LogicalExpressionType> {
  const node = detachedProps<LogicalExpressionType>(
    props.parent as $FlowFixMe,
    {
      type: 'LogicalExpression',
      left: asDetachedNodeForCodeGen(props.left),
      right: asDetachedNodeForCodeGen(props.right),
      operator: props.operator,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchArrayPattern(props: {
  ...MatchArrayPatternProps,
  readonly parent?: ESNode,
}): DetachedNode<MatchArrayPatternType> {
  const node = detachedProps<MatchArrayPatternType>(
    props.parent as $FlowFixMe,
    {
      type: 'MatchArrayPattern',
      elements: props.elements.map(n => asDetachedNodeForCodeGen(n)),
      rest: asDetachedNodeForCodeGen(props.rest),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchAsPattern(props: {
  ...MatchAsPatternProps,
  readonly parent?: ESNode,
}): DetachedNode<MatchAsPatternType> {
  const node = detachedProps<MatchAsPatternType>(props.parent as $FlowFixMe, {
    type: 'MatchAsPattern',
    pattern: asDetachedNodeForCodeGen(props.pattern),
    target: asDetachedNodeForCodeGen(props.target),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchBindingPattern(props: {
  ...MatchBindingPatternProps,
  readonly parent?: ESNode,
}): DetachedNode<MatchBindingPatternType> {
  const node = detachedProps<MatchBindingPatternType>(
    props.parent as $FlowFixMe,
    {
      type: 'MatchBindingPattern',
      id: asDetachedNodeForCodeGen(props.id),
      kind: props.kind,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchExpression(props: {
  ...MatchExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<MatchExpressionType> {
  const node = detachedProps<MatchExpressionType>(props.parent as $FlowFixMe, {
    type: 'MatchExpression',
    argument: asDetachedNodeForCodeGen(props.argument),
    cases: props.cases.map(n => asDetachedNodeForCodeGen(n)),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchExpressionCase(props: {
  ...MatchExpressionCaseProps,
  readonly parent?: ESNode,
}): DetachedNode<MatchExpressionCaseType> {
  const node = detachedProps<MatchExpressionCaseType>(
    props.parent as $FlowFixMe,
    {
      type: 'MatchExpressionCase',
      pattern: asDetachedNodeForCodeGen(props.pattern),
      body: asDetachedNodeForCodeGen(props.body),
      guard: asDetachedNodeForCodeGen(props.guard),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchIdentifierPattern(props: {
  ...MatchIdentifierPatternProps,
  readonly parent?: ESNode,
}): DetachedNode<MatchIdentifierPatternType> {
  const node = detachedProps<MatchIdentifierPatternType>(
    props.parent as $FlowFixMe,
    {
      type: 'MatchIdentifierPattern',
      id: asDetachedNodeForCodeGen(props.id),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchInstanceObjectPattern(props: {
  ...MatchInstanceObjectPatternProps,
  readonly parent?: ESNode,
}): DetachedNode<MatchInstanceObjectPatternType> {
  const node = detachedProps<MatchInstanceObjectPatternType>(
    props.parent as $FlowFixMe,
    {
      type: 'MatchInstanceObjectPattern',
      properties: props.properties.map(n => asDetachedNodeForCodeGen(n)),
      rest: asDetachedNodeForCodeGen(props.rest),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchInstancePattern(props: {
  ...MatchInstancePatternProps,
  readonly parent?: ESNode,
}): DetachedNode<MatchInstancePatternType> {
  const node = detachedProps<MatchInstancePatternType>(
    props.parent as $FlowFixMe,
    {
      type: 'MatchInstancePattern',
      targetConstructor: asDetachedNodeForCodeGen(props.targetConstructor),
      properties: asDetachedNodeForCodeGen(props.properties),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchLiteralPattern(props: {
  ...MatchLiteralPatternProps,
  readonly parent?: ESNode,
}): DetachedNode<MatchLiteralPatternType> {
  const node = detachedProps<MatchLiteralPatternType>(
    props.parent as $FlowFixMe,
    {
      type: 'MatchLiteralPattern',
      literal: asDetachedNodeForCodeGen(props.literal),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchMemberPattern(props: {
  ...MatchMemberPatternProps,
  readonly parent?: ESNode,
}): DetachedNode<MatchMemberPatternType> {
  const node = detachedProps<MatchMemberPatternType>(
    props.parent as $FlowFixMe,
    {
      type: 'MatchMemberPattern',
      base: asDetachedNodeForCodeGen(props.base),
      property: asDetachedNodeForCodeGen(props.property),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchObjectPattern(props: {
  ...MatchObjectPatternProps,
  readonly parent?: ESNode,
}): DetachedNode<MatchObjectPatternType> {
  const node = detachedProps<MatchObjectPatternType>(
    props.parent as $FlowFixMe,
    {
      type: 'MatchObjectPattern',
      properties: props.properties.map(n => asDetachedNodeForCodeGen(n)),
      rest: asDetachedNodeForCodeGen(props.rest),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchObjectPatternProperty(props: {
  ...MatchObjectPatternPropertyProps,
  readonly parent?: ESNode,
}): DetachedNode<MatchObjectPatternPropertyType> {
  const node = detachedProps<MatchObjectPatternPropertyType>(
    props.parent as $FlowFixMe,
    {
      type: 'MatchObjectPatternProperty',
      key: asDetachedNodeForCodeGen(props.key),
      pattern: asDetachedNodeForCodeGen(props.pattern),
      shorthand: props.shorthand,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchOrPattern(props: {
  ...MatchOrPatternProps,
  readonly parent?: ESNode,
}): DetachedNode<MatchOrPatternType> {
  const node = detachedProps<MatchOrPatternType>(props.parent as $FlowFixMe, {
    type: 'MatchOrPattern',
    patterns: props.patterns.map(n => asDetachedNodeForCodeGen(n)),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchRestPattern(props: {
  ...MatchRestPatternProps,
  readonly parent?: ESNode,
}): DetachedNode<MatchRestPatternType> {
  const node = detachedProps<MatchRestPatternType>(props.parent as $FlowFixMe, {
    type: 'MatchRestPattern',
    argument: asDetachedNodeForCodeGen(props.argument),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchStatement(props: {
  ...MatchStatementProps,
  readonly parent?: ESNode,
}): DetachedNode<MatchStatementType> {
  const node = detachedProps<MatchStatementType>(props.parent as $FlowFixMe, {
    type: 'MatchStatement',
    argument: asDetachedNodeForCodeGen(props.argument),
    cases: props.cases.map(n => asDetachedNodeForCodeGen(n)),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchStatementCase(props: {
  ...MatchStatementCaseProps,
  readonly parent?: ESNode,
}): DetachedNode<MatchStatementCaseType> {
  const node = detachedProps<MatchStatementCaseType>(
    props.parent as $FlowFixMe,
    {
      type: 'MatchStatementCase',
      pattern: asDetachedNodeForCodeGen(props.pattern),
      body: asDetachedNodeForCodeGen(props.body),
      guard: asDetachedNodeForCodeGen(props.guard),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchUnaryPattern(props: {
  ...MatchUnaryPatternProps,
  readonly parent?: ESNode,
}): DetachedNode<MatchUnaryPatternType> {
  const node = detachedProps<MatchUnaryPatternType>(
    props.parent as $FlowFixMe,
    {
      type: 'MatchUnaryPattern',
      argument: asDetachedNodeForCodeGen(props.argument),
      operator: props.operator,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MatchWildcardPattern(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<MatchWildcardPatternType> {
  return detachedProps<MatchWildcardPatternType>(props.parent as $FlowFixMe, {
    type: 'MatchWildcardPattern',
  });
}

export function MetaProperty(props: {
  ...MetaPropertyProps,
  readonly parent?: ESNode,
}): DetachedNode<MetaPropertyType> {
  const node = detachedProps<MetaPropertyType>(props.parent as $FlowFixMe, {
    type: 'MetaProperty',
    meta: asDetachedNodeForCodeGen(props.meta),
    property: asDetachedNodeForCodeGen(props.property),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MethodDefinition(props: {
  ...MethodDefinitionProps,
  readonly parent?: ESNode,
}): DetachedNode<MethodDefinitionType> {
  const node = detachedProps<MethodDefinitionType>(props.parent as $FlowFixMe, {
    type: 'MethodDefinition',
    key: asDetachedNodeForCodeGen(props.key),
    value: asDetachedNodeForCodeGen(props.value),
    kind: props.kind,
    computed: props.computed,
    static: props.static,
    decorators: props.decorators.map(n => asDetachedNodeForCodeGen(n)),
    override: props.override,
    tsAccessibility: props.tsAccessibility,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function MixedTypeAnnotation(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<MixedTypeAnnotationType> {
  return detachedProps<MixedTypeAnnotationType>(props.parent as $FlowFixMe, {
    type: 'MixedTypeAnnotation',
  });
}

export function NamespaceExportDeclaration(props: {
  ...NamespaceExportDeclarationProps,
  readonly parent?: ESNode,
}): DetachedNode<NamespaceExportDeclarationType> {
  const node = detachedProps<NamespaceExportDeclarationType>(
    props.parent as $FlowFixMe,
    {
      type: 'NamespaceExportDeclaration',
      id: asDetachedNodeForCodeGen(props.id),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function NeverTypeAnnotation(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<NeverTypeAnnotationType> {
  return detachedProps<NeverTypeAnnotationType>(props.parent as $FlowFixMe, {
    type: 'NeverTypeAnnotation',
  });
}

export function NewExpression(props: {
  ...NewExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<NewExpressionType> {
  const node = detachedProps<NewExpressionType>(props.parent as $FlowFixMe, {
    type: 'NewExpression',
    callee: asDetachedNodeForCodeGen(props.callee),
    typeArguments: asDetachedNodeForCodeGen(props.typeArguments),
    arguments: props.arguments.map(n => asDetachedNodeForCodeGen(n)),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function NullableTypeAnnotation(props: {
  ...NullableTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<NullableTypeAnnotationType> {
  const node = detachedProps<NullableTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'NullableTypeAnnotation',
      typeAnnotation: asDetachedNodeForCodeGen(props.typeAnnotation),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function NullLiteralTypeAnnotation(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<NullLiteralTypeAnnotationType> {
  return detachedProps<NullLiteralTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'NullLiteralTypeAnnotation',
    },
  );
}

export function NumberLiteralTypeAnnotation(props: {
  ...NumberLiteralTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<NumberLiteralTypeAnnotationType> {
  const node = detachedProps<NumberLiteralTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'NumberLiteralTypeAnnotation',
      value: props.value,
      raw: props.raw,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function NumberTypeAnnotation(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<NumberTypeAnnotationType> {
  return detachedProps<NumberTypeAnnotationType>(props.parent as $FlowFixMe, {
    type: 'NumberTypeAnnotation',
  });
}

export function ObjectExpression(props: {
  ...ObjectExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<ObjectExpressionType> {
  const node = detachedProps<ObjectExpressionType>(props.parent as $FlowFixMe, {
    type: 'ObjectExpression',
    properties: props.properties.map(n => asDetachedNodeForCodeGen(n)),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ObjectPattern(props: {
  ...ObjectPatternProps,
  readonly parent?: ESNode,
}): DetachedNode<ObjectPatternType> {
  const node = detachedProps<ObjectPatternType>(props.parent as $FlowFixMe, {
    type: 'ObjectPattern',
    properties: props.properties.map(n => asDetachedNodeForCodeGen(n)),
    typeAnnotation: asDetachedNodeForCodeGen(props.typeAnnotation),
    optional: props.optional,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ObjectTypeAnnotation(props: {
  ...ObjectTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<ObjectTypeAnnotationType> {
  const node = detachedProps<ObjectTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'ObjectTypeAnnotation',
      properties: props.properties.map(n => asDetachedNodeForCodeGen(n)),
      indexers: props.indexers.map(n => asDetachedNodeForCodeGen(n)),
      callProperties: props.callProperties.map(n =>
        asDetachedNodeForCodeGen(n),
      ),
      internalSlots: props.internalSlots.map(n => asDetachedNodeForCodeGen(n)),
      inexact: props.inexact,
      exact: props.exact,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ObjectTypeCallProperty(props: {
  ...ObjectTypeCallPropertyProps,
  readonly parent?: ESNode,
}): DetachedNode<ObjectTypeCallPropertyType> {
  const node = detachedProps<ObjectTypeCallPropertyType>(
    props.parent as $FlowFixMe,
    {
      type: 'ObjectTypeCallProperty',
      value: asDetachedNodeForCodeGen(props.value),
      static: props.static,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ObjectTypeIndexer(props: {
  ...ObjectTypeIndexerProps,
  readonly parent?: ESNode,
}): DetachedNode<ObjectTypeIndexerType> {
  const node = detachedProps<ObjectTypeIndexerType>(
    props.parent as $FlowFixMe,
    {
      type: 'ObjectTypeIndexer',
      id: asDetachedNodeForCodeGen(props.id),
      key: asDetachedNodeForCodeGen(props.key),
      value: asDetachedNodeForCodeGen(props.value),
      static: props.static,
      variance: asDetachedNodeForCodeGen(props.variance),
      optional: props.optional,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ObjectTypeInternalSlot(props: {
  ...ObjectTypeInternalSlotProps,
  readonly parent?: ESNode,
}): DetachedNode<ObjectTypeInternalSlotType> {
  const node = detachedProps<ObjectTypeInternalSlotType>(
    props.parent as $FlowFixMe,
    {
      type: 'ObjectTypeInternalSlot',
      id: asDetachedNodeForCodeGen(props.id),
      value: asDetachedNodeForCodeGen(props.value),
      optional: props.optional,
      static: props.static,
      method: props.method,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ObjectTypeMappedTypeProperty(props: {
  ...ObjectTypeMappedTypePropertyProps,
  readonly parent?: ESNode,
}): DetachedNode<ObjectTypeMappedTypePropertyType> {
  const node = detachedProps<ObjectTypeMappedTypePropertyType>(
    props.parent as $FlowFixMe,
    {
      type: 'ObjectTypeMappedTypeProperty',
      keyTparam: asDetachedNodeForCodeGen(props.keyTparam),
      propType: asDetachedNodeForCodeGen(props.propType),
      sourceType: asDetachedNodeForCodeGen(props.sourceType),
      nameType: asDetachedNodeForCodeGen(props.nameType),
      variance: asDetachedNodeForCodeGen(props.variance),
      varianceOp: props.varianceOp,
      optional: props.optional,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ObjectTypePrivateField(props: {
  ...ObjectTypePrivateFieldProps,
  readonly parent?: ESNode,
}): DetachedNode<ObjectTypePrivateFieldType> {
  const node = detachedProps<ObjectTypePrivateFieldType>(
    props.parent as $FlowFixMe,
    {
      type: 'ObjectTypePrivateField',
      key: asDetachedNodeForCodeGen(props.key),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ObjectTypeSpreadProperty(props: {
  ...ObjectTypeSpreadPropertyProps,
  readonly parent?: ESNode,
}): DetachedNode<ObjectTypeSpreadPropertyType> {
  const node = detachedProps<ObjectTypeSpreadPropertyType>(
    props.parent as $FlowFixMe,
    {
      type: 'ObjectTypeSpreadProperty',
      argument: asDetachedNodeForCodeGen(props.argument),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function OpaqueType(props: {
  ...OpaqueTypeProps,
  readonly parent?: ESNode,
}): DetachedNode<OpaqueTypeType> {
  const node = detachedProps<OpaqueTypeType>(props.parent as $FlowFixMe, {
    type: 'OpaqueType',
    id: asDetachedNodeForCodeGen(props.id),
    typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
    impltype: asDetachedNodeForCodeGen(props.impltype),
    lowerBound: asDetachedNodeForCodeGen(props.lowerBound),
    upperBound: asDetachedNodeForCodeGen(props.upperBound),
    supertype: asDetachedNodeForCodeGen(props.supertype),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function OptionalIndexedAccessType(props: {
  ...OptionalIndexedAccessTypeProps,
  readonly parent?: ESNode,
}): DetachedNode<OptionalIndexedAccessTypeType> {
  const node = detachedProps<OptionalIndexedAccessTypeType>(
    props.parent as $FlowFixMe,
    {
      type: 'OptionalIndexedAccessType',
      objectType: asDetachedNodeForCodeGen(props.objectType),
      indexType: asDetachedNodeForCodeGen(props.indexType),
      optional: props.optional,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function PrivateIdentifier(props: {
  ...PrivateIdentifierProps,
  readonly parent?: ESNode,
}): DetachedNode<PrivateIdentifierType> {
  const node = detachedProps<PrivateIdentifierType>(
    props.parent as $FlowFixMe,
    {
      type: 'PrivateIdentifier',
      name: props.name,
      typeAnnotation: null,
      optional: false,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function Property(props: {
  ...PropertyProps,
  readonly parent?: ESNode,
}): DetachedNode<PropertyType> {
  const node = detachedProps<PropertyType>(props.parent as $FlowFixMe, {
    type: 'Property',
    key: asDetachedNodeForCodeGen(props.key),
    value: asDetachedNodeForCodeGen(props.value),
    kind: props.kind,
    computed: props.computed,
    method: props.method,
    shorthand: props.shorthand,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function PropertyDefinition(props: {
  ...PropertyDefinitionProps,
  readonly parent?: ESNode,
}): DetachedNode<PropertyDefinitionType> {
  const node = detachedProps<PropertyDefinitionType>(
    props.parent as $FlowFixMe,
    {
      type: 'PropertyDefinition',
      key: asDetachedNodeForCodeGen(props.key),
      value: asDetachedNodeForCodeGen(props.value),
      computed: props.computed,
      static: props.static,
      decorators: props.decorators.map(n => asDetachedNodeForCodeGen(n)),
      declare: props.declare,
      optional: props.optional,
      variance: asDetachedNodeForCodeGen(props.variance),
      typeAnnotation: asDetachedNodeForCodeGen(props.typeAnnotation),
      override: props.override,
      tsAccessibility: props.tsAccessibility,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function QualifiedTypeIdentifier(props: {
  ...QualifiedTypeIdentifierProps,
  readonly parent?: ESNode,
}): DetachedNode<QualifiedTypeIdentifierType> {
  const node = detachedProps<QualifiedTypeIdentifierType>(
    props.parent as $FlowFixMe,
    {
      type: 'QualifiedTypeIdentifier',
      qualification: asDetachedNodeForCodeGen(props.qualification),
      id: asDetachedNodeForCodeGen(props.id),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function QualifiedTypeofIdentifier(props: {
  ...QualifiedTypeofIdentifierProps,
  readonly parent?: ESNode,
}): DetachedNode<QualifiedTypeofIdentifierType> {
  const node = detachedProps<QualifiedTypeofIdentifierType>(
    props.parent as $FlowFixMe,
    {
      type: 'QualifiedTypeofIdentifier',
      qualification: asDetachedNodeForCodeGen(props.qualification),
      id: asDetachedNodeForCodeGen(props.id),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function RecordDeclaration(props: {
  ...RecordDeclarationProps,
  readonly parent?: ESNode,
}): DetachedNode<RecordDeclarationType> {
  const node = detachedProps<RecordDeclarationType>(
    props.parent as $FlowFixMe,
    {
      type: 'RecordDeclaration',
      id: asDetachedNodeForCodeGen(props.id),
      typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
      implements: props.implements.map(n => asDetachedNodeForCodeGen(n)),
      body: asDetachedNodeForCodeGen(props.body),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function RecordDeclarationBody(props: {
  ...RecordDeclarationBodyProps,
  readonly parent?: ESNode,
}): DetachedNode<RecordDeclarationBodyType> {
  const node = detachedProps<RecordDeclarationBodyType>(
    props.parent as $FlowFixMe,
    {
      type: 'RecordDeclarationBody',
      elements: props.elements.map(n => asDetachedNodeForCodeGen(n)),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function RecordDeclarationImplements(props: {
  ...RecordDeclarationImplementsProps,
  readonly parent?: ESNode,
}): DetachedNode<RecordDeclarationImplementsType> {
  const node = detachedProps<RecordDeclarationImplementsType>(
    props.parent as $FlowFixMe,
    {
      type: 'RecordDeclarationImplements',
      id: asDetachedNodeForCodeGen(props.id),
      typeArguments: asDetachedNodeForCodeGen(props.typeArguments),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function RecordDeclarationProperty(props: {
  ...RecordDeclarationPropertyProps,
  readonly parent?: ESNode,
}): DetachedNode<RecordDeclarationPropertyType> {
  const node = detachedProps<RecordDeclarationPropertyType>(
    props.parent as $FlowFixMe,
    {
      type: 'RecordDeclarationProperty',
      key: asDetachedNodeForCodeGen(props.key),
      typeAnnotation: asDetachedNodeForCodeGen(props.typeAnnotation),
      defaultValue: asDetachedNodeForCodeGen(props.defaultValue),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function RecordDeclarationStaticProperty(props: {
  ...RecordDeclarationStaticPropertyProps,
  readonly parent?: ESNode,
}): DetachedNode<RecordDeclarationStaticPropertyType> {
  const node = detachedProps<RecordDeclarationStaticPropertyType>(
    props.parent as $FlowFixMe,
    {
      type: 'RecordDeclarationStaticProperty',
      key: asDetachedNodeForCodeGen(props.key),
      typeAnnotation: asDetachedNodeForCodeGen(props.typeAnnotation),
      value: asDetachedNodeForCodeGen(props.value),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function RecordExpression(props: {
  ...RecordExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<RecordExpressionType> {
  const node = detachedProps<RecordExpressionType>(props.parent as $FlowFixMe, {
    type: 'RecordExpression',
    recordConstructor: asDetachedNodeForCodeGen(props.recordConstructor),
    typeArguments: asDetachedNodeForCodeGen(props.typeArguments),
    properties: asDetachedNodeForCodeGen(props.properties),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function RecordExpressionProperties(props: {
  ...RecordExpressionPropertiesProps,
  readonly parent?: ESNode,
}): DetachedNode<RecordExpressionPropertiesType> {
  const node = detachedProps<RecordExpressionPropertiesType>(
    props.parent as $FlowFixMe,
    {
      type: 'RecordExpressionProperties',
      properties: props.properties.map(n => asDetachedNodeForCodeGen(n)),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function RestElement(props: {
  ...RestElementProps,
  readonly parent?: ESNode,
}): DetachedNode<RestElementType> {
  const node = detachedProps<RestElementType>(props.parent as $FlowFixMe, {
    type: 'RestElement',
    argument: asDetachedNodeForCodeGen(props.argument),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ReturnStatement(props: {
  ...ReturnStatementProps,
  readonly parent?: ESNode,
}): DetachedNode<ReturnStatementType> {
  const node = detachedProps<ReturnStatementType>(props.parent as $FlowFixMe, {
    type: 'ReturnStatement',
    argument: asDetachedNodeForCodeGen(props.argument),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function SatisfiesExpression(props: {
  ...SatisfiesExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<SatisfiesExpressionType> {
  const node = detachedProps<SatisfiesExpressionType>(
    props.parent as $FlowFixMe,
    {
      type: 'SatisfiesExpression',
      expression: asDetachedNodeForCodeGen(props.expression),
      typeAnnotation: asDetachedNodeForCodeGen(props.typeAnnotation),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function SequenceExpression(props: {
  ...SequenceExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<SequenceExpressionType> {
  const node = detachedProps<SequenceExpressionType>(
    props.parent as $FlowFixMe,
    {
      type: 'SequenceExpression',
      expressions: props.expressions.map(n => asDetachedNodeForCodeGen(n)),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function SpreadElement(props: {
  ...SpreadElementProps,
  readonly parent?: ESNode,
}): DetachedNode<SpreadElementType> {
  const node = detachedProps<SpreadElementType>(props.parent as $FlowFixMe, {
    type: 'SpreadElement',
    argument: asDetachedNodeForCodeGen(props.argument),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function StaticBlock(props: {
  ...StaticBlockProps,
  readonly parent?: ESNode,
}): DetachedNode<StaticBlockType> {
  const node = detachedProps<StaticBlockType>(props.parent as $FlowFixMe, {
    type: 'StaticBlock',
    body: props.body.map(n => asDetachedNodeForCodeGen(n)),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function StringLiteralTypeAnnotation(props: {
  ...StringLiteralTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<StringLiteralTypeAnnotationType> {
  const node = detachedProps<StringLiteralTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'StringLiteralTypeAnnotation',
      value: props.value,
      raw: props.raw,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function StringTypeAnnotation(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<StringTypeAnnotationType> {
  return detachedProps<StringTypeAnnotationType>(props.parent as $FlowFixMe, {
    type: 'StringTypeAnnotation',
  });
}

export function Super(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<SuperType> {
  return detachedProps<SuperType>(props.parent as $FlowFixMe, {
    type: 'Super',
  });
}

export function SwitchCase(props: {
  ...SwitchCaseProps,
  readonly parent?: ESNode,
}): DetachedNode<SwitchCaseType> {
  const node = detachedProps<SwitchCaseType>(props.parent as $FlowFixMe, {
    type: 'SwitchCase',
    test: asDetachedNodeForCodeGen(props.test),
    consequent: props.consequent.map(n => asDetachedNodeForCodeGen(n)),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function SwitchStatement(props: {
  ...SwitchStatementProps,
  readonly parent?: ESNode,
}): DetachedNode<SwitchStatementType> {
  const node = detachedProps<SwitchStatementType>(props.parent as $FlowFixMe, {
    type: 'SwitchStatement',
    discriminant: asDetachedNodeForCodeGen(props.discriminant),
    cases: props.cases.map(n => asDetachedNodeForCodeGen(n)),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function SymbolTypeAnnotation(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<SymbolTypeAnnotationType> {
  return detachedProps<SymbolTypeAnnotationType>(props.parent as $FlowFixMe, {
    type: 'SymbolTypeAnnotation',
  });
}

export function TaggedTemplateExpression(props: {
  ...TaggedTemplateExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<TaggedTemplateExpressionType> {
  const node = detachedProps<TaggedTemplateExpressionType>(
    props.parent as $FlowFixMe,
    {
      type: 'TaggedTemplateExpression',
      tag: asDetachedNodeForCodeGen(props.tag),
      quasi: asDetachedNodeForCodeGen(props.quasi),
      typeArguments: asDetachedNodeForCodeGen(props.typeArguments),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function TemplateLiteral(props: {
  ...TemplateLiteralProps,
  readonly parent?: ESNode,
}): DetachedNode<TemplateLiteralType> {
  const node = detachedProps<TemplateLiteralType>(props.parent as $FlowFixMe, {
    type: 'TemplateLiteral',
    quasis: props.quasis.map(n => asDetachedNodeForCodeGen(n)),
    expressions: props.expressions.map(n => asDetachedNodeForCodeGen(n)),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function TemplateLiteralTypeAnnotation(props: {
  ...TemplateLiteralTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<TemplateLiteralTypeAnnotationType> {
  const node = detachedProps<TemplateLiteralTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'TemplateLiteralTypeAnnotation',
      quasis: props.quasis.map(n => asDetachedNodeForCodeGen(n)),
      types: props.types.map(n => asDetachedNodeForCodeGen(n)),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function ThisExpression(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<ThisExpressionType> {
  return detachedProps<ThisExpressionType>(props.parent as $FlowFixMe, {
    type: 'ThisExpression',
  });
}

export function ThisTypeAnnotation(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<ThisTypeAnnotationType> {
  return detachedProps<ThisTypeAnnotationType>(props.parent as $FlowFixMe, {
    type: 'ThisTypeAnnotation',
  });
}

export function ThrowStatement(props: {
  ...ThrowStatementProps,
  readonly parent?: ESNode,
}): DetachedNode<ThrowStatementType> {
  const node = detachedProps<ThrowStatementType>(props.parent as $FlowFixMe, {
    type: 'ThrowStatement',
    argument: asDetachedNodeForCodeGen(props.argument),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function TryStatement(props: {
  ...TryStatementProps,
  readonly parent?: ESNode,
}): DetachedNode<TryStatementType> {
  const node = detachedProps<TryStatementType>(props.parent as $FlowFixMe, {
    type: 'TryStatement',
    block: asDetachedNodeForCodeGen(props.block),
    handler: asDetachedNodeForCodeGen(props.handler),
    finalizer: asDetachedNodeForCodeGen(props.finalizer),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function TupleTypeAnnotation(props: {
  ...TupleTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<TupleTypeAnnotationType> {
  const node = detachedProps<TupleTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'TupleTypeAnnotation',
      elementTypes: props.elementTypes.map(n => asDetachedNodeForCodeGen(n)),
      inexact: props.inexact,
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function TupleTypeElement(props: {
  ...TupleTypeElementProps,
  readonly parent?: ESNode,
}): DetachedNode<TupleTypeElementType> {
  const node = detachedProps<TupleTypeElementType>(props.parent as $FlowFixMe, {
    type: 'TupleTypeElement',
    elementType: asDetachedNodeForCodeGen(props.elementType),
    optional: props.optional,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function TupleTypeLabeledElement(props: {
  ...TupleTypeLabeledElementProps,
  readonly parent?: ESNode,
}): DetachedNode<TupleTypeLabeledElementType> {
  const node = detachedProps<TupleTypeLabeledElementType>(
    props.parent as $FlowFixMe,
    {
      type: 'TupleTypeLabeledElement',
      label: asDetachedNodeForCodeGen(props.label),
      elementType: asDetachedNodeForCodeGen(props.elementType),
      optional: props.optional,
      variance: asDetachedNodeForCodeGen(props.variance),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function TupleTypeSpreadElement(props: {
  ...TupleTypeSpreadElementProps,
  readonly parent?: ESNode,
}): DetachedNode<TupleTypeSpreadElementType> {
  const node = detachedProps<TupleTypeSpreadElementType>(
    props.parent as $FlowFixMe,
    {
      type: 'TupleTypeSpreadElement',
      label: asDetachedNodeForCodeGen(props.label),
      typeAnnotation: asDetachedNodeForCodeGen(props.typeAnnotation),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function TypeAlias(props: {
  ...TypeAliasProps,
  readonly parent?: ESNode,
}): DetachedNode<TypeAliasType> {
  const node = detachedProps<TypeAliasType>(props.parent as $FlowFixMe, {
    type: 'TypeAlias',
    id: asDetachedNodeForCodeGen(props.id),
    typeParameters: asDetachedNodeForCodeGen(props.typeParameters),
    right: asDetachedNodeForCodeGen(props.right),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function TypeAnnotation(props: {
  ...TypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<TypeAnnotationType> {
  const node = detachedProps<TypeAnnotationType>(props.parent as $FlowFixMe, {
    type: 'TypeAnnotation',
    typeAnnotation: asDetachedNodeForCodeGen(props.typeAnnotation),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function TypeCastExpression(props: {
  ...TypeCastExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<TypeCastExpressionType> {
  const node = detachedProps<TypeCastExpressionType>(
    props.parent as $FlowFixMe,
    {
      type: 'TypeCastExpression',
      expression: asDetachedNodeForCodeGen(props.expression),
      typeAnnotation: asDetachedNodeForCodeGen(props.typeAnnotation),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function TypeofTypeAnnotation(props: {
  ...TypeofTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<TypeofTypeAnnotationType> {
  const node = detachedProps<TypeofTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'TypeofTypeAnnotation',
      argument: asDetachedNodeForCodeGen(props.argument),
      typeArguments: asDetachedNodeForCodeGen(props.typeArguments),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function TypeOperator(props: {
  ...TypeOperatorProps,
  readonly parent?: ESNode,
}): DetachedNode<TypeOperatorType> {
  const node = detachedProps<TypeOperatorType>(props.parent as $FlowFixMe, {
    type: 'TypeOperator',
    operator: props.operator,
    typeAnnotation: asDetachedNodeForCodeGen(props.typeAnnotation),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function TypeParameter(props: {
  ...TypeParameterProps,
  readonly parent?: ESNode,
}): DetachedNode<TypeParameterType> {
  const node = detachedProps<TypeParameterType>(props.parent as $FlowFixMe, {
    type: 'TypeParameter',
    name: props.name,
    const: props.const,
    bound: asDetachedNodeForCodeGen(props.bound),
    variance: asDetachedNodeForCodeGen(props.variance),
    default: asDetachedNodeForCodeGen(props.default),
    usesExtendsBound: props.usesExtendsBound,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function TypeParameterDeclaration(props: {
  ...TypeParameterDeclarationProps,
  readonly parent?: ESNode,
}): DetachedNode<TypeParameterDeclarationType> {
  const node = detachedProps<TypeParameterDeclarationType>(
    props.parent as $FlowFixMe,
    {
      type: 'TypeParameterDeclaration',
      params: props.params.map(n => asDetachedNodeForCodeGen(n)),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function TypeParameterInstantiation(props: {
  ...TypeParameterInstantiationProps,
  readonly parent?: ESNode,
}): DetachedNode<TypeParameterInstantiationType> {
  const node = detachedProps<TypeParameterInstantiationType>(
    props.parent as $FlowFixMe,
    {
      type: 'TypeParameterInstantiation',
      params: props.params.map(n => asDetachedNodeForCodeGen(n)),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function TypePredicate(props: {
  ...TypePredicateProps,
  readonly parent?: ESNode,
}): DetachedNode<TypePredicateType> {
  const node = detachedProps<TypePredicateType>(props.parent as $FlowFixMe, {
    type: 'TypePredicate',
    parameterName: asDetachedNodeForCodeGen(props.parameterName),
    typeAnnotation: asDetachedNodeForCodeGen(props.typeAnnotation),
    kind: props.kind,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function UnaryExpression(props: {
  ...UnaryExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<UnaryExpressionType> {
  const node = detachedProps<UnaryExpressionType>(props.parent as $FlowFixMe, {
    type: 'UnaryExpression',
    operator: props.operator,
    argument: asDetachedNodeForCodeGen(props.argument),
    prefix: props.prefix,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function UndefinedTypeAnnotation(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<UndefinedTypeAnnotationType> {
  return detachedProps<UndefinedTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'UndefinedTypeAnnotation',
    },
  );
}

export function UnionTypeAnnotation(props: {
  ...UnionTypeAnnotationProps,
  readonly parent?: ESNode,
}): DetachedNode<UnionTypeAnnotationType> {
  const node = detachedProps<UnionTypeAnnotationType>(
    props.parent as $FlowFixMe,
    {
      type: 'UnionTypeAnnotation',
      types: props.types.map(n => asDetachedNodeForCodeGen(n)),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function UnknownTypeAnnotation(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<UnknownTypeAnnotationType> {
  return detachedProps<UnknownTypeAnnotationType>(props.parent as $FlowFixMe, {
    type: 'UnknownTypeAnnotation',
  });
}

export function UpdateExpression(props: {
  ...UpdateExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<UpdateExpressionType> {
  const node = detachedProps<UpdateExpressionType>(props.parent as $FlowFixMe, {
    type: 'UpdateExpression',
    operator: props.operator,
    argument: asDetachedNodeForCodeGen(props.argument),
    prefix: props.prefix,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function VariableDeclaration(props: {
  ...VariableDeclarationProps,
  readonly parent?: ESNode,
}): DetachedNode<VariableDeclarationType> {
  const node = detachedProps<VariableDeclarationType>(
    props.parent as $FlowFixMe,
    {
      type: 'VariableDeclaration',
      kind: props.kind,
      declarations: props.declarations.map(n => asDetachedNodeForCodeGen(n)),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function VariableDeclarator(props: {
  ...VariableDeclaratorProps,
  readonly parent?: ESNode,
}): DetachedNode<VariableDeclaratorType> {
  const node = detachedProps<VariableDeclaratorType>(
    props.parent as $FlowFixMe,
    {
      type: 'VariableDeclarator',
      init: asDetachedNodeForCodeGen(props.init),
      id: asDetachedNodeForCodeGen(props.id),
    },
  );
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function Variance(props: {
  ...VarianceProps,
  readonly parent?: ESNode,
}): DetachedNode<VarianceType> {
  const node = detachedProps<VarianceType>(props.parent as $FlowFixMe, {
    type: 'Variance',
    kind: props.kind,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function VoidTypeAnnotation(
  props: {
    readonly parent?: ESNode,
  } = {...null},
): DetachedNode<VoidTypeAnnotationType> {
  return detachedProps<VoidTypeAnnotationType>(props.parent as $FlowFixMe, {
    type: 'VoidTypeAnnotation',
  });
}

export function WhileStatement(props: {
  ...WhileStatementProps,
  readonly parent?: ESNode,
}): DetachedNode<WhileStatementType> {
  const node = detachedProps<WhileStatementType>(props.parent as $FlowFixMe, {
    type: 'WhileStatement',
    body: asDetachedNodeForCodeGen(props.body),
    test: asDetachedNodeForCodeGen(props.test),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function WithStatement(props: {
  ...WithStatementProps,
  readonly parent?: ESNode,
}): DetachedNode<WithStatementType> {
  const node = detachedProps<WithStatementType>(props.parent as $FlowFixMe, {
    type: 'WithStatement',
    object: asDetachedNodeForCodeGen(props.object),
    body: asDetachedNodeForCodeGen(props.body),
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export function YieldExpression(props: {
  ...YieldExpressionProps,
  readonly parent?: ESNode,
}): DetachedNode<YieldExpressionType> {
  const node = detachedProps<YieldExpressionType>(props.parent as $FlowFixMe, {
    type: 'YieldExpression',
    argument: asDetachedNodeForCodeGen(props.argument),
    delegate: props.delegate,
  });
  setParentPointersInDirectChildren(node as $FlowFixMe);
  return node;
}

export * from './special-case-node-types';
