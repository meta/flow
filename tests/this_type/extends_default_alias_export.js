interface I<T> {
  v: T;
}
type X<T = number> = I<T>;
export interface J extends X<string> {}
