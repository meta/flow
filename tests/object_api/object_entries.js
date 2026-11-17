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

Object.entries(dict) as Array<[string, number]>; // OK
Object.entries(iface) as Array<[string, unknown]>; // OK
Object.entries(dictWithProps) as Array<[string, unknown]>; // OK
Object.entries(dictUnion) as Array<[string, number | boolean]>; // OK
Object.entries(writeOnlyDict) as Array<[string, unknown]>; // OK
Object.entries(obj) as Array<['a' | 'b', unknown]>; // OK
Object.entries(instance) as Array<['a' | 'b', unknown]>; // OK

Object.entries(dict) as Array<[string, empty]>; // ERROR
Object.entries(iface) as Array<[string, empty]>; // ERROR
Object.entries(dictWithProps) as Array<[string, empty]>; // ERROR
Object.entries(dictUnion) as Array<[string, empty]>; // ERROR
Object.entries(writeOnlyDict) as Array<[string, empty]>; // ERROR
Object.entries(obj) as Array<['a' | 'b', empty]>; // ERROR
Object.entries(instance) as Array<['a' | 'b', empty]>; // ERROR
Object.entries(dict) as Array<empty>; // ERROR

// Invalid inputs
Object.entries(undefined); // ERROR
Object.entries(null); // ERROR
Object.entries(1); // ERROR
Object.entries(true); // ERROR
Object.entries([1, 2]); // ERROR
