/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// tsc accepts this file.
import type { LexicalNode } from './node';
export type NodeClass = typeof LexicalNode;
export declare class ParagraphNode extends LexicalNode {}
