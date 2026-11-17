declare const maybeMap: ?{[key: string]: ?{[key: string]: number}};

function writeAfterAssertion(): number {
  let value: number;
  maybeMap?.['outer']![(value = 1, 'inner')];
  return value; // error: the key is skipped when `maybeMap` is null or undefined
}
