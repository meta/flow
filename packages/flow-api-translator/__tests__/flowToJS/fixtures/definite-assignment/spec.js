/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 * @noformat
 */

let count!: number;
let first!: number, second!: string;
class Box {
  value!: number;
  #secret!: string;
  ['computed']!: boolean;
}
