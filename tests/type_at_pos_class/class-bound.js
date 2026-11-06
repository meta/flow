// @flow

class R<X> {}
type C<T extends R<any>> = Class<T>;
//   ^
