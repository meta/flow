import {withDefaults} from './callback_writes_helper';

declare function describe(name: string, cb: () => void): void;
declare function it(name: string, cb: () => void): void;

describe('suite', () => {
  let initialState;

  it('first', () => {
    initialState = {
      field: {
        data: 'x',
      },
    };
  });

  it('second', () => {
    initialState = {
      field: {
        data: undefined, // ERROR
      },
    };
  });
});

withDefaults(
  {a: 1},
  ({a, b}) => () => b,
) as (args: {b: string}) => () => string; // ERROR
