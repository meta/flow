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

const rule = require('../use-indexed-access-type');
const parser = require('flow-eslint');

function reportFor(node) {
  const report = jest.fn();
  const context = {
    report,
    getSourceCode() {
      return {
        getText(child) {
          return child.text;
        },
        getCommentsInside() {
          return [{}];
        },
      };
    },
  };
  const visitor = rule.create(context);
  visitor[
    'GenericTypeAnnotation[id.type="Identifier"][id.name="$ElementType"]'
  ](node);
  return report.mock.calls[0][0];
}

const argumentsNode = {
  type: 'TypeParameterInstantiation',
  params: [
    {type: 'GenericTypeAnnotation', text: 'Box<Array<string>>'},
    {type: 'StringLiteralTypeAnnotation', text: "'key'"},
  ],
};

test('the runtime parser exposes canonical type arguments', () => {
  const result = parser.parseForESLint(
    'type T = $ElementType<Box<Array<string>>, Key>;',
    {
      sourceType: 'module',
    },
  );
  expect(result.ast.body[0].right).toHaveProperty('typeArguments');
  expect(result.ast.body[0].right).not.toHaveProperty('typeParameters');
});

test('canonical argument lists drive diagnostic text', () => {
  const result = reportFor({
    type: 'GenericTypeAnnotation',
    id: {name: '$ElementType'},
    typeArguments: argumentsNode,
  });
  expect(result.data).toEqual({
    util: '$ElementType',
    object: 'Box<Array<string>>',
    index: "'key'",
  });
  expect(result.fix).toBeUndefined();
});

test('old parser argument lists remain supported when the canonical field is absent', () => {
  const result = reportFor({
    type: 'GenericTypeAnnotation',
    id: {name: '$ElementType'},
    typeParameters: argumentsNode,
  });
  expect(result.data.object).toBe('Box<Array<string>>');
  expect(result.data.index).toBe("'key'");
});

test('an explicit canonical null does not fall back to a stale old-name field', () => {
  const result = reportFor({
    type: 'GenericTypeAnnotation',
    id: {name: '$ElementType'},
    typeArguments: null,
    typeParameters: argumentsNode,
  });
  expect(result.data).toEqual({
    util: '$ElementType',
    object: 'Obj',
    index: 'K',
  });
  expect(result.fix).toBeUndefined();
});
