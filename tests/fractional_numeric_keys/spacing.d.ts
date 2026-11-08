/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// Fractional numeric keys in exported declarations retain ordinary string property types.
export type Spacing = {
  readonly 0.5: string; // OK
  known: string;
};
