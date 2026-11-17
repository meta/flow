//@flow

declare class Pr<out T> {}

type Tr = <Return>(f: () => Pr<Return>) => Return;

declare function aw<T>(p: Pr<T> | T): T;

(f => {
  return aw(f());
}) as Tr;
