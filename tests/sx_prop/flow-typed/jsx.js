declare class Element {}
declare class HTMLElement extends Element {}

type ReactDOM$HTMLElementProps = {
  className?: string,
  style?: {[string]: string | number},
};

type ReactDOM$divProps = ReactDOM$HTMLElementProps;
type ReactDOM$spanProps = ReactDOM$HTMLElementProps;

declare class HTMLDivElement extends HTMLElement {}
declare class HTMLSpanElement extends HTMLElement {}

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
  div: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$divProps, HTMLDivElement>,
  span: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$spanProps, HTMLSpanElement>,
}
