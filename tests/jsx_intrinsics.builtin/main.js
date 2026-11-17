const React = require('react');

const dataProps: {[key: StringPrefix<'data-'>]: string} = {};
const d = <div {...dataProps} />; // OK
