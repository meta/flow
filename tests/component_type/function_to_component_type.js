declare function Foo(props: {bar: string}): undefined;
declare class ImNotARefSetter {}

Foo as component(bar: string); // ok
Foo as component(bar: number); // error: number ~> string
Foo as component(...{bar: number}); // error: number ~> string
Foo as component(bar: string, ref: React.RefSetter<void>); // error: extra ref prop
Foo as component( // error: void ~> string
    bar: string,
    ref: React.RefSetter<string>
);
Foo as component( // error again due to bad ref
    bar: string,
    ref: ImNotARefSetter // ok: ref prop in component type can be arbitrary
);
