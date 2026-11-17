export type BaseType = boolean | number; 

type RecursiveProp =
  | BaseType
  | Partial<{[key: string]: RecursiveProp}>
  | ReadonlyArray<RecursiveProp>;

type Props = Partial<{[key: string]: RecursiveProp}>;

type Provider<T> = () => (x: T) => null;

declare export function register<T extends Props>(
 x: Provider<T>,
): void;


// All errors should be positioned here, not at any definitions
register(() => (x: {foo: number}) => null); // ERROR!
