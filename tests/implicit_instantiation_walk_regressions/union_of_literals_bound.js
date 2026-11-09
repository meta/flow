// A union of literals, such as the result of a `match`, is kept precise where the bound of the
// type parameter needs it: `t` stays `'a' | 'b'` for `TType extends 'a' | 'b' | void`.
import * as React from 'react';

declare component Text<TType extends 'a' | 'b' | void = any>(type?: TType);
declare const k: 'x' | 'y';
const t = match (k) {
  'x' => 'a',
  'y' => 'b',
};
<Text type={t} />; // ok

// Also through a required parameter, where the flow-based solve widens `t` to `string`.
declare function req<T extends 'a' | 'b'>(x: T): T;
req(t); // ok
