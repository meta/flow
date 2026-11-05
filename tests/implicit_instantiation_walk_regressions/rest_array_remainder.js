type Schedule = <TArgs extends Array<unknown>>(
  callback: (...args: TArgs) => unknown,
  ms?: ?number,
  ...args: TArgs
) => number;

declare const schedule: Schedule;

const handle: ?ReturnType<typeof schedule> = null;
