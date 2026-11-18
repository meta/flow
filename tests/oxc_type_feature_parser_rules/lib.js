// Library definitions are parsed without the oxc rules.
declare function libNamed(s: string): void; // ok
declare type LibCallback = string | () => void; // ok
declare type LibIndexer = {[key: string]: number}; // ok
