type Model = {lineCount(): number, lineLength(n: number): number, edit(): void};
type PairModel = {left: Model, right: Model};
type Base = {model(): Model | PairModel | null, focus(): void};
type Single = {model(): Model | null, hasFocus(): boolean} & Base;
type Full = {addCommand(): void} & Single & {update(): void};
interface Lines {
  lineCount(): number;
  lineLength(n: number): number;
}
type HasModel<TModel extends Lines> = Readonly<{model: () => ?TModel, ...}>;

declare function check<TModel extends Lines>(ref: Readonly<{current: ?HasModel<TModel>}>): void;
declare const ref: {current: ?Full};

check(ref);
