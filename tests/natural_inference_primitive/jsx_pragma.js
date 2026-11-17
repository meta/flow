// @jsx createElement

declare export function createElement<TProps extends {...}>(
  Component: (props: TProps) => void,
  props: NoInfer<?TProps>,
): Readonly<{
  props: {[propName: string]: unknown, ...},
}>;

declare const C: (props: Readonly<{foo: Array<{bar: 'BAZ'}>}>) => void;
const c1 = <C foo={[{bar: 'BAZ'}]} />; // okay
const c2 = <C foo={[{bar: 'BAK'}]} />; // error 'BAZ' != 'BAK'
const c3 = <C {...{foo: [{bar: 'BAZ'}]}} />; // okay

declare const D: (props: Readonly<{foo: Array<{bar: string}>}>) => void;
const d1 = <D foo={[{bar: 'BAZ'}]} />; // okay
const d2 = <D foo={[{bar: 'BAK'}]} />; // okay
