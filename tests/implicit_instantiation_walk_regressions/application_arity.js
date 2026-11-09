// Applications of the same alias are compared argument by argument only when they write as many
// arguments, as in subtyping: `Strategy<>` is `Strategy<Base>`, whose `selectedEntry` is read-only,
// so `T` is only below `Base`, and `T` is `Entry`, from the resolver.
interface Base {
  readonly key: string;
}
type Entry = {readonly key: string, readonly raw: number};
type Strategy<T extends Base = Base> = (v: {readonly selectedEntry: T}) => void;
declare function choose<T extends Base>(c: Strategy<T>, r: () => T): T;
declare const strategy: Strategy<>;
declare const resolver: () => Entry;
choose(strategy, resolver) as Entry; // ok
