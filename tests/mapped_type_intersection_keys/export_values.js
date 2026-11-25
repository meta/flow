// Exported values whose types are mapped types over intersection keys, read
// in an export position, so that annotation inference evaluates them.

// tsc accepts the TypeScript form of these two (with `{}` for `{...}`).
declare const intersectionKeys: {[K in 'a' | (string & {...})]: number};
export const intersectionKeysProp = intersectionKeys.a;

type IntersectionIndexer = {f: number, [key: string & {...}]: number};
declare const intersectionIndexerKeys: {
  [K in keyof IntersectionIndexer | 'g']: number,
};
export const intersectionIndexerKeysProp = intersectionIndexerKeys.g;

// tsc: TS2456 for each alias (circular).
type RecursiveFirst<T> = RecursiveFirst<T> & T;
declare const recursiveFirst: {[K in RecursiveFirst<string>]: number};
export const recursiveFirstProp = recursiveFirst.zz;

type RecursiveInUnion<T> = (RecursiveInUnion<T> | T) & {...};
declare const recursiveInUnion: {[K in RecursiveInUnion<string>]: number};
export const recursiveInUnionProp = recursiveInUnion.zz;

type RecursiveInUnionLast<T> = (T | RecursiveInUnionLast<T>) & {...};
declare const recursiveInUnionLast: {
  [K in RecursiveInUnionLast<string>]: number,
};
export const recursiveInUnionLastProp = recursiveInUnionLast.zz;

// tsc accepts both. Annotation inference cannot evaluate the conditional
// member in either order.
declare const conditionalFirst: {
  [K in (string extends string ? {...} : {...}) & string]: number,
};
export const conditionalFirstProp = conditionalFirst.abc;
declare const conditionalLast: {
  [K in string & (string extends string ? {...} : {...})]: number,
};
export const conditionalLastProp = conditionalLast.abc;

// tsc: TS1337 (literal index signature parameter). Checking picks the member
// by speculating against the access; annotation inference cannot speculate.
type IndexedByIntersection = {[k: string & 'a']: number};
declare const accessedByIntersection: {a: {p: number}}[keyof IndexedByIntersection];
export const accessedByIntersectionProp = accessedByIntersection.p;

// tsc accepts. Aliases that share their members.
type Shared0 = string;
type Other0 = string;
type Shared1 = (Shared0 | Other0) & {...};
type Other1 = (Other0 | Shared0) & {g: number};
type Shared2 = (Shared1 | Other1) & {...};
type Other2 = (Other1 | Shared1) & {g: number};
declare const sharedMembers: {[K in Shared2]: number};
export const sharedMembersProp = sharedMembers.zz;

// tsc: TS2456, TS2313 (circular).
type SelfKeyed = {[K in keyof SelfKeyed & string]: number};
declare const selfKeyed: SelfKeyed;
export const selfKeyedProp = selfKeyed.zz;
