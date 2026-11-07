// The literals of a type parameter's solution stay precise when the bound of another type parameter
// that mentions it needs them.
type Surface = 'asset_details' | 'other';
type EntryPoint<in TParams, out TComponent> = Readonly<{
  getPreloadProps: (params: TParams) => {...},
  root: TComponent,
}>;

declare function loadEntryPoint<
  TParams extends {...},
  TComponent,
  TEntryPoint extends EntryPoint<TParams, TComponent>,
>(entryPoint: TEntryPoint, params: TParams): TComponent;

declare const ep: EntryPoint<Readonly<{surface: Surface, id: string}>, component(x: number)>;
loadEntryPoint(ep, {surface: 'asset_details', id: '1'});
loadEntryPoint(ep, {surface: 'nope', id: '1'}); // error
