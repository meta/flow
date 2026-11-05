/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

type TsVoidHandler = (event: string) => void;
type TsNumberHandler = (event: string) => number;

interface TsProps {
  onEvent?: TsVoidHandler | undefined;
}

declare function takeTsVoidHandler(handler: TsVoidHandler): void;
declare function takeTsNumberHandler(handler: TsNumberHandler): void;
declare function takeTsProps(props: TsProps): void;
