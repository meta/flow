/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

declare module 'flow-parser' {
  declare export function parse(
    code: string,
    options: {flow: 'all', babel: false, throwOnParseErrors: true},
  ): {interpreter?: any, comments: Array<any>, ...};
  declare export function mutateESTreeASTForPrettier(
    ast: {
      interpreter?: any,
      comments: Array<any>,
      ...
    },
    visitorKeys?: ?Readonly<Record<string, ReadonlyArray<string>>>,
    prettierVersion?: string,
  ): void;
}
