/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @noflow
 * @format
 */

const assert = require('node:assert/strict');
const {test} = require('node:test');
const Module = require('node:module');
const path = require('node:path');
const {transformFileSync} = require('@babel/core');
const React = require('react');
const {renderToStaticMarkup} = require('react-dom/server');

function loadComponent(filename, exportName = 'default', overrides = {}) {
  const compiled = new Module(filename, module);
  compiled.paths = Module._nodeModulePaths(path.dirname(filename));
  const originalRequire = compiled.require.bind(compiled);
  compiled.require = name => {
    if (Object.hasOwn(overrides, name)) return overrides[name];
    if (name.endsWith('.css')) return {};
    if (name === 'react-json-view') return () => null;
    if (name === './clipboard') return () => Promise.resolve(true);
    if (name === './formatAstForDisplay') {
      return loadComponent(path.resolve(path.dirname(filename), `${name}.js`));
    }
    return originalRequire(name);
  };
  compiled._compile(
    transformFileSync(filename, {
      configFile: false,
      babelrc: false,
      presets: ['@babel/preset-flow', '@babel/preset-react'],
      plugins: [
        'babel-plugin-syntax-hermes-parser',
        '@babel/plugin-transform-modules-commonjs',
      ],
      parserOpts: {flow: 'all'},
    }).code,
    filename,
  );
  return compiled.exports[exportName];
}

const TryFlowResults = loadComponent(
  path.resolve(__dirname, '../src/try-flow/TryFlowResults.js'),
);

function render(overrides = {}) {
  return renderToStaticMarkup(
    React.createElement(TryFlowResults, {
      flowVersion: 'master',
      flowVersions: ['master', 'v0.300.0'],
      changeFlowVersion() {},
      loading: false,
      errors: [],
      internalError: '',
      cursorPosition: null,
      ast: {},
      onErrorSelect() {},
      ...overrides,
    }),
  );
}

// JSON is an error format, not a third top-level tab; Rich remains selected.
test('starts with Errors and AST tabs and Rich/JSON format pills', () => {
  const html = render();
  assert.match(html, /aria-pressed="true"[^>]*>.*Errors<\/button>/); // OK
  assert.match(html, /aria-pressed="false"[^>]*>.*AST<\/button>/); // OK
  assert.match(html, /role="group" aria-label="Error format"/); // OK
  assert.doesNotMatch(html, /<select aria-label="Error format"/); // OK
  assert.match(html, /aria-pressed="true"[^>]*>Rich<\/button>/); // OK
  assert.match(html, /aria-pressed="false"[^>]*>JSON<\/button>/); // OK
  assert.match(html, /No errors/); // OK
});

// The existing error cards, count, and editor-navigation targets remain intact.
test('renders error cards by default', () => {
  const html = render({
    errors: [
      {
        level: 'error',
        error_codes: ['incompatible-type'],
        message: [
          {
            loc: {
              source: '-',
              start: {line: 1, column: 1},
              end: {line: 1, column: 5},
            },
            context: 'value;',
            descr: 'Example diagnostic.',
          },
        ],
      },
    ],
  });
  assert.match(html, /Example diagnostic\./); // OK
  assert.match(html, /incompatible-type/); // OK
  assert.match(html, /aria-label="Go to error at line 1, column 1"/); // OK
  assert.match(html, /title="Copy all errors"/); // OK
  assert.match(html, /title="Copy this error"/); // OK
});

// Loading and internal errors must not look like a successful empty check.
test('distinguishes loading and internal failures from no errors', () => {
  assert.doesNotMatch(render({loading: true}), /No errors/); // OK
  const html = render({internalError: 'Example internal failure'});
  assert.match(html, /TryFlow encountered an internal error:/); // OK
  assert.match(html, /Example internal failure/); // OK
  assert.doesNotMatch(html, /No errors/); // OK
});

const AstResult = loadComponent(
  path.resolve(__dirname, '../src/try-flow/TryFlowResults.js'),
  'AstResult',
);

function renderAst(overrides = {}) {
  return renderToStaticMarkup(
    React.createElement(AstResult, {
      ast: {},
      format: 'tree',
      hideLocations: true,
      hideEmptyFields: true,
      cursorPosition: null,
      ...overrides,
    }),
  );
}

// JSON uses the same display filters as the tree, without changing the original AST.
test('serializes the filtered AST as indented JSON', () => {
  const loc = {source: 'example.js', start: {line: 1, column: 0}};
  const ast = {
    body: [{name: 'x', loc, optional: null, type: 'Identifier'}],
    loc,
    range: [0, 1],
    type: 'Program',
  };
  const original = JSON.stringify(ast);
  const expected = {type: 'Program', body: [{type: 'Identifier', name: 'x'}]};
  assert.equal(
    renderAst({ast, format: 'json'}),
    renderToStaticMarkup(
      React.createElement('pre', null, JSON.stringify(expected, null, 2)),
    ),
  ); // OK
  assert.equal(JSON.stringify(ast), original); // OK
  const shown = renderAst({
    ast,
    format: 'json',
    hideLocations: false,
    hideEmptyFields: false,
  });
  assert.match(shown, /&quot;loc&quot;:/); // OK
  assert.match(shown, /&quot;range&quot;:/); // OK
  assert.match(shown, /&quot;optional&quot;: null/); // OK
  assert.doesNotMatch(shown, /example\.js/); // OK
});

// Empty trees and parser/unsupported-version messages stay meaningful in either format.
test('preserves empty and string AST results in Tree and JSON modes', () => {
  for (const format of ['tree', 'json']) {
    assert.match(renderAst({format}), /No AST to show\./); // OK
    const ast = 'AST output is not supported in this version of Flow.';
    assert.equal(
      renderAst({ast, format}),
      renderToStaticMarkup(React.createElement('pre', null, ast)),
    ); // OK
  }
});

function findElement(element, predicate) {
  if (!React.isValidElement(element)) return undefined;
  if (predicate(element)) return element;
  for (const child of React.Children.toArray(element.props.children)) {
    const found = findElement(child, predicate);
    if (found != null) return found;
  }
  return undefined;
}

function textContent(element) {
  if (typeof element === 'string' || typeof element === 'number') {
    return String(element);
  }
  if (!React.isValidElement(element)) return '';
  return React.Children.toArray(element.props.children)
    .map(textContent)
    .join('');
}

function findButton(element, text) {
  return findElement(
    element,
    node => node.type === 'button' && textContent(node) === text,
  );
}

function createResultsRenderer(
  overrides = {},
  copy = () => Promise.resolve(true),
) {
  const state = [];
  let hookIndex = 0;
  const Component = loadComponent(
    path.resolve(__dirname, '../src/try-flow/TryFlowResults.js'),
    'default',
    {
      react: {
        ...React,
        useState(initial) {
          const index = hookIndex++;
          if (!(index in state)) state[index] = initial;
          return [
            state[index],
            value => {
              state[index] =
                typeof value === 'function' ? value(state[index]) : value;
            },
          ];
        },
      },
      './clipboard': copy,
    },
  );
  const props = {
    flowVersion: 'master',
    flowVersions: ['master'],
    changeFlowVersion() {},
    loading: false,
    errors: [],
    internalError: '',
    cursorPosition: null,
    ast: {type: 'Program', body: []},
    onErrorSelect() {},
    ...overrides,
  };
  return {
    render() {
      hookIndex = 0;
      return Component(props);
    },
  };
}

function showAstJSON(renderer) {
  findButton(renderer.render(), 'AST').props.onClick({stopPropagation() {}});
  findButton(renderer.render(), 'JSON').props.onClick();
  return renderer.render();
}

// Copy JSON belongs only to the AST JSON toolbar, immediately before the filters.
test('places Copy JSON before the AST hide options only in JSON mode', () => {
  const renderer = createResultsRenderer();
  assert.equal(findButton(renderer.render(), 'Copy JSON'), undefined); // OK
  findButton(renderer.render(), 'AST').props.onClick({stopPropagation() {}});
  assert.equal(findButton(renderer.render(), 'Copy JSON'), undefined); // OK
  findButton(renderer.render(), 'JSON').props.onClick();
  const tree = renderer.render();
  assert.equal(findButton(tree, 'Copy JSON').props.disabled, false); // OK
  const html = renderToStaticMarkup(tree);
  assert.ok(
    html.indexOf('title="Copy JSON"') <
      html.indexOf('aria-label="Hide locations"'),
  ); // OK
  findButton(tree, 'Tree').props.onClick();
  assert.equal(findButton(renderer.render(), 'Copy JSON'), undefined); // OK
});

// The clipboard must receive exactly the rendered JSON for every display-filter combination.
test('copies the displayed AST JSON with the active hide options', async () => {
  const loc = {source: 'example.js', start: {line: 1, column: 0}};
  const ast = {
    body: [{name: 'x', loc, optional: null, type: 'Identifier'}],
    loc,
    range: [0, 1],
    type: 'Program',
  };
  const original = JSON.stringify(ast);
  const copied = [];
  const renderer = createResultsRenderer({ast}, async text => {
    copied.push(text);
    return true;
  });
  showAstJSON(renderer);
  for (const hideLocations of [true, false]) {
    for (const hideEmptyFields of [true, false]) {
      let tree = renderer.render();
      findElement(
        tree,
        node => node.props['aria-label'] === 'Hide locations',
      ).props.onChange({currentTarget: {checked: hideLocations}});
      findElement(
        tree,
        node => node.props['aria-label'] === 'Hide empty fields',
      ).props.onChange({currentTarget: {checked: hideEmptyFields}});
      tree = renderer.render();
      findElement(
        tree,
        node => node.props.title === 'Copy JSON',
      ).props.onClick();
      await Promise.resolve();
      const text = copied.at(-1);
      const output = findElement(tree, node => node.props.ast === ast);
      assert.equal(
        renderToStaticMarkup(output),
        renderToStaticMarkup(React.createElement('pre', null, text)),
      ); // OK
      const json = JSON.parse(text);
      assert.equal(Object.hasOwn(json, 'loc'), !hideLocations); // OK
      assert.equal(Object.hasOwn(json.body[0], 'optional'), !hideEmptyFields); // OK
      assert.doesNotMatch(text, /example\.js/); // OK
      assert.equal(JSON.stringify(ast), original); // OK
    }
  }
  assert.equal(copied.length, 4); // OK
});

// Loading, missing ASTs, and explanatory text have no JSON output to copy.
test('disables Copy JSON when JSON output is unavailable', () => {
  for (const props of [
    {loading: true},
    {ast: {}},
    {ast: 'AST output is not supported in this version of Flow.'},
  ]) {
    const renderer = createResultsRenderer(props);
    const tree = showAstJSON(renderer);
    assert.equal(findButton(tree, 'Copy JSON').props.disabled, true); // OK
  }
});

// Serialization may produce no text; never pass it to the clipboard or confirm success.
test('does not copy an AST whose JSON serialization is unavailable', async () => {
  const copied = [];
  const renderer = createResultsRenderer(
    {
      ast: {
        type: 'Program',
        toJSON() {},
      },
    },
    async text => {
      copied.push(text);
      return true;
    },
  );
  findButton(showAstJSON(renderer), 'Copy JSON').props.onClick();
  await Promise.resolve();
  assert.deepEqual(copied, []); // OK
  assert.equal(findButton(renderer.render(), 'Copied!'), undefined); // OK
});

// A pending or failed clipboard write must not show a success confirmation.
test('shows Copied only after a successful JSON clipboard write', async () => {
  let finishCopy;
  const renderer = createResultsRenderer(
    {},
    () =>
      new Promise(resolve => {
        finishCopy = resolve;
      }),
  );
  let tree = showAstJSON(renderer);
  findButton(tree, 'Copy JSON').props.onClick();
  assert.equal(findButton(renderer.render(), 'Copied!'), undefined); // OK
  finishCopy(false);
  await Promise.resolve();
  tree = renderer.render();
  assert.equal(findButton(tree, 'Copied!'), undefined); // OK
  findButton(tree, 'Copy JSON').props.onClick();
  finishCopy(true);
  await Promise.resolve();
  assert.ok(findButton(renderer.render(), 'Copied!')); // OK
});
