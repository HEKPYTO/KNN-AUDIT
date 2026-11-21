# mini fixture

`data/base.csv` holds 64 vectors of dimension 8. Vector `i` has first
coordinate `i` and seven zero coordinates, so row index and id are the same.

`data/mini/mini_00.json` through `mini_07.json` query `c + 0.25` for `c` from
10 to 17. The hand-checked top-10 is `c, c+1, c-1, c+2, c-2, ...`, with exact
quarter-step distances and no ties.
