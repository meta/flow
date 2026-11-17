declare const s1: string;
const f1 = {foo: 'string', [s1]: 3}; // error: might overwrite
const f2 = {foo: 'string', bar: '', [s1]: 3}; // error: might overwrite

declare const s2: 'foo' | 'bar';
const f3 = {foo: 'string', [s2]: 3};
const f4 = {baz: 'string', [s2]: 3}; // error: Flow conservatively rejects all such cases (does not check that 'baz' is disjoint from 'foo' | 'bar')
f4 as {baz: string, [key: 'foo' | 'bar']: number}; // ok
