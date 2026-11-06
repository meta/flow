// @flow

// The receiver of a member access can be a type destructor rather than a plain
// object or instance. Expanding its members forces the destructor regardless of
// the normalizer's `evaluate_type_destructors` setting, so the property's kind
// still comes out. The qualifier, on the other hand, is read off the receiver
// left *unevaluated*, so it is there only when that form is still named: an
// alias over the destructor, or the operand the destructor normalizes back to.

type Base = {
  p: number,
  m(): string,
  get g(): boolean,
  set s(v: boolean): void,
};

declare var partial: Partial<Base>;
// The receiver of a member access can itself be a type destructor. Expanding its
// members forces evaluation, so the property's kind is still recovered; the
// qualifier comes off the unevaluated receiver.
partial.p;
//      ^
partial.m();
//      ^

type NamedPartial = Partial<Base>;
declare var named: NamedPartial;
named.p;
//    ^
named.m();
//    ^

declare var readOnly: Readonly<Base>;
readOnly.m();
//       ^
readOnly.g;
//       ^

declare var nonMaybe: NonNullable<?Base>;
nonMaybe.p;
//       ^
nonMaybe.m();
//       ^
nonMaybe.g;
//       ^
nonMaybe.s = true;
//       ^

declare var spread: {...Base, extra: string};
spread.p;
//     ^
spread.m();
//     ^

declare var mapped: {[K in keyof Base]: Base[K]};
mapped.p;
//     ^

declare var conditional: (string extends string ? Base : empty);
conditional.p;
//          ^
conditional.m();
//          ^
