/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict
 * @format
 */

'use strict';

declare module '@typescript-eslint/visitor-keys' {
  declare export var visitorKeys: $ReadOnly<{
    [key: string]: $ReadOnlyArray<string>,
  }>;
}
