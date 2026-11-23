# Prompt for this goal

```text
You are sending our framework fixes and features from our NSH fork to upstream NSH, one small pull
request at a time.

Repository: https://github.com/hexuria/nsh, branch feat/nsh-upstream-prs. Upstream is
https://github.com/longbridge/nsh, branch main.
Read handover/upstream-prs/GOAL.md and handover/upstream-prs/PLAN.md completely, then everything
GOAL.md lists under "Read first" (upstream's CONTRIBUTING.md in full, and our merged and closed
pull requests there), before you change anything. The "Decisions made to fill gaps" in PLAN.md are
final; do not ask about them again.

Work the themes in PLAN.md's order. For each: check whether upstream main already has it, rebuild
the change on upstream main in a branch up/<area>-<short-name>, test it on the platform it touches,
and open the pull request with upstream's template filled in. A theme marked new API gets an issue
first and no pull request until a maintainer agrees. Keep at most five of our pull requests open at
once. Never mention gpui-agent upstream. Follow GOAL.md's rules and its Allowed / Not allowed lists
exactly.

After every theme, update handover/upstream-prs/LEDGER.md on feat/nsh-upstream-prs and push it.

This session is on <MACHINE: Linux | macOS | Windows>. Take only themes whose platform this machine
can build and run; leave the others as "not started" in the ledger for the session on the right
machine.

Finish with the report GOAL.md describes.
```

Run one session per machine at the same time if you want the Windows, macOS and Linux themes to move together. They
share the ledger, so each session pulls before it edits a row and pushes straight after.
