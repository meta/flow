import * as React from 'react';
import type {DetailedReactHTMLElement} from 'react';
import JsxRuntime, {Fragment, JSX, jsx} from 'react/jsx-runtime';
import JsxDevRuntime, {jsxDEV, type JSXSource} from 'react/jsx-dev-runtime';

<Fragment />; // ok
<div /> as JSX.Element; // ok
1 as JSX.Element; // error
jsx('div', {}); // error: like createElement, jsx cannot be called
jsxDEV('div', {}); // error
({fileName: 'a.js', lineNumber: 1}) as JSXSource; // ok
JsxRuntime.jsxs as typeof jsx; // ok: the CommonJS module object
JsxDevRuntime.Fragment as typeof Fragment; // ok

declare const fce: React.FunctionComponentElement<{x: number}>;
fce as React.ReactElement<{x: number}>; // ok
fce as React.ReactElement<{x: string}>; // error

declare const html: React.DetailedReactHTMLElement<{readonly role: string}, HTMLDivElement>;
html as React.ReactElement<{readonly role: string}, string>; // ok
html as React.ReactElement<{readonly role: string}, React.FC<{}>>; // error
html as React.DetailedReactHTMLElement<{readonly role: string}, HTMLElement>; // ok
html as React.DetailedReactHTMLElement<{readonly role?: string}, HTMLDivElement>; // ok
html as DetailedReactHTMLElement<{readonly role: string}, HTMLElement>; // ok
type Bad = React.DetailedReactHTMLElement<{role: number}, HTMLDivElement>; // error
