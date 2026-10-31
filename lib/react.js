/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 */

/**
 * A UI node that can be rendered by React. React can render most primitives in
 * addition to elements and arrays of nodes.
 */

declare type React$Node =
  | void
  | null
  | boolean
  | number
  | string
  | ExactReactElement_DEPRECATED<any>
  | React$Portal
  | Iterable<?React$Node>;

declare type React$AbstractComponentStatics = {
  displayName?: ?string,
  // This is only on function components, but trying to access name when
  // displayName is undefined is a common pattern.
  name?: ?string,
  propTypes?: {[key: string]: any, ...},
  ...
};

/**
 * The type of an element in React. A React element may be a:
 *
 * - String. These elements are intrinsics that depend on the React renderer
 *   implementation.
 * - React component. See `ComponentType` for more information about its
 *   different variants.
 */
declare type React$ElementType =
  | string
  | component(...empty);

/**
 * Type of a React element. React elements are commonly created using JSX
 * literals, which desugar to React.createElement calls (see below).
 *
 * The internal structure of React.Element is intentionally omitted to
 * discourage React.Element inspection.
 *
 * See https://react.dev/reference/react/Children#alternatives for alternatives.
 *
 * @deprecated You should not require very specific React elements.
 * Use React.Node, React.MixedElement, or [render types](https://flow.org/en/docs/react/render-types/) instead.
 */
declare opaque type ExactReactElement_DEPRECATED<out ElementType extends React$ElementType, out P = React.ComponentProps<ElementType>>: {...};

/**
 * A type that should be used as the arguments of a render type.
 * Referencing nominal components in type positions will produce this type.
 *
 * e.g.
 * ```flow
 * declare component Foo();
 *
 * type T1 = renders Foo;
 * // Desugars to
 * type T1 = renders React$RendersExactly<typeof Foo>;
 * ```
 */
declare opaque type React$RendersExactly<out ElementType extends React$ElementType>: React$Node;

/**
 * The type of the key that React uses to determine where items in a new list
 * have moved.
 */
declare type React$Key = string | number;

declare opaque type React$RefObject<T>: { current: T }

/**
 * A React portal node. The implementation of the portal node is hidden to React
 * users so we use an opaque type.
 */
declare opaque type React$Portal;

declare namespace React {
  type ComponentType<in P extends {...}> = component(...P);
  type PropsOf<E extends string | MixedElement | React$RendersExactly<React$ElementType>> = E extends ExactReactElement_DEPRECATED<infer C> ? ComponentProps<C> : E extends React$RendersExactly<infer C> ? ComponentProps<C> : (E extends string ? ComponentProps<E> : empty);
  type PropOf<E extends string | MixedElement | React$RendersExactly<React$ElementType>, K extends string> = PropsOf<E>[K]
  type RefOf<E extends string | MixedElement | React$RendersExactly<React$ElementType>> = E extends ExactReactElement_DEPRECATED<infer C> ? ElementRef<C> : E extends React$RendersExactly<infer C> ? ElementRef<C> : (E extends string ? $JSXIntrinsics[E]['instance'] : empty);
  /**
   * @flowCustomError
   */
  type MixedElement = ExactReactElement_DEPRECATED<React$ElementType, unknown>;
  type ElementType = React$ElementType;

  type Key = React$Key;

  /**
   * The type of the `ref` prop on forwardRef components and the ref argument
   * for useImperativeHandle.
   */
  type RefSetter<in T> =
    | { writeonly current: T | null, ... }
    | ((T | null) => void | (() => void))
    | null
    | void;
  type Node = React$Node;
  /**
   * The type of a React Context.  React Contexts are created by calling
   * createContext() with a default value.
   */
  declare opaque type Context<T>:
  component<Renders extends React$Node = React$Node>(value: T, children?: Renders, ...{...}) renders Renders
  & {
    Provider: component<Renders extends React$Node = React$Node>(value: T, children?: Renders, ...{...}) renders Renders,
    Consumer: React.ComponentType<{ readonly children: (value: T) => ?React$Node, ... }>,
    // Optional, user-specified value for custom display label in React DevTools.
    displayName?: string,
    ...
  }
  type Portal = React$Portal;

  type ElementRef<C extends string | component(...empty)> =
    C extends empty ? any :
    C extends (infer S extends string) ? $JSXIntrinsics[S]['instance'] :
    C extends component(...infer Props extends {...})
      ? ('ref' extends keyof Props
        ? (Props extends {readonly ref?: RefSetter<infer I>, ...} ? I : unknown)
        : void
      )
      : empty;

  type InexactPartial<T> = {[K in keyof T]?: T[K] | void};

  /**
   * Any prop that has a default prop becomes optional, but its type is unchanged.
   * Undeclared default props are augmented into the resulting allowable attributes.
   * If declared props have indexed properties, ignore default props entirely as keyof gets widened.
   * The result is exact if and only if the declared props are exact.
   * Wrap in an outer-level conditional type to allow distribution over props that are unions.
   */
  type Defaultize<P, D> = P extends any
    ? string extends keyof P
      ? P
      : P extends ExactObjectType
        ? {
            ...Pick<P, Exclude<keyof P, keyof D>>,
            ...InexactPartial<Pick<P, Extract<keyof P, keyof D>>>,
            ...InexactPartial<Pick<D, Exclude<keyof D, keyof P>>>,
          }
        : {
            ...Pick<P, Exclude<keyof P, keyof D>>,
            ...InexactPartial<Pick<P, Extract<keyof P, keyof D>>>,
            ...InexactPartial<Pick<D, Exclude<keyof D, keyof P>>>,
            ...
          }
    : empty;

  type ReactManagedAttributes<C, P> = Readonly<C extends {defaultProps: infer D, ...} ? Defaultize<P, D> : P>;

  type ChildrenArray<out T> = ReadonlyArray<ChildrenArray<T>> | T;
  type ReactSetStateFunction<S> = ((S => S) | S) => void;
  type RefObject<T> = React$RefObject<T>;
  type Interaction = {
    name: string,
    timestamp: number,
    ...
  };

  declare namespace JSX {
    type Element = ReactElement<any, any>;
  }

  // TS compat: core types
  /**
   * @deprecated TypeScript compatibility alias. Use `React.Node` instead.
   */
  type ReactNode = React$Node;
  /**
   * Represents a JSX element.
   */
  type ReactElement<out P = unknown, out T = React$ElementType> = ExactReactElement_DEPRECATED<T, P>;
  /**
   * @deprecated TypeScript compatibility alias. Use `React.Portal` instead.
   */
  type ReactPortal = React$Portal;
  /**
   * @deprecated TypeScript compatibility alias. Use `string | number` instead.
   */
  type ReactText = string | number;
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  type JSXElementConstructor<in P extends {...} = {...}> = component(...P);

  // TS compat: component types
  /**
   * @deprecated TypeScript compatibility alias.
   */
  type ReactInstance = any;
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  type FC<in P extends {...} = {...}> = component(...P);
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  type FunctionComponent<in P extends {...} = {...}> = component(...P);
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  type ComponentClass<in P extends {...} = {...}, S = any> = component(...P);
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  type ForwardRefExoticComponent<in P extends {...} = {...}> = component(...P);
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  type NamedExoticComponent<in P extends {...} = {...}> = component(...P);
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  type ExoticComponent<in P extends {...} = {...}> = component(...P);
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  type MemoExoticComponent<T extends ComponentType<any>> = component(...ComponentProps<T>);
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  type LazyExoticComponent<T extends ComponentType<any>> = component(...ComponentProps<T>);
  /**
   * The props accepted by a component or JSX intrinsic, including its ref prop.
   */
  type ComponentProps<T extends React$ElementType> = T extends component(...infer Props extends {...}) ? Props : T extends keyof $JSXIntrinsics ? {...$JSXIntrinsics[T]['props'], ref?: RefSetter<$JSXIntrinsics[T]['instance']>} : empty;
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentProps` instead.
   */
  type ComponentPropsWithRef<T extends React$ElementType> = ComponentProps<T>;
  /**
   * @deprecated TypeScript compatibility alias. Use `Omit<React.ComponentProps<T>, 'ref'>` instead.
   */
  type ComponentPropsWithoutRef<T extends React$ElementType> = Omit<ComponentProps<T>, 'ref'>;
  /**
   * @deprecated TypeScript compatibility alias.
   */
  type PropsWithChildren<out P = {...}> = P & {children?: React$Node, ...};
  /**
   * @deprecated TypeScript compatibility alias.
   */
  type PropsWithoutRef<P> = Omit<P, 'ref'>;

  // TS compat: ref & attribute types
  /**
   * @deprecated TypeScript compatibility alias. Use `React.RefSetter` instead.
   */
  type Ref<in T> = RefSetter<T>;
  /**
   * @deprecated TypeScript compatibility alias. Use `React.RefSetter` instead.
   */
  type RefCallback<in T> = (T | null) => void | (() => void);
  /**
   * @deprecated TypeScript compatibility alias.
   */
  type RefAttributes<T> = { ref?: RefSetter<T>, ...};
  /**
   * @deprecated TypeScript compatibility alias.
   */
  type ClassAttributes<T> = RefAttributes<T>;
  /**
   * @deprecated TypeScript compatibility alias.
   */
  type Attributes = { key?: React$Key, ...};
  /**
   * @deprecated TypeScript compatibility alias. Use `React.RefObject` instead.
   */
  type MutableRefObject<T> = { current: T, ... };
  /**
   * @deprecated TypeScript compatibility alias.
   */
  type ForwardRefRenderFunction<out T, in P = {...}> = (props: P, ref: RefSetter<T>) => React$Node;

  // TS compat: context types
  /**
   * @deprecated TypeScript compatibility alias. Prefer `React.Context<T>` for the context
   * object; this type represents only the provider component.
   */
  type Provider<in T> = component(value: T, children?: React$Node);
  /**
   * @deprecated TypeScript compatibility alias. Prefer `React.Context<T>` for the context
   * object; this type represents only the consumer component.
   */
  type Consumer<out T> = component(children: (value: T) => React$Node);
  /**
   * @deprecated TypeScript compatibility alias.
   */
  type ProviderProps<T> = {value: T, children?: React$Node, ...};

  // TS compat: hook/misc types
  /**
   * @deprecated TypeScript compatibility alias.
   */
  type Dispatch<in A> = (A) => void;
  /**
   * @deprecated TypeScript compatibility alias.
   */
  type SetStateAction<S> = S | ((S) => S);
  /**
   * @deprecated TypeScript compatibility alias.
   */
  type DependencyList = ReadonlyArray<unknown>;
  /**
   * @deprecated TypeScript compatibility alias.
   */
  type EffectCallback = () => void | (() => void);
  /**
   * @deprecated TypeScript compatibility alias.
   */
  type ErrorInfo = {componentStack?: string | null, ...};
  /**
   * @deprecated TypeScript compatibility alias.
   */
  type ValidationMap<T> = {[key: string]: any, ...};
  /**
   * @deprecated TypeScript compatibility alias.
   */
  type WeakValidationMap<T> = {[key: string]: any, ...};
  /**
   * @deprecated TypeScript compatibility alias.
   */
  type TransitionStartFunction = (callback: () => void | Promise<void>) => void;
  /**
   * @deprecated TypeScript compatibility alias.
   */
  type ActionDispatch<in ActionArg extends ReadonlyArray<any> = []> = (...args: ActionArg) => void;
}

/**
 * Intentionally fully opaque to ban direct calls to `React.createElement`.
 * Use JSX instead.
 * @see https://react.dev/blog/2024/04/25/react-19-upgrade-guide#new-jsx-transform-is-now-required
 */
declare opaque type React$CreateElement;

type React$CloneElement =
  & (<C extends keyof $JSXIntrinsics, E extends ExactReactElement_DEPRECATED<C>>(
    element: E,
    props?: ?Partial<$JSXIntrinsics[C]['props']>,
    ...children: ReadonlyArray<React$Node>
  ) => E)
  & (<Props, E extends ExactReactElement_DEPRECATED<component(...empty) | string, Props>>(
    element: E,
    props?: ?Partial<Readonly<{...$Exact<NoInfer<Props>>, key: React$Key}>>,
  ) => E)
  & (<Props, E extends ExactReactElement_DEPRECATED<component(...empty) | string, Props>>(
    element: E,
    props: ?Partial<Readonly<{...$Exact<NoInfer<Props>>, key: React$Key}>>,
    children: NoInfer<Props>['children']
  ) => E)
  & (<Props, E extends ExactReactElement_DEPRECATED<component(...empty) | string, Props>>(
    element: E,
    props: ?Partial<Readonly<{...$Exact<NoInfer<Props>>, key: React$Key}>>,
    firstChild: NoInfer<Props>['children'][0],
    secondChild: NoInfer<Props>['children'][1],
    ...restChildren: NoInfer<Props>['children'] extends [readonly first: unknown, readonly second: unknown, ...infer Rest] ? Rest : NoInfer<Props>['children']
  ) => E);

declare module "react" {
  declare export type BaseSyntheticEvent<E = object, C = any, T = any> = globalThis.React.BaseSyntheticEvent<E, C, T>;
  declare export type SyntheticEvent<T = Element, E = Event> = globalThis.React.SyntheticEvent<T, E>;
  declare export type ClipboardEvent<T = Element> = globalThis.React.ClipboardEvent<T>;
  declare export type CompositionEvent<T = Element> = globalThis.React.CompositionEvent<T>;
  declare export type DragEvent<T = Element> = globalThis.React.DragEvent<T>;
  declare export type PointerEvent<T = Element> = globalThis.React.PointerEvent<T>;
  declare export type FocusEvent<Target = Element, RelatedTarget = Element> = globalThis.React.FocusEvent<Target, RelatedTarget>;
  declare export type FormEvent<T = Element> = globalThis.React.FormEvent<T>;
  declare export type InvalidEvent<T = Element> = globalThis.React.InvalidEvent<T>;
  declare export type ChangeEvent<CurrentTarget = Element, Target = Element> = globalThis.React.ChangeEvent<CurrentTarget, Target>;
  declare export type InputEvent<T = Element> = globalThis.React.InputEvent<T>;
  declare export type ModifierKey = globalThis.React.ModifierKey;
  declare export type KeyboardEvent<T = Element> = globalThis.React.KeyboardEvent<T>;
  declare export type MouseEvent<T = Element, E = globalThis.MouseEvent> = globalThis.React.MouseEvent<T, E>;
  declare export type SubmitEvent<T = Element> = globalThis.React.SubmitEvent<T>;
  declare export type TouchEvent<T = Element> = globalThis.React.TouchEvent<T>;
  declare export type UIEvent<T = Element, E = globalThis.UIEvent> = globalThis.React.UIEvent<T, E>;
  declare export type WheelEvent<T = Element> = globalThis.React.WheelEvent<T>;
  declare export type AnimationEvent<T = Element> = globalThis.React.AnimationEvent<T>;
  declare export type ToggleEvent<T = Element> = globalThis.React.ToggleEvent<T>;
  declare export type TransitionEvent<T = Element> = globalThis.React.TransitionEvent<T>;
  declare export type EventHandler<E> = globalThis.React.EventHandler<E>;
  declare export type ReactEventHandler<T = Element> = globalThis.React.ReactEventHandler<T>;
  declare export type ClipboardEventHandler<T = Element> = globalThis.React.ClipboardEventHandler<T>;
  declare export type CompositionEventHandler<T = Element> = globalThis.React.CompositionEventHandler<T>;
  declare export type DragEventHandler<T = Element> = globalThis.React.DragEventHandler<T>;
  declare export type FocusEventHandler<T = Element> = globalThis.React.FocusEventHandler<T>;
  declare export type FormEventHandler<T = Element> = globalThis.React.FormEventHandler<T>;
  declare export type ChangeEventHandler<CurrentTarget = Element, Target = Element> = globalThis.React.ChangeEventHandler<CurrentTarget, Target>;
  declare export type InputEventHandler<T = Element> = globalThis.React.InputEventHandler<T>;
  declare export type KeyboardEventHandler<T = Element> = globalThis.React.KeyboardEventHandler<T>;
  declare export type MouseEventHandler<T = Element> = globalThis.React.MouseEventHandler<T>;
  declare export type SubmitEventHandler<T = Element> = globalThis.React.SubmitEventHandler<T>;
  declare export type TouchEventHandler<T = Element> = globalThis.React.TouchEventHandler<T>;
  declare export type PointerEventHandler<T = Element> = globalThis.React.PointerEventHandler<T>;
  declare export type UIEventHandler<T = Element> = globalThis.React.UIEventHandler<T>;
  declare export type WheelEventHandler<T = Element> = globalThis.React.WheelEventHandler<T>;
  declare export type AnimationEventHandler<T = Element> = globalThis.React.AnimationEventHandler<T>;
  declare export type ToggleEventHandler<T = Element> = globalThis.React.ToggleEventHandler<T>;
  declare export type TransitionEventHandler<T = Element> = globalThis.React.TransitionEventHandler<T>;
  declare export type HTMLProps<T> = globalThis.React.HTMLProps<T>;
  declare export type DetailedHTMLProps<E, T> = globalThis.React.DetailedHTMLProps<E, T>;
  declare export type SVGProps<T> = globalThis.React.SVGProps<T>;
  declare export type SVGLineElementAttributes<T> = globalThis.React.SVGLineElementAttributes<T>;
  declare export type SVGTextElementAttributes<T> = globalThis.React.SVGTextElementAttributes<T>;
  declare export type DOMAttributes<T> = globalThis.React.DOMAttributes<T>;
  declare export type CSSProperties = globalThis.React.CSSProperties;
  declare export type AriaAttributes = globalThis.React.AriaAttributes;
  declare export type AriaRole = globalThis.React.AriaRole;
  declare export type HTMLAttributes<T> = globalThis.React.HTMLAttributes<T>;
  declare export type DO_NOT_USE_OR_YOU_WILL_BE_FIRED_EXPERIMENTAL_FORM_ACTIONS = globalThis.React.DO_NOT_USE_OR_YOU_WILL_BE_FIRED_EXPERIMENTAL_FORM_ACTIONS;
  declare export type AllHTMLAttributes<T> = globalThis.React.AllHTMLAttributes<T>;
  declare export type HTMLAttributeReferrerPolicy = globalThis.React.HTMLAttributeReferrerPolicy;
  declare export type HTMLAttributeAnchorTarget = globalThis.React.HTMLAttributeAnchorTarget;
  declare export type AnchorHTMLAttributes<T> = globalThis.React.AnchorHTMLAttributes<T>;
  declare export type AudioHTMLAttributes<T> = globalThis.React.AudioHTMLAttributes<T>;
  declare export type AreaHTMLAttributes<T> = globalThis.React.AreaHTMLAttributes<T>;
  declare export type BaseHTMLAttributes<T> = globalThis.React.BaseHTMLAttributes<T>;
  declare export type BlockquoteHTMLAttributes<T> = globalThis.React.BlockquoteHTMLAttributes<T>;
  declare export type ButtonHTMLAttributes<T> = globalThis.React.ButtonHTMLAttributes<T>;
  declare export type CanvasHTMLAttributes<T> = globalThis.React.CanvasHTMLAttributes<T>;
  declare export type ColHTMLAttributes<T> = globalThis.React.ColHTMLAttributes<T>;
  declare export type ColgroupHTMLAttributes<T> = globalThis.React.ColgroupHTMLAttributes<T>;
  declare export type DataHTMLAttributes<T> = globalThis.React.DataHTMLAttributes<T>;
  declare export type DetailsHTMLAttributes<T> = globalThis.React.DetailsHTMLAttributes<T>;
  declare export type DelHTMLAttributes<T> = globalThis.React.DelHTMLAttributes<T>;
  declare export type DialogHTMLAttributes<T> = globalThis.React.DialogHTMLAttributes<T>;
  declare export type EmbedHTMLAttributes<T> = globalThis.React.EmbedHTMLAttributes<T>;
  declare export type FieldsetHTMLAttributes<T> = globalThis.React.FieldsetHTMLAttributes<T>;
  declare export type FormHTMLAttributes<T> = globalThis.React.FormHTMLAttributes<T>;
  declare export type HtmlHTMLAttributes<T> = globalThis.React.HtmlHTMLAttributes<T>;
  declare export type IframeHTMLAttributes<T> = globalThis.React.IframeHTMLAttributes<T>;
  declare export type DO_NOT_USE_OR_YOU_WILL_BE_FIRED_EXPERIMENTAL_IMG_SRC_TYPES = globalThis.React.DO_NOT_USE_OR_YOU_WILL_BE_FIRED_EXPERIMENTAL_IMG_SRC_TYPES;
  declare export type ImgHTMLAttributes<T> = globalThis.React.ImgHTMLAttributes<T>;
  declare export type InsHTMLAttributes<T> = globalThis.React.InsHTMLAttributes<T>;
  declare export type HTMLInputTypeAttribute = globalThis.React.HTMLInputTypeAttribute;
  declare export type AutoFillAddressKind = globalThis.React.AutoFillAddressKind;
  declare export type AutoFillBase = globalThis.React.AutoFillBase;
  declare export type AutoFillContactField = globalThis.React.AutoFillContactField;
  declare export type AutoFillContactKind = globalThis.React.AutoFillContactKind;
  declare export type AutoFillCredentialField = globalThis.React.AutoFillCredentialField;
  declare export type AutoFillNormalField = globalThis.React.AutoFillNormalField;
  declare export type OptionalPrefixToken<T extends string> = globalThis.React.OptionalPrefixToken<T>;
  declare export type OptionalPostfixToken<T extends string> = globalThis.React.OptionalPostfixToken<T>;
  declare export type AutoFillField = globalThis.React.AutoFillField;
  declare export type AutoFillSection = globalThis.React.AutoFillSection;
  declare export type AutoFill = globalThis.React.AutoFill;
  declare export type HTMLInputAutoCompleteAttribute = globalThis.React.HTMLInputAutoCompleteAttribute;
  declare export type InputHTMLAttributes<T> = globalThis.React.InputHTMLAttributes<T>;
  declare export type KeygenHTMLAttributes<T> = globalThis.React.KeygenHTMLAttributes<T>;
  declare export type LabelHTMLAttributes<T> = globalThis.React.LabelHTMLAttributes<T>;
  declare export type LiHTMLAttributes<T> = globalThis.React.LiHTMLAttributes<T>;
  declare export type LinkHTMLAttributes<T> = globalThis.React.LinkHTMLAttributes<T>;
  declare export type MapHTMLAttributes<T> = globalThis.React.MapHTMLAttributes<T>;
  declare export type MenuHTMLAttributes<T> = globalThis.React.MenuHTMLAttributes<T>;
  declare export type DO_NOT_USE_OR_YOU_WILL_BE_FIRED_EXPERIMENTAL_MEDIA_SRC_TYPES = globalThis.React.DO_NOT_USE_OR_YOU_WILL_BE_FIRED_EXPERIMENTAL_MEDIA_SRC_TYPES;
  declare export type MediaHTMLAttributes<T> = globalThis.React.MediaHTMLAttributes<T>;
  declare export type MetaHTMLAttributes<T> = globalThis.React.MetaHTMLAttributes<T>;
  declare export type MeterHTMLAttributes<T> = globalThis.React.MeterHTMLAttributes<T>;
  declare export type QuoteHTMLAttributes<T> = globalThis.React.QuoteHTMLAttributes<T>;
  declare export type ObjectHTMLAttributes<T> = globalThis.React.ObjectHTMLAttributes<T>;
  declare export type OlHTMLAttributes<T> = globalThis.React.OlHTMLAttributes<T>;
  declare export type OptgroupHTMLAttributes<T> = globalThis.React.OptgroupHTMLAttributes<T>;
  declare export type OptionHTMLAttributes<T> = globalThis.React.OptionHTMLAttributes<T>;
  declare export type OutputHTMLAttributes<T> = globalThis.React.OutputHTMLAttributes<T>;
  declare export type ParamHTMLAttributes<T> = globalThis.React.ParamHTMLAttributes<T>;
  declare export type ProgressHTMLAttributes<T> = globalThis.React.ProgressHTMLAttributes<T>;
  declare export type SlotHTMLAttributes<T> = globalThis.React.SlotHTMLAttributes<T>;
  declare export type ScriptHTMLAttributes<T> = globalThis.React.ScriptHTMLAttributes<T>;
  declare export type SelectHTMLAttributes<T> = globalThis.React.SelectHTMLAttributes<T>;
  declare export type SourceHTMLAttributes<T> = globalThis.React.SourceHTMLAttributes<T>;
  declare export type StyleHTMLAttributes<T> = globalThis.React.StyleHTMLAttributes<T>;
  declare export type TableHTMLAttributes<T> = globalThis.React.TableHTMLAttributes<T>;
  declare export type TextareaHTMLAttributes<T> = globalThis.React.TextareaHTMLAttributes<T>;
  declare export type TdHTMLAttributes<T> = globalThis.React.TdHTMLAttributes<T>;
  declare export type ThHTMLAttributes<T> = globalThis.React.ThHTMLAttributes<T>;
  declare export type TimeHTMLAttributes<T> = globalThis.React.TimeHTMLAttributes<T>;
  declare export type TrackHTMLAttributes<T> = globalThis.React.TrackHTMLAttributes<T>;
  declare export type VideoHTMLAttributes<T> = globalThis.React.VideoHTMLAttributes<T>;
  declare export type SVGAttributes<T> = globalThis.React.SVGAttributes<T>;
  declare export type WebViewHTMLAttributes<T> = globalThis.React.WebViewHTMLAttributes<T>;
  declare export type HTMLElementType = globalThis.React.HTMLElementType;
  declare export type SVGElementType = globalThis.React.SVGElementType;
  declare export type AbstractView = globalThis.React.AbstractView;
  declare export type Touch = globalThis.React.Touch;
  declare export type TouchList = globalThis.React.TouchList;

    /**
   * `act` makes sure updates are applied before making test assertions
   *
   * @see https://react.dev/reference/react/act
   */
  declare export const act: typeof React.act;
  /**
   * `captureOwnerStack` reads the current Owner Stack in development and returns
   * it as a string if available.
   *
   * @see https://react.dev/reference/react/captureOwnerStack
   */
  declare export const captureOwnerStack: typeof React.captureOwnerStack;

  declare export const version: string;
  /**
   * `createContext` lets you create a context that components can provide or read.
   *
   * @see https://react.dev/reference/react/createContext
   */
  declare export const createContext: typeof React.createContext;
  /**
   * `createElement` lets you create a React element.
   * It serves as an alternative to writing JSX.
   *
   * @see https://react.dev/reference/react/createElement
   */
  declare export const createElement: React$CreateElement;
  /**
   * Using cloneElement is uncommon and can lead to fragile code.
   * [See common alternatives](https://react.dev/reference/react/cloneElement#alternatives)
   *
   * @see https://react.dev/reference/react/cloneElement
   */
  declare export const cloneElement: React$CloneElement;
  /**
   * `createRef` creates a ref object which can contain arbitrary value.
   *
   * `createRef` is mostly used for class components.
   * Function components typically rely on `useRef` instead.
   *
   * @see https://react.dev/reference/react/createRef
   */
  declare export const createRef: typeof React.createRef;
  /**
   * `isValidElement` checks whether a value is a React element.
   *
   * It is very uncommon to need isValidElement.
   * It’s mostly useful if you’re calling another API that only accepts
   * elements (like cloneElement does) and you want to avoid an error
   * when your argument is not a React element.
   *
   * Unless you have some very specific reason to add an isValidElement check,
   * you probably don’t need it.
   *
   * @see https://react.dev/reference/react/isValidElement
   */
  declare export function isValidElement(element: any): boolean;

  /**
   * `Component` lets you define a React component as a JavaScript class.
   *
   * We recommend defining components as functions instead of classes.
   * [See how to migrate](https://react.dev/reference/react/PureComponent#alternatives).
   *
   * @see https://react.dev/reference/react/Component
   */
  declare export const Component: typeof React.Component;

  /**
   * `PureComponent` is similar to `Component`, but it skip re-renders with same props.
   *
   * We recommend defining components as functions instead of classes.
   * [See how to migrate](https://react.dev/reference/react/PureComponent#alternatives).
   *
   * @see https://react.dev/reference/react/PureComponent
   */
  declare export const PureComponent: typeof React.PureComponent;
  declare export type ComponentType<in P extends {...}> = component(...P);
  declare export type PropsOf<C extends string | MixedElement | React$RendersExactly<React$ElementType>> = React.PropsOf<C>;
  declare export type PropOf<C extends string | MixedElement | React$RendersExactly<React$ElementType>, K extends string> = React.PropOf<C, K>;
  declare export type RefOf<C extends string | MixedElement | React$RendersExactly<React$ElementType>> = React.RefOf<C>;
  declare export type MixedElement = React.MixedElement;
  declare export type ElementType = React$ElementType;

  /**
   * `<Fragment>`, often used via `<>...</>` syntax,
   * lets you group elements without a wrapper node.
   *
   * @see https://react.dev/reference/react/Fragment
   */
  declare export const Fragment: typeof React.Fragment;

  declare export type Key = React$Key;
  declare export type RefSetter<in T> = React.RefSetter<T>;
  declare export type Node = React$Node;
  declare export type Context<T> = React.Context<T>;
  declare export type Portal = React$Portal;
  /**
   * `<StrictMode>` lets you find common bugs in your components early
   * during development.
   *
   * @see https://react.dev/reference/react/StrictMode
   */
  declare export const StrictMode: typeof React.StrictMode;

  /**
   * `<Suspense>` lets you display a fallback until its children have finished
   * loading.
   *
   * @see https://react.dev/reference/react/Suspense
   */
  declare export const Suspense: typeof React.Suspense; // 16.6+

  /**
   * `<Activity>` lets you hide and show parts of the UI.
   */
  declare export const Activity: typeof React.Activity;
  declare export type ElementRef<C> = React.ElementRef<C>;
  declare export type ReactManagedAttributes<C, P> = globalThis.React.ReactManagedAttributes<C, P>;

  declare interface Thenable<out T> {
    then(
      onFulfill: (value: T) => any,
      onReject: (error: unknown) => unknown,
    ): unknown;
  }

  declare type Usable<T> = Thenable<T> | React.Context<T>;

  declare export type ChildrenArray<out T> = React.ChildrenArray<T>;
  /**
   * `Children`lets you manipulate and transform the JSX received as the
   * children prop.
   *
   * Using `Children` is uncommon and can lead to fragile code.
   * [See common alternatives](https://react.dev/reference/react/Children#alternatives).
   *
   * @see https://react.dev/reference/react/Children
   */
  declare export const Children: typeof React.Children;

  /**
   * In React 19, `forwardRef` is no longer necessary. Pass `ref` as a prop instead.
   * forwardRef will deprecated in a future release.
   *
   * @see https://react.dev/blog/2024/12/05/react-19#ref-as-a-prop
   * @deprecated
   */
  declare export const forwardRef: typeof React.forwardRef;

  /**
   * `memo` lets you skip re-rendering a component when its props are unchanged.
   *
   * @see https://react.dev/reference/react/memo
   */
  declare export const memo: typeof React.memo;

  /**
   * `lazy` lets you defer loading component’s code until it is rendered for
   * the first time.
   *
   * @see https://react.dev/reference/react/lazy
   */
  declare export const lazy: typeof React.lazy;

  declare type MaybeCleanUpFn = void | (() => void);
  declare export type ReactSetStateFunction<S> = React.ReactSetStateFunction<S>;

  /**
   * `use` is a React Hook that lets you read the value of a resource like a
   * `Promise` or `context`.
   *
   * @see https://react.dev/reference/react/use
   */
  declare export function use<T>(usable: Usable<T>): T;

  /**
   * `useContext` is a React Hook that lets you read and subscribe to context
   * from your component.
   *
   * @see https://react.dev/reference/react/useContext
   */
  declare export const useContext: typeof React.useContext;

  /**
   * `useState` is a React Hook that lets you add a
   * [state variable](https://react.dev/learn/state-a-components-memory)
   * to your component.
   *
   * @see https://react.dev/reference/react/useState
   */
  declare export const useState: typeof React.useState;

  declare export type Dispatch<in A> = (A) => void;

  /**
   * `useReducer` is a React Hook that lets you add a
   * [reducer](https://react.dev/learn/extracting-state-logic-into-a-reducer)
   * to your component.
   *
   * @see https://react.dev/reference/react/useReducer
   */
  declare export const useReducer: typeof React.useReducer;

  declare export type RefObject<T> = React$RefObject<T>;
  /**
   * `useRef` is a React Hook that lets you reference a value that's
   * not needed for rendering.
   *
   * @see https://react.dev/reference/react/useRef
   */
  declare export const useRef: typeof React.useRef;

  /**
   * `useDebugValue` is a React Hook that lets you add a label to a custom Hook
   * in React DevTools.
   *
   * @see https://react.dev/reference/react/useDebugValue
   */
  declare export function useDebugValue(value: any): void;

  /**
   * `useEffect` is a React Hook that lets you
   * [synchronize a component with an external system](https://react.dev/learn/synchronizing-with-effects).
   *
   * @see https://react.dev/reference/react/useEffect
   */
  declare export const useEffect: typeof React.useEffect;

  /**
   * `useEffectEvent` is a React Hook that lets you
   * [create a stable event function with the latest values that can be called in Effects](https://react.dev/learn/separating-events-from-effects)
   */
  declare export const useEffectEvent: typeof React.useEffectEvent;

  /**
   * `useLayoutEffect` is a version of useEffect that fires before the browser
   * repaints the screen.
   *
   * `useLayoutEffect` can hurt performance. Prefer `useEffect` when possible.
   *
   * @see https://react.dev/reference/react/useLayoutEffect
   */
  declare export const useLayoutEffect: typeof React.useLayoutEffect;

  /**
   * `useCallback` is a React Hook that lets you cache a function definition
   * between re-renders.
   *
   * @see https://react.dev/reference/react/useCallback
   */
  declare export const useCallback: typeof React.useCallback;

  /**
   * useMemo is a React Hook that lets you cache the result of a calculation
   * between re-renders.
   *
   * @see https://react.dev/reference/react/useMemo
   */
  declare export const useMemo: typeof React.useMemo;

  /**
   * `useImperativeHandle` is a React Hook that lets you customize the handle
   * exposed as a ref.
   *
   * @see https://react.dev/reference/react/useImperativeHandle
   */
  declare export const useImperativeHandle: typeof React.useImperativeHandle;

  /**
   * `useDeferredValue` is a React Hook that lets you defer updating
   * a part of the UI.
   *
   * @see https://react.dev/reference/react/useDeferredValue
   */
  declare export const useDeferredValue: typeof React.useDeferredValue;

  /**
   * `useTransition` is a React Hook that lets you update the state
   * without blocking the UI.
   *
   * @see https://react.dev/reference/react/useTransition
   */
  declare export const useTransition: typeof React.useTransition;

  /**
   * `startTransition` lets you update the state without blocking the UI.
   *
   * @see https://react.dev/reference/react/startTransition
   */
  declare export const startTransition: typeof React.startTransition;

  /**
   * `useId` is a React Hook for generating unique IDs that
   * can be passed to accessibility attributes.
   *
   * @see https://react.dev/reference/react/useId
   */
  declare export const useId: typeof React.useId;

  /**
   * `useInsertionEffect` allows inserting elements into the DOM
   * before any layout effects fire.
   *
   * `useInsertionEffect` is for CSS-in-JS library authors. Unless you are
   * working on a CSS-in-JS library and need a place to inject the styles,
   * you probably want `useEffect` or `useLayoutEffect` instead.
   *
   * @see https://react.dev/reference/react/useInsertionEffect
   */
  declare export const useInsertionEffect: typeof React.useInsertionEffect;

  /**
   * `useSyncExternalStore` is a React Hook that lets you subscribe
   * to an external store.
   *
   * @see https://react.dev/reference/react/useSyncExternalStore
   */
  declare export const useSyncExternalStore: typeof React.useSyncExternalStore;

  /**
   * `useOptimistic` is a React Hook that lets you optimistically update
   * the UI.
   *
   * @see https://react.dev/reference/react/useOptimistic
   */
  declare export const useOptimistic: typeof React.useOptimistic;

  /**
   * `useActionState` is a Hook that allows you to update state based on the
   * result of a form action.
   *
   * @see https://react.dev/reference/react/useActionState
   */
  declare export const useActionState: typeof React.useActionState;

  declare export type Interaction = React.Interaction;

  // TS compat: core types
  /**
   * @deprecated TypeScript compatibility alias. Use `React.Node` instead.
   */
  declare export type ReactNode = React.Node;
  /**
   * Represents a JSX element.
   */
  declare export type ReactElement<out P = unknown, out T = React.ElementType> = ExactReactElement_DEPRECATED<T, P>;
  /**
   * @deprecated TypeScript compatibility alias. Use `React.Portal` instead.
   */
  declare export type ReactPortal = React.Portal;
  /**
   * @deprecated TypeScript compatibility alias. Use `string | number` instead.
   */
  declare export type ReactText = string | number;
  /**
   * @deprecated TypeScript compatibility alias.
   */
  declare export type ReactInstance = any;
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  declare export type JSXElementConstructor<in P extends {...} = {...}> = component(...P);

  // TS compat: component types
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  declare export type FC<in P extends {...} = {...}> = component(...P);
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  declare export type FunctionComponent<in P extends {...} = {...}> = component(...P);
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  declare export type ComponentClass<in P extends {...} = {...}, S = any> = component(...P);
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  declare export type ForwardRefExoticComponent<in P extends {...} = {...}> = component(...P);
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  declare export type NamedExoticComponent<in P extends {...} = {...}> = component(...P);
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  declare export type ExoticComponent<in P extends {...} = {...}> = component(...P);
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  declare export type MemoExoticComponent<T extends React.ComponentType<any>> = component(...React.ComponentProps<T>);
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
   */
  declare export type LazyExoticComponent<T extends React.ComponentType<any>> = component(...React.ComponentProps<T>);
  /**
   * The props accepted by a component or JSX intrinsic, including its ref prop.
   */
  declare export type ComponentProps<T extends React.ElementType> = globalThis.React.ComponentProps<T>;
  /**
   * @deprecated TypeScript compatibility alias. Use `React.ComponentProps` instead.
   */
  declare export type ComponentPropsWithRef<T extends React.ElementType> = globalThis.React.ComponentProps<T>;
  /**
   * @deprecated TypeScript compatibility alias. Use `Omit<React.ComponentProps<T>, 'ref'>` instead.
   */
  declare export type ComponentPropsWithoutRef<T extends React.ElementType> = Omit<globalThis.React.ComponentProps<T>, 'ref'>;
  /**
   * @deprecated TypeScript compatibility alias.
   */
  declare export type PropsWithChildren<out P = {...}> = P & {children?: React.Node, ...};
  /**
   * @deprecated TypeScript compatibility alias.
   */
  declare export type PropsWithoutRef<P> = Omit<P, 'ref'>;

  // TS compat: ref & attribute types
  /**
   * @deprecated TypeScript compatibility alias. Use `React.RefSetter` instead.
   */
  declare export type Ref<in T> = React.RefSetter<T>;
  /**
   * @deprecated TypeScript compatibility alias. Use `React.RefSetter` instead.
   */
  declare export type RefCallback<in T> = (T | null) => void | (() => void);
  /**
   * @deprecated TypeScript compatibility alias.
   */
  declare export type RefAttributes<T> = { ref?: React.RefSetter<T>, ...};
  /**
   * @deprecated TypeScript compatibility alias.
   */
  declare export type ClassAttributes<T> = React.RefAttributes<T>;
  /**
   * @deprecated TypeScript compatibility alias.
   */
  declare export type Attributes = { key?: React.Key, ...};
  /**
   * @deprecated TypeScript compatibility alias. Use `React.RefObject` instead.
   */
  declare export type MutableRefObject<T> = { current: T, ... };
  /**
   * @deprecated TypeScript compatibility alias.
   */
  declare export type ForwardRefRenderFunction<out T, in P = {...}> = (props: P, ref: React.RefSetter<T>) => React.Node;

  // TS compat: context types
  /**
   * @deprecated TypeScript compatibility alias. Prefer `React.Context<T>` for the context
   * object; this type represents only the provider component.
   */
  declare export type Provider<in T> = component(value: T, children?: React.Node);
  /**
   * @deprecated TypeScript compatibility alias. Prefer `React.Context<T>` for the context
   * object; this type represents only the consumer component.
   */
  declare export type Consumer<out T> = component(children: (value: T) => React.Node);
  /**
   * @deprecated TypeScript compatibility alias.
   */
  declare export type ProviderProps<T> = {value: T, children?: React.Node, ...};

  // TS compat: hook/misc types
  /**
   * @deprecated TypeScript compatibility alias.
   */
  declare export type SetStateAction<S> = S | ((S) => S);
  /**
   * @deprecated TypeScript compatibility alias.
   */
  declare export type DependencyList = ReadonlyArray<unknown>;
  /**
   * @deprecated TypeScript compatibility alias.
   */
  declare export type EffectCallback = () => void | (() => void);
  /**
   * @deprecated TypeScript compatibility alias.
   */
  declare export type ErrorInfo = {componentStack?: string | null, ...};
  /**
   * @deprecated TypeScript compatibility alias.
   */
  declare export type ValidationMap<T> = {[key: string]: any, ...};
  /**
   * @deprecated TypeScript compatibility alias.
   */
  declare export type WeakValidationMap<T> = {[key: string]: any, ...};
  /**
   * @deprecated TypeScript compatibility alias.
   */
  declare export type TransitionStartFunction = (callback: () => void | Promise<void>) => void;
  /**
   * @deprecated TypeScript compatibility alias.
   */
  declare export type ActionDispatch<in ActionArg extends ReadonlyArray<any> = []> = (...args: ActionArg) => void;

  declare type ProfilerOnRenderFnType = (
    id: string,
    phase: "mount" | "update",
    actualDuration: number,
    baseDuration: number,
    startTime: number,
    commitTime: number,
    interactions: Set<Interaction>,
  ) => void;

  /**
   * `<Profiler>` lets you measure rendering performance of a
   * React tree programmatically.
   *
   * @see https://react.dev/reference/react/Profiler
   */
  declare export const Profiler: typeof React.Profiler;

  declare type TimeoutConfig = {
    timeoutMs: number,
  };

  declare namespace React {
    type BaseSyntheticEvent<E = object, C = any, T = any> = globalThis.React.BaseSyntheticEvent<E, C, T>;
    type SyntheticEvent<T = Element, E = Event> = globalThis.React.SyntheticEvent<T, E>;
    type ClipboardEvent<T = Element> = globalThis.React.ClipboardEvent<T>;
    type CompositionEvent<T = Element> = globalThis.React.CompositionEvent<T>;
    type DragEvent<T = Element> = globalThis.React.DragEvent<T>;
    type PointerEvent<T = Element> = globalThis.React.PointerEvent<T>;
    type FocusEvent<Target = Element, RelatedTarget = Element> = globalThis.React.FocusEvent<Target, RelatedTarget>;
    type FormEvent<T = Element> = globalThis.React.FormEvent<T>;
    type InvalidEvent<T = Element> = globalThis.React.InvalidEvent<T>;
    type ChangeEvent<CurrentTarget = Element, Target = Element> = globalThis.React.ChangeEvent<CurrentTarget, Target>;
    type InputEvent<T = Element> = globalThis.React.InputEvent<T>;
    type ModifierKey = globalThis.React.ModifierKey;
    type KeyboardEvent<T = Element> = globalThis.React.KeyboardEvent<T>;
    type MouseEvent<T = Element, E = globalThis.MouseEvent> = globalThis.React.MouseEvent<T, E>;
    type SubmitEvent<T = Element> = globalThis.React.SubmitEvent<T>;
    type TouchEvent<T = Element> = globalThis.React.TouchEvent<T>;
    type UIEvent<T = Element, E = globalThis.UIEvent> = globalThis.React.UIEvent<T, E>;
    type WheelEvent<T = Element> = globalThis.React.WheelEvent<T>;
    type AnimationEvent<T = Element> = globalThis.React.AnimationEvent<T>;
    type ToggleEvent<T = Element> = globalThis.React.ToggleEvent<T>;
    type TransitionEvent<T = Element> = globalThis.React.TransitionEvent<T>;
    type EventHandler<E> = globalThis.React.EventHandler<E>;
    type ReactEventHandler<T = Element> = globalThis.React.ReactEventHandler<T>;
    type ClipboardEventHandler<T = Element> = globalThis.React.ClipboardEventHandler<T>;
    type CompositionEventHandler<T = Element> = globalThis.React.CompositionEventHandler<T>;
    type DragEventHandler<T = Element> = globalThis.React.DragEventHandler<T>;
    type FocusEventHandler<T = Element> = globalThis.React.FocusEventHandler<T>;
    type FormEventHandler<T = Element> = globalThis.React.FormEventHandler<T>;
    type ChangeEventHandler<CurrentTarget = Element, Target = Element> = globalThis.React.ChangeEventHandler<CurrentTarget, Target>;
    type InputEventHandler<T = Element> = globalThis.React.InputEventHandler<T>;
    type KeyboardEventHandler<T = Element> = globalThis.React.KeyboardEventHandler<T>;
    type MouseEventHandler<T = Element> = globalThis.React.MouseEventHandler<T>;
    type SubmitEventHandler<T = Element> = globalThis.React.SubmitEventHandler<T>;
    type TouchEventHandler<T = Element> = globalThis.React.TouchEventHandler<T>;
    type PointerEventHandler<T = Element> = globalThis.React.PointerEventHandler<T>;
    type UIEventHandler<T = Element> = globalThis.React.UIEventHandler<T>;
    type WheelEventHandler<T = Element> = globalThis.React.WheelEventHandler<T>;
    type AnimationEventHandler<T = Element> = globalThis.React.AnimationEventHandler<T>;
    type ToggleEventHandler<T = Element> = globalThis.React.ToggleEventHandler<T>;
    type TransitionEventHandler<T = Element> = globalThis.React.TransitionEventHandler<T>;
    type HTMLProps<T> = globalThis.React.HTMLProps<T>;
    type DetailedHTMLProps<E, T> = globalThis.React.DetailedHTMLProps<E, T>;
    type SVGProps<T> = globalThis.React.SVGProps<T>;
    type SVGLineElementAttributes<T> = globalThis.React.SVGLineElementAttributes<T>;
    type SVGTextElementAttributes<T> = globalThis.React.SVGTextElementAttributes<T>;
    type DOMAttributes<T> = globalThis.React.DOMAttributes<T>;
    type CSSProperties = globalThis.React.CSSProperties;
    type AriaAttributes = globalThis.React.AriaAttributes;
    type AriaRole = globalThis.React.AriaRole;
    type HTMLAttributes<T> = globalThis.React.HTMLAttributes<T>;
    type DO_NOT_USE_OR_YOU_WILL_BE_FIRED_EXPERIMENTAL_FORM_ACTIONS = globalThis.React.DO_NOT_USE_OR_YOU_WILL_BE_FIRED_EXPERIMENTAL_FORM_ACTIONS;
    type AllHTMLAttributes<T> = globalThis.React.AllHTMLAttributes<T>;
    type HTMLAttributeReferrerPolicy = globalThis.React.HTMLAttributeReferrerPolicy;
    type HTMLAttributeAnchorTarget = globalThis.React.HTMLAttributeAnchorTarget;
    type AnchorHTMLAttributes<T> = globalThis.React.AnchorHTMLAttributes<T>;
    type AudioHTMLAttributes<T> = globalThis.React.AudioHTMLAttributes<T>;
    type AreaHTMLAttributes<T> = globalThis.React.AreaHTMLAttributes<T>;
    type BaseHTMLAttributes<T> = globalThis.React.BaseHTMLAttributes<T>;
    type BlockquoteHTMLAttributes<T> = globalThis.React.BlockquoteHTMLAttributes<T>;
    type ButtonHTMLAttributes<T> = globalThis.React.ButtonHTMLAttributes<T>;
    type CanvasHTMLAttributes<T> = globalThis.React.CanvasHTMLAttributes<T>;
    type ColHTMLAttributes<T> = globalThis.React.ColHTMLAttributes<T>;
    type ColgroupHTMLAttributes<T> = globalThis.React.ColgroupHTMLAttributes<T>;
    type DataHTMLAttributes<T> = globalThis.React.DataHTMLAttributes<T>;
    type DetailsHTMLAttributes<T> = globalThis.React.DetailsHTMLAttributes<T>;
    type DelHTMLAttributes<T> = globalThis.React.DelHTMLAttributes<T>;
    type DialogHTMLAttributes<T> = globalThis.React.DialogHTMLAttributes<T>;
    type EmbedHTMLAttributes<T> = globalThis.React.EmbedHTMLAttributes<T>;
    type FieldsetHTMLAttributes<T> = globalThis.React.FieldsetHTMLAttributes<T>;
    type FormHTMLAttributes<T> = globalThis.React.FormHTMLAttributes<T>;
    type HtmlHTMLAttributes<T> = globalThis.React.HtmlHTMLAttributes<T>;
    type IframeHTMLAttributes<T> = globalThis.React.IframeHTMLAttributes<T>;
    type DO_NOT_USE_OR_YOU_WILL_BE_FIRED_EXPERIMENTAL_IMG_SRC_TYPES = globalThis.React.DO_NOT_USE_OR_YOU_WILL_BE_FIRED_EXPERIMENTAL_IMG_SRC_TYPES;
    type ImgHTMLAttributes<T> = globalThis.React.ImgHTMLAttributes<T>;
    type InsHTMLAttributes<T> = globalThis.React.InsHTMLAttributes<T>;
    type HTMLInputTypeAttribute = globalThis.React.HTMLInputTypeAttribute;
    type AutoFillAddressKind = globalThis.React.AutoFillAddressKind;
    type AutoFillBase = globalThis.React.AutoFillBase;
    type AutoFillContactField = globalThis.React.AutoFillContactField;
    type AutoFillContactKind = globalThis.React.AutoFillContactKind;
    type AutoFillCredentialField = globalThis.React.AutoFillCredentialField;
    type AutoFillNormalField = globalThis.React.AutoFillNormalField;
    type OptionalPrefixToken<T extends string> = globalThis.React.OptionalPrefixToken<T>;
    type OptionalPostfixToken<T extends string> = globalThis.React.OptionalPostfixToken<T>;
    type AutoFillField = globalThis.React.AutoFillField;
    type AutoFillSection = globalThis.React.AutoFillSection;
    type AutoFill = globalThis.React.AutoFill;
    type HTMLInputAutoCompleteAttribute = globalThis.React.HTMLInputAutoCompleteAttribute;
    type InputHTMLAttributes<T> = globalThis.React.InputHTMLAttributes<T>;
    type KeygenHTMLAttributes<T> = globalThis.React.KeygenHTMLAttributes<T>;
    type LabelHTMLAttributes<T> = globalThis.React.LabelHTMLAttributes<T>;
    type LiHTMLAttributes<T> = globalThis.React.LiHTMLAttributes<T>;
    type LinkHTMLAttributes<T> = globalThis.React.LinkHTMLAttributes<T>;
    type MapHTMLAttributes<T> = globalThis.React.MapHTMLAttributes<T>;
    type MenuHTMLAttributes<T> = globalThis.React.MenuHTMLAttributes<T>;
    type DO_NOT_USE_OR_YOU_WILL_BE_FIRED_EXPERIMENTAL_MEDIA_SRC_TYPES = globalThis.React.DO_NOT_USE_OR_YOU_WILL_BE_FIRED_EXPERIMENTAL_MEDIA_SRC_TYPES;
    type MediaHTMLAttributes<T> = globalThis.React.MediaHTMLAttributes<T>;
    type MetaHTMLAttributes<T> = globalThis.React.MetaHTMLAttributes<T>;
    type MeterHTMLAttributes<T> = globalThis.React.MeterHTMLAttributes<T>;
    type QuoteHTMLAttributes<T> = globalThis.React.QuoteHTMLAttributes<T>;
    type ObjectHTMLAttributes<T> = globalThis.React.ObjectHTMLAttributes<T>;
    type OlHTMLAttributes<T> = globalThis.React.OlHTMLAttributes<T>;
    type OptgroupHTMLAttributes<T> = globalThis.React.OptgroupHTMLAttributes<T>;
    type OptionHTMLAttributes<T> = globalThis.React.OptionHTMLAttributes<T>;
    type OutputHTMLAttributes<T> = globalThis.React.OutputHTMLAttributes<T>;
    type ParamHTMLAttributes<T> = globalThis.React.ParamHTMLAttributes<T>;
    type ProgressHTMLAttributes<T> = globalThis.React.ProgressHTMLAttributes<T>;
    type SlotHTMLAttributes<T> = globalThis.React.SlotHTMLAttributes<T>;
    type ScriptHTMLAttributes<T> = globalThis.React.ScriptHTMLAttributes<T>;
    type SelectHTMLAttributes<T> = globalThis.React.SelectHTMLAttributes<T>;
    type SourceHTMLAttributes<T> = globalThis.React.SourceHTMLAttributes<T>;
    type StyleHTMLAttributes<T> = globalThis.React.StyleHTMLAttributes<T>;
    type TableHTMLAttributes<T> = globalThis.React.TableHTMLAttributes<T>;
    type TextareaHTMLAttributes<T> = globalThis.React.TextareaHTMLAttributes<T>;
    type TdHTMLAttributes<T> = globalThis.React.TdHTMLAttributes<T>;
    type ThHTMLAttributes<T> = globalThis.React.ThHTMLAttributes<T>;
    type TimeHTMLAttributes<T> = globalThis.React.TimeHTMLAttributes<T>;
    type TrackHTMLAttributes<T> = globalThis.React.TrackHTMLAttributes<T>;
    type VideoHTMLAttributes<T> = globalThis.React.VideoHTMLAttributes<T>;
    type SVGAttributes<T> = globalThis.React.SVGAttributes<T>;
    type WebViewHTMLAttributes<T> = globalThis.React.WebViewHTMLAttributes<T>;
    type HTMLElementType = globalThis.React.HTMLElementType;
    type SVGElementType = globalThis.React.SVGElementType;
    type AbstractView = globalThis.React.AbstractView;
    type Touch = globalThis.React.Touch;
    type TouchList = globalThis.React.TouchList;

    /**
     * `act` makes sure updates are applied before making test assertions
     *
     * @see https://react.dev/reference/react/act
     */
    declare export function act(callback: () => void): void;
    declare export function act<T>(callback: () => T | Promise<T>): Promise<T>;

    /**
     * `captureOwnerStack` reads the current Owner Stack in development and returns
     * it as a string if available.
     *
     * @see https://react.dev/reference/react/captureOwnerStack
     */
    declare export function captureOwnerStack(): string | null;

    declare const version: string;

    /**
     * `memo` lets you skip re-rendering a component when its props are unchanged.
     *
     * @see https://react.dev/reference/react/memo
     */
    declare function memo<Config extends {...}, Renders extends React.Node = React.Node>(
      component: component(...Config) renders Renders,
      equal?: (Config, Config) => boolean,
    ): component(...Config) renders Renders;

    /**
     * `lazy` lets you defer loading component’s code until it is rendered for
     * the first time.
     *
     * @see https://react.dev/reference/react/lazy
     */
    declare function lazy<Config extends {...}, Renders extends React.Node = React.Node>(
      component: () => Promise<
        Readonly<{
          default: component(...Config) renders Renders,
          ...
        }>,
      >,
    ): component(...Config) renders Renders;

    /**
     * `createContext` lets you create a context that components can provide or read.
     *
     * @see https://react.dev/reference/react/createContext
     */
    declare function createContext<T>(
      defaultValue: T,
      calculateChangedBits: ?(a: T, b: T) => number,
    ): React.Context<T>;
    /**
     * `createElement` lets you create a React element.
     * It serves as an alternative to writing JSX.
     *
     * @see https://react.dev/reference/react/createElement
     */
    declare const createElement: React$CreateElement;

    /**
     * Using cloneElement is uncommon and can lead to fragile code.
     * [See common alternatives](https://react.dev/reference/react/cloneElement#alternatives)
     *
     * @see https://react.dev/reference/react/cloneElement
     */
    declare const cloneElement: React$CloneElement;

    /**
     * `createRef` creates a ref object which can contain arbitrary value.
     *
     * `createRef` is mostly used for class components.
     * Function components typically rely on `useRef` instead.
     *
     * @see https://react.dev/reference/react/createRef
     */
    declare function createRef<T>(
    ): React$RefObject<T | null>;
    /**
     * `isValidElement` checks whether a value is a React element.
     *
     * It is very uncommon to need isValidElement.
     * It’s mostly useful if you’re calling another API that only accepts
     * elements (like cloneElement does) and you want to avoid an error
     * when your argument is not a React element.
     *
     * Unless you have some very specific reason to add an isValidElement check,
     * you probably don’t need it.
     *
     * @see https://react.dev/reference/react/isValidElement
     */
    declare function isValidElement(element: any): boolean;

    /**
     * `Component` lets you define a React component as a JavaScript class.
     *
     * We recommend defining components as functions instead of classes.
     * [See how to migrate](https://react.dev/reference/react/PureComponent#alternatives).
     *
     * @see https://react.dev/reference/react/Component
     */
    declare class Component<Props, State = void> {
      // fields

      props: Props;
      state: State;

      // action methods

      setState(
        partialState: ?Readonly<Partial<State>> | ((State, Props) => ?Readonly<Partial<State>>),
        callback?: () => unknown,
      ): void;

      forceUpdate(callback?: () => void): void;

      // lifecycle methods

      constructor(props: Props, context?: any): void;
      render(): React$Node;
      componentWillMount(): unknown;
      UNSAFE_componentWillMount(): unknown;
      componentDidMount(): unknown;
      componentWillReceiveProps(
        nextProps: Props,
        nextContext: any,
      ): unknown;
      UNSAFE_componentWillReceiveProps(
        nextProps: Props,
        nextContext: any,
      ): unknown;
      shouldComponentUpdate(
        nextProps: Props,
        nextState: State,
        nextContext: any,
      ): boolean;
      componentWillUpdate(
        nextProps: Props,
        nextState: State,
        nextContext: any,
      ): unknown;
      UNSAFE_componentWillUpdate(
        nextProps: Props,
        nextState: State,
        nextContext: any,
      ): unknown;
      componentDidUpdate(
        prevProps: Props,
        prevState: State,
        prevContext: any,
      ): unknown;
      componentWillUnmount(): unknown;
      componentDidCatch(
        error: Error,
        info: { componentStack: string, ... }
      ): unknown;

      // long tail of other stuff not modeled very well

      refs: any;
      context: any;
      getChildContext(): any;
      static displayName?: ?string;
      static childContextTypes: empty;
      static contextTypes: empty;
      static propTypes: any;

      // We don't add a type for `defaultProps` so that its type may be entirely
      // inferred when we diff the type for `defaultProps` with `Props`. Otherwise
      // the user would need to define a type (which would be redundant) to override
      // the type we provide here in the base class.
      //
      // static defaultProps: $Shape<Props>;
    }

    /**
     * `PureComponent` is similar to `Component`, but it skip re-renders with same props.
     *
     * We recommend defining components as functions instead of classes.
     * [See how to migrate](https://react.dev/reference/react/PureComponent#alternatives).
     *
     * @see https://react.dev/reference/react/PureComponent
     */
    declare export class PureComponent<Props, State = void> extends Component<Props, State> {}

    /**
     * In React 19, `forwardRef` is no longer necessary. Pass `ref` as a prop instead.
     * forwardRef will deprecated in a future release.
     *
     * @see https://react.dev/blog/2024/12/05/react-19#ref-as-a-prop
     * @deprecated
     */
    declare function forwardRef<
      Config extends {...},
      Instance,
      Renders extends React$Node = React$Node,
    >(
      render: (props: Config, ref: React.RefSetter<Instance>) => Renders,
    ): component(ref?: React.RefSetter<Instance>, ...Config) renders Renders;

    /**
     * `<Fragment>`, often used via `<>...</>` syntax,
     * lets you group elements without a wrapper node.
     *
     * @see https://react.dev/reference/react/Fragment
     */
    declare component Fragment<Renders extends React$Node = void>(
      children?: Renders,
    ) renders Renders;

    /**
     * `Children`lets you manipulate and transform the JSX received as the
     * children prop.
     *
     * Using `Children` is uncommon and can lead to fragile code.
     * [See common alternatives](https://react.dev/reference/react/Children#alternatives).
     *
     * @see https://react.dev/reference/react/Children
     */
    declare const Children: {
      map<T, U, This>(
        children: ChildrenArray<T>,
        fn: (this : This, child: NonNullable<T>, index: number) => U,
        thisArg: This,
      ): Array<NonNullable<U>>,
      forEach<T, This>(
        children: ChildrenArray<T>,
        fn: (this : This, child: T, index: number) => unknown,
        thisArg: This,
      ): void,
      count(children: ChildrenArray<any>): number,
      only<T>(children: ChildrenArray<T>): NonNullable<T>,
      toArray<T>(children: ChildrenArray<T>): Array<NonNullable<T>>,
      ...
    };

    /**
     * `<StrictMode>` lets you find common bugs in your components early
     * during development.
     *
     * @see https://react.dev/reference/react/StrictMode
     */
    declare const StrictMode: ({ readonly children?: React$Node, ... }) => React$Node;

    /**
     * `<Suspense>` lets you display a fallback until its children have finished
     * loading.
     *
     * @see https://react.dev/reference/react/Suspense
     */
    declare const Suspense: React.ComponentType<{
      readonly children?: React$Node,
      readonly fallback?: React$Node,
      ...
    }>; // 16.6+

    /**
     * `<Activity>` lets you hide and show parts of the UI.
     */
    declare const Activity: component<Renders extends React.Node = void>(
      children?: React.Node,
      mode?: 'visible' | 'hidden',
    ) renders Renders;

    /**
     * `<Profiler>` lets you measure rendering performance of a
     * React tree programmatically.
     *
     * @see https://react.dev/reference/react/Profiler
     */
    declare const Profiler: component(
      children?: React$Node,
      id: string,
      onRender: ProfilerOnRenderFnType,
    );

    /**
     * `useContext` is a React Hook that lets you read and subscribe to context
     * from your component.
     *
     * @see https://react.dev/reference/react/useContext
     */
    declare hook useContext<T>(context: React.Context<T>): T;

    /**
     * `useState` is a React Hook that lets you add a
     * [state variable](https://react.dev/learn/state-a-components-memory)
     * to your component.
     *
     * @see https://react.dev/reference/react/useState
     */
    declare hook useState<S>(
      initialState: (() => S) | S,
    ): [S, ReactSetStateFunction<S>];

    /**
     * `useReducer` is a React Hook that lets you add a
     * [reducer](https://react.dev/learn/extracting-state-logic-into-a-reducer)
     * to your component.
     *
     * @see https://react.dev/reference/react/useReducer
     */
    declare hook useReducer<S, A>(
      reducer: (S, A) => S,
      initialState: S,
    ): [S, Dispatch<A>];

    declare hook useReducer<S, A>(
      reducer: (S, A) => S,
      initialState: S,
      init: void,
    ): [S, Dispatch<A>];

    declare hook useReducer<S, A, I>(
      reducer: (S, A) => S,
      initialArg: I,
      init: (I) => S,
    ): [S, Dispatch<A>];

    /**
     * `useRef` is a React Hook that lets you reference a value that's
     * not needed for rendering.
     *
     * @see https://react.dev/reference/react/useRef
     */
    declare hook useRef<T>(initialValue: T): RefObject<T>;

    /**
     * `useEffect` is a React Hook that lets you
     * [synchronize a component with an external system](https://react.dev/learn/synchronizing-with-effects).
     *
     * @see https://react.dev/reference/react/useEffect
     */
    declare hook useEffect(
      create: () => MaybeCleanUpFn,
      inputs?: ?ReadonlyArray<unknown>,
    ): void;

    /**
     * `useEffectEvent` is a React Hook that lets you
     * [create a stable event function with the latest values that can be called in Effects](https://react.dev/learn/separating-events-from-effects)
     */
    declare hook useEffectEvent<F extends (...empty) => unknown>(
      callback: F,
    ): F;

    /**
     * `useLayoutEffect` is a version of useEffect that fires before the browser
     * repaints the screen.
     *
     * `useLayoutEffect` can hurt performance. Prefer `useEffect` when possible.
     *
     * @see https://react.dev/reference/react/useLayoutEffect
     */
    declare hook useLayoutEffect(
      create: () => MaybeCleanUpFn,
      inputs?: ?ReadonlyArray<unknown>,
    ): void;

    /**
     * `useCallback` is a React Hook that lets you cache a function definition
     * between re-renders.
     *
     * @see https://react.dev/reference/react/useCallback
     */
    declare hook useCallback<T extends (...args: ReadonlyArray<empty>) => unknown>(
      callback: T,
      inputs: ?ReadonlyArray<unknown>,
    ): T;

    /**
     * useMemo is a React Hook that lets you cache the result of a calculation
     * between re-renders.
     *
     * @see https://react.dev/reference/react/useMemo
     */
    declare hook useMemo<T>(
      create: () => T,
      inputs: ?ReadonlyArray<unknown>,
    ): T;

    /**
     * `useImperativeHandle` is a React Hook that lets you customize the handle
     * exposed as a ref.
     *
     * @see https://react.dev/reference/react/useImperativeHandle
     */
    declare hook useImperativeHandle<T>(
      ref: React.RefSetter<T> | null | void,
      create: () => T,
      inputs: ?ReadonlyArray<unknown>,
    ): void;

    /**
     * `useDeferredValue` is a React Hook that lets you defer updating
     * a part of the UI.
     *
     * @see https://react.dev/reference/react/useDeferredValue
     */
    declare hook useDeferredValue<T>(value: T, initialValue?: T): T;

    /**
     * `useTransition` is a React Hook that lets you update the state
     * without blocking the UI.
     *
     * @see https://react.dev/reference/react/useTransition
     */
    declare hook useTransition(): [boolean, (() => void | Promise<void>) => void];

    /**
     * `startTransition` lets you update the state without blocking the UI.
     *
     * @see https://react.dev/reference/react/startTransition
     */
    declare function startTransition(callback: () => void | Promise<void>): void;

    /**
     * `useId` is a React Hook for generating unique IDs that
     * can be passed to accessibility attributes.
     *
     * @see https://react.dev/reference/react/useId
     */
    declare hook useId(): string;

    /**
     * `useInsertionEffect` allows inserting elements into the DOM
     * before any layout effects fire.
     *
     * `useInsertionEffect` is for CSS-in-JS library authors. Unless you are
     * working on a CSS-in-JS library and need a place to inject the styles,
     * you probably want `useEffect` or `useLayoutEffect` instead.
     *
     * @see https://react.dev/reference/react/useInsertionEffect
     */
    declare hook useInsertionEffect(
      create: () => MaybeCleanUpFn,
      inputs?: ?ReadonlyArray<unknown>,
    ): void;

    /**
     * `useSyncExternalStore` is a React Hook that lets you subscribe
     * to an external store.
     *
     * @see https://react.dev/reference/react/useSyncExternalStore
     */
    declare hook useSyncExternalStore<Snapshot>(
      subscribe: (onStoreChange: () => void) => () => void,
      getSnapshot: () => Snapshot,
      getServerSnapshot?: () => Snapshot,
    ): Snapshot;

    /**
     * `useOptimistic` is a React Hook that lets you optimistically update
     * the UI.
     *
     * @see https://react.dev/reference/react/useOptimistic
     */
    declare hook useOptimistic<State>(
      passthrough: State,
    ): [State, (action: State | ((pendingState: State) => State)) => void];
    declare hook useOptimistic<State, Action>(
      passthrough: State,
      reducer: (state: State, action: Action) => State,
    ): [State, (action: Action) => void];

    /**
     * `useActionState` is a Hook that allows you to update state based on the
     * result of a form action.
     *
     * @see https://react.dev/reference/react/useActionState
     */
    declare hook useActionState<State>(
      action: (state: Awaited<State>) => State | Promise<State>,
      initialState: Awaited<State>,
      permalink?: string,
    ): [state: Awaited<State>, dispatch: () => void, isPending: boolean];
    declare hook useActionState<State, Payload>(
      action: (state: Awaited<State>, payload: Payload) => State | Promise<State>,
      initialState: Awaited<State>,
      permalink?: string,
    ): [
      state: Awaited<State>,
      dispatch: (payload: Payload) => void,
      isPending: boolean,
    ];

    type ComponentType<in P extends {...}> = component(...P);
    type PropsOf<C extends string | MixedElement | React$RendersExactly<React$ElementType>> = globalThis.React.PropsOf<C>;
    type PropOf<C extends string | MixedElement | React$RendersExactly<React$ElementType>, K extends string> = globalThis.React.PropOf<C, K>;
    type RefOf<C extends string | MixedElement | React$RendersExactly<React$ElementType>> = globalThis.React.RefOf<C>;
    type MixedElement = globalThis.React.MixedElement;
    type ElementType = React$ElementType;

    type Key = React$Key;
    type RefSetter<in T> = globalThis.React.RefSetter<T>;
    type Node = React$Node;
    type Context<T> = globalThis.React.Context<T>;
    type Portal = React$Portal;
    type ElementRef<C> = globalThis.React.ElementRef<C>;

    type ChildrenArray<out T> = ReadonlyArray<ChildrenArray<T>> | T;
    type ReactSetStateFunction<S> = ((S => S) | S) => void;
    type RefObject<T> = React$RefObject<T>;
    type Interaction = {
      name: string,
      timestamp: number,
      ...
    };

    declare namespace JSX {
      type Element = ReactElement<any, any>;
      type IntrinsicElements = globalThis.React.JSX.IntrinsicElements;
    }

    // TS compat: core types
    /**
     * @deprecated TypeScript compatibility alias. Use `React.Node` instead.
     */
    type ReactNode = Node;
    /**
     * Represents a JSX element.
     */
    type ReactElement<out P = unknown, out T = React.ElementType> = ExactReactElement_DEPRECATED<T, P>;
    /**
     * @deprecated TypeScript compatibility alias. Use `React.Portal` instead.
     */
    type ReactPortal = Portal;
    /**
     * @deprecated TypeScript compatibility alias. Use `string | number` instead.
     */
    type ReactText = string | number;
    /**
     * @deprecated TypeScript compatibility alias.
     */
    type ReactInstance = any;
    /**
     * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
     */
    type JSXElementConstructor<in P extends {...} = {...}> = component(...P);

    // TS compat: component types
    /**
     * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
     */
    type FC<in P extends {...} = {...}> = component(...P);
    /**
     * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
     */
    type FunctionComponent<in P extends {...} = {...}> = component(...P);
    /**
     * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
     */
    type ComponentClass<in P extends {...} = {...}, S = any> = component(...P);
    /**
     * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
     */
    type ForwardRefExoticComponent<in P extends {...} = {...}> = component(...P);
    /**
     * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
     */
    type NamedExoticComponent<in P extends {...} = {...}> = component(...P);
    /**
     * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
     */
    type ExoticComponent<in P extends {...} = {...}> = component(...P);
    /**
     * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
     */
    type MemoExoticComponent<T extends ComponentType<any>> = component(...ComponentProps<T>);
    /**
     * @deprecated TypeScript compatibility alias. Use `React.ComponentType` instead.
     */
    type LazyExoticComponent<T extends ComponentType<any>> = component(...ComponentProps<T>);
    /**
     * The props accepted by a component or JSX intrinsic, including its ref prop.
     */
    type ComponentProps<T extends ElementType> = globalThis.React.ComponentProps<T>;
    /**
     * @deprecated TypeScript compatibility alias. Use `React.ComponentProps` instead.
     */
    type ComponentPropsWithRef<T extends ElementType> = ComponentProps<T>;
    /**
     * @deprecated TypeScript compatibility alias. Use `Omit<React.ComponentProps<T>, 'ref'>` instead.
     */
    type ComponentPropsWithoutRef<T extends ElementType> = Omit<ComponentProps<T>, 'ref'>;
    /**
     * @deprecated TypeScript compatibility alias.
     */
    type PropsWithChildren<out P = {...}> = P & {children?: Node, ...};
    /**
     * @deprecated TypeScript compatibility alias.
     */
    type PropsWithoutRef<P> = Omit<P, 'ref'>;

    // TS compat: ref & attribute types
    /**
     * @deprecated TypeScript compatibility alias. Use `React.RefSetter` instead.
     */
    type Ref<in T> = RefSetter<T>;
    /**
     * @deprecated TypeScript compatibility alias. Use `React.RefSetter` instead.
     */
    type RefCallback<in T> = (T | null) => void | (() => void);
    /**
     * @deprecated TypeScript compatibility alias.
     */
    type RefAttributes<T> = { ref?: RefSetter<T>, ...};
    /**
     * @deprecated TypeScript compatibility alias.
     */
    type ClassAttributes<T> = RefAttributes<T>;
    /**
     * @deprecated TypeScript compatibility alias.
     */
    type Attributes = { key?: Key, ...};
    /**
     * @deprecated TypeScript compatibility alias. Use `React.RefObject` instead.
     */
    type MutableRefObject<T> = { current: T, ... };
    /**
     * @deprecated TypeScript compatibility alias.
     */
    type ForwardRefRenderFunction<out T, in P = {...}> = (props: P, ref: RefSetter<T>) => Node;

    // TS compat: context types
    /**
     * @deprecated TypeScript compatibility alias. Prefer `React.Context<T>` for the context
     * object; this type represents only the provider component.
     */
    type Provider<in T> = component(value: T, children?: Node);
    /**
     * @deprecated TypeScript compatibility alias. Prefer `React.Context<T>` for the context
     * object; this type represents only the consumer component.
     */
    type Consumer<out T> = component(children: (value: T) => Node);
    /**
     * @deprecated TypeScript compatibility alias.
     */
    type ProviderProps<T> = {value: T, children?: Node, ...};

    // TS compat: hook/misc types
    /**
     * @deprecated TypeScript compatibility alias.
     */
    type Dispatch<in A> = (A) => void;
    /**
     * @deprecated TypeScript compatibility alias.
     */
    type SetStateAction<S> = S | ((S) => S);
    /**
     * @deprecated TypeScript compatibility alias.
     */
    type DependencyList = ReadonlyArray<unknown>;
    /**
     * @deprecated TypeScript compatibility alias.
     */
    type EffectCallback = () => void | (() => void);
    /**
     * @deprecated TypeScript compatibility alias.
     */
    type ErrorInfo = {componentStack?: string | null, ...};
    /**
     * @deprecated TypeScript compatibility alias.
     */
    type ValidationMap<T> = {[key: string]: any, ...};
    /**
     * @deprecated TypeScript compatibility alias.
     */
    type WeakValidationMap<T> = {[key: string]: any, ...};
    /**
     * @deprecated TypeScript compatibility alias.
     */
    type TransitionStartFunction = (callback: () => void | Promise<void>) => void;
    /**
     * @deprecated TypeScript compatibility alias.
     */
    type ActionDispatch<in ActionArg extends ReadonlyArray<any> = []> = (...args: ActionArg) => void;
  };
  declare export default typeof React;

  declare export namespace JSX {
    type Element = React.JSX.Element;
    type IntrinsicElements = globalThis.React.JSX.IntrinsicElements;
  }
}

// We only keep a bare-bone jsx intrinsics definition. Everything else is moved to flow-typed
declare type $JSXIntrinsics = {
  [key: string]: {
    instance: any,
    props: any,
    ...
  },
};

// This is really shameful. `tslib_patches/react-dom.diff` forks @types/react so
// that its user-visible string attributes (`aria-label`, `title`, `alt`,
// `placeholder`, `label`, ...) use this type instead of `string`, and libraries
// may redeclare it to accept non-strings such as Fbt objects. It should not
// exist.
// TODO(T295956711): delete it and the fork.
declare type ReactDOM$UserVisibleString = string;
