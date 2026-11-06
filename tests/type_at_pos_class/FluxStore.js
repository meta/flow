// @flow

class FluxStore<TPayload> {
  constructor(dispatcher: any): void { }
  p: string;
//^
}

declare const FluxStoreClass: Class<FluxStore<any>>;

class Dispatcher {}

class Store extends FluxStoreClass {
  constructor() {
    super(new Dispatcher());
//  ^
  }
}
