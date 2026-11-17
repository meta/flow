// A symbol index signature whose key is written behind a type alias
// application, in a module of its own, so that `keyof DeferredDict` in another
// file is built by signature inference rather than by the checker.

declare export const s: unique symbol;

export type Id<T> = T;

export type DeferredDict = {[k: Id<typeof s>]: number};

declare export const definitions: {
  str: number,
  [s]: string,
};

export type DefinitionKey = keyof typeof definitions;
export type DictKeyedByKeySet = {[key: DefinitionKey]: boolean};
