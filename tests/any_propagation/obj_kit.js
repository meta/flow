{
    declare function f<T>(a: {...{x: T}}, b: (x: T) => number): T;

    f(42 as any, (value) => { return 42 }); // no error
}

{
    declare function f<T>(a: Readonly<{x: T}>, b: (x: T) => number): T;

    f(42 as any, (value) => { return 42 }); // no error
}
