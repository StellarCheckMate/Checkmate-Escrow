# Tutorial Assets — Screenshots & Recordings

This folder holds the visual walkthroughs referenced by the
[interactive tutorial](../../tutorial-step-by-step.md). It is intentionally a
contribution-ready placeholder: the tutorial works fully as text today, and the
images/recordings below are the next layer of polish for new contributors to add.

> 🧑‍🤝‍🧑 **Good first contribution.** Capturing these is a quick, high-impact way
> to make the tutorial friendlier — no Rust required.

## Screenshots to add

Drop PNGs here with these exact names so the tutorial picks them up:

| Filename | What it should show |
|----------|---------------------|
| `00-setup-deployed.png` | Terminal printing the two contract IDs after deploy |
| `01-create-match.png` | `get_match` output with `state: Pending` |
| `02-deposit-funded.png` | Funded check returning `true` and balance `200000000` |
| `03-result-completed.png` | `get_match` showing `state: Completed`, balance `0` |

## Screen recordings to add

Record with [OBS Studio](https://obsproject.com/) (free, cross-platform) or
[Loom](https://www.loom.com/) (quick browser capture):

| Recording | Target length | Covers |
|-----------|---------------|--------|
| Full run | ~12 min | Setup → Step 3 payout, end to end |
| Step 1 | ~2 min | Create a match |
| Step 2 | ~2 min | Deposit funds |
| Step 3 | ~3 min | Check result & payout |

Host the video (Loom/YouTube), then add the link to the **Video walkthroughs**
table in [`tutorial-step-by-step.md`](../../tutorial-step-by-step.md) and to the
README tutorial section.

## Capture checklist (keep recordings consistent)

- [ ] Use **testnet** only — never show mainnet keys or real funds
- [ ] **Redact secret keys** (`S...` seeds) — only show public addresses (`G...`)
      and contract IDs (`C...`)
- [ ] Keep the terminal window a consistent size (~120 columns)
- [ ] Use a legible font size (recordings are watched on small screens too)
- [ ] Add a short caption/title for each step
- [ ] Keep each per-step clip focused on a single step
- [ ] Trim dead air so the full run stays **under 15 minutes**

## Why placeholders instead of committed binaries?

Screenshots and video drift out of date as the CLI output evolves, and binary
blobs bloat the git history. Keeping this as a documented, named placeholder lets
contributors add current visuals on demand while the **text tutorial remains the
always-accurate source of truth**.
