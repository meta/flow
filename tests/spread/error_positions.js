const React = require('react');

type F<T extends React.ElementType> = {
  apply: (cb: (props: React.ComponentProps<T>) => void) => void,
  component: T,
};

declare function foo<T extends React.ElementType>(x: T): F<T>;

declare function bar<P>(x: React.ComponentType<{ m: number, ...P, ...}>): React.ComponentType<P>; // error: P is not known to be an object

class C extends React.Component<{...}> {}

foo(bar(C)) as F<typeof C>; // error on call

declare function spread<T extends {...}>(x: T): {...T, ...{...}, ...}; // error should not appear here

declare const inexact: {foo: number, ...};
spread(inexact); // OK
