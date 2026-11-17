//@flow
function f<X extends string, Y extends 'lit', Z extends 'prop'>(
  x: X,
  y: Y,
  z: Z,
  a: {[key: X]: string},
  b: {prop: number, ...},
) {
  a[x] = 'hi';
  a['foo'] = 'hi'; // nope
  a[x] as string;
  a['foo']; // allowed as a result of assignment above
  b[x] = 42; // nope
  b[y] = 42; // nope
  b[z] = 42;
  b[x]; // nope
  b[y]; // nope
  b[z];
}
