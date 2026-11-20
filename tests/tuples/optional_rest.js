// The rest of a tuple with an optional element may hold `undefined`.
declare const t: [a: number, b?: string];
const [x, ...rest] = t;
declare const i: number;
rest[i] as string; // error
for (const v of rest) {
  v as string; // error
}
