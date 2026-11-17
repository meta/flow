declare const dict: {[key: string]: number};

declare const dictWithProps: {[key: string]: number, a: boolean};

declare const writeOnlyDict: {writeonly [key: string]: number};

declare const dictUnion: {[key: string]: number} | {[key: string]: boolean};

declare const obj: {a: 1, b: 2};

declare class C {
  a: number;
  b: string;
}
declare const instance: C;

declare const iface: interface {[key: string]: number};

Object.values(dict) as Array<number>; // OK
Object.values(iface) as Array<unknown>; // OK
Object.values(dictWithProps) as Array<unknown>; // OK
Object.values(dictUnion) as Array<number | boolean>; // OK
Object.values(writeOnlyDict) as Array<unknown>; // OK
Object.values(obj) as Array<unknown>; // OK
Object.values(instance) as Array<unknown>; // OK

Object.values(dict) as Array<empty>; // ERROR
Object.values(iface) as Array<empty>; // ERROR
Object.values(dictWithProps) as Array<empty>; // ERROR
Object.values(dictUnion) as Array<empty>; // ERROR
Object.values(writeOnlyDict) as Array<empty>; // ERROR
Object.values(obj) as Array<empty>; // ERROR
Object.values(instance) as Array<empty>; // ERROR

// Invalid inputs
Object.values(undefined); // ERROR
Object.values(null); // ERROR
Object.values(1); // ERROR
Object.values(true); // ERROR
Object.values([1, 2]); // ERROR
