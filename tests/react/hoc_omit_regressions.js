import * as React from 'react';

declare function connect<AllProps extends {...}, ServiceProps extends {...}>(
  mapServicesToProps: (
    n: number,
    props: Omit<AllProps, keyof ServiceProps>,
  ) => ServiceProps,
): (component: React.ComponentType<AllProps>) => React.ComponentType<
  Omit<AllProps, keyof ServiceProps>,
>;

type BannerProps = {
  title: string,
  description: string,
  onBackClick?: () => void,
  ...
};

function serviceToProps(
  _services: number,
  ownProps: {
    location?: string,
    history?: {push(path: string): void, ...},
    ...
  },
): BannerProps {
  const props: BannerProps = {
    title: 'Untitled',
    description: '',
  };

  if (ownProps.location && ownProps.history) {
    const history = ownProps.history;
    props.onBackClick = () => history.push('/');
  }

  return props;
}

function Banner(props: BannerProps): React.Node {
  return props.title;
}

const wrapBanner = connect(serviceToProps);
const ConnectedBanner = wrapBanner(Banner);

<ConnectedBanner />;

type ResizeProps<
  Props extends Readonly<{height?: number, width?: number, ...}>,
> = Readonly<{
  ...Omit<Props, 'height' | 'width'>,
  component: React.ComponentType<Props>,
 ...}>;

function injectProps<
  Props extends Readonly<{height?: number, width?: number, ...}>,
>(
  Component: React.ComponentType<Props>,
): React.ComponentType<Omit<Props, 'height' | 'width'>> {
  return Component as any;
}

function Resize<Props extends Readonly<{height?: number, width?: number, ...}>>(
  props: ResizeProps<Props>,
): React.Node {
  const {component: Component} = props;
  injectProps<Props>(Component);
  return null;
}

declare function ComponentWithSize(props: {
  foo: string,
  height?: ?number,
  width?: ?number,
 ...}): React.Node;

<Resize component={ComponentWithSize} foo="ok" />; // ERROR

type ConfigProps<
  Props extends Readonly<{height?: number, width?: number, ...}>,
> = Readonly<{
  ...Omit<Props, 'height' | 'width'>,
  component: React.ComponentType<Props>,
  ...
}>;

declare function takesConfig<
  Props extends Readonly<{height?: number, width?: number, ...}>,
>(props: ConfigProps<Props>): void;

type ComponentProps = Readonly<{
  foo: string,
  height?: ?number,
  width?: ?number,
 ...}>;

class SizedComponent extends React.Component<ComponentProps> {
  render(): React.Node {
    return null;
  }
}

takesConfig({component: SizedComponent, foo: 'ok'}); // ERROR
