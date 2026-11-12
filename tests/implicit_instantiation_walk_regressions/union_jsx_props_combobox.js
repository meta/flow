export type IconProps =
  | Readonly<{icon?: ?string, iconType?: empty}>
  | Readonly<{icon?: Readonly<{src: string}>, iconType: 'pic'}>;
export type OptionProps<T> = Readonly<{label: string, value: T}>;
export type Props<TValue> = Readonly<{
  ...IconProps,
  label: string,
  onValueChange: (newValue: TValue) => void,
  options: ReadonlyArray<OptionProps<TValue>>,
  value?: ?TValue,
}>;
declare export default component Combobox<TValue>(...props: Props<TValue>);
