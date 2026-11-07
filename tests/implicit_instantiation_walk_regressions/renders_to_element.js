// A render type leaves the renders world as subtyping does: a `renders C` of a component is a
// `React.MixedElement`, so the overload of `cloneElement` for intrinsics does not fit it.
import * as React from 'react';

component Button() { return null; }
component Group() { return null; }

declare const buttons: ReadonlyArray<renders (Button | Group)>;
buttons.map((b, i) => React.cloneElement(b, {key: String(i)}));
