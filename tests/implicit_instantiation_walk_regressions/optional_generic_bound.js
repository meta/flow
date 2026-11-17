// A generic flows to a maybe or optional type narrowed, as subtyping narrows it: the members of
// its bound that the type absorbs are dropped, and the inner type gets the generic with the rest.
type Color = 'blue' | 'red';
declare function inner<T extends Color>(props: Readonly<{color?: T}>): void;
declare function innerMaybe<T extends Color>(color: ?T): void;

function outer<TColor extends Color | void>(props: Readonly<{color?: TColor}>) {
  inner(props); // was an error
}

function outerMaybe<TColor extends Color | null | void>(color: TColor) {
  innerMaybe(color); // was an error
}

// An optional type does not absorb `null`.
function outerNull<TColor extends Color | null>(props: Readonly<{color?: TColor}>) {
  inner(props); // error
}
