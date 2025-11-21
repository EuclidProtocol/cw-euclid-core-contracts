# Title
Migrating Exclusively to Vouchers

## Status
Proposed

## Context
Having both Native and Vouchers in the system creates confusion for users. It makes keeping track of balances more difficult, steepens the learning curve for using our system, hampers performance and overall makes the whole system more complex.

## Decision
Use Vouchers exclusively. Users will need to have a voucher balance in our system to use it, and our system will only operate with Vouchers.

## Consequences
Will require a significant refactoring of our system, which will delay audits and our launch date.
Our system would require more trust from users since they'd have to convert their assets to our Vouchers from the beginning.
Makes security an even bigger concern because all the user funds will be custodied by our system.

Our system's performance will greatly improve.
The system will become easier to use and less complex: Balances will become clear since they'll no longer be separated by vouchers and non-vouchers.

