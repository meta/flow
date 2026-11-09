// An intersection of objects meets an object type through the first member that fits it, as in
// speculation: the first member fails on `newProps`, which is invariant and may be `null`, so `C`
// is the `Component` of the second member.
import type {Component} from 'react';

type AnimateParams<P, C extends Component<P, any>> = {target: C, newProps: ?P};
type AnimateConfig<P, C extends Component<P, any>> = {
  animate?: ?(v: AnimateParams<P, C>) => unknown,
  name: string,
};
declare function delayStart<P, C extends Component<P>, Cfg extends AnimateConfig<P, C>>(
  c: Array<Cfg> | Cfg,
  d: number,
): Array<Cfg>;
declare var config: {
  animate: (p: {target: {setState: () => void}, newProps: {value: number}}) => unknown,
  name: string,
} & AnimateConfig<{value: number}, Component<{value: number}>>;
delayStart(config, 200); // ok
