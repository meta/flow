declare class T {}
declare class U {}

declare const o1: {...{[key: string]:T},...{p:U, ...}, ...}; // OK
o1 as {p?:T|U,[key: string]:T}; // Error: p is invariant
