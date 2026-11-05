// A literal stays precise when the bound of its type parameter asks for it through another type
// parameter.

function keys() {
  type Events = {add: [], remove: [], change: [number]};

  declare class Emitter<T> {}

  declare function listenTo<
    TEvents extends {...},
    N extends keyof TEvents | ReadonlyArray<keyof TEvents>,
  >(emitter: Emitter<TEvents>, name: N, cb: () => void): void;

  declare const e: Emitter<Events>;

  listenTo(e, 'add', () => {});
  listenTo(e, ['add', 'remove'], () => {});
  listenTo(e, ['nope'], () => {}); // error
}

function indexed() {
  type Kind = 'in_play' | 'start';
  type Events = {enter: [Kind], leave: []};

  declare class Emitter<TEvents extends {...}> {
    emit<K extends keyof TEvents, TArgs extends TEvents[K]>(name: K, ...args: TArgs): void;
  }

  declare const e: Emitter<Events>;
  declare const b: boolean;

  e.emit('enter', b ? 'in_play' : 'start');
  e.emit('enter', 'nope'); // error
}
