/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import type {ESNode, Program} from 'flow-estree';

import {parse as parseTS} from '@typescript-eslint/parser';
import {SimpleTraverser} from 'flow-parser';
import {parse, print} from 'flow-transform';
import {visitorKeys as tsVisitorKeys} from '@typescript-eslint/visitor-keys';
import {flowDefToTSDef} from '../src/flowDefToTSDef';
import {TSDefToFlowDef} from '../src/TSDefToFlowDef';
import flowToFlowDef from '../src/flowToFlowDef';
// $FlowExpectedError[cannot-resolve-module]
import prettierConfig from '../../.prettierrc.json';

function expectCanonicalReferences(ast: Program): Array<ESNode> {
  const references = [];
  SimpleTraverser.traverse(ast, {
    enter(node) {
      switch (node.type) {
        case 'GenericTypeAnnotation':
        case 'InterfaceExtends':
        case 'ClassImplements':
          expect(node).toHaveProperty('typeArguments');
          expect(node).not.toHaveProperty('typeParameters');
          if (node.typeArguments != null) {
            expect(node.typeArguments.type).toBe('TypeParameterInstantiation');
          }
          references.push(node);
          break;
      }
    },
    leave() {},
  });
  return references;
}

async function translateToTS(code: string): Promise<string> {
  const {ast, scopeManager} = parse(code);
  const [result, mutatedCode] = flowDefToTSDef(code, ast, scopeManager, {
    recoverFromErrors: false,
  });
  return print(
    result as $FlowFixMe,
    mutatedCode,
    prettierConfig,
    tsVisitorKeys,
  );
}

function translateToFlow(code: string): Program {
  const ast = parseTS(code, {loc: true, range: true, sourceType: 'module'});
  return TSDefToFlowDef(code, ast, {recoverFromErrors: false})[0];
}

function extractFlowDefinition(code: string): Program {
  const {ast, scopeManager} = parse(code);
  return flowToFlowDef(ast, code, scopeManager, {
    recoverFromErrors: false,
  })[0] as $FlowFixMe;
}

describe('consumer type arguments', () => {
  test('Flow to TS preserves nested and qualified references and declarations', async () => {
    const result = await translateToTS(
      'type Plain = NS.Box; type Nested<T> = NS.Box<Map<string, Array<T>>>;',
    );
    expect(result).toContain('type Plain = NS.Box;');
    expect(result).toContain('type Nested<T> = NS.Box<Map<string, Array<T>>>;');
  });

  test('Flow to TS preserves declared superclass, implements, and interface arguments', async () => {
    const result = await translateToTS(`
      declare class C<T> extends NS.Base<Array<T>> implements NS.I<Map<string, T>> {}
      interface Child<T> extends NS.Base<Array<T>> {}
      type Inline<T> = interface extends Base<Array<T>> {};
    `);
    expect(result).toContain(
      'declare class C<T> extends NS.Base<Array<T>> implements NS.I<Map<string, T>> {}',
    );
    expect(result).toContain('interface Child<T> extends NS.Base<Array<T>> {}');
    expect(result).toContain('type Inline<T> = Base<Array<T>> & {};');
  });

  test('utility helpers consume exact and ranged argument lists', async () => {
    const result = await translateToTS(`
      type ReadOnly<T> = $ReadOnlyArray<NS.Box<T>>;
      type Prefix = StringPrefix<'pre', string>;
      type Suffix = StringSuffix<'end'>;
    `);
    expect(result).toContain('ReadonlyArray<NS.Box<T>>');
    expect(result).toContain('`pre${string}`');
    expect(result).toContain('`${string}end`');
  });

  test.each([
    ['type T = $ReadOnlyArray<string, number>;', 'Expected exactly 1'],
    [
      "type T = StringPrefix<'pre', string, number>;",
      'Expected between 1 and 2',
    ],
  ])('utility arity errors remain enforced: %s', async (code, message) => {
    await expect(translateToTS(code)).rejects.toThrow(message);
  });

  test('React utility helpers retain their argument lists', async () => {
    const result = await translateToTS(`
      type C = React.Component<Props, State>;
      type A = React.AbstractComponent<Props, Instance>;
    `);
    expect(result).toContain('React.Component<Props, State>');
    expect(result).toContain(
      'React.ComponentType<Props & React.RefAttributes<Instance>>',
    );
  });

  test('TS to Flow constructs canonical nested references and heritage', () => {
    const ast = translateToFlow(`
      type Plain = NS.Box;
      type Nested<T> = NS.Box<Map<string, Array<T>>>;
      interface Child<T> extends NS.Base<Array<T>> {}
      declare class C<T> extends NS.Base<Array<T>> implements I<Map<string, T>> {}
    `);
    const references = expectCanonicalReferences(ast);
    expect(references.some(node => node.type === 'InterfaceExtends')).toBe(
      true,
    );
    expect(references.some(node => node.type === 'ClassImplements')).toBe(true);
    expect(references).toContainEqual(
      expect.objectContaining({typeArguments: null}),
    );
    expect(ast.body[1]).toMatchObject({
      type: 'TypeAlias',
      typeParameters: {type: 'TypeParameterDeclaration', params: [{name: 'T'}]},
      right: {
        type: 'GenericTypeAnnotation',
        typeArguments: {
          params: [
            {
              type: 'GenericTypeAnnotation',
              typeArguments: {
                params: [
                  {type: 'StringTypeAnnotation'},
                  {
                    type: 'GenericTypeAnnotation',
                    typeArguments: {
                      params: [
                        {
                          type: 'GenericTypeAnnotation',
                          id: {name: 'T'},
                          typeArguments: null,
                        },
                      ],
                    },
                  },
                ],
              },
            },
          ],
        },
      },
    });
  });

  test('TS readonly, import and this constructions use canonical arguments', () => {
    const ast = translateToFlow(`
      type A = readonly string[];
      type B = readonly [string, number];
      type C = import('module');
      interface I { self: this; }
    `);
    const references = expectCanonicalReferences(ast);
    expect(references).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: expect.objectContaining({name: '$ReadOnlyArray'}),
        }),
        expect.objectContaining({
          id: expect.objectContaining({name: '$ReadOnly'}),
        }),
        expect.objectContaining({
          id: expect.objectContaining({name: '$Exports'}),
        }),
        expect.objectContaining({
          id: expect.objectContaining({name: 'this'}),
          typeArguments: null,
        }),
      ]),
    );
  });

  test('Flow definition extraction constructs superclass references and walks argument dependencies', () => {
    const ast = extractFlowDefinition(`
      type Arg = string;
      class Base<T> {}
      export class C<T> extends Base<Array<Arg>> {}
    `);
    expectCanonicalReferences(ast);
    expect(ast.body).toContainEqual(
      expect.objectContaining({
        type: 'TypeAlias',
        id: expect.objectContaining({name: 'Arg'}),
      }),
    );
    expect(ast.body[ast.body.length - 1]).toMatchObject({
      type: 'DeclareExportDeclaration',
      declaration: {
        type: 'DeclareClass',
        typeParameters: {params: [{name: 'T'}]},
        extends: [
          {
            type: 'InterfaceExtends',
            typeArguments: {
              params: [
                {
                  type: 'GenericTypeAnnotation',
                  id: {name: 'Array'},
                  typeArguments: {
                    params: [
                      {
                        type: 'GenericTypeAnnotation',
                        id: {name: 'Arg'},
                        typeArguments: null,
                      },
                    ],
                  },
                },
              ],
            },
          },
        ],
      },
    });
  });

  test('inferred async return construction retains Promise<void>', () => {
    const ast = extractFlowDefinition('export async function f() {}');
    const references = expectCanonicalReferences(ast);
    expect(references).toContainEqual(
      expect.objectContaining({
        type: 'GenericTypeAnnotation',
        id: expect.objectContaining({name: 'Promise'}),
        typeArguments: expect.objectContaining({
          params: [expect.objectContaining({type: 'VoidTypeAnnotation'})],
        }),
      }),
    );
  });
});
