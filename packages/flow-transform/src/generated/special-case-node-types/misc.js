/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import type {
  ArrowFunctionExpression as ArrowFunctionExpressionType,
  BlockStatement as BlockStatementType,
  ClassDeclaration as ClassDeclarationType,
  DeclareFunction as DeclareFunctionType,
  ESNode,
  FunctionTypeAnnotation as FunctionTypeAnnotationType,
  Identifier as IdentifierType,
  InterpreterDirective as InterpreterDirectiveType,
  Statement as StatementType,
  Token as TokenType,
  Comment as CommentType,
  TemplateElement as TemplateElementType,
  Program as ProgramType,
  DocblockMetadata as DocblockMetadataType,
  MemberExpression as MemberExpressionType,
  TypeofTypeAnnotation as TypeofTypeAnnotationType,
  TypeParameterInstantiation as TypeParameterInstantiationType,
} from 'flow-estree';
import type {DetachedNode, MaybeDetachedNode} from '../../detachedNode';

import {
  asDetachedNode,
  detachedProps,
  setParentPointersInDirectChildren,
} from '../../detachedNode';

/*********************************************************************
 * this file should only contain one-off variant node type functions *
 * if you are creating multiple variants for the same "type", then   *
 * put them in their own file to help keep things organised          *
 *********************************************************************/

// hermes adds an `id` prop which is always null, and it adds an `expression`
// boolean which is true when the body isn't a BlockStatement.
// No need to make consumers set these
export type ArrowFunctionExpressionProps = {
  readonly params: ReadonlyArray<
    MaybeDetachedNode<ArrowFunctionExpressionType['params'][number]>,
  >,
  readonly body: MaybeDetachedNode<ArrowFunctionExpressionType['body']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    ArrowFunctionExpressionType['typeParameters'],
  >,
  readonly returnType?: ?MaybeDetachedNode<
    ArrowFunctionExpressionType['returnType'],
  >,
  readonly predicate?: ?MaybeDetachedNode<
    ArrowFunctionExpressionType['predicate'],
  >,
  readonly async: ArrowFunctionExpressionType['async'],
};
export function ArrowFunctionExpression(props: {
  ...Readonly<ArrowFunctionExpressionProps>,
  readonly parent?: ESNode,
}): DetachedNode<ArrowFunctionExpressionType> {
  const node = detachedProps<ArrowFunctionExpressionType>(props.parent, {
    type: 'ArrowFunctionExpression',
    id: null,
    generator: false,
    // $FlowExpectedError[incompatible-use]
    expression: props.body.type !== 'BlockStatement',
    params: props.params.map(n => asDetachedNode(n)),
    body: asDetachedNode(props.body),
    // $FlowFixMe[incompatible-type]
    typeParameters: asDetachedNode(props.typeParameters),
    // $FlowFixMe[incompatible-type]
    returnType: asDetachedNode(props.returnType),
    // $FlowFixMe[incompatible-type]
    predicate: asDetachedNode(props.predicate),
    async: props.async,
  });
  setParentPointersInDirectChildren(node);
  return node;
}

export type ClassDeclarationProps = {
  readonly id?: ?MaybeDetachedNode<ClassDeclarationType['id']>,
  readonly typeParameters?: ?MaybeDetachedNode<
    ClassDeclarationType['typeParameters'],
  >,
  readonly superClass?: ?MaybeDetachedNode<ClassDeclarationType['superClass']>,
  readonly superTypeArguments?: ?MaybeDetachedNode<
    ClassDeclarationType['superTypeArguments'],
  >,
  // make this optional as it's rarer that people would want to include them
  readonly implements?: ReadonlyArray<
    MaybeDetachedNode<ClassDeclarationType['implements'][number]>,
  >,
  // make this optional as it's rarer that people would want to include them
  readonly decorators?: ReadonlyArray<
    MaybeDetachedNode<ClassDeclarationType['decorators'][number]>,
  >,
  readonly body: MaybeDetachedNode<ClassDeclarationType['body']>,
};
export function ClassDeclaration(props: {
  ...Readonly<ClassDeclarationProps>,
  readonly parent?: ESNode,
}): DetachedNode<ClassDeclarationType> {
  const node = detachedProps<ClassDeclarationType>(props.parent, {
    type: 'ClassDeclaration',
    // $FlowFixMe[incompatible-type]
    id: asDetachedNode(props.id),
    // $FlowFixMe[incompatible-type]
    typeParameters: asDetachedNode(props.typeParameters),
    // $FlowFixMe[incompatible-type]
    superClass: asDetachedNode(props.superClass),
    // $FlowFixMe[incompatible-type]
    superTypeArguments: asDetachedNode(props.superTypeArguments),
    decorators: (props.decorators ?? []).map(n => asDetachedNode(n)),
    implements: (props.implements ?? []).map(n => asDetachedNode(n)),
    body: asDetachedNode(props.body),
  });
  setParentPointersInDirectChildren(node);
  return node;
}

// Missing typeof arguments are omitted, including explicitly null builder inputs.
export type TypeofTypeAnnotationProps = {
  readonly argument: MaybeDetachedNode<TypeofTypeAnnotationType['argument']>,
  readonly typeArguments?: ?MaybeDetachedNode<TypeParameterInstantiationType>,
};
export function TypeofTypeAnnotation(props: {
  ...Readonly<TypeofTypeAnnotationProps>,
  readonly parent?: ESNode,
}): DetachedNode<TypeofTypeAnnotationType> {
  const typeArguments = props.typeArguments;
  const node = detachedProps<TypeofTypeAnnotationType>(props.parent, {
    type: 'TypeofTypeAnnotation',
    argument: asDetachedNode(props.argument),
    ...(typeArguments == null
      ? null
      : {
          typeArguments:
            asDetachedNode<TypeParameterInstantiationType>(typeArguments),
        }),
  });
  setParentPointersInDirectChildren(node);
  return node;
}

// raw/cooked are on a subobject in the estree spec, but are flat on the hermes types
export type TemplateElementProps = {
  readonly tail: TemplateElementType['tail'],
  readonly cooked: TemplateElementType['value']['cooked'],
  readonly raw: TemplateElementType['value']['raw'],
};
export function TemplateElement(props: {
  ...Readonly<TemplateElementProps>,
  readonly parent?: ESNode,
}): DetachedNode<TemplateElementType> {
  return detachedProps<TemplateElementType>(props.parent, {
    type: 'TemplateElement',
    tail: props.tail,
    value: {
      cooked: props.cooked,
      raw: props.raw,
    },
  });
}

// Identifier has a bunch of stuff that usually you don't want to provide - so we have
// this manual def to allow us to default some values
export type IdentifierProps = {
  readonly name: IdentifierType['name'],
  readonly typeAnnotation?: ?MaybeDetachedNode<
    IdentifierType['typeAnnotation'],
  >,
  readonly optional?: IdentifierType['optional'],
};
export function Identifier(props: {
  ...Readonly<IdentifierProps>,
  readonly parent?: ESNode,
}): DetachedNode<IdentifierType> {
  const node = detachedProps<IdentifierType>(props.parent, {
    type: 'Identifier',
    name: props.name,
    optional: props.optional ?? false,
    // $FlowFixMe[incompatible-type]
    typeAnnotation: asDetachedNode(props.typeAnnotation),
  });
  setParentPointersInDirectChildren(node);
  return node;
}

// Program has a bunch of stuff that usually you don't want to provide - so we have
// this manual def to allow us to default some values
export type ProgramProps = {
  readonly sourceType?: ?ProgramType['sourceType'],
  readonly body: ReadonlyArray<MaybeDetachedNode<ProgramType['body'][number]>>,
  readonly tokens?: ?ReadonlyArray<MaybeDetachedNode<TokenType>>,
  readonly comments?: ?ReadonlyArray<MaybeDetachedNode<CommentType>>,
  readonly interpreter?: ?string,
  readonly docblock?: ?DocblockMetadataType,
};
export function Program(props: {
  ...Readonly<ProgramProps>,
}): DetachedNode<ProgramType> {
  return detachedProps<ProgramType>(null, {
    type: 'Program',
    sourceType: props.sourceType ?? 'module',
    body: props.body.map(n => asDetachedNode(n)),
    tokens: props.tokens ?? [],
    comments: props.comments ?? [],
    interpreter:
      props.interpreter != null
        ? // $FlowFixMe[incompatible-type]
          asDetachedNode<InterpreterDirectiveType>({
            type: 'InterpreterDirective',
            value: props.interpreter,
          })
        : null,
    docblock: props.docblock,
  });
}

// the type annotation is stored on the Identifier's typeAnnotation
// which is super awkward to work with and type - so we flatten the input
// and put it in the right spot after
export type DeclareFunctionProps = {
  readonly name: string,
  readonly functionType: MaybeDetachedNode<FunctionTypeAnnotationType>,
  readonly predicate?: ?MaybeDetachedNode<DeclareFunctionType['predicate']>,
};
export function DeclareFunction(props: {
  ...Readonly<DeclareFunctionProps>,
  readonly parent?: ESNode,
}): DetachedNode<DeclareFunctionType> {
  const node = detachedProps<DeclareFunctionType>(props.parent, {
    type: 'DeclareFunction',
    id: detachedProps(null, {
      type: 'Identifier',
      name: props.name,
      typeAnnotation: detachedProps(null, {
        type: 'TypeAnnotation',
        typeAnnotation: asDetachedNode(props.functionType),
      }),
    }),
    // $FlowFixMe[incompatible-type]
    predicate: asDetachedNode(props.predicate),
  });
  setParentPointersInDirectChildren(node);
  return node;
}

export type MemberExpressionProps = {
  readonly object: MaybeDetachedNode<MemberExpressionType['object']>,
  readonly property: MaybeDetachedNode<MemberExpressionType['property']>,
  readonly computed: MemberExpressionType['computed'],
  readonly optional?: MemberExpressionType['optional'],
};

export function MemberExpression(props: {
  ...Readonly<MemberExpressionProps>,
  readonly parent?: ESNode,
}): DetachedNode<MemberExpressionType> {
  const node = detachedProps<MemberExpressionType>(props.parent, {
    type: 'MemberExpression',
    object: asDetachedNode(props.object),
    property: asDetachedNode(props.property),
    computed: props.computed,
    optional: props.optional ?? false,
  });
  setParentPointersInDirectChildren(node);
  return node;
}

// Ignore the hermes-specific `implicit` property.
export type BlockStatementProps = {
  readonly body: ReadonlyArray<MaybeDetachedNode<StatementType>>,
};
export function BlockStatement(props: {
  ...Readonly<BlockStatementProps>,
  readonly parent?: ESNode,
}): DetachedNode<BlockStatementType> {
  const node = detachedProps<BlockStatementType>(props.parent, {
    type: 'BlockStatement',
    body: props.body.map(n => asDetachedNode(n)),
  });
  setParentPointersInDirectChildren(node);
  return node;
}
