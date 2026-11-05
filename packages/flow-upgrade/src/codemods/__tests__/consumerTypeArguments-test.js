/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import {parse, SimpleTraverser} from 'flow-parser';
import {transform} from 'flow-transform';
import replaceReactDollarUtilityTypes from '../replaceReactDollarUtilityTypes';
import eliminateAbstractComponent from '../eliminateAbstractComponent';
import convertLegacyUtilityTypes from '../convertLegacyUtilityTypes';
import replaceTemporaryTypes from '../replaceTemporaryTypes';
import convertShapeToPartial from '../convertShapeToPartial';
import renamePartial from '../renamePartial';

const cases = [
  {
    name: 'React dollar utility',
    codemod: replaceReactDollarUtilityTypes,
    code: 'type T<X> = React$Element<Array<X>>;',
    output: 'type T<X> = React.Element<Array<X>>;',
  },
  {
    name: 'AbstractComponent',
    codemod: eliminateAbstractComponent,
    code: 'type T<X> = React.AbstractComponent<Array<X>>;',
    output: 'type T<X> = React.ComponentType<Array<X>>;',
  },
  {
    name: 'nested AbstractComponent props',
    codemod: eliminateAbstractComponent,
    code: 'type T<X> = React.ComponentProps<React.AbstractComponent<Array<X>, Instance>>;',
    output: 'type T<X> = Array<X>;',
  },
  {
    name: 'nested AbstractComponent ref',
    codemod: eliminateAbstractComponent,
    code: 'type T<X> = React.ElementRef<React.AbstractComponent<Props, Array<X>>>;',
    output: 'type T<X> = Array<X>;',
  },
  {
    name: 'legacy utility argument read',
    codemod: convertLegacyUtilityTypes,
    code: 'type T<X> = $Keys<NS.Box<X>>;',
    output: 'type T<X> = keyof NS.Box<X>;',
  },
  {
    name: 'temporary type argument guard',
    codemod: replaceTemporaryTypes,
    code: 'type T<X> = $TEMPORARY$array<Box<X>>;',
    output: 'type T<X> = $ReadOnlyArray<Box<X>>;',
  },
  {
    name: 'Shape selector',
    codemod: convertShapeToPartial,
    code: 'type T<X> = $Shape<Box<X>>;',
    output: 'type T<X> = Partial<Box<X>>;',
  },
  {
    name: 'Partial selector',
    codemod: renamePartial,
    code: 'type T<X> = $Partial<Box<X>>;',
    output: 'type T<X> = Partial<Box<X>>;',
  },
];

test.each(cases)(
  '$name consumes canonical type arguments',
  async ({codemod, code, output}) => {
    const result = await transform(code, codemod.transform);
    expect(result.trim()).toBe(output);
    const ast = parse(result, {flow: 'all'});
    expect(ast.body[0]).toMatchObject({
      type: 'TypeAlias',
      typeParameters: {type: 'TypeParameterDeclaration', params: [{name: 'X'}]},
    });
    let genericCount = 0;
    SimpleTraverser.traverse(ast, {
      enter(node) {
        if (node.type === 'GenericTypeAnnotation') {
          genericCount++;
          expect(node).toHaveProperty('typeArguments');
          expect(node).not.toHaveProperty('typeParameters');
        }
      },
      leave() {},
    });
    expect(genericCount).toBeGreaterThan(0);
  },
);
