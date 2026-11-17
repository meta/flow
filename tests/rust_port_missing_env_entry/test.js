// Regression test for missing location entry in type_env.
// Without the with_debug_exn catch_unwind fix, Flow's Rust port panics
// with "Missing location entry" when private fields are accessed inside
// generic methods of a class with mapped-type generic constraints.

type ItemDef = Readonly<{
  [key: string]: Readonly<{
    value: unknown,
    isAvailable?: (...args: ReadonlyArray<empty>) => boolean,
  }>,
}>;

class Registry<TItems extends ItemDef = {}> {
  #items: {
    [Key in NonNullable<keyof TItems>]?: ReadonlyArray<TItems[Key]>,
  } = {};

  add<TKey extends keyof TItems>(
    key: TKey,
    ...values: ReadonlyArray<TItems[TKey]>
  ): void {
    this.#items[key] = [].concat(this.#getItems(key), values);
  }

  getAll<TKey extends keyof TItems>(
    key: TKey,
    ...context: TItems[TKey] extends {
      readonly isAvailable: (...args: infer TArg) => boolean,
      ...
    }
      ? TArg
      : []
  ): Array<TItems[TKey]['value']> {
    return this.#getItems(key)
      .filter(item => item.isAvailable?.(...context) ?? true)
      .map(item => item.value);
  }

  #getItems<TKey extends keyof TItems>(
    key: TKey,
  ): ReadonlyArray<TItems[TKey]> {
    return this.#items[key] ?? [];
  }
}

module.exports = Registry;
