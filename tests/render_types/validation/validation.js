import * as React from 'react';

declare component Foo();
component Bar<T>() {return 0}
declare function Baz(): React.Node;

type RGood0 = renders 'svg'; // ok
type RBadDeprecated = renders React.MixedElement; // error
type RBadNumber = renders 1; // error
type RBadNode = renders React.Node; // error
type RBadError = renders Error; // error

component GoodComponentRenders() renders Foo {return <Foo />} // ok

component PermittedGenericRenders1<T extends React.Node>(children: T) renders T { return children } // ok
component PermittedGenericRenders2<T extends React.Node>(children: T) renders (T | T) { return children } // ok
component PermittedGenericRenders3<T extends React.Node>(children: T) renders? T { return children } // ok
component BannedGenericRenders1<T extends Error>(children: T) renders? T { return children } // error
component BannedGenericRenders2<T extends React.Node>(children: T) renders? (T | GoodComponentRenders) { return children } // error
type AllowedGenericRenders<T extends React.Node> = renders T; // ok

type BadSpecificRenders1 = renders (false | null | undefined); // error
type BadSpecificRenders2 = renders (Array<Foo>); // error
type BadSpecificRenders3 = renders (ReadonlyArray<Foo>); // error
type BadSpecificRenders4 = renders (Iterable<Foo>); // error
type BadSpecificRenders5 = renders (React.ChildrenArray<Foo>); // error
type BadSpecificRenders6 = renders (React.ChildrenArray<Foo | null>); // error

type BadUnion = renders (Bar | typeof Baz); // error

type BadStructuralComponent = renders (component() renders number); // error

// Showing why generic renders should be allowed everywhere
// If it's not allowed everywhere, then we have to make the hook return annotation
// to be an exact react element.
{
  hook useTransparent<T extends React.Node>(n: T): renders T { // ok
    return <>{n}</>;
  }
  component GenericRenders<T extends React.Node>(n: T) renders T {
    return useTransparent(n); // ok
  }
}
