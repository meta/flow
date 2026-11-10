declare const React: any;
declare function Render(): null;

class StringKeyAssignment {
  value: string;
  constructor() {
    this['value'] = 'hi';
  }
}

class StringKeyRead {
  value: string;
  constructor() {
    this['value']; // error
    this.value = 'hi';
  }
}

class JsxTagRead {
  Render: typeof Render;
  constructor() {
    <this.Render />; // error
    this.Render = Render;
  }
}

class LiteralNames {
  ['text']: string; // error
  [0]: number; // error
  ['assigned']: string;
  [1]: number;
  constructor() {
    this.assigned = 'a';
    this[1] = 1;
  }
}

class LiteralNameReadEarly {
  ['value']: number;
  constructor() {
    this['value']; // error
    this.value = 1;
  }
}

class LiteralNameInitializerOrder {
  a: number = this.later; // error
  ['later']: number = 1;
}

class NonIntegerNumberNames {
  [1.5]: number; // error
  [9007199254740993]: number; // error
  [2.5]: number;
  [1e21]: number;
  constructor() {
    this[2.5] = 1;
    this[1e21] = 1;
  }
}

class NonIntegerNumberNameReadEarly {
  [0.5]: number;
  constructor() {
    this[0.5].toFixed(); // error
    this[0.5] = 1;
  }
}
