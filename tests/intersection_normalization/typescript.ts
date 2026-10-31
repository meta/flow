/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

type TypeScriptMergedObject = {left: string} & {right: number};

declare const typeScriptMergedObject: TypeScriptMergedObject;
typeScriptMergedObject.left as string;
typeScriptMergedObject.right as number;

interface TypeScriptInterface {
  fromInterface: boolean;
}

type TypeScriptMergedInterface = TypeScriptInterface & {fromObject: string};
declare const typeScriptMergedInterface: TypeScriptMergedInterface;
typeScriptMergedInterface.fromInterface as boolean;
typeScriptMergedInterface.fromObject as string;

export type ImportedReadonlyDictionary =
  {readonly [key: string]: string} & {readonly [key: string]: string};

export interface ImportedTypeScriptMember {
  shared: string;
}
