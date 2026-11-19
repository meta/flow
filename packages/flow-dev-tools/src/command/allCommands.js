/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 * @format
 */

module.exports = {
  'add-comments': () => require('../comment/add-commentsCommand'),
  help: () => require('../help/helpCommand'),
  'new-test': () => require('../new-test/new-testCommand'),
  ping: () => require('../ping/pingCommand'),
  record: () => require('../record/recordCommand'),
  test: () => require('../test/testCommand'),
  // prettier-ignore
  // @fb-only: cgroups: () => require('../facebook/cgroups/cgroupsCommand'),
  // prettier-ignore
  // @fb-only: 'cherry-pick': () => require('../facebook/release/cherry-pickCommand'),
  // prettier-ignore
  // @fb-only: 'list-commits': () => require('../facebook/release/list-commitsCommand'),
  // prettier-ignore
  // @fb-only: 'new-version': () => require('../facebook/release/new-versionCommand'),
  // prettier-ignore
  // @fb-only: 'windtunnel-prof': () => require('../facebook/windtunnel-prof/windtunnel-profCommand'),
  // prettier-ignore
  // @fb-only: 'windtunnel-recheck-prof': () => require('../facebook/windtunnel-recheck-prof/windtunnel-recheck-profCommand'),
} as {
  [key: string]: () => {
    readonly default: interface {
      description(): string,
      go(): Promise<void>,
      showUsage(v: number): void,
    },
    ...
  },
};
