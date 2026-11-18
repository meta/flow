// Library definitions name their function type parameters too.
declare function libNamed(s: string): void; // ok
declare type LibCallback = (n: number) => void; // ok

declare module 'm' {
  declare export function namedInModule(s: string): void; // ok
}
