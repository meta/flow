// Library definitions are exempt, as they are from `unclear-type`.
declare type LibDict = {[string]: number}; // ok
declare class LibClass {
  [string]: number; // ok
}

declare module 'm' {
  declare export type D = {[string]: number}; // ok
}
