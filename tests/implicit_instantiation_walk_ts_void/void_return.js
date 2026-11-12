// A function type returning `void` ignores the return of the function it
// accepts, so a type parameter in that return gets no bound from `void`.
declare function apply<T>(f: () => T, g: (h: () => T) => void): T;
apply(() => 1, (h: () => void) => {}) as number; // OK

declare function useCallback<T>(f: (x: T) => void, x: T): T;
useCallback((x: number) => 'a', 1) as number; // OK
