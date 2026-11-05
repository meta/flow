/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 * @oncall flow
 */

'use strict';

/**
 * Sometimes we need to put parens around the object type so we produce
 * a valid fix.
 */
function doesObjectNeedsParens(node) {
  switch (node.type) {
    case 'IntersectionTypeAnnotation':
    case 'UnionTypeAnnotation':
    case 'NullableTypeAnnotation':
    case 'FunctionTypeAnnotation':
    case 'TypeofTypeAnnotation':
    case 'OptionalIndexedAccessType':
      return true;
    default:
      return false;
  }
}

function report(context, sourceCode, node) {
  const util = node.id.name;
  const typeArguments = Object.prototype.hasOwnProperty.call(
    node,
    'typeArguments',
  )
    ? node.typeArguments
    : node.typeParameters;

  // Defaults if we can't produce a fix.
  let fix = undefined;
  let objectText = 'Obj';
  let indexText = util === '$ElementType' ? 'K' : "'prop'";

  if (
    typeArguments != null &&
    typeArguments.type === 'TypeParameterInstantiation' &&
    Array.isArray(typeArguments.params) &&
    typeArguments.params.length === 2
  ) {
    const [objectNode, indexNode] = typeArguments.params;
    const objectNeedsParens = doesObjectNeedsParens(objectNode);
    const objectSource = sourceCode.getText(objectNode).trim();
    objectText = objectNeedsParens ? `(${objectSource})` : objectSource;
    indexText = sourceCode.getText(indexNode).trim();

    // Don't supply a fix if there are comments, which we don't handle
    if (sourceCode.getCommentsInside(node).length === 0) {
      const openingAngleBracket = sourceCode.getTokenAfter(
        node.id,
        token => token.type === 'Punctuator' && token.value === '<',
      );
      if (openingAngleBracket == null) {
        return;
      }
      const tokenAfterOpeningAngleBracket =
        sourceCode.getTokenAfter(openingAngleBracket);
      if (tokenAfterOpeningAngleBracket == null) {
        return;
      }
      const closingAngleBracket = sourceCode.getLastToken(
        node,
        token => token.type === 'Punctuator' && token.value === '>',
      );
      if (closingAngleBracket == null) {
        return;
      }
      const indexEndToken = sourceCode.getTokenBefore(
        closingAngleBracket,
        token => !(token.type === 'Punctuator' && token.value === ','),
      );
      if (indexEndToken == null) {
        return;
      }
      const comma = sourceCode.getTokenAfter(
        objectNode,
        token => token.type === 'Punctuator' && token.value === ',',
      );
      if (comma == null) {
        return;
      }
      const tokenAfterComma = sourceCode.getTokenAfter(comma);
      if (tokenAfterComma == null) {
        return;
      }

      fix = fixer => [
        objectNeedsParens
          ? fixer.replaceTextRange(
              [node.range[0], tokenAfterOpeningAngleBracket.range[0]],
              '(',
            )
          : fixer.removeRange([
              node.range[0],
              tokenAfterOpeningAngleBracket.range[0],
            ]),
        fixer.replaceTextRange(
          [comma.range[0], tokenAfterComma.range[0]],
          objectNeedsParens ? ')[' : '[',
        ),
        fixer.replaceTextRange(
          [indexEndToken.range[1], closingAngleBracket.range[1]],
          ']',
        ),
      ];
    }
  }

  context.report({
    node,
    messageId: 'useIndexedAccess',
    data: {
      util: node.id.name,
      object: objectText,
      index: indexText,
    },
    fix,
  });
}

const rule = {
  meta: {
    messages: {
      useIndexedAccess:
        'Use Indexed Access instead of `{{util}}`. E.g. you can change `{{util}}<{{object}}, {{index}}>` to `{{object}}[{{index}}]`.',
    },
    fixable: 'code',
  },
  create(context) {
    const sourceCode = context.getSourceCode();
    return {
      'GenericTypeAnnotation[id.type="Identifier"][id.name="$ElementType"]'(
        node,
      ) {
        report(context, sourceCode, node);
      },
      'GenericTypeAnnotation[id.type="Identifier"][id.name="$PropertyType"]'(
        node,
      ) {
        report(context, sourceCode, node);
      },
    };
  },
};

module.exports = rule;
