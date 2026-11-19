/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 * @flow
 */

'use strict';

import * as util from 'util';

type OldConsole = $ReadOnly<{
  [key: $Keys<typeof console>]: (...rest: Array<$FlowFixMe>) => void,
}>;
interface Reporter {
  onStdout(v: string): void;
  onStderr(v: string | Error): void;
}

const oldConsole: OldConsole = {
  error: console.error,
  info: console.info,
  log: console.log,
  warn: console.warn,
};

export function redirectConsole(
  reporter_: Reporter | ((v: OldConsole) => Reporter),
): void {
  const reporter =
    typeof reporter_ === 'function' ? reporter_(oldConsole) : reporter_;

  // $FlowExpectedError[cannot-write]
  console.log = (...args) => {
    reporter.onStdout(util.format(...args));
  };
  // $FlowExpectedError[cannot-write]
  console.info = (...args) => {
    reporter.onStdout(util.format(...args));
  };
  // $FlowExpectedError[cannot-write]
  console.warn = (...args) => {
    reporter.onStdout('[WARN]' + util.format(...args));
  };
  // $FlowExpectedError[cannot-write]
  console.error = (...args) => {
    reporter.onStderr(util.format(...args));
  };
}

export function restoreConsole(): void {
  for (const [name, fn] of Object.entries(oldConsole)) {
    // $FlowExpectedError
    console[name] = fn;
  }
}
