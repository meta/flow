/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 * @flow
 */

import type {CodemodModule} from '../Types';

import {codemod} from '../Types';

const AMBIGUOUS_OBJECT_TYPE_DIRECTIVE =
  /^[ \t\r\n*]*flowlint(?:-line|-next-line)?[ \t\r\n*]+ambiguous-object-type[ \t\r\n*]*:[ \t\r\n*]*(?:error|warn|off)[ \t\r\n*]*$/;

export default codemod({
  title: 'Remove explicitly exact object type syntax',
  describe:
    'Convert explicitly exact object type syntax `{| |}` to be just `{ }`. Object types are exact by default. Also removes comments that only set the `ambiguous-object-type` lint.',
  transform: context => {
    return {
      Program(node) {
        // Removing one comment from a file with no statements makes the
        // printer drop all of its comments.
        if (node.body.length === 0) {
          return;
        }
        for (const comment of node.comments) {
          if (AMBIGUOUS_OBJECT_TYPE_DIRECTIVE.test(comment.value)) {
            context.removeComments(comment);
            if (node.docblock?.comment === comment) {
              context.modifyNodeInPlace(node, {docblock: null});
            }
          }
        }
      },
      'ObjectTypeAnnotation[exact=true]'(node) {
        context.modifyNodeInPlace(node, {
          exact: false,
        });
      },
    };
  },
}) as CodemodModule;
