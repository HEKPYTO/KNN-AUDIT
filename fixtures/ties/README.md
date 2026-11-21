# ties fixture

`data/base.csv` holds 12 vectors of dimension 4. Coordinates include the
smallest positive subnormal `2^-149` and `f32::MAX`.

| Case | What it exercises |
|---|---|
| `tie_origin` | Exact ties resolved by smaller id, with subnormal distances below unit distance. |
| `tie_unit` | A near-tie around distance 1: `1 - 2^-148`, exactly 1, and `1 + 2^-148`. |
| `subnormal` | Zero, one, and two subnormal steps are distinct exact distances. |
| `overflow` | Squared distances near `f32::MAX^2` stay ordered, and `MAX^2 + 1` ranks last. |

Ground truth ids were checked with exact rational arithmetic.
