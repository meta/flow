declare function bar(x: unknown): Promise<void>;
bar(0); // error
// $FlowFixMe[unused-promise]
bar(1); // suppressed
