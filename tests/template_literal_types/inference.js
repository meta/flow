// --- Generic function parameter inference (single param) ---

declare function stripPrefix<T extends string>(s: `data-${T}`): T;
stripPrefix('data-value') as 'value'; // OK
stripPrefix('data-value') as 'other'; // ERROR

declare function f<T extends string>(x: `hello ${T}`): T;
f('hello world') as 'world'; // OK
f('hello world') as 'x'; // ERROR

// --- Generic bounded union assignability ---

function test<T extends 'foo' | 'bar'>(name: `get${T}`) {
    name as string; // OK
    name as 'getfoo' | 'getbar'; // OK
}

// --- Generic function return type (resolved via concretization at subtyping time) ---

declare function greet<N extends string>(name: N): `Hello, ${N}!`;
greet('World') as string; // OK
greet('World') as 'Hello, World!'; // OK
greet('World') as 'Hello, Other!'; // ERROR

// --- Multi-parameter inference (parseKV) ---

declare function parseKV<K extends string, V extends string>(s: `${K}=${V}`): { key: K, value: V };
const kv = parseKV('name=Alice');
kv.key as 'name'; // OK
kv.value as 'Alice'; // OK
kv.key as 'age'; // ERROR
kv.value as 'Bob'; // ERROR

// --- Conditional type: single infer with delimiter ---

type ExtractParam<S extends string> = S extends `${string}:${infer P}` ? P : never;
type P1 = ExtractParam<'route:/users'>;
'/users' as P1; // OK

type P2 = ExtractParam<'noParam'>;
// P2 should be never
declare const p2: P2;
p2 as string; // OK (never <: string)
'whatever' as P2; // ERROR (string </: never)

// --- Conditional type: multiple infers with delimiters ---

type ParseRoute<S extends string> = S extends `/${infer A}/${infer B}` ? [A, B] : never;
type R1 = ParseRoute<'/users/123'>;
// R1 should be ['users', '123']
declare const r1: R1;
r1[0] as 'users'; // OK
r1[1] as '123'; // OK
r1[0] as 'admins'; // ERROR
r1[1] as '456'; // ERROR

// --- MatchPair (greedy second infer) ---

type MatchPair<S extends string> = S extends `[${infer A},${infer B}]` ? [A, B] : unknown;

type T20 = MatchPair<'[1,2]'>;       // ['1', '2']
declare const t20: T20;
t20[0] as '1'; // OK
t20[1] as '2'; // OK
t20[0] as '3'; // ERROR

type T21 = MatchPair<'[foo,bar]'>;   // ['foo', 'bar']
declare const t21: T21;
t21[0] as 'foo'; // OK
t21[1] as 'bar'; // OK
t21[1] as 'baz'; // ERROR

type T22 = MatchPair<' [1,2]'>;      // unknown
declare const t22: T22;
t22 as string; // ERROR (unknown </: string)
'whatever' as T22; // OK (anything <: unknown)

type T23 = MatchPair<'[123]'>;       // unknown
declare const t23: T23;
t23 as string; // ERROR

type T24 = MatchPair<'[1,2,3,4]'>;   // ['1', '2,3,4'] — greedy second infer
declare const t24: T24;
t24[0] as '1'; // OK
t24[1] as '2,3,4'; // OK
t24[1] as '2'; // ERROR (greedy: tail keeps the rest)

// --- SplitFirst (infer with generic delimiter) ---

type SplitFirst<S extends string, D extends string> =
    S extends `${infer Head}${D}${infer Tail}` ? { head: Head, tail: Tail } : { head: S, tail: never };

type SF1 = SplitFirst<'hello:world:foo', ':'>;
// SF1 should be { head: 'hello', tail: 'world:foo' }
declare const sf1: SF1;
sf1.head as 'hello'; // OK
sf1.tail as 'world:foo'; // OK
sf1.head as 'world'; // ERROR
sf1.tail as 'foo'; // ERROR

type SF2 = SplitFirst<'no-colon', ':'>;
// SF2 should be { head: 'no-colon', tail: never }
declare const sf2: SF2;
sf2.head as 'no-colon'; // OK
sf2.tail as string; // OK (never <: string)
sf2.head as 'wrong'; // ERROR
'anything' as SF2['tail']; // ERROR (tail is never)

// --- ExtractDomain (nested conditional, fallback branches) ---

type ExtractDomain<S extends string> =
    S extends `${infer Protocol}://${infer Domain}/${infer Path}` ? Domain :
    S extends `${infer Protocol}://${infer Domain}` ? Domain :
    never;

type ED1 = ExtractDomain<'https://example.com/path'>;
'example.com' as ED1; // OK
'other.com' as ED1; // ERROR

type ED2 = ExtractDomain<'http://test.org'>;
'test.org' as ED2; // OK (matched by fallback branch — no trailing path)
'example.com' as ED2; // ERROR

// No match — falls through to `never`
type ED3 = ExtractDomain<'noprotocol'>;
declare const ed3: ED3;
ed3 as string; // OK (never <: string)
'anything' as ED3; // ERROR (string </: never)

// --- IsNegative (number-to-string conversion in conditional) ---

type IsNegative<T extends number> = `${T}` extends `-${string}` ? true : false;
type IN1 = IsNegative<-5>;
type IN2 = IsNegative<5>;
true as IN1; // OK
false as IN1; // ERROR
false as IN2; // OK
true as IN2; // ERROR

type AA<T extends number, Q extends number> =
    [true, true] extends [IsNegative<T>, IsNegative<Q>] ? 'Every thing is ok!' : ['strange', IsNegative<T>, IsNegative<Q>];

type BB = AA<-2, -2>;
'Every thing is ok!' as BB; // OK (both negative → first branch)
'strange' as BB; // ERROR

// --- StartsWith / EndsWith / IsPrefix ---

type StartsWith<S extends string, P extends string> = S extends `${P}${string}` ? true : false;
type SW1 = StartsWith<'hello world', 'hello'>;
true as SW1; // OK
false as SW1; // ERROR

type SW2 = StartsWith<'hello world', 'world'>;
false as SW2; // OK (doesn't start with 'world')
true as SW2; // ERROR

type EndsWith<S extends string, Suffix extends string> = S extends `${string}${Suffix}` ? true : false;
type EW1 = EndsWith<'hello world', 'world'>;
true as EW1; // OK
false as EW1; // ERROR

type EW2 = EndsWith<'hello world', 'hello'>;
false as EW2; // OK (doesn't end with 'hello')
true as EW2; // ERROR

type IsPrefix<S extends string, P extends string> =
    S extends `${P}${infer Rest}` ? true : false;

type IP1 = IsPrefix<'hello world', 'hello'>;
true as IP1; // OK
false as IP1; // ERROR

type IP2 = IsPrefix<'hello world', 'world'>;
false as IP2; // OK
true as IP2; // ERROR

// --- Replace (infer + concrete + infer adjacent) ---

type Replace<S extends string, From extends string, To extends string> =
    S extends `${infer Head}${From}${infer Tail}` ? `${Head}${To}${Tail}` : S;

type REP1 = Replace<'hello world', 'world', 'there'>;
'hello there' as REP1; // OK
'hello world' as REP1; // ERROR

// --- Single-character inference (FirstTwoAndRest) ---

type FirstTwoAndRest<S extends string> = S extends `${infer A}${infer B}${infer R}` ? [`${A}${B}`, R] : unknown;

type T25 = FirstTwoAndRest<'abcde'>;  // ['ab', 'cde']
declare const t25: T25;
t25[0] as 'ab'; // OK
t25[1] as 'cde'; // OK
t25[0] as 'xy'; // ERROR

type T26 = FirstTwoAndRest<'ab'>;     // ['ab', '']
declare const t26: T26;
t26[0] as 'ab'; // OK
t26[1] as ''; // OK
t26[1] as 'c'; // ERROR

type T27 = FirstTwoAndRest<'a'>;      // unknown (not enough chars)
declare const t27: T27;
t27 as string; // ERROR (unknown </: string)

// --- Batched single-character inference (Chars, 10 infers) ---
// `S extends ''` MUST come before the adjacent-infer patterns: the matcher
// allows `${infer C}${infer R}` to bind C='' / R='' degenerately on an empty
// remainder, so without the early base case the recursion never terminates
// and the result widens to `any`.

type Chars<S extends string> =
    string extends S ? string[] :
    S extends '' ? [] :
    S extends `${infer C0}${infer C1}${infer C2}${infer C3}${infer C4}${infer C5}${infer C6}${infer C7}${infer C8}${infer C9}${infer R}` ? [C0, C1, C2, C3, C4, C5, C6, C7, C8, C9, ...Chars<R>] :
    S extends `${infer C}${infer R}` ? [C, ...Chars<R>] :
    never;

type L1 = Chars<'FooBarBazThisIsALongerString'>;  // ['F','o','o','B','a','r',...]
declare const l1: L1;
l1[0] as 'F'; // OK
l1[1] as 'o'; // OK
l1[27] as 'g'; // OK
l1[0] as 'X'; // ERROR (l1[0] narrows to 'F')

// Wide string input → string[]
type L3 = Chars<string>;
declare const l3: L3;
l3 as string[]; // OK
l3 as [string]; // ERROR (string[] is wider than a fixed tuple)

// --- HexColor (single-char + tuple extends) ---
// The inner tuple extends uses `Readonly<[...]>` so element-wise comparison is
// covariant (subtyping). Without the `Readonly` wrap Flow would invariantly
// unify each inferred single-char with the `HexDigit` union and fall through.

type HexDigit = '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9'
              | 'A' | 'B' | 'C' | 'D' | 'E' | 'F'
              | 'a' | 'b' | 'c' | 'd' | 'e' | 'f';

type HexColor<S extends string> =
    S extends `#${infer R1}${infer R2}${infer G1}${infer G2}${infer B1}${infer B2}` ?
        [R1, R2, G1, G2, B1, B2] extends Readonly<[HexDigit, HexDigit, HexDigit, HexDigit, HexDigit, HexDigit]> ?
            S :
            never :
        never;

type HC1 = HexColor<'#FF0000'>;
'#FF0000' as HC1; // OK

type HC2 = HexColor<'#abc123'>;
'#abc123' as HC2; // OK

type HC3 = HexColor<'#XYZ123'>;
declare const hc3: HC3;
hc3 as string; // OK (never <: string)
'#XYZ123' as HC3; // ERROR (HC3 is never — XYZ are not HexDigit)

// --- Infer type constraint (S1/S2) ---
// S2's bound `extends string` forces U to satisfy it.

type S1<T> = T extends `foo${infer U}bar` ? S2<U> : never;
type S2<S extends string> = S;

type S1_match = S1<'foobazbar'>;
'baz' as S1_match; // OK
'qux' as S1_match; // ERROR

type S1_nomatch = S1<'noformat'>;
declare const s1n: S1_nomatch;
s1n as string; // OK (never <: string)
'anything' as S1_nomatch; // ERROR

// --- Infer never when source doesn't match (Foo) ---

type Foo<T> = T extends `*${infer S}*` ? S : never;

type TF1 = Foo<any>;      // TS: any (any short-circuits conditional types).
                          // Flow LIMITATION: returns `string`, the bound of `S`, here.
declare const tf1: TF1;
tf1 as string; // OK
tf1 as number; // ERROR (limitation: should be OK since `any` should short-circuit)

type TF2 = Foo<string>;   // never (string is not a literal `*..*`)
declare const tf2: TF2;
tf2 as string; // OK (never <: string)
'foo' as TF2; // ERROR

type TF3 = Foo<'abc'>;    // never (literal doesn't match `*..*`)
declare const tf3: TF3;
tf3 as string; // OK
'foo' as TF3; // ERROR

type TF4 = Foo<'*abc*'>;  // 'abc'
'abc' as TF4; // OK
'xyz' as TF4; // ERROR

type TF5 = Foo<`*${number}*`>; // `${number}`: the part of the string that `${number}` matches
'1' as TF5; // OK
'x' as TF5; // ERROR

type TF6 = Foo<`*${'a' | `b${number}`}*`>; // 'a' | `b${number}`
'a' as TF6; // OK
'c' as TF6; // ERROR

type ToNum<T> = T extends `${infer N extends number}` ? N : 0;
declare const toNum: ToNum<`${number}`>;
toNum as number; // OK: `${number}` doesn't fit the bound of `N`, which replaces it

declare function numOf<T extends number>(x: `n${T}`): T;
declare const nn: `n${number}`;
numOf(nn) as number; // OK

declare function stripOn<K extends string>(k: `on${K}`): K;
function rewrap<K extends string>(k: `on${K}`): K { return stripOn(k); } // OK: `K` fits the bound

type ToLit<T> = T extends `${infer N extends number}` ? N : never;
declare const toLit: ToLit<'1'>;
toLit as 1; // OK: the string is read as the number its bound asks for

declare function inArray<T>(x: Array<`x${T}`>): T;
declare const xs: Array<`x${number}`>;
inArray(xs) as `${number}`; // OK: `x${`${number}`}` is `x${number}`

declare function inCallback<T>(f: (x: `a${T}`) => void): T;
inCallback((x: `a${number}`) => {}); // OK

type FromParam<S> = S extends (x: `x${infer N}`) => void ? N : 0;
declare const fromParam: FromParam<(x: `x${number}`) => void>;
fromParam as `${number}`; // OK

// --- Distributive conditional with template literal (Wrap) ---

type Wrap<T extends string> = T extends string ? `[${T}]` : never;
type W1 = Wrap<'a' | 'b' | 'c'>;
'[a]' as W1; // OK
'[b]' as W1; // OK
'[c]' as W1; // OK
'[d]' as W1; // ERROR

// --- Longer infer chains (Parse4, Parse5) ---

type Parse4<S extends string> = S extends `${infer A}-${infer B}-${infer C}-${infer D}` ? [A, B, C, D] : never;
type P4_1 = Parse4<'a-b-c-d'>;
declare const p4_1: P4_1;
p4_1[0] as 'a'; // OK
p4_1[1] as 'b'; // OK
p4_1[2] as 'c'; // OK
p4_1[3] as 'd'; // OK
p4_1[0] as 'x'; // ERROR

type Parse5<S extends string> = S extends `${infer A}/${infer B}/${infer C}/${infer D}/${infer E}` ? [A, B, C, D, E] : never;
type P5_1 = Parse5<'a/b/c/d/e'>;
declare const p5_1: P5_1;
p5_1[0] as 'a'; // OK
p5_1[4] as 'e'; // OK
p5_1[4] as 'x'; // ERROR

// --- Recursive Trim ---

type TrimLeft<S extends string> =
    S extends ` ${infer R}` ? TrimLeft<R> : S;

type TrimRight<S extends string> =
    S extends `${infer R} ` ? TrimRight<R> : S;

type Trim<S extends string> = TrimLeft<TrimRight<S>>;

type TR1 = Trim<'  hello  '>;
'hello' as TR1; // OK
' hello' as TR1; // ERROR

type TR2 = Trim<'no_spaces'>;
'no_spaces' as TR2; // OK
'other' as TR2; // ERROR

// --- Recursive Split ---

type Split<S extends string, D extends string> =
    string extends S ? string[] :
    S extends '' ? [] :
    S extends `${infer T}${D}${infer U}` ? [T, ...Split<U, D>] :
    [S];

type SP1 = Split<'a.b.c', '.'>;
// Should be ['a', 'b', 'c']
declare const sp1: SP1;
sp1[0] as 'a'; // OK
sp1[1] as 'b'; // OK
sp1[2] as 'c'; // OK
sp1[0] as 'x'; // ERROR

type SP2 = Split<'hello', ''>;
// Should be ['h', 'e', 'l', 'l', 'o']
declare const sp2: SP2;
sp2[0] as 'h'; // OK
sp2[1] as 'e'; // OK
sp2[4] as 'o'; // OK
sp2[0] as 'x'; // ERROR

// --- Recursive Join ---
// The inner `extends` shapes use `Readonly<[...]>` because Flow's mutable
// tuple subtyping is invariant: a plain `T extends [string | ...]` would force
// element-wise *unification* (not subtyping) of T's elements against the union,
// which a literal element can never satisfy. `Readonly<[...]>` makes element
// positions covariant (matching TS's default conditional-extends semantics).

type Join<T extends ReadonlyArray<string | number | boolean | bigint>, D extends string> =
    T extends [] ? '' :
    T extends Readonly<[string | number | boolean | bigint]> ? `${T[0]}` :
    T extends Readonly<[string | number | boolean | bigint, ...infer U extends ReadonlyArray<string | number | boolean | bigint>]> ? `${T[0]}${D}${Join<U, D>}` :
    string;

type J1 = Join<['a', 'b', 'c'], '.'>;
'a.b.c' as J1; // OK
'a.b.d' as J1; // ERROR

type J2 = Join<[1, 2, 3], '-'>;
'1-2-3' as J2; // OK
'wrong' as J2; // ERROR

type J3 = Join<[true, false], ','>;
'true,false' as J3; // OK

// --- Recursive ReplaceAll ---
// The base-case guard uses `[From] extends ['']` (tuple-wrapped) instead of
// `From extends ''`. The bare-tparam form makes the outer conditional
// distributive over From, and Flow's distribution-then-recursive-eval breaks
// the inner recursive call when From also appears as a placeholder in the
// inner template-literal extends. Wrapping in `[...]` skips distribution.

type ReplaceAll<S extends string, From extends string, To extends string> =
    [From] extends [''] ? S :
    S extends `${infer Head}${From}${infer Tail}` ? ReplaceAll<`${Head}${To}${Tail}`, From, To> :
    S;

type RA1 = ReplaceAll<'a.b.c.d', '.', '/'>;
'a/b/c/d' as RA1; // OK
'wrong' as RA1; // ERROR

type RA2 = ReplaceAll<'hello', 'l', 'r'>;
'herro' as RA2; // OK
'hello' as RA2; // ERROR

// Edge case: empty `From` returns S unchanged (the base-case guard fires).
type RA3 = ReplaceAll<'hello', '', 'X'>;
'hello' as RA3; // OK
'helloX' as RA3; // ERROR

// --- Backtracking on adjacent prefix candidates ---
// First candidate ('a') chops 'abc' to 'bc' but 'bc' is not a member of 'c'.
// Matcher must backtrack and try 'ab' so the remainder is 'c'.
type Backtrack<T extends string> = `${'a' | 'ab'}${T}`;
'abc' as Backtrack<'c'>; // OK (must use 'ab' as prefix)
'ade' as Backtrack<'c'>; // ERROR

// --- Generic correlation: T appears both in argument shape and return type ---
// Eagerly expanding T's bound at construction would lose this correlation.
declare function pick<T extends 'foo' | 'bar'>(x: `get${T}`): T;
pick('getfoo') as 'foo'; // OK
pick('getbar') as 'bar'; // OK
pick('getfoo') as 'bar'; // ERROR

// --- Concretization through indirected types ---
// The syntactic `extract_strings` walks UnionT/GenericT only. The concretized
// variant calls into Flow_js to resolve EvalT, KeysT, OpenT, AnnotT, TypeAppT
// before extracting, enabling enumeration through these layers.

// KeysT inside a template — self-referential type with `keyof R`.
type R = {f: `${keyof R}`};
declare const r: R;
const f1: 'f' = r.f; // OK

// TypeAppT (alias indirection) wrapping a KeysT.
type Id<T> = T;
type RW = {f: `${keyof Id<RW>}`};
declare const rw: RW;
const f2: 'f' = rw.f; // OK

// EvalT (indexed access) as a placeholder.
type Pick1 = {a: 'x', b: 'y'}['a'];
const s: `pre-${Pick1}` = 'pre-x'; // OK
