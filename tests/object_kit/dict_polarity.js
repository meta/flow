import * as React from 'react';

type InvariantDict = {[key: string]: number};
type ReadonlyDict = {readonly [key: string]: number};
type WriteonlyDict = {writeonly [key: string]: number};

type ConstantMapped<T extends {...}> = {[K in keyof T]: number};

// MakeExact, ReadOnly, Partial, Required, Rest(Omit), and ObjectMap applied to
// invariant indexers.
{
  declare const exact: $Exact<InvariantDict>;
  exact as {[key: string]: number}; // OK

  declare const readonly: Readonly<InvariantDict>;
  readonly as {readonly [key: string]: number}; // OK

  declare const partial: Partial<InvariantDict>;
  partial as {[key: string]: number}; // OK

  declare const required: Required<InvariantDict>;
  required as {[key: string]: number}; // OK

  declare const omit: Omit<InvariantDict, empty>;
  omit as {[key: string]: number}; // OK

  declare const mapped: ConstantMapped<InvariantDict>;
  mapped as {[key: string]: number}; // OK
}

// The preserving variants keep readonly indexers readonly.
{
  declare const exact: $Exact<ReadonlyDict>;
  exact as {[key: string]: number}; // ERROR

  declare const readonly: Readonly<ReadonlyDict>;
  readonly as {[key: string]: number}; // ERROR

  declare const partial: Partial<ReadonlyDict>;
  partial as {[key: string]: number}; // ERROR

  declare const required: Required<ReadonlyDict>;
  required as {[key: string]: number}; // ERROR

  declare const omit: Omit<ReadonlyDict, empty>;
  omit as {[key: string]: number}; // ERROR

  declare const mapped: ConstantMapped<ReadonlyDict>;
  mapped as {[key: string]: number}; // ERROR
}

// The preserving variants keep writeonly indexers writeonly.
{
  declare const exact: $Exact<WriteonlyDict>;
  exact as {[key: string]: unknown}; // ERROR

  declare const readonly: Readonly<WriteonlyDict>;
  readonly as {readonly [key: string]: unknown}; // OK

  declare const partial: Partial<WriteonlyDict>;
  partial as {[key: string]: unknown}; // ERROR

  declare const required: Required<WriteonlyDict>;
  required as {[key: string]: unknown}; // ERROR

  declare const omit: Omit<WriteonlyDict, empty>;
  omit as {[key: string]: unknown}; // ERROR

  declare const mapped: ConstantMapped<WriteonlyDict>;
  mapped as {[key: string]: number}; // ERROR
}

// Spread deliberately makes indexers invariant so its operands can be merged.
{
  declare const invariant: {...InvariantDict};
  invariant as {[key: string]: number}; // OK

  declare const readonly: {...ReadonlyDict};
  readonly as {[key: string]: number}; // OK

  declare const writeonly: {...WriteonlyDict};
  writeonly as {[key: string]: unknown}; // OK
}

// Rest(SpreadReversal) uses the neutral override during implicit instantiation.
{
  type FixedProps = {fixed: number};
  declare function reverseSpread<T extends {...}>(
    component: (props: {...T, ...FixedProps}) => void,
  ): T;

  const invariant = reverseSpread((props: InvariantDict) => {});
  invariant as {[key: string]: number}; // OK

  const readonly = reverseSpread((props: ReadonlyDict) => {});
  readonly as {[key: string]: number}; // OK

  const writeonly = reverseSpread((props: WriteonlyDict) => {});
  writeonly as {[key: string]: unknown}; // OK
}

// Component rest parameters run ReactCheckComponentConfig with a neutral override.
declare component InvariantRestComponent(...props: InvariantDict);
declare component ReadonlyRestComponent(...props: ReadonlyDict);
declare component WriteonlyRestComponent(...props: WriteonlyDict);

{
  declare const invariant: React.ComponentProps<typeof InvariantRestComponent>;
  invariant.arbitrary as number; // OK
  invariant.arbitrary = 0; // OK

  declare const readonly: React.ComponentProps<typeof ReadonlyRestComponent>;
  readonly.arbitrary as number; // OK
  readonly.arbitrary = 0; // OK

  declare const writeonly: React.ComponentProps<typeof WriteonlyRestComponent>;
  writeonly.arbitrary as unknown; // OK
  writeonly.arbitrary = 0; // OK
}

// Default props run Rest(ReactConfigMerge) with a neutral override.
class InvariantDefaultsComponent extends React.Component<InvariantDict> {
  static defaultProps: {foo: number} = {foo: 0};
}

class ReadonlyDefaultsComponent extends React.Component<ReadonlyDict> {
  static defaultProps: {foo: number} = {foo: 0};
}

class WriteonlyDefaultsComponent extends React.Component<WriteonlyDict> {
  static defaultProps: {foo: number} = {foo: 0};
}

{
  declare const invariant: React.ComponentProps<typeof InvariantDefaultsComponent>;
  invariant.arbitrary as number; // OK
  invariant.arbitrary = 0; // OK

  declare const readonly: React.ComponentProps<typeof ReadonlyDefaultsComponent>;
  readonly.arbitrary as number; // OK
  readonly.arbitrary = 0; // OK

  declare const writeonly: React.ComponentProps<typeof WriteonlyDefaultsComponent>;
  writeonly.arbitrary as unknown; // OK
  writeonly.arbitrary = 0; // OK
}

// JSX config checking runs ReactConfig with a neutral override, followed by
// ObjectRep, which preserves the indexer's polarity for key/ref lookup.
declare component DictComponent(...props: {[key: string]: unknown});

{
  declare const invariant: InvariantDict;
  <DictComponent {...invariant} />; // OK

  declare const readonly: ReadonlyDict;
  <DictComponent {...readonly} />; // OK

  declare const writeonly: WriteonlyDict;
  <DictComponent {...writeonly} />; // ERROR: writeonly indexer cannot provide a key
}
