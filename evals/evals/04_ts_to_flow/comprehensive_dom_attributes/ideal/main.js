/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 */

import * as React from 'react';

component SearchField(
  value: string,
  placeholder?: string,
  disabled?: boolean,
  containerStyle?: {[string]: string | number},
  onChange: (value: string) => void,
  onSubmit: (value: string) => void,
) {
  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    onChange(e.currentTarget.value);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      onSubmit(e.currentTarget.value);
    }
  };

  const handleClick = (e: React.MouseEvent<HTMLButtonElement>) => {
    e.preventDefault();
    onSubmit(value);
  };

  return (
    <div style={containerStyle} className="search-field">
      <input
        type="search"
        value={value}
        placeholder={placeholder}
        disabled={disabled}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
      />
      <button type="button" disabled={disabled} onClick={handleClick}>
        Search
      </button>
    </div>
  );
}

export default SearchField;
