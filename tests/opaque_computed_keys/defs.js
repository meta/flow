export opaque type Op: string = 'special';

export const op: Op = 'special';

const obj = {[op]: 'abc'};

obj as Readonly<{[key: Op]: string}>;
obj as {[key: Op]: string};

obj as {[key: string]: string};
obj as {['special']: string};

obj as {['other']: string}; // error: 'other' missing, 'special' extra
