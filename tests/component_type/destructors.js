//@flow

const React = require('react');

declare const C: component(bar: number, foo?: number, ref?: React.RefSetter<undefined>, ...{...});

// Create element tests props_to_tin
const _a = <C bar={3} />;
const _b = <C baz={3} />; // Error, bar missing

// ElementRef tests get_instance

undefined as React.ElementRef<typeof C>;
3 as React.ElementRef<typeof C>; // Error, 3 is not undefined.

({foo: 3, bar: 3}) as React.ComponentProps<typeof C>;
({bar: 3}) as React.ComponentProps<typeof C>;
({foo: 3, bar: 3, baz: 3}) as React.ComponentProps<typeof C>;
({baz: 3}) as React.ComponentProps<typeof C>; // Error, bar missing
