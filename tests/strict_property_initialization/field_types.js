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
  nested: Nested<string>; // error: no `void` among `string`, `Array<string>`, `Array<Array<string>>`, ...
}
