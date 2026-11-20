import * as React from 'react';
import {CheckboxInput} from 'props';

function onFocus(e: React.FocusEvent<HTMLInputElement>) {}
function onKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {}

// tsc accepts.
const input = <CheckboxInput onFocus={onFocus} onKeyDown={onKeyDown} />;
