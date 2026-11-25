interface Attrs {
  onFocus: (e: string) => void;
}

interface Base extends Attrs {
  [key: `data-${string}`]: string;
}

export type Props = Omit<Base, 'title'>;

interface Derived extends Base {
  onBlur: (e: string) => void;
}

export type DerivedProps = Omit<Derived, 'title'>;
