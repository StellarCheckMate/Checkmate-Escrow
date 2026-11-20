#1921 Fix: get_frozen_players and admin_list_frozen_players overlap; the non-paginated variant should be deprecated
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Bug
Priority: Low
Estimated Time: 20 minutes

Description:
get_frozen_players and admin_list_frozen_players both return frozen players; the non-paginated variant is unsafe at scale and should be deprecated in favor of the paginated one.

Tasks:

Deprecate get_frozen_players, delegating to the paginated accessor
Add a deprecation note to the doc comment


#1922 Fix: pause_match and resume_match do not emit events
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Bug
Priority: Low
Estimated Time: 20 minutes

Description:
pause_match and resume_match change match state without emitting events, so off-chain indexers cannot track pause/resume transitions.

Tasks:

Emit a match/paused and match/resumed event
Add tests asserting the events are emitted

#1923 Fix: get_global_platform_stats recomputes aggregates from per-platform stats on every call
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Bug
Priority: Medium
Estimated Time: 1 hour

Description:
get_global_platform_stats recomputes the aggregate from per-platform stats on every invocation instead of maintaining a running total, costing budget on each call.

Tasks:

Maintain a running global aggregate updated on match lifecycle transitions
Add a test asserting the aggregate matches the sum of per-platform stats


#1924 Fix: calculate_fee_by_tier errors for a stake below the lowest tier instead of defaulting
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Bug
Priority: Low
Estimated Time: 20 minutes

Description:
calculate_fee_by_tier returns an error when the stake is below the lowest tier's lower bound instead of defaulting to the lowest tier's fee, breaking small-stake matches.

Tasks:

Default to the lowest tier's fee for stakes below its lower bound
Add a test asserting a small stake resolves to the lowest tier