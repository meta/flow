// Regression test for using ground_eq to summarize solution of implicit instantiation

export type TRefFor<in TElement> =
  | {writeonly current: TElement | null, ...}
  | ((x: TElement | null) => unknown);

type RefType<TElement> = TRefFor<TElement>;

declare const ref1: { current: {...} | null };
declare const ref2: TRefFor<{...}>;

declare function mergeRefs<TElement>(
  ref1: RefType<TElement>,
  ref2: RefType<TElement>,
): void;

const mergedRefs = mergeRefs(ref1, ref2);
