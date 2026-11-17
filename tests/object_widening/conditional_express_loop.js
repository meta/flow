//@flow
declare function reduce<T>(fn: (x: T) => T): any;

reduce(
    (newItem) =>
  true ? {...newItem} : newItem // Ok!
);
