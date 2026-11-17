// The type that allows additional properties is printed

// Indexer: the key type
{
  declare const x: {[key: string]: number};

  match (x) {
    {foo: _} => {} // ERROR
    _ => {}
  }
}

// Array
{
  declare const x: Array<number>;

  match (x) {
    {length: _} => {} // ERROR
  }
}

// `unknown`
{
  declare const x: unknown;

  match (x) {
    {type: 'a'} => {} // ERROR
    _ => {}
  }
}

// `any`
{
  declare const x: any;

  match (x) {
    {type: 'a'} => {} // ERROR
    _ => {}
  }
}

// Optional property already matched: the object type
{
  declare const x: {a?: 1};

  match (x) {
    {a: _} => {}
    {} => {} // ERROR
  }
}
