// Pure TS: object types with an index signature that applies to some keys only.

export type DataAttributes = {[key: `data-${string}`]: string | undefined};

export type LabeledData = DataAttributes & {label: string};

export interface BaseProps {
  [key: `data-${string}`]: string | undefined;
  xstyle?: unknown;
}

export interface ButtonProps extends BaseProps {
  label: string;
}
