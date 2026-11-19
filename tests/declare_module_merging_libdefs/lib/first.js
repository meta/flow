declare module 'm' {
  declare type Local = number;
  declare export type FirstLocal = Local;
  declare export const first: number;
  declare export type First = number;
  declare export type FromSecond = Second;
  declare export function overloaded(x: number): number;
  declare export interface Merged {
    a: number;
  }
  declare export namespace NS {
    declare const a: number;
  }
  declare export class C {
    a: number;
  }
  declare export const duplicate: number;
}

declare module 'implicit' {
  declare const implicitFirst: number;
}

declare module 'cjs' {
  declare module.exports: number;
}

declare module 'ns' {
  declare export namespace Types {
    declare type FirstType = number;
  }
  declare export namespace Nested {
    declare namespace Inner {
      declare const first: number;
    }
    declare type FirstType = number;
    declare type FromSecond = SecondType;
    declare const fromSecond: typeof secondValue;
    declare const fromInner: typeof Inner.second; // error: like for global namespaces, the library check sees this block's `Inner`
  }
  declare export namespace TypesThenValues {
    declare const firstValue: number;
  }
  declare export namespace ValuesThenTypes {
    declare type FirstType = number;
  }
}
