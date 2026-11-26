/* @flow */

function testKeysOfAny(str: string, lit: 'hi') {
  str as keyof any; // Any string should be fine
  if (str) {
    str as keyof any; // No error, truthy string should be fine
  }
  'hi' as keyof any; // String literal should be fine

  123 as keyof any; // No error, `keyof any` is `any`
}

type AnyAlias = any;
type Id<T> = T;
function testKeysOfAnyIsAny(
  k: keyof any,
  kAlias: keyof AnyAlias,
  kApp: keyof Id<any>,
  sym: symbol,
) {
  k as number; // No error, `keyof any` is `any` in the source position too
  kAlias as number; // No error, the alias is resolved
  kApp as number; // No error, the type application is resolved

  sym as keyof any; // No error, every key type is allowed
  123 as keyof AnyAlias; // No error, the alias is resolved
  123 as keyof Id<any>; // No error, the type application is resolved
}

type StrDict = {[key: string]: unknown};
function testKeysOfStrDict(str: string, lit: 'hi') {
  str as keyof StrDict; // Any string should be fine
  if (str) {
    str as keyof StrDict; // No error, truthy string should be fine
  }
  'hi' as keyof StrDict; // String literal should be fine

  123 as keyof StrDict; // Error: number -> keys of StrDict
}

type StrLitDict = {[key: 'hi']: unknown};
function testKeysOfStrLitDict(str: string, lit: 'hi') {
  str as keyof StrLitDict; // Error: Not all strings are allowed
  if (str) {
    str as keyof StrLitDict; // Error: Not all truthy strings are allowed
  }
  'hi' as keyof StrLitDict; // The right string literal is allowed
  'bye' as keyof StrLitDict; // Error: The wrong string literal is not allowed

  123 as keyof StrLitDict; // Error: number -> keys of StrLitDict
}

type ObjLit = {hi: unknown};
function testKeysOfOtherObj(str: string, lit: 'hi') {
  str as keyof ObjLit; // Error: string -> keys of ObjLit
  if (str) {
    str as keyof ObjLit; // Error: truthy string -> keys of ObjLit
  }
  'hi' as keyof ObjLit; // String literal should be fine

  123 as keyof ObjLit; // Error: number -> keys of ObjLit
}

function testKeysOfSameTypeParam<T>(k: keyof T): keyof T {
  return k; // OK
}

function testKeysOfOtherTypeParam<T, S>(k: keyof T): keyof S {
  return k; // Error
}

type Rec<K extends string | number | symbol, V> = {[P in K]: V};

function testIndexByKeysBoundedParam<T, K extends keyof T>(o: Rec<keyof T, number>, k: K): number {
  return o[k]; // OK
}

function testIndexByOtherKeysBoundedParam<T, S, K extends keyof S>(o: Rec<keyof T, number>, k: K): number {
  return o[k]; // Error
}
