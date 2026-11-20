//@flow

'use strict';

import type {Props} from './lib.js';

class Class {
  constructor(props: Props) {
    const inputProps = {
      ...props,
      foo: 'bar',
    };
  }
}

declare const x : {p: unknown};
//$FlowFixMe[unnecessary-optional-chain] should suppress
x?.p;
