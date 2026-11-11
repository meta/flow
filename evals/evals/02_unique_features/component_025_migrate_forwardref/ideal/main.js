/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 */

import * as React from 'react';

export default component SearchField(
  value: string,
  onChange: (string) => void,
  placeholder?: string,
  ref: React.RefSetter<HTMLInputElement>,
) {
  return (
    <input
      ref={ref}
      value={value}
      placeholder={placeholder}
      onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
        onChange(e.currentTarget.value)
      }
    />
  );
}
