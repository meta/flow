// Only a `unique symbol` value resolves to a symbol key in a bracketed key
// position. A key that names a type is an index signature keyed by that type
// and never becomes a symbol key: a type alias is one as written, and a class
// or an enum, whose name also binds a value, is one in the labeled form.

class C {}
type TC = {[key: C]: number};
declare const tc: TC;
declare const c: C;
tc[c] as number; // OK: index signature keyed by the instance type

enum E {
  A,
  B,
}
type TE = {[key: E]: number};
declare const te: TE;
te[E.A] as number; // OK: index signature keyed by the enum type

type K = string;
type TK = {[key: K]: number};
declare const tk: TK;
tk['x'] as number; // OK: string index signature
