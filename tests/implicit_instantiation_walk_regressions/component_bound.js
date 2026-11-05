import * as React from 'react';

function case1() {
  declare function wrap<TProps extends {...}, TComponent extends React.ComponentType<TProps>>(
    name: string,
    Wrapped: TComponent,
  ): TComponent;

  component Chip<TValue>(label: string, value: TValue, onPress?: (TValue) => void) {
    return null;
  }

  const C = wrap('Chip', Chip) as typeof Chip;

  component Plain(label: string) {
    return null;
  }

  const D = wrap('Plain', Plain) as typeof Plain;
}

function case2() {
  declare function withTheme<
    TProps extends Readonly<{theme?: ?string, ...}>,
    TComponent extends component(...TProps),
  >(C: TComponent): TComponent;

  type HeaderProps<A, B> = {a: A, b: B, ...};

  const Header = withTheme(
    class Header<A, B> extends React.Component<HeaderProps<A, B>, void> { // error: no `theme` prop
      render(): React.Node {
        return null;
      }
    },
  );

  class Plain extends React.Component<{x: number}, void> {}

  const P = withTheme(Plain);
}
