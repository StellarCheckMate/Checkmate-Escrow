#1909 Security: create_match does not reject a players vector with duplicate addresses
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Security
Priority: Low
Estimated Time: 20 minutes

Description:
create_match accepts a players vector and does not check for duplicate addresses, allowing a player to occupy both sides of a match (self-play).

Tasks:

Reject matches where the two players are the same address
Add a test asserting duplicate players are rejected

#1910 Security: dispute_oracle_result does not bound the number of disputes per match
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Security
Priority: Medium
Estimated Time: 1 hour

Description:
dispute_oracle_result can be called repeatedly for the same match with no cap on open disputes, enabling a caller to grief the oracle and burn dispute-period budget.

Tasks:

Enforce at most one open dispute per match (or a small bounded count)
Add a test asserting a second open dispute is rejected

#1911 Security: vote_on_dispute does not exclude the disputed oracle from voting
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Security
Priority: Low
Estimated Time: 20 minutes

Description:
vote_on_dispute does not prevent the oracle whose result is being disputed from casting a vote on its own outcome, biasing the resolution.

Tasks:

Exclude the disputed oracle/operator from voting on their own result
Add a test asserting the disputed oracle cannot vote

#1912 Security: dispute_and_rollback_match does not emit an event naming the rollback initiator
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Security
Priority: Low
Estimated Time: 15 minutes

Description:
dispute_and_rollback_match rolls back a match without emitting an event naming who initiated the rollback, so rollbacks are not attributable for audit.

Tasks:

Emit a match/rollback event naming the caller
Add a test asserting the event is emitted with the caller