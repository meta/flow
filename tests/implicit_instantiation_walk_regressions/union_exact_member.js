declare class ReadOnlyCell<out T> {
  key: string;
}
declare class Cell<T> extends ReadOnlyCell<T> {}

type CellValue<T> = ReadOnlyCell<T> | Cell<T>;
type Settings<Stored> = Readonly<{
  kind: string,
  validate: (unknown) => Stored,
}>;
type OptionsWithoutDefault<T> = Readonly<{
  key: string,
  settings?: Settings<T>,
}>;
type OptionsWithDefault<T> = Readonly<{
  ...OptionsWithoutDefault<T>,
  default: CellValue<T> | Promise<T> | T,
}>;
type Options<T> = OptionsWithDefault<T> | OptionsWithoutDefault<T>;

declare function cell<T>(options: Options<T>): Cell<T>;
declare function numberSettings(): Settings<number>;
declare const seed: Cell<?number>;

export const chosen: Cell<?number> = cell({
  key: 'k',
  default: seed,
  settings: numberSettings(),
});
