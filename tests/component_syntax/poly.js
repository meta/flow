import * as React from 'react';

export component Poly<T>(foo: T, bar: [T], ...props: { baz?: (x: T) => void}) {
    return null;
}

<Poly foo={1} bar={[2]} />;
<Poly foo={"a"} bar={["a"]} />;
<Poly foo={1} bar={["a"]} />;
<Poly foo={1} bar={[1]} baz={(x: string) => {}} />; //error

declare const x: Poly<number>;
x as Poly<string>;

declare const C: React.ComponentType<{foo: string}>;
const el = <C foo="str" />;
