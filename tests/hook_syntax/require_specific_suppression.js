hook useH<T>(x: T): [T] { return [x];}
useH as (x: empty) => unknown; // error
// $FlowFixMe[react-rule-hook-incompatible]
useH as (x: empty) => unknown; // suppressed
// $FlowFixMe
useH as (x: empty) => unknown; // error, not suppressed
