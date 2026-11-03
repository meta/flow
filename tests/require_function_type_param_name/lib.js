// Library definitions are exempt, as they are from `unclear-type`.
declare function libUnnamed(string): void; // ok
declare type LibCallback = (number) => void; // ok

declare module 'm' {
  declare export function unnamedInModule(string): void; // ok
}
