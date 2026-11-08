/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// The motivating cross-file declaration retains both the fractional field and its negative control.
import type {Spacing} from './spacing';
declare const spacing: Spacing;
const half: string = spacing[0.5]; // OK
const quotedHalf: string = spacing['0.5']; // OK
const bad: number = spacing.known; // ERROR
spacing[0.5] = 'new'; // ERROR
