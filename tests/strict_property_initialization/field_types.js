type VoidableAlias = number | void;

class FieldsThatAcceptUndefined {
  anyValue: any;
  unknownValue: unknown;
  maybeValue: ?number;
  unionValue: number | void;
  voidValue: void;
  aliasValue: VoidableAlias;
}

class GenericField<T> {
  x: T; // error
}

class GenericVoidBound<T extends void> {
  x: T; // error: `T` may be `empty`, a narrower type than `void`
}

declare opaque type TextOrNothing super void | string;
declare opaque type Text super string;
declare opaque type Label extends ?string;
declare opaque type Nested<T> super Nested<Array<T>> | T;

class OpaqueFields {
  textOrNothing: TextOrNothing; // `undefined` is a `TextOrNothing`
  text: Text; // error
  label: Label; // error: every `Label` is a `?string`, but `undefined` is not a `Label`
  nested: Nested<string>; // Flow accepts `undefined` as a `Nested<string>`
}

type TextOrNothingAndNumberOrNothing = (void | string) & (void | number);

class IntersectionFields {
  both: TextOrNothingAndNumberOrNothing; // `undefined` is in both halves of the intersection
  oneSide: ?string & string; // error
}

declare const moreDefinitions: {[key: DefinitionKey]: number};
const definitions = {...moreDefinitions, alpha: 1};
type DefinitionKey = keyof typeof definitions;
const numberKey: DefinitionKey = 42; // Flow accepts any value as a `DefinitionKey`

class SelfReferencingKeys {
  key: DefinitionKey; // so `undefined` is a `DefinitionKey` too
}

class KeyOfParam<T extends {...}> {
  key: keyof T; // checked as `keyof empty`, which accepts any value
}

class GenericBrokenAnnotation<T> {
  value: {a: T}['missing']; // error
}
