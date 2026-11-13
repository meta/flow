type A = (string) => void;
type B = (x: string, number) => void;
type C = (...Array<string>) => void;
type D = string => void;
type E = <T>(T) => T;
type F = new (string) => void;
type G = (x: string, ...rest: Array<number>) => void;
type H = (this: mixed, x?: string) => void;
type I = (string);
