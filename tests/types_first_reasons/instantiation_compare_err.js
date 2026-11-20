type Conf<T> = {t: T};
class Provider<out C extends Conf<unknown>> {}
type S = Conf<number> | Conf<string>;
export class Stacks extends Provider<S<string>> {} // Err: cannot instantiate
