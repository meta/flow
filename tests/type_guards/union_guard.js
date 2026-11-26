// A union guard is intersected with the refined type one member at a time.

type Falsy = false | 0 | 0n | '' | null | undefined;

function literals(x: string | number) {
  declare function isLit(x: unknown): x is 'a' | 1 | true;
  if (isLit(x)) {
    x as 'a' | 1; // okay
    x as 'a'; // error 1 ~> 'a'
  } else {
    x as string | number; // okay
  }
}

function falsy(x: string | number | undefined) {
  declare function isFalsy(x: unknown): x is Falsy;
  if (isFalsy(x)) {
    x as '' | 0 | undefined; // okay
    x as '' | 0; // error undefined ~> '' | 0
  } else {
    x as string | number; // okay
  }
}

function maybe_literal(x: number) {
  declare function isZero(x: unknown): x is ?0;
  if (isZero(x)) {
    x as 0; // okay
  }
}

function general_and_literal(x: string | number | boolean) {
  declare function isStrOr1(x: unknown): x is string | 1;
  if (isStrOr1(x)) {
    x as string | 1; // okay
    x as string; // error 1 ~> string
  } else {
    x as number | boolean; // okay
  }
}

function generic<T extends string | number>(x: T) {
  declare function isFalsy(x: unknown): x is Falsy;
  if (isFalsy(x)) {
    x as '' | 0; // okay
    x as T; // okay
  }
}

function objects(x: {readonly b: number}) {
  declare function isB0(x: unknown): x is {readonly a: false, ...} | {readonly b: 0};
  if (isB0(x)) {
    x as {readonly b: 0}; // error {readonly a: false, ...} & {readonly b: number} ~> {readonly b: 0}
  }
}
