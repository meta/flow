function case1() {
  type Connector<WP> = (c: WP) => void;

  declare function connect<D, SP>(m: () => SP): Connector<{...$Exact<SP>, dispatch: D}>;
  declare function connect<SP, DP>(m: () => SP, d: DP): Connector<{...$Exact<SP>, ...$Exact<DP>}>;

  type Props = {a: boolean};

  declare function mapStateToProps(): Props;

  const connector: Connector<Props> = connect(mapStateToProps);
}

function case2() {
  type Map = {a: {x: 1}};

  declare class C {
    get<K extends keyof Map>(k: K): Map[K] | null;
    get<E = {x: 2}>(k: string): E | null;
  }
  declare var o: C;
  declare function click(e: {...}): void;

  click(o.get('a')); // error: `get` may return `null`
}
