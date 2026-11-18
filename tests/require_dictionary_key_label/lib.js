// Library definitions label their dictionary keys too.
declare type LibDict = {[key: string]: number}; // ok
declare class LibClass {
  [key: string]: number; // ok
}

declare module 'm' {
  declare export type D = {[key: string]: number}; // ok
}
