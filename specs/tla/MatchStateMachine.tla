--------------------------- MODULE MatchStateMachine ---------------------------
(***************************************************************************)
(* Formal specification of the Checkmate-Escrow match state machine.       *)
(*                                                                        *)
(* Scope                                                                  *)
(*   This module models the *escrow lifecycle* of a match: creation,       *)
(*   deposits, activation, result submission, the dispute window,          *)
(*   finalisation, pausing, cancellation and expiry — together with the    *)
(*   escrowed balance and who is entitled to it.                          *)
(*                                                                        *)
(*   Source of truth: contracts/escrow/src/lib.rs                          *)
(*     create_match      -> CreateMatch                                    *)
(*     deposit           -> Deposit                                        *)
(*     submit_result     -> SubmitResult                                   *)
(*     claim_vested_payout -> ReleasePot                                   *)
(*     finalize_match    -> FinalizeMatch                                  *)
(*     resolve_dispute_by_vote -> ResolveDispute                           *)
(*     cancel_match      -> CancelMatch                                    *)
(*     expire_match      -> ExpireMatch                                    *)
(*     pause_match       -> PauseMatch                                     *)
(*     resume_match      -> ResumeMatch                                    *)
(*                                                                        *)
(* Deliberately out of scope (see docs/formal-spec.md, "Non-goals")        *)
(*   - Authorisation. Every action is modelled as callable by *somebody*   *)
(*     with the right to call it; who signs is a property of the Soroban   *)
(*     host, verified separately by the Kani harness.                      *)
(*   - Token arithmetic: multi-token conversion rates, fees and vesting    *)
(*     durations. The pot is modelled as an integer amount of a single     *)
(*     token so that conservation is checkable.                            *)
(*   - Dispute voting mechanics (quorum, bonds, weights). A dispute is     *)
(*     modelled as an action that may return *any* outcome, which is       *)
(*     strictly more permissive than the real vote and therefore sound     *)
(*     for safety.                                                        *)
(***************************************************************************)

EXTENDS Integers, FiniteSets

CONSTANTS
    Matches,        \* Finite set of match identifiers, e.g. {"m1", "m2"}
    Players,        \* The two seats: {"p1", "p2"}
    Stake,          \* Stake escrowed by each player (integer > 0)
    DisputePeriod,  \* Ledgers to wait before finalisation; 0 = immediate
    Timeout,        \* Ledgers after creation at which a pending match expires
    MaxLedger       \* Upper bound on the modelled clock (keeps the model finite)

ASSUME
    /\ IsFiniteSet(Matches)
    /\ Players = {"p1", "p2"}
    /\ Stake \in Nat /\ Stake > 0
    /\ DisputePeriod \in Nat
    /\ Timeout \in Nat /\ Timeout > 0
    /\ MaxLedger \in Nat

--------------------------------------------------------------------------------
(* Domains *)

\* "none" is the pre-creation state: a match id that has not been used yet.
States == {"none", "pending", "active", "pendingResult", "completed",
           "cancelled", "paused"}

Terminal == {"completed", "cancelled"}

Outcomes == {"none", "p1", "p2", "draw"}

Results == {"p1", "p2", "draw"}

(*************************************************************************)
(* Which players are entitled to the *pot* under a given outcome.         *)
(*                                                                       *)
(* A draw is a refund, not a win: nobody takes the pot, each player gets  *)
(* their own stake back.  Modelling it this way is what makes             *)
(* AtMostOneWinner a real property rather than a tautology.               *)
(*************************************************************************)
Entitled(w) ==
    CASE w = "p1"   -> {"p1"}
      [] w = "p2"   -> {"p2"}
      [] OTHER      -> {}

\* Who receives funds when the pot is released under outcome `w`.
Recipients(w, depositors) ==
    IF w = "draw" THEN depositors ELSE Entitled(w)

--------------------------------------------------------------------------------
(* State *)

VARIABLES
    state,          \* [Matches -> States]
    deposits,       \* [Matches -> SUBSET Players]   who has funded
    escrow,         \* [Matches -> Nat]              amount held by the contract
    winner,         \* [Matches -> Outcomes]         recorded outcome
    winners,        \* [Matches -> SUBSET Players]   who won the pot
    pendingWinner,  \* [Matches -> Outcomes]         oracle result awaiting finality
    paid,           \* [Matches -> SUBSET Players]   who has received funds
    released,       \* [Matches -> Nat]              pot-release count
    created,        \* [Matches -> Nat]              ledger at creation
    deadline,       \* [Matches -> Nat]              dispute deadline ledger
    pausedFor,      \* [Matches -> Nat]              ledgers spent paused
    ledger          \* Nat                           the clock

vars == <<state, deposits, escrow, winner, winners, pendingWinner, paid,
          released, created, deadline, pausedFor, ledger>>

TypeOK ==
    /\ state \in [Matches -> States]
    /\ deposits \in [Matches -> SUBSET Players]
    /\ escrow \in [Matches -> Nat]
    /\ winner \in [Matches -> Outcomes]
    /\ winners \in [Matches -> SUBSET Players]
    /\ pendingWinner \in [Matches -> Outcomes]
    /\ paid \in [Matches -> SUBSET Players]
    /\ released \in [Matches -> Nat]
    /\ created \in [Matches -> Nat]
    /\ deadline \in [Matches -> Nat]
    /\ pausedFor \in [Matches -> Nat]
    /\ ledger \in Nat

Init ==
    /\ state         = [m \in Matches |-> "none"]
    /\ deposits      = [m \in Matches |-> {}]
    /\ escrow        = [m \in Matches |-> 0]
    /\ winner        = [m \in Matches |-> "none"]
    /\ winners       = [m \in Matches |-> {}]
    /\ pendingWinner = [m \in Matches |-> "none"]
    /\ paid          = [m \in Matches |-> {}]
    /\ released      = [m \in Matches |-> 0]
    /\ created       = [m \in Matches |-> 0]
    /\ deadline      = [m \in Matches |-> 0]
    /\ pausedFor     = [m \in Matches |-> 0]
    /\ ledger        = 0

--------------------------------------------------------------------------------
(* Actions *)

(*************************************************************************)
(* create_match: allocates the next id and records the creation ledger.   *)
(* Modelled as claiming any unused id — id allocation order is irrelevant *)
(* to every property here.                                               *)
(*************************************************************************)
CreateMatch(m) ==
    /\ state[m] = "none"
    /\ state' = [state EXCEPT ![m] = "pending"]
    /\ created' = [created EXCEPT ![m] = ledger]
    /\ UNCHANGED <<deposits, escrow, winner, winners, pendingWinner, paid,
                   released, deadline, pausedFor, ledger>>

(*************************************************************************)
(* deposit: a player funds their side.  The match activates exactly when  *)
(* both sides have funded.  A player cannot deposit twice (the contract   *)
(* checks the per-player deposited flag).                                *)
(*************************************************************************)
Deposit(m, p) ==
    /\ state[m] = "pending"
    /\ p \notin deposits[m]
    /\ deposits' = [deposits EXCEPT ![m] = deposits[m] \cup {p}]
    /\ escrow' = [escrow EXCEPT ![m] = escrow[m] + Stake]
    /\ state' = [state EXCEPT ![m] =
            IF deposits[m] \cup {p} = Players THEN "active" ELSE "pending"]
    /\ UNCHANGED <<winner, winners, pendingWinner, paid, released, created,
                   deadline, pausedFor, ledger>>

(*************************************************************************)
(* submit_result: the oracle reports an outcome for a fully funded active  *)
(* match.  Two shapes, selected by the configured dispute period:         *)
(*                                                                       *)
(*   DisputePeriod = 0 -> state becomes Completed immediately and the     *)
(*                        winner is recorded, but *no funds move*: the    *)
(*                        payout is claimed later (vesting).             *)
(*   DisputePeriod > 0 -> state becomes PendingResult, the outcome is     *)
(*                        parked as `pendingWinner`, and a deadline is    *)
(*                        recorded.                                      *)
(*                                                                       *)
(* Modelling both shapes in one action is what lets TLC check that the    *)
(* two payout routes cannot both fire for the same match.                *)
(*************************************************************************)
SubmitResult(m, w) ==
    /\ state[m] = "active"
    /\ deposits[m] = Players
    /\ w \in Results
    /\ IF DisputePeriod = 0
         THEN /\ state' = [state EXCEPT ![m] = "completed"]
              /\ winner' = [winner EXCEPT ![m] = w]
              /\ winners' = [winners EXCEPT ![m] = Entitled(w)]
              /\ UNCHANGED <<pendingWinner, deadline>>
         ELSE /\ state' = [state EXCEPT ![m] = "pendingResult"]
              /\ pendingWinner' = [pendingWinner EXCEPT ![m] = w]
              /\ deadline' = [deadline EXCEPT ![m] = ledger + DisputePeriod]
              /\ UNCHANGED <<winner, winners>>
    /\ UNCHANGED <<deposits, escrow, paid, released, created, pausedFor, ledger>>

(*************************************************************************)
(* claim_vested_payout: releases the pot of an already-completed match.    *)
(* Modelled as one atomic release because execute_payout pays the winner  *)
(* (or splits on a draw) in a single call.                               *)
(*************************************************************************)
ReleasePot(m) ==
    /\ state[m] = "completed"
    /\ released[m] = 0
    /\ escrow[m] > 0
    /\ released' = [released EXCEPT ![m] = 1]
    /\ escrow' = [escrow EXCEPT ![m] = 0]
    /\ paid' = [paid EXCEPT ![m] = Recipients(winner[m], deposits[m])]
    /\ UNCHANGED <<state, deposits, winner, winners, pendingWinner, created,
                   deadline, pausedFor, ledger>>

(*************************************************************************)
(* finalize_match: after the dispute window closes with no dispute, the    *)
(* parked result becomes final and the pot is released in the same call.  *)
(*************************************************************************)
FinalizeMatch(m) ==
    /\ state[m] = "pendingResult"
    /\ ledger >= deadline[m]
    /\ released[m] = 0
    /\ state' = [state EXCEPT ![m] = "completed"]
    /\ winner' = [winner EXCEPT ![m] = pendingWinner[m]]
    /\ winners' = [winners EXCEPT ![m] = Entitled(pendingWinner[m])]
    /\ released' = [released EXCEPT ![m] = 1]
    /\ escrow' = [escrow EXCEPT ![m] = 0]
    /\ paid' = [paid EXCEPT ![m] = Recipients(pendingWinner[m], deposits[m])]
    /\ UNCHANGED <<deposits, pendingWinner, created, deadline, pausedFor, ledger>>

(*************************************************************************)
(* resolve_dispute_by_vote: a dispute can *overturn* the oracle, so the    *)
(* resolved outcome is modelled as an arbitrary result rather than the    *)
(* parked one.  This over-approximates the real vote and is therefore     *)
(* sound for the safety properties below.                                *)
(*************************************************************************)
ResolveDispute(m, w) ==
    /\ state[m] = "pendingResult"
    /\ released[m] = 0
    /\ w \in Results
    /\ state' = [state EXCEPT ![m] = "completed"]
    /\ winner' = [winner EXCEPT ![m] = w]
    /\ winners' = [winners EXCEPT ![m] = Entitled(w)]
    /\ released' = [released EXCEPT ![m] = 1]
    /\ escrow' = [escrow EXCEPT ![m] = 0]
    /\ paid' = [paid EXCEPT ![m] = Recipients(w, deposits[m])]
    /\ UNCHANGED <<deposits, pendingWinner, created, deadline, pausedFor, ledger>>

(*************************************************************************)
(* cancel_match: only a *pending* match can be cancelled, and only        *)
(* depositors are refunded.  The refund counts as releasing the pot, so    *)
(* the "release at most once" property also covers refund-then-payout.    *)
(*************************************************************************)
CancelMatch(m) ==
    /\ state[m] = "pending"
    /\ state' = [state EXCEPT ![m] = "cancelled"]
    /\ escrow' = [escrow EXCEPT ![m] = 0]
    /\ paid' = [paid EXCEPT ![m] = deposits[m]]
    /\ released' = [released EXCEPT ![m] = released[m] + 1]
    /\ UNCHANGED <<deposits, winner, winners, pendingWinner, created, deadline,
                   pausedFor, ledger>>

(*************************************************************************)
(* expire_match: a pending match that has not funded within the timeout is *)
(* cancelled and depositors are refunded.  The contract subtracts          *)
(* accumulated pause time from the elapsed ledgers; that subtraction is    *)
(* reproduced here (see PendingMatchIsNeverPaused for what the model says  *)
(* about it).                                                            *)
(*************************************************************************)
ExpireMatch(m) ==
    /\ state[m] = "pending"
    /\ (ledger - created[m]) - pausedFor[m] >= Timeout
    /\ state' = [state EXCEPT ![m] = "cancelled"]
    /\ escrow' = [escrow EXCEPT ![m] = 0]
    /\ paid' = [paid EXCEPT ![m] = deposits[m]]
    /\ released' = [released EXCEPT ![m] = released[m] + 1]
    /\ UNCHANGED <<deposits, winner, winners, pendingWinner, created, deadline,
                   pausedFor, ledger>>

(*************************************************************************)
(* pause_match / resume_match: either player may pause an active match and *)
(* either may resume it.  Note there is no transition out of Paused other  *)
(* than Resume — see PausedIsOnlyLeftByResume.                            *)
(*************************************************************************)
PauseMatch(m) ==
    /\ state[m] = "active"
    /\ state' = [state EXCEPT ![m] = "paused"]
    /\ UNCHANGED <<deposits, escrow, winner, winners, pendingWinner, paid,
                   released, created, deadline, pausedFor, ledger>>

ResumeMatch(m) ==
    /\ state[m] = "paused"
    /\ state' = [state EXCEPT ![m] = "active"]
    /\ UNCHANGED <<deposits, escrow, winner, winners, pendingWinner, paid,
                   released, created, deadline, pausedFor, ledger>>

(*************************************************************************)
(* The ledger advances.  Time spent in Paused accumulates, mirroring       *)
(* total_pause_duration.                                                 *)
(*************************************************************************)
Tick ==
    /\ ledger < MaxLedger
    /\ ledger' = ledger + 1
    /\ pausedFor' = [m \in Matches |->
            IF state[m] = "paused" THEN pausedFor[m] + 1 ELSE pausedFor[m]]
    /\ UNCHANGED <<state, deposits, escrow, winner, winners, pendingWinner,
                   paid, released, created, deadline>>

Next ==
    \/ \E m \in Matches : CreateMatch(m)
    \/ \E m \in Matches, p \in Players : Deposit(m, p)
    \/ \E m \in Matches, w \in Results : SubmitResult(m, w)
    \/ \E m \in Matches : ReleasePot(m)
    \/ \E m \in Matches : FinalizeMatch(m)
    \/ \E m \in Matches, w \in Results : ResolveDispute(m, w)
    \/ \E m \in Matches : CancelMatch(m)
    \/ \E m \in Matches : ExpireMatch(m)
    \/ \E m \in Matches : PauseMatch(m)
    \/ \E m \in Matches : ResumeMatch(m)
    \/ Tick

Spec == Init /\ [][Next]_vars

--------------------------------------------------------------------------------
(* Safety invariants *)

(*************************************************************************)
(* INV-1  "A match can only complete once."                              *)
(*                                                                       *)
(* The pot of a match is released at most once, counting every route:     *)
(* the vesting claim, finalisation, dispute resolution, cancellation and  *)
(* expiry.  This is the property that a double-payout bug would break.   *)
(*************************************************************************)
AtMostOneRelease ==
    \A m \in Matches : released[m] <= 1

(*************************************************************************)
(* INV-2  "Both players cannot both be winners."                         *)
(*************************************************************************)
NotBothPlayersWin ==
    \A m \in Matches : ~(Players \subseteq winners[m])

\* Stronger form of INV-2: there is never more than one winner at all.
AtMostOneWinner ==
    \A m \in Matches : Cardinality(winners[m]) <= 1

\* The winner set is always exactly the entitlement implied by the outcome,
\* so the two representations can never disagree.
WinnersMatchOutcome ==
    \A m \in Matches : winners[m] = Entitled(winner[m])

(*************************************************************************)
(* INV-3  An outcome is only recorded on a completed match.               *)
(*************************************************************************)
NoOutcomeBeforeCompletion ==
    \A m \in Matches : state[m] # "completed" => winner[m] = "none"

(*************************************************************************)
(* INV-4  Funds are conserved: while the pot is unreleased the escrow     *)
(* holds exactly one stake per depositor, and once released it is empty.  *)
(*************************************************************************)
EscrowMatchesDeposits ==
    \A m \in Matches :
        IF released[m] = 0
          THEN escrow[m] = Stake * Cardinality(deposits[m])
          ELSE escrow[m] = 0

(*************************************************************************)
(* INV-5  Only players who funded the match can receive funds from it.    *)
(*************************************************************************)
OnlyDepositorsArePaid ==
    \A m \in Matches : paid[m] \subseteq deposits[m]

(*************************************************************************)
(* INV-6  A match cannot be playing, awaiting a result, or completed by    *)
(* result unless both stakes are in escrow.                              *)
(*************************************************************************)
PlayingImpliesFullyFunded ==
    \A m \in Matches :
        state[m] \in {"active", "paused", "pendingResult"} => deposits[m] = Players

NoPayoutWithoutFullFunding ==
    \A m \in Matches :
        (winner[m] # "none" /\ released[m] = 1) => deposits[m] = Players

(*************************************************************************)
(* INV-7  Design note, not a safety requirement.                          *)
(*                                                                       *)
(* expire_match subtracts total_pause_duration from the elapsed ledgers,   *)
(* but pausing requires the Active state and no transition ever returns a  *)
(* match to Pending.  A pending match therefore always has zero           *)
(* accumulated pause time, which makes that subtraction dead code.        *)
(* Checking it here documents the reasoning and will start failing if a    *)
(* future change introduces an Active -> Pending edge.                    *)
(*************************************************************************)
PendingMatchIsNeverPaused ==
    \A m \in Matches : state[m] = "pending" => pausedFor[m] = 0

\* Everything above, as one invariant for convenience.
Safety ==
    /\ TypeOK
    /\ AtMostOneRelease
    /\ NotBothPlayersWin
    /\ AtMostOneWinner
    /\ WinnersMatchOutcome
    /\ NoOutcomeBeforeCompletion
    /\ EscrowMatchesDeposits
    /\ OnlyDepositorsArePaid
    /\ PlayingImpliesFullyFunded
    /\ NoPayoutWithoutFullFunding
    /\ PendingMatchIsNeverPaused

--------------------------------------------------------------------------------
(* Action properties (checked as temporal formulas) *)

(*************************************************************************)
(* PROP-1  Terminal states are final: nothing re-opens a completed or      *)
(* cancelled match.  This is the temporal counterpart of INV-1 — it rules  *)
(* out "complete, re-open, complete again" rather than just counting.     *)
(*************************************************************************)
TerminalIsFinal ==
    [][ \A m \in Matches :
          state[m] \in Terminal => state'[m] = state[m] ]_vars

(*************************************************************************)
(* PROP-2  The release counter never decreases, so no action can "unpay"   *)
(* a match and thereby permit a second payout.                           *)
(*************************************************************************)
ReleaseIsMonotonic ==
    [][ \A m \in Matches : released'[m] >= released[m] ]_vars

(*************************************************************************)
(* PROP-3  Documented liveness gap.                                       *)
(*                                                                       *)
(* The only way out of Paused is back to Active: pausing an active match   *)
(* removes every timeout escape, because expire_match applies to Pending   *)
(* only.  Two cooperating players can therefore leave stakes in escrow    *)
(* indefinitely.  That is by design today (pausing is consensual), and    *)
(* this property pins the shape of the state machine so the assumption    *)
(* cannot be broken silently.                                            *)
(*************************************************************************)
PausedIsOnlyLeftByResume ==
    [][ \A m \in Matches :
          (state[m] = "paused" /\ state'[m] # "paused") => state'[m] = "active" ]_vars

(*************************************************************************)
(* PROP-4  A match's outcome, once recorded, never changes.               *)
(*************************************************************************)
OutcomeIsStable ==
    [][ \A m \in Matches :
          winner[m] # "none" => winner'[m] = winner[m] ]_vars

================================================================================
