export class Emitter<Events extends {[key: string]: Array<unknown>}> {
  emit<E extends keyof Events, A extends Events[E]>(event: E, ...args: A): void {}
}
