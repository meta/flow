const obj = {
  foo: 'foo' as 'foo',
};

type T = typeof obj.foo;
//                  ^

type Id<X> = X;
type U = Id<typeof obj.foo>;
//                     ^

import * as ns from './helpers/exports_named';

type V = typeof ns.foo;
//                 ^
