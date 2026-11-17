//@flow
declare function test<T>(): ((f: (x: T) => T) => any);

export default test<unknown>();
