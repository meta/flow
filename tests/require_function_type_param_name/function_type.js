type Unnamed = (string) => void; // error
type Named = (x: string) => void; // ok
type NoParams = () => void; // ok
type Mixed = (x: string, number, y: boolean) => void; // error on `number`
type UnnamedRest = (string, ...Array<number>) => void; // error on `string` and the rest
type NamedRest = (x: string, ...rest: Array<number>) => void; // ok
type ThisParam = (this: unknown, string) => void; // error on `string` only
type Nested = ((string) => void) => void; // error on both params
type NestedNamed = (cb: (s: string) => void) => void; // ok
type Generic = <T>(T) => T; // error
type Hook = hook (string) => void; // error
type Optional = (x?: string) => void; // ok

let annot: (string) => void = (s: string) => {}; // error

export type Exported = (number) => void; // error
