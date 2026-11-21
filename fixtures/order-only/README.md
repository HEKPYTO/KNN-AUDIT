# order-only fixture

`data/base.csv` holds three vectors of dimension 2: `[0,0]`, `[1,0]`, `[2,0]`.

`data/order/order_only.json` queries `[0,0]` with `gt_ids` `[1,0,2]`. The exact
answer by distance is `[0,1,2]`, so the id sets match and only two rank swaps
are present. The CLI reports the swaps but does not fail with
`--fail-on-mismatch`; use `--k 3` when running this fixture.
