// No errors are expected in this file.

type GenericFnType<TArgs, TReturn> = (...args: TArgs) => TReturn;
type T = GenericFnType<Iterable<string>, boolean>;
var t: T = function(x: string, y: string): boolean {return false;}
type U = (x: number, ...rest: Iterable<string>) => boolean;
var u: U = function(x: number, y: string): boolean {return false;}
type GenericFnType2<TArgs, TReturn> = (x: number, ...args: TArgs) => TReturn;
type V = (x: number, ...rest: Iterable<string>) => boolean;
var v: V = function(x: number, y: string): boolean {return false;}
