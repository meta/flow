type RenderProps<TF, TC> = {client?: TC, data?: TF};
type Base<TKey extends string, TF, TC> = {
  fallback?: () => void,
  cached?: true,
  key: TKey,
  render: (props?: RenderProps<TF, TC>) => void,
};
type Area<TKey extends string, TF, TC> = {...Base<TKey, TF, TC>, area: string};
type Plugin<TKey extends string, TF, TC> = Base<TKey, TF, TC> | Area<TKey, TF, TC>;
type Resolver<TPlugin, TKey> = (key: TKey) => ?TPlugin;

declare function factory<TKey extends string, TF, TC>(
  resolver: ?Resolver<Readonly<Plugin<TKey, TF, TC>>, TKey>,
): TC;

type MyPlugin = {
  cached: true,
  key: 'a' | 'b',
  render: (props?: {client?: void, data?: string}) => void,
};

declare function getByMixed(key: unknown): ?MyPlugin;

// `Readonly` of an alias of a union: the union of the read-only members, whose fields are
// compared covariantly.
factory(getByMixed) as void; // ok
