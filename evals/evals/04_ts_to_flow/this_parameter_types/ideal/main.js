/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 */

type Counter = {
  count: number,
  ...
};

function increment(this: Counter, by: number): number {
  return this.count + by;
}

type IncrementThis = ThisParameterType<typeof increment>;

type BoundIncrement = OmitThisParameter<typeof increment>;

const ctx: IncrementThis = {count: 10};
const receiver = {...ctx, increment};
const next: number = receiver.increment.call(receiver, 5);

const bound: BoundIncrement = receiver.increment.bind(receiver);
const after: number = bound(1);

console.log(next, after);
