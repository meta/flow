// tsc accepts.
export interface SymbolLast {
  [k: {} & symbol]: number;
}
export interface SymbolFirst {
  [k: symbol & {}]: number;
}
export type SymbolLastKeys = {[K in keyof SymbolLast | 'g']: number};
export type SymbolFirstKeys = {[K in keyof SymbolFirst | 'g']: number};
export declare const symbolLastKeys: SymbolLastKeys;
export declare const symbolFirstKeys: SymbolFirstKeys;
