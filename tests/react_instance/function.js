declare const any: any;

function Foo(props: {}) { return undefined; }

(any as React.ElementRef<typeof Foo>).nope; // Error
