import * as React from 'react';

type Props<T> = {
  data: T,
  onClick?: (data: T) => void,
  disabled: boolean,
  ...
};

class MenuItem<T> extends React.PureComponent<Props<T>> {
  static defaultProps: {data: null, disabled: boolean} = {data: null, disabled: false};
}

declare function goToThread(data: {threadId: string, ...}): void;

// With `React.ReactManagedAttributes`, a default prop is optional in the config, with the type of
// the prop: its default value says nothing about `T`.
<MenuItem data={{threadId: 'a'}} onClick={goToThread} />; // ok
<MenuItem data={{threadId: 1}} onClick={goToThread} />; // error
