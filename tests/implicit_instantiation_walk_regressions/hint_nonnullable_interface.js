function case1() {
  declare function parse<T>(text: ?string): null | NonNullable<T>;

  type Range = {start: number, end: number};

  declare const text: ?string;

  const a: ?Range = parse(text);
  const b: ?Range = parse<Range>(text);
}

function case2() {
  type Debounced<Args extends ReadonlyArray<unknown>> = {(...Args): void, ...};

  declare function debounce<Args extends ReadonlyArray<unknown>>(
    func: (...Args) => unknown,
  ): Debounced<Args>;

  interface Callback {
    (entries: Array<string>, other: number): void;
  }

  declare function observe(cb: Callback): void;

  observe(debounce(entries => {}));
}
