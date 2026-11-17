// Valid Usage
type Obj = {p: number, ...};
declare const obj: Readonly<Obj>;
obj.p = 42; // Should error!
obj.p as number; // OK

declare const writableObj: Obj;
writableObj as Readonly<Obj>; // OK

type MultiKeyObj = {p: number, q: number, ...};
declare const multiKeyObj: Readonly<MultiKeyObj>;
multiKeyObj.p = 42; // Should error!
multiKeyObj.q = 42; // Should error!
multiKeyObj.p as number; // OK
multiKeyObj.q as number; // OK

type UnionObj = {key: 1, p: number, ...} | {key: 2, p: number, q: number, ...};
declare const unionObj: Readonly<UnionObj>;
if (unionObj.key === 1) {
  unionObj.p = 42; // Should error!
  unionObj.p as number; // OK
  unionObj.q as number; // Should error!
} else {
  unionObj.p = 42; // Should error!
  unionObj.q = 42; // Should error!
  unionObj.p as number; // OK
  unionObj.q as number; // OK
}
unionObj as {readonly key: 1, readonly p: number, ...} | {readonly key: 2, readonly p: number, readonly q: number, ...}; // OK

type SpreadUnionObj = {...UnionObj, z: number, ...};
declare const spreadUnionObj: Readonly<SpreadUnionObj>
if (spreadUnionObj.key === 1) {
  spreadUnionObj.p = 42; // Should error!
  spreadUnionObj.z = 42; // Should error!
  spreadUnionObj.p as number; // OK
  spreadUnionObj.z as number; // OK
  spreadUnionObj.q as number; // Should error!
} else if (spreadUnionObj.key === 2) {
  spreadUnionObj.p = 42; // Should error!
  spreadUnionObj.q = 42; // Should error!
  spreadUnionObj.z = 42; // Should error!
  spreadUnionObj.p as number; // OK
  spreadUnionObj.q as number; // OK
  spreadUnionObj.z as number; // OK
}

type IntersectionObj = Obj & MultiKeyObj;
declare const intersectionObj: Readonly<IntersectionObj>;
intersectionObj.p = 42; // Should error!
intersectionObj.q = 42; // Should error!
intersectionObj.p as number; // OK
intersectionObj.q as number; // OK

type IndexerKeyObj = {p: number, [key: string]: boolean};
declare const indexerKeyObj: Readonly<IndexerKeyObj>;
indexerKeyObj.p = 42; // Should error!
indexerKeyObj.x = true; // Should error!
indexerKeyObj.p as number; // OK
indexerKeyObj.x as boolean; // OK

type ExactObj = {p: number};
declare const exactObj: Readonly<ExactObj>;
exactObj.p = 42; // Should error!
exactObj.p as number; // OK

type SpreadObj = {...{p: number, ...}, ...};
declare const spreadObj: Readonly<SpreadObj>;
spreadObj.p = 42; // Should error!
spreadObj.p as number; // OK

type SpreadExactObj = {...{p: number}};
declare const spreadExactObj: Readonly<SpreadExactObj>;
spreadExactObj.p = 42; // Should error!
spreadExactObj.p as number; // OK

type ObjWithMethod = {p: number, fn(): boolean, ...};
declare const objWithMethod: Readonly<ObjWithMethod>;
objWithMethod.p = 42; // Should error!
objWithMethod.fn = () => true; // Should error!
objWithMethod.p as number; // OK
objWithMethod.fn as () => boolean; // OK

class A {
  p: number;
}
declare const instance: Readonly<A>;
instance.p = 42; // Should error!
instance.p as number; // OK

type WriteOnlyObj = {writeonly p: 42, ...};
declare const writeOnlyObj: Readonly<WriteOnlyObj>;
writeOnlyObj.p = 42; // Should error!
writeOnlyObj.p as number // OK

// Invalid usage
type readOnlyNum = Readonly<number>;
42 as readOnlyNum; // Should error!

// $ReadOnly preserves literal-ness
declare function create<S>(obj: S): Readonly<S>;
create({ f: 1 }) as { f: any, ... };
