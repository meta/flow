declare module 'm' {
  declare type Local = string;
  declare export type SecondLocal = Local;
  declare export const second: string;
  declare export type Second = string;
  declare export type FromFirst = First;
  declare export function overloaded(x: string): string;
  declare export interface Merged {
    b: string;
  }
  declare export namespace NS {
    declare const b: string;
  }
  declare export interface C {
    b: string;
  }
  declare export const duplicate: string; // error: duplicate export
}

declare module 'implicit' {
  declare export const implicitSecond: string;
}

declare module 'cjs' {
  declare module.exports: string; // error: module.exports doesn't merge
}

declare module 'ns' {
  declare export namespace Types {
    declare type SecondType = string;
  }
  declare export namespace Nested {
    declare namespace Inner {
      declare const second: string;
    }
    declare type SecondType = string;
    declare type FromFirst = FirstType;
    declare const secondValue: string;
  }
  declare export namespace TypesThenValues {
    declare type SecondType = string;
  }
  declare export namespace ValuesThenTypes {
    declare const secondValue: string;
  }
}
