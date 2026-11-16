// A spread tuple `[...A, ...B]` that waits for `A` from a rest argument is walked before the return
// hint, so that the hint meets the evidence it gives `B`: against a union of callbacks, `bind`'s
// second overload takes `B` from the parameters of the bound method, which only the second member
// fits.
type SubscriptionCallback = (...args: Array<unknown>) => unknown;
type Payload = {selected: {text: string, ...}, ...};
declare function subscribe(
  callback: SubscriptionCallback | ((channel: unknown, payload: Payload) => unknown),
): void;

class C {
  selectTier(_channel: unknown, payload: Payload): void {}
  m(): void {
    subscribe(this.selectTier.bind(this)); // ok
  }
}
