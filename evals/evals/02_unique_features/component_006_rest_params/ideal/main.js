/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 */

import * as React from 'react';

export component BaseInput(
  value: string,
  onChange: (string) => void,
  placeholder: string,
) {
  return (
    <input
      value={value}
      onChange={(e: React.ChangeEvent<HTMLInputElement>) => onChange(e.currentTarget.value)}
      placeholder={placeholder}
    />
  );
}

export component SearchInput(
  onSearch: (string) => void,
  ...baseProps: React.PropsOf<BaseInput>,
) {
  return (
    <div>
      <BaseInput {...baseProps} />
      <button onClick={() => onSearch(baseProps.value)}>Search</button>
    </div>
  );
}

export component LabeledInput(
  label: string,
  ...baseProps: React.PropsOf<BaseInput>,
) {
  return (
    <label>
      {label}
      <BaseInput {...baseProps} />
    </label>
  );
}
