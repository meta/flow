//@flow
export type Value = void | null | boolean | number | string | Aggregate;

export type Aggregate = {
  readonly [key: string]: Value,
};

(() => {}) as Value;
