// @flow

import {
  type Animal as TypeScriptAnimal,
  type EventListenerObjectLike,
  type MethodTarget,
  aliasedMethodSource,
  classMethodSource,
  literalMethodSource,
} from './method_bivariance';

// Flow-authored method shapes remain contravariant.
// The consumer extension is irrelevant because both participating object
// types retain Flow strictness through the compatibility check.
// The narrower method parameter must therefore still be rejected.

declare class Animal {
  name: string;
}
declare class Dog extends Animal {
  bark(): void;
}

type MethodHolder = {
  cb(x: Animal): void,
};

// Same shape as method_bivariance.ts:23 (which is OK in .ts). In .js, the
// standard contravariant FunT~>FunT path runs and rejects this.
const m: MethodHolder = {cb(x: Dog): void {}}; // ERROR in .js: no bivariance

type FlowFunctionProperty = {
  cb: (x: TypeScriptAnimal) => void,
};

const aliasedMethodAsFlowFunctionProperty: FlowFunctionProperty =
  aliasedMethodSource; // OK
const literalMethodAsFlowFunctionProperty: FlowFunctionProperty =
  literalMethodSource; // OK
const classMethodAsFlowFunctionProperty: FlowFunctionProperty =
  classMethodSource; // OK

type FlowFunctionIndexer = {
  [name: string]: (x: TypeScriptAnimal) => void,
};

const methodAsFlowFunctionIndexer: FlowFunctionIndexer = literalMethodSource; // OK

interface TypeScriptDog extends TypeScriptAnimal {
  bark(): void;
}

declare const narrowFlowField: {
  cb: (x: TypeScriptDog) => void,
};
const flowFieldAsTypeScriptMethod: MethodTarget = narrowFlowField; // OK

type FlowEventHandler = (x: TypeScriptDog) => unknown;
declare const flowListenerObject: {
  handleEvent: FlowEventHandler,
  ...
};
const flowListenerObjectAsTypeScriptListener: EventListenerObjectLike =
  flowListenerObject; // OK

// Flow-authored overloads keep the bounds of their generic signatures.
interface FlowOverloaded {
  m<C extends Animal>(c: C): void;
  m(x: number): void;
}
interface FlowOverloadedDog {
  m<C extends Dog>(c: C): void;
  m(x: number): void;
}
declare const flowOverloadedDog: FlowOverloadedDog;
const flowOverloadSet: FlowOverloaded = flowOverloadedDog; // ERROR in .js
