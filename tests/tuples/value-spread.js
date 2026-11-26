// Basic
{
  declare const x: [1, 2];
  const y = [0, ...x, 3] as const;
  y as Readonly<[0, 1, 2, 3]>; // OK
  y as Readonly<[0, 1, 2]>; // ERROR
  y as Readonly<[0, 1, 2, 3, 4]>; // ERROR
  y as Readonly<[0, 1, 2, 999]>; // ERROR
}

// Optional members
{
  declare const x: [b: 1, c?: 2];
  const y = [0, ...x] as const; // OK
  y as [1, 2]; // ERROR
  const z = [0, ...x, 3];
  z as Readonly<[0, 1, 2, 3]>; // ERROR
  z as ReadonlyArray<number | undefined>; // OK
}
{
  declare const x: [b: 0, c?: 1];
  declare const y: [2, 3];
  const z = [...x, ...y] as const;
  z as Readonly<[0, 1, 2, 3]>; // ERROR
  z as ReadonlyArray<number | undefined>; // OK
}
{
  const x: [0, b?: 1] = [0];
  const y: [c?: 2] = [2];
  const z = [...x, ...y] as const;
  z as [a?: 0, b?: 1, c?: 2]; // ERROR
  z as ReadonlyArray<number | undefined>; // OK
}

// Variance is not preserved when doing value spread as the result is a copy
{
  declare const x: [readonly b: string, writeonly c: boolean];
  const y = [0, ...x]; // OK
  y[1] = 's'; // OK
  const n: boolean = y[2]; // OK
}
