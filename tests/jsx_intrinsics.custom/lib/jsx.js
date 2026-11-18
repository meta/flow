declare namespace React {
  declare namespace JSX {
    interface IntrinsicElements {
      testdiv: {id: string, [key: StringPrefix<'data-'>]: string};
      testspan: {id: string, class: string, ...};
    }
  }
}
