#1945 Enhancement: add a get_referral_volume view
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Enhancement
Priority: Low
Estimated Time: 20 minutes

Description:
There is no view of the total stake attributed to a referrer; referral analytics require event replay. A get_referral_volume view would expose the aggregate.

Tasks:

Add get_referral_volume(referrer) returning total attributed stake
Add a test asserting the volume matches referral events

#1946 Enhancement: add a get_pending_results view listing matches awaiting oracle submission
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Enhancement
Priority: Medium
Estimated Time: 1 hour

Description:
There is no view listing matches awaiting oracle result submission; monitoring requires event replay. A get_pending_results view would let dashboards surface unsettled matches.

Tasks:

Add get_pending_results(offset, limit) returning matches awaiting submission
Add a test asserting pagination bounds are respected

#1947 Test: propose_admin rejects proposing the zero address as the next admin
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Testing
Priority: Low
Estimated Time: 15 minutes

Description:
propose_admin does not have a test asserting it rejects the zero address as the next admin, so the guard (once added) is unverified.

Tasks:

Add a test asserting propose_admin rejects the zero address
Add a test asserting a valid address is accepted

#1948 Test: cancel_admin_proposal clears the pending nominee so a stale nominee cannot accept
Repo Avatar
StellarCheckMate/Checkmate-Escrow
Category: Escrow Contract - Testing
Priority: Low
Estimated Time: 15 minutes

Description:
cancel_admin_proposal clearing the pending nominee is not covered by a test, so the stale-nominee guard is unverified.

Tasks:

Add a test asserting accept_admin fails after cancel_admin_proposal
Add a test asserting a fresh propose_admin works after cancellation

