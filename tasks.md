#1941 Enhancement: add a get_total_escrowed global view
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Enhancement
Priority: Medium
Estimated Time: 1 hour

Description:
There is no global view of the total funds currently locked across all matches. A get_total_escrowed view would help dashboards and risk monitoring.

Tasks:

Add get_total_escrowed() returning the sum of currently-locked funds
Add a test asserting the total matches the sum of per-match escrow balances

#1942 Enhancement: add a get_match_dispute view
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Enhancement
Priority: Low
Estimated Time: 20 minutes

Description:
There is no view returning the dispute (if any) for a given match_id; clients must list all disputes and filter. A get_match_dispute view would simplify dispute-aware UIs.

Tasks:

Add get_match_dispute(match_id) returning the open dispute or none
Add a test asserting the view returns the expected dispute

#1943 Enhancement: emit match/paused and match/resumed events
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Enhancement
Priority: Low
Estimated Time: 20 minutes

Description:
The match lifecycle is not fully indexable because pause/resume transitions emit no events. Adding match/paused and match/resumed events would close the gap.

Tasks:

Emit the events on pause_match/resume_match
Update docs/architecture.md events table
Add tests asserting the events are emitted

#1944 Enhancement: add a get_oracle_rotation_history view
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Enhancement
Priority: Medium
Estimated Time: 1 hour

Description:
Oracle rotations are applied without a queryable history, so operators cannot audit past temporary rotations. A get_oracle_rotation_history view would expose past rotations.

Tasks:

Add get_oracle_rotation_history() returning past rotation entries
Add a test asserting the view returns recorded rotations
