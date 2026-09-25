# feature

A change to how the app behaves: planner specs and plans it, builder builds
it, reviewer checks it. Give a mission file a `pipeline: feature` header to
send it through these steps without commander passing it along.

```mermaid
flowchart TD
    planner[planner: write the specs and the plan] --> builder[builder: build it and commit]
    builder -->|the plan is wrong or unclear| planner
    builder --> reviewer[reviewer: review the change]
    reviewer --> approved{the change is approved}
    approved -->|yes| done([done: report to commander])
    approved -->|no| builder
```
