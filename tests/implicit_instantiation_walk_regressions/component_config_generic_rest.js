import * as React from 'react';

// The config of a component with only a rest parameter is the type of the rest parameter, also
// when it has a type parameter of the enclosing component in it: the props of the component meet
// it as they are, so `TChild` meets the enclosing `TChild` through `children`.
type TierProps<TChild extends {...} = {...}> = Readonly<{
  tierChildren?: ReadonlyArray<TChild>,
  depth: number,
  ...
}>;
type CustomTierProps<TAdditional, TChild extends {...} = {...}> = TierProps<TChild> & TAdditional;
type TierComponent<TAdditional, TChild extends {...} = {...}> = component(
  ...CustomTierProps<TAdditional, TChild>
);

component Container<TAdditional, TChild extends {...} = {...}>(
  TierComponent: TierComponent<TAdditional, TChild>,
  additionalProps?: TAdditional,
  depth: number,
) {
  if (depth > 0) {
    return (
      <Container
        additionalProps={additionalProps}
        depth={depth - 1}
        TierComponent={TierComponent}
      />
    ); // ok
  }
  return null;
}
