import * as React from 'react';

// A spread of a type parameter beside other props, against an object: the type parameter is bounded
// by what the object has besides the other props. Once the type parameter has a lower bound from
// the other arguments, the spread meets the object, so the other props meet the object's own, as
// when the flow-based solve flows the spread into it. `TOnClose` is the dialog's `onClose`, not its
// default.
type TOnCloseType = (...args: ReadonlyArray<empty>) => void;
type DialogProps<TOnClose extends TOnCloseType> = Readonly<{onClose: TOnClose}>;
type TDialogComponentProps<
  out TDialogProps extends {...},
  out TOnClose extends TOnCloseType = TOnCloseType,
> = Readonly<{...$Exact<TDialogProps>, ...DialogProps<TOnClose>}>;
type TDialog<
  in TDialogProps extends {...},
  in TOnClose extends TOnCloseType = TOnCloseType,
> = React.ComponentType<TDialogComponentProps<TDialogProps, TOnClose>>;

type EntryPointProps<Q, E = {}, R = {}> = Readonly<{entryPoints: E, props: R, queries: Q}>;
type EntryPointComponent<Q, E = {}, R = {}> = component(...EntryPointProps<Q, E, R>);
type EntryPoint<in P, out C> = Readonly<{getPreloadProps: (params: P) => void, root: C}>;
type DialogEntryPoint<TQueries, TOtherProps extends {...}, TOnClose extends TOnCloseType> = EntryPoint<
  {},
  EntryPointComponent<TQueries, {}, Readonly<React.ComponentProps<TDialog<TOtherProps, TOnClose>>>>,
>;

declare component Trigger<
  TQueries extends {...},
  TOtherProps extends {...},
  TOnClose extends TOnCloseType = TOnCloseType,
>(
  dialogEntryPoint: DialogEntryPoint<TQueries, TOtherProps, TOnClose>,
  otherProps: TOtherProps,
);

type OtherProps = Readonly<{logger?: string, onClose: (v: ?string) => void}>;
declare component Dialog(...props: EntryPointProps<Readonly<{q: number}>, {}, OtherProps>);
declare const ep: EntryPoint<{}, typeof Dialog>;

<Trigger dialogEntryPoint={ep} otherProps={{logger: 'x'}} />; // ok

// Also when the props are a rest parameter, whose object meets the spread before `otherProps`.
type TriggerProps<
  TQueries extends {...},
  TOtherProps extends {...},
  TOnClose extends TOnCloseType = TOnCloseType,
> = Readonly<{
  dialogEntryPoint: DialogEntryPoint<TQueries, TOtherProps, TOnClose>,
  onHide?: TOnClose,
  otherProps: TOtherProps,
}>;
declare component RestTrigger<
  TQueries extends {...},
  TOtherProps extends {...},
  TOnClose extends TOnCloseType = TOnCloseType,
>(...props: TriggerProps<TQueries, TOtherProps, TOnClose>);

<RestTrigger dialogEntryPoint={ep} otherProps={{logger: 'x'}} />; // ok

// Without a lower bound for the type parameter, the flow-based solve never evaluates the spread, so
// an optional `onClose` does not make `TOnClose` `void | (() => void)`, which its bound rejects.
declare hook useDialog<
  TQueries extends {...},
  TOtherProps extends {...},
  TOnClose extends TOnCloseType = TOnCloseType,
>(dialogEntryPoint: DialogEntryPoint<TQueries, TOtherProps, TOnClose>): (props: TOtherProps) => void;

type OptionalOtherProps = Readonly<{logger?: string, onClose?: () => void}>;
declare component OptionalDialog(
  ...props: EntryPointProps<Readonly<{q: number}>, {}, OptionalOtherProps>
);
declare const optionalEp: EntryPoint<{}, typeof OptionalDialog>;

component C() {
  const show = useDialog(optionalEp); // ok
  return null;
}
