type T = {[_K in keyof {f: number}]: string};
declare const t: T;

export const getProp = t.f;
export const getElem = t['f'];
export const spread = {...t};
declare const exact: $Exact<T>;
export const exactProp = exact.f;
declare export const proto: {__proto__: T};
export const not = !t;
declare const keyMirror: $KeyMirror<T>;
export const keyMirrorProp = keyMirror.f;
declare const keys: keyof T;
export const keysLength = keys.length;

type NonHomomorphic = {[K in 'a' | 'b']: number};
declare const nonHomomorphic: NonHomomorphic;
export const nonHomomorphicProp = nonHomomorphic.a;

type Optional = {[K in keyof {f: number}]?: string};
declare const optional: Optional;
export const optionalProp = optional.f;

type Source = {f: number, g?: string, h?: ?string};

declare const picked: Pick<Source, 'f' | 'g'>;
export const pickedF = picked.f;
export const pickedG = picked.g;

declare const stringKeys: {[K in string]: number};
export const stringKeysProp = stringKeys.anything;

declare const required: {[K in keyof Source]-?: Source[K]};
export const requiredF = required.f;
export const requiredG = required.g;
export const requiredH = required.h;

type SelfKeyed = {[K in 'a' | keyof SelfKeyed]: number};
declare const selfKeyed: SelfKeyed;
export const selfKeyedProp = selfKeyed.a;
