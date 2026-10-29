# Finishing a mission stops a session that still has queued work

**Status:** fixed in the working tree, not yet committed or running in legiond
**Reported by:** Joe · 2026-10-23
**Where:** temp_plus_platform · deployment dev

## What happens

The commander gives one copy of the integrator two review-done missions to
merge as a batch. The integrator finishes the first one and hands it off.
Legion closes that mission and stops the integrator's session ("session_ended
... mission finished"), even though the second mission is still queued with it.
The second mission then sits with no one on it ("handed off", With: —) until a
person notices. Messages to the integrator fail with "isn't running".

## What should happen

The queued mission keeps moving without a person stepping in.

## Evidence

- Round 4: batch of m101 + m102. The integrator handed off only m102, at 10:08, and its session ended (#10961–#10973). m101 sat idle until 10:44, about 36 minutes.
- Round 5: batch of m103 + m104. The integrator handed off m103 at 12:54 and the mission closed (#11104–#11109). The commander's "carry on with 104" (#11107) went nowhere. m104 sat idle until 13:12, about 18 minutes.

## Already tried

Telling the integrator, in its own instructions, to finish the whole batch
before handing off. It didn't help.

## Asks

1. Don't stop a session while another open mission is waiting on that same
   position, or move the session onto that mission instead of stopping it.
2. When a mission has been "handed off" with no one on it for more than a few
   minutes, name it in the commander's "Work waiting on you" nudge.

## Related

A handoff from an operator to itself (opie-reviewer → opie-reviewer, #10987)
starts no session either, so that mission stalls the same way. Either refuse
self-handoffs or treat them as "keep going".
