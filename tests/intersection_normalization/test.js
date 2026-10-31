// @flow

type MergedObject = {a: string, shared: string, ...} & {
  b: number,
  shared: 'shared',
  ...,
};

declare const mergedObject: MergedObject;
mergedObject.a as string;
mergedObject.b as number;
mergedObject.shared as 'shared';
mergedObject.shared as 'other'; // ERROR: the overlapping property was normalized

type MergedReadonlyObject =
  {readonly left: string, ...} & {readonly right: number, ...};
declare const mergedReadonlyObject: MergedReadonlyObject;
mergedReadonlyObject.left = 'changed'; // ERROR: normalization preserves property polarity

declare const ComputedLeft: 'computedLeft';
declare const ComputedRight: 'computedRight';
type MergedComputedObject = {
  readonly [typeof ComputedLeft]: string,
  ...,
} & {
  readonly [typeof ComputedRight]: number,
  ...,
};
declare const mergedComputedObject: MergedComputedObject;
mergedComputedObject.computedLeft as string;
mergedComputedObject.computedRight as number;

type ComputedUnionKeys = 'computedUnionLeft' | 'computedUnionRight';
declare const ComputedExtra: 'computedExtra';
type MergedComputedUnionObject = {
  readonly [ComputedUnionKeys]: string,
} & {
  readonly [typeof ComputedExtra]: number,
  ...,
};
declare const mergedComputedUnionObject: MergedComputedUnionObject;
mergedComputedUnionObject.computedUnionLeft as string;
mergedComputedUnionObject.computedUnionRight as string;
mergedComputedUnionObject.computedExtra as number;

interface LeftInterface {
  left: string;
  nested: {fromLeft: string, ...};
}

interface RightInterface {
  right: number;
  nested: {fromRight: number, ...};
}

type MergedInterfaces = LeftInterface & RightInterface;
declare const mergedInterfaces: MergedInterfaces;
mergedInterfaces.left as string;
mergedInterfaces.right as number;
mergedInterfaces.nested.fromLeft as string;
mergedInterfaces.nested.fromRight as number;

interface ChildOfMergedInterfaces extends MergedInterfaces {
  child: boolean;
}

declare const childOfMergedInterfaces: ChildOfMergedInterfaces;
childOfMergedInterfaces.left as string;
childOfMergedInterfaces.right as number;
childOfMergedInterfaces.child as boolean;

interface BaseInterface {
  base: boolean;
}

interface DerivedInterface extends BaseInterface {
  derived: string;
}

type MergedObjectAndInterface = DerivedInterface & {object: number, ...};
declare const mergedObjectAndInterface: MergedObjectAndInterface;
mergedObjectAndInterface.base as boolean;
mergedObjectAndInterface.derived as string;
mergedObjectAndInterface.object as number;

interface GenericInterface<T> {
  genericValue: T;
}

type MergedGenericInterface =
  GenericInterface<string> & {object: number, ...};
declare const mergedGenericInterface: MergedGenericInterface;
mergedGenericInterface.genericValue as string;
mergedGenericInterface.genericValue as number; // ERROR: generic interfaces retain their type arguments
mergedGenericInterface.object as number;

interface DerivedFromGenericInterface extends GenericInterface<string> {
  derived: boolean;
}

type MergedDerivedFromGenericInterface =
  DerivedFromGenericInterface & {object: number, ...};
declare const mergedDerivedFromGenericInterface: MergedDerivedFromGenericInterface;
mergedDerivedFromGenericInterface.genericValue as string;
mergedDerivedFromGenericInterface.genericValue as number; // ERROR: generic interface ancestors retain their type arguments
mergedDerivedFromGenericInterface.derived as boolean;
mergedDerivedFromGenericInterface.object as number;

type LeftRecursive = {
  next: LeftRecursive,
  left: string,
  ...,
};
type RightRecursive = {
  next: RightRecursive,
  right: number,
  ...,
};
type MergedRecursive = LeftRecursive & RightRecursive;

declare const mergedRecursive: MergedRecursive;
mergedRecursive.next.next.left as string;
mergedRecursive.next.next.right as number;
mergedRecursive.next.next.left as number; // ERROR: recursive normalization terminated

interface LeftRecursiveInterface {
  next: LeftRecursiveInterface;
  left: string;
}

interface RightRecursiveInterface {
  next: RightRecursiveInterface;
  right: number;
}

type MergedRecursiveInterfaces =
  LeftRecursiveInterface & RightRecursiveInterface;
declare const mergedRecursiveInterfaces: MergedRecursiveInterfaces;
mergedRecursiveInterfaces.next.next.left as string;
mergedRecursiveInterfaces.next.next.right as number;

type MergedDictionary = {
  [key: string]: {fromLeft: string, ...},
  ...,
} & {
  [key: string]: {fromRight: number, ...},
  ...,
};

declare const mergedDictionary: MergedDictionary;
mergedDictionary.key.fromLeft as string;
mergedDictionary.key.fromRight as number;

type StringLiteral = string & 'literal';
'literal' as StringLiteral;
'other' as StringLiteral; // ERROR: string singleton is preserved

type NumberLiteral = number & 42;
42 as NumberLiteral;
41 as NumberLiteral; // ERROR: number singleton is preserved

type BigIntLiteral = bigint & 42n;
42n as BigIntLiteral;
41n as BigIntLiteral; // ERROR: bigint singleton is preserved

type BooleanLiteral = boolean & true;
true as BooleanLiteral;
false as BooleanLiteral; // ERROR: boolean singleton is preserved

type ImpossibleLiterals = 'left' & 'right';
declare const impossibleLiterals: ImpossibleLiterals;
impossibleLiterals as empty;

type ImpossibleDomains = string & number;
declare const impossibleDomains: ImpossibleDomains;
impossibleDomains as empty;

type UnsupportedFunctions =
  ((value: string) => number) & ((value: number) => string);
declare const unsupportedFunctions: UnsupportedFunctions;
unsupportedFunctions('value') as number;
unsupportedFunctions(42) as string;

type GenericIntersection<T> = T & {generic: string, ...};
declare const genericIntersection: GenericIntersection<{other: number, ...}>;
genericIntersection.generic as string;
genericIntersection.other as number;

interface CyclicInterfaceLeft extends CyclicInterfaceRight {
  cycleLeft: string;
}

interface CyclicInterfaceRight extends CyclicInterfaceLeft {
  cycleRight: number;
}

type MergedCyclicInterfaces = CyclicInterfaceLeft & CyclicInterfaceRight;
declare const mergedCyclicInterfaces: MergedCyclicInterfaces;
mergedCyclicInterfaces.cycleLeft as string;
mergedCyclicInterfaces.cycleRight as number;

type InvalidExactIntersection = {|exactLeft: string|} & {|
  exactRight: number,
|}; // ERROR: each exact object lacks the other's required property

declare const invalidExactIntersection: InvalidExactIntersection;
invalidExactIntersection.exactLeft as string;
invalidExactIntersection.exactRight as number;

type SameExactObjects = {foo: string} & {foo: string};
declare const sameExactObjects: SameExactObjects;
sameExactObjects.foo as string;
sameExactObjects.foo as number; // ERROR: identical exact objects normalize

type ExactAndInexact = {|exactShared: string, exactOnly: number|} & {
  exactShared: 'shared',
  ...,
};
declare const exactAndInexact: ExactAndInexact;
exactAndInexact.exactShared as 'shared';
exactAndInexact.exactShared as 'other'; // ERROR: the overlapping property was normalized
exactAndInexact.exactOnly as number;

type DisjointOptionalExact = {|optionalLeft?: string|} & {|
  optionalRight?: number,
|};
declare const disjointOptionalExact: DisjointOptionalExact;
disjointOptionalExact.optionalLeft;

type ExactWithDictionary = {|exactKey: number|} & {[key: string]: number, ...};
declare const exactWithDictionary: ExactWithDictionary;
exactWithDictionary.exactKey as number;
exactWithDictionary.otherKey;

type ExactMissingInexactRequired = {|exactKey: string|} & {
  inexactRequired: number,
  ...,
}; // ERROR: the exact object lacks a required property
