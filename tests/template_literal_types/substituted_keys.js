// A template literal type whose placeholders are all literal is the union of
// the strings it produces, so a mapped type gets one property per string.

type Events = {mark: number, measure: string};

type Handlers = {[K in keyof Events as `on${K}`]: (e: Events[K]) => void};
declare const h: Handlers;
h.onmark(1); // OK
h.onmark('a'); // ERROR: string is incompatible with number
h.onmeasure('a'); // OK
h.onnope; // ERROR: property missing

type On<K extends string> = `on${K}`;
type AB = 'a' | 'b';
type Inline = {[K in On<'a' | 'b'>]: number};
type Alias = {[K in On<AB>]: number};
type Keys = {[K in On<keyof Events>]: number};
type Filtered = {
  [K in keyof Events as K extends 'mark' ? never : `on${K}`]: number,
};
({ona: 1, onb: 2}) as Inline; // OK
({ona: 1, onb: 2}) as Alias; // OK
({ona: 1}) as Alias; // ERROR: property `onb` missing
({onmark: 1, onmeasure: 2}) as Keys; // OK
({onmeasure: 1}) as Filtered; // OK
({onmark: 1}) as Filtered; // ERROR: property `onmeasure` missing

// A generic placeholder isn't expanded.
function generic<K extends string>(x: {[P in On<K>]: number}) {
  x.ona; // ERROR: no property `ona` on a generic mapped type
}

// A template that only expands through another template, such as a recursive
// alias, stays an indexer.
type Rec<T extends string> = T | `x${Rec<T>}`;
type Recursive = {[K in Rec<'a'>]: number};
declare const r: Recursive;
r.a as number; // OK
