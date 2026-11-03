var React = require('react');

<testdiv id="asdf" />;
<testdiv id={42} />; // ERROR

const dataProps: {[StringPrefix<'data-'>]: string} = {};
const d = <testdiv {...dataProps} />; // ERROR

<testspan id="ok" class="ok" />;
<testspan id="ok" />; // ERROR
