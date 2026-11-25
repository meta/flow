// @flow

import {
  BaseNode,
  register,
  take,
  takeCatBox,
  takeDict,
  takeDictD,
  takeHandler,
  takeIndexed,
  takeNumbers,
  takeSpread,
  mkFn,
  mkFnT,
  ka,
  takeK,
  type FnT,
  type SerializedNode,
} from 'repro';
import {Cat, type Dict, type DictD, type Fn, type Handler} from './flow_alias';

type SerializedMyNode = {type: string};
declare const node: SerializedMyNode;

// TypeScript gives an object type an implicit index signature and checks its
// values covariantly against `Record<string, unknown>`.
take(node); // OK
takeIndexed(node); // OK
takeNumbers(node); // ERROR: `type` is a string

class MyNode extends BaseNode {
  static importJSON(json: SerializedMyNode): MyNode { // OK
    return new MyNode();
  }
  // `Record<string, SerializedNode>` against `Record<string, SerializedPartialNode | SerializedNode>`.
  exportJSON(): {...SerializedNode, name: string} { // OK
    return {type: 'my', name: ''};
  }
}
register(MyNode); // OK

// Flow applying `Record` keeps Flow's semantics: the values of an indexer are invariant.
declare function takeInFlow(values: Record<string, unknown>): void;
takeInFlow(node); // ERROR

// TypeScript applying a Flow alias gets TypeScript's semantics, Flow applying it Flow's.
takeDict(node); // OK
takeDictD(node); // OK
takeSpread(node); // OK
declare function takeDictInFlow(values: Dict<unknown>): void;
takeDictInFlow(node); // ERROR
declare function takeDictDInFlow(values: DictD): void;
takeDictDInFlow(node); // ERROR
// Two applications of the same alias are compared by its type parameters' variance, under the
// strictness the applications produce. An unannotated type parameter of an alias declared in
// TypeScript is covariant, and so it is for a Flow alias applied in TypeScript.
mkFnT('') as FnT<unknown>; // OK
const stringFn = mkFn('');
stringFn as Fn<unknown>; // OK
// The applications are not expanded, so a recursive alias whose type arguments grow is fast.
takeK(ka); // OK

// Classes and interfaces declared in Flow stay as they are declared.
takeCatBox({value: new Cat()}); // OK
takeCatBox({value: {name: ''}}); // ERROR: not a `Cat`
declare const stringHandler: Handler<string>;
takeHandler(stringHandler); // ERROR: `Handler`'s type parameter is invariant
