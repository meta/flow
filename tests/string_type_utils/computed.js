declare const keyStringPrefix: StringPrefix<'abc'>;
declare const keyStringSuffix: StringSuffix<'xyz'>;
declare const empty: empty;

{
    const x = {def: 0, [keyStringPrefix]: 0}; // okay no overlap
    x as {def: number, [key: typeof keyStringPrefix]: number}; // okay
    x.def as number; // okay
    x.abc as number; // okay
    x.xyz; // error property 'xyz' missing
}
{
    const x = {abc: 0, [keyStringPrefix]: 0}; // error overlap
    x as {abc: number}; // okay [keyStringPrefix] is not recorded
    empty as {abc: number} as typeof x; // okay [keyStringPrefix] is not recorded
}
{
    const x = {abc: 0, abcdef: 1, [keyStringPrefix]: 0}; // error overlap with both
    x as {abc: number, abcdef: number}; // okay [keyStringPrefix] is not recorded
    empty as {abc: number, abcdef: number} as typeof x; // okay [keyStringPrefix] is not
}
{
    const x = {abc: 0, def: 1, [keyStringPrefix]: 0}; // error overlap with one
    x as {abc: number, def: number}; // okay [keyStringPrefix] is not recorded
    empty as {abc: number, def: number} as typeof x; // okay [keyStringPrefix] is not recorded
}
{
    const x = {[keyStringPrefix]: 0, abc: 0}; // okay (overlaps but comes later)
    x as {[key: typeof keyStringPrefix]: number, abc: number}; // okay [keyStringPrefix] is recorded
}
{
    const x = {abc: 0, [keyStringSuffix]: 0}; // okay no overlap
    x as {[key: typeof keyStringSuffix]: number, abc: number}; // okay [keyStringSuffix] is recorded
}
{
    const x = {uvwxyz: 0, [keyStringSuffix]: 0}; // error overlap
    x as {uvwxyz: number}; // okay [keyStringSuffix] is not recorded
}
