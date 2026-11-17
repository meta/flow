//@flow

{
  const x: {[key: 'A' | 'B']: (x: number) => number} = {[42]: v => v}; // ERROR
}
{
  const x: {['A']: (x: number) => number} = {[42]: v => v}; // ERROR
}
{
  const x: {...} | {...} = {a: v => v};
}
{
  const x: {[key: number]: (x: number) => number} = {[42]: v => v};
}
{
  type StrCb = (x: string) => void;
  type NumCb = (x: number) => void;
  type Cb = Readonly<{cb?: ?(NumCb | StrCb)}>;
  declare const fn: (config: Cb) => {};
  fn({
    cb: x => {
      x as string | number;
    },
  }); // okay
  fn({
    cb: x => {
      x as string;
    },
  }); // error number ~> string
}
{
  declare const j: {
    mock(moduleFactory?: any): void,
  };
  j.mock(() => ({f: n => {}}));
}
{
  type A<T> = (x: T) => void;
  type B<T> = A<T> | A<number>;
  const x: B<string> = x => {
    x as string;
  }; // error number ~> string
}

{
  declare const mixedArray1: Array<string> | Array<number>;
  const list = mixedArray1.slice();
  list.forEach(elem => {
    elem as empty; // error number ~> empty
  });
}
{
  declare const mixedArray2: Array<empty> | Array<number>;
  const list = mixedArray2.slice();
  list.forEach(elem => {
    elem as empty; // error number ~> empty
  });
}
{
  declare const mixedArray3: Array<number> | Array<empty>;
  mixedArray3.map(x => {
    x as empty; // error number ~> empty
  });
}
{
  declare const emptyArray: Array<empty>;
  emptyArray.forEach(x => {}); // okay
}
