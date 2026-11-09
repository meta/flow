import * as React from 'react';

type ListProps<T> = {itemData?: T, itemCount: number};
declare class List<P> extends React.PureComponent<P> {}
declare class FixedSizeList<T = unknown> extends List<ListProps<T>> {}
declare var listRef: {current: FixedSizeList<unknown> | null};
declare var items: Array<string>;

// With `React.ReactManagedAttributes`, the config of a class component has `ref` for its
// instance: the invariant instance gives `T` the type `unknown`.
<FixedSizeList itemCount={1} itemData={items} ref={listRef} />; // ok
