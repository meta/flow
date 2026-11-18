// --- No interpolation ---

type Plain = `hello`;
"hello" as Plain; // OK
"world" as Plain; // ERROR

// --- Inline literal interpolation: string ---

type TS1 = `${'abc'}`;
'abc' as TS1; // OK
'def' as TS1; // ERROR

// --- Inline literal interpolation: number ---

type TS2 = `${42}`;
'42' as TS2; // OK
'43' as TS2; // ERROR

// --- Inline literal interpolation: boolean ---

type Bool = `is_${true | false}`;
"is_true" as Bool; // OK
"is_false" as Bool; // OK

type TS3 = `${true}`;
'true' as TS3; // OK
'false' as TS3; // ERROR

// --- Inline literal interpolation: bigint ---

type Big = `big_${0n | 1n}`;
"big_0" as Big; // OK
"big_1" as Big; // OK
"big_2" as Big; // ERROR

type TS4 = `${100n}`;
'100' as TS4; // OK
'101' as TS4; // ERROR

// `${bigint}` lexical body accepts `-0` (tsc 5.9.3: `isBigInt('-0')` is
// treated as a valid bigint placeholder body).
type BigBody = `${bigint}`;
'-0' as BigBody; // OK
'0' as BigBody; // OK
'-1' as BigBody; // OK
'+1' as BigBody; // ERROR (leading + sign not allowed)
'01' as BigBody; // ERROR (no leading zero in decimal)
'1_000' as BigBody; // ERROR (no separators)

// --- Number edge cases: negative, decimal, large ---

type NegNum = `${-5}`;
'-5' as NegNum; // OK
'5' as NegNum; // ERROR

type DecNum = `${1.5}`;
'1.5' as DecNum; // OK
'1' as DecNum; // ERROR

type LargeNum = `${9007199254740991}`;
'9007199254740991' as LargeNum; // OK

// Numeric stringification: hex literal -> decimal value, separators stripped.
type HexNum = `${0xff}`;
'255' as HexNum; // OK
'0xff' as HexNum; // ERROR

type SepNum = `${1_000}`;
'1000' as SepNum; // OK
'1_000' as SepNum; // ERROR

// --- Type alias interpolation ---

type World = "world";
type Greeting = `hello ${World}`;
"hello world" as Greeting; // OK
"hello world!" as Greeting; // ERROR

// --- Empty string edge cases ---

type Empty = `${''}`;
'' as Empty; // OK

type EmptyInterp = `${'' | 'a'}`;
'' as EmptyInterp; // OK
'a' as EmptyInterp; // OK
'b' as EmptyInterp; // ERROR

// --- Single-position union distribution ---

type EmailLocaleIDs = "welcome_email" | "email_heading";
type FooterLocaleIDs = "footer_title" | "footer_sendoff";
type AllLocaleIDs = `${EmailLocaleIDs | FooterLocaleIDs}_id`;
"welcome_email_id" as AllLocaleIDs; // OK
"email_heading_id" as AllLocaleIDs; // OK
"footer_title_id" as AllLocaleIDs; // OK
"footer_sendoff_id" as AllLocaleIDs; // OK
"unknown_id" as AllLocaleIDs; // ERROR

type E1 = `${'foo' | 'bar' | 'baz'}Changed`;
'fooChanged' as E1; // OK
'barChanged' as E1; // OK
'bazChanged' as E1; // OK
'quxChanged' as E1; // ERROR
'foo' as E1; // ERROR

// --- Multi-position unions (cross product with delimiter) ---

type AB = "a" | "b";
type CD = "c" | "d";
type ABCD = `${AB}_${CD}`;
"a_c" as ABCD; // OK
"b_d" as ABCD; // OK
"a_e" as ABCD; // ERROR

// --- Number literal union ---

type Num = `value_${1 | 2}`;
"value_1" as Num; // OK
"value_2" as Num; // OK
"value_3" as Num; // ERROR

type Coord = `${0 | 1 | 2},${0 | 1 | 2}`;
'0,0' as Coord; // OK
'1,2' as Coord; // OK
'2,1' as Coord; // OK
'3,0' as Coord; // ERROR

// --- Adjacent interpolations (empty quasis) ---

type Concat = `${AB}${CD}`;
"ac" as Concat; // OK
"bd" as Concat; // OK

type D1 = `${'ab'}${'ab'}`;
'abab' as D1; // OK
'abba' as D1; // ERROR

type D2 = `${'a' | 'b'}${'a' | 'b'}`;
'aa' as D2; // OK
'bb' as D2; // OK
'ab' as D2; // OK
'ba' as D2; // OK

type Adjacent3 = `${'a' | 'b'}${'c' | 'd'}${'e' | 'f'}`;
'ace' as Adjacent3; // OK
'bdf' as Adjacent3; // OK
'adf' as Adjacent3; // OK
'bcf' as Adjacent3; // OK
'acg' as Adjacent3; // ERROR

// --- Four-position cross product ---

type Deep3 = `${'a' | 'b'}-${'c' | 'd'}-${'e' | 'f'}-${'g' | 'h'}`;
'a-c-e-g' as Deep3; // OK
'b-d-f-h' as Deep3; // OK
'a-d-e-h' as Deep3; // OK
'a-c-e-i' as Deep3; // ERROR

// --- Nested template literals ---

type TL1<T extends string> = `a${T}b${T}c`;
type TL2<U extends string> = TL1<`x${U}y`>;
type TL3 = TL2<'o'>;  // 'axoybxoyc'
'axoybxoyc' as TL3; // OK
'axybxyc' as TL3; // ERROR
'aobyoc' as TL3; // ERROR

type Deep = `${`${'a' | 'b'}${'x' | 'y'}`}_end`;
'ax_end' as Deep; // OK
'by_end' as Deep; // OK
'ay_end' as Deep; // OK
'bx_end' as Deep; // OK
'cx_end' as Deep; // ERROR

// --- Type alias union references ---

type AB2 = 'a' | 'b';
type CD2 = 'c' | 'd';
type AliasTemplate = `${AB2}-${CD2}`;
'a-c' as AliasTemplate; // OK
'b-d' as AliasTemplate; // OK
'a-e' as AliasTemplate; // ERROR

// --- Mixed string + number unions in cross product ---

type MixedInterp = `item_${'a' | 'b'}_${1 | 2}`;
'item_a_1' as MixedInterp; // OK
'item_b_2' as MixedInterp; // OK
'item_a_2' as MixedInterp; // OK
'item_b_1' as MixedInterp; // OK
'item_c_1' as MixedInterp; // ERROR
'item_a_3' as MixedInterp; // ERROR

// --- Non-literal interpolation ---

type WithString = `hello ${string}`;
"hello anything" as WithString; // OK

type Mixed = `val_${string | number | boolean}`;
'val_hello' as Mixed; // OK
'val_42' as Mixed; // OK
'val_true' as Mixed; // OK

type JustInterp = `${string}`;
'anything' as JustInterp; // OK

// --- ${number}<suffix> pattern: matches concrete strings whose interpolated
//     portion is a valid lexical number form ---

type PixelValue = `${number}px`;
'42px' as PixelValue; // OK
'-5px' as PixelValue; // OK
'+5px' as PixelValue; // OK
'1.5px' as PixelValue; // OK
'.5px' as PixelValue; // OK
'1.px' as PixelValue; // OK
'1e10px' as PixelValue; // OK
'1.5e-3px' as PixelValue; // OK
'0x10px' as PixelValue; // OK (hex)
'0o10px' as PixelValue; // OK (octal)
'0b10px' as PixelValue; // OK (binary)
// Whitespace at boundaries is accepted: Number(' 42') === 42, finite.
'  42px' as PixelValue; // OK (leading whitespace)
'42 px' as PixelValue; // OK (trailing whitespace before suffix)
' px' as PixelValue; // OK (whitespace-only number portion: Number(' ') === 0)
'NaNpx' as PixelValue; // ERROR
'Infinitypx' as PixelValue; // ERROR
'1_000px' as PixelValue; // ERROR (separators not allowed)
'pxpx' as PixelValue; // ERROR (empty number portion)
'1.5.5px' as PixelValue; // ERROR (multiple decimals)
'1epx' as PixelValue; // ERROR (missing exponent digits)
'--5px' as PixelValue; // ERROR (double sign)
'0xff_00px' as PixelValue; // ERROR (underscore in hex)
'.px' as PixelValue; // ERROR (dot only)
// Signed radix forms are not valid Number() coercions: Number('-0xff') === NaN.
'-0x10px' as PixelValue; // ERROR (signed hex)
'+0o10px' as PixelValue; // ERROR (signed oct)
'-0b10px' as PixelValue; // ERROR (signed bin)

type Percentage = `${number}%`;
'50%' as Percentage; // OK
'-12.5%' as Percentage; // OK
'%' as Percentage; // ERROR

// --- ${number}.${number}.${number} (multi-position number interpolation) ---

type SemVerLike = `${number}.${number}.${number}`;
'1.2.3' as SemVerLike; // OK
'10.20.30' as SemVerLike; // OK
'0.0.0' as SemVerLike; // OK
'1.2' as SemVerLike; // ERROR (missing third)
'a.b.c' as SemVerLike; // ERROR

// --- ${bigint} ---

type BigSuf = `${bigint}n`;
'42n' as BigSuf; // OK
'-5n' as BigSuf; // OK
'0n' as BigSuf; // OK
'0xfn' as BigSuf; // OK (hex bigint)
'0o10n' as BigSuf; // OK
'0b10n' as BigSuf; // OK
'1.5n' as BigSuf; // ERROR (decimal not bigint)
'+5n' as BigSuf; // ERROR (no leading + for bigint)
'05n' as BigSuf; // ERROR (no leading zero)

// --- ${boolean} prefix/suffix ---

type BoolPrefix = `is_${boolean}`;
'is_true' as BoolPrefix; // OK
'is_false' as BoolPrefix; // OK
'is_maybe' as BoolPrefix; // ERROR

// --- ${null} and ${undefined} (stringified to "null"/"undefined") ---

type NullInterp = `pre_${null}_post`;
'pre_null_post' as NullInterp; // OK
'pre_other_post' as NullInterp; // ERROR

type UndefInterp = `<${undefined}>`;
'<undefined>' as UndefInterp; // OK
'<null>' as UndefInterp; // ERROR

type MixedNullable = `${null | undefined | 'x'}`;
'null' as MixedNullable; // OK
'undefined' as MixedNullable; // OK
'x' as MixedNullable; // OK
'y' as MixedNullable; // ERROR

// --- Template literal to string assignability ---

type Greeting2 = `hello ${string}`;
declare const g: Greeting2;
g as string; // OK (template literal is subtype of string)

type SemVer = `${number}.${number}.${number}`;
declare const ver: SemVer;
ver as string; // OK

// --- Narrower template <: wider template ---

type Wider = `prefix_${string}`;
type Narrower = `prefix_${'a' | 'b'}`;
declare const narrow: Narrower;
narrow as Wider; // OK (narrower is subtype of wider)

// --- Placeholder coercion to string in template subtyping ---
// All template-literal placeholders (number, bigint, boolean, null, undefined)
// coerce to string at runtime, so they're subtypes of `${string}`.

declare const num_t: `${number}`;
num_t as `${string}`; // OK (number coerces to string)

declare const num_around: `pre${number}post`;
num_around as `pre${string}post`; // OK

declare const big_t: `${bigint}`;
big_t as `${string}`; // OK

declare const bool_t: `${boolean}`;
bool_t as `${string}`; // OK

declare const null_t: `${null}`;
null_t as `${string}`; // OK

declare const undef_t: `${undefined}`;
undef_t as `${string}`; // OK

// Union placeholders distribute: every member must coerce to string.
declare const union_nb: `${number | boolean}`;
union_nb as `${string}`; // OK (both number and boolean coerce)

declare const union_full: `${string | number | bigint | boolean | null | undefined}`;
union_full as `${string}`; // OK

// --- Adjacent placeholders collapse against `${string}` ---
// LHS with multiple coerce-to-string placeholders is a subtype of `${string}`
// because the runtime result is always a string.

declare const nn: `${number}${number}`;
nn as `${string}`; // OK

declare const nbn: `${number}${boolean}${null}`;
nbn as `${string}`; // OK

declare const adj_around: `pre_${number}${number}_post`;
adj_around as `pre_${string}_post`; // OK

// --- Structural normalization via concrete-string placeholders ---
// `${number}${'px'}` should be subtype of `${number}px` (same runtime form).

declare const num_px_interp: `${number}${'px'}`;
num_px_interp as `${number}px`; // OK

// --- Template in union with other types ---

type StringOrTemplate = string | `prefix_${number}`;
'anything' as StringOrTemplate; // OK (string covers all)

type NumOrTemplate = number | `hello_${string}`;
42 as NumOrTemplate; // OK
'hello_world' as NumOrTemplate; // OK

// --- Special characters in quasis ---

type WithDot = `${string}.${string}`;
'hello.world' as WithDot; // OK

type WithBrackets = `{${string}}`;
'{hello}' as WithBrackets; // OK

type WithSlashes = `/${string}/${string}`;
'/foo/bar' as WithSlashes; // OK

type Bracketed = `[${string}]`;
'[hello]' as Bracketed; // OK
'[42]' as Bracketed; // OK
'hello' as Bracketed; // ERROR

// --- URL pattern (longer quasis) ---

type LongQuasi = `https://${'example.com' | 'test.org'}/api/v1/${'users' | 'posts'}`;
'https://example.com/api/v1/users' as LongQuasi; // OK
'https://test.org/api/v1/posts' as LongQuasi; // OK
'https://example.com/api/v1/comments' as LongQuasi; // ERROR
'http://example.com/api/v1/users' as LongQuasi; // ERROR

// --- Single union member ---

type OneOption = `prefix_${'only'}`;
'prefix_only' as OneOption; // OK
'prefix_other' as OneOption; // ERROR

// --- Spacing pattern (real-world) ---

export type Spacing =
    | `0`
    | `${number}px`
    | `${number}rem`
    | `s${1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 17 | 18 | 19 | 20}`;

'0' as Spacing; // OK
'12px' as Spacing; // OK
'3.14rem' as Spacing; // OK
'-2px' as Spacing; // OK (negative number)
's1' as Spacing; // OK
's20' as Spacing; // OK
'1' as Spacing; // ERROR: no unit
'px' as Spacing; // ERROR: no number
'12em' as Spacing; // ERROR: wrong unit
's0' as Spacing; // ERROR: 0 not in 1..20
's21' as Spacing; // ERROR: out of range
'red' as Spacing; // ERROR: unrelated string

export type SpacingShorthand =
    | `${Spacing} ${Spacing}`
    | `${Spacing} ${Spacing} ${Spacing}`
    | `${Spacing} ${Spacing} ${Spacing} ${Spacing}`;

'0 0' as SpacingShorthand; // OK (2-tuple)
'12px 3rem' as SpacingShorthand; // OK (2-tuple, mixed units)
's5 0 12px' as SpacingShorthand; // OK (3-tuple)
'0 1px 2rem s3' as SpacingShorthand; // OK (4-tuple, mixed)
'0' as SpacingShorthand; // ERROR: 1-tuple not allowed
'0 0 0 0 0' as SpacingShorthand; // ERROR: 5-tuple not allowed
'0,0,0' as SpacingShorthand; // ERROR: wrong delimiter
'foo bar' as SpacingShorthand; // ERROR: invalid Spacing values
's21 0' as SpacingShorthand; // ERROR: invalid Spacing on the left

// --- Complexity limits ---

// Over limit (10^5 > 10,000 limit) - ERROR
type TooComplex = `${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}`; // ERROR: too complex

// Way over limit (10^19 raw product) - ERROR; must not bypass the guard via
// OCaml-int overflow on the size calculation.
type FarTooComplex = `${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}${0|1|2|3|4|5|6|7|8|9}`; // ERROR: too complex

// Under limit (10^3 = 1,000, under limit) - OK
type SmallSet = 'a' | 'b' | 'c' | 'd' | 'e' | 'f' | 'g' | 'h' | 'i' | 'j';
type AtLimit = `${SmallSet}${SmallSet}${SmallSet}`;
'abc' as AtLimit; // OK
'jjj' as AtLimit; // OK
'aaa' as AtLimit; // OK
'abk' as AtLimit; // ERROR

// --- Placeholder type constraint: must be string|number|bigint|boolean|null|undefined ---

type BadObj = `${{a: 1}}`; // ERROR: object placeholder
type BadFunc = `${() => void}`; // ERROR: function placeholder
type BadArr = `${Array<string>}`; // ERROR: array placeholder
type BadMixed = `${unknown}`; // ERROR: unknown placeholder
type BadInUnion = `${string | {a: 1}}`; // ERROR: invalid union member

// Soundness: invalid placeholders reached via EvalT. The syntactic walker
// can't see through indexed access or property access, so these are caught
// by a deferred post-inference deep check.
type BadIndexedAccess = `${[{a: 1}][0]}`; // ERROR: object via array indexing
type BadPropAccess = `${{f: {a: 1}}['f']}`; // ERROR: object via property access

// Allowed forms
type OkAll = `${string | number | bigint | boolean | null | undefined}`;
type OkGeneric<T extends string | number> = `${T}`;
type OkNested = `${`${number}px`}`;

// Explicit non-primitive generic bounds are also invalid placeholders.
// Unconstrained `<T>` is left alone because its representation in Flow is
// indistinguishable from `infer P`, which TS implicitly accepts.
type BadGenericObj<T extends {a: 1}> = `${T}`; // ERROR: object-bounded generic
type BadGenericArr<T extends Array<string>> = `${T}`; // ERROR: array-bounded generic
type BadKeysOfGeneric<T> = `${keyof T}`; // ERROR: a key of `T` may be a symbol
type OkStringKeysOfGeneric<T> = `${keyof T & string}`;

// --- Number body accepts JS whitespace, including multi-byte codepoints ---
// `Number(' 42')` returns 42 (NBSP is JS WhiteSpace), so these are accepted.
type NumPx = `${number}px`;
' 42px' as NumPx; // OK (ASCII space + 42 + px)
' 42px' as NumPx; // OK (NBSP + 42 + px)
'　1px' as NumPx; // OK (ideographic space U+3000 + 1 + px)
'abcpx' as NumPx; // ERROR (not numeric body)

// --- Un-normalized prefix/suffix subtyping ---
// When a generic placeholder is substituted with a concrete-string type, the
// resulting TemplateLiteralT has un-normalized quasis (e.g. quasis=["","",""])
// even though it represents the same string shape as the normalized form.
// The prefix- and suffix-extension subtyping rules must normalize via
// fold_concrete_placeholders before pattern matching.

// Un-normalized substitution still matches prefix-extension subtyping.
declare const a1: `${'data-foo-'}${string}`;
const b1: `data-${string}` = a1; // OK

// Un-normalized substitution still matches suffix-extension subtyping.
declare const a2: `${string}${'-end-foo'}`;
const b2: `${string}-foo` = a2; // OK

// Negative case: prefix doesn't match.
declare const c: `${'other-'}${string}`;
const d: `data-${string}` = c; // ERROR
