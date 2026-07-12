# nagent standard library

Lightweight multi-agent orchestration structure: named agents with message logs, local key/value memory, handoffs, and a simple round-robin `run`. **No LLM calls** — scaffolding only (steps echo `acked: …` placeholders).

## Import

```niao
import "nagent"
```

Paths `import "std/nagent"` and `import "nagent"` are equivalent. Flat builtins (`nagent_new`, `nagent_step`, …) are also available globally after import.

## Quick start

```niao
import "nagent"

let researcher = nagent.new("alice", "researcher")
let writer = nagent.new("bob", "writer")

nagent.say(researcher, "system", "stay factual")
print(nagent.step(researcher, "summarize cats"))  // acked: summarize cats

nagent.remember(researcher, "topic", "cats")
print(nagent.recall(researcher, "topic"))          // cats

nagent.handoff(researcher, writer, "draft the intro")
print(nagent.run([researcher, writer], "kickoff", 2))
```

## Creating agents

| Method | Description |
|--------|-------------|
| `nagent.new(name, role?)` | Create an agent handle. Optional `role` string (default `""`). |
| `nagent.close(handle)` | Free the agent; returns `true` if the handle existed. |

## Messaging

| Method | Description |
|--------|-------------|
| `nagent.say(h, role_or_name, content)` | Append `{role, content}` to the agent's log. |
| `nagent.step(h, input)` | Append a `user` message, append an `assistant` placeholder `acked: <input>`, return that string. |
| `nagent.messages(h)` | Array of `{role, content}` objects. |
| `nagent.clear_messages(h)` | Clear the message log (memory and identity survive). |

## Memory

| Method | Description |
|--------|-------------|
| `nagent.remember(h, key, val)` | Store any Niao value in agent-local KV. |
| `nagent.recall(h, key)` | Value or `nil` on miss. |

## Coordination

| Method | Description |
|--------|-------------|
| `nagent.handoff(from_h, to_h, msg)` | Append a `handoff` note to both logs (`handoff→to: …` / `handoff←from: …`). |
| `nagent.run(handles, kickoff, max_steps?)` | Round-robin: each agent `step`s with the previous output. Default `max_steps` is `handles.len()`. Returns the final string. |
| `nagent.name(h)` / `nagent.role(h)` | Identity getters. |

## Errors

| Code | Meaning |
|------|---------|
| 2990 | Wrong argument count. |
| 2991 | Operation failed (empty name, empty handle list, bad `max_steps`). |
| 2992 | Wrong argument type. |
| 2993 | Invalid or closed agent handle (catchable `error`). |
