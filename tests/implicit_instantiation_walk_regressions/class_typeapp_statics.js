// A class applied to type arguments, `Class<C<T>>`, has the statics of `C`: passed as a callback,
// it is called through the static call signature of `C`, whose result `U` gets.
declare class Parser<T> {
  static (input: string): number;
  value: T;
}
declare const P: Class<Parser<boolean>>;

const parsed = ['1', '2'].map(P); // ok
parsed as Array<number>; // ok
parsed as Array<string>; // error
