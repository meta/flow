/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import {t, transform} from '../test_codemods/test-utils';
import * as prettier from 'prettier';

const [prettierMajor, prettierMinor] = prettier.version.split('.').map(Number);
const bodyCommentBlankLine =
  prettierMajor > 3 || (prettierMajor === 3 && prettierMinor >= 10) ? '\n' : '';

describe('function parameter spacing', () => {
  function codemod(code: string) {
    return transform(code, context => ({
      FunctionExpression(node) {
        context.replaceNode(
          node,
          t.ArrowFunctionExpression({
            async: node.async,
            body: node.body,
            params: node.params,
            predicate: node.predicate,
            returnType: node.returnType,
            typeParameters: node.typeParameters,
          }),
        );
      },
    }));
  }

  it('should not add newlines between params that fit on one line', async () => {
    const result = await codemod(`\
/**
 */

const foo = function foo(a, b, c, d, e, f) {
  // body


  // the above double empty line should be collapsed
};
`);
    expect(result).toBe(`\
/**
 */

const foo = (a, b, c, d, e, f) => {
  // body
${bodyCommentBlankLine}  // the above double empty line should be collapsed
};
`);
  });

  it("should not add newlines between params that don't fit on one line", async () => {
    const result = await codemod(`\
/**
 */

const foo = function foo(a, b, c, d, e, f, a_really_long_param, that_forces_this_onto_a_new_line) {
  // body


  // the above double empty line should be collapsed
};
`);
    expect(result).toBe(`\
/**
 */

const foo = (
  a,
  b,
  c,
  d,
  e,
  f,
  a_really_long_param,
  that_forces_this_onto_a_new_line,
) => {
  // body
${bodyCommentBlankLine}  // the above double empty line should be collapsed
};
`);
  });
});

describe('call argument spacing', () => {
  function codemod(code: string) {
    return transform(code, context => ({
      NewExpression(node) {
        context.replaceNode(
          node,
          t.CallExpression({
            callee: node.callee,
            arguments: node.arguments,
          }),
        );
      },
    }));
  }

  it('should not add newlines between arguments that fit on one line', async () => {
    const result = await codemod(`\
/**
 */

throw new Foo(
  a,
  b,
  c,
  d,
  e,
  f,
);
`);
    expect(result).toBe(`\
/**
 */

throw Foo(a, b, c, d, e, f);
`);
  });

  it("should not add newlines between arguments that don't fit on one line", async () => {
    const result = await codemod(`\
/**
 */

throw new Foo(a, b, c, d, e, f, a_really_long_param, that_forces_this_onto_a_new_line);
`);
    expect(result).toBe(`\
/**
 */

throw Foo(
  a,
  b,
  c,
  d,
  e,
  f,
  a_really_long_param,
  that_forces_this_onto_a_new_line,
);
`);
  });
});

describe('call with function spacing', () => {
  function codemod(code: string) {
    return transform(code, context => ({
      CallExpression(node) {
        if (
          node.arguments.length === 0 ||
          node.arguments[0].type !== 'FunctionExpression'
        ) {
          return;
        }
        const callback = node.arguments[0];
        context.modifyNodeInPlace(node, {
          arguments: [
            t.ArrowFunctionExpression({
              async: callback.async,
              body: callback.body,
              params: callback.params,
              predicate: callback.predicate,
              returnType: callback.returnType,
              typeParameters: callback.typeParameters,
            }),
          ],
        });
      },
    }));
  }

  it('should not add newlines between params that fit on one line within a call expression', async () => {
    const result = await codemod(`\
/**
 */

const foo = [].forEach(function foo(a, b, c) {
  // body


  // the above double empty line should be collapsed
});
`);
    expect(result).toBe(`\
/**
 */

const foo = [].forEach((a, b, c) => {
  // body
${bodyCommentBlankLine}  // the above double empty line should be collapsed
});
`);
  });

  // Long callback parameters wrap without adding blank lines between them.
  it("should not add newlines between params that don't fit on one line within a call expression", async () => {
    const result = await codemod(`\
/**
 */

const foo = [].forEach(function foo(a, b, long, enough, to, _break, out, of, the, call, but, not, that, _function) {
  // body


  // the above double empty line should be collapsed
});
`);
    expect(result).toBe(`\
/**
 */

const foo = [].forEach(
  (
    a,
    b,
    long,
    enough,
    to,
    _break,
    out,
    of,
    the,
    call,
    but,
    not,
    that,
    _function,
  ) => {
    // body
${bodyCommentBlankLine}    // the above double empty line should be collapsed
  },
);
`); // OK
  });
});

describe('class member spacing', () => {
  function codemod(code: string) {
    return transform(code, context => ({
      ClassDeclaration(node) {
        context.replaceNode(
          node,
          t.ClassDeclaration({
            body: node.body,
            id: t.Identifier({name: `${node.id?.name ?? 'Class'}_Replaced`}),
          }),
        );
      },
    }));
  }

  it('should not add or remove spaces between class members', async () => {
    const result = await codemod(`\
/**
 */

class Foo {
  a() {}
  b = 1;
  c = 2;
  replaceMe() {
    // body here
  }
}
class Bar {
  a() {}

  b = 1;

  c = 2;

  replaceMe() {
    // body here
  }
}
class Baz {
  a() {}

  b = 1;
  c = 2;

  replaceMe() {
    // body here
  }
}
`);
    expect(result).toBe(`\
/**
 */

class Foo_Replaced {
  a() {}
  b = 1;
  c = 2;
  replaceMe() {
    // body here
  }
}
class Bar_Replaced {
  a() {}

  b = 1;

  c = 2;

  replaceMe() {
    // body here
  }
}
class Baz_Replaced {
  a() {}

  b = 1;
  c = 2;

  replaceMe() {
    // body here
  }
}
`);
  });
});
