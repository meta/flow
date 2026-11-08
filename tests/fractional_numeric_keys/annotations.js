// Object annotations and structural interfaces preserve fractional field types.
type Fractional = {0.5: string, 1.5: number, '-0.5': boolean}; // OK
interface FractionalInterface {
  0.5: string; // OK
  '1.5': number; // OK
  '-0.5': boolean; // OK
}
{
  declare const o: Fractional;
  o as FractionalInterface; // OK
  o[0.5] as string; // OK
  o['1.5'] as number; // OK
  o[-0.5] as boolean; // OK
  o['0.5'] as number; // ERROR
  ({0.5: 'half', 1.5: 15}) as Fractional; // ERROR: missing signed field
  ({0.5: 5, 1.5: 15, '-0.5': true}) as FractionalInterface; // ERROR
}

// Quoted covariant and contravariant properties keep their fractional names.
type Directional = {
  readonly '0.5': string, // OK
  writeonly '-0.5': number, // OK
};
{
  declare const o: Directional;
  o[0.5] as string; // OK
  o['0.5'] = 'new'; // ERROR
  o[-0.5] = 5; // OK
  o['-0.5'] as number; // ERROR
  o[-0.5] = 'new'; // ERROR
}

// Declared classes carry direct, quoted and computed members through instances.
declare class FractionalClass {
  0.5: string; // OK
  '1.5': number; // OK
  '-0.5': boolean; // OK
  2.5(value: string): number; // OK
  static [3.5]: string; // OK
}
{
  declare const o: FractionalClass;
  o['0.5'] as string; // OK
  o[1.5] as number; // OK
  o['-0.5'] as boolean; // OK
  o[2.5]('value') as number; // OK
  FractionalClass['3.5'] as string; // OK
  o[0.5] as number; // ERROR
  o[2.5](1); // ERROR
  o[3.5]; // ERROR: static field is not on an instance
}

// Indexed-access types resolve numeric and quoted forms to the same value type.
{
  declare const numeric: Fractional[0.5];
  declare const quoted: Fractional['0.5'];
  numeric as Fractional['0.5']; // OK
  quoted as Fractional[0.5]; // OK
  numeric as string; // OK
  quoted as number; // ERROR
  type Missing = Fractional[2.5]; // ERROR
  type NonCanonical = Fractional['0.50']; // ERROR
}

// Key utilities expose string property names, never numeric key values.
// $FlowFixMe[deprecated-utility]
type LegacyKeys<T> = $Keys<T>; // OK
{
  '0.5' as keyof Fractional; // OK
  '1.5' as LegacyKeys<Fractional>; // OK
  '-0.5' as keyof Fractional; // OK
  0.5 as keyof Fractional; // ERROR
  -0.5 as LegacyKeys<Fractional>; // ERROR
  '0.50' as keyof Fractional; // ERROR
  declare const o: Fractional;
  Object.keys(o) as ReadonlyArray<string>; // OK
  Object.keys(o) as ReadonlyArray<number>; // ERROR
  '0.5' as number; // ERROR
  '-0.5' as number; // ERROR
}

// Homomorphic mapped and readonly types preserve every name and its value.
type Identity<O extends {...}> = {[K in keyof O]: O[K]}; // OK
type Optional<O extends {...}> = {[K in keyof O]?: O[K]}; // OK
{
  declare const identity: Identity<Fractional>;
  identity as Fractional; // OK
  identity[0.5] as string; // OK
  identity['-0.5'] as boolean; // OK
  identity[1.5] as string; // ERROR
  declare const readonly: Readonly<Fractional>;
  readonly['0.5'] as string; // OK
  readonly[0.5] = 'new'; // ERROR
  declare const optional: Optional<Fractional>;
  optional['0.5'] as string | void; // OK
  optional[0.5] as string; // ERROR
  ({}) as Optional<Fractional>; // OK
  ({0.5: 5}) as Optional<Fractional>; // ERROR
  ({1.5: 15, '-0.5': true}) as Identity<Fractional>; // ERROR
}
