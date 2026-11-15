// A spread of a union of tuples that is all a rest parameter receives gives it that union, as the
// call spreads each member, rather than an array of the elements of all the members, which fits
// none of them.
type Events = {a: [string], b: [number, boolean], c: []};
declare function emit<TEvent extends keyof Events, TArgs extends Events[TEvent]>(
  eventType: TEvent,
  ...rawArgs: TArgs
): void;

function forward<E extends keyof Events>(eventName: E, ...args: Events[E]) {
  emit(eventName, ...args); // was an error
}
