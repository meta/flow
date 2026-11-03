/* @flow */

import * as React from 'react';

const Div = 'div' as const;
const Bad = 'bad' as const;
declare const Str: string;

const empty_exact: {} = {...null};

<Div />;
<Bad />; // ERROR
<Str />; // ERROR

<div {...empty_exact} />;
<bad {...empty_exact} />; // ERROR

<Div id="str" />;
<Div id={42} />; // ERROR
<input disabled />;
<input disabled="yes" />; // ERROR
<input id={42} />; // ERROR
<select aria-hidden="yes" />; // ERROR
<textarea onClick={(event: number) => {}} />; // ERROR
<button type="submit" />;
<button type="invalid" />; // ERROR
<svg viewBox="0 0 10 10"><circle cx={5} r={4} /></svg>;
<svg viewBox={42} />; // ERROR
<div aria-hidden="true" />;
<div aria-hidden="yes" />; // ERROR
<div style={{display: 'flex', opacity: 0.5}} />;
<div style={{opacity: false}} />; // ERROR

({className: 'ok'}) as React.ComponentProps<'div'>;
({disabled: true}) as React.ComponentProps<'input'>;
({disabled: 'yes'}) as React.ComponentProps<'input'>; // ERROR
({viewBox: '0 0 10 10'}) as React.JSX.IntrinsicElements['svg'];
({viewBox: 42}) as React.JSX.IntrinsicElements['svg']; // ERROR
({type: 'submit'}) as React.ButtonHTMLAttributes<HTMLButtonElement>;
({type: 'invalid'}) as React.ButtonHTMLAttributes<HTMLButtonElement>; // ERROR
({viewBox: 42}) as Readonly<React.ComponentProps<'svg'>>; // ERROR
({'aria-hidden': 'yes'}) as Partial<React.ComponentProps<'div'>>; // ERROR
({onClick: (event: number) => {}}) as Readonly<React.ComponentProps<'button'>>; // ERROR

declare const ref: {current: HTMLDivElement | null};
<div ref={ref} />;
<div ref={(node: number | null) => {}} />; // ERROR
({ref}) as React.ComponentProps<'div'>;
declare const instance: React.ElementRef<'div'>;
instance as HTMLDivElement;
instance as number; // ERROR

const handleClick = (event: React.MouseEvent<HTMLButtonElement>) => {
  event.currentTarget.disabled as boolean;
};
<button onClick={handleClick} />;
<button onClick={(event: number) => {}} />; // ERROR

declare const style: React.CSSProperties;
style.opacity as empty; // ERROR

declare const union: 'div' | 'input';
const Union = union;
<Union id="ok" />;
<Union id={false} />; // ERROR
