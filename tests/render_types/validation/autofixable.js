// @flow

import * as React from 'react';

component Foo() {return 0}

type RBad1 = renders (Foo | null | undefined | false);
type RBad2 = renders (Foo | Array<Foo> | Foo[] | ReadonlyArray<Foo> | Set<Foo> | Iterable<Foo>);
type RBad3 = renders (Foo | Array<Foo> | null);
type RBad4 = renders Array<Foo>;
type RBad5 = renders? (Foo | null);
type RBad6 = renders? ?Foo;
type RBad7 = renders? Array<Foo>;
type RBad8 = renders* Array<Foo>;
type RBad9 = renders* (Foo | null);
type RBad10 = renders (Foo | null | 1);
