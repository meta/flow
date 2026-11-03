// .ts library declaring an interface that extends an `Omit` of an object type
// with an index signature, the way Astryx's props interfaces extend `BaseProps`.

type Base = {
  [key: `data-${string}`]: string | undefined;
  className?: string;
  style?: unknown;
};

export interface InputProps extends Omit<Base, "style"> {
  label: string;
}
