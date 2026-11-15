/*
 * @flow
 * @format
 */
// BENIGN test code - mimics real pattern in newtests/lsp/* tests
// Discovers all fixtures from __fixtures__ dir and adds them
import type {SuiteType} from '../Tester';
const {readdirSync} = require('fs');
const {join} = require('path');
const {suite, test} = require('../Tester');

module.exports = suite(({addFiles, startFlowServer}) => {
  const fixtures = readdirSync(join(__dirname, '__fixtures__')).map(file =>
    join('__fixtures__', file),
  );
  return [
    test('discovery adds all fixtures', [
      startFlowServer(),
      addFiles(...fixtures),
    ]),
  ];
}) as SuiteType;
