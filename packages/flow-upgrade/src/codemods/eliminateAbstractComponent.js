/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 * @flow
 */

import type {CodemodModule} from '../Types';
import type {GenericTypeAnnotation, TypeAnnotationType} from 'flow-estree';
import type {MaybeDetachedNode, TransformContext} from 'flow-transform';

import {codemod} from '../Types';
import {t} from 'flow-transform';
import {getTypeArguments} from '../codemodUtils/getTypeArguments';

function eliminateAbstractComponent(
  context: TransformContext,
  node: GenericTypeAnnotation,
): MaybeDetachedNode<TypeAnnotationType> | null {
  const {id: maybeQualifiedIdentifier} = node;
  const typeArguments = getTypeArguments(node);
  const innerType = typeArguments?.params[0];
  const innerTypeArguments =
    innerType?.type === 'GenericTypeAnnotation'
      ? getTypeArguments(innerType)
      : null;

  // `React.AbstractComponent<Props>` -> `React.ComponentType<Props>`
  // `React.AbstractComponent<Props, mixed>` -> `React.ComponentType<Props>`
  if (
    maybeQualifiedIdentifier.type === 'QualifiedTypeIdentifier' &&
    maybeQualifiedIdentifier.qualification.name === 'React' &&
    maybeQualifiedIdentifier.id.name === 'AbstractComponent' &&
    typeArguments != null
  ) {
    const [config, instance, renders] = typeArguments.params;

    if (renders != null) {
      return null;
    }

    if (instance == null || instance.type === 'MixedTypeAnnotation') {
      context.modifyNodeInPlace(maybeQualifiedIdentifier.id, {
        name: 'ComponentType',
      });
      context.modifyNodeInPlace(typeArguments, {params: [config]});
      return null;
    }

    return null;
  }

  // React.ComponentProps<React.AbstractComponent<Props, ...> -> Props
  if (
    maybeQualifiedIdentifier.type === 'QualifiedTypeIdentifier' &&
    maybeQualifiedIdentifier.qualification.name === 'React' &&
    (maybeQualifiedIdentifier.id.name === 'ComponentProps' ||
      maybeQualifiedIdentifier.id.name === 'ElementConfig') &&
    typeArguments != null &&
    typeArguments.params.length === 1 &&
    typeArguments.params[0].type === 'GenericTypeAnnotation' &&
    typeArguments.params[0].id.type === 'QualifiedTypeIdentifier' &&
    typeArguments.params[0].id.qualification.name === 'React' &&
    typeArguments.params[0].id.id.name === 'AbstractComponent' &&
    innerTypeArguments != null &&
    innerTypeArguments.params.length >= 1
  ) {
    return innerTypeArguments.params[0];
  }

  // React.ElementRef<React.AbstractComponent<Props, Instance, ...> -> Instance
  if (
    maybeQualifiedIdentifier.type === 'QualifiedTypeIdentifier' &&
    maybeQualifiedIdentifier.qualification.name === 'React' &&
    maybeQualifiedIdentifier.id.name === 'ElementRef' &&
    typeArguments != null &&
    typeArguments.params.length === 1 &&
    typeArguments.params[0].type === 'GenericTypeAnnotation' &&
    typeArguments.params[0].id.type === 'QualifiedTypeIdentifier' &&
    typeArguments.params[0].id.qualification.name === 'React' &&
    typeArguments.params[0].id.id.name === 'AbstractComponent' &&
    innerTypeArguments != null &&
    innerTypeArguments.params.length >= 2
  ) {
    return innerTypeArguments.params[1];
  }

  // React.ElementRef<React.AbstractComponent<Props> -> mixed
  if (
    maybeQualifiedIdentifier.type === 'QualifiedTypeIdentifier' &&
    maybeQualifiedIdentifier.qualification.name === 'React' &&
    maybeQualifiedIdentifier.id.name === 'ElementRef' &&
    typeArguments != null &&
    typeArguments.params.length === 1 &&
    typeArguments.params[0].type === 'GenericTypeAnnotation' &&
    typeArguments.params[0].id.type === 'QualifiedTypeIdentifier' &&
    typeArguments.params[0].id.qualification.name === 'React' &&
    typeArguments.params[0].id.id.name === 'AbstractComponent' &&
    innerTypeArguments != null &&
    innerTypeArguments.params.length === 1
  ) {
    return t.MixedTypeAnnotation({});
  }

  return null;
}

export default codemod({
  title: 'Eliminate `React.AbstractComponent<...>`',
  describe: `
- \`React.AbstractComponent<Props>\` -> \`React.ComponentType<Props>\`
- \`React.ComponentProps<React.AbstractComponent<Props, Instance>>\` -> \`Props\`
- \`React.ElementRef<React.AbstractComponent<Props, Instance>>\` -> \`Instance\`
- \`React.ElementRef<React.AbstractComponent<Props>\` -> \`mixed\``,
  transform: context => {
    return {
      GenericTypeAnnotation(node) {
        const replacement = eliminateAbstractComponent(context, node);
        if (replacement) {
          context.replaceNode(node, replacement, {
            keepComments: true,
          });
        }
      },
    };
  },
}) as CodemodModule;
