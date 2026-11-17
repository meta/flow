declare let a: (x: number) => unknown;
declare let b: (x: number) => unknown;
declare let c: (x: number) => unknown;
declare let d: (x: number) => unknown;

[a, [b, [c, {d}]]] = [
   (n) => n as empty, // error: num ~> empty
   [
      (n) => n as empty, // error: num ~> empty
      [
        (n) => n as empty, // error: num ~> empty
        {d: (n) => n as empty} // error: num ~> empty
      ]
   ]
];
({a, b} = {
  a: (n) => n as empty, // error: num ~> empty
  b: (n) => n as empty, // error: num ~> empty
})

declare let e: ReadonlyArray<(x: number) => unknown>;
declare let f: {[key: string]: (x: number) => unknown};
[...e] = [(n) => n as empty]; // error: num ~> empty
({...f} = {hello: (n) => n as empty}); // error: num ~> empty

[, b] = [1, (n) => {}]; // error: missing-local-annot (array hole pattern is unsupported)
