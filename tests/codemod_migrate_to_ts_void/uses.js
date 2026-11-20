import type {Props} from './types';
import {get, log} from './types';

function label(props: Props): ?string {
  return props.label;
}

function run(): void { // read into `undefined` below: changed, then `get` is
  return get();
}
const result: undefined = run();

function noop() {} // read into `?string` below: annotated
const s: ?string = noop();

function keep(x: void): void { // kept
  log('');
}
keep(undefined);

import type {Options} from './types';
import {exportedNoop, maybeName} from './types';

function force(options: Options): ?{force: boolean} {
  return options;
}
const name: ?string = maybeName();
const exported: ?string = exportedNoop();

function maybeOne(): number | void { // read into `?number` below: changed
  return 1;
}
const one: ?number = maybeOne();

import type {Callback} from './types';
import {onRequest, onResponse} from './types';

function handle(response: string): void {} // kept
onResponse(handle);
onRequest(handle);
const callback: Callback = (): void => {}; // kept

import type {Form} from './types';

const form: Form = {getError: (): void => {}}; // changed once `Form` no longer admits `void`
const error: ?string = form.getError();

function anything(): unknown {} // returns `undefined`
function nothing(): undefined {} // kept
async function load(): Promise<?string> {} // returns `undefined`
function noValue(): string | void {} // read into `?string` below: changed, then returns `undefined`
const value: ?string = noValue();

import {mapMaybe} from './types';

const skip = (item: number): void => {}; // changed, since `mapMaybe` keeps its return type
const kept: Array<string> = mapMaybe([1], skip);

import type {Confirm} from './types';

declare const confirm: Confirm;
const confirmed: undefined = confirm(); // still an error
