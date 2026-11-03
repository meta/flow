/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

declare module 'prettier/standalone' {
  declare export function formatWithCursor(
    code: string,
    options: {
      parser: string,
      plugins: ReadonlyArray<unknown>,
      cursorOffset: number,
      endOfLine: 'auto',
    },
  ): Promise<{formatted: string, cursorOffset: number}>;
}

declare module 'prettier/plugins/estree' {
  declare export var printers: any;
}
