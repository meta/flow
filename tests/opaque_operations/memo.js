export opaque type Memoized<out T>: T = T;

declare export hook useMemo<T>(create: () => T): Memoized<T>;
