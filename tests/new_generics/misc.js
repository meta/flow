function a<X extends number>(x: X) {
  // AdderT
  x + x as X; // nope
  x + x as number;
  // UnaryMinusT
  -x as X; // nope
  -x as number;
  // AssertArithOperandT
  x * x as X;
  x * x as number;
  // coercion
  `blah ${x}` as string;
}

function b<X extends number, Y extends string>(x: X, y: Y) {
  //EqT, StrictEqT, CompartatorT
  (x == x) as boolean;
  (x === x) as boolean;
  (x == y) as boolean; // nope
  (x === y) as boolean; // nope, constant-condition error
  (x < x) as boolean;
  (x < y) as boolean; // nope
}

function c<S extends string, X extends {[key: string]: unknown}, Y extends Array<number>>(
  s: S,
  x: X,
  y: Y,
) {
  // AssertForInRHST
  for (const _ in x) {
  }
  // AssertBinaryIn[L|R]HST
  s in x;
  42 in y;
}

// MatchingPropT
function e<X extends {a: 'T', ...} | {a: 'S', ...}>(x: X) {
  if (x.a === 'T') {
  }
}

//TestPropT
function f<X>(x: Array<X>) {
  if (x[0] != null && x[0].id === 'a') {
    // This seems questionable but models current behavior for mixed
  }
}

function h<X extends [number]>(x: X): {[_K in keyof X]: string} {
  return ['a']; // error, mapped type doesn't have the unsoundness issue
}

// ToStringT
function gn<TType>(jsEnum: {[key: TType]: string, ...}) {
  Object.keys(jsEnum) as ReadonlyArray<TType>;
}

// KeysT
function gv<
  TFormData extends {...},
  TValidators extends {[K in keyof TFormData]: number},
>(
  data: TFormData,
  validators: TValidators,
): {[K in keyof TFormData]: ?string} {
  return Object.keys(data).reduce( // error: cannot satisfy generic mapped type
    <K extends keyof TFormData>(acc: {[K in keyof TFormData]: ?string}, k: K): {[K in keyof TFormData]: ?string} =>
      // $FlowExpectedError[unsafe-object-assign]
      Object.assign(acc, {[k]: validators[k](k, data)}),
    {},
  );
}

// More KeysT
function kt<TKey extends keyof {a: 42, ...}>(fieldName: TKey): void {
  if (fieldName) {
    return;
  }
}

// OptionalChain.run
function oc<
  T extends Readonly<{id: ?string, ...}>,
  L extends ?Readonly<{
    id: ?string,
    nextOneOne: ?{...},
    ...
  }>,
>(
  conversations: ReadonlyArray<T>,
  lookup: {[key: string]: L, ...},
): ReadonlyArray<T> {
  return conversations.filter(conversation => {
    const {id} = conversation;
    if (id == null) {
      return false;
    }
    return lookup[id]?.nextOneOne == null;
  });
}

// generic of key of generic
const cc = () => <D extends {...}, K extends keyof D>(key: K, data: D) => {
  const [click, view] = data[key];
};

// generic refined to empty
type ObjectKey = string | number;
type ObjectType<TK, TV> = {[key: TK]: TV, ...};
function ObjectFlip<TK extends ObjectKey, TV extends ?ObjectKey>(
  obj: ObjectType<TK, TV>,
  key: TK,
): {[key: string | number]: TK} {
  const flipped: {[key: string | number]: TK} = {};
  const value: ?TV = obj[key];
  if (value !== null && value !== undefined) {
    flipped[value] = key;
  }
  return flipped;
}

// Type Destructor
type LLETT =
  | {response: {account: {activities: number, ...}, ...}, ...}
  | {response: {account: {...}, ...}, ...};

type LLETR = LLETT['response'];
const elementType = <T extends LLETR>(data: T): T['account'] =>
  data.account;

const directAccount = <T extends LLETR>(data: T, otherData: LLETR): Readonly<T> =>
  otherData;

type t = {a: number, ...} | {v: string, ...};

class C<TConfig extends t> {
  _config: TConfig;
  __updateConfig(updater: (config: Readonly<TConfig>) => TConfig) {
    const config = updater(this._config);
  }
}

// Keys
function gejses<TMapKey extends string>(
  key: keyof {[key: TMapKey]: $FlowFixMe, ...},
): null {
  if (key) {
  }
  return null;
}

// Partial
function partial_test<T>(p: Partial<T>): T {
  return p; // error
}
