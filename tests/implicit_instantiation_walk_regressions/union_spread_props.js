import * as React from 'react';

// A spread of a union makes a union of objects. Each member has to accept the props: the
// first member says nothing about `V` and does not take a string `value`.
type Standalone = Readonly<{pressed?: boolean, value?: void}>;
type Grouped<V> = Readonly<{value: V}>;
type ToggleProps<V> = Readonly<{...Standalone | Grouped<V>, label: string}>;

component Toggle<V = void>(...{label, ...props}: ToggleProps<V>) {
  return null;
}

<Toggle label="x" value="A" />; // ok: V = 'A'
<Toggle label="y" pressed={true} />; // ok: V = void

type IconProps =
  | Readonly<{icon?: string, kind?: empty}>
  | Readonly<{icon: string, kind: 'picture'}>;
type SelectProps<V> = Readonly<{
  ...IconProps,
  onChange: (newValue: V) => void,
  value?: ?V,
}>;

component Select<V>(...props: SelectProps<V>) {
  return null;
}

declare const setKey: ((('a' | 'b') => 'a' | 'b') | 'a' | 'b') => void;
declare const key: 'a' | 'b';

<Select onChange={setKey} value={key} />; // ok: V = 'a' | 'b'
