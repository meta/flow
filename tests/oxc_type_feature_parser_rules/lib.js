// Library definitions are parsed without the oxc rules.
declare function libUnnamed(string): void; // ok
declare type LibCallback = string | () => void; // ok
declare type LibIndexer = {[string]: number}; // ok
