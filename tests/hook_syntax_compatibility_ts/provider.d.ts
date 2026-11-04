// TypeScript has no hook syntax: hooks are functions named `use*`.
export declare function useValue(): number;
export declare function useIdentity<T>(x: T): T;
export declare namespace Hooks {
  function useNamespaced(): number;
}
export declare function getValue(): number;
