declare function unnamed(string): void; // error
declare function named(x: string): void; // ok
declare function mixed(x: string, number): void; // error on `number`
declare function unnamedRest(...Array<string>): void; // error
declare function namedRest(...rest: Array<string>): void; // ok
declare function overloaded(string): void; // error
declare function overloaded(x: number): void; // ok
declare function withCallback(cb: (string) => void): void; // error on the callback param
declare hook useUnnamed(string): void; // error
