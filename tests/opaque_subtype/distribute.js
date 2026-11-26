// @flow

type A<T> = { readonly tag: 1, value: T } | { readonly tag: 2, value: T };

declare opaque type O: A<string>;

declare const o: O;
o as A<string>; // okay
o as A<number>; // error

declare opaque type Client<T> super T extends T;

declare const maybeStr: Client<?string>;
maybeStr as ?string; // okay - opaque maybe unwrapped via upper bound
maybeStr as string; // error - ?string not <: string

declare const optUndefined: Client<string | undefined>;
optUndefined as string | undefined; // okay
optUndefined as ?string; // okay - union upper retains undefined against nullable target
optUndefined as string | undefined | null; // okay

// Plain opaque type with nullable upper - not just generic Client
declare opaque type OMaybe: ?string;
declare const plainMaybe: OMaybe;
plainMaybe as ?string; // okay - plain opaque maybe
plainMaybe as string; // error - ?string not <: string

declare function nonnull<T>(value: ?T): T;
nonnull(plainMaybe) as string; // okay - infer through the nullable upper bound

declare opaque type OUnion: A<string>;
declare const plainUnion: OUnion;
plainUnion as A<string>; // okay - plain opaque with union upper
plainUnion as ?A<string>; // okay - plain opaque union upper to maybe target

// Preserve nominal identity when inferring through a nullable target.
type StringAlias = string;
declare opaque type StringID: StringAlias;
declare opaque type OtherStringID: StringAlias;

type Result<out T> = {readonly value: T};
declare function inferOptional<T>(
  validators: ReadonlyArray<(s: string) => ?T>,
): Result<?T>;
declare const parseStringID: (s: string) => ?StringID;
declare const parseOtherStringID: (s: string) => ?OtherStringID;
const inferredIDs: Result<?StringID | OtherStringID> = inferOptional([
  parseStringID,
  parseOtherStringID,
]); // okay - non-nullish aliases preserve nominal inference

declare opaque type OOptional: string | undefined;
declare const opaqueOptional: OOptional;
declare function takesOptional(value?: string): void;
takesOptional(opaqueOptional); // okay - opaque union upper retains undefined
