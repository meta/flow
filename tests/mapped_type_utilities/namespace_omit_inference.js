import actionNamespace from './action_namespace';

actionNamespace.tools.action(
  {a: 1},
  ({a, b}) => b,
) as (x: {b: string}) => string; // ERROR
