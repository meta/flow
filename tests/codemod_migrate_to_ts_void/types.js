export type Props = {
  label?: string | void, // read into `?string` in uses.js: changed
  onChange: (value: string) => void, // kept
};

declare export function log(message: string): void; // kept
declare export function get(): void; // read through `run` in uses.js: changed
export type Options = void | {force: boolean}; // read into `?{force: boolean}` in uses.js: changed
declare export function maybeName(): string | void; // read into `?string` in uses.js: changed
export function exportedNoop() {} // read into `?string` in uses.js: annotated
declare export function onResponse(handler: (response: string) => ?boolean): void; // rejects the returned `void` of `handle` in uses.js: made to admit `void`
export function onRequest(handler: (request: string) => ?boolean): void {} // rejects the returned `void` of `handle` in uses.js: made to admit `void`
export type Callback = () => boolean | undefined; // rejects the returned `void` of `callback` in uses.js: made to admit `void`
export type Form = {getError: () => ?string}; // rejects the returned `void` of `form.getError` in uses.js, made to admit `void`, which is read as `?string`: dropped again
export function stub(): ?string {} // no `return` statement: returns `undefined`
