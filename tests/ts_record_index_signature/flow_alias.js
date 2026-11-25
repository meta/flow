// @flow

// Flow declarations that `node_modules/repro` applies from TypeScript.
export type Dict<V> = {[key: string]: V};
export type Box<T> = {value: T};
export type Fn<T> = (x: T) => void;
export type Spread<T> = {...T};
export type Typed = {type: unknown};
export type DictD<V = unknown> = {[key: string]: V};
export type J<T> = {v: T};
export type K<T> = {v: T, k1: K<J<T>>, k2: K<J<T>>, k3: K<J<T>>, k4: K<J<T>>};
export class Cat {
  name: string = '';
}
export interface Handler<T> {
  handle(x: T): void;
}
