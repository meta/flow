function forOf(): void {
  return;
  for (const value of [0, 1]) {}
}

function forIn(object: {a: number}): void {
  return;
  for (const key in object) {}
}

function forOfDestructuring(pairs: Array<[number, string]>): void {
  return;
  for (const [first, second] of pairs) {}
}

function destructuring(pairs: Array<[number, string]>): void {
  return;
  const [first, second] = pairs[0];
}
