// Canonical finite numeric names are eligible for a number dictionary.
{
  const o = {
    '0.5': true,
    '1.5': false,
    '-0.5': true,
    '-1.5': false,
    '0': true,
    '1e-7': false,
    '1e+21': true,
    '9007199254740992': false,
    '1.7976931348623157e+308': true,
  }; // OK
  o as {readonly [key: number]: unknown}; // OK
  o as {[key: number]: boolean}; // OK
  o as {[key: number]: string}; // ERROR
  o as {readonly [key: 0.5 | 1.5]: boolean}; // ERROR: other keys are present
}

// Mutable unknown dictionaries use exactly unknown-valued fields and canonical keys.
type UnknownDictionary = {[key: number]: unknown}; // OK
{
  const values = {
    0.5: 'half' as unknown, // OK
    [1.5]: 15 as unknown, // OK
    '-0.5': true as unknown, // OK
  }; // OK
  const dict: UnknownDictionary = values; // OK
  const constructed: UnknownDictionary = {'0.5': 'half' as unknown}; // OK
  dict[0.5] as unknown; // OK
  dict['1.5'] as unknown; // OK
  dict[-0.5] as unknown; // OK
  dict[0.5] = 'new'; // OK
  dict['1.5'] = false; // OK
  constructed[-1.5] = 15; // OK
  dict['0.5'] as string; // ERROR
  dict['0.50'] = 'new'; // ERROR
  dict['-0'] = false; // ERROR
  const nonCanonical: UnknownDictionary = {'5e-1': true as unknown}; // ERROR
  const negativeZero: UnknownDictionary = {'-0': false as unknown}; // ERROR
}

// Noncanonical numeric-looking strings cannot hide behind an unknown value.
{
  ({'0.50': true}) as {readonly [key: number]: unknown}; // ERROR
  ({'01.5': true}) as {readonly [key: number]: unknown}; // ERROR
  ({'+0.5': true}) as {readonly [key: number]: unknown}; // ERROR
  ({' 0.5 ': true}) as {readonly [key: number]: unknown}; // ERROR
  ({'.5': true}) as {readonly [key: number]: unknown}; // ERROR
  ({'5e-1': true}) as {readonly [key: number]: unknown}; // ERROR
  ({'-0': true}) as {readonly [key: number]: unknown}; // ERROR
  ({'NaN': true}) as {readonly [key: number]: unknown}; // ERROR
  ({'Infinity': true}) as {readonly [key: number]: unknown}; // ERROR
  ({'0.5': true, unrelated: false}) as {readonly [key: number]: unknown}; // ERROR
  ({'0.50': true, '-0': false}) as {[key: string]: boolean}; // OK
}

// Literal dictionary lookup uses canonical names, including negative-zero normalization.
{
  declare const dict: {[key: number]: boolean};
  dict[0.5] as boolean; // OK
  dict['0.5'] as boolean; // OK
  dict[-1.5] as boolean; // OK
  dict['-1.5'] as boolean; // OK
  dict[-0] as boolean; // OK
  dict['0'] as boolean; // OK
  dict['1e+21'] as boolean; // OK
  dict['9007199254740992'] as boolean; // OK
  dict['0.5'] as string; // ERROR
  dict['0.50']; // ERROR
  dict['01.5']; // ERROR
  dict['+0.5']; // ERROR
  dict[' 0.5 ']; // ERROR
  dict['.5']; // ERROR
  dict['5e-1']; // ERROR
  dict['-0']; // ERROR
}

// A declared property wins over a broader number indexer, even through an alias.
type WithDeclared = {'0.5': 'declared', [key: number]: boolean}; // OK
{
  declare const o: WithDeclared;
  const alias: WithDeclared = o; // OK
  o[0.5] as 'declared'; // OK
  alias['0.5'] as 'declared'; // OK
  alias[1.5] as boolean; // OK
  alias[0.5] = 'declared'; // OK
  alias['1.5'] = false; // OK
  alias[0.5] as boolean; // ERROR
  alias['0.5'] = false; // ERROR
  alias[1.5] = 'declared'; // ERROR
  alias as {readonly [key: number]: unknown}; // OK
  alias as {readonly [key: number]: boolean | 'declared'}; // OK
  alias as {readonly [key: number]: boolean}; // ERROR: cannot skip the declared field
}

// Generic key inference sees property-name strings, not their numeric spellings.
{
  const o = {0.5: true, 1.5: false}; // OK
  declare const inferKey: <K, V>(o: {[K]: V}) => K;
  const key = inferKey(o); // OK
  key as string; // OK
  key as number; // ERROR
  declare const numericKey: <K extends number, V>(o: {[K]: V}) => K;
  numericKey(o); // ERROR
  declare const inferValue: <V>(o: {[number]: V}) => V;
  const value = inferValue(o); // OK
  value as boolean; // OK
  value as string; // ERROR
  type InferredKeys<O> = O extends {readonly [infer K]: unknown} ? K : empty;
  declare const inferred: InferredKeys<{0.5: boolean, '1.5': string}>;
  inferred as string; // OK
  inferred as number; // ERROR
}
