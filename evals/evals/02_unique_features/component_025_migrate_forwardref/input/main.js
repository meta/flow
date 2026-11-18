/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 */

import * as React from 'react';

type Props = {
  value: string,
  onChange: (value: string) => void,
  placeholder?: string,
};

const SearchField = React.forwardRef<Props, HTMLInputElement>(
  ({value, onChange, placeholder}, ref) => {
    return (
      <input
        ref={ref}
        value={value}
        placeholder={placeholder}
        onChange={(e: SyntheticInputEvent<HTMLInputElement>) =>
          onChange(e.currentTarget.value)
        }
      />
    );
  });

export default SearchField;
