//@flow

const React = require('react');

class PolyReactClass<T> extends React.Component<{...}> {}

<PolyReactClass />;

function PolyFunctionComponent<T>(props: {foo: ((o: {x: T, ...}) => unknown), ...}): React.Node { return null }

<PolyFunctionComponent foo={() => {}}/>;
