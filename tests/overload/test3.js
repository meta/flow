// unions
declare function f(x: string): void;
declare function f(x: number): void;
declare const x_f: string | number;
f(x_f); // error: Flow does not split union args across overload branches

// maybe
declare function g(x: null): void;
declare function g(x: undefined): void;
declare function g(x: string): void;
declare const x_g: ?string;
g(x_g); // error: Flow does not split union args across overload branches

// optional
declare function h(x: undefined): void;
declare function h(x: string): void;
declare const x_h: {p?: string};
h(x_h.p); // error: Flow does not split union args across overload branches
