/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import * as Before from './before';
import { BeforeClass } from './before';

export declare function viaImportType(): import('./before').BeforeClass;
export declare function viaNamespace(): Before.BeforeClass;
export declare function viaName(): BeforeClass;
