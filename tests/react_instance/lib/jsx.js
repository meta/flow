type React$JSXIntrinsicElements = {
  foo: {
    ref?: React.RefSetter<{yep1: true, ...}>,
    ...
  },
  bar: {
    ref?: React.RefSetter<{yep2: true, ...}>,
    ...
  },
  ...
};
