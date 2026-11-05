declare function c<T = string>(x: [T] extends [string] ? T : Array<T>, y: T): T;

// The default does not decide the conditional type before the later arguments are seen.
c([1], 1) as number; // ok
c([1], 1) as empty; // error: `number`
