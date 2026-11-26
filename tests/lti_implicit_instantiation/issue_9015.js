// @flow

type ResultSet<Row> = {
  readonly rowCount: number,
  readonly rows: Array<Row>,
 ...};

interface Submittable {
  submit: () => void;
}

declare class Client {
  constructor(): void;
  query<R>(config: string): Promise<ResultSet<R>>;
  query<Q extends Submittable>(config: Q): Q;
}

declare const c: Client;

c.query('SELECT 1');
c.query({submit: (): void => undefined});
