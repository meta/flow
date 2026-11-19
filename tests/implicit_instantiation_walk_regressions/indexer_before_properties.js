type Metric = {value: number};
type Path = 'A' | 'B';
type Item = {[key: string]: Metric, path: Path};
declare function filterNulls<K, V>(map: {readonly [key: K]: ?V}): {[key: K]: V};
declare const metrics: {[key: string]: Metric};
declare const path: Path;

// The hint of the spread meets the result indexer first, as in subtyping, and then `path`.
({...filterNulls(metrics), path}) as Item; // was an error
({...filterNulls(metrics), path: 1}) as Item; // error

type Point = {[series: string]: ?number, xSeries: string | null};
declare function mk<V>(cb: () => V): {[key: string]: V, ...};
const point: Point = {...mk(() => null), xSeries: null}; // was an error
