type Annotations = Partial<{
  string: ?{[key: string]: ?string, ...},
  int: ?{[key: string]: ?number, ...},
}>;


function addAnnotations(
  target: Annotations,
  source: Annotations,
): void {
  Object.keys(source).forEach(annotationTypeName => {
    target[annotationTypeName] = Object.assign( // error
      target[annotationTypeName],
      source[annotationTypeName],
    );
  });
}
