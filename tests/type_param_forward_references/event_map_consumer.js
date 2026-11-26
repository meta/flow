// @flow

import type {Args, EventMap} from './event_map';
import {EventEmitter} from './event_map';

declare const emitter: EventEmitter<{foo: [number]}>;
emitter.args('foo') as [number]; // OK
emitter.listeners('foo') as Array<(n: number) => void>; // OK
emitter.listeners('bar'); // ERROR
EventEmitter.defaultMaxListeners as number; // OK

declare export class Sub<T extends EventMap<T>> extends EventEmitter<T> {
  argsOf<E extends string | symbol>(eventName: E): Args<T, E>;
}
declare const sub: Sub<{foo: [number]}>;
sub.argsOf('foo') as [number]; // OK
