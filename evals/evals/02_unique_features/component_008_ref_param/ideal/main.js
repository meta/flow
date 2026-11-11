/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 */

import * as React from 'react';
import {useRef, useState} from 'react';

export component TextInput(
  value: string,
  onChange: (string) => void,
  placeholder: string,
  ref: React.RefSetter<HTMLInputElement>,
) {
  return (
    <input
      ref={ref}
      value={value}
      onChange={(e: React.ChangeEvent<HTMLInputElement>) => onChange(e.currentTarget.value)}
      placeholder={placeholder}
    />
  );
}

export component TextArea(
  value: string,
  onChange: (string) => void,
  rows: number,
  ref: React.RefSetter<HTMLTextAreaElement>,
) {
  return (
    <textarea
      ref={ref}
      value={value}
      onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) => onChange(e.currentTarget.value)}
      rows={rows}
    />
  );
}

export component FocusableForm() {
  const [name, setName] = useState('');
  const [bio, setBio] = useState('');
  const nameRef = useRef<HTMLInputElement | null>(null);
  const bioRef = useRef<HTMLTextAreaElement | null>(null);

  return (
    <form>
      <TextInput
        ref={nameRef}
        value={name}
        onChange={setName}
        placeholder="Name"
      />
      <TextArea
        ref={bioRef}
        value={bio}
        onChange={setBio}
        rows={4}
      />
      <button type="button" onClick={() => nameRef.current?.focus()}>
        Focus Name
      </button>
      <button type="button" onClick={() => bioRef.current?.focus()}>
        Focus Bio
      </button>
    </form>
  );
}
