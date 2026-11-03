// @flow
//
// Cross-file: a `.js` object checked against an interface declared in
// `interface_extends_indexed_lib.ts`. The object is checked against the
// interface's indexed super like against any object type.

import type {InputProps} from './interface_extends_indexed_lib';

({label: 'x'}) as InputProps; // OK
({label: 'x', className: 'c', 'data-testid': 'a'}) as InputProps; // OK

({label: 'x', 'data-testid': 1}) as InputProps; // ERROR: number vs string
({label: 'x', className: 1}) as InputProps; // ERROR: number vs string
({}) as InputProps; // ERROR: missing `label`
