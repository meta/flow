// @flow

type ReadonlyDict = {readonly [key: string]: string, ...} & {readonly [key: string]: string, ...};
declare const dict: ReadonlyDict;
dict.key = 'changed'; // ERROR

type ReadonlyPropWithDict = {readonly key: string, ...} & {readonly [key: string]: string, ...};
declare const propWithDict: ReadonlyPropWithDict;
propWithDict.key = 'changed'; // ERROR

interface SetterInterface {
  set value(value: string): void;
}
type WithSetter = SetterInterface & {other: number, ...};
declare const setter: WithSetter;
setter.value = 'ok';
function readSetter(v: WithSetter): string {
  return v.value; // ERROR
}

interface GetterSetterInterface {
  get value(): string;
  set value(value: string | number): void;
}
type WithGetterSetter = GetterSetterInterface & {other: number, ...};
declare const getterSetter: WithGetterSetter;
getterSetter.value = 42;
function readGetterSetter(v: WithGetterSetter): string {
  return v.value;
}

interface ConstructableBase {
  new (): {x: number, ...};
}
interface DerivedConstructable extends ConstructableBase {
  x: number;
}
type WithConstructable = DerivedConstructable & {y: number, ...};
declare const constructable: WithConstructable;
new constructable();

interface A {a: string;}
interface B {b: number;}
interface C {c: boolean;}
interface D {d: string;}
type AB = A & B;
type CD = C & D;
type ABCD = AB & CD;
declare const abcd: ABCD;
abcd.a as string;
abcd.b as number;
abcd.c as boolean;
abcd.d as string;

type WriteonlyProp = {writeonly value: string, ...} & {other: number, ...};
declare const writeonlyProp: WriteonlyProp;
writeonlyProp.value = 'ok';
writeonlyProp.value = 42; // ERROR
function readWriteonlyProp(v: WriteonlyProp): string {
  return v.value; // ERROR
}

type WriteonlyDict = {writeonly [key: string]: string, ...} & {other: number, ...};
declare const writeonlyDict: WriteonlyDict;
writeonlyDict.value = 'ok';
writeonlyDict.value = 42; // ERROR
function readWriteonlyDict(v: WriteonlyDict): string {
  return v.value; // ERROR
}

import type {ImportedReadonlyDictionary} from './typescript';
declare const importedReadonlyDictionary: ImportedReadonlyDictionary;
importedReadonlyDictionary.key = 'changed'; // ERROR

type NumericDictionary = {[key: number]: string, ...} & {extra: number, ...};
declare const numericDictionary: NumericDictionary;
numericDictionary.extra = 42;
function readNumericDictionary(v: NumericDictionary): empty {
  return v.extra; // ERROR
}

import type {ImportedTypeScriptMember} from './typescript';
type ExactAndTypeScript = {|shared: string|} & ImportedTypeScriptMember;
declare const inexactShared: {shared: string, other: number, ...};
inexactShared as ExactAndTypeScript; // ERROR
