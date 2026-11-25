import type {DerivedProps, Props} from './lib';

declare const props: Props;

props.onFocus as string; // tsc: error, `(e: string) => void` is not assignable to `string`
({onFocus: (e: string) => {}}) as Props; // tsc: ok
({onFocus: (e: string) => {}, onBlur: (e: string) => {}}) as DerivedProps; // tsc: ok
