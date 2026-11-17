declare function get(key: string): void;
declare function get(key: number): void;
get(true); // incompatible-type

declare const x: null & 3;

x(); // not-a-function

declare const f: null & ((x: number) => void);
f(true); // incompatible-type

declare const y : number & string;
y as boolean // incompatible-type
