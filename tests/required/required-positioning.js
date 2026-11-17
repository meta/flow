export type BaseType = boolean | number; 

type RecursiveProp =
  | BaseType
  | Required<{[key: string]: RecursiveProp}>
  | ReadonlyArray<RecursiveProp>;

type Props = Required<{[key: string]: RecursiveProp}>;

type Provider<T> = () => (value: T) => null;

declare export function register<T extends Props>(
 x: Provider<T>,
): void;


// All errors should be positioned here, not at any definitions
register(() => (x: {foo: number}) => null); // ERROR!
