import * as React from 'react';
declare const Poly: component<T extends React.Node>(children: T) renders T;

declare component Button();
declare component NotAButton();

const el = <Poly><Button /></Poly>;
el as renders NotAButton; // ERROR
el as renders Button; // OK

declare const Poly2: component<T extends React.Node = undefined>(children?: T) renders? T;

<Poly2 /> as renders? NotAButton; // OK
<Poly2 /> as renders? Button; // OK
<Poly2><Button /></Poly2> as renders? NotAButton; // ERROR
<Poly2><Button /></Poly2> as renders? Button; // OK
<Poly2><Button /></Poly2> as renders* NotAButton; // ERROR
<Poly2><Button /></Poly2> as renders* Button; // OK

declare const Poly3: component<T extends React.Node = undefined>(children?: T) renders* T; // banned
