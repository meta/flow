/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

'use strict';

import type {ObjectWithLoc, TypeAnnotationType} from 'flow-estree';
import type {TranslationContext} from './TranslationUtils';
import type {DetachedNode} from 'flow-transform';

import {t} from 'flow-transform';
import {codeFrameColumns} from '@babel/code-frame';

export function flowFixMeOrError(
  container: ObjectWithLoc,
  message: string,
  context: TranslationContext,
): DetachedNode<TypeAnnotationType> {
  if (context.recoverFromErrors) {
    return t.GenericTypeAnnotation({id: t.Identifier({name: '$FlowFixMe'})});
  }
  throw translationError(container, message, context);
}

class TranslationErrorBase extends Error {
  name: string = 'TranslationError';
  constructor(
    node: ObjectWithLoc,
    message: string,
    context: Readonly<{code: string, ...}>,
  ) {
    const framedMessage = buildCodeFrame(node, message, context.code);
    super(
      // jest error snapshots will use a hard-coded string representation if
      // `instanceof Error` which makes the code frame look awful and hard to verify:
      //
      // [TranslationError: > 12 | code
      //       | ^^^^ error]
      //
      // this just adds a newline in jest tests so that it all lines up nicely
      //
      // [TranslationError:
      // > 12 | code
      //      | ^^^^ error]
      process.env.JEST_WORKER_ID == null ? framedMessage : `\n${framedMessage}`,
    );
  }
}

export class ExpectedTranslationError extends TranslationErrorBase {
  name: string = 'ExpectedTranslationError';
}
export function translationError(
  node: ObjectWithLoc,
  message: string,
  context: Readonly<{code: string, ...}>,
): ExpectedTranslationError {
  return new ExpectedTranslationError(node, message, context);
}

export class UnexpectedTranslationError extends TranslationErrorBase {
  name: string = 'UnexpectedTranslationError';
}
export function unexpectedTranslationError(
  node: ObjectWithLoc,
  message: string,
  context: Readonly<{code: string, ...}>,
): UnexpectedTranslationError {
  return new UnexpectedTranslationError(node, message, context);
}

export function buildCodeFrame(
  node: ObjectWithLoc,
  message: string,
  code: string,
  highlightCode: boolean = process.env.NODE_ENV !== 'test',
): string {
  if (code.length === 0) {
    return message;
  }

  // babel uses 1-indexed columns
  const locForBabel = {
    start: {
      line: node.loc.start.line,
      column: node.loc.start.column + 1,
    },
    end: {
      line: node.loc.end.line,
      column: node.loc.end.column + 1,
    },
  };
  return codeFrameColumns(code, locForBabel, {
    linesAbove: 0,
    linesBelow: 0,
    highlightCode,
    message: message,
  });
}
