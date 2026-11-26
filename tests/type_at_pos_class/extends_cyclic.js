// @flow
declare const StateRecord: Class<State> & (() => State);
class State extends StateRecord {
  own: number = 1;
}

declare const state: State;
state.own;
//    ^
