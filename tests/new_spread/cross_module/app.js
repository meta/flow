import fn from './fn';

const app = fn((o: {a: number, b: number, ...}) => {});
app({a: 'foo', b: 2});

export default fn((o: {a: number, b: number, ...}) => {}) as (x: {a:number, b:number}) => void;
