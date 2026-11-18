/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 */

const tally: {
  counts: {[key: string]: number},
  total: number,
  record(key: string, n: number): number,
  topKey(): string | null,
} = {
  counts: {},
  total: 0,
  record(key: string, n: number): number {
    const prev = this.counts[key] ?? 0;
    this.counts[key] = prev + n;
    this.total += n;
    return this.counts[key];
  },
  topKey(): string | null {
    let best: string | null = null;
    let bestN = 0;
    for (const k of Object.keys(this.counts)) {
      const v = this.counts[k];
      if (v > bestN) {
        bestN = v;
        best = k;
      }
    }
    return best;
  },
};

tally.record('cache_hit', 3);
tally.record('cache_miss', 1);
tally.record('cache_hit', 2);
console.log(`top=${String(tally.topKey())} total=${tally.total}`);
