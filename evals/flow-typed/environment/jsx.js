/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @noformat
 * @oncall flow
 */

// https://www.w3.org/TR/uievents-key/#keys-modifier
type ModifierKey =
  | 'Alt'
  | 'AltGraph'
  | 'CapsLock'
  | 'Control'
  | 'Fn'
  | 'FnLock'
  | 'Hyper'
  | 'Meta'
  | 'NumLock'
  | 'ScrollLock'
  | 'Shift'
  | 'Super'
  | 'Symbol'
  | 'SymbolLock';

declare class SyntheticEvent<out T extends EventTarget = EventTarget, out E extends Event = Event> {
  bubbles: boolean;
  cancelable: boolean;
  readonly currentTarget: T;
  defaultPrevented: boolean;
  eventPhase: number;
  isDefaultPrevented(): boolean;
  isPropagationStopped(): boolean;
  isTrusted: boolean;
  readonly nativeEvent: E;
  persist(): void;
  preventDefault(): void;
  stopPropagation(): void;
  // This should not be `T`. Use `currentTarget` instead. See:
  // https://github.com/DefinitelyTyped/DefinitelyTyped/issues/11508#issuecomment-256045682
  readonly target: EventTarget;
  timeStamp: number;
  type: string;
}

declare class SyntheticAnimationEvent<out T extends EventTarget = EventTarget>
  extends SyntheticEvent<T>
{
  animationName: string;
  elapsedTime: number;
  pseudoElement: string;
}

declare class SyntheticClipboardEvent<out T extends EventTarget = EventTarget>
  extends SyntheticEvent<T>
{
  clipboardData: any;
}

declare class SyntheticCompositionEvent<out T extends EventTarget = EventTarget>
  extends SyntheticEvent<T>
{
  data: any;
}

declare class SyntheticInputEvent<out T extends EventTarget = EventTarget>
  extends SyntheticEvent<T>
{
  data: any;
  readonly target: HTMLInputElement;
}

declare class SyntheticUIEvent<out T extends EventTarget = EventTarget, out E extends Event = Event>
  extends SyntheticEvent<T, E>
{
  detail: number;
  view: any;
}

declare class SyntheticFocusEvent<out T extends EventTarget = EventTarget>
  extends SyntheticUIEvent<T>
{
  relatedTarget: EventTarget;
}

declare class SyntheticKeyboardEvent<out T extends EventTarget = EventTarget>
  extends SyntheticUIEvent<T, KeyboardEvent>
{
  altKey: boolean;
  charCode: number;
  ctrlKey: boolean;
  getModifierState(key: ModifierKey): boolean;
  key: string;
  keyCode: number;
  locale: string;
  location: number;
  metaKey: boolean;
  repeat: boolean;
  shiftKey: boolean;
  which: number;
}

declare class SyntheticMouseEvent<
  out T extends EventTarget = EventTarget,
  out E extends Event = MouseEvent,
> extends SyntheticUIEvent<T, E>
{
  altKey: boolean;
  button: number;
  buttons: number;
  clientX: number;
  clientY: number;
  ctrlKey: boolean;
  getModifierState(key: ModifierKey): boolean;
  metaKey: boolean;
  pageX: number;
  pageY: number;
  relatedTarget: EventTarget;
  screenX: number;
  screenY: number;
  shiftKey: boolean;
}

declare class SyntheticDragEvent<out T extends EventTarget = EventTarget>
  extends SyntheticMouseEvent<T, DragEvent>
{
  dataTransfer: any;
}

declare class SyntheticWheelEvent<out T extends EventTarget = EventTarget>
  extends SyntheticMouseEvent<T, WheelEvent>
{
  deltaMode: number;
  deltaX: number;
  deltaY: number;
  deltaZ: number;
}

declare class SyntheticPointerEvent<out T extends EventTarget = EventTarget>
  extends SyntheticMouseEvent<T, PointerEvent>
{
  height: number;
  isPrimary: boolean;
  pointerId: number;
  pointerType: string;
  pressure: number;
  tangentialPressure: number;
  tiltX: number;
  tiltY: number;
  twist: number;
  width: number;
}

declare class SyntheticTouchEvent<out T extends EventTarget = EventTarget>
  extends SyntheticUIEvent<T, TouchEvent>
{
  altKey: boolean;
  changedTouches: TouchList;
  ctrlKey: boolean;
  getModifierState(key: ModifierKey): boolean;
  metaKey: boolean;
  shiftKey: boolean;
  targetTouches: TouchList;
  touches: TouchList;
}

declare class SyntheticTransitionEvent<out T extends EventTarget = EventTarget>
  extends SyntheticEvent<T>
{
  elapsedTime: number;
  propertyName: string;
  pseudoElement: string;
}

// prettier-ignore
declare type $JSXIntrinsics = {
  // Catch-all for custom elements.
  [key: string]: ReactDOM$HTMLElementJSXIntrinsic,
  // HTML
  a: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$aProps, ReactDOM$aInstance>,
  abbr: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$abbrProps, ReactDOM$abbrInstance>,
  address: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$addressProps, ReactDOM$addressInstance>,
  area: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$areaProps, ReactDOM$areaInstance>,
  article: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$articleProps, ReactDOM$articleInstance>,
  aside: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$asideProps, ReactDOM$asideInstance>,
  audio: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$audioProps, ReactDOM$audioInstance>,
  b: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$bProps, ReactDOM$bInstance>,
  base: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$baseProps, ReactDOM$baseInstance>,
  bdi: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$bdiProps, ReactDOM$bdiInstance>,
  bdo: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$bdoProps, ReactDOM$bdoInstance>,
  big: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$bigProps, ReactDOM$bigInstance>,
  blockquote: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$blockquoteProps, ReactDOM$blockquoteInstance>,
  body: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$bodyProps, ReactDOM$bodyInstance>,
  br: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$brProps, ReactDOM$brInstance>,
  button: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$buttonProps, ReactDOM$buttonInstance>,
  canvas: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$canvasProps, ReactDOM$canvasInstance>,
  caption: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$captionProps, ReactDOM$captionInstance>,
  cite: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$citeProps, ReactDOM$citeInstance>,
  code: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$codeProps, ReactDOM$codeInstance>,
  col: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$colProps, ReactDOM$colInstance>,
  colgroup: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$colgroupProps, ReactDOM$colgroupInstance>,
  data: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$dataProps, ReactDOM$dataInstance>,
  datalist: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$datalistProps, ReactDOM$datalistInstance>,
  dd: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$ddProps, ReactDOM$ddInstance>,
  del: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$delProps, ReactDOM$delInstance>,
  details: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$detailsProps, ReactDOM$detailsInstance>,
  dfn: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$dfnProps, ReactDOM$dfnInstance>,
  dialog: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$dialogProps, ReactDOM$dialogInstance>,
  div: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$divProps, ReactDOM$divInstance>,
  dl: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$dlProps, ReactDOM$dlInstance>,
  dt: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$dtProps, ReactDOM$dtInstance>,
  em: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$emProps, ReactDOM$emInstance>,
  embed: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$embedProps, ReactDOM$embedInstance>,
  fieldset: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$fieldsetProps, ReactDOM$fieldsetInstance>,
  figcaption: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$figcaptionProps, ReactDOM$figcaptionInstance>,
  figure: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$figureProps, ReactDOM$figureInstance>,
  footer: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$footerProps, ReactDOM$footerInstance>,
  form: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$formProps, ReactDOM$formInstance>,
  h1: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$h1Props, ReactDOM$h1Instance>,
  h2: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$h2Props, ReactDOM$h2Instance>,
  h3: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$h3Props, ReactDOM$h3Instance>,
  h4: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$h4Props, ReactDOM$h4Instance>,
  h5: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$h5Props, ReactDOM$h5Instance>,
  h6: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$h6Props, ReactDOM$h6Instance>,
  head: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$headProps, ReactDOM$headInstance>,
  header: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$headerProps, ReactDOM$headerInstance>,
  hgroup: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$hgroupProps, ReactDOM$hgroupInstance>,
  hr: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$hrProps, ReactDOM$hrInstance>,
  html: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$htmlProps, ReactDOM$htmlInstance>,
  i: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$iProps, ReactDOM$iInstance>,
  iframe: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$iframeProps, ReactDOM$iframeInstance>,
  img: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$imgProps, ReactDOM$imgInstance>,
  input: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$inputProps, ReactDOM$inputInstance>,
  ins: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$insProps, ReactDOM$insInstance>,
  kbd: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$kbdProps, ReactDOM$kbdInstance>,
  keygen: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$keygenProps, ReactDOM$keygenInstance>,
  label: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$labelProps, ReactDOM$labelInstance>,
  legend: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$legendProps, ReactDOM$legendInstance>,
  li: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$liProps, ReactDOM$liInstance>,
  link: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$linkProps, ReactDOM$linkInstance>,
  main: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$mainProps, ReactDOM$mainInstance>,
  map: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$mapProps, ReactDOM$mapInstance>,
  mark: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$markProps, ReactDOM$markInstance>,
  media: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$mediaProps, ReactDOM$mediaInstance>,
  menu: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$menuProps, ReactDOM$menuInstance>,
  menuitem: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$menuitemProps, ReactDOM$menuitemInstance>,
  meta: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$metaProps, ReactDOM$metaInstance>,
  meter: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$meterProps, ReactDOM$meterInstance>,
  nav: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$navProps, ReactDOM$navInstance>,
  noscript: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$noscriptProps, ReactDOM$noscriptInstance>,
  object: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$objectProps, ReactDOM$objectInstance>,
  ol: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$olProps, ReactDOM$olInstance>,
  optgroup: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$optgroupProps, ReactDOM$optgroupInstance>,
  option: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$optionProps, ReactDOM$optionInstance>,
  output: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$outputProps, ReactDOM$outputInstance>,
  p: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$pProps, ReactDOM$pInstance>,
  param: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$paramProps, ReactDOM$paramInstance>,
  picture: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$pictureProps, ReactDOM$pictureInstance>,
  pre: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$preProps, ReactDOM$preInstance>,
  progress: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$progressProps, ReactDOM$progressInstance>,
  q: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$qProps, ReactDOM$qInstance>,
  rp: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$rpProps, ReactDOM$rpInstance>,
  rt: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$rtProps, ReactDOM$rtInstance>,
  ruby: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$rubyProps, ReactDOM$rubyInstance>,
  s: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$sProps, ReactDOM$sInstance>,
  samp: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$sampProps, ReactDOM$sampInstance>,
  script: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$scriptProps, ReactDOM$scriptInstance>,
  section: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$sectionProps, ReactDOM$sectionInstance>,
  select: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$selectProps, ReactDOM$selectInstance>,
  slot: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$slotProps, ReactDOM$slotInstance>,
  small: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$smallProps, ReactDOM$smallInstance>,
  source: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$sourceProps, ReactDOM$sourceInstance>,
  span: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$spanProps, ReactDOM$spanInstance>,
  strong: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$strongProps, ReactDOM$strongInstance>,
  style: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$styleProps, ReactDOM$styleInstance>,
  sub: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$subProps, ReactDOM$subInstance>,
  summary: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$summaryProps, ReactDOM$summaryInstance>,
  sup: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$supProps, ReactDOM$supInstance>,
  table: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$tableProps, ReactDOM$tableInstance>,
  tbody: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$tbodyProps, ReactDOM$tbodyInstance>,
  td: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$tdProps, ReactDOM$tdInstance>,
  textarea: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$textareaProps, ReactDOM$textareaInstance>,
  tfoot: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$tfootProps, ReactDOM$tfootInstance>,
  th: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$thProps, ReactDOM$thInstance>,
  thead: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$theadProps, ReactDOM$theadInstance>,
  time: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$timeProps, ReactDOM$timeInstance>,
  title: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$titleProps, ReactDOM$titleInstance>,
  tr: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$trProps, ReactDOM$trInstance>,
  track: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$trackProps, ReactDOM$trackInstance>,
  u: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$uProps, ReactDOM$uInstance>,
  ul: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$ulProps, ReactDOM$ulInstance>,
  'var': ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$varProps, ReactDOM$varInstance>,
  video: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$videoProps, ReactDOM$videoInstance>,
  wbr: ReactDOM$HTMLElementJSXIntrinsicTyped<ReactDOM$wbrProps, ReactDOM$wbrInstance>,
  // SVG
  svg: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$svgProps, ReactDOM$svgInstance>,
  animate: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$animateProps, ReactDOM$animateInstance>,
  animateMotion: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$animateMotionProps, ReactDOM$animateMotionInstance>,
  animateTransform: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$animateTransformProps, ReactDOM$animateTransformInstance>,
  circle: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$circleProps, ReactDOM$circleInstance>,
  clipPath: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$clipPathProps, ReactDOM$clipPathInstance>,
  defs: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$defsProps, ReactDOM$defsInstance>,
  desc: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$descProps, ReactDOM$descInstance>,
  ellipse: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$ellipseProps, ReactDOM$ellipseInstance>,
  feBlend: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feBlendProps, ReactDOM$feBlendInstance>,
  feColorMatrix: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feColorMatrixProps, ReactDOM$feColorMatrixInstance>,
  feComponentTransfer: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feComponentTransferProps, ReactDOM$feComponentTransferInstance>,
  feComposite: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feCompositeProps, ReactDOM$feCompositeInstance>,
  feConvolveMatrix: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feConvolveMatrixProps, ReactDOM$feConvolveMatrixInstance>,
  feDiffuseLighting: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feDiffuseLightingProps, ReactDOM$feDiffuseLightingInstance>,
  feDisplacementMap: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feDisplacementMapProps, ReactDOM$feDisplacementMapInstance>,
  feDistantLight: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feDistantLightProps, ReactDOM$feDistantLightInstance>,
  feDropShadow: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feDropShadowProps, ReactDOM$feDropShadowInstance>,
  feFlood: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feFloodProps, ReactDOM$feFloodInstance>,
  feFuncA: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feFuncAProps, ReactDOM$feFuncAInstance>,
  feFuncB: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feFuncBProps, ReactDOM$feFuncBInstance>,
  feFuncG: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feFuncGProps, ReactDOM$feFuncGInstance>,
  feFuncR: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feFuncRProps, ReactDOM$feFuncRInstance>,
  feGaussianBlur: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feGaussianBlurProps, ReactDOM$feGaussianBlurInstance>,
  feImage: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feImageProps, ReactDOM$feImageInstance>,
  feMerge: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feMergeProps, ReactDOM$feMergeInstance>,
  feMergeNode: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feMergeNodeProps, ReactDOM$feMergeNodeInstance>,
  feMorphology: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feMorphologyProps, ReactDOM$feMorphologyInstance>,
  feOffset: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feOffsetProps, ReactDOM$feOffsetInstance>,
  fePointLight: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$fePointLightProps, ReactDOM$fePointLightInstance>,
  feSpecularLighting: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feSpecularLightingProps, ReactDOM$feSpecularLightingInstance>,
  feSpotLight: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feSpotLightProps, ReactDOM$feSpotLightInstance>,
  feTile: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feTileProps, ReactDOM$feTileInstance>,
  feTurbulence: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$feTurbulenceProps, ReactDOM$feTurbulenceInstance>,
  filter: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$filterProps, ReactDOM$filterInstance>,
  foreignObject: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$foreignObjectProps, ReactDOM$foreignObjectInstance>,
  g: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$gProps, ReactDOM$gInstance>,
  image: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$imageProps, ReactDOM$imageInstance>,
  line: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$lineProps, ReactDOM$lineInstance>,
  linearGradient: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$linearGradientProps, ReactDOM$linearGradientInstance>,
  marker: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$markerProps, ReactDOM$markerInstance>,
  mask: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$maskProps, ReactDOM$maskInstance>,
  metadata: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$metadataProps, ReactDOM$metadataInstance>,
  mpath: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$mpathProps, ReactDOM$mpathInstance>,
  path: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$pathProps, ReactDOM$pathInstance>,
  pattern: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$patternProps, ReactDOM$patternInstance>,
  polygon: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$polygonProps, ReactDOM$polygonInstance>,
  polyline: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$polylineProps, ReactDOM$polylineInstance>,
  radialGradient: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$radialGradientProps, ReactDOM$radialGradientInstance>,
  rect: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$rectProps, ReactDOM$rectInstance>,
  set: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$setProps, ReactDOM$setInstance>,
  stop: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$stopProps, ReactDOM$stopInstance>,
  switch: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$switchProps, ReactDOM$switchInstance>,
  symbol: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$symbolProps, ReactDOM$symbolInstance>,
  text: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$textProps, ReactDOM$textInstance>,
  textPath: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$textPathProps, ReactDOM$textPathInstance>,
  tspan: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$tspanProps, ReactDOM$tspanInstance>,
  use: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$useProps, ReactDOM$useInstance>,
  view: ReactDOM$SVGElementJSXIntrinsicTyped<ReactDOM$viewProps, ReactDOM$viewInstance>,
};

type ReactDOM$HTMLElementJSXIntrinsic = {
  instance: HTMLElement,
  props: {
    readonly [key: string]: any,
    readonly children?: React$Node,
    ...
  },
  ...
};

type ReactDOM$SVGElementJSXIntrinsic = {
  instance: Element,
  props: {
    readonly [key: string]: any,
    readonly children?: React$Node,
    ...
  },
  ...
};

type ReactDOM$SVGElementJSXIntrinsicTyped<Props extends {...}, Instance extends Element> = {
  instance: Instance,
  props: {
    ...Props,
    ...ReactDOM$EventHandlers<Instance>,
    // We add data props here to avoid spreading errors
    [key: StringPrefix<'data-'>]: ReactDOM$DataPropValues,
  },
};

// Override this if you want to add custom props to all HTML elements
type ReactDOM$CustomHTMLElementProps = Readonly<{}>;

// Override this if you want to change the types accepted by data-prefixed props
type ReactDOM$DataPropValues = ?(string | boolean | number);

// Override this if you want to change the types accepted for user-visible DOM attributes
type ReactDOM$UserVisibleString = string;

// Override this if you want to add custom events to all HTML elements
type ReactDOM$CustomEvents<in E> = Readonly<{}>;

type ReactDOM$BooleanishString = boolean | 'true' | 'false';

type ReactDOM$Style = any;

type ReactDOM$Number = number | string;

type ReactDOM$Boolean<AttributeName extends string> = AttributeName | boolean;

type ReactDOM$CrossOrigin = ?('anonymous' | 'use-credentials' | '');

// Adapted from https://github.com/DefinitelyTyped/DefinitelyTyped/blob/ffe46d9382e765fc0f54530b4653e57e6ef0921c/types/react/index.d.ts#L2377
// All the WAI-ARIA 1.1 attributes from https://www.w3.org/TR/wai-aria-1.1/
type ReactDOM$AriaAttributes = {
  /** Identifies the currently active element when DOM focus is on a composite widget, textbox, group, or application. */
  'aria-activedescendant'?: ?string,
  /** Indicates whether assistive technologies will present all, or only parts of, the changed region based on the change notifications defined by the aria-relevant attribute. */
  'aria-atomic'?: ?ReactDOM$BooleanishString,
  /**
   * Indicates whether inputting text could trigger display of one or more predictions of the user's intended value for an input and specifies how predictions would be
   * presented if they are made.
   */
  'aria-autocomplete'?: ?('none' | 'inline' | 'list' | 'both'),
  /** Indicates an element is being modified and that assistive technologies MAY want to wait until the modifications are complete before exposing them to the user. */
  /**
   * Defines a string value that labels the current element, which is intended to be converted into Braille.
   * @see aria-label.
   */
  'aria-braillelabel'?: ?ReactDOM$UserVisibleString,
  /**
   * Defines a human-readable, author-localized abbreviated description for the role of an element, which is intended to be converted into Braille.
   * @see aria-roledescription.
   */
  'aria-brailleroledescription'?: ?ReactDOM$UserVisibleString,
  'aria-busy'?: ?ReactDOM$BooleanishString,
  /**
   * Indicates the current "checked" state of checkboxes, radio buttons, and other widgets.
   * @see aria-pressed @see aria-selected.
   */
  'aria-checked'?: ?(ReactDOM$BooleanishString | 'mixed'),
  /**
   * Defines the total number of columns in a table, grid, or treegrid.
   * @see aria-colindex.
   */
  'aria-colcount'?: ?number,
  /**
   * Defines an element's column index or position with respect to the total number of columns within a table, grid, or treegrid.
   * @see aria-colcount @see aria-colspan.
   */
  'aria-colindex'?: ?number,
  /**
   * Defines a human readable text alternative of aria-colindex.
   * @see aria-rowindextext.
   */
  'aria-colindextext'?: ?ReactDOM$UserVisibleString,
  /**
   * Defines the number of columns spanned by a cell or gridcell within a table, grid, or treegrid.
   * @see aria-colindex @see aria-rowspan.
   */
  'aria-colspan'?: ?number,
  /**
   * Identifies the element (or elements) whose contents or presence are controlled by the current element.
   * @see aria-owns.
   */
  'aria-controls'?: ?string,
  /** Indicates the element that represents the current item within a container or set of related elements. */
  'aria-current'?: ?(
    | ReactDOM$BooleanishString
    | 'page'
    | 'step'
    | 'location'
    | 'date'
    | 'time'
  ),
  /**
   * Identifies the element (or elements) that describes the object.
   * @see aria-labelledby
   */
  'aria-describedby'?: ?string,
  /**
   * Defines a string value that describes or annotates the current element.
   * @see related aria-describedby.
   */
  'aria-description'?: ?ReactDOM$UserVisibleString,
  /**
   * Identifies the element that provides a detailed, extended description for the object.
   * @see aria-describedby.
   */
  'aria-details'?: ?string,
  /**
   * Indicates that the element is perceivable but disabled, so it is not editable or otherwise operable.
   * @see aria-hidden @see aria-readonly.
   */
  'aria-disabled'?: ?ReactDOM$BooleanishString,
  /**
   * Indicates what functions can be performed when a dragged object is released on the drop target.
   * @deprecated in ARIA 1.1
   */
  'aria-dropeffect'?: ?(
    | 'none'
    | 'copy'
    | 'execute'
    | 'link'
    | 'move'
    | 'popup'
  ),
  /**
   * Identifies the element that provides an error message for the object.
   * @see aria-invalid @see aria-describedby.
   */
  'aria-errormessage'?: ?string,
  /** Indicates whether the element, or another grouping element it controls, is currently expanded or collapsed. */
  'aria-expanded'?: ?ReactDOM$BooleanishString,
  /**
   * Identifies the next element (or elements) in an alternate reading order of content which, at the user's discretion,
   * allows assistive technology to override the general default of reading in document source order.
   */
  'aria-flowto'?: ?string,
  /**
   * Indicates an element's "grabbed" state in a drag-and-drop operation.
   * @deprecated in ARIA 1.1
   */
  'aria-grabbed'?: ?ReactDOM$BooleanishString,
  /** Indicates the availability and type of interactive popup element, such as menu or dialog, that can be triggered by an element. */
  'aria-haspopup'?: ?(
    | ReactDOM$BooleanishString
    | 'menu'
    | 'listbox'
    | 'tree'
    | 'grid'
    | 'dialog'
  ),
  /**
   * Indicates whether the element is exposed to an accessibility API.
   * @see aria-disabled.
   */
  'aria-hidden'?: ?ReactDOM$BooleanishString,
  /**
   * Indicates the entered value does not conform to the format expected by the application.
   * @see aria-errormessage.
   */
  'aria-invalid'?: ?(ReactDOM$BooleanishString | 'grammar' | 'spelling'),
  /** Indicates keyboard shortcuts that an author has implemented to activate or give focus to an element. */
  'aria-keyshortcuts'?: ?string,
  /**
   * Defines a string value that labels the current element.
   * @see aria-labelledby.
   */
  'aria-label'?: ?ReactDOM$UserVisibleString,
  /**
   * Identifies the element (or elements) that labels the current element.
   * @see aria-describedby.
   */
  'aria-labelledby'?: ?string,
  /** Defines the hierarchical level of an element within a structure. */
  'aria-level'?: ?number,
  /** Indicates that an element will be updated, and describes the types of updates the user agents, assistive technologies, and user can expect from the live region. */
  'aria-live'?: ?('off' | 'assertive' | 'polite'),
  /** Indicates whether an element is modal when displayed. */
  'aria-modal'?: ?ReactDOM$BooleanishString,
  /** Indicates whether a text box accepts multiple lines of input or only a single line. */
  'aria-multiline'?: ?ReactDOM$BooleanishString,
  /** Indicates that the user may select more than one item from the current selectable descendants. */
  'aria-multiselectable'?: ?ReactDOM$BooleanishString,
  /** Indicates whether the element's orientation is horizontal, vertical, or unknown/ambiguous. */
  'aria-orientation'?: ?('horizontal' | 'vertical'),
  /**
   * Identifies an element (or elements) in order to define a visual, functional, or contextual parent/child relationship
   * between DOM elements where the DOM hierarchy cannot be used to represent the relationship.
   * @see aria-controls.
   */
  'aria-owns'?: ?string,
  /**
   * Defines a short hint (a word or short phrase) intended to aid the user with data entry when the control has no value.
   * A hint could be a sample value or a brief description of the expected format.
   */
  'aria-placeholder'?: ?ReactDOM$UserVisibleString,
  /**
   * Defines an element's number or position in the current set of listitems or treeitems. Not required if all elements in the set are present in the DOM.
   * @see aria-setsize.
   */
  'aria-posinset'?: ?number,
  /**
   * Indicates the current "pressed" state of toggle buttons.
   * @see aria-checked @see aria-selected.
   */
  'aria-pressed'?: ?(ReactDOM$BooleanishString | 'mixed'),
  /**
   * Indicates that the element is not editable, but is otherwise operable.
   * @see aria-disabled.
   */
  'aria-readonly'?: ?ReactDOM$BooleanishString,
  /**
   * Indicates what notifications the user agent will trigger when the accessibility tree within a live region is modified.
   * @see aria-atomic.
   */
  'aria-relevant'?: ?(
    | 'additions'
    | 'additions removals'
    | 'additions text'
    | 'all'
    | 'removals'
    | 'removals additions'
    | 'removals text'
    | 'text'
    | 'text additions'
    | 'text removals'
  ),
  /** Indicates that user input is required on the element before a form may be submitted. */
  'aria-required'?: ?ReactDOM$BooleanishString,
  /** Defines a human-readable, author-localized description for the role of an element. */
  'aria-roledescription'?: ?ReactDOM$UserVisibleString,
  /**
   * Defines the total number of rows in a table, grid, or treegrid.
   * @see aria-rowindex.
   */
  'aria-rowcount'?: ?number,
  /**
   * Defines an element's row index or position with respect to the total number of rows within a table, grid, or treegrid.
   * @see aria-rowcount @see aria-rowspan.
   */
  'aria-rowindex'?: ?number,
  /**
   * Defines a human readable text alternative of aria-rowindex.
   * @see aria-colindextext.
   */
  'aria-rowindextext'?: ?ReactDOM$UserVisibleString,
  /**
   * Defines the number of rows spanned by a cell or gridcell within a table, grid, or treegrid.
   * @see aria-rowindex @see aria-colspan.
   */
  'aria-rowspan'?: ?number,
  /**
   * Indicates the current "selected" state of various widgets.
   * @see aria-checked @see aria-pressed.
   */
  'aria-selected'?: ?ReactDOM$BooleanishString,
  /**
   * Defines the number of items in the current set of listitems or treeitems. Not required if all elements in the set are present in the DOM.
   * @see aria-posinset.
   */
  'aria-setsize'?: ?number,
  /** Indicates if items in a table or grid are sorted in ascending or descending order. */
  'aria-sort'?: ?('none' | 'ascending' | 'descending' | 'other'),
  /** Defines the maximum allowed value for a range widget. */
  'aria-valuemax'?: ?number,
  /** Defines the minimum allowed value for a range widget. */
  'aria-valuemin'?: ?number,
  /**
   * Defines the current value for a range widget.
   * @see aria-valuetext.
   */
  'aria-valuenow'?: ?number,
  /** Defines the human readable text alternative of aria-valuenow for a range widget. */
  'aria-valuetext'?: ?ReactDOM$UserVisibleString,
};

type ReactDOM$EventHandlers<in E> = Readonly<{
  // Animation Events
  onAnimationEnd?: ?(event: SyntheticAnimationEvent<E>) => unknown,
  onAnimationEndCapture?: ?(event: SyntheticAnimationEvent<E>) => unknown,
  onAnimationIteration?: ?(event: SyntheticAnimationEvent<E>) => unknown,
  onAnimationIterationCapture?: ?(event: SyntheticAnimationEvent<E>) => unknown,
  onAnimationStart?: ?(event: SyntheticAnimationEvent<E>) => unknown,
  onAnimationStartCapture?: ?(event: SyntheticAnimationEvent<E>) => unknown,

  // Clipboard Events
  onCopy?: ?(event: SyntheticEvent<E>) => unknown,
  onCopyCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onCut?: ?(event: SyntheticEvent<E>) => unknown,
  onCutCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onPaste?: ?(event: SyntheticEvent<E>) => unknown,
  onPasteCapture?: ?(event: SyntheticEvent<E>) => unknown,

  // Composition Events
  onCompositionEnd?: ?(event: SyntheticCompositionEvent<E>) => unknown,
  onCompositionEndCapture?: ?(event: SyntheticCompositionEvent<E>) => unknown,
  onCompositionStart?: ?(event: SyntheticCompositionEvent<E>) => unknown,
  onCompositionStartCapture?: ?(event: SyntheticCompositionEvent<E>) => unknown,
  onCompositionUpdate?: ?(event: SyntheticCompositionEvent<E>) => unknown,
  onCompositionUpdateCapture?: ?(event: SyntheticCompositionEvent<E>) => unknown,

  // Focus Events
  onFocus?: ?(event: SyntheticFocusEvent<E>) => unknown,
  onFocusCapture?: ?(event: SyntheticFocusEvent<E>) => unknown,
  onBlur?: ?(event: SyntheticFocusEvent<E>) => unknown,
  onBlurCapture?: ?(event: SyntheticFocusEvent<E>) => unknown,

  // Form Events
  onBeforeInput?: ?(event: SyntheticInputEvent<E>) => unknown,
  onBeforeInputCapture?: ?(event: SyntheticInputEvent<E>) => unknown,
  onChange?: ?(event: SyntheticEvent<E>) => unknown,
  onChangeCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onInput?: ?(event: SyntheticEvent<E>) => unknown,
  onInputCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onInvalid?: ?(event: SyntheticEvent<E>) => unknown,
  onInvalidCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onReset?: ?(event: SyntheticEvent<E>) => unknown,
  onResetCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onSubmit?: ?(event: SyntheticEvent<E>) => unknown,
  onSubmitCapture?: ?(event: SyntheticEvent<E>) => unknown,

  // Image Events
  onError?: ?(event: SyntheticEvent<E>) => unknown,
  onErrorCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onLoad?: ?(event: SyntheticEvent<E>) => unknown,
  onLoadCapture?: ?(event: SyntheticEvent<E>) => unknown,

  // Keyboard Events
  onKeyDown?: ?(event: SyntheticKeyboardEvent<E>) => unknown,
  onKeyDownCapture?: ?(event: SyntheticKeyboardEvent<E>) => unknown,
  onKeyPress?: ?(event: SyntheticKeyboardEvent<E>) => unknown,
  onKeyPressCapture?: ?(event: SyntheticKeyboardEvent<E>) => unknown,
  onKeyUp?: ?(event: SyntheticKeyboardEvent<E>) => unknown,
  onKeyUpCapture?: ?(event: SyntheticKeyboardEvent<E>) => unknown,

  // Media Events
  onAbort?: ?(event: SyntheticEvent<E>) => unknown,
  onAbortCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onCanPlay?: ?(event: SyntheticEvent<E>) => unknown,
  onCanPlayCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onCanPlayThrough?: ?(event: SyntheticEvent<E>) => unknown,
  onCanPlayThroughCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onDurationChange?: ?(event: SyntheticEvent<E>) => unknown,
  onDurationChangeCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onEmptied?: ?(event: SyntheticEvent<E>) => unknown,
  onEmptiedCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onEncrypted?: ?(event: SyntheticEvent<E>) => unknown,
  onEncryptedCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onEnded?: ?(event: SyntheticEvent<E>) => unknown,
  onEndedCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onLoadedData?: ?(event: SyntheticEvent<E>) => unknown,
  onLoadedDataCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onLoadedMetadata?: ?(event: SyntheticEvent<E>) => unknown,
  onLoadedMetadataCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onLoadStart?: ?(event: SyntheticEvent<E>) => unknown,
  onLoadStartCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onPause?: ?(event: SyntheticEvent<E>) => unknown,
  onPauseCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onPlay?: ?(event: SyntheticEvent<E>) => unknown,
  onPlayCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onPlaying?: ?(event: SyntheticEvent<E>) => unknown,
  onPlayingCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onProgress?: ?(event: SyntheticEvent<E>) => unknown,
  onProgressCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onRateChange?: ?(event: SyntheticEvent<E>) => unknown,
  onRateChangeCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onResize?: ?(event: SyntheticEvent<E>) => unknown,
  onResizeCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onSeeked?: ?(event: SyntheticEvent<E>) => unknown,
  onSeekedCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onSeeking?: ?(event: SyntheticEvent<E>) => unknown,
  onSeekingCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onStalled?: ?(event: SyntheticEvent<E>) => unknown,
  onStalledCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onSuspend?: ?(event: SyntheticEvent<E>) => unknown,
  onSuspendCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onTimeUpdate?: ?(event: SyntheticEvent<E>) => unknown,
  onTimeUpdateCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onVolumeChange?: ?(event: SyntheticEvent<E>) => unknown,
  onVolumeChangeCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onWaiting?: ?(event: SyntheticEvent<E>) => unknown,
  onWaitingCapture?: ?(event: SyntheticEvent<E>) => unknown,

  // Mouse Events
  onAuxClick?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onAuxClickCapture?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onClick?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onClickCapture?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onContextMenu?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onContextMenuCapture?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onDoubleClick?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onDoubleClickCapture?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onDrag?: ?(event: SyntheticDragEvent<E>) => unknown,
  onDragCapture?: ?(event: SyntheticDragEvent<E>) => unknown,
  onDragEnd?: ?(event: SyntheticDragEvent<E>) => unknown,
  onDragEndCapture?: ?(event: SyntheticDragEvent<E>) => unknown,
  onDragEnter?: ?(event: SyntheticDragEvent<E>) => unknown,
  onDragEnterCapture?: ?(event: SyntheticDragEvent<E>) => unknown,
  onDragExit?: ?(event: SyntheticDragEvent<E>) => unknown,
  onDragExitCapture?: ?(event: SyntheticDragEvent<E>) => unknown,
  onDragLeave?: ?(event: SyntheticDragEvent<E>) => unknown,
  onDragLeaveCapture?: ?(event: SyntheticDragEvent<E>) => unknown,
  onDragOver?: ?(event: SyntheticDragEvent<E>) => unknown,
  onDragOverCapture?: ?(event: SyntheticDragEvent<E>) => unknown,
  onDragStart?: ?(event: SyntheticDragEvent<E>) => unknown,
  onDragStartCapture?: ?(event: SyntheticDragEvent<E>) => unknown,
  onDrop?: ?(event: SyntheticDragEvent<E>) => unknown,
  onDropCapture?: ?(event: SyntheticDragEvent<E>) => unknown,
  onMouseDown?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onMouseDownCapture?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onMouseEnter?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onMouseLeave?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onMouseMove?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onMouseMoveCapture?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onMouseOut?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onMouseOutCapture?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onMouseOver?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onMouseOverCapture?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onMouseUp?: ?(event: SyntheticMouseEvent<E>) => unknown,
  onMouseUpCapture?: ?(event: SyntheticMouseEvent<E>) => unknown,

  // Pointer Events
  onGotPointerCapture?: ?(event: SyntheticPointerEvent<E>) => unknown,
  onGotPointerCaptureCapture?: ?(event: SyntheticPointerEvent<E>) => unknown,
  onLostPointerCapture?: ?(event: SyntheticPointerEvent<E>) => unknown,
  onLostPointerCaptureCapture?: ?(event: SyntheticPointerEvent<E>) => unknown,
  onPointerCancel?: ?(event: SyntheticPointerEvent<E>) => unknown,
  onPointerCancelCapture?: ?(event: SyntheticPointerEvent<E>) => unknown,
  onPointerDown?: ?(event: SyntheticPointerEvent<E>) => unknown,
  onPointerDownCapture?: ?(event: SyntheticPointerEvent<E>) => unknown,
  onPointerEnter?: ?(event: SyntheticPointerEvent<E>) => unknown,
  onPointerLeave?: ?(event: SyntheticPointerEvent<E>) => unknown,
  onPointerMove?: ?(event: SyntheticPointerEvent<E>) => unknown,
  onPointerMoveCapture?: ?(event: SyntheticPointerEvent<E>) => unknown,
  onPointerOut?: ?(event: SyntheticPointerEvent<E>) => unknown,
  onPointerOutCapture?: ?(event: SyntheticPointerEvent<E>) => unknown,
  onPointerOver?: ?(event: SyntheticPointerEvent<E>) => unknown,
  onPointerOverCapture?: ?(event: SyntheticPointerEvent<E>) => unknown,
  onPointerUp?: ?(event: SyntheticPointerEvent<E>) => unknown,
  onPointerUpCapture?: ?(event: SyntheticPointerEvent<E>) => unknown,

  // Selection Events
  onSelect?: ?(event: SyntheticEvent<E>) => unknown,
  onSelectCapture?: ?(event: SyntheticEvent<E>) => unknown,

  // Toggle Events
  onToggle?: ?(event: SyntheticEvent<E>) => unknown,
  onToggleCapture?: ?(event: SyntheticEvent<E>) => unknown,

  // Touch Events
  onTouchCancel?: ?(event: SyntheticTouchEvent<E>) => unknown,
  onTouchCancelCapture?: ?(event: SyntheticTouchEvent<E>) => unknown,
  onTouchEnd?: ?(event: SyntheticTouchEvent<E>) => unknown,
  onTouchEndCapture?: ?(event: SyntheticTouchEvent<E>) => unknown,
  onTouchMove?: ?(event: SyntheticEvent<E>) => unknown,
  onTouchMoveCapture?: ?(event: SyntheticEvent<E>) => unknown,
  onTouchStart?: ?(event: SyntheticTouchEvent<E>) => unknown,
  onTouchStartCapture?: ?(event: SyntheticTouchEvent<E>) => unknown,

  // Transition Events
  onTransitionCancel?: ?(event: SyntheticTransitionEvent<E>) => unknown,
  onTransitionCancelCapture?: ?(event: SyntheticTransitionEvent<E>) => unknown,
  onTransitionEnd?: ?(event: SyntheticTransitionEvent<E>) => unknown,
  onTransitionEndCapture?: ?(event: SyntheticTransitionEvent<E>) => unknown,
  onTransitionRun?: ?(event: SyntheticTransitionEvent<E>) => unknown,
  onTransitionRunCapture?: ?(event: SyntheticTransitionEvent<E>) => unknown,
  onTransitionStart?: ?(event: SyntheticTransitionEvent<E>) => unknown,
  onTransitionStartCapture?: ?(event: SyntheticTransitionEvent<E>) => unknown,

  // UI Events
  onScroll?: ?(event: SyntheticUIEvent<E>) => unknown,
  onScrollCapture?: ?(event: SyntheticUIEvent<E>) => unknown,

  // Wheel Events
  onWheel?: ?(event: SyntheticWheelEvent<E>) => unknown,
  onWheelCapture?: ?(event: SyntheticWheelEvent<E>) => unknown,

  ...ReactDOM$CustomEvents<E>,
}>;

// Special props provided by React
type ReactDOM$ReactSpecificProps = {
  children?: React$Node,
  dangerouslySetInnerHTML?: {
    __html: ReactDOM$UserVisibleString,
  },
  defaultChecked?: ?boolean,
  defaultValue?: ?(string | number | ReadonlyArray<string>),
  suppressContentEditableWarning?: ?boolean,
  suppressHydrationWarning?: ?boolean,
};

type ReactDOM$HTMLElementProps = {
  ...ReactDOM$ReactSpecificProps,
  ...ReactDOM$AriaAttributes,
  accessKey?: ?string,
  autoCapitalize?: ?(
    | 'off'
    | 'none'
    | 'on'
    | 'sentences'
    | 'words'
    | 'characters'
  ),
  autoCorrect?: ?('off' | 'on'),
  autoFocus?: ?boolean,
  className?: ?string,
  contentEditable?: ?ReactDOM$BooleanishString,
  contextMenu?: ?string,
  dir?: ?('ltr' | 'rtl' | 'LTR' | 'RTL' | 'auto'),
  draggable?: ?ReactDOM$BooleanishString,
  enterKeyHint?: ?(
    | 'enter'
    | 'done'
    | 'go'
    | 'next'
    | 'previous'
    | 'search'
    | 'send'
  ),
  hidden?: ?ReactDOM$Boolean<'hidden'>,
  id?: ?string,
  inert?: ?ReactDOM$BooleanishString,
  inputMode?: ?(
    | 'none'
    | 'text'
    | 'tel'
    | 'url'
    | 'email'
    | 'numeric'
    | 'decimal'
    | 'search'
  ),
  is?: ?string,
  itemID?: ?string,
  itemProp?: ?string,
  itemRef?: ?string,
  itemScope?: ReactDOM$Boolean<'itemScope' | 'itemscope'>,
  itemType?: ?string,
  lang?: ?string,
  nonce?: ?string,
  popover?: ?('' | 'auto' | 'manual'),
  role?: ?string,
  slot?: ?string,
  spellCheck?: ?ReactDOM$BooleanishString,
  style?: ?ReactDOM$Style,
  suppressContentEditableWarning?: ?boolean,
  tabIndex?: ?number,
  title?: ?ReactDOM$UserVisibleString,
  translate?: ?('yes' | 'no'),
  ...ReactDOM$CustomHTMLElementProps,
};

// Self closing tags, like br, do not allow children
type ReactDOM$SelfClosingHTMLElementProps = Omit<
  ReactDOM$HTMLElementProps,
  'children' | 'dangerouslySetInnerHTML',
>;

type ReactDOM$HTMLElementJSXIntrinsicTyped<Props, Instance = HTMLElement> = {
  instance: Instance,
  props: {
    ...Props,
    ...ReactDOM$EventHandlers<Instance>,
    // We add data props here to avoid spreading errors
    [key: StringPrefix<'data-'>]: ReactDOM$DataPropValues,
  },
};

type ReactDOM$HTMLAttributeReferrerPolicy =
  | ''
  | 'no-referrer'
  | 'no-referrer-when-downgrade'
  | 'origin'
  | 'origin-when-cross-origin'
  | 'same-origin'
  | 'strict-origin'
  | 'strict-origin-when-cross-origin'
  | 'unsafe-url';

type ReactDOM$HTMLAttributeAnchorTarget =
  | '_self'
  | '_blank'
  | '_parent'
  | '_top';

type ReactDOM$aProps = {
  ...ReactDOM$HTMLElementProps,
  download?: any,
  href?: ?string,
  hrefLang?: ?string,
  media?: ?string,
  ping?: ?string,
  referrerPolicy?: ?ReactDOM$HTMLAttributeReferrerPolicy,
  rel?: ?string,
  target?: ?ReactDOM$HTMLAttributeAnchorTarget,
  type?: ?string,
};
type ReactDOM$aInstance = HTMLAnchorElement;

type ReactDOM$abbrProps = ReactDOM$HTMLElementProps;
type ReactDOM$abbrInstance = HTMLElement;

type ReactDOM$addressProps = ReactDOM$HTMLElementProps;
type ReactDOM$addressInstance = HTMLElement;

type ReactDOM$areaProps = {
  ...ReactDOM$SelfClosingHTMLElementProps,
  alt?: ?ReactDOM$UserVisibleString,
  coords?: ?string,
  download?: any,
  href?: ?string,
  hrefLang?: ?string,
  media?: ?string,
  referrerPolicy?: ?ReactDOM$HTMLAttributeReferrerPolicy,
  shape?: ?string,
  target?: ?string,
};
type ReactDOM$areaInstance = HTMLAreaElement;

type ReactDOM$articleProps = ReactDOM$HTMLElementProps;
type ReactDOM$articleInstance = HTMLElement;

type ReactDOM$asideProps = ReactDOM$HTMLElementProps;
type ReactDOM$asideInstance = HTMLElement;

type ReactDOM$audioProps = {
  ...ReactDOM$HTMLElementProps,
  autoPlay?: ?boolean,
  controls?: ?boolean,
  controlsList?: ?string,
  crossOrigin?: ?ReactDOM$CrossOrigin,
  disableRemotePlayback?: ?boolean,
  loop?: ?boolean,
  muted?: ?boolean,
  preload?: ?string,
  src?: ?string,
};
type ReactDOM$audioInstance = HTMLAudioElement;

type ReactDOM$bProps = ReactDOM$HTMLElementProps;
type ReactDOM$bInstance = HTMLElement;

type ReactDOM$baseProps = {
  ...ReactDOM$SelfClosingHTMLElementProps,
  href?: ?string,
  target?: ?string,
};
type ReactDOM$baseInstance = HTMLBaseElement;

type ReactDOM$bdiProps = ReactDOM$HTMLElementProps;
type ReactDOM$bdiInstance = HTMLElement;

type ReactDOM$bdoProps = ReactDOM$HTMLElementProps;
type ReactDOM$bdoInstance = HTMLElement;

type ReactDOM$bigProps = ReactDOM$HTMLElementProps;
type ReactDOM$bigInstance = HTMLElement;

type ReactDOM$blockquoteProps = ReactDOM$HTMLElementProps;
type ReactDOM$blockquoteInstance = HTMLQuoteElement;

type ReactDOM$bodyProps = ReactDOM$HTMLElementProps;
type ReactDOM$bodyInstance = HTMLBodyElement;

type ReactDOM$brProps = ReactDOM$SelfClosingHTMLElementProps;
type ReactDOM$brInstance = HTMLBRElement;

type ReactDOM$buttonProps = {
  ...ReactDOM$HTMLElementProps,
  disabled?: ?boolean,
  form?: ?string,
  formAction?: ?(string | ((formData: FormData) => void | Promise<void>)),
  formEncType?: ?string,
  formMethod?: ?string,
  formNoValidate?: ?boolean,
  formTarget?: ?string,
  name?: ?string,
  type?: ?('submit' | 'reset' | 'button'),
  value?: ?(string | ReadonlyArray<string> | number),
};
type ReactDOM$buttonInstance = HTMLButtonElement;

type ReactDOM$canvasProps = {
  ...ReactDOM$HTMLElementProps,
  height?: ?(number | string),
  width?: ?(number | string),
};
type ReactDOM$canvasInstance = HTMLCanvasElement;

type ReactDOM$captionProps = ReactDOM$HTMLElementProps;
type ReactDOM$captionInstance = HTMLTableCaptionElement;

type ReactDOM$citeProps = ReactDOM$HTMLElementProps;
type ReactDOM$citeInstance = HTMLElement;

type ReactDOM$codeProps = ReactDOM$HTMLElementProps;
type ReactDOM$codeInstance = HTMLElement;

type ReactDOM$colProps = ReactDOM$SelfClosingHTMLElementProps;
type ReactDOM$colInstance = HTMLTableColElement;

type ReactDOM$colgroupProps = {
  ...ReactDOM$HTMLElementProps,
  span?: ?number,
};
type ReactDOM$colgroupInstance = HTMLTableColElement;

type ReactDOM$dataProps = {
  ...ReactDOM$HTMLElementProps,
  value?: ?(string | ReadonlyArray<string> | number),
};

type ReactDOM$dataInstance = HTMLDataElement;

type ReactDOM$datalistProps = ReactDOM$HTMLElementProps;
type ReactDOM$datalistInstance = HTMLDataListElement;

type ReactDOM$ddProps = ReactDOM$HTMLElementProps;
type ReactDOM$ddInstance = HTMLElement;

type ReactDOM$delProps = {
  ...ReactDOM$HTMLElementProps,
  cite?: ?string,
  dateTime?: ?string,
};
type ReactDOM$delInstance = HTMLModElement;

type ReactDOM$detailsProps = {
  ...ReactDOM$HTMLElementProps,
  name?: ?string,
  open?: ?boolean,
};
type ReactDOM$detailsInstance = HTMLDetailsElement;

type ReactDOM$dfnProps = ReactDOM$HTMLElementProps;
type ReactDOM$dfnInstance = HTMLElement;

type ReactDOM$dialogProps = {
  ...ReactDOM$HTMLElementProps,
  onCancel?: ?(event: SyntheticEvent<HTMLDialogElement>) => unknown,
  onClose?: ?(event: SyntheticEvent<HTMLDialogElement>) => unknown,
  open?: ?boolean,
};
type ReactDOM$dialogInstance = HTMLDialogElement;

type ReactDOM$divProps = ReactDOM$HTMLElementProps;
type ReactDOM$divInstance = HTMLDivElement;

type ReactDOM$dlProps = ReactDOM$HTMLElementProps;
type ReactDOM$dlInstance = HTMLDListElement;

type ReactDOM$dtProps = ReactDOM$HTMLElementProps;
type ReactDOM$dtInstance = HTMLElement;

type ReactDOM$emProps = ReactDOM$HTMLElementProps;
type ReactDOM$emInstance = HTMLElement;

type ReactDOM$embedProps = ReactDOM$SelfClosingHTMLElementProps;
type ReactDOM$embedInstance = HTMLEmbedElement;

type ReactDOM$fieldsetProps = {
  ...ReactDOM$HTMLElementProps,
  disabled?: ?boolean,
  form?: ?string,
  name?: ?string,
};
type ReactDOM$fieldsetInstance = HTMLFieldSetElement;

type ReactDOM$figcaptionProps = ReactDOM$HTMLElementProps;
type ReactDOM$figcaptionInstance = HTMLElement;

type ReactDOM$figureProps = ReactDOM$HTMLElementProps;
type ReactDOM$figureInstance = HTMLElement;

type ReactDOM$footerProps = ReactDOM$HTMLElementProps;
type ReactDOM$footerInstance = HTMLFormElement;

type ReactDOM$formProps = {
  ...ReactDOM$HTMLElementProps,
  acceptCharset?: ?string,
  action?: ?(string | ((formData: FormData) => void | Promise<void>)),
  autoComplete?: ?string,
  encType?: ?string,
  method?: ?string,
  name?: ?string,
  noValidate?: ?boolean,
  rel?: string,
  target?: ?string,
};
type ReactDOM$formInstance = HTMLFormElement;

type ReactDOM$h1Props = ReactDOM$HTMLElementProps;
type ReactDOM$h1Instance = HTMLHeadingElement;

type ReactDOM$h2Props = ReactDOM$HTMLElementProps;
type ReactDOM$h2Instance = HTMLHeadingElement;

type ReactDOM$h3Props = ReactDOM$HTMLElementProps;
type ReactDOM$h3Instance = HTMLHeadingElement;

type ReactDOM$h4Props = ReactDOM$HTMLElementProps;
type ReactDOM$h4Instance = HTMLHeadingElement;

type ReactDOM$h5Props = ReactDOM$HTMLElementProps;
type ReactDOM$h5Instance = HTMLHeadingElement;

type ReactDOM$h6Props = ReactDOM$HTMLElementProps;
type ReactDOM$h6Instance = HTMLHeadingElement;

type ReactDOM$headProps = ReactDOM$HTMLElementProps;
type ReactDOM$headInstance = HTMLHeadElement;

type ReactDOM$headerProps = ReactDOM$HTMLElementProps;
type ReactDOM$headerInstance = HTMLElement;

type ReactDOM$hgroupProps = ReactDOM$HTMLElementProps;
type ReactDOM$hgroupInstance = HTMLElement;

type ReactDOM$hrProps = ReactDOM$SelfClosingHTMLElementProps;
type ReactDOM$hrInstance = HTMLHRElement;

type ReactDOM$htmlProps = {
  ...ReactDOM$HTMLElementProps,
  manifest?: ?string,
};
type ReactDOM$htmlInstance = HTMLHtmlElement;

type ReactDOM$iProps = ReactDOM$HTMLElementProps;
type ReactDOM$iInstance = HTMLElement;

type ReactDOM$iframeProps = {
  ...ReactDOM$HTMLElementProps,
  allow?: ?string,
  allowFullScreen?: ?boolean,
  allowTransparency?: ?boolean,
  /** @deprecated */
  frameBorder?: ?(number | string),
  height?: ?(number | string),
  loading?: ?('eager' | 'lazy'),
  /** @deprecated */
  marginHeight?: ?number,
  /** @deprecated */
  marginWidth?: ?number,
  name?: ?string,
  referrerPolicy?: ?ReactDOM$HTMLAttributeReferrerPolicy,
  sandbox?: ?string,
  /** @deprecated */
  scrolling?: ?string,
  seamless?: ?boolean,
  src?: ?string,
  srcDoc?: ?string,
  width?: ?(number | string),
};
type ReactDOM$iframeInstance = HTMLIFrameElement;

type ReactDOM$imgProps = {
  ...ReactDOM$SelfClosingHTMLElementProps,
  alt?: ?ReactDOM$UserVisibleString,
  crossOrigin?: ?ReactDOM$CrossOrigin,
  decoding?: ?('async' | 'auto' | 'sync'),
  fetchPriority?: 'high' | 'low' | 'auto',
  height?: ?(number | string),
  loading?: ?('eager' | 'lazy'),
  referrerPolicy?: ?ReactDOM$HTMLAttributeReferrerPolicy,
  sizes?: ?string,
  src?: ?string,
  srcSet?: ?string,
  useMap?: ?string,
  width?: ?(number | string),
};
type ReactDOM$imgInstance = HTMLImageElement;

type ReactDOM$inputProps = {
  ...ReactDOM$HTMLElementProps,
  accept?: ?string,
  alt?: ?ReactDOM$UserVisibleString,
  autoComplete?: ?string,
  capture?: ?(boolean | 'user' | 'environment'),
  checked?: ?boolean,
  disabled?: ?boolean,
  form?: ?string,
  formAction?: ?(string | ((formData: FormData) => void | Promise<void>)),
  formEncType?: ?string,
  formMethod?: ?string,
  formNoValidate?: ?boolean,
  formTarget?: ?string,
  height?: ?string,
  list?: ?string,
  max?: ?(number | string),
  maxLength?: ?number,
  min?: ?(number | string),
  minLength?: ?number,
  multiple?: ?boolean,
  name?: ?string,
  onChange?: ?(event: SyntheticInputEvent<HTMLInputElement>) => unknown,
  pattern?: ?string,
  placeholder?: ?ReactDOM$UserVisibleString,
  readOnly?: ?boolean,
  required?: ?boolean,
  size?: ?number,
  src?: ?string,
  step?: ?(number | string),
  type?: ?string,
  value?: ?(string | ReadonlyArray<string> | number),
  width?: ?(number | string),
};
type ReactDOM$inputInstance = HTMLInputElement;

type ReactDOM$insProps = {
  ...ReactDOM$HTMLElementProps,
  cite?: ?string,
  dateTime?: ?string,
};
type ReactDOM$insInstance = HTMLModElement;

type ReactDOM$kbdProps = ReactDOM$HTMLElementProps;
type ReactDOM$kbdInstance = HTMLElement;

type ReactDOM$keygenProps = {
  ...ReactDOM$SelfClosingHTMLElementProps,
  challenge?: ?string,
  disabled?: ?boolean,
  form?: ?string,
  keyParams?: ?string,
  keyType?: ?string,
  name?: ?string,
};
type ReactDOM$keygenInstance = HTMLElement;

type ReactDOM$labelProps = {
  ...ReactDOM$HTMLElementProps,
  form?: ?string,
  htmlFor?: ?string,
};
type ReactDOM$labelInstance = HTMLLabelElement;

type ReactDOM$legendProps = ReactDOM$HTMLElementProps;
type ReactDOM$legendInstance = HTMLLegendElement;

type ReactDOM$liProps = {
  ...ReactDOM$HTMLElementProps,
  value?: ?(string | ReadonlyArray<string> | number),
};

type ReactDOM$liInstance = HTMLLIElement;

type ReactDOM$linkProps = {
  ...ReactDOM$SelfClosingHTMLElementProps,
  as?: ?string,
  crossOrigin?: ?ReactDOM$CrossOrigin,
  fetchPriority?: ?('high' | 'low' | 'auto'),
  href?: ?string,
  hrefLang?: ?string,
  integrity?: ?string,
  media?: ?string,
  imageSizes?: ?string,
  imageSrcSet?: ?string,
  referrerPolicy?: ?ReactDOM$HTMLAttributeReferrerPolicy,
  rel?: ?string,
  sizes?: ?string,
  type?: ?string,
  charSet?: ?string,

  // React props
  precedence?: ?string,
};
type ReactDOM$linkInstance = HTMLLinkElement;

type ReactDOM$mainProps = ReactDOM$HTMLElementProps;
type ReactDOM$mainInstance = HTMLElement;

type ReactDOM$mapProps = {
  ...ReactDOM$HTMLElementProps,
  name?: ?string,
};
type ReactDOM$mapInstance = HTMLMapElement;

type ReactDOM$markProps = ReactDOM$HTMLElementProps;
type ReactDOM$markInstance = HTMLElement;

type ReactDOM$mediaProps = {
  ...ReactDOM$HTMLElementProps,
  autoPlay?: ?boolean,
  controls?: ?boolean,
  controlsList?: ?string,
  crossOrigin?: ?ReactDOM$CrossOrigin,
  loop?: ?boolean,
  mediaGroup?: ?string,
  muted?: ?boolean,
  playsInline?: ?boolean,
  preload?: ?string,
  src?: ?string,
};
type ReactDOM$mediaInstance = HTMLElement;

type ReactDOM$menuProps = ReactDOM$HTMLElementProps;
type ReactDOM$menuInstance = HTMLMenuElement;

type ReactDOM$menuitemProps = ReactDOM$HTMLElementProps;
type ReactDOM$menuitemInstance = HTMLElement;

type ReactDOM$metaProps = {
  ...ReactDOM$SelfClosingHTMLElementProps,
  charSet?: ?string,
  content?: ?string,
  httpEquiv?: ?string,
  media?: ?string,
  name?: ?string,
};
type ReactDOM$metaInstance = HTMLMetaElement;

type ReactDOM$meterProps = {
  ...ReactDOM$HTMLElementProps,
  form?: ?string,
  high?: ?number,
  low?: ?number,
  max?: ?(number | string),
  min?: ?(number | string),
  optimum?: ?number,
  value?: ?(string | ReadonlyArray<string> | number),
};
type ReactDOM$meterInstance = HTMLMeterElement;

type ReactDOM$navProps = ReactDOM$HTMLElementProps;
type ReactDOM$navInstance = HTMLElement;

type ReactDOM$noscriptProps = ReactDOM$HTMLElementProps;
type ReactDOM$noscriptInstance = HTMLElement;

type ReactDOM$objectProps = {
  ...ReactDOM$HTMLElementProps,
  classID?: ?string,
  data?: ?string,
  form?: ?string,
  height?: ?(number | string),
  name?: ?string,
  type?: ?string,
  useMap?: ?string,
  width?: ?(number | string),
  wmode?: ?string,
};
type ReactDOM$objectInstance = HTMLObjectElement;

type ReactDOM$olProps = {
  ...ReactDOM$HTMLElementProps,
  reversed?: ?boolean,
  start?: ?number,
  type?: ?('1' | 'a' | 'A' | 'i' | 'I'),
};
type ReactDOM$olInstance = HTMLOListElement;

type ReactDOM$optgroupProps = {
  ...ReactDOM$HTMLElementProps,
  disabled?: ?boolean,
  label?: ?ReactDOM$UserVisibleString,
};

type ReactDOM$optgroupInstance = HTMLOptGroupElement;

type ReactDOM$optionProps = {
  ...ReactDOM$HTMLElementProps,
  disabled?: ?boolean,
  label?: ?ReactDOM$UserVisibleString,
  selected?: ?boolean,
  value?: ?(string | ReadonlyArray<string> | number),
};
type ReactDOM$optionInstance = HTMLOptionElement;

type ReactDOM$outputProps = {
  ...ReactDOM$HTMLElementProps,
  form?: ?string,
  htmlFor?: ?string,
  name?: ?string,
};
type ReactDOM$outputInstance = HTMLElement;

type ReactDOM$pProps = ReactDOM$HTMLElementProps;
type ReactDOM$pInstance = HTMLParagraphElement;

type ReactDOM$paramProps = {
  ...ReactDOM$SelfClosingHTMLElementProps,
  name?: ?string,
  value?: ?(string | ReadonlyArray<string> | number),
};
type ReactDOM$paramInstance = HTMLParamElement;

type ReactDOM$pictureProps = ReactDOM$HTMLElementProps;
type ReactDOM$pictureInstance = HTMLPictureElement;

type ReactDOM$preProps = ReactDOM$HTMLElementProps;
type ReactDOM$preInstance = HTMLPreElement;

type ReactDOM$progressProps = {
  ...ReactDOM$HTMLElementProps,
  max?: ?(number | string),
  value?: ?(string | ReadonlyArray<string> | number),
};
type ReactDOM$progressInstance = HTMLProgressElement;

type ReactDOM$qProps = ReactDOM$HTMLElementProps;
type ReactDOM$qInstance = HTMLQuoteElement;

type ReactDOM$rpProps = ReactDOM$HTMLElementProps;
type ReactDOM$rpInstance = HTMLElement;

type ReactDOM$rtProps = ReactDOM$HTMLElementProps;
type ReactDOM$rtInstance = HTMLElement;

type ReactDOM$rubyProps = ReactDOM$HTMLElementProps;
type ReactDOM$rubyInstance = HTMLElement;

type ReactDOM$sProps = ReactDOM$HTMLElementProps;
type ReactDOM$sInstance = HTMLElement;

type ReactDOM$sampProps = ReactDOM$HTMLElementProps;
type ReactDOM$sampInstance = HTMLElement;

type ReactDOM$scriptProps = {
  ...ReactDOM$HTMLElementProps,
  async?: ?boolean,
  /** @deprecated */
  charSet?: ?string,
  crossOrigin?: ?ReactDOM$CrossOrigin,
  defer?: ?boolean,
  integrity?: ?string,
  noModule?: ?boolean,
  referrerPolicy?: ?ReactDOM$HTMLAttributeReferrerPolicy,
  src?: ?string,
  type?: ?string,
};
type ReactDOM$scriptInstance = HTMLScriptElement;

type ReactDOM$sectionProps = ReactDOM$HTMLElementProps;
type ReactDOM$sectionInstance = HTMLElement;

type ReactDOM$selectProps = {
  ...ReactDOM$HTMLElementProps,
  autoComplete?: ?string,
  disabled?: ?boolean,
  form?: ?string,
  multiple?: ?boolean,
  name?: ?string,
  onChange?: ?(event: SyntheticEvent<HTMLSelectElement>) => unknown,
  required?: ?boolean,
  size?: ?number,
  value?: ?(string | ReadonlyArray<string> | number),
};
type ReactDOM$selectInstance = HTMLSelectElement;

type ReactDOM$slotProps = {
  ...ReactDOM$HTMLElementProps,
  name?: ?string,
};
type ReactDOM$slotInstance = HTMLSlotElement;

type ReactDOM$smallProps = ReactDOM$HTMLElementProps;
type ReactDOM$smallInstance = HTMLElement;

type ReactDOM$sourceProps = {
  ...ReactDOM$SelfClosingHTMLElementProps,
  height?: ?(number | string),
  media?: ?string,
  sizes?: ?string,
  src?: ?string,
  srcSet?: ?string,
  type?: ?string,
  width?: ?(number | string),
};
type ReactDOM$sourceInstance = HTMLSourceElement;

type ReactDOM$spanProps = ReactDOM$HTMLElementProps;
type ReactDOM$spanInstance = HTMLSpanElement;

type ReactDOM$strongProps = ReactDOM$HTMLElementProps;
type ReactDOM$strongInstance = HTMLElement;

type ReactDOM$styleProps = {
  ...ReactDOM$HTMLElementProps,
  href?: ?string,
  media?: ?string,
  precedence?: ?string,
  scoped?: ?boolean,
  type?: ?string,
};
type ReactDOM$styleInstance = HTMLStyleElement;

type ReactDOM$subProps = ReactDOM$HTMLElementProps;
type ReactDOM$subInstance = HTMLElement;

type ReactDOM$summaryProps = ReactDOM$HTMLElementProps;
type ReactDOM$summaryInstance = HTMLElement;

type ReactDOM$supProps = ReactDOM$HTMLElementProps;
type ReactDOM$supInstance = HTMLElement;

type ReactDOM$tableProps = {
  ...ReactDOM$HTMLElementProps,
  align?: ?('left' | 'center' | 'right'),
  bgcolor?: ?string,
  border?: ?number,
  cellPadding?: ?(number | string),
  cellSpacing?: ?(number | string),
  frame?: ?boolean,
  rules?: ?('none' | 'groups' | 'rows' | 'columns' | 'all'),
  summary?: ?string,
  width?: ?(number | string),
};
type ReactDOM$tableInstance = HTMLTableElement;

type ReactDOM$tbodyProps = ReactDOM$HTMLElementProps;
type ReactDOM$tbodyInstance = HTMLTableSectionElement;

type ReactDOM$tdProps = {
  ...ReactDOM$HTMLElementProps,
  abbr?: ?string,
  align?: ?('left' | 'center' | 'right' | 'justify' | 'char'),
  colSpan?: ?number,
  headers?: ?string,
  height?: ?(number | string),
  rowSpan?: ?number,
  scope?: ?string,
  valign?: ?('top' | 'middle' | 'bottom' | 'baseline'),
  width?: ?(number | string),
};
type ReactDOM$tdInstance = HTMLTableCellElement;

type ReactDOM$textareaProps = {
  ...ReactDOM$HTMLElementProps,
  autoComplete?: ?string,
  cols?: ?number,
  dirName?: ?string,
  disabled?: ?boolean,
  form?: ?string,
  maxLength?: ?number,
  minLength?: ?number,
  name?: ?string,
  onChange?: ?(event: SyntheticEvent<HTMLTextAreaElement>) => unknown,
  placeholder?: ?ReactDOM$UserVisibleString,
  readOnly?: ?boolean,
  required?: ?boolean,
  rows?: ?number,
  value?: ?(string | ReadonlyArray<string> | number),
  wrap?: ?string,
};
type ReactDOM$textareaInstance = HTMLTextAreaElement;

type ReactDOM$tfootProps = ReactDOM$HTMLElementProps;
type ReactDOM$tfootInstance = HTMLTableSectionElement;

type ReactDOM$thProps = {
  ...ReactDOM$HTMLElementProps,
  abbr?: ?string,
  align?: ?('left' | 'center' | 'right' | 'justify' | 'char'),
  colSpan?: ?number,
  headers?: ?string,
  rowSpan?: ?number,
  scope?: ?string,
  width?: ?(number | string),
};
type ReactDOM$thInstance = HTMLTableCellElement;

type ReactDOM$theadProps = ReactDOM$HTMLElementProps;
type ReactDOM$theadInstance = HTMLTableSectionElement;

type ReactDOM$timeProps = {
  ...ReactDOM$HTMLElementProps,
  dateTime?: ?string,
};
type ReactDOM$timeInstance = HTMLTimeElement;

type ReactDOM$titleProps = ReactDOM$HTMLElementProps;
type ReactDOM$titleInstance = HTMLTitleElement;

type ReactDOM$trProps = ReactDOM$HTMLElementProps;
type ReactDOM$trInstance = HTMLTableRowElement;

type ReactDOM$trackProps = {
  ...ReactDOM$SelfClosingHTMLElementProps,
  default?: ?boolean,
  kind?: ?string,
  label?: ?ReactDOM$UserVisibleString,
  src?: ?string,
  srcLang?: ?string,
};
type ReactDOM$trackInstance = HTMLTrackElement;

type ReactDOM$uProps = ReactDOM$HTMLElementProps;
type ReactDOM$uInstance = HTMLElement;

type ReactDOM$ulProps = ReactDOM$HTMLElementProps;
type ReactDOM$ulInstance = HTMLUListElement;

type ReactDOM$varProps = ReactDOM$HTMLElementProps;
type ReactDOM$varInstance = HTMLElement;

type ReactDOM$videoProps = {
  ...ReactDOM$HTMLElementProps,
  autoPlay?: ?boolean,
  controls?: ?boolean,
  controlsList?: ?string,
  crossOrigin?: ?ReactDOM$CrossOrigin,
  disablePictureInPicture?: ?boolean,
  disableRemotePlayback?: ?boolean,
  height?: ?(number | string),
  loop?: ?boolean,
  muted?: ?boolean,
  playsInline?: ?boolean,
  poster?: ?string,
  preload?: ?string,
  src?: ?string,
  width?: ?(number | string),
};
type ReactDOM$videoInstance = HTMLVideoElement;

type ReactDOM$wbrProps = ReactDOM$SelfClosingHTMLElementProps;
type ReactDOM$wbrInstance = HTMLElement;

type ReactDOM$SVGElementProps = {
  ...ReactDOM$ReactSpecificProps,
  ...ReactDOM$AriaAttributes,

  // Attributes which also defined in HTMLAttributes
  className?: ?string,
  color?: ?string,
  height?: ?(number | string),
  id?: ?string,
  lang?: ?string,
  max?: ?(number | string),
  media?: ?string,
  method?: ?string,
  min?: ?(number | string),
  name?: ?string,
  style?: ?any,
  target?: ?string,
  type?: ?string,
  width?: ?(number | string),

  // Other HTML properties supported by SVG elements in browsers
  role?: ?string,
  tabIndex?: ?number,
  crossOrigin?: ?ReactDOM$CrossOrigin,

  // SVG Specific attributes
  accentHeight?: ?(number | string),
  accumulate?: ?('none' | 'sum'),
  additive?: ?('replace' | 'sum'),
  alignmentBaseline?: ?(
    | 'auto'
    | 'baseline'
    | 'before-edge'
    | 'text-before-edge'
    | 'middle'
    | 'central'
    | 'after-edge'
    | 'text-after-edge'
    | 'ideographic'
    | 'alphabetic'
    | 'hanging'
    | 'mathematical'
    | 'inherit'
  ),
  allowReorder?: ?('no' | 'yes'),
  alphabetic?: ?(number | string),
  amplitude?: ?(number | string),
  arabicForm?: ?('initial' | 'medial' | 'terminal' | 'isolated'),
  ascent?: ?(number | string),
  attributeName?: ?string,
  attributeType?: ?string,
  autoReverse?: ?ReactDOM$BooleanishString,
  azimuth?: ?(number | string),
  baseFrequency?: ?(number | string),
  baselineShift?: ?(number | string),
  baseProfile?: ?(number | string),
  bbox?: ?(number | string),
  begin?: ?(number | string),
  bias?: ?(number | string),
  by?: ?(number | string),
  calcMode?: ?(number | string),
  capHeight?: ?(number | string),
  clip?: ?(number | string),
  clipPath?: ?string,
  clipPathUnits?: ?(number | string),
  clipRule?: ?(number | string),
  colorInterpolation?: ?(number | string),
  colorInterpolationFilters?: ?('auto' | 'sRGB' | 'linearRGB' | 'inherit'),
  colorProfile?: ?(number | string),
  colorRendering?: ?(number | string),
  contentScriptType?: ?(number | string),
  contentStyleType?: ?(number | string),
  cursor?: ?(number | string),
  cx?: ?(number | string),
  cy?: ?(number | string),
  d?: ?string,
  decelerate?: ?(number | string),
  descent?: ?(number | string),
  diffuseConstant?: ?(number | string),
  direction?: ?(number | string),
  display?: ?(number | string),
  divisor?: ?(number | string),
  dominantBaseline?: ?(number | string),
  dur?: ?(number | string),
  dx?: ?(number | string),
  dy?: ?(number | string),
  edgeMode?: ?(number | string),
  elevation?: ?(number | string),
  enableBackground?: ?(number | string),
  end?: ?(number | string),
  exponent?: ?(number | string),
  externalResourcesRequired?: ?ReactDOM$BooleanishString,
  fill?: ?string,
  fillOpacity?: ?(number | string),
  fillRule?: ?('nonzero' | 'evenodd' | 'inherit'),
  filter?: ?string,
  filterRes?: ?(number | string),
  filterUnits?: ?(number | string),
  floodColor?: ?(number | string),
  floodOpacity?: ?(number | string),
  focusable?: ?(ReactDOM$BooleanishString | 'auto'),
  fontFamily?: ?string,
  fontSize?: ?(number | string),
  fontSizeAdjust?: ?(number | string),
  fontStretch?: ?(number | string),
  fontStyle?: ?(number | string),
  fontVariant?: ?(number | string),
  fontWeight?: ?(number | string),
  format?: ?(number | string),
  fr?: ?(number | string),
  from?: ?(number | string),
  fx?: ?(number | string),
  fy?: ?(number | string),
  g1?: ?(number | string),
  g2?: ?(number | string),
  glyphName?: ?(number | string),
  glyphOrientationHorizontal?: ?(number | string),
  glyphOrientationVertical?: ?(number | string),
  glyphRef?: ?(number | string),
  gradientTransform?: ?string,
  gradientUnits?: ?string,
  hanging?: ?(number | string),
  horizAdvX?: ?(number | string),
  horizOriginX?: ?(number | string),
  href?: ?string,
  ideographic?: ?(number | string),
  imageRendering?: ?(number | string),
  in?: ?string,
  in2?: ?(number | string),
  intercept?: ?(number | string),
  k?: ?(number | string),
  k1?: ?(number | string),
  k2?: ?(number | string),
  k3?: ?(number | string),
  k4?: ?(number | string),
  kernelMatrix?: ?(number | string),
  kernelUnitLength?: ?(number | string),
  kerning?: ?(number | string),
  keyPoints?: ?(number | string),
  keySplines?: ?(number | string),
  keyTimes?: ?(number | string),
  lengthAdjust?: ?(number | string),
  letterSpacing?: ?(number | string),
  lightingColor?: ?(number | string),
  limitingConeAngle?: ?(number | string),
  local?: ?(number | string),
  markerEnd?: ?string,
  markerHeight?: ?(number | string),
  markerMid?: ?string,
  markerStart?: ?string,
  markerUnits?: ?(number | string),
  markerWidth?: ?(number | string),
  mask?: ?string,
  maskContentUnits?: ?(number | string),
  maskUnits?: ?(number | string),
  mathematical?: ?(number | string),
  mode?: ?(number | string),
  numOctaves?: ?(number | string),
  offset?: ?(number | string),
  opacity?: ?(number | string),
  operator?: ?(number | string),
  order?: ?(number | string),
  orient?: ?(number | string),
  orientation?: ?(number | string),
  origin?: ?(number | string),
  overflow?: ?(number | string),
  overlinePosition?: ?(number | string),
  overlineThickness?: ?(number | string),
  paintOrder?: ?(number | string),
  panose1?: ?(number | string),
  path?: ?string,
  pathLength?: ?(number | string),
  patternContentUnits?: ?string,
  patternTransform?: ?(number | string),
  patternUnits?: ?string,
  pointerEvents?: ?(number | string),
  points?: ?string,
  pointsAtX?: ?(number | string),
  pointsAtY?: ?(number | string),
  pointsAtZ?: ?(number | string),
  preserveAlpha?: ?ReactDOM$BooleanishString,
  preserveAspectRatio?: ?string,
  primitiveUnits?: ?(number | string),
  r?: ?(number | string),
  radius?: ?(number | string),
  refX?: ?(number | string),
  refY?: ?(number | string),
  renderingIntent?: ?(number | string),
  repeatCount?: ?(number | string),
  repeatDur?: ?(number | string),
  requiredExtensions?: ?(number | string),
  requiredFeatures?: ?(number | string),
  restart?: ?(number | string),
  result?: ?string,
  rotate?: ?(number | string),
  rx?: ?(number | string),
  ry?: ?(number | string),
  scale?: ?(number | string),
  seed?: ?(number | string),
  shapeRendering?: ?(number | string),
  slope?: ?(number | string),
  spacing?: ?(number | string),
  specularConstant?: ?(number | string),
  specularExponent?: ?(number | string),
  speed?: ?(number | string),
  spreadMethod?: ?string,
  startOffset?: ?(number | string),
  stdDeviation?: ?(number | string),
  stemh?: ?(number | string),
  stemv?: ?(number | string),
  stitchTiles?: ?(number | string),
  stopColor?: ?string,
  stopOpacity?: ?(number | string),
  strikethroughPosition?: ?(number | string),
  strikethroughThickness?: ?(number | string),
  string?: ?(number | string),
  stroke?: ?string,
  strokeDasharray?: ?(string | number),
  strokeDashoffset?: ?(string | number),
  strokeLinecap?: ?('butt' | 'round' | 'square' | 'inherit'),
  strokeLinejoin?: ?('miter' | 'round' | 'bevel' | 'inherit'),
  strokeMiterlimit?: ?(number | string),
  strokeOpacity?: ?(number | string),
  strokeWidth?: ?(number | string),
  surfaceScale?: ?(number | string),
  systemLanguage?: ?(number | string),
  tableValues?: ?(number | string),
  targetX?: ?(number | string),
  targetY?: ?(number | string),
  textAnchor?: ?string,
  textDecoration?: ?(number | string),
  textLength?: ?(number | string),
  textRendering?: ?(number | string),
  to?: ?(number | string),
  transform?: ?string,
  u1?: ?(number | string),
  u2?: ?(number | string),
  underlinePosition?: ?(number | string),
  underlineThickness?: ?(number | string),
  unicode?: ?(number | string),
  unicodeBidi?: ?(number | string),
  unicodeRange?: ?(number | string),
  unitsPerEm?: ?(number | string),
  vAlphabetic?: ?(number | string),
  values?: ?string,
  vectorEffect?: ?(number | string),
  version?: ?string,
  vertAdvY?: ?(number | string),
  vertOriginX?: ?(number | string),
  vertOriginY?: ?(number | string),
  vHanging?: ?(number | string),
  vIdeographic?: ?(number | string),
  viewBox?: ?string,
  viewTarget?: ?(number | string),
  visibility?: ?(number | string),
  vMathematical?: ?(number | string),
  widths?: ?(number | string),
  wordSpacing?: ?(number | string),
  writingMode?: ?(number | string),
  x?: ?(number | string),
  x1?: ?(number | string),
  x2?: ?(number | string),
  xChannelSelector?: ?string,
  xHeight?: ?(number | string),
  xlinkActuate?: ?string,
  xlinkArcrole?: ?string,
  xlinkHref?: ?string,
  xlinkRole?: ?string,
  xlinkShow?: ?string,
  xlinkTitle?: ?string,
  xlinkType?: ?string,
  xmlBase?: ?string,
  xmlLang?: ?string,
  xmlns?: ?string,
  xmlnsXlink?: ?string,
  xmlSpace?: ?string,
  y?: ?(number | string),
  y1?: ?(number | string),
  y2?: ?(number | string),
  yChannelSelector?: ?string,
  z?: ?(number | string),
  zoomAndPan?: ?string,
  ...ReactDOM$CustomSVGProps,
};

// SVG Elements

type ReactDOM$CustomSVGProps = {
  line_height?: ?(number | string),
};

type ReactDOM$svgProps = ReactDOM$SVGElementProps;
type ReactDOM$svgInstance = Element;

type ReactDOM$animateProps = ReactDOM$SVGElementProps;
type ReactDOM$animateInstance = Element;

type ReactDOM$animateMotionProps = ReactDOM$SVGElementProps;
type ReactDOM$animateMotionInstance = Element;

type ReactDOM$animateTransformProps = ReactDOM$SVGElementProps;
type ReactDOM$animateTransformInstance = Element;

type ReactDOM$circleProps = ReactDOM$SVGElementProps;
type ReactDOM$circleInstance = Element;

type ReactDOM$clipPathProps = ReactDOM$SVGElementProps;
type ReactDOM$clipPathInstance = Element;

type ReactDOM$defsProps = ReactDOM$SVGElementProps;
type ReactDOM$defsInstance = Element;

type ReactDOM$descProps = ReactDOM$SVGElementProps;
type ReactDOM$descInstance = Element;

type ReactDOM$ellipseProps = ReactDOM$SVGElementProps;
type ReactDOM$ellipseInstance = Element;

type ReactDOM$feBlendProps = ReactDOM$SVGElementProps;
type ReactDOM$feBlendInstance = Element;

type ReactDOM$feColorMatrixProps = ReactDOM$SVGElementProps;
type ReactDOM$feColorMatrixInstance = Element;

type ReactDOM$feComponentTransferProps = ReactDOM$SVGElementProps;
type ReactDOM$feComponentTransferInstance = Element;

type ReactDOM$feCompositeProps = ReactDOM$SVGElementProps;
type ReactDOM$feCompositeInstance = Element;

type ReactDOM$feConvolveMatrixProps = ReactDOM$SVGElementProps;
type ReactDOM$feConvolveMatrixInstance = Element;

type ReactDOM$feDiffuseLightingProps = ReactDOM$SVGElementProps;
type ReactDOM$feDiffuseLightingInstance = Element;

type ReactDOM$feDisplacementMapProps = ReactDOM$SVGElementProps;
type ReactDOM$feDisplacementMapInstance = Element;

type ReactDOM$feDistantLightProps = ReactDOM$SVGElementProps;
type ReactDOM$feDistantLightInstance = Element;

type ReactDOM$feDropShadowProps = ReactDOM$SVGElementProps;
type ReactDOM$feDropShadowInstance = Element;

type ReactDOM$feFloodProps = ReactDOM$SVGElementProps;
type ReactDOM$feFloodInstance = Element;

type ReactDOM$feFuncAProps = ReactDOM$SVGElementProps;
type ReactDOM$feFuncAInstance = Element;

type ReactDOM$feFuncBProps = ReactDOM$SVGElementProps;
type ReactDOM$feFuncBInstance = Element;

type ReactDOM$feFuncGProps = ReactDOM$SVGElementProps;
type ReactDOM$feFuncGInstance = Element;

type ReactDOM$feFuncRProps = ReactDOM$SVGElementProps;
type ReactDOM$feFuncRInstance = Element;

type ReactDOM$feGaussianBlurProps = ReactDOM$SVGElementProps;
type ReactDOM$feGaussianBlurInstance = Element;

type ReactDOM$feImageProps = ReactDOM$SVGElementProps;
type ReactDOM$feImageInstance = Element;

type ReactDOM$feMergeProps = ReactDOM$SVGElementProps;
type ReactDOM$feMergeInstance = Element;

type ReactDOM$feMergeNodeProps = ReactDOM$SVGElementProps;
type ReactDOM$feMergeNodeInstance = Element;

type ReactDOM$feMorphologyProps = ReactDOM$SVGElementProps;
type ReactDOM$feMorphologyInstance = Element;

type ReactDOM$feOffsetProps = ReactDOM$SVGElementProps;
type ReactDOM$feOffsetInstance = Element;

type ReactDOM$fePointLightProps = ReactDOM$SVGElementProps;
type ReactDOM$fePointLightInstance = Element;

type ReactDOM$feSpecularLightingProps = ReactDOM$SVGElementProps;
type ReactDOM$feSpecularLightingInstance = Element;

type ReactDOM$feSpotLightProps = ReactDOM$SVGElementProps;
type ReactDOM$feSpotLightInstance = Element;

type ReactDOM$feTileProps = ReactDOM$SVGElementProps;
type ReactDOM$feTileInstance = Element;

type ReactDOM$feTurbulenceProps = ReactDOM$SVGElementProps;
type ReactDOM$feTurbulenceInstance = Element;

type ReactDOM$filterProps = ReactDOM$SVGElementProps;
type ReactDOM$filterInstance = Element;

type ReactDOM$foreignObjectProps = ReactDOM$SVGElementProps;
type ReactDOM$foreignObjectInstance = Element;

type ReactDOM$gProps = ReactDOM$SVGElementProps;
type ReactDOM$gInstance = Element;

type ReactDOM$imageProps = ReactDOM$SVGElementProps;
type ReactDOM$imageInstance = Element;

type ReactDOM$lineProps = ReactDOM$SVGElementProps;
type ReactDOM$lineInstance = Element;

type ReactDOM$linearGradientProps = ReactDOM$SVGElementProps;
type ReactDOM$linearGradientInstance = Element;

type ReactDOM$markerProps = ReactDOM$SVGElementProps;
type ReactDOM$markerInstance = Element;

type ReactDOM$maskProps = ReactDOM$SVGElementProps;
type ReactDOM$maskInstance = Element;

type ReactDOM$metadataProps = ReactDOM$SVGElementProps;
type ReactDOM$metadataInstance = Element;

type ReactDOM$mpathProps = ReactDOM$SVGElementProps;
type ReactDOM$mpathInstance = Element;

type ReactDOM$pathProps = ReactDOM$SVGElementProps;
type ReactDOM$pathInstance = Element;

type ReactDOM$patternProps = ReactDOM$SVGElementProps;
type ReactDOM$patternInstance = Element;

type ReactDOM$polygonProps = ReactDOM$SVGElementProps;
type ReactDOM$polygonInstance = Element;

type ReactDOM$polylineProps = ReactDOM$SVGElementProps;
type ReactDOM$polylineInstance = Element;

type ReactDOM$radialGradientProps = ReactDOM$SVGElementProps;
type ReactDOM$radialGradientInstance = Element;

type ReactDOM$rectProps = ReactDOM$SVGElementProps;
type ReactDOM$rectInstance = Element;

type ReactDOM$setProps = ReactDOM$SVGElementProps;
type ReactDOM$setInstance = Element;

type ReactDOM$stopProps = ReactDOM$SVGElementProps;
type ReactDOM$stopInstance = Element;

type ReactDOM$switchProps = ReactDOM$SVGElementProps;
type ReactDOM$switchInstance = Element;

type ReactDOM$symbolProps = ReactDOM$SVGElementProps;
type ReactDOM$symbolInstance = Element;

type ReactDOM$textProps = ReactDOM$SVGElementProps;
type ReactDOM$textInstance = Element;

type ReactDOM$textPathProps = ReactDOM$SVGElementProps;
type ReactDOM$textPathInstance = Element;

type ReactDOM$tspanProps = ReactDOM$SVGElementProps;
type ReactDOM$tspanInstance = Element;

type ReactDOM$useProps = ReactDOM$SVGElementProps;
type ReactDOM$useInstance = Element;

type ReactDOM$viewProps = ReactDOM$SVGElementProps;
type ReactDOM$viewInstance = Element;
