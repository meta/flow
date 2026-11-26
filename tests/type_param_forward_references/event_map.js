// @flow

// The `EventMap` pattern of @types/node: a bound that maps the keys of its own type parameter.

export type Rec<K extends string | number | symbol, V> = {readonly [P in K]: V};
export type EventMap<T> = Rec<keyof T, ReadonlyArray<unknown>>;
export type IfEventMap<Events extends EventMap<Events>, True, False> = {} extends Events
  ? False
  : True;
export type Args<Events extends EventMap<Events>, EventName extends string | symbol> = IfEventMap<
  Events,
  EventName extends keyof Events ? Events[EventName] : ReadonlyArray<unknown>,
  ReadonlyArray<unknown>,
>;
export interface Emitter<T extends EventMap<T>> {
  listeners<E extends keyof T>(eventName: E): Array<(...args: T[E]) => void>;
}
declare export class EventEmitter<T extends EventMap<T>> implements Emitter<T> {
  static defaultMaxListeners: number;
  args<E extends string | symbol>(eventName: E): Args<T, E>;
  listeners<E extends keyof T>(eventName: E): Array<(...args: T[E]) => void>;
}

declare const local: EventEmitter<{foo: [number]}>;
local.args('foo') as [number]; // OK
local.listeners('foo') as Array<(n: number) => void>; // OK
local.listeners('bar'); // ERROR
EventEmitter.defaultMaxListeners as number; // OK
