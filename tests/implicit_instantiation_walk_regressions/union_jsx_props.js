import typeof ComboboxType from './union_jsx_props_combobox';
import type {IconProps, OptionProps} from './union_jsx_props_combobox';
import * as React from 'react';

// A spread of a union of objects makes the props of the element a union: each member goes to the
// config of the component on its own, so `TValue` gets the evidence of the other props.
declare component Field<TValue>(
  errorMessage?: ?string,
  ...rest: React.ComponentProps<ComboboxType<TValue>>
);

declare function handleChange(v: null | number | string): void;
declare const counts: ReadonlyArray<OptionProps<string>>;
declare const iconProps: IconProps;

<Field
  {...iconProps}
  label="l"
  onValueChange={handleChange}
  options={[{value: null, label: 'x'}, ...counts]}
  value="s"
/>; // ok
