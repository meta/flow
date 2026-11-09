// An indexed access of an object type whose property is a destructor with a type parameter inside,
// such as Relay's `UseMutationConfig<TMutation>`, which reads `TMutation['response']`: the
// property's type as declared, `Readonly<{...TResponse, extra?: ?number}>`, gives evidence for
// `TResponse`.
type Config<TMutation extends {readonly response: {...}, ...}> = {
  onCompleted?: ?(response: TMutation['response']) => void,
};
declare function useHook<TResponse>(
  commit: (config: Config<{response: Readonly<{...TResponse, extra?: ?number}>}>) => void,
): TResponse;

type Data = {readonly a?: ?string, readonly extra?: ?number};
declare const commit: (config: {onCompleted?: ?(response: Data) => void}) => void;
const r = useHook(commit);
r as {readonly a?: ?string};
r as empty; // error

// The same through a read-only object, as Relay's config types are.
declare function useReadOnlyHook<TResponse>(
  commit: (config: Config<Readonly<{response: Readonly<{...TResponse, extra?: ?number}>}>>) => void,
): TResponse;
const s = useReadOnlyHook(commit);
s as {readonly a?: ?string};
