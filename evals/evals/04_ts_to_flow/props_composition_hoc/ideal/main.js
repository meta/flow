/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 */

import * as React from 'react';

function withBorder<P extends {...}>(
  Wrapped: component(...P),
): component(...P) {
  return (props: P) => (
    <div className="bordered">
      <Wrapped {...props} />
    </div>
  );
}

function withTimestamp<P extends {...}>(
  Wrapped: component(...props: P & { now: number, ...}),
): component(...P) {
  return (props: P) => <Wrapped {...props} now={Date.now()} />;
}

type ClockProps = {
  now: number,
  label: string,
};

component Clock(now: number, label: string) {
  return (
    <div>
      {label}: {new Date(now).toISOString()}
    </div>
  );
}

const BorderedClock: component(...ClockProps) = withBorder(Clock);
const AutoClock: component(label: string) = withTimestamp<{label: string}>(Clock);

export {withBorder, withTimestamp, Clock, BorderedClock, AutoClock};
