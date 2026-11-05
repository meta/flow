/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import {translateFlowDefToTSDef} from '../../src';

describe('React import tracking for type translation', () => {
  // JSX-qualified type references require an import even without other React types.
  test.each(['React.MixedElement', 'React$MixedElement'])(
    'adds the React import for %s',
    async source => {
      const result = await translateFlowDefToTSDef(`type T = ${source};`, {
        singleQuote: true,
      });
      expect(result).toContain("import * as React from 'react';"); // OK
      expect(result).toContain('type T = React.JSX.Element;'); // OK
    },
  );

  // An existing namespace import must not be duplicated.
  test('reuses an existing React import', async () => {
    const result = await translateFlowDefToTSDef(
      "import * as React from 'react'; type T = React.MixedElement;",
      {singleQuote: true},
    );
    expect(result.split("import * as React from 'react';")).toHaveLength(2); // OK
    expect(result).toContain('type T = React.JSX.Element;'); // OK
  });
});
