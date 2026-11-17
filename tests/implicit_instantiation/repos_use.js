// @flow

type MyEnum = 'FOO' | 'BAR';
type T<X> = X;

declare function getKeys1<TKey>(x: {readonly [key: {foo: TKey}['foo']]: unknown}): TKey;
declare function getKeys2<TKey>(x: {readonly [key: T<TKey>]: unknown}): TKey;

declare const dict: {[key: MyEnum]: unknown};

const x1 = getKeys1(dict); // OK
x1 as MyEnum; // OK
const y1: MyEnum = getKeys1(dict); // OK

const x2 = getKeys2(dict); // OK
x2 as MyEnum; // OK
const y2: MyEnum = getKeys2(dict); // OK

const err1 = getKeys1(dict); // OK
err1 as 'FOO'; // ERR
const err1_: 'FOO' = getKeys1(dict); // ERR

const err2 = getKeys2(dict); // OK
err2 as 'FOO'; // ERR
const err2_: 'FOO' = getKeys2(dict); // ERR
