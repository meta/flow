declare opaque type CommonInstance;
declare opaque type DivInstance: CommonInstance;
declare opaque type SpanInstance: CommonInstance;
declare opaque type ImgInstance: CommonInstance;

type React$JSXIntrinsicElements = {
  [string]: {[string]: any, ref?: React.RefSetter<CommonInstance>},
  div: {ref?: React.RefSetter<DivInstance>, ...},
  span: {ref?: React.RefSetter<SpanInstance>, ...},
  img: {ref?: React.RefSetter<ImgInstance>, ...},
}
