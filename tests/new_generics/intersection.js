//@flow

function f1<X extends number & string, Y extends X & boolean>(x: X, y: Y) {
  x as number & string;
  x as number;
  x as X;
  y as X;
  x as number & string & boolean; // nope
}

function f2<X extends {readonly a: number} & {readonly b: string}>(x: X) {
  x as X & {readonly a: number};
  x as X & {readonly c: number}; // nope
}
