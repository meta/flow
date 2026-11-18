/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 */

import * as React from 'react';
import {useState, useCallback} from 'react';

declare const __DEV__: boolean;

hook useLocalCounter(initial: number): [number, () => void] {
  const [count, setCount] = useState(initial);
  const increment = useCallback(() => {
    setCount(prev => prev + 1);
  }, []);
  return [count, increment];
}

hook useAlwaysZero(initial: number): [number, () => void] {
  return [0, () => {}];
}

const useActiveCounter: hook (initial: number) => [number, () => void] =
  __DEV__ ? useLocalCounter : useAlwaysZero;

export default component Counter() {
  const [count, increment] = useActiveCounter(0);
  return (
    <div>
      <span>{count}</span>
      <button onClick={increment}>Increment</button>
    </div>
  );
}
