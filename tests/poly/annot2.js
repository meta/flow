const f = <T>(x: T) => (y: T) => y;
const g = <T>(x: T): ((y: T) => T) => (y: T) => y;

f(0)(1) as number; // ok, since return of f is any
g(0)(1) as number; // ok
