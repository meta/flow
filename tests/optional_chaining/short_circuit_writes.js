declare const maybeCallback: ?(value: number) => void;
declare const maybeDouble: ?(value: number) => number;
declare const maybeObject: ?{
  run(value: number): void,
  inner: ?{run(value: number): void},
};
declare const maybeMap: ?{[key: string]: number};
declare const map: {[key: string]: number};
declare const maybeDeletableMap: ?{[key: string]: ?number};
declare const callback: (value: number) => void;
declare const maybePair: ?(first: number, second: number) => void;
declare const holder: {maybeCallback: ?(value: number) => void};
declare const maybeFactory: ?() => {
  run(value: number): void,
  map: {[key: string]: number},
};
declare const maybeChain: ?{first(value: number): {second(value: number): void}};
declare const maybeNested: ?{[key: string]: ?{[key: string]: number}};
declare const getMap: ?() => {[key: string]: number};

function optionalCall(): number {
  let value: number;
  maybeCallback?.((value = 1));
  return value; // error: the call is skipped when `maybeCallback` is null or undefined
}

function optionalMethodCall(): number {
  let value: number;
  maybeObject?.run((value = 1));
  return value; // error: the chain stops before the call when `maybeObject` is nullish
}

function laterOptionalLink(): number {
  let value: number;
  maybeObject?.inner?.run((value = 1));
  return value; // error
}

function optionalComputedKey(): number {
  let value: number;
  maybeMap?.[String((value = 1))];
  return value; // error: the key is skipped when `maybeMap` is nullish
}

function optionalCallOfProperty(): number {
  let value: number;
  holder.maybeCallback?.((value = 1));
  return value; // error
}

function deleteWithOptionalKey(): number {
  let value: number;
  delete maybeDeletableMap?.[String((value = 1))];
  return value; // error
}

function laterCallInChain(): number {
  let value: number;
  maybeFactory?.().run((value = 1));
  return value; // error: the chain stops before `run` when `maybeFactory` is nullish
}

function laterKeyInChain(): number {
  let value: number;
  maybeFactory?.().map[String((value = 1))];
  return value; // error
}

function argumentsOfOrdinaryCall(): number {
  let value: number;
  callback((value = 1));
  return value; // ok: the arguments of an ordinary call always run
}

function refinementInArguments(x: ?number): number {
  maybeCallback?.(x == null ? 0 : x);
  return x ?? 0; // ok
}

function writeBeforeQuestionDot(): number {
  let value: number;
  (value = 1, maybeCallback)?.(value);
  return value; // ok: what comes before `?.` always runs
}

function writeAfterChain(): number {
  let value: number;
  maybeCallback?.((value = 1));
  value = 2;
  return value; // ok: assigned after the chain
}

function laterArgumentSeesWrite(): number {
  let value: number;
  maybePair?.((value = 1), value); // ok: the second argument runs after the first
  return value; // error
}

function laterLinkSeesWrite(): number {
  let value: number;
  maybeChain?.first((value = 1)).second(value); // ok: `second` runs only after `first`
  return value; // error
}

function laterKeySeesWrite(): number {
  let value: number;
  maybeNested?.[String((value = 1))]?.[String(value)]; // ok: the second key runs after the first
  return value; // error
}

function nestedOptionalChain(): number {
  let value: number;
  maybePair?.(maybeDouble?.((value = 1)) ?? 0, value); // error: the inner call may be skipped
  return value; // error
}

function parenthesesEndChain(): number {
  let value: number;
  (getMap?.())[String((value = 1))]; // error: `getMap?.()` may be undefined
  return value; // ok: the key always runs
}

function skippedPathKeepsType(): number {
  let value: number | string = 'before';
  maybeCallback?.((value = 1));
  return value; // error: `value` is still a string when the call is skipped
}

function skippedPathKeepsNarrowing(box: {value: number | string}): number {
  if (typeof box.value === 'string') {
    maybeMap?.[(box.value = 1, 'key')];
    return box.value; // error: `box.value` is still a string when the key is skipped
  }
  return 0;
}

function skippedPathKeepsNullishBase(): number {
  let current = maybeMap;
  current?.[((current = map), 'key')];
  return current['key']; // error: `current` is still null or undefined when the key is skipped
}

function refinementAfterChain(): void {
  if (maybeObject?.inner) {
    maybeObject.inner.run(1); // ok
  }
}
