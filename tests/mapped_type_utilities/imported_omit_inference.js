import {action} from './action_helper';

action(
  {a: 1},
  ({a, b}) => b,
) as (x: {b: string}) => string; // ERROR
