declare class Loaded<T> {
  value: T;
}

type Getter<Args, Data, Source> = {
  get: (args: Args, source: Source) => Data,
  source: Source,
};

declare function query<Args, Data, Source>(
  getter: Getter<Args, Loaded<Data>, Source> | Getter<Args, Data, Source>,
): Array<Data>;
declare class Source {
  isLoaded(): boolean;
}
declare const source: Source;

const q = query({
  get: (_id: string, source) => source.isLoaded(),
  source,
});
