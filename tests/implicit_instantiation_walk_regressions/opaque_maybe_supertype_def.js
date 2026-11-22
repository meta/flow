export opaque type MaybeObj: ?{readonly foo: string} = ?{readonly foo: string};
export opaque type MaybeStr super 'a' | null extends ?string = ?string;
type Maybe<X> = ?X;
export opaque type MaybeAlias super Maybe<'a'> extends ?string = ?string;
