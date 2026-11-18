declare function mergedFunction<T>(value: T): T;

declare namespace mergedFunction {
  type Identity<T> = T;
  export function member<T>(value: T): Identity<T>;
  export type NumberResult = Identity<number>;
}

// A later block sees the exports of earlier blocks
declare namespace mergedFunction {
  export function other(): NumberResult;
}

declare class MergedClass {}
declare namespace MergedClass {
  export type A = number;
}
declare namespace MergedClass {
  export type B = A;
}
