export type Spread<T1, T2> = Omit<T2, keyof T1> & T1;

export type SerializedTextNode = Spread<{text: string}, {type: string}>;
export type SerializedEmojiNode = Spread<{className: string}, SerializedTextNode>;
