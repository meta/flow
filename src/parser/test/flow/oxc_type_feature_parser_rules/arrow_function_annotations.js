const f = (cb: (string) => void) => {};
const g = (x: string | () => void): void => {};
f<(string) => void>(x);
