import * as React from 'react';

function Foo(): React.Node { return 0 }
<Foo />;

'' as SVGMatrix; // OK
1 as SVGMatrix; // ERROR
