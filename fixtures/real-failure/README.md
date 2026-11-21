# real-failure fixture

`pgvector-signature.json` pins `failures[0]` from the committed
`results/scorecards/pgvector-0.8.7.json` scorecard. It holds only the failure
signature fields. No query vectors or dataset rows are included.

Regenerate it only when the scorecard is intentionally re-accepted.
