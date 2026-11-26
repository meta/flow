// The Immutable.js record idiom: the class is its own record (tsc: TS2310).
declare function Record(spec: {...}): any;
const StateRecord: Class<State> & (() => State) = Record({a: 1});
export class State extends StateRecord {
  a: number;
}

declare class Other {
  o: number;
}
declare const R: Class<C> & Class<Other>;
export class C extends R {}
