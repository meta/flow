// Duplicate indexers
interface Ok {
  [key: string]: string;
}

interface Bad {
  [k1: string]: string;
  [k2: number]: number; // ERROR
}

// Access with string literal type
interface PropAndIndexer {
  [key: number]: string;
  foo: boolean;
}
{
  declare const x: PropAndIndexer;
  x.foo as boolean; // OK
  x['foo'] as boolean; // OK
  x['foo'] = true; // OK
}

// Variance
type Keys = 'a' | 'b';

interface Invariant {
  [key: Keys]: number | string;
}
interface Covariant {
  readonly [key: Keys]: number | string;
}
interface Contravariant {
  writeonly [key: Keys]: number;
}

{
  declare const x: Invariant;
  x['a']; // OK
  x['a'] = 1; // OK
  x['xxx']; // ERROR
}
{
  declare const x: Covariant;
  x['a']; // OK
  x['a'] = 1; // ERROR
  x['xxx']; // ERROR
}
{
  declare const x: Contravariant;
  x['a']; // ERROR
  x['a'] = 1; // OK
  x['xxx'] = 1; // ERROR
}
{
  declare const x: interface {readonly [key: string]: number};
  declare const s: string;
  x[s]; // OK
  x[s] = 1; // ERROR
  x[true]; // ERROR
}

{
  declare const x: {[key: Keys]: number};
  x as Invariant; // ERROR
  x as Covariant; // OK
  x as Contravariant; // OK
}
{
  declare const x: interface {[key: Keys]: number};
  x as Invariant; // ERROR
  x as Covariant; // OK
  x as Contravariant; // OK
}

{
  declare const x: {[key: Keys]: number | string};
  x as interface {[key: Keys]: number}; // ERROR
  x as interface {readonly [key: Keys]: number}; // ERROR
  x as interface {writeonly [key: Keys]: number}; // OK
}
{
  declare const x: interface {[key: Keys]: number | string};
  x as interface {[key: Keys]: number}; // ERROR
  x as interface {readonly [key: Keys]: number}; // ERROR
  x as interface {writeonly [key: Keys]: number}; // OK
}

// Extends
interface NumIndexer {
  [key: number]: boolean;
}
interface A extends NumIndexer {
  foo: string;
}
{
  declare const x: A;
  x['foo'] as string; // OK
  x[1] as boolean; // OK
  x[1] = true; // OK
}

interface ParentValue {
  parent: number;
}
interface ChildValue extends ParentValue {
  child: string;
}
interface CovariantNumIndexer {
  readonly [key: number]: ParentValue;
}
interface NarrowCovariantNumIndexer extends CovariantNumIndexer {
  readonly [key: number]: ChildValue;
}
{
  declare const x: NarrowCovariantNumIndexer;
  x[1] as ChildValue; // OK
  x[1] as string; // ERROR
}

interface ArrayLikeBase {
  readonly length: number;
  readonly [key: number]: string;
}
interface InheritedArrayLike extends ArrayLikeBase {}
{
  declare const x: InheritedArrayLike;
  const values = Array.from(x);
  values as Array<string>; // OK
  values as Array<number>; // ERROR
}

interface StrIndexer {
  [key: string]: boolean;
}
interface B extends StrIndexer {
  foo: string;
}
{
  declare const x: B;
  x['foo'] as string; // OK
  x['bar'] as boolean; // OK
  x['bar'] = true; // OK
}

interface Super {
  bar: string;
}
interface C extends Super {
  [key: string]: boolean;
}
{
  declare const x: C;
  x.bar as string; // OK
  x['bar'] as string; // OK
}
interface SuperWithMethod {
  bar: string;
  foo(): null;
}
interface D extends SuperWithMethod {
  [key: string]: () => number;
}
{
  declare const x: D;
  x.bar as string; // OK
  x['bar'] as string; // OK
  x['bar'] as empty; // ERROR
  x['xxx'] as () => number; // OK

  (x['bar'] = "foo"); // OK
  (x.bar = "foo"); // OK

  x.foo(); // OK
  x['foo'](); // OK
  x['foo'] as empty; // ERROR

  x['xxx']() as number; // OK
  x['xxx'] as empty; // ERROR
}

// Methods
interface M {
  foo(): number;
  [key: string]: boolean;
}
{
  declare const x: M;
  x.foo(); // OK
  x['foo'](); // OK
  x['xxx'] as boolean; // OK
}
interface M2 extends M {
  bar(): boolean;
}
{
  declare const x: M2;
  x.bar(); // OK
  x['bar'](); // OK
  x.foo(); // OK
  x['foo'](); // OK
}

// $ReadOnly type util
{
  declare const x: Readonly<interface {[key: Keys]: number}>;
  declare const s: string;
  x['a']; // OK
  x['a'] = 1; // ERROR
  x[true]; // ERROR

  declare const y: {readonly a: number};
  y as Readonly<interface {[key: string]: number}>; // OK
}
