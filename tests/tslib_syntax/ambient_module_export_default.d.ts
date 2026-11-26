declare module 'export-default-value' {
  const content: string;
  export default content;
}
declare module 'export-default-qualified' {
  namespace NS {
    export const v: number;
  }
  export default NS.v;
}
declare module 'export-default-class' {
  export default class C {
    x: number;
  }
}
declare module 'export-default-call' {
  function f(): number;
  export default f(); // ERROR
}
declare module 'export-default-method-body' {
  export default class {
    m(x: number) {} // ERROR
  }
}
declare module 'export-default-other-block' {
  export default content;
}
declare module 'export-default-other-block' {
  export const content: number;
}
declare module 'export-default-plain-class' {
  class Foo {
    x: number;
  }
  export default Foo;
}
declare module 'export-default-overloads' {
  export default function f(x: number): number;
  export default function f(x: string): string;
}
declare module 'export-default-duplicate' {
  const a: number;
  const b: string;
  export default a;
  export default b; // ERROR
}
declare module 'export-default-function-and-value' {
  const v: number;
  export default function f(x: string): string;
  export default v; // ERROR
}
declare module 'export-default-and-assignment' {
  const a: number;
  export default a;
  export = a; // ERROR
}
declare module 'export-default-heritage' {
  class D extends Base {}
  export default D;
}
declare module 'export-default-heritage' {
  export class Base {
    b: number;
  }
}
declare module 'export-default-implementation' {
  function g() { // ERROR
    return 1;
  }
  export default g;
}
