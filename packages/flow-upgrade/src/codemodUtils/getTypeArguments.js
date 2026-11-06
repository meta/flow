/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import type {TypeParameterInstantiation} from 'flow-estree';

type TypeApplication = $ReadOnly<{
  type: 'GenericTypeAnnotation',
  typeArguments?: ?TypeParameterInstantiation,
  typeParameters?: ?TypeParameterInstantiation,
  ...
}>;

export function getTypeArguments(
  node: TypeApplication,
): ?TypeParameterInstantiation {
  return 'typeArguments' in node ? node.typeArguments : node.typeParameters;
}
