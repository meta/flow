// @flow

import * as React from 'react';

declare const Context: React.Context<string>;

component GenericProvider<TChildren extends React.Node>(
  children: TChildren,
) renders TChildren {
  return (
    <Context.Provider value="outer">
      <Context.Provider value="middle">
        <Context.Provider value="inner">{children}</Context.Provider>
      </Context.Provider>
    </Context.Provider>
  );
}

component OptionalGenericProvider<TChildren extends React.Node = undefined>(
  children?: TChildren,
) renders? TChildren {
  return <Context.Provider value="value">{children}</Context.Provider>;
}

component NestedGenericProvider<TChildren extends React.Node>(
  children: TChildren,
) renders TChildren {
  return <GenericProvider>{children}</GenericProvider>;
}

component EitherGenericProvider<
  TLeft extends React.Node,
  TRight extends React.Node,
>(
  chooseLeft: boolean,
  left: TLeft,
  right: TRight,
) renders (TLeft | TRight) {
  return (
    <Context.Provider value="value">
      {chooseLeft ? left : right}
    </Context.Provider>
  );
}

component First() {
  return <div />;
}

component Second() {
  return <span />;
}

component Third() {
  return <section />;
}

component AcceptsFirst(child: renders First) {
  return null;
}

component AcceptsOptionalFirst(child: renders? First) {
  return null;
}

component AcceptsEither(child: renders (First | Second)) {
  return null;
}

component WrappedFirst() renders First {
  return (
    <NestedGenericProvider>
      <First />
    </NestedGenericProvider>
  );
}

<AcceptsFirst child={<GenericProvider><First /></GenericProvider>} />;
<AcceptsOptionalFirst
  child={<OptionalGenericProvider><First /></OptionalGenericProvider>}
/>;
<AcceptsOptionalFirst child={<OptionalGenericProvider />} />;
<AcceptsEither
  child={
    <EitherGenericProvider
      chooseLeft={true}
      left={<First />}
      right={<Second />}
    />
  }
/>;

<AcceptsFirst child={<GenericProvider><Second /></GenericProvider>} />; // ERROR
<AcceptsEither
  child={
    <EitherGenericProvider
      chooseLeft={false}
      left={<First />}
      right={<Third />}
    />
  }
/>; // ERROR
