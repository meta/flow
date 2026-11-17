declare export const options: {[key: string]: Value};

export opaque type Value: number = Values<
  // error: cyclic
  typeof options,
>;

declare const Results: {
  A: number,
  B: Result,
  ...
};

export type Result = Values<typeof Results>; // error: cyclic
