// @flow

function fn1<T extends {p: any, ...}>(a: T, b: (t: T) => void): T['p'] {
  b(a);
  return a.p;
}

function fn2<T extends {p: any, ...}>(a: T, b: (t: T) => void): (p: T['p']) => void {
  return p => {
    a.p = p;
    return b(a);
  };
}

// These function calls are errors because in the second argument we have an
// empty object type, but $PropertyType expects that all bounds of T have a p
// property. Looking at these calls, erroring because we could not evaluate
// $PropertyType is overrestrictive. However, it is consistent with the
// implementation of type destructors.
//
// As we show in the following examples, there are some cases where we do need
// our type destructor to be restrictive. However, in this case we are
// over-restrictive.
fn1({p: 42}, (x: {...}) => {}); // over-restrictive in classic, not in LTI
fn2({p: 42}, (x: {...}) => {})('foo');

function fn3<T extends {p: any, ...}>(a: (t: T) => void): (t: Omit<T, 'p'>) => void {
  return x => a({...x, p: 42});
}

// In this case, we have a clear error that we want to catch. foo is passed a
// number, but we expect a string.
fn3((x: {foo: string, p: number, ...}) => {})({foo: 42});

fn3((x: {foo: number, ...}) => {})({foo: 42}); // intended as the paired OK case for `Omit<T, 'p'>` (T inferred to include p); currently ERROR — Flow does not widen T to satisfy `{p: any, ...}` from a callback whose param has no `p`

function fn4<T extends {p: any}>(a: (t: T) => void): (p: T['p']) => void {
  // We error here because the implementation of {p: empty} ~> empty currently
  // errors, but it would be a sound subtyping rule to allow.
  return p => a({p});
}

// Here is an error we need to catch.
fn4((x: {p: string}) => {})(42);

// However, we are overly restrictive and error here. This would be an OK
// program to accept.
fn4((x: {}) => {})(42);
