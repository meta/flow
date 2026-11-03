/* @flow */

var React = require('react');

var Div = 'testdiv' as const;
var Bad = 'bad' as const;
var Str: string = 'str';

<Div id="foo" />; // This is fine
<Bad />; // ERROR
<Str />; // ERROR

<testdiv />; // ERROR
<bad />; // ERROR
<Str />; // ERROR

<Div id={42} />; // Error: 42 ~> string
