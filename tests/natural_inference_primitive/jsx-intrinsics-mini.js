declare class Element {}
declare class HTMLElement extends Element {}
declare class Event {}

type ReactDOM$HTMLElementProps = {
  id?: ?string,
  onClick?: ?(Event) => unknown,
};

type ReactDOM$divProps = ReactDOM$HTMLElementProps;

declare class HTMLDivElement extends HTMLElement {
  tagName: 'DIV';
}

type ReactDOM$divInstance = HTMLDivElement;

type ReactDOM$HTMLElementJSXIntrinsicTyped<Props, Instance = HTMLElement> = {
  ...Props,
  ref?: React.RefSetter<Instance>,
};

type ReactDOM$HTMLElementJSXIntrinsic = {
  readonly [key: string]: any,
  readonly children?: React.Node,
  ref?: React.RefSetter<HTMLElement>,
  ...
};

declare type React$JSXIntrinsicElements = {
  [string]: ReactDOM$HTMLElementJSXIntrinsic,
  div: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$divProps, ReactDOM$divInstance>,
}
