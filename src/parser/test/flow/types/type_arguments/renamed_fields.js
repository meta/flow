type Ålias<T> = NS.Box</* first */ T, Foo<string>>;
type Bare = Plain;
interface I<T> extends NS.Base<T>, Plain {}
class C<T> implements I<T>, Plain {}
declare interface Ambient<T> extends I<T>, Plain {}
declare class Decl<T> extends Base<T> mixins M<T> implements I<T> {}
type Imported<T> = import("module").Box<T>;
declare class Mixed<T> extends M<T>(Base<T>) {}
const inferred = f<_>();
