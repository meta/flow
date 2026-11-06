component Foo(arr: Array<number>) {
    arr.sort((a: number, b: number) => {
//      ^
        return 1;
    });
    return null;
}
