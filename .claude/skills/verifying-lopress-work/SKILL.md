---
name: verifying-lopress-work
description: Use when about to report lopress work as working, passing, fixed, done, or verified — especially after driving the live editor — and before writing any verification report or PR description.
---

# Evidence-Based Verification for lopress

## Overview

The reviewer was not watching you work. Your job is not to *conclude* "it works" — it is to **produce the evidence that lets someone else conclude it**. Report what you observed, pasted verbatim, never what you expect.

## The Iron Rule

> No "works" / "PASS" / "fixed" / "done" without the exact command and its **actual output** quoted next to the claim.

If you cannot show the output, say **UNVERIFIED** and what's needed to verify. An honest UNVERIFIED is reviewable; a confident false PASS ships bugs.

## Real failures these rules come from

| Failure | Rule |
|---|---|
| Cited `screenshot_8a.png` as proof — the file didn't exist | Only cite artifacts you actually saved AND re-opened. If it's gone, it's not evidence. |
| Reported "ChangeType → ✅" because `/action` returned `dispatched` — the block didn't change | `dispatched`/`200`/`ok` means *accepted*, not *worked*. Re-fetch `/state` (or re-screenshot) and quote the end-state. |
| Noticed a data-loss anomaly mid-investigation, buried it under a green PASS | Anything surprising is a **headline finding** with before/after evidence, never a footnote. |
| Said "can't make `/action` work" with no status code | Quote the exact status + body — `Invoke-RestMethod` throws and hides them; use `Invoke-WebRequest` in try/catch. The specific code (504 vs 409 vs 422) IS the diagnosis. |
| Reported "all tests pass" as proof a GUI behavior works | `scripts/check.sh` green proves compile + unit logic. Live GUI behavior needs `/state` + `/screenshot` from a running editor. State which kind of evidence you have. |

## What counts as evidence

- **Document state**: verbatim `GET /state` JSON, before *and* after.
- **Visuals**: a `/screenshot` PNG saved to a real path that you then **opened with your file-reading tool and describe**. Unopened screenshot = unverified visual.
- **An action's effect**: the response line AND the `/state` diff. Block ids change after structural actions — always re-fetch.
- **A failure**: exact status code + body.
- **"No longer reproduces"**: the original repro steps, run now, with output showing the good end-state.
- **Gate**: the tail of `bash scripts/check.sh` output (and note the clippy cache false-pass: after `cargo test/run/build`, `touch` changed `.rs` files before trusting a green clippy).

## Don't mutate what you're inspecting

`/action` and the editor's autosave **persist to disk**. Drive edits only against a scratch site (`cargo run --quiet -- new <scratch-dir>` under `$env:TEMP` — see the `driving-lopress-editor` skill), never a real site or committed fixture. If you must exercise a real fixture's content, copy it into the scratch site first (`Copy-Item <fixture> "$site\src\posts\test.md"`). If you touched a real file anyway, say so in the report and restore it (`git checkout -- <file>`).

Save debug artifacts (screenshots, `/state` dumps, command logs) under `.pi/results/<YYYY-MM-DD>-<short-task>/` — it's gitignored, so it never dirties the repo. Never drop PNGs at the repo root, and reference each artifact by its real path there.

## Self-check before reporting

- [ ] Every works/PASS/fixed claim has command + actual output beside it.
- [ ] Every cited artifact exists at the stated path and I opened it.
- [ ] I read the end-state after each action — never reported "dispatched" as "worked".
- [ ] Surprises are surfaced as findings, not buried.
- [ ] Failures quote real status codes + bodies.
- [ ] "Compiles/tests pass" is stated separately from "live behavior verified".
- [ ] No real file was left mutated by my verification.
