// A class that extends a value with a construct signature (`Promise` is a `PromiseConstructor`)
// inherits from what the signature returns.
declare class Request extends Promise<{status: number}> {
  send(body: unknown): this;
}
declare const request: Request;

declare function unwrap<T>(p: Promise<T>): T;
unwrap(request).status as number;
unwrap(request).status as string; // error

async function awaited() {
  (await request.send(1)).status as number;
  (await request).status as string; // error
}
