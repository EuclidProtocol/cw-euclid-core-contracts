# Single-sided liquidity — issue breakdown

Vertical-slice issues derived from `PRD-single-sided-liquidity.md`. Each issue is a tracer-bullet cutting through every layer (shared types, IBC schema, factory + ack, router + reply chain, unit tests, integration test).

| # | Title | Type | Blocked by |
|---|---|---|---|
| 1 | [Native tracer bullet](ISSUE-01-single-sided-native-tracer-bullet.md) | AFK | None |
| 2 | [Partner fee support](ISSUE-02-single-sided-partner-fee.md) | AFK | Issue 1 |
| 3 | [Smart (CW20) asset_in support](ISSUE-03-single-sided-smart-cw20.md) | AFK | Issue 1 |
| 4 | [Integration: stable pool + ack-failure refund](ISSUE-04-single-sided-integration-stable-and-failure.md) | AFK | Issue 1 (gains coverage from 2 and 3) |

Issues 2 and 3 are sibling extensions of Issue 1 and can run in parallel. Issue 4 strengthens coverage but is technically only blocked on Issue 1.
