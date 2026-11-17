const p = new Promise((resolve) => resolve(""));
p as Promise<string>; // error

declare function magicCall1<T>(x: (y: T) => unknown): void;
declare function magicCall2<T = number>(x: (y: T) => unknown): T;

magicCall1(s => s as string); // error
magicCall2(s => s as string); // error
magicCall1(s => s.abcd); // error
magicCall2(s => s.abcd); // error
