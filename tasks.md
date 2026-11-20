
4 matches
#1937 Enhancement: add a get_frozen_players_count view
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Enhancement
Priority: Low
Estimated Time: 15 minutes

Description:
Counting frozen players currently requires pulling the full frozen list. A get_frozen_players_count view would return the count without the budget cost of the list.

Tasks:

Add get_frozen_players_count() returning the count
Add a test asserting the count matches the paginated list length

#1938 Enhancement: emit an event when set_maximum_stake changes
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Enhancement
Priority: Low
Estimated Time: 20 minutes

Description:
set_maximum_stake changes config without emitting an event, so indexers cannot track stake-cap drift without polling get_protocol_config.

Tasks:

Emit an event with the old and new maximum stake
Add a test asserting the event is emitted

#1939 Enhancement: add a get_blacklist_reason view returning the reason for a blacklisted token
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Enhancement
Priority: Low
Estimated Time: 20 minutes

Description:
add_token_to_blacklist stores a reason string, but there is no view to read it back; clients can't show why a token is blacklisted.

Tasks:

Add get_blacklist_reason(token) returning the stored reason
Add a test asserting the reason round-trips

#1940 Enhancement: add a get_fee_tier_for_stake view returning the matched tier
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Enhancement
Priority: Low
Estimated Time: 20 minutes

Description:
calculate_fee_by_tier returns the fee but not the matched tier; a get_fee_tier_for_stake view would let UIs show which tier applies before deposit.

Tasks:

Add get_fee_tier_for_stake(stake) returning the matched FeeTier
Add a test asserting the returned tier matches the fee calculation
