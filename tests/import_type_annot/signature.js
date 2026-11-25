// The types of `test.js`, in signatures that `test.js` imports.
export type ImportedClass = import('./exports').MyClass;
export type ImportedType = import('./exports').MyType;
export type Inner = import('./exports').Nested.Inner;
export type T = import('./export_type_only').T;
export type Pair = import('./export_type_only').Pair<number>;
