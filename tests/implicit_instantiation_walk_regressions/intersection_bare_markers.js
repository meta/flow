type Store = {s: number};
type Extra = {progress: string};

declare function connect<Own, State, Dispatch>(
  Component: component(...Own & State & Dispatch & Extra),
  mapParams: (Store, Own) => Array<string>,
  mapState: (Store, Own) => State,
  mapDispatch: (string, Own) => Dispatch,
): void;

type Own = {id: string, ...};
type State = {items: Array<string>};
type Dispatch = {};

declare component Item(...props: {...Own, ...State, ...Dispatch, ...Extra});

connect(
  Item,
  (_s: Store, own: Own) => [own.id],
  (_s: Store, _own: Own): State => ({items: []}),
  (_d: string, _own: Own): Dispatch => ({}),
);
