import * as React from 'react';

declare component Link();
declare component Tip<T extends React.Node = React.Node>(
  label: string,
  ...props: {children: renders T, disabled?: boolean}
);
declare const cond: boolean;

const content = cond ? <Link /> : 'text';
const a = <Tip label="x">{content}</Tip>;
let b: ?React.MixedElement = null;
if (cond) {
  b = <Link />;
}
if (b != null) {
  b = <Tip label="x">{b}</Tip>;
}
