# Prompts for this goal

## To start (the lead session)

```text
You are leading the move of our gpui-agent work out of our NSH fork.

Repository: https://github.com/hexuria/nsh, branch feat/nsh-agent-socket.
Read handover/agent-socket/GOAL.md and handover/agent-socket/PLAN.md completely, then everything
GOAL.md lists under "Read first", before you change anything. The picks and the "Decisions made to
fill gaps" in PLAN.md are final; do not ask about them again.

Do Stage 0 of PLAN.md yourself in this session: start branch gpui-agent on hexuria/nsh from upstream
tag v0.6.1, set up https://github.com/hexuria/nsh-gpui-agent (it exists and holds only a README),
move the adapter crate into it, design the doors, write SOCKET.md on branch gpui-agent, move the
agent actions out of nsh-testing, and keep the README true about which NSH to use. Follow GOAL.md's
rules and its Allowed / Not allowed lists exactly.

When Stage 0 passes its done list, stop and report: the repository and branch links, SOCKET.md as it
stands, what you ran and its output, and anything the six platform sessions need to know that
PLAN.md does not already say. If Stage 0 shows the socket cannot be small, stop there and report
that instead, as GOAL.md says.

Later stages (2, 3 and 4) come back to you once the platform sessions have reported.
```

## One per platform (after Stage 0 has reported)

Start one session on the machine the platform needs, and replace `<PLATFORM>` with one of: macOS, Linux, Windows,
Android, iOS, web.

```text
You are doing one platform of the move of our gpui-agent work out of our NSH fork: <PLATFORM>.

Repository: https://github.com/hexuria/nsh, branch feat/nsh-agent-socket.
Read handover/agent-socket/GOAL.md and handover/agent-socket/PLAN.md completely, then SOCKET.md on
branch gpui-agent and the repository https://github.com/hexuria/nsh-gpui-agent, which Stage 0 filled.

Do the <PLATFORM> row of PLAN.md's Stage 1 table: move that platform's agent code into
hexuria/nsh-gpui-agent, add the doors it needs to branch gpui-agent and to SOCKET.md, and prove it on
this machine against that row's done list. Follow GOAL.md's rules and its Allowed / Not allowed lists
exactly. Other platform sessions are running at the same time: rebase on gpui-agent often, and put a
door that more than one platform needs in the shared crates, not in yours.

Finish with a report for this platform: what you moved, each door you added and its size, every
command you ran with its output, and anything that had to stay in the fork and why.
```
