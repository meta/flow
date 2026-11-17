// A mapped type over the keys `TKey` waits for the lower bounds of `TKey`, so `payload` says
// nothing about `TKey`, which is `'instanceID'`, from `requiredFieldName`.
type A = {type: 'a', instanceID: string, x: number, ...};
type B = {type: 'b', instanceID: string, y: number, ...};
declare function assertPayload<TKey extends string>(
  payload: {readonly [_key in TKey]?: unknown} & Readonly<{type: string, ...}>,
  requiredFieldName: TKey,
): void;
declare const payload: A | B;

assertPayload(payload, 'instanceID'); // was an error

// The mapped type over the solution of `K` says what `V` is, before or after `K` has its evidence.
declare function lookup<K extends string, V>(key: K, table: {readonly [_ in K]: V}): V;
declare function lookupIn<K extends string, V>(table: {readonly [_ in K]: V}, key: K): V;
declare const table: {readonly a: number};

lookup('a', table) as number; // ok
lookupIn(table, 'a') as number; // ok
lookup('a', table) as string; // error
