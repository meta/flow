// regression test for order sentitive post inference check

import {InitialState, MOCK_UI_STATE} from './let_var_widen_with_spread_lib';

let newState;

newState = InitialState;

newState = {
  a: {},
  c: {},
  b: {
    ...MOCK_UI_STATE.b,
    fetching: true,
  },
}; // error

newState = MOCK_UI_STATE; // error
