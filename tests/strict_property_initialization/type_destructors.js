class IndexedMissing<O extends {readonly label: ?string}> {
  value: O['label']; // error
}

class IndexedWithVoid<O extends {readonly label: ?string}> {
  value: O['label'] | void;
}

type VoidableObject = {readonly label: ?string};

class ConcreteIndexed {
  value: VoidableObject['label'];
}

class ValuesOfParam<O extends {readonly label: ?string}> {
  value: Values<O>; // error
}

class IndexedByKeysOfParam<O extends {readonly label: ?string}> {
  value: O[keyof O]; // error
}

class ParamOrVoid<T> {
  value: T | void;
}

class MaybeParam<T> {
  value: ?T;
}
