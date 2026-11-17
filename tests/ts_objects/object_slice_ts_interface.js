// @flow

import {
  type TsNarrowInterface,
  tsNarrowInterfaceValue,
} from './object_slice_ts';
import {type FlowDog} from './object_slice_flow_base';

type SpreadInterface = {...TsNarrowInterface};

declare const typeSpread: SpreadInterface;
typeSpread.value.bark();
typeSpread.extra as string;
typeSpread.inherited.bark();
typeSpread.inheritedFunction() as string;
typeSpread.ownFunction() as string;
typeSpread.inheritedMethod() as string;
typeSpread.ownMethod() as string;

const valueSpread = {...tsNarrowInterfaceValue}; // OK
valueSpread.value.bark();
valueSpread.extra as string;
valueSpread.extra as empty; // ERROR: the result is not any
valueSpread.inherited.bark();
valueSpread.inheritedFunction() as string;
valueSpread.ownFunction() as string;
valueSpread.inheritedMethod() as string;
valueSpread.ownMethod() as string;

const valueSpreadAfterKnown = {sentinel: true, ...tsNarrowInterfaceValue}; // OK
valueSpreadAfterKnown.sentinel as boolean;

declare const optionalTail: {value?: number};
const valueSpreadBeforeOptional = {...tsNarrowInterfaceValue, ...optionalTail}; // OK
valueSpreadBeforeOptional.value as FlowDog | number;

declare const indexedHead: {[key: string]: number};
const valueSpreadAfterIndexer = {...indexedHead, ...tsNarrowInterfaceValue}; // ERROR

declare const tsOrExact: TsNarrowInterface | {kind: 'exact', flow: number};
const tsOrExactSpread = {...tsOrExact}; // OK

interface FlowInterface {
  kind: 'flow';
  flow: number;
}
declare const tsOrFlowInterface: TsNarrowInterface | FlowInterface;
const tsOrFlowInterfaceSpread = {...tsOrFlowInterface}; // ERROR

declare const flowInterfaceOrExact: FlowInterface | {kind: 'exact', exact: number};
const flowInterfaceOrExactSpread = {...flowInterfaceOrExact}; // ERROR

type FlowInexact = {kind: 'flow-inexact', flow: number, ...};
declare const tsOrFlowInexact: TsNarrowInterface | FlowInexact;
const tsOrFlowInexactSpread = {sentinel: true, ...tsOrFlowInexact}; // ERROR

interface FlowDerivedInterface extends TsNarrowInterface {}
declare const flowDerivedInterfaceValue: FlowDerivedInterface;
const flowDerivedValueSpread = {...flowDerivedInterfaceValue}; // ERROR
