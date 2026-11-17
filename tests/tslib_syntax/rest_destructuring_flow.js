interface DestructuredRest<T> {
  next(...[value]: [] | [T]): number;
}

declare const rest: DestructuredRest<string>;
rest.next() as number;
rest.next('value') as number;
rest.next(1); // ERROR
rest.next('first', 'second'); // ERROR

declare const pair: (...[first, second]: [number, string]) => number;
pair(1, 'value') as number;
pair('value', 1); // ERROR
pair(1); // ERROR

declare const anonymous: (...args: [number, string]) => number;
anonymous(1, 'value') as number;
anonymous('value', 1); // ERROR
