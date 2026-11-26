import * as React from 'react';

declare const any: any;

class A extends React.Component<{foo: number, ...}, void> {}
class B extends React.Component<{foo: number, bar: number, ...}, void> {}
class C extends React.Component<{children: number, ...}, void> {}
class D extends React.Component<{children: Array<number>, ...}, void> {}
class E extends React.Component<{foo: number, bar: number, ...}, void> {
  static defaultProps: {bar: number, ...} = {bar: 42};
}

declare const a: ExactReactElement_DEPRECATED<Class<A>>;
declare const b: ExactReactElement_DEPRECATED<Class<B>>;
declare const c: ExactReactElement_DEPRECATED<Class<C>>;
declare const d: ExactReactElement_DEPRECATED<Class<D>>;
declare const e: ExactReactElement_DEPRECATED<Class<E>>;

React.cloneElement(); // Error: Needs a minimum of two arguments.
React.cloneElement('nope'); // Error: Not a valid element type.
React.cloneElement({ type: any }); // Error: Not a valid element type.
React.cloneElement(a); // OK: `a` is an element.

React.cloneElement(a) as ExactReactElement_DEPRECATED<Class<A>>; // OK
React.cloneElement(a) as ExactReactElement_DEPRECATED<Class<B>>; // Error: A ~> B

React.cloneElement(a, {}); // OK
React.cloneElement(a, undefined); // OK
React.cloneElement(a, null); // OK
React.cloneElement(a, {foo: 1}); // OK
React.cloneElement(a, {foo: 1, bar: 2}); // Error: `bar` is extra
React.cloneElement(a, {foo: '1'}); // Error: `foo` is a number.
React.cloneElement(b, {}); // OK
React.cloneElement(b, undefined); // OK
React.cloneElement(b, null); // OK
React.cloneElement(b, {foo: 1}); // OK
React.cloneElement(b, {foo: 1, bar: 2}); // OK
React.cloneElement(b, {foo: '1'}); // Error: `foo` is a number.

React.cloneElement(c, {}); // OK
React.cloneElement(c, undefined); // OK
React.cloneElement(c, null); // OK
React.cloneElement(c, {children: 42}); // OK
React.cloneElement(c, {children: '42'}); // Error: `children` is a number.
React.cloneElement(c, {}, 42); // OK
React.cloneElement(c, undefined, 42); // OK
React.cloneElement(c, null, 42); // OK
React.cloneElement(c, {}, 1, 2, 3); // Error: `children` is not an array.
React.cloneElement(c, undefined, 1, 2, 3); // Error: `children` is not an array.
React.cloneElement(c, null, 1, 2, 3); // Error: `children` is not an array.
React.cloneElement(c, {}, ...[]); // OK

React.cloneElement(d, {}); // OK
React.cloneElement(d, {children: 42}); // Error: `children` is an array.
React.cloneElement(d, {children: [1, 2, 3]}); // OK
React.cloneElement(d, {}, 42); // Error: `children` is an array.
React.cloneElement(d, undefined, 42); // Error: `children` is an array.
React.cloneElement(d, null, 42); // Error: `children` is an array.
React.cloneElement(d, {}, 1, 2, 3); // OK
React.cloneElement(d, undefined, 1, 2, 3); // OK
React.cloneElement(d, null, 1, 2, 3); // OK

React.cloneElement(e, {}); // OK
React.cloneElement(e, {foo: 1}); // OK
React.cloneElement(e, {foo: 1, bar: 2}); // OK
React.cloneElement(e, {foo: undefined, bar: 2});
React.cloneElement(e, {foo: 1, bar: undefined}); // OK: `bar` has a default.

function SFC(props: { p: number, ... }) { return null };
React.cloneElement(<SFC p={0} />, { p: "bad" }); // ok

// Exact
declare function Exact(x: {foo: number}): undefined;
declare const exact: ExactReactElement_DEPRECATED<typeof Exact>;
React.cloneElement(exact, {foo: 1}); // OK
React.cloneElement(exact, {foo: 1, bar: 2}); // ERROR

// Clone typeof element
type CompProps = Readonly<{
  foo: string,
  bar: string,
}>;
{
  declare function Comp(x: CompProps): ExactReactElement_DEPRECATED<'div'>;
  declare const el: ExactReactElement_DEPRECATED<typeof Comp>;
  React.cloneElement(el, {foo: 'hi'}); // OK
}

// MixedElement
{
  declare const el: React.MixedElement;
  React.cloneElement(el); // OK
  React.cloneElement(el, {}); // OK
}

// Node
{
  declare const el: React.Node;
  React.cloneElement(el); // ERROR
  React.cloneElement(el, {}); // ERROR
}

// Cloned element is a union
function cloneUnionElement() {
  declare class A extends React.Component<{foo: number, ...}, void> {}
  declare component B(...props: { foo: number, ... });

  declare const element:
    | ExactReactElement_DEPRECATED<Class<A>>
    | ExactReactElement_DEPRECATED<typeof B>;

  React.cloneElement(element); // OK
  React.cloneElement(element, {foo: 1}); // OK

  type Wrap<T> = { f: T, ... }['f'];

  declare const wrappedElement: Wrap<
    | ExactReactElement_DEPRECATED<Class<A>>
    | ExactReactElement_DEPRECATED<typeof B>
  >;

  // Tests that wrapping does not affect result
  React.cloneElement(wrappedElement); // OK
  React.cloneElement(wrappedElement, {foo: 1}); // OK
}
