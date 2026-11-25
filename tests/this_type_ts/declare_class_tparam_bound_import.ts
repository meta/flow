/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import { Node, Sub } from './declare_class_tparam_bound';

declare const node: Node;
declare const sub: Sub;
sub.config({ node: sub }); // OK
sub.config({ node: node }); // ERROR
