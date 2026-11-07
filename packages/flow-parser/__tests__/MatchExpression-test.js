/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import {printForSnapshot} from '../__test_utils__/parse';

const transform = (src: string) => printForSnapshot(src, {babel: true});

class C {
  foo: number;
  bar: string;
  baz: string;

  constructor({foo, bar, baz}: {foo: number, bar: string, baz: string}) {
    this.foo = foo;
    this.bar = bar;
    this.baz = baz;
  }
}

function runMatchExp(code: string, x: unknown): unknown {
  const f: $FlowFixMe = new Function(
    'x',
    'foo',
    'bar',
    'no',
    'yes',
    'C',
    `${code}; return e;`,
  );
  return f(
    x,
    'foo',
    {a: 'bar'},
    () => false,
    () => true,
    C,
  );
}

describe('MatchExpression', () => {
  test('no cases', async () => {
    const code = `
      const e = match (x()) {};
    `;
    const output = await transform(code);
    expect(output).toMatchInlineSnapshot(`
      "const e = ($$gen$m0 => {
        throw Error("Match: No case succesfully matched. Make exhaustive or add a wildcard case using '_'. Argument: " + $$gen$m0);
      })(x());"
    `);

    const fn1 = jest.fn(() => 'xxx');
    expect(() => runMatchExp(output, fn1)).toThrow(
      /Match: No case succesfully matched\. .* Argument: xxx/,
    );
    expect(fn1).toHaveBeenCalled();

    const fn2 = jest.fn(() => undefined);
    expect(() => runMatchExp(output, fn2)).toThrow(
      /Match: No case succesfully matched\. .* Argument: undefined/,
    );
    expect(fn2).toHaveBeenCalled();
  });

  test('generated variables', async () => {
    const code = `
      const $$gen$0 = true;
      const e = match (x()) {
        'a' => $$gen$0,
        _ => 1,
      };
    `;
    const output = await transform(code);
    expect(output).toMatchInlineSnapshot(`
      "const $$gen$0 = true;

      const e = ($$gen$m0 => {
        if ($$gen$m0 === 'a') {
          return $$gen$0;
        }

        return 1;
      })(x());"
    `);

    expect(runMatchExp(output, () => 'a')).toBe(true);
  });

  describe('transform errors', () => {
    test('+/-0 not allowed', async () => {
      const code = `
        const e = match (x) {
          -0 => 0,
        }
      `;
      await expect(async () => {
        await transform(code);
      }).rejects.toThrow('not yet supported');
    });

    test('bindings in "or" patterns', async () => {
      const code = `
        const e = match (x) {
          [const a] | {const a} => 0,
        }
      `;
      await expect(async () => {
        await transform(code);
      }).rejects.toThrow('not yet supported');
    });

    test('as pattern on binding pattern', async () => {
      const code = `
        const e = match (x) {
          const x as y => 0,
          _ => 1,
        }
      `;
      await expect(async () => {
        await transform(code);
      }).rejects.toThrow('not allowed');
    });

    test('var binding: binding pattern', async () => {
      const code = `
        const e = match (x) {
          var a => 0,
          _ => 1,
        }
      `;
      await expect(async () => {
        await transform(code);
      }).rejects.toThrow('not allowed');
    });

    test('var binding: as pattern', async () => {
      const code = `
        const e = match (x) {
          0 as var a => 0,
          _ => 1,
        }
      `;
      await expect(async () => {
        await transform(code);
      }).rejects.toThrow('not allowed');
    });

    test('var binding: array rest', async () => {
      const code = `
        const e = match (x) {
          [...var a] => 0,
          _ => 1,
        }
      `;
      await expect(async () => {
        await transform(code);
      }).rejects.toThrow('not allowed');
    });

    test('var binding: object rest', async () => {
      const code = `
        const e = match (x) {
          {...var a} => 0,
          _ => 1,
        }
      `;
      await expect(async () => {
        await transform(code);
      }).rejects.toThrow('not allowed');
    });

    test('duplicate object props', async () => {
      const code = `
        const e = match (x) {
          {a: 0, const a} => 0,
          _ => 1,
        }
      `;
      await expect(async () => {
        await transform(code);
      }).rejects.toThrow(`Duplicate property name 'a'`);
    });

    test('duplicate binding name: binding pattern', async () => {
      const code = `
        const e = match (x) {
          [const a, const a] => 0,
          _ => 1,
        }
      `;
      await expect(async () => {
        await transform(code);
      }).rejects.toThrow(`Duplicate variable name 'a'`);
    });

    test('duplicate binding name: as pattern', async () => {
      const code = `
        const e = match (x) {
          [const a, 1 as const a] => 0,
          _ => 1,
        }
      `;
      await expect(async () => {
        await transform(code);
      }).rejects.toThrow(`Duplicate variable name 'a'`);
    });

    test('duplicate binding name: array rest', async () => {
      const code = `
        const e = match (x) {
          [const a, ...const a] => 0,
          _ => 1,
        }
      `;
      await expect(async () => {
        await transform(code);
      }).rejects.toThrow(`Duplicate variable name 'a'`);
    });

    test('duplicate binding name: object rest', async () => {
      const code = `
        const e = match (x) {
          {const a, ...const a} => 0,
          _ => 1,
        }
      `;
      await expect(async () => {
        await transform(code);
      }).rejects.toThrow(`Duplicate variable name 'a'`);
    });
  });

  describe('conditional expression output', () => {
    test('simple and guards', async () => {
      const code = `
        const e = match (x) {
          'a' => 0,
          888 => 1,
          -999 => 2,
          foo => 3,
          bar.a => 4,
          NaN => 5,
          'b' if (no()) => 6,
          _ if (yes()) => 7,
          _ => 8,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(
        `"const e = x === 'a' ? 0 : x === 888 ? 1 : x === -999 ? 2 : x === foo ? 3 : x === bar.a ? 4 : Number.isNaN(x) ? 5 : x === 'b' && no() ? 6 : yes() ? 7 : 8;"`,
      );

      expect(runMatchExp(output, 'a')).toBe(0);
      expect(runMatchExp(output, 'foo')).toBe(3);
      expect(runMatchExp(output, NaN)).toBe(5);
      expect(runMatchExp(output, null)).toBe(7);
    });

    test('simple argument', async () => {
      const code = `
        const e = match (x.foo['bar'][0]) {
          'a' => 0,
          'b' => 1,
          _ => 2,
        }
      `;

      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(
        `"const e = x.foo['bar'][0] === 'a' ? 0 : x.foo['bar'][0] === 'b' ? 1 : 2;"`,
      );
      expect(runMatchExp(output, {foo: {bar: ['a']}})).toBe(0);
    });

    test('no wildcard', async () => {
      const code = `
        const e = match (x) {
          'a' => 0,
          'b' => 1,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`
        "const e = x === 'a' ? 0 : x === 'b' ? 1 : (() => {
          throw Error("Match: No case succesfully matched. Make exhaustive or add a wildcard case using '_'. Argument: " + x);
        })();"
      `);

      expect(runMatchExp(output, 'a')).toBe(0);
      expect(runMatchExp(output, 'b')).toBe(1);
      expect(() => runMatchExp(output, 'xxx')).toThrow(
        /Match: No case succesfully matched\. .* Argument: xxx/,
      );
    });

    test('only wildcard', async () => {
      const code = `
        const e = match (x) {
          _ => 1,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`"const e = 1;"`);

      expect(runMatchExp(output, 'xxx')).toBe(1);
    });

    test('objects', async () => {
      const code = `
        const e = match (x) {
          {a: 'a', b: 'b'} => 0,
          {c: _, d: _} => 1,
          {999: 999, 's': 's'} => 2,
          {} => 3,
          _ => 4,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(
        `"const e = (typeof x === "object" && x !== null || typeof x === "function") && x.a === 'a' && x.b === 'b' ? 0 : (typeof x === "object" && x !== null || typeof x === "function") && "c" in x && "d" in x ? 1 : (typeof x === "object" && x !== null || typeof x === "function") && x[999] === 999 && x['s'] === 's' ? 2 : typeof x === "object" && x !== null || typeof x === "function" ? 3 : 4;"`,
      );

      expect(runMatchExp(output, {a: 'a', b: 'b'})).toBe(0);
      expect(runMatchExp(output, {c: true, d: false})).toBe(1);
      expect(runMatchExp(output, {xxx: true})).toBe(3);
      expect(runMatchExp(output, null)).toBe(4);
    });

    test('objects matches arrays', async () => {
      const code = `
        const e = match (x) {
          {0: 'a', 1: 'b'} => 0,
          _ => 1,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(
        `"const e = (typeof x === "object" && x !== null || typeof x === "function") && x[0] === 'a' && x[1] === 'b' ? 0 : 1;"`,
      );

      expect(runMatchExp(output, {0: 'a', 1: 'b'})).toBe(0);
      expect(runMatchExp(output, ['a', 'b'])).toBe(0);
      expect(runMatchExp(output, ['a'])).toBe(1);
    });

    test('objects matches classes and functions', async () => {
      const code = `
        const e = match (x) {
          {foo: true, ...} => 0,
          {...} => 1,
          _ => 2,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(
        `"const e = (typeof x === "object" && x !== null || typeof x === "function") && x.foo === true ? 0 : typeof x === "object" && x !== null || typeof x === "function" ? 1 : 2;"`,
      );

      expect(runMatchExp(output, () => {})).toBe(1);
      expect(runMatchExp(output, class {})).toBe(1);

      const arrow = () => {};
      arrow.foo = true;
      expect(runMatchExp(output, arrow)).toBe(0);

      class C {
        static foo = true;
      }
      expect(runMatchExp(output, C)).toBe(0);
    });

    test('arrays', async () => {
      const code = `
        const e = match (x) {
          ['a'] => 0,
          ['b', ...] => 1,
          [_] => 2,
          _ => 3,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(
        `"const e = Array.isArray(x) && x.length === 1 && x[0] === 'a' ? 0 : Array.isArray(x) && x.length >= 1 && x[0] === 'b' ? 1 : Array.isArray(x) && x.length === 1 ? 2 : 3;"`,
      );

      expect(runMatchExp(output, ['a'])).toBe(0);
      expect(runMatchExp(output, ['b'])).toBe(1);
      expect(runMatchExp(output, ['b', true, false])).toBe(1);
      expect(runMatchExp(output, ['xxx'])).toBe(2);
      expect(runMatchExp(output, [])).toBe(3);
    });

    test('as', async () => {
      const code = `
        const e = match (x) {
          (1 | 2) as a => a,
          [(3 | 4) as const a] => -a,
          _ => 3,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`
        "const e = ($$gen$m0 => {
          if ($$gen$m0 === 1 || $$gen$m0 === 2) {
            const a = $$gen$m0;
            return a;
          }

          if (Array.isArray($$gen$m0) && $$gen$m0.length === 1 && ($$gen$m0[0] === 3 || $$gen$m0[0] === 4)) {
            const a = $$gen$m0[0];
            return -a;
          }

          return 3;
        })(x);"
      `);

      expect(runMatchExp(output, 1)).toBe(1);
      expect(runMatchExp(output, 2)).toBe(2);
      expect(runMatchExp(output, [3])).toBe(-3);
      expect(runMatchExp(output, [4])).toBe(-4);
      expect(runMatchExp(output, 'xxx')).toBe(3);
    });

    test('or: simple', async () => {
      const code = `
        const e = match (x) {
          'a' | 'b' => 0,
          _ => 1,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(
        `"const e = x === 'a' || x === 'b' ? 0 : 1;"`,
      );

      expect(runMatchExp(output, 'a')).toBe(0);
      expect(runMatchExp(output, 'b')).toBe(0);
      expect(runMatchExp(output, 'xxx')).toBe(1);
    });

    test('or: wildcard', async () => {
      const code = `
        const e = match (x) {
          'a' | _ => 0,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`"const e = 0;"`);

      expect(runMatchExp(output, 'xxx')).toBe(0);
    });

    test('or: complex', async () => {
      const code = `
        const e = match (x) {
          [[1] | {foo: 2}] => 0,
          _ => 1,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(
        `"const e = Array.isArray(x) && x.length === 1 && (Array.isArray(x[0]) && x[0].length === 1 && x[0][0] === 1 || (typeof x[0] === "object" && x[0] !== null || typeof x[0] === "function") && x[0].foo === 2) ? 0 : 1;"`,
      );

      expect(runMatchExp(output, [[1]])).toBe(0);
      expect(runMatchExp(output, [{foo: 2}])).toBe(0);
      expect(runMatchExp(output, [[2]])).toBe(1);
    });

    test('nested', async () => {
      const code = `
        const e = match (x) {
          'a' | 'b' => match (x) {
            'a' => 0,
            'b' => 1,
          },
          _ => 2,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`
        "const e = x === 'a' || x === 'b' ? x === 'a' ? 0 : x === 'b' ? 1 : (() => {
          throw Error("Match: No case succesfully matched. Make exhaustive or add a wildcard case using '_'. Argument: " + x);
        })() : 2;"
      `);

      expect(runMatchExp(output, 'a')).toBe(0);
      expect(runMatchExp(output, 'b')).toBe(1);
      expect(runMatchExp(output, 'xxx')).toBe(2);
    });
  });

  describe('iife output', () => {
    test('simple and guards', async () => {
      const code = `
        const e = match (x) {
          'a' => 0,
          888 => 1,
          -999 => 2,
          foo => 3,
          bar.a => 4,
          'b' if (no()) => 5,
          const foo if (yes(foo)) => 6,
          const bar => bar,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`
        "const e = ($$gen$m0 => {
          if ($$gen$m0 === 'a') {
            return 0;
          }

          if ($$gen$m0 === 888) {
            return 1;
          }

          if ($$gen$m0 === -999) {
            return 2;
          }

          if ($$gen$m0 === foo) {
            return 3;
          }

          if ($$gen$m0 === bar.a) {
            return 4;
          }

          if ($$gen$m0 === 'b') {
            if (no()) return 5;
          }

          {
            const foo = $$gen$m0;
            if (yes(foo)) return 6;
          }
          {
            const bar = $$gen$m0;
            return bar;
          }
        })(x);"
      `);

      expect(runMatchExp(output, 'a')).toBe(0);
      expect(runMatchExp(output, 'foo')).toBe(3);
      expect(runMatchExp(output, null)).toBe(6);
    });

    test('simple and same named binding', async () => {
      const code = `
        function f(x) {
          return match (x) {
            [const x] => x,
            const x => x,
          };
        }
        const e = f(x);
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`
        "function f(x) {
          return ($$gen$m0 => {
            if (Array.isArray($$gen$m0) && $$gen$m0.length === 1) {
              const x = $$gen$m0[0];
              return x;
            }

            {
              const x = $$gen$m0;
              return x;
            }
          })(x);
        }

        const e = f(x);"
      `);

      expect(runMatchExp(output, [1])).toBe(1);
      expect(runMatchExp(output, 2)).toBe(2);
    });

    test('non-simple argument: call', async () => {
      const code = `
        const e = match (x()) {
          'a' => 0,
          _ => 1,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`
        "const e = ($$gen$m0 => {
          if ($$gen$m0 === 'a') {
            return 0;
          }

          return 1;
        })(x());"
      `);

      expect(runMatchExp(output, () => 'a')).toBe(0);
      expect(runMatchExp(output, () => false)).toBe(1);
    });

    test('non-simple argument: computed access', async () => {
      const code = `
        const e = match (x[1 + 2 - 3]) {
          'a' => 0,
          _ => 1,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`
        "const e = ($$gen$m0 => {
          if ($$gen$m0 === 'a') {
            return 0;
          }

          return 1;
        })(x[1 + 2 - 3]);"
      `);

      expect(runMatchExp(output, ['a'])).toBe(0);
      expect(runMatchExp(output, ['xxx'])).toBe(1);
    });

    test('no wildcard', async () => {
      const code = `
        const e = match (x()) {
          'a' => 0,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`
        "const e = ($$gen$m0 => {
          if ($$gen$m0 === 'a') {
            return 0;
          }

          throw Error("Match: No case succesfully matched. Make exhaustive or add a wildcard case using '_'. Argument: " + $$gen$m0);
        })(x());"
      `);

      expect(runMatchExp(output, () => 'a')).toBe(0);
      expect(() => runMatchExp(output, () => 123)).toThrow(
        /Match: No case succesfully matched\. .* Argument: 123/,
      );
    });

    test('only wildcard', async () => {
      const code = `
        const e = match (x()) {
          _ => 0,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`
        "const e = ($$gen$m0 => {
          return 0;
        })(x());"
      `);

      expect(runMatchExp(output, () => 'xxx')).toBe(0);
    });

    test('arrays', async () => {
      const code = `
        const e = match (x) {
          [1, const a] => a,
          _ => 1,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`
        "const e = ($$gen$m0 => {
          if (Array.isArray($$gen$m0) && $$gen$m0.length === 2 && $$gen$m0[0] === 1) {
            const a = $$gen$m0[1];
            return a;
          }

          return 1;
        })(x);"
      `);

      expect(runMatchExp(output, [1, 'foo'])).toBe('foo');
      expect(runMatchExp(output, [1])).toBe(1);
      expect(runMatchExp(output, [1, 2, 3])).toBe(1);
    });

    test('array rest', async () => {
      const code = `
        const e = match (x) {
          [1, 2, ...const rest] => rest,
          _ => 1,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`
        "const e = ($$gen$m0 => {
          if (Array.isArray($$gen$m0) && $$gen$m0.length >= 2 && $$gen$m0[0] === 1 && $$gen$m0[1] === 2) {
            const rest = $$gen$m0.slice(2);
            return rest;
          }

          return 1;
        })(x);"
      `);

      expect(runMatchExp(output, [1, 2])).toStrictEqual([]);
      expect(runMatchExp(output, [1, 2, 3, 4])).toStrictEqual([3, 4]);
      expect(runMatchExp(output, [2, 1])).toBe(1);
    });

    test('objects', async () => {
      const code = `
        const e = match (x) {
          {foo: 1, bar: const a, const baz} => a + baz,
          _ => 1,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`
        "const e = ($$gen$m0 => {
          if ((typeof $$gen$m0 === "object" && $$gen$m0 !== null || typeof $$gen$m0 === "function") && $$gen$m0.foo === 1 && "bar" in $$gen$m0 && "baz" in $$gen$m0) {
            const a = $$gen$m0.bar;
            const baz = $$gen$m0.baz;
            return a + baz;
          }

          return 1;
        })(x);"
      `);

      expect(runMatchExp(output, {foo: 1, bar: 'bar', baz: 'baz'})).toBe(
        'barbaz',
      );
      expect(runMatchExp(output, {foo: true, bar: 'bar', baz: 'baz'})).toBe(1);
    });

    test('object rest', async () => {
      const code = `
        const e = match (x) {
          {foo: 1, bar: const a, ...const rest} => rest,
          _ => 1,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`
        "const e = ($$gen$m0 => {
          if ((typeof $$gen$m0 === "object" && $$gen$m0 !== null || typeof $$gen$m0 === "function") && $$gen$m0.foo === 1 && "bar" in $$gen$m0) {
            const a = $$gen$m0.bar;
            const {
              foo: $$gen$m1,
              bar: $$gen$m2,
              ...rest
            } = $$gen$m0;
            return rest;
          }

          return 1;
        })(x);"
      `);

      expect(runMatchExp(output, {foo: 1, bar: 2})).toStrictEqual({});
      expect(runMatchExp(output, {foo: 1, bar: 2, a: 3, b: 4})).toStrictEqual({
        a: 3,
        b: 4,
      });
      expect(runMatchExp(output, {foo: false, bar: 2})).toBe(1);
    });

    test('object property with `undefined` value', async () => {
      const code = `
        const e = match (x) {
          {foo: undefined} => true,
          {bar: undefined as const a} => true,
          {baz: 0 | undefined} => true,
          _ => false,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`
        "const e = ($$gen$m0 => {
          if ((typeof $$gen$m0 === "object" && $$gen$m0 !== null || typeof $$gen$m0 === "function") && "foo" in $$gen$m0 && $$gen$m0.foo === undefined) {
            return true;
          }

          if ((typeof $$gen$m0 === "object" && $$gen$m0 !== null || typeof $$gen$m0 === "function") && "bar" in $$gen$m0 && $$gen$m0.bar === undefined) {
            const a = $$gen$m0.bar;
            return true;
          }

          if ((typeof $$gen$m0 === "object" && $$gen$m0 !== null || typeof $$gen$m0 === "function") && "baz" in $$gen$m0 && ($$gen$m0.baz === 0 || $$gen$m0.baz === undefined)) {
            return true;
          }

          return false;
        })(x);"
      `);

      expect(runMatchExp(output, {})).toBe(false);
      expect(runMatchExp(output, {foo: undefined})).toBe(true);
      expect(runMatchExp(output, {bar: undefined})).toBe(true);
      expect(runMatchExp(output, {baz: undefined})).toBe(true);
    });

    test('instance pattern', async () => {
      const code = `
        const e = match (x) {
          C {foo: 1, bar: const a, const baz} => a + baz,
          _ => 1,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`
       "const e = ($$gen$m0 => {
         if ($$gen$m0 instanceof C && $$gen$m0.foo === 1 && "bar" in $$gen$m0 && "baz" in $$gen$m0) {
           const a = $$gen$m0.bar;
           const baz = $$gen$m0.baz;
           return a + baz;
         }

         return 1;
       })(x);"
      `);

      expect(runMatchExp(output, new C({foo: 1, bar: 'bar', baz: 'baz'}))).toBe(
        'barbaz',
      );
      expect(runMatchExp(output, {foo: 1, bar: 'bar', baz: 'baz'})).toBe(1);
      expect(runMatchExp(output, null)).toBe(1);
    });

    test('instance pattern with structural fallback', async () => {
      const output = await transform(`
        const e = match (x) {
          C {const foo, ...} => 'instance:' + foo,
          {const foo, ...} => 'object:' + foo,
          _ => 'none',
        };
      `);

      expect(runMatchExp(output, new C({foo: 1, bar: 'bar', baz: 'baz'}))).toBe(
        'instance:1',
      );
      expect(runMatchExp(output, {foo: 1})).toBe('object:1');
      expect(runMatchExp(output, Object.create({foo: 1}))).toBe('object:1');
      expect(runMatchExp(output, null)).toBe('none');
      expect(runMatchExp(output, undefined)).toBe('none');
    });

    test('instance pattern rest', async () => {
      const code = `
        const e = match (x) {
          C {foo: 1, bar: const a, ...const rest} => rest,
          _ => 1,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`
       "const e = ($$gen$m0 => {
         if ($$gen$m0 instanceof C && $$gen$m0.foo === 1 && "bar" in $$gen$m0) {
           const a = $$gen$m0.bar;
           const {
             foo: $$gen$m1,
             bar: $$gen$m2,
             ...rest
           } = $$gen$m0;
           return rest;
         }

         return 1;
       })(x);"
      `);

      expect(
        runMatchExp(output, new C({foo: 1, bar: 'bar', baz: 'baz'})),
      ).toStrictEqual({baz: 'baz'});
      expect(runMatchExp(output, {foo: 1, bar: 'bar'})).toBe(1);
      expect(runMatchExp(output, null)).toBe(1);
    });

    test('nested', async () => {
      const code = `
        const e = match (x) {
          'a' | 'b' => match (x) {
            'a' => 0,
            const a => a,
          },
          const a => a,
        };
      `;
      const output = await transform(code);
      expect(output).toMatchInlineSnapshot(`
        "const e = ($$gen$m0 => {
          if ($$gen$m0 === 'a' || $$gen$m0 === 'b') {
            return ($$gen$m1 => {
              if ($$gen$m1 === 'a') {
                return 0;
              }

              {
                const a = $$gen$m1;
                return a;
              }
            })(x);
          }

          {
            const a = $$gen$m0;
            return a;
          }
        })(x);"
      `);

      expect(runMatchExp(output, 'a')).toBe(0);
      expect(runMatchExp(output, 'b')).toBe('b');
      expect(runMatchExp(output, 'xxx')).toBe('xxx');
    });
  });
});
