// @flow

import * as React from 'react';

import {
  type FlowAnimal,
  type FlowNarrowValue,
  flowNarrowValue,
} from './object_slice_flow_base';
import {
  type TsDefaults,
  type TsMarker,
  type TsNarrowInterface,
  type TsNarrowValue,
  tsNarrowValue,
} from './object_slice_ts';

export type MixedIntersectionValue = Omit<
  FlowNarrowValue & TsMarker,
  'marker',
>;

export interface FlowDerivedFromTs extends TsNarrowInterface {
  marker: string;
}

export type SuperAccumulatedValue = Omit<
  FlowDerivedFromTs & {},
  'marker',
>;

type FlowProps = {
  value: FlowAnimal,
  extra: string,
  marker: string,
};

export declare class FlowComponent extends React.Component<FlowProps> {
  static defaultProps: TsDefaults;
}

export type FlowTypeSpreadOfTs = {...TsNarrowValue}; // OK
export const flowValueSpreadOfTs = {...tsNarrowValue}; // OK

export type FlowTypeSpreadOfFlow = {...FlowNarrowValue};
export const flowValueSpreadOfFlow = {...flowNarrowValue};
