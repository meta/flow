// @flow

// Test well-known symbol [Symbol.iterator] in class methods

// Basic class with [Symbol.iterator] method
class MyIterable {
  *[Symbol.iterator](): IteratorObject<number, undefined> {
    yield 1;
    yield 2;
    yield 3;
  }
}

// Should work - MyIterable implements Iterable<number>
const iterable: Iterable<number> = new MyIterable();

// Should work - can use for...of
for (const x of new MyIterable()) {
  x as number;
}

// Test [Symbol.asyncIterator]
class MyAsyncIterable {
  async *[Symbol.asyncIterator](): AsyncGenerator<string, void, void> {
    yield "a";
    yield "b";
  }
}

// Should work - MyAsyncIterable implements AsyncIterable<string>
const asyncIterable: AsyncIterable<string> = new MyAsyncIterable();

// Error case - wrong type parameter
class BadIterable {
  *[Symbol.iterator](): IteratorObject<string, undefined> {
    yield "a";
  }
}
const bad: Iterable<number> = new BadIterable(); // Error: string vs number

// Test [Symbol.dispose] (for using statement support, not fully implemented yet)
class MyDisposable {
  [Symbol.dispose](): void {
    // cleanup
  }
}

// Test [Symbol.asyncDispose]
class MyAsyncDisposable {
  async [Symbol.asyncDispose](): Promise<void> {
    // async cleanup
  }
}
