type Params = Readonly<{
  'a': boolean,
  'b': boolean,
}>;

declare const params: Params;

const test1 = <T extends keyof typeof params>(
  key: T,
): Params => {
  return {...params, [key]: true}; // ok: key set is normalized to StrT. error: indexed incompatible with Params
};

const test2 = <T extends keyof Params>(
  key: T,
): Params => {
  return {...params, [key]: true}; // ok: key set is normalized to StrT. error: indexed incompatible with Params
};

const test3 = <T extends 'a' | 'b'>(
  key: T,
): Params => {
  return {...params, [key]: true}; // ok: key set is normalized to StrT. error: indexed incompatible with Params
};

const test4 = (
  key: 'a' | 'b',
): {[key: string]: boolean, ...} => {
  return {[key]: true}; // okay
}
